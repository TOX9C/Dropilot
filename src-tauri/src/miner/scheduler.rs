use std::time::Duration;

use chrono::{Local, Timelike};
use rand::Rng;

use crate::settings::ScheduleSettings;

pub const MIN_CAMPAIGN_REFRESH_SECS: u64 = 10 * 60;
pub const PROGRESS_POLL_SECS: u64 = 5 * 60;
pub const LIVE_CHECK_SECS: u64 = 150;
pub const WATCH_TICK_SECS: u64 = 60;
pub const IDLE_RETRY_SECS: u64 = 5 * 60;
pub const ERROR_RETRY_SECS: u64 = 15 * 60;
pub const MOCK_WATCH_TICK_SECS: u64 = 6;

const STANDARD_JITTER_SPREAD: f64 = 0.10;
const HUMANIZED_JITTER_SPREAD: f64 = 0.25;

/// Applies +-10% uniform jitter so requests never synchronize into rigid intervals.
pub fn jittered_duration(base_secs: u64) -> Duration {
    jittered_duration_with_mode(base_secs, false)
}

/// Applies +-25% humanized jitter when `humanized` is enabled, or +-10% otherwise.
pub fn jittered_duration_with_mode(base_secs: u64, humanized: bool) -> Duration {
    let mut rng = rand::thread_rng();
    let spread = if humanized {
        HUMANIZED_JITTER_SPREAD
    } else {
        STANDARD_JITTER_SPREAD
    };
    jittered_with_rng_and_spread(base_secs, spread, &mut rng)
}

pub fn jittered_with_rng<R: Rng + ?Sized>(base_secs: u64, rng: &mut R) -> Duration {
    jittered_with_rng_and_spread(base_secs, STANDARD_JITTER_SPREAD, rng)
}

pub fn jittered_with_rng_and_spread<R: Rng + ?Sized>(
    base_secs: u64,
    spread: f64,
    rng: &mut R,
) -> Duration {
    if base_secs == 0 {
        return Duration::ZERO;
    }
    let clamped = spread.clamp(0.0, 0.50);
    let factor: f64 = rng.gen_range((1.0 - clamped)..=(1.0 + clamped));
    let millis = ((base_secs as f64) * 1000.0 * factor).round() as u64;
    Duration::from_millis(millis.max(100))
}

pub fn local_minutes_since_midnight() -> u16 {
    let now = Local::now();
    (now.hour() * 60 + now.minute()) as u16
}

/// Returns true if `now_minutes` (0..1440) falls inside the configured quiet hours window.
/// Supports both daytime windows (e.g. 09:00 -> 17:00) and overnight windows (e.g. 22:00 -> 08:00).
pub fn is_in_quiet_hours(schedule: &ScheduleSettings, now_minutes: u16) -> bool {
    if !schedule.quiet_hours_enabled {
        return false;
    }
    let start = schedule.quiet_start_minutes % (24 * 60);
    let end = schedule.quiet_end_minutes % (24 * 60);
    let now = now_minutes % (24 * 60);
    if start == end {
        return false;
    }
    if start < end {
        now >= start && now < end
    } else {
        now >= start || now < end
    }
}

pub fn is_in_quiet_hours_now(schedule: &ScheduleSettings) -> bool {
    is_in_quiet_hours(schedule, local_minutes_since_midnight())
}

/// Returns how long remains until quiet hours end from `now_minutes`.
pub fn duration_until_quiet_end(schedule: &ScheduleSettings, now_minutes: u16) -> Duration {
    let end = schedule.quiet_end_minutes % (24 * 60);
    let now = now_minutes % (24 * 60);
    let mins = if end > now {
        end - now
    } else {
        (24 * 60 - now) + end
    };
    Duration::from_secs((mins.max(1) as u64) * 60)
}

pub fn format_minutes_hhmm(minutes: u16) -> String {
    let m = minutes % (24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
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

    #[test]
    fn humanized_jitter_stays_within_plus_minus_twenty_five_percent() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let d = jittered_with_rng_and_spread(60, HUMANIZED_JITTER_SPREAD, &mut rng);
            assert!(d >= Duration::from_secs(45));
            assert!(d <= Duration::from_secs(75));
        }
    }

    #[test]
    fn quiet_hours_handles_daytime_and_overnight_windows() {
        let daytime = ScheduleSettings {
            quiet_hours_enabled: true,
            quiet_start_minutes: 9 * 60,
            quiet_end_minutes: 17 * 60,
            humanized_jitter: true,
        };
        assert!(!is_in_quiet_hours(&daytime, 8 * 60 + 59));
        assert!(is_in_quiet_hours(&daytime, 9 * 60));
        assert!(is_in_quiet_hours(&daytime, 16 * 60 + 59));
        assert!(!is_in_quiet_hours(&daytime, 17 * 60));

        let overnight = ScheduleSettings {
            quiet_hours_enabled: true,
            quiet_start_minutes: 22 * 60,
            quiet_end_minutes: 8 * 60,
            humanized_jitter: true,
        };
        assert!(!is_in_quiet_hours(&overnight, 21 * 60 + 59));
        assert!(is_in_quiet_hours(&overnight, 22 * 60));
        assert!(is_in_quiet_hours(&overnight, 2 * 60));
        assert!(!is_in_quiet_hours(&overnight, 8 * 60));
        assert_eq!(duration_until_quiet_end(&overnight, 23 * 60), Duration::from_secs(9 * 3600));
        assert_eq!(format_minutes_hhmm(8 * 60 + 5), "08:05");
    }
}
