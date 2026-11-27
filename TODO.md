# Pendências

O que falta fazer ou conferir. O que já foi decidido como "fica assim" está no fim.

## Recursos

- [ ] Macros: gravar digitando. Hoje os passos são montados um a um.
- [ ] Repositório APT/DNF assinado, para o próprio sistema trazer as versões novas sem o programa
      precisar baixar nada.

## Falta conferir

- [ ] Atraso do END e do RS e o modo de macro em que tocar de novo não faz nada: ver a
      [issue #19](https://github.com/julianol1berato/Avell-MAG/issues/19). O resto das teclas avançadas e das macros foi conferido digitando.
- [ ] A instalação da versão nova por `.deb` (via `pkexec apt-get`) nunca foi exercitada. Por
      `.rpm` e em `~/.local` foi, com versões publicadas.
- [ ] Seleção por arrasto e atalhos (Ctrl+A, Esc, F5) não têm teste automático: o autoteste chama
      as ações direto, sem simular mouse nem teclado.

## Projeto

- [ ] O spec do RPM baixa as dependências Rust durante a compilação; para os construtores oficiais
      do Fedora (sem rede) seria preciso empacotar as dependências (vendor).
- [ ] O teclado simulado guarda um perfil só: trocar de perfil muda o número do perfil ativo, mas
      as teclas e as luzes continuam as mesmas. No teclado de verdade cada perfil tem as suas.

## Limites conhecidos, sem solução à vista

- Calibração aberta por outro programa não é detectada: não há comando conhecido para perguntar
  ao teclado se ele está em modo de calibração. A que este programa abre e não fecha (queda de
  energia, `kill -9`) é desfeita na abertura seguinte.
- O configurador web no navegador não usa a trava do dispositivo deste programa; contra ele a
  proteção é só a conferência do pedido nas leituras. Não use os dois ao mesmo tempo.
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
