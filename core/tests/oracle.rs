//! Oracle validation for the Discord desktop reader.
//!
//! Two tiers (see `docs/validation.md`):
//!
//! * **Committed CI gate (tier-2 for the storage stack / tier-3 for the Discord
//!   decode):** mint a *real* LevelDB with an independent writer (`rusty-leveldb`),
//!   carrying Discord-shaped Local Storage records in the documented Chromium key
//!   format, then read them back through the full stack
//!   (`leveldb-core` → `chromium-storage-localstorage` → `discord-desktop-core`).
//!   The leveldb/localstorage layers are exercised against bytes this crate did
//!   not author; the Discord record *content* + expected answers are self-authored.
//!
//! * **Real-app oracle (tier-2, env-gated):** point `DISCORD_PROFILE` at a real
//!   Discord Electron profile dir; the test then asserts the reader recovers a
//!   token and account from genuine app-authored bytes. Skips cleanly when unset.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use discord_desktop_core::channel::RecentKind;
use discord_desktop_core::read_profile;
use std::path::Path;

/// Build a Chromium Local Storage **data key**: `_` + origin + NUL + Latin-1
/// marker + script-key bytes.
fn data_key(origin: &str, script_key: &str) -> Vec<u8> {
    let mut key = vec![b'_'];
    key.extend_from_slice(origin.as_bytes());
    key.push(0x00); // NUL separator
    key.push(0x01); // Latin-1 marker for the script key
    key.extend_from_slice(script_key.as_bytes());
    key
}

/// Build a Chromium Local Storage **value**: Latin-1 marker + body bytes.
fn latin1_value(value: &str) -> Vec<u8> {
    let mut v = vec![0x01];
    v.extend_from_slice(value.as_bytes());
    v
}

const ORIGIN: &str = "https://ptb.discord.com";
// A synthetic, non-credential fixture value (redaction only measures length).
const FIXTURE_TOKEN: &str = "SYNTHETIC-DISCORD-TOKEN-FIXTURE-not-a-real-credential-000";
const MULTI_ACCOUNT: &str = r#"{"_state":{"users":[{"id":"700000000000000000","username":"testuser","discriminator":"0","avatar":"abc","tokenStatus":2}]}}"#;
const SELECTED_GUILD: &str = r#"{"selectedGuildId":"81384788765712384"}"#;

/// Mint a real LevelDB at `<dir>/Local Storage/leveldb` carrying Discord-shaped
/// records, using the independent `rusty-leveldb` writer.
fn mint_discord_profile(dir: &Path) {
    let leveldb_dir = dir.join("Local Storage").join("leveldb");
    std::fs::create_dir_all(&leveldb_dir).unwrap();

    let opt = rusty_leveldb::Options {
        create_if_missing: true,
        ..Default::default()
    };
    let mut db = rusty_leveldb::DB::open(&leveldb_dir, opt).unwrap();

    db.put(&data_key(ORIGIN, "token"), &latin1_value(FIXTURE_TOKEN))
        .unwrap();
    db.put(
        &data_key(ORIGIN, "MultiAccountStore"),
        &latin1_value(MULTI_ACCOUNT),
    )
    .unwrap();
    db.put(
        &data_key(ORIGIN, "SelectedGuildStore"),
        &latin1_value(SELECTED_GUILD),
    )
    .unwrap();
    db.flush().unwrap();
    drop(db);
}

#[test]
fn minted_leveldb_round_trips_all_artifact_classes() {
    let tmp = tempfile::tempdir().unwrap();
    mint_discord_profile(tmp.path());

    let artifacts = read_profile(tmp.path()).expect("read minted Discord profile");

    // Token: present, redacted, correct length — never the secret.
    assert_eq!(artifacts.tokens.len(), 1, "token record");
    let token = &artifacts.tokens[0];
    assert_eq!(token.origin, ORIGIN);
    assert_eq!(token.token.len(), FIXTURE_TOKEN.len());
    assert!(!token.token.redacted().contains(FIXTURE_TOKEN));

    // Account identity.
    assert_eq!(artifacts.accounts.len(), 1, "account");
    let account = &artifacts.accounts[0];
    assert_eq!(account.user_id.as_deref(), Some("700000000000000000"));
    assert_eq!(account.username.as_deref(), Some("testuser"));
    assert_eq!(account.token_status, Some(2));

    // Recent guild.
    assert!(
        artifacts
            .recents
            .iter()
            .any(|r| r.kind == RecentKind::Guild && r.snowflake == "81384788765712384"),
        "recent guild snowflake, got {:?}",
        artifacts.recents
    );
}

/// Real-app oracle: run against a genuine Discord Electron profile when
/// `DISCORD_PROFILE` points at one (e.g. `~/Library/Application Support/discord`).
/// Skips cleanly when unset so the committed gate never depends on host state.
#[test]
fn real_discord_profile_when_present() {
    let Ok(profile) = std::env::var("DISCORD_PROFILE") else {
        eprintln!("DISCORD_PROFILE unset — skipping real-app oracle");
        return;
    };
    let artifacts = read_profile(Path::new(&profile))
        .expect("read the real Discord profile named by DISCORD_PROFILE");

    // A signed-in Discord install must carry a token and at least one origin META.
    assert!(
        !artifacts.tokens.is_empty(),
        "real profile should hold a token"
    );
    assert!(
        !artifacts.origins.is_empty(),
        "real profile should hold origin META"
    );
    // Never assert on the secret's value — only its presence + length.
    for token in &artifacts.tokens {
        assert!(!token.token.is_empty());
        eprintln!(
            "real token: origin={} len={} seq={} deleted={}",
            token.origin,
            token.token.len(),
            token.seq,
            token.deleted
        );
    }
    eprintln!(
        "real profile: {} accounts, {} recents, {} origins",
        artifacts.accounts.len(),
        artifacts.recents.len(),
        artifacts.origins.len()
    );
}
