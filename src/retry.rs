use std::future::Future;
use std::time::Duration;

use backoff::ExponentialBackoff;
use backoff::future::retry;

use crate::error::SdkError;

/// Returns `true` if the error is retryable (transient).
pub fn is_retryable(err: &SdkError) -> bool {
    match err {
        SdkError::Timeout => true,
        SdkError::ConnectionClosed => true,
        SdkError::Http(e) => e.is_timeout() || e.is_connect() || e.is_request(),
        SdkError::HttpStatus { status, .. } => *status == 429 || *status >= 500,
        SdkError::WebSocket(_) => true,
        SdkError::Json(_)
        | SdkError::Url(_)
        | SdkError::Api { .. }
        | SdkError::MissingApiKey
        | SdkError::Validation(_) => false,
        #[cfg(feature = "chain")]
        SdkError::Chain(_) | SdkError::MissingPrivateKey => false,
    }
}

/// Retry an async operation with exponential backoff.
///
/// Only retries if `is_retryable` returns true for the error.
/// Uses sensible defaults: initial interval 500ms, max interval 30s, max elapsed 2min.
pub async fn with_retry<F, Fut, T>(op: F) -> Result<T, SdkError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, SdkError>>,
{
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(500),
        max_interval: Duration::from_secs(30),
        max_elapsed_time: Some(Duration::from_secs(120)),
        ..ExponentialBackoff::default()
    };

    retry(backoff, || {
        let fut = op();
        async {
            fut.await.map_err(|e| {
                if is_retryable(&e) {
                    backoff::Error::transient(e)
                } else {
                    backoff::Error::permanent(e)
                }
            })
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn retryable_timeout() {
        assert!(is_retryable(&SdkError::Timeout));
    }

    #[test]
    fn retryable_connection_closed() {
        assert!(is_retryable(&SdkError::ConnectionClosed));
    }

    #[test]
    fn retryable_http_status_429() {
        assert!(is_retryable(&SdkError::HttpStatus {
            status: 429,
            body: "rate limited".into(),
        }));
    }

    #[test]
    fn retryable_http_status_500() {
        assert!(is_retryable(&SdkError::HttpStatus {
            status: 500,
            body: "server error".into(),
        }));
    }

    #[test]
    fn not_retryable_api_error() {
        assert!(!is_retryable(&SdkError::Api {
            code: 1001,
            msg: "bad param".into(),
        }));
    }

    #[test]
    fn not_retryable_missing_api_key() {
        assert!(!is_retryable(&SdkError::MissingApiKey));
    }

    #[test]
    fn not_retryable_json_error() {
        let json_err = serde_json::from_str::<i32>("bad").unwrap_err();
        assert!(!is_retryable(&SdkError::Json(json_err)));
    }

    #[test]
    fn not_retryable_http_status_404() {
        assert!(!is_retryable(&SdkError::HttpStatus {
            status: 404,
            body: "not found".into(),
        }));
    }

    #[tokio::test]
    async fn with_retry_succeeds_immediately() {
        let result = with_retry(|| async { Ok::<_, SdkError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn with_retry_permanent_error_no_retry() {
        let attempts = AtomicU32::new(0);
        let result = with_retry(|| {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { Err::<i32, _>(SdkError::MissingApiKey) }
        })
        .await;
        assert!(matches!(result.unwrap_err(), SdkError::MissingApiKey));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn with_retry_transient_then_success() {
        let attempts = AtomicU32::new(0);
        let result = with_retry(|| {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if n < 2 {
                    Err::<i32, _>(SdkError::Timeout)
                } else {
                    Ok(99)
                }
            }
        })
        .await;
        assert_eq!(result.unwrap(), 99);
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }
}
