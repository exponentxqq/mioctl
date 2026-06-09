use crate::api::client::MihomoClient;
use crate::api::error::ApiResult;
use crate::api::types::*;
use crate::app::state::ProxyMode;

pub struct ProxyManager;

impl ProxyManager {
    pub async fn refresh_all(client: &MihomoClient) -> ApiResult<(ProxiesResponse, Vec<Group>)> {
        let proxies = client.get_proxies().await?;
        let groups = MihomoClient::extract_groups(&proxies);
        Ok((proxies, groups))
    }

    pub async fn switch_node(client: &MihomoClient, group: &str, node: &str) -> ApiResult<()> {
        client.select_proxy(group, node).await
    }

    pub async fn test_node_delay(
        client: &MihomoClient,
        node: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<DelayResponse> {
        client.test_proxy_delay(node, test_url, timeout_ms).await
    }

    pub async fn test_group_delay(
        client: &MihomoClient,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> ApiResult<Vec<DelayResponse>> {
        client.test_group_delay(group, test_url, timeout_ms).await
    }

    pub fn detect_proxy_mode(groups: &[Group]) -> ProxyMode {
        for g in groups {
            if g.name == "GLOBAL" {
                match g.now.as_deref() {
                    Some("DIRECT") => return ProxyMode::Direct,
                    Some("GLOBAL") => return ProxyMode::Global,
                    _ => return ProxyMode::Rule,
                }
            }
        }
        ProxyMode::Rule
    }

    pub async fn set_proxy_mode(
        client: &MihomoClient,
        mode: &ProxyMode,
    ) -> ApiResult<()> {
        let mode_str = match mode {
            ProxyMode::Global => "global",
            ProxyMode::Rule => "rule",
            ProxyMode::Direct => "direct",
        };
        client.patch_configs(serde_json::json!({"mode": mode_str})).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Group;

    fn make_group(now: &str) -> Group {
        Group {
            name: "GLOBAL".into(),
            group_type: "Selector".into(),
            now: Some(now.into()),
            all: vec![now.into(), "DIRECT".into()],
        }
    }

    #[test]
    fn test_detect_proxy_mode_global() {
        let groups = vec![make_group("GLOBAL")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Global);
    }

    #[test]
    fn test_detect_proxy_mode_direct() {
        let groups = vec![make_group("DIRECT")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Direct);
    }

    #[test]
    fn test_detect_proxy_mode_rule() {
        let groups = vec![make_group("Japan-01")];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Rule);
    }

    #[test]
    fn test_detect_proxy_mode_no_global_group() {
        let groups = vec![Group {
            name: "YouTube".into(),
            group_type: "Selector".into(),
            now: Some("DIRECT".into()),
            all: vec!["DIRECT".into()],
        }];
        assert_eq!(ProxyManager::detect_proxy_mode(&groups), ProxyMode::Rule);
    }
}
