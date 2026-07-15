# Mihomo 内存占用排查报告

> **日期**: 2026-06-17
> **环境**: Arch Linux, 31GB RAM, mihomo v1.19.27
> **作者**: Claude Code

---

## 一、排查背景

在 v1.19.27（已包含 #1908 unclosed session 泄漏修复）上仍观察到高内存占用，需确认是否存在内存泄漏并提供优化方案。

---

## 二、系统状态（排查时刻）

### 2.1 整体内存压力

```
总内存   31 GB
已用     27 GB        ← 剩余仅 ~500MB 可用
Swap     8 GB 全满    ← 严重告警
swappiness 60         ← 内核积极换页
```

系统整体处于**严重过载**状态。8GB swap 全部用满，表明物理内存严重不足。

### 2.2 主要进程内存排行

| 进程 | RSS | 占比 | 说明 |
|------|-----|------|------|
| **mihomo** | **~2.4 GB** | **7.4%** | 本报告排查对象 |
| DataGrip | 1.4 GB | 4.3% | JetBrains IDE |
| Chrome（主进程+渲染器） | 多个进程合计 ~5+ GB | ~15% | 大量浏览器标签 |
| DingTalk | 640 MB | 1.9% | 钉钉桌面客户端 |
| litellm | 640 MB | 1.9% | LLM 网关服务 |
| chroma-mcp | 544 MB | 1.6% | 向量数据库 MCP |
| figma-linux | 516 MB | 1.5% | Figma 客户端 |

### 2.3 mihomo 进程详情

**进程信息:**
- PID: 759
- 运行时长: 6 天 0 小时
- 启动命令: `/usr/bin/mihomo -d /home/xuqinqin/.config/mihomo -f /home/xuqinqin/.config/mihomo/config.yaml`
- 无 systemd 托管，无内存限制

**内存快照:**

| 指标 | 数值 | 说明 |
|------|------|------|
| VmPeak | 11.4 GB | 虚拟地址空间峰值（Go 预留行为） |
| VmRSS  | 1.9 GB | 驻留物理内存 |
| VmSwap | 2.5 GB | 被换出到 swap 的部分 |

> **注**: VmRSS 在两次采样中分别为 2.4GB 和 1.9GB，说明 RRS 有波动，不是持续单向增长。

---

## 三、mihomo 配置分析

### 3.1 关键配置项

```yaml
# --- 网络模式 ---
tun:
  enable: true
  stack: gvisor        # ← 用户态网络栈，内存开销大
  dns-hijack:
  - any:53

# --- DNS ---
dns:
  enable: true
  enhanced-mode: fake-ip    # ← Fake-IP 缓存无 TTL 限制
  fake-ip-range: 198.18.0.1/16

# --- 日志 ---
log-level: warning          # ← 这个设置合理，不是 debug

# --- 持久化 ---
profile:
  store-selected: true      # ← 每次切换节点写入 cache.db

# --- 代理节点 ---
# 共 50+ 节点，分三种协议：
#   - Tuic × 9  （QUIC/h3，最耗内存）
#   - AnyTLS × ~16 （TCP，较轻量）
#   - Mieru × ~25  （TCP + multiplexing）
```

### 3.2 代理组结构

```yaml
proxy-groups:
  - ♻️自动选择    # url-test，包含全部 50+ 节点
  - 🔯故障转移    # fallback，包含全部 50+ 节点
  - 狗狗加速.com  # select，全部节点
  - Tuic         # select，仅 Tuic 节点
  - AnyTLS       # select，仅 AnyTLS 节点
  - 专线         # select，仅专线节点
  - M            # select，仅 Mieru 节点
  - 🔥ChatGPT    # select，全部节点
```

**问题**: `♻️自动选择` 使用 `url-test` 类型且包含所有 50+ 节点。url-test 每 1800 秒对所有节点并发测速，这会同时唤醒所有连接（包括 9 个 Tuic 的 QUIC 连接）。

---

## 四、内存消耗归因

### 4.1 是否为内存泄漏？

**结论: 大概率不是传统意义上的内存泄漏，而是配置不当 + 系统过载导致的持续高占用。**

依据:
1. 已在 v1.19.27（远超修复版本 #1908）
2. RRS 有波动（2.4GB ↔ 1.9GB），GC 在工作
3. VmPeak 11.4GB 是 Go 运行时预留地址空间的行为，并非实际物理占用
4. 系统整体 swap 已满，所有进程都在被换出换入

### 4.2 各模块内存估算

| 模块 | 估算内存 | 说明 |
|------|----------|------|
| TUN gVisor 栈 | ~80-150 MB | 用户态网络栈，含所有连接缓冲区 |
| Tuic × 9 | ~270 MB | 每个 QUIC 连接 ~30MB |
| AnyTLS × 16 | ~80 MB | TCP 连接，开销低 |
| Mieru × 25 | ~200 MB | TCP + multiplexing 中间层 |
| Fake-IP 缓存 | ~50-200 MB | 随时间增长，无上限 |
| Geo 数据 | ~30 MB | Country.mmdb + geoip.dat + geosite.dat |
| Go 运行时 | ~200-500 MB | GC 不归还给 OS 的空闲堆 |

> **估算总计**: ~1.5~2.5 GB，与观测到的 RSS 吻合。

---

## 五、优化建议

### 5.1 立即执行（不改配置，只加环境变量）

**设置 `GOMEMLIMIT`**

mihomo 是 Go 编写，Go 的 GC 释放堆内存但不归还给 OS。设置硬上限：

```bash
GOMEMLIMIT=1536MiB /usr/bin/mihomo -d /home/xuqinqin/.config/mihomo ...
```

或写到启动脚本中。可将 RSS 从 ~2.4GB 限制到 ~1.5GB 以内。

### 5.2 5 分钟内可完成

**TUN stack 从 `gvisor` 换成 `system`**

```yaml
tun:
  enable: true
  stack: system    # 从 gvisor 改成 system
```

`stack: system` 直接走内核网络栈，省去用户态栈的额外内存（约 50-80MB）。

### 5.3 推荐执行

**瘦身 url-test 组**

`♻️自动选择` 从 50+ 节点精简到 5-10 个代表性节点：

```yaml
- name: ♻️自动选择
  type: url-test
  proxies:
  - 🇭🇰1香港-专线(AnyTLS)
  - 🇯🇵7日本-专线(AnyTLS)
  - 🇸🇬15新加坡-专线(AnyTLS)
  - 🇺🇸11美国西集群-专线(AnyTLS)
  - 🇬🇧17英国-全网优化(Tuic)
  # 只保留最快最稳的节点
  url: http://cp.cloudflare.com/generate_204
  interval: 1800
```

**其他配置微调：**

```yaml
dns:
  fake-ip-filter-mode: blacklist   # 减少 fake-ip 查找

profile:
  store-selected: false            # 关闭持久化选中节点
```

### 5.4 如果不需要全局代理

**关闭 TUN 模式**是最彻底的方案。如果只是浏览器 + 终端代理使用场景，改为系统代理可节省 200-500 MB。

---

## 六、排查方法参考

如需确认是否真正泄漏：

```bash
# 1. 启用 debug 端口
# 配置中已有 external-controller: 127.0.0.1:9090

# 2. 设置 log-level: debug 后重启

# 3. 下载 heap profile
curl -o heap.pprof http://127.0.0.1:9090/debug/pprof/heap?raw=true

# 4. 连续采样对比
sleep 3600 && curl -o heap2.pprof http://127.0.0.1:9090/debug/pprof/heap?raw=true

# 5. 用 go tool pprof 分析差异
go tool pprof -base heap.pprof heap2.pprof
```

---

## 七、总结

| 问题 | 判断 |
|------|------|
| 是否 unclosed session 泄漏 | ❌ 已在 v1.19.4 修复，当前 v1.19.27 |
| 是否有未修复的泄漏 | ⚠️ 大概率不是，但无法完全排除 |
| 主要原因 | **配置过重（TUN + 50+节点含 Tuic）+ 系统过载（27GB/31GB 已用）** |
| 首推措施 | `GOMEMLIMIT=1536MiB` |

单靠 mihomo 端优化可以缓解，但根本问题在于这台机器同时运行的进程太多（Chrome、DataGrip、DingTalk、litellm、chroma-mcp、figma...），总内存需求已超过物理 31GB。
