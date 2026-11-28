# Contribuindo

Obrigado por querer ajudar. Este guia explica o caminho de uma contribuição: do fork até a mudança
entrar numa versão.

Mantenedor: Juliano Liberato ([9level.com.br](https://9level.com.br) ·
[github.com/julianol1berato](https://github.com/julianol1berato)).

## Antes de começar

- **Problema ou ideia:** abra uma [issue](https://github.com/julianol1berato/Avell-MAG/issues).
  Para problemas, informe a distribuição, a versão do programa (`mag82 --version`), o firmware
  (`mag82 dispositivo`) e o passo a passo. Se envolver calibração, anexe
  `~/.local/state/mag82/calibracao.log`.
- **Mudança grande:** combine antes numa issue, para não gastar tempo em algo que não vai entrar.
- **Mudança pequena** (correção, texto, um item do [TODO.md](TODO.md)): pode mandar direto o
  pull request.
- **Outro teclado parecido:** mande a saída de `lsusb` numa issue. O programa hoje só procura o
  MAG82 (`1ca6:3920`).

## Passo a passo

1. **Faça um fork** do repositório pelo botão *Fork* do GitHub.
2. **Clone o seu fork** e crie um ramo a partir de `main`:

   ```sh
   git clone https://github.com/SEU-USUARIO/Avell-MAG.git
   cd Avell-MAG
   git checkout -b corrige-brilho-lateral
   ```

3. **Prepare o ambiente:**

   ```sh
   sudo apt install cargo libgtk-4-dev libadwaita-1-dev     # Ubuntu/Debian
   sudo dnf install cargo gtk4-devel libadwaita-devel       # Fedora
   ```

4. **Faça a mudança** e confira no seu computador. Os testes de desenvolvimento são sempre feitos
   na máquina de quem fez o código, antes de enviar; um comando roda tudo:

   ```sh
   scripts/verificar.sh             # compila, testa, exercita a interface e gera os pacotes
   scripts/verificar.sh --teclado   # o mesmo, mais o autoteste no teclado de verdade
   ```

   Ele compila, roda os testes unitários e de integração, exercita todas as telas contra o
   teclado simulado e gera os pacotes da versão (`.deb`, `.tar.gz` e, onde houver `rpmbuild`,
   `.rpm`). No fim diz o que foi feito; cole esse resumo no pull request. A esteira de integração
   repete as conferências depois, numa máquina limpa: ela é a segunda opinião, não o lugar de
   descobrir que algo quebrou.

   Toda mudança de comportamento vem com o seu teste: unitário, junto do código, para a regra
   nova; e uma conferência no autoteste da interface (`crates/mag82-gui/src/teste.rs` e
   `scripts/teste-interface.sh`) quando a mudança aparece na tela.

   As partes, para rodar em separado:

   ```sh
   cargo test                   # precisa passar; não usa o teclado
   scripts/teste-interface.sh   # exercita todas as telas contra o teclado simulado
   packaging/deb.sh             # se mexeu em empacotamento
   scripts/teste-fumaca.sh      # confere um pacote instalado, sem teclado
   ```

   Você não precisa do teclado para contribuir: `MAG82_SIMULADO=1` troca o teclado de verdade
   por um simulado, na interface e na linha de comando. Se a sua mudança envia um comando novo
   ao teclado, ensine o simulado a respondê-lo (`crates/mag82/src/simulado.rs`).

   Se você tem o teclado, rode o mesmo autoteste nele, sem a variável do simulado:

   ```sh
   cargo build --release -p mag82-gui --features autoteste --target-dir target/autoteste
   MAG82_TESTE=/tmp/tela target/autoteste/release/mag82-gui
   ```

   Ele **grava no teclado de verdade** e restaura no fim, mas desfaz um SOCD entre A e D e uma cor
   fixa na tecla W, se existirem. Também grava na tecla Insert cada função das listas do modo
   Avançada, em cada tipo (algumas centenas de gravações), e remove no fim. O autoteste não vai nos pacotes.

5. **Faça commits pequenos**, cada um com uma mudança só e uma mensagem que diga o que mudou e por
   quê.
6. **Envie o ramo** para o seu fork e **abra o pull request** contra `main`:

   ```sh
   git push -u origin corrige-brilho-lateral
   ```

7. **Mantenha o ramo atualizado** se `main` andar enquanto o pull request está aberto:

   ```sh
   git remote add upstream https://github.com/julianol1berato/Avell-MAG.git
   git fetch upstream
   git rebase upstream/main
   git push --force-with-lease
   ```

## O que explicar no pull request

O formulário do pull request já traz estes itens:

- **O que muda**, em uma ou duas frases.
- **Por quê:** o problema que resolve, com o número da issue, se houver.
- **Como você testou:** os comandos que rodou e o resultado.
- **Teclado de verdade:** se testou no hardware, diga o firmware e a distribuição. Se não tem o
  teclado, diga isso.
- **Grava no teclado?** Se a mudança envia algum comando novo ou altera uma gravação, descreva
  qual e como conferiu o efeito.
- **Telas:** para mudança visual, uma captura de antes e depois.

Pull request sem explicação de como foi testado volta para o autor.

## A esteira de integração (CI)

Todo pull request e todo envio para `main` dispara o fluxo em
[`.github/workflows/ci.yml`](.github/workflows/ci.yml), com três etapas:

| Etapa | Onde roda | O que confere |
|---|---|---|
| **Testes** | Ubuntu 24.04 | Compila, roda os testes unitários e de integração (e falha se nenhum rodar), confere o que as teclas avançadas aceitam contra as respostas medidas no teclado e exercita a interface inteira contra o teclado simulado |
| **Pacote .deb** | Ubuntu 24.04 | Gera o `.deb`, instala com `apt` e roda o teste de fumaça como usuário comum |
| **Pacote .rpm** | Fedora | Gera o `.rpm` (compila e testa de novo), instala com `dnf` e roda o teste de fumaça como usuário comum |

O teste de fumaça confere que o programa roda sem root, que a linha de comando responde, que a
janela abre sem teclado e com o teclado simulado e que nada é criado fora da pasta do usuário.
Os pacotes gerados ficam anexados à execução, para quem quiser baixar e testar.

A esteira usa o teclado simulado, que repete o que o teclado de verdade respondeu um dia; ela não
substitui o teste no hardware quando a mudança envia um comando novo.

No primeiro pull request de alguém, o GitHub só roda a esteira depois que o mantenedor aprova a
execução.

## Revisão e merge

- O pull request só entra com a **esteira verde** e a revisão do mantenedor.
- O merge é feito pelo mantenedor, como **merge comum**: os commits do pull request entram em
  `main` como estão, junto com um commit de merge que leva o título do pull request. Por isso cada
  commit precisa ter uma mensagem que se explique sozinha, e o título do pull request precisa
  descrever bem a mudança.
- Pedidos de ajuste são respondidos com novos commits no mesmo ramo; não precisa abrir outro
  pull request.

## Versões (CD)

As versões são publicadas pelo mantenedor:

1. A versão é atualizada no `Cargo.toml` e em `data/io.github.mag82linux.metainfo.xml`.
2. Uma tag `vX.Y.Z` é criada em `main`.
3. A mesma esteira roda de novo e, se tudo passar, cria a versão no GitHub com o `.deb` e o `.rpm`
   anexados.

## Regras para mexer no protocolo

O teclado aceita comandos que mudam tudo de uma vez, sem confirmação. Por isso:

1. **Nunca envie um comando cujo significado você não conhece.** Nada de varrer subcomandos para
   ver o que responde.
2. Atualização de firmware e restauração de fábrica ficam bloqueadas na biblioteca. Não remova o
   bloqueio.
3. Toda gravação deve ser conferida lendo de volta, e a interface só mostra o que o teclado
   confirmou.
4. **Só ofereça o que o teclado aceita.** Nas teclas avançadas (DKS, multiponto, dupla função,
   contínuo, ao soltar, SOCD e RS) ele recusa tudo o que não é tecla comum. O que ele respondeu a
   cada função está em
   [`crates/mag82/src/avancadas-medidas.txt`](crates/mag82/src/avancadas-medidas.txt), e a esteira
   confere três coisas contra essa tabela: a regra do programa (`serve_em_avancada`), que toda
   função do catálogo foi medida, e que cada função oferecida nas listas da interface é de fato
   gravada em cada tipo. Função nova no catálogo ou tipo novo de tecla avançada precisa ser medido
   no teclado e registrado na tabela; sem isso a esteira reprova. Não edite uma linha da tabela
   sem medir de novo.

## Estilo do código

- O código e os comentários são em português, como o resto do projeto.
- Comentário explica o porquê, não repete o que a linha faz.
- A interface é para quem não é técnico: mensagem de erro diz o que fazer, sem mandar a pessoa
  para o terminal.
