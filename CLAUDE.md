# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

mioctl — terminal management TUI for [mihomo](https://github.com/MetaCubeX/mihomo) (Clash.Meta). Rust async TUI with REST API, WebSocket streams, and subscription management.

## Commands

```bash
cargo build                  # debug build
cargo build --release        # release build
cargo test                   # all tests (unit + integration + subscription)
cargo test -- test_name      # run specific test by name
cargo test os::proxy         # run tests matching pattern
cargo clippy -- -D warnings  # lint
cargo fmt --check            # format check
```

Integration tests use `wiremock` and live in `tests/integration_test.rs` (API), `tests/profiles_test.rs` (subscription profile lifecycle), and `tests/sub_test.rs` (subscription parsing, requires network). Unit tests are inline `#[cfg(test)] mod tests` in each source file; tests set `MIOCTL_HOME` to a temp dir and `MIOCTL_TEST_NO_SYSTEMCTL=1` to stay hermetic.

## Architecture

### Module Layout (`src/`)

- **`api/`** — `MihomoClient` REST + WebSocket endpoints, typed request/response structs, error types
- **`app/`** — Business logic: `ProxyManager` (node switching, delay tests, mode cycling), `ConnectionManager`, `AppState`
- **`ui/`** — TUI layer: event loop (`app.rs`), keybindings → `Action` enum, views (dashboard/proxies/connections/rules/logs/subscriptions, key 6), sidebar, widgets, catppuccin theme
- **`subscription/`** — Subscription profiles (single-active, clash-verge style): fetch, format
  auto-detection (YAML/Base64/URI), normalize-to-YAML archive in `~/.config/mioctl/profiles/`,
  activation merges proxies/proxy-groups/rules verbatim into mihomo config (those three sections
  are fully managed by mioctl — manual edits are overwritten), backup/rollback, reload,
  all other top-level keys are preserved (only proxy-providers is removed);
  the subscription's `dns.nameserver-policy` is merged into the config's dns
  (subscription wins per-domain) so airport resolvers keep steering node domains
- **`config/`** — `MioctlConfig` in TOML at `~/.config/mioctl/config.toml`, auto-creates defaults
- **`os/`** — Linux system proxy via `~/.config/environment.d/proxy.conf`
- **`cli/`** — clap CLI (tui/sub/connect/doctor subcommands; `sub` = list/add/register(alias)/use/update/remove; `doctor` = run/nodes)

### Key Patterns

**Shared State:** `Arc<Mutex<AppState>>` (alias `SharedState`). All UI updates and background tasks go through this. Lock briefly, clone what you need, release before async work.

**Async TUI Event Loop** (`src/ui/app.rs`):
1. 100ms poll for crossterm events
2. Parse key/mouse → `Action` enum
3. `handle_action` dispatches — async operations spawn `tokio::spawn` tasks, never block the render loop
4. Render: sidebar + active view + status bar + overlays

**Concurrent Data Fetching:** `tokio::join!` with 3s timeout per request. Both init and `refresh_state()` fetch all endpoints in parallel, then lock state once to write results.

**Mihomo API Notes:**
- `/proxies` returns all proxies; groups have non-empty `.all` field (extracted via `extract_groups()`, sorted by name)
- `/group` returns `{"proxies": [...]}` array format (not an object) — do not use for group listing
- Flag emoji in node names (🇯🇵) converted to `[JP]` via `ui::util::readable_name()` for terminal compatibility
- WebSocket streams return `mpsc::Receiver` for async iteration

**Action Handling:** Mutations (SwitchNode, CycleMode, ToggleProxy, etc.) spawn background tasks that call `refresh_state()` on success to update UI immediately.

## Release

Version tags MUST use the `v` prefix (`v0.6.0`), never a bare `0.6.0`:

1. `release.yml` CI only triggers on `v*.*.*` tags — it builds 4 targets
   (x86_64-linux-gnu, x86_64-darwin, aarch64-darwin, x86_64-windows), packages
   `mioctl-<tag>-<target>.tar.gz` + `.sha256` assets, and creates the GitHub Release.
2. `install.sh` resolves the version from the GitHub `/releases/latest` API — a
   bare tag never becomes a Release, so installs stay stuck on the last
   `v`-prefixed version.

Release flow: bump `Cargo.toml` → commit `chore: bump version to X.Y.Z` →
`git tag vX.Y.Z && git push origin main vX.Y.Z` → CI builds and publishes.

Historical note: bare tags `0.4.1`–`0.5.0` have no Release (predate this rule).

## Config

- User config: `~/.config/mioctl/config.toml`
- Subscription archives: `~/.config/mioctl/profiles/*.yaml` (one per subscription, `[subscriptions].active` marks the current one)
- System proxy: `~/.config/environment.d/proxy.conf`
- mihomo must have `external-controller` enabled; `secret` is optional
