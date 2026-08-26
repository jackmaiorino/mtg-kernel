#!/usr/bin/env python3
"""Kernel-native search opponent v1 -- analyzer seed set (seven registered-
lineage layers, item f; KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md "Runtime
and authority integration").

This is a REGISTRATION-time analyzer, not a calibration-panel analyzer: the
design's calibration panels (16-pair throughput screen at base seed
1,987,001; 256-pair matched panels against promoted(2) and the frozen
de-novo line at base seeds 1,988,001 / 1,989,001; the post-parity CP7
skill-7 bridge panel at 1,990,001) have not been run by this stage and this
script does not run them -- per KERNEL-NATIVE-SEARCH-OPPONENT-V1-DESIGN.md
"Calibration after implementation", running those panels is a separate,
later, coordinator-run phase. This script's only job is to validate that a
diagnostic registration-smoke completion record (produced by
run-diagnostic-registration-smoke.ps1, which drives the Rust test
`kernel_native_search_diagnostic_env_surface_is_registered_and_fails_closed`)
names one of the four countersigned calibration seeds and one of the four
registered tiers, matching the same allowlists Rust enforces
(KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1,
KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1 in
kernel_native_search_opponent_v1.rs). A mismatch here without a matching
mismatch on the Rust side would mean the two layers drifted; that is exactly
what this script exists to catch.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Mirrors KERNEL_NATIVE_SEARCH_AUTHORIZED_SEEDS_V1
# (kernel_native_search_opponent_v1.rs). Four countersigned calibration
# seeds: throughput screen, promoted(2) panel, de-novo-line panel, CP7
# skill-7 bridge panel (COUNTERSIGN amendment 4: full-drive uniqueness
# verification is TBD-COORD, required before the first calibration panel
# launches, not before this registration).
AUTHORIZED_SEEDS = (1_987_001, 1_988_001, 1_989_001, 1_990_001)

# Mirrors KERNEL_NATIVE_SEARCH_DIAGNOSTIC_TIER_ALLOWLIST_V1.
REGISTERED_TIERS = ("T512", "T2048", "T8192", "T32768")

# CLAUDE-SEARCHER-POOL-AUTHORITY-SHEET-V1.md (countersigned 6a0db07d),
# Section 5 layer 6 / Section 9.2-9.3. Pool registration is a NARROWER,
# separate surface from the calibration allowlists above: only T2048 is an
# enabled pool tier (T8192 is reserved but not enabled, Section 9.3;
# T512/T32768 are never pool-eligible, Section 4), and the pool action_seed
# is its own array, mirroring KERNEL_NATIVE_SEARCH_AUTHORIZED_POOL_SEEDS_V1
# (kernel_native_search_opponent_v1.rs), never one of the four calibration
# seeds above. Real cycle-3 launch value: 2026082601 (Jack's own
# launch-parameter decision, CLAUDE #345), replacing the prior 2001001
# build-time placeholder; keep this literal and the matching Rust array
# and PowerShell ValidateSet in sync.
POOL_ENABLED_TIERS = ("T2048",)
POOL_AUTHORIZED_ACTION_SEEDS = (2_026_082_601,)


def validate_env_record(record: dict) -> list[str]:
    """Returns a list of violations; empty means the record is registered-valid."""
    violations = []
    tier = record.get("tier")
    if tier not in REGISTERED_TIERS:
        violations.append(f"tier {tier!r} is not one of the registered tiers {REGISTERED_TIERS}")
    seed = record.get("evaluation_seed")
    if seed not in AUTHORIZED_SEEDS:
        violations.append(f"evaluation_seed {seed!r} is not one of the authorized seeds {AUTHORIZED_SEEDS}")
    pairs = record.get("pair_count")
    if not isinstance(pairs, int) or not (1 <= pairs <= 256):
        violations.append(f"pair_count {pairs!r} must be an integer in 1..256")
    return violations


def validate_pool_record(record: dict) -> list[str]:
    """Returns a list of violations; empty means the pool record is
    registered-valid. Pool records use a distinct schema
    ({tier, action_seed}, no pair_count/evaluation_seed): population-pool
    slots are trained continuously, not run as a fixed-pair panel."""
    violations = []
    tier = record.get("tier")
    if tier not in POOL_ENABLED_TIERS:
        violations.append(
            f"pool tier {tier!r} is not enabled; only {POOL_ENABLED_TIERS} may occupy a pool slot "
            "(T8192 is reserved but not yet enabled; see Section 9.3)"
        )
    seed = record.get("action_seed")
    if seed not in POOL_AUTHORIZED_ACTION_SEEDS:
        violations.append(
            f"pool action_seed {seed!r} is not one of the pool-authorized seeds {POOL_AUTHORIZED_ACTION_SEEDS}"
        )
    if seed in AUTHORIZED_SEEDS:
        violations.append(
            f"pool action_seed {seed!r} reuses a calibration seed; the pool and the ruler must never share one"
        )
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("record_json", type=Path, help="path to a {tier, evaluation_seed, pair_count} JSON record")
    parser.add_argument(
        "--pool",
        action="store_true",
        help="validate record_json as a pool-registration {tier, action_seed} record instead",
    )
    args = parser.parse_args()

    record = json.loads(args.record_json.read_text(encoding="utf-8"))
    violations = validate_pool_record(record) if args.pool else validate_env_record(record)
    if violations:
        for violation in violations:
            print(f"REJECTED: {violation}", file=sys.stderr)
        return 1
    if args.pool:
        print("pool-registered-valid: tier and action_seed are on the pool-specific allowlists")
    else:
        print("registered-valid: tier and seed are on the countersigned allowlists")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
