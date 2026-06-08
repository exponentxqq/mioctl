# mioctl Design Specification

**Date**: 2026-06-08
**Status**: Approved

## Overview

mioctl 是一个基于 Rust + ratatui 的 mihomo 终端管理工具，通过 mihomo RESTful API (external-controller) 管理代理节点、订阅、连接和配置。

## Goals

- 完整的 mihomo API 封装（REST + WebSocket）
- 交互式 TUI 仪表盘（侧边栏布局，5 个视图）
- URL 订阅管理：添加、更新、删除，自动解析多格式并注入 proxy-provider
- 代理节点管理：策略组浏览、节点切换、延迟测试
- 配置文件管理：加载、校验、热重载

## Non-Goals

- 不实现代理内核功能（仅管理工具）
- 首版仅支持单个本地 mihomo 实例（不跨机器）
- 不作为系统服务运行
- 不提供 GUI（Web/桌面）

---

## Architecture

### 3-Layer Structure

```
┌─────────────────────────────────────┐
│  UI Layer (ratatui TUI + CLI cmds)  │
│  src/ui/  src/cli/                  │
├─────────────────────────────────────┤
│  Business Logic Layer               │
│  src/app/  src/subscription/        │
│  src/config/                        │
├─────────────────────────────────────┤
│  API Client Layer (reqwest + WS)    │
│  src/api/                           │
└─────────────────────────────────────┘
```

- **API Client** (`src/api/`): MihomoClient 封装 reqwest + secret 鉴权，类型安全的强类型响应，WebSocket 实时流（logs/traffic/connections/memory）
- **Business Logic** (`src/app/`, `src/subscription/`, `src/config/`): ProxyManager 节点缓存与切换，SubscriptionManager 订阅生命周期，MioctlConfig 自身配置
- **UI** (`src/ui/`, `src/cli/`): ratatui TUI 侧边栏布局 + 3 个精简 CLI 子命令

### Project Tree

```
src/
├── main.rs                 # CLI arg parse, dispatch
├── api/                    # Mihomo API Client
│   ├── client.rs           # MihomoClient: reqwest + secret auth
│   ├── endpoints.rs        # All REST methods
│   ├── websocket.rs        # WS streams (traffic/logs/connections)
│   ├── types.rs            # Strong-typed response structs
│   └── error.rs            # ApiError enum
├── config/
│   └── mioctl_config.rs    # MioctlConfig + load/save
├── app/                    # Business logic
│   ├── proxy_manager.rs    # Node cache, switch, latency test
│   ├── connection_manager.rs
│   └── state.rs            # Global shared AppState
├── subscription/           # Subscription management
│   ├── manager.rs          # SubscriptionStore CRUD
│   ├── fetcher.rs          # HTTP fetch
│   ├── parser.rs           # Format parsing (YAML / Base64 / URI)
│   └── injector.rs         # Generate proxy-provider file + API inject
├── ui/                     # TUI (ratatui)
│   ├── app.rs              # Event loop + view routing
│   ├── theme.rs            # Catppuccin Mocha colors
│   ├── keybindings.rs      # Vim-style shortcuts
│   ├── views/
│   │   ├── dashboard.rs    # Overview
│   │   ├── proxies.rs      # Proxy nodes
│   │   ├── connections.rs  # Active connections
│   │   ├── rules.rs        # Routing rules
│   │   ├── logs.rs         # Real-time logs
│   │   └── sidebar.rs      # Sidebar component
│   └── widgets/
│       ├── status_bar.rs
│       ├── sparkline.rs
│       └── table.rs
└── cli/                    # CLI subcommands
    ├── tui.rs              # mioctl tui
    ├── sub.rs              # mioctl sub update --all
    └── connect.rs          # mioctl connect test
```

### Tech Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust | Single binary, no runtime deps |
| TUI | ratatui | Most mature Rust TUI ecosystem |
| HTTP | reqwest + tokio | Async, WS support |
| CLI args | clap | Standard Rust CLI framework |
| Config format | TOML (~/.config/mioctl/config.toml) | Rust ecosystem standard |
| Serialization | serde + serde_json + serde_yaml | De facto standard |

---

## TUI Design

### Layout: Sidebar + Main Area

```
┌────────────┬──────────────────────────────────┐
│  📊 概览   │                                  │
│  🔗 代理   │     Main Content Area            │
│  🌐 连接   │     (switches per view)          │
│  📋 规则   │                                  │
│  📜 日志   │                                  │
│            │                                  │
│ ─────────  │                                  │
│  ⚙ 设置    │                                  │
│  🔄 更新   │                                  │
├────────────┴──────────────────────────────────┤
│ 🟢 connected | mihomo v1.18.0 | updated 12:30 │  ← status bar
└───────────────────────────────────────────────┘
```

### Views

1. **📊 概览** (default): Mode indicator, up/down traffic rate, connection count, memory, sparkline traffic history, mihomo version
2. **🔗 代理**: Left column = policy groups, right column = node table (name, type, latency, selected). `/` search, Enter switch, `t` latency test, `Ctrl+T` test all
3. **🌐 连接**: Active connection table (source, destination, proxy, rule, traffic). `d` close one, `D` close all
4. **📋 规则**: Scrollable rule list, read-only, `/` search
5. **📜 日志**: Real-time scrolling log stream, `s` toggle level filter (info/warn/error/debug/all), Space pause/resume

### Color Scheme: Catppuccin Mocha

| Usage | Color |
|-------|-------|
| Background | `#1e1e2e` |
| Surface | `#313244` |
| Primary accent (purple) | `#cba6f7` |
| Up / success (green) | `#a6e3a1` |
| Down / error (red) | `#f38ba8` |
| Warning (yellow) | `#f9e2af` |
| Text primary | `#cdd6f4` |
| Text secondary | `#a6adc8` |

### Keybindings (Vim-style)

**Global:**

| Key | Action |
|-----|--------|
| `1-5` | Switch view (1=dashboard, 2=proxies, 3=connections, 4=rules, 5=logs) |
| `j` / `k` | Move down / up |
| `g g` | Jump to top |
| `G` | Jump to bottom |
| `/` | Search / filter |
| `n` / `N` | Next / previous search match |
| `:` | Command mode (`:reload`, `:q`) |
| `q` | Quit |

**Dashboard:**

| Key | Action |
|-----|--------|
| `m` | Cycle proxy mode (Global → Rule → Direct) |

**Proxies:**

| Key | Action |
|-----|--------|
| `h` / `l` | Prev / next policy group |
| `Enter` | Switch to selected node |
| `t` | Test selected node latency |
| `Ctrl+T` | Test all nodes in current group |
| `Esc` | Back to group list from node list |

**Connections:**

| Key | Action |
|-----|--------|
| `d` | Close selected connection |
| `D` | Close all connections |

**Logs:**

| Key | Action |
|-----|--------|
| `Space` | Pause / resume scrolling |
| `s` | Cycle log level filter |

---

## Data Flow

### Real-time data (WebSocket)

- traffic, memory, logs, connections → WebSocket push → UI update on each event
- Auto-reconnect on disconnect (exponential backoff, max 5 retries)

### Polled data (REST)

- proxies, rules, providers → HTTP GET every 3 seconds → diff with cache → UI update if changed

### Subscription update flow

```
1. User: mioctl sub add <url> [name]
   → Save to ~/.config/mioctl/config.toml [subscriptions] section

2. Fetcher: GET <url> (User-Agent: mioctl/0.1, timeout: 30s)
   → 200 OK → format detection
     ├─ YAML (Content-Type or parse attempt): serde_yaml → extract proxies[]
     ├─ Base64: decode → split lines → parse URIs
     └─ Plain URI: split lines → parse URIs
   → Error: report and skip

3. Parser: URI schemes (ss://, vmess://, trojan://, vless://, hysteria2://)
   → Vec<ProxyNode>

4. Injector: Generate proxy-provider YAML → ~/.config/mioctl/providers/<name>.yaml
   → PUT /providers/proxies/<name> (tell mihomo to load this file)

5. mihomo auto health-checks and loads the nodes
```

---

## Subscription Injection Mechanism

Chosen: **Proxy-provider file mode** (Option A)

mioctl writes parsed nodes as a YAML file in `~/.config/mioctl/providers/<sub-name>.yaml` containing:

```yaml
proxies:
  - {name: "🇯🇵 Japan-01", type: ss, server: ..., port: 443, cipher: ..., password: ...}
  - {name: "🇸🇬 Singapore-02", type: vmess, server: ..., port: 443, uuid: ..., alterId: 0}
```

Then calls `PUT /providers/proxies/<sub-name>` to instruct mihomo. The user's main mihomo config references this provider via:

```yaml
proxy-providers:
  <sub-name>:
    type: file
    path: ~/.config/mioctl/providers/<sub-name>.yaml
    health-check:
      enable: true
      url: https://www.gstatic.com/generate_204
      interval: 300
```

---

## CLI Commands

Three subcommands only:

| Command | Purpose |
|---------|---------|
| `mioctl tui` | Launch interactive TUI |
| `mioctl sub update --all` | Update all subscriptions (script/cron use) |
| `mioctl connect test` | Verify API connectivity |

---

## Configuration

`~/.config/mioctl/config.toml`:

```toml
[mihomo]
external-controller = "127.0.0.1:9090"
secret = ""

[subscriptions]
update-interval-minutes = 240
[[subscriptions.items]]
name = "example-sub"
url = "https://example.com/sub"
last_updated = "2026-06-08T12:00:00Z"

[preferences]
delay-test-url = "https://www.gstatic.com/generate_204"
delay-test-timeout-ms = 5000
theme = "catppuccin-mocha"
```

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| mihomo API changes | Version compatibility matrix, CI API detection |
| WebSocket disconnect | Auto-reconnect, exponential backoff, data catch-up on reconnect |
| Large subscription parsing blocks UI | Async parsing in tokio task, progress indicator |
| ratatui immediate-mode learning curve | Start with simple views, iterate |
| Single instance assumption | Future: profile system for multi-instance |

## Open Questions

- Unix socket connection support? (defer, TCP first)
- Delay test URL: default to `generate_204`, user-configurable via preferences
