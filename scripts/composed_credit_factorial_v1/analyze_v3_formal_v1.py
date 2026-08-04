"""Independent analyzer for the frozen Net8 GAE V3 formal gate.

The evaluator owns the raw outcome records.  This module deliberately does
not trust evaluator-computed scores or decisions: it validates the retained
records, recomputes leg and cluster scores, and delegates the confidence
sequence and boundary rule to the exact V3 reference implementation in the
shared ``collab`` directory.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from types import ModuleType
from typing import Any, Mapping, Sequence


ANALYSIS_SCHEMA = "mtg-kernel-gae-v3-formal-analysis/v1"
RAW_REPORT_SCHEMA = "mtg-kernel-gae-v3-formal-raw/v1"
RAW_CHUNK_SCHEMA = "mtg-kernel-gae-v3-formal-chunk/v1"
FROZEN_BASE_SEED = 970001
FROZEN_ALPHA = 0.00875
FROZEN_C = 0.5
FROZEN_GATE_CLASS = "LARGE-EFFECT"
FROZEN_DELTA_WORTHWHILE = 0.01
FROZEN_DELTA_PROMOTE = 0.01
FROZEN_MAX_CLUSTERS = 16384
FROZEN_CONDITIONAL_MEAN_STABILITY = "IID-MIXTURE"
FROZEN_REFERENCE_SHA256 = "ffae17bdc020578a34d7cc420e138951fcb587531cf5191c978384a4bd4b73ef"
FROZEN_MODES = ("initial", "confirmation")
_SCORE_TOKENS = (-1.0, -0.5, 0.0, 0.5, 1.0)
_LEG_SCORE_TOKENS = (-1.0, 0.0, 1.0)
_TERMINAL_RETURN_TOKENS = (-1.0, 0.0, 1.0)
_POOL_COMPONENTS = ("primary", "predecessor_a", "predecessor_b", "uniform_floor")


class ArtifactValidationError(ValueError):
    """The raw artifact is not authoritative input for this analyzer."""


@dataclass(frozen=True)
class GateSpec:
    """Frozen design constants, injectable only for small unit-test fixtures."""

    mode: str
    base_seed: int
    first_episode_index: int
    first_pair_index: int
    max_clusters: int
    pre_outcome_seed_schedule_sha256: str
    candidate_identity: Mapping[str, str]
    parent_identity: Mapping[str, str]
    alpha: float = FROZEN_ALPHA
    c: float = FROZEN_C
    gate_class: str = FROZEN_GATE_CLASS
    delta_worthwhile: float = FROZEN_DELTA_WORTHWHILE
    delta_promote: float = FROZEN_DELTA_PROMOTE
    conditional_mean_stability: str = FROZEN_CONDITIONAL_MEAN_STABILITY


_CANDIDATE_IDENTITY = {
    "file_sha256": "a0b7752181a562f8e5a0821a490ce20b777b509855d754283536e8242f489b98",
    "native_state_sha256": "ab7dd25ca6619a4a613ca089e1eb8e75981f8e5cfc0bae8535b78cddd7efa952",
    "model_parameter_sha256": "5efe2f167045bde379da3be8af6c480b6702f5d7a849ff8435d8ac6b1d91daa8",
}
_PARENT_IDENTITY = {
    "native_state_sha256": "00333d987584d5cf7f9a37f1ba2b558cfd22a60388f2487c1bf1623fcc6686a0",
    "model_parameter_sha256": "5c8e09aabab375a2eb73aba2201b8d616a18bac13f28f74a03d93c6ff0e05c6b",
}


FROZEN_SPECS = {
    "initial": GateSpec(
        mode="initial",
        base_seed=FROZEN_BASE_SEED,
        first_episode_index=131072,
        first_pair_index=65536,
        max_clusters=FROZEN_MAX_CLUSTERS,
        pre_outcome_seed_schedule_sha256="488b64430f2aa806dbaa2689e6bd0d14570f87ed091ca1ac4c553561d05dfa96",
        candidate_identity=_CANDIDATE_IDENTITY,
        parent_identity=_PARENT_IDENTITY,
    ),
    "confirmation": GateSpec(
        mode="confirmation",
        base_seed=FROZEN_BASE_SEED,
        first_episode_index=196608,
        first_pair_index=98304,
        max_clusters=FROZEN_MAX_CLUSTERS,
        pre_outcome_seed_schedule_sha256="b82fa7bd4b4220bcfac60415c097448e7d992846871f1d485865dc3e12f9faaa",
        candidate_identity=_CANDIDATE_IDENTITY,
        parent_identity=_PARENT_IDENTITY,
    ),
}


def _sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_load(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_no_duplicate_keys)
    except (OSError, UnicodeError, json.JSONDecodeError, ArtifactValidationError) as exc:
        raise ArtifactValidationError(f"cannot load JSON {path}: {exc}") from exc


def _no_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ArtifactValidationError(f"duplicate JSON object key {key!r}")
        result[key] = value
    return result


def _fail(message: str) -> None:
    raise ArtifactValidationError(message)


def _mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        _fail(f"{label} must be an object")
    return value


def _required(mapping: Mapping[str, Any], key: str, label: str) -> Any:
    if key not in mapping:
        _fail(f"{label} is missing required field {key!r}")
    return mapping[key]


def _string(value: Any, label: str, *, nonempty: bool = True) -> str:
    if not isinstance(value, str) or (nonempty and not value):
        _fail(f"{label} must be a {'nonempty ' if nonempty else ''}string")
    return value


def _integer(value: Any, label: str, *, minimum: int | None = None) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        _fail(f"{label} must be an integer")
    if minimum is not None and value < minimum:
        _fail(f"{label} must be >= {minimum}")
    return value


def _finite_number(value: Any, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        _fail(f"{label} must be a real number")
    result = float(value)
    if not math.isfinite(result):
        _fail(f"{label} must be finite")
    return result


def _exact_number(value: Any, allowed: Sequence[float], label: str) -> float:
    result = _finite_number(value, label)
    if result not in allowed:
        _fail(f"{label}={value!r} is outside the exact allowed alphabet {tuple(allowed)!r}")
    return result


def _validate_sha(value: Any, label: str) -> str:
    result = _string(value, label).lower()
    if len(result) != 64 or any(ch not in "0123456789abcdef" for ch in result):
        _fail(f"{label} must be a 64-character SHA-256 hex string")
    return result


def _safe_filename(value: Any, label: str) -> str:
    name = _string(value, label)
    if Path(name).name != name or name in ("", ".", ".."):
        _fail(f"{label} must be a relative basename, got {name!r}")
    return name


def _validate_identity(value: Any, expected: Mapping[str, str], label: str) -> dict[str, Any]:
    identity = dict(_mapping(value, label))
    for key, expected_value in expected.items():
        observed = _validate_sha(_required(identity, key, label), f"{label}.{key}")
        if observed != expected_value:
            _fail(f"{label}.{key} does not match the frozen design")
    return identity


def _validate_gate_contract(value: Any, spec: GateSpec) -> dict[str, Any]:
    gate = dict(_mapping(value, "report.gate"))
    expected_id = f"candidate-01-gae-{'initial' if spec.mode == 'initial' else 'confirm'}"
    exact = {
        "gate_id": expected_id,
        "gate_class": spec.gate_class,
        "conditional_mean_stability": spec.conditional_mean_stability,
        "blinded_pilot": "none",
        "alpha_pool": "candidates",
        "alpha_consumed_at_launch": True,
    }
    for key, expected in exact.items():
        if _required(gate, key, "report.gate") != expected:
            _fail(f"report.gate.{key} does not match the frozen design")
    for key, expected in {
        "delta_worthwhile": spec.delta_worthwhile,
        "delta_promote": spec.delta_promote,
        "alpha": spec.alpha,
        "c": spec.c,
    }.items():
        if _finite_number(_required(gate, key, "report.gate"), f"report.gate.{key}") != expected:
            _fail(f"report.gate.{key} does not match the frozen design")
    return gate


def _validate_initial_success_authority(value: Any, spec: GateSpec) -> dict[str, Any] | None:
    if spec.mode == "initial":
        if value is not None:
            _fail("initial gate must not carry an initial_success_authority")
        return None
    authority = dict(_mapping(value, "report.initial_success_authority"))
    authority_path = Path(_string(_required(authority, "path", "report.initial_success_authority"), "report.initial_success_authority.path")).resolve()
    authority_sha = _validate_sha(
        _required(authority, "sha256", "report.initial_success_authority"),
        "report.initial_success_authority.sha256",
    )
    if not authority_path.is_file() or _sha256_file(authority_path) != authority_sha:
        _fail("report.initial_success_authority does not bind the retained initial analysis")
    initial = _mapping(_json_load(authority_path), "initial analysis authority")
    if _string(_required(initial, "schema", "initial analysis authority"), "initial analysis authority.schema") != ANALYSIS_SCHEMA:
        _fail("initial analysis authority schema mismatch")
    if _string(_required(initial, "status", "initial analysis authority"), "initial analysis authority.status") != "analysis-complete":
        _fail("initial analysis authority is incomplete")
    if _string(_required(initial, "mode", "initial analysis authority"), "initial analysis authority.mode") != "initial":
        _fail("confirmation authority must be an initial-gate analysis")
    initial_schedule = _validate_sha(
        _required(initial, "pre_outcome_seed_schedule_sha256", "initial analysis authority"),
        "initial analysis authority.pre_outcome_seed_schedule_sha256",
    )
    if initial_schedule != FROZEN_SPECS["initial"].pre_outcome_seed_schedule_sha256:
        _fail("initial analysis authority schedule mismatch")
    initial_candidate = _validate_identity(
        _required(initial, "candidate", "initial analysis authority"),
        _CANDIDATE_IDENTITY,
        "initial analysis authority.candidate",
    )
    decision = _mapping(_required(initial, "gate_decision", "initial analysis authority"), "initial analysis authority.gate_decision")
    if _string(_required(decision, "verdict", "initial analysis authority.gate_decision"), "initial analysis authority.gate_decision.verdict") != "SUCCESS":
        _fail("confirmation is unauthorized because the initial gate did not reach SUCCESS")
    decision_n = _integer(
        _required(decision, "decision_n", "initial analysis authority.gate_decision"),
        "initial analysis authority.gate_decision.decision_n",
        minimum=1,
    )
    if decision_n > FROZEN_MAX_CLUSTERS:
        _fail("initial SUCCESS decision_n exceeds the frozen cap")
    if authority.get("verdict") != "SUCCESS" or authority.get("decision_n") != decision_n:
        _fail("report.initial_success_authority summary disagrees with the bound analysis")
    if authority.get("pre_outcome_seed_schedule_sha256") != initial_schedule:
        _fail("report.initial_success_authority schedule summary disagrees with the bound analysis")
    raw_root = Path(
        _string(
            _required(authority, "raw_artifact_directory", "report.initial_success_authority"),
            "report.initial_success_authority.raw_artifact_directory",
        )
    ).resolve()
    proof = verify_existing_analysis(raw_root, authority_path)
    for authority_key, proof_key in (
        ("sha256", "analysis_sha256"),
        ("decision_n", "decision_n"),
        ("raw_report_sha256", "raw_report_sha256"),
        ("realized_score_stream_sha256", "realized_score_stream_sha256"),
        ("reference_sha256", "reference_sha256"),
    ):
        if authority.get(authority_key) != proof.get(proof_key):
            _fail(f"report.initial_success_authority.{authority_key} disagrees with full recomputation")
    del initial_candidate
    return authority


def _append_seed_atom(hasher: Any, tag: str, payload: bytes) -> None:
    tag_bytes = tag.encode("utf-8")
    hasher.update(len(tag_bytes).to_bytes(4, "big"))
    hasher.update(tag_bytes)
    hasher.update(len(payload).to_bytes(8, "big"))
    hasher.update(payload)


def _native_environment_seed(base_seed: int, pair_index: int) -> int:
    """Reproduce native_trainer_schedule_v1 train-env derivation exactly."""
    for label, value in (("base_seed", base_seed), ("pair_index", pair_index)):
        if value < 0 or value >= 1 << 63:
            _fail(f"{label} is outside the frozen u63 schedule domain")
    hasher = hashlib.sha256()
    _append_seed_atom(hasher, "version", b"kernel-python-rl-trainer-sha256-v2")
    _append_seed_atom(hasher, "namespace", b"train-env")
    for label, value in (("base_seed", base_seed), ("pair_index", pair_index)):
        _append_seed_atom(hasher, "field-name", label.encode("utf-8"))
        _append_seed_atom(hasher, "u63", value.to_bytes(8, "big"))
    return int.from_bytes(hasher.digest()[:8], "big") & ((1 << 63) - 1)


def _reference_path() -> Path:
    return Path(__file__).resolve().parents[3] / "collab" / "eb_cs_reference_v1.py"


def load_reference_module(path: Path | None = None) -> ModuleType:
    """Import the exact shared V3 reference implementation by file path."""
    source = (path or _reference_path()).resolve()
    if not source.is_file():
        _fail(f"V3 reference implementation not found: {source}")
    module_name = "eb_cs_reference_v1_formal_sidecar"
    spec = importlib.util.spec_from_file_location(module_name, source)
    if spec is None or spec.loader is None:
        _fail(f"cannot import V3 reference implementation: {source}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[module_name] = module
    spec.loader.exec_module(module)
    return module


def _schedule_identifier(spec: GateSpec, cluster: Mapping[str, Any]) -> str:
    p0 = _mapping(cluster["p0"], "cluster.p0")
    p1 = _mapping(cluster["p1"], "cluster.p1")
    return (
        "mtg-kernel-native-trainer-schedule-sha256-v2;"
        f"base_seed={spec.base_seed};pair_index={cluster['pair_index']};"
        f"episode_p0={p0['episode_index']};p0_component={p0['opponent_component']};"
        f"episode_p1={p1['episode_index']};p1_component={p1['opponent_component']}"
    )


def _validate_cluster(
    cluster: Any,
    ordinal: int,
    spec: GateSpec,
) -> tuple[dict[str, Any], float, tuple[float, float]]:
    record = dict(_mapping(cluster, f"cluster[{ordinal}]"))
    observed_ordinal = _integer(_required(record, "ordinal", f"cluster[{ordinal}]"), f"cluster[{ordinal}].ordinal", minimum=0)
    if observed_ordinal != ordinal:
        _fail(f"cluster ordinal is not contiguous: expected {ordinal}, got {observed_ordinal}")
    pair_index = _integer(_required(record, "pair_index", f"cluster[{ordinal}]"), f"cluster[{ordinal}].pair_index", minimum=0)
    expected_pair = spec.first_pair_index + ordinal
    if pair_index != expected_pair:
        _fail(f"cluster[{ordinal}].pair_index={pair_index} expected {expected_pair}")

    legs: list[dict[str, Any]] = []
    leg_scores: list[float] = []
    expected_environment_seed = _native_environment_seed(spec.base_seed, pair_index)
    for seat, expected_episode in (("p0", 2 * pair_index), ("p1", 2 * pair_index + 1)):
        leg = dict(_mapping(_required(record, seat, f"cluster[{ordinal}]"), f"cluster[{ordinal}].{seat}"))
        episode = _integer(_required(leg, "episode_index", f"cluster[{ordinal}].{seat}"), f"cluster[{ordinal}].{seat}.episode_index", minimum=0)
        if episode != expected_episode:
            _fail(f"cluster[{ordinal}].{seat}.episode_index={episode} expected {expected_episode}")
        if episode < spec.first_episode_index or episode >= spec.first_episode_index + 2 * spec.max_clusters:
            _fail(f"cluster[{ordinal}].{seat}.episode_index is outside the frozen {spec.mode} range")
        environment_seed = _integer(
            _required(leg, "environment_seed", f"cluster[{ordinal}].{seat}"),
            f"cluster[{ordinal}].{seat}.environment_seed",
            minimum=0,
        )
        if environment_seed != expected_environment_seed:
            _fail(f"cluster[{ordinal}].{seat}.environment_seed does not match the frozen native schedule")
        component = _string(
            _required(leg, "opponent_component", f"cluster[{ordinal}].{seat}"),
            f"cluster[{ordinal}].{seat}.opponent_component",
        )
        if component not in _POOL_COMPONENTS:
            _fail(f"cluster[{ordinal}].{seat}.opponent_component is outside the frozen Pool3 alphabet")
        parent_return = _exact_number(
            _required(leg, "parent_return", f"cluster[{ordinal}].{seat}"),
            _TERMINAL_RETURN_TOKENS,
            f"cluster[{ordinal}].{seat}.parent_return",
        )
        candidate_return = _exact_number(
            _required(leg, "candidate_return", f"cluster[{ordinal}].{seat}"),
            _TERMINAL_RETURN_TOKENS,
            f"cluster[{ordinal}].{seat}.candidate_return",
        )
        derived_score = 1.0 if candidate_return > parent_return else -1.0 if candidate_return < parent_return else 0.0
        reported_score = _exact_number(_required(leg, "leg_score", f"cluster[{ordinal}].{seat}"), _LEG_SCORE_TOKENS, f"cluster[{ordinal}].{seat}.leg_score")
        if reported_score != derived_score:
            _fail(f"cluster[{ordinal}].{seat}.leg_score disagrees with candidate_return and parent_return")
        legs.append(leg)
        leg_scores.append(reported_score)

    if legs[0]["environment_seed"] != legs[1]["environment_seed"]:
        _fail(f"cluster[{ordinal}] p0/p1 environment_seed values differ")
    cluster_score = _exact_number(_required(record, "cluster_score", f"cluster[{ordinal}]"), _SCORE_TOKENS, f"cluster[{ordinal}].cluster_score")
    derived_cluster_score = (leg_scores[0] + leg_scores[1]) / 2.0
    if cluster_score != derived_cluster_score:
        _fail(f"cluster[{ordinal}].cluster_score disagrees with its two leg scores")
    record["p0"], record["p1"] = legs
    record["cluster_score"] = cluster_score
    record["ordinal"] = ordinal
    record["pair_index"] = pair_index
    return record, cluster_score, (leg_scores[0], leg_scores[1])


def _validate_raw(artifact_dir: Path, spec: GateSpec, reference: ModuleType) -> tuple[dict[str, Any], list[dict[str, Any]], list[float], list[tuple[float, float]], str, dict[str, str]]:
    report_path = artifact_dir / "report.json"
    report = dict(_mapping(_json_load(report_path), "report.json"))
    if _string(_required(report, "schema", "report.json"), "report.schema") != RAW_REPORT_SCHEMA:
        _fail("report.schema does not match the V3 raw-artifact schema")
    if _string(_required(report, "mode", "report.json"), "report.mode") != spec.mode:
        _fail(f"report.mode does not match frozen mode {spec.mode!r}")
    if _string(_required(report, "status", "report.json"), "report.status") != "measurement-complete":
        _fail("report.status must be 'measurement-complete'")
    if _string(_required(report, "reward", "report.json"), "report.reward") != "natural-terminal-win-loss-draw-only/v1":
        _fail("report.reward does not match the terminal-only formal contract")
    if _integer(_required(report, "base_seed", "report.json"), "report.base_seed") != spec.base_seed:
        _fail("report.base_seed does not match the frozen design")
    if _integer(_required(report, "first_episode_index", "report.json"), "report.first_episode_index") != spec.first_episode_index:
        _fail("report.first_episode_index does not match the frozen mode")
    if _integer(_required(report, "max_clusters", "report.json"), "report.max_clusters") != spec.max_clusters:
        _fail("report.max_clusters does not match the frozen design")
    observed_clusters = _integer(_required(report, "observed_clusters", "report.json"), "report.observed_clusters", minimum=1)
    if observed_clusters != spec.max_clusters:
        _fail("report.observed_clusters must equal the frozen full-cap measurement")
    worker_count = _integer(_required(report, "worker_count", "report.json"), "report.worker_count", minimum=1)
    sessions_per_worker = _integer(
        _required(report, "sessions_per_worker", "report.json"),
        "report.sessions_per_worker",
        minimum=1,
    )
    if worker_count * sessions_per_worker != 64:
        _fail("formal worker_count*sessions_per_worker must equal 64")
    if _integer(_required(report, "gpu_ordinal", "report.json"), "report.gpu_ordinal", minimum=0) != 1:
        _fail("formal measurement must bind exclusive physical GPU ordinal 1")
    schedule_sha = _validate_sha(_required(report, "pre_outcome_seed_schedule_sha256", "report.json"), "report.pre_outcome_seed_schedule_sha256")
    if schedule_sha != spec.pre_outcome_seed_schedule_sha256:
        _fail("report.pre_outcome_seed_schedule_sha256 does not match the frozen design")
    candidate = _validate_identity(_required(report, "candidate", "report.json"), spec.candidate_identity, "report.candidate")
    parent = _validate_identity(_required(report, "parent", "report.json"), spec.parent_identity, "report.parent")
    if candidate == parent:
        _fail("candidate and parent identities must be distinct")
    gate = _validate_gate_contract(_required(report, "gate", "report.json"), spec)
    initial_authority = _validate_initial_success_authority(
        _required(report, "initial_success_authority", "report.json"),
        spec,
    )
    run_start_path = artifact_dir / "run-start.json"
    run_start_sha = _validate_sha(
        _required(report, "run_start_sha256", "report.json"),
        "report.run_start_sha256",
    )
    if not run_start_path.is_file() or _sha256_file(run_start_path) != run_start_sha:
        _fail("report.run_start_sha256 does not bind the retained pre-outcome run-start.json")
    run_start = _mapping(_json_load(run_start_path), "run-start.json")
    exact_start_fields = {
        "schema": "mtg-kernel-gae-v3-formal-run-start/v1",
        "mode": spec.mode,
        "status": "measurement-started",
        "base_seed": spec.base_seed,
        "first_episode_index": spec.first_episode_index,
        "max_clusters": spec.max_clusters,
        "pre_outcome_seed_schedule_sha256": spec.pre_outcome_seed_schedule_sha256,
        "worker_count": worker_count,
        "sessions_per_worker": sessions_per_worker,
        "gpu_ordinal": 1,
        "parent_native_state_sha256": parent["native_state_sha256"],
        "candidate_native_state_sha256": candidate["native_state_sha256"],
        "gate": gate,
        "initial_success_authority": initial_authority,
    }
    for key, expected in exact_start_fields.items():
        if _required(run_start, key, "run-start.json") != expected:
            _fail(f"run-start.json.{key} disagrees with the completed report or frozen design")
    nonclaims = _required(report, "nonclaims", "report.json")
    if not isinstance(nonclaims, list) or not nonclaims or any(not isinstance(item, str) or not item for item in nonclaims):
        _fail("report.nonclaims must be a nonempty list of nonempty strings")

    chunks = _required(report, "chunks", "report.json")
    if not isinstance(chunks, list) or not chunks:
        _fail("report.chunks must be a nonempty list")
    all_clusters: list[dict[str, Any]] = []
    scores: list[float] = []
    leg_scores: list[tuple[float, float]] = []
    chunk_hashes: dict[str, str] = {}
    expected_ordinal = 0
    for expected_chunk_index, chunk_meta_raw in enumerate(chunks):
        chunk_meta = _mapping(chunk_meta_raw, f"report.chunks[{expected_chunk_index}]")
        chunk_index = _integer(_required(chunk_meta, "chunk_index", f"report.chunks[{expected_chunk_index}]"), f"report.chunks[{expected_chunk_index}].chunk_index", minimum=0)
        if chunk_index != expected_chunk_index:
            _fail(f"report chunks are not ordered contiguously at index {expected_chunk_index}")
        file_name = _safe_filename(_required(chunk_meta, "file_name", f"report.chunks[{expected_chunk_index}]"), f"report.chunks[{expected_chunk_index}].file_name")
        declared_hash = _validate_sha(_required(chunk_meta, "sha256", f"report.chunks[{expected_chunk_index}]"), f"report.chunks[{expected_chunk_index}].sha256")
        first_ordinal = _integer(_required(chunk_meta, "first_cluster_ordinal", f"report.chunks[{expected_chunk_index}]"), f"report.chunks[{expected_chunk_index}].first_cluster_ordinal", minimum=0)
        cluster_count = _integer(_required(chunk_meta, "cluster_count", f"report.chunks[{expected_chunk_index}]"), f"report.chunks[{expected_chunk_index}].cluster_count", minimum=1)
        if first_ordinal != expected_ordinal:
            _fail(f"chunk {chunk_index} starts at ordinal {first_ordinal}, expected {expected_ordinal}")
        if file_name in chunk_hashes:
            _fail(f"chunk file {file_name!r} is referenced more than once")
        chunk_path = artifact_dir / file_name
        if not chunk_path.is_file():
            _fail(f"chunk file does not exist: {file_name}")
        observed_hash = _sha256_file(chunk_path)
        if observed_hash != declared_hash:
            _fail(f"chunk {file_name} SHA-256 does not match report binding")
        chunk_hashes[file_name] = declared_hash
        chunk = dict(_mapping(_json_load(chunk_path), file_name))
        if _string(_required(chunk, "schema", file_name), f"{file_name}.schema") != RAW_CHUNK_SCHEMA:
            _fail(f"{file_name}.schema does not match the V3 chunk schema")
        if _string(_required(chunk, "mode", file_name), f"{file_name}.mode") != spec.mode:
            _fail(f"{file_name}.mode does not match frozen mode")
        if _integer(_required(chunk, "chunk_index", file_name), f"{file_name}.chunk_index", minimum=0) != chunk_index:
            _fail(f"{file_name}.chunk_index does not match report metadata")
        if _integer(
            _required(chunk, "first_cluster_ordinal", file_name),
            f"{file_name}.first_cluster_ordinal",
            minimum=0,
        ) != first_ordinal:
            _fail(f"{file_name}.first_cluster_ordinal does not match report metadata")
        if _integer(
            _required(chunk, "cluster_count", file_name),
            f"{file_name}.cluster_count",
            minimum=1,
        ) != cluster_count:
            _fail(f"{file_name}.cluster_count does not match report metadata")
        raw_clusters = _required(chunk, "clusters", file_name)
        if not isinstance(raw_clusters, list) or len(raw_clusters) != cluster_count:
            _fail(f"{file_name}.clusters length does not match report cluster_count")
        for raw_cluster in raw_clusters:
            validated, score, pair = _validate_cluster(raw_cluster, expected_ordinal, spec)
            all_clusters.append(validated)
            scores.append(score)
            leg_scores.append(pair)
            expected_ordinal += 1
    if expected_ordinal != observed_clusters:
        _fail(f"report.observed_clusters={observed_clusters} but chunks contain {expected_ordinal} clusters")
    identifiers = [_schedule_identifier(spec, cluster) for cluster in all_clusters]
    computed_schedule = reference.canonical_ordered_identifier_sha256(identifiers)
    if computed_schedule != schedule_sha:
        _fail("full retained cluster schedule does not match pre-outcome schedule SHA-256")
    return report, all_clusters, scores, leg_scores, schedule_sha, chunk_hashes


def _score_token(score: float) -> str:
    return {-1.0: "-1", -0.5: "-0.5", 0.0: "0", 0.5: "0.5", 1.0: "1"}[score]


def _write_exclusive(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        raise ArtifactValidationError(f"analysis output already exists: {path}")
    staging = path.with_name(f".{path.name}.partial")
    try:
        descriptor = os.open(str(staging), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise ArtifactValidationError(f"analysis staging output already exists: {staging}") from exc
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.rename(staging, path)
    except Exception:
        try:
            os.close(descriptor)
        except OSError:
            pass
        try:
            staging.unlink()
        except OSError:
            pass
        raise


def analyze_artifact(
    artifact_dir: str | Path,
    *,
    output_path: str | Path | None = None,
    reference_path: str | Path | None = None,
    spec: GateSpec | None = None,
    reference_module: ModuleType | None = None,
    write_output: bool = True,
) -> dict[str, Any]:
    """Validate and analyze one completed raw formal-gate artifact directory."""
    root = Path(artifact_dir).resolve()
    if not root.is_dir():
        _fail(f"raw artifact directory does not exist: {root}")
    report_probe = _json_load(root / "report.json")
    mode = _string(_required(_mapping(report_probe, "report.json"), "mode", "report.json"), "report.mode")
    if mode not in FROZEN_MODES and spec is None:
        _fail(f"unsupported raw-artifact mode {mode!r}")
    chosen_spec = spec or FROZEN_SPECS[mode]
    if chosen_spec.mode != mode:
        _fail("injected GateSpec mode does not match report.mode")
    reference_source = (Path(reference_path) if reference_path else _reference_path()).resolve()
    if reference_module is None and reference_path is None and _sha256_file(reference_source) != FROZEN_REFERENCE_SHA256:
        _fail("V3 reference implementation SHA-256 does not match the pre-outcome freeze")
    reference = reference_module or load_reference_module(reference_source)
    report, clusters, scores, leg_scores, schedule_sha, chunk_hashes = _validate_raw(root, chosen_spec, reference)
    trajectory = reference.compute_eb_cs_trajectory(scores, alpha=chosen_spec.alpha, c=chosen_spec.c)
    decision = reference.gate_decision(
        trajectory,
        max_n=chosen_spec.max_clusters,
        delta_promote=chosen_spec.delta_promote,
        delta_worthwhile=chosen_spec.delta_worthwhile,
        gate_class=chosen_spec.gate_class,
    )
    realized_score_sha = reference.canonical_stream_sha256(scores)
    cluster_counts = {_score_token(token): scores.count(token) for token in _SCORE_TOKENS}
    leg_counts: dict[str, dict[str, int]] = {}
    for seat, index in (("p0", 0), ("p1", 1)):
        values = [pair[index] for pair in leg_scores]
        leg_counts[seat] = {"wins": values.count(1.0), "ties": values.count(0.0), "losses": values.count(-1.0), "games": len(values)}
    trajectory_records = [
        {
            "n": point.n,
            "lambda_t": point.lambda_t,
            "mu_hat_t": point.mu_hat_t,
            "sigma_hat_sq_t": point.sigma_hat_sq_t,
            "v_t": point.v_t,
            "center_nu": point.center_nu,
            "half_width_nu": point.half_width_nu,
            "delta_hat_running": point.delta_hat_running,
            "cs_delta_lower": point.cs_delta_lower,
            "cs_delta_upper": point.cs_delta_upper,
            "is_empty_cs": point.is_empty_cs,
        }
        for point in trajectory
    ]
    decision_record = {"verdict": decision.verdict, "decision_n": decision.decision_n, "reason": decision.reason}
    analysis = {
        "schema": ANALYSIS_SCHEMA,
        "status": "analysis-complete",
        "raw_artifact": {"directory": str(root), "run_start_sha256": report["run_start_sha256"], "report_sha256": _sha256_file(root / "report.json"), "chunk_sha256": chunk_hashes},
        "mode": chosen_spec.mode,
        "base_seed": chosen_spec.base_seed,
        "first_episode_index": chosen_spec.first_episode_index,
        "max_clusters": chosen_spec.max_clusters,
        "observed_clusters": len(scores),
        "candidate": dict(report["candidate"]),
        "parent": dict(report["parent"]),
        "gate_constants": {
            "gate_class": chosen_spec.gate_class,
            "delta_worthwhile": chosen_spec.delta_worthwhile,
            "delta_promote": chosen_spec.delta_promote,
            "alpha": chosen_spec.alpha,
            "c": chosen_spec.c,
            "conditional_mean_stability": chosen_spec.conditional_mean_stability,
        },
        "pre_outcome_seed_schedule_sha256": schedule_sha,
        "realized_score_stream_sha256": realized_score_sha,
        "scores": {"alphabet": list(_SCORE_TOKENS), "cluster_counts": cluster_counts, "sum": sum(scores), "mean": sum(scores) / len(scores)},
        "leg_reporting": leg_counts,
        "trajectory": trajectory_records,
        "looks": trajectory_records,
        "gate_decision": decision_record,
        "decision": decision_record,
        "nonclaims": list(report["nonclaims"]),
        "reference": {"path": str(reference_source), "sha256": _sha256_file(reference_source)},
    }
    if write_output:
        output = Path(output_path).resolve() if output_path is not None else root / "analysis.json"
        payload = (json.dumps(analysis, indent=2, sort_keys=True, allow_nan=False) + "\n").encode("utf-8")
        _write_exclusive(output, payload)
    return analysis


def verify_existing_analysis(
    artifact_dir: str | Path,
    analysis_path: str | Path,
    *,
    spec: GateSpec | None = None,
    reference_module: ModuleType | None = None,
) -> dict[str, Any]:
    """Recompute the entire frozen analysis and require exact retained identity."""
    root = Path(artifact_dir).resolve()
    retained_path = Path(analysis_path).resolve()
    retained = _mapping(_json_load(retained_path), "retained analysis")
    recomputed = analyze_artifact(
        root,
        spec=spec,
        reference_module=reference_module,
        write_output=False,
    )
    if retained != recomputed:
        _fail("retained analysis does not exactly match full recomputation from raw authorities")
    decision = _mapping(_required(recomputed, "gate_decision", "recomputed analysis"), "recomputed gate_decision")
    if decision.get("verdict") != "SUCCESS":
        _fail("initial gate did not reach SUCCESS; confirmation is unauthorized")
    return {
        "schema": "mtg-kernel-gae-v3-confirmation-authorization/v1",
        "authorized": True,
        "mode": recomputed["mode"],
        "verdict": "SUCCESS",
        "decision_n": decision["decision_n"],
        "analysis_path": str(retained_path),
        "analysis_sha256": _sha256_file(retained_path),
        "raw_artifact_directory": str(root),
        "raw_report_sha256": recomputed["raw_artifact"]["report_sha256"],
        "realized_score_stream_sha256": recomputed["realized_score_stream_sha256"],
        "reference_sha256": recomputed["reference"]["sha256"],
    }


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Analyze a frozen GAE V3 formal raw artifact")
    parser.add_argument("artifact_dir", type=Path)
    parser.add_argument("--output", type=Path, default=None)
    parser.add_argument("--reference", type=Path, default=None)
    parser.add_argument("--verify-existing", type=Path, default=None)
    args = parser.parse_args(argv)
    try:
        if args.verify_existing is not None:
            if args.output is not None or args.reference is not None:
                parser.error("--verify-existing cannot be combined with --output or --reference")
            proof = verify_existing_analysis(args.artifact_dir, args.verify_existing)
            print(json.dumps(proof, sort_keys=True))
            return 0
        result = analyze_artifact(args.artifact_dir, output_path=args.output, reference_path=args.reference)
    except (ArtifactValidationError, OSError, ImportError, ValueError) as exc:
        parser.error(str(exc))
    print(json.dumps({"output": str(args.output or args.artifact_dir / "analysis.json"), "verdict": result["gate_decision"]["verdict"], "decision_n": result["gate_decision"]["decision_n"]}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
