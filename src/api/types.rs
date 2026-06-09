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

// === Logs ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    #[serde(rename = "type")]
    pub level: String,
    pub payload: String,
}

// === Subscription Node ===

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
        assert_eq!(resp.connections[0].id, "abc-123");
        assert_eq!(resp.connections[0].metadata.host, "google.com");
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
        let json = r#"{"type": "info", "payload": "new connection"}"#;
        let entry: LogEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.level, "info");
    }

    #[test]
    fn test_extract_groups_from_proxies() {
        let json = r#"{
            "proxies": {
                "GLOBAL": {
                    "name": "GLOBAL",
                    "type": "Selector",
                    "now": "DIRECT",
                    "all": ["Node-A", "DIRECT"],
                    "history": [],
                    "udp": false,
                    "alive": true
                },
                "Node-A": {
                    "name": "Node-A",
                    "type": "Shadowsocks",
                    "now": null,
                    "all": [],
                    "history": [{"time": "2026-06-08T12:00:00Z", "delay": 45}],
                    "udp": true,
                    "alive": true
                },
                "Auto": {
                    "name": "Auto",
                    "type": "URLTest",
                    "now": "Node-A",
                    "all": ["Node-A", "Node-B"],
                    "history": [],
                    "udp": false,
                    "alive": true
                }
            }
        }"#;
        let resp: ProxiesResponse = serde_json::from_str(json).unwrap();
        let groups = crate::api::client::MihomoClient::extract_groups(&resp);
        // Only GLOBAL and Auto have non-empty .all
        assert_eq!(groups.len(), 2);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert!(names.contains(&"GLOBAL"));
        assert!(names.contains(&"Auto"));
        assert!(!names.contains(&"Node-A"));

        let global = groups.iter().find(|g| g.name == "GLOBAL").unwrap();
        assert_eq!(global.group_type, "Selector");
        assert_eq!(global.now.as_deref(), Some("DIRECT"));
        assert_eq!(global.all.len(), 2);
    }
}
