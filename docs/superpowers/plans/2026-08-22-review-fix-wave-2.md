# 评审修复第二波 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 2026-08-22 全分支独立评审确认的 12 项问题（1 High / 4 Medium / 7 Low），全部经代码级实证。

**Architecture:** 全部为局部修复，不动激活/回滚核心状态机：CLI 退出码语义、TUI 并发守卫、错误信息保真、config 损坏保护、merger 键保留语义、名字健壮性、备份文件生命周期、输入模态中断。

**Tech Stack:** Rust（tokio / clap / serde_yaml / reqwest / wiremock），ratatui + crossterm。

## Global Constraints

- 禁止 git commit —— 全部改动保持未提交工作树（用户约定），每任务以测试门禁收尾
- 不添加任何代码注释
- TDD：每个行为修复先写失败测试再实现
- 门禁标准：`cargo test` 仅 `debug_yaml` 与 `test_real_subscription_parse` 允许失败（离线网络豁免）；`cargo clippy --tests -- -D warnings` 零警告；`cargo fmt --check` 干净
- 测试封闭性：涉及 env 的测试设置 `MIOCTL_HOME` + `MIOCTL_TEST_NO_SYSTEMCTL=1`，并持有 `crate::testutil::env_lock()`（注意 bin target 的 testutil 在 main.rs，lib target 在 lib.rs）

---

### Task 1: CLI 错误路径非零退出码（H1）

**Files:**
- Modify: `src/cli/sub.rs`（`run` 签名与全部分支）
- Modify: `src/cli/connect.rs`（`run` 签名与失败分支）
- Modify: `src/main.rs:28-36`（分发处理返回值）
- Test: `src/cli/sub.rs` tests（子进程 dispatch 测试改造）

**Interfaces:**
- Produces: `pub async fn run(action: SubAction) -> bool`（true=成功 exit 0，false=失败）；`pub async fn run(action: ConnectAction) -> bool`
- 决策：`cli/doctor.rs` 不改——doctor 是诊断报告工具，输出本身就是结果，退出码恒 0 是正确语义

- [ ] **Step 1: 改造测试先行**

`cli_sub_child_dispatch` 子进程内把返回值打到 stdout（外层据此断言，子进程自身仍 exit 0 走 test harness）：

```rust
#[test]
fn cli_sub_child_dispatch() {
    let mode = match std::env::var("MIOCTL_TEST_CHILD") {
        Ok(mode) => mode,
        Err(_) => return,
    };
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ok = runtime.block_on(run(action_for(&mode)));
    if !ok {
        println!("__MIOCTL_EXIT_1__");
    }
    std::process::exit(0);
}
```

错误场景测试追加断言（`update_without_flags_dispatches_to_active_target`、`update_with_name_dispatches_to_named_target`、`use_dispatches_missing_name_error`、`remove_with_yes_dispatches_missing_name_error`、`register_and_add_dispatch_to_same_offline_error`）：

```rust
assert!(
    stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
    "expected nonzero-exit signal, stdout: {}",
    stdout_of(&out)
);
```

成功场景测试（`list_dispatches_empty_output`、`update_all_with_empty_config_succeeds_silently`、`update_all_flag_takes_precedence_over_name`、`remove_without_yes_cancels_in_non_interactive_dispatch`）追加：

```rust
assert!(
    !stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
    "unexpected failure signal, stdout: {}",
    stdout_of(&out)
);
```

注意 `register_and_add_dispatch_to_same_offline_error` 的 add_err/register_err 相等断言保留。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline cli::sub`
Expected: 新增断言 FAIL（run 返回 ()，`!ok` 处编译错误即失败信号）

- [ ] **Step 3: 实现**

`src/cli/sub.rs`：

```rust
pub async fn run(action: SubAction) -> bool {
    let mut config = MioctlConfig::load();
    for warning in SubscriptionManager::ensure_archived(&mut config).await {
        eprintln!("{}", warning);
    }
    match action {
        SubAction::List => {
            println!("{}", SubscriptionManager::list(&config));
            true
        }
        SubAction::Add { url, name, no_reload, activate } => {
            match SubscriptionManager::add(&mut config, &url, name, no_reload, activate).await {
                Ok(summary) => {
                    println!("{}", summary);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
        SubAction::Register { url, name, no_reload } => {
            match SubscriptionManager::add(&mut config, &url, name, no_reload, false).await {
                Ok(summary) => {
                    println!("{}", summary);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
        SubAction::Use { name, no_reload } => {
            match SubscriptionManager::use_profile(&mut config, &name, no_reload).await {
                Ok(message) => {
                    println!("{}", message);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
        SubAction::Update { name, all } => {
            let target = if all {
                UpdateTarget::All
            } else if let Some(name) = name {
                UpdateTarget::Named(name)
            } else {
                UpdateTarget::Active
            };
            match SubscriptionManager::update(&mut config, &target).await {
                Ok(result) => {
                    println!("{}", result);
                    !result.lines().any(|l| l.contains("ERROR"))
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
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
                return true;
            }
            match SubscriptionManager::remove(&mut config, &name).await {
                Ok(message) => {
                    println!("{}", message);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
    }
}
```

语义说明（写进任务执行者的理解，不写进代码）：`Cancelled.` 是用户主动取消，exit 0 正确；update 的 Ok 结果中含 `ERROR` 行（单项拉取/归一化失败，格式见 manager.rs `": ERROR - "`）视为整体失败 exit 1——cron/systemd timer 场景必须能感知；`ensure_archived` 的迁移 warning 不代表命令失败。

`src/cli/connect.rs`：`pub async fn run(action: ConnectAction) -> bool`，成功 `println!` 分支 `true`，三个失败分支（认证失败/API 错误/连接失败）`false`。

`src/main.rs`：

```rust
Some(Commands::Sub { action }) => {
    if !cli::sub::run(action).await {
        std::process::exit(1);
    }
}
Some(Commands::Connect { action }) => {
    if !cli::connect::run(action).await {
        std::process::exit(1);
    }
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline cli:: && cargo clippy --tests -- -D warnings`
Expected: 全 PASS（含新断言）

---

### Task 2: TUI 订阅操作 loading 守卫（M1）

**Files:**
- Modify: `src/ui/app.rs:281-286`（confirm Enter→remove）、`:801-815`（SwitchNode→switch）、`:960-973`（SubUpdate→update）
- Test: `src/ui/app.rs` tests

**Interfaces:**
- Consumes: 既有 `LoadingKind`、`spawn_*` helpers（不改动）
- Produces: 无新接口，仅行为守卫

- [ ] **Step 1: 失败测试**（沿用文件内既有 spawn 测试的 state/shared 构造模式，参照 `test_add_subscription_spawn_activate_failure_keeps_shared_config`）：

```rust
#[tokio::test]
async fn subscription_switch_ignored_while_loading() {
    let mut state = crate::app::state::AppState::default();
    state.ui.active_view = crate::app::state::ActiveView::Subscriptions;
    state.ui.loading = Some(crate::app::state::LoadingKind::Refresh);
    state.config.add_subscription("sub1".into(), "https://x".into());
    let shared: std::sync::Arc<tokio::sync::Mutex<crate::app::state::AppState>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(state));
    {
        let mut s = shared.lock().await;
        handle_action(&Action::SwitchNode, &mut s, shared.clone()).await;
    }
    let s = shared.lock().await;
    assert_eq!(
        s.ui.loading,
        Some(crate::app::state::LoadingKind::Refresh),
        "loading must not be replaced while an operation is in flight"
    );
}
```

同构再写两个：`Action::SubUpdate`（预设 items + loading=Some(Refresh)，断言不变）与 confirm 路径（预设 `confirm_remove = Some("sub1")` + loading，发送 `KeyCode::Char('y')`——confirm 路径在事件循环而非 handle_action，改为直接对状态调用模式不可行时，允许只覆盖 handle_action 两个入口 + confirm 分支以代码走查收尾，在任务报告中注明）。

注意：AppState 构造以现有测试实际使用的构造函数为准（可能是 `AppState::new(...)` 或 Default），先读 app.rs 既有测试再对齐，测试骨架中名字对不上时以既有模式为准——断言核心不变：**loading 占用时触发键不得改变 loading 值**。

- [ ] **Step 2: 跑测试确认失败**

Run: `cargo test --offline subscription_switch_ignored`
Expected: FAIL（loading 被覆盖为 SwitchProfile）

- [ ] **Step 3: 实现**（三处同款守卫）

confirm 处：

```rust
} else if let Some(name) = s.ui.confirm_remove.clone() {
    if handle_confirm_key(&mut s.ui, key.code) && s.ui.loading.is_none() {
        s.ui.loading = Some(LoadingKind::SwitchProfile);
        let cfg = s.config.clone();
        spawn_remove_subscription(state.clone(), cfg, name);
    }
}
```

SwitchNode 订阅分支：

```rust
if let Some(name) = name {
    if s.ui.loading.is_none() {
        s.ui.loading = Some(LoadingKind::SwitchProfile);
        let cfg = s.config.clone();
        spawn_switch_profile(shared.clone(), cfg, name);
    }
}
```

SubUpdate 分支：

```rust
Action::SubUpdate => {
    if s.ui.active_view == Subscriptions && s.ui.loading.is_none() {
        let name = s
            .config
            .subscriptions
            .items
            .get(s.ui.selected_sub_idx)
            .map(|i| i.name.clone());
        if let Some(name) = name {
            s.ui.loading = Some(LoadingKind::UpdateSubs);
            let cfg = s.config.clone();
            spawn_update_subscription(shared.clone(), cfg, name);
        }
    }
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline ui::app && cargo clippy --tests -- -D warnings`
Expected: 全 PASS

---

### Task 3: fetch 失败保留底层原因（M2）

**Files:**
- Modify: `src/subscription/fetcher.rs:9-33`
- Test: `src/subscription/fetcher.rs` tests

**Interfaces:**
- Produces: `fetch_with_ua_probe` 签名不变；错误消息在原有文案后追加 `(last error: {})`（仅当存在网络/HTTP 层错误时）

- [ ] **Step 1: 失败测试**

```rust
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
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_fetch_with_ua_probe_reports`
Expected: FAIL（消息不含 404）

- [ ] **Step 3: 实现**

```rust
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
                if count_proxy_entries(&body) >= 3 {
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
```

（thin-body 无网络错误的情形保持原消息——那种情况才真的是"客户端身份"问题。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline fetcher && cargo test --offline cli::sub`
Expected: 全 PASS（cli 子测试的 "all User-Agent probes failed" contains 断言不被破坏，127.0.0.1:1 的连接错误现在会附进 last error）

---

### Task 4: config.toml 损坏时备份后重置（M3）

**Files:**
- Modify: `src/config/mioctl_config.rs:125-137`（`load`）
- Test: `src/config/mioctl_config.rs` tests

**Interfaces:**
- Produces: `load()` 签名不变；损坏 TOML 被 rename 为 `config.toml.corrupt` 后返回默认值（后续 save 不会覆盖丢数据）

- [ ] **Step 1: 失败测试**

```rust
#[test]
fn test_load_corrupt_config_preserves_original_as_corrupt() {
    let _guard = crate::testutil::env_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
    let broken = "not [ valid toml {{{";
    std::fs::write(dir.path().join("config.toml"), broken).unwrap();

    let config = MioctlConfig::load();

    assert!(config.subscriptions.items.is_empty());
    assert!(!dir.path().join("config.toml").exists());
    let preserved = dir.path().join("config.toml.corrupt");
    assert_eq!(std::fs::read_to_string(preserved).unwrap(), broken);
    unsafe { std::env::remove_var("MIOCTL_HOME") };
}
```

（若 `broken` 意外能被 toml 解析，换 `"[section]\nkey = " ` 等确定非法的串——先跑一次验证。）

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_load_corrupt`
Expected: FAIL（.corrupt 不存在）

- [ ] **Step 3: 实现**

```rust
pub fn load() -> Self {
    let path = Self::config_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(_) => {
                    let corrupt = Self::config_dir().join("config.toml.corrupt");
                    let _ = std::fs::rename(&path, &corrupt);
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    } else {
        let config = Self::default();
        let _ = config.save();
        config
    }
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline config:: && cargo test --offline`
Expected: 全 PASS（现有 load 测试不受影响：合法配置路径不变）

---

### Task 5: merger 保留白名单外的既有顶层键（M4）

**Files:**
- Modify: `src/subscription/merger.rs:97-108`（排序段）
- Test: `src/subscription/merger.rs` tests

**Interfaces:**
- Produces: `PRESERVE_KEYS` 语义从"存活白名单"变为"排序提示"：输出顺序 = 白名单序 → 其余既有键原序 → proxies/proxy-groups/rules；`proxy-providers` 仍被删除；三段仍被替换

- [ ] **Step 1: 失败测试**

```rust
#[test]
fn test_merge_preserves_unknown_top_level_keys() {
    let existing = r#"mixed-port: 7897
secret: "abc"
external-ui: ./ui
rule-providers:
  rp:
    type: http
    url: https://example.com/rp.yaml
my-custom-key: 42
proxy-providers:
  pp:
    type: http
    url: https://example.com/pp.yaml
"#;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, existing).unwrap();

    let full: Value = serde_yaml::from_str(
        "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 443\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G",
    )
    .unwrap();

    let result = merge_mihomo_config(
        path.to_str().unwrap(),
        full.get("proxies").unwrap(),
        full.get("proxy-groups").unwrap(),
        full.get("rules").unwrap(),
    )
    .unwrap();

    let out: Value = serde_yaml::from_str(&result.yaml).unwrap();
    assert_eq!(out.get("secret").and_then(|v| v.as_str()), Some("abc"));
    assert_eq!(out.get("my-custom-key").and_then(|v| v.as_i64()), Some(42));
    assert!(out.get("rule-providers").is_some());
    assert!(out.get("external-ui").is_some());
    assert!(out.get("proxy-providers").is_none());
    assert_eq!(
        out.get("proxies").unwrap().as_sequence().unwrap().len(),
        1
    );
}
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_merge_preserves_unknown`
Expected: FAIL（secret/my-custom-key 丢失）

- [ ] **Step 3: 实现**（替换 merger.rs 现有排序段）

```rust
let mut ordered = Mapping::new();
for &key in PRESERVE_KEYS {
    if let Some(v) = config.remove(key) {
        ordered.insert(Value::String(key.into()), v);
    }
}
let rest: Vec<(Value, Value)> = config.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
for (k, v) in rest {
    config.remove(&k);
    ordered.insert(k, v);
}
for key in &["proxies", "proxy-groups", "rules"] {
    if let Some(v) = config.remove(*key) {
        ordered.insert(Value::String(key.to_string()), v);
    }
}
```

（`config.remove` 的参数形式以现有代码编译通过的形式为准：白名单循环沿用现状，rest 循环传 `&k`；如 serde_yaml 版本 remove 签名不同，以编译器指引微调，语义不变。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline merger && cargo test --offline`
Expected: 全 PASS（现有 merger/manager/profiles 测试兼容——它们只断言 contains 与三段内容）

---

### Task 6: 名字健壮性——sanitize 碰撞 + 截断 + URL 凭据剥离（L1+L5+L6）

**Files:**
- Modify: `src/subscription/profile.rs:13-21`（sanitize 截断 + 新增 `name_conflicts`）
- Modify: `src/subscription/manager.rs:29-41`（unique_name 基于 sanitize 比较）、`:162-176`（显式名检查）
- Modify: `src/subscription/parser.rs:364-378`（name_from_url）
- Test: 上述三文件 tests

**Interfaces:**
- Produces: `pub fn name_conflicts(new: &str, existing: &[String]) -> bool`（profile.rs，基于 `sanitize_filename` 相等）；`sanitize_filename` 截断至 80 chars；`unique_name` 与 `add` 显式名检查改用 `name_conflicts`

- [ ] **Step 1: 失败测试**

profile.rs：

```rust
#[test]
fn test_sanitize_filename_truncates_long_names() {
    let long = "x".repeat(300);
    assert!(sanitize_filename(&long).chars().count() <= 80);
}

#[test]
fn test_name_conflicts_across_sanitization() {
    let existing = vec!["a/b".to_string()];
    assert!(name_conflicts("a_b", &existing));
    assert!(name_conflicts("a/b", &existing));
    assert!(!name_conflicts("other", &existing));
}
```

parser.rs：

```rust
#[test]
fn test_name_from_url_strips_userinfo() {
    assert_eq!(
        name_from_url("https://user:secret@example.com/sub").unwrap(),
        "example"
    );
}

#[test]
fn test_name_from_url_case_insensitive_scheme() {
    let name = name_from_url("HTTPS://Example.COM/sub").unwrap();
    assert!(!name.contains('/'), "got: {}", name);
    assert!(!name.contains(':'), "got: {}", name);
}
```

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_name_conflicts test_name_from_url_strips test_sanitize_filename_truncates`
Expected: FAIL（name_conflicts 不存在；userinfo 未剥离；不截断）

- [ ] **Step 3: 实现**

profile.rs：

```rust
pub fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| if c == '/' || c.is_control() { '_' } else { c })
        .collect();
    sanitized.chars().take(80).collect()
}

pub fn name_conflicts(new: &str, existing: &[String]) -> bool {
    let sanitized = sanitize_filename(new);
    existing.iter().any(|e| sanitize_filename(e) == sanitized)
}
```

（80 chars 上限：最坏 3 字节/字符 = 240 字节 + ".yaml" < 255 字节文件名上限。）

manager.rs：

```rust
use crate::subscription::profile::name_conflicts;

fn unique_name(base: &str, existing: &[String]) -> String {
    if !name_conflicts(base, existing) {
        return base.to_string();
    }
    let mut i = 2;
    loop {
        let candidate = format!("{} ({})", base, i);
        if !name_conflicts(&candidate, existing) {
            return candidate;
        }
        i += 1;
    }
}
```

add 显式名分支：

```rust
Some(n) => {
    if name_conflicts(&n, &existing) {
        return Err(format!(
            "subscription '{}' already exists. Remove it first or use a different --name.",
            n
        ));
    }
    n
}
```

parser.rs：

```rust
pub fn name_from_url(url: &str) -> Result<String, String> {
    let lower = url.to_lowercase();
    let without_scheme = if lower.starts_with("https://") {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        url
    };
    let host = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty() {
        return Err("URL has empty host".into());
    }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() >= 2 {
        Ok(parts[parts.len() - 2].to_string())
    } else {
        Ok(host.to_string())
    }
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline && cargo clippy --tests -- -D warnings`
Expected: 全 PASS（注意既有 `test_unique_name_appends_suffix` 语义不变——原始名相等仍是冲突子集；`test_name_from_url_empty_host` 仍过：`https:///path` → 空 host → Err）

---

### Task 7: 备份生命周期 + 原子写（L2）

**Files:**
- Modify: `src/subscription/merger.rs:148-153`（write_config 原子化 + 新增 `discard_backup`）
- Modify: `src/subscription/manager.rs:103-115`（activate 成功后清理 .bak）、`:137-140`（write_empty_state 同）
- Test: `src/subscription/merger.rs` + `src/subscription/manager.rs` tests

**Interfaces:**
- Produces: `pub fn discard_backup(path: &str)`（merger.rs，静默删除 `<path>.bak`）；`write_config` 先写 `<path>.tmp` 再 rename
- 语义边界：清理发生在 write 成功后、reload 之前——与"reload 失败不回滚"的既有用户裁决一致；merge/write 失败路径的 rollback 及 .bak 保留不变（用户手救材料）

- [ ] **Step 1: 失败测试**

merger.rs：

```rust
#[test]
fn test_write_config_atomic_no_tmp_left() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    write_config(path.to_str().unwrap(), "content").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");
    assert!(!dir.path().join("config.yaml.tmp").exists());
}
```

manager.rs（追加到既有 `test_use_profile_writes_three_sections` 或新写）：

```rust
#[tokio::test]
async fn test_activate_success_discards_backup() {
    let (_env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
    config.add_subscription("sub1".into(), "https://x".into());
    crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
    SubscriptionManager::use_profile(&mut config, "sub1", true)
        .await
        .unwrap();
    assert!(!std::path::Path::new(&format!("{}.bak", config.mihomo.config_path)).exists());
}
```

同时给既有回滚测试（`test_activate_write_failure_rolls_back_file` 等）确认未破坏：失败后原内容仍在。

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_write_config_atomic test_activate_success_discards`
Expected: FAIL（.tmp 残留 / .bak 存在）

- [ ] **Step 3: 实现**

merger.rs：

```rust
pub fn discard_backup(path: &str) {
    let _ = std::fs::remove_file(format!("{}.bak", path));
}

pub fn write_config(path: &str, yaml: &str) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = format!("{}.tmp", path);
    std::fs::write(&tmp, yaml).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}
```

manager.rs `activate` 成功路径（write_config Ok 分支末尾、构造 reload_msg 之前）与 `write_empty_state` 成功末尾：

```rust
discard_backup(&config_path);
```

（import 行补 `discard_backup`。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline && cargo clippy --tests -- -D warnings`
Expected: 全 PASS

---

### Task 8: 模态内 Ctrl+C 中断 + save 失败可见（L4+L7）

**Files:**
- Modify: `src/ui/app.rs`（事件循环模态分支前统一拦截；新增 `is_quit_interrupt`）
- Modify: `src/subscription/manager.rs:309-311`（update save）、`:415`（ensure_archived save）
- Test: `src/ui/app.rs` + `tests/profiles_test.rs` tests

**Interfaces:**
- Produces: `fn is_quit_interrupt(key: &crossterm::event::KeyEvent) -> bool`（app.rs，私有限定 pub(crate) 视测试需要）；update 的 save 失败产生 `config: ERROR - ...` 行（自动被 Task 1 的 exit-code 检测捕获）；ensure_archived 的 save 失败产生 warning 行

- [ ] **Step 1: 失败测试**

app.rs：

```rust
#[test]
fn test_is_quit_interrupt() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    assert!(is_quit_interrupt(&KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    )));
    assert!(!is_quit_interrupt(&KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::NONE
    )));
    assert!(!is_quit_interrupt(&KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::CONTROL
    )));
}
```

profiles_test.rs（save 失败构造：config.toml 路径被目录占位，profiles/ 仍可写）：

```rust
#[tokio::test]
async fn update_save_failure_is_visible() {
    let server = wiremock::MockServer::start().await;
    mount_sub_endpoint(&server, sub_yaml_nodes(3));
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
    unsafe { std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1") };
    std::fs::create_dir(dir.path().join("config.toml")).unwrap();
    let mut config = base_config();
    config.mihomo.external_controller = server.uri();
    config.add_subscription("s".into(), format!("{}/sub", server.uri()));

    let result = mioctl::subscription::manager::SubscriptionManager::update(
        &mut config,
        &mioctl::subscription::manager::UpdateTarget::Named("s".into()),
    )
    .await
    .unwrap();

    assert!(
        result.contains("config: ERROR -"),
        "result: {}",
        result
    );
    unsafe { std::env::remove_var("MIOCTL_HOME") };
    unsafe { std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL") };
}
```

（`mount_sub_endpoint`/`sub_yaml_nodes`/`base_config` 以 profiles_test.rs 既有 helper 的实际名称与签名为准对齐；ENV_LOCK 守卫照既有模式持有。）

- [ ] **Step 2: 确认失败**

Run: `cargo test --offline test_is_quit_interrupt update_save_failure`
Expected: FAIL（函数不存在 / 无 ERROR 行）

- [ ] **Step 3: 实现**

app.rs 事件循环 `Event::Key(key)` 臂内、Release 过滤之后、`let mut s = state.lock().await;` 之前：

```rust
if is_quit_interrupt(&key) {
    break;
}
```

（`break` 跳出主 loop，走既有终端恢复路径——与裸 `q` 相同；锁尚未获取无泄漏。）

```rust
fn is_quit_interrupt(key: &crossterm::event::KeyEvent) -> bool {
    key.code == crossterm::event::KeyCode::Char('c')
        && key
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL)
}
```

manager.rs update：

```rust
if need_save {
    if let Err(e) = config.save() {
        results.push(format!("config: ERROR - save failed: {}", e));
    }
}
```

ensure_archived：

```rust
if let Err(e) = config.save() {
    warnings.push(format!("config save failed: {}", e));
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline && cargo clippy --tests -- -D warnings`
Expected: 全 PASS

---

### Task 9: 文档收尾 + 终验（L3）

**Files:**
- Modify: `README.md`（在订阅/安全相关小节注明：订阅拉取接受自签证书 `danger_accept_invalid_certs`，属刻意妥协；先 grep README 确认插入点）
- Modify: `CLAUDE.md`（subscription/ 描述补一句：激活合并保留三段之外的一切顶层键，仅删除 `proxy-providers`）

- [ ] **Step 1: README 注明证书策略**

在订阅章节合适位置（grep `subscription` 定位）追加一行：

```markdown
> Note: subscription fetch accepts self-signed TLS certificates (required by some airport panels); node credentials in transit rely on the server's TLS config.
```

- [ ] **Step 2: CLAUDE.md 键保留语义**

把 `**`subscription/`**` 条目中 "activation merges proxies/proxy-groups/rules verbatim into mihomo config (those three sections are fully managed by mioctl — manual edits are overwritten), backup/rollback, reload" 之后补 ", all other top-level keys are preserved (only proxy-providers is removed)" 。

- [ ] **Step 3: 全量终验**

Run:
```bash
cargo test
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo build --release
```
Expected: cargo test 仅 `debug_yaml` + `test_real_subscription_parse` 失败（离线网络豁免）；其余全绿；release 构建成功。

- [ ] **Step 4: 汇总**

向用户汇报：12 项修复对照表（H1/M1-M4/L1-L7 → 任务号）、新增测试清单、门禁结果。不执行 git commit。
