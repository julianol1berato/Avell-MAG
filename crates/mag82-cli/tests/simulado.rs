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

/// O teclado só aceita teclas comuns nas funções avançadas: multimídia, luzes, perfis e macros
/// são recusados antes de gravar, com o nome da função, e a tecla fica como estava.
#[test]
fn avancadas_recusam_funcoes_especiais() {
    for args in [
        &["mpt", "insert", "--acao", "0x20e9:0.5", "--acao", "0x20b5:1.5", "--acao", "0x2183:2.5"][..],
        &["mt", "caps", "--toque", "0x20e9", "--segura", "lctrl"],
        &["tgl", "f8", "--funcao", "0xf301"],
        &["end", "q", "--pressionar", "q", "--soltar", "macro1"],
        &["dks", "insert", "--acao", "0xf205:1"],
    ] {
        let (certo, texto) = mag82(args);
        assert!(!certo, "mag82 {args:?} deveria ser recusado:\n{texto}");
        assert!(texto.contains("só aceitam teclas comuns"), "{texto}");
        assert!(!texto.contains("não confirmou"), "{texto}");
    }
    let (_, texto) = mag82(&["mpt", "insert", "--acao", "0x20e9:0.5"]);
    assert!(texto.contains("\"Aumentar o volume\" não pode ser usada no multiponto"), "{texto}");
    // tecla com modificadores (Ctrl + D): o teclado grava, mas envia só a tecla; o programa recusa
    let (certo, texto) = mag82(&["mpt", "insert", "--acao", "0x0107:0.5"]);
    assert!(!certo && texto.contains("\"Ctrl + d\"") && texto.contains("envia só a tecla"), "{texto}");
    let (certo, texto) = mag82(&["bruto", "0602010e020701"]);
    assert!(certo && texto.starts_with("06 02 01 0e 02 07 01"), "o simulado aceita como o teclado: {texto}");
    // o que a biblioteca deixa passar e o teclado recusa aparece como recusa, na hora
    let (certo, texto) = mag82(&["bruto", "0602010e02e920"]);
    assert!(certo && texto.starts_with("06 02 01 0e ff"), "{texto}");
}

#[test]
fn macros() {
    assert!(ok(&["macro"]).contains("nenhuma macro gravada (o teclado guarda 8)"));
    let texto = ok(&["macro", "3", "--acoes", "+lctrl c -lctrl 50ms v", "--repetir", "2"]);
    assert!(texto.contains("macro 3: +lctrl c -lctrl 50ms v"), "{texto}");
    assert!(texto.contains("2 vez(es)"), "{texto}");
    let texto = ok(&["macro", "1", "--acoes", "a b", "--modo", "segurar"]);
    assert!(texto.contains("Roda enquanto a tecla está pressionada, até as repetições; soltar para na hora, 1 vez(es)"), "{texto}");
    assert!(ok(&["remapear", "f9", "macro3"]).contains("f9         -> macro3"));
    // as macros dividem 224 ações: uma de 240 não cabe
    let longa = vec!["a"; 120].join(" ");
    let (certo, texto) = mag82(&["macro", "1", "--acoes", &longa]);
    assert!(!certo && texto.contains("cabem 224"), "{texto}");
    let (certo, texto) = mag82(&["macro", "2", "--apagar"]);
    assert!(certo, "apagar uma vaga vazia não é erro:\n{texto}");
    for args in [
        &["macro", "1", "--acoes", "+a"][..],
        &["macro", "1", "--acoes", "macro2"],
        &["macro", "9", "--acoes", "a"],
        &["macro", "1", "--acoes", "a", "--repetir", "0"],
        &["macro", "1", "--acoes", "naoexiste"],
    ] {
        let (certo, texto) = mag82(args);
        assert!(!certo, "mag82 {args:?} deveria ser recusado:\n{texto}");
    }
}

#[test]
fn funcoes_pelo_nome() {
    let lista = ok(&["funcoes"]);
    assert!(lista.contains("abrir-a-calculadora") && lista.contains("mouse-botao-esquerdo") && lista.contains("num7"), "{lista}");
    assert!(ok(&["remapear", "insert", "abrir-a-calculadora"]).contains("insert     -> abrir-a-calculadora"));
    assert!(ok(&["remapear", "f9", "ctrl+shift+esc"]).contains("f9         -> ctrl+shift+esc"));
    assert!(ok(&["remapear", "f9", "super+d"]).contains("f9         -> super+d"));
    assert!(ok(&["remapear", "f10", "mouse-botao-direito"]).contains("f10        -> mouse-botao-direito"));
    assert!(ok(&["atalhos", "pgdn", "num+"]).contains("num+"));
    // numa tecla avançada, função especial e tecla com modificadores continuam recusadas
    for args in [
        &["mt", "caps", "--toque", "ctrl+c", "--segura", "lctrl"][..],
        &["tgl", "f8", "--funcao", "mouse-botao-esquerdo"],
        &["dks", "insert", "--acao", "mouse-botao-esquerdo:1"],
    ] {
        let (certo, texto) = mag82(args);
        assert!(!certo && texto.contains("teclas comuns"), "mag82 {args:?}:\n{texto}");
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

#[test]
fn exporta_e_importa() {
    let pasta = std::env::temp_dir().join(format!("mag82-cli-copia-{}", std::process::id()));
    std::fs::create_dir_all(&pasta).unwrap();
    let arquivo = pasta.join("teclado.mag82");
    let caminho = arquivo.to_str().unwrap();
    assert!(ok(&["exportar", caminho]).contains("configuração salva"));
    let original = std::fs::read_to_string(&arquivo).unwrap();
    for secao in ["[atuacao]", "[funcoes]", "[avancadas]", "[macros]", "[luz-teclas]", "[luz-lateral]"] {
        assert!(original.contains(secao), "falta {secao}:\n{original}");
    }
    // o mesmo teclado, o mesmo arquivo: nada a gravar
    assert!(ok(&["importar", caminho]).contains("nada a gravar"));

    // muda um pouco de cada parte e importa
    let mut editado = String::new();
    for linha in original.lines() {
        let nova = match linha {
            "[funcoes]" => "[funcoes]\n0 1,14 abrir-a-calculadora\n1 2,1 ctrl+c".to_string(),
            "[avancadas]" => "[avancadas]\nsocd 3,1 3,3 neutro\nmt 3,0 esc lctrl 250".to_string(),
            "[macros]" => "[macros]\n2 toque 3 +lctrl c -lctrl 50ms v".to_string(),
            l if l.starts_with("brilho ") => "brilho 37".to_string(),
            l if l.starts_with("2,2 ") => "2,2   sim  1.20 1.20 0.50 0.15 0.15 0.00 0.00".to_string(),
            l => l.to_string(),
        };
        editado.push_str(&nova);
        editado.push('\n');
    }
    editado.push_str("fixa 3 ff0000\n"); // a última seção é a da luz lateral
    std::fs::write(&arquivo, &editado).unwrap();
    let texto = ok(&["importar", caminho, "--simular"]);
    assert!(texto.contains("mudaria:") && !texto.contains("importada"), "{texto}");
    let texto = ok(&["importar", caminho]);
    for parte in [
        "atuação de 1 tecla",
        "função de 2 teclas",
        "função avançada de 3 teclas",
        "1 macro",
        "luz das teclas (efeito, brilho ou paleta)",
        "luz lateral (efeito, brilho ou paleta)",
        "cor fixa de 1 LED da lateral",
        "configuração anterior guardada em",
        "configuração importada e conferida",
    ] {
        assert!(texto.contains(parte), "falta \"{parte}\":\n{texto}");
    }

    // o que o programa não conhece não é gravado
    std::fs::write(&arquivo, "mag82-configuracao 1\n[funcoes]\n0 1,14 0xf203\n").unwrap();
    let (certo, texto) = mag82(&["importar", caminho]);
    assert!(!certo && texto.contains("não conhece essa função"), "{texto}");
    std::fs::write(&arquivo, "mag82-configuracao 1\n[avancadas]\nsocd 3,1 9,9 neutro\n").unwrap();
    let (certo, texto) = mag82(&["importar", caminho]);
    assert!(!certo && texto.contains("não tem tecla na posição 9,9"), "{texto}");
    std::fs::remove_dir_all(&pasta).ok();
}

#[test]
fn troca_automatica_de_perfil() {
    // pasta de ajustes própria: o teste não mexe na configuração de quem o roda
    let pasta = std::env::temp_dir().join(format!("mag82-cli-automatico-{}", std::process::id()));
    std::fs::create_dir_all(&pasta).unwrap();
    let rodar = |args: &[&str]| {
        let saida = Command::new(env!("CARGO_BIN_EXE_mag82"))
            .args(args)
            .env("MAG82_SIMULADO", "1")
            .env("XDG_CONFIG_HOME", &pasta)
            .env("XDG_STATE_HOME", &pasta)
            .env("XDG_RUNTIME_DIR", &pasta)
            .output()
            .expect("não foi possível rodar o mag82");
        (saida.status.success(), format!("{}{}", String::from_utf8_lossy(&saida.stdout), String::from_utf8_lossy(&saida.stderr)))
    };
    assert!(rodar(&["automatico"]).1.contains("desligada"));
    // o próprio mag82 é um programa aberto enquanto roda: serve de programa da regra
    let eu = std::path::Path::new(env!("CARGO_BIN_EXE_mag82")).file_name().unwrap().to_str().unwrap().to_lowercase();
    assert!(rodar(&["automatico", "abertos"]).1.lines().any(|l| l == eu));
    let texto = rodar(&["automatico", "regra", "3", &eu]).1;
    assert!(texto.contains(&format!("perfil 3: {eu}  (aberto: é a regra em vigor)")), "{texto}");
    assert!(rodar(&["automatico", "regra", "2", "Um Jogo Que Nao Existe.exe"]).1.contains("perfil 2: um jogo que nao existe.exe"));
    // desligado, o vigia não faz nada e sai
    let texto = rodar(&["vigiar", "--uma-vez"]).1;
    assert!(texto.contains("está desligada") && !texto.contains("perfil 3"), "{texto}");
    // ligado (pelo arquivo, para o teste não deixar um vigia rodando): uma conferência troca o perfil
    let arquivo = pasta.join("mag82/perfil-automatico");
    let ajustes = std::fs::read_to_string(&arquivo).unwrap().replace("ligado nao", "ligado sim");
    std::fs::write(&arquivo, ajustes).unwrap();
    let texto = rodar(&["vigiar", "--uma-vez"]).1;
    assert!(texto.contains(&format!("{eu} abriu: perfil 3")), "{texto}");
    // regra para um perfil que o teclado não tem
    rodar(&["automatico", "regra", "9", &eu]);
    assert!(rodar(&["vigiar", "--uma-vez"]).1.contains("não tem o perfil 9"));
    assert!(rodar(&["automatico", "remover", &eu]).0);
    let (certo, texto) = rodar(&["automatico", "remover", &eu]);
    assert!(!certo && texto.contains("não há regra"), "{texto}");
    std::fs::remove_dir_all(&pasta).ok();
}
