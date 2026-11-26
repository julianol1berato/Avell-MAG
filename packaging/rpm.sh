#!/usr/bin/env bash
# Gera dist/mag82-<versão>-1.<distro>.<arquitetura>.rpm (precisa de rpmbuild; pensado para Fedora).
# Uso: packaging/rpm.sh
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RAIZ"
VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
TOPO="$(mktemp -d)"
trap 'rm -rf "$TOPO"' EXIT
mkdir -p "$TOPO/SOURCES" dist
# o tarball leva só o que faz parte do projeto
tar --transform "s|^|mag82-$VERSAO/|" -czf "$TOPO/SOURCES/mag82-$VERSAO.tar.gz" \
    Cargo.toml Cargo.lock LICENSE README.md crates data udev packaging scripts
rpmbuild -bb --define "_topdir $TOPO" --define "versao $VERSAO" packaging/mag82.spec
cp "$TOPO"/RPMS/*/mag82-"$VERSAO"-*.rpm dist/
ls dist/mag82-"$VERSAO"-*.rpm
