//! Rótulos e pequenos construtores de widgets usados por todas as páginas.

use adw::prelude::*;
use gtk::gdk;
use mag82::Rgb;

/// Legenda curta de uma tecla, como aparece no desenho do teclado.
pub fn rotulo(nome: &str) -> String {
    let fixo = match nome {
        "espaco" => "Espaço",
        "esquerda" => "←",
        "direita" => "→",
        "cima" => "↑",
        "baixo" => "↓",
        "backspace" => "Backspace",
        "enter" => "Enter",
        "lshift" | "rshift" => "Shift",
        "lctrl" | "rctrl" => "Ctrl",
        "lalt" => "Alt",
        "ralt" => "AltGr",
        "win" => "Super",
        "caps" => "Caps",
        "esc" => "Esc",
        "tab" => "Tab",
        "del" => "Del",
        "end" => "End",
        "fn" => "Fn",
        "insert" => "Ins",
        "pgup" => "PgUp",
        "pgdn" => "PgDn",
        "prtsc" => "PrtSc",
        "scrlk" => "ScrLk",
        "pause" => "Pause",
        "menu" => "Menu",
        _ => "",
    };
    if !fixo.is_empty() {
        fixo.to_string()
    } else if nome.chars().count() <= 3 {
        nome.to_uppercase()
    } else {
        let mut letras = nome.chars();
        letras.next().map(|c| c.to_uppercase().collect::<String>() + letras.as_str()).unwrap_or_default()
    }
}

/// Nome por extenso, para listas: distingue as teclas que têm a mesma legenda curta.
pub fn rotulo_longo(nome: &str) -> String {
    match nome {
        "lshift" => "Shift esquerdo".into(),
        "rshift" => "Shift direito".into(),
        "lctrl" => "Ctrl esquerdo".into(),
        "rctrl" => "Ctrl direito".into(),
        "lalt" => "Alt esquerdo".into(),
        "ralt" => "AltGr (Alt direito)".into(),
        "esquerda" => "Seta para a esquerda".into(),
        "direita" => "Seta para a direita".into(),
        "cima" => "Seta para cima".into(),
        "baixo" => "Seta para baixo".into(),
        "insert" => "Insert".into(),
        "del" => "Delete".into(),
        "pgup" => "Page Up".into(),
        "pgdn" => "Page Down".into(),
        "prtsc" => "Print Screen".into(),
        "caps" => "Caps Lock".into(),
        "scrlk" => "Scroll Lock".into(),
        "menu" => "Menu de contexto".into(),
        outro => rotulo(outro),
    }
}

/// O que um código de função faz, por extenso: tecla, função especial ou tecla com modificadores.
pub fn descrever_funcao(codigo: u16) -> String {
    if let Some(e) = mag82::especial(codigo) {
        return e.nome.to_string();
    }
    if let Some((modificadores, tecla)) = mag82::combinacao(codigo) {
        return format!("{} + {}", modificadores.join(" + "), rotulo(&mag82::nome_tecla(tecla)));
    }
    let nome = mag82::nome_tecla(codigo);
    if nome.starts_with("0x") {
        format!("Função do teclado sem nome ({nome})")
    } else {
        rotulo_longo(&nome)
    }
}

/// O mesmo, numa forma que cabe na legenda de uma tecla.
pub fn rotulo_funcao(codigo: u16) -> String {
    if let Some(e) = mag82::especial(codigo) {
        return e.curto.to_string();
    }
    if let Some((modificadores, tecla)) = mag82::combinacao(codigo) {
        let m: String = modificadores.iter().map(|m| if *m == "Super" { "⊞" } else { &m[..1] }).collect();
        return format!("{m}+{}", rotulo(&mag82::nome_tecla(tecla)));
    }
    let nome = mag82::nome_tecla(codigo);
    if nome.starts_with("0x") {
        "?".to_string()
    } else {
        rotulo(&nome)
    }
}

/// Milésimos de milímetro como texto: 2000 -> "2,00".
pub fn mm(valor: u16) -> String {
    format!("{:.2}", f64::from(valor) / 1000.0).replace('.', ",")
}

pub fn para_rgba(c: Rgb) -> gdk::RGBA {
    gdk::RGBA::new(f32::from(c.0) / 255.0, f32::from(c.1) / 255.0, f32::from(c.2) / 255.0, 1.0)
}

pub fn de_rgba(c: &gdk::RGBA) -> Rgb {
    let b = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb(b(c.red()), b(c.green()), b(c.blue()))
}

pub fn botao(texto: &str, destaque: bool) -> gtk::Button {
    let b = gtk::Button::builder().label(texto).valign(gtk::Align::Center).build();
    if destaque {
        b.add_css_class("suggested-action");
    }
    b
}

pub fn botao_cor() -> gtk::ColorDialogButton {
    let dialogo = gtk::ColorDialog::builder().with_alpha(false).build();
    gtk::ColorDialogButton::builder().dialog(&dialogo).valign(gtk::Align::Center).build()
}

pub fn linha_escala(titulo: &str) -> (adw::ActionRow, gtk::Scale) {
    let escala = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    escala.set_size_request(150, -1);
    escala.set_hexpand(true);
    escala.set_draw_value(true);
    escala.set_value_pos(gtk::PositionType::Right);
    escala.set_valign(gtk::Align::Center);
    let linha = adw::ActionRow::builder().title(titulo).build();
    linha.add_suffix(&escala);
    (linha, escala)
}

/// Página rolável com o conteúdo centralizado e largura limitada.
pub fn rolavel(conteudo: &impl IsA<gtk::Widget>, largura: i32) -> gtk::ScrolledWindow {
    let limite = adw::Clamp::builder().child(conteudo).maximum_size(largura).margin_top(6).margin_bottom(18).margin_start(12).margin_end(12).build();
    gtk::ScrolledWindow::builder().child(&limite).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build()
}
