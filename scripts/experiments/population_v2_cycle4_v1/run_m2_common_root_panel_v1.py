#!/usr/bin/env python3
"""Cycle-4 M2 common-root panel runner.

M2 is the CP7-free ranking measurement of the ratified section-6 mechanical
amendment (`LEAD_CYCLE4_SECTION6_MECHANICAL_AMENDMENT_V2.md`, section B):

    "M2 is the pairwise common-root seat-swapped comparison. Unit: one root =
     one seed played with both seat-swapped legs. N = 1,024 common roots per
     pair. Opponents: the frozen cycle-4 genesis pool (the eight pinned
     identities), identical for every arm. Estimator: the fixed-N root-cluster
     paired winrate delta with a two-sided 95% CI from the root-level
     differences (normal approximation on the cluster mean, the same form as
     M1); the legacy integer and CS gates are reported as diagnostics only and
     gate nothing."

This runner plays FOUR endpoints -- the three arms' update-2048 checkpoints
and the frozen g896 start -- against that one frozen pool on ONE shared set
of 1,024 roots, and emits the per-root outcome table from which any pair of
endpoints can be compared as a paired delta.

## Why the roots really are common

The panel drives the same ignored Rust probe the payoff panel drives
(`native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1`)
through the same `H2H_*` protocol. That probe schedules episodes through
`native_trainer_episode_schedule_v1(evaluation_base_seed, episode_index)`,
whose environment seed is `derive_seed(TRAIN_ENV_NAMESPACE, [base_seed,
pair_index])` and whose learner seat alternates P0/P1 on consecutive episode
indexes. So one PAIR is exactly one root: one environment seed played with
both seat-swapped legs. The seed depends only on `H2H_EVAL_SEED` and the pair
index -- never on which model plays -- so handing every endpoint the SAME
`H2H_EVAL_SEED` for the same pool slot gives every endpoint literally the
same roots. That is asserted post hoc as well: the emitted document is only
written if every endpoint reported the identical `environment_seed` for every
root (`bind_roots`).

## Root allocation over the pool

Roots are apportioned across the eight pool slots by the genesis manifest's
own `weight_units` under the largest-remainder (Hamilton) rule, ties broken
toward the lower slot index (`allocate_roots`). The cycle-4 GENESIS manifest
pins every slot at `CYCLE4_GENESIS_SLOT_WEIGHT_UNITS_V1 = 125_000` of
`CYCLE4_WEIGHT_TOTAL_UNITS_V1 = 1_000_000`, so the genesis allocation is a
uniform 128 roots per slot; the general rule is implemented anyway so the
document states the arithmetic it actually used rather than a hidden literal.
A non-genesis manifest is refused: "the frozen cycle-4 genesis pool" is the
amendment's pool, and a later refresh's roster is a different pool.

Root ordering is `(slot_index, pair_index)` ascending, so `root_index` is a
stable name for a root across every endpoint and every rerun.

## Estimator (recomputed independently by the routing bin)

Per root `r` and endpoint `e`, the two legs give terminal ranks in
`{-1, 0, +1}` from the endpoint's perspective. Scores are the standard
`Y = I(win) + 0.5 * I(draw) = (rank + 1) / 2`:

    score_pooled(e, r) = (Y(p0 leg) + Y(p1 leg)) / 2
    score_p1(e, r)     = Y(p1 leg)                     (the P1 stratum)

For an ordered endpoint pair `(a, b)` the root-level difference is
`d_r = score(a, r) - score(b, r)`; the reported delta is `100 * mean(d)` in
percentage points, the standard error is `100 * sd(d) / sqrt(N)` with `sd`
the SAMPLE standard deviation (denominator `N - 1`), the two-sided 95% CI is
`delta +/- Z_TWO_SIDED_95 * se`, and the one-sided 95% lower bound is
`delta - Z_ONE_SIDED_95 * se`. Sums are plain sequential f64 sums in
`root_index` order and the sd is two-pass, so the arithmetic is bit-exactly
reproducible: `cycle4_routing_v1` recomputes every one of these numbers from
the `roots` table and requires bit equality with what this runner declared.

Every real number is emitted as the object `{"f64_bits": "<16 lower-hex>",
"text": "<decimal>"}` -- the shape `native_cycle4_m3_audit_v1::RealV1`
decodes -- because the canonical-JSON codec this document is read with
FORBIDS floating point outright
(`canonical_json_v1::CanonicalJsonErrorKindV1::FloatingPointForbidden`).
`f64_bits` is the IEEE-754 bit pattern and is AUTHORITATIVE; `text` is a
display rendering for human readers that nothing decides on (Python and Rust
spell exponents differently, so the Rust decoder reads only the bits).

## Diagnostics that gate nothing

`legacy_integer_net*` is the paired terminal-order net (an integer, the
legacy native-evaluator gate quantity) overall and per seat. The legacy
CONFIDENCE-SEQUENCE gate is NOT reproduced: its frozen reference
implementation (`eb_cs_reference_v1.py`) lives outside this repository, and
importing an out-of-tree module into a launcher-level artifact would put an
unhashed dependency inside a routing input. The document records that
explicitly, and the fixed-N normal-approximation one-sided lower bound
already carried in `pooled` is its fixed-N analogue. Both are diagnostics;
neither gates anything.

## Outputs

Under `--output-dir`:
  - `<endpoint>/slot-<n>/{outcome.json,stdout.log,stderr.log}` per matchup
    (4 endpoints x 8 slots = 32), each `outcome.json` the probe's own
    create-new terminal-stream artifact.
  - `m2-common-root-panel.json`: ONE canonical document, schema
    `mtg-kernel-cycle4-m2-common-root-panel/v1`, staged to a temporary name
    and committed last, so its presence is the single signal the whole run
    succeeded.

`--dry-run` prints every matchup's exact command line, `H2H_*` environment
and evaluation seed and touches neither a process nor the filesystem.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shlex
import struct
import subprocess
import sys
import time
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path

TEST_NAME = (
    "native_science_loop_v1::windows_science_loop_tests::"
    "ladder_head_to_head_eval_v1"
)

MANIFEST_SCHEMA = "mtg-kernel-population-refresh-manifest-cycle4/v1"
SLOT_LOCATOR_SCHEMA = "mtg-kernel-cycle4-slot-locator/v1"
ENDPOINT_LOCATOR_SCHEMA = "mtg-kernel-cycle4-m2-endpoint-locator/v1"
PANEL_SCHEMA = "mtg-kernel-cycle4-m2-common-root-panel/v1"
OUTCOME_SCHEMA = "mtg-kernel-head-to-head-terminal-stream/v1"

PANEL_FILENAME = "m2-common-root-panel.json"

SLOT_COUNT = 8
ROOT_COUNT = 1_024

# ---------------------------------------------------------------------------
# Launcher-owned formal seed literals. THIS IS THE ONLY PLACE THEY EXIST.
#
# The M2 band is disjoint from every other band the cycle-4 campaign uses:
# the three arms' TRAINING base seeds are 978_000 / 979_000 / 980_000, and the
# payoff-panel band starts at 4_100_000_000 and strides 32_000_000 per refresh
# through refresh 16 (highest reachable panel seed 4_612_000_000 + 27_000_000
# for the 28th matchup). The M2 band occupies
# [5_100_000_000, 5_100_000_000 + 7 * 1_000_000] and nothing else in the
# campaign reaches it.
# ---------------------------------------------------------------------------
M2_COMMON_ROOT_BASE_SEED_V1 = 5_100_000_000
M2_OPPONENT_SEED_STRIDE_V1 = 1_000_000

# Pinned endpoints. Generations are NOT operator inputs: the amendment pins
# "the carried arm's update-2048 checkpoint (endpoint pinned; no in-arm
# checkpoint selection)" and the frozen g896 start, so the runner supplies
# both from these literals and the locator carries only machine-local paths.
ARM_ENDPOINT_IDS = ("control-r", "static-rb", "treatment-rb")
BASELINE_ENDPOINT_ID = "g896"
ENDPOINT_IDS = (*ARM_ENDPOINT_IDS, BASELINE_ENDPOINT_ID)
# A cycle-4 arm's own Store restarts at generation 0 for trainee-local 896, so
# trainee-local 2944 (update 2048) is store generation 2048.
ARM_ENDPOINT_STORE_GENERATION = 2_048
# The frozen start is the cycle-3 focal Store's own generation 896.
BASELINE_ENDPOINT_STORE_GENERATION = 896
# The two rb arms train through the v4 trainer, so their own Store only reads
# back through their baseline chain directory; control-r and the cycle-3
# g896 Store are v3 and must not carry one.
BASELINE_V4_ENDPOINT_IDS = ("static-rb", "treatment-rb")

ARM_KINDS = ("control-r", "static-rb", "treatment-rb")
BASELINE_V4_ARM_KINDS = ("static-rb", "treatment-rb")
TRAINEE_START_LOCAL_GENERATION = 896

EXPECTED_ROLES = (
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
)

REQUIRED_SLOT_FIELDS = (
    "slot_index",
    "role",
    "occupant_class",
    "source_base_seed",
    "source_run_sha256",
    "source_generation",
    "checkpoint_manifest_sha256",
    "checkpoint_payload_sha256",
    "model_parameter_sha256",
    "train_state_sha256",
    "weight_units",
)

# Every H2H_* (and WIDE) knob the ignored probe reads, cleared before each
# matchup so no ambient value from the calling shell leaks in silently.
H2H_ENVIRONMENT_KEYS = (
    "H2H_CANDIDATE_STORE_ROOT",
    "H2H_CANDIDATE_GEN",
    "H2H_CANDIDATE_USE_STORE_RUN",
    "H2H_CANDIDATE_BASE_SEED",
    "H2H_CANDIDATE_POOL_JSON",
    "H2H_CANDIDATE_CHAIN_DIR",
    "H2H_OPPONENT_CHAIN_DIR",
    "H2H_UPDATES",
    "H2H_INIT_STORE",
    "H2H_INIT_GEN",
    "H2H_OPPONENT_STORE_ROOT",
    "H2H_OPPONENT_GEN",
    "H2H_PAIRS",
    "H2H_EVAL_SEED",
    "H2H_ENVIRONMENT_RANDOMIZATION_V2",
    "H2H_OUTCOME_JSON",
    "H2H_STARTING_PLAYER",
    "WIDE",
)

# Normal quantiles, as decimal literals that parse to exactly the doubles the
# Rust selector parses from the same text. `Z_TWO_SIDED_95` is the same
# literal `python/mtg_kernel_rl/evaluation_stats.py` already pins.
Z_TWO_SIDED_95 = 1.959963984540054
Z_ONE_SIDED_95 = 1.6448536269514722

# The legacy confidence-sequence gate is not reproduced here; see the module
# docstring. The reason is recorded in the document rather than left implicit.
LEGACY_CS_NOT_COMPUTED_REASON = (
    "the frozen EB confidence-sequence reference (eb_cs_reference_v1.py) is "
    "out of tree; the fixed-N normal-approximation one-sided lower bound in "
    "pooled is its fixed-N analogue. Diagnostic only; gates nothing."
)


class M2PanelError(ValueError):
    """Any fail-closed rejection: malformed input, an outcome that does not
    match its spec or the pool's declared identity, a root the endpoints do
    not agree on, or a reused environment seed."""


@dataclass(frozen=True)
class MatchupSpec:
    """One endpoint against one pool slot: `pair_count` roots, played as
    `2 * pair_count` games. `evaluation_seed` depends ONLY on the slot, so
    the same slot gives every endpoint the same roots."""

    endpoint_id: str
    slot_index: int
    evaluation_seed: int
    pair_count: int
    game_count: int
    first_root_index: int
    label: str


@dataclass(frozen=True)
class EndpointLocation:
    """One endpoint's machine-local location and its pinned generation."""

    endpoint_id: str
    store_root: Path
    store_generation: int
    baseline_chain_dir: Path | None


@dataclass(frozen=True)
class SlotLocation:
    store_root: Path
    baseline_chain_dir: Path | None


# ---------------------------------------------------------------------------
# Canonical JSON, hashing, and float encoding.
# ---------------------------------------------------------------------------


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def canonical_bytes(value: dict) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def f64_bits(value: float) -> str:
    """The IEEE-754 bit pattern of `value` as 16 lower-hex characters, the
    same encoding `native_training_store_update_group_v4`'s
    `residual_sum_f64_bits` uses. Canonical JSON forbids floating point, so
    this is how every real number crosses into the document."""
    if not math.isfinite(value):
        raise M2PanelError(f"refusing to encode a non-finite statistic: {value!r}")
    return f"{struct.unpack('>Q', struct.pack('>d', value))[0]:016x}"


def real(value: float) -> dict:
    """One real number, in the shape `native_cycle4_m3_audit_v1::RealV1`
    decodes: `f64_bits` is AUTHORITATIVE and `text` is a derived display
    rendering that nothing decides on (the two languages spell exponents
    differently, so the Rust decoder reads only the bits)."""
    return {"f64_bits": f64_bits(value), "text": repr(value)}


def write_new_json(path: Path, value: dict) -> bytes:
    """Writes `value` as canonical JSON to a NEW file (never overwrites), so
    an interrupted or repeated run can never silently replace an earlier
    panel. Returns the exact bytes written."""
    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())
    return encoded


def staged_temp_path(final_path: Path) -> Path:
    return final_path.with_name(f"{final_path.name}.tmp-{os.getpid()}")


def commit_staged_file(temp_path: Path, final_path: Path) -> None:
    """Commits a staged file at its final name with CREATE-NEW semantics.

    The panel is an input the routing record binds by SHA-256, so it is
    immutable in exactly the way the Rust side's documents are: a rerun that
    produced byte-identical content is a no-op success (the replay case, and
    the staged copy is discarded), and anything else already at the final name
    is an error. `os.replace` would have silently overwritten a published
    panel, re-keying a freeze under whoever already read it.

    The existence check below is a courtesy that gives the replay case its
    no-op and a differing panel a clear message; it is NOT what makes the
    commit safe, because a panel can appear between the check and the commit.
    Safety comes from `os.link`, which raises FileExistsError rather than
    replacing on both Windows and POSIX. Hard linking is the only publication
    primitive because it makes the complete staged bytes visible atomically.
    If hard links are unavailable, publication fails closed."""
    encoded = temp_path.read_bytes()
    if final_path.exists():
        if final_path.read_bytes() == encoded:
            remove_stray(temp_path)
            return
        raise M2PanelError(
            f"{final_path} already holds a DIFFERENT panel; the M2 panel is immutable "
            "because the routing record binds it by SHA-256, so publish to a fresh "
            "--output-dir rather than replacing one"
        )
    raced = M2PanelError(
        f"{final_path} was created by another writer during this run; the M2 panel is "
        "immutable and is never replaced"
    )
    try:
        os.link(temp_path, final_path)
    except FileExistsError:
        raise raced from None
    except (AttributeError, OSError) as error:
        raise M2PanelError(
            f"{final_path} cannot be published immutably: hard-link publication "
            f"is required and os.link is unavailable or failed ({error})"
        ) from error
    remove_stray(temp_path)


def remove_stray(path: Path) -> None:
    try:
        path.unlink()
    except FileNotFoundError:
        pass


# ---------------------------------------------------------------------------
# Inputs: the genesis manifest, the slot locator, the endpoint locator.
# ---------------------------------------------------------------------------


def store_generation_for_slot(slot: dict, trainee_run_sha256: str) -> int:
    """The generation this slot is loaded at in ITS OWN Store, translated by
    the 896 offset for slots bound to the arm's own run. Identical rule to
    `run_payoff_panel_v1.store_generation_for_slot` and the launcher's
    `store_generation_for_slot_v1`; the three must never drift."""
    generation = slot["source_generation"]
    if not isinstance(generation, int) or isinstance(generation, bool) or generation < 0:
        raise M2PanelError(
            f"slot {slot.get('slot_index')!r} source_generation must be a "
            f"non-negative int: {generation!r}"
        )
    if slot["source_run_sha256"] != trainee_run_sha256:
        return generation
    if generation < TRAINEE_START_LOCAL_GENERATION:
        raise M2PanelError(
            f"slot {slot.get('slot_index')!r} names the arm's own run at trainee-local "
            f"generation {generation}, below the program start "
            f"{TRAINEE_START_LOCAL_GENERATION}"
        )
    return generation - TRAINEE_START_LOCAL_GENERATION


def load_genesis_manifest(path: Path) -> tuple[str, str, list[dict]]:
    """Reads the manifest's SHA-256, its `trainee_run_sha256`, and its eight
    slots ordered 0..7, each carrying a derived `store_generation`.

    Only the GENESIS manifest is admissible: the amendment's pool is "the
    frozen cycle-4 genesis pool (the eight pinned identities)", and every
    later refresh's roster is a different pool with different weights. Only a
    structural check runs here; the manifest's semantic contract was the Rust
    builder's job when it was constructed."""
    raw = path.read_bytes()
    manifest_sha256 = sha256_bytes(raw)
    document = json.loads(raw.decode("utf-8-sig"))
    if document.get("schema") != MANIFEST_SCHEMA:
        raise M2PanelError(f"unexpected manifest schema: {document.get('schema')!r}")
    refresh_index = document.get("refresh_index")
    if refresh_index != 0:
        raise M2PanelError(
            f"M2 plays the frozen GENESIS pool; this manifest is refresh_index="
            f"{refresh_index!r}"
        )
    slots = document.get("slots")
    if not isinstance(slots, list) or len(slots) != SLOT_COUNT:
        raise M2PanelError("manifest must carry exactly eight slots")
    ordered: list[dict | None] = [None] * SLOT_COUNT
    for entry in slots:
        if not isinstance(entry, dict) or not all(
            field in entry for field in REQUIRED_SLOT_FIELDS
        ):
            raise M2PanelError("malformed slot record")
        index = entry["slot_index"]
        if (
            not isinstance(index, int)
            or isinstance(index, bool)
            or not 0 <= index < SLOT_COUNT
            or ordered[index] is not None
        ):
            raise M2PanelError("slot indexes must be 0..7 with no duplicates")
        if entry["role"] != EXPECTED_ROLES[index]:
            raise M2PanelError(f"slot {index} role mismatch: {entry['role']!r}")
        weight = entry["weight_units"]
        if not isinstance(weight, int) or isinstance(weight, bool) or weight <= 0:
            raise M2PanelError(f"slot {index} weight_units must be a positive int")
        ordered[index] = entry
    if any(slot is None for slot in ordered):
        raise M2PanelError("manifest is missing a slot")
    trainee_run_sha256 = document.get("trainee_run_sha256")
    if not isinstance(trainee_run_sha256, str):
        raise M2PanelError("manifest must carry a trainee_run_sha256 string")
    for slot in ordered:
        slot["store_generation"] = store_generation_for_slot(slot, trainee_run_sha256)
    return manifest_sha256, trainee_run_sha256, ordered


def _absolute_path_or_reject(value: object, what: str) -> Path:
    if not isinstance(value, str) or not value:
        raise M2PanelError(f"{what} must be a non-empty string")
    resolved = Path(value)
    if not resolved.is_absolute():
        raise M2PanelError(f"{what} must be an absolute path: {value!r}")
    return resolved


def parse_slot_location(index: int, value: object) -> SlotLocation:
    """One `stores` entry of the index-keyed slot locator, in either
    admissible form: a bare store-root string, or the object form
    `{"store_root", "baseline_chain_dir"}` carrying both."""
    if isinstance(value, str):
        return SlotLocation(
            _absolute_path_or_reject(value, f"slot locator store root for slot {index}"),
            None,
        )
    if not isinstance(value, dict):
        raise M2PanelError(
            f"slot locator value for slot {index} must be a string or an object: {value!r}"
        )
    unknown = set(value) - {"store_root", "baseline_chain_dir"}
    if unknown:
        raise M2PanelError(
            f"slot locator entry for slot {index} has unknown keys: {sorted(unknown)}"
        )
    missing = {"store_root", "baseline_chain_dir"} - set(value)
    if missing:
        raise M2PanelError(
            f"slot locator entry for slot {index} is missing {sorted(missing)}; "
            "the object form carries both"
        )
    return SlotLocation(
        _absolute_path_or_reject(
            value["store_root"], f"slot locator store root for slot {index}"
        ),
        _absolute_path_or_reject(
            value["baseline_chain_dir"],
            f"slot locator baseline_chain_dir for slot {index}",
        ),
    )


def load_slot_locator(path: Path) -> dict[int, SlotLocation]:
    document = load_json(path)
    if document.get("schema") != SLOT_LOCATOR_SCHEMA:
        raise M2PanelError(f"unexpected slot-locator schema: {document.get('schema')!r}")
    stores = document.get("stores")
    if not isinstance(stores, dict) or len(stores) != SLOT_COUNT:
        raise M2PanelError("slot locator must map exactly eight slot indexes")
    resolved: dict[int, SlotLocation] = {}
    for key, value in stores.items():
        try:
            index = int(key)
        except (TypeError, ValueError) as error:
            raise M2PanelError(f"slot locator key is not an integer: {key!r}") from error
        if not 0 <= index < SLOT_COUNT or index in resolved:
            raise M2PanelError(
                f"slot locator has an out-of-range or duplicate index: {key!r}"
            )
        resolved[index] = parse_slot_location(index, value)
    if set(resolved) != set(range(SLOT_COUNT)):
        raise M2PanelError("slot locator must cover slots 0..7 exactly once")
    return resolved


def validate_slot_chain_dirs(
    pool_arm: str,
    slots: list[dict],
    locator: dict[int, SlotLocation],
    trainee_run_sha256: str,
) -> None:
    """Every pool slot bound to a v4 arm's OWN run must carry a baseline
    chain directory, and no other slot may. Same rule as
    `run_payoff_panel_v1.validate_slot_chain_dirs`: a missing one sends the
    probe down the plain boundary walk (which refuses a v4 run outright) and
    a superfluous one is refused rather than silently ignored."""
    if pool_arm not in ARM_KINDS:
        raise M2PanelError(f"unknown arm kind: {pool_arm!r}")
    for index, slot in enumerate(slots):
        own_run = slot["source_run_sha256"] == trainee_run_sha256
        needs_chain_dir = own_run and pool_arm in BASELINE_V4_ARM_KINDS
        has_chain_dir = locator[index].baseline_chain_dir is not None
        if needs_chain_dir and not has_chain_dir:
            raise M2PanelError(
                f"pool slot {index} ({slot['role']}) is bound to the {pool_arm} arm's own "
                "run, so its locator entry must carry baseline_chain_dir"
            )
        if has_chain_dir and not needs_chain_dir:
            raise M2PanelError(
                f"pool slot {index} ({slot['role']}) carries baseline_chain_dir but is "
                f"not an own-run slot of a v4 arm ({pool_arm})"
            )


def endpoint_store_generation(endpoint_id: str) -> int:
    return (
        BASELINE_ENDPOINT_STORE_GENERATION
        if endpoint_id == BASELINE_ENDPOINT_ID
        else ARM_ENDPOINT_STORE_GENERATION
    )


def load_endpoint_locator(path: Path) -> dict[str, EndpointLocation]:
    """The machine-local endpoint table. Paths only: the pinned generations
    come from this module's literals, so no operator can point M2 at an
    unpinned in-arm checkpoint by editing a locator."""
    document = load_json(path)
    if document.get("schema") != ENDPOINT_LOCATOR_SCHEMA:
        raise M2PanelError(
            f"unexpected endpoint-locator schema: {document.get('schema')!r}"
        )
    entries = document.get("endpoints")
    if not isinstance(entries, dict) or set(entries) != set(ENDPOINT_IDS):
        raise M2PanelError(
            f"endpoint locator must map exactly {sorted(ENDPOINT_IDS)}"
        )
    resolved: dict[str, EndpointLocation] = {}
    for endpoint_id in ENDPOINT_IDS:
        value = entries[endpoint_id]
        if not isinstance(value, dict):
            raise M2PanelError(f"endpoint {endpoint_id!r} entry must be an object")
        unknown = set(value) - {"store_root", "baseline_chain_dir"}
        if unknown:
            raise M2PanelError(
                f"endpoint {endpoint_id!r} entry has unknown keys: {sorted(unknown)}"
            )
        if "store_root" not in value:
            raise M2PanelError(f"endpoint {endpoint_id!r} entry needs a store_root")
        chain_dir = value.get("baseline_chain_dir")
        needs_chain_dir = endpoint_id in BASELINE_V4_ENDPOINT_IDS
        if needs_chain_dir and chain_dir is None:
            raise M2PanelError(
                f"endpoint {endpoint_id!r} trains through the v4 trainer, so its entry "
                "must carry baseline_chain_dir"
            )
        if chain_dir is not None and not needs_chain_dir:
            raise M2PanelError(
                f"endpoint {endpoint_id!r} is not a v4 arm endpoint but carries "
                "baseline_chain_dir"
            )
        resolved[endpoint_id] = EndpointLocation(
            endpoint_id=endpoint_id,
            store_root=_absolute_path_or_reject(
                value["store_root"], f"endpoint {endpoint_id} store_root"
            ),
            store_generation=endpoint_store_generation(endpoint_id),
            baseline_chain_dir=(
                None
                if chain_dir is None
                else _absolute_path_or_reject(
                    chain_dir, f"endpoint {endpoint_id} baseline_chain_dir"
                )
            ),
        )
    return resolved


# ---------------------------------------------------------------------------
# Planning: pure and deterministic given the manifest weights.
# ---------------------------------------------------------------------------


def allocate_roots(weight_units: list[int], root_count: int) -> list[int]:
    """Largest-remainder (Hamilton) apportionment of `root_count` roots over
    the pool's weights, ties broken toward the LOWER slot index.

    Every slot must receive at least one root: a pool member that plays no
    game is not in the pool, and silently dropping one would change the
    opponent distribution the amendment pins."""
    if len(weight_units) != SLOT_COUNT:
        raise M2PanelError("root allocation needs exactly eight weights")
    if root_count <= 0:
        raise M2PanelError("root_count must be positive")
    total = sum(weight_units)
    if total <= 0:
        raise M2PanelError("pool weights must sum to a positive value")
    scaled = [root_count * weight for weight in weight_units]
    allocation = [value // total for value in scaled]
    remainders = [value % total for value in scaled]
    leftover = root_count - sum(allocation)
    order = sorted(range(SLOT_COUNT), key=lambda index: (-remainders[index], index))
    for index in order[:leftover]:
        allocation[index] += 1
    if sum(allocation) != root_count:
        raise M2PanelError("root allocation did not sum to the root count")
    if any(value <= 0 for value in allocation):
        raise M2PanelError(
            f"root allocation starves a pool slot: {allocation}; every pinned "
            "identity must receive at least one root"
        )
    return allocation


def build_matchup_specs(allocation: list[int]) -> list[MatchupSpec]:
    """The 32 matchups (4 endpoints x 8 slots), endpoint-major then slot.

    `evaluation_seed` is a function of the SLOT alone, so all four endpoints
    receive byte-identical root schedules; `first_root_index` walks the pool
    in slot order so `root_index` is `(slot, pair)` lexicographic."""
    offsets = []
    running = 0
    for count in allocation:
        offsets.append(running)
        running += count
    specs = []
    for endpoint_id in ENDPOINT_IDS:
        for slot_index in range(SLOT_COUNT):
            pair_count = allocation[slot_index]
            specs.append(
                MatchupSpec(
                    endpoint_id=endpoint_id,
                    slot_index=slot_index,
                    evaluation_seed=(
                        M2_COMMON_ROOT_BASE_SEED_V1
                        + slot_index * M2_OPPONENT_SEED_STRIDE_V1
                    ),
                    pair_count=pair_count,
                    game_count=pair_count * 2,
                    first_root_index=offsets[slot_index],
                    label=f"{endpoint_id}/slot-{slot_index}",
                )
            )
    return specs


def matchup_environment(
    slots: list[dict],
    slot_locator: dict[int, SlotLocation],
    endpoints: dict[str, EndpointLocation],
    spec: MatchupSpec,
    outcome_path: Path,
) -> dict[str, str]:
    endpoint = endpoints[spec.endpoint_id]
    opponent = slots[spec.slot_index]
    opponent_location = slot_locator[spec.slot_index]
    environment = {
        "H2H_CANDIDATE_STORE_ROOT": str(endpoint.store_root),
        "H2H_CANDIDATE_GEN": str(endpoint.store_generation),
        "H2H_CANDIDATE_USE_STORE_RUN": "1",
        "H2H_OPPONENT_STORE_ROOT": str(opponent_location.store_root),
        "H2H_OPPONENT_GEN": str(opponent["store_generation"]),
        "H2H_PAIRS": str(spec.pair_count),
        "H2H_EVAL_SEED": str(spec.evaluation_seed),
        "H2H_ENVIRONMENT_RANDOMIZATION_V2": "1",
        "H2H_OUTCOME_JSON": str(outcome_path),
    }
    if endpoint.baseline_chain_dir is not None:
        environment["H2H_CANDIDATE_CHAIN_DIR"] = str(endpoint.baseline_chain_dir)
    if opponent_location.baseline_chain_dir is not None:
        environment["H2H_OPPONENT_CHAIN_DIR"] = str(opponent_location.baseline_chain_dir)
    return environment


def matchup_command(executable: Path) -> list[str]:
    return [
        str(executable),
        TEST_NAME,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]


def render_dry_run_lines(
    specs: list[MatchupSpec],
    slots: list[dict],
    slot_locator: dict[int, SlotLocation],
    endpoints: dict[str, EndpointLocation],
    executable: Path,
    output_dir: Path,
) -> list[str]:
    lines = []
    for spec in specs:
        outcome_path = matchup_outcome_path(output_dir, spec)
        environment = matchup_environment(
            slots, slot_locator, endpoints, spec, outcome_path
        )
        env_str = " ".join(
            f"{key}={shlex.quote(value)}" for key, value in environment.items()
        )
        cmd_str = " ".join(shlex.quote(part) for part in matchup_command(executable))
        lines.append(
            f"{spec.label} seed={spec.evaluation_seed} pairs={spec.pair_count} "
            f"games={spec.game_count} roots={spec.first_root_index}.."
            f"{spec.first_root_index + spec.pair_count - 1} :: {env_str} {cmd_str}"
        )
    return lines


def matchup_outcome_path(output_dir: Path, spec: MatchupSpec) -> Path:
    return output_dir / spec.endpoint_id / f"slot-{spec.slot_index}" / "outcome.json"


# ---------------------------------------------------------------------------
# Outcome validation -- pure given an already-parsed outcome document.
# ---------------------------------------------------------------------------


def summarize_outcome(outcome: dict, spec: MatchupSpec, opponent: dict) -> dict:
    """Validates one matchup's outcome against its spec and the pool slot's
    declared identity, then returns this matchup's per-root legs plus the
    endpoint identity the probe reported for the candidate side.

    The candidate identity is checked for internal consistency across an
    endpoint's eight matchups by `bind_roots`; it is authoritative here
    because the probe reports what its own validated Store walk actually
    loaded, which is the same post-hoc guarantee `run_payoff_panel_v1` relies
    on."""
    if (
        outcome.get("schema") != OUTCOME_SCHEMA
        or outcome.get("evaluation_base_seed") != spec.evaluation_seed
        or outcome.get("pair_count") != spec.pair_count
        or outcome.get("episode_count") != spec.pair_count * 2
    ):
        raise M2PanelError(f"{spec.label}: outcome header mismatch")
    runtime = outcome.get("runtime")
    if not isinstance(runtime, dict) or (
        runtime.get("all_natural") is not True
        or runtime.get("environment_randomization_v2") is not True
    ):
        raise M2PanelError(f"{spec.label}: outcome runtime flags mismatch")
    opponent_identity = outcome.get("opponent")
    if not isinstance(opponent_identity, dict) or (
        opponent_identity.get("run_sha256") != opponent["source_run_sha256"]
        or opponent_identity.get("generation") != opponent["store_generation"]
        or opponent_identity.get("checkpoint_manifest_sha256")
        != opponent["checkpoint_manifest_sha256"]
        or opponent_identity.get("checkpoint_payload_sha256")
        != opponent["checkpoint_payload_sha256"]
        or opponent_identity.get("model_parameter_sha256")
        != opponent["model_parameter_sha256"]
    ):
        raise M2PanelError(
            f"{spec.label}: opponent identity does not match the genesis manifest slot"
        )
    candidate_identity = outcome.get("candidate")
    if not isinstance(candidate_identity, dict) or not all(
        isinstance(candidate_identity.get(field), str)
        for field in (
            "run_sha256",
            "identity_bundle_sha256",
            "checkpoint_manifest_sha256",
            "checkpoint_payload_sha256",
            "model_parameter_sha256",
        )
    ):
        raise M2PanelError(f"{spec.label}: outcome carries no candidate identity")
    if candidate_identity.get("generation") != endpoint_store_generation(spec.endpoint_id):
        raise M2PanelError(
            f"{spec.label}: candidate loaded generation "
            f"{candidate_identity.get('generation')!r}, not the pinned "
            f"{endpoint_store_generation(spec.endpoint_id)}"
        )
    episodes = outcome.get("episodes")
    if not isinstance(episodes, list) or len(episodes) != spec.pair_count * 2:
        raise M2PanelError(f"{spec.label}: episode count mismatch")
    by_pair: dict[int, list[dict]] = defaultdict(list)
    for episode in episodes:
        rank = episode.get("terminal_order_rank")
        if rank not in (-1, 0, 1):
            raise M2PanelError(f"{spec.label}: nonterminal rank entered the M2 panel")
        by_pair[int(episode["pair_index"])].append(episode)
    legs = []
    for pair_index in range(spec.pair_count):
        pair = by_pair.get(pair_index, [])
        seats = {row.get("learner_seat") for row in pair}
        seeds = {int(row["environment_seed"]) for row in pair}
        if len(pair) != 2 or seats != {"P0", "P1"} or len(seeds) != 1:
            raise M2PanelError(
                f"{spec.label}: seat-swap binding mismatch at pair {pair_index}"
            )
        by_seat = {row["learner_seat"]: int(row["terminal_order_rank"]) for row in pair}
        legs.append(
            {
                "pair_index": pair_index,
                "environment_seed": seeds.pop(),
                "p0": by_seat["P0"],
                "p1": by_seat["P1"],
            }
        )
    return {"legs": legs, "candidate_identity": candidate_identity}


def bind_roots(
    specs: list[MatchupSpec],
    summaries: dict[tuple[str, int], dict],
    allocation: list[int],
) -> list[dict]:
    """Folds the 32 per-matchup summaries into the ONE root table.

    Fails closed unless every endpoint reported the SAME environment seed for
    the same root (the common-root proof) and every one of the `ROOT_COUNT`
    seeds is distinct within the panel (no seed reuse across pool slots)."""
    roots: list[dict | None] = [None] * sum(allocation)
    for spec in specs:
        summary = summaries[(spec.endpoint_id, spec.slot_index)]
        for leg in summary["legs"]:
            root_index = spec.first_root_index + leg["pair_index"]
            entry = roots[root_index]
            if entry is None:
                entry = {
                    "root_index": root_index,
                    "slot_index": spec.slot_index,
                    "pair_index": leg["pair_index"],
                    "environment_seed": leg["environment_seed"],
                    "legs": {},
                }
                roots[root_index] = entry
            if entry["environment_seed"] != leg["environment_seed"]:
                raise M2PanelError(
                    f"root {root_index} is not common: endpoint {spec.endpoint_id!r} "
                    f"played environment seed {leg['environment_seed']} where an "
                    f"earlier endpoint played {entry['environment_seed']}"
                )
            if spec.endpoint_id in entry["legs"]:
                raise M2PanelError(f"root {root_index} bound twice for {spec.endpoint_id!r}")
            entry["legs"][spec.endpoint_id] = {"p0": leg["p0"], "p1": leg["p1"]}
    bound = []
    for root_index, entry in enumerate(roots):
        if entry is None or set(entry["legs"]) != set(ENDPOINT_IDS):
            raise M2PanelError(f"root {root_index} was not played by every endpoint")
        bound.append(entry)
    seeds = {entry["environment_seed"] for entry in bound}
    if len(seeds) != len(bound):
        raise M2PanelError(
            "environment seeds are reused across roots; the M2 panel requires "
            f"{len(bound)} distinct roots"
        )
    return bound


# ---------------------------------------------------------------------------
# The estimator. Bit-exactly reproduced by `native_cycle4_routing_v1`.
# ---------------------------------------------------------------------------


def leg_score(rank: int) -> float:
    """`Y = I(win) + 0.5 * I(draw) = (rank + 1) / 2` for rank in {-1, 0, 1}."""
    if rank not in (-1, 0, 1):
        raise M2PanelError(f"terminal rank out of range: {rank!r}")
    return (rank + 1) / 2


def root_scores(roots: list[dict], endpoint_id: str) -> tuple[list[float], list[float], list[float]]:
    """Pooled, P0-stratum and P1-stratum per-root scores in `root_index`
    order."""
    pooled, p0, p1 = [], [], []
    for entry in roots:
        legs = entry["legs"][endpoint_id]
        score_p0 = leg_score(legs["p0"])
        score_p1 = leg_score(legs["p1"])
        pooled.append((score_p0 + score_p1) / 2)
        p0.append(score_p0)
        p1.append(score_p1)
    return pooled, p0, p1


def paired_statistics(differences: list[float]) -> dict:
    """The fixed-N root-cluster paired statistics in percentage points.

    Plain sequential f64 sums in root order and a two-pass sample standard
    deviation, so `native_cycle4_routing_v1` reproduces every bit."""
    count = len(differences)
    if count < 2:
        raise M2PanelError("a paired comparison needs at least two roots")
    total = 0.0
    for value in differences:
        total += value
    mean = total / count
    squares = 0.0
    for value in differences:
        deviation = value - mean
        squares += deviation * deviation
    variance = squares / (count - 1)
    standard_deviation = math.sqrt(variance)
    standard_error = standard_deviation / math.sqrt(count)
    delta_pp = 100.0 * mean
    standard_error_pp = 100.0 * standard_error
    return {
        "root_count": count,
        "delta_pp": real(delta_pp),
        "standard_deviation_pp": real(100.0 * standard_deviation),
        "standard_error_pp": real(standard_error_pp),
        "ci_low_pp": real(delta_pp - Z_TWO_SIDED_95 * standard_error_pp),
        "ci_high_pp": real(delta_pp + Z_TWO_SIDED_95 * standard_error_pp),
        "one_sided_lower_bound_pp": real(
            delta_pp - Z_ONE_SIDED_95 * standard_error_pp
        ),
    }


def legacy_integer_net(roots: list[dict], endpoint_a: str, endpoint_b: str) -> dict:
    """The legacy native-evaluator paired terminal-order net, an integer:
    the summed rank difference over both legs of every root, overall and per
    seat. Diagnostic only; gates nothing (amendment section B)."""
    net_p0 = 0
    net_p1 = 0
    for entry in roots:
        legs_a = entry["legs"][endpoint_a]
        legs_b = entry["legs"][endpoint_b]
        net_p0 += legs_a["p0"] - legs_b["p0"]
        net_p1 += legs_a["p1"] - legs_b["p1"]
    return {
        "legacy_integer_net": net_p0 + net_p1,
        "legacy_integer_net_p0": net_p0,
        "legacy_integer_net_p1": net_p1,
        "gates_nothing": True,
        "confidence_sequence_computed": False,
        "confidence_sequence_reason": LEGACY_CS_NOT_COMPUTED_REASON,
    }


def build_comparisons(roots: list[dict]) -> list[dict]:
    """Every unordered endpoint pair, oriented `(a, b)` with `a` earlier in
    `ENDPOINT_IDS`; the delta is `score(a) - score(b)`. The three arm-vs-arm
    rows feed the amendment's separability and Copeland ranking; the three
    arm-vs-g896 rows feed its carry rule."""
    scores = {
        endpoint_id: root_scores(roots, endpoint_id) for endpoint_id in ENDPOINT_IDS
    }
    comparisons = []
    for a_index, endpoint_a in enumerate(ENDPOINT_IDS):
        for endpoint_b in ENDPOINT_IDS[a_index + 1 :]:
            pooled_a, p0_a, p1_a = scores[endpoint_a]
            pooled_b, p0_b, p1_b = scores[endpoint_b]
            comparisons.append(
                {
                    "endpoint_a": endpoint_a,
                    "endpoint_b": endpoint_b,
                    "pooled": paired_statistics(
                        [a - b for a, b in zip(pooled_a, pooled_b, strict=True)]
                    ),
                    "p0_stratum": paired_statistics(
                        [a - b for a, b in zip(p0_a, p0_b, strict=True)]
                    ),
                    "p1_stratum": paired_statistics(
                        [a - b for a, b in zip(p1_a, p1_b, strict=True)]
                    ),
                    "diagnostics": legacy_integer_net(roots, endpoint_a, endpoint_b),
                }
            )
    return comparisons


# ---------------------------------------------------------------------------
# Document assembly.
# ---------------------------------------------------------------------------


def build_panel_document(
    genesis_manifest_sha256: str,
    pool_arm: str,
    slots: list[dict],
    allocation: list[int],
    specs: list[MatchupSpec],
    summaries: dict[tuple[str, int], dict],
    outcome_hashes: dict[tuple[str, int], str],
    roots: list[dict],
) -> dict:
    endpoint_rows = []
    for endpoint_id in ENDPOINT_IDS:
        identities = [
            summaries[(endpoint_id, slot_index)]["candidate_identity"]
            for slot_index in range(SLOT_COUNT)
        ]
        first = identities[0]
        for other in identities[1:]:
            if other != first:
                raise M2PanelError(
                    f"endpoint {endpoint_id!r} reported two different checkpoint "
                    "identities across its eight matchups"
                )
        if first.get("generation") != endpoint_store_generation(endpoint_id):
            raise M2PanelError(
                f"endpoint {endpoint_id!r} loaded generation "
                f"{first.get('generation')!r}, not the pinned "
                f"{endpoint_store_generation(endpoint_id)}"
            )
        endpoint_rows.append(
            {
                "endpoint_id": endpoint_id,
                "store_generation": endpoint_store_generation(endpoint_id),
                "run_sha256": first["run_sha256"],
                "identity_bundle_sha256": first["identity_bundle_sha256"],
                "checkpoint_manifest_sha256": first["checkpoint_manifest_sha256"],
                "checkpoint_payload_sha256": first["checkpoint_payload_sha256"],
                "model_parameter_sha256": first["model_parameter_sha256"],
            }
        )
    manifests = [row["checkpoint_manifest_sha256"] for row in endpoint_rows]
    if len(set(manifests)) != len(manifests):
        raise M2PanelError(
            "two endpoints resolved to the same checkpoint manifest; the panel would "
            "compare a checkpoint against itself"
        )
    return {
        "schema": PANEL_SCHEMA,
        "genesis_manifest_sha256": genesis_manifest_sha256,
        "pool_arm": pool_arm,
        "root_count": len(roots),
        "base_seed": M2_COMMON_ROOT_BASE_SEED_V1,
        "opponent_seed_stride": M2_OPPONENT_SEED_STRIDE_V1,
        "pool": [
            {
                "slot_index": index,
                "role": slots[index]["role"],
                "weight_units": slots[index]["weight_units"],
                "root_allocation": allocation[index],
                "store_generation": slots[index]["store_generation"],
                "source_run_sha256": slots[index]["source_run_sha256"],
                "checkpoint_manifest_sha256": slots[index]["checkpoint_manifest_sha256"],
                "checkpoint_payload_sha256": slots[index]["checkpoint_payload_sha256"],
                "model_parameter_sha256": slots[index]["model_parameter_sha256"],
            }
            for index in range(SLOT_COUNT)
        ],
        "endpoints": endpoint_rows,
        "matchups": [
            {
                "endpoint_id": spec.endpoint_id,
                "slot_index": spec.slot_index,
                "evaluation_seed": spec.evaluation_seed,
                "pair_count": spec.pair_count,
                "game_count": spec.game_count,
                "first_root_index": spec.first_root_index,
                "outcome_sha256": outcome_hashes[(spec.endpoint_id, spec.slot_index)],
            }
            for spec in specs
        ],
        "roots": roots,
        "comparisons": build_comparisons(roots),
    }


# ---------------------------------------------------------------------------
# Execution.
# ---------------------------------------------------------------------------


def run_matchup(
    executable: Path,
    repo_root: Path,
    output_dir: Path,
    slots: list[dict],
    slot_locator: dict[int, SlotLocation],
    endpoints: dict[str, EndpointLocation],
    spec: MatchupSpec,
) -> dict:
    matchup_root = output_dir / spec.endpoint_id / f"slot-{spec.slot_index}"
    matchup_root.mkdir(parents=True)
    outcome_path = matchup_root / "outcome.json"
    stdout_path = matchup_root / "stdout.log"
    stderr_path = matchup_root / "stderr.log"
    environment = os.environ.copy()
    for key in H2H_ENVIRONMENT_KEYS:
        environment.pop(key, None)
    environment.update(
        matchup_environment(slots, slot_locator, endpoints, spec, outcome_path)
    )
    started = time.perf_counter()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        completed = subprocess.run(
            matchup_command(executable),
            cwd=repo_root,
            env=environment,
            stdout=stdout,
            stderr=stderr,
        )
    wall_seconds = time.perf_counter() - started
    if completed.returncode != 0 or not outcome_path.is_file():
        raise M2PanelError(
            f"{spec.label} failed: exit_code={completed.returncode} "
            f"stderr={stderr_path} stdout={stdout_path}"
        )
    return {
        "outcome_path": outcome_path,
        "wall_seconds": wall_seconds,
        "outcome_sha256": sha256_file(outcome_path),
    }


def run(args: argparse.Namespace) -> Path | None:
    manifest_sha256, trainee_run_sha256, slots = load_genesis_manifest(args.genesis_manifest)
    slot_locator = load_slot_locator(args.slot_locator)
    validate_slot_chain_dirs(args.pool_arm, slots, slot_locator, trainee_run_sha256)
    endpoints = load_endpoint_locator(args.endpoint_locator)
    allocation = allocate_roots(
        [slot["weight_units"] for slot in slots], args.root_count
    )
    specs = build_matchup_specs(allocation)
    output_dir = args.output_dir.resolve()

    if args.dry_run:
        for line in render_dry_run_lines(
            specs, slots, slot_locator, endpoints, args.executable, output_dir
        ):
            print(line)
        return None

    output_dir.mkdir(parents=True, exist_ok=True)
    summaries: dict[tuple[str, int], dict] = {}
    outcome_hashes: dict[tuple[str, int], str] = {}
    for spec in specs:
        result = run_matchup(
            args.executable.resolve(),
            args.repo_root.resolve(),
            output_dir,
            slots,
            slot_locator,
            endpoints,
            spec,
        )
        outcome = load_json(result["outcome_path"])
        summaries[(spec.endpoint_id, spec.slot_index)] = summarize_outcome(
            outcome, spec, slots[spec.slot_index]
        )
        outcome_hashes[(spec.endpoint_id, spec.slot_index)] = result["outcome_sha256"]

    roots = bind_roots(specs, summaries, allocation)
    panel = build_panel_document(
        manifest_sha256,
        args.pool_arm,
        slots,
        allocation,
        specs,
        summaries,
        outcome_hashes,
        roots,
    )

    panel_path = output_dir / PANEL_FILENAME
    panel_temp = staged_temp_path(panel_path)
    try:
        write_new_json(panel_temp, panel)
        commit_staged_file(panel_temp, panel_path)
    except BaseException:
        remove_stray(panel_temp)
        raise

    print(panel_path)
    return panel_path


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--genesis-manifest", required=True, type=Path)
    parser.add_argument("--slot-locator", required=True, type=Path)
    parser.add_argument("--endpoint-locator", required=True, type=Path)
    parser.add_argument(
        "--pool-arm",
        required=True,
        choices=list(ARM_KINDS),
        help="which arm's genesis manifest and own-run slot the pool is read "
        "from; decides which pool slots must carry a baseline chain directory",
    )
    parser.add_argument("--root-count", type=int, default=ROOT_COUNT)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="print every matchup's exact command/environment/seed and exit "
        "without touching a process or the filesystem",
    )
    args = parser.parse_args(argv)
    if not args.dry_run and args.root_count != ROOT_COUNT:
        parser.error(
            f"--root-count={args.root_count} is not allowed outside --dry-run: the "
            f"ratified amendment pins N = {ROOT_COUNT} common roots per pair (pass "
            "--dry-run for local smoke testing with a different root count)"
        )
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        run(args)
    except M2PanelError as error:
        print(f"run_m2_common_root_panel_v1: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
