#!/usr/bin/env python3
"""Orchestrate the cycle-3 ACCUMULATION promotion chain (16 sequential
steps + K=5 meta-gate blocks over the refresh-boundary checkpoints).

Authority: CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md Section 6.3 ("Promotion
path: ACCUMULATION class") + Amendment 9 (Jack's ruling: reading (b), the
two-arm mechanism, budget corrected to 573,440 games), implemented per the
countersigned CLAUDE-ACCUMULATION-SPEC-LAYER-PORT-PLAN-V1.md (SHA-256
8574202658a3660be4dd227197563caa7144d4bf277cc14eeecfdad82692b3a5).

Sibling of candidate_02_v3.py (port plan Section 1.2's own conclusion).
Reuses run_payoff_evaluation.py's generic primitives and run_anchor_read.py's
checkpoint_slot unmodified (port plan Section 1.1). The chain-specific
additions candidate_02_v3.py has no equivalent of: identity pinning across
16 candidate boundary checkpoints (Section 3), the sequential
anchor-carrying step loop with both-gates-SUCCESS installation (Section
2.1/6.3), the accepted-step-count-triggered K=5 meta-gate (Section 2.3),
and seed governance across all 35 streams (Section 4).

Per Jack's ruling on Section 2.2 (sheet Amendment 9): reading (b), the
two-arm mechanism structurally reused from candidate_02_v3's own exercised
shape (4 games/cluster) with the second arm's identity generalized from a
literal self-mirror to the anchor -- see accumulation_v1_analysis.py's own
module docstring for the exact difference from candidate_02.

HOLD: per the coordinator's explicit instruction, this module is
implemented and unit-tested but the real 16-step gate sequence is not
launched from here; run_chain() is real, callable orchestration code, but
every acceptance-gate test below drives it with synthetic/mocked
execution, matching this program's own standing discipline (Amendment 7,
Amendment 8, the CP7 harness port all landed only after unit tests, before
any real leg).
"""

from __future__ import annotations

import argparse
import itertools
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Callable

from run_anchor_read import checkpoint_slot
from run_payoff_evaluation import (
    arm_spec,
    file_record,
    git_record,
    load_json,
    run_batch,
    sha256_file,
    toolchain_record,
    unique_attempt_root,
)


CHAIN_SPEC_SCHEMA = "scaled-selfplay-accumulation-v1-chain-spec/v1"
FRESHNESS_SCHEMA = "scaled-selfplay-accumulation-v1-freshness/v1"
CHAIN_STATE_SCHEMA = "scaled-selfplay-accumulation-v1-chain-state/v1"
LEG_SPEC_SCHEMA = "scaled-selfplay-accumulation-v1-leg-spec/v1"
PLAN_SCHEMA = "scaled-selfplay-accumulation-v1-plan/v1"
RECEIPT_SCHEMA = "scaled-selfplay-accumulation-v1-chunk-receipt/v1"
MANIFEST_SCHEMA = "scaled-selfplay-accumulation-v1-execution/v1"

# Section 6.3's own frozen parameters (CLAUDE-POPULATION-V2-CYCLE3-SHEET-V1.md),
# not implementation choices -- validate_chain_spec asserts every one of
# these exactly, per acceptance gate 1.
N_PLANNED_STEPS = 16
K_META_BLOCK = 5
STEP_GATE_CLASS = "ACCUMULATION"
STEP_DELTA_WORTHWHILE = 0.005
STEP_DELTA_PROMOTE = 0.0
GATE_C = 0.5
STEP_ALPHA_INITIAL = 0.00225
STEP_ALPHA_CONFIRM = 0.00225
META_GATE_CLASS = "LARGE-EFFECT"
META_DELTA_WORTHWHILE = 0.025
META_DELTA_PROMOTE = 0.025
META_ALPHA = 0.006
MAX_N_CLUSTERS = 4096
# Amendment 9 (sheet, Jack's ruling reading (b)): 4 games/cluster (2 arms x
# 2 seat-swapped games each), structurally reused from candidate_02_v3's
# own exercised shape -- NOT 2, which was Section 6.3's own original,
# now-corrected arithmetic (reading (a)).
GAMES_PER_CLUSTER = 4
ALPHA_CAMPAIGN = 0.10
ALPHA_RESERVE = 0.01
# Ledger check (Section 6.3, unchanged by Amendment 9 -- only games/cluster
# changed, not alpha): 16*0.0045 + 3*0.006 + 0.01 = 0.10 exactly.
ALPHA_STEP_TOTAL = STEP_ALPHA_INITIAL + STEP_ALPHA_CONFIRM


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def write_new_json(path: Path, value: dict[str, Any]) -> None:
    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())


def durable_file(path: Path) -> None:
    with path.open("rb+") as stream:
        stream.flush()
        os.fsync(stream.fileno())


def analyzer_path() -> Path:
    return Path(__file__).with_name("accumulation_v1_analysis.py").resolve()


def alpha_ledger_check() -> dict[str, Any]:
    # 3 complete meta-gate blocks land within the cycle (Section 6.3: "K = 5
    # -> 3 complete meta-gate blocks... plus one trailing partial step (16)
    # carried forward"), not K_META_BLOCK meta-gates -- the ledger spends
    # 16 steps' own alpha but only 3 meta-gates', by design (Section 2.4).
    total = round(N_PLANNED_STEPS * ALPHA_STEP_TOTAL + 3 * META_ALPHA + ALPHA_RESERVE, 10)
    require(total == ALPHA_CAMPAIGN, f"alpha ledger does not sum to {ALPHA_CAMPAIGN}: got {total}")
    return {
        "n_planned_steps": N_PLANNED_STEPS,
        "alpha_step_total_per_step": ALPHA_STEP_TOTAL,
        "meta_gate_blocks_this_cycle": 3,
        "alpha_meta_per_block": META_ALPHA,
        "alpha_reserve": ALPHA_RESERVE,
        "alpha_campaign": ALPHA_CAMPAIGN,
        "check": f"{N_PLANNED_STEPS}*{ALPHA_STEP_TOTAL} + 3*{META_ALPHA} + {ALPHA_RESERVE} = {total}",
    }


# ---------------------------------------------------------------------------
# Seed governance (Section 4, PROPOSED mechanism; values chosen here are the
# implementation-time choice the port plan explicitly leaves open)
# ---------------------------------------------------------------------------


def stream_block_size(evaluation_seed_stride: int, chunks: int) -> int:
    """The full seed-width one stream's own chunk_count-many chunks span
    (chunks apart by evaluation_seed_stride each). Inter-stream spacing
    below is exactly one block apart, not one intra-stream stride apart --
    the bug an earlier revision of this module had (verified by actually
    running the sweep, not merely by inspection: streams spanning 31 chunks
    of 1,000,000 each are 31,000,000 wide, so spacing stream STARTS only
    1,000,000 apart collided every stream with every other one; caught by
    check_pairwise_disjoint itself, the exact mechanism acceptance gate 6
    requires). One full block's gap between a stream's own last seed
    (first + (chunks-1)*stride) and the next stream's first
    (first + chunks*stride) leaves a clean stride-wide margin, not a
    touching boundary."""
    return chunks * evaluation_seed_stride


def first_evaluation_seed(accum_base_seed: int, evaluation_seed_stride: int, chunks: int, step_index: int, leg_offset: int) -> int:
    """step_index in 1..16, leg_offset in {0=initial, 1=confirm}. Meta-gates
    draw from a disjoint high band (see meta_first_evaluation_seed) so no
    step/meta arithmetic can ever collide by construction."""
    require(1 <= step_index <= N_PLANNED_STEPS, "step_index out of range")
    require(leg_offset in (0, 1), "leg_offset must be 0 (initial) or 1 (confirm)")
    block = stream_block_size(evaluation_seed_stride, chunks)
    return accum_base_seed + (step_index * 2 + leg_offset) * block


def meta_first_evaluation_seed(accum_base_seed: int, evaluation_seed_stride: int, chunks: int, meta_index: int) -> int:
    require(1 <= meta_index <= 3, "meta_index out of range for this cycle (3 complete blocks)")
    block = stream_block_size(evaluation_seed_stride, chunks)
    return accum_base_seed + (100 + meta_index) * block


def all_stream_labels() -> list[str]:
    labels = []
    for step_index in range(1, N_PLANNED_STEPS + 1):
        labels.append(f"step-{step_index:02d}-initial")
        labels.append(f"step-{step_index:02d}-confirm")
    for meta_index in range(1, 4):
        labels.append(f"meta-{meta_index:02d}")
    return labels


def stream_seed_ranges(
    accum_base_seed: int, evaluation_seed_stride: int, chunk_pair_count: int, max_n_clusters: int,
) -> dict[str, tuple[int, int]]:
    """Every one of the 35 streams' own [first_seed, last_seed] inclusive
    range, at the frozen chunk size max_N_clusters/chunk_pair_count chunks
    apart by evaluation_seed_stride each. Used both to build the freshness
    manifest and to check pairwise disjointness programmatically (acceptance
    gate 6), not merely by the construction formula's own non-collision
    argument."""
    require(max_n_clusters % chunk_pair_count == 0, "max_N must be divisible by chunk_pair_count")
    chunks = max_n_clusters // chunk_pair_count
    ranges: dict[str, tuple[int, int]] = {}
    for step_index in range(1, N_PLANNED_STEPS + 1):
        for leg_offset, name in ((0, "initial"), (1, "confirm")):
            first = first_evaluation_seed(accum_base_seed, evaluation_seed_stride, chunks, step_index, leg_offset)
            last = first + (chunks - 1) * evaluation_seed_stride
            ranges[f"step-{step_index:02d}-{name}"] = (first, last)
    for meta_index in range(1, 4):
        first = meta_first_evaluation_seed(accum_base_seed, evaluation_seed_stride, chunks, meta_index)
        last = first + (chunks - 1) * evaluation_seed_stride
        ranges[f"meta-{meta_index:02d}"] = (first, last)
    require(len(ranges) == 35, f"expected 35 streams (16*2+3), got {len(ranges)}")
    return ranges


def check_pairwise_disjoint(ranges: dict[str, tuple[int, int]]) -> list[tuple[str, str]]:
    """Returns every colliding pair (empty means fully disjoint). Checked
    programmatically over all C(35,2)=595 pairs, not asserted by the
    construction formula's own non-collision argument alone (acceptance
    gate 6)."""
    collisions: list[tuple[str, str]] = []
    for (name_a, (start_a, end_a)), (name_b, (start_b, end_b)) in itertools.combinations(ranges.items(), 2):
        if start_a <= end_b and start_b <= end_a:
            collisions.append((name_a, name_b))
    return collisions


def check_excluded_intervals(
    ranges: dict[str, tuple[int, int]], excluded: list[dict[str, int]],
) -> list[tuple[str, dict[str, int]]]:
    """Returns every (stream_name, excluded_interval) overlap found (empty
    means clean). `excluded` entries are {"start_inclusive": int,
    "end_inclusive": int, "label": str}."""
    overlaps: list[tuple[str, dict[str, int]]] = []
    for name, (start, end) in ranges.items():
        for interval in excluded:
            if start <= interval["end_inclusive"] and interval["start_inclusive"] <= end:
                overlaps.append((name, interval))
    return overlaps


def build_freshness_manifest(
    accum_base_seed: int,
    evaluation_seed_stride: int,
    chunk_pair_count: int,
    max_n_clusters: int,
    excluded_evaluation_seed_intervals: list[dict[str, Any]],
) -> dict[str, Any]:
    """Runs the freshness sweep for real: computes all 35 streams' own seed
    ranges, checks pairwise disjointness across all of them, and checks
    each against every excluded interval. Raises if either check fails --
    the manifest this function returns is only ever a CLEAN sweep's
    record, never a report of a failed one (a failed sweep is a defect to
    fix before any spec is built, not evidence to bank)."""
    ranges = stream_seed_ranges(accum_base_seed, evaluation_seed_stride, chunk_pair_count, max_n_clusters)
    collisions = check_pairwise_disjoint(ranges)
    require(not collisions, f"stream seed ranges collide: {collisions}")
    overlaps = check_excluded_intervals(ranges, excluded_evaluation_seed_intervals)
    require(not overlaps, f"stream seed ranges overlap excluded intervals: {overlaps}")
    return {
        "schema": FRESHNESS_SCHEMA,
        "accum_base_seed": accum_base_seed,
        "evaluation_seed_stride": evaluation_seed_stride,
        "chunk_pair_count": chunk_pair_count,
        "max_N_clusters": max_n_clusters,
        "stream_count": len(ranges),
        "streams": [
            {"name": name, "first_evaluation_seed": start, "last_evaluation_seed": end}
            for name, (start, end) in sorted(ranges.items())
        ],
        "pairwise_disjoint": True,
        "excluded_evaluation_seed_intervals": excluded_evaluation_seed_intervals,
        "excluded_intervals_clean": True,
        "policy": (
            "every one of the 35 ACCUMULATION streams (16 steps x "
            "{initial, confirm} + 3 meta-gates) must have a pairwise-disjoint "
            "evaluation-seed range, and none may overlap any previously "
            "revealed native H2H development/selection evaluation-seed "
            "interval"
        ),
    }


# ---------------------------------------------------------------------------
# Identity pinning (Section 3)
# ---------------------------------------------------------------------------


def spec_identity(store_root: Path, base_seed: int, generation: int, role: str) -> dict[str, Any]:
    """checkpoint_slot()'s own raw output (run_anchor_read.py, Section 1.1,
    reused unmodified) uses source_run_sha256/source_generation as its own
    field names; the FROZEN SPEC identity shape this module's leg specs and
    accumulation_v1_analysis.py's own validate_identity() expect is
    candidate_02_v3.py's own real spec shape instead (run_sha256/generation,
    plus identity_bundle_sha256, which checkpoint_slot() does not include at
    all) -- this is the same remap candidate_02_v3.py's own slot_from_spec()
    performs, done here once for every identity this module ever builds
    rather than re-derived ad hoc per call site."""
    raw = checkpoint_slot(store_root, base_seed, generation, role)
    checkpoint = load_json(store_root.resolve() / "checkpoints" / f"update-{generation:08d}.checkpoint.json")
    return {
        "role": raw["role"],
        "store_root": raw["store_root"],
        "run_sha256": raw["source_run_sha256"],
        "generation": raw["source_generation"],
        "checkpoint_sha256": raw["checkpoint_sha256"],
        "sidecar_sha256": raw["sidecar_sha256"],
        "state_sha256": raw["state_sha256"],
        "model_parameter_sha256": raw["model_parameter_sha256"],
        "identity_bundle_sha256": checkpoint["identity_bundle_sha256"],
    }


def build_candidate_identities(store_root: Path, base_seed: int) -> list[dict[str, Any]]:
    """The 16 candidate identities, one per step, at local generation
    128*step_index (Section 3: "the cycle-3 store's own heads at local
    generation 128, 256, ..., 2048")."""
    return [
        spec_identity(store_root, base_seed, step_index * 128, f"candidate-generation-{step_index * 128:04d}")
        for step_index in range(1, N_PLANNED_STEPS + 1)
    ]


def validate_chain_spec(path: Path) -> dict[str, Any]:
    """Acceptance gate 1: schema, gate_class, the exact Section 6.3
    parameter values, and the 16-boundary candidate identity list resolved
    against the real cycle-3 store."""
    chain_spec = load_json(path)
    require(chain_spec.get("schema") == CHAIN_SPEC_SCHEMA, "unexpected chain spec schema")
    require(chain_spec["gate_class"] == STEP_GATE_CLASS, "step gate_class changed")
    require(chain_spec["n_planned_steps"] == N_PLANNED_STEPS, "n_planned_steps changed")
    require(chain_spec["k_meta_block"] == K_META_BLOCK, "k_meta_block changed")
    require(chain_spec["step_alpha_initial"] == STEP_ALPHA_INITIAL, "step_alpha_initial changed")
    require(chain_spec["step_alpha_confirm"] == STEP_ALPHA_CONFIRM, "step_alpha_confirm changed")
    require(chain_spec["meta_alpha"] == META_ALPHA, "meta_alpha changed")
    require(chain_spec["delta_worthwhile"] == STEP_DELTA_WORTHWHILE, "delta_worthwhile changed")
    require(chain_spec["delta_promote"] == STEP_DELTA_PROMOTE, "delta_promote changed")
    require(chain_spec["meta_gate_class"] == META_GATE_CLASS, "meta_gate_class changed")
    require(chain_spec["meta_delta_worthwhile"] == META_DELTA_WORTHWHILE, "meta_delta_worthwhile changed")
    require(chain_spec["meta_delta_promote"] == META_DELTA_PROMOTE, "meta_delta_promote changed")
    require(chain_spec["c"] == GATE_C, "c changed")
    require(chain_spec["max_N_clusters"] == MAX_N_CLUSTERS, "max_N_clusters changed")
    require(chain_spec["games_per_cluster"] == GAMES_PER_CLUSTER, "games_per_cluster changed (Amendment 9 reading (b) = 4)")
    ledger = alpha_ledger_check()
    require(chain_spec["alpha_ledger"] == ledger, "alpha ledger mismatch")
    candidates = chain_spec["candidate_identities"]
    require(type(candidates) is list and len(candidates) == N_PLANNED_STEPS, "candidate identity list must have 16 entries")
    store_root = Path(chain_spec["candidate_store_root"]).resolve()
    require(store_root.is_dir(), "candidate store root is missing")
    run = load_json(store_root / "run.json")
    base_seed = int(run["schedule"]["base_seed"])
    live_candidates = build_candidate_identities(store_root, base_seed)
    for step_index, (recorded, live) in enumerate(zip(candidates, live_candidates, strict=True), start=1):
        require(recorded == live, f"step {step_index} candidate identity does not match the live store")
    freshness = load_json(Path(chain_spec["freshness_manifest"]["path"]))
    require(freshness.get("schema") == FRESHNESS_SCHEMA, "freshness manifest schema mismatch")
    require(sha256_file(Path(chain_spec["freshness_manifest"]["path"])) == chain_spec["freshness_manifest"]["sha256"], "freshness manifest hash mismatch")
    require(freshness["pairwise_disjoint"] is True and freshness["excluded_intervals_clean"] is True, "freshness sweep did not pass clean")
    return chain_spec


# ---------------------------------------------------------------------------
# Per-leg execution (mirrors candidate_02_v3.py's own context/screen/
# formal_run, adapted to reading (b)'s 3-identity, 2-arm shape)
# ---------------------------------------------------------------------------


def leg_spec_for_step(
    chain_spec: dict[str, Any],
    step_index: int,
    mode_leg: str,
    anchor_identity: dict[str, Any],
    gate_id: str,
) -> dict[str, Any]:
    """Builds one step-leg's own spec fragment (accumulation_v1_analysis.py's
    LEG_SPEC_SCHEMA), binding the candidate at this step's own boundary
    checkpoint, the CURRENT anchor (carried in from the chain's own state,
    Section 2.1), and the fixed comparator. `mode_leg` is 'initial' or
    'confirmation' -- only its own gate_id/first_evaluation_seed differ;
    both share the same candidate/anchor/fixed_opponent identities and gate
    parameters, matching Section 6.3's own "both arms are FIXED, frozen
    checkpoints for the duration of that step's reads" requirement."""
    require(mode_leg in ("initial", "confirmation"), "invalid step leg mode")
    candidate = chain_spec["candidate_identities"][step_index - 1]
    chunks = chain_spec["max_N_clusters"] // chain_spec["chunk_pair_count"]
    stride = chain_spec["evaluation_seed_stride"]
    base = chain_spec["accum_base_seed"]
    # Both real seeds are always computed, regardless of which mode this
    # particular leg_spec instance is about to be launched for: the
    # confirmation leg's own seed is a real, meaningful, already-frozen
    # value even while only the initial leg is running (Section 4's own
    # per-leg formula), not a placeholder -- see _leg_spec_common.
    seeds = {
        "initial": first_evaluation_seed(base, stride, chunks, step_index, 0),
        "confirmation": first_evaluation_seed(base, stride, chunks, step_index, 1),
    }
    gate_ids = {
        mode_leg: gate_id,
        ("confirmation" if mode_leg == "initial" else "initial"): f"accumulation-step-{step_index:02d}-{'confirmation' if mode_leg == 'initial' else 'initial'}",
    }
    return _leg_spec_common(
        chain_spec,
        leg_id=f"step-{step_index:02d}-{mode_leg}",
        gate_class=STEP_GATE_CLASS,
        alpha=STEP_ALPHA_INITIAL if mode_leg == "initial" else STEP_ALPHA_CONFIRM,
        delta_promote=STEP_DELTA_PROMOTE,
        delta_worthwhile=STEP_DELTA_WORTHWHILE,
        candidate=candidate,
        anchor=anchor_identity,
        gate_ids=gate_ids,
        seeds=seeds,
    )


def leg_spec_for_meta(
    chain_spec: dict[str, Any],
    meta_index: int,
    block_start_anchor: dict[str, Any],
    current_anchor: dict[str, Any],
    gate_id: str,
) -> dict[str, Any]:
    """A meta-gate is structurally the same leg shape as a step (Section
    2.3: "the SAME leg/cluster mechanism as a step... just gate_class=
    LARGE-EFFECT instead of ACCUMULATION, comparing anchor-at-block-end vs
    anchor-at-block-start"). candidate=current anchor, anchor=block-start
    anchor. No confirmation leg (Section 6.3: "no further confirmation
    gate needed") -- callers only ever request mode_leg='initial' for a
    meta-gate; there is no leg_spec_for_meta confirmation variant."""
    chunks = chain_spec["max_N_clusters"] // chain_spec["chunk_pair_count"]
    stride = chain_spec["evaluation_seed_stride"]
    base = chain_spec["accum_base_seed"]
    initial_seed = meta_first_evaluation_seed(base, stride, chunks, meta_index)
    # Meta-gates have no confirmation leg (Section 6.3: "no further
    # confirmation gate needed") -- there is no 36th/37th/38th official
    # stream for this. The confirmation slot below is a well-formed but
    # genuinely synthetic, NEVER-launched entry, offset 1000 stream-blocks
    # past the real 35-stream layout (steps occupy block indexes 2..33,
    # meta 101..103) so it cannot collide with any real stream even though
    # it is never included in build_freshness_manifest's own 35-stream
    # sweep.
    synthetic_confirm_seed = base + 1000 * stream_block_size(stride, chunks) + meta_index * stream_block_size(stride, chunks)
    seeds = {"initial": initial_seed, "confirmation": synthetic_confirm_seed}
    gate_ids = {"initial": gate_id, "confirmation": f"{gate_id}-no-confirmation-gate"}
    return _leg_spec_common(
        chain_spec,
        leg_id=f"meta-{meta_index:02d}",
        gate_class=META_GATE_CLASS,
        alpha=META_ALPHA,
        delta_promote=META_DELTA_PROMOTE,
        delta_worthwhile=META_DELTA_WORTHWHILE,
        candidate=current_anchor,
        anchor=block_start_anchor,
        gate_ids=gate_ids,
        seeds=seeds,
    )


def _leg_spec_common(
    chain_spec: dict[str, Any],
    *,
    leg_id: str,
    gate_class: str,
    alpha: float,
    delta_promote: float,
    delta_worthwhile: float,
    candidate: dict[str, Any],
    anchor: dict[str, Any],
    gate_ids: dict[str, str],
    seeds: dict[str, int],
) -> dict[str, Any]:
    """Builds a fully self-consistent leg spec: EVERY mode (screen, initial,
    confirmation) gets a real first_evaluation_seed and a correctly
    computed pre_outcome_schedule_sha256, not a placeholder -- validate_leg_spec
    (accumulation_v1_analysis.py) checks all three unconditionally, so a
    stubbed hash would always fail validation for a mode this leg is not
    currently being launched for. `seeds`/`gate_ids` must carry
    'initial'/'confirmation' keys; 'screen' is derived here from the
    chain-level screen block, shared identically by every leg (screening
    happens once, chain-wide, matching candidate_02_v3.py's own screen()
    shape -- Section 6 acceptance gate 8)."""
    chunk_pair_count = chain_spec["chunk_pair_count"]
    max_n = chain_spec["max_N_clusters"]
    stride = chain_spec["evaluation_seed_stride"]
    leg = {
        "schema": LEG_SPEC_SCHEMA,
        "leg_id": leg_id,
        "gate_class": gate_class,
        "alpha": alpha,
        "c": chain_spec["c"],
        "delta_promote": delta_promote,
        "delta_worthwhile": delta_worthwhile,
        "conditional_mean_stability": "BOTH-ARMS-FIXED-PER-LEG",
        "chunk_pair_count": chunk_pair_count,
        "max_N": max_n,
        "candidate": candidate,
        "anchor": anchor,
        "fixed_opponent": chain_spec["fixed_opponent"],
        "screen": {
            "chunk_count": chain_spec["screen"]["chunk_count"],
            "pair_count_per_chunk": chain_spec["screen"]["pair_count_per_chunk"],
            "gate_id": f"{leg_id}-screen",
            "first_evaluation_seed": chain_spec["screen"]["first_evaluation_seed"],
            "pre_outcome_schedule_sha256": "",
        },
        "initial": {"gate_id": gate_ids["initial"], "first_evaluation_seed": seeds["initial"], "pre_outcome_schedule_sha256": ""},
        "confirmation": {"gate_id": gate_ids["confirmation"], "first_evaluation_seed": seeds["confirmation"], "pre_outcome_schedule_sha256": ""},
        "evaluation_seed_stride": stride,
        "expected_rally_deck_hashes_u64": chain_spec["expected_rally_deck_hashes_u64"],
        "executable": chain_spec["executable"],
        "contract": chain_spec["contract"],
    }
    from accumulation_v1_analysis import schedule_identifiers as _schedule_identifiers, load_reference as _load_reference

    reference = _load_reference(leg)
    for mode in ("screen", "initial", "confirmation"):
        identifiers = _schedule_identifiers(leg, mode)
        leg[mode]["pre_outcome_schedule_sha256"] = reference.canonical_ordered_identifier_sha256(identifiers)
    return leg


def slot_from_spec_identity(identity: dict[str, Any]) -> dict[str, Any]:
    """Re-derives the RAW checkpoint_slot() shape run_payoff_evaluation.py's
    own run_arm() actually reads off its candidate/opponent slots (role,
    store_root, source_generation -- used to build the
    H2H_CANDIDATE_GEN/H2H_OPPONENT_GEN environment variables) from a FROZEN
    SPEC identity (spec_identity()'s own run_sha256/generation/
    identity_bundle_sha256 shape), cross-validating every pinned hash
    against the live store at leg-execution time -- structurally identical
    to candidate_02_v3.py's own slot_from_spec(), which this module was
    missing.

    Real bug this closes (2026-08-28, first real chain launch, caught by
    actually running the code, not by any of the 20 unit tests that
    predated it): leg_context() was passing the spec-shaped identity
    dicts directly as 'slots' into run_batch/run_arm, which raised
    KeyError('source_generation') the instant the first real leg tried to
    build its env vars -- spec_identity() names that field 'generation',
    not 'source_generation', and never carries 'source_generation' at
    all. Every acceptance-gate test stubs run_arm_fn and never exercises
    the real run_payoff_evaluation.run_arm(), so this gap was invisible
    to the test suite until a real leg actually ran."""
    root = Path(identity["store_root"]).resolve()
    run = load_json(root / "run.json")
    seed = int(run["schedule"]["base_seed"])
    slot = checkpoint_slot(root, seed, int(identity["generation"]), identity["role"])
    checkpoint = load_json(root / "checkpoints" / f"update-{int(identity['generation']):08d}.checkpoint.json")
    require(checkpoint["identity_bundle_sha256"] == identity["identity_bundle_sha256"], f"{identity['role']} identity bundle mismatch")
    slot["identity_bundle_sha256"] = checkpoint["identity_bundle_sha256"]
    for field in ("source_run_sha256", "checkpoint_sha256", "sidecar_sha256", "state_sha256", "model_parameter_sha256"):
        spec_field = "run_sha256" if field == "source_run_sha256" else field
        require(slot[field] == identity[spec_field], f"{identity['role']} {spec_field} mismatch")
    return slot


def leg_context(repo_root: Path, leg_spec: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    executable = Path(leg_spec["executable"]["path"]).resolve()
    require(executable.is_file(), "accumulation executable is missing")
    require(sha256_file(executable) == leg_spec["executable"]["sha256"], "accumulation executable hash mismatch")
    slots = [
        slot_from_spec_identity(leg_spec["candidate"]),
        slot_from_spec_identity(leg_spec["anchor"]),
        slot_from_spec_identity(leg_spec["fixed_opponent"]),
    ]
    require(len({slot["model_parameter_sha256"] for slot in slots}) in (2, 3), "candidate/anchor/opponent slots are not distinct as expected")
    return (
        {
            "git": git_record(repo_root, leg_spec["executable"]["source_commit"]),
            "toolchain": toolchain_record(repo_root),
            "executable": {**file_record(executable), "source_commit": leg_spec["executable"]["source_commit"]},
            "candidate": leg_spec["candidate"],
            "anchor": leg_spec["anchor"],
            "fixed_opponent": leg_spec["fixed_opponent"],
            "gpu_ordinal": "not-used; ACCUMULATION's H2H evaluator is CPU-resident (Section 6.3: no CP7/GPU involvement)",
            "terminal_reward_only": True,
        },
        slots,
    )


def normalize_arm_record(record: dict[str, Any]) -> dict[str, Any]:
    normalized = {key: record[key] for key in ("label", "candidate_index", "opponent_index", "pair_count", "evaluation_seed", "exit_code", "wall_seconds", "stdout", "stderr", "outcome")}
    for file_key in ("stdout", "stderr", "outcome"):
        path = Path(normalized[file_key]["path"])
        durable_file(path)
        normalized[file_key] = file_record(path)
    return normalized


def acquire_chunk_batch(
    executable: Path,
    repo_root: Path,
    root: Path,
    slots: list[dict[str, Any]],
    leg_spec: dict[str, Any],
    mode: str,
    chunk_indexes: list[int],
    concurrency: int,
    run_arm_fn: Callable[..., dict[str, Any]] | None = None,
) -> tuple[list[dict[str, Any]], float]:
    from accumulation_v1_analysis import chunk_seed as _chunk_seed, mode_pair_count as _mode_pair_count

    seeds_by_index = {index: _chunk_seed(leg_spec, mode, index) for index in chunk_indexes}
    pair_count = _mode_pair_count(leg_spec, mode)
    arm_specs: list[dict[str, Any]] = []
    for chunk_index in chunk_indexes:
        evaluation_seed = seeds_by_index[chunk_index]
        arm_specs.extend(
            [
                arm_spec(f"chunk-{chunk_index:03d}-candidate", 0, 2, pair_count, evaluation_seed),
                arm_spec(f"chunk-{chunk_index:03d}-control", 1, 2, pair_count, evaluation_seed),
            ]
        )
    if run_arm_fn is None:
        batch, wall = run_batch(executable, repo_root, root, slots, arm_specs, concurrency)
    else:
        # Test-only seam (acceptance gates 4/5/7 drive this with a stub, per
        # this program's own standing discipline of unit-testing gate logic
        # without real subprocess execution before any real leg is
        # launched); production callers never pass run_arm_fn.
        import concurrent.futures

        started = time.perf_counter()
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
            futures = [executor.submit(run_arm_fn, executable, repo_root, root, slots, spec) for spec in arm_specs]
            batch = [future.result() for future in futures]
        wall = time.perf_counter() - started
    from accumulation_v1_analysis import validate_outcome as _validate_outcome

    normalized = []
    for arm, record in zip(arm_specs, batch, strict=True):
        # label shape: "chunk-{index:03d}-{candidate|control}"
        _, chunk_index_text, arm_name = arm["label"].split("-", 2)
        _validate_outcome(Path(record["outcome"]["path"]), leg_spec, mode, int(chunk_index_text), arm_name)
        normalized.append(normalize_arm_record(record))
    receipts = []
    for offset, chunk_index in enumerate(chunk_indexes):
        receipt = {
            "schema": RECEIPT_SCHEMA,
            "chunk_index": chunk_index,
            "evaluation_seed": seeds_by_index[chunk_index],
            "candidate_arm": normalized[offset * 2],
            "control_arm": normalized[offset * 2 + 1],
        }
        receipt_path = root / f"chunk-{chunk_index:03d}-receipt.json"
        write_new_json(receipt_path, receipt)
        receipts.append(file_record(receipt_path))
    return receipts, wall


def existing_manifest(root_glob_parent: Path, gate_id: str) -> Path | None:
    """Resumability (Section 5, acceptance gate 7): a prior attempt root for
    this exact gate_id whose own gate-execution-manifest.json has
    passed=True. Mirrors formal_run(mode='confirmation')'s own
    trust-the-prior-manifest pattern -- found, loaded, and independently
    hash-validated, never re-executed."""
    gate_root = root_glob_parent / gate_id
    if not gate_root.is_dir():
        return None
    for attempt_dir in sorted(gate_root.glob("attempt-*")):
        manifest_path = attempt_dir / "gate-execution-manifest.json"
        if manifest_path.is_file():
            manifest = load_json(manifest_path)
            if manifest.get("schema") == MANIFEST_SCHEMA and manifest.get("passed") is True and manifest.get("gate_id") == gate_id:
                return manifest_path
    return None


def formal_run(
    evidence_root: Path,
    repo_root: Path,
    leg_spec: dict[str, Any],
    leg_spec_path: Path,
    mode: str,
    initial_manifest: Path | None = None,
    run_arm_fn: Callable[..., dict[str, Any]] | None = None,
) -> Path:
    """One leg's own formal acquisition (a step's initial or confirmation
    gate, or a meta-gate's single leg). Resumability is checked by the
    CALLER (run_chain), which passes an already-resolved gate_id and only
    invokes this when existing_manifest() found nothing -- formal_run
    itself always executes when called, matching candidate_02_v3.py's own
    formal_run shape exactly."""
    gate_id = leg_spec[mode]["gate_id"]
    run_context, slots = leg_context(repo_root, leg_spec)
    root = unique_attempt_root(evidence_root.resolve(), gate_id)
    from accumulation_v1_analysis import (
        chunk_seed as _chunk_seed,
        mode_max_n as _mode_max_n,
        mode_pair_count as _mode_pair_count,
        validate_leg_spec as _validate_leg_spec,
        build_analysis as _build_analysis,
        verify_existing as _verify_existing,
    )

    _validate_leg_spec(leg_spec_path)
    initial_verification_record = None
    if mode == "confirmation":
        require(initial_manifest is not None, "confirmation requires the step's own initial manifest")
        verification_path = root / "initial-independent-verification.json"
        initial_manifest_doc = load_json(initial_manifest)
        require(initial_manifest_doc.get("schema") == MANIFEST_SCHEMA and initial_manifest_doc.get("mode") == "initial", "initial manifest shape is invalid")
        retained_analysis = Path(initial_manifest_doc["analysis"]["path"]).resolve()
        # BUG FIX (2026-08-29, caught live at step-07-confirm on attempt-002,
        # ~7h into the overnight chain): this used to pass `root` here, which
        # is the CONFIRMATION leg's own fresh, empty attempt root (bound
        # above), not the INITIAL leg's own root whose durable gate-plan.json
        # / chunk-receipts verify_existing()'s own reconstruct() needs to
        # recompute the retained analysis from raw receipts. `initial_manifest`
        # is itself the initial leg's own gate-execution-manifest.json path
        # (either freshly returned by formal_run(mode="initial") or recovered
        # by existing_manifest() on resume -- both cases put that file directly
        # under the initial leg's own run root), so its parent IS that root.
        # Not caught by test_confirmation_requires_independent_verification_of_initial
        # because that test calls verify_existing() directly with one shared
        # root standing in for "the initial leg's root" -- it never goes
        # through formal_run()'s own confirmation-mode wiring at all, so it
        # cannot see which root formal_run chooses to pass.
        initial_run_root = initial_manifest.resolve().parent
        # SECOND BUG FIX (2026-08-29, caught live on the very next relaunch,
        # after the root fix above let execution reach validate_plan() for
        # the first time): this used to pass `leg_spec_path` here, which is
        # formal_run's OWN incoming parameter -- for a mode="confirmation"
        # call, that is the CONFIRMATION leg's own spec file (leg_id=
        # "step-NN-confirmation"), not the INITIAL leg's own spec file
        # (leg_id="step-NN-initial") the initial leg's own gate-plan.json
        # was actually built and recorded against. validate_plan's own
        # require(plan["leg_id"] == leg_spec["leg_id"], ...) (and its
        # plan["leg_spec"] file-record cross-check) correctly rejected the
        # mismatch: ValueError('leg id mismatch'). The initial leg's own
        # manifest durably records exactly which spec file it used
        # (manifest["leg_spec"]), so recovering it from there -- rather
        # than reusing formal_run's own incoming leg_spec_path, which only
        # ever names the CURRENT (confirmation) leg -- is the correct fix,
        # mirroring the initial_run_root fix's own principle.
        initial_leg_spec_path = Path(initial_manifest_doc["leg_spec"]["path"]).resolve()
        _verify_existing(initial_run_root, initial_leg_spec_path, retained_analysis, verification_path)
        initial_verification_record = file_record(verification_path)
    max_n = _mode_max_n(leg_spec, mode)
    pair_count = _mode_pair_count(leg_spec, mode)
    chunk_count = max_n // pair_count
    plan = {
        "schema": PLAN_SCHEMA,
        "mode": mode,
        "gate_id": gate_id,
        "leg_id": leg_spec["leg_id"],
        "leg_spec": file_record(leg_spec_path),
        **run_context,
        "gate": {
            "gate_class": leg_spec["gate_class"], "alpha": leg_spec["alpha"], "c": leg_spec["c"],
            "delta_promote": leg_spec["delta_promote"], "delta_worthwhile": leg_spec["delta_worthwhile"],
            "conditional_mean_stability": leg_spec["conditional_mean_stability"],
            "max_N": max_n, "chunk_pair_count": pair_count,
        },
        "pre_outcome_schedule_sha256": leg_spec[mode]["pre_outcome_schedule_sha256"],
        "first_evaluation_seed": leg_spec[mode]["first_evaluation_seed"],
        "evaluation_seed_stride": leg_spec["evaluation_seed_stride"],
        "arm_order": "candidate and control are acquired together; both play the fixed_opponent independently under shared CRN roots",
        "expected_rally_deck_hashes_u64": leg_spec["expected_rally_deck_hashes_u64"],
        "chunk_plan": [
            {
                "chunk_index": index,
                "evaluation_seed": _chunk_seed(leg_spec, mode, index),
                "global_cluster_start": index * pair_count,
                "global_cluster_end_exclusive": (index + 1) * pair_count,
            }
            for index in range(chunk_count)
        ],
    }
    plan_path = root / "gate-plan.json"
    write_new_json(plan_path, plan)
    executable = Path(leg_spec["executable"]["path"]).resolve()
    concurrent_chunks = 2
    started = time.perf_counter()
    for wave_start in range(0, chunk_count, concurrent_chunks):
        chunk_indexes = list(range(wave_start, min(wave_start + concurrent_chunks, chunk_count)))
        acquire_chunk_batch(executable, repo_root, root, slots, leg_spec, mode, chunk_indexes, 4, run_arm_fn)
        acquired_n = (chunk_indexes[-1] + 1) * pair_count
        look_path = root / f"analysis-look-{acquired_n:06d}.json"
        look = _build_analysis(root, leg_spec_path, mode, False)
        write_new_json(look_path, look)
        if look["decision"] != "CONTINUE":
            break
    final_analysis_path = root / "analysis.json"
    final_analysis = _build_analysis(root, leg_spec_path, mode, True)
    write_new_json(final_analysis_path, final_analysis)
    require(final_analysis["decision"] != "CONTINUE", "formal acquisition ended before a terminal gate decision")
    wall_seconds = time.perf_counter() - started
    total_games = GAMES_PER_CLUSTER * int(final_analysis["acquired_N"])
    receipt_records = [file_record(path) for path in sorted(root.glob("chunk-*-receipt.json"))]
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "passed": True,
        "mode": mode,
        "gate_id": gate_id,
        "leg_id": leg_spec["leg_id"],
        "disposition": final_analysis["decision"],
        "plan": file_record(plan_path),
        "leg_spec": file_record(leg_spec_path),
        "initial_verification": initial_verification_record,
        "analysis": file_record(final_analysis_path),
        "analysis_summary": {key: final_analysis[key] for key in ("decision", "decision_N", "acquired_N", "delta_hat", "cs_delta_lower", "cs_delta_upper", "acquired_stream_sha256", "decision_prefix_stream_sha256")},
        "chunk_receipts": receipt_records,
        "wall_seconds": wall_seconds,
        "total_game_count": total_games,
        "aggregate_games_per_second": total_games / wall_seconds if wall_seconds > 0 else None,
        "terminal_reward_only": True,
    }
    manifest_path = root / "gate-execution-manifest.json"
    write_new_json(manifest_path, manifest)
    return manifest_path


# ---------------------------------------------------------------------------
# The sequential 16-step chain + K=5 meta-gate driver (Section 2.1/2.3/5)
# ---------------------------------------------------------------------------


def load_chain_state(evidence_root: Path) -> dict[str, Any]:
    state_path = evidence_root / "chain-state.json"
    if state_path.is_file():
        state = load_json(state_path)
        require(state.get("schema") == CHAIN_STATE_SCHEMA, "chain state schema mismatch")
        return state
    return {
        "schema": CHAIN_STATE_SCHEMA,
        "current_anchor": None,  # set to chain_spec["initial_anchor"] on first use
        "accepted_step_count": 0,
        "completed_steps": [],
        "completed_meta_gates": [],
        "block_start_anchor": None,
    }


def write_chain_state(evidence_root: Path, state: dict[str, Any]) -> None:
    state_path = evidence_root / "chain-state.json"
    tmp_path = evidence_root / "chain-state.json.tmp"
    tmp_path.write_bytes(canonical_bytes(state))
    os.replace(tmp_path, state_path)


def run_chain(
    evidence_root: Path,
    repo_root: Path,
    chain_spec: dict[str, Any],
    chain_spec_path: Path,
    run_arm_fn: Callable[..., dict[str, Any]] | None = None,
    max_steps: int | None = None,
) -> dict[str, Any]:
    """The 16-step sequential loop (Section 2.1/5): step i+1 cannot start
    until step i's install-or-carry decision is final. Resumability
    (Section 5, acceptance gate 7): before launching any leg, checks for an
    existing gate-execution-manifest.json with passed=True; if found, it is
    loaded and trusted (not re-executed). The meta-gate fires when
    accepted_step_count crosses a multiple of K_META_BLOCK (Section 2.3,
    acceptance gate 4), not when step_index does.

    `max_steps` (test-only) bounds how many steps this call attempts,
    letting acceptance-gate tests exercise a short synthetic chain rather
    than all 16."""
    evidence_root.mkdir(parents=True, exist_ok=True)
    state = load_chain_state(evidence_root)
    if state["current_anchor"] is None:
        state["current_anchor"] = chain_spec["initial_anchor"]
        state["block_start_anchor"] = chain_spec["initial_anchor"]
    steps_to_run = N_PLANNED_STEPS if max_steps is None else min(max_steps, N_PLANNED_STEPS)
    for step_index in range(1, steps_to_run + 1):
        if any(row["step_index"] == step_index for row in state["completed_steps"]):
            continue
        anchor = state["current_anchor"]
        initial_gate_id = f"accumulation-step-{step_index:02d}-initial"
        initial_leg = leg_spec_for_step(chain_spec, step_index, "initial", anchor, initial_gate_id)
        initial_leg_path = evidence_root / f"step-{step_index:02d}-initial-leg-spec.json"
        if not initial_leg_path.is_file():
            write_new_json(initial_leg_path, initial_leg)
        existing = existing_manifest(evidence_root, initial_gate_id)
        initial_manifest_path = existing if existing is not None else formal_run(
            evidence_root, repo_root, initial_leg, initial_leg_path, "initial", run_arm_fn=run_arm_fn,
        )
        initial_manifest = load_json(initial_manifest_path)
        installed = False
        if initial_manifest["disposition"] == "SUCCESS":
            confirm_gate_id = f"accumulation-step-{step_index:02d}-confirm"
            confirm_leg = leg_spec_for_step(chain_spec, step_index, "confirmation", anchor, confirm_gate_id)
            confirm_leg_path = evidence_root / f"step-{step_index:02d}-confirm-leg-spec.json"
            if not confirm_leg_path.is_file():
                write_new_json(confirm_leg_path, confirm_leg)
            existing_confirm = existing_manifest(evidence_root, confirm_gate_id)
            confirm_manifest_path = existing_confirm if existing_confirm is not None else formal_run(
                evidence_root, repo_root, confirm_leg, confirm_leg_path, "confirmation",
                initial_manifest=initial_manifest_path, run_arm_fn=run_arm_fn,
            )
            confirm_manifest = load_json(confirm_manifest_path)
            installed = confirm_manifest["disposition"] == "SUCCESS"
        if installed:
            state["current_anchor"] = chain_spec["candidate_identities"][step_index - 1]
            state["accepted_step_count"] += 1
        state["completed_steps"].append({
            "step_index": step_index,
            "installed": installed,
            "initial_manifest": str(initial_manifest_path.resolve()),
            "initial_disposition": initial_manifest["disposition"],
        })
        write_chain_state(evidence_root, state)
        if state["accepted_step_count"] > 0 and state["accepted_step_count"] % K_META_BLOCK == 0:
            meta_index = state["accepted_step_count"] // K_META_BLOCK
            if meta_index <= 3 and not any(row["meta_index"] == meta_index for row in state["completed_meta_gates"]):
                meta_gate_id = f"accumulation-meta-{meta_index:02d}"
                meta_leg = leg_spec_for_meta(
                    chain_spec, meta_index, state["block_start_anchor"], state["current_anchor"], meta_gate_id,
                )
                meta_leg_path = evidence_root / f"meta-{meta_index:02d}-leg-spec.json"
                if not meta_leg_path.is_file():
                    write_new_json(meta_leg_path, meta_leg)
                existing_meta = existing_manifest(evidence_root, meta_gate_id)
                meta_manifest_path = existing_meta if existing_meta is not None else formal_run(
                    evidence_root, repo_root, meta_leg, meta_leg_path, "initial", run_arm_fn=run_arm_fn,
                )
                meta_manifest = load_json(meta_manifest_path)
                state["completed_meta_gates"].append({
                    "meta_index": meta_index,
                    "disposition": meta_manifest["disposition"],
                    "manifest": str(meta_manifest_path.resolve()),
                })
                state["block_start_anchor"] = state["current_anchor"]
                write_chain_state(evidence_root, state)
    return state


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--chain-spec", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("validate-spec")
    subparsers.add_parser("run")
    args = parser.parse_args()
    chain_spec = validate_chain_spec(args.chain_spec.resolve())
    if args.command == "validate-spec":
        print("chain spec OK")
        return 0
    # HOLD: real chain execution is authorized separately (the coordinator's
    # own go-ahead after this implementation checkpoint), not by this CLI
    # existing. Left wired for that authorized run, not invoked by any test
    # or by main() being merely present.
    run_chain(args.evidence_root, args.repo_root, chain_spec, args.chain_spec.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
