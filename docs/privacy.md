# Privacy Policy

*Last updated: 2026-07-29*

## Summary

discord-desktop-forensic is a local Rust library. It does not collect, transmit, or store any personal data on remote servers.

## Data Access

discord-desktop-forensic reads only the file bytes and inputs you pass to it — a Discord Electron profile's `Local Storage/leveldb` directory. All processing — record decoding and finding generation — happens in memory on your local machine. Nothing is uploaded anywhere. The recovered auth token is **redacted by default** and never emitted in ordinary output.

## Telemetry

discord-desktop-forensic has **no telemetry**. It makes no network requests of any kind.

## Open Source

discord-desktop-forensic is open source (Apache-2.0). You can audit every line of code at [github.com/SecurityRonin/discord-desktop-forensic](https://github.com/SecurityRonin/discord-desktop-forensic).

## Contact

Privacy questions: [security@securityronin.com](mailto:security@securityronin.com)

---

[Terms of Service](terms.md) · © 2026 Security Ronin Ltd.
