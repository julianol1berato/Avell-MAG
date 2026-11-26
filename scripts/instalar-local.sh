#!/usr/bin/env bash
# Instala o MAG82 só para o usuário atual, em ~/.local, sem root.
# A única parte que precisa de root é a regra udev que libera o acesso ao teclado; ela é
# instalada uma vez, com sudo, pelo próprio script (ou pulada com --sem-udev).
# Uso: scripts/instalar-local.sh [--sem-udev]
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
[ "$(id -u)" -ne 0 ] || { echo "rode como usuário comum, sem sudo"; exit 1; }

cd "$RAIZ"
cargo build --release --locked

BASE="${XDG_DATA_HOME:-$HOME/.local/share}"
install -Dm755 target/release/mag82 "$HOME/.local/bin/mag82"
install -Dm755 target/release/mag82-gui "$HOME/.local/bin/mag82-gui"
install -Dm644 data/io.github.mag82linux.svg "$BASE/icons/hicolor/scalable/apps/io.github.mag82linux.svg"
install -Dm644 data/io.github.mag82linux.metainfo.xml "$BASE/metainfo/io.github.mag82linux.metainfo.xml"
# o atalho aponta para o caminho completo: ~/.local/bin nem sempre está no PATH do ambiente gráfico
mkdir -p "$BASE/applications"
sed "s|^Exec=mag82-gui|Exec=$HOME/.local/bin/mag82-gui|" data/io.github.mag82linux.desktop \
    > "$BASE/applications/io.github.mag82linux.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$BASE/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$BASE/icons/hicolor" 2>/dev/null || true
echo "Programa instalado em ~/.local (procure por MAG82 no menu)."

if [ "${1:-}" != "--sem-udev" ] && [ ! -e /etc/udev/rules.d/70-mag82.rules ] && [ ! -e /usr/lib/udev/rules.d/70-mag82.rules ]; then
    echo "Falta liberar o acesso ao teclado (regra udev). Isso pede a senha de administrador uma vez:"
    sudo install -Dm644 udev/70-mag82.rules /etc/udev/rules.d/70-mag82.rules
    sudo udevadm control --reload
    sudo udevadm trigger --subsystem-match=hidraw
    echo "Acesso liberado. Se o programa não achar o teclado, desconecte e reconecte o cabo."
fi
