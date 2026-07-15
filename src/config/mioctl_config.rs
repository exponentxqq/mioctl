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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscriptions {
    #[serde(default = "default_update_interval")]
    pub update_interval_minutes: u64,
    #[serde(default)]
    pub items: Vec<SubscriptionItem>,
}

fn default_update_interval() -> u64 {
    240
}

impl Default for Subscriptions {
    fn default() -> Self {
        Self {
            update_interval_minutes: 240,
            items: vec![],
        }
    }
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
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mioctl")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn providers_dir() -> PathBuf {
        Self::config_dir().join("providers")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => toml::from_str(&content).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            let config = Self::default();
            let _ = config.save();
            config
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(Self::providers_dir()).map_err(|e| e.to_string())?;
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::config_path(), content).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_subscription(&mut self, name: String, url: String) {
        self.subscriptions.items.push(SubscriptionItem {
            name,
            url,
            last_updated: None,
        });
    }

    pub fn remove_subscription(&mut self, name: &str) -> bool {
        let len_before = self.subscriptions.items.len();
        self.subscriptions.items.retain(|s| s.name != name);
        self.subscriptions.items.len() < len_before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MioctlConfig::default();
        assert_eq!(config.mihomo.external_controller, "127.0.0.1:9090");
        assert_eq!(config.mihomo.secret, "");
        assert_eq!(config.subscriptions.update_interval_minutes, 240);
        assert!(config.subscriptions.items.is_empty());
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
    fn test_default_app_log_level() {
        assert_eq!(Preferences::default().app_log_level, "info");
    }

    #[test]
    fn test_app_log_level_can_change() {
        let mut prefs = Preferences::default();
        prefs.app_log_level = "error".into();
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
