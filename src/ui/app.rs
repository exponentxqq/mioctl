use std::time::Duration;

use crossterm::{
    event::{self, EnableMouseCapture, DisableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Terminal,
};

use crate::app::connection_manager::ConnectionManager;
use crate::app::proxy_manager::ProxyManager;
use crate::app::state::{ActiveView::*, AppState, SharedState};
use crate::ui::keybindings::{parse_key, parse_mouse, Action};
use crate::ui::views::{connections, dashboard, help, logs, proxies, rules, sidebar};
use crate::ui::widgets::{sparkline::TrafficSpark, status_bar};

pub async fn run_tui() -> Result<(), String> {
    let state: SharedState = crate::app::state::new_shared_state();

    // Background: connect + load initial data (lock briefly, await unlocked)
    let init_handle = {
        let s = state.clone();
        tokio::spawn(async move {
            let client = {
                let mut s = s.lock().await;
                s.connect();
                s.client.clone()
            };
            let Some(ref client) = client else { return; };

            // Do ALL network I/O WITHOUT holding the lock
            let version = client.get_version().await;
            let proxies = ProxyManager::refresh_all(client).await;
            let conns = ConnectionManager::list(client).await;
            let rules = client.get_rules().await;
            let traffic = client.get_traffic().await;

            // Lock briefly to update state
            let mut s = s.lock().await;
            if let Ok(v) = version {
                s.version = v.version;
                s.connected = true;
            }
            if let Ok((p, g)) = proxies {
                s.proxies = p;
                s.groups = g;
                s.proxy_mode = ProxyManager::detect_proxy_mode(&s.groups);
            }
            if let Ok(c) = conns { s.connections = c; }
            if let Ok(r) = rules { s.rules = r; }
            if let Ok(t) = traffic { s.traffic = t; }
            s.update_time();
        })
    };

    // Setup terminal
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture).map_err(|e| e.to_string())?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut spark = TrafficSpark::new();
    let mut proxy_table = ratatui::widgets::TableState::default();
    let mut conn_table = ratatui::widgets::TableState::default();

    let poll_interval = Duration::from_secs(3);
    let mut last_poll = tokio::time::Instant::now();

    loop {
        if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Release { continue; }
                    if let Some(action) = parse_key(key) {
                        let mut s = state.lock().await;
                        if !handle_action(&action, &mut s).await { break; }
                    }
                }
                Event::Mouse(mouse) => {
                    if let Some(action) = parse_mouse(mouse) {
                        let mut s = state.lock().await;
                        if s.ui.show_help {
                            // Close help on any click outside
                            s.ui.show_help = false;
                        } else {
                            handle_action(&action, &mut s).await;
                        }
                    }
                }
                _ => {}
            }
        }

        if last_poll.elapsed() >= poll_interval {
            let client = { state.lock().await.client.clone() };
            if let Some(ref client) = client {
                // Fire all requests concurrently with short timeout
                let t = Duration::from_secs(3);
                let (proxies, conns, rules, traffic) = tokio::join!(
                    tokio::time::timeout(t, ProxyManager::refresh_all(client)),
                    tokio::time::timeout(t, ConnectionManager::list(client)),
                    tokio::time::timeout(t, client.get_rules()),
                    tokio::time::timeout(t, client.get_traffic()),
                );
                let proxies = proxies.unwrap_or(Err(crate::api::error::ApiError::Timeout));
                let conns = conns.unwrap_or(Err(crate::api::error::ApiError::Timeout));
                let rules = rules.unwrap_or(Err(crate::api::error::ApiError::Timeout));
                let traffic = traffic.unwrap_or(Err(crate::api::error::ApiError::Timeout));

                // Lock briefly to update state
                let mut s = state.lock().await;
                if let Ok((p, g)) = proxies {
                    s.proxies = p;
                    s.groups = g;
                    s.proxy_mode = ProxyManager::detect_proxy_mode(&s.groups);
                }
                if let Ok(c) = conns { s.connections = c; }
                if let Ok(r) = rules { s.rules = r; }
                if let Ok(t) = traffic { s.traffic = t; }
                spark.push(s.traffic.up, s.traffic.down);
                s.update_time();
            }
            last_poll = tokio::time::Instant::now();
        }

        let s = state.lock().await;
        terminal
            .draw(|f| render_frame(f, &s, &spark, &mut proxy_table, &mut conn_table))
            .map_err(|e| e.to_string())?;
    }

    // Abort background tasks so shutdown is instant
    init_handle.abort();

    disable_raw_mode().map_err(|e| e.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture).map_err(|e| e.to_string())?;
    terminal.show_cursor().map_err(|e| e.to_string())?;
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
}

async fn handle_action(action: &Action, s: &mut AppState) -> bool {
    let client = s.client.clone();
    match action {
        Action::Quit => return false,
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
        Action::CycleMode => {
            if let Some(c) = client {
                let mode = s.proxy_mode.clone();
                tokio::spawn(async move { let _ = ProxyManager::cycle_proxy_mode(&c, mode).await; });
            }
        }
        Action::SwitchNode => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let group_name = s.groups.get(i).map(|g| g.name.clone());
            let node_name = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
                tokio::spawn(async move { let _ = ProxyManager::switch_node(&c, &gn, &nn).await; });
            }
        }
        Action::TestNodeDelay => {
            let i = s.ui.selected_group_idx;
            let j = s.ui.selected_node_idx;
            let node = s.groups.get(i).and_then(|g| g.all.get(j).cloned());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(n)) = (client, node) {
                tokio::spawn(async move { let _ = ProxyManager::test_node_delay(&c, &n, &url, timeout).await; });
            }
        }
        Action::TestGroupDelay => {
            let i = s.ui.selected_group_idx;
            let group = s.groups.get(i).map(|g| g.name.clone());
            let url = s.config.preferences.delay_test_url.clone();
            let timeout = s.config.preferences.delay_test_timeout_ms;
            if let (Some(c), Some(g)) = (client, group) {
                tokio::spawn(async move { let _ = ProxyManager::test_group_delay(&c, &g, &url, timeout).await; });
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
        Action::Back => {
            if s.ui.show_help {
                s.ui.show_help = false;
            }
            // Back is also used in proxies view context (Esc)
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
        _ => {}
    }
    true
}
