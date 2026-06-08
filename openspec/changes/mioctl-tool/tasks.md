## 1. 项目初始化

- [ ] 1.1 使用 `cargo init` 创建 Rust 项目，配置 Cargo.toml 依赖（ratatui, reqwest, tokio, serde, serde_json, serde_yaml, toml, clap, tracing, thiserror）
- [ ] 1.2 建立目录结构：`src/{api, app, cli, config, ui, subscription}` 各模块
- [ ] 1.3 配置 tracing 日志，输出到文件 + TUI 日志视图

## 2. Mihomo API Client（`src/api/`）

- [ ] 2.1 实现 `MihomoClient` 结构体，封装 reqwest client、base URL、secret 鉴权
- [ ] 2.2 实现 `GET /proxies`、`GET /proxies/:name`、`PUT /proxies/:name` 端点封装
- [ ] 2.3 实现 `GET /group`、`GET /group/:name`、`DELETE /group/:name`、`GET /group/:name/delay` 端点封装
- [ ] 2.4 实现 `GET /rules` 端点封装
- [ ] 2.5 实现 `GET /connections`、`DELETE /connections`、`DELETE /connections/:id` 端点封装
- [ ] 2.6 实现 `GET /providers/proxies`、`GET/PUT /providers/proxies/:name`、`GET /providers/proxies/:name/healthcheck` 端点封装
- [ ] 2.7 实现 `GET /configs`、`PUT /configs`、`PATCH /configs`、`POST /restart` 端点封装
- [ ] 2.8 实现 `GET /traffic`、WebSocket `/traffic` 实时流量流
- [ ] 2.9 实现 WebSocket `/logs` 实时日志流（支持 level 过滤）
- [ ] 2.10 实现 `GET /memory`、`GET /version`、`GET /dns/query` 端点封装
- [ ] 2.11 实现 API 响应类型定义（TypeScript 风格的 Rust 强类型 struct，覆盖所有端点返回值）
- [ ] 2.12 实现错误类型 `ApiError`（网络错误、鉴权失败、超时、反序列化失败）

## 3. 配置管理（`src/config/`）

- [ ] 3.1 定义 `MioctlConfig` 结构体（mihomo API 地址、secret、订阅列表、UI 偏好）及序列化/反序列化
- [ ] 3.2 实现配置文件加载：启动时读取 `~/.config/mioctl/config.toml`，不存在则创建默认配置
- [ ] 3.3 实现运行时配置修改和持久化
- [ ] 3.4 实现 `connect-test` 子命令：向 `/version` 发请求验证连接

## 4. 代理控制（`src/app/` - 业务逻辑层）

- [ ] 4.1 实现 `ProxyManager`：缓存节点列表，提供按策略组查询、节点查找接口
- [ ] 4.2 实现节点切换逻辑：调 API 切换、错误处理、状态更新
- [ ] 4.3 实现延迟测试逻辑：并发测速、超时处理、结果聚合
- [ ] 4.4 实现策略组管理：列出、选中、清除固定选择
- [ ] 4.5 实现代理模式切换：通过 GLOBAL 策略组在 Global/Rule/Direct 间切换
- [ ] 4.6 实现连接管理：列出、关闭单个、关闭全部

## 5. 订阅管理（`src/subscription/`）

- [ ] 5.1 定义 `Subscription` 和 `SubscriptionStore` 结构体，管理订阅源列表
- [ ] 5.2 实现 HTTP 拉取：GET 请求订阅 URL，User-Agent 可配置，30 秒超时
- [ ] 5.3 实现格式解析器 trait + 多格式支持：YAML（Serde 解析 proxies 字段）、Base64（解码后逐行解析 URI）、纯文本 URI 列表
- [ ] 5.4 实现 URI 节点解析（ss://, vmess://, trojan://, vless://, hysteria2:// 等协议）
- [ ] 5.5 实现订阅内容注入：通过 proxy-provider API 写入 mihomo
- [ ] 5.6 实现 CLI 命令：`mioctl sub add <url> [name]`、`mioctl sub remove <name>`、`mioctl sub list`、`mioctl sub update [name]`
- [ ] 5.7 实现自动定时更新（TUI 运行期间按配置间隔触发）

## 6. TUI 仪表盘（`src/ui/`）

- [ ] 6.1 实现 ratatui 应用框架：事件循环、输入处理、多视图路由
- [ ] 6.2 实现 **概览仪表盘视图**：代理模式指示器、流量速率图表（Sparkline）、连接数、内存使用
- [ ] 6.3 实现 **代理节点视图**：策略组侧边栏 + 节点详情表格（名称、类型、延迟、选中标记）
- [ ] 6.4 实现 **连接视图**：连接表格（源地址、目标、代理、规则、流量）+ 关闭连接快捷键
- [ ] 6.5 实现 **规则视图**：规则可滚动列表
- [ ] 6.6 实现 **日志视图**：实时滚动日志流 + 级别过滤 + 暂停/恢复
- [ ] 6.7 实现底部状态栏：API 连接状态指示灯、最后更新时间
- [ ] 6.8 实现键盘快捷键：Tab 切视图、1-5 数字键快速跳转、q 退出、r 刷新
- [ ] 6.9 实现节点搜索/过滤输入框
- [ ] 6.10 实现延迟测试触发器（选中节点按 t 测速）

## 7. CLI 命令（`src/cli/`）

- [ ] 7.1 使用 clap 定义 CLI 结构：`mioctl` 主命令 + `tui` / `proxy` / `sub` / `config` / `connect` 子命令
- [ ] 7.2 实现 `mioctl tui`：启动 TUI 模式
- [ ] 7.3 实现 `mioctl proxy list`、`mioctl proxy set <group> <node>`、`mioctl proxy test [group]`
- [ ] 7.4 实现 `mioctl config show`、`mioctl config validate <path>`、`mioctl config reload`
- [ ] 7.5 实现 `mioctl connect test`：连接验证
- [ ] 7.6 实现 `mioctl --help` 完整帮助信息

## 8. 测试与完善

- [ ] 8.1 为 API Client 层编写单元测试（mock HTTP server）
- [ ] 8.2 为订阅解析器编写单元测试（各格式样本文件）
- [ ] 8.3 编写集成测试（启动 mihomo 实例进行端到端验证）
- [ ] 8.4 编写 README：安装说明、配置指南、使用示例截图
- [ ] 8.5 配置 CI（GitHub Actions）：构建、测试、clippy lint、fmt 检查
