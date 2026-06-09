use thiserror::Error;

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names, dead_code)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Authentication failed — check your secret")]
    Unauthorized,

    #[error("Request timed out")]
    Timeout,

    #[error("Mihomo API returned error status {0}: {1}")]
    ApiError(u16, String),

    #[error("JSON deserialization failed: {0}")]
    Deserialization(#[from] serde_json::Error),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

pub type ApiResult<T> = Result<T, ApiError>;
