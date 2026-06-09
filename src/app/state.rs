use std::sync::Arc;
use tokio::sync::Mutex;
use crate::api::client::MihomoClient;
use crate::api::types::*;
use crate::config::mioctl_config::MioctlConfig;
use chrono::Local;

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
    pub loading: Option<LoadingKind>,
    pub spinner_frame: u8,
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
            loading: None,
            spinner_frame: 0,
        }
    }
}

#[allow(dead_code)]
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

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: MioctlConfig::load(),
            client: None,
            ui: UiState {
                loading: Some(LoadingKind::Init),
                ..UiState::default()
            },
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
        assert!(ui.loading.is_none());
        assert_eq!(ui.spinner_frame, 0);
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

    #[test]
    fn test_app_state_init_shows_loading() {
        let state = AppState::new();
        assert_eq!(state.ui.loading, Some(LoadingKind::Init));
    }

    #[test]
    fn test_loading_kind_as_str() {
        assert_eq!(LoadingKind::Init.as_str(), "Loading...");
        assert_eq!(LoadingKind::Refresh.as_str(), "Refreshing...");
        assert_eq!(LoadingKind::SwitchMode.as_str(), "Switching mode...");
        assert_eq!(LoadingKind::SwitchNode.as_str(), "Switching node...");
        assert_eq!(LoadingKind::ToggleProxy.as_str(), "Toggling proxy...");
        assert_eq!(LoadingKind::TestNodeDelay.as_str(), "Testing delay...");
        assert_eq!(LoadingKind::TestGroupDelay.as_str(), "Testing group delay...");
        assert_eq!(LoadingKind::UpdateSubs.as_str(), "Updating subscriptions...");
    }

    #[test]
    fn test_spinner_frame_wraps() {
        let mut ui = UiState::default();
        ui.spinner_frame = 9;
        ui.spinner_frame = (ui.spinner_frame + 1) % 10;
        assert_eq!(ui.spinner_frame, 0);
    }

    #[test]
    fn test_loading_set_and_clear() {
        let mut ui = UiState::default();
        assert!(ui.loading.is_none());
        ui.loading = Some(LoadingKind::SwitchMode);
        assert_eq!(ui.loading, Some(LoadingKind::SwitchMode));
        ui.loading = None;
        assert!(ui.loading.is_none());
    }
}
