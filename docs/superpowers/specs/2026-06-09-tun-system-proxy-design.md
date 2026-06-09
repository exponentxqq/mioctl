# TUN & System Proxy Display & Toggle

Date: 2026-06-09

## Goal

Display TUN mode status, system proxy status, and proxy port info in the Dashboard view. Provide a single hotkey to toggle TUN + system proxy with smart linkage.

## Background

- mihomo `/configs` API returns `tun.enable` (bool) and port fields (`mixed-port`, `port`, `socks-port`).
- System proxy is NOT managed by mihomo — it's an OS-level setting.
- On Linux, we use `~/.config/environment.d/proxy.conf` (systemd user environment) to set/clear `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`.
- No sudo required. Changes take effect on next login (or `systemctl --user import-environment` for current session).

## Design

### 1. Data Layer

**`api/types.rs` — extend `MihomoConfig`:**

```rust
pub struct TunConfig {
    pub enable: bool,
    pub stack: Option<String>,       // "system" | "gvisor" | "mixed"
    pub device: Option<String>,
    pub auto_route: Option<bool>,
}

// Add to MihomoConfig:
pub tun: Option<TunConfig>,
```

**`app/state.rs` — extend `AppState`:**

```rust
pub tun: Option<TunConfig>,
pub system_proxy_enabled: bool,
```

**`refresh_state`** — parse `tun` from configs response; call `detect_system_proxy()` to detect OS proxy.

### 2. OS Proxy Module — `src/os/proxy.rs`

File path: `~/.config/environment.d/proxy.conf`

- **detect**: read the file, check if `HTTP_PROXY` points to `127.0.0.1:<mixed-port>`
- **enable**: write `HTTP_PROXY=http://127.0.0.1:<port>` etc. to the file
- **disable**: remove the file (or clear proxy lines)

For current-session effect: run `systemctl --user import-environment HTTP_PROXY HTTPS_PROXY ALL_PROXY NO_PROXY` after writing.

### 3. Dashboard Display

Add a row of status cards and a ports info section:

```
┌──────┐ ┌─────────┐ ┌──────────┐ ┌──────────┐
│ Mode │ │  Upload │ │ Download │ │  Conns   │
│Rule  │ │ 0.1KB/s │ │ 0.2KB/s  │ │   12     │
└──────┘ └─────────┘ └──────────┘ └──────────┘
┌──────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│ TUN  │ │ SysProxy │ │ MixPort  │ │Allow LAN │
│ OFF  │ │   ON     │ │  :7897   │ │   No     │
└──────┘ └──────────┘ └──────────┘ └──────────┘
[        Traffic Sparkline             ]
┌─ Proxy Groups ──────────────────────────────┐
│ Group    │ Type     │ Current Node          │
│ ...      │ ...      │ ...                   │
└──────────────────────────────────────────────┘
 Memory: 23.4 MB          Version: mihomo 1.18
```

Status indicators use color: green = ON, red/surface = OFF.

### 4. Smart Toggle — `Action::ToggleProxy`

Bound to key `p` (proxy toggle).

Logic:
1. Read current `tun.enable` and `system_proxy_enabled`
2. If TUN is ON:
   - Disable TUN via `PATCH /configs {"tun": {"enable": false}}`
   - Enable system proxy (write proxy.conf)
3. If TUN is OFF and system proxy is OFF:
   - Enable TUN via `PATCH /configs {"tun": {"enable": true}}`
   - Disable system proxy (remove proxy.conf)
4. If TUN is OFF and system proxy is ON:
   - Disable system proxy (remove proxy.conf)

Simplified: one key cycles through states `TUN ON` → `SysProxy ON` → `All OFF` → `TUN ON`.

Or even simpler: binary toggle — if any proxy is active, turn all off; if all off, turn TUN on.

**Chosen behavior**: binary toggle.
- Current state has any proxy active → disable TUN + disable system proxy
- Current state has no proxy → enable TUN + disable system proxy

### 5. Keybinding

- `p` — Toggle proxy (TUN / system proxy / off)

### 6. Files to Change

| File | Change |
|------|--------|
| `src/api/types.rs` | Add `TunConfig` struct, add `tun` field to `MihomoConfig` |
| `src/app/state.rs` | Add `tun` and `system_proxy_enabled` fields to `AppState` |
| `src/os/mod.rs` | New module |
| `src/os/proxy.rs` | Detect/enable/disable system proxy via environment.d |
| `src/ui/views/dashboard.rs` | Add TUN/SysProxy/Ports cards row |
| `src/ui/keybindings.rs` | Add `ToggleProxy` action, bind `p` key |
| `src/ui/app.rs` | Handle `ToggleProxy` action, update `refresh_state` |
| `src/ui/views/help.rs` | Add `p Toggle proxy` to help text |
| `src/main.rs` | Add `mod os;` |

### 7. Testing

- Unit tests for `TunConfig` deserialization
- Unit tests for `detect_system_proxy` with mock file content
- Unit tests for `set_system_proxy` / `clear_system_proxy` file output
- Integration: verify `PATCH /configs` with tun payload
