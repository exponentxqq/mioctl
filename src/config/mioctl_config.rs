use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MihomoConnection {
    #[serde(default = "default_host")]
    pub external_controller: String,
    #[serde(default)]
    pub secret: String,
    #[serde(default = "default_config_path")]
    pub config_path: String,
}

fn default_host() -> String {
    "127.0.0.1:9090".into()
}
fn default_config_path() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("mihomo")
        .join("config.yaml")
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionItem {
    pub name: String,
    pub url: String,
    pub last_updated: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Subscriptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub items: Vec<SubscriptionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default = "default_delay_url")]
    pub delay_test_url: String,
    #[serde(default = "default_delay_timeout")]
    pub delay_test_timeout_ms: u64,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_app_log_level")]
    pub app_log_level: String,
}

fn default_delay_url() -> String {
    "https://www.gstatic.com/generate_204".into()
}
fn default_delay_timeout() -> u64 {
    5000
}
fn default_theme() -> String {
    "catppuccin-mocha".into()
}
fn default_app_log_level() -> String {
    "info".into()
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            delay_test_url: default_delay_url(),
            delay_test_timeout_ms: default_delay_timeout(),
            theme: default_theme(),
            app_log_level: default_app_log_level(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MioctlConfig {
    #[serde(default)]
    pub mihomo: MihomoConnection,
    #[serde(default)]
    pub subscriptions: Subscriptions,
    #[serde(default)]
    pub preferences: Preferences,
}

impl Default for MioctlConfig {
    fn default() -> Self {
        Self {
            mihomo: MihomoConnection {
                external_controller: default_host(),
                secret: String::new(),
                config_path: default_config_path(),
            },
            subscriptions: Subscriptions::default(),
            preferences: Preferences::default(),
        }
    }
}

#[allow(dead_code)]
impl MioctlConfig {
    pub fn config_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("MIOCTL_HOME") {
            if !dir.is_empty() {
                return PathBuf::from(dir);
            }
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mioctl")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn profiles_dir() -> PathBuf {
        Self::config_dir().join("profiles")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => config,
                    Err(_) => Self::recover_corrupt(&path),
                },
                Err(e) if read_error_is_transient(&e) => Self::default(),
                Err(_) => Self::recover_corrupt(&path),
            }
        } else {
            let config = Self::default();
            let _ = config.save();
            config
        }
    }

    fn recover_corrupt(path: &std::path::Path) -> Self {
        let corrupt = path.with_extension("toml.corrupt");
        let _ = std::fs::rename(path, corrupt);
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let path = Self::config_path();
        let tmp = dir.join("config.toml.tmp");
        std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }

    pub fn add_subscription(&mut self, name: String, url: String) {
        self.subscriptions.items.push(SubscriptionItem {
            name,
            url,
            last_updated: None,
            node_count: None,
        });
    }

    pub fn set_active(&mut self, name: Option<&str>) {
        self.subscriptions.active = name.map(|s| s.to_string());
    }

    pub fn find_subscription(&self, name: &str) -> Option<&SubscriptionItem> {
        self.subscriptions.items.iter().find(|s| s.name == name)
    }

    pub fn remove_subscription(&mut self, name: &str) -> bool {
        let len_before = self.subscriptions.items.len();
        self.subscriptions.items.retain(|s| s.name != name);
        self.subscriptions.items.len() < len_before
    }
}

fn read_error_is_transient(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MioctlConfig::default();
        assert_eq!(config.mihomo.external_controller, "127.0.0.1:9090");
        assert_eq!(config.mihomo.secret, "");
        assert!(config.subscriptions.items.is_empty());
        assert_eq!(config.subscriptions.active, None);
        assert_eq!(
            config.preferences.delay_test_url,
            "https://www.gstatic.com/generate_204"
        );
    }

    #[test]
    fn test_add_remove_subscription() {
        let mut config = MioctlConfig::default();
        config.add_subscription("test-sub".into(), "https://example.com/sub".into());
        assert_eq!(config.subscriptions.items.len(), 1);
        assert_eq!(config.subscriptions.items[0].name, "test-sub");
        assert!(config.remove_subscription("test-sub"));
        assert!(config.subscriptions.items.is_empty());
        assert!(!config.remove_subscription("nonexistent"));
    }

    #[test]
    fn test_active_and_node_count_roundtrip() {
        let mut config = MioctlConfig::default();
        config.add_subscription("my-sub".into(), "https://example.com/sub".into());
        config.subscriptions.items[0].node_count = Some(42);
        config.set_active(Some("my-sub"));
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.subscriptions.active.as_deref(), Some("my-sub"));
        assert_eq!(deserialized.subscriptions.items[0].node_count, Some(42));
    }

    #[test]
    fn test_legacy_config_without_new_fields_loads() {
        let legacy = r#"
[mihomo]
external_controller = "127.0.0.1:9090"
secret = ""
config_path = "/tmp/x.yaml"

[[subscriptions.items]]
name = "old"
url = "https://example.com/sub"
last_updated = "2026-01-01T00:00:00Z"
"#;
        let config: MioctlConfig = toml::from_str(legacy).unwrap();
        assert_eq!(config.subscriptions.items.len(), 1);
        assert_eq!(config.subscriptions.active, None);
        assert_eq!(config.subscriptions.items[0].node_count, None);
    }

    #[test]
    fn test_find_subscription() {
        let mut config = MioctlConfig::default();
        config.add_subscription("a".into(), "https://a".into());
        assert!(config.find_subscription("a").is_some());
        assert!(config.find_subscription("b").is_none());
    }

    #[test]
    fn test_config_dir_uses_mioctl_home() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", "/tmp/mioctl-test") };
        assert_eq!(
            MioctlConfig::config_dir(),
            PathBuf::from("/tmp/mioctl-test")
        );
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_config_dir_ignores_empty_mioctl_home() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", "") };
        assert_eq!(
            MioctlConfig::config_dir(),
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("mioctl")
        );
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_profiles_dir_and_set_active() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", "/tmp/mioctl-test") };
        let mut config = MioctlConfig::default();
        assert_eq!(
            MioctlConfig::profiles_dir(),
            PathBuf::from("/tmp/mioctl-test/profiles")
        );
        config.set_active(Some("profile"));
        assert_eq!(config.subscriptions.active.as_deref(), Some("profile"));
        config.set_active(None);
        assert_eq!(config.subscriptions.active, None);
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_toml_roundtrip() {
        let mut config = MioctlConfig::default();
        config.add_subscription("my-sub".into(), "https://example.com/sub".into());
        config.preferences.delay_test_url = "http://localhost/test".into();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.subscriptions.items.len(), 1);
        assert_eq!(deserialized.subscriptions.items[0].name, "my-sub");
        assert_eq!(
            deserialized.preferences.delay_test_url,
            "http://localhost/test"
        );
    }

    #[test]
    fn test_load_corrupt_config_preserves_original_as_corrupt() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
        let broken = "not [ valid toml {{{";
        assert!(toml::from_str::<MioctlConfig>(broken).is_err());
        std::fs::write(dir.path().join("config.toml"), broken).unwrap();

        let config = MioctlConfig::load();

        assert!(config.subscriptions.items.is_empty());
        assert!(!dir.path().join("config.toml").exists());
        let preserved = dir.path().join("config.toml.corrupt");
        assert_eq!(std::fs::read_to_string(preserved).unwrap(), broken);
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_load_unreadable_config_preserves_original_as_corrupt() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
        let bytes = vec![0xff, 0xfe, 0x00];
        std::fs::write(dir.path().join("config.toml"), &bytes).unwrap();

        let config = MioctlConfig::load();

        assert!(config.subscriptions.items.is_empty());
        assert!(!dir.path().join("config.toml").exists());
        let preserved = dir.path().join("config.toml.corrupt");
        assert_eq!(std::fs::read(preserved).unwrap(), bytes);
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_save_atomic_leaves_no_tmp() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
        let config = MioctlConfig::default();
        config.save().unwrap();
        assert!(dir.path().join("config.toml").exists());
        assert!(!dir.path().join("config.toml.tmp").exists());
        let reloaded = MioctlConfig::load();
        assert!(reloaded.subscriptions.items.is_empty());
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_read_error_kind_classification() {
        assert!(read_error_is_transient(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        assert!(read_error_is_transient(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(!read_error_is_transient(&std::io::Error::from(
            std::io::ErrorKind::InvalidData
        )));
        assert!(!read_error_is_transient(&std::io::Error::from(
            std::io::ErrorKind::Other
        )));
    }

    #[test]
    fn test_load_config_path_is_directory_recovers_to_default() {
        let _guard = crate::testutil::env_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
        std::fs::create_dir(dir.path().join("config.toml")).unwrap();

        let config = MioctlConfig::load();

        assert!(config.subscriptions.items.is_empty());
        if dir.path().join("config.toml.corrupt").exists() {
            assert!(
                !dir.path().join("config.toml").exists(),
                "rename should have moved the directory away"
            );
        } else {
            assert!(
                dir.path().join("config.toml").exists(),
                "original must be untouched when rename of a directory fails"
            );
        }
        unsafe { std::env::remove_var("MIOCTL_HOME") };
    }

    #[test]
    fn test_default_app_log_level() {
        assert_eq!(Preferences::default().app_log_level, "info");
    }

    #[test]
    fn test_app_log_level_can_change() {
        let prefs = Preferences {
            app_log_level: "error".into(),
            ..Default::default()
        };
        assert_eq!(prefs.app_log_level, "error");
    }

    #[test]
    fn test_app_log_level_roundtrip() {
        let mut config = MioctlConfig::default();
        config.preferences.app_log_level = "debug".into();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: MioctlConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.preferences.app_log_level, "debug");
    }
}
