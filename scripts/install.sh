#!/bin/sh
# Installs the snappea CLI from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh -s -- --version 0.2.0
#
# Options (or environment variables):
#   --version <X.Y.Z>   SNAPPEA_VERSION      release to install (default: the latest)
#   --to <DIR>          SNAPPEA_INSTALL_DIR  folder for the binary (default: ~/.local/bin)
#
# Supports Linux (x86_64, arm64; glibc), macOS (Intel, Apple silicon) and Windows
# (x86_64, from Git Bash). The archive is checked against the release's SHA256SUMS.
# In GitHub Actions the folder is also added to the PATH of later steps.
set -eu

REPO="crowecawcaw/snappea"
version="${SNAPPEA_VERSION:-}"
dir="${SNAPPEA_INSTALL_DIR:-${HOME}/.local/bin}"

say() { printf 'snappea install: %s\n' "$*" >&2; }
fail() { say "error: $*"; exit 1; }
usage() {
  cat <<'EOT'
Installs the snappea CLI from a GitHub release.

Usage: install.sh [--version X.Y.Z] [--to DIR]

  --version <X.Y.Z>  release to install (default: the latest; or SNAPPEA_VERSION)
  --to <DIR>         folder for the binary (default: ~/.local/bin; or SNAPPEA_INSTALL_DIR)
EOT
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || fail "--version needs a value"; version="$2"; shift 2 ;;
    --version=*) version="${1#*=}"; shift ;;
    --to) [ $# -ge 2 ] || fail "--to needs a value"; dir="$2"; shift 2 ;;
    --to=*) dir="${1#*=}"; shift ;;
    -h | --help) usage; exit 0 ;;
    *) fail "unknown option $1 (try --help)" ;;
  esac
done

command -v curl >/dev/null 2>&1 || fail "curl is needed"

# The release target for this machine.
os=$(uname -s)
arch=$(uname -m)
case "$arch" in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) fail "no prebuilt binary for CPU $arch; build one with: cargo install snappea" ;;
esac
exe=snappea
case "$os" in
  Linux)
    if ldd --version 2>&1 | grep -qi musl; then
      fail "the Linux binaries need glibc (this looks like musl, e.g. Alpine); build one with: cargo install snappea"
    fi
    target="${arch}-unknown-linux-gnu" ext=tar.gz ;;
  Darwin) target="${arch}-apple-darwin" ext=tar.gz ;;
  MINGW* | MSYS* | CYGWIN*)
    [ "$arch" = x86_64 ] || fail "no prebuilt binary for Windows on $arch"
    target="x86_64-pc-windows-msvc" ext=zip exe=snappea.exe ;;
  *) fail "no prebuilt binary for $os; build one with: cargo install snappea" ;;
esac

# The latest release: github.com/<repo>/releases/latest redirects to its tag.
if [ -z "$version" ]; then
  url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest") ||
    fail "could not look up the latest release"
  version="${url##*/}"
  [ "$version" != latest ] || fail "the repository has no releases yet"
fi
version="${version#v}"
tag="v${version}"
archive="snappea-${tag}-${target}.${ext}"
base="https://github.com/${REPO}/releases/download/${tag}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading snappea ${version} for ${target}"
curl -fsSL -o "$tmp/$archive" "$base/$archive" || fail "could not download $base/$archive"
curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS" || fail "could not download $base/SHA256SUMS"

want=$(awk -v f="$archive" '$2 == f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || fail "SHA256SUMS has no entry for $archive"
if command -v sha256sum >/dev/null 2>&1; then
  got=$(sha256sum "$tmp/$archive" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
  got=$(shasum -a 256 "$tmp/$archive" | awk '{ print $1 }')
else
  fail "sha256sum or shasum is needed to check the download"
fi
[ "$got" = "$want" ] || fail "checksum mismatch for $archive"

if [ "$ext" = zip ]; then
  command -v unzip >/dev/null 2>&1 || fail "unzip is needed"
  unzip -q "$tmp/$archive" -d "$tmp"
else
  tar -xzf "$tmp/$archive" -C "$tmp"
fi

mkdir -p "$dir"
cp "$tmp/snappea-${tag}-${target}/$exe" "$dir/$exe"
chmod 755 "$dir/$exe"
say "installed $("$dir/$exe" --version) to $dir/$exe"

if [ -n "${GITHUB_PATH:-}" ]; then
  # Windows runners need a Windows path here.
  if command -v cygpath >/dev/null 2>&1; then cygpath -w "$dir"; else echo "$dir"; fi >>"$GITHUB_PATH"
else
  case ":${PATH}:" in
    *":${dir}:"*) ;;
    *) say "add $dir to your PATH to run snappea, e.g. in ~/.profile: export PATH=\"$dir:\$PATH\"" ;;
  esac
fi
