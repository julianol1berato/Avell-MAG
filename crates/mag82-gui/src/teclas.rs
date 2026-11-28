//! Página Teclas: o desenho do teclado fica fixo no topo e o painel de baixo muda conforme o
//! modo (Atuação, Remapear, Cor, SOCD). Cada modo define quantas teclas podem ser selecionadas
//! e o que o desenho mostra em cada uma.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::gdk;
use mag82::{Atuacao, Avancada, Campo, Erro, ModoSocd, Pos, Rgb};

use crate::ui::Ui;
use crate::util::{
    botao, botao_cor, de_rgba, descrever_funcao, icone_da_funcao, mm, para_rgba, rolavel, rotulo, rotulo_funcao, rotulo_longo,
};

/// Tamanho de uma tecla, em pixels: acompanha o tamanho da janela, dentro destes limites.
const U_MIN: f64 = 18.0;
const U_MAX: f64 = 64.0;
const U_PADRAO: f64 = 50.0;
/// Espaço entre teclas, em pixels.
const FOLGA: f64 = 4.0;
/// Fração da altura da página que o desenho do teclado pode ocupar.
const ALTURA_TECLADO: f64 = 0.42;
/// Abaixo desta largura de página, a linha de modos e seleção se divide em duas.
/// Posição da opção RS na lista do modo Pares, depois dos quatro modos do SOCD.
const OPCAO_RS: u32 = 4;
/// Abaixo desta largura os títulos dos modos encurtam para o seletor caber.
const LARGURA_TITULOS: i32 = 700;
/// Abaixo desta largura a legenda das marcas não cabe em uma linha e é escondida.
const LARGURA_LEGENDA: i32 = 760;
/// Abaixo deste tamanho de tecla não cabe a segunda linha de texto.
const U_COM_SUB: f64 = 36.0;
/// Movimento mínimo do mouse, em pixels, para um clique virar seleção por arrasto.
const ARRASTO_MIN: f64 = 8.0;

const SUB_RT: &str = "Reage ao movimento da tecla, não a um ponto fixo";
const SUB_ALTERADO: &str = "Alterado, ainda não aplicado";
const SUB_MISTO: &str = "Varia entre as teclas selecionadas; só muda se você editar";

/// Campos de atuação editáveis, com o passo do controle (o ponto de soltura acompanha o de atuação).
const CAMPOS: [(Campo, f64); 6] = [
    (Campo::Ponto, 0.05),
    (Campo::RtInicio, 0.05),
    (Campo::RtPress, 0.01),
    (Campo::RtSolta, 0.01),
    (Campo::ZonaTopo, 0.05),
    (Campo::ZonaFundo, 0.05),
];
/// Campos que só valem com o Rapid Trigger ligado.
const CAMPOS_RT: [Campo; 3] = [Campo::RtInicio, Campo::RtPress, Campo::RtSolta];

/// Conjunto de valores de atuação pronto para um uso; preenche os campos sem gravar nada.
/// Põe uma função na lista e na linha "Junto com". Uma combinação montada pelo usuário
/// (Ctrl + C) aparece como a tecla na lista e os modificadores marcados; as que têm nome próprio
/// no catálogo (Super + D) aparecem pelo nome. Devolve falso se a lista não tem a função.
pub fn mostrar_funcao(lista: &adw::ComboRow, mods: &crate::util::Modificadores, opcoes: &[(u16, String)], codigo: u16) -> bool {
    let escolher = |c: u16| opcoes.iter().position(|(o, _)| *o == c).map(|i| lista.set_selected(i as u32)).is_some();
    if escolher(codigo) {
        mods.permitir(crate::util::aceita_modificadores(codigo));
        mods.marcar(&[]);
        return true;
    }
    match mag82::combinacao(codigo) {
        Some((modificadores, tecla)) if escolher(tecla) => {
            mods.permitir(true);
            mods.marcar(&modificadores);
            true
        }
        _ => false,
    }
}

/// Exemplo pronto de par: as duas teclas, pelo que enviam de fábrica, e a opção da lista
/// "Com as duas pressionadas".
pub struct PredefDePar {
    pub titulo: &'static str,
    pub explicacao: &'static str,
    pub teclas: (u16, u16),
    pub opcao: u32,
}

pub const PREDEFS_DE_PAR: [PredefDePar; 2] = [
    PredefDePar {
        titulo: "A e D: vale a última pressionada",
        explicacao: "Para trocar de lado sem esperar soltar a outra tecla",
        teclas: (0x04, 0x07),
        opcao: 0,
    },
    PredefDePar {
        titulo: "W e S: vale a última pressionada",
        explicacao: "O mesmo, para frente e para trás",
        teclas: (0x1A, 0x16),
        opcao: 0,
    },
];

pub struct Predef {
    nome: &'static str,
    dica: &'static str,
    rt: bool,
    valores: &'static [(Campo, u16)],
}

// Competitivo e Digitação definem o ponto de atuação e tudo do Rapid Trigger; as zonas mortas
// são de cada tecla e ficam como estão. Só o padrão de fábrica define todos os campos.
pub const PREDEFS: [Predef; 3] = [
    Predef {
        nome: "Competitivo",
        dica: "Atuação em 0,50 mm e Rapid Trigger de 0,15 mm: resposta rápida para jogos (WASD)",
        rt: true,
        valores: &[(Campo::Ponto, 500), (Campo::RtInicio, 500), (Campo::RtPress, 150), (Campo::RtSolta, 150)],
    },
    Predef {
        nome: "Digitação",
        dica: "Atuação em 2,20 mm, sem Rapid Trigger: evita toques acidentais",
        rt: false,
        valores: &[(Campo::Ponto, 2200), (Campo::RtInicio, 500), (Campo::RtPress, 300), (Campo::RtSolta, 300)],
    },
    Predef {
        nome: "Padrão de fábrica",
        dica: "Todos os campos como o teclado vem de fábrica",
        rt: false,
        valores: &[
            (Campo::Ponto, 2000),
            (Campo::RtInicio, 500),
            (Campo::RtPress, 300),
            (Campo::RtSolta, 300),
            (Campo::ZonaTopo, 200),
            (Campo::ZonaFundo, 200),
        ],
    },
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Modo {
    Atuacao,
    Remapear,
    /// Atalhos com a tecla Fn: o que cada tecla faz enquanto Fn está pressionado.
    Fn,
    Cor,
    /// Pares de teclas: SOCD e RS.
    Socd,
    /// Funções avançadas de uma tecla só: DKS, multiponto, dupla função, contínuo e ao soltar.
    Avancada,
}

impl Modo {
    #[cfg_attr(not(feature = "autoteste"), allow(dead_code))] // o autoteste passa por todos
    pub const TODOS: [Modo; 6] = [Modo::Atuacao, Modo::Remapear, Modo::Fn, Modo::Cor, Modo::Socd, Modo::Avancada];

    pub fn nome(self) -> &'static str {
        match self {
            Modo::Atuacao => "atuacao",
            Modo::Remapear => "remapear",
            Modo::Fn => "fn",
            Modo::Cor => "cor",
            Modo::Socd => "socd",
            Modo::Avancada => "avancada",
        }
    }

    fn titulo(self) -> &'static str {
        match self {
            Modo::Atuacao => "Atuação",
            Modo::Remapear => "Remapear",
            Modo::Fn => "Atalhos Fn",
            Modo::Cor => "Cor fixa",
            Modo::Socd => "Pares",
            Modo::Avancada => "Avançada",
        }
    }

    /// Os modos que têm botão no seletor. Remapear e Atalhos Fn dividem o botão "Funções": são a
    /// mesma tarefa (o que a tecla envia) em duas camadas, escolhidas dentro do modo.
    pub const NO_SELETOR: [Modo; 5] = [Modo::Atuacao, Modo::Remapear, Modo::Cor, Modo::Socd, Modo::Avancada];

    /// O botão do seletor que representa este modo.
    fn botao(self) -> Modo {
        if self == Modo::Fn {
            Modo::Remapear
        } else {
            self
        }
    }

    /// Texto do botão do seletor.
    fn no_seletor(self) -> &'static str {
        match self.botao() {
            Modo::Remapear => "Funções",
            Modo::Cor => "Cor",
            outro => outro.titulo(),
        }
    }

    /// Título que cabe no seletor quando a janela é estreita.
    fn titulo_curto(self) -> &'static str {
        match self {
            Modo::Atuacao => "Atuação",
            Modo::Remapear | Modo::Fn => "Funções",
            Modo::Cor => "Cor",
            Modo::Socd => "Pares",
            Modo::Avancada => "Avanç.",
        }
    }

    /// Quantas teclas podem ficar selecionadas ao mesmo tempo (`None` = sem limite).
    pub fn limite(self) -> Option<usize> {
        match self {
            Modo::Atuacao | Modo::Cor => None,
            Modo::Remapear | Modo::Fn | Modo::Avancada => Some(1),
            Modo::Socd => Some(2),
        }
    }

    fn convite(self) -> &'static str {
        match self {
            Modo::Atuacao => "Clique ou arraste sobre as teclas que quer ajustar",
            Modo::Remapear => "Clique na tecla que quer remapear",
            Modo::Fn => "Clique numa tecla para ver ou mudar o que ela faz com Fn",
            Modo::Cor => "Clique ou arraste sobre as teclas que quer colorir",
            Modo::Socd => "Clique nas duas teclas do par, como A e D",
            Modo::Avancada => "Clique na tecla que vai ter a função avançada",
        }
    }
}

/// Verdadeiro se os dois retângulos (x, y, largura, altura) se tocam.
fn intersecta(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

pub struct Teclas {
    pagina: gtk::Box,
    fixo: gtk::Fixed,
    legenda: gtk::Box,
    /// Escolha da camada dentro do modo Funções.
    camadas: gtk::Box,
    camada_normal: gtk::ToggleButton,
    camada_fn: gtk::ToggleButton,
    css: gtk::CssProvider,
    /// Tamanho da letra das teclas, que acompanha o tamanho delas.
    css_escala: gtk::CssProvider,
    /// Tamanho atual de uma tecla, em pixels.
    u: Cell<f64>,
    /// Tamanho da página na última vez em que o desenho foi ajustado.
    tamanho: Cell<(i32, i32)>,
    titulo_sel: gtk::Label,
    todas: gtk::Button,
    /// O que fica ao lado de "Limpar": "Todas", as predefinições do modo, ou nada. É uma pilha
    /// para a linha não mudar de tamanho a cada troca de modo.
    ao_lado: gtk::Stack,
    nenhuma: gtk::Button,
    pub modo: Cell<Modo>,
    modo_btns: Vec<(Modo, gtk::ToggleButton)>,
    paineis: gtk::Stack,
    pub botoes: RefCell<BTreeMap<Pos, gtk::ToggleButton>>,
    /// Retângulo de cada tecla dentro de `fixo`, em pixels.
    areas: RefCell<BTreeMap<Pos, (f64, f64, f64, f64)>>,
    /// Teclas selecionadas, na ordem em que foram clicadas.
    pub ordem: RefCell<Vec<Pos>>,
    /// Seleção no momento em que um arrasto começou.
    base_arrasto: RefCell<Vec<Pos>>,
    arrastando: Cell<bool>,

    g_atu: adw::PreferencesGroup,
    pub rt: adw::SwitchRow,
    pub campos: Vec<(Campo, adw::SpinRow)>,
    /// Campos de atuação que o usuário editou e ainda não aplicou: só eles são gravados.
    pub tocados: RefCell<Vec<Campo>>,
    /// O mesmo para a chave do Rapid Trigger.
    rt_tocado: Cell<bool>,
    ap_atu: gtk::Button,
    predef_btns: Vec<gtk::Button>,

    g_cor: adw::PreferencesGroup,
    pub cor_btn: gtk::ColorDialogButton,
    ap_cor: gtk::Button,
    tira_cor: gtk::Button,

    g_fun: adw::PreferencesGroup,
    pub funcoes: Vec<(u16, String)>,
    pub funcao: adw::ComboRow,
    pub fun_mods: crate::util::Modificadores,
    pub fn_mods: crate::util::Modificadores,
    ap_fun: gtk::Button,
    padrao_fun: gtk::Button,

    g_fn: adw::PreferencesGroup,
    /// Opções do seletor de atalho: (código, descrição). A primeira é "função normal da tecla".
    pub fn_opcoes: Vec<(u16, String)>,
    pub fn_funcao: adw::ComboRow,
    ap_fn: gtk::Button,
    padrao_fn: gtk::Button,
    /// Lista dos atalhos que existem agora, refeita a cada mudança.
    g_atalhos: adw::PreferencesGroup,
    linhas_atalhos: RefCell<Vec<adw::ActionRow>>,

    pub socd_modo: adw::ComboRow,
    pub predefs_par: gtk::MenuButton,
    lista_predefs_par: gtk::ListBox,
    pub ap_socd: gtk::Button,
    tira_socd: gtk::Button,
}

impl Teclas {
    pub fn construir(tela: &gdk::Display) -> (Self, gtk::Box, crate::avancadas::PainelAvancadas, crate::curso::PainelCurso) {
        let css = gtk::CssProvider::new();
        let css_escala = gtk::CssProvider::new();
        for provedor in [&css, &css_escala] {
            gtk::style_context_add_provider_for_display(tela, provedor, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        // ----- desenho do teclado, legenda e linha de seleção (sempre à vista) -----
        let fixo = gtk::Fixed::new();
        let moldura = gtk::Box::builder().halign(gtk::Align::Center).css_classes(["painel-teclado"]).build();
        moldura.append(&fixo);
        // A rolagem sem barras existe só para o desenho não impor largura mínima à janela: assim
        // ela pode encolher, e o desenho é refeito menor logo em seguida.
        let vitrine = gtk::ScrolledWindow::builder()
            .child(&moldura)
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();

        let legenda = gtk::Box::builder().spacing(6).halign(gtk::Align::Center).css_classes(["legenda"]).build();
        for (marca, classe, texto) in [
            ("▁", "legenda-rt", "Rapid Trigger ligado"),
            ("▔", "legenda-socd", "faz parte de um par"),
            ("▔", "legenda-dks", "função avançada"),
            ("▏", "legenda-remap", "função trocada"),
            ("■", "", "fundo colorido = cor fixa"),
        ] {
            let m = gtk::Label::new(Some(marca));
            if !classe.is_empty() {
                m.add_css_class(classe);
            }
            legenda.append(&m);
            legenda.append(&gtk::Label::builder().label(texto).margin_end(14).build());
        }

        let titulo_sel = gtk::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["title-4"])
            .build();
        // comandos da seleção: discretos, porque o que importa na linha é o que está selecionado
        let todas = botao("Todas", false);
        todas.add_css_class("flat");
        todas.set_tooltip_text(Some("Selecionar todas as teclas (Ctrl+A)"));
        let nenhuma = botao("Limpar", false);
        nenhuma.add_css_class("flat");
        nenhuma.set_tooltip_text(Some("Limpar a seleção (Esc)"));

        // ----- seletor de modo: uma faixa só, centralizada, com as partes do mesmo tamanho -----
        let seletor = gtk::Box::builder().css_classes(["linked"]).homogeneous(true).hexpand(true).build();
        let mut modo_btns: Vec<(Modo, gtk::ToggleButton)> = Vec::new();
        for modo in Modo::NO_SELETOR {
            let b = gtk::ToggleButton::builder().label(modo.no_seletor()).build();
            match modo_btns.first() {
                Some((_, primeiro)) => b.set_group(Some(primeiro)),
                None => b.set_active(true),
            }
            seletor.append(&b);
            modo_btns.push((modo, b));
        }
        // dentro de Funções: a tecla sozinha ou com Fn
        let camada_normal = gtk::ToggleButton::builder().label("Tecla sozinha").active(true).hexpand(true).build();
        let camada_fn = gtk::ToggleButton::builder().label("Com Fn").group(&camada_normal).hexpand(true).build();
        let camadas = gtk::Box::builder().css_classes(["linked"]).homogeneous(true).visible(false).build();
        camadas.append(&camada_normal);
        camadas.append(&camada_fn);
        let selecao = gtk::Box::builder().spacing(4).build();
        selecao.append(&titulo_sel);
        let ao_lado = gtk::Stack::builder().hhomogeneous(true).vhomogeneous(true).build();
        ao_lado.add_named(&todas, Some("todas"));
        ao_lado.add_named(&gtk::Box::new(gtk::Orientation::Horizontal, 0), Some("nada"));
        selecao.append(&ao_lado);
        selecao.append(&nenhuma);
        // modos, seleção e camadas: três linhas na mesma coluna do painel de ajustes
        let linha_sel = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(10).build();
        linha_sel.append(&seletor);
        linha_sel.append(&selecao);
        linha_sel.append(&camadas);
        // os botões de ação ficam fora das linhas, para as linhas poderem estreitar com a janela
        let acoes = |botoes: &[&gtk::Button]| {
            let caixa = gtk::Box::builder().spacing(8).halign(gtk::Align::End).margin_top(12).build();
            for b in botoes {
                caixa.append(*b);
            }
            caixa
        };

        // ----- modo Atuação -----
        let g_atu = adw::PreferencesGroup::builder()
            .title("Atuação")
            .description("Distâncias em milímetros de curso da tecla. Só os campos que você editar são gravados.")
            .build();
        let ap_atu = botao("Aplicar atuação", true);
        g_atu.set_header_suffix(Some(&ap_atu));
        let linha_predef = gtk::Box::builder().spacing(8).homogeneous(true).build();
        linha_predef.set_tooltip_text(Some("Predefinições: preenchem os campos; nada é gravado até aplicar"));
        let predef_btns: Vec<gtk::Button> = PREDEFS
            .iter()
            .map(|p| {
                let b = botao(p.nome, false);
                b.set_tooltip_text(Some(p.dica));
                linha_predef.append(&b);
                b
            })
            .collect();
        let painel_atu = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
        let curso = crate::curso::PainelCurso::construir(tela);
        painel_atu.append(&curso.grupo);
        painel_atu.append(&linha_predef);
        painel_atu.append(&g_atu);
        let rt = adw::SwitchRow::builder().title("Rapid Trigger").subtitle(SUB_RT).build();
        let mut campos = Vec::new();
        for (campo, passo) in CAMPOS {
            let maximo = f64::from(mag82::CURSO_MAX) / 1000.0;
            let linha = adw::SpinRow::with_range(f64::from(campo.minimo()) / 1000.0, maximo, passo);
            linha.set_title(campo.titulo());
            linha.set_digits(2);
            g_atu.add(&linha);
            if campo == Campo::Ponto {
                g_atu.add(&rt);
            }
            campos.push((campo, linha));
        }

        // ----- modo Remapear -----
        let g_fun = adw::PreferencesGroup::builder()
            .title("Função da tecla")
            .description("O que a tecla selecionada envia ao computador. Digite na lista para procurar.")
            .build();
        let mut funcoes = mag82::funcoes();
        funcoes.sort_by_key(|(_, nome)| rotulo_longo(nome).to_lowercase());
        // depois das teclas, as funções especiais (multimídia, luzes, perfis), na ordem do catálogo
        let mut nomes_funcoes: Vec<String> = funcoes.iter().map(|(_, n)| rotulo_longo(n)).collect();
        let teclas_comuns = funcoes.clone();
        for e in mag82::ESPECIAIS.iter().filter(|e| e.familia != mag82::Familia::Reservada) {
            funcoes.push((e.codigo, e.nome.to_string()));
            nomes_funcoes.push(e.nome.to_string());
        }
        let nomes_ref: Vec<&str> = nomes_funcoes.iter().map(String::as_str).collect();
        let funcao = adw::ComboRow::builder()
            .title("Envia")
            .model(&gtk::StringList::new(&nomes_ref))
            .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
            .enable_search(true)
            .build();
        let padrao_fun = botao("Restaurar padrão", false);
        let ap_fun = botao("Aplicar", true);
        let fun_mods = crate::util::Modificadores::construir();
        g_fun.add(&funcao);
        g_fun.add(&fun_mods.linha);
        g_fun.add(&acoes(&[&padrao_fun, &ap_fun]));

        // ----- modo Atalhos Fn -----
        let g_fn = adw::PreferencesGroup::builder()
            .title("Atalho com Fn")
            .description("O que a tecla selecionada faz enquanto a tecla Fn está pressionada")
            .build();
        let mut fn_opcoes: Vec<(u16, String)> = vec![(mag82::FUNCAO_PADRAO, "Nada (a tecla continua normal)".to_string())];
        // primeiro as funções especiais, na ordem do catálogo; depois as teclas, em ordem alfabética
        fn_opcoes.extend(
            mag82::ESPECIAIS
                .iter()
                .filter(|e| e.familia != mag82::Familia::Reservada)
                .map(|e| (e.codigo, e.nome.to_string())),
        );
        fn_opcoes.extend(teclas_comuns.iter().filter(|(c, _)| *c != mag82::TECLA_FN).map(|(c, n)| (*c, format!("Tecla {}", rotulo_longo(n)))));
        let nomes_fn: Vec<&str> = fn_opcoes.iter().map(|(_, n)| n.as_str()).collect();
        let fn_funcao = adw::ComboRow::builder()
            .title("Com Fn, faz")
            .model(&gtk::StringList::new(&nomes_fn))
            .expression(gtk::PropertyExpression::new(gtk::StringObject::static_type(), gtk::Expression::NONE, "string"))
            .enable_search(true)
            .build();
        let padrao_fn = botao("Restaurar padrão", false);
        let ap_fn = botao("Aplicar", true);
        let fn_mods = crate::util::Modificadores::construir();
        g_fn.add(&fn_funcao);
        g_fn.add(&fn_mods.linha);
        g_fn.add(&acoes(&[&padrao_fn, &ap_fn]));
        let g_atalhos = adw::PreferencesGroup::builder()
            .title("Atalhos que existem agora")
            .description("Segure Fn e aperte a tecla. Clique num atalho para selecionar a tecla dele.")
            .build();
        let painel_fn = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(18).build();
        painel_fn.append(&g_fn);
        painel_fn.append(&g_atalhos);

        // ----- modo Cor -----
        let g_cor = adw::PreferencesGroup::builder()
            .title("Cor fixa")
            .description("As teclas selecionadas ficam nesta cor, por cima do efeito de luz")
            .build();
        let cor_btn = botao_cor();
        cor_btn.set_rgba(&para_rgba(Rgb(255, 0, 0)));
        let linha_cor = adw::ActionRow::builder().title("Cor").build();
        let tira_cor = botao("Voltar ao efeito", false);
        let ap_cor = botao("Aplicar cor", true);
        linha_cor.add_suffix(&cor_btn);
        g_cor.add(&linha_cor);
        g_cor.add(&acoes(&[&tira_cor, &ap_cor]));

        // ----- modo SOCD -----
        let g_socd = adw::PreferencesGroup::builder()
            .title("Par de teclas (SOCD e RS)")
            .description(
                "Escolha o que vale quando as duas teclas do par estão pressionadas ao mesmo tempo. \
                 As quatro primeiras opções são o SOCD; a última é o RS, em que vale a tecla mais afundada \
                 (as duas só saem juntas com ambas exatamente no fundo, o que é difícil de acertar).",
            )
            .build();
        let socd_modo = adw::ComboRow::builder().title("Com as duas pressionadas").build();
        let tira_socd = botao("Remover", false);
        let ap_socd = botao("Aplicar", true);
        let itens: Vec<(&str, &str)> = PREDEFS_DE_PAR.iter().map(|p| (p.titulo, p.explicacao)).collect();
        let (predefs_par, lista_predefs_par) = crate::util::menu_de_predefinicoes(&itens);
        ao_lado.add_named(&predefs_par, Some(Modo::Socd.nome()));
        g_socd.add(&socd_modo);
        g_socd.add(&acoes(&[&tira_socd, &ap_socd]));

        let paineis = gtk::Stack::builder().vexpand(true).build();
        paineis.add_named(&rolavel(&painel_atu, 820), Some(Modo::Atuacao.nome()));
        paineis.add_named(&rolavel(&g_fun, 820), Some(Modo::Remapear.nome()));
        paineis.add_named(&rolavel(&painel_fn, 820), Some(Modo::Fn.nome()));
        paineis.add_named(&rolavel(&g_cor, 820), Some(Modo::Cor.nome()));
        paineis.add_named(&rolavel(&g_socd, 820), Some(Modo::Socd.nome()));
        // nas funções avançadas o teclado só aceita teclas comuns: as especiais nem entram na lista
        let teclas_avancadas: Vec<(u16, String)> = teclas_comuns
            .iter()
            .filter(|(c, _)| mag82::serve_em_avancada(*c))
            .map(|(c, n)| (*c, rotulo_longo(n)))
            .collect();
        let avancadas = crate::avancadas::PainelAvancadas::construir(&teclas_avancadas);
        paineis.add_named(&rolavel(&avancadas.painel, 820), Some(Modo::Avancada.nome()));
        // o seletor de tipo fica com os outros seletores, abaixo da linha da seleção
        linha_sel.append(&avancadas.tipos);
        ao_lado.add_named(&avancadas.predefs, Some(Modo::Avancada.nome()));

        let topo = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(10)
            .margin_top(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        topo.append(&vitrine);
        // Como o desenho do teclado, a legenda e a linha de modos ficam em rolagens sem barras: assim
        // não impõem largura mínima à janela, e `ajustar_tamanho` as reorganiza quando ela estreita.
        let sem_minimo = |conteudo: &gtk::Widget| {
            gtk::ScrolledWindow::builder()
                .child(conteudo)
                .hscrollbar_policy(gtk::PolicyType::External)
                .vscrollbar_policy(gtk::PolicyType::Never)
                .build()
        };
        topo.append(&sem_minimo(legenda.upcast_ref()));
        // a mesma largura do painel de ajustes: as bordas de tudo o que fica abaixo do teclado alinham
        let linha_limitada = adw::Clamp::builder().child(&linha_sel).maximum_size(820).tightening_threshold(820).build();
        topo.append(&sem_minimo(linha_limitada.upcast_ref()));
        let pagina = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
        pagina.append(&topo);
        pagina.append(&paineis);

        let teclas = Self {
            pagina: pagina.clone(),
            fixo,
            legenda,
            camadas,
            camada_normal,
            camada_fn,
            css,
            css_escala,
            u: Cell::new(U_PADRAO),
            tamanho: Cell::new((0, 0)),
            titulo_sel,
            todas,
            ao_lado,
            nenhuma,
            modo: Cell::new(Modo::Atuacao),
            modo_btns,
            paineis,
            botoes: RefCell::new(BTreeMap::new()),
            areas: RefCell::new(BTreeMap::new()),
            ordem: RefCell::new(Vec::new()),
            base_arrasto: RefCell::new(Vec::new()),
            arrastando: Cell::new(false),
            g_atu,
            rt,
            campos,
            tocados: RefCell::new(Vec::new()),
            rt_tocado: Cell::new(false),
            ap_atu,
            predef_btns,
            g_cor,
            cor_btn,
            ap_cor,
            tira_cor,
            g_fun,
            fun_mods,
            fn_mods,
            funcoes,
            funcao,
            ap_fun,
            padrao_fun,
            g_fn,
            fn_opcoes,
            fn_funcao,
            ap_fn,
            padrao_fn,
            g_atalhos,
            linhas_atalhos: RefCell::new(Vec::new()),
            socd_modo,
            predefs_par,
            lista_predefs_par,
            ap_socd,
            tira_socd,
        };
        (teclas, pagina, avancadas, curso)
    }

    pub fn campo(&self, campo: Campo) -> &adw::SpinRow {
        &self.campos.iter().find(|(c, _)| *c == campo).expect("campo sem controle na tela").1
    }
}

pub fn ligar(ui: &Rc<Ui>) {
    let t = &ui.tec;
    macro_rules! clique {
        ($botao:expr, $acao:expr) => {{
            let ui2 = ui.clone();
            $botao.connect_clicked(move |_| $acao(&ui2));
        }};
    }
    clique!(t.todas, |u: &Rc<Ui>| u.marcar(true));
    clique!(t.nenhuma, |u: &Rc<Ui>| u.marcar(false));
    clique!(t.ap_atu, |u: &Rc<Ui>| u.aplicar_atuacao());
    clique!(t.ap_cor, |u: &Rc<Ui>| u.aplicar_cor(false));
    clique!(t.tira_cor, |u: &Rc<Ui>| u.aplicar_cor(true));
    clique!(t.ap_fun, |u: &Rc<Ui>| u.aplicar_funcao(false));
    // os modificadores só valem para teclas comuns: a linha acompanha a função escolhida
    let ui2 = ui.clone();
    t.funcao.connect_selected_notify(move |lista| {
        let t = &ui2.tec;
        t.fun_mods.permitir(t.funcoes.get(lista.selected() as usize).is_some_and(|(c, _)| crate::util::aceita_modificadores(*c)));
    });
    let ui2 = ui.clone();
    t.fn_funcao.connect_selected_notify(move |lista| {
        let t = &ui2.tec;
        t.fn_mods.permitir(t.fn_opcoes.get(lista.selected() as usize).is_some_and(|(c, _)| crate::util::aceita_modificadores(*c)));
    });
    clique!(t.padrao_fun, |u: &Rc<Ui>| u.aplicar_funcao(true));
    clique!(t.ap_fn, |u: &Rc<Ui>| u.aplicar_fn(false));
    clique!(t.padrao_fn, |u: &Rc<Ui>| u.aplicar_fn(true));
    clique!(t.ap_socd, |u: &Rc<Ui>| u.aplicar_socd());
    let ui2 = ui.clone();
    t.lista_predefs_par.connect_row_activated(move |_, linha| {
        ui2.tec.predefs_par.popdown();
        ui2.usar_predef_de_par(linha.index() as usize);
    });
    clique!(t.tira_socd, |u: &Rc<Ui>| u.remover_socd());
    for (i, b) in t.predef_btns.iter().enumerate() {
        let ui2 = ui.clone();
        b.connect_clicked(move |_| ui2.usar_predef(i));
    }
    for (modo, b) in &t.modo_btns {
        let (ui2, modo) = (ui.clone(), *modo);
        b.connect_toggled(move |b| {
            if b.is_active() && !ui2.carregando.get() {
                ui2.mudar_modo(modo);
            }
        });
    }
    let ui2 = ui.clone();
    t.camada_fn.connect_toggled(move |b| {
        if !ui2.carregando.get() {
            ui2.mudar_modo(if b.is_active() { Modo::Fn } else { Modo::Remapear });
        }
    });
    let ui2 = ui.clone();
    t.rt.connect_active_notify(move |_| {
        ui2.sensibilizar_rt();
        ui2.campo_editado(None);
    });
    for (campo, linha) in &t.campos {
        let (ui2, campo) = (ui.clone(), *campo);
        linha.connect_value_notify(move |_| ui2.campo_editado(Some(campo)));
    }

    // Arrastar sobre o teclado seleciona as teclas tocadas pelo retângulo. O gesto observa na
    // fase de captura e só assume o controle depois de um movimento mínimo, para o clique
    // simples continuar chegando à tecla.
    let arrasto = gtk::GestureDrag::new();
    arrasto.set_propagation_phase(gtk::PropagationPhase::Capture);
    let ui2 = ui.clone();
    arrasto.connect_drag_begin(move |_, _, _| {
        ui2.tec.arrastando.set(false);
        *ui2.tec.base_arrasto.borrow_mut() = ui2.tec.ordem.borrow().clone();
    });
    let ui2 = ui.clone();
    arrasto.connect_drag_update(move |gesto, dx, dy| {
        if ui2.tec.modo.get().limite().is_some() {
            return;
        }
        if !ui2.tec.arrastando.get() {
            if dx.hypot(dy) < ARRASTO_MIN {
                return;
            }
            ui2.tec.arrastando.set(true);
            gesto.set_state(gtk::EventSequenceState::Claimed);
        }
        if let Some((x, y)) = gesto.start_point() {
            ui2.selecionar_area((x.min(x + dx), y.min(y + dy), dx.abs(), dy.abs()));
        }
    });
    t.fixo.add_controller(arrasto);

    // a cada quadro desenhado confere se a página mudou de tamanho; se mudou, refaz o desenho
    let ui2 = ui.clone();
    t.pagina.add_tick_callback(move |pagina, _| {
        let tamanho = (pagina.width(), pagina.height());
        if tamanho != ui2.tec.tamanho.get() && tamanho.0 > 0 {
            ui2.tec.tamanho.set(tamanho);
            ui2.ajustar_tamanho();
        }
        gtk::glib::ControlFlow::Continue
    });
}

impl Ui {
    // ---------- desenho do teclado ----------

    pub fn montar_teclado(self: &Rc<Self>) {
        let t = &self.tec;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let mut botoes = t.botoes.borrow_mut();
        for b in botoes.values() {
            t.fixo.remove(b);
        }
        botoes.clear();
        let mut areas = t.areas.borrow_mut();
        areas.clear();
        t.ordem.borrow_mut().retain(|p| d.nomes.contains_key(p));
        let selecionadas = t.ordem.borrow().clone();
        for pos in d.retangulos().into_keys() {
            let b = gtk::ToggleButton::builder().css_classes(["tecla"]).name(format!("k{}_{}", pos.0, pos.1)).build();
            b.set_active(selecionadas.contains(&pos));
            let ui = self.clone();
            b.connect_toggled(move |b| ui.tecla_alternada(pos, b.is_active()));
            t.fixo.put(&b, 0.0, 0.0);
            botoes.insert(pos, b);
        }
        drop((botoes, areas, dados));
        self.ajustar_tamanho();
    }

    /// Calcula o tamanho de tecla que cabe na página e reposiciona o desenho. Também divide a
    /// linha de modos e seleção em duas, e esconde a legenda, quando a página é estreita.
    pub fn ajustar_tamanho(&self) {
        let t = &self.tec;
        let (largura, altura) = t.tamanho.get();
        // em janela muito estreita (letra grande) os títulos encurtam
        for (modo, botao) in &t.modo_btns {
            let curto = largura > 0 && largura < LARGURA_TITULOS;
            let titulo = if curto { modo.titulo_curto() } else { modo.no_seletor() };
            if botao.label().as_deref() != Some(titulo) {
                botao.set_label(titulo);
            }
        }
        t.legenda.set_visible(largura == 0 || largura >= LARGURA_LEGENDA);

        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let teclas = d.retangulos();
        let total_x = teclas.values().map(|r| r.0 + r.2).fold(1.0, f64::max);
        let total_y = teclas.values().map(|r| r.1 + r.3).fold(1.0, f64::max);
        let u = if largura > 0 {
            // 50 = margens da página e da moldura em volta do desenho
            let por_largura = (f64::from(largura) - 50.0) / total_x;
            let por_altura = f64::from(altura) * ALTURA_TECLADO / total_y;
            por_largura.min(por_altura).floor().clamp(U_MIN, U_MAX)
        } else {
            U_PADRAO
        };
        let mudou_sub = (u >= U_COM_SUB) != (t.u.get() >= U_COM_SUB);
        t.u.set(u);
        let mut areas = t.areas.borrow_mut();
        areas.clear();
        for (pos, b) in t.botoes.borrow().iter() {
            let Some((x, y, l, a)) = teclas.get(pos) else { continue };
            let folga = FOLGA.min(u * 0.1);
            let area = (x * u, y * u, l * u - folga, a * u - folga);
            b.set_size_request(area.2 as i32, area.3 as i32);
            t.fixo.move_(b, area.0, area.1);
            areas.insert(*pos, area);
        }
        t.css_escala.load_from_string(&format!(
            ".tecla {{ font-size: {:.1}px; }} .tecla .sub {{ font-size: {:.1}px; }}",
            (u * 0.24).max(7.5),
            (u * 0.18).max(7.0)
        ));
        drop((areas, dados));
        if mudou_sub {
            self.pintar_teclas();
        }
    }

    /// Atualiza legenda, marcas e cor de fundo de cada tecla. A segunda linha depende do modo.
    pub fn pintar_teclas(&self) {
        let t = &self.tec;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let modo = t.modo.get();
        let mut regras = String::new();
        for (pos, b) in t.botoes.borrow().iter() {
            let a = &d.atuacao[pos];
            let remapeada = (d.atual[pos] != d.fabrica[pos]).then(|| rotulo_funcao(d.atual[pos]));
            let (par, tipo_do_par) = match d.avancada[pos] {
                Avancada::Socd { par, .. } => (Some(d.nomes.get(&par).map(|n| rotulo(n)).unwrap_or_default()), "SOCD"),
                Avancada::Rs { par, .. } => (Some(d.nomes.get(&par).map(|n| rotulo(n)).unwrap_or_default()), "RS"),
                _ => (None, ""),
            };
            let sub = match modo {
                Modo::Atuacao => Some(mm(a.mm(Campo::Ponto))),
                Modo::Remapear => remapeada.as_ref().map(|novo| format!("→ {novo}")),
                Modo::Fn => (d.fn_atual[pos] != mag82::FUNCAO_PADRAO).then(|| rotulo_funcao(d.fn_atual[pos])),
                Modo::Cor => None,
                Modo::Socd => par.as_ref().map(|outra| format!("⇄ {outra}")),
                Modo::Avancada => crate::avancadas::sigla(&d.avancada[pos]).map(str::to_string),
            };
            // o desenho da função, quando ela tem um: mais fácil de reconhecer que a sigla
            let tamanho = (t.u.get() * 0.3).clamp(10.0, 16.0) as i32;
            let desenho = |codigo: u16| crate::util::desenho_da_funcao(codigo, tamanho);
            let caixa = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).build();
            // a tecla remapeada para uma função com desenho mostra o desenho ao lado do nome, em
            // todos os modos
            let nome = gtk::Label::new(Some(&rotulo(&d.nomes[pos])));
            match remapeada.as_ref().and_then(|_| desenho(d.atual[pos])) {
                Some(imagem) => {
                    let linha = gtk::Box::builder().spacing(3).halign(gtk::Align::Center).build();
                    linha.append(&nome);
                    linha.append(&imagem);
                    caixa.append(&linha);
                }
                None => caixa.append(&nome),
            }
            // na segunda linha dos modos Funções e Atalhos Fn, o desenho entra no lugar da sigla;
            // em Funções o desenho já está ao lado do nome, e a segunda linha fica só para o texto
            let desenho_sub = match modo {
                Modo::Fn if d.fn_atual[pos] != mag82::FUNCAO_PADRAO => desenho(d.fn_atual[pos]),
                _ => None,
            };
            let sub = match modo {
                Modo::Remapear if remapeada.is_some() && icone_da_funcao(d.atual[pos]).is_some() => None,
                _ => sub,
            };
            if t.u.get() >= U_COM_SUB {
                if let Some(imagem) = desenho_sub {
                    imagem.set_halign(gtk::Align::Center);
                    imagem.add_css_class("sub");
                    caixa.append(&imagem);
                } else if let Some(sub) = sub {
                    caixa.append(&gtk::Label::builder().label(sub).css_classes(["sub"]).build());
                }
            }
            b.set_child(Some(&caixa));

            let mut dica = vec![format!("Atuação: {} mm", mm(a.mm(Campo::Ponto)))];
            match (a.mm(Campo::ZonaTopo), a.mm(Campo::ZonaFundo)) {
                (0, 0) => {}
                (inicio, 0) => dica.push(format!("Zona morta: os primeiros {} mm", mm(inicio))),
                (0, fim) => dica.push(format!("Zona morta: os últimos {} mm", mm(fim))),
                (inicio, fim) => dica.push(format!("Zonas mortas: os primeiros {} mm e os últimos {} mm", mm(inicio), mm(fim))),
            }
            if a.rapid_trigger() {
                b.add_css_class("rt");
                dica.push("Rapid Trigger ligado".into());
            } else {
                b.remove_css_class("rt");
            }
            if let Some(novo) = &remapeada {
                dica.push(format!("Remapeada para {novo}"));
                // a marca da tecla remapeada aparece em todos os modos, não só em Funções: uma
                // barra na lateral, somada às outras marcas que a tecla tiver (a regra por tecla
                // passa por cima das regras de classe, por isso leva todas)
                let mut sombras = vec!["inset 4px 0 0 @purple_3"];
                if a.rapid_trigger() {
                    sombras.push("inset 0 -3px 0 @accent_bg_color");
                }
                if par.is_some() {
                    sombras.push("inset 0 3px 0 @warning_bg_color");
                } else if crate::avancadas::sigla(&d.avancada[pos]).is_some() {
                    sombras.push("inset 0 3px 0 @success_bg_color");
                }
                regras.push_str(&format!("#{} {{ box-shadow: {}; }}\n", b.widget_name(), sombras.join(", ")));
            }
            if d.fn_atual[pos] != mag82::FUNCAO_PADRAO {
                dica.push(format!("Com Fn: {}", descrever_funcao(d.fn_atual[pos])));
            }
            if let Some(outra) = &par {
                b.add_css_class("socd");
                dica.push(format!("{tipo_do_par} com {outra}"));
            } else {
                b.remove_css_class("socd");
            }
            if let Some(sigla) = crate::avancadas::sigla(&d.avancada[pos]) {
                b.add_css_class("dks");
                dica.push(format!("Função avançada: {sigla}"));
            } else {
                b.remove_css_class("dks");
            }
            if let Some(c) = d.grade.fixa(*pos) {
                let brilho = 0.299 * f64::from(c.0) + 0.587 * f64::from(c.1) + 0.114 * f64::from(c.2);
                let texto = if brilho > 140.0 { "#000000" } else { "#ffffff" };
                regras.push_str(&format!("#{} {{ background: {}; color: {texto}; }}\n", b.widget_name(), c.hex()));
                dica.push(format!("Cor fixa {}", c.hex()));
            }
            b.set_tooltip_text(Some(&dica.join("\n")));
        }
        t.css.load_from_string(&regras);
    }

    // ---------- seleção ----------

    pub fn selecionadas(&self) -> Vec<Pos> {
        self.tec.ordem.borrow().clone()
    }

    fn tecla_alternada(&self, pos: Pos, ativa: bool) {
        if self.carregando.get() {
            return;
        }
        {
            let mut ordem = self.tec.ordem.borrow_mut();
            ordem.retain(|p| *p != pos);
            if ativa {
                ordem.push(pos);
            }
        }
        self.aplicar_limite();
        self.atualizar_painel();
    }

    /// No modo de uma tecla (ou de duas), as mais antigas saem da seleção quando entra uma nova.
    fn aplicar_limite(&self) {
        let Some(limite) = self.tec.modo.get().limite() else { return };
        let excedentes: Vec<Pos> = {
            let mut ordem = self.tec.ordem.borrow_mut();
            let sobra = ordem.len().saturating_sub(limite);
            ordem.drain(..sobra).collect()
        };
        self.silencioso(|| {
            let botoes = self.tec.botoes.borrow();
            for pos in excedentes {
                if let Some(b) = botoes.get(&pos) {
                    b.set_active(false);
                }
            }
        });
    }

    /// Define a seleção inteira de uma vez, na ordem dada.
    pub fn selecionar(&self, teclas: &[Pos]) {
        self.silencioso(|| {
            for (pos, b) in self.tec.botoes.borrow().iter() {
                b.set_active(teclas.contains(pos));
            }
        });
        *self.tec.ordem.borrow_mut() = teclas.to_vec();
        self.aplicar_limite();
        self.atualizar_painel();
    }

    pub fn marcar(&self, todas: bool) {
        let teclas: Vec<Pos> = if todas { self.tec.botoes.borrow().keys().copied().collect() } else { Vec::new() };
        self.selecionar(&teclas);
    }

    /// Seleção por arrasto: o que já estava selecionado mais as teclas tocadas pelo retângulo.
    fn selecionar_area(&self, area: (f64, f64, f64, f64)) {
        let mut teclas = self.tec.base_arrasto.borrow().clone();
        for (pos, r) in self.tec.areas.borrow().iter() {
            if intersecta(*r, area) && !teclas.contains(pos) {
                teclas.push(*pos);
            }
        }
        if teclas != *self.tec.ordem.borrow() {
            self.selecionar(&teclas);
        }
    }

    pub fn mudar_modo(&self, modo: Modo) {
        let t = &self.tec;
        t.modo.set(modo);
        self.silencioso(|| {
            for (m, b) in &t.modo_btns {
                b.set_active(*m == modo.botao());
            }
            t.camada_fn.set_active(modo == Modo::Fn);
            t.camada_normal.set_active(modo != Modo::Fn);
        });
        t.camadas.set_visible(modo.botao() == Modo::Remapear);
        self.av.tipos.set_visible(modo == Modo::Avancada);
        t.paineis.set_visible_child_name(modo.nome());
        self.aplicar_limite();
        self.pintar_teclas();
        self.atualizar_painel();
    }

    /// Ajusta título, botões e campos do painel à seleção atual.
    pub fn atualizar_painel(&self) {
        let t = &self.tec;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let sel = self.selecionadas();
        let modo = t.modo.get();
        // sempre uma frase: o nome da tecla sozinho na linha não dizia o que era
        t.titulo_sel.set_label(&match sel[..] {
            [] => modo.convite().to_string(),
            [unica] => format!("Tecla selecionada: {}", rotulo_longo(&d.nomes[&unica])),
            [a, b] => format!("Teclas selecionadas: {} e {}", rotulo_longo(&d.nomes[&a]), rotulo_longo(&d.nomes[&b])),
            _ => format!("{} teclas selecionadas", sel.len()),
        });
        // Nos modos de uma ou duas teclas o botão "Todas" não serve: o lugar dele fica com as
        // predefinições do modo, ou vazio.
        t.ao_lado.set_visible_child_name(match modo {
            Modo::Socd | Modo::Avancada => modo.nome(),
            _ if modo.limite().is_none() => "todas",
            _ => "nada",
        });
        t.nenhuma.set_sensitive(!sel.is_empty());
        t.g_atu.set_sensitive(!sel.is_empty());
        t.g_cor.set_sensitive(!sel.is_empty());
        t.g_fun.set_sensitive(sel.len() == 1);
        // a própria tecla Fn não tem atalho: é ela que ativa a camada
        let e_fn = sel.first().is_some_and(|p| d.atual[p] == mag82::TECLA_FN || d.fabrica[p] == mag82::TECLA_FN);
        t.g_fn.set_sensitive(sel.len() == 1 && !e_fn);
        match sel[..] {
            [pos] if !e_fn => {
                let codigo = d.fn_atual[&pos];
                mostrar_funcao(&t.fn_funcao, &t.fn_mods, &t.fn_opcoes, codigo);
                t.fn_funcao.set_title(&format!("Fn + {} faz", rotulo(&d.nomes[&pos])));
                t.fn_funcao.set_subtitle(&if codigo == mag82::FUNCAO_PADRAO {
                    "Agora: nada (a tecla continua normal)".to_string()
                } else {
                    format!("Agora: {}", descrever_funcao(codigo))
                });
            }
            _ => {
                t.fn_funcao.set_title("Com Fn, faz");
                t.fn_funcao.set_subtitle(if e_fn { "A tecla Fn é a que ativa os atalhos" } else { "" });
            }
        }
        self.atualizar_avancada(&sel);
        t.socd_modo.set_sensitive(!sel.is_empty());
        t.ap_socd.set_sensitive(sel.len() == 2);
        t.tira_socd.set_sensitive(sel.iter().any(|p| matches!(d.avancada[p], Avancada::Socd { .. } | Avancada::Rs { .. })));

        // SOCD: as opções citam as teclas escolhidas; se já formam um par, mostra o modo em uso
        let (k1, k2) = match sel[..] {
            [a, b] => (rotulo(&d.nomes[&a]), rotulo(&d.nomes[&b])),
            _ => ("A primeira".to_string(), "A segunda".to_string()),
        };
        let opcoes = [
            "Vale a última pressionada".to_string(),
            format!("{k1} tem prioridade"),
            format!("{k2} tem prioridade"),
            "Nenhuma é enviada (neutro)".to_string(),
            "Vale a mais pressionada (RS)".to_string(),
        ];
        let opcoes_ref: Vec<&str> = opcoes.iter().map(String::as_str).collect();
        let mut escolhido = t.socd_modo.selected().min(OPCAO_RS);
        if let [a, b] = sel[..] {
            match d.avancada[&a] {
                Avancada::Socd { par, regra, .. } if par == b => {
                    escolhido = ModoSocd::TODOS.iter().position(|m| *m == ModoSocd::da_regra(regra)).unwrap_or(0) as u32;
                }
                Avancada::Rs { par, .. } if par == b => escolhido = OPCAO_RS,
                _ => {}
            }
        }
        t.socd_modo.set_model(Some(&gtk::StringList::new(&opcoes_ref)));
        t.socd_modo.set_selected(escolhido);

        let Some(primeira) = sel.first() else {
            drop(dados);
            self.esquecer_edicoes();
            return;
        };
        // Os campos mostram os valores da primeira tecla selecionada, menos os que o usuário já
        // editou: esses ficam como estão até aplicar.
        let a = &d.atuacao[primeira];
        let tocados = t.tocados.borrow().clone();
        self.silencioso(|| {
            if t.rt_tocado.get() {
                t.rt.set_subtitle(SUB_ALTERADO);
            } else {
                t.rt.set_active(a.rapid_trigger());
                let misto = sel.iter().any(|p| d.atuacao[p].rapid_trigger() != a.rapid_trigger());
                t.rt.set_subtitle(if misto { SUB_MISTO } else { SUB_RT });
            }
            for (campo, linha) in &t.campos {
                if tocados.contains(campo) {
                    linha.set_subtitle(SUB_ALTERADO);
                } else {
                    linha.set_value(f64::from(a.mm(*campo)) / 1000.0);
                    let misto = sel.iter().any(|p| d.atuacao[p].mm(*campo) != a.mm(*campo));
                    linha.set_subtitle(if misto { SUB_MISTO } else { "" });
                }
            }
        });
        self.sensibilizar_rt();
        if let Some(c) = d.grade.fixa(*primeira) {
            t.cor_btn.set_rgba(&para_rgba(c));
        }
        if sel.len() == 1 {
            mostrar_funcao(&t.funcao, &t.fun_mods, &t.funcoes, d.atual[primeira]);
        }
    }

    // ---------- modo Atuação ----------

    /// Os ajustes do Rapid Trigger só fazem sentido com ele ligado.
    fn sensibilizar_rt(&self) {
        let ligado = self.tec.rt.is_active();
        for campo in CAMPOS_RT {
            self.tec.campo(campo).set_sensitive(ligado);
        }
    }

    /// O usuário mexeu em um campo de atuação (`None` = chave do Rapid Trigger).
    pub fn campo_editado(&self, campo: Option<Campo>) {
        if self.carregando.get() {
            return;
        }
        let t = &self.tec;
        match campo {
            None => {
                t.rt_tocado.set(true);
                t.rt.set_subtitle(SUB_ALTERADO);
            }
            Some(campo) => {
                let mut tocados = t.tocados.borrow_mut();
                if !tocados.contains(&campo) {
                    tocados.push(campo);
                }
                t.campo(campo).set_subtitle(SUB_ALTERADO);
            }
        }
    }

    pub fn esquecer_edicoes(&self) {
        let t = &self.tec;
        t.tocados.borrow_mut().clear();
        t.rt_tocado.set(false);
        t.rt.set_subtitle(SUB_RT);
        for (_, linha) in &t.campos {
            linha.set_subtitle("");
        }
    }

    /// Preenche os campos com uma predefinição. Os campos dela ficam marcados como editados, à
    /// espera do Aplicar; os que ela não define voltam a mostrar o valor das teclas selecionadas,
    /// sem sobra de uma edição ou predefinição anterior.
    pub fn usar_predef(&self, indice: usize) {
        let Some(p) = PREDEFS.get(indice) else { return };
        let t = &self.tec;
        self.esquecer_edicoes();
        self.atualizar_painel();
        self.silencioso(|| {
            t.rt.set_active(p.rt);
            for (campo, valor) in p.valores {
                t.campo(*campo).set_value(f64::from(*valor) / 1000.0);
            }
        });
        self.sensibilizar_rt();
        self.campo_editado(None);
        for (campo, _) in p.valores {
            self.campo_editado(Some(*campo));
        }
    }

    /// Grava nas teclas selecionadas só os campos que o usuário editou; o resto de cada tecla
    /// fica como está.
    pub fn aplicar_atuacao(self: &Rc<Self>) {
        let t = &self.tec;
        let sel = self.selecionadas();
        let tocados = t.tocados.borrow().clone();
        let rt = t.rt_tocado.get().then(|| t.rt.is_active());
        if sel.is_empty() {
            return;
        }
        if tocados.is_empty() && rt.is_none() {
            self.avisar("Nada foi alterado na atuação");
            return;
        }
        let valores: Vec<(Campo, u16)> = t
            .campos
            .iter()
            .filter(|(c, _)| tocados.contains(c))
            .map(|(c, linha)| (*c, (linha.value() * 1000.0).round() as u16))
            .collect();
        // monta e confere todas antes de gravar: um valor inválido não deixa o lote pela metade
        let montadas: Result<Vec<(Pos, Atuacao)>, String> = {
            let dados = self.d.borrow();
            let Some(d) = dados.as_ref() else { return };
            sel.iter()
                .map(|pos| {
                    let mut a = d.atuacao[pos];
                    if let Some(rt) = rt {
                        a.definir_rapid_trigger(rt);
                    }
                    for (campo, v) in &valores {
                        a.definir_mm(*campo, *v);
                        if *campo == Campo::Ponto {
                            a.definir_mm(Campo::Soltar, *v);
                        }
                    }
                    match a.validar() {
                        Ok(()) => Ok((*pos, a)),
                        Err(e) if sel.len() > 1 => Err(format!("{}: {e}", rotulo(&d.nomes[pos]))),
                        Err(e) => Err(e.to_string()),
                    }
                })
                .collect()
        };
        let novas = match montadas {
            Ok(novas) => novas,
            Err(mensagem) => {
                self.avisar_erro(&mensagem);
                return;
            }
        };
        let total = novas.len();
        self.pedir(
            move |t| {
                // se o teclado falhar no meio, devolve o que já foi gravado junto com o erro
                let mut feitas: Vec<(Pos, Atuacao)> = Vec::new();
                for (pos, a) in novas {
                    match t.definir_atuacao(pos, &a) {
                        Ok(lida) => feitas.push((pos, lida)),
                        Err(e) => return Ok((feitas, Some(e))),
                    }
                }
                Ok((feitas, None::<Erro>))
            },
            move |ui, (feitas, erro)| {
                let gravadas = feitas.len();
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    d.atuacao.extend(feitas);
                }
                match erro {
                    None => {
                        ui.esquecer_edicoes();
                        ui.avisar(&format!("Atuação aplicada a {total} tecla(s)"));
                    }
                    Some(e) => ui.avisar_erro(&format!("{e}: só {gravadas} de {total} teclas foram alteradas")),
                }
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }

    // ---------- modo Cor ----------

    pub fn aplicar_cor(self: &Rc<Self>, limpar: bool) {
        let sel = self.selecionadas();
        let cor = (!limpar).then(|| de_rgba(&self.tec.cor_btn.rgba()));
        let Some(mut grade) = self.d.borrow().as_ref().map(|d| d.grade.clone()) else { return };
        if sel.is_empty() {
            return;
        }
        for pos in &sel {
            grade.definir(*pos, cor);
        }
        let total = sel.len();
        self.pedir(
            move |t| {
                t.definir_grade(&grade)?;
                Ok(grade)
            },
            move |ui, grade| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    d.grade = grade;
                }
                ui.avisar(&if limpar {
                    "Teclas de volta ao efeito".to_string()
                } else {
                    format!("Cor aplicada a {total} tecla(s)")
                });
                ui.pintar_teclas();
            },
        );
    }

    // ---------- modo Remapear ----------

    pub fn aplicar_funcao(self: &Rc<Self>, padrao: bool) {
        let [pos] = self.selecionadas()[..] else { return };
        let escolhida = self.tec.fun_mods.codigo(self.tec.funcoes[self.tec.funcao.selected() as usize].0);
        let Some(codigo) = self.d.borrow().as_ref().map(|d| if padrao { d.fabrica[&pos] } else { escolhida }) else {
            return;
        };
        self.pedir(
            move |t| {
                let novo = t.definir_funcao(pos, codigo)?;
                // se a tecla faz parte de um par (SOCD ou RS), o par foi regravado com a função nova
                let mut pares = vec![(pos, t.avancada(pos)?)];
                if let Avancada::Socd { par, .. } | Avancada::Rs { par, .. } = pares[0].1 {
                    pares.push((par, t.avancada(par)?));
                }
                Ok((novo, pares))
            },
            move |ui, (novo, pares)| {
                let mensagem = {
                    let mut dados = ui.d.borrow_mut();
                    let Some(d) = dados.as_mut() else { return };
                    d.atual.insert(pos, novo);
                    d.avancada.extend(pares);
                    format!("{} agora faz: {}", rotulo(&d.nomes[&pos]), descrever_funcao(novo))
                };
                ui.avisar(&mensagem);
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }

    // ---------- modo Atalhos Fn ----------

    /// Refaz a lista "Atalhos que existem agora" a partir do que o teclado tem na camada Fn.
    pub fn listar_atalhos(self: &Rc<Self>) {
        let t = &self.tec;
        let dados = self.d.borrow();
        let Some(d) = dados.as_ref() else { return };
        let mut linhas = t.linhas_atalhos.borrow_mut();
        for linha in linhas.drain(..) {
            t.g_atalhos.remove(&linha);
        }
        for (pos, codigo) in &d.fn_atual {
            let Some(nome) = d.nomes.get(pos) else { continue };
            if *codigo == mag82::FUNCAO_PADRAO {
                continue;
            }
            let mudou = d.fn_fabrica.get(pos) != Some(codigo);
            let linha = adw::ActionRow::builder()
                .title(format!("Fn + {}", rotulo_longo(nome)))
                .subtitle(format!("{}{}", descrever_funcao(*codigo), if mudou { "  ·  alterado por você" } else { "" }))
                .activatable(true)
                .build();
            let (ui, pos) = (self.clone(), *pos);
            linha.connect_activated(move |_| ui.selecionar(&[pos]));
            t.g_atalhos.add(&linha);
            linhas.push(linha);
        }
    }

    /// Grava o atalho de Fn da tecla selecionada (`padrao` = como vem de fábrica).
    pub fn aplicar_fn(self: &Rc<Self>, padrao: bool) {
        let [pos] = self.selecionadas()[..] else { return };
        let escolhido =
            self.tec.fn_mods.codigo(self.tec.fn_opcoes[(self.tec.fn_funcao.selected() as usize).min(self.tec.fn_opcoes.len() - 1)].0);
        let Some(codigo) = self.d.borrow().as_ref().map(|d| if padrao { d.fn_fabrica[&pos] } else { escolhido }) else {
            return;
        };
        self.pedir(
            move |t| t.definir_funcao_na_camada(mag82::CAMADA_FN, pos, codigo),
            move |ui, novo| {
                let mensagem = {
                    let mut dados = ui.d.borrow_mut();
                    let Some(d) = dados.as_mut() else { return };
                    d.fn_atual.insert(pos, novo);
                    let tecla = rotulo(&d.nomes[&pos]);
                    if novo == mag82::FUNCAO_PADRAO {
                        format!("Fn + {tecla} não faz mais nada de especial")
                    } else {
                        format!("Fn + {tecla} agora faz: {}", descrever_funcao(novo))
                    }
                };
                ui.avisar(&mensagem);
                ui.pintar_teclas();
                ui.listar_atalhos();
                ui.atualizar_painel();
            },
        );
    }

    // ---------- modo SOCD ----------

    /// Seleciona as duas teclas da predefinição e escolhe o modo do par, sem gravar.
    pub fn usar_predef_de_par(&self, indice: usize) {
        let Some(predef) = PREDEFS_DE_PAR.get(indice) else { return };
        let achar = |codigo: u16| self.d.borrow().as_ref().and_then(|d| d.fabrica.iter().find(|(_, c)| **c == codigo).map(|(p, _)| *p));
        let (Some(a), Some(b)) = (achar(predef.teclas.0), achar(predef.teclas.1)) else {
            self.avisar_erro("Este teclado não tem as teclas dessa predefinição");
            return;
        };
        self.selecionar(&[a, b]);
        self.tec.socd_modo.set_selected(predef.opcao);
        self.avisar("Predefinição montada na tela. Confira e clique em Aplicar");
    }

    pub fn aplicar_socd(self: &Rc<Self>) {
        let [a, b] = self.selecionadas()[..] else { return };
        let escolhido = self.tec.socd_modo.selected();
        let rs = escolhido == OPCAO_RS;
        let modo = ModoSocd::TODOS[(escolhido as usize).min(3)];
        let (antigos, cod_a, cod_b) = {
            let dados = self.d.borrow();
            let Some(d) = dados.as_ref() else { return };
            // uma tecla só participa de um par: os pares antigos são desfeitos antes do novo
            let antigos: Vec<Pos> = [a, b]
                .iter()
                .filter_map(|pos| match d.avancada[pos] {
                    Avancada::Socd { par, .. } | Avancada::Rs { par, .. } if par != a && par != b => Some(par),
                    _ => None,
                })
                .collect();
            (antigos, d.atual[&a], d.atual[&b])
        };
        // o par guarda a função das duas teclas, e o teclado só aceita teclas comuns ali: o aviso
        // vem antes de desfazer os pares antigos
        for (pos, codigo) in [(a, cod_a), (b, cod_b)] {
            if !mag82::serve_em_avancada(codigo) {
                let tecla = self.d.borrow().as_ref().map(|d| rotulo(&d.nomes[&pos])).unwrap_or_default();
                self.avisar_erro(&format!(
                    "A tecla {tecla} está com a função \"{}\", e pares só funcionam entre teclas comuns. \
                     Mude a função dela no modo Funções e tente de novo.",
                    descrever_funcao(codigo)
                ));
                return;
            }
        }
        self.pedir(
            move |t| {
                for par in &antigos {
                    t.remover_avancada(*par)?;
                }
                if rs {
                    t.definir_rs(a, b, cod_a, cod_b, 0)?;
                } else {
                    t.definir_socd(a, b, cod_a, cod_b, modo)?;
                }
                Ok((antigos, t.avancada(a)?, t.avancada(b)?))
            },
            move |ui, (antigos, av_a, av_b)| {
                let mensagem = {
                    let mut dados = ui.d.borrow_mut();
                    let Some(d) = dados.as_mut() else { return };
                    for par in antigos {
                        d.avancada.insert(par, Avancada::Nenhuma);
                    }
                    d.avancada.insert(a, av_a);
                    d.avancada.insert(b, av_b);
                    let tipo = if rs { "RS" } else { "SOCD" };
                    format!("{tipo} ligado entre {} e {}", rotulo(&d.nomes[&a]), rotulo(&d.nomes[&b]))
                };
                ui.avisar(&mensagem);
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }

    pub fn remover_socd(self: &Rc<Self>) {
        let mut alvos: Vec<Pos> = Vec::new();
        {
            let dados = self.d.borrow();
            let Some(d) = dados.as_ref() else { return };
            for pos in self.selecionadas() {
                if let Avancada::Socd { par, .. } | Avancada::Rs { par, .. } = d.avancada[&pos] {
                    for alvo in [pos, par] {
                        if !alvos.contains(&alvo) {
                            alvos.push(alvo);
                        }
                    }
                }
            }
        }
        if alvos.is_empty() {
            return;
        }
        self.pedir(
            move |t| {
                for alvo in &alvos {
                    t.remover_avancada(*alvo)?;
                }
                Ok(alvos)
            },
            |ui, alvos| {
                if let Some(d) = ui.d.borrow_mut().as_mut() {
                    for alvo in alvos {
                        d.avancada.insert(alvo, Avancada::Nenhuma);
                    }
                }
                ui.avisar("Par desfeito");
                ui.pintar_teclas();
                ui.atualizar_painel();
            },
        );
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn arrasto_pega_as_teclas_tocadas() {
        let tecla = (100.0, 50.0, 46.0, 46.0);
        assert!(intersecta(tecla, (90.0, 40.0, 20.0, 20.0))); // pega o canto
        assert!(intersecta(tecla, (110.0, 60.0, 1.0, 1.0))); // retângulo dentro da tecla
        assert!(intersecta(tecla, (0.0, 0.0, 500.0, 500.0))); // tecla dentro do retângulo
        assert!(!intersecta(tecla, (0.0, 0.0, 99.0, 500.0))); // para antes da tecla
        assert!(!intersecta(tecla, (147.0, 50.0, 30.0, 30.0))); // começa depois
    }

    #[test]
    fn predefinicoes_sao_validas() {
        for p in &PREDEFS {
            for (campo, valor) in p.valores {
                assert!(*valor >= campo.minimo() && *valor <= mag82::CURSO_MAX, "{}: {:?}", p.nome, campo);
                assert!(CAMPOS.iter().any(|(c, _)| c == campo), "{}: {:?} sem controle na tela", p.nome, campo);
            }
            // toda predefinição decide o ponto de atuação e tudo do Rapid Trigger
            for campo in [Campo::Ponto].into_iter().chain(CAMPOS_RT) {
                assert!(p.valores.iter().any(|(c, _)| *c == campo), "{}: falta {:?}", p.nome, campo);
            }
        }
    }
}
