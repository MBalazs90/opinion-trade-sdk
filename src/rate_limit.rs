use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

/// A token bucket rate limiter for controlling API request rates.
///
/// Thread-safe via `Arc<Mutex<_>>` — clone the `RateLimiter` to share across tasks.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<TokenBucket>>,
}

#[derive(Debug)]
struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// - `requests_per_second`: Maximum sustained request rate.
    /// - `burst`: Maximum burst size (tokens available at start and after idle periods).
    pub fn new(requests_per_second: f64, burst: u32) -> Self {
        let burst = burst.max(1) as f64;
        Self {
            inner: Arc::new(Mutex::new(TokenBucket {
                capacity: burst,
                tokens: burst,
                refill_rate: requests_per_second,
                last_refill: Instant::now(),
            })),
        }
    }

    /// Acquire a token, waiting if necessary until one is available.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut bucket = self.inner.lock().await;
                bucket.refill();
                if bucket.tokens >= 1.0 {
                    bucket.tokens -= 1.0;
                    return;
                }
                let deficit = 1.0 - bucket.tokens;
                Duration::from_secs_f64(deficit / bucket.refill_rate)
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// Try to acquire a token without waiting. Returns `true` if acquired.
    pub async fn try_acquire(&self) -> bool {
        let mut bucket = self.inner.lock().await;
        bucket.refill();
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Returns the number of tokens currently available (may be fractional).
    pub async fn available(&self) -> f64 {
        let mut bucket = self.inner.lock().await;
        bucket.refill();
        bucket.tokens
    }
}

impl TokenBucket {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn try_acquire_succeeds_with_tokens() {
        let limiter = RateLimiter::new(10.0, 5);
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn try_acquire_fails_when_exhausted() {
        let limiter = RateLimiter::new(10.0, 2);
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn available_reflects_consumption() {
        let limiter = RateLimiter::new(10.0, 5);
        let before = limiter.available().await;
        assert!((before - 5.0).abs() < 0.1);
        limiter.try_acquire().await;
        let after = limiter.available().await;
        assert!((after - 4.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn acquire_waits_for_refill() {
        let limiter = RateLimiter::new(100.0, 1); // 1 burst, 100/s refill
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await);

        // acquire should wait and succeed
        let start = Instant::now();
        limiter.acquire().await;
        let elapsed = start.elapsed();
        assert!(elapsed < Duration::from_millis(50)); // 100/s = 10ms per token
    }

    #[tokio::test]
    async fn tokens_refill_over_time() {
        let limiter = RateLimiter::new(100.0, 2);
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await);

        tokio::time::sleep(Duration::from_millis(25)).await;
        // Should have refilled ~2.5 tokens
        assert!(limiter.try_acquire().await);
    }

    #[tokio::test]
    async fn tokens_capped_at_burst() {
        let limiter = RateLimiter::new(1000.0, 3);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let available = limiter.available().await;
        assert!((available - 3.0).abs() < 0.1); // capped at burst=3
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let limiter = RateLimiter::new(10.0, 2);
        let limiter2 = limiter.clone();
        assert!(limiter.try_acquire().await);
        assert!(limiter2.try_acquire().await);
        assert!(!limiter.try_acquire().await);
        assert!(!limiter2.try_acquire().await);
    }

    #[tokio::test]
    async fn burst_minimum_is_one() {
        let limiter = RateLimiter::new(10.0, 0);
        assert!(limiter.try_acquire().await); // burst=0 clamped to 1
        assert!(!limiter.try_acquire().await);
    }
}
