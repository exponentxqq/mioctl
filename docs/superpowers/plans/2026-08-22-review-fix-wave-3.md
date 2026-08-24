# 评审修复第三波 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 2026-08-22 第三轮全分支复审（两名独立评审 + 控制器核实）确认的 3 High + 3 Medium + 若干 Low 问题，全部经代码级实证。

**Architecture:** 全部为局部修复，不动订阅激活/回滚状态机：unique_name 终止保证、update 结构化失败标志（退出码判定去子串化）、TUI 输入模态并发守卫、config 持久化健壮化（原子写 + 读错误分类 + TUI 非零退出）、README/CLAUDE.md 同步、base64 宽松解码 + probe 阈值、名字逗号校验 + 死代码清理、UI 健壮性（终端恢复 / 持锁剪贴板 / loading 覆盖）。

**Tech Stack:** Rust（tokio / clap / serde_yaml / reqwest / wiremock / base64），ratatui + crossterm。

## Background（ condensed ）

第三轮复审针对全部未提交工作（wave-1 + wave-2 修复波），发现 3 High + 3 Medium + 若干 Low。本波全部修复。发现 → 任务映射：H1→T1（unique_name 无限循环）、H2→T2（update 退出码 `contains("ERROR")` 子串误报/漏报）、H3→T3（sub_input 提交无 loading 守卫）、M1→T5（README/CLAUDE.md 陈旧）、M2→T4（config 持久化）、M3→T6（抓取解码）；Low：EACCES 误改名 + TUI 错误退出码 0→T4、probe 阈值≥3 误拒 1-2 节点订阅→T6、名字逗号 + 死代码→T7、UI 健壮性→T8。

## Global Constraints

- **禁止一切 git 状态变更命令**（不 commit / 不 stage / 不 stash / 不 checkout）；只读 git 命令（`git status` / `git diff` / `git log`）允许。全部改动保持未提交工作树
- 不添加任何代码注释
- TDD 强制：每个行为修复先有 RED 证据再实现（编译错误亦可作为 RED 信号，沿 wave-2 惯例；Task 1 的 (a) 测试以"修复前必然挂起、无法运行"作为 RED 证据记录在任务报告中）
- 每任务操作规程：
  1. 编辑前快照：`mkdir -p /tmp/opencode/pre-tN && cp <本任务全部目标文件> /tmp/opencode/pre-tN/`
  2. 完成后：`git diff -u -- <目标文件> > .superpowers/sdd/task-N-diff.txt`（只读 diff，允许）
  3. 完整报告写 `.superpowers/sdd/task-N-report.md`（RED 证据、实现摘要、验证输出、偏离记录）
  4. 后续任务的 diff 基线是上一任务末态的工作树
- 每任务门禁：本任务定向测试 + `cargo clippy --tests -- -D warnings` + `cargo fmt --check`
- `cargo test` 全量：仅 `debug_yaml` 与 `test_real_subscription_parse` 允许失败（离线网络豁免）
- 测试封闭性：涉及 env 的测试设置 `MIOCTL_HOME` + `MIOCTL_TEST_NO_SYSTEMCTL=1`，并持有 `crate::testutil::env_lock()`（bin target 的 testutil 在 main.rs，lib target 在 lib.rs）
- 当前基线：247×2 lib（lib+bin 双 target）+ 14 integration + 11 profiles 通过

---

### Task 1 (H1): unique_name 终止保证

**Files:**
- Modify: `src/subscription/manager.rs:31-43`（`unique_name`）、`:176-179`（`add()` 自动命名路径）
- Test: `src/subscription/manager.rs` tests

**Interfaces:**
- Produces: `fn unique_name(base: &str, existing: &[String]) -> Result<String, String>`（私有，返回 Err 表示 200 次尝试后仍碰撞）
- Consumes: 既有 `crate::subscription::profile::name_conflicts`（不改）

**缺陷实证：** `sanitize_filename`（profile.rs:13-19）对每个候选名做 `chars().take(80)` 截断。当 `sanitize_filename(base) ≥ 80` 且与既有项冲突时，`format!("{} ({})", base, i)` 的后缀 `" ({i})"` 被截断丢弃——每个候选的 sanitize 都等于 sanitize(base)，循环永不着陆。

**修复契约：** 候选名从截短的 stem 派生：`let stem: String = base.chars().take(70).collect();`，候选为 `format!("{} ({})", stem, i)`，`i` 取 `2..=200`；200 次后仍冲突返回 Err。依据：70 + `" (200)"` = 76 chars < 80，位数增长在尝试上限内保持候选互异。

- [ ] **Step 1: 先写/改测试（不得在实现前运行 test (a)——它在旧代码上必然挂起）**

改写既有 `test_unique_name_appends_suffix`（:469-475，`unique_name` 返回 Result 后需要 `.unwrap()`）：

```rust
#[test]
fn test_unique_name_appends_suffix() {
    let mut names = vec!["base".to_string()];
    assert_eq!(unique_name("base", &names).unwrap(), "base (2)");
    names.push("base (2)".to_string());
    assert_eq!(unique_name("base", &names).unwrap(), "base (3)");
    assert_eq!(unique_name("other", &names).unwrap(), "other");
}
```

新增（a）超长名冲突终止（修复前挂起，禁止预跑）：

```rust
#[test]
fn test_unique_name_long_base_conflict_yields_distinct_short_name() {
    let long = "x".repeat(100);
    let existing = vec![long.clone()];
    let result = unique_name(&long, &existing).unwrap();
    let sanitized = crate::subscription::profile::sanitize_filename(&result);
    assert!(sanitized.chars().count() <= 80, "got: {}", sanitized);
    assert_ne!(
        sanitized,
        crate::subscription::profile::sanitize_filename(&long),
        "sanitized candidate must differ from sanitized base"
    );
}
```

新增（c）耗尽返回 Err：

```rust
#[test]
fn test_unique_name_exhaustion_returns_error() {
    let base = "base";
    let mut existing: Vec<String> = vec![base.to_string()];
    for i in 2..=200 {
        existing.push(format!("{} ({})", base, i));
    }
    assert!(unique_name(base, &existing).is_err());
}
```

（base 短于 70 chars，stem == base，199 个候选全覆盖。）

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_unique_name`
Expected: 编译失败——`unique_name` 返回 `String`，测试按 `Result` 使用（`.unwrap()` / `.is_err()` 不可用）。此编译失败即签名层 RED。**(a) 的运行时 RED 无法采集：旧实现对该输入无限循环，测试会挂起整个进程——将此论证写入 task-1-report.md 作为 (a) 的 RED 证据，且实现未落地前禁止运行 (a)。**

- [ ] **Step 3: 实现**

`src/subscription/manager.rs:31-43` 整体替换：

```rust
fn unique_name(base: &str, existing: &[String]) -> Result<String, String> {
    if !name_conflicts(base, existing) {
        return Ok(base.to_string());
    }
    let stem: String = base.chars().take(70).collect();
    for i in 2..=200 {
        let candidate = format!("{} ({})", stem, i);
        if !name_conflicts(&candidate, existing) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not derive a unique name from '{}' after 199 attempts",
        base
    ))
}
```

调用点 `add()` 自动命名路径（:176-179）：

```rust
None => {
    let base = detect_subscription_name(&content).or_else(|_| name_from_url(url))?;
    unique_name(&base, &existing)?
}
```

- [ ] **Step 4: 验证（此时才允许运行 test (a)）**

Run: `cargo test --offline test_unique_name && cargo test --offline manager && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS（含 (a) (c) 新测试与既有 `add_auto_detects_name_and_suffixes_collisions`——短名场景 stem==base，`Auto (2)` 行为不变）

---

### Task 2 (H2): update 结构化失败标志

**Files:**
- Modify: `src/subscription/manager.rs:239-319`（`update` 返回 `UpdateReport`）
- Modify: `src/cli/sub.rs:57-75`（Update 臂）、`src/cli/sub.rs` tests（新增 child 模式 + env 传递 helper）
- Modify: `src/ui/app.rs:588-603`（`spawn_update_subscription`）
- Modify: `src/subscription/manager.rs` tests（`test_update_target_validation` :731-737）
- Modify: `tests/profiles_test.rs`（4 处 update 调用点断言适配）
- Test: `src/subscription/manager.rs` tests + `src/cli/sub.rs` tests

**Interfaces:**
- Produces: `pub struct UpdateReport { pub lines: Vec<String>, pub failed: bool }`（manager.rs，置于 `UpdateTarget` 旁）；`pub async fn update(config: &mut MioctlConfig, target: &UpdateTarget) -> Result<UpdateReport, String>`
- 行格式契约：既有 `"{name}: ERROR - {e}"` 行全部保留；re-merge 失败行从 `"{name}: re-merge failed - {e}"` 改为 `"{name}: ERROR - re-merge failed - {e}"`（且 failed=true）；config save 失败行 `"config: ERROR - save failed: {e}"` 格式不变（failed=true）
- Consumes: Task 1 之后的 manager.rs 现状

**缺陷实证：** sub.rs:68 用 `result.lines().any(|l| l.contains("ERROR"))` 判定退出码——订阅名（可经远程 proxy-group 名自动命名，远程可控）含 "ERROR" 即误报失败；manager.rs:302 的 `"{name}: re-merge failed - {e}"` 无 ERROR 标记——真实失败漏报 exit 0。

- [ ] **Step 1: 失败测试**

manager.rs tests 新增（离线：fetch 连接 127.0.0.1:1 立即拒绝）：

```rust
#[tokio::test]
async fn test_update_fetch_failure_sets_failed_flag() {
    let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
    config.add_subscription("s".into(), "http://127.0.0.1:1/sub".into());
    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("s".into()))
        .await
        .unwrap();
    assert!(report.failed, "lines: {:?}", report.lines);
    assert!(report
        .lines
        .iter()
        .any(|l| l.contains("s: ERROR - "), "lines: {:?}", report.lines));
}
```

re-merge 失败路径（wiremock 正常供数 + mihomo 配置非法 → activate 失败）：

```rust
#[tokio::test]
async fn test_update_remerge_failure_marks_failed() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/sub"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(SUB_YAML))
        .mount(&mock)
        .await;
    let (_env, mut config) = TestEnv::new("invalid: [yaml\n");
    config.add_subscription("sub1".into(), format!("{}/sub", mock.uri()));
    config.set_active(Some("sub1"));
    let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("sub1".into()))
        .await
        .unwrap();
    assert!(report.failed, "lines: {:?}", report.lines);
    assert!(report
        .lines
        .iter()
        .any(|l| l.contains("sub1: ERROR - re-merge failed -"), "lines: {:?}", report.lines));
}
```

（TestEnv 已设 `MIOCTL_TEST_NO_SYSTEMCTL=1`，reload 分支离线安全。）

判别性 child-dispatch 测试（THE discriminator）。sub.rs tests：

dispatch helper 增加 env 传递（既有 `dispatch_child` 委托之，13 处调用点零改动）：

```rust
fn dispatch_child_with_env(
    mode: &str,
    home: &Path,
    extra: &[(&str, &str)],
) -> std::process::Output {
    let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        "cli::sub::tests::cli_sub_child_dispatch",
        "--nocapture",
    ])
    .env("MIOCTL_TEST_CHILD", mode)
    .env("MIOCTL_HOME", home);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    assert!(
        stdout_of(&out).contains("running 1 test"),
        "child ran no tests; stdout: {}; stderr: {}",
        stdout_of(&out),
        stderr_of(&out)
    );
    out
}

fn dispatch_child(mode: &str, home: &Path) -> std::process::Output {
    dispatch_child_with_env(mode, home, &[])
}
```

`cli_sub_child_dispatch` 开头插入 child 侧配置写入（URL 经 env 传入，child 在 run() 前落盘 config）：

```rust
if mode == "update-error-name" {
    let url = std::env::var("MIOCTL_TEST_UPDATE_URL").unwrap();
    let home = std::env::var("MIOCTL_HOME").unwrap();
    std::fs::write(
        std::path::Path::new(&home).join("config.toml"),
        format!("[[subscriptions.items]]\nname = \"ERRORS\"\nurl = \"{url}\"\n"),
    )
    .unwrap();
}
```

（active 不设置——update 不触发 re-merge，成功路径输出单行。）`action_for` 增加模式：

```rust
"update-error-name" => SubAction::Update {
    name: Some("ERRORS".into()),
    all: false,
},
```

父测试：

```rust
#[tokio::test]
async fn update_name_containing_error_substring_exits_zero() {
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/sub"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
            "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: ERRORS\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,ERRORS\n",
        ))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/sub", server.uri());
    let out = dispatch_child_with_env(
        "update-error-name",
        dir.path(),
        &[("MIOCTL_TEST_UPDATE_URL", url.as_str())],
    );
    assert!(out.status.success());
    let stdout = stdout_of(&out);
    assert!(
        stdout.contains("ERRORS: 1 nodes updated"),
        "stdout: {}",
        stdout
    );
    assert!(
        !stdout.contains("__MIOCTL_EXIT_1__"),
        "name containing 'ERROR' must not be mistaken for failure, stdout: {}",
        stdout
    );
}
```

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_update_fetch_failure_sets_failed test_update_remerge_failure update_name_containing_error`
Expected: 编译失败（`UpdateReport` 不存在）为签名 RED；实现前不得运行 remerge/child 测试。**关键判据预演（写入报告）：** 现行 sub.rs:68 对 `"ERRORS: 1 nodes updated"` 做 `contains("ERROR")` → 命中 → run 返回 false → child 打出 `__MIOCTL_EXIT_1__` → 新断言必失败；这正是本任务消除的误报。

- [ ] **Step 3: 实现**

manager.rs——`UpdateTarget` 枚举之后新增：

```rust
pub struct UpdateReport {
    pub lines: Vec<String>,
    pub failed: bool,
}
```

`update` 签名与错误路径（保留全部既有行格式，仅 re-merge 行加前缀；标 `failed`）：

```rust
pub async fn update(
    config: &mut MioctlConfig,
    target: &UpdateTarget,
) -> Result<UpdateReport, String> {
```

循环前：`let mut failed = false;`。三处 ERROR 行后追加 `failed = true;`：

```rust
results.push(format!("{}: ERROR - {}", name, e));
failed = true;
```

（write_archive 失败 :277、normalize 失败 :307、fetch 失败 :309 三处同款。）re-merge 失败 :302：

```rust
Err(e) => {
    results.push(format!("{}: ERROR - re-merge failed - {}", name, e));
    failed = true;
}
```

config save 失败 :315：

```rust
if let Err(e) = config.save() {
    results.push(format!("config: ERROR - save failed: {}", e));
    failed = true;
}
```

返回值 :318：

```rust
Ok(UpdateReport {
    lines: results,
    failed,
})
```

cli/sub.rs Update 臂（:65-74）：

```rust
match SubscriptionManager::update(&mut config, &target).await {
    Ok(report) => {
        println!("{}", report.lines.join("\n"));
        !report.failed
    }
    Err(e) => {
        eprintln!("Error: {}", e);
        false
    }
}
```

ui/app.rs `spawn_update_subscription`（:594-597）——UX 文案不变（join 后与原单串一致）：

```rust
match result {
    Ok(report) => st.add_log("info", &report.lines.join("\n")),
    Err(e) => st.add_log("error", &format!("Update failed: {}", e)),
}
```

manager.rs tests `test_update_target_validation` 末段（:731-736）：

```rust
let report = SubscriptionManager::update(&mut config, &UpdateTarget::All)
    .await
    .unwrap();
assert!(report.lines.is_empty());
assert!(!report.failed);
```

tests/profiles_test.rs 4 处调用点适配（断言内容不变，仅取值方式）——`full_lifecycle_add_use_update_remove` 两处、`update_normalize_failure_reports_error_line`、`update_save_failure_is_visible`，统一模式：

```rust
let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("subA".into()))
    .await
    .unwrap();
let result = report.lines.join("\n");
assert!(!report.failed);
assert!(result.contains("subA: 1 nodes updated"));
```

（`update_normalize_failure_reports_error_line` 与 `update_save_failure_is_visible` 追加 `assert!(report.failed);`。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline manager && cargo test --offline cli::sub && cargo test --offline --test profiles_test && cargo test --offline ui::app && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS——含既有 `update_fetch_fail_ok_result_error_line_yields_nonzero_exit`（report.failed 路径，marker 仍打出）、`test_update_subscription_spawn_fetch_failure_logs_error`（行格式未变）

---

### Task 3 (H3): sub_input 提交 loading 守卫

**Files:**
- Modify: `src/ui/app.rs:276-283`（事件循环 sub_input 分支；提交逻辑提取为可测函数）
- Test: `src/ui/app.rs` tests

**Interfaces:**
- Produces: `fn handle_sub_input_submitted(s: &mut AppState, shared: SharedState, url: String)`（私有；loading 占用时不 spawn）
- Consumes: 既有 `handle_sub_input_key`、`spawn_add_subscription`、`LoadingKind::AddSub`/`UpdateSubs`

**缺陷实证：** app.rs:276-283 的 sub_input 提交分支直接 `s.ui.loading = Some(LoadingKind::AddSub)` 并 spawn，无 `s.ui.loading.is_none()` 守卫（confirm/SwitchNode/SubUpdate 均已有，wave-2 T2）。并发下 `config.save()` 竞态。

- [ ] **Step 1: 失败测试**

```rust
#[tokio::test]
async fn test_sub_input_submit_ignored_while_loading() {
    let _env = TestEnv::new();
    let shared = crate::app::state::new_shared_state();
    let mut s = shared.lock().await;
    s.ui.sub_input_mode = true;
    s.ui.sub_input = "https://x".into();
    s.ui.loading = Some(LoadingKind::UpdateSubs);
    let url = match handle_sub_input_key(&mut s.ui, KeyCode::Enter) {
        SubInputOutcome::Submitted(url) => url,
        SubInputOutcome::Canceled | SubInputOutcome::Editing => {
            panic!("expected Submitted")
        }
    };
    handle_sub_input_submitted(&mut s, shared.clone(), url);
    assert!(!s.ui.sub_input_mode, "key must still be consumed by input handler");
    assert_eq!(
        s.ui.loading,
        Some(LoadingKind::UpdateSubs),
        "submit must not spawn while an operation is in flight"
    );
}
```

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_sub_input_submit_ignored`
Expected: 编译失败（`handle_sub_input_submitted` 不存在）。若先做机械提取不加守卫，则测试运行时 FAIL（loading 被覆盖为 `AddSub`）——两种证据任一均有效，报告中记录实际形态。

- [ ] **Step 3: 实现**

提取提交逻辑（新增私有函数，置于 `handle_sub_input_key` 之后）：

```rust
fn handle_sub_input_submitted(s: &mut AppState, shared: SharedState, url: String) {
    if s.ui.loading.is_none() {
        s.ui.loading = Some(LoadingKind::AddSub);
        let cfg = s.config.clone();
        spawn_add_subscription(shared, cfg, url);
    }
}
```

事件循环分支（:276-283）替换为：

```rust
} else if s.ui.sub_input_mode {
    if let SubInputOutcome::Submitted(url) = handle_sub_input_key(&mut s.ui, key.code) {
        handle_sub_input_submitted(&mut s, state.clone(), url);
    }
}
```

（按键仍被 `handle_sub_input_key` 消费、输入模式照常退出；loading 期间提交被静默忽略——与 wave-2 T2 行为一致。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline ui::app && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS（既有 sub_input 键处理测试不受影响）

---

### Task 4 (M2 + L): config 持久化健壮化

**Files:**
- Modify: `src/config/mioctl_config.rs:125-156`（`load` 读错误分类 + `save` 原子写；新增 `recover_corrupt` 与 `read_error_is_transient`）
- Modify: `src/main.rs:23-27`（TUI 臂非零退出）
- Test: `src/config/mioctl_config.rs` tests

**Interfaces:**
- Produces: `save()` 写 `<path>.tmp` 后 rename（镜像 merger.rs `write_config` 模式），签名不变；`load()` 读错误分类：NotFound → 默认值不改名（TOCTOU 竞态容忍）；PermissionDenied → 默认值**不**改名（配置可能完好只是不可读，保留原文件）；其余读错误（InvalidData 非 UTF-8、EISDIR 等）→ 尽力 rename `.corrupt` + 默认值（真损坏保留现状）；`fn read_error_is_transient(e: &std::io::Error) -> bool` 与 `fn recover_corrupt(path: &std::path::Path) -> Self`（均私有）
- main.rs TUI 臂：`if let Err(e)` → eprintln + `std::process::exit(1)`（对齐 Sub/Connect 语义；doctor 保持 exit 0）

- [ ] **Step 1: 失败测试**

```rust
#[test]
fn test_save_atomic_leaves_no_tmp() {
    let _guard = crate::testutil::env_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
    let config = MioctlConfig::default();
    config.save().unwrap();
    assert!(dir.path().join("config.toml").exists());
    assert!(!dir.path().join("config.toml.tmp").exists());
    let reloaded = MioctlConfig::load();
    assert!(reloaded.subscriptions.items.is_empty());
    unsafe { std::env::remove_var("MIOCTL_HOME") };
}

#[test]
fn test_read_error_kind_classification() {
    assert!(read_error_is_transient(&std::io::Error::from(
        std::io::ErrorKind::NotFound
    )));
    assert!(read_error_is_transient(&std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )));
    assert!(!read_error_is_transient(&std::io::Error::from(
        std::io::ErrorKind::InvalidData
    )));
    assert!(!read_error_is_transient(&std::io::Error::from(
        std::io::ErrorKind::Other
    )));
}

#[test]
fn test_load_config_path_is_directory_recovers_to_default() {
    let _guard = crate::testutil::env_lock().lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
    std::fs::create_dir(dir.path().join("config.toml")).unwrap();

    let config = MioctlConfig::load();

    assert!(config.subscriptions.items.is_empty());
    if dir.path().join("config.toml.corrupt").exists() {
        assert!(
            !dir.path().join("config.toml").exists(),
            "rename should have moved the directory away"
        );
    } else {
        assert!(
            dir.path().join("config.toml").exists(),
            "original must be untouched when rename of a directory fails"
        );
    }
    unsafe { std::env::remove_var("MIOCTL_HOME") };
}
```

（EISDIR 分支断言写成两可形态：Linux 上 `rename(dir → 不存在的 .corrupt)` 成功，预期走第一分支；运行后在 task-4-report.md 记录实际分支，若环境行为不同按实际收紧断言。**PermissionDenied 集成测试明确 SKIP**：chmod 000 在 root 下 CI 仍可读，测试将环境依赖性失败——以 `test_read_error_kind_classification` 单元覆盖分类逻辑，理由写入报告。既有 `test_load_corrupt_config_preserves_original_as_corrupt` 与 `test_load_unreadable_config_preserves_original_as_corrupt`（InvalidData 路径）必须保持绿。）

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_save_atomic test_read_error_kind test_load_config_path_is_directory`
Expected: 前两个 FAIL（.tmp 残留 / 函数不存在）；第三个当前行为：EISDIR 落入 `Err(_)` 改名分支——若 rename 目录成功则碰巧 PASS，此时 RED 证据以前两个测试为准（报告中说明）。

- [ ] **Step 3: 实现**

`save`（:150-156）替换：

```rust
pub fn save(&self) -> Result<(), String> {
    let dir = Self::config_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
    let path = Self::config_path();
    let tmp = dir.join("config.toml.tmp");
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}
```

`load`（:125-148）与两个新私有函数替换：

```rust
pub fn load() -> Self {
    let path = Self::config_path();
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(config) => config,
                Err(_) => Self::recover_corrupt(&path),
            },
            Err(e) if read_error_is_transient(&e) => Self::default(),
            Err(_) => Self::recover_corrupt(&path),
        }
    } else {
        let config = Self::default();
        let _ = config.save();
        config
    }
}

fn read_error_is_transient(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    )
}

fn recover_corrupt(path: &std::path::Path) -> Self {
    let corrupt = path.with_extension("toml.corrupt");
    let _ = std::fs::rename(path, corrupt);
    Self::default()
}
```

（`config.toml`.with_extension("toml.corrupt") → `config.toml.corrupt`，与既有命名一致。）

main.rs TUI 臂（:23-27）：

```rust
None | Some(Commands::Tui) => {
    if let Err(e) = cli::tui::run().await {
        eprintln!("TUI error: {}", e);
        std::process::exit(1);
    }
}
```

- [ ] **Step 4: 验证**

Run: `cargo test --offline config:: && cargo test --offline && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS（profiles_test 的 `update_save_failure_is_visible` / `ensure_archived_save_failure_warns` 用 config.toml 目录占位构造 save 失败——写 tmp 成功、rename 到目录失败，仍产生失败行，断言不变）

---

### Task 5 (M1): README/CLAUDE.md 同步

**Files:**
- Modify: `README.md`（:30-38 特性行、:62-79 订阅指引、:92-98 配置示例、:126-168 快捷键表、:170-192 视图说明、:226-232 命令行、:259-263 FAQ）
- Modify: `CLAUDE.md`（`cli/` 条目补 doctor）

**Interfaces:**
- 无代码接口；只改文档。编辑时逐条对照源码核实（sidebar 实为 `src/ui/views/sidebar.rs` 渲染 6 视图、`parse_key` :64-66 `6`→SwitchView(5)=Subscriptions、:184-192 `u`→SubUpdate / `a`→SubAdd、订阅视图 footer "Enter 激活 · u 更新 · a 添加 · d 删除"）——不得引入新的不准确表述

**已核实的陈旧点（全部修复）：**
1. README:30 "5 个视图（概览、代理、连接、规则、日志）"——实际 6 个（订阅视图，key 6）
2. README:34 "自动识别格式并注入 proxy-provider"——现行架构是归档 + 三段 verbatim 合入，**不**注入 proxy-provider
3. 快捷键表缺 `6` / `u` / `a`（订阅视图键位）与订阅视图小节
4. CLI 节缺 `sub add/use/remove/register/list` 与 `doctor`（doctor 为顶层子命令，cli/mod.rs:35-38 已核实）
5. README:62-79 与 :262-263 手工 `[[subscriptions.items]]` 指引 → 改为 `mioctl sub add <url>`
6. README:67-68、:92-94 `update-interval-minutes = 240`——`MioctlConfig`/`Subscriptions` 结构体无此字段（mioctl_config.rs:36-42，未知键被 serde 静默忽略），属未实现配置的虚假文档

- [ ] **Step 1: README 修正**

:30 与 :34 行替换为：

```markdown
- **交互式 TUI** — 侧边栏导航，6 个视图（概览、代理、连接、规则、日志、订阅）
```

```markdown
- **订阅管理** — URL 添加/更新/切换/删除，订阅归档为 `~/.config/mioctl/profiles/*.yaml`，激活时将 proxies/proxy-groups/rules 三段原样合入 mihomo 配置（其余顶层键保留，不注入 proxy-provider）
```

"### 4. 管理订阅"（:62-79）整节替换：

```markdown
### 4. 管理订阅

添加订阅（名称自动从订阅内容识别，也可 `--name` 指定；首个订阅自动激活）：

```bash
mioctl sub add https://example.com/api/v1/client/xxxxxxxx
```

常用操作：

```bash
mioctl sub list                  列出订阅（* 为当前）
mioctl sub use <name>            切换当前订阅
mioctl sub update --all          更新全部订阅
mioctl sub remove <name>         删除订阅
```
```

配置文件示例（:85-107）中删除两处 `update-interval-minutes` 注释/键值行，`[subscriptions]` 段改为：

```toml
[subscriptions]
# active 标记当前激活的订阅（由 mioctl sub use 维护，可省略）
# active = "我的机场"

[[subscriptions.items]]
name = "我的机场"
url = "https://example.com/api/v1/client/xxxxxxxx"
```

全局快捷键表（:130-142）首行替换为：

```markdown
| `1` `2` `3` `4` `5` `6` | 切换视图（概览/代理/连接/规则/日志/订阅） |
```

"### 连接视图"（:154-159）之后新增小节：

```markdown
### 订阅视图

| 按键    | 功能              |
| ------- | ----------------- |
| `Enter` | 激活选中的订阅    |
| `u`     | 更新选中的订阅    |
| `a`     | 输入 URL 添加订阅 |
| `d`     | 删除选中的订阅    |
```

"### 📜 日志"（:190-192）之后新增视图说明：

```markdown
### 📁 订阅

订阅列表（`*` 为当前激活）。`Enter` 激活、`u` 更新、`a` 添加、`d` 删除。激活即将订阅的 proxies/proxy-groups/rules 三段合入 mihomo 配置并 reload。
```

命令行节（:226-232）替换为：

```markdown
```
mioctl tui               启动交互式 TUI
mioctl connect test      测试 API 连接
mioctl doctor            诊断 mihomo 环境
mioctl sub add <url>     添加订阅（--name 指定名称，--activate 立即激活）
mioctl sub register <url> 注册订阅（add 的别名，不激活）
mioctl sub use <name>    切换当前订阅
mioctl sub update --all  更新全部订阅
mioctl sub remove <name> 删除订阅（--yes 跳过确认）
mioctl sub list          列出订阅（* 为当前）
```
```

FAQ"如何添加新的订阅？"（:262-263）替换为：

```markdown
**Q: 如何添加新的订阅？**
A: 运行 `mioctl sub add <url>`（TUI 订阅视图按 `a` 也可）。手动编辑 `[[subscriptions.items]]` 仍可行，但订阅的 proxies/proxy-groups/rules 三段由 mioctl 管理，下次更新/激活时会被覆盖。
```

- [ ] **Step 2: CLAUDE.md 修正**

`**`cli/`**` 条目（"clap CLI (tui/sub/connect subcommands; ..."）改为：

```markdown
- **`cli/`** — clap CLI (tui/sub/connect/doctor subcommands; `sub` = list/add/register(alias)/use/update/remove)
```

- [ ] **Step 3: 验证**

Run: `rg -n "5 个视图|proxy-provider|update-interval" README.md; rg -c "订阅" README.md`
Expected: 前三项零命中；订阅相关小节存在。再人工通读一遍改动区域，确认无新增失实表述（视图数、键位、命令名与 cli/mod.rs 一一对应）。

---

### Task 6 (M3 + L): 抓取解码健壮化

**Files:**
- Modify: `src/subscription/parser.rs`（新增共享 helper `decode_base64_lenient` + `pad_base64`）
- Modify: `src/subscription/profile.rs:54-60`（删除 `try_base64_decode`，:118 改用 helper；import 更新）
- Modify: `src/subscription/fetcher.rs:26`（probe 阈值 ≥3 → ≥1）、`:74-84`（`count_proxy_entries` 解码改用 helper）
- Test: `src/subscription/parser.rs` + `src/subscription/fetcher.rs` + `src/subscription/profile.rs` tests

**Interfaces:**
- Produces: `pub fn decode_base64_lenient(s: &str) -> Option<String>`（parser.rs）：trim → 移除全部空白（`chars().filter(|c| !c.is_whitespace())`）→ 试 STANDARD；失败则补 `=` 至 4 倍数重试 STANDARD；再失败以同样补齐逻辑试 URL_SAFE；全失败返回 None；空输入返回 None
- **范围决策（刻意）：** parser.rs:20 的严格解码位于 `detect_format` 内部——该函数属 Task 7 删除的死代码，本任务**不**转换该处（避免为将删代码做无用功）；Task 7 一并移除。`parse_shadowsocks` 内部 URL_SAFE 回退链（parser.rs:198-203）保留不动
- **范围决策（刻意）：** probe 不重构为委托 normalize（最小安全修复仅动阈值 + 计数器内的解码器）；但 `count_proxy_entries` 的 base64 回退解码同步换成宽松 helper——否则宽松解码放行的未填充/换行订阅仍会被严格计数探针拒绝，M3 修复不闭环

- [ ] **Step 1: 失败测试**

parser.rs tests：

```rust
#[test]
fn test_decode_base64_lenient_unpadded() {
    let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388, cipher: aes-256-gcm, password: p }\n";
    let mut b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
    while b64.ends_with('=') {
        b64.pop();
    }
    assert_eq!(decode_base64_lenient(&b64).unwrap(), yaml);
}

#[test]
fn test_decode_base64_lenient_line_wrapped() {
    let yaml = "proxies:\n  - { name: N1, type: ss, server: 1.2.3.4, port: 8388 }\n";
    let b64 = base64::engine::general_purpose::STANDARD.encode(yaml);
    let wrapped: String = b64
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(decode_base64_lenient(&wrapped).unwrap(), yaml);
}

#[test]
fn test_decode_base64_lenient_url_safe_alphabet() {
    let b64 = base64::engine::general_purpose::URL_SAFE.encode("😀");
    assert!(b64.contains('-'), "test premise: URL_SAFE output must use URL-safe chars, got {}", b64);
    assert_eq!(decode_base64_lenient(&b64).unwrap(), "😀");
}

#[test]
fn test_decode_base64_lenient_garbage_is_none() {
    assert!(decode_base64_lenient("!!!not base64!!!").is_none());
    assert!(decode_base64_lenient("   ").is_none());
}
```

（前提校验："😀" 的 URL_SAFE 编码含 `-`（0xF0 0x9F 0x98 0x80 → `8J-YgA==`），STANDARD 拒绝 `-`，只有 URL_SAFE 分支能解——测试自带前提断言防退化。）

profile.rs tests（端到端接线证明）：

```rust
#[test]
fn test_normalize_unpadded_base64_uri_list() {
    let uri_list = "ss://Y2hhY2hhMjAtaWV0Zi1wb2x5MTMwNTpwYXNzd29yZA@1.2.3.4:8388#N1\n";
    let mut b64 = base64::engine::general_purpose::STANDARD.encode(uri_list);
    while b64.ends_with('=') {
        b64.pop();
    }
    let p = normalize_to_yaml("unpadded", &b64).unwrap();
    assert_eq!(p.node_count, 1);
    assert!(p.yaml.contains("name: N1"));
}
```

fetcher.rs tests：

```rust
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
```

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_decode_base64_lenient test_normalize_unpadded test_fetch_with_ua_probe_accepts_single`
Expected: parser/profile 测试编译失败（helper 不存在）；fetcher 测试运行时 FAIL（`all User-Agent probes failed`——单节点计数 1 < 3）

- [ ] **Step 3: 实现**

parser.rs 顶部（`SubscriptionFormat` 之前，Task 7 删除时避开此块）新增：

```rust
pub fn decode_base64_lenient(s: &str) -> Option<String> {
    let cleaned: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return None;
    }
    let decode_standard = |input: &str| -> Option<String> {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(input)
            .ok()?;
        String::from_utf8(decoded).ok()
    };
    if let Some(text) = decode_standard(&cleaned) {
        return Some(text);
    }
    let padded = pad_base64(&cleaned);
    if let Some(text) = decode_standard(&padded) {
        return Some(text);
    }
    let decode_url_safe = |input: &str| -> Option<String> {
        let decoded = base64::engine::general_purpose::URL_SAFE
            .decode(input)
            .ok()?;
        String::from_utf8(decoded).ok()
    };
    if let Some(text) = decode_url_safe(&cleaned) {
        return Some(text);
    }
    decode_url_safe(&padded)
}

fn pad_base64(s: &str) -> String {
    let mut out = s.to_string();
    while out.len() % 4 != 0 {
        out.push('=');
    }
    out
}
```

profile.rs——删除 `try_base64_decode`（:54-60），import 改为：

```rust
use crate::subscription::parser::{
    decode_base64_lenient, parse_subscription_full, parse_uri_list, SubscriptionContent,
};
```

`normalize_to_yaml`（:118）：

```rust
if let Some(decoded) = decode_base64_lenient(content) {
```

fetcher.rs——`fetch_with_ua_probe` 判据（:26）：

```rust
if count_proxy_entries(&body) >= 1 {
```

（函数上方既有 doc 注释中 ">= 3 valid proxy entries" 文案同步改为 ">= 1"——修改既有注释允许，新增注释禁止。）`count_proxy_entries` 解码段（:78-84）替换：

```rust
let direct = count_entry_lines(body);
if direct >= 3 {
    return direct;
}
if let Some(text) = crate::subscription::parser::decode_base64_lenient(body) {
    return count_entry_lines(&text);
}
direct
```

（`use base64::Engine;` 若再无其他使用则移除该 import。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline parser profile fetcher && cargo test --offline && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS。重点回归：`test_count_proxy_entries_base64_below_threshold_falls_back`（计数语义未变，仍返回 2）、`test_fetch_with_ua_probe_rejects_thin_body`（"hello" 计数 0，仍拒绝）、`test_normalize_invalid_utf8_base64`（STANDARD 解码成功但非 UTF-8 → helper 各分支均 None → 走 parse_uri_list 报错，仍 Err）

---

### Task 7 (L): 名字逗号校验 + 死代码清理

**Files:**
- Modify: `src/subscription/manager.rs:166-175`（`add()` 显式名分支拒绝含逗号名）
- Modify: `src/subscription/parser.rs`（删除 `SubscriptionFormat`、`detect_format`、`parse_yaml`、`parse_base64`、`parse_proxy_value` 及其测试）
- Modify: `tests/sub_test.rs`（`test_real_subscription_parse` 迁移到 live API）
- Test: `src/subscription/manager.rs` tests

**Interfaces:**
- Produces: `add()` 显式名含 `,` → `Err`（逗号破坏生成的 `MATCH,{name}` 规则与组名）。register 走同一 `add()` 路径（sub.rs:35），自动覆盖
- 死代码删除前置核实（已验证，实现时重跑）：`rg -n "detect_format|parse_yaml|parse_base64|SubscriptionFormat|parse_proxy_value" src/ tests/` → 仅 parser.rs 自身 + 其测试 + tests/sub_test.rs 引用；`parse_proxy_value` 仅被 `parse_yaml` 调用（parser.rs:73,:80），删 `parse_yaml` 后成死代码须一并删除（否则 clippy dead_code 报警）

- [ ] **Step 1: 失败测试（逗号校验）**

manager.rs tests（wiremock 供有效订阅——名字校验在 fetch 之后，离线 URL 会先死于网络错误）：

```rust
#[tokio::test]
async fn test_add_rejects_name_with_comma() {
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/sub"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(SUB_YAML))
        .mount(&mock)
        .await;
    let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
    let err = SubscriptionManager::add(
        &mut config,
        &format!("{}/sub", mock.uri()),
        Some("a,b".into()),
        true,
        false,
    )
    .await
    .unwrap_err();
    assert!(err.contains("commas"), "got: {}", err);
    assert!(config.subscriptions.items.is_empty());
}
```

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_add_rejects_name_with_comma`
Expected: FAIL——当前无逗号校验，add 成功返回（err 为 `unwrap_err` panic 或断言失败）

- [ ] **Step 3: 实现**

manager.rs `add()` 显式名分支（:167-175）：

```rust
Some(n) => {
    if n.contains(',') {
        return Err(format!(
            "subscription name '{}' must not contain commas — they break the generated MATCH rule and group name",
            n
        ));
    }
    if name_conflicts(&n, &existing) {
        return Err(format!(
            "subscription '{}' already exists. Remove it first or use a different --name.",
            n
        ));
    }
    n
}
```

parser.rs 删除（含各自测试 `test_detect_yaml_clash`、`test_detect_yaml_simple`、`test_detect_plain_uri`、`test_detect_base64_yaml`、`test_parse_inline_yaml`、`test_parse_full_clash_config`、`test_parse_yaml_empty`、`test_parse_base64`、`test_parse_base64_invalid`）：

- `pub enum SubscriptionFormat`（:5-9）
- `pub fn detect_format`（:11-47）
- `pub fn parse_yaml`（:49-83）
- `fn parse_proxy_value`（:85-158）
- `pub fn parse_base64`（:177-184）

删除前重跑核实命令确认无新增引用；若出现计划外引用（如 Task 6 新增误用），停下报告，不得强删。

tests/sub_test.rs `test_real_subscription_parse`（:1-44）迁移到 live API（保留"真实订阅端到端解析"意图，仍是离线豁免测试）：

```rust
#[tokio::test]
async fn test_real_subscription_parse() {
    let url = "https://example.com/api/v1/client/xxxxxxxx";

    let content = mioctl::subscription::fetcher::fetch_with_ua_probe(url, None)
        .await
        .expect("fetch failed");
    eprintln!("Fetched: {} bytes", content.len());
    eprintln!("Preview:\n{}", &content[..content.len().min(500)]);

    let profile = mioctl::subscription::profile::normalize_to_yaml("real", &content)
        .expect("normalize failed");
    eprintln!("Success: {} nodes", profile.node_count);
    for warning in &profile.warnings {
        eprintln!("warning: {}", warning);
    }
}
```

（`debug_yaml` 不使用死 API——仅 fetch + serde_yaml 直查——保持原样。若迁移中发现 `normalize_to_yaml` 无法表达原测试意图且改造风险大于价值，允许保留死 API 并在 task-7-report.md 论证——执行者裁决，须记录。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS（debug_yaml / test_real_subscription_parse 仍为仅有的离线豁免失败）；clippy 零 dead_code 报警

---

### Task 8 (L): UI 健壮性

**Files:**
- Modify: `src/ui/app.rs:141-420`（`run_tui` 拆分：事件循环提取为 `event_loop`，清理段无条件执行）
- Modify: `src/ui/app.rs:357-370`（visual `y` 复制：先 drop 锁再跑命令）
- Modify: `src/ui/app.rs:1042-1065`（`Action::LogCopy`：剪贴板 + 日志写入移入 spawn_blocking）
- Modify: `src/ui/app.rs:299-337`（mode selector Enter 提取为 `handle_mode_selector_enter` + loading 守卫）、`:622-638`（Refresh）、`:825-827`（proxies 视图 SwitchNode）、`:851-853`（TestNodeDelay）、`:880-882`（TestGroupDelay）——四处 loading 赋值守卫
- Test: `src/ui/app.rs` tests

**Interfaces:**
- Produces:
  - `async fn event_loop(terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>, state: &SharedState, spark: &TrafficSpark, proxy_table: &mut ratatui::widgets::TableState, conn_table: &mut ratatui::widgets::TableState) -> Result<(), String>`（私有）——`run_tui` 无论 Ok/Err 均先 abort + 终端恢复再向上传播
  - `async fn handle_mode_selector_enter(s: &mut AppState, shared: SharedState)`（私有；loading 占用时仅关闭选择器、不赋值不 spawn）
  - 行为守卫（无新接口）：Refresh / proxies 视图 SwitchNode / TestNodeDelay / TestGroupDelay 的 `s.ui.loading = Some(...)` 赋值仅在 `s.ui.loading.is_none()` 时执行；**代理操作本身保持始终允许**（spawn 照旧，它们不写 config.toml）。任务完成回调里的 `s.ui.loading = None` 属既有完成语义，本任务刻意不动（范围决策，写入报告）

- [ ] **Step 1: 失败测试**

```rust
#[tokio::test]
async fn test_mode_selector_enter_ignored_while_loading() {
    let shared = crate::app::state::new_shared_state();
    let mut s = shared.lock().await;
    s.ui.show_mode_selector = true;
    s.ui.loading = Some(LoadingKind::UpdateSubs);
    handle_mode_selector_enter(&mut s, shared.clone()).await;
    assert!(!s.ui.show_mode_selector, "selector must still close");
    assert_eq!(
        s.ui.loading,
        Some(LoadingKind::UpdateSubs),
        "loading must not be replaced while an operation is in flight"
    );
}

#[tokio::test]
async fn test_proxy_view_switch_node_keeps_subscription_loading() {
    let shared = crate::app::state::new_shared_state();
    let mut s = shared.lock().await;
    s.ui.active_view = Proxies;
    s.client = Some(
        crate::api::client::MihomoClient::new("127.0.0.1:1", None).unwrap(),
    );
    s.groups = vec![make_group("G", vec!["a"])];
    s.ui.loading = Some(LoadingKind::UpdateSubs);
    handle_action(&Action::SwitchNode, &mut s, shared.clone()).await;
    assert_eq!(
        s.ui.loading,
        Some(LoadingKind::UpdateSubs),
        "proxy op must not replace an in-flight subscription loading indicator"
    );
}
```

（断言在 spawn 尚未被轮询的单线程 runtime 内同步完成——后台任务随后的 `loading = None` 不影响断言；构造方式对齐 `client_for` 的 `MihomoClient::new(&addr, secret)` 用法。）

- [ ] **Step 2: RED 证据**

Run: `cargo test --offline test_mode_selector_enter_ignored test_proxy_view_switch_node_keeps`
Expected: 第一个编译失败（函数不存在）；实现提取但未加守卫时运行必 FAIL（loading 被覆盖为 SwitchMode / SwitchNode）——两段证据都写入报告

- [ ] **Step 3: 实现**

(a) `run_tui` 拆分——`:247` 的 `loop {` 至 `:406` 的 `}` 整体移入新函数（循环体逐字保留，仅机械适配引用）：

```rust
async fn event_loop(
    terminal: &mut Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    state: &SharedState,
    spark: &TrafficSpark,
    proxy_table: &mut ratatui::widgets::TableState,
    conn_table: &mut ratatui::widgets::TableState,
) -> Result<(), String> {
    loop {
        ...
    }
    Ok(())
}
```

循环体内机械适配（无语义变化）：`terminal.draw(...)` / `event::poll` / `event::read` 的 `?` 原样保留；`render_frame(f, &s, &spark, &mut proxy_table, &mut conn_table)` → `render_frame(f, &s, spark, proxy_table, conn_table)`（参数已是引用）；`state.lock().await` 不变（`&SharedState` 上 lock 可用）；裸 `break` → `return Ok(())`（两处：`is_quit_interrupt` 与 `handle_action` 返回 false 的退出）。`run_tui` 尾段替换：

```rust
    let result = event_loop(
        &mut terminal,
        &state,
        &spark,
        &mut proxy_table,
        &mut conn_table,
    )
    .await;

    init_handle.abort();

    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
    result
```

(b) 持锁剪贴板——事件循环 visual `y` 臂（现 :357-370）替换：

```rust
KeyCode::Char('y') => {
    let text = collect_log_selection(&s);
    s.add_log("info", &format!("Visual copy: {} chars", text.len()));
    s.ui.log_visual = false;
    drop(s);
    let copied = copy_to_clipboard(&text);
    if !copied {
        let mut s = state.lock().await;
        s.add_log(
            "error",
            "xclip failed — is xclip installed? (pacman -S xclip)",
        );
    }
    continue;
}
```

`Action::LogCopy` 臂（现 :1042-1065）替换——handle_action 借用调用方守卫、无法就地 drop，剪贴板与日志写入移入 spawn_blocking（文案逐字保留）：

```rust
Action::LogCopy => {
    if s.ui.active_view == Logs {
        let text = if s.ui.log_visual {
            let t = collect_log_selection(s);
            s.ui.log_visual = false;
            t
        } else if let Some(entry) = s.logs.get(s.ui.log_cursor) {
            entry.payload.clone()
        } else {
            return true;
        };
        let shared2 = shared.clone();
        tokio::task::spawn_blocking(move || {
            let copied = copy_to_clipboard(&text);
            let mut st = shared2.blocking_lock();
            if !copied {
                st.add_log(
                    "error",
                    "Clipboard unavailable — install wl-clipboard (Wayland) or xclip (X11)",
                );
            } else {
                st.add_log("info", &format!("Copied: {} chars", text.len()));
            }
        });
    }
}
```

(c) loading 赋值守卫——mode selector Enter 提取（spawn 体逐字保留）：

```rust
async fn handle_mode_selector_enter(s: &mut AppState, shared: SharedState) {
    let idx = s.ui.mode_selector_idx;
    let target = match idx {
        0 => ProxyMode::Rule,
        1 => ProxyMode::Global,
        2 => ProxyMode::Direct,
        _ => ProxyMode::Rule,
    };
    s.ui.show_mode_selector = false;
    if s.ui.loading.is_some() {
        return;
    }
    s.ui.loading = Some(LoadingKind::SwitchMode);
    let c = s.client.clone();
    let s2 = shared.clone();
    tokio::spawn(async move {
        if let Some(ref client) = c {
            match ProxyManager::set_proxy_mode(client, &target).await {
                Ok(()) => {
                    refresh_state(&s2).await;
                    let mut s = s2.lock().await;
                    s.add_log("info", &format!("Mode switched to {:?}", target));
                    s.ui.loading = None;
                }
                Err(e) => {
                    let mut s = s2.lock().await;
                    s.add_log("error", &format!("Failed to switch mode: {}", e));
                    s.ui.loading = None;
                }
            }
        } else {
            let mut s = s2.lock().await;
            s.ui.loading = None;
        }
    });
}
```

事件循环 `KeyCode::Enter` 臂（现 :299-337）替换为：

```rust
KeyCode::Enter => {
    handle_mode_selector_enter(&mut s, state.clone()).await;
}
```

同款赋值守卫（spawn 与回调不动）四处：

```rust
if s.ui.loading.is_none() {
    s.ui.loading = Some(LoadingKind::Refresh);
}
```

（Action::Refresh，:623；`let c`/`shared2`/spawn 原样跟在后面）

```rust
if let (Some(c), Some(gn), Some(nn)) = (client, group_name, node_name) {
    if s.ui.loading.is_none() {
        s.ui.loading = Some(LoadingKind::SwitchNode);
    }
    let shared2 = shared.clone();
    ...
}
```

（proxies 视图 SwitchNode，:825-827；TestNodeDelay :852 与 TestGroupDelay :881 同构：在各自 `if let (Some(c), ...)` 内、spawn 之前包一层 `if s.ui.loading.is_none()` 赋值。注意 SwitchNode 订阅视图分支 :813 已有守卫，不动。）

- [ ] **Step 4: 验证**

Run: `cargo test --offline ui::app && cargo test --offline && cargo clippy --tests -- -D warnings && cargo fmt --check`
Expected: 全 PASS（既有 ui::app 全部测试含 Task 3 新增守卫测试保持绿；(a) 为编译级重构，行为不变——成功路径 Ok(())、错误路径经清理后传播给 Task 4 的 main.rs exit(1)）

---

### Task 9: 终验 + 台账

**Files:**
- Modify: `.superpowers/sdd/progress.md`（追加第三波小节）
- Create: `.superpowers/sdd/task-9-report.md`

- [ ] **Step 1: 全量终验**

Run:
```bash
cargo test
cargo clippy --tests -- -D warnings
cargo fmt --check
cargo build --release
```
Expected: cargo test 仅 `debug_yaml` + `test_real_subscription_parse` 失败（离线网络豁免），其余全绿；clippy / fmt 干净；release 构建成功。记录各 target 通过数并与基线对比（lib 应为 247×2 + 本波新增）。

- [ ] **Step 2: 台账**

`.superpowers/sdd/progress.md` 末尾追加（沿用第二波格式：每任务一行 + 最终状态）：

```markdown
## 修复第三波 (2026-08-22, plan: docs/superpowers/plans/2026-08-22-review-fix-wave-3.md, 基线: 当前未提交工作树)
fix-T1: complete (unique_name stem 70 + 2..=200 + Result；超长名 (a) 以挂起论证为 RED)
fix-T2: complete (UpdateReport{lines,failed} + cli 子串判定移除 + re-merge 行加 ERROR 前缀 + update-error-name 判别测试)
fix-T3: complete (sub_input 提交 loading 守卫，提取 handle_sub_input_submitted)
fix-T4: complete (save tmp+rename；load 读错误三分；main TUI exit(1)；EACCES chmod 测试 SKIP 论证)
fix-T5: complete (README 6 视图/三段合入/键位表/CLI/FAQ/update-interval 清理；CLAUDE.md 补 doctor)
fix-T6: complete (decode_base64_lenient 共享 helper + probe ≥1 + 计数器宽松解码；parser.rs:20 刻意不动归 T7)
fix-T7: complete (add 显式名逗号拒绝 + 死代码四删 + sub_test 迁移 normalize_to_yaml)
fix-T8: complete (event_loop 拆分清理无条件化 + 剪贴板脱离状态锁 + 四处 loading 赋值守卫 + mode selector 提取)
最终门禁: <cargo test 计数> / clippy / fmt / release 结果
状态: 修复第三波完成
```

`task-9-report.md` 写入发现 → 任务 → 状态对照表：

| 发现 | 任务 | 状态 |
| --- | --- | --- |
| H1 unique_name 截断碰撞无限循环 | T1 | done |
| H2 update 退出码 contains("ERROR") 误报/漏报 | T2 | done |
| H3 sub_input 提交无 loading 守卫 | T3 | done |
| M1 README/CLAUDE.md 陈旧 | T5 | done |
| M2 config 持久化（非原子写 + 读错误误改名） | T4 | done |
| Low: TUI 错误退出码 0 | T4 | done |
| Low: EACCES 配置被改名 | T4 | done |
| M3 抓取解码（严格 base64 拒未填充/换行/URL-safe） | T6 | done |
| Low: probe 阈值 ≥3 拒 1-2 节点订阅 | T6 | done |
| Low: 名字逗号破坏 MATCH 规则 | T7 | done |
| Low: parser 死代码 (detect_format/parse_yaml/parse_base64) | T7 | done |
| Low: UI 健壮性（终端恢复/持锁剪贴板/loading 覆盖） | T8 | done |

- [ ] **Step 3: 汇总**

向用户汇报：12 项修复对照表、新增测试清单（按任务）、门禁结果、与 wave-2 相比的测试增量。不执行任何 git 状态变更命令。
