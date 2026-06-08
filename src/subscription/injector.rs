use crate::api::client::MihomoClient;
use crate::api::types::ParsedNode;
use crate::config::mioctl_config::MioctlConfig;
use std::path::PathBuf;

pub fn generate_provider_yaml(_name: &str, nodes: &[ParsedNode]) -> String {
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
                entry.push_str(&format!(
                    "    ws-opts:\n      path: {}\n",
                    path.as_str().unwrap_or("/")
                ));
            }
            if let Some(headers) = ws.get("headers") {
                if let Some(host) = headers.get("Host") {
                    entry.push_str(&format!(
                        "      headers:\n        Host: {}\n",
                        host.as_str().unwrap_or("")
                    ));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node() -> ParsedNode {
        ParsedNode {
            name: "Test".into(),
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
        assert!(yaml.contains("name: \"Test\""));
        assert!(yaml.contains("type: ss"));
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
