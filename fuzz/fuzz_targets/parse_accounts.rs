#![no_main]
//! Fuzz the Discord **record-value JSON parsers** with arbitrary UTF-8: the
//! `MultiAccountStore` account parser, the recent-store snowflake collector, and
//! token extraction. Invariant: never panic on malformed / adversarial JSON.

use chromium_storage_localstorage::{Encoding, LocalStorageRecord, StorageValue};
use discord_desktop_core::{account, channel, token};
use libfuzzer_sys::fuzz_target;

fn value(text: &str) -> StorageValue {
    StorageValue {
        text: text.to_string(),
        raw: text.as_bytes().to_vec(),
        encoding: Encoding::Latin1,
        lossy: false,
    }
}

fn datum(key: &str, text: &str) -> LocalStorageRecord {
    LocalStorageRecord::Data {
        origin: "https://discord.com".to_string(),
        script_key: value(key),
        value: value(text),
        seq: 1,
        deleted: false,
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    // Feed the arbitrary text through every key-specific parser.
    let records = vec![
        datum("MultiAccountStore", text),
        datum("SelectedGuildStore", text),
        datum("RecentVoiceChannelStore", text),
        datum("user_id_cache", text),
        datum("email_cache", text),
        datum("token", text),
    ];
    let _ = account::extract_accounts(&records);
    let _ = channel::extract_recent_channels(&records);
    let _ = token::extract_tokens(&records);
});
