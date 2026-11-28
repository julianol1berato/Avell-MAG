//! Exportar e importar a configuração do perfil ativo: o grupo "Cópia da configuração" da
//! página Dispositivo. A importação mostra o que muda antes de gravar e pode ser desfeita.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::gio;
use mag82::copia::{Copia, Resumo, EXTENSAO};

use crate::dados::Dados;
use crate::ui::Ui;

fn filtros() -> gio::ListStore {
    let lista = gio::ListStore::new::<gtk::FileFilter>();
    for (nome, padrao) in [("Configuração do MAG82", format!("*.{EXTENSAO}")), ("Todos os arquivos", "*".to_string())] {
        let filtro = gtk::FileFilter::new();
        filtro.set_name(Some(nome));
        filtro.add_pattern(&padrao);
        lista.append(&filtro);
    }
    lista
}

/// "Perfil 1" vira `mag82-perfil-1.mag82`: só letras sem acento, números e hífens.
pub fn nome_do_arquivo(perfil: &str) -> String {
    let apelido = mag82::apelido(perfil);
    format!("mag82-{}.{EXTENSAO}", if apelido.is_empty() { "configuracao" } else { &apelido })
}

fn nome_curto(caminho: &Path) -> String {
    caminho.file_name().map_or_else(|| caminho.display().to_string(), |n| n.to_string_lossy().into_owned())
}

impl Ui {
    fn perfil_ativo(&self) -> String {
        let dados = self.d.borrow();
        dados.as_ref().and_then(|d| d.perfis.get(d.perfil_atual as usize).cloned()).unwrap_or_else(|| "perfil ativo".into())
    }

    pub fn escolher_exportacao(self: &Rc<Self>) {
        let dialogo = gtk::FileDialog::builder()
            .title("Salvar a configuração")
            .initial_name(nome_do_arquivo(&self.perfil_ativo()))
            .filters(&filtros())
            .modal(true)
            .build();
        let ui = self.clone();
        dialogo.save(Some(&self.janela), gio::Cancellable::NONE, move |escolha| {
            if let Some(caminho) = escolha.ok().and_then(|arquivo| arquivo.path()) {
                ui.exportar_para(caminho);
            }
        });
    }

    pub fn exportar_para(self: &Rc<Self>, caminho: PathBuf) {
        self.pedir_bruto(
            true,
            |t| Copia::ler(t).map(|copia| copia.texto()),
            move |ui, lido| {
                let salvo = lido.map_err(|e| e.to_string()).and_then(|texto| std::fs::write(&caminho, texto).map_err(|e| e.to_string()));
                match salvo {
                    Ok(()) => ui.avisar(&format!("Configuração salva em {}", nome_curto(&caminho))),
                    Err(erro) => ui.importacao_falhou("Não foi possível salvar a configuração", &erro, None),
                }
            },
        );
    }

    pub fn escolher_importacao(self: &Rc<Self>) {
        let dialogo = gtk::FileDialog::builder().title("Abrir uma configuração").filters(&filtros()).modal(true).build();
        let ui = self.clone();
        dialogo.open(Some(&self.janela), gio::Cancellable::NONE, move |escolha| {
            if let Some(caminho) = escolha.ok().and_then(|arquivo| arquivo.path()) {
                ui.importar_de(&caminho);
            }
        });
    }

    /// Lê o arquivo, compara com o teclado e pergunta antes de gravar.
    pub fn importar_de(self: &Rc<Self>, caminho: &Path) {
        let copia = std::fs::read_to_string(caminho).map_err(|e| e.to_string()).and_then(|t| Copia::de_texto(&t).map_err(|e| e.to_string()));
        let copia = match copia {
            Ok(copia) => copia,
            Err(erro) => return self.importacao_falhou("Não foi possível ler o arquivo", &erro, None),
        };
        let arquivo = nome_curto(caminho);
        self.pedir_bruto(
            true,
            move |t| copia.comparar(t).map(|muda| (copia, muda)),
            move |ui, comparado| match comparado {
                Err(erro) => ui.importacao_falhou("Este arquivo não pode ser importado", &erro.to_string(), None),
                Ok((_, muda)) if muda.vazio() => ui.avisar("O teclado já está igual ao arquivo: nada a gravar"),
                Ok((copia, muda)) => ui.confirmar_importacao(copia, &muda, &arquivo),
            },
        );
    }

    fn confirmar_importacao(self: &Rc<Self>, copia: Copia, muda: &Resumo, arquivo: &str) {
        let itens: Vec<String> = muda.frases().iter().map(|frase| format!("•  {frase}")).collect();
        let dialogo = adw::AlertDialog::builder()
            .heading(format!("Importar para o perfil “{}”?", self.perfil_ativo()))
            .body(format!(
                "O arquivo {arquivo} muda no teclado:\n\n{}\n\nA configuração de agora fica guardada, e a importação pode ser desfeita.",
                itens.join("\n")
            ))
            .close_response("cancelar")
            .default_response("importar")
            .build();
        dialogo.add_responses(&[("cancelar", "Cancelar"), ("importar", "Importar")]);
        dialogo.set_response_appearance("importar", adw::ResponseAppearance::Suggested);
        let ui = self.clone();
        let copia = std::cell::RefCell::new(Some(copia));
        dialogo.connect_response(Some("importar"), move |_, _| {
            if let Some(copia) = copia.borrow_mut().take() {
                ui.importar(copia);
            }
        });
        *self.disp.dialogo_importar.borrow_mut() = Some(dialogo.clone());
        dialogo.present(Some(&self.janela));
    }

    /// Guarda a configuração atual, grava a do arquivo e relê o teclado.
    fn importar(self: &Rc<Self>, copia: Copia) {
        type Feito = Result<(Dados, PathBuf), (String, Option<PathBuf>)>;
        self.avisar_demorado("Importando a configuração…");
        self.pedir_bruto(
            true,
            move |t| -> mag82::Resultado<Feito> {
                let guardada = match Copia::guardar_a_atual(t) {
                    Ok(caminho) => caminho,
                    Err(erro) => return Ok(Err((erro.to_string(), None))),
                };
                Ok(match copia.aplicar(t).and_then(|_| Dados::ler(t)) {
                    Ok(dados) => Ok((dados, guardada)),
                    Err(erro) => Err((erro.to_string(), Some(guardada))),
                })
            },
            |ui, feito| match feito {
                Ok(Ok((dados, guardada))) => {
                    ui.carregar(dados, false);
                    let ui2 = ui.clone();
                    ui.avisar_com_botao("Configuração importada", "Desfazer", move || ui2.importar_de(&guardada));
                }
                Ok(Err((erro, guardada))) => {
                    ui.limpar_aviso();
                    ui.conectar(false);
                    ui.importacao_falhou("A importação não terminou", &erro, guardada);
                }
                Err(erro) => {
                    ui.limpar_aviso();
                    ui.avisar_erro(&erro.to_string());
                }
            },
        );
    }

    /// `guardada` é a configuração de antes de uma importação que parou no meio: o diálogo
    /// oferece voltar a ela.
    fn importacao_falhou(self: &Rc<Self>, titulo: &str, erro: &str, guardada: Option<PathBuf>) {
        let corpo = match &guardada {
            Some(_) => format!("{erro}\n\nParte do arquivo pode ter sido gravada. A configuração de antes está guardada."),
            None => erro.to_string(),
        };
        let dialogo = adw::AlertDialog::builder().heading(titulo).body(corpo).close_response("fechar").build();
        dialogo.add_response("fechar", "Fechar");
        if let Some(guardada) = guardada {
            dialogo.add_response("voltar", "Voltar à configuração de antes");
            dialogo.set_response_appearance("voltar", adw::ResponseAppearance::Suggested);
            let ui = self.clone();
            dialogo.connect_response(Some("voltar"), move |_, _| ui.importar_de(&guardada));
        }
        *self.disp.dialogo_importar.borrow_mut() = Some(dialogo.clone());
        dialogo.present(Some(&self.janela));
    }
}

#[cfg(test)]
mod testes {
    #[test]
    fn nome_do_arquivo_sem_acentos() {
        assert_eq!(super::nome_do_arquivo("Perfil 1"), "mag82-perfil-1.mag82");
        assert_eq!(super::nome_do_arquivo("Competição / FPS"), "mag82-competicao-fps.mag82");
        assert_eq!(super::nome_do_arquivo("配置一"), "mag82-configuracao.mag82");
    }
}
