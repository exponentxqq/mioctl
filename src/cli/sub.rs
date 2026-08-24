use crate::cli::SubAction;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::manager::{SubscriptionManager, UpdateTarget};
use std::io::IsTerminal;

pub async fn run(action: SubAction) -> bool {
    let mut config = MioctlConfig::load();
    for warning in SubscriptionManager::ensure_archived(&mut config).await {
        eprintln!("{}", warning);
    }
    match action {
        SubAction::List => {
            println!("{}", SubscriptionManager::list(&config));
            true
        }
        SubAction::Add {
            url,
            name,
            no_reload,
            activate,
        } => match SubscriptionManager::add(&mut config, &url, name, no_reload, activate).await {
            Ok(summary) => {
                println!("{}", summary);
                true
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                false
            }
        },
        SubAction::Register {
            url,
            name,
            no_reload,
        } => match SubscriptionManager::add(&mut config, &url, name, no_reload, false).await {
            Ok(summary) => {
                println!("{}", summary);
                true
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                false
            }
        },
        SubAction::Use { name, no_reload } => {
            match SubscriptionManager::use_profile(&mut config, &name, no_reload).await {
                Ok(message) => {
                    println!("{}", message);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
        SubAction::Update { name, all } => {
            let target = if all {
                UpdateTarget::All
            } else if let Some(name) = name {
                UpdateTarget::Named(name)
            } else {
                UpdateTarget::Active
            };
            match SubscriptionManager::update(&mut config, &target).await {
                Ok(report) => {
                    println!("{}", report.lines.join("\n"));
                    !report.failed
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
        SubAction::Remove { name, yes } => {
            let confirmed = yes || {
                if !std::io::stdin().is_terminal() {
                    false
                } else {
                    print!("Remove subscription '{}'? [y/N] ", name);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input).unwrap_or(0);
                    input.trim().eq_ignore_ascii_case("y")
                }
            };
            if !confirmed {
                println!("Cancelled.");
                return true;
            }
            match SubscriptionManager::remove(&mut config, &name).await {
                Ok(message) => {
                    println!("{}", message);
                    true
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::SubAction;
    use std::path::Path;

    fn dispatch_child_with_env(
        mode: &str,
        home: &Path,
        extra: &[(&str, &str)],
    ) -> std::process::Output {
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "cli::sub::tests::cli_sub_child_dispatch",
            "--nocapture",
        ])
        .env("MIOCTL_TEST_CHILD", mode)
        .env("MIOCTL_HOME", home);
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let out = cmd.output().unwrap();
        assert!(
            stdout_of(&out).contains("running 1 test"),
            "child ran no tests; stdout: {}; stderr: {}",
            stdout_of(&out),
            stderr_of(&out)
        );
        out
    }

    fn dispatch_child(mode: &str, home: &Path) -> std::process::Output {
        dispatch_child_with_env(mode, home, &[])
    }

    fn stdout_of(out: &std::process::Output) -> String {
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn stderr_of(out: &std::process::Output) -> String {
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    fn action_for(mode: &str) -> SubAction {
        match mode {
            "list-empty" => SubAction::List,
            "use-nosuch" => SubAction::Use {
                name: "nosuch".into(),
                no_reload: true,
            },
            "remove-nosuch-yes" => SubAction::Remove {
                name: "nosuch".into(),
                yes: true,
            },
            "remove-nosuch-prompt" => SubAction::Remove {
                name: "nosuch".into(),
                yes: false,
            },
            "update-active" => SubAction::Update {
                name: None,
                all: false,
            },
            "update-named" => SubAction::Update {
                name: Some("nosuch".into()),
                all: false,
            },
            "update-all-empty" => SubAction::Update {
                name: None,
                all: true,
            },
            "update-all-over-name" => SubAction::Update {
                name: Some("nosuch".into()),
                all: true,
            },
            "update-fetch-fail" => SubAction::Update {
                name: Some("s".into()),
                all: false,
            },
            "update-error-name" => SubAction::Update {
                name: Some("ERRORS".into()),
                all: false,
            },
            "add-fetch-fail" => SubAction::Add {
                url: "http://127.0.0.1:1/sub".into(),
                name: Some("x".into()),
                no_reload: true,
                activate: false,
            },
            "register-fetch-fail" => SubAction::Register {
                url: "http://127.0.0.1:1/sub".into(),
                name: Some("x".into()),
                no_reload: true,
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn cli_sub_child_dispatch() {
        let mode = match std::env::var("MIOCTL_TEST_CHILD") {
            Ok(mode) => mode,
            Err(_) => return,
        };
        if mode == "update-error-name" {
            let url = std::env::var("MIOCTL_TEST_UPDATE_URL").unwrap();
            let home = std::env::var("MIOCTL_HOME").unwrap();
            std::fs::write(
                std::path::Path::new(&home).join("config.toml"),
                format!("[[subscriptions.items]]\nname = \"ERRORS\"\nurl = \"{url}\"\n"),
            )
            .unwrap();
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let ok = runtime.block_on(run(action_for(&mode)));
        if !ok {
            println!("__MIOCTL_EXIT_1__");
        }
        std::process::exit(0);
    }

    #[test]
    fn update_without_flags_dispatches_to_active_target() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("update-active", dir.path());
        assert!(out.status.success());
        let err = stderr_of(&out);
        assert!(err.starts_with("Error: "), "stderr: {}", err);
        assert!(err.contains("no active subscription"), "stderr: {}", err);
        assert!(
            stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&out)
        );
    }

    #[test]
    fn update_with_name_dispatches_to_named_target() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("update-named", dir.path());
        assert!(out.status.success());
        let err = stderr_of(&out);
        assert!(err.starts_with("Error: "), "stderr: {}", err);
        assert!(
            err.contains("no subscription named 'nosuch'"),
            "stderr: {}",
            err
        );
        assert!(!err.contains("no active"), "stderr: {}", err);
        assert!(
            stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&out)
        );
    }

    #[test]
    fn update_all_with_empty_config_succeeds_silently() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("update-all-empty", dir.path());
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        let err = stderr_of(&out);
        assert!(!err.contains("Error"), "stderr: {}", err);
        assert!(!stdout.contains("Error"), "stdout: {}", stdout);
        assert!(!stdout.contains("nodes updated"), "stdout: {}", stdout);
        assert!(
            !stdout.contains("__MIOCTL_EXIT_1__"),
            "unexpected failure signal, stdout: {}",
            stdout
        );
    }

    #[test]
    fn update_all_flag_takes_precedence_over_name() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("update-all-over-name", dir.path());
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        let err = stderr_of(&out);
        assert!(
            !err.contains("no subscription named 'nosuch'"),
            "stderr: {}",
            err
        );
        assert!(!err.contains("Error"), "stderr: {}", err);
        assert!(!stdout.contains("nodes updated"), "stdout: {}", stdout);
        assert!(
            !stdout.contains("__MIOCTL_EXIT_1__"),
            "unexpected failure signal, stdout: {}",
            stdout
        );
    }

    #[test]
    fn use_dispatches_missing_name_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("use-nosuch", dir.path());
        assert!(out.status.success());
        let err = stderr_of(&out);
        assert!(err.starts_with("Error: "), "stderr: {}", err);
        assert!(
            err.contains("no subscription named 'nosuch'"),
            "stderr: {}",
            err
        );
        assert!(
            stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&out)
        );
    }

    #[test]
    fn list_dispatches_empty_output() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("list-empty", dir.path());
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        assert!(
            stdout.contains("No subscriptions. Add one: mioctl sub add <url>"),
            "stdout: {}",
            stdout
        );
        assert!(
            !stdout.contains("__MIOCTL_EXIT_1__"),
            "unexpected failure signal, stdout: {}",
            stdout
        );
    }

    #[test]
    fn remove_with_yes_dispatches_missing_name_error() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("remove-nosuch-yes", dir.path());
        assert!(out.status.success());
        let err = stderr_of(&out);
        assert!(err.starts_with("Error: "), "stderr: {}", err);
        assert!(
            err.contains("no subscription named 'nosuch'"),
            "stderr: {}",
            err
        );
        assert!(
            stdout_of(&out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&out)
        );
    }

    #[test]
    fn remove_without_yes_cancels_in_non_interactive_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let out = dispatch_child("remove-nosuch-prompt", dir.path());
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        assert!(stdout.contains("Cancelled."), "stdout: {}", stdout);
        assert!(!stderr_of(&out).contains("Error"));
        assert!(
            !stdout.contains("__MIOCTL_EXIT_1__"),
            "unexpected failure signal, stdout: {}",
            stdout
        );
    }

    #[test]
    fn update_fetch_fail_ok_result_error_line_yields_nonzero_exit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            "[[subscriptions.items]]\nname = \"s\"\nurl = \"http://127.0.0.1:1/sub\"\n",
        )
        .unwrap();
        let out = dispatch_child("update-fetch-fail", dir.path());
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        let err = stderr_of(&out);
        assert!(stdout.contains("ERROR -"), "stdout: {}", stdout);
        assert!(
            stdout.contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout
        );
        assert!(!err.starts_with("Error: "), "stderr: {}", err);
    }

    #[tokio::test]
    async fn update_name_containing_error_substring_exits_zero() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/sub"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(
                "proxies:\n  - name: N1\n    type: ss\n    server: 1.2.3.4\n    port: 8388\n    cipher: aes-256-gcm\n    password: p\nproxy-groups:\n  - name: ERRORS\n    type: select\n    proxies: [N1]\nrules:\n  - MATCH,ERRORS\n",
            ))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let url = format!("{}/sub", server.uri());
        let out = dispatch_child_with_env(
            "update-error-name",
            dir.path(),
            &[("MIOCTL_TEST_UPDATE_URL", url.as_str())],
        );
        assert!(out.status.success());
        let stdout = stdout_of(&out);
        assert!(
            stdout.contains("ERRORS: 1 nodes updated"),
            "stdout: {}",
            stdout
        );
        assert!(
            !stdout.contains("__MIOCTL_EXIT_1__"),
            "name containing 'ERROR' must not be mistaken for failure, stdout: {}",
            stdout
        );
    }

    #[test]
    fn register_and_add_dispatch_to_same_offline_error() {
        let add_dir = tempfile::tempdir().unwrap();
        let register_dir = tempfile::tempdir().unwrap();
        let add_out = dispatch_child("add-fetch-fail", add_dir.path());
        let register_out = dispatch_child("register-fetch-fail", register_dir.path());
        assert!(add_out.status.success());
        assert!(register_out.status.success());
        let add_err = stderr_of(&add_out);
        let register_err = stderr_of(&register_out);
        assert!(
            add_err.contains("Error: all User-Agent probes failed"),
            "add stderr: {}",
            add_err
        );
        assert_eq!(add_err, register_err);
        assert!(
            stdout_of(&add_out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&add_out)
        );
        assert!(
            stdout_of(&register_out).contains("__MIOCTL_EXIT_1__"),
            "expected nonzero-exit signal, stdout: {}",
            stdout_of(&register_out)
        );
    }
}
