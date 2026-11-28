//! Troca automática de perfil conforme o programa aberto.
//!
//! Uma lista de regras liga um programa a um perfil do teclado. Um vigia (`mag82 vigiar`, posto
//! para iniciar com a sessão) olha de tempos em tempos quais programas do usuário estão rodando:
//! quando um da lista abre, o teclado vai para o perfil dele; quando fecha, volta ao perfil de
//! antes. O que conta é o programa estar aberto, não a janela estar em foco: isso funciona igual
//! no X11 e no Wayland, onde não há forma geral de saber qual janela está ativa.
//!
//! O vigia só fala com o teclado no instante de uma troca, e só existe enquanto o recurso está
//! ligado: desligado, nada do programa roda em segundo plano.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{Erro, Resultado, Teclado};

/// De quanto em quanto tempo o vigia confere os programas abertos.
pub const INTERVALO: Duration = Duration::from_secs(2);

/// Um programa e o perfil (a partir de 0) que o teclado usa enquanto ele está aberto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regra {
    /// Nome do programa como aparece em [`programas_abertos`], em minúsculas.
    pub programa: String,
    pub perfil: u8,
}

/// O que o usuário configurou: se o recurso está ligado e as regras, em ordem de prioridade.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ajustes {
    pub ligado: bool,
    pub regras: Vec<Regra>,
}

fn pasta_de_ajustes() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
}

fn erro_de_arquivo(o_que: &str, caminho: &Path, e: std::io::Error) -> Erro {
    Erro::Invalido(format!("não foi possível {o_que} {}: {e}", caminho.display()))
}

impl Ajustes {
    /// `$XDG_CONFIG_HOME/mag82/perfil-automatico`, ou `~/.config/mag82/perfil-automatico`.
    pub fn caminho() -> PathBuf {
        pasta_de_ajustes().join("mag82/perfil-automatico")
    }

    /// Os ajustes guardados; sem arquivo, o recurso está desligado e sem regras.
    pub fn ler() -> Self {
        fs::read_to_string(Self::caminho()).map(|t| Self::de_texto(&t)).unwrap_or_default()
    }

    pub fn salvar(&self) -> Resultado<()> {
        let caminho = Self::caminho();
        if let Some(pasta) = caminho.parent() {
            fs::create_dir_all(pasta).map_err(|e| erro_de_arquivo("criar", pasta, e))?;
        }
        fs::write(&caminho, self.texto()).map_err(|e| erro_de_arquivo("salvar", &caminho, e))
    }

    pub fn de_texto(texto: &str) -> Self {
        let mut ajustes = Self::default();
        for linha in texto.lines().map(str::trim) {
            if let Some(valor) = linha.strip_prefix("ligado ") {
                ajustes.ligado = valor.trim() == "sim";
            } else if let Some(resto) = linha.strip_prefix("regra ") {
                // o nome do programa vai até o fim da linha: pode ter espaços
                let Some((perfil, programa)) = resto.trim().split_once(' ') else { continue };
                let (Ok(perfil), programa) = (perfil.parse::<u8>(), programa.trim().to_lowercase()) else { continue };
                if perfil >= 1 && !programa.is_empty() {
                    ajustes.definir_regra(&programa, perfil - 1);
                }
            }
        }
        ajustes
    }

    pub fn texto(&self) -> String {
        let mut s = String::from("# Troca automática de perfil do MAG82: regra PERFIL PROGRAMA, em ordem de prioridade\n");
        s.push_str(if self.ligado { "ligado sim\n" } else { "ligado nao\n" });
        for r in &self.regras {
            s.push_str(&format!("regra {} {}\n", r.perfil + 1, r.programa));
        }
        s
    }

    /// Põe ou troca a regra do programa. Uma regra nova entra no fim da lista.
    pub fn definir_regra(&mut self, programa: &str, perfil: u8) {
        let programa = programa.trim().to_lowercase();
        match self.regras.iter_mut().find(|r| r.programa == programa) {
            Some(regra) => regra.perfil = perfil,
            None => self.regras.push(Regra { programa, perfil }),
        }
    }

    /// Tira a regra do programa; devolve falso se não havia.
    pub fn remover_regra(&mut self, programa: &str) -> bool {
        let programa = programa.trim().to_lowercase();
        let antes = self.regras.len();
        self.regras.retain(|r| r.programa != programa);
        self.regras.len() != antes
    }
}

/// O fim de um caminho, de Linux ou de Windows (jogos pelo Wine ou Proton), em minúsculas.
fn nome_do_caminho(caminho: &str) -> Option<String> {
    let nome = caminho.rsplit(['/', '\\']).next()?.trim().to_lowercase();
    (!nome.is_empty()).then_some(nome)
}

/// Os nomes pelos quais um processo é conhecido: o que ele se dá e o do executável.
fn nomes_do_processo(pasta: &Path) -> Vec<String> {
    let mut nomes = Vec::new();
    // o primeiro argumento da linha de comando: é o que distingue um jogo do Windows do "wine" que o roda
    if let Ok(bruto) = fs::read(pasta.join("cmdline")) {
        let primeiro = bruto.split(|b| *b == 0).next().unwrap_or_default();
        nomes.extend(nome_do_caminho(&String::from_utf8_lossy(primeiro)));
    }
    if let Ok(destino) = fs::read_link(pasta.join("exe")) {
        nomes.extend(nome_do_caminho(&destino.to_string_lossy().replace(" (deleted)", "")));
    }
    nomes.dedup();
    nomes
}

fn processos_do_usuario() -> Vec<PathBuf> {
    // SAFETY: getuid não tem pré-condições e não falha.
    let eu = unsafe { libc::getuid() };
    let Ok(entradas) = fs::read_dir("/proc") else { return Vec::new() };
    entradas
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter(|e| e.metadata().is_ok_and(|m| m.uid() == eu))
        .map(|e| e.path())
        .collect()
}

/// Os programas do usuário que estão rodando agora, por todos os nomes de cada um.
pub fn programas_abertos() -> BTreeSet<String> {
    processos_do_usuario().iter().flat_map(|p| nomes_do_processo(p)).collect()
}

/// Verdadeiro para os serviços da sessão (systemd, portais, agentes do ambiente gráfico), que
/// ninguém escolheria numa regra: só atrapalham a lista.
fn servico_do_sistema(pasta: &Path, nome: &str) -> bool {
    let exe = fs::read_link(pasta.join("exe")).map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    nome.starts_with('(')
        || nome.starts_with('-')
        || ["/usr/libexec/", "/usr/lib/systemd/", "/lib/systemd/", "/usr/lib/x86_64-linux-gnu/", "/usr/lib64/"].iter().any(|p| exe.starts_with(p))
}

/// Os programas abertos, um nome por programa e sem os serviços do sistema, para o usuário
/// escolher numa lista. (Uma regra escrita à mão pode usar qualquer nome de [`programas_abertos`].)
pub fn programas_para_escolher() -> Vec<String> {
    let nomes: BTreeSet<String> = processos_do_usuario()
        .iter()
        .filter_map(|p| nomes_do_processo(p).into_iter().next().filter(|nome| !servico_do_sistema(p, nome)))
        .collect();
    nomes.into_iter().collect()
}

/// A regra que vale agora: a primeira da lista cujo programa está aberto.
pub fn regra_em_vigor<'a>(regras: &'a [Regra], abertos: &BTreeSet<String>) -> Option<&'a Regra> {
    regras.iter().find(|r| abertos.contains(&r.programa))
}

/// O que o vigia lembra de uma conferência para a outra.
#[derive(Debug, Default)]
pub struct Vigia {
    /// A regra que pôs o teclado no perfil em que ele está.
    em_vigor: Option<Regra>,
    /// O perfil que estava ativo antes de a primeira regra entrar em vigor.
    anterior: Option<u8>,
}

impl Vigia {
    /// Verdadeiro se a regra em vigor mudou desde a última decisão: só aí o teclado é consultado.
    pub fn mudou(&self, regras: &[Regra], abertos: &BTreeSet<String>) -> bool {
        regra_em_vigor(regras, abertos) != self.em_vigor.as_ref()
    }

    /// Decide o que fazer com a mudança; `atual` é o perfil ativo no teclado. Devolve o perfil a
    /// pôr, se for o caso.
    ///
    /// Quando o último programa da lista fecha, o teclado volta ao perfil de antes, mas só se
    /// ainda estiver no perfil que a regra pôs: se o usuário trocou à mão no meio, fica como ele
    /// deixou.
    pub fn decidir(&mut self, regras: &[Regra], abertos: &BTreeSet<String>, atual: u8) -> Option<u8> {
        let nova = regra_em_vigor(regras, abertos).cloned();
        let antiga = std::mem::replace(&mut self.em_vigor, nova.clone());
        match (antiga, nova) {
            (None, Some(nova)) => {
                self.anterior = Some(atual);
                (atual != nova.perfil).then_some(nova.perfil)
            }
            (Some(_), Some(nova)) => (atual != nova.perfil).then_some(nova.perfil),
            (Some(antiga), None) => self.anterior.take().filter(|volta| atual == antiga.perfil && *volta != atual),
            (None, None) => None,
        }
    }

    /// Faz uma conferência: olha os programas abertos e, se a regra em vigor mudou, troca o
    /// perfil. Devolve uma frase quando algo aconteceu. Sem teclado, não decide nada e tenta de
    /// novo na próxima.
    pub fn conferir(&mut self, regras: &[Regra]) -> Option<String> {
        let abertos = programas_abertos();
        if !self.mudou(regras, &abertos) {
            return None;
        }
        let mut teclado = Teclado::abrir().ok()?;
        let (total, atual) = teclado.perfis().ok()?;
        let motivo = match regra_em_vigor(regras, &abertos) {
            Some(regra) => format!("{} abriu", regra.programa),
            None => "os programas da lista fecharam".to_string(),
        };
        let alvo = self.decidir(regras, &abertos, atual)?;
        if alvo >= total {
            return Some(format!("{motivo}, mas o teclado não tem o perfil {}", alvo + 1));
        }
        Some(match teclado.definir_perfil(alvo) {
            Ok(()) => format!("{motivo}: perfil {}", alvo + 1),
            Err(erro) => format!("{motivo}, mas a troca de perfil falhou: {erro}"),
        })
    }

    /// Ao desligar o recurso com uma regra em vigor, devolve o teclado ao perfil de antes.
    fn encerrar(&mut self) -> Option<String> {
        let regra = self.em_vigor.take()?;
        let volta = self.anterior.take()?;
        let mut teclado = Teclado::abrir().ok()?;
        let (_, atual) = teclado.perfis().ok()?;
        if atual != regra.perfil || volta == atual {
            return None;
        }
        teclado.definir_perfil(volta).ok().map(|()| format!("troca automática desligada: perfil {}", volta + 1))
    }
}

/// Garante um vigia só por usuário: a trava vale enquanto o arquivo devolvido existir.
fn trava_do_vigia() -> Option<fs::File> {
    use std::os::fd::AsRawFd;
    let pasta = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(pasta_de_ajustes);
    let arquivo = fs::OpenOptions::new().create(true).truncate(false).write(true).open(pasta.join("mag82-perfil-automatico.trava")).ok()?;
    // SAFETY: o descritor é válido e pertence a `arquivo`, que segue aberto com quem chamou.
    (unsafe { libc::flock(arquivo.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0).then_some(arquivo)
}

/// O vigia: confere os programas abertos de tempos em tempos e troca o perfil, até o recurso ser
/// desligado. `avisar` recebe uma frase a cada coisa que acontece. Com `uma_vez`, faz uma
/// conferência só e sai (para testes e para ver o que ele faria).
pub fn vigiar(uma_vez: bool, mut avisar: impl FnMut(&str)) -> Resultado<()> {
    let Some(_trava) = trava_do_vigia() else {
        avisar("já há um vigia rodando para este usuário");
        return Ok(());
    };
    let mut vigia = Vigia::default();
    let mut lido_em = None;
    let mut ajustes = Ajustes::default();
    loop {
        // os ajustes são relidos quando o arquivo muda: a interface não precisa avisar o vigia
        let mudou_em = fs::metadata(Ajustes::caminho()).and_then(|m| m.modified()).ok();
        if mudou_em != lido_em || lido_em.is_none() {
            ajustes = Ajustes::ler();
            lido_em = mudou_em;
        }
        if !ajustes.ligado {
            if let Some(frase) = vigia.encerrar() {
                avisar(&frase);
            }
            avisar("a troca automática de perfil está desligada");
            return Ok(());
        }
        if let Some(frase) = vigia.conferir(&ajustes.regras) {
            avisar(&frase);
        }
        if uma_vez {
            return Ok(());
        }
        std::thread::sleep(INTERVALO);
    }
}

// ---------- iniciar com a sessão ----------

/// O arquivo que faz o vigia iniciar com a sessão do usuário (padrão XDG de início automático).
pub fn caminho_do_inicio() -> PathBuf {
    pasta_de_ajustes().join("autostart/mag82-perfil-automatico.desktop")
}

/// Liga ou desliga o recurso: guarda o ajuste e cria ou apaga o arquivo de início automático.
/// `mag82` é o caminho da linha de comando, que o arquivo vai chamar.
pub fn ligar(ligado: bool, mag82: &Path) -> Resultado<()> {
    let mut ajustes = Ajustes::ler();
    ajustes.ligado = ligado;
    ajustes.salvar()?;
    let inicio = caminho_do_inicio();
    if !ligado {
        return match fs::remove_file(&inicio) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(erro_de_arquivo("apagar", &inicio, e)),
            _ => Ok(()),
        };
    }
    if let Some(pasta) = inicio.parent() {
        fs::create_dir_all(pasta).map_err(|e| erro_de_arquivo("criar", pasta, e))?;
    }
    let texto = format!(
        "[Desktop Entry]\nType=Application\nName=MAG82: troca automática de perfil\n\
         Comment=Troca o perfil do teclado conforme o programa aberto\nExec=\"{}\" vigiar\nNoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        mag82.display()
    );
    fs::write(&inicio, texto).map_err(|e| erro_de_arquivo("salvar", &inicio, e))
}

/// Põe o vigia para rodar agora, solto deste programa. Se já houver um, o novo sai sozinho.
pub fn iniciar_vigia(mag82: &Path) -> Resultado<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    Command::new(mag82)
        .arg("vigiar")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(|_| ())
        .map_err(|e| Erro::Invalido(format!("não foi possível iniciar o vigia ({}): {e}", mag82.display())))
}

/// Onde está a linha de comando `mag82`: ao lado deste executável, ou no caminho do sistema.
pub fn linha_de_comando() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|pasta| pasta.join("mag82")))
        .filter(|vizinho| vizinho.is_file())
        .unwrap_or_else(|| PathBuf::from("mag82"))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn regras() -> Vec<Regra> {
        vec![Regra { programa: "cs2".into(), perfil: 1 }, Regra { programa: "blender".into(), perfil: 2 }]
    }

    fn abertos(nomes: &[&str]) -> BTreeSet<String> {
        nomes.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn ajustes_vao_e_voltam() {
        let mut a = Ajustes { ligado: true, regras: regras() };
        a.definir_regra("Cities Skylines.exe", 3);
        assert_eq!(Ajustes::de_texto(&a.texto()), a);
        assert_eq!(a.regras[2].programa, "cities skylines.exe");
        a.definir_regra("CS2", 0); // troca o perfil, não duplica nem muda de lugar
        assert_eq!((a.regras.len(), a.regras[0].perfil), (3, 0));
        assert!(a.remover_regra("blender") && !a.remover_regra("blender"));
        assert_eq!(Ajustes::de_texto("qualquer coisa\nregra 0 x\nregra 9\nregra dois y"), Ajustes::default());
    }

    #[test]
    fn nomes_de_linux_e_de_windows() {
        assert_eq!(nome_do_caminho("/usr/bin/Blender"), Some("blender".into()));
        assert_eq!(nome_do_caminho(r"Z:\jogos\Counter Strike\cs2.exe"), Some("cs2.exe".into()));
        assert_eq!(nome_do_caminho(""), None);
        // este teste é um programa do usuário rodando: ele mesmo aparece na lista
        assert!(!programas_abertos().is_empty() && !programas_para_escolher().is_empty());
    }

    #[test]
    fn abre_troca_fecha_volta() {
        let (r, mut v) = (regras(), Vigia::default());
        assert!(!v.mudou(&r, &abertos(&["firefox"])));
        // o jogo abre com o teclado no perfil 1 (índice 0): vai para o do jogo
        let com_jogo = abertos(&["firefox", "cs2"]);
        assert!(v.mudou(&r, &com_jogo));
        assert_eq!(v.decidir(&r, &com_jogo, 0), Some(1));
        assert!(!v.mudou(&r, &com_jogo));
        // abre o segundo da lista: o primeiro tem prioridade, nada muda
        assert!(!v.mudou(&r, &abertos(&["cs2", "blender"])));
        // o jogo fecha com o outro ainda aberto: passa para o perfil do outro
        let so_blender = abertos(&["blender"]);
        assert_eq!(v.decidir(&r, &so_blender, 1), Some(2));
        // fecha tudo: volta ao perfil de antes de a primeira regra entrar
        assert_eq!(v.decidir(&r, &abertos(&[]), 2), Some(0));
        assert!(!v.mudou(&r, &abertos(&[])));
    }

    #[test]
    fn respeita_a_troca_feita_a_mao() {
        let (r, mut v) = (regras(), Vigia::default());
        assert_eq!(v.decidir(&r, &abertos(&["cs2"]), 0), Some(1));
        // com o jogo aberto, o usuário foi à mão para o perfil 4: ao fechar, fica onde ele deixou
        assert_eq!(v.decidir(&r, &abertos(&[]), 3), None);
        // o teclado já estava no perfil do programa: não há o que trocar, nem para onde voltar
        assert_eq!(v.decidir(&r, &abertos(&["cs2"]), 1), None);
        assert_eq!(v.decidir(&r, &abertos(&[]), 1), None);
    }
}
