## ADDED Requirements

### Requirement: 实时状态仪表盘
系统 SHALL 提供一个交互式终端仪表盘，实时展示 mihomo 运行状态，包括：当前代理模式、上下行流量速率、活跃连接数、内存使用量。

#### Scenario: 仪表盘展示基础状态
- **WHEN** 用户启动 mioctl TUI 模式
- **THEN** 仪表盘展示当前代理模式（Global/Rule/Direct）、实时上传/下载流量（kbps）、活跃连接数、内存使用量

#### Scenario: 流量数据实时更新
- **WHEN** 仪表盘正在运行
- **THEN** 流量数据每 1 秒通过 WebSocket 自动刷新

### Requirement: 代理节点视图
系统 SHALL 提供代理节点列表视图，按策略组分组显示所有代理节点及其状态信息。

#### Scenario: 按策略组浏览节点
- **WHEN** 用户在代理视图中选择策略组
- **THEN** 展示该策略组下所有代理节点，包含：节点名称、类型、当前延迟、是否选中

#### Scenario: 切换代理节点
- **WHEN** 用户在代理视图中选择节点并按确认键
- **THEN** 系统通过 API 将当前策略组切换到选定节点，并反馈切换结果

### Requirement: 连接视图
系统 SHALL 提供活跃连接列表视图，展示所有当前网络连接及其元数据。

#### Scenario: 查看活跃连接
- **WHEN** 用户切换到连接视图
- **THEN** 展示所有活跃连接，包含：源地址、目标地址、代理名称、规则匹配、上传/下载流量

#### Scenario: 关闭指定连接
- **WHEN** 用户在连接视图中选择连接并按删除键
- **THEN** 系统通过 API 关闭该连接，并从列表中移除

### Requirement: 规则视图
系统 SHALL 提供规则列表视图，展示所有路由规则及其匹配统计。

#### Scenario: 查看规则列表
- **WHEN** 用户切换到规则视图
- **THEN** 展示所有规则，包含：规则类型、匹配条件、目标策略、匹配次数（如有）

### Requirement: 日志视图
系统 SHALL 提供实时日志流视图，展示 mihomo 运行日志。

#### Scenario: 实时日志流
- **WHEN** 用户切换到日志视图
- **THEN** 通过 WebSocket 实时展示 mihomo 日志，支持按日志级别过滤（info/warning/error/debug）

#### Scenario: 暂停/恢复日志流
- **WHEN** 用户在日志视图中按暂停键
- **THEN** 日志展示暂停滚动，再次按下恢复实时滚动

### Requirement: 多视图切换
系统 SHALL 支持通过键盘快捷键在仪表盘不同视图之间切换。

#### Scenario: 键盘切换视图
- **WHEN** 用户按下 Tab 或数字键（1-5）
- **THEN** TUI 切换到对应视图（1=仪表盘, 2=代理, 3=连接, 4=规则, 5=日志）

### Requirement: 状态栏
系统 SHALL 在 TUI 底部显示状态栏，包含 mihomo API 连接状态和更新时间。

#### Scenario: 状态栏显示连接状态
- **WHEN** TUI 正在运行
- **THEN** 状态栏持续显示 API 连接状态（已连接/已断开）、数据更新时间
