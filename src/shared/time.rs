//! Calendar-time ABI declarations and conversions for Rust timestamps.
use std::time::{SystemTime, UNIX_EPOCH};

pub use ::libc::{timespec, tm, CLOCK_REALTIME};

/// Whole Unix seconds for the remaining calendar-time formatting interfaces.
/// Fractional times before the epoch round down, like the former C timestamp field.
pub fn unix_seconds(time: SystemTime) -> super::abi::time_t {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as super::abi::time_t,
        Err(error) => {
            let duration = error.duration();
            -(duration.as_secs() as super::abi::time_t) - i64::from(duration.subsec_nanos() != 0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn calendar_seconds_round_down_on_both_sides_of_epoch() {
        assert_eq!(unix_seconds(UNIX_EPOCH), 0);
        assert_eq!(
            unix_seconds(UNIX_EPOCH + Duration::from_micros(1_999_999)),
            1
        );
        assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_micros(1)), -1);
        assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_secs(1)), -1);
        assert_eq!(
            unix_seconds(UNIX_EPOCH - Duration::from_micros(1_000_001)),
            -2
        );
    }
}
