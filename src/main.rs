mod api;
mod app;
mod cli;
mod config;
mod os;
mod subscription;
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
            }
        }
        Some(Commands::Sub { action }) => {
            cli::sub::run(action).await;
        }
        Some(Commands::Connect { action }) => {
            cli::connect::run(action).await;
        }
        Some(Commands::Doctor { action }) => {
            cli::doctor::run(action).await;
        }
    }
}
