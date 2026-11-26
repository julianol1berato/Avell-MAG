//! Thread única que conversa com o teclado. As tarefas rodam na ordem em que chegam e a
//! resposta volta por um canal que a thread da interface aguarda sem travar.

use std::sync::mpsc;

use futures_channel::oneshot;
use mag82::Teclado;

type Tarefa = Box<dyn FnOnce(&mut Option<Teclado>) + Send>;

pub struct Fila {
    tx: mpsc::Sender<Tarefa>,
}

impl Fila {
    pub fn nova() -> Self {
        let (tx, rx) = mpsc::channel::<Tarefa>();
        std::thread::Builder::new()
            .name("teclado".into())
            .spawn(move || {
                let mut teclado = None;
                for tarefa in rx {
                    tarefa(&mut teclado);
                }
            })
            .expect("não foi possível criar a thread do teclado");
        Self { tx }
    }

    /// Agenda `f` na thread do teclado e devolve o canal por onde o resultado chega.
    pub fn enviar<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Option<Teclado>) -> T + Send + 'static,
    ) -> oneshot::Receiver<T> {
        let (resposta, rx) = oneshot::channel();
        let _ = self.tx.send(Box::new(move |t| {
            let _ = resposta.send(f(t));
        }));
        rx
    }
}
