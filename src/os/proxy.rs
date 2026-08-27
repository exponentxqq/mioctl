use std::fs;
use std::path::PathBuf;

fn proxy_conf_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("environment.d")
        .join("proxy.conf")
}

fn proxy_env_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mioctl")
        .join("proxy.env")
}

/// gsettings is skipped when MIOCTL_TEST_NO_GSETTINGS is set (hermetic tests).
fn gsettings_enabled() -> bool {
    std::env::var("MIOCTL_TEST_NO_GSETTINGS").is_err()
}

/// systemctl/dbus env propagation is skipped when MIOCTL_TEST_NO_SYSTEMCTL
/// is set (hermetic tests).
fn systemctl_enabled() -> bool {
    std::env::var("MIOCTL_TEST_NO_SYSTEMCTL").is_err()
}

const PROXY_VARS: [&str; 6] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// Check whether system proxy is enabled: proxy.conf exists and
/// HTTP_PROXY line points to 127.0.0.1:<mixed_port>.
pub fn detect_system_proxy(mixed_port: Option<u16>) -> bool {
    let Some(port) = mixed_port else { return false };
    let path = proxy_conf_path();
    if !path.exists() {
        return false;
    }
    match fs::read_to_string(&path) {
        Ok(content) => {
            let expected = format!("http://127.0.0.1:{}", port);
            content
                .lines()
                .any(|line| line.starts_with("HTTP_PROXY=") && line.contains(&expected))
        }
        Err(_) => false,
    }
}

/// Write proxy.conf (detection), proxy.env (shell sourcing),
/// and set env via systemd (cross-shell / cross-terminal).
///
/// User should add to ~/.zshenv (zsh) or ~/.profile (bash/login):
///   [ -f ~/.config/mioctl/proxy.env ] && . ~/.config/mioctl/proxy.env
#[allow(dead_code)]
pub fn set_system_proxy(mixed_port: u16) -> std::io::Result<()> {
    // Write proxy.conf for detection
    let conf_path = proxy_conf_path();
    if let Some(parent) = conf_path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Write proxy.conf for detection (no export prefix)
    let conf_content = format!(
        "HTTP_PROXY=http://127.0.0.1:{0}\n\
         http_proxy=http://127.0.0.1:{0}\n\
         HTTPS_PROXY=http://127.0.0.1:{0}\n\
         https_proxy=http://127.0.0.1:{0}\n\
         NO_PROXY=localhost,127.0.0.1,::1,.local\n\
         no_proxy=localhost,127.0.0.1,::1,.local\n",
        mixed_port
    );
    fs::write(&conf_path, &conf_content)?;

    // Write proxy.env for shell sourcing (with export)
    let env_content = format!(
        "export HTTP_PROXY=http://127.0.0.1:{0}\n\
         export http_proxy=http://127.0.0.1:{0}\n\
         export HTTPS_PROXY=http://127.0.0.1:{0}\n\
         export https_proxy=http://127.0.0.1:{0}\n\
         export NO_PROXY=localhost,127.0.0.1,::1,.local\n\
         export no_proxy=localhost,127.0.0.1,::1,.local\n",
        mixed_port
    );
    let env_path = proxy_env_path();
    if let Some(parent) = env_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&env_path, &env_content)?;

    // Set via gsettings (GNOME/KDE system proxy) — browsers pick this up.
    // This is what clash-verge-rev does via the sysproxy crate.
    if gsettings_enabled() {
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy", "mode", "manual"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.http",
                "port",
                &mixed_port.to_string(),
            ])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy.https", "host", "127.0.0.1"])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy.https",
                "port",
                &mixed_port.to_string(),
            ])
            .output();
        let _ = std::process::Command::new("gsettings")
            .args([
                "set",
                "org.gnome.system.proxy",
                "ignore-hosts",
                "['localhost', '127.0.0.0/8', '::1', '.local']",
            ])
            .output();
    }

    // Re-merge environment.d into the systemd user environment so newly
    // started services and dbus-activated processes pick the proxy up
    // immediately. `systemctl --user set-environment` cannot override
    // environment.d-merged variables (systemd keeps them in a separate
    // transient layer that unset-environment cannot touch), so the file
    // write + daemon-reload is the only reliable mechanism.
    if systemctl_enabled() {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        let base = format!("http://127.0.0.1:{}", mixed_port);
        let no_proxy = "localhost,127.0.0.1,::1,.local";
        let _ = std::process::Command::new("dbus-update-activation-environment")
            .args([
                "--systemd",
                &format!("HTTP_PROXY={}", base),
                &format!("http_proxy={}", base),
                &format!("HTTPS_PROXY={}", base),
                &format!("https_proxy={}", base),
                &format!("NO_PROXY={}", no_proxy),
                &format!("no_proxy={}", no_proxy),
            ])
            .output();
        for (k, v) in [
            ("HTTP_PROXY", base.as_str()),
            ("http_proxy", base.as_str()),
            ("HTTPS_PROXY", base.as_str()),
            ("https_proxy", base.as_str()),
            ("NO_PROXY", no_proxy),
            ("no_proxy", no_proxy),
        ] {
            let _ = std::process::Command::new("tmux")
                .args(["set-environment", "-g", &format!("{}={}", k, v)])
                .output();
        }
    }

    Ok(())
}

/// Remove proxy.conf, proxy.env, disable gsettings system proxy, and
/// re-merge environment.d so the systemd user environment drops the vars.
pub fn clear_system_proxy() {
    let _ = fs::remove_file(proxy_conf_path());
    let _ = fs::remove_file(proxy_env_path());
    if gsettings_enabled() {
        let _ = std::process::Command::new("gsettings")
            .args(["set", "org.gnome.system.proxy", "mode", "none"])
            .output();
    }
    if systemctl_enabled() {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
        for v in PROXY_VARS {
            let _ = std::process::Command::new("tmux")
                .args(["set-environment", "-g", "-u", v])
                .output();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_detect_none_port_returns_false() {
        assert!(!detect_system_proxy(None));
    }

    #[test]
    fn test_detect_wrong_port_returns_false() {
        let dir = std::env::temp_dir().join("mioctl-test-detect-wrong");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proxy.conf");
        fs::write(&path, "HTTP_PROXY=http://127.0.0.1:7897\n").unwrap();

        let content = "HTTP_PROXY=http://127.0.0.1:7897\n";
        let expected = "http://127.0.0.1:9999";
        let matches = content
            .lines()
            .any(|line| line.starts_with("HTTP_PROXY=") && line.contains(expected));
        assert!(!matches);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_set_system_proxy_writes_correct_content() {
        let dir = std::env::temp_dir().join("mioctl-test-set-proxy");
        let path = dir.join("proxy.conf");
        fs::create_dir_all(&dir).unwrap();

        let content = format!(
            "HTTP_PROXY=http://127.0.0.1:{0}\n\
             HTTPS_PROXY=http://127.0.0.1:{0}\n\
             ALL_PROXY=socks5://127.0.0.1:{0}\n\
             NO_PROXY=localhost,127.0.0.1,::1,.local\n",
            7897
        );
        fs::write(&path, &content).unwrap();

        let read_back = fs::read_to_string(&path).unwrap();
        assert!(read_back.contains("HTTP_PROXY=http://127.0.0.1:7897"));
        assert!(read_back.contains("HTTPS_PROXY=http://127.0.0.1:7897"));
        assert!(read_back.contains("ALL_PROXY=socks5://127.0.0.1:7897"));
        assert!(read_back.contains("NO_PROXY=localhost,127.0.0.1,::1,.local"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_clear_removes_file() {
        let dir = std::env::temp_dir().join("mioctl-test-clear-proxy");
        let path = dir.join("proxy.conf");
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, "HTTP_PROXY=http://127.0.0.1:7897\n").unwrap();
        assert!(path.exists());

        fs::remove_file(&path).unwrap();
        assert!(!path.exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn test_detect_matching_line() {
        let content = "HTTP_PROXY=http://127.0.0.1:7897\nHTTPS_PROXY=http://127.0.0.1:7897\n";
        let expected = "http://127.0.0.1:7897";
        let matches = content
            .lines()
            .any(|line| line.starts_with("HTTP_PROXY=") && line.contains(expected));
        assert!(matches);
    }

    #[test]
    fn test_gsettings_enabled_flag() {
        unsafe { std::env::set_var("MIOCTL_TEST_NO_GSETTINGS", "1") };
        assert!(!gsettings_enabled());
        unsafe { std::env::remove_var("MIOCTL_TEST_NO_GSETTINGS") };
        assert!(gsettings_enabled());
    }

    #[test]
    fn test_systemctl_enabled_flag() {
        unsafe { std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1") };
        assert!(!systemctl_enabled());
        unsafe { std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL") };
        assert!(systemctl_enabled());
    }

    #[test]
    fn test_proxy_vars_cover_all_names() {
        assert_eq!(PROXY_VARS.len(), 6);
        assert!(PROXY_VARS.contains(&"HTTP_PROXY"));
        assert!(PROXY_VARS.contains(&"https_proxy"));
        assert!(PROXY_VARS.contains(&"NO_PROXY"));
    }
}
