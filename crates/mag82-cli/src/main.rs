//! Linha de comando do VOLCANOS MAG82.

use std::collections::BTreeMap;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use mag82::{Atuacao, Avancada, Campo, Erro, ModoSocd, Pos, Resultado, Rgb, Teclado};

#[derive(Parser)]
#[command(name = "mag82", about = "Controle do teclado VOLCANOS MAG82 no Linux", version)]
struct Args {
    /// Mostra os pacotes enviados e recebidos
    #[arg(short, long, global = true)]
    verboso: bool,
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Clone, Copy, ValueEnum)]
enum Chave {
    On,
    Off,
}

#[derive(Clone, Copy, ValueEnum)]
enum Socd {
    /// Vale a última tecla pressionada
    Ultima,
    /// A primeira tecla informada sempre vence
    Primeira,
    /// A segunda tecla informada sempre vence
    Segunda,
    /// Nenhuma das duas é enviada
    Neutro,
}

#[derive(Subcommand)]
enum Comando {
    /// Mostra os perfis, troca o ativo (ex.: perfil 2) ou renomeia (ex.: perfil 2 --nome Jogos)
    Perfil {
        /// Número do perfil, a partir de 1
        numero: Option<u8>,
        /// Novo nome para o perfil informado (não troca o perfil ativo)
        #[arg(long, requires = "numero")]
        nome: Option<String>,
    },
    /// Mostra os pares SOCD ou configura um (ex.: socd a d --modo neutro)
    Socd {
        /// As duas teclas opostas; para remover basta uma
        teclas: Vec<String>,
        #[arg(long, value_enum, default_value = "ultima")]
        modo: Socd,
        /// Remove o SOCD das teclas informadas e de seus pares
        #[arg(long)]
        remover: bool,
    },
    /// Mostra as teclas com DKS ou configura uma (ex.: dks caps --acao lshift:1-3 --acao w:4)
    ///
    /// O DKS aciona até quatro funções em momentos diferentes do curso da tecla:
    /// 1 = ao descer até o início, 2 = ao chegar ao fundo, 3 = ao sair do fundo,
    /// 4 = ao voltar ao início. "1-3" aciona no momento 1 e segura até o 3.
    Dks {
        /// Tecla física que recebe o DKS
        tecla: Option<String>,
        /// Função e momentos, até quatro vezes (ex.: --acao lshift:1-3 --acao w:2,4)
        #[arg(long = "acao", value_name = "FUNÇÃO:MOMENTOS")]
        acoes: Vec<String>,
        /// Ponto de início, em mm (de 0.1 a 3.3)
        #[arg(long, default_value_t = 1.5)]
        inicio: f32,
        /// Ponto do fundo, em mm (de 0.1 a 3.3)
        #[arg(long, default_value_t = 3.0)]
        fundo: f32,
        /// Remove o DKS da tecla
        #[arg(long)]
        remover: bool,
    },
    /// Lista as interfaces USB do teclado
    Info,
    /// Versão de firmware e recursos
    Dispositivo,
    /// Mostra ou altera a taxa de resposta (ex.: taxa 1000)
    Taxa { hz: Option<u32> },
    /// Mostra ou altera a iluminação das teclas ou, com --lateral, da faixa lateral
    Luz {
        /// Age na luz lateral do teclado em vez da luz das teclas
        #[arg(long)]
        lateral: bool,
        #[arg(long, conflicts_with = "desligar")]
        ligar: bool,
        #[arg(long)]
        desligar: bool,
        /// Efeito de luz: de 0 a 20 nas teclas, de 0 a 10 na lateral
        #[arg(long)]
        modo: Option<u8>,
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        brilho: Option<u8>,
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
        velocidade: Option<u8>,
        /// Sentido do efeito (0 ou 1)
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=1))]
        direcao: Option<u8>,
        /// Cor da paleta usada pelo efeito, de 0 a 7
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=7))]
        cor: Option<u8>,
    },
    /// Mostra a paleta ou troca uma das cores (ex.: paleta 1 ff8800)
    Paleta {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=7))]
        indice: Option<u8>,
        #[arg(value_parser = cor_hex)]
        cor: Option<Rgb>,
        /// Age na paleta da luz lateral em vez da paleta das teclas
        #[arg(long)]
        lateral: bool,
    },
    /// Cor fixa por tecla (ex.: cor w a s d --cor ff0000) ou por LED da lateral (ex.: cor --lateral 1 2 3 --cor ff0000)
    Cor {
        /// Nomes das teclas, 'todas' ou linha,coluna; com --lateral, números dos LEDs (a partir de 1) ou 'todos'
        teclas: Vec<String>,
        /// Age nos LEDs da faixa lateral em vez das teclas
        #[arg(long)]
        lateral: bool,
        #[arg(long, value_parser = cor_hex)]
        cor: Option<Rgb>,
        /// A tecla volta a seguir o efeito
        #[arg(long, conflicts_with = "cor")]
        limpar: bool,
    },
    /// Mostra os nomes das teclas, na disposição do teclado
    Teclas,
    /// Mostra ou altera o remapeamento (ex.: remapear caps lctrl)
    Remapear {
        /// Tecla física a alterar
        tecla: Option<String>,
        /// Nova função: nome de tecla ou código 0x....
        nova: Option<String>,
        /// Devolve a tecla à função de fábrica
        #[arg(long, conflicts_with = "nova")]
        padrao: bool,
    },
    /// Mostra os atalhos com a tecla Fn ou muda um (ex.: atalhos pgdn home)
    Atalhos {
        /// Tecla que, com Fn, dispara o atalho
        tecla: Option<String>,
        /// O que ela passa a fazer com Fn: nome de tecla ou código 0x....
        nova: Option<String>,
        /// Devolve o atalho ao que vem de fábrica
        #[arg(long, conflicts_with = "nova")]
        padrao: bool,
    },
    /// Mostra ou altera ponto de atuação e rapid trigger (valores em mm)
    Atuacao {
        /// Nomes das teclas (ex.: w a s d), 'todas' ou linha,coluna
        #[arg(required = true)]
        teclas: Vec<String>,
        /// Ponto de atuação
        #[arg(long)]
        ponto: Option<f64>,
        /// Ponto de soltura (padrão: igual ao ponto)
        #[arg(long)]
        soltar: Option<f64>,
        /// Liga ou desliga o rapid trigger
        #[arg(long)]
        rt: Option<Chave>,
        /// Curso até o rapid trigger começar a valer
        #[arg(long)]
        rt_inicio: Option<f64>,
        /// Sensibilidade do rapid trigger ao pressionar
        #[arg(long)]
        rt_press: Option<f64>,
        /// Sensibilidade do rapid trigger ao soltar
        #[arg(long)]
        rt_solta: Option<f64>,
        /// Zona morta no início do curso
        #[arg(long)]
        zona_topo: Option<f64>,
        /// Zona morta no fim do curso
        #[arg(long)]
        zona_fundo: Option<f64>,
    },
    /// Calibra as teclas: mede de novo o curso de cada uma (pressione todas até o fundo)
    Calibrar {
        /// Só tira o teclado do modo de calibração (use se ele ficou preso nesse modo)
        #[arg(long)]
        encerrar: bool,
    },
    /// Mostra o registro das calibrações (onde fica e o que foi gravado)
    Registro,
    /// Envia um pacote em hex e mostra a resposta (para exploração)
    Bruto { hex: String },
}

/// Ligado pelo Ctrl+C durante `mag82 calibrar`, para o laço sair e encerrar a calibração.
static INTERROMPIDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn ao_interromper(_: libc::c_int) {
    INTERROMPIDO.store(true, std::sync::atomic::Ordering::SeqCst);
}

fn cor_hex(texto: &str) -> Result<Rgb, String> {
    Rgb::de_hex(texto).ok_or_else(|| format!("cor inválida: {texto} (use o formato ff8800)"))
}

fn invalido<T>(mensagem: impl Into<String>) -> Resultado<T> {
    Err(Erro::Invalido(mensagem.into()))
}

/// Nome de cada tecla pelo layout de fábrica, para o nome não mudar depois de remapear.
fn mapa(t: &mut Teclado) -> Resultado<BTreeMap<Pos, String>> {
    Ok(t.layout(true)?.into_iter().filter(|(_, c)| *c != 0).map(|(p, c)| (p, mag82::nome_tecla(c))).collect())
}

/// Converte nomes ('w', 'esc', 'todas', '2,3') em posições.
fn selecionar(mapa: &BTreeMap<Pos, String>, pedidos: &[String]) -> Resultado<Vec<Pos>> {
    let mut posicoes = Vec::new();
    for pedido in pedidos {
        let chave = pedido.to_lowercase();
        if chave == "todas" {
            return Ok(mapa.keys().copied().collect());
        }
        if let Some((pos, _)) = mapa.iter().find(|(_, nome)| **nome == chave) {
            posicoes.push(*pos);
        } else if let Some((l, c)) = chave.split_once(',').and_then(|(l, c)| Some((l.parse().ok()?, c.parse().ok()?))) {
            if !mapa.contains_key(&(l, c)) {
                return invalido(format!("não existe tecla na posição {pedido}"));
            }
            posicoes.push((l, c));
        } else {
            return invalido(format!("tecla desconhecida: {pedido}  (veja os nomes com: mag82 teclas)"));
        }
    }
    Ok(posicoes)
}

fn mm(valor: u16) -> String {
    format!("{:.2}", f64::from(valor) / 1000.0)
}

fn executar(args: Args) -> Resultado<()> {
    if let Comando::Registro = args.comando {
        let caminho = mag82::registro::Registro::caminho_padrao("calibracao.log");
        println!("arquivo: {}", caminho.display());
        match std::fs::read_to_string(&caminho) {
            Ok(texto) if !texto.trim().is_empty() => print!("{texto}"),
            _ => println!("(nenhuma calibração registrada ainda)"),
        }
        return Ok(());
    }
    if let Comando::Info = args.comando {
        let ifs = mag82::interfaces();
        if ifs.is_empty() {
            return Err(Erro::NaoEncontrado);
        }
        for (n, caminho) in ifs {
            let tipo = if n == mag82::CANAL { "comandos" } else { "outra" };
            println!("interface {n}: {}  [{tipo}]", caminho.display());
        }
        return Ok(());
    }

    let mut t = Teclado::abrir()?;
    t.verboso = args.verboso;
    match args.comando {
        Comando::Info | Comando::Registro => unreachable!(),
        Comando::Dispositivo => {
            let d = t.dispositivo()?;
            println!("protocolo:  {}", d.protocolo);
            println!("firmware:   {}", d.firmware);
            println!("pcb:        {}", d.pcb);
            println!("data:       {}", d.data_firmware);
            println!("switch:     {}", d.switches.join(", "));
            println!("conexão:    {}", d.conexoes.join(", "));
            println!("recursos:   {}", d.recursos.join(", "));
        }
        Comando::Taxa { hz } => {
            let (suportadas, _) = t.taxas()?;
            let nomes: Vec<String> = suportadas.iter().map(|c| mag82::nome_taxa(*c)).collect();
            println!("suportadas: {}", nomes.join(", "));
            if let Some(hz) = hz {
                let alvo = format!("{hz} Hz");
                match suportadas.iter().find(|c| mag82::nome_taxa(**c) == alvo) {
                    Some(codigo) => t.definir_taxa(*codigo)?,
                    None => return invalido(format!("taxa não suportada: {alvo}")),
                }
            }
            println!("atual:      {}", mag82::nome_taxa(t.taxas()?.1));
        }
        Comando::Luz { lateral, ligar, desligar, modo, brilho, velocidade, direcao, cor } => {
            let indice = if lateral { mag82::AREA_LATERAL } else { mag82::AREA_TECLAS };
            let Some(area) = t.areas_luz()?.into_iter().find(|a| a.indice == indice) else {
                return invalido("este teclado não tem luz lateral");
            };
            if modo.is_some_and(|m| m >= area.efeitos) {
                return invalido(format!("efeito inexistente: use de 0 a {}", area.efeitos - 1));
            }
            let mut luz = t.luz_de(indice)?;
            let antes = luz;
            if ligar || desligar {
                luz.ligada = ligar;
            }
            luz.modo = modo.unwrap_or(luz.modo);
            luz.brilho = brilho.unwrap_or(luz.brilho);
            luz.velocidade = velocidade.unwrap_or(luz.velocidade);
            luz.invertida = direcao.map_or(luz.invertida, |d| d == 1);
            luz.cor = cor.unwrap_or(luz.cor);
            if luz != antes {
                t.definir_luz_de(indice, area.efeitos, &luz)?;
                luz = t.luz_de(indice)?;
            }
            println!(
                "ligada: {}   modo: {} ({})   brilho: {}   velocidade: {}   direção: {}   cor da paleta: {}",
                if luz.ligada { "sim" } else { "não" },
                luz.modo,
                mag82::nome_efeito_de(indice, area.efeitos, luz.modo),
                luz.brilho,
                luz.velocidade,
                luz.invertida as u8,
                luz.cor
            );
        }
        Comando::Paleta { indice, cor, lateral } => {
            let area = if lateral { mag82::AREA_LATERAL } else { mag82::AREA_TECLAS };
            if lateral && !t.areas_luz()?.iter().any(|a| a.indice == area) {
                return invalido("este teclado não tem luz lateral");
            }
            let mut paleta = t.paleta_de(area)?;
            if let Some(i) = indice {
                let Some(cor) = cor else { return invalido("informe a cor, ex.: mag82 paleta 1 ff8800") };
                paleta.definir(i as usize, cor);
                t.definir_paleta_de(area, &paleta)?;
                paleta = t.paleta_de(area)?;
            }
            for i in 0..8 {
                println!("cor {i}: {}  {}", paleta.cor(i).hex(), paleta.cor(i).nome());
            }
        }
        Comando::Cor { teclas, lateral: true, cor, limpar } => {
            let Some(area) = t.areas_luz()?.into_iter().find(|a| a.indice == mag82::AREA_LATERAL) else {
                return invalido("este teclado não tem luz lateral");
            };
            let mut grade = t.grade_de(&area)?;
            if !teclas.is_empty() {
                if cor.is_none() && !limpar {
                    return invalido("informe --cor ff8800 ou --limpar");
                }
                let mut leds = Vec::new();
                for pedido in &teclas {
                    match pedido.parse::<usize>() {
                        _ if matches!(pedido.to_lowercase().as_str(), "todos" | "todas") => leds.extend(0..grade.leds()),
                        Ok(n) if (1..=grade.leds()).contains(&n) => leds.push(n - 1),
                        _ => return invalido(format!("LED inexistente: {pedido}  (use de 1 a {} ou 'todos')", grade.leds())),
                    }
                }
                for led in leds {
                    grade.definir_led(led, cor);
                }
                t.definir_grade_de(area.indice, &grade)?;
            }
            let fixos: Vec<_> = (0..grade.leds()).filter_map(|i| grade.fixa_do_led(i).map(|c| (i + 1, c))).collect();
            if fixos.is_empty() {
                println!("nenhum LED da lateral com cor fixa (todos seguem o efeito)");
            }
            for (n, c) in fixos {
                println!("LED {n:<6} {}", c.hex());
            }
        }
        Comando::Cor { teclas, lateral: false, cor, limpar } => {
            let mapa = mapa(&mut t)?;
            let mut grade = t.grade()?;
            if !teclas.is_empty() {
                if cor.is_none() && !limpar {
                    return invalido("informe --cor ff8800 ou --limpar");
                }
                for pos in selecionar(&mapa, &teclas)? {
                    grade.definir(pos, cor);
                }
                t.definir_grade(&grade)?;
            }
            let fixas: Vec<_> = mapa.iter().filter_map(|(p, n)| grade.fixa(*p).map(|c| (n, c))).collect();
            if fixas.is_empty() {
                println!("nenhuma tecla com cor fixa (todas seguem o efeito)");
            }
            for (nome, c) in fixas {
                println!("{nome:<10} {}", c.hex());
            }
        }
        Comando::Teclas => {
            let mapa = mapa(&mut t)?;
            for linha in 0..mag82::LINHAS {
                let nomes: Vec<&str> = mapa.iter().filter(|(p, _)| p.0 == linha).map(|(_, n)| n.as_str()).collect();
                println!("{}", nomes.join("  "));
            }
        }
        Comando::Remapear { tecla, nova, padrao } => {
            let mapa = mapa(&mut t)?;
            let fabrica = t.layout(true)?;
            let mut atual = t.layout(false)?;
            if let Some(tecla) = tecla {
                let posicoes = selecionar(&mapa, &[tecla])?;
                let [pos] = posicoes[..] else { return invalido("remapeie uma tecla por vez") };
                let codigo = if padrao {
                    fabrica[&pos]
                } else {
                    let Some(nova) = nova else {
                        return invalido("informe a nova função (ex.: mag82 remapear caps lctrl) ou --padrao");
                    };
                    match mag82::codigo_tecla(&nova) {
                        Some(c) => c,
                        None => return invalido(format!("função desconhecida: {nova}  (veja os nomes com: mag82 teclas)")),
                    }
                };
                atual.insert(pos, t.definir_funcao(pos, codigo)?);
            }
            let mudadas: Vec<_> = mapa.iter().filter(|(p, _)| atual[p] != fabrica[p]).collect();
            if mudadas.is_empty() {
                println!("nenhuma tecla remapeada");
            }
            for (pos, nome) in mudadas {
                println!("{nome:<10} -> {}", mag82::nome_tecla(atual[pos]));
            }
        }
        Comando::Atalhos { tecla, nova, padrao } => {
            let mapa = mapa(&mut t)?;
            let fabrica = t.layout_da_camada(mag82::CAMADA_FN, true)?;
            let mut atual = t.layout_da_camada(mag82::CAMADA_FN, false)?;
            if let Some(tecla) = tecla {
                let posicoes = selecionar(&mapa, &[tecla])?;
                let [pos] = posicoes[..] else { return invalido("mude um atalho por vez") };
                let codigo = if padrao {
                    fabrica[&pos]
                } else {
                    let Some(nova) = nova else {
                        return invalido("informe o que a tecla faz com Fn (ex.: mag82 atalhos pgdn home) ou --padrao");
                    };
                    match mag82::codigo_tecla(&nova) {
                        Some(c) => c,
                        None => return invalido(format!("função desconhecida: {nova}  (use um nome de tecla ou um código 0x....)")),
                    }
                };
                atual.insert(pos, t.definir_funcao_na_camada(mag82::CAMADA_FN, pos, codigo)?);
            }
            for (pos, nome) in &mapa {
                let codigo = atual[pos];
                if codigo == mag82::FUNCAO_PADRAO {
                    continue;
                }
                let descricao = match (mag82::especial(codigo), mag82::combinacao(codigo)) {
                    (Some(e), _) => e.nome.to_string(),
                    (None, Some((modificadores, tecla))) => format!("{} + {}", modificadores.join(" + "), mag82::nome_tecla(tecla)),
                    (None, None) => mag82::nome_tecla(codigo),
                };
                let marca = if fabrica[pos] == codigo { "" } else { "  (alterado)" };
                println!("fn + {nome:<10} {descricao}{marca}");
            }
        }
        Comando::Atuacao { teclas, ponto, soltar, rt, rt_inicio, rt_press, rt_solta, zona_topo, zona_fundo } => {
            let mapa = mapa(&mut t)?;
            let posicoes = selecionar(&mapa, &teclas)?;
            let soltar = soltar.or(ponto); // o teclado mantém os dois iguais por padrão
            let novos = [
                (Campo::Ponto, ponto),
                (Campo::Soltar, soltar),
                (Campo::RtInicio, rt_inicio),
                (Campo::RtPress, rt_press),
                (Campo::RtSolta, rt_solta),
                (Campo::ZonaTopo, zona_topo),
                (Campo::ZonaFundo, zona_fundo),
            ];
            let alterar = rt.is_some() || novos.iter().any(|(_, v)| v.is_some());
            println!("{:<10} {:<14} {:>6} {:>9} {:>9} {:>7} {:>8}", "tecla", "modo", "ponto", "RT press", "RT solta", "z.topo", "z.fundo");
            for pos in posicoes {
                let mut a: Atuacao = t.atuacao(pos)?;
                if alterar {
                    if let Some(chave) = rt {
                        a.definir_rapid_trigger(matches!(chave, Chave::On));
                    }
                    for (campo, valor) in novos {
                        if let Some(v) = valor {
                            if !(0.0..=65.0).contains(&v) {
                                return invalido(format!("{}: valor inválido", campo.titulo()));
                            }
                            a.definir_mm(campo, (v * 1000.0).round() as u16);
                        }
                    }
                    a = t.definir_atuacao(pos, &a)?;
                }
                println!(
                    "{:<10} {:<14} {:>6} {:>9} {:>9} {:>7} {:>8}",
                    mapa[&pos],
                    if a.rapid_trigger() { "rapid trigger" } else { "normal" },
                    mm(a.mm(Campo::Ponto)),
                    mm(a.mm(Campo::RtPress)),
                    mm(a.mm(Campo::RtSolta)),
                    mm(a.mm(Campo::ZonaTopo)),
                    mm(a.mm(Campo::ZonaFundo))
                );
            }
        }
        Comando::Perfil { numero, nome } => {
            let (total, _) = t.perfis()?;
            if let Some(n) = numero {
                if n == 0 || n > total {
                    return invalido(format!("perfil inexistente: use de 1 a {total}"));
                }
                match nome {
                    Some(nome) => t.definir_nome_perfil(n - 1, &nome)?,
                    None => t.definir_perfil(n - 1)?,
                }
            }
            let atual = t.perfis()?.1;
            for i in 0..total {
                let marca = if i == atual { "*" } else { " " };
                println!("{marca} {}: {}", i + 1, mag82::rotulo_perfil(i, &t.nome_perfil(i)?));
            }
        }
        Comando::Socd { teclas, modo, remover } => {
            let mapa = mapa(&mut t)?;
            let posicoes = selecionar(&mapa, &teclas)?;
            if remover {
                if posicoes.is_empty() {
                    return invalido("informe a tecla, ex.: mag82 socd a --remover");
                }
                for pos in posicoes {
                    if let Avancada::Socd { par, .. } = t.avancada(pos)? {
                        t.remover_avancada(par)?;
                    }
                    t.remover_avancada(pos)?;
                }
            } else if !posicoes.is_empty() {
                let [a, b] = posicoes[..] else { return invalido("informe exatamente duas teclas, ex.: mag82 socd a d") };
                let atual = t.layout(false)?;
                let modo = match modo {
                    Socd::Ultima => ModoSocd::UltimaVence,
                    Socd::Primeira => ModoSocd::PrioridadeA,
                    Socd::Segunda => ModoSocd::PrioridadeB,
                    Socd::Neutro => ModoSocd::Neutro,
                };
                t.definir_socd(a, b, atual[&a], atual[&b], modo)?;
            }
            let mut achou = false;
            for (pos, nome) in &mapa {
                if let Avancada::Socd { par, regra, .. } = t.avancada(*pos)? {
                    if *pos < par {
                        achou = true;
                        let outra = mapa.get(&par).map(String::as_str).unwrap_or("?");
                        let texto = match ModoSocd::da_regra(regra) {
                            ModoSocd::UltimaVence => "vale a última pressionada".to_string(),
                            ModoSocd::PrioridadeA => format!("{nome} tem prioridade"),
                            ModoSocd::PrioridadeB => format!("{outra} tem prioridade"),
                            ModoSocd::Neutro => "neutro (nenhuma é enviada)".to_string(),
                        };
                        println!("{nome} + {outra}: {texto}");
                    }
                }
            }
            if !achou {
                println!("nenhum par SOCD configurado");
            }
        }
        Comando::Dks { tecla, acoes, inicio, fundo, remover } => {
            let mapa = mapa(&mut t)?;
            if let Some(tecla) = tecla {
                let posicoes = selecionar(&mapa, &[tecla])?;
                let [pos] = posicoes[..] else { return invalido("o DKS é configurado em uma tecla por vez") };
                if remover {
                    if !matches!(t.avancada(pos)?, Avancada::Dks(_)) {
                        return invalido("essa tecla não tem DKS");
                    }
                    t.remover_avancada(pos)?;
                } else {
                    if acoes.is_empty() || acoes.len() > 4 {
                        return invalido("informe de uma a quatro ações, ex.: mag82 dks caps --acao lshift:1-3 --acao w:4");
                    }
                    let mm = |v: f32| (v * 1000.0).round().clamp(0.0, 65535.0) as u16;
                    let mut dks = mag82::Dks { inicio: mm(inicio), fundo: mm(fundo), ..Default::default() };
                    for (i, acao) in acoes.iter().enumerate() {
                        let Some((funcao, momentos)) = acao.rsplit_once(':') else {
                            return invalido(format!("ação sem momentos: {acao}  (ex.: lshift:1-3)"));
                        };
                        let Some(codigo) = mag82::codigo_tecla(funcao).filter(|c| *c != 0) else {
                            return invalido(format!("função desconhecida: {funcao}  (veja os nomes com: mag82 teclas)"));
                        };
                        let Some(casas) = mag82::Dks::casas_de_texto(momentos) else {
                            return invalido(format!("momentos inválidos: {momentos}  (use 1 a 4, ex.: 1  1-3  2,4)"));
                        };
                        dks.teclas[i] = codigo;
                        dks.definir_casas(i, casas);
                    }
                    t.definir_dks(pos, &dks)?;
                }
            }
            let mut achou = false;
            for (pos, nome) in &mapa {
                if let Avancada::Dks(dks) = t.avancada(*pos)? {
                    achou = true;
                    println!(
                        "{nome}: início {:.2} mm, fundo {:.2} mm",
                        f32::from(dks.inicio) / 1000.0,
                        f32::from(dks.fundo) / 1000.0
                    );
                    for i in (0..4).filter(|i| dks.teclas[*i] != 0) {
                        println!("  {:<10} momentos {}", mag82::nome_tecla(dks.teclas[i]), mag82::Dks::casas_em_texto(dks.casas(i)));
                    }
                }
            }
            if !achou {
                println!("nenhuma tecla com DKS");
            }
        }
        Comando::Calibrar { encerrar: true } => {
            t.encerrar_calibracao()?;
            println!("calibração encerrada");
        }
        Comando::Calibrar { encerrar: false } => {
            let mapa = mapa(&mut t)?;
            println!("O teclado vai entrar em modo de calibração: as teclas acendem em vermelho e NÃO digitam.");
            println!("Pressione cada tecla até o fundo, uma vez; ela fica verde quando é calibrada.");
            println!("A calibração termina sozinha quando todas estiverem calibradas, ou depois de 20 s sem");
            println!("nenhuma tecla nova. Ctrl+C também encerra. Se ficar presa: mag82 calibrar --encerrar");
            let firmware = t.dispositivo()?.firmware;
            let ler = |t: &mut Teclado| -> Resultado<Vec<(Pos, bool)>> {
                mapa.keys().map(|pos| Ok((*pos, t.atuacao(*pos)?.calibrada()))).collect()
            };
            // SAFETY: o tratador só grava num booleano atômico.
            unsafe {
                libc::signal(libc::SIGINT, ao_interromper as extern "C" fn(libc::c_int) as libc::sighandler_t);
                libc::signal(libc::SIGTERM, ao_interromper as extern "C" fn(libc::c_int) as libc::sighandler_t);
            }
            t.iniciar_calibracao()?;
            let caminho = mag82::registro::Registro::caminho_padrao("calibracao.log");
            let mut sessao = None;
            let resultado = (|| -> Resultado<()> {
                let s = sessao.insert(mag82::registro::SessaoCalibracao::nova(
                    &caminho,
                    "linha de comando",
                    &firmware,
                    mapa.clone(),
                    &ler(&mut t)?,
                ));
                let (mut feitas, mut parado) = (usize::MAX, std::time::Instant::now());
                loop {
                    let (agora, total) = s.contagem();
                    if agora != feitas {
                        (feitas, parado) = (agora, std::time::Instant::now());
                        println!("{agora} de {total} teclas calibradas");
                    }
                    let interrompido = INTERROMPIDO.load(std::sync::atomic::Ordering::SeqCst);
                    if agora == total || interrompido || parado.elapsed() > std::time::Duration::from_secs(20) {
                        return Ok(());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    s.atualizar(&ler(&mut t)?);
                }
            })();
            // sai do modo de calibração mesmo se a leitura falhou no meio
            let encerrado = t.encerrar_calibracao();
            if let Some(mut s) = sessao {
                if let Err(e) = resultado.as_ref().and(encerrado.as_ref()) {
                    s.erro(&e.to_string());
                }
                let depois = ler(&mut t).ok();
                let resumo = s.encerrar(depois.as_deref());
                println!("calibração encerrada: {}", resumo.texto());
                if let Some(arquivo) = resumo.registro {
                    println!("registro: {}", arquivo.display());
                }
            }
            encerrado?;
            resultado?;
        }
        Comando::Bruto { hex } => {
            let limpo: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
            if limpo.is_empty() || limpo.len() % 2 != 0 || !limpo.is_ascii() {
                return invalido("informe os bytes em hex, ex.: 0101");
            }
            let dados: Option<Vec<u8>> =
                (0..limpo.len()).step_by(2).map(|i| u8::from_str_radix(&limpo[i..i + 2], 16).ok()).collect();
            let Some(dados) = dados else { return invalido("hex inválido") };
            println!("{}", mag82::hexa(&t.comando(&dados)?));
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    // `mag82 ... | head`: encerra em silêncio quando o leitor fecha, em vez de entrar em pânico
    // SAFETY: troca só o tratamento de SIGPIPE, antes de qualquer outra thread existir.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    match executar(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
