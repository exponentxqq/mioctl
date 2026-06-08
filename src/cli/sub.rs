use crate::cli::SubAction;
use crate::config::mioctl_config::MioctlConfig;
use crate::subscription::manager::SubscriptionManager;

pub async fn run(action: SubAction) {
    match action {
        SubAction::Update { all } => {
            if !all {
                eprintln!("Use --all to update all subscriptions");
                return;
            }
            let mut config = MioctlConfig::load();
            let secret = if config.mihomo.secret.is_empty() {
                None
            } else {
                Some(config.mihomo.secret.clone())
            };
            match crate::api::client::MihomoClient::new(&config.mihomo.external_controller, secret) {
                Ok(c) => match SubscriptionManager::update_all(&mut config, &c).await {
                    Ok(result) => println!("{}", result),
                    Err(e) => eprintln!("Error: {}", e),
                },
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
    }
}
