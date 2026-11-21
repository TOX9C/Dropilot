use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

#[derive(Debug)]
struct BucketState {
    tokens: f64,
    last_refill: Instant,
}

/// Async token-bucket rate limiter: sustained <= 1 req/sec, burst <= 5.
#[derive(Debug)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    state: Mutex<BucketState>,
}

impl TokenBucket {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            state: Mutex::new(BucketState {
                tokens: capacity,
                last_refill: Instant::now(),
            }),
        }
    }

    pub fn standard_platform() -> Self {
        Self::new(5.0, 1.0)
    }

    pub async fn acquire(&self) {
        loop {
            let wait_for = {
                let mut guard = self.state.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(guard.last_refill).as_secs_f64();
                guard.tokens = (guard.tokens + elapsed * self.refill_per_sec).min(self.capacity);
                guard.last_refill = now;

                if guard.tokens >= 1.0 {
                    guard.tokens -= 1.0;
                    None
                } else {
                    let missing = 1.0 - guard.tokens;
                    let secs = (missing / self.refill_per_sec).max(0.01);
                    Some(Duration::from_secs_f64(secs))
                }
            };

            match wait_for {
                None => return,
                Some(dur) => sleep(dur).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn allows_burst_then_throttles() {
        let bucket = TokenBucket::new(3.0, 10.0);
        let start = Instant::now();
        for _ in 0..3 {
            bucket.acquire().await;
        }
        assert!(start.elapsed() < Duration::from_millis(80));

        // 4th request needs ~100ms refill at 10 tokens/sec
        bucket.acquire().await;
        assert!(start.elapsed() >= Duration::from_millis(80));
    }
}
