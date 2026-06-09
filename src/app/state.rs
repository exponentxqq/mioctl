use std::sync::Arc;
use tokio::sync::Mutex;
use crate::api::client::MihomoClient;
use crate::api::types::*;
use crate::config::mioctl_config::MioctlConfig;
use chrono::Local;

#[derive(Debug, Clone, PartialEq)]
pub enum ProxyMode {
    Global,
    Rule,
    Direct,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveView {
    Dashboard,
    Proxies,
    Connections,
    Rules,
    Logs,
}

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

pub struct AppState {
    pub config: MioctlConfig,
    pub client: Option<MihomoClient>,
    pub ui: UiState,

    pub proxies: ProxiesResponse,
    pub groups: Vec<Group>,
    pub rules: RulesResponse,
    pub connections: Vec<Connection>,
    pub traffic: Traffic,
    pub memory: Memory,
    pub tun: Option<TunConfig>,
    pub mixed_port: Option<u16>,
    pub allow_lan: Option<bool>,
    pub system_proxy_enabled: bool,
    pub version: String,
    pub logs: Vec<LogEntry>,
    pub proxy_providers: std::collections::HashMap<String, ProxyProvider>,

    pub connected: bool,
    pub proxy_mode: ProxyMode,
    pub last_updated: String,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: MioctlConfig::load(),
            client: None,
            ui: UiState::default(),
            proxies: ProxiesResponse {
                proxies: std::collections::HashMap::new(),
            },
            groups: Vec::new(),
            rules: RulesResponse { rules: Vec::new() },
            connections: Vec::new(),
            traffic: Traffic { up: 0, down: 0 },
            memory: Memory { inuse: 0, oslimit: 0 },
            tun: None,
            mixed_port: None,
            allow_lan: None,
            system_proxy_enabled: false,
            version: String::new(),
            logs: Vec::new(),
            proxy_providers: std::collections::HashMap::new(),
            connected: false,
            proxy_mode: ProxyMode::Rule,
            last_updated: String::new(),
        }
    }

    pub fn connect(&mut self) {
        let cfg = &self.config.mihomo;
        self.client =
            MihomoClient::new(&cfg.external_controller, Some(cfg.secret.clone())).ok();
    }

    pub fn update_time(&mut self) {
        self.last_updated = Local::now().format("%H:%M:%S").to_string();
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(AppState::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_state_defaults() {
        let ui = UiState::default();
        assert!(!ui.show_mode_selector);
        assert_eq!(ui.mode_selector_idx, 0);
        assert!(!ui.show_help);
        assert!(!ui.show_settings);
    }

    #[test]
    fn test_mode_selector_idx_from_proxy_mode() {
        assert_eq!(match ProxyMode::Rule { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 }, 0);
        assert_eq!(match ProxyMode::Global { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 }, 1);
        assert_eq!(match ProxyMode::Direct { ProxyMode::Rule => 0, ProxyMode::Global => 1, ProxyMode::Direct => 2 }, 2);
    }

    #[test]
    fn test_proxy_mode_default_is_rule() {
        let state = AppState::new();
        assert_eq!(state.proxy_mode, ProxyMode::Rule);
    }
}
