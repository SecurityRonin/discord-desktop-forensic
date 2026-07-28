//! Error type for the Discord desktop reader.

/// A failure reading or interpreting a Discord desktop profile.
///
/// A missing store or a malformed record is *not* an error — the reader degrades
/// to an empty/partial result and surfaces what it could read. This type is for
/// *bootstrap* failures: the underlying Local Storage LevelDB could not be opened,
/// or the Discord spec is unavailable. (Fail loud on the prerequisite chain;
/// degrade-to-empty only after a validated bootstrap.)
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The Chromium Local Storage LevelDB could not be opened/read.
    #[error("failed to read Discord Local Storage LevelDB: {0}")]
    LocalStorage(#[from] leveldb_core::Error),

    /// The `Discord` messenger spec (store paths) was not found in the fleet
    /// KNOWLEDGE leaf — a build-time invariant broke.
    #[error("forensicnomicon_core::messenger_desktop has no 'Discord' spec")]
    SpecMissing,

    /// The `Discord` spec has no store for the expected role.
    #[error("Discord spec has no {0} store")]
    StoreMissing(&'static str),
}
