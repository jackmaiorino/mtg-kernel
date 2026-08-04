#!/usr/bin/env python3
"""Collect candidate-state CP7 priority labels with matched outcome tensors."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import time
from typing import Any


SCHEMA = "mtg-kernel-cp7-candidate-shadow-corpus/v1"
TEACHER_SCHEMA = "xmage-rally-cp7-counterfactual-teacher-jsonl/v1"
TEACHER_SOURCE = "xmage_rally_shadow_cp7_candidate_priority"
OUTCOME_CONTRACT_PREFIX = "mtg-kernel-xmage-cp7-outcome-jsonl/"
PARENT_IDENTITY = {
    "adam_step": "1",
    "manifest": "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb",
    "payload": "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c",
    "train_state": "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8",
    "model": "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546",
}
USABLE_STATUSES = {
    "source_id",
    "text",
    "text_disambig",
    "plan_pass",
    "step_pass",
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
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=True,
    )
    return completed.stdout.strip().splitlines()[0]


def _decision_key(row: dict[str, Any]) -> tuple[Any, ...]:
    return (
        row.get("episode_id"),
        row.get("step"),
        row.get("physical_decision_id"),
        row.get("substep_index"),
        row.get("model_input_sha256"),
    )


def _load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            try:
                value = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(value, dict):
                _fail(f"{path}:{line_number}: record is not an object")
            rows.append(value)
    return rows


def _validate_task_outputs(
    outcome_path: Path,
    teacher_path: Path,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    outcome_rows = _load_jsonl(outcome_path)
    teacher_rows = _load_jsonl(teacher_path)
    outcome_headers = [row for row in outcome_rows if row.get("record_type") == "header"]
    outcome_decisions = [row for row in outcome_rows if row.get("record_type") == "decision"]
    terminals = [row for row in outcome_rows if row.get("record_type") == "terminal"]
    if len(outcome_headers) != 1:
        _fail(f"{outcome_path}: expected one outcome header")
    contract = outcome_headers[0].get("export_contract")
    if not isinstance(contract, str) or not contract.startswith(OUTCOME_CONTRACT_PREFIX):
        _fail(f"{outcome_path}: unexpected outcome contract")
    expected_terminals = {
        (pair, pair * 2 + seat, f"p{seat}")
        for pair in range(first_pair, first_pair + pair_count)
        for seat in (0, 1)
    }
    observed_terminals = {
        (row.get("pair_index"), row.get("episode_id"), row.get("candidate_seat"))
        for row in terminals
    }
    if observed_terminals != expected_terminals:
        _fail(f"{outcome_path}: terminal coverage mismatch")
    outcome_by_key: dict[tuple[Any, ...], dict[str, Any]] = {}
    for row in outcome_decisions:
        key = _decision_key(row)
        if key in outcome_by_key:
            _fail(f"{outcome_path}: duplicate outcome decision key {key}")
        outcome_by_key[key] = row

    teacher_headers = [row for row in teacher_rows if row.get("record_type") == "header"]
    teacher_decisions = [row for row in teacher_rows if row.get("record_type") == "decision"]
    teacher_summaries = [row for row in teacher_rows if row.get("record_type") == "summary"]
    if len(teacher_headers) != 1 or len(teacher_summaries) != 1:
        _fail(f"{teacher_path}: expected one teacher header and summary")
    header = teacher_headers[0]
    if header.get("schema") != TEACHER_SCHEMA or header.get("selection_source") != TEACHER_SOURCE:
        _fail(f"{teacher_path}: teacher identity mismatch")
    if teacher_summaries[0].get("queries") != len(teacher_decisions):
        _fail(f"{teacher_path}: teacher summary query count mismatch")
    expected_episodes = {pair * 2 + seat for pair in range(first_pair, first_pair + pair_count) for seat in (0, 1)}
    observed_episodes: set[int] = set()
    usable = 0
    disagreements = 0
    statuses: dict[str, int] = {}
    for row in teacher_decisions:
        key = _decision_key(row)
        outcome = outcome_by_key.get(key)
        if outcome is None:
            _fail(f"{teacher_path}: teacher decision does not join to outcome {key}")
        if row.get("episode_id") not in expected_episodes:
            _fail(f"{teacher_path}: teacher decision escaped task episodes")
        observed_episodes.add(int(row["episode_id"]))
        for teacher_field, outcome_field in (
            ("candidate_selected_index", "selected_index"),
            ("legal_action_count", "legal_action_count"),
            ("candidate_order_commitment_128_hex", "candidate_order_commitment_128_hex"),
            ("action_semantics", "action_semantics"),
        ):
            if row.get(teacher_field) != outcome.get(outcome_field):
                _fail(f"{teacher_path}: joined field mismatch for {key}: {teacher_field}")
        status = str(row.get("teacher_status"))
        statuses[status] = statuses.get(status, 0) + 1
        teacher_index = row.get("teacher_selected_index")
        legal_count = row.get("legal_action_count")
        is_usable = (
            status in USABLE_STATUSES
            and isinstance(teacher_index, int)
            and isinstance(legal_count, int)
            and 0 <= teacher_index < legal_count
        )
        if is_usable:
            usable += 1
            disagreements += int(teacher_index != row.get("candidate_selected_index"))
    if observed_episodes != expected_episodes:
        _fail(f"{teacher_path}: at least one episode has no priority label")
    if not teacher_decisions or usable / len(teacher_decisions) < 0.95:
        _fail(f"{teacher_path}: fewer than 95 percent of labels are usable")
    return {
        "outcome_path": str(outcome_path),
        "outcome_sha256": _sha256(outcome_path),
        "outcome_bytes": outcome_path.stat().st_size,
        "outcome_decisions": len(outcome_decisions),
        "terminal_count": len(terminals),
        "teacher_path": str(teacher_path),
        "teacher_sha256": _sha256(teacher_path),
        "teacher_bytes": teacher_path.stat().st_size,
        "teacher_decisions": len(teacher_decisions),
        "usable_labels": usable,
        "candidate_teacher_disagreements": disagreements,
        "teacher_statuses": dict(sorted(statuses.items())),
    }


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
                    f"-Dxmage.rally.cp7Outcome.adamStep={PARENT_IDENTITY['adam_step']}",
                    f"-Dxmage.rally.cp7Outcome.manifestSha256={PARENT_IDENTITY['manifest']}",
                    f"-Dxmage.rally.cp7Outcome.payloadSha256={PARENT_IDENTITY['payload']}",
                    f"-Dxmage.rally.cp7Outcome.trainStateSha256={PARENT_IDENTITY['train_state']}",
                    f"-Dxmage.rally.cp7Outcome.modelParameterSha256={PARENT_IDENTITY['model']}",
                )
            ),
        }
    )
    return environment


def _prepare_workers(args: argparse.Namespace, source_sha256: str) -> list[Path]:
    roots: list[Path] = []
    for worker in range(args.workers):
        database_root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        database_root.mkdir(parents=True, exist_ok=True)
        destination = database_root / "cards.h2.mv.db"
        if destination.exists():
            _fail(f"worker database already exists: {destination}")
        shutil.copyfile(args.source_database, destination)
        if _sha256(destination) != source_sha256:
            _fail(f"worker {worker} database copy mismatch")
        roots.append(database_root)
    return roots


def _run_task(
    args: argparse.Namespace,
    slots: queue.Queue[tuple[int, Path]],
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    worker, database_root = slots.get()
    try:
        task_root = args.evidence_root / "tasks"
        stem = f"p{first_pair:06d}-n{pair_count:03d}"
        outcome_path = task_root / f"{stem}.outcome.jsonl"
        teacher_path = task_root / f"{stem}.teacher.jsonl"
        log_path = task_root / f"{stem}.log"
        for path in (outcome_path, teacher_path, log_path):
            if path.exists():
                _fail(f"task output already exists: {path}")
        exec_args = " ".join(
            (
                "--repo-root",
                str(args.mage_repo),
                "--scorer-exe",
                str(args.scorer_exe),
                "--outcome-root",
                str(args.outcome_root),
                "--base-seed",
                str(args.base_seed),
                "--first-episode",
                str(first_pair * 2),
                "--pairs",
                str(pair_count),
                "--opponent cp7 --cp7-skill 7",
                "--outcome-export",
                str(outcome_path),
                "--shadow-cp7-export",
                str(teacher_path),
                "--shadow-cp7-max-think-seconds",
                str(args.shadow_max_think_seconds),
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
                env=_environment(database_root),
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=args.task_timeout_seconds,
                check=False,
            )
        elapsed = time.perf_counter() - started
        if completed.returncode != 0:
            _fail(f"task {stem} exited {completed.returncode}; see {log_path}")
        validated = _validate_task_outputs(outcome_path, teacher_path, first_pair, pair_count)
        return {
            "worker": worker,
            "first_pair": first_pair,
            "pair_count": pair_count,
            "games": pair_count * 2,
            "elapsed_seconds": elapsed,
            "games_per_second": pair_count * 2 / elapsed,
            "log_path": str(log_path),
            "log_sha256": _sha256(log_path),
            **validated,
        }
    finally:
        slots.put((worker, database_root))


def collect(args: argparse.Namespace) -> dict[str, Any]:
    started = time.perf_counter()
    kernel_repo = Path(__file__).resolve().parents[2]
    if args.evidence_root.exists():
        _fail(f"evidence root already exists: {args.evidence_root}")
    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    source_sha256 = _sha256(args.source_database)
    worker_roots = _prepare_workers(args, source_sha256)
    slots: queue.Queue[tuple[int, Path]] = queue.Queue()
    for worker, root in enumerate(worker_roots):
        slots.put((worker, root))
    tasks = []
    stop_pair = args.pair_start + args.pairs
    for first_pair in range(args.pair_start, stop_pair, args.task_pairs):
        tasks.append((first_pair, min(args.task_pairs, stop_pair - first_pair)))
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {
            executor.submit(_run_task, args, slots, first_pair, pair_count): (first_pair, pair_count)
            for first_pair, pair_count in tasks
        }
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            results.append(result)
            print(
                json.dumps(
                    {
                        "completed_first_pair": result["first_pair"],
                        "pair_count": result["pair_count"],
                        "worker": result["worker"],
                        "elapsed_seconds": result["elapsed_seconds"],
                        "teacher_decisions": result["teacher_decisions"],
                        "usable_labels": result["usable_labels"],
                    },
                    sort_keys=True,
                ),
                flush=True,
            )
    results.sort(key=lambda value: value["first_pair"])
    elapsed = time.perf_counter() - started
    total_games = args.pairs * 2
    total_labels = sum(result["teacher_decisions"] for result in results)
    total_usable = sum(result["usable_labels"] for result in results)
    total_disagreements = sum(result["candidate_teacher_disagreements"] for result in results)
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": args.base_seed,
        "pair_start": args.pair_start,
        "pairs": args.pairs,
        "games": total_games,
        "workers": args.workers,
        "task_pairs": args.task_pairs,
        "shadow_max_think_seconds": args.shadow_max_think_seconds,
        "elapsed_seconds": elapsed,
        "games_per_second": total_games / elapsed,
        "teacher_decisions": total_labels,
        "usable_labels": total_usable,
        "usable_fraction": total_usable / total_labels,
        "candidate_teacher_disagreements": total_disagreements,
        "disagreement_fraction": total_disagreements / total_usable,
        "inputs": {
            "kernel_git_commit": _version(["git", "rev-parse", "HEAD"], kernel_repo),
            "mage_git_commit": _version(["git", "rev-parse", "HEAD"], args.mage_repo),
            "mage_repo": str(args.mage_repo),
            "scorer_exe": str(args.scorer_exe),
            "scorer_sha256": _sha256(args.scorer_exe),
            "outcome_root": str(args.outcome_root),
            "source_database": str(args.source_database),
            "source_database_sha256": source_sha256,
            "maven": str(args.maven),
            "parent_identity": PARENT_IDENTITY,
        },
        "toolchain": {
            "python": sys.version.split()[0],
            "java": _version(["java", "-version"]),
            "maven": _version([str(args.maven), "--version"]),
            "rustc": _version(["rustc", "--version"]),
            "cargo": _version(["cargo", "--version"]),
            "scorer_build_linker_file_version": "14.50.35725.0",
            "gpu_ordinal": None,
        },
        "tasks": results,
    }
    _atomic_json(args.evidence_root / "report.json", report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--mage-repo", type=Path, required=True)
    parser.add_argument("--scorer-exe", type=Path, required=True)
    parser.add_argument("--outcome-root", type=Path, required=True)
    parser.add_argument("--source-database", type=Path, required=True)
    parser.add_argument("--maven", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, default=1_400_001)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--pairs", type=int, default=256)
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--task-pairs", type=int, default=32)
    parser.add_argument("--shadow-max-think-seconds", type=int, default=5)
    parser.add_argument("--task-timeout-seconds", type=int, default=7_200)
    args = parser.parse_args()
    args.evidence_root = args.evidence_root.resolve()
    args.mage_repo = args.mage_repo.resolve(strict=True)
    args.scorer_exe = args.scorer_exe.resolve(strict=True)
    args.outcome_root = args.outcome_root.resolve(strict=True)
    args.source_database = args.source_database.resolve(strict=True)
    args.maven = args.maven.resolve(strict=True)
    if (
        not args.mage_repo.is_dir()
        or not args.outcome_root.is_dir()
        or not args.scorer_exe.is_file()
        or not args.source_database.is_file()
        or not args.maven.is_file()
        or args.base_seed < 0
        or args.pair_start < 0
        or args.pairs < 1
        or not 1 <= args.workers <= 24
        or not 1 <= args.task_pairs <= 128
        or not 1 <= args.shadow_max_think_seconds <= 120
        or args.task_timeout_seconds < 60
    ):
        _fail("invalid collector arguments")
    report = collect(args)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
