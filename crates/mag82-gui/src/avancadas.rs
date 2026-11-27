//! Modo Avançada da página Teclas: as funções avançadas de uma tecla só. O tipo é escolhido numa
//! lista e cada tipo tem o seu painel: DKS, multiponto, dupla função, contínuo e ao soltar.
//! Os pares (SOCD e RS) ficam no modo Pares.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use mag82::{Avancada, Dks, End, Mpt, Mt, Pos, Tgl, ATRASO_MAX, DKS_MAX, DKS_MIN, DKS_MOMENTOS, TEMPO_MAX, TEMPO_MIN};

use crate::ui::Ui;
use crate::util::{botao, rotulo};

/// Os tipos, na ordem do seletor: (nome do painel, nome no seletor, explicação).
const TIPOS: [(&str, &str, &str); 5] = [
    (
        "dks",
        "DKS",
        "Acionamento dinâmico: a tecla faz coisas diferentes conforme o quanto é pressionada. São dois pontos do \
         curso e, portanto, quatro momentos: 1 ao descer até o início, 2 ao chegar ao fundo, 3 ao sair do fundo \
         e 4 ao voltar ao início.",
    ),
    (
        "mt",
        "Dupla função",
        "Um toque curto envia uma função; segurar a tecla envia outra. Exemplo: Caps Lock que é Esc no toque e Ctrl ao segurar.",
    ),
    (
        "tgl",
        "Contínuo",
        "Um toque deixa a função pressionada, como se você continuasse segurando; outro toque solta. \
         Segurando a tecla, ela funciona como uma tecla comum. Serve para correr ou andar sem segurar a tecla.",
    ),
    (
        "mpt",
        "Multiponto",
        "Até três funções na mesma tecla, cada uma a partir de uma profundidade. Elas se somam: \
         apertando até o fundo, saem as três.",
    ),
    (
        "end",
        "Ao soltar",
        "A tecla envia uma função ao ser pressionada, que repete enquanto ela está segurada, e outra ao ser solta.",
    ),
];

/// O que pode ir numa função avançada, mostrado embaixo da explicação de cada tipo. O teclado
/// recusa o resto (ver [`mag82::serve_em_avancada`]), por isso as listas só trazem teclas comuns.
const SO_TECLAS_COMUNS: &str = "As funções daqui são sempre teclas comuns: letras, números, F1 a F12, setas, Ctrl, \
    Shift, Alt… O teclado não aceita multimídia, luzes, perfis nem macros numa função avançada; essas vão direto \
    na tecla, no modo Funções.";

/// Lista de funções para o fim de uma linha, com busca; a primeira opção é a vaga livre.
fn funcao_na_linha(nomes: &gtk::StringList) -> gtk::DropDown {
    let lista = gtk::DropDown::builder()
        .model(nomes)
        .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
        .enable_search(true)
        .valign(gtk::Align::Center)
        .build();
    lista.set_size_request(190, -1);
    lista
}

/// Lista de funções com busca; a primeira opção é a vaga livre.
fn lista_de_funcoes(titulo: &str, nomes: &[&str]) -> adw::ComboRow {
    adw::ComboRow::builder()
        .title(titulo)
        .model(&gtk::StringList::new(nomes))
        .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
        .enable_search(true)
        .build()
}

fn numero(titulo: &str, subtitulo: &str, minimo: f64, maximo: f64, passo: f64, casas: u32, valor: f64) -> adw::SpinRow {
    let linha = adw::SpinRow::with_range(minimo, maximo, passo);
    linha.set_title(titulo);
    linha.set_subtitle(subtitulo);
    linha.set_digits(casas);
    linha.set_value(valor);
    linha
}

fn profundidade(titulo: &str, subtitulo: &str, valor: f64) -> adw::SpinRow {
    numero(titulo, subtitulo, f64::from(DKS_MIN) / 1000.0, f64::from(DKS_MAX) / 1000.0, 0.1, 1, valor)
}

fn tempo() -> adw::SpinRow {
    numero(
        "Tempo",
        "Em milissegundos: mais rápido que isso é toque, mais demorado é segurar",
        f64::from(TEMPO_MIN),
        f64::from(TEMPO_MAX),
        10.0,
        0,
        200.0,
    )
}

fn mm(linha: &adw::SpinRow) -> u16 {
    (linha.value() * 1000.0).round() as u16
}

/// Controles de uma das quatro funções do DKS.
struct FuncaoDks {
    qual: gtk::DropDown,
    /// Momento, segura, momento, segura, momento, segura, momento (ver [`Dks::casas`]).
    casas: [gtk::ToggleButton; 7],
}

pub struct PainelAvancadas {
    pub painel: gtk::Box,
    /// (código, descrição) de cada opção das listas; a primeira é a vaga livre.
    pub opcoes: Vec<(u16, String)>,
    /// Seletor do tipo, mostrado pela página Teclas logo abaixo da linha da seleção.
    pub tipos: gtk::Box,
    tipo_btns: Vec<gtk::ToggleButton>,
    explicacao: gtk::Label,
    paineis: gtk::Stack,
    aplicar: gtk::Button,
    remover: gtk::Button,
    /// A tecla cuja função está no painel: ele só é preenchido de novo quando a seleção muda,
    /// para não apagar o que o usuário está editando.
    mostrando: Cell<Option<Pos>>,

    dks_inicio: adw::SpinRow,
    dks_fundo: adw::SpinRow,
    dks_funcoes: Vec<FuncaoDks>,
    mt_toque: adw::ComboRow,
    mt_segura: adw::ComboRow,
    mt_tempo: adw::SpinRow,
    tgl_tecla: adw::ComboRow,
    tgl_tempo: adw::SpinRow,
    mpt: Vec<(gtk::DropDown, gtk::SpinButton)>,
    end_pressionar: adw::ComboRow,
    end_soltar: adw::ComboRow,
    end_atraso: adw::SpinRow,
}

impl PainelAvancadas {
    /// `teclas` são as funções que uma ação pode enviar: (código, descrição). Só as que o teclado
    /// aceita numa função avançada (ver [`mag82::serve_em_avancada`]).
    pub fn construir(teclas: &[(u16, String)]) -> Self {
        let mut opcoes = vec![(0, "Nenhuma".to_string())];
        opcoes.extend(teclas.iter().cloned());
        let nomes: Vec<&str> = opcoes.iter().map(|(_, n)| n.as_str()).collect();
        let grupo = |titulo: &str| adw::PreferencesGroup::builder().title(titulo).build();
        let coluna = || gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).build();

        let lista_nomes = gtk::StringList::new(&nomes);
        // ----- tipo: uma faixa de partes iguais, como o seletor de modos -----
        let tipos = gtk::Box::builder().css_classes(["linked"]).homogeneous(true).visible(false).build();
        let mut tipo_btns: Vec<gtk::ToggleButton> = Vec::new();
        for (_, nome, _) in TIPOS {
            let b = gtk::ToggleButton::builder().label(nome).hexpand(true).build();
            match tipo_btns.first() {
                Some(primeiro) => b.set_group(Some(primeiro)),
                None => b.set_active(true),
            }
            tipos.append(&b);
            tipo_btns.push(b);
        }
        let explicacao = gtk::Label::builder().wrap(true).xalign(0.0).css_classes(["dim-label"]).build();

        // ----- DKS -----
        let p_dks = coluna();
        let g_pontos = grupo("Pontos do curso");
        let dks_inicio = profundidade("Início", "Distância, em mm, dos momentos 1 e 4", 1.5);
        let dks_fundo = profundidade("Fundo", "Distância, em mm, dos momentos 2 e 3", 3.0);
        g_pontos.add(&dks_inicio);
        g_pontos.add(&dks_fundo);
        p_dks.append(&g_pontos);
        let g_funcoes = adw::PreferencesGroup::builder()
            .title("Funções")
            .description("Em cada linha: o que a tecla envia e em quais momentos. Número aciona; seta segura até o seguinte.")
            .build();
        let dks_funcoes: Vec<FuncaoDks> = (0..4)
            .map(|i| {
                let qual = funcao_na_linha(&lista_nomes);
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
                            .tooltip_text(format!("Continua pressionada do momento {} até o {}", c / 2 + 1, c / 2 + 2))
                            .valign(gtk::Align::Center)
                            .build()
                    }
                });
                let caixa = gtk::Box::builder().spacing(2).valign(gtk::Align::Center).margin_start(12).build();
                for casa in &casas {
                    caixa.append(casa);
                }
                let linha = adw::ActionRow::builder().title(format!("Função {}", i + 1)).build();
                linha.add_suffix(&qual);
                linha.add_suffix(&caixa);
                g_funcoes.add(&linha);
                FuncaoDks { qual, casas }
            })
            .collect();
        p_dks.append(&g_funcoes);

        // ----- dupla função -----
        let g_mt = grupo("Dupla função");
        let mt_toque = lista_de_funcoes("No toque curto, envia", &nomes);
        let mt_segura = lista_de_funcoes("Ao segurar, envia", &nomes);
        let mt_tempo = tempo();
        g_mt.add(&mt_toque);
        g_mt.add(&mt_segura);
        g_mt.add(&mt_tempo);

        // ----- contínuo -----
        let g_tgl = grupo("Contínuo");
        let tgl_tecla = lista_de_funcoes("Fica pressionada", &nomes);
        let tgl_tempo = tempo();
        g_tgl.add(&tgl_tecla);
        g_tgl.add(&tgl_tempo);

        // ----- multiponto: uma linha por profundidade -----
        let g_mpt = adw::PreferencesGroup::builder()
            .title("Profundidades")
            .description("O que a tecla envia a partir de cada profundidade, em milímetros, da mais rasa à mais funda.")
            .build();
        let mpt: Vec<(gtk::DropDown, gtk::SpinButton)> = [("Toque leve", 0.5), ("Toque médio", 1.5), ("Toque fundo", 2.5)]
            .into_iter()
            .map(|(titulo, valor)| {
                let qual = funcao_na_linha(&lista_nomes);
                let ponto = gtk::SpinButton::with_range(f64::from(DKS_MIN) / 1000.0, f64::from(DKS_MAX) / 1000.0, 0.1);
                ponto.set_digits(1);
                ponto.set_value(valor);
                ponto.set_valign(gtk::Align::Center);
                ponto.set_margin_start(12);
                ponto.set_tooltip_text(Some("A partir de quantos milímetros"));
                let linha = adw::ActionRow::builder().title(titulo).build();
                linha.add_suffix(&qual);
                linha.add_suffix(&ponto);
                linha.add_suffix(&gtk::Label::builder().label("mm").css_classes(["dim-label"]).build());
                g_mpt.add(&linha);
                (qual, ponto)
            })
            .collect();

        // ----- ao soltar -----
        let g_end = grupo("Ao soltar");
        let end_pressionar = lista_de_funcoes("Ao pressionar, envia", &nomes);
        let end_soltar = lista_de_funcoes("Ao soltar, envia", &nomes);
        let end_atraso = numero("Atraso", "Em milissegundos", 0.0, f64::from(ATRASO_MAX), 1.0, 0, 0.0);
        g_end.add(&end_pressionar);
        g_end.add(&end_soltar);
        g_end.add(&end_atraso);

        let paineis = gtk::Stack::builder().vhomogeneous(false).build();
        paineis.add_named(&p_dks, Some("dks"));
        paineis.add_named(&g_mt, Some("mt"));
        paineis.add_named(&g_tgl, Some("tgl"));
        paineis.add_named(&g_mpt, Some("mpt"));
        paineis.add_named(&g_end, Some("end"));

        let remover = botao("Remover", false);
        let aplicar = botao("Aplicar", true);
        let acoes = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        acoes.append(&remover);
        acoes.append(&aplicar);

        let painel = coluna();
        painel.append(&explicacao);
        painel.append(&paineis);
        painel.append(&acoes);

        let p = Self {
            painel,
            opcoes,
            tipos,
            tipo_btns,
            explicacao,
            paineis,
            aplicar,
            remover,
            mostrando: Cell::new(None),
            dks_inicio,
            dks_fundo,
            dks_funcoes,
            mt_toque,
            mt_segura,
            mt_tempo,
            tgl_tecla,
            tgl_tempo,
            mpt,
            end_pressionar,
            end_soltar,
            end_atraso,
        };
        p.trocar_tipo();
        p
    }

    /// Mostra o painel e a explicação do tipo escolhido na lista.
    fn trocar_tipo(&self) {
        let (nome, _, explicacao) = TIPOS[self.tipo()];
        self.paineis.set_visible_child_name(nome);
        self.explicacao.set_label(&format!("{explicacao}\n{SO_TECLAS_COMUNS}"));
    }

    /// Posição, em [`TIPOS`], do tipo escolhido no seletor.
    fn tipo(&self) -> usize {
        self.tipo_btns.iter().position(|b| b.is_active()).unwrap_or(0)
    }

    fn escolher_tipo(&self, tipo: usize) {
        if let Some(b) = self.tipo_btns.get(tipo) {
            b.set_active(true);
        }
        self.trocar_tipo();
    }

    fn escolher_na_linha(&self, lista: &gtk::DropDown, codigo: u16) {
        lista.set_selected(self.opcoes.iter().position(|(c, _)| *c == codigo).unwrap_or(0) as u32);
    }

    fn codigo_da_linha(&self, lista: &gtk::DropDown) -> u16 {
        self.opcoes.get(lista.selected() as usize).map_or(0, |(c, _)| *c)
    }

    fn escolher(&self, lista: &adw::ComboRow, codigo: u16) {
        lista.set_selected(self.opcoes.iter().position(|(c, _)| *c == codigo).unwrap_or(0) as u32);
    }

    fn codigo(&self, lista: &adw::ComboRow) -> u16 {
        self.opcoes.get(lista.selected() as usize).map_or(0, |(c, _)| *c)
    }

    /// Põe uma função avançada na tela: escolhe o tipo dela e preenche o painel.
    pub fn mostrar(&self, avancada: &Avancada) {
        let tipo = match avancada {
            Avancada::Dks(dks) => {
                self.dks_inicio.set_value(f64::from(dks.inicio) / 1000.0);
                self.dks_fundo.set_value(f64::from(dks.fundo) / 1000.0);
                for (i, f) in self.dks_funcoes.iter().enumerate() {
                    self.escolher_na_linha(&f.qual, dks.teclas[i]);
                    for (casa, ligada) in f.casas.iter().zip(dks.casas(i)) {
                        casa.set_active(ligada);
                    }
                }
                0
            }
            Avancada::Mt(mt) => {
                self.escolher(&self.mt_toque, mt.toque);
                self.escolher(&self.mt_segura, mt.segura);
                self.mt_tempo.set_value(f64::from(mt.tempo));
                1
            }
            Avancada::Tgl(tgl) => {
                self.escolher(&self.tgl_tecla, tgl.tecla);
                self.tgl_tempo.set_value(f64::from(tgl.tempo));
                2
            }
            Avancada::Mpt(mpt) => {
                for (i, (qual, ponto)) in self.mpt.iter().enumerate() {
                    self.escolher_na_linha(qual, mpt.teclas[i]);
                    ponto.set_value(f64::from(mpt.pontos[i]) / 1000.0);
                }
                3
            }
            Avancada::End(end) => {
                self.escolher(&self.end_pressionar, end.ao_pressionar);
                self.escolher(&self.end_soltar, end.ao_soltar);
                self.end_atraso.set_value(f64::from(end.atraso));
                4
            }
            _ => return,
        };
        self.escolher_tipo(tipo);
    }

    /// Deixa todos os painéis em branco, sem mudar o tipo escolhido.
    fn limpar(&self) {
        let tipo = self.tipo();
        self.mostrar(&Avancada::Dks(Dks::default()));
        self.mostrar(&Avancada::Mt(Mt { toque: 0, segura: 0, tempo: 200 }));
        self.mostrar(&Avancada::Tgl(Tgl { tecla: 0, tempo: 200 }));
        self.mostrar(&Avancada::Mpt(Mpt { teclas: [0; 3], pontos: [500, 1500, 2500] }));
        self.mostrar(&Avancada::End(End { ao_pressionar: 0, ao_soltar: 0, atraso: 0 }));
        self.escolher_tipo(tipo);
    }

    /// A função avançada que está montada no painel do tipo escolhido.
    fn da_tela(&self) -> Avancada {
        match self.tipo() {
            0 => {
                let mut dks = Dks { inicio: mm(&self.dks_inicio), fundo: mm(&self.dks_fundo), ..Default::default() };
                for (i, f) in self.dks_funcoes.iter().enumerate() {
                    dks.teclas[i] = self.codigo_da_linha(&f.qual);
                    // vaga sem função fica sem momentos
                    if dks.teclas[i] != 0 {
                        dks.definir_casas(i, std::array::from_fn(|c| f.casas[c].is_active()));
                    }
                }
                Avancada::Dks(dks)
            }
            1 => Avancada::Mt(Mt {
                toque: self.codigo(&self.mt_toque),
                segura: self.codigo(&self.mt_segura),
                tempo: self.mt_tempo.value().round() as u16,
            }),
            2 => Avancada::Tgl(Tgl { tecla: self.codigo(&self.tgl_tecla), tempo: self.tgl_tempo.value().round() as u16 }),
            3 => Avancada::Mpt(Mpt {
                teclas: std::array::from_fn(|i| self.codigo_da_linha(&self.mpt[i].0)),
                pontos: std::array::from_fn(|i| (self.mpt[i].1.value() * 1000.0).round() as u16),
            }),
            _ => Avancada::End(End {
                ao_pressionar: self.codigo(&self.end_pressionar),
                ao_soltar: self.codigo(&self.end_soltar),
                atraso: self.end_atraso.value().round() as u16,
            }),
        }
    }
}

/// Verdadeiro para as funções avançadas de uma tecla só (as deste modo).
pub fn de_uma_tecla(avancada: &Avancada) -> bool {
    matches!(avancada, Avancada::Dks(_) | Avancada::Mpt(_) | Avancada::Mt(_) | Avancada::Tgl(_) | Avancada::End(_))
}

/// Sigla do tipo, para a legenda da tecla.
pub fn sigla(avancada: &Avancada) -> Option<&'static str> {
    Some(match avancada {
        Avancada::Dks(_) => "DKS",
        Avancada::Mpt(_) => "MPT",
        Avancada::Mt(_) => "MT",
        Avancada::Tgl(_) => "TGL",
        Avancada::End(_) => "END",
        _ => return None,
    })
}

pub fn ligar(ui: &Rc<Ui>) {
    for b in &ui.av.tipo_btns {
        let ui2 = ui.clone();
        b.connect_toggled(move |b| {
            if b.is_active() {
                ui2.av.trocar_tipo();
            }
        });
    }
    let ui2 = ui.clone();
    ui.av.aplicar.connect_clicked(move |_| ui2.aplicar_avancada());
    let ui2 = ui.clone();
    ui.av.remover.connect_clicked(move |_| ui2.remover_avancada());
}

impl Ui {
    /// Ajusta o painel à tecla selecionada: mostra a função avançada que ela tem, ou em branco.
    pub fn atualizar_avancada(&self, selecionadas: &[Pos]) {
        let p = &self.av;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let escolhida = match selecionadas {
            [pos] => Some(*pos),
            _ => None,
        };
        let atual = escolhida.map(|pos| d.avancada[&pos]).filter(de_uma_tecla);
        p.painel.set_sensitive(escolhida.is_some());
        p.remover.set_sensitive(atual.is_some());
        if p.mostrando.replace(escolhida) != escolhida {
            p.limpar();
            if let Some(atual) = &atual {
                p.mostrar(atual);
            }
        }
    }

    pub fn aplicar_avancada(self: &Rc<Self>) {
        let [pos] = self.selecionadas()[..] else { return };
        let nova = self.av.da_tela();
        // os erros de preenchimento aparecem antes de qualquer gravação
        let conferida = match &nova {
            Avancada::Dks(dks) => dks.validar(),
            Avancada::Mpt(mpt) => mpt.validar(),
            Avancada::Mt(mt) => mt.validar(),
            Avancada::Tgl(tgl) => tgl.validar(),
            Avancada::End(end) => end.validar(),
            _ => Ok(()),
        };
        if let Err(e) = conferida {
            self.avisar_erro(&e.to_string());
            return;
        }
        // se a tecla fazia parte de um par, o par é desfeito: a outra tecla também muda
        let par = self.d.borrow().as_ref().and_then(|d| match d.avancada[&pos] {
            Avancada::Socd { par, .. } | Avancada::Rs { par, .. } => Some(par),
            _ => None,
        });
        self.pedir(
            move |t| {
                match &nova {
                    Avancada::Dks(dks) => t.definir_dks(pos, dks)?,
                    Avancada::Mpt(mpt) => t.definir_mpt(pos, mpt)?,
                    Avancada::Mt(mt) => t.definir_mt(pos, mt)?,
                    Avancada::Tgl(tgl) => t.definir_tgl(pos, tgl)?,
                    Avancada::End(end) => t.definir_end(pos, end)?,
                    _ => {}
                }
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
                    let sigla = sigla(&lidas[0].1).unwrap_or("função avançada");
                    d.avancada.extend(lidas);
                    format!("{sigla} aplicado em {}", rotulo(&d.nomes[&pos]))
                };
                ui.av.mostrando.set(None);
                ui.avisar(&mensagem);
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }

    pub fn remover_avancada(self: &Rc<Self>) {
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
                ui.av.mostrando.set(None);
                ui.avisar("Função avançada removida");
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }
}
