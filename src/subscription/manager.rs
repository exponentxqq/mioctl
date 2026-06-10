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

        // 3. Reject duplicate subscriptions
        if config.subscriptions.items.iter().any(|s| s.name == sub_name) {
            return Err(format!(
                "subscription '{}' already exists. Remove it first or use a different --name.",
                sub_name
            ));
        }

        // 4. Parse subscription content
        let format = detect_format(&content);
        let sub = match format {
            SubscriptionFormat::Yaml => parse_subscription_full(&content)?,
            SubscriptionFormat::Base64 => {
                let nodes = parse_base64(&content)?;
                let (proxies, proxy_groups, rules) =
                    nodes_to_subscription_content(&sub_name, &nodes);
                SubscriptionContent { proxies, proxy_groups, rules }
            }
            SubscriptionFormat::PlainUri => {
                let nodes = parse_uri_list(&content)?;
                let (proxies, proxy_groups, rules) =
                    nodes_to_subscription_content(&sub_name, &nodes);
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
        let reload_msg = if !no_reload {
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret.clone())
            };
            match MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match c.reload_config(None).await {
                    Ok(_) => "mihomo reloaded successfully".into(),
                    Err(e) => {
                        let base = format!(
                            "mihomo API reload failed: {}. Trying systemctl fallback...",
                            e
                        );
                        let status = std::process::Command::new("systemctl")
                            .args(["--user", "restart", "mihomo"])
                            .output();
                        match status {
                            Ok(o) if o.status.success() => {
                                format!("{} systemctl restart succeeded.", base)
                            }
                            _ => format!(
                                "{} systemctl restart also failed. Run: systemctl --user restart mihomo",
                                base
                            ),
                        }
                    }
                },
                Err(e) => format!("could not connect to mihomo for reload: {}", e),
            }
        } else {
            "reload skipped (--no-reload)".into()
        };

        let summary = format!(
            "Subscription '{}' registered successfully.\n  {} proxies, {} groups, {} rules\n  config: {}\n  {}",
            sub_name, result.proxy_count, result.group_count, result.rule_count,
            config_path, reload_msg,
        );
        Ok(summary)
    }

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
                    if let Some(s) =
                        config.subscriptions.items.iter_mut().find(|s| s.name == item.name)
                    {
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
        name: String,
        url: &str,
        client: &MihomoClient,
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
    group.insert(Value::String("name".into()), Value::String(name.to_string()));
    group.insert(Value::String("type".into()), Value::String("select".to_string()));
    let node_names: Vec<Value> =
        nodes.iter().map(|n| Value::String(n.name.clone())).collect();
    group.insert(
        Value::String("proxies".into()),
        Value::Sequence(node_names),
    );
    let proxy_groups = Value::Sequence(vec![Value::Mapping(group)]);

    let rules = Value::Sequence(vec![Value::String(format!("MATCH,{}", name))]);

    (proxies, proxy_groups, rules)
}
