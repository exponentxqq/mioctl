use crate::api::client::MihomoClient;
use crate::api::error::{ApiError, ApiResult};
use crate::api::types::*;

/// Percent-encode a string for use in URL path segments.
fn encode_path(s: &str) -> String {
    s.bytes()
        .flat_map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                vec![b as char]
            }
            _ => format!("%{:02X}", b).chars().collect(),
        })
        .collect()
}

#[allow(dead_code)]
impl MihomoClient {
    /// Helper: check response status, extract error body for non-2xx
    async fn check_response(resp: reqwest::Response) -> Result<reqwest::Response, ApiError> {
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            // Try to extract message from JSON error response
            let msg = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                val.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&body)
                    .to_string()
            } else {
                body
            };
            let msg = if msg.is_empty() {
                format!("HTTP {}", status)
            } else {
                format!("HTTP {}: {}", status, msg)
            };
            return Err(ApiError::ApiError(status, msg));
        }
        Ok(resp)
    }

    // --- Proxies ---

    pub async fn get_proxies(&self) -> ApiResult<ProxiesResponse> {
        let url = format!("{}/proxies", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn get_proxy(&self, name: &str) -> ApiResult<Proxy> {
        let url = format!("{}/proxies/{}", self.base_url(), encode_path(name));
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn select_proxy(&self, group: &str, name: &str) -> ApiResult<()> {
        let url = format!("{}/proxies/{}", self.base_url(), encode_path(group));
        let body = serde_json::json!({"name": name});
        let resp = self.client().put(&url).json(&body).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn test_proxy_delay(
        &self,
        name: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<DelayResponse> {
        let url = format!(
            "{}/proxies/{}/delay?url={}&timeout={}",
            self.base_url(),
            encode_path(name),
            test_url,
            timeout_ms
        );
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- Groups ---

    /// Extract groups from the `/proxies` response.
    /// Mihomo groups are proxies with a non-empty `.all` field
    /// (Selector, URLTest, Fallback, LoadBalance, etc.).
    /// Sorted by name for stable ordering across refreshes.
    pub fn extract_groups(proxies: &ProxiesResponse) -> Vec<Group> {
        let mut groups: Vec<Group> = proxies
            .proxies
            .values()
            .filter(|p| !p.all.is_empty())
            .map(|p| Group {
                name: p.name.clone(),
                group_type: p.proxy_type.clone(),
                now: p.now.clone(),
                all: p.all.clone(),
            })
            .collect();
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        groups
    }

    pub async fn get_group(&self, name: &str) -> ApiResult<Group> {
        let url = format!("{}/group/{}", self.base_url(), encode_path(name));
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn test_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<std::collections::HashMap<String, i64>> {
        let url = format!(
            "{}/group/{}/delay?url={}&timeout={}",
            self.base_url(),
            encode_path(group),
            test_url,
            timeout_ms
        );
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- Rules ---

    pub async fn get_rules(&self) -> ApiResult<RulesResponse> {
        let url = format!("{}/rules", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- Connections ---

    pub async fn get_connections(&self) -> ApiResult<ConnectionsResponse> {
        let url = format!("{}/connections", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn close_connection(&self, id: &str) -> ApiResult<()> {
        let url = format!("{}/connections/{}", self.base_url(), encode_path(id));
        let resp = self.client().delete(&url).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn close_all_connections(&self) -> ApiResult<()> {
        let url = format!("{}/connections", self.base_url());
        let resp = self.client().delete(&url).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    // --- Providers ---

    pub async fn get_proxy_providers(&self) -> ApiResult<ProvidersResponse> {
        let url = format!("{}/providers/proxies", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn update_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!(
            "{}/providers/proxies/{}",
            self.base_url(),
            encode_path(name)
        );
        let resp = self.client().put(&url).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn healthcheck_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!(
            "{}/providers/proxies/{}/healthcheck",
            self.base_url(),
            encode_path(name)
        );
        let resp = self.client().get(&url).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    // --- Config ---

    pub async fn get_configs(&self) -> ApiResult<MihomoConfig> {
        let url = format!("{}/configs", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    pub async fn reload_config(&self, path: Option<&str>) -> ApiResult<()> {
        let url = format!("{}/configs?force=true", self.base_url());
        let body = serde_json::json!({"path": path.unwrap_or(""), "payload": ""});
        let resp = self.client().put(&url).json(&body).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn patch_configs(&self, payload: serde_json::Value) -> ApiResult<()> {
        let url = format!("{}/configs", self.base_url());
        let resp = self.client().patch(&url).json(&payload).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    pub async fn restart(&self) -> ApiResult<()> {
        let url = format!("{}/restart", self.base_url());
        let body = serde_json::json!({"path": "", "payload": ""});
        let resp = self.client().post(&url).json(&body).send().await?;
        Self::check_response(resp).await?;
        Ok(())
    }

    // --- Traffic ---

    pub async fn get_traffic(&self) -> ApiResult<Traffic> {
        let url = format!("{}/traffic", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- Memory ---

    pub async fn get_memory(&self) -> ApiResult<Memory> {
        let url = format!("{}/memory", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- Version ---

    pub async fn get_version(&self) -> ApiResult<Version> {
        let url = format!("{}/version", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }

    // --- DNS ---

    pub async fn dns_query(&self, name: &str, record_type: &str) -> ApiResult<DnsQueryResponse> {
        let url = format!(
            "{}/dns/query?name={}&type={}",
            self.base_url(),
            encode_path(name),
            record_type
        );
        let resp = self.client().get(&url).send().await?;
        let data = Self::check_response(resp).await?.json().await?;
        Ok(data)
    }
}
