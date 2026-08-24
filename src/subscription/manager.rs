use crate::api::client::MihomoClient;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::fetcher::fetch_with_ua_probe;
use crate::subscription::merger::{
    backup_file, discard_backup, merge_mihomo_config, rollback_file, write_config,
};
use crate::subscription::parser::{detect_subscription_name, name_from_url};
use crate::subscription::profile::{
    archive_exists, name_conflicts, normalize_to_yaml, read_archive, remove_archive, write_archive,
};
use serde_yaml::Value;

pub enum UpdateTarget {
    Active,
    Named(String),
    All,
}

#[derive(Debug)]
pub struct UpdateReport {
    pub lines: Vec<String>,
    pub failed: bool,
}

pub struct SubscriptionManager;

fn client_for(config: &MioctlConfig) -> Result<MihomoClient, String> {
    let secret = if config.mihomo.secret.is_empty() {
        None
    } else {
        Some(config.mihomo.secret.clone())
    };
    MihomoClient::new(&config.mihomo.external_controller, secret)
        .map_err(|e| format!("could not connect to mihomo: {}", e))
}

fn unique_name(base: &str, existing: &[String]) -> Result<String, String> {
    if !name_conflicts(base, existing) {
        return Ok(base.to_string());
    }
    let stem: String = base.chars().take(70).collect();
    for i in 2..=200 {
        let candidate = format!("{} ({})", stem, i);
        if !name_conflicts(&candidate, existing) {
            return Ok(candidate);
        }
    }
    Err(format!(
        "could not derive a unique name from '{}' after 199 attempts",
        base
    ))
}

async fn fetch_version(config: &MioctlConfig) -> Option<String> {
    match client_for(config) {
        Ok(c) => c.get_version().await.ok().map(|v| v.version),
        Err(_) => None,
    }
}

async fn reload_mihomo(config: &MioctlConfig) -> String {
    let base = match client_for(config) {
        Ok(c) => match c.reload_config(None).await {
            Ok(_) => return "mihomo reloaded successfully".into(),
            Err(e) => format!(
                "mihomo API reload failed: {}. Trying systemctl fallback...",
                e
            ),
        },
        Err(e) => format!(
            "mihomo API client unavailable: {}. Trying systemctl fallback...",
            e
        ),
    };
    if std::env::var_os("MIOCTL_TEST_NO_SYSTEMCTL").is_some() {
        return format!(
            "{} systemctl restart also failed. Run: systemctl --user restart mihomo",
            base
        );
    }
    let status = tokio::task::spawn_blocking(|| {
        std::process::Command::new("systemctl")
            .args(["--user", "restart", "mihomo"])
            .output()
    })
    .await;
    match status {
        Ok(Ok(o)) if o.status.success() => format!("{} systemctl restart succeeded.", base),
        _ => format!(
            "{} systemctl restart also failed. Run: systemctl --user restart mihomo",
            base
        ),
    }
}

async fn activate(config: &MioctlConfig, name: &str, no_reload: bool) -> Result<String, String> {
    let archive = read_archive(name).map_err(|_| {
        format!(
            "profile archive for '{}' is missing or unreadable — run `mioctl sub update {}` first",
            name, name
        )
    })?;
    let sub = crate::subscription::parser::parse_subscription_full(&archive).map_err(|e| {
        format!(
            "profile archive for '{}' could not be parsed ({}) — run `mioctl sub update {}` first",
            name, e, name
        )
    })?;

    let config_path = config.mihomo.config_path.clone();
    backup_file(&config_path)?;
    match merge_mihomo_config(&config_path, &sub.proxies, &sub.proxy_groups, &sub.rules) {
        Ok(r) => {
            if let Err(e) = write_config(&config_path, &r.yaml) {
                rollback_file(&config_path).ok();
                return Err(format!("failed to write config: {}", e));
            }
            discard_backup(&config_path);
            let reload_msg = if no_reload {
                "reload skipped".to_string()
            } else {
                reload_mihomo(config).await
            };
            Ok(format!(
                "Switched to '{}'.\n  {} proxies, {} groups, {} rules\n  {}",
                name, r.proxy_count, r.group_count, r.rule_count, reload_msg
            ))
        }
        Err(e) => {
            rollback_file(&config_path).ok();
            Err(e)
        }
    }
}

fn write_empty_state(config: &MioctlConfig) -> Result<(), String> {
    let config_path = config.mihomo.config_path.clone();
    let proxies = Value::Sequence(vec![]);
    let groups = Value::Sequence(vec![]);
    let rules = Value::Sequence(vec![Value::String("MATCH,DIRECT".into())]);
    backup_file(&config_path)?;
    let result = match merge_mihomo_config(&config_path, &proxies, &groups, &rules) {
        Ok(result) => result,
        Err(e) => {
            rollback_file(&config_path).ok();
            return Err(e);
        }
    };
    if let Err(e) = write_config(&config_path, &result.yaml) {
        rollback_file(&config_path).ok();
        return Err(format!("failed to write config: {}", e));
    }
    discard_backup(&config_path);
    Ok(())
}

impl SubscriptionManager {
    pub async fn add(
        config: &mut MioctlConfig,
        url: &str,
        name: Option<String>,
        no_reload: bool,
        activate_flag: bool,
    ) -> Result<String, String> {
        let version = fetch_version(config).await;
        let content = fetch_with_ua_probe(url, version).await?;

        let existing: Vec<String> = config
            .subscriptions
            .items
            .iter()
            .map(|s| s.name.clone())
            .collect();

        let final_name = match name {
            Some(n) => {
                if n.contains(',') {
                    return Err(format!(
                        "subscription name '{}' must not contain commas — they break the generated MATCH rule and group name",
                        n
                    ));
                }
                if name_conflicts(&n, &existing) {
                    return Err(format!(
                        "subscription '{}' already exists. Remove it first or use a different --name.",
                        n
                    ));
                }
                n
            }
            None => {
                let base = detect_subscription_name(&content).or_else(|_| name_from_url(url))?;
                unique_name(&base, &existing)?
            }
        };

        let normalized = normalize_to_yaml(&final_name, &content)?;
        if normalized.node_count == 0 {
            return Err("no proxies found in subscription".into());
        }
        write_archive(&final_name, &normalized.yaml)?;

        let is_first = config.subscriptions.items.is_empty();
        config.add_subscription(final_name.clone(), url.to_string());
        if let Some(item) = config
            .subscriptions
            .items
            .iter_mut()
            .find(|s| s.name == final_name)
        {
            item.node_count = Some(normalized.node_count);
            item.last_updated = Some(chrono::Utc::now().to_rfc3339());
        }

        let mut summary = format!(
            "Subscription '{}' added. {} proxies archived.",
            final_name, normalized.node_count
        );
        for w in &normalized.warnings {
            summary.push_str(&format!("\n  warning: {}", w));
        }

        if is_first || activate_flag {
            config.set_active(Some(&final_name));
            let msg = activate(config, &final_name, no_reload).await?;
            summary.push_str(&format!("\n  {}", msg));
            summary.push_str("\n  (activated)");
        } else {
            summary.push_str("\n  (not active — run `mioctl sub use` to switch)");
        }

        config
            .save()
            .map_err(|e| format!("subscription archived but config save failed: {}", e))?;
        Ok(summary)
    }

    pub async fn use_profile(
        config: &mut MioctlConfig,
        name: &str,
        no_reload: bool,
    ) -> Result<String, String> {
        if config.find_subscription(name).is_none() {
            return Err(format!("no subscription named '{}'", name));
        }
        let msg = activate(config, name, no_reload).await?;
        config.set_active(Some(name));
        config
            .save()
            .map_err(|e| format!("activated but config save failed: {}", e))?;
        Ok(msg)
    }

    pub async fn update(
        config: &mut MioctlConfig,
        target: &UpdateTarget,
    ) -> Result<UpdateReport, String> {
        let names: Vec<String> = match target {
            UpdateTarget::All => config
                .subscriptions
                .items
                .iter()
                .map(|s| s.name.clone())
                .collect(),
            UpdateTarget::Named(n) => {
                if config.find_subscription(n).is_none() {
                    return Err(format!("no subscription named '{}'", n));
                }
                vec![n.clone()]
            }
            UpdateTarget::Active => match config.subscriptions.active.clone() {
                Some(a) => vec![a],
                None => return Err("no active subscription — specify a name or use --all".into()),
            },
        };

        let version = fetch_version(config).await;
        let now = chrono::Utc::now().to_rfc3339();
        let mut results = Vec::new();
        let mut need_save = false;
        let mut failed = false;

        for name in names {
            let url = config
                .find_subscription(&name)
                .map(|s| s.url.clone())
                .unwrap_or_default();
            let fetch_result = fetch_with_ua_probe(&url, version.clone()).await;
            match fetch_result {
                Ok(content) => match normalize_to_yaml(&name, &content) {
                    Ok(normalized) => {
                        if let Err(e) = write_archive(&name, &normalized.yaml) {
                            results.push(format!("{}: ERROR - {}", name, e));
                            failed = true;
                            continue;
                        }
                        if let Some(item) = config
                            .subscriptions
                            .items
                            .iter_mut()
                            .find(|s| s.name == name)
                        {
                            item.node_count = Some(normalized.node_count);
                            item.last_updated = Some(now.clone());
                        }
                        need_save = true;
                        let mut line = format!("{}: {} nodes updated", name, normalized.node_count);
                        for w in &normalized.warnings {
                            line.push_str(&format!(" (warning: {})", w));
                        }
                        results.push(line);

                        if config.subscriptions.active.as_deref() == Some(name.as_str()) {
                            match activate(config, &name, false).await {
                                Ok(_) => {
                                    results.push(format!("{}: re-merged into mihomo config", name))
                                }
                                Err(e) => {
                                    results
                                        .push(format!("{}: ERROR - re-merge failed - {}", name, e));
                                    failed = true;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        results.push(format!("{}: ERROR - {}", name, e));
                        failed = true;
                    }
                },
                Err(e) => {
                    results.push(format!("{}: ERROR - {}", name, e));
                    failed = true;
                }
            }
        }

        if need_save {
            if let Err(e) = config.save() {
                results.push(format!("config: ERROR - save failed: {}", e));
                failed = true;
            }
        }
        Ok(UpdateReport {
            lines: results,
            failed,
        })
    }

    pub async fn remove(config: &mut MioctlConfig, name: &str) -> Result<String, String> {
        if config.find_subscription(name).is_none() {
            return Err(format!("no subscription named '{}'", name));
        }
        let was_active = config.subscriptions.active.as_deref() == Some(name);
        let mut msg = format!("Subscription '{}' removed.", name);
        if was_active {
            config.set_active(None);
            if let Err(e) = write_empty_state(config) {
                config.set_active(Some(name));
                return Err(e);
            }
            msg.push_str("\n  active subscription was removed — mihomo config reset to empty state (MATCH,DIRECT)");
            msg.push_str(&format!("\n  {}", reload_mihomo(config).await));
        }
        if let Err(e) = remove_archive(name) {
            if was_active {
                config.set_active(Some(name));
            }
            return Err(e);
        }
        config.remove_subscription(name);
        config
            .save()
            .map_err(|e| format!("removed but config save failed: {}", e))?;
        Ok(msg)
    }

    pub fn list(config: &MioctlConfig) -> String {
        if config.subscriptions.items.is_empty() {
            return "No subscriptions. Add one: mioctl sub add <url>".into();
        }
        let mut out = String::from("  NAME                 NODES  LAST UPDATED\n");
        for item in &config.subscriptions.items {
            let mark = if config.subscriptions.active.as_deref() == Some(item.name.as_str()) {
                "*"
            } else {
                " "
            };
            let updated = item.last_updated.as_deref().unwrap_or("(never)");
            out.push_str(&format!(
                "{} {:20} {:>5}  {}\n",
                mark,
                item.name,
                item.node_count.unwrap_or(0),
                updated
            ));
        }
        out
    }

    pub async fn ensure_archived(config: &mut MioctlConfig) -> Vec<String> {
        let mut warnings = Vec::new();
        let legacy_providers = MioctlConfig::config_dir().join("providers");
        if legacy_providers.exists() {
            let _ = std::fs::remove_dir_all(&legacy_providers);
            warnings.push("removed legacy providers/ directory".into());
        }

        let missing: Vec<(String, String)> = config
            .subscriptions
            .items
            .iter()
            .filter(|s| !archive_exists(&s.name))
            .map(|s| (s.name.clone(), s.url.clone()))
            .collect();
        if missing.is_empty() {
            return warnings;
        }

        let version = fetch_version(config).await;
        for (name, url) in missing {
            match fetch_with_ua_probe(&url, version.clone()).await {
                Ok(content) => match normalize_to_yaml(&name, &content) {
                    Ok(normalized) => {
                        let _ = write_archive(&name, &normalized.yaml);
                        if let Some(item) = config
                            .subscriptions
                            .items
                            .iter_mut()
                            .find(|s| s.name == name)
                        {
                            item.node_count = Some(normalized.node_count);
                        }
                        warnings.push(format!(
                            "archived profile '{}' ({} nodes)",
                            name, normalized.node_count
                        ));
                    }
                    Err(e) => warnings.push(format!(
                        "profile '{}' has no archive and could not be normalized: {} — run `mioctl sub update {}`",
                        name, e, name
                    )),
                },
                Err(e) => warnings.push(format!(
                    "profile '{}' has no archive and fetch failed: {} — run `mioctl sub update {}`",
                    name, e, name
                )),
            }
        }
        if let Err(e) = config.save() {
            warnings.push(format!("config save failed: {}", e));
        }
        warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEnv {
        _dir: tempfile::TempDir,
        _guard: std::sync::MutexGuard<'static, ()>,
    }

    impl TestEnv {
        fn new(mihomo_yaml: &str) -> (Self, MioctlConfig) {
            let guard = crate::testutil::env_lock().lock().unwrap();
            let dir = tempfile::tempdir().unwrap();
            unsafe { std::env::set_var("MIOCTL_HOME", dir.path()) };
            unsafe { std::env::set_var("MIOCTL_TEST_NO_SYSTEMCTL", "1") };
            let mihomo_path = dir.path().join("mihomo-config.yaml");
            std::fs::write(&mihomo_path, mihomo_yaml).unwrap();
            let mut config = MioctlConfig::default();
            config.mihomo.config_path = mihomo_path.to_string_lossy().into_owned();
            (
                (TestEnv {
                    _dir: dir,
                    _guard: guard,
                }),
                config,
            )
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            unsafe {
                std::env::remove_var("MIOCTL_HOME");
                std::env::remove_var("MIOCTL_TEST_NO_SYSTEMCTL");
            }
        }
    }

    const SUB_YAML: &str = "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: G\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,G\n";

    #[test]
    fn test_unique_name_appends_suffix() {
        let mut names = vec!["base".to_string()];
        assert_eq!(unique_name("base", &names).unwrap(), "base (2)");
        names.push("base (2)".to_string());
        assert_eq!(unique_name("base", &names).unwrap(), "base (3)");
        assert_eq!(unique_name("other", &names).unwrap(), "other");
    }

    #[test]
    fn test_unique_name_long_base_conflict_yields_distinct_short_name() {
        let long = "x".repeat(100);
        let existing = vec![long.clone()];
        let result = unique_name(&long, &existing).unwrap();
        let sanitized = crate::subscription::profile::sanitize_filename(&result);
        assert!(sanitized.chars().count() <= 80, "got: {}", sanitized);
        assert_ne!(
            sanitized,
            crate::subscription::profile::sanitize_filename(&long),
            "sanitized candidate must differ from sanitized base"
        );
    }

    #[test]
    fn test_unique_name_exhaustion_returns_error() {
        let base = "base";
        let mut existing: Vec<String> = vec![base.to_string()];
        for i in 2..=200 {
            existing.push(format!("{} ({})", base, i));
        }
        assert!(unique_name(base, &existing).is_err());
    }

    #[tokio::test]
    async fn test_use_profile_writes_three_sections() {
        let (env, mut config) =
            TestEnv::new("mixed-port: 7897\nmode: rule\ndns:\n  enable: true\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        let result = SubscriptionManager::use_profile(&mut config, "sub1", true).await;
        assert!(result.is_ok(), "{:?}", result);
        assert_eq!(config.subscriptions.active.as_deref(), Some("sub1"));
        let written = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        assert!(written.contains("name: N1"));
        assert!(written.contains("MATCH,G"));
        assert!(written.contains("mixed-port: 7897"));
        assert!(written.contains("dns:"));
        drop(env);
    }

    #[tokio::test]
    async fn test_activate_success_discards_backup() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        SubscriptionManager::use_profile(&mut config, "sub1", true)
            .await
            .unwrap();
        assert!(!std::path::Path::new(&format!("{}.bak", config.mihomo.config_path)).exists());
    }

    #[tokio::test]
    async fn test_use_profile_missing_archive_fails() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("nope".into(), "https://x".into());
        let err = SubscriptionManager::use_profile(&mut config, "nope", true)
            .await
            .unwrap_err();
        assert!(err.contains("update"));
        drop(env);
    }

    #[tokio::test]
    async fn test_use_profile_corrupt_archive_suggests_refetch() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("bad".into(), "https://x".into());
        crate::subscription::profile::write_archive("bad", "corrupted archive content").unwrap();
        let err = SubscriptionManager::use_profile(&mut config, "bad", true)
            .await
            .unwrap_err();
        assert!(err.contains("mioctl sub update bad"), "got: {}", err);
        assert_eq!(config.subscriptions.active, None);
    }

    #[tokio::test]
    async fn test_activate_write_failure_rolls_back_file() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        let original = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        std::fs::create_dir(format!("{}.tmp", config.mihomo.config_path)).unwrap();

        let result = SubscriptionManager::use_profile(&mut config, "sub1", true).await;

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&config.mihomo.config_path).unwrap(),
            original
        );
        assert!(std::path::Path::new(&format!("{}.bak", config.mihomo.config_path)).exists());
    }

    #[tokio::test]
    async fn test_remove_inactive_archive_failure_preserves_entry() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("act".into(), "https://x".into());
        config.add_subscription("other".into(), "https://y".into());
        config.set_active(Some("act"));
        crate::subscription::profile::write_archive("other", SUB_YAML).unwrap();
        let archive = crate::subscription::profile::archive_path("other");
        std::fs::remove_file(&archive).unwrap();
        std::fs::create_dir(&archive).unwrap();

        let result = SubscriptionManager::remove(&mut config, "other").await;

        assert!(result.is_err());
        assert!(config.find_subscription("other").is_some());
        assert!(config.find_subscription("act").is_some());
        assert_eq!(config.subscriptions.active.as_deref(), Some("act"));
    }

    #[tokio::test]
    async fn test_remove_active_writes_empty_state() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        SubscriptionManager::use_profile(&mut config, "sub1", true)
            .await
            .unwrap();
        SubscriptionManager::remove(&mut config, "sub1")
            .await
            .unwrap();
        assert!(config.subscriptions.items.is_empty());
        assert_eq!(config.subscriptions.active, None);
        let written = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        assert!(written.contains("proxies: []"));
        assert!(written.contains("MATCH,DIRECT"));
        assert!(written.contains("mixed-port: 7897"));
        assert!(!crate::subscription::profile::archive_exists("sub1"));
        drop(env);
    }

    #[tokio::test]
    async fn test_remove_inactive_keeps_active_config() {
        let (env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("act".into(), "https://x".into());
        config.add_subscription("other".into(), "https://y".into());
        crate::subscription::profile::write_archive("act", SUB_YAML).unwrap();
        crate::subscription::profile::write_archive("other", SUB_YAML).unwrap();
        SubscriptionManager::use_profile(&mut config, "act", true)
            .await
            .unwrap();
        SubscriptionManager::remove(&mut config, "other")
            .await
            .unwrap();
        assert_eq!(config.subscriptions.active.as_deref(), Some("act"));
        assert!(config.find_subscription("act").is_some());
        drop(env);
    }

    #[test]
    fn test_write_empty_state_merge_failure_rolls_back_file() {
        let (_env, config) = TestEnv::new("invalid: [yaml\n");
        let result = write_empty_state(&config);
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&config.mihomo.config_path).unwrap(),
            "invalid: [yaml\n"
        );
    }

    #[test]
    fn test_write_empty_state_write_failure_rolls_back_file() {
        let (_env, config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        let original = std::fs::read_to_string(&config.mihomo.config_path).unwrap();
        std::fs::create_dir(format!("{}.tmp", config.mihomo.config_path)).unwrap();

        let result = write_empty_state(&config);

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&config.mihomo.config_path).unwrap(),
            original
        );
        assert!(std::path::Path::new(&format!("{}.bak", config.mihomo.config_path)).exists());
    }

    #[tokio::test]
    async fn test_remove_active_failure_keeps_subscription_and_archive() {
        let (_env, mut config) = TestEnv::new("invalid: [yaml\n");
        config.add_subscription("sub1".into(), "https://x".into());
        config.set_active(Some("sub1"));
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();

        let result = SubscriptionManager::remove(&mut config, "sub1").await;

        assert!(result.is_err());
        assert!(config.find_subscription("sub1").is_some());
        assert_eq!(config.subscriptions.active.as_deref(), Some("sub1"));
        assert!(crate::subscription::profile::archive_exists("sub1"));
        assert_eq!(
            std::fs::read_to_string(&config.mihomo.config_path).unwrap(),
            "invalid: [yaml\n"
        );
    }

    #[tokio::test]
    async fn test_remove_active_archive_failure_preserves_config() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\nmode: rule\n");
        config.add_subscription("sub1".into(), "https://x".into());
        config.set_active(Some("sub1"));
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        let archive = crate::subscription::profile::archive_path("sub1");
        std::fs::remove_file(&archive).unwrap();
        std::fs::create_dir(&archive).unwrap();

        let result = SubscriptionManager::remove(&mut config, "sub1").await;

        assert!(result.is_err());
        assert!(config.find_subscription("sub1").is_some());
        assert_eq!(config.subscriptions.active.as_deref(), Some("sub1"));
    }

    #[tokio::test]
    async fn test_reload_mihomo_offline_reaches_manual_hint() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.mihomo.external_controller = "http://127.0.0.1:1".into();
        let start = std::time::Instant::now();
        let result = reload_mihomo(&config).await;
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        assert!(
            result.contains("systemctl restart also failed. Run: systemctl --user restart mihomo")
        );
    }

    #[test]
    fn test_list_output() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("sub1".into(), "https://x".into());
        config.subscriptions.items[0].node_count = Some(10);
        config.set_active(Some("sub1"));
        config.add_subscription("sub2".into(), "https://y".into());
        let out = SubscriptionManager::list(&config);
        assert!(out.contains("sub1"));
        assert!(out.contains("sub2"));
        assert!(out.contains('*'));
    }

    #[test]
    fn test_list_empty_output() {
        let (_env, config) = TestEnv::new("mixed-port: 7897\n");
        assert_eq!(
            SubscriptionManager::list(&config),
            "No subscriptions. Add one: mioctl sub add <url>"
        );
    }

    #[tokio::test]
    async fn test_missing_subscription_errors() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        assert!(SubscriptionManager::remove(&mut config, "missing")
            .await
            .unwrap_err()
            .contains("no subscription"));
        assert!(
            SubscriptionManager::use_profile(&mut config, "missing", true)
                .await
                .unwrap_err()
                .contains("no subscription")
        );
    }

    #[tokio::test]
    async fn test_update_target_validation() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        assert!(
            SubscriptionManager::update(&mut config, &UpdateTarget::Named("missing".into()))
                .await
                .unwrap_err()
                .contains("no subscription")
        );
        assert!(
            SubscriptionManager::update(&mut config, &UpdateTarget::Active)
                .await
                .unwrap_err()
                .contains("no active")
        );
        let report = SubscriptionManager::update(&mut config, &UpdateTarget::All)
            .await
            .unwrap();
        assert!(report.lines.is_empty());
        assert!(!report.failed);
    }

    #[tokio::test]
    async fn test_update_fetch_failure_sets_failed_flag() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("s".into(), "http://127.0.0.1:1/sub".into());
        let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("s".into()))
            .await
            .unwrap();
        assert!(report.failed, "lines: {:?}", report.lines);
        assert!(
            report.lines.iter().any(|l| l.contains("s: ERROR - ")),
            "lines: {:?}",
            report.lines
        );
    }

    #[tokio::test]
    async fn test_update_remerge_failure_marks_failed() {
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/sub"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(SUB_YAML))
            .mount(&mock)
            .await;
        let (_env, mut config) = TestEnv::new("invalid: [yaml\n");
        config.add_subscription("sub1".into(), format!("{}/sub", mock.uri()));
        config.set_active(Some("sub1"));
        let report = SubscriptionManager::update(&mut config, &UpdateTarget::Named("sub1".into()))
            .await
            .unwrap();
        assert!(report.failed, "lines: {:?}", report.lines);
        assert!(
            report
                .lines
                .iter()
                .any(|l| l.contains("sub1: ERROR - re-merge failed -")),
            "lines: {:?}",
            report.lines
        );
    }

    #[tokio::test]
    async fn test_add_rejects_name_with_comma() {
        let mock = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/sub"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(SUB_YAML))
            .mount(&mock)
            .await;
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        let err = SubscriptionManager::add(
            &mut config,
            &format!("{}/sub", mock.uri()),
            Some("a,b".into()),
            true,
            false,
        )
        .await
        .unwrap_err();
        assert!(err.contains("commas"), "got: {}", err);
        assert!(config.subscriptions.items.is_empty());
    }

    #[tokio::test]
    async fn test_ensure_archived_returns_without_missing_profiles() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        assert!(SubscriptionManager::ensure_archived(&mut config)
            .await
            .is_empty());
    }

    #[tokio::test]
    async fn test_activate_merge_failure_rolls_back_file() {
        let (_env, mut config) = TestEnv::new("invalid: [yaml\n");
        config.add_subscription("sub1".into(), "https://x".into());
        crate::subscription::profile::write_archive("sub1", SUB_YAML).unwrap();
        let result = SubscriptionManager::use_profile(&mut config, "sub1", true).await;
        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(&config.mihomo.config_path).unwrap(),
            "invalid: [yaml\n"
        );
    }

    #[test]
    fn test_client_for_rejects_invalid_secret() {
        let mut config = MioctlConfig::default();
        config.mihomo.secret = "bad\nsecret".into();
        assert!(client_for(&config).is_err());
    }

    #[tokio::test]
    async fn test_reload_mihomo_client_construction_failure_reaches_manual_hint() {
        let (_env, mut config) = TestEnv::new("mixed-port: 7897\n");
        config.mihomo.secret = "bad\nsecret".into();

        let result = reload_mihomo(&config).await;

        assert!(result.contains("systemctl restart also failed."));
        assert!(result.contains("Run: systemctl --user restart mihomo"));
    }
}
