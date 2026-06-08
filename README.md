# mioctl

Terminal UI management tool for [mihomo](https://github.com/MetaCubeX/mihomo) (Clash.Meta).

## Features

- Interactive TUI with sidebar navigation (Dashboard, Proxies, Connections, Rules, Logs)
- Full mihomo REST API integration
- Subscription management via URL (YAML / Base64 / URI format support)
- Proxy node switching, latency testing, health checks
- Vim-style keybindings, Catppuccin Mocha theme

## Installation

```bash
cargo install --path .
```

## Quick Start

1. Ensure mihomo is running with `external-controller` enabled:
   ```yaml
   external-controller: 127.0.0.1:9090
   secret: "your-secret"
   ```

2. Launch TUI:
   ```bash
   mioctl tui
   ```

3. Test connectivity:
   ```bash
   mioctl connect test
   ```

4. Update subscriptions:
   ```bash
   mioctl sub update --all
   ```

## Configuration

`~/.config/mioctl/config.toml`:
```toml
[mihomo]
external-controller = "127.0.0.1:9090"
secret = ""

[subscriptions]
update-interval-minutes = 240
[[subscriptions.items]]
name = "example"
url = "https://example.com/sub"
```

## Keybindings

| Key | Action |
|-----|--------|
| `1-5` | Switch view |
| `j/k` | Navigate up/down |
| `g` / `G` | Jump top / bottom |
| `h/l` | Prev/next group (Proxies view) |
| `Enter` | Switch to selected node |
| `t` | Test node latency |
| `d` / `D` | Close connection / all |
| `Space` | Pause logs |
| `:` | Command mode |
| `q` | Quit |

## License

MIT
