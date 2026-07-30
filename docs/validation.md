# Validation

How the Discord desktop reader's correctness is evidenced, and at what tier
(per the fleet Evidence-Based-Rigor discipline). The trustworthiness axis is
*who confirms the result*, not whether the data is synthetic.

## Oracle selection — the honesty rule

The reader was validated against the first oracle in the fleet's priority order
that yielded real data:

1. **Real app installed on THIS host — used (tier-2).** A genuine Discord desktop
   client (the Public Test Build, `discordptb`) was installed and signed-in on the
   development host. Its `Local Storage` store was copied to `/tmp` and the reader
   run over the **real, app-authored bytes** — not a fixture this crate encoded.
2. Public third-party DFIR sample (tier-1) — *not needed*; step 1 yielded real data.
3. Structure-only minted store (tier-2/3) — used **in addition**, as the committed
   CI gate (below), because the real profile is host-only and cannot be committed.
4. **Independent re-implementation differential (tier-1) — used** for the Chromium
   Local Storage decode our reader sits on: the same on-disk LevelDB bytes are
   decoded by `cclgroupltd/ccl_chromium_reader` and reconciled record-for-record
   (see "Differential against ccl_chromium_reader" below).

**Two distinct layers, two distinct tiers.** The **Chromium Local Storage decode**
(origin / script-key / value) our Discord reader consumes is now reconciled against
an **independent third-party decoder** (ccl) on the same bytes — **tier-1** for that
storage layer. The **Discord-specific interpretation** on top (token redaction,
`MultiAccountStore` identity, snowflake recovery) is validated at **tier-2** (real
app-authored bytes, scenario chosen by us) / tier-3 (minted record content), because
no public Discord Local Storage corpus with a documented ground-truth
account/token was located — Discord keeps no local message database. What would
upgrade that interpretation layer to tier-1: a published DFIR test image containing a
Discord profile with a documented ground-truth account/token (e.g. a DLEAPP /
Abrignoni-style sample), reconciled here.

## Tier-2 — real Discord profile (`DISCORD_PROFILE`, env-gated)

`core/tests/oracle.rs::real_discord_profile_when_present` runs the full
`read_profile` pipeline against a real Discord Electron profile named by the
`DISCORD_PROFILE` environment variable, and skips cleanly when unset (so the
committed gate never depends on host state).

Run against the real `discordptb` profile on the development host, the reader
recovered, from genuine app-authored bytes:

| Artifact | Result |
|---|---|
| Auth token | **1** record — origin `https://ptb.discord.com`, length 126, seq 160084, live (not deleted). The secret was **never surfaced** — only presence + length + location. |
| Accounts | **1** — user id, username, discriminator, avatar, `tokenStatus` all recovered from `MultiAccountStore`. |
| Recent channels/guilds | **97** snowflake references, each with a decoded creation time. |
| Origin metadata | **43** `META:` records with WebKit last-modified timestamps. |

The independent oracle for the *storage layers* is the Chromium engine itself,
which authored the LevelDB — the reader agrees with what a real Discord client
wrote. Ground truth for the *account* identity is the signed-in account on the
host (self-evident: the recovered username matches the logged-in user).

To reproduce:

```bash
cp -R "~/Library/Application Support/discordptb/Local Storage" /tmp/discord-oracle/
DISCORD_PROFILE=/tmp/discord-oracle cargo test -p discord-desktop-core \
  --test oracle real_discord_profile_when_present -- --nocapture
```

## Committed CI gate — minted LevelDB round-trip (tier-2 storage / tier-3 decode)

`core/tests/oracle.rs::minted_leveldb_round_trips_all_artifact_classes` is the
self-contained gate (no host state, no external tool). It mints a **real LevelDB**
with an *independent writer* (`rusty-leveldb`), writing Discord-shaped records in
the documented Chromium Local Storage key format (`_` + origin + NUL + type-marker
+ script-key → type-marked value), then reads them back through the entire stack:

```
rusty-leveldb (writer)  →  leveldb-core  →  chromium-storage-localstorage  →  discord-desktop-core
```

- For the **leveldb / localstorage layers** this is **tier-2**: the bytes are
  produced by an independent LevelDB implementation this crate did not author, so
  a wrong SSTable/record decode would surface.
- For the **Discord decode layer** it is **tier-3**: the record *content*
  (`MultiAccountStore` JSON, the snowflakes) and the expected answers are
  self-authored. It is honest regression scaffolding — it proves the decode is
  self-consistent and panic-free, not that it matches an external answer key. The
  tier-2 real-profile run above is the non-circular check on the decode.

## Differential against ccl_chromium_reader (tier 1)

`core/tests/differential_ccl.rs::differential_matches_ccl_chromium_reader`
reconciles the **Chromium Local Storage decode our Discord reader consumes**
(`chromium_storage_localstorage::read_dir`) against the independent Python
re-implementation
[`cclgroupltd/ccl_chromium_reader`](https://github.com/cclgroupltd/ccl_chromium_reader),
reading the **same** on-disk LevelDB bytes. Two decoders authored by different
parties agreeing on the record set is tier-1 evidence for that storage layer: the
answer key is CCL's, not ours.

- **Store:** the repo's existing minted Discord Local Storage fixture — the same
  `rusty-leveldb`-written store `core/tests/oracle.rs` builds (origin
  `https://ptb.discord.com`; script keys `token`, `MultiAccountStore`,
  `SelectedGuildStore`) — so the differential needs zero host state. Set
  `CCL_DISCORD_DIR` to point at a real copied Chromium `Local Storage/leveldb`
  directory instead.
- **Reconciliation:** our full record stream (tombstones + superseded versions
  retained) is collapsed to the same *live view* ccl exposes
  (`iter_all_records(include_deletions=False)`) — per `(origin, script_key)` the
  highest-seq, non-deleted record — then the live `(origin, script_key, value)`
  triple sets are asserted **equal**. Metadata origins from ccl's `iter_metadata`
  must be a subset of ours. Divergence fails loud, printing the symmetric
  set-difference.
- **Result:** on the minted store the two decoders agree exactly — all three live
  records reconcile (`token`, `MultiAccountStore`, `SelectedGuildStore`), decoded
  origin/script-key/value identical on both sides.
- **Scope (honest):** the committed fixture writes each key **once**, so it lives
  entirely in the LevelDB `.log` memtable and exercises neither SSTable (`.ldb`)
  compaction nor multi-version seq-resolution — the two substantive parts of
  LevelDB decoding. Those are exercised only via `CCL_DISCORD_DIR` pointed at a
  real, multi-write Chromium store; there the seq-collapse above (highest-seq per
  key on **both** sides) is what makes the sets reconcile. So the committed gate
  is a *floor* (independent decoders agree on a small live store); the real-store
  path is where the tier-1 evidence extends to compacted, multi-version data.
- **Gating:** env-gated on `CCL_DISCORD_ORACLE` — a Python interpreter that can
  `import ccl_chromium_reader`. Unset ⇒ the test skips cleanly (the committed
  workspace gate never depends on the oracle); set ⇒ any oracle error fails loud
  (a broken interpreter is a bootstrap failure, not a silent skip). The bundled
  driver is `core/tests/ccl_oracle.py`.

To reproduce:

```bash
PYTHONPATH=/path/to/ccl_chromium_reader \
  CCL_DISCORD_ORACLE=$(which python3) \
  cargo test -p discord-desktop-core --test differential_ccl -- --nocapture
```

## Source differential against DLEAPP (tier-1 for the semantics)

The *meaning* of the recent-navigation records — which JSON key holds which fact —
is reconciled against an independent reference implementation of the same
artifact, [DLEAPP](https://github.com/abrignoni/DLEAPP)'s
`scripts/artifacts/discordLocalStorage.py` (`discordActivity`). The answer key
there is authored by another party, so it is a genuine external check on the
semantics (not on our bytes):

| Fact | DLEAPP reads | This reader |
|---|---|---|
| Guild selection (activity) time | `SelectedGuildStore._state.selectedGuildTimestampMillis[guildId]`, epoch ms → "Server selected" | `RecentChannel::selected_at_unix_ms` |
| Guild/channel creation time | not reported | `RecentChannel::created_at_unix_ms`, decoded from the snowflake, labelled as creation |
| Recent voice channels | `RecentVoiceChannelStore._state.voiceChannelHistory` → "Recent voice channel" | `RecentKind::VoiceChannel` |
| Recent text channels | `RecentVoiceChannelStore._state.textChannelHistory` → "Recent text channel" | `RecentKind::TextChannel` |

`core/tests/oracle.rs::recent_guild_activity_time_is_the_recorded_selection_time`
and `::recent_text_channels_are_not_labelled_voice_channels` hold this
reconciliation, over a minted store whose recorded selection time is 3,079 days
away from the guild id's creation time — so substituting one for the other cannot
pass unnoticed.

## Panic-freedom — fuzzing (tier-2)

Two `cargo-fuzz` targets exercise every parsed structure against arbitrary input
(invariant: never panic, never read out of bounds):

| Target | Covers | Local run |
|---|---|---|
| `parse_local_storage` | full pipeline over arbitrary LevelDB `.log` bytes → decode → timeline → redaction | ~6.1M execs, 0 crashes |
| `parse_accounts` | the record-value JSON parsers (account / recent / token) over arbitrary UTF-8 | ~1.9M execs, 0 crashes |

`fuzz.yml` runs both weekly; `ci.yml` compiles them on every push. Fuzzing shows
present-robustness over N execs; it does not prove the absence of all panics. The
static partner is the lint posture: `#![forbid(unsafe_code)]` and
`unwrap_used`/`expect_used = deny` in production.

## Credential handling

The auth token is a live bearer credential. It is wrapped in `SensitiveToken`,
which redacts in `Debug`, `Display`, and `serde`; the only path to the secret is
the explicit `reveal()`. Tests assert the secret appears in **no** default
rendering and in **no** emitted finding. No cryptography is performed by this
crate — Discord stores the token in plaintext, so there is no key to derive and
nothing to fabricate.

## Open scaffolding

- The `supply-chain/` cargo-vet config declares this repo's own crates first-party
  but does not yet carry a regenerated third-party `[[exemptions]]` block; run
  `cargo vet` against the resolved graph before enabling the vet CI job.
- A published third-party Discord profile corpus with a documented ground-truth
  account/token would raise the account/token path from tier-2 to tier-1.
