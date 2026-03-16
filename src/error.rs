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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_http_status_error() {
        let err = SdkError::HttpStatus {
            status: 404,
            body: "not found".into(),
        };
        assert_eq!(err.to_string(), "http status 404: not found");
    }

    #[test]
    fn display_api_error() {
        let err = SdkError::Api {
            code: 1001,
            msg: "invalid param".into(),
        };
        assert_eq!(
            err.to_string(),
            "api returned non-success code=1001, msg=invalid param"
        );
    }

    #[test]
    fn display_missing_api_key() {
        let err = SdkError::MissingApiKey;
        assert_eq!(err.to_string(), "api key required for this endpoint");
    }

    #[test]
    fn display_websocket_error() {
        let err = SdkError::WebSocket("connection refused".into());
        assert_eq!(err.to_string(), "websocket error: connection refused");
    }

    #[test]
    fn from_json_error() {
        let json_err = serde_json::from_str::<i32>("not json").unwrap_err();
        let err = SdkError::from(json_err);
        assert!(matches!(err, SdkError::Json(_)));
        assert!(err.to_string().starts_with("json error:"));
    }

    #[test]
    fn from_url_parse_error() {
        let url_err = url::Url::parse("://bad").unwrap_err();
        let err = SdkError::from(url_err);
        assert!(matches!(err, SdkError::Url(_)));
        assert!(err.to_string().starts_with("url parse error:"));
    }
}
