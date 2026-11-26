#!/usr/bin/env bash
# Teste de fumaça do programa INSTALADO (pacote .deb/.rpm ou ~/.local), sem teclado conectado:
# confere que tudo roda como usuário comum, que a janela abre e que o registro vai para a pasta
# do usuário. É o que a integração contínua roda no Ubuntu e no Fedora.
# Uso: scripts/teste-fumaca.sh        (precisa de xvfb-run e dbus-run-session)
set -euo pipefail
falhou() { echo "FALHOU: $*"; exit 1; }

[ "$(id -u)" -ne 0 ] || falhou "este teste precisa rodar como usuário comum (está como root)"
echo "usuário: $(id -un) (uid $(id -u))"

for bin in mag82 mag82-gui; do command -v "$bin" >/dev/null || falhou "$bin não está no PATH"; done
mag82 --version
# a saída vai primeiro para uma variável: com `pipefail`, um `grep -q` que fecha o cano cedo
# faria o próprio mag82 terminar com SIGPIPE e o teste falhar de vez em quando
ajuda="$(mag82 --help)"
grep -q calibrar <<<"$ajuda" || falhou "ajuda da linha de comando incompleta"

# sem teclado: erro claro e código de saída 1, não uma queda
saida="$(mag82 info 2>&1)" && falhou "mag82 info deveria falhar sem teclado" || true
grep -q "teclado não encontrado" <<<"$saida" || falhou "mensagem inesperada sem teclado: $saida"
saida="$(mag82 bruto 020600 2>&1)" || true
grep -q -e "bloqueado" -e "teclado não encontrado" <<<"$saida" || falhou "comando de calibração bruto não foi recusado: $saida"

# o registro fica na pasta do usuário
registro="$(mag82 registro)"
grep -q "^arquivo: $HOME/.local/state/mag82/calibracao.log" <<<"$registro" \
    || falhou "registro fora da pasta do usuário: $registro"

# arquivos do pacote (só confere os que existirem: na instalação em ~/.local os caminhos mudam)
for f in /usr/share/applications/io.github.mag82linux.desktop /usr/lib/udev/rules.d/70-mag82.rules; do
    if [ -e "$f" ]; then echo "ok: $f"; fi
done
if command -v desktop-file-validate >/dev/null && [ -e /usr/share/applications/io.github.mag82linux.desktop ]; then
    desktop-file-validate /usr/share/applications/io.github.mag82linux.desktop
fi

# a janela abre sem teclado, mostra a tela de espera, fotografa as telas e fecha sozinha
tmp="$(mktemp -d)"
GSK_RENDERER=cairo MAG82_TESTE="$tmp/tela" timeout 120 dbus-run-session -- xvfb-run -a mag82-gui > "$tmp/saida.txt" 2>&1 \
    || { cat "$tmp/saida.txt"; falhou "a interface gráfica não rodou até o fim"; }
cat "$tmp/saida.txt"
grep -q "sem teclado" "$tmp/saida.txt" || falhou "a interface não chegou à tela de espera"
ls "$tmp"/tela-*.png >/dev/null 2>&1 || falhou "a interface não desenhou as telas"
echo "telas geradas: $(ls "$tmp"/tela-*.png | wc -l)"
[ -z "$(find "$HOME/.local" "$HOME/.config" -user 0 2>/dev/null | head -1)" ] || falhou "o programa deixou arquivos de root na pasta do usuário"
echo "TESTE DE FUMAÇA OK"
