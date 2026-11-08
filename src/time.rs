// Author: David M. Anderson
// Built with AI assistance (Claude, Anthropic)

use chrono::{DateTime, Utc};

pub fn instant(time: DateTime<Utc>) -> bz::Instant {
    bz::Instant(time.timestamp_micros() as f64 / 1e6)
}

pub fn utc(instant: bz::Instant) -> DateTime<Utc> {
    DateTime::from_timestamp_micros((instant.0 * 1e6).round() as i64).unwrap_or_default()
}

pub fn seconds(seconds: f64) -> chrono::Duration {
    chrono::Duration::microseconds((seconds * 1e6).round() as i64)
}
