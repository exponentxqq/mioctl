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
    if trimmed.starts_with("proxies:")
        || trimmed.starts_with("mixed-port:")
        || trimmed.starts_with("port:")
        || trimmed.starts_with("---")
    {
        return SubscriptionFormat::Yaml;
    }
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
    SubscriptionFormat::Yaml
}

pub fn parse_yaml(content: &str) -> Result<Vec<ParsedNode>, String> {
    let yaml_val: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    // Extract proxies array from top-level config
    let proxies_arr = match yaml_val.get("proxies") {
        Some(seq) => seq.as_sequence().cloned(),
        None => yaml_val.as_sequence().cloned(),
    };

    let items = match proxies_arr {
        Some(arr) => arr,
        None => {
            // Try old format
            #[derive(serde::Deserialize)]
            struct YamlProxies {
                proxies: Option<Vec<serde_json::Value>>,
            }
            let config: YamlProxies =
                serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;
            return Ok(config
                .proxies
                .unwrap_or_default()
                .iter()
                .filter_map(parse_proxy_value)
                .collect());
        }
    };

    let nodes: Vec<ParsedNode> = items
        .iter()
        .filter_map(|v| parse_proxy_value(&serde_json::to_value(v).unwrap_or_default()))
        .collect();
    Ok(nodes)
}

fn parse_proxy_value(val: &serde_json::Value) -> Option<ParsedNode> {
    let obj = val.as_object()?;
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let node_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let server = obj
        .get("server")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    // Port can be string or number in YAML
    let port = obj
        .get("port")
        .and_then(|v| {
            if let Some(n) = v.as_u64() {
                return Some(n as u16);
            }
            if let Some(s) = v.as_str() {
                return s.parse().ok();
            }
            None
        })
        .unwrap_or(443);
    let cipher = obj
        .get("cipher")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let password = obj
        .get("password")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let uuid = obj
        .get("uuid")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let alter_id = obj
        .get("alterId")
        .and_then(|v| v.as_u64())
        .map(|n| n as u16);
    let network = obj
        .get("network")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let ws_opts = obj.get("ws-opts").cloned();
    let sni = obj
        .get("sni")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let skip_cert_verify = obj.get("skip-cert-verify").and_then(|v| v.as_bool());
    let udp = obj.get("udp").and_then(|v| v.as_bool());

    Some(ParsedNode {
        name,
        node_type,
        server,
        port,
        cipher,
        password,
        uuid,
        alter_id,
        network,
        ws_opts,
        sni,
        skip_cert_verify,
        udp,
    })
}

pub fn parse_uri_list(content: &str) -> Result<Vec<ParsedNode>, String> {
    let mut nodes = Vec::new();
    for line in content.trim().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
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
    let raw = url.username();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(raw))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(format!("{}==", raw)))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(format!("{}=", raw)))
        .ok()?;
    let s = String::from_utf8(decoded).ok()?;
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    let (cipher, password) = match parts.len() {
        2 => (parts[0].to_string(), parts[1].to_string()),
        _ => ("aes-256-gcm".to_string(), s),
    };
    Some(ParsedNode {
        name: url.fragment().unwrap_or("").into(),
        node_type: "ss".into(),
        server: url.host_str()?.into(),
        port: url.port().unwrap_or(443),
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
    let encoded = uri.strip_prefix("vmess://")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let json_str = String::from_utf8(decoded).ok()?;
    #[derive(serde::Deserialize)]
    struct V {
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
    let cfg: V = serde_json::from_str(&json_str).ok()?;
    let ws = if cfg.net.as_deref() == Some("ws") {
        Some(
            serde_json::json!({"path":cfg.path.unwrap_or("/".into()),"headers":{"Host":cfg.host.unwrap_or_default()}}),
        )
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
        ws_opts: ws,
        sni: cfg.sni,
        skip_cert_verify: cfg.tls.map(|t| t == "tls"),
        udp: Some(true),
    })
}

fn parse_trojan(url: &Url) -> Option<ParsedNode> {
    let mut sni = None;
    for (k, v) in url.query_pairs() {
        if k == "sni" {
            sni = Some(v.to_string());
        }
    }
    Some(ParsedNode {
        name: url.fragment().unwrap_or("").into(),
        node_type: "trojan".into(),
        server: url.host_str()?.into(),
        port: url.port().unwrap_or(443),
        cipher: None,
        password: Some(url.username().into()),
        uuid: None,
        alter_id: None,
        network: None,
        ws_opts: None,
        sni,
        skip_cert_verify: None,
        udp: Some(true),
    })
}

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

    let mapping = yaml_val
        .as_mapping()
        .ok_or_else(|| "subscription content is not a YAML mapping".to_string())?;

    let proxies = mapping
        .get("proxies")
        .cloned()
        .unwrap_or(serde_yaml::Value::Sequence(vec![]));
    let proxy_groups = mapping
        .get("proxy-groups")
        .cloned()
        .unwrap_or(serde_yaml::Value::Sequence(vec![]));
    let rules = mapping
        .get("rules")
        .cloned()
        .unwrap_or(serde_yaml::Value::Sequence(vec![]));

    // Validate proxies is a non-empty sequence
    match &proxies {
        serde_yaml::Value::Sequence(s) if !s.is_empty() => {}
        _ => return Err("no proxies found in subscription".to_string()),
    }

    Ok(SubscriptionContent {
        proxies,
        proxy_groups,
        rules,
    })
}

/// Auto-detect a name from subscription content.
/// Returns the first proxy-group's name field.
pub fn detect_subscription_name(content: &str) -> Result<String, String> {
    let yaml_val: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| format!("YAML parse error: {}", e))?;

    let mapping = yaml_val
        .as_mapping()
        .ok_or_else(|| "not a YAML mapping".to_string())?;

    if let Some(groups) = mapping.get("proxy-groups") {
        if let Some(seq) = groups.as_sequence() {
            for item in seq {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    if !name.is_empty() {
                        return Ok(name.to_string());
                    }
                }
            }
        }
    }
    Err("could not detect name from subscription".into())
}

/// Extract a readable name from a subscription URL's hostname.
pub fn name_from_url(url: &str) -> Result<String, String> {
    let without_scheme = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    if host.is_empty() {
        return Err("URL has empty host".into());
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 2 {
        Ok(parts[parts.len() - 2].to_string())
    } else {
        Ok(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_yaml_clash() {
        assert!(matches!(
            detect_format("mixed-port: 7897\nproxies:"),
            SubscriptionFormat::Yaml
        ));
    }
    #[test]
    fn test_detect_yaml_simple() {
        assert!(matches!(
            detect_format("proxies:\n  - name: t\n    type: ss"),
            SubscriptionFormat::Yaml
        ));
    }
    #[test]
    fn test_detect_plain_uri() {
        assert!(matches!(
            detect_format("ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#T"),
            SubscriptionFormat::PlainUri
        ));
    }
    #[test]
    fn test_parse_inline_yaml() {
        let yaml = r#"proxies:
  - { name: "Node1", type: ss, server: 1.2.3.4, port: '8388', cipher: aes-256-gcm, password: pass123, udp: true }
  - { name: "Node2", type: vmess, server: vm.example.com, port: 443, uuid: abc-123, alterId: 0 }
"#;
        let nodes = parse_yaml(yaml).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "Node1");
        assert_eq!(nodes[0].port, 8388);
        assert_eq!(nodes[1].node_type, "vmess");
    }
    #[test]
    fn test_parse_full_clash_config() {
        let yaml = "mixed-port: 7897\nmode: rule\nproxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388, cipher: aes-256-gcm, password: p }\nproxy-groups:\n  - name: G\nrules:\n  - MATCH,G";
        let nodes = parse_yaml(yaml).unwrap();
        assert_eq!(nodes.len(), 1);
    }
    #[test]
    fn test_parse_yaml_empty() {
        assert!(parse_yaml("proxies: []\n").unwrap().is_empty());
    }
    #[test]
    fn test_parse_ss_uri() {
        let n = parse_uri_list("ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#TestNode").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].server, "1.2.3.4");
    }
    #[test]
    fn test_parse_trojan() {
        let n = parse_uri_list("trojan://pass@t.example.com:443?sni=example.com#TN").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].sni.as_deref(), Some("example.com"));
    }
    #[test]
    fn test_parse_multiline() {
        let n = parse_uri_list(
            "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#A\ntrojan://p@5.6.7.8:443#B\n\n",
        )
        .unwrap();
        assert_eq!(n.len(), 2);
    }
    #[test]
    fn test_parse_base64() {
        let plain = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#B64";
        let enc = base64::engine::general_purpose::STANDARD.encode(plain);
        let n = parse_base64(&enc).unwrap();
        assert_eq!(n.len(), 1);
    }
    #[test]
    fn test_parse_base64_invalid() {
        assert!(parse_base64("!!!invalid!!!").is_err());
    }

    #[test]
    fn test_parse_subscription_full_with_groups_and_rules() {
        let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 443 }\nproxy-groups:\n  - name: G1\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G1";
        let sub = parse_subscription_full(yaml).unwrap();
        assert_eq!(sub.proxies.as_sequence().unwrap().len(), 1);
        assert_eq!(sub.proxy_groups.as_sequence().unwrap().len(), 1);
        assert_eq!(sub.rules.as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn test_parse_subscription_full_empty_proxies_is_error() {
        assert!(parse_subscription_full("proxies: []\n").is_err());
    }

    #[test]
    fn test_parse_subscription_full_missing_proxies_is_error() {
        assert!(parse_subscription_full("mode: rule\n").is_err());
    }

    #[test]
    fn test_detect_subscription_name_from_groups() {
        let yaml = "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: MySub\n    type: select\n    proxies: [N1]";
        let name = detect_subscription_name(yaml).unwrap();
        assert_eq!(name, "MySub");
    }

    #[test]
    fn test_detect_subscription_name_no_groups_is_error() {
        let yaml = "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\n";
        assert!(detect_subscription_name(yaml).is_err());
    }

    #[test]
    fn test_name_from_url_with_subdomain() {
        assert_eq!(
            name_from_url("https://xWjXVnD.doggygosubs.com:8443/api/v1/client/abc").unwrap(),
            "doggygosubs"
        );
    }

    #[test]
    fn test_name_from_url_simple_host() {
        assert_eq!(
            name_from_url("https://sub.example.com/link").unwrap(),
            "example"
        );
    }

    #[test]
    fn test_name_from_url_empty_host() {
        assert!(name_from_url("https:///path").is_err());
    }
}
