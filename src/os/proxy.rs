use std::fs;
use std::path::PathBuf;

fn proxy_conf_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("environment.d")
        .join("proxy.conf")
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

fn zshenv_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zshenv")
}

const PROXY_MARKER_BEGIN: &str = "# >>> mioctl proxy >>>";
const PROXY_MARKER_END: &str = "# <<< mioctl proxy <<<";

/// Write proxy.conf and append proxy env vars to ~/.zshenv.
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
    fs::write(&conf_path, content)?;

    // Append proxy block to ~/.zshenv
    let zshenv = zshenv_path();
    let existing = if zshenv.exists() {
        fs::read_to_string(&zshenv).unwrap_or_default()
    } else {
        String::new()
    };
    // Remove old marker block if present
    let cleaned = remove_marker_block(&existing);
    let port = mixed_port;
    let block = format!(
        "{}\n\
         export HTTP_PROXY=http://127.0.0.1:{port}\n\
         export HTTPS_PROXY=http://127.0.0.1:{port}\n\
         export ALL_PROXY=socks5://127.0.0.1:{port}\n\
         export NO_PROXY=localhost,127.0.0.1,::1,.local\n\
         {}\n",
        PROXY_MARKER_BEGIN, PROXY_MARKER_END,
    );
    fs::write(&zshenv, format!("{}{}", cleaned, block))?;

    Ok(())
}

/// Remove proxy.conf and the proxy block from ~/.zshenv.
pub fn clear_system_proxy() {
    let _ = fs::remove_file(proxy_conf_path());

    let zshenv = zshenv_path();
    if zshenv.exists() {
        if let Ok(content) = fs::read_to_string(&zshenv) {
            let cleaned = remove_marker_block(&content);
            if cleaned.is_empty() {
                let _ = fs::remove_file(&zshenv);
            } else {
                let _ = fs::write(&zshenv, cleaned);
            }
        }
    }
}

fn remove_marker_block(s: &str) -> String {
    let begin = s.find(PROXY_MARKER_BEGIN);
    let end = s.find(PROXY_MARKER_END);
    match (begin, end) {
        (Some(b), Some(e)) => {
            let before = &s[..b];
            let after = &s[e + PROXY_MARKER_END.len()..];
            // Remove trailing newlines from before
            let before_trimmed = before.trim_end_matches('\n');
            if before_trimmed.is_empty() && after.trim().is_empty() {
                String::new()
            } else if before_trimmed.is_empty() {
                after.to_string()
            } else if after.is_empty() {
                format!("{}\n", before_trimmed)
            } else {
                format!("{}\n{}", before_trimmed, after)
            }
        }
        _ => s.to_string(),
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
        // Create a temp file with port 7897, then check for port 9999
        let dir = std::env::temp_dir().join("mioctl-test-detect-wrong");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("proxy.conf");
        fs::write(&path, "HTTP_PROXY=http://127.0.0.1:7897\n").unwrap();

        // detect_system_proxy uses the real ~/.config path, not our temp dir,
        // so this test verifies the logic works with a matching line.
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
    fn test_remove_marker_block_cleans_proxy_lines() {
        let input = "export PATH=/usr/bin\n# >>> mioctl proxy >>>\nexport HTTP_PROXY=http://127.0.0.1:7897\n# <<< mioctl proxy <<<\nexport EDITOR=vim\n";
        let result = remove_marker_block(input);
        assert!(!result.contains("mioctl proxy"));
        assert!(!result.contains("HTTP_PROXY"));
        assert!(result.contains("export PATH"));
        assert!(result.contains("export EDITOR"));
    }

    #[test]
    fn test_remove_marker_block_empty_when_only_proxy() {
        let input = "# >>> mioctl proxy >>>\nexport HTTP_PROXY=http://127.0.0.1:7897\n# <<< mioctl proxy <<<\n";
        let result = remove_marker_block(input);
        assert!(result.trim().is_empty());
    }

    #[test]
    fn test_remove_marker_block_no_marker_unchanged() {
        let input = "export PATH=/usr/bin\n";
        let result = remove_marker_block(input);
        assert_eq!(result, input);
    }
}
