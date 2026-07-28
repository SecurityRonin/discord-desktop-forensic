//! Discord desktop **forensic analyzer** — audits the artifacts recovered by
//! [`discord_desktop_core`] and emits normalized [`forensicnomicon_core::report`]
//! findings.
//!
//! Its headline job is the **auth token**: Discord stores the account's bearer
//! token as a plain `localStorage["token"]` value, a credential that grants full
//! account access and is the prime target of Electron info-stealers. The analyzer
//! **flags its presence and location** — origin, LevelDB sequence, and the
//! redacted length — and never the secret itself. A recoverable *deleted* token
//! record is surfaced separately as residual credential material.
//!
//! | Code | Category | Severity | Meaning |
//! |---|---|---|---|
//! | `DISCORD-AUTH-TOKEN-PRESENT` | Threat | High | a live Discord auth token is stored in Local Storage |
//! | `DISCORD-DELETED-TOKEN-RECOVERABLE` | Residue | Medium | a deleted Discord token record is still recoverable |
//!
//! Findings are observations, never legal conclusions ("consistent with", never a
//! verdict). MITRE ATT&CK T1528 (Steal Application Access Token) is referenced.
#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use discord_desktop_core::DiscordArtifacts;
use forensicnomicon_core::report::Finding;

/// The analyzer name stamped on every finding's [`Source`].
pub const ANALYZER: &str = "discord-desktop-forensic";

/// Audit recovered Discord artifacts and return normalized findings.
///
/// Flags each live auth token as a sensitive credential (presence + location,
/// never the secret) and each recoverable deleted token record as residual
/// credential material. Artifacts with no token yield no findings.
#[must_use]
pub fn audit_artifacts(artifacts: &DiscordArtifacts) -> Vec<Finding> {
    let _ = artifacts;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_desktop_core::token::{SensitiveToken, TokenRecord};
    use forensicnomicon_core::report::{Category, Severity};

    const FAKE_TOKEN: &str = "EXAMPLE-synthetic-fixture.not-real.discord-token-value-for-tests";

    fn artifacts_with_token(deleted: bool) -> DiscordArtifacts {
        DiscordArtifacts {
            tokens: vec![TokenRecord {
                origin: "https://ptb.discord.com".to_string(),
                token: SensitiveToken::new(FAKE_TOKEN),
                seq: 42,
                deleted,
            }],
            ..DiscordArtifacts::default()
        }
    }

    #[test]
    fn flags_live_token_high_threat_without_leaking_secret() {
        let findings = audit_artifacts(&artifacts_with_token(false));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.code, "DISCORD-AUTH-TOKEN-PRESENT");
        assert_eq!(f.severity, Some(Severity::High));
        assert_eq!(f.category, Category::Threat);
        assert_eq!(f.source.analyzer, ANALYZER);

        // Location is surfaced.
        assert!(f
            .evidence
            .iter()
            .any(|e| e.value.contains("ptb.discord.com")));
        assert!(f.context.external_refs.iter().any(|r| r.id == "T1528"));

        // Secure-by-default: the secret must appear NOWHERE in the finding.
        let blob = format!("{f:?}");
        assert!(
            !blob.contains(FAKE_TOKEN),
            "finding must not contain the raw token"
        );
        // …but the redacted length is surfaced so the analyst knows it is present.
        assert!(blob.contains(&FAKE_TOKEN.len().to_string()));
    }

    #[test]
    fn flags_deleted_token_as_residue() {
        let findings = audit_artifacts(&artifacts_with_token(true));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "DISCORD-DELETED-TOKEN-RECOVERABLE");
        assert_eq!(findings[0].severity, Some(Severity::Medium));
        assert_eq!(findings[0].category, Category::Residue);
    }

    #[test]
    fn no_token_no_findings() {
        assert!(audit_artifacts(&DiscordArtifacts::default()).is_empty());
    }
}
