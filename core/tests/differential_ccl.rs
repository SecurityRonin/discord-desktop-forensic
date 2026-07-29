//! Tier-1 **differential** validation against the independent Python oracle
//! [`cclgroupltd/ccl_chromium_reader`](https://github.com/cclgroupltd/ccl_chromium_reader).
//!
//! `oracle.rs` asserts our decode against the *documented* writes we minted
//! (construction-derived ground truth — tier-2 for the storage stack, tier-3 for
//! the Discord record content). This test instead reconciles the **Chromium
//! Local Storage decode our Discord reader consumes**
//! (`chromium_storage_localstorage::read_dir`) against a **separate, third-party
//! re-implementation** reading the *same* on-disk LevelDB bytes. Agreement of two
//! independent decoders is tier-1 evidence for that storage layer: the answer key
//! is authored by someone else (CCL), not by us.
//!
//! The store is the repo's existing minted Discord Local Storage fixture — the
//! same `rusty-leveldb`-written store `oracle.rs` builds — so the differential
//! runs with zero host state once the oracle interpreter is available.
//!
//! ## Gating (skips cleanly unless the oracle is present)
//! * `CCL_DISCORD_ORACLE` — path to a Python interpreter that can
//!   `import ccl_chromium_reader` (e.g. `PYTHONPATH=/path/to/ccl_chromium_reader
//!   $(which python3)`). Unset ⇒ skip.
//! * `CCL_DISCORD_DIR` — optional override pointing at a real Chromium
//!   `Local Storage/leveldb` directory (e.g. a copied Discord profile). Defaults
//!   to the freshly minted fixture.
//!
//! When `CCL_DISCORD_ORACLE` is set we fail loud on any oracle error (a broken
//! interpreter is a bootstrap failure, never a silent skip). Absence of the env
//! var is the only clean-skip path.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use chromium_storage_localstorage::{read_dir, LocalStorageRecord};

const ORIGIN: &str = "https://ptb.discord.com";
// Synthetic, non-credential fixture values (identical shape to `oracle.rs`).
const FIXTURE_TOKEN: &str = "SYNTHETIC-DISCORD-TOKEN-FIXTURE-not-a-real-credential-000";
const MULTI_ACCOUNT: &str = r#"{"_state":{"users":[{"id":"700000000000000000","username":"testuser","discriminator":"0","avatar":"abc","tokenStatus":2}]}}"#;
const SELECTED_GUILD: &str = r#"{"selectedGuildId":"81384788765712384"}"#;

/// Build a Chromium Local Storage **data key**: `_` + origin + NUL + Latin-1
/// marker + script-key bytes (the documented Chromium key schema, mirrored from
/// `oracle.rs`).
fn data_key(origin: &str, script_key: &str) -> Vec<u8> {
    let mut key = vec![b'_'];
    key.extend_from_slice(origin.as_bytes());
    key.push(0x00); // NUL separator
    key.push(0x01); // Latin-1 marker for the script key
    key.extend_from_slice(script_key.as_bytes());
    key
}

/// Build a Chromium Local Storage **value**: Latin-1 marker + body bytes.
fn latin1_value(value: &str) -> Vec<u8> {
    let mut v = vec![0x01];
    v.extend_from_slice(value.as_bytes());
    v
}

/// Mint a real LevelDB at `<dir>/Local Storage/leveldb` carrying Discord-shaped
/// records, using the independent `rusty-leveldb` writer. Returns the leveldb dir.
fn mint_discord_leveldb(dir: &Path) -> PathBuf {
    let leveldb_dir = dir.join("Local Storage").join("leveldb");
    std::fs::create_dir_all(&leveldb_dir).unwrap();

    let opt = rusty_leveldb::Options {
        create_if_missing: true,
        ..Default::default()
    };
    let mut db = rusty_leveldb::DB::open(&leveldb_dir, opt).unwrap();

    db.put(&data_key(ORIGIN, "token"), &latin1_value(FIXTURE_TOKEN))
        .unwrap();
    db.put(
        &data_key(ORIGIN, "MultiAccountStore"),
        &latin1_value(MULTI_ACCOUNT),
    )
    .unwrap();
    db.put(
        &data_key(ORIGIN, "SelectedGuildStore"),
        &latin1_value(SELECTED_GUILD),
    )
    .unwrap();
    db.flush().unwrap();
    drop(db);

    leveldb_dir
}

/// The bundled Python oracle driver (this test's private companion).
fn oracle_script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/ccl_oracle.py")
}

/// Decode two hex nibbles.
fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len() % 2 == 0, "odd-length hex from oracle: {s:?}");
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex from oracle"))
        .collect()
}

fn unhex_str(s: &str) -> String {
    String::from_utf8(unhex(s)).expect("oracle emits UTF-8 hex")
}

/// What both decoders are reconciled on.
#[derive(Default)]
struct Sets {
    /// Origins carrying storage metadata.
    origins: BTreeSet<String>,
    /// Live `(origin, script_key, value)` triples.
    data: BTreeSet<(String, String, String)>,
}

/// Run the ccl oracle over `dir` and parse its hex-line output into [`Sets`].
fn oracle_sets(python: &str, dir: &Path) -> Sets {
    let out = Command::new(python)
        .arg(oracle_script())
        .arg(dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to launch ccl oracle {python:?}: {e}"));
    assert!(
        out.status.success(),
        "ccl oracle exited {:?}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("oracle stdout is UTF-8");
    let mut sets = Sets::default();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut f = line.split('\t');
        match f.next() {
            Some("ORIGIN") => {
                let origin = unhex_str(f.next().expect("ORIGIN storage_key"));
                sets.origins.insert(origin);
            }
            Some("DATA") => {
                let origin = unhex_str(f.next().expect("DATA origin"));
                let key = unhex_str(f.next().expect("DATA script_key"));
                let value = unhex_str(f.next().expect("DATA value"));
                sets.data.insert((origin, key, value));
            }
            other => panic!("unrecognised oracle line tag {other:?} in {line:?}"),
        }
    }
    sets
}

/// Collapse our full record stream (which keeps tombstones + superseded
/// versions) into the same *live view* ccl exposes: per `(origin, script_key)`,
/// the highest-`seq` record, kept only when it is not a deletion.
fn our_sets(recs: &[LocalStorageRecord]) -> Sets {
    let mut origins = BTreeSet::new();
    let mut best: HashMap<(String, String), (u64, bool, String)> = HashMap::new();
    for r in recs {
        match r {
            LocalStorageRecord::Meta {
                origin,
                deleted: false,
                ..
            } => {
                origins.insert(origin.clone());
            }
            LocalStorageRecord::Data {
                origin,
                script_key,
                value,
                seq,
                deleted,
            } => {
                let k = (origin.clone(), script_key.text.clone());
                let replace = best.get(&k).map_or(true, |(s, _, _)| *seq > *s);
                if replace {
                    best.insert(k, (*seq, *deleted, value.text.clone()));
                }
            }
            _ => {}
        }
    }
    let data = best
        .into_iter()
        .filter(|(_, (_, deleted, _))| !*deleted)
        .map(|((origin, key), (_, _, value))| (origin, key, value))
        .collect();
    Sets { origins, data }
}

#[test]
fn differential_matches_ccl_chromium_reader() {
    let Ok(python) = std::env::var("CCL_DISCORD_ORACLE") else {
        eprintln!(
            "skipping ccl differential: set CCL_DISCORD_ORACLE to a Python \
             interpreter that can `import ccl_chromium_reader` (e.g. \
             PYTHONPATH=/path/to/ccl_chromium_reader $(which python3))"
        );
        return;
    };
    if python.trim().is_empty() {
        eprintln!("skipping ccl differential: CCL_DISCORD_ORACLE is empty");
        return;
    }

    // Either reconcile over a real Chromium leveldb dir the caller points at, or
    // the repo's minted Discord fixture (default, zero host state).
    let tmp = tempfile::tempdir().unwrap();
    let dir = std::env::var_os("CCL_DISCORD_DIR")
        .map_or_else(|| mint_discord_leveldb(tmp.path()), PathBuf::from);
    assert!(
        dir.is_dir(),
        "CCL_DISCORD_DIR / fixture is not a directory: {}",
        dir.display()
    );

    let ours =
        our_sets(&read_dir(&dir).expect("our chromium-storage decoder reads the leveldb dir"));
    let theirs = oracle_sets(&python, &dir);

    // The store must carry at least one live entry, or the differential proves
    // nothing (guards against pointing at an empty/wrong directory).
    assert!(
        !theirs.data.is_empty(),
        "ccl oracle found no live Local Storage records in {} — wrong directory?",
        dir.display()
    );

    assert_eq!(
        ours.data,
        theirs.data,
        "live (origin, script_key, value) sets diverge from ccl_chromium_reader\n\
         ours-only:   {:?}\n\
         theirs-only: {:?}",
        ours.data.difference(&theirs.data).collect::<Vec<_>>(),
        theirs.data.difference(&ours.data).collect::<Vec<_>>(),
    );

    // ccl only lists origins that carry a metadata record; ours must cover all
    // of them (we may additionally surface metadata-less origins, so ⊇, not ==).
    assert!(
        theirs.origins.is_subset(&ours.origins),
        "origins with metadata diverge\nours:   {:?}\ntheirs: {:?}",
        ours.origins,
        theirs.origins,
    );
}
