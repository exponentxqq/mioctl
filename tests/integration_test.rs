use mioctl::api::client::MihomoClient;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn test_get_version() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"version": "mihomo v1.18.0"})),
        )
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let v = client.get_version().await.unwrap();
    assert_eq!(v.version, "mihomo v1.18.0");
}

#[tokio::test]
async fn test_get_proxies() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/proxies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "proxies": {"GLOBAL": {"name":"GLOBAL","type":"Selector","now":"DIRECT","all":["DIRECT","Node1"],"history":[],"udp":true,"alive":true}}
        })))
        .mount(&server).await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let r = client.get_proxies().await.unwrap();
    assert!(r.proxies.contains_key("GLOBAL"));
}

#[tokio::test]
async fn test_get_connections() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/connections"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"connections":[],"downloadTotal":0,"uploadTotal":0}),
            ),
        )
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let r = client.get_connections().await.unwrap();
    assert!(r.connections.is_empty());
}

#[tokio::test]
async fn test_select_proxy() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/proxies/GLOBAL"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    assert!(client.select_proxy("GLOBAL", "Node1").await.is_ok());
}

#[tokio::test]
async fn test_close_connection() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/connections/abc-123"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    assert!(client.close_connection("abc-123").await.is_ok());
}

#[tokio::test]
async fn test_close_all_connections() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/connections"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    assert!(client.close_all_connections().await.is_ok());
}

#[tokio::test]
async fn test_get_rules() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "rules":[{"type":"DOMAIN-SUFFIX","payload":"google.com","proxy":"Proxy"}]
        })))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let r = client.get_rules().await.unwrap();
    assert_eq!(r.rules.len(), 1);
    assert_eq!(r.rules[0].payload, "google.com");
}

#[tokio::test]
async fn test_get_traffic() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/traffic"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"up":102400,"down":204800})),
        )
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let t = client.get_traffic().await.unwrap();
    assert_eq!(t.up, 102400);
    assert_eq!(t.down, 204800);
}

#[tokio::test]
async fn test_get_configs() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"port":7890,"mixed-port":7890,"mode":"rule","log-level":"info"}),
        ))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let c = client.get_configs().await.unwrap();
    assert_eq!(c.mode.as_deref(), Some("rule"));
}

#[tokio::test]
async fn test_reload_config() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    assert!(client.reload_config(None).await.is_ok());
}

#[tokio::test]
async fn test_api_error_handling() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/proxies"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    assert!(client.get_proxies().await.is_err());
}
