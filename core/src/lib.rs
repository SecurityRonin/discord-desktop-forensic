//! Discord **desktop** (Electron/Chromium) artifact reader.
//!
//! Discord is an Electron app: its account/session state lives in Chromium
//! **Local Storage** (a per-profile LevelDB at `Local Storage/leveldb/`), not in a
//! chat database — Discord keeps **no local message DB** (messages are fetched
//! from the server and only cached). This crate sits on top of the Wave-2
//! [`chromium_storage_localstorage`] reader and interprets the Discord-specific
//! records inside it:
//!
//! * the auth **`token`** record — a session credential, surfaced as a
//!   [`token::TokenRecord`] whose value is a [`token::SensitiveToken`] that
//!   **redacts by default** (presence + length + location, never the secret in
//!   `Debug`/`Display`/serde output);
//! * **account identity** — the `MultiAccountStore` / `user_id_cache` /
//!   `email_cache` records, parsed into [`account::Account`];
//! * **recent channels / guilds** — the `SelectedGuildStore` /
//!   `RecentVoiceChannelStore` records, parsed into [`channel::RecentChannel`].
//!
//! The Discord store paths and the Chromium Local Storage key/encoding constants
//! come from the fleet KNOWLEDGE leaf [`forensicnomicon_core::messenger_desktop`]
//! / [`forensicnomicon_core::chromium_local_storage`] — this crate re-uses them
//! and never re-hardcodes a path or a marker byte.
//!
//! # Sources
//!
//! - AhnLab ASEC, *Distribution of Infostealer Made with Electron* — the Discord
//!   `Local Storage\leveldb` token store and the `discordptb`/`discordcanary`
//!   variants: <https://asec.ahnlab.com/en/24512/>
//! - forensafe, *Investigating Discord*: <https://www.forensafe.com/blogs/discord.html>
//! - Discord snowflake ID epoch (2015-01-01): <https://discord.com/developers/docs/reference#snowflakes>
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod account;
pub mod channel;
pub mod error;
pub mod timeline;
pub mod token;

pub use account::Account;
pub use channel::RecentChannel;
pub use error::Error;
pub use token::TokenRecord;

use chromium_storage_localstorage::LocalStorageRecord;
use forensicnomicon_core::messenger_desktop::{self, StoreRole};
use forensicnomicon_core::report::TimelineEvent;
use serde::Serialize;
use std::path::Path;

/// The canonical app name for the Discord messenger spec in the KNOWLEDGE leaf.
const DISCORD_APP: &str = "Discord";

/// Does this Local Storage origin belong to Discord?
///
/// Matches the stable client (`https://discord.com`) and the test-build variants
/// (`https://ptb.discord.com`, `https://canary.discord.com`) by the shared
/// `discord.com` host — the general rule, not one hardcoded origin.
#[must_use]
pub fn is_discord_origin(origin: &str) -> bool {
    origin.contains("discord.com")
}

/// A per-origin Local Storage metadata record (`META:`) — the store's declared
/// size and its last-modified time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OriginMeta {
    /// The Discord origin this metadata describes.
    pub origin: String,
    /// Last-modified time as Unix milliseconds (decoded from WebKit µs), when
    /// representable.
    pub last_modified_unix_ms: Option<u64>,
    /// Declared store size in bytes, if the record carried it.
    pub size: Option<u64>,
    /// LevelDB sequence number.
    pub seq: u64,
}

/// Extract per-origin `META:` metadata for Discord origins.
#[must_use]
pub fn extract_origin_meta(records: &[LocalStorageRecord]) -> Vec<OriginMeta> {
    records
        .iter()
        .filter_map(|record| match record {
            LocalStorageRecord::Meta {
                origin,
                timestamp_webkit_micros,
                size,
                seq,
                deleted: false,
            } if is_discord_origin(origin) => Some(OriginMeta {
                origin: origin.clone(),
                last_modified_unix_ms: timeline::webkit_micros_to_unix_ms(*timestamp_webkit_micros),
                size: *size,
                seq: *seq,
            }),
            _ => None,
        })
        .collect()
}

/// Everything the reader recovered from a Discord Local Storage store.
///
/// A missing or malformed individual record degrades to an empty vec (the store
/// open in [`read_profile`] is the only fail-loud point).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DiscordArtifacts {
    /// Auth-token records (secret redacted by default). Superseded versions and
    /// tombstones surface too.
    pub tokens: Vec<TokenRecord>,
    /// Recovered account identities.
    pub accounts: Vec<Account>,
    /// Recent guild / voice-channel references.
    pub recents: Vec<RecentChannel>,
    /// Per-origin store metadata (last-modified time / size).
    pub origins: Vec<OriginMeta>,
}

impl DiscordArtifacts {
    /// The ascending timeline reconstructed from these artifacts (snowflake
    /// creation times + per-origin last-modified times).
    #[must_use]
    pub fn timeline(&self) -> Vec<TimelineEvent> {
        timeline::build_timeline(self)
    }
}

/// Interpret already-decoded Local Storage records into typed Discord artifacts.
///
/// Pure and I/O-free — the counterpart to [`read_profile`] for callers that
/// already hold records from the [`chromium_storage_localstorage`] reader.
#[must_use]
pub fn decode_local_storage(records: &[LocalStorageRecord]) -> DiscordArtifacts {
    DiscordArtifacts::default()
}

/// Read and interpret a Discord desktop **profile directory** (the Electron
/// `userData` dir, e.g. `~/Library/Application Support/discord`).
///
/// The Local Storage sub-path comes from the fleet KNOWLEDGE leaf
/// ([`messenger_desktop`]'s `Discord` Account store), never a hardcoded literal.
/// A failure to open the LevelDB is a **loud, typed error** (bootstrap failure);
/// an absent/empty store is not — it yields empty artifacts.
pub fn read_profile(profile_dir: &Path) -> Result<DiscordArtifacts, Error> {
    let spec = messenger_desktop::spec(DISCORD_APP).ok_or(Error::SpecMissing)?;
    let store = spec
        .store(StoreRole::Account)
        .ok_or(Error::StoreMissing("Account"))?;

    // `relative_path` is forward-slash separated for OS neutrality; join each
    // component so it resolves natively on Windows too.
    let mut leveldb_dir = profile_dir.to_path_buf();
    for component in store.relative_path.split('/') {
        leveldb_dir.push(component);
    }

    let records = chromium_storage_localstorage::read_dir(&leveldb_dir)?;
    Ok(decode_local_storage(&records))
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

    fn data(key: &str, value: &str) -> LocalStorageRecord {
        LocalStorageRecord::Data {
            origin: "https://ptb.discord.com".to_string(),
            script_key: sv(key),
            value: sv(value),
            seq: 1,
            deleted: false,
        }
    }

    const MULTI: &str = r#"{"_state":{"users":[{"id":"700000000000000000","username":"testuser","discriminator":"0","avatar":"a","tokenStatus":2}]}}"#;

    fn sample_records() -> Vec<LocalStorageRecord> {
        vec![
            data("token", "MTk4NjIyNDgzNDcxOTI1MjQ4.fake.synthetic"),
            data("MultiAccountStore", MULTI),
            data(
                "SelectedGuildStore",
                r#"{"selectedGuildId":"81384788765712384"}"#,
            ),
            LocalStorageRecord::Meta {
                origin: "https://ptb.discord.com".to_string(),
                timestamp_webkit_micros: timeline::WEBKIT_EPOCH_OFFSET_MICROS
                    + 1_600_000_000_000_000,
                size: Some(4096),
                seq: 99,
                deleted: false,
            },
        ]
    }

    #[test]
    fn decode_aggregates_all_artifact_classes() {
        let artifacts = decode_local_storage(&sample_records());
        assert_eq!(artifacts.tokens.len(), 1, "token");
        assert_eq!(artifacts.accounts.len(), 1, "account");
        assert_eq!(artifacts.recents.len(), 1, "recent");
        assert_eq!(artifacts.origins.len(), 1, "origin meta");
        assert_eq!(artifacts.accounts[0].username.as_deref(), Some("testuser"));
    }

    #[test]
    fn timeline_is_sorted_and_dated() {
        let artifacts = decode_local_storage(&sample_records());
        let events = artifacts.timeline();
        // One snowflake-creation event + one origin last-modified event.
        assert_eq!(events.len(), 2);
        assert!(
            events.iter().all(|e| e.when.is_some()),
            "all events datable"
        );
        let whens: Vec<_> = events.iter().filter_map(|e| e.when.clone()).collect();
        let mut sorted = whens.clone();
        sorted.sort();
        assert_eq!(whens, sorted, "ascending by time");
    }

    #[test]
    fn read_profile_missing_store_errors_loud() {
        // A path with no Local Storage/leveldb is a bootstrap failure, not empty.
        let dir = std::path::Path::new("/nonexistent/discord/profile");
        assert!(matches!(read_profile(dir), Err(Error::LocalStorage(_))));
    }
}
