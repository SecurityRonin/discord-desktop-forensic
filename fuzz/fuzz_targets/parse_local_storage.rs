#![no_main]
//! Fuzz the **full Discord decode pipeline** over arbitrary LevelDB `.log` bytes:
//! `leveldb-core` → `chromium-storage-localstorage` → `discord-desktop-core`.
//! Invariant: never panic, never read out of bounds, whatever the input.

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    // Parse arbitrary bytes as a LevelDB write-ahead log.
    let Ok(records) = leveldb_core::parse_log_bytes(data, Path::new("fuzz.log")) else {
        return;
    };
    // Decode into Local Storage records, then into typed Discord artifacts.
    let ls_records = chromium_storage_localstorage::decode_records(&records);
    let artifacts = discord_desktop_core::decode_local_storage(&ls_records);
    // Exercise the timeline reconstruction and redaction paths too.
    let _ = artifacts.timeline();
    for token in &artifacts.tokens {
        let _ = token.token.redacted();
    }
});
