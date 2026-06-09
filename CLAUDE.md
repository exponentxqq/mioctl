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

Integration tests use `wiremock` and live in `tests/integration_test.rs` (API) and `tests/sub_test.rs` (subscription parsing). Unit tests are inline `#[cfg(test)] mod tests` in each source file.

## Architecture

### Module Layout (`src/`)

- **`api/`** — `MihomoClient` REST + WebSocket endpoints, typed request/response structs, error types
- **`app/`** — Business logic: `ProxyManager` (node switching, delay tests, mode cycling), `ConnectionManager`, `AppState`
- **`ui/`** — TUI layer: event loop (`app.rs`), keybindings → `Action` enum, views (dashboard/proxies/connections/rules/logs), sidebar, widgets, catppuccin theme
- **`subscription/`** — Subscription fetch, format auto-detection (YAML/Base64/URI), parser, injector (writes proxy-provider YAML for mihomo)
- **`config/`** — `MioctlConfig` in TOML at `~/.config/mioctl/config.toml`, auto-creates defaults
- **`os/`** — Linux system proxy via `~/.config/environment.d/proxy.conf`
- **`cli/`** — clap CLI (tui/sub/connect subcommands)

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

## Config

- User config: `~/.config/mioctl/config.toml`
- Provider dir: `~/.config/mioctl/providers/`
- System proxy: `~/.config/environment.d/proxy.conf`
- mihomo must have `external-controller` enabled; `secret` is optional
