# Pendências

O que falta fazer ou conferir. O que já foi decidido como "fica assim" está no fim.

## Recursos

A ordem vem do levantamento em [docs/pesquisa-de-uso.md](docs/pesquisa-de-uso.md): o que usuários
de teclados magnéticos mais pedem e o programa ainda não tem.

- [ ] Exportar e importar a configuração em arquivo (atuação, remapeamento, atalhos Fn, teclas
      avançadas, macros, luz e cores fixas). É o pedido mais recorrente; também serve de backup,
      para copiar de um perfil para outro e para compartilhar.
- [ ] Predefinições de teclas avançadas: SOCD em A/D e W/S, dupla função no Caps Lock, num clique.
- [ ] Macros: gravar digitando. Hoje os passos são montados um a um.
- [ ] Troca automática de perfil conforme o programa ou jogo aberto. Precisa de algo rodando em
      segundo plano e, no Wayland, não há forma geral de saber qual janela está ativa; se for
      feito, opcional.
- [ ] Repositório APT/DNF assinado, para o próprio sistema trazer as versões novas sem o programa
      precisar baixar nada.

## Falta conferir

- [ ] Funções de mouse e atalhos prontos (Super + D, Super + E, Super + R, Ctrl + Shift + Esc)
      entraram no catálogo a partir do configurador oficial, e o teclado aceita gravá-los numa
      tecla. Falta conferir, apertando, o que cada um envia. O mesmo vale para as teclas que o
      MAG82 não tem (teclado numérico, F13 a F24, teclas de teclados japoneses e coreanos).
- [ ] Tecla com modificadores montada pelo usuário (ex.: Ctrl + C): a linha de comando aceita
      (`mag82 remapear f9 ctrl+c`); na interface só aparecem os quatro atalhos prontos. Falta um
      jeito de montar a combinação na tela, e conferir digitando uma combinação que não seja o
      Super + D de fábrica.

- [ ] Seleção por arrasto e atalhos (Ctrl+A, Esc, F5) não têm teste automático: o autoteste chama
      as ações direto, sem simular mouse nem teclado.

## Projeto

- [ ] O spec do RPM baixa as dependências Rust durante a compilação; para os construtores oficiais
      do Fedora (sem rede) seria preciso empacotar as dependências (vendor).
- [ ] O teclado simulado guarda um perfil só: trocar de perfil muda o número do perfil ativo, mas
      as teclas e as luzes continuam as mesmas. No teclado de verdade cada perfil tem as suas.

## Limites conhecidos, sem solução à vista

- Teclas avançadas e pares só aceitam teclas comuns: o teclado recusa multimídia, luzes, perfis e
  macros dentro de DKS, multiponto, dupla função, contínuo, ao soltar, SOCD e RS (firmware
  0.0.2.0). A tabela está no README e as respostas medidas, em
  `crates/mag82/src/avancadas-medidas.txt`. Uma tecla com modificador junto (Ctrl + D) o teclado
  aceita gravar, mas ao digitar envia só a tecla; o programa recusa
  ([issue #28](https://github.com/julianol1berato/Avell-MAG/issues/28)).

- Calibração aberta por outro programa não é detectada: não há comando conhecido para perguntar
  ao teclado se ele está em modo de calibração. A que este programa abre e não fecha (queda de
  energia, `kill -9`) é desfeita na abertura seguinte.
- O configurador web no navegador não usa a trava do dispositivo deste programa; contra ele a
  proteção é só a conferência do pedido nas leituras. Não use os dois ao mesmo tempo.
- O teclado guardou todos os tempos e atrasos testados nas teclas avançadas, de 0 a 65535 ms, e respeita
  valores acima dos limites do programa: um "ao soltar" com 500 ms esperou 500 ms, e uma dupla
  função com 2000 ms separou toque de segurar em 2 segundos (firmware 0.0.2.0).
- O atraso do RS não tem efeito que se consiga medir (firmware 0.0.2.0). Com 0, 200 e 500 ms o
  teclado troca de tecla no instante em que uma passa a outra em profundidade (0 a 2 ms depois do
  cruzamento, nos três casos), e só envia as duas juntas quando ambas estão no fundo. O teclado
  guarda o valor, a linha de comando deixa gravar, e a interface sempre grava 0. Medido lendo o
  curso das duas teclas cerca de mil vezes por segundo junto com as teclas enviadas
  ([issue #19](https://github.com/julianol1berato/Avell-MAG/issues/19)).
- Os limites de tempo (10 a 1000 ms) e de atraso (0 a 50 ms) das teclas avançadas são uma faixa
  prudente, não um limite lido do teclado.

## Decidido: fica assim

- Efeitos 6 a 10 da luz lateral aparecem como "Efeito N": não há nome oficial para eles.
- Knob: girar envia volume + e volume -, apertar envia mudo, e não dá para trocar. Gravar uma
  tecla na posição dele não muda o que ele envia (conferido com o teclado), e o configurador
  oficial também não tem ajuste.
- Fn+W e Fn+Ctrl vêm de fábrica com funções sem efeito conhecido: não enviam tecla, não acendem
  nada e não mudam nada que se consiga ler do teclado. Aparecem como "Função de fábrica sem
  efeito conhecido".
