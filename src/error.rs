use thiserror::Error;

#[derive(Debug, Error)]
pub enum SdkError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("api returned non-success code={code}, msg={msg}")]
    Api { code: i64, msg: String },

    #[error("http status {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("api key required for this endpoint")]
    MissingApiKey,
}

pub type Result<T> = std::result::Result<T, SdkError>;

impl From<tokio_tungstenite::tungstenite::Error> for SdkError {
    fn from(value: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(value.to_string())
    }
}
