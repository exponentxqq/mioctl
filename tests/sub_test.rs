#[tokio::test]
async fn test_real_subscription_parse() {
    let url = "https://xWjXVnD.doggygosubs.com:8443/api/v1/client/4139e9bccc8cff837ee74fcfe65ef140";

    let content = mioctl::subscription::fetcher::fetch_subscription(url)
        .await
        .expect("fetch failed");
    eprintln!("Fetched: {} bytes", content.len());
    eprintln!("Preview:\n{}", &content[..content.len().min(500)]);

    let format = mioctl::subscription::parser::detect_format(&content);
    let nodes = match format {
        mioctl::subscription::parser::SubscriptionFormat::Yaml => {
            eprintln!("Format: YAML");
            mioctl::subscription::parser::parse_yaml(&content)
        }
        mioctl::subscription::parser::SubscriptionFormat::Base64 => {
            eprintln!("Format: Base64");
            mioctl::subscription::parser::parse_base64(&content)
        }
        mioctl::subscription::parser::SubscriptionFormat::PlainUri => {
            eprintln!("Format: Plain URI list");
            mioctl::subscription::parser::parse_uri_list(&content)
        }
    };

    match nodes {
        Ok(nodes) => {
            eprintln!("Success: {} nodes parsed", nodes.len());
            eprintln!("--- Node list ---");
            for (i, n) in nodes.iter().enumerate() {
                eprintln!(
                    "{:3}. {:40} | {:8} | {}:{}",
                    i + 1,
                    n.name,
                    n.node_type,
                    n.server,
                    n.port
                );
            }
        }
        Err(e) => eprintln!("Parse error: {}", e),
    }
}
#[tokio::test]
async fn debug_yaml() {
    let url = "https://xWjXVnD.doggygosubs.com:8443/api/v1/client/4139e9bccc8cff837ee74fcfe65ef140";
    let content = mioctl::subscription::fetcher::fetch_subscription(url)
        .await
        .unwrap();

    // Find proxies section
    if let Some(idx) = content.find("proxies:") {
        let after_proxies = &content[idx..];
        eprintln!(
            "=== From 'proxies:' ===\n{}",
            &after_proxies[..after_proxies.len().min(2000)]
        );
    }

    // Try parsing raw YAML
    let yaml_val: Result<serde_yaml::Value, _> = serde_yaml::from_str(&content);
    match yaml_val {
        Ok(val) => {
            eprintln!("\n=== YAML keys ===");
            if let Some(mapping) = val.as_mapping() {
                for k in mapping.keys() {
                    eprintln!(
                        "  key: {:?} = {:?} type",
                        k,
                        mapping.get(k).map(|v| {
                            if v.is_sequence() {
                                format!("sequence[{}]", v.as_sequence().unwrap().len())
                            } else if v.is_mapping() {
                                format!("mapping")
                            } else {
                                format!("scalar")
                            }
                        })
                    );
                }
            }
        }
        Err(e) => eprintln!("YAML error: {}", e),
    }
}
