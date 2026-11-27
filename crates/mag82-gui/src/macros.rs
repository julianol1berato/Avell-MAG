//! Página Macros: sequências de teclas que o teclado executa sozinho. Cada macro é uma lista de
//! passos (tocar, pressionar ou soltar uma função, com uma espera antes) e um modo de disparo.
//! Para usar uma macro, liga-se uma tecla a ela em Teclas → Remapear ou Atalhos Fn.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use mag82::{AcaoMacro, Macro, ModoMacro, MACRO_ESPERA_MAX, MACRO_REPETICOES_MAX};

use crate::dados::Dados;
use crate::ui::Ui;
use crate::util::{botao, rotulo_longo};

/// O que um passo faz com a função dele.
const TIPOS: [&str; 3] = ["Tocar", "Pressionar", "Soltar"];

/// Um passo como aparece na tela, com a espera que vem depois dele. "Tocar" são duas ações do
/// teclado: pressionar e soltar.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Passo {
    tipo: u32,
    codigo: u16,
    espera: u16,
}

fn passos_de(acoes: &[AcaoMacro]) -> Vec<Passo> {
    let mut passos = Vec::new();
    let mut i = 0;
    while i < acoes.len() {
        let a = acoes[i];
        match acoes.get(i + 1) {
            // pressionar e soltar em seguida é um toque; a espera dele é a de depois de soltar
            Some(b) if a.pressiona && !b.pressiona && b.codigo == a.codigo && a.espera == 0 => {
                passos.push(Passo { tipo: 0, codigo: a.codigo, espera: b.espera });
                i += 2;
            }
            _ => {
                passos.push(Passo { tipo: if a.pressiona { 1 } else { 2 }, codigo: a.codigo, espera: a.espera });
                i += 1;
            }
        }
    }
    passos
}

fn acoes_de(passos: &[Passo]) -> Vec<AcaoMacro> {
    let mut acoes = Vec::new();
    for p in passos {
        match p.tipo {
            0 => {
                acoes.push(AcaoMacro { pressiona: true, espera: 0, codigo: p.codigo });
                acoes.push(AcaoMacro { pressiona: false, espera: p.espera, codigo: p.codigo });
            }
            tipo => acoes.push(AcaoMacro { pressiona: tipo == 1, espera: p.espera, codigo: p.codigo }),
        }
    }
    acoes
}

/// Os controles de um passo na lista.
struct LinhaPasso {
    linha: adw::ActionRow,
    tipo: gtk::DropDown,
    funcao: gtk::DropDown,
    espera: gtk::SpinButton,
}

pub struct PainelMacros {
    /// (código, descrição) das funções que um passo pode usar.
    opcoes: Vec<(u16, String)>,
    nomes: gtk::StringList,
    qual: adw::ComboRow,
    /// Quais teclas disparam a macro escolhida.
    usada_por: adw::ActionRow,
    /// Linha mostrada no lugar dos passos enquanto não há nenhum.
    vazia: adw::ActionRow,
    g_passos: adw::PreferencesGroup,
    linhas: RefCell<Vec<LinhaPasso>>,
    adicionar: gtk::Button,
    modo: adw::ComboRow,
    repeticoes: adw::SpinRow,
    apagar: gtk::Button,
    gravar: gtk::Button,
    /// A macro que está na tela: a lista de passos só é refeita quando ela muda, para não
    /// apagar o que o usuário está montando.
    mostrando: Cell<Option<u32>>,
}

impl PainelMacros {
    pub fn construir() -> (Self, adw::PreferencesPage) {
        // teclas em ordem alfabética, depois as funções especiais; macro não entra em macro
        let mut teclas = mag82::funcoes();
        teclas.retain(|(c, _)| *c != 0 && *c != mag82::TECLA_FN);
        teclas.sort_by_key(|(_, nome)| rotulo_longo(nome).to_lowercase());
        let mut opcoes: Vec<(u16, String)> = teclas.into_iter().map(|(c, n)| (c, rotulo_longo(&n))).collect();
        opcoes.extend(
            mag82::ESPECIAIS
                .iter()
                .filter(|e| !matches!(e.familia, mag82::Familia::Reservada | mag82::Familia::Macro))
                .map(|e| (e.codigo, e.nome.to_string())),
        );
        let nomes_ref: Vec<&str> = opcoes.iter().map(|(_, n)| n.as_str()).collect();
        let nomes = gtk::StringList::new(&nomes_ref);

        let g_qual = adw::PreferencesGroup::builder()
            .title("Macros")
            .description(
                "Uma macro é uma sequência de teclas que o teclado executa sozinho. Monte os passos aqui e \
                 depois ligue uma tecla à macro em Teclas → Funções.",
            )
            .build();
        let qual = adw::ComboRow::builder().title("Macro").build();
        let usada_por = adw::ActionRow::builder().title("Disparada por").css_classes(["property"]).build();
        g_qual.add(&qual);
        g_qual.add(&usada_por);

        let adicionar = botao("Adicionar passo", false);
        let g_passos = adw::PreferencesGroup::builder()
            .title("Passos")
            .description(
                "Na ordem em que acontecem. Depois de cada passo o teclado pode esperar um tempo; \
                 em um passo de pressionar, é por esse tempo que a tecla fica segurada.",
            )
            .header_suffix(&adicionar)
            .build();
        // sem passos, a lista diz o que fazer em vez de ficar em branco
        let vazia = adw::ActionRow::builder()
            .title("Nenhum passo ainda")
            .subtitle("Use Adicionar passo para começar a montar a macro")
            .build();
        g_passos.add(&vazia);

        let g_modo = adw::PreferencesGroup::builder().title("Disparo").build();
        let descricoes: Vec<&str> = ModoMacro::TODOS.iter().map(|m| m.descricao()).collect();
        let modo = adw::ComboRow::builder()
            .title("Ao usar a tecla")
            .model(&gtk::StringList::new(&descricoes))
            .use_subtitle(true)
            .build();
        let repeticoes = adw::SpinRow::with_range(1.0, f64::from(MACRO_REPETICOES_MAX), 1.0);
        repeticoes.set_title("Repetições");
        repeticoes.set_subtitle("Quantas vezes a macro roda a cada disparo; nos modos de segurar, ela para ao completar");
        g_modo.add(&modo);
        g_modo.add(&repeticoes);

        let apagar = botao("Apagar macro", false);
        let gravar = botao("Gravar macro", true);
        let acoes = gtk::Box::builder().spacing(8).halign(gtk::Align::End).build();
        acoes.append(&apagar);
        acoes.append(&gravar);
        let g_acoes = adw::PreferencesGroup::new();
        g_acoes.add(&acoes);

        let pagina = adw::PreferencesPage::new();
        pagina.add(&g_qual);
        pagina.add(&g_passos);
        pagina.add(&g_modo);
        pagina.add(&g_acoes);
        let painel = Self {
            opcoes,
            nomes,
            qual,
            usada_por,
            vazia,
            g_passos,
            linhas: RefCell::new(Vec::new()),
            adicionar,
            modo,
            repeticoes,
            apagar,
            gravar,
            mostrando: Cell::new(None),
        };
        (painel, pagina)
    }

    fn acrescentar(self_: &Rc<Ui>, passo: Passo) {
        let p = &self_.macros;
        let tipo = gtk::DropDown::from_strings(&TIPOS);
        tipo.set_valign(gtk::Align::Center);
        // larguras fixas: as colunas ficam alinhadas de uma linha para a outra
        tipo.set_size_request(132, -1);
        tipo.set_selected(passo.tipo);
        let funcao = gtk::DropDown::builder()
            .model(&p.nomes)
            .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
            .enable_search(true)
            .valign(gtk::Align::Center)
            .build();
        funcao.set_size_request(190, -1);
        funcao.set_selected(p.opcoes.iter().position(|(c, _)| *c == passo.codigo).unwrap_or(0) as u32);
        let espera = gtk::SpinButton::with_range(0.0, f64::from(MACRO_ESPERA_MAX), 10.0);
        espera.set_valign(gtk::Align::Center);
        espera.set_width_chars(5);
        espera.set_tooltip_text(Some("Espera depois do passo, em milissegundos"));
        espera.set_value(f64::from(passo.espera));
        let remover = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .tooltip_text("Tirar este passo")
            .build();
        // lê-se como uma frase: "Tocar C e esperar 50 ms"
        let frase = gtk::Box::builder().spacing(8).valign(gtk::Align::Center).build();
        frase.append(&tipo);
        frase.append(&funcao);
        frase.append(&gtk::Label::builder().label("e esperar").css_classes(["dim-label"]).build());
        frase.append(&espera);
        frase.append(&gtk::Label::builder().label("ms").css_classes(["dim-label"]).build());
        frase.append(&remover);
        let linha = adw::ActionRow::new();
        linha.add_suffix(&frase);
        p.g_passos.add(&linha);
        let (ui, alvo) = (self_.clone(), linha.clone());
        remover.connect_clicked(move |_| {
            let p = &ui.macros;
            let mut linhas = p.linhas.borrow_mut();
            if let Some(i) = linhas.iter().position(|l| l.linha == alvo) {
                p.g_passos.remove(&linhas.remove(i).linha);
            }
            drop(linhas);
            p.numerar();
        });
        p.linhas.borrow_mut().push(LinhaPasso { linha, tipo, funcao, espera });
        p.numerar();
    }

    fn numerar(&self) {
        let linhas = self.linhas.borrow();
        for (i, l) in linhas.iter().enumerate() {
            l.linha.set_title(&format!("{}", i + 1));
        }
        self.vazia.set_visible(linhas.is_empty());
    }

    fn esvaziar(&self) {
        for l in self.linhas.borrow_mut().drain(..) {
            self.g_passos.remove(&l.linha);
        }
        self.vazia.set_visible(true);
    }

    fn passos(&self) -> Vec<Passo> {
        self.linhas
            .borrow()
            .iter()
            .map(|l| Passo {
                tipo: l.tipo.selected(),
                codigo: self.opcoes.get(l.funcao.selected() as usize).map_or(0, |(c, _)| *c),
                espera: l.espera.value().round() as u16,
            })
            .collect()
    }

    /// A macro montada na tela.
    fn da_tela(&self) -> Macro {
        let acoes = acoes_de(&self.passos());
        Macro {
            valida: !acoes.is_empty(),
            modo: ModoMacro::do_byte(self.modo.selected() as u8),
            repeticoes: self.repeticoes.value().round() as u16,
            acoes,
        }
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let p = &ui.macros;
    let ui2 = ui.clone();
    p.qual.connect_selected_notify(move |_| ui2.mostrar_macro());
    // ao chegar à página, confere de novo quais teclas disparam a macro (podem ter mudado em Teclas)
    let ui2 = ui.clone();
    ui.pilha.connect_visible_child_name_notify(move |pilha| {
        if pilha.visible_child_name().as_deref() == Some("macros") {
            ui2.mostrar_macro();
        }
    });
    let ui2 = ui.clone();
    p.adicionar.connect_clicked(move |_| {
        let primeira = ui2.macros.opcoes.first().map_or(0, |(c, _)| *c);
        PainelMacros::acrescentar(&ui2, Passo { tipo: 0, codigo: primeira, espera: 0 });
    });
    let ui2 = ui.clone();
    p.gravar.connect_clicked(move |_| ui2.gravar_macro(false));
    let ui2 = ui.clone();
    p.apagar.connect_clicked(move |_| ui2.gravar_macro(true));
}

impl Ui {
    /// Refaz a lista de macros com o estado de cada uma e mostra a escolhida.
    pub fn preencher_macros(self: &Rc<Self>, d: &Dados) {
        let p = &self.macros;
        self.pilha.page(&self.pagina_macros).set_visible(d.macros.is_some());
        let Some(macros) = &d.macros else { return };
        let nomes: Vec<String> = macros
            .iter()
            .enumerate()
            .map(|(i, m)| match m.acoes.len() {
                0 => format!("Macro {} (vazia)", i + 1),
                n => format!("Macro {} ({n} ações)", i + 1),
            })
            .collect();
        let nomes_ref: Vec<&str> = nomes.iter().map(String::as_str).collect();
        // sem lista ainda, a seleção é inválida (um número enorme): começa na primeira macro
        let escolhida = Some(p.qual.selected()).filter(|i| (*i as usize) < macros.len()).unwrap_or(0);
        self.silencioso(|| {
            p.qual.set_model(Some(&gtk::StringList::new(&nomes_ref)));
            p.qual.set_selected(escolhida);
        });
        p.mostrando.set(None);
        self.mostrar_macro_de(d);
    }

    fn mostrar_macro(self: &Rc<Self>) {
        if self.carregando.get() {
            return;
        }
        let dados = self.d.borrow();
        if let Some(d) = dados.as_ref() {
            self.mostrar_macro_de(d);
        }
    }

    fn mostrar_macro_de(self: &Rc<Self>, d: &Dados) {
        let p = &self.macros;
        let indice = p.qual.selected();
        let Some(m) = d.macros.as_ref().and_then(|l| l.get(indice as usize)) else { return };
        // quais teclas disparam esta macro, sozinhas ou com Fn
        let codigo = mag82::codigo_da_macro(indice as u8);
        let mut teclas: Vec<String> = d
            .nomes
            .iter()
            .flat_map(|(pos, nome)| {
                let sozinha = (d.atual.get(pos) == Some(&codigo)).then(|| rotulo_longo(nome));
                let com_fn = (d.fn_atual.get(pos) == Some(&codigo)).then(|| format!("Fn + {}", rotulo_longo(nome)));
                [sozinha, com_fn].into_iter().flatten()
            })
            .collect();
        teclas.sort();
        p.usada_por.set_subtitle(&if teclas.is_empty() {
            "Nenhuma tecla ainda: ligue uma em Teclas → Funções".to_string()
        } else {
            teclas.join(", ")
        });
        if p.mostrando.replace(Some(indice)) == Some(indice) {
            return;
        }
        self.montar_macro(m);
        p.apagar.set_sensitive(m.valida);
    }

    /// Põe uma macro na tela, no lugar do que estiver montado.
    pub fn montar_macro(self: &Rc<Self>, m: &Macro) {
        let p = &self.macros;
        p.esvaziar();
        for passo in passos_de(&m.acoes) {
            PainelMacros::acrescentar(self, passo);
        }
        p.modo.set_selected(u32::from(m.modo.byte()));
        p.repeticoes.set_value(f64::from(m.repeticoes.max(1)));
    }

    /// Grava no teclado a macro montada na tela, ou apaga a escolhida.
    pub fn gravar_macro(self: &Rc<Self>, apagar: bool) {
        let indice = self.macros.qual.selected() as u8;
        let nova = if apagar { Macro::default() } else { self.macros.da_tela() };
        if !apagar && nova.acoes.is_empty() {
            self.avisar_erro("Adicione pelo menos um passo; para esvaziar a macro use Apagar macro");
            return;
        }
        // os erros de montagem aparecem antes de qualquer gravação
        if let Err(e) = nova.validar() {
            self.avisar_erro(&e.to_string());
            return;
        }
        self.pedir(
            move |t| {
                t.definir_macro(indice, &nova)?;
                t.macro_gravada(indice)
            },
            move |ui, lida| {
                if let Some(vaga) = ui.d.borrow_mut().as_mut().and_then(|d| d.macros.as_mut()).and_then(|l| l.get_mut(indice as usize)) {
                    *vaga = lida;
                }
                if let Some(d) = ui.d.borrow().as_ref() {
                    ui.preencher_macros(d);
                }
                ui.avisar(&if apagar {
                    format!("Macro {} apagada", indice + 1)
                } else {
                    format!("Macro {} gravada", indice + 1)
                });
            },
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn passos_viram_acoes_e_voltam() {
        let texto = "+lctrl c -lctrl 50ms v 20ms +a 30ms -a";
        // depois de soltar o Ctrl espera 50 ms; o V é um toque seguido de 20 ms; o A fica 30 ms segurado
        assert_eq!(passos_de(&Macro::acoes_de_texto(texto).unwrap()).iter().map(|p| p.espera).collect::<Vec<_>>(), [0, 0, 50, 20, 30, 0]);
        let acoes = Macro::acoes_de_texto(texto).unwrap();
        let passos = passos_de(&acoes);
        assert_eq!(passos.iter().map(|p| p.tipo).collect::<Vec<_>>(), [1, 0, 2, 0, 1, 2]);
        assert_eq!(acoes_de(&passos), acoes);
    }
}
