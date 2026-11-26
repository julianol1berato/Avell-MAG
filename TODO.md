# Pendências

## Projeto

- [ ] Páginas de manual (`man mag82`, `man mag82-gui`): é o único aviso que o lintian ainda dá no .deb.
- [ ] O spec do RPM baixa as dependências Rust durante a compilação; para os construtores oficiais
      do Fedora (sem rede) seria preciso empacotar as dependências (vendor).
- [ ] **Tirar o autoteste do binário de produção.** `crates/mag82-gui/src/teste.rs` roda com
      `MAG82_TESTE=prefixo` e grava no teclado de verdade (restaura no fim, mas desfaz um SOCD que
      já exista entre A e D e a cor fixa de W). Mover para um binário ou feature de teste e criar um
      teclado falso na biblioteca (transporte abstrato) para testar GUI e CLI sem hardware.

## Atualização do programa

- [ ] A instalação da versão nova por `.deb` e `.rpm` (via `pkexec`) nunca foi exercitada de ponta a
      ponta; só o caminho de `~/.local` foi testado com uma versão simulada.
- [ ] Repositório APT/DNF assinado, para o próprio sistema trazer as versões novas sem o programa
      precisar baixar nada.

## Encontrado durante a revisão, ainda não feito

- [ ] Seleção por arrasto e atalhos (Ctrl+A, Esc, F5) não têm teste automático: o autoteste não
      simula mouse nem teclado. Só a conta de quais teclas o retângulo toca tem teste unitário.
- [ ] Luz lateral: os efeitos 6 a 10 não têm nome oficial e aparecem como "Efeito N"; a faixa de
      LEDs é desenhada em linha reta, sem saber como eles se distribuem pelas laterais do teclado.
- [ ] Recursos do configurador oficial que ainda não existem aqui: macros e as teclas avançadas
      MPT, MT, TGL, END e RS (o DKS e o SOCD já existem).
- [ ] Knob: girar envia volume + e volume -, apertar envia mudo. Não dá para trocar pelo layout:
      o teclado informa uma posição do tipo knob (linha 0, coluna 14), mas gravar uma tecla nela
      não muda o que o knob envia (conferido com o teclado). O configurador oficial também não
      tem ajuste para ele.
- [ ] Atalhos Fn: duas funções de fábrica sem efeito conhecido (Fn+W = 0xf20b e Fn+Ctrl = 0xff01).
      Conferido com o teclado: não enviam tecla ao computador, não acendem nada e não mudam nada
      que se consiga ler dele (perfil, taxa, luzes, paletas, teclas, atalhos, SOCD, DKS).
- [ ] Calibração interrompida à força (kill -9, queda de energia): o programa deixa um marcador em
      `~/.local/state/mag82/calibracao-em-andamento` e, ao abrir de novo, tira o teclado do modo de
      calibração e avisa. O que falta: não há comando conhecido para perguntar ao próprio teclado
      se ele está nesse modo, então o caso "calibração aberta por outro programa" não é detectado.
- [ ] O configurador web no navegador não usa a nossa trava do dispositivo; contra ele a proteção
      é só a conferência do pedido nas leituras.
