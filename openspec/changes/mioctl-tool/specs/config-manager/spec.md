## ADDED Requirements

### Requirement: mioctl 自身配置管理
系统 SHALL 管理 mioctl 自身的配置文件（TOML 格式），存储 mihomo 连接信息和用户偏好。

#### Scenario: 初始化配置
- **WHEN** mioctl 首次运行且配置文件不存在
- **THEN** 系统在 `~/.config/mioctl/config.toml` 创建默认配置文件

#### Scenario: 配置验证
- **WHEN** mioctl 启动并加载配置
- **THEN** 系统验证配置字段，对无效值使用默认值并警告

#### Scenario: 运行时修改配置
- **WHEN** 用户在 TUI 设置面板中修改配置
- **THEN** 更改立即写入配置文件并生效

### Requirement: mihomo 配置查看与校验
系统 SHALL 支持查看 mihomo 当前配置，并对配置文件进行基本校验。

#### Scenario: 查看当前配置
- **WHEN** 用户执行查看配置命令
- **THEN** 系统通过 API 获取并展示 mihomo 当前配置（格式化显示）

#### Scenario: 配置校验
- **WHEN** 用户执行配置校验命令并传入配置文件路径
- **THEN** 系统解析 YAML 文件，检查必需字段和格式，报告错误和警告

### Requirement: 配置热重载
系统 SHALL 支持通过 API 触发 mihomo 配置热重载。

#### Scenario: 重载配置
- **WHEN** 用户执行重载配置命令
- **THEN** 系统调用 mihomo API 触发配置重载，反馈重载成功或失败

#### Scenario: 重载指定配置文件
- **WHEN** 用户执行重载配置命令并传入配置路径
- **THEN** 系统调用 mihomo API 使用指定路径重载配置

### Requirement: 外部控制连接配置
系统 SHALL 支持配置 mihomo 外部控制器的连接参数。

#### Scenario: 配置 API 连接参数
- **WHEN** 用户在配置中设置 external-controller 地址和 secret
- **THEN** mioctl 使用该配置连接 mihomo API

#### Scenario: 连接测试
- **WHEN** 用户执行连接测试命令
- **THEN** 系统向 mihomo API 发送 `/version` 请求，报告连接状态和 mihomo 版本
