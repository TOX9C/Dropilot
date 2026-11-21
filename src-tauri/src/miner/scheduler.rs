use std::time::Duration;

use rand::Rng;

pub const MIN_CAMPAIGN_REFRESH_SECS: u64 = 10 * 60;
pub const PROGRESS_POLL_SECS: u64 = 5 * 60;
pub const LIVE_CHECK_SECS: u64 = 150;
pub const WATCH_TICK_SECS: u64 = 60;
pub const IDLE_RETRY_SECS: u64 = 5 * 60;
pub const ERROR_RETRY_SECS: u64 = 15 * 60;
pub const MOCK_WATCH_TICK_SECS: u64 = 6;

/// Applies +-10% uniform jitter so requests never synchronize into rigid intervals.
pub fn jittered_duration(base_secs: u64) -> Duration {
    let mut rng = rand::thread_rng();
    jittered_with_rng(base_secs, &mut rng)
}

pub fn jittered_with_rng<R: Rng + ?Sized>(base_secs: u64, rng: &mut R) -> Duration {
    if base_secs == 0 {
        return Duration::ZERO;
    }
    let factor: f64 = rng.gen_range(0.90..=1.10);
    let millis = ((base_secs as f64) * 1000.0 * factor).round() as u64;
    Duration::from_millis(millis.max(100))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn jitter_stays_within_plus_minus_ten_percent() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        for _ in 0..100 {
            let d = jittered_with_rng(60, &mut rng);
            assert!(d >= Duration::from_secs(54));
            assert!(d <= Duration::from_secs(66));
        }
    }
}
