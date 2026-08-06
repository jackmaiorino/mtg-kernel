#!/usr/bin/env python3
"""Screen concurrency and run a complete population payoff matrix."""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import itertools
import json
import os
import subprocess
import time
from collections import defaultdict
from pathlib import Path


TEST_NAME = (
    "native_science_loop_v1::windows_science_loop_tests::"
    "ladder_head_to_head_eval_v1"
)
H2H_ENVIRONMENT = (
    "H2H_CANDIDATE_STORE_ROOT",
    "H2H_CANDIDATE_GEN",
    "H2H_CANDIDATE_USE_STORE_RUN",
    "H2H_CANDIDATE_BASE_SEED",
    "H2H_CANDIDATE_POOL_JSON",
    "H2H_UPDATES",
    "H2H_INIT_STORE",
    "H2H_INIT_GEN",
    "H2H_OPPONENT_STORE_ROOT",
    "H2H_OPPONENT_GEN",
    "H2H_PAIRS",
    "H2H_EVAL_SEED",
    "H2H_ENVIRONMENT_RANDOMIZATION_V2",
    "H2H_OUTCOME_JSON",
    "WIDE",
)
PAIR_COUNT = 512
SCREEN_PAIR_COUNT = 128
SCREEN_REPLICAS = 4
SCREEN_MIN_SPEEDUP = 2.0
SCREEN_EVAL_SEED = 639_800_001
MATRIX_EVAL_SEED_STRIDE = 1_000_000


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def file_record(path: Path) -> dict:
    resolved = path.resolve()
    return {
        "path": str(resolved),
        "bytes": resolved.stat().st_size,
        "sha256": sha256_file(resolved),
    }


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def canonical_bytes(value: dict) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def write_new_json(path: Path, value: dict) -> None:
    encoded = canonical_bytes(value)
    with path.open("xb") as stream:
        stream.write(encoded)
        stream.flush()


def unique_attempt_root(evidence_root: Path, gate: str) -> Path:
    parent = evidence_root / gate
    parent.mkdir(parents=True, exist_ok=True)
    for index in itertools.count(1):
        candidate = parent / f"attempt-{index:03d}"
        try:
            candidate.mkdir()
            return candidate
        except FileExistsError:
            continue


def command_output(command: list[str], cwd: Path) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout.strip()


def find_linker() -> Path:
    installer = (
        Path(os.environ["ProgramFiles(x86)"])
        / "Microsoft Visual Studio"
        / "Installer"
        / "vswhere.exe"
    )
    if not installer.is_file():
        raise ValueError("vswhere.exe was not found")
    completed = subprocess.run(
        [
            str(installer),
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    )
    installation = Path(completed.stdout.strip())
    candidates = sorted(
        (installation / "VC" / "Tools" / "MSVC").glob("*/bin/Hostx64/x64/link.exe")
    )
    if not candidates:
        raise ValueError("MSVC link.exe was not found")
    return candidates[-1].resolve()


def toolchain_record(repo_root: Path) -> dict:
    linker = find_linker()
    linker_run = subprocess.run(
        [str(linker)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    return {
        "rustc": command_output(["rustc", "-Vv"], repo_root),
        "cargo": command_output(["cargo", "-V"], repo_root),
        "linker": {
            "path": str(linker),
            "sha256": sha256_file(linker),
            "banner": linker_run.stdout.strip(),
        },
    }


def root_mapping(args: argparse.Namespace) -> dict[int, Path]:
    return {
        920012: args.anchor_0.resolve(),
        920005: args.anchor_1.resolve(),
        970001: args.seed_970001.resolve(),
        970002: args.seed_970002.resolve(),
        970003: args.seed_970003.resolve(),
    }


def resolve_slots(args: argparse.Namespace, bindings: dict) -> list[dict]:
    roots = root_mapping(args)
    slots = []
    for expected_index, source in enumerate(bindings["slots"]):
        if source["slot_index"] != expected_index:
            raise ValueError("slot indexes are not ordered")
        seed = int(source["source_base_seed"])
        root = roots[seed]
        generation = int(source["source_generation"])
        prefix = root / "checkpoints" / f"update-{generation:08d}"
        paths = {
            "run": root / "run.json",
            "checkpoint": Path(f"{prefix}.checkpoint.json"),
            "sidecar": Path(f"{prefix}.sidecar.json"),
            "state": Path(f"{prefix}.state.f32le"),
        }
        expected_hashes = {
            "run": source["source_run_sha256"],
            "checkpoint": source["checkpoint_sha256"],
            "sidecar": source["sidecar_sha256"],
            "state": source["state_sha256"],
        }
        for name, path in paths.items():
            if not path.is_file() or sha256_file(path) != expected_hashes[name]:
                raise ValueError(f"slot {expected_index} {name} authority mismatch")
        checkpoint = load_json(paths["checkpoint"])
        if (
            checkpoint["generation_index"] != generation
            or checkpoint["train_state"]["model_parameter_sha256"]
            != source["model_parameter_sha256"]
        ):
            raise ValueError(f"slot {expected_index} checkpoint identity mismatch")
        slots.append({**source, "store_root": str(root)})
    if len(slots) != 8 or len({slot["model_parameter_sha256"] for slot in slots}) != 8:
        raise ValueError("payoff panel requires eight unique slots")
    return slots


def git_record(repo_root: Path, executable_source_commit: str) -> dict:
    head = command_output(["git", "rev-parse", "HEAD"], repo_root)
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", executable_source_commit, head],
        cwd=repo_root,
    )
    if ancestor.returncode != 0:
        raise ValueError("executable source commit is not an ancestor of HEAD")
    if command_output(["git", "status", "--short"], repo_root):
        raise ValueError("payoff evaluation requires a clean worktree")
    return {"commit": head, "executable_source_commit": executable_source_commit}


def run_context(args: argparse.Namespace) -> tuple[dict, list[dict]]:
    bindings = load_json(args.bindings)
    if (
        bindings.get("schema") != "scaled-selfplay-population-slot-bindings/v1"
        or bindings.get("selection_uses_terminal_outcomes") is not False
    ):
        raise ValueError("unexpected binding schema")
    slots = resolve_slots(args, bindings)
    executable = args.executable.resolve()
    if not executable.is_file():
        raise ValueError("release test executable is missing")
    context = {
        "git": git_record(args.repo_root.resolve(), args.executable_source_commit),
        "toolchain": toolchain_record(args.repo_root.resolve()),
        "executable": {
            **file_record(executable),
            "source_commit": args.executable_source_commit,
        },
        "bindings": file_record(args.bindings),
        "global_generation": int(bindings["global_generation"]),
        "refresh_index": int(bindings["refresh_index"]),
        "gpu_ordinal": "not-used; native head-to-head inference is CPU-resident",
        "terminal_reward_only": True,
    }
    return context, slots


def arm_spec(
    label: str,
    candidate_index: int,
    opponent_index: int,
    pair_count: int,
    evaluation_seed: int,
) -> dict:
    return {
        "label": label,
        "candidate_index": candidate_index,
        "opponent_index": opponent_index,
        "pair_count": pair_count,
        "evaluation_seed": evaluation_seed,
    }


def run_arm(
    executable: Path,
    repo_root: Path,
    root: Path,
    slots: list[dict],
    spec: dict,
) -> dict:
    arm_root = root / spec["label"]
    arm_root.mkdir()
    outcome_path = arm_root / "outcome.json"
    stdout_path = arm_root / "stdout.log"
    stderr_path = arm_root / "stderr.log"
    candidate = slots[spec["candidate_index"]]
    opponent = slots[spec["opponent_index"]]
    environment = os.environ.copy()
    for name in H2H_ENVIRONMENT:
        environment.pop(name, None)
    environment.update(
        {
            "H2H_CANDIDATE_STORE_ROOT": candidate["store_root"],
            "H2H_CANDIDATE_GEN": str(candidate["source_generation"]),
            "H2H_CANDIDATE_USE_STORE_RUN": "1",
            "H2H_OPPONENT_STORE_ROOT": opponent["store_root"],
            "H2H_OPPONENT_GEN": str(opponent["source_generation"]),
            "H2H_PAIRS": str(spec["pair_count"]),
            "H2H_EVAL_SEED": str(spec["evaluation_seed"]),
            "H2H_ENVIRONMENT_RANDOMIZATION_V2": "1",
            "H2H_OUTCOME_JSON": str(outcome_path),
        }
    )
    command = [
        str(executable),
        TEST_NAME,
        "--ignored",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ]
    start = time.perf_counter()
    with stdout_path.open("xb") as stdout, stderr_path.open("xb") as stderr:
        completed = subprocess.run(
            command,
            cwd=repo_root,
            env=environment,
            stdout=stdout,
            stderr=stderr,
        )
    wall_seconds = time.perf_counter() - start
    record = {
        **spec,
        "candidate_role": candidate["role"],
        "opponent_role": opponent["role"],
        "exit_code": completed.returncode,
        "wall_seconds": wall_seconds,
        "stdout": file_record(stdout_path),
        "stderr": file_record(stderr_path),
        "outcome": file_record(outcome_path) if outcome_path.is_file() else None,
    }
    if completed.returncode != 0 or record["outcome"] is None:
        raise RuntimeError(f"payoff arm failed: {record}")
    return record


def run_batch(
    executable: Path,
    repo_root: Path,
    root: Path,
    slots: list[dict],
    specs: list[dict],
    concurrency: int,
) -> tuple[list[dict], float]:
    started = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [
            executor.submit(run_arm, executable, repo_root, root, slots, spec)
            for spec in specs
        ]
        records = [future.result() for future in futures]
    return records, time.perf_counter() - started


def validate_outcome(path: Path, spec: dict, slots: list[dict]) -> dict:
    outcome = load_json(path)
    candidate = slots[spec["candidate_index"]]
    opponent = slots[spec["opponent_index"]]
    if (
        outcome.get("schema") != "mtg-kernel-head-to-head-terminal-stream/v1"
        or outcome["evaluation_base_seed"] != spec["evaluation_seed"]
        or outcome["pair_count"] != spec["pair_count"]
        or outcome["episode_count"] != spec["pair_count"] * 2
        or outcome["runtime"]["all_natural"] is not True
        or outcome["runtime"]["environment_randomization_v2"] is not True
    ):
        raise ValueError(f"outcome header mismatch for {spec['label']}")
    for side, slot in (("candidate", candidate), ("opponent", opponent)):
        identity = outcome[side]
        if (
            identity["run_sha256"] != slot["source_run_sha256"]
            or identity["generation"] != slot["source_generation"]
            or identity["checkpoint_manifest_sha256"] != slot["checkpoint_sha256"]
            or identity["checkpoint_payload_sha256"] != slot["state_sha256"]
            or identity["model_parameter_sha256"] != slot["model_parameter_sha256"]
        ):
            raise ValueError(f"{side} identity mismatch for {spec['label']}")
    episodes = outcome["episodes"]
    if len(episodes) != spec["pair_count"] * 2:
        raise ValueError(f"episode count mismatch for {spec['label']}")
    by_pair: dict[int, list[dict]] = defaultdict(list)
    counts = {-1: 0, 0: 0, 1: 0}
    for episode in episodes:
        rank = int(episode["terminal_order_rank"])
        if rank not in counts:
            raise ValueError("nonterminal rank entered payoff panel")
        counts[rank] += 1
        by_pair[int(episode["pair_index"])].append(episode)
    pair_seeds = []
    for pair_index in range(spec["pair_count"]):
        pair = by_pair[pair_index]
        if (
            len(pair) != 2
            or {row["learner_seat"] for row in pair} != {"P0", "P1"}
            or len({row["environment_seed"] for row in pair}) != 1
        ):
            raise ValueError(f"seat-swap binding mismatch at pair {pair_index}")
        pair_seeds.append(int(pair[0]["environment_seed"]))
    if len(set(pair_seeds)) != len(pair_seeds):
        raise ValueError(f"duplicate pair seeds within {spec['label']}")
    overall = outcome["learner_outcomes"]["overall"]
    if (
        overall["wins"] != counts[1]
        or overall["losses"] != counts[-1]
        or overall["draws"] != counts[0]
    ):
        raise ValueError(f"W/L/D mismatch for {spec['label']}")
    return {
        "wins": counts[1],
        "losses": counts[-1],
        "draws": counts[0],
        "terminal_order_sum": counts[1] - counts[-1],
        "game_count": len(episodes),
        "pair_environment_seeds": pair_seeds,
    }


def run_screen(args: argparse.Namespace) -> Path:
    context, slots = run_context(args)
    root = unique_attempt_root(args.evidence_root.resolve(), "payoff-concurrency-screen")
    baseline_spec = arm_spec("baseline", 4, 0, SCREEN_PAIR_COUNT, SCREEN_EVAL_SEED)
    replica_specs = [
        arm_spec(f"parallel-replica-{index:02d}", 4, 0, SCREEN_PAIR_COUNT, SCREEN_EVAL_SEED)
        for index in range(SCREEN_REPLICAS)
    ]
    plan = {
        "schema": "scaled-selfplay-population-payoff-concurrency-screen-plan/v1",
        **context,
        "candidate_slot": 4,
        "opponent_slot": 0,
        "pair_count_per_arm": SCREEN_PAIR_COUNT,
        "evaluation_seed": SCREEN_EVAL_SEED,
        "replicas": SCREEN_REPLICAS,
        "minimum_aggregate_speedup": SCREEN_MIN_SPEEDUP,
    }
    plan_path = root / "screen-plan.json"
    write_new_json(plan_path, plan)
    baseline, baseline_wall = run_batch(
        args.executable.resolve(),
        args.repo_root.resolve(),
        root,
        slots,
        [baseline_spec],
        1,
    )
    replicas, parallel_wall = run_batch(
        args.executable.resolve(),
        args.repo_root.resolve(),
        root,
        slots,
        replica_specs,
        SCREEN_REPLICAS,
    )
    validate_outcome(Path(baseline[0]["outcome"]["path"]), baseline_spec, slots)
    for spec, record in zip(replica_specs, replicas, strict=True):
        validate_outcome(Path(record["outcome"]["path"]), spec, slots)
    baseline_hash = baseline[0]["outcome"]["sha256"]
    exact = all(record["outcome"]["sha256"] == baseline_hash for record in replicas)
    speedup = baseline_wall * SCREEN_REPLICAS / parallel_wall
    selected = SCREEN_REPLICAS if exact and speedup >= SCREEN_MIN_SPEEDUP else 1
    manifest = {
        "schema": "scaled-selfplay-population-payoff-concurrency-screen/v1",
        "passed": exact,
        "plan": file_record(plan_path),
        "executable": context["executable"],
        "bindings": context["bindings"],
        "baseline": baseline[0],
        "parallel_replicas": replicas,
        "parallel_wall_seconds": parallel_wall,
        "aggregate_speedup": speedup,
        "outcomes_bit_identical": exact,
        "selected_concurrency": selected,
        "gpu_ordinal": context["gpu_ordinal"],
    }
    manifest_path = root / "screen-manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    return manifest_path


def run_matrix(args: argparse.Namespace) -> Path:
    context, slots = run_context(args)
    screen = load_json(args.screen_manifest)
    if (
        screen.get("passed") is not True
        or screen["executable"]["sha256"] != context["executable"]["sha256"]
        or screen["bindings"]["sha256"] != context["bindings"]["sha256"]
    ):
        raise ValueError("payoff concurrency screen does not bind this run")
    concurrency = int(screen["selected_concurrency"])
    if concurrency not in (1, SCREEN_REPLICAS):
        raise ValueError("unexpected selected concurrency")
    root = unique_attempt_root(
        args.evidence_root.resolve(),
        f"payoff-matrix-generation-{context['global_generation']:04d}",
    )
    specs = []
    matrix_first_eval_seed = context["global_generation"] * 1_000_000 + 100_001
    for matchup_index, (candidate, opponent) in enumerate(itertools.combinations(range(8), 2)):
        specs.append(
            arm_spec(
                f"matchup-{candidate}-{opponent}",
                candidate,
                opponent,
                PAIR_COUNT,
                matrix_first_eval_seed + matchup_index * MATRIX_EVAL_SEED_STRIDE,
            )
        )
    plan = {
        "schema": "scaled-selfplay-population-payoff-matrix-plan/v1",
        **context,
        "screen": file_record(args.screen_manifest),
        "pair_count_per_unordered_matchup": PAIR_COUNT,
        "unordered_matchup_count": 28,
        "total_pair_count": 14336,
        "total_game_count": 28672,
        "selected_concurrency": concurrency,
        "first_evaluation_seed": matrix_first_eval_seed,
        "evaluation_seed_stride": MATRIX_EVAL_SEED_STRIDE,
        "matchups": specs,
        "analysis": {
            "per_matchup_payoff": "candidate terminal-order sum divided by 1024 games",
            "per_policy_input": "signed terminal-order sum over seven opponents divided by 7168 games",
            "draw_rank": 0,
            "win_rank": 1,
            "loss_rank": -1,
        },
    }
    plan_path = root / "matrix-plan.json"
    write_new_json(plan_path, plan)
    records, wall_seconds = run_batch(
        args.executable.resolve(),
        args.repo_root.resolve(),
        root,
        slots,
        specs,
        concurrency,
    )
    matrix = [[0 for _ in range(8)] for _ in range(8)]
    matchup_rows = []
    all_pair_seeds: set[int] = set()
    for spec, record in zip(specs, records, strict=True):
        outcome_path = Path(record["outcome"]["path"])
        summary = validate_outcome(outcome_path, spec, slots)
        for seed in summary.pop("pair_environment_seeds"):
            if seed in all_pair_seeds:
                raise ValueError("pair environment seed was reused across matchups")
            all_pair_seeds.add(seed)
        candidate = spec["candidate_index"]
        opponent = spec["opponent_index"]
        net = summary["terminal_order_sum"]
        matrix[candidate][opponent] = net
        matrix[opponent][candidate] = -net
        matchup_rows.append(
            {
                **spec,
                "candidate_role": slots[candidate]["role"],
                "opponent_role": slots[opponent]["role"],
                **summary,
                "normalized_terminal_order_payoff": net / (PAIR_COUNT * 2),
                "outcome_sha256": record["outcome"]["sha256"],
            }
        )
    if len(all_pair_seeds) != 14336:
        raise ValueError("complete payoff panel does not contain 14,336 fresh pair seeds")
    policy_inputs = []
    for index, slot_record in enumerate(slots):
        terminal_sum = sum(matrix[index])
        policy_inputs.append(
            {
                "slot_index": index,
                "role": slot_record["role"],
                "model_parameter_sha256": slot_record["model_parameter_sha256"],
                "terminal_order_sum": terminal_sum,
                "game_count": 7 * PAIR_COUNT * 2,
                "normalized_terminal_order_payoff": terminal_sum / (7 * PAIR_COUNT * 2),
            }
        )
    panel = {
        "schema": "scaled-selfplay-population-payoff-panel/v1",
        "global_generation": context["global_generation"],
        "refresh_index": context["refresh_index"],
        "bindings_sha256": context["bindings"]["sha256"],
        "executable_sha256": context["executable"]["sha256"],
        "executable_source_commit": context["executable"]["source_commit"],
        "terminal_reward_only": True,
        "pair_count_per_matchup": PAIR_COUNT,
        "matchup_count": 28,
        "total_pair_count": 14336,
        "total_game_count": 28672,
        "all_natural": True,
        "fresh_pair_seed_count": len(all_pair_seeds),
        "matchups": matchup_rows,
        "policy_inputs": policy_inputs,
    }
    panel_path = root / "payoff-panel.json"
    write_new_json(panel_path, panel)
    manifest = {
        "schema": "scaled-selfplay-population-payoff-matrix-execution/v1",
        "passed": True,
        "disposition": f"PAYOFF-MATRIX-{context['global_generation']}-COMPLETE",
        "plan": file_record(plan_path),
        "screen": file_record(args.screen_manifest),
        "bindings": context["bindings"],
        "executable": context["executable"],
        "wall_seconds": wall_seconds,
        "aggregate_games_per_second": 28672 / wall_seconds,
        "arms": records,
        "payoff_panel": file_record(panel_path),
        "terminal_reward_only": True,
        "gpu_ordinal": context["gpu_ordinal"],
    }
    manifest_path = root / "matrix-execution-manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    return manifest_path


def add_common_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--executable-source-commit", required=True)
    parser.add_argument("--bindings", required=True, type=Path)
    parser.add_argument("--anchor-0", required=True, type=Path)
    parser.add_argument("--anchor-1", required=True, type=Path)
    parser.add_argument("--seed-970001", required=True, type=Path)
    parser.add_argument("--seed-970002", required=True, type=Path)
    parser.add_argument("--seed-970003", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    screen = subparsers.add_parser("screen")
    add_common_arguments(screen)
    matrix = subparsers.add_parser("matrix")
    add_common_arguments(matrix)
    matrix.add_argument("--screen-manifest", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "screen":
        run_screen(args)
    else:
        run_matrix(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
