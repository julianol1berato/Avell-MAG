# O que usuários reclamam nos programas de teclado magnético, e onde estamos

Levantamento de novembro de 2026 sobre o que jogadores e usuários pedem ou reclamam nos programas
de configuração de teclados magnéticos (Wootility, Razer Synapse, Keychron Launcher, drivers web
da Epomaker, Iqunix, DrunkDeer e parecidos), cruzado com o que o `mag82` faz hoje. Serve para
decidir o que entra no [TODO.md](../TODO.md).

## Limites deste levantamento

- **O Reddit ficou quase inacessível na busca.** Dele só entraram trechos que apareceram nos
  resultados, não tópicos lidos por inteiro. O peso de "voz de usuário" vem da pesquisa da própria
  Wooting (6.378 respostas), dos fóruns oficiais da Razer e do Manjaro, de GitHub e de reviews.
- Vários itens dependem de uma fonte forte só (a pesquisa da Wooting). A ordem de importância
  abaixo é uma leitura, não uma contagem.
- Buscas em português só voltaram páginas de fabricante e lojas.
- Não se achou fonte nenhuma para: ícones nas teclas remapeadas, visualização do curso ao vivo e
  calibração. Essas melhorias do programa vieram de uso próprio, não desta pesquisa.
- As fontes marcadas com *(trecho)* não foram abertas: só o trecho do resultado de busca foi lido.

## O que é reclamação válida, e como estamos

| # | Reclamação ou pedido | Força nas fontes | No `mag82` hoje |
|---|---|---|---|
| 1 | Exportar e importar a configuração em arquivo; backup | várias fontes | **Tem**: o perfil inteiro num arquivo de texto, com conferência do que muda antes de gravar e desfazer |
| 2 | Atualização de firmware apaga a configuração | algumas | Não se aplica: o programa não atualiza firmware (bloqueado de propósito). Um backup (item 1) cobriria quem atualiza por fora |
| 3 | Trocar de perfil sozinho conforme o programa ou jogo aberto | forte | **Falta** |
| 4 | Detecção da janela ativa não funciona no Wayland | algumas | Obstáculo para o item 3 |
| 5 | Macros gravadas no teclado, sem programa rodando | forte | **Tem**: as 8 macros ficam no teclado |
| 6 | Gravar a macro digitando, em vez de montar passo a passo | indireta | **Falta** (já estava no TODO) |
| 7 | A tela mostra um estado diferente do que o teclado está fazendo | várias | **Melhora neste PR**: a tela é conferida ao voltar para a janela. Antes, só perfil trocado por fora era percebido |
| 8 | Alterações não ficam gravadas; sincronização trava | forte na Razer, isolada nas outras | **Tem**: toda gravação é lida de volta, e a tela só mostra o que o teclado confirmou |
| 9 | O programa não reconhece o teclado | forte | **Tem em parte**: reconecta sozinho e explica a falta de permissão. Só procura o MAG82 |
| 10 | No Linux, a permissão do hidraw (udev) é a barreira de entrada | várias | **Tem**: os pacotes instalam a regra udev e a mensagem de erro diz o que fazer |
| 11 | Fabricante não dá suporte a Linux | forte | É a razão de o programa existir |
| 12 | Driver web só funciona em navegador Chromium | várias | **Tem**: programa nativo, com linha de comando |
| 13 | Os recursos existem, mas ninguém acha nem entende | forte | **Tem em parte**: cada tipo de tecla avançada tem explicação na tela, e a atuação tem predefinições. **Faltam** predefinições para as teclas avançadas (por exemplo, SOCD em A/D num clique) |
| 14 | Interface travada, mal traduzida, com enfeites inúteis | várias (reviews) | Interface nativa, em português, sem enfeite. Sem tradução para outros idiomas |
| 15 | Mais controle de luz por tecla e prévia na tela | forte | **Tem**: cor fixa por tecla e por LED da lateral, com prévia |
| 16 | Biblioteca ou compartilhamento de perfis | algumas | **Tem em parte**: o arquivo exportado pode ser passado adiante; não há biblioteca |
| 17 | Perfis completos no teclado, troca por atalho, e ver se o SOCD está ligado | algumas | **Tem**: 4 perfis no teclado, função de trocar de perfil para pôr numa tecla, e marca nas teclas que estão num par |
| 18 | Copiar ajustes de um perfil para outro | fraca | **Tem**: exportar, trocar de perfil e importar |
| 19 | Serviço em segundo plano e dependência de site | fraca, opiniões divididas | **Tem**: nada roda em segundo plano e nada depende de site. Vale manter assim se o item 3 for feito |
| 20 | Atualização de firmware arriscada | algumas | Não se aplica: bloqueada na biblioteca |

## O que mais aparece

1. **Exportar, importar e ter backup da configuração** (itens 1, 2 e 16).
2. **Macros gravadas no teclado** (item 5). Já temos.
3. **Troca automática de perfil por programa ou jogo** (itens 3 e 4).
4. **Confiança na ligação entre o programa e o teclado**: detectar, gravar de fato e mostrar o
   estado real (itens 7, 8 e 9).
5. **Recursos fáceis de achar e entender**, com predefinições (item 13).

## O que falta, em ordem sugerida

1. ~~**Exportar e importar a configuração em arquivo.**~~ Feito: o arquivo leva atuação,
   remapeamento, atalhos Fn, teclas avançadas, macros, luz e cores fixas, porque a reclamação
   recorrente é justamente a exportação que deixa partes de fora. Sendo legível, também serve para
   compartilhar (item 16) e para copiar de um perfil para outro (item 18).
2. **Predefinições de teclas avançadas**: SOCD em A/D e W/S, dupla função no Caps Lock, num clique.
3. **Gravar macro digitando.**
4. **Troca automática de perfil por programa.** É o mais pedido depois do backup, mas o mais
   caro: precisa de algo rodando em segundo plano e, no Wayland, não há forma geral de saber qual
   janela está ativa. Fica por último, e opcional.

## Fontes

Pesquisa da Wooting e notas de versão:

- <https://wooting.io/post/wootility-v5-what-you-told-us>
- <https://wooting.io/post/wootility-5-4-app-linking-and-advanced-keys-presets>
- <https://wooting.io/post/wootility-update-introducing-the-background-service>
- <https://wooting.io/wootility/changelogs/5.4.0-beta.0> *(trecho)*
- <https://profiles.wooting.io>

Reviews:

- <https://www.notebookcheck.net/Iqunix-Hall-effect-gaming-keyboards-finally-surpass-Wooting-New-firmware-update-adds-macros-to-EZ63.1088061.0.html>
- <https://www.notebookcheck.net/Iqunix-EZ63-EZ60-Hall-effect-gaming-keyboards-get-game-changing-update-with-much-requested-on-board-profiles-SOCD-toggle-shortcut.969199.0.html>
- <https://www.notebookcheck.net/Keychron-s-latest-firmware-update-cemented-its-HE-series-as-my-favourite-gaming-keyboards-for-all-round-use.1082046.0.html>
- <https://www.notebookcheck.net/Wooting-updates-80HE-gaming-keyboard-with-a-host-of-fancy-features-in-its-stand-out-Wootility-web-configurator.1104410.0.html>
- <https://www.kitguru.net/peripherals/carlosribeiro/epomaker-he75-v2-review/>
- <https://prosettings.net/reviews/epomaker-he75-v2-tmr/>
- <https://www.pcgamer.com/drunkdeer-a75-gaming-keyboard-review/> *(trecho)*
- <https://rtings.com/keyboard/reviews/nuphy/air75-he> *(trecho)*
- <https://dexerto.com/tech/counter-strike-2-players-report-being-wrongly-kicked-for-input-automation-after-socd-update-2868830>

Fóruns e comunidades:

- <https://insider.razer.com/razer-support-45/huntsman-v3-pro-tenkeyless-cannot-sync-profiles-properly-with-synapse-4-de-iso-73120>
- <https://insider.razer.com/razer-support-45/huntsman-v3-pro-mini-rapid-trigger-and-actuation-point-91647>
- <https://insider.razer.com/razer-support-45/huntsman-v3-tkl-8khz-onboard-profile-resets-when-switching-pcs-or-os-is-this-intentional-91912>
- <https://insider.razer.com/razer-support-45/macros-issues-for-razer-snyapse-4-89812?postid=288302>
- <https://insider.razer.com/razer-synapse-4-55/synapse-4-on-linux-78035/index2.html>
- <https://forum.manjaro.org/t/keychron-launcher-does-not-work/183350>
- <https://discourse.nixos.org/t/cant-get-wootility-to-detect-keyboard/51213> *(trecho)*
- <https://aur.archlinux.org/packages/wootility-appimage> *(trecho)*
- <https://connect.mozilla.org/t5/ideas/webhid-api-support-please/idc-p/52388/highlight/true> *(trecho)*

Projetos e fabricantes:

- <https://github.com/ShayBox/WootingProfileSwitcher>
- <https://support.mchose.store/hc/en-us/articles/49273621979924-Why-Is-My-MCHOSE-Keyboard-Not-Being-Detected> *(trecho)*
- <https://en.akkogear.com/faq/my-keyboard-cannot-be-detected-by-akko-cloud-driver> *(trecho)*
- <https://drunkdeer.com/blogs/news/Web-Driver-Update-Important-Changes> *(trecho)*
- <https://attackshark.com/blogs/knowledges/post-update-recalibrate-magnetic-keys> *(trecho)*
