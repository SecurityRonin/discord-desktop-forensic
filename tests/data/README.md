# Test data provenance

This repo commits **no** binary test artifacts. Its oracle data is either minted
at test time by an independent LevelDB writer, or read from a real Discord profile
on the host (never committed). This README records the provenance of both, per the
fleet Test-Data-Provenance standard. The single fleet-wide index is
`ronin-issen/docs/test-data-catalog.md` — cross-reference, do not duplicate.

## SYNTHETIC — minted LevelDB (committed generator, no committed bytes)

- **Classification:** `SYNTHETIC` (`✓` confirmed).
- **What:** a real LevelDB carrying Discord-shaped Chromium Local Storage records
  (a `token` record, a `MultiAccountStore` account, a `SelectedGuildStore` guild),
  minted into a temp dir at test time.
- **Generator (verbatim):** `core/tests/oracle.rs::mint_discord_profile` — writes
  the records with the independent `rusty-leveldb` crate, in the documented
  Chromium key format (`_` + origin + NUL + `0x01` Latin-1 marker + script-key →
  `0x01` + value). No download URL — it is generated, not sourced.
- **Consumed by:** `core/tests/oracle.rs::minted_leveldb_round_trips_all_artifact_classes`.
- **Ground truth:** the exact records written by the generator (tier-2 for the
  leveldb/localstorage layers, tier-3 for the Discord decode — see
  `docs/validation.md`).
- **Note:** the fixture token is a non-credential placeholder string; redaction
  only measures length, so no real-shaped secret is needed or committed.

## REAL-ext — real Discord profile (host-only, env-gated, NOT committed)

- **Classification:** `REAL-self` (`✓` confirmed) — a real app on the analyst's
  own host, not a third-party corpus.
- **What:** a genuine signed-in Discord Electron profile's `Local Storage/leveldb`
  directory (the Public Test Build, `discordptb`, was used during development).
- **Source:** the Discord desktop client itself, on the development host
  (`~/Library/Application Support/discordptb/Local Storage`). App-authored bytes.
- **Identity/contents:** one live 126-char auth token, one account
  (`MultiAccountStore`), 97 recent snowflakes, 43 origin `META:` records (as
  observed on the dev host). Values are user-specific and **not** committed or
  reproduced here.
- **Consumed by:** `core/tests/oracle.rs::real_discord_profile_when_present`,
  gated on the `DISCORD_PROFILE` env var (skips cleanly when unset).
- **Redistribution:** none — a live account credential; never committed, never
  logged verbatim. The test asserts token presence + length only.
