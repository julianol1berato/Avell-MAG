//! Atualização do programa: consulta a última versão publicada, baixa o pacote certo para esta
//! instalação, confere o SHA-256 publicado e instala. Nada acontece sem o usuário aceitar, e a
//! consulta pode ser desligada na página Dispositivo.
//!
//! A primeira metade do arquivo não depende da interface e é coberta por testes com um servidor
//! local; a segunda liga isso à janela.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::{gio, glib};
use sha2::{Digest, Sha256};

use crate::ui::Ui;

const AGENTE: &str = concat!("mag82-linux/", env!("CARGO_PKG_VERSION"));
const VERSAO_ATUAL: &str = env!("CARGO_PKG_VERSION");
/// Nenhum pacote do projeto chega perto disto; um arquivo maior não é um pacote nosso.
const TAMANHO_MAX: u64 = 64 * 1024 * 1024;
/// Sem versão nova conhecida, a consulta automática espera este tempo desde a anterior.
const INTERVALO: u64 = 20 * 3600;

/// De onde vêm a lista de versões e os pacotes.
#[derive(Debug, Clone)]
pub struct Fonte {
    api: String,
    /// Só se baixa arquivo cujo endereço começa assim.
    downloads: String,
}

impl Fonte {
    pub fn oficial() -> Self {
        Self {
            api: "https://api.github.com/repos/julianol1berato/Avell-MAG/releases/latest".into(),
            downloads: "https://github.com/julianol1berato/Avell-MAG/releases/download/".into(),
        }
    }

    /// A oficial; em compilação de desenvolvimento, `MAG82_ATUALIZACAO_API` aponta para outra.
    fn do_ambiente() -> Self {
        if cfg!(debug_assertions) {
            if let Ok(api) = std::env::var("MAG82_ATUALIZACAO_API") {
                return Self::de_teste(&api);
            }
        }
        Self::oficial()
    }

    fn de_teste(api: &str) -> Self {
        // "http://127.0.0.1:1234/qualquer/coisa" -> "http://127.0.0.1:1234/"
        let fim = api.match_indices('/').nth(2).map_or(api.len(), |(i, _)| i);
        Self { api: api.to_string(), downloads: format!("{}/", &api[..fim]) }
    }
}

pub const PAGINA_VERSOES: &str = "https://github.com/julianol1berato/Avell-MAG/releases/latest";

#[derive(Debug, Clone, PartialEq)]
pub struct Pacote {
    pub nome: String,
    pub url: String,
    pub sha256: String,
    pub tamanho: u64,
}

#[derive(Debug, Clone)]
pub struct Versao {
    /// Sem o "v" da frente: "0.1.1".
    pub numero: String,
    pub notas: String,
    pub pagina: String,
    pub pacotes: Vec<Pacote>,
}

/// "v0.1.1" ou "0.1.1" -> [0, 1, 1]. Qualquer outra coisa não é uma versão nossa.
pub fn numero(texto: &str) -> Option<Vec<u32>> {
    let t = texto.strip_prefix('v').unwrap_or(texto);
    let partes: Option<Vec<u32>> = t.split('.').map(|p| p.parse().ok()).collect();
    partes.filter(|p| !p.is_empty() && p.len() <= 4)
}

pub fn mais_nova(candidata: &str, atual: &str) -> bool {
    match (numero(candidata), numero(atual)) {
        (Some(c), Some(a)) => c > a,
        _ => false,
    }
}

/// Como o programa foi instalado: decide qual pacote serve e como instalar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Instalacao {
    Deb,
    Rpm,
    /// Em `~/.local`, sem root.
    Local,
    /// Compilado e rodando de outro lugar, ou arquitetura sem pacote: só dá para avisar.
    Outra,
}

impl Instalacao {
    fn sufixo(self) -> Option<&'static str> {
        match self {
            Instalacao::Deb => Some("_amd64.deb"),
            Instalacao::Rpm => Some(".x86_64.rpm"),
            Instalacao::Local => Some("-x86_64.tar.gz"),
            Instalacao::Outra => None,
        }
    }

    pub fn pacote(self, versao: &Versao) -> Option<&Pacote> {
        let sufixo = self.sufixo()?;
        versao.pacotes.iter().find(|p| p.nome.ends_with(sufixo))
    }
}

fn pasta_do_usuario() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn pasta_xdg(variavel: &str, padrao: &str) -> PathBuf {
    match std::env::var_os(variavel) {
        Some(p) if Path::new(&p).is_absolute() => PathBuf::from(p),
        _ => pasta_do_usuario().join(padrao),
    }
}

/// O executável e o tipo de instalação, vistos uma vez ao abrir (depois de uma atualização o
/// caminho do processo já não aponta para o arquivo novo).
fn esta_instalacao() -> &'static (PathBuf, Instalacao) {
    static ESTA: OnceLock<(PathBuf, Instalacao)> = OnceLock::new();
    ESTA.get_or_init(|| {
        let exe = std::env::current_exe().and_then(|p| p.canonicalize()).unwrap_or_default();
        let roda = |programa: &str, args: &[&str]| {
            Command::new(programa).args(args).arg(&exe).output().map(|s| s.status.success()).unwrap_or(false)
        };
        let tipo = if std::env::consts::ARCH != "x86_64" {
            Instalacao::Outra
        } else if exe == pasta_do_usuario().join(".local/bin/mag82-gui") {
            Instalacao::Local
        } else if !exe.starts_with("/usr") {
            Instalacao::Outra
        } else if roda("dpkg-query", &["-S"]) {
            Instalacao::Deb
        } else if roda("rpm", &["-qf"]) {
            Instalacao::Rpm
        } else {
            Instalacao::Outra
        };
        (exe, tipo)
    })
}

fn agente_http() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(120)).build()
}

fn erro_de_rede(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(404, _) => "ainda não há versão publicada".into(),
        ureq::Error::Status(403 | 429, _) => "o GitHub recusou a consulta por excesso de pedidos; tente mais tarde".into(),
        ureq::Error::Status(codigo, _) => format!("o GitHub respondeu com o erro {codigo}"),
        ureq::Error::Transport(_) => "sem conexão com o GitHub; confira a internet".into(),
    }
}

/// Pergunta qual é a última versão publicada.
pub fn consultar(fonte: &Fonte) -> Result<Versao, String> {
    let resposta = agente_http()
        .get(&fonte.api)
        .set("User-Agent", AGENTE)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(erro_de_rede)?;
    let json: serde_json::Value = serde_json::from_reader(resposta.into_reader().take(1 << 20))
        .map_err(|_| "a resposta do GitHub veio em um formato inesperado".to_string())?;
    let texto = |v: &serde_json::Value, campo: &str| v[campo].as_str().unwrap_or_default().to_string();
    let etiqueta = texto(&json, "tag_name");
    if numero(&etiqueta).is_none() {
        return Err("a resposta do GitHub veio em um formato inesperado".into());
    }
    let pacotes = json["assets"]
        .as_array()
        .map(|lista| {
            lista
                .iter()
                .filter_map(|a| {
                    // sem SHA-256 publicado não há como conferir o arquivo: fica de fora
                    let sha256 = a["digest"].as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
                    let nome = texto(a, "name");
                    let nome_ok = !nome.is_empty()
                        && !nome.starts_with('.')
                        && nome.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-+".contains(&b));
                    let sha_ok = sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit());
                    (nome_ok && sha_ok).then(|| Pacote {
                        nome,
                        url: texto(a, "browser_download_url"),
                        sha256,
                        tamanho: a["size"].as_u64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let pagina = Some(texto(&json, "html_url")).filter(|p| p.starts_with("https://github.com/"));
    Ok(Versao {
        numero: etiqueta.trim_start_matches('v').to_string(),
        notas: texto(&json, "body"),
        pagina: pagina.unwrap_or_else(|| PAGINA_VERSOES.to_string()),
        pacotes,
    })
}

/// Baixa o pacote para `pasta` e só o entrega se o SHA-256 for o publicado.
pub fn baixar(fonte: &Fonte, pacote: &Pacote, pasta: &Path) -> Result<PathBuf, String> {
    if !pacote.url.starts_with(&fonte.downloads) {
        return Err("o endereço do pacote não é o do projeto".into());
    }
    if pacote.tamanho > TAMANHO_MAX {
        return Err("o pacote publicado é maior do que o esperado".into());
    }
    let falha = |e: std::io::Error| format!("não foi possível gravar o pacote baixado: {e}");
    let _ = std::fs::remove_dir_all(pasta);
    std::fs::create_dir_all(pasta).map_err(falha)?;
    let resposta = agente_http().get(&pacote.url).set("User-Agent", AGENTE).call().map_err(erro_de_rede)?;
    let destino = pasta.join(&pacote.nome);
    let mut arquivo = std::fs::File::create(&destino).map_err(falha)?;
    let mut entrada = resposta.into_reader().take(TAMANHO_MAX + 1);
    let mut soma = Sha256::new();
    let mut bloco = [0u8; 64 * 1024];
    loop {
        let lidos = entrada.read(&mut bloco).map_err(|_| "o download foi interrompido".to_string())?;
        if lidos == 0 {
            break;
        }
        soma.update(&bloco[..lidos]);
        arquivo.write_all(&bloco[..lidos]).map_err(falha)?;
    }
    arquivo.sync_all().map_err(falha)?;
    let obtido: String = soma.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if obtido != pacote.sha256 {
        let _ = std::fs::remove_file(&destino);
        return Err("o arquivo baixado não confere com o publicado (SHA-256 diferente); nada foi instalado".into());
    }
    Ok(destino)
}

#[derive(Debug, PartialEq)]
pub enum Falha {
    /// O usuário fechou o pedido de senha.
    Cancelada,
    Erro(String),
}

fn final_da_saida(saida: &[u8]) -> String {
    let texto = String::from_utf8_lossy(saida);
    let linhas: Vec<&str> = texto.lines().filter(|l| !l.trim().is_empty()).collect();
    linhas[linhas.len().saturating_sub(4)..].join("\n")
}

/// Instala um `.deb` ou `.rpm` pelo gerenciador do sistema, que pede a senha de administrador.
fn instalar_pacote(tipo: Instalacao, arquivo: &Path) -> Result<(), Falha> {
    let mut comando = Command::new("pkexec");
    match tipo {
        Instalacao::Deb => comando.args(["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y"]),
        _ => comando.args(["dnf", "install", "-y"]),
    };
    let saida = comando.arg(arquivo).output().map_err(|_| {
        Falha::Erro("o sistema não tem o pkexec, usado para pedir a senha de administrador".into())
    })?;
    match saida.status.code() {
        Some(0) => Ok(()),
        Some(126) => Err(Falha::Cancelada),
        Some(127) => Err(Falha::Erro("a senha de administrador não foi aceita".into())),
        _ => Err(Falha::Erro(format!("o instalador do sistema falhou:\n{}", final_da_saida(&saida.stderr)))),
    }
}

/// Troca um arquivo de uma vez: grava ao lado e renomeia, para nunca deixar um pela metade.
fn trocar_arquivo(origem: &Path, destino: &Path, executavel: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(pasta) = destino.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    let provisorio = destino.with_extension("novo");
    std::fs::copy(origem, &provisorio)?;
    std::fs::set_permissions(&provisorio, std::fs::Permissions::from_mode(if executavel { 0o755 } else { 0o644 }))?;
    std::fs::rename(&provisorio, destino)
}

/// Atualiza a instalação em `~/.local` a partir do `.tar.gz` da versão. `prefixo` é `~/.local`
/// e `dados` é a pasta de dados do usuário (`~/.local/share`).
pub fn instalar_local(arquivo: &Path, numero_esperado: &str, prefixo: &Path, dados: &Path) -> Result<(), Falha> {
    const ID: &str = "io.github.mag82linux";
    let erro = |texto: &str| Falha::Erro(texto.to_string());
    let pasta = arquivo.parent().ok_or_else(|| erro("pacote sem pasta"))?.join("extraido");
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).map_err(|e| Falha::Erro(e.to_string()))?;
    let extraiu = Command::new("tar")
        .args(["--no-same-owner", "-xzf"])
        .arg(arquivo)
        .arg("-C")
        .arg(&pasta)
        .output()
        .map_err(|_| erro("o sistema não tem o comando tar"))?;
    if !extraiu.status.success() {
        return Err(Falha::Erro(format!("não foi possível abrir o pacote:\n{}", final_da_saida(&extraiu.stderr))));
    }
    let raiz = pasta.join("mag82");
    let arquivos = [
        ("bin/mag82".to_string(), prefixo.join("bin/mag82"), true),
        ("bin/mag82-gui".to_string(), prefixo.join("bin/mag82-gui"), true),
        (
            format!("share/icons/hicolor/scalable/apps/{ID}.svg"),
            dados.join(format!("icons/hicolor/scalable/apps/{ID}.svg")),
            false,
        ),
        (format!("share/metainfo/{ID}.metainfo.xml"), dados.join(format!("metainfo/{ID}.metainfo.xml")), false),
        (format!("share/applications/{ID}.desktop"), dados.join(format!("applications/{ID}.desktop")), false),
    ];
    for (relativo, _, _) in &arquivos {
        let comum = std::fs::symlink_metadata(raiz.join(relativo)).map(|m| m.is_file()).unwrap_or(false);
        if !comum {
            return Err(Falha::Erro(format!("o pacote está incompleto: falta {relativo}")));
        }
    }
    // antes de trocar qualquer coisa: o programa novo roda neste sistema e é a versão anunciada?
    let conferiu = Command::new(raiz.join("bin/mag82")).arg("--version").output();
    let roda = conferiu
        .map(|s| s.status.success() && String::from_utf8_lossy(&s.stdout).trim() == format!("mag82 {numero_esperado}"))
        .unwrap_or(false);
    if !roda {
        return Err(erro("a versão nova não roda neste sistema; a instalação atual foi mantida"));
    }
    // o atalho do menu aponta para o caminho completo, como faz o script de instalação
    let atalho = raiz.join(&arquivos[4].0);
    let texto = std::fs::read_to_string(&atalho).map_err(|e| Falha::Erro(e.to_string()))?;
    let exec = format!("Exec={}", prefixo.join("bin/mag82-gui").display());
    std::fs::write(&atalho, texto.replace("Exec=mag82-gui", &exec)).map_err(|e| Falha::Erro(e.to_string()))?;
    // os executáveis vão por último: se um arquivo de dados falhar, o programa continua o antigo
    for (relativo, destino, executavel) in arquivos.iter().rev() {
        trocar_arquivo(&raiz.join(relativo), destino, *executavel)
            .map_err(|e| Falha::Erro(format!("não foi possível gravar {}: {e}", destino.display())))?;
    }
    let _ = Command::new("update-desktop-database").arg(dados.join("applications")).output();
    let _ = std::fs::remove_dir_all(&pasta);
    Ok(())
}

/// O que o programa guarda entre uma execução e outra, em `~/.config/mag82/config`.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Procurar versão nova ao abrir.
    pub atualizacoes: bool,
    /// Versão que o usuário pediu para não oferecer mais.
    pub pular: String,
    /// Quando foi a última consulta que deu certo (segundos desde 1970).
    pub verificado: u64,
    /// Versão nova já vista e ainda não instalada.
    pub disponivel: String,
}

impl Default for Config {
    fn default() -> Self {
        Self { atualizacoes: true, pular: String::new(), verificado: 0, disponivel: String::new() }
    }
}

impl Config {
    fn arquivo() -> PathBuf {
        pasta_xdg("XDG_CONFIG_HOME", ".config").join("mag82/config")
    }

    pub fn de_texto(texto: &str) -> Self {
        let mut c = Self::default();
        for linha in texto.lines() {
            let Some((chave, valor)) = linha.split_once('=') else { continue };
            match chave.trim() {
                "atualizacoes" => c.atualizacoes = valor.trim() != "nao",
                "pular" => c.pular = valor.trim().to_string(),
                "verificado" => c.verificado = valor.trim().parse().unwrap_or(0),
                "disponivel" => c.disponivel = valor.trim().to_string(),
                _ => {}
            }
        }
        c
    }

    pub fn em_texto(&self) -> String {
        format!(
            "atualizacoes={}\npular={}\nverificado={}\ndisponivel={}\n",
            if self.atualizacoes { "sim" } else { "nao" },
            self.pular,
            self.verificado,
            self.disponivel
        )
    }

    pub fn ler() -> Self {
        Self::de_texto(&std::fs::read_to_string(Self::arquivo()).unwrap_or_default())
    }

    pub fn gravar(&self) {
        let arquivo = Self::arquivo();
        if let Some(pasta) = arquivo.parent() {
            let _ = std::fs::create_dir_all(pasta);
        }
        let _ = std::fs::write(arquivo, self.em_texto());
    }

    /// A consulta automática roda agora? Com versão nova já vista, sempre (é preciso a lista de
    /// pacotes para oferecê-la); sem, no máximo uma vez por dia.
    pub fn deve_consultar(&self, agora: u64) -> bool {
        self.atualizacoes && (!self.disponivel.is_empty() || agora.saturating_sub(self.verificado) >= INTERVALO)
    }
}

fn agora() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// As notas da versão em texto corrido, sem as marcas de formatação, e em tamanho de diálogo.
pub fn resumir_notas(notas: &str) -> String {
    let limpo: String = notas.chars().filter(|c| !matches!(c, '#' | '*' | '`' | '\r')).collect();
    let linhas: Vec<&str> = limpo.lines().map(str::trim).collect();
    let mut texto = linhas.join("\n");
    while texto.contains("\n\n\n") {
        texto = texto.replace("\n\n\n", "\n\n");
    }
    let texto = texto.trim();
    match texto.char_indices().nth(600) {
        Some((corte, _)) => format!("{}…", texto[..corte].trim_end()),
        None => texto.to_string(),
    }
}

// ---------- interface ----------

/// Em que ponto a atualização está; decide o que o botão da faixa faz.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Fase {
    #[default]
    Nada,
    Disponivel,
    Trabalhando,
    Instalada,
}

pub fn ligar(ui: &Rc<Ui>) {
    let ui2 = ui.clone();
    ui.faixa_atualizacao.connect_button_clicked(move |_| match ui2.fase_atualizacao.get() {
        Fase::Disponivel => ui2.oferecer_atualizacao(),
        Fase::Instalada => ui2.reiniciar(),
        _ => {}
    });
    ui.disp.atualizar_auto.set_active(Config::ler().atualizacoes);
    ui.disp.atualizar_auto.connect_active_notify(|linha| {
        let mut config = Config::ler();
        config.atualizacoes = linha.is_active();
        config.gravar();
    });
    let ui2 = ui.clone();
    ui.disp.procurar_atualizacao.connect_activated(move |_| ui2.verificar_atualizacao(true));
}

impl Ui {
    /// Chamado ao abrir o programa: consulta só se o usuário deixou e se já passou o intervalo.
    pub fn atualizacao_ao_abrir(self: &Rc<Self>) {
        if Config::ler().deve_consultar(agora()) {
            self.verificar_atualizacao(false);
        }
    }

    /// `manual`: o usuário pediu, então ele também ouve "já está atualizado" e os erros.
    pub fn verificar_atualizacao(self: &Rc<Self>, manual: bool) {
        if self.fase_atualizacao.get() == Fase::Trabalhando || self.fase_atualizacao.get() == Fase::Instalada {
            return;
        }
        let ui = self.clone();
        glib::spawn_future_local(async move {
            if manual {
                ui.avisar("Procurando atualizações…");
            }
            let fonte = Fonte::do_ambiente();
            let resultado = gio::spawn_blocking(move || consultar(&fonte)).await;
            let versao = match resultado {
                Ok(Ok(versao)) => versao,
                Ok(Err(erro)) => {
                    if manual {
                        ui.avisar_erro(&format!("Não foi possível procurar atualizações: {erro}"));
                    }
                    return;
                }
                Err(_) => return,
            };
            let mut config = Config::ler();
            config.verificado = agora();
            let nova = mais_nova(&versao.numero, VERSAO_ATUAL);
            config.disponivel = if nova { versao.numero.clone() } else { String::new() };
            config.gravar();
            if !nova {
                if manual {
                    ui.avisar(&format!("Você já tem a versão mais recente ({VERSAO_ATUAL})"));
                }
                return;
            }
            if !manual && config.pular == versao.numero {
                return;
            }
            ui.faixa_atualizacao.set_title(&format!("Versão {} disponível", versao.numero));
            ui.faixa_atualizacao.set_button_label(Some("Ver novidades"));
            ui.faixa_atualizacao.set_revealed(true);
            ui.fase_atualizacao.set(Fase::Disponivel);
            *ui.versao_nova.borrow_mut() = Some(versao);
            // só em compilação de desenvolvimento: abre o diálogo ou aceita sozinho, para testar
            let ensaio = if cfg!(debug_assertions) { std::env::var("MAG82_ATUALIZACAO_ENSAIO").ok() } else { None };
            if ensaio.as_deref().is_some_and(|e| e.starts_with("aceitar")) {
                let (versao, tipo) = (ui.versao_nova.borrow().clone(), esta_instalacao().1);
                if let Some((versao, pacote)) = versao.and_then(|v| tipo.pacote(&v).cloned().map(|p| (v, p))) {
                    ui.atualizar(versao, pacote);
                }
            } else if manual || ensaio.is_some() {
                ui.oferecer_atualizacao();
            }
        });
    }

    /// O diálogo com as novidades e a escolha: atualizar, deixar para depois ou pular a versão.
    fn oferecer_atualizacao(self: &Rc<Self>) {
        let Some(versao) = self.versao_nova.borrow().clone() else { return };
        let tipo = esta_instalacao().1;
        let pacote = tipo.pacote(&versao).cloned();
        let como = match (tipo, &pacote) {
            (Instalacao::Deb | Instalacao::Rpm, Some(_)) => {
                "O programa baixa o pacote, confere e chama o instalador do sistema, que pede a senha de administrador."
            }
            (Instalacao::Local, Some(_)) => {
                "O programa baixa a versão nova, confere e atualiza a instalação em ~/.local, sem pedir senha."
            }
            _ => "Esta instalação não pode ser atualizada pelo programa; baixe a versão nova na página do projeto.",
        };
        let notas = resumir_notas(&versao.notas);
        let corpo = if notas.is_empty() { como.to_string() } else { format!("{notas}\n\n{como}") };
        let dialogo = adw::AlertDialog::builder()
            .heading(format!("Versão {} disponível", versao.numero))
            .body(corpo)
            .close_response("depois")
            .build();
        dialogo.add_responses(&[("depois", "Agora não"), ("pular", "Pular esta versão")]);
        if pacote.is_some() {
            dialogo.add_response("atualizar", "Atualizar");
            dialogo.set_response_appearance("atualizar", adw::ResponseAppearance::Suggested);
            dialogo.set_default_response(Some("atualizar"));
        } else {
            dialogo.add_response("pagina", "Abrir a página");
            dialogo.set_default_response(Some("pagina"));
        }
        let ui = self.clone();
        dialogo.connect_response(None, move |_, resposta| match resposta {
            "atualizar" => {
                if let Some(pacote) = pacote.clone() {
                    ui.atualizar(versao.clone(), pacote);
                }
            }
            "pagina" => ui.abrir_pagina(&versao.pagina),
            "pular" => {
                let mut config = Config::ler();
                config.pular = versao.numero.clone();
                config.gravar();
                ui.faixa_atualizacao.set_revealed(false);
                ui.fase_atualizacao.set(Fase::Nada);
            }
            _ => {}
        });
        dialogo.present(Some(&self.janela));
    }

    fn abrir_pagina(&self, endereco: &str) {
        gtk::UriLauncher::new(endereco).launch(Some(&self.janela), gio::Cancellable::NONE, |_| {});
    }

    fn atualizar(self: &Rc<Self>, versao: Versao, pacote: Pacote) {
        let (_, tipo) = *esta_instalacao();
        self.fase_atualizacao.set(Fase::Trabalhando);
        self.faixa_atualizacao.set_button_label(None);
        self.faixa_atualizacao.set_title(&format!("Baixando a versão {}…", versao.numero));
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let fonte = Fonte::do_ambiente();
            let pasta = pasta_xdg("XDG_CACHE_HOME", ".cache").join("mag82/atualizacao");
            let pasta2 = pasta.clone();
            let baixado = gio::spawn_blocking(move || baixar(&fonte, &pacote, &pasta2)).await;
            let resultado = match baixado {
                Ok(Ok(arquivo)) => {
                    ui.faixa_atualizacao.set_title(&format!("Instalando a versão {}…", versao.numero));
                    let numero = versao.numero.clone();
                    let instalado = gio::spawn_blocking(move || match tipo {
                        Instalacao::Local => instalar_local(
                            &arquivo,
                            &numero,
                            &pasta_do_usuario().join(".local"),
                            &pasta_xdg("XDG_DATA_HOME", ".local/share"),
                        ),
                        _ => instalar_pacote(tipo, &arquivo),
                    })
                    .await;
                    instalado.unwrap_or_else(|_| Err(Falha::Erro("a instalação parou de forma inesperada".into())))
                }
                Ok(Err(erro)) => Err(Falha::Erro(erro)),
                Err(_) => Err(Falha::Erro("o download parou de forma inesperada".into())),
            };
            let _ = std::fs::remove_dir_all(&pasta);
            match resultado {
                Ok(()) => {
                    let mut config = Config::ler();
                    config.disponivel.clear();
                    config.gravar();
                    ui.fase_atualizacao.set(Fase::Instalada);
                    ui.faixa_atualizacao.set_title(&format!(
                        "Versão {} instalada. Reinicie o programa para usar.",
                        versao.numero
                    ));
                    ui.faixa_atualizacao.set_button_label(Some("Reiniciar"));
                    if cfg!(debug_assertions) && std::env::var("MAG82_ATUALIZACAO_ENSAIO").as_deref() == Ok("aceitar-reiniciar") {
                        ui.reiniciar();
                    }
                }
                Err(falha) => {
                    ui.fase_atualizacao.set(Fase::Disponivel);
                    ui.faixa_atualizacao.set_title(&format!("Versão {} disponível", versao.numero));
                    ui.faixa_atualizacao.set_button_label(Some("Ver novidades"));
                    match falha {
                        Falha::Cancelada => ui.avisar("Atualização cancelada"),
                        Falha::Erro(erro) => ui.atualizacao_falhou(&erro, &versao.pagina),
                    }
                }
            }
        });
    }

    fn atualizacao_falhou(self: &Rc<Self>, erro: &str, pagina: &str) {
        let dialogo = adw::AlertDialog::builder()
            .heading("Não foi possível atualizar")
            .body(format!("{erro}\n\nO programa continua na versão {VERSAO_ATUAL}."))
            .close_response("fechar")
            .build();
        dialogo.add_responses(&[("fechar", "Fechar"), ("pagina", "Abrir a página de versões")]);
        let ui = self.clone();
        let pagina = pagina.to_string();
        dialogo.connect_response(Some("pagina"), move |_, _| ui.abrir_pagina(&pagina));
        dialogo.present(Some(&self.janela));
    }

    /// Fecha o programa e abre o que acabou de ser instalado, assim que este terminar de sair.
    fn reiniciar(self: &Rc<Self>) {
        use std::os::unix::process::CommandExt;
        let exe = &esta_instalacao().0;
        let espera = format!("while kill -0 {} 2>/dev/null; do sleep 0.2; done; exec \"$0\"", std::process::id());
        let _ = Command::new("sh").arg("-c").arg(espera).arg(exe).process_group(0).spawn();
        self.encerrar_programa();
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn pasta_temporaria() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let pasta = std::env::temp_dir().join(format!(
            "mag82-teste-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    /// Servidor HTTP mínimo: `/ultima` devolve `json`, qualquer outro caminho devolve `pacote`.
    fn servir(json: impl Fn(&str) -> String + Send + 'static, pacote: Vec<u8>) -> String {
        let escuta = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", escuta.local_addr().unwrap());
        let base2 = base.clone();
        std::thread::spawn(move || {
            for conexao in escuta.incoming() {
                let Ok(mut conexao) = conexao else { continue };
                let mut leitor = BufReader::new(conexao.try_clone().unwrap());
                let mut pedido = String::new();
                let _ = leitor.read_line(&mut pedido);
                let mut linha = String::new();
                while leitor.read_line(&mut linha).map(|n| n > 2).unwrap_or(false) {
                    linha.clear();
                }
                let corpo = if pedido.contains(" /ultima ") { json(&base2).into_bytes() } else { pacote.clone() };
                let _ = write!(conexao, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", corpo.len());
                let _ = conexao.write_all(&corpo);
            }
        });
        base
    }

    fn sha256(dados: &[u8]) -> String {
        Sha256::digest(dados).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Um `.tar.gz` no formato das versões, com um `mag82` de mentira que responde à `--version`.
    fn pacote_local(versao: &str) -> Vec<u8> {
        use std::os::unix::fs::PermissionsExt;
        let pasta = pasta_temporaria();
        let raiz = pasta.join("mag82");
        let id = "io.github.mag82linux";
        for (caminho, conteudo) in [
            ("bin/mag82".to_string(), format!("#!/bin/sh\necho mag82 {versao}\n")),
            ("bin/mag82-gui".to_string(), format!("#!/bin/sh\necho interface {versao}\n")),
            (format!("share/icons/hicolor/scalable/apps/{id}.svg"), "<svg/>".to_string()),
            (format!("share/metainfo/{id}.metainfo.xml"), "<component/>".to_string()),
            (format!("share/applications/{id}.desktop"), "[Desktop Entry]\nExec=mag82-gui\n".to_string()),
        ] {
            let arquivo = raiz.join(&caminho);
            std::fs::create_dir_all(arquivo.parent().unwrap()).unwrap();
            std::fs::write(&arquivo, conteudo).unwrap();
            if caminho.starts_with("bin/") {
                std::fs::set_permissions(&arquivo, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let saida = pasta.join("pacote.tar.gz");
        let ok = Command::new("tar").arg("-czf").arg(&saida).arg("-C").arg(&pasta).arg("mag82").status().unwrap();
        assert!(ok.success());
        std::fs::read(saida).unwrap()
    }

    fn resposta(base: &str, versao: &str, nome: &str, sha: &str, tamanho: usize) -> String {
        format!(
            r#"{{"tag_name":"v{versao}","body":"Novidades","html_url":"https://github.com/x/y","assets":[
            {{"name":"{nome}","size":{tamanho},"digest":"sha256:{sha}","browser_download_url":"{base}/baixar/{nome}"}},
            {{"name":"sem-soma.deb","size":1,"digest":null,"browser_download_url":"{base}/baixar/sem-soma.deb"}}]}}"#
        )
    }

    #[test]
    fn compara_versoes() {
        assert_eq!(numero("v0.1.10"), Some(vec![0, 1, 10]));
        assert_eq!(numero("0.2"), Some(vec![0, 2]));
        assert_eq!(numero("v1.0-rc1"), None);
        assert_eq!(numero(""), None);
        assert!(mais_nova("0.1.10", "0.1.9"));
        assert!(mais_nova("v0.2.0", "0.1.9"));
        assert!(mais_nova("1.0", "0.9.9"));
        assert!(!mais_nova("0.1.0", "0.1.0"));
        assert!(!mais_nova("0.0.9", "0.1.0"));
        assert!(!mais_nova("lixo", "0.1.0"));
    }

    #[test]
    fn escolhe_o_pacote_da_instalacao() {
        let pacote = |nome: &str| Pacote { nome: nome.into(), url: String::new(), sha256: String::new(), tamanho: 0 };
        let versao = Versao {
            numero: "0.1.1".into(),
            notas: String::new(),
            pagina: String::new(),
            pacotes: vec![
                pacote("mag82-0.1.1-1.fc45.x86_64.rpm"),
                pacote("mag82_0.1.1_amd64.deb"),
                pacote("mag82-0.1.1-x86_64.tar.gz"),
            ],
        };
        assert_eq!(Instalacao::Deb.pacote(&versao).unwrap().nome, "mag82_0.1.1_amd64.deb");
        assert_eq!(Instalacao::Rpm.pacote(&versao).unwrap().nome, "mag82-0.1.1-1.fc45.x86_64.rpm");
        assert_eq!(Instalacao::Local.pacote(&versao).unwrap().nome, "mag82-0.1.1-x86_64.tar.gz");
        assert!(Instalacao::Outra.pacote(&versao).is_none());
    }

    #[test]
    fn config_vai_e_volta() {
        let c = Config { atualizacoes: false, pular: "0.1.1".into(), verificado: 1234, disponivel: "0.1.2".into() };
        assert_eq!(Config::de_texto(&c.em_texto()), c);
        // arquivo vazio ou estranho: vale o padrão, com a consulta ligada
        assert_eq!(Config::de_texto("lixo\n=\nverificado=abc\n"), Config::default());
        assert!(Config::default().atualizacoes);
    }

    #[test]
    fn respeita_o_interruptor_e_o_intervalo() {
        let mut c = Config::default();
        assert!(c.deve_consultar(1_000_000));
        c.verificado = 1_000_000;
        assert!(!c.deve_consultar(1_000_000 + 3600));
        assert!(c.deve_consultar(1_000_000 + INTERVALO));
        c.disponivel = "0.1.1".into();
        assert!(c.deve_consultar(1_000_000 + 60));
        c.atualizacoes = false;
        assert!(!c.deve_consultar(9_000_000));
    }

    #[test]
    fn resume_as_notas() {
        assert_eq!(resumir_notas("## Novidades\r\n\r\n\r\n* **Cores** com `nome`\n"), "Novidades\n\nCores com nome");
        let longo = resumir_notas(&"á".repeat(2000));
        assert_eq!(longo.chars().count(), 601);
    }

    #[test]
    fn consulta_baixa_confere_e_instala_em_local() {
        let pacote = pacote_local("9.9.9");
        let (sha, tamanho) = (sha256(&pacote), pacote.len());
        let base = servir(move |b| resposta(b, "9.9.9", "mag82-9.9.9-x86_64.tar.gz", &sha, tamanho), pacote);
        let fonte = Fonte::de_teste(&format!("{base}/ultima"));

        let versao = consultar(&fonte).unwrap();
        assert_eq!(versao.numero, "9.9.9");
        assert!(mais_nova(&versao.numero, VERSAO_ATUAL));
        assert_eq!(versao.pacotes.len(), 1, "pacote sem SHA-256 publicado fica de fora");
        let escolhido = Instalacao::Local.pacote(&versao).unwrap();

        let pasta = pasta_temporaria();
        let arquivo = baixar(&fonte, escolhido, &pasta.join("cache")).unwrap();
        let (prefixo, dados) = (pasta.join("local"), pasta.join("local/share"));
        std::fs::create_dir_all(prefixo.join("bin")).unwrap();
        std::fs::write(prefixo.join("bin/mag82"), "antigo").unwrap();
        instalar_local(&arquivo, "9.9.9", &prefixo, &dados).unwrap();

        let saida = Command::new(prefixo.join("bin/mag82")).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&saida.stdout).trim(), "mag82 9.9.9");
        let atalho = std::fs::read_to_string(dados.join("applications/io.github.mag82linux.desktop")).unwrap();
        assert!(atalho.contains(&format!("Exec={}", prefixo.join("bin/mag82-gui").display())), "{atalho}");
        assert!(dados.join("icons/hicolor/scalable/apps/io.github.mag82linux.svg").exists());
    }

    #[test]
    fn recusa_arquivo_que_nao_confere() {
        let pacote = pacote_local("9.9.9");
        let tamanho = pacote.len();
        let errado = "0".repeat(64);
        let base = servir(move |b| resposta(b, "9.9.9", "mag82-9.9.9-x86_64.tar.gz", &errado, tamanho), pacote);
        let fonte = Fonte::de_teste(&format!("{base}/ultima"));
        let versao = consultar(&fonte).unwrap();
        let pasta = pasta_temporaria();
        let erro = baixar(&fonte, &versao.pacotes[0], &pasta).unwrap_err();
        assert!(erro.contains("não confere"), "{erro}");
        assert!(!pasta.join("mag82-9.9.9-x86_64.tar.gz").exists(), "o arquivo recusado não pode ficar no disco");
    }

    #[test]
    fn recusa_pacote_de_outro_endereco() {
        let fonte = Fonte::oficial();
        let pacote = Pacote {
            nome: "mag82_9.9.9_amd64.deb".into(),
            url: "https://exemplo.invalid/mag82_9.9.9_amd64.deb".into(),
            sha256: "0".repeat(64),
            tamanho: 10,
        };
        let erro = baixar(&fonte, &pacote, &pasta_temporaria()).unwrap_err();
        assert!(erro.contains("não é o do projeto"), "{erro}");
    }

    #[test]
    fn nao_troca_a_instalacao_por_versao_que_nao_roda() {
        // o pacote diz ser a 9.9.9, mas o programa dentro dele responde outra versão
        let pasta = pasta_temporaria();
        std::fs::create_dir_all(pasta.join("cache")).unwrap();
        let arquivo = pasta.join("cache/pacote.tar.gz");
        std::fs::write(&arquivo, pacote_local("1.2.3")).unwrap();
        let prefixo = pasta.join("local");
        std::fs::create_dir_all(prefixo.join("bin")).unwrap();
        std::fs::write(prefixo.join("bin/mag82"), "antigo").unwrap();
        let erro = instalar_local(&arquivo, "9.9.9", &prefixo, &prefixo.join("share")).unwrap_err();
        assert!(matches!(erro, Falha::Erro(ref e) if e.contains("mantida")), "{erro:?}");
        assert_eq!(std::fs::read_to_string(prefixo.join("bin/mag82")).unwrap(), "antigo");
    }
}
