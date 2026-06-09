# Loading Indicator Design Spec

**Date:** 2026-06-09
**Status:** Approved
**Scope:** Add visual loading feedback for all async operations in the TUI

## Problem

All async operations (mode switch, node switch, TUN toggle, refresh, delay test, subscription update) run via `tokio::spawn` with no visual feedback. Users press a key and see nothing until data arrives. Only subscription updates had a text status via `update_status`, other operations are completely silent.

## Solution

A unified loading indicator system using a `LoadingKind` enum, Braille dot spinner animation in the status bar, and contextual highlighting on dashboard cards for the relevant operation.

## Data Model

### LoadingKind Enum (`src/app/state.rs`)

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum LoadingKind {
    Init,            // Initial connection and data load
    Refresh,         // Manual refresh (r key)
    SwitchMode,      // Proxy mode switch
    SwitchNode,      // Proxy node switch
    ToggleProxy,     // TUN/system proxy toggle
    TestNodeDelay,   // Single node delay test
    TestGroupDelay,  // Group delay test
    UpdateSubs,      // Subscription update
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

### UiState Changes (`src/app/state.rs`)

**Add:**
- `loading: Option<LoadingKind>` — currently active operation, `None` when idle
- `spinner_frame: u8` — current animation frame (0–9 cycling through Braille dots)

**Remove:**
- `update_status: Option<String>` — replaced by `loading`; subscription update status now uses the same mechanism

Default values: `loading: None`, `spinner_frame: 0`.

AppState::new() sets `loading: Some(LoadingKind::Init)` to show spinner during initial load.

### Spinner Animation

Braille dot sequence: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` (10 frames).

The `spinner_frame` field indexes into this array. Frame advances on every render cycle (100ms poll interval provides natural ~10fps animation).

## UI Feedback

### Status Bar (`src/ui/widgets/status_bar.rs`)

When `loading.is_some()`:
- Show spinner character + description in yellow: `⠋ Switching mode...`
- Placed in the same position as the former `update_status`
- Example: ` connected | mihomo v1.18 | 14:32:05 | ⠋ Switching mode... | 1-5 views | ? help | q quit`

When `loading.is_none()`:
- Normal status bar, no extra text

### Dashboard Cards (`src/ui/views/dashboard.rs`)

Contextual feedback for operations that affect specific dashboard cards:

| LoadingKind | Card | Display |
|---|---|---|
| `SwitchMode` | Mode | Value becomes spinner: `⠋ ...` |
| `ToggleProxy` | TUN | Value becomes spinner: `⠋ ...` |
| `ToggleProxy` | SysProxy | Value becomes spinner: `⠋ ...` |
| All others | — | Status bar only (cards unchanged) |

Only Mode, TUN, and SysProxy cards get value-level feedback. All other operations show spinner only in the status bar. This keeps the card logic simple and focused.

## Integration Pattern

### Spawn Wrapper

Every `tokio::spawn` in `handle_action` follows this pattern:

```rust
// Before spawn: set loading
s.ui.loading = Some(LoadingKind::SwitchMode);
let s2 = shared.clone();
tokio::spawn(async move {
    let result = some_async_operation().await;
    if result.is_ok() {
        refresh_state(&s2).await;
    }
    // Always clear loading when done
    let mut s = s2.lock().await;
    s.ui.loading = None;
});
```

Key rules:
- `loading` is set **before** spawn (synchronous, under UI lock)
- `loading` is cleared **inside** spawn after operation completes (success or failure)
- `refresh_state` is called on success before clearing loading

### Spinner Tick (`src/ui/app.rs` main loop)

After every poll cycle, before rendering:

```rust
{
    let mut s = state.lock().await;
    if s.ui.loading.is_some() {
        s.ui.spinner_frame = (s.ui.spinner_frame + 1) % 10;
    }
}
```

This leverages the existing 100ms poll interval for animation timing. No additional timer needed.

### Init Handling

`AppState::new()` sets `loading: Some(LoadingKind::Init)`. The init spawn clears it after all data loads. This shows a spinner from the first frame of the TUI until initial data arrives.

## Error Handling

Failed operations clear `loading` (the spawn always sets `loading = None` at the end). No explicit error toast — the spinner simply disappears and the data remains unchanged. This matches the current behavior where errors are silently handled.

## Files Modified

| File | Changes |
|---|---|
| `src/app/state.rs` | Add `LoadingKind` enum, add `loading`/`spinner_frame` to `UiState`, remove `update_status`, update `Default` impl |
| `src/ui/app.rs` | Add spinner tick in main loop, add `loading = Some(...)` before every spawn, clear `loading` inside every spawn, remove `update_status` usage in `UpdateSubs` |
| `src/ui/widgets/status_bar.rs` | Render spinner + description when `loading.is_some()` |
| `src/ui/views/dashboard.rs` | Mode/TUN/SysProxy card value replacement when relevant `loading` is active |

4 files total. No new files needed.

## Out of Scope

- Error toasts / failure notifications (future work)
- Progress bars for subscription updates (content length unknown)
- Per-node delay spinner in proxy list (would require per-node state)
- Canceling in-progress operations
