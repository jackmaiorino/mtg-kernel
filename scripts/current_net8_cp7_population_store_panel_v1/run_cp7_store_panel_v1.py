#!/usr/bin/env python3
"""Run a terminal-only, three-model CP7 Rally panel from verified StoreV2 checkpoints.

This runner has no promotion decision. It accepts only population Store roots,
binds each requested generation before launching XMage, and creates a new
evidence root for an explicit smoke or formal panel.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from typing import Any


AUTHORITY_KIND = "population-store-validated-generation"
ENVIRONMENT_CONTRACT = "environment-randomization-v2"
SAMPLER_IDENTITY = "f32-q8-expq63-hamilton-splitmix64-v1"
SAMPLER_CONTRACT = "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6"
OUTCOME_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
CARD_DB_HASH = "b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d"
HEX_16 = re.compile(r"[0-9a-f]{16}\Z")
HEX_32 = re.compile(r"[0-9a-f]{32}\Z")
HEX_64 = re.compile(r"[0-9a-f]{64}\Z")
DECISION_KEYS = {
    "record_type", "schema_version", "record_ordinal", "outcome_decision_ordinal",
    "pair_index", "episode_id", "candidate_seat", "base_seed_u64_hex",
    "pair_environment_seed_u64_hex", "deck_ids", "environment_revision",
    "randomization_identity", "selection_source", "acting_player", "step",
    "decision_kind", "physical_decision_id", "actor_physical_decision_ordinal",
    "substep_index", "substep_count", "legal_action_count", "selected_index",
    "selected_semantic", "candidate_order_commitment_128_hex", "action_semantics",
    "tensor", "model_input_sha256", "old_policy_logits_f32_bits",
    "old_value_f32_bits", "checkpoint",
}
TERMINAL_KEYS = {
    "record_type", "schema_version", "record_ordinal", "pair_index", "episode_id",
    "candidate_seat", "base_seed_u64_hex", "pair_environment_seed_u64_hex",
    "deck_ids", "randomization_identity", "core_environment_hash_u64_hex",
    "diagnostic_state_hash_u64_hex", "first_outcome_decision_ordinal",
    "outcome_decision_count", "terminal", "candidate_terminal_reward", "checkpoint",
}


def fail(message: str) -> None:
    raise ValueError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical_json_bytes(value: Any, *, indent: int | None = None) -> bytes:
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True,
                       indent=indent, separators=None if indent is not None else (",", ":"))
            + "\n").encode("utf-8")


def exclusive_write(path: Path, payload: bytes) -> None:
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise


def load_canonical_json(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    if not raw.endswith(b"\n") or b"\r" in raw:
        fail(f"noncanonical JSON file: {path}")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"invalid JSON {path}: {error}")
    if not isinstance(value, dict):
        fail(f"JSON document is not an object: {path}")
    return value


def require_sha(value: Any, context: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or any(
        character not in "0123456789abcdef" for character in value
    ):
        fail(f"invalid SHA-256 {context}")
    return value


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, check=True,
    )
    return completed.stdout.splitlines()[0] if completed.stdout else ""


def _git_commit(repo: Path) -> str:
    return _version(["git", "-c", f"safe.directory={repo}", "-C", str(repo),
                     "rev-parse", "HEAD"])


def load_store_identity(root: Path, generation: int) -> dict[str, Any]:
    if generation < 0 or not root.is_dir():
        fail("population Store root or generation is invalid")
    root = root.resolve()
    stem = f"update-{generation:08d}"
    run_path = root / "run.json"
    checkpoint_path = root / "checkpoints" / f"{stem}.checkpoint.json"
    sidecar_path = root / "checkpoints" / f"{stem}.sidecar.json"
    state_path = root / "checkpoints" / f"{stem}.state.f32le"
    latest_path = root / "latest.json"
    if not all(path.is_file() for path in (run_path, checkpoint_path, sidecar_path, state_path, latest_path)):
        fail(f"population Store generation files are incomplete: {root} g{generation}")
    run = load_canonical_json(run_path)
    checkpoint = load_canonical_json(checkpoint_path)
    sidecar = load_canonical_json(sidecar_path)
    latest = load_canonical_json(latest_path)
    run_sha = sha256(run_path)
    checkpoint_sha = sha256(checkpoint_path)
    sidecar_sha = sha256(sidecar_path)
    state_sha = sha256(state_path)
    if (
        run.get("schema") != "mtg_kernel_native_train_run/v2"
        or run.get("store_identity") != "mtg-kernel-native-training-store-v2"
        or checkpoint.get("schema") != "mtg_kernel_native_train_checkpoint/v3"
        or sidecar.get("schema") != "mtg_kernel_native_train_checkpoint_sidecar/v2"
        or latest.get("schema") != "mtg_kernel_native_train_latest/v2"
        or checkpoint.get("generation_index") != generation
        or sidecar.get("generation_index") != generation
        or not _plain_int(latest.get("generation_index"))
        or latest["generation_index"] < generation
    ):
        fail("population Store schema or generation mismatch")
    payload = checkpoint.get("payload")
    train_state = checkpoint.get("train_state")
    contracts = run.get("contracts")
    sampler = contracts.get("learner_sampler") if isinstance(contracts, dict) else None
    if not isinstance(payload, dict) or not isinstance(train_state, dict) or not isinstance(sampler, dict):
        fail("population Store checkpoint fields are incomplete")
    if (
        payload.get("byte_count") != state_path.stat().st_size
        or require_sha(payload.get("sha256"), "checkpoint payload") != state_sha
        or require_sha(checkpoint.get("run_sha256"), "checkpoint run") != run_sha
        or require_sha(sidecar.get("run_sha256"), "sidecar run") != run_sha
        or require_sha(sidecar.get("checkpoint_manifest_sha256"), "sidecar checkpoint") != checkpoint_sha
        or require_sha(sidecar.get("checkpoint_payload_sha256"), "sidecar payload") != state_sha
        or require_sha(sidecar.get("train_state_sha256"), "sidecar train state")
            != require_sha(train_state.get("state_sha256"), "checkpoint train state")
        or require_sha(sidecar.get("model_parameter_sha256"), "sidecar model")
            != require_sha(train_state.get("model_parameter_sha256"), "checkpoint model")
        or require_sha(latest.get("run_sha256"), "latest run") != run_sha
        or require_sha(latest.get("identity_bundle_sha256"), "latest identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or require_sha(run.get("contracts", {}).get("identity_bundle_sha256"), "run identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or sampler.get("identity") != SAMPLER_IDENTITY
        or sampler.get("contract_sha256") != SAMPLER_CONTRACT
    ):
        fail("population Store checkpoint chain mismatch")
    identity = {
        "authority_kind": AUTHORITY_KIND,
        "source_run_sha256": run_sha,
        "source_generation": generation,
        "source_checkpoint_sha256": checkpoint_sha,
        "source_sidecar_sha256": sidecar_sha,
        "source_payload_sha256": state_sha,
        "source_train_state_sha256": train_state["state_sha256"],
        "loaded_run_sha256": run_sha,
        "loaded_generation": generation,
        "loaded_checkpoint_sha256": checkpoint_sha,
        "loaded_payload_sha256": state_sha,
        "loaded_train_state_sha256": train_state["state_sha256"],
        "model_parameter_sha256": train_state["model_parameter_sha256"],
        "environment_trajectory_contract": ENVIRONMENT_CONTRACT,
        "sampler_identity": SAMPLER_IDENTITY,
        "sampler_contract_sha256": SAMPLER_CONTRACT,
    }
    return {
        "root": str(root), "generation": generation, "checkpoint": identity,
        "store_files": {
            "run_json_sha256": run_sha, "checkpoint_json_sha256": checkpoint_sha,
            "sidecar_json_sha256": sidecar_sha, "state_payload_sha256": state_sha,
            "identity_bundle_sha256": checkpoint["identity_bundle_sha256"],
            "store_head_generation": latest["generation_index"],
        },
        "payload": {
            "byte_count": payload["byte_count"],
            "parameters_sha256": payload["sections"][0]["sha256"],
            "first_moments_sha256": payload["sections"][1]["sha256"],
            "second_moments_sha256": payload["sections"][2]["sha256"],
        },
    }


def maven_opts(identity: dict[str, Any]) -> str:
    prefix = "-Dxmage.rally.populationStore."
    names = {
        "authorityKind": "authority_kind", "sourceRunSha256": "source_run_sha256",
        "sourceGeneration": "source_generation", "sourceCheckpointSha256": "source_checkpoint_sha256",
        "sourceSidecarSha256": "source_sidecar_sha256", "sourcePayloadSha256": "source_payload_sha256",
        "sourceTrainStateSha256": "source_train_state_sha256", "loadedRunSha256": "loaded_run_sha256",
        "loadedGeneration": "loaded_generation", "loadedCheckpointSha256": "loaded_checkpoint_sha256",
        "loadedPayloadSha256": "loaded_payload_sha256", "loadedTrainStateSha256": "loaded_train_state_sha256",
        "modelParameterSha256": "model_parameter_sha256",
        "environmentTrajectoryContract": "environment_trajectory_contract",
        "samplerIdentity": "sampler_identity", "samplerContractSha256": "sampler_contract_sha256",
    }
    return " ".join(prefix + key + "=" + str(identity[value]) for key, value in names.items())


def chunk_ranges(pair_start: int, pairs: int, task_pairs: int) -> list[tuple[int, int]]:
    if pair_start < 0 or pairs < 1 or task_pairs < 1:
        fail("invalid shard range")
    stop = pair_start + pairs
    return [(first, min(task_pairs, stop - first))
            for first in range(pair_start, stop, task_pairs)]


def planned_tasks(labels: list[str], chunks: list[tuple[int, int]]) -> list[dict[str, Any]]:
    return [
        {"label": label, "first_pair": first_pair, "pair_count": pair_count,
         "first_episode": first_pair * 2, "episode_count": pair_count * 2,
         "stem": f"{label}-p{first_pair:06d}-n{pair_count:03d}"}
        for first_pair, pair_count in chunks
        for label in sorted(labels)
    ]


def _exec_argument_string(parts: list[str]) -> str:
    def quote(value: str) -> str:
        if not value or any(character.isspace() for character in value) or '"' in value:
            return '"' + value.replace('"', '\\"') + '"'
        return value
    return " ".join(quote(part) for part in parts)


def anchor_command(args: argparse.Namespace, model: dict[str, Any], first_pair: int,
                   pair_count: int, outcome: Path) -> list[str]:
    execution_args = _exec_argument_string([
        "--repo-root", str(args.mage_repo), "--scorer-exe", str(args.scorer_exe),
        "--population-store-root", model["root"], "--generation", str(args.generation),
        "--base-seed", str(args.base_seed), "--first-episode", str(first_pair * 2),
        "--pairs", str(pair_count), "--opponent", "cp7", "--cp7-skill", "7",
        "--outcome-export", str(outcome),
    ])
    return [str(args.maven), "-o", "-q", "-pl", "Mage.Server.Plugins/Mage.Player.AIRL",
            "-DskipTests", "exec:java", "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
            "-Dexec.args=" + execution_args]


def environment(database_root: Path, model: dict[str, Any]) -> dict[str, str]:
    value = os.environ.copy()
    value.update({"MAGE_DB_DIR": str(database_root), "MAGE_DB_AUTO_SERVER": "false",
                  "AI_DETERMINISTIC_TIEBREAKS": "true", "AI_DETERMINISTIC_SEARCH": "true",
                  "AI_DETERMINISTIC_MAX_NODES": "5000", "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
                  "CUDA_VISIBLE_DEVICES": "1", "MAVEN_OPTS": maven_opts(model["checkpoint"])})
    return value


def expected_header(checkpoint: dict[str, Any]) -> dict[str, Any]:
    return {
        "record_type": "header", "schema_version": 2, "record_ordinal": 0,
        "export_contract": OUTCOME_CONTRACT,
        "selection_source": "candidate_checkpoint_policy",
        "tensorizer_identity": "mtg-kernel-python-encoded-decision-tensor-contract-v2",
        "tensorizer_features_source_sha256":
            "fce419176dbd15e2b911e5c5f688bb390e731e3817da142571f38b1a7cc778eb",
        "model_input_commitment":
            "mtg-kernel-checkpoint-shadow-model-input-framed-sha256/v1",
        "checkpoint": checkpoint,
    }


def _strict_json(raw: bytes, path: Path, line_number: int) -> dict[str, Any]:
    def object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        value: dict[str, Any] = {}
        for key, item in pairs:
            if key in value:
                fail(f"{path}:{line_number}: duplicate JSON field {key}")
            value[key] = item
        return value
    try:
        row = json.loads(raw, object_pairs_hook=object_pairs,
                         parse_constant=lambda value: fail(
                             f"{path}:{line_number}: nonfinite JSON value {value}"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        fail(f"{path}:{line_number}: invalid JSON: {error}")
    if not isinstance(row, dict):
        fail(f"{path}:{line_number}: row is not an object")
    return row


def _plain_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _validate_decision(path: Path, row: dict[str, Any], ordinal: int) -> None:
    legal_count, selected = row.get("legal_action_count"), row.get("selected_index")
    semantics, logits = row.get("action_semantics"), row.get("old_policy_logits_f32_bits")
    if (set(row) != DECISION_KEYS or row.get("record_type") != "decision"
            or row.get("selection_source") != "candidate_checkpoint_policy"
            or not _plain_int(row.get("outcome_decision_ordinal"))
            or not _plain_int(legal_count) or legal_count < 1
            or not _plain_int(selected) or not 0 <= selected < legal_count
            or not isinstance(semantics, list) or len(semantics) != legal_count
            or row.get("selected_semantic") != semantics[selected]
            or not isinstance(logits, list) or len(logits) != legal_count
            or any(not _plain_int(value) for value in logits)
            or not isinstance(row.get("tensor"), dict)
            or HEX_64.fullmatch(row.get("model_input_sha256", "")) is None
            or HEX_32.fullmatch(row.get("candidate_order_commitment_128_hex", "")) is None
            or not _plain_int(row.get("old_value_f32_bits"))
            or row.get("acting_player") != row.get("candidate_seat")
            or not _plain_int(row.get("physical_decision_id"))
            or row["physical_decision_id"] < 0
            or not _plain_int(row.get("actor_physical_decision_ordinal"))
            or row["actor_physical_decision_ordinal"] < 0
            or not _plain_int(row.get("substep_index"))
            or not _plain_int(row.get("substep_count")) or row["substep_count"] < 1
            or not 0 <= row["substep_index"] < row["substep_count"]):
        fail(f"{path}: malformed outcome-v2 decision at record {ordinal}")


def _validate_terminal(path: Path, row: dict[str, Any], ordinal: int) -> str:
    terminal, reward = row.get("terminal"), row.get("candidate_terminal_reward")
    if not isinstance(terminal, dict):
        fail(f"{path}: terminal payload missing at record {ordinal}")
    outcome = terminal.get("terminal_outcome")
    expected = {"p0_win": ("p0", [1, -1]), "p1_win": ("p1", [-1, 1]),
                "draw": (None, [0, 0])}.get(outcome)
    seat_index = 0 if row.get("candidate_seat") == "p0" else 1
    if (set(row) != TERMINAL_KEYS or row.get("record_type") != "terminal"
            or terminal.get("schema_version") != 5
            or terminal.get("episode_id") != row.get("episode_id")
            or terminal.get("terminal_classification") != "natural"
            or terminal.get("terminal_code") != "natural_game_over"
            or terminal.get("terminal_reason") != "game_over" or expected is None
            or terminal.get("winner") != expected[0]
            or terminal.get("terminal_reward") != expected[1]
            or reward not in (-1, 0, 1) or expected[1][seat_index] != reward
            or not _plain_int(row.get("outcome_decision_count"))
            or row["outcome_decision_count"] < 0):
        fail(f"{path}: terminal is not an exact natural outcome at record {ordinal}")
    return "win" if reward == 1 else "draw" if reward == 0 else "loss"


def validate_outcome_shard(path: Path, model: dict[str, Any], *, base_seed: int,
                           first_pair: int, pair_count: int) -> dict[str, Any]:
    if not path.is_file() or not 0 <= base_seed <= 0x7FFF_FFFF_FFFF_FFFF:
        fail(f"invalid outcome path or base seed: {path}")
    expected_checkpoint = model["checkpoint"]
    exact_header = expected_header(expected_checkpoint)
    expected_episode, decision_ordinal = first_pair * 2, 0
    active_episode: int | None = None
    active_first_decision: int | None = None
    active_decision_count = 0
    terminal_keys: set[tuple[int, int, str]] = set()
    environment_seeds: dict[int, str] = {}
    outcomes: list[dict[str, Any]] = []
    record_count = 0
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            digest.update(raw)
            if not raw.endswith(b"\n") or b"\r" in raw or raw == b"\n":
                fail(f"{path}:{line_number}: noncanonical JSONL row")
            row = _strict_json(raw, path, line_number)
            if raw != (json.dumps(row, ensure_ascii=False, allow_nan=False,
                                  separators=(",", ":")) + "\n").encode("utf-8"):
                fail(f"{path}:{line_number}: JSONL row is not canonical compact JSON")
            ordinal = record_count
            record_count += 1
            if ordinal == 0:
                if row != exact_header:
                    fail(f"{path}: first row is not the exact population outcome-v2 header")
                continue
            if row.get("record_type") == "header":
                fail(f"{path}: duplicate header at record {ordinal}")
            if (row.get("schema_version") != 2 or row.get("record_ordinal") != ordinal
                    or row.get("checkpoint") != expected_checkpoint):
                fail(f"{path}: schema, ordinal, or checkpoint mismatch at record {ordinal}")
            pair, episode, seat = row.get("pair_index"), row.get("episode_id"), row.get("candidate_seat")
            if (not _plain_int(pair) or not first_pair <= pair < first_pair + pair_count
                    or seat not in {"p0", "p1"} or not _plain_int(episode)
                    or episode != pair * 2 + int(seat[1]) or episode != expected_episode
                    or row.get("base_seed_u64_hex") != f"{base_seed:016x}"
                    or HEX_16.fullmatch(row.get("pair_environment_seed_u64_hex", "")) is None):
                fail(f"{path}: pair, episode, seat, or seed mismatch at record {ordinal}")
            environment_seed = row["pair_environment_seed_u64_hex"]
            if pair in environment_seeds and environment_seeds[pair] != environment_seed:
                fail(f"{path}: pair environment seed changed for pair {pair}")
            environment_seeds[pair] = environment_seed
            if row.get("record_type") == "decision":
                _validate_decision(path, row, ordinal)
                if active_episode is None:
                    active_episode, active_first_decision = episode, decision_ordinal
                    active_decision_count = 0
                elif active_episode != episode:
                    fail(f"{path}: decisions are interleaved between episodes")
                if row["outcome_decision_ordinal"] != decision_ordinal:
                    fail(f"{path}: noncontiguous outcome decision ordinal")
                decision_ordinal += 1
                active_decision_count += 1
            elif row.get("record_type") == "terminal":
                result = _validate_terminal(path, row, ordinal)
                if active_episode is None:
                    active_episode = episode
                    active_first_decision = None
                    active_decision_count = 0
                if (active_episode != episode
                        or row.get("first_outcome_decision_ordinal") != active_first_decision
                        or row.get("outcome_decision_count") != active_decision_count):
                    fail(f"{path}: terminal does not exactly close the active episode")
                key = (pair, episode, seat)
                if key in terminal_keys:
                    fail(f"{path}: duplicate terminal {key}")
                terminal_keys.add(key)
                outcomes.append({"pair_index": pair, "seat": seat, "result": result,
                                 "environment_seed": environment_seed})
                expected_episode += 1
                active_episode = active_first_decision = None
                active_decision_count = 0
            else:
                fail(f"{path}: unknown record type at record {ordinal}")
    expected_terminals = {(pair, pair * 2 + seat, f"p{seat}")
                          for pair in range(first_pair, first_pair + pair_count)
                          for seat in (0, 1)}
    if record_count == 0 or active_episode is not None or terminal_keys != expected_terminals:
        fail(f"{path}: terminal pair, episode, or seat coverage mismatch")
    if set(environment_seeds) != set(range(first_pair, first_pair + pair_count)):
        fail(f"{path}: pair environment seed coverage mismatch")
    return {"sha256": digest.hexdigest(), "byte_count": path.stat().st_size,
            "first_pair": first_pair, "pair_count": pair_count,
            "record_count": record_count, "decision_count": decision_ordinal,
            "outcomes": outcomes, "environment_seeds": environment_seeds}


class DatabaseLeasePool:
    def __init__(self, roots: list[Path]):
        if not roots:
            fail("database lease pool is empty")
        self._slots: queue.Queue[tuple[int, Path]] = queue.Queue()
        self._active: set[int] = set()
        self._lock = threading.Lock()
        for worker, root in enumerate(roots):
            self._slots.put((worker, root))

    def acquire(self) -> tuple[int, Path]:
        worker, root = self._slots.get()
        with self._lock:
            if worker in self._active:
                fail(f"database worker {worker} received an overlapping lease")
            self._active.add(worker)
        return worker, root

    def release(self, worker: int, root: Path) -> None:
        with self._lock:
            if worker not in self._active:
                fail(f"database worker {worker} released without an active lease")
            self._active.remove(worker)
        self._slots.put((worker, root))


def run_task(args: argparse.Namespace, leases: DatabaseLeasePool, label: str,
             model: dict[str, Any], first_pair: int, pair_count: int) -> dict[str, Any]:
    worker, database = leases.acquire()
    try:
        task_root = args.evidence_root / "tasks"
        stem = f"{label}-p{first_pair:06d}-n{pair_count:03d}"
        log, outcome = task_root / (stem + ".log"), task_root / (stem + ".outcome.jsonl")
        started = time.perf_counter()
        with log.open("x", encoding="utf-8", newline="\n") as handle:
            completed = subprocess.run(
                anchor_command(args, model, first_pair, pair_count, outcome),
                cwd=args.mage_repo, env=environment(database, model), stdout=handle,
                stderr=subprocess.STDOUT, timeout=args.task_timeout_seconds,
            )
        if completed.returncode != 0 or not outcome.is_file():
            fail(f"panel task failed: {label} pairs {first_pair}+{pair_count}")
        return {"label": label, "first_pair": first_pair, "pair_count": pair_count,
                "worker": worker, "elapsed_seconds": time.perf_counter() - started,
                "log": str(log.resolve()), "log_sha256": sha256(log),
                "outcome": str(outcome.resolve()), "outcome_sha256": sha256(outcome)}
    finally:
        leases.release(worker, database)


def aggregate_terminal_wdl(results: list[dict[str, Any]], labels: list[str]) -> dict[str, dict[str, Any]]:
    summary: dict[str, dict[str, Any]] = {}
    for label in labels:
        rows = [row for row in results if row["label"] == label]
        counts = {"win": 0, "draw": 0, "loss": 0}
        seats = {seat: {"win": 0, "draw": 0, "loss": 0} for seat in ("p0", "p1")}
        for row in rows:
            for seat, result in row["by_seat"].items():
                if seat not in seats or result not in counts:
                    fail("terminal aggregation input is invalid")
                counts[result] += 1
                seats[seat][result] += 1
        summary[label] = {"overall_wdl": counts, "by_seat_wdl": seats}
    return summary


def build_launch_plan(args: argparse.Namespace, identities: dict[str, dict[str, Any]],
                      tasks: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema": "mtg-kernel-population-store-cp7-panel-plan/v1",
        "status": "planned",
        "inputs": {
            "runner": str(Path(__file__).resolve()),
            "runner_sha256": sha256(Path(__file__).resolve()),
            "scorer_exe": str(args.scorer_exe.resolve()),
            "scorer_sha256": sha256(args.scorer_exe),
            "mage_repo": str(args.mage_repo.resolve()),
            "mage_commit": _git_commit(args.mage_repo),
            "source_database": str(args.source_database.resolve()),
            "source_database_sha256": sha256(args.source_database),
            "models": identities,
        },
        "panel": {
            "mode": args.mode, "opponent": "xmage-cp7", "cp7_skill": 7,
            "base_seed": args.base_seed, "pair_start": args.pair_start,
            "pair_count": args.pairs, "episode_count": args.pairs * 2,
            "generation": args.generation, "workers": args.workers,
            "task_pairs": args.task_pairs, "task_count": len(tasks),
            "task_timeout_seconds": args.task_timeout_seconds,
            "tasks": tasks,
        },
        "toolchain": {
            "python": sys.version, "rustc": _version(["rustc", "-V"]),
            "maven": _version([str(args.maven), "-version"]),
        },
        "analysis_policy": {
            "parse_outcomes_after_all_shards_complete": True,
            "terminal_win_draw_loss_only": True,
            "promotion_claim": False,
        },
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    if args.evidence_root.exists():
        fail(f"evidence root already exists: {args.evidence_root}")
    if args.pairs != (1 if args.mode == "smoke" else 128):
        fail("smoke requires one pair and formal requires 128 pairs")
    if sha256(args.source_database) != CARD_DB_HASH:
        fail("card database hash mismatch")
    models = dict(args.models)
    identities = {label: load_store_identity(root, args.generation) for label, root in models.items()}
    if len({identity["store_files"]["identity_bundle_sha256"] for identity in identities.values()}) != 1:
        fail("population Store models do not share one identity-bundle root")
    chunks = chunk_ranges(args.pair_start, args.pairs, args.task_pairs)
    task_plan = planned_tasks(list(identities), chunks)
    args.evidence_root.mkdir(parents=True)
    plan_path = args.evidence_root / "panel-plan.json"
    launch_plan = build_launch_plan(args, identities, task_plan)
    exclusive_write(plan_path, canonical_json_bytes(launch_plan, indent=2))
    (args.evidence_root / "tasks").mkdir()
    workers: list[Path] = []
    for worker in range(args.workers):
        root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True)
        target = root / "cards.h2.mv.db"
        shutil.copyfile(args.source_database, target)
        if sha256(target) != CARD_DB_HASH:
            fail("worker card database copy hash mismatch")
        workers.append(root)
    leases = DatabaseLeasePool(workers)
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = [executor.submit(run_task, args, leases, task["label"],
                                   identities[task["label"]], task["first_pair"],
                                   task["pair_count"])
                   for task in task_plan]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda row: (row["first_pair"], row["label"]))
    completed = [(row["label"], row["first_pair"], row["pair_count"]) for row in results]
    expected = [(task["label"], task["first_pair"], task["pair_count"])
                for task in task_plan]
    if completed != expected:
        fail("completed shard coverage differs from the create-new launch plan")

    pair_results: dict[tuple[str, int], dict[str, Any]] = {}
    for result in results:
        validation = validate_outcome_shard(
            Path(result["outcome"]), identities[result["label"]],
            base_seed=args.base_seed, first_pair=result["first_pair"],
            pair_count=result["pair_count"],
        )
        result["validation"] = {
            key: value for key, value in validation.items()
            if key not in {"outcomes", "environment_seeds"}
        }
        for outcome in validation["outcomes"]:
            key = (result["label"], outcome["pair_index"])
            row = pair_results.setdefault(key, {
                "label": result["label"], "pair_index": outcome["pair_index"],
                "environment_seed": outcome["environment_seed"], "by_seat": {},
            })
            if (row["environment_seed"] != outcome["environment_seed"]
                    or outcome["seat"] in row["by_seat"]):
                fail("duplicate terminal or pair environment seed mismatch across shards")
            row["by_seat"][outcome["seat"]] = outcome["result"]
    pairs = list(range(args.pair_start, args.pair_start + args.pairs))
    expected_pair_keys = {(label, pair) for label in identities for pair in pairs}
    if set(pair_results) != expected_pair_keys or any(
        set(row["by_seat"]) != {"p0", "p1"} for row in pair_results.values()
    ):
        fail("parsed model, pair, or seat coverage is incomplete")
    terminal_results = sorted(pair_results.values(),
                              key=lambda row: (row["pair_index"], row["label"]))
    summary = aggregate_terminal_wdl(terminal_results, sorted(identities))
    for pair in pairs:
        seeds = {row["environment_seed"] for row in terminal_results
                 if row["pair_index"] == pair}
        if len(seeds) != 1:
            fail(f"models did not share pair environment seed for pair {pair}")
    manifest = {
        "schema": "mtg-kernel-population-store-cp7-panel/v2", "mode": args.mode,
        "base_seed": args.base_seed, "pair_start": args.pair_start, "pairs": args.pairs,
        "workers": args.workers, "task_pairs": args.task_pairs,
        "plan": {"path": str(plan_path.resolve()), "sha256": sha256(plan_path)},
        "tasks": results, "terminal_wdl": summary,
        "non_claims": ["terminal win/loss/draw is the only playing-strength outcome",
                       "this external CP7 anchor panel is not a promotion or professional-level claim"],
    }
    output = args.evidence_root / "panel-summary.json"
    exclusive_write(output, canonical_json_bytes(manifest, indent=2))
    return manifest


def self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "store"; (root / "checkpoints").mkdir(parents=True)
        def write_json(path: Path, value: dict[str, Any]) -> None:
            with path.open("w", encoding="utf-8", newline="\n") as handle:
                handle.write(json.dumps(value, sort_keys=True) + "\n")
        generation = 1024; state = b"state"; state_sha = hashlib.sha256(state).hexdigest()
        run = {"schema": "mtg_kernel_native_train_run/v2", "store_identity": "mtg-kernel-native-training-store-v2",
               "contracts": {"identity_bundle_sha256": "a" * 64,
                             "learner_sampler": {"identity": SAMPLER_IDENTITY, "contract_sha256": SAMPLER_CONTRACT}}}
        write_json(root / "run.json", run)
        run_sha = sha256(root / "run.json")
        checkpoint = {"schema": "mtg_kernel_native_train_checkpoint/v3", "generation_index": generation,
                      "identity_bundle_sha256": "a" * 64, "run_sha256": run_sha,
                      "payload": {"byte_count": 5, "sha256": state_sha,
                                  "sections": [{"sha256": "1" * 64}, {"sha256": "2" * 64}, {"sha256": "3" * 64}]},
                      "train_state": {"state_sha256": "4" * 64, "model_parameter_sha256": "5" * 64}}
        checkpoint_path = root / "checkpoints" / "update-00001024.checkpoint.json"
        write_json(checkpoint_path, checkpoint)
        checkpoint_sha = sha256(checkpoint_path)
        sidecar = {"schema": "mtg_kernel_native_train_checkpoint_sidecar/v2", "generation_index": generation,
                   "run_sha256": run_sha, "checkpoint_manifest_sha256": checkpoint_sha,
                   "checkpoint_payload_sha256": state_sha, "train_state_sha256": "4" * 64,
                   "model_parameter_sha256": "5" * 64}
        write_json(root / "checkpoints" / "update-00001024.sidecar.json", sidecar)
        (root / "checkpoints" / "update-00001024.state.f32le").write_bytes(state)
        write_json(root / "latest.json", {"schema": "mtg_kernel_native_train_latest/v2", "generation_index": generation + 4, "run_sha256": run_sha, "identity_bundle_sha256": "a" * 64})
        identity = load_store_identity(root, generation)
        assert identity["checkpoint"]["loaded_generation"] == generation
        assert identity["store_files"]["store_head_generation"] == generation + 4
        try:
            load_store_identity(root, generation + 4)
            raise AssertionError("missing generation was accepted")
        except ValueError:
            pass
        aggregate = aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        assert aggregate["a"]["overall_wdl"] == {"win": 1, "draw": 2, "loss": 1}
    print("PASS retained population Store generation and missing-generation rejection")
    return 0


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--self-test", action="store_true")
    value.add_argument("--evidence-root", type=Path)
    value.add_argument("--model", action="append", dest="model_specs", default=[])
    value.add_argument("--generation", type=int)
    value.add_argument("--mode", choices=("smoke", "formal"))
    value.add_argument("--base-seed", type=int)
    value.add_argument("--pair-start", type=int, default=0)
    value.add_argument("--pairs", type=int)
    value.add_argument("--workers", type=int, choices=(1, 8), default=8)
    value.add_argument("--task-pairs", type=int, default=32)
    value.add_argument("--task-timeout-seconds", type=int, default=1800)
    value.add_argument("--scorer-exe", type=Path)
    value.add_argument("--mage-repo", type=Path)
    value.add_argument("--source-database", type=Path)
    value.add_argument("--maven", type=Path)
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.self_test:
        return self_test()
    required = (args.evidence_root, args.generation, args.mode, args.base_seed,
                args.pairs, args.scorer_exe, args.mage_repo, args.source_database, args.maven)
    if (any(value is None for value in required)
            or not 0 <= args.base_seed <= 0x7FFF_FFFF_FFFF_FFFF
            or args.pair_start < 0 or not 1 <= args.task_pairs <= 128
            or args.task_timeout_seconds < 60):
        fail("missing or invalid panel arguments")
    parsed: list[tuple[str, Path]] = []
    for spec in args.model_specs:
        if spec.count("=") != 1:
            fail("model must be label=STORE_ROOT")
        label, raw_root = spec.split("=", 1)
        if not label or not label.replace("-", "").replace("_", "").isalnum():
            fail("model label is invalid")
        parsed.append((label, Path(raw_root)))
    if len(parsed) != 3 or len({label for label, _ in parsed}) != 3:
        fail("exactly three uniquely labelled models are required")
    if len({root.resolve() for _, root in parsed}) != 3:
        fail("three distinct population Store roots are required")
    args.models = parsed
    run(args)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"panel runner failed: {error}", file=sys.stderr)
        raise SystemExit(2)
