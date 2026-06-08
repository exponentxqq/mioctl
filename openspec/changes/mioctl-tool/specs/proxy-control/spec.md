## ADDED Requirements

### Requirement: 策略组管理
系统 SHALL 支持查看和操作 mihomo 策略组（proxy-groups）。

#### Scenario: 列出所有策略组
- **WHEN** 用户请求查看策略组
- **THEN** 展示所有策略组，包含名称、类型（select/url-test/fallback/load-balance）、当前选中节点

#### Scenario: 查看策略组详情
- **WHEN** 用户选择特定策略组
- **THEN** 展示该策略组的详细信息和所有可用节点

### Requirement: 节点切换
系统 SHALL 支持在全局模式和 Rule 模式下切换策略组选中的代理节点。

#### Scenario: 切换策略组节点
- **WHEN** 用户选择策略组中的目标节点并确认
- **THEN** 系统调用 mihomo API 切换该策略组到指定节点，并反馈结果

#### Scenario: 切换到直连
- **WHEN** 用户选择 DIRECT 节点
- **THEN** 策略组切换为直连模式

### Requirement: 延迟测试
系统 SHALL 支持对所有代理节点或指定策略组节点进行延迟测试。

#### Scenario: 测试单个节点延迟
- **WHEN** 用户对代理视图中的节点触发延迟测试
- **THEN** 显示该节点的实时延迟（毫秒），超时显示 TIMEOUT

#### Scenario: 测试策略组全部节点延迟
- **WHEN** 用户对策略组触发全部延迟测试
- **THEN** 系统并发测试该组所有节点延迟，更新显示所有结果

#### Scenario: 延迟测试使用自定义 URL
- **WHEN** 用户在配置中设置了自定义延迟测试 URL
- **THEN** 延迟测试使用该自定义 URL 而非默认 URL

### Requirement: 代理模式切换
系统 SHALL 支持切换 mihomo 代理模式（Global / Rule / Direct）。

#### Scenario: 切换代理模式
- **WHEN** 用户选择目标代理模式
- **THEN** 系统通过 GLOBAL 策略组切换模式，仪表盘显示当前模式

### Requirement: 节点健康检查
系统 SHALL 支持触发 proxy-provider 的健康检查。

#### Scenario: 触发健康检查
- **WHEN** 用户触发 provider 健康检查
- **THEN** 系统调用 mihomo API 执行健康检查，标记不可用节点
