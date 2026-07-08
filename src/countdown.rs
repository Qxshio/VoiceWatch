use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct AnchoredCountdown {
    banned_until_ms: i64,
    wall_clock_at_fetch_ms: i64,
    monotonic_at_fetch: Instant,
}

impl AnchoredCountdown {
    pub fn new(banned_until_ms: i64) -> Self {
        Self {
            banned_until_ms,
            wall_clock_at_fetch_ms: now_wall_clock_ms(),
            monotonic_at_fetch: Instant::now(),
        }
    }

    pub fn remaining_ms(&self) -> i64 {
        self.banned_until_ms - self.estimated_now_ms()
    }

    pub fn remaining(&self) -> Duration {
        Duration::from_millis(self.remaining_ms().max(0) as u64)
    }

    pub fn is_expired(&self) -> bool {
        self.remaining_ms() <= 0
    }

    fn estimated_now_ms(&self) -> i64 {
        self.wall_clock_at_fetch_ms + self.monotonic_at_fetch.elapsed().as_millis() as i64
    }
}

pub fn now_wall_clock_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn format_remaining(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_short_countdowns() {
        assert_eq!(format_remaining(Duration::from_secs(9)), "00:09");
        assert_eq!(format_remaining(Duration::from_secs(65)), "01:05");
        assert_eq!(format_remaining(Duration::from_secs(3661)), "01:01:01");
    }
}
