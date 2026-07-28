//! Timeline reconstruction from decoded Discord artifacts.
//!
//! Datable Discord evidence in Local Storage is sparse — Discord keeps no message
//! DB — but two kinds of time survive: each recovered snowflake embeds its
//! **creation** time, and each origin's `META:` record carries the store's
//! **last-modified** time (a WebKit microsecond timestamp). This module folds both
//! into the fleet [`TimelineEvent`] model, ascending by time.

use crate::DiscordArtifacts;
use chrono::{DateTime, Utc};
use forensicnomicon_core::report::TimelineEvent;

/// Microseconds between the WebKit/Windows epoch (1601-01-01) and the Unix epoch
/// (1970-01-01). A `META:` last-modified value is WebKit microseconds.
pub const WEBKIT_EPOCH_OFFSET_MICROS: u64 = 11_644_473_600_000_000;

/// The analyzer name stamped on each timeline event.
const SOURCE: &str = "discord-desktop";

/// Convert a WebKit-microsecond timestamp to Unix milliseconds.
///
/// Returns `None` for values at/below the epoch offset (a zero/garbage timestamp),
/// so a bogus record never becomes a 1601-era event (fail closed, not loud —
/// per-record miss).
#[must_use]
pub fn webkit_micros_to_unix_ms(webkit_micros: u64) -> Option<u64> {
    webkit_micros
        .checked_sub(WEBKIT_EPOCH_OFFSET_MICROS)
        .map(|unix_micros| unix_micros / 1000)
}

/// Render Unix milliseconds as an RFC 3339 UTC string, if representable.
#[must_use]
pub fn unix_ms_to_rfc3339(unix_ms: u64) -> Option<String> {
    let millis = i64::try_from(unix_ms).ok()?;
    DateTime::<Utc>::from_timestamp_millis(millis).map(|dt| dt.to_rfc3339())
}

/// Build the ascending timeline for a set of decoded artifacts.
#[must_use]
pub fn build_timeline(artifacts: &DiscordArtifacts) -> Vec<TimelineEvent> {
    let mut dated: Vec<(u64, TimelineEvent)> = Vec::new();

    for recent in &artifacts.recents {
        if let Some(ms) = recent.created_at_unix_ms {
            dated.push((
                ms,
                TimelineEvent {
                    when: unix_ms_to_rfc3339(ms),
                    source: SOURCE.to_string(),
                    event: format!(
                        "Recent {:?} {} (snowflake creation time)",
                        recent.kind, recent.snowflake
                    ),
                },
            ));
        }
    }

    for meta in &artifacts.origins {
        if let Some(ms) = meta.last_modified_unix_ms {
            dated.push((
                ms,
                TimelineEvent {
                    when: unix_ms_to_rfc3339(ms),
                    source: SOURCE.to_string(),
                    event: format!("Local Storage last modified for {}", meta.origin),
                },
            ));
        }
    }

    dated.sort_by_key(|(ms, _)| *ms);
    dated.into_iter().map(|(_, event)| event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_conversion_matches_known_value() {
        // 13290000000000000 µs WebKit = 2022-01-... ; below-offset returns None.
        assert_eq!(webkit_micros_to_unix_ms(0), None);
        assert_eq!(
            webkit_micros_to_unix_ms(WEBKIT_EPOCH_OFFSET_MICROS),
            Some(0)
        );
        // 1 second after Unix epoch.
        assert_eq!(
            webkit_micros_to_unix_ms(WEBKIT_EPOCH_OFFSET_MICROS + 1_000_000),
            Some(1000)
        );
    }

    #[test]
    fn rfc3339_round_trips_epoch() {
        assert_eq!(
            unix_ms_to_rfc3339(0).as_deref(),
            Some("1970-01-01T00:00:00+00:00")
        );
    }
}
