#!/usr/bin/env python3
"""Collect the V4 current-GAE8 versus XMage CP7 outcome corpus.

The intended runtime is the pinned ``mage-kernel-anchor-spike-v1`` worktree,
whose ``XMageRallyAnchorSpike`` supports multiple matched pairs per JVM.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
from typing import Any

from outcome_v2 import (
    EXPECTED_CHECKPOINT,
    PACKAGE_MANIFEST_SHA256,
    PACKAGE_PAYLOAD_SHA256,
    canonical_json_bytes,
    exclusive_write,
    load_exact_gae8_package,
    sha256,
    validate_outcome_shard,
)


SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-collection/v1"
MANIFEST_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-manifest/v1"
IDENTITY_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-revealed-identity/v1"
CARD_DATABASE_SHA256 = "b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d"
PAIR_PREFIX = "XMAGE_RALLY_ANCHOR_PAIR PASS "
FINAL_PREFIX = "XMAGE_RALLY_ANCHOR_SPIKE PASS "
MARKER_FIELDS = re.compile(r"([a-z0-9_]+)=([^ ]+)")
DEFAULT_MAGE_REPO = Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1")
SCREEN_PANEL = (1_820_001, 0, 32)
FORMAL_COLLECTION_PANEL = (1_930_001, 0, 256)
MINIMUM_SCREEN_GAMES_PER_SECOND = 0.60
MINIMUM_AVAILABLE_MEMORY_BYTES = 16 * 1024**3
MAXIMUM_SYSTEM_MEMORY_USED_PERCENT = 90.0
MAXIMUM_GPU_1_MEMORY_USED_MIB = 512.0


def fail(message: str) -> None:
    raise RuntimeError(message)


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        timeout=60,
        check=True,
    )
    lines = completed.stdout.strip().splitlines()
    return lines[0] if lines else ""


def _git_commit(repo: Path) -> str:
    return _version(
        [
            "git",
            "-c",
            "safe.directory=" + repo.as_posix(),
            "-C",
            str(repo),
            "rev-parse",
            "HEAD",
        ]
    )


def _load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        fail(f"JSON evidence is not an object: {path}")
    return value


def _same_path(left: str, right: Path) -> bool:
    return Path(left).resolve(strict=False) == right.resolve(strict=False)


def _validate_collection_prerequisites(args: argparse.Namespace) -> dict[str, Any] | None:
    panel = (args.base_seed, args.pair_start, args.pairs)
    if panel == SCREEN_PANEL:
        if args.throughput_screen_report is not None or args.revealed_identity_report is not None:
            fail("the revealed throughput screen may not claim prior gate evidence")
        return None
    if panel != FORMAL_COLLECTION_PANEL:
        fail("collector panel must be the exact revealed screen or formal V4 collection")
    if args.throughput_screen_report is None or args.revealed_identity_report is None:
        fail("formal V4 collection requires throughput-screen and revealed-identity reports")

    screen_path = args.throughput_screen_report.resolve(strict=True)
    identity_path = args.revealed_identity_report.resolve(strict=True)
    screen = _load_json(screen_path)
    identity = _load_json(identity_path)
    screen_panel = screen.get("panel")
    resources = screen.get("resource_usage")
    screen_manifest = screen.get("manifest")
    if (
        screen.get("schema") != SCHEMA
        or screen.get("status") != "complete"
        or not isinstance(screen_panel, dict)
        or screen_panel.get("arm") != "gae8"
        or screen_panel.get("opponent") != "xmage-cp7"
        or screen_panel.get("cp7_skill") != 7
        or screen_panel.get("base_seed") != SCREEN_PANEL[0]
        or screen_panel.get("pair_start") != SCREEN_PANEL[1]
        or screen_panel.get("pair_count") != SCREEN_PANEL[2]
        or screen_panel.get("episode_count") != SCREEN_PANEL[2] * 2
        or screen_panel.get("workers") != args.workers
        or not isinstance(screen.get("games_per_second"), (int, float))
        or isinstance(screen.get("games_per_second"), bool)
        or not math.isfinite(float(screen["games_per_second"]))
        or float(screen["games_per_second"]) < MINIMUM_SCREEN_GAMES_PER_SECOND
        or not isinstance(resources, dict)
        or resources.get("system_memory_available_bytes_minimum", 0)
        < MINIMUM_AVAILABLE_MEMORY_BYTES
        or resources.get("system_memory_used_percent", {}).get("maximum", 100.0)
        > MAXIMUM_SYSTEM_MEMORY_USED_PERCENT
        or resources.get("gpu_1_memory_used_mib", {}).get("maximum", float("inf"))
        > MAXIMUM_GPU_1_MEMORY_USED_MIB
        or not isinstance(screen_manifest, dict)
        or not isinstance(screen_manifest.get("path"), str)
        or not isinstance(screen_manifest.get("sha256"), str)
    ):
        fail("throughput-screen report does not satisfy the formal collection gate")
    screen_manifest_path = Path(screen_manifest["path"]).resolve(strict=True)
    if sha256(screen_manifest_path) != screen_manifest["sha256"]:
        fail("throughput-screen manifest hash mismatch")

    identity_candidate = identity.get("collection_reports", {}).get("candidate")
    expected_candidate_root = screen_path.parent / "tasks"
    if (
        identity.get("schema") != IDENTITY_SCHEMA
        or identity.get("status") != "pass"
        or identity.get("base_seed") != SCREEN_PANEL[0]
        or identity.get("first_pair") != SCREEN_PANEL[1]
        or identity.get("pair_count") != SCREEN_PANEL[2]
        or identity.get("episode_count") != SCREEN_PANEL[2] * 2
        or identity.get("comparison")
        != "all outcome-v2 fields except shard-local ordinal offsets"
        or not isinstance(identity.get("candidate_root"), str)
        or not _same_path(identity["candidate_root"], expected_candidate_root)
        or not isinstance(identity_candidate, dict)
        or identity_candidate.get("sha256") != sha256(screen_path)
        or not isinstance(identity_candidate.get("path"), str)
        or not _same_path(identity_candidate["path"], screen_path)
        or not isinstance(identity.get("pairs"), list)
        or len(identity["pairs"]) != SCREEN_PANEL[2]
    ):
        fail("revealed-identity report does not satisfy the formal collection gate")
    return {
        "throughput_screen_report": {
            "path": str(screen_path),
            "sha256": sha256(screen_path),
            "games_per_second": screen["games_per_second"],
            "workers": screen_panel["workers"],
        },
        "revealed_identity_report": {
            "path": str(identity_path),
            "sha256": sha256(identity_path),
        },
    }


def _chunk_ranges(pair_start: int, pairs: int, task_pairs: int) -> list[tuple[int, int]]:
    stop = pair_start + pairs
    return [
        (first, min(task_pairs, stop - first))
        for first in range(pair_start, stop, task_pairs)
    ]


def _maven_opts(package: dict[str, Any]) -> str:
    return " ".join(
        (
            f"-Dxmage.rally.cp7Outcome.authorityKind={package['authority_kind']}",
            f"-Dxmage.rally.cp7Outcome.adamStep={package['adam_step']}",
            f"-Dxmage.rally.cp7Outcome.manifestSha256={package['manifest_sha256']}",
            f"-Dxmage.rally.cp7Outcome.payloadSha256={package['payload_sha256']}",
            f"-Dxmage.rally.cp7Outcome.trainStateSha256={package['train_state_sha256']}",
            f"-Dxmage.rally.cp7Outcome.modelParameterSha256={package['model_parameter_sha256']}",
            "-Dxmage.rally.cp7Outcome.environmentTrajectoryContract=environment-randomization-v2",
        )
    )


def _environment(database_root: Path, package: dict[str, Any]) -> dict[str, str]:
    environment = os.environ.copy()
    environment.update(
        {
            "MAGE_DB_DIR": str(database_root),
            "MAGE_DB_AUTO_SERVER": "false",
            "AI_DETERMINISTIC_TIEBREAKS": "true",
            "AI_DETERMINISTIC_SEARCH": "true",
            "AI_DETERMINISTIC_MAX_NODES": "5000",
            "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
            "CUDA_VISIBLE_DEVICES": "1",
            "MAVEN_OPTS": _maven_opts(package),
        }
    )
    return environment


def _exec_argument_string(parts: list[str]) -> str:
    def quote(value: str) -> str:
        if not value or any(character.isspace() for character in value) or '"' in value:
            return '"' + value.replace('"', '\\"') + '"'
        return value

    return " ".join(quote(part) for part in parts)


def _anchor_command(
    args: argparse.Namespace,
    package: dict[str, Any],
    first_pair: int,
    pair_count: int,
    outcome_path: Path,
) -> list[str]:
    execution_args = _exec_argument_string(
        [
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(args.scorer_exe),
            "--outcome-root",
            package["root"],
            "--base-seed",
            str(args.base_seed),
            "--first-episode",
            str(first_pair * 2),
            "--pairs",
            str(pair_count),
            "--opponent",
            "cp7",
            "--cp7-skill",
            "7",
            "--outcome-export",
            str(outcome_path),
        ]
    )
    return [
        str(args.maven),
        "-o",
        "-q",
        "-pl",
        "Mage.Server.Plugins/Mage.Player.AIRL",
        "-DskipTests",
        "exec:java",
        "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
        f"-Dexec.args={execution_args}",
    ]


def _popen_group_options() -> dict[str, Any]:
    if os.name == "nt":
        return {"creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
    return {"start_new_session": True}


def _process_tree_snapshot(pid: int) -> list[tuple[Any, int, float]]:
    import psutil

    try:
        root = psutil.Process(pid)
        processes = [root, *root.children(recursive=True)]
    except (psutil.NoSuchProcess, psutil.AccessDenied):
        return []
    snapshot: list[tuple[Any, int, float]] = []
    for process in processes:
        try:
            snapshot.append((process, process.pid, process.create_time()))
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return snapshot


def _matching_process_is_alive(entry: tuple[Any, int, float]) -> bool:
    import psutil

    process, _, created = entry
    try:
        return (
            process.create_time() == created
            and process.is_running()
            and process.status() != psutil.STATUS_ZOMBIE
        )
    except psutil.NoSuchProcess:
        return False
    except psutil.AccessDenied:
        return True


def _terminate_process_tree(process: subprocess.Popen[Any]) -> None:
    snapshot = _process_tree_snapshot(process.pid)
    if process.poll() is None:
        if os.name == "nt":
            subprocess.run(
                ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=30,
                check=False,
            )
        else:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    try:
        process.wait(timeout=30)
    except subprocess.TimeoutExpired:
        fail(f"process root {process.pid} did not exit after tree termination")
    deadline = time.monotonic() + 15.0
    survivors = [entry for entry in snapshot if _matching_process_is_alive(entry)]
    while survivors and time.monotonic() < deadline:
        time.sleep(0.1)
        survivors = [entry for entry in survivors if _matching_process_is_alive(entry)]
    if process.poll() is None or survivors:
        survivor_pids = [entry[1] for entry in survivors]
        fail(
            f"process tree {process.pid} remains live after termination; "
            f"survivors={survivor_pids}"
        )


def _parse_markers(log_path: Path, prefix: str) -> list[dict[str, str]]:
    return [
        dict(MARKER_FIELDS.findall(line))
        for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines()
        if line.startswith(prefix)
    ]


def _validate_log_markers(
    log_path: Path,
    *,
    base_seed: int,
    first_pair: int,
    pair_count: int,
) -> None:
    pairs = _parse_markers(log_path, PAIR_PREFIX)
    if len(pairs) != pair_count:
        fail(f"{log_path}: expected {pair_count} pair completion markers")
    by_index: dict[int, dict[str, str]] = {}
    for marker in pairs:
        try:
            pair_index = int(marker.get("pair_index", ""))
        except ValueError:
            fail(f"{log_path}: invalid pair marker index")
        if pair_index in by_index:
            fail(f"{log_path}: duplicate pair marker {pair_index}")
        by_index[pair_index] = marker
    for pair_index in range(first_pair, first_pair + pair_count):
        marker = by_index.get(pair_index)
        if (
            marker is None
            or marker.get("base_seed") != str(base_seed)
            or marker.get("opponent") != "cp7"
            or marker.get("cp7_skill") != "7"
            or marker.get("episodes") != f"{pair_index * 2},{pair_index * 2 + 1}"
            or marker.get("candidate_seats") != "p0,p1"
        ):
            fail(f"{log_path}: pair marker identity mismatch for pair {pair_index}")
    finals = _parse_markers(log_path, FINAL_PREFIX)
    if len(finals) != 1:
        fail(f"{log_path}: expected one final completion marker")
    final = finals[0]
    if (
        final.get("base_seed") != str(base_seed)
        or final.get("opponent") != "cp7"
        or final.get("cp7_skill") != "7"
        or final.get("first_episode") != str(first_pair * 2)
        or final.get("pairs") != str(pair_count)
        or final.get("games") != str(pair_count * 2)
    ):
        fail(f"{log_path}: final completion marker identity mismatch")


def _prepare_workers(args: argparse.Namespace) -> list[Path]:
    roots: list[Path] = []
    for worker in range(args.workers):
        database_root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        database_root.mkdir(parents=True)
        destination = database_root / "cards.h2.mv.db"
        if destination.exists():
            fail(f"worker database already exists: {destination}")
        shutil.copyfile(args.source_database, destination)
        if sha256(destination) != CARD_DATABASE_SHA256:
            fail(f"worker {worker} card database copy SHA-256 mismatch")
        roots.append(database_root)
    return roots


def _gpu_sample(gpu_ordinal: int) -> dict[str, float]:
    completed = subprocess.run(
        [
            "nvidia-smi",
            "--query-gpu=index,utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=15,
        check=False,
    )
    if completed.returncode != 0:
        fail(f"nvidia-smi failed: {completed.stderr.strip()}")
    for line in completed.stdout.splitlines():
        fields = [field.strip() for field in line.split(",")]
        if len(fields) == 4 and int(fields[0]) == gpu_ordinal:
            return {
                "gpu_utilization_percent": float(fields[1]),
                "gpu_memory_used_mib": float(fields[2]),
                "gpu_memory_total_mib": float(fields[3]),
            }
    fail(f"nvidia-smi did not return GPU {gpu_ordinal}")


def _resource_monitor(
    stop: threading.Event,
    samples: list[dict[str, float]],
    errors: list[str],
    started: float,
) -> None:
    import psutil

    root = psutil.Process(os.getpid())
    psutil.cpu_percent(interval=None)
    while not stop.is_set():
        try:
            rss = 0
            for process in [root, *root.children(recursive=True)]:
                try:
                    rss += process.memory_info().rss
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    pass
            memory = psutil.virtual_memory()
            sample = {
                "elapsed_seconds": time.perf_counter() - started,
                "system_cpu_percent": float(psutil.cpu_percent(interval=None)),
                "process_tree_rss_bytes": float(rss),
                "system_memory_total_bytes": float(memory.total),
                "system_memory_available_bytes": float(memory.available),
                "system_memory_used_percent": float(memory.percent),
            }
            sample.update(_gpu_sample(1))
            samples.append(sample)
        except Exception as error:
            errors.append(f"{type(error).__name__}: {error}")
            return
        stop.wait(1.0)


def _resource_summary(samples: list[dict[str, float]]) -> dict[str, Any]:
    if not samples:
        fail("resource monitor produced no samples")

    def average_maximum(field: str) -> dict[str, float]:
        values = [sample[field] for sample in samples]
        return {"average": sum(values) / len(values), "maximum": max(values)}

    return {
        "sample_count": len(samples),
        "system_cpu_percent": average_maximum("system_cpu_percent"),
        "process_tree_rss_bytes": average_maximum("process_tree_rss_bytes"),
        "system_memory_total_bytes": samples[0]["system_memory_total_bytes"],
        "system_memory_available_bytes_minimum": min(
            sample["system_memory_available_bytes"] for sample in samples
        ),
        "system_memory_used_percent": average_maximum("system_memory_used_percent"),
        "gpu_1_utilization_percent": average_maximum("gpu_utilization_percent"),
        "gpu_1_memory_used_mib": average_maximum("gpu_memory_used_mib"),
        "gpu_1_memory_total_mib": samples[0]["gpu_memory_total_mib"],
    }


def _run_task(
    args: argparse.Namespace,
    package: dict[str, Any],
    worker_slots: queue.Queue[tuple[int, Path]],
    active_processes: dict[int, subprocess.Popen[Any]],
    active_lock: threading.Lock,
    first_pair: int,
    pair_count: int,
) -> dict[str, Any]:
    worker, database_root = worker_slots.get()
    process: subprocess.Popen[Any] | None = None
    try:
        stem = f"gae8-p{first_pair:06d}-n{pair_count:03d}"
        task_root = args.evidence_root / "tasks"
        log_path = task_root / f"{stem}.log"
        outcome_path = task_root / f"{stem}.outcome.jsonl"
        if log_path.exists() or outcome_path.exists():
            fail(f"task output already exists: {stem}")
        started = time.perf_counter()
        with log_path.open("x", encoding="utf-8", newline="\n") as log:
            process = subprocess.Popen(
                _anchor_command(args, package, first_pair, pair_count, outcome_path),
                cwd=args.mage_repo,
                env=_environment(database_root, package),
                stdout=log,
                stderr=subprocess.STDOUT,
                **_popen_group_options(),
            )
            with active_lock:
                active_processes[process.pid] = process
            try:
                return_code = process.wait(timeout=args.task_timeout_seconds)
            except subprocess.TimeoutExpired:
                _terminate_process_tree(process)
                fail(f"task {stem} exceeded timeout {args.task_timeout_seconds}s")
        elapsed = time.perf_counter() - started
        if return_code != 0:
            fail(f"task {stem} exited {return_code}; see {log_path}")
        if not outcome_path.is_file():
            fail(f"task {stem} did not create its outcome shard")
        _validate_log_markers(
            log_path,
            base_seed=args.base_seed,
            first_pair=first_pair,
            pair_count=pair_count,
        )
        validation = validate_outcome_shard(
            outcome_path,
            base_seed=args.base_seed,
            first_pair=first_pair,
            pair_count=pair_count,
        )
        return {
            "worker": worker,
            "first_pair": first_pair,
            "pair_count": pair_count,
            "game_count": pair_count * 2,
            "elapsed_seconds": elapsed,
            "games_per_second": pair_count * 2 / elapsed,
            "log": {
                "path": str(log_path.resolve()),
                "sha256": sha256(log_path),
                "byte_count": log_path.stat().st_size,
            },
            "outcome": {key: value for key, value in validation.items() if key != "header"},
        }
    finally:
        if process is not None:
            with active_lock:
                active_processes.pop(process.pid, None)
            if process.poll() is None:
                _terminate_process_tree(process)
        worker_slots.put((worker, database_root))


def _toolchain(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "python": sys.version.split()[0],
        "java": _version(["java", "-version"]),
        "maven": _version([str(args.maven), "--version"]),
        "rustc": _version(["rustc", "--version"]),
        "cargo": _version(["cargo", "--version"]),
        "scorer_build_linker_file_version": args.linker_file_version,
    }


def _manifest(
    args: argparse.Namespace,
    package: dict[str, Any],
    input_hashes: dict[str, str],
    toolchain: dict[str, Any],
    prerequisites: dict[str, Any] | None,
) -> dict[str, Any]:
    kernel_repo = Path(__file__).resolve().parents[2]
    script_root = Path(__file__).resolve().parent
    return {
        "schema": MANIFEST_SCHEMA,
        "kernel_git_commit": _git_commit(kernel_repo),
        "mage_git_commit": _git_commit(args.mage_repo),
        "mage_repo": str(args.mage_repo),
        "mage_main_class": "mage.player.ai.rl.XMageRallyAnchorSpike",
        "inputs": {
            "runner_sha256": sha256(Path(__file__).resolve()),
            "outcome_validator_sha256": sha256(script_root / "outcome_v2.py"),
            "merger_sha256": sha256(script_root / "merge_corpus_v4.py"),
            "scorer_exe": str(args.scorer_exe),
            "scorer_sha256": input_hashes["scorer"],
            "source_database": str(args.source_database),
            "source_database_sha256": input_hashes["database"],
            "gae8_package": package,
        },
        "panel": {
            "arm": "gae8",
            "opponent": "xmage-cp7",
            "cp7_skill": 7,
            "base_seed": args.base_seed,
            "pair_start": args.pair_start,
            "pair_count": args.pairs,
            "episode_count": args.pairs * 2,
            "workers": args.workers,
            "task_pairs": args.task_pairs,
            "task_timeout_seconds": args.task_timeout_seconds,
            "gpu_ordinal_reserved": 1,
            "workload_device": "cpu",
        },
        "output_root": str(args.evidence_root),
        "collection_prerequisites": prerequisites,
        "toolchain": toolchain,
        "analysis_policy": {
            "outcome_based_early_analysis": False,
            "terminal_win_draw_loss_only": True,
        },
    }


def collect(args: argparse.Namespace) -> dict[str, Any]:
    prerequisites = _validate_collection_prerequisites(args)
    package = load_exact_gae8_package(args.gae8_root)
    scorer_hash = sha256(args.scorer_exe)
    database_hash = sha256(args.source_database)
    if database_hash != CARD_DATABASE_SHA256:
        fail("source card database SHA-256 mismatch")
    toolchain = _toolchain(args)
    args.evidence_root.mkdir(parents=True)
    manifest_path = args.evidence_root / "manifest.json"
    manifest = _manifest(
        args,
        package,
        {"scorer": scorer_hash, "database": database_hash},
        toolchain,
        prerequisites,
    )
    exclusive_write(manifest_path, canonical_json_bytes(manifest, indent=2))
    (args.evidence_root / "tasks").mkdir()
    worker_roots = _prepare_workers(args)
    worker_slots: queue.Queue[tuple[int, Path]] = queue.Queue()
    for worker, root in enumerate(worker_roots):
        worker_slots.put((worker, root))

    chunks = _chunk_ranges(args.pair_start, args.pairs, args.task_pairs)
    active_processes: dict[int, subprocess.Popen[Any]] = {}
    active_lock = threading.Lock()
    samples: list[dict[str, float]] = []
    monitor_errors: list[str] = []
    stop_monitor = threading.Event()
    started = time.perf_counter()
    monitor = threading.Thread(
        target=_resource_monitor,
        args=(stop_monitor, samples, monitor_errors, started),
        daemon=True,
    )
    monitor.start()
    results: list[dict[str, Any]] = []
    executor = concurrent.futures.ThreadPoolExecutor(max_workers=args.workers)
    futures: dict[concurrent.futures.Future[dict[str, Any]], tuple[int, int]] = {}
    try:
        futures = {
            executor.submit(
                _run_task,
                args,
                package,
                worker_slots,
                active_processes,
                active_lock,
                first_pair,
                pair_count,
            ): (first_pair, pair_count)
            for first_pair, pair_count in chunks
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
                    },
                    sort_keys=True,
                    allow_nan=False,
                ),
                flush=True,
            )
    except BaseException:
        for future in futures:
            future.cancel()
        with active_lock:
            processes = list(active_processes.values())
        for process in processes:
            if process.poll() is None:
                _terminate_process_tree(process)
        executor.shutdown(wait=True, cancel_futures=True)
        raise
    else:
        executor.shutdown(wait=True)
    finally:
        stop_monitor.set()
        monitor.join(timeout=30)
    if monitor.is_alive():
        fail("resource monitor did not stop")
    if monitor_errors:
        fail("resource monitor failed: " + "; ".join(monitor_errors))
    elapsed = time.perf_counter() - started
    results.sort(key=lambda row: row["first_pair"])
    if [(row["first_pair"], row["pair_count"]) for row in results] != chunks:
        fail("completed task coverage differs from the configured pair range")

    sample_path = args.evidence_root / "resource_samples.jsonl"
    sample_payload = b"".join(canonical_json_bytes(sample) for sample in samples)
    exclusive_write(sample_path, sample_payload)
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "manifest": {
            "path": str(manifest_path.resolve()),
            "sha256": sha256(manifest_path),
        },
        "panel": manifest["panel"],
        "elapsed_seconds": elapsed,
        "games_per_second": args.pairs * 2 / elapsed,
        "resource_usage": _resource_summary(samples),
        "resource_samples": {
            "path": str(sample_path.resolve()),
            "sha256": sha256(sample_path),
            "byte_count": sample_path.stat().st_size,
        },
        "package_identity": package,
        "checkpoint_identity": EXPECTED_CHECKPOINT,
        "tasks": results,
        "analysis_policy": manifest["analysis_policy"],
        "non_claims": [
            "CP7 is part of the training distribution",
            "collection completion is not playing-strength evidence",
        ],
    }
    report_path = args.evidence_root / "report.json"
    exclusive_write(report_path, canonical_json_bytes(report, indent=2))
    return report


def _self_test() -> int:
    if _chunk_ranges(4, 10, 4) != [(4, 4), (8, 4), (12, 2)]:
        fail("chunk range self-test failed")
    if EXPECTED_CHECKPOINT["loaded_checkpoint_sha256"] != PACKAGE_MANIFEST_SHA256:
        fail("manifest checkpoint identity self-test failed")
    if EXPECTED_CHECKPOINT["loaded_payload_sha256"] != PACKAGE_PAYLOAD_SHA256:
        fail("payload checkpoint identity self-test failed")
    print("collect_corpus_v4: SELF-TEST PASS")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=(
            "Use the mage-kernel-anchor-spike-v1 worktree. All evidence files "
            "and the evidence root itself must be absent before launch."
        ),
    )
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--gae8-root", type=Path)
    parser.add_argument("--scorer-exe", type=Path)
    parser.add_argument("--mage-repo", type=Path, default=DEFAULT_MAGE_REPO)
    parser.add_argument("--source-database", type=Path)
    parser.add_argument("--maven", type=Path)
    parser.add_argument("--base-seed", type=int, default=1_930_001)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--pairs", type=int, default=256)
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--task-pairs", type=int, default=32)
    parser.add_argument("--task-timeout-seconds", type=int, default=7_200)
    parser.add_argument("--throughput-screen-report", type=Path)
    parser.add_argument("--revealed-identity-report", type=Path)
    parser.add_argument("--linker-file-version", default="14.50.35725.0")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        return _self_test()
    required = ("evidence_root", "gae8_root", "scorer_exe", "source_database", "maven")
    if any(getattr(args, field) is None for field in required):
        fail("all run paths are required")
    if (
        not 0 <= args.base_seed <= 0x7FFF_FFFF_FFFF_FFFF
        or args.pair_start < 0
        or not 1 <= args.pairs <= 4096
        or not 1 <= args.workers <= 24
        or not 1 <= args.task_pairs <= 128
        or args.task_timeout_seconds < 60
        or args.pair_start + args.pairs > 0x3FFF_FFFF_FFFF_FFFF
    ):
        fail("invalid seed, pair range, worker count, task size, or timeout")
    for field in ("gae8_root", "scorer_exe", "mage_repo", "source_database", "maven"):
        setattr(args, field, getattr(args, field).resolve(strict=True))
    if (
        not args.gae8_root.is_dir()
        or not args.scorer_exe.is_file()
        or not args.mage_repo.is_dir()
        or not args.source_database.is_file()
        or not args.maven.is_file()
    ):
        fail("one or more run paths have the wrong file type")
    args.evidence_root = args.evidence_root.resolve()
    if args.evidence_root.exists():
        fail(f"evidence root already exists: {args.evidence_root}")
    report = collect(args)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"collect_corpus_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
