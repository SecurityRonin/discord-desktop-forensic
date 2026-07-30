# discord-desktop-forensic — Purpose & Scope

This is a **library** repo (PARSER tier); per [ADR-0015] the PRD is the lighter
*Purpose & Scope*, not a product PRD.

## Purpose

Interpret a **Discord desktop** (Electron/Chromium) profile's stored artifacts on
top of the fleet's Wave-2 Chromium storage readers, turning raw Local Storage
records into typed forensic records and normalized findings. Discord is an
info-stealer's prime desktop target because it persists a plaintext bearer
**token**; this repo surfaces that credential's presence and location for an
investigator while keeping the secret out of ordinary output.

## Audience

DFIR analysts triaging a Discord install (account attribution, session-token
exposure, recent-activity context) and Rust engineers embedding Discord artifact
parsing into a larger pipeline (Issen orchestration).

## Scope

In scope:

- **Account identity** — user id, username, discriminator, avatar, `tokenStatus`
  from `MultiAccountStore`; active-session email from `email_cache` /
  `user_id_cache`.
- **Auth token** — the `token` Local Storage record, redacted by default; flagged
  by the analyzer as a sensitive credential (presence + location, MITRE T1528).
- **Recent channels/guilds** — guild ids from `SelectedGuildStore` and voice/text
  channel ids from `RecentVoiceChannelStore`, each carrying the **selection**
  (activity) time Discord recorded and, separately, the id's **creation** time
  decoded from the snowflake. The two are never conflated.
- **Store metadata + timeline** — per-origin `META:` last-modified times folded
  into an ascending timeline alongside the selection and creation times, each
  event naming which kind of time dates it.

Out of scope (deliberate):

- **A local message database** — Discord has none (chats are server-side, only
  cached). Recoverable message *bodies* live in the Simple Cache, handled by the
  separate `chromium-storage-cache` reader, not here.
- **IndexedDB message reconstruction** — no ground-truth corpus was available to
  validate it; left to a future iteration (see `docs/validation.md`).
- **Decrypting anything** — Discord stores the token in plaintext; there is no key
  to derive.

## Non-goals

A CLI/GUI (this is a library pair; the `discord4n6`-style front-end, if ever
built, is a separate concern), and any capability that would require re-deriving
the Chromium storage formats this repo depends on rather than re-using the Wave-2
readers.

## Dependencies

`forensicnomicon-core` (KNOWLEDGE: Discord store paths + Chromium constants,
report model), `chromium-storage-localstorage` + `leveldb-core` (the Wave-2
storage readers). Path deps today (unpublished Wave-1/2 outputs); switch to
registry once published.

[ADR-0015]: https://github.com/SecurityRonin/ronin-issen/blob/main/docs/decisions/0015-prd-adr-standard.md
