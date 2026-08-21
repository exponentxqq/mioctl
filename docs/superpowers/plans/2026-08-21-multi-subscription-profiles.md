# 多订阅 Profile 管理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 支持保存多个订阅 profile、单激活切换（`sub list/add/use/update/remove`），并在 TUI 新增订阅管理视图。

**Architecture:** 单 profile 激活模式（参考 clash-verge-rev）：订阅统一转为标准 YAML 存档到 `~/.config/mioctl/profiles/`；激活 = 三段（proxies/proxy-groups/rules）整段透传合并进主 config + reload mihomo；废弃 provider 文件路径。

**Tech Stack:** Rust + tokio + ratatui + clap + serde_yaml/toml + wiremock（集成测试）

## Global Constraints

- 订阅存档目录：`~/.config/mioctl/profiles/`；文件名 = 订阅名（仅 `/` 与控制字符替换为 `_`）
- 自动检测名撞车 → 追加 ` (2)` 递增后缀；显式 `--name` 撞车 → 报错
- 主 config 三段（proxies/proxy-groups/rules）全托 mioctl；PRESERVE_KEYS 基础设施段永不触碰
- 删除激活订阅后主 config 写：`proxies: []`、`proxy-groups: []`、`rules: [MATCH,DIRECT]`
- `sub update` 无参 = 更新激活项；非激活项更新只刷存档不碰主 config
- reload 失败三级回退：API → `systemctl --user restart mihomo` → 提示手动命令；文件系统为事实源，reload 失败不回滚文件
- 删除：`injector.rs`、3 个 provider API 方法、`ProxyProvider`/`ProvidersResponse` 类型、`AppState.proxy_providers` 字段、`providers_dir()`、`update_interval_minutes`（含 settings 视图展示）
- `register` 保留为 `add` 纯别名
- 测试注入：`MIOCTL_HOME` 环境变量覆盖 config 目录
- 每任务结束必须通过：`cargo test`、`cargo clippy -- -D warnings`、`cargo fmt --check`
- 代码不加注释（跟随现有代码库风格）

---

### Task 1: 配置模型扩展 + MIOCTL_HOME 注入

**Files:**
- Modify: `src/config/mioctl_config.rs`

**Interfaces:**
- Produces: `SubscriptionItem { name, url, last_updated, node_count: Option<usize> }`；
  `Subscriptions { active: Option<String>, items }`；
  `MioctlConfig::profiles_dir() -> PathBuf`；`MioctlConfig::set_active(name: Option<&str>)`；
  `MioctlConfig::find_subscription(&self, name: &str) -> Option<&SubscriptionItem>`；
  `MioctlConfig::config_dir()` 读 `MIOCTL_HOME` 环境变量

- [ ] **Step 1: 更新失败测试（改断言 + 新增 roundtrip）**

`src/config/mioctl_config.rs` tests 模块中：

```rust
    #[test]
    fn test_default_config() {
        let config = MioctlConfig::default();
        assert_eq!(config.mihomo.external_controller, "127.0.0.1:9090");
        assert_eq!(config.mihomo.secret, "");
        assert!(config.subscriptions.items.is_empty());
        assert_eq!(config.subscriptions.active, None);
        assert_eq!(
            config.preferences.delay_test_url,
            "https://www.gstatic.com/generate_204"
        );
    }
```

（删除原 `assert_eq!(config.subscriptions.update_interval_minutes, 240);` 行）

新增测试：

```rust
    #[test]
    fn test_active_and_node_count_roundtrip() {
        let mut config = MioctlConfig::default();
        config.add_subscription("my-sub".into(), "https://example.com/sub".into());
        config.subscriptions.items[0].node_count = Some(42);
        config.set_active(Some("my-sub"));
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.subscriptions.active.as_deref(), Some("my-sub"));
        assert_eq!(deserialized.subscriptions.items[0].node_count, Some(42));
    }

    #[test]
    fn test_legacy_config_without_new_fields_loads() {
        let legacy = r#"
[mihomo]
external_controller = "127.0.0.1:9090"
secret = ""
config_path = "/tmp/x.yaml"

[[subscriptions.items]]
name = "old"
url = "https://example.com/sub"
last_updated = "2026-01-01T00:00:00Z"
"#;
        let config: MioctlConfig = toml::from_str(legacy).unwrap();
        assert_eq!(config.subscriptions.items.len(), 1);
        assert_eq!(config.subscriptions.active, None);
        assert_eq!(config.subscriptions.items[0].node_count, None);
    }

    #[test]
    fn test_find_subscription() {
        let mut config = MioctlConfig::default();
        config.add_subscription("a".into(), "https://a".into());
        assert!(config.find_subscription("a").is_some());
        assert!(config.find_subscription("b").is_none());
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib config::mioctl_config`
Expected: FAIL（`active`/`node_count` 字段不存在、`set_active`/`find_subscription` 未定义）

- [ ] **Step 3: 实现**

`src/config/mioctl_config.rs` 修改：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionItem {
    pub name: String,
    pub url: String,
    pub last_updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Subscriptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub items: Vec<SubscriptionItem>,
}
```

删除 `default_update_interval` 函数与手工 `impl Default for Subscriptions`（derive Default 即可）。

`config_dir` 加环境变量覆盖：

```rust
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("MIOCTL_HOME") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mioctl")
    }
```

删除 `providers_dir()`；`save()` 中删除 `std::fs::create_dir_all(Self::providers_dir())...` 行。新增：

```rust
    pub fn profiles_dir() -> PathBuf {
        Self::config_dir().join("profiles")
    }

    pub fn set_active(&mut self, name: Option<&str>) {
        self.subscriptions.active = name.map(|s| s.to_string());
    }

    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionItem> {
        self.subscriptions.items.iter().find(|s| s.name == name)
    }
```

`add_subscription` 补充 `node_count: None` 字段初始化。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib config::mioctl_config && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add src/config/mioctl_config.rs
git commit -m "feat: subscription config model — active mark, node_count, MIOCTL_HOME override"
```

---

### Task 2: profile 存档层 + parser 增强

**Files:**
- Create: `src/subscription/profile.rs`
- Modify: `src/subscription/parser.rs`
- Modify: `src/subscription/mod.rs`
- Modify: `tests/sub_test.rs`（`parse_uri_list` 签名变化）

**Interfaces:**
- Consumes: `parser::{detect_format, parse_subscription_full, SubscriptionFormat, SubscriptionContent}`；现有 `ParsedNode`
- Produces:
  - `parser::parse_uri_list(content: &str) -> Result<(Vec<ParsedNode>, Vec<String>), String>`（第二个值为被跳过的行）
  - `profile::NormalizedProfile { yaml: String, node_count: usize, warnings: Vec<String> }`
  - `profile::sanitize_filename(name: &str) -> String`
  - `profile::archive_path(name: &str) -> PathBuf`
  - `profile::write_archive(name: &str, yaml: &str) -> Result<(), String>`
  - `profile::read_archive(name: &str) -> Result<String, String>`
  - `profile::remove_archive(name: &str) -> Result<(), String>`
  - `profile::archive_exists(name: &str) -> bool`
  - `profile::normalize_to_yaml(sub_name: &str, content: &str) -> Result<NormalizedProfile, String>`

- [ ] **Step 1: 写失败测试（profile.rs 内嵌 tests）**

新建 `src/subscription/profile.rs`，先只写测试框架：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("狗狗加速.com"), "狗狗加速.com");
        assert_eq!(sanitize_filename("a/b\\c"), "a_b\\c");
        assert_eq!(sanitize_filename("x\u{0000}y"), "x_y");
    }

    #[test]
    fn test_normalize_yaml_passthrough_tuic() {
        let content = r#"proxies:
  - name: "🇨🇦26加拿大(Tuic)"
    type: tuic
    server: 26ca.example.com
    port: 4430
    uuid: 03523c1e
    password: 03523c1e
    congestion-control: bbr
    alpn: [h3]
proxy-groups:
  - name: G
    type: select
    proxies: ["🇨🇦26加拿大(Tuic)"]
rules:
  - MATCH,G
"#;
        let p = normalize_to_yaml("test", content).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.warnings.is_empty());
        assert!(p.yaml.contains("type: tuic"));
        assert!(p.yaml.contains("congestion-control: bbr"));
    }

    #[test]
    fn test_normalize_base64_uri_list() {
        let uri_list = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\nvless://uuid@host:443?x=1#Bad\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(uri_list);
        let p = normalize_to_yaml("mysub", &b64).unwrap();
        assert_eq!(p.node_count, 1);
        assert_eq!(p.warnings.len(), 1);
        assert!(p.warnings[0].contains("vless"));
        assert!(p.yaml.contains("name: N1"));
        assert!(p.yaml.contains("name: mysub"));
        assert!(p.yaml.contains("MATCH,mysub"));
    }

    #[test]
    fn test_normalize_base64_embedded_yaml() {
        let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388, cipher: aes-256-gcm, password: p }\n";
        let b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
        let p = normalize_to_yaml("t", &b64).unwrap();
        assert_eq!(p.node_count, 1);
        assert!(p.yaml.contains("name: N1"));
    }

    #[test]
    fn test_archive_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("MIOCTL_HOME", dir.path());
        write_archive("测试 sub", "proxies: []").unwrap();
        assert!(archive_exists("测试 sub"));
        assert_eq!(read_archive("测试 sub").unwrap(), "proxies: []");
        remove_archive("测试 sub").unwrap();
        assert!(!archive_exists("测试 sub"));
        assert!(read_archive("测试 sub").is_err());
        std::env::remove_var("MIOCTL_HOME");
    }
}
```

`tests/sub_test.rs` 中 `parse_uri_list(&content)` 两处调用改为：

```rust
            mioctl::subscription::parser::SubscriptionFormat::PlainUri => {
                eprintln!("Format: Plain URI list");
                mioctl::subscription::parser::parse_uri_list(&content).map(|(nodes, _)| nodes)
            }
```

Base64 分支同理 `.map(|(nodes, _)| nodes)`。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib subscription::profile`
Expected: FAIL（模块函数未定义）

- [ ] **Step 3: 实现 profile.rs**

`src/subscription/profile.rs`：

```rust
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::parser::{
    parse_subscription_full, parse_uri_list, SubscriptionContent,
};
use serde_yaml::{Mapping, Value};

pub struct NormalizedProfile {
    pub yaml: String,
    pub node_count: usize,
    pub warnings: Vec<String>,
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect()
}

pub fn archive_path(name: &str) -> std::path::PathBuf {
    MioctlConfig::profiles_dir().join(format!("{}.yaml", sanitize_filename(name)))
}

pub fn archive_exists(name: &str) -> bool {
    archive_path(name).exists()
}

pub fn write_archive(name: &str, yaml: &str) -> Result<(), String> {
    let path = archive_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, yaml).map_err(|e| e.to_string())
}

pub fn read_archive(name: &str) -> Result<String, String> {
    std::fs::read_to_string(archive_path(name)).map_err(|e| e.to_string())
}

pub fn remove_archive(name: &str) -> Result<(), String> {
    let path = archive_path(name);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn try_base64_decode(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .ok()?;
    String::from_utf8(decoded).ok()
}

fn looks_like_yaml(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("proxies:")
        || t.starts_with("mixed-port:")
        || t.starts_with("port:")
        || t.starts_with("---")
}

fn from_content(sub: SubscriptionContent) -> Result<NormalizedProfile, String> {
    let node_count = sub
        .proxies
        .as_sequence()
        .map(|s| s.len())
        .unwrap_or(0);
    let mut out = Mapping::new();
    out.insert(Value::String("proxies".into()), sub.proxies);
    out.insert(Value::String("proxy-groups".into()), sub.proxy_groups);
    out.insert(Value::String("rules".into()), sub.rules);
    let yaml = serde_yaml::to_string(&Value::Mapping(out)).map_err(|e| e.to_string())?;
    Ok(NormalizedProfile {
        yaml,
        node_count,
        warnings: vec![],
    })
}

fn from_nodes(sub_name: &str, nodes: &[crate::api::types::ParsedNode], skipped: Vec<String>) -> Result<NormalizedProfile, String> {
    if nodes.is_empty() {
        return Err("no parsable nodes found in subscription".into());
    }
    let (proxies, proxy_groups, rules) = nodes_to_subscription_content(sub_name, nodes);
    let mut out = Mapping::new();
    out.insert(Value::String("proxies".into()), proxies);
    out.insert(Value::String("proxy-groups".into()), proxy_groups);
    out.insert(Value::String("rules".into()), rules);
    let yaml = serde_yaml::to_string(&Value::Mapping(out)).map_err(|e| e.to_string())?;
    let warnings = if skipped.is_empty() {
        vec![]
    } else {
        vec![format!(
            "skipped {} unsupported entries: {}",
            skipped.len(),
            skipped.join(", ")
        )]
    };
    Ok(NormalizedProfile {
        yaml,
        node_count: nodes.len(),
        warnings,
    })
}

pub fn normalize_to_yaml(sub_name: &str, content: &str) -> Result<NormalizedProfile, String> {
    if let Ok(sub) = parse_subscription_full(content) {
        return from_content(sub);
    }
    if let Some(decoded) = try_base64_decode(content) {
        if looks_like_yaml(&decoded) {
            if let Ok(sub) = parse_subscription_full(&decoded) {
                return from_content(sub);
            }
        }
        let (nodes, skipped) = parse_uri_list(&decoded)?;
        return from_nodes(sub_name, &nodes, skipped);
    }
    let (nodes, skipped) = parse_uri_list(content)?;
    from_nodes(sub_name, &nodes, skipped)
}

fn nodes_to_subscription_content(
    name: &str,
    nodes: &[crate::api::types::ParsedNode],
) -> (Value, Value, Value) {
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
    group.insert(Value::String("type".into()), Value::String("select".into()));
    let node_names: Vec<Value> = nodes
        .iter()
        .map(|n| Value::String(n.name.clone()))
        .collect();
    group.insert(Value::String("proxies".into()), Value::Sequence(node_names));
    let proxy_groups = Value::Sequence(vec![Value::Mapping(group)]);
    let rules = Value::Sequence(vec![Value::String(format!("MATCH,{}", name))]);
    (proxies, proxy_groups, rules)
}
```

（`nodes_to_subscription_content` 从 manager.rs 复制而来，Task 3 重写 manager 时删除原版）

- [ ] **Step 4: 修改 parser.rs parse_uri_list 返回跳过行**

```rust
pub fn parse_uri_list(content: &str) -> Result<(Vec<ParsedNode>, Vec<String>), String> {
    let mut nodes = Vec::new();
    let mut skipped = Vec::new();
    for line in content.trim().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_single_uri(line) {
            Some(node) => nodes.push(node),
            None => skipped.push(line.to_string()),
        }
    }
    Ok((nodes, skipped))
}
```

parser.rs 内使用 `parse_uri_list` 的现有测试改为解构元组（如 `let (nodes, _) = parse_uri_list(...).unwrap();`）。`SubscriptionFormat` 枚举加 `#[derive(Debug, PartialEq)]`（tests 已用 matches!，无需；跳过）。

`src/subscription/mod.rs` 加 `pub mod profile;`。

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test --lib subscription:: && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全部 PASS（注意：`test_archive_roundtrip` 设置/清除 `MIOCTL_HOME`，若与并行测试冲突，在该测试上加 `#[ignore]` 并由集成测试串行覆盖——默认应无冲突，因其他测试不 touch `config_dir()`）

- [ ] **Step 6: Commit**

```bash
git add src/subscription/profile.rs src/subscription/parser.rs src/subscription/mod.rs tests/sub_test.rs
git commit -m "feat: profile archive layer — normalize any subscription format to standard YAML"
```

---

### Task 3: SubscriptionManager 重构

**Files:**
- Modify: `src/subscription/manager.rs`（重写）

**Interfaces:**
- Consumes: Task 1 的 `set_active`/`find_subscription`/`profiles_dir`；Task 2 的 `profile::*`；`merger::{backup_file, merge_mihomo_config, rollback_file, write_config}`；`fetcher::fetch_with_ua_probe`；`parser::{detect_subscription_name, name_from_url}`
- Produces:
  - `SubscriptionManager::add(config: &mut MioctlConfig, url: &str, name: Option<String>, no_reload: bool, activate: bool) -> Result<String, String>`
  - `SubscriptionManager::use_profile(config: &mut MioctlConfig, name: &str, no_reload: bool) -> Result<String, String>`
  - `enum UpdateTarget { Active, Named(String), All }`（manager 模块内）
  - `SubscriptionManager::update(config: &mut MioctlConfig, target: &UpdateTarget) -> Result<String, String>`
  - `SubscriptionManager::remove(config: &mut MioctlConfig, name: &str) -> Result<String, String>`
  - `SubscriptionManager::list(config: &MioctlConfig) -> String`
  - `SubscriptionManager::ensure_archived(config: &mut MioctlConfig) -> Vec<String>`
  - 兼容层（Task 5 删除）：`register`、`update_all`、`update_one`

- [ ] **Step 1: 写失败测试（manager.rs 内嵌 tests）**

manager.rs tests 模块新增（利用 `MIOCTL_HOME` + tempdir；fetch 无法在单测中模拟，测不涉及网络的函数：unique_name、activate 从存档、remove 写空三段）：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct TestEnv {
        _dir: tempfile::TempDir,
    }

    impl TestEnv {
        fn new(mihomo_yaml: &str) -> (Self, MioctlConfig) {
            let dir = tempfile::tempdir().unwrap();
            std::env::set_var("MIOCTL_HOME", dir.path());
            let mihomo_path = dir.path().join("mihomo-config.yaml");
            std::fs::write(&mihomo_path, mihomo_yaml).unwrap();
            let mut config = MioctlConfig::default();
            config.mihomo.config_path = mihomo_path.to_string_lossy().into_owned();
            ((TestEnv { _dir: dir }), config)
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            std::env::remove_var("MIOCTL_HOME");
        }
    }

    const SUB_YAML: &str = "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G\n";

    #[test]
    fn test_unique_name_appends_suffix() {
        let mut names = vec!["base".to_string()];
        assert_eq!(unique_name("base", &names), "base (2)");
        names.push("base (2)".to_string());
        assert_eq!(unique_name("base", &names), "base (3)");
        assert_eq!(unique_name("other", &names), "other");
    }

    #[tokio::test]
    async fn test_use_profile_writes_three_sections() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\ndns:\n  enable: true\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        let result = SubscriptionManager::use_profile(&mut config, "sub1", true).await;
        assert!(result.is_ok(), "{:?}", result);
        assert_eq!(config.subscriptions.active.as_deref(), Some("sub1"));
        let written = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        assert!(written.contains("name: N1"));
        assert!(written.contains("MATCH,G"));
        assert!(written.contains("mixed-port: 7897"));
        assert!(written.contains("dns:"));
        drop(env);
    }

    #[tokio::test]
    async fn test_use_profile_missing_archive_fails() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("nope".into(), "https://x".into());
        let err = SubscriptionManager::use_profile(&mut config, "nope", true).await;
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("update"));
        drop(env);
    }

    #[tokio::test]
    async fn test_remove_active_writes_empty_state() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        SubscriptionManager::use_profile(&mut config, "sub1", true).await.unwrap();
        SubscriptionManager::remove(&mut config, "sub1").await.unwrap();
        assert!(config.subscriptions.items.is_empty());
        assert_eq!(config.subscriptions.active, None);
        let written = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        assert!(written.contains("proxies: []"));
        assert!(written.contains("MATCH,DIRECT"));
        assert!(written.contains("mixed-port: 7897"));
        assert!(!crate::subscription::profile::archive_exists("sub1"));
        drop(env);
    }

    #[tokio::test]
    async fn test_remove_inactive_keeps_active_config() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("act".into(), "https://x".into());
        config.add_subscription("other".into(), "https://y".into());
        crate::subscription::profile::write_archive("act", SUB_YAML).unwrap();
        crate::subscription::profile::write_archive("other", SUB_YAML).unwrap();
        SubscriptionManager::use_profile(&mut config, "act", true).await.unwrap();
        SubscriptionManager::remove(&mut config, "other").await.unwrap();
        assert_eq!(config.subscriptions.active.as_deref(), Some("act"));
        assert!(config.find_subscription("act").is_some());
        drop(env);
    }

    #[test]
    fn test_list_output() {
        let (_, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("sub1".into(), "https://x".into());
        config.subscriptions.items[0].node_count = Some(10);
        config.set_active(Some("sub1"));
        config.add_subscription("sub2".into(), "https://y".into());
        let out = SubscriptionManager::list(&config);
        assert!(out.contains("sub1"));
        assert!(out.contains("sub2"));
        assert!(out.contains('*'));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib subscription::manager`
Expected: FAIL（`unique_name`、`use_profile`、`remove`、`list` 未定义）

- [ ] **Step 3: 重写 manager.rs**

完整新文件内容：

```rust
use crate::api::client::MihomoClient;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::fetcher::fetch_with_ua_probe;
use crate::subscription::merger::{backup_file, merge_mihomo_config, rollback_file, write_config};
use crate::subscription::parser::{detect_subscription_name, name_from_url};
use crate::subscription::profile::{
    archive_exists, normalize_to_yaml, read_archive, remove_archive, write_archive,
    NormalizedProfile,
};
use serde_yaml::{Mapping, Value};

pub enum UpdateTarget {
    Active,
    Named(String),
    All,
}

pub struct SubscriptionManager;

fn client_for(config: &MioctlConfig) -> Result<MihomoClient, String> {
    let secret = if config.mihomo.secret.is_empty() {
        None
    } else {
        Some(config.mihomo.secret.clone())
    };
    MihomoClient::new(&config.mihomo.external_controller, secret)
        .map_err(|e| format!("could not connect to mihomo: {}", e))
}

fn unique_name(base: &str, existing: &[String]) -> String {
    if !existing.iter().any(|n| n == base) {
        return base.to_string();
    }
    let mut i = 2;
    loop {
        let candidate = format!("{} ({})", base, i);
        if !existing.iter().any(|n| n == &candidate) {
            return candidate;
        }
        i += 1;
    }
}

async fn fetch_version(config: &MioctlConfig) -> Option<String> {
    match client_for(config) {
        Ok(c) => c.get_version().await.ok().map(|v| v.version),
        Err(_) => None,
    }
}

async fn reload_mihomo(config: &MioctlConfig) -> String {
    match client_for(config) {
        Ok(c) => match c.reload_config(None).await {
            Ok(_) => "mihomo reloaded successfully".into(),
            Err(e) => {
                let base = format!("mihomo API reload failed: {}. Trying systemctl fallback...", e);
                let status = std::process::Command::new("systemctl")
                    .args(["--user", "restart", "mihomo"])
                    .output();
                match status {
                    Ok(o) if o.status.success() => format!("{} systemctl restart succeeded.", base),
                    _ => format!(
                        "{} systemctl restart also failed. Run: systemctl --user restart mihomo",
                        base
                    ),
                }
            }
        },
        Err(e) => e,
    }
}

async fn activate(config: &MioctlConfig, name: &str, no_reload: bool) -> Result<String, String> {
    let archive = read_archive(name).map_err(|_| {
        format!(
            "profile archive for '{}' is missing or unreadable — run `mioctl sub update {}` first",
            name, name
        )
    })?;
    let sub = crate::subscription::parser::parse_subscription_full(&archive)?;

    let config_path = config.mihomo.config_path.clone();
    backup_file(&config_path)?;
    match merge_mihomo_config(&config_path, &sub.proxies, &sub.proxy_groups, &sub.rules) {
        Ok(r) => {
            if let Err(e) = write_config(&config_path, &r.yaml) {
                rollback_file(&config_path).ok();
                return Err(format!("failed to write config: {}", e));
            }
            let reload_msg = if no_reload {
                "reload skipped".to_string()
            } else {
                reload_mihomo(config).await
            };
            Ok(format!(
                "Switched to '{}'.\n  {} proxies, {} groups, {} rules\n  {}",
                name, r.proxy_count, r.group_count, r.rule_count, reload_msg
            ))
        }
        Err(e) => {
            rollback_file(&config_path).ok();
            Err(e)
        }
    }
}

fn write_empty_state(config: &MioctlConfig) -> Result<(), String> {
    let config_path = config.mihomo.config_path.clone();
    let proxies = Value::Sequence(vec![]);
    let groups = Value::Sequence(vec![]);
    let rules = Value::Sequence(vec![Value::String("MATCH,DIRECT".into())]);
    backup_file(&config_path)?;
    let result = merge_mihomo_config(&config_path, &proxies, &groups, &rules)?;
    write_config(&config_path, &result.yaml)
}

impl SubscriptionManager {
    pub async fn add(
        config: &mut MioctlConfig,
        url: &str,
        name: Option<String>,
        no_reload: bool,
        activate_flag: bool,
    ) -> Result<String, String> {
        let version = fetch_version(config).await;
        let content = fetch_with_ua_probe(url, version).await?;

        let existing: Vec<String> = config
            .subscriptions
            .items
            .iter()
            .map(|s| s.name.clone())
            .collect();

        let final_name = match name {
            Some(n) => {
                if existing.iter().any(|e| e == &n) {
                    return Err(format!(
                        "subscription '{}' already exists. Remove it first or use a different --name.",
                        n
                    ));
                }
                n
            }
            None => {
                let base = detect_subscription_name(&content)
                    .or_else(|_| name_from_url(url))?;
                unique_name(&base, &existing)
            }
        };

        let normalized = normalize_to_yaml(&final_name, &content)?;
        if normalized.node_count == 0 {
            return Err("no proxies found in subscription".into());
        }
        write_archive(&final_name, &normalized.yaml)?;

        let is_first = config.subscriptions.items.is_empty();
        config.add_subscription(final_name.clone(), url.to_string());
        if let Some(item) = config
            .subscriptions
            .items
            .iter_mut()
            .find(|s| s.name == final_name)
        {
            item.node_count = Some(normalized.node_count);
            item.last_updated = Some(chrono::Utc::now().to_rfc3339());
        }

        let mut summary = format!(
            "Subscription '{}' added. {} proxies archived.",
            final_name, normalized.node_count
        );
        for w in &normalized.warnings {
            summary.push_str(&format!("\n  warning: {}", w));
        }

        if is_first || activate_flag {
            config.set_active(Some(&final_name));
            let msg = activate(config, &final_name, no_reload).await?;
            summary.push_str(&format!("\n  {}", msg));
            summary.push_str("\n  (activated)");
        } else {
            summary.push_str("\n  (not active — run `mioctl sub use` to switch)");
        }

        config
            .save()
            .map_err(|e| format!("subscription archived but config save failed: {}", e))?;
        Ok(summary)
    }

    pub async fn use_profile(
        config: &mut MioctlConfig,
        name: &str,
        no_reload: bool,
    ) -> Result<String, String> {
        if config.find_subscription(name).is_none() {
            return Err(format!("no subscription named '{}'", name));
        }
        let msg = activate(config, name, no_reload).await?;
        config.set_active(Some(name));
        config
            .save()
            .map_err(|e| format!("activated but config save failed: {}", e))?;
        Ok(msg)
    }

    pub async fn update(
        config: &mut MioctlConfig,
        target: &UpdateTarget,
    ) -> Result<String, String> {
        let names: Vec<String> = match target {
            UpdateTarget::All => config.subscriptions.items.iter().map(|s| s.name.clone()).collect(),
            UpdateTarget::Named(n) => {
                if config.find_subscription(n).is_none() {
                    return Err(format!("no subscription named '{}'", n));
                }
                vec![n.clone()]
            }
            UpdateTarget::Active => {
                match config.subscriptions.active.clone() {
                    Some(a) => vec![a],
                    None => {
                        return Err(
                            "no active subscription — specify a name or use --all".into()
                        )
                    }
                }
            }
        };

        let version = fetch_version(config).await;
        let now = chrono::Utc::now().to_rfc3339();
        let mut results = Vec::new();
        let mut need_save = false;

        for name in names {
            let url = config
                .find_subscription(&name)
                .map(|s| s.url.clone())
                .unwrap_or_default();
            let fetch_result = fetch_with_ua_probe(&url, version.clone()).await;
            match fetch_result {
                Ok(content) => match normalize_to_yaml(&name, &content) {
                    Ok(normalized) => {
                        if let Err(e) = write_archive(&name, &normalized.yaml) {
                            results.push(format!("{}: ERROR - {}", name, e));
                            continue;
                        }
                        if let Some(item) = config
                            .subscriptions
                            .items
                            .iter_mut()
                            .find(|s| s.name == name)
                        {
                            item.node_count = Some(normalized.node_count);
                            item.last_updated = Some(now.clone());
                        }
                        need_save = true;
                        let mut line = format!("{}: {} nodes updated", name, normalized.node_count);
                        for w in &normalized.warnings {
                            line.push_str(&format!(" (warning: {})", w));
                        }
                        results.push(line);

                        if config.subscriptions.active.as_deref() == Some(name.as_str()) {
                            match activate(config, &name, false).await {
                                Ok(_) => results.push(format!("{}: re-merged into mihomo config", name)),
                                Err(e) => results.push(format!("{}: re-merge failed - {}", name, e)),
                            }
                        }
                    }
                    Err(e) => results.push(format!("{}: ERROR - {}", name, e)),
                },
                Err(e) => results.push(format!("{}: ERROR - {}", name, e)),
            }
        }

        if need_save {
            let _ = config.save();
        }
        Ok(results.join("\n"))
    }

    pub async fn remove(config: &mut MioctlConfig, name: &str) -> Result<String, String> {
        if config.find_subscription(name).is_none() {
            return Err(format!("no subscription named '{}'", name));
        }
        let was_active = config.subscriptions.active.as_deref() == Some(name);
        config.remove_subscription(name);
        remove_archive(name)?;

        let mut msg = format!("Subscription '{}' removed.", name);
        if was_active {
            config.set_active(None);
            write_empty_state(config)?;
            msg.push_str("\n  active subscription was removed — mihomo config reset to empty state (MATCH,DIRECT)");
            msg.push_str(&format!("\n  {}", reload_mihomo(config).await));
        }
        config
            .save()
            .map_err(|e| format!("removed but config save failed: {}", e))?;
        Ok(msg)
    }

    pub fn list(config: &MioctlConfig) -> String {
        if config.subscriptions.items.is_empty() {
            return "No subscriptions. Add one: mioctl sub add <url>".into();
        }
        let mut out = String::from("  NAME                 NODES  LAST UPDATED\n");
        for item in &config.subscriptions.items {
            let mark = if config.subscriptions.active.as_deref() == Some(item.name.as_str()) {
                "*"
            } else {
                " "
            };
            let updated = item.last_updated.as_deref().unwrap_or("(never)");
            out.push_str(&format!(
                "{} {:20} {:>5}  {}\n",
                mark, item.name, item.node_count.unwrap_or(0), updated
            ));
        }
        out
    }

    pub async fn ensure_archived(config: &mut MioctlConfig) -> Vec<String> {
        let mut warnings = Vec::new();
        let legacy_providers = crate::config::mioctl_config::MioctlConfig::config_dir().join("providers");
        if legacy_providers.exists() {
            let _ = std::fs::remove_dir_all(&legacy_providers);
            warnings.push("removed legacy providers/ directory".into());
        }

        let missing: Vec<(String, String)> = config
            .subscriptions
            .items
            .iter()
            .filter(|s| !archive_exists(&s.name))
            .map(|s| (s.name.clone(), s.url.clone()))
            .collect();
        if missing.is_empty() {
            return warnings;
        }

        let version = fetch_version(config).await;
        for (name, url) in missing {
            match fetch_with_ua_probe(&url, version.clone()).await {
                Ok(content) => match normalize_to_yaml(&name, &content) {
                    Ok(normalized) => {
                        let _ = write_archive(&name, &normalized.yaml);
                        if let Some(item) = config
                            .subscriptions
                            .items
                            .iter_mut()
                            .find(|s| s.name == name)
                        {
                            item.node_count = Some(normalized.node_count);
                        }
                        warnings.push(format!("archived profile '{}' ({} nodes)", name, normalized.node_count));
                    }
                    Err(e) => warnings.push(format!(
                        "profile '{}' has no archive and fetch failed: {} — run `mioctl sub update {}`",
                        name, e, name
                    )),
                },
                Err(e) => warnings.push(format!(
                    "profile '{}' has no archive and fetch failed: {} — run `mioctl sub update {}`",
                    name, e, name
                )),
            }
        }
        let _ = config.save();
        warnings
    }

    pub async fn register(
        config: &mut MioctlConfig,
        url: String,
        name: Option<String>,
        no_reload: bool,
    ) -> Result<String, String> {
        Self::add(config, &url, name, no_reload, false).await
    }

    pub async fn update_all(config: &mut MioctlConfig, _client: &MihomoClient) -> Result<String, String> {
        Self::update(config, &UpdateTarget::All).await
    }
}
```

**注意**：`activate` 直接使用 `merge_mihomo_config` 返回的 `MergerResult` 的 `proxy_count`/`group_count`/`rule_count` 计数（见上方实现）。删除旧 `register`/`update_all`/`update_one` 实现与 `nodes_to_subscription_content`（已移至 profile.rs），删除文件末尾 `use crate::subscription::parser::SubscriptionContent;`。保留新 `register`/`update_all` 兼容转发（CLI 尚未切换，Task 5 删）。`Mapping` import 若未用则删。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test --lib subscription:: && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全部 PASS（`update_one` 删除后若 injector 路径报 unused 警告，属预期——Task 4 清理）

- [ ] **Step 5: Commit**

```bash
git add src/subscription/manager.rs
git commit -m "feat: rewrite SubscriptionManager — add/use/update/remove/list + ensure_archived migration"
```

---

### Task 4: 清理废弃的 provider 路径

**Files:**
- Delete: `src/subscription/injector.rs`
- Modify: `src/subscription/mod.rs`
- Modify: `src/api/endpoints.rs:164-193`
- Modify: `src/api/types.rs:172-191`
- Modify: `src/app/state.rs`（删 `proxy_providers` 字段）

**Interfaces:**
- Consumes: 无
- Produces: 无（纯删除）

- [ ] **Step 1: 删除代码**

1. `rm src/subscription/injector.rs`，`mod.rs` 删 `pub mod injector;`
2. `endpoints.rs` 删除 `// --- Providers ---` 段的三个方法：`get_proxy_providers`、`update_proxy_provider`、`healthcheck_proxy_provider`
3. `types.rs` 删除 `// === Proxy Provider ===` 段：`ProxyProvider` struct 与 `ProvidersResponse` struct
4. `state.rs` 删除 `pub proxy_providers: std::collections::HashMap<String, ProxyProvider>,` 字段及 `AppState::new()` 中对应初始化行

- [ ] **Step 2: 全量验证**

Run: `cargo build && cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 编译通过，无 unused 警告，测试全 PASS

- [ ] **Step 3: Commit**

```bash
git add -A src/subscription src/api src/app
git commit -m "refactor: remove dead proxy-provider file path (injector, provider API, types)"
```

---

### Task 5: CLI 命令

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/cli/sub.rs`

**Interfaces:**
- Consumes: Task 3 的 `SubscriptionManager::{add, use_profile, update, remove, list, ensure_archived}`、`UpdateTarget`
- Produces: CLI 子命令 `sub list/add/use/update/remove`（`register` 为 `add` 别名）；删除 manager 兼容层 `register`/`update_all`

- [ ] **Step 1: 写 CLI 定义**

`src/cli/mod.rs` 替换 `SubAction`：

```rust
#[derive(Subcommand)]
pub enum SubAction {
    /// List all subscriptions (* = active)
    List,
    /// Add a new subscription
    Add {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after activation
        #[arg(long)]
        no_reload: bool,
        /// Activate immediately even if another subscription is active
        #[arg(long)]
        activate: bool,
    },
    /// Register a new subscription (alias of add)
    Register {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after activation
        #[arg(long)]
        no_reload: bool,
    },
    /// Switch the active subscription
    Use {
        /// Subscription name
        name: String,
        /// Skip mihomo reload
        #[arg(long)]
        no_reload: bool,
    },
    /// Update subscriptions (default: active; --all for every; or name one)
    Update {
        /// Subscription name (omit for active)
        name: Option<String>,
        /// Update all subscriptions
        #[arg(long)]
        all: bool,
    },
    /// Remove a subscription
    Remove {
        /// Subscription name
        name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}
```

- [ ] **Step 2: 重写 sub.rs**

```rust
use crate::cli::SubAction;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::manager::{SubscriptionManager, UpdateTarget};
use std::io::IsTerminal;

pub async fn run(action: SubAction) {
    let mut config = MioctlConfig::load();
    for w in SubscriptionManager::ensure_archived(&mut config).await {
        eprintln!("{}", w);
    }
    match action {
        SubAction::List => {
            println!("{}", SubscriptionManager::list(&config));
        }
        SubAction::Add {
            url,
            name,
            no_reload,
            activate,
        } => {
            match SubscriptionManager::add(&mut config, &url, name, no_reload, activate).await {
                Ok(summary) => println!("{}", summary),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        SubAction::Register { url, name, no_reload } => {
            match SubscriptionManager::add(&mut config, &url, name, no_reload, false).await {
                Ok(summary) => println!("{}", summary),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        SubAction::Use { name, no_reload } => {
            match SubscriptionManager::use_profile(&mut config, &name, no_reload).await {
                Ok(msg) => println!("{}", msg),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        SubAction::Update { name, all } => {
            let target = if all {
                UpdateTarget::All
            } else if let Some(n) = name {
                UpdateTarget::Named(n)
            } else {
                UpdateTarget::Active
            };
            match SubscriptionManager::update(&mut config, &target).await {
                Ok(result) => println!("{}", result),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        SubAction::Remove { name, yes } => {
            let confirmed = yes || {
                if !std::io::stdin().is_terminal() {
                    false
                } else {
                    print!("Remove subscription '{}'? [y/N] ", name);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap_or(0);
                    input.trim().eq_ignore_ascii_case("y")
                }
            };
            if !confirmed {
                println!("Cancelled.");
                return;
            }
            match SubscriptionManager::remove(&mut config, &name).await {
                Ok(msg) => println!("{}", msg),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
    }
}
```

注意：`ensure_archived` 里 fetch 失败只 eprintln 警告不阻断。`list` 无订阅时不触发 fetch（missing 为空直接返回）。

- [ ] **Step 3: 删除 manager.rs 兼容层**

删除 Task 3 保留的 `register` 与 `update_all` 两个转发方法。

- [ ] **Step 4: 构建 + 手动冒烟**

Run: `cargo build && cargo run -- sub list && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 编译通过；`sub list` 显示当前订阅（真实环境的「狗狗加速.com」，且首次会 fetch 生成存档并打印 archived 警告）

- [ ] **Step 5: Commit**

```bash
git add src/cli/mod.rs src/cli/sub.rs src/subscription/manager.rs
git commit -m "feat: sub CLI — list/add/use/update/remove with single-profile activation"
```

---

### Task 6: TUI 状态与按键

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/ui/keybindings.rs`
- Modify: `src/ui/views/settings.rs`（删 update interval 行）

**Interfaces:**
- Consumes: 无
- Produces:
  - `ActiveView::Subscriptions`
  - `UiState { selected_sub_idx: usize, sub_input_mode: bool, sub_input: String, confirm_remove: Option<String> }`
  - `LoadingKind::{SwitchProfile, AddSub}`（`as_str`: `"Switching profile..."` / `"Adding subscription..."`）
  - `Action::{SubUpdate, SubAdd}`
  - 按键：`6` → `SwitchView(5)`、`u` → `SubUpdate`、`a` → `SubAdd`

- [ ] **Step 1: 更新失败测试**

`state.rs` tests 新增：

```rust
    #[test]
    fn test_loading_kind_new_variants() {
        assert_eq!(LoadingKind::SwitchProfile.as_str(), "Switching profile...");
        assert_eq!(LoadingKind::AddSub.as_str(), "Adding subscription...");
    }

    #[test]
    fn test_sub_ui_defaults() {
        let ui = UiState::default();
        assert_eq!(ui.selected_sub_idx, 0);
        assert!(!ui.sub_input_mode);
        assert!(ui.sub_input.is_empty());
        assert!(ui.confirm_remove.is_none());
    }
```

`keybindings.rs` tests 新增：

```rust
    #[test]
    fn test_sub_keybindings() {
        assert_eq!(parse_key(k('6')), Some(Action::SwitchView(5)));
        assert_eq!(parse_key(k('u')), Some(Action::SubUpdate));
        assert_eq!(parse_key(k('a')), Some(Action::SubAdd));
    }
```

（tests 模块需 `use Action::*;` 已有或补齐引用方式与现有一致）

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --lib app::state ui::keybindings`
Expected: FAIL（变体/字段/Action 不存在）

- [ ] **Step 3: 实现**

`state.rs`：

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveView {
    Dashboard,
    Proxies,
    Connections,
    Rules,
    Logs,
    Subscriptions,
}
```

`LoadingKind` 加两个变体 + `as_str` 分支；`UiState` 加四个字段 + Default 初始化（`selected_sub_idx: 0` 等）。

`keybindings.rs`：`Action` 枚举加 `SubUpdate, SubAdd`；`parse_key` 加：

```rust
        KeyEvent {
            code: KeyCode::Char('6'),
            ..
        } => Some(Action::SwitchView(5)),
        KeyEvent {
            code: KeyCode::Char('u'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::SubUpdate),
        KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            ..
        } => Some(Action::SubAdd),
```

`settings.rs`：删除 `Update interval: {} min\n\n\` 行与对应参数 `state.config.subscriptions.update_interval_minutes,`（`Subscriptions` 小节保留 Count 行）。

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: PASS（`Action::SubUpdate` 未被 handle_action 处理会出 non-exhaustive match 编译错——此时在 `app.rs` handle_action 的 match 末尾临时加 `Action::SubUpdate | Action::SubAdd => {}` 占位，Task 8 替换为真实逻辑）

- [ ] **Step 5: Commit**

```bash
git add src/app/state.rs src/ui/keybindings.rs src/ui/views/settings.rs src/ui/app.rs
git commit -m "feat: TUI state — Subscriptions view, sub input/confirm UI state, new keybindings"
```

---

### Task 7: TUI 订阅视图渲染

**Files:**
- Create: `src/ui/views/subscriptions.rs`
- Modify: `src/ui/views/mod.rs`
- Modify: `src/ui/views/sidebar.rs`
- Modify: `src/ui/app.rs`（render_frame）

**Interfaces:**
- Consumes: Task 6 的 `ActiveView::Subscriptions`、`UiState` 新字段
- Produces: `subscriptions::render(f: &mut Frame, area: Rect, state: &AppState)`（含 URL 输入行与删除确认弹窗）

- [ ] **Step 1: 创建视图**

`src/ui/views/subscriptions.rs`：

```rust
use crate::app::state::{ActiveView, AppState};
use crate::ui::theme::CATPPUCCIN_MOCHA as T;
use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let items = &state.config.subscriptions.items;
    let active = state.config.subscriptions.active.as_deref();

    let mut lines: Vec<Line> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let is_active = active == Some(item.name.as_str());
        let is_selected = i == state.ui.selected_sub_idx;
        let mark = if is_active { "* " } else { "  " };
        let label = format!(
            "{}{}  {} nodes  {}",
            mark,
            item.name,
            item.node_count.unwrap_or(0),
            item.last_updated.as_deref().unwrap_or("(never)")
        );
        let style = if is_selected {
            Style::default().fg(T.primary).bg(T.surface)
        } else if is_active {
            Style::default().fg(T.green)
        } else {
            Style::default().fg(T.text)
        };
        lines.push(Line::from(Span::styled(label, style)));
    }

    if items.is_empty() {
        lines.push(Line::from(Span::styled(
            "No subscriptions — press 'a' to add, or run: mioctl sub add <url>",
            Style::default().fg(T.text_secondary),
        )));
    }

    let input_line = if state.ui.sub_input_mode {
        Line::from(Span::styled(
            format!("URL: {}_", state.ui.sub_input),
            Style::default().fg(T.yellow),
        ))
    } else {
        Line::from(Span::styled(
            "Enter 激活 · u 更新 · a 添加 · d 删除",
            Style::default().fg(T.text_secondary),
        ))
    };
    lines.push(Line::from(""));
    lines.push(input_line);

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(p, area);

    if let Some(ref name) = state.ui.confirm_remove {
        let popup = centered_rect(46, 18, f.area());
        let block = Block::default()
            .title(" Remove subscription ")
            .borders(Borders::ALL)
            .style(Style::default().bg(T.surface));
        let inner = block.inner(popup);
        f.render_widget(Clear, popup);
        f.render_widget(block, popup);
        let text = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("Remove '{}'?", name),
                Style::default().fg(T.text),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "y confirm · Esc cancel",
                Style::default().fg(T.text_secondary),
            )),
        ])
        .wrap(Wrap { trim: true });
        f.render_widget(text, inner);
    }
}

fn centered_rect(px: u16, py: u16, area: Rect) -> Rect {
    let w = area.width * px / 100;
    let h = area.height * py / 100;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}
```

`views/mod.rs` 加 `pub mod subscriptions;`

**sidebar.rs** items 修改：

```rust
        item("Logs       ", ActiveView::Logs, &state.ui.active_view),
        ListItem::new(""),
        item("Subs       ", ActiveView::Subscriptions, &state.ui.active_view),
        item("Settings   ", ActiveView::Dashboard, &state.ui.active_view),
```

（删除 `Update Subs` 行）

**keybindings.rs** `parse_mouse` 行映射更新（row 6 现在是 Subs，row 7 是 Settings）：

```rust
                match row {
                    0..=4 => {
                        return Some(Action::SwitchView(row));
                    }
                    6 => return Some(Action::SwitchView(5)),
                    7 => return Some(Action::ShowSettings),
                    _ => {}
                }
```

**app.rs** `render_frame` 的 match 加分支 + import：

```rust
        Subscriptions => subscriptions::render(f, content[0], state),
```

`use crate::ui::views::{..., subscriptions, ...}` 补充。

- [ ] **Step 2: 构建验证**

Run: `cargo build && cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: PASS（`unused variable: state` 之类警告不允许出现——`render` 签名已用 state）

- [ ] **Step 3: Commit**

```bash
git add src/ui/views/subscriptions.rs src/ui/views/mod.rs src/ui/views/sidebar.rs src/ui/keybindings.rs src/ui/app.rs
git commit -m "feat: TUI subscriptions view — list, URL input line, remove-confirm popup"
```

---

### Task 8: TUI action 处理与输入模式

**Files:**
- Modify: `src/ui/app.rs`

**Interfaces:**
- Consumes: Task 5 的 `SubscriptionManager::{add, use_profile, update, remove}`、`UpdateTarget`；Task 6/7 的 UI 状态
- Produces: 完整 TUI 订阅管理交互（Enter 激活 / u 更新 / a 添加 / d 删除确认 / y 确认）

- [ ] **Step 1: 主循环键拦截**

在 `run_tui` 主循环 `if s.ui.search_mode {...}` 块之后、`else if s.ui.show_mode_selector` 之前插入：

```rust
                    } else if s.ui.sub_input_mode {
                        match key.code {
                            KeyCode::Esc => {
                                s.ui.sub_input_mode = false;
                                s.ui.sub_input.clear();
                            }
                            KeyCode::Enter => {
                                let url = std::mem::take(&mut s.ui.sub_input);
                                s.ui.sub_input_mode = false;
                                if url.is_empty() {
                                    continue;
                                }
                                s.ui.loading = Some(LoadingKind::AddSub);
                                let mut cfg = s.config.clone();
                                let shared2 = state.clone();
                                tokio::spawn(async move {
                                    let result =
                                        SubscriptionManager::add(&mut cfg, &url, None, false, false)
                                            .await;
                                    let mut st = shared2.lock().await;
                                    st.config = cfg;
                                    match result {
                                        Ok(msg) => st.add_log("info", &msg),
                                        Err(e) => st.add_log("error", &format!("Add failed: {}", e)),
                                    }
                                    st.ui.loading = None;
                                });
                            }
                            KeyCode::Backspace => {
                                s.ui.sub_input.pop();
                            }
                            KeyCode::Char(c) => {
                                s.ui.sub_input.push(c);
                            }
                            _ => {}
                        }
                    } else if let Some(ref name) = s.ui.confirm_remove.clone() {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                s.ui.confirm_remove = None;
                                s.ui.loading = Some(LoadingKind::SwitchProfile);
                                let mut cfg = s.config.clone();
                                let shared2 = state.clone();
                                tokio::spawn(async move {
                                    let result = SubscriptionManager::remove(&mut cfg, &name).await;
                                    let mut st = shared2.lock().await;
                                    st.config = cfg;
                                    match result {
                                        Ok(msg) => st.add_log("info", &msg),
                                        Err(e) => st.add_log("error", &format!("Remove failed: {}", e)),
                                    }
                                    refresh_state(&shared2).await;
                                    st.ui.loading = None;
                                });
                            }
                            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('N') => {
                                s.ui.confirm_remove = None;
                            }
                            _ => {}
                        }
                    } else if s.ui.show_mode_selector {
```

（原 `else if s.ui.show_mode_selector` 的 `if` 改为接续链）

- [ ] **Step 2: handle_action 分派**

`Action::SwitchNode` 分支开头插入视图分派（原 proxies 逻辑之前）：

```rust
        Action::SwitchNode => {
            if s.ui.active_view == Subscriptions {
                let name = s
                    .config
                    .subscriptions
                    .items
                    .get(s.ui.selected_sub_idx)
                    .map(|i| i.name.clone());
                if let Some(name) = name {
                    s.ui.loading = Some(LoadingKind::SwitchProfile);
                    let mut cfg = s.config.clone();
                    let shared2 = shared.clone();
                    tokio::spawn(async move {
                        let result =
                            SubscriptionManager::use_profile(&mut cfg, &name, false).await;
                        let mut st = shared2.lock().await;
                        st.config = cfg;
                        match result {
                            Ok(msg) => st.add_log("info", &msg),
                            Err(e) => st.add_log("error", &format!("Switch failed: {}", e)),
                        }
                        refresh_state(&shared2).await;
                        st.ui.loading = None;
                    });
                }
                return true;
            }
            ...（原逻辑不动）
```

`Action::CloseConnection` 分支开头同理：

```rust
        Action::CloseConnection => {
            if s.ui.active_view == Subscriptions {
                if let Some(item) =
                    s.config.subscriptions.items.get(s.ui.selected_sub_idx)
                {
                    s.ui.confirm_remove = Some(item.name.clone());
                }
                return true;
            }
            ...（原逻辑不动）
```

替换 Task 6 的占位 `Action::SubUpdate | Action::SubAdd => {}`：

```rust
        Action::SubUpdate => {
            if s.ui.active_view == Subscriptions {
                let name = s
                    .config
                    .subscriptions
                    .items
                    .get(s.ui.selected_sub_idx)
                    .map(|i| i.name.clone());
                if let Some(name) = name {
                    s.ui.loading = Some(LoadingKind::UpdateSubs);
                    let mut cfg = s.config.clone();
                    let shared2 = shared.clone();
                    tokio::spawn(async move {
                        let result =
                            SubscriptionManager::update(&mut cfg, &UpdateTarget::Named(name))
                                .await;
                        let mut st = shared2.lock().await;
                        st.config = cfg;
                        match result {
                            Ok(msg) => st.add_log("info", &msg),
                            Err(e) => st.add_log("error", &format!("Update failed: {}", e)),
                        }
                        refresh_state(&shared2).await;
                        st.ui.loading = None;
                    });
                }
            }
        }
        Action::SubAdd => {
            if s.ui.active_view == Subscriptions && !s.ui.sub_input_mode {
                s.ui.sub_input_mode = true;
                s.ui.sub_input.clear();
            }
        }
```

`Action::UpdateSubs`（sidebar 旧入口已删，但 Action 保留供 SubUpdate 复用 LoadingKind）分支更新为调用新接口：

```rust
        Action::UpdateSubs => {
            s.ui.loading = Some(LoadingKind::UpdateSubs);
            let mut cfg = s.config.clone();
            let shared2 = shared.clone();
            tokio::spawn(async move {
                let result = SubscriptionManager::update(&mut cfg, &UpdateTarget::All).await;
                let mut state = shared2.lock().await;
                state.config = cfg;
                match result {
                    Ok(_) => state.add_log("info", "Subscriptions updated"),
                    Err(e) => state.add_log("error", &format!("Subscription update failed: {}", e)),
                }
                state.ui.loading = None;
            });
        }
```

app.rs 顶部 import 加 `use crate::subscription::manager::UpdateTarget;`。`Action::MoveUp`/`MoveDown` 分支加订阅视图支持：

```rust
        Action::MoveDown => {
            if s.ui.active_view == Subscriptions {
                s.ui.selected_sub_idx = (s.ui.selected_sub_idx + 1)
                    .min(s.config.subscriptions.items.len().saturating_sub(1));
            } else {
                s.ui.selected_node_idx += 1;
            }
        }
        Action::MoveUp => {
            if s.ui.active_view == Subscriptions {
                s.ui.selected_sub_idx = s.ui.selected_sub_idx.saturating_sub(1);
            } else {
                s.ui.selected_node_idx = s.ui.selected_node_idx.saturating_sub(1);
            }
        }
```

（先阅读现有 `MoveUp`/`MoveDown` 实现再改，保持其他视图行为不变；若现有实现有额外 clamp 逻辑需保留）

- [ ] **Step 3: TUI 启动迁移**

`run_tui` 中 init spawn 完成后（`s.add_log("info", "Connected");` 附近）追加：

```rust
                let mut cfg = s.config.clone();
                let warnings = SubscriptionManager::ensure_archived(&mut cfg).await;
                s.config = cfg;
                for w in warnings {
                    s.add_log("info", &w);
                }
```

- [ ] **Step 4: 构建 + 全量测试**

Run: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/ui/app.rs
git commit -m "feat: TUI subscription actions — activate/update/add/remove with async spawn"
```

---

### Task 9: 集成测试

**Files:**
- Create: `tests/profiles_test.rs`

**Interfaces:**
- Consumes: `mioctl::subscription::manager::{SubscriptionManager, UpdateTarget}`、`mioctl::config::mioctl_config::MioctlConfig`、`MIOCTL_HOME`

- [ ] **Step 1: 写测试**

```rust
use std::sync::{Mutex, OnceLock};

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

const SUB_YAML_A: &str = "proxies:\n  - name: A1\n    type: ss\n    server: 1.1.1.1\n    port: 8388\n    cipher: aes-256-gcm\n    password: pa\nproxy-groups:\n  - name: GA\n    type: select\n    proxies: [A1]\nrules:\n  - MATCH,GA\n";
const SUB_YAML_B: &str = "proxies:\n  - name: B1\n    type: tuic\n    server: 2.2.2.2\n    port: 4430\n    uuid: u1\n    password: pb\nproxy-groups:\n  - name: GB\n    type: select\n    proxies: [B1]\nrules:\n  - MATCH,GB\n";

fn b64(content: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(content)
}

async fn setup(
    mock: &wiremock::MockServer,
    path: &str,
    status: u16,
    body: String,
) {
    use wiremock::matchers::{method, path_eq};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("GET"))
        .and(path_eq(path))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(mock)
        .await;
}

#[tokio::test]
async fn full_lifecycle_add_use_update_remove() {
    let _guard = lock().lock().unwrap();
    let mock = wiremock::MockServer::start().await;
    setup(&mock, "/sub/a", 200, SUB_YAML_A.to_string()).await;
    setup(&mock, "/sub/b", 200, SUB_YAML_B.to_string()).await;

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MIOCTL_HOME", dir.path());
    let mihomo_path = dir.path().join("mihomo.yaml");
    std::fs::write(&mihomo_path, "mixed-port: 7897\nmode: rule\n").unwrap();

    let mut config = mioctl::config::mioctl_config::MioctlConfig::default();
    config.mihomo.config_path = mihomo_path.to_string_lossy().into_owned();
    config.mihomo.external_controller = "127.0.0.1:1".into();

    let mgr = mioctl::subscription::manager::SubscriptionManager;

    let s = mgr
        .add(&mut config, &mock.uri().to_string(), Some("subA".into()), true, false)
        .await
        .unwrap();
    assert!(s.contains("activated"));
    assert_eq!(config.subscriptions.active.as_deref(), Some("subA"));
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("name: A1"));

    mgr.add(&mut config, &mock.uri().to_string(), Some("subB".into()), true, false)
        .await
        .unwrap();
    assert_eq!(config.subscriptions.active.as_deref(), Some("subA"));
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("name: A1"));
    assert!(!written.contains("name: B1"));

    mgr.use_profile(&mut config, "subB", true).await.unwrap();
    assert_eq!(config.subscriptions.active.as_deref(), Some("subB"));
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("name: B1"));
    assert!(!written.contains("name: A1"));
    assert!(written.contains("mixed-port: 7897"));

    use mioctl::subscription::manager::UpdateTarget;
    mgr.update(&mut config, &UpdateTarget::Named("subB".into()))
        .await
        .unwrap();

    mgr.remove(&mut config, "subB").await.unwrap();
    assert_eq!(config.subscriptions.active, None);
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("proxies: []"));
    assert!(written.contains("MATCH,DIRECT"));
    assert!(config.find_subscription("subA").is_some());

    mgr.use_profile(&mut config, "subA", true).await.unwrap();
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("name: A1"));

    std::env::remove_var("MIOCTL_HOME");
}

#[tokio::test]
async fn add_base64_subscription_generates_archive() {
    let _guard = lock().lock().unwrap();
    let uri_list = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\nss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.5:8388#N2\nss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.6:8388#N3\n";
    let mock = wiremock::MockServer::start().await;
    setup(&mock, "/sub/b64", 200, b64(uri_list)).await;

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MIOCTL_HOME", dir.path());
    let mihomo_path = dir.path().join("mihomo.yaml");
    std::fs::write(&mihomo_path, "mixed-port: 7897\n").unwrap();

    let mut config = mioctl::config::mioctl_config::MioctlConfig::default();
    config.mihomo.config_path = mihomo_path.to_string_lossy().into_owned();
    config.mihomo.external_controller = "127.0.0.1:1".into();

    let mgr = mioctl::subscription::manager::SubscriptionManager;
    mgr.add(&mut config, &mock.uri().to_string(), Some("b64sub".into()), true, true)
        .await
        .unwrap();

    assert_eq!(
        config.find_subscription("b64sub").unwrap().node_count,
        Some(3)
    );
    let archive = mioctl::subscription::profile::read_archive("b64sub").unwrap();
    assert!(archive.contains("name: N1"));
    assert!(archive.contains("name: b64sub"));
    let written = std::fs::read_to_string(&mihomo_path).unwrap();
    assert!(written.contains("name: N1"));

    std::env::remove_var("MIOCTL_HOME");
}

#[tokio::test]
async fn add_duplicate_explicit_name_fails() {
    let _guard = lock().lock().unwrap();
    let mock = wiremock::MockServer::start().await;
    setup(&mock, "/sub/a", 200, SUB_YAML_A.to_string()).await;

    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MIOCTL_HOME", dir.path());
    let mihomo_path = dir.path().join("mihomo.yaml");
    std::fs::write(&mihomo_path, "mixed-port: 7897\n").unwrap();

    let mut config = mioctl::config::mioctl_config::MioctlConfig::default();
    config.mihomo.config_path = mihomo_path.to_string_lossy().into_owned();
    config.mihomo.external_controller = "127.0.0.1:1".into();

    let mgr = mioctl::subscription::manager::SubscriptionManager;
    mgr.add(&mut config, &mock.uri().to_string(), Some("dup".into()), true, false)
        .await
        .unwrap();
    let err = mgr
        .add(&mut config, &mock.uri().to_string(), Some("dup".into()), true, false)
        .await
        .unwrap_err();
    assert!(err.contains("already exists"));

    std::env::remove_var("MIOCTL_HOME");
}
```

注意：`Cargo.toml` dev-dependencies 需确认已有 `wiremock`、`tempfile`、`base64`（base64 是主依赖）。检查后若缺则补。

- [ ] **Step 2: 运行集成测试**

Run: `cargo test --test profiles_test`
Expected: 3 个测试 PASS（fetch 走 wiremock；mihomo API 不可达 → get_version 返回 None → UA 降级 ClashMeta；add 用 `no_reload=true` 避免真实 reload）

- [ ] **Step 3: 全量验证**

Run: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全 PASS

- [ ] **Step 4: Commit**

```bash
git add tests/profiles_test.rs Cargo.toml Cargo.lock
git commit -m "test: profile lifecycle integration tests (add/use/update/remove, base64 archive, dup name)"
```

---

### Task 10: CLAUDE.md 更新 + 真实环境验证

**Files:**
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: 全部前序任务
- Produces: 更新的架构文档

- [ ] **Step 1: 更新 CLAUDE.md**

`## Architecture` 的 `subscription/` 行替换为：

```markdown
- **`subscription/`** — Subscription profiles (single-active, clash-verge style): fetch, format
  auto-detection (YAML/Base64/URI), normalize-to-YAML archive in `~/.config/mioctl/profiles/`,
  activation merges proxies/proxy-groups/rules verbatim into mihomo config (those three sections
  are fully managed by mioctl — manual edits are overwritten), backup/rollback, reload
```

`## Config` 小节替换 provider 行为 profile 描述：

```markdown
- User config: `~/.config/mioctl/config.toml`
- Subscription archives: `~/.config/mioctl/profiles/*.yaml` (one per subscription, `[subscriptions].active` marks the current one)
- System proxy: `~/.config/environment.d/proxy.conf`
- mihomo must have `external-controller` enabled; `secret` is optional
```

`## Commands` 无需变（命令名未变）。Mihomo API Notes 中如有 provider 相关描述则删除。

- [ ] **Step 2: 真实环境手动验证**

Run（依次，真实 mihomo 环境）:
```bash
cargo build
./target/debug/mioctl sub list                 # 存量订阅迁移为存档，列出
./target/debug/mioctl sub update               # 更新激活项
./target/debug/mioctl sub use 狗狗加速.com      # 重新激活（应成功且节点完整）
cargo run -- tui                               # 数字键 6 进订阅视图，Enter 激活、d 删除流程走一遍（最后取消）
```
Expected: 每步输出正常；`~/.config/mihomo/config.yaml` 三段来自订阅、基础设施段保留；tuic/anytls/mieru 节点仍在 config 中

- [ ] **Step 3: 全量回归**

Run: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`
Expected: 全 PASS

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: CLAUDE.md — profile archive architecture and managed config sections"
```

---

## Self-Review 结论

- **Spec 覆盖**：A1 sanitize（Task 2）、A2 重名（Task 3 add + Task 9 测试）、A3 统一 YAML + parser 增强（Task 2）、B1 三段全托（Task 3 activate 用 merger）、B2 空三段（Task 3 remove + 测试）、C1 update 默认激活（Task 3/5）、C2 死配置删除（Task 1/6）、C3 三级回退（Task 3 reload_mihomo）、D1–D3 TUI（Task 6/7/8）、E1 清理（Task 4）、E2 别名（Task 5）、E3 迁移（Task 3 ensure_archived + Task 5/8 入口）、E4 node_count（Task 1/3）、存量 providers 目录清理（Task 3 ensure_archived）——全覆盖
- **类型一致性**：`use_profile(name: &str, no_reload: bool)`、`update(target: &UpdateTarget)`、`add(url: &str, name: Option<String>, no_reload, activate)` 在 Task 3/5/8/9 间一致
- **占位符扫描**：无 TBD/TODO/占位代码；所有代码步骤含完整代码
