//! Interface gráfica do VOLCANOS MAG82 (GTK4 + libadwaita).
//!
//! - `fila`: thread que conversa com o teclado, para a janela nunca travar;
//! - `dados`: tudo o que foi lido do teclado;
//! - `ui`: janela, conexão e vigilância (conectar/desconectar, troca de perfil no teclado);
//! - `teclas`, `luz`, `dispositivo`: uma página cada;
//! - `atualizacao`: procura, baixa e instala versões novas do programa;
//! - `avancadas`: modo Avançada da página Teclas (DKS, multiponto, dupla função, contínuo, ao soltar);
//! - `macros`: página das macros;
//! - `sobre`: janela Sobre, com a abertura animada;
//! - `curso`: curso das teclas ao vivo, no modo Atuação;
//! - `calibracao`: diálogo de calibração das teclas;
//! - `teste`: autoteste acionado pela variável `MAG82_TESTE`.

mod atualizacao;
mod avancadas;
mod calibracao;
mod copia;
mod curso;
mod dados;
mod dispositivo;

mod fila;
mod luz;
mod macros;
mod sobre;
mod teclas;
#[cfg(feature = "autoteste")]
mod teste;
mod ui;
mod util;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    // O autoteste roda com outra identidade: assim ele abre a própria janela mesmo com o programa
    // de verdade aberto, em vez de só chamar a janela que já existe e sair.
    let em_teste = cfg!(feature = "autoteste") && std::env::var_os("MAG82_TESTE").is_some();
    let identidade = if em_teste { "io.github.mag82linux.Teste" } else { "io.github.mag82linux" };
    let app = adw::Application::builder().application_id(identidade).build();
    app.connect_activate(|app| {
        if let Some(janela) = app.active_window() {
            janela.present();
            return;
        }
        let ui = ui::Ui::construir(app);
        ui.janela.present();
        #[cfg(feature = "autoteste")]
        if let Ok(prefixo) = std::env::var("MAG82_TESTE") {
            teste::iniciar(ui, prefixo);
            return;
        }
        // com o teclado simulado o programa não procura versão nova: é ensaio, não uso
        if !mag82::simulado::ligado() {
            ui.atualizacao_ao_abrir();
        }
    });
    app.run()
}
