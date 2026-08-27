use std::{io::Write, time::Duration};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Terminal,
};

use crate::api::client::MihomoClient;
use crate::api::types::{Proxy, ProxyHistory, TunConfig};
use crate::app::connection_manager::ConnectionManager;
use crate::app::proxy_manager::ProxyManager;
use crate::app::state::{
    ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState, UiState, LOG_CAP,
};
use crate::config::mioctl_config::MioctlConfig;
use crate::os;
use crate::subscription::manager::{SubscriptionManager, UpdateTarget};
use crate::ui::keybindings::{parse_key, parse_mouse, Action};
use crate::ui::views::{
    connections, dashboard, help, logs, mode_selector, proxies, rules, settings, sidebar,
    subscriptions,
};
use crate::ui::widgets::{sparkline::TrafficSpark, status_bar};
use std::collections::HashMap;

/// Collect payload text from the selected log range, joined by newlines.
fn collect_log_selection(state: &AppState) -> String {
    let start = state.ui.log_select_start.min(state.ui.log_select_end);
    let end = state.ui.log_select_start.max(state.ui.log_select_end);
    let end = end.min(state.logs.len().saturating_sub(1));
    if start > end {
        return String::new();
    }
    state.logs[start..=end]
        .iter()
        .map(|e| e.payload.as_str())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Start a background task that re-applies system proxy every 30s.
/// Prevents other apps/system-updates from clearing the proxy settings.
fn start_proxy_guard(shared: SharedState, port: u16) -> tokio::task::AbortHandle {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            let enabled = {
                let s = shared.lock().await;
                s.system_proxy_enabled
            };
            if !enabled {
                break;
            }
            // Re-apply gsettings (browser)
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy", "mode", "manual"])
                .output();
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"])
                .output();
            let _ = std::process::Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy.https", "host", "127.0.0.1"])
                .output();
            let _ = std::process::Command::new("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy.http",
                    "port",
                    &port.to_string(),
                ])
                .output();
            let _ = std::process::Command::new("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy.https",
                    "port",
                    &port.to_string(),
                ])
                .output();
            // Re-write proxy.env in case it was deleted
            let env_content = format!(
                "export HTTP_PROXY=http://127.0.0.1:{port}\n\
                 export http_proxy=http://127.0.0.1:{port}\n\
                 export HTTPS_PROXY=http://127.0.0.1:{port}\n\
                 export https_proxy=http://127.0.0.1:{port}\n\
                 export NO_PROXY=localhost,127.0.0.1,::1,.local\n\
                 export no_proxy=localhost,127.0.0.1,::1,.local\n",
            );
            let env_path = dirs::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("mioctl")
                .join("proxy.env");
            let _ = std::fs::write(&env_path, &env_content);
        }
    })
    .abort_handle()
}

/// Copy text to system clipboard via xclip (X11) or wl-copy (Wayland).
fn copy_to_clipboard(text: &str) -> bool {
    use std::process::{Command, Stdio};

    let (cmd, args) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
        ("wl-copy", vec![])
    } else {
        ("xclip", vec!["-selection", "clipboard"])
    };

    match Command::new(cmd).args(&args).stdin(Stdio::piped()).spawn() {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait().is_ok()
        }
        Err(_) => false,
    }
}

/// Toggle `tun.enable` in a mihomo YAML config file (best-effort persistence).
/// Creates a `tun:` section if missing. Only effective when `path` is the file
/// mihomo actually loads (e.g. `mihomo -f <path>`); runtime switching is done
/// via `PATCH /configs` in `toggle_tun_flow`.
fn set_tun_config(path: &str, enable: bool) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?;
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("parse YAML: {}", e))?;
    let mapping = doc
        .as_mapping_mut()
        .ok_or_else(|| "config is not a YAML mapping".to_string())?;
    if !mapping.contains_key(serde_yaml::Value::String("tun".into())) {
        mapping.insert(
            serde_yaml::Value::String("tun".into()),
            serde_yaml::Value::Mapping(serde_yaml::Mapping::new()),
        );
    }
    let tun = mapping
        .get_mut(serde_yaml::Value::String("tun".into()))
        .and_then(|v| v.as_mapping_mut())
        .ok_or_else(|| "'tun' section is not a mapping".to_string())?;
    tun.insert(
        serde_yaml::Value::String("enable".into()),
        serde_yaml::Value::Bool(enable),
    );
    let out = serde_yaml::to_string(&doc).map_err(|e| format!("serialize YAML: {}", e))?;
    std::fs::write(path, out).map_err(|e| format!("write {}: {}", path, e))?;
    Ok(())
}

/// Build the `PATCH /configs` payload for toggling TUN at runtime.
///
/// Only `enable` (plus a fallback `stack` when enabling without one) is sent;
/// mihomo keeps every other runtime TUN field (dns-hijack, mtu, ...) as-is via
/// its `LastTunConf` default. Fields present in the current runtime config are
/// passed through so the toggle never resets them.
fn tun_patch_payload(current: Option<&TunConfig>, enable: bool) -> serde_json::Value {
    let mut tun = serde_json::Map::new();
    tun.insert("enable".into(), serde_json::Value::Bool(enable));
    if let Some(c) = current {
        if let Some(stack) = &c.stack {
            tun.insert("stack".into(), serde_json::Value::String(stack.clone()));
        }
        if let Some(device) = &c.device {
            tun.insert("device".into(), serde_json::Value::String(device.clone()));
        }
        if let Some(auto_route) = c.auto_route {
            tun.insert("auto-route".into(), serde_json::Value::Bool(auto_route));
        }
    }
    if enable && !tun.contains_key("stack") {
        tun.insert("stack".into(), serde_json::Value::String("system".into()));
    }
    serde_json::json!({ "tun": tun })
}

pub async fn run_tui() -> Result<(), String> {
    let state: SharedState = crate::app::state::new_shared_state();

    // Background: connect + load initial data concurrently (max 3s wall time)
    let init_handle = {
        let s = state.clone();
        tokio::spawn(async move {
            let client = {
                let mut s = s.lock().await;
                s.connect();
                s.client.clone()
            };
            let Some(ref client) = client else {
                return;
            };

            // Concurrent init — all requests in parallel, max 3s total
            let t = Duration::from_secs(3);
            let (version_r, proxies_r, conns_r, rules_r, traffic_r, configs_r, memory_r) = tokio::join!(
                tokio::time::timeout(t, client.get_version()),
                tokio::time::timeout(t, ProxyManager::refresh_all(client)),
                tokio::time::timeout(t, ConnectionManager::list(client)),
                tokio::time::timeout(t, client.get_rules()),
                tokio::time::timeout(t, client.get_traffic()),
                tokio::time::timeout(t, client.get_configs()),
                tokio::time::timeout(t, client.get_memory()),
            );

            // Unwrap timeout layer, keeping inner ApiResult
            let version_r = version_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let proxies_r = proxies_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let conns_r = conns_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let rules_r = rules_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let traffic_r = traffic_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let configs_r = configs_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));
            let memory_r = memory_r.unwrap_or(Err(crate::api::error::ApiError::Timeout));

            // Lock briefly to update state
            {
                let mut s = s.lock().await;
                if let Ok(v) = version_r {
                    s.version = v.version;
                    s.connected = true;
                }
                if let Ok((p, g)) = proxies_r {
                    s.proxies = p;
                    s.groups = g;
                }
                if let Ok(c) = conns_r {
                    s.connections = c;
                }
                if let Ok(r) = rules_r {
                    s.rules = r;
                }
                if let Ok(t) = traffic_r {
                    s.traffic = t;
                }
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
                if let Ok(m) = memory_r {
                    s.memory = m;
                }
                s.update_time();
                s.add_log("info", "Connected");
                s.ui.loading = None;
            }

            migrate_startup_archives(&s).await;

            // Start log stream — writes directly into state.logs
            let log_client = client.clone();
            let log_state = s.clone();
            tokio::spawn(async move {
                if let Ok(mut rx) = log_client.log_stream(None).await {
                    while let Some(entry) = rx.recv().await {
                        let mut state = log_state.lock().await;
                        state.logs.push(entry);
                        while state.logs.len() > LOG_CAP {
                            state.logs.remove(0);
                        }
                    }
                }
            });
        })
    };

    // Setup terminal
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| e.to_string())?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let spark = TrafficSpark::new();
    let mut proxy_table = ratatui::widgets::TableState::default();
    let mut conn_table = ratatui::widgets::TableState::default();

    let result = event_loop(
        &mut terminal,
        &state,
        &spark,
        &mut proxy_table,
        &mut conn_table,
    )
    .await;

    init_handle.abort();

    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
    result
}

async fn event_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state: &SharedState,
    spark: &TrafficSpark,
    proxy_table: &mut ratatui::widgets::TableState,
    conn_table: &mut ratatui::widgets::TableState,
) -> Result<(), String> {
    loop {
        if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
                    }
                    if is_quit_interrupt(&key) {
                        break;
                    }
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
                    } else if s.ui.sub_input_mode {
                        if let SubInputOutcome::Submitted(url) =
                            handle_sub_input_key(&mut s.ui, key.code)
                        {
                            handle_sub_input_submitted(&mut s, state.clone(), url);
                        }
                    } else if let Some(name) = s.ui.confirm_remove.clone() {
                        if handle_confirm_key(&mut s.ui, key.code) && s.ui.loading.is_none() {
                            s.ui.loading = Some(LoadingKind::SwitchProfile);
                            let cfg = s.config.clone();
                            spawn_remove_subscription(state.clone(), cfg, name);
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
                                handle_mode_selector_enter(&mut s, state.clone()).await;
                            }
                            KeyCode::Esc => {
                                s.ui.show_mode_selector = false;
                            }
                            _ => {}
                        }
                    } else if s.ui.active_view == Logs && s.ui.log_visual {
                        // Logs visual mode: intercept navigation keys
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => {
                                let m = s.logs.len().saturating_sub(1);
                                s.ui.log_select_end = (s.ui.log_select_end + 1).min(m);
                                s.ui.log_cursor = s.ui.log_select_end;
                            }
                            KeyCode::Char('k') | KeyCode::Up
                                if s.ui.log_select_end > s.ui.log_select_start =>
                            {
                                s.ui.log_select_end = s.ui.log_select_end.saturating_sub(1);
                                s.ui.log_cursor = s.ui.log_select_end;
                            }
                            KeyCode::Char('y') => {
                                let text = collect_log_selection(&s);
                                s.add_log("info", &format!("Visual copy: {} chars", text.len()));
                                s.ui.log_visual = false;
                                drop(s);
                                let copied = copy_to_clipboard(&text);
                                if !copied {
                                    let mut s = state.lock().await;
                                    s.add_log(
                                        "error",
                                        "xclip failed — is xclip installed? (pacman -S xclip)",
                                    );
                                }
                                continue;
                            }
                            KeyCode::Esc => {
                                s.ui.log_visual = false;
                            }
                            _ => {}
                        }
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await {
                            break;
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if let Some(action) = parse_mouse(mouse) {
                        let mut s = state.lock().await;
                        if !dismiss_popups_on_mouse(&mut s.ui) {
                            handle_action(&action, &mut s, state.clone()).await;
                        }
                    }
                }
                _ => {}
            }
        }

        // Advance spinner animation frame if loading
        {
            let mut s = state.lock().await;
            if s.ui.loading.is_some() {
                s.ui.spinner_frame = (s.ui.spinner_frame + 1) % 10;
            }
        }

        let s = state.lock().await;
        terminal
            .draw(|f| render_frame(f, &s, spark, proxy_table, conn_table))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn render_frame(
    f: &mut ratatui::Frame,
    state: &AppState,
    spark: &TrafficSpark,
    proxy_table: &mut ratatui::widgets::TableState,
    conn_table: &mut ratatui::widgets::TableState,
) {
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(16), Constraint::Min(40)])
        .split(f.area());
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(main[1]);

    sidebar::render(f, main[0], state);
    match state.ui.active_view {
        Dashboard => dashboard::render(f, content[0], state, spark),
        Proxies => proxies::render(f, content[0], state, proxy_table),
        Connections => connections::render(f, content[0], state, conn_table),
        Rules => rules::render(f, content[0], state),
        Logs => logs::render(f, content[0], state),
        Subscriptions => subscriptions::render(f, content[0], state),
    }
    status_bar::render(f, content[1], state);

    // Help popup overlay
    if state.ui.show_help {
        help::render(f);
    }

    // Settings popup overlay
    if state.ui.show_settings {
        settings::render(f, state);
    }

    // Mode selector popup overlay
    if state.ui.show_mode_selector {
        mode_selector::render(f, state);
    }
}

enum SubInputOutcome {
    Canceled,
    Submitted(String),
    Editing,
}

fn handle_sub_input_key(ui: &mut UiState, code: KeyCode) -> SubInputOutcome {
    match code {
        KeyCode::Esc => {
            ui.sub_input_mode = false;
            ui.sub_input.clear();
            SubInputOutcome::Canceled
        }
        KeyCode::Enter => {
            let url = std::mem::take(&mut ui.sub_input);
            ui.sub_input_mode = false;
            if url.is_empty() {
                SubInputOutcome::Canceled
            } else {
                SubInputOutcome::Submitted(url)
            }
        }
        KeyCode::Backspace => {
            ui.sub_input.pop();
            SubInputOutcome::Editing
        }
        KeyCode::Char(c) => {
            ui.sub_input.push(c);
            SubInputOutcome::Editing
        }
        _ => SubInputOutcome::Editing,
    }
}

fn handle_sub_input_submitted(s: &mut AppState, shared: SharedState, url: String) {
    if s.ui.loading.is_none() {
        s.ui.loading = Some(LoadingKind::AddSub);
        let cfg = s.config.clone();
        spawn_add_subscription(shared, cfg, url);
    }
}

async fn handle_mode_selector_enter(s: &mut AppState, shared: SharedState) {
    let idx = s.ui.mode_selector_idx;
    let target = match idx {
        0 => ProxyMode::Rule,
        1 => ProxyMode::Global,
        2 => ProxyMode::Direct,
        _ => ProxyMode::Rule,
    };
    s.ui.show_mode_selector = false;
    if s.ui.loading.is_some() {
        return;
    }
    s.ui.loading = Some(LoadingKind::SwitchMode);
    let c = s.client.clone();
    let s2 = shared.clone();
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
}

fn handle_confirm_key(ui: &mut UiState, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            ui.confirm_remove = None;
            true
        }
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('N') => {
            ui.confirm_remove = None;
            false
        }
        _ => false,
    }
}

/// Close any open popup/modal on a mouse click and swallow the event.
/// Returns true when a popup was open (and thus dismissed).
fn dismiss_popups_on_mouse(ui: &mut UiState) -> bool {
    if !(ui.show_help
        || ui.show_settings
        || ui.show_mode_selector
        || ui.sub_input_mode
        || ui.confirm_remove.is_some())
    {
        return false;
    }
    ui.show_help = false;
    ui.show_settings = false;
    ui.show_mode_selector = false;
    ui.sub_input_mode = false;
    ui.sub_input.clear();
    ui.confirm_remove = None;
    true
}

fn spawn_add_subscription(shared: SharedState, mut cfg: MioctlConfig, url: String) {
    tokio::spawn(async move {
        let result = SubscriptionManager::add(&mut cfg, &url, None, false, false).await;
        {
            let mut st = shared.lock().await;
            match result {
                Ok(msg) => {
                    st.config = cfg;
                    st.add_log("info", &msg);
                }
                Err(e) => st.add_log("error", &format!("Add failed: {}", e)),
            }
        }
        refresh_state(&shared).await;
        let mut st = shared.lock().await;
        st.ui.loading = None;
    });
}

fn spawn_remove_subscription(shared: SharedState, mut cfg: MioctlConfig, name: String) {
    tokio::spawn(async move {
        let result = SubscriptionManager::remove(&mut cfg, &name).await;
        {
            let mut st = shared.lock().await;
            st.config = cfg;
            match result {
                Ok(msg) => st.add_log("info", &msg),
                Err(e) => st.add_log("error", &format!("Remove failed: {}", e)),
            }
        }
        refresh_state(&shared).await;
        let mut st = shared.lock().await;
        let last_idx = st.config.subscriptions.items.len().saturating_sub(1);
        st.ui.selected_sub_idx = st.ui.selected_sub_idx.min(last_idx);
        st.ui.loading = None;
    });
}

fn spawn_switch_profile(shared: SharedState, mut cfg: MioctlConfig, name: String) {
    tokio::spawn(async move {
        let result = SubscriptionManager::use_profile(&mut cfg, &name, false).await;
        {
            let mut st = shared.lock().await;
            st.config = cfg;
            match result {
                Ok(msg) => st.add_log("info", &msg),
                Err(e) => st.add_log("error", &format!("Switch failed: {}", e)),
            }
        }
        refresh_state(&shared).await;
        let mut st = shared.lock().await;
        st.ui.loading = None;
    });
}

fn spawn_update_subscription(shared: SharedState, mut cfg: MioctlConfig, name: String) {
    tokio::spawn(async move {
        let result = SubscriptionManager::update(&mut cfg, &UpdateTarget::Named(name)).await;
        {
            let mut st = shared.lock().await;
            st.config = cfg;
            match result {
                Ok(report) => st.add_log("info", &report.lines.join("\n")),
                Err(e) => st.add_log("error", &format!("Update failed: {}", e)),
            }
        }
        refresh_state(&shared).await;
        let mut st = shared.lock().await;
        st.ui.loading = None;
    });
}

async fn migrate_startup_archives(shared: &SharedState) {
    let mut cfg = {
        let st = shared.lock().await;
        st.config.clone()
    };
    let warnings = SubscriptionManager::ensure_archived(&mut cfg).await;
    let mut st = shared.lock().await;
    st.config = cfg;
    for w in warnings {
        st.add_log("info", &w);
    }
}

async fn handle_action(action: &Action, s: &mut AppState, shared: SharedState) -> bool {
    let client = s.client.clone();
    match action {
        Action::Quit => return false,
        Action::Refresh => {
            if s.ui.loading.is_none() {
                s.ui.loading = Some(LoadingKind::Refresh);
            }
            let c = s.client.clone();
            let shared2 = shared.clone();
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
        }
        Action::SwitchView(i) => {
            let v = match i {
                0 => Dashboard,
                1 => Proxies,
                2 => Connections,
                3 => Rules,
                4 => Logs,
                5 => Subscriptions,
                _ => return true,
            };
            s.ui.active_view = v;
        }
        Action::MoveDown => match s.ui.active_view {
            Proxies => {
                let i = s.ui.selected_group_idx;
                let m = s
                    .groups
                    .get(i)
                    .map(|g| g.all.len().saturating_sub(1))
                    .unwrap_or(0);
                s.ui.selected_node_idx = (s.ui.selected_node_idx + 1).min(m);
            }
            Connections => {
                let m = s.connections.len().saturating_sub(1);
                s.ui.selected_conn_idx = (s.ui.selected_conn_idx + 1).min(m);
            }
            Logs => {
                let m = s.logs.len().saturating_sub(1);
                if s.ui.log_visual {
                    s.ui.log_select_end = (s.ui.log_select_end + 1).min(m);
                    s.ui.log_cursor = s.ui.log_select_end;
                } else {
                    s.ui.log_cursor = (s.ui.log_cursor + 1).min(m);
                }
            }
            Subscriptions => {
                s.ui.selected_sub_idx = (s.ui.selected_sub_idx + 1)
                    .min(s.config.subscriptions.items.len().saturating_sub(1));
            }
            _ => {}
        },
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
            Subscriptions => {
                s.ui.selected_sub_idx = s.ui.selected_sub_idx.saturating_sub(1);
            }
            _ => {}
        },
        Action::JumpTop => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = 0,
            Connections => s.ui.selected_conn_idx = 0,
            Logs => {
                s.ui.log_cursor = 0;
            }
            _ => {}
        },
        Action::JumpBottom => match s.ui.active_view {
            Proxies => {
                let i = s.ui.selected_group_idx;
                let m = s
                    .groups
                    .get(i)
                    .map(|g| g.all.len().saturating_sub(1))
                    .unwrap_or(0);
                s.ui.selected_node_idx = m;
            }
            Connections => s.ui.selected_conn_idx = s.connections.len().saturating_sub(1),
            Logs => {
                s.ui.log_cursor = s.logs.len().saturating_sub(1);
            }
            _ => {}
        },
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
        Action::ToggleProxy => {
            if let Some(c) = client {
                s.ui.loading = Some(LoadingKind::ToggleProxy);
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    toggle_tun_flow(&shared2, c).await;
                });
            }
        }
        Action::SwitchNode => {
            if s.ui.active_view == Subscriptions {
                let name = s
                    .config
                    .subscriptions
                    .items
                    .get(s.ui.selected_sub_idx)
                    .map(|i| i.name.clone());
                if let Some(name) = name {
                    if s.ui.loading.is_none() {
                        s.ui.loading = Some(LoadingKind::SwitchProfile);
                        let cfg = s.config.clone();
                        spawn_switch_profile(shared.clone(), cfg, name);
                    }
                }
                return true;
            }
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let group_name = s.groups.get(i).map(|g| g.name.clone());
            let node_name = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
                if s.ui.loading.is_none() {
                    s.ui.loading = Some(LoadingKind::SwitchNode);
                }
                let shared2 = shared.clone();
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
            }
        }
        Action::TestNodeDelay => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let node = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(n)) = (client, node) {
                if s.ui.loading.is_none() {
                    s.ui.loading = Some(LoadingKind::TestNodeDelay);
                }
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    match ProxyManager::test_node_delay(&c, &n, &url, timeout).await {
                        Ok(result) => {
                            {
                                let mut s = shared2.lock().await;
                                apply_node_delay(&mut s, &n, result.delay);
                            }
                            refresh_state(&shared2).await;
                            let mut s = shared2.lock().await;
                            s.add_log("info", &format!("Delay: {} tested", n));
                            s.ui.loading = None;
                        }
                        Err(e) => {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Delay test failed: {}", e));
                            s.ui.loading = None;
                        }
                    }
                });
            }
        }
        Action::TestGroupDelay => {
            let i = s.ui.selected_group_idx;
            let group = s.groups.get(i).map(|g| g.name.clone());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(g)) = (client, group) {
                if s.ui.loading.is_none() {
                    s.ui.loading = Some(LoadingKind::TestGroupDelay);
                }
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    match ProxyManager::test_group_delay(&c, &g, &url, timeout).await {
                        Ok(results) => {
                            {
                                let mut s = shared2.lock().await;
                                apply_group_delays(&mut s, &results);
                            }
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
            }
        }
        Action::PrevGroup => {
            s.ui.selected_group_idx = s.ui.selected_group_idx.saturating_sub(1);
            s.ui.selected_node_idx = 0;
        }
        Action::NextGroup => {
            s.ui.selected_group_idx =
                (s.ui.selected_group_idx + 1).min(s.groups.len().saturating_sub(1));
            s.ui.selected_node_idx = 0;
        }
        Action::CloseConnection => {
            if s.ui.active_view == Subscriptions {
                if let Some(item) = s.config.subscriptions.items.get(s.ui.selected_sub_idx) {
                    s.ui.confirm_remove = Some(item.name.clone());
                }
                return true;
            }
            let idx = s.ui.selected_conn_idx;
            let id = s.connections.get(idx).map(|c| c.id.clone());
            if let (Some(c), Some(id)) = (client, id) {
                let shared2 = shared.clone();
                let id2 = id.clone();
                tokio::spawn(async move {
                    match ConnectionManager::close_one(&c, &id).await {
                        Ok(()) => {
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
        }
        Action::CloseAllConnections => {
            if let Some(c) = client {
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    match ConnectionManager::close_all(&c).await {
                        Ok(()) => {
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
        Action::TogglePause => s.ui.log_paused = !s.ui.log_paused,
        Action::ToggleHelp => s.ui.show_help = !s.ui.show_help,
        Action::ShowSettings => {
            s.ui.show_settings = !s.ui.show_settings;
            if s.ui.show_settings {
                s.ui.show_help = false;
                s.ui.show_mode_selector = false;
            }
        }
        Action::SubUpdate => {
            if s.ui.active_view == Subscriptions && s.ui.loading.is_none() {
                let name = s
                    .config
                    .subscriptions
                    .items
                    .get(s.ui.selected_sub_idx)
                    .map(|i| i.name.clone());
                if let Some(name) = name {
                    s.ui.loading = Some(LoadingKind::UpdateSubs);
                    let cfg = s.config.clone();
                    spawn_update_subscription(shared.clone(), cfg, name);
                }
            }
        }
        Action::SubAdd => {
            if s.ui.active_view == Subscriptions && !s.ui.sub_input_mode {
                s.ui.sub_input_mode = true;
                s.ui.sub_input.clear();
            }
        }
        Action::Back => {
            if s.ui.search_mode {
                s.ui.search_mode = false;
                s.ui.search_query.clear();
            } else if s.ui.show_mode_selector {
                s.ui.show_mode_selector = false;
            } else if s.ui.show_help {
                s.ui.show_help = false;
            } else if s.ui.show_settings {
                s.ui.show_settings = false;
            }
        }
        Action::Search => {
            s.ui.search_mode = true;
            s.ui.search_query.clear();
        }
        Action::SearchNext => {
            if !s.ui.search_query.is_empty() {
                if let Some(group) = s.groups.get(s.ui.selected_group_idx) {
                    let query = s.ui.search_query.to_lowercase();
                    let start = s.ui.selected_node_idx + 1;
                    for i in start..group.all.len() {
                        if group.all[i].to_lowercase().contains(&query) {
                            s.ui.selected_node_idx = i;
                            break;
                        }
                    }
                }
            }
        }
        Action::SearchPrev => {
            if !s.ui.search_query.is_empty() {
                if let Some(group) = s.groups.get(s.ui.selected_group_idx) {
                    let query = s.ui.search_query.to_lowercase();
                    if s.ui.selected_node_idx > 0 {
                        for i in (0..s.ui.selected_node_idx).rev() {
                            if group.all[i].to_lowercase().contains(&query) {
                                s.ui.selected_node_idx = i;
                                break;
                            }
                        }
                    }
                }
            }
        }
        Action::CommandMode => {
            s.ui.search_mode = true;
            s.ui.search_query.clear();
        }
        Action::LogVisual => {
            if s.ui.active_view == Logs {
                s.ui.log_visual = true;
                s.ui.log_select_start = s.ui.log_cursor;
                s.ui.log_select_end = s.ui.log_cursor;
            }
        }
        Action::LogCopy => {
            if s.ui.active_view == Logs {
                let text = if s.ui.log_visual {
                    let t = collect_log_selection(s);
                    s.ui.log_visual = false;
                    t
                } else if let Some(entry) = s.logs.get(s.ui.log_cursor) {
                    entry.payload.clone()
                } else {
                    return true;
                };
                let shared2 = shared.clone();
                tokio::task::spawn_blocking(move || {
                    let copied = copy_to_clipboard(&text);
                    let mut st = shared2.blocking_lock();
                    if !copied {
                        st.add_log(
                            "error",
                            "Clipboard unavailable — install wl-clipboard (Wayland) or xclip (X11)",
                        );
                    } else {
                        st.add_log("info", &format!("Copied: {} chars", text.len()));
                    }
                });
            }
        }
        Action::CycleLogLevel => {
            s.ui.log_level_filter = match s.ui.log_level_filter.as_deref() {
                None => Some("info".into()),
                Some("info") => Some("warning".into()),
                Some("warning") => Some("error".into()),
                Some("error") => Some("debug".into()),
                Some("debug") => None,
                _ => None,
            };
        }
    }
    true
}

/// Maximum history entries kept per proxy after a delay-test write-back.
const MAX_DELAY_HISTORY: usize = 10;

/// Push a delay measurement into a proxy's history (most recent last),
/// trimming to [`MAX_DELAY_HISTORY`] entries. Missing proxies are ignored.
fn record_delay(proxy: Option<&mut Proxy>, delay: i64) {
    if let Some(p) = proxy {
        p.history.push(ProxyHistory {
            time: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, false),
            delay,
        });
        while p.history.len() > MAX_DELAY_HISTORY {
            p.history.remove(0);
        }
    }
}

/// Write a single-node delay test result into `state.proxies`.
fn apply_node_delay(state: &mut AppState, node: &str, delay: i64) {
    record_delay(state.proxies.proxies.get_mut(node), delay);
}

/// Write a group delay test result (node -> delay) into `state.proxies`.
fn apply_group_delays(state: &mut AppState, results: &HashMap<String, i64>) {
    for (node, delay) in results {
        record_delay(state.proxies.proxies.get_mut(node), *delay);
    }
}

/// Toggle TUN mode at runtime via `PATCH /configs` (immediate, no restart),
/// then re-apply the system proxy accordingly.
///
/// The config file is also updated as best-effort persistence — it only takes
/// effect on the next mihomo start when `config_path` is the file mihomo
/// actually loads (e.g. `mihomo -f <path>`).
pub async fn toggle_tun_flow(shared: &SharedState, client: MihomoClient) {
    let (tun_enabled, config_path, mixed_port) = {
        let s = shared.lock().await;
        (
            s.tun.as_ref().map(|t| t.enable).unwrap_or(false),
            s.config.mihomo.config_path.clone(),
            s.mixed_port,
        )
    };

    if tun_enabled {
        // TUN ON → disable TUN, enable system proxy
        if let Err(e) = client.patch_configs(tun_patch_payload(None, false)).await {
            let mut s = shared.lock().await;
            s.add_log("error", &format!("Failed to disable TUN: {}", e));
            s.ui.loading = None;
            return;
        }
        refresh_state(shared).await;
        let actual = {
            shared
                .lock()
                .await
                .tun
                .as_ref()
                .map(|t| t.enable)
                .unwrap_or(true)
        };
        if actual {
            let mut s = shared.lock().await;
            s.add_log(
                "error",
                "TUN toggle: config updated but TUN is still running — mihomo tun fd leak; attempting automatic recovery via mihomo restart",
            );
            drop(s);
            if let Err(e) = set_tun_config(&config_path, false) {
                let mut s = shared.lock().await;
                s.add_log("debug", &format!("TUN config persist skipped: {}", e));
            }
            match restart_mihomo_and_wait(&client, Duration::from_secs(10)).await {
                Ok(()) => {
                    refresh_state(shared).await;
                    let actual = {
                        shared
                            .lock()
                            .await
                            .tun
                            .as_ref()
                            .map(|t| t.enable)
                            .unwrap_or(true)
                    };
                    let mut s = shared.lock().await;
                    if !actual {
                        s.add_log(
                            "info",
                            "TUN disabled via mihomo restart (recovered from tun fd leak)",
                        );
                    } else {
                        s.add_log(
                            "error",
                            "TUN still running after mihomo restart — check `journalctl --user -u mihomo -n 20`",
                        );
                    }
                }
                Err(e) => {
                    let mut s = shared.lock().await;
                    s.add_log(
                        "error",
                        &format!(
                            "TUN recovery failed: {} — run `systemctl --user restart mihomo` manually",
                            e
                        ),
                    );
                }
            }
            let mut s = shared.lock().await;
            s.ui.loading = None;
            return;
        }
        if let Err(e) = set_tun_config(&config_path, false) {
            let mut s = shared.lock().await;
            s.add_log("debug", &format!("TUN config persist skipped: {}", e));
        }
        let mut s = shared.lock().await;
        if let Some(port) = mixed_port {
            if let Err(e) = crate::os::proxy::set_system_proxy(port) {
                s.add_log("error", &format!("System proxy failed: {}", e));
            } else {
                s.system_proxy_enabled = true;
            }
        } else {
            s.add_log("error", "System proxy failed: mixed-port unknown");
        }
        if let Some(port) = mixed_port {
            if let Some(old) = s.proxy_guard.take() {
                old.abort();
            }
            s.proxy_guard = Some(start_proxy_guard(shared.clone(), port));
        }
        s.add_log(
            "info",
            "TUN disabled (runtime), system proxy enabled — mihomo restart restores config-file state",
        );
        s.ui.loading = None;
    } else {
        // TUN OFF → enable TUN, disable system proxy
        crate::os::proxy::clear_system_proxy();
        {
            let mut s = shared.lock().await;
            if let Some(g) = s.proxy_guard.take() {
                g.abort();
            }
        }
        let current = { shared.lock().await.tun.clone() };
        if let Err(e) = client
            .patch_configs(tun_patch_payload(current.as_ref(), true))
            .await
        {
            let mut s = shared.lock().await;
            s.add_log("error", &format!("Failed to enable TUN: {}", e));
            s.ui.loading = None;
            return;
        }
        refresh_state(shared).await;
        let actual = {
            shared
                .lock()
                .await
                .tun
                .as_ref()
                .map(|t| t.enable)
                .unwrap_or(false)
        };
        if actual {
            if let Err(e) = set_tun_config(&config_path, true) {
                let mut s = shared.lock().await;
                s.add_log("debug", &format!("TUN config persist skipped: {}", e));
            }
            let mut s = shared.lock().await;
            s.add_log(
                "info",
                "TUN enabled (runtime) — mihomo restart restores config-file state",
            );
        } else {
            let mut s = shared.lock().await;
            s.add_log(
                "error",
                "TUN toggle: config updated but TUN did not start — mihomo tun fd leak (device or resource busy); attempting automatic recovery via mihomo restart",
            );
            drop(s);
            if let Err(e) = set_tun_config(&config_path, true) {
                let mut s = shared.lock().await;
                s.add_log("debug", &format!("TUN config persist skipped: {}", e));
            }
            match restart_mihomo_and_wait(&client, Duration::from_secs(10)).await {
                Ok(()) => {
                    refresh_state(shared).await;
                    let actual = {
                        shared
                            .lock()
                            .await
                            .tun
                            .as_ref()
                            .map(|t| t.enable)
                            .unwrap_or(false)
                    };
                    let mut s = shared.lock().await;
                    if actual {
                        s.add_log(
                            "info",
                            "TUN enabled via mihomo restart (recovered from tun fd leak)",
                        );
                    } else {
                        s.add_log(
                            "error",
                            "TUN still not enabled after mihomo restart — check `journalctl --user -u mihomo -n 20`",
                        );
                    }
                }
                Err(e) => {
                    let mut s = shared.lock().await;
                    s.add_log(
                        "error",
                        &format!(
                            "TUN recovery failed: {} — run `systemctl --user restart mihomo` manually",
                            e
                        ),
                    );
                }
            }
        }
        let mut s = shared.lock().await;
        s.ui.loading = None;
    }
}

/// Restart the mihomo user service and wait for its API to come back.
/// Returns `Ok(())` when the API responds within `timeout`.
async fn restart_mihomo_and_wait(client: &MihomoClient, timeout: Duration) -> Result<(), String> {
    if std::env::var_os("MIOCTL_TEST_NO_SYSTEMCTL").is_some() {
        return Err("systemctl disabled in test env".into());
    }
    let status = tokio::task::spawn_blocking(|| {
        std::process::Command::new("systemctl")
            .args(["--user", "restart", "mihomo"])
            .output()
    })
    .await;
    match status {
        Ok(Ok(o)) if o.status.success() => {}
        _ => return Err("systemctl --user restart mihomo failed".into()),
    }
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if client.get_version().await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("mihomo API did not recover after restart".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Fetch all data from mihomo API and update shared state.
/// Clamps UI selections to valid ranges after updating groups.
async fn refresh_state(shared: &SharedState) {
    let client = { shared.lock().await.client.clone() };
    let Some(ref client) = client else { return };

    let t = Duration::from_secs(3);
    let (proxies, conns, rules, traffic, configs, memory) = tokio::join!(
        tokio::time::timeout(t, ProxyManager::refresh_all(client)),
        tokio::time::timeout(t, ConnectionManager::list(client)),
        tokio::time::timeout(t, client.get_rules()),
        tokio::time::timeout(t, client.get_traffic()),
        tokio::time::timeout(t, client.get_configs()),
        tokio::time::timeout(t, client.get_memory()),
    );
    let proxies = proxies.unwrap_or(Err(crate::api::error::ApiError::Timeout));
    let conns = conns.unwrap_or(Err(crate::api::error::ApiError::Timeout));
    let rules = rules.unwrap_or(Err(crate::api::error::ApiError::Timeout));
    let traffic = traffic.unwrap_or(Err(crate::api::error::ApiError::Timeout));
    let configs = configs.unwrap_or(Err(crate::api::error::ApiError::Timeout));
    let memory = memory.unwrap_or(Err(crate::api::error::ApiError::Timeout));

    let mut s = shared.lock().await;
    if let Ok((p, g)) = proxies {
        s.proxies = p;
        s.groups = g;
        s.ui.selected_group_idx =
            s.ui.selected_group_idx
                .min(s.groups.len().saturating_sub(1));
        if let Some(grp) = s.groups.get(s.ui.selected_group_idx) {
            s.ui.selected_node_idx = s.ui.selected_node_idx.min(grp.all.len().saturating_sub(1));
        }
    }
    if let Ok(c) = conns {
        s.connections = c;
    }
    if let Ok(r) = rules {
        s.rules = r;
    }
    if let Ok(t) = traffic {
        s.traffic = t;
    }
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
    if let Ok(m) = memory {
        s.memory = m;
    }
    s.update_time();
}

fn is_quit_interrupt(key: &crossterm::event::KeyEvent) -> bool {
    key.code == crossterm::event::KeyCode::Char('c')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_is_quit_interrupt() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        assert!(is_quit_interrupt(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_quit_interrupt(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
        assert!(!is_quit_interrupt(&KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn test_set_tun_enable() {
        let yaml = "tun:\n  enable: false\n  stack: system\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        set_tun_config(&path, true).unwrap();
        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("enable: true"));
    }

    #[test]
    fn test_set_tun_disable() {
        let yaml = "tun:\n  enable: true\n  stack: system\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        set_tun_config(&path, false).unwrap();
        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("enable: false"));
    }

    #[test]
    fn test_set_tun_creates_missing_section() {
        let yaml = "mode: rule\nport: 7890\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        set_tun_config(&path, true).unwrap();
        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("tun:"));
        assert!(result.contains("enable: true"));
        assert!(result.contains("mode: rule"));
        assert!(result.contains("port: 7890"));
    }

    #[test]
    fn test_set_tun_rejects_non_mapping_tun() {
        let yaml = "tun: not-a-mapping\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        let result = set_tun_config(&path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a mapping"));
    }

    #[test]
    fn test_tun_patch_payload_disable_only_enable() {
        let payload = tun_patch_payload(None, false);
        assert_eq!(payload, serde_json::json!({"tun": {"enable": false}}));
    }

    #[test]
    fn test_tun_patch_payload_enable_adds_default_stack() {
        let payload = tun_patch_payload(None, true);
        assert_eq!(
            payload,
            serde_json::json!({"tun": {"enable": true, "stack": "system"}})
        );
    }

    #[test]
    fn test_tun_patch_payload_preserves_current_fields() {
        let current = TunConfig {
            enable: false,
            stack: Some("gVisor".into()),
            device: Some("Meta".into()),
            auto_route: Some(true),
        };
        let payload = tun_patch_payload(Some(&current), true);
        assert_eq!(
            payload,
            serde_json::json!({
                "tun": {
                    "enable": true,
                    "stack": "gVisor",
                    "device": "Meta",
                    "auto-route": true
                }
            })
        );
    }

    #[test]
    fn test_tun_patch_payload_enable_keeps_existing_stack() {
        let current = TunConfig {
            enable: false,
            stack: Some("gVisor".into()),
            device: None,
            auto_route: None,
        };
        let payload = tun_patch_payload(Some(&current), true);
        assert_eq!(
            payload,
            serde_json::json!({"tun": {"enable": true, "stack": "gVisor"}})
        );
    }

    #[test]
    fn test_tun_patch_payload_enable_without_stack_but_with_device() {
        let current = TunConfig {
            enable: false,
            stack: None,
            device: Some("utun".into()),
            auto_route: None,
        };
        let payload = tun_patch_payload(Some(&current), true);
        assert_eq!(
            payload,
            serde_json::json!({"tun": {"enable": true, "stack": "system", "device": "utun"}})
        );
    }

    #[test]
    fn test_set_tun_config_missing_file_errors() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::remove_file(&path).unwrap();
        let result = set_tun_config(&path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("read"));
    }

    #[test]
    fn test_set_tun_config_invalid_yaml_errors() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, "tun: [unclosed").unwrap();
        let result = set_tun_config(&path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("parse YAML"));
    }

    #[test]
    fn test_set_tun_config_non_mapping_root_errors() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, "just a scalar\n").unwrap();
        let result = set_tun_config(&path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a YAML mapping"));
    }

    #[test]
    fn test_set_tun_preserves_other_fields() {
        let yaml = "tun:\n  enable: false\n  stack: gvisor\n  device: utun\n  auto-route: true\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        set_tun_config(&path, true).unwrap();
        let result = std::fs::read_to_string(&path).unwrap();
        assert!(result.contains("enable: true"));
        assert!(result.contains("stack: gvisor"));
        assert!(result.contains("device: utun"));
        assert!(result.contains("auto-route: true"));
    }

    fn make_proxy(name: &str) -> Proxy {
        Proxy {
            name: name.into(),
            proxy_type: "Shadowsocks".into(),
            now: None,
            all: Vec::new(),
            history: Vec::new(),
            udp: true,
            alive: true,
        }
    }

    #[test]
    fn test_apply_node_delay_writes_history() {
        let mut s = AppState::new();
        s.proxies
            .proxies
            .insert("NodeA".into(), make_proxy("NodeA"));

        apply_node_delay(&mut s, "NodeA", 123);

        let p = s.proxies.proxies.get("NodeA").unwrap();
        assert_eq!(p.history.len(), 1);
        assert_eq!(p.history[0].delay, 123);
        assert!(!p.history[0].time.is_empty());
    }

    #[test]
    fn test_apply_node_delay_ignores_missing_proxy() {
        let mut s = AppState::new();
        apply_node_delay(&mut s, "Ghost", 123);
        assert!(!s.proxies.proxies.contains_key("Ghost"));
    }

    #[test]
    fn test_apply_group_delays_writes_all_nodes() {
        let mut s = AppState::new();
        s.proxies
            .proxies
            .insert("NodeA".into(), make_proxy("NodeA"));
        s.proxies
            .proxies
            .insert("NodeB".into(), make_proxy("NodeB"));

        let results = HashMap::from([("NodeA".to_string(), 100), ("NodeB".to_string(), 200)]);
        apply_group_delays(&mut s, &results);

        assert_eq!(
            s.proxies.proxies["NodeA"].history.last().unwrap().delay,
            100
        );
        assert_eq!(
            s.proxies.proxies["NodeB"].history.last().unwrap().delay,
            200
        );
    }

    #[test]
    fn test_apply_group_delays_ignores_unknown_nodes() {
        let mut s = AppState::new();
        s.proxies
            .proxies
            .insert("NodeA".into(), make_proxy("NodeA"));

        let results = HashMap::from([("NodeA".to_string(), 100), ("Ghost".to_string(), 999)]);
        apply_group_delays(&mut s, &results);

        assert_eq!(
            s.proxies.proxies["NodeA"].history.last().unwrap().delay,
            100
        );
        assert!(!s.proxies.proxies.contains_key("Ghost"));
    }

    #[test]
    fn test_record_delay_trims_history() {
        let mut p = make_proxy("NodeA");
        for i in 0..(MAX_DELAY_HISTORY + 5) {
            p.history.push(ProxyHistory {
                time: i.to_string(),
                delay: i as i64,
            });
        }
        record_delay(Some(&mut p), 999);

        assert_eq!(p.history.len(), MAX_DELAY_HISTORY);
        assert_eq!(p.history.last().unwrap().delay, 999);
        assert_eq!(p.history.first().unwrap().delay, 6);
    }

    #[test]
    fn test_record_delay_none_is_noop() {
        record_delay(None, 42);
    }

    #[tokio::test]
    async fn test_switch_view_to_subscriptions() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        assert!(handle_action(&Action::SwitchView(5), &mut s, shared.clone()).await);
        assert_eq!(s.ui.active_view, Subscriptions);
    }

    fn input_ui(buffer: &str) -> UiState {
        UiState {
            sub_input_mode: true,
            sub_input: buffer.into(),
            ..UiState::default()
        }
    }

    fn confirm_ui() -> UiState {
        UiState {
            confirm_remove: Some("sub1".into()),
            ..UiState::default()
        }
    }

    #[test]
    fn test_sub_input_key_esc_cancels_and_clears() {
        let mut ui = input_ui("https://x");
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Esc),
            SubInputOutcome::Canceled
        ));
        assert!(!ui.sub_input_mode);
        assert!(ui.sub_input.is_empty());
    }

    #[test]
    fn test_sub_input_key_enter_submits_url() {
        let mut ui = input_ui("https://x");
        match handle_sub_input_key(&mut ui, KeyCode::Enter) {
            SubInputOutcome::Submitted(url) => assert_eq!(url, "https://x"),
            SubInputOutcome::Canceled | SubInputOutcome::Editing => {
                panic!("expected Submitted")
            }
        }
        assert!(!ui.sub_input_mode);
        assert!(ui.sub_input.is_empty());
    }

    #[test]
    fn test_sub_input_key_enter_empty_cancels_without_submit() {
        let mut ui = input_ui("");
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Enter),
            SubInputOutcome::Canceled
        ));
        assert!(!ui.sub_input_mode);
    }

    #[test]
    fn test_sub_input_key_backspace_pops() {
        let mut ui = UiState {
            sub_input: "ab".into(),
            ..UiState::default()
        };
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Backspace),
            SubInputOutcome::Editing
        ));
        assert_eq!(ui.sub_input, "a");
    }

    #[test]
    fn test_sub_input_key_backspace_empty_is_noop() {
        let mut ui = UiState::default();
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Backspace),
            SubInputOutcome::Editing
        ));
        assert!(ui.sub_input.is_empty());
    }

    #[test]
    fn test_sub_input_key_char_pushes() {
        let mut ui = UiState::default();
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Char('x')),
            SubInputOutcome::Editing
        ));
        assert!(matches!(
            handle_sub_input_key(&mut ui, KeyCode::Char('Y')),
            SubInputOutcome::Editing
        ));
        assert_eq!(ui.sub_input, "xY");
    }

    #[test]
    fn test_sub_input_key_other_codes_ignored() {
        let mut ui = input_ui("keep");
        for code in [KeyCode::Left, KeyCode::Tab, KeyCode::F(1)] {
            assert!(matches!(
                handle_sub_input_key(&mut ui, code),
                SubInputOutcome::Editing
            ));
        }
        assert!(ui.sub_input_mode);
        assert_eq!(ui.sub_input, "keep");
    }

    #[tokio::test]
    async fn test_sub_input_submit_ignored_while_loading() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.sub_input_mode = true;
        s.ui.sub_input = "https://x".into();
        s.ui.loading = Some(LoadingKind::UpdateSubs);
        let url = match handle_sub_input_key(&mut s.ui, KeyCode::Enter) {
            SubInputOutcome::Submitted(url) => url,
            SubInputOutcome::Canceled | SubInputOutcome::Editing => {
                panic!("expected Submitted")
            }
        };
        handle_sub_input_submitted(&mut s, shared.clone(), url);
        assert!(
            !s.ui.sub_input_mode,
            "key must still be consumed by input handler"
        );
        assert_eq!(
            s.ui.loading,
            Some(LoadingKind::UpdateSubs),
            "submit must not spawn while an operation is in flight"
        );
    }

    #[test]
    fn test_confirm_key_y_confirms() {
        let mut ui = confirm_ui();
        assert!(handle_confirm_key(&mut ui, KeyCode::Char('y')));
        assert!(ui.confirm_remove.is_none());
        ui.confirm_remove = Some("sub1".into());
        assert!(handle_confirm_key(&mut ui, KeyCode::Char('Y')));
        assert!(ui.confirm_remove.is_none());
    }

    #[test]
    fn test_confirm_key_dismiss_keys() {
        for code in [
            KeyCode::Esc,
            KeyCode::Enter,
            KeyCode::Char('n'),
            KeyCode::Char('N'),
        ] {
            let mut ui = confirm_ui();
            assert!(!handle_confirm_key(&mut ui, code));
            assert!(ui.confirm_remove.is_none());
        }
    }

    #[test]
    fn test_confirm_key_other_keys_ignored() {
        let mut ui = confirm_ui();
        assert!(!handle_confirm_key(&mut ui, KeyCode::Char('x')));
        assert_eq!(ui.confirm_remove.as_deref(), Some("sub1"));
        assert!(!handle_confirm_key(&mut ui, KeyCode::Backspace));
        assert_eq!(ui.confirm_remove.as_deref(), Some("sub1"));
    }

    #[test]
    fn test_mouse_dismiss_closes_sub_input_mode_and_clears_buffer() {
        let mut ui = input_ui("https://x");
        assert!(dismiss_popups_on_mouse(&mut ui));
        assert!(!ui.sub_input_mode);
        assert!(ui.sub_input.is_empty());
    }

    #[test]
    fn test_mouse_dismiss_closes_confirm_remove() {
        let mut ui = confirm_ui();
        assert!(dismiss_popups_on_mouse(&mut ui));
        assert!(ui.confirm_remove.is_none());
    }

    #[test]
    fn test_mouse_dismiss_closes_help_settings_and_mode_selector() {
        let mut ui = UiState {
            show_help: true,
            show_settings: true,
            show_mode_selector: true,
            ..UiState::default()
        };
        assert!(dismiss_popups_on_mouse(&mut ui));
        assert!(!ui.show_help);
        assert!(!ui.show_settings);
        assert!(!ui.show_mode_selector);
    }

    #[test]
    fn test_mouse_dismiss_false_when_no_popups_open() {
        let mut ui = UiState::default();
        assert!(!dismiss_popups_on_mouse(&mut ui));
        assert!(!ui.sub_input_mode);
        assert!(ui.confirm_remove.is_none());
    }

    fn make_group(name: &str, all: Vec<&str>) -> crate::api::types::Group {
        crate::api::types::Group {
            name: name.into(),
            group_type: "Selector".into(),
            now: None,
            all: all.into_iter().map(String::from).collect(),
        }
    }

    fn isolate_config(s: &mut AppState) {
        s.config.subscriptions = crate::config::mioctl_config::Subscriptions::default();
    }

    #[tokio::test]
    async fn test_move_down_subscriptions_clamps_to_last() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        s.config.add_subscription("a".into(), "https://a".into());
        s.config.add_subscription("b".into(), "https://b".into());
        handle_action(&Action::MoveDown, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_sub_idx, 1);
        handle_action(&Action::MoveDown, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_sub_idx, 1);
    }

    #[tokio::test]
    async fn test_move_down_subscriptions_empty_list_stays_zero() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        handle_action(&Action::MoveDown, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_sub_idx, 0);
    }

    #[tokio::test]
    async fn test_move_up_subscriptions_saturates_at_zero() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        s.ui.selected_sub_idx = 1;
        handle_action(&Action::MoveUp, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_sub_idx, 0);
        handle_action(&Action::MoveUp, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_sub_idx, 0);
    }

    #[tokio::test]
    async fn test_move_down_proxies_clamp_unchanged() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Proxies;
        s.groups = vec![make_group("G", vec!["a", "b"])];
        s.ui.selected_node_idx = 1;
        handle_action(&Action::MoveDown, &mut s, shared.clone()).await;
        assert_eq!(s.ui.selected_node_idx, 1);
    }

    #[tokio::test]
    async fn test_sub_add_enters_input_mode_and_clears_buffer() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        s.ui.sub_input = "junk".into();
        handle_action(&Action::SubAdd, &mut s, shared.clone()).await;
        assert!(s.ui.sub_input_mode);
        assert!(s.ui.sub_input.is_empty());
    }

    #[tokio::test]
    async fn test_sub_add_other_view_is_noop() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Dashboard;
        handle_action(&Action::SubAdd, &mut s, shared.clone()).await;
        assert!(!s.ui.sub_input_mode);
    }

    #[tokio::test]
    async fn test_sub_add_while_input_mode_active_preserves_buffer() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        s.ui.sub_input_mode = true;
        s.ui.sub_input = "typed".into();
        handle_action(&Action::SubAdd, &mut s, shared.clone()).await;
        assert_eq!(s.ui.sub_input, "typed");
    }

    #[tokio::test]
    async fn test_sub_update_no_selection_sets_no_loading() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        s.ui.loading = None;
        isolate_config(&mut s);
        handle_action(&Action::SubUpdate, &mut s, shared.clone()).await;
        assert!(s.ui.loading.is_none());
    }

    #[tokio::test]
    async fn test_sub_update_other_view_is_noop() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Dashboard;
        s.ui.loading = None;
        isolate_config(&mut s);
        s.config.add_subscription("a".into(), "https://a".into());
        handle_action(&Action::SubUpdate, &mut s, shared.clone()).await;
        assert!(s.ui.loading.is_none());
    }

    #[tokio::test]
    async fn test_switch_node_subscriptions_no_selection_no_spawn() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        s.ui.loading = None;
        isolate_config(&mut s);
        assert!(handle_action(&Action::SwitchNode, &mut s, shared.clone()).await);
        assert!(s.ui.loading.is_none());
    }

    #[tokio::test]
    async fn test_subscription_switch_ignored_while_loading() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        s.config.add_subscription("sub1".into(), "https://x".into());
        s.ui.loading = Some(LoadingKind::Refresh);
        handle_action(&Action::SwitchNode, &mut s, shared.clone()).await;
        assert_eq!(
            s.ui.loading,
            Some(LoadingKind::Refresh),
            "loading must not be replaced while an operation is in flight"
        );
    }

    #[tokio::test]
    async fn test_sub_update_ignored_while_loading() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        s.config.add_subscription("sub1".into(), "https://x".into());
        s.ui.loading = Some(LoadingKind::Refresh);
        handle_action(&Action::SubUpdate, &mut s, shared.clone()).await;
        assert_eq!(
            s.ui.loading,
            Some(LoadingKind::Refresh),
            "loading must not be replaced while an operation is in flight"
        );
    }

    #[tokio::test]
    async fn test_mode_selector_enter_ignored_while_loading() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.show_mode_selector = true;
        s.ui.loading = Some(LoadingKind::UpdateSubs);
        handle_mode_selector_enter(&mut s, shared.clone()).await;
        assert!(!s.ui.show_mode_selector, "selector must still close");
        assert_eq!(
            s.ui.loading,
            Some(LoadingKind::UpdateSubs),
            "loading must not be replaced while an operation is in flight"
        );
    }

    #[tokio::test]
    async fn test_proxy_view_switch_node_keeps_subscription_loading() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Proxies;
        s.client = Some(crate::api::client::MihomoClient::new("127.0.0.1:1", None).unwrap());
        s.groups = vec![make_group("G", vec!["a"])];
        s.ui.loading = Some(LoadingKind::UpdateSubs);
        handle_action(&Action::SwitchNode, &mut s, shared.clone()).await;
        assert_eq!(
            s.ui.loading,
            Some(LoadingKind::UpdateSubs),
            "proxy op must not replace an in-flight subscription loading indicator"
        );
    }

    #[tokio::test]
    async fn test_close_connection_subscriptions_sets_confirm() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        s.config.add_subscription("sub1".into(), "https://x".into());
        assert!(handle_action(&Action::CloseConnection, &mut s, shared.clone()).await);
        assert_eq!(s.ui.confirm_remove.as_deref(), Some("sub1"));
    }

    #[tokio::test]
    async fn test_close_connection_subscriptions_empty_no_confirm() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Subscriptions;
        isolate_config(&mut s);
        assert!(handle_action(&Action::CloseConnection, &mut s, shared.clone()).await);
        assert!(s.ui.confirm_remove.is_none());
    }

    #[tokio::test]
    async fn test_close_connection_connections_view_unchanged() {
        let shared = crate::app::state::new_shared_state();
        let mut s = shared.lock().await;
        s.ui.active_view = Connections;
        handle_action(&Action::CloseConnection, &mut s, shared.clone()).await;
        assert!(s.ui.confirm_remove.is_none());
    }

    struct TestEnv {
        _dir: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
        mihomo_path: std::path::PathBuf,
    }

    impl TestEnv {
        fn new() -> Self {
            Self::new_with("mixed-port: 7897\n")
        }

        fn new_with(mihomo_yaml: &str) -> Self {
            let guard = crate::testutil::env_lock().lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
            unsafe { std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1") };
            let mihomo_path = dir.path().join("mihomo-config.yaml");
            std::fs::write(&mihomo_path, mihomo_yaml).unwrap();
            TestEnv {
                _dir: dir,
                _guard: guard,
                mihomo_path,
            }
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("MIOCTL_HOME");
                std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL");
            }
        }
    }

    async fn wait_loading_cleared(shared: &SharedState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while shared.lock().await.ui.loading.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for background task"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    async fn logs_contain(shared: &SharedState, needle: &str) -> bool {
        shared
            .lock()
            .await
            .logs
            .iter()
            .any(|l| l.payload.contains(needle))
    }

    #[tokio::test]
    async fn test_switch_profile_spawn_missing_archive_logs_error() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.ui.active_view = Subscriptions;
            s.config
                .add_subscription("ghost".into(), "https://x".into());
        }
        let cfg = shared.lock().await.config.clone();
        spawn_switch_profile(shared.clone(), cfg, "ghost".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Switch failed:").await);
        assert!(logs_contain(&shared, "profile archive for 'ghost' is missing").await);
    }

    #[tokio::test]
    async fn test_update_subscription_spawn_fetch_failure_logs_result() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.ui.active_view = Subscriptions;
            s.config.add_subscription("sub1".into(), "not-a-url".into());
        }
        let cfg = shared.lock().await.config.clone();
        spawn_update_subscription(shared.clone(), cfg, "sub1".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "sub1: ERROR -").await);
    }

    #[tokio::test]
    async fn test_update_subscription_spawn_unknown_name_logs_error() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let cfg = shared.lock().await.config.clone();
        spawn_update_subscription(shared.clone(), cfg, "missing".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Update failed: no subscription named 'missing'").await);
    }

    #[tokio::test]
    async fn test_remove_subscription_spawn_inactive_succeeds() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.config.add_subscription("sub1".into(), "https://x".into());
        }
        let cfg = shared.lock().await.config.clone();
        spawn_remove_subscription(shared.clone(), cfg, "sub1".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Subscription 'sub1' removed.").await);
        let s = shared.lock().await;
        assert!(s.config.subscriptions.items.is_empty());
        assert_eq!(s.config.subscriptions.active, None);
    }

    #[tokio::test]
    async fn test_remove_subscription_spawn_unknown_name_logs_error() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let cfg = shared.lock().await.config.clone();
        spawn_remove_subscription(shared.clone(), cfg, "missing".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Remove failed: no subscription named 'missing'").await);
    }

    #[tokio::test]
    async fn test_remove_subscription_spawn_clamps_selected_index() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.config.add_subscription("sub1".into(), "https://x".into());
            s.config.add_subscription("sub2".into(), "https://y".into());
            s.ui.selected_sub_idx = 1;
        }
        let cfg = shared.lock().await.config.clone();
        spawn_remove_subscription(shared.clone(), cfg, "sub2".into());
        wait_loading_cleared(&shared).await;
        {
            let s = shared.lock().await;
            assert_eq!(s.config.subscriptions.items.len(), 1);
            assert_eq!(s.ui.selected_sub_idx, 0);
        }

        {
            let mut s = shared.lock().await;
            s.ui.loading = Some(LoadingKind::SwitchProfile);
        }
        let cfg = shared.lock().await.config.clone();
        spawn_remove_subscription(shared.clone(), cfg, "sub1".into());
        wait_loading_cleared(&shared).await;
        let s = shared.lock().await;
        assert!(s.config.subscriptions.items.is_empty());
        assert_eq!(s.ui.selected_sub_idx, 0);
    }

    #[tokio::test]
    async fn test_add_subscription_spawn_invalid_url_logs_error() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let cfg = shared.lock().await.config.clone();
        spawn_add_subscription(shared.clone(), cfg, "not-a-url".into());
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Add failed:").await);
    }

    #[tokio::test]
    async fn test_add_subscription_spawn_activate_failure_keeps_shared_config() {
        let env = TestEnv::new_with("invalid: [yaml\n");
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/sub"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G\n",
            ))
            .mount(&mock)
            .await;
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.config.mihomo.config_path = env.mihomo_path.to_string_lossy().into_owned();
        }
        let cfg = shared.lock().await.config.clone();
        spawn_add_subscription(shared.clone(), cfg, format!("{}/sub", mock.uri()));
        wait_loading_cleared(&shared).await;
        assert!(logs_contain(&shared, "Add failed:").await);
        let s = shared.lock().await;
        assert!(s.config.subscriptions.items.is_empty());
        assert_eq!(s.config.subscriptions.active, None);
    }

    #[tokio::test]
    async fn test_migrate_startup_archives_logs_warnings_for_missing_profiles() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.config.add_subscription("sub1".into(), "not-a-url".into());
        }
        migrate_startup_archives(&shared).await;
        assert!(logs_contain(&shared, "profile 'sub1' has no archive and fetch failed").await);
    }

    #[tokio::test]
    async fn test_migrate_startup_archives_no_warnings_when_archived() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        {
            let mut s = shared.lock().await;
            s.config.add_subscription("sub1".into(), "https://x".into());
        }
        crate::subscription::profile::write_archive(
            "sub1",
            "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\n",
        )
        .unwrap();
        migrate_startup_archives(&shared).await;
        assert!(shared.lock().await.logs.is_empty());
    }

    #[tokio::test]
    async fn test_migrate_startup_archives_removes_legacy_providers_dir() {
        let _env = TestEnv::new();
        let shared = crate::app::state::new_shared_state();
        let legacy = MioctlConfig::config_dir().join("providers");
        std::fs::create_dir_all(&legacy).unwrap();
        migrate_startup_archives(&shared).await;
        assert!(!legacy.exists());
        assert!(logs_contain(&shared, "removed legacy providers/ directory").await);
    }

    #[test]
    fn test_render_frame_subscriptions_content() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::new();
        state.ui.active_view = Subscriptions;
        state.ui.loading = None;
        state.config.subscriptions = crate::config::mioctl_config::Subscriptions {
            active: Some("main".into()),
            items: vec![crate::config::mioctl_config::SubscriptionItem {
                name: "main".into(),
                url: "https://example.com/sub".into(),
                last_updated: Some("2026-01-01T00:00:00Z".into()),
                node_count: Some(7),
            }],
        };
        let spark = TrafficSpark::new();
        let mut proxy_table = ratatui::widgets::TableState::default();
        let mut conn_table = ratatui::widgets::TableState::default();
        terminal
            .draw(|f| render_frame(f, &state, &spark, &mut proxy_table, &mut conn_table))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            let mut x = 0u16;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                text.push_str(cell.symbol());
                let c = cell.symbol().chars().next().unwrap_or(' ');
                x += if (c as u32) >= 0x2E80 { 2 } else { 1 };
            }
            text.push('\n');
        }
        assert!(text.contains("Subs       "));
        assert!(!text.contains("Update Subs"));
        assert!(text.contains("* main  7 nodes  2026-01-01T00:00:00Z"));
        assert!(text.contains("Enter 激活 · u 更新 · a 添加 · d 删除"));
    }

    #[test]
    fn test_render_frame_subscriptions_empty() {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut state = AppState::new();
        state.ui.active_view = Subscriptions;
        state.ui.loading = None;
        state.config.subscriptions = crate::config::mioctl_config::Subscriptions::default();
        let spark = TrafficSpark::new();
        let mut proxy_table = ratatui::widgets::TableState::default();
        let mut conn_table = ratatui::widgets::TableState::default();
        terminal
            .draw(|f| render_frame(f, &state, &spark, &mut proxy_table, &mut conn_table))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            let mut x = 0u16;
            while x < buffer.area.width {
                let cell = &buffer[(x, y)];
                text.push_str(cell.symbol());
                let c = cell.symbol().chars().next().unwrap_or(' ');
                x += if (c as u32) >= 0x2E80 { 2 } else { 1 };
            }
            text.push('\n');
        }
        assert!(text.contains("No subscriptions — press 'a' to add"));
    }
}
