#!/usr/bin/env sh
set -eu

# ---- detect platform ----
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)  os_label="linux-gnu" ;;
  Darwin) os_label="darwin" ;;
  MINGW*|MSYS*|CYGWIN*) os_label="windows" ;;
  *)
    echo "error: unsupported OS '$OS'. Supported: Linux, macOS, Windows (Git Bash / WSL)"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch_label="x86_64" ;;
  aarch64|arm64) arch_label="aarch64" ;;
  *)
    echo "error: unsupported architecture '$ARCH'. Supported: x86_64, aarch64"
    exit 1
    ;;
esac

SUFFIX="${arch_label}-${os_label}"
echo "detected: $OS / $ARCH  →  suffix: $SUFFIX"

# ---- resolve version ----
echo ""
if [ -n "${VERSION:-}" ]; then
  echo "using VERSION=$VERSION (from environment)"
else
  echo "resolving latest version from GitHub API..."
  if ! command -v curl >/dev/null 2>&1; then
    echo "error: curl is required to download mioctl. Please install curl and retry."
    exit 1
  fi
  VERSION=$(curl -sSf https://api.github.com/repos/exponentxqq/mioctl/releases/latest \
    | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$VERSION" ]; then
    echo "error: could not resolve latest version from GitHub API."
    echo "  Try setting VERSION= manually: VERSION=v0.1.0 $0"
    exit 1
  fi
  echo "latest version: $VERSION"
fi
