## Context

mioctl 是一个终端管理工具，运行在 mihomo 实例外部，通过其 RESTful API（external-controller）进行通信。目标用户是偏好终端工作流的开发者、系统管理员，以及运行 mihomo 的无 GUI 服务器环境。

mihomo 通过 `external-controller` 暴露 RESTful API，包含 proxies、rules、connections、providers、DNS、logs、traffic、configs 等端点，部分端点支持 WebSocket 实时数据推送。mioctl 需要完整封装这些端点并构建易用的 TUI 操控界面。

## Goals / Non-Goals

**Goals:**
- 完整封装 mihomo RESTful API，所有公开端点均可通过 CLI 或 TUI 访问
- 提供基于 ratatui 的交互式仪表盘，支持多视图切换（概览、代理、连接、规则、日志）
- 订阅源管理：通过 URL 添加、更新、删除订阅；自动解析 Clash YAML、Base64 编码等格式
- 代理节点管理：策略组浏览、节点切换、延迟测试、健康检查
- 配置文件管理：加载、校验、重载

**Non-Goals:**
- 不实现 mihomo 内核功能（代理协议实现、DNS 解析等），仅作为管理工具
- 不实现跨机器的远程管理（首个版本仅支持本地或同机器 mihomo 实例）
- 不作为系统服务运行（用户主动启动的 CLI 工具）
- 不提供 GUI（Web/桌面应用）

## Decisions

### 语言与框架选型：Rust + ratatui

**选择**: Rust 语言，ratatui 作为 TUI 框架，reqwest 作为 HTTP 客户端，tokio 作为异步运行时。

**理由**:
- ratatui 是 Rust 生态最成熟的 TUI 框架，提供丰富的组件（表格、列表、图表、分页等），支持复杂布局和键盘输入
- Rust 提供单二进制分发，无运行时依赖，适合 CLI 工具场景
- 相比 Go（bubbletea）、Python（textual），Rust 性能更高、二进制更小、无 GC 延迟
- reqwest + tokio 生态成熟，WebSocket 支持完善

**考虑的替代方案**:
- Go + bubbletea：开发速度快但二进制较大，TUI 生态不如 ratatui 丰富
- Python + textual：依赖 Python 运行时，分发不便

### 架构：分层模块化

**选择**: 采用 3 层架构——API Client 层 / 业务逻辑层 / UI 层

```
┌─────────────────────────────────────┐
│  UI 层 (ratatui TUI + CLI 命令)      │
├─────────────────────────────────────┤
│  业务逻辑层                           │
│  ┌──────────┬──────────┬──────────┐│
│  │ ProxyMgr │ SubMgr   │ ConfigMgr││
│  └──────────┴──────────┴──────────┘│
├─────────────────────────────────────┤
│  API Client 层 (reqwest + WebSocket) │
│  ┌─────────────────────────────────┐│
│  │ MihomoClient                    ││
│  │ - REST (JSON)                   ││
│  │ - WebSocket (实时数据流)         ││
│  └─────────────────────────────────┘│
└─────────────────────────────────────┘
```

**理由**: 清晰的关注点分离，各层可独立测试。API Client 层变化频率最低（跟随 mihomo API 变更），UI 层变化频率最高（交互优化）。

### 数据流：事件驱动 + 轮询混合

**选择**: WebSocket 端点（logs、traffic、connections、memory）使用事件流推送更新；其他端点（proxies、rules、providers）使用定时轮询（默认 3 秒间隔）。

**理由**:
- WebSocket 推送实时性高且开销低，适合高频更新的数据（日志、流量）
- 轮询实现简单，适合更新频率低的数据（节点列表、规则列表）
- 避免为所有数据建立 WebSocket 连接导致连接数膨胀

### 订阅解析：多格式支持

**选择**: 内置解析器，支持以下格式：
- Clash YAML 配置（proxy-providers 格式）
- Base64 编码的节点列表（常见于机场订阅）
- URI 格式节点列表（ss://, vmess://, trojan:// 等）

**理由**: 这些是代理订阅最常见的格式。不依赖外部解析工具，减少依赖。

### 配置存储：TOML 文件

**选择**: mioctl 自身配置使用 TOML 格式，存储在 `~/.config/mioctl/config.toml`。mihomo 配置继续使用其原生 YAML 格式，mioctl 通过 API 管理。

**理由**: TOML 比 YAML 更简洁、类型更安全，是 Rust 生态的配置标准（cargo 默认格式）。

## Risks / Trade-offs

- **[Risk] mihomo API 变更导致不兼容** → 对每个 mihomo 版本标注兼容性，CI 中接入 API 版本检测
- **[Risk] WebSocket 连接断开后数据丢失** → 实现自动重连 + 断线期间的数据补全
- **[Risk] 大订阅源解析耗时长** → 异步解析 + 进度提示，不阻塞 UI
- **[Trade-off] ratatui 学习曲线** → ratatui 基于 immediate mode，与 React/Vue 的声明式模式差异大，开发初期需要适应。但这个成本是值得的：性能优异，布局控制力强。
- **[Trade-off] 单连接假设** → 首个版本仅支持连接一个 mihomo 实例。未来可通过 profile 切换支持多实例。

## Open Questions

- 是否需要支持 mihomo 的 Unix socket 连接方式？（当前设计先支持 TCP/HTTP）
- 延迟测试 URL 是否需要可配置？（初步使用 `https://www.gstatic.com/generate_204`）
