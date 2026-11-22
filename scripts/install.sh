#!/bin/sh
# sigillr installer. Usage: install.sh [--version vX.Y.Z] [--dir PATH]
# Env: SIGILLR_VERSION, SIGILLR_INSTALL_DIR, SIGILLR_RELEASE_BASE (test hook; file://DIR reads local files,
# and then SIGILLR_VERSION is required).
# Exit codes: 0 ok, 1 failure (including a checksum mismatch: nothing is installed), 2 usage or unsupported platform.
set -eu

BASE_DEFAULT="https://github.com/startr-trade/sigillr/releases"
BASE="${SIGILLR_RELEASE_BASE:-$BASE_DEFAULT}"
VERSION="${SIGILLR_VERSION:-latest}"
DIR="${SIGILLR_INSTALL_DIR:-${HOME:-}/.local/bin}"

die() { code=$1; shift; echo "install.sh: $*" >&2; exit "$code"; }

while [ $# -gt 0 ]; do
  case $1 in
    --version) [ $# -ge 2 ] || die 2 "--version needs a value"; VERSION=$2; shift 2 ;;
    --dir) [ $# -ge 2 ] || die 2 "--dir needs a value"; DIR=$2; shift 2 ;;
    -h|--help) echo "usage: install.sh [--version vX.Y.Z] [--dir PATH]"; exit 0 ;;
    *) die 2 "unknown argument: $1" ;;
  esac
done
[ -n "$DIR" ] || die 2 "no install directory (set --dir or HOME)"

# 1. Detect the target.
os=$(uname -s)
arch=$(uname -m)
case "$os/$arch" in
  Linux/x86_64|Linux/amd64) TARGET=x86_64-unknown-linux-gnu ;;
  Darwin/x86_64) TARGET=x86_64-apple-darwin ;;
  Darwin/arm64) TARGET=aarch64-apple-darwin ;;
  *) die 2 "unsupported platform: $os $arch" ;;
esac

TMP=$(mktemp -d) || die 1 "cannot create a temporary directory"
trap 'rm -rf "$TMP"' EXIT

# 2. Resolve the version. "latest" follows the releases/latest redirect to its tag.
if [ "$VERSION" = latest ]; then
  case $BASE in
    file://*) die 2 "SIGILLR_VERSION is required with a file:// release base" ;;
  esac
  command -v curl >/dev/null 2>&1 || die 1 "curl is required to find the latest release; set SIGILLR_VERSION instead"
  final=$(curl --proto '=https' --tlsv1.2 -fsSL -o /dev/null -w '%{url_effective}' "$BASE/latest") \
    || die 1 "cannot find the latest release at $BASE/latest"
  VERSION=${final##*/}
fi
TAG=v${VERSION#v}
VER=${TAG#v}
ASSET="sigillr-$VER-$TARGET.tar.gz"

# fetch <asset> <dest>: URL for GitHub, plain copy for file://.
fetch() {
  asset=$1; dest=$2
  case $BASE in
    file://*) cp "${BASE#file://}/$asset" "$dest" || die 1 "cannot read $asset from $BASE" ;;
    *)
      url="$BASE/download/$TAG/$asset"
      if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -fsSL -o "$dest" "$url" || die 1 "download failed: $url"
      elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$dest" "$url" || die 1 "download failed: $url"
      else
        die 1 "neither curl nor wget is available"
      fi ;;
  esac
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d ' ' -f 1
  else die 1 "neither sha256sum nor shasum is available"; fi
}

fetch "$ASSET" "$TMP/$ASSET"
fetch "$ASSET.sha256" "$TMP/$ASSET.sha256"
expected=$(cut -d ' ' -f 1 "$TMP/$ASSET.sha256" | head -n 1)
[ -n "$expected" ] || die 1 "$ASSET.sha256 is empty"

# 3. Verify. Nothing is installed unless this passes.
actual=$(sha256_of "$TMP/$ASSET")
if [ "$actual" != "$expected" ]; then
  echo "install.sh: checksum mismatch for $ASSET" >&2
  echo "  expected: $expected" >&2
  echo "  actual:   $actual" >&2
  exit 1
fi

# 4. Extract and install.
mkdir "$TMP/x"
tar -xzf "$TMP/$ASSET" -C "$TMP/x" || die 1 "cannot extract $ASSET"
BIN="$TMP/x/sigillr-$VER-$TARGET/sigillr"
[ -f "$BIN" ] || die 1 "archive does not contain sigillr"
mkdir -p "$DIR" || die 1 "cannot create $DIR"
if command -v install >/dev/null 2>&1; then
  install -m 0755 "$BIN" "$DIR/sigillr" || die 1 "cannot install into $DIR"
else
  cp "$BIN" "$DIR/sigillr" || die 1 "cannot install into $DIR"
  chmod 0755 "$DIR/sigillr"
fi

echo "installed $DIR/sigillr"
case ":${PATH:-}:" in
  *":$DIR:"*) ;;
  *) echo "note: $DIR is not on your PATH; add: export PATH=\"$DIR:\$PATH\"" ;;
esac
