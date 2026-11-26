//! Página Dispositivo: nome do perfil, taxa de resposta e informações do teclado.

use std::rc::Rc;

use adw::prelude::*;

use crate::dados::Dados;
use crate::ui::Ui;

pub struct PainelDisp {
    linha_site: adw::ActionRow,
    linha_sobre: adw::ActionRow,
    calibrar: gtk::Button,
    pub atualizar_auto: adw::SwitchRow,
    pub procurar_atualizacao: adw::ActionRow,
    pub taxa: adw::ComboRow,
    pub nome_perfil: adw::EntryRow,
    info: Vec<adw::ActionRow>,
}

impl PainelDisp {
    pub fn construir() -> (Self, adw::PreferencesPage) {
        let taxa = adw::ComboRow::builder()
            .title("Taxa de resposta")
            .subtitle("Quantas vezes por segundo o teclado informa o computador")
            .build();
        let g_desemp = adw::PreferencesGroup::builder().title("Desempenho").build();
        g_desemp.add(&taxa);

        let g_perfil = adw::PreferencesGroup::builder()
            .title("Perfil")
            .description("Cada perfil guarda um conjunto completo de ajustes; troque pelo seletor no topo da janela")
            .build();
        let nome_perfil = adw::EntryRow::builder().title("Nome do perfil ativo").show_apply_button(true).build();
        g_perfil.add(&nome_perfil);

        let g_sobre = adw::PreferencesGroup::builder().title("Sobre o teclado").build();
        let info: Vec<adw::ActionRow> = ["Modelo", "Firmware", "Data do firmware", "Protocolo", "Tipo de switch"]
            .iter()
            .map(|titulo| {
                let linha = adw::ActionRow::builder().title(*titulo).css_classes(["property"]).build();
                g_sobre.add(&linha);
                linha
            })
            .collect();

        let g_cal = adw::PreferencesGroup::builder().title("Manutenção").build();
        let calibrar = gtk::Button::builder().label("Calibrar…").valign(gtk::Align::Center).build();
        let linha_cal = adw::ActionRow::builder()
            .title("Calibração das teclas")
            .subtitle("Mede de novo o curso de cada tecla; refaça se alguma dispara cedo, tarde ou falha")
            .build();
        linha_cal.add_suffix(&calibrar);
        g_cal.add(&linha_cal);

        // quem fez o programa, logo depois das informações do teclado
        let g_programa = adw::PreferencesGroup::builder().title("Sobre o programa").build();
        let autoria = adw::ActionRow::builder()
            .title("Desenvolvimento independente")
            .subtitle(crate::ui::AUTOR_E_CIDADE)
            .css_classes(["property"])
            .build();
        let linha_site = adw::ActionRow::builder()
            .title("9level.com.br")
            .subtitle("Conheça o trabalho do autor")
            .activatable(true)
            .build();
        linha_site.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
        let linha_sobre = adw::ActionRow::builder()
            .title("Sobre o MAG82 para Linux")
            .subtitle(concat!("Versão ", env!("CARGO_PKG_VERSION"), ", código-fonte e como contribuir"))
            .activatable(true)
            .build();
        linha_sobre.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        g_programa.add(&autoria);
        g_programa.add(&linha_site);
        g_programa.add(&linha_sobre);

        let g_atualizacao = adw::PreferencesGroup::builder().title("Atualizações").build();
        let atualizar_auto = adw::SwitchRow::builder()
            .title("Procurar atualizações ao abrir")
            .subtitle("Consulta a página de versões do projeto no GitHub, no máximo uma vez por dia. Nada é instalado sem você aceitar")
            .build();
        let procurar_atualizacao = adw::ActionRow::builder()
            .title("Procurar atualizações agora")
            .subtitle(concat!("Versão instalada: ", env!("CARGO_PKG_VERSION")))
            .activatable(true)
            .build();
        procurar_atualizacao.add_suffix(&gtk::Image::from_icon_name("software-update-available-symbolic"));
        g_atualizacao.add(&atualizar_auto);
        g_atualizacao.add(&procurar_atualizacao);

        let pagina = adw::PreferencesPage::new();
        pagina.add(&g_perfil);
        pagina.add(&g_desemp);
        pagina.add(&g_cal);
        pagina.add(&g_sobre);
        pagina.add(&g_atualizacao);
        pagina.add(&g_programa);
        (Self { linha_site, linha_sobre, calibrar, atualizar_auto, procurar_atualizacao, taxa, nome_perfil, info }, pagina)
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let ui2 = ui.clone();
    ui.disp.linha_sobre.connect_activated(move |_| ui2.mostrar_sobre());
    let ui2 = ui.clone();
    ui.disp.linha_site.connect_activated(move |_| {
        gtk::UriLauncher::new(crate::ui::SITE_AUTOR).launch(Some(&ui2.janela), gtk::gio::Cancellable::NONE, |_| {});
    });
    let ui2 = ui.clone();
    ui.disp.calibrar.connect_clicked(move |_| ui2.abrir_calibracao());
    let ui2 = ui.clone();
    ui.disp.taxa.connect_selected_notify(move |_| ui2.taxa_mudou());
    let ui2 = ui.clone();
    ui.disp.nome_perfil.connect_apply(move |_| ui2.renomear_perfil());
}

impl Ui {
    pub fn preencher_dispositivo(&self, d: &Dados) {
        let nomes: Vec<String> = d.taxas.iter().map(|c| mag82::nome_taxa(*c)).collect();
        let nomes_ref: Vec<&str> = nomes.iter().map(String::as_str).collect();
        self.disp.taxa.set_model(Some(&gtk::StringList::new(&nomes_ref)));
        if let Some(i) = d.taxas.iter().position(|c| *c == d.taxa_atual) {
            self.disp.taxa.set_selected(i as u32);
        }
        let disp = &d.dispositivo;
        let f = &disp.data_firmware;
        let data = match (f.get(0..4), f.get(4..6), f.get(6..8), f.get(8..)) {
            (Some(ano), Some(mes), Some(dia), Some(hora)) => format!("{dia}/{mes}/{ano} {hora}"),
            _ => f.clone(),
        };
        let valores =
            ["VOLCANOS MAG82".to_string(), disp.firmware.clone(), data, disp.protocolo.clone(), disp.switches.join(", ")];
        for (linha, valor) in self.disp.info.iter().zip(valores) {
            linha.set_subtitle(&valor);
        }
    }

    fn taxa_mudou(self: &Rc<Self>) {
        if self.carregando.get() {
            return;
        }
        let i = self.disp.taxa.selected() as usize;
        let Some(codigo) = self.d.borrow().as_ref().and_then(|d| d.taxas.get(i).copied()) else { return };
        self.pedir(
            move |t| t.definir_taxa(codigo),
            move |ui, ()| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    d.taxa_atual = codigo;
                }
                ui.avisar(&format!("Taxa de resposta: {}", mag82::nome_taxa(codigo)));
            },
        );
    }

    fn renomear_perfil(self: &Rc<Self>) {
        let nome = self.disp.nome_perfil.text().trim().to_string();
        let Some(indice) = self.d.borrow().as_ref().map(|d| d.perfil_atual) else { return };
        self.pedir(
            move |t| {
                t.definir_nome_perfil(indice, &nome)?;
                Ok(mag82::rotulo_perfil(indice, &t.nome_perfil(indice)?))
            },
            move |ui, lido| {
                let perfis = {
                    let mut dados = ui.d.borrow_mut();
                    let Some(d) = dados.as_mut() else { return };
                    if let Some(vaga) = d.perfis.get_mut(indice as usize) {
                        *vaga = lido;
                    }
                    d.perfis.clone()
                };
                ui.silencioso(|| ui.preencher_perfis(&perfis, indice));
                ui.avisar("Perfil renomeado");
            },
        );
    }
}
