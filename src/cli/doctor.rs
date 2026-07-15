use std::process::Command;

#[derive(clap::Subcommand)]
pub enum DoctorAction {
    /// Run diagnostic checks
    Run,
}

pub async fn run(_action: DoctorAction) {
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

fn check_config_syntax(_config: &crate::config::mioctl_config::MioctlConfig) {
    let home = std::env::var("HOME").unwrap_or_default();
    let config_path = std::path::Path::new(&home).join(".config/mihomo/config.yaml");

    if !config_path.exists() {
        status(
            false,
            "Config syntax",
            &format!("config not found at {}", config_path.display()),
        );
        return;
    }

    let output = Command::new("mihomo")
        .args(["-t", "-f"])
        .arg(config_path.to_str().unwrap())
        .arg("-d")
        .arg(config_path.parent().unwrap().to_str().unwrap())
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
}
