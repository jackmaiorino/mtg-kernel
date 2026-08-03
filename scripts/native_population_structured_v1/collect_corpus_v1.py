#!/usr/bin/env python3
"""Collect native candidate trajectories against the frozen Pool3 mixture."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
from typing import Any


SCHEMA = "mtg-kernel-native-population-structured-corpus-collection/v1"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _request(process: subprocess.Popen[str], payload: dict[str, Any]) -> dict[str, Any]:
    assert process.stdin is not None and process.stdout is not None
    process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
    process.stdin.flush()
    line = process.stdout.readline()
    if not line:
        stderr = process.stderr.read() if process.stderr is not None else ""
        raise RuntimeError(f"scorer exited before responding: {stderr}")
    response = json.loads(line)
    if response.get("response_type") == "error":
        raise RuntimeError(
            f"scorer error {response.get('error_code')}: {response.get('message')}"
        )
    return response


def collect(args: argparse.Namespace) -> dict[str, Any]:
    for path in (args.scorer, args.candidate_root, args.pool_root):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    for path in (args.teacher_jsonl, args.outcome_jsonl, args.output):
        if path.exists():
            raise RuntimeError(f"output already exists: {path}")
        path.parent.mkdir(parents=True, exist_ok=True)

    command = [
        str(args.scorer),
        "--candidate-outcome-root",
        str(args.candidate_root),
        "--pool-root",
        str(args.pool_root),
        "--teacher-jsonl",
        str(args.teacher_jsonl),
        "--outcome-jsonl",
        str(args.outcome_jsonl),
    ]
    started = time.perf_counter()
    process = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )
    terminal_counts = {"p0_win": 0, "p1_win": 0, "draw": 0}
    candidate_returns = {"-1": 0, "0": 0, "1": 0}
    policy_steps = 0
    startup_seconds = None
    episode_seconds: list[float] = []
    try:
        first_episode = args.pair_start * 2
        final_episode = first_episode + args.pairs * 2
        for episode_id in range(first_episode, final_episode):
            episode_started = time.perf_counter()
            response = _request(
                process,
                {
                    "request_type": "reset",
                    "request_id": f"reset-{episode_id}",
                    "episode_id": episode_id,
                    "base_seed": args.base_seed,
                },
            )
            if startup_seconds is None:
                startup_seconds = time.perf_counter() - started
            while response.get("response_type") == "decision":
                decision = response["decision"]
                selected = decision.get("selected_action_index")
                if not isinstance(selected, int):
                    raise RuntimeError(
                        f"episode {episode_id} step {decision.get('step')} has no model selection"
                    )
                response = _request(
                    process,
                    {
                        "request_type": "step",
                        "request_id": f"step-{episode_id}-{decision['step']}",
                        "episode_id": episode_id,
                        "expected_step": decision["step"],
                        "selected_index": selected,
                    },
                )
                policy_steps += 1
            if response.get("response_type") != "terminal":
                raise RuntimeError(f"episode {episode_id} ended with an invalid response")
            terminal = response["terminal"]["terminal"]
            outcome = terminal["terminal_outcome"]
            terminal_counts[outcome] += 1
            candidate_seat = response["terminal"]["candidate_seat"]
            reward_index = 0 if candidate_seat == "p0" else 1
            reward = terminal["terminal_reward"][reward_index]
            candidate_returns[str(reward)] += 1
            episode_seconds.append(time.perf_counter() - episode_started)
    finally:
        if process.stdin is not None:
            process.stdin.close()
    stderr = process.stderr.read() if process.stderr is not None else ""
    return_code = process.wait(timeout=30)
    if return_code != 0:
        raise RuntimeError(f"scorer exited {return_code}: {stderr}")

    elapsed = time.perf_counter() - started
    report = {
        "schema": SCHEMA,
        "base_seed": args.base_seed,
        "pair_start": args.pair_start,
        "pairs": args.pairs,
        "episodes": args.pairs * 2,
        "policy_steps": policy_steps,
        "terminal_counts": terminal_counts,
        "candidate_returns": candidate_returns,
        "elapsed_seconds": elapsed,
        "startup_seconds": startup_seconds,
        "episode_seconds": episode_seconds,
        "mean_episode_seconds": sum(episode_seconds) / len(episode_seconds),
        "games_per_second": (args.pairs * 2) / elapsed,
        "policy_steps_per_second": policy_steps / elapsed,
        "teacher_jsonl": str(args.teacher_jsonl),
        "teacher_sha256": _sha256(args.teacher_jsonl),
        "outcome_jsonl": str(args.outcome_jsonl),
        "outcome_sha256": _sha256(args.outcome_jsonl),
        "scorer_stderr": stderr.strip().splitlines(),
    }
    args.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scorer", type=Path, required=True)
    parser.add_argument("--candidate-root", type=Path, required=True)
    parser.add_argument("--pool-root", type=Path, required=True)
    parser.add_argument("--teacher-jsonl", type=Path, required=True)
    parser.add_argument("--outcome-jsonl", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--base-seed", type=int, default=1_500_001)
    parser.add_argument("--pair-start", type=int, default=0)
    parser.add_argument("--pairs", type=int, default=1)
    args = parser.parse_args()
    if args.base_seed < 0 or args.pair_start < 0 or args.pairs < 1:
        parser.error("base seed and pair start must be nonnegative; pairs must be positive")
    print(json.dumps(collect(args), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
