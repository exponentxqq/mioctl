# mioctl

基于 [mihomo](https://github.com/MetaCubeX/mihomo)（Clash.Meta）的终端管理工具，提供交互式 TUI 界面，支持完整的 REST API 操作和订阅管理。

## 一键安装

```bash
curl -fsSL https://raw.githubusercontent.com/exponentxqq/mioctl/main/install.sh | sh
```

安装脚本自动完成：

- 下载 mioctl 与 mihomo 二进制
- 安装剪贴板依赖（`xclip` / `wl-clipboard`）
- 生成默认配置（`~/.config/mioctl/config.toml`、`~/.config/mihomo/config.yaml`）
- 配置 systemd 用户服务并启用 mihomo
- 授予 `CAP_NET_ADMIN` 权限以支持 TUN 模式

支持 Linux（x86_64）、macOS（x86_64 / arm64）、Windows（Git Bash / WSL）。

ARM64 Linux 用户需要 Rust 工具链：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
curl -fsSL https://raw.githubusercontent.com/exponentxqq/mioctl/main/install.sh | sh
```

## 功能特性

- **交互式 TUI** — 侧边栏导航，5 个视图（概览、代理、连接、规则、日志）
- **完整 API 封装** — 所有 mihomo REST 端点，WebSocket 实时数据流，Braille 旋转加载提示
- **TUN & 系统代理** — Dashboard 显示 TUN / 系统代理 / 端口状态，`p` 键一键切换，两者互斥
- **系统代理守护** — 启用后每 30 秒自动重设代理（gsettings + proxy.env），防止被清除
- **订阅管理** — URL 添加/更新/删除，自动识别格式并注入 proxy-provider
- **代理控制** — 策略组浏览、节点切换、延迟测试、模式切换
- **日志视图** — 实时日志流、级别过滤、暂停滚动、Vim 式选中复制（`v` 选中 + `y` 复制）
- **跨 shell 代理** — 同时配置 GNOME 系统代理（浏览器）和 proxy.env（终端），支持 `~/.zshenv` 自动加载
- **Vim 风格快捷键**，Catppuccin Mocha 配色

## 快速开始

### 1. 安装后确认 mihomo 已启动

```bash
systemctl --user status mihomo
```

### 2. 连接测试

```bash
mioctl connect test
```

输出 `✓ Connected to mihomo v1.19.x` 表示连接成功。

### 3. 启动 TUI

```bash
mioctl tui
```

### 4. 管理订阅

在 `~/.config/mioctl/config.toml` 中配置订阅：

```toml
[subscriptions]
update-interval-minutes = 240

[[subscriptions.items]]
name = "我的机场"
url = "https://example.com/api/v1/client/xxxxxxxx"
```

然后更新：

```bash
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

[preferences]
# 延迟测试地址
delay-test-url = "https://www.gstatic.com/generate_204"
# 延迟测试超时（毫秒）
delay-test-timeout-ms = 5000
# 应用日志等级：off / error / info / debug
app-log-level = "info"
```

## mihomo 配置

mioctl 通过编辑 `~/.config/mihomo/config.yaml` 来切换 TUN 模式。
需在 mihomo 配置中启用外部控制器：

```yaml
external-controller: 127.0.0.1:9090
secret: "" # 可选
tun:
  enable: true
  stack: gvisor # 推荐，SSH / Git 等非 HTTP 协议更稳定
  auto-route: true
  auto-detect-interface: true
  dns-hijack:
    - any:53
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
| `p`                    | 切换代理开关（TUN ↔ 系统代理 ↔ 关闭） |
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
| `v`     | 进入选中模式，`j`/`k` 或 `↑`/`↓` 扩展选区                 |
| `y`     | 复制选中文本到剪贴板                                      |

## 视图说明

### 📊 概览

默认视图。第一行显示代理模式、上下行速率、连接数；第二行显示 TUN 状态、系统代理状态、混合端口、LAN 开放状态；下方为流量趋势图、策略组表格（显示每个组的当前活跃节点）、内存和版本信息。

按 `p` 可一键切换代理状态：**TUN ↔ 系统代理 ↔ 全部关闭**，TUN 和系统代理永远二选一。

### 🔗 代理

左侧策略组列表，右侧节点详情表格。显示节点名称、类型、延迟和选中状态。支持即时切换和延迟测试。国旗 emoji 自动转换为 `[XX]` 格式以确保终端兼容性。

### 🌐 连接

所有活跃连接表格。显示源地址、目标地址、代理链路、匹配规则和流量。支持关闭单个或全部连接。

### 📋 规则

路由规则列表，包含规则类型、匹配条件和目标策略。

### 📜 日志

实时滚动日志流，按日志级别着色（info=绿、warning=黄、error=红、debug=灰）。支持暂停、级别过滤和 Vim 式文本选中复制（`v` 进入选中模式，`j`/`k` 扩展选区，`y` 复制到剪贴板）。

## 系统代理

mioctl 提供 3 层系统代理机制，同时覆盖浏览器和终端：

| 层  | 机制       | 覆盖范围 | 说明                                                                            |
| --- | ---------- | -------- | ------------------------------------------------------------------------------- |
| 1   | gsettings  | 浏览器   | GNOME 桌面系统代理设置                                                          |
| 2   | proxy.env  | 终端     | `~/.config/mioctl/proxy.env`，在 `~/.zshenv` 或 `~/.profile` 中 source 即可生效 |
| 3   | proxy.conf | 检测     | `~/.config/environment.d/proxy.conf`，仅用于 Dashboard 的 SysProxy 状态检测     |

**建议**：在 `~/.zshenv`（zsh）或 `~/.profile`（bash）中添加：

```bash
[ -f ~/.config/mioctl/proxy.env ] && . ~/.config/mioctl/proxy.env
```

启用系统代理后，**代理守护**每 30 秒自动重设 gsettings 和 proxy.env，确保配置不被其他应用覆盖。

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
├── bin/mihomo            # mihomo 二进制（由 install.sh 安装）
├── proxy.env             # 终端代理环境变量
└── providers/            # 生成的 proxy-provider 文件
    └── my-sub.yaml

~/.config/mihomo/
└── config.yaml           # mihomo 配置

~/.config/environment.d/
└── proxy.conf            # 系统代理检测文件（自动管理）

~/.config/systemd/user/
└── mihomo.service        # mihomo systemd 用户服务
```

## 常见问题

**Q: 启动 TUI 后显示 disconnected？**
A: 确认 mihomo 已运行且 `external-controller` 配置正确。`systemctl --user status mihomo` 查看状态，`mioctl connect test` 测试连接。

**Q: 订阅更新失败？**
A: 检查订阅 URL 是否可访问。部分订阅需要特定 User-Agent（mioctl 使用 `clash-verge/1.3.8`），或服务器使用自签名证书（mioctl 已支持）。

**Q: 如何添加新的订阅？**
A: 在 `~/.config/mioctl/config.toml` 的 `[[subscriptions.items]]` 段中添加 name 和 url，然后运行 `mioctl sub update --all`。

**Q: 按 `p` 切换 TUN 没有反应？**
A: 确认 mihomo 配置中包含 `tun` 段，且 mihomo 二进制已设置 `CAP_NET_ADMIN` 权限：`sudo setcap cap_net_admin+ep $(which mihomo)`。

**Q: 为什么终端（curl / Git）不走代理？**
A: 确保已 source proxy.env，在 `~/.zshenv` 中添加：`[ -f ~/.config/mioctl/proxy.env ] && . ~/.config/mioctl/proxy.env`

**Q: 按 `v` 和 `y` 无法复制日志？**
A: 需要安装剪贴板工具：X11 用户装 `xclip`，Wayland 用户装 `wl-clipboard`。

**Q: 为什么很多节点显示 0ms？**
A: 0ms 表示 mihomo 对该节点做过延迟测试但连接失败（超时或不可达）。按 `T` 重新测试正常节点。

## 开发

```bash
cargo test                          # 运行全部测试
cargo test --test integration_test  # 集成测试（wiremock）
cargo clippy -- -D warnings         # lint
cargo build --release               # 编译发布版本
```

## 许可

MIT
