//! Tudo o que foi lido do teclado. A interface só mostra o que está aqui, e isto só muda
//! depois que o teclado confirma uma gravação.

use std::collections::BTreeMap;

use mag82::{AreaLuz, Atuacao, Avancada, Dispositivo, Geometria, Grade, Luz, Paleta, Pos, Resultado, Teclado};

/// Luz da faixa lateral do teclado.
pub struct Lateral {
    pub area: AreaLuz,
    pub luz: Luz,
    pub paleta: Paleta,
    /// Cor de cada LED (as fixas e as que o efeito mostrava na leitura).
    pub grade: Grade,
}

pub struct Dados {
    pub nomes: BTreeMap<Pos, String>,
    pub geo: BTreeMap<Pos, Geometria>,
    pub fabrica: BTreeMap<Pos, u16>,
    pub atual: BTreeMap<Pos, u16>,
    /// O que cada tecla faz com Fn pressionado, e como isso vem de fábrica.
    pub fn_atual: BTreeMap<Pos, u16>,
    pub fn_fabrica: BTreeMap<Pos, u16>,
    pub grade: Grade,
    pub atuacao: BTreeMap<Pos, Atuacao>,
    pub avancada: BTreeMap<Pos, Avancada>,
    /// As macros gravadas no teclado; `None` se ele não responde aos pedidos de macro.
    pub macros: Option<Vec<mag82::Macro>>,
    /// Luz das teclas.
    pub luz: Luz,
    pub paleta: Paleta,
    /// Quantidade de efeitos da luz das teclas.
    pub efeitos: u8,
    /// `None` se o teclado não tem luz lateral.
    pub lateral: Option<Lateral>,
    pub taxas: Vec<u8>,
    pub taxa_atual: u8,
    /// Nomes dos perfis, já prontos para exibição.
    pub perfis: Vec<String>,
    pub perfil_atual: u8,
    pub dispositivo: Dispositivo,
}

impl Dados {
    pub fn ler(t: &mut Teclado) -> Resultado<Self> {
        let fabrica = t.layout(true)?;
        let nomes: BTreeMap<Pos, String> =
            fabrica.iter().filter(|(_, c)| **c != 0).map(|(p, c)| (*p, mag82::nome_tecla(*c))).collect();
        let mut atuacao = BTreeMap::new();
        let mut avancada = BTreeMap::new();
        for pos in nomes.keys() {
            atuacao.insert(*pos, t.atuacao(*pos)?);
            avancada.insert(*pos, t.avancada(*pos)?);
        }
        let (total, perfil_atual) = t.perfis()?;
        let mut perfis = Vec::new();
        for i in 0..total {
            perfis.push(mag82::rotulo_perfil(i, &t.nome_perfil(i)?));
        }
        let (taxas, taxa_atual) = t.taxas()?;
        let areas = t.areas_luz()?;
        let efeitos = areas.iter().find(|a| a.indice == mag82::AREA_TECLAS).map_or(mag82::EFEITOS, |a| a.efeitos);
        let lateral = match areas.iter().find(|a| a.indice == mag82::AREA_LATERAL) {
            Some(area) => Some(Lateral {
                area: *area,
                luz: t.luz_de(area.indice)?,
                paleta: t.paleta_de(area.indice)?,
                grade: t.grade_de(area)?,
            }),
            None => None,
        };
        Ok(Self {
            avancada,
            macros: match t.espaco_de_macros() {
                Ok((total, _)) => Some((0..total.min(mag82::MACROS_MAX)).map(|i| t.macro_gravada(i)).collect::<Resultado<Vec<_>>>()?),
                Err(_) => None,
            },
            perfis,
            perfil_atual,
            taxas,
            taxa_atual,
            efeitos,
            lateral,
            geo: t.geometria()?,
            atual: t.layout(false)?,
            fn_atual: t.layout_da_camada(mag82::CAMADA_FN, false)?,
            fn_fabrica: t.layout_da_camada(mag82::CAMADA_FN, true)?,
            grade: t.grade()?,
            luz: t.luz()?,
            paleta: t.paleta()?,
            dispositivo: t.dispositivo()?,
            nomes,
            fabrica,
            atuacao,
        })
    }

    /// Tudo o que é configuração, num texto só, para saber se duas leituras do teclado são a
    /// mesma configuração. Ficam de fora as cores que o efeito de luz mostrava no instante da
    /// leitura, que mudam sozinhas.
    pub fn configuracao(&self) -> String {
        let fixas: Vec<_> = self.nomes.keys().filter_map(|p| self.grade.fixa(*p).map(|c| (*p, c))).collect();
        let lateral = self.lateral.as_ref().map(|l| {
            let fixos: Vec<_> = (0..l.grade.leds()).filter_map(|i| l.grade.fixa_do_led(i).map(|c| (i, c))).collect();
            format!("{:?} {:?} {fixos:?}", l.luz, l.paleta)
        });
        format!(
            "{:?} {:?} {:?} {:?} {fixas:?} {:?} {:?} {lateral:?} {:?} {:?} {} {}",
            self.atuacao,
            self.atual,
            self.fn_atual,
            self.avancada,
            self.luz,
            self.paleta,
            self.macros,
            self.perfis,
            self.perfil_atual,
            self.taxa_atual
        )
    }

    /// Quantidade de efeitos da área de luz, ou `None` se o teclado não tem essa área.
    pub fn efeitos_de(&self, area: u8) -> Option<u8> {
        match area {
            mag82::AREA_LATERAL => self.lateral.as_ref().map(|l| l.area.efeitos),
            _ => Some(self.efeitos),
        }
    }

    /// Retângulo de cada tecla, em unidades de tecla: (x, y, largura, altura).
    pub fn retangulos(&self) -> BTreeMap<Pos, (f64, f64, f64, f64)> {
        self.geo
            .iter()
            .filter(|(pos, _)| self.nomes.contains_key(pos)) // sem o knob e as posições vazias
            .map(|(pos, g)| {
                let r = if g.formato == mag82::FORMATO_ENTER {
                    // Enter ABNT2: ocupa duas linhas e é mais estreito embaixo
                    (g.x + 0.25, g.y, 1.25, 2.0)
                } else {
                    (g.x, g.y, g.largura, 1.0)
                };
                (*pos, r)
            })
            .collect()
    }
}
