use clap::{Parser, Subcommand};

pub mod connect;
pub mod doctor;
pub mod sub;
pub mod tui;

pub use doctor::DoctorAction;

#[derive(Parser)]
#[command(name = "mioctl", version, about = "mihomo terminal management tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch interactive TUI
    Tui,

    /// Manage subscriptions
    Sub {
        #[command(subcommand)]
        action: SubAction,
    },

    /// Test API connectivity
    Connect {
        #[command(subcommand)]
        action: ConnectAction,
    },

    /// Run diagnostic checks on mihomo setup
    Doctor {
        #[command(subcommand)]
        action: DoctorAction,
    },
}

#[derive(Subcommand)]
pub enum SubAction {
    /// List all subscriptions (* = active)
    List,
    /// Add a new subscription
    Add {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after activation
        #[arg(long)]
        no_reload: bool,
        /// Activate immediately even if another subscription is active
        #[arg(long)]
        activate: bool,
    },
    /// Register a new subscription (alias of add)
    Register {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after activation
        #[arg(long)]
        no_reload: bool,
    },
    /// Switch the active subscription
    Use {
        /// Subscription name
        name: String,
        /// Skip mihomo reload
        #[arg(long)]
        no_reload: bool,
    },
    /// Update subscriptions (default: active; --all for every; or name one)
    Update {
        /// Subscription name (omit for active)
        name: Option<String>,
        /// Update all subscriptions
        #[arg(long)]
        all: bool,
    },
    /// Remove a subscription
    Remove {
        /// Subscription name
        name: String,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
pub enum ConnectAction {
    /// Test connection to mihomo API
    Test,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_subscription_commands_and_flags() {
        let cli = Cli::try_parse_from([
            "mioctl",
            "sub",
            "add",
            "https://example.com/sub",
            "--name",
            "work",
            "--no-reload",
            "--activate",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Sub {
                action: SubAction::Add {
                    url,
                    name: Some(name),
                    no_reload: true,
                    activate: true,
                }
            }) if url == "https://example.com/sub" && name == "work"
        ));

        let cli = Cli::try_parse_from(["mioctl", "sub", "use", "work", "--no-reload"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Sub {
                action: SubAction::Use { name, no_reload: true }
            }) if name == "work"
        ));

        let cli = Cli::try_parse_from(["mioctl", "sub", "update", "work"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Sub {
                action: SubAction::Update { name: Some(name), all: false }
            }) if name == "work"
        ));

        let cli = Cli::try_parse_from(["mioctl", "sub", "remove", "work", "--yes"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Sub {
                action: SubAction::Remove { name, yes: true }
            }) if name == "work"
        ));
    }

    #[test]
    fn register_has_same_add_inputs_without_activation() {
        let add = Cli::try_parse_from([
            "mioctl",
            "sub",
            "add",
            "https://example.com/sub",
            "--name",
            "work",
            "--no-reload",
        ])
        .unwrap();
        let register = Cli::try_parse_from([
            "mioctl",
            "sub",
            "register",
            "https://example.com/sub",
            "--name",
            "work",
            "--no-reload",
        ])
        .unwrap();
        assert!(matches!(
            add.command,
            Some(Commands::Sub {
                action: SubAction::Add { url, name, no_reload: true, activate: false }
            }) if url == "https://example.com/sub" && name.as_deref() == Some("work")
        ));
        assert!(matches!(
            register.command,
            Some(Commands::Sub {
                action: SubAction::Register { url, name, no_reload: true }
            }) if url == "https://example.com/sub" && name.as_deref() == Some("work")
        ));
    }

    #[test]
    fn parses_list_and_update_modes() {
        assert!(matches!(
            Cli::try_parse_from(["mioctl", "sub", "list"])
                .unwrap()
                .command,
            Some(Commands::Sub {
                action: SubAction::List
            })
        ));
        assert!(matches!(
            Cli::try_parse_from(["mioctl", "sub", "update", "--all"])
                .unwrap()
                .command,
            Some(Commands::Sub {
                action: SubAction::Update {
                    name: None,
                    all: true
                }
            })
        ));

        let cli = Cli::try_parse_from(["mioctl", "sub", "update", "work", "--all"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Sub {
                action: SubAction::Update {
                    name: Some(name),
                    all: true
                }
            }) if name == "work"
        ));
    }
}
