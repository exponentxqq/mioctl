# Mihomo 1.19.x 兼容性改进 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 merger 模板与 install.sh 不一致、增强 install.sh（geodata 下载 + cap 检查 + system service）、新增 `mioctl doctor` 诊断命令。

**Architecture:** 三个独立改动：merger.rs 模板字符串替换 + 测试更新；install.sh 追加三个 shell 逻辑块；新增 doctor 模块（CLI 子命令），在 main.rs 注册路由，无外部依赖。

**Tech Stack:** Rust (tokio async), Bash (POSIX sh), clap derive macros

## Global Constraints

- install.sh 使用 POSIX `sh`（非 bash），不使用 bash 特有关键字
- CLI 子命令遵循现有模式：`pub async fn run(action: DoctorAction)` 签名
- 不改动订阅解析/注入逻辑、API 客户端、TUI
- `cargo clippy -- -D warnings` 零警告
- `cargo fmt --check` 通过

---

### Task 1: 更新 merger.rs 默认模板

**Files:**
- Modify: `src/subscription/merger.rs:13-40`

- [ ] **Step 1: 替换 DEFAULT_TEMPLATE 字符串**

将第 13-40 行的 `DEFAULT_TEMPLATE` 替换为与 install.sh 一致的 fake-ip 模板：

```rust
/// Default template for new config.yaml when none exists.
const DEFAULT_TEMPLATE: &str = r#"mixed-port: 7897
external-controller: 127.0.0.1:9090
mode: rule
log-level: info
allow-lan: false
dns:
  enable: true
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  fake-ip-filter:
    - '*.github.com'
    - github.com
  nameserver:
    - https://223.5.5.5/dns-query
    - https://doh.pub/dns-query
tun:
  enable: true
  stack: gvisor
  auto-route: true
  auto-detect-interface: true
  dns-hijack:
    - any:53
sniffer:
  enable: true
  sniffing:
    - tls
    - http
rules:
  - DST-PORT,22,DIRECT
"#;
```

- [ ] **Step 2: 更新测试断言**

`test_merge_with_default_template_when_no_config` 中第 198 行断言了 `redir-host`，需改为 `fake-ip`：

```rust
assert!(result.yaml.contains("fake-ip"));
```

- [ ] **Step 3: 运行测试**

```bash
cargo test subscription::merger::tests
```
Expected: 4 tests PASS

- [ ] **Step 4: Clippy + fmt 检查**

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

- [ ] **Step 5: Commit**

```bash
git add src/subscription/merger.rs
git commit -m "fix(subscription): align merger default template with install.sh (fake-ip mode)"
```

---

### Task 2: install.sh 增强 — geodata 下载 + cap 完整性检查

**Files:**
- Modify: `install.sh:243` (在 mihomo 下载之后、config 创建之前)

- [ ] **Step 1: 在 setcap 之后添加 geodata 下载逻辑**

在第 243 行（`ok "mihomo $MIHOMO_VERSION → ..."`）之后、config 创建之前插入：

```bash
# ---- download geodata files ----
echo ""
echo "${BOLD}Downloading geodata files...${NC}"

GEO_SITE_DAT_URL="https://github.com/MetaCubeX/meta-rules-dat/releases/latest/download/geosite.dat"
GEO_IP_MMDB_URL="https://github.com/MetaCubeX/meta-rules-dat/releases/latest/download/geoip.metadb"

curl -fsSL -o "$MIHOMO_CONFIG_DIR/geosite.dat" "$GEO_SITE_DAT_URL" && \
  ok "geosite.dat → $MIHOMO_CONFIG_DIR/geosite.dat" || \
  warn "geosite.dat download failed (GEOSITE rules may not work)"

curl -fsSL -o "$MIHOMO_CONFIG_DIR/Country.mmdb" "$GEO_IP_MMDB_URL" && \
  ok "Country.mmdb → $MIHOMO_CONFIG_DIR/Country.mmdb" || \
  warn "Country.mmdb download failed (GEOIP rules may not work)"
```

- [ ] **Step 2: 将 setcap 改成无条件检查修复**

当前第 237 行 `if command -v setcap >/dev/null 2>&1; then` 内部只在首次下载后执行。但这段逻辑已经每次运行都执行了（每次 run 都下载 mihomo），问题不大。但为了防守场景（用户手动替换了二进制），在 setcap 块中增加 `cap_net_raw` 和 `cap_net_bind_service`：

将：
```bash
sudo setcap cap_net_admin+ep "$MIHOMO_BIN_DIR/mihomo" 2>/dev/null && \
```

改为：
```bash
sudo setcap cap_net_admin,cap_net_raw,cap_net_bind_service=+eip "$MIHOMO_BIN_DIR/mihomo" 2>/dev/null && \
```

- [ ] **Step 3: 新增 --system 参数支持**

在脚本开头（第 3 行 `set -eu` 之后）添加参数解析：

```bash
# Parse arguments
INSTALL_SYSTEM=false
while [ $# -gt 0 ]; do
  case "$1" in
    --system) INSTALL_SYSTEM=true ;;
  esac
  shift
done
```

在 service 创建部分（第 282-309 行），当 `INSTALL_SYSTEM=true` 时走系统级路径：

```bash
if [ "$INSTALL_SYSTEM" = true ]; then
  # System-level service: uses /etc/mihomo/ and /usr/bin/mihomo
  echo ""
  echo "${BOLD}Setting up mihomo systemd SYSTEM service...${NC}"

  sudo mkdir -p /etc/mihomo
  sudo cp "$MIHOMO_BIN_DIR/mihomo" /usr/bin/mihomo
  sudo setcap cap_net_admin,cap_net_raw,cap_net_bind_service=+eip /usr/bin/mihomo

  sudo cp "$MIHOMO_CONFIG_DIR/geosite.dat" /etc/mihomo/ 2>/dev/null || true
  sudo cp "$MIHOMO_CONFIG_DIR/Country.mmdb" /etc/mihomo/ 2>/dev/null || true

  # Install start script
  sudo mkdir -p /usr/lib/mihomo
  sudo tee /usr/lib/mihomo/start > /dev/null << 'SYSTEMD_START'
#!/usr/bin/bash
install "${CREDENTIALS_DIRECTORY}/config.yaml" "${STATE_DIRECTORY}"/config.yaml
install /etc/mihomo/geosite.dat -t "${STATE_DIRECTORY}" 2>/dev/null || true
install /etc/mihomo/Country.mmdb -t "${STATE_DIRECTORY}" 2>/dev/null || true
SYSTEMD_START
  sudo chmod +x /usr/lib/mihomo/start

  sudo cp "$MIHOMO_CONFIG_FILE" /etc/mihomo/config.yaml

  sudo tee /etc/systemd/system/mihomo.service > /dev/null << 'SYSTEMD_UNIT'
[Unit]
Description=Mihomo daemon
After=network.target NetworkManager.service systemd-networkd.service iwd.service

[Service]
Type=simple
DynamicUser=yes
Restart=on-failure
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE
RestartSec=5
StateDirectory=mihomo
StateDirectoryMode=0700
ExecStartPre=/usr/lib/mihomo/start
ExecStart=/usr/bin/mihomo -d "$STATE_DIRECTORY"
LoadCredential=config.yaml:/etc/mihomo/config.yaml
ProtectSystem=strict
RemoveIPC=yes
NoNewPrivileges=yes
ProtectClock=yes
ProtectKernelLogs=yes
ProtectKernelModules=yes
PrivateMounts=yes
SystemCallArchitectures=native
MemoryDenyWriteExecute=yes
RestrictNamespaces=true
ProtectHostname=yes
RestrictSUIDSGID=yes
LockPersonality=yes
ProtectKernelTunables=yes
ProtectControlGroups=yes
RestrictRealtime=yes
PrivateTmp=disconnected
ProtectHome=yes
ProtectProc=invisible
ProcSubset=pid
UMask=077

[Install]
WantedBy=multi-user.target
SYSTEMD_UNIT

  sudo systemctl daemon-reload
  sudo systemctl enable mihomo.service
  sudo systemctl restart mihomo.service 2>/dev/null || true
  ok "mihomo system service enabled and started"
else
  # 原有 user service 逻辑保持不变
  ...
fi
```

- [ ] **Step 4: Commit**

```bash
git add install.sh
git commit -m "feat(install): add geodata download, cap integrity check, --system service option"
```

---

### Task 3: 新增 mioctl doctor 诊断命令

**Files:**
- Create: `src/cli/doctor.rs`
- Modify: `src/cli/mod.rs` (add `pub mod doctor;` + register subcommand)
- Modify: `src/main.rs` (add dispatch arm)

#### Task 3a: 创建 doctor 模块

- [ ] **Step 1: 创建 `src/cli/doctor.rs`**

```rust
use crate::config::mioctl_config::MioctlConfig;
use std::process::Command;

#[derive(clap::Subcommand)]
pub enum DoctorAction {
    /// Run diagnostic checks
    Run,
}

pub async fn run(_action: DoctorAction) {
    println!("\n  Mihomo Doctor\n");

    let config = MioctlConfig::load();

    // 1. CAP_NET_ADMIN check
    check_cap_net_admin();

    // 2. Geo data files check
    check_geo_files(&config);

    // 3. Config syntax check
    check_config_syntax(&config);

    // 4. Process conflict check
    check_process_conflict();

    // 5. API reachable check
    check_api_reachable(&config).await;

    // 6. System proxy check
    check_system_proxy();

    println!();
}

fn status(ok: bool, label: &str, detail: &str) {
    let icon = if ok { "\x1b[32m✅\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
    let warn_icon = "\x1b[33m⚠️\x1b[0m";
    // Use warn_icon for non-fatal issues
    println!("  {}  {:<22} {}", icon, label, detail);
}

fn warn(label: &str, detail: &str) {
    println!("  \x1b[33m⚠️\x1b[0m  {:<22} {}", label, detail);
}

fn check_cap_net_admin() {
    // Try common mihomo binary locations
    let candidates = ["/usr/bin/mihomo", &format!("{}/.config/mioctl/bin/mihomo", std::env::var("HOME").unwrap_or_default())];
    let mut found = false;
    for path in &candidates {
        if !std::path::Path::new(path).exists() {
            continue;
        }
        found = true;
        let output = Command::new("getcap")
            .arg(path)
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                if stdout.contains("cap_net_admin") {
                    status(true, "CAP_NET_ADMIN", &format!("{} has required capabilities", path));
                } else {
                    status(false, "CAP_NET_ADMIN", &format!("{} lacks cap_net_admin — TUN mode will fail. Run: sudo setcap cap_net_admin,cap_net_raw,cap_net_bind_service=+eip {}", path, path));
                }
            }
            _ => {
                warn("CAP_NET_ADMIN", &format!("cannot check {} (getcap not found?)", path));
            }
        }
    }
    if !found {
        warn("CAP_NET_ADMIN", "no mihomo binary found at common paths");
    }
}

fn check_geo_files(config: &MioctlConfig) {
    // Derive mihomo config dir from config
    let home = std::env::var("HOME").unwrap_or_default();
    let mihomo_dir = std::path::Path::new(&home).join(".config/mihomo");

    let geosite = mihomo_dir.join("geosite.dat");
    let mmdb = mihomo_dir.join("Country.mmdb");

    let has_geosite = geosite.exists();
    let has_mmdb = mmdb.exists();

    if has_geosite && has_mmdb {
        status(true, "Geo data files", "geosite.dat + Country.mmdb found");
    } else {
        let mut missing = vec![];
        if !has_geosite { missing.push("geosite.dat"); }
        if !has_mmdb { missing.push("Country.mmdb"); }
        status(false, "Geo data files", &format!("missing: {} — GEOSITE/GEOIP rules may fail", missing.join(", ")));
    }
}

fn check_config_syntax(config: &MioctlConfig) {
    let home = std::env::var("HOME").unwrap_or_default();
    let config_path = std::path::Path::new(&home).join(".config/mihomo/config.yaml");

    if !config_path.exists() {
        status(false, "Config syntax", &format!("config not found at {}", config_path.display()));
        return;
    }

    let output = Command::new("mihomo")
        .args(["-t", "-f"])
        .arg(config_path.to_str().unwrap())
        .arg("-d")
        .arg(config_path.parent().unwrap().to_str().unwrap())
        .output();

    match output {
        Ok(o) if o.status.success() => {
            status(true, "Config syntax", "valid");
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Take last meaningful line
            let msg = stderr.lines().last().unwrap_or("unknown error");
            status(false, "Config syntax", &format!("invalid — {}", msg));
        }
        Err(e) => {
            warn("Config syntax", &format!("cannot run mihomo -t: {}", e));
        }
    }
}

fn check_process_conflict() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("ps aux | grep '[m]ihomo' | wc -l")
        .output();

    match output {
        Ok(o) => {
            let count_str = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Ok(count) = count_str.parse::<usize>() {
                if count == 0 {
                    status(false, "Process", "mihomo is not running");
                } else if count == 1 {
                    status(true, "Process", "1 mihomo instance running");
                } else {
                    warn("Process", &format!("{} mihomo instances running — possible port/config conflict", count));
                }
            }
        }
        Err(_) => warn("Process", "cannot check mihomo processes"),
    }
}

async fn check_api_reachable(config: &MioctlConfig) {
    let secret = if config.mihomo.secret.is_empty() {
        None
    } else {
        Some(config.mihomo.secret.clone())
    };

    match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
        Ok(client) => {
            let url = format!("{}/version", client.base_url());
            match client.client().get(&url).send().await {
                Ok(resp) => {
                    let body = resp.text().await.unwrap_or_default();
                    status(true, "API reachable", &format!("{} ({})", config.mihomo.external_controller, body.trim()));
                }
                Err(e) => status(false, "API reachable", &format!("{} — {}", config.mihomo.external_controller, e)),
            }
        }
        Err(e) => {
            status(false, "API reachable", &format!("{} — {}", config.mihomo.external_controller, e));
        }
    }
}

fn check_system_proxy() {
    let mut active = false;
    let mut details = vec![];

    // Check env vars
    for var in &["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"] {
        if std::env::var(var).is_ok() {
            active = true;
            details.push(format!("env:{}", var));
            break;
        }
    }

    // Check gsettings (GNOME)
    if let Ok(o) = Command::new("gsettings").args(["get", "org.gnome.system.proxy", "mode"]).output() {
        let mode = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if mode == "'manual'" || mode == "'auto'" {
            active = true;
            details.push("gsettings".to_string());
        }
    }

    // Check environment.d
    let home = std::env::var("HOME").unwrap_or_default();
    let env_conf = std::path::Path::new(&home).join(".config/environment.d/proxy.conf");
    if env_conf.exists() {
        active = true;
        details.push("environment.d".to_string());
    }

    if active {
        status(true, "System proxy", &format!("configured ({})", details.join(", ")));
    } else {
        warn("System proxy", "not configured — browser traffic won't go through proxy. Set http_proxy/https_proxy or use TUN mode.");
    }
}
```

#### Task 3b: 注册 doctor 到 CLI

- [ ] **Step 2: 修改 `src/cli/mod.rs`**

在第 5 行添加 `pub mod doctor;`：

```rust
pub mod connect;
pub mod doctor;   // <-- 新增
pub mod sub;
pub mod tui;
```

在 `Commands` enum 中添加 `Doctor` 分支：

```rust
#[derive(Subcommand)]
pub enum Commands {
    /// Launch interactive TUI
    Tui,

    /// Manage subscriptions
    Sub {
        #[command(subcommand)]
        action: SubAction,
    },

    /// Test API connectivity
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },

    /// Run diagnostic checks
    Doctor {
        #[command(subcommand)]
        action: DoctorAction,
    },
}

// Re-export DoctorAction for main.rs
pub use doctor::DoctorAction;
```

- [ ] **Step 3: 修改 `src/main.rs`**

在第 33 行 `Some(Commands::Connect { action })` 之后添加：

```rust
Some(Commands::Doctor { action }) => {
    cli::doctor::run(action).await;
}
```

- [ ] **Step 4: 运行编译检查**

```bash
cargo check
```
Expected: Compiles without errors. If `get_version()` doesn't exist on `MihomoClient`, check the API client.

- [ ] **Step 5: 运行 clippy + fmt**

```bash
cargo clippy -- -D warnings
cargo fmt --check
```

- [ ] **Step 6: 手动验证**

```bash
cargo run -- doctor run
```
Expected: 6 check items with colored output (some ✅ some ⚠️ depending on state).

- [ ] **Step 7: Commit**

```bash
git add src/cli/doctor.rs src/cli/mod.rs src/main.rs
git commit -m "feat(cli): add 'mioctl doctor' diagnostic command"
```

---

### Task 4: 集成测试 + 最终验证

- [ ] **Step 1: 添加 doctor 集成测试**

在 `src/cli/doctor.rs` 末尾添加测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_formatting() {
        // Just verify helpers don't panic
        status(true, "Test", "all good");
        status(false, "Test", "something wrong");
        warn("Test", "a warning");
    }

    #[tokio::test]
    async fn test_check_process_conflict_does_not_panic() {
        check_process_conflict();
    }
}
```

- [ ] **Step 2: 运行全部测试**

```bash
cargo test
```
Expected: All tests PASS

- [ ] **Step 3: 最终检查**

```bash
cargo clippy -- -D warnings
cargo fmt --check
cargo build --release
```

- [ ] **Step 4: Commit**

```bash
git commit -m "test: add basic tests for doctor command"
```

---

### Task 5: 验证 install.sh 改动

- [ ] **Step 1: 语法检查**

```bash
sh -n install.sh
```
Expected: No output (no syntax errors)

- [ ] **Step 2: Commit**

```bash
git add install.sh
git commit -m "feat(install): add --system flag, geodata download, cap check"
```
