# One-Click Install for mioctl

## Goal

A single command installs mioctl on Linux, macOS, and Windows, with a default
config ready to use.

```bash
curl -fsSL https://raw.githubusercontent.com/exponentxqq/mioctl/main/install.sh | sh
```

## Scope

- CI pipeline that builds release binaries and publishes them to GitHub Releases
- `install.sh` script that downloads the correct binary and creates a default config
- Cross-platform: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64 via Git Bash / WSL)

Out of scope (future work): crates.io publishing, Homebrew formula, AUR package, scoop, shell
completions.

---

## CI Pipeline

**Trigger**: push of an annotated tag matching `v*.*.*` (e.g. `v0.2.0`).

**Workflow file**: `.github/workflows/release.yml`

**Build matrix**:

| OS          | Target                 | Binary name suffix     |
|-------------|------------------------|------------------------|
| ubuntu-latest | x86_64-unknown-linux-gnu   | x86_64-linux-gnu   |
| ubuntu-latest | aarch64-unknown-linux-gnu  | aarch64-linux-gnu  |
| macos-latest  | x86_64-apple-darwin        | x86_64-darwin      |
| macos-latest  | aarch64-apple-darwin       | aarch64-darwin     |
| windows-latest| x86_64-pc-windows-msvc     | x86_64-windows     |

For the aarch64-linux target, use `cross` or install the `aarch64-unknown-linux-gnu` target
via rustup + a cross-compiler.

**Steps** (per job):
1. Checkout at tag ref
2. Install Rust toolchain (stable)
3. `cargo build --release --target <target>`
4. Generate SHA256 checksum: `sha256sum target/<target>/release/mioctl[.exe] > mioctl-<version>-<suffix>.sha256`
5. Archive: `tar czf mioctl-<version>-<suffix>.tar.gz -C target/<target>/release mioctl[.exe]`
6. Upload archive + checksum as job artifacts

**Release job** (runs after all matrix jobs complete):
1. Download all artifacts
2. Create GitHub Release with `softprops/action-gh-release`
   - Upload all `.tar.gz` and `.sha256` files
   - Release body: brief changelog or placeholder
   - Mark as latest

---

## install.sh

Hosted in the repo root. Users run:

```bash
curl -fsSL https://raw.githubusercontent.com/exponentxqq/mioctl/main/install.sh | sh
```

Or with a specific version:

```bash
VERSION=v0.2.0 curl -fsSL https://.../install.sh | sh
```

### Logic

```
1. DETECT PLATFORM
   - OS: uname -s → Linux | Darwin | MINGW* (or MSYS*)
   - ARCH: uname -m → x86_64 | aarch64
   - Map to the right binary suffix from the CI matrix
   - If unsupported combo, print error and exit

2. RESOLVE VERSION
   - If $VERSION is set, use it
   - Otherwise fetch latest tag via GitHub API:
     curl -s https://api.github.com/repos/exponentxqq/mioctl/releases/latest | grep tag_name

3. DETERMINE INSTALL DIR
   - Default: /usr/local/bin
   - Environment variable $MIOCTL_INSTALL_DIR can override
   - If not writable by current user, try sudo, else prompt user

4. DOWNLOAD
   - URL: https://github.com/exponentxqq/mioctl/releases/download/$VERSION/mioctl-$VERSION-$SUFFIX.tar.gz
   - Download to temp dir (mktemp -d)
   - Download SHA256 checksum file

5. VERIFY (best effort)
   - If sha256sum is available, verify the archive
   - If not, print a warning and skip

6. EXTRACT & INSTALL
   - tar xzf archive
   - cp mioctl to $INSTALL_DIR/mioctl
   - chmod +x $INSTALL_DIR/mioctl

7. CREATE DEFAULT CONFIG (if not exists)
   - Target: ~/.config/mioctl/config.toml
   - If already present, skip (do not overwrite)
   - Write:
     [mihomo]
     external-controller = "127.0.0.1:9090"
     secret = ""
   - Create parent directories if needed

8. PRINT SUMMARY
   - ✓ mioctl $VERSION installed to /usr/local/bin/mioctl
   - ✓ config created at ~/.config/mioctl/config.toml  (or "skipped, already exists")
   - Next steps:
     - Ensure mihomo is running with external-controller enabled
     - Run 'mioctl tui'
```

### Edge Cases

| Situation | Behavior |
|-----------|----------|
| Unsupported OS/arch | Print supported list, exit 1 |
| GitHub API rate-limited | Fall back to unauthenticated; if still fails, instruct user to set VERSION= manually |
| Download fails | Print URL and HTTP status, exit 1 |
| `~/.config/mioctl/config.toml` already exists | Skip config creation, print notice |
| No `sha256sum` binary | Skip verification, print warning |
| Install dir not writable, no sudo | Print dir and suggest manual chown or MIOCTL_INSTALL_DIR |
| curl not available | Suggest installing curl, exit 1 |

### Security

- `curl | sh` is a known concern. The script prints the version and install path
  before doing anything, so the user can Ctrl-C if it looks wrong.
- SHA256 verification is best-effort (depends on `sha256sum` being available).
  Future improvement: sign releases with GPG / cosign.

---

## Testing

- **CI pipeline**: push a test tag (e.g. `v0.1.0-test`) and verify the workflow
  completes and produces a draft release with all 5 archives.
- **install.sh**: test manually on Linux (native), macOS (if available), and
  Windows Git Bash. Test scenarios:
  - Fresh install (no existing config)
  - Config already exists (should skip)
  - Specific version via VERSION=
  - Install dir override via MIOCTL_INSTALL_DIR
