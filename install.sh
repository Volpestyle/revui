#!/bin/sh
# Installs the latest released revui binary. No Rust needed.
#   curl -fsSL https://raw.githubusercontent.com/Volpestyle/revui/main/install.sh | sh
# REVUI_INSTALL_DIR picks the folder (default ~/.local/bin). REVUI_DOWNLOAD picks where the archives are read from.
set -eu

repo="Volpestyle/revui"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target="aarch64-apple-darwin" ;;
  Darwin-x86_64) target="x86_64-apple-darwin" ;;
  Linux-x86_64) target="x86_64-unknown-linux-gnu" ;;
  Linux-aarch64 | Linux-arm64) target="aarch64-unknown-linux-gnu" ;;
  *)
    echo "No prebuilt revui for $(uname -s) $(uname -m). Build it instead: cargo install --git https://github.com/$repo" >&2
    exit 1
    ;;
esac

from="${REVUI_DOWNLOAD:-https://github.com/$repo/releases/latest/download}"
into="${REVUI_INSTALL_DIR:-$HOME/.local/bin}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

if ! curl -fsSL "$from/revui-$target.tar.gz" -o "$tmp/revui.tar.gz"; then
  echo "Could not download $from/revui-$target.tar.gz. Build it instead: cargo install --git https://github.com/$repo" >&2
  exit 1
fi
tar -xzf "$tmp/revui.tar.gz" -C "$tmp"
mkdir -p "$into"
install -m 755 "$tmp/revui" "$into/revui"

echo "Installed $into/revui"
case ":$PATH:" in
  *":$into:"*) ;;
  *) echo "$into is not on your PATH. Add it, then run: revui doctor" ;;
esac
