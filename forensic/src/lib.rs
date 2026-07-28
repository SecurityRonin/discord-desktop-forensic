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

use discord_desktop_core::token::TokenRecord;
use discord_desktop_core::DiscordArtifacts;
use forensicnomicon_core::report::{
    Category, Evidence, Finding, Location, Observation, Severity, Source, SubjectRef,
};

/// The analyzer name stamped on every finding's [`Source`].
pub const ANALYZER: &str = "discord-desktop-forensic";

/// A Discord credential-exposure observation. Carries the redacted metadata that
/// backs the finding — never the token secret itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscordCredentialFinding {
    /// A live Discord auth token is present in Local Storage — a bearer credential
    /// granting full account access, and a prime info-stealer target.
    AuthTokenPresent {
        /// The Local Storage origin the token was stored under.
        origin: String,
        /// The token's length in bytes (its presence signal, never the secret).
        token_len: usize,
        /// LevelDB sequence number.
        seq: u64,
    },
    /// A deleted Discord token record is still recoverable — residual credential
    /// material that outlived its deletion.
    DeletedTokenRecoverable {
        /// The Local Storage origin the token was stored under.
        origin: String,
        /// The token's length in bytes.
        token_len: usize,
        /// LevelDB sequence number.
        seq: u64,
    },
}

impl DiscordCredentialFinding {
    fn origin(&self) -> &str {
        match self {
            Self::AuthTokenPresent { origin, .. }
            | Self::DeletedTokenRecoverable { origin, .. } => origin,
        }
    }

    fn token_len(&self) -> usize {
        match self {
            Self::AuthTokenPresent { token_len, .. }
            | Self::DeletedTokenRecoverable { token_len, .. } => *token_len,
        }
    }

    fn seq(&self) -> u64 {
        match self {
            Self::AuthTokenPresent { seq, .. } | Self::DeletedTokenRecoverable { seq, .. } => *seq,
        }
    }
}

impl Observation for DiscordCredentialFinding {
    fn severity(&self) -> Option<Severity> {
        Some(match self {
            Self::AuthTokenPresent { .. } => Severity::High,
            Self::DeletedTokenRecoverable { .. } => Severity::Medium,
        })
    }

    fn code(&self) -> &'static str {
        match self {
            Self::AuthTokenPresent { .. } => "DISCORD-AUTH-TOKEN-PRESENT",
            Self::DeletedTokenRecoverable { .. } => "DISCORD-DELETED-TOKEN-RECOVERABLE",
        }
    }

    // `TOKEN` is not one of Category::from_code's keywords, so classify explicitly.
    fn category(&self) -> Category {
        match self {
            Self::AuthTokenPresent { .. } => Category::Threat,
            Self::DeletedTokenRecoverable { .. } => Category::Residue,
        }
    }

    fn note(&self) -> String {
        match self {
            Self::AuthTokenPresent {
                origin, token_len, ..
            } => format!(
                "A Discord authentication token ({token_len} chars, redacted) is stored in \
                 Local Storage at {origin} — a bearer credential that grants full account \
                 access and is a known info-stealer target. Consistent with a signed-in \
                 account recoverable from this artifact."
            ),
            Self::DeletedTokenRecoverable {
                origin, token_len, ..
            } => format!(
                "A deleted Discord authentication token record ({token_len} chars, redacted) \
                 remains recoverable in Local Storage at {origin} — residual credential \
                 material that outlived its deletion."
            ),
        }
    }

    fn subjects(&self) -> Vec<SubjectRef> {
        vec![SubjectRef {
            scheme: "messenger".to_string(),
            kind: "discord_auth_token".to_string(),
            id: self.origin().to_string(),
            label: Some("Discord".to_string()),
        }]
    }

    fn evidence(&self) -> Vec<Evidence> {
        vec![
            Evidence {
                field: "origin".to_string(),
                value: self.origin().to_string(),
                location: Some(Location::Path(self.origin().to_string())),
            },
            Evidence {
                field: "token_length_chars".to_string(),
                value: self.token_len().to_string(),
                location: None,
            },
            Evidence {
                field: "leveldb_seq".to_string(),
                value: self.seq().to_string(),
                location: Some(Location::RecordId(self.seq())),
            },
            Evidence {
                field: "store".to_string(),
                value: "Local Storage/leveldb".to_string(),
                location: None,
            },
        ]
    }

    fn mitre(&self) -> &'static [&'static str] {
        // T1528 — Steal Application Access Token.
        &["T1528"]
    }
}

/// Classify one token record into a credential-exposure observation.
fn observe_token(record: &TokenRecord) -> DiscordCredentialFinding {
    let origin = record.origin.clone();
    let token_len = record.token.len();
    let seq = record.seq;
    if record.deleted {
        DiscordCredentialFinding::DeletedTokenRecoverable {
            origin,
            token_len,
            seq,
        }
    } else {
        DiscordCredentialFinding::AuthTokenPresent {
            origin,
            token_len,
            seq,
        }
    }
}

/// Audit recovered Discord artifacts and return normalized findings.
///
/// Flags each live auth token as a sensitive credential (presence + location,
/// never the secret) and each recoverable deleted token record as residual
/// credential material. Artifacts with no token yield no findings.
#[must_use]
pub fn audit_artifacts(artifacts: &DiscordArtifacts) -> Vec<Finding> {
    artifacts
        .tokens
        .iter()
        .map(|record| {
            let observation = observe_token(record);
            let source = Source {
                analyzer: ANALYZER.to_string(),
                scope: record.origin.clone(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            };
            observation.to_finding(source)
        })
        .collect()
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
