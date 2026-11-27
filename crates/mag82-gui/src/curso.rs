//! Curso ao vivo, no modo Atuação: mostra o quanto cada tecla está afundada enquanto é apertada.
//! No desenho do teclado, a tecla se enche de cor; numa barra, aparece a profundidade em
//! milímetros da tecla mais afundada, com o ponto de atuação e as zonas mortas dela.

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
        let barra = gtk::DrawingArea::builder().content_height(34).hexpand(true).visible(false).build();
        let legenda = gtk::Label::builder().xalign(0.0).css_classes(["dim-label", "caption"]).visible(false).build();
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
    p.barra.set_draw_func(move |_, cr, largura, altura| ui2.desenhar_curso(cr, f64::from(largura), f64::from(altura)));
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
                format!(
                    "{}: {} mm de {} mm. O traço marca o ponto de atuação ({} mm).",
                    rotulo_longo(&d.nomes[&pos]),
                    mm(curso),
                    mm(CURSO_MAX),
                    mm(d.atuacao[&pos].mm(Campo::Ponto))
                )
            }
            _ => "Aperte uma tecla para ver o curso dela aqui.".to_string(),
        };
        p.legenda.set_label(&texto);
    }

    /// A barra do curso da tecla em foco: o fundo é o curso inteiro, as pontas mais escuras são
    /// as zonas mortas, o preenchimento é o quanto ela está afundada e o traço, o ponto de atuação.
    fn desenhar_curso(&self, cr: &gtk::cairo::Context, largura: f64, altura: f64) {
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
        cr.clip();
        retangulo(0.0, largura, (0.5, 0.5, 0.5, 0.18));
        let (Some(pos), Some(d)) = (p.em_foco.get(), dados.as_ref()) else { return };
        let Some(atuacao) = d.atuacao.get(&pos) else { return };
        // zonas mortas: no começo (topo do curso) e no fim (fundo)
        retangulo(0.0, x(atuacao.mm(Campo::ZonaTopo)), (0.5, 0.5, 0.5, 0.22));
        retangulo(largura - x(atuacao.mm(Campo::ZonaFundo)), largura, (0.5, 0.5, 0.5, 0.22));
        let curso = p.cursos.borrow().get(&pos).copied().unwrap_or(0);
        let ponto = atuacao.mm(Campo::Ponto);
        // verde depois que passa do ponto de atuação, laranja antes
        let cor = if curso >= ponto { (0.18, 0.76, 0.49, 0.95) } else { (0.96, 0.55, 0.12, 0.95) };
        retangulo(0.0, x(curso), cor);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.rectangle(x(ponto) - 1.5, 0.0, 3.0, altura);
        let _ = cr.fill();
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        cr.rectangle(x(ponto) - 2.5, 0.0, 1.0, altura);
        cr.rectangle(x(ponto) + 1.5, 0.0, 1.0, altura);
        let _ = cr.fill();
    }
}
