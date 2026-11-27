//! Curso ao vivo, no modo Atuação: mostra o quanto cada tecla está afundada enquanto é apertada.
//! No desenho do teclado, a tecla se enche de cor; numa barra, aparece a profundidade em
//! milímetros da tecla mais afundada, com o ponto de atuação e as zonas mortas dela: riscadas na
//! barra, com a medida em milímetros embaixo, e ditas por extenso na legenda.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib};
use mag82::{Campo, Pos, CURSO_MAX};

use crate::teclas::Modo;
use crate::ui::Ui;
use crate::util::{mm, rotulo_longo};

/// Intervalo entre as leituras do curso.
const QUADRO: Duration = Duration::from_millis(50);
/// Altura da barra e da régua com as medidas embaixo dela, em pixels.
const ALTURA_BARRA: f64 = 34.0;
const ALTURA_REGUA: f64 = 16.0;

/// Em que trecho do curso a tecla está, para a legenda e para a cor da barra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trecho {
    Solta,
    /// Dentro da zona morta do início: o teclado ainda não considera o movimento.
    ZonaDoInicio,
    AntesDoPonto,
    Atuada,
    /// Dentro da zona morta do fim: o teclado já trata como fundo.
    ZonaDoFim,
}

impl Trecho {
    /// O trecho de um curso, dadas as medidas da tecla (tudo em milésimos de milímetro).
    pub fn de(curso: u16, ponto: u16, zona_inicio: u16, zona_fim: u16) -> Self {
        if curso == 0 {
            Trecho::Solta
        } else if curso < zona_inicio {
            Trecho::ZonaDoInicio
        } else if zona_fim > 0 && curso >= CURSO_MAX.saturating_sub(zona_fim) {
            Trecho::ZonaDoFim
        } else if curso >= ponto {
            Trecho::Atuada
        } else {
            Trecho::AntesDoPonto
        }
    }

    fn por_extenso(self) -> &'static str {
        match self {
            Trecho::Solta => "solta",
            Trecho::ZonaDoInicio => "na zona morta do início: o teclado ainda ignora o movimento",
            Trecho::AntesDoPonto => "antes do ponto de atuação",
            Trecho::Atuada => "atuada",
            Trecho::ZonaDoFim => "na zona morta do fim: para o teclado já é o fundo",
        }
    }
}

pub struct PainelCurso {
    pub grupo: adw::PreferencesGroup,
    pub ligado: adw::SwitchRow,
    barra: gtk::DrawingArea,
    legenda: gtk::Label,
    css: gtk::CssProvider,
    /// Último curso lido de cada tecla, em milésimos de milímetro.
    cursos: RefCell<BTreeMap<Pos, u16>>,
    /// A tecla mostrada na barra: a mais afundada, ou a última que esteve.
    em_foco: Cell<Option<Pos>>,
    pendente: Cell<bool>,
}

impl PainelCurso {
    pub fn construir(tela: &gdk::Display) -> Self {
        let css = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(tela, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        let ligado = adw::SwitchRow::builder()
            .title("Curso ao vivo")
            .subtitle("Mostra o quanto cada tecla afunda e onde fica o ponto de atuação dela")
            .build();
        let barra = gtk::DrawingArea::builder()
            .content_height((ALTURA_BARRA + ALTURA_REGUA) as i32)
            .hexpand(true)
            .visible(false)
            .build();
        let legenda =
            gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label", "caption"]).visible(false).build();
        let grupo = adw::PreferencesGroup::new();
        grupo.add(&ligado);
        let medidor = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(12).build();
        medidor.append(&barra);
        medidor.append(&legenda);
        grupo.add(&medidor);
        Self {
            grupo,
            ligado,
            barra,
            legenda,
            css,
            cursos: RefCell::new(BTreeMap::new()),
            em_foco: Cell::new(None),
            pendente: Cell::new(false),
        }
    }
}

impl PainelCurso {
    /// A tecla mais afundada na última leitura e o curso dela.
    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn mais_afundada(&self) -> Option<(Pos, u16)> {
        self.cursos.borrow().iter().filter(|(_, c)| **c > 0).max_by_key(|(_, c)| **c).map(|(p, c)| (*p, *c))
    }

    /// O texto embaixo da barra.
    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn texto(&self) -> String {
        self.legenda.label().to_string()
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let p = &ui.curso;
    let ui2 = ui.clone();
    p.ligado.connect_active_notify(move |chave| {
        let p = &ui2.curso;
        // a escolha fica guardada: da próxima vez o programa abre do jeito que o usuário deixou
        if !ui2.carregando.get() {
            let mut config = crate::atualizacao::Config::ler();
            config.curso_ao_vivo = chave.is_active();
            config.gravar();
        }
        p.barra.set_visible(chave.is_active());
        p.legenda.set_visible(chave.is_active());
        if !chave.is_active() {
            p.cursos.borrow_mut().clear();
            p.em_foco.set(None);
            p.css.load_from_string("");
        }
        ui2.descrever_curso();
    });
    // vem ligado, a não ser que o usuário tenha desligado antes
    ui.silencioso(|| p.ligado.set_active(crate::atualizacao::Config::ler().curso_ao_vivo));
    let ui2 = ui.clone();
    p.barra.set_draw_func(move |area, cr, largura, _| ui2.desenhar_curso(cr, f64::from(largura), area.color()));
    let ui2 = ui.clone();
    glib::timeout_add_local(QUADRO, move || {
        ui2.ler_cursos();
        glib::ControlFlow::Continue
    });
}

impl Ui {
    /// Pede os cursos ao teclado, se a chave está ligada, o modo Atuação está à vista e nada
    /// mais está na fila.
    fn ler_cursos(self: &Rc<Self>) {
        let p = &self.curso;
        // só com a janela em uso: em segundo plano o programa não fica consultando o teclado
        // (o autoteste roda sem gerenciador de janelas, onde nenhuma janela conta como em uso)
        let a_vista = (self.janela.is_active() || cfg!(feature = "autoteste"))
            && self.pilha.visible_child_name().as_deref() == Some("teclas")
            && self.tec.modo.get() == Modo::Atuacao
            && p.barra.is_mapped();
        if !p.ligado.is_active() || !a_vista || p.pendente.get() || self.ocupado.get() > 0 || self.d.borrow().is_none() {
            return;
        }
        p.pendente.set(true);
        self.pedir_bruto(
            false,
            |t| t.cursos(),
            |ui, resultado| {
                ui.curso.pendente.set(false);
                if let Ok(cursos) = resultado {
                    ui.mostrar_cursos(cursos);
                }
            },
        );
    }

    fn mostrar_cursos(&self, cursos: BTreeMap<Pos, u16>) {
        let p = &self.curso;
        if !p.ligado.is_active() {
            return;
        }
        // cada tecla apertada se enche de baixo para cima, na proporção do curso
        let regras: String = cursos
            .iter()
            .filter(|(_, c)| **c > 0)
            .map(|(pos, c)| {
                let cheio = (f64::from(*c) / f64::from(CURSO_MAX) * 100.0).clamp(0.0, 100.0);
                format!(
                    "#k{}_{} {{ background-image: linear-gradient(to top, alpha(@accent_bg_color, 0.85) {cheio:.0}%, transparent {cheio:.0}%); }}\n",
                    pos.0, pos.1
                )
            })
            .collect();
        p.css.load_from_string(&regras);
        if let Some((pos, _)) = cursos.iter().filter(|(_, c)| **c > 0).max_by_key(|(_, c)| **c) {
            p.em_foco.set(Some(*pos));
        }
        *p.cursos.borrow_mut() = cursos;
        self.descrever_curso();
        p.barra.queue_draw();
    }

    fn descrever_curso(&self) {
        let p = &self.curso;
        let dados = self.d.borrow();
        let texto = match (p.em_foco.get(), dados.as_ref()) {
            (Some(pos), Some(d)) if d.nomes.contains_key(&pos) => {
                let curso = p.cursos.borrow().get(&pos).copied().unwrap_or(0);
                let atuacao = &d.atuacao[&pos];
                let (ponto, inicio, fim) = (atuacao.mm(Campo::Ponto), atuacao.mm(Campo::ZonaTopo), atuacao.mm(Campo::ZonaFundo));
                let zonas = match (inicio, fim) {
                    (0, 0) => "Sem zonas mortas.".to_string(),
                    (_, 0) => format!("Zona morta: os primeiros {} mm.", mm(inicio)),
                    (0, _) => format!("Zona morta: os últimos {} mm.", mm(fim)),
                    _ => format!("Zonas mortas (riscadas): os primeiros {} mm e os últimos {} mm.", mm(inicio), mm(fim)),
                };
                format!(
                    "{}: {} mm de {} mm, {}. O traço marca o ponto de atuação ({} mm). {zonas}",
                    rotulo_longo(&d.nomes[&pos]),
                    mm(curso),
                    mm(CURSO_MAX),
                    Trecho::de(curso, ponto, inicio, fim).por_extenso(),
                    mm(ponto)
                )
            }
            _ => "Aperte uma tecla para ver o curso dela aqui.".to_string(),
        };
        p.legenda.set_label(&texto);
    }

    /// A barra do curso da tecla em foco: o fundo é o curso inteiro, as pontas riscadas são as
    /// zonas mortas, o preenchimento é o quanto ela está afundada e o traço, o ponto de atuação.
    /// Embaixo, uma régua com as medidas em milímetros: onde acaba a zona do início, o ponto de
    /// atuação e onde começa a zona do fim. `tinta` é a cor do texto do tema.
    fn desenhar_curso(&self, cr: &gtk::cairo::Context, largura: f64, tinta: gdk::RGBA) {
        let altura = ALTURA_BARRA;
        let p = &self.curso;
        let dados = self.d.borrow();
        let maximo = f64::from(CURSO_MAX);
        let x = |valor: u16| (f64::from(valor) / maximo).clamp(0.0, 1.0) * largura;
        let retangulo = |de: f64, ate: f64, cor: (f64, f64, f64, f64)| {
            cr.set_source_rgba(cor.0, cor.1, cor.2, cor.3);
            cr.rectangle(de, 0.0, (ate - de).max(0.0), altura);
            let _ = cr.fill();
        };
        // cantos arredondados, como os outros controles
        let raio = 6.0;
        cr.new_sub_path();
        cr.arc(largura - raio, raio, raio, -std::f64::consts::FRAC_PI_2, 0.0);
        cr.arc(largura - raio, altura - raio, raio, 0.0, std::f64::consts::FRAC_PI_2);
        cr.arc(raio, altura - raio, raio, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
        cr.arc(raio, raio, raio, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        cr.close_path();
        let _ = cr.save();
        cr.clip();
        retangulo(0.0, largura, (0.5, 0.5, 0.5, 0.18));
        let (Some(pos), Some(d)) = (p.em_foco.get(), dados.as_ref()) else {
            let _ = cr.restore();
            return;
        };
        let Some(atuacao) = d.atuacao.get(&pos) else {
            let _ = cr.restore();
            return;
        };
        let curso = p.cursos.borrow().get(&pos).copied().unwrap_or(0);
        let (ponto, inicio, fim) = (atuacao.mm(Campo::Ponto), atuacao.mm(Campo::ZonaTopo), atuacao.mm(Campo::ZonaFundo));
        let (fim_do_inicio, comeco_do_fim) = (x(inicio), largura - x(fim));
        // cinza dentro da zona morta do início (o teclado ignora), laranja até o ponto de atuação
        // e verde depois dele
        let cor = match Trecho::de(curso, ponto, inicio, fim) {
            Trecho::ZonaDoInicio => (0.55, 0.55, 0.6, 0.95),
            Trecho::Atuada | Trecho::ZonaDoFim => (0.18, 0.76, 0.49, 0.95),
            Trecho::Solta | Trecho::AntesDoPonto => (0.96, 0.55, 0.12, 0.95),
        };
        retangulo(0.0, x(curso), cor);
        // zonas mortas: riscadas na diagonal, por cima do preenchimento, para aparecerem com a
        // tecla solta e com ela apertada
        for (de, ate) in [(0.0, fim_do_inicio), (comeco_do_fim, largura)] {
            if ate - de < 0.5 {
                continue;
            }
            retangulo(de, ate, (0.5, 0.5, 0.5, 0.22));
            let _ = cr.save();
            cr.rectangle(de, 0.0, ate - de, altura);
            cr.clip();
            cr.set_source_rgba(tinta.red().into(), tinta.green().into(), tinta.blue().into(), 0.45);
            cr.set_line_width(1.0);
            let mut risco = de - altura;
            while risco < ate {
                cr.move_to(risco, altura);
                cr.line_to(risco + altura, 0.0);
                risco += 6.0;
            }
            let _ = cr.stroke();
            let _ = cr.restore();
        }
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.rectangle(x(ponto) - 1.5, 0.0, 3.0, altura);
        let _ = cr.fill();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        cr.rectangle(x(ponto) - 2.5, 0.0, 1.0, altura);
        cr.rectangle(x(ponto) + 1.5, 0.0, 1.0, altura);
        let _ = cr.fill();
        let _ = cr.restore();

        // régua: as medidas em milímetros, embaixo da barra. A do ponto de atuação fica centrada
        // no traço; as das zonas, no limite de cada uma, e somem se encostarem na do ponto.
        cr.set_font_size(11.0);
        let base = altura + ALTURA_REGUA - 3.0;
        let escrever = |texto: &str, centro: f64, opacidade: f64| -> (f64, f64) {
            let largura_do_texto = cr.text_extents(texto).map_or(0.0, |e| e.width());
            let de = (centro - largura_do_texto / 2.0).clamp(0.0, (largura - largura_do_texto).max(0.0));
            cr.set_source_rgba(tinta.red().into(), tinta.green().into(), tinta.blue().into(), opacidade);
            cr.move_to(de, base);
            let _ = cr.show_text(texto);
            (de, de + largura_do_texto)
        };
        let (ponto_de, ponto_ate) = escrever(&mm(ponto), x(ponto), 0.95);
        let cabe = |centro: f64, texto: &str| {
            let metade = cr.text_extents(texto).map_or(0.0, |e| e.width()) / 2.0 + 6.0;
            centro + metade < ponto_de || centro - metade > ponto_ate
        };
        if inicio > 0 && cabe(fim_do_inicio, &mm(inicio)) {
            escrever(&mm(inicio), fim_do_inicio, 0.65);
        }
        if fim > 0 {
            let texto = mm(CURSO_MAX.saturating_sub(fim));
            if cabe(comeco_do_fim, &texto) {
                escrever(&texto, comeco_do_fim, 0.65);
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn trechos_do_curso() {
        // ponto em 2,00 mm, zonas mortas de 0,20 mm nas duas pontas
        let trecho = |curso| Trecho::de(curso, 2000, 200, 200);
        assert_eq!(trecho(0), Trecho::Solta);
        assert_eq!(trecho(150), Trecho::ZonaDoInicio);
        assert_eq!(trecho(200), Trecho::AntesDoPonto);
        assert_eq!(trecho(1999), Trecho::AntesDoPonto);
        assert_eq!(trecho(2000), Trecho::Atuada);
        assert_eq!(trecho(3199), Trecho::Atuada);
        assert_eq!(trecho(3200), Trecho::ZonaDoFim);
        assert_eq!(trecho(3400), Trecho::ZonaDoFim);
        // sem zonas mortas, o fundo é só "atuada"
        assert_eq!(Trecho::de(3400, 2000, 0, 0), Trecho::Atuada);
        assert_eq!(Trecho::de(50, 2000, 0, 0), Trecho::AntesDoPonto);
    }
}
