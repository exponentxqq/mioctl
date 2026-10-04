use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use futures_util::stream::{self, StreamExt};
use serde::Deserialize;

#[derive(clap::Subcommand)]
pub enum DoctorAction {
    /// Run diagnostic checks
    Run,
    /// Pre-check TCP connectivity of every node endpoint in the mihomo config
    Nodes {
        /// Per-endpoint TCP connect timeout in milliseconds
        #[arg(long, default_value_t = 3000)]
        timeout_ms: u64,
        /// Maximum number of concurrent probes
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
        /// Force raw TCP probing even when mihomo TUN would intercept it
        #[arg(long)]
        raw: bool,
    },
}

pub async fn run(action: Option<DoctorAction>) -> bool {
    match action.unwrap_or(DoctorAction::Run) {
        DoctorAction::Run => {
            run_diagnostics().await;
            true
        }
        DoctorAction::Nodes {
            timeout_ms,
            concurrency,
            raw,
        } => check_nodes(timeout_ms, concurrency, raw).await,
    }
}

async fn run_diagnostics() {
    println!("\n  Mihomo Doctor\n");

    let config = crate::config::mioctl_config::MioctlConfig::load();

    // 1. CAP_NET_ADMIN check
    check_cap_net_admin();

    // 2. Geo data files check
    check_geo_files(&config);

    // 3. Config syntax check
    check_config_syntax(&config);

    // 4. Process conflict check
    check_process_conflict();

    // 5. API reachable check
    check_api_reachable(&config).await;

    // 6. System proxy check
    check_system_proxy();

    println!();
}

fn status(ok: bool, label: &str, detail: &str) {
    let icon = if ok {
        "\x1b[32m✅\x1b[0m"
    } else {
        "\x1b[31m✗\x1b[0m"
    };
    println!("  {}  {:<22} {}", icon, label, detail);
}

fn warn(label: &str, detail: &str) {
    println!("  \x1b[33m⚠️\x1b[0m  {:<22} {}", label, detail);
}

fn check_cap_net_admin() {
    let candidates = [
        "/usr/bin/mihomo".to_string(),
        format!(
            "{}/.config/mioctl/bin/mihomo",
            std::env::var("HOME").unwrap_or_default()
        ),
    ];
    let mut found = false;
    for path in &candidates {
        if !std::path::Path::new(path).exists() {
            continue;
        }
        found = true;
        let output = Command::new("getcap").arg(path).output();
        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                if stdout.contains("cap_net_admin") {
                    status(
                        true,
                        "CAP_NET_ADMIN",
                        &format!("{} has required capabilities", path),
                    );
                } else {
                    status(
                        false,
                        "CAP_NET_ADMIN",
                        &format!(
                            "{} lacks cap_net_admin — TUN mode will fail. Run: sudo setcap cap_net_admin,cap_net_raw,cap_net_bind_service=+eip {}",
                            path, path
                        ),
                    );
                }
            }
            _ => {
                warn(
                    "CAP_NET_ADMIN",
                    &format!("cannot check {} (getcap not found?)", path),
                );
            }
        }
    }
    if !found {
        warn("CAP_NET_ADMIN", "no mihomo binary found at common paths");
    }
}

fn check_geo_files(_config: &crate::config::mioctl_config::MioctlConfig) {
    let home = std::env::var("HOME").unwrap_or_default();
    let mihomo_dir = std::path::Path::new(&home).join(".config/mihomo");

    let geosite = mihomo_dir.join("geosite.dat");
    let mmdb = mihomo_dir.join("Country.mmdb");

    let has_geosite = geosite.exists();
    let has_mmdb = mmdb.exists();

    if has_geosite && has_mmdb {
        status(true, "Geo data files", "geosite.dat + Country.mmdb found");
    } else {
        let mut missing = vec![];
        if !has_geosite {
            missing.push("geosite.dat");
        }
        if !has_mmdb {
            missing.push("Country.mmdb");
        }
        status(
            false,
            "Geo data files",
            &format!(
                "missing: {} — GEOSITE/GEOIP rules may fail",
                missing.join(", ")
            ),
        );
    }
}

/// Locate the mihomo binary: prefer PATH, fall back to the mioctl-managed install.
fn mihomo_binary() -> PathBuf {
    if let Ok(output) = Command::new("mihomo").arg("-v").output() {
        if output.status.success() {
            return PathBuf::from("mihomo");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let managed = Path::new(&home).join(".config/mioctl/bin/mihomo");
        if managed.exists() {
            return managed;
        }
    }
    PathBuf::from("mihomo")
}

fn check_config_syntax(config: &crate::config::mioctl_config::MioctlConfig) {
    let config_path = PathBuf::from(&config.mihomo.config_path);

    if !config_path.exists() {
        status(
            false,
            "Config syntax",
            &format!("config not found at {}", config_path.display()),
        );
        return;
    }

    let Some(config_str) = config_path.to_str() else {
        warn("Config syntax", "config path is not valid UTF-8");
        return;
    };
    let config_dir = config_path.parent().and_then(|p| p.to_str()).unwrap_or("");

    let output = Command::new(mihomo_binary())
        .args(["-t", "-f", config_str, "-d", config_dir])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            status(true, "Config syntax", "valid");
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let msg = stderr.lines().last().unwrap_or("unknown error");
            status(false, "Config syntax", &format!("invalid — {}", msg));
        }
        Err(e) => {
            warn("Config syntax", &format!("cannot run mihomo -t: {}", e));
        }
    }
}

fn check_process_conflict() {
    let output = Command::new("sh")
        .arg("-c")
        .arg("ps aux | grep '[m]ihomo' | wc -l")
        .output();

    match output {
        Ok(o) => {
            let count_str = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if let Ok(count) = count_str.parse::<usize>() {
                if count == 0 {
                    status(false, "Process", "mihomo is not running");
                } else if count == 1 {
                    status(true, "Process", "1 mihomo instance running");
                } else {
                    warn(
                        "Process",
                        &format!(
                            "{} mihomo instances running — possible port/config conflict",
                            count
                        ),
                    );
                }
            }
        }
        Err(_) => warn("Process", "cannot check mihomo processes"),
    }
}

async fn check_api_reachable(config: &crate::config::mioctl_config::MioctlConfig) {
    let secret = if config.mihomo.secret.is_empty() {
        None
    } else {
        Some(config.mihomo.secret.clone())
    };

    match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
        Ok(client) => {
            let url = format!("{}/version", client.base_url());
            match client.client().get(&url).send().await {
                Ok(resp) => {
                    let body = resp.text().await.unwrap_or_default();
                    status(
                        true,
                        "API reachable",
                        &format!("{} ({})", config.mihomo.external_controller, body.trim()),
                    );
                }
                Err(e) => status(
                    false,
                    "API reachable",
                    &format!("{} — {}", config.mihomo.external_controller, e),
                ),
            }
        }
        Err(e) => {
            status(
                false,
                "API reachable",
                &format!("{} — {}", config.mihomo.external_controller, e),
            );
        }
    }
}

fn check_system_proxy() {
    let mut active = false;
    let mut details = vec![];

    // Check env vars
    for var in &[
        "http_proxy",
        "https_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ] {
        if std::env::var(var).is_ok() {
            active = true;
            details.push(format!("env:{}", var));
            break;
        }
    }

    // Check gsettings (GNOME)
    if let Ok(o) = Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
    {
        let mode = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if mode == "'manual'" || mode == "'auto'" {
            active = true;
            details.push("gsettings".to_string());
        }
    }

    // Check environment.d
    let home = std::env::var("HOME").unwrap_or_default();
    let env_conf = std::path::Path::new(&home).join(".config/environment.d/proxy.conf");
    if env_conf.exists() {
        active = true;
        details.push("environment.d".to_string());
    }

    if active {
        status(
            true,
            "System proxy",
            &format!("configured ({})", details.join(", ")),
        );
    } else {
        warn(
            "System proxy",
            "not configured — browser traffic won't go through proxy. Set http_proxy/https_proxy or use TUN mode.",
        );
    }
}

// ---------------------------------------------------------------------------
// Node connectivity pre-check (`mioctl doctor nodes`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct ProxyEntry {
    #[serde(default)]
    name: String,
    #[serde(rename = "type", default)]
    node_type: String,
    #[serde(default)]
    server: String,
    #[serde(default, deserialize_with = "de_port")]
    port: Option<u16>,
}

/// Accept both `port: 443` and `port: "443"` — subscription configs vary.
fn de_port<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_yaml::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_yaml::Value::Number(n)) => n.as_u64().and_then(|v| u16::try_from(v).ok()),
        Some(serde_yaml::Value::String(s)) => s.trim().parse::<u16>().ok(),
        _ => None,
    })
}

/// Exact-match `hosts` overrides from the mihomo config (wildcards are ignored).
#[derive(Debug, Default)]
struct HostsOverrides {
    exact: HashMap<String, Vec<IpAddr>>,
}

fn load_hosts_overrides(root: &serde_yaml::Value) -> HostsOverrides {
    let mut overrides = HostsOverrides::default();
    let Some(mapping) = root.get("hosts").and_then(|v| v.as_mapping()) else {
        return overrides;
    };
    for (key, value) in mapping {
        let Some(domain) = key.as_str() else { continue };
        let ips: Vec<IpAddr> = match value {
            serde_yaml::Value::String(s) => s.parse().ok().into_iter().collect(),
            serde_yaml::Value::Sequence(seq) => seq
                .iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| s.parse().ok())
                .collect(),
            _ => Vec::new(),
        };
        if !ips.is_empty() {
            overrides.exact.insert(domain.to_ascii_lowercase(), ips);
        }
    }
    overrides
}

struct LoadedConfig {
    entries: Vec<ProxyEntry>,
    skipped: usize,
    hosts: HostsOverrides,
    tun_enabled: bool,
    tun_device: String,
}

fn load_config_entries(path: &Path) -> Result<LoadedConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {} 失败: {}", path.display(), e))?;
    let root: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|e| format!("解析 YAML 失败: {}", e))?;
    let hosts = load_hosts_overrides(&root);

    let tun = root.get("tun");
    let tun_enabled = tun
        .and_then(|t| t.get("enable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let tun_device = tun
        .and_then(|t| t.get("device"))
        .and_then(|v| v.as_str())
        .unwrap_or("Meta")
        .to_string();

    let items = root
        .get("proxies")
        .and_then(|v| v.as_sequence())
        .cloned()
        .unwrap_or_default();

    let mut entries = Vec::new();
    let mut skipped = 0usize;
    for item in items {
        match serde_yaml::from_value::<ProxyEntry>(item) {
            Ok(entry) if !entry.server.is_empty() && entry.port.is_some() => entries.push(entry),
            _ => skipped += 1,
        }
    }
    Ok(LoadedConfig {
        entries,
        skipped,
        hosts,
        tun_enabled,
        tun_device,
    })
}

#[derive(Debug)]
struct Endpoint {
    server: String,
    port: u16,
    names: Vec<String>,
    /// True when every node on this endpoint uses a UDP-based transport.
    udp_only: bool,
}

fn is_udp_type(node_type: &str) -> bool {
    matches!(
        node_type.to_ascii_lowercase().as_str(),
        "hysteria" | "hysteria2" | "tuic"
    )
}

/// Deduplicate nodes into unique `server:port` endpoints (case-insensitive host).
fn group_endpoints(entries: &[ProxyEntry]) -> Vec<Endpoint> {
    let mut order: Vec<(String, u16)> = Vec::new();
    let mut map: HashMap<(String, u16), Endpoint> = HashMap::new();

    for entry in entries {
        let Some(port) = entry.port else { continue };
        let key = (entry.server.to_ascii_lowercase(), port);
        let endpoint = map.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Endpoint {
                server: entry.server.clone(),
                port,
                names: Vec::new(),
                udp_only: true,
            }
        });
        if !entry.name.is_empty() && !endpoint.names.contains(&entry.name) {
            endpoint.names.push(entry.name.clone());
        }
        if !is_udp_type(&entry.node_type) {
            endpoint.udp_only = false;
        }
    }

    order
        .into_iter()
        .filter_map(|key| map.remove(&key))
        .collect()
}

#[derive(Debug)]
enum ProbeOutcome {
    Reachable,
    Refused,
    TimedOut,
    /// The TCP handshake was answered locally by a TUN stack, not the server.
    TunIntercepted,
    FakeIp,
    DnsFailed(String),
    Error(String),
}

#[derive(Debug)]
struct ProbeReport {
    outcome: ProbeOutcome,
    dial_ip: Option<IpAddr>,
    extra_ips: usize,
    /// Where `dial_ip` came from: hosts / mihomo-dns / system / literal.
    source: &'static str,
    elapsed: Duration,
}

fn is_fake_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)
        }
        IpAddr::V6(_) => false,
    }
}

fn is_tun_intercepted(local_addr: Option<SocketAddr>) -> bool {
    local_addr
        .map(|addr| is_fake_ip(&addr.ip()))
        .unwrap_or(false)
}

/// Ask mihomo's own resolver (`/dns/query`) for real A records.
async fn resolve_via_dns_api(
    client: &crate::api::client::MihomoClient,
    name: &str,
) -> Result<Vec<IpAddr>, String> {
    let url = format!("{}/dns/query", client.base_url());
    let response = client
        .client()
        .get(&url)
        .query(&[("name", name), ("type", "A")])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let status = body.get("Status").and_then(|v| v.as_u64()).unwrap_or(0);
    if status != 0 {
        return Err(format!("DNS status {}", status));
    }

    let mut ips = Vec::new();
    if let Some(answers) = body.get("Answer").and_then(|v| v.as_array()) {
        for answer in answers {
            if let Some(data) = answer.get("data").and_then(|v| v.as_str()) {
                if let Ok(ip) = data.parse::<IpAddr>() {
                    if !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
        }
    }
    if ips.is_empty() {
        Err("no A record".into())
    } else {
        Ok(ips)
    }
}

/// Resolve the address mihomo would actually dial: hosts override > mihomo DNS > system.
async fn resolve_dial_ips(
    server: &str,
    hosts: &HostsOverrides,
    client: Option<&crate::api::client::MihomoClient>,
) -> Result<(Vec<IpAddr>, &'static str), String> {
    if let Ok(ip) = server.parse::<IpAddr>() {
        return Ok((vec![ip], "literal"));
    }
    if let Some(ips) = hosts.exact.get(&server.to_ascii_lowercase()) {
        return Ok((ips.clone(), "hosts"));
    }
    if let Some(client) = client {
        if let Ok(ips) = resolve_via_dns_api(client, server).await {
            return Ok((ips, "mihomo-dns"));
        }
    }
    match tokio::net::lookup_host((server, 0)).await {
        Ok(addrs) => {
            let mut ips: Vec<IpAddr> = Vec::new();
            for addr in addrs {
                let ip = addr.ip();
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
            ips.sort_by_key(|ip| ip.is_ipv6());
            if ips.is_empty() {
                Err("no address returned".into())
            } else {
                Ok((ips, "system"))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

async fn probe_endpoint(
    endpoint: &Endpoint,
    hosts: &HostsOverrides,
    client: Option<&crate::api::client::MihomoClient>,
    timeout: Duration,
) -> ProbeReport {
    let started = Instant::now();
    let (ips, source) = match resolve_dial_ips(&endpoint.server, hosts, client).await {
        Ok(resolved) => resolved,
        Err(e) => {
            return ProbeReport {
                outcome: ProbeOutcome::DnsFailed(e),
                dial_ip: None,
                extra_ips: 0,
                source: "-",
                elapsed: started.elapsed(),
            }
        }
    };

    let dial_ip = ips.first().copied();
    let extra_ips = ips.len().saturating_sub(1);

    if ips.iter().any(is_fake_ip) {
        return ProbeReport {
            outcome: ProbeOutcome::FakeIp,
            dial_ip,
            extra_ips,
            source,
            elapsed: started.elapsed(),
        };
    }

    let Some(dial_ip) = dial_ip else {
        return ProbeReport {
            outcome: ProbeOutcome::DnsFailed("no address".into()),
            dial_ip: None,
            extra_ips,
            source,
            elapsed: started.elapsed(),
        };
    };

    let outcome = match tokio::time::timeout(
        timeout,
        tokio::net::TcpStream::connect((dial_ip, endpoint.port)),
    )
    .await
    {
        Ok(Ok(stream)) => {
            if is_tun_intercepted(stream.local_addr().ok()) {
                ProbeOutcome::TunIntercepted
            } else {
                ProbeOutcome::Reachable
            }
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => ProbeOutcome::Refused,
        Ok(Err(e)) => ProbeOutcome::Error(e.to_string()),
        Err(_) => ProbeOutcome::TimedOut,
    };

    ProbeReport {
        outcome,
        dial_ip: Some(dial_ip),
        extra_ips,
        source,
        elapsed: started.elapsed(),
    }
}

fn display_name(names: &[String]) -> String {
    match names.split_first() {
        Some((first, rest)) if !rest.is_empty() => format!("{} 等 {} 个节点", first, names.len()),
        Some((first, _)) => first.clone(),
        None => "(未命名节点)".into(),
    }
}

fn source_tag(source: &str) -> &'static str {
    match source {
        "hosts" => " [hosts 覆写]",
        _ => "",
    }
}

fn describe_target(endpoint: &Endpoint, report: &ProbeReport) -> String {
    let base = format!("{}:{}", endpoint.server, endpoint.port);
    match &report.outcome {
        ProbeOutcome::FakeIp => {
            let ip = report
                .dial_ip
                .map(|ip| ip.to_string())
                .unwrap_or_else(|| "-".into());
            format!("{} → {} (fake-ip，DNS 被 mihomo 劫持，跳过探测)", base, ip)
        }
        ProbeOutcome::DnsFailed(_) => base,
        _ => match report.dial_ip {
            Some(ip) if report.extra_ips > 0 => format!(
                "{} → {} (+{}){}",
                base,
                ip,
                report.extra_ips,
                source_tag(report.source)
            ),
            Some(ip) => format!("{} → {}{}", base, ip, source_tag(report.source)),
            None => base,
        },
    }
}

fn report_group_key(report: &ProbeReport) -> String {
    match &report.outcome {
        ProbeOutcome::FakeIp => "fake-ip".to_string(),
        ProbeOutcome::DnsFailed(_) => "解析失败".to_string(),
        _ => report
            .dial_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "未知".to_string()),
    }
}

#[derive(Default)]
struct IpGroup {
    label: String,
    total: usize,
    reachable: usize,
    other: usize,
    tcp_total: usize,
    tcp_timed_out: usize,
}

pub async fn check_nodes(timeout_ms: u64, concurrency: usize, raw: bool) -> bool {
    println!("\n  Mihomo Doctor — 节点连通性预检\n");

    let config = crate::config::mioctl_config::MioctlConfig::load();
    let path = PathBuf::from(&config.mihomo.config_path);

    let loaded = match load_config_entries(&path) {
        Ok(loaded) => loaded,
        Err(e) => {
            status(false, "Config", &e);
            println!();
            return false;
        }
    };
    if loaded.entries.is_empty() {
        status(
            false,
            "Nodes",
            "mihomo 配置中没有可解析的节点（proxies 为空？）",
        );
        println!();
        return false;
    }

    let timeout = Duration::from_millis(timeout_ms.max(1));
    let concurrency = concurrency.max(1);
    let secret = if config.mihomo.secret.is_empty() {
        None
    } else {
        Some(config.mihomo.secret.clone())
    };
    let client =
        crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret).ok();
    let tun_active =
        loaded.tun_enabled && Path::new(&format!("/sys/class/net/{}", loaded.tun_device)).exists();
    let use_api = !raw && client.is_some();

    let skipped_note = if loaded.skipped > 0 {
        format!("（跳过 {} 个无 server/port 条目）", loaded.skipped)
    } else {
        String::new()
    };
    println!("  配置文件: {}", path.display());
    println!(
        "  节点 {} 个{}   超时 {}ms   并发 {}",
        loaded.entries.len(),
        skipped_note,
        timeout_ms,
        concurrency
    );
    println!(
        "  探测方式: {}",
        if use_api {
            "mihomo API 延迟测试（真实节点可用性）"
        } else {
            "原始 TCP 直连探测"
        }
    );
    if tun_active && use_api {
        println!(
            "  \x1b[33m⚠️\x1b[0m  TUN                    TUN 运行中 — 直连会被接管，已自动改用 API 延迟测试"
        );
    } else if tun_active {
        warn(
            "TUN",
            "TUN 运行中 — 直连 TCP 会被本地接管，结果标记为不可判定；去掉 --raw 可获得真实结果",
        );
    }
    println!();

    match client.as_ref() {
        Some(client) if use_api => {
            run_api_probes(
                &loaded,
                client,
                &config.preferences.delay_test_url,
                timeout,
                concurrency,
            )
            .await
        }
        client => run_raw_probes(&loaded, client, timeout, concurrency).await,
    }
}

fn finish_summary(
    groups: &[IpGroup],
    reachable: usize,
    total: usize,
    undetermined: usize,
    blocked_hint: &str,
) -> bool {
    println!();
    println!("  按解析 IP 汇总:");
    let mut blocked_labels: Vec<&str> = Vec::new();
    for group in groups {
        let mut verdict = format!("{}/{} 可达", group.reachable, group.total);
        if group.reachable == 0 && group.tcp_total > 0 && group.tcp_timed_out == group.tcp_total {
            verdict.push_str(&format!("  ← {}", blocked_hint));
            blocked_labels.push(&group.label);
        }
        println!("    {:<16} {}", group.label, verdict);
    }
    if !blocked_labels.is_empty() {
        warn(
            "Hints",
            &format!(
                "{} 可能被阻断 — 可在 mihomo 配置 hosts 段把相关域名指向可用边缘后重载",
                blocked_labels.join(", ")
            ),
        );
    }

    println!();
    if reachable == 0 && undetermined == total && total > 0 {
        warn(
            "Summary",
            &format!("0/{} 可达（全部被 TUN 接管，无法判定）", total),
        );
        println!();
        false
    } else if reachable == 0 {
        status(false, "Summary", &format!("0/{} 可达", total));
        println!();
        false
    } else {
        status(true, "Summary", &format!("{}/{} 可达", reachable, total));
        println!();
        true
    }
}

/// Delay-test every node through mihomo itself. This is the ground truth when
/// TUN is active, because mihomo dials the node from userspace (bypassing the
/// local TUN accept), and it also validates the full protocol handshake.
async fn run_api_probes(
    loaded: &LoadedConfig,
    client: &crate::api::client::MihomoClient,
    delay_test_url: &str,
    timeout: Duration,
    concurrency: usize,
) -> bool {
    // Resolve each unique server domain once (display only — mihomo dials itself).
    let mut dns_cache: HashMap<String, (Vec<IpAddr>, &'static str)> = HashMap::new();
    for entry in &loaded.entries {
        let key = entry.server.to_ascii_lowercase();
        if dns_cache.contains_key(&key) {
            continue;
        }
        let resolved = resolve_dial_ips(&entry.server, &loaded.hosts, Some(client))
            .await
            .unwrap_or_default();
        dns_cache.insert(key, resolved);
    }

    let mut results: Vec<(usize, Result<u64, String>)> =
        stream::iter(loaded.entries.iter().enumerate())
            .map(|(idx, entry)| async move {
                let result = probe_node_delay(client, &entry.name, delay_test_url, timeout).await;
                (idx, result)
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;
    results.sort_by_key(|(idx, _)| *idx);

    let mut reachable = 0usize;
    let mut groups: Vec<IpGroup> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();

    for (idx, result) in &results {
        let entry = &loaded.entries[*idx];
        let name = if entry.name.is_empty() {
            "(未命名节点)".to_string()
        } else {
            entry.name.clone()
        };
        let (resolved, source) = dns_cache
            .get(&entry.server.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default();
        let dial_ip = resolved.first().copied();
        let port = entry.port.unwrap_or_default();
        let mut target = format!("{}:{}", entry.server, port);
        if let Some(ip) = dial_ip {
            if resolved.len() > 1 {
                target.push_str(&format!(
                    " → {} (+{}){}",
                    ip,
                    resolved.len() - 1,
                    source_tag(source)
                ));
            } else {
                target.push_str(&format!(" → {}{}", ip, source_tag(source)));
            }
        }

        let line = match result {
            Ok(delay) => {
                reachable += 1;
                format!(
                    "  \x1b[32m✅\x1b[0m {} — {} (延迟 {}ms)",
                    name, target, delay
                )
            }
            Err(e) => format!("  \x1b[31m✗\x1b[0m  {} — {} 测速失败: {}", name, target, e),
        };
        println!("{}", line);

        let key = dial_ip
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "解析失败".to_string());
        let group_idx = if let Some(&idx) = group_index.get(&key) {
            idx
        } else {
            groups.push(IpGroup {
                label: key.clone(),
                ..Default::default()
            });
            let idx = groups.len() - 1;
            group_index.insert(key, idx);
            idx
        };
        let group = &mut groups[group_idx];
        group.total += 1;
        group.tcp_total += 1;
        match result {
            Ok(_) => group.reachable += 1,
            Err(_) => {
                group.tcp_timed_out += 1;
                group.other += 1;
            }
        }
    }

    finish_summary(
        &groups,
        reachable,
        loaded.entries.len(),
        0,
        "全部测速失败，该边缘可能被阻断或节点已失效",
    )
}

fn parse_delay_response(success: bool, body: &serde_json::Value) -> Result<u64, String> {
    if success {
        if let Some(delay) = body.get("delay").and_then(|v| v.as_u64()) {
            return Ok(delay);
        }
    }
    Err(body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("delay test failed")
        .to_string())
}

async fn probe_node_delay(
    client: &crate::api::client::MihomoClient,
    name: &str,
    delay_test_url: &str,
    timeout: Duration,
) -> Result<u64, String> {
    let url = format!(
        "{}/proxies/{}/delay",
        client.base_url(),
        crate::api::endpoints::encode_path(name)
    );
    let timeout_ms = timeout.as_millis().to_string();
    let response = client
        .client()
        .get(&url)
        .query(&[("url", delay_test_url), ("timeout", timeout_ms.as_str())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let success = response.status().is_success();
    let body: serde_json::Value = response.json().await.unwrap_or_default();
    parse_delay_response(success, &body)
}

async fn run_raw_probes(
    loaded: &LoadedConfig,
    client: Option<&crate::api::client::MihomoClient>,
    timeout: Duration,
    concurrency: usize,
) -> bool {
    let endpoints = group_endpoints(&loaded.entries);
    if endpoints.is_empty() {
        status(false, "Nodes", "没有可探测的端点（缺少 server/port）");
        println!();
        return false;
    }

    let client_ref = client;
    let hosts_ref = &loaded.hosts;
    let mut results: Vec<(usize, ProbeReport)> = stream::iter(endpoints.iter().enumerate())
        .map(|(idx, endpoint)| async move {
            (
                idx,
                probe_endpoint(endpoint, hosts_ref, client_ref, timeout).await,
            )
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;
    results.sort_by_key(|(idx, _)| *idx);

    let mut reachable = 0usize;
    let mut undetermined = 0usize;
    let mut any_fake_ip = false;
    let mut any_tun_intercepted = false;
    let mut groups: Vec<IpGroup> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();

    for (idx, report) in &results {
        let endpoint = &endpoints[*idx];
        let name = display_name(&endpoint.names);
        let udp_tag = if endpoint.udp_only { " [UDP]" } else { "" };
        let target = describe_target(endpoint, report);

        let line = match &report.outcome {
            ProbeOutcome::Reachable => {
                reachable += 1;
                format!(
                    "  \x1b[32m✅\x1b[0m {}{} — {} ({:.0}ms)",
                    name,
                    udp_tag,
                    target,
                    report.elapsed.as_secs_f64() * 1000.0
                )
            }
            ProbeOutcome::Refused => {
                let hint = if endpoint.udp_only {
                    "拒绝 TCP 连接（IP 可达；UDP 节点仅供参考）"
                } else {
                    "拒绝 TCP 连接（IP 可达，边缘未开放此端口？）"
                };
                format!(
                    "  \x1b[33m⚠️\x1b[0m  {}{} — {} {}",
                    name, udp_tag, target, hint
                )
            }
            ProbeOutcome::TimedOut => {
                let hint = if endpoint.udp_only {
                    "连接超时（UDP 节点，TCP 探测仅供参考）"
                } else {
                    "连接超时（可能被阻断）"
                };
                format!(
                    "  \x1b[31m✗\x1b[0m  {}{} — {} {}",
                    name, udp_tag, target, hint
                )
            }
            ProbeOutcome::TunIntercepted => {
                any_tun_intercepted = true;
                undetermined += 1;
                format!(
                    "  \x1b[33m⚠️\x1b[0m  {}{} — {} 连接被 TUN 接管，结果不可判定",
                    name, udp_tag, target
                )
            }
            ProbeOutcome::FakeIp => {
                any_fake_ip = true;
                undetermined += 1;
                format!("  \x1b[33m⚠️\x1b[0m  {}{} — {}", name, udp_tag, target)
            }
            ProbeOutcome::DnsFailed(e) => {
                format!(
                    "  \x1b[31m✗\x1b[0m  {}{} — {} 解析失败: {}",
                    name, udp_tag, target, e
                )
            }
            ProbeOutcome::Error(e) => {
                format!(
                    "  \x1b[31m✗\x1b[0m  {}{} — {} 连接错误: {}",
                    name, udp_tag, target, e
                )
            }
        };
        println!("{}", line);

        let key = report_group_key(report);
        let group_idx = if let Some(&idx) = group_index.get(&key) {
            idx
        } else {
            groups.push(IpGroup {
                label: key.clone(),
                ..Default::default()
            });
            let idx = groups.len() - 1;
            group_index.insert(key, idx);
            idx
        };
        let group = &mut groups[group_idx];
        group.total += 1;
        if !endpoint.udp_only {
            group.tcp_total += 1;
        }
        match &report.outcome {
            ProbeOutcome::Reachable => group.reachable += 1,
            ProbeOutcome::TimedOut => {
                if !endpoint.udp_only {
                    group.tcp_timed_out += 1;
                }
            }
            _ => group.other += 1,
        }
    }

    if any_tun_intercepted {
        warn(
            "Hints",
            "存在被 TUN 接管的直连 TCP — 去掉 --raw 使用默认的 API 延迟测试，或先 systemctl --user stop mihomo",
        );
    }
    if any_fake_ip {
        warn(
            "Hints",
            "存在 fake-ip 解析 — 系统 DNS 正被 mihomo 劫持，停掉 mihomo 复测可得到真实边缘 IP",
        );
    }
    finish_summary(
        &groups,
        reachable,
        endpoints.len(),
        undetermined,
        "TCP 端点全部超时，该边缘可能被阻断",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_formatting() {
        status(true, "Test", "all good");
        status(false, "Test", "something wrong");
        warn("Test", "a warning");
    }

    #[tokio::test]
    async fn test_check_process_conflict_does_not_panic() {
        check_process_conflict();
    }

    #[test]
    fn de_port_accepts_numbers_and_numeric_strings() {
        let entry: ProxyEntry =
            serde_yaml::from_str("name: n\ntype: anytls\nserver: example.com\nport: 12001\n")
                .unwrap();
        assert_eq!(entry.port, Some(12001));

        let entry: ProxyEntry =
            serde_yaml::from_str("name: n\ntype: anytls\nserver: example.com\nport: \"46817\"\n")
                .unwrap();
        assert_eq!(entry.port, Some(46817));

        let entry: ProxyEntry =
            serde_yaml::from_str("name: n\ntype: ss\nserver: example.com\n").unwrap();
        assert_eq!(entry.port, None);
    }

    #[test]
    fn load_config_entries_skips_incomplete_and_parses_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "tun:\n  enable: true\n  device: utun9\nproxies:\n  - name: HK1\n    type: anytls\n    server: cdn.example.com\n    port: 12001\n  - name: HK1-alt\n    type: anytls\n    server: CDN.example.com\n    port: 12001\n  - name: US-hy2\n    type: hysteria2\n    server: 11us.example.com\n    port: \"46817\"\n  - name: bad-no-port\n    type: ss\n    server: no-port.example.com\nhosts:\n  cdn.example.com: 157.255.67.38\n  multi.example.com: [1.2.3.4, 5.6.7.8]\n",
        )
        .unwrap();

        let loaded = load_config_entries(&path).unwrap();
        assert_eq!(loaded.entries.len(), 3);
        assert_eq!(loaded.skipped, 1);
        assert!(loaded.tun_enabled);
        assert_eq!(loaded.tun_device, "utun9");
        assert_eq!(
            loaded.hosts.exact.get("cdn.example.com").unwrap(),
            &vec!["157.255.67.38".parse::<IpAddr>().unwrap()]
        );
        assert_eq!(
            loaded.hosts.exact.get("multi.example.com").unwrap().len(),
            2
        );

        let endpoints = group_endpoints(&loaded.entries);
        assert_eq!(endpoints.len(), 2);
        let hk = endpoints.iter().find(|e| e.port == 12001).unwrap();
        assert_eq!(hk.names.len(), 2, "host dedup must be case-insensitive");
        assert!(!hk.udp_only);
        let hy2 = endpoints.iter().find(|e| e.port == 46817).unwrap();
        assert!(hy2.udp_only);
    }

    #[test]
    fn tun_defaults_when_section_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(
            &path,
            "proxies:\n  - name: n\n    type: ss\n    server: a.com\n    port: 443\n",
        )
        .unwrap();
        let loaded = load_config_entries(&path).unwrap();
        assert!(!loaded.tun_enabled);
        assert_eq!(loaded.tun_device, "Meta");
    }

    #[test]
    fn parse_delay_response_handles_success_and_failure() {
        let ok = serde_json::json!({"delay": 110});
        assert_eq!(parse_delay_response(true, &ok).unwrap(), 110);

        let fail = serde_json::json!({"message": "An error occurred in the delay test"});
        assert_eq!(
            parse_delay_response(false, &fail).unwrap_err(),
            "An error occurred in the delay test"
        );

        // 2xx without a delay field must still be an error.
        assert!(parse_delay_response(true, &serde_json::json!({})).is_err());
        // Non-2xx without a message falls back to a generic error.
        assert_eq!(
            parse_delay_response(false, &serde_json::json!({})).unwrap_err(),
            "delay test failed"
        );
    }

    #[test]
    fn tun_interception_detection() {
        assert!(is_tun_intercepted(Some(
            "198.18.0.1:33136".parse().unwrap()
        )));
        assert!(is_tun_intercepted(Some("198.19.9.9:1".parse().unwrap())));
        assert!(!is_tun_intercepted(Some(
            "192.168.3.56:1234".parse().unwrap()
        )));
        assert!(!is_tun_intercepted(None));
    }

    #[test]
    fn fake_ip_detection() {
        assert!(is_fake_ip(&"198.18.0.7".parse().unwrap()));
        assert!(is_fake_ip(&"198.19.1.1".parse().unwrap()));
        assert!(!is_fake_ip(&"170.33.12.185".parse().unwrap()));
        assert!(!is_fake_ip(&"::1".parse().unwrap()));
    }

    #[tokio::test]
    async fn probe_reaches_local_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        let endpoint = Endpoint {
            server: "127.0.0.1".into(),
            port,
            names: vec!["local".into()],
            udp_only: false,
        };
        let report = probe_endpoint(
            &endpoint,
            &HostsOverrides::default(),
            None,
            Duration::from_millis(2000),
        )
        .await;
        assert!(matches!(report.outcome, ProbeOutcome::Reachable));
        assert_eq!(report.source, "literal");
    }

    #[tokio::test]
    async fn probe_reports_refused_on_closed_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let endpoint = Endpoint {
            server: "127.0.0.1".into(),
            port,
            names: vec!["closed".into()],
            udp_only: false,
        };
        let report = probe_endpoint(
            &endpoint,
            &HostsOverrides::default(),
            None,
            Duration::from_millis(2000),
        )
        .await;
        assert!(matches!(
            report.outcome,
            ProbeOutcome::Refused | ProbeOutcome::Error(_)
        ));
    }

    #[tokio::test]
    async fn hosts_override_short_circuits_resolution() {
        // TEST-NET-1 address with a short timeout: assert the dial target
        // comes from the hosts override, not from actual reachability.
        let mut hosts = HostsOverrides::default();
        hosts.exact.insert(
            "pinned.example.com".into(),
            vec!["192.0.2.1".parse().unwrap()],
        );
        let endpoint = Endpoint {
            server: "pinned.example.com".into(),
            port: 443,
            names: vec!["pinned".into()],
            udp_only: false,
        };
        let report = probe_endpoint(&endpoint, &hosts, None, Duration::from_millis(50)).await;
        assert_eq!(report.source, "hosts");
        assert_eq!(report.dial_ip, Some("192.0.2.1".parse().unwrap()));
    }
}
