#!/usr/bin/env python3
"""Writes or checks the sha256 sidecar for a sideboard search-campaign
manifest (design section 4, W5). Matches the byte-for-byte convention the
Rust loader (mtg-kernel/src/sideboard_search_campaign_v1.rs,
load_and_verify_manifest_v1) uses: sha256 over the raw manifest file bytes,
recorded in a sibling <name>.json.sha256 file as one lower-hex line.
"""
from __future__ import annotations

import argparse
import hashlib
import sys
from pathlib import Path


def sha256_hex(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--write-hash", metavar="MANIFEST_JSON")
    group.add_argument("--check", metavar="MANIFEST_JSON")
    args = parser.parse_args(argv)

    manifest_path = Path(args.write_hash or args.check)
    if not manifest_path.exists():
        print(f"{manifest_path} does not exist", file=sys.stderr)
        return 1
    hash_path = manifest_path.with_suffix(".json.sha256")
    digest = sha256_hex(manifest_path)

    if args.write_hash:
        hash_path.write_text(digest + "\n", encoding="utf-8")
        print(f"wrote {hash_path}: {digest}")
        return 0

    if not hash_path.exists():
        print(f"{hash_path} does not exist; run --write-hash first", file=sys.stderr)
        return 1
    recorded = hash_path.read_text(encoding="utf-8").strip()
    if recorded != digest:
        print(f"hash mismatch: recorded {recorded}, recomputed {digest}", file=sys.stderr)
        return 1
    print(f"{manifest_path} matches its recorded hash: {digest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
