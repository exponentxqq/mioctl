use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Terminal,
};

use crate::api::types::{Proxy, ProxyHistory};
use crate::app::connection_manager::ConnectionManager;
use crate::app::proxy_manager::ProxyManager;
use crate::app::state::{ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState, LOG_CAP};
use crate::os;
use crate::subscription::manager::SubscriptionManager;
use crate::ui::keybindings::{parse_key, parse_mouse, Action};
use crate::ui::views::{
    connections, dashboard, help, logs, mode_selector, proxies, rules, settings, sidebar,
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
    use std::io::Write;
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

/// Toggle `tun.enable` in a mihomo YAML config file.
fn set_tun_config(path: &str, enable: bool) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?;
    let mut doc: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("parse YAML: {}", e))?;
    let tun = doc["tun"]
        .as_mapping_mut()
        .ok_or_else(|| "no 'tun' section in config".to_string())?;
    tun.insert(
        serde_yaml::Value::String("enable".into()),
        serde_yaml::Value::Bool(enable),
    );
    let out = serde_yaml::to_string(&doc).map_err(|e| format!("serialize YAML: {}", e))?;
    std::fs::write(path, out).map_err(|e| format!("write {}: {}", path, e))?;
    Ok(())
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

    loop {
        if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release {
                        continue;
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
                                s.ui.loading = Some(LoadingKind::SwitchMode);
                                let c = s.client.clone();
                                let s2 = state.clone();
                                tokio::spawn(async move {
                                    if let Some(ref client) = c {
                                        match ProxyManager::set_proxy_mode(client, &target).await {
                                            Ok(()) => {
                                                refresh_state(&s2).await;
                                                let mut s = s2.lock().await;
                                                s.add_log(
                                                    "info",
                                                    &format!("Mode switched to {:?}", target),
                                                );
                                                s.ui.loading = None;
                                            }
                                            Err(e) => {
                                                let mut s = s2.lock().await;
                                                s.add_log(
                                                    "error",
                                                    &format!("Failed to switch mode: {}", e),
                                                );
                                                s.ui.loading = None;
                                            }
                                        }
                                    } else {
                                        let mut s = s2.lock().await;
                                        s.ui.loading = None;
                                    }
                                });
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
                                let copied = copy_to_clipboard(&text);
                                s.ui.log_visual = false;
                                if !copied {
                                    s.add_log(
                                        "error",
                                        "xclip failed — is xclip installed? (pacman -S xclip)",
                                    );
                                }
                                drop(s);
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
                        if s.ui.show_help || s.ui.show_settings || s.ui.show_mode_selector {
                            s.ui.show_help = false;
                            s.ui.show_settings = false;
                            s.ui.show_mode_selector = false;
                        } else {
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
            .draw(|f| render_frame(f, &s, &spark, &mut proxy_table, &mut conn_table))
            .map_err(|e| e.to_string())?;
    }

    // Abort background tasks so shutdown is instant
    init_handle.abort();

    // Fault-tolerant cleanup — attempt all steps even if some fail
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
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

async fn handle_action(action: &Action, s: &mut AppState, shared: SharedState) -> bool {
    let client = s.client.clone();
    match action {
        Action::Quit => return false,
        Action::Refresh => {
            s.ui.loading = Some(LoadingKind::Refresh);
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
                let tun_enabled = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                let config_path = s.config.mihomo.config_path.clone();
                let shared2 = shared.clone();
                tokio::spawn(async move {
                    if tun_enabled {
                        // TUN ON → disable TUN, enable system proxy
                        if let Err(e) = set_tun_config(&config_path, false) {
                            let mut s = shared2.lock().await;
                            s.add_log("error", &format!("Failed to update config: {}", e));
                        }
                        let _ = c.restart().await;
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        let mut s = shared2.lock().await;
                        s.connect();
                        // Set system proxy BEFORE refresh_state so it's detected
                        if let Some(port) = s.mixed_port {
                            if let Err(e) = crate::os::proxy::set_system_proxy(port) {
                                s.add_log("error", &format!("System proxy failed: {}", e));
                            }
                        }
                        drop(s);
                        refresh_state(&shared2).await;
                        let mut s = shared2.lock().await;
                        // Start proxy guard
                        if let Some(port) = s.mixed_port {
                            if let Some(old) = s.proxy_guard.take() {
                                old.abort();
                            }
                            s.proxy_guard = Some(start_proxy_guard(shared2.clone(), port));
                        }
                        s.add_log("info", "TUN disabled, system proxy enabled");
                        s.ui.loading = None;
                    } else {
                        // TUN OFF → enable TUN, disable system proxy
                        crate::os::proxy::clear_system_proxy();
                        // Stop proxy guard if running
                        {
                            let mut s = shared2.lock().await;
                            if let Some(g) = s.proxy_guard.take() {
                                g.abort();
                            }
                        }
                        match set_tun_config(&config_path, true) {
                            Ok(()) => {
                                let _ = c.restart().await;
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                let mut s = shared2.lock().await;
                                s.connect();
                                drop(s);
                                refresh_state(&shared2).await;
                                let mut s = shared2.lock().await;
                                let actual = s.tun.as_ref().map(|t| t.enable).unwrap_or(false);
                                if actual {
                                    s.add_log("info", "TUN enabled");
                                } else {
                                    s.add_log("error", "TUN toggle: config updated but TUN did not start — check stack/permissions in mihomo config");
                                }
                                s.ui.loading = None;
                            }
                            Err(e) => {
                                let mut s = shared2.lock().await;
                                s.add_log("error", &format!("Failed to update config: {}", e));
                                s.ui.loading = None;
                            }
                        }
                    }
                });
            }
        }
        Action::SwitchNode => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let group_name = s.groups.get(i).map(|g| g.name.clone());
            let node_name = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
                s.ui.loading = Some(LoadingKind::SwitchNode);
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
                s.ui.loading = Some(LoadingKind::TestNodeDelay);
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
                s.ui.loading = Some(LoadingKind::TestGroupDelay);
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
        Action::UpdateSubs => {
            s.ui.loading = Some(LoadingKind::UpdateSubs);
            let mut cfg = s.config.clone();
            let c = s.client.clone();
            let shared2 = shared.clone();
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
                let (text, copied) = if s.ui.log_visual {
                    let t = collect_log_selection(s);
                    let ok = copy_to_clipboard(&t);
                    s.ui.log_visual = false;
                    (t, ok)
                } else if let Some(entry) = s.logs.get(s.ui.log_cursor) {
                    let t = entry.payload.clone();
                    let ok = copy_to_clipboard(&t);
                    (t, ok)
                } else {
                    return true;
                };
                if !copied {
                    s.add_log(
                        "error",
                        "Clipboard unavailable — install wl-clipboard (Wayland) or xclip (X11)",
                    );
                } else {
                    s.add_log("info", &format!("Copied: {} chars", text.len()));
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
    fn test_set_tun_no_tun_section() {
        let yaml = "mode: rule\nport: 7890\n";
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().to_string();
        std::fs::write(&path, yaml).unwrap();

        let result = set_tun_config(&path, true);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no 'tun' section"));
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
}
