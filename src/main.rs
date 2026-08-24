mod api;
mod app;
mod cli;
mod config;
mod os;
pub mod subscription;
mod ui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        None | Some(Commands::Tui) => {
            if let Err(e) = cli::tui::run().await {
                eprintln!("TUI error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Sub { action }) => {
            if !cli::sub::run(action).await {
                std::process::exit(1);
            }
        }
        Some(Commands::Connect { action }) => {
            if !cli::connect::run(action).await {
                std::process::exit(1);
            }
        }
        Some(Commands::Doctor { action }) => {
            cli::doctor::run(action).await;
        }
    }
}

#[cfg(test)]
pub mod testutil {
    use std::sync::{Mutex, OnceLock};

    pub fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
