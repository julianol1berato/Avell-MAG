//! Troca automática de perfil conforme o programa aberto: a linha que se abre no grupo Perfil da
//! página Dispositivo. Aqui só se editam os ajustes; quem troca o perfil é o vigia
//! (`mag82 vigiar`), que roda em segundo plano enquanto o recurso está ligado.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use mag82::automatico::{self, Ajustes};

use crate::ui::Ui;

const DESLIGADO: &str = "Desligado: nada do programa roda em segundo plano";
const LIGADO: &str = "Ligado: um vigia confere os programas abertos a cada dois segundos, mesmo com esta janela fechada";

pub struct PainelAuto {
    pub linha: adw::ExpanderRow,
    /// As linhas das regras, para refazer a lista.
    regras: RefCell<Vec<adw::ActionRow>>,
    /// Programas abertos, na ordem da lista de escolha.
    nomes: RefCell<Vec<String>>,
    pub programa: gtk::DropDown,
    pub perfil: gtk::DropDown,
    nova: adw::ActionRow,
    atualizar: gtk::Button,
    adicionar: gtk::Button,
}

impl PainelAuto {
    pub fn construir() -> Self {
        let linha = adw::ExpanderRow::builder()
            .title("Trocar de perfil conforme o programa aberto")
            .subtitle(DESLIGADO)
            .show_enable_switch(true)
            .enable_expansion(false)
            .build();
        let programa = gtk::DropDown::builder()
            .enable_search(true)
            .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
            .valign(gtk::Align::Center)
            .tooltip_text("Programas abertos agora. Abra o programa ou o jogo para ele aparecer aqui")
            .build();
        programa.set_size_request(200, -1);
        let atualizar = gtk::Button::builder()
            .icon_name("view-refresh-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text("Ler de novo os programas abertos")
            .build();
        let perfil = gtk::DropDown::builder().valign(gtk::Align::Center).build();
        let adicionar = gtk::Button::builder().label("Adicionar").valign(gtk::Align::Center).build();
        let caixa = gtk::Box::builder().spacing(8).valign(gtk::Align::Center).build();
        caixa.append(&programa);
        caixa.append(&atualizar);
        caixa.append(&gtk::Label::builder().label("usa o").css_classes(["dim-label"]).build());
        caixa.append(&perfil);
        caixa.append(&adicionar);
        let nova = adw::ActionRow::builder().title("Nova regra").build();
        nova.add_suffix(&caixa);
        linha.add_row(&nova);
        Self { linha, regras: RefCell::new(Vec::new()), nomes: RefCell::new(Vec::new()), programa, perfil, nova, atualizar, adicionar }
    }

    /// Relê os programas abertos para a lista de escolha, sem o próprio configurador.
    pub fn ler_programas(&self) {
        let nomes: Vec<String> = automatico::programas_para_escolher().into_iter().filter(|n| !n.starts_with("mag82")).collect();
        let nomes_ref: Vec<&str> = nomes.iter().map(String::as_str).collect();
        self.programa.set_model(Some(&gtk::StringList::new(&nomes_ref)));
        *self.nomes.borrow_mut() = nomes;
    }

    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))]
    pub fn quantas_regras(&self) -> usize {
        self.regras.borrow().len()
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let p = &ui.disp.auto;
    let ui2 = ui.clone();
    p.linha.connect_enable_expansion_notify(move |linha| {
        if !ui2.carregando.get() {
            ui2.ligar_automatico(linha.enables_expansion());
        }
    });
    let ui2 = ui.clone();
    p.atualizar.connect_clicked(move |_| ui2.disp.auto.ler_programas());
    let ui2 = ui.clone();
    p.linha.connect_expanded_notify(move |linha| {
        if linha.is_expanded() {
            ui2.disp.auto.ler_programas();
        }
    });
    let ui2 = ui.clone();
    p.adicionar.connect_clicked(move |_| ui2.adicionar_regra());
}

impl Ui {
    /// Mostra os ajustes guardados: o interruptor, as regras e os perfis para escolher.
    pub fn preencher_automatico(self: &Rc<Self>, perfis: &[String]) {
        let p = &self.disp.auto;
        let ajustes = Ajustes::ler();
        let nomes_ref: Vec<&str> = perfis.iter().map(String::as_str).collect();
        let escolhido = p.perfil.selected();
        p.perfil.set_model(Some(&gtk::StringList::new(&nomes_ref)));
        p.perfil.set_selected(if (escolhido as usize) < perfis.len() { escolhido } else { 0 });
        self.silencioso(|| p.linha.set_enable_expansion(ajustes.ligado));
        p.linha.set_subtitle(if ajustes.ligado { LIGADO } else { DESLIGADO });
        if p.nomes.borrow().is_empty() {
            p.ler_programas();
        }
        for antiga in p.regras.borrow_mut().drain(..) {
            p.linha.remove(&antiga);
        }
        // as regras ficam acima da linha de criar: ela sai e volta para o fim
        p.linha.remove(&p.nova);
        for regra in &ajustes.regras {
            let perfil = perfis.get(regra.perfil as usize).cloned().unwrap_or_else(|| format!("Perfil {} (o teclado não tem)", regra.perfil + 1));
            let linha = adw::ActionRow::builder().title(&regra.programa).subtitle(format!("Enquanto estiver aberto, o teclado usa: {perfil}")).build();
            let tirar = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .css_classes(["flat"])
                .valign(gtk::Align::Center)
                .tooltip_text("Tirar esta regra")
                .build();
            let (ui, programa) = (self.clone(), regra.programa.clone());
            tirar.connect_clicked(move |_| ui.remover_regra(&programa));
            linha.add_suffix(&tirar);
            p.linha.add_row(&linha);
            p.regras.borrow_mut().push(linha);
        }
        p.linha.add_row(&p.nova);
    }

    fn perfis_conhecidos(&self) -> Vec<String> {
        self.d.borrow().as_ref().map(|d| d.perfis.clone()).unwrap_or_default()
    }

    /// Liga ou desliga o recurso: guarda o ajuste, cuida do início com a sessão e, ao ligar, põe
    /// o vigia para rodar já.
    pub fn ligar_automatico(self: &Rc<Self>, ligado: bool) {
        let mag82 = automatico::linha_de_comando();
        let mut feito = automatico::ligar(ligado, &mag82);
        // o autoteste confere os ajustes, mas não deixa um vigia rodando na máquina de quem testa
        if ligado && feito.is_ok() && !cfg!(feature = "autoteste") {
            feito = automatico::iniciar_vigia(&mag82);
        }
        match feito {
            Ok(()) if ligado => self.avisar("Troca automática de perfil ligada"),
            Ok(()) => self.avisar("Troca automática de perfil desligada"),
            Err(erro) => self.avisar_erro(&erro.to_string()),
        }
        self.preencher_automatico(&self.perfis_conhecidos());
    }

    pub fn adicionar_regra(self: &Rc<Self>) {
        let p = &self.disp.auto;
        let Some(programa) = p.nomes.borrow().get(p.programa.selected() as usize).cloned() else {
            self.avisar_erro("Abra o programa ou o jogo e clique em atualizar para ele aparecer na lista");
            return;
        };
        let mut ajustes = Ajustes::ler();
        ajustes.definir_regra(&programa, p.perfil.selected() as u8);
        self.guardar_regras(&ajustes, &format!("Regra guardada para {programa}"));
    }

    fn remover_regra(self: &Rc<Self>, programa: &str) {
        let mut ajustes = Ajustes::ler();
        ajustes.remover_regra(programa);
        self.guardar_regras(&ajustes, &format!("Regra de {programa} removida"));
    }

    fn guardar_regras(self: &Rc<Self>, ajustes: &Ajustes, aviso: &str) {
        match ajustes.salvar() {
            Ok(()) => self.avisar(aviso),
            Err(erro) => self.avisar_erro(&erro.to_string()),
        }
        self.preencher_automatico(&self.perfis_conhecidos());
    }
}
