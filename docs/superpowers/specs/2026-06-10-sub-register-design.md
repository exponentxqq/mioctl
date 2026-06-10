# mioctl sub register — Design Spec

Date: 2026-06-10

## Problem

Currently, importing a subscription into mioctl + mihomo requires 5+ manual steps:
edit `config.toml` to add URL, edit `config.yaml` to add proxy-provider reference,
manually merge proxy-groups/rules, fiddle with User-Agent, restart mihomo.
The goal is a single command: `mioctl sub register <url>` that does all of this.

## Command

```
mioctl sub register <url> [--name <name>] [--no-reload]
```

| Argument | Required | Description |
|----------|----------|-------------|
| `url` | yes | Subscription URL |
| `--name` | no | Override auto-detected subscription name |
| `--no-reload` | no | Skip mihomo reload (debug use) |

## Flow

```
1. fetch(url)        — HTTP GET, auto-detect User-Agent
2. detect_format()   — YAML / Base64 / PlainUri (existing logic)
3. parse_full()      — NEW: preserve proxies + proxy-groups + rules (not just proxies)
4. auto_detect_name()— NEW: extract name from subscription or URL
5. merge_config()    — NEW: smart-merge into ~/.config/mihomo/config.yaml
6. save_to_config()  — add subscription item to config.toml (existing add_subscription)
7. reload_mihomo()   — PUT /configs?force=true, fallback to systemctl restart
8. print_summary()   — nodes count, groups list, mihomo status
```

### Auto-detect name

Priority order:
1. `--name` CLI argument (if provided)
2. First proxy-group's `name` field from subscription YAML (e.g., "狗狗加速.com")
3. Hostname from URL (e.g., `doggygosubs` from `xWjXVnD.doggygosubs.com`)
4. Prompt user interactively via stdin

### Auto-detect User-Agent

Try in order, first response with >= 3 valid proxies wins. Timeout 10s per attempt.

| Priority | User-Agent |
|----------|-----------|
| 1 | `mihomo/{version}` — version from mihomo API `/version` |
| 2 | `ClashMeta/1.19.0` |
| 3 | `clash-verge/1.3.8` |

All failed → error exit with suggestion to use `--user-agent` (future flag).

### Parser changes

New function `parse_subscription_full()` in `parser.rs`:

- YAML format: deserialize into `serde_yaml::Value`, extract top-level `proxies`, `proxy-groups`, `rules` keys. Validate proxies is a non-empty sequence.
- Base64/PlainUri format: only proxies available (no groups/rules). Generate a single proxy-group named after the subscription and a default `MATCH,{group-name}` rule.
- Return `SubscriptionContent { proxies, proxy_groups, rules }` struct.

### YAML smart merge

Function `merge_mihomo_config()`:

1. Read existing `~/.config/mihomo/config.yaml` as `serde_yaml::Mapping`
2. If file doesn't exist, create from built-in template:
   ```yaml
   mixed-port: 7897
   external-controller: 127.0.0.1:9090
   mode: rule
   log-level: info
   allow-lan: false
   dns:
     enable: true
     enhanced-mode: redir-host
     nameserver: [223.5.5.5, 119.29.29.29]
     fallback: [tls://1.1.1.1:853, tls://8.8.8.8:853]
     fallback-filter: { geoip: true, geoip-code: CN }
   tun:
     enable: true
     stack: gvisor
     auto-route: true
     auto-detect-interface: true
   sniffer:
     enable: true
     sniffing: [tls, http]
   ```
3. **Preserve** these keys (keep existing values): `mixed-port`, `external-controller`, `mode`, `log-level`, `allow-lan`, `dns`, `tun`, `sniffer`, `ipv6`, `profile`, `hosts`, `interface-name`, `routing-mark`, `bind-address`, `authentication`, `tcp-concurrent`, `geodata-mode`, `geox-url`, `unified-delay`, `keep-alive-interval`
4. **Replace** these keys: `proxies`, `proxy-groups`, `rules`
5. **Remove** key `proxy-providers` (no longer needed with inline proxies)
6. Serialize back preserving key order: infrastructure keys first, then proxies, proxy-groups, rules
7. Write, preserving Unix line endings

### Mihomo reload

1. Primary: `PUT /configs?force=true` via `MihomoClient::reload_config()` (existing endpoint)
2. If API call fails or returns error: fall back to `systemctl --user restart mihomo`
3. If `--no-reload` flag set: skip, print manual reload instructions

### Safety

- Before writing config.yaml: copy old file to `config.yaml.bak`
- If reload fails: attempt rollback from `config.yaml.bak`, print warning
- If fetch/parse fails: do not touch any files on disk
- Validate merged YAML is syntactically valid before writing

## Files changed

| File | Change |
|------|--------|
| `src/cli/mod.rs` | Add `SubAction::Register { url, name, no_reload }` variant |
| `src/cli/sub.rs` | Add `run_register()` handler |
| `src/subscription/fetcher.rs` | Add `fetch_with_ua_probe()` — tries multiple UAs |
| `src/subscription/parser.rs` | Add `parse_subscription_full()` and `SubscriptionContent` struct |
| `src/subscription/merger.rs` | NEW — YAML config merge logic |
| `src/subscription/manager.rs` | Add `register()` method |
| `src/subscription/mod.rs` | Export new types |
| `src/config/mioctl_config.rs` | No structural changes (add_subscription already exists) |

## Testing

- Unit: parser tests for `parse_subscription_full()` with multi-group YAML
- Unit: merger tests for key-preserve/replace/remove logic
- Unit: name detection from various URL and YAML inputs
- Integration: wiremock-based test simulating subscription server with different UAs
- Manual: `mioctl sub register` against real subscription URL, verify config.yaml output and mihomo loads

## Out of scope

- `sub remove` or `sub list` commands (future work)
- Parallel subscription updates from multiple URLs
- Filter-based proxy-group optimization (subscriptions that return 500+ nodes)
- Subscription auto-update scheduling
