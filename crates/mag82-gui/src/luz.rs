//! Página Iluminação: efeito e paleta de cada área de luz (teclas e faixa lateral) e a
//! demonstração ao vivo do que o teclado está mostrando.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{cairo, gdk, glib};
use mag82::{Grade, Luz, Rgb, AREA_LATERAL, AREA_TECLAS};

use crate::dados::Dados;
use crate::ui::Ui;
use crate::util::{botao, botao_cor, de_rgba, linha_escala, para_rgba};

/// Intervalo entre os quadros da demonstração.
const QUADRO: Duration = Duration::from_millis(100);

/// Controles de uma área de luz.
pub struct Controles {
    area: u8,
    g_efeito: adw::PreferencesGroup,
    g_paleta: adw::PreferencesGroup,
    ligada: adw::SwitchRow,
    modo: adw::ComboRow,
    pub brilho: gtk::Scale,
    veloc: gtk::Scale,
    dir: adw::SwitchRow,
    cor: adw::ComboRow,
    paleta_btns: Vec<gtk::ColorDialogButton>,
    /// Contador usado para enviar a iluminação só depois que o usuário para de arrastar.
    geracao: Cell<u32>,
}

impl Controles {
    fn construir(area: u8, titulo: &str, titulo_paleta: &str, descricao_paleta: &str) -> Self {
        let ligada = adw::SwitchRow::builder().title("Ligada").build();
        let modo = adw::ComboRow::builder().title("Efeito").build();
        let (linha_brilho, brilho) = linha_escala("Brilho");
        let (linha_veloc, veloc) = linha_escala("Velocidade");
        let dir = adw::SwitchRow::builder().title("Inverter o sentido do efeito").build();
        let cor = adw::ComboRow::builder()
            .title("Cor do efeito")
            .subtitle("Nem todo efeito usa a cor escolhida")
            .model(&gtk::StringList::new(&["Padrão do efeito", "Cor 1", "Cor 2", "Cor 3", "Cor 4", "Cor 5", "Cor 6", "Cor 7"]))
            .build();
        let g_efeito = adw::PreferencesGroup::builder().title(titulo).build();
        g_efeito.add(&ligada);
        g_efeito.add(&modo);
        g_efeito.add(&linha_brilho);
        g_efeito.add(&linha_veloc);
        g_efeito.add(&dir);
        g_efeito.add(&cor);

        let g_paleta = adw::PreferencesGroup::builder().title(titulo_paleta).description(descricao_paleta).build();
        let linha_paleta = adw::ActionRow::builder().title("Cores 1 a 7").build();
        let paleta_btns: Vec<gtk::ColorDialogButton> = (0..7).map(|_| botao_cor()).collect();
        for (i, b) in paleta_btns.iter().enumerate() {
            b.set_tooltip_text(Some(&format!("Cor {}", i + 1)));
            linha_paleta.add_suffix(b);
        }
        g_paleta.add(&linha_paleta);
        Self { area, g_efeito, g_paleta, ligada, modo, brilho, veloc, dir, cor, paleta_btns, geracao: Cell::new(0) }
    }

    fn ligar(&self, ui: &Rc<Ui>) {
        let area = self.area;
        let ui2 = ui.clone();
        self.ligada.connect_active_notify(move |_| ui2.luz_mudou(area));
        let ui2 = ui.clone();
        self.dir.connect_active_notify(move |_| ui2.luz_mudou(area));
        let ui2 = ui.clone();
        self.modo.connect_selected_notify(move |_| ui2.luz_mudou(area));
        let ui2 = ui.clone();
        self.cor.connect_selected_notify(move |_| ui2.luz_mudou(area));
        let ui2 = ui.clone();
        self.brilho.connect_value_changed(move |_| ui2.luz_mudou(area));
        let ui2 = ui.clone();
        self.veloc.connect_value_changed(move |_| ui2.luz_mudou(area));
        for (i, b) in self.paleta_btns.iter().enumerate() {
            let ui2 = ui.clone();
            b.connect_rgba_notify(move |botao| ui2.paleta_mudou(area, i + 1, botao));
        }
    }

    fn preencher(&self, efeitos: u8, luz: &Luz, paleta: &mag82::Paleta) {
        let nomes: Vec<String> = (0..efeitos).map(|m| mag82::nome_efeito_de(self.area, efeitos, m)).collect();
        let nomes_ref: Vec<&str> = nomes.iter().map(String::as_str).collect();
        self.modo.set_model(Some(&gtk::StringList::new(&nomes_ref)));
        self.modo.set_selected(u32::from(luz.modo.min(efeitos.saturating_sub(1))));
        self.ligada.set_active(luz.ligada);
        self.brilho.set_value(f64::from(luz.brilho));
        self.veloc.set_value(f64::from(luz.velocidade));
        self.dir.set_active(luz.invertida);
        self.cor.set_selected(u32::from(luz.cor.min(7)));
        for (i, b) in self.paleta_btns.iter().enumerate() {
            b.set_rgba(&para_rgba(paleta.cor(i + 1)));
        }
    }

    fn da_tela(&self) -> Luz {
        Luz {
            ligada: self.ligada.is_active(),
            modo: self.modo.selected() as u8,
            brilho: self.brilho.value() as u8,
            velocidade: self.veloc.value() as u8,
            invertida: self.dir.is_active(),
            cor: self.cor.selected() as u8,
        }
    }
}

pub struct PainelLuz {
    pub pagina: adw::PreferencesPage,
    pub teclas: Controles,
    pub lateral: Controles,
    /// Desenho do teclado com as cores lidas dele.
    pub area: gtk::DrawingArea,
    /// Último quadro lido: a cor que cada tecla está mostrando.
    pub previa: RefCell<Option<Grade>>,
    /// O mesmo para os LEDs da faixa lateral.
    previa_lateral: RefCell<Option<Grade>>,
    previa_pendente: Cell<bool>,

    // ----- cor fixa por LED da faixa lateral -----
    g_leds: adw::PreferencesGroup,
    faixa: gtk::Box,
    /// Um botão por LED, criados quando se sabe quantos LEDs o teclado tem.
    pub led_btns: RefCell<Vec<gtk::ToggleButton>>,
    css_leds: gtk::CssProvider,
    cor_led: gtk::ColorDialogButton,
    todos_leds: gtk::Button,
    nenhum_led: gtk::Button,
    tira_cor_led: gtk::Button,
    pub ap_cor_led: gtk::Button,
}

impl PainelLuz {
    pub fn construir() -> (Self, adw::PreferencesPage) {
        let area = gtk::DrawingArea::builder().content_width(240).content_height(270).hexpand(true).build();
        let g_previa = adw::PreferencesGroup::builder()
            .title("Demonstração")
            .description(
                "O que o teclado está mostrando agora; a faixa de baixo é a luz lateral. \
                 Os efeitos de acionamento só acendem ao digitar.",
            )
            .build();
        g_previa.add(&area);

        let teclas =
            Controles::construir(AREA_TECLAS, "Luz das teclas", "Paleta das teclas", "As sete cores que os efeitos das teclas podem usar");
        let lateral =
            Controles::construir(AREA_LATERAL, "Luz lateral", "Paleta da lateral", "As sete cores que os efeitos da faixa lateral podem usar");

        // cor fixa por LED: a faixa é uma fileira de botões, um por LED
        let g_leds = adw::PreferencesGroup::builder()
            .title("Cores fixas da lateral")
            .description("Clique nos LEDs da faixa e aplique uma cor: eles ficam nessa cor, por cima do efeito")
            .build();
        let faixa = gtk::Box::builder().homogeneous(true).spacing(2).build();
        let cor_led = botao_cor();
        cor_led.set_rgba(&para_rgba(Rgb(255, 0, 0)));
        let todos_leds = botao("Selecionar todos", false);
        let nenhum_led = botao("Limpar seleção", false);
        let tira_cor_led = botao("Voltar ao efeito", false);
        let ap_cor_led = botao("Aplicar cor", true);
        let selecao = gtk::Box::builder().spacing(8).hexpand(true).build();
        selecao.append(&todos_leds);
        selecao.append(&nenhum_led);
        let aplicar = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        aplicar.append(&cor_led);
        aplicar.append(&tira_cor_led);
        aplicar.append(&ap_cor_led);
        // duas caixas numa linha que quebra em duas quando a página estreita
        let acoes = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(2)
            .column_spacing(8)
            .row_spacing(8)
            .margin_top(12)
            .build();
        acoes.append(&selecao);
        acoes.append(&aplicar);
        g_leds.add(&faixa);
        g_leds.add(&acoes);
        let css_leds = gtk::CssProvider::new();
        if let Some(tela) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&tela, &css_leds, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        let pagina = adw::PreferencesPage::new();
        pagina.add(&g_previa);
        pagina.add(&teclas.g_efeito);
        pagina.add(&teclas.g_paleta);
        pagina.add(&lateral.g_efeito);
        pagina.add(&g_leds);
        pagina.add(&lateral.g_paleta);
        let painel = Self {
            pagina: pagina.clone(),
            teclas,
            lateral,
            area,
            previa: RefCell::new(None),
            previa_lateral: RefCell::new(None),
            previa_pendente: Cell::new(false),
            g_leds,
            faixa,
            led_btns: RefCell::new(Vec::new()),
            css_leds,
            cor_led,
            todos_leds,
            nenhum_led,
            tira_cor_led,
            ap_cor_led,
        };
        (painel, pagina)
    }

    fn controles(&self, area: u8) -> &Controles {
        if area == AREA_LATERAL {
            &self.lateral
        } else {
            &self.teclas
        }
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let p = &ui.luz;
    p.teclas.ligar(ui);
    p.lateral.ligar(ui);
    let ui2 = ui.clone();
    p.todos_leds.connect_clicked(move |_| ui2.marcar_leds(true));
    let ui2 = ui.clone();
    p.nenhum_led.connect_clicked(move |_| ui2.marcar_leds(false));
    let ui2 = ui.clone();
    p.ap_cor_led.connect_clicked(move |_| ui2.aplicar_cor_lateral(false));
    let ui2 = ui.clone();
    p.tira_cor_led.connect_clicked(move |_| ui2.aplicar_cor_lateral(true));
    let ui2 = ui.clone();
    p.area.set_draw_func(move |_, cr, largura, altura| ui2.desenhar_previa(cr, f64::from(largura), f64::from(altura)));
    let ui2 = ui.clone();
    glib::timeout_add_local(QUADRO, move || {
        ui2.atualizar_previa();
        glib::ControlFlow::Continue
    });
}

impl Ui {
    pub fn preencher_luz(&self, d: &Dados) {
        let p = &self.luz;
        p.teclas.preencher(d.efeitos, &d.luz, &d.paleta);
        // nem todo teclado tem luz lateral
        p.lateral.g_efeito.set_visible(d.lateral.is_some());
        p.lateral.g_paleta.set_visible(d.lateral.is_some());
        p.g_leds.set_visible(d.lateral.is_some());
        if let Some(l) = &d.lateral {
            p.lateral.preencher(l.area.efeitos, &l.luz, &l.paleta);
        }
        *p.previa_lateral.borrow_mut() = d.lateral.as_ref().map(|l| l.grade.clone());

        // a faixa de botões é refeita só se a quantidade de LEDs mudou (a seleção se mantém)
        let total = d.lateral.as_ref().map_or(0, |l| l.grade.leds());
        let mut botoes = p.led_btns.borrow_mut();
        if botoes.len() != total {
            for b in botoes.drain(..) {
                p.faixa.remove(&b);
            }
            for i in 0..total {
                let b = gtk::ToggleButton::builder()
                    .css_classes(["led"])
                    .name(format!("led{i}"))
                    .tooltip_text(format!("LED {}", i + 1))
                    .build();
                p.faixa.append(&b);
                botoes.push(b);
            }
        }
        drop(botoes);
        self.pintar_leds(d);
    }

    // ---------- cor fixa por LED da lateral ----------

    /// Pinta cada botão da faixa com a cor fixa do seu LED.
    fn pintar_leds(&self, d: &Dados) {
        let Some(l) = &d.lateral else { return };
        let regras: String = (0..l.grade.leds())
            .filter_map(|i| l.grade.fixa_do_led(i).map(|c| format!("#led{i} {{ background: {}; }}\n", c.hex())))
            .collect();
        self.luz.css_leds.load_from_string(&regras);
    }

    pub fn marcar_leds(&self, todos: bool) {
        for b in self.luz.led_btns.borrow().iter() {
            b.set_active(todos);
        }
    }

    pub fn aplicar_cor_lateral(self: &Rc<Self>, limpar: bool) {
        let p = &self.luz;
        let sel: Vec<usize> =
            p.led_btns.borrow().iter().enumerate().filter(|(_, b)| b.is_active()).map(|(i, _)| i).collect();
        if sel.is_empty() {
            self.avisar("Clique nos LEDs da faixa que quer colorir");
            return;
        }
        let cor = (!limpar).then(|| de_rgba(&p.cor_led.rgba()));
        let atual = self.d.borrow().as_ref().and_then(|d| d.lateral.as_ref().map(|l| (l.area.indice, l.grade.clone())));
        let Some((area, mut grade)) = atual else { return };
        for led in &sel {
            grade.definir_led(*led, cor);
        }
        let total = sel.len();
        self.pedir(
            move |t| {
                t.definir_grade_de(area, &grade)?;
                Ok(grade)
            },
            move |ui, grade| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    if let Some(l) = d.lateral.as_mut() {
                        l.grade = grade;
                    }
                    ui.pintar_leds(d);
                }
                ui.avisar(&if limpar {
                    "LEDs de volta ao efeito".to_string()
                } else {
                    format!("Cor aplicada a {total} LED(s) da lateral")
                });
            },
        );
    }

    fn luz_mudou(self: &Rc<Self>, area: u8) {
        if self.carregando.get() {
            return;
        }
        // espera um instante: ao arrastar o controle chegam dezenas de mudanças seguidas
        let c = self.luz.controles(area);
        let geracao = c.geracao.get().wrapping_add(1);
        c.geracao.set(geracao);
        let ui = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(120), move || {
            let c = ui.luz.controles(area);
            if c.geracao.get() != geracao {
                return;
            }
            let luz = c.da_tela();
            let Some(efeitos) = ui.d.borrow().as_ref().and_then(|d| d.efeitos_de(area)) else { return };
            ui.pedir(
                move |t| t.definir_luz_de(area, efeitos, &luz),
                move |ui, ()| {
                    if let Some(d) = ui.d.borrow_mut().as_mut() {
                        match (area, d.lateral.as_mut()) {
                            (AREA_LATERAL, Some(l)) => l.luz = luz,
                            _ => d.luz = luz,
                        }
                    }
                },
            );
        });
    }

    fn paleta_mudou(self: &Rc<Self>, area: u8, indice: usize, botao: &gtk::ColorDialogButton) {
        if self.carregando.get() {
            return;
        }
        let atual = self.d.borrow().as_ref().and_then(|d| match area {
            AREA_LATERAL => d.lateral.as_ref().map(|l| l.paleta.clone()),
            _ => Some(d.paleta.clone()),
        });
        let Some(mut paleta) = atual else { return };
        paleta.definir(indice, de_rgba(&botao.rgba()));
        self.pedir(
            move |t| {
                t.definir_paleta_de(area, &paleta)?;
                Ok(paleta)
            },
            move |ui, paleta| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    match (area, d.lateral.as_mut()) {
                        (AREA_LATERAL, Some(l)) => l.paleta = paleta,
                        _ => d.paleta = paleta,
                    }
                }
            },
        );
    }

    // ---------- demonstração ao vivo ----------

    /// Pede ao teclado as cores do momento, se a página está à vista e nada mais está na fila.
    fn atualizar_previa(self: &Rc<Self>) {
        let p = &self.luz;
        let visivel = self.pilha.visible_child_name().as_deref() == Some("luz") && p.area.is_mapped();
        if !visivel || p.previa_pendente.get() || self.ocupado.get() > 0 {
            return;
        }
        let Some(lateral) = self.d.borrow().as_ref().map(|d| d.lateral.as_ref().map(|l| l.area)) else { return };
        p.previa_pendente.set(true);
        self.pedir_bruto(
            false,
            move |t| {
                let grade = t.grade()?;
                let leds = match lateral {
                    Some(area) => Some(t.grade_de(&area)?),
                    None => None,
                };
                Ok((grade, leds))
            },
            |ui, resultado| {
                ui.luz.previa_pendente.set(false);
                if let Ok((grade, leds)) = resultado {
                    *ui.luz.previa.borrow_mut() = Some(grade);
                    *ui.luz.previa_lateral.borrow_mut() = leds;
                    ui.luz.area.queue_draw();
                }
            },
        );
    }

    fn desenhar_previa(&self, cr: &cairo::Context, largura: f64, altura: f64) {
        let dados = self.d.borrow();
        let previa = self.luz.previa.borrow();
        let (Some(d), Some(grade)) = (dados.as_ref(), previa.as_ref()) else { return };
        let leds = self.luz.previa_lateral.borrow();
        let teclas = d.retangulos();
        let total_x = teclas.values().map(|r| r.0 + r.2).fold(1.0, f64::max);
        let teclado_y = teclas.values().map(|r| r.1 + r.3).fold(1.0, f64::max);
        // a faixa lateral ocupa meia tecla de altura, com um respiro, embaixo do teclado
        let total_y = if d.lateral.is_some() { teclado_y + 0.9 } else { teclado_y };
        let u = (largura / total_x).min(altura / total_y);
        let (x0, y0) = ((largura - total_x * u) / 2.0, (altura - total_y * u) / 2.0);
        let folga = u * 0.07;
        // LED apagado aparece como peça escura, não como buraco
        let pintar = |x: f64, y: f64, l: f64, a: f64, raio: f64, c: Rgb| {
            use std::f64::consts::{FRAC_PI_2, PI};
            cr.new_sub_path();
            cr.arc(x + l - raio, y + raio, raio, -FRAC_PI_2, 0.0);
            cr.arc(x + l - raio, y + a - raio, raio, 0.0, FRAC_PI_2);
            cr.arc(x + raio, y + a - raio, raio, FRAC_PI_2, PI);
            cr.arc(x + raio, y + raio, raio, PI, 3.0 * FRAC_PI_2);
            cr.close_path();
            let canal = |v: u8| 0.13 + 0.87 * f64::from(v) / 255.0;
            cr.set_source_rgb(canal(c.0), canal(c.1), canal(c.2));
            let _ = cr.fill();
        };
        for (pos, (x, y, l, a)) in &teclas {
            pintar(x0 + x * u + folga, y0 + y * u + folga, l * u - 2.0 * folga, a * u - 2.0 * folga, u * 0.16, grade.cor(*pos));
        }
        if let Some(lateral) = &d.lateral {
            let n = (lateral.area.linhas as usize * lateral.area.colunas as usize).max(1);
            let passo = total_x * u / n as f64;
            let y = y0 + (teclado_y + 0.4) * u;
            for i in 0..n {
                let cor = leds.as_ref().map_or(Rgb(0, 0, 0), |g| g.cor_do_led(i));
                pintar(x0 + i as f64 * passo + passo * 0.08, y, passo * 0.84, u * 0.4, u * 0.1, cor);
            }
        }
    }
}
