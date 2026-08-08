#!/usr/bin/env python3
"""Acquire candidate-02 games and delegate all scoring to the independent analyzer."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

from run_anchor_read import checkpoint_slot
from run_payoff_evaluation import (
    arm_spec,
    file_record,
    git_record,
    load_json,
    run_batch,
    sha256_file,
    toolchain_record,
    unique_attempt_root,
    validate_outcome,
)


SPEC_SCHEMA = "scaled-selfplay-candidate-02-v3-spec/v1"
COUNTERSIGN_SCHEMA = "scaled-selfplay-candidate-02-v3-countersign/v2"
SCREEN_SCHEMA = "scaled-selfplay-candidate-02-v3-screen/v2"
PLAN_SCHEMA = "scaled-selfplay-candidate-02-v3-plan/v2"
RECEIPT_SCHEMA = "scaled-selfplay-candidate-02-v3-chunk-receipt/v1"
ANALYSIS_SCHEMA = "scaled-selfplay-candidate-02-v3-analysis/v2"
MANIFEST_SCHEMA = "scaled-selfplay-candidate-02-v3-execution/v2"
SCREEN_MIN_SPEEDUP = 1.5


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def canonical_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def write_new_json(path: Path, value: dict[str, Any]) -> None:
    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()
        os.fsync(stream.fileno())


def durable_file(path: Path) -> None:
    with path.open("rb+") as stream:
        stream.flush()
        os.fsync(stream.fileno())


def load_analyzer(path: Path):
    module_spec = importlib.util.spec_from_file_location("candidate_02_v3_analyzer_contract", path)
    require(module_spec is not None and module_spec.loader is not None, "cannot load analyzer contract")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_spec.name] = module
    module_spec.loader.exec_module(module)
    return module


def analyzer_path() -> Path:
    return Path(__file__).with_name("candidate_02_v3_analysis.py").resolve()


def validate_spec(path: Path) -> tuple[dict[str, Any], Any]:
    analyzer = load_analyzer(analyzer_path())
    spec, reference = analyzer.validate_spec(path.resolve())
    require(spec.get("schema") == SPEC_SCHEMA, "unexpected candidate-02 spec schema")
    require(int(spec["concurrent_chunks"]) == 2, "candidate-02 concurrency geometry changed")
    return spec, reference


def slot_from_spec(binding: dict[str, Any]) -> dict[str, Any]:
    root = Path(binding["store_root"]).resolve()
    run = load_json(root / "run.json")
    seed = int(run["schedule"]["base_seed"])
    slot = checkpoint_slot(root, seed, int(binding["generation"]), binding["role"])
    checkpoint = load_json(root / "checkpoints" / f"update-{int(binding['generation']):08d}.checkpoint.json")
    require(checkpoint["identity_bundle_sha256"] == binding["identity_bundle_sha256"], f"{binding['role']} identity bundle mismatch")
    slot["identity_bundle_sha256"] = checkpoint["identity_bundle_sha256"]
    for field in ("source_run_sha256", "checkpoint_sha256", "sidecar_sha256", "state_sha256", "model_parameter_sha256"):
        spec_field = "run_sha256" if field == "source_run_sha256" else field
        require(slot[field] == binding[spec_field], f"{binding['role']} {spec_field} mismatch")
    return slot


def exclusive_gpu1_snapshot() -> dict[str, Any]:
    gpu_query = subprocess.run(
        ["nvidia-smi", "--query-gpu=index,uuid,name", "--format=csv,noheader,nounits"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    rows = []
    for line in gpu_query.stdout.splitlines():
        if line.strip():
            parts = [part.strip() for part in line.split(",", 2)]
            require(len(parts) == 3, "unexpected nvidia-smi GPU row")
            rows.append({"index": int(parts[0]), "uuid": parts[1], "name": parts[2]})
    gpu1 = [row for row in rows if row["index"] == 1]
    require(len(gpu1) == 1, "GPU ordinal 1 is unavailable")
    apps_query = subprocess.run(
        ["nvidia-smi", "--query-compute-apps=gpu_uuid,pid,process_name,used_memory", "--format=csv,noheader,nounits"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    apps = []
    for line in apps_query.stdout.splitlines():
        if line.strip():
            parts = [part.strip() for part in line.split(",", 3)]
            require(len(parts) == 4, "unexpected nvidia-smi compute-app row")
            apps.append({"gpu_uuid": parts[0], "pid": int(parts[1]), "process_name": parts[2], "used_memory_mib": parts[3]})
    gpu1_apps = [app for app in apps if app["gpu_uuid"] == gpu1[0]["uuid"]]
    require(not gpu1_apps, f"GPU 1 formal window is not exclusive: {gpu1_apps}")
    return {"gpu": gpu1[0], "compute_apps": gpu1_apps, "all_gpus": rows}


def chunk_evaluation_seeds(spec: dict[str, Any], mode: str) -> list[int]:
    count = int(spec["screen"]["chunk_count"]) if mode == "screen" else int(spec["max_N"]) // int(spec["chunk_pair_count"])
    first = int(spec[mode]["first_evaluation_seed"])
    stride = int(spec["evaluation_seed_stride"])
    return [first + index * stride for index in range(count)]


def mode_pair_count(spec: dict[str, Any], mode: str) -> int:
    return int(spec["screen"]["pair_count_per_chunk"]) if mode == "screen" else int(spec["chunk_pair_count"])


def mode_max_n(spec: dict[str, Any], mode: str) -> int:
    return len(chunk_evaluation_seeds(spec, mode)) * mode_pair_count(spec, mode)


def context(args: argparse.Namespace, spec: dict[str, Any]) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    repo_root = args.repo_root.resolve()
    executable = Path(spec["executable"]["path"]).resolve()
    require(executable.is_file(), "candidate-02 executable is missing")
    require(sha256_file(executable) == spec["executable"]["sha256"], "candidate-02 executable hash mismatch")
    slots = [slot_from_spec(spec["candidate"]), slot_from_spec(spec["opponent_and_control"])]
    require(slots[0]["model_parameter_sha256"] != slots[1]["model_parameter_sha256"], "candidate and control model hashes are not distinct")
    return (
        {
            "git": git_record(repo_root, spec["executable"]["source_commit"]),
            "toolchain": toolchain_record(repo_root),
            "executable": {**file_record(executable), "source_commit": spec["executable"]["source_commit"]},
            "spec": file_record(args.spec.resolve()),
            "candidate": spec["candidate"],
            "opponent_and_control": spec["opponent_and_control"],
            "gpu_ordinal": "exclusive headless GPU 1 window; evaluator is CPU-resident",
            "terminal_reward_only": True,
        },
        slots,
    )


def chunk_plan(spec: dict[str, Any], mode: str) -> list[dict[str, int]]:
    pair_count = mode_pair_count(spec, mode)
    return [
        {
            "chunk_index": index,
            "evaluation_seed": seed,
            "global_cluster_start": index * pair_count,
            "global_cluster_end_exclusive": (index + 1) * pair_count,
        }
        for index, seed in enumerate(chunk_evaluation_seeds(spec, mode))
    ]


def build_plan(
    spec: dict[str, Any],
    mode: str,
    run_context: dict[str, Any],
    screen: dict[str, Any] | None,
    countersign: dict[str, Any] | None,
    initial_verification: dict[str, Any] | None,
) -> dict[str, Any]:
    gate = {key: spec[key] for key in ("gate_class", "alpha", "c", "delta_promote", "delta_worthwhile", "max_N", "chunk_pair_count", "concurrent_chunks", "conditional_mean_stability")}
    gate["max_N"] = mode_max_n(spec, mode)
    gate["chunk_pair_count"] = mode_pair_count(spec, mode)
    return {
        "schema": PLAN_SCHEMA,
        "mode": mode,
        "gate_id": spec[mode]["gate_id"],
        **run_context,
        "screen": screen,
        "countersign": countersign,
        "initial_verification": initial_verification,
        "gate": gate,
        "joint_component_law": spec["joint_component_law"],
        "pre_outcome_schedule_sha256": spec[mode]["pre_outcome_schedule_sha256"],
        "first_evaluation_seed": spec[mode]["first_evaluation_seed"],
        "evaluation_seed_stride": spec["evaluation_seed_stride"],
        "arm_order": "candidate and control are acquired together; inference follows ascending frozen cluster index",
        "expected_rally_deck_hashes_u64": spec["expected_rally_deck_hashes_u64"],
        "chunk_plan": chunk_plan(spec, mode),
    }


def normalize_arm_record(record: dict[str, Any]) -> dict[str, Any]:
    normalized = {key: record[key] for key in ("label", "candidate_index", "opponent_index", "pair_count", "evaluation_seed", "exit_code", "wall_seconds", "stdout", "stderr", "outcome")}
    for file_key in ("stdout", "stderr", "outcome"):
        path = Path(normalized[file_key]["path"])
        durable_file(path)
        normalized[file_key] = file_record(path)
    return normalized


def acquire_chunk_batch(
    executable: Path,
    repo_root: Path,
    root: Path,
    slots: list[dict[str, Any]],
    spec: dict[str, Any],
    mode: str,
    chunk_indexes: list[int],
    concurrency: int,
) -> tuple[list[dict[str, Any]], float]:
    seeds = chunk_evaluation_seeds(spec, mode)
    arm_specs: list[dict[str, Any]] = []
    for chunk_index in chunk_indexes:
        evaluation_seed = seeds[chunk_index]
        arm_specs.extend(
            [
                arm_spec(f"chunk-{chunk_index:03d}-candidate", 0, 1, mode_pair_count(spec, mode), evaluation_seed),
                arm_spec(f"chunk-{chunk_index:03d}-control", 1, 1, mode_pair_count(spec, mode), evaluation_seed),
            ]
        )
    batch, wall = run_batch(executable, repo_root, root, slots, arm_specs, concurrency)
    normalized = []
    for arm, record in zip(arm_specs, batch, strict=True):
        validate_outcome(Path(record["outcome"]["path"]), arm, slots)
        normalized.append(normalize_arm_record(record))
    receipts = []
    for offset, chunk_index in enumerate(chunk_indexes):
        receipt = {
            "schema": RECEIPT_SCHEMA,
            "chunk_index": chunk_index,
            "evaluation_seed": seeds[chunk_index],
            "candidate_arm": normalized[offset * 2],
            "control_arm": normalized[offset * 2 + 1],
        }
        receipt_path = root / f"chunk-{chunk_index:03d}-receipt.json"
        write_new_json(receipt_path, receipt)
        receipts.append(file_record(receipt_path))
    return receipts, wall


def run_analyzer(
    root: Path,
    spec_path: Path,
    mode: str,
    output: Path,
    trajectory: str,
) -> dict[str, Any]:
    command = [
        sys.executable,
        str(analyzer_path()),
        "analyze",
        "--run-root",
        str(root),
        "--spec",
        str(spec_path),
        "--mode",
        mode,
        "--output",
        str(output),
        "--trajectory",
        trajectory,
    ]
    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    require(completed.returncode == 0, f"independent analyzer failed: {completed.stderr}")
    analysis = load_json(output)
    require(analysis.get("schema") == ANALYSIS_SCHEMA, "independent analyzer returned wrong schema")
    return analysis


def validate_screen(path: Path, run_context: dict[str, Any]) -> dict[str, Any]:
    screen_manifest = load_json(path)
    require(screen_manifest.get("schema") == SCREEN_SCHEMA, "unexpected candidate-02 screen schema")
    require(screen_manifest.get("passed") is True, "candidate-02 screen did not pass")
    require(screen_manifest.get("raw_outcomes_bit_identical") is True, "screen raw outcomes were not bit-identical")
    require(screen_manifest.get("analysis_core_bit_identical") is True, "screen analysis cores were not bit-identical")
    require(int(screen_manifest["selected_process_count"]) == 4, "formal candidate-02 requires four evaluator processes")
    require(screen_manifest["executable"]["sha256"] == run_context["executable"]["sha256"], "screen executable differs from formal context")
    require(screen_manifest["spec"]["sha256"] == run_context["spec"]["sha256"], "screen spec differs")
    return screen_manifest


def validate_countersign(path: Path, run_context: dict[str, Any], screen_path: Path) -> dict[str, Any]:
    countersign = load_json(path)
    require(countersign.get("schema") == COUNTERSIGN_SCHEMA, "unexpected countersign schema")
    require(countersign.get("decision") == "COUNTERSIGN", "candidate-02 sheet is not countersigned")
    require(countersign.get("spec_sha256") == run_context["spec"]["sha256"], "countersign spec mismatch")
    require(countersign.get("screen_sha256") == sha256_file(screen_path), "countersign screen mismatch")
    require(countersign.get("implementation_commit") == run_context["git"]["commit"], "implementation changed after countersign")
    require(countersign.get("runner_sha256") == sha256_file(Path(__file__)), "countersigned runner changed")
    require(countersign.get("analyzer_sha256") == sha256_file(analyzer_path()), "countersigned analyzer changed")
    design = Path(countersign["design_path"]).resolve()
    require(design.is_file() and sha256_file(design) == countersign["design_sha256"], "countersigned design mismatch")
    return countersign


def screen(args: argparse.Namespace, spec: dict[str, Any]) -> Path:
    run_context, slots = context(args, spec)
    executable = Path(spec["executable"]["path"]).resolve()
    root = unique_attempt_root(args.evidence_root.resolve(), "candidate-02-v3-throughput-screen")
    topology_records: dict[str, Any] = {}
    gpu_snapshots: list[dict[str, Any]] = []
    for process_count in (1, 2, 4):
        gpu_snapshots.append(exclusive_gpu1_snapshot())
        topology_root = root / f"processes-{process_count}"
        topology_root.mkdir()
        plan = build_plan(spec, "screen", run_context, None, None, None)
        plan_path = topology_root / "gate-plan.json"
        write_new_json(plan_path, plan)
        _, wall = acquire_chunk_batch(
            executable,
            args.repo_root.resolve(),
            topology_root,
            slots,
            spec,
            "screen",
            list(range(int(spec["screen"]["chunk_count"]))),
            process_count,
        )
        analysis_path = topology_root / "analysis.json"
        analysis = run_analyzer(topology_root, args.spec.resolve(), "screen", analysis_path, "full")
        receipts = [load_json(topology_root / f"chunk-{index:03d}-receipt.json") for index in range(int(spec["screen"]["chunk_count"]))]
        raw_hashes = [receipt[arm]["outcome"]["sha256"] for receipt in receipts for arm in ("candidate_arm", "control_arm")]
        topology_records[str(process_count)] = {
            "wall_seconds": wall,
            "plan": file_record(plan_path),
            "analysis": file_record(analysis_path),
            "inferential_core_sha256": analysis["inferential_core_sha256"],
            "raw_outcome_sha256s": raw_hashes,
            "receipts": [file_record(topology_root / f"chunk-{index:03d}-receipt.json") for index in range(int(spec["screen"]["chunk_count"]))],
        }
    baseline = topology_records["1"]
    raw_exact = all(record["raw_outcome_sha256s"] == baseline["raw_outcome_sha256s"] for record in topology_records.values())
    core_exact = all(record["inferential_core_sha256"] == baseline["inferential_core_sha256"] for record in topology_records.values())
    baseline_wall = baseline["wall_seconds"]
    speedups = {key: baseline_wall / record["wall_seconds"] for key, record in topology_records.items()}
    passed = raw_exact and core_exact and speedups["4"] >= SCREEN_MIN_SPEEDUP
    manifest = {
        "schema": SCREEN_SCHEMA,
        "passed": passed,
        **run_context,
        "screen_mode": "candidate/control two-chunk mini-gate under the revealed screen schedule",
        "screen_chunk_count": spec["screen"]["chunk_count"],
        "pair_count_per_chunk": spec["screen"]["pair_count_per_chunk"],
        "minimum_four_process_speedup": SCREEN_MIN_SPEEDUP,
        "topologies": topology_records,
        "aggregate_speedups": speedups,
        "raw_outcomes_bit_identical": raw_exact,
        "analysis_core_bit_identical": core_exact,
        "selected_process_count": 4 if passed else 1,
        "gpu_window_snapshots": gpu_snapshots,
        "runner": file_record(Path(__file__)),
        "analyzer": file_record(analyzer_path()),
    }
    manifest_path = root / "screen-manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    return manifest_path


def verify_initial_for_confirmation(initial_manifest_path: Path, spec_path: Path, output: Path) -> dict[str, Any]:
    initial_manifest = load_json(initial_manifest_path)
    require(initial_manifest.get("schema") == MANIFEST_SCHEMA and initial_manifest.get("mode") == "initial", "initial manifest shape is invalid")
    retained_analysis = Path(initial_manifest["analysis"]["path"]).resolve()
    command = [
        sys.executable,
        str(analyzer_path()),
        "verify-existing",
        "--run-root",
        str(initial_manifest_path.resolve().parent),
        "--spec",
        str(spec_path.resolve()),
        "--retained-analysis",
        str(retained_analysis),
        "--output",
        str(output.resolve()),
    ]
    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    require(completed.returncode == 0, f"independent initial verification failed: {completed.stderr}")
    verification = load_json(output)
    require(verification.get("decision") == "VERIFIED-SUCCESS", "initial verification did not authorize confirmation")
    return verification


def formal_run(args: argparse.Namespace, spec: dict[str, Any]) -> Path:
    mode = args.mode
    run_context, slots = context(args, spec)
    validate_screen(args.screen_manifest, run_context)
    validate_countersign(args.countersign, run_context, args.screen_manifest)
    root = unique_attempt_root(args.evidence_root.resolve(), spec[mode]["gate_id"])
    initial_verification_record = None
    if mode == "confirmation":
        require(args.initial_manifest is not None, "confirmation requires the initial manifest")
        verification_path = root / "initial-independent-verification.json"
        verify_initial_for_confirmation(args.initial_manifest, args.spec, verification_path)
        initial_verification_record = file_record(verification_path)
    plan = build_plan(
        spec,
        mode,
        run_context,
        file_record(args.screen_manifest),
        file_record(args.countersign),
        initial_verification_record,
    )
    plan_path = root / "gate-plan.json"
    write_new_json(plan_path, plan)
    executable = Path(spec["executable"]["path"]).resolve()
    chunk_seeds = chunk_evaluation_seeds(spec, mode)
    concurrent_chunks = int(spec["concurrent_chunks"])
    gpu_window_snapshots = [exclusive_gpu1_snapshot()]
    look_records: list[dict[str, Any]] = []
    started = time.perf_counter()
    for wave_start in range(0, len(chunk_seeds), concurrent_chunks):
        gpu_window_snapshots.append(exclusive_gpu1_snapshot())
        chunk_indexes = list(range(wave_start, min(wave_start + concurrent_chunks, len(chunk_seeds))))
        acquire_chunk_batch(executable, args.repo_root.resolve(), root, slots, spec, mode, chunk_indexes, 4)
        acquired_n = (chunk_indexes[-1] + 1) * mode_pair_count(spec, mode)
        look_path = root / f"analysis-look-{acquired_n:06d}.json"
        look = run_analyzer(root, args.spec.resolve(), mode, look_path, "endpoint")
        look_records.append(file_record(look_path))
        if look["decision"] != "CONTINUE":
            break
    require(look_records, "formal run produced no independently analyzed look")
    final_analysis_path = root / "analysis.json"
    final_analysis = run_analyzer(root, args.spec.resolve(), mode, final_analysis_path, "full")
    require(final_analysis["decision"] != "CONTINUE", "formal acquisition ended before a terminal gate decision")
    gpu_window_snapshots.append(exclusive_gpu1_snapshot())
    wall_seconds = time.perf_counter() - started
    total_games = 4 * int(final_analysis["acquired_N"])
    receipt_records = [file_record(path) for path in sorted(root.glob("chunk-*-receipt.json"))]
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "passed": True,
        "mode": mode,
        "disposition": final_analysis["decision"],
        "plan": file_record(plan_path),
        "spec": run_context["spec"],
        "screen": file_record(args.screen_manifest),
        "countersign": file_record(args.countersign),
        "initial_verification": initial_verification_record,
        "analysis": file_record(final_analysis_path),
        "analysis_summary": {key: final_analysis[key] for key in ("decision", "decision_N", "acquired_N", "delta_hat", "cs_delta_lower", "cs_delta_upper", "acquired_stream_sha256", "decision_prefix_stream_sha256")},
        "analysis_looks": look_records,
        "chunk_receipts": receipt_records,
        "wall_seconds": wall_seconds,
        "total_game_count": total_games,
        "aggregate_games_per_second": total_games / wall_seconds,
        "gpu_window_snapshots": gpu_window_snapshots,
        "terminal_reward_only": True,
    }
    manifest_path = root / "gate-execution-manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    return manifest_path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--spec", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("screen")
    run = subparsers.add_parser("run")
    run.add_argument("--mode", choices=("initial", "confirmation"), required=True)
    run.add_argument("--screen-manifest", required=True, type=Path)
    run.add_argument("--countersign", required=True, type=Path)
    run.add_argument("--initial-manifest", type=Path)
    args = parser.parse_args()
    spec, _ = validate_spec(args.spec.resolve())
    if args.command == "screen":
        screen(args, spec)
    else:
        formal_run(args, spec)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
