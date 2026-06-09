# One-Click Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide a single `curl \| sh` command that installs mioctl on Linux/macOS/Windows (via Git Bash/WSL) with a default config file.

**Architecture:** Two artifacts — a GitHub Actions workflow (`.github/workflows/release.yml`) that builds and publishes cross-platform binaries on tag push, and an `install.sh` script in the repo root that detects the platform, downloads the matching binary from GitHub Releases, installs to `/usr/local/bin`, and creates a default `~/.config/mioctl/config.toml`.

**Tech Stack:** GitHub Actions CI, POSIX shell script, `softprops/action-gh-release` for release management.

---

### Task 1: CI Release Workflow (Build Matrix)

**Files:**
- Create: `.github/workflows/release.yml`

- [ ] **Step 1: Write the workflow skeleton with matrix strategy**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*.*.*'

jobs:
  build:
    name: Build ${{ matrix.target }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            suffix: x86_64-linux-gnu
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            suffix: aarch64-linux-gnu
          - os: macos-latest
            target: x86_64-apple-darwin
            suffix: x86_64-darwin
          - os: macos-latest
            target: aarch64-apple-darwin
            suffix: aarch64-darwin
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            suffix: x86_64-windows

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Install cross-compiler (aarch64-linux only)
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Prepare artifacts (Linux/macOS)
        if: runner.os != 'Windows'
        run: |
          cd target/${{ matrix.target }}/release
          VERSION=${GITHUB_REF#refs/tags/}
          ARCHIVE="mioctl-${VERSION}-${{ matrix.suffix }}.tar.gz"
          tar czf "$ARCHIVE" mioctl
          sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"
          echo "ARCHIVE=$ARCHIVE" >> $GITHUB_ENV

      - name: Prepare artifacts (Windows)
        if: runner.os == 'Windows'
        run: |
          cd target/${{ matrix.target }}/release
          $VERSION = $env:GITHUB_REF -replace 'refs/tags/', ''
          $ARCHIVE = "mioctl-${VERSION}-${{ matrix.suffix }}.tar.gz"
          tar czf "$ARCHIVE" mioctl.exe
          sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"
          echo "ARCHIVE=$ARCHIVE" >> $env:GITHUB_ENV

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: mioctl-${{ matrix.suffix }}
          path: |
            target/${{ matrix.target }}/release/mioctl-*.tar.gz
            target/${{ matrix.target }}/release/mioctl-*.tar.gz.sha256
```

- [ ] **Step 2: Commit the workflow skeleton**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add build matrix for cross-platform release"
```

---

### Task 2: CI Release Workflow (GitHub Release Job)

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Append the release job at the end of the workflow file**

```yaml
  release:
    name: Create GitHub Release
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write

    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Flatten artifact files
        run: |
          mkdir dist
          find artifacts -type f -name '*.tar.gz' -exec cp {} dist/ \;
          find artifacts -type f -name '*.sha256' -exec cp {} dist/ \;
          ls -la dist/

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: dist/*
          generate_release_notes: true
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add GitHub Release job"
```

---

### Task 3: install.sh — Platform Detection & Version Resolution

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Write the detection and version resolution logic**

```bash
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
```

- [ ] **Step 2: Commit**

```bash
git add install.sh && chmod +x install.sh
git commit -m "feat: add install.sh — platform detection and version resolution"
```

---

### Task 4: install.sh — Download, Verify, Install, Config

**Files:**
- Modify: `install.sh`

- [ ] **Step 1: Append download, verify, install, and config logic**

```bash
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
```

- [ ] **Step 2: Commit**

```bash
git add install.sh
git commit -m "feat: add download, verify, install, and config to install.sh"
```

---

### Task 5: Validate & Push

- [ ] **Step 1: Static check — shell syntax**

```bash
sh -n install.sh && echo "syntax OK"
```

- [ ] **Step 2: Static check — YAML syntax**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML OK"
```

- [ ] **Step 3: Commit final state and push**

```bash
git add install.sh .github/workflows/release.yml
git commit -m "feat: one-click install via curl | sh"  # or amend previous commits
git push origin main
```

- [ ] **Step 4: Create and push a test tag to trigger CI**

```bash
git tag v0.1.0
git push origin v0.1.0
```

Observe the workflow run at: `https://github.com/exponentxqq/mioctl/actions`. Verify that:
- All 5 build jobs complete successfully
- The release job creates a GitHub Release with 5 `.tar.gz` + 5 `.sha256` files

- [ ] **Step 5: Manual test of install.sh on this machine**

```bash
# Simulate fresh install to a temp dir
MIOCTL_INSTALL_DIR=$(mktemp -d) bash install.sh

# Verify binary is executable
$MIOCTL_INSTALL_DIR/mioctl --help

# Verify config was created (or skipped if already exists)
ls -la ~/.config/mioctl/config.toml
```
