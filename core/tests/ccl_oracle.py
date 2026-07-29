#!/usr/bin/env python3
"""Independent-oracle driver for the discord-desktop ccl differential test.

Reads a Chromium ``Local Storage/leveldb`` directory with the third-party
``cclgroupltd/ccl_chromium_reader`` and emits its *live* view on stdout for the
Rust harness (``tests/differential_ccl.rs``) to reconcile against our own
Chromium-storage decode (``chromium_storage_localstorage::read_dir``) — the
storage layer the Discord reader consumes.

Point ``CCL_DISCORD_ORACLE`` at a Python interpreter that can
``import ccl_chromium_reader`` (via PYTHONPATH or an install):

    PYTHONPATH=/path/to/ccl_chromium_reader \\
      CCL_DISCORD_ORACLE=$(which python3) \\
      cargo test -p discord-desktop-core --test differential_ccl -- --nocapture

Output is a hex-encoded, tab-separated line stream (hex sidesteps every
delimiter/encoding hazard in real storage values):

    ORIGIN\t<hex utf-8 storage_key>                    # one per metadata origin
    DATA\t<hex origin>\t<hex script_key>\t<hex value>  # one per live record

Only *live* (highest-seq, non-deleted) records are emitted, matching ccl's
``iter_all_records(include_deletions=False)`` — the Rust side collapses its full
tombstone-keeping stream to the same live view before comparing.
"""

import pathlib
import sys

from ccl_chromium_reader import ccl_chromium_localstorage as ls


def _hex(s: str) -> str:
    return s.encode("utf-8").hex()


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        sys.stderr.write("usage: ccl_oracle.py <leveldb-dir>\n")
        return 2
    leveldb_dir = pathlib.Path(argv[1])
    if not leveldb_dir.is_dir():
        sys.stderr.write(f"not a directory: {leveldb_dir}\n")
        return 2

    db = ls.LocalStoreDb(leveldb_dir)
    try:
        out = []
        for meta in db.iter_metadata():
            out.append(f"ORIGIN\t{_hex(meta.storage_key)}")
        # ccl's iter_all_records yields EVERY live seq version of every key, but
        # our reader (and Chromium itself) presents the single latest value per
        # key. Collapse to highest-leveldb_seq_number per (storage_key, script_key)
        # so both sides compare the same live view — otherwise the sets agree only
        # on write-once keys (they diverge on any real multi-version store).
        latest: dict[tuple[str, str], object] = {}
        for rec in db.iter_all_records(include_deletions=False):
            if rec.value is None:
                continue
            k = (rec.storage_key, rec.script_key)
            cur = latest.get(k)
            if cur is None or rec.leveldb_seq_number > cur.leveldb_seq_number:
                latest[k] = rec
        for rec in latest.values():
            out.append(
                "DATA\t"
                f"{_hex(rec.storage_key)}\t{_hex(rec.script_key)}\t{_hex(rec.value)}"
            )
    finally:
        db.close()

    sys.stdout.write("\n".join(out))
    if out:
        sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
