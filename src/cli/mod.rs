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
    /// Update all subscriptions
    Update {
        #[arg(long)]
        all: bool,
    },
    /// Register a new subscription
    Register {
        /// Subscription URL
        url: String,
        /// Custom name (auto-detected from subscription if not provided)
        #[arg(long)]
        name: Option<String>,
        /// Skip mihomo reload after registration
        #[arg(long)]
        no_reload: bool,
    },
}

#[derive(Subcommand)]
pub enum ConnectAction {
    /// Test connection to mihomo API
    Test,
}
