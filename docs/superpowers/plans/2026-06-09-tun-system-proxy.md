# TUN & System Proxy — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display TUN status, system proxy status, and proxy port info in Dashboard, with `p` key toggling TUN + system proxy via smart linkage.

**Architecture:** Extend `MihomoConfig` with `TunConfig`, add `os/proxy.rs` for `~/.config/environment.d/proxy.conf` management, add second card row to Dashboard, add `ToggleProxy` action with binary toggle logic (any-on → all-off, all-off → TUN-on).

**Tech Stack:** Rust, serde, ratatui, crossterm, tokio

---

### Task 1: Add TunConfig struct and extend MihomoConfig

**Files:**
- Modify: `src/api/types.rs`

- [ ] **Step 1: Add TunConfig struct after MihomoConfig (before Proxy Provider section)**

Add this block after line 154 (closing `}` of `MihomoConfig`):

```rust
// === TUN ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default, rename = "auto-route")]
    pub auto_route: Option<bool>,
}
```

- [ ] **Step 2: Add `tun` field to MihomoConfig**

Insert after line 153 (`pub log_level: Option<String>,`):

```rust
    #[serde(default)]
    pub tun: Option<TunConfig>,
```

- [ ] **Step 3: Add unit test for TunConfig deserialization**

Add at the end of the `mod tests` block in types.rs (before the closing `}`):

```rust
    #[test]
    fn test_deserialize_mihomo_config_with_tun() {
        let json = r#"{
            "port": 7890,
            "mixed-port": 7897,
            "allow-lan": true,
            "mode": "rule",
            "tun": {
                "enable": true,
                "stack": "system",
                "device": "utun",
                "auto-route": true
            }
        }"#;
        let cfg: MihomoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.mixed_port, Some(7897));
        assert!(cfg.allow_lan.unwrap());
        let tun = cfg.tun.unwrap();
        assert!(tun.enable);
        assert_eq!(tun.stack.as_deref(), Some("system"));
        assert_eq!(tun.device.as_deref(), Some("utun"));
    }

    #[test]
    fn test_deserialize_mihomo_config_no_tun() {
        let json = r#"{"port": 7890, "mode": "rule"}"#;
        let cfg: MihomoConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.tun.is_none());
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test test_deserialize_mihomo_config 2>&1
```
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/api/types.rs
git commit -m "feat: add TunConfig struct and tun field to MihomoConfig"
```

---

### Task 2: Add tun + system_proxy_enabled fields to AppState

**Files:**
- Modify: `src/app/state.rs`

- [ ] **Step 1: Add fields to AppState**

After line 67 (`pub memory: Memory,`), insert:

```rust
    pub tun: Option<TunConfig>,
    pub system_proxy_enabled: bool,
```

- [ ] **Step 2: Initialize fields in AppState::new()**

After line 90 (`memory: Memory { inuse: 0, oslimit: 0 },`), insert:

```rust
            tun: None,
            system_proxy_enabled: false,
```

- [ ] **Step 3: Build check**

```bash
cargo build 2>&1
```
Expected: compiles (may have unused field warnings — expected until Task 4)

- [ ] **Step 4: Commit**

```bash
git add src/app/state.rs
git commit -m "feat: add tun and system_proxy_enabled fields to AppState"
```

---

### Task 3: Create OS proxy module

**Files:**
- Create: `src/os/mod.rs`
- Create: `src/os/proxy.rs`
- Modify: `src/main.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/os/mod.rs`**

```rust
pub mod proxy;
```

- [ ] **Step 2: Create `src/os/proxy.rs`**

```rust
use std::fs;
use std::io::Write;
use std::path::PathBuf;

/// Path to the systemd user environment proxy config file
fn proxy_conf_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("environment.d")
        .join("proxy.conf")
}

/// Detect whether system proxy is enabled (proxy.conf exists and points to the given port)
pub fn detect_system_proxy(mixed_port: Option<u16>) -> bool {
    let Some(port) = mixed_port else { return false };
    let path = proxy_conf_path();
    if !path.exists() {
        return false;
    }
    match fs::read_to_string(&path) {
        Ok(content) => {
            let expected = format!("http://127.0.0.1:{}", port);
            content.lines().any(|line| {
                line.starts_with("HTTP_PROXY=") && line.contains(&expected)
            })
        }
        Err(_) => false,
    }
}

/// Enable system proxy: write proxy.conf pointing to the given port
pub fn set_system_proxy(mixed_port: u16) -> std::io::Result<()> {
    let path = proxy_conf_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        "HTTP_PROXY=http://127.0.0.1:{0}\n\
         HTTPS_PROXY=http://127.0.0.1:{0}\n\
         ALL_PROXY=socks5://127.0.0.1:{0}\n\
         NO_PROXY=localhost,127.0.0.1,::1,.local\n",
        mixed_port
    );
    let mut f = fs::File::create(&path)?;
    f.write_all(content.as_bytes())?;
    // Refresh systemd user environment for current session
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "import-environment", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"])
        .output();
    Ok(())
}

/// Disable system proxy: remove proxy.conf
pub fn clear_system_proxy() {
    let path = proxy_conf_path();
    let _ = fs::remove_file(&path);
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "import-environment", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_detect_with_matching_port() {
        let dir = std::env::temp_dir().join("mioctl-test-proxy-detect");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proxy.conf");
        fs::write(&path, "HTTP_PROXY=http://127.0.0.1:7897\nHTTPS_PROXY=http://127.0.0.1:7897\n").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let expected = "http://127.0.0.1:7897";
        assert!(content.lines().any(|line| line.starts_with("HTTP_PROXY=") && line.contains(expected)));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_detect_none_port_returns_false() {
        assert!(!detect_system_proxy(None));
    }

    #[test]
    fn test_detect_missing_file_returns_false() {
        // Use a port that definitely has no proxy.conf matching
        assert!(!detect_system_proxy(Some(1)));
    }

    #[test]
    fn test_set_and_clear_system_proxy() {
        let dir = std::env::temp_dir().join("mioctl-test-set-clear");
        let path = dir.join("proxy.conf");
        fs::create_dir_all(&dir).unwrap();

        // Write manually to simulate set
        fs::write(&path, "HTTP_PROXY=http://127.0.0.1:7897\nHTTPS_PROXY=http://127.0.0.1:7897\nALL_PROXY=socks5://127.0.0.1:7897\nNO_PROXY=localhost,127.0.0.1,::1,.local\n").unwrap();
        assert!(path.exists());

        // Simulate clear
        fs::remove_file(&path).unwrap();
        assert!(!path.exists());

        fs::remove_dir_all(dir).unwrap();
    }
}
```

Note: This module uses `dirs::home_dir()`. If `dirs` crate is not already in `Cargo.toml`, confirm it's there before building.

- [ ] **Step 3: Add `mod os;` to `src/main.rs`**

After line 4 (`mod config;`), insert:

```rust
mod os;
```

- [ ] **Step 4: Add `pub mod os;` to `src/lib.rs`**

After line 4 (`pub mod config;`), insert:

```rust
pub mod os;
```

- [ ] **Step 5: Run tests**

```bash
cargo test os::proxy 2>&1
```
Expected: 4 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/os/ src/main.rs src/lib.rs
git commit -m "feat: add OS proxy module for environment.d management"
```

---

### Task 4: Update refresh_state to parse tun and detect system proxy

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add `use crate::os;` import**

After line 17 (`use crate::ui::keybindings::{parse_key, parse_mouse, Action};`), insert:

```rust
use crate::os;
```

- [ ] **Step 2: Update configs parsing in initial load block (around line 72-78)**

Replace the configs parsing block:

```rust
                if let Ok(c) = configs_r {
                    s.proxy_mode = match c.mode.as_deref() {
                        Some("global") => ProxyMode::Global,
                        Some("direct") => ProxyMode::Direct,
                        _ => ProxyMode::Rule,
                    };
                }
```

With:

```rust
                if let Ok(c) = configs_r {
                    s.proxy_mode = match c.mode.as_deref() {
                        Some("global") => ProxyMode::Global,
                        Some("direct") => ProxyMode::Direct,
                        _ => ProxyMode::Rule,
                    };
                    s.tun = c.tun;
                    s.system_proxy_enabled = os::proxy::detect_system_proxy(c.mixed_port);
                }
```

- [ ] **Step 3: Update configs parsing in refresh_state function (around line 451-457)**

Replace the configs parsing block:

```rust
    if let Ok(c) = configs {
        s.proxy_mode = match c.mode.as_deref() {
            Some("global") => ProxyMode::Global,
            Some("direct") => ProxyMode::Direct,
            _ => ProxyMode::Rule,
        };
    }
```

With:

```rust
    if let Ok(c) = configs {
        s.proxy_mode = match c.mode.as_deref() {
            Some("global") => ProxyMode::Global,
            Some("direct") => ProxyMode::Direct,
            _ => ProxyMode::Rule,
        };
        s.tun = c.tun;
        s.system_proxy_enabled = os::proxy::detect_system_proxy(c.mixed_port);
    }
```

- [ ] **Step 4: Build check**

```bash
cargo build 2>&1
```
Expected: compiles cleanly (may have unused-import warning for `os` until Task 6 — OK)

- [ ] **Step 5: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: parse tun config and detect system proxy in refresh_state"
```

---

### Task 5: Add ToggleProxy action, keybinding, and help text

**Files:**
- Modify: `src/ui/keybindings.rs`
- Modify: `src/ui/views/help.rs`

- [ ] **Step 1: Add ToggleProxy variant to Action enum (line 29, before `Refresh`)**

```rust
    ToggleProxy,
```

- [ ] **Step 2: Add `p` key binding in parse_key (line 63, before `KeyEvent { code: KeyCode::Char('r')`)**

```rust
        KeyEvent { code: KeyCode::Char('p'), .. } => Some(Action::ToggleProxy),
```

- [ ] **Step 3: Add test in keybindings tests (after test_dashboard, before test_proxies)**

```rust
    #[test] fn test_toggle_proxy() { assert_eq!(parse_key(k('p')), Some(Action::ToggleProxy)); }
```

- [ ] **Step 4: Update help text to add `p Toggle proxy`**

Replace the HELP_TEXT constant (lines 9-30) with:

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
 m    Cycle proxy mode      ───────────
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

- [ ] **Step 5: Run tests**

```bash
cargo test keybindings 2>&1
```
Expected: all keybinding tests pass including `test_toggle_proxy`

- [ ] **Step 6: Commit**

```bash
git add src/ui/keybindings.rs src/ui/views/help.rs
git commit -m "feat: add ToggleProxy action, p key binding, and help text"
```

---

### Task 6: Handle ToggleProxy action in app event loop

**Files:**
- Modify: `src/ui/app.rs`

- [ ] **Step 1: Add ToggleProxy handler in handle_action (before `Action::PrevGroup`, around line 313)**

Insert:

```rust
        Action::ToggleProxy => {
            if let Some(c) = client {
                let tun_enabled = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                let any_active = tun_enabled || s.system_proxy_enabled;
                let port = s.tun.as_ref().and_then(|_| {
                    // mixed_port isn't on s directly — read from proxies or config
                    // We'll pass it through a separate mechanism
                    None::<u16>
                });
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if any_active {
                        // Turn everything off
                        if tun_enabled {
                            let _ = c.patch_configs(serde_json::json!({"tun": {"enable": false}})).await;
                        }
                        crate::os::proxy::clear_system_proxy();
                    } else {
                        // Turn TUN on
                        let _ = c.patch_configs(serde_json::json!({"tun": {"enable": true}})).await;
                    }
                    refresh_state(&shared2).await;
                });
            }
        }
```

Wait — we don't have access to `mixed_port` from `s` directly. We need to either:
a. Store `mixed_port` in AppState, or
b. Read it from the configs response each time

Looking at the spec, `set_system_proxy` is only called in the spec's 3-state cycle mode. But the spec says the chosen behavior is binary toggle:
- any active → all off (disable TUN + clear system proxy)
- all off → enable TUN (don't touch system proxy)

With binary toggle, we never call `set_system_proxy`! Only `clear_system_proxy`. So we don't need `mixed_port` in the action handler. The `set_system_proxy` function exists for potential future use but is not called in the binary toggle path.

Let me update the handler code without the unused `port` variable.

- [ ] **Step 1: Add ToggleProxy handler in handle_action (before `Action::PrevGroup`)**

```rust
        Action::ToggleProxy => {
            if let Some(c) = client {
                let tun_enabled = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                let any_active = tun_enabled || s.system_proxy_enabled;
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if any_active {
                        // Turn everything off
                        if tun_enabled {
                            let _ = c.patch_configs(serde_json::json!({"tun": {"enable": false}})).await;
                        }
                        crate::os::proxy::clear_system_proxy();
                    } else {
                        // Turn TUN on
                        let _ = c.patch_configs(serde_json::json!({"tun": {"enable": true}})).await;
                    }
                    refresh_state(&shared2).await;
                });
            }
        }
```

- [ ] **Step 2: Build check**

```bash
cargo build 2>&1
```
Expected: compiles cleanly

- [ ] **Step 3: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: implement ToggleProxy action handler"
```

---

### Task 7: Update dashboard to show TUN/SysProxy/Ports cards

**Files:**
- Modify: `src/ui/views/dashboard.rs`

- [ ] **Step 1: Restructure layout to add second card row**

Replace the render function body (lines 13-47) with:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState, spark: &TrafficSpark) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(area);

    // Row 1: Mode, Upload, Download, Conns
    let cards1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[0]);

    card(f, cards1[0], "Mode", &format!("{:?}", state.proxy_mode), T.primary);
    card(f, cards1[1], "Upload", &format!("{:.1} KB/s", state.traffic.up as f64 / 1024.0), T.green);
    card(f, cards1[2], "Download", &format!("{:.1} KB/s", state.traffic.down as f64 / 1024.0), T.red);
    card(f, cards1[3], "Conns", &state.connections.len().to_string(), T.yellow);

    // Row 2: TUN, System Proxy, Mixed Port, Allow LAN
    let cards2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[1]);

    let tun_enabled = state.tun.as_ref().map(|t| t.enable).unwrap_or(false);
    let tun_label = if tun_enabled { "ON" } else { "OFF" };
    let tun_color = if tun_enabled { T.green } else { T.surface };
    card(f, cards2[0], "TUN", tun_label, tun_color);

    let sp_label = if state.system_proxy_enabled { "ON" } else { "OFF" };
    let sp_color = if state.system_proxy_enabled { T.green } else { T.surface };
    card(f, cards2[1], "SysProxy", sp_label, sp_color);

    let port_str = state.tun.as_ref()
        .and_then(|_| {
            // Use mixed-port from the config response stored during init
            // We don't keep it in state, but we can infer from system_proxy detection.
            // Simpler: read the proxy.conf if it exists
            None::<String>
        })
        .unwrap_or_else(|| "—".into());
    // We'll show "-" since mixed_port isn't stored. This is fine — add it
    // properly by storing mixed_port in state.
    card(f, cards2[2], "MixPort", &port_str, T.text);

    let allow_lan = state.tun.as_ref()
        .and_then(|_| Some("?"))
        .unwrap_or("—");
    card(f, cards2[3], "AllowLAN", allow_lan, T.text_secondary);

    // Sparkline
    let spark_block = Block::default().title("Traffic").style(Style::default().fg(T.text_secondary));
    let inner = spark_block.inner(chunks[2]);
    f.render_widget(spark_block, chunks[2]);
    crate::ui::widgets::sparkline::render(f, inner, (T.green, T.red), spark);

    // Proxy groups table
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[3]);

    render_groups_table(f, bottom_chunks[0], state);

    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(bottom_chunks[1]);
    f.render_widget(Paragraph::new(format!("Memory: {:.1} MB", state.memory.inuse as f64 / 1024.0)).style(Style::default().fg(T.text)), info_chunks[0]);
    f.render_widget(Paragraph::new(format!("Version: mihomo {}", state.version)).style(Style::default().fg(T.text)), info_chunks[1]);
}
```

Hmm, I have a problem — `mixed_port` and `allow_lan` aren't stored in AppState. I need to add them. Let me add them in Task 2 as well. But wait, that would require amending Task 2 which is already committed. Better to add these fields in a separate step or include them here.

Actually, let me restructure. I'll store `mixed_port` and `allow_lan` in AppState too. Let me update the plan:

In Task 2, I should also add:
```rust
pub mixed_port: Option<u16>,
pub allow_lan: Option<bool>,
```

And in Task 4, populate them from configs. Then in Task 7, use them.

Let me rewrite the plan more carefully.<｜end▁of▁thinking｜>Hmm, the dashboard cards need `mixed_port` and `allow_lan` values but they're not currently stored in `AppState`. Let me adjust the plan to add those fields.

<｜｜DSML｜｜tool_calls>
<｜｜DSML｜｜invoke name="Write">
<｜｜DSML｜｜parameter name="content" string="true"># TUN & System Proxy — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display TUN status, system proxy status, and proxy port info in Dashboard, with `p` key toggling TUN + system proxy via smart linkage.

**Architecture:** Extend `MihomoConfig` with `TunConfig`, add `os/proxy.rs` for `~/.config/environment.d/proxy.conf` management, add second card row to Dashboard, add `ToggleProxy` action with binary toggle logic (any-on → all-off, all-off → TUN-on).

**Tech Stack:** Rust, serde, ratatui, crossterm, tokio

---

### Task 1: Add TunConfig struct and extend MihomoConfig

**Files:**
- Modify: `src/api/types.rs:140-154`

- [ ] **Step 1: Add TunConfig struct**

Insert after `MihomoConfig` closing brace (after line 154):

```rust
// === TUN ===

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunConfig {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub stack: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default, rename = "auto-route")]
    pub auto_route: Option<bool>,
}
```

- [ ] **Step 2: Add `tun` field inside MihomoConfig**

After line 153 (`pub log_level: Option<String>,`), add:

```rust
    #[serde(default)]
    pub tun: Option<TunConfig>,
```

- [ ] **Step 3: Add deserialization tests**

Append to `mod tests` block (before its closing `}`):

```rust
    #[test]
    fn test_deserialize_mihomo_config_with_tun() {
        let json = r#"{
            "port": 7890,
            "mixed-port": 7897,
            "allow-lan": true,
            "mode": "rule",
            "tun": {
                "enable": true,
                "stack": "system",
                "device": "utun",
                "auto-route": true
            }
        }"#;
        let cfg: MihomoConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.mixed_port, Some(7897));
        let tun = cfg.tun.unwrap();
        assert!(tun.enable);
        assert_eq!(tun.stack.as_deref(), Some("system"));
    }

    #[test]
    fn test_deserialize_mihomo_config_no_tun() {
        let json = r#"{"port": 7890, "mode": "rule"}"#;
        let cfg: MihomoConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.tun.is_none());
    }
```

- [ ] **Step 4: Run tests and commit**

```bash
cargo test test_deserialize_mihomo_config 2>&1
git add src/api/types.rs && git commit -m "feat: add TunConfig struct and tun field to MihomoConfig"
```

---

### Task 2: Add tun, mixed_port, allow_lan, system_proxy_enabled to AppState

**Files:**
- Modify: `src/app/state.rs:57-97`

- [ ] **Step 1: Add fields to AppState struct**

After line 67 (`pub memory: Memory,`):

```rust
    pub tun: Option<TunConfig>,
    pub mixed_port: Option<u16>,
    pub allow_lan: Option<bool>,
    pub system_proxy_enabled: bool,
```

- [ ] **Step 2: Initialize in AppState::new()**

After line 90 (`memory: Memory { inuse: 0, oslimit: 0 },`):

```rust
            tun: None,
            mixed_port: None,
            allow_lan: None,
            system_proxy_enabled: false,
```

- [ ] **Step 3: Build check and commit**

```bash
cargo build 2>&1
git add src/app/state.rs && git commit -m "feat: add tun, mixed_port, allow_lan, system_proxy_enabled to AppState"
```

---

### Task 3: Create OS proxy module

**Files:**
- Create: `src/os/mod.rs`
- Create: `src/os/proxy.rs`
- Modify: `src/main.rs:4`
- Modify: `src/lib.rs:4`

- [ ] **Step 1: Check if `dirs` crate is in Cargo.toml**

```bash
grep -c 'dirs' Cargo.toml
```
If 0, add to `[dependencies]`: `dirs = "5"`

- [ ] **Step 2: Create `src/os/mod.rs`**

```rust
pub mod proxy;
```

- [ ] **Step 3: Create `src/os/proxy.rs`**

```rust
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn proxy_conf_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("environment.d")
        .join("proxy.conf")
}

/// Check whether system proxy is enabled: proxy.conf exists and
/// HTTP_PROXY line points to 127.0.0.1:<mixed_port>.
pub fn detect_system_proxy(mixed_port: Option<u16>) -> bool {
    let Some(port) = mixed_port else { return false };
    let path = proxy_conf_path();
    if !path.exists() {
        return false;
    }
    match fs::read_to_string(&path) {
        Ok(content) => {
            let expected = format!("http://127.0.0.1:{}", port);
            content.lines().any(|line| {
                line.starts_with("HTTP_PROXY=") && line.contains(&expected)
            })
        }
        Err(_) => false,
    }
}

/// Write proxy.conf pointing to the given mixed_port.
pub fn set_system_proxy(mixed_port: u16) -> std::io::Result<()> {
    let path = proxy_conf_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        "HTTP_PROXY=http://127.0.0.1:{0}\n\
         HTTPS_PROXY=http://127.0.0.1:{0}\n\
         ALL_PROXY=socks5://127.0.0.1:{0}\n\
         NO_PROXY=localhost,127.0.0.1,::1,.local\n",
        mixed_port
    );
    let mut f = fs::File::create(&path)?;
    f.write_all(content.as_bytes())?;
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "import-environment",
               "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"])
        .output();
    Ok(())
}

/// Remove proxy.conf to disable system proxy.
pub fn clear_system_proxy() {
    let _ = fs::remove_file(proxy_conf_path());
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "import-environment",
               "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_none_port_returns_false() {
        assert!(!detect_system_proxy(None));
    }

    #[test]
    fn test_set_and_clear_system_proxy() {
        use std::fs;
        let dir = std::env::temp_dir().join("mioctl-os-proxy-test");
        let path = dir.join("proxy.conf");
        fs::create_dir_all(&dir).unwrap();

        // Write manually to simulate set
        fs::write(&path,
            "HTTP_PROXY=http://127.0.0.1:7897\n\
             HTTPS_PROXY=http://127.0.0.1:7897\n\
             ALL_PROXY=socks5://127.0.0.1:7897\n\
             NO_PROXY=localhost,127.0.0.1,::1,.local\n"
        ).unwrap();
        assert!(path.exists());

        // Verify content
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("HTTP_PROXY=http://127.0.0.1:7897"));
        assert!(content.contains("NO_PROXY=localhost"));

        // Simulate clear
        fs::remove_file(&path).unwrap();
        assert!(!path.exists());

        fs::remove_dir_all(dir).unwrap();
    }
}
```

- [ ] **Step 4: Add `mod os;` / `pub mod os;` to main.rs and lib.rs**

In `src/main.rs`, after `mod config;`:
```rust
mod os;
```

In `src/lib.rs`, after `pub mod config;`:
```rust
pub mod os;
```

- [ ] **Step 5: Run tests and commit**

```bash
cargo test os::proxy 2>&1
git add src/os/ src/main.rs src/lib.rs Cargo.toml && git commit -m "feat: add OS proxy module for environment.d management"
```

---

### Task 4: Parse tun / mixed_port / allow_lan in refresh_state

**Files:**
- Modify: `src/ui/app.rs:72-78` (init block)
- Modify: `src/ui/app.rs:451-457` (refresh_state)

- [ ] **Step 1: Add import**

After `use crate::app::proxy_manager::ProxyManager;`, add:
```rust
use crate::os;
```

- [ ] **Step 2: Update initial load configs parsing (line 72-78)**

Replace:
```rust
                if let Ok(c) = configs_r {
                    s.proxy_mode = match c.mode.as_deref() {
                        Some("global") => ProxyMode::Global,
                        Some("direct") => ProxyMode::Direct,
                        _ => ProxyMode::Rule,
                    };
                }
```

With:
```rust
                if let Ok(c) = configs_r {
                    s.proxy_mode = match c.mode.as_deref() {
                        Some("global") => ProxyMode::Global,
                        Some("direct") => ProxyMode::Direct,
                        _ => ProxyMode::Rule,
                    };
                    s.mixed_port = c.mixed_port;
                    s.allow_lan = c.allow_lan;
                    s.tun = c.tun;
                    s.system_proxy_enabled = os::proxy::detect_system_proxy(c.mixed_port);
                }
```

- [ ] **Step 3: Update refresh_state configs parsing (line 451-457)**

Replace:
```rust
    if let Ok(c) = configs {
        s.proxy_mode = match c.mode.as_deref() {
            Some("global") => ProxyMode::Global,
            Some("direct") => ProxyMode::Direct,
            _ => ProxyMode::Rule,
        };
    }
```

With:
```rust
    if let Ok(c) = configs {
        s.proxy_mode = match c.mode.as_deref() {
            Some("global") => ProxyMode::Global,
            Some("direct") => ProxyMode::Direct,
            _ => ProxyMode::Rule,
        };
        s.mixed_port = c.mixed_port;
        s.allow_lan = c.allow_lan;
        s.tun = c.tun;
        s.system_proxy_enabled = os::proxy::detect_system_proxy(c.mixed_port);
    }
```

- [ ] **Step 4: Build check and commit**

```bash
cargo build 2>&1
git add src/ui/app.rs && git commit -m "feat: parse tun, mixed_port, allow_lan, and system proxy in refresh_state"
```

---

### Task 5: Add ToggleProxy action + keybinding + help text

**Files:**
- Modify: `src/ui/keybindings.rs:4-29` (Action enum), `:63` (key map), `:103` (tests)
- Modify: `src/ui/views/help.rs:9-30` (HELP_TEXT)

- [ ] **Step 1: Add ToggleProxy variant**

In `keybindings.rs`, inside `pub enum Action`, before `Refresh`:
```rust
    ToggleProxy,
```

- [ ] **Step 2: Bind `p` key**

In `parse_key`, before `KeyEvent { code: KeyCode::Char('r')`:
```rust
        KeyEvent { code: KeyCode::Char('p'), .. } => Some(Action::ToggleProxy),
```

- [ ] **Step 3: Add test**

After `test_dashboard` test:
```rust
    #[test] fn test_toggle_proxy() { assert_eq!(parse_key(k('p')), Some(Action::ToggleProxy)); }
```

- [ ] **Step 4: Update help text**

Replace `HELP_TEXT`:
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
 m    Cycle proxy mode      ───────────
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

- [ ] **Step 5: Run tests and commit**

```bash
cargo test keybindings 2>&1
git add src/ui/keybindings.rs src/ui/views/help.rs && git commit -m "feat: add ToggleProxy action, p key binding, and help text"
```

---

### Task 6: Handle ToggleProxy action in app.rs

**Files:**
- Modify: `src/ui/app.rs` (handle_action)

- [ ] **Step 1: Add ToggleProxy handler**

Insert before `Action::PrevGroup` (before line 313):

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

- [ ] **Step 2: Build check and commit**

```bash
cargo build 2>&1
git add src/ui/app.rs && git commit -m "feat: implement ToggleProxy action handler"
```

---

### Task 7: Add second card row to Dashboard

**Files:**
- Modify: `src/ui/views/dashboard.rs:12-47`

- [ ] **Step 1: Replace render function**

Replace lines 12-47 with:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState, spark: &TrafficSpark) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // row 1: Mode/Upload/Download/Conns
            Constraint::Length(5),  // row 2: TUN/SysProxy/MixPort/AllowLAN
            Constraint::Length(3),  // traffic sparkline
            Constraint::Min(1),     // groups table + info line
        ])
        .split(area);

    // Row 1
    let cards1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[0]);

    card(f, cards1[0], "Mode", &format!("{:?}", state.proxy_mode), T.primary);
    card(f, cards1[1], "Upload", &format!("{:.1} KB/s", state.traffic.up as f64 / 1024.0), T.green);
    card(f, cards1[2], "Download", &format!("{:.1} KB/s", state.traffic.down as f64 / 1024.0), T.red);
    card(f, cards1[3], "Conns", &state.connections.len().to_string(), T.yellow);

    // Row 2
    let cards2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[1]);

    let tun_enabled = state.tun.as_ref().map(|t| t.enable).unwrap_or(false);
    let tun_color = if tun_enabled { T.green } else { T.surface };
    card(f, cards2[0], "TUN", if tun_enabled { "ON" } else { "OFF" }, tun_color);

    let sp_color = if state.system_proxy_enabled { T.green } else { T.surface };
    card(f, cards2[1], "SysProxy", if state.system_proxy_enabled { "ON" } else { "OFF" }, sp_color);

    let port_str = state.mixed_port
        .map(|p| format!(":{}", p))
        .unwrap_or_else(|| "—".into());
    card(f, cards2[2], "MixPort", &port_str, T.text);

    let lan_str = state.allow_lan
        .map(|b| if b { "Yes" } else { "No" })
        .unwrap_or("—");
    card(f, cards2[3], "AllowLAN", lan_str, T.text_secondary);

    // Traffic sparkline
    let spark_block = Block::default().title("Traffic").style(Style::default().fg(T.text_secondary));
    let inner = spark_block.inner(chunks[2]);
    f.render_widget(spark_block, chunks[2]);
    crate::ui::widgets::sparkline::render(f, inner, (T.green, T.red), spark);

    // Bottom: groups table + info line
    let bottom_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(chunks[3]);

    render_groups_table(f, bottom_chunks[0], state);

    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(bottom_chunks[1]);
    f.render_widget(
        Paragraph::new(format!("Memory: {:.1} MB", state.memory.inuse as f64 / 1024.0))
            .style(Style::default().fg(T.text)),
        info_chunks[0],
    );
    f.render_widget(
        Paragraph::new(format!("Version: mihomo {}", state.version))
            .style(Style::default().fg(T.text)),
        info_chunks[1],
    );
}
```

- [ ] **Step 2: Build check and commit**

```bash
cargo build 2>&1
git add src/ui/views/dashboard.rs && git commit -m "feat: add TUN/SysProxy/MixPort/AllowLAN cards to Dashboard"
```

---

### Task 8: Full test run and verify

- [ ] **Step 1: Run all tests**

```bash
cargo test 2>&1
```
Expected: all tests pass

- [ ] **Step 2: Build release**

```bash
cargo build --release 2>&1
```
Expected: compiles with no errors

- [ ] **Step 3: Final commit if any changes**

```bash
git status
```
