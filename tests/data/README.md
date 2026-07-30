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
- **Records written (the ground truth), origin `https://ptb.discord.com`:**
  - `token` = `SYNTHETIC-DISCORD-TOKEN-FIXTURE-not-a-real-credential-000`
  - `MultiAccountStore` = `{"_state":{"users":[{"id":"700000000000000000","username":"testuser","discriminator":"0","avatar":"abc","tokenStatus":2}]}}`
  - `SelectedGuildStore` = `{"selectedGuildId":"81384788765712384"}`

## Independent oracle — `ccl_chromium_reader` differential

`core/tests/differential_ccl.rs::differential_matches_ccl_chromium_reader` mints the
same store shape (`core/tests/differential_ccl.rs::mint_discord_leveldb`) and then
reconciles our `chromium_storage_localstorage::read_dir` decode against
**`cclgroupltd/ccl_chromium_reader`** — a third-party Python implementation of the
same LevelDB/Chromium-storage decode — reading the identical bytes. Two independent
implementations agreeing on the storage layer is the tier-1 evidence; the Discord
*decode* on top of it stays tier-3 (see `docs/validation.md`).

Driver script (committed): `core/tests/ccl_oracle.py` — emits a hex/TSV line stream
(`ORIGIN\t<hex>` and `DATA\t<hex origin>\t<hex script_key>\t<hex value>`).

```sh
PYTHONPATH=/path/to/ccl_chromium_reader \
  CCL_DISCORD_ORACLE=$(which python3) \
  cargo test -p discord-desktop-core --test differential_ccl -- --nocapture
```

| Env var | Purpose |
|---|---|
| `CCL_DISCORD_ORACLE` | a Python interpreter that can `import ccl_chromium_reader`; unset ⇒ skip |
| `CCL_DISCORD_DIR` | optional — read an existing `Local Storage/leveldb` dir instead of minting one |

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
