# mioctl

基于 [mihomo](https://github.com/MetaCubeX/mihomo)（Clash.Meta）的终端管理工具，提供交互式 TUI 界面，支持完整的 REST API 操作和订阅管理。

## 功能特性

- **交互式 TUI** — 侧边栏导航，5 个视图（概览、代理、连接、规则、日志）
- **完整 API 封装** — 所有 mihomo REST 端点，WebSocket 实时数据流
- **TUN & 系统代理** — Dashboard 显示 TUN / 系统代理 / 端口状态，`p` 键一键切换
- **订阅管理** — URL 添加/更新/删除，自动识别格式并注入 proxy-provider
- **代理控制** — 策略组浏览、节点切换、延迟测试、模式切换
- **Vim 风格快捷键**，Catppuccin Mocha 配色

## 安装

```bash
cargo install --path .
```

## 快速开始

### 1. 确保 mihomo 已启动

在 mihomo 配置中启用外部控制器：

```yaml
external-controller: 127.0.0.1:9090
secret: "你的密钥" # 可选
```

### 2. 连接测试

```bash
mioctl connect test
```

输出 `✓ Connected to mihomo v1.18.0` 表示连接成功。

### 3. 启动 TUI

```bash
mioctl tui
```

### 4. 管理订阅

```bash
# 添加订阅（在配置文件中手动添加，暂无 CLI 添加命令）
# 编辑 ~/.config/mioctl/config.toml

# 更新全部订阅
mioctl sub update --all
```

## 配置文件

配置文件位于 `~/.config/mioctl/config.toml`，首次运行自动创建：

```toml
[mihomo]
# mihomo 外部控制器地址
external-controller = "127.0.0.1:9090"
# API 密钥（与 mihomo 配置中的 secret 一致）
secret = ""

[subscriptions]
# 自动更新间隔（分钟），默认 240
update-interval-minutes = 240

[[subscriptions.items]]
name = "我的机场"
url = "https://example.com/api/v1/client/xxxxxxxx"

[[subscriptions.items]]
name = "免费节点"
url = "https://free.example.com/sub"
```

## 快捷键

### 全局

| 按键                   | 功能                                  |
| ---------------------- | ------------------------------------- |
| `1` `2` `3` `4` `5`    | 切换视图（概览/代理/连接/规则/日志）  |
| `j` / `k` 或 `↑` / `↓` | 上下移动                              |
| `g`                    | 跳到顶部                              |
| `G`                    | 跳到底部                              |
| `/`                    | 搜索 / 过滤                           |
| `n` / `N`              | 下一个 / 上一个搜索结果               |
| `r`                    | 手动刷新数据                          |
| `m`                    | 切换代理模式（全局 → 规则 → 直连）    |
| `p`                    | 切换代理开关（TUN / 系统代理 / 关闭） |
| `?`                    | 显示帮助                              |
| `q`                    | 退出                                  |

### 代理视图

| 按键                   | 功能                       |
| ---------------------- | -------------------------- |
| `h` / `l` 或 `←` / `→` | 切换上一个 / 下一个策略组  |
| `Enter`                | 切换到选中的节点           |
| `t`                    | 测试当前节点延迟           |
| `T`                    | 测试当前策略组全部节点延迟 |
| `Esc`                  | 从节点列表回到策略组列表   |

### 连接视图

| 按键 | 功能           |
| ---- | -------------- |
| `d`  | 关闭选中的连接 |
| `D`  | 关闭全部连接   |

### 日志视图

| 按键    | 功能                                                      |
| ------- | --------------------------------------------------------- |
| `Space` | 暂停 / 恢复实时滚动                                       |
| `s`     | 切换日志级别过滤（info → warning → error → debug → 全部） |

## 视图说明

### 📊 概览

默认视图。第一行显示代理模式、上下行速率、连接数；第二行显示 TUN 状态、系统代理状态、混合端口、LAN 开放状态；下方为流量趋势图、策略组表格（显示每个组的当前活跃节点）、内存和版本信息。

按 `p` 可一键切换代理状态：有任何代理活跃时按 `p` 全部关闭，全部关闭时按 `p` 开启 TUN 模式。

### 🔗 代理

左侧策略组列表，右侧节点详情表格。显示节点名称、类型、延迟和选中状态。支持即时切换和延迟测试。国旗 emoji 自动转换为 `[XX]` 格式以确保终端兼容性。

### 🌐 连接

所有活跃连接表格。显示源地址、目标地址、代理链路、匹配规则和流量。支持关闭单个或全部连接。

### 📋 规则

路由规则列表，包含规则类型、匹配条件和目标策略。

### 📜 日志

实时滚动日志流，按日志级别着色（info=绿、warning=黄、error=红、debug=灰）。支持暂停和级别过滤。

## 系统代理

mioctl 通过 `~/.config/environment.d/proxy.conf`（systemd 用户环境）管理系统代理设置，无需 sudo。

- **检测**：读取 proxy.conf，检查 `HTTP_PROXY` 是否指向 mihomo 端口
- **启用**：写入 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` / `NO_PROXY`，并执行 `systemctl --user import-environment` 刷新当前会话
- **禁用**：删除 proxy.conf 并刷新环境变量

TUN 模式通过 mihomo `PATCH /configs` API 直接控制，与系统代理联动切换。

## 支持的订阅格式

mioctl 自动检测并解析以下订阅格式：

| 格式            | 说明                                                   |
| --------------- | ------------------------------------------------------ |
| Clash YAML 配置 | 完整的 mihomo/clash 配置文件，自动提取 `proxies:` 部分 |
| Base64 编码列表 | 常见于机场订阅，解码后逐行解析节点 URI                 |
| 纯文本 URI 列表 | 每行一个代理节点 URI                                   |

支持的节点协议：Shadowsocks (`ss://`)、Vmess (`vmess://`)、Trojan (`trojan://`)

## 命令行

```
mioctl tui              启动交互式 TUI
mioctl connect test     测试 API 连接
mioctl sub update --all 更新全部订阅
```

## 项目结构

```
~/.config/mioctl/
├── config.toml           # mioctl 配置
└── providers/            # 生成的 proxy-provider 文件
    └── my-sub.yaml

~/.config/environment.d/
└── proxy.conf            # 系统代理环境变量（自动管理）
```

## 常见问题

**Q: 启动 TUI 后显示 disconnected？**
A: 检查 mihomo 是否已启动且 `external-controller` 配置正确。运行 `mioctl connect test` 验证。

**Q: 订阅更新失败？**
A: 检查订阅 URL 是否可访问。部分订阅需要特定 User-Agent（mioctl 使用 `clash-verge/1.3.8`），或服务器使用自签名证书（mioctl 已支持）。

**Q: 如何添加新的订阅？**
A: 在 `~/.config/mioctl/config.toml` 的 `[[subscriptions.items]]` 段中添加 name 和 url，然后运行 `mioctl sub update --all`。

**Q: 按 `p` 切换 TUN 没有反应？**
A: 确认 mihomo 配置中包含 `tun` 段。TUN 模式需要 mihomo 以相应权限运行（如 Linux 上需要 `CAP_NET_ADMIN`）。

## 开发

```bash
cargo test                          # 运行全部测试（~120 个）
cargo test --test integration_test  # 集成测试（wiremock）
cargo clippy -- -D warnings         # lint
cargo build --release               # 编译发布版本
```

## 许可

MIT
