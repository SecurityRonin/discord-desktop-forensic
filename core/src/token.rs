//! The Discord auth **`token`** Local Storage record — a session credential.
//!
//! Discord persists the account's authentication token as a plain
//! `localStorage["token"]` string under the app origin. It is a bearer
//! credential: possession alone grants full account access, so it is the prime
//! target of Electron info-stealers (AhnLab ASEC, <https://asec.ahnlab.com/en/24512/>).
//!
//! This module surfaces the token's **presence, length, origin and LevelDB
//! sequence** — never the secret itself in any default output. The value is
//! wrapped in [`SensitiveToken`], which redacts in `Debug`, `Display`, and serde;
//! the raw secret is reachable only through the explicit [`SensitiveToken::reveal`]
//! opt-in (secure by default).

use crate::{is_discord_origin, LocalStorageRecord};
use serde::Serialize;

/// The Local Storage script key that holds the Discord auth token.
const TOKEN_KEY: &str = "token";

/// A Discord auth token, wrapped so it cannot leak by accident.
///
/// `Debug`, `Display`, and `Serialize` all emit the **redacted** form
/// (`<redacted N-char Discord token>`); the real secret is reachable only via the
/// explicit [`SensitiveToken::reveal`]. This is secure-by-default: a caller that
/// prints or serializes the record without reading the docs never leaks the token.
#[derive(Clone, PartialEq, Eq)]
pub struct SensitiveToken {
    raw: String,
}

impl SensitiveToken {
    /// Wrap a raw token string.
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }

    /// Length of the underlying token, in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// Whether the token string is empty (e.g. a cleared/tombstoned record).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    /// The redacted rendering — presence and length, never the secret.
    #[must_use]
    pub fn redacted(&self) -> String {
        format!("<redacted {}-char Discord token>", self.raw.len())
    }

    /// Reveal the raw token. **Explicit opt-in** — the only path to the secret.
    ///
    /// Use only where surfacing the credential is intended (e.g. an examiner who
    /// deliberately requests it); default output paths must use [`Self::redacted`].
    #[must_use]
    pub fn reveal(&self) -> &str {
        &self.raw
    }
}

impl std::fmt::Debug for SensitiveToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl std::fmt::Display for SensitiveToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl Serialize for SensitiveToken {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.redacted())
    }
}

/// A recovered Discord auth-token record.
///
/// The token value is a [`SensitiveToken`] (redacted by default). Superseded
/// versions and deletion tombstones surface too, each carrying its LevelDB `seq`
/// and `deleted` flag.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TokenRecord {
    /// The Local Storage origin the token was stored under (e.g.
    /// `https://ptb.discord.com`).
    pub origin: String,
    /// The token value — redacted in every default rendering.
    pub token: SensitiveToken,
    /// LevelDB sequence number (ordering; higher = newer).
    pub seq: u64,
    /// `true` if this is a deletion tombstone (the token was cleared).
    pub deleted: bool,
}

/// Extract every Discord `token` record from decoded Local Storage records.
///
/// Returns one [`TokenRecord`] per matching data record — including superseded
/// versions and tombstones — so the caller sees the full history. Records from
/// non-Discord origins are ignored.
#[must_use]
pub fn extract_tokens(records: &[LocalStorageRecord]) -> Vec<TokenRecord> {
    records
        .iter()
        .filter_map(|record| match record {
            LocalStorageRecord::Data {
                origin,
                script_key,
                value,
                seq,
                deleted,
            } if is_discord_origin(origin) && script_key.text == TOKEN_KEY => Some(TokenRecord {
                origin: origin.clone(),
                token: SensitiveToken::new(value.text.clone()),
                seq: *seq,
                deleted: *deleted,
            }),
            _ => None,
        })
        .collect()
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

    fn data(origin: &str, key: &str, value: &str, seq: u64) -> LocalStorageRecord {
        LocalStorageRecord::Data {
            origin: origin.to_string(),
            script_key: sv(key),
            value: sv(value),
            seq,
            deleted: false,
        }
    }

    // A structurally-realistic Discord token: MTk4... base64-ish, 3 dot-separated
    // parts. NOT a real credential — a synthetic fixture in the documented shape.
    const FAKE_TOKEN: &str = "EXAMPLE-synthetic-fixture.not-real.discord-token-value-for-tests";

    #[test]
    fn extracts_token_and_redacts_by_default() {
        let records = vec![
            data("https://ptb.discord.com", "token", FAKE_TOKEN, 42),
            data(
                "https://ptb.discord.com",
                "SelectedGuildStore",
                "\"123\"",
                7,
            ),
        ];

        let tokens = extract_tokens(&records);
        assert_eq!(tokens.len(), 1, "exactly one token record");
        let t = &tokens[0];
        assert_eq!(t.origin, "https://ptb.discord.com");
        assert_eq!(t.seq, 42);
        assert!(!t.deleted);
        assert_eq!(t.token.len(), FAKE_TOKEN.len());

        // Secure-by-default: no default rendering may leak the secret.
        let redacted = t.token.redacted();
        assert!(
            !redacted.contains(FAKE_TOKEN),
            "redacted must not contain the secret"
        );
        assert!(
            redacted.contains(&FAKE_TOKEN.len().to_string()),
            "redacted shows length"
        );
        assert_eq!(format!("{}", t.token), redacted, "Display is redacted");
        assert_eq!(format!("{:?}", t.token), redacted, "Debug is redacted");

        // The secret is reachable only via the explicit opt-in.
        assert_eq!(t.token.reveal(), FAKE_TOKEN);
    }

    #[test]
    fn ignores_non_discord_origins_and_non_token_keys() {
        let records = vec![
            data("https://example.com", "token", FAKE_TOKEN, 1),
            data(
                "https://ptb.discord.com",
                "user_id_cache",
                "\"695526392107892786\"",
                2,
            ),
        ];
        assert!(extract_tokens(&records).is_empty());
    }

    #[test]
    fn serde_output_redacts_the_secret() {
        let records = vec![data("https://discord.com", "token", FAKE_TOKEN, 5)];
        let tokens = extract_tokens(&records);
        let json = serde_json::to_string(&tokens[0]).unwrap();
        assert!(
            !json.contains(FAKE_TOKEN),
            "serialized JSON must not leak the token"
        );
        assert!(json.contains("redacted"));
    }
}
