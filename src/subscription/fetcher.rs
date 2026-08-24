use reqwest::Client;

const UA_CANDIDATES: &[&str] = &["mihomo/{version}", "ClashMeta/1.19.0", "clash-verge/1.3.8"];

/// Fetch subscription content, trying multiple User-Agents in order.
/// First response that contains >= 1 valid proxy entries wins.
/// The mihomo/{version} UA requires the current mihomo version string.
pub async fn fetch_with_ua_probe(
    url: &str,
    mihomo_version: Option<String>,
) -> Result<String, String> {
    let mut last_error: Option<String> = None;
    for &ua_template in UA_CANDIDATES {
        let ua = if ua_template.contains("{version}") {
            match &mihomo_version {
                Some(v) => ua_template.replace("{version}", v),
                None => continue,
            }
        } else {
            ua_template.to_string()
        };

        match try_fetch(url, &ua).await {
            Ok(body) => {
                if count_proxy_entries(&body) >= 1 {
                    return Ok(body);
                }
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }
    let base = "all User-Agent probes failed — subscription requires a different client identity";
    match last_error {
        Some(e) => Err(format!("{} (last error: {})", base, e)),
        None => Err(base.into()),
    }
}

async fn try_fetch(url: &str, user_agent: &str) -> Result<String, String> {
    let client = Client::builder()
        .user_agent(user_agent)
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.text().await.map_err(|e| e.to_string())
}

/// Count how many proxy entries the body contains (cheap heuristic).
/// Falls back to decoding the body as base64 so that base64-encoded
/// subscriptions (URI lists or embedded YAML) also pass the probe.
fn count_proxy_entries(body: &str) -> usize {
    fn count_entry_lines(text: &str) -> usize {
        text.lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("- ")
                    || t.starts_with("- {")
                    || t.starts_with("ss://")
                    || t.starts_with("vmess://")
                    || t.starts_with("trojan://")
            })
            .count()
    }

    let direct = count_entry_lines(body);
    if direct >= 3 {
        return direct;
    }
    if let Some(text) = crate::subscription::parser::decode_base64_lenient(body) {
        return count_entry_lines(&text);
    }
    direct
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn b64(content: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(content)
    }

    #[test]
    fn test_count_proxy_entries_yaml_direct() {
        let yaml = "proxies:\n  - name: A\n    type: ss\n  - name: B\n    type: ss\nproxy-groups:\n  - name: G\nrules:\n  - MATCH,G\n";
        assert_eq!(count_proxy_entries(yaml), 4);
    }

    #[test]
    fn test_count_proxy_entries_plain_uri_list() {
        let list = "ss://a@1.2.3.4:1#N1\nss://a@1.2.3.5:2#N2\nss://a@1.2.3.6:3#N3\n";
        assert_eq!(count_proxy_entries(list), 3);
    }

    #[test]
    fn test_count_proxy_entries_base64_uri_list() {
        let list = "ss://a@1.2.3.4:1#N1\nss://a@1.2.3.5:2#N2\nss://a@1.2.3.6:3#N3\n";
        assert_eq!(count_proxy_entries(&b64(list)), 3);
    }

    #[test]
    fn test_count_proxy_entries_base64_yaml() {
        let yaml = "proxies:\n  - name: A\n    type: ss\nproxy-groups:\n  - name: G\nrules:\n  - MATCH,G\n";
        assert_eq!(count_proxy_entries(&b64(yaml)), 3);
    }

    #[test]
    fn test_count_proxy_entries_base64_below_threshold_falls_back() {
        let list = "ss://a@1.2.3.4:1#N1\nss://a@1.2.3.5:2#N2\n";
        assert_eq!(count_proxy_entries(&b64(list)), 2);
    }

    #[test]
    fn test_count_proxy_entries_non_base64_garbage() {
        assert_eq!(count_proxy_entries("this is not a subscription"), 0);
        assert_eq!(count_proxy_entries(""), 0);
    }

    #[test]
    fn test_count_proxy_entries_yaml_short_circuits_without_decode() {
        let yaml = "proxies:\n  - name: A\n  - name: B\n  - name: C\n";
        assert_eq!(count_proxy_entries(yaml), 3);
    }

    #[tokio::test]
    async fn test_fetch_with_ua_probe_accepts_single_node_list() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/one"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#Solo\n",
            ))
            .mount(&server)
            .await;
        let result = fetch_with_ua_probe(&format!("{}/one", server.uri()), None).await;
        assert!(result.is_ok(), "got: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_fetch_with_ua_probe_rejects_thin_body() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/thin"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("hello"))
            .mount(&server)
            .await;
        let result = fetch_with_ua_probe(&format!("{}/thin", server.uri()), None).await;
        assert!(result.unwrap_err().contains("all User-Agent probes failed"));
    }

    #[tokio::test]
    async fn test_fetch_with_ua_probe_reports_last_http_error() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/gone"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let result = fetch_with_ua_probe(&format!("{}/gone", server.uri()), None).await;
        let err = result.unwrap_err();
        assert!(err.contains("all User-Agent probes failed"), "got: {}", err);
        assert!(err.contains("404"), "got: {}", err);
    }
}
