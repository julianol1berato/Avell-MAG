#!/usr/bin/env bash
# Gera dist/mag82_<versão>_<arquitetura>.deb a partir dos binários de target/release.
# Uso: packaging/deb.sh        (compila antes, se preciso)
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RAIZ"
cargo build --release --locked

VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
ARQ="$(dpkg --print-architecture)"
P="$(mktemp -d)"
trap 'rm -rf "$P"' EXIT
chmod 755 "$P"

install -Dm755 target/release/mag82 "$P/usr/bin/mag82"
install -Dm755 target/release/mag82-gui "$P/usr/bin/mag82-gui"
install -Dm644 udev/70-mag82.rules "$P/usr/lib/udev/rules.d/70-mag82.rules"
install -Dm644 data/io.github.mag82linux.desktop "$P/usr/share/applications/io.github.mag82linux.desktop"
install -Dm644 data/io.github.mag82linux.svg "$P/usr/share/icons/hicolor/scalable/apps/io.github.mag82linux.svg"
install -Dm644 data/io.github.mag82linux.metainfo.xml "$P/usr/share/metainfo/io.github.mag82linux.metainfo.xml"
# páginas de manual: a da linha de comando sai da própria ajuda do programa
install -d -m755 "$P/usr/share/man" "$P/usr/share/man/man1"
target/release/mag82 manual | gzip -9n > "$P/usr/share/man/man1/mag82.1.gz"
gzip -9nc data/mag82-gui.1 > "$P/usr/share/man/man1/mag82-gui.1.gz"
chmod 644 "$P"/usr/share/man/man1/*.gz
install -Dm644 LICENSE "$P/usr/share/doc/mag82/copyright"
install -Dm644 README.md "$P/usr/share/doc/mag82/README.md"
printf 'mag82 (%s) unstable; urgency=medium\n\n  * Versão %s; as notas ficam na página de versões do projeto.\n\n -- Juliano Liberato <juliano@9level.com.br>  %s\n' \
    "$VERSAO" "$VERSAO" "$(date -R)" | gzip -9n > "$P/usr/share/doc/mag82/changelog.gz"
chmod 644 "$P/usr/share/doc/mag82/changelog.gz"

# dependências a partir das bibliotecas que os binários realmente usam
mkdir -p "$P/DEBIAN" debian
[ -e debian/control ] || { echo "Source: mag82" > debian/control; CRIOU_CONTROLE=1; }
DEPS="$(dpkg-shlibdeps -O "$P/usr/bin/mag82" "$P/usr/bin/mag82-gui" 2>/dev/null | sed 's/^shlibs:Depends=//')"
[ -z "${CRIOU_CONTROLE:-}" ] || rm -rf debian
TAMANHO="$(du -sk "$P/usr" | cut -f1)"

cat > "$P/DEBIAN/control" <<CONTROLE
Package: mag82
Version: $VERSAO
Architecture: $ARQ
Maintainer: Juliano Liberato <juliano@9level.com.br>
Installed-Size: $TAMANHO
Depends: $DEPS
Recommends: pkexec | policykit-1
Section: utils
Priority: optional
Homepage: https://github.com/julianol1berato/Avell-MAG
Description: Configuração do teclado Avell Volcano's MAG82
 Configura o teclado magnético Avell Volcano's MAG82: ponto de atuação e
 Rapid Trigger por tecla, remapeamento, SOCD, cores fixas, efeitos de luz
 das teclas e da faixa lateral, perfis, taxa de resposta e calibração.
 .
 Inclui a interface gráfica (mag82-gui), a linha de comando (mag82) e a
 regra udev que libera o acesso ao teclado para o usuário logado, sem root.
 .
 Projeto independente, sem vínculo com a Avell.
 Feito por Juliano Liberato (https://9level.com.br).
CONTROLE

# depois de instalar ou remover: recarrega as regras udev para o teclado já conectado
for roteiro in postinst postrm; do
cat > "$P/DEBIAN/$roteiro" <<'ROTEIRO'
#!/bin/sh
set -e
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || true
fi
ROTEIRO
chmod 755 "$P/DEBIAN/$roteiro"
done

mkdir -p dist
SAIDA="dist/mag82_${VERSAO}_${ARQ}.deb"
dpkg-deb --root-owner-group --build "$P" "$SAIDA" >/dev/null
echo "$SAIDA"
