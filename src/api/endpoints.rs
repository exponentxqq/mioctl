use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::*;

impl MihomoClient {
    // --- Proxies ---

    pub async fn get_proxies(&self) -> ApiResult<ProxiesResponse> {
        let url = format!("{}/proxies", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn get_proxy(&self, name: &str) -> ApiResult<Proxy> {
        let url = format!("{}/proxies/{}", self.base_url(), name);
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn select_proxy(&self, group: &str, name: &str) -> ApiResult<()> {
        let url = format!("{}/proxies/{}", self.base_url(), group);
        let body = serde_json::json!({"name": name});
        self.client().put(&url).json(&body).send().await?;
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
            self.base_url(), name, test_url, timeout_ms
        );
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Groups ---

    pub async fn get_groups(&self) -> ApiResult<Vec<Group>> {
        let url = format!("{}/group", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data: serde_json::Value = resp.json().await?;
        let groups: Vec<Group> = data
            .as_object()
            .map(|obj| {
                obj.values()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(groups)
    }

    pub async fn get_group(&self, name: &str) -> ApiResult<Group> {
        let url = format!("{}/group/{}", self.base_url(), name);
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn test_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<Vec<DelayResponse>> {
        let url = format!(
            "{}/group/{}/delay?url={}&timeout={}",
            self.base_url(), group, test_url, timeout_ms
        );
        let resp = self.client().get(&url).send().await?;
        let data: Vec<DelayResponse> = resp.json().await?;
        Ok(data)
    }

    // --- Rules ---

    pub async fn get_rules(&self) -> ApiResult<RulesResponse> {
        let url = format!("{}/rules", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Connections ---

    pub async fn get_connections(&self) -> ApiResult<ConnectionsResponse> {
        let url = format!("{}/connections", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn close_connection(&self, id: &str) -> ApiResult<()> {
        let url = format!("{}/connections/{}", self.base_url(), id);
        self.client().delete(&url).send().await?;
        Ok(())
    }

    pub async fn close_all_connections(&self) -> ApiResult<()> {
        let url = format!("{}/connections", self.base_url());
        self.client().delete(&url).send().await?;
        Ok(())
    }

    // --- Providers ---

    pub async fn get_proxy_providers(&self) -> ApiResult<ProvidersResponse> {
        let url = format!("{}/providers/proxies", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn update_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!("{}/providers/proxies/{}", self.base_url(), name);
        self.client().put(&url).send().await?;
        Ok(())
    }

    pub async fn healthcheck_proxy_provider(&self, name: &str) -> ApiResult<()> {
        let url = format!("{}/providers/proxies/{}/healthcheck", self.base_url(), name);
        self.client().get(&url).send().await?;
        Ok(())
    }

    // --- Config ---

    pub async fn get_configs(&self) -> ApiResult<MihomoConfig> {
        let url = format!("{}/configs", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    pub async fn reload_config(&self, path: Option<&str>) -> ApiResult<()> {
        let url = format!("{}/configs?force=true", self.base_url());
        let body = serde_json::json!({"path": path.unwrap_or(""), "payload": ""});
        self.client().put(&url).json(&body).send().await?;
        Ok(())
    }

    pub async fn restart(&self) -> ApiResult<()> {
        let url = format!("{}/restart", self.base_url());
        let body = serde_json::json!({"path": "", "payload": ""});
        self.client().post(&url).json(&body).send().await?;
        Ok(())
    }

    // --- Traffic ---

    pub async fn get_traffic(&self) -> ApiResult<Traffic> {
        let url = format!("{}/traffic", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Memory ---

    pub async fn get_memory(&self) -> ApiResult<Memory> {
        let url = format!("{}/memory", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- Version ---

    pub async fn get_version(&self) -> ApiResult<Version> {
        let url = format!("{}/version", self.base_url());
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }

    // --- DNS ---

    pub async fn dns_query(&self, name: &str, record_type: &str) -> ApiResult<DnsQueryResponse> {
        let url = format!(
            "{}/dns/query?name={}&type={}",
            self.base_url(), name, record_type
        );
        let resp = self.client().get(&url).send().await?;
        let data = resp.json().await?;
        Ok(data)
    }
}
