//! Registro em arquivo do que o programa fez no teclado, para análise posterior, e o
//! acompanhamento de uma calibração (que alimenta o registro e os resumos de GUI e CLI).

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::Pos;

/// Arquivo de registro: uma linha por evento, com data e hora, sempre acrescentando ao fim.
pub struct Registro {
    arquivo: File,
    caminho: PathBuf,
}

impl Registro {
    /// `$XDG_STATE_HOME/mag82/<nome>`, ou `~/.local/state/mag82/<nome>`.
    pub fn caminho_padrao(nome: &str) -> PathBuf {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
            .unwrap_or_else(std::env::temp_dir);
        base.join("mag82").join(nome)
    }

    pub fn abrir(caminho: &Path) -> std::io::Result<Self> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta)?;
        }
        let arquivo = OpenOptions::new().create(true).append(true).open(caminho)?;
        Ok(Self { arquivo, caminho: caminho.to_path_buf() })
    }

    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    /// Acrescenta uma linha. Falha de escrita é ignorada: o registro nunca atrapalha a operação.
    pub fn linha(&mut self, texto: &str) {
        let _ = writeln!(self.arquivo, "{} {texto}", agora());
    }
}

/// Arquivo que existe enquanto o teclado está em modo de calibração por causa deste programa.
/// Se o programa morrer no meio (queda de energia, processo encerrado à força), o arquivo
/// sobra, e é assim que a próxima execução sabe que precisa tirar o teclado desse modo.
fn marcador_de_calibracao() -> PathBuf {
    Registro::caminho_padrao("calibracao-em-andamento")
}

/// Verdadeiro se uma calibração foi iniciada e não chegou a ser encerrada.
pub fn calibracao_pendente() -> bool {
    marcador_de_calibracao().exists()
}

pub(crate) fn marcar_calibracao(em_andamento: bool) {
    let marcador = marcador_de_calibracao();
    if em_andamento {
        if let Some(pasta) = marcador.parent() {
            let _ = std::fs::create_dir_all(pasta);
        }
        let _ = std::fs::write(&marcador, agora());
    } else {
        let _ = std::fs::remove_file(&marcador);
    }
}

/// Acrescenta uma linha avulsa ao registro de calibração (fora de uma sessão).
pub fn anotar_calibracao(texto: &str) {
    if let Ok(mut r) = Registro::abrir(&Registro::caminho_padrao("calibracao.log")) {
        r.linha(texto);
    }
}

/// Data e hora locais, `AAAA-MM-DD hh:mm:ss`.
fn agora() -> String {
    let mut partes: libc::tm = unsafe { std::mem::zeroed() };
    let mut texto = [0u8; 32];
    // SAFETY: `partes` e `texto` são válidos e do tamanho informado; o formato termina em NUL.
    let n = unsafe {
        let segundos = libc::time(std::ptr::null_mut());
        libc::localtime_r(&segundos, &mut partes);
        libc::strftime(texto.as_mut_ptr().cast(), texto.len(), c"%Y-%m-%d %H:%M:%S".as_ptr(), &partes)
    };
    String::from_utf8_lossy(&texto[..n]).into_owned()
}

/// Resultado de uma calibração, para mostrar ao usuário.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumoCalibracao {
    pub total: usize,
    /// Teclas calibradas nesta sessão.
    pub calibradas: usize,
    /// Nomes das teclas que não foram pressionadas.
    pub faltaram: Vec<String>,
    pub segundos: f64,
    /// Teclas que o teclado marca como calibradas depois de sair do modo de calibração
    /// (`None` se essa leitura não foi feita ou falhou).
    pub conferidas: Option<usize>,
    /// Onde o registro foi gravado, se foi.
    pub registro: Option<PathBuf>,
}

impl ResumoCalibracao {
    pub fn texto(&self) -> String {
        let mut t = format!("{} de {} teclas calibradas em {:.0} s.", self.calibradas, self.total, self.segundos);
        if !self.faltaram.is_empty() {
            t.push_str(&format!(" Não foram pressionadas: {}.", self.faltaram.join(" ")));
        }
        match self.conferidas {
            Some(n) if n == self.total => t.push_str(" Conferido no teclado depois de encerrar: todas marcadas."),
            Some(n) => t.push_str(&format!(" Conferido no teclado depois de encerrar: {n} marcadas.")),
            None => {}
        }
        t
    }
}

/// Acompanha uma calibração em andamento e registra cada passo.
pub struct SessaoCalibracao {
    registro: Option<Registro>,
    inicio: Instant,
    nomes: BTreeMap<Pos, String>,
    estado: BTreeMap<Pos, bool>,
    /// Posições na ordem em que foram calibradas, com o instante (segundos desde o início).
    ordem: Vec<(Pos, f64)>,
}

impl SessaoCalibracao {
    /// `inicial` é a primeira leitura das marcas, feita logo depois de iniciar a calibração.
    /// O registro vai para `caminho`; se não der para abrir, a sessão segue sem registro.
    pub fn nova(caminho: &Path, origem: &str, firmware: &str, nomes: BTreeMap<Pos, String>, inicial: &[(Pos, bool)]) -> Self {
        let mut registro = Registro::abrir(caminho).ok();
        let estado: BTreeMap<Pos, bool> = inicial.iter().copied().collect();
        let ja_marcadas = estado.values().filter(|f| **f).count();
        if let Some(r) = registro.as_mut() {
            r.linha(&format!(
                "calibração iniciada pela {origem} (firmware {firmware}, {} teclas, {ja_marcadas} já marcadas ao iniciar)",
                estado.len()
            ));
        }
        Self { registro, inicio: Instant::now(), nomes, estado, ordem: Vec::new() }
    }

    fn nome(&self, pos: Pos) -> String {
        self.nomes.get(&pos).cloned().unwrap_or_else(|| format!("{},{}", pos.0, pos.1))
    }

    /// Verdadeiro se o teclado não zerou as marcas ao iniciar: não dá para acompanhar o progresso.
    pub fn sem_progresso(&self) -> bool {
        self.ordem.is_empty() && !self.estado.is_empty() && self.estado.values().all(|f| *f)
    }

    pub fn estado(&self) -> &BTreeMap<Pos, bool> {
        &self.estado
    }

    /// (calibradas, total)
    pub fn contagem(&self) -> (usize, usize) {
        (self.estado.values().filter(|f| **f).count(), self.estado.len())
    }

    /// Recebe uma nova leitura das marcas e registra as teclas que acabaram de ser calibradas.
    pub fn atualizar(&mut self, lido: &[(Pos, bool)]) {
        let passou = self.inicio.elapsed().as_secs_f64();
        for (pos, feita) in lido {
            let antes = self.estado.insert(*pos, *feita).unwrap_or(false);
            if *feita && !antes {
                self.ordem.push((*pos, passou));
                let (feitas, total) = self.contagem();
                let nome = self.nome(*pos);
                if let Some(r) = self.registro.as_mut() {
                    r.linha(&format!("+{passou:.1}s {nome} ({feitas}/{total})"));
                }
            } else if !*feita && antes {
                let nome = self.nome(*pos);
                if let Some(r) = self.registro.as_mut() {
                    r.linha(&format!("+{passou:.1}s {nome} perdeu a marca de calibrada"));
                }
            }
        }
    }

    pub fn erro(&mut self, texto: &str) {
        let passou = self.inicio.elapsed().as_secs_f64();
        if let Some(r) = self.registro.as_mut() {
            r.linha(&format!("+{passou:.1}s erro: {texto}"));
        }
    }

    /// Fecha a sessão. `depois` é a leitura das marcas feita depois de sair do modo de calibração.
    pub fn encerrar(mut self, depois: Option<&[(Pos, bool)]>) -> ResumoCalibracao {
        let segundos = self.inicio.elapsed().as_secs_f64();
        let (calibradas, total) = self.contagem();
        let faltaram: Vec<String> = self.estado.iter().filter(|(_, f)| !**f).map(|(p, _)| self.nome(*p)).collect();
        let conferidas = depois.map(|l| l.iter().filter(|(_, f)| *f).count());
        let resumo = ResumoCalibracao {
            total,
            calibradas,
            faltaram,
            segundos,
            conferidas,
            registro: self.registro.as_ref().map(|r| r.caminho().to_path_buf()),
        };
        if let Some(r) = self.registro.as_mut() {
            r.linha(&format!("calibração encerrada: {}", resumo.texto()));
        }
        resumo
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sessao_registra_cada_tecla_e_o_resumo() {
        let caminho = std::env::temp_dir().join(format!("mag82-teste-{}/calibracao.log", std::process::id()));
        let _ = std::fs::remove_file(&caminho);
        let nomes: BTreeMap<Pos, String> = [((0, 0), "esc"), ((2, 2), "w"), ((3, 1), "a")].map(|(p, n)| (p, n.to_string())).into();
        let zerado = [((0, 0), false), ((2, 2), false), ((3, 1), false)];
        let mut s = SessaoCalibracao::nova(&caminho, "teste", "0.0.2.0", nomes, &zerado);
        assert!(!s.sem_progresso());
        s.atualizar(&[((0, 0), false), ((2, 2), true), ((3, 1), false)]);
        s.atualizar(&[((0, 0), true), ((2, 2), true), ((3, 1), false)]);
        s.atualizar(&[((0, 0), true), ((2, 2), true), ((3, 1), false)]); // nada novo: não registra
        assert_eq!(s.contagem(), (2, 3));
        let resumo = s.encerrar(Some(&[((0, 0), true), ((2, 2), true), ((3, 1), false)]));
        assert_eq!((resumo.calibradas, resumo.total, resumo.conferidas), (2, 3, Some(2)));
        assert_eq!(resumo.faltaram, vec!["a".to_string()]);

        let texto = std::fs::read_to_string(&caminho).unwrap();
        let linhas: Vec<&str> = texto.lines().collect();
        assert_eq!(linhas.len(), 4, "{texto}");
        assert!(linhas[0].contains("calibração iniciada pela teste (firmware 0.0.2.0, 3 teclas, 0 já marcadas"));
        assert!(linhas[1].ends_with("w (1/3)") && linhas[2].ends_with("esc (2/3)"));
        assert!(linhas[3].contains("2 de 3 teclas calibradas") && linhas[3].contains("Não foram pressionadas: a."));
        // cada linha começa com data e hora
        assert!(linhas.iter().all(|l| l.as_bytes()[4] == b'-' && l.as_bytes()[13] == b':'));
        let _ = std::fs::remove_dir_all(caminho.parent().unwrap());
    }

    #[test]
    fn marcas_que_nao_zeram_sao_detectadas() {
        let caminho = std::env::temp_dir().join(format!("mag82-teste-{}-b/calibracao.log", std::process::id()));
        let s = SessaoCalibracao::nova(&caminho, "teste", "x", BTreeMap::new(), &[((0, 0), true), ((0, 1), true)]);
        assert!(s.sem_progresso());
        let _ = std::fs::remove_dir_all(caminho.parent().unwrap());
    }
}
