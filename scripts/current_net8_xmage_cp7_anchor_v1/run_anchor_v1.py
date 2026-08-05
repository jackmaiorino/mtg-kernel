#!/usr/bin/env python3
"""Run matched current-Net8 policies against XMage CP7 skill 7."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time
from typing import Any


SCHEMA = "mtg-kernel-current-net8-xmage-cp7-anchor/v1"
PACKAGE_SCHEMA = "mtg-kernel-xmage-fixed-native-state/v1"
OUTCOME_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
PAIR_PREFIX = "XMAGE_RALLY_ANCHOR_PAIR PASS "
PAIR_FIELDS = re.compile(r"([a-z0-9_]+)=([^ ]+)")
CARD_DB_SHA256 = "b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d"
SOURCE_IDENTITY = {
    "source_run_sha256": "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
    "source_generation": 384,
    "source_checkpoint_sha256": "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
    "source_sidecar_sha256": "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb",
    "source_payload_sha256": "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
    "source_train_state_sha256": "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8",
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
        _fail(f"temporary output already exists: {temporary}")
    with temporary.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(value, handle, indent=2, sort_keys=True, allow_nan=False)
        handle.write("\n")
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
    lines = completed.stdout.strip().splitlines()
    return lines[0] if lines else ""


def _load_package(root: Path) -> dict[str, Any]:
    files = sorted(path.name for path in root.iterdir())
    if files != ["checkpoint.state.f32le", "fixed_native_state.json"]:
        _fail(f"package inventory mismatch: {root}")
    manifest_path = root / "fixed_native_state.json"
    raw = manifest_path.read_bytes()
    if not raw.endswith(b"\n") or b"\r" in raw:
        _fail(f"package manifest is not LF canonical: {manifest_path}")
    manifest = json.loads(raw)
    payload = manifest.get("payload")
    if (
        manifest.get("schema") != PACKAGE_SCHEMA
        or not isinstance(payload, dict)
        or payload.get("filename") != "checkpoint.state.f32le"
    ):
        _fail(f"package manifest contract mismatch: {manifest_path}")
    payload_path = root / payload["filename"]
    if payload_path.stat().st_size != payload.get("byte_count"):
        _fail(f"package payload byte count mismatch: {payload_path}")
    if _sha256(payload_path) != payload.get("payload_sha256"):
        _fail(f"package payload SHA-256 mismatch: {payload_path}")
    return {
        "root": str(root),
        "authority_kind": manifest["authority_kind"],
        "adam_step": int(payload["adam_step"]),
        "manifest_sha256": _sha256(manifest_path),
        "payload_sha256": payload["payload_sha256"],
        "train_state_sha256": payload["native_state_sha256"],
        "model_parameter_sha256": payload["model_parameter_sha256"],
        "source_result_sha256": manifest["source_result_sha256"],
    }


def _maven_opts(identity: dict[str, Any]) -> str:
    return " ".join(
        (
            f"-Dxmage.rally.cp7Outcome.authorityKind={identity['authority_kind']}",
            f"-Dxmage.rally.cp7Outcome.adamStep={identity['adam_step']}",
            f"-Dxmage.rally.cp7Outcome.manifestSha256={identity['manifest_sha256']}",
            f"-Dxmage.rally.cp7Outcome.payloadSha256={identity['payload_sha256']}",
            "-Dxmage.rally.cp7Outcome.trainStateSha256="
            + identity["train_state_sha256"],
            "-Dxmage.rally.cp7Outcome.modelParameterSha256="
            + identity["model_parameter_sha256"],
            "-Dxmage.rally.cp7Outcome.environmentTrajectoryContract="
            "environment-randomization-v2",
        )
    )


def _environment(database_root: Path, identity: dict[str, Any]) -> dict[str, str]:
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
            "MAVEN_OPTS": _maven_opts(identity),
        }
    )
    return environment


def _anchor_command(
    args: argparse.Namespace,
    package: dict[str, Any],
    pair_index: int,
    outcome: Path,
) -> list[str]:
    execution_args = " ".join(
        (
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(args.scorer_exe),
            "--outcome-root",
            package["root"],
            "--base-seed",
            str(args.base_seed),
            "--first-episode",
            str(pair_index * 2),
            "--pairs 1 --opponent cp7 --cp7-skill 7 --outcome-export",
            str(outcome),
        )
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


def _terminate_process_tree(process: subprocess.Popen[Any]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=30,
            check=False,
        )
    else:
        process.kill()
    try:
        process.wait(timeout=30)
    except subprocess.TimeoutExpired:
        _fail(f"process tree {process.pid} did not terminate")
    if process.poll() is None:
        _fail(f"process tree {process.pid} remains live after termination")


def _pair_marker_valid(log: Path, base_seed: int, pair_index: int) -> bool:
    markers: list[dict[str, str]] = []
    for line in log.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith(PAIR_PREFIX):
            markers.append(dict(PAIR_FIELDS.findall(line)))
    if len(markers) != 1:
        return False
    marker = markers[0]
    return (
        marker.get("base_seed") == str(base_seed)
        and marker.get("pair_index") == str(pair_index)
        and marker.get("episodes") == f"{pair_index * 2},{pair_index * 2 + 1}"
        and marker.get("candidate_seats") == "p0,p1"
    )


def _run_task(
    args: argparse.Namespace,
    worker: int,
    database_root: Path,
    arm: str,
    package: dict[str, Any],
    pair_index: int,
) -> dict[str, Any]:
    stem = f"{arm}-pair-{pair_index:04d}"
    task_root = args.evidence_root / "tasks"
    log = task_root / f"{stem}.log"
    outcome = task_root / f"{stem}.outcome.jsonl"
    if log.exists() or outcome.exists():
        _fail(f"task output already exists: {stem}")
    started = time.perf_counter()
    failure = None
    return_code = None
    try:
        with log.open("x", encoding="utf-8", newline="\n") as handle:
            process = subprocess.Popen(
                _anchor_command(args, package, pair_index, outcome),
                cwd=args.mage_repo,
                env=_environment(database_root, package),
                stdout=handle,
                stderr=subprocess.STDOUT,
            )
            try:
                return_code = process.wait(timeout=args.task_timeout_seconds)
            except subprocess.TimeoutExpired:
                _terminate_process_tree(process)
                return_code = process.returncode
                failure = "task_timeout"
        if return_code != 0:
            failure = failure or f"exit_{return_code}"
        elif not outcome.is_file():
            failure = "missing_outcome_export"
        elif not _pair_marker_valid(log, args.base_seed, pair_index):
            failure = "invalid_pair_marker"
    except Exception as error:
        failure = f"{type(error).__name__}: {error}"
    result = {
        "worker": worker,
        "arm": arm,
        "pair_index": pair_index,
        "status": "success" if failure is None else "failed",
        "failure": failure,
        "return_code": return_code,
        "elapsed_seconds": time.perf_counter() - started,
        "log": str(log),
        "log_sha256": _sha256(log) if log.is_file() else None,
        "outcome": str(outcome),
        "outcome_sha256": _sha256(outcome) if outcome.is_file() else None,
    }
    return result


def _prepare_workers(args: argparse.Namespace) -> list[Path]:
    if _sha256(args.source_database) != CARD_DB_SHA256:
        _fail("source card database SHA-256 mismatch")
    roots: list[Path] = []
    for worker in range(args.workers):
        root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True)
        destination = root / "cards.h2.mv.db"
        shutil.copyfile(args.source_database, destination)
        if _sha256(destination) != CARD_DB_SHA256:
            _fail(f"worker {worker} card database copy mismatch")
        roots.append(root)
    return roots


def _run_wave(
    args: argparse.Namespace,
    worker_roots: list[Path],
    packages: dict[str, dict[str, Any]],
    pair_indices: list[int],
) -> list[dict[str, Any]]:
    tasks = [
        (arm, packages[arm], pair_index)
        for pair_index in pair_indices
        for arm in ("gae8", "gae16")
    ]
    assignments = [tasks[worker :: args.workers] for worker in range(args.workers)]

    def worker_run(worker: int) -> list[dict[str, Any]]:
        return [
            _run_task(args, worker, worker_roots[worker], arm, package, pair_index)
            for arm, package, pair_index in assignments[worker]
        ]

    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = [executor.submit(worker_run, worker) for worker in range(args.workers)]
        for future in concurrent.futures.as_completed(futures):
            results.extend(future.result())
    return sorted(results, key=lambda row: (row["pair_index"], row["arm"]))


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
        _fail(f"nvidia-smi sample failed: {completed.stderr.strip()}")
    for line in completed.stdout.splitlines():
        fields = [field.strip() for field in line.split(",")]
        if len(fields) == 4 and int(fields[0]) == gpu_ordinal:
            return {
                "gpu_utilization_percent": float(fields[1]),
                "gpu_memory_used_mib": float(fields[2]),
                "gpu_memory_total_mib": float(fields[3]),
            }
    _fail(f"nvidia-smi did not return GPU {gpu_ordinal}")


def _monitor(stop: threading.Event, samples: list[dict[str, float]]) -> None:
    import psutil

    process = psutil.Process(os.getpid())
    psutil.cpu_percent(interval=None)
    while not stop.wait(1.0):
        rss = 0
        for child in [process, *process.children(recursive=True)]:
            try:
                rss += child.memory_info().rss
            except (psutil.NoSuchProcess, psutil.AccessDenied):
                pass
        sample = {
            "system_cpu_percent": float(psutil.cpu_percent(interval=None)),
            "system_memory_available_bytes": float(psutil.virtual_memory().available),
            "process_tree_rss_bytes": float(rss),
        }
        sample.update(_gpu_sample(1))
        samples.append(sample)


def _monitor_summary(samples: list[dict[str, float]]) -> dict[str, Any]:
    if not samples:
        _fail("resource monitor produced no samples")

    def summary(field: str) -> dict[str, float]:
        values = [sample[field] for sample in samples]
        return {"average": sum(values) / len(values), "maximum": max(values)}

    available = [sample["system_memory_available_bytes"] for sample in samples]
    return {
        "sample_count": len(samples),
        "system_cpu_percent": summary("system_cpu_percent"),
        "process_tree_rss_bytes": summary("process_tree_rss_bytes"),
        "system_memory_available_bytes_minimum": min(available),
        "gpu_1_utilization_percent": summary("gpu_utilization_percent"),
        "gpu_1_memory_used_mib": summary("gpu_memory_used_mib"),
        "gpu_1_memory_total_mib": samples[0]["gpu_memory_total_mib"],
    }


def _expected_checkpoint(identity: dict[str, Any]) -> dict[str, Any]:
    return {
        "authority_kind": identity["authority_kind"],
        **SOURCE_IDENTITY,
        "loaded_run_sha256": SOURCE_IDENTITY["source_run_sha256"],
        "loaded_generation": identity["adam_step"],
        "loaded_checkpoint_sha256": identity["manifest_sha256"],
        "loaded_payload_sha256": identity["payload_sha256"],
        "loaded_train_state_sha256": identity["train_state_sha256"],
        "model_parameter_sha256": identity["model_parameter_sha256"],
        "environment_trajectory_contract": "environment-randomization-v2",
        "sampler_identity": "f32-q8-expq63-hamilton-splitmix64-v1",
        "sampler_contract_sha256": "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6",
    }


def _parse_outcome(
    path: Path,
    identity: dict[str, Any],
    base_seed: int,
    pair_index: int,
) -> dict[str, Any]:
    rows: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                _fail(f"{path}:{line_number}: invalid JSON: {error}")
            if not isinstance(row, dict):
                _fail(f"{path}:{line_number}: record is not an object")
            rows.append(row)
    headers = [row for row in rows if row.get("record_type") == "header"]
    if len(headers) != 1 or rows[0] is not headers[0]:
        _fail(f"{path}: expected one first-position header")
    expected_checkpoint = _expected_checkpoint(identity)
    header = headers[0]
    if (
        header.get("export_contract") != OUTCOME_CONTRACT
        or header.get("selection_source") != "candidate_checkpoint_policy"
        or header.get("checkpoint") != expected_checkpoint
    ):
        _fail(f"{path}: header identity mismatch")
    for ordinal, row in enumerate(rows):
        if row.get("record_ordinal") != ordinal:
            _fail(f"{path}: record ordinals are not contiguous")
        if row.get("checkpoint") not in (None, expected_checkpoint):
            _fail(f"{path}: checkpoint identity changed within stream")
    terminals = [row for row in rows if row.get("record_type") == "terminal"]
    if len(terminals) != 2:
        _fail(f"{path}: expected exactly two terminals")
    results: dict[str, Any] = {}
    environment_seed = None
    for row in terminals:
        seat = row.get("candidate_seat")
        terminal = row.get("terminal")
        reward = row.get("candidate_terminal_reward")
        if (
            seat not in {"p0", "p1"}
            or not isinstance(terminal, dict)
            or terminal.get("terminal_classification") != "natural"
            or terminal.get("terminal_code") != "natural_game_over"
            or reward not in {-1, 0, 1}
            or row.get("pair_index") != pair_index
            or row.get("episode_id") != pair_index * 2 + int(seat[1])
            or row.get("base_seed_u64_hex") != f"{base_seed:016x}"
        ):
            _fail(f"{path}: terminal contract mismatch")
        rewards = terminal.get("terminal_reward")
        if not isinstance(rewards, list) or len(rewards) != 2 or rewards[int(seat[1])] != reward:
            _fail(f"{path}: terminal reward mismatch")
        seed = row.get("pair_environment_seed_u64_hex")
        if environment_seed is None:
            environment_seed = seed
        elif environment_seed != seed:
            _fail(f"{path}: pair environment seed changed between seats")
        results[seat] = {
            "reward": reward,
            "outcome": "win" if reward == 1 else "draw" if reward == 0 else "loss",
        }
    if set(results) != {"p0", "p1"}:
        _fail(f"{path}: candidate seat coverage mismatch")
    return {"environment_seed_u64_hex": environment_seed, "seats": results}


def _adjudicate(
    args: argparse.Namespace,
    packages: dict[str, dict[str, Any]],
    accepted: list[int],
    by_task: dict[tuple[int, str], dict[str, Any]],
) -> dict[str, Any]:
    arm_counts = {
        arm: {"win": 0, "draw": 0, "loss": 0, "by_seat": {"p0": {"win": 0, "draw": 0, "loss": 0}, "p1": {"win": 0, "draw": 0, "loss": 0}}}
        for arm in packages
    }
    paired = {
        "gae16_better": 0,
        "gae8_better": 0,
        "tied": 0,
        "by_seat": {"p0": {"gae16_better": 0, "gae8_better": 0, "tied": 0}, "p1": {"gae16_better": 0, "gae8_better": 0, "tied": 0}},
    }
    matched_pairs: list[dict[str, Any]] = []
    for pair_index in accepted:
        parsed = {
            arm: _parse_outcome(
                Path(by_task[(pair_index, arm)]["outcome"]),
                packages[arm],
                args.base_seed,
                pair_index,
            )
            for arm in packages
        }
        if parsed["gae8"]["environment_seed_u64_hex"] != parsed["gae16"]["environment_seed_u64_hex"]:
            _fail(f"pair {pair_index}: arms received different environment seeds")
        for seat in ("p0", "p1"):
            rewards = {arm: parsed[arm]["seats"][seat]["reward"] for arm in packages}
            for arm, reward in rewards.items():
                label = "win" if reward == 1 else "draw" if reward == 0 else "loss"
                arm_counts[arm][label] += 1
                arm_counts[arm]["by_seat"][seat][label] += 1
            comparison = (
                "gae16_better"
                if rewards["gae16"] > rewards["gae8"]
                else "gae8_better"
                if rewards["gae8"] > rewards["gae16"]
                else "tied"
            )
            paired[comparison] += 1
            paired["by_seat"][seat][comparison] += 1
        matched_pairs.append(
            {
                "pair_index": pair_index,
                "environment_seed_u64_hex": parsed["gae8"]["environment_seed_u64_hex"],
                "outcome_sha256": {
                    arm: by_task[(pair_index, arm)]["outcome_sha256"] for arm in packages
                },
            }
        )
    net = paired["gae16_better"] - paired["gae8_better"]
    seat_net = {
        seat: paired["by_seat"][seat]["gae16_better"]
        - paired["by_seat"][seat]["gae8_better"]
        for seat in ("p0", "p1")
    }
    select_gae16 = net >= 4 and all(value >= -2 for value in seat_net.values())
    return {
        "outcomes_hidden_until_mutual_completion": True,
        "matched_pairs": matched_pairs,
        "arms": arm_counts,
        "paired_terminal_order": paired,
        "gae16_net": net,
        "gae16_seat_net": seat_net,
        "ranking_gate": {
            "minimum_net": 4,
            "minimum_each_seat_net": -2,
            "pass": select_gae16,
            "current_external_anchor_candidate": "gae16" if select_gae16 else "gae8",
        },
    }


def run(args: argparse.Namespace) -> dict[str, Any]:
    if args.evidence_root.exists():
        unexpected = [path.name for path in args.evidence_root.iterdir() if path.name != "manifest.json"]
        if unexpected:
            _fail("evidence root may initially contain only manifest.json")
    else:
        args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    packages = {"gae8": _load_package(args.gae8_root), "gae16": _load_package(args.gae16_root)}
    worker_roots = _prepare_workers(args)
    samples: list[dict[str, float]] = []
    stop = threading.Event()
    monitor = threading.Thread(target=_monitor, args=(stop, samples), daemon=True)
    monitor.start()
    started = time.perf_counter()
    results: list[dict[str, Any]] = []
    accepted: list[int] = []
    excluded: list[int] = []
    next_pair = args.pair_start
    try:
        while len(accepted) < args.target_pairs and next_pair < args.pair_start + args.max_pairs:
            needed = args.target_pairs - len(accepted)
            available = args.pair_start + args.max_pairs - next_pair
            pair_indices = list(range(next_pair, next_pair + min(needed, available)))
            next_pair += len(pair_indices)
            wave = _run_wave(args, worker_roots, packages, pair_indices)
            results.extend(wave)
            by_task = {(row["pair_index"], row["arm"]): row for row in results}
            for pair_index in pair_indices:
                if all(by_task[(pair_index, arm)]["status"] == "success" for arm in packages):
                    accepted.append(pair_index)
                else:
                    excluded.append(pair_index)
            _atomic_json(
                args.evidence_root / "state.json",
                {
                    "schema": SCHEMA,
                    "status": "running",
                    "outcomes_parsed": False,
                    "accepted_pairs": accepted,
                    "excluded_pairs": excluded,
                    "tasks": sorted(results, key=lambda row: (row["pair_index"], row["arm"])),
                },
            )
        if len(accepted) != args.target_pairs:
            _fail(f"only {len(accepted)} mutually valid pairs within {args.max_pairs} attempts")
    finally:
        stop.set()
        monitor.join(timeout=20)
    elapsed = time.perf_counter() - started
    by_task = {(row["pair_index"], row["arm"]): row for row in results}
    adjudication = _adjudicate(args, packages, accepted, by_task)
    report = {
        "schema": SCHEMA,
        "status": "complete",
        "base_seed": args.base_seed,
        "pair_start": args.pair_start,
        "target_pairs": args.target_pairs,
        "max_pairs": args.max_pairs,
        "accepted_pairs": accepted,
        "excluded_pairs": excluded,
        "games_per_arm": args.target_pairs * 2,
        "total_games": args.target_pairs * 4,
        "workers": args.workers,
        "elapsed_seconds": elapsed,
        "games_per_second": (args.target_pairs * 4) / elapsed,
        "projected_256_game_wall_seconds": 256 / ((args.target_pairs * 4) / elapsed),
        "resource_usage": _monitor_summary(samples),
        "packages": packages,
        "inputs": {
            "scorer_exe": str(args.scorer_exe),
            "scorer_sha256": _sha256(args.scorer_exe),
            "mage_repo": str(args.mage_repo),
            "mage_commit": _version(["git", "rev-parse", "HEAD"], args.mage_repo),
            "source_database": str(args.source_database),
            "source_database_sha256": _sha256(args.source_database),
            "gpu_ordinal_reserved": 1,
            "workload_device": "cpu",
        },
        "toolchain": {
            "python": sys.version.split()[0],
            "java": _version(["java", "-version"]),
            "maven": _version([str(args.maven), "--version"]),
            "rustc": _version(["rustc", "--version"]),
            "cargo": _version(["cargo", "--version"]),
        },
        "runner_sha256": _sha256(Path(__file__).resolve()),
        "tasks": sorted(results, key=lambda row: (row["pair_index"], row["arm"])),
        **adjudication,
        "non_claims": [
            "external software anchor is not professional-level evidence",
            "terminal win/loss/draw is the only playing-strength outcome",
        ],
    }
    _atomic_json(args.evidence_root / "report.json", report)
    state = json.loads((args.evidence_root / "state.json").read_text(encoding="utf-8"))
    state.update(
        {
            "status": "complete",
            "outcomes_parsed": True,
            "report_sha256": _sha256(args.evidence_root / "report.json"),
        }
    )
    _atomic_json(args.evidence_root / "state.json.next", state)
    os.replace(args.evidence_root / "state.json.next", args.evidence_root / "state.json")
    return report


def _self_test() -> int:
    marker = dict(
        PAIR_FIELDS.findall(
            "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=7 episodes=2,3 pair_index=1 "
            "environment_seed=abc candidate_seats=p0,p1 winners=p1,p0"
        )
    )
    if marker.get("episodes") != "2,3" or marker.get("candidate_seats") != "p0,p1":
        _fail("pair parser self-test failed")
    if _expected_checkpoint(
        {
            "authority_kind": "a",
            "adam_step": 1,
            "manifest_sha256": "b",
            "payload_sha256": "c",
            "train_state_sha256": "d",
            "model_parameter_sha256": "e",
        }
    )["source_generation"] != 384:
        _fail("checkpoint lineage self-test failed")
    print("run_anchor_v1: SELF-TEST PASS")
    return 0


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--gae8-root", type=Path)
    parser.add_argument("--gae16-root", type=Path)
    parser.add_argument("--scorer-exe", type=Path)
    parser.add_argument("--mage-repo", type=Path)
    parser.add_argument("--source-database", type=Path)
    parser.add_argument("--maven", type=Path)
    parser.add_argument("--base-seed", type=int)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--target-pairs", type=int, default=1)
    parser.add_argument("--max-pairs", type=int, default=1)
    parser.add_argument("--workers", type=int, choices=(1, 2, 4), default=2)
    parser.add_argument("--task-timeout-seconds", type=int, default=1800)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.self_test:
        return _self_test()
    required = (
        "evidence_root",
        "gae8_root",
        "gae16_root",
        "scorer_exe",
        "mage_repo",
        "source_database",
        "maven",
        "base_seed",
    )
    if any(getattr(args, name) is None for name in required):
        _fail("all run path and seed arguments are required")
    if not (0 <= args.pair_start and 1 <= args.target_pairs <= args.max_pairs <= 72):
        _fail("require nonnegative pair start and 1 <= target pairs <= max pairs <= 72")
    if args.task_timeout_seconds < 60:
        _fail("task timeout must be at least 60 seconds")
    for name in ("gae8_root", "gae16_root", "scorer_exe", "mage_repo", "source_database", "maven"):
        setattr(args, name, getattr(args, name).resolve(strict=True))
    args.evidence_root = args.evidence_root.resolve()
    report = run(args)
    print(json.dumps(report, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"run_anchor_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
