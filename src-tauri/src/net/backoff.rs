use std::time::Duration;

use rand::Rng;

pub const BASE_BACKOFF_SECS: u64 = 5;
pub const MAX_BACKOFF_SECS: u64 = 15 * 60;
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Debug, Clone)]
pub struct Backoff {
    consecutive_failures: u32,
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
        }
    }

    pub fn failures(&self) -> u32 {
        self.consecutive_failures
    }

    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
    }

    pub fn exceeded_threshold(&self) -> bool {
        self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES
    }

    /// Records a failure and returns the next full-jitter delay (honoring `Retry-After` if larger).
    pub fn record_failure(&mut self, retry_after_secs: Option<u64>) -> Duration {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let attempt = self.consecutive_failures.saturating_sub(1);
        let mut rng = rand::thread_rng();
        compute_full_jitter(attempt, retry_after_secs, &mut rng)
    }
}

pub fn compute_full_jitter<R: Rng + ?Sized>(
    attempt: u32,
    retry_after_secs: Option<u64>,
    rng: &mut R,
) -> Duration {
    let shift = attempt.min(10);
    let cap = BASE_BACKOFF_SECS
        .saturating_mul(1u64 << shift)
        .min(MAX_BACKOFF_SECS);
    let jittered = rng.gen_range(1..=cap.max(1));
    let delay_secs = match retry_after_secs {
        Some(ra) => jittered.max(ra).min(MAX_BACKOFF_SECS.max(ra)),
        None => jittered,
    };
    Duration::from_secs(delay_secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn backoff_respects_bounds_and_retry_after() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for attempt in 0..12 {
            let d = compute_full_jitter(attempt, None, &mut rng);
            assert!(d >= Duration::from_secs(1));
            assert!(d <= Duration::from_secs(MAX_BACKOFF_SECS));
        }

        let honored = compute_full_jitter(0, Some(120), &mut rng);
        assert!(honored >= Duration::from_secs(120));
    }

    #[test]
    fn tracks_five_consecutive_failure_threshold() {
        let mut backoff = Backoff::new();
        for _ in 0..4 {
            backoff.record_failure(None);
            assert!(!backoff.exceeded_threshold());
        }
        backoff.record_failure(None);
        assert!(backoff.exceeded_threshold());
        backoff.reset();
        assert!(!backoff.exceeded_threshold());
    }
}
