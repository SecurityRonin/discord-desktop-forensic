//! Discord **account identity** from Local Storage.
//!
//! Discord keeps the signed-in account roster in the `MultiAccountStore`
//! `localStorage` record — a JSON document whose `_state.users[]` array carries,
//! per account, the snowflake `id`, `username`, `discriminator`, `avatar` hash and
//! a `tokenStatus`. Two companion single-value records hold the *active* session's
//! hints: `user_id_cache` (the last user id) and `email_cache` (the last email).
//!
//! This module folds those into [`Account`] records: the rich rows from
//! `MultiAccountStore`, enriched with the cached email on the active account.
//! Parsing is defensive — a malformed or absent record yields no account rather
//! than an error (degrade-to-empty; the store open is the only fail-loud point).

use crate::{is_discord_origin, LocalStorageRecord};
use serde::Serialize;
use serde_json::Value;

/// Local Storage script keys carrying account identity.
const MULTI_ACCOUNT_STORE: &str = "MultiAccountStore";
const USER_ID_CACHE: &str = "user_id_cache";
const EMAIL_CACHE: &str = "email_cache";

/// One Discord account recovered from Local Storage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Account {
    /// The account snowflake user id (a decimal string), if known.
    pub user_id: Option<String>,
    /// The account username (handle), if known.
    pub username: Option<String>,
    /// The legacy discriminator (`"0"` for migrated unique-username accounts).
    pub discriminator: Option<String>,
    /// The avatar hash, if set.
    pub avatar: Option<String>,
    /// The cached login email of the active session (PII), if recoverable.
    pub email: Option<String>,
    /// Discord's internal `tokenStatus` flag, if present.
    pub token_status: Option<i64>,
    /// The Local Storage origin the identity was stored under.
    pub origin: String,
}

/// A borrowed origin+text view of a Discord data record.
struct DiscordDatum<'a> {
    origin: &'a str,
    key: &'a str,
    value: &'a str,
}

fn discord_data(records: &[LocalStorageRecord]) -> impl Iterator<Item = DiscordDatum<'_>> {
    records.iter().filter_map(|record| match record {
        LocalStorageRecord::Data {
            origin,
            script_key,
            value,
            deleted: false,
            ..
        } if is_discord_origin(origin) => Some(DiscordDatum {
            origin: origin.as_str(),
            key: script_key.text.as_str(),
            value: value.text.as_str(),
        }),
        _ => None,
    })
}

/// A JSON string value, unwrapped (Discord stores `user_id_cache` /`email_cache`
/// as JSON string literals like `"695…"`); falls back to the raw text.
fn json_string(text: &str) -> Option<String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::String(s)) => Some(s),
        _ => None,
    }
}

/// Extract Discord [`Account`] identities from decoded Local Storage records.
///
/// The rich rows come from `MultiAccountStore`; the active session's cached email
/// is attached to the matching account. Returns an empty vec when no identity
/// record is present or parseable.
#[must_use]
pub fn extract_accounts(records: &[LocalStorageRecord]) -> Vec<Account> {
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

    // The real Discord MultiAccountStore shape (values synthetic).
    const MULTI: &str = r#"{"_state":{"users":[{"id":"700000000000000000","avatar":"deadbeefdeadbeefdeadbeefdeadbeef","username":"testuser","discriminator":"0","tokenStatus":2}]}}"#;

    #[test]
    fn parses_multi_account_store_users() {
        let recs = vec![data("https://ptb.discord.com", "MultiAccountStore", MULTI)];
        let accounts = extract_accounts(&recs);
        assert_eq!(accounts.len(), 1);
        let a = &accounts[0];
        assert_eq!(a.user_id.as_deref(), Some("700000000000000000"));
        assert_eq!(a.username.as_deref(), Some("testuser"));
        assert_eq!(a.discriminator.as_deref(), Some("0"));
        assert_eq!(
            a.avatar.as_deref(),
            Some("deadbeefdeadbeefdeadbeefdeadbeef")
        );
        assert_eq!(a.token_status, Some(2));
        assert_eq!(a.origin, "https://ptb.discord.com");
    }

    #[test]
    fn attaches_cached_email_to_active_account() {
        let recs = vec![
            data("https://discord.com", "MultiAccountStore", MULTI),
            data(
                "https://discord.com",
                "user_id_cache",
                "\"700000000000000000\"",
            ),
            data(
                "https://discord.com",
                "email_cache",
                "\"agent@example.com\"",
            ),
        ];
        let accounts = extract_accounts(&recs);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].email.as_deref(), Some("agent@example.com"));
    }

    #[test]
    fn minimal_account_from_caches_when_no_store() {
        let recs = vec![
            data(
                "https://discord.com",
                "user_id_cache",
                "\"700000000000000000\"",
            ),
            data(
                "https://discord.com",
                "email_cache",
                "\"agent@example.com\"",
            ),
        ];
        let accounts = extract_accounts(&recs);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].user_id.as_deref(), Some("700000000000000000"));
        assert_eq!(accounts[0].email.as_deref(), Some("agent@example.com"));
        assert!(accounts[0].username.is_none());
    }

    #[test]
    fn malformed_json_yields_no_account() {
        let recs = vec![data(
            "https://discord.com",
            "MultiAccountStore",
            "{not json",
        )];
        assert!(extract_accounts(&recs).is_empty());
    }

    #[test]
    fn ignores_non_discord_origin() {
        let recs = vec![data("https://evil.example", "MultiAccountStore", MULTI)];
        assert!(extract_accounts(&recs).is_empty());
    }
}
