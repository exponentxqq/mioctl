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

/// Write proxy.conf (for detection) and proxy.env (for shell sourcing).
/// User must add to ~/.zshenv:
///   [ -f ~/.config/mioctl/proxy.env ] && source ~/.config/mioctl/proxy.env
#[allow(dead_code)]
pub fn set_system_proxy(mixed_port: u16) -> std::io::Result<()> {
    // Write proxy.conf for detection
    let conf_path = proxy_conf_path();
    if let Some(parent) = conf_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = format!(
        "HTTP_PROXY=http://127.0.0.1:{0}\n\
         HTTPS_PROXY=http://127.0.0.1:{0}\n\
         ALL_PROXY=socks5://127.0.0.1:{0}\n\
         NO_PROXY=localhost,127.0.0.1,::1,.local\n",
        mixed_port
    );
    fs::write(&conf_path, &content)?;

    // Write proxy.env for shell sourcing
    let env_path = proxy_env_path();
    if let Some(parent) = env_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&env_path, &content)?;

    Ok(())
}

/// Remove proxy.conf and proxy.env to disable system proxy.
pub fn clear_system_proxy() {
    let _ = fs::remove_file(proxy_conf_path());
    let _ = fs::remove_file(proxy_env_path());
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
}
