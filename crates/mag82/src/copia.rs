//! Cópia da configuração do perfil ativo num arquivo de texto: exportar, comparar e importar.
//!
//! O arquivo é dividido em seções (atuação, funções, teclas avançadas, macros e luzes). Ao
//! importar, cada seção presente passa a valer por inteiro e uma seção ausente deixa aquela
//! parte do teclado como está. Só o que for diferente é gravado, e no fim o teclado é lido de
//! novo para conferir que ficou igual ao arquivo.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::{
    codigo_tecla, combinacao, especial, funcoes, macro_do_codigo, nome_tecla, serve_em_avancada, AreaLuz, Atuacao,
    Avancada, Campo, Dks, End, Erro, Familia, Grade, Luz, Macro, ModoMacro, ModoSocd, Mpt, Mt, Paleta, Pos, Resultado,
    Rgb, Teclado, Tgl, AREA_LATERAL, AREA_TECLAS, CAMADAS, COLUNAS, EFEITOS, FUNCAO_PADRAO, LINHAS, MACROS_MAX,
};

/// Extensão sugerida para o arquivo (o conteúdo é texto simples).
pub const EXTENSAO: &str = "mag82";
/// Primeira linha útil do arquivo, seguida do número do formato.
const MARCA: &str = "mag82-configuracao";
const FORMATO: u32 = 1;
/// Nomes dos modos de macro no arquivo, na ordem de [`ModoMacro::TODOS`].
const MODOS_DE_MACRO: [&str; 6] = ["toque", "toque-recomeca", "toque-para", "toque-termina", "segurar", "segurar-termina"];
/// Nomes dos modos de SOCD no arquivo, na ordem de [`ModoSocd::TODOS`].
const MODOS_DE_SOCD: [&str; 4] = ["ultima", "primeira", "segunda", "neutro"];

/// Atuação de uma tecla como vai no arquivo: os valores de [`Campo::TODOS`], em milésimos de mm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtuacaoCopia {
    pub rapid_trigger: bool,
    pub mm: [u16; 7],
}

impl AtuacaoCopia {
    fn de(a: &Atuacao) -> Self {
        Self { rapid_trigger: a.rapid_trigger(), mm: Campo::TODOS.map(|c| a.mm(c)) }
    }

    /// A atuação `base` (lida do teclado) com os valores desta cópia.
    fn sobre(&self, base: &Atuacao) -> Atuacao {
        let mut a = *base;
        a.definir_rapid_trigger(self.rapid_trigger);
        for (campo, valor) in Campo::TODOS.into_iter().zip(self.mm) {
            a.definir_mm(campo, valor);
        }
        a
    }
}

/// Função avançada de uma tecla como vai no arquivo. Os pares não levam as funções das duas
/// teclas: elas vêm do que as teclas enviam na camada principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvancadaCopia {
    /// `regra` é a visão desta tecla, como em [`Avancada::Socd`].
    Socd { par: Pos, regra: u8 },
    Rs { par: Pos, atraso: u16 },
    Dks(Dks),
    Mpt(Mpt),
    Mt(Mt),
    Tgl(Tgl),
    End(End),
    /// Tipo que o programa não conhece: só é aceito de volta se o teclado já estiver assim.
    Outra(u8),
}

/// Luz de uma área: o efeito, as cores 1 a 7 da paleta e as cores fixas. `K` é a posição da
/// tecla, na luz das teclas, ou o número do LED a partir de 0, na lateral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuzCopia<K: Ord> {
    pub luz: Luz,
    pub paleta: [Rgb; 7],
    pub fixas: BTreeMap<K, Rgb>,
}

/// A configuração de um perfil. Cada seção é opcional (ver a documentação do módulo).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Copia {
    /// De onde veio, para o cabeçalho do arquivo ("Perfil 1, firmware 0.0.2.0").
    pub origem: String,
    /// Nome de fábrica de cada tecla, para os comentários do arquivo.
    pub nomes: BTreeMap<Pos, String>,
    pub atuacao: Option<BTreeMap<Pos, AtuacaoCopia>>,
    /// (camada, tecla) → função, só onde é diferente do padrão de fábrica.
    pub funcoes: Option<BTreeMap<(u8, Pos), u16>>,
    pub avancadas: Option<BTreeMap<Pos, AvancadaCopia>>,
    /// Macros gravadas, pelo índice a partir de 0; as vagas vazias ficam de fora.
    pub macros: Option<BTreeMap<u8, Macro>>,
    pub luz_teclas: Option<LuzCopia<Pos>>,
    pub luz_lateral: Option<LuzCopia<usize>>,
}

/// O que uma importação muda (ou mudou) no teclado.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resumo {
    pub atuacao: usize,
    pub funcoes: usize,
    pub avancadas: usize,
    pub macros: usize,
    pub luz_teclas: bool,
    pub cores_teclas: usize,
    pub luz_lateral: bool,
    pub cores_lateral: usize,
}

impl Resumo {
    pub fn vazio(&self) -> bool {
        *self == Self::default()
    }

    /// Uma frase curta para cada parte que muda: "atuação de 12 teclas", "2 macros".
    pub fn frases(&self) -> Vec<String> {
        let teclas = |n: usize| if n == 1 { "1 tecla".to_string() } else { format!("{n} teclas") };
        let mut f = Vec::new();
        if self.atuacao > 0 {
            f.push(format!("atuação de {}", teclas(self.atuacao)));
        }
        if self.funcoes > 0 {
            f.push(format!("função de {}", teclas(self.funcoes)));
        }
        if self.avancadas > 0 {
            f.push(format!("função avançada de {}", teclas(self.avancadas)));
        }
        if self.macros > 0 {
            f.push(if self.macros == 1 { "1 macro".into() } else { format!("{} macros", self.macros) });
        }
        if self.luz_teclas {
            f.push("luz das teclas (efeito, brilho ou paleta)".into());
        }
        if self.cores_teclas > 0 {
            f.push(format!("cor fixa de {}", teclas(self.cores_teclas)));
        }
        if self.luz_lateral {
            f.push("luz lateral (efeito, brilho ou paleta)".into());
        }
        if self.cores_lateral > 0 {
            f.push(if self.cores_lateral == 1 {
                "cor fixa de 1 LED da lateral".into()
            } else {
                format!("cor fixa de {} LEDs da lateral", self.cores_lateral)
            });
        }
        f
    }
}

// ---------- o que está no teclado ----------

struct LuzLida {
    area: AreaLuz,
    luz: Luz,
    paleta: Paleta,
    grade: Grade,
}

/// Tudo o que a cópia cobre, como está no teclado agora.
struct Estado {
    /// As teclas de verdade: posições que de fábrica enviam alguma coisa na camada principal.
    nomes: BTreeMap<Pos, String>,
    fabrica: Vec<BTreeMap<Pos, u16>>,
    atual: Vec<BTreeMap<Pos, u16>>,
    atuacao: BTreeMap<Pos, Atuacao>,
    avancada: BTreeMap<Pos, Avancada>,
    macros: Option<Vec<Macro>>,
    teclas: LuzLida,
    lateral: Option<LuzLida>,
}

impl Estado {
    fn ler(t: &mut Teclado) -> Resultado<Self> {
        let mut fabrica = Vec::new();
        let mut atual = Vec::new();
        for camada in 0..CAMADAS {
            fabrica.push(t.layout_da_camada(camada, true)?);
            atual.push(t.layout_da_camada(camada, false)?);
        }
        let nomes: BTreeMap<Pos, String> = fabrica[0].iter().filter(|(_, c)| **c != 0).map(|(p, c)| (*p, nome_tecla(*c))).collect();
        let mut atuacao = BTreeMap::new();
        let mut avancada = BTreeMap::new();
        for pos in nomes.keys() {
            atuacao.insert(*pos, t.atuacao(*pos)?);
            avancada.insert(*pos, t.avancada(*pos)?);
        }
        let macros = match t.espaco_de_macros() {
            Ok((total, _)) => Some((0..total.min(MACROS_MAX)).map(|i| t.macro_gravada(i)).collect::<Resultado<Vec<_>>>()?),
            Err(_) => None,
        };
        let areas = t.areas_luz()?;
        let area_teclas = areas.iter().find(|a| a.indice == AREA_TECLAS).copied().unwrap_or(AreaLuz {
            indice: AREA_TECLAS,
            efeitos: EFEITOS,
            linhas: LINHAS,
            colunas: COLUNAS,
        });
        let teclas = LuzLida { area: area_teclas, luz: t.luz()?, paleta: t.paleta()?, grade: t.grade()? };
        let lateral = match areas.iter().find(|a| a.indice == AREA_LATERAL) {
            Some(area) => Some(LuzLida {
                area: *area,
                luz: t.luz_de(area.indice)?,
                paleta: t.paleta_de(area.indice)?,
                grade: t.grade_de(area)?,
            }),
            None => None,
        };
        Ok(Self { nomes, fabrica, atual, atuacao, avancada, macros, teclas, lateral })
    }

    fn copia(&self, origem: String) -> Copia {
        let mut funcoes = BTreeMap::new();
        for camada in 0..CAMADAS {
            for (pos, codigo) in &self.atual[camada as usize] {
                if self.fabrica[camada as usize].get(pos) != Some(codigo) {
                    funcoes.insert((camada, *pos), *codigo);
                }
            }
        }
        let avancadas = self
            .avancada
            .iter()
            .filter_map(|(pos, a)| {
                let copia = match *a {
                    Avancada::Nenhuma => return None,
                    Avancada::Socd { par, regra, .. } => AvancadaCopia::Socd { par, regra },
                    Avancada::Rs { par, atraso, .. } => AvancadaCopia::Rs { par, atraso },
                    Avancada::Dks(d) => AvancadaCopia::Dks(d),
                    Avancada::Mpt(m) => AvancadaCopia::Mpt(m),
                    Avancada::Mt(m) => AvancadaCopia::Mt(m),
                    Avancada::Tgl(g) => AvancadaCopia::Tgl(g),
                    Avancada::End(e) => AvancadaCopia::End(e),
                    Avancada::Outra(n) => AvancadaCopia::Outra(n),
                };
                Some((*pos, copia))
            })
            .collect();
        let paleta = |p: &Paleta| std::array::from_fn(|i| p.cor(i + 1));
        Copia {
            origem,
            nomes: self.nomes.clone(),
            atuacao: Some(self.atuacao.iter().map(|(p, a)| (*p, AtuacaoCopia::de(a))).collect()),
            funcoes: Some(funcoes),
            avancadas: Some(avancadas),
            macros: self
                .macros
                .as_ref()
                .map(|ms| ms.iter().enumerate().filter(|(_, m)| m.valida).map(|(i, m)| (i as u8, m.clone())).collect()),
            luz_teclas: Some(LuzCopia {
                luz: self.teclas.luz,
                paleta: paleta(&self.teclas.paleta),
                fixas: self.nomes.keys().filter_map(|p| self.teclas.grade.fixa(*p).map(|c| (*p, c))).collect(),
            }),
            luz_lateral: self.lateral.as_ref().map(|l| LuzCopia {
                luz: l.luz,
                paleta: paleta(&l.paleta),
                fixas: (0..l.grade.leds()).filter_map(|i| l.grade.fixa_do_led(i).map(|c| (i, c))).collect(),
            }),
        }
    }
}

// ---------- o que falta gravar ----------

/// Mudanças de uma área de luz: só o que é diferente vem preenchido.
#[derive(Default)]
struct PlanoDeLuz {
    luz: Option<Luz>,
    paleta: Option<Paleta>,
    grade: Option<Grade>,
    cores: usize,
}

#[derive(Default)]
struct Plano {
    /// Funções avançadas a tirar antes de mexer nas funções das teclas.
    remover: Vec<Pos>,
    funcoes: Vec<(u8, Pos, u16)>,
    avancadas: Vec<(Pos, AvancadaCopia)>,
    /// Quantas teclas mudam de função avançada (um par conta as duas).
    teclas_avancadas: usize,
    atuacao: Vec<(Pos, Atuacao)>,
    /// A macro vazia apaga a vaga.
    macros: Vec<(u8, Macro)>,
    teclas: PlanoDeLuz,
    lateral: PlanoDeLuz,
}

impl Plano {
    fn resumo(&self) -> Resumo {
        Resumo {
            atuacao: self.atuacao.len(),
            funcoes: self.funcoes.len(),
            avancadas: self.teclas_avancadas,
            macros: self.macros.len(),
            luz_teclas: self.teclas.luz.is_some() || self.teclas.paleta.is_some(),
            cores_teclas: self.teclas.cores,
            luz_lateral: self.lateral.luz.is_some() || self.lateral.paleta.is_some(),
            cores_lateral: self.lateral.cores,
        }
    }
}

fn invalido<T>(texto: impl Into<String>) -> Resultado<T> {
    Err(Erro::Invalido(texto.into()))
}

/// Verdadeiro se a função está no catálogo do programa. Uma função fora dele só é aceita de um
/// arquivo se o teclado já a tiver naquela tecla: o programa não grava o que não conhece.
fn conhecida(codigo: u16) -> bool {
    macro_do_codigo(codigo).is_some()
        || funcoes().iter().any(|(c, _)| *c == codigo)
        || combinacao(codigo).is_some()
        || especial(codigo).is_some_and(|e| e.familia != Familia::Reservada)
}

/// Verdadeiro se a função pode ser um passo de macro vindo de um arquivo: teclas comuns e as
/// funções especiais que a tela de macros oferece.
fn serve_em_macro(codigo: u16) -> bool {
    funcoes().iter().any(|(c, _)| *c == codigo && *c < 0x100)
        || especial(codigo).is_some_and(|e| {
            codigo != 0 && !matches!(e.familia, Familia::Reservada | Familia::Macro | Familia::Mouse | Familia::Atalho)
        })
}

fn tecla_comum(codigo: u16) -> bool {
    funcoes().iter().any(|(c, _)| *c == codigo && serve_em_avancada(*c))
}

impl Copia {
    /// Lê do teclado a configuração do perfil ativo.
    pub fn ler(t: &mut Teclado) -> Resultado<Self> {
        let (_, perfil) = t.perfis()?;
        let nome = crate::rotulo_perfil(perfil, &t.nome_perfil(perfil)?);
        let firmware = t.dispositivo()?.firmware;
        Ok(Estado::ler(t)?.copia(format!("{nome}, firmware {firmware}")))
    }

    /// Guarda em disco a configuração que o teclado tem agora, para poder voltar atrás depois de
    /// uma importação, e devolve onde ficou. O arquivo é sempre o mesmo: vale a última importação.
    pub fn guardar_a_atual(t: &mut Teclado) -> Resultado<std::path::PathBuf> {
        let caminho = crate::registro::Registro::caminho_padrao(&format!("antes-da-ultima-importacao.{EXTENSAO}"));
        let erro = |e: std::io::Error| Erro::Invalido(format!("não foi possível guardar a configuração atual em {}: {e}", caminho.display()));
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta).map_err(erro)?;
        }
        std::fs::write(&caminho, Self::ler(t)?.texto()).map_err(erro)?;
        Ok(caminho)
    }

    /// O que importar esta cópia mudaria no teclado, sem gravar nada. Falha se o arquivo pede
    /// algo que o teclado não tem ou que o programa não grava.
    pub fn comparar(&self, t: &mut Teclado) -> Resultado<Resumo> {
        Ok(self.plano(&Estado::ler(t)?)?.resumo())
    }

    /// Grava no perfil ativo o que for diferente e confere, relendo o teclado, que ele ficou
    /// igual à cópia. Devolve o que foi mudado.
    pub fn aplicar(&self, t: &mut Teclado) -> Resultado<Resumo> {
        let estado = Estado::ler(t)?;
        let plano = self.plano(&estado)?;
        let resumo = plano.resumo();
        if resumo.vazio() {
            return Ok(resumo);
        }
        self.gravar(t, &estado, plano)?;
        let falta = self.plano(&Estado::ler(t)?)?.resumo();
        if !falta.vazio() {
            return invalido(format!(
                "o teclado não ficou igual ao arquivo. Ainda diferente: {}",
                falta.frases().join(", ")
            ));
        }
        Ok(resumo)
    }

    /// A função avançada que a tecla deve ter, como o teclado a devolve: nos pares entram as
    /// funções das duas teclas na camada principal (`base`).
    fn esperada(&self, pos: Pos, base: &BTreeMap<Pos, u16>) -> Avancada {
        let codigos = |par: Pos| (base.get(&pos).copied().unwrap_or(0), base.get(&par).copied().unwrap_or(0));
        match self.avancadas.as_ref().and_then(|a| a.get(&pos)) {
            None => Avancada::Nenhuma,
            Some(AvancadaCopia::Socd { par, regra }) => Avancada::Socd { par: *par, codigos: codigos(*par), regra: *regra },
            Some(AvancadaCopia::Rs { par, atraso }) => Avancada::Rs { par: *par, codigos: codigos(*par), atraso: *atraso },
            Some(AvancadaCopia::Dks(d)) => Avancada::Dks(*d),
            Some(AvancadaCopia::Mpt(m)) => Avancada::Mpt(*m),
            Some(AvancadaCopia::Mt(m)) => Avancada::Mt(*m),
            Some(AvancadaCopia::Tgl(g)) => Avancada::Tgl(*g),
            Some(AvancadaCopia::End(e)) => Avancada::End(*e),
            Some(AvancadaCopia::Outra(n)) => Avancada::Outra(*n),
        }
    }

    fn plano(&self, e: &Estado) -> Resultado<Plano> {
        let mut plano = Plano::default();
        let nome = |pos: &Pos| e.nomes.get(pos).cloned().unwrap_or_else(|| format!("{},{}", pos.0, pos.1));
        let existe = |pos: &Pos, onde: &str| match e.nomes.contains_key(pos) {
            true => Ok(()),
            false => invalido(format!("{onde}: este teclado não tem tecla na posição {},{}", pos.0, pos.1)),
        };

        // funções de cada camada: o que o arquivo não cita volta ao padrão de fábrica
        let mut base = e.atual[0].clone();
        if let Some(funcoes) = &self.funcoes {
            for ((camada, pos), _) in funcoes {
                if *camada >= CAMADAS || pos.0 >= LINHAS || pos.1 >= COLUNAS {
                    return invalido(format!("funções: não existe a camada {camada}, posição {},{}", pos.0, pos.1));
                }
            }
            for camada in 0..CAMADAS {
                for (pos, atual) in &e.atual[camada as usize] {
                    let fabrica = e.fabrica[camada as usize][pos];
                    let alvo = funcoes.get(&(camada, *pos)).copied().unwrap_or(fabrica);
                    if alvo == *atual {
                        continue;
                    }
                    let padrao_da_camada = camada > 0 && alvo == FUNCAO_PADRAO;
                    if !(alvo == fabrica || padrao_da_camada || conhecida(alvo)) {
                        return invalido(format!(
                            "funções: o arquivo pede a função {} na tecla {}, e o programa não conhece essa função",
                            nome_tecla(alvo),
                            nome(pos)
                        ));
                    }
                    plano.funcoes.push((camada, *pos, alvo));
                    if camada == 0 {
                        base.insert(*pos, alvo);
                    }
                }
            }
        }

        // teclas avançadas
        if let Some(avancadas) = &self.avancadas {
            for (pos, a) in avancadas {
                existe(pos, "teclas avançadas")?;
                let funcoes_comuns = |codigos: &[u16]| match codigos.iter().find(|c| **c != 0 && !tecla_comum(**c)) {
                    Some(c) => invalido(format!(
                        "teclas avançadas: a tecla {} usa a função {}, e funções avançadas só aceitam teclas comuns",
                        nome(pos),
                        nome_tecla(*c)
                    )),
                    None => Ok(()),
                };
                match a {
                    AvancadaCopia::Socd { par, .. } | AvancadaCopia::Rs { par, .. } => {
                        existe(par, "teclas avançadas")?;
                        let espelho = match (a, avancadas.get(par)) {
                            (AvancadaCopia::Socd { regra, .. }, Some(AvancadaCopia::Socd { par: volta, regra: outra })) => {
                                volta == pos && ModoSocd::da_regra(*regra).regras() == (*regra, *outra)
                            }
                            (AvancadaCopia::Rs { atraso, .. }, Some(AvancadaCopia::Rs { par: volta, atraso: outro })) => {
                                volta == pos && atraso == outro
                            }
                            _ => false,
                        };
                        if !espelho || par == pos {
                            return invalido(format!("teclas avançadas: o par da tecla {} está incompleto", nome(pos)));
                        }
                        if let AvancadaCopia::Rs { atraso, .. } = a {
                            if *atraso > crate::ATRASO_MAX {
                                return invalido(format!("teclas avançadas: o atraso do RS vai de 0 a {} ms", crate::ATRASO_MAX));
                            }
                        }
                        if let Some(c) = [base[pos], base[par]].into_iter().find(|c| !serve_em_avancada(*c)) {
                            return invalido(format!(
                                "teclas avançadas: o par de {} e {} só funciona entre teclas comuns, e uma delas fica com a função {}",
                                nome(pos),
                                nome(par),
                                nome_tecla(c)
                            ));
                        }
                    }
                    AvancadaCopia::Dks(d) => d.validar().and_then(|()| funcoes_comuns(&d.teclas))?,
                    AvancadaCopia::Mpt(m) => m.validar().and_then(|()| funcoes_comuns(&m.teclas))?,
                    AvancadaCopia::Mt(m) => m.validar().and_then(|()| funcoes_comuns(&[m.toque, m.segura]))?,
                    AvancadaCopia::Tgl(g) => g.validar().and_then(|()| funcoes_comuns(&[g.tecla]))?,
                    AvancadaCopia::End(f) => f.validar().and_then(|()| funcoes_comuns(&[f.ao_pressionar, f.ao_soltar]))?,
                    AvancadaCopia::Outra(n) => {
                        if e.avancada.get(pos) != Some(&Avancada::Outra(*n)) {
                            return invalido(format!(
                                "teclas avançadas: a tecla {} usa um tipo de função ({n}) que o programa não sabe gravar",
                                nome(pos)
                            ));
                        }
                    }
                }
            }
            let mut em_par = BTreeSet::new();
            for (pos, atual) in &e.avancada {
                let alvo = self.esperada(*pos, &base);
                if alvo == *atual {
                    continue;
                }
                plano.teclas_avancadas += 1;
                if *atual != Avancada::Nenhuma {
                    plano.remover.push(*pos);
                }
                match avancadas.get(pos) {
                    Some(a @ (AvancadaCopia::Socd { par, .. } | AvancadaCopia::Rs { par, .. })) => {
                        // o par é gravado de uma vez, pelas duas teclas
                        if em_par.insert(*pos) && em_par.insert(*par) {
                            plano.avancadas.push((*pos, *a));
                        }
                    }
                    Some(a) => plano.avancadas.push((*pos, *a)),
                    None => {}
                }
            }
            // a outra tecla de um par regravado conta como mudada mesmo que já estivesse certa
            plano.teclas_avancadas = plano.teclas_avancadas.max(em_par.len());
        } else {
            // sem a seção, os pares que existem precisam continuar valendo com as funções novas
            for (pos, atual) in &e.avancada {
                if let Avancada::Socd { par, .. } | Avancada::Rs { par, .. } = atual {
                    if let Some(c) = [base[pos], base[par]].into_iter().find(|c| !serve_em_avancada(*c)) {
                        return invalido(format!(
                            "funções: a tecla {} faz parte de um par e não pode ficar com a função {}",
                            nome(pos),
                            nome_tecla(c)
                        ));
                    }
                }
            }
        }

        // atuação
        if let Some(atuacao) = &self.atuacao {
            for (pos, copia) in atuacao {
                existe(pos, "atuação")?;
                let atual = &e.atuacao[pos];
                if AtuacaoCopia::de(atual) == *copia {
                    continue;
                }
                let nova = copia.sobre(atual);
                nova.validar().map_err(|erro| Erro::Invalido(format!("atuação da tecla {}: {erro}", nome(pos))))?;
                plano.atuacao.push((*pos, nova));
            }
        }

        // macros: a vaga que o arquivo não cita fica vazia
        if let Some(macros) = &self.macros {
            let Some(gravadas) = &e.macros else {
                return invalido("macros: este teclado não guarda macros");
            };
            if let Some(indice) = macros.keys().find(|i| usize::from(**i) >= gravadas.len()) {
                return invalido(format!("macros: este teclado tem as macros de 1 a {}, e o arquivo traz a {}", gravadas.len(), indice + 1));
            }
            for (indice, gravada) in gravadas.iter().enumerate() {
                let alvo = macros.get(&(indice as u8)).cloned().unwrap_or_default();
                let igual = if alvo.valida { alvo == *gravada } else { !gravada.valida };
                if igual {
                    continue;
                }
                let erro = |texto: String| Erro::Invalido(format!("macro {}: {texto}", indice + 1));
                alvo.validar().map_err(|e| erro(e.to_string()))?;
                if let Some(acao) = alvo.acoes.iter().find(|a| !serve_em_macro(a.codigo)) {
                    return Err(erro(format!("a função {} não pode ser usada numa macro", nome_tecla(acao.codigo))));
                }
                plano.macros.push((indice as u8, alvo));
            }
        }

        // luzes
        if let Some(copia) = &self.luz_teclas {
            for pos in copia.fixas.keys() {
                existe(pos, "luz das teclas")?;
            }
            let teclas: Vec<Pos> = e.nomes.keys().copied().collect();
            plano.teclas = plano_de_luz("luz das teclas", copia, &e.teclas, &teclas, |g, p| g.fixa(*p), |g, p, c| g.definir(*p, c))?;
        }
        if let Some(copia) = &self.luz_lateral {
            let Some(lida) = &e.lateral else {
                return invalido("luz lateral: este teclado não tem luz lateral");
            };
            let leds: Vec<usize> = (0..lida.grade.leds()).collect();
            if let Some(led) = copia.fixas.keys().find(|l| **l >= leds.len()) {
                return invalido(format!("luz lateral: este teclado tem {} LEDs, e o arquivo cita o {}", leds.len(), led + 1));
            }
            plano.lateral =
                plano_de_luz("luz lateral", copia, lida, &leds, |g, l| g.fixa_do_led(*l), |g, l, c| g.definir_led(*l, c))?;
        }
        Ok(plano)
    }

    fn gravar(&self, t: &mut Teclado, e: &Estado, plano: Plano) -> Resultado<()> {
        // 1. tira as funções avançadas que mudam: um par segura a função das suas teclas
        for pos in &plano.remover {
            t.remover_avancada(*pos)?;
        }
        // 2. o que cada tecla envia
        let mut base = e.atual[0].clone();
        for (camada, pos, codigo) in &plano.funcoes {
            let lido = t.definir_funcao_na_camada(*camada, *pos, *codigo)?;
            if lido != *codigo {
                return invalido(format!(
                    "o teclado não aceitou a função {} na tecla {}",
                    nome_tecla(*codigo),
                    e.nomes.get(pos).cloned().unwrap_or_default()
                ));
            }
            if *camada == 0 {
                base.insert(*pos, *codigo);
            }
        }
        // 3. as funções avançadas novas
        for (pos, a) in &plano.avancadas {
            match a {
                AvancadaCopia::Socd { par, regra } => t.definir_socd(*pos, *par, base[pos], base[par], ModoSocd::da_regra(*regra))?,
                AvancadaCopia::Rs { par, atraso } => t.definir_rs(*pos, *par, base[pos], base[par], *atraso)?,
                AvancadaCopia::Dks(d) => t.definir_dks(*pos, d)?,
                AvancadaCopia::Mpt(m) => t.definir_mpt(*pos, m)?,
                AvancadaCopia::Mt(m) => t.definir_mt(*pos, m)?,
                AvancadaCopia::Tgl(g) => t.definir_tgl(*pos, g)?,
                AvancadaCopia::End(f) => t.definir_end(*pos, f)?,
                AvancadaCopia::Outra(_) => {}
            }
        }
        // 4. atuação
        for (pos, nova) in &plano.atuacao {
            t.definir_atuacao(*pos, nova)?;
        }
        // 5. macros
        for (indice, m) in &plano.macros {
            if m.valida {
                t.definir_macro(*indice, m)?;
            } else {
                t.apagar_macro(*indice)?;
            }
        }
        // 6. luzes: primeiro o efeito, porque a gravação das cores fixas depende dele
        for (lida, p, das_teclas) in [(Some(&e.teclas), plano.teclas, true), (e.lateral.as_ref(), plano.lateral, false)] {
            let Some(lida) = lida else { continue };
            if let Some(luz) = &p.luz {
                t.definir_luz_de(lida.area.indice, lida.area.efeitos, luz)?;
            }
            if let Some(paleta) = &p.paleta {
                t.definir_paleta_de(lida.area.indice, paleta)?;
            }
            if let Some(grade) = &p.grade {
                if das_teclas {
                    t.definir_grade(grade)?;
                } else {
                    t.definir_grade_de(lida.area.indice, grade)?;
                }
            }
        }
        Ok(())
    }
}

fn plano_de_luz<K: Ord + Copy>(
    onde: &str,
    copia: &LuzCopia<K>,
    lida: &LuzLida,
    posicoes: &[K],
    fixa: impl Fn(&Grade, &K) -> Option<Rgb>,
    definir: impl Fn(&mut Grade, &K, Option<Rgb>),
) -> Resultado<PlanoDeLuz> {
    let l = &copia.luz;
    if l.modo >= lida.area.efeitos {
        return invalido(format!("{onde}: o efeito vai de 0 a {}", lida.area.efeitos.saturating_sub(1)));
    }
    if l.brilho > 100 || l.velocidade > 100 {
        return invalido(format!("{onde}: brilho e velocidade vão de 0 a 100"));
    }
    if l.cor > 7 {
        return invalido(format!("{onde}: a cor do efeito vai de 0 a 7"));
    }
    let mut plano = PlanoDeLuz::default();
    if *l != lida.luz {
        plano.luz = Some(*l);
    }
    if (0..7).any(|i| lida.paleta.cor(i + 1) != copia.paleta[i]) {
        let mut paleta = lida.paleta.clone();
        for (i, cor) in copia.paleta.iter().enumerate() {
            paleta.definir(i + 1, *cor);
        }
        plano.paleta = Some(paleta);
    }
    let mut grade = lida.grade.clone();
    for pos in posicoes {
        let alvo = copia.fixas.get(pos).copied();
        if fixa(&grade, pos) != alvo {
            definir(&mut grade, pos, alvo);
            plano.cores += 1;
        }
    }
    if plano.cores > 0 {
        plano.grade = Some(grade);
    }
    Ok(plano)
}

// ---------- texto ----------

fn mm(valor: u16) -> String {
    if valor % 10 == 0 {
        format!("{}.{:02}", valor / 1000, valor % 1000 / 10)
    } else {
        format!("{}.{:03}", valor / 1000, valor % 1000)
    }
}

fn sim(valor: bool) -> &'static str {
    if valor {
        "sim"
    } else {
        "nao"
    }
}

/// Nome de uma função no arquivo. `nada` é a vaga livre de uma função avançada.
fn funcao(codigo: u16) -> String {
    match codigo {
        0 => "nada".into(),
        c => nome_tecla(c),
    }
}

fn posicao(pos: Pos) -> String {
    format!("{},{}", pos.0, pos.1)
}

impl Copia {
    /// A cópia escrita como arquivo de texto (ver [`Copia::de_texto`]).
    pub fn texto(&self) -> String {
        let mut s = String::new();
        let de = |pos: &Pos| self.nomes.get(pos).map(|n| format!("  # {n}")).unwrap_or_default();
        let origem = if self.origem.is_empty() { String::new() } else { format!(" ({})", self.origem) };
        let _ = writeln!(s, "# Configuração do teclado Avell Volcano's MAG82{origem}");
        let _ = writeln!(s, "# Arquivo de texto: pode ser lido, editado e importado de volta pelo programa.");
        let _ = writeln!(s, "# Ao importar, cada seção presente vale por inteiro; uma seção apagada deixa");
        let _ = writeln!(s, "# aquela parte do teclado como está. As teclas são indicadas por linha,coluna.");
        let _ = writeln!(s, "{MARCA} {FORMATO}");

        if let Some(atuacao) = &self.atuacao {
            let _ = writeln!(s, "\n[atuacao]");
            let _ = writeln!(s, "# tecla, rapid trigger (sim ou nao) e, em milímetros: ponto de atuação, ponto de");
            let _ = writeln!(s, "# soltura, início do rapid trigger, rapid trigger ao pressionar e ao soltar,");
            let _ = writeln!(s, "# zona morta no início e no fim do curso");
            for (pos, a) in atuacao {
                let valores: Vec<String> = a.mm.iter().map(|v| mm(*v)).collect();
                let _ = writeln!(s, "{:<5} {}  {}{}", posicao(*pos), sim(a.rapid_trigger), valores.join(" "), de(pos));
            }
        }
        if let Some(funcoes) = &self.funcoes {
            let _ = writeln!(s, "\n[funcoes]");
            let _ = writeln!(s, "# O que cada tecla envia, só onde é diferente do padrão de fábrica:");
            let _ = writeln!(s, "# camada (0 = tecla sozinha, 1 = com Fn), tecla e função");
            for ((camada, pos), codigo) in funcoes {
                let nome = match *codigo {
                    0 => "desativada".to_string(),
                    FUNCAO_PADRAO if *camada > 0 => "igual-a-tecla".to_string(),
                    c => nome_tecla(c),
                };
                let _ = writeln!(s, "{camada} {:<5} {nome}{}", posicao(*pos), de(pos));
            }
        }
        if let Some(avancadas) = &self.avancadas {
            let _ = writeln!(s, "\n[avancadas]");
            let _ = writeln!(s, "# socd TECLA TECLA ultima|primeira|segunda|neutro");
            let _ = writeln!(s, "# rs   TECLA TECLA ATRASO(ms)");
            let _ = writeln!(s, "# dks  TECLA INICIO(mm) FUNDO(mm) e até quatro vezes: FUNCAO MOMENTOS");
            let _ = writeln!(s, "# mpt  TECLA e três vezes: FUNCAO PROFUNDIDADE(mm)");
            let _ = writeln!(s, "# mt   TECLA FUNCAO-NO-TOQUE FUNCAO-AO-SEGURAR TEMPO(ms)");
            let _ = writeln!(s, "# tgl  TECLA FUNCAO TEMPO(ms)");
            let _ = writeln!(s, "# end  TECLA FUNCAO-AO-PRESSIONAR FUNCAO-AO-SOLTAR ATRASO(ms)");
            for (pos, a) in avancadas {
                let p = posicao(*pos);
                let linha = match a {
                    // o par aparece uma vez, a partir da primeira das duas teclas
                    AvancadaCopia::Socd { par, .. } | AvancadaCopia::Rs { par, .. } if par < pos => continue,
                    AvancadaCopia::Socd { par, regra } => {
                        let modo = ModoSocd::TODOS.iter().position(|m| *m == ModoSocd::da_regra(*regra)).unwrap_or(0);
                        format!("socd {p} {} {}", posicao(*par), MODOS_DE_SOCD[modo])
                    }
                    AvancadaCopia::Rs { par, atraso } => format!("rs {p} {} {atraso}", posicao(*par)),
                    AvancadaCopia::Dks(d) => {
                        // as vagas ficam na ordem em que estão; uma vaga livre no meio vira "nada -"
                        let usadas = d.teclas.iter().rposition(|t| *t != 0).map_or(0, |i| i + 1);
                        let vagas: Vec<String> = (0..usadas)
                            .map(|i| match d.teclas[i] {
                                0 => "nada -".to_string(),
                                tecla => format!("{} {}", funcao(tecla), Dks::casas_em_texto(d.casas(i))),
                            })
                            .collect();
                        format!("dks {p} {} {} {}", mm(d.inicio), mm(d.fundo), vagas.join(" "))
                    }
                    AvancadaCopia::Mpt(m) => {
                        let vagas: Vec<String> = (0..3).map(|i| format!("{} {}", funcao(m.teclas[i]), mm(m.pontos[i]))).collect();
                        format!("mpt {p} {}", vagas.join(" "))
                    }
                    AvancadaCopia::Mt(m) => format!("mt {p} {} {} {}", funcao(m.toque), funcao(m.segura), m.tempo),
                    AvancadaCopia::Tgl(g) => format!("tgl {p} {} {}", funcao(g.tecla), g.tempo),
                    AvancadaCopia::End(e) => format!("end {p} {} {} {}", funcao(e.ao_pressionar), funcao(e.ao_soltar), e.atraso),
                    AvancadaCopia::Outra(n) => format!("outra {p} {n}"),
                };
                let _ = writeln!(s, "{linha}{}", de(pos));
            }
        }
        if let Some(macros) = &self.macros {
            let _ = writeln!(s, "\n[macros]");
            let _ = writeln!(s, "# número, modo ({}), repetições e as ações:", MODOS_DE_MACRO.join(", "));
            let _ = writeln!(s, "# +a pressiona, -a solta, a faz as duas coisas e 50ms é uma espera");
            for (indice, m) in macros {
                let _ = writeln!(s, "{} {} {} {}", indice + 1, MODOS_DE_MACRO[m.modo.byte() as usize], m.repeticoes, m.em_texto());
            }
        }
        let mut luz = |titulo: &str, l: &Luz, paleta: &[Rgb; 7], fixas: Vec<(String, Rgb, String)>, alvo: &str| {
            let _ = writeln!(s, "\n[{titulo}]");
            let _ = writeln!(s, "ligada {}", sim(l.ligada));
            let _ = writeln!(s, "efeito {}", l.modo);
            let _ = writeln!(s, "brilho {}", l.brilho);
            let _ = writeln!(s, "velocidade {}", l.velocidade);
            let _ = writeln!(s, "invertida {}", sim(l.invertida));
            let _ = writeln!(s, "cor {}", l.cor);
            let cores: Vec<String> = paleta.iter().map(|c| c.hex()[1..].to_string()).collect();
            let _ = writeln!(s, "paleta {}", cores.join(" "));
            if !fixas.is_empty() {
                let _ = writeln!(s, "# cores fixas: {alvo} e cor");
            }
            for (onde, cor, nota) in fixas {
                let _ = writeln!(s, "fixa {onde:<5} {}{nota}", &cor.hex()[1..]);
            }
        };
        if let Some(l) = &self.luz_teclas {
            let fixas = l.fixas.iter().map(|(p, c)| (posicao(*p), *c, de(p))).collect();
            luz("luz-teclas", &l.luz, &l.paleta, fixas, "tecla");
        }
        if let Some(l) = &self.luz_lateral {
            let fixas = l.fixas.iter().map(|(led, c)| ((led + 1).to_string(), *c, String::new())).collect();
            luz("luz-lateral", &l.luz, &l.paleta, fixas, "número do LED");
        }
        s
    }

    /// Lê um arquivo escrito por [`Copia::texto`]. O erro diz a linha e o que não foi entendido.
    pub fn de_texto(texto: &str) -> Resultado<Self> {
        let mut copia = Copia::default();
        let mut secao = String::new();
        let mut marcado = false;
        let mut luzes: BTreeMap<String, LuzEmLeitura> = BTreeMap::new();
        for (numero, bruta) in texto.lines().enumerate() {
            let linha = sem_comentario(bruta).trim();
            if linha.is_empty() {
                continue;
            }
            let erro = |texto: String| Erro::Invalido(format!("linha {}: {texto}", numero + 1));
            let partes: Vec<&str> = linha.split_whitespace().collect();
            if !marcado {
                if partes.len() != 2 || partes[0] != MARCA {
                    return Err(erro("este não parece ser um arquivo de configuração do MAG82".into()));
                }
                match partes[1].parse::<u32>() {
                    Ok(n) if (1..=FORMATO).contains(&n) => marcado = true,
                    Ok(_) => return Err(erro("o arquivo foi feito por uma versão mais nova do programa".into())),
                    Err(_) => return Err(erro("número de formato inválido".into())),
                }
                continue;
            }
            if let Some(nome) = linha.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                secao = nome.trim().to_lowercase();
                let repetida = match secao.as_str() {
                    "atuacao" => copia.atuacao.replace(BTreeMap::new()).is_some(),
                    "funcoes" => copia.funcoes.replace(BTreeMap::new()).is_some(),
                    "avancadas" => copia.avancadas.replace(BTreeMap::new()).is_some(),
                    "macros" => copia.macros.replace(BTreeMap::new()).is_some(),
                    "luz-teclas" | "luz-lateral" => luzes.insert(secao.clone(), LuzEmLeitura::default()).is_some(),
                    _ => return Err(erro(format!("seção desconhecida: [{nome}]"))),
                };
                if repetida {
                    return Err(erro(format!("a seção [{nome}] aparece duas vezes")));
                }
                continue;
            }
            let resultado = match secao.as_str() {
                "atuacao" => ler_atuacao(&partes).map(|(pos, a)| copia.atuacao.as_mut().unwrap().insert(pos, a).is_some()),
                "funcoes" => ler_funcao(&partes).map(|(chave, c)| copia.funcoes.as_mut().unwrap().insert(chave, c).is_some()),
                "avancadas" => ler_avancada(&partes).and_then(|itens| {
                    let mapa = copia.avancadas.as_mut().unwrap();
                    Ok(itens.into_iter().any(|(pos, a)| mapa.insert(pos, a).is_some()))
                }),
                "macros" => ler_macro(&partes).map(|(i, m)| copia.macros.as_mut().unwrap().insert(i, m).is_some()),
                "luz-teclas" | "luz-lateral" => luzes.get_mut(&secao).unwrap().ler(&partes, secao == "luz-lateral"),
                _ => Err("o arquivo precisa começar por uma seção, como [atuacao]".to_string()),
            };
            match resultado {
                Ok(false) => {}
                Ok(true) => return Err(erro("item repetido".into())),
                Err(texto) => return Err(erro(texto)),
            }
        }
        if !marcado {
            return invalido("este não parece ser um arquivo de configuração do MAG82");
        }
        if let Some(l) = luzes.remove("luz-teclas") {
            let (luz, paleta) = l.fechar("luz-teclas")?;
            copia.luz_teclas = Some(LuzCopia { luz, paleta, fixas: l.teclas });
        }
        if let Some(l) = luzes.remove("luz-lateral") {
            let (luz, paleta) = l.fechar("luz-lateral")?;
            copia.luz_lateral = Some(LuzCopia { luz, paleta, fixas: l.leds });
        }
        Ok(copia)
    }
}

/// Tira o comentário: um `#` no começo da linha ou depois de um espaço (o `#` colado numa
/// palavra faz parte dela, como em `iso#`).
fn sem_comentario(linha: &str) -> &str {
    let mut anterior = ' ';
    for (i, letra) in linha.char_indices() {
        if letra == '#' && anterior.is_whitespace() {
            return &linha[..i];
        }
        anterior = letra;
    }
    linha
}

type Lido<T> = Result<T, String>;

fn ler_pos(texto: &str) -> Lido<Pos> {
    texto
        .split_once(',')
        .and_then(|(l, c)| Some((l.parse().ok()?, c.parse().ok()?)))
        .ok_or_else(|| format!("posição de tecla inválida: {texto} (o formato é linha,coluna)"))
}

fn ler_mm(texto: &str) -> Lido<u16> {
    match texto.replace(',', ".").parse::<f64>() {
        Ok(v) if (0.0..=65.0).contains(&v) => Ok((v * 1000.0).round() as u16),
        _ => Err(format!("distância inválida: {texto}")),
    }
}

fn ler_numero<T: std::str::FromStr>(texto: &str) -> Lido<T> {
    texto.parse().map_err(|_| format!("número inválido: {texto}"))
}

fn ler_sim(texto: &str) -> Lido<bool> {
    match texto.to_lowercase().as_str() {
        "sim" => Ok(true),
        "nao" | "não" => Ok(false),
        _ => Err(format!("esperava sim ou nao, e veio: {texto}")),
    }
}

fn ler_nome(texto: &str) -> Lido<u16> {
    match texto.to_lowercase().as_str() {
        "nada" | "desativada" => Ok(0),
        outro => codigo_tecla(outro).ok_or_else(|| format!("função desconhecida: {texto}")),
    }
}

fn ler_cor(texto: &str) -> Lido<Rgb> {
    Rgb::de_hex(texto).ok_or_else(|| format!("cor inválida: {texto} (o formato é ff8800)"))
}

fn quantidade(partes: &[&str], n: usize, modelo: &str) -> Lido<()> {
    if partes.len() == n {
        Ok(())
    } else {
        Err(format!("o formato é: {modelo}"))
    }
}

fn ler_atuacao(partes: &[&str]) -> Lido<(Pos, AtuacaoCopia)> {
    quantidade(partes, 9, "TECLA sim|nao e sete distâncias em milímetros")?;
    let mut valores = [0u16; 7];
    for (vaga, texto) in valores.iter_mut().zip(&partes[2..]) {
        *vaga = ler_mm(texto)?;
    }
    Ok((ler_pos(partes[0])?, AtuacaoCopia { rapid_trigger: ler_sim(partes[1])?, mm: valores }))
}

fn ler_funcao(partes: &[&str]) -> Lido<((u8, Pos), u16)> {
    quantidade(partes, 3, "CAMADA TECLA FUNCAO")?;
    let camada: u8 = ler_numero(partes[0])?;
    let codigo = match partes[2].to_lowercase().as_str() {
        "igual-a-tecla" if camada > 0 => FUNCAO_PADRAO,
        _ => ler_nome(partes[2])?,
    };
    Ok(((camada, ler_pos(partes[1])?), codigo))
}

fn ler_avancada(partes: &[&str]) -> Lido<Vec<(Pos, AvancadaCopia)>> {
    let tipo = partes[0].to_lowercase();
    let pos = ler_pos(partes.get(1).ok_or("falta a tecla")?)?;
    let resto = &partes[2..];
    let uma = |a: AvancadaCopia| Ok(vec![(pos, a)]);
    match tipo.as_str() {
        "socd" => {
            quantidade(resto, 2, "socd TECLA TECLA ultima|primeira|segunda|neutro")?;
            let par = ler_pos(resto[0])?;
            let modo = MODOS_DE_SOCD
                .iter()
                .position(|m| *m == resto[1].to_lowercase())
                .ok_or_else(|| format!("modo de SOCD desconhecido: {}", resto[1]))?;
            let (regra_a, regra_b) = ModoSocd::TODOS[modo].regras();
            Ok(vec![(pos, AvancadaCopia::Socd { par, regra: regra_a }), (par, AvancadaCopia::Socd { par: pos, regra: regra_b })])
        }
        "rs" => {
            quantidade(resto, 2, "rs TECLA TECLA ATRASO")?;
            let (par, atraso) = (ler_pos(resto[0])?, ler_numero(resto[1])?);
            Ok(vec![(pos, AvancadaCopia::Rs { par, atraso }), (par, AvancadaCopia::Rs { par: pos, atraso })])
        }
        "dks" => {
            let modelo = "dks TECLA INICIO FUNDO e até quatro vezes: FUNCAO MOMENTOS";
            if resto.len() < 4 || resto.len() > 10 || resto.len() % 2 != 0 {
                return Err(format!("o formato é: {modelo}"));
            }
            let mut dks = Dks { inicio: ler_mm(resto[0])?, fundo: ler_mm(resto[1])?, ..Dks::default() };
            for (i, vaga) in resto[2..].chunks(2).enumerate() {
                dks.teclas[i] = ler_nome(vaga[0])?;
                if dks.teclas[i] == 0 && vaga[1] == "-" {
                    continue;
                }
                let casas = Dks::casas_de_texto(vaga[1]).ok_or_else(|| format!("momentos do DKS inválidos: {}", vaga[1]))?;
                dks.definir_casas(i, casas);
            }
            uma(AvancadaCopia::Dks(dks))
        }
        "mpt" => {
            quantidade(resto, 6, "mpt TECLA e três vezes: FUNCAO PROFUNDIDADE")?;
            let mut mpt = Mpt::default();
            for (i, vaga) in resto.chunks(2).enumerate() {
                mpt.teclas[i] = ler_nome(vaga[0])?;
                mpt.pontos[i] = ler_mm(vaga[1])?;
            }
            uma(AvancadaCopia::Mpt(mpt))
        }
        "mt" => {
            quantidade(resto, 3, "mt TECLA FUNCAO-NO-TOQUE FUNCAO-AO-SEGURAR TEMPO")?;
            uma(AvancadaCopia::Mt(Mt { toque: ler_nome(resto[0])?, segura: ler_nome(resto[1])?, tempo: ler_numero(resto[2])? }))
        }
        "tgl" => {
            quantidade(resto, 2, "tgl TECLA FUNCAO TEMPO")?;
            uma(AvancadaCopia::Tgl(Tgl { tecla: ler_nome(resto[0])?, tempo: ler_numero(resto[1])? }))
        }
        "end" => {
            quantidade(resto, 3, "end TECLA FUNCAO-AO-PRESSIONAR FUNCAO-AO-SOLTAR ATRASO")?;
            uma(AvancadaCopia::End(End {
                ao_pressionar: ler_nome(resto[0])?,
                ao_soltar: ler_nome(resto[1])?,
                atraso: ler_numero(resto[2])?,
            }))
        }
        "outra" => {
            quantidade(resto, 1, "outra TECLA TIPO")?;
            uma(AvancadaCopia::Outra(ler_numero(resto[0])?))
        }
        _ => Err(format!("tipo de função avançada desconhecido: {}", partes[0])),
    }
}

fn ler_macro(partes: &[&str]) -> Lido<(u8, Macro)> {
    if partes.len() < 4 {
        return Err("o formato é: NUMERO MODO REPETICOES e as ações".into());
    }
    let numero: u8 = ler_numero(partes[0])?;
    if numero == 0 {
        return Err("as macros são numeradas a partir de 1".into());
    }
    let modo = MODOS_DE_MACRO
        .iter()
        .position(|m| *m == partes[1].to_lowercase())
        .ok_or_else(|| format!("modo de macro desconhecido: {}", partes[1]))?;
    let acoes = Macro::acoes_de_texto(&partes[3..].join(" ")).map_err(|e| e.to_string())?;
    Ok((numero - 1, Macro { valida: true, modo: ModoMacro::TODOS[modo], repeticoes: ler_numero(partes[2])?, acoes }))
}

/// Uma seção de luz enquanto o arquivo é lido: todos os campos precisam aparecer.
#[derive(Default)]
struct LuzEmLeitura {
    ligada: Option<bool>,
    efeito: Option<u8>,
    brilho: Option<u8>,
    velocidade: Option<u8>,
    invertida: Option<bool>,
    cor: Option<u8>,
    paleta: Option<[Rgb; 7]>,
    teclas: BTreeMap<Pos, Rgb>,
    leds: BTreeMap<usize, Rgb>,
}

impl LuzEmLeitura {
    /// Devolve verdadeiro se o item já tinha aparecido.
    fn ler(&mut self, partes: &[&str], lateral: bool) -> Lido<bool> {
        let campo = partes[0].to_lowercase();
        if campo == "fixa" {
            quantidade(partes, 3, if lateral { "fixa NUMERO-DO-LED COR" } else { "fixa TECLA COR" })?;
            let cor = ler_cor(partes[2])?;
            return Ok(if lateral {
                let led: usize = ler_numero(partes[1])?;
                if led == 0 {
                    return Err("os LEDs são numerados a partir de 1".into());
                }
                self.leds.insert(led - 1, cor).is_some()
            } else {
                self.teclas.insert(ler_pos(partes[1])?, cor).is_some()
            });
        }
        if campo == "paleta" {
            quantidade(partes, 8, "paleta e sete cores")?;
            let mut cores = [Rgb(0, 0, 0); 7];
            for (vaga, texto) in cores.iter_mut().zip(&partes[1..]) {
                *vaga = ler_cor(texto)?;
            }
            return Ok(self.paleta.replace(cores).is_some());
        }
        quantidade(partes, 2, "CAMPO VALOR")?;
        Ok(match campo.as_str() {
            "ligada" => self.ligada.replace(ler_sim(partes[1])?).is_some(),
            "efeito" => self.efeito.replace(ler_numero(partes[1])?).is_some(),
            "brilho" => self.brilho.replace(ler_numero(partes[1])?).is_some(),
            "velocidade" => self.velocidade.replace(ler_numero(partes[1])?).is_some(),
            "invertida" => self.invertida.replace(ler_sim(partes[1])?).is_some(),
            "cor" => self.cor.replace(ler_numero(partes[1])?).is_some(),
            _ => return Err(format!("campo desconhecido: {}", partes[0])),
        })
    }

    fn fechar(&self, secao: &str) -> Resultado<(Luz, [Rgb; 7])> {
        let falta = |campo: &str| Erro::Invalido(format!("falta \"{campo}\" na seção [{secao}]"));
        Ok((
            Luz {
                ligada: self.ligada.ok_or_else(|| falta("ligada"))?,
                modo: self.efeito.ok_or_else(|| falta("efeito"))?,
                brilho: self.brilho.ok_or_else(|| falta("brilho"))?,
                velocidade: self.velocidade.ok_or_else(|| falta("velocidade"))?,
                invertida: self.invertida.ok_or_else(|| falta("invertida"))?,
                cor: self.cor.ok_or_else(|| falta("cor"))?,
            },
            self.paleta.ok_or_else(|| falta("paleta"))?,
        ))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::AcaoMacro;

    fn exemplo() -> Copia {
        let mut dks = Dks { teclas: [0x04, 0, 0x05, 0], inicio: 1500, fundo: 3000, ..Dks::default() };
        dks.definir_casas(0, Dks::casas_de_texto("1-2").unwrap());
        dks.definir_casas(2, Dks::casas_de_texto("3,4").unwrap());
        let tecla = |codigo| [AcaoMacro { pressiona: true, espera: 0, codigo }, AcaoMacro { pressiona: false, espera: 50, codigo }];
        Copia {
            origem: "Perfil 1, firmware 0.0.2.0".into(),
            nomes: [((0, 0), "esc".to_string()), ((2, 1), "a".to_string())].into(),
            atuacao: Some(
                [
                    ((0, 0), AtuacaoCopia { rapid_trigger: false, mm: [2000, 2000, 500, 300, 300, 0, 0] }),
                    ((2, 1), AtuacaoCopia { rapid_trigger: true, mm: [1200, 1200, 505, 150, 155, 200, 100] }),
                ]
                .into(),
            ),
            funcoes: Some(
                [((0, (1, 14)), 0x2192), ((0, (3, 3)), 0), ((1, (2, 2)), FUNCAO_PADRAO), ((1, (0, 5)), 0x1807), ((0, (4, 1)), 0x32)].into(),
            ),
            avancadas: Some(
                [
                    ((2, 1), AvancadaCopia::Socd { par: (2, 3), regra: 1 }),
                    ((2, 3), AvancadaCopia::Socd { par: (2, 1), regra: 2 }),
                    ((1, 1), AvancadaCopia::Rs { par: (1, 2), atraso: 5 }),
                    ((1, 2), AvancadaCopia::Rs { par: (1, 1), atraso: 5 }),
                    ((3, 1), AvancadaCopia::Dks(dks)),
                    ((3, 2), AvancadaCopia::Mpt(Mpt { teclas: [0x04, 0, 0x06], pontos: [500, 1000, 1505] })),
                    ((3, 3), AvancadaCopia::Mt(Mt { toque: 0x04, segura: 0xE0, tempo: 200 })),
                    ((3, 4), AvancadaCopia::Tgl(Tgl { tecla: 0x1A, tempo: 300 })),
                    ((3, 5), AvancadaCopia::End(End { ao_pressionar: 0x14, ao_soltar: 0, atraso: 10 })),
                ]
                .into(),
            ),
            macros: Some(
                [(2u8, Macro { valida: true, modo: ModoMacro::SegurarTermina, repeticoes: 3, acoes: [tecla(0x0B), tecla(0x0C)].concat() })]
                    .into(),
            ),
            luz_teclas: Some(LuzCopia {
                luz: Luz { ligada: true, modo: 3, brilho: 80, velocidade: 50, invertida: false, cor: 2 },
                paleta: [Rgb(255, 0, 0); 7],
                fixas: [((2, 1), Rgb(255, 136, 0))].into(),
            }),
            luz_lateral: Some(LuzCopia {
                luz: Luz { ligada: false, modo: 0, brilho: 10, velocidade: 0, invertida: true, cor: 7 },
                paleta: [Rgb(1, 2, 3); 7],
                fixas: [(0, Rgb(0, 0, 255)), (29, Rgb(0, 255, 0))].into(),
            }),
        }
    }

    #[test]
    fn o_texto_vai_e_volta() {
        let copia = exemplo();
        let texto = copia.texto();
        let lida = Copia::de_texto(&texto).unwrap();
        // os nomes e a origem são só comentários no arquivo
        assert_eq!(lida, Copia { origem: String::new(), nomes: BTreeMap::new(), ..copia });
        assert_eq!(lida.texto().lines().skip(1).collect::<Vec<_>>(), {
            let sem_nomes = Copia { origem: String::new(), nomes: BTreeMap::new(), ..exemplo() };
            sem_nomes.texto().lines().skip(1).map(str::to_string).collect::<Vec<_>>()
        });
    }

    #[test]
    fn secao_ausente_fica_de_fora() {
        let lida = Copia::de_texto("mag82-configuracao 1\n[funcoes]\n0 1,14 abrir-a-calculadora # insert\n").unwrap();
        assert_eq!(lida.funcoes, Some([((0, (1, 14)), 0x2192)].into()));
        assert!(lida.atuacao.is_none() && lida.avancadas.is_none() && lida.macros.is_none());
        assert!(lida.luz_teclas.is_none() && lida.luz_lateral.is_none());
    }

    #[test]
    fn erros_dizem_a_linha() {
        let erro = |texto: &str| Copia::de_texto(texto).unwrap_err().to_string();
        assert!(erro("qualquer coisa").contains("não parece"));
        assert!(erro("mag82-configuracao 2").contains("versão mais nova"));
        assert!(erro("mag82-configuracao 1\n[funcoes]\n0 1,14 tecla-que-nao-existe").starts_with("linha 3"));
        assert!(erro("mag82-configuracao 1\n[atuacao]\n0,0 sim 1.0").starts_with("linha 3"));
        assert!(erro("mag82-configuracao 1\n[funcoes]\n0 1,1 a\n0 1,1 b").contains("repetido"));
        assert!(erro("mag82-configuracao 1\n[luz-teclas]\nligada sim").contains("falta"));
        assert!(erro("mag82-configuracao 1\n[inventada]").contains("desconhecida"));
    }

    #[test]
    fn comentario_nao_corta_nome_com_cerquilha() {
        assert_eq!(sem_comentario("0 4,1 iso#  # tecla"), "0 4,1 iso#  ");
        assert_eq!(sem_comentario("# tudo"), "");
    }

    #[test]
    fn so_grava_funcoes_do_catalogo() {
        assert!(conhecida(0x04) && conhecida(0x2192) && conhecida(0x1807) && conhecida(0xF500) && conhecida(0));
        // reservadas (restaurar, calibrar), modos sem fio e códigos soltos ficam de fora
        assert!(!conhecida(0xF200) && !conhecida(0xF203) && !conhecida(0xF401) && !conhecida(0x99) && !conhecida(0x0107));
        assert!(serve_em_macro(0x04) && serve_em_macro(0x20E9));
        assert!(!serve_em_macro(0x4100) && !serve_em_macro(0x1807) && !serve_em_macro(0xF500) && !serve_em_macro(0xF200));
    }
}
