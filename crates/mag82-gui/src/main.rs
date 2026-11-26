//! Interface gráfica do VOLCANOS MAG82 (GTK4 + libadwaita).
//!
//! - `fila`: thread que conversa com o teclado, para a janela nunca travar;
//! - `dados`: tudo o que foi lido do teclado;
//! - `ui`: janela, conexão e vigilância (conectar/desconectar, troca de perfil no teclado);
//! - `teclas`, `luz`, `dispositivo`: uma página cada;
//! - `atualizacao`: procura, baixa e instala versões novas do programa;
//! - `dks`: modo DKS da página Teclas;
//! - `calibracao`: diálogo de calibração das teclas;
//! - `teste`: autoteste acionado pela variável `MAG82_TESTE`.

mod atualizacao;
mod calibracao;
mod dados;
mod dispositivo;
mod dks;
mod fila;
mod luz;
mod teclas;
mod teste;
mod ui;
mod util;

use adw::prelude::*;
use gtk::glib;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id("io.github.mag82linux").build();
    app.connect_activate(|app| {
        if let Some(janela) = app.active_window() {
            janela.present();
            return;
        }
        let ui = ui::Ui::construir(app);
        ui.janela.present();
        if let Ok(prefixo) = std::env::var("MAG82_TESTE") {
            teste::iniciar(ui, prefixo);
        } else {
            ui.atualizacao_ao_abrir();
        }
    });
    app.run()
}
