use clap::{Parser, Subcommand};

pub mod connect;
pub mod sub;
pub mod tui;

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
}

#[derive(Subcommand)]
pub enum SubAction {
    /// Update all subscriptions
    Update {
        #[arg(long)]
        all: bool,
    },
}

#[derive(Subcommand)]
pub enum ConnectAction {
    /// Test connection to mihomo API
    Test,
}
