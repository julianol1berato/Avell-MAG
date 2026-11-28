//! Teclado simulado: responde aos mesmos pedidos que o teclado de verdade, a partir de um estado
//! inicial guardado em `simulado.txt`, e guarda o que for gravado enquanto o programa roda.
//!
//! Serve para testar a biblioteca, a linha de comando e a interface sem o teclado, e para quem
//! quer conhecer o programa antes de ter um. Liga com a variável `MAG82_SIMULADO`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::{grupo, COLUNAS, LINHAS, TAM};

/// Variável de ambiente que troca o teclado de verdade pelo simulado.
pub const VARIAVEL: &str = "MAG82_SIMULADO";

/// Verdadeiro se o programa deve usar o teclado simulado.
pub fn ligado() -> bool {
    std::env::var_os(VARIAVEL).is_some_and(|v| !v.is_empty() && v != "0")
}

/// Estado do teclado simulado: a resposta guardada para cada pedido de leitura.
pub struct Simulado {
    respostas: HashMap<Vec<u8>, [u8; TAM]>,
    calibrando: bool,
    /// Macros: o cabeçalho (ativa, total de ações, repetições, modo) e as ações de cada uma.
    macros: Vec<([u8; 6], Vec<u8>)>,
    /// Leituras de curso já atendidas: o simulado "aperta" a tecla W devagar, de novo e de novo.
    quadros: u32,
    /// Leituras de atuação desde que a calibração abriu: cada tecla "é pressionada" na segunda
    /// vez em que é lida, para a calibração andar sozinha.
    leituras: HashMap<(u8, u8), u8>,
    /// O que cada perfil que não está ativo guarda: as respostas que são do perfil e as macros.
    /// Um perfil que nunca foi ativado ainda não está aqui e começa como o de fábrica.
    perfis: HashMap<u8, DoPerfil>,
}

/// A parte do estado que pertence a um perfil.
type DoPerfil = (HashMap<Vec<u8>, [u8; TAM]>, Vec<([u8; 6], Vec<u8>)>);

/// Verdadeiro se a resposta a este pedido muda de um perfil para outro: funções das teclas,
/// atuação, luzes e teclas avançadas. O resto (perfis, taxa, dados do aparelho) é do teclado.
fn do_perfil(pedido: &[u8]) -> bool {
    matches!(pedido.first(), Some(&grupo::LAYOUT | &grupo::PERFORMANCE | &grupo::LIGHTING | &grupo::TECLA_AVANCADA))
}

impl Default for Simulado {
    fn default() -> Self {
        Self::novo()
    }
}

impl Simulado {
    pub fn novo() -> Self {
        let mut respostas = HashMap::new();
        for linha in include_str!("simulado.txt").lines() {
            let Some((pedido, resposta)) = linha.split_once(' ') else { continue };
            let bytes = |texto: &str| -> Vec<u8> {
                (0..texto.len() / 2).filter_map(|i| u8::from_str_radix(&texto[2 * i..2 * i + 2], 16).ok()).collect()
            };
            let mut pacote = [0u8; TAM];
            let lidos = bytes(resposta);
            pacote[..lidos.len().min(TAM)].copy_from_slice(&lidos[..lidos.len().min(TAM)]);
            respostas.insert(bytes(pedido), pacote);
        }
        let macros = vec![([0; 6], Vec::new()); crate::MACROS_MAX as usize];
        Self { respostas, calibrando: false, macros, quadros: 0, leituras: HashMap::new(), perfis: HashMap::new() }
    }

    /// O estado compartilhado por todas as conexões simuladas deste processo, como um teclado só.
    pub fn compartilhado() -> &'static Mutex<Simulado> {
        static UNICO: OnceLock<Mutex<Simulado>> = OnceLock::new();
        UNICO.get_or_init(|| Mutex::new(Simulado::novo()))
    }

    /// Troca o perfil ativo: guarda o que é do perfil atual e traz o que o outro tinha. No
    /// teclado de verdade cada perfil tem as suas teclas, luzes e macros (conferido no MAG82).
    fn trocar_perfil(&mut self, novo: u8) {
        let Some(atual) = self.respostas.get(&[grupo::GLOBAL, 3, 1][..]).map(|r| r[3]) else { return };
        if novo == atual {
            return;
        }
        let (mut respostas, macros) = match self.perfis.remove(&novo) {
            Some(guardado) => guardado,
            None => {
                let fabrica = Self::novo();
                (fabrica.respostas.into_iter().filter(|(pedido, _)| do_perfil(pedido)).collect(), fabrica.macros)
            }
        };
        // a marca de calibração de cada tecla é do teclado, não do perfil
        for (pedido, resposta) in respostas.iter_mut() {
            if let [grupo::PERFORMANCE, 1, ..] = pedido[..] {
                if let Some(agora) = self.respostas.get(pedido) {
                    resposta[4 + 16] = agora[4 + 16];
                }
            }
        }
        let mut deste: HashMap<Vec<u8>, [u8; TAM]> = HashMap::new();
        for (pedido, resposta) in respostas {
            if let Some(antiga) = self.respostas.insert(pedido.clone(), resposta) {
                deste.insert(pedido, antiga);
            }
        }
        let macros_deste = std::mem::replace(&mut self.macros, macros);
        self.perfis.insert(atual, (deste, macros_deste));
    }

    fn guardada(&mut self, pedido: &[u8]) -> Option<&mut [u8; TAM]> {
        self.respostas.get_mut(pedido)
    }

    /// Quantos efeitos tem a área de luz, como informado em `[2, 8, 0]`.
    fn efeitos(&self, area: u8) -> u8 {
        let Some(r) = self.respostas.get(&[grupo::GLOBAL, 8, 0][..]) else { return 0 };
        (0..r[3] as usize).map(|i| &r[4 + 4 * i..8 + 4 * i]).find(|a| a[0] == area).map_or(0, |a| a[1])
    }

    /// Atende um pedido. `None` é o teclado sem resposta (pedido de leitura que ele não conhece).
    pub fn responder(&mut self, dados: &[u8]) -> Option<[u8; TAM]> {
        let mut eco = [0u8; TAM];
        eco[..dados.len().min(TAM)].copy_from_slice(&dados[..dados.len().min(TAM)]);
        match *dados {
            // ----- gravações: mudam o que as leituras seguintes devolvem -----
            [grupo::GLOBAL, 3, 2, perfil, ..] => {
                self.trocar_perfil(perfil);
                self.guardada(&[grupo::GLOBAL, 3, 1])?[3] = perfil;
            }
            [grupo::GLOBAL, 3, 4, perfil, ref nome @ ..] => {
                let guardado = self.guardada(&[grupo::GLOBAL, 3, 3, perfil])?;
                guardado[4..36].fill(0);
                let n = nome.len().min(32);
                guardado[4..4 + n].copy_from_slice(&nome[..n]);
            }
            [grupo::GLOBAL, 5, 2, taxa, ..] => self.guardada(&[grupo::GLOBAL, 5, 1])?[3] = taxa,
            [grupo::GLOBAL, 6, 0, ..] => {
                self.calibrando = true;
                self.leituras.clear();
                for linha in 0..LINHAS {
                    for coluna in 0..COLUNAS {
                        if let Some(r) = self.guardada(&[grupo::PERFORMANCE, 1, linha, coluna]) {
                            r[4 + 16] = 0;
                        }
                    }
                }
            }
            [grupo::GLOBAL, 6, 1, ..] => self.calibrando = false,
            [grupo::LAYOUT, 4, camada, linha, coluna, baixo, alto, ..] => {
                let tecla = self.guardada(&[grupo::LAYOUT, 3, camada, linha, coluna]);
                if let Some(tecla) = tecla {
                    tecla[5..7].copy_from_slice(&[baixo, alto]);
                }
                let i = 4 + 2 * coluna as usize;
                self.guardada(&[grupo::LAYOUT, 1, camada, linha])?.get_mut(i..i + 2)?.copy_from_slice(&[baixo, alto]);
            }
            [grupo::PERFORMANCE, 2, linha, coluna, ref corpo @ ..] if corpo.len() >= 23 => {
                let guardado = self.guardada(&[grupo::PERFORMANCE, 1, linha, coluna])?;
                let calibrada = guardado[4 + 16];
                guardado[4..27].copy_from_slice(&corpo[..23]);
                guardado[4 + 16] = calibrada; // a marca de calibração é do teclado
            }
            [grupo::LIGHTING, 2, area, 0, ref ajustes @ ..] if ajustes.len() >= 6 => {
                self.guardada(&[grupo::LIGHTING, 1, area, 0])?[4..10].copy_from_slice(&ajustes[..6]);
            }
            [grupo::LIGHTING, 2, area, 1, ref cores @ ..] if cores.len() >= 32 => {
                self.guardada(&[grupo::LIGHTING, 1, area, 1])?[4..36].copy_from_slice(&cores[..32]);
            }
            [grupo::LIGHTING, 4, area, pagina, ref cores @ ..] => {
                // no modo "só as cores fixas" (o último da área) o pedido de soltar uma cor é ignorado
                let modo = self.respostas.get(&[grupo::LIGHTING, 1, area, 0][..]).map_or(0, |r| r[5]);
                let so_fixas = self.efeitos(area) > 0 && modo + 1 == self.efeitos(area);
                let guardado = self.guardada(&[grupo::LIGHTING, 3, area, pagina])?;
                for (antes, depois) in guardado[4..].chunks_mut(4).zip(cores.chunks(4)) {
                    if depois.len() < 4 {
                        break;
                    }
                    if depois[3] == 255 {
                        antes.copy_from_slice(depois);
                    } else if !so_fixas {
                        antes[3] = 0; // volta ao efeito; a cor do momento fica como estava
                    }
                }
            }
            // como o teclado de verdade: função que não é tecla comum é recusada, com 0xFF no
            // lugar do tipo, e a tecla fica como estava
            [grupo::TECLA_AVANCADA, 2, linha, coluna, ref resto @ ..] if !funcoes_servem(resto) => {
                let mut recusa = [0u8; TAM];
                recusa[..5].copy_from_slice(&[grupo::TECLA_AVANCADA, 2, linha, coluna, 0xFF]);
                return Some(recusa);
            }
            [grupo::TECLA_AVANCADA, 2, linha, coluna, ref resto @ ..] => {
                let guardado = self.guardada(&[grupo::TECLA_AVANCADA, 1, linha, coluna])?;
                guardado[4..].fill(0);
                let n = resto.len().min(TAM - 4);
                guardado[4..4 + n].copy_from_slice(&resto[..n]);
            }

            // ----- macros -----
            [grupo::GLOBAL, 14, 0, ..] => {
                // como no teclado de verdade: 8 macros e 224 ações ao todo
                eco[3] = self.macros.len() as u8;
                eco[4..6].copy_from_slice(&224u16.to_le_bytes());
            }
            [grupo::MACRO, 2, indice, ref cabecalho @ ..] if cabecalho.len() >= 6 => {
                let (guardado, acoes) = self.macros.get_mut(indice as usize)?;
                // a vaga apagada fica toda zerada, como no teclado
                *guardado = if cabecalho[0] == 0 { [0; 6] } else { [cabecalho[0], cabecalho[1], cabecalho[2], cabecalho[3], cabecalho[4], cabecalho[5]] };
                acoes.resize(4 * u16::from_le_bytes([cabecalho[1], cabecalho[2]]) as usize, 0);
            }
            [grupo::MACRO, 4, indice, pagina, ref dados @ ..] => {
                let (_, acoes) = self.macros.get_mut(indice as usize)?;
                let inicio = 60 * pagina as usize;
                for (i, byte) in dados.iter().enumerate() {
                    if let Some(vaga) = acoes.get_mut(inicio + i) {
                        *vaga = *byte;
                    }
                }
            }
            [grupo::MACRO, 1, indice, ..] => eco[3..9].copy_from_slice(&self.macros.get(indice as usize)?.0),
            [grupo::MACRO, 3, indice, pagina, ..] => {
                let (_, acoes) = self.macros.get(indice as usize)?;
                let inicio = (60 * pagina as usize).min(acoes.len());
                let fim = (inicio + 60).min(acoes.len());
                eco[4..4 + fim - inicio].copy_from_slice(&acoes[inicio..fim]);
            }

            // ----- curso ao vivo: tudo solto, menos a tecla W, que desce e sobe -----
            [grupo::PERFORMANCE, 3, 1, linha, ..] => {
                if linha == 2 {
                    self.quadros = self.quadros.wrapping_add(1);
                    let fase = (self.quadros % 40) as i32;
                    let curso = (3400 - (fase - 20).abs() * 170).clamp(0, 3400) as u16;
                    eco[4 + 2 * 2..6 + 2 * 2].copy_from_slice(&curso.to_le_bytes());
                }
            }

            // ----- leituras -----
            [grupo::PERFORMANCE, 1, linha, coluna, ..] if self.calibrando => {
                let vezes = self.leituras.entry((linha, coluna)).or_insert(0);
                *vezes = vezes.saturating_add(1);
                let pressionada = *vezes >= 2;
                let guardado = self.guardada(&[grupo::PERFORMANCE, 1, linha, coluna])?;
                if pressionada {
                    guardado[4 + 16] = 1;
                }
                return Some(*guardado);
            }
            [grupo::LAYOUT, 3, camada, linha, coluna, ..] => {
                return self.respostas.get(&[grupo::LAYOUT, 3, camada, linha, coluna][..]).copied().or_else(|| {
                    // posição que não veio no estado inicial: monta a resposta a partir da linha
                    let da_linha = self.respostas.get(&[grupo::LAYOUT, 1, camada, linha][..])?;
                    let i = 4 + 2 * coluna as usize;
                    let mut r = eco;
                    r[5..7].copy_from_slice(da_linha.get(i..i + 2)?);
                    Some(r)
                });
            }
            _ => return self.respostas.get(dados).copied(),
        }
        Some(eco)
    }
}

/// Confere as funções de uma gravação de tecla avançada (`corpo` começa no tipo), como o
/// teclado faz (ver [`crate::teclado_aceita_em_avancada`]): ele recusa as funções especiais e
/// aceita as teclas com modificadores, mesmo sem enviá-los depois.
fn funcoes_servem(corpo: &[u8]) -> bool {
    // onde ficam as funções, em cada tipo: DKS, multiponto, dupla função, contínuo, ao soltar e,
    // nos pares (SOCD e RS), depois da posição da outra tecla
    let (inicio, quantas) = match corpo.first() {
        Some(1) => (1, 4),
        Some(2) => (1, 3),
        Some(3) | Some(5) => (1, 2),
        Some(4) => (1, 1),
        Some(6) | Some(7) => (3, 2),
        _ => return true,
    };
    (0..quantas).all(|i| {
        let em = inicio + 2 * i;
        corpo.get(em..em + 2).map_or(true, |c| crate::teclado_aceita_em_avancada(corpo[0], u16::from_le_bytes([c[0], c[1]])))
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_o_estado_inicial() {
        let mut s = Simulado::novo();
        assert_eq!(s.responder(&[grupo::DEVICE, 1]).unwrap()[..4], [1, 1, 1, 2]);
        assert!(s.responder(&[grupo::LAYOUT, 1, 0, 0]).is_some());
        assert!(s.responder(&[grupo::PERFORMANCE, 1, 5, 14]).is_some());
        assert!(s.responder(&[9, 9]).is_none(), "leitura desconhecida fica sem resposta");
    }

    #[test]
    fn cada_perfil_tem_a_sua_configuracao() {
        let mut s = Simulado::novo();
        let funcao = |s: &mut Simulado| s.responder(&[grupo::LAYOUT, 3, 0, 2, 2]).unwrap()[5];
        let brilho = |s: &mut Simulado| s.responder(&[grupo::LIGHTING, 1, 0, 0]).unwrap()[4..10].to_vec();
        let (funcao0, luz0) = (funcao(&mut s), brilho(&mut s));
        // muda uma tecla, a luz e a taxa no perfil 1
        s.responder(&[grupo::LAYOUT, 4, 0, 2, 2, 0x0e, 0]).unwrap();
        s.responder(&[grupo::LIGHTING, 2, 0, 0, 1, 2, 33, 44, 0, 5]).unwrap();
        s.responder(&[grupo::GLOBAL, 5, 2, 3]).unwrap();
        let luz1 = brilho(&mut s);
        assert_ne!(luz1, luz0);
        // o perfil 2 começa de fábrica; a taxa é do teclado e continua
        s.responder(&[grupo::GLOBAL, 3, 2, 1]).unwrap();
        assert_eq!(s.responder(&[grupo::GLOBAL, 3, 1]).unwrap()[3], 1);
        assert_eq!((funcao(&mut s), brilho(&mut s)), (funcao0, luz0.clone()));
        assert_eq!(s.responder(&[grupo::GLOBAL, 5, 1]).unwrap()[3], 3);
        s.responder(&[grupo::LAYOUT, 4, 0, 2, 2, 0x0f, 0]).unwrap();
        // de volta ao 1, está como foi deixado; e o 2 guarda o que recebeu
        s.responder(&[grupo::GLOBAL, 3, 2, 0]).unwrap();
        assert_eq!((funcao(&mut s), brilho(&mut s)), (0x0e, luz1));
        s.responder(&[grupo::GLOBAL, 3, 2, 1]).unwrap();
        assert_eq!(funcao(&mut s), 0x0f);
        // pedir o perfil que já está ativo não muda nada
        s.responder(&[grupo::GLOBAL, 3, 2, 1]).unwrap();
        assert_eq!(funcao(&mut s), 0x0f);
    }

    #[test]
    fn gravacao_aparece_na_leitura() {
        let mut s = Simulado::novo();
        s.responder(&[grupo::GLOBAL, 5, 2, 3]).unwrap();
        assert_eq!(s.responder(&[grupo::GLOBAL, 5, 1]).unwrap()[3], 3);
        s.responder(&[grupo::LAYOUT, 4, 0, 2, 2, 0x0e, 0]).unwrap();
        assert_eq!(s.responder(&[grupo::LAYOUT, 3, 0, 2, 2]).unwrap()[5], 0x0e);
        assert_eq!(s.responder(&[grupo::LAYOUT, 1, 0, 2]).unwrap()[4 + 4], 0x0e);
    }

    #[test]
    fn avancada_com_funcao_especial_e_recusada() {
        let mut s = Simulado::novo();
        // multiponto na tecla Insert com "aumentar o volume" (0x20E9): recusado, nada gravado
        let recusa = s.responder(&[grupo::TECLA_AVANCADA, 2, 1, 14, 2, 0xE9, 0x20, 0, 0, 0, 0, 0xF4, 1, 0xDC, 5, 0xC4, 9]).unwrap();
        assert_eq!(recusa[..6], [grupo::TECLA_AVANCADA, 2, 1, 14, 0xFF, 0]);
        assert_eq!(s.responder(&[grupo::TECLA_AVANCADA, 1, 1, 14]).unwrap()[4], 0);
        // com teclas comuns é aceito e aparece na leitura
        let aceito = s.responder(&[grupo::TECLA_AVANCADA, 2, 1, 14, 2, 0x1A, 0, 0xE1, 0, 0, 0, 0xF4, 1, 0xDC, 5, 0xC4, 9]).unwrap();
        assert_eq!(aceito[4..8], [2, 0x1A, 0, 0xE1]);
        assert_eq!(s.responder(&[grupo::TECLA_AVANCADA, 1, 1, 14]).unwrap()[4..8], [2, 0x1A, 0, 0xE1]);
        // par SOCD em que uma das teclas envia uma função de luz (0xF301)
        let par = s.responder(&[grupo::TECLA_AVANCADA, 2, 3, 1, 6, 3, 3, 0x01, 0xF3, 0x07, 0, 0, 0, 0]).unwrap();
        assert_eq!(par[4], 0xFF);
    }
}
