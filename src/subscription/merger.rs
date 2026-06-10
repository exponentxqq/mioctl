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
    let existing_yaml = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut config: Mapping = if existing_yaml.trim().is_empty() {
        serde_yaml::from_str(DEFAULT_TEMPLATE).map_err(|e| format!("template error: {}", e))?
    } else {
        let val: Value = serde_yaml::from_str(&existing_yaml)
            .map_err(|e| format!("config YAML parse error: {}", e))?;
        val.as_mapping().cloned().unwrap_or_default()
    };

    config.remove("proxy-providers");

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

pub fn backup_file(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(path, format!("{}.bak", path))
        .map_err(|e| format!("backup failed: {}", e))?;
    Ok(())
}

pub fn rollback_file(path: &str) -> Result<(), String> {
    let bak = format!("{}.bak", path);
    let p = std::path::Path::new(&bak);
    if !p.exists() {
        return Ok(());
    }
    std::fs::copy(&bak, path).map_err(|e| format!("rollback failed: {}", e))?;
    Ok(())
}

pub fn write_config(path: &str, yaml: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, yaml).map_err(|e| e.to_string())
}

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

        let full: Value = serde_yaml::from_str("proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G").unwrap();
        let proxies = full.get("proxies").unwrap();
        let proxy_groups = full.get("proxy-groups").unwrap();
        let rules = full.get("rules").unwrap();

        let result = merge_mihomo_config(
            path.to_str().unwrap(),
            proxies, proxy_groups, rules,
        ).unwrap();

        assert!(result.yaml.contains("mixed-port: 7897"));
        assert!(result.yaml.contains("dns:"));
        assert!(result.yaml.contains("name: N1"));
        assert!(result.yaml.contains("name: G"));
        assert!(result.yaml.contains("MATCH,G"));
        assert!(!result.yaml.contains("proxy-providers"));
        assert_eq!(result.proxy_count, 1);
        assert_eq!(result.group_count, 1);
        assert_eq!(result.rule_count, 1);
    }

    #[test]
    fn test_merge_with_default_template_when_no_config() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.yaml");

        let proxies = Value::Sequence(vec![]);
        let proxy_groups = Value::Sequence(vec![]);
        let rules = Value::Sequence(vec![]);

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

        backup_file(path.to_str().unwrap()).unwrap();
        assert!(dir.path().join("config.yaml.bak").exists());

        std::fs::write(&path, "modified").unwrap();

        rollback_file(path.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original content");
    }

    #[test]
    fn test_write_config_creates_parent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("subdir").join("config.yaml");
        write_config(path.to_str().unwrap(), "test content").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "test content");
    }
}
