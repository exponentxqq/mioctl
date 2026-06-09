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

# ---- determine install dir ----
INSTALL_DIR="${MIOCTL_INSTALL_DIR:-/usr/local/bin}"
echo "install dir: $INSTALL_DIR"

if [ ! -d "$INSTALL_DIR" ]; then
  echo "error: install directory '$INSTALL_DIR' does not exist."
  echo "  Create it first or set MIOCTL_INSTALL_DIR to a writable directory."
  exit 1
fi

# ---- download ----
TMPDIR=$(mktemp -d)
cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

ARCHIVE="mioctl-${VERSION}-${SUFFIX}.tar.gz"
DOWNLOAD_URL="https://github.com/exponentxqq/mioctl/releases/download/${VERSION}/${ARCHIVE}"
CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

echo "downloading $DOWNLOAD_URL ..."
curl -fsSL -o "$TMPDIR/$ARCHIVE" "$DOWNLOAD_URL"

echo "downloading checksum ..."
curl -fsSL -o "$TMPDIR/$ARCHIVE.sha256" "$CHECKSUM_URL" || true

# ---- verify (best effort) ----
if command -v sha256sum >/dev/null 2>&1; then
  echo "verifying checksum..."
  cd "$TMPDIR"
  if sha256sum --check "$ARCHIVE.sha256" --status 2>/dev/null; then
    echo "checksum OK"
  else
    echo "warning: checksum verification failed. Continuing anyway..."
  fi
  cd - >/dev/null
else
  echo "warning: sha256sum not found, skipping checksum verification"
fi

# ---- extract & install ----
echo "extracting..."
tar xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR"

BINARY_NAME="mioctl"
if [ "$os_label" = "windows" ]; then
  BINARY_NAME="mioctl.exe"
fi

if [ -w "$INSTALL_DIR" ]; then
  cp "$TMPDIR/$BINARY_NAME" "$INSTALL_DIR/"
else
  echo "install dir not writable, trying sudo..."
  sudo cp "$TMPDIR/$BINARY_NAME" "$INSTALL_DIR/"
fi
chmod +x "$INSTALL_DIR/$BINARY_NAME"

# ---- create default config ----
CONFIG_DIR="${HOME}/.config/mioctl"
CONFIG_FILE="${CONFIG_DIR}/config.toml"

if [ -f "$CONFIG_FILE" ]; then
  echo ""
  echo "config already exists at $CONFIG_FILE — skipping"
else
  mkdir -p "$CONFIG_DIR"
  cat > "$CONFIG_FILE" <<'TOML'
[mihomo]
external-controller = "127.0.0.1:9090"
secret = ""
TOML
  echo ""
  echo "config created at $CONFIG_FILE"
fi

# ---- summary ----
echo ""
echo "✓ mioctl $VERSION installed to $INSTALL_DIR/$BINARY_NAME"
echo "  Ensure mihomo is running with external-controller enabled"
echo "  Run 'mioctl tui' to start"
if ! command -v mioctl >/dev/null 2>&1; then
  echo "  Note: $INSTALL_DIR may not be in your PATH"
fi
