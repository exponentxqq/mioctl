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
                Ok(c) => {
                    let url = format!("{}/version", c.base_url());
                    match c.get_version().await {
                        Ok(v) => println!("Connected to mihomo {}", v.version),
                        Err(e) => {
                            // Try raw request for debugging
                            match reqwest::get(&url).await {
                                Ok(resp) => {
                                    let status = resp.status();
                                    match resp.text().await {
                                        Ok(body) => eprintln!(
                                            "API error: {}\n  URL: {}\n  Status: {}\n  Body (first 200): {}",
                                            e, url, status,
                                            &body[..body.len().min(200)]
                                        ),
                                        Err(_) => eprintln!("API error: {}\n  URL: {}", e, url),
                                    }
                                }
                                Err(_) => eprintln!("API error: {}\n  URL: {}", e, url),
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Connection failed: {}", e),
            }
        }
    }
}
