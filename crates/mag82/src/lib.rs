//! Protocolo do teclado VOLCANOS MAG82 (USB 1ca6:3920) sobre hidraw.
//!
//! Pacotes de 64 bytes na interface 3 (usage page 0xFFB0), no formato
//! `[grupo, subcomando, argumentos..., zeros]`.
//! A resposta repete grupo e subcomando.

pub mod registro;
pub mod simulado;

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const VID: u16 = 0x1CA6;
pub const PID: u16 = 0x3920;
/// Interface USB que recebe os comandos.
pub const CANAL: u8 = 3;
pub const TAM: usize = 64;
pub const LINHAS: u8 = 6;
pub const COLUNAS: u8 = 15;
/// Curso útil do switch magnético, em milésimos de milímetro.
pub const CURSO_MAX: u16 = 3400;
/// Curso mínimo que precisa sobrar entre as duas zonas mortas, em milésimos de milímetro.
pub const CURSO_UTIL_MIN: u16 = 100;
/// Quantidade de efeitos de luz das teclas (numerados de 0 a 20).
pub const EFEITOS: u8 = 21;
/// Quantidade de camadas de teclas (0 = base).
pub const CAMADAS: u8 = 4;
/// Camada que vale enquanto a tecla Fn está pressionada.
pub const CAMADA_FN: u8 = 1;
/// Nas camadas acima da base: a tecla faz o mesmo que na camada base.
pub const FUNCAO_PADRAO: u16 = 1;
/// Código da própria tecla Fn.
pub const TECLA_FN: u16 = 0xF101;
/// Área de luz das teclas.
pub const AREA_TECLAS: u8 = 0;
/// Área de luz da faixa lateral do teclado.
pub const AREA_LATERAL: u8 = 1;

mod grupo {
    pub const DEVICE: u8 = 1;
    pub const GLOBAL: u8 = 2;
    pub const LAYOUT: u8 = 3;
    pub const PERFORMANCE: u8 = 4;
    pub const LIGHTING: u8 = 5;
    pub const TECLA_AVANCADA: u8 = 6;
    /// Atualização de firmware: nunca enviado, pode inutilizar o teclado.
    pub const FIRMWARE: u8 = 8;
}

/// Posição de uma tecla na matriz: (linha, coluna).
pub type Pos = (u8, u8);

#[derive(Debug)]
pub enum Erro {
    NaoEncontrado,
    SemPermissao(PathBuf),
    Desconectado,
    SemResposta([u8; 4]),
    Bloqueado,
    Invalido(String),
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erro::NaoEncontrado => write!(f, "teclado não encontrado (está conectado nesta máquina?)"),
            Erro::SemPermissao(p) => {
                write!(
                    f,
                    "sem permissão para acessar o teclado ({}). Desconecte e reconecte o cabo; se continuar, \
                     reinstale o programa (é a instalação que libera o acesso)",
                    p.display()
                )
            }
            Erro::Desconectado => write!(f, "teclado desconectado"),
            Erro::SemResposta(c) => write!(f, "sem resposta para o comando {}", hexa(c)),
            Erro::Bloqueado => write!(f, "comando bloqueado (firmware, restauração de fábrica ou calibração)"),
            Erro::Invalido(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Erro {}

pub type Resultado<T> = Result<T, Erro>;

pub fn hexa(dados: &[u8]) -> String {
    dados.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn u16le(r: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([r[i], r[i + 1]])
}

/// Localiza os nós hidraw do teclado: `{interface USB: /dev/hidrawN}`.
pub fn interfaces() -> BTreeMap<u8, PathBuf> {
    if simulado::ligado() {
        return BTreeMap::from([(CANAL, PathBuf::from("(teclado simulado)"))]);
    }
    let alvo = format!("{:08X}:{:08X}", VID, PID);
    let mut achados = BTreeMap::new();
    let Ok(dir) = fs::read_dir("/sys/class/hidraw") else { return achados };
    for entrada in dir.flatten() {
        let dispositivo = entrada.path().join("device");
        let Ok(uevent) = fs::read_to_string(dispositivo.join("uevent")) else { continue };
        if !uevent.to_uppercase().contains(&alvo) {
            continue;
        }
        // caminho real: .../1-8:1.3/0003:1CA6:3920.000X  ->  interface 3
        let Ok(real) = fs::canonicalize(&dispositivo) else { continue };
        let interface = real
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .and_then(|n| n.rsplit('.').next())
            .and_then(|n| n.parse::<u8>().ok());
        if let Some(n) = interface {
            achados.insert(n, PathBuf::from("/dev").join(entrada.file_name()));
        }
    }
    achados
}

/// Comandos que a biblioteca nunca envia: atualização de firmware (grupo 8), que pode inutilizar
/// o teclado, e restauração de fábrica (2, 1).
fn nunca_enviado(dados: &[u8]) -> bool {
    matches!(dados, [grupo::FIRMWARE, ..] | [grupo::GLOBAL, 1, ..])
}

/// Comandos recusados em [`Teclado::comando`] (e portanto no `mag82 bruto`): os de
/// [`nunca_enviado`] e a calibração (2, 6), que muda o teclado inteiro só de ser enviada, sem
/// argumento que a torne uma leitura (`02 06 00` abre a calibração). A calibração só passa por
/// [`Teclado::iniciar_calibracao`] e [`Teclado::encerrar_calibracao`].
fn bloqueado(dados: &[u8]) -> bool {
    nunca_enviado(dados) || matches!(dados, [grupo::GLOBAL, 6, ..])
}

/// Trava exclusiva (flock) no dispositivo, solta ao sair de escopo.
struct Trava(libc::c_int);

impl Trava {
    fn obter(arquivo: &File) -> Resultado<Self> {
        let fd = arquivo.as_raw_fd();
        loop {
            // SAFETY: `fd` é um descritor aberto que pertence a `arquivo`.
            if unsafe { libc::flock(fd, libc::LOCK_EX) } == 0 {
                return Ok(Self(fd));
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return Err(Erro::Desconectado);
            }
        }
    }
}

impl Drop for Trava {
    fn drop(&mut self) {
        // SAFETY: o descritor continua aberto enquanto o `Teclado` que criou a trava existir.
        unsafe { libc::flock(self.0, libc::LOCK_UN) };
    }
}

/// Conexão aberta com o canal de comandos do teclado.
pub struct Teclado {
    /// O dispositivo de verdade; `None` no teclado simulado (ver [`simulado`]).
    arquivo: Option<File>,
    /// Mostra em stderr os pacotes enviados e recebidos.
    pub verboso: bool,
}

impl Teclado {
    pub fn abrir() -> Resultado<Self> {
        if simulado::ligado() {
            return Ok(Self { arquivo: None, verboso: false });
        }
        let caminho = interfaces().remove(&CANAL).ok_or(Erro::NaoEncontrado)?;
        let arquivo = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&caminho)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => Erro::SemPermissao(caminho.clone()),
                _ => Erro::NaoEncontrado,
            })?;
        Ok(Self { arquivo: Some(arquivo), verboso: false })
    }

    /// Espera até `limite` por dados; devolve `true` se há algo para ler.
    fn aguardar(arquivo: &File, limite: Duration) -> bool {
        let mut pfd = libc::pollfd { fd: arquivo.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: `pfd` é uma estrutura válida e o descritor pertence a `self.arquivo`.
        unsafe { libc::poll(&mut pfd, 1, limite.as_millis() as libc::c_int) > 0 }
    }

    /// Envia `[grupo, sub, args...]` e devolve a resposta de 64 bytes.
    ///
    /// Confere só grupo e subcomando na resposta; as leituras usam [`Teclado::ler`], que confere
    /// o pedido inteiro.
    pub fn comando(&mut self, dados: &[u8]) -> Resultado<[u8; TAM]> {
        if bloqueado(dados) {
            return Err(Erro::Bloqueado);
        }
        self.trocar(dados, 2)
    }

    /// Como [`Teclado::comando`], para leituras: o teclado repete o pedido inteiro no começo da
    /// resposta, então uma resposta destinada a outro pedido (de outro programa) é descartada.
    fn ler(&mut self, dados: &[u8]) -> Resultado<[u8; TAM]> {
        self.trocar(dados, dados.len())
    }

    /// Envia o pacote e espera a resposta cujos primeiros `eco` bytes repetem o pedido.
    ///
    /// Segura uma trava exclusiva no dispositivo durante a troca: todos os leitores do hidraw
    /// recebem todas as respostas, e sem a trava dois processos nossos (GUI e linha de comando)
    /// pegariam a resposta um do outro. Reenvia se não houver resposta: o teclado atende um
    /// comando por vez e descarta o nosso quando outro programa (ex.: o configurador web no
    /// navegador, que não usa a trava) está falando com ele.
    fn trocar(&mut self, dados: &[u8], eco: usize) -> Resultado<[u8; TAM]> {
        if dados.is_empty() || dados.len() > TAM {
            return Err(Erro::Invalido(format!("o pacote precisa ter de 1 a {TAM} bytes")));
        }
        if nunca_enviado(dados) {
            return Err(Erro::Bloqueado);
        }
        let Some(arquivo) = self.arquivo.as_mut() else {
            // teclado simulado: mesma conversa, sem dispositivo
            let resposta = simulado::Simulado::compartilhado().lock().ok().and_then(|mut s| s.responder(dados));
            if self.verboso {
                eprintln!("  -> {}", hexa(dados));
                if let Some(r) = &resposta {
                    eprintln!("  <- {}", hexa(r));
                }
            }
            return resposta.ok_or(Erro::SemResposta([
                dados[0],
                dados.get(1).copied().unwrap_or(0),
                dados.get(2).copied().unwrap_or(0),
                dados.get(3).copied().unwrap_or(0),
            ]));
        };
        let mut pacote = [0u8; TAM + 1]; // byte 0 = report ID (não usado)
        pacote[1..=dados.len()].copy_from_slice(dados);
        let mut resp = [0u8; TAM + 1];
        let eco = eco.clamp(2, dados.len().max(2));
        let _trava = Trava::obter(arquivo)?;

        for _ in 0..4 {
            while Self::aguardar(arquivo, Duration::ZERO) {
                if arquivo.read(&mut resp).is_err() {
                    break; // descarta sobras de respostas antigas
                }
            }
            arquivo.write_all(&pacote).map_err(|_| Erro::Desconectado)?;
            if self.verboso {
                eprintln!("  -> {}", hexa(&pacote[1..]));
            }
            let fim = Instant::now() + Duration::from_millis(400);
            loop {
                let resta = fim.saturating_duration_since(Instant::now());
                if resta.is_zero() || !Self::aguardar(arquivo, resta) {
                    break;
                }
                let n = match arquivo.read(&mut resp) {
                    Ok(n) => n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => return Err(Erro::Desconectado),
                };
                if self.verboso {
                    eprintln!("  <- {}", hexa(&resp[..n]));
                }
                if n >= eco && resp[..eco] == pacote[1..=eco] {
                    let mut saida = [0u8; TAM];
                    saida[..n.min(TAM)].copy_from_slice(&resp[..n.min(TAM)]);
                    return Ok(saida);
                }
            }
        }
        Err(Erro::SemResposta([pacote[1], pacote[2], pacote[3], pacote[4]]))
    }

    // ---------- dispositivo ----------

    pub fn dispositivo(&mut self) -> Resultado<Dispositivo> {
        let p = self.ler(&[grupo::DEVICE, 1])?;
        let d = self.ler(&[grupo::DEVICE, 2])?;
        let f = self.ler(&[grupo::DEVICE, 3])?;
        let bits = |valor: u8, nomes: &[&str]| -> Vec<String> {
            nomes.iter().enumerate().filter(|(i, _)| valor >> i & 1 == 1).map(|(_, n)| n.to_string()).collect()
        };
        let texto: Vec<u8> = d[29..46].iter().copied().filter(|b| *b != 0).collect();
        Ok(Dispositivo {
            protocolo: format!("{}.{}", p[2], p[3]),
            firmware: d[8..12].iter().map(|b| b.to_string()).collect::<Vec<_>>().join("."),
            pcb: d[12..16].iter().map(|b| b.to_string()).collect::<Vec<_>>().join("-"),
            data_firmware: String::from_utf8_lossy(&texto).into_owned(),
            switches: bits(f[2], &["mecânico", "magnético", "óptico", "indutivo", "magnético 3D"]),
            conexoes: bits(f[3], &["USB", "2.4G", "Bluetooth", "USB3"]),
            recursos: bits(f[4], &["RGB", "knob"]),
        })
    }

    /// Taxas de resposta suportadas e a atual, como códigos (ver [`nome_taxa`]).
    pub fn taxas(&mut self) -> Resultado<(Vec<u8>, u8)> {
        let r = self.ler(&[grupo::GLOBAL, 5, 0])?;
        let suportadas = r[4..4 + (r[3] as usize).min(TAM - 4)].to_vec();
        let atual = self.ler(&[grupo::GLOBAL, 5, 1])?[3];
        Ok((suportadas, atual))
    }

    pub fn definir_taxa(&mut self, codigo: u8) -> Resultado<()> {
        self.comando(&[grupo::GLOBAL, 5, 2, codigo])?;
        self.esperar("a taxa de resposta", |t| Ok(t.taxas()?.1 == codigo))
    }

    /// Depois de uma gravação, relê até o teclado devolver o valor novo. Ele leva alguns
    /// milissegundos para refletir o que foi gravado; sem esperar, quem lê logo em seguida
    /// recebe o valor antigo e pode regravá-lo por cima.
    fn esperar(&mut self, o_que: &str, mut confere: impl FnMut(&mut Self) -> Resultado<bool>) -> Resultado<()> {
        for _ in 0..50 {
            if confere(self)? {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(Erro::Invalido(format!("o teclado não confirmou {o_que}")))
    }

    // ---------- calibração ----------

    /// Põe o teclado em modo de calibração: as teclas acendem em vermelho, deixam de digitar e
    /// cada uma fica verde (e [`Atuacao::calibrada`]) ao ser pressionada até o fundo. O modo só
    /// termina com [`Teclado::encerrar_calibracao`].
    ///
    /// Antes de enviar, deixa um marcador em disco (ver [`registro::calibracao_pendente`]): se o
    /// programa morrer no meio, a próxima execução sabe que precisa encerrar a calibração.
    pub fn iniciar_calibracao(&mut self) -> Resultado<()> {
        registro::marcar_calibracao(true);
        self.trocar(&[grupo::GLOBAL, 6, 0], 2).map(|_| ())
    }

    /// Tira o teclado do modo de calibração e apaga o marcador em disco.
    pub fn encerrar_calibracao(&mut self) -> Resultado<()> {
        self.trocar(&[grupo::GLOBAL, 6, 1], 2)?;
        registro::marcar_calibracao(false);
        Ok(())
    }

    // ---------- perfis ----------

    /// Quantidade de perfis e o índice do perfil ativo (a partir de 0).
    pub fn perfis(&mut self) -> Resultado<(u8, u8)> {
        let total = self.ler(&[grupo::GLOBAL, 3, 0])?[3];
        let atual = self.ler(&[grupo::GLOBAL, 3, 1])?[3];
        Ok((total, atual))
    }

    /// Troca o perfil ativo. Todas as configurações (teclas, luz) passam a ser as do novo perfil.
    ///
    /// Só retorna quando o teclado confirma a troca: logo depois do comando ele ainda responde
    /// com os dados do perfil anterior.
    pub fn definir_perfil(&mut self, indice: u8) -> Resultado<()> {
        self.comando(&[grupo::GLOBAL, 3, 2, indice])?;
        for _ in 0..50 {
            if self.perfis()?.1 == indice {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(Erro::Invalido("o teclado não confirmou a troca de perfil".into()))
    }

    pub fn nome_perfil(&mut self, indice: u8) -> Resultado<String> {
        let r = self.ler(&[grupo::GLOBAL, 3, 3, indice])?;
        let bytes: Vec<u8> = r[4..36].iter().copied().take_while(|b| *b != 0).collect();
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    }

    /// Renomeia um perfil; o nome é cortado em 32 bytes.
    pub fn definir_nome_perfil(&mut self, indice: u8, nome: &str) -> Resultado<()> {
        let mut fim = nome.len().min(32);
        while !nome.is_char_boundary(fim) {
            fim -= 1;
        }
        let mut dados = vec![grupo::GLOBAL, 3, 4, indice];
        dados.extend_from_slice(&nome.as_bytes()[..fim]);
        self.comando(&dados)?;
        let esperado = nome[..fim].trim().to_string();
        self.esperar("o nome do perfil", |t| Ok(t.nome_perfil(indice)? == esperado))
    }

    // ---------- teclas avançadas ----------

    pub fn avancada(&mut self, pos: Pos) -> Resultado<Avancada> {
        let r = self.ler(&[grupo::TECLA_AVANCADA, 1, pos.0, pos.1])?;
        Ok(match r[4] {
            0 => Avancada::Nenhuma,
            1 => Avancada::Dks(Dks {
                teclas: [u16le(&r, 5), u16le(&r, 7), u16le(&r, 9), u16le(&r, 11)],
                gatilhos: [r[13], r[14], r[15], r[16]],
                inicio: u16le(&r, 17),
                fundo: u16le(&r, 19),
            }),
            2 => Avancada::Mpt(Mpt {
                teclas: [u16le(&r, 5), u16le(&r, 7), u16le(&r, 9)],
                pontos: [u16le(&r, 11), u16le(&r, 13), u16le(&r, 15)],
            }),
            3 => Avancada::Mt(Mt { toque: u16le(&r, 5), segura: u16le(&r, 7), tempo: u16le(&r, 9) }),
            4 => Avancada::Tgl(Tgl { tecla: u16le(&r, 5), tempo: u16le(&r, 7) }),
            5 => Avancada::End(End { ao_pressionar: u16le(&r, 5), ao_soltar: u16le(&r, 7), atraso: u16le(&r, 9) }),
            6 => Avancada::Socd { par: (r[5], r[6]), codigos: (u16le(&r, 7), u16le(&r, 9)), regra: r[13] },
            7 => Avancada::Rs { par: (r[5], r[6]), codigos: (u16le(&r, 7), u16le(&r, 9)), atraso: u16le(&r, 11) },
            outro => Avancada::Outra(outro),
        })
    }

    /// Liga o SOCD entre duas teclas opostas (ex.: A e D). `cod_a` e `cod_b` são as funções
    /// atuais das teclas.
    pub fn definir_socd(&mut self, a: Pos, b: Pos, cod_a: u16, cod_b: u16, modo: ModoSocd) -> Resultado<()> {
        if a == b {
            return Err(Erro::Invalido("o SOCD precisa de duas teclas diferentes".into()));
        }
        self.desfazer_par(a, Some(b))?;
        self.desfazer_par(b, Some(a))?;
        let (regra_a, regra_b) = modo.regras();
        let (ca, cb) = (cod_a.to_le_bytes(), cod_b.to_le_bytes());
        // um pacote para cada tecla, cada um apontando para a outra; atraso (2 bytes) sempre 0
        self.comando(&[grupo::TECLA_AVANCADA, 2, a.0, a.1, 6, b.0, b.1, ca[0], ca[1], cb[0], cb[1], 0, 0, regra_a])?;
        self.comando(&[grupo::TECLA_AVANCADA, 2, b.0, b.1, 6, a.0, a.1, cb[0], cb[1], ca[0], ca[1], 0, 0, regra_b])?;
        let de_a = Avancada::Socd { par: b, codigos: (cod_a, cod_b), regra: regra_a };
        let de_b = Avancada::Socd { par: a, codigos: (cod_b, cod_a), regra: regra_b };
        self.esperar("o SOCD", |t| Ok(t.avancada(a)? == de_a && t.avancada(b)? == de_b))
    }

    /// Se a tecla faz parte de um par (SOCD ou RS), tira a função avançada da outra tecla do par:
    /// uma tecla só não sustenta o par, e a outra ficaria apontando para cá.
    fn desfazer_par(&mut self, pos: Pos, menos: Option<Pos>) -> Resultado<()> {
        if let Avancada::Socd { par, .. } | Avancada::Rs { par, .. } = self.avancada(pos)? {
            if Some(par) != menos {
                self.remover_avancada(par)?;
            }
        }
        Ok(())
    }

    /// Grava uma função avançada de uma tecla só (`corpo` vem depois do tipo) e espera a leitura
    /// devolver `esperada`.
    fn gravar_avancada(&mut self, pos: Pos, tipo: u8, corpo: &[u8], esperada: Avancada, nome: &str) -> Resultado<()> {
        self.desfazer_par(pos, None)?;
        let mut dados = vec![grupo::TECLA_AVANCADA, 2, pos.0, pos.1, tipo];
        dados.extend_from_slice(corpo);
        self.comando(&dados)?;
        self.esperar(nome, |t| Ok(t.avancada(pos)? == esperada))
    }

    /// Multiponto: até três funções, cada uma acionada quando a tecla passa de uma profundidade.
    pub fn definir_mpt(&mut self, pos: Pos, mpt: &Mpt) -> Resultado<()> {
        mpt.validar()?;
        let corpo: Vec<u8> = mpt.teclas.iter().chain(&mpt.pontos).flat_map(|v| v.to_le_bytes()).collect();
        self.gravar_avancada(pos, 2, &corpo, Avancada::Mpt(*mpt), "o multiponto")
    }

    /// Dupla função: um toque curto envia uma função, segurar envia outra.
    pub fn definir_mt(&mut self, pos: Pos, mt: &Mt) -> Resultado<()> {
        mt.validar()?;
        let corpo: Vec<u8> = [mt.toque, mt.segura, mt.tempo].iter().flat_map(|v| v.to_le_bytes()).collect();
        self.gravar_avancada(pos, 3, &corpo, Avancada::Mt(*mt), "a dupla função")
    }

    /// Contínuo: um toque deixa a função pressionada até o toque seguinte.
    pub fn definir_tgl(&mut self, pos: Pos, tgl: &Tgl) -> Resultado<()> {
        tgl.validar()?;
        let corpo: Vec<u8> = [tgl.tecla, tgl.tempo].iter().flat_map(|v| v.to_le_bytes()).collect();
        self.gravar_avancada(pos, 4, &corpo, Avancada::Tgl(*tgl), "o acionamento contínuo")
    }

    /// Ao soltar: uma função ao pressionar e outra ao soltar a tecla.
    pub fn definir_end(&mut self, pos: Pos, end: &End) -> Resultado<()> {
        end.validar()?;
        let corpo: Vec<u8> = [end.ao_pressionar, end.ao_soltar, end.atraso].iter().flat_map(|v| v.to_le_bytes()).collect();
        self.gravar_avancada(pos, 5, &corpo, Avancada::End(*end), "o acionamento ao soltar")
    }

    /// Liga o RS entre duas teclas: vale a que estiver mais pressionada, e as duas quando ambas
    /// chegam ao fundo. `cod_a` e `cod_b` são as funções atuais das teclas; `atraso` em ms.
    pub fn definir_rs(&mut self, a: Pos, b: Pos, cod_a: u16, cod_b: u16, atraso: u16) -> Resultado<()> {
        if a == b {
            return Err(Erro::Invalido("o RS precisa de duas teclas diferentes".into()));
        }
        if atraso > ATRASO_MAX {
            return Err(Erro::Invalido(format!("o atraso vai de 0 a {ATRASO_MAX} ms")));
        }
        self.desfazer_par(a, Some(b))?;
        self.desfazer_par(b, Some(a))?;
        let (ca, cb, at) = (cod_a.to_le_bytes(), cod_b.to_le_bytes(), atraso.to_le_bytes());
        self.comando(&[grupo::TECLA_AVANCADA, 2, a.0, a.1, 7, b.0, b.1, ca[0], ca[1], cb[0], cb[1], at[0], at[1]])?;
        self.comando(&[grupo::TECLA_AVANCADA, 2, b.0, b.1, 7, a.0, a.1, cb[0], cb[1], ca[0], ca[1], at[0], at[1]])?;
        let de_a = Avancada::Rs { par: b, codigos: (cod_a, cod_b), atraso };
        let de_b = Avancada::Rs { par: a, codigos: (cod_b, cod_a), atraso };
        self.esperar("o RS", |t| Ok(t.avancada(a)? == de_a && t.avancada(b)? == de_b))
    }

    /// Põe um DKS na tecla: até quatro funções, cada uma acionada em momentos diferentes do
    /// curso (ver [`Dks`]). Se a tecla fazia parte de um par SOCD ou RS, o par é desfeito.
    pub fn definir_dks(&mut self, pos: Pos, dks: &Dks) -> Resultado<()> {
        dks.validar()?;
        self.desfazer_par(pos, None)?;
        let mut dados = vec![grupo::TECLA_AVANCADA, 2, pos.0, pos.1, 1];
        for tecla in dks.teclas {
            dados.extend_from_slice(&tecla.to_le_bytes());
        }
        dados.extend_from_slice(&dks.gatilhos);
        dados.extend_from_slice(&dks.inicio.to_le_bytes());
        dados.extend_from_slice(&dks.fundo.to_le_bytes());
        self.comando(&dados)?;
        let esperado = Avancada::Dks(*dks);
        self.esperar("o DKS", |t| Ok(t.avancada(pos)? == esperado))
    }

    /// Remove a função avançada (SOCD etc.) de uma tecla.
    pub fn remover_avancada(&mut self, pos: Pos) -> Resultado<()> {
        self.comando(&[grupo::TECLA_AVANCADA, 2, pos.0, pos.1, 0])?;
        self.esperar("a remoção da função avançada", |t| Ok(t.avancada(pos)? == Avancada::Nenhuma))
    }

    // ---------- iluminação ----------

    /// Áreas de luz do teclado: a das teclas ([`AREA_TECLAS`]) e, se houver, a faixa lateral
    /// ([`AREA_LATERAL`]), cada uma com sua quantidade de efeitos e de LEDs.
    pub fn areas_luz(&mut self) -> Resultado<Vec<AreaLuz>> {
        let r = self.ler(&[grupo::GLOBAL, 8, 0])?;
        Ok((0..(r[3] as usize).min(15))
            .map(|i| &r[4 + 4 * i..8 + 4 * i])
            .map(|a| AreaLuz { indice: a[0], efeitos: a[1], linhas: a[2], colunas: a[3] })
            .collect())
    }

    pub fn luz(&mut self) -> Resultado<Luz> {
        self.luz_de(AREA_TECLAS)
    }

    pub fn definir_luz(&mut self, l: &Luz) -> Resultado<()> {
        self.definir_luz_de(AREA_TECLAS, EFEITOS, l)
    }

    pub fn luz_de(&mut self, area: u8) -> Resultado<Luz> {
        let r = self.ler(&[grupo::LIGHTING, 1, area, 0])?;
        Ok(Luz { ligada: r[4] != 0, modo: r[5], brilho: r[6], velocidade: r[7], invertida: r[8] != 0, cor: r[9] })
    }

    /// `efeitos` é a quantidade de efeitos da área (ver [`Teclado::areas_luz`]).
    pub fn definir_luz_de(&mut self, area: u8, efeitos: u8, l: &Luz) -> Resultado<()> {
        if l.modo >= efeitos || l.brilho > 100 || l.velocidade > 100 || l.cor > 7 {
            return Err(Erro::Invalido("iluminação fora da faixa permitida".into()));
        }
        self.comando(&[
            grupo::LIGHTING, 2, area, 0, l.ligada as u8, l.modo, l.brilho, l.velocidade, l.invertida as u8, l.cor,
        ])?;
        let esperada = l.clone();
        self.esperar("a iluminação", |t| Ok(t.luz_de(area)? == esperada))
    }

    /// As 8 cores da paleta (a posição 0 é reservada e não deve ser alterada).
    pub fn paleta(&mut self) -> Resultado<Paleta> {
        self.paleta_de(AREA_TECLAS)
    }

    pub fn definir_paleta(&mut self, p: &Paleta) -> Resultado<()> {
        self.definir_paleta_de(AREA_TECLAS, p)
    }

    /// Cada área de luz tem a sua paleta.
    pub fn paleta_de(&mut self, area: u8) -> Resultado<Paleta> {
        let r = self.ler(&[grupo::LIGHTING, 1, area, 1])?;
        let mut bruto = [0u8; 32];
        bruto.copy_from_slice(&r[4..36]);
        Ok(Paleta { bruto })
    }

    pub fn definir_paleta_de(&mut self, area: u8, p: &Paleta) -> Resultado<()> {
        let mut dados = vec![grupo::LIGHTING, 2, area, 1];
        dados.extend_from_slice(&p.bruto);
        self.comando(&dados)?;
        let esperada = p.bruto;
        self.esperar("a paleta", |t| Ok(t.paleta_de(area)?.bruto == esperada))
    }

    /// Cores por tecla (as fixas e as que o efeito está mostrando no momento).
    pub fn grade(&mut self) -> Resultado<Grade> {
        self.ler_grade(AREA_TECLAS, Grade::PAGINAS)
    }

    /// Grava as cores fixas das teclas e só retorna quando o teclado passa a devolvê-las na leitura.
    ///
    /// O teclado leva alguns milissegundos para refletir a gravação; sem esperar, quem lê logo em
    /// seguida (outro `mag82 cor`, por exemplo) recebe a grade antiga e, ao regravar, desfaz esta.
    pub fn definir_grade(&mut self, g: &Grade) -> Resultado<()> {
        self.gravar_grade(AREA_TECLAS, g)
    }

    /// Cores dos LEDs de uma área que não é a das teclas (a faixa lateral), um LED por posição.
    pub fn grade_de(&mut self, area: &AreaLuz) -> Resultado<Grade> {
        let leds = area.linhas as usize * area.colunas as usize;
        let mut grade = self.ler_grade(area.indice, leds.div_ceil(15))?;
        grade.bruto.truncate(4 * leds);
        Ok(grade)
    }

    /// Como [`Teclado::definir_grade`], para a área de onde `g` foi lida com [`Teclado::grade_de`].
    pub fn definir_grade_de(&mut self, area: u8, g: &Grade) -> Resultado<()> {
        self.gravar_grade(area, g)
    }

    fn ler_grade(&mut self, area: u8, paginas: usize) -> Resultado<Grade> {
        let mut bruto = Vec::with_capacity(paginas * Grade::BYTES_PAGINA);
        for pagina in 0..paginas as u8 {
            let r = self.ler(&[grupo::LIGHTING, 3, area, pagina])?;
            bruto.extend_from_slice(&r[4..4 + Grade::BYTES_PAGINA]);
        }
        Ok(Grade { bruto })
    }

    fn gravar_grade(&mut self, area: u8, g: &Grade) -> Resultado<()> {
        let paginas = g.bruto.len().div_ceil(Grade::BYTES_PAGINA);
        // No modo "só as cores fixas" o teclado ignora o pedido de soltar uma cor: a posição
        // continua fixa. Fora desse modo ele aceita, então a gravação passa por outro efeito por
        // um instante e volta.
        let mut atual = self.ler_grade(area, paginas)?;
        atual.bruto.truncate(g.bruto.len());
        let solta = atual.bruto.chunks(4).zip(g.bruto.chunks(4)).any(|(antes, depois)| antes[3] == 255 && depois[3] != 255);
        if solta {
            let luz = self.luz_de(area)?;
            let efeitos = self.areas_luz()?.iter().find(|a| a.indice == area).map_or(EFEITOS, |a| a.efeitos);
            if efeitos > 1 && luz.modo + 1 == efeitos {
                self.definir_luz_de(area, efeitos, &Luz { modo: 0, ..luz })?;
                let gravou = self.gravar_grade_direto(area, g, paginas);
                self.definir_luz_de(area, efeitos, &luz)?;
                return gravou;
            }
        }
        self.gravar_grade_direto(area, g, paginas)
    }

    fn gravar_grade_direto(&mut self, area: u8, g: &Grade, paginas: usize) -> Resultado<()> {
        for (pagina, bloco) in g.bruto.chunks(Grade::BYTES_PAGINA).enumerate() {
            let mut dados = vec![grupo::LIGHTING, 4, area, pagina as u8];
            dados.extend_from_slice(bloco);
            self.comando(&dados)?;
        }
        for _ in 0..50 {
            let mut lida = self.ler_grade(area, paginas)?;
            lida.bruto.truncate(g.bruto.len());
            if lida.mesmas_fixas(g) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(Erro::Invalido("o teclado não confirmou as cores gravadas".into()))
    }

    // ---------- teclas ----------

    /// Código (usage HID) de cada posição na camada base; `fabrica` lê o layout original.
    pub fn layout(&mut self, fabrica: bool) -> Resultado<BTreeMap<Pos, u16>> {
        self.layout_da_camada(0, fabrica)
    }

    /// Código de cada posição numa camada: 0 é a base e [`CAMADA_FN`] a que vale com Fn
    /// pressionado. Nas camadas acima da base, [`FUNCAO_PADRAO`] quer dizer "igual à base".
    pub fn layout_da_camada(&mut self, camada: u8, fabrica: bool) -> Resultado<BTreeMap<Pos, u16>> {
        let sub = if fabrica { 6 } else { 1 };
        let mut layout = BTreeMap::new();
        for linha in 0..LINHAS {
            let r = self.ler(&[grupo::LAYOUT, sub, camada, linha])?;
            for col in 0..COLUNAS {
                layout.insert((linha, col), u16le(&r, 4 + 2 * col as usize));
            }
        }
        Ok(layout)
    }

    /// Remapeia uma tecla na camada base e devolve o código que o teclado passou a usar.
    pub fn definir_funcao(&mut self, pos: Pos, codigo: u16) -> Resultado<u16> {
        self.definir_funcao_na_camada(0, pos, codigo)
    }

    /// Muda o que uma tecla faz numa camada (ver [`Teclado::layout_da_camada`]) e devolve o
    /// código que o teclado passou a usar.
    pub fn definir_funcao_na_camada(&mut self, camada: u8, pos: Pos, codigo: u16) -> Resultado<u16> {
        if camada >= CAMADAS {
            return Err(Erro::Invalido(format!("camada inexistente: use de 0 a {}", CAMADAS - 1)));
        }
        let c = codigo.to_le_bytes();
        self.comando(&[grupo::LAYOUT, 4, camada, pos.0, pos.1, c[0], c[1]])?;
        // espera a leitura refletir a gravação; se o teclado ficar com outro código, é esse que vale
        let mut lido = self.funcao_na_camada(camada, pos)?;
        for _ in 0..50 {
            if lido == codigo {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            lido = self.funcao_na_camada(camada, pos)?;
        }
        // um par SOCD guarda a função das duas teclas: acompanha a mudança na camada base
        if camada == 0 {
            if let Avancada::Socd { par, regra, .. } = self.avancada(pos)? {
                let do_par = self.funcao_na_camada(0, par)?;
                self.definir_socd(pos, par, lido, do_par, ModoSocd::da_regra(regra))?;
            }
        }
        Ok(lido)
    }

    /// O que uma tecla faz numa camada.
    pub fn funcao_na_camada(&mut self, camada: u8, pos: Pos) -> Resultado<u16> {
        let r = self.ler(&[grupo::LAYOUT, 3, camada, pos.0, pos.1])?;
        Ok(u16le(&r, 5))
    }

    /// Posição física de cada tecla, em unidades de tecla.
    pub fn geometria(&mut self) -> Resultado<BTreeMap<Pos, Geometria>> {
        let mut geo = BTreeMap::new();
        for linha in 0..LINHAS {
            let r = self.ler(&[grupo::LAYOUT, 5, linha])?;
            for col in 0..COLUNAS {
                let v = u16le(&r, 3 + 2 * col as usize);
                if v == 0 {
                    continue;
                }
                let formato = (v & 15) as u8;
                geo.insert(
                    (linha, col),
                    Geometria {
                        x: f64::from((v >> 4) & 127) / 4.0,
                        y: f64::from((v >> 11) & 31) / 4.0,
                        largura: largura_formato(formato),
                        formato,
                    },
                );
            }
        }
        Ok(geo)
    }

    pub fn atuacao(&mut self, pos: Pos) -> Resultado<Atuacao> {
        let r = self.ler(&[grupo::PERFORMANCE, 1, pos.0, pos.1])?;
        let mut corpo = [0u8; 23];
        corpo.copy_from_slice(&r[4..27]);
        Ok(Atuacao { corpo })
    }

    /// Grava a atuação e devolve o que o teclado passou a usar.
    pub fn definir_atuacao(&mut self, pos: Pos, a: &Atuacao) -> Resultado<Atuacao> {
        a.validar()?;
        let mut dados = vec![grupo::PERFORMANCE, 2, pos.0, pos.1];
        dados.extend_from_slice(&a.corpo);
        dados[4 + 16] = 0; // marca de calibração: sempre 0 na gravação
        self.comando(&dados)?;
        // espera a leitura refletir a gravação (a marca de calibração é do teclado, não entra na conta)
        let igual = |lida: &Atuacao| lida.corpo[..16] == dados[4..20] && lida.corpo[17..] == dados[21..27];
        let mut lida = self.atuacao(pos)?;
        for _ in 0..50 {
            if igual(&lida) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            lida = self.atuacao(pos)?;
        }
        Ok(lida)
    }
}

#[derive(Debug, Clone)]
pub struct Dispositivo {
    pub protocolo: String,
    pub firmware: String,
    pub pcb: String,
    /// No formato `AAAAMMDDhh:mm:ss`.
    pub data_firmware: String,
    pub switches: Vec<String>,
    pub conexoes: Vec<String>,
    pub recursos: Vec<String>,
}

/// Função avançada configurada em uma tecla.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Avancada {
    Nenhuma,
    /// SOCD com a tecla `par`. `codigos` são as funções guardadas no par (a desta tecla e a da
    /// outra). `regra` é a visão desta tecla: 0 = última pressionada vence, 1 = esta tecla tem
    /// prioridade, 2 = a outra tem prioridade, 3 = neutro.
    Socd { par: Pos, codigos: (u16, u16), regra: u8 },
    /// Acionamento dinâmico: funções diferentes em pontos diferentes do curso da tecla.
    Dks(Dks),
    /// Multiponto: funções diferentes a cada profundidade.
    Mpt(Mpt),
    /// Dupla função: toque curto e segurar.
    Mt(Mt),
    /// Contínuo: um toque deixa a função pressionada.
    Tgl(Tgl),
    /// Ao soltar: uma função ao pressionar, outra ao soltar.
    End(End),
    /// RS com a tecla `par`: vale a mais pressionada das duas. `codigos` como no SOCD; `atraso`
    /// em milissegundos.
    Rs { par: Pos, codigos: (u16, u16), atraso: u16 },
    /// Outro tipo (toggle etc.), ainda não suportado; o número é o código do tipo.
    Outra(u8),
}

/// Maior atraso, em milissegundos, aceito nos pares (RS) e no acionamento ao soltar.
pub const ATRASO_MAX: u16 = 50;
/// Faixa do tempo, em milissegundos, que separa toque de segurar (MT e TGL).
pub const TEMPO_MIN: u16 = 10;
pub const TEMPO_MAX: u16 = 1000;

/// Multiponto (MPT): a função `teclas[i]` (0 = vaga livre) é acionada quando a tecla passa de
/// `pontos[i]`, em milésimos de milímetro. As profundidades usadas precisam crescer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mpt {
    pub teclas: [u16; 3],
    pub pontos: [u16; 3],
}

impl Default for Mpt {
    fn default() -> Self {
        Self { teclas: [0; 3], pontos: [500, 1000, 1500] }
    }
}

impl Mpt {
    pub fn validar(&self) -> Resultado<()> {
        if self.teclas.iter().all(|t| *t == 0) {
            return Err(Erro::Invalido("o multiponto precisa de pelo menos uma função".into()));
        }
        if self.pontos.iter().any(|p| !(DKS_MIN..=DKS_MAX).contains(p)) {
            return Err(Erro::Invalido("as profundidades do multiponto vão de 0,1 a 3,3 mm".into()));
        }
        if !(self.pontos[0] < self.pontos[1] && self.pontos[1] < self.pontos[2]) {
            return Err(Erro::Invalido("as profundidades do multiponto precisam crescer da primeira à terceira".into()));
        }
        Ok(())
    }
}

/// Dupla função (MT): soltar a tecla antes de `tempo` (ms) envia `toque`; segurar além disso
/// envia `segura`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mt {
    pub toque: u16,
    pub segura: u16,
    pub tempo: u16,
}

impl Mt {
    pub fn validar(&self) -> Resultado<()> {
        if self.toque == 0 || self.segura == 0 {
            return Err(Erro::Invalido("a dupla função precisa das duas funções: a do toque e a de segurar".into()));
        }
        validar_tempo(self.tempo)
    }
}

/// Contínuo (TGL): um toque mais curto que `tempo` (ms) deixa `tecla` pressionada até o toque
/// seguinte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tgl {
    pub tecla: u16,
    pub tempo: u16,
}

impl Tgl {
    pub fn validar(&self) -> Resultado<()> {
        if self.tecla == 0 {
            return Err(Erro::Invalido("o acionamento contínuo precisa de uma função".into()));
        }
        validar_tempo(self.tempo)
    }
}

/// Ao soltar (END): `ao_pressionar` é enviada quando a tecla desce e `ao_soltar` quando ela
/// sobe; `atraso` em milissegundos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct End {
    pub ao_pressionar: u16,
    pub ao_soltar: u16,
    pub atraso: u16,
}

impl End {
    pub fn validar(&self) -> Resultado<()> {
        if self.ao_pressionar == 0 && self.ao_soltar == 0 {
            return Err(Erro::Invalido("o acionamento ao soltar precisa de pelo menos uma função".into()));
        }
        if self.atraso > ATRASO_MAX {
            return Err(Erro::Invalido(format!("o atraso vai de 0 a {ATRASO_MAX} ms")));
        }
        Ok(())
    }
}

fn validar_tempo(tempo: u16) -> Resultado<()> {
    if (TEMPO_MIN..=TEMPO_MAX).contains(&tempo) {
        Ok(())
    } else {
        Err(Erro::Invalido(format!("o tempo vai de {TEMPO_MIN} a {TEMPO_MAX} ms")))
    }
}

/// Menor e maior distância aceitas nos dois pontos do DKS, em milésimos de milímetro.
pub const DKS_MIN: u16 = 100;
pub const DKS_MAX: u16 = 3300;
/// Os quatro momentos do curso em que um DKS pode agir.
pub const DKS_MOMENTOS: [&str; 4] =
    ["ao descer até o início", "ao chegar ao fundo", "ao sair do fundo", "ao voltar ao início"];

/// DKS de uma tecla. O curso tem dois pontos, `inicio` e `fundo`, e portanto quatro momentos:
/// descer até o início, chegar ao fundo, sair do fundo e voltar ao início. Cada uma das até
/// quatro funções (`teclas`, 0 = vaga livre) tem um mapa de sete casas (ver [`Dks::casas`]):
/// as pares dizem em quais momentos a função é acionada e as ímpares, se ela continua
/// pressionada até o momento seguinte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dks {
    pub teclas: [u16; 4],
    /// Mapa de cada função, como o teclado guarda (um byte por função).
    pub gatilhos: [u8; 4],
    /// Distâncias em milésimos de milímetro.
    pub inicio: u16,
    pub fundo: u16,
}

impl Default for Dks {
    fn default() -> Self {
        Self { teclas: [0; 4], gatilhos: [0; 4], inicio: 1500, fundo: 3000 }
    }
}

impl Dks {
    /// Os bits de cada uma das sete casas no byte do teclado: momento 1, segura até o 2,
    /// momento 2, segura até o 3, momento 3, segura até o 4, momento 4.
    const BITS: [u8; 7] = [1, 2, 4, 8 | 16, 32, 64, 128];

    /// As sete casas da função `i`: momento, segura, momento, segura, momento, segura, momento.
    pub fn casas(&self, i: usize) -> [bool; 7] {
        Self::BITS.map(|bits| self.gatilhos[i] & bits == bits)
    }

    pub fn definir_casas(&mut self, i: usize, casas: [bool; 7]) {
        self.gatilhos[i] = Self::BITS.iter().zip(casas).filter(|(_, ligada)| *ligada).fold(0, |soma, (b, _)| soma | b);
    }

    /// As casas escritas como texto: "1" aciona no momento 1, "1-3" aciona no 1 e segura até o
    /// 3, "2+" só segura depois do momento 2; itens separados por vírgula ("1-2,4").
    pub fn casas_em_texto(casas: [bool; 7]) -> String {
        let mut itens = Vec::new();
        let mut m = 0;
        while m < 4 {
            if !casas[2 * m] {
                m += 1;
                continue;
            }
            let inicio = m;
            while m < 3 && casas[2 * m + 1] && casas[2 * m + 2] {
                m += 1;
            }
            itens.push(if m > inicio { format!("{}-{}", inicio + 1, m + 1) } else { format!("{}", m + 1) });
            m += 1;
        }
        // "segura" sem os dois momentos em volta ligados: não cabe em um intervalo
        for m in 0..3 {
            if casas[2 * m + 1] && !(casas[2 * m] && casas[2 * m + 2]) {
                itens.push(format!("{}+", m + 1));
            }
        }
        itens.join(",")
    }

    /// O contrário de [`Dks::casas_em_texto`]; `None` se o texto não segue o formato.
    pub fn casas_de_texto(texto: &str) -> Option<[bool; 7]> {
        let momento = |t: &str| t.parse::<usize>().ok().filter(|m| (1..=4).contains(m)).map(|m| m - 1);
        let mut casas = [false; 7];
        for item in texto.split(',').map(str::trim) {
            if let Some(m) = item.strip_suffix('+') {
                let m = momento(m).filter(|m| *m < 3)?;
                casas[2 * m + 1] = true;
            } else if let Some((de, ate)) = item.split_once('-') {
                let (de, ate) = (momento(de)?, momento(ate)?);
                if de >= ate {
                    return None;
                }
                casas[2 * de..=2 * ate].fill(true);
            } else {
                casas[2 * momento(item)?] = true;
            }
        }
        Some(casas)
    }

    pub fn validar(&self) -> Resultado<()> {
        let faixa = DKS_MIN..=DKS_MAX;
        if !faixa.contains(&self.inicio) || !faixa.contains(&self.fundo) {
            return Err(Erro::Invalido("as distâncias do DKS vão de 0,1 a 3,3 mm".into()));
        }
        if self.inicio >= self.fundo {
            return Err(Erro::Invalido("no DKS o ponto de início precisa vir antes do fundo".into()));
        }
        if self.teclas.iter().all(|t| *t == 0) {
            return Err(Erro::Invalido("o DKS precisa de pelo menos uma função".into()));
        }
        for (tecla, gatilho) in self.teclas.iter().zip(self.gatilhos) {
            if (*tecla == 0) != (gatilho == 0) {
                return Err(Erro::Invalido(
                    "no DKS cada função precisa de pelo menos um momento, e vaga livre não tem momento".into(),
                ));
            }
        }
        Ok(())
    }
}

/// O que o teclado envia quando as duas teclas de um par SOCD estão pressionadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModoSocd {
    /// Vale a última tecla pressionada.
    UltimaVence,
    /// A primeira tecla do par sempre vence.
    PrioridadeA,
    /// A segunda tecla do par sempre vence.
    PrioridadeB,
    /// Nenhuma das duas é enviada.
    Neutro,
}

impl ModoSocd {
    pub const TODOS: [ModoSocd; 4] =
        [ModoSocd::UltimaVence, ModoSocd::PrioridadeA, ModoSocd::PrioridadeB, ModoSocd::Neutro];

    /// Regra gravada em cada tecla do par: (primeira, segunda).
    fn regras(self) -> (u8, u8) {
        match self {
            ModoSocd::UltimaVence => (0, 0),
            ModoSocd::PrioridadeA => (1, 2),
            ModoSocd::PrioridadeB => (2, 1),
            ModoSocd::Neutro => (3, 3),
        }
    }

    /// Modo do par a partir da regra lida na primeira tecla.
    pub fn da_regra(regra: u8) -> Self {
        match regra {
            1 => ModoSocd::PrioridadeA,
            2 => ModoSocd::PrioridadeB,
            3 => ModoSocd::Neutro,
            _ => ModoSocd::UltimaVence,
        }
    }
}

/// De que lado do teclado fica um LED da luz lateral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lado {
    Esquerdo,
    Direito,
}

/// Onde fica o LED `i` (a partir de 0) de uma luz lateral de `total` LEDs: o lado e a altura,
/// de 0 (em cima, na linha do Esc) a 1 (embaixo, na linha do Ctrl).
///
/// Conferido no MAG82, que tem 30: os LEDs sobem pela lateral esquerda (o 1 perto do Ctrl, o 15
/// perto do Esc) e descem pela direita (o 16 perto do knob, o 30 perto da seta para a direita).
pub fn posicao_do_led_lateral(i: usize, total: usize) -> (Lado, f64) {
    let metade = total.div_ceil(2).max(1);
    if i < metade {
        (Lado::Esquerdo, 1.0 - (i as f64 + 0.5) / metade as f64)
    } else {
        (Lado::Direito, ((i - metade) as f64 + 0.5) / (total - metade).max(1) as f64)
    }
}

/// Uma área de luz, como o teclado a descreve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaLuz {
    pub indice: u8,
    /// Quantidade de efeitos (modos de 0 a `efeitos - 1`).
    pub efeitos: u8,
    pub linhas: u8,
    pub colunas: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Luz {
    pub ligada: bool,
    /// Efeito, de 0 a 20.
    pub modo: u8,
    /// 0 a 100.
    pub brilho: u8,
    /// 0 a 100.
    pub velocidade: u8,
    pub invertida: bool,
    /// Índice da cor da paleta usada pelo efeito (0 a 7).
    pub cor: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Aceita `ff8800` ou `#ff8800`.
    pub fn de_hex(texto: &str) -> Option<Self> {
        let h = texto.strip_prefix('#').unwrap_or(texto);
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Nome aproximado da cor, para quem não lê hexadecimal ("Laranja", "Azul claro").
    pub fn nome(&self) -> String {
        let (r, g, b) = (f32::from(self.0), f32::from(self.1), f32::from(self.2));
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let brilho = max / 255.0;
        let saturacao = if max == 0.0 { 0.0 } else { (max - min) / max };
        if brilho < 0.12 {
            return "Preto".into();
        }
        if saturacao < 0.12 {
            return if brilho > 0.85 { "Branco" } else { "Cinza" }.into();
        }
        let d = max - min;
        let tom = 60.0
            * if max == r {
                ((g - b) / d).rem_euclid(6.0)
            } else if max == g {
                (b - r) / d + 2.0
            } else {
                (r - g) / d + 4.0
            };
        let base = match tom {
            t if t < 15.0 => "Vermelho",
            t if t < 38.0 => "Laranja",
            t if t < 52.0 => "Âmbar",
            t if t < 70.0 => "Amarelo",
            t if t < 160.0 => "Verde",
            t if t < 200.0 => "Ciano",
            t if t < 260.0 => "Azul",
            t if t < 295.0 => "Roxo",
            t if t < 320.0 => "Magenta",
            t if t < 345.0 => "Rosa",
            _ => "Vermelho",
        };
        if brilho < 0.55 {
            format!("{base} escuro")
        } else if saturacao < 0.5 {
            format!("{base} claro")
        } else {
            base.into()
        }
    }
}

/// Paleta de 8 cores; cada uma ocupa 4 bytes (B, G, R e um byte que o teclado devolve e preservamos).
#[derive(Debug, Clone)]
pub struct Paleta {
    bruto: [u8; 32],
}

impl Paleta {
    pub fn cor(&self, i: usize) -> Rgb {
        Rgb(self.bruto[4 * i + 2], self.bruto[4 * i + 1], self.bruto[4 * i])
    }

    pub fn definir(&mut self, i: usize, c: Rgb) {
        self.bruto[4 * i..4 * i + 3].copy_from_slice(&[c.2, c.1, c.0]);
    }
}

/// Cor de cada posição: grade de 6 × 21, 4 bytes por tecla (B, G, R, marca; marca 255 = cor fixa).
#[derive(Debug, Clone)]
pub struct Grade {
    bruto: Vec<u8>,
}

impl Grade {
    const COLUNAS: usize = 21;
    const PAGINAS: usize = 9;
    const BYTES_PAGINA: usize = 15 * 4;

    fn indice(pos: Pos) -> usize {
        4 * (pos.0 as usize * Self::COLUNAS + pos.1 as usize)
    }

    /// Cor que a tecla está mostrando agora (a fixa, ou a do efeito naquele instante).
    pub fn cor(&self, pos: Pos) -> Rgb {
        let i = Self::indice(pos);
        Rgb(self.bruto[i + 2], self.bruto[i + 1], self.bruto[i])
    }

    /// Quantidade de LEDs da grade.
    pub fn leds(&self) -> usize {
        self.bruto.len() / 4
    }

    /// Como [`Grade::cor`], pelo número do LED (para a faixa lateral). Fora da grade devolve preto.
    pub fn cor_do_led(&self, led: usize) -> Rgb {
        self.bruto.get(4 * led..4 * led + 3).map_or(Rgb(0, 0, 0), |c| Rgb(c[2], c[1], c[0]))
    }

    /// Como [`Grade::fixa`], pelo número do LED.
    pub fn fixa_do_led(&self, led: usize) -> Option<Rgb> {
        (self.bruto.get(4 * led + 3) == Some(&255)).then(|| self.cor_do_led(led))
    }

    /// Como [`Grade::definir`], pelo número do LED. Fora da grade não faz nada.
    pub fn definir_led(&mut self, led: usize, cor: Option<Rgb>) {
        let Some(c) = self.bruto.get_mut(4 * led..4 * led + 4) else { return };
        match cor {
            Some(cor) => c.copy_from_slice(&[cor.2, cor.1, cor.0, 255]),
            None => c.fill(0),
        }
    }

    /// Cor fixa da tecla, ou `None` se ela segue o efeito.
    pub fn fixa(&self, pos: Pos) -> Option<Rgb> {
        let i = Self::indice(pos);
        (self.bruto[i + 3] == 255).then(|| Rgb(self.bruto[i + 2], self.bruto[i + 1], self.bruto[i]))
    }

    /// Verdadeiro se as duas grades têm as mesmas teclas fixas, nas mesmas cores. As demais
    /// posições mostram o efeito em andamento e mudam a cada leitura.
    fn mesmas_fixas(&self, outra: &Grade) -> bool {
        self.bruto.len() == outra.bruto.len()
            && self.bruto.chunks(4).zip(outra.bruto.chunks(4)).all(|(a, b)| match (a[3] == 255, b[3] == 255) {
                (true, true) => a[..3] == b[..3],
                (false, false) => true,
                _ => false,
            })
    }

    /// `Some(cor)` fixa a cor; `None` devolve a tecla ao efeito.
    pub fn definir(&mut self, pos: Pos, cor: Option<Rgb>) {
        let i = Self::indice(pos);
        match cor {
            Some(c) => self.bruto[i..i + 4].copy_from_slice(&[c.2, c.1, c.0, 255]),
            // apaga a cor junto com a marca: no modo "só as cores fixas" o teclado volta a marcar
            // como fixa qualquer posição gravada com cor
            None => self.bruto[i..i + 4].fill(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometria {
    pub x: f64,
    pub y: f64,
    pub largura: f64,
    /// Código de formato informado pelo teclado (ver [`FORMATO_ENTER`] e [`FORMATO_KNOB`]).
    pub formato: u8,
}

pub const FORMATO_ENTER: u8 = 11;
pub const FORMATO_KNOB: u8 = 12;

fn largura_formato(formato: u8) -> f64 {
    match formato {
        2 => 1.25,
        3 | 11 => 1.5,
        4 => 1.75,
        5 => 2.0,
        6 => 2.25,
        7 => 2.75,
        8 => 3.0,
        9 => 6.25,
        10 => 7.0,
        _ => 1.0,
    }
}

/// Valores de atuação de uma tecla, todos em milésimos de milímetro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Campo {
    Ponto,
    Soltar,
    RtInicio,
    RtPress,
    RtSolta,
    ZonaTopo,
    ZonaFundo,
}

impl Campo {
    pub const TODOS: [Campo; 7] =
        [Campo::Ponto, Campo::Soltar, Campo::RtInicio, Campo::RtPress, Campo::RtSolta, Campo::ZonaTopo, Campo::ZonaFundo];

    /// Posição do valor dentro do corpo do pacote.
    fn deslocamento(self) -> usize {
        1 + 2 * Self::TODOS.iter().position(|c| *c == self).unwrap()
    }

    pub fn minimo(self) -> u16 {
        match self {
            Campo::Ponto | Campo::Soltar => 100,
            Campo::RtInicio | Campo::RtPress | Campo::RtSolta => 10,
            Campo::ZonaTopo | Campo::ZonaFundo => 0,
        }
    }

    pub fn titulo(self) -> &'static str {
        match self {
            Campo::Ponto => "Ponto de atuação",
            Campo::Soltar => "Ponto de soltura",
            Campo::RtInicio => "Início do Rapid Trigger",
            Campo::RtPress => "Rapid Trigger ao pressionar",
            Campo::RtSolta => "Rapid Trigger ao soltar",
            Campo::ZonaTopo => "Zona morta no início do curso",
            Campo::ZonaFundo => "Zona morta no fim do curso",
        }
    }
}

/// Configuração de atuação de uma tecla. Guarda o corpo do pacote inteiro para preservar
/// os campos que não editamos (tipo de switch e coeficientes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atuacao {
    corpo: [u8; 23],
}

impl Atuacao {
    /// Verdadeiro se o teclado marca a tecla como calibrada. Durante a calibração, a marca
    /// aparece quando a tecla é pressionada até o fundo.
    pub fn calibrada(&self) -> bool {
        self.corpo[16] != 0
    }

    /// Confere as faixas de cada campo e se as zonas mortas deixam algum curso útil.
    /// O firmware aceita qualquer valor, inclusive zonas mortas que cobrem o curso inteiro.
    pub fn validar(&self) -> Resultado<()> {
        for campo in Campo::TODOS {
            let v = self.mm(campo);
            if v < campo.minimo() || v > CURSO_MAX {
                return Err(Erro::Invalido(format!(
                    "{}: use um valor entre {:.2} e {:.1} mm",
                    campo.titulo(),
                    f64::from(campo.minimo()) / 1000.0,
                    f64::from(CURSO_MAX) / 1000.0
                )));
            }
        }
        let zonas = u32::from(self.mm(Campo::ZonaTopo)) + u32::from(self.mm(Campo::ZonaFundo));
        if zonas + u32::from(CURSO_UTIL_MIN) > u32::from(CURSO_MAX) {
            return Err(Erro::Invalido(format!(
                "as zonas mortas somam {:.2} mm e não deixam curso útil: juntas, use até {:.2} mm",
                f64::from(zonas) / 1000.0,
                f64::from(CURSO_MAX - CURSO_UTIL_MIN) / 1000.0
            )));
        }
        Ok(())
    }

    pub fn rapid_trigger(&self) -> bool {
        self.corpo[0] == 1
    }

    pub fn definir_rapid_trigger(&mut self, ligado: bool) {
        self.corpo[0] = ligado as u8;
    }

    /// Valor do campo em milésimos de milímetro.
    pub fn mm(&self, campo: Campo) -> u16 {
        u16le(&self.corpo, campo.deslocamento())
    }

    pub fn definir_mm(&mut self, campo: Campo, valor: u16) {
        let i = campo.deslocamento();
        self.corpo[i..i + 2].copy_from_slice(&valor.to_le_bytes());
    }
}

// ---------- nomes ----------

/// Nome de um perfil para exibição: os nomes de fábrica vêm em chinês ("配置一"), então viram "Perfil N".
pub fn rotulo_perfil(indice: u8, nome: &str) -> String {
    if nome.is_empty() || nome.starts_with("配置") {
        format!("Perfil {}", indice + 1)
    } else {
        nome.to_string()
    }
}

/// Nomes oficiais dos efeitos de luz, na ordem dos modos (0 = Estático).
/// Conferido no teclado: o modo 0 é fixo e os modos 16 a 19 só acendem ao pressionar.
const NOMES_EFEITOS: [&str; 20] = [
    "Estático", "Onda", "Cascata", "Respiração", "Redemoinho", "Vento", "Surgir", "Pulsação", "Piscar", "Alternar",
    "Cobra", "Órbita", "Dissolver", "Mosaico", "Gotas d'água", "Fluxo", "Acionamento por tecla",
    "Acionamento por região", "Acionamento por linha", "Acionamento completo",
];

/// Nomes oficiais dos efeitos da faixa lateral. Conferido no teclado: o modo 0 é fixo e o 2
/// pulsa por igual (Respiração). Os modos 5 a 9 não têm nome oficial.
const NOMES_EFEITOS_LATERAL: [&str; 5] = ["Estático", "Onda", "Respiração", "Cascata", "Fluxo"];

/// Nome do efeito de luz de uma área que tem `efeitos` modos. O último modo de cada área apaga
/// o efeito e deixa acesas só as cores fixas; os outros modos sem nome oficial viram "Efeito N".
pub fn nome_efeito_de(area: u8, efeitos: u8, modo: u8) -> String {
    let nomes: &[&str] = if area == AREA_TECLAS { &NOMES_EFEITOS } else { &NOMES_EFEITOS_LATERAL };
    match nomes.get(modo as usize) {
        Some(nome) => nome.to_string(),
        None if modo + 1 == efeitos => "Só as cores fixas".to_string(),
        None => format!("Efeito {}", u16::from(modo) + 1),
    }
}

/// Nome do efeito de luz das teclas.
pub fn nome_efeito(modo: u8) -> String {
    nome_efeito_de(AREA_TECLAS, EFEITOS, modo)
}

pub fn nome_taxa(codigo: u8) -> String {
    match codigo {
        0 => "8000 Hz".into(),
        1 => "4000 Hz".into(),
        2 => "2000 Hz".into(),
        3 => "1000 Hz".into(),
        4 => "500 Hz".into(),
        5 => "250 Hz".into(),
        6 => "125 Hz".into(),
        outro => format!("código {outro}"),
    }
}

const NOMES_FIXOS: &[(u16, &str)] = &[
    (0x29, "esc"), (0x2A, "backspace"), (0x2B, "tab"), (0x28, "enter"), (0x2C, "espaco"), (0x39, "caps"),
    (0x35, "'"), (0x2D, "-"), (0x2E, "="), (0x2F, "´"), (0x30, "["), (0x31, "]"), (0x33, "ç"), (0x34, "~"),
    (0x36, ","), (0x37, "."), (0x38, ";"), (0x64, "\\"), (0x87, "/"),
    (0x49, "insert"), (0x4A, "home"), (0x4B, "pgup"), (0x4C, "del"), (0x4D, "end"), (0x4E, "pgdn"),
    (0x4F, "direita"), (0x50, "esquerda"), (0x51, "baixo"), (0x52, "cima"), (0x46, "prtsc"),
    (0x47, "scrlk"), (0x48, "pause"), (0x65, "menu"),
    (0xE0, "lctrl"), (0xE1, "lshift"), (0xE2, "lalt"), (0xE3, "win"),
    (0xE4, "rctrl"), (0xE5, "rshift"), (0xE6, "ralt"), (0xF101, "fn"),
];

/// Todas as funções de tecla conhecidas: (usage HID, nome), com as legendas do layout ABNT2.
pub fn funcoes() -> Vec<(u16, String)> {
    let mut lista: Vec<(u16, String)> = NOMES_FIXOS.iter().map(|(c, n)| (*c, n.to_string())).collect();
    lista.extend((0..26u16).map(|i| (0x04 + i, char::from(b'a' + i as u8).to_string())));
    lista.extend((0..10u16).map(|i| (0x1E + i, ((i + 1) % 10).to_string())));
    lista.extend((0..12u16).map(|i| (0x3A + i, format!("f{}", i + 1))));
    lista
}

pub fn nome_tecla(codigo: u16) -> String {
    funcoes().into_iter().find(|(c, _)| *c == codigo).map(|(_, n)| n).unwrap_or_else(|| format!("0x{codigo:04x}"))
}

/// Família de uma função especial, para agrupar nas listas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Familia {
    Multimidia,
    LuzTeclas,
    LuzLateral,
    Teclado,
    /// Muda o teclado inteiro (restaurar, calibrar, trocar de camada): aparece quando já está
    /// num atalho, mas não é oferecida para criar atalhos novos.
    Reservada,
}

/// Função que não é uma tecla comum: multimídia, controle das luzes, perfis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Especial {
    pub codigo: u16,
    pub nome: &'static str,
    /// Versão que cabe na legenda de uma tecla.
    pub curto: &'static str,
    pub familia: Familia,
}

const fn esp(codigo: u16, nome: &'static str, curto: &'static str, familia: Familia) -> Especial {
    Especial { codigo, nome, curto, familia }
}

pub const ESPECIAIS: &[Especial] = &[
    esp(0x0000, "Tecla desativada", "—", Familia::Teclado),
    esp(0x206F, "Aumentar o brilho da tela", "Tela+", Familia::Multimidia),
    esp(0x2070, "Diminuir o brilho da tela", "Tela−", Familia::Multimidia),
    esp(0x20B5, "Próxima faixa", "Próx.", Familia::Multimidia),
    esp(0x20B6, "Faixa anterior", "Ant.", Familia::Multimidia),
    esp(0x20B7, "Parar a música", "Parar", Familia::Multimidia),
    esp(0x20CD, "Tocar ou pausar", "Play", Familia::Multimidia),
    esp(0x20E2, "Mudo", "Mudo", Familia::Multimidia),
    esp(0x20E9, "Aumentar o volume", "Vol+", Familia::Multimidia),
    esp(0x20EA, "Diminuir o volume", "Vol−", Familia::Multimidia),
    esp(0x2183, "Abrir o player de música", "Música", Familia::Multimidia),
    esp(0x218A, "Abrir o e-mail", "E-mail", Familia::Multimidia),
    esp(0x2192, "Abrir a calculadora", "Calc.", Familia::Multimidia),
    esp(0x2194, "Abrir os arquivos", "Arquiv.", Familia::Multimidia),
    esp(0x2221, "Pesquisar", "Busca", Familia::Multimidia),
    esp(0x2223, "Abrir o navegador", "Web", Familia::Multimidia),
    esp(0x2224, "Voltar (navegador)", "Voltar", Familia::Multimidia),
    esp(0xF300, "Luz das teclas: próximo efeito", "Efeito", Familia::LuzTeclas),
    esp(0xF301, "Luz das teclas: próxima cor", "Cor", Familia::LuzTeclas),
    esp(0xF302, "Luz das teclas: mais brilho", "Luz+", Familia::LuzTeclas),
    esp(0xF303, "Luz das teclas: menos brilho", "Luz−", Familia::LuzTeclas),
    esp(0xF304, "Luz das teclas: efeito mais rápido", "Vel+", Familia::LuzTeclas),
    esp(0xF305, "Luz das teclas: efeito mais lento", "Vel−", Familia::LuzTeclas),
    esp(0xF306, "Luz das teclas: efeito anterior", "Ef.ant", Familia::LuzTeclas),
    esp(0xF307, "Luz das teclas: ligar ou desligar", "Luz", Familia::LuzTeclas),
    esp(0xF308, "Luz das teclas: inverter o sentido", "Sentido", Familia::LuzTeclas),
    esp(0xF310, "Luz lateral: próximo efeito", "Lat.ef", Familia::LuzLateral),
    esp(0xF311, "Luz lateral: próxima cor", "Lat.cor", Familia::LuzLateral),
    esp(0xF312, "Luz lateral: mais brilho", "Lat+", Familia::LuzLateral),
    esp(0xF313, "Luz lateral: menos brilho", "Lat−", Familia::LuzLateral),
    esp(0xF314, "Luz lateral: efeito mais rápido", "Lat.v+", Familia::LuzLateral),
    esp(0xF315, "Luz lateral: efeito mais lento", "Lat.v−", Familia::LuzLateral),
    esp(0xF316, "Luz lateral: efeito anterior", "Lat.ant", Familia::LuzLateral),
    esp(0xF317, "Luz lateral: ligar ou desligar", "Lateral", Familia::LuzLateral),
    esp(0xF318, "Luz lateral: inverter o sentido", "Lat.sen", Familia::LuzLateral),
    esp(0xF205, "Mudar para o perfil 1", "Perf.1", Familia::Teclado),
    esp(0xF206, "Mudar para o perfil 2", "Perf.2", Familia::Teclado),
    esp(0xF207, "Mudar para o perfil 3", "Perf.3", Familia::Teclado),
    esp(0xF208, "Mudar para o perfil 4", "Perf.4", Familia::Teclado),
    esp(0xF209, "Bloquear a tecla Super (Windows)", "Trava", Familia::Teclado),
    esp(0xFF00, "Alternar entre janelas (Alt + Tab)", "Alt+Tab", Familia::Teclado),
    esp(0xF200, "Restaurar o teclado de fábrica (segurando a tecla)", "Reset", Familia::Reservada),
    esp(0xF201, "Mudar para o layout Windows (segurando a tecla)", "Win", Familia::Reservada),
    esp(0xF202, "Mudar para o layout Mac (segurando a tecla)", "Mac", Familia::Reservada),
    esp(0xF203, "Calibrar as teclas (segurando a tecla)", "Calibr.", Familia::Reservada),
    // Vêm de fábrica em Fn+W e Fn+Ctrl. Conferido com o teclado (firmware 0.0.2.0): não enviam
    // tecla ao computador nem mudam nada que se consiga ler do teclado.
    esp(0xF20B, "Função de fábrica sem efeito conhecido", "?", Familia::Reservada),
    esp(0xFF01, "Função de fábrica sem efeito conhecido", "?", Familia::Reservada),
    esp(0xF100, "Voltar para a camada principal", "Cam.0", Familia::Reservada),
    esp(0xF101, "Tecla Fn", "Fn", Familia::Reservada),
    esp(0xF102, "Mudar para a 2ª camada", "Cam.2", Familia::Reservada),
    esp(0xF103, "Mudar para a 3ª camada", "Cam.3", Familia::Reservada),
];

/// A função especial com este código, se houver.
pub fn especial(codigo: u16) -> Option<&'static Especial> {
    ESPECIAIS.iter().find(|e| e.codigo == codigo)
}

/// Decompõe um código de "tecla com modificadores" (ex.: Super+D): devolve os nomes dos
/// modificadores e o código da tecla. O byte alto traz os modificadores (1 Ctrl, 2 Shift, 4 Alt,
/// 8 Super; 16 marca o lado direito) e o baixo, a tecla.
pub fn combinacao(codigo: u16) -> Option<(Vec<&'static str>, u16)> {
    let (mods, tecla) = ((codigo >> 8) as u8, codigo & 0xFF);
    if mods == 0 || mods > 0x1F || mods & 0x0F == 0 || !funcoes().iter().any(|(c, _)| *c == tecla) {
        return None;
    }
    let nomes = [(1, "Ctrl"), (2, "Shift"), (4, "Alt"), (8, "Super")];
    Some((nomes.iter().filter(|(bit, _)| mods & bit != 0).map(|(_, n)| *n).collect(), tecla))
}

/// Aceita um nome de tecla (`caps`, `w`) ou um código (`0x39`).
pub fn codigo_tecla(nome: &str) -> Option<u16> {
    let nome = nome.to_lowercase();
    if let Some(h) = nome.strip_prefix("0x") {
        return u16::from_str_radix(h, 16).ok();
    }
    funcoes().into_iter().find(|(_, n)| *n == nome).map(|(c, _)| c)
}

#[cfg(test)]
mod testes_dks {
    use super::Dks;

    #[test]
    fn casas_vao_e_voltam() {
        let mut d = Dks::default();
        // aciona no momento 1 e segura até o 3; a casa "segura 2-3" ocupa dois bits
        d.definir_casas(0, [true, true, true, true, true, false, false]);
        assert_eq!(d.gatilhos[0], 1 | 2 | 4 | 8 | 16 | 32);
        assert_eq!(d.casas(0), [true, true, true, true, true, false, false]);
        d.definir_casas(1, [false, false, false, false, false, false, true]);
        assert_eq!(d.gatilhos[1], 128);
        // metade dos bits da casa do meio não conta como ligada
        d.gatilhos[2] = 8;
        assert_eq!(d.casas(2), [false; 7]);
    }

    #[test]
    fn casas_em_texto_vao_e_voltam() {
        for texto in ["1", "4", "1-3", "1-4", "1-2,4", "2,3", "1,3-4", "2+", "1,1+", "1-2,2+"] {
            let casas = Dks::casas_de_texto(texto).unwrap_or_else(|| panic!("{texto}"));
            assert_eq!(Dks::casas_em_texto(casas), texto);
        }
        assert_eq!(Dks::casas_de_texto("1-3"), Some([true, true, true, true, true, false, false]));
        for errado in ["", "0", "5", "3-1", "2-2", "4+", "a", "1,,2"] {
            assert_eq!(Dks::casas_de_texto(errado), None, "{errado}");
        }
    }

    #[test]
    fn valida_o_dks() {
        let mut d = Dks::default();
        assert!(d.validar().is_err(), "sem função nenhuma");
        d.teclas[0] = 4;
        assert!(d.validar().is_err(), "função sem momento");
        d.definir_casas(0, [true, false, false, false, false, false, false]);
        assert!(d.validar().is_ok());
        d.gatilhos[1] = 1;
        assert!(d.validar().is_err(), "momento em vaga livre");
        d.gatilhos[1] = 0;
        d.inicio = 3000;
        assert!(d.validar().is_err(), "início depois do fundo");
        d.inicio = 50;
        assert!(d.validar().is_err(), "distância fora da faixa");
    }
}

#[cfg(test)]
mod testes_lateral {
    use super::{posicao_do_led_lateral, Lado};

    #[test]
    fn leds_sobem_pela_esquerda_e_descem_pela_direita() {
        let (lado, altura) = posicao_do_led_lateral(0, 30);
        assert!(lado == Lado::Esquerdo && altura > 0.9, "LED 1: embaixo, à esquerda");
        let (lado, altura) = posicao_do_led_lateral(14, 30);
        assert!(lado == Lado::Esquerdo && altura < 0.1, "LED 15: em cima, à esquerda");
        let (lado, altura) = posicao_do_led_lateral(15, 30);
        assert!(lado == Lado::Direito && altura < 0.1, "LED 16: em cima, à direita");
        let (lado, altura) = posicao_do_led_lateral(29, 30);
        assert!(lado == Lado::Direito && altura > 0.9, "LED 30: embaixo, à direita");
        // LED 7 na altura do Caps Lock e LED 24 na do PgUp: as duas no meio do teclado
        assert!((posicao_do_led_lateral(6, 30).1 - 0.57).abs() < 0.02);
        assert!((posicao_do_led_lateral(23, 30).1 - 0.57).abs() < 0.02);
    }
}

#[cfg(test)]
mod testes_cor {
    use super::Rgb;

    #[test]
    fn nome_das_cores() {
        let nome = |h: &str| Rgb::de_hex(h).unwrap().nome();
        for (hex, esperado) in [
            ("ff0000", "Vermelho"),
            ("ff8800", "Laranja"),
            ("f5c211", "Âmbar"),
            ("ffff00", "Amarelo"),
            ("00ff00", "Verde"),
            ("00ffff", "Ciano"),
            ("0000ff", "Azul"),
            ("8000ff", "Roxo"),
            ("ff00ff", "Magenta"),
            ("ff0080", "Rosa"),
            ("ff0010", "Vermelho"),
            ("ffffff", "Branco"),
            ("808080", "Cinza"),
            ("000000", "Preto"),
            ("000080", "Azul escuro"),
            ("a0c0ff", "Azul claro"),
        ] {
            assert_eq!(nome(hex), esperado, "{hex}");
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn campos_ficam_nas_posicoes_do_pacote() {
        // resposta real do teclado para a tecla (0,0): 04 01 00 00 | 00 d0 07 d0 07 f4 01 2c 01 2c 01 c8 00 c8 00 ...
        let mut corpo = [0u8; 23];
        corpo[..15].copy_from_slice(&[0, 0xd0, 7, 0xd0, 7, 0xf4, 1, 0x2c, 1, 0x2c, 1, 0xc8, 0, 0xc8, 0]);
        let mut a = Atuacao { corpo };
        assert!(!a.rapid_trigger());
        assert_eq!(a.mm(Campo::Ponto), 2000);
        assert_eq!(a.mm(Campo::RtInicio), 500);
        assert_eq!(a.mm(Campo::RtSolta), 300);
        assert_eq!(a.mm(Campo::ZonaFundo), 200);
        a.definir_mm(Campo::RtPress, 150);
        assert_eq!(&a.corpo[7..9], &[0x96, 0x00]);
    }

    #[test]
    fn comandos_perigosos_sao_bloqueados() {
        assert!(bloqueado(&[8]));
        assert!(bloqueado(&[8, 1, 2]));
        assert!(bloqueado(&[2, 6, 0])); // abre a calibração
        assert!(bloqueado(&[2, 6, 1]));
        assert!(bloqueado(&[2, 1])); // restauração de fábrica
        assert!(nunca_enviado(&[2, 1]) && nunca_enviado(&[8, 0]));
        assert!(!nunca_enviado(&[2, 6, 0])); // a calibração passa, mas só pelas funções próprias
        // os que a biblioteca usa continuam passando
        for usado in [&[2u8, 3, 0][..], &[2, 3, 2, 1], &[2, 5, 2, 0], &[2, 8, 0], &[1, 1], &[4, 1, 0, 0], &[5, 4, 0, 0]] {
            assert!(!bloqueado(usado), "{usado:?}");
        }
    }

    #[test]
    fn zonas_mortas_precisam_deixar_curso_util() {
        let mut a = Atuacao { corpo: [0u8; 23] };
        for (campo, v) in [(Campo::Ponto, 2000), (Campo::Soltar, 2000), (Campo::RtInicio, 500), (Campo::RtPress, 300), (Campo::RtSolta, 300)] {
            a.definir_mm(campo, v);
        }
        a.definir_mm(Campo::ZonaTopo, 200);
        a.definir_mm(Campo::ZonaFundo, 200);
        assert!(a.validar().is_ok());
        a.definir_mm(Campo::ZonaTopo, 3400);
        a.definir_mm(Campo::ZonaFundo, 3400);
        assert!(a.validar().is_err());
        a.definir_mm(Campo::ZonaTopo, 1650);
        a.definir_mm(Campo::ZonaFundo, 1650);
        assert!(a.validar().is_ok()); // sobra exatamente o mínimo
        a.definir_mm(Campo::ZonaFundo, 1651);
        assert!(a.validar().is_err());
        a.definir_mm(Campo::ZonaFundo, 200);
        a.definir_mm(Campo::Ponto, 50);
        assert!(a.validar().is_err());
    }

    #[test]
    fn grade_usa_21_colunas_e_ordem_bgr() {
        let mut g = Grade { bruto: vec![0; Grade::PAGINAS * Grade::BYTES_PAGINA] };
        g.definir((1, 2), Some(Rgb(0x11, 0x22, 0x33)));
        let i = 4 * (21 + 2);
        assert_eq!(&g.bruto[i..i + 4], &[0x33, 0x22, 0x11, 255]);
        assert_eq!(g.fixa((1, 2)), Some(Rgb(0x11, 0x22, 0x33)));
        let mut efeito = g.clone();
        efeito.bruto[0] = 0x77; // posição sem cor fixa: o efeito pode mostrar qualquer coisa
        assert!(g.mesmas_fixas(&efeito));
        efeito.definir((1, 2), Some(Rgb(0x11, 0x22, 0x34)));
        assert!(!g.mesmas_fixas(&efeito));
        g.definir((1, 2), None);
        assert!(!g.mesmas_fixas(&efeito));
        let mut faixa = Grade { bruto: vec![0; 4 * 30] };
        faixa.definir_led(29, Some(Rgb(1, 2, 3)));
        faixa.definir_led(30, Some(Rgb(9, 9, 9))); // fora da faixa: ignorado
        assert_eq!((faixa.leds(), faixa.fixa_do_led(29), faixa.fixa_do_led(0)), (30, Some(Rgb(1, 2, 3)), None));
        assert_eq!(faixa.cor_do_led(30), Rgb(0, 0, 0));
        assert_eq!(g.fixa((1, 2)), None);
    }

    #[test]
    fn regras_de_socd_sao_simetricas() {
        for modo in ModoSocd::TODOS {
            assert_eq!(ModoSocd::da_regra(modo.regras().0), modo);
        }
        assert_eq!(ModoSocd::PrioridadeA.regras(), (1, 2));
    }

    #[test]
    fn nomes_e_cores() {
        assert_eq!(codigo_tecla("W"), Some(0x1A));
        assert_eq!(codigo_tecla("0x39"), Some(0x39));
        assert_eq!(nome_tecla(0x27), "0");
        assert_eq!(nome_tecla(0xF101), "fn");
        assert_eq!(Rgb::de_hex("#ff8800"), Some(Rgb(0xff, 0x88, 0x00)));
        assert_eq!(Rgb::de_hex("xyz"), None);
        assert_eq!(Rgb::de_hex("+12345"), None);
        assert_eq!(Rgb::de_hex("##ff8800"), None);
        assert_eq!(especial(0x20E9).map(|e| e.curto), Some("Vol+"));
        assert_eq!(especial(0xF200).map(|e| e.familia), Some(Familia::Reservada));
        assert_eq!(combinacao(0x1807), Some((vec!["Super"], 0x07))); // Super+D
        assert_eq!(combinacao(0x004A), None); // tecla comum
        assert_eq!(combinacao(0xF300), None); // função especial
        let mut codigos: Vec<u16> = ESPECIAIS.iter().map(|e| e.codigo).collect();
        codigos.sort_unstable();
        codigos.dedup();
        assert_eq!(codigos.len(), ESPECIAIS.len(), "código repetido na tabela de funções especiais");
        assert_eq!(nome_efeito(0), "Estático");
        assert_eq!(nome_efeito(19), "Acionamento completo");
        assert_eq!(nome_efeito(20), "Só as cores fixas");
        assert_eq!(nome_efeito_de(AREA_LATERAL, 11, 2), "Respiração");
        assert_eq!(nome_efeito_de(AREA_LATERAL, 11, 7), "Efeito 8");
        assert_eq!(nome_efeito_de(AREA_LATERAL, 11, 10), "Só as cores fixas");
    }
}
