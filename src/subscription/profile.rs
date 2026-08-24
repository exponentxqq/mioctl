use crate::api::types::ParsedNode;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::parser::{
    decode_base64_lenient, parse_subscription_full, parse_uri_list, SubscriptionContent,
};
use serde_yaml::{Mapping, Value};

pub struct NormalizedProfile {
    pub yaml: String,
    pub node_count: usize,
    pub warnings: Vec<String>,
}

pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    sanitized.chars().take(80).collect()
}

pub fn name_conflicts(new: &str, existing: &[String]) -> bool {
    let sanitized = sanitize_filename(new);
    existing.iter().any(|e| sanitize_filename(e) == sanitized)
}

pub fn archive_path(name: &str) -> std::path::PathBuf {
    MioctlConfig::profiles_dir().join(format!("{}.yaml", sanitize_filename(name)))
}

pub fn archive_exists(name: &str) -> bool {
    archive_path(name).exists()
}

pub fn write_archive(name: &str, yaml: &str) -> Result<(), String> {
    let path = archive_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, yaml).map_err(|e| e.to_string())
}

pub fn read_archive(name: &str) -> Result<String, String> {
    std::fs::read_to_string(archive_path(name)).map_err(|e| e.to_string())
}

pub fn remove_archive(name: &str) -> Result<(), String> {
    let path = archive_path(name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn looks_like_yaml(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("proxies:")
        || t.starts_with("mixed-port:")
        || t.starts_with("port:")
        || t.starts_with("---")
}

fn from_content(sub: SubscriptionContent) -> Result<NormalizedProfile, String> {
    let node_count = sub.proxies.as_sequence().map(|s| s.len()).unwrap_or(0);
    let mut out = Mapping::new();
    out.insert(Value::String("proxies".into()), sub.proxies);
    out.insert(Value::String("proxy-groups".into()), sub.proxy_groups);
    out.insert(Value::String("rules".into()), sub.rules);
    let yaml = serde_yaml::to_string(&Value::Mapping(out)).map_err(|e| e.to_string())?;
    Ok(NormalizedProfile {
        yaml,
        node_count,
        warnings: vec![],
    })
}

fn from_nodes(
    sub_name: &str,
    nodes: &[ParsedNode],
    skipped: Vec<String>,
) -> Result<NormalizedProfile, String> {
    if nodes.is_empty() {
        return Err("no parsable nodes found in subscription".into());
    }
    let (proxies, proxy_groups, rules) = nodes_to_subscription_content(sub_name, nodes);
    let mut out = Mapping::new();
    out.insert(Value::String("proxies".into()), proxies);
    out.insert(Value::String("proxy-groups".into()), proxy_groups);
    out.insert(Value::String("rules".into()), rules);
    let yaml = serde_yaml::to_string(&Value::Mapping(out)).map_err(|e| e.to_string())?;
    let warnings = if skipped.is_empty() {
        vec![]
    } else {
        vec![format!(
            "skipped {} unsupported entries: {}",
            skipped.len(),
            skipped.join(", ")
        )]
    };
    Ok(NormalizedProfile {
        yaml,
        node_count: nodes.len(),
        warnings,
    })
}

pub fn normalize_to_yaml(sub_name: &str, content: &str) -> Result<NormalizedProfile, String> {
    if let Ok(sub) = parse_subscription_full(content) {
        return from_content(sub);
    }
    if let Some(decoded) = decode_base64_lenient(content) {
        if looks_like_yaml(&decoded) {
            if let Ok(sub) = parse_subscription_full(&decoded) {
                return from_content(sub);
            }
        }
        let (nodes, skipped) = parse_uri_list(&decoded)?;
        return from_nodes(sub_name, &nodes, skipped);
    }
    let (nodes, skipped) = parse_uri_list(content)?;
    from_nodes(sub_name, &nodes, skipped)
}

fn nodes_to_subscription_content(name: &str, nodes: &[ParsedNode]) -> (Value, Value, Value) {
    let mut proxy_entries = Vec::new();
    for node in nodes {
        let mut entry = Mapping::new();
        entry.insert(
            Value::String("name".into()),
            Value::String(node.name.clone()),
        );
        entry.insert(
            Value::String("type".into()),
            Value::String(node.node_type.clone()),
        );
        entry.insert(
            Value::String("server".into()),
            Value::String(node.server.clone()),
        );
        entry.insert(
            Value::String("port".into()),
            Value::Number(node.port.into()),
        );
        if let Some(ref c) = node.cipher {
            entry.insert(Value::String("cipher".into()), Value::String(c.clone()));
        }
        if let Some(ref p) = node.password {
            entry.insert(Value::String("password".into()), Value::String(p.clone()));
        }
        if let Some(ref u) = node.uuid {
            entry.insert(Value::String("uuid".into()), Value::String(u.clone()));
        }
        if let Some(a) = node.alter_id {
            entry.insert(Value::String("alterId".into()), Value::Number(a.into()));
        }
        if let Some(ref n) = node.network {
            entry.insert(Value::String("network".into()), Value::String(n.clone()));
        }
        if let Some(ref w) = node.ws_opts {
            entry.insert(
                Value::String("ws-opts".into()),
                serde_yaml::to_value(w).unwrap_or_default(),
            );
        }
        if let Some(ref s) = node.sni {
            entry.insert(Value::String("sni".into()), Value::String(s.clone()));
        }
        if let Some(s) = node.skip_cert_verify {
            entry.insert(Value::String("skip-cert-verify".into()), Value::Bool(s));
        }
        if let Some(u) = node.udp {
            entry.insert(Value::String("udp".into()), Value::Bool(u));
        }
        proxy_entries.push(Value::Mapping(entry));
    }
    let proxies = Value::Sequence(proxy_entries);
    let mut group = Mapping::new();
    group.insert(
        Value::String("name".into()),
        Value::String(name.to_string()),
    );
    group.insert(Value::String("type".into()), Value::String("select".into()));
    let node_names: Vec<Value> = nodes
        .iter()
        .map(|n| Value::String(n.name.clone()))
        .collect();
    group.insert(Value::String("proxies".into()), Value::Sequence(node_names));
    let proxy_groups = Value::Sequence(vec![Value::Mapping(group)]);
    let rules = Value::Sequence(vec![Value::String(format!("MATCH,{}", name))]);
    (proxies, proxy_groups, rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        crate::testutil::env_lock().lock().unwrap()
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("狗狗加速.com"), "狗狗加速.com");
        assert_eq!(sanitize_filename("a/b\\c"), "a_b\\c");
        assert_eq!(sanitize_filename("x\u{0000}y"), "x_y");
    }

    #[test]
    fn test_sanitize_filename_truncates_long_names() {
        let long = "x".repeat(300);
        assert!(sanitize_filename(&long).chars().count() <= 80);
    }

    #[test]
    fn test_name_conflicts_across_sanitization() {
        let existing = vec!["a/b".to_string()];
        assert!(name_conflicts("a_b", &existing));
        assert!(name_conflicts("a/b", &existing));
        assert!(!name_conflicts("other", &existing));
    }

    #[test]
    fn test_normalize_yaml_passthrough_tuic() {
        let content = r#"proxies:
  - name: "🇨🇦26加拿大(Tuic)"
    type: tuic
    server: 26ca.example.com
    port: 4430
    uuid: 03523c1e
    password: 03523c1e
    congestion-control: bbr
    alpn: [h3]
proxy-groups:
  - name: G
    type: select
    proxies: ["🇨🇦26加拿大(Tuic)"]
rules:
  - MATCH,G
"#;
        let p = normalize_to_yaml("test", content).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.warnings.is_empty());
        assert!(p.yaml.contains("type: tuic"));
        assert!(p.yaml.contains("congestion-control: bbr"));
    }

    #[test]
    fn test_normalize_unpadded_base64_uri_list() {
        let uri_list = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\n";
        let mut b64 = base64::engine::general_purpose::STANDARD.encode(uri_list);
        while b64.ends_with('=') {
            b64.pop();
        }
        let p = normalize_to_yaml("unpadded", &b64).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.yaml.contains("name: N1"));
    }

    #[test]
    fn test_normalize_base64_uri_list() {
        let uri_list = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\nvless://uuid@host:443?x=1#Bad\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(uri_list);
        let p = normalize_to_yaml("mysub", &b64).unwrap();
        assert_eq!(p.node_count, 1);
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("vless"));
        assert!(p.yaml.contains("name: N1"));
        assert!(p.yaml.contains("name: mysub"));
        assert!(p.yaml.contains("MATCH,mysub"));
    }

    #[test]
    fn test_normalize_base64_embedded_yaml() {
        let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388, cipher: aes-256-gcm, password: p }\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
        let p = normalize_to_yaml("t", &b64).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.yaml.contains("name: N1"));
    }

    #[test]
    fn test_normalize_plain_uri_list() {
        let content = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1";
        let p = normalize_to_yaml("plain", content).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.yaml.contains("name: N1"));
    }

    #[test]
    fn test_normalize_invalid_utf8_base64() {
        let content = base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe]);
        assert!(normalize_to_yaml("invalid", &content).is_err());
    }

    #[test]
    fn test_normalize_base64_yaml_without_proxies() {
        let content = base64::engine::general_purpose::STANDARD.encode("port: 123");
        assert!(normalize_to_yaml("invalid", &content).is_err());
    }

    #[test]
    fn test_normalize_empty_nodes() {
        match normalize_to_yaml("empty", "") {
            Err(error) => assert_eq!(error, "no parsable nodes found in subscription"),
            Ok(_) => panic!("expected empty nodes to fail"),
        }
    }

    #[test]
    fn test_archive_roundtrip() {
        let _guard = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("MIOCTL_HOME", dir.path());
        write_archive("测试 sub", "proxies: []").unwrap();
        assert!(archive_exists("测试 sub"));
        assert_eq!(read_archive("测试 sub").unwrap(), "proxies: []");
        remove_archive("测试 sub").unwrap();
        assert!(!archive_exists("测试 sub"));
        assert!(read_archive("测试 sub").is_err());
        std::env::remove_var("MIOCTL_HOME");
    }

    #[test]
    fn test_archive_path_and_remove_missing() {
        let _guard = lock_env();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("MIOCTL_HOME", dir.path());
        assert_eq!(archive_path("a/b"), dir.path().join("profiles/a_b.yaml"));
        remove_archive("missing").unwrap();
        std::env::remove_var("MIOCTL_HOME");
    }

    #[test]
    fn test_write_archive_error() {
        let _guard = lock_env();
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("file");
        std::fs::write(&home, "not a directory").unwrap();
        std::env::set_var("MIOCTL_HOME", &home);
        assert!(write_archive("x", "proxies: []").is_err());
        std::env::remove_var("MIOCTL_HOME");
    }

    #[test]
    fn test_normalize_invalid_content() {
        assert!(normalize_to_yaml("x", "unsupported").is_err());
    }

    #[test]
    fn test_from_content_without_sequence() {
        let profile = from_content(SubscriptionContent {
            proxies: Value::String("invalid".into()),
            proxy_groups: Value::Null,
            rules: Value::Null,
        })
        .unwrap();
        assert_eq!(profile.node_count, 0);
    }

    #[test]
    fn test_from_nodes_with_all_optional_fields() {
        let node = ParsedNode {
            name: "N".into(),
            node_type: "vmess".into(),
            server: "host".into(),
            port: 443,
            cipher: Some("auto".into()),
            password: Some("pass".into()),
            uuid: Some("uuid".into()),
            alter_id: Some(1),
            network: Some("ws".into()),
            ws_opts: Some(serde_json::json!({"path": "/"})),
            sni: Some("sni".into()),
            skip_cert_verify: Some(true),
            udp: Some(true),
        };
        let profile = from_nodes("group", &[node], vec![]).unwrap();
        assert_eq!(profile.node_count, 1);
        assert!(profile.warnings.is_empty());
    }
}
