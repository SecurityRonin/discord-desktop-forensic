//! Discord **recent channels / guilds** from Local Storage.
//!
//! Discord caches the user's navigation state in `localStorage`: the
//! `SelectedGuildStore` record (guild selection, with the time of each selection)
//! and the `RecentVoiceChannelStore` record (recent voice **and text** channels).
//! Their values are JSON documents — a Flux store payload wrapped in `_state` —
//! embedding Discord **snowflake** ids, 64-bit ids whose high bits encode the id's
//! creation time.
//!
//! Each [`RecentChannel`] therefore carries **two independent times**, never
//! conflated:
//!
//! * [`RecentChannel::selected_at_unix_ms`] — the **activity** time, read from
//!   `SelectedGuildStore._state.selectedGuildTimestampMillis` (a guildId → epoch
//!   milliseconds map of when each guild was last selected). `None` when the store
//!   recorded none for that id; the creation time is never substituted for it.
//! * [`RecentChannel::created_at_unix_ms`] — the **creation** time of the entity
//!   the id names, decoded from the snowflake itself ([`snowflake_timestamp_ms`]).
//!   For a guild joined long after it was created the two are years apart.
//!
//! Ids are attributed by the documented layout — the guild-timestamp map and
//! `selectedGuildId`/`lastSelectedGuildId` for guilds, `voiceChannelHistory` and
//! `textChannelHistory` for channels — so the *store* name never decides the kind
//! of an id inside it. Extraction stays schema-tolerant: any other
//! snowflake-shaped id anywhere in the record still surfaces, as
//! [`RecentKind::Unattributed`] rather than guessed into a kind.
//!
//! # Sources
//!
//! - Snowflake layout / epoch: <https://discord.com/developers/docs/reference#snowflakes>
//! - Store keys and the selection-time semantics follow DLEAPP's
//!   `scripts/artifacts/discordLocalStorage.py` (`discordActivity`):
//!   <https://github.com/abrignoni/DLEAPP>

use crate::{is_discord_origin, LocalStorageRecord};
use serde::Serialize;
use serde_json::Value;

/// Local Storage script keys that carry recent channel/guild navigation.
const SELECTED_GUILD_STORE: &str = "SelectedGuildStore";
const RECENT_VOICE_CHANNEL_STORE: &str = "RecentVoiceChannelStore";

/// Keys inside `SelectedGuildStore._state`.
const SELECTED_GUILD_TIMESTAMP_MILLIS: &str = "selectedGuildTimestampMillis";
const SELECTED_GUILD_ID: &str = "selectedGuildId";
const LAST_SELECTED_GUILD_ID: &str = "lastSelectedGuildId";

/// Keys inside `RecentVoiceChannelStore._state` — the store holds two lists.
const VOICE_CHANNEL_HISTORY: &str = "voiceChannelHistory";
const TEXT_CHANNEL_HISTORY: &str = "textChannelHistory";

/// Discord snowflake epoch: the first second of 2015, in Unix milliseconds.
/// A snowflake's timestamp is `(id >> 22) + DISCORD_EPOCH_MS`.
pub const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// Decode the **creation** time embedded in a Discord snowflake id, as Unix ms.
///
/// This is when the guild/channel/user the id names came into existence — not
/// when it was last visited. For an activity time use
/// [`RecentChannel::selected_at_unix_ms`].
#[must_use]
pub fn snowflake_timestamp_ms(id: u64) -> u64 {
    (id >> 22) + DISCORD_EPOCH_MS
}

/// What a [`RecentChannel`] entry refers to.
///
/// `#[non_exhaustive]`: Discord adds Local-Storage record shapes over time, so new
/// kinds are expected. Marking it here (in the same breaking release that added
/// `TextChannel`/`Unattributed`) makes every future variant additive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum RecentKind {
    /// A guild (server) id — from the guild-timestamp map, `selectedGuildId` or
    /// `lastSelectedGuildId`.
    Guild,
    /// A voice-channel id — from `RecentVoiceChannelStore._state.voiceChannelHistory`.
    VoiceChannel,
    /// A text-channel id — from `RecentVoiceChannelStore._state.textChannelHistory`.
    TextChannel,
    /// A snowflake recovered from a recent-navigation record whose role in the
    /// record's layout could not be determined (a reshaped client version, a key
    /// this reader does not model). The id is reported rather than dropped, and
    /// deliberately not labelled guild/voice/text.
    Unattributed,
}

/// A recent channel/guild reference recovered from Local Storage.
///
/// `#[non_exhaustive]`: further per-entry facts (selection counts, guild ids for
/// channels) are expected as more record shapes are modelled. Marking it in the
/// same breaking release as `selected_at_unix_ms` makes future fields additive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct RecentChannel {
    /// The Discord snowflake id (decimal string, as stored).
    pub snowflake: String,
    /// What the id refers to, per the record's documented layout.
    pub kind: RecentKind,
    /// The Local Storage origin the reference was stored under.
    pub origin: String,
    /// The **creation** time of the entity the id names: the snowflake's embedded
    /// timestamp, Unix ms, when the id parses as a `u64`. Not an activity time.
    pub created_at_unix_ms: Option<u64>,
    /// The **activity** time Discord recorded for this id — when the guild was
    /// last selected, Unix ms, from `selectedGuildTimestampMillis`. `None` when
    /// the store holds no selection time for it (never back-filled from
    /// [`Self::created_at_unix_ms`], which is a different fact).
    pub selected_at_unix_ms: Option<u64>,
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

/// Which recent-navigation store a record came from.
#[derive(Clone, Copy)]
enum RecentStore {
    /// `SelectedGuildStore` — guild selection, with per-guild selection times.
    SelectedGuild,
    /// `RecentVoiceChannelStore` — recent voice **and** text channel history.
    RecentVoiceChannel,
}

/// Unwrap a Flux store payload: current clients serialize `{"_state": {…}}`,
/// older ones store the state object bare.
fn state_of(value: &Value) -> &Value {
    value.get("_state").unwrap_or(value)
}

/// Read an epoch-millisecond value, which real records carry as a JSON number
/// and some client versions as a decimal string.
fn epoch_ms(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

/// Add one entry, keeping the first (most specific) attribution of an id within a
/// record — the guild-timestamp map is walked before the bare selected-guild keys,
/// so an id with a recorded selection time keeps it.
fn push_recent(
    out: &mut Vec<RecentChannel>,
    origin: &str,
    snowflake: String,
    kind: RecentKind,
    selected_at_unix_ms: Option<u64>,
) {
    if out.iter().any(|entry| entry.snowflake == snowflake) {
        return;
    }
    let created_at_unix_ms = snowflake.parse::<u64>().ok().map(snowflake_timestamp_ms);
    out.push(RecentChannel {
        snowflake,
        kind,
        origin: origin.to_string(),
        created_at_unix_ms,
        selected_at_unix_ms,
    });
}

/// Attribute the ids a store's documented layout gives a kind — and, for a guild,
/// the selection time recorded alongside it.
fn attributed_recents(
    store: RecentStore,
    state: &Value,
    origin: &str,
    out: &mut Vec<RecentChannel>,
) {
    match store {
        RecentStore::SelectedGuild => {
            // guildId -> epoch ms of the last selection: the activity time.
            if let Some(map) = state
                .get(SELECTED_GUILD_TIMESTAMP_MILLIS)
                .and_then(Value::as_object)
            {
                for (guild_id, when) in map {
                    if is_snowflake(guild_id) {
                        push_recent(
                            out,
                            origin,
                            guild_id.clone(),
                            RecentKind::Guild,
                            epoch_ms(when),
                        );
                    }
                }
            }
            // The currently- and last-selected guilds carry no time of their own.
            for key in [SELECTED_GUILD_ID, LAST_SELECTED_GUILD_ID] {
                if let Some(id) = state.get(key).and_then(Value::as_str) {
                    if is_snowflake(id) {
                        push_recent(out, origin, id.to_string(), RecentKind::Guild, None);
                    }
                }
            }
        }
        RecentStore::RecentVoiceChannel => {
            // The store name names only its first list; the second holds TEXT
            // channels, so the kind comes from the list, never the store.
            for (key, kind) in [
                (VOICE_CHANNEL_HISTORY, RecentKind::VoiceChannel),
                (TEXT_CHANNEL_HISTORY, RecentKind::TextChannel),
            ] {
                let ids = state.get(key).and_then(Value::as_array);
                for id in ids.into_iter().flatten().filter_map(Value::as_str) {
                    if is_snowflake(id) {
                        push_recent(out, origin, id.to_string(), kind, None);
                    }
                }
            }
        }
    }
}

/// Parse one recent-store JSON value into attributed [`RecentChannel`] entries.
fn recents_from_store(origin: &str, store: RecentStore, json: &str) -> Vec<RecentChannel> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    attributed_recents(store, state_of(&value), origin, &mut out);

    // Schema tolerance: a reshaped record must still yield its ids, but an id
    // whose role in the layout is unknown is reported as such, never guessed.
    let mut swept = Vec::new();
    collect_snowflakes(&value, &mut swept);
    for snowflake in swept {
        push_recent(&mut out, origin, snowflake, RecentKind::Unattributed, None);
    }
    out
}

/// Extract Discord recent-channel / recent-guild references from decoded Local
/// Storage records. Returns an empty vec when no recent-store record is present
/// or parseable.
#[must_use]
pub fn extract_recent_channels(records: &[LocalStorageRecord]) -> Vec<RecentChannel> {
    records
        .iter()
        .filter_map(|record| match record {
            LocalStorageRecord::Data {
                origin,
                script_key,
                value,
                deleted: false,
                ..
            } if is_discord_origin(origin) => {
                let store = match script_key.text.as_str() {
                    SELECTED_GUILD_STORE => RecentStore::SelectedGuild,
                    RECENT_VOICE_CHANNEL_STORE => RecentStore::RecentVoiceChannel,
                    _ => return None,
                };
                Some(recents_from_store(origin, store, &value.text))
            }
            _ => None,
        })
        .flatten()
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
    fn extracts_guild_voice_and_text_snowflakes() {
        // `RecentVoiceChannelStore` holds two lists: the kind of each id comes
        // from the list it sits in, not from the store's name.
        let recs = vec![
            data(
                "https://ptb.discord.com",
                "SelectedGuildStore",
                r#"{"selectedGuildId":"81384788765712384"}"#,
            ),
            data(
                "https://ptb.discord.com",
                "RecentVoiceChannelStore",
                r#"{"_state":{"voiceChannelHistory":["96628290369007616"],"textChannelHistory":["155361364909588482"]}}"#,
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
        // No selection time was recorded for it — never back-filled from creation.
        assert_eq!(guilds[0].selected_at_unix_ms, None);

        let voice: Vec<_> = recents
            .iter()
            .filter(|r| r.kind == RecentKind::VoiceChannel)
            .collect();
        assert_eq!(voice.len(), 1);
        assert_eq!(voice[0].snowflake, "96628290369007616");

        let text: Vec<_> = recents
            .iter()
            .filter(|r| r.kind == RecentKind::TextChannel)
            .collect();
        assert_eq!(text.len(), 1);
        assert_eq!(text[0].snowflake, "155361364909588482");
    }

    #[test]
    fn an_id_in_both_history_lists_is_reported_once_per_list() {
        // A channel the user both spoke and typed in is listed by Discord in
        // BOTH `voiceChannelHistory` and `textChannelHistory`. The two
        // appearances are separate facts about separate activity, so neither may
        // be dropped as a duplicate of the other (DLEAPP emits a row per list).
        let recs = vec![data(
            "https://discord.com",
            "RecentVoiceChannelStore",
            r#"{"_state":{"voiceChannelHistory":["700000000000000004"],"textChannelHistory":["700000000000000004"]}}"#,
        )];
        let recents = extract_recent_channels(&recs);
        assert!(
            recents
                .iter()
                .any(|r| r.snowflake == "700000000000000004" && r.kind == RecentKind::VoiceChannel),
            "voiceChannelHistory appearance missing, got {recents:?}"
        );
        assert!(
            recents
                .iter()
                .any(|r| r.snowflake == "700000000000000004" && r.kind == RecentKind::TextChannel),
            "textChannelHistory appearance dropped as a duplicate of the voice \
             entry, got {recents:?}"
        );
        // Exactly the two attributed appearances — the same id twice in one list
        // still collapses, and the unattributed sweep adds nothing for an id the
        // layout already explains.
        assert_eq!(recents.len(), 2, "one entry per list, got {recents:?}");
    }

    #[test]
    fn guild_selection_time_is_read_not_derived_from_the_snowflake() {
        // 1781524800000 = 2026-06-15T12:00:00Z — years after the id's creation
        // time, so a creation-time substitution would be unmistakable. The same id
        // also appears as `selectedGuildId`, which must not displace the timed
        // entry or duplicate it.
        let recs = vec![data(
            "https://discord.com",
            "SelectedGuildStore",
            r#"{"_state":{"selectedGuildId":"400000000000000001","lastSelectedGuildId":"400000000000000001","selectedGuildTimestampMillis":{"400000000000000001":1781524800000,"notASnowflake":1}}}"#,
        )];
        let recents = extract_recent_channels(&recs);
        assert_eq!(recents.len(), 1, "one guild, got {recents:?}");
        assert_eq!(recents[0].kind, RecentKind::Guild);
        assert_eq!(recents[0].selected_at_unix_ms, Some(1_781_524_800_000));
        assert_eq!(
            recents[0].created_at_unix_ms,
            Some(snowflake_timestamp_ms(400_000_000_000_000_001)),
            "the creation time stays available as its own distinct fact"
        );
        assert_ne!(
            recents[0].selected_at_unix_ms, recents[0].created_at_unix_ms,
            "selection and creation are different facts"
        );
    }

    #[test]
    fn guild_selection_time_accepts_a_string_epoch() {
        // Some client versions serialize the epoch ms as a decimal string.
        let recs = vec![data(
            "https://discord.com",
            "SelectedGuildStore",
            r#"{"_state":{"selectedGuildTimestampMillis":{"400000000000000001":"1781524800000","400000000000000002":"not a number"}}}"#,
        )];
        let recents = extract_recent_channels(&recs);
        assert_eq!(recents.len(), 2);
        assert_eq!(recents[0].selected_at_unix_ms, Some(1_781_524_800_000));
        // An unparseable time yields None, not a substituted creation time.
        assert_eq!(recents[1].selected_at_unix_ms, None);
        assert!(recents[1].created_at_unix_ms.is_some());
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
    fn unmodelled_layout_yields_unattributed_ids() {
        // An id-KEYED shape none of the documented keys covers. Schema-tolerant
        // extraction must still recover the key and the nested id — but their role
        // is unknown, so they are `Unattributed` rather than labelled voice/text.
        let recs = vec![data(
            "https://discord.com",
            "RecentVoiceChannelStore",
            r#"{"81384788765712384":{"channelId":"96628290369007616"},"notASnowflake":1}"#,
        )];
        let recents = extract_recent_channels(&recs);
        let ids: Vec<&str> = recents.iter().map(|r| r.snowflake.as_str()).collect();
        assert_eq!(ids, ["81384788765712384", "96628290369007616"]);
        // The key-derived entry decodes its embedded creation time like any other.
        assert_eq!(
            recents[0].created_at_unix_ms,
            Some(snowflake_timestamp_ms(81_384_788_765_712_384))
        );
        assert!(recents.iter().all(|r| r.kind == RecentKind::Unattributed));
        assert!(recents.iter().all(|r| r.selected_at_unix_ms.is_none()));
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
