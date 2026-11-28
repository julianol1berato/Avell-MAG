//! Montar os passos de uma macro digitando: um diálogo capta as teclas apertadas e soltas, na
//! ordem e com o tempo entre elas, e entrega a sequência à página Macros. Nada é gravado no
//! teclado aqui.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;
use mag82::{AcaoMacro, MACRO_ESPERA_MAX};

use crate::ui::Ui;
use crate::util::rotulo_longo;

/// Quantas ações uma captura aceita: o teclado guarda 224 ao todo, somando as oito macros.
pub const ACOES_MAX: usize = 120;

/// Até quantos milissegundos apertada uma tecla ainda conta como um toque.
const TOQUE_MAX: u16 = 300;

/// O código de tecla do sistema (o `keycode` do GDK, que é o do evdev mais 8) como a função que o
/// teclado envia (usage HID). `None` para o que não é tecla comum: multimídia, botões do sistema.
pub fn funcao_da_tecla(keycode: u32) -> Option<u16> {
    let evdev = keycode.checked_sub(8)?;
    Some(match evdev {
        1 => 0x29,                                // Esc
        2..=10 => 0x1E + (evdev - 2) as u16,      // 1 a 9
        11 => 0x27,                               // 0
        12 => 0x2D,
        13 => 0x2E,
        14 => 0x2A, // Backspace
        15 => 0x2B, // Tab
        16 => 0x14,
        17 => 0x1A,
        18 => 0x08,
        19 => 0x15,
        20 => 0x17,
        21 => 0x1C,
        22 => 0x18,
        23 => 0x0C,
        24 => 0x12,
        25 => 0x13,
        26 => 0x2F,
        27 => 0x30,
        28 => 0x28, // Enter
        29 => 0xE0, // Ctrl esquerdo
        30 => 0x04,
        31 => 0x16,
        32 => 0x07,
        33 => 0x09,
        34 => 0x0A,
        35 => 0x0B,
        36 => 0x0D,
        37 => 0x0E,
        38 => 0x0F,
        39 => 0x33,
        40 => 0x34,
        41 => 0x35,
        42 => 0xE1, // Shift esquerdo
        43 => 0x31,
        44 => 0x1D,
        45 => 0x1B,
        46 => 0x06,
        47 => 0x19,
        48 => 0x05,
        49 => 0x11,
        50 => 0x10,
        51 => 0x36,
        52 => 0x37,
        53 => 0x38,
        54 => 0xE5, // Shift direito
        55 => 0x55, // * do teclado numérico
        56 => 0xE2, // Alt esquerdo
        57 => 0x2C, // Espaço
        58 => 0x39, // Caps Lock
        59..=68 => 0x3A + (evdev - 59) as u16, // F1 a F10
        69 => 0x53,                            // Num Lock
        70 => 0x47,                            // Scroll Lock
        71 => 0x5F,
        72 => 0x60,
        73 => 0x61,
        74 => 0x56,
        75 => 0x5C,
        76 => 0x5D,
        77 => 0x5E,
        78 => 0x57,
        79 => 0x59,
        80 => 0x5A,
        81 => 0x5B,
        82 => 0x62,
        83 => 0x63,
        86 => 0x64, // a tecla a mais dos teclados ISO e ABNT2, ao lado do Shift esquerdo
        87 => 0x44, // F11
        88 => 0x45, // F12
        89 => 0x87, // a tecla a mais do ABNT2, ao lado do Shift direito
        96 => 0x58, // Enter do teclado numérico
        97 => 0xE4, // Ctrl direito
        98 => 0x54,
        99 => 0x46,  // Print Screen
        100 => 0xE6, // Alt direito
        102 => 0x4A, // Home
        103 => 0x52,
        104 => 0x4B,
        105 => 0x50,
        106 => 0x4F,
        107 => 0x4D, // End
        108 => 0x51,
        109 => 0x4E,
        110 => 0x49, // Insert
        111 => 0x4C, // Delete
        117 => 0x67,
        119 => 0x48, // Pause
        125 => 0xE3, // Super esquerdo
        126 => 0xE7,
        127 => 0x65, // Menu
        183..=194 => 0x68 + (evdev - 183) as u16, // F13 a F24
        _ => return None,
    })
}

/// O que foi digitado até agora: cada tecla apertada ou solta, com o instante em milissegundos.
#[derive(Debug, Default, Clone)]
pub struct Captura {
    eventos: Vec<(u16, bool, u32)>,
    pressionadas: Vec<u16>,
}

impl Captura {
    /// Anota uma tecla apertada ou solta. A repetição automática de quem segura a tecla e o
    /// soltar de uma tecla que não foi vista apertar ficam de fora. Devolve falso se não coube.
    pub fn tecla(&mut self, codigo: u16, pressiona: bool, instante: u32) -> bool {
        let ja = self.pressionadas.iter().position(|c| *c == codigo);
        match (pressiona, ja) {
            (true, Some(_)) | (false, None) => return true,
            // toda tecla apertada ainda vai ser solta: precisa sobrar lugar para isso
            (true, None) if self.eventos.len() + self.pressionadas.len() + 2 > ACOES_MAX => return false,
            (true, None) => self.pressionadas.push(codigo),
            (false, Some(i)) => {
                self.pressionadas.remove(i);
            }
        }
        self.eventos.push((codigo, pressiona, instante));
        true
    }

    pub fn vazia(&self) -> bool {
        self.eventos.is_empty()
    }

    /// A sequência como ações de macro. As teclas que ficaram apertadas são soltas no fim, ou a
    /// macro deixaria a tecla presa no computador. Com `pausas`, cada ação espera o tempo que
    /// passou até a seguinte (o teclado espera depois de agir).
    pub fn acoes(&self, pausas: bool) -> Vec<AcaoMacro> {
        let mut acoes: Vec<AcaoMacro> = Vec::new();
        for (i, (codigo, pressiona, instante)) in self.eventos.iter().enumerate() {
            let espera = match self.eventos.get(i + 1) {
                Some((_, _, seguinte)) if pausas => seguinte.saturating_sub(*instante).min(u32::from(MACRO_ESPERA_MAX)) as u16,
                _ => 0,
            };
            acoes.push(AcaoMacro { pressiona: *pressiona, espera, codigo: *codigo });
        }
        for codigo in self.pressionadas.iter().rev() {
            acoes.push(AcaoMacro { pressiona: false, espera: 0, codigo: *codigo });
        }
        // Um toque rápido fica como toque: o tempo que a tecla ficou apertada passa para depois de
        // soltar, e o ritmo da sequência é o mesmo. Só quem segura a tecla de propósito vira
        // "pressionar, esperar, soltar".
        for i in 1..acoes.len() {
            let (antes, depois) = acoes.split_at_mut(i);
            let (a, b) = (&mut antes[i - 1], &mut depois[0]);
            if a.pressiona && !b.pressiona && a.codigo == b.codigo && a.espera < TOQUE_MAX {
                b.espera = b.espera.saturating_add(a.espera).min(MACRO_ESPERA_MAX);
                a.espera = 0;
            }
        }
        acoes
    }
}

/// O diálogo aberto, para a página Macros e o autoteste falarem com ele.
pub struct Dialogo {
    pub dialogo: adw::Dialog,
    captura: RefCell<Captura>,
    previa: gtk::Label,
    pausas: adw::SwitchRow,
    recomecar: gtk::Button,
    usar: gtk::Button,
}

const CONVITE: &str = "Aperte as teclas agora…";

impl Ui {
    /// Abre o diálogo de digitar a sequência. `ao_usar` recebe as ações quando o usuário aceita.
    pub fn abrir_captura(self: &Rc<Self>, ao_usar: impl Fn(&Rc<Ui>, Vec<AcaoMacro>) + 'static) -> Rc<Dialogo> {
        let previa = gtk::Label::builder()
            .label(CONVITE)
            .wrap(true)
            .xalign(0.0)
            .yalign(0.0)
            .selectable(false)
            .css_classes(["title-4"])
            .build();
        let quadro = gtk::ScrolledWindow::builder()
            .child(&previa)
            .min_content_height(120)
            .max_content_height(220)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .css_classes(["card"])
            .build();
        previa.set_margin_top(12);
        previa.set_margin_bottom(12);
        previa.set_margin_start(14);
        previa.set_margin_end(14);
        let pausas = adw::SwitchRow::builder()
            .title("Guardar as pausas")
            .subtitle("A macro repete o tempo que você levou entre uma tecla e outra")
            .active(true)
            .build();
        let g_pausas = adw::PreferencesGroup::new();
        g_pausas.add(&pausas);
        let explicacao = gtk::Label::builder()
            .label(
                "Digite a sequência como quer que a macro a repita. Apertar e soltar contam em separado, então \
                 combinações como Ctrl + C saem certas.",
            )
            .wrap(true)
            .xalign(0.0)
            .build();
        let nota = gtk::Label::builder()
            .label(
                "Termine com o mouse: aqui o teclado só digita a sequência. Teclas que o sistema usa para si, como \
                 Super e Print Screen, podem não chegar; acrescente essas depois, pela lista de passos.",
            )
            .wrap(true)
            .xalign(0.0)
            .css_classes(["dim-label", "caption"])
            .build();
        let cancelar = gtk::Button::builder().label("Cancelar").build();
        let recomecar = gtk::Button::builder().label("Recomeçar").sensitive(false).build();
        let usar = gtk::Button::builder().label("Usar estes passos").css_classes(["suggested-action"]).sensitive(false).build();
        let botoes = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        botoes.append(&cancelar);
        botoes.append(&recomecar);
        botoes.append(&usar);
        let corpo = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(6)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();
        corpo.append(&explicacao);
        corpo.append(&quadro);
        corpo.append(&g_pausas);
        corpo.append(&nota);
        corpo.append(&botoes);
        let vista = adw::ToolbarView::new();
        vista.add_top_bar(&adw::HeaderBar::builder().show_end_title_buttons(false).show_start_title_buttons(false).build());
        vista.set_content(Some(&corpo));
        let dialogo = adw::Dialog::builder().title("Digitar os passos").content_width(520).child(&vista).build();
        let d = Rc::new(Dialogo { dialogo, captura: RefCell::new(Captura::default()), previa, pausas, recomecar, usar });

        // todas as teclas são da sequência, inclusive Esc, Enter e Tab: nenhuma chega aos botões
        let teclas = gtk::EventControllerKey::new();
        teclas.set_propagation_phase(gtk::PropagationPhase::Capture);
        let (ui, d2) = (self.clone(), d.clone());
        teclas.connect_key_pressed(move |controle, _, keycode, _| {
            ui.captura_tecla(&d2, keycode, true, controle.current_event_time());
            glib::Propagation::Stop
        });
        let (ui, d2) = (self.clone(), d.clone());
        teclas.connect_key_released(move |controle, _, keycode, _| {
            ui.captura_tecla(&d2, keycode, false, controle.current_event_time());
        });
        d.dialogo.add_controller(teclas);

        let d2 = d.clone();
        cancelar.connect_clicked(move |_| {
            d2.dialogo.force_close();
        });
        let (ui, d2) = (self.clone(), d.clone());
        d.recomecar.connect_clicked(move |_| {
            *d2.captura.borrow_mut() = Captura::default();
            ui.mostrar_captura(&d2);
        });
        let (ui, d2) = (self.clone(), d.clone());
        d.pausas.connect_active_notify(move |_| ui.mostrar_captura(&d2));
        let (ui, d2) = (self.clone(), d.clone());
        d.usar.connect_clicked(move |_| {
            let acoes = d2.acoes();
            d2.dialogo.force_close();
            ao_usar(&ui, acoes);
        });
        d.dialogo.present(Some(&self.janela));
        d
    }

    /// Uma tecla apertada ou solta dentro do diálogo.
    fn captura_tecla(&self, d: &Dialogo, keycode: u32, pressiona: bool, instante: u32) {
        let Some(codigo) = funcao_da_tecla(keycode).filter(|c| self.macros.serve(*c)) else {
            if pressiona {
                self.avisar("Essa tecla não pode ser um passo de macro");
            }
            return;
        };
        self.captura_funcao(d, codigo, pressiona, instante);
    }

    /// Como [`Ui::captura_tecla`], já com a função que a tecla envia.
    pub fn captura_funcao(&self, d: &Dialogo, codigo: u16, pressiona: bool, instante: u32) {
        if !d.captura.borrow_mut().tecla(codigo, pressiona, instante) {
            self.avisar("A sequência chegou ao tamanho máximo");
        }
        self.mostrar_captura(d);
    }

    fn mostrar_captura(&self, d: &Dialogo) {
        let vazia = d.captura.borrow().vazia();
        let texto = if vazia { CONVITE.to_string() } else { self.macros.descrever(&d.acoes()) };
        d.previa.set_label(&texto);
        d.recomecar.set_sensitive(!vazia);
        d.usar.set_sensitive(!vazia);
    }
}

impl Dialogo {
    /// A sequência digitada, como ficaria na macro.
    pub fn acoes(&self) -> Vec<AcaoMacro> {
        self.captura.borrow().acoes(self.pausas.is_active())
    }

    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn previa(&self) -> String {
        self.previa.label().to_string()
    }

    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn guardar_pausas(&self, sim: bool) {
        self.pausas.set_active(sim);
    }

    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn recomecar(&self) {
        self.recomecar.emit_clicked();
    }

    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn usar(&self) {
        self.usar.emit_clicked();
    }
}

/// Uma ação escrita para a prévia: "Ctrl esquerdo ↓", "C", "Ctrl esquerdo ↑", "espera 50 ms".
pub fn descrever(acoes: &[AcaoMacro], nome: impl Fn(u16) -> String) -> String {
    let mut partes: Vec<String> = Vec::new();
    let mut i = 0;
    while i < acoes.len() {
        let a = acoes[i];
        // apertar e soltar em seguida é um toque: só o nome da tecla
        let ultima = match acoes.get(i + 1) {
            Some(b) if a.pressiona && !b.pressiona && b.codigo == a.codigo && a.espera == 0 => {
                partes.push(nome(a.codigo));
                i += 2;
                *b
            }
            _ => {
                partes.push(format!("{} {}", nome(a.codigo), if a.pressiona { "↓" } else { "↑" }));
                i += 1;
                a
            }
        };
        if ultima.espera >= 400 && i < acoes.len() {
            partes.push(format!("({:.1} s)", f64::from(ultima.espera) / 1000.0).replace('.', ","));
        }
    }
    partes.join("   ")
}

/// Nome de uma função para a prévia, a partir do nome curto do catálogo.
pub fn nome_na_previa(codigo: u16) -> String {
    rotulo_longo(&mag82::nome_tecla(codigo))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn teclas_do_sistema_viram_funcoes() {
        // keycode do GDK = evdev + 8
        assert_eq!(funcao_da_tecla(8 + 30), Some(0x04)); // A
        assert_eq!(funcao_da_tecla(8 + 1), Some(0x29)); // Esc
        assert_eq!(funcao_da_tecla(8 + 2), Some(0x1E)); // 1
        assert_eq!(funcao_da_tecla(8 + 11), Some(0x27)); // 0
        assert_eq!(funcao_da_tecla(8 + 29), Some(0xE0)); // Ctrl esquerdo
        assert_eq!(funcao_da_tecla(8 + 68), Some(0x43)); // F10
        assert_eq!(funcao_da_tecla(8 + 88), Some(0x45)); // F12
        assert_eq!(funcao_da_tecla(8 + 183), Some(0x68)); // F13
        assert_eq!(funcao_da_tecla(8 + 113), None); // Mudo: multimídia não é tecla comum
        assert_eq!(funcao_da_tecla(3), None);
        // nenhuma função repetida, e todas existem no catálogo
        let mut vistas = std::collections::BTreeSet::new();
        let catalogo = mag82::funcoes();
        for keycode in 0..300 {
            if let Some(codigo) = funcao_da_tecla(keycode) {
                assert!(vistas.insert(codigo), "função {codigo:#x} aparece duas vezes");
                assert!(catalogo.iter().any(|(c, _)| *c == codigo), "função {codigo:#x} fora do catálogo");
            }
        }
        assert!(vistas.len() > 100);
    }

    #[test]
    fn captura_vira_macro_valida() {
        let mut c = Captura::default();
        // Ctrl + C, com repetição automática do Ctrl, e depois V; o Shift fica apertado no fim
        for (codigo, pressiona, instante) in [
            (0xE0, true, 1000),
            (0xE0, true, 1030),
            (0x06, true, 1100),
            (0x06, false, 1150),
            (0xE0, false, 1200),
            (0x19, false, 1210), // soltar sem ter apertado: ignorado
            (0x19, true, 1700),
            (0x19, false, 1760),
            (0xE1, true, 1800),
            (0x04, true, 1900), // o A fica meio segundo apertado: não é toque
            (0x04, false, 2400),
        ] {
            assert!(c.tecla(codigo, pressiona, instante));
        }
        let com = c.acoes(true);
        assert_eq!(mag82::Macro { valida: true, modo: mag82::ModoMacro::TODOS[0], repeticoes: 1, acoes: com.clone() }.validar().map_err(|e| e.to_string()), Ok(()));
        assert_eq!(com.iter().map(|a| a.espera).collect::<Vec<_>>(), [100, 0, 100, 500, 0, 100, 100, 500, 0, 0]);
        assert_eq!(com.last().map(|a| (a.codigo, a.pressiona)), Some((0xE1, false)));
        let sem = c.acoes(false);
        assert!(sem.iter().all(|a| a.espera == 0));
        assert_eq!(mag82::Macro { valida: true, modo: mag82::ModoMacro::TODOS[0], repeticoes: 1, acoes: sem }.em_texto(), "+lctrl c -lctrl v +lshift a -lshift");
        assert_eq!(descrever(&com, |c| mag82::nome_tecla(c)), "lctrl ↓   c   lctrl ↑   (0,5 s)   v   lshift ↓   a ↓   (0,5 s)   a ↑   lshift ↑");
    }

    #[test]
    fn captura_tem_limite() {
        let mut c = Captura::default();
        let mut coube = 0;
        for i in 0..200u32 {
            if c.tecla(0x04, true, i * 10) && c.tecla(0x04, false, i * 10 + 5) {
                coube += 1;
            }
        }
        assert!(c.acoes(false).len() <= ACOES_MAX && coube >= 50);
    }
}
