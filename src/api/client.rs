use crate::api::error::{ApiError, ApiResult};
use reqwest::{header, Client};

#[derive(Clone)]
pub struct MihomoClient {
    client: Client,
    base_url: String,
}

impl MihomoClient {
    pub fn new(host: &str, secret: Option<String>) -> ApiResult<Self> {
        let base_url = if host.starts_with("http") {
            host.trim_end_matches('/').to_string()
        } else {
            format!("http://{}", host.trim_end_matches('/'))
        };

        let mut headers = header::HeaderMap::new();
        if let Some(ref s) = secret {
            if !s.is_empty() {
                let auth_value = format!("Bearer {}", s);
                let auth_header = header::HeaderValue::from_str(&auth_value)
                    .map_err(|_| ApiError::WebSocketError("invalid secret characters".into()))?;
                headers.insert(header::AUTHORIZATION, auth_header);
            }
        }

        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()?;

        Ok(Self { client, base_url })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}
