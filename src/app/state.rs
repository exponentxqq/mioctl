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
