//! Janela "Sobre": uma abertura animada no estilo dos keygens antigos (letreiro em blocos com as
//! cores correndo, estrelas e um texto que passa ondulando) e, embaixo, o essencial do projeto.

use std::f64::consts::TAU;

use adw::prelude::*;
use gtk::{cairo, gio};

use crate::ui::{Ui, AUTOR_E_CIDADE};

const LARGURA: i32 = 520;
const ALTURA: i32 = 230;
const VERSAO: &str = env!("CARGO_PKG_VERSION");
const REPOSITORIO: &str = env!("CARGO_PKG_REPOSITORY");

/// As letras do letreiro, em pontos de 5 x 7 (um bit por ponto, o mais alto à esquerda).
const LETREIRO: [[u8; 7]; 5] = [
    [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001], // M
    [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001], // A
    [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110], // G
    [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110], // 8
    [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111], // 2
];

/// Texto que passa embaixo, sem acentos, como nas aberturas antigas.
fn texto_que_passa() -> String {
    format!(
        "MAG82 PARA LINUX v{VERSAO}  ***  O CONFIGURADOR QUE FALTAVA PARA O AVELL VOLCANO'S MAG82  ***  \
         DESENVOLVIMENTO INDEPENDENTE  ***  JULIANO LIBERATO - JOINVILLE - SC  ***  \
         SEM VINCULO COM A AVELL  ***  CODIGO ABERTO, LICENCA MIT  ***  \
         SALVE PARA QUEM USA, TESTA E CONTRIBUI  ***          "
    )
}

/// Cor do arco-íris: `tom` de 0 a 1 dá a volta completa.
fn arco_iris(tom: f64, brilho: f64) -> (f64, f64, f64) {
    let canal = |desvio: f64| (0.5 + 0.5 * (TAU * (tom + desvio)).cos()) * brilho;
    (canal(0.0), canal(-1.0 / 3.0), canal(-2.0 / 3.0))
}

/// Número pseudoaleatório fixo entre 0 e 1 para a estrela `i` (as estrelas não mudam entre quadros).
fn sorteio(i: u32, sal: u32) -> f64 {
    let mut x = i.wrapping_mul(2_654_435_761).wrapping_add(sal.wrapping_mul(40_503));
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    f64::from(x % 10_000) / 10_000.0
}

/// Desenha um quadro da abertura. `t` é o tempo em segundos; parado em 0 quando o sistema pede
/// para não animar.
fn desenhar(cr: &cairo::Context, largura: f64, altura: f64, t: f64) {
    // tela preta com os cantos arredondados
    let raio = 14.0;
    cr.new_sub_path();
    cr.arc(largura - raio, raio, raio, -TAU / 4.0, 0.0);
    cr.arc(largura - raio, altura - raio, raio, 0.0, TAU / 4.0);
    cr.arc(raio, altura - raio, raio, TAU / 4.0, TAU / 2.0);
    cr.arc(raio, raio, raio, TAU / 2.0, TAU * 0.75);
    cr.close_path();
    cr.clip();
    cr.set_source_rgb(0.02, 0.02, 0.05);
    let _ = cr.paint();

    // estrelas em três velocidades, correndo para a esquerda
    for i in 0..90u32 {
        let camada = f64::from(i % 3) + 1.0;
        let x = (sorteio(i, 1) * largura - t * 14.0 * camada).rem_euclid(largura);
        let y = sorteio(i, 2) * altura;
        let claro = 0.25 + 0.25 * camada;
        cr.set_source_rgb(claro, claro, claro);
        cr.rectangle(x, y, camada.min(2.0), camada.min(2.0));
        let _ = cr.fill();
    }

    // barras de cor subindo e descendo atrás do letreiro
    for barra in 0..3 {
        let centro = altura * 0.42 + (t * 1.3 + f64::from(barra) * 0.9).sin() * altura * 0.24;
        let (r, g, b) = arco_iris(t * 0.07 + f64::from(barra) / 3.0, 1.0);
        let faixa = cairo::LinearGradient::new(0.0, centro - 9.0, 0.0, centro + 9.0);
        faixa.add_color_stop_rgba(0.0, r, g, b, 0.0);
        faixa.add_color_stop_rgba(0.5, r, g, b, 0.38);
        faixa.add_color_stop_rgba(1.0, r, g, b, 0.0);
        let _ = cr.set_source(&faixa);
        cr.rectangle(0.0, centro - 9.0, largura, 18.0);
        let _ = cr.fill();
    }

    // letreiro em blocos: cada coluna ondula e as cores correm na diagonal
    let ponto = 11.0;
    let (colunas, vao) = (5.0, 1.4);
    let total = ponto * (LETREIRO.len() as f64 * colunas + (LETREIRO.len() as f64 - 1.0) * vao);
    let (x0, y0) = ((largura - total) / 2.0, altura * 0.42 - ponto * 3.5);
    for sombra in [true, false] {
        for (l, letra) in LETREIRO.iter().enumerate() {
            for (linha, bits) in letra.iter().enumerate() {
                for coluna in 0..5 {
                    if bits & (0b10000 >> coluna) == 0 {
                        continue;
                    }
                    let x = x0 + (l as f64 * (colunas + vao) + f64::from(coluna)) * ponto;
                    let y = y0 + linha as f64 * ponto + (t * 2.6 + x * 0.012).sin() * 4.5;
                    if sombra {
                        cr.set_source_rgba(0.0, 0.0, 0.0, 0.7);
                        cr.rectangle(x + 4.0, y + 4.0, ponto - 1.5, ponto - 1.5);
                    } else {
                        let (r, g, b) = arco_iris(t * 0.25 - x * 0.0016 - linha as f64 * 0.045, 1.0);
                        cr.set_source_rgb(r, g, b);
                        cr.rectangle(x, y, ponto - 1.5, ponto - 1.5);
                    }
                    let _ = cr.fill();
                }
            }
        }
    }

    // subtítulo fixo
    cr.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(13.0);
    let sub = format!("P A R A   L I N U X   ·   v{VERSAO}");
    if let Ok(medida) = cr.text_extents(&sub) {
        cr.set_source_rgb(0.75, 0.8, 0.9);
        cr.move_to((largura - medida.width()) / 2.0, altura * 0.42 + ponto * 3.5 + 30.0);
        let _ = cr.show_text(&sub);
    }

    // texto que passa embaixo, cada letra numa altura da onda
    cr.set_font_size(17.0);
    let passo = cr.text_extents("M").map(|m| m.x_advance()).unwrap_or(10.0);
    let texto: Vec<char> = texto_que_passa().chars().collect();
    let comprimento = texto.len() as f64 * passo;
    let inicio = largura - (t * 95.0).rem_euclid(comprimento + largura);
    let mut um = [0u8; 4];
    for (i, letra) in texto.iter().enumerate() {
        let x = inicio + i as f64 * passo;
        if x < -passo || x > largura {
            continue;
        }
        let y = altura - 22.0 + (t * 4.2 + x * 0.03).sin() * 7.0;
        let (r, g, b) = arco_iris(x * 0.0022 + t * 0.12, 1.0);
        cr.set_source_rgb(0.35 + 0.65 * r, 0.35 + 0.65 * g, 0.35 + 0.65 * b);
        cr.move_to(x, y);
        let _ = cr.show_text(letra.encode_utf8(&mut um));
    }
}

fn abertura() -> gtk::DrawingArea {
    let tela = gtk::DrawingArea::builder().content_width(LARGURA).content_height(ALTURA).build();
    tela.set_draw_func(|tela, cr, largura, altura| {
        // sem animação quando o sistema está configurado para reduzir movimento
        let animar = gtk::Settings::default().map_or(true, |s| s.is_gtk_enable_animations());
        let t = match tela.frame_clock() {
            Some(relogio) if animar => relogio.frame_time() as f64 / 1e6,
            _ => 0.0,
        };
        desenhar(cr, f64::from(largura), f64::from(altura), t);
    });
    tela.add_tick_callback(|tela, _| {
        tela.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    tela
}

impl Ui {
    pub fn mostrar_sobre(&self) {
        let corpo = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(6)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        corpo.append(&abertura());

        let texto = gtk::Label::builder()
            .label(
                "Configura o teclado Avell Volcano's MAG82 no Linux: atuação, Rapid Trigger, remapeamento, \
                 atalhos Fn, SOCD, DKS, luzes, perfis e calibração.",
            )
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();
        corpo.append(&texto);

        let links = adw::PreferencesGroup::new();
        for (titulo, endereco) in [
            ("Código-fonte do projeto", REPOSITORIO.to_string()),
            ("Como contribuir", format!("{REPOSITORIO}/blob/main/CONTRIBUTING.md")),
        ] {
            let linha = adw::ActionRow::builder().title(titulo).activatable(true).build();
            linha.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            let janela = self.janela.clone();
            linha.connect_activated(move |_| {
                gtk::UriLauncher::new(&endereco).launch(Some(&janela), gio::Cancellable::NONE, |_| {});
            });
            links.add(&linha);
        }
        corpo.append(&links);

        let rodape = gtk::Label::builder()
            .label(format!(
                "Desenvolvimento independente, sem vínculo com a Avell.\n{AUTOR_E_CIDADE} · Licença MIT"
            ))
            .justify(gtk::Justification::Center)
            .css_classes(["dim-label", "caption"])
            .build();
        corpo.append(&rodape);

        let rolagem = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .child(&corpo)
            .build();
        let visao = adw::ToolbarView::new();
        visao.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
        visao.set_content(Some(&rolagem));
        adw::Dialog::builder()
            .title("Sobre o MAG82 para Linux")
            .content_width(LARGURA + 48)
            .child(&visao)
            .build()
            .present(Some(&self.janela));
    }
}
