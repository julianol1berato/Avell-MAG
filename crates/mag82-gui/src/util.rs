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
        "rwin" => "Super",
        "numlock" => "NumLk",
        "numenter" => "N.Ent",
        "iso#" => "#",
        "power" => "Power",
        _ => "",
    };
    // teclado numérico: "N" e o que a tecla mostra ("N7", "N+")
    if fixo.is_empty() {
        if let Some(resto) = nome.strip_prefix("num") {
            return format!("N{resto}");
        }
    }
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
        "win" => "Super esquerdo".into(),
        "rwin" => "Super direito".into(),
        "numlock" => "Num Lock".into(),
        "numenter" => "Enter do teclado numérico".into(),
        "iso#" => "# (teclado ISO)".into(),
        "power" => "Power (desligar)".into(),
        "henkan" => "Henkan (teclado japonês)".into(),
        "muhenkan" => "Muhenkan (teclado japonês)".into(),
        "katakana" => "Katakana (teclado japonês)".into(),
        "hangul" => "Hangul (teclado coreano)".into(),
        "hanja" => "Hanja (teclado coreano)".into(),
        outro if outro.starts_with("num") => format!("{} do teclado numérico", &outro[3..]),
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

/// Ícone de uma função especial, para desenhar na tecla: os do tema de ícones do sistema, na
/// versão simbólica (um traço só, na cor do texto). `None` para as funções sem ícone óbvio
/// (luzes, perfis, macros) e quando o tema instalado não tem o ícone; aí vale o texto de
/// [`rotulo_funcao`].
/// O desenho de uma função, pronto para pôr na tecla, ou `None` se ela não tem um.
///
/// O ícone vem do tema Adwaita, que acompanha o GTK, e não do tema de ícones do sistema: cada
/// tema desenha (ou deixa de desenhar) esses ícones do seu jeito, e num deles a calculadora
/// saía em branco. Se o Adwaita não tiver o ícone, vale o do tema do sistema; sem nenhum, o
/// texto.
pub fn desenho_da_funcao(codigo: u16, tamanho: i32) -> Option<gtk::Image> {
    thread_local! {
        static ADWAITA: gtk::IconTheme = {
            let tema = gtk::IconTheme::new();
            tema.set_theme_name(Some("Adwaita"));
            tema
        };
    }
    let nome = icone_da_funcao(codigo)?;
    let do_adwaita = ADWAITA.with(|tema| {
        tema.has_icon(nome).then(|| {
            tema.lookup_icon(nome, &[], tamanho, 1, gtk::TextDirection::None, gtk::IconLookupFlags::FORCE_SYMBOLIC)
        })
    });
    let imagem = match do_adwaita {
        Some(desenho) => gtk::Image::from_paintable(Some(&desenho)),
        None => {
            let do_sistema = gdk::Display::default().map(|tela| gtk::IconTheme::for_display(&tela))?;
            if !do_sistema.has_icon(nome) {
                return None;
            }
            gtk::Image::from_icon_name(nome)
        }
    };
    imagem.set_pixel_size(tamanho);
    Some(imagem)
}

pub fn icone_da_funcao(codigo: u16) -> Option<&'static str> {
    let nome = match codigo {
        0x206F => "display-brightness-symbolic",
        0x2070 => "night-light-symbolic",
        0x20B5 => "media-skip-forward-symbolic",
        0x20B6 => "media-skip-backward-symbolic",
        0x20B7 => "media-playback-stop-symbolic",
        0x20CD => "media-playback-start-symbolic",
        0x20E2 => "audio-volume-muted-symbolic",
        0x20E9 => "audio-volume-high-symbolic",
        0x20EA => "audio-volume-low-symbolic",
        0x2183 => "multimedia-player-symbolic",
        0x218A => "mail-unread-symbolic",
        0x2192 => "accessories-calculator-symbolic",
        0x2194 => "system-file-manager-symbolic",
        0x2221 => "system-search-symbolic",
        0x2223 => "web-browser-symbolic",
        0x2224 => "go-previous-symbolic",
        0xF209 => "changes-prevent-symbolic",
        0xFF00 => "focus-windows-symbolic",
        0x4100..=0x4BFF => "input-mouse-symbolic",
        0x1807 => "user-desktop-symbolic",
        0x1808 => "system-file-manager-symbolic",
        0x1815 => "system-run-symbolic",
        0x1329 => "view-list-symbolic",
        _ => return None,
    };
    let tela = gtk::gdk::Display::default()?;
    gtk::IconTheme::for_display(&tela).has_icon(nome).then_some(nome)
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

/// Botão "Predefinições", que abre uma lista de exemplos prontos: (título, explicação) de cada
/// um. Devolve também a lista, para ligar o clique em cada linha.
pub fn menu_de_predefinicoes(itens: &[(&str, &str)]) -> (gtk::MenuButton, gtk::ListBox) {
    let lista = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    for (titulo, explicacao) in itens {
        lista.append(&adw::ActionRow::builder().title(*titulo).subtitle(*explicacao).activatable(true).build());
    }
    let nota = gtk::Label::builder()
        .label("Uma predefinição escolhe as teclas e preenche a tela. Nada é gravado até você aplicar.")
        .wrap(true)
        .xalign(0.0)
        .max_width_chars(40)
        .css_classes(["dim-label", "caption"])
        .build();
    let caixa = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(8)
        .margin_end(8)
        .width_request(340)
        .build();
    caixa.append(&lista);
    caixa.append(&nota);
    let popover = gtk::Popover::builder().child(&caixa).build();
    let menu = gtk::MenuButton::builder()
        .label("Predefinições")
        .popover(&popover)
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .tooltip_text("Exemplos prontos: escolhem as teclas e preenchem a tela")
        .build();
    (menu, lista)
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
    // sem a faixa de transição do Clamp: todos os painéis ficam com a mesma largura, e não uma que
    // depende do conteúdo de cada um
    let limite = adw::Clamp::builder().child(conteudo).maximum_size(largura).tightening_threshold(largura).margin_top(6).margin_bottom(18).margin_start(12).margin_end(12).build();
    gtk::ScrolledWindow::builder().child(&limite).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build()
}
