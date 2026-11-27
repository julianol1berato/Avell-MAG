//! Autoteste (variável `MAG82_TESTE=prefixo`): exercita as ações da interface com gravações
//! reversíveis no teclado, salva uma imagem de cada tela em `prefixo-<tela>.png` e encerra.

use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use mag82::{Campo, Pos};

use crate::teclas::Modo;
use crate::ui::Ui;

const W: Pos = (2, 2);
const A: Pos = (3, 1);
const S: Pos = (3, 2);
const D: Pos = (3, 3);

pub fn iniciar(ui: Rc<Ui>, prefixo: String) {
    glib::spawn_future_local(async move {
        pausa(1200).await;
        ocioso(&ui).await;
        if ui.d.borrow().is_some() {
            exercitar(&ui, &prefixo).await;
            ui.mudar_modo(Modo::Atuacao);
            ui.selecionar(&[W, A, S, D]);
            for modo in Modo::TODOS {
                ui.mudar_modo(modo);
                fotografar(&ui, &prefixo, "teclas", &format!("teclas-{}", modo.nome())).await;
            }
            ui.mudar_modo(Modo::Atuacao);
        } else {
            println!("sem teclado: só a tela de espera");
        }
        fotografar(&ui, &prefixo, "luz", "luz").await;
        ui.luz.aba_lateral.set_active(true);
        fotografar(&ui, &prefixo, "luz", "luz-lateral").await;
        ui.luz.ap_cor_led.grab_focus();
        fotografar(&ui, &prefixo, "luz", "luz-fim").await;
        ui.luz.pagina.scroll_to_top();
        ui.luz.aba_teclas.set_active(true);
        fotografar(&ui, &prefixo, "disp", "disp").await;
        ui.mostrar_sobre();
        fotografar(&ui, &prefixo, "disp", "sobre").await;
        if let Some(dialogo) = ui.janela.visible_dialog() {
            dialogo.force_close();
        }
        pausa(300).await;
        // o diálogo de calibração é só aberto e fotografado: iniciar exige alguém apertando as teclas
        ui.abrir_calibracao();
        fotografar(&ui, &prefixo, "disp", "calibracao").await;
        ui.fechar_calibracao();
        pausa(400).await;
        println!("diálogo de calibração aberto e fechado sem iniciar: ativa={}", ui.cal.ativa.get());
        redimensionar(&ui, &prefixo).await;
        ui.janela.close();
    });
}

async fn pausa(ms: u64) {
    glib::timeout_future(Duration::from_millis(ms)).await;
}

/// Espera a fila do teclado esvaziar.
async fn ocioso(ui: &Rc<Ui>) {
    loop {
        pausa(40).await;
        if ui.ocupado.get() == 0 && !ui.conectando.get() {
            return;
        }
    }
}

async fn fotografar(ui: &Rc<Ui>, prefixo: &str, pagina: &str, nome: &str) {
    ui.pilha.set_visible_child_name(pagina);
    ui.limpar_aviso(); // as imagens mostram a tela, não o aviso do passo anterior
    pausa(700).await;
    let (larg, alt) = (ui.janela.width(), ui.janela.height());
    let snapshot = gtk::Snapshot::new();
    gtk::WidgetPaintable::new(Some(&ui.janela)).snapshot(&snapshot, f64::from(larg), f64::from(alt));
    let renderizador = ui.janela.native().and_then(|n| n.renderer());
    if let (Some(no), Some(r)) = (snapshot.to_node(), renderizador) {
        let area = gtk::graphene::Rect::new(0.0, 0.0, larg as f32, alt as f32);
        if let Err(e) = r.render_texture(&no, Some(&area)).save_to_png(format!("{prefixo}-{nome}.png")) {
            eprintln!("falha ao salvar imagem: {e}");
        }
    }
}

/// Fotografa as telas em vários tamanhos de janela, do mínimo ao maximizado.
async fn redimensionar(ui: &Rc<Ui>, prefixo: &str) {
    use crate::ui::{ALTURA_MIN, LARGURA_MIN};
    ui.avisar("Redimensionando"); // tira da frente o aviso de erro do passo anterior
    pausa(3500).await;
    let tamanhos = [(LARGURA_MIN, ALTURA_MIN), (LARGURA_MIN, 900), (640, 700), (800, ALTURA_MIN), (1280, 640), (1500, 1000)];
    for (largura, altura) in tamanhos {
        ui.janela.set_default_size(largura, altura);
        pausa(500).await;
        let nome = format!("r{largura}x{altura}");
        println!("janela pedida {largura}x{altura}: ficou {}x{}", ui.janela.width(), ui.janela.height());
        for (i, modo) in [Modo::Atuacao, Modo::Remapear, Modo::Cor, Modo::Socd, Modo::Dks].into_iter().enumerate() {
            // todos os modos no menor tamanho; nos outros, só a atuação (o painel mais cheio)
            if i == 0 || (largura, altura) == (LARGURA_MIN, ALTURA_MIN) {
                ui.mudar_modo(modo);
                fotografar(ui, prefixo, "teclas", &format!("{nome}-{}", modo.nome())).await;
            }
        }
        ui.mudar_modo(Modo::Atuacao);
        fotografar(ui, prefixo, "luz", &format!("{nome}-luz")).await;
        if (largura, altura) == (LARGURA_MIN, ALTURA_MIN) {
            ui.luz.aba_lateral.set_active(true);
            ui.luz.ap_cor_led.grab_focus();
            fotografar(ui, prefixo, "luz", &format!("{nome}-luz-fim")).await;
            ui.luz.pagina.scroll_to_top();
            ui.luz.aba_teclas.set_active(true);
        }
        fotografar(ui, prefixo, "disp", &format!("{nome}-disp")).await;
    }
    ui.janela.maximize();
    pausa(800).await;
    println!("maximizada: {}x{}", ui.janela.width(), ui.janela.height());
    fotografar(ui, prefixo, "teclas", "rmax-atuacao").await;
    fotografar(ui, prefixo, "luz", "rmax-luz").await;
    fotografar(ui, prefixo, "disp", "rmax-disp").await;
    ui.janela.unmaximize();
    pausa(500).await;
}

/// Resumo do que o autoteste altera, para conferir no fim que tudo voltou ao que era.
fn retrato(ui: &Ui) -> String {
    let dados = ui.d.borrow();
    let d = dados.as_ref().unwrap();
    let fixas: Vec<_> = d.nomes.keys().filter_map(|p| d.grade.fixa(*p).map(|c| (*p, c))).collect();
    let lateral = d.lateral.as_ref().map(|l| {
        let fixos: Vec<_> = (0..l.grade.leds()).filter_map(|i| l.grade.fixa_do_led(i).map(|c| (i, c))).collect();
        (l.luz, fixos)
    });
    format!("{:?} {:?} {:?} {:?} {:?} {:?} {:?} {}", d.atuacao, d.atual, d.fn_atual, d.avancada, fixas, d.luz, lateral, d.perfil_atual)
}

async fn exercitar(ui: &Rc<Ui>, prefixo: &str) {
    let t = &ui.tec;
    let lido = |p: Pos, c: Campo| ui.d.borrow().as_ref().unwrap().atuacao[&p].mm(c);
    let subtitulo = |c: Campo| t.campo(c).subtitle().map(|s| s.to_string()).unwrap_or_default();
    let antes = retrato(ui);

    // ----- atuação em uma tecla -----
    ui.selecionar(&[W]);
    t.campo(Campo::Ponto).set_value(1.5);
    t.rt.set_active(true);
    println!("RT ligado libera os campos de RT: {}", t.campo(Campo::RtPress).is_sensitive());
    ui.aplicar_atuacao();
    ocioso(ui).await;
    let rt = ui.d.borrow().as_ref().unwrap().atuacao[&W].rapid_trigger();
    println!("atuação W: rt={rt} ponto={}", lido(W, Campo::Ponto));
    t.campo(Campo::Ponto).set_value(2.0);
    t.rt.set_active(false);
    println!("RT desligado bloqueia os campos de RT: {}", !t.campo(Campo::RtPress).is_sensitive());
    ui.aplicar_atuacao();
    ocioso(ui).await;

    // ----- lote: W fica com RT ao pressionar diferente; editar só o ponto em W+A não iguala o resto -----
    t.campo(Campo::RtPress).set_value(0.5);
    ui.aplicar_atuacao();
    ocioso(ui).await;
    ui.selecionar(&[W, A]);
    println!("lote, aviso de misto: {:?}", subtitulo(Campo::RtPress));
    t.campo(Campo::Ponto).set_value(1.2);
    println!("lote, aviso de editado: {:?}", subtitulo(Campo::Ponto));
    ui.aplicar_atuacao();
    ocioso(ui).await;
    println!(
        "lote: ponto W={} A={}  rt_press W={} A={}",
        lido(W, Campo::Ponto),
        lido(A, Campo::Ponto),
        lido(W, Campo::RtPress),
        lido(A, Campo::RtPress)
    );
    t.campo(Campo::ZonaTopo).set_value(3.4);
    t.campo(Campo::ZonaFundo).set_value(3.4);
    ui.aplicar_atuacao();
    ocioso(ui).await;
    println!("zonas recusadas: topo W={} A={}", lido(W, Campo::ZonaTopo), lido(A, Campo::ZonaTopo));
    t.campo(Campo::ZonaTopo).set_value(0.2);
    t.campo(Campo::ZonaFundo).set_value(0.2);
    t.campo(Campo::Ponto).set_value(2.0);
    ui.aplicar_atuacao();
    ocioso(ui).await;
    ui.selecionar(&[W]);
    t.campo(Campo::RtPress).set_value(0.3);
    ui.aplicar_atuacao();
    ocioso(ui).await;

    // ----- predefinição: preenche e marca como editado, sem gravar -----
    ui.usar_predef(0);
    println!(
        "predefinição Competitivo: ponto na tela={:.2} rt={} editados={} gravado no teclado={}",
        t.campo(Campo::Ponto).value(),
        t.rt.is_active(),
        t.tocados.borrow().len(),
        lido(W, Campo::Ponto)
    );
    // trocar de predefinição não deixa sobra da anterior, e a zona morta mostra o valor da tecla
    t.campo(Campo::ZonaTopo).set_value(0.35);
    ui.usar_predef(1);
    println!(
        "depois de Digitação: rt ao pressionar na tela={:.2} rt={} zona do topo na tela={:.2} (na tecla={}) zonas marcadas={}",
        t.campo(Campo::RtPress).value(),
        t.rt.is_active(),
        t.campo(Campo::ZonaTopo).value(),
        lido(W, Campo::ZonaTopo),
        t.tocados.borrow().iter().any(|c| matches!(c, Campo::ZonaTopo | Campo::ZonaFundo))
    );
    ui.marcar(false);
    println!("limpar a seleção descarta as edições: {}", t.tocados.borrow().is_empty());

    // ----- cor -----
    ui.mudar_modo(Modo::Cor);
    ui.selecionar(&[W]);
    ui.aplicar_cor(false);
    ocioso(ui).await;
    println!("cor W: {:?}", ui.d.borrow().as_ref().unwrap().grade.fixa(W));
    ui.aplicar_cor(true);
    ocioso(ui).await;
    println!("cor W removida: {:?}", ui.d.borrow().as_ref().unwrap().grade.fixa(W));

    // ----- remapear: uma tecla por vez -----
    ui.mudar_modo(Modo::Remapear);
    t.botoes.borrow()[&A].set_active(true);
    println!("remapear aceita só uma tecla: {:?}", ui.selecionadas());
    ui.selecionar(&[W]);
    let tecla_a = t.funcoes.iter().position(|(c, _)| *c == 0x04).unwrap();
    t.funcao.set_selected(tecla_a as u32);
    ui.aplicar_funcao(false);
    ocioso(ui).await;
    println!("função W: {}", mag82::nome_tecla(ui.d.borrow().as_ref().unwrap().atual[&W]));
    ui.aplicar_funcao(true);
    ocioso(ui).await;
    println!("função W restaurada: {}", mag82::nome_tecla(ui.d.borrow().as_ref().unwrap().atual[&W]));

    // ----- atalho de Fn numa tecla que não tem (Q): cria, confere no teclado e desfaz -----
    const Q: Pos = (2, 1);
    ui.mudar_modo(Modo::Fn);
    let atalhos = ui.d.borrow().as_ref().unwrap().fn_atual.values().filter(|c| **c != mag82::FUNCAO_PADRAO).count();
    println!("atalhos de Fn lidos do teclado: {atalhos}");
    ui.selecionar(&[Q]);
    let volume = t.fn_opcoes.iter().position(|(c, _)| *c == 0x20E9).unwrap();
    t.fn_funcao.set_selected(volume as u32);
    ui.aplicar_fn(false);
    ocioso(ui).await;
    ui.pedir_bruto(
        false,
        |t| Ok((t.layout_da_camada(mag82::CAMADA_FN, false)?[&Q], t.layout(false)?[&Q])),
        |_, r| println!("Fn+Q no teclado (camada Fn, camada base): {:x?}", r.ok()),
    );
    ocioso(ui).await;
    ui.aplicar_fn(true);
    ocioso(ui).await;
    println!("Fn+Q restaurado: {:#x}", ui.d.borrow().as_ref().unwrap().fn_atual[&Q]);

    // ----- SOCD: duas teclas; a terceira tira a mais antiga -----
    ui.mudar_modo(Modo::Socd);
    for pos in [A, D] {
        t.botoes.borrow()[&pos].set_active(true);
    }
    println!("SOCD aceita só duas teclas: {:?}", ui.selecionadas());
    t.socd_modo.set_selected(3);
    ui.aplicar_socd();
    ocioso(ui).await;
    println!("socd A: {:?}", ui.d.borrow().as_ref().unwrap().avancada[&A]);
    ui.remover_socd();
    ocioso(ui).await;
    println!("socd removido: {:?}", ui.d.borrow().as_ref().unwrap().avancada[&D]);
    ui.marcar(false);

    // ----- DKS: monta no painel, grava, confere e remove -----
    const INSERT: Pos = (1, 14);
    ui.mudar_modo(Modo::Dks);
    ui.selecionar(&[INSERT]);
    let mut dks = mag82::Dks { teclas: [0xe1, 0x1a, 0, 0], inicio: 1200, ..Default::default() };
    dks.definir_casas(0, mag82::Dks::casas_de_texto("1-3").unwrap());
    dks.definir_casas(1, mag82::Dks::casas_de_texto("4").unwrap());
    ui.dks.mostrar(&dks);
    ui.aplicar_dks();
    ocioso(ui).await;
    let gravado = ui.d.borrow().as_ref().unwrap().avancada[&INSERT];
    println!("dks Insert: gravado igual ao montado={}", gravado == mag82::Avancada::Dks(dks));
    fotografar(ui, prefixo, "teclas", "teclas-dks-aplicado").await;
    ui.remover_dks();
    ocioso(ui).await;
    println!("dks removido: {:?}", ui.d.borrow().as_ref().unwrap().avancada[&INSERT]);
    ui.marcar(false);
    ui.mudar_modo(Modo::Atuacao);

    // ----- luz: grava pelo controle e confere lendo do teclado -----
    let brilho0 = ui.luz.teclas.brilho.value();
    ui.luz.teclas.brilho.set_value(77.0);
    pausa(300).await;
    ocioso(ui).await;
    ui.pedir_bruto(false, |t| t.luz(), |_, r| println!("brilho no teclado: {:?}", r.map(|l| l.brilho).ok()));
    ocioso(ui).await;
    ui.luz.teclas.brilho.set_value(brilho0);
    pausa(300).await;
    ocioso(ui).await;

    // ----- luz lateral: mesma coisa, na outra área -----
    if ui.d.borrow().as_ref().is_some_and(|d| d.lateral.is_some()) {
        let lateral0 = ui.luz.lateral.brilho.value();
        ui.luz.lateral.brilho.set_value(66.0);
        pausa(300).await;
        ocioso(ui).await;
        ui.pedir_bruto(
            false,
            |t| Ok((t.luz_de(mag82::AREA_LATERAL)?.brilho, t.luz()?.brilho)),
            |_, r| println!("brilho no teclado (lateral, teclas): {:?}", r.ok()),
        );
        ocioso(ui).await;
        ui.luz.lateral.brilho.set_value(lateral0);
        pausa(300).await;
        ocioso(ui).await;

        // cor fixa em dois LEDs da lateral, conferida lendo do teclado, e de volta ao efeito
        let fixos_no_teclado = |ui: &Rc<Ui>, rotulo: &'static str| {
            let area = ui.d.borrow().as_ref().unwrap().lateral.as_ref().unwrap().area;
            ui.pedir_bruto(
                false,
                move |t| t.grade_de(&area),
                move |_, r| {
                    let fixos: Vec<_> = r
                        .map(|g| (0..g.leds()).filter_map(|i| g.fixa_do_led(i).map(|c| (i + 1, c.hex()))).collect())
                        .unwrap_or_default();
                    println!("{rotulo}: {fixos:?}");
                },
            );
        };
        let ultimo = ui.luz.led_btns.borrow().len() - 1;
        for i in [0, ultimo] {
            ui.luz.led_btns.borrow()[i].set_active(true);
        }
        ui.aplicar_cor_lateral(false);
        ocioso(ui).await;
        fixos_no_teclado(ui, "LEDs fixos da lateral no teclado");
        ocioso(ui).await;
        ui.aplicar_cor_lateral(true);
        ocioso(ui).await;
        fixos_no_teclado(ui, "depois de voltar ao efeito");
        ocioso(ui).await;
        ui.marcar_leds(false);
    } else {
        println!("este teclado não tem luz lateral");
    }

    // ----- perfil pelo seletor, como o usuário faz -----
    let perfil0 = ui.perfil.selected();
    ui.perfil.set_selected(1);
    ocioso(ui).await;
    println!("seletor → perfil ativo: {:?}", ui.d.borrow().as_ref().map(|d| d.perfil_atual));
    ui.perfil.set_selected(perfil0);
    ocioso(ui).await;
    println!("seletor → de volta: {:?}", ui.d.borrow().as_ref().map(|d| d.perfil_atual));

    // ----- perfil trocado por fora (outro programa ou o próprio teclado): a tela acompanha -----
    let por_fora = |indice: u8| {
        std::thread::spawn(move || mag82::Teclado::abrir().and_then(|mut t| t.definir_perfil(indice)).is_ok())
    };
    println!("troca por fora enviada: {:?}", por_fora(2).join().ok());
    pausa(2500).await;
    ocioso(ui).await;
    println!(
        "troca por fora percebida: perfil {:?}, seletor em {}",
        ui.d.borrow().as_ref().map(|d| d.perfil_atual),
        ui.perfil.selected()
    );
    let _ = por_fora(perfil0 as u8).join();
    pausa(2500).await;
    ocioso(ui).await;
    println!("de volta por fora: perfil {:?}", ui.d.borrow().as_ref().map(|d| d.perfil_atual));

    // ----- falha numa gravação: a tela volta a mostrar o que está no teclado, sem perder edições -----
    ui.selecionar(&[W]);
    t.campo(Campo::Ponto).set_value(1.1);
    ui.silencioso(|| ui.luz.teclas.brilho.set_value(11.0)); // tela diferente do teclado, como após uma gravação que falhou
    ui.pedir(|_| Err::<(), _>(mag82::Erro::SemResposta([0; 4])), |_, ()| println!("ERRO: a falha simulada passou"));
    ocioso(ui).await;
    println!(
        "depois da falha: tela={} brilho voltou={} edição preservada={}",
        ui.tela(),
        ui.luz.teclas.brilho.value() == brilho0,
        t.tocados.borrow().contains(&Campo::Ponto)
    );
    ui.marcar(false);

    // ----- teclado some e volta: reconecta sozinho -----
    ui.sem_teclado("desconexão simulada");
    println!("sem teclado: tela={}", ui.tela());
    pausa(2500).await;
    ocioso(ui).await;
    println!("reconectou sozinho: tela={} dados={}", ui.tela(), ui.d.borrow().is_some());

    println!("restaurado: {}", antes == retrato(ui));
}
