# discord-desktop-forensic

<!-- Row 1 — identity + adoption decision -->
[![Crates.io core](https://img.shields.io/crates/v/discord-desktop-core.svg?label=discord-desktop-core)](https://crates.io/crates/discord-desktop-core)
[![Crates.io forensic](https://img.shields.io/crates/v/discord-desktop-forensic.svg?label=discord-desktop-forensic)](https://crates.io/crates/discord-desktop-forensic)
[![Docs.rs](https://img.shields.io/docsrs/discord-desktop-core?label=docs.rs)](https://docs.rs/discord-desktop-core)
[![Rust 1.80+](https://img.shields.io/badge/rust-1.80%2B-blue.svg)](https://www.rust-lang.org)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Sponsor](https://img.shields.io/badge/sponsor-h4x0r-ea4aaa?logo=githubsponsors)](https://github.com/sponsors/h4x0r)

<!-- Row 2 — trust proof -->
[![CI](https://github.com/SecurityRonin/discord-desktop-forensic/actions/workflows/ci.yml/badge.svg)](https://github.com/SecurityRonin/discord-desktop-forensic/actions/workflows/ci.yml)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance)
[![fuzzed](https://img.shields.io/badge/fuzzed-libFuzzer-orange.svg)](fuzz/)
[![Security advisories: clean](https://img.shields.io/badge/advisories-clean-success.svg)](deny.toml)

**Pull the account, the recent activity, and the exposed auth token out of a Discord desktop profile — no server, no message DB, just the Local Storage LevelDB.**

Discord is an Electron app that persists your account's **bearer token in plaintext** Local Storage — which is exactly why it is an info-stealer's favourite desktop target. This library reads that store on top of the fleet's Chromium storage readers and hands an investigator the account identity, the recent channels/guilds, and a **flagged, redacted** token record.

## Above the fold — audit a profile

```rust
use discord_desktop_core::read_profile;
use discord_desktop_forensic::audit_artifacts;

// Point at a Discord Electron profile (the `userData` dir).
let artifacts = read_profile("/evidence/discord".as_ref())?;

// The analyzer flags the auth token as a sensitive credential — presence + location, never the secret.
for finding in audit_artifacts(&artifacts) {
    println!("[{:?}] {} — {}", finding.severity, finding.code, finding.note);
}
// [Some(High)] DISCORD-AUTH-TOKEN-PRESENT — A Discord authentication token (126 chars, redacted)
//              is stored in Local Storage at https://ptb.discord.com — a bearer credential …
# Ok::<(), discord_desktop_core::Error>(())
```

The token secret is **never** in that output. `artifacts.tokens[0].token` is a `SensitiveToken` that redacts in `Debug`, `Display`, and `serde`; the only way to the raw value is the explicit `.reveal()`.

## What it recovers

| Artifact | Local Storage record(s) | Typed record |
|---|---|---|
| Auth token (sensitive) | `token` | `TokenRecord` — value redacted by default |
| Account identity | `MultiAccountStore`, `user_id_cache`, `email_cache` | `Account` — id, username, discriminator, avatar, email |
| Recent channels/guilds | `SelectedGuildStore`, `RecentVoiceChannelStore` | `RecentChannel` — guild/voice/text id, the recorded **selection** time, and the id's **creation** time (two distinct fields) |
| Store metadata + timeline | `META:` per origin | `OriginMeta` + `DiscordArtifacts::timeline()` |

Discord keeps **no local message database** — chats are server-side and only cached. Recoverable message *media* lives in the Simple Cache, read by the separate `chromium-storage-cache` crate.

## The two crates

- **`discord-desktop-core`** — the reader. `read_profile(dir)` → `DiscordArtifacts` (typed records + timeline). Exposes navigation, emits no findings.
- **`discord-desktop-forensic`** — the analyzer. `audit_artifacts(&artifacts)` → normalized `forensicnomicon` findings.

### Findings

| Code | Category | Severity | Meaning |
|---|---|---|---|
| `DISCORD-AUTH-TOKEN-PRESENT` | Threat | High | a live Discord auth token is stored in Local Storage (MITRE T1528) |
| `DISCORD-DELETED-TOKEN-RECOVERABLE` | Residue | Medium | a deleted token record is still recoverable |

## Trust, but verify

- **Fuzzed.** Two `cargo-fuzz` targets cover every parsed structure — the full pipeline over arbitrary LevelDB bytes and the record-value JSON parsers. Local runs: ~6.1M + ~1.9M executions, zero crashes.
- **Panic-free by lint.** `#![forbid(unsafe_code)]`; `unwrap_used`/`expect_used` denied in production; records from a missing store degrade to empty, a failed store *open* is a loud typed error.
- **Validated against a real Discord profile.** The reader was run over a genuine signed-in Discord Electron store (tier-2, app-authored bytes), recovering the token, account, 97 recent snowflakes and 43 origin records — the secret never surfaced. See [docs/validation.md](docs/validation.md).
- **Re-uses the fleet KNOWLEDGE.** Store paths and Chromium key/encoding constants come from `forensicnomicon_core::messenger_desktop` / `chromium_local_storage` — never re-hardcoded.

## How it compares

| | discord-desktop-forensic | Generic LevelDB dumpers |
|---|---|---|
| Discord token located + **redacted** | ✅ | — |
| Account identity parsed | ✅ | — |
| Recent snowflakes + decoded time | ✅ | — |
| Normalized findings (MITRE) | ✅ | — |
| Panic-free by lint, input-fuzzed | ✅ | partial |

## Install

```toml
[dependencies]
discord-desktop-core = "0.1"
discord-desktop-forensic = "0.1"
```

---

[Privacy Policy](https://securityronin.github.io/discord-desktop-forensic/privacy/) · [Terms of Service](https://securityronin.github.io/discord-desktop-forensic/terms/) · © 2026 Security Ronin Ltd
