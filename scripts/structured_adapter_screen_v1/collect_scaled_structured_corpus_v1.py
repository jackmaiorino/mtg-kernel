#!/usr/bin/env python3
"""Collect the fixed 2,048-pair matched structured corpus with eight workers."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import shutil
import subprocess
import threading
import time
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-scaled-structured-corpus-collector/v1"
BASE_SEED = 1_400_001
PRIMARY_PAIRS = 2_048
WORKERS = 8
TASK_PAIRS = 32
CARD_DB_SHA256 = "20f400b058b8974806b235422d8514e9c5494acb116050823334f0dd21b4c521"
SCORER_SHA256 = "3cfa92c7b96ab984600555ee91192aab0eada633fc69c27204ca7eb07457ddbe"
PARENT_MANIFEST_SHA256 = "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb"
PARENT_PAYLOAD_SHA256 = "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c"
PARENT_TRAIN_STATE_SHA256 = "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8"
PARENT_MODEL_PARAMETER_SHA256 = "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546"


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _atomic_json(path: Path, value: Any) -> None:
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(
        json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    os.replace(temporary, path)


@dataclass(frozen=True)
class Task:
    first_pair: int
    pair_count: int
    kind: str
    replaces_pair: int | None = None


def _task_dict(task: Task) -> dict[str, Any]:
    return asdict(task)


def _task_from_dict(value: dict[str, Any]) -> Task:
    return Task(
        first_pair=int(value["first_pair"]),
        pair_count=int(value["pair_count"]),
        kind=str(value["kind"]),
        replaces_pair=(
            None if value.get("replaces_pair") is None else int(value["replaces_pair"])
        ),
    )


def _next_stem(tasks_root: Path, task: Task) -> str:
    base = f"{task.kind}-p{task.first_pair:06d}-n{task.pair_count:03d}"
    attempt = 1
    while True:
        stem = f"{base}-a{attempt:02d}"
        if not any((tasks_root / f"{stem}.{suffix}").exists() for suffix in ("log", "teacher.jsonl", "outcome.jsonl")):
            return stem
        attempt += 1


def _validate_export(
    path: Path, expected_contract_prefix: str, task: Task
) -> dict[str, Any]:
    header_count = 0
    decision_count = 0
    terminals: set[tuple[int, int, str]] = set()
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            record_type = row.get("record_type")
            if record_type == "header":
                header_count += 1
                if line_number != 1 or not str(row.get("export_contract", "")).startswith(
                    expected_contract_prefix
                ):
                    _fail(f"{path}: invalid export header")
            elif record_type == "decision":
                decision_count += 1
            elif record_type == "terminal":
                pair = row.get("pair_index")
                episode = row.get("episode_id")
                seat = row.get("candidate_seat")
                terminal = row.get("terminal")
                if (
                    not isinstance(pair, int)
                    or not isinstance(episode, int)
                    or seat not in ("p0", "p1")
                    or not isinstance(terminal, dict)
                    or terminal.get("terminal_classification") != "natural"
                    or terminal.get("terminal_code") != "natural_game_over"
                ):
                    _fail(f"{path}: invalid terminal record")
                key = (pair, episode, seat)
                if key in terminals:
                    _fail(f"{path}: duplicate terminal {key}")
                terminals.add(key)
            else:
                _fail(f"{path}: unknown record type {record_type!r}")
    expected = {
        (pair, pair * 2 + seat, f"p{seat}")
        for pair in range(task.first_pair, task.first_pair + task.pair_count)
        for seat in (0, 1)
    }
    if header_count != 1 or terminals != expected or decision_count < 1:
        _fail(f"{path}: incomplete export coverage")
    return {
        "path": str(path),
        "sha256": _sha256(path),
        "bytes": path.stat().st_size,
        "decision_count": decision_count,
        "terminal_count": len(terminals),
    }


def _worker_database(
    evidence_root: Path, worker: int, source_database: Path
) -> Path:
    database_root = evidence_root / "workers" / f"worker-{worker:02d}" / "db"
    database_file = database_root / "cards.h2.mv.db"
    if not database_file.exists():
        database_root.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source_database, database_file)
        if _sha256(database_file) != CARD_DB_SHA256:
            _fail(f"worker {worker} card database copy mismatch")
    return database_root


def _environment(database_root: Path) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "MAGE_DB_DIR": str(database_root),
            "MAGE_DB_AUTO_SERVER": "false",
            "AI_DETERMINISTIC_TIEBREAKS": "true",
            "AI_DETERMINISTIC_SEARCH": "true",
            "AI_DETERMINISTIC_MAX_NODES": "5000",
            "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
            "MAVEN_OPTS": " ".join(
                (
                    "-Dxmage.rally.cp7Outcome.adamStep=1",
                    f"-Dxmage.rally.cp7Outcome.manifestSha256={PARENT_MANIFEST_SHA256}",
                    f"-Dxmage.rally.cp7Outcome.payloadSha256={PARENT_PAYLOAD_SHA256}",
                    f"-Dxmage.rally.cp7Outcome.trainStateSha256={PARENT_TRAIN_STATE_SHA256}",
                    f"-Dxmage.rally.cp7Outcome.modelParameterSha256={PARENT_MODEL_PARAMETER_SHA256}",
                )
            ),
        }
    )
    return environment


def _run_task(
    task: Task,
    worker: int,
    args: argparse.Namespace,
    database_root: Path,
) -> dict[str, Any]:
    tasks_root = args.evidence_root / "tasks"
    tasks_root.mkdir(parents=True, exist_ok=True)
    stem = _next_stem(tasks_root, task)
    log_path = tasks_root / f"{stem}.log"
    teacher_path = tasks_root / f"{stem}.teacher.jsonl"
    outcome_path = tasks_root / f"{stem}.outcome.jsonl"
    exec_args = " ".join(
        (
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(args.scorer_exe),
            "--outcome-root",
            str(args.outcome_root),
            "--base-seed",
            str(BASE_SEED),
            "--first-episode",
            str(task.first_pair * 2),
            "--pairs",
            str(task.pair_count),
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
    started = time.time()
    with log_path.open("x", encoding="utf-8", newline="\n") as log:
        completed = subprocess.run(
            command,
            cwd=args.mage_repo,
            env=_environment(database_root),
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
        )
    result: dict[str, Any] = {
        "task": _task_dict(task),
        "worker": worker,
        "stem": stem,
        "log": str(log_path),
        "log_sha256": _sha256(log_path),
        "return_code": completed.returncode,
        "elapsed_seconds": time.time() - started,
    }
    if completed.returncode != 0:
        result["status"] = "failed"
        return result
    try:
        result["teacher"] = _validate_export(
            teacher_path, "mtg-kernel-xmage-cp7-teacher-jsonl/", task
        )
        result["outcome"] = _validate_export(
            outcome_path, "mtg-kernel-xmage-cp7-outcome-jsonl/", task
        )
    except (OSError, ValueError) as error:
        result["status"] = "failed-validation"
        result["validation_error"] = str(error)
        return result
    result["status"] = "success"
    return result


def _collection_config(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "base_seed": BASE_SEED,
        "primary_pair_count": PRIMARY_PAIRS,
        "workers": WORKERS,
        "task_pairs": TASK_PAIRS,
        "mage_repo": str(args.mage_repo.resolve()),
        "scorer_exe": str(args.scorer_exe.resolve()),
        "scorer_sha256": SCORER_SHA256,
        "outcome_root": str(args.outcome_root.resolve()),
        "source_database": str(args.source_database.resolve()),
        "source_database_sha256": CARD_DB_SHA256,
        "maven": str(args.maven.resolve()),
    }


def _initial_state(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "schema": SCHEMA,
        "config": _collection_config(args),
        "phase": "primary",
        "in_progress": [],
        "pending": [
            _task_dict(Task(first, min(TASK_PAIRS, PRIMARY_PAIRS - first), "primary"))
            for first in range(0, PRIMARY_PAIRS, TASK_PAIRS)
        ],
        "successful": [],
        "failed_attempts": [],
        "excluded_primary_pairs": [],
        "completed_replacements": [],
    }


def _completed_pairs(state: dict[str, Any]) -> set[int]:
    pairs: set[int] = set()
    for result in state["successful"]:
        task = _task_from_dict(result["task"])
        pairs.update(range(task.first_pair, task.first_pair + task.pair_count))
    return pairs


def _replacement_task(
    excluded: int, used: set[int], lower_bound: int = PRIMARY_PAIRS
) -> Task:
    candidate = lower_bound + ((excluded - lower_bound) % 4)
    while candidate in used:
        candidate += 4
    return Task(candidate, 1, "replacement", excluded)


def collect(args: argparse.Namespace) -> dict[str, Any]:
    root: Path = args.evidence_root
    state_path = root / "collection-state.json"
    if root.exists() and not state_path.exists() and any(root.iterdir()):
        _fail("nonempty evidence root has no resumable collection state")
    root.mkdir(parents=True, exist_ok=True)
    lock = threading.Lock()
    if state_path.exists():
        state = json.loads(state_path.read_text(encoding="utf-8"))
        if state.get("schema") != SCHEMA:
            _fail("existing collection state schema mismatch")
        if state.get("config") != _collection_config(args):
            _fail("existing collection state configuration mismatch")
        completed = _completed_pairs(state)
        recovered = [
            value
            for value in state.get("in_progress", [])
            if not set(
                range(
                    int(value["first_pair"]),
                    int(value["first_pair"]) + int(value["pair_count"]),
                )
            ).issubset(completed)
        ]
        state["pending"] = recovered + state.get("pending", [])
        state["in_progress"] = []
        _atomic_json(state_path, state)
    else:
        state = _initial_state(args)
        _atomic_json(state_path, state)

    databases = [
        _worker_database(root, worker, args.source_database)
        for worker in range(WORKERS)
    ]
    while state["pending"]:
        wave = [_task_from_dict(value) for value in state["pending"]]
        state["pending"] = []
        state["in_progress"] = [_task_dict(task) for task in wave]
        _atomic_json(state_path, state)
        results: list[dict[str, Any]] = []
        for batch_start in range(0, len(wave), WORKERS):
            batch = wave[batch_start : batch_start + WORKERS]
            with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as executor:
                futures = {
                    executor.submit(_run_task, task, worker, args, databases[worker]): task
                    for worker, task in enumerate(batch)
                }
                for future in concurrent.futures.as_completed(futures):
                    task = futures[future]
                    try:
                        result = future.result()
                    except Exception as error:
                        result = {
                            "task": _task_dict(task),
                            "worker": None,
                            "status": "failed-infrastructure",
                            "error": repr(error),
                        }
                    with lock:
                        results.append(result)
                        if result["status"] == "success":
                            state["successful"].append(result)
                            completed_task = _task_from_dict(result["task"])
                            if completed_task.kind == "replacement":
                                state["completed_replacements"].append(
                                    {
                                        "excluded": completed_task.replaces_pair,
                                        "replacement": completed_task.first_pair,
                                    }
                                )
                        else:
                            state["failed_attempts"].append(result)
                        state["in_progress"] = [
                            value
                            for value in state["in_progress"]
                            if _task_from_dict(value) != task
                        ]
                        _atomic_json(state_path, state)
        follow_up: list[Task] = []
        for result in sorted(results, key=lambda value: value["task"]["first_pair"]):
            if result["status"] == "success":
                continue
            task = _task_from_dict(result["task"])
            if task.pair_count > 1:
                left = task.pair_count // 2
                follow_up.extend(
                    (
                        Task(task.first_pair, left, task.kind, task.replaces_pair),
                        Task(
                            task.first_pair + left,
                            task.pair_count - left,
                            task.kind,
                            task.replaces_pair,
                        ),
                    )
                )
            elif task.kind == "primary":
                state["excluded_primary_pairs"].append(task.first_pair)
            else:
                used = _completed_pairs(state)
                follow_up.append(
                    _replacement_task(
                        int(task.replaces_pair), used, task.first_pair + 4
                    )
                )
        if not follow_up and state["phase"] == "primary":
            state["phase"] = "replacement"
            used = _completed_pairs(state)
            for excluded in sorted(set(state["excluded_primary_pairs"])):
                replacement = _replacement_task(excluded, used)
                used.add(replacement.first_pair)
                follow_up.append(replacement)
        state["pending"] = [_task_dict(task) for task in follow_up]
        state["in_progress"] = []
        _atomic_json(state_path, state)

    completed = _completed_pairs(state)
    fold_counts = {str(fold): sum(pair % 4 == fold for pair in completed) for fold in range(4)}
    state["phase"] = "complete"
    state["completed_pair_count"] = len(completed)
    state["fold_pair_counts"] = fold_counts
    state["pass"] = len(completed) == PRIMARY_PAIRS and set(fold_counts.values()) == {512}
    _atomic_json(state_path, state)
    return {
        "state": str(state_path),
        "state_sha256": _sha256(state_path),
        "completed_pair_count": len(completed),
        "excluded_primary_pairs": sorted(set(state["excluded_primary_pairs"])),
        "completed_replacements": state["completed_replacements"],
        "fold_pair_counts": fold_counts,
        "pass": state["pass"],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--mage-repo", type=Path, required=True)
    parser.add_argument("--scorer-exe", type=Path, required=True)
    parser.add_argument("--outcome-root", type=Path, required=True)
    parser.add_argument("--source-database", type=Path, required=True)
    parser.add_argument("--maven", type=Path, required=True)
    args = parser.parse_args()
    if (
        not args.mage_repo.is_dir()
        or not args.scorer_exe.is_file()
        or not args.outcome_root.is_dir()
        or not args.source_database.is_file()
        or not args.maven.is_file()
        or _sha256(args.source_database) != CARD_DB_SHA256
        or _sha256(args.scorer_exe) != SCORER_SHA256
    ):
        _fail("fixed collector input validation failed")
    result = collect(args)
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0 if result["pass"] else 2


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
