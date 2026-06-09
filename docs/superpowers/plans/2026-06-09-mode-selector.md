# Mode Selector Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace blind `m` key mode cycling with a centered popup selector showing all three proxy modes with descriptions and arrow-key navigation.

**Architecture:** Follow existing popup overlay pattern (help/settings). Add `show_mode_selector` + `mode_selector_idx` to `UiState`, create `src/ui/views/mode_selector.rs` rendering a centered block with styled list items, intercept keys above `parse_key` dispatch when the selector is open (same pattern as `search_mode`), and replace `cycle_proxy_mode` with a direct `set_proxy_mode` method on `ProxyManager`.

**Tech Stack:** Rust, ratatui, tokio (async spawn for API call), crossterm (key events)

---

### Task 1: Add mode selector state fields to UiState

**Files:**
- Modify: `src/app/state.rs`

- [ ] **Step 1: Add `show_mode_selector` and `mode_selector_idx` to UiState**

Open `src/app/state.rs`. In the `UiState` struct, add two fields after `show_settings`:

```rust
#[derive(Debug, Clone)]
pub struct UiState {
    pub active_view: ActiveView,
    pub selected_group_idx: usize,
    pub selected_node_idx: usize,
    pub selected_conn_idx: usize,
    pub log_paused: bool,
    pub log_level_filter: Option<String>,
    pub search_query: String,
    pub search_mode: bool,
    pub show_help: bool,
    pub show_settings: bool,
    pub show_mode_selector: bool,
    pub mode_selector_idx: usize,
    pub update_status: Option<String>,
}
```

In `impl Default for UiState`, add the defaults after `show_settings: false,`:

```rust
impl Default for UiState {
    fn default() -> Self {
        Self {
            active_view: ActiveView::Dashboard,
            selected_group_idx: 0,
            selected_node_idx: 0,
            selected_conn_idx: 0,
            log_paused: false,
            log_level_filter: None,
            search_query: String::new(),
            search_mode: false,
            show_help: false,
            show_settings: false,
            show_mode_selector: false,
            mode_selector_idx: 0,
            update_status: None,
        }
    }
}
```

- [ ] **Step 2: Build check**

```bash
cargo build 2>&1
```
Expected: compiles successfully (warnings about unused fields ok at this stage).

- [ ] **Step 3: Commit**

```bash
git add src/app/state.rs
git commit -m "feat: add mode selector state fields to UiState"
```

---

### Task 2: Add set_proxy_mode and remove cycle_proxy_mode

**Files:**
- Modify: `src/app/proxy_manager.rs`

- [ ] **Step 1: Replace `cycle_proxy_mode` with `set_proxy_mode`**

In `src/app/proxy_manager.rs`, remove the entire `cycle_proxy_mode` method body and replace with `set_proxy_mode`. Also add the `#[allow(dead_code)]` import since `detect_proxy_mode` is only used in tests for now (but keep it, it's still needed at runtime).

Old code (lines 50-66):
```rust
    pub async fn cycle_proxy_mode(
        client: &MihomoClient,
        current: ProxyMode,
    ) -> ApiResult<ProxyMode> {
        let next = match current {
            ProxyMode::Global => ProxyMode::Direct,
            ProxyMode::Rule => ProxyMode::Global,
            ProxyMode::Direct => ProxyMode::Rule,
        };
        let mode_str = match next {
            ProxyMode::Global => "global",
            ProxyMode::Rule => "rule",
            ProxyMode::Direct => "direct",
        };
        client.patch_configs(serde_json::json!({"mode": mode_str})).await?;
        Ok(next)
    }
```

Replace with:
```rust
    pub async fn set_proxy_mode(
        client: &MihomoClient,
        mode: &ProxyMode,
    ) -> ApiResult<()> {
        let mode_str = match mode {
            ProxyMode::Global => "global",
            ProxyMode::Rule => "rule",
            ProxyMode::Direct => "direct",
        };
        client.patch_configs(serde_json::json!({"mode": mode_str})).await
    }
```

- [ ] **Step 2: Build check**

```bash
cargo build 2>&1
```
Expected: COMPILE ERROR — `src/ui/app.rs` still references `ProxyManager::cycle_proxy_mode`. This is expected; we fix it in Task 4.

- [ ] **Step 3: Commit**

```bash
git add src/app/proxy_manager.rs
git commit -m "feat: replace cycle_proxy_mode with set_proxy_mode"
```

---

### Task 3: Rename CycleMode to OpenModeSelector in keybindings

**Files:**
- Modify: `src/ui/keybindings.rs`

- [ ] **Step 1: Rename the Action variant**

Change line 15 from `CycleMode` to `OpenModeSelector`:

```rust
    #[derive(Debug, Clone, PartialEq)]
    pub enum Action {
        Quit,
        SwitchView(usize),
        MoveDown,
        MoveUp,
        JumpTop,
        JumpBottom,
        Search,
        SearchNext,
        SearchPrev,
        CommandMode,
        OpenModeSelector,
        SwitchNode,
        TestNodeDelay,
        TestGroupDelay,
        PrevGroup,
        NextGroup,
        Back,
        CloseConnection,
        CloseAllConnections,
        TogglePause,
        CycleLogLevel,
        ToggleHelp,
        UpdateSubs,
        ShowSettings,
        ToggleProxy,
        Refresh,
    }
```

- [ ] **Step 2: Update the key mapping**

Change line 51 from `Action::CycleMode` to `Action::OpenModeSelector`:

```rust
        KeyEvent { code: KeyCode::Char('m'), .. } => Some(Action::OpenModeSelector),
```

- [ ] **Step 3: Update the test**

Change `test_dashboard` (line 105) from `Action::CycleMode` to `Action::OpenModeSelector`:

```rust
    #[test] fn test_dashboard() { assert_eq!(parse_key(k('m')), Some(Action::OpenModeSelector)); }
```

- [ ] **Step 4: Build check**

```bash
cargo build 2>&1
```
Expected: COMPILE ERROR — `src/ui/app.rs` still references `Action::CycleMode`. Expected.

- [ ] **Step 5: Commit**

```bash
git add src/ui/keybindings.rs
git commit -m "refactor: rename CycleMode to OpenModeSelector in keybindings"
```

---

### Task 4: Create mode_selector popup component

**Files:**
- Create: `src/ui/views/mode_selector.rs`

- [ ] **Step 1: Write the full mode_selector.rs**

Create `src/ui/views/mode_selector.rs` with the following content:

```rust
use ratatui::{
    layout::Rect,
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

const MODES: &[(&str, &str)] = &[
    ("Rule",    "按规则路由，匹配分流策略"),
    ("Global",  "全部流量经代理服务器转发"),
    ("Direct",  "全部流量直连，不经代理"),
];

pub fn render(f: &mut Frame, state: &AppState) {
    let area = centered_rect(50, 55, f.area());
    let block = Block::default()
        .title(" Proxy Mode  ↑↓/jk 选 ")
        .borders(Borders::ALL)
        .style(Style::default().bg(T.surface));
    let inner = block.inner(area);

    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let current_mode = format!("{:?}", state.proxy_mode);

    for (i, (name, desc)) in MODES.iter().enumerate() {
        let is_current = *name == current_mode;
        let is_highlighted = i == state.ui.mode_selector_idx;

        let prefix = if is_current { "✓ " } else { "  " };
        let label = if is_highlighted {
            format!("{}{}", prefix, name).fg(T.primary).bold()
        } else {
            format!("{}{}", prefix, name).fg(T.text)
        };
        lines.push(Line::from(label));

        let detail = if is_highlighted {
            format!("   {}", desc).fg(T.overlay)
        } else {
            format!("   {}", desc).fg(T.subtext)
        };
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(
        "Enter 确认 · Esc 取消".fg(T.subtext)
    ));

    let text = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(text, inner);
}

fn centered_rect(px: u16, py: u16, area: Rect) -> Rect {
    let w = area.width * px / 100;
    let h = area.height * py / 100;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}
```

- [ ] **Step 2: Build check**

```bash
cargo build 2>&1
```
Expected: COMPILE ERROR — module not registered in `views/mod.rs` and `app.rs` not updated. Expected.

- [ ] **Step 3: Commit**

```bash
git add src/ui/views/mode_selector.rs
git commit -m "feat: add mode selector popup component"
```

---

### Task 5: Wire up module registration, rendering, and event handling

**Files:**
- Modify: `src/ui/views/mod.rs`
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Register mode_selector module**

In `src/ui/views/mod.rs`, add `pub mod mode_selector;` after `pub mod logs;`:

```rust
pub mod connections;
pub mod dashboard;
pub mod help;
pub mod logs;
pub mod mode_selector;
pub mod proxies;
pub mod rules;
pub mod settings;
pub mod sidebar;
```

- [ ] **Step 2: Add mode_selector import in app.rs**

In `src/ui/app.rs`, change the views import line (19) from:
```rust
use crate::ui::views::{connections, dashboard, help, logs, proxies, rules, settings, sidebar};
```
to:
```rust
use crate::ui::views::{connections, dashboard, help, logs, mode_selector, proxies, rules, settings, sidebar};
```

- [ ] **Step 3: Add mode selector key interception in the main event loop**

In `src/ui/app.rs`, add a new `else if s.ui.show_mode_selector` block after the `search_mode` block (after line 139, `_ => {}`), and before `else if let Some(action) = parse_key(key)`:

The current code block (lines 121-142):
```rust
                    let mut s = state.lock().await;
                    // Search mode: capture keys as search input
                    if s.ui.search_mode {
                        match key.code {
                            KeyCode::Esc => {
                                s.ui.search_mode = false;
                                s.ui.search_query.clear();
                            }
                            KeyCode::Enter => {
                                s.ui.search_mode = false;
                            }
                            KeyCode::Backspace => {
                                s.ui.search_query.pop();
                            }
                            KeyCode::Char(c) => {
                                s.ui.search_query.push(c);
                            }
                            _ => {}
                        }
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await { break; }
                    }
```

Replace with:
```rust
                    let mut s = state.lock().await;
                    // Search mode: capture keys as search input
                    if s.ui.search_mode {
                        match key.code {
                            KeyCode::Esc => {
                                s.ui.search_mode = false;
                                s.ui.search_query.clear();
                            }
                            KeyCode::Enter => {
                                s.ui.search_mode = false;
                            }
                            KeyCode::Backspace => {
                                s.ui.search_query.pop();
                            }
                            KeyCode::Char(c) => {
                                s.ui.search_query.push(c);
                            }
                            _ => {}
                        }
                    } else if s.ui.show_mode_selector {
                        // Mode selector: capture navigation keys
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => {
                                s.ui.mode_selector_idx = (s.ui.mode_selector_idx + 1).min(2);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                s.ui.mode_selector_idx = s.ui.mode_selector_idx.saturating_sub(1);
                            }
                            KeyCode::Enter => {
                                let idx = s.ui.mode_selector_idx;
                                let target = match idx {
                                    0 => ProxyMode::Rule,
                                    1 => ProxyMode::Global,
                                    2 => ProxyMode::Direct,
                                    _ => ProxyMode::Rule,
                                };
                                s.ui.show_mode_selector = false;
                                if let Some(c) = client.clone() {
                                    let shared2 = shared.clone();
                                    tokio::spawn(async move {
                                        if ProxyManager::set_proxy_mode(&c, &target).await.is_ok() {
                                            refresh_state(&shared2).await;
                                        }
                                    });
                                }
                            }
                            KeyCode::Esc => {
                                s.ui.show_mode_selector = false;
                            }
                            _ => {}
                        }
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await { break; }
                    }
```

Note: `client` needs to be extracted before the lock, similar to how it's done in `handle_action`. Looking at the existing code, `client` is NOT extracted before the lock in the current event loop — the `client` variable used in `handle_action` is cloned inside `handle_action` from `s.client.clone()`. We need to clone it ourselves before entering the lock.

The actual fix: `client` is not in scope here. We need to clone it from `s.client` inside the lock, before the spawn. Looking at the above code, `client.clone()` is called inside the `Enter` branch which already holds `s`. So `s.client.clone()` should work. Let me adjust:

The `Enter` branch should read:
```rust
                            KeyCode::Enter => {
                                let idx = s.ui.mode_selector_idx;
                                let target = match idx {
                                    0 => ProxyMode::Rule,
                                    1 => ProxyMode::Global,
                                    2 => ProxyMode::Direct,
                                    _ => ProxyMode::Rule,
                                };
                                s.ui.show_mode_selector = false;
                                let c = s.client.clone();
                                let s2 = state.clone();
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        if ProxyManager::set_proxy_mode(client, &target).await.is_ok() {
                                            refresh_state(&s2).await;
                                        }
                                    }
                                });
                            }
```

- [ ] **Step 4: Replace Action::CycleMode handler with OpenModeSelector**

In `src/ui/app.rs`, in `handle_action`, replace the `Action::CycleMode` arm (lines 264-274):
```rust
        Action::CycleMode => {
            if let Some(c) = client {
                let mode = s.proxy_mode.clone();
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if ProxyManager::cycle_proxy_mode(&c, mode).await.is_ok() {
                        refresh_state(&shared2).await;
                    }
                });
            }
        }
```

With:
```rust
        Action::OpenModeSelector => {
            s.ui.show_mode_selector = !s.ui.show_mode_selector;
            if s.ui.show_mode_selector {
                s.ui.show_help = false;
                s.ui.show_settings = false;
                s.ui.mode_selector_idx = match s.proxy_mode {
                    ProxyMode::Rule => 0,
                    ProxyMode::Global => 1,
                    ProxyMode::Direct => 2,
                };
            }
        }
```

- [ ] **Step 5: Add mode selector close to Action::Back handler**

In `handle_action`, in the `Action::Back` arm (lines 386-392), add mode_selector close:

Current:
```rust
        Action::Back => {
            if s.ui.search_mode {
                s.ui.search_mode = false;
                s.ui.search_query.clear();
            } else if s.ui.show_help { s.ui.show_help = false; }
            else if s.ui.show_settings { s.ui.show_settings = false; }
        }
```

Replace with:
```rust
        Action::Back => {
            if s.ui.search_mode {
                s.ui.search_mode = false;
                s.ui.search_query.clear();
            } else if s.ui.show_mode_selector { s.ui.show_mode_selector = false; }
            else if s.ui.show_help { s.ui.show_help = false; }
            else if s.ui.show_settings { s.ui.show_settings = false; }
        }
```

- [ ] **Step 6: Add mode selector to ShowSettings mutual exclusion**

In `handle_action`, the `Action::ShowSettings` arm (lines 362-365):

Current:
```rust
        Action::ShowSettings => {
            s.ui.show_settings = !s.ui.show_settings;
            if s.ui.show_settings { s.ui.show_help = false; }
        }
```

Replace with:
```rust
        Action::ShowSettings => {
            s.ui.show_settings = !s.ui.show_settings;
            if s.ui.show_settings {
                s.ui.show_help = false;
                s.ui.show_mode_selector = false;
            }
        }
```

- [ ] **Step 7: Add mode selector to mouse click-outside-close and render**

In the mouse handler (lines 147-149), add `show_mode_selector` to the popup dismissal check:

Current:
```rust
                        if s.ui.show_help || s.ui.show_settings {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
```

Replace with:
```rust
                        if s.ui.show_help || s.ui.show_settings || s.ui.show_mode_selector {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
                            s.ui.show_mode_selector = false;
```

In `render_frame` (after line 209, after the settings popup block), add the mode selector render:

Current:
```rust
    // Settings popup overlay
    if state.ui.show_settings {
        settings::render(f, state);
    }
}
```

Replace with:
```rust
    // Settings popup overlay
    if state.ui.show_settings {
        settings::render(f, state);
    }

    // Mode selector popup overlay
    if state.ui.show_mode_selector {
        mode_selector::render(f, state);
    }
}
```

- [ ] **Step 8: Build check**

```bash
cargo build 2>&1
```
Expected: compiles successfully, no errors.

- [ ] **Step 9: Commit**

```bash
git add src/ui/views/mod.rs src/ui/app.rs
git commit -m "feat: wire up mode selector popup rendering and event handling"
```

---

### Task 6: Update help text

**Files:**
- Modify: `src/ui/views/help.rs`

- [ ] **Step 1: Update the help string**

In `src/ui/views/help.rs`, change line 20 from:
```
 m    Cycle proxy mode      ───────────
```
to:
```
 m    Open mode selector    ───────────
```

The full `HELP_TEXT` constant becomes:
```rust
const HELP_TEXT: &str = r#"
 Global                     Proxy View
 ──────                     ──────────
 1-5  Switch view           Enter  Switch node
 j/↓  Move down             t      Test node delay
 k/↑  Move up               T      Test group delay
 g    Jump to top           h/←    Prev group
 G    Jump to bottom        l/→    Next group
 /    Search nodes          Esc    Back / close
 n/N  Next/prev match
 r    Refresh data          Connections
 m    Open mode selector    ───────────
 p    Toggle proxy          d      Close connection
 q    Quit                  D      Close all
 ?    Toggle help

                            Logs
                            ────
                            Space  Pause/resume
                            s      Cycle log level

 Click sidebar to switch views  ·  Press ? or Esc to close
"#;
```

- [ ] **Step 2: Build check**

```bash
cargo build 2>&1
```
Expected: compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add src/ui/views/help.rs
git commit -m "docs: update help text for mode selector"
```

---

### Task 7: Run full test suite and verify

**Files:**
- None (verification only)

- [ ] **Step 1: Run all tests**

```bash
cargo test 2>&1
```
Expected: all tests pass. Key test to watch: `test_dashboard` in `keybindings` module should still pass with `OpenModeSelector`.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1
```
Expected: no warnings.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "chore: final verification — all tests pass, no clippy warnings"
```

(If no changes to commit, skip this commit — test only task.)

---

### Task 8: Write unit test for set_proxy_mode index mapping

**Files:**
- Modify: `src/app/proxy_manager.rs`

- [ ] **Step 1: Add test for mode index mapping**

At the bottom of the `#[cfg(test)] mod tests` block in `src/app/proxy_manager.rs`, after the last test function, add:

```rust
    #[test]
    fn test_mode_to_index_mapping() {
        // Verify the mapping used by mode selector Enter handler
        assert_eq!(0, match ProxyMode::Rule { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 });
        assert_eq!(1, match ProxyMode::Global { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 });
        assert_eq!(2, match ProxyMode::Direct { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 });
    }
```

- [ ] **Step 2: Run tests**

```bash
cargo test test_mode_to_index_mapping 2>&1
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/app/proxy_manager.rs
git commit -m "test: add mode-to-index mapping test"
```

---

## Summary

| Task | Files | Purpose |
|------|-------|---------|
| 1 | `state.rs` | Add `show_mode_selector`, `mode_selector_idx` to `UiState` |
| 2 | `proxy_manager.rs` | Replace `cycle_proxy_mode` → `set_proxy_mode` |
| 3 | `keybindings.rs` | Rename `CycleMode` → `OpenModeSelector` |
| 4 | `mode_selector.rs` (new) | Popup render: centered block with 3 modes + descriptions |
| 5 | `app.rs`, `views/mod.rs` | Wire up: key interception, render, modal exclusion |
| 6 | `help.rs` | Update help text string |
| 7 | — | Full test suite + clippy verification |
| 8 | `proxy_manager.rs` | Unit test for mode-to-index mapping |
