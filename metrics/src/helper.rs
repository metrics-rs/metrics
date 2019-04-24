use std::{
    io::{Error, ErrorKind},
    time::Duration,
};

/// Helpers to create an I/O error from a string.
pub fn io_error(reason: &str) -> Error {
    Error::new(ErrorKind::Other, reason)
}

/// Converts a duration to nanoseconds.
pub fn duration_as_nanos(d: Duration) -> u64 {
    (d.as_secs() * 1_000_000_000) + u64::from(d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::duration_as_nanos;
    use std::time::Duration;

    #[test]
    fn test_simple_duration_as_nanos() {
        let d1 = Duration::from_secs(3);
        let d2 = Duration::from_millis(500);

        assert_eq!(duration_as_nanos(d1), 3_000_000_000);
        assert_eq!(duration_as_nanos(d2), 500_000_000);
    }
}
