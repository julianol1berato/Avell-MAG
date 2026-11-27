//! A linha de comando contra o teclado simulado: cada teste roda o programa de verdade, com
//! `MAG82_SIMULADO`, e confere o que ele responde. Cada execução começa do estado inicial.

use std::process::Command;

fn mag82(args: &[&str]) -> (bool, String) {
    let pasta = std::env::temp_dir().join(format!("mag82-cli-teste-{}", std::process::id()));
    let saida = Command::new(env!("CARGO_BIN_EXE_mag82"))
        .args(args)
        .env("MAG82_SIMULADO", "1")
        .env("XDG_STATE_HOME", &pasta)
        .output()
        .expect("não foi possível rodar o mag82");
    let texto = format!("{}{}", String::from_utf8_lossy(&saida.stdout), String::from_utf8_lossy(&saida.stderr));
    (saida.status.success(), texto)
}

fn ok(args: &[&str]) -> String {
    let (certo, texto) = mag82(args);
    assert!(certo, "mag82 {args:?} falhou:\n{texto}");
    texto
}

#[test]
fn le_o_dispositivo() {
    let texto = ok(&["dispositivo"]);
    assert!(texto.contains("firmware:   0.0.2.0"), "{texto}");
    assert!(ok(&["info"]).contains("teclado simulado"));
    assert!(ok(&["teclas"]).starts_with("esc  f1"));
}

#[test]
fn grava_e_le_de_volta() {
    assert!(ok(&["taxa", "1000"]).contains("atual:      1000 Hz"));
    assert!(ok(&["luz", "--brilho", "42", "--modo", "3"]).contains("brilho: 42"));
    assert!(ok(&["luz", "--lateral", "--modo", "1"]).contains("modo: 1 (Onda)"));
    assert!(ok(&["paleta", "1", "ff8800"]).contains("cor 1: #ff8800  Laranja"));
    assert!(ok(&["paleta", "--lateral", "2", "0000ff"]).contains("cor 2: #0000ff  Azul"));
    assert!(ok(&["perfil", "3"]).contains("* 3:"));
    assert!(ok(&["remapear", "caps", "lctrl"]).contains("caps       -> lctrl"));
    assert!(ok(&["atalhos", "pgdn", "home"]).contains("home"));
    assert!(ok(&["cor", "w", "--cor", "00ff00"]).contains("w          #00ff00"));
    assert!(ok(&["cor", "--lateral", "7", "--cor", "ff0000"]).contains("LED 7      #ff0000"));
    assert!(ok(&["atuacao", "w", "--ponto", "0.5", "--rt", "on"]).contains("0.50"));
}

#[test]
fn teclas_avancadas() {
    assert!(ok(&["socd", "a", "d", "--modo", "neutro"]).contains("a + d: neutro"));
    let texto = ok(&["dks", "insert", "--acao", "lshift:1-3", "--acao", "w:4", "--inicio", "1.2"]);
    assert!(texto.contains("insert: início 1.20 mm, fundo 3.00 mm"), "{texto}");
    assert!(texto.contains("lshift     momentos 1-3") && texto.contains("w          momentos 4"), "{texto}");
}

#[test]
fn teclas_avancadas_de_uma_tecla() {
    let texto = ok(&["mt", "caps", "--toque", "esc", "--segura", "lctrl", "--tempo", "250"]);
    assert!(texto.contains("caps: dupla função: toque esc, segurar lctrl (a partir de 250 ms)"), "{texto}");
    let texto = ok(&["tgl", "f8", "--funcao", "w"]);
    assert!(texto.contains("f8: contínuo: w fica pressionada"), "{texto}");
    let texto = ok(&["mpt", "w", "--acao", "w:0.5", "--acao", "lshift:2.5"]);
    assert!(texto.contains("w: multiponto: w a partir de 0.50 mm, lshift a partir de 2.50 mm"), "{texto}");
    let texto = ok(&["end", "q", "--pressionar", "q", "--soltar", "e", "--atraso", "5"]);
    assert!(texto.contains("q: ao soltar: q ao pressionar, e ao soltar (atraso 5 ms)"), "{texto}");
    let texto = ok(&["rs", "a", "d"]);
    assert!(texto.contains("a: RS com d: vale a mais pressionada"), "{texto}");
    assert!(ok(&["avancadas"]).contains("nenhuma tecla com função avançada"), "cada execução começa do estado inicial");
}

#[test]
fn recusa_avancadas_invalidas() {
    for args in [
        &["mt", "caps", "--toque", "esc"][..],
        &["mt", "caps", "--toque", "esc", "--segura", "lctrl", "--tempo", "5"],
        &["mpt", "w", "--acao", "a:2.0", "--acao", "b:1.0"],
        &["mpt", "w", "--acao", "a:9"],
        &["end", "q", "--pressionar", "q", "--atraso", "500"],
        &["rs", "a"],
        &["tgl", "f8", "--remover"],
    ] {
        let (certo, texto) = mag82(args);
        assert!(!certo, "mag82 {args:?} deveria ser recusado:\n{texto}");
    }
}

#[test]
fn macros() {
    assert!(ok(&["macro"]).contains("nenhuma macro gravada (o teclado guarda 16)"));
    let texto = ok(&["macro", "3", "--acoes", "+lctrl c -lctrl 50ms v", "--repetir", "2"]);
    assert!(texto.contains("macro 3: +lctrl c -lctrl 50ms v"), "{texto}");
    assert!(texto.contains("2 vez(es)"), "{texto}");
    let texto = ok(&["macro", "1", "--acoes", "a b", "--modo", "segurar"]);
    assert!(texto.contains("Roda enquanto a tecla está pressionada; soltar para na hora"), "{texto}");
    assert!(ok(&["remapear", "f9", "macro3"]).contains("f9         -> macro3"));
    for args in [
        &["macro", "1", "--acoes", "+a"][..],
        &["macro", "1", "--acoes", "macro2"],
        &["macro", "17", "--acoes", "a"],
        &["macro", "1", "--acoes", "a", "--repetir", "0"],
        &["macro", "1", "--acoes", "naoexiste"],
    ] {
        let (certo, texto) = mag82(args);
        assert!(!certo, "mag82 {args:?} deveria ser recusado:\n{texto}");
    }
}

#[test]
fn recusa_o_que_nao_pode() {
    let (certo, texto) = mag82(&["atuacao", "w", "--ponto", "9"]);
    assert!(!certo, "ponto de 9 mm deveria ser recusado:\n{texto}");
    let (certo, texto) = mag82(&["bruto", "020600"]);
    assert!(!certo && texto.contains("bloqueado"), "{texto}");
    let (certo, texto) = mag82(&["dks", "insert", "--acao", "a:9"]);
    assert!(!certo && texto.contains("momentos inválidos"), "{texto}");
    let (certo, _) = mag82(&["cor", "w", "--cor", "xyz"]);
    assert!(!certo);
}

#[test]
fn calibra_ate_o_fim() {
    let texto = ok(&["calibrar"]);
    assert!(texto.contains("83 de 83 teclas calibradas"), "{texto}");
}
