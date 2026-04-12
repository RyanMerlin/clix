#!/bin/sh
set -eu

repo="RyanMerlin/clix"
version="${CLIX_VERSION:-latest}"
install_dir="${CLIX_INSTALL_DIR:-$HOME/.local/bin}"
os="$(uname -s | tr '[:upper:]' '[:lower:]')"
arch="$(uname -m)"

case "$os" in
  linux|darwin) ;;
  *)
    echo "unsupported operating system: $os" >&2
    exit 1
    ;;
esac

case "$arch" in
  x86_64|amd64) arch="amd64" ;;
  arm64|aarch64) arch="arm64" ;;
  *)
    echo "unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

asset="clix-${os}-${arch}"
if [ "$os" = "windows" ]; then
  asset="${asset}.exe"
fi

if [ "$version" = "latest" ]; then
  url="https://github.com/${repo}/releases/latest/download/${asset}"
else
  url="https://github.com/${repo}/releases/download/${version}/${asset}"
fi
tmp="$(mktemp)"
cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT INT TERM

mkdir -p "$install_dir"
curl -fsSL "$url" -o "$tmp"
chmod +x "$tmp"
mv "$tmp" "$install_dir/clix"
trap - EXIT INT TERM
echo "installed clix to $install_dir/clix"
