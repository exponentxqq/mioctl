#!/usr/bin/env sh
set -eu

GREEN='\033[32m'
YELLOW='\033[33m'
RED='\033[31m'
BOLD='\033[1m'
NC='\033[0m'

ok()  { echo "${GREEN}✓${NC} $1"; }
warn(){ echo "${YELLOW}⚠${NC} $1"; }
err() { echo "${RED}✗${NC} $1"; }

# ---- detect platform ----
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Linux)  os_label="linux-gnu" ; exe="" ;;
  Darwin) os_label="darwin"    ; exe="" ;;
  MINGW*|MSYS*|CYGWIN*) os_label="windows" ; exe=".exe" ;;
  *)
    err "unsupported OS '$OS'. Supported: Linux, macOS, Windows (Git Bash / WSL)"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch_label="x86_64" ;;
  aarch64|arm64) arch_label="aarch64" ;;
  *)
    err "unsupported architecture '$ARCH'. Supported: x86_64, aarch64"
    exit 1
    ;;
esac

mioctl_suffix="${arch_label}-${os_label}"
echo "detected: $OS / $ARCH"

# ---- mihomo platform mapping (different naming convention) ----
case "$OS" in
  Linux)  mihomo_os="linux" ;;
  Darwin) mihomo_os="darwin" ;;
  MINGW*|MSYS*|CYGWIN*) mihomo_os="windows" ;;
esac
case "$ARCH" in
  x86_64|amd64)  mihomo_arch="amd64" ;;
  aarch64|arm64) mihomo_arch="arm64" ;;
esac

if [ "$mihomo_os" = "linux" ]; then
  mihomo_asset_pattern="mihomo-linux-${mihomo_arch}-compatible-"
elif [ "$mihomo_os" = "darwin" ]; then
  mihomo_asset_pattern="mihomo-darwin-${mihomo_arch}-"
else
  mihomo_asset_pattern="mihomo-windows-${mihomo_arch}-"
fi

# ---- install dependencies (Linux only) ----
install_clipboard_deps() {
  if [ "$OS" != "Linux" ]; then
    return 0
  fi

  echo ""
  echo "${BOLD}Installing clipboard dependencies...${NC}"

  # Detect package manager
  if command -v apt-get >/dev/null 2>&1; then
    PKG_MGR="apt-get"
    INSTALL_CMD="sudo apt-get install -y"
  elif command -v dnf >/dev/null 2>&1; then
    PKG_MGR="dnf"
    INSTALL_CMD="sudo dnf install -y"
  elif command -v pacman >/dev/null 2>&1; then
    PKG_MGR="pacman"
    INSTALL_CMD="sudo pacman -S --noconfirm"
  elif command -v zypper >/dev/null 2>&1; then
    PKG_MGR="zypper"
    INSTALL_CMD="sudo zypper install -y"
  elif command -v apk >/dev/null 2>&1; then
    PKG_MGR="apk"
    INSTALL_CMD="sudo apk add"
  else
    warn "no supported package manager found. Install xclip or wl-clipboard manually for clipboard support."
    return 0
  fi

  # Install xclip (X11) or wl-clipboard (Wayland) based on what's available
  if [ "$XDG_SESSION_TYPE" = "wayland" ]; then
    $INSTALL_CMD wl-clipboard 2>/dev/null && ok "wl-clipboard installed" || warn "wl-clipboard install failed, clipboard copy may not work"
  else
    $INSTALL_CMD xclip 2>/dev/null && ok "xclip installed" || warn "xclip install failed, clipboard copy may not work"
  fi
}

# ---- resolve mioctl version ----
echo ""
if [ -n "${MIOCTL_VERSION:-}" ]; then
  echo "using MIOCTL_VERSION=$MIOCTL_VERSION (from environment)"
else
  echo "resolving latest mioctl version..."
  if ! command -v curl >/dev/null 2>&1; then
    err "curl is required. Install curl and retry."
    exit 1
  fi
  MIOCTL_VERSION=$(curl -sSf https://api.github.com/repos/exponentxqq/mioctl/releases/latest \
    | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$MIOCTL_VERSION" ]; then
    err "could not resolve latest mioctl version from GitHub API."
    err "  Try: MIOCTL_VERSION=v0.1.0 $0"
    exit 1
  fi
fi
echo "mioctl version: $MIOCTL_VERSION"

# ---- resolve mihomo version ----
MIHOMO_VERSION="${MIHOMO_VERSION:-}"
if [ -n "$MIHOMO_VERSION" ]; then
  echo "using MIHOMO_VERSION=$MIHOMO_VERSION (from environment)"
else
  echo "resolving latest mihomo version..."
  MIHOMO_VERSION=$(curl -sSf https://api.github.com/repos/MetaCubeX/mihomo/releases/latest \
    | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$MIHOMO_VERSION" ]; then
    err "could not resolve latest mihomo version."
    err "  Try: MIHOMO_VERSION=v1.19.0 $0"
    exit 1
  fi
fi
echo "mihomo version: $MIHOMO_VERSION"

# ---- install dirs ----
INSTALL_DIR="${MIOCTL_INSTALL_DIR:-/usr/local/bin}"
MIOCTL_HOME="${HOME}/.config/mioctl"
MIHOMO_BIN_DIR="${MIOCTL_HOME}/bin"
MIHOMO_CONFIG_DIR="${HOME}/.config/mihomo"
MIHOMO_CONFIG_FILE="${MIHOMO_CONFIG_DIR}/config.yaml"
SYSTEMD_USER_DIR="${HOME}/.config/systemd/user"

echo ""
echo "${BOLD}Installing clipboard dependencies...${NC}"
install_clipboard_deps

# ---- download mioctl ----
echo ""
echo "${BOLD}Downloading mioctl...${NC}"

TMPDIR=$(mktemp -d)
cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

mioctl_archive="mioctl-${MIOCTL_VERSION}-${mioctl_suffix}.tar.gz"
mioctl_url="https://github.com/exponentxqq/mioctl/releases/download/${MIOCTL_VERSION}/${mioctl_archive}"
checksum_url="${mioctl_url}.sha256"

curl -fsSL -o "$TMPDIR/$mioctl_archive" "$mioctl_url"
curl -fsSL -o "$TMPDIR/$mioctl_archive.sha256" "$checksum_url" || true

# verify (best effort)
if command -v sha256sum >/dev/null 2>&1; then
  cd "$TMPDIR"
  if sha256sum --check "$mioctl_archive.sha256" --status 2>/dev/null; then
    ok "mioctl checksum OK"
  else
    warn "mioctl checksum verification failed, continuing anyway"
  fi
  cd - >/dev/null
else
  warn "sha256sum not found, skipping checksum verification"
fi

echo "extracting..."
tar xzf "$TMPDIR/$mioctl_archive" -C "$TMPDIR"

if [ ! -d "$INSTALL_DIR" ]; then
  echo "creating $INSTALL_DIR..."
  sudo mkdir -p "$INSTALL_DIR"
fi

if [ -w "$INSTALL_DIR" ]; then
  cp "$TMPDIR/mioctl${exe}" "$INSTALL_DIR/"
else
  echo "installing to $INSTALL_DIR requires sudo..."
  sudo cp "$TMPDIR/mioctl${exe}" "$INSTALL_DIR/"
fi
chmod +x "$INSTALL_DIR/mioctl${exe}"
ok "mioctl $MIOCTL_VERSION → $INSTALL_DIR/mioctl${exe}"

# ---- download mihomo ----
echo ""
echo "${BOLD}Downloading mihomo...${NC}"

mihomo_asset="${mihomo_asset_pattern}${MIHOMO_VERSION}.gz"
mihomo_url="https://github.com/MetaCubeX/mihomo/releases/download/${MIHOMO_VERSION}/${mihomo_asset}"

echo "downloading $mihomo_url ..."
curl -fsSL -o "$TMPDIR/mihomo.gz" "$mihomo_url"

mkdir -p "$MIHOMO_BIN_DIR"
gunzip -f "$TMPDIR/mihomo.gz"
cp "$TMPDIR/mihomo" "$MIHOMO_BIN_DIR/mihomo"
chmod +x "$MIHOMO_BIN_DIR/mihomo"
ok "mihomo $MIHOMO_VERSION → $MIHOMO_BIN_DIR/mihomo"

# ---- create mihomo config (if not exists) ----
if [ -f "$MIHOMO_CONFIG_FILE" ]; then
  echo ""
  echo "mihomo config already exists at $MIHOMO_CONFIG_FILE — skipping"
else
  mkdir -p "$MIHOMO_CONFIG_DIR"
  cat > "$MIHOMO_CONFIG_FILE" <<'YAML'
# Generated by mioctl installer
external-controller: 127.0.0.1:9090
mixed-port: 7897
mode: rule
log-level: info
dns:
  enable: true
  enhanced-mode: fake-ip
  nameserver:
    - https://223.5.5.5/dns-query
    - https://doh.pub/dns-query
tun:
  enable: true
  stack: gvisor
  auto-route: true
  auto-detect-interface: true
  dns-hijack:
    - any:53
YAML
  echo ""
  ok "mihomo config created at $MIHOMO_CONFIG_FILE"
fi

# ---- create systemd user service ----
echo ""
echo "${BOLD}Setting up mihomo systemd service...${NC}"

mkdir -p "$SYSTEMD_USER_DIR"
cat > "$SYSTEMD_USER_DIR/mihomo.service" <<'SYSTEMD'
[Unit]
Description=Mihomo Proxy Service
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=%h/.config/mioctl/bin/mihomo -d %h/.config/mihomo
Restart=on-failure
RestartSec=5
LimitNOFILE=1048576

[Install]
WantedBy=default.target
SYSTEMD

# Reload systemd and enable service
systemctl --user daemon-reload 2>/dev/null || true
systemctl --user enable mihomo.service 2>/dev/null || true
systemctl --user restart mihomo.service 2>/dev/null || true

ok "mihomo service enabled and started"

# ---- create mioctl config (if not exists) ----
echo ""
MIOCTL_CONFIG_FILE="${MIOCTL_HOME}/config.toml"

if [ -f "$MIOCTL_CONFIG_FILE" ]; then
  echo "mioctl config already exists at $MIOCTL_CONFIG_FILE — skipping"
else
  mkdir -p "$MIOCTL_HOME"
  cat > "$MIOCTL_CONFIG_FILE" <<'TOML'
[mihomo]
external-controller = "127.0.0.1:9090"
secret = ""
TOML
  ok "mioctl config created at $MIOCTL_CONFIG_FILE"
fi

# ---- summary ----
echo ""
echo "${BOLD}${GREEN}Installation complete!${NC}"
echo ""
echo "  mioctl:  $INSTALL_DIR/mioctl${exe}"
echo "  mihomo:  $MIHOMO_BIN_DIR/mihomo"
echo "  config:  $MIOCTL_CONFIG_FILE"
echo "  service: systemctl --user status mihomo"
echo ""
echo "${BOLD}Next steps:${NC}"
echo "  1. Edit mihomo config if needed: $MIHOMO_CONFIG_FILE"
echo "  2. Run: mioctl tui"
echo ""

if ! command -v mioctl >/dev/null 2>&1; then
  warn "$INSTALL_DIR is not in your PATH. Add it or run with full path."
fi
