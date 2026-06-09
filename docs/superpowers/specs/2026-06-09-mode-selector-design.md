# Mode Selector Popup Design

## Summary

Replace the current `m` key mode cycling (Rule → Global → Direct → Rule) with a centered popup selector that displays all three modes with descriptions, allowing direct selection via arrow keys and Enter.

## Motivation

Current mode switching is a blind cycle — users press `m` repeatedly without seeing what modes are available or which one they're targeting. A selector popup provides visibility and direct access.

## Interaction Flow

### Open

Press `m` → opens centered popup selector over the current view.

### Popup Layout (~50% width × 55% height, centered)

```
┌─────────────────────────────────┐
│       Proxy Mode  ↑↓/jk 选      │
│─────────────────────────────────│
│ ✓ Rule                          │
│   按规则路由，匹配分流策略        │
│                                 │
│   Global                        │
│   全部流量经代理服务器转发        │
│                                 │
│   Direct                        │
│   全部流量直连，不经代理          │
│                                 │
│ Enter 确认 · Esc 取消            │
└─────────────────────────────────┘
```

### Controls

| Key | Action |
|-----|--------|
| `j` / `↓` | Move highlight down |
| `k` / `↑` | Move highlight up |
| `Enter` | Confirm selection, switch mode, close popup |
| `Esc` | Cancel, close popup without switching |
| Mouse click outside popup | Close popup without switching |

### Behavior

- Highlight starts on the **current mode** (not index 0)
- Current mode has a `✓` prefix
- Highlighted item uses `T.primary` color
- On confirm: call `MihomoClient::patch_configs({"mode": ...})` → `refresh_state()` → close popup
- On API failure: close popup, leave mode unchanged (no extra error toast — matches existing style)
- Selector blocks all other keys while open (view switching, refresh, etc.)

### Mutual Exclusion

- Opening mode selector closes Help / Settings popups
- Opening Help / Settings closes mode selector
- Follows existing popup mutual-exclusion pattern

## Code Changes

### 1. `src/app/state.rs` — State fields

Add to `UiState`:

```rust
pub show_mode_selector: bool,
pub mode_selector_idx: usize,  // highlighted index (0–2), initialized to current mode
```

### 2. `src/ui/keybindings.rs` — Action enum

Rename `CycleMode` → `OpenModeSelector`. No new actions needed — existing Up/Down/Enter/Back actions are reused when the selector is open.

### 3. `src/ui/views/mode_selector.rs` — New file (~100 lines)

Render function:
- `centered_rect(50, 55, f.area())` for sizing
- Iterate `["rule", "global", "direct"]` with name + description pairs
- Render each item: `✓` for current mode, highlight for selected item
- Footer line: `Enter 确认 · Esc 取消`

Mode descriptions:
- Rule: `按规则路由，匹配分流策略`
- Global: `全部流量经代理服务器转发`
- Direct: `全部流量直连，不经代理`

### 4. `src/ui/app.rs` — Event handling changes

**`OpenModeSelector` action handler:**
```rust
Action::OpenModeSelector => {
    s.ui.show_mode_selector = !s.ui.show_mode_selector;
    if s.ui.show_mode_selector {
        s.ui.show_help = false;
        s.ui.show_settings = false;
        // Set highlight to current mode
        s.ui.mode_selector_idx = match s.proxy_mode {
            ProxyMode::Rule => 0,
            ProxyMode::Global => 1,
            ProxyMode::Direct => 2,
        };
    }
}
```

**Key event interception when selector is open:**
When `show_mode_selector` is true, intercept key events before the normal action dispatch:
- `j/↓` → `mode_selector_idx = min(2, idx + 1)`
- `k/↑` → `mode_selector_idx = max(0, idx - 1)`
- `Enter` → spawn async task to call `set_proxy_mode()`, close selector
- `Esc`/`Back` → close selector

**Render pipeline:**
Add after existing popup renders:
```rust
if state.ui.show_mode_selector {
    mode_selector::render(f, state);
}
```

**Mouse handler:**
Add mode selector to the click-outside-to-close check alongside help/settings.

### 5. `src/app/proxy_manager.rs` — New method

```rust
pub async fn set_proxy_mode(client: &MihomoClient, mode: ProxyMode) -> ApiResult<()> {
    let mode_str = match mode {
        ProxyMode::Global => "global",
        ProxyMode::Rule => "rule",
        ProxyMode::Direct => "direct",
    };
    client.patch_configs(serde_json::json!({"mode": mode_str})).await
}
```

Remove `cycle_proxy_mode` — replaced entirely by the selector + `set_proxy_mode`.

### 6. `src/ui/views/help.rs` — Update help text

Change `m Cycle proxy mode` → `m Open mode selector`

### 7. `src/ui/views/dashboard.rs` — No changes required

Mode card continues to display current mode text.

## Files Changed

| File | Change |
|------|--------|
| `src/app/state.rs` | Add 2 fields to `UiState` |
| `src/ui/keybindings.rs` | Rename `CycleMode` → `OpenModeSelector` |
| `src/ui/views/mode_selector.rs` | **New file** — popup rendering |
| `src/ui/app.rs` | Action handler + key interception + render |
| `src/app/proxy_manager.rs` | Add `set_proxy_mode()` method |
| `src/ui/views/help.rs` | Update help text |
| `src/ui/mod.rs` | Register `mode_selector` module |

Estimated ~120–150 lines of new code.

## Error Handling

- API call failure: selector closes, mode unchanged, no extra UI feedback (matches existing CycleMode behavior)
- Invalid index: clamped to `[0, 2]` in event handler

## Testing

- Unit test: `set_proxy_mode()` sends correct mode string
- Unit test: mode selector index initialization matches current mode
- Manual test: open selector, navigate, confirm, verify mode switches via dashboard card
