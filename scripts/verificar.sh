#!/usr/bin/env bash
# Verificação completa no computador de quem fez a mudança, antes de enviar: compila, roda os
# testes unitários e de integração, exercita a interface contra o teclado simulado e gera os
# pacotes da versão. É o mesmo que a integração contínua confere depois, de forma independente.
#
# Uso: scripts/verificar.sh [--teclado] [--sem-pacotes]
#   --teclado       roda também o autoteste da interface no teclado de verdade. Ele GRAVA no
#                   teclado e restaura no fim; a configuração do perfil ativo é comparada antes
#                   e depois.
#   --sem-pacotes   pula a geração dos pacotes (.deb, .tar.gz e .rpm).
set -euo pipefail
RAIZ="$(cd "$(dirname "$0")/.." && pwd)"
cd "$RAIZ"
teclado=0 pacotes=1
for opcao in "$@"; do
    case "$opcao" in
        --teclado) teclado=1 ;;
        --sem-pacotes) pacotes=0 ;;
        *) echo "opção desconhecida: $opcao"; exit 2 ;;
    esac
done
falhou() { echo; echo "VERIFICAÇÃO FALHOU: $*"; exit 1; }
etapa() { echo; echo "== $* =="; }
feito=()
saida="dist/verificacao"
rm -rf "$saida"
mkdir -p "$saida"
VERSAO="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"

etapa "Compilar (versão $VERSAO)"
cargo build --release --locked || falhou "não compilou"
feito+=("compilou")

etapa "Testes unitários e de integração"
cargo test --release --locked 2>&1 | tee "$saida/testes.txt" | grep -E "^test result|FAILED|panicked|^error" || true
grep -q "^test result: FAILED\|^error" "$saida/testes.txt" && falhou "há teste falhando (veja $saida/testes.txt)"
total="$(grep "^test result: ok" "$saida/testes.txt" | sed 's/.*ok\. \([0-9]*\) passed.*/\1/' | paste -sd+ | bc)"
[ "${total:-0}" -gt 0 ] || falhou "nenhum teste rodou"
feito+=("$total testes passaram")

etapa "Interface contra o teclado simulado"
scripts/teste-interface.sh "$saida/telas" > "$saida/interface.txt" 2>&1 || { tail -5 "$saida/interface.txt"; falhou "o teste da interface não passou (veja $saida/interface.txt)"; }
tail -1 "$saida/interface.txt"
feito+=("interface exercitada no teclado simulado")

if [ "$teclado" = 1 ]; then
    etapa "Interface no teclado de verdade (grava e restaura)"
    target/release/mag82 exportar "$saida/antes.mag82" > /dev/null || falhou "o teclado não respondeu"
    # o autoteste também importa uma configuração: o arquivo de desfazer do usuário é preservado
    guardada="${XDG_STATE_HOME:-$HOME/.local/state}/mag82/antes-da-ultima-importacao.mag82"
    [ -f "$guardada" ] && cp "$guardada" "$saida/guardada.bak"
    mkdir -p "$saida/teclado"
    MAG82_TESTE="$PWD/$saida/teclado/tela" timeout 600 target/autoteste/release/mag82-gui > "$saida/teclado.txt" 2>&1 \
        || falhou "o autoteste no teclado não rodou até o fim (veja $saida/teclado.txt)"
    if [ -f "$saida/guardada.bak" ]; then cp "$saida/guardada.bak" "$guardada"; else rm -f "$guardada"; fi
    grep -q "^restaurado: true" "$saida/teclado.txt" || falhou "o autoteste não restaurou o teclado (veja $saida/teclado.txt)"
    target/release/mag82 exportar "$saida/depois.mag82" > /dev/null
    diff -q "$saida/antes.mag82" "$saida/depois.mag82" > /dev/null \
        || falhou "o teclado não ficou como estava: compare $saida/antes.mag82 e $saida/depois.mag82"
    feito+=("interface exercitada no teclado de verdade, que ficou idêntico ao que era")
fi

if [ "$pacotes" = 1 ]; then
    etapa "Gerar os pacotes da versão $VERSAO"
    gerados=()
    if command -v dpkg-deb > /dev/null; then
        packaging/deb.sh > "$saida/deb.txt" 2>&1 || falhou "o .deb não foi gerado (veja $saida/deb.txt)"
        gerados+=(dist/mag82_"$VERSAO"_*.deb)
    else
        echo "sem dpkg-deb: o .deb fica para a integração contínua"
    fi
    packaging/tar.sh > "$saida/tar.txt" 2>&1 || falhou "o .tar.gz não foi gerado (veja $saida/tar.txt)"
    gerados+=(dist/mag82-"$VERSAO"-*.tar.gz)
    if command -v rpmbuild > /dev/null; then
        packaging/rpm.sh > "$saida/rpm.txt" 2>&1 || falhou "o .rpm não foi gerado (veja $saida/rpm.txt)"
        gerados+=(dist/mag82-"$VERSAO"-*.rpm)
    else
        echo "sem rpmbuild: o .rpm é gerado no Fedora pela integração contínua"
    fi
    sha256sum "${gerados[@]}"
    feito+=("${#gerados[@]} pacote(s) da versão $VERSAO em dist/")
fi

echo
echo "VERIFICAÇÃO OK"
for item in "${feito[@]}"; do echo "  - $item"; done
