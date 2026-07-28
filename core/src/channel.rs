//! Discord **recent channels / guilds** from Local Storage.
//!
//! Discord caches the user's navigation state in `localStorage`: the
//! `SelectedGuildStore` record (the last-selected guild/server) and the
//! `RecentVoiceChannelStore` record (recently-joined voice channels). Their values
//! are JSON documents embedding Discord **snowflake** ids — 64-bit ids whose high
//! bits encode the creation time.
//!
//! This module pulls every snowflake out of those records into [`RecentChannel`]
//! entries and decodes each snowflake's embedded creation timestamp
//! ([`snowflake_timestamp_ms`]). Extraction is schema-tolerant: it collects any
//! snowflake-shaped id anywhere in the JSON rather than pinning to one exact
//! layout, so a client version that reshapes the record still yields the ids.
//!
//! Snowflake reference: <https://discord.com/developers/docs/reference#snowflakes>

use crate::{is_discord_origin, LocalStorageRecord};
use serde::Serialize;
use serde_json::Value;

/// Local Storage script keys that carry recent channel/guild navigation.
const SELECTED_GUILD_STORE: &str = "SelectedGuildStore";
const RECENT_VOICE_CHANNEL_STORE: &str = "RecentVoiceChannelStore";

/// Discord snowflake epoch: the first second of 2015, in Unix milliseconds.
/// A snowflake's timestamp is `(id >> 22) + DISCORD_EPOCH_MS`.
pub const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// Decode the creation time embedded in a Discord snowflake id, as Unix ms.
#[must_use]
pub fn snowflake_timestamp_ms(id: u64) -> u64 {
    (id >> 22) + DISCORD_EPOCH_MS
}

/// What a [`RecentChannel`] entry refers to — the store it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum RecentKind {
    /// A guild (server) id from `SelectedGuildStore`.
    Guild,
    /// A voice-channel id from `RecentVoiceChannelStore`.
    VoiceChannel,
}

/// A recent channel/guild reference recovered from Local Storage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecentChannel {
    /// The Discord snowflake id (decimal string, as stored).
    pub snowflake: String,
    /// Whether this is a guild or a voice channel.
    pub kind: RecentKind,
    /// The Local Storage origin the reference was stored under.
    pub origin: String,
    /// The snowflake's embedded creation time, Unix ms (decoded), when the id
    /// parses as a `u64`.
    pub created_at_unix_ms: Option<u64>,
}

/// Is `s` a Discord snowflake — a 17–20 digit decimal that fits in a `u64`?
fn is_snowflake(s: &str) -> bool {
    (17..=20).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_digit())
        && s.parse::<u64>().is_ok()
}

/// Collect every snowflake-shaped string anywhere in a JSON value (recursively),
/// in document order.
fn collect_snowflakes(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) if is_snowflake(s) => out.push(s.clone()),
        Value::Array(items) => {
            for item in items {
                collect_snowflakes(item, out);
            }
        }
        Value::Object(map) => {
            for (key, v) in map {
                // Object keys are frequently snowflakes too (id-keyed maps).
                if is_snowflake(key) {
                    out.push(key.clone());
                }
                collect_snowflakes(v, out);
            }
        }
        _ => {}
    }
}

/// Extract Discord recent-channel / recent-guild references from decoded Local
/// Storage records. Returns an empty vec when no recent-store record is present
/// or parseable.
#[must_use]
pub fn extract_recent_channels(records: &[LocalStorageRecord]) -> Vec<RecentChannel> {
    let _ = records;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chromium_storage_localstorage::{Encoding, StorageValue};

    fn sv(text: &str) -> StorageValue {
        StorageValue {
            text: text.to_string(),
            raw: text.as_bytes().to_vec(),
            encoding: Encoding::Latin1,
            lossy: false,
        }
    }

    fn data(origin: &str, key: &str, value: &str) -> LocalStorageRecord {
        LocalStorageRecord::Data {
            origin: origin.to_string(),
            script_key: sv(key),
            value: sv(value),
            seq: 1,
            deleted: false,
        }
    }

    #[test]
    fn snowflake_decodes_to_plausible_time() {
        // 700000000000000000 — a real-shape snowflake; decodes to ~2020.
        let ms = snowflake_timestamp_ms(695_526_392_107_892_786);
        assert!(ms > DISCORD_EPOCH_MS);
        assert!(
            (1_580_000_000_000..=1_620_000_000_000).contains(&ms),
            "≈2020: {ms}"
        );
    }

    #[test]
    fn extracts_guild_and_voice_snowflakes() {
        let recs = vec![
            data(
                "https://ptb.discord.com",
                "SelectedGuildStore",
                r#"{"selectedGuildId":"81384788765712384"}"#,
            ),
            data(
                "https://ptb.discord.com",
                "RecentVoiceChannelStore",
                r#"{"recentChannels":["96628290369007616","155361364909588482"]}"#,
            ),
        ];
        let recents = extract_recent_channels(&recs);
        assert_eq!(recents.len(), 3);

        let guilds: Vec<_> = recents
            .iter()
            .filter(|r| r.kind == RecentKind::Guild)
            .collect();
        assert_eq!(guilds.len(), 1);
        assert_eq!(guilds[0].snowflake, "81384788765712384");
        assert!(guilds[0].created_at_unix_ms.is_some());

        let voice: Vec<_> = recents
            .iter()
            .filter(|r| r.kind == RecentKind::VoiceChannel)
            .collect();
        assert_eq!(voice.len(), 2);
        assert_eq!(voice[0].snowflake, "96628290369007616");
    }

    #[test]
    fn skips_non_snowflake_numbers() {
        // tokenStatus-like small ints and non-Discord origins must not appear.
        let recs = vec![
            data(
                "https://discord.com",
                "SelectedGuildStore",
                r#"{"v":2,"n":"abc"}"#,
            ),
            data(
                "https://evil.example",
                "SelectedGuildStore",
                r#"{"g":"81384788765712384"}"#,
            ),
        ];
        assert!(extract_recent_channels(&recs).is_empty());
    }

    #[test]
    fn malformed_json_yields_nothing() {
        let recs = vec![data(
            "https://discord.com",
            "RecentVoiceChannelStore",
            "{bad",
        )];
        assert!(extract_recent_channels(&recs).is_empty());
    }
}
