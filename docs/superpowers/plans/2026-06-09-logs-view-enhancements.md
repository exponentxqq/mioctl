# Logs View Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add app-level logging (timestamped info/error messages), visual selection with vim-style v/j/k/y keys, mouse drag-to-select-and-copy, and configurable `app_log_level`

**Architecture:** AppState::add_log() pushes timestamped LogEntry messages filtered by configurable level. UiState gains cursor/visual/selection fields. Logs view renders selection highlighting and cursor tracking with computed scroll. Event loop intercepts v/y/Esc/j/k/g/G for Logs view. arboard provides system clipboard access. Every async spawn calls add_log on completion.

**Tech Stack:** Rust, ratatui, crossterm, arboard, serde

---

### Task 1: Add arboard dependency and app_log_level to Preferences

**Files:**
- Modify: `Cargo.toml:30` (add after dirs)
- Modify: `src/config/mioctl_config.rs:44-72` (Preferences struct)

- [ ] **Step 1: Add arboard to Cargo.toml**

After `dirs = "5"` (line 29), add:

```toml
arboard = "3"
```

- [ ] **Step 2: Add default_app_log_level function**

After `default_theme` (line 60-62), add:

```rust
fn default_app_log_level() -> String {
    "info".into()
}
```

- [ ] **Step 3: Add app_log_level field to Preferences**

Add after `theme` field (line 51):

```rust
    #[serde(default = "default_app_log_level")]
    pub app_log_level: String,
```

- [ ] **Step 4: Update Preferences Default impl**

Add after `theme: default_theme(),` (line 69):

```rust
            app_log_level: default_app_log_level(),
```

- [ ] **Step 5: Add tests**

Add to `mod tests` block after `test_toml_roundtrip`:

```rust
    #[test]
    fn test_default_app_log_level() {
        assert_eq!(Preferences::default().app_log_level, "info");
    }

    #[test]
    fn test_app_log_level_can_change() {
        let mut prefs = Preferences::default();
        prefs.app_log_level = "error".into();
        assert_eq!(prefs.app_log_level, "error");
    }

    #[test]
    fn test_app_log_level_roundtrip() {
        let mut config = MioctlConfig::default();
        config.preferences.app_log_level = "debug".into();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.preferences.app_log_level, "debug");
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test -- config::`
Expected: 6 tests pass

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/config/mioctl_config.rs
git commit -m "feat: add app_log_level config field and arboard dependency"
```

---

### Task 2: Add log_cursor/selection fields to UiState and add_log() to AppState

**Files:**
- Modify: `src/app/state.rs`

- [ ] **Step 1: Move LOG_CAP constant**

Add after `use` block (line 6):

```rust
pub const LOG_CAP: usize = 1000;
```

- [ ] **Step 2: Add log fields to UiState**

Add after `spinner_frame: u8,` (line 67):

```rust
    pub log_cursor: usize,
    pub log_visual: bool,
    pub log_select_start: usize,
    pub log_select_end: usize,
```

- [ ] **Step 3: Update UiState Default**

Add after `spinner_frame: 0,` (line 86):

```rust
            log_cursor: 0,
            log_visual: false,
            log_select_start: 0,
            log_select_end: 0,
```

- [ ] **Step 4: Add LOG_CAP import to app.rs**

Remove `const LOG_CAP` from `src/ui/app.rs` line 22 and add import there:

In `src/ui/app.rs`, change line 16 from:
```rust
use crate::app::state::{ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState};
```
To:
```rust
use crate::app::state::{ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState, LOG_CAP};
```

And delete line 22: `const LOG_CAP: usize = 1000;`

- [ ] **Step 5: Add add_log() method to AppState**

Add after `update_time` (line 160), before the closing `}` of `impl AppState`:

```rust
    /// Push an app-level log entry with timestamp, respecting configured log level.
    pub fn add_log(&mut self, level: &str, msg: &str) {
        let cfg_level = self.config.preferences.app_log_level.as_str();
        if cfg_level == "off" { return; }
        if cfg_level == "info" && level == "debug" { return; }
        if cfg_level == "error" && level != "error" { return; }

        let entry = LogEntry {
            level: level.to_string(),
            payload: format!("[{}] {}", Local::now().format("%H:%M:%S"), msg),
        };
        self.logs.push(entry);
        while self.logs.len() > LOG_CAP {
            self.logs.remove(0);
        }
    }
```

- [ ] **Step 6: Add unit tests**

Add to `mod tests` after `test_loading_set_and_clear`:

```rust
    #[test]
    fn test_add_log_info() {
        let mut state = AppState::new();
        state.add_log("info", "test message");
        assert_eq!(state.logs.len(), 1);
        assert_eq!(state.logs[0].level, "info");
        assert!(state.logs[0].payload.contains("test message"));
        assert!(state.logs[0].payload.contains("["));
        assert!(state.logs[0].payload.contains("]"));
    }

    #[test]
    fn test_add_log_error() {
        let mut state = AppState::new();
        state.add_log("error", "failure");
        assert_eq!(state.logs.len(), 1);
        assert_eq!(state.logs[0].level, "error");
    }

    #[test]
    fn test_add_log_cap() {
        let mut state = AppState::new();
        for i in 0..(LOG_CAP + 10) {
            state.add_log("info", &format!("msg {}", i));
        }
        assert_eq!(state.logs.len(), LOG_CAP);
    }

    #[test]
    fn test_add_log_off() {
        let mut state = AppState::new();
        state.config.preferences.app_log_level = "off".into();
        state.add_log("info", "should not appear");
        assert_eq!(state.logs.len(), 0);
    }

    #[test]
    fn test_add_log_error_only_filters_info() {
        let mut state = AppState::new();
        state.config.preferences.app_log_level = "error".into();
        state.add_log("info", "info msg");
        state.add_log("error", "err msg");
        assert_eq!(state.logs.len(), 1);
        assert_eq!(state.logs[0].level, "error");
    }

    #[test]
    fn test_add_log_debug_passes_all() {
        let mut state = AppState::new();
        state.config.preferences.app_log_level = "debug".into();
        state.add_log("debug", "debug msg");
        state.add_log("info", "info msg");
        state.add_log("error", "err msg");
        assert_eq!(state.logs.len(), 3);
    }

    #[test]
    fn test_log_ui_defaults() {
        let ui = UiState::default();
        assert_eq!(ui.log_cursor, 0);
        assert!(!ui.log_visual);
        assert_eq!(ui.log_select_start, 0);
        assert_eq!(ui.log_select_end, 0);
    }

    #[test]
    fn test_log_visual_selection_range() {
        let mut ui = UiState::default();
        ui.log_cursor = 5;
        ui.log_visual = true;
        ui.log_select_start = 5;
        ui.log_select_end = 10;
        assert!(ui.log_select_start <= ui.log_select_end);
        assert_eq!(ui.log_select_end, 10);
    }
```

- [ ] **Step 7: Run tests**

Run: `cargo test -- state::`
Expected: all tests pass (13 existing + 9 new = 22)

- [ ] **Step 8: Fix app.rs LOG_CAP reference**

After Step 4, the log stream spawn in app.rs (lines 96-98) uses `LOG_CAP` — verify it resolves from the import.

Run: `cargo build`
Expected: compiles

- [ ] **Step 9: Commit**

```bash
git add src/app/state.rs src/ui/app.rs
git commit -m "feat: add log_cursor/selection fields to UiState and add_log() to AppState"
```

---

### Task 3: Add LogVisual and LogCopy actions, update keybindings

**Files:**
- Modify: `src/ui/keybindings.rs`

- [ ] **Step 1: Add Action variants**

Add after `Refresh,` (line 30):

```rust
    LogVisual,
    LogCopy,
```

- [ ] **Step 2: Map v → LogVisual, y → LogCopy**

Add after line 66 (`_ => None,`):

Before `_ => None,` at the end of `parse_key`, add:

```rust
        KeyEvent { code: KeyCode::Char('v'), modifiers: KeyModifiers::NONE, .. } => Some(Action::LogVisual),
        KeyEvent { code: KeyCode::Char('y'), modifiers: KeyModifiers::NONE, .. } => Some(Action::LogCopy),
```

Wait — these should go before the catch-all `_ => None` at line 67. Insert after line 65 (`KeyEvent { code: KeyCode::Char('r'), .. } => Some(Action::Refresh),`):

Actually, let me be more precise. Insert these two lines between the `KeyEvent { code: KeyCode::Char('r'), .. }` line and the `_ => None` line:

```rust
        KeyEvent { code: KeyCode::Char('v'), modifiers: KeyModifiers::NONE, .. } => Some(Action::LogVisual),
        KeyEvent { code: KeyCode::Char('y'), modifiers: KeyModifiers::NONE, .. } => Some(Action::LogCopy),
```

- [ ] **Step 3: Add unit tests**

Add to `mod tests` after `test_help`:

```rust
    #[test] fn test_log_visual() { assert_eq!(parse_key(k('v')), Some(Action::LogVisual)); }
    #[test] fn test_log_copy() { assert_eq!(parse_key(k('y')), Some(Action::LogCopy)); }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -- keybindings::`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add src/ui/keybindings.rs
git commit -m "feat: add LogVisual/LogCopy actions and key mappings"
```

---

### Task 4: Rewrite logs.rs render with cursor, selection, and scroll

**Files:**
- Modify: `src/ui/views/logs.rs`

- [ ] **Step 1: Rewrite the entire logs.rs file**

Replace the file content with:

```rust
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let filtered: Vec<&crate::api::types::LogEntry> = match state.ui.log_level_filter.as_deref() {
        Some(level) => state.logs.iter().filter(|e| e.level == level).collect(),
        None => state.logs.iter().collect(),
    };

    let total = filtered.len();
    let visible = area.height.saturating_sub(3); // borders + title
    let visible = visible.max(1);

    // Compute scroll to keep cursor visible
    let cursor = state.ui.log_cursor.min(total.saturating_sub(1));
    let scroll = if cursor >= total.saturating_sub(1) && total > visible {
        total.saturating_sub(visible)
    } else if total > visible {
        let pos = (cursor + visible / 3).saturating_sub(visible / 3);
        pos.min(total.saturating_sub(visible))
    } else {
        0
    };

    // Build log lines with selection highlighting
    let (sel_start, sel_end) = if state.ui.log_visual
        || state.ui.log_select_start != state.ui.log_select_end
    {
        let s = state.ui.log_select_start.min(state.ui.log_select_end);
        let e = state.ui.log_select_start.max(state.ui.log_select_end);
        (s, e)
    } else {
        (0, 0)
    };

    let log_lines: Vec<Line> = filtered.iter().enumerate().map(|(idx, entry)| {
        let color = match entry.level.as_str() {
            "error" => T.red, "warning" => T.yellow, "debug" => T.text_secondary, _ => T.green,
        };

        let in_selection = state.ui.log_visual && idx >= sel_start && idx <= sel_end;
        let is_cursor = idx == cursor;

        let style = if in_selection || is_cursor {
            Style::default().bg(T.surface).fg(T.text)
        } else {
            Style::default()
        };

        Line::from(vec![
            Span::styled(
                format!("{:5} ", entry.level.to_uppercase()),
                Style::default().fg(color).bg(if in_selection { T.surface } else { T.bg }),
            ),
            Span::styled(&entry.payload, Style::default().fg(T.text).bg(if in_selection { T.surface } else { T.bg })),
        ]).style(style)
    }).collect();

    let paused = if state.ui.log_paused { " [PAUSED]" } else { "" };
    let visual = if state.ui.log_visual { " [VISUAL]" } else { "" };
    let level = state.ui.log_level_filter.as_deref().unwrap_or("all");
    let title = format!(
        "Logs ({}){} | level: {} | s:switch space:pause{}{}",
        total, paused, level,
        if total > 0 { format!(" | {}/{}", cursor + 1, total) } else { String::new() },
        visual,
    );
    let block = Block::default().title(title);
    let para = Paragraph::new(log_lines).block(block).wrap(Wrap { trim: true })
        .scroll((scroll as u16, 0));
    f.render_widget(para, area);
}
```

- [ ] **Step 2: Build check**

Run: `cargo build`
Expected: compiles (app.rs will show errors about unhandled LogVisual/LogCopy — acceptable, fixed in Task 5)

- [ ] **Step 3: Commit**

```bash
git add src/ui/views/logs.rs
git commit -m "feat: add cursor tracking, selection highlighting, and scroll to logs view"
```

---

### Task 5: Wire Logs view keyboard/mouse handling and add_log into app.rs

**Files:**
- Modify: `src/ui/app.rs`

This is the largest task. We need to:
A. Add Logs view key interception in event loop
B. Handle LogVisual, LogCopy, and Logs MoveDown/MoveUp/JumpTop/JumpBottom in handle_action
C. Handle mouse drag for log selection
D. Wire add_log into all spawns

- [ ] **Step 1: Add Logs view key interception in event loop**

After the mode_selector interception block (line 176: `} else if let Some(action) = parse_key(key) {`) and before `parse_key`, add a Logs view interception block.

Replace lines 176-178:
```rust
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await { break; }
                    }
```

With:
```rust
                    } else if s.ui.active_view == Logs && s.ui.log_visual {
                        // Logs visual mode: intercept navigation keys
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => {
                                s.ui.log_select_end = (s.ui.log_select_end + 1)
                                    .min(s.logs.len().saturating_sub(1));
                                s.ui.log_cursor = s.ui.log_select_end;
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if s.ui.log_select_end > s.ui.log_select_start {
                                    s.ui.log_select_end = s.ui.log_select_end.saturating_sub(1);
                                    s.ui.log_cursor = s.ui.log_select_end;
                                }
                            }
                            KeyCode::Char('y') => {
                                let text = collect_log_selection(&s);
                                s.ui.log_visual = false;
                                drop(s);
                                copy_to_clipboard(&text);
                                continue;
                            }
                            KeyCode::Esc => {
                                s.ui.log_visual = false;
                            }
                            _ => {}
                        }
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await { break; }
                    }
```

- [ ] **Step 2: Add collect_log_selection helper**

Add before `run_tui` (after `const LOG_CAP` removal, before `pub async fn run_tui`):

```rust
/// Collect payload text from the selected log range, joined by newlines.
fn collect_log_selection(state: &AppState) -> String {
    let start = state.ui.log_select_start.min(state.ui.log_select_end);
    let end = state.ui.log_select_start.max(state.ui.log_select_end);
    let end = end.min(state.logs.len().saturating_sub(1));
    state.logs[start..=end]
        .iter()
        .map(|e| e.payload.as_str())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Copy text to system clipboard. If clipboard fails, silently ignore.
fn copy_to_clipboard(text: &str) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}
```

The `drop(s);` in Step 1 requires the MutexGuard type to be compatible. Since `s` is `tokio::sync::MutexGuard<AppState>`, we need `drop(s)` to release the lock before `handle_action` tries to lock it again. Actually, the `handle_action` call is inside the same if-else block and already has `&mut s` borrowed. The `drop(s)` and `continue` pattern releases the guard before the `parse_key` branch tries to re-lock.

Wait — actually this won't work because `handle_action` borrows `&mut s` from the lock. The `parse_key` call doesn't lock. Let me restructure.

Actually looking at the current code flow:
1. `let mut s = state.lock().await;` — acquires lock
2. Search mode check, mode selector check, then parse_key + handle_action
3. handle_action returns, s is dropped, lock released

For my visual mode interception, I need to:
- handle `y` → copy text, exit visual mode
- `copy_to_clipboard` doesn't need the lock (only reads from state)
- `drop(s)` releases the lock before we skip to next loop iteration with `continue`

But `collect_log_selection` borrows `&s` — this is fine, it's a read-only borrow.

- [ ] **Step 3: Update handle_action for Logs view j/k/g/G**

In `handle_action`, update the `MoveDown` handler (lines 286-297). The current code:
```rust
        Action::MoveDown => match s.ui.active_view {
            Proxies => { ... }
            Connections => { ... }
            _ => {}
        },
```

Add a `Logs` branch before `_ => {}`:
```rust
            Logs => {
                let m = s.logs.len().saturating_sub(1);
                if s.ui.log_visual {
                    s.ui.log_select_end = (s.ui.log_select_end + 1).min(m);
                    s.ui.log_cursor = s.ui.log_select_end;
                } else {
                    s.ui.log_cursor = (s.ui.log_cursor + 1).min(m);
                }
            }
```

Similarly for `MoveUp` (lines 298-302):
```rust
        Action::MoveUp => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = s.ui.selected_node_idx.saturating_sub(1),
            Connections => s.ui.selected_conn_idx = s.ui.selected_conn_idx.saturating_sub(1),
            _ => {}
        },
```

Add `Logs` branch:
```rust
        Action::MoveUp => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = s.ui.selected_node_idx.saturating_sub(1),
            Connections => s.ui.selected_conn_idx = s.ui.selected_conn_idx.saturating_sub(1),
            Logs => {
                if s.ui.log_visual && s.ui.log_select_end > s.ui.log_select_start {
                    s.ui.log_select_end = s.ui.log_select_end.saturating_sub(1);
                    s.ui.log_cursor = s.ui.log_select_end;
                } else {
                    s.ui.log_cursor = s.ui.log_cursor.saturating_sub(1);
                }
            }
            _ => {}
        },
```

For `JumpTop` (lines 303-307) — add Logs branch:
```rust
        Action::JumpTop => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = 0,
            Connections => s.ui.selected_conn_idx = 0,
            Logs => { s.ui.log_cursor = 0; }
            _ => {}
        },
```

For `JumpBottom` (lines 308-316) — add Logs branch:
```rust
        Action::JumpBottom => match s.ui.active_view {
            Proxies => { ... }
            Connections => s.ui.selected_conn_idx = s.connections.len().saturating_sub(1),
            Logs => { s.ui.log_cursor = s.logs.len().saturating_sub(1); }
            _ => {}
        },
```

- [ ] **Step 4: Add LogVisual and LogCopy handlers**

Add to `match action` in `handle_action`, after `Action::CycleLogLevel` block:

```rust
        Action::LogVisual => {
            if s.ui.active_view == Logs {
                s.ui.log_visual = true;
                s.ui.log_select_start = s.ui.log_cursor;
                s.ui.log_select_end = s.ui.log_cursor;
            }
        }
        Action::LogCopy => {
            if s.ui.active_view == Logs {
                let text = collect_log_selection(s);
                copy_to_clipboard(&text);
            }
        }
```

Wait — `Action::LogCopy` when not in visual mode should copy just the cursor line. Update:

```rust
        Action::LogCopy => {
            if s.ui.active_view == Logs {
                if s.ui.log_visual {
                    let text = collect_log_selection(s);
                    copy_to_clipboard(&text);
                    s.ui.log_visual = false;
                } else if let Some(entry) = s.logs.get(s.ui.log_cursor) {
                    copy_to_clipboard(&entry.payload);
                }
            }
        }
```

- [ ] **Step 5: Handle mouse drag for log selection**

Update the `Event::Mouse` handler (lines 180-191). The current code:
```rust
                Event::Mouse(mouse) => {
                    if let Some(action) = parse_mouse(mouse) {
                        let mut s = state.lock().await;
                        if s.ui.show_help || s.ui.show_settings || s.ui.show_mode_selector {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
                            s.ui.show_mode_selector = false;
                        } else {
                            handle_action(&action, &mut s, state.clone()).await;
                        }
                    }
                }
```

For mouse drag detection on log lines, we need to track mouse state. Simplest approach: when Logs view is active and user clicks in content area, set selection; on drag, extend; on release, copy.

The mouse event provides column and row. We need to determine if the click is in the logs content area (column >= 16, since sidebar is 16 wide).

Replace the `Event::Mouse(mouse)` block with:

```rust
                Event::Mouse(mouse) => {
                    // Logs view: handle line selection via mouse
                    if mouse.kind == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left)
                        || mouse.kind == crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left)
                        || mouse.kind == crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left)
                    {
                        let mut s = state.lock().await;
                        let active_logs = s.ui.active_view == Logs
                            && !s.ui.show_help && !s.ui.show_settings && !s.ui.show_mode_selector;
                        if active_logs && mouse.column > 16 {
                            // Compute which log line was clicked (row within content area)
                            // Content area starts at row 0 of terminal; logs title takes 1 row
                            let log_row = mouse.row.saturating_sub(1) as usize;
                            let total = s.logs.len();
                            let visible = terminal.size().map(|sz| sz.height.saturating_sub(4) as usize).unwrap_or(20);
                            let cursor = s.ui.log_cursor.min(total.saturating_sub(1));
                            let scroll = if cursor >= total.saturating_sub(1) && total > visible {
                                total.saturating_sub(visible)
                            } else if total > visible {
                                (cursor + visible / 3).saturating_sub(visible / 3)
                                    .min(total.saturating_sub(visible))
                            } else {
                                0
                            };
                            let abs_idx = scroll + log_row;
                            if abs_idx < total {
                                match mouse.kind {
                                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                                        s.ui.log_cursor = abs_idx;
                                        s.ui.log_select_start = abs_idx;
                                        s.ui.log_select_end = abs_idx;
                                        s.ui.log_visual = false;
                                    }
                                    crossterm::event::MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                                        s.ui.log_select_end = abs_idx;
                                    }
                                    crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                                        s.ui.log_select_end = abs_idx;
                                        let text = collect_log_selection(&s);
                                        copy_to_clipboard(&text);
                                        s.ui.log_visual = false;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        drop(s);
                        continue;
                    }

                    if let Some(action) = parse_mouse(mouse) {
                        let mut s = state.lock().await;
                        if s.ui.show_help || s.ui.show_settings || s.ui.show_mode_selector {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
                            s.ui.show_mode_selector = false;
                        } else {
                            handle_action(&action, &mut s, state.clone()).await;
                        }
                    }
                }
```

Hmm, using `terminal.size()` inside the event handler is problematic because `terminal` is declared later. Let me simplify the mouse handling to avoid needing terminal size in the event loop.

Simpler approach: use an estimated visible line count (e.g., 20) or compute from the area. Actually, the simplest is to just use the terminal size which we have access to via crossterm's `terminal::size()`.

Wait, actually the mouse row is in screen coordinates. The logs content area starts at row 1 (after the top bar? No — there's no top bar. The sidebar starts at row 0, content area at column 16). The logs title block takes 1 row, then the logs area. The exact row mapping depends on the layout.

Let me simplify: the mouse row relative to content area = `mouse.row - 1` (1 for the block title). Then `scroll + (mouse.row - 1)` gives the absolute log index.

But we don't have `scroll` readily available in the event handler since it's computed in the render function. Let me add a small helper that computes the same scroll or just approximate it.

Actually, the cleanest approach: store a `log_area_height: u16` field in UiState that the render function updates, then use it in the mouse handler. But this adds complexity.

Simplest practical approach: just use a reasonable estimate. Most terminals are 24-40 lines tall. The logs area is roughly `terminal_height - 1 (status bar) - 1 (title) - sidebar padding ≈ terminal_height - 3`.

Let me just store the content area height from the render function. Actually, I think the simplest approach is to compute the scroll in the mouse handler the same way as in the render function. I can refactor the scroll computation into a shared function.

Let me create a helper:

```rust
fn log_scroll_offset(state: &AppState, visible: usize) -> usize {
    let total = state.logs.len();
    if total == 0 { return 0; }
    let cursor = state.ui.log_cursor.min(total.saturating_sub(1));
    if cursor >= total.saturating_sub(1) && total > visible {
        total.saturating_sub(visible)
    } else if total > visible {
        let pos = (cursor + visible / 3).saturating_sub(visible / 3);
        pos.min(total.saturating_sub(visible))
    } else {
        0
    }
}
```

Use this in both `logs::render()` and the mouse handler. The `visible` in the mouse handler can come from `terminal.size()`:

```rust
let terminal_height = crossterm::terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
let visible = terminal_height.saturating_sub(3); // status bar + title + margin
```

Let me restructure. I'll:
1. Add the `log_scroll_offset` helper to app.rs
2. Update logs.rs to use it (or keep it internal — simpler to keep them separate and compute similarly)
3. In mouse handler, use terminal size for visible count, compute scroll from `log_scroll_offset`

Actually, I'm overcomplicating the plan. Let me simplify:

For the mouse handler, approximate visible as `terminal_size.1 - 3` and compute scroll the same way. It's fine if the mapping isn't pixel-perfect — the user clicks roughly on the line they want, and it'll be within 1-2 lines of accuracy.

Let me redo Step 5 with a cleaner approach.

- [ ] **Step 5: Handle mouse selection in Logs view (revised)**

Replace the `Event::Mouse(mouse)` block (lines 180-191):

```rust
                Event::Mouse(mouse) => {
                    use crossterm::event::MouseEventKind;
                    use crossterm::event::MouseButton;

                    let is_click_drag = matches!(mouse.kind,
                        MouseEventKind::Down(MouseButton::Left)
                        | MouseEventKind::Drag(MouseButton::Left)
                        | MouseEventKind::Up(MouseButton::Left)
                    );

                    if is_click_drag {
                        let mut s = state.lock().await;
                        let in_logs = s.ui.active_view == Logs
                            && !s.ui.show_help && !s.ui.show_settings
                            && !s.ui.show_mode_selector;
                        drop(s);

                        if in_logs && mouse.column > 16 {
                            let mut s = state.lock().await;
                            let visible = crossterm::terminal::size()
                                .map(|(_, h)| h.saturating_sub(3) as usize)
                                .unwrap_or(20);
                            let total = s.logs.len();
                            if total == 0 { drop(s); continue; }
                            let cursor = s.ui.log_cursor.min(total.saturating_sub(1));
                            let scroll = if cursor >= total.saturating_sub(1) && total > visible {
                                total.saturating_sub(visible)
                            } else if total > visible {
                                (cursor + visible / 3).saturating_sub(visible / 3)
                                    .min(total.saturating_sub(visible))
                            } else {
                                0
                            };
                            let log_row = mouse.row.saturating_sub(1) as usize;
                            let abs_idx = (scroll + log_row).min(total.saturating_sub(1));

                            match mouse.kind {
                                MouseEventKind::Down(MouseButton::Left) => {
                                    s.ui.log_cursor = abs_idx;
                                    s.ui.log_select_start = abs_idx;
                                    s.ui.log_select_end = abs_idx;
                                    s.ui.log_visual = false;
                                }
                                MouseEventKind::Drag(MouseButton::Left) => {
                                    s.ui.log_select_end = abs_idx;
                                }
                                MouseEventKind::Up(MouseButton::Left) => {
                                    s.ui.log_select_end = abs_idx;
                                    let text = collect_log_selection(&s);
                                    drop(s);
                                    copy_to_clipboard(&text);
                                    continue;
                                }
                                _ => {}
                            }
                            drop(s);
                            continue;
                        }
                    }

                    if let Some(action) = parse_mouse(mouse) {
                        let mut s = state.lock().await;
                        if s.ui.show_help || s.ui.show_settings || s.ui.show_mode_selector {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
                            s.ui.show_mode_selector = false;
                        } else {
                            handle_action(&action, &mut s, state.clone()).await;
                        }
                    }
                }
```

- [ ] **Step 6: Wire add_log into all spawns (mode switch)**

In the mode selector handler (lines 162-169), replace:
```rust
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        let _ = ProxyManager::set_proxy_mode(client, &target).await;
                                        refresh_state(&s2).await;
                                    }
                                    let mut s = s2.lock().await;
                                    s.ui.loading = None;
                                });
```

With:
```rust
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        match ProxyManager::set_proxy_mode(client, &target).await {
                                            Ok(()) => {
                                                refresh_state(&s2).await;
                                                let mut s = s2.lock().await;
                                                s.add_log("info", &format!("Mode switched to {:?}", target));
                                                s.ui.loading = None;
                                            }
                                            Err(e) => {
                                                let mut s = s2.lock().await;
                                                s.add_log("error", &format!("Failed to switch mode: {}", e));
                                                s.ui.loading = None;
                                            }
                                        }
                                    } else {
                                        let mut s = s2.lock().await;
                                        s.ui.loading = None;
                                    }
                                });
```

- [ ] **Step 7: Wire add_log into Refresh spawn**

Replace lines 272-278:
```rust
            tokio::spawn(async move {
                if c.is_some() {
                    refresh_state(&shared2).await;
                }
                let mut s = shared2.lock().await;
                s.ui.loading = None;
            });
```

With:
```rust
            tokio::spawn(async move {
                if c.is_some() {
                    refresh_state(&shared2).await;
                } else {
                    let mut s = shared2.lock().await;
                    s.add_log("error", "Refresh failed: no client");
                    s.ui.loading = None;
                    return;
                }
                let mut s = shared2.lock().await;
                s.ui.loading = None;
            });
```

- [ ] **Step 8: Wire add_log into ToggleProxy spawn**

Replace lines 335-351:
```rust
                tokio::spawn(async move {
                    if any_active {
                        if tun_enabled {
                            let _ = c.patch_configs(
                                serde_json::json!({"tun": {"enable": false}})
                            ).await;
                        }
                        crate::os::proxy::clear_system_proxy();
                    } else {
                        let _ = c.patch_configs(
                            serde_json::json!({"tun": {"enable": true}})
                        ).await;
                    }
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
```

With:
```rust
                tokio::spawn(async move {
                    let result: Result<(), crate::api::error::ApiError> = if any_active {
                        if tun_enabled {
                            c.patch_configs(
                                serde_json::json!({"tun": {"enable": false}})
                            ).await
                        } else {
                            Ok(())
                        };
                        crate::os::proxy::clear_system_proxy();
                        Ok(())
                    } else {
                        c.patch_configs(
                            serde_json::json!({"tun": {"enable": true}})
                        ).await
                    };
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    match result {
                        Ok(()) => s.add_log("info",
                            if any_active { "Proxy disabled" } else { "TUN enabled" }),
                        Err(e) => s.add_log("error", &format!("Failed to toggle: {}", e)),
                    }
                    s.ui.loading = None;
                });
```

Wait, the ToggleProxy logic is more complex with the `clear_system_proxy` call that always succeeds. Let me simplify:

```rust
                tokio::spawn(async move {
                    let result = if any_active {
                        if tun_enabled {
                            c.patch_configs(serde_json::json!({"tun": {"enable": false}})).await
                        } else {
                            Ok(())
                        }
                    } else {
                        c.patch_configs(serde_json::json!({"tun": {"enable": true}})).await
                    };
                    if any_active {
                        crate::os::proxy::clear_system_proxy();
                    }
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    match result {
                        Ok(()) => s.add_log("info",
                            if any_active && tun_enabled { "TUN disabled" }
                            else if any_active { "Proxy disabled" }
                            else { "TUN enabled" }),
                        Err(e) => s.add_log("error", &format!("Failed to toggle: {}", e)),
                    }
                    s.ui.loading = None;
                });
```

- [ ] **Step 9: Wire add_log into SwitchNode spawn**

Replace lines 362-367:
```rust
                tokio::spawn(async move {
                    let _ = ProxyManager::switch_node(&c, &gn, &nn).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
```

With:
```rust
                tokio::spawn(async move {
                    match ProxyManager::switch_node(&c, &gn, &nn).await {
                        Ok(()) => {
                            refresh_state(&shared2).await;
                            let mut s = shared2.lock().await;
                            s.add_log("info", &format!("Switched to {}", nn));
                            s.ui.loading = None;
                        }
                        Err(e) => {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Failed to switch node: {}", e));
                            s.ui.loading = None;
                        }
                    }
                });
```

- [ ] **Step 10: Wire add_log into TestNodeDelay spawn**

Replace lines 379-384:
```rust
                tokio::spawn(async move {
                    let _ = ProxyManager::test_node_delay(&c, &n, &url, timeout).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
```

With:
```rust
                tokio::spawn(async move {
                    let delay = ProxyManager::test_node_delay(&c, &n, &url, timeout).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    match delay {
                        Ok(_) => s.add_log("info", &format!("Delay: {} tested", n)),
                        Err(e) => s.add_log("error", &format!("Delay test failed: {}", e)),
                    }
                    s.ui.loading = None;
                });
```

- [ ] **Step 11: Wire add_log into TestGroupDelay spawn**

Replace lines 396-400:
```rust
                tokio::spawn(async move {
                    let _ = ProxyManager::test_group_delay(&c, &g, &url, timeout).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
```

With:
```rust
                tokio::spawn(async move {
                    match ProxyManager::test_group_delay(&c, &g, &url, timeout).await {
                        Ok(_) => {
                            refresh_state(&shared2).await;
                            let mut s = shared2.lock().await;
                            s.add_log("info", "Group delay test done");
                            s.ui.loading = None;
                        }
                        Err(e) => {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Group delay test failed: {}", e));
                            s.ui.loading = None;
                        }
                    }
                });
```

- [ ] **Step 12: Wire add_log into CloseConnection spawn**

Replace lines 414-416:
```rust
            if let (Some(c), Some(id)) = (client, id) {
                tokio::spawn(async move { let _ = ConnectionManager::close_one(&c, &id).await; });
            }
```

With:
```rust
            if let (Some(c), Some(id)) = (client, id) {
                let shared2 = shared.clone();
                let id2 = id.clone();
                tokio::spawn(async move {
                    match ConnectionManager::close_one(&c, &id).await {
                        Ok(_) => {
                            let mut s = shared2.lock().await;
                            s.add_log("info", &format!("Closed {}", id2));
                        }
                        Err(e) => {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Failed to close: {}", e));
                        }
                    }
                });
            }
```

- [ ] **Step 13: Wire add_log into CloseAllConnections spawn**

Replace lines 418-421:
```rust
        Action::CloseAllConnections => {
            if let Some(c) = client {
                tokio::spawn(async move { let _ = ConnectionManager::close_all(&c).await; });
            }
        }
```

With:
```rust
        Action::CloseAllConnections => {
            if let Some(c) = client {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    match ConnectionManager::close_all(&c).await {
                        Ok(_) => {
                            let mut s = shared2.lock().await;
                            s.add_log("info", "All connections closed");
                        }
                        Err(e) => {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Failed to close all: {}", e));
                        }
                    }
                });
            }
        }
```

- [ ] **Step 14: Wire add_log into UpdateSubs spawn**

Replace lines 437-444:
```rust
            tokio::spawn(async move {
                if let Some(ref client) = c {
                    let _ = SubscriptionManager::update_all(&mut cfg, client).await;
                }
                let mut state = shared2.lock().await;
                state.config = cfg;
                state.ui.loading = None;
            });
```

With:
```rust
            tokio::spawn(async move {
                let result = if let Some(ref client) = c {
                    SubscriptionManager::update_all(&mut cfg, client).await
                } else {
                    Err("no client".into())
                };
                let mut state = shared2.lock().await;
                state.config = cfg;
                match result {
                    Ok(_) => state.add_log("info", "Subscriptions updated"),
                    Err(e) => state.add_log("error", &format!("Subscription update failed: {}", e)),
                }
                state.ui.loading = None;
            });
```

- [ ] **Step 15: Build check**

Run: `cargo build`
Expected: all compiles, no errors

- [ ] **Step 16: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire Logs view keyboard/mouse handling and add_log into all spawns"
```

---

### Task 6: Run full test suite + clippy

**Files:** All

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: zero warnings

- [ ] **Step 3: Fix any issues and commit**

```bash
git add -A
git commit -m "chore: fix any test/clippy issues from logs enhancements"
```

---

### Summary

| Task | Files | Description |
|------|-------|-------------|
| 1 | `Cargo.toml`, `mioctl_config.rs` | arboard deps + app_log_level config |
| 2 | `state.rs`, `app.rs` | log_cursor/selection fields + add_log() + LOG_CAP |
| 3 | `keybindings.rs` | LogVisual/LogCopy actions + key mappings |
| 4 | `logs.rs` | Rewrite render with cursor/selection/scroll |
| 5 | `app.rs` | Event loop interception, mouse drag, add_log in all spawns |
| 6 | all | Full test suite + clippy verification |
