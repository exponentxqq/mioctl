## ADDED Requirements

### Requirement: 订阅源管理
系统 SHALL 支持添加、删除、列出、更新代理订阅源（URL 形式）。

#### Scenario: 添加订阅源
- **WHEN** 用户执行添加订阅命令并传入 URL 和可选名称
- **THEN** 订阅源被保存到 mioctl 配置中，并立即尝试拉取和解析

#### Scenario: 删除订阅源
- **WHEN** 用户执行删除订阅命令
- **THEN** 指定订阅源从配置中移除，确认操作不可逆

#### Scenario: 列出所有订阅源
- **WHEN** 用户执行列出订阅命令
- **THEN** 展示所有已保存的订阅源，包含名称、URL、最后更新时间、节点数量

### Requirement: 订阅内容拉取
系统 SHALL 通过 HTTP(S) 拉取订阅源内容，支持 User-Agent 自定义和超时配置。

#### Scenario: 成功拉取订阅
- **WHEN** 用户执行更新订阅命令
- **THEN** 系统向订阅 URL 发起 GET 请求，获取订阅内容

#### Scenario: 拉取超时处理
- **WHEN** 订阅 URL 在 30 秒内无响应
- **THEN** 系统报告超时错误，提示用户检查网络或 URL

#### Scenario: HTTP 错误处理
- **WHEN** 订阅 URL 返回非 2xx 状态码
- **THEN** 系统报告 HTTP 错误码，不将错误内容解析为节点列表

### Requirement: 多格式解析
系统 SHALL 自动检测并解析多种订阅格式，统一转换为内部节点表示。

#### Scenario: 解析 Clash YAML 格式
- **WHEN** 订阅内容为有效的 Clash 配置文件（包含 `proxies` 字段）
- **THEN** 系统提取所有代理节点信息

#### Scenario: 解析 Base64 编码格式
- **WHEN** 订阅内容为 Base64 编码的节点列表
- **THEN** 系统先解码 Base64，再逐行解析节点 URI（ss://, vmess://, trojan:// 等）

#### Scenario: 解析原始 URI 格式
- **WHEN** 订阅内容为纯文本的节点 URI 列表（每行一个）
- **THEN** 系统逐行解析节点 URI

### Requirement: 订阅更新
系统 SHALL 支持手动触发订阅更新和自动定时更新。

#### Scenario: 手动更新单个订阅
- **WHEN** 用户执行更新命令指定订阅名称
- **THEN** 系统拉取该订阅最新内容，解析节点，更新 mihomo 配置

#### Scenario: 手动更新所有订阅
- **WHEN** 用户执行更新全部订阅命令
- **THEN** 系统依次拉取所有订阅，汇总更新的节点数量

#### Scenario: 自动定时更新
- **WHEN** 用户在配置中设置自动更新间隔（如每隔 4 小时）
- **THEN** 系统在 TUI 运行期间按间隔自动刷新订阅

### Requirement: 订阅内容注入 mihomo
系统 SHALL 将解析后的节点信息通过 proxy-providers API 注入 mihomo。

#### Scenario: 通过 provider 注入节点
- **WHEN** 订阅解析完成
- **THEN** 系统生成 proxy-provider 配置并通过 mihomo API 更新 provider

### Requirement: 订阅持久化
系统 SHALL 将订阅源列表和元数据持久化到 mioctl 配置文件。

#### Scenario: 订阅配置持久化
- **WHEN** 用户添加或修改订阅
- **THEN** 订阅元数据（名称、URL、最后更新时间）保存到 `~/.config/mioctl/config.toml`
