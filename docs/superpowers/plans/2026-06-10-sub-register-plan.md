# mioctl sub register — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `mioctl sub register <url>` — single-command subscription import: fetch, auto-detect UA, parse full YAML (proxies + groups + rules), smart-merge into mihomo config.yaml, reload mihomo.

**Architecture:** Extend the existing subscription pipeline (fetcher → parser → manager) with auto-UA probing, full-YAML parsing (not just proxies), and a new merger module that reads/writes mihomo's config.yaml using serde_yaml::Value mapping manipulation.

**Tech Stack:** Rust, reqwest, serde_yaml, serde_json, tom, clap, tokio

---

### Task 1: Add CLI variant and handler skeleton

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/cli/sub.rs`

- [ ] **Step 1: Add `Register` variant to `SubAction`**

In `src/cli/mod.rs`, add the new variant inside `SubAction`:

```rust
#[derive(Subcommand)]
pub enum SubAction {
    /// Update all subscriptions
    Update {
        #[arg(long)]
        all: bool,
    },
    /// Register a new subscription
    Register {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after registration
        #[arg(long)]
        no_reload: bool,
    },
}
```

- [ ] **Step 2: Add `run_register` handler skeleton in `src/cli/sub.rs`**

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
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret.clone())
            };
            match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match SubscriptionManager::update_all(&mut config, &c).await {
                    Ok(result) => println!("{}", result),
                    Err(e) => eprintln!("Error: {}", e),
                },
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
        SubAction::Register { url, name, no_reload } => {
            run_register(url, name, no_reload).await;
        }
    }
}

async fn run_register(url: String, name: Option<String>, no_reload: bool) {
    println!("Registering subscription...");
    // TODO: implement in subsequent tasks
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check`
Expected: compiles, unused import warnings for `run_register` params are fine

- [ ] **Step 4: Commit**

```bash
git add src/cli/mod.rs src/cli/sub.rs
git commit -m "feat(cli): add sub register command skeleton"
```

---

### Task 2: Add UA auto-probe to fetcher

**Files:**
- Modify: `src/subscription/fetcher.rs`

- [ ] **Step 1: Add `fetch_with_ua_probe` function**

Replace `src/subscription/fetcher.rs` with:

```rust
use reqwest::Client;

const UA_CANDIDATES: &[&str] = &[
    "mihomo/{version}",
    "ClashMeta/1.19.0",
    "clash-verge/1.3.8",
];

/// Fetch subscription content, trying multiple User-Agents in order.
/// First response that contains >= 3 valid proxy entries wins.
/// The mihomo/{version} UA requires the current mihomo version string.
pub async fn fetch_with_ua_probe(
    url: &str,
    mihomo_version: Option<String>,
) -> Result<String, String> {
    for &ua_template in UA_CANDIDATES {
        let ua = if ua_template.contains("{version}") {
            match &mihomo_version {
                Some(v) => ua_template.replace("{version}", v),
                None => continue,
            }
        } else {
            ua_template.to_string()
        };

        match try_fetch(url, &ua).await {
            Ok(body) => {
                if count_proxy_entries(&body) >= 3 {
                    return Ok(body);
                }
            }
            Err(_) => continue,
        }
    }
    Err("all User-Agent probes failed — subscription requires a different client identity".into())
}

async fn try_fetch(url: &str, user_agent: &str) -> Result<String, String> {
    let client = Client::builder()
        .user_agent(user_agent)
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Count how many proxy entries the body contains (cheap heuristic).
fn count_proxy_entries(body: &str) -> usize {
    body.lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("- ") || t.starts_with("- {") || t.starts_with("ss://") || t.starts_with("vmess://") || t.starts_with("trojan://")
        })
        .count()
}

/// Legacy single-UA fetch, kept for backwards compatibility with update_all.
pub async fn fetch_subscription(url: &str) -> Result<String, String> {
    try_fetch(url, "clash-verge/1.3.8").await
}
```

- [ ] **Step 2: Check compilation**

Run: `cargo check`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/subscription/fetcher.rs
git commit -m "feat(fetcher): add UA auto-probe with multiple User-Agents"
```

---

### Task 3: Add full subscription parsing

**Files:**
- Modify: `src/subscription/parser.rs`

- [ ] **Step 1: Add `SubscriptionContent` struct and `parse_subscription_full()`**

Append to `src/subscription/parser.rs`:

```rust
/// Full parsed subscription content — proxies, proxy-groups, and rules.
/// Preserved as serde_yaml Values so merger can write them back verbatim.
pub struct SubscriptionContent {
    pub proxies: serde_yaml::Value,
    pub proxy_groups: serde_yaml::Value,
    pub rules: serde_yaml::Value,
}

pub fn parse_subscription_full(content: &str) -> Result<SubscriptionContent, String> {
    let yaml_val: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    let mapping = yaml_val.as_mapping()
        .ok_or_else(|| "subscription content is not a YAML mapping".to_string())?;

    let proxies = mapping.get("proxies").cloned().unwrap_or(serde_yaml::Value::Sequence(vec![]));
    let proxy_groups = mapping.get("proxy-groups").cloned().unwrap_or(serde_yaml::Value::Sequence(vec![]));
    let rules = mapping.get("rules").cloned().unwrap_or(serde_yaml::Value::Sequence(vec![]));

    // Validate proxies is a non-empty sequence
    match &proxies {
        serde_yaml::Value::Sequence(s) if !s.is_empty() => {}
        _ => return Err("no proxies found in subscription".to_string()),
    }

    Ok(SubscriptionContent { proxies, proxy_groups, rules })
}

/// Auto-detect a name from subscription content.
/// Priority: first proxy-group name → first non-empty proxy-group name
pub fn detect_subscription_name(content: &str) -> Result<String, String> {
    let yaml_val: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|_| "YAML parse error".to_string())?;

    let mapping = yaml_val.as_mapping()
        .ok_or_else(|| "not a YAML mapping".to_string())?;

    if let Some(groups) = mapping.get("proxy-groups") {
        if let Some(seq) = groups.as_sequence() {
            for item in seq {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    return Ok(name.to_string());
                }
            }
        }
    }
    Err("could not detect name from subscription".into())
}

/// Detect name from a URL's hostname portion.
pub fn name_from_url(url: &str) -> Result<String, String> {
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 2 {
        Ok(parts[parts.len() - 2].to_string())
    } else {
        Ok(host.to_string())
    }
}
```

- [ ] **Step 2: Check compilation**

Run: `cargo check`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/subscription/parser.rs
git commit -m "feat(parser): add parse_subscription_full and name detection"
```

---

### Task 4: Create YAML merger module

**Files:**
- Create: `src/subscription/merger.rs`
- Modify: `src/subscription/mod.rs`

- [ ] **Step 1: Write `merger.rs`**

Create `src/subscription/merger.rs`:

```rust
use serde_yaml::{self, Value, Mapping};

/// Keys to preserve in the mihomo config during merge (infrastructure config).
const PRESERVE_KEYS: &[&str] = &[
    "mixed-port", "external-controller", "mode", "log-level", "allow-lan",
    "dns", "tun", "sniffer", "ipv6", "profile", "hosts", "interface-name",
    "routing-mark", "bind-address", "authentication", "tcp-concurrent",
    "geodata-mode", "geox-url", "unified-delay", "keep-alive-interval",
    "port", "socks-port", "redir-port", "tproxy-port", "find-process-mode",
];

/// Default template for new config.yaml when none exists.
const DEFAULT_TEMPLATE: &str = r#"mixed-port: 7897
external-controller: 127.0.0.1:9090
mode: rule
log-level: info
allow-lan: false
dns:
  enable: true
  enhanced-mode: redir-host
  nameserver:
    - 223.5.5.5
    - 119.29.29.29
  fallback:
    - tls://1.1.1.1:853
    - tls://8.8.8.8:853
  fallback-filter:
    geoip: true
    geoip-code: CN
tun:
  enable: true
  stack: gvisor
  auto-route: true
  auto-detect-interface: true
sniffer:
  enable: true
  sniffing:
    - tls
    - http
"#;

pub struct MergerResult {
    pub yaml: String,
    pub proxy_count: usize,
    pub group_count: usize,
    pub rule_count: usize,
}

/// Merge subscription content into a mihomo config.yaml.
///
/// Reads the existing config (or uses default template), preserves infrastructure
/// keys, replaces proxies/proxy-groups/rules with subscription content, removes
/// proxy-providers.
pub fn merge_mihomo_config(
    config_path: &str,
    proxies: &Value,
    proxy_groups: &Value,
    rules: &Value,
) -> Result<MergerResult, String> {
    // Load existing config or use template
    let existing_yaml = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut config: Mapping = if existing_yaml.trim().is_empty() {
        serde_yaml::from_str(DEFAULT_TEMPLATE).map_err(|e| format!("template error: {}", e))?
    } else {
        let val: Value = serde_yaml::from_str(&existing_yaml)
            .map_err(|e| format!("config YAML parse error: {}", e))?;
        val.as_mapping().cloned().unwrap_or_default()
    };

    // Remove proxy-providers (no longer needed with inline proxies)
    config.remove("proxy-providers");

    // Inject subscription content
    config.insert(
        Value::String("proxies".into()),
        proxies.clone(),
    );
    config.insert(
        Value::String("proxy-groups".into()),
        proxy_groups.clone(),
    );
    config.insert(
        Value::String("rules".into()),
        rules.clone(),
    );

    // Clean up any keys that aren't in the preserve list or our new keys
    // (keeps the config minimal)
    let known_keys: std::collections::HashSet<&str> = PRESERVE_KEYS
        .iter()
        .chain(&["proxies", "proxy-groups", "rules"])
        .copied()
        .collect();

    // Re-build mapping in preferred key order: infrastructure first, then proxies/groups/rules
    let mut ordered = Mapping::new();
    for &key in PRESERVE_KEYS {
        if let Some(v) = config.remove(key) {
            ordered.insert(Value::String(key.into()), v);
        }
    }
    for key in &["proxies", "proxy-groups", "rules"] {
        if let Some(v) = config.remove(*key) {
            ordered.insert(Value::String(key.to_string()), v);
        }
    }

    // Serialize back
    let yaml = serde_yaml::to_string(&Value::Mapping(ordered))
        .map_err(|e| format!("serialization error: {}", e))?;

    let proxy_count = count_sequence(proxies);
    let group_count = count_sequence(proxy_groups);
    let rule_count = count_sequence(rules);

    Ok(MergerResult {
        yaml,
        proxy_count,
        group_count,
        rule_count,
    })
}

fn count_sequence(val: &Value) -> usize {
    val.as_sequence().map(|s| s.len()).unwrap_or(0)
}

/// Back up a file by copying it to `<path>.bak`.
pub fn backup_file(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(path, format!("{}.bak", path))
        .map_err(|e| format!("backup failed: {}", e))?;
    Ok(())
}

/// Rollback from backup, returning the backup to original.
pub fn rollback_file(path: &str) -> Result<(), String> {
    let bak = format!("{}.bak", path);
    let p = std::path::Path::new(&bak);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(&bak, path).map_err(|e| format!("rollback failed: {}", e))?;
    Ok(())
}

/// Write config YAML to mihomo config path.
pub fn write_config(path: &str, yaml: &str) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, yaml).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Update `src/subscription/mod.rs`**

```rust
pub mod fetcher;
pub mod injector;
pub mod manager;
pub mod merger;
pub mod parser;
```

- [ ] **Step 3: Check compilation**

Run: `cargo check`
Expected: compiles

- [ ] **Step 4: Write unit tests in merger.rs**

Add at the end of `src/subscription/merger.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_merge_into_existing_config() {
        let existing = r#"mixed-port: 7897
external-controller: 127.0.0.1:9090
mode: rule
dns:
  enable: true
  nameserver: [8.8.8.8]
"#;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, existing).unwrap();

        let proxies = serde_yaml::from_str("proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443").unwrap();
        let proxy_groups = serde_yaml::from_str("proxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]").unwrap();
        let rules = serde_yaml::from_str("rules:\n  - MATCH,G").unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies.get("proxies").unwrap(),
            &proxy_groups.get("proxy-groups").unwrap(),
            &rules.get("rules").unwrap(),
        ).unwrap();

        // Infrastructure preserved
        assert!(result.yaml.contains("mixed-port: 7897"));
        assert!(result.yaml.contains("dns:"));
        assert!(result.yaml.contains("enable: true"));
        // Subscription injected
        assert!(result.yaml.contains("name: N1"));
        assert!(result.yaml.contains("name: G"));
        assert!(result.yaml.contains("MATCH,G"));
        // proxy-providers removed
        assert!(!result.yaml.contains("proxy-providers"));
        // Counts
        assert_eq!(result.proxy_count, 1);
        assert_eq!(result.group_count, 1);
        assert_eq!(result.rule_count, 1);
    }

    #[test]
    fn test_merge_with_default_template_when_no_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.yaml");

        let proxies = serde_yaml::Value::Sequence(vec![]);
        let proxy_groups = serde_yaml::Value::Sequence(vec![]);
        let rules = serde_yaml::Value::Sequence(vec![]);

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            &proxies, &proxy_groups, &rules,
        ).unwrap();

        assert!(result.yaml.contains("mixed-port: 7897"));
        assert!(result.yaml.contains("gvisor"));
        assert!(result.yaml.contains("redir-host"));
    }

    #[test]
    fn test_backup_and_rollback() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "original content").unwrap();

        // Backup
        backup_file(path.to_str().unwrap()).unwrap();
        assert!(dir.path().join("config.yaml.bak").exists());

        // Modify
        std::fs::write(&path, "modified").unwrap();

        // Rollback
        rollback_file(path.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original content");
    }

    #[test]
    fn test_name_from_url() {
        assert_eq!(name_from_url("https://xWjXVnD.doggygosubs.com:8443/api/v1/client/abc").unwrap(), "doggygosubs");
        assert_eq!(name_from_url("https://sub.example.com/link/abc").unwrap(), "example");
    }
}
```

Run: `cargo test subscription::merger::tests`
Expected: 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/subscription/merger.rs src/subscription/mod.rs
git commit -m "feat(merger): add YAML config merge, backup/rollback, and tests"
```

---

### Task 5: Add `register()` to SubscriptionManager

**Files:**
- Modify: `src/subscription/manager.rs`

- [ ] **Step 1: Add `register()` method and supporting logic**

Replace `src/subscription/manager.rs`:

```rust
use crate::api::client::MihomoClient;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::fetcher::{fetch_subscription, fetch_with_ua_probe};
use crate::subscription::parser::{
    detect_format, detect_subscription_name, name_from_url,
    parse_base64, parse_subscription_full, parse_uri_list, SubscriptionFormat,
};
use crate::subscription::merger::{backup_file, merge_mihomo_config, rollback_file, write_config};

pub struct SubscriptionManager;

impl SubscriptionManager {
    /// Register a new subscription: fetch, parse, merge, reload.
    pub async fn register(
        config: &mut MioctlConfig,
        url: String,
        name: Option<String>,
        no_reload: bool,
    ) -> Result<String, String> {
        let mihomo_version = match MihomoClient::new(&config.mihomo.external_controller, None) {
            Ok(c) => c.get_version().await.ok().map(|v| v.version),
            Err(_) => None,
        };

        // 1. Fetch with UA auto-probe
        let content = fetch_with_ua_probe(&url, mihomo_version).await?;

        // 2. Auto-detect name
        let sub_name = match name {
            Some(n) => n,
            None => detect_subscription_name(&content)
                .or_else(|_| name_from_url(&url))?,
        };

        // 3. Deal with existing subscription of the same name
        if config.subscriptions.items.iter().any(|s| s.name == sub_name) {
            return Err(format!(
                "subscription '{}' already exists. Remove it first or use a different --name.",
                sub_name
            ));
        }

        // 4. Parse the subscription content
        let format = detect_format(&content);
        let sub = match format {
            SubscriptionFormat::Yaml => parse_subscription_full(&content)?,
            SubscriptionFormat::Base64 => {
                let nodes = parse_base64(&content)?;
                let (proxies, proxy_groups, rules) = nodes_to_subscription_content(&sub_name, &nodes);
                SubscriptionContent { proxies, proxy_groups, rules }
            }
            SubscriptionFormat::PlainUri => {
                let nodes = parse_uri_list(&content)?;
                let (proxies, proxy_groups, rules) = nodes_to_subscription_content(&sub_name, &nodes);
                SubscriptionContent { proxies, proxy_groups, rules }
            }
        };

        // 5. Merge into mihomo config
        let config_path = config.mihomo.config_path.clone();
        backup_file(&config_path)?;

        let result = merge_mihomo_config(
            &config_path,
            &sub.proxies,
            &sub.proxy_groups,
            &sub.rules,
        )?;

        write_config(&config_path, &result.yaml)?;

        // 6. Add to mioctl config
        config.add_subscription(sub_name.clone(), url.clone());

        // 7. Save mioctl config
        config.save().map_err(|e| {
            rollback_file(&config_path).ok();
            format!("failed to save mioctl config: {}", e)
        })?;

        // 8. Reload mihomo
        let mut reload_msg = String::new();
        if !no_reload {
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret.clone())
            };
            match MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => {
                    match c.reload_config(None).await {
                        Ok(_) => reload_msg = "mihomo reloaded successfully".into(),
                        Err(e) => {
                            reload_msg = format!(
                                "mihomo API reload failed: {}. Trying systemctl fallback...", e
                            );
                            let status = std::process::Command::new("systemctl")
                                .args(["--user", "restart", "mihomo"])
                                .output();
                            match status {
                                Ok(o) if o.status.success() => {
                                    reload_msg.push_str(" systemctl restart succeeded.")
                                }
                                _ => reload_msg.push_str(
                                    " systemctl restart also failed. Run: systemctl --user restart mihomo"
                                ),
                            }
                        }
                    }
                }
                Err(e) => {
                    reload_msg = format!("could not connect to mihomo for reload: {}", e);
                }
            }
        } else {
            reload_msg = "reload skipped (--no-reload)".into();
        }

        // 9. Build summary
        let summary = format!(
            "Subscription '{}' registered successfully.\n  {} proxies, {} groups, {} rules\n  config: {}\n  {}",
            sub_name, result.proxy_count, result.group_count, result.rule_count,
            config_path, reload_msg,
        );

        Ok(summary)
    }

    /// Legacy: update all existing subscriptions (unchanged logic).
    pub async fn update_all(
        config: &mut MioctlConfig,
        client: &MihomoClient,
    ) -> Result<String, String> {
        let items: Vec<_> = config.subscriptions.items.to_vec();
        let mut results = Vec::new();
        let now = chrono::Utc::now().to_rfc3339();
        for item in &items {
            match Self::update_one(item.name.clone(), &item.url, client).await {
                Ok(count) => {
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
        name: String, url: &str, client: &MihomoClient,
    ) -> Result<usize, String> {
        let content = fetch_subscription(url).await?;
        let nodes = match detect_format(&content) {
            SubscriptionFormat::Yaml => crate::subscription::parser::parse_yaml(&content)?,
            SubscriptionFormat::Base64 => parse_base64(&content)?,
            SubscriptionFormat::PlainUri => parse_uri_list(&content)?,
        };
        if nodes.is_empty() {
            return Err("no nodes found in subscription".into());
        }
        crate::subscription::injector::write_provider_file(&name, &nodes)?;
        crate::subscription::injector::inject_provider(client, &name).await?;
        Ok(nodes.len())
    }
}

use crate::subscription::parser::SubscriptionContent;

/// Convert parsed nodes (from Base64/PlainUri formats) into a SubscriptionContent.
/// Generates a single proxy-group named after the subscription with all nodes,
/// and a default MATCH rule.
fn nodes_to_subscription_content(
    name: &str,
    nodes: &[crate::api::types::ParsedNode],
) -> (serde_yaml::Value, serde_yaml::Value, serde_yaml::Value) {
    use serde_yaml::{Value, Mapping};

    let mut proxy_entries = Vec::new();
    for node in nodes {
        let mut entry = Mapping::new();
        entry.insert(Value::String("name".into()), Value::String(node.name.clone()));
        entry.insert(Value::String("type".into()), Value::String(node.node_type.clone()));
        entry.insert(Value::String("server".into()), Value::String(node.server.clone()));
        entry.insert(Value::String("port".into()), Value::Number(node.port.into()));
        if let Some(ref c) = node.cipher { entry.insert(Value::String("cipher".into()), Value::String(c.clone())); }
        if let Some(ref p) = node.password { entry.insert(Value::String("password".into()), Value::String(p.clone())); }
        if let Some(ref u) = node.uuid { entry.insert(Value::String("uuid".into()), Value::String(u.clone())); }
        if let Some(a) = node.alter_id { entry.insert(Value::String("alterId".into()), Value::Number(a.into())); }
        if let Some(ref n) = node.network { entry.insert(Value::String("network".into()), Value::String(n.clone())); }
        if let Some(ref w) = node.ws_opts {
            entry.insert(Value::String("ws-opts".into()), serde_yaml::to_value(w).unwrap_or_default());
        }
        if let Some(ref s) = node.sni { entry.insert(Value::String("sni".into()), Value::String(s.clone())); }
        if let Some(s) = node.skip_cert_verify { entry.insert(Value::String("skip-cert-verify".into()), Value::Bool(s)); }
        if let Some(u) = node.udp { entry.insert(Value::String("udp".into()), Value::Bool(u)); }
        proxy_entries.push(Value::Mapping(entry));
    }

    let proxies = Value::Sequence(proxy_entries);

    let mut group = Mapping::new();
    group.insert(Value::String("name".into()), Value::String(name.clone()));
    group.insert(Value::String("type".into()), Value::String("select".into()));
    let node_names: Vec<Value> = nodes.iter().map(|n| Value::String(n.name.clone())).collect();
    group.insert(Value::String("proxies".into()), Value::Sequence(node_names));
    let proxy_groups = Value::Sequence(vec![Value::Mapping(group)]);

    let rules = Value::Sequence(vec![Value::String(format!("MATCH,{}", name))]);

    (proxies, proxy_groups, rules)
}
```

- [ ] **Step 2: Check compilation**

Run: `cargo check`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add src/subscription/manager.rs
git commit -m "feat(manager): add register() with full subscription pipeline"
```

---

### Task 6: Wire up CLI handler

**Files:**
- Modify: `src/cli/sub.rs`

- [ ] **Step 1: Complete `run_register`**

Replace the contents of `src/cli/sub.rs`:

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
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret.clone())
            };
            match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match SubscriptionManager::update_all(&mut config, &c).await {
                    Ok(result) => println!("{}", result),
                    Err(e) => eprintln!("Error: {}", e),
                },
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
        SubAction::Register { url, name, no_reload } => {
            let mut config = MioctlConfig::load();
            match SubscriptionManager::register(&mut config, url, name, no_reload).await {
                Ok(summary) => println!("{}", summary),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}
```

- [ ] **Step 2: Check compilation**

Run: `cargo check`
Expected: no warnings

- [ ] **Step 3: Commit**

```bash
git add src/cli/sub.rs
git commit -m "feat(cli): wire up sub register handler"
```

---

### Task 7: End-to-end build and test

**Files:** None new

- [ ] **Step 1: Build release**

```bash
cargo build --release
```
Expected: builds cleanly

- [ ] **Step 2: Run all existing tests**

```bash
cargo test
```
Expected: all tests pass (including the new merger tests)

- [ ] **Step 3: Verify CLI help**

```bash
cargo run -- sub --help
```
Expected: shows `register` command with `url`, `--name`, `--no-reload`

- [ ] **Step 4: Lint check**

```bash
cargo clippy -- -D warnings
```
Expected: no warnings

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: finalize sub register feature, all tests pass"
```

---

### Files changed summary

| File | Action |
|------|--------|
| `src/cli/mod.rs` | Add `SubAction::Register` variant |
| `src/cli/sub.rs` | Add `run_register` dispatch |
| `src/subscription/fetcher.rs` | Add `fetch_with_ua_probe`, keep `fetch_subscription` |
| `src/subscription/parser.rs` | Add `SubscriptionContent`, `parse_subscription_full`, `detect_subscription_name`, `name_from_url` |
| `src/subscription/merger.rs` | **NEW** — YAML merge, backup/rollback, write_config |
| `src/subscription/manager.rs` | Add `register()` method, `nodes_to_subscription_content` helper |
| `src/subscription/mod.rs` | Add `pub mod merger` |
