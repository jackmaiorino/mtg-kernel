#!/usr/bin/env python3
"""Collect recurrent-policy CP7 outcomes and build a complete-history cache."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
sys.path.insert(0, str(STRUCTURED_DIR))
import run_screen as structured  # noqa: E402


SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-corpus/v1"
CACHE_SCHEMA = "mtg-kernel-recurrent-terminal-onpolicy-cache/v1"
BASE_SEED = 2_030_001
PAIR_COUNT = 512
WORKERS = 8
TASK_PAIRS = 64
SOURCE_DATABASE_SHA256 = "1defa6420bcf02b0f79c3313e964efce3b401838231e7ffe86c7c7ee6724e0b1"
CANDIDATE_IDENTITY = {
    "adam_step": "1",
    "manifest": "55130977d8e5a4d98060e8e436169356205b4a7e1ba47fe567fde487ad233e50",
    "payload": "6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d",
    "train_state": "d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca",
    "model": "397e2576fe71edba2e31a15da654b219e04318c8fe71be3867e333fdf7989dda",
}


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _atomic_json(path: Path, value: Any) -> None:
    temporary = path.with_name(path.name + ".tmp")
    if temporary.exists():
        _fail(f"temporary path already exists: {temporary}")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    os.replace(temporary, path)


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return completed.stdout.strip().splitlines()[0]


def _task_ranges(pair_count: int = PAIR_COUNT) -> list[tuple[int, int]]:
    return [
        (first, min(TASK_PAIRS, pair_count - first))
        for first in range(0, pair_count, TASK_PAIRS)
    ]


def _environment(database_root: Path, python: Path) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "MAGE_DB_DIR": str(database_root),
            "MAGE_DB_AUTO_SERVER": "false",
            "AI_DETERMINISTIC_TIEBREAKS": "true",
            "AI_DETERMINISTIC_SEARCH": "true",
            "AI_DETERMINISTIC_MAX_NODES": "5000",
            "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
            "MTG_KERNEL_RECURRENT_CP7_PYTHON": str(python),
            "PYTHONDONTWRITEBYTECODE": "1",
            "MAVEN_OPTS": " ".join(
                (
                    "-Dxmage.rally.cp7Outcome.authorityKind=recurrent-cp7-deployment-v1",
                    f"-Dxmage.rally.cp7Outcome.adamStep={CANDIDATE_IDENTITY['adam_step']}",
                    f"-Dxmage.rally.cp7Outcome.manifestSha256={CANDIDATE_IDENTITY['manifest']}",
                    f"-Dxmage.rally.cp7Outcome.payloadSha256={CANDIDATE_IDENTITY['payload']}",
                    f"-Dxmage.rally.cp7Outcome.trainStateSha256={CANDIDATE_IDENTITY['train_state']}",
                    f"-Dxmage.rally.cp7Outcome.modelParameterSha256={CANDIDATE_IDENTITY['model']}",
                )
            ),
        }
    )
    return environment


def _worker_database(args: argparse.Namespace, worker: int) -> Path:
    database_root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
    database_root.mkdir(parents=True)
    destination = database_root / "cards.h2.mv.db"
    if destination.exists():
        _fail(f"worker database already exists: {destination}")
    shutil.copyfile(args.source_database, destination)
    if _sha256(destination) != SOURCE_DATABASE_SHA256:
        _fail(f"worker {worker} database copy mismatch")
    return database_root


def _validate_task(
    teacher_path: Path,
    outcome_path: Path,
    first_pair: int,
    pair_count: int,
) -> tuple[dict[str, Any], list[dict[str, Any]], list[dict[str, Any]]]:
    policy, teacher_terminals = structured._load_teacher(teacher_path)
    value, outcome_terminals = structured._load_outcome(outcome_path)
    expected_pairs = set(range(first_pair, first_pair + pair_count))
    if (
        {int(row["pair_index"]) for row in policy} != expected_pairs
        or {int(row["pair_index"]) for row in value} != expected_pairs
    ):
        _fail(f"task at pair {first_pair} has incomplete pair coverage")
    join = structured._validate_complete_history_join(
        policy, value, teacher_terminals, outcome_terminals
    )
    if (
        int(join["pair_count"]) != pair_count
        or int(join["episode_count"]) != pair_count * 2
        or not all(
            bool(join[key])
            for key in (
                "selected_semantics_public",
                "terminal_replays_exact",
                "complete_policy_steps",
                "complete_physical_decisions",
            )
        )
    ):
        _fail(f"task at pair {first_pair} failed complete-history validation")
    result = {
        "first_pair": first_pair,
        "pair_count": pair_count,
        "teacher_path": str(teacher_path),
        "teacher_sha256": _sha256(teacher_path),
        "teacher_decisions": len(policy),
        "outcome_path": str(outcome_path),
        "outcome_sha256": _sha256(outcome_path),
        "outcome_decisions": len(value),
        "join": join,
    }
    return result, policy, value


def _run_task(
    args: argparse.Namespace,
    worker: int,
    first_pair: int,
    pair_count: int,
    database_root: Path,
) -> dict[str, Any]:
    task_root = args.evidence_root / "tasks"
    stem = f"p{first_pair:06d}-n{pair_count:03d}"
    teacher_path = task_root / f"{stem}.teacher.jsonl"
    outcome_path = task_root / f"{stem}.outcome.jsonl"
    log_path = task_root / f"{stem}.log"
    if any(path.exists() for path in (teacher_path, outcome_path, log_path)):
        _fail(f"task outputs already exist for {stem}")
    exec_args = " ".join(
        (
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(args.scorer_exe),
            "--outcome-root",
            str(args.candidate_root),
            "--base-seed",
            str(BASE_SEED),
            "--first-episode",
            str(first_pair * 2),
            "--pairs",
            str(pair_count),
            "--opponent cp7 --cp7-skill 7",
            "--teacher-export",
            str(teacher_path),
            "--outcome-export",
            str(outcome_path),
        )
    )
    command = [
        str(args.maven),
        "-o",
        "-q",
        "-pl",
        "Mage.Server.Plugins/Mage.Player.AIRL",
        "-DskipTests",
        "exec:java",
        "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
        f"-Dexec.args={exec_args}",
    ]
    started = time.perf_counter()
    with log_path.open("x", encoding="utf-8", newline="\n") as log:
        completed = subprocess.run(
            command,
            cwd=args.mage_repo,
            env=_environment(database_root, args.python),
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=args.task_timeout_seconds,
            check=False,
        )
    if completed.returncode != 0:
        _fail(f"task {stem} exited {completed.returncode}; see {log_path}")
    validated, _, _ = _validate_task(
        teacher_path, outcome_path, first_pair, pair_count
    )
    return {
        "worker": worker,
        "elapsed_seconds": time.perf_counter() - started,
        "log_path": str(log_path),
        "log_sha256": _sha256(log_path),
        **validated,
    }


def _merge_join(target: dict[str, Any], source: dict[str, Any]) -> None:
    for key in ("episode_count", "pair_count", "policy_step_count", "physical_decision_count"):
        target[key] += int(source[key])
    for kind, count in source["selected_action_kind_counts"].items():
        target["selected_action_kind_counts"][kind] = (
            target["selected_action_kind_counts"].get(kind, 0) + int(count)
        )
    for key in (
        "selected_semantics_public",
        "terminal_replays_exact",
        "complete_policy_steps",
        "complete_physical_decisions",
    ):
        target[key] = bool(target[key] and source[key])


def run(args: argparse.Namespace) -> int:
    for path in (
        args.mage_repo,
        args.scorer_exe,
        args.candidate_root,
        args.source_database,
        args.maven,
        args.python,
    ):
        if not path.exists():
            _fail(f"required path does not exist: {path}")
    if args.evidence_root.exists() or args.cache.exists():
        _fail("evidence root and cache must both be new")
    if _sha256(args.source_database) != SOURCE_DATABASE_SHA256:
        _fail("source card database SHA-256 mismatch")
    if _sha256(args.candidate_root / "recurrent_cp7_deployment.json") != CANDIDATE_IDENTITY["manifest"]:
        _fail("candidate package manifest SHA-256 mismatch")

    started = time.perf_counter()
    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    databases = [
        _worker_database(args, worker) for worker in range(WORKERS)
    ]
    ranges = _task_ranges()
    task_results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as executor:
        futures = {
            executor.submit(
                _run_task,
                args,
                worker,
                first_pair,
                pair_count,
                databases[worker],
            ): (first_pair, pair_count)
            for worker, (first_pair, pair_count) in enumerate(ranges)
        }
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            task_results.append(result)
            print(
                json.dumps(
                    {
                        "completed_first_pair": result["first_pair"],
                        "pair_count": result["pair_count"],
                        "elapsed_seconds": result["elapsed_seconds"],
                    },
                    sort_keys=True,
                ),
                flush=True,
            )
    task_results.sort(key=lambda value: int(value["first_pair"]))
    collection_seconds = time.perf_counter() - started

    policy: list[dict[str, Any]] = []
    value: list[dict[str, Any]] = []
    complete_join = {
        "episode_count": 0,
        "pair_count": 0,
        "policy_step_count": 0,
        "physical_decision_count": 0,
        "selected_action_kind_counts": {},
        "selected_semantics_public": True,
        "terminal_replays_exact": True,
        "complete_policy_steps": True,
        "complete_physical_decisions": True,
    }
    for task in task_results:
        validated, task_policy, task_value = _validate_task(
            Path(task["teacher_path"]),
            Path(task["outcome_path"]),
            int(task["first_pair"]),
            int(task["pair_count"]),
        )
        if (
            validated["teacher_sha256"] != task["teacher_sha256"]
            or validated["outcome_sha256"] != task["outcome_sha256"]
        ):
            _fail("task export changed after collection")
        policy.extend(task_policy)
        value.extend(task_value)
        _merge_join(complete_join, task["join"])
    if (
        int(complete_join["pair_count"]) != PAIR_COUNT
        or int(complete_join["episode_count"]) != PAIR_COUNT * 2
        or {int(row["pair_index"]) for row in value} != set(range(PAIR_COUNT))
    ):
        _fail("aggregate cache coverage mismatch")
    card_max = max(int(row["object_card_ids"].max().item()) for row in policy + value)
    group_max = max(int(row["object_groups"].max().item()) for row in policy + value)
    payload = {
        "version": structured.SCRIPT_VERSION,
        "schema": CACHE_SCHEMA,
        "policy": policy,
        "value": value,
        "card_max": card_max,
        "group_max": group_max,
        "complete_history_join": complete_join,
        "source": {
            "schema": SCHEMA,
            "base_seed": BASE_SEED,
            "pairs": PAIR_COUNT,
            "tasks": task_results,
        },
    }
    args.cache.parent.mkdir(parents=True, exist_ok=True)
    temporary_cache = args.cache.with_name(args.cache.name + ".tmp")
    torch.save(payload, temporary_cache)
    os.replace(temporary_cache, args.cache)
    cache_sha256 = _sha256(args.cache)
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": BASE_SEED,
        "pairs": PAIR_COUNT,
        "games": PAIR_COUNT * 2,
        "workers": WORKERS,
        "task_pairs": TASK_PAIRS,
        "collection_seconds": collection_seconds,
        "total_seconds": time.perf_counter() - started,
        "games_per_collection_second": PAIR_COUNT * 2 / collection_seconds,
        "complete_history_join": complete_join,
        "cache": str(args.cache),
        "cache_sha256": cache_sha256,
        "cache_bytes": args.cache.stat().st_size,
        "inputs": {
            "kernel_git_commit": _version(["git", "rev-parse", "HEAD"], SCRIPT_DIR.parents[1]),
            "mage_git_commit": _version(
                ["git", "-c", f"safe.directory={args.mage_repo}", "rev-parse", "HEAD"],
                args.mage_repo,
            ),
            "scorer_exe": str(args.scorer_exe),
            "scorer_sha256": _sha256(args.scorer_exe),
            "candidate_root": str(args.candidate_root),
            "candidate_identity": CANDIDATE_IDENTITY,
            "source_database": str(args.source_database),
            "source_database_sha256": SOURCE_DATABASE_SHA256,
        },
        "toolchain": {
            "python": sys.version.split()[0],
            "torch": torch.__version__,
            "java": _version(["java", "-version"]),
            "maven": _version([str(args.maven), "--version"]),
            "rustc": _version(["rustc", "+1.94.1", "--version"]),
            "gpu_ordinal": None,
        },
        "tasks": task_results,
        "non_claims": [
            "corpus outcomes are training data and not strength evidence",
            "natural terminal win or loss is the only reward",
        ],
    }
    _atomic_json(args.evidence_root / "report.json", report)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


def self_test() -> int:
    ranges = _task_ranges()
    covered = [pair for first, count in ranges for pair in range(first, first + count)]
    if ranges != [(index * 64, 64) for index in range(8)] or covered != list(range(512)):
        _fail("task-range self-test failed")
    print("collect_recurrent_terminal_onpolicy_v1: SELF-TEST PASS")
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--cache", type=Path)
    parser.add_argument(
        "--mage-repo",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1"),
    )
    parser.add_argument(
        "--scorer-exe",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_recurrent_cp7_stdio_v1.exe"
        ),
    )
    parser.add_argument(
        "--candidate-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-cp7-deployment-v1-preflight-02"),
    )
    parser.add_argument(
        "--source-database",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1\db\cards.h2.mv.db"
        ),
    )
    parser.add_argument(
        "--maven",
        type=Path,
        default=Path(r"C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"),
    )
    parser.add_argument(
        "--python",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage\.mtgrl_venv\Scripts\python.exe"),
    )
    parser.add_argument("--task-timeout-seconds", type=int, default=7_200)
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.evidence_root is None or args.cache is None:
        parser.error("--evidence-root and --cache are required")
    if args.task_timeout_seconds < 60:
        parser.error("--task-timeout-seconds must be at least 60")
    return args


if __name__ == "__main__":
    try:
        parsed = arguments()
        raise SystemExit(self_test() if parsed.self_test else run(parsed))
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"collect_recurrent_terminal_onpolicy_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
