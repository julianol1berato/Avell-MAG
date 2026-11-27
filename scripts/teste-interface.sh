#!/usr/bin/env bash
# Autoteste da interface contra o teclado simulado: não precisa do teclado e não grava em nenhum.
# Compila a interface com o recurso `autoteste`, roda todas as telas e confere os resultados.
# É o que a integração contínua roda a cada mudança.
# Uso: scripts/teste-interface.sh [pasta-das-capturas]     (com xvfb-run e dbus-run-session, roda sem janela)
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RAIZ"
falhou() { echo "FALHOU: $*"; exit 1; }

cargo build --release --locked -p mag82-gui --features autoteste --target-dir target/autoteste
saida="${1:-$(mktemp -d)}"
mkdir -p "$saida"
# pasta de usuário própria: o teste não lê nem escreve a configuração de quem o roda
export HOME="$saida/home" XDG_CONFIG_HOME="$saida/home/.config" XDG_STATE_HOME="$saida/home/.local/state" \
    XDG_CACHE_HOME="$saida/home/.cache" XDG_DATA_HOME="$saida/home/.local/share"
mkdir -p "$HOME"
# com xvfb a janela não aparece para ninguém; sem ele, o teste usa a tela de quem o roda
if command -v xvfb-run >/dev/null && command -v dbus-run-session >/dev/null; then
    tela=(dbus-run-session -- xvfb-run -a -s "-screen 0 1600x1100x24" env GSK_RENDERER=cairo GTK_A11Y=none)
else
    echo "sem xvfb-run: a janela do teste vai aparecer na sua tela por cerca de um minuto"
    tela=(env)
fi
MAG82_SIMULADO=1 MAG82_TESTE="$saida/tela" timeout 300 "${tela[@]}" \
    target/autoteste/release/mag82-gui > "$saida/saida.txt" 2>&1 \
    || { cat "$saida/saida.txt"; falhou "o autoteste não rodou até o fim"; }
grep -v -e WARNING -e '^$' "$saida/saida.txt" || true

confere() { grep -q -F -- "$1" "$saida/saida.txt" || falhou "esperava no resultado: $1"; }
confere "atuação W: rt=true ponto=1500"
confere "lote: ponto W=1200 A=1200  rt_press W=500 A=300"
confere "zonas recusadas: topo W=200 A=200"
confere "depois de Digitação: rt ao pressionar na tela=0.30 rt=false"
confere "zonas marcadas=false"
confere "cor W: Some(Rgb(255, 0, 0))"
confere "cor W removida: None"
confere "função W: a"
confere "função W restaurada: w"
confere "Fn+Q restaurado: 0x1"
confere "socd removido: Nenhuma"
confere "dks Insert: gravado igual ao montado=true"
confere "dks removido: Nenhuma"
confere "brilho no teclado: Some(77)"
confere "depois de voltar ao efeito: []"
confere "troca por fora percebida: perfil Some(2), seletor em 2"
confere "depois da falha: tela=ok brilho voltou=true edição preservada=true"
confere "reconectou sozinho: tela=ok dados=true"
confere "restaurado: true"
# Aviso de tela que não coube na janela. Sob o xvfb, sem gerenciador de janelas, a janela fica
# 10 px menor que o pedido em cada medida e o aviso aparece com 20 px de diferença: até aí é do
# ambiente, acima disso é a tela que cresceu.
excesso="$(sed -n 's/.*exceeds AdwApplicationWindow \(.*\)/\1/p' "$saida/saida.txt" | python3 -c '
import re, sys
pior = 0
for linha in sys.stdin:
    pedido, disponivel = [list(map(int, re.findall(r"\d+", parte))) for parte in linha.split(" px, ")[:2]]
    pior = max([pior] + [p - d for p, d in zip(pedido, disponivel)])
print(pior)')"
if [ "$excesso" -gt 20 ]; then
    grep "exceeds AdwApplicationWindow" "$saida/saida.txt" | sed 's/.*exceeds/exceeds/' | sort | uniq -c
    falhou "alguma tela não coube na janela (passou $excesso px)"
fi
telas="$(find "$saida" -maxdepth 1 -name 'tela-*.png' | wc -l)"
[ "$telas" -ge 30 ] || falhou "poucas telas desenhadas: $telas"
echo "TESTE DA INTERFACE OK ($telas telas em $saida)"
