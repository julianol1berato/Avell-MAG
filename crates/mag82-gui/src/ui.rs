//! Janela principal, conexão com o teclado e vigilância do que muda fora do programa.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib};
use mag82::{Erro, Resultado, Teclado};

use crate::calibracao::Calibracao;
use crate::dados::Dados;
use crate::dispositivo::PainelDisp;
use crate::fila::Fila;
use crate::luz::PainelLuz;
use crate::teclas::Teclas;

const CSS_BASE: &str = "
.tecla { padding: 0; min-width: 0; min-height: 0; font-size: 12px; font-weight: 600; }
.tecla.rt { box-shadow: inset 0 -3px 0 @accent_bg_color; }
.tecla.socd { box-shadow: inset 0 3px 0 @warning_bg_color; }
.tecla.dks { box-shadow: inset 0 3px 0 @success_bg_color; }
.tecla.rt.dks { box-shadow: inset 0 -3px 0 @accent_bg_color, inset 0 3px 0 @success_bg_color; }
.tecla.rt.socd { box-shadow: inset 0 -3px 0 @accent_bg_color, inset 0 3px 0 @warning_bg_color; }
.tecla:checked { outline: 2px solid @accent_color; outline-offset: -2px; }
.tecla .sub { font-size: 9px; font-weight: 400; opacity: 0.8; }
.painel-teclado { padding: 12px; border-radius: 14px; background: alpha(@window_fg_color, 0.06); }
.legenda { font-size: 12px; opacity: 0.75; }
.led { padding: 0; min-width: 0; min-height: 30px; border-radius: 5px; }
.led:checked { outline: 2px solid @accent_color; outline-offset: -2px; }
.legenda-rt { color: @accent_color; font-weight: 900; }
.legenda-socd { color: @warning_color; font-weight: 900; }
.legenda-dks { color: @success_color; font-weight: 900; }
";

/// Ligado quando o sistema pede para o programa encerrar (SIGHUP, SIGINT, SIGTERM).
static ENCERRAR: AtomicBool = AtomicBool::new(false);

extern "C" fn ao_encerrar(_: libc::c_int) {
    ENCERRAR.store(true, Ordering::SeqCst);
}

/// Site do autor, mostrado na janela Sobre e na página Dispositivo.
pub const SITE_AUTOR: &str = "https://9level.com.br";
pub const AUTOR_E_CIDADE: &str = "Juliano Liberato · Joinville - SC";

/// Menor janela em que tudo continua utilizável.
pub const LARGURA_MIN: i32 = 540;
pub const ALTURA_MIN: i32 = 580;

pub struct Ui {
    pub fila: Fila,
    /// O que foi lido do teclado; `None` enquanto não há teclado conectado.
    pub d: RefCell<Option<Dados>>,
    /// Verdadeiro enquanto os controles são preenchidos pelo programa (ignora os sinais de mudança).
    pub carregando: Cell<bool>,
    /// Tarefas na fila do teclado ainda sem resposta.
    pub ocupado: Cell<u32>,
    /// Dessas, as que o usuário pediu (mostram o indicador de atividade).
    visiveis: Cell<u32>,
    pub conectando: Cell<bool>,

    pub janela: adw::ApplicationWindow,
    raiz: gtk::Stack,
    pub pilha: adw::ViewStack,
    vazio: adw::StatusPage,
    avisos: adw::ToastOverlay,
    /// Aviso que está na tela: sai quando chega um novo, para não formar fila de avisos velhos.
    aviso_atual: RefCell<Option<adw::Toast>>,
    girando: gtk::Spinner,
    /// "Gravando…" ao lado do indicador, enquanto uma mudança espera a confirmação do teclado.
    gravando: gtk::Label,
    gravacoes: Cell<u32>,
    pub perfil: gtk::DropDown,

    pub tec: Teclas,
    pub dks: crate::dks::PainelDks,
    pub luz: PainelLuz,
    pub disp: PainelDisp,
    pub cal: Calibracao,

    /// Faixa no topo da janela que anuncia uma versão nova do programa.
    pub faixa_atualizacao: adw::Banner,
    pub fase_atualizacao: Cell<crate::atualizacao::Fase>,
    pub versao_nova: RefCell<Option<crate::atualizacao::Versao>>,
}

impl Ui {
    pub fn construir(app: &adw::Application) -> Rc<Self> {
        let css = gtk::CssProvider::new();
        css.load_from_string(CSS_BASE);
        let tela = gdk::Display::default().expect("sem display gráfico");
        gtk::style_context_add_provider_for_display(&tela, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

        let (tec, pagina_teclas, dks) = Teclas::construir(&tela);
        let (luz, pagina_luz) = PainelLuz::construir();
        let (disp, pagina_disp) = PainelDisp::construir();

        let pilha = adw::ViewStack::new();
        pilha.add_titled_with_icon(&pagina_teclas, Some("teclas"), "Teclas", "input-keyboard-symbolic");
        pilha.add_titled_with_icon(&pagina_luz, Some("luz"), "Iluminação", "preferences-color-symbolic");
        pilha.add_titled_with_icon(&pagina_disp, Some("disp"), "Dispositivo", "computer-symbolic");

        let alternador = adw::ViewSwitcher::builder().stack(&pilha).policy(adw::ViewSwitcherPolicy::Wide).build();
        let barra = adw::HeaderBar::builder().title_widget(&alternador).build();
        let recarregar = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .tooltip_text("Reler tudo do teclado (F5)")
            .build();
        let girando = gtk::Spinner::new();
        barra.pack_start(&recarregar);
        barra.pack_start(&girando);
        let gravando = gtk::Label::builder().label("Gravando…").css_classes(["dim-label", "caption"]).visible(false).build();
        barra.pack_start(&gravando);
        let sobre = gtk::Button::builder().icon_name("help-about-symbolic").tooltip_text("Sobre o MAG82").build();
        barra.pack_end(&sobre);
        let perfil = gtk::DropDown::from_strings(&[]);
        perfil.set_tooltip_text(Some("Perfil ativo"));
        barra.pack_end(&perfil);

        let vazio =
            adw::StatusPage::builder().icon_name("input-keyboard-symbolic").title("Teclado não encontrado").build();
        let tentar = gtk::Button::builder()
            .label("Tentar de novo")
            .halign(gtk::Align::Center)
            .css_classes(["pill", "suggested-action"])
            .build();
        vazio.set_child(Some(&tentar));

        let raiz = gtk::Stack::new();
        raiz.add_named(&vazio, Some("vazio"));
        raiz.add_named(&pilha, Some("ok"));
        let avisos = adw::ToastOverlay::new();
        avisos.set_child(Some(&raiz));
        let visao = adw::ToolbarView::new();
        visao.add_top_bar(&barra);
        let faixa_atualizacao = adw::Banner::new("");
        visao.add_top_bar(&faixa_atualizacao);
        visao.set_content(Some(&avisos));
        // em janela estreita as abas saem do título e vão para uma barra embaixo
        let abas_embaixo = adw::ViewSwitcherBar::builder().stack(&pilha).build();
        visao.add_bottom_bar(&abas_embaixo);
        let estreita = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            700.0,
            adw::LengthUnit::Sp,
        ));
        estreita.add_setter(&abas_embaixo, "reveal", Some(&true.to_value()));
        estreita.add_setter(&barra, "title-widget", Some(&None::<gtk::Widget>.to_value()));
        let janela = adw::ApplicationWindow::builder()
            .application(app)
            .title("MAG82")
            .default_width(1020)
            .default_height(880)
            .width_request(LARGURA_MIN)
            .height_request(ALTURA_MIN)
            .content(&visao)
            .build();
        janela.add_breakpoint(estreita);

        let ui = Rc::new(Self {
            fila: Fila::nova(),
            d: RefCell::new(None),
            carregando: Cell::new(false),
            ocupado: Cell::new(0),
            visiveis: Cell::new(0),
            conectando: Cell::new(false),
            janela,
            raiz,
            pilha,
            vazio,
            avisos,
            aviso_atual: RefCell::new(None),
            girando,
            gravando,
            gravacoes: Cell::new(0),
            perfil,
            tec,
            dks,
            luz,
            disp,
            cal: Calibracao::default(),
            faixa_atualizacao,
            fase_atualizacao: Cell::default(),
            versao_nova: RefCell::new(None),
        });

        // Encerramento pedido pelo sistema (fim de sessão, desligar, Ctrl+C): encerra a calibração,
        // se houver uma em andamento, e sai.
        for sinal in [libc::SIGHUP, libc::SIGINT, libc::SIGTERM] {
            // SAFETY: o tratador só grava num booleano atômico.
            unsafe { libc::signal(sinal, ao_encerrar as extern "C" fn(libc::c_int) as libc::sighandler_t) };
        }
        let ui2 = ui.clone();
        glib::timeout_add_local(Duration::from_millis(200), move || {
            if ENCERRAR.swap(false, Ordering::SeqCst) {
                ui2.encerrar_programa();
            }
            glib::ControlFlow::Continue
        });

        // fechar a janela no meio de uma calibração: primeiro tira o teclado desse modo
        let ui2 = ui.clone();
        ui.janela.connect_close_request(move |janela| {
            if !ui2.cal.ativa.get() {
                return glib::Propagation::Proceed;
            }
            ui2.concluir_calibracao();
            let janela = janela.clone();
            let ui3 = ui2.clone();
            glib::spawn_future_local(async move {
                while ui3.ocupado.get() > 0 {
                    glib::timeout_future(Duration::from_millis(30)).await;
                }
                janela.close();
            });
            glib::Propagation::Stop
        });

        let ui2 = ui.clone();
        sobre.connect_clicked(move |_| ui2.mostrar_sobre());
        let ui2 = ui.clone();
        recarregar.connect_clicked(move |_| ui2.conectar(false));
        let ui2 = ui.clone();
        tentar.connect_clicked(move |_| ui2.conectar(false));
        let ui2 = ui.clone();
        ui.perfil.connect_selected_notify(move |seletor| {
            if !ui2.carregando.get() {
                ui2.trocar_perfil(seletor.selected() as u8);
            }
        });
        crate::teclas::ligar(&ui);
        crate::dks::ligar(&ui);
        crate::luz::ligar(&ui);
        crate::dispositivo::ligar(&ui);
        crate::atualizacao::ligar(&ui);
        Ui::ligar_atalhos(&ui);

        // o teclado pode ser conectado, desconectado ou trocar de perfil sem o programa saber
        let ui2 = ui.clone();
        glib::timeout_add_local(Duration::from_millis(1500), move || {
            ui2.vigiar();
            glib::ControlFlow::Continue
        });

        ui.conectar(false);
        ui
    }

    fn ligar_atalhos(ui: &Rc<Self>) {
        let teclas = gtk::EventControllerKey::new();
        let ui2 = ui.clone();
        teclas.connect_key_pressed(move |_, tecla, _, estado| {
            let ctrl = estado.contains(gdk::ModifierType::CONTROL_MASK);
            let nas_teclas = ui2.pilha.visible_child_name().as_deref() == Some("teclas");
            if tecla == gdk::Key::F5 || (ctrl && tecla == gdk::Key::r) {
                ui2.conectar(false);
            } else if ctrl && tecla == gdk::Key::a && nas_teclas && ui2.tec.modo.get().limite().is_none() {
                ui2.marcar(true);
            } else if tecla == gdk::Key::Escape && nas_teclas {
                ui2.marcar(false);
            } else {
                return glib::Propagation::Proceed;
            }
            glib::Propagation::Stop
        });
        ui.janela.add_controller(teclas);
    }

    /// Sai do programa a pedido do sistema. Fechar a janela não basta: com um diálogo aberto,
    /// o pedido de fechar só fecha o diálogo. Antes de sair, encerra a calibração, se houver.
    pub(crate) fn encerrar_programa(self: &Rc<Self>) {
        self.concluir_calibracao();
        let ui = self.clone();
        glib::spawn_future_local(async move {
            while ui.ocupado.get() > 0 {
                glib::timeout_future(Duration::from_millis(30)).await;
            }
            if let Some(app) = ui.janela.application() {
                app.quit();
            }
        });
    }

    pub fn mostrar_sobre(&self) {
        let sobre = adw::AboutDialog::builder()
            .application_name("MAG82 para Linux")
            .application_icon("io.github.mag82linux")
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name(AUTOR_E_CIDADE)
            .comments(
                "Tudo o que o Avell Volcano's MAG82 faz, agora no Linux: atuação e Rapid Trigger por tecla, \
                 remapeamento, SOCD, luzes das teclas e da lateral, perfis e calibração.\n\n\
                 O teclado só tinha aplicativo para Windows. Este programa foi escrito do zero, de forma \
                 independente, para quem usa Linux não ficar de fora.\n\n\
                 Desenvolvimento independente, sem vínculo com a Avell.",
            )
            .website(SITE_AUTOR)
            .support_url(concat!(env!("CARGO_PKG_REPOSITORY"), "#readme"))
            .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"))
            .developers(["Juliano Liberato https://9level.com.br"])
            .copyright("© 2026 Juliano Liberato · Joinville - SC")
            .license_type(gtk::License::MitX11)
            .release_notes_version(env!("CARGO_PKG_VERSION"))
            .release_notes(
                "<p>Primeira versão pública.</p>\
                 <ul>\
                 <li>Atuação, Rapid Trigger e zonas mortas por tecla, com predefinições</li>\
                 <li>Remapeamento, SOCD e cor fixa por tecla</li>\
                 <li>Efeitos e paleta das teclas e da faixa lateral, com demonstração ao vivo</li>\
                 <li>Perfis, taxa de resposta e calibração com registro</li>\
                 <li>Pacotes para Ubuntu, Debian e Fedora</li>\
                 </ul>",
            )
            .build();
        sobre.add_link("9level.com.br — site do autor", SITE_AUTOR);
        sobre.add_link("Juliano Liberato no GitHub", "https://github.com/julianol1berato");
        sobre.add_link("Código-fonte do projeto", env!("CARGO_PKG_REPOSITORY"));
        sobre.add_link("Como contribuir", concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/CONTRIBUTING.md"));
        sobre.present(Some(&self.janela));
    }

    // ---------- infraestrutura ----------

    pub fn avisar(&self, texto: &str) {
        self.mostrar_aviso(texto, 3);
    }

    /// Erros ficam mais tempo na tela que as confirmações.
    pub fn avisar_erro(&self, texto: &str) {
        self.mostrar_aviso(texto, 8);
    }

    fn mostrar_aviso(&self, texto: &str, segundos: u32) {
        let aviso = adw::Toast::builder().title(texto).timeout(segundos).build();
        if let Some(anterior) = self.aviso_atual.replace(Some(aviso.clone())) {
            anterior.dismiss();
        }
        self.avisos.add_toast(aviso);
    }

    /// Para mensagens que o usuário precisa de tempo para ler.
    pub fn avisar_demorado(&self, texto: &str) {
        self.mostrar_aviso(texto, 20);
    }

    /// Tira da tela o aviso que estiver aparecendo.
    pub fn limpar_aviso(&self) {
        if let Some(aviso) = self.aviso_atual.take() {
            aviso.dismiss();
        }
    }

    /// Nome da tela à mostra: "ok" (teclado conectado) ou "vazio" (tela de espera).
    pub fn tela(&self) -> String {
        self.raiz.visible_child_name().map(|n| n.to_string()).unwrap_or_default()
    }

    /// Roda `f` sem que as mudanças nos controles sejam tratadas como ação do usuário.
    pub fn silencioso<R>(&self, f: impl FnOnce() -> R) -> R {
        let antes = self.carregando.replace(true);
        let r = f();
        self.carregando.set(antes);
        r
    }

    fn contar(&self, visivel: bool, entrou: bool) {
        let ajustar = |c: &Cell<u32>| c.set(if entrou { c.get() + 1 } else { c.get().saturating_sub(1) });
        ajustar(&self.ocupado);
        if visivel {
            ajustar(&self.visiveis);
            self.girando.set_spinning(self.visiveis.get() > 0);
        }
    }

    /// Roda `trabalho` na thread do teclado e entrega o resultado a `depois`, de volta na
    /// thread da interface. `visivel` liga o indicador de atividade.
    pub fn pedir_bruto<T: Send + 'static>(
        self: &Rc<Self>,
        visivel: bool,
        trabalho: impl FnOnce(&mut Teclado) -> Resultado<T> + Send + 'static,
        depois: impl FnOnce(&Rc<Self>, Resultado<T>) + 'static,
    ) {
        let rx = self.fila.enviar(move |t| match t.as_mut() {
            Some(t) => trabalho(t),
            None => Err(Erro::NaoEncontrado),
        });
        self.contar(visivel, true);
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let resultado = rx.await;
            ui.contar(visivel, false);
            if let Ok(resultado) = resultado {
                depois(&ui, resultado);
            }
        });
    }

    /// Como [`Ui::pedir_bruto`], para ações do usuário. Se a operação falhar, avisa e relê o
    /// teclado, para a tela voltar a mostrar o que está de fato gravado nele.
    pub fn pedir<T: Send + 'static>(
        self: &Rc<Self>,
        trabalho: impl FnOnce(&mut Teclado) -> Resultado<T> + Send + 'static,
        depois: impl FnOnce(&Rc<Self>, T) + 'static,
    ) {
        self.gravacoes.set(self.gravacoes.get() + 1);
        self.gravando.set_visible(true);
        self.pedir_bruto(true, trabalho, |ui, resultado| {
            ui.gravacoes.set(ui.gravacoes.get().saturating_sub(1));
            ui.gravando.set_visible(ui.gravacoes.get() > 0);
            match resultado {
                Ok(valor) => depois(ui, valor),
                Err(e) => {
                    ui.avisar_erro(&e.to_string());
                    ui.conectar(true);
                }
            }
        });
    }

    // ---------- conexão ----------

    /// Abre o teclado e lê tudo. `manter_edicoes` preserva o que o usuário editou e ainda não
    /// aplicou (usado quando a releitura é só para conferir o estado depois de uma falha).
    pub fn conectar(self: &Rc<Self>, manter_edicoes: bool) {
        if self.conectando.replace(true) {
            return;
        }
        let verboso = std::env::var_os("MAG82_VERBOSO").is_some();
        // Uma calibração que não foi encerrada (o programa morreu no meio) deixa o teclado sem
        // digitar: a primeira coisa a fazer ao conectar é tirá-lo desse modo.
        let recuperar = !self.cal.ativa.get() && mag82::registro::calibracao_pendente();
        let rx = self.fila.enviar(move |t| -> Resultado<(Dados, bool)> {
            *t = None;
            let mut novo = Teclado::abrir()?;
            novo.verboso = verboso;
            if recuperar {
                novo.encerrar_calibracao()?;
                mag82::registro::anotar_calibracao(
                    "calibração anterior não tinha sido encerrada: modo de calibração encerrado ao conectar",
                );
            }
            let dados = Dados::ler(&mut novo)?;
            *t = Some(novo);
            Ok((dados, recuperar))
        });
        self.contar(true, true);
        let ui = self.clone();
        glib::spawn_future_local(async move {
            let resultado = rx.await;
            ui.contar(true, false);
            ui.conectando.set(false);
            match resultado {
                Ok(Ok((dados, recuperada))) => {
                    ui.carregar(dados, manter_edicoes);
                    if recuperada {
                        ui.avisar_calibracao_interrompida();
                    }
                }
                Ok(Err(e)) => ui.sem_teclado(&e.to_string()),
                Err(_) => {}
            }
        });
    }

    /// Mostra na tela um estado recém-lido do teclado.
    pub fn carregar(self: &Rc<Self>, dados: Dados, manter_edicoes: bool) {
        self.silencioso(|| {
            self.preencher_luz(&dados);
            self.preencher_dispositivo(&dados);
            self.preencher_perfis(&dados.perfis, dados.perfil_atual);
        });
        *self.luz.previa.borrow_mut() = Some(dados.grade.clone());
        *self.d.borrow_mut() = Some(dados);
        self.montar_teclado();
        if !manter_edicoes {
            self.esquecer_edicoes();
        }
        self.pintar_teclas();
        self.listar_atalhos();
        self.atualizar_painel();
        self.luz.area.queue_draw();
        self.raiz.set_visible_child_name("ok");
    }

    pub fn sem_teclado(&self, motivo: &str) {
        *self.d.borrow_mut() = None;
        self.vazio.set_description(Some(motivo));
        self.raiz.set_visible_child_name("vazio");
    }

    /// Chamado a cada 1,5 s: conecta quando o teclado aparece, mostra a tela de espera quando
    /// ele some e relê tudo se o perfil foi trocado no próprio teclado ou por outro programa.
    fn vigiar(self: &Rc<Self>) {
        let presente = mag82::interfaces().contains_key(&mag82::CANAL);
        let conectado = self.d.borrow().is_some();
        if !presente {
            self.calibracao_perdeu_teclado();
            if conectado {
                drop(self.fila.enviar(|t| *t = None));
                self.sem_teclado(&Erro::Desconectado.to_string());
            }
            return;
        }
        if !conectado {
            self.conectar(false);
            return;
        }
        if self.ocupado.get() > 0 || self.cal.ativa.get() {
            return;
        }
        self.pedir_bruto(
            false,
            |t| t.perfis(),
            |ui, resultado| {
                let conhecido = ui.d.borrow().as_ref().map(|d| d.perfil_atual);
                if let (Ok((_, atual)), Some(conhecido)) = (resultado, conhecido) {
                    if atual != conhecido && ui.ocupado.get() == 0 {
                        ui.avisar("O perfil foi trocado no teclado");
                        ui.conectar(false);
                    }
                }
            },
        );
    }

    // ---------- perfis ----------

    pub fn preencher_perfis(&self, nomes: &[String], atual: u8) {
        let nomes_ref: Vec<&str> = nomes.iter().map(String::as_str).collect();
        self.perfil.set_model(Some(&gtk::StringList::new(&nomes_ref)));
        self.perfil.set_selected(u32::from(atual));
        self.disp.nome_perfil.set_text(nomes.get(atual as usize).map(String::as_str).unwrap_or(""));
    }

    pub fn trocar_perfil(self: &Rc<Self>, indice: u8) {
        if self.d.borrow().as_ref().map(|d| d.perfil_atual) == Some(indice) {
            return;
        }
        // o perfil muda tudo: relê teclas, luz e demais ajustes
        self.pedir(
            move |t| {
                t.definir_perfil(indice)?;
                Dados::ler(t)
            },
            |ui, dados| {
                ui.carregar(dados, false);
                ui.avisar("Perfil trocado");
            },
        );
    }
}
