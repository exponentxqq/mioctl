## ADDED Requirements

### Requirement: 完整 HTTP API 封装
系统 SHALL 封装 mihomo 所有 RESTful API 端点，提供类型安全的 Rust 接口。

#### Scenario: 请求代理节点列表
- **WHEN** 调用 `get_proxies()` 方法
- **THEN** 返回所有代理节点信息，包括名称、类型、延迟历史、所属策略组

#### Scenario: 请求规则列表
- **WHEN** 调用 `get_rules()` 方法
- **THEN** 返回所有路由规则信息

#### Scenario: 请求连接列表
- **WHEN** 调用 `get_connections()` 方法
- **THEN** 返回所有活跃连接信息，支持可选的时间间隔参数

#### Scenario: 关闭指定连接
- **WHEN** 调用 `close_connection(id)` 方法
- **THEN** 向 API 发送 DELETE 请求关闭指定 ID 的连接

#### Scenario: 关闭所有连接
- **WHEN** 调用 `close_all_connections()` 方法
- **THEN** 向 API 发送 DELETE 请求关闭所有活跃连接

### Requirement: WebSocket 实时数据流
系统 SHALL 支持通过 WebSocket 连接接收实时数据流（日志、流量、内存、连接）。

#### Scenario: 订阅流量 WebSocket
- **WHEN** 调用 `subscribe_traffic()` 方法
- **THEN** 建立 WebSocket 连接并持续接收流量数据，通过异步流（Stream）返回

#### Scenario: 订阅日志 WebSocket
- **WHEN** 调用 `subscribe_logs(level)` 方法
- **THEN** 建立 WebSocket 连接并持续接收日志数据，支持按级别过滤

#### Scenario: WebSocket 自动重连
- **WHEN** WebSocket 连接意外断开
- **THEN** 系统自动重连（最多 5 次，指数退避），重连成功后恢复数据流

### Requirement: 配置管理 API
系统 SHALL 封装 mihomo 配置管理相关端点。

#### Scenario: 获取当前配置
- **WHEN** 调用 `get_configs()` 方法
- **THEN** 返回 mihomo 当前基础配置

#### Scenario: 重载配置
- **WHEN** 调用 `reload_config(path)` 方法
- **THEN** 向 API 发送 PUT 请求，触发 mihomo 配置重载

### Requirement: 延迟测试
系统 SHALL 封装代理节点延迟测试端点。

#### Scenario: 测试单个节点延迟
- **WHEN** 调用 `test_proxy_delay(name, url, timeout)` 方法
- **THEN** 返回该节点的延迟（毫秒），超时则返回 -1

#### Scenario: 测试策略组所有节点延迟
- **WHEN** 调用 `test_group_delay(group_name, url, timeout)` 方法
- **THEN** 返回该策略组下所有节点的延迟信息

### Requirement: DNS 查询
系统 SHALL 封装 DNS 查询端点。

#### Scenario: DNS 查询
- **WHEN** 调用 `dns_query(name, record_type)` 方法
- **THEN** 返回指定域名和记录类型的 DNS 解析结果

### Requirement: API 鉴权与连接管理
系统 SHALL 通过 mihomo 配置的 secret 进行 API 鉴权，支持 TCP 和 Unix socket 两种连接方式。

#### Scenario: Bearer Token 鉴权
- **WHEN** 初始化客户端时提供 API secret
- **THEN** 所有 HTTP 请求自动附加 `Authorization: Bearer ${secret}` 头部

#### Scenario: Unix socket 连接
- **WHEN** 初始化客户端时指定 Unix socket 路径
- **THEN** 客户端通过 Unix socket 与 mihomo 通信

### Requirement: 类型安全的反序列化
系统 SHALL 为所有 API 响应提供强类型的 Rust 结构体，正确反序列化 JSON 数据。

#### Scenario: 反序列化代理节点数据
- **WHEN** 收到 mihomo proxies API 响应
- **THEN** 数据被正确反序列化为 `Proxy` 结构体，包含所有字段（name, type, now, history 等）
