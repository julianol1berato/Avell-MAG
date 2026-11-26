//! Modo DKS da página Teclas: até quatro funções na mesma tecla, cada uma acionada em momentos
//! diferentes do curso.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use mag82::{Avancada, Dks, Pos, DKS_MAX, DKS_MIN, DKS_MOMENTOS};

use crate::ui::Ui;
use crate::util::{botao, rotulo};

/// Controles de uma das quatro funções.
struct Funcao {
    qual: adw::ComboRow,
    /// Momento, segura, momento, segura, momento, segura, momento (ver [`Dks::casas`]).
    casas: [gtk::ToggleButton; 7],
}

pub struct PainelDks {
    pub painel: gtk::Box,
    /// (código, descrição) de cada opção das listas; a primeira é a vaga livre.
    opcoes: Vec<(u16, String)>,
    inicio: adw::SpinRow,
    fundo: adw::SpinRow,
    funcoes: Vec<Funcao>,
    aplicar: gtk::Button,
    remover: gtk::Button,
    /// A tecla cujo DKS está no painel: ele só é preenchido de novo quando a seleção muda,
    /// para não apagar o que o usuário está editando.
    mostrando: Cell<Option<Pos>>,
}

impl PainelDks {
    /// `teclas` são as funções que uma ação pode enviar: (código, descrição).
    pub fn construir(teclas: &[(u16, String)]) -> Self {
        let mut opcoes = vec![(0, "Nenhuma".to_string())];
        opcoes.extend(teclas.iter().cloned());
        let nomes: Vec<&str> = opcoes.iter().map(|(_, n)| n.as_str()).collect();

        let g_pontos = adw::PreferencesGroup::builder()
            .title("DKS (acionamento dinâmico)")
            .description(
                "A tecla faz coisas diferentes conforme o quanto é pressionada. São dois pontos do curso \
                 e, portanto, quatro momentos: 1 ao descer até o início, 2 ao chegar ao fundo, \
                 3 ao sair do fundo e 4 ao voltar ao início.",
            )
            .build();
        let ponto = |titulo: &str, subtitulo: &str, valor: f64| {
            let linha = adw::SpinRow::with_range(f64::from(DKS_MIN) / 1000.0, f64::from(DKS_MAX) / 1000.0, 0.1);
            linha.set_title(titulo);
            linha.set_subtitle(subtitulo);
            linha.set_digits(1);
            linha.set_value(valor);
            linha
        };
        let inicio = ponto("Início", "Distância, em mm, dos momentos 1 e 4", 1.5);
        let fundo = ponto("Fundo", "Distância, em mm, dos momentos 2 e 3", 3.0);
        g_pontos.add(&inicio);
        g_pontos.add(&fundo);

        let painel = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).build();
        painel.append(&g_pontos);

        let funcoes: Vec<Funcao> = (0..4)
            .map(|i| {
                let grupo = adw::PreferencesGroup::builder().title(format!("Função {}", i + 1)).build();
                let qual = adw::ComboRow::builder()
                    .title("Envia")
                    .model(&gtk::StringList::new(&nomes))
                    .expression(gtk::PropertyExpression::new(
                        gtk::StringObject::static_type(),
                        gtk::Expression::NONE,
                        "string",
                    ))
                    .enable_search(true)
                    .build();
                let casas: [gtk::ToggleButton; 7] = std::array::from_fn(|c| {
                    if c % 2 == 0 {
                        gtk::ToggleButton::builder()
                            .label(format!("{}", c / 2 + 1))
                            .css_classes(["circular"])
                            .tooltip_text(format!("Aciona {}", DKS_MOMENTOS[c / 2]))
                            .valign(gtk::Align::Center)
                            .build()
                    } else {
                        gtk::ToggleButton::builder()
                            .icon_name("pan-end-symbolic")
                            .css_classes(["flat"])
                            .tooltip_text(format!(
                                "Continua pressionada do momento {} até o {}",
                                c / 2 + 1,
                                c / 2 + 2
                            ))
                            .valign(gtk::Align::Center)
                            .build()
                    }
                });
                let caixa = gtk::Box::builder().spacing(4).valign(gtk::Align::Center).build();
                for casa in &casas {
                    caixa.append(casa);
                }
                let linha = adw::ActionRow::builder()
                    .title("Momentos")
                    .subtitle("Números: aciona. Setas: segura até o seguinte")
                    .build();
                linha.add_suffix(&caixa);
                grupo.add(&qual);
                grupo.add(&linha);
                painel.append(&grupo);
                Funcao { qual, casas }
            })
            .collect();

        let remover = botao("Remover DKS", false);
        let aplicar = botao("Aplicar DKS", true);
        let acoes = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        acoes.append(&remover);
        acoes.append(&aplicar);
        painel.append(&acoes);

        Self { painel, opcoes, inicio, fundo, funcoes, aplicar, remover, mostrando: Cell::new(None) }
    }

    pub fn mostrar(&self, dks: &Dks) {
        self.inicio.set_value(f64::from(dks.inicio) / 1000.0);
        self.fundo.set_value(f64::from(dks.fundo) / 1000.0);
        for (i, f) in self.funcoes.iter().enumerate() {
            let escolhida = self.opcoes.iter().position(|(c, _)| *c == dks.teclas[i]).unwrap_or(0);
            f.qual.set_selected(escolhida as u32);
            for (casa, ligada) in f.casas.iter().zip(dks.casas(i)) {
                casa.set_active(ligada);
            }
        }
    }

    /// O DKS que está montado no painel. Vaga sem função fica sem momentos.
    fn da_tela(&self) -> Dks {
        let mm = |linha: &adw::SpinRow| (linha.value() * 1000.0).round() as u16;
        let mut dks = Dks { inicio: mm(&self.inicio), fundo: mm(&self.fundo), ..Default::default() };
        for (i, f) in self.funcoes.iter().enumerate() {
            dks.teclas[i] = self.opcoes.get(f.qual.selected() as usize).map_or(0, |(c, _)| *c);
            if dks.teclas[i] != 0 {
                dks.definir_casas(i, std::array::from_fn(|c| f.casas[c].is_active()));
            }
        }
        dks
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let ui2 = ui.clone();
    ui.dks.aplicar.connect_clicked(move |_| ui2.aplicar_dks());
    let ui2 = ui.clone();
    ui.dks.remover.connect_clicked(move |_| ui2.remover_dks());
}

impl Ui {
    /// Ajusta o painel à tecla selecionada: mostra o DKS que ela tem, ou um em branco.
    pub fn atualizar_dks(&self, selecionadas: &[Pos]) {
        let p = &self.dks;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let escolhida = match selecionadas {
            [pos] => Some(*pos),
            _ => None,
        };
        let atual = escolhida.and_then(|pos| match d.avancada[&pos] {
            Avancada::Dks(dks) => Some(dks),
            _ => None,
        });
        p.painel.set_sensitive(escolhida.is_some());
        p.remover.set_sensitive(atual.is_some());
        if p.mostrando.replace(escolhida) != escolhida {
            p.mostrar(&atual.unwrap_or_default());
        }
    }

    pub fn aplicar_dks(self: &Rc<Self>) {
        let [pos] = self.selecionadas()[..] else { return };
        let dks = self.dks.da_tela();
        // os erros de preenchimento aparecem antes de qualquer gravação
        if let Err(e) = dks.validar() {
            self.avisar_erro(&e.to_string());
            return;
        }
        let par = self.d.borrow().as_ref().and_then(|d| match d.avancada[&pos] {
            Avancada::Socd { par, .. } => Some(par),
            _ => None,
        });
        self.pedir(
            move |t| {
                t.definir_dks(pos, &dks)?;
                let mut lidas = vec![(pos, t.avancada(pos)?)];
                if let Some(par) = par {
                    lidas.push((par, t.avancada(par)?));
                }
                Ok(lidas)
            },
            move |ui, lidas| {
                let mensagem = {
                    let mut dados = ui.d.borrow_mut();
                    let Some(d) = dados.as_mut() else { return };
                    d.avancada.extend(lidas);
                    format!("DKS aplicado em {}", rotulo(&d.nomes[&pos]))
                };
                ui.dks.mostrando.set(None);
                ui.avisar(&mensagem);
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }

    pub fn remover_dks(self: &Rc<Self>) {
        let [pos] = self.selecionadas()[..] else { return };
        self.pedir(
            move |t| {
                t.remover_avancada(pos)?;
                t.avancada(pos)
            },
            move |ui, lida| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    d.avancada.insert(pos, lida);
                }
                ui.dks.mostrando.set(None);
                ui.avisar("DKS removido");
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }
}
