use crate::api::types::ParsedNode;
use base64::Engine;
use url::Url;

pub fn decode_base64_lenient(s: &str) -> Option<String> {
    let cleaned: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return None;
    }
    let decode_standard = |input: &str| -> Option<String> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(input)
            .ok()?;
        String::from_utf8(decoded).ok()
    };
    if let Some(text) = decode_standard(&cleaned) {
        return Some(text);
    }
    let padded = pad_base64(&cleaned);
    if let Some(text) = decode_standard(&padded) {
        return Some(text);
    }
    let decode_url_safe = |input: &str| -> Option<String> {
        let decoded = base64::engine::general_purpose::URL_SAFE
            .decode(input)
            .ok()?;
        String::from_utf8(decoded).ok()
    };
    if let Some(text) = decode_url_safe(&cleaned) {
        return Some(text);
    }
    decode_url_safe(&padded)
}

fn pad_base64(s: &str) -> String {
    let mut out = s.to_string();
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

pub fn parse_uri_list(content: &str) -> Result<(Vec<ParsedNode>, Vec<String>), String> {
    let mut nodes = Vec::new();
    let mut skipped = Vec::new();
    for line in content.trim().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(node) = parse_single_uri(line) {
            nodes.push(node);
        } else {
            skipped.push(line.to_string());
        }
    }
    Ok((nodes, skipped))
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
    let lower = url.to_lowercase();
    let without_scheme = if lower.starts_with("https://") {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        url
    };
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = host.rsplit('@').next().unwrap_or(host);
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
    fn test_parse_ss_uri() {
        let (n, _) =
            parse_uri_list("ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#TestNode").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].server, "1.2.3.4");
    }
    #[test]
    fn test_parse_trojan() {
        let (n, _) = parse_uri_list("trojan://pass@t.example.com:443?sni=example.com#TN").unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].sni.as_deref(), Some("example.com"));
    }
    #[test]
    fn test_parse_multiline() {
        let (n, _) = parse_uri_list(
            "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#A\ntrojan://p@5.6.7.8:443#B\n\n",
        )
        .unwrap();
        assert_eq!(n.len(), 2);
    }
    #[test]
    fn test_parse_uri_list_reports_skipped_lines() {
        let (nodes, skipped) =
            parse_uri_list("not-a-uri\nvless://uuid@host:443#V\n# comment").unwrap();
        assert!(nodes.is_empty());
        assert_eq!(skipped, vec!["not-a-uri", "vless://uuid@host:443#V"]);
    }

    #[test]
    fn test_decode_base64_lenient_unpadded() {
        let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388, cipher: aes-256-gcm, password: p }\n";
        let mut b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
        while b64.ends_with('=') {
            b64.pop();
        }
        assert_eq!(decode_base64_lenient(&b64).unwrap(), yaml);
    }

    #[test]
    fn test_decode_base64_lenient_line_wrapped() {
        let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388 }\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
        let wrapped: String = b64
            .as_bytes()
            .chunks(76)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(decode_base64_lenient(&wrapped).unwrap(), yaml);
    }

    #[test]
    fn test_decode_base64_lenient_url_safe_alphabet() {
        let b64 = base64::engine::general_purpose::URL_SAFE.encode("😀");
        assert!(
            b64.contains('-'),
            "test premise: URL_SAFE output must use URL-safe chars, got {}",
            b64
        );
        assert_eq!(decode_base64_lenient(&b64).unwrap(), "😀");
    }

    #[test]
    fn test_decode_base64_lenient_garbage_is_none() {
        assert!(decode_base64_lenient("!!!not base64!!!").is_none());
        assert!(decode_base64_lenient("   ").is_none());
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

    #[test]
    fn test_name_from_url_strips_userinfo() {
        assert_eq!(
            name_from_url("https://user:secret@example.com/sub").unwrap(),
            "example"
        );
    }

    #[test]
    fn test_name_from_url_case_insensitive_scheme() {
        let name = name_from_url("HTTPS://Example.COM/sub").unwrap();
        assert!(!name.contains('/'), "got: {}", name);
        assert!(!name.contains(':'), "got: {}", name);
    }
}
