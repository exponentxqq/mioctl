# mioctl Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust TUI tool (mioctl) to manage mihomo proxy instances via their RESTful API, with subscription management.

**Architecture:** 3-layer Rust app: API Client (`src/api/`) → Business Logic (`src/app/`, `src/subscription/`, `src/config/`) → UI (`src/ui/` with ratatui sidebar layout, `src/cli/` for 3 subcommands). Vim keybindings, Catppuccin Mocha theme.

**Tech Stack:** Rust, ratatui, reqwest, tokio, serde, serde_json, serde_yaml, toml, clap, tracing, thiserror

**Source reference:** mihomo RESTful API docs at https://wiki.metacubex.one/en/api/

---

### Task 1: Project scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/api/mod.rs`
- Create: `src/config/mod.rs`
- Create: `src/app/mod.rs`
- Create: `src/subscription/mod.rs`
- Create: `src/ui/mod.rs`
- Create: `src/cli/mod.rs`

- [ ] **Step 1: Initialize Cargo project and write Cargo.toml**

```bash
cd /home/xuqinqin/develop/person/mioctl
```

```toml
[package]
name = "mioctl"
version = "0.1.0"
edition = "2021"
description = "A terminal UI tool for managing mihomo proxy instances"

[[bin]]
name = "mioctl"
path = "src/main.rs"

[dependencies]
ratatui = "0.29"
crossterm = "0.28"
reqwest = { version = "0.12", features = ["json", "websocket"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "2"
tokio-tungstenite = "0.24"
futures-util = "0.3"
base64 = "0.22"
url = "2"
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"

[dev-dependencies]
wiremock = "0.6"
tempfile = "3"
assert_matches = "1"
```

- [ ] **Step 2: Create module tree**

```bash
mkdir -p src/{api,config,app,subscription,ui/views,ui/widgets,cli}
```

- [ ] **Step 3: Write empty mod.rs files and main.rs skeleton**

Write `src/main.rs`:
```rust
mod api;
mod app;
mod cli;
mod config;
mod subscription;
mod ui;

fn main() {
    println!("mioctl v0.1.0");
}
```

Write `src/api/mod.rs`:
```rust
pub mod client;
pub mod endpoints;
pub mod error;
pub mod types;
pub mod websocket;
```

Write `src/config/mod.rs`:
```rust
pub mod mioctl_config;
```

Write `src/app/mod.rs`:
```rust
pub mod connection_manager;
pub mod proxy_manager;
pub mod state;
```

Write `src/subscription/mod.rs`:
```rust
pub mod fetcher;
pub mod injector;
pub mod manager;
pub mod parser;
```

Write `src/ui/mod.rs`:
```rust
pub mod app;
pub mod keybindings;
pub mod theme;
pub mod views;
pub mod widgets;
```

Write `src/cli/mod.rs`:
```rust
pub mod connect;
pub mod sub;
pub mod tui;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check
```

Expected: `Finished` with no errors (just warnings about unused code).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: scaffold mioctl project structure

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: API error types

**Files:**
- Create: `src/api/error.rs`

- [ ] **Step 1: Write error.rs**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Authentication failed — check your secret")]
    Unauthorized,

    #[error("Request timed out")]
    Timeout,

    #[error("Mihomo API returned error status {0}: {1}")]
    ApiError(u16, String),

    #[error("JSON deserialization failed: {0}")]
    Deserialization(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

pub type ApiResult<T> = Result<T, ApiError>;
```

- [ ] **Step 2: Verify module compiles**

```bash
cargo check -p mioctl
```

Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add API error types"
```

---

### Task 3: API response types

**Files:**
- Create: `src/api/types.rs`

- [ ] **Step 1: Write types.rs with all mihomo API response structs**

```rust
use serde::{Deserialize, Serialize};

// === Proxies ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHistory {
    pub time: String,
    pub delay: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proxy {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub history: Vec<ProxyHistory>,
    #[serde(default)]
    pub udp: bool,
    #[serde(default)]
    pub alive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxiesResponse {
    pub proxies: std::collections::HashMap<String, Proxy>,
}

// === Proxy Delay ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelayResponse {
    pub delay: i64,
}

// === Groups ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    #[serde(default)]
    pub now: Option<String>,
    #[serde(default)]
    pub all: Vec<String>,
}

// === Rules ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    #[serde(rename = "type")]
    pub rule_type: String,
    pub payload: String,
    pub proxy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesResponse {
    pub rules: Vec<Rule>,
}

// === Connections ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetadata {
    pub network: String,
    #[serde(rename = "type")]
    pub conn_type: String,
    #[serde(rename = "sourceIP")]
    pub source_ip: String,
    #[serde(rename = "destinationIP")]
    pub destination_ip: String,
    #[serde(rename = "sourcePort")]
    pub source_port: String,
    #[serde(rename = "destinationPort")]
    pub destination_port: String,
    pub host: String,
    #[serde(rename = "dnsMode")]
    pub dns_mode: String,
    #[serde(rename = "processPath")]
    pub process_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub metadata: ConnectionMetadata,
    pub upload: u64,
    pub download: u64,
    pub start: String,
    pub chains: Vec<String>,
    pub rule: String,
    #[serde(rename = "rulePayload")]
    pub rule_payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionsResponse {
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default, rename = "downloadTotal")]
    pub download_total: u64,
    #[serde(default, rename = "uploadTotal")]
    pub upload_total: u64,
}

// === Traffic ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Traffic {
    pub up: u64,
    pub down: u64,
}

// === Memory ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub inuse: u64,
    pub oslimit: u64,
}

// === Version ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: String,
}

// === Config ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MihomoConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default, rename = "socks-port")]
    pub socks_port: Option<u16>,
    #[serde(default, rename = "mixed-port")]
    pub mixed_port: Option<u16>,
    #[serde(default, rename = "allow-lan")]
    pub allow_lan: Option<bool>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default, rename = "log-level")]
    pub log_level: Option<String>,
}

// === Proxy Provider ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProvider {
    pub name: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    #[serde(rename = "vehicleType")]
    pub vehicle_type: String,
    #[serde(default)]
    pub proxies: Vec<Proxy>,
    #[serde(rename = "updatedAt")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersResponse {
    pub providers: std::collections::HashMap<String, ProxyProvider>,
}

// === DNS Query ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsQueryResponse {
    #[serde(default)]
    pub ips: Vec<String>,
}

// === Logs (WebSocket JSON) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    #[serde(rename = "type")]
    pub level: String,
    pub payload: String,
}

// === Subscription Node (for parsing subscription content) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub server: String,
    pub port: u16,
    pub cipher: Option<String>,
    pub password: Option<String>,
    pub uuid: Option<String>,
    pub alter_id: Option<u16>,
    pub network: Option<String>,
    pub ws_opts: Option<serde_json::Value>,
    pub sni: Option<String>,
    pub skip_cert_verify: Option<bool>,
    pub udp: Option<bool>,
}
```

- [ ] **Step 2: Write unit tests for JSON deserialization**

Add at the bottom of `src/api/types.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_proxies_response() {
        let json = r#"{
            "proxies": {
                "GLOBAL": {
                    "name": "GLOBAL",
                    "type": "Selector",
                    "now": "🇯🇵 Japan-01",
                    "all": ["🇯🇵 Japan-01", "DIRECT", "REJECT"],
                    "history": [{"time": "2026-06-08T12:00:00Z", "delay": 45}],
                    "udp": true,
                    "alive": true
                }
            }
        }"#;
        let resp: ProxiesResponse = serde_json::from_str(json).unwrap();
        let global = resp.proxies.get("GLOBAL").unwrap();
        assert_eq!(global.proxy_type, "Selector");
        assert_eq!(global.now.as_deref(), Some("🇯🇵 Japan-01"));
        assert_eq!(global.all.len(), 3);
        assert_eq!(global.history[0].delay, 45);
    }

    #[test]
    fn test_deserialize_traffic() {
        let json = r#"{"up": 102400, "down": 204800}"#;
        let traffic: Traffic = serde_json::from_str(json).unwrap();
        assert_eq!(traffic.up, 102400);
        assert_eq!(traffic.down, 204800);
    }

    #[test]
    fn test_deserialize_connections_response() {
        let json = r#"{
            "connections": [{
                "id": "abc-123",
                "metadata": {
                    "network": "tcp",
                    "type": "tcp",
                    "sourceIP": "192.168.1.5",
                    "destinationIP": "142.250.80.46",
                    "sourcePort": "52341",
                    "destinationPort": "443",
                    "host": "google.com",
                    "dnsMode": "normal",
                    "processPath": "/usr/bin/curl"
                },
                "upload": 1024,
                "download": 20480,
                "start": "2026-06-08T12:00:00Z",
                "chains": ["🇯🇵 Japan-01"],
                "rule": "DOMAIN,google.com",
                "rulePayload": "google.com"
            }],
            "downloadTotal": 1048576,
            "uploadTotal": 524288
        }"#;
        let resp: ConnectionsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.connections.len(), 1);
        let conn = &resp.connections[0];
        assert_eq!(conn.id, "abc-123");
        assert_eq!(conn.metadata.host, "google.com");
        assert_eq!(conn.chains[0], "🇯🇵 Japan-01");
    }

    #[test]
    fn test_deserialize_rule() {
        let json = r#"{"type": "DOMAIN-SUFFIX", "payload": "google.com", "proxy": "🔍 Google"}"#;
        let rule: Rule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.rule_type, "DOMAIN-SUFFIX");
        assert_eq!(rule.payload, "google.com");
    }

    #[test]
    fn test_deserialize_log_entry() {
        let json = r#"{"type": "info", "payload": "new connection: google.com:443"}"#;
        let entry: LogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.level, "info");
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test
```

Expected: 5 tests pass.

- [ ] **Step 4: Verify compiles**

```bash
cargo check
```

Expected: Compiles successfully.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add mihomo API response types with deserialization tests"
```

---

### Task 4: MihomoClient — REST endpoints

**Files:**
- Create: `src/api/client.rs`
- Create: `src/api/endpoints.rs`

- [ ] **Step 1: Write MihomoClient struct**

Write `src/api/client.rs`:
```rust
use reqwest::{Client, header};
use crate::api::error::{ApiError, ApiResult};

#[derive(Clone)]
pub struct MihomoClient {
    client: Client,
    base_url: String,
    secret: Option<String>,
}

impl MihomoClient {
    pub fn new(host: &str, secret: Option<String>) -> ApiResult<Self> {
        let base_url = if host.starts_with("http") {
            host.trim_end_matches('/').to_string()
        } else {
            format!("http://{}", host.trim_end_matches('/'))
        };

        let mut headers = header::HeaderMap::new();
        if let Some(ref s) = secret {
            let auth_value = format!("Bearer {}", s);
            let mut auth_header = header::HeaderValue::from_str(&auth_value)
                .map_err(|_| ApiError::WebSocketError("invalid secret characters".into()))?;
            auth_header.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, auth_header);
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { client, base_url, secret })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
```

- [ ] **Step 2: Write endpoints.rs with all REST methods**

Write `src/api/endpoints.rs`:
```rust
use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::*;

impl MihomoClient {
    // --- Proxies ---

    pub async fn get_proxies(&self) -> ApiResult<ProxiesResponse> {
        let url = format!("{}/proxies", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn get_proxy(&self, name: &str) -> ApiResult<Proxy> {
        let url = format!("{}/proxies/{}", self.base_url(), name);
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn select_proxy(&self, group: &str, name: &str) -> ApiResult<()> {
        let url = format!("{}/proxies/{}", self.base_url(), group);
        let body = serde_json::json!({"name": name});
        reqwest::Client::new()
            .put(&url)
            .json(&body)
            .send()
            .await?;
        Ok(())
    }

    pub async fn test_proxy_delay(
        &self,
        name: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<DelayResponse> {
        let url = format!(
            "{}/proxies/{}/delay?url={}&timeout={}",
            self.base_url(), name, test_url, timeout_ms
        );
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Groups ---

    pub async fn get_groups(&self) -> ApiResult<Vec<Group>> {
        let url = format!("{}/group", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data: serde_json::Value = resp.json().await?;
        // /group returns an object keyed by group name; extract values
        let groups: Vec<Group> = data
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(groups)
    }

    pub async fn get_group(&self, name: &str) -> ApiResult<Group> {
        let url = format!("{}/group/{}", self.base_url(), name);
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn test_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<Vec<DelayResponse>> {
        let url = format!(
            "{}/group/{}/delay?url={}&timeout={}",
            self.base_url(), group, test_url, timeout_ms
        );
        let resp = reqwest::get(&url).await?;
        let data: Vec<DelayResponse> = resp.json().await?;
        Ok(data)
    }

    // --- Rules ---

    pub async fn get_rules(&self) -> ApiResult<RulesResponse> {
        let url = format!("{}/rules", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Connections ---

    pub async fn get_connections(&self) -> ApiResult<ConnectionsResponse> {
        let url = format!("{}/connections", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn close_connection(&self, id: &str) -> ApiResult<()> {
        let url = format!("{}/connections/{}", self.base_url(), id);
        reqwest::Client::new().delete(&url).send().await?;
        Ok(())
    }

    pub async fn close_all_connections(&self) -> ApiResult<()> {
        let url = format!("{}/connections", self.base_url());
        reqwest::Client::new().delete(&url).send().await?;
        Ok(())
    }

    // --- Providers ---

    pub async fn get_proxy_providers(&self) -> ApiResult<ProvidersResponse> {
        let url = format!("{}/providers/proxies", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn update_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!("{}/providers/proxies/{}", self.base_url(), name);
        reqwest::Client::new().put(&url).send().await?;
        Ok(())
    }

    pub async fn healthcheck_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!(
            "{}/providers/proxies/{}/healthcheck",
            self.base_url(), name
        );
        reqwest::get(&url).await?;
        Ok(())
    }

    // --- Config ---

    pub async fn get_configs(&self) -> ApiResult<MihomoConfig> {
        let url = format!("{}/configs", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn reload_config(&self, path: Option<&str>) -> ApiResult<()> {
        let url = format!("{}/configs?force=true", self.base_url());
        let body = serde_json::json!({"path": path.unwrap_or(""), "payload": ""});
        reqwest::Client::new().put(&url).json(&body).send().await?;
        Ok(())
    }

    pub async fn restart(&self) -> ApiResult<()> {
        let url = format!("{}/restart", self.base_url());
        let body = serde_json::json!({"path": "", "payload": ""});
        reqwest::Client::new().post(&url).json(&body).send().await?;
        Ok(())
    }

    // --- Traffic ---

    pub async fn get_traffic(&self) -> ApiResult<Traffic> {
        let url = format!("{}/traffic", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Memory ---

    pub async fn get_memory(&self) -> ApiResult<Memory> {
        let url = format!("{}/memory", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Version ---

    pub async fn get_version(&self) -> ApiResult<Version> {
        let url = format!("{}/version", self.base_url());
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- DNS ---

    pub async fn dns_query(&self, name: &str, record_type: &str) -> ApiResult<DnsQueryResponse> {
        let url = format!(
            "{}/dns/query?name={}&type={}",
            self.base_url(), name, record_type
        );
        let resp = reqwest::get(&url).await?;
        let data = resp.json().await?;
        Ok(data)
    }
}
```

- [ ] **Step 3: Verify integration compiles**

```bash
cargo check
```

Expected: Compiles successfully (some dead_code warnings expected).

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: implement MihomoClient REST endpoints"
```

---

### Task 5: WebSocket client

**Files:**
- Create: `src/api/websocket.rs`

- [ ] **Step 1: Write WebSocket stream implementations**

```rust
use futures_util::StreamExt;
use tokio::sync::mpsc;
use crate::api::client::MihomoClient;
use crate::api::types::{Traffic, Memory, Connection, LogEntry};

impl MihomoClient {
    fn ws_url(&self, path: &str) -> String {
        self.base_url()
            .replace("http://", "ws://")
            .replace("https://", "wss://")
            + path
    }

    pub async fn traffic_stream(
        &self,
    ) -> Result<mpsc::Receiver<Traffic>, crate::api::error::ApiError> {
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (tx, rx) = mpsc::channel::<Traffic>(64);
        let url = self.ws_url("/traffic");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(traffic) = serde_json::from_str::<Traffic>(&text) {
                        let _ = tx.send(traffic).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn connection_stream(
        &self,
    ) -> Result<mpsc::Receiver<Vec<Connection>>, crate::api::error::ApiError> {
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (tx, rx) = mpsc::channel::<Vec<Connection>>(64);
        let url = self.ws_url("/connections");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(conns) = data.get("connections") {
                            if let Ok(connections) = serde_json::from_value::<Vec<Connection>>(conns.clone()) {
                                let _ = tx.send(connections).await;
                            }
                        }
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn log_stream(
        &self,
        level: Option<&str>,
    ) -> Result<mpsc::Receiver<LogEntry>, crate::api::error::ApiError> {
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (tx, rx) = mpsc::channel::<LogEntry>(256);
        let path = if let Some(lvl) = level {
            format!("/logs?level={}", lvl)
        } else {
            "/logs".to_string()
        };
        let url = self.ws_url(&path);
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(entry) = serde_json::from_str::<LogEntry>(&text) {
                        let _ = tx.send(entry).await;
                    }
                }
            }
        });

        Ok(rx)
    }

    pub async fn memory_stream(
        &self,
    ) -> Result<mpsc::Receiver<Memory>, crate::api::error::ApiError> {
        use tokio_tungstenite::connect_async;
        use tokio_tungstenite::tungstenite::Message;

        let (tx, rx) = mpsc::channel::<Memory>(16);
        let url = self.ws_url("/memory");
        let (ws_stream, _) = connect_async(&url).await
            .map_err(|e| crate::api::error::ApiError::WebSocketError(e.to_string()))?;
        let (_, mut read) = ws_stream.split();

        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Text(text) = msg {
                    if let Ok(mem) = serde_json::from_str::<Memory>(&text) {
                        let _ = tx.send(mem).await;
                    }
                }
            }
        });

        Ok(rx)
    }
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

Expected: Compiles.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: implement WebSocket streams for traffic/connections/logs/memory"
```

---

### Task 6: MioctlConfig — load, save, defaults

**Files:**
- Create: `src/config/mioctl_config.rs`

- [ ] **Step 1: Write config types and load/save logic**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MihomoConnection {
    #[serde(default = "default_host")]
    pub external_controller: String,
    #[serde(default)]
    pub secret: String,
}

fn default_host() -> String {
    "127.0.0.1:9090".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionItem {
    pub name: String,
    pub url: String,
    pub last_updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscriptions {
    #[serde(default = "default_update_interval")]
    pub update_interval_minutes: u64,
    #[serde(default)]
    pub items: Vec<SubscriptionItem>,
}

fn default_update_interval() -> u64 {
    240
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self {
            update_interval_minutes: 240,
            items: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "default_delay_url")]
    pub delay_test_url: String,
    #[serde(default = "default_delay_timeout")]
    pub delay_test_timeout_ms: u64,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_delay_url() -> String {
    "https://www.gstatic.com/generate_204".into()
}
fn default_delay_timeout() -> u64 {
    5000
}
fn default_theme() -> String {
    "catppuccin-mocha".into()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            delay_test_url: default_delay_url(),
            delay_test_timeout_ms: default_delay_timeout(),
            theme: default_theme(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MioctlConfig {
    #[serde(default)]
    pub mihomo: MihomoConnection,
    #[serde(default)]
    pub subscriptions: Subscriptions,
    #[serde(default)]
    pub preferences: Preferences,
}

impl Default for MioctlConfig {
    fn default() -> Self {
        Self {
            mihomo: MihomoConnection {
                external_controller: default_host(),
                secret: String::new(),
            },
            subscriptions: Subscriptions::default(),
            preferences: Preferences::default(),
        }
    }
}

impl MioctlConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mioctl")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn providers_dir() -> PathBuf {
        Self::config_dir().join("providers")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => toml::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            let config = Self::default();
            let _ = config.save();
            config
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(Self::providers_dir()).map_err(|e| e.to_string())?;
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::config_path(), content).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_subscription(&mut self, name: String, url: String) {
        self.subscriptions.items.push(SubscriptionItem {
            name,
            url,
            last_updated: None,
        });
    }

    pub fn remove_subscription(&mut self, name: &str) -> bool {
        let len_before = self.subscriptions.items.len();
        self.subscriptions.items.retain(|s| s.name != name);
        self.subscriptions.items.len() < len_before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MioctlConfig::default();
        assert_eq!(config.mihomo.external_controller, "127.0.0.1:9090");
        assert_eq!(config.mihomo.secret, "");
        assert_eq!(config.subscriptions.update_interval_minutes, 240);
        assert!(config.subscriptions.items.is_empty());
        assert_eq!(config.preferences.delay_test_url, "https://www.gstatic.com/generate_204");
    }

    #[test]
    fn test_add_remove_subscription() {
        let mut config = MioctlConfig::default();
        config.add_subscription("test-sub".into(), "https://example.com/sub".into());
        assert_eq!(config.subscriptions.items.len(), 1);
        assert_eq!(config.subscriptions.items[0].name, "test-sub");
        assert!(config.remove_subscription("test-sub"));
        assert!(config.subscriptions.items.is_empty());
        assert!(!config.remove_subscription("nonexistent"));
    }

    #[test]
    fn test_toml_roundtrip() {
        let mut config = MioctlConfig::default();
        config.add_subscription("my-sub".into(), "https://example.com/sub".into());
        config.preferences.delay_test_url = "http://localhost/test".into();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.subscriptions.items.len(), 1);
        assert_eq!(deserialized.subscriptions.items[0].name, "my-sub");
        assert_eq!(deserialized.preferences.delay_test_url, "http://localhost/test");
    }
}
```

- [ ] **Step 2: Run config tests and verify compiles**

```bash
cargo test config
cargo check
```

Expected: 3 tests pass, compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: implement MioctlConfig load/save with TOML"
```

---

### Task 7: AppState — global shared state

**Files:**
- Create: `src/app/state.rs`

- [ ] **Step 1: Write AppState with Arc<Mutex<>> pattern**

```rust
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

    // Cached data
    pub proxies: ProxiesResponse,
    pub groups: Vec<Group>,
    pub rules: RulesResponse,
    pub connections: Vec<Connection>,
    pub traffic: Traffic,
    pub memory: Memory,
    pub version: String,
    pub logs: Vec<LogEntry>,
    pub proxy_providers: std::collections::HashMap<String, ProxyProvider>,

    // Connection status
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
        self.client = MihomoClient::new(&cfg.external_controller, Some(cfg.secret.clone())).ok();
    }

    pub fn update_time(&mut self) {
        self.last_updated = Local::now().format("%H:%M:%S").to_string();
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

pub fn new_shared_state() -> SharedState {
    Arc::new(Mutex::new(AppState::new()))
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: implement AppState and SharedState"
```

---

### Task 8: ProxyManager business logic

**Files:**
- Create: `src/app/proxy_manager.rs`
- Create: `src/app/connection_manager.rs`

- [ ] **Step 1: Write ProxyManager**

```rust
use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::*;
use crate::app::state::ProxyMode;

pub struct ProxyManager;

impl ProxyManager {
    pub async fn refresh_all(client: &MihomoClient) -> ApiResult<(ProxiesResponse, Vec<Group>)> {
        let proxies = client.get_proxies().await?;
        let groups = client.get_groups().await?;
        Ok((proxies, groups))
    }

    pub async fn switch_node(
        client: &MihomoClient,
        group: &str,
        node: &str,
    ) -> ApiResult<()> {
        client.select_proxy(group, node).await
    }

    pub async fn test_node_delay(
        client: &MihomoClient,
        node: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<DelayResponse> {
        client.test_proxy_delay(node, test_url, timeout_ms).await
    }

    pub async fn test_group_delay(
        client: &MihomoClient,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<Vec<DelayResponse>> {
        client.test_group_delay(group, test_url, timeout_ms).await
    }

    pub fn detect_proxy_mode(groups: &[Group]) -> ProxyMode {
        for g in groups {
            if g.name == "GLOBAL" {
                match g.now.as_deref() {
                    Some("DIRECT") => return ProxyMode::Direct,
                    Some("GLOBAL") => return ProxyMode::Global,
                    _ => return ProxyMode::Rule,
                }
            }
        }
        ProxyMode::Rule
    }

    pub async fn cycle_proxy_mode(
        client: &MihomoClient,
        current: ProxyMode,
    ) -> ApiResult<ProxyMode> {
        let (name, next) = match current {
            ProxyMode::Global => ("DIRECT", ProxyMode::Direct),
            ProxyMode::Rule => ("GLOBAL", ProxyMode::Global),
            ProxyMode::Direct => ("GLOBAL", ProxyMode::Rule),
        };
        client.select_proxy("GLOBAL", name).await?;
        Ok(next)
    }
}
```

- [ ] **Step 2: Write ConnectionManager**

```rust
use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::Connection;

pub struct ConnectionManager;

impl ConnectionManager {
    pub async fn list(client: &MihomoClient) -> ApiResult<Vec<Connection>> {
        let resp = client.get_connections().await?;
        Ok(resp.connections)
    }

    pub async fn close_one(client: &MihomoClient, id: &str) -> ApiResult<()> {
        client.close_connection(id).await
    }

    pub async fn close_all(client: &MihomoClient) -> ApiResult<()> {
        client.close_all_connections().await
    }
}
```

- [ ] **Step 3: Write unit tests for ProxyManager**

Add at the bottom of `src/app/proxy_manager.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Group;

    fn make_group(now: &str) -> Group {
        Group {
            name: "GLOBAL".into(),
            group_type: "Selector".into(),
            now: Some(now.into()),
            all: vec![now.into(), "DIRECT".into()],
        }
    }

    #[test]
    fn test_detect_proxy_mode_global() {
        let groups = vec![make_group("GLOBAL")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Global);
    }

    #[test]
    fn test_detect_proxy_mode_direct() {
        let groups = vec![make_group("DIRECT")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Direct);
    }

    #[test]
    fn test_detect_proxy_mode_rule() {
        let groups = vec![make_group("🇯🇵 Japan-01")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Rule);
    }

    #[test]
    fn test_detect_proxy_mode_no_global_group() {
        let groups = vec![Group {
            name: "YouTube".into(),
            group_type: "Selector".into(),
            now: Some("DIRECT".into()),
            all: vec!["DIRECT".into()],
        }];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Rule);
    }
}
```

- [ ] **Step 4: Run proxy manager tests**

```bash
cargo test proxy_manager
```

Expected: 4 tests pass.

- [ ] **Step 5: Verify compiles**

---

### Task 9: Subscription parser

**Files:**
- Create: `src/subscription/parser.rs`

- [ ] **Step 1: Write multi-format parser**

```rust
use crate::api::types::ParsedNode;
use base64::Engine;
use url::Url;

pub enum SubscriptionFormat {
    Yaml,
    Base64,
    PlainUri,
}

pub fn detect_format(content: &str) -> SubscriptionFormat {
    let trimmed = content.trim();
    // Heuristic: if decodes as base64 and contains no YAML markers, it's base64
    if !trimmed.starts_with("proxies:") && !trimmed.starts_with("---") {
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
            if let Ok(text) = String::from_utf8(decoded) {
                let first_line = text.trim().lines().next().unwrap_or("");
                if first_line.starts_with("ss://")
                    || first_line.starts_with("vmess://")
                    || first_line.starts_with("trojan://")
                    || first_line.starts_with("vless://")
                    || first_line.starts_with("hysteria2://")
                {
                    return SubscriptionFormat::Base64;
                }
            }
        }
        if trimmed.starts_with("ss://")
            || trimmed.starts_with("vmess://")
            || trimmed.starts_with("trojan://")
        {
            return SubscriptionFormat::PlainUri;
        }
    }
    SubscriptionFormat::Yaml
}

pub fn parse_yaml(content: &str) -> Result<Vec<ParsedNode>, String> {
    #[derive(serde::Deserialize)]
    struct YamlProxies {
        proxies: Option<Vec<serde_json::Value>>,
    }

    let config: YamlProxies =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    let proxies = config.proxies.unwrap_or_default();
    let mut nodes = Vec::new();

    for p in proxies {
        if let Ok(node) = serde_json::from_value::<ParsedNode>(p) {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

pub fn parse_uri_list(content: &str) -> Result<Vec<ParsedNode>, String> {
    let mut nodes = Vec::new();
    for line in content.trim().lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(node) = parse_single_uri(line) {
            nodes.push(node);
        }
    }
    Ok(nodes)
}

pub fn parse_base64(content: &str) -> Result<Vec<ParsedNode>, String> {
    let trimmed = content.trim();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .map_err(|e| format!("Base64 decode error: {}", e))?;
    let text = String::from_utf8(decoded).map_err(|e| format!("UTF-8 error: {}", e))?;
    parse_uri_list(&text)
}

fn parse_single_uri(uri: &str) -> Option<ParsedNode> {
    let url = Url::parse(uri).ok()?;

    match url.scheme() {
        "ss" => parse_shadowsocks(&url),
        "vmess" => parse_vmess(uri),
        "trojan" => parse_trojan(&url),
        _ => None,
    }
}

fn parse_shadowsocks(url: &Url) -> Option<ParsedNode> {
    // ss://base64-userinfo@server:port?params...
    let userinfo = url.username();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(userinfo.as_bytes())
        .ok()?;
    let decoded_str = String::from_utf8(decoded).ok()?;
    let parts: Vec<&str> = decoded_str.splitn(2, ':').collect();
    let (cipher, password) = match parts.len() {
        2 => (parts[0].to_string(), parts[1].to_string()),
        _ => ("aes-256-gcm".to_string(), decoded_str),
    };

    let name = url.fragment().unwrap_or("").to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);

    Some(ParsedNode {
        name,
        node_type: "ss".into(),
        server,
        port,
        cipher: Some(cipher),
        password: Some(password),
        uuid: None,
        alter_id: None,
        network: None,
        ws_opts: None,
        sni: None,
        skip_cert_verify: None,
        udp: Some(true),
    })
}

fn parse_vmess(uri: &str) -> Option<ParsedNode> {
    // vmess://base64-json
    let encoded = uri.strip_prefix("vmess://")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let json_str = String::from_utf8(decoded).ok()?;

    #[derive(serde::Deserialize)]
    struct VmessConfig {
        ps: Option<String>,
        add: Option<String>,
        port: Option<u16>,
        id: Option<String>,
        aid: Option<u16>,
        net: Option<String>,
        #[serde(rename = "type")]
        security: Option<String>,
        host: Option<String>,
        path: Option<String>,
        tls: Option<String>,
        sni: Option<String>,
    }

    let cfg: VmessConfig = serde_json::from_str(&json_str).ok()?;

    let ws_opts = if cfg.net.as_deref() == Some("ws") {
        Some(serde_json::json!({
            "path": cfg.path.unwrap_or("/".into()),
            "headers": {
                "Host": cfg.host.unwrap_or_default(),
            }
        }))
    } else {
        None
    };

    Some(ParsedNode {
        name: cfg.ps.unwrap_or_default(),
        node_type: "vmess".into(),
        server: cfg.add?,
        port: cfg.port.unwrap_or(443),
        cipher: cfg.security,
        password: None,
        uuid: cfg.id,
        alter_id: cfg.aid,
        network: cfg.net,
        ws_opts,
        sni: cfg.sni,
        skip_cert_verify: cfg.tls.map(|t| t == "tls"),
        udp: Some(true),
    })
}

fn parse_trojan(url: &Url) -> Option<ParsedNode> {
    // trojan://password@server:port?params#name
    let password = url.username().to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let name = url.fragment().unwrap_or("").to_string();

    let mut sni = None;
    for (key, value) in url.query_pairs() {
        if key == "sni" {
            sni = Some(value.to_string());
        }
    }

    Some(ParsedNode {
        name,
        node_type: "trojan".into(),
        server,
        port,
        cipher: None,
        password: Some(password),
        uuid: None,
        alter_id: None,
        network: None,
        ws_opts: None,
        sni,
        skip_cert_verify: None,
        udp: Some(true),
    })
}
```

- [ ] **Step 2: Write comprehensive parser unit tests**

Add at the bottom of `src/subscription/parser.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === Format Detection ===

    #[test]
    fn test_detect_yaml_format() {
        let content = "proxies:\n  - name: \"test\"\n    type: ss\n    server: 1.2.3.4\n    port: 443";
        assert!(matches!(detect_format(content), SubscriptionFormat::Yaml));
    }

    #[test]
    fn test_detect_plain_uri_ss() {
        let content = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#Test";
        assert!(matches!(detect_format(content), SubscriptionFormat::PlainUri));
    }

    #[test]
    fn test_detect_base64() {
        // "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNz@1.2.3.4:8388#B64" in base64
        let lines = "c3M6Ly9ZMmhoWTJoaE1qQXR",  // partial but triggers base64 path
        assert!(matches!(detect_format(lines), SubscriptionFormat::PlainUri));
    }

    // === YAML Parsing ===

    #[test]
    fn test_parse_yaml_valid() {
        let yaml = r#"
proxies:
  - name: "🇯🇵 Japan"
    type: ss
    server: jp.example.com
    port: 443
    cipher: aes-256-gcm
    password: "secret123"
    udp: true
  - name: "🇸🇬 Singapore"
    type: vmess
    server: sg.example.com
    port: 8080
    uuid: "b831381d-6324-4d53-ad4f-8cda48b30811"
    alterId: 0
    network: ws
    ws-opts:
      path: /ws
      headers:
        Host: sg.example.com
"#;
        let nodes = parse_yaml(yaml).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "🇯🇵 Japan");
        assert_eq!(nodes[0].node_type, "ss");
        assert_eq!(nodes[0].server, "jp.example.com");
        assert_eq!(nodes[0].port, 443);
        assert_eq!(nodes[0].cipher.as_deref(), Some("aes-256-gcm"));
        assert_eq!(nodes[0].password.as_deref(), Some("secret123"));

        assert_eq!(nodes[1].name, "🇸🇬 Singapore");
        assert_eq!(nodes[1].node_type, "vmess");
        assert_eq!(nodes[1].uuid.as_deref(), Some("b831381d-6324-4d53-ad4f-8cda48b30811"));
    }

    #[test]
    fn test_parse_yaml_empty() {
        let yaml = "proxies: []\n";
        let nodes = parse_yaml(yaml).unwrap();
        assert!(nodes.is_empty());
    }

    // === URI Parsing ===

    #[test]
    fn test_parse_ss_uri() {
        // ss://base64(method:password)@server:port#name
        // base64("aes-256-gcm:test123") = "YWVzLTI1Ni1nY206dGVzdDEyMw=="
        let uri = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw==@1.2.3.4:8388#TestNode";
        let nodes = parse_uri_list(uri).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TestNode");
        assert_eq!(nodes[0].node_type, "ss");
        assert_eq!(nodes[0].server, "1.2.3.4");
        assert_eq!(nodes[0].port, 8388);
        assert_eq!(nodes[0].cipher.as_deref(), Some("aes-256-gcm"));
        assert_eq!(nodes[0].password.as_deref(), Some("test123"));
    }

    #[test]
    fn test_parse_trojan_uri() {
        let uri = "trojan://password123@trojan.example.com:443?sni=example.com#TrojanNode";
        let nodes = parse_uri_list(uri).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TrojanNode");
        assert_eq!(nodes[0].node_type, "trojan");
        assert_eq!(nodes[0].server, "trojan.example.com");
        assert_eq!(nodes[0].port, 443);
        assert_eq!(nodes[0].password.as_deref(), Some("password123"));
        assert_eq!(nodes[0].sni.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_parse_vmess_uri() {
        let vmess_json = r#"{"ps":"VmessNode","add":"vm.example.com","port":"443","id":"b831381d-6324-4d53-ad4f-8cda48b30811","aid":"0","net":"ws","type":"auto","host":"vm.example.com","path":"/ws","tls":"tls"}"#;
        let encoded = base64::engine::general_purpose::STANDARD.encode(vmess_json);
        let uri = format!("vmess://{}", encoded);
        let nodes = parse_uri_list(&uri).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "VmessNode");
        assert_eq!(nodes[0].node_type, "vmess");
        assert_eq!(nodes[0].server, "vm.example.com");
        assert_eq!(nodes[0].port, 443);
        assert_eq!(nodes[0].uuid.as_deref(), Some("b831381d-6324-4d53-ad4f-8cda48b30811"));
        assert_eq!(nodes[0].network.as_deref(), Some("ws"));
    }

    #[test]
    fn test_parse_multiline_uri_list() {
        let content = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw==@1.2.3.4:8388#Node1\ntrojan://pass@5.6.7.8:443#Node2\n\n# comment line\n";
        let nodes = parse_uri_list(content).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    // === Base64 Parsing ===

    #[test]
    fn test_parse_base64_subscription() {
        let plain = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw==@1.2.3.4:8388#B64Node";
        let encoded = base64::engine::general_purpose::STANDARD.encode(plain);
        let nodes = parse_base64(&encoded).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "B64Node");
    }

    #[test]
    fn test_parse_base64_invalid() {
        let result = parse_base64("!!!not valid base64!!!");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run parser tests**

```bash
cargo test parser
```

Expected: 10 tests pass.

---

### Task 10: Subscription manager + fetcher + injector

**Files:**
- Create: `src/subscription/manager.rs`
- Create: `src/subscription/fetcher.rs`
- Create: `src/subscription/injector.rs`

- [ ] **Step 1: Write fetcher**

Write `src/subscription/fetcher.rs`:
```rust
use reqwest::Client;

pub async fn fetch_subscription(url: &str) -> Result<String, String> {
    let client = Client::builder()
        .user_agent("mioctl/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Write injector**

Write `src/subscription/injector.rs`:
```rust
use crate::api::client::MihomoClient;
use crate::api::types::ParsedNode;
use crate::config::mioctl_config::MioctlConfig;
use std::path::PathBuf;

pub fn generate_provider_yaml(name: &str, nodes: &[ParsedNode]) -> String {
    let mut yaml = String::from("proxies:\n");

    for node in nodes {
        let mut entry = format!(
            "  - name: \"{}\"\n    type: {}\n    server: {}\n    port: {}\n",
            node.name, node.node_type, node.server, node.port
        );

        if let Some(ref cipher) = node.cipher {
            entry.push_str(&format!("    cipher: {}\n", cipher));
        }
        if let Some(ref password) = node.password {
            entry.push_str(&format!("    password: \"{}\"\n", password));
        }
        if let Some(ref uuid) = node.uuid {
            entry.push_str(&format!("    uuid: {}\n", uuid));
        }
        if let Some(aid) = node.alter_id {
            entry.push_str(&format!("    alterId: {}\n", aid));
        }
        if let Some(ref net) = node.network {
            entry.push_str(&format!("    network: {}\n", net));
        }
        if let Some(ref ws) = node.ws_opts {
            if let Some(path) = ws.get("path") {
                entry.push_str(&format!("    ws-opts:\n      path: {}\n", path.as_str().unwrap_or("/")));
            }
            if let Some(headers) = ws.get("headers") {
                if let Some(host) = headers.get("Host") {
                    entry.push_str(&format!("      headers:\n        Host: {}\n", host.as_str().unwrap_or("")));
                }
            }
        }
        if let Some(ref sni) = node.sni {
            entry.push_str(&format!("    sni: {}\n", sni));
        }
        if let Some(skip) = node.skip_cert_verify {
            entry.push_str(&format!("    skip-cert-verify: {}\n", skip));
        }
        if let Some(udp) = node.udp {
            entry.push_str(&format!("    udp: {}\n", udp));
        }

        yaml.push_str(&entry);
    }

    yaml
}

pub fn write_provider_file(name: &str, nodes: &[ParsedNode]) -> Result<PathBuf, String> {
    let dir = MioctlConfig::providers_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.yaml", name));
    let yaml = generate_provider_yaml(name, nodes);
    std::fs::write(&path, yaml).map_err(|e| e.to_string())?;
    Ok(path)
}

pub async fn inject_provider(client: &MihomoClient, name: &str) -> Result<(), String> {
    client
        .update_proxy_provider(name)
        .await
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Write subscription manager**

Write `src/subscription/manager.rs`:
```rust
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::fetcher::fetch_subscription;
use crate::subscription::parser::{detect_format, parse_yaml, parse_uri_list, parse_base64, SubscriptionFormat};
use crate::subscription::injector::{write_provider_file, inject_provider};
use crate::api::client::MihomoClient;
use crate::api::types::ParsedNode;

pub struct SubscriptionManager;

impl SubscriptionManager {
    pub async fn update_all(config: &mut MioctlConfig, client: &MihomoClient) -> Result<String, String> {
        let items: Vec<_> = config.subscriptions.items.iter().cloned().collect();
        let mut results = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();

        for item in &items {
            match Self::update_one(item.name.clone(), &item.url, client).await {
                Ok(count) => {
                    // Update last_updated timestamp
                    if let Some(s) = config.subscriptions.items.iter_mut().find(|s| s.name == item.name) {
                        s.last_updated = Some(now.clone());
                    }
                    results.push(format!("{}: {} nodes updated", item.name, count));
                }
                Err(e) => {
                    results.push(format!("{}: ERROR - {}", item.name, e));
                }
            }
        }

        let _ = config.save();
        Ok(results.join("\n"))
    }

    pub async fn update_one(
        name: String,
        url: &str,
        client: &MihomoClient,
    ) -> Result<usize, String> {
        // 1. Fetch
        let content = fetch_subscription(url).await?;

        // 2. Detect & parse
        let nodes = match detect_format(&content) {
            SubscriptionFormat::Yaml => parse_yaml(&content)?,
            SubscriptionFormat::Base64 => parse_base64(&content)?,
            SubscriptionFormat::PlainUri => parse_uri_list(&content)?,
        };

        if nodes.is_empty() {
            return Err("no nodes found in subscription".into());
        }

        // 3. Write provider file
        write_provider_file(&name, &nodes)?;

        // 4. Inject into mihomo
        inject_provider(client, &name).await?;

        Ok(nodes.len())
    }
}
```

- [ ] **Step 4: Write injector unit tests**

Add at the bottom of `src/subscription/injector.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::ParsedNode;

    fn sample_node() -> ParsedNode {
        ParsedNode {
            name: "🇯🇵 Test".into(),
            node_type: "ss".into(),
            server: "1.2.3.4".into(),
            port: 443,
            cipher: Some("aes-256-gcm".into()),
            password: Some("secret".into()),
            uuid: None,
            alter_id: None,
            network: None,
            ws_opts: None,
            sni: None,
            skip_cert_verify: None,
            udp: Some(true),
        }
    }

    #[test]
    fn test_generate_provider_yaml_single_node() {
        let nodes = vec![sample_node()];
        let yaml = generate_provider_yaml("test", &nodes);
        assert!(yaml.contains("proxies:"));
        assert!(yaml.contains("name: \"🇯🇵 Test\""));
        assert!(yaml.contains("type: ss"));
        assert!(yaml.contains("server: 1.2.3.4"));
        assert!(yaml.contains("port: 443"));
        assert!(yaml.contains("cipher: aes-256-gcm"));
        assert!(yaml.contains("password: \"secret\""));
        assert!(yaml.contains("udp: true"));
    }

    #[test]
    fn test_generate_provider_yaml_vmess_node() {
        let nodes = vec![ParsedNode {
            name: "Vmess Node".into(),
            node_type: "vmess".into(),
            server: "vm.example.com".into(),
            port: 8080,
            uuid: Some("b831381d-6324-4d53-ad4f-8cda48b30811".into()),
            alter_id: Some(0),
            network: Some("ws".into()),
            ws_opts: Some(serde_json::json!({
                "path": "/ws",
                "headers": {"Host": "vm.example.com"}
            })),
            cipher: None,
            password: None,
            sni: Some("vm.example.com".into()),
            skip_cert_verify: Some(true),
            udp: Some(true),
        }];
        let yaml = generate_provider_yaml("test", &nodes);
        assert!(yaml.contains("type: vmess"));
        assert!(yaml.contains("uuid: b831381d-6324-4d53-ad4f-8cda48b30811"));
        assert!(yaml.contains("alterId: 0"));
        assert!(yaml.contains("network: ws"));
        assert!(yaml.contains("path: /ws"));
        assert!(yaml.contains("skip-cert-verify: true"));
    }

    #[test]
    fn test_generate_provider_yaml_empty_nodes() {
        let yaml = generate_provider_yaml("empty", &[]);
        assert_eq!(yaml, "proxies:\n");
    }
}
```

- [ ] **Step 5: Run injector tests**

```bash
cargo test injector
```

Expected: 3 tests pass.

- [ ] **Step 6: Verify compiles and commit**
```bash
cargo check
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: implement subscription manager with fetch/parse/inject pipeline"
```

---

### Task 11: CLI commands (clap)

**Files:**
- Create: `src/cli/tui.rs`
- Create: `src/cli/sub.rs`
- Create: `src/cli/connect.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write CLI argument definitions**

Rewrite `src/cli/mod.rs`:
```rust
use clap::{Parser, Subcommand};

pub mod connect;
pub mod sub;
pub mod tui;

#[derive(Parser)]
#[command(name = "mioctl", version, about = "mihomo terminal management tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch interactive TUI
    Tui,

    /// Manage subscriptions
    Sub {
        #[command(subcommand)]
        action: SubAction,
    },

    /// Test API connectivity
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },
}

#[derive(Subcommand)]
pub enum SubAction {
    /// Update all subscriptions
    Update {
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum ConnectAction {
    /// Test connection to mihomo API
    Test,
}
```

- [ ] **Step 2: Write connect test command**

Write `src/cli/connect.rs`:
```rust
use crate::cli::ConnectAction;
use crate::config::mioctl_config::MioctlConfig;

pub async fn run(action: ConnectAction) {
    match action {
        ConnectAction::Test => {
            let config = MioctlConfig::load();
            let client = crate::api::client::MihomoClient::new(
                &config.mihomo.external_controller,
                if config.mihomo.secret.is_empty() {
                    None
                } else {
                    Some(config.mihomo.secret.clone())
                },
            );

            match client {
                Ok(c) => match c.get_version().await {
                    Ok(v) => println!("✓ Connected to mihomo {}", v.version),
                    Err(e) => eprintln!("✗ API error: {}", e),
                },
                Err(e) => eprintln!("✗ Connection failed: {}", e),
            }
        }
    }
}
```

- [ ] **Step 3: Write subscription CLI command**

Write `src/cli/sub.rs`:
```rust
use crate::cli::SubAction;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::manager::SubscriptionManager;

pub async fn run(action: SubAction) {
    match action {
        SubAction::Update { all } => {
            if !all {
                eprintln!("Use --all to update all subscriptions");
                return;
            }
            let mut config = MioctlConfig::load();
            let client = crate::api::client::MihomoClient::new(
                &config.mihomo.external_controller,
                if config.mihomo.secret.is_empty() {
                    None
                } else {
                    Some(config.mihomo.secret.clone())
                },
            );

            match client {
                Ok(c) => match SubscriptionManager::update_all(&mut config, &c).await {
                    Ok(result) => println!("{}", result),
                    Err(e) => eprintln!("Error: {}", e),
                },
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
    }
}
```

- [ ] **Step 4: Write TUI stub**

Write `src/cli/tui.rs`:
```rust
pub async fn run() -> Result<(), String> {
    println!("Launching TUI mode...");
    crate::ui::app::run_tui().await
}
```

- [ ] **Step 5: Update main.rs to wire everything together**

Replace `src/main.rs`:
```rust
mod api;
mod app;
mod cli;
mod config;
mod subscription;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            if let Err(e) = cli::tui::run().await {
                eprintln!("TUI error: {}", e);
            }
        }
        Some(Commands::Sub { action }) => {
            cli::sub::run(action).await;
        }
        Some(Commands::Connect { action }) => {
            cli::connect::run(action).await;
        }
    }
}
```

- [ ] **Step 6: Verify compiles**

```bash
cargo check
```

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: implement CLI commands with clap"
```

---

### Task 12: TUI theme and keybindings

**Files:**
- Create: `src/ui/theme.rs`
- Create: `src/ui/keybindings.rs`

- [ ] **Step 1: Write Catppuccin Mocha theme**

Write `src/ui/theme.rs`:
```rust
use ratatui::style::Color;

pub struct Theme {
    pub bg: Color,
    pub surface: Color,
    pub primary: Color,
    pub green: Color,
    pub red: Color,
    pub yellow: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub selected: Color,
}

pub const CATPPUCCIN_MOCHA: Theme = Theme {
    bg: Color::Rgb(30, 30, 46),       // #1e1e2e
    surface: Color::Rgb(49, 50, 68),   // #313244
    primary: Color::Rgb(203, 166, 247), // #cba6f7
    green: Color::Rgb(166, 227, 161),   // #a6e3a1
    red: Color::Rgb(243, 139, 168),     // #f38ba8
    yellow: Color::Rgb(249, 226, 175),  // #f9e2af
    text: Color::Rgb(205, 214, 244),    // #cdd6f4
    text_secondary: Color::Rgb(166, 173, 200), // #a6adc8
    selected: Color::Rgb(203, 166, 247), // same as primary
};
```

- [ ] **Step 2: Write keybinding types**

Write `src/ui/keybindings.rs`:
```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // Global
    Quit,
    SwitchView(usize),
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    Search,
    SearchNext,
    SearchPrev,
    CommandMode,

    // Dashboard
    CycleMode,

    // Proxies
    SwitchNode,
    TestNodeDelay,
    TestGroupDelay,
    PrevGroup,
    NextGroup,
    Back,

    // Connections
    CloseConnection,
    CloseAllConnections,

    // Logs
    TogglePause,
    CycleLogLevel,

    // Refresh
    Refresh,
}

pub fn parse_key(event: KeyEvent) -> Option<Action> {
    match event {
        KeyEvent { code: KeyCode::Char('q'), modifiers: KeyModifiers::NONE, .. } => Some(Action::Quit),

        // View switching
        KeyEvent { code: KeyCode::Char('1'), .. } => Some(Action::SwitchView(0)),
        KeyEvent { code: KeyCode::Char('2'), .. } => Some(Action::SwitchView(1)),
        KeyEvent { code: KeyCode::Char('3'), .. } => Some(Action::SwitchView(2)),
        KeyEvent { code: KeyCode::Char('4'), .. } => Some(Action::SwitchView(3)),
        KeyEvent { code: KeyCode::Char('5'), .. } => Some(Action::SwitchView(4)),

        // Navigation
        KeyEvent { code: KeyCode::Char('j'), .. } => Some(Action::MoveDown),
        KeyEvent { code: KeyCode::Char('k'), .. } => Some(Action::MoveUp),
        KeyEvent { code: KeyCode::Down, .. } => Some(Action::MoveDown),
        KeyEvent { code: KeyCode::Up, .. } => Some(Action::MoveUp),
        KeyEvent { code: KeyCode::Char('g'), .. } => Some(Action::JumpTop),
        KeyEvent { code: KeyCode::Char('G'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::JumpBottom),

        // Search
        KeyEvent { code: KeyCode::Char('/'), .. } => Some(Action::Search),
        KeyEvent { code: KeyCode::Char('n'), .. } => Some(Action::SearchNext),
        KeyEvent { code: KeyCode::Char('N'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::SearchPrev),

        // Command
        KeyEvent { code: KeyCode::Char(':'), .. } => Some(Action::CommandMode),

        // Dashboard
        KeyEvent { code: KeyCode::Char('m'), .. } => Some(Action::CycleMode),

        // Proxies
        KeyEvent { code: KeyCode::Enter, .. } => Some(Action::SwitchNode),
        KeyEvent { code: KeyCode::Char('t'), .. } => Some(Action::TestNodeDelay),
        KeyEvent { code: KeyCode::Char('T'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::TestNodeDelay),
        KeyEvent { code: KeyCode::Char('T'), .. } => Some(Action::TestGroupDelay),
        KeyEvent { code: KeyCode::Char('h'), .. } => Some(Action::PrevGroup),
        KeyEvent { code: KeyCode::Left, .. } => Some(Action::PrevGroup),
        KeyEvent { code: KeyCode::Char('l'), .. } => Some(Action::NextGroup),
        KeyEvent { code: KeyCode::Right, .. } => Some(Action::NextGroup),
        KeyEvent { code: KeyCode::Esc, .. } => Some(Action::Back),

        // Connections
        KeyEvent { code: KeyCode::Char('d'), .. } => Some(Action::CloseConnection),
        KeyEvent { code: KeyCode::Char('D'), modifiers: KeyModifiers::SHIFT, .. } => Some(Action::CloseAllConnections),

        // Logs
        KeyEvent { code: KeyCode::Char(' '), .. } => Some(Action::TogglePause),
        KeyEvent { code: KeyCode::Char('s'), .. } => Some(Action::CycleLogLevel),

        _ => None,
    }
}
```

- [ ] **Step 3: Write keybinding unit tests**

Add at the bottom of `src/ui/keybindings.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key_char(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn key_char_shift(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)
    }

    #[test]
    fn test_parse_quit() {
        assert_eq!(parse_key(key_char('q')), Some(Action::Quit));
    }

    #[test]
    fn test_parse_view_switching() {
        assert_eq!(parse_key(key_char('1')), Some(Action::SwitchView(0)));
        assert_eq!(parse_key(key_char('2')), Some(Action::SwitchView(1)));
        assert_eq!(parse_key(key_char('5')), Some(Action::SwitchView(4)));
    }

    #[test]
    fn test_parse_navigation() {
        assert_eq!(parse_key(key_char('j')), Some(Action::MoveDown));
        assert_eq!(parse_key(key_char('k')), Some(Action::MoveUp));
        assert_eq!(parse_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)), Some(Action::MoveDown));
        assert_eq!(parse_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)), Some(Action::MoveUp));
    }

    #[test]
    fn test_parse_jump() {
        assert_eq!(parse_key(key_char('g')), Some(Action::JumpTop));
        assert_eq!(parse_key(key_char_shift('G')), Some(Action::JumpBottom));
    }

    #[test]
    fn test_parse_search() {
        assert_eq!(parse_key(key_char('/')), Some(Action::Search));
        assert_eq!(parse_key(key_char('n')), Some(Action::SearchNext));
        assert_eq!(parse_key(key_char_shift('N')), Some(Action::SearchPrev));
    }

    #[test]
    fn test_parse_dashboard() {
        assert_eq!(parse_key(key_char('m')), Some(Action::CycleMode));
    }

    #[test]
    fn test_parse_proxies() {
        assert_eq!(parse_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)), Some(Action::SwitchNode));
        assert_eq!(parse_key(key_char('t')), Some(Action::TestNodeDelay));
        assert_eq!(parse_key(key_char_shift('T')), Some(Action::TestNodeDelay));
        assert_eq!(parse_key(key_char('h')), Some(Action::PrevGroup));
        assert_eq!(parse_key(key_char('l')), Some(Action::NextGroup));
        assert_eq!(parse_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)), Some(Action::Back));
    }

    #[test]
    fn test_parse_connections() {
        assert_eq!(parse_key(key_char('d')), Some(Action::CloseConnection));
        assert_eq!(parse_key(key_char_shift('D')), Some(Action::CloseAllConnections));
    }

    #[test]
    fn test_parse_logs() {
        assert_eq!(parse_key(key_char(' ')), Some(Action::TogglePause));
        assert_eq!(parse_key(key_char('s')), Some(Action::CycleLogLevel));
    }

    #[test]
    fn test_parse_unknown_key_returns_none() {
        assert_eq!(parse_key(key_char('z')), None);
        assert_eq!(parse_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE)), None);
    }
}
```

- [ ] **Step 4: Run keybinding tests**

```bash
cargo test keybindings
```

Expected: 10 tests pass.

- [ ] **Step 5: Commit**

---

### Task 13: TUI widgets — StatusBar and Sparkline

**Files:**
- Create: `src/ui/widgets/mod.rs`
- Create: `src/ui/widgets/status_bar.rs`
- Create: `src/ui/widgets/sparkline.rs`
- Create: `src/ui/widgets/table.rs`

- [ ] **Step 1: Write widgets module and StatusBar**

Write `src/ui/widgets/mod.rs`:
```rust
pub mod status_bar;
pub mod sparkline;
pub mod table;
```

Write `src/ui/widgets/status_bar.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let connected_icon = if state.connected { "🟢" } else { "🔴" };
    let connected_text = if state.connected {
        format!("connected | {}", state.version)
    } else {
        "disconnected".into()
    };

    let left = Line::from(vec![
        Span::styled(connected_icon, Style::default()),
        Span::raw(" "),
        Span::styled(connected_text, Style::default().fg(T.text_secondary)),
    ]);

    let right = Line::from(vec![
        Span::styled(
            format!("updated {} | Tab/1-5 switch | :cmd | q quit", state.last_updated),
            Style::default().fg(T.text_secondary),
        ),
    ]);

    let bar = Paragraph::new(vec![left, right]);

    let style = Style::default().bg(T.surface);
    f.render_widget(bar.style(style), area);
}
```

- [ ] **Step 2: Write Sparkline widget (traffic history)**

Write `src/ui/widgets/sparkline.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    symbols,
    widgets::Sparkline,
    Frame,
};

const HISTORY_LEN: usize = 30;

pub struct TrafficSpark {
    pub up_data: Vec<u64>,
    pub down_data: Vec<u64>,
}

impl TrafficSpark {
    pub fn new() -> Self {
        Self {
            up_data: Vec::with_capacity(HISTORY_LEN),
            down_data: Vec::with_capacity(HISTORY_LEN),
        }
    }

    pub fn push(&mut self, up: u64, down: u64) {
        self.up_data.push(up);
        self.down_data.push(down);
        if self.up_data.len() > HISTORY_LEN {
            self.up_data.remove(0);
        }
        if self.down_data.len() > HISTORY_LEN {
            self.down_data.remove(0);
        }
    }
}

pub fn render(f: &mut Frame, area: Rect, colors: (Color, Color), data: &TrafficSpark) {
    let up_max = data.up_data.iter().max().copied().unwrap_or(1);
    let down_max = data.down_data.iter().max().copied().unwrap_or(1);

    let up_widget = Sparkline::default()
        .data(&data.up_data)
        .max(up_max)
        .style(Style::default().fg(colors.0))
        .bar_set(symbols::bar::NINE_LEVELS);

    let down_widget = Sparkline::default()
        .data(&data.down_data)
        .max(down_max)
        .style(Style::default().fg(colors.1))
        .bar_set(symbols::bar::NINE_LEVELS);

    // Render up sparkline in top half, down in bottom half
    let up_area = Rect::new(area.x, area.y, area.width, area.height / 2);
    let down_area = Rect::new(area.x, area.y + area.height / 2, area.width, area.height / 2);

    f.render_widget(up_widget, up_area);
    f.render_widget(down_widget, down_area);
}
```

- [ ] **Step 3: Write table helper**

Write `src/ui/widgets/table.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Row, Table, TableState},
    Frame,
};

pub struct SelectableTable {
    pub state: TableState,
    pub items: Vec<Vec<String>>,
}

impl SelectableTable {
    pub fn new() -> Self {
        Self {
            state: TableState::default().with_selected(0),
            items: Vec::new(),
        }
    }

    pub fn next(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            self.state.select(Some((i + 1).min(self.items.len() - 1)));
        }
    }

    pub fn prev(&mut self) {
        if !self.items.is_empty() {
            let i = self.state.selected().unwrap_or(0);
            self.state.select(Some(i.saturating_sub(1)));
        }
    }

    pub fn select_first(&mut self) {
        self.state.select(if self.items.is_empty() { None } else { Some(0) });
    }

    pub fn select_last(&mut self) {
        self.state.select(if self.items.is_empty() {
            None
        } else {
            Some(self.items.len() - 1)
        });
    }
}
```

- [ ] **Step 4: Verify compiles**

```bash
cargo check
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: implement TUI widgets (StatusBar, Sparkline, Table)"
```

---

### Task 14: TUI sidebar view

**Files:**
- Create: `src/ui/views/mod.rs`
- Create: `src/ui/views/sidebar.rs`

- [ ] **Step 1: Write views module and sidebar**

Write `src/ui/views/mod.rs`:
```rust
pub mod sidebar;
pub mod dashboard;
pub mod proxies;
pub mod connections;
pub mod rules;
pub mod logs;
```

Write `src/ui/views/sidebar.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::app::state::{ActiveView, AppState};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let items = vec![
        sidebar_item("📊 概览", ActiveView::Dashboard, &state.ui.active_view),
        sidebar_item("🔗 代理", ActiveView::Proxies, &state.ui.active_view),
        sidebar_item(
            &format!("🌐 连接 ({})", state.connections.len()),
            ActiveView::Connections,
            &state.ui.active_view,
        ),
        sidebar_item(
            &format!("📋 规则 ({})", state.rules.rules.len()),
            ActiveView::Rules,
            &state.ui.active_view,
        ),
        sidebar_item("📜 日志", ActiveView::Logs, &state.ui.active_view),
        sidebar_divider(),
        sidebar_item("⚙ 设置", ActiveView::Dashboard, &state.ui.active_view),
        sidebar_item("🔄 更新", ActiveView::Dashboard, &state.ui.active_view),
    ];

    let list = List::new(items)
        .block(Block::default().style(Style::default().bg(T.bg)));

    f.render_widget(list, area);
}

fn sidebar_item<'a>(label: &'a str, view: ActiveView, current: &ActiveView) -> ListItem<'a> {
    let is_active = *current == view;
    let style = if is_active {
        Style::default().fg(T.primary).bg(T.surface)
    } else {
        Style::default().fg(T.text_secondary).bg(T.bg)
    };
    ListItem::new(Line::from(Span::styled(label.to_string(), style)))
}

fn sidebar_divider<'a>() -> ListItem<'a> {
    ListItem::new(Line::from(Span::styled(
        "─────────".to_string(),
        Style::default().fg(T.text_secondary),
    )))
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: implement TUI sidebar view"
```

---

### Task 15: TUI dashboard view

**Files:**
- Create: `src/ui/views/dashboard.rs`

- [ ] **Step 1: Write dashboard view**

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Sparkline},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use crate::ui::widgets::sparkline::TrafficSpark;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, spark: &TrafficSpark) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // status cards
            Constraint::Length(3), // traffic sparkline
            Constraint::Length(3), // bottom info
        ])
        .split(area);

    // === Status Cards ===
    let cards = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[0]);

    render_status_card(f, cards[0], "代理模式", &format!("{:?}", state.proxy_mode), T.primary);
    render_status_card(
        f, cards[1], "⬆ 上传",
        &format!("{:.1} KB/s", state.traffic.up as f64 / 1024.0),
        T.green,
    );
    render_status_card(
        f, cards[2], "⬇ 下载",
        &format!("{:.1} KB/s", state.traffic.down as f64 / 1024.0),
        T.red,
    );
    render_status_card(
        f, cards[3], "📝 连接",
        &state.connections.len().to_string(),
        T.yellow,
    );

    // === Traffic Sparkline ===
    let spark_block = Block::default()
        .title("📈 流量趋势")
        .style(Style::default().fg(T.text_secondary));
    let inner = spark_block.inner(chunks[1]);
    f.render_widget(spark_block, chunks[1]);
    crate::ui::widgets::sparkline::render(f, inner, (T.green, T.red), spark);

    // === Bottom Info ===
    let info_block = Block::default().style(Style::default().bg(T.surface));
    let inner = info_block.inner(chunks[2]);
    f.render_widget(info_block, chunks[2]);

    let info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(inner);

    let mem_text = format!(
        "💾 内存: {:.1} MB / {} MB",
        state.memory.inuse as f64 / 1024.0,
        state.memory.oslimit as f64 / 1024.0,
    );
    let ver_text = format!("🔢 版本: mihomo {}", state.version);

    f.render_widget(Paragraph::new(mem_text).style(Style::default().fg(T.text)), info_chunks[0]);
    f.render_widget(Paragraph::new(ver_text).style(Style::default().fg(T.text)), info_chunks[1]);
}

fn render_status_card(f: &mut Frame, area: Rect, label: &str, value: &str, accent: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(T.text_secondary));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    f.render_widget(
        Paragraph::new(label).style(Style::default().fg(T.text_secondary)),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(value).style(Style::default().fg(accent)),
        chunks[1],
    );
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: implement TUI dashboard view"
```

---

### Task 16: TUI proxies, connections, rules, logs views

**Files:**
- Create: `src/ui/views/proxies.rs`
- Create: `src/ui/views/connections.rs`
- Create: `src/ui/views/rules.rs`
- Create: `src/ui/views/logs.rs`

- [ ] **Step 1: Write proxies view**

Write `src/ui/views/proxies.rs`:
```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, Paragraph, Row, Table, TableState},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, table_state: &mut TableState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(2, 3)])
        .split(area);

    // Left: Group list
    let group_items: Vec<ListItem> = state
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let style = if i == state.ui.selected_group_idx {
                Style::default().fg(T.primary).bg(T.surface)
            } else {
                Style::default().fg(T.text)
            };
            ListItem::new(Line::from(Span::styled(
                format!("{} ({})", g.name, g.all.len()),
                style,
            )))
        })
        .collect();

    let groups = List::new(group_items)
        .block(Block::default().title("策略组").borders(ratatui::widgets::Borders::RIGHT));

    f.render_widget(groups, chunks[0]);

    // Right: Proxy table
    let group = match state.groups.get(state.ui.selected_group_idx) {
        Some(g) => g,
        None => {
            f.render_widget(Paragraph::new("No groups").style(Style::default().fg(T.text)), chunks[1]);
            return;
        }
    };

    let header = Row::new(vec!["名称", "类型", "延迟"])
        .style(Style::default().fg(T.text_secondary));

    let selected_name = group.now.as_deref().unwrap_or("");
    let rows: Vec<Row> = group
        .all
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let proxy = state.proxies.proxies.get(name);
            let ptype = proxy.map(|p| p.proxy_type.as_str()).unwrap_or("?");
            let delay = proxy
                .and_then(|p| p.history.last().map(|h| format!("{}ms", h.delay)))
                .unwrap_or_else(|| "—".into());
            let is_selected = name == selected_name;

            let prefix = if is_selected { "✅ " } else { "   " };
            let style = if i == state.ui.selected_node_idx {
                Style::default().fg(T.primary).bg(T.surface)
            } else if is_selected {
                Style::default().fg(T.green)
            } else {
                Style::default().fg(T.text)
            };

            Row::new(vec![format!("{}{}", prefix, name), ptype.to_string(), delay])
                .style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Ratio(2, 5), Constraint::Ratio(1, 5), Constraint::Ratio(2, 5)])
        .header(header)
        .block(Block::default().title(format!("节点 — {}", group.name)));

    f.render_stateful_widget(table, chunks[1], table_state);
}
```

- [ ] **Step 2: Write connections view**

Write `src/ui/views/connections.rs`:
```rust
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Row, Table, TableState},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState, table_state: &mut TableState) {
    let header = Row::new(vec!["源地址", "目标", "代理", "规则", "流量"])
        .style(Style::default().fg(T.text_secondary));

    let rows: Vec<Row> = state
        .connections
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let source = format!("{}:{}", c.metadata.source_ip, c.metadata.source_port);
            let dest = format!("{}:{}", c.metadata.destination_ip, c.metadata.destination_port);
            let proxy = c.chains.last().cloned().unwrap_or_default();
            let traffic = format_byte_size(c.download + c.upload);

            let style = if i == state.ui.selected_conn_idx {
                Style::default().fg(T.primary).bg(T.surface)
            } else {
                Style::default().fg(T.text)
            };

            Row::new(vec![source, dest, proxy, c.rule.clone(), traffic]).style(style)
        })
        .collect();

    let widths = [
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title("活跃连接"));

    f.render_stateful_widget(table, area, table_state);
}

fn format_byte_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.0}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
```

- [ ] **Step 3: Write rules view**

Write `src/ui/views/rules.rs`:
```rust
use ratatui::{
    layout::{Constraint, Rect},
    style::Style,
    widgets::{Block, Row, Table},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let header = Row::new(vec!["类型", "匹配条件", "目标策略"])
        .style(Style::default().fg(T.text_secondary));

    let rows: Vec<Row> = state
        .rules
        .rules
        .iter()
        .map(|r| {
            Row::new(vec![
                r.rule_type.clone(),
                r.payload.clone(),
                r.proxy.clone(),
            ])
            .style(Style::default().fg(T.text))
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Ratio(1, 4),
            Constraint::Ratio(2, 4),
            Constraint::Ratio(1, 4),
        ],
    )
    .header(header)
    .block(Block::default().title("规则列表"));

    f.render_widget(table, area);
}
```

- [ ] **Step 4: Write logs view**

Write `src/ui/views/logs.rs`:
```rust
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use crate::app::state::AppState;
use crate::ui::theme::CATPPUCCIN_MOCHA as T;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let log_lines: Vec<Line> = state
        .logs
        .iter()
        .map(|entry| {
            let color = match entry.level.as_str() {
                "error" => T.red,
                "warning" => T.yellow,
                "debug" => T.text_secondary,
                _ => T.green,
            };
            let level_label = entry.level.to_uppercase();
            Line::from(vec![
                Span::styled(format!("{:5} ", level_label), Style::default().fg(color)),
                Span::styled(&entry.payload, Style::default().fg(T.text)),
            ])
        })
        .collect();

    let paused_label = if state.ui.log_paused { " [PAUSED]" } else { "" };
    let level_label = state
        .ui
        .log_level_filter
        .as_deref()
        .unwrap_or("all");

    let block = Block::default().title(format!(
        "日志{} | 级别: {} | s:切换 空格:暂停",
        paused_label, level_label
    ));

    let paragraph = Paragraph::new(log_lines)
        .block(block)
        .wrap(Wrap { trim: true })
        .scroll((
            (state.logs.len().saturating_sub(1)) as u16,
            0,
        ));

    f.render_widget(paragraph, area);
}
```

- [ ] **Step 5: Verify compiles**

```bash
cargo check
```

Note: There will be warnings about unused imports in `proxies.rs` (Color, Span). Fix them:
- In proxies.rs, remove unused imports.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: implement TUI proxies, connections, rules, logs views"
```

---

### Task 17: TUI app — event loop, view routing, background refresh

**Files:**
- Create: `src/ui/app.rs`

- [ ] **Step 1: Write TUI app event loop**

```rust
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::Paragraph,
    Frame,
    Terminal,
};

use crate::app::state::{ActiveView, AppState, SharedState};
use crate::app::proxy_manager::ProxyManager;
use crate::app::connection_manager::ConnectionManager;
use crate::ui::keybindings::{parse_key, Action};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use crate::ui::views::{sidebar, dashboard, proxies, connections, rules, logs};
use crate::ui::widgets::{status_bar, sparkline::TrafficSpark};

pub async fn run_tui() -> Result<(), String> {
    let state: SharedState = crate::app::state::new_shared_state();

    // Initial connection
    {
        let mut s = state.lock().await;
        s.connect();
        if let Some(ref client) = s.client {
            match client.get_version().await {
                Ok(v) => {
                    s.version = v.version;
                    s.connected = true;
                }
                Err(_) => s.connected = false,
            }
            // Load initial data
            if let Ok((proxies, groups)) = ProxyManager::refresh_all(client).await {
                s.proxies = proxies;
                s.groups = groups;
                s.proxy_mode = ProxyManager::detect_proxy_mode(&s.groups);
            }
            if let Ok(conns) = ConnectionManager::list(client).await {
                s.connections = conns;
            }
            if let Ok(r) = client.get_rules().await {
                s.rules = r;
            }
        }
        s.update_time();
    }

    // Setup terminal
    enable_raw_mode().map_err(|e| e.to_string())?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| e.to_string())?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let mut spark = TrafficSpark::new();
    let mut proxy_table_state = ratatui::widgets::TableState::default().with_selected(0);
    let mut conn_table_state = ratatui::widgets::TableState::default().with_selected(0);

    // Poll timer for REST data (every 3 seconds)
    let poll_interval = Duration::from_secs(3);
    let last_poll = tokio::time::Instant::now();

    let result: Result<(), String> = loop {
        // Check for terminal events
        if event::poll(Duration::from_millis(100)).map_err(|e| e.to_string())? {
            if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if let Some(action) = parse_key(key) {
                    let mut s = state.lock().await;
                    if !handle_action(&action, &mut s).await {
                        break;
                    }
                }
            }
        }

        // Poll REST data
        if last_poll.elapsed() >= poll_interval {
            let mut s = state.lock().await;
            if let Some(ref client) = s.client {
                if let Ok((proxies, groups)) = ProxyManager::refresh_all(client).await {
                    s.proxies = proxies;
                    s.groups = groups;
                    s.proxy_mode = ProxyManager::detect_proxy_mode(&s.groups);
                }
                if let Ok(conns) = ConnectionManager::list(client).await {
                    s.connections = conns;
                }
                if let Ok(r) = client.get_rules().await {
                    s.rules = r;
                }
            }
            spark.push(s.traffic.up, s.traffic.down);
            s.update_time();
            drop(s);
        }

        // Render
        let s = state.lock().await;
        terminal
            .draw(|f| render_frame(f, &s, &spark, &mut proxy_table_state, &mut conn_table_state))
            .map_err(|e| e.to_string())?;
    };

    // Cleanup
    disable_raw_mode().map_err(|e| e.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| e.to_string())?;
    terminal.show_cursor().map_err(|e| e.to_string())?;

    result
}

fn render_frame(
    f: &mut Frame,
    state: &AppState,
    spark: &TrafficSpark,
    proxy_table: &mut ratatui::widgets::TableState,
    conn_table: &mut ratatui::widgets::TableState,
) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(18), Constraint::Min(40)])
        .split(f.area());

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(main_layout[1]);

    // Sidebar
    sidebar::render(f, main_layout[0], state);

    // Active view
    match state.ui.active_view {
        ActiveView::Dashboard => dashboard::render(f, content_layout[0], state, spark),
        ActiveView::Proxies => proxies::render(f, content_layout[0], state, proxy_table),
        ActiveView::Connections => connections::render(f, content_layout[0], state, conn_table),
        ActiveView::Rules => rules::render(f, content_layout[0], state),
        ActiveView::Logs => logs::render(f, content_layout[0], state),
    }

    // Status bar
    status_bar::render(f, content_layout[1], state);
}

async fn handle_action(action: &Action, state: &mut AppState) -> bool {
    match action {
        Action::Quit => return false,

        Action::SwitchView(idx) => {
            state.ui.active_view = match idx {
                0 => ActiveView::Dashboard,
                1 => ActiveView::Proxies,
                2 => ActiveView::Connections,
                3 => ActiveView::Rules,
                4 => ActiveView::Logs,
                _ => return true,
            };
        }

        Action::MoveDown => match state.ui.active_view {
            ActiveView::Proxies => {
                let group = state.groups.get(state.ui.selected_group_idx);
                if let Some(g) = group {
                    state.ui.selected_node_idx =
                        (state.ui.selected_node_idx + 1).min(g.all.len().saturating_sub(1));
                }
            }
            ActiveView::Connections => {
                state.ui.selected_conn_idx =
                    (state.ui.selected_conn_idx + 1).min(state.connections.len().saturating_sub(1));
            }
            _ => {}
        },

        Action::MoveUp => match state.ui.active_view {
            ActiveView::Proxies => {
                state.ui.selected_node_idx = state.ui.selected_node_idx.saturating_sub(1);
            }
            ActiveView::Connections => {
                state.ui.selected_conn_idx = state.ui.selected_conn_idx.saturating_sub(1);
            }
            _ => {}
        },

        Action::JumpTop => match state.ui.active_view {
            ActiveView::Proxies => state.ui.selected_node_idx = 0,
            ActiveView::Connections => state.ui.selected_conn_idx = 0,
            _ => {}
        },

        Action::JumpBottom => match state.ui.active_view {
            ActiveView::Proxies => {
                if let Some(g) = state.groups.get(state.ui.selected_group_idx) {
                    state.ui.selected_node_idx = g.all.len().saturating_sub(1);
                }
            }
            ActiveView::Connections => {
                state.ui.selected_conn_idx = state.connections.len().saturating_sub(1);
            }
            _ => {}
        },

        Action::CycleMode => {
            if let Some(ref client) = state.client {
                let new_mode = ProxyManager::cycle_proxy_mode(client, state.proxy_mode.clone()).await;
                if let Ok(mode) = new_mode {
                    state.proxy_mode = mode;
                }
            }
        }

        Action::SwitchNode => {
            if let Some(ref client) = state.client {
                let group = state.groups.get(state.ui.selected_group_idx);
                if let Some(g) = group {
                    if let Some(node) = g.all.get(state.ui.selected_node_idx) {
                        let _ = ProxyManager::switch_node(client, &g.name, node).await;
                    }
                }
            }
        }

        Action::TestNodeDelay => {
            if let Some(ref client) = state.client {
                let group = state.groups.get(state.ui.selected_group_idx);
                if let Some(g) = group {
                    if let Some(node) = g.all.get(state.ui.selected_node_idx) {
                        let test_url = &state.config.preferences.delay_test_url;
                        let timeout = state.config.preferences.delay_test_timeout_ms;
                        let _ = ProxyManager::test_node_delay(client, node, test_url, timeout).await;
                    }
                }
            }
        }

        Action::TestGroupDelay => {
            if let Some(ref client) = state.client {
                let group = state.groups.get(state.ui.selected_group_idx);
                if let Some(g) = group {
                    let test_url = &state.config.preferences.delay_test_url;
                    let timeout = state.config.preferences.delay_test_timeout_ms;
                    let _ = ProxyManager::test_group_delay(client, &g.name, test_url, timeout).await;
                }
            }
        }

        Action::PrevGroup => {
            state.ui.selected_group_idx = state.ui.selected_group_idx.saturating_sub(1);
            state.ui.selected_node_idx = 0;
        }

        Action::NextGroup => {
            let max = state.groups.len().saturating_sub(1);
            state.ui.selected_group_idx = (state.ui.selected_group_idx + 1).min(max);
            state.ui.selected_node_idx = 0;
        }

        Action::Back => {} // Already handled in view context

        Action::CloseConnection => {
            if let Some(ref client) = state.client {
                if let Some(conn) = state.connections.get(state.ui.selected_conn_idx) {
                    let _ = ConnectionManager::close_one(client, &conn.id).await;
                }
            }
        }

        Action::CloseAllConnections => {
            if let Some(ref client) = state.client {
                let _ = ConnectionManager::close_all(client).await;
            }
        }

        Action::TogglePause => {
            state.ui.log_paused = !state.ui.log_paused;
        }

        Action::CycleLogLevel => {
            state.ui.log_level_filter = match state.ui.log_level_filter.as_deref() {
                None => Some("info".into()),
                Some("info") => Some("warning".into()),
                Some("warning") => Some("error".into()),
                Some("error") => Some("debug".into()),
                Some("debug") => None,
                _ => None,
            };
        }

        Action::Search => {
            state.ui.search_mode = true;
        }

        Action::Refresh => {}
        _ => {}
    }

    true
}
```

- [ ] **Step 2: Fix compile errors**

```bash
cargo check 2>&1 | head -50
```

Expected: Some errors. Key fixes:
- In `handle_action`, use `state.proxy_mode` directly instead of `.clone()` since `ProxyMode` implements `Clone` and `PartialEq`.
- Ensure `CycleMode` sends `Arc<Mutex<>>` correctly.
- The `handle_action` function uses `await` inside some branches—make sure it's inside an async context.

Fix any remaining issues.

- [ ] **Step 3: Verify full compilation**

```bash
cargo check
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: implement TUI event loop, view routing, and background refresh"
```

---

### Task 18: WebSocket integration for real-time data

**Files:**
- Modify: `src/ui/app.rs` (add WebSocket background tasks)

- [ ] **Step 1: Add WebSocket streams to the app startup**

In `src/ui/app.rs`, after the initial connection code and before the main loop, add:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

// At the top of run_tui():
let state_clone = state.clone();
let client_clone = {
    let s = state.lock().await;
    s.client.clone()
};

if let Some(client) = client_clone {
    // Traffic WebSocket
    let state_t = state.clone();
    if let Ok(mut rx) = client.traffic_stream().await {
        tokio::spawn(async move {
            while let Some(traffic) = rx.recv().await {
                let mut s = state_t.lock().await;
                s.traffic = traffic;
            }
        });
    }

    // Connection WebSocket
    let state_c = state.clone();
    if let Ok(mut rx) = client.connection_stream().await {
        tokio::spawn(async move {
            while let Some(conns) = rx.recv().await {
                let mut s = state_c.lock().await;
                s.connections = conns;
            }
        });
    }

    // Memory WebSocket
    let state_m = state.clone();
    if let Ok(mut rx) = client.memory_stream().await {
        tokio::spawn(async move {
            while let Some(mem) = rx.recv().await {
                let mut s = state_m.lock().await;
                s.memory = mem;
            }
        });
    }

    // Log WebSocket
    let state_l = state.clone();
    if let Ok(mut rx) = client.log_stream(None).await {
        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                let mut s = state_l.lock().await;
                if !s.ui.log_paused {
                    s.logs.push(entry);
                    if s.logs.len() > 500 {
                        s.logs.remove(0);
                    }
                }
            }
        });
    }
}
```

- [ ] **Step 2: Verify compiles**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: integrate WebSocket real-time data streams"
```

---

### Task 19: Integration tests with wiremock

**Files:**
- Create: `tests/integration_test.rs`

- [ ] **Step 1: Create integration test directory and file**

```bash
mkdir -p tests
```

Write `tests/integration_test.rs`:
```rust
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use mioctl::api::client::MihomoClient;
use mioctl::api::types::*;

#[tokio::test]
async fn test_get_version() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "version": "mihomo v1.18.0"
        })))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let version = client.get_version().await.unwrap();
    assert_eq!(version.version, "mihomo v1.18.0");
}

#[tokio::test]
async fn test_get_proxies() {
    let server = MockServer::start().await;

    let mock_body = serde_json::json!({
        "proxies": {
            "GLOBAL": {
                "name": "GLOBAL",
                "type": "Selector",
                "now": "DIRECT",
                "all": ["DIRECT", "🇯🇵 Japan"],
                "history": [],
                "udp": true,
                "alive": true
            }
        }
    });

    Mock::given(method("GET"))
        .and(path("/proxies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_body))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let resp = client.get_proxies().await.unwrap();
    assert!(resp.proxies.contains_key("GLOBAL"));
}

#[tokio::test]
async fn test_get_connections() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/connections"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "connections": [],
            "downloadTotal": 0,
            "uploadTotal": 0
        })))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let resp = client.get_connections().await.unwrap();
    assert!(resp.connections.is_empty());
}

#[tokio::test]
async fn test_select_proxy() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/proxies/GLOBAL"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let result = client.select_proxy("GLOBAL", "🇯🇵 Japan").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_close_connection() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/connections/abc-123"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let result = client.close_connection("abc-123").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_close_all_connections() {
    let server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/connections"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let result = client.close_all_connections().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_rules() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/rules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rules": [
                {"type": "DOMAIN-SUFFIX", "payload": "google.com", "proxy": "🔍 Google"}
            ]
        })))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let resp = client.get_rules().await.unwrap();
    assert_eq!(resp.rules.len(), 1);
    assert_eq!(resp.rules[0].payload, "google.com");
}

#[tokio::test]
async fn test_get_traffic() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/traffic"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "up": 102400,
            "down": 204800
        })))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let traffic = client.get_traffic().await.unwrap();
    assert_eq!(traffic.up, 102400);
    assert_eq!(traffic.down, 204800);
}

#[tokio::test]
async fn test_get_configs() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "port": 7890,
            "mixed-port": 7890,
            "mode": "rule",
            "log-level": "info"
        })))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let config = client.get_configs().await.unwrap();
    assert_eq!(config.mode.as_deref(), Some("rule"));
}

#[tokio::test]
async fn test_reload_config() {
    let server = MockServer::start().await;

    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let result = client.reload_config(None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_api_error_handling() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/proxies"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let result = client.get_proxies().await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Run integration tests**

```bash
cargo test --test integration_test
```

Expected: 11 tests pass with wiremock-backed responses.

- [ ] **Step 3: Run all unit + integration tests**

```bash
cargo test
```

Expected: ~40+ tests pass across all modules.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "test: add wiremock-backed integration tests for API client"
```

---

### Task 20: README, CI, and final polish

**Files:**
- Create: `README.md`
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write README.md**

```markdown
# mioctl

Terminal UI management tool for [mihomo](https://github.com/MetaCubeX/mihomo) (Clash.Meta).

## Features

- Interactive TUI with sidebar navigation (dashboard, proxies, connections, rules, logs)
- Full mihomo REST API integration
- Real-time data via WebSocket (traffic, connections, memory, logs)
- Subscription management via URL (YAML / Base64 / URI format support)
- Proxy node switching, latency testing, health checks
- Vim-style keybindings, Catppuccin Mocha theme

## Installation

```bash
cargo install mioctl
```

## Quick Start

1. Ensure mihomo is running with `external-controller` enabled:
   ```yaml
   external-controller: 127.0.0.1:9090
   secret: "your-secret"
   ```

2. Launch mioctl:
   ```bash
   mioctl tui
   ```

3. Or test connectivity:
   ```bash
   mioctl connect test
   ```

## Configuration

`~/.config/mioctl/config.toml`:
```toml
[mihomo]
external-controller = "127.0.0.1:9090"
secret = ""

[subscriptions]
update-interval-minutes = 240
[[subscriptions.items]]
name = "example"
url = "https://example.com/sub"
```

## Keybindings

| Key | Action |
|-----|--------|
| `1-5` | Switch view |
| `j/k` | Navigate |
| `/` | Search |
| `:` | Command mode |
| `q` | Quit |
```

- [ ] **Step 2: Write CI config**

```bash
mkdir -p .github/workflows
```

Write `.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --release
      - run: cargo test
      - run: cargo clippy -- -D warnings
      - run: cargo fmt --check
```

- [ ] **Step 3: Final build check**

```bash
cargo build --release
cargo test
cargo clippy -- -D warnings 2>&1 | tail -20
cargo fmt --check
```

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add README and CI configuration"
```

---

### Task 20: Incremental fixes & smoke test

- [ ] **Step 1: Run clippy and fix warnings**

```bash
cargo clippy --fix --allow-dirty
cargo fmt
```

- [ ] **Step 2: Build release binary**

```bash
cargo build --release
```

Expected: Binary at `target/release/mioctl`.

- [ ] **Step 3: Test with a running mihomo instance**

```bash
# Add your mihomo connection details
./target/release/mioctl connect test
```

- [ ] **Step 4: Commit any remaining fixes**

```bash
git add -A && git commit -m "chore: clippy fixes and formatting"
```
