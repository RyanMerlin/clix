#!/bin/sh
set -eu
umask 022

repo="RyanMerlin/clix"
version="${CLIX_VERSION:-latest}"
install_dir="${CLIX_INSTALL_DIR:-$HOME/.local/bin}"
strict="${CLIX_STRICT_VERIFY:-0}"
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
  checksum_url="https://github.com/${repo}/releases/latest/download/${asset}.sha256"
  sbom_url="https://github.com/${repo}/releases/latest/download/${asset}.spdx.json"
  sbom_checksum_url="https://github.com/${repo}/releases/latest/download/${asset}.spdx.json.sha256"
else
  url="https://github.com/${repo}/releases/download/${version}/${asset}"
  checksum_url="https://github.com/${repo}/releases/download/${version}/${asset}.sha256"
  sbom_url="https://github.com/${repo}/releases/download/${version}/${asset}.spdx.json"
  sbom_checksum_url="https://github.com/${repo}/releases/download/${version}/${asset}.spdx.json.sha256"
fi
tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT INT TERM

mkdir -p "$install_dir"
binary="$tmpdir/$asset"
checksum_file="$tmpdir/$asset.sha256"
sbom_file="$tmpdir/$asset.spdx.json"
sbom_checksum_file="$tmpdir/$asset.spdx.json.sha256"

curl -fsSL --retry 3 --retry-connrefused --connect-timeout 10 --max-time 120 "$url" -o "$binary"
curl -fsSL --retry 3 --retry-connrefused --connect-timeout 10 --max-time 120 "$checksum_url" -o "$checksum_file"

expected_checksum="$(awk 'NR==1 {print $1}' "$checksum_file")"
if [ -z "$expected_checksum" ]; then
  echo "downloaded checksum file was empty" >&2
  exit 1
fi

actual_checksum=""
if command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "$binary" | awk '{print $1}')"
elif command -v shasum >/dev/null 2>&1; then
  actual_checksum="$(shasum -a 256 "$binary" | awk '{print $1}')"
else
  echo "no sha256 utility found" >&2
  exit 1
fi

if [ "$actual_checksum" != "$expected_checksum" ]; then
  echo "checksum verification failed" >&2
  exit 1
fi

if [ "$strict" != "0" ] && [ "$strict" != "false" ] && [ "$strict" != "no" ]; then
  curl -fsSL --retry 3 --retry-connrefused --connect-timeout 10 --max-time 120 "$sbom_url" -o "$sbom_file"
  curl -fsSL --retry 3 --retry-connrefused --connect-timeout 10 --max-time 120 "$sbom_checksum_url" -o "$sbom_checksum_file"

  expected_sbom_checksum="$(awk 'NR==1 {print $1}' "$sbom_checksum_file")"
  if [ -z "$expected_sbom_checksum" ]; then
    echo "downloaded sbom checksum file was empty" >&2
    exit 1
  fi

  actual_sbom_checksum=""
  if command -v sha256sum >/dev/null 2>&1; then
    actual_sbom_checksum="$(sha256sum "$sbom_file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual_sbom_checksum="$(shasum -a 256 "$sbom_file" | awk '{print $1}')"
  else
    echo "no sha256 utility found" >&2
    exit 1
  fi

  if [ "$actual_sbom_checksum" != "$expected_sbom_checksum" ]; then
    echo "sbom checksum verification failed" >&2
    exit 1
  fi

  if command -v gh >/dev/null 2>&1; then
    gh attestation verify "$binary" -R "$repo"
    gh attestation verify "$binary" -R "$repo" --predicate-type https://spdx.dev/Document/v2.3
  else
    echo "strict verification requested but gh is not installed" >&2
    exit 1
  fi
fi

chmod +x "$binary"
mv "$binary" "$install_dir/clix"
trap - EXIT INT TERM
echo "installed clix to $install_dir/clix"
