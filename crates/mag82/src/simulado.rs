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
    /// Leituras de atuação desde que a calibração abriu: cada tecla "é pressionada" na segunda
    /// vez em que é lida, para a calibração andar sozinha.
    leituras: HashMap<(u8, u8), u8>,
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
        let macros = vec![([0, 0, 0, 1, 0, 0], Vec::new()); crate::MACROS_MAX as usize];
        Self { respostas, calibrando: false, macros, leituras: HashMap::new() }
    }

    /// O estado compartilhado por todas as conexões simuladas deste processo, como um teclado só.
    pub fn compartilhado() -> &'static Mutex<Simulado> {
        static UNICO: OnceLock<Mutex<Simulado>> = OnceLock::new();
        UNICO.get_or_init(|| Mutex::new(Simulado::novo()))
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
            [grupo::GLOBAL, 3, 2, perfil, ..] => self.guardada(&[grupo::GLOBAL, 3, 1])?[3] = perfil,
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
            [grupo::TECLA_AVANCADA, 2, linha, coluna, ref resto @ ..] => {
                let guardado = self.guardada(&[grupo::TECLA_AVANCADA, 1, linha, coluna])?;
                guardado[4..].fill(0);
                let n = resto.len().min(TAM - 4);
                guardado[4..4 + n].copy_from_slice(&resto[..n]);
            }

            // ----- macros -----
            [grupo::GLOBAL, 14, 0, ..] => {
                // 16 macros; o total de ações que cabem é uma suposição do simulado
                eco[3] = self.macros.len() as u8;
                eco[4..6].copy_from_slice(&1024u16.to_le_bytes());
            }
            [grupo::MACRO, 2, indice, ref cabecalho @ ..] if cabecalho.len() >= 6 => {
                let (guardado, acoes) = self.macros.get_mut(indice as usize)?;
                guardado.copy_from_slice(&cabecalho[..6]);
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
    fn gravacao_aparece_na_leitura() {
        let mut s = Simulado::novo();
        s.responder(&[grupo::GLOBAL, 5, 2, 3]).unwrap();
        assert_eq!(s.responder(&[grupo::GLOBAL, 5, 1]).unwrap()[3], 3);
        s.responder(&[grupo::LAYOUT, 4, 0, 2, 2, 0x0e, 0]).unwrap();
        assert_eq!(s.responder(&[grupo::LAYOUT, 3, 0, 2, 2]).unwrap()[5], 0x0e);
        assert_eq!(s.responder(&[grupo::LAYOUT, 1, 0, 2]).unwrap()[4 + 4], 0x0e);
    }
}
