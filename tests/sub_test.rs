#[tokio::test]
async fn test_real_subscription_parse() {
    let url = "https://xWjXVnD.doggygosubs.com:8443/api/v1/client/4139e9bccc8cff837ee74fcfe65ef140";

    let content = mioctl::subscription::fetcher::fetch_with_ua_probe(url, None)
        .await
        .expect("fetch failed");
    eprintln!("Fetched: {} bytes", content.len());
    eprintln!("Preview:\n{}", &content[..content.len().min(500)]);

    let profile = mioctl::subscription::profile::normalize_to_yaml("real", &content)
        .expect("normalize failed");
    eprintln!("Success: {} nodes", profile.node_count);
    for warning in &profile.warnings {
        eprintln!("warning: {}", warning);
    }
}
#[tokio::test]
async fn debug_yaml() {
    let url = "https://xWjXVnD.doggygosubs.com:8443/api/v1/client/4139e9bccc8cff837ee74fcfe65ef140";
    let content = mioctl::subscription::fetcher::fetch_with_ua_probe(url, None)
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
                                "mapping".to_string()
                            } else {
                                "scalar".to_string()
                            }
                        })
                    );
                }
            }
        }
        Err(e) => eprintln!("YAML error: {}", e),
    }
}
