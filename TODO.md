# Pendências

## Projeto

- [ ] Páginas de manual (`man mag82`, `man mag82-gui`): é o único aviso que o lintian ainda dá no .deb.
- [ ] O spec do RPM baixa as dependências Rust durante a compilação; para os construtores oficiais
      do Fedora (sem rede) seria preciso empacotar as dependências (vendor).
- [ ] O teclado simulado guarda um perfil só: trocar de perfil muda o número do perfil ativo, mas
      as teclas e as luzes continuam as mesmas. No teclado de verdade cada perfil tem as suas.

## Atualização do programa

- [ ] A instalação da versão nova por `.deb` (via `pkexec apt-get`) nunca foi exercitada. Por
      `.rpm` e em `~/.local` foi, com versões publicadas.
- [ ] Repositório APT/DNF assinado, para o próprio sistema trazer as versões novas sem o programa
      precisar baixar nada.

## Encontrado durante a revisão, ainda não feito

- [ ] Seleção por arrasto e atalhos (Ctrl+A, Esc, F5) não têm teste automático: o autoteste chama
      as ações direto, sem simular mouse nem teclado.
- [ ] Luz lateral: os efeitos 6 a 10 não têm nome oficial e aparecem como "Efeito N".
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
