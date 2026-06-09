# Loading Indicator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Braille spinner loading feedback to status bar and dashboard cards for all async operations

**Architecture:** A unified `LoadingKind` enum in `UiState` tracks which operation is in progress. The status bar renders a Braille dot spinner + description. Dashboard Mode/TUN/SysProxy cards show inline spinner when relevant. All `tokio::spawn` calls set loading before spawn and clear it inside the spawn after completion.

**Tech Stack:** Rust, ratatui, tokio

**Files:** 4 files modified, 0 new files

---

### Task 1: Add LoadingKind enum and modify UiState

**Files:**
- Modify: `src/app/state.rs`

- [ ] **Step 1: Add LoadingKind enum above ProxyMode**

At line 8 (after imports, before `ProxyMode`), insert:

```rust
/// Identifies which async operation is currently in progress.
#[derive(Debug, Clone, PartialEq)]
pub enum LoadingKind {
    Init,
    Refresh,
    SwitchMode,
    SwitchNode,
    ToggleProxy,
    TestNodeDelay,
    TestGroupDelay,
    UpdateSubs,
}

impl LoadingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Init => "Loading...",
            Self::Refresh => "Refreshing...",
            Self::SwitchMode => "Switching mode...",
            Self::SwitchNode => "Switching node...",
            Self::ToggleProxy => "Toggling proxy...",
            Self::TestNodeDelay => "Testing delay...",
            Self::TestGroupDelay => "Testing group delay...",
            Self::UpdateSubs => "Updating subscriptions...",
        }
    }
}
```

- [ ] **Step 2: Replace `update_status` with `loading` + `spinner_frame` in UiState**

At lines 25-39, change the `UiState` struct:

Remove line 38: `pub update_status: Option<String>,`

Add after `pub show_mode_selector: bool,` (line 36):
```rust
    pub loading: Option<LoadingKind>,
    pub spinner_frame: u8,
```

- [ ] **Step 3: Update UiState::default()**

At lines 42-58, in `impl Default for UiState`:

Remove line 56: `update_status: None,`

Add after `show_mode_selector: false,` (line 54):
```rust
            loading: None,
            spinner_frame: 0,
```

- [ ] **Step 4: Set loading: Some(LoadingKind::Init) in AppState::new()**

At lines 93-117, add after `ui: UiState::default(),` (line 96):
```rust
            ui: {
                let mut ui = UiState::default();
                ui.loading = Some(LoadingKind::Init);
                ui
            },
```

Wait — simpler approach: add the init loading in `AppState::new()` directly:

Replace line 96 (`ui: UiState::default(),`) with the approach of setting loading field after construction. Change the `ui` field assignment in `AppState::new()` (line 96) from:

```rust
            ui: UiState::default(),
```

to constructing inline so we can set loading:

Actually the cleanest way: set it right after `Self { ... }` construction. Change to:

```rust
    pub fn new() -> Self {
        let ui = UiState {
            loading: Some(LoadingKind::Init),
            ..UiState::default()
        };
        Self {
            config: MioctlConfig::load(),
            client: None,
            ui,
            // ... rest unchanged
```

- [ ] **Step 5: Update existing unit test for new fields**

At lines 140-147, in `test_ui_state_defaults`, add two assertions after line 146:

```rust
        assert!(ui.loading.is_none());
        assert_eq!(ui.spinner_frame, 0);
```

- [ ] **Step 6: Run tests to verify no breakage**

Run: `cargo test -- state`
Expected: compile error on `update_status` usages in other files (we'll fix those in later tasks)

- [ ] **Step 7: Commit**

```bash
git add src/app/state.rs
git commit -m "feat: add LoadingKind enum, replace update_status with loading field"
```

---

### Task 2: Update status bar to render spinner

**Files:**
- Modify: `src/ui/widgets/status_bar.rs`

- [ ] **Step 1: Add spinner frame array constant and replace update_status rendering**

At the top of the file (line 10), add after the `use` block:

```rust
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
```

- [ ] **Step 2: Replace `update_status` rendering (lines 21-23)**

Replace lines 21-23:
```rust
    if let Some(ref status) = state.ui.update_status {
        spans.push(Span::styled(format!("| {} ", status), Style::default().fg(T.yellow)));
    }
```

With:
```rust
    if let Some(ref kind) = state.ui.loading {
        let frame = SPINNER[state.ui.spinner_frame as usize % SPINNER.len()];
        spans.push(Span::styled(
            format!("{} {} ", frame, kind.as_str()),
            Style::default().fg(T.yellow),
        ));
    }
```

- [ ] **Step 3: Build check**

Run: `cargo build 2>&1`
Expected: error only about `update_status` in `app.rs` (fixed in later tasks). If status_bar has other errors, fix them.

- [ ] **Step 4: Commit**

```bash
git add src/ui/widgets/status_bar.rs
git commit -m "feat: render Braille spinner in status bar when loading"
```

---

### Task 3: Update dashboard cards for contextual loading feedback

**Files:**
- Modify: `src/ui/views/dashboard.rs`

- [ ] **Step 1: Add helper to render card value conditionally**

The `card` function (lines 154-161) takes a static `value: &str`. We need Mode/TUN/SysProxy cards to show spinner when relevant.

Add a helper function after `card` (after line 161):

```rust
/// Returns the card value to display, showing spinner when a relevant
/// loading operation is active.
fn card_value(normal: &str, loading_kind: LoadingKind, state: &AppState) -> String {
    if state.ui.loading.as_ref() == Some(&loading_kind) {
        let frame = SPINNER[state.ui.spinner_frame as usize % SPINNER.len()];
        format!("{} ...", frame)
    } else {
        normal.to_string()
    }
}
```

This needs `LoadingKind` in scope — add the import at the top.

- [ ] **Step 2: Add imports**

Add after line 10:
```rust
use crate::app::state::LoadingKind;
```

And add the spinner constant:
```rust
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
```

- [ ] **Step 3: Update Mode card (line 35)**

Replace:
```rust
    card(f, cards1[0], "Mode", &format!("{:?}", state.proxy_mode), T.primary);
```

With:
```rust
    let mode_val = card_value(&format!("{:?}", state.proxy_mode), LoadingKind::SwitchMode, state);
    card(f, cards1[0], "Mode", &mode_val, T.primary);
```

- [ ] **Step 4: Update TUN card (lines 51-53)**

Replace:
```rust
    let tun_enabled = state.tun.as_ref().map(|t| t.enable).unwrap_or(false);
    let tun_color = if tun_enabled { T.green } else { T.surface };
    card(f, cards2[0], "TUN", if tun_enabled { "ON" } else { "OFF" }, tun_color);
```

With:
```rust
    let tun_enabled = state.tun.as_ref().map(|t| t.enable).unwrap_or(false);
    let tun_color = if tun_enabled { T.green } else { T.surface };
    let tun_val = card_value(if tun_enabled { "ON" } else { "OFF" }, LoadingKind::ToggleProxy, state);
    card(f, cards2[0], "TUN", &tun_val, tun_color);
```

- [ ] **Step 5: Update SysProxy card (lines 55-56)**

Replace:
```rust
    let sp_color = if state.system_proxy_enabled { T.green } else { T.surface };
    card(f, cards2[1], "SysProxy", if state.system_proxy_enabled { "ON" } else { "OFF" }, sp_color);
```

With:
```rust
    let sp_color = if state.system_proxy_enabled { T.green } else { T.surface };
    let sp_val = card_value(if state.system_proxy_enabled { "ON" } else { "OFF" }, LoadingKind::ToggleProxy, state);
    card(f, cards2[1], "SysProxy", &sp_val, sp_color);
```

- [ ] **Step 6: Build check**

Run: `cargo build 2>&1`
Expected: dashboard.rs compiles cleanly

- [ ] **Step 7: Commit**

```bash
git add src/ui/views/dashboard.rs
git commit -m "feat: show inline spinner on dashboard cards during loading"
```

---

### Task 4: Add spinner tick to event loop

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add LoadingKind import**

At line 13, change:
```rust
use crate::app::state::{ActiveView::*, AppState, ProxyMode, SharedState};
```

To:
```rust
use crate::app::state::{ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState};
```

- [ ] **Step 2: Add spinner tick between event handling and rendering**

After line 191 (the closing brace of `Event::Mouse` handling block) and before line 193 (`let s = state.lock().await;`), add the spinner tick:

```rust
        // Advance spinner animation frame if loading
        {
            let mut s = state.lock().await;
            if s.ui.loading.is_some() {
                s.ui.spinner_frame = (s.ui.spinner_frame + 1) % 10;
            }
        }
```

- [ ] **Step 3: Build check**

Run: `cargo build 2>&1`
Expected: app.rs compiles (errors will be about `update_status` in UpdateSubs handler — fixed in later tasks)

- [ ] **Step 4: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: add spinner frame tick in event loop"
```

---

### Task 5: Wire loading into mode switch spawn

**Files:**
- Modify: `src/ui/app.rs`

In the mode selector Enter handler (lines 149-167), add `loading` before spawn and clear it inside.

- [ ] **Step 1: Add loading set before spawn (after line 157 `s.ui.show_mode_selector = false;`)**

Insert:
```rust
                                s.ui.loading = Some(LoadingKind::SwitchMode);
```

- [ ] **Step 2: Add loading clear inside spawn**

After line 163 (`if ProxyManager::set_proxy_mode(client, &target).await.is_ok() {`), after the `refresh_state(&s2).await;` call, add loading clear:

The current code (lines 159-166):
```rust
                                let c = s.client.clone();
                                let s2 = state.clone();
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        if ProxyManager::set_proxy_mode(client, &target).await.is_ok() {
                                            refresh_state(&s2).await;
                                        }
                                    }
                                });
```

Replace with:
```rust
                                let c = s.client.clone();
                                let s2 = state.clone();
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        let _ = ProxyManager::set_proxy_mode(client, &target).await;
                                        refresh_state(&s2).await;
                                    }
                                    let mut s = s2.lock().await;
                                    s.ui.loading = None;
                                });
```

Note: always call `refresh_state` and clear `loading`, regardless of success/failure, so state is consistent.

- [ ] **Step 3: Build check**

Run: `cargo build 2>&1`
Expected: compiles

- [ ] **Step 4: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into mode switch"
```

---

### Task 6: Wire loading into refresh spawn

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add loading + clear loading in Refresh handler (lines 257-265)**

Replace:
```rust
        Action::Refresh => {
            let c = s.client.clone();
            let shared2 = shared.clone();
            tokio::spawn(async move {
                if c.is_some() {
                    refresh_state(&shared2).await;
                }
            });
        }
```

With:
```rust
        Action::Refresh => {
            s.ui.loading = Some(LoadingKind::Refresh);
            let c = s.client.clone();
            let shared2 = shared.clone();
            tokio::spawn(async move {
                if c.is_some() {
                    refresh_state(&shared2).await;
                }
                let mut s = shared2.lock().await;
                s.ui.loading = None;
            });
        }
```

- [ ] **Step 2: Build check**

Run: `cargo build 2>&1`

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into refresh"
```

---

### Task 7: Wire loading into TUN/system proxy toggle spawn

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add loading + clear loading in ToggleProxy handler (lines 315-336)**

Replace:
```rust
        Action::ToggleProxy => {
            if let Some(c) = client {
                let tun_enabled = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                let any_active = tun_enabled || s.system_proxy_enabled;
                let shared2 = shared.clone();
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
                });
            }
        }
```

With:
```rust
        Action::ToggleProxy => {
            if let Some(c) = client {
                s.ui.loading = Some(LoadingKind::ToggleProxy);
                let tun_enabled = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                let any_active = tun_enabled || s.system_proxy_enabled;
                let shared2 = shared.clone();
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
            }
        }
```

- [ ] **Step 2: Build check**

Run: `cargo build 2>&1`

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into TUN/proxy toggle"
```

---

### Task 8: Wire loading into node switch spawn

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add loading + clear loading in SwitchNode handler (lines 337-350)**

Replace:
```rust
        Action::SwitchNode => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let group_name = s.groups.get(i).map(|g| g.name.clone());
            let node_name = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if ProxyManager::switch_node(&c, &gn, &nn).await.is_ok() {
                        refresh_state(&shared2).await;
                    }
                });
            }
        }
```

With:
```rust
        Action::SwitchNode => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let group_name = s.groups.get(i).map(|g| g.name.clone());
            let node_name = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
                s.ui.loading = Some(LoadingKind::SwitchNode);
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    let _ = ProxyManager::switch_node(&c, &gn, &nn).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
            }
        }
```

Note: changed `if ProxyManager::...is_ok()` to `let _ = ...` so refresh always runs and loading always cleared.

- [ ] **Step 2: Build check**

Run: `cargo build 2>&1`

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into node switch"
```

---

### Task 9: Wire loading into delay test spawns

**Files:**
- Modify: `src/ui/app.rs`

Handles two actions: `TestNodeDelay` (lines 351-364) and `TestGroupDelay` (lines 366-378).

- [ ] **Step 1: Add loading + clear loading in TestNodeDelay handler**

Replace:
```rust
        Action::TestNodeDelay => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let node = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(n)) = (client, node) {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if ProxyManager::test_node_delay(&c, &n, &url, timeout).await.is_ok() {
                        refresh_state(&shared2).await;
                    }
                });
            }
        }
```

With:
```rust
        Action::TestNodeDelay => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let node = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(n)) = (client, node) {
                s.ui.loading = Some(LoadingKind::TestNodeDelay);
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    let _ = ProxyManager::test_node_delay(&c, &n, &url, timeout).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
            }
        }
```

- [ ] **Step 2: Add loading + clear loading in TestGroupDelay handler**

Replace:
```rust
        Action::TestGroupDelay => {
            let i = s.ui.selected_group_idx;
            let group = s.groups.get(i).map(|g| g.name.clone());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(g)) = (client, group) {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if ProxyManager::test_group_delay(&c, &g, &url, timeout).await.is_ok() {
                        refresh_state(&shared2).await;
                    }
                });
            }
        }
```

With:
```rust
        Action::TestGroupDelay => {
            let i = s.ui.selected_group_idx;
            let group = s.groups.get(i).map(|g| g.name.clone());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(g)) = (client, group) {
                s.ui.loading = Some(LoadingKind::TestGroupDelay);
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    let _ = ProxyManager::test_group_delay(&c, &g, &url, timeout).await;
                    refresh_state(&shared2).await;
                    let mut s = shared2.lock().await;
                    s.ui.loading = None;
                });
            }
        }
```

- [ ] **Step 3: Build check**

Run: `cargo build 2>&1`

- [ ] **Step 4: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into delay tests"
```

---

### Task 10: Wire loading into subscription update spawn

**Files:**
- Modify: `src/ui/app.rs`

This is the only handler that used `update_status`. Replace with `loading`.

- [ ] **Step 1: Replace UpdateSubs handler (lines 409-428)**

Replace:
```rust
        Action::UpdateSubs => {
            let mut cfg = s.config.clone();
            let c = s.client.clone();
            tokio::spawn(async move {
                if let Some(ref client) = c {
                    match SubscriptionManager::update_all(&mut cfg, client).await {
                        Ok(r) => {
                            let mut state = shared.lock().await;
                            state.config = cfg;
                            state.ui.update_status = Some(format!("Subs updated: {}", r));
                        }
                        Err(e) => {
                            let mut state = shared.lock().await;
                            state.ui.update_status = Some(format!("Subs failed: {}", e));
                        }
                    }
                }
            });
            s.ui.update_status = Some("Updating subscriptions...".into());
        }
```

With:
```rust
        Action::UpdateSubs => {
            s.ui.loading = Some(LoadingKind::UpdateSubs);
            let mut cfg = s.config.clone();
            let c = s.client.clone();
            let shared2 = shared.clone();
            tokio::spawn(async move {
                if let Some(ref client) = c {
                    let _ = SubscriptionManager::update_all(&mut cfg, client).await;
                }
                let mut state = shared2.lock().await;
                state.config = cfg;
                state.ui.loading = None;
            });
        }
```

Note: `update_status` is no longer referenced anywhere. The spec removes success/failure messages — only spinner shows during the operation.

- [ ] **Step 2: Build check**

Run: `cargo build 2>&1`
Expected: compiles with zero warnings, no `update_status` references remain

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: wire loading indicator into subscription update, remove update_status"
```

---

### Task 11: Wire loading into init spawn

**Files:**
- Modify: `src/ui/app.rs`

The init handle (lines 28-103) runs once at startup. `AppState::new()` already sets `loading: Some(LoadingKind::Init)` (done in Task 1). We need to clear it after the init data loads.

- [ ] **Step 1: Clear loading at end of init state update block**

In the init handle (around lines 60-85), after the state update block (`s.update_time();` at line 85), add loading clear:

At line 85, after `s.update_time();` and before the closing `}` of the lock block (line 86), insert:
```rust
                s.ui.loading = None;
```

- [ ] **Step 2: Build check**

Run: `cargo build 2>&1`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: clear Init loading after initial data load"
```

---

### Task 12: Run full test suite + clippy

**Files:** All

- [ ] **Step 1: Run all tests**

Run: `cargo test`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: zero warnings

- [ ] **Step 3: If any test fails, fix and re-run**

Check for any test that references `update_status` — there should be none since no test directly tested it.

- [ ] **Step 4: Commit test fixes if any**

```bash
git add -A
git commit -m "chore: fix tests for loading indicator changes"
```

---

### Task 13: Final verification

**Files:** All

- [ ] **Step 1: Full test suite**

Run: `cargo test`

- [ ] **Step 2: Clippy**

Run: `cargo clippy -- -D warnings`

- [ ] **Step 3: Verify no references to `update_status` remain**

Run: `grep -rn "update_status" src/`
Expected: no output

- [ ] **Step 4: Confirm init loading is set in AppState::new()**

Run: `grep -A3 "LoadingKind::Init" src/app/state.rs`
Expected: shows `loading: Some(LoadingKind::Init)` in AppState::new()

---

### Summary

| Task | Files | Description |
|------|-------|-------------|
| 1 | `state.rs` | Add LoadingKind enum, replace update_status with loading/spinner_frame |
| 2 | `status_bar.rs` | Render Braille spinner + description |
| 3 | `dashboard.rs` | Show inline spinner on Mode/TUN/SysProxy cards |
| 4 | `app.rs` | Add spinner frame tick in event loop |
| 5 | `app.rs` | Wire loading into mode switch spawn |
| 6 | `app.rs` | Wire loading into refresh spawn |
| 7 | `app.rs` | Wire loading into TUN/proxy toggle spawn |
| 8 | `app.rs` | Wire loading into node switch spawn |
| 9 | `app.rs` | Wire loading into delay test spawns |
| 10 | `app.rs` | Wire loading into subscription update spawn + remove update_status |
| 11 | `app.rs` | Clear Init loading after initial data load |
| 12 | all | Run full test suite + clippy, fix any issues |
| 13 | all | Final verification |
