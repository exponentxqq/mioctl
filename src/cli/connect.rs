use crate::cli::ConnectAction;
use crate::config::mioctl_config::MioctlConfig;

pub async fn run(action: ConnectAction) {
    match action {
        ConnectAction::Test => {
            let config = MioctlConfig::load();
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret)
            };
            match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match c.get_version().await {
                    Ok(v) => println!("Connected to mihomo {}", v.version),
                    Err(e) => eprintln!("API error: {}", e),
                },
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
    }
}
