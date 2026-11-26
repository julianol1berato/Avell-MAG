//! Tela de calibração: põe o teclado em modo de calibração, mostra quais teclas já foram
//! pressionadas até o fundo, tira o teclado desse modo ao concluir e deixa o resumo na tela.
//!
//! Em modo de calibração o teclado não digita. Por isso todo caminho de saída (botão, fechar o
//! diálogo, fechar a janela, fim de sessão) encerra a calibração; e se o programa morrer no
//! meio, a próxima execução encerra ao conectar (ver `Ui::conectar`).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{cairo, glib};
use mag82::registro::{Registro, SessaoCalibracao};
use mag82::{Pos, Resultado};

use crate::ui::Ui;
use crate::util::rotulo;

/// Intervalo entre as leituras do estado das teclas durante a calibração.
const LEITURA: Duration = Duration::from_millis(300);

const TEXTO_INICIO: &str = "A calibração mede de novo o curso de cada tecla. Vale a pena refazer se alguma \
    tecla dispara cedo, tarde ou falha.\n\nDepois de iniciar, as teclas acendem em vermelho e o teclado para \
    de digitar. Pressione cada tecla até o fundo, uma vez: ela fica verde. Use o mouse para concluir. \
    As teclas que você não pressionar continuam como estavam.";

/// Widgets do diálogo aberto.
struct Tela {
    dialogo: adw::Dialog,
    area: gtk::DrawingArea,
    texto: gtk::Label,
    progresso: gtk::ProgressBar,
    botao: gtk::Button,
}

#[derive(Default)]
pub struct Calibracao {
    /// Verdadeiro enquanto o teclado está em modo de calibração por causa deste programa.
    pub ativa: Cell<bool>,
    /// A calibração terminou e o diálogo mostra o resumo: o botão só fecha.
    concluida: Cell<bool>,
    tela: RefCell<Option<Tela>>,
    /// Acompanha a calibração em andamento e grava o registro.
    sessao: RefCell<Option<SessaoCalibracao>>,
    /// Tecla -> já calibrada, para o desenho (continua valendo no resumo, depois da sessão).
    estado: RefCell<BTreeMap<Pos, bool>>,
    lendo: Cell<bool>,
}

/// Lê do teclado a marca de calibrada de cada tecla.
fn ler_marcas(t: &mut mag82::Teclado, teclas: &[Pos]) -> Resultado<Vec<(Pos, bool)>> {
    teclas.iter().map(|pos| Ok((*pos, t.atuacao(*pos)?.calibrada()))).collect()
}

impl Ui {
    /// Abre o diálogo de calibração. Nada é enviado ao teclado até o usuário clicar em iniciar.
    pub fn abrir_calibracao(self: &Rc<Self>) {
        if self.cal.tela.borrow().is_some() || self.d.borrow().is_none() {
            return;
        }
        let texto = gtk::Label::builder().label(TEXTO_INICIO).wrap(true).xalign(0.0).selectable(true).build();
        let area = gtk::DrawingArea::builder().content_width(240).content_height(250).hexpand(true).vexpand(true).build();
        let progresso = gtk::ProgressBar::builder().visible(false).build();
        let botao = gtk::Button::builder()
            .label("Iniciar calibração")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        let caixa = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(14)
            .margin_top(6)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();
        caixa.append(&texto);
        caixa.append(&area);
        caixa.append(&progresso);
        caixa.append(&botao);
        let visao = adw::ToolbarView::new();
        visao.add_top_bar(&adw::HeaderBar::new());
        visao.set_content(Some(&caixa));
        let dialogo = adw::Dialog::builder()
            .title("Calibração das teclas")
            .content_width(760)
            .content_height(580)
            .child(&visao)
            .build();

        let ui = self.clone();
        area.set_draw_func(move |_, cr, largura, altura| ui.desenhar_calibracao(cr, f64::from(largura), f64::from(altura)));
        let ui = self.clone();
        botao.connect_clicked(move |_| {
            if ui.cal.concluida.get() {
                ui.fechar_calibracao();
            } else if ui.cal.ativa.get() {
                ui.concluir_calibracao();
            } else {
                ui.iniciar_calibracao();
            }
        });
        // fechar o diálogo por qualquer caminho também tira o teclado do modo de calibração
        let ui = self.clone();
        dialogo.connect_closed(move |_| {
            ui.cal.tela.borrow_mut().take();
            if ui.cal.ativa.get() {
                ui.concluir_calibracao();
            }
        });
        self.cal.concluida.set(false);
        self.cal.estado.borrow_mut().clear();
        dialogo.present(Some(&self.janela));
        *self.cal.tela.borrow_mut() = Some(Tela { dialogo, area, texto, progresso, botao });
    }

    /// Fecha o diálogo de calibração, se estiver aberto.
    pub fn fechar_calibracao(&self) {
        let dialogo = self.cal.tela.borrow().as_ref().map(|t| t.dialogo.clone());
        if let Some(dialogo) = dialogo {
            dialogo.close();
        }
    }

    fn teclas_do_teclado(&self) -> Vec<Pos> {
        self.d.borrow().as_ref().map(|d| d.nomes.keys().copied().collect()).unwrap_or_default()
    }

    fn iniciar_calibracao(self: &Rc<Self>) {
        let teclas = self.teclas_do_teclado();
        if let Some(t) = self.cal.tela.borrow().as_ref() {
            t.botao.set_sensitive(false);
        }
        self.pedir_bruto(
            true,
            move |t| {
                t.iniciar_calibracao()?;
                // se a primeira leitura falhar, não deixa o teclado preso no modo de calibração
                let lido = ler_marcas(t, &teclas);
                if lido.is_err() {
                    let _ = t.encerrar_calibracao();
                }
                lido
            },
            |ui, resultado| match resultado {
                Ok(lido) => {
                    let (nomes, firmware) = match ui.d.borrow().as_ref() {
                        Some(d) => (d.nomes.clone(), d.dispositivo.firmware.clone()),
                        None => (BTreeMap::new(), String::new()),
                    };
                    let caminho = Registro::caminho_padrao("calibracao.log");
                    let sessao = SessaoCalibracao::nova(&caminho, "interface gráfica", &firmware, nomes, &lido);
                    *ui.cal.estado.borrow_mut() = sessao.estado().clone();
                    *ui.cal.sessao.borrow_mut() = Some(sessao);
                    ui.cal.ativa.set(true);
                    ui.mostrar_calibracao();
                    let ui2 = ui.clone();
                    glib::timeout_add_local(LEITURA, move || {
                        if !ui2.cal.ativa.get() {
                            return glib::ControlFlow::Break;
                        }
                        ui2.ler_calibracao();
                        glib::ControlFlow::Continue
                    });
                }
                Err(e) => {
                    mag82::registro::anotar_calibracao(&format!("não foi possível iniciar a calibração: {e}"));
                    ui.avisar_erro(&format!("Não foi possível iniciar a calibração: {e}"));
                    ui.fechar_calibracao();
                }
            },
        );
    }

    /// Lê do teclado quais teclas já foram calibradas.
    fn ler_calibracao(self: &Rc<Self>) {
        if self.cal.lendo.replace(true) {
            return;
        }
        let teclas = self.teclas_do_teclado();
        self.pedir_bruto(
            false,
            move |t| ler_marcas(t, &teclas),
            |ui, resultado| {
                ui.cal.lendo.set(false);
                if !ui.cal.ativa.get() {
                    return;
                }
                if let Some(sessao) = ui.cal.sessao.borrow_mut().as_mut() {
                    match resultado {
                        Ok(lido) => {
                            sessao.atualizar(&lido);
                            *ui.cal.estado.borrow_mut() = sessao.estado().clone();
                        }
                        Err(e) => sessao.erro(&format!("leitura das marcas falhou: {e}")),
                    }
                }
                ui.mostrar_calibracao();
            },
        );
    }

    /// Atualiza texto, barra e desenho do diálogo com o estado lido.
    fn mostrar_calibracao(&self) {
        let tela = self.cal.tela.borrow();
        let Some(t) = tela.as_ref() else { return };
        let sessao = self.cal.sessao.borrow();
        let Some(s) = sessao.as_ref() else { return };
        t.botao.set_sensitive(true);
        t.botao.set_label("Concluir calibração");
        t.area.queue_draw();
        if s.sem_progresso() {
            t.texto.set_label(
                "Calibração em andamento. Pressione cada tecla até o fundo e acompanhe pelo próprio teclado: \
                 as teclas passam de vermelho para verde. Quando terminar, conclua com o mouse.",
            );
            return;
        }
        let (feitas, total) = s.contagem();
        t.progresso.set_visible(true);
        t.progresso.set_fraction(if total == 0 { 0.0 } else { feitas as f64 / total as f64 });
        t.texto.set_label(&if feitas == total {
            "Todas as teclas foram calibradas. Conclua para o teclado voltar a digitar.".to_string()
        } else {
            format!(
                "{feitas} de {total} teclas calibradas. Pressione até o fundo as que ainda estão em vermelho; \
                 o teclado só volta a digitar depois de concluir (use o mouse)."
            )
        });
    }

    /// Tira o teclado do modo de calibração, confere as marcas, grava o registro e mostra o
    /// resumo: no diálogo, se ele ainda está aberto, ou num aviso demorado.
    pub fn concluir_calibracao(self: &Rc<Self>) {
        if !self.cal.ativa.replace(false) {
            return;
        }
        if let Some(t) = self.cal.tela.borrow().as_ref() {
            t.botao.set_sensitive(false);
        }
        let teclas = self.teclas_do_teclado();
        self.pedir_bruto(
            true,
            move |t| {
                let encerrado = t.encerrar_calibracao();
                Ok((encerrado, ler_marcas(t, &teclas).ok()))
            },
            |ui, resultado| {
                let (encerrado, depois) = match resultado {
                    Ok(par) => par,
                    Err(e) => (Err(e), None),
                };
                let mut sessao = ui.cal.sessao.borrow_mut().take();
                if let (Some(s), Err(e)) = (sessao.as_mut(), &encerrado) {
                    s.erro(&format!("não foi possível encerrar o modo de calibração: {e}"));
                }
                let resumo = sessao.map(|s| s.encerrar(depois.as_deref()));
                if let Some(depois) = depois {
                    *ui.cal.estado.borrow_mut() = depois.into_iter().collect();
                }
                let texto = match (&encerrado, &resumo) {
                    (Err(e), _) => format!(
                        "Não foi possível tirar o teclado do modo de calibração ({e}).\n\nFeche o programa e abra \
                         de novo: ao abrir, ele encerra a calibração sozinho e o teclado volta a digitar."
                    ),
                    (Ok(()), Some(r)) => {
                        let arquivo = r.registro.as_ref().map(|p| format!("\n\nRegistro gravado em {}", p.display()));
                        format!("Calibração encerrada. O teclado voltou a digitar.\n\n{}{}", r.texto(), arquivo.unwrap_or_default())
                    }
                    (Ok(()), None) => "Calibração encerrada. O teclado voltou a digitar.".to_string(),
                };
                ui.cal.concluida.set(true);
                match ui.cal.tela.borrow().as_ref() {
                    Some(t) => {
                        t.texto.set_label(&texto);
                        t.botao.set_label("Fechar");
                        t.botao.set_sensitive(true);
                        t.area.queue_draw();
                    }
                    // o diálogo já foi fechado: o resumo vai num aviso que dá tempo de ler
                    None => ui.avisar_demorado(&texto.replace("\n\n", " ")),
                }
                ui.conectar(false);
            },
        );
    }

    /// O teclado sumiu no meio da calibração: fecha a sessão. O marcador em disco continua, e é
    /// ele que faz a calibração ser encerrada quando o teclado for conectado de novo.
    pub fn calibracao_perdeu_teclado(&self) {
        if !self.cal.ativa.replace(false) {
            return;
        }
        if let Some(mut s) = self.cal.sessao.borrow_mut().take() {
            s.erro("teclado desconectado durante a calibração");
            s.encerrar(None);
        }
        self.fechar_calibracao();
    }

    /// Ao conectar, o programa encontrou e encerrou uma calibração que tinha ficado aberta.
    pub fn avisar_calibracao_interrompida(self: &Rc<Self>) {
        let aviso = adw::AlertDialog::new(
            Some("A calibração anterior foi interrompida"),
            Some(
                "O programa foi fechado, ou o teclado desconectado, no meio de uma calibração. O teclado já saiu do \
                 modo de calibração e voltou a digitar.\n\nAs teclas que não chegaram a ser pressionadas podem não \
                 estar bem calibradas. Se alguma falhar, refaça a calibração.",
            ),
        );
        aviso.add_response("depois", "Agora não");
        aviso.add_response("calibrar", "Calibrar agora");
        aviso.set_response_appearance("calibrar", adw::ResponseAppearance::Suggested);
        aviso.set_default_response(Some("depois"));
        aviso.set_close_response("depois");
        let ui = self.clone();
        aviso.connect_response(None, move |_, resposta| {
            if resposta == "calibrar" {
                ui.abrir_calibracao();
            }
        });
        aviso.present(Some(&self.janela));
    }

    fn desenhar_calibracao(&self, cr: &cairo::Context, largura: f64, altura: f64) {
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let estado = self.cal.estado.borrow();
        let teclas = d.retangulos();
        let total_x = teclas.values().map(|r| r.0 + r.2).fold(1.0, f64::max);
        let total_y = teclas.values().map(|r| r.1 + r.3).fold(1.0, f64::max);
        let u = (largura / total_x).min(altura / total_y);
        let (x0, y0) = ((largura - total_x * u) / 2.0, (altura - total_y * u) / 2.0);
        let (folga, raio) = (u * 0.06, u * 0.16);
        cr.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
        cr.set_font_size((u * 0.24).max(7.0));
        for (pos, (x, y, l, a)) in &teclas {
            use std::f64::consts::{FRAC_PI_2, PI};
            let (x, y, l, a) = (x0 + x * u + folga, y0 + y * u + folga, l * u - 2.0 * folga, a * u - 2.0 * folga);
            cr.new_sub_path();
            cr.arc(x + l - raio, y + raio, raio, -FRAC_PI_2, 0.0);
            cr.arc(x + l - raio, y + a - raio, raio, 0.0, FRAC_PI_2);
            cr.arc(x + raio, y + a - raio, raio, FRAC_PI_2, PI);
            cr.arc(x + raio, y + raio, raio, PI, 3.0 * FRAC_PI_2);
            cr.close_path();
            // antes de iniciar: cinza; calibrando: vermelho até a tecla ser pressionada, depois verde
            match estado.get(pos) {
                None => cr.set_source_rgb(0.30, 0.30, 0.33),
                Some(true) => cr.set_source_rgb(0.18, 0.62, 0.30),
                Some(false) => cr.set_source_rgb(0.80, 0.22, 0.20),
            }
            let _ = cr.fill();
            // a fonte simples do desenho não tem as setas
            let nome = match rotulo(&d.nomes[pos]).as_str() {
                "←" => "<".to_string(),
                "→" => ">".to_string(),
                "↑" => "^".to_string(),
                "↓" => "v".to_string(),
                outro => outro.to_string(),
            };
            if let Ok(medida) = cr.text_extents(&nome) {
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.move_to(x + (l - medida.width()) / 2.0 - medida.x_bearing(), y + (a + medida.height()) / 2.0);
                let _ = cr.show_text(&nome);
            }
        }
    }
}
