# mioctl 多订阅 Profile 管理 — Design Spec

Date: 2026-08-21

## Problem

现状 mioctl 只支持单一订阅生效：

- `SubscriptionManager::register` 是**替换式**合并——注册第二个订阅会覆盖主 config 的
  `proxies`/`proxy-groups`/`rules` 三段，丢掉前一个订阅的全部节点
- `update_one` 走 provider 文件路径（写 `providers/*.yaml` + 调 provider API），但主 config
  从未声明 `proxy-providers` 段引用这些文件，该路径实际不生效
- CLI 没有 `list` / `use` / `remove`；TUI 没有订阅管理视图
- `update_interval_minutes` 是死配置（无任何定时任务使用）

目标：支持同时保存多个订阅（profile），同一时刻激活一个；可在订阅间切换、更新、删除，
并在 TUI 中完成全部操作。

## 核心决策：单 profile 激活模式（参考 clash-verge-rev）

订阅保存为独立 profile 存档文件，同一时刻只有一个**激活**。切换订阅 = 换激活 profile →
重新合并写入 mihomo config → reload。节点切换仍在现有 TUI proxies 视图完成（激活订阅内）。

**关键原则：profile 存档保存订阅的标准化 YAML；激活时 `proxies`/`proxy-groups`/`rules`
三段整段透传**，保证 tuic/anytls/mieru 等任意节点类型不丢失（节点级重解析只支持
ss/vmess/trojan，会丢掉其他协议）。

## 存储模型

```
~/.config/mioctl/
├── config.toml           # [subscriptions] 增加 active = "<profile 名>"
└── profiles/
    └── 狗狗加速.com.yaml  # 订阅标准化 YAML 存档
```

### config.toml 变更

```toml
[subscriptions]
active = "狗狗加速.com"        # Option<String>，当前激活的 profile 名

[[subscriptions.items]]
name = "狗狗加速.com"
url = "https://..."
node_count = 68               # Option<usize>，add/update 时统计
last_updated = "..."          # 已有字段
```

删除字段：`update_interval_minutes`（死配置，连同 settings 视图展示与相关测试一并移除；
mioctl 非常驻进程，不提供自动更新）。

### 存档文件名规则（A1）

文件名 = 订阅名，仅做轻度 sanitize：`/` 与控制字符替换为 `_`。中文/emoji/空格保留。
Linux 文件名天然支持。文件与订阅名直观对应，删除订阅即删除对应文件。

### 重名策略（A2）

- **自动检测的名称**撞车 → 自动追加序号后缀（`狗狗加速 (2)`、`狗狗加速 (3)`…），
  输出中明确提示最终名称
- **显式 `--name` 指定的名称**撞车 → 直接报错（显式意图被违背应失败而非静默改名）

### 存档格式（A3）

所有订阅统一转为节点 YAML 存档，激活路径单一：

- **YAML 订阅**：原样存档（已含三段结构）
- **Base64/URI 订阅**：解析为节点后生成标准 YAML 存档（select group 以订阅名命名 +
  `MATCH,<订阅名>` 规则），复用现有 `nodes_to_subscription_content` 逻辑

Parser 增强（不再静默丢节点）：

1. Base64 解码后若内容实为 YAML（含 `proxies:`）→ 按 YAML 处理
2. URI 列表中无法识别的协议条目（vless/hysteria2/ssr 等）→ 计数并在结果中显式警告
   （如 `警告: 3 个节点无法导入（不支持的协议）: vless://..., ...`）

## 激活语义与主 config 归属

### 三段全托（B1）

主 config.yaml 的 `proxies`/`proxy-groups`/`rules` 三段完全归 mioctl 管理，
是激活订阅的镜像。切换/更新激活订阅 = 三段整段替换；用户手动修改三段会被覆盖
（文档写明）。基础设施段（PRESERVE_KEYS：dns/tun/port/mode 等）永不触碰。

### 无订阅态（B2）

删除激活订阅后（无论是否还有其他订阅），主 config 三段写为：

```yaml
proxies: []
proxy-groups: []
rules:
  - MATCH,DIRECT
```

空三段 + `MATCH,DIRECT` 兜底，mihomo 加载合法，流量明确全直连。

### 选择记忆与模式（B3）

`profile.store-selected` 由 mihomo 自行处理（同名 group 继承选择、失效重置），
mioctl 不干预。`mode` 属于 PRESERVE_KEYS，切换订阅不重置。

## 核心流程（SubscriptionManager 重构）

| 操作 | 流程 |
|---|---|
| **add** | fetch（UA 探测，沿用现有）→ 检测格式 → 转标准 YAML → 名称检测/去重（A2）→ 存档 `profiles/{name}.yaml` → 写 config.toml（含 node_count）→ 若无激活订阅则自动激活；已有激活则不切换（`--activate` 立即切换） |
| **use** | 校验存档存在 → 读存档 → 提取三段 → `backup_file` → 合并写入主 config → reload → 更新 `active` 标记 + config.toml |
| **update** | fetch → 转标准 YAML → 覆盖存档 + 更新 node_count/last_updated → **仅当是激活项**才重新合并主 config + reload；非激活项只刷存档 |
| **remove** | 确认（CLI 交互确认，`--yes` 跳过；TUI 弹窗确认）→ 删存档文件 + config.toml 条目 → 若删的是激活项：清空 `active`，主 config 写无订阅态（B2）+ reload |

### 错误处理

- fetch 失败：add/update 报错退出，不动任何文件
- 合并/写主 config 失败：`rollback_file` 恢复 `.bak`
- reload 失败：三级回退（API reload → `systemctl --user restart mihomo` → 提示手动命令），
  文件系统为事实源，reload 失败不回滚文件
- 存档损坏（YAML 解析失败）：use 报错并提示先 `sub update <name>` 重新拉取

### 存量迁移（E3）

新增 `ensure_archived()`：各操作入口（list/add/use/update/remove/TUI 启动）检测
config.toml 有 items 但 `profiles/` 缺对应存档 → 按现有 URL fetch 一次生成存档。
fetch 失败则该 profile 标记"无存档"，禁止 use（提示先 update），不影响其他操作。

## CLI 命令

```
mioctl sub list                      # 表格：名称/节点数/最后更新/激活标记(*)
mioctl sub add <url> [--name N] [--no-reload] [--activate]
mioctl sub use <name>                # 切换激活
mioctl sub update [name] [--all]     # 无参=更新激活项；--all=全部；指定 name=单个
mioctl sub remove <name> [--yes]     # 删除订阅
mioctl sub register <url> ...        # 保留为 add 的纯别名（新语义）
```

- `update` 无参且无激活订阅 → 报错提示指定 name 或 --all
- `update --all` 保持现有行为，逐项更新并汇总结果

## TUI 订阅视图（D1–D3）

### 视图与入口

- sidebar 新增「订阅」页（数字键 6 / `ActiveView::Subscriptions`）
- 现有 sidebar「Update Subs」项（第 7 项）移除——被订阅视图吸收
- 列表项：名称、节点数、最后更新时间、激活标记 `*`

### 按键（新增 Action 变体 + 按视图分派）

| 键 | 行为（订阅视图内） |
|---|---|
| `Enter` | 切换激活（use） |
| `u` | 更新选中订阅 |
| `a` | 添加订阅：底部单步 URL 输入框（复用 search_mode 模式），名称自动检测 + 自动后缀去重 |
| `d` | 删除选中（确认弹窗，复用现有 overlay 机制；`y` 确认 / `Esc` 取消） |
| `j/k` | 上下移动（复用现有） |

`Enter`/`d` 与其他视图的现有绑定（SwitchNode/CloseConnection）冲突，在 `handle_action`
按 `ActiveView::Subscriptions` 分派。`u`/`a` 为新增 Action 变体（`u` 现无绑定；`a` 现无绑定）。

### 异步模式

操作后台 `tokio::spawn` 执行（设置 `LoadingKind`），成功后 log 提示 + 更新
config 到 state + `refresh_state()`；切换/删除激活项后自动刷新 proxies 视图数据。
失败写 error log，loading 复位。

## 代码清理（E1/E2）

| 删除项 | 理由 |
|---|---|
| `src/subscription/injector.rs` 整个文件 | provider 文件路径不生效，被存档路径取代 |
| `MihomoClient::{update_proxy_provider, get_proxy_providers, healthcheck_proxy_provider}` | 仅被 injector 路径引用 |
| `MioctlConfig::providers_dir()` + `save()` 中的目录创建 | 同上 |
| `update_interval_minutes` 字段 + settings 视图展示 + 相关测试 | 死配置（C2） |
| `~/.config/mioctl/providers/` 目录（运行时） | 不再使用 |

`register` 保留为 `add` 的纯别名，接受新语义（无旧格式兼容负担——config.toml 的 items
结构不变，仅新增字段，serde 默认值兼容旧文件）。

## 测试

- **单元**：
  - 文件名 sanitize（`/`、控制字符、中文名）
  - 重名去重（自动后缀递增、显式报错）
  - 存档读写 roundtrip；Base64 内嵌 YAML 识别；未知 URI 协议警告
  - 激活合并：tuic 节点样例断言三段透传完整（节点字段无损）
  - remove 激活项 → 无订阅态三段
  - config.toml `active`/`node_count` 字段 roundtrip（旧文件无新字段可加载）
  - `ensure_archived()` 迁移逻辑
- **集成**（wiremock）：add → use → update → remove 全链路；多订阅并存切换；
  无订阅态写入

## 文档

CLAUDE.md 的 subscription 模块描述与 Commands 部分同步更新（profile 存档模式、
新 CLI 命令、三段全托约定）。
