# Compila a partir do código-fonte. As dependências Rust são baixadas pelo cargo durante a
# compilação, então este spec serve para `rpmbuild` com rede (como na integração contínua),
# não para os construtores oficiais do Fedora.
Name:           mag82
Version:        %{?versao}%{!?versao:0.1.1}
Release:        1%{?dist}
Summary:        Configuração do teclado Avell Volcano's MAG82
License:        MIT
Vendor:         Juliano Liberato (https://9level.com.br)
Packager:       Juliano Liberato <juliano@9level.com.br>
URL:            https://github.com/julianol1berato/Avell-MAG
Source0:        mag82-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconfig(gtk4) >= 4.12
BuildRequires:  pkgconfig(libadwaita-1) >= 1.5
BuildRequires:  systemd-rpm-macros
BuildRequires:  desktop-file-utils
# pkexec: pede a senha de administrador quando o programa instala uma versão nova
Recommends:     polkit

%global debug_package %{nil}

%description
Configura o teclado magnético Avell Volcano's MAG82: ponto de atuação e Rapid
Trigger por tecla, remapeamento, SOCD, cores fixas, efeitos de luz das teclas e
da faixa lateral, perfis, taxa de resposta e calibração.

Inclui a interface gráfica (mag82-gui), a linha de comando (mag82) e a regra
udev que libera o acesso ao teclado para o usuário logado, sem root.

Projeto independente, sem vínculo com a Avell.
Feito por Juliano Liberato (https://9level.com.br).

%prep
%autosetup -n mag82-%{version}

%build
cargo build --release --locked

%check
cargo test --release --locked

%install
install -Dm755 target/release/mag82 %{buildroot}%{_bindir}/mag82
install -Dm755 target/release/mag82-gui %{buildroot}%{_bindir}/mag82-gui
install -Dm644 udev/70-mag82.rules %{buildroot}%{_udevrulesdir}/70-mag82.rules
install -Dm644 data/io.github.mag82linux.desktop %{buildroot}%{_datadir}/applications/io.github.mag82linux.desktop
install -Dm644 data/io.github.mag82linux.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.mag82linux.svg
install -Dm644 data/io.github.mag82linux.metainfo.xml %{buildroot}%{_metainfodir}/io.github.mag82linux.metainfo.xml
desktop-file-validate %{buildroot}%{_datadir}/applications/io.github.mag82linux.desktop

%post
%udev_rules_update
udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || :

%postun
%udev_rules_update

%files
%license LICENSE
%doc README.md
%{_bindir}/mag82
%{_bindir}/mag82-gui
%{_udevrulesdir}/70-mag82.rules
%{_datadir}/applications/io.github.mag82linux.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.mag82linux.svg
%{_metainfodir}/io.github.mag82linux.metainfo.xml
