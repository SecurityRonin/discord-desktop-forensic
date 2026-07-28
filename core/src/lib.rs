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
pub mod error;
pub mod token;

pub use error::Error;

use chromium_storage_localstorage::LocalStorageRecord;

/// Does this Local Storage origin belong to Discord?
///
/// Matches the stable client (`https://discord.com`) and the test-build variants
/// (`https://ptb.discord.com`, `https://canary.discord.com`) by the shared
/// `discord.com` host — the general rule, not one hardcoded origin.
#[must_use]
pub fn is_discord_origin(origin: &str) -> bool {
    origin.contains("discord.com")
}

/// The origin string of a record, if it carries one (a `Data` or `Meta` record).
fn record_origin(record: &LocalStorageRecord) -> Option<&str> {
    match record {
        LocalStorageRecord::Data { origin, .. } | LocalStorageRecord::Meta { origin, .. } => {
            Some(origin.as_str())
        }
        LocalStorageRecord::Other { .. } => None,
    }
}
