//! Calendar-time ABI declarations and conversions for Rust timestamps.
use std::ffi::CString;
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

/// `tm` in the fixed C-locale layout of `asctime`, without its newline.
pub fn asctime_text(tm: &tm) -> CString {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let name = |names: &[&'static str], index: i32| {
        usize::try_from(index)
            .ok()
            .and_then(|index| names.get(index).copied())
            .unwrap_or("???")
    };
    let text = format!(
        "{} {}{:3} {:02}:{:02}:{:02} {}",
        name(&DAYS, tm.tm_wday),
        name(&MONTHS, tm.tm_mon),
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        1900 + i64::from(tm.tm_year)
    );
    CString::new(text).expect("formatted time contains no NUL")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn asctime_text_uses_fixed_names_and_widths() {
        let tm = tiri_rt::unix::utc_time(1_700_000_000).unwrap();
        assert_eq!(asctime_text(&tm).as_bytes(), b"Tue Nov 14 22:13:20 2023");
        let tm = tiri_rt::unix::utc_time(0).unwrap();
        assert_eq!(asctime_text(&tm).as_bytes(), b"Thu Jan  1 00:00:00 1970");
    }

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
