#!/usr/bin/env bash
# Fetch (or build) the aria2c binary that LalaLM bundles as a Tauri sidecar.
#
# macOS:   built from source with Apple TLS (--with-appletls) and without any
#          third-party dynamic dependencies, so the single binary can be shipped
#          inside the .app and work out of the box.
# Windows: downloads a static mingw build from abcfy2/aria2-static-build
#          (run with WINDOWS=1 to prepare it ahead of time).
#
# Usage:  bash scripts/fetch-aria2.sh [--force]
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
BIN_DIR="src-tauri/binaries"
mkdir -p "$BIN_DIR"

ARIA2_VERSION="1.37.0"
FORCE="${1:-}"

host_triple() {
  rustc -vV 2>/dev/null | awk '/^host:/ { print $2 }'
}

build_macos() {
  local triple dest workdir tarball srcdir
  triple="$(host_triple)"
  [ -n "$triple" ] || triple="$(uname -m)-apple-darwin"
  dest="$BIN_DIR/aria2c-$triple"

  if [[ -x "$dest" && "$FORCE" != "--force" ]]; then
    echo "[fetch-aria2] already present: $dest"
    return 0
  fi

  workdir="$ROOT/.aria2-build"
  mkdir -p "$workdir"
  tarball="$workdir/aria2-$ARIA2_VERSION.tar.xz"
  if [[ ! -f "$tarball" ]]; then
    echo "[fetch-aria2] downloading aria2 $ARIA2_VERSION source..."
    curl -fL --retry 3 -o "$tarball" \
      "https://github.com/aria2/aria2/releases/download/release-$ARIA2_VERSION/aria2-$ARIA2_VERSION.tar.xz"
  fi

  rm -rf "$workdir/aria2-$ARIA2_VERSION"
  tar -xf "$tarball" -C "$workdir"
  srcdir="$workdir/aria2-$ARIA2_VERSION"

  echo "[fetch-aria2] configuring (Apple TLS, no external dylibs)..."
  (
    cd "$srcdir"
    ./configure \
      --disable-nls \
      --disable-metalink \
      --disable-bittorrent \
      --without-openssl \
      --without-gnutls \
      --with-appletls \
      --without-libssh2 \
      --without-sqlite3 \
      --without-libxml2 \
      --without-expat \
      --without-libgmp \
      --without-libnettle \
      --without-libcares \
      CFLAGS="-O2" CXXFLAGS="-O2 -std=c++17"
  ) >"$workdir/configure.log" 2>&1 || {
    echo "[fetch-aria2] configure failed, see $workdir/configure.log"; exit 1;
  }

  echo "[fetch-aria2] building ($(sysctl -n hw.ncpu 2>/dev/null || nproc) jobs)..."
  (
    cd "$srcdir"
    make -j "$(sysctl -n hw.ncpu 2>/dev/null || nproc)"
  ) >"$workdir/make.log" 2>&1 || {
    echo "[fetch-aria2] make failed, see $workdir/make.log"; exit 1;
  }

  cp "$srcdir/src/aria2c" "$dest"
  chmod +x "$dest"
  codesign --force --sign - "$dest" 2>/dev/null || true
  echo "[fetch-aria2] built: $dest"
  "$dest" --version | head -1 || true
}

fetch_windows() {
  local dest zip url
  dest="$BIN_DIR/aria2c-x86_64-pc-windows-msvc.exe"
  if [[ -f "$dest" && "$FORCE" != "--force" ]]; then
    echo "[fetch-aria2] already present: $dest"
    return 0
  fi
  zip="$ROOT/.aria2-build/aria2-win-x64.zip"
  mkdir -p "$(dirname "$zip")"
  url="https://github.com/abcfy2/aria2-static-build/releases/download/continuous/aria2-x86_64-w64-mingw32_static.zip"
  echo "[fetch-aria2] downloading windows static build..."
  curl -fL --retry 3 -o "$zip" "$url"
  rm -rf "$ROOT/.aria2-build/win-x64"
  mkdir -p "$ROOT/.aria2-build/win-x64"
  unzip -o -q "$zip" -d "$ROOT/.aria2-build/win-x64"
  find "$ROOT/.aria2-build/win-x64" -name 'aria2c.exe' -exec cp {} "$dest" \;
  echo "[fetch-aria2] saved: $dest"
}

case "$(uname -s)" in
  Darwin)
    build_macos
    [[ "${WINDOWS:-0}" == "1" ]] && fetch_windows
    ;;
  MINGW*|MSYS*|CYGWIN*)
    fetch_windows
    ;;
  *)
    echo "[fetch-aria2] unsupported host OS: $(uname -s)" >&2
    exit 1
    ;;
esac
