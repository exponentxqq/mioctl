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

// Quick fix: replace the parse_shadowsocks function
fn parse_shadowsocks(url: &Url) -> Option<ParsedNode> {
    // SS URI: ss://base64(method:password)@server:port#name
    // The = padding in base64 may cause issues with URL parsing.
    // We extract username from the URL, handling various encodings.
    let raw_userinfo = url.username();
    
    // Try multiple base64 decode strategies
    let decoded_bytes = {
        let s = raw_userinfo;
        // Strategy 1: standard base64
        base64::engine::general_purpose::STANDARD.decode(s)
            // Strategy 2: URL-safe base64 (no padding)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(s))
            // Strategy 3: standard with = added
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(format!("{}==", s)))
            // Strategy 4: standard with == added
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(format!("{}=", s)))
    }.ok()?;
    
    let decoded_str = String::from_utf8(decoded_bytes).ok()?;
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
            "headers": {"Host": cfg.host.unwrap_or_default()}
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_yaml_valid() {
        let yaml = r#"
proxies:
  - name: "Japan"
    type: ss
    server: jp.example.com
    port: 443
    cipher: aes-256-gcm
    password: "secret123"
    udp: true
  - name: "Singapore"
    type: vmess
    server: sg.example.com
    port: 8080
    uuid: "b831381d-6324-4d53-ad4f-8cda48b30811"
    alterId: 0
"#;
        let nodes = parse_yaml(yaml).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "Japan");
        assert_eq!(nodes[0].node_type, "ss");
        assert_eq!(nodes[1].name, "Singapore");
    }

    #[test]
    fn test_parse_yaml_empty() {
        let yaml = "proxies: []\n";
        let nodes = parse_yaml(yaml).unwrap();
        assert!(nodes.is_empty());
    }

    #[test]
    fn test_parse_ss_uri() {
        let uri = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#TestNode";
        let nodes = parse_uri_list(uri).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "TestNode");
        assert_eq!(nodes[0].node_type, "ss");
        assert_eq!(nodes[0].server, "1.2.3.4");
        assert_eq!(nodes[0].port, 8388);
    }

    #[test]
    fn test_parse_trojan_uri() {
        let uri = "trojan://password123@trojan.example.com:443?sni=example.com#TrojanNode";
        let nodes = parse_uri_list(uri).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].node_type, "trojan");
        assert_eq!(nodes[0].sni.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_parse_multiline_uri_list() {
        let content = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#Node1\ntrojan://pass@5.6.7.8:443#Node2\n\n";
        let nodes = parse_uri_list(content).unwrap();
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn test_parse_base64_subscription() {
        // base64 of: ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#B64Node
        let encoded = "c3M6Ly9ZV1Z6TFRJMU5pMW5ZMjA2ZEdWemRERXlNd0AxLjIuMy40OjgzODgjQjY0Tm9kZQ==";
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

// DEBUG - will remove
#[test]
fn debug_ss_parse() {
    let uri = "ss://YWVzLTI1Ni1nY206dGVzdDEyMw@1.2.3.4:8388#TestNode";
    let url = url::Url::parse(uri);
    eprintln!("URL parse result: {:?}", url.as_ref().map(|u| (u.username(), u.host_str())));
    
    // Try base64 with various padding
    let enc = "YWVzLTI1Ni1nY206dGVzdDEyMw";
    let r1 = base64::engine::general_purpose::STANDARD.decode(enc);
    let r2 = base64::engine::general_purpose::STANDARD.decode(format!("{}=", enc));
    let r3 = base64::engine::general_purpose::STANDARD.decode(format!("{}==", enc));
    eprintln!("decode1: {:?}", r1);
    eprintln!("decode2: {:?}", r2);
    eprintln!("decode3: {:?}", r3);
}
