use crate::cli::ConnectAction;
use crate::config::mioctl_config::MioctlConfig;

pub async fn run(action: ConnectAction) {
    match action {
        ConnectAction::Test => {
            let config = MioctlConfig::load();
            let has_secret = !config.mihomo.secret.is_empty();
            let secret = if has_secret {
                Some(config.mihomo.secret)
            } else {
                None
            };
            match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match c.get_version().await {
                    Ok(v) => println!("Connected to mihomo {}", v.version),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("401") || msg.contains("Unauthorized") || msg.contains("403") {
                            eprintln!("Authentication failed!");
                            eprintln!("  URL: {}", c.base_url());
                            eprintln!("  The mihomo server requires a secret key.");
                            eprintln!("  Edit ~/.config/mioctl/config.toml and set:");
                            eprintln!();
                            eprintln!("  [mihomo]");
                            eprintln!("  secret = \"your-secret-here\"");
                            eprintln!();
                            eprintln!("  Find your secret in mihomo config under 'secret' field.");
                        } else {
                            eprintln!("API error: {}", e);
                        }
                    }
                },
                Err(e) => eprintln!("Connection failed: {}\n  Check that mihomo is running and external-controller is enabled.", e),
            }
        }
    }
}
