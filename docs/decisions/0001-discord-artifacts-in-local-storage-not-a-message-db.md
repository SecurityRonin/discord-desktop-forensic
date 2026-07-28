# 1. Discord evidence is Local Storage account state, not a message database

Date: 2026-07-29

## Status

Accepted

## Context

Discord is an Electron/Chromium desktop app. A naive port of the fleet's
messenger-parser shape (Signal, Wire) would expect a local chat database to
decrypt and walk. Discord has none: chats are fetched from the server and only
*cached*. The recoverable account evidence lives in Chromium **Local Storage**
(`Local Storage/leveldb/`) — the auth `token`, `MultiAccountStore` account
identity, and the recent-navigation stores — plus the Simple Cache for cached
media/API bodies. This is documented in the fleet KNOWLEDGE leaf
`forensicnomicon_core::messenger_desktop` (Discord = Electron, Account store =
`Local Storage/leveldb`, MediaCache store = `Cache/Cache_Data`, note: "no local
message database").

Two design questions followed:

1. **What does this PARSER own, and what does it re-use?** PARSER-tier crates
   depend on FOUNDATION only and never re-implement a storage format (ADR-0016).
2. **How is the token handled?** It is a live bearer credential — leaking it in
   ordinary output would be an evidence-handling failure.

## Decision

- **Model Discord as Local Storage account state, not a chat DB.** The reader
  produces `TokenRecord`, `Account`, `RecentChannel`, and `OriginMeta` from Local
  Storage records; it asserts no message database. Message *bodies* remain the
  Simple Cache's concern (`chromium-storage-cache`), not this crate's.

- **Sit on the Wave-2 storage readers; re-use the KNOWLEDGE constants.** Records
  come from `chromium-storage-localstorage` (over `leveldb-core`); the store path
  and Chromium key/encoding markers come from `forensicnomicon_core`. No path or
  marker byte is re-hardcoded, and no LevelDB/SSTable code is re-implemented here.

- **Reader/analyzer split (ADR-0008), Pattern-A naming (ADR-0009).**
  `discord-desktop-core` is the reader (typed records + timeline, no findings);
  `discord-desktop-forensic` is the analyzer (emits `forensicnomicon` findings).

- **Redact the token by construction (secure-by-default).** The token value is a
  `SensitiveToken` that redacts in `Debug`/`Display`/`serde`; the only path to the
  secret is an explicit `reveal()`. The analyzer flags the token's presence and
  location (MITRE T1528) without ever emitting the secret.

- **Match origins by host, not by one literal.** `is_discord_origin` matches any
  `discord.com` origin, covering the stable, `discordptb`, and `discordcanary`
  variants — the general rule, not a hardcoded origin.

## Consequences

- The parser is honest about Discord's evidence model: an examiner is not misled
  into expecting decryptable local chats. Cached message media is reached through
  a different, already-published reader.
- Correctness is validated at tier-2 against a real signed-in Discord profile;
  there is no tier-1 message corpus to reconcile, and none is claimed (see
  `docs/validation.md`).
- Token handling cannot regress into a leak by accident: a caller must call
  `reveal()` deliberately, and tests assert the secret appears in no default
  rendering or finding.
