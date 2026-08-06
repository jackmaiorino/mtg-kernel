#!/usr/bin/env python3
"""Run the scheduled three-lineage and shared-control native anchor read."""

from __future__ import annotations

import argparse
from pathlib import Path

from run_payoff_evaluation import (
    file_record,
    git_record,
    load_json,
    run_batch,
    sha256_file,
    toolchain_record,
    unique_attempt_root,
    validate_outcome,
    write_new_json,
)


PAIR_COUNT = 1024
EVALUATION_SEED = 768_300_001


def checkpoint_slot(root: Path, seed: int, generation: int, role: str) -> dict:
    resolved = root.resolve()
    prefix = resolved / "checkpoints" / f"update-{generation:08d}"
    run_path = resolved / "run.json"
    checkpoint_path = Path(f"{prefix}.checkpoint.json")
    sidecar_path = Path(f"{prefix}.sidecar.json")
    state_path = Path(f"{prefix}.state.f32le")
    for path in (run_path, checkpoint_path, sidecar_path, state_path):
        if not path.is_file():
            raise ValueError(f"missing anchor-read input: {path}")
    run = load_json(run_path)
    checkpoint = load_json(checkpoint_path)
    if (
        run["schedule"]["base_seed"] != seed
        or checkpoint["generation_index"] != generation
    ):
        raise ValueError(f"anchor-read Store identity mismatch: {resolved}")
    return {
        "role": role,
        "store_root": str(resolved),
        "source_base_seed": seed,
        "source_generation": generation,
        "source_run_sha256": sha256_file(run_path),
        "checkpoint_sha256": sha256_file(checkpoint_path),
        "sidecar_sha256": sha256_file(sidecar_path),
        "state_sha256": sha256_file(state_path),
        "model_parameter_sha256": checkpoint["train_state"]["model_parameter_sha256"],
    }


def arm_spec(label: str, candidate_index: int) -> dict:
    return {
        "label": label,
        "candidate_index": candidate_index,
        "opponent_index": 3,
        "pair_count": PAIR_COUNT,
        "evaluation_seed": EVALUATION_SEED,
    }


def paired_summary(candidate: dict, control: dict) -> dict:
    candidate_rows = {row["episode_index"]: row for row in candidate["episodes"]}
    control_rows = {row["episode_index"]: row for row in control["episodes"]}
    if candidate_rows.keys() != control_rows.keys():
        raise ValueError("candidate and control episode indexes differ")
    totals = {"overall": 0, "P0": 0, "P1": 0}
    games = {"overall": 0, "P0": 0, "P1": 0}
    for index, candidate_row in candidate_rows.items():
        control_row = control_rows[index]
        binding_keys = ("pair_index", "environment_seed", "learner_seat", "deck_hashes_u64")
        if any(candidate_row[key] != control_row[key] for key in binding_keys):
            raise ValueError(f"candidate/control CRN binding differs at episode {index}")
        seat = candidate_row["learner_seat"]
        delta = candidate_row["terminal_order_rank"] - control_row["terminal_order_rank"]
        totals["overall"] += delta
        totals[seat] += delta
        games["overall"] += 1
        games[seat] += 1
    return {
        key: {
            "terminal_order_delta_sum": totals[key],
            "game_count": games[key],
            "normalized_terminal_order_delta": totals[key] / games[key],
        }
        for key in ("overall", "P0", "P1")
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", required=True, type=Path)
    parser.add_argument("--executable", required=True, type=Path)
    parser.add_argument("--executable-source-commit", required=True)
    parser.add_argument("--seed-970001", required=True, type=Path)
    parser.add_argument("--seed-970002", required=True, type=Path)
    parser.add_argument("--seed-970003", required=True, type=Path)
    parser.add_argument("--promoted-2", required=True, type=Path)
    parser.add_argument("--screen-manifest", required=True, type=Path)
    parser.add_argument("--prerequisite-interval", required=True, type=Path)
    parser.add_argument("--evidence-root", required=True, type=Path)
    args = parser.parse_args()

    repo_root = args.repo_root.resolve()
    executable = args.executable.resolve()
    screen = load_json(args.screen_manifest)
    prerequisite = load_json(args.prerequisite_interval)
    if (
        screen.get("passed") is not True
        or screen["selected_concurrency"] != 4
        or screen["executable"]["sha256"] != sha256_file(executable)
        or prerequisite.get("disposition") != "INTERVAL-0768-COMPLETE"
    ):
        raise ValueError("anchor read prerequisites do not pass")
    slots = [
        checkpoint_slot(args.seed_970001, 970001, 768, "lineage-970001"),
        checkpoint_slot(args.seed_970002, 970002, 768, "lineage-970002"),
        checkpoint_slot(args.seed_970003, 970003, 768, "lineage-970003"),
        checkpoint_slot(args.promoted_2, 920012, 384, "promoted-2-control"),
    ]
    root = unique_attempt_root(args.evidence_root.resolve(), "native-anchor-read-generation-0768")
    specs = [
        arm_spec("lineage-970001", 0),
        arm_spec("lineage-970002", 1),
        arm_spec("lineage-970003", 2),
        arm_spec("shared-control", 3),
    ]
    context = {
        "git": git_record(repo_root, args.executable_source_commit),
        "toolchain": toolchain_record(repo_root),
        "executable": {**file_record(executable), "source_commit": args.executable_source_commit},
        "screen": file_record(args.screen_manifest),
        "prerequisite_interval": file_record(args.prerequisite_interval),
        "global_generation": 768,
        "program_update": 256,
        "pair_count_per_arm": PAIR_COUNT,
        "evaluation_seed": EVALUATION_SEED,
        "concurrency": 4,
        "gpu_ordinal": "not-used; native head-to-head inference is CPU-resident",
        "terminal_reward_only": True,
        "slots": slots,
    }
    plan_path = root / "anchor-read-plan.json"
    write_new_json(plan_path, {"schema": "scaled-selfplay-native-anchor-read-plan/v1", **context})
    records, wall_seconds = run_batch(executable, repo_root, root, slots, specs, 4)
    outcomes = []
    decoded = []
    for spec, record in zip(specs, records, strict=True):
        path = Path(record["outcome"]["path"])
        validate_outcome(path, spec, slots)
        outcome = load_json(path)
        decoded.append(outcome)
        outcomes.append(
            {
                "label": spec["label"],
                "candidate": outcome["candidate"],
                "direct_wld": outcome["learner_outcomes"],
                "outcome_sha256": record["outcome"]["sha256"],
            }
        )
    control = decoded[3]
    paired = [
        {"label": specs[index]["label"], "versus_shared_control": paired_summary(decoded[index], control)}
        for index in range(3)
    ]
    result = {
        "schema": "scaled-selfplay-native-anchor-read-result/v1",
        "global_generation": 768,
        "program_update": 256,
        "pair_count_per_arm": PAIR_COUNT,
        "total_game_count": 8192,
        "all_natural": True,
        "terminal_reward_only": True,
        "direct_results": outcomes,
        "paired_results": paired,
    }
    result_path = root / "anchor-read-result.json"
    write_new_json(result_path, result)
    manifest = {
        "schema": "scaled-selfplay-native-anchor-read-execution/v1",
        "passed": True,
        "disposition": "NATIVE-ANCHOR-READ-0768-COMPLETE",
        "plan": file_record(plan_path),
        "result": file_record(result_path),
        "wall_seconds": wall_seconds,
        "aggregate_games_per_second": 8192 / wall_seconds,
        "arms": records,
    }
    manifest_path = root / "anchor-read-execution-manifest.json"
    write_new_json(manifest_path, manifest)
    print(manifest_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
