#!/usr/bin/env python3
"""Cycle-4 payoff panel runner: the 28-matchup round robin over the eight
identities bound in a cycle-4 population refresh manifest.

Ports `scaled_selfplay_population_v1/run_payoff_evaluation.py`'s game
mechanism verbatim rather than reinventing it: the SAME ignored Rust test,
`native_science_loop_v1::windows_science_loop_tests::ladder_head_to_head_eval_v1`,
driven through the SAME `H2H_*` environment-variable protocol (seat-swapped
pairs, `H2H_ENVIRONMENT_RANDOMIZATION_V2=1`, one create-new
`H2H_OUTCOME_JSON` file per matchup, schema
`mtg-kernel-head-to-head-terminal-stream/v1`). Two things differ from v1,
both because cycle-4's Store and manifest schema are newer than v1's:

  1. Slot->store resolution: v1 mapped five fixed program-v1 SEEDS to store
     roots and re-derived on-disk checkpoint/sidecar/state file paths itself,
     verifying their SHA-256 against the bindings document before trusting
     them. Cycle-4's eight slots are looked up directly by SLOT INDEX in a
     machine-local locator (`--slot-locator`; absolute paths only, never
     hashed into any artifact this script writes), and identity binding is
     verified POST-hoc instead of pre-hoc: `H2H_CANDIDATE_USE_STORE_RUN=1`
     hands the whole store root to the same validated V2/V4 Store boundary
     walk (`ValidatedNativeTrainingStoreRootV2::open_v2` +
     `load_native_training_boundary_v2`) the ignored test already performs
     internally -- which fails closed on corruption on its own -- and this
     script then cross-checks the outcome document's self-reported
     `run_sha256`/`generation`/`checkpoint_manifest_sha256`/
     `checkpoint_payload_sha256`/`model_parameter_sha256` against the
     manifest's declared identity for that slot (see `summarize_outcome`).
     v1's on-disk file layout (`checkpoint.json`/`sidecar.json`/
     `state.f32le` with a four-hash identity) does not describe the V2/V4
     Store cycle-4 targets, so porting v1's pre-hoc file-hash walk verbatim
     would not even be meaningful here; the post-hoc check achieves the same
     "never trust an unverified identity" guarantee against the SAME wire
     document v1 itself also cross-checked in `validate_outcome`.
  1b. Own-run generation translation: the manifest labels every slot in the
     contract's trainee-local numbering, but a cycle-4 arm is a NEW run
     identity seeded from the cycle-3 g896 checkpoint, so its own Store
     restarts at generation 0 and counts 0..=2048 for 896..=2944. Any slot
     whose `source_run_sha256` is the manifest's own `trainee_run_sha256`
     (current-1 always, historical-0 from refresh index 4) is therefore
     loaded at `source_generation - 896`, and the outcome document's
     self-reported generation is checked against that translated value. This
     mirrors the launcher's `store_generation_for_slot_v1`
     (`native_cycle4_arm_v1.rs`) exactly; the two must never drift. Slots
     naming other runs keep their labels verbatim, since those runs number
     their own Stores.

  2. No concurrency screen: v1 first measured whether N parallel replicas of
     one arm were bit-identical and fast enough before committing to
     parallel execution across the full matrix. Cycle-4's round-C contract
     does not ask for that apparatus, so this runner executes the 28
     matchups sequentially. The per-matchup mechanism, outcome schema, and
     validation are otherwise identical, so a concurrency screen could be
     layered on top later without changing anything below it.

Build command for `--executable` (mirrors every existing wrapper in this
family -- see `scripts/experiments/regularized_continuation_retest_v1/
common.ps1`'s `Get-ReleaseTestExecutable`, which this script deliberately
does NOT reimplement; a caller builds the executable once and passes its
path in):

    cargo test -p mtg-kernel --release \
        --features experimental-burn-net8-packed-cuda-v1 \
        --lib --no-run --message-format=json

Parse the JSON lines; the executable is the `executable` field of the LAST
line with `reason == "compiler-artifact"`, `target.name == "mtg_kernel"`,
and `"lib" in target.kind`.

Inputs: the current cycle-4 refresh manifest (its eight slots are the panel
roster; its own `refresh_index` picks this run's output filename, see
below; its own SHA-256 is recorded in the panel document as the content the
next manifest binds against), a slot-locator JSON, `G` games per matchup
(default 256 -- MUST equal `CYCLE4_PANEL_GAMES_PER_MATCHUP_V1` in
`native_population_refresh_manifest_cycle4_v1.rs` for the panel to be valid
input to the Rust builder; a different `G` is accepted ONLY under
`--dry-run`, for local smoke testing -- passing a non-canonical `G` outside
`--dry-run` is a hard usage error, since the Rust side's rank-sum bound is
FIXED at `7*256=1792` regardless of what `G` this runner used and the
production panel schema is only ever emitted for `G=256`), a base-seed
literal (the caller is responsible for using a fresh literal per refresh so
no pair environment seed is ever reused across the whole campaign; this
script only guarantees no reuse WITHIN one panel run), and an output
directory.

Outputs, all under `--output-dir` (resolved to an absolute path before any
child process is launched, since matchup subprocesses run with
`cwd=repo_root`, not `cwd=output_dir`):
  - `<matchup-label>/{outcome.json,stdout.log,stderr.log}` per matchup (28
    directories), each `outcome.json` the ignored test's own create-new
    terminal-stream artifact.
  - `refresh-NN.panel.json` (`NN` = the loaded manifest's own
    `refresh_index + 1`, zero-padded to two digits -- `panel_filename`):
    ONE canonical document (sorted keys, LF -- see `canonical_bytes`),
    schema `mtg-kernel-cycle4-payoff-panel/v1`. Its exact bytes are what
    refresh `NN`'s manifest binds by SHA-256
    (`build_cycle4_next_refresh_v1`'s `panel_bytes` argument); nothing in
    this script's own JSON encoding path may drift from the canonical form
    the Rust builder re-derives independently. This filename is the SAME
    fixed chain-directory naming scheme the Rust builder module documents
    (`cycle4_chain_panel_filename_v1` in
    `native_population_refresh_builder_cycle4_v1.rs`) -- both sides must
    never drift from it. Staged to a temporary name and committed (renamed
    into place) LAST, after `bt-rating-input.json`, so its presence at this
    path is the single signal that the whole run succeeded; any exception
    during staging or committing removes every temporary file so a failed
    run never leaves a consumable panel document.
  - `bt-rating-input.json`: schema `mtg-kernel-bt-rating-input/v1`, ready for
    `bt_rating_v1.py <this file> <result.json>`. Scoped to this one panel's
    28 pairs only; a later aggregator, not this script, is responsible for
    folding in cross-panel history across refreshes (the derived-metric
    module's own docstring anticipates that as a separate step). Committed
    BEFORE the panel document (it is downstream analysis input, not content
    any manifest binds by hash).

`--dry-run` computes and prints every matchup's exact command line, H2H_*
environment, and evaluation seed without touching a process or the
filesystem -- safe to run without the executable, store roots, or output
directory actually existing.
"""

from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import os
import shlex
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
PANEL_SCHEMA = "mtg-kernel-cycle4-payoff-panel/v1"
BT_INPUT_SCHEMA = "mtg-kernel-bt-rating-input/v1"
OUTCOME_SCHEMA = "mtg-kernel-head-to-head-terminal-stream/v1"

SLOT_COUNT = 8
DEFAULT_GAMES_PER_MATCHUP = 256
# Matches the Rust contract's own constant
# (`CYCLE4_PANEL_GAMES_PER_MATCHUP_V1`); see the module docstring's note on
# --games-per-matchup -- outside --dry-run this is a hard requirement, not a
# warning.
CANONICAL_GAMES_PER_MATCHUP = 256
MATCHUP_COUNT = 28
# Matches the Rust contract's own constant (`CYCLE4_REFRESH_MAX_INDEX_V1`):
# the highest refresh index the campaign ever chains to.
MAX_REFRESH_INDEX = 16
# Ported from v1's MATRIX_EVAL_SEED_STRIDE: generously larger than any
# plausible per-matchup pair count, so the per-matchup evaluation seeds
# cannot plausibly collide before the explicit global uniqueness check below
# even runs.
EVAL_SEED_STRIDE = 1_000_000

# Every H2H_* (and WIDE) knob the ignored test reads, cleared before each
# matchup so no ambient value from the calling shell leaks in silently.
H2H_ENVIRONMENT_KEYS = (
    "H2H_CANDIDATE_STORE_ROOT",
    "H2H_CANDIDATE_GEN",
    "H2H_CANDIDATE_USE_STORE_RUN",
    "H2H_CANDIDATE_BASE_SEED",
    "H2H_CANDIDATE_POOL_JSON",
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


# The contract's trainee-local start generation. An own-run slot's manifest
# label is this many generations above the arm Store's own numbering; see the
# module docstring and `store_generation_for_slot`.
TRAINEE_START_LOCAL_GENERATION = 896


class PanelRunnerError(ValueError):
    """Any fail-closed rejection: malformed input, an outcome that does not
    match its spec or the manifest's declared identity, or a reused pair
    seed. Never caught silently -- `main` reports it and exits non-zero."""


@dataclass(frozen=True)
class MatchupSpec:
    """One of the 28 unordered slot pairs. `lower_slot` is always the
    smaller index and plays the H2H "candidate" role; `higher_slot` plays
    "opponent" -- matching v1's own `itertools.combinations` convention
    exactly, so a reader already familiar with v1's panel output recognizes
    the same role split here."""

    matchup_index: int
    lower_slot: int
    higher_slot: int
    evaluation_seed: int
    pair_count: int
    game_count: int
    label: str


# ---------------------------------------------------------------------------
# Small canonical-JSON and hashing helpers (byte-identical to v1's).
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


def write_new_json(path: Path, value: dict) -> bytes:
    """Writes `value` as canonical JSON to a NEW file (never overwrites), so
    an interrupted or repeated run can never silently replace an earlier
    panel or BT-input document. Returns the exact bytes written."""
    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())
    return encoded


def staged_temp_path(final_path: Path) -> Path:
    """The create-new temporary name `write_new_json` stages `final_path`'s
    content to before the atomic commit (`commit_staged_file`) renames it
    into place. Computed as pure path arithmetic (no I/O) so a caller always
    knows this path for cleanup, even if staging itself never got far enough
    to create the file."""
    return final_path.with_name(f"{final_path.name}.tmp-{os.getpid()}")


def commit_staged_file(temp_path: Path, final_path: Path) -> None:
    """The atomic commit: renames a file staged by `write_new_json` (at
    `staged_temp_path(final_path)`) into its final name. Whichever staged
    file a caller commits LAST is the one whose presence at its final name
    is the signal that the whole batch of commits succeeded."""
    os.replace(temp_path, final_path)


def remove_stray(path: Path) -> None:
    """Best-effort cleanup of a staged-but-not-committed (or partially
    written) temporary file; a no-op if it was never created or was already
    committed away."""
    try:
        path.unlink()
    except FileNotFoundError:
        pass


# ---------------------------------------------------------------------------
# Manifest and slot-locator loading.
# ---------------------------------------------------------------------------


def panel_filename(refresh_index: int) -> str:
    """Fixed on-disk name for the payoff panel that will be bound into
    refresh `refresh_index`'s manifest, matching the Rust chain builder's
    `cycle4_chain_panel_filename_v1` exactly
    (`native_population_refresh_builder_cycle4_v1.rs`); both sides must
    never drift from this scheme."""
    return f"refresh-{refresh_index:02d}.panel.json"


def store_generation_for_slot(slot: dict, trainee_run_sha256: str) -> int:
    """The generation this slot is actually loaded at in ITS OWN Store.

    Slots bound to the arm's own run carry trainee-local labels that sit 896
    above that Store's numbering (the arm restarts at generation 0), so they
    translate; slots naming other runs are returned unchanged. A label below
    896 on an own-run slot names no Store generation at all and fails closed,
    matching the launcher's `store_generation_for_slot_v1`."""
    generation = slot["source_generation"]
    if not isinstance(generation, int) or isinstance(generation, bool) or generation < 0:
        raise PanelRunnerError(
            f"slot {slot.get('slot_index')!r} source_generation must be a "
            f"non-negative int: {generation!r}"
        )
    if slot["source_run_sha256"] != trainee_run_sha256:
        return generation
    if generation < TRAINEE_START_LOCAL_GENERATION:
        raise PanelRunnerError(
            f"slot {slot.get('slot_index')!r} names the arm's own run at trainee-local "
            f"generation {generation}, below the program start "
            f"{TRAINEE_START_LOCAL_GENERATION}"
        )
    return generation - TRAINEE_START_LOCAL_GENERATION


def load_manifest(path: Path) -> tuple[bytes, str, int, list[dict]]:
    """Reads the manifest's exact bytes, its own SHA-256, its own
    `refresh_index` (the panel this run produces evaluates THIS index's
    roster and is bound into refresh `refresh_index + 1`'s manifest, so the
    panel's own output filename is derived from this field -- see
    `panel_filename`), and its eight slot records ordered 0..7. Only a
    STRUCTURAL check runs here (schema tag, exactly eight slots, expected
    roles, required fields present); the manifest's full semantic contract
    -- roster identities, weight arithmetic, chain linkage against its
    predecessor -- was already the Rust builder's job when this file was
    constructed, and is not re-verified here.

    Each returned slot additionally carries a derived `store_generation`: the
    generation that slot is loaded at in its own Store, translated by the 896
    offset for slots bound to the arm's own run (`store_generation_for_slot`).
    It is a local convenience only and never reaches any emitted document."""
    raw = path.read_bytes()
    manifest_sha256 = sha256_bytes(raw)
    document = json.loads(raw.decode("utf-8-sig"))
    if document.get("schema") != MANIFEST_SCHEMA:
        raise PanelRunnerError(f"unexpected manifest schema: {document.get('schema')!r}")
    refresh_index = document.get("refresh_index")
    if (
        not isinstance(refresh_index, int)
        or isinstance(refresh_index, bool)
        or refresh_index < 0
    ):
        raise PanelRunnerError(
            f"manifest refresh_index must be a non-negative int: {refresh_index!r}"
        )
    slots = document.get("slots")
    if not isinstance(slots, list) or len(slots) != SLOT_COUNT:
        raise PanelRunnerError("manifest must carry exactly eight slots")
    ordered: list[dict | None] = [None] * SLOT_COUNT
    for entry in slots:
        if not isinstance(entry, dict) or not all(
            field in entry for field in REQUIRED_SLOT_FIELDS
        ):
            raise PanelRunnerError("malformed slot record")
        index = entry["slot_index"]
        if (
            not isinstance(index, int)
            or isinstance(index, bool)
            or not 0 <= index < SLOT_COUNT
            or ordered[index] is not None
        ):
            raise PanelRunnerError("slot indexes must be 0..7 with no duplicates")
        if entry["role"] != EXPECTED_ROLES[index]:
            raise PanelRunnerError(f"slot {index} role mismatch: {entry['role']!r}")
        ordered[index] = entry
    if any(slot is None for slot in ordered):
        raise PanelRunnerError("manifest is missing a slot")
    trainee_run_sha256 = document.get("trainee_run_sha256")
    if not isinstance(trainee_run_sha256, str):
        raise PanelRunnerError("manifest must carry a trainee_run_sha256 string")
    for slot in ordered:
        slot["store_generation"] = store_generation_for_slot(slot, trainee_run_sha256)
    return raw, manifest_sha256, refresh_index, ordered


def load_slot_locator(path: Path) -> dict[int, Path]:
    """Machine-local slot index -> store root mapping. Absolute paths only
    (the manifest's own "no absolute paths in hashed contracts" rule extends
    here: this file is never read by the Rust builder and its bytes never
    enter any hashed artifact)."""
    document = load_json(path)
    if document.get("schema") != SLOT_LOCATOR_SCHEMA:
        raise PanelRunnerError(
            f"unexpected slot-locator schema: {document.get('schema')!r}"
        )
    stores = document.get("stores")
    if not isinstance(stores, dict) or len(stores) != SLOT_COUNT:
        raise PanelRunnerError("slot locator must map exactly eight slot indexes")
    resolved: dict[int, Path] = {}
    for key, value in stores.items():
        try:
            index = int(key)
        except (TypeError, ValueError) as error:
            raise PanelRunnerError(f"slot locator key is not an integer: {key!r}") from error
        if not 0 <= index < SLOT_COUNT or index in resolved:
            raise PanelRunnerError(
                f"slot locator has an out-of-range or duplicate index: {key!r}"
            )
        if not isinstance(value, str) or not value:
            raise PanelRunnerError(
                f"slot locator value for slot {index} must be a non-empty string"
            )
        root = Path(value)
        if not root.is_absolute():
            raise PanelRunnerError(
                f"slot locator store root for slot {index} must be an absolute path: {value!r}"
            )
        resolved[index] = root
    if set(resolved) != set(range(SLOT_COUNT)):
        raise PanelRunnerError("slot locator must cover slots 0..7 exactly once")
    return resolved


# ---------------------------------------------------------------------------
# Matchup planning: pure, deterministic given (base_seed, games_per_matchup).
# ---------------------------------------------------------------------------


def build_matchup_specs(base_seed: int, games_per_matchup: int) -> list[MatchupSpec]:
    if games_per_matchup <= 0 or games_per_matchup % 2 != 0:
        raise PanelRunnerError("--games-per-matchup must be a positive even integer")
    pair_count = games_per_matchup // 2
    specs = [
        MatchupSpec(
            matchup_index=matchup_index,
            lower_slot=lower,
            higher_slot=higher,
            evaluation_seed=base_seed + matchup_index * EVAL_SEED_STRIDE,
            pair_count=pair_count,
            game_count=games_per_matchup,
            label=f"matchup-{lower}-{higher}",
        )
        for matchup_index, (lower, higher) in enumerate(
            itertools.combinations(range(SLOT_COUNT), 2)
        )
    ]
    assert len(specs) == MATCHUP_COUNT
    return specs


def matchup_environment(
    slots: list[dict],
    locator: dict[int, Path],
    spec: MatchupSpec,
    outcome_path: Path,
) -> dict[str, str]:
    candidate = slots[spec.lower_slot]
    opponent = slots[spec.higher_slot]
    return {
        "H2H_CANDIDATE_STORE_ROOT": str(locator[spec.lower_slot]),
        # STORE generations, translated by `store_generation_for_slot`: an
        # own-run slot's trainee-local label does not exist in the arm's own
        # Store, which restarted at generation 0.
        "H2H_CANDIDATE_GEN": str(candidate["store_generation"]),
        "H2H_CANDIDATE_USE_STORE_RUN": "1",
        "H2H_OPPONENT_STORE_ROOT": str(locator[spec.higher_slot]),
        "H2H_OPPONENT_GEN": str(opponent["store_generation"]),
        "H2H_PAIRS": str(spec.pair_count),
        "H2H_EVAL_SEED": str(spec.evaluation_seed),
        "H2H_ENVIRONMENT_RANDOMIZATION_V2": "1",
        "H2H_OUTCOME_JSON": str(outcome_path),
    }


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
    locator: dict[int, Path],
    executable: Path,
    output_dir: Path,
) -> list[str]:
    """Pure rendering of the exact command and environment each matchup
    would run, with no process or filesystem access -- deterministic given
    identical inputs, which is exactly what the dry-run-determinism tests
    exercise."""
    lines = []
    for spec in specs:
        outcome_path = output_dir / spec.label / "outcome.json"
        environment = matchup_environment(slots, locator, spec, outcome_path)
        command = matchup_command(executable)
        env_str = " ".join(
            f"{key}={shlex.quote(value)}" for key, value in environment.items()
        )
        cmd_str = " ".join(shlex.quote(part) for part in command)
        lines.append(
            f"[{spec.matchup_index:02d}] {spec.label} "
            f"seed={spec.evaluation_seed} pairs={spec.pair_count} "
            f"games={spec.game_count} :: {env_str} {cmd_str}"
        )
    return lines


# ---------------------------------------------------------------------------
# Outcome validation and tabulation -- pure given an already-parsed outcome
# document, so directly testable against synthetic outcomes (no games).
# ---------------------------------------------------------------------------


def summarize_outcome(outcome: dict, spec: MatchupSpec, candidate: dict, opponent: dict) -> dict:
    """Validates one matchup's outcome document against its spec and the
    manifest's declared candidate/opponent identity (the post-hoc identity
    check described in the module docstring), then tabulates W/D/L from the
    lower slot's (candidate's) perspective and collects this matchup's pair
    environment seeds. Raises `PanelRunnerError` on any mismatch -- fail
    closed, matching v1's `validate_outcome`."""
    if (
        outcome.get("schema") != OUTCOME_SCHEMA
        or outcome.get("evaluation_base_seed") != spec.evaluation_seed
        or outcome.get("pair_count") != spec.pair_count
        or outcome.get("episode_count") != spec.pair_count * 2
    ):
        raise PanelRunnerError(f"{spec.label}: outcome header mismatch")
    runtime = outcome.get("runtime")
    if not isinstance(runtime, dict) or (
        runtime.get("all_natural") is not True
        or runtime.get("environment_randomization_v2") is not True
    ):
        raise PanelRunnerError(f"{spec.label}: outcome runtime flags mismatch")
    for side, slot in (("candidate", candidate), ("opponent", opponent)):
        identity = outcome.get(side)
        if not isinstance(identity, dict) or (
            identity.get("run_sha256") != slot["source_run_sha256"]
            # The outcome reports the generation the Store actually loaded,
            # which for an own-run slot is the translated one.
            or identity.get("generation") != slot["store_generation"]
            or identity.get("checkpoint_manifest_sha256") != slot["checkpoint_manifest_sha256"]
            or identity.get("checkpoint_payload_sha256") != slot["checkpoint_payload_sha256"]
            or identity.get("model_parameter_sha256") != slot["model_parameter_sha256"]
        ):
            raise PanelRunnerError(
                f"{spec.label}: {side} identity does not match the manifest slot"
            )
    episodes = outcome.get("episodes")
    if not isinstance(episodes, list) or len(episodes) != spec.pair_count * 2:
        raise PanelRunnerError(f"{spec.label}: episode count mismatch")
    by_pair: dict[int, list[dict]] = defaultdict(list)
    counts = {-1: 0, 0: 0, 1: 0}
    for episode in episodes:
        rank = episode.get("terminal_order_rank")
        if rank not in counts:
            raise PanelRunnerError(f"{spec.label}: nonterminal rank entered the payoff panel")
        counts[rank] += 1
        by_pair[int(episode["pair_index"])].append(episode)
    pair_seeds: list[int] = []
    for pair_index in range(spec.pair_count):
        pair = by_pair.get(pair_index, [])
        if (
            len(pair) != 2
            or {row.get("learner_seat") for row in pair} != {"P0", "P1"}
            or len({row.get("environment_seed") for row in pair}) != 1
        ):
            raise PanelRunnerError(f"{spec.label}: seat-swap binding mismatch at pair {pair_index}")
        pair_seeds.append(int(pair[0]["environment_seed"]))
    if len(set(pair_seeds)) != len(pair_seeds):
        raise PanelRunnerError(f"{spec.label}: duplicate pair seeds within one matchup")
    overall = outcome.get("learner_outcomes", {}).get("overall", {})
    if (
        overall.get("wins") != counts[1]
        or overall.get("losses") != counts[-1]
        or overall.get("draws") != counts[0]
    ):
        raise PanelRunnerError(f"{spec.label}: W/L/D summary mismatch")
    return {
        "lower_wins": counts[1],
        "lower_draws": counts[0],
        "lower_losses": counts[-1],
        "pair_environment_seeds": pair_seeds,
    }


# ---------------------------------------------------------------------------
# Panel and BT-input document assembly -- pure given the per-matchup
# summaries, so directly testable without games.
# ---------------------------------------------------------------------------


def build_panel_document(
    manifest_sha256: str,
    base_seed: int,
    games_per_matchup: int,
    slots: list[dict],
    specs: list[MatchupSpec],
    summaries: list[dict],
    outcome_hashes: list[str],
) -> dict:
    """Assembles the one canonical panel document. `rank_sums` is exactly
    the shape `native_population_refresh_builder_cycle4_v1.rs`'s
    `ordered_panel_rank_sums_v1` parses: `{"slot_index": int, "u_i": int}`
    per slot, `u_i` the signed terminal-rank sum over that slot's seven
    matchups (win +1, draw 0, loss -1 per game, matching
    `mw_update_cycle4_v1`'s documented convention)."""
    matchup_rows = []
    contributions = [0] * SLOT_COUNT
    for spec, summary, outcome_sha256 in zip(specs, summaries, outcome_hashes, strict=True):
        lower_wins = summary["lower_wins"]
        lower_draws = summary["lower_draws"]
        lower_losses = summary["lower_losses"]
        # Zero-sum: the higher slot's record is exactly the lower slot's
        # record with wins and losses swapped. Stated explicitly (both
        # orderings) rather than left for a consumer to derive by sign flip.
        higher_wins = lower_losses
        higher_draws = lower_draws
        higher_losses = lower_wins
        contributions[spec.lower_slot] += lower_wins - lower_losses
        contributions[spec.higher_slot] += higher_wins - higher_losses
        matchup_rows.append(
            {
                "matchup_index": spec.matchup_index,
                "lower_slot_index": spec.lower_slot,
                "higher_slot_index": spec.higher_slot,
                "lower_role": slots[spec.lower_slot]["role"],
                "higher_role": slots[spec.higher_slot]["role"],
                "evaluation_seed": spec.evaluation_seed,
                "pair_count": spec.pair_count,
                "game_count": spec.game_count,
                "lower_wins": lower_wins,
                "lower_draws": lower_draws,
                "lower_losses": lower_losses,
                "higher_wins": higher_wins,
                "higher_draws": higher_draws,
                "higher_losses": higher_losses,
                "outcome_sha256": outcome_sha256,
            }
        )
    rank_sums = [
        {
            "slot_index": index,
            "role": slots[index]["role"],
            "u_i": contributions[index],
        }
        for index in range(SLOT_COUNT)
    ]
    return {
        "schema": PANEL_SCHEMA,
        "manifest_sha256": manifest_sha256,
        "base_seed": base_seed,
        "games_per_matchup": games_per_matchup,
        "matchups": matchup_rows,
        "rank_sums": rank_sums,
    }


def build_bt_input_document(slots: list[dict], panel: dict) -> dict:
    """Assembles the BT-rating input document straight from the panel's own
    matchup rows, so it can never disagree with `panel.json` about what
    happened. `reference_id` is anchor-0's `model_parameter_sha256` --
    anchor-0 is the one slot whose identity is frozen across every refresh
    (`promoted(2)@384`), which is exactly why it is the fixed reference the
    derived metric's docstring calls for. Model ids are every OTHER slot's
    `model_parameter_sha256` too: content-addressed, so a slot whose
    occupant changes next refresh (current-1, historical-0) is correctly
    treated as a different model, never conflated with its predecessor."""
    anchor_zero = next(slot for slot in slots if slot["role"] == "anchor-0")
    pairs = [
        {
            "a_id": slots[row["lower_slot_index"]]["model_parameter_sha256"],
            "b_id": slots[row["higher_slot_index"]]["model_parameter_sha256"],
            "a_wins": row["lower_wins"],
            "b_wins": row["higher_wins"],
            "draws": row["lower_draws"],
        }
        for row in panel["matchups"]
    ]
    return {
        "schema": BT_INPUT_SCHEMA,
        "reference_id": anchor_zero["model_parameter_sha256"],
        "pairs": pairs,
    }


# ---------------------------------------------------------------------------
# Execution.
# ---------------------------------------------------------------------------


def run_matchup(
    executable: Path,
    repo_root: Path,
    output_dir: Path,
    slots: list[dict],
    locator: dict[int, Path],
    spec: MatchupSpec,
) -> dict:
    arm_root = output_dir / spec.label
    arm_root.mkdir(parents=True)
    outcome_path = arm_root / "outcome.json"
    stdout_path = arm_root / "stdout.log"
    stderr_path = arm_root / "stderr.log"
    environment = os.environ.copy()
    for key in H2H_ENVIRONMENT_KEYS:
        environment.pop(key, None)
    environment.update(matchup_environment(slots, locator, spec, outcome_path))
    command = matchup_command(executable)
    started = time.perf_counter()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        completed = subprocess.run(
            command, cwd=repo_root, env=environment, stdout=stdout, stderr=stderr
        )
    wall_seconds = time.perf_counter() - started
    if completed.returncode != 0 or not outcome_path.is_file():
        raise PanelRunnerError(
            f"{spec.label} failed: exit_code={completed.returncode} "
            f"stderr={stderr_path} stdout={stdout_path}"
        )
    return {
        "outcome_path": outcome_path,
        "wall_seconds": wall_seconds,
        "outcome_sha256": sha256_file(outcome_path),
    }


def run(args: argparse.Namespace) -> Path | None:
    manifest_bytes, manifest_sha256, refresh_index, slots = load_manifest(args.manifest)
    del manifest_bytes  # only its hash (and refresh_index) is needed once loaded
    locator = load_slot_locator(args.slot_locator)
    next_refresh_index = refresh_index + 1
    if next_refresh_index > MAX_REFRESH_INDEX:
        raise PanelRunnerError(
            f"manifest refresh_index={refresh_index} is already at the campaign's "
            f"max ({MAX_REFRESH_INDEX}); there is no next boundary to panel"
        )
    specs = build_matchup_specs(args.base_seed, args.games_per_matchup)
    # Resolved once, up front: every matchup path derived from this (the
    # per-matchup outcome directories AND H2H_OUTCOME_JSON) must be absolute,
    # since `run_matchup` launches its subprocess with `cwd=repo_root`, not
    # `cwd=output_dir` -- a relative --output-dir would otherwise resolve
    # against the WRONG directory inside the child process.
    output_dir = args.output_dir.resolve()

    if args.dry_run:
        for line in render_dry_run_lines(specs, slots, locator, args.executable, output_dir):
            print(line)
        return None

    output_dir.mkdir(parents=True, exist_ok=True)
    summaries = []
    outcome_hashes = []
    all_seeds: set[int] = set()
    for spec in specs:
        result = run_matchup(
            args.executable.resolve(),
            args.repo_root.resolve(),
            output_dir,
            slots,
            locator,
            spec,
        )
        outcome = load_json(result["outcome_path"])
        summary = summarize_outcome(
            outcome, spec, slots[spec.lower_slot], slots[spec.higher_slot]
        )
        for seed in summary["pair_environment_seeds"]:
            if seed in all_seeds:
                raise PanelRunnerError(f"pair environment seed {seed} reused across matchups")
            all_seeds.add(seed)
        summaries.append(summary)
        outcome_hashes.append(result["outcome_sha256"])

    expected_pair_count = sum(spec.pair_count for spec in specs)
    if len(all_seeds) != expected_pair_count:
        raise PanelRunnerError(
            "payoff panel does not contain the expected number of fresh pair seeds"
        )

    panel = build_panel_document(
        manifest_sha256,
        args.base_seed,
        args.games_per_matchup,
        slots,
        specs,
        summaries,
        outcome_hashes,
    )
    bt_input = build_bt_input_document(slots, panel)

    panel_path = output_dir / panel_filename(next_refresh_index)
    bt_input_path = output_dir / "bt-rating-input.json"
    panel_temp = staged_temp_path(panel_path)
    bt_input_temp = staged_temp_path(bt_input_path)
    try:
        write_new_json(panel_temp, panel)
        write_new_json(bt_input_temp, bt_input)
        # bt-rating-input.json is downstream analysis input; panel_path is
        # the content the NEXT manifest binds by SHA-256, so it is committed
        # LAST -- its presence there is the single signal this whole run
        # succeeded.
        commit_staged_file(bt_input_temp, bt_input_path)
        commit_staged_file(panel_temp, panel_path)
    except BaseException:
        remove_stray(panel_temp)
        remove_stray(bt_input_temp)
        raise

    print(panel_path)
    print(bt_input_path)
    return panel_path


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--slot-locator", required=True, type=Path)
    parser.add_argument(
        "--games-per-matchup", type=int, default=DEFAULT_GAMES_PER_MATCHUP
    )
    parser.add_argument("--base-seed", required=True, type=int)
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
    if not args.dry_run and args.games_per_matchup != CANONICAL_GAMES_PER_MATCHUP:
        parser.error(
            f"--games-per-matchup={args.games_per_matchup} is not allowed outside "
            f"--dry-run: the production panel schema is only ever emitted for "
            f"G={CANONICAL_GAMES_PER_MATCHUP} (pass --dry-run for local smoke "
            "testing with a different game count)"
        )
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        run(args)
    except PanelRunnerError as error:
        print(f"run_payoff_panel_v1: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
