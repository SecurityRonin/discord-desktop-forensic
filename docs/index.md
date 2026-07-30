# discord-desktop-forensic

Interpret a **Discord desktop** (Electron/Chromium) profile — the account
identity, recent channels/guilds, and the auth **token** — from the Chromium
**Local Storage** LevelDB, on top of the fleet's Wave-2 storage readers. Discord
keeps **no local message database** (chats are server-side and only cached), so
the recoverable account evidence lives in Local Storage.

| Artifact | Local Storage record(s) | Typed record |
|---|---|---|
| Auth token (sensitive) | `token` | `TokenRecord` (secret redacted by default) |
| Account identity | `MultiAccountStore`, `user_id_cache`, `email_cache` | `Account` |
| Recent channels/guilds | `SelectedGuildStore`, `RecentVoiceChannelStore` | `RecentChannel` (recorded selection time + snowflake creation time, kept apart) |
| Store metadata | `META:` per origin | `OriginMeta` (WebKit last-modified) |

The store paths and Chromium key/encoding constants are re-used from the fleet
KNOWLEDGE leaf `forensicnomicon_core::messenger_desktop` /
`chromium_local_storage` — never re-hardcoded.

See the [README](https://github.com/SecurityRonin/discord-desktop-forensic#readme)
for usage, and [Validation](validation.md) for how the reader was proven against a
real Discord profile.

## Crates

- **`discord-desktop-core`** — the reader: decodes Local Storage into typed
  records + a timeline. Exposes navigation, no findings.
- **`discord-desktop-forensic`** — the analyzer: audits the records and emits
  normalized `forensicnomicon` findings (flags the auth token as a sensitive
  credential — presence + location, never the secret).

## Security

`#![forbid(unsafe_code)]`; `unwrap`/`expect` denied in production; every parsed
structure has a fuzz target; the token secret is structurally redacted (only an
explicit `reveal()` returns it). See
[Validation](validation.md).

---

[Privacy Policy](privacy.md) · [Terms of Service](terms.md) · © 2026 Security Ronin Ltd
