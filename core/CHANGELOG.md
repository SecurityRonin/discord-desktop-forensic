# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/SecurityRonin/discord-desktop-forensic/compare/discord-desktop-core-v0.1.0...discord-desktop-core-v0.2.0) - 2026-07-30

### Fixed

- *(channel)* [**breaking**] mark RecentKind/RecentChannel non_exhaustive; source the error figures
- *(channel)* read the recorded selection time; label text vs voice recents
- *(differential)* collapse ccl oracle to highest-seq-per-key + honest scope

## [0.1.0](https://github.com/SecurityRonin/discord-desktop-forensic/releases/tag/discord-desktop-core-v0.1.0) - 2026-07-29

### Added

- *(aggregate)* GREEN — decode_local_storage folds all artifact classes
- *(channel)* GREEN — extract recent guilds/voice channels with decoded time
- *(account)* GREEN — parse Discord account identity from Local Storage
- *(token)* GREEN — extract Discord auth token records, redacted by default

### Documentation

- README (two-row badges), PRD, validation, ADR-0001, index, privacy/terms, tests/data provenance
