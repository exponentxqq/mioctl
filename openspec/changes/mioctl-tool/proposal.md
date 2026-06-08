## Why

现有 mihomo（Clash.Meta）生态缺乏一个好用的终端管理工具。用户通常需要手动编辑 YAML 配置文件、通过 curl 调用 RESTful API、或依赖 GUI 客户端来管理代理。这导致在无 GUI 的服务器环境或偏好终端工作流的场景下，管理 mihomo 代理节点、订阅和规则变更效率低下且容易出错。

## What Changes

- 新建 `mioctl` CLI 工具，提供完整的终端用户界面（TUI）来管理 mihomo 实例
- 实现对 mihomo RESTful API 的全面对接，涵盖：连接信息、节点切换、规则查询、流量统计、日志查看、DNS 查询等
- 支持通过 URL 拉取、更新、新增代理订阅源，自动解析为 mihomo 配置格式
- 提供实时状态仪表盘，展示当前连接、流量、延迟等信息
- 支持配置文件管理：加载、编辑、校验、重载 mihomo 配置

## Capabilities

### New Capabilities

- `tui-dashboard`: 交互式终端仪表盘，实时展示 mihomo 代理状态、连接信息、流量统计和节点延迟
- `mihomo-api-client`: 完整的 mihomo RESTful API 客户端封装，覆盖所有公开 API 端点（proxies、rules、connections、logs、traffic、DNS、providers 等）
- `subscription-manager`: 订阅源管理，支持通过 URL 添加/更新/删除订阅，自动解析多种订阅格式并转换为 mihomo 配置
- `proxy-control`: 代理节点和策略组管理，包括节点切换、延迟测试、健康检查、按策略组浏览
- `config-manager`: mihomo 配置文件管理，包括加载、校验、热重载、差异对比

### Modified Capabilities

（无，全新项目）

## Impact

- **语言/运行时**: Rust（提供单二进制分发、高性能、优秀的 TUI 生态）
- **核心依赖**: ratatui（TUI 框架）、reqwest（HTTP 客户端）、tokio（异步运行时）、serde（序列化）
- **外部系统**: 需要运行中的 mihomo 实例并开启 RESTful API（external-controller）
- **分发**: 通过 cargo 安装，或提供预编译二进制文件（Linux/macOS/Windows）
- **配置**: mioctl 自身配置文件（~/.config/mioctl/config.toml），存储 mihomo API 地址和认证信息
