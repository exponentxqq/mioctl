use mioctl::api::client::MihomoClient;
use mioctl::api::types::TunConfig;
use mioctl::app::state::{AppState, SharedState};
use mioctl::ui::app::toggle_tun_flow;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::Mutex as TokioMutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct TestEnv {
    _dir: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
}

impl TestEnv {
    fn new() -> Self {
        let guard = lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("MIOCTL_HOME", dir.path());
            std::env::set_var("HOME", dir.path());
            std::env::set_var("MIOCTL_TEST_NO_GSETTINGS", "1");
            std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1");
        }
        TestEnv {
            _dir: dir,
            _guard: guard,
        }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("MIOCTL_HOME");
            std::env::remove_var("HOME");
            std::env::remove_var("MIOCTL_TEST_NO_GSETTINGS");
            std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL");
        }
    }
}

fn configs_body(tun_enable: bool) -> serde_json::Value {
    serde_json::json!({
        "port": 7890,
        "socks-port": 7891,
        "mixed-port": 7897,
        "allow-lan": false,
        "mode": "rule",
        "log-level": "info",
        "tun": {
            "enable": tun_enable,
            "device": "Meta",
            "stack": "gVisor",
            "dns-hijack": ["any:53"],
            "auto-route": true,
            "auto-detect-interface": true,
            "mtu": 1500
        }
    })
}

async fn mount_refresh_endpoints(server: &MockServer, tun_enable: bool) {
    Mock::given(method("GET"))
        .and(path("/proxies"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"proxies": {}})))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/connections"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"connections":[],"downloadTotal":0,"uploadTotal":0}),
            ),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"rules": []})))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/traffic"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"up":0,"down":0})),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(configs_body(tun_enable)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/memory"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"inuse":0,"oslimit":0})),
        )
        .mount(server)
        .await;
}

fn make_state(server: &MockServer, tun: TunConfig) -> SharedState {
    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let mut s = AppState::new();
    s.tun = Some(tun);
    s.mixed_port = Some(7897);
    s.client = Some(client);
    Arc::new(TokioMutex::new(s))
}

#[tokio::test]
async fn test_toggle_tun_disable_flow() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, false).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: true,
            stack: Some("gVisor".into()),
            device: Some("Meta".into()),
            auto_route: Some(true),
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(false));
    assert!(s.system_proxy_enabled);
    assert!(s.ui.loading.is_none());
    assert!(s.logs.iter().any(|l| l.payload.contains("TUN disabled")));
    drop(s);

    let requests = server.received_requests().await.unwrap();
    let patch = requests
        .iter()
        .find(|r| r.method == "PATCH" && r.url.path() == "/configs")
        .expect("PATCH /configs must be sent");
    let body: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(body, serde_json::json!({"tun": {"enable": false}}));
    assert!(
        !requests.iter().any(|r| r.url.path() == "/restart"),
        "no /restart request expected"
    );
}

#[tokio::test]
async fn test_toggle_tun_enable_flow() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, true).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: false,
            stack: None,
            device: None,
            auto_route: None,
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(true));
    assert!(!s.system_proxy_enabled);
    assert!(s.ui.loading.is_none());
    assert!(s.logs.iter().any(|l| l.payload.contains("TUN enabled")));
    drop(s);

    let requests = server.received_requests().await.unwrap();
    let patch = requests
        .iter()
        .find(|r| r.method == "PATCH" && r.url.path() == "/configs")
        .expect("PATCH /configs must be sent");
    let body: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"tun": {"enable": true, "stack": "system"}})
    );
    assert!(
        !requests.iter().any(|r| r.url.path() == "/restart"),
        "no /restart request expected"
    );
}

#[tokio::test]
async fn test_toggle_tun_enable_preserves_runtime_fields() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, true).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: false,
            stack: Some("gVisor".into()),
            device: Some("Meta".into()),
            auto_route: Some(true),
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let requests = server.received_requests().await.unwrap();
    let patch = requests
        .iter()
        .find(|r| r.method == "PATCH" && r.url.path() == "/configs")
        .expect("PATCH /configs must be sent");
    let body: serde_json::Value = serde_json::from_slice(&patch.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "tun": {
                "enable": true,
                "stack": "gVisor",
                "device": "Meta",
                "auto-route": true
            }
        })
    );
}

#[tokio::test]
async fn test_toggle_tun_disable_patch_failure_aborts_without_system_proxy() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: true,
            stack: Some("gVisor".into()),
            device: None,
            auto_route: None,
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(true));
    assert!(
        !s.system_proxy_enabled,
        "system proxy must not be set on failure"
    );
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("Failed to disable TUN")));
}

#[tokio::test]
async fn test_toggle_tun_enable_patch_failure_logs_error() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: false,
            stack: None,
            device: None,
            auto_route: None,
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(false));
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("Failed to enable TUN")));
}

#[tokio::test]
async fn test_toggle_tun_disable_without_mixed_port_logs_error() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, false).await;

    let client = MihomoClient::new(&server.uri(), None).unwrap();
    let mut s = AppState::new();
    s.tun = Some(TunConfig {
        enable: true,
        stack: Some("gVisor".into()),
        device: None,
        auto_route: None,
    });
    s.mixed_port = None;
    s.client = Some(client.clone());
    let shared: SharedState = Arc::new(TokioMutex::new(s));

    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(false));
    assert!(!s.system_proxy_enabled);
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("mixed-port unknown")));
}

#[tokio::test]
async fn test_toggle_tun_enable_but_tun_did_not_start_logs_error() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, false).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: false,
            stack: None,
            device: None,
            auto_route: None,
        },
    );
    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(false));
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("TUN did not start")));
}

#[tokio::test]
async fn test_toggle_tun_enable_failure_persists_config_for_restart_recovery() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, false).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: false,
            stack: None,
            device: None,
            auto_route: None,
        },
    );
    let config_path = {
        let s = shared.lock().await;
        s.config.mihomo.config_path.clone()
    };
    if let Some(parent) = std::path::Path::new(&config_path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&config_path, "tun:\n  enable: false\n  stack: gVisor\n").unwrap();

    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        written.contains("enable: true"),
        "config file must be flipped to true to prepare for restart recovery"
    );
    assert!(written.contains("stack: gVisor"));

    let s = shared.lock().await;
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("automatic recovery")));
    assert!(s.logs.iter().any(|l| l
        .payload
        .contains("run `systemctl --user restart mihomo` manually")));
}

#[tokio::test]
async fn test_toggle_tun_disable_failure_persists_config_for_restart_recovery() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, true).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: true,
            stack: Some("gVisor".into()),
            device: None,
            auto_route: None,
        },
    );
    let config_path = {
        let s = shared.lock().await;
        s.config.mihomo.config_path.clone()
    };
    if let Some(parent) = std::path::Path::new(&config_path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&config_path, "tun:\n  enable: true\n  stack: gVisor\n").unwrap();

    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let s = shared.lock().await;
    assert_eq!(s.tun.as_ref().map(|t| t.enable), Some(true));
    assert!(
        !s.system_proxy_enabled,
        "system proxy must not be enabled when TUN failed to stop"
    );
    assert!(s.ui.loading.is_none());
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("TUN is still running")));
    assert!(s
        .logs
        .iter()
        .any(|l| l.payload.contains("automatic recovery")));
    assert!(s.logs.iter().any(|l| l
        .payload
        .contains("run `systemctl --user restart mihomo` manually")));
    drop(s);

    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        written.contains("enable: false"),
        "config file must be flipped to false to prepare for restart recovery"
    );
}

#[tokio::test]
async fn test_toggle_tun_disable_persists_config_file() {
    let _env = TestEnv::new();
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/configs"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    mount_refresh_endpoints(&server, false).await;

    let shared = make_state(
        &server,
        TunConfig {
            enable: true,
            stack: Some("gVisor".into()),
            device: None,
            auto_route: None,
        },
    );
    let config_path = {
        let s = shared.lock().await;
        s.config.mihomo.config_path.clone()
    };
    if let Some(parent) = std::path::Path::new(&config_path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&config_path, "tun:\n  enable: true\n  stack: gVisor\n").unwrap();

    let client = shared.lock().await.client.clone().unwrap();
    toggle_tun_flow(&shared, client).await;

    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        written.contains("enable: false"),
        "config file must be persisted"
    );
    assert!(
        written.contains("stack: gVisor"),
        "other tun fields preserved"
    );
}
