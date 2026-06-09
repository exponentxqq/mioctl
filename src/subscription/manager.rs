use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::fetcher::fetch_subscription;
use crate::subscription::parser::{detect_format, parse_yaml, parse_uri_list, parse_base64, SubscriptionFormat};
use crate::subscription::injector::{write_provider_file, inject_provider};
use crate::api::client::MihomoClient;

pub struct SubscriptionManager;

impl SubscriptionManager {
    pub async fn update_all(config: &mut MioctlConfig, client: &MihomoClient) -> Result<String, String> {
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
        name: String,
        url: &str,
        client: &MihomoClient,
    ) -> Result<usize, String> {
        let content = fetch_subscription(url).await?;
        let nodes = match detect_format(&content) {
            SubscriptionFormat::Yaml => parse_yaml(&content)?,
            SubscriptionFormat::Base64 => parse_base64(&content)?,
            SubscriptionFormat::PlainUri => parse_uri_list(&content)?,
        };
        if nodes.is_empty() {
            return Err("no nodes found in subscription".into());
        }
        write_provider_file(&name, &nodes)?;
        inject_provider(client, &name).await?;
        Ok(nodes.len())
    }
}
