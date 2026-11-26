#!/usr/bin/env bash
# Gera dist/mag82-<versão>-<arquitetura>.tar.gz com os binários de target/release e os arquivos
# de dados, no formato que o programa usa para atualizar uma instalação em ~/.local.
# Uso: packaging/tar.sh        (compila antes, se preciso)
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RAIZ"
cargo build --release --locked

VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARQ="$(uname -m)"
P="$(mktemp -d)"
trap 'rm -rf "$P"' EXIT

install -Dm755 target/release/mag82 "$P/mag82/bin/mag82"
install -Dm755 target/release/mag82-gui "$P/mag82/bin/mag82-gui"
install -Dm644 data/io.github.mag82linux.desktop "$P/mag82/share/applications/io.github.mag82linux.desktop"
install -Dm644 data/io.github.mag82linux.svg "$P/mag82/share/icons/hicolor/scalable/apps/io.github.mag82linux.svg"
install -Dm644 data/io.github.mag82linux.metainfo.xml "$P/mag82/share/metainfo/io.github.mag82linux.metainfo.xml"

mkdir -p dist
SAIDA="dist/mag82-${VERSAO}-${ARQ}.tar.gz"
tar --owner=0 --group=0 --sort=name -czf "$SAIDA" -C "$P" mag82
echo "gerado: $SAIDA"
tar -tzf "$SAIDA"
