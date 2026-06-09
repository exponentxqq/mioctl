# Logs View Enhancements Design Spec

**Date:** 2026-06-09
**Status:** Approved
**Scope:** App-level logging to Logs view + visual selection and clipboard copy

## Problem

1. Application-level events (API call results, operation outcomes) have no visibility — errors are swallowed silently (`let _ = ...`), successes are invisible
2. Log entries cannot be selected or copied — users can't extract useful debug information from the Logs view

## Solution

Two features that share the Logs view:

**App-level logging**: `AppState::add_log()` pushes timestamped `LogEntry` messages with `info`/`error` level. Every async spawn calls it on completion.

**Visual selection + copy**: vim-style `v`/`j`/`k`/`y` key sequence for keyboard selection, mouse drag for direct selection, system clipboard via `arboard`.

## Data Model

### UiState New Fields (`src/app/state.rs`)

```rust
pub log_cursor: usize,       // current highlighted line (absolute index into state.logs)
pub log_visual: bool,        // visual selection mode active
pub log_select_start: usize, // selection start (absolute index)
pub log_select_end: usize,   // selection end (absolute index)
```

Defaults: all `0`, `log_visual: false`.

### AppState::add_log (`src/app/state.rs`)

```rust
impl AppState {
    pub fn add_log(&mut self, level: &str, msg: &str) {
        let entry = LogEntry {
            level: level.to_string(),
            payload: format!("[{}] {}", Local::now().format("%H:%M:%S"), msg),
        };
        self.logs.push(entry);
        while self.logs.len() > LOG_CAP {
            self.logs.remove(0);
        }
    }
}
```

`LOG_CAP` constant (1000) moves from `app.rs` to `state.rs`.

### New Dependency

```toml
arboard = "3"
```

## Interaction Design

### Keyboard (Logs view only — overrides global MoveUp/MoveDown)

| Key | Behavior |
|-----|----------|
| `j` / `↓` | Non-visual: move cursor down. Visual: extend selection downward |
| `k` / `↑` | Non-visual: move cursor up. Visual: shrink/collapse selection |
| `v` | Enter visual mode, set `select_start = select_end = cursor` |
| `y` | Copy `state.logs[select_start..=select_end]` payloads to system clipboard via arboard, exit visual mode, clear selection |
| `Esc` | Exit visual mode without copying, clear selection |
| `g` | Jump cursor to top (first log entry) |
| `G` | Jump cursor to bottom (last log entry) |

### Mouse

| Event | Behavior |
|-------|----------|
| Left click on log line | Set cursor to clicked line, exit visual, clear selection |
| Left drag over log lines | Select range, highlight as you drag |
| Left release (after drag) | Copy selected range to clipboard, clear visual/selection |

Note: Mouse event handling needs `EnableMouseCapture` which is already used. Mouse drag events require checking column position — only handle drag if starting inside the log content area.

### Auto-scroll Rules

- **Cursor at bottom** (last log entry): new entries auto-scroll the view to stay at bottom
- **Cursor not at bottom**: view position locks, user is inspecting older entries. No auto-scroll.
- When cursor moves beyond visible area, scroll to make it visible
- `g` / `G` scrolls to top/bottom, setting cursor accordingly

### Visual Selection Rendering

- Lines within `[select_start, select_end]` rendered with reversed/inverted background style
- In visual mode, a `[VISUAL]` indicator appears in the title bar
- Selected range (start and end) remains valid even as new entries arrive at the tail

## App-Level Logging Integration

### Per-Operation Log Messages

| Operation | Success (info) | Failure (error) |
|-----------|---------------|-----------------|
| SwitchMode | `Mode switched to <mode>` | `Failed to switch mode: <err>` |
| SwitchNode | `Switched to <name>` | `Failed to switch node: <err>` |
| ToggleProxy | `TUN enabled` / `TUN disabled` / `Proxy disabled` | `Failed to toggle: <err>` |
| Refresh | (none — silent) | `Refresh failed: <err>` |
| TestNodeDelay | `Delay: <node> = <ms>ms` | `Delay test failed: <err>` |
| TestGroupDelay | `Group delay test done` | `Group delay test failed: <err>` |
| UpdateSubs | `Subscriptions updated` | `Subscription update failed: <err>` |
| CloseConnection | `Closed <id>` | `Failed to close: <err>` |
| CloseAllConnections | `All connections closed` | `Failed to close all: <err>` |

### Code Pattern

Each spawn follows this pattern:

```rust
s.ui.loading = Some(LoadingKind::SwitchNode);
let s2 = shared.clone();
tokio::spawn(async move {
    let result = some_operation(&c, &arg).await;
    match result {
        Ok(()) => {
            refresh_state(&s2).await;
            let mut s = s2.lock().await;
            s.add_log("info", "Success message");
            s.ui.loading = None;
        }
        Err(e) => {
            let mut s = s2.lock().await;
            s.add_log("error", &format!("Failed: {}", e));
            s.ui.loading = None;
        }
    }
});
```

Note: `add_log` and `loading` clear happen in the same lock for atomicity. `refresh_state` is only called on success (preserving existing behavior where possible).

## Files Modified

| File | Changes |
|------|---------|
| `src/app/state.rs` | Add `add_log()`, 4 new UiState fields (`log_cursor`, `log_visual`, `log_select_start`, `log_select_end`), move `LOG_CAP` |
| `src/ui/views/logs.rs` | Rewrite render: selection highlighting, cursor line highlight, scroll management, mouse click position tracking |
| `src/ui/app.rs` | Handle `v`/`y`/`Esc` in Logs view context, mouse drag handling, wire `add_log` into all spawns |
| `src/ui/keybindings.rs` | Add `Action::LogVisual`, `Action::LogCopy` variants, map `v`/`y` when active view is Logs |
| `Cargo.toml` | Add `arboard = "3"` dependency |

5 files total (1 new dependency, 0 new files).

## Scroll Implementation

The logs view uses ratatui `Paragraph::scroll()`. Scroll position is derived from cursor + total + viewport height on each render — no persistent scroll offset field needed.

```rust
let total = filtered.len();
let visible = area.height.saturating_sub(2); // minus borders

let scroll = if log_cursor >= total.saturating_sub(1) {
    // Cursor at tail: show latest entries
    total.saturating_sub(visible)
} else {
    // Cursor inspecting older entry: keep it visible near top-third of view
    (log_cursor + visible / 3).saturating_sub(visible / 3)
        .min(total.saturating_sub(visible.min(total)))
};
```

## Clipboard

`arboard` `Clipboard::new()` for Linux (X11/Wayland). `set_text()` for copy. No need for `get_text()`.

Error handling: if clipboard access fails (e.g., Wayland without wl-clipboard), log the error and continue — copy failure is non-fatal.

## Out of Scope

- Scroll with mouse wheel (future work)
- Horizontal scroll for long log lines
- Search within logs (already partially supported via `/` search at app level)
- Persistent log file (logs are in-memory only)
- Copy format customization (raw payload only, no level/color metadata)
