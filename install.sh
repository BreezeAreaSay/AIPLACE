#!/bin/sh
# Triad Canvas installer — downloads the latest release binary for this machine.
#   curl -fsSL https://raw.githubusercontent.com/BreezeAreaSay/AIPLACE/main/install.sh | sh
# Env: TRIAD_VERSION=v0.2.0 (pin), TRIAD_INSTALL_DIR=/usr/local/bin, TRIAD_NO_SETUP=1 (skip wizard)
set -eu

REPO="BreezeAreaSay/AIPLACE"
BIN="triad"
INSTALL_DIR="${TRIAD_INSTALL_DIR:-$HOME/.triad/bin}"

say() { printf '\033[1;32m▸\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "нужна утилита: $1"; }
need curl; need tar

os=$(uname -s); arch=$(uname -m)
case "$os" in
  Linux)  case "$arch" in x86_64|amd64) target=x86_64-unknown-linux-gnu ;; aarch64|arm64) target=aarch64-unknown-linux-gnu ;; *) die "неподдерживаемая архитектура: $arch" ;; esac ;;
  Darwin) case "$arch" in x86_64) target=x86_64-apple-darwin ;; arm64) target=aarch64-apple-darwin ;; *) die "неподдерживаемая архитектура: $arch" ;; esac ;;
  *) die "ОС $os не поддерживается этим скриптом. Windows: install.ps1" ;;
esac

if [ -n "${TRIAD_VERSION:-}" ]; then
  tag="$TRIAD_VERSION"
else
  say "ищу последний релиз…"
  tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
  [ -n "$tag" ] || die "не смог получить последний релиз (нет релизов или нет сети)"
fi
ver="${tag#v}"
name="$BIN-$ver-$target"
url="https://github.com/$REPO/releases/download/$tag/$name.tar.gz"

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
say "скачиваю $url"
curl -fL --progress-bar "$url" -o "$tmp/pkg.tar.gz" || die "нет сборки $name в релизе $tag"
if curl -fsSL "$url.sha256" -o "$tmp/pkg.sha256" 2>/dev/null; then
  expected=$(awk '{print $1}' "$tmp/pkg.sha256")
  actual=$( (command -v sha256sum >/dev/null && sha256sum "$tmp/pkg.tar.gz" || shasum -a 256 "$tmp/pkg.tar.gz") | awk '{print $1}')
  [ "$expected" = "$actual" ] || die "контрольная сумма не совпала"
fi
tar -xzf "$tmp/pkg.tar.gz" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/$name/$BIN" "$INSTALL_DIR/$BIN"
say "установлено: $INSTALL_DIR/$BIN ($tag)"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    line="export PATH=\"$INSTALL_DIR:\$PATH\""
    for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do
      if [ -f "$rc" ] && ! grep -qF "$INSTALL_DIR" "$rc"; then
        printf '\n# Triad Canvas\n%s\n' "$line" >> "$rc"; say "добавил в PATH через $rc"; break
      fi
    done
    say "в текущей сессии выполни:  $line"
    ;;
esac

if [ -z "${TRIAD_NO_SETUP:-}" ] && [ -r /dev/tty ]; then
  say "настройка (ключ OpenRouter, модели). Ctrl+C — пропустить, потом: triad setup"
  "$INSTALL_DIR/$BIN" setup </dev/tty || true
else
  say "дальше:  $INSTALL_DIR/$BIN setup   (ключ OpenRouter)   →   $INSTALL_DIR/$BIN"
fi
