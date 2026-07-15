# Mioctl: mihomo 1.19.x 兼容性改进

## 背景

mihomo 升级到 v1.19.28 后出现三个问题：
1. 更新后二进制文件丢失 `CAP_NET_ADMIN`，TUN 模式失效
2. `geosite.dat` 缺失，GEOSITE 规则初始化失败
3. `merger.rs` 的默认模板与 `install.sh` 不一致（`redir-host` vs `fake-ip`），缺少 `fake-ip-filter`

此外还存在 `relay` 代理组类型、`interface-name` 在代理组上的使用等已被移除的特性。

## 改动范围

### 1. merger.rs 模板修复 (`src/subscription/merger.rs`)

`DEFAULT_TEMPLATE` 与 `install.sh` 模板对齐：

- `enhanced-mode` 从 `redir-host` 改为 `fake-ip`
- 新增 `fake-ip-range: 198.18.0.1/16`
- 新增 `fake-ip-filter` 包含 `*.github.com` 和 `github.com`
- DNS 服务器改为 DoH（`https://223.5.5.5/dns-query`, `https://doh.pub/dns-query`）
- 移除 `fallback` 和 `fallback-filter` 段（fake-ip 模式不需要）
- 新增 `DST-PORT,22,DIRECT` 规则（SSH 绕过）
- 新增 `dns-hijack` 配置

### 2. install.sh 增强 (`install.sh`)

**geodata 自动下载：** 从 MetaCubeX/meta-rules-dat 下载 `geosite.dat` 和 `geoip.metadb` 到 `~/.config/mihomo/`。

**CAP_NET_ADMIN 完整性检查：** 不再仅在新下载二进制时设置，每次运行都检查并修复。同时增加 `cap_net_raw` 和 `cap_net_bind_service`。

**系统级 service 选项：** 新增 `--system` 参数，安装为系统级 systemd 服务（使用 `/etc/mihomo/` 和 `/usr/bin/mihomo`），适配非 user-service 的部署方式。

### 3. mioctl doctor 诊断命令 (`src/cli/doctor.rs`)

新增 `mioctl doctor` 子命令，检查 6 项：

| 检查项 | 说明 | 检测方式 |
|--------|------|----------|
| CAP_NET_ADMIN | 二进制是否有 TUN 权限 | `getcap` 输出检查 |
| Geo 数据文件 | geosite.dat / Country.mmdb 是否存在 | 文件存在性检查 |
| 配置语法 | 配置是否合法 | `mihomo -t` |
| 进程冲突 | 是否有多个 mihomo 实例 | `ps aux | grep mihomo` |
| API 可达 | external-controller 是否响应 | HTTP GET `/version` |
| 系统代理 | 流量是否经过代理 | 环境变量 / gsettings / environment.d |

输出语义化 emoji + 中文说明。

### 4. CLI 结构

```
mioctl
├── tui           # 已有
├── sub           # 已有 (subscription)
├── connect       # 已有
└── doctor        # 新增
```

## 不变更

- 不修改 `install.sh` 中已有的 mihomo 二进制安装逻辑
- 不修改订阅解析 / 注入逻辑
- 不更改现有 CLI 参数结构

## 测试要点

- `merger.rs`: 现有单元测试需要更新期望值以匹配新模板
- `doctor`: 新增测试覆盖各种检查的通过/失败路径
- `install.sh`: 手动测试 geodata 下载和 setcap 修复逻辑
