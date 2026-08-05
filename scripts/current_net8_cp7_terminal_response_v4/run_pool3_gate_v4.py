#!/usr/bin/env python3
"""Run the ordered revealed bridge or fresh Pool3 V4 terminal gate."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
from pathlib import Path
import sys
import time
from types import SimpleNamespace
from typing import Any


COLLECTOR_DIR = Path(__file__).resolve().parents[1] / "native_population_structured_v1"
sys.path.insert(0, str(COLLECTOR_DIR))
import collect_corpus_v1 as collector  # noqa: E402


MANIFEST_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-pool3-manifest/v1"
REPORT_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-pool3-result/v1"
STATE_SCHEMA = "mtg-kernel-current-net8-cp7-terminal-response-v4-pool3-state/v1"
OUTCOME_CONTRACT = "mtg-kernel-native-population-outcome-jsonl/v1"
TEACHER_CONTRACT = "mtg-kernel-native-population-opponent-jsonl/v1"
SELECTION_SOURCE = "candidate_checkpoint_policy"
TEACHER_SELECTION_SOURCE = "native_pool3_ladder_40_20_20_20"
POOL_SHA256 = "6c3c8ff09ab519dc9f462b41cbf898da902d230656d14e64d79fc66a19f3bc71"
BRIDGE_PANEL = {
    "base_seed": 1_650_001,
    "pair_start": 0,
    "pairs": 1,
    "episodes_per_arm": 2,
    "revealed": True,
}
FORMAL_PANEL = {
    "base_seed": 1_830_001,
    "pair_start": 0,
    "pairs": 1_024,
    "episodes_per_arm": 2_048,
    "revealed": False,
}
FORMAL_GATES = {
    "overall_terminal_order_net_floor": -16,
    "p0_terminal_order_net_floor": -12,
    "p1_terminal_order_net_floor": -12,
}
COMMON_CHECKPOINT = {
    "source_run_sha256": "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
    "source_generation": 384,
    "source_checkpoint_sha256": "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
    "source_sidecar_sha256": "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb",
    "source_payload_sha256": "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
    "source_train_state_sha256": "fc471f85d28293d72b42dc61de628859173bd67426e251a51bfbbe86c7d586d8",
    "loaded_run_sha256": "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
    "environment_trajectory_contract": "environment-randomization-v2",
    "sampler_identity": "f32-q8-expq63-hamilton-splitmix64-v1",
    "sampler_contract_sha256": "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6",
}
ARM_ORDER = ("candidate", "baseline")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not a JSON object")
    return value


def write_new_json(path: Path, value: dict[str, Any]) -> str:
    data = (json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_BINARY", 0)
    descriptor = os.open(path, flags)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        try:
            path.unlink()
        except FileNotFoundError:
            pass
        raise
    return hashlib.sha256(data).hexdigest()


def _path(value: Any, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ValueError(f"manifest {field} path is absent")
    path = Path(value)
    if not path.is_absolute():
        raise ValueError(f"manifest {field} path is not absolute")
    return path.resolve(strict=False)


def _validate_checkpoint(checkpoint: Any, expected: dict[str, Any], field: str) -> None:
    if not isinstance(checkpoint, dict) or checkpoint != expected:
        raise ValueError(f"{field} checkpoint identity mismatch")


def validate_manifest(path: Path) -> dict[str, Any]:
    manifest = load_json(path)
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValueError("Pool3 manifest schema mismatch")
    if not isinstance(manifest.get("git_commit"), str) or len(manifest["git_commit"]) != 40:
        raise ValueError("Pool3 manifest git commit is absent")
    toolchain = manifest.get("toolchain")
    if not isinstance(toolchain, dict) or not all(
        isinstance(toolchain.get(field), str) and toolchain[field]
        for field in ("rustc", "cargo", "linker")
    ):
        raise ValueError("Pool3 manifest toolchain is incomplete")
    if manifest.get("gpu_ordinal") != 1 or manifest.get("topology") != "two-concurrent-arms":
        raise ValueError("Pool3 manifest device or topology mismatch")
    mode = manifest.get("mode")
    if mode not in ("revealed-bridge", "formal-pool3"):
        raise ValueError("Pool3 manifest mode is invalid")
    expected_panel = BRIDGE_PANEL if mode == "revealed-bridge" else FORMAL_PANEL
    if manifest.get("panel") != expected_panel:
        raise ValueError("Pool3 manifest panel mismatch")
    expected_gates = None if mode == "revealed-bridge" else FORMAL_GATES
    if manifest.get("gates") != expected_gates:
        raise ValueError("Pool3 manifest gate mismatch")

    scorer = manifest.get("scorer")
    pool = manifest.get("pool")
    if not isinstance(scorer, dict) or not isinstance(pool, dict):
        raise ValueError("Pool3 manifest scorer or pool binding is absent")
    scorer_path = _path(scorer.get("path"), "scorer")
    pool_root = _path(pool.get("root"), "pool")
    if (
        not scorer_path.is_file()
        or scorer.get("sha256") != sha256(scorer_path)
        or not (pool_root / "pool.json").is_file()
        or pool.get("contract_sha256") != POOL_SHA256
        or sha256(pool_root / "pool.json") != POOL_SHA256
    ):
        raise ValueError("Pool3 scorer or pool evidence mismatch")

    arms = manifest.get("arms")
    if not isinstance(arms, dict) or set(arms) != set(ARM_ORDER):
        raise ValueError("Pool3 manifest arm inventory mismatch")
    normalized_arms: dict[str, dict[str, Any]] = {}
    for name in ARM_ORDER:
        record = arms[name]
        if not isinstance(record, dict) or set(record) != {"root", "checkpoint"}:
            raise ValueError(f"Pool3 manifest {name} arm binding is malformed")
        root = _path(record["root"], f"{name} root")
        if not root.is_dir():
            raise ValueError(f"Pool3 manifest {name} root is absent")
        checkpoint = record["checkpoint"]
        if not isinstance(checkpoint, dict) or set(checkpoint) != set(COMMON_CHECKPOINT) | {
            "authority_kind",
            "loaded_generation",
            "loaded_checkpoint_sha256",
            "loaded_payload_sha256",
            "loaded_train_state_sha256",
            "model_parameter_sha256",
        }:
            raise ValueError(f"Pool3 manifest {name} checkpoint binding is malformed")
        if any(checkpoint.get(field) != value for field, value in COMMON_CHECKPOINT.items()):
            raise ValueError(f"Pool3 manifest {name} common checkpoint binding mismatch")
        normalized_arms[name] = {"root": root, "checkpoint": checkpoint}

    outputs = manifest.get("outputs")
    if not isinstance(outputs, dict) or set(outputs) != {"evidence_root", "report_path"}:
        raise ValueError("Pool3 manifest output binding is malformed")
    evidence_root = _path(outputs["evidence_root"], "evidence root")
    report_path = _path(outputs["report_path"], "report")
    if report_path != evidence_root / "report.json" or path.resolve() != evidence_root / "manifest.json":
        raise ValueError("Pool3 manifest output paths are not canonical")

    prerequisites = manifest.get("prerequisites")
    normalized_prerequisites = None
    if mode == "revealed-bridge":
        if prerequisites is not None:
            raise ValueError("revealed bridge cannot claim prerequisites")
    else:
        if not isinstance(prerequisites, dict) or set(prerequisites) != {
            "bridge_report_path",
            "bridge_report_sha256",
        }:
            raise ValueError("formal Pool3 bridge prerequisite is absent")
        bridge_report = _path(prerequisites["bridge_report_path"], "bridge report")
        bridge = load_json(bridge_report)
        if (
            prerequisites["bridge_report_sha256"] != sha256(bridge_report)
            or bridge.get("schema") != REPORT_SCHEMA
            or bridge.get("mode") != "revealed-bridge"
            or bridge.get("status") != "pass"
        ):
            raise ValueError("formal Pool3 bridge prerequisite mismatch")
        normalized_prerequisites = {
            "bridge_report_path": str(bridge_report),
            "bridge_report_sha256": prerequisites["bridge_report_sha256"],
        }
    return {
        "manifest": manifest,
        "mode": mode,
        "panel": expected_panel,
        "gates": expected_gates,
        "scorer": scorer_path,
        "pool_root": pool_root,
        "arms": normalized_arms,
        "evidence_root": evidence_root,
        "report_path": report_path,
        "prerequisites": normalized_prerequisites,
    }


def _collection_args(validated: dict[str, Any], arm: str) -> SimpleNamespace:
    stream_root = validated["evidence_root"] / "streams" / arm
    return SimpleNamespace(
        scorer=validated["scorer"],
        candidate_root=validated["arms"][arm]["root"],
        pool_root=validated["pool_root"],
        python=None,
        teacher_jsonl=stream_root / "teacher.jsonl",
        outcome_jsonl=stream_root / "outcome.jsonl",
        output=stream_root / "collection.json",
        base_seed=validated["panel"]["base_seed"],
        pair_start=validated["panel"]["pair_start"],
        pairs=validated["panel"]["pairs"],
    )


def _validate_collection_report(args: SimpleNamespace, report: dict[str, Any]) -> None:
    if (
        report.get("schema") != collector.SCHEMA
        or report.get("base_seed") != args.base_seed
        or report.get("pair_start") != args.pair_start
        or report.get("pairs") != args.pairs
        or report.get("episodes") != args.pairs * 2
        or report.get("outcome_jsonl") != str(args.outcome_jsonl)
        or report.get("outcome_sha256") != sha256(args.outcome_jsonl)
        or report.get("teacher_jsonl") != str(args.teacher_jsonl)
        or report.get("teacher_sha256") != sha256(args.teacher_jsonl)
        or not isinstance(report.get("elapsed_seconds"), (int, float))
        or not math.isfinite(float(report["elapsed_seconds"]))
        or float(report["elapsed_seconds"]) <= 0
        or not isinstance(report.get("policy_steps"), int)
        or report["policy_steps"] <= 0
    ):
        raise ValueError("native Pool3 collection report mismatch")


def _validate_teacher_header(path: Path, expected_checkpoint: dict[str, Any]) -> None:
    with path.open("r", encoding="utf-8") as handle:
        header = json.loads(handle.readline())
    if (
        header.get("record_type") != "header"
        or header.get("export_contract") != TEACHER_CONTRACT
        or header.get("selection_source") != TEACHER_SELECTION_SOURCE
    ):
        raise ValueError(f"{path} teacher header mismatch")
    _validate_checkpoint(header.get("checkpoint"), expected_checkpoint, str(path))


def _load_terminals(
    path: Path,
    expected_checkpoint: dict[str, Any],
    base_seed: int,
    pair_start: int,
    pairs: int,
) -> tuple[dict[str, Any], dict[tuple[int, int, str], dict[str, Any]], int]:
    header: dict[str, Any] | None = None
    terminals: dict[tuple[int, int, str], dict[str, Any]] = {}
    decision_count = 0
    expected_ordinal = 0
    base_seed_hex = f"{base_seed:016x}"
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            row = json.loads(line)
            if row.get("record_ordinal") != expected_ordinal:
                raise ValueError(f"{path}:{line_number} record ordinal mismatch")
            expected_ordinal += 1
            record_type = row.get("record_type")
            if line_number == 1:
                if (
                    record_type != "header"
                    or row.get("export_contract") != OUTCOME_CONTRACT
                    or row.get("selection_source") != SELECTION_SOURCE
                ):
                    raise ValueError(f"{path} outcome header mismatch")
                _validate_checkpoint(row.get("checkpoint"), expected_checkpoint, str(path))
                header = row
                continue
            _validate_checkpoint(row.get("checkpoint"), expected_checkpoint, f"{path}:{line_number}")
            if record_type not in ("decision", "terminal"):
                raise ValueError(f"{path}:{line_number} record type mismatch")
            if row.get("base_seed_u64_hex") != base_seed_hex:
                raise ValueError(f"{path}:{line_number} base seed mismatch")
            pair_index = row.get("pair_index")
            if not isinstance(pair_index, int) or not pair_start <= pair_index < pair_start + pairs:
                raise ValueError(f"{path}:{line_number} pair index mismatch")
            if record_type == "decision":
                decision_count += 1
                continue
            terminal = row.get("terminal")
            episode_id = row.get("episode_id")
            seat = row.get("candidate_seat")
            reward = row.get("candidate_terminal_reward")
            if (
                not isinstance(terminal, dict)
                or terminal.get("terminal_classification") != "natural"
                or not isinstance(episode_id, int)
                or seat not in ("p0", "p1")
                or reward not in (-1, 0, 1)
                or not isinstance(row.get("pair_environment_seed_u64_hex"), str)
            ):
                raise ValueError(f"{path}:{line_number} terminal mismatch")
            key = (pair_index, episode_id, seat)
            if key in terminals:
                raise ValueError(f"{path}:{line_number} duplicate terminal")
            terminals[key] = row
    if header is None or decision_count <= 0 or len(terminals) != pairs * 2:
        raise ValueError(f"{path} outcome inventory mismatch")
    for pair_index in range(pair_start, pair_start + pairs):
        rows = [key for key in terminals if key[0] == pair_index]
        if len(rows) != 2 or {key[2] for key in rows} != {"p0", "p1"}:
            raise ValueError(f"{path} pair {pair_index} is not an exact seat swap")
    return header, terminals, decision_count


def adjudicate(
    candidate: dict[tuple[int, int, str], dict[str, Any]],
    baseline: dict[tuple[int, int, str], dict[str, Any]],
    gates: dict[str, int] | None,
) -> dict[str, Any]:
    if set(candidate) != set(baseline):
        raise ValueError("candidate and baseline terminal inventories differ")
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
        for field in ("pair_environment_seed_u64_hex", "episode_id", "candidate_seat"):
            if candidate_row.get(field) != baseline_row.get(field):
                raise ValueError(f"matched receipt differs at {key}: {field}")
        seat = key[2]
        candidate_reward = int(candidate_row["candidate_terminal_reward"])
        baseline_reward = int(baseline_row["candidate_terminal_reward"])
        wins["candidate"]["overall"] += int(candidate_reward > 0)
        wins["candidate"][seat] += int(candidate_reward > 0)
        wins["baseline"]["overall"] += int(baseline_reward > 0)
        wins["baseline"][seat] += int(baseline_reward > 0)
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
    gate_results = None
    if gates is not None:
        gate_results = {
            "overall_terminal_order_net_floor": nets["overall"]
            >= gates["overall_terminal_order_net_floor"],
            "p0_terminal_order_net_floor": nets["p0"] >= gates["p0_terminal_order_net_floor"],
            "p1_terminal_order_net_floor": nets["p1"] >= gates["p1_terminal_order_net_floor"],
        }
    return {
        "terminal_order": {"overall": overall, "by_candidate_seat": seats, "nets": nets},
        "wins": wins,
        "gates": gate_results,
        "pass": True if gate_results is None else all(gate_results.values()),
    }


def run(manifest_path: Path) -> dict[str, Any]:
    validated = validate_manifest(manifest_path)
    root = validated["evidence_root"]
    existing = sorted(path.name for path in root.iterdir()) if root.exists() else []
    if existing != ["manifest.json"]:
        raise FileExistsError("Pool3 evidence root must contain only manifest.json")
    args = {name: _collection_args(validated, name) for name in ARM_ORDER}
    started = time.perf_counter()
    reports: dict[str, dict[str, Any]] = {}
    errors: dict[str, str] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        futures = {name: executor.submit(collector.collect, args[name]) for name in ARM_ORDER}
        concurrent.futures.wait(futures.values())
        for name in ARM_ORDER:
            try:
                reports[name] = futures[name].result()
            except BaseException as error:
                errors[name] = repr(error)
    wall_seconds = time.perf_counter() - started
    if errors:
        raise RuntimeError(f"Pool3 collection arm failure after both completed: {errors}")
    for name in ARM_ORDER:
        _validate_collection_report(args[name], reports[name])
    state = {
        "schema": STATE_SCHEMA,
        "mode": validated["mode"],
        "manifest_sha256": sha256(manifest_path),
        "wall_seconds": wall_seconds,
        "arms": {
            name: {
                "collection_report_path": str(args[name].output),
                "collection_report_sha256": sha256(args[name].output),
                "outcome_path": str(args[name].outcome_jsonl),
                "outcome_sha256": sha256(args[name].outcome_jsonl),
                "teacher_path": str(args[name].teacher_jsonl),
                "teacher_sha256": sha256(args[name].teacher_jsonl),
            }
            for name in ARM_ORDER
        },
        "outcomes_parsed": False,
    }
    state_path = root / "state.json"
    write_new_json(state_path, state)

    loaded: dict[str, tuple[dict[str, Any], dict[tuple[int, int, str], dict[str, Any]], int]] = {}
    for name in ARM_ORDER:
        _validate_teacher_header(args[name].teacher_jsonl, validated["arms"][name]["checkpoint"])
        loaded[name] = _load_terminals(
            args[name].outcome_jsonl,
            validated["arms"][name]["checkpoint"],
            validated["panel"]["base_seed"],
            validated["panel"]["pair_start"],
            validated["panel"]["pairs"],
        )
    comparison = adjudicate(loaded["candidate"][1], loaded["baseline"][1], validated["gates"])
    report = {
        "schema": REPORT_SCHEMA,
        "mode": validated["mode"],
        "status": "pass" if comparison["pass"] else "fail",
        "manifest": {"path": str(manifest_path.resolve()), "sha256": sha256(manifest_path)},
        "prerequisites": validated["prerequisites"],
        "panel": validated["panel"],
        "gate_config": validated["gates"],
        "wall_seconds": wall_seconds,
        "achieved_combined_games_per_second": (
            validated["panel"]["episodes_per_arm"] * 2 / wall_seconds
        ),
        "arms": {
            name: {
                "checkpoint": loaded[name][0]["checkpoint"],
                "decision_count": loaded[name][2],
                "collection_report_path": str(args[name].output),
                "collection_report_sha256": sha256(args[name].output),
                "outcome_path": str(args[name].outcome_jsonl),
                "outcome_sha256": sha256(args[name].outcome_jsonl),
                "teacher_path": str(args[name].teacher_jsonl),
                "teacher_sha256": sha256(args[name].teacher_jsonl),
                "elapsed_seconds": reports[name]["elapsed_seconds"],
                "games_per_second": reports[name]["games_per_second"],
            }
            for name in ARM_ORDER
        },
        "comparison": comparison,
        "nonclaims": [
            "terminal win, draw, or loss is the only gate signal",
            "Pool3 is a native Rally transport and noninferiority gate, not professional-level evidence",
        ],
    }
    write_new_json(validated["report_path"], report)
    state["outcomes_parsed"] = True
    state["report_path"] = str(validated["report_path"])
    state["report_sha256"] = sha256(validated["report_path"])
    state["state_supersedes_sha256"] = sha256(state_path)
    replacement = root / "state-final.json"
    write_new_json(replacement, state)
    return report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args(argv)
    print(json.dumps(run(args.manifest), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (KeyError, OSError, TypeError, ValueError, RuntimeError, json.JSONDecodeError) as error:
        print(f"run_pool3_gate_v4: ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
