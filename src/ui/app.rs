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

use crate::app::connection_manager::ConnectionManager;
use crate::app::proxy_manager::ProxyManager;
use crate::os;
use crate::app::state::{ActiveView::*, AppState, LoadingKind, ProxyMode, SharedState};
use crate::subscription::manager::SubscriptionManager;
use crate::ui::keybindings::{parse_key, parse_mouse, Action};
use crate::ui::views::{connections, dashboard, help, logs, mode_selector, proxies, rules, settings, sidebar};
use crate::ui::widgets::{sparkline::TrafficSpark, status_bar};

const LOG_CAP: usize = 1000;

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
            let Some(ref client) = client else { return; };

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
                if let Ok(c) = conns_r { s.connections = c; }
                if let Ok(r) = rules_r { s.rules = r; }
                if let Ok(t) = traffic_r { s.traffic = t; }
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
                if let Ok(m) = memory_r { s.memory = m; }
                s.update_time();
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
                    if key.kind == KeyEventKind::Release { continue; }
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
                                        let _ = ProxyManager::set_proxy_mode(client, &target).await;
                                        refresh_state(&s2).await;
                                    }
                                    let mut s = s2.lock().await;
                                    s.ui.loading = None;
                                });
                            }
                            KeyCode::Esc => {
                                s.ui.show_mode_selector = false;
                            }
                            _ => {}
                        }
                    } else if let Some(action) = parse_key(key) {
                        if !handle_action(&action, &mut s, state.clone()).await { break; }
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
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture);
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

async fn handle_action(
    action: &Action, s: &mut AppState, shared: SharedState,
) -> bool {
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
                }
                let mut s = shared2.lock().await;
                s.ui.loading = None;
            });
        }
        Action::SwitchView(i) => {
            let v = match i {
                0 => Dashboard, 1 => Proxies, 2 => Connections, 3 => Rules, 4 => Logs, _ => return true,
            };
            s.ui.active_view = v;
        }
        Action::MoveDown => match s.ui.active_view {
            Proxies => {
                let i = s.ui.selected_group_idx;
                let m = s.groups.get(i).map(|g| g.all.len().saturating_sub(1)).unwrap_or(0);
                s.ui.selected_node_idx = (s.ui.selected_node_idx + 1).min(m);
            }
            Connections => {
                let m = s.connections.len().saturating_sub(1);
                s.ui.selected_conn_idx = (s.ui.selected_conn_idx + 1).min(m);
            }
            _ => {}
        },
        Action::MoveUp => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = s.ui.selected_node_idx.saturating_sub(1),
            Connections => s.ui.selected_conn_idx = s.ui.selected_conn_idx.saturating_sub(1),
            _ => {}
        },
        Action::JumpTop => match s.ui.active_view {
            Proxies => s.ui.selected_node_idx = 0,
            Connections => s.ui.selected_conn_idx = 0,
            _ => {}
        },
        Action::JumpBottom => match s.ui.active_view {
            Proxies => {
                let i = s.ui.selected_group_idx;
                let m = s.groups.get(i).map(|g| g.all.len().saturating_sub(1)).unwrap_or(0);
                s.ui.selected_node_idx = m;
            }
            Connections => s.ui.selected_conn_idx = s.connections.len().saturating_sub(1),
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
        Action::PrevGroup => {
            s.ui.selected_group_idx = s.ui.selected_group_idx.saturating_sub(1);
            s.ui.selected_node_idx = 0;
        }
        Action::NextGroup => {
            s.ui.selected_group_idx = (s.ui.selected_group_idx + 1).min(s.groups.len().saturating_sub(1));
            s.ui.selected_node_idx = 0;
        }
        Action::CloseConnection => {
            let idx = s.ui.selected_conn_idx;
            let id = s.connections.get(idx).map(|c| c.id.clone());
            if let (Some(c), Some(id)) = (client, id) {
                tokio::spawn(async move { let _ = ConnectionManager::close_one(&c, &id).await; });
            }
        }
        Action::CloseAllConnections => {
            if let Some(c) = client {
                tokio::spawn(async move { let _ = ConnectionManager::close_all(&c).await; });
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
                if let Some(ref client) = c {
                    let _ = SubscriptionManager::update_all(&mut cfg, client).await;
                }
                let mut state = shared2.lock().await;
                state.config = cfg;
                state.ui.loading = None;
            });
        }
        Action::Back => {
            if s.ui.search_mode {
                s.ui.search_mode = false;
                s.ui.search_query.clear();
            } else if s.ui.show_mode_selector { s.ui.show_mode_selector = false; }
            else if s.ui.show_help { s.ui.show_help = false; }
            else if s.ui.show_settings { s.ui.show_settings = false; }
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
        s.ui.selected_group_idx = s.ui.selected_group_idx.min(s.groups.len().saturating_sub(1));
        if let Some(grp) = s.groups.get(s.ui.selected_group_idx) {
            s.ui.selected_node_idx = s.ui.selected_node_idx.min(grp.all.len().saturating_sub(1));
        }
    }
    if let Ok(c) = conns { s.connections = c; }
    if let Ok(r) = rules { s.rules = r; }
    if let Ok(t) = traffic { s.traffic = t; }
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
    if let Ok(m) = memory { s.memory = m; }
    s.update_time();
}
