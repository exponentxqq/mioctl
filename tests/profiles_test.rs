use mioctl::config::mioctl_config::MioctlConfig;
use mioctl::subscription::manager::{SubscriptionManager, UpdateTarget};
use mioctl::subscription::profile::{archive_exists, archive_path, read_archive};
use std::sync::{Mutex, MutexGuard, OnceLock};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const SUB_YAML_A: &str = "proxies:\n  - name: A1\n    type: ss\n    server: 1.1.1.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: pa\nproxy-groups:\n  - name: GA\n    type: select\n    proxies: [A1]\nrules:\n  - MATCH,GA\n";
const SUB_YAML_A2: &str = "proxies:\n  - name: A9\n    type: ss\n    server: 1.1.1.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: pa\nproxy-groups:\n  - name: GA\n    type: select\n    proxies: [A9]\nrules:\n  - MATCH,GA\n";
const SUB_YAML_B: &str = "proxies:\n  - name: B1\n    type: tuic\n    server: 2.2.2.2\n    port: 4430\n    uuid: u1\n    password: pb\nproxy-groups:\n  - name: GB\n    type: select\n    proxies: [B1]\nrules:\n  - MATCH,GB\n";
const SUB_YAML_AUTO: &str = "proxies:\n  - name: P1\n    type: ss\n    server: 9.9.9.9\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: Auto\n    type: select\n    proxies: [P1]\nrules:\n  - MATCH,Auto\n";
const URI_LIST: &str = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\nss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.5:8388#N2\nss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.6:8388#N3\n";

fn b64(content: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(content)
}

struct TestEnv {
    _dir: tempfile::TempDir,
    _guard: MutexGuard<'static, ()>,
    mihomo_path: std::path::PathBuf,
}

impl TestEnv {
    fn new(mihomo_yaml: &str) -> Self {
        let guard = lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
        unsafe { std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1") };
        let mihomo_path = dir.path().join("mihomo.yaml");
        std::fs::write(&mihomo_path, mihomo_yaml).unwrap();
        TestEnv {
            _dir: dir,
            _guard: guard,
            mihomo_path,
        }
    }

    fn config(&self) -> MioctlConfig {
        let mut config = MioctlConfig::default();
        config.mihomo.config_path = self.mihomo_path.to_string_lossy().into_owned();
        config.mihomo.external_controller = "127.0.0.1:1".into();
        config
    }

    fn written(&self) -> String {
        std::fs::read_to_string(&self.mihomo_path).unwrap()
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var("MIOCTL_HOME");
            std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL");
        }
    }
}

async fn serve(mock: &MockServer, path_str: &str, body: String) {
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(mock)
        .await;
}

async fn serve_scoped(mock: &MockServer, path_str: &str, body: String) -> wiremock::MockGuard {
    Mock::given(method("GET"))
        .and(path(path_str))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount_as_scoped(mock)
        .await
}

#[tokio::test]
async fn full_lifecycle_add_use_update_remove() {
    let mock = MockServer::start().await;
    let a_guard = serve_scoped(&mock, "/sub/a", SUB_YAML_A.to_string()).await;
    serve(&mock, "/sub/b", SUB_YAML_B.to_string()).await;
    let env = TestEnv::new("mixed-port: 7897\nmode: rule\n");
    let mut config = env.config();

    let summary = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/a", mock.uri()),
        Some("subA".into()),
        true,
        false,
    )
    .await
    .unwrap();
    assert!(summary.contains("(activated)"));
    assert_eq!(config.subscriptions.active.as_deref(), Some("subA"));
    let written = env.written();
    assert!(written.contains("name: A1"));
    assert!(written.contains("proxy-groups:"));
    assert!(written.contains("MATCH,GA"));
    assert!(written.contains("mixed-port: 7897"));

    let summary = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/b", mock.uri()),
        Some("subB".into()),
        true,
        false,
    )
    .await
    .unwrap();
    assert!(summary.contains("(not active"));
    assert_eq!(config.subscriptions.active.as_deref(), Some("subA"));
    let written = env.written();
    assert!(written.contains("name: A1"));
    assert!(!written.contains("name: B1"));

    drop(a_guard);
    serve(&mock, "/sub/a", SUB_YAML_A2.to_string()).await;

    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("subA".into()))
        .await
        .unwrap();
    let result = report.lines.join("\n");
    assert!(!report.failed);
    assert!(result.contains("subA: 1 nodes updated"));
    assert!(result.contains("subA: re-merged into mihomo config"));
    assert!(read_archive("subA").unwrap().contains("name: A9"));
    let written = env.written();
    assert!(written.contains("name: A9"));
    assert!(!written.contains("name: A1"));

    SubscriptionManager::use_profile(&mut config, "subB", true)
        .await
        .unwrap();
    assert_eq!(config.subscriptions.active.as_deref(), Some("subB"));
    let written = env.written();
    assert!(written.contains("name: B1"));
    assert!(written.contains("type: tuic"));
    assert!(!written.contains("name: A9"));
    assert!(written.contains("mixed-port: 7897"));

    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("subA".into()))
        .await
        .unwrap();
    let result = report.lines.join("\n");
    assert!(!report.failed);
    assert!(result.contains("subA: 1 nodes updated"));
    assert!(!result.contains("re-merged"));
    assert!(read_archive("subA").unwrap().contains("name: A9"));
    let written = env.written();
    assert!(written.contains("name: B1"));
    assert!(!written.contains("name: A9"));

    SubscriptionManager::remove(&mut config, "subB")
        .await
        .unwrap();
    assert_eq!(config.subscriptions.active, None);
    let written = env.written();
    assert!(written.contains("proxies: []"));
    assert!(written.contains("proxy-groups: []"));
    assert!(written.contains("MATCH,DIRECT"));
    assert!(written.contains("mixed-port: 7897"));
    assert!(config.find_subscription("subA").is_some());
    assert!(!archive_exists("subB"));

    SubscriptionManager::use_profile(&mut config, "subA", true)
        .await
        .unwrap();
    assert!(env.written().contains("name: A9"));
}

#[tokio::test]
async fn add_base64_uri_list_generates_archive() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/b64", b64(URI_LIST)).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/b64", mock.uri()),
        Some("b64sub".into()),
        true,
        true,
    )
    .await
    .unwrap();

    assert_eq!(
        config.find_subscription("b64sub").unwrap().node_count,
        Some(3)
    );
    let archive = read_archive("b64sub").unwrap();
    assert!(archive.contains("name: N1"));
    assert!(archive.contains("name: N2"));
    assert!(archive.contains("name: b64sub"));
    assert!(archive.contains("MATCH,b64sub"));
    assert!(env.written().contains("name: N1"));
}

#[tokio::test]
async fn add_base64_yaml_subscription_preserves_groups() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/yamlb64", b64(SUB_YAML_A)).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/yamlb64", mock.uri()),
        Some("yamlb64".into()),
        true,
        true,
    )
    .await
    .unwrap();

    assert_eq!(
        config.find_subscription("yamlb64").unwrap().node_count,
        Some(1)
    );
    let archive = read_archive("yamlb64").unwrap();
    assert!(archive.contains("name: A1"));
    assert!(archive.contains("name: GA"));
    assert!(archive.contains("MATCH,GA"));
    assert!(env.written().contains("name: A1"));
}

#[tokio::test]
async fn add_auto_detects_name_and_suffixes_collisions() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/auto", SUB_YAML_AUTO.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/auto", mock.uri()),
        None,
        true,
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        config.subscriptions.items[0].name, "Auto",
        "name should be detected from first proxy-group"
    );
    assert_eq!(config.subscriptions.active.as_deref(), Some("Auto"));

    let summary = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/auto", mock.uri()),
        None,
        true,
        false,
    )
    .await
    .unwrap();
    assert!(summary.contains("(not active"));
    assert_eq!(
        config.subscriptions.items[1].name, "Auto (2)",
        "second add should get a collision suffix"
    );
    assert!(archive_exists("Auto (2)"));
    assert_eq!(config.subscriptions.active.as_deref(), Some("Auto"));
}

#[tokio::test]
async fn add_duplicate_explicit_name_fails() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/a", SUB_YAML_A.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/a", mock.uri()),
        Some("dup".into()),
        true,
        false,
    )
    .await
    .unwrap();
    let err = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/a", mock.uri()),
        Some("dup".into()),
        true,
        false,
    )
    .await
    .unwrap_err();
    assert!(err.contains("already exists"));
    assert_eq!(config.subscriptions.items.len(), 1);
}

#[tokio::test]
async fn ensure_archived_refetches_missing_archive() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/migra", SUB_YAML_A.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/migra", mock.uri()),
        Some("migra".into()),
        true,
        false,
    )
    .await
    .unwrap();
    std::fs::remove_file(archive_path("migra")).unwrap();
    assert!(!archive_exists("migra"));

    let warnings = SubscriptionManager::ensure_archived(&mut config).await;
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("archived profile 'migra'"));
    assert!(archive_exists("migra"));
    assert!(read_archive("migra").unwrap().contains("name: A1"));
    assert_eq!(
        config.find_subscription("migra").unwrap().node_count,
        Some(1)
    );

    let warnings = SubscriptionManager::ensure_archived(&mut config).await;
    assert!(warnings.is_empty());
}

#[tokio::test]
async fn register_adds_without_activation_like_cli_register() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/first", SUB_YAML_A.to_string()).await;
    serve(&mock, "/sub/second", SUB_YAML_B.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();

    SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/first", mock.uri()),
        Some("first".into()),
        true,
        false,
    )
    .await
    .unwrap();

    let summary = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub/second", mock.uri()),
        Some("second".into()),
        true,
        false,
    )
    .await
    .unwrap();

    assert!(summary.contains("(not active"));
    assert_eq!(config.subscriptions.active.as_deref(), Some("first"));
    assert!(archive_exists("second"));
    let written = env.written();
    assert!(written.contains("name: A1"));
    assert!(!written.contains("name: B1"));
}

const NON_NORMALIZABLE_LIST: &str = "- alpha\n- beta\n- gamma\n";

#[tokio::test]
async fn update_normalize_failure_reports_error_line() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/garbage", NON_NORMALIZABLE_LIST.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();
    config.add_subscription("bad".into(), format!("{}/sub/garbage", mock.uri()));
    config.subscriptions.items[0].node_count = Some(2);

    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("bad".into()))
        .await
        .unwrap();
    let result = report.lines.join("\n");
    assert!(report.failed);
    assert!(result.contains("bad: ERROR -"), "got: {}", result);
    assert!(result.contains("no parsable nodes"));
    assert_eq!(config.subscriptions.items[0].node_count, Some(2));
    assert!(!archive_exists("bad"));
}

#[tokio::test]
async fn ensure_archived_normalize_failure_warns() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/garbage", NON_NORMALIZABLE_LIST.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    let mut config = env.config();
    config.add_subscription("bad".into(), format!("{}/sub/garbage", mock.uri()));

    let warnings = SubscriptionManager::ensure_archived(&mut config).await;

    assert_eq!(warnings.len(), 1, "got: {:?}", warnings);
    assert!(warnings[0].contains("has no archive"));
    assert!(warnings[0].contains("could not be normalized"));
    assert!(warnings[0].contains("no parsable nodes"));
    assert!(!archive_exists("bad"));
}

#[tokio::test]
async fn update_save_failure_is_visible() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/a", SUB_YAML_A.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    std::fs::create_dir(MioctlConfig::config_path()).unwrap();
    let mut config = env.config();
    config.add_subscription("s".into(), format!("{}/sub/a", mock.uri()));

    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("s".into()))
        .await
        .unwrap();
    let result = report.lines.join("\n");
    assert!(report.failed);
    assert!(
        result.contains("config: ERROR - save failed:"),
        "result: {}",
        result
    );
    assert!(archive_exists("s"));
}

#[tokio::test]
async fn ensure_archived_save_failure_warns() {
    let mock = MockServer::start().await;
    serve(&mock, "/sub/a", SUB_YAML_A.to_string()).await;

    let env = TestEnv::new("mixed-port: 7897\n");
    std::fs::create_dir(MioctlConfig::config_path()).unwrap();
    let mut config = env.config();
    config.add_subscription("s".into(), format!("{}/sub/a", mock.uri()));

    let warnings = SubscriptionManager::ensure_archived(&mut config).await;

    assert_eq!(warnings.len(), 2, "got: {:?}", warnings);
    assert!(warnings[0].contains("archived profile 's'"));
    assert!(
        warnings[1].contains("config save failed"),
        "got: {:?}",
        warnings
    );
}
