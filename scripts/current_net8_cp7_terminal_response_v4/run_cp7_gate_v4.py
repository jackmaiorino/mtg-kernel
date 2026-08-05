#!/usr/bin/env python3
"""Run the frozen V4 candidate versus GAE8 XMage CP7 skill-7 gate."""

from __future__ import annotations

import argparse
import concurrent.futures
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import threading
import time
from types import SimpleNamespace
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))
import collect_corpus_v4 as common  # noqa: E402
import outcome_v2 as outcome  # noqa: E402


MANIFEST_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-cp7-manifest/v1"
REPORT_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-cp7-result/v1"
STATE_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-cp7-state/v1"
PACKAGE_SCHEMA = "mtg-kernel-xmage-fixed-native-state/v1"
ARM_ORDER = ("candidate", "baseline")
PANEL = {
    "base_seed": 1_840_001,
    "pair_start": 0,
    "pairs": 128,
    "episodes_per_arm": 256,
    "cp7_skill": 7,
}
GATES = {
    "terminal_order_net_floor": 4,
    "win_margin_floor": 4,
    "p0_terminal_order_net_floor": -2,
    "p1_terminal_order_net_floor": -2,
}
TOPOLOGY = {
    "workers": 8,
    "task_pairs": 32,
    "task_timeout_seconds": 7_200,
    "gpu_ordinal_reserved": 1,
    "workload_device": "cpu",
}
POOL3_REPORT_SHA256 = "0d38b7b871b3392f9dce7f64f0fe34de4e26a6398548859d79b5d85ca71ebf64"
SCORER_SHA256 = "222c6c5b95b88bfec13efe5a0f485e73168688160bb626922d1aa45bf457cc9a"
SCORER_SOURCE_GIT_COMMIT = "ac572249aff4c6c0499ed2262296e21d5094bb79"
LINKER_FILE_VERSION = "14.50.35725.0"


CANDIDATE_MANIFEST: dict[str, Any] = {
    "schema": PACKAGE_SCHEMA,
    "authority_kind": "current-net8-cp7-terminal-response-v4-kl-0.3",
    "source_result_sha256": "08258904de7c7892241283773e6e867e35838983d63c23b2794a803950c5cb3f",
    "payload": {
        "filename": "checkpoint.state.f32le",
        "byte_count": 14_771_928,
        "adam_step": 4,
        "scorer_bias_anchor_f32_bits": 3_141_403_366,
        "payload_sha256": "caa483bb1a5ccd86037f21f8fdb4aeb0c8f3dd4fa5ded552b57ee78312d168b8",
        "parameters_sha256": "ea15f7111606f12926d89e466acae4ce2c3dc2de72b789e7b7a89adc380d8613",
        "first_moments_sha256": "102acc11cca7f2ce1024ca2cc8f0a04a30c176ec6e4052aa9cbd13f0b760b9b7",
        "second_moments_sha256": "8b4606b514dc0b89e509c0322921101afb01fdbf2de224b2762caee01c92c7d0",
        "model_parameter_sha256": "ac3b21fde5d71619144ef80b8440900527b266f3b2680e276b23b9a015349d1e",
        "native_state_sha256": "fdc13f19df23c6c26c169c2878def3a0d31be00b5cc9ed81bdcf4d4cc3811388",
    },
    "non_claims": [
        "external software anchor is not professional-level evidence",
        "terminal win/loss/draw is the only playing-strength outcome",
    ],
}

EXPECTED_PACKAGE_MANIFESTS = {
    "candidate": CANDIDATE_MANIFEST,
    "baseline": outcome.EXPECTED_PACKAGE_MANIFEST,
}
EXPECTED_MANIFEST_SHA256 = {
    "candidate": "c2d3c258492dc0ed328d5f39cd1e4d817dd44c0fe1854ee21a29a47070fed043",
    "baseline": outcome.PACKAGE_MANIFEST_SHA256,
}


def fail(message: str) -> None:
    raise ValueError(message)


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        fail(f"{path} is not a JSON object")
    return value


def _require_clean_worktree(repo: Path, label: str) -> None:
    completed = subprocess.run(
        [
            "git",
            "-c",
            "safe.directory=" + repo.as_posix(),
            "-C",
            str(repo),
            "status",
            "--porcelain",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    if completed.stdout.strip():
        fail(f"{label} worktree is not clean")


def _runtime_dependencies(maven: Path) -> dict[str, dict[str, Any]]:
    paths = {
        "collector": SCRIPT_DIR / "collect_corpus_v4.py",
        "outcome_validator": SCRIPT_DIR / "outcome_v2.py",
        "maven": maven,
    }
    return {
        name: {
            "path": str(path.resolve()),
            "sha256": common.sha256(path),
            "byte_count": path.stat().st_size,
        }
        for name, path in paths.items()
    }


def _absolute_path(value: Any, field: str) -> Path:
    if not isinstance(value, str) or not value:
        fail(f"manifest {field} path is absent")
    path = Path(value)
    if not path.is_absolute():
        fail(f"manifest {field} path is not absolute")
    return path.resolve(strict=False)


def expected_checkpoint(arm: str) -> dict[str, Any]:
    manifest = EXPECTED_PACKAGE_MANIFESTS[arm]
    payload = manifest["payload"]
    return {
        "authority_kind": manifest["authority_kind"],
        **outcome.SOURCE_IDENTITY,
        "loaded_run_sha256": outcome.SOURCE_IDENTITY["source_run_sha256"],
        "loaded_generation": payload["adam_step"],
        "loaded_checkpoint_sha256": EXPECTED_MANIFEST_SHA256[arm],
        "loaded_payload_sha256": payload["payload_sha256"],
        "loaded_train_state_sha256": payload["native_state_sha256"],
        "model_parameter_sha256": payload["model_parameter_sha256"],
        "environment_trajectory_contract": "environment-randomization-v2",
        "sampler_identity": "f32-q8-expq63-hamilton-splitmix64-v1",
        "sampler_contract_sha256": "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6",
    }


def load_exact_package(root: Path, arm: str) -> dict[str, Any]:
    if not root.is_dir():
        fail(f"{arm} package root is absent: {root}")
    inventory = sorted(path.name for path in root.iterdir())
    if inventory != ["checkpoint.state.f32le", "fixed_native_state.json"]:
        fail(f"{arm} package inventory mismatch: {inventory}")
    manifest_path = root / "fixed_native_state.json"
    payload_path = root / "checkpoint.state.f32le"
    raw = manifest_path.read_bytes()
    if not raw.endswith(b"\n") or b"\r" in raw:
        fail(f"{arm} manifest is not canonical LF text")
    if hashlib.sha256(raw).hexdigest() != EXPECTED_MANIFEST_SHA256[arm]:
        fail(f"{arm} manifest SHA-256 mismatch")
    try:
        manifest = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"{arm} manifest is invalid JSON: {error}")
    if manifest != EXPECTED_PACKAGE_MANIFESTS[arm]:
        fail(f"{arm} manifest semantic identity mismatch")
    payload = manifest["payload"]
    if (
        payload_path.stat().st_size != payload["byte_count"]
        or common.sha256(payload_path) != payload["payload_sha256"]
    ):
        fail(f"{arm} payload identity mismatch")
    return {
        "root": str(root.resolve()),
        "inventory": inventory,
        "authority_kind": manifest["authority_kind"],
        "manifest_sha256": EXPECTED_MANIFEST_SHA256[arm],
        "payload_sha256": payload["payload_sha256"],
        "payload_bytes": payload["byte_count"],
        "adam_step": payload["adam_step"],
        "train_state_sha256": payload["native_state_sha256"],
        "model_parameter_sha256": payload["model_parameter_sha256"],
        "source_result_sha256": manifest["source_result_sha256"],
        "checkpoint": expected_checkpoint(arm),
    }


def _validate_pool3_prerequisite(path: Path, claimed_sha256: str) -> dict[str, Any]:
    if not path.is_file() or claimed_sha256 != POOL3_REPORT_SHA256:
        fail("Pool3 prerequisite path or declared SHA-256 mismatch")
    if common.sha256(path) != POOL3_REPORT_SHA256:
        fail("Pool3 prerequisite bytes mismatch")
    report = load_json(path)
    if (
        report.get("schema")
        != "mtg-kernel-current-net8-cp7-terminal-response-v4-pool3-result/v1"
        or report.get("mode") != "formal-pool3"
        or report.get("status") != "pass"
        or report.get("panel", {}).get("base_seed") != 1_830_001
        or report.get("comparison", {}).get("pass") is not True
        or report.get("comparison", {}).get("terminal_order", {}).get("nets")
        != {"overall": -8, "p0": -3, "p1": -5}
    ):
        fail("Pool3 prerequisite semantics mismatch")
    return {"path": str(path.resolve()), "sha256": claimed_sha256}


def build_manifest(
    *,
    evidence_root: Path,
    kernel_repo: Path,
    mage_repo: Path,
    scorer: Path,
    source_database: Path,
    maven: Path,
    candidate_root: Path,
    baseline_root: Path,
    pool3_report: Path,
) -> dict[str, Any]:
    if evidence_root.exists():
        fail(f"formal CP7 evidence root already exists: {evidence_root}")
    _require_clean_worktree(kernel_repo, "kernel")
    _require_clean_worktree(mage_repo, "Mage")
    packages = {
        "candidate": load_exact_package(candidate_root, "candidate"),
        "baseline": load_exact_package(baseline_root, "baseline"),
    }
    if common.sha256(scorer) != SCORER_SHA256:
        fail("repaired XMage scorer SHA-256 mismatch")
    if common.sha256(source_database) != common.CARD_DATABASE_SHA256:
        fail("source card database SHA-256 mismatch")
    pool3 = _validate_pool3_prerequisite(pool3_report, POOL3_REPORT_SHA256)
    tool_args = SimpleNamespace(maven=maven, linker_file_version=LINKER_FILE_VERSION)
    return {
        "schema": MANIFEST_SCHEMA,
        "kernel_git_commit": common._git_commit(kernel_repo),
        "mage_git_commit": common._git_commit(mage_repo),
        "toolchain": common._toolchain(tool_args),
        "runner_sha256": common.sha256(Path(__file__).resolve()),
        "runtime_dependencies": _runtime_dependencies(maven),
        "scorer": {
            "path": str(scorer.resolve()),
            "sha256": SCORER_SHA256,
            "source_git_commit": SCORER_SOURCE_GIT_COMMIT,
        },
        "source_database": {
            "path": str(source_database.resolve()),
            "sha256": common.CARD_DATABASE_SHA256,
        },
        "mage_repo": str(mage_repo.resolve()),
        "arms": packages,
        "panel": dict(PANEL),
        "gates": dict(GATES),
        "topology": dict(TOPOLOGY),
        "prerequisites": {"pool3_report": pool3},
        "outputs": {
            "evidence_root": str(evidence_root.resolve()),
            "report_path": str((evidence_root / "report.json").resolve()),
        },
        "analysis_policy": {
            "outcomes_parsed_only_after_all_tasks_complete": True,
            "terminal_win_draw_loss_only": True,
        },
        "nonclaims": [
            "CP7 skill 7 is not a professional player",
            "this development gate is not promotion-grade strength evidence",
        ],
    }


def validate_manifest(path: Path) -> dict[str, Any]:
    manifest = load_json(path)
    if manifest.get("schema") != MANIFEST_SCHEMA:
        fail("CP7 gate manifest schema mismatch")
    if manifest.get("panel") != PANEL or manifest.get("gates") != GATES:
        fail("CP7 gate panel or gates mismatch")
    if manifest.get("topology") != TOPOLOGY:
        fail("CP7 gate topology mismatch")
    if manifest.get("analysis_policy") != {
        "outcomes_parsed_only_after_all_tasks_complete": True,
        "terminal_win_draw_loss_only": True,
    }:
        fail("CP7 gate analysis policy mismatch")
    kernel_repo = SCRIPT_DIR.parents[1]
    _require_clean_worktree(kernel_repo, "kernel")
    if manifest.get("kernel_git_commit") != common._git_commit(kernel_repo):
        fail("CP7 gate kernel commit mismatch")
    if manifest.get("runner_sha256") != common.sha256(Path(__file__).resolve()):
        fail("CP7 gate runner SHA-256 mismatch")
    mage_repo = _absolute_path(manifest.get("mage_repo"), "Mage repository")
    _require_clean_worktree(mage_repo, "Mage")
    if not mage_repo.is_dir() or manifest.get("mage_git_commit") != common._git_commit(mage_repo):
        fail("CP7 gate Mage commit mismatch")
    scorer = _absolute_path(manifest.get("scorer", {}).get("path"), "scorer")
    if (
        not scorer.is_file()
        or manifest.get("scorer", {}).get("sha256") != SCORER_SHA256
        or manifest.get("scorer", {}).get("source_git_commit")
        != SCORER_SOURCE_GIT_COMMIT
        or common.sha256(scorer) != SCORER_SHA256
    ):
        fail("CP7 gate scorer binding mismatch")
    source_database = _absolute_path(
        manifest.get("source_database", {}).get("path"), "source database"
    )
    if (
        not source_database.is_file()
        or manifest.get("source_database", {}).get("sha256")
        != common.CARD_DATABASE_SHA256
        or common.sha256(source_database) != common.CARD_DATABASE_SHA256
    ):
        fail("CP7 gate database binding mismatch")
    dependencies = manifest.get("runtime_dependencies")
    if not isinstance(dependencies, dict) or set(dependencies) != {
        "collector",
        "outcome_validator",
        "maven",
    }:
        fail("CP7 gate runtime dependency inventory mismatch")
    if any(
        not isinstance(record, dict) or set(record) != {"path", "sha256", "byte_count"}
        for record in dependencies.values()
    ):
        fail("CP7 gate runtime dependency binding is malformed")
    maven = _absolute_path(dependencies["maven"].get("path"), "Maven")
    expected_dependencies = _runtime_dependencies(maven)
    if dependencies != expected_dependencies:
        fail("CP7 gate runtime dependency binding mismatch")
    arms = manifest.get("arms")
    if not isinstance(arms, dict) or set(arms) != set(ARM_ORDER):
        fail("CP7 gate arm inventory mismatch")
    packages = {}
    for arm in ARM_ORDER:
        declared = arms[arm]
        if not isinstance(declared, dict):
            fail(f"CP7 gate {arm} binding is malformed")
        root = _absolute_path(declared.get("root"), f"{arm} package")
        package = load_exact_package(root, arm)
        if declared != package:
            fail(f"CP7 gate {arm} package binding mismatch")
        packages[arm] = package
    prerequisites = manifest.get("prerequisites")
    if not isinstance(prerequisites, dict) or set(prerequisites) != {"pool3_report"}:
        fail("CP7 gate prerequisite inventory mismatch")
    pool3_record = prerequisites["pool3_report"]
    if not isinstance(pool3_record, dict) or set(pool3_record) != {"path", "sha256"}:
        fail("CP7 gate Pool3 prerequisite binding is malformed")
    pool3 = _validate_pool3_prerequisite(
        _absolute_path(pool3_record["path"], "Pool3 report"),
        pool3_record["sha256"],
    )
    outputs = manifest.get("outputs")
    if not isinstance(outputs, dict) or set(outputs) != {"evidence_root", "report_path"}:
        fail("CP7 gate output binding is malformed")
    root = _absolute_path(outputs["evidence_root"], "evidence root")
    report_path = _absolute_path(outputs["report_path"], "report")
    if path.resolve() != root / "manifest.json" or report_path != root / "report.json":
        fail("CP7 gate output paths are not canonical")
    toolchain = manifest.get("toolchain")
    expected_toolchain = common._toolchain(
        SimpleNamespace(maven=maven, linker_file_version=LINKER_FILE_VERSION)
    )
    if toolchain != expected_toolchain:
        fail("CP7 gate toolchain binding is incomplete")
    return {
        "manifest": manifest,
        "root": root,
        "report_path": report_path,
        "mage_repo": mage_repo,
        "scorer": scorer,
        "source_database": source_database,
        "maven": maven,
        "packages": packages,
        "pool3": pool3,
    }


def _chunk_ranges() -> list[tuple[int, int]]:
    return [
        (first, min(TOPOLOGY["task_pairs"], PANEL["pair_start"] + PANEL["pairs"] - first))
        for first in range(
            PANEL["pair_start"],
            PANEL["pair_start"] + PANEL["pairs"],
            TOPOLOGY["task_pairs"],
        )
    ]


def _prepare_workers(validated: dict[str, Any]) -> list[Path]:
    roots = []
    for worker in range(TOPOLOGY["workers"]):
        root = validated["root"] / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True)
        destination = root / "cards.h2.mv.db"
        shutil.copyfile(validated["source_database"], destination)
        if common.sha256(destination) != common.CARD_DATABASE_SHA256:
            fail(f"worker {worker} database copy mismatch")
        roots.append(root)
    return roots


def _run_task(
    args: SimpleNamespace,
    arm: str,
    package: dict[str, Any],
    first_pair: int,
    pair_count: int,
    worker_slots: queue.Queue[tuple[int, Path]],
    active_processes: dict[int, subprocess.Popen[Any]],
    active_lock: threading.Lock,
) -> dict[str, Any]:
    worker, database_root = worker_slots.get()
    process: subprocess.Popen[Any] | None = None
    try:
        stem = f"{arm}-p{first_pair:06d}-n{pair_count:03d}"
        log_path = args.evidence_root / "tasks" / f"{stem}.log"
        outcome_path = args.evidence_root / "tasks" / f"{stem}.outcome.jsonl"
        if log_path.exists() or outcome_path.exists():
            fail(f"task output already exists: {stem}")
        started = time.perf_counter()
        with log_path.open("x", encoding="utf-8", newline="\n") as log:
            process = subprocess.Popen(
                common._anchor_command(args, package, first_pair, pair_count, outcome_path),
                cwd=args.mage_repo,
                env=common._environment(database_root, package),
                stdout=log,
                stderr=subprocess.STDOUT,
                **common._popen_group_options(),
            )
            with active_lock:
                active_processes[process.pid] = process
            try:
                return_code = process.wait(timeout=TOPOLOGY["task_timeout_seconds"])
            except subprocess.TimeoutExpired:
                common._terminate_process_tree(process)
                fail(f"task {stem} exceeded timeout")
        elapsed = time.perf_counter() - started
        if return_code != 0:
            fail(f"task {stem} exited {return_code}; see {log_path}")
        if not outcome_path.is_file():
            fail(f"task {stem} did not create its outcome shard")
        return {
            "arm": arm,
            "worker": worker,
            "first_pair": first_pair,
            "pair_count": pair_count,
            "game_count": pair_count * 2,
            "elapsed_seconds": elapsed,
            "log": {
                "path": str(log_path.resolve()),
                "sha256": common.sha256(log_path),
                "byte_count": log_path.stat().st_size,
            },
            "outcome": {
                "path": str(outcome_path.resolve()),
                "sha256": common.sha256(outcome_path),
                "byte_count": outcome_path.stat().st_size,
            },
        }
    finally:
        if process is not None:
            with active_lock:
                active_processes.pop(process.pid, None)
            if process.poll() is None:
                common._terminate_process_tree(process)
        worker_slots.put((worker, database_root))


def _validate_outcome_shard(
    path: Path,
    *,
    arm: str,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    checkpoint = expected_checkpoint(arm)
    expected_header = copy.deepcopy(outcome.EXPECTED_HEADER)
    expected_header["checkpoint"] = checkpoint
    base_seed_hex = f"{PANEL['base_seed']:016x}"
    digest = hashlib.sha256()
    record_count = 0
    decision_ordinal = 0
    expected_episode = first_pair * 2
    active_episode: int | None = None
    active_first_decision: int | None = None
    active_decision_count = 0
    terminals: dict[tuple[int, str], dict[str, Any]] = {}
    environment_seeds: dict[int, str] = {}
    with path.open("rb") as handle:
        for line_number, raw in enumerate(handle, 1):
            digest.update(raw)
            if not raw.endswith(b"\n") or b"\r" in raw or raw == b"\n":
                fail(f"{path}:{line_number}: noncanonical JSONL row")
            try:
                row = json.loads(raw)
            except (UnicodeDecodeError, json.JSONDecodeError) as error:
                fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(row, dict):
                fail(f"{path}:{line_number}: row is not an object")
            ordinal = record_count
            record_count += 1
            if ordinal == 0:
                if row != expected_header:
                    fail(f"{path}: outcome header identity mismatch")
                continue
            if row.get("record_ordinal") != ordinal or row.get("checkpoint") != checkpoint:
                fail(f"{path}: record ordinal or checkpoint mismatch at {ordinal}")
            pair = row.get("pair_index")
            seat = row.get("candidate_seat")
            episode = row.get("episode_id")
            if (
                not isinstance(pair, int)
                or isinstance(pair, bool)
                or not first_pair <= pair < first_pair + pair_count
                or seat not in ("p0", "p1")
                or episode != pair * 2 + int(seat == "p1")
                or row.get("base_seed_u64_hex") != base_seed_hex
                or not isinstance(row.get("pair_environment_seed_u64_hex"), str)
                or outcome.HEX_16.fullmatch(row["pair_environment_seed_u64_hex"]) is None
                or row.get("deck_ids") != ["Rally", "Rally"]
                or row.get("randomization_identity") != "environment-randomization-v2"
            ):
                fail(f"{path}: pair receipt mismatch at {ordinal}")
            if episode != expected_episode:
                fail(f"{path}: episode rows are reordered or interleaved")
            environment_seed = row["pair_environment_seed_u64_hex"]
            if pair in environment_seeds and environment_seeds[pair] != environment_seed:
                fail(f"{path}: environment seed changed within pair {pair}")
            environment_seeds[pair] = environment_seed
            if row.get("record_type") == "decision":
                outcome._validate_decision(path, row, ordinal)
                if active_episode is None:
                    active_episode = episode
                    active_first_decision = decision_ordinal
                    active_decision_count = 0
                elif active_episode != episode:
                    fail(f"{path}: decisions are interleaved")
                if row.get("outcome_decision_ordinal") != decision_ordinal:
                    fail(f"{path}: decision ordinal mismatch")
                decision_ordinal += 1
                active_decision_count += 1
            elif row.get("record_type") == "terminal":
                outcome._validate_terminal(path, row, ordinal)
                if active_episode is None:
                    active_episode = episode
                    active_first_decision = None
                    active_decision_count = 0
                if (
                    active_episode != episode
                    or row.get("first_outcome_decision_ordinal") != active_first_decision
                    or row.get("outcome_decision_count") != active_decision_count
                ):
                    fail(f"{path}: terminal decision range mismatch")
                key = (pair, seat)
                if key in terminals:
                    fail(f"{path}: duplicate terminal {key}")
                terminals[key] = {
                    "pair_index": pair,
                    "episode_id": episode,
                    "candidate_seat": seat,
                    "base_seed_u64_hex": row["base_seed_u64_hex"],
                    "pair_environment_seed_u64_hex": environment_seed,
                    "deck_ids": row["deck_ids"],
                    "randomization_identity": row["randomization_identity"],
                    "candidate_terminal_reward": row["candidate_terminal_reward"],
                }
                expected_episode += 1
                active_episode = None
                active_first_decision = None
                active_decision_count = 0
            else:
                fail(f"{path}: unknown record type at {ordinal}")
    expected_keys = {
        (pair, seat)
        for pair in range(first_pair, first_pair + pair_count)
        for seat in ("p0", "p1")
    }
    if (
        record_count == 0
        or active_episode is not None
        or set(terminals) != expected_keys
        or set(environment_seeds) != set(range(first_pair, first_pair + pair_count))
    ):
        fail(f"{path}: terminal inventory mismatch")
    return {
        "sha256": digest.hexdigest(),
        "byte_count": path.stat().st_size,
        "record_count": record_count,
        "decision_count": decision_ordinal,
        "episode_count": len(terminals),
        "terminals": terminals,
    }


def adjudicate(
    candidate: dict[tuple[int, str], dict[str, Any]],
    baseline: dict[tuple[int, str], dict[str, Any]],
) -> dict[str, Any]:
    if set(candidate) != set(baseline):
        fail("CP7 arm terminal inventories differ")
    overall = {"candidate_better": 0, "baseline_better": 0, "equal": 0}
    seats = {
        "p0": {"candidate_better": 0, "baseline_better": 0, "equal": 0},
        "p1": {"candidate_better": 0, "baseline_better": 0, "equal": 0},
    }
    wins = {
        "candidate": {"overall": 0, "p0": 0, "p1": 0},
        "baseline": {"overall": 0, "p0": 0, "p1": 0},
    }
    for key in sorted(candidate):
        candidate_row = candidate[key]
        baseline_row = baseline[key]
        for field in (
            "pair_index",
            "episode_id",
            "candidate_seat",
            "base_seed_u64_hex",
            "pair_environment_seed_u64_hex",
            "deck_ids",
            "randomization_identity",
        ):
            if candidate_row.get(field) != baseline_row.get(field):
                fail(f"matched CP7 receipt differs at {key}: {field}")
        seat = key[1]
        candidate_reward = candidate_row["candidate_terminal_reward"]
        baseline_reward = baseline_row["candidate_terminal_reward"]
        wins["candidate"]["overall"] += int(candidate_reward == 1)
        wins["candidate"][seat] += int(candidate_reward == 1)
        wins["baseline"]["overall"] += int(baseline_reward == 1)
        wins["baseline"][seat] += int(baseline_reward == 1)
        bucket = (
            "candidate_better"
            if candidate_reward > baseline_reward
            else "baseline_better"
            if candidate_reward < baseline_reward
            else "equal"
        )
        overall[bucket] += 1
        seats[seat][bucket] += 1
    nets = {
        "overall": overall["candidate_better"] - overall["baseline_better"],
        "p0": seats["p0"]["candidate_better"] - seats["p0"]["baseline_better"],
        "p1": seats["p1"]["candidate_better"] - seats["p1"]["baseline_better"],
    }
    win_margin = wins["candidate"]["overall"] - wins["baseline"]["overall"]
    gates = {
        "terminal_order_net_floor": nets["overall"] >= GATES["terminal_order_net_floor"],
        "win_margin_floor": win_margin >= GATES["win_margin_floor"],
        "p0_terminal_order_net_floor": nets["p0"] >= GATES["p0_terminal_order_net_floor"],
        "p1_terminal_order_net_floor": nets["p1"] >= GATES["p1_terminal_order_net_floor"],
    }
    return {
        "terminal_order": {"overall": overall, "by_candidate_seat": seats, "nets": nets},
        "wins": wins,
        "win_margin": win_margin,
        "gates": gates,
        "pass": all(gates.values()),
    }


def _validate_task_log(task: dict[str, Any]) -> None:
    path = Path(task["log"]["path"])
    if (
        not path.is_file()
        or path.stat().st_size != task["log"]["byte_count"]
        or common.sha256(path) != task["log"]["sha256"]
    ):
        fail("CP7 task log changed after collection")
    common._validate_log_markers(
        path,
        base_seed=PANEL["base_seed"],
        first_pair=task["first_pair"],
        pair_count=task["pair_count"],
    )


def _collect(validated: dict[str, Any]) -> tuple[list[dict[str, Any]], list[dict[str, float]], float]:
    args = SimpleNamespace(
        evidence_root=validated["root"],
        mage_repo=validated["mage_repo"],
        scorer_exe=validated["scorer"],
        maven=validated["maven"],
        base_seed=PANEL["base_seed"],
    )
    (validated["root"] / "tasks").mkdir()
    worker_roots = _prepare_workers(validated)
    worker_slots: queue.Queue[tuple[int, Path]] = queue.Queue()
    for worker, root in enumerate(worker_roots):
        worker_slots.put((worker, root))
    tasks = [
        (arm, first_pair, pair_count)
        for first_pair, pair_count in _chunk_ranges()
        for arm in ARM_ORDER
    ]
    active_processes: dict[int, subprocess.Popen[Any]] = {}
    active_lock = threading.Lock()
    samples: list[dict[str, float]] = []
    monitor_errors: list[str] = []
    stop_monitor = threading.Event()
    started = time.perf_counter()
    monitor = threading.Thread(
        target=common._resource_monitor,
        args=(stop_monitor, samples, monitor_errors, started),
        daemon=True,
    )
    monitor.start()
    results: list[dict[str, Any]] = []
    executor = concurrent.futures.ThreadPoolExecutor(max_workers=TOPOLOGY["workers"])
    futures: list[concurrent.futures.Future[dict[str, Any]]] = []
    try:
        futures = [
            executor.submit(
                _run_task,
                args,
                arm,
                validated["packages"][arm],
                first_pair,
                pair_count,
                worker_slots,
                active_processes,
                active_lock,
            )
            for arm, first_pair, pair_count in tasks
        ]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    except BaseException:
        for future in futures:
            future.cancel()
        with active_lock:
            processes = list(active_processes.values())
        for process in processes:
            if process.poll() is None:
                common._terminate_process_tree(process)
        executor.shutdown(wait=True, cancel_futures=True)
        raise
    else:
        executor.shutdown(wait=True)
    finally:
        stop_monitor.set()
        monitor.join(timeout=30)
    if monitor.is_alive() or monitor_errors:
        fail("CP7 resource monitor failed or did not stop")
    elapsed = time.perf_counter() - started
    results.sort(key=lambda row: (row["first_pair"], row["arm"]))
    expected_tasks = sorted(tasks, key=lambda row: (row[1], row[0]))
    actual_tasks = [(row["arm"], row["first_pair"], row["pair_count"]) for row in results]
    if actual_tasks != expected_tasks:
        fail("CP7 task coverage mismatch")
    return results, samples, elapsed


def run(manifest_path: Path) -> dict[str, Any]:
    validated = validate_manifest(manifest_path)
    existing = sorted(path.name for path in validated["root"].iterdir())
    if existing != ["manifest.json"]:
        raise FileExistsError("CP7 evidence root must initially contain only manifest.json")
    results, samples, elapsed = _collect(validated)
    state = {
        "schema": STATE_SCHEMA,
        "manifest_sha256": common.sha256(manifest_path),
        "wall_seconds": elapsed,
        "tasks": results,
        "outcomes_parsed": False,
    }
    state_path = validated["root"] / "state.json"
    common.exclusive_write(state_path, common.canonical_json_bytes(state, indent=2))

    by_arm: dict[str, dict[tuple[int, str], dict[str, Any]]] = {
        arm: {} for arm in ARM_ORDER
    }
    arm_validation: dict[str, dict[str, int]] = {
        arm: {"decision_count": 0, "record_count": 0, "episode_count": 0}
        for arm in ARM_ORDER
    }
    validated_tasks = []
    for task in results:
        outcome_path = Path(task["outcome"]["path"])
        _validate_task_log(task)
        parsed = _validate_outcome_shard(
            outcome_path,
            arm=task["arm"],
            first_pair=task["first_pair"],
            pair_count=task["pair_count"],
        )
        if parsed["sha256"] != task["outcome"]["sha256"]:
            fail("CP7 outcome changed between collection and adjudication")
        for key, terminal in parsed["terminals"].items():
            if key in by_arm[task["arm"]]:
                fail(f"duplicate CP7 terminal across shards: {task['arm']} {key}")
            by_arm[task["arm"]][key] = terminal
        for field in arm_validation[task["arm"]]:
            arm_validation[task["arm"]][field] += parsed[field]
        validated_tasks.append(
            {
                **task,
                "outcome_validation": {
                    key: value for key, value in parsed.items() if key != "terminals"
                },
            }
        )
    expected_keys = {
        (pair, seat)
        for pair in range(PANEL["pair_start"], PANEL["pair_start"] + PANEL["pairs"])
        for seat in ("p0", "p1")
    }
    if any(set(by_arm[arm]) != expected_keys for arm in ARM_ORDER):
        fail("CP7 complete arm inventory mismatch")
    comparison = adjudicate(by_arm["candidate"], by_arm["baseline"])
    sample_path = validated["root"] / "resource_samples.jsonl"
    common.exclusive_write(
        sample_path,
        b"".join(common.canonical_json_bytes(sample) for sample in samples),
    )
    report = {
        "schema": REPORT_SCHEMA,
        "status": "pass" if comparison["pass"] else "fail",
        "manifest": {
            "path": str(manifest_path.resolve()),
            "sha256": common.sha256(manifest_path),
        },
        "prerequisites": validated["manifest"]["prerequisites"],
        "panel": dict(PANEL),
        "gate_config": dict(GATES),
        "topology": dict(TOPOLOGY),
        "wall_seconds": elapsed,
        "achieved_games_per_second": PANEL["episodes_per_arm"] * 2 / elapsed,
        "resource_usage": common._resource_summary(samples),
        "resource_samples": {
            "path": str(sample_path.resolve()),
            "sha256": common.sha256(sample_path),
            "byte_count": sample_path.stat().st_size,
        },
        "arms": {
            arm: {
                "package": validated["packages"][arm],
                **arm_validation[arm],
            }
            for arm in ARM_ORDER
        },
        "tasks": validated_tasks,
        "comparison": comparison,
        "nonclaims": validated["manifest"]["nonclaims"],
    }
    common.exclusive_write(
        validated["report_path"], common.canonical_json_bytes(report, indent=2)
    )
    state["outcomes_parsed"] = True
    state["report_path"] = str(validated["report_path"])
    state["report_sha256"] = common.sha256(validated["report_path"])
    state["state_supersedes_sha256"] = common.sha256(state_path)
    common.exclusive_write(
        validated["root"] / "state-final.json",
        common.canonical_json_bytes(state, indent=2),
    )
    return report


def _self_test() -> int:
    if _chunk_ranges() != [(0, 32), (32, 32), (64, 32), (96, 32)]:
        fail("CP7 task partition self-test failed")
    if expected_checkpoint("candidate")["loaded_checkpoint_sha256"] != EXPECTED_MANIFEST_SHA256["candidate"]:
        fail("CP7 candidate checkpoint self-test failed")
    print("run_cp7_gate_v4: SELF-TEST PASS")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args(argv)
    if args.self_test:
        return _self_test()
    if args.manifest is None:
        fail("--manifest is required")
    report = run(args.manifest)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0 if report["status"] == "pass" else 3


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, TypeError, ValueError, subprocess.SubprocessError) as error:
        print(f"run_cp7_gate_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
