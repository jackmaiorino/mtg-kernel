#!/usr/bin/env python3
"""Card DB identity re-pin checker/rewriter.

Every card wave (an edit to data/cards_v1.json) moves the FNV1a64 hash that
mtg-kernel/build.rs bakes into the generated code as
`pub const KERNEL_CARDDB_HASH: u64 = 0x...;`. A shifting number of source
files pin that value as a literal, so it can be checked and re-pinned. This
tool finds every one of those pin sites, verifies none of the expected ones
were missed, and (with --write) rewrites them all to the live value in one
mechanical pass.

Live hash source
-----------------
The live hash is never recomputed in Python (that would mean reimplementing
build.rs's canonicalization). Build the crate first, e.g. from Git Bash:

    export CARGO_TARGET_DIR=E:/cargo-target-pauper-meta
    cmd //c "start /b /wait /belownormal /affinity FF0000 cargo build --locked -p mtg-kernel"

then point this tool at the same target dir with the MTG_KERNEL_CARGO_TARGET_DIR
environment variable (default: "target", Cargo's own default, resolved
relative to the repo root if not absolute). This tool globs both
"$MTG_KERNEL_CARGO_TARGET_DIR/debug/build/mtg-kernel-*/out/*.rs" and
"$MTG_KERNEL_CARGO_TARGET_DIR/release/build/mtg-kernel-*/out/*.rs" for the
line "pub const KERNEL_CARDDB_HASH: u64 = 0x...;" (both profiles: CI's
python-tests job only ever runs `cargo build --release`, a local dev build
usually only runs plain `cargo build`). If several "out" directories exist
across either profile, it uses the newest by modification time among the
ones that actually contain the KERNEL_CARDDB_HASH line, and prints which
directory and profile it used.

Old literal
-----------
The "old" literal is never hardcoded here. It is parsed fresh from
mtg-kernel/src/card_def.rs's own test assertion
("assert_eq!(KERNEL_CARDDB_HASH, 0x...);"), so this tool keeps working wave
after wave without needing an edit of its own.

Pin sites
---------
Both spellings of the old literal (grouped, e.g. "0xAAAA_BBBB_CCCC_DDDD",
and bare, e.g. "aaaabbbbccccdddd") are searched for recursively under
mtg-kernel/src, mtg-kernel/tests, python/tests, and python/tools. Every
file named in REQUIRED_PIN_SITES_V1 must show at least one occurrence, or
--check (and --write) treat the missing site as a hard error: a moved pin
would otherwise go unnoticed. See NOT_CURRENTLY_PINNED_SITES_V1 below for
five task-2-brief-listed files that are deliberately excluded from that
requirement.

Deliberately, no real hash value appears anywhere in this docstring or
anywhere else in this file: python/tools is itself one of the recursively
searched directories, so a concrete example here would make this file
match itself as a "pin site" and get rewritten by its own --write.

Protected training-store constants
-----------------------------------
mtg-kernel/src/native_training_store_run_v2.rs's
FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1 and FROZEN_CARD_DB_HASH_U64_HEX_V2 are
never rewritten by this tool; Task 3 owns their migration. Matches on that
file are reported separately as "training-store constants, owned by the
catalog profile migration". Task 3 landed the card lane's third catalog
profile, FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1, and named it in
TASK3_REWRITABLE_IDENTIFIERS_V1, so this tool rewrites that one constant
like any other pin site while still leaving the two protected ones alone.

Usage
-----
    repin_card_db_identity_v1.py --check    # exit 1 and list every stale pin
    repin_card_db_identity_v1.py --write    # rewrite the pins in place
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

CARGO_TARGET_DIR_ENV = "MTG_KERNEL_CARGO_TARGET_DIR"
DEFAULT_CARGO_TARGET_DIR = "target"

CARD_DEF_PATH = REPO_ROOT / "mtg-kernel" / "src" / "card_def.rs"
OLD_LITERAL_PATTERN = re.compile(
    r"assert_eq!\(KERNEL_CARDDB_HASH,\s*0x([0-9a-fA-F_]+)\)"
)
GENERATED_HASH_PATTERN = re.compile(
    r"pub const KERNEL_CARDDB_HASH:\s*u64\s*=\s*0x([0-9a-fA-F_]+);"
)

SEARCH_DIRS = (
    "mtg-kernel/src",
    "mtg-kernel/tests",
    "python/tests",
    "python/tools",
)
SEARCH_SUFFIXES = (".rs", ".py")

# Every file the task-2 SDD brief (2026-09-09) names as a card-DB-hash pin
# site. Kept verbatim for traceability even though five of these files
# (NOT_CURRENTLY_PINNED_SITES_V1) do not currently carry the
# KERNEL_CARDDB_HASH literal at all -- see the note below.
BRIEF_LISTED_SITES_V1 = (
    "mtg-kernel/src/card_def.rs",
    "mtg-kernel/src/native_training_store_run_v2.rs",
    "mtg-kernel/src/runtime_decks.rs",
    "mtg-kernel/src/kernel_native_search_calibration_runner_v1.rs",
    "mtg-kernel/src/native_full_episode_trajectory_v2.rs",
    "mtg-kernel/src/native_full_episode_trajectory_v2_goldens.rs",
    "mtg-kernel/tests/caw_gates_completion_v1.rs",
    "mtg-kernel/tests/caw_gates_future_v1.rs",
    "mtg-kernel/tests/faeries_future_v1.rs",
    "mtg-kernel/tests/final_pool_completion_v1.rs",
    "mtg-kernel/tests/wildfire_utility_future_v1.rs",
    "python/tests/test_flat_policy_v2_goldens.py",
    "python/tools/generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py",
    "python/tools/generate_native_full_episode_trajectory_v2_goldens.py",
)

# Verified 2026-09-09 by direct inspection: these five brief-listed files
# pin a SHA-256 of the two-deck runtime catalog file, which is a distinct
# identity from KERNEL_CARDDB_HASH -- RUNTIME_DECK_CATALOG_FILE_SHA256 /
# EXPECTED_RUNTIME_DECK_CATALOG_FILE_SHA256_V2 in runtime_decks.rs,
# native_full_episode_trajectory_v2.rs,
# native_full_episode_trajectory_v2_goldens.rs, and
# generate_native_full_episode_trajectory_v2_goldens.py; the differently
# named RUNTIME_DECK_CATALOG_SHA256 (no "FILE") in
# generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py.
# That pin was last re-baselined by commits 40a0fadc and bca8f91b
# (2026-08-14), unrelated to and predating this card-DB-hash re-pin tool.
# As of this writing they carry zero occurrences of either spelling of the
# card-DB-hash literal, so they are excluded from REQUIRED_PIN_SITES_V1
# (folding them in would fail --check on an otherwise perfectly consistent
# tree). They stay inside the recursive glob search below, so if a future
# wave ever does add a card-DB-hash literal to one of them, this tool will
# find and manage it like any other site.
NOT_CURRENTLY_PINNED_SITES_V1 = (
    "mtg-kernel/src/runtime_decks.rs",
    "mtg-kernel/src/native_full_episode_trajectory_v2.rs",
    "mtg-kernel/src/native_full_episode_trajectory_v2_goldens.rs",
    "python/tools/generate_environment_randomization_v2_reset_physical_trajectory_goldens_v1.py",
    "python/tools/generate_native_full_episode_trajectory_v2_goldens.py",
)

REQUIRED_PIN_SITES_V1 = tuple(
    site
    for site in BRIEF_LISTED_SITES_V1
    if site not in NOT_CURRENTLY_PINNED_SITES_V1
)

# native_training_store_run_v2.rs is a training-store "catalog profile
# migration" module; Task 3 owns re-pinning it. This tool must never
# rewrite these constants there, only report them.
PROTECTED_FILE_V1 = "mtg-kernel/src/native_training_store_run_v2.rs"
PROTECTED_IDENTIFIERS_V1 = (
    "FROZEN_CARD_DB_HASH_U64_HEX_CURRENT_V1",
    "FROZEN_CARD_DB_HASH_U64_HEX_V2",
)
# Task 3's own new training-store constant: the card lane's third catalog
# profile (schema migration, design ruling 6 pending). Unlike
# PROTECTED_IDENTIFIERS_V1 above, this one IS rewritten by --write like any
# other pin site, since (unlike CURRENT_V1/V2) it is meant to track the live
# hash forward across future card waves.
TASK3_REWRITABLE_IDENTIFIERS_V1: tuple[str, ...] = (
    "FROZEN_CARD_DB_HASH_U64_HEX_PAUPER_META_W1",
)

TRAINING_STORE_LABEL = "training-store constants, owned by the catalog profile migration"


class RepinError(RuntimeError):
    """The tree is not in a state this tool can safely reason about."""


def _hex16(value: int) -> str:
    return format(value, "016x")


def grouped_spelling(value: int) -> str:
    hex16 = _hex16(value)
    return "0x" + "_".join(hex16[i : i + 4] for i in range(0, 16, 4))


def bare_spelling(value: int) -> str:
    return _hex16(value)


def parse_old_hash(card_def_path: Path = CARD_DEF_PATH) -> int:
    text = card_def_path.read_bytes().decode("utf-8")
    match = OLD_LITERAL_PATTERN.search(text)
    if not match:
        raise RepinError(
            "could not find the KERNEL_CARDDB_HASH test assertion in "
            f"{card_def_path}"
        )
    return int(match.group(1).replace("_", ""), 16)


def cargo_target_dir() -> Path:
    raw = os.environ.get(CARGO_TARGET_DIR_ENV, DEFAULT_CARGO_TARGET_DIR)
    path = Path(raw)
    if not path.is_absolute():
        path = REPO_ROOT / path
    return path


# CI's python-tests job builds only `cargo build --release --locked --bin
# kernel_rl_env` (see .github/workflows/ci.yml, python-tests job), never a
# plain (debug) `cargo build`, so only release/build/mtg-kernel-*/out ever
# exists there; a local dev build usually only runs plain `cargo build`, so
# only debug/build/mtg-kernel-*/out exists. Both profiles are searched so
# this tool (and its covering test) works unmodified in either environment.
CARGO_PROFILES = ("debug", "release")


def find_out_dir_candidates(target_dir: Path) -> list[tuple[Path, str]]:
    """Return (out_dir, profile) pairs across both cargo profiles, ordered
    newest first by the newest KERNEL_CARDDB_HASH-bearing `*.rs` file's own
    modification time inside each out directory -- deliberately *not* the
    out directory's own mtime.

    NTFS does not bump a directory's own mtime when a file inside it is
    overwritten in place (only when an entry is added/removed/renamed), so
    an out directory that cargo has silently regenerated many times across
    a session can carry an older directory mtime than one that has not been
    touched since cargo first created it. Sorting on the directory's own
    mtime can then hand back a stale `card_defs.rs` from a directory that
    merely happened to be created later, even though a *different*
    directory's file was actually regenerated more recently. Confirmed by
    hand during Task 11: two `mtg-kernel-<hash>/out` directories existed
    under the same target dir, and the one with the newer directory mtime
    held the staler `card_defs.rs` content.

    An out directory with no `KERNEL_CARDDB_HASH`-bearing file at all sorts
    last (as `float("-inf")`) but is still returned, so `parse_live_hash`'s
    own "no constant found under any candidate" diagnostic still sees it.
    """
    candidates: list[tuple[float, Path, str]] = []
    for profile in CARGO_PROFILES:
        pattern = str(target_dir / profile / "build" / "mtg-kernel-*" / "out")
        for raw in glob.glob(pattern):
            out_dir = Path(raw)
            if not out_dir.is_dir():
                continue
            newest_generated_mtime = float("-inf")
            for rs_path in out_dir.glob("*.rs"):
                try:
                    text = rs_path.read_bytes().decode("utf-8")
                except (OSError, UnicodeDecodeError):
                    continue
                if GENERATED_HASH_PATTERN.search(text):
                    newest_generated_mtime = max(
                        newest_generated_mtime, rs_path.stat().st_mtime
                    )
            candidates.append((newest_generated_mtime, out_dir, profile))
    candidates.sort(key=lambda item: item[0], reverse=True)
    return [(out_dir, profile) for _, out_dir, profile in candidates]


def parse_live_hash(target_dir: Path) -> tuple[int, Path, str]:
    """Return (live_hash, rs_path, profile) from the newest out directory,
    across both the debug and release profiles, that actually contains a
    KERNEL_CARDDB_HASH line.
    """
    candidates = find_out_dir_candidates(target_dir)
    if not candidates:
        patterns = " or ".join(
            str(target_dir / profile / "build" / "mtg-kernel-*" / "out")
            for profile in CARGO_PROFILES
        )
        raise RepinError(
            f"no generated build output directory matched {patterns}; build "
            "the crate first (cargo build --locked -p mtg-kernel, or cargo "
            "build --release --locked -p mtg-kernel) with "
            f"{CARGO_TARGET_DIR_ENV} (or Cargo's own CARGO_TARGET_DIR) "
            "pointed at the same target dir"
        )
    for out_dir, profile in candidates:
        for rs_path in sorted(out_dir.glob("*.rs")):
            text = rs_path.read_bytes().decode("utf-8")
            match = GENERATED_HASH_PATTERN.search(text)
            if match:
                return int(match.group(1).replace("_", ""), 16), rs_path, profile
    raise RepinError(
        "no KERNEL_CARDDB_HASH constant found under any candidate out "
        "directory: " + ", ".join(str(out_dir) for out_dir, _ in candidates)
    )


def discover_search_files() -> list[Path]:
    files: list[Path] = []
    for rel_dir in SEARCH_DIRS:
        root = REPO_ROOT / rel_dir
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*")):
            if path.is_file() and path.suffix in SEARCH_SUFFIXES:
                files.append(path)
    return files


@dataclass(frozen=True)
class Occurrence:
    rel_path: str
    line_no: int
    spelling: str
    protected: bool


def _classify_protected_line(rel_path: str, line_no: int, line: str) -> bool:
    """Return True if this line's literal must not be rewritten.

    Every match in PROTECTED_FILE_V1 is protected by default, unless its
    line names an identifier Task 3 has explicitly allow-listed as
    rewritable. As a safety net, a match in PROTECTED_FILE_V1 that names
    neither a known protected identifier nor an allow-listed one is a
    surprise (the file gained some other card-DB-hash literal this tool
    doesn't know how to classify) and is treated as a hard error rather
    than silently guessed at.
    """
    if any(identifier in line for identifier in TASK3_REWRITABLE_IDENTIFIERS_V1):
        return False
    if any(identifier in line for identifier in PROTECTED_IDENTIFIERS_V1):
        return True
    raise RepinError(
        f"{rel_path}:{line_no}: unrecognized card-DB-hash literal in "
        f"{PROTECTED_FILE_V1}; add its identifier to PROTECTED_IDENTIFIERS_V1 "
        "or TASK3_REWRITABLE_IDENTIFIERS_V1"
    )


def find_occurrences(files: list[Path], old_grouped: str, old_bare: str) -> list[Occurrence]:
    occurrences: list[Occurrence] = []
    for path in files:
        raw = path.read_bytes()
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            continue
        rel_path = path.relative_to(REPO_ROOT).as_posix()
        protected_file = rel_path == PROTECTED_FILE_V1
        for line_no, line in enumerate(text.splitlines(), start=1):
            for spelling, literal in (("grouped", old_grouped), ("bare", old_bare)):
                if literal in line:
                    is_protected = protected_file and _classify_protected_line(
                        rel_path, line_no, line
                    )
                    occurrences.append(
                        Occurrence(rel_path, line_no, spelling, is_protected)
                    )
    return occurrences


def rewrite_files(
    files: list[Path],
    old_grouped: str,
    old_bare: str,
    new_grouped: str,
    new_bare: str,
) -> list[str]:
    """Rewrite every non-protected occurrence in place.

    Each file is split with `str.splitlines(keepends=True)`, which keeps
    every line's original terminator (LF or CRLF, even mixed within one
    file) attached verbatim; only a line's literal spans are substituted,
    and re-joining the lines reproduces the file's original bytes exactly
    except for those spans. No separate CRLF-vs-LF detection step is
    needed. Working line-by-line (rather than one whole-file bytes
    substitution) also lets PROTECTED_FILE_V1 be handled line-by-line
    instead of skipped wholesale, so a future TASK3_REWRITABLE_IDENTIFIERS_V1
    entry is honored on its own line while every other line in that file
    stays protected.
    """
    changed: list[str] = []
    for path in files:
        rel_path = path.relative_to(REPO_ROOT).as_posix()
        raw = path.read_bytes()
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            continue
        if old_grouped not in text and old_bare not in text:
            continue
        protected_file = rel_path == PROTECTED_FILE_V1
        lines = text.splitlines(keepends=True)
        rewrote_a_line = False
        for index, line in enumerate(lines):
            if old_grouped not in line and old_bare not in line:
                continue
            if protected_file and _classify_protected_line(
                rel_path, index + 1, line
            ):
                continue
            lines[index] = line.replace(old_grouped, new_grouped).replace(
                old_bare, new_bare
            )
            rewrote_a_line = True
        if rewrote_a_line:
            path.write_bytes("".join(lines).encode("utf-8"))
            changed.append(rel_path)
    return changed


PROTECTED_VALUE_PATTERN = re.compile(r'"([0-9a-fA-F]{16})"')


def scan_protected_constants(path: Path) -> list[tuple[int, str, str]]:
    """Return (line_no, identifier, current_bare_hex_value) for every
    PROTECTED_IDENTIFIERS_V1 declaration/assertion found in `path`.

    This is deliberately identity-based (matches the constant's *name*,
    not today's old-literal value): PROTECTED_FILE_V1's constants are
    allowed to lag behind the rest of the tree until Task 3 migrates
    them, so their presence must stay visible even after a --write moves
    every other site's value past what's still pinned here.
    """
    if not path.is_file():
        return []
    text = path.read_bytes().decode("utf-8")
    rows: list[tuple[int, str, str]] = []
    for line_no, line in enumerate(text.splitlines(), start=1):
        for identifier in PROTECTED_IDENTIFIERS_V1:
            if identifier in line:
                match = PROTECTED_VALUE_PATTERN.search(line)
                if match:
                    rows.append((line_no, identifier, match.group(1)))
    return rows


def check_required_sites(found_files: set[str]) -> list[str]:
    missing = []
    for site in REQUIRED_PIN_SITES_V1:
        path = REPO_ROOT / site
        if not path.is_file():
            missing.append(f"{site} (file does not exist)")
            continue
        if site == PROTECTED_FILE_V1:
            if not scan_protected_constants(path):
                missing.append(f"{site} (protected identifiers not found)")
            continue
        if site not in found_files:
            missing.append(site)
    return missing


def run(*, write: bool) -> int:
    try:
        old_hash = parse_old_hash()
        old_grouped = grouped_spelling(old_hash)
        old_bare = bare_spelling(old_hash)

        target_dir = cargo_target_dir()
        live_hash, generated_path, profile = parse_live_hash(target_dir)
        new_grouped = grouped_spelling(live_hash)
        new_bare = bare_spelling(live_hash)

        files = discover_search_files()
        occurrences = find_occurrences(files, old_grouped, old_bare)
    except RepinError as exc:
        print(f"repin_card_db_identity_v1: {exc}", file=sys.stderr)
        return 1

    print(f"card_db_hash {new_bare} ({new_grouped})")
    print(f"generated_from {generated_path} (profile={profile})")

    found_files = {occ.rel_path for occ in occurrences}

    missing_sites = check_required_sites(found_files)
    if missing_sites:
        print("missing pin sites (expected the old literal, found none):")
        for site in sorted(missing_sites):
            print(f"  {site}")
        return 1

    # When the old literal already equals the live hash (nothing added this
    # wave yet), nothing found is actually stale: it already matches.
    stale = [occ for occ in occurrences if not occ.protected] if old_hash != live_hash else []

    if stale:
        print("stale pins:")
        for occ in sorted(stale, key=lambda o: (o.rel_path, o.line_no)):
            print(f"  {occ.rel_path}:{occ.line_no}: {occ.spelling} spelling")

    protected_rows = scan_protected_constants(REPO_ROOT / PROTECTED_FILE_V1)
    if protected_rows:
        print(f"{TRAINING_STORE_LABEL}:")
        for line_no, identifier, value in sorted(protected_rows):
            freshness = (
                "matches the live hash"
                if value == new_bare
                else "stale, not rewritten (Task 3 owns re-pinning it)"
            )
            print(f"  {PROTECTED_FILE_V1}:{line_no}: {identifier} = {value} ({freshness})")

    if write:
        if not stale:
            print("nothing to rewrite (old literal already matches the live hash)")
            return 0
        changed = rewrite_files(files, old_grouped, old_bare, new_grouped, new_bare)
        print("rewrote:")
        for rel_path in sorted(changed):
            print(f"  {rel_path}")
        return 0

    return 1 if stale else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--check",
        action="store_true",
        help="report every stale card-DB-hash pin; exit 1 if any are found",
    )
    mode.add_argument(
        "--write",
        action="store_true",
        help="rewrite every stale card-DB-hash pin to the live value",
    )
    args = parser.parse_args(argv)
    return run(write=args.write)


if __name__ == "__main__":
    raise SystemExit(main())
