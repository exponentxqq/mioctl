use reqwest::Client;

const UA_CANDIDATES: &[&str] = &["mihomo/{version}", "ClashMeta/1.19.0", "clash-verge/1.3.8"];

/// Fetch subscription content, trying multiple User-Agents in order.
/// First response that contains >= 3 valid proxy entries wins.
/// The mihomo/{version} UA requires the current mihomo version string.
pub async fn fetch_with_ua_probe(
    url: &str,
    mihomo_version: Option<String>,
) -> Result<String, String> {
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
                if count_proxy_entries(&body) >= 3 {
                    return Ok(body);
                }
            }
            Err(_) => continue,
        }
    }
    Err("all User-Agent probes failed — subscription requires a different client identity".into())
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
fn count_proxy_entries(body: &str) -> usize {
    body.lines()
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

/// Legacy single-UA fetch, kept for backwards compatibility with update_all.
pub async fn fetch_subscription(url: &str) -> Result<String, String> {
    try_fetch(url, "clash-verge/1.3.8").await
}
