#!/usr/bin/env python3
"""Run the fresh native matched gate for the structured policy successor."""

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
import time
from typing import Any


sys.path.insert(
    0, str(Path(__file__).resolve().parents[1] / "history_value_search_live_v2")
)
import run_matched_gate_v2 as base  # noqa: E402


SCHEMA = "mtg-kernel-structured-policy-successor-matched-gate/v1"
CANDIDATE_FILENAME = "structured_policy_successor.json"
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v1"
COMPOSITE_DOMAIN = b"mtg-kernel-structured-policy-successor-composite-model/v1"
FORMAL_BASE_SEED = 1_650_001
FORMAL_TARGET_PAIRS = 1_024
FORMAL_MAX_PAIRS = 1_280
FORMAL_BATCH_PAIRS = 8
PROFILE_MAX_PAIRS = 64
TASK_RETRIES = 2
PAIR_PREFIX = base.PAIR_PREFIX
LEG_PREFIX = "XMAGE_RALLY_ANCHOR_LEG PASS "
KEY_VALUE = re.compile(r"([a-z_][a-z0-9_]*)=([^ ]+)")


def _fields(line: str) -> dict[str, str]:
    return dict(KEY_VALUE.findall(line.strip()))


def _candidate_identity(root: Path) -> dict[str, Any]:
    candidate_path = root / CANDIDATE_FILENAME
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    if candidate.get("schema") != CANDIDATE_SCHEMA:
        raise RuntimeError("candidate schema mismatch")
    report_ref = candidate.get("report", {})
    weights_ref = candidate.get("weights", {})
    parent = candidate.get("parent", {})
    report_path = root / str(report_ref.get("filename"))
    weights_path = root / str(weights_ref.get("filename"))
    if report_ref.get("filename") != "report.json":
        raise RuntimeError("candidate report filename mismatch")
    if weights_ref.get("filename") != "weights.f32le":
        raise RuntimeError("candidate weights filename mismatch")
    if not report_path.is_file() or not weights_path.is_file():
        raise RuntimeError("candidate report or weights are missing")
    report_sha256 = base._sha256(report_path)
    weights_sha256 = base._sha256(weights_path)
    if report_ref.get("sha256") != report_sha256:
        raise RuntimeError("candidate report SHA-256 mismatch")
    if weights_ref.get("sha256") != weights_sha256:
        raise RuntimeError("candidate weights SHA-256 mismatch")
    expected_parent = {
        "directory": "parent",
        "manifest_sha256": base.PARENT_IDENTITY["manifest"],
        "payload_sha256": base.PARENT_IDENTITY["payload"],
        "native_state_sha256": base.PARENT_IDENTITY["train_state"],
        "model_parameter_sha256": base.PARENT_IDENTITY["model"],
        "adam_step": int(base.PARENT_IDENTITY["adam_step"]),
    }
    if parent != expected_parent:
        raise RuntimeError("candidate retained-parent identity mismatch")
    composite = candidate.get("composite_model_parameter_sha256")
    if not isinstance(composite, str) or len(composite) != 64:
        raise RuntimeError("candidate composite identity is incomplete")
    expected_composite = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(parent["model_parameter_sha256"])
        + weights_path.read_bytes()
    ).hexdigest()
    if composite != expected_composite:
        raise RuntimeError("candidate composite SHA-256 mismatch")
    identity = {
        "adam_step": str(parent["adam_step"]),
        "manifest": base._sha256(candidate_path),
        "payload": weights_sha256,
        "train_state": report_sha256,
        "model": composite,
        "candidate_json_sha256": base._sha256(candidate_path),
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite,
        "parent": parent,
    }
    if any(
        len(identity[key]) != 64
        for key in ("manifest", "payload", "train_state", "model")
    ):
        raise RuntimeError("candidate identity is incomplete")
    return identity


def _leg_markers(path: Path) -> list[dict[str, str]]:
    rows = []
    with path.open("r", encoding="utf-8", errors="strict") as handle:
        for line in handle:
            if line.startswith(LEG_PREFIX):
                rows.append(_fields(line))
    return rows


def _valid_transport_marker(
    marker: dict[str, str], base_seed: int, pair_index: int
) -> bool:
    if (
        marker.get("base_seed") != str(base_seed)
        or marker.get("pair_index") != str(pair_index)
        or marker.get("episodes") != f"{pair_index * 2},{pair_index * 2 + 1}"
        or marker.get("candidate_seats") != "p0,p1"
        or marker.get("opponent") != "cp7"
        or marker.get("cp7_skill") != "7"
        or not re.fullmatch(r"[0-9a-f]{16}", marker.get("environment_seed", ""))
    ):
        return False
    winners = marker.get("winners", "").split(",")
    if len(winners) != 2 or any(winner not in ("p0", "p1", "draw") for winner in winners):
        return False
    for field in (
        "turns",
        "rust_steps",
        "physical_decisions",
        "candidate_priority_projections",
    ):
        values = marker.get(field, "").split(",")
        if len(values) != 2 or any(not value.isdigit() for value in values):
            return False
    return marker.get("alignment") in {
        "no_selected_action_projection",
        "selected_action_projection",
    }


def _pair_marker(path: Path, base_seed: int, pair_index: int) -> bool:
    markers = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if line.startswith(PAIR_PREFIX):
                markers.append(_fields(line))
    if len(markers) != 1 or not _valid_transport_marker(
        markers[0], base_seed, pair_index
    ):
        return False
    legs = _leg_markers(path)
    return _natural_terminal_marker(legs, pair_index)


def _natural_terminal_marker(legs: list[dict[str, str]], pair_index: int) -> bool:
    if len(legs) != 2:
        return False
    expected_episodes = {str(pair_index * 2), str(pair_index * 2 + 1)}
    return (
        {leg.get("episode") for leg in legs} == expected_episodes
        and [leg.get("candidate") for leg in legs] == ["p0", "p1"]
        and all(leg.get("winner") in ("p0", "p1", "draw") for leg in legs)
        and all(
            leg.get(field, "").isdigit()
            for leg in legs
            for field in ("rust_steps", "physical_decisions")
        )
    )


def _maven_opts(identity: dict[str, str]) -> str:
    return " ".join(
        (
            f"-Dxmage.rally.cp7Outcome.adamStep={identity['adam_step']}",
            f"-Dxmage.rally.cp7Outcome.manifestSha256={identity['manifest']}",
            f"-Dxmage.rally.cp7Outcome.payloadSha256={identity['payload']}",
            f"-Dxmage.rally.cp7Outcome.trainStateSha256={identity['train_state']}",
            f"-Dxmage.rally.cp7Outcome.modelParameterSha256={identity['model']}",
        )
    )


def _worker_database(args: argparse.Namespace, worker: int) -> Path:
    root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
    database = root / "cards.h2.mv.db"
    if not database.exists():
        root.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(args.source_database, database)
        if base._sha256(database) != base.CARD_DB_SHA256:
            raise RuntimeError(f"worker {worker} card database copy SHA-256 mismatch")
    return root


def _run_task(
    args: argparse.Namespace, worker: int, arm: str, pair_index: int
) -> dict[str, Any]:
    database = _worker_database(args, worker)
    scorer = args.candidate_scorer if arm == "candidate" else args.parent_scorer
    outcome_root = args.candidate_root if arm == "candidate" else args.parent_root
    identity = args.candidate_identity if arm == "candidate" else base.PARENT_IDENTITY
    execution_args = " ".join(
        (
            "--repo-root",
            str(args.mage_repo),
            "--scorer-exe",
            str(scorer),
            "--outcome-root",
            str(outcome_root),
            "--base-seed",
            str(args.base_seed),
            "--first-episode",
            str(pair_index * 2),
            "--pairs 1 --opponent cp7 --cp7-skill 7",
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
        f"-Dexec.args={execution_args}",
    ]
    environment = os.environ.copy()
    environment.update(
        {
            "MAGE_DB_DIR": str(database),
            "MAGE_DB_AUTO_SERVER": "false",
            "AI_DETERMINISTIC_TIEBREAKS": "true",
            "AI_DETERMINISTIC_SEARCH": "true",
            "AI_DETERMINISTIC_MAX_NODES": "5000",
            "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
            "MAVEN_OPTS": _maven_opts(identity),
        }
    )
    attempts: list[dict[str, Any]] = []
    for attempt in range(TASK_RETRIES + 1):
        log = args.evidence_root / "tasks" / (
            f"{arm}-pair-{pair_index:04d}-attempt-{attempt:02d}.log"
        )
        started = time.time()
        return_code: int | None = None
        error: str | None = None
        try:
            with log.open("x", encoding="utf-8", newline="\n") as output:
                completed = subprocess.run(
                    command,
                    cwd=args.mage_repo,
                    env=environment,
                    stdout=output,
                    stderr=subprocess.STDOUT,
                    check=False,
                )
            return_code = completed.returncode
        except OSError as exception:
            error = str(exception)
        marker_valid = return_code == 0 and error is None and _pair_marker(
            log, args.base_seed, pair_index
        )
        attempt_result = {
            "attempt": attempt,
            "status": "success" if marker_valid else "failed",
            "return_code": return_code,
            "error": error,
            "elapsed_seconds": time.time() - started,
            "log": str(log),
            "log_sha256": base._sha256(log),
        }
        attempts.append(attempt_result)
        if marker_valid:
            return {
                "arm": arm,
                "pair_index": pair_index,
                "status": "success",
                "return_code": return_code,
                "elapsed_seconds": sum(row["elapsed_seconds"] for row in attempts),
                "log": str(log),
                "log_sha256": attempt_result["log_sha256"],
                "attempts": attempts,
                "identity": identity,
            }
    final = attempts[-1]
    return {
        "arm": arm,
        "pair_index": pair_index,
        "status": "failed",
        "return_code": final["return_code"],
        "elapsed_seconds": sum(row["elapsed_seconds"] for row in attempts),
        "log": final["log"],
        "log_sha256": final["log_sha256"],
        "attempts": attempts,
        "identity": identity,
    }


def _adjudicate(
    args: argparse.Namespace,
    accepted: list[int],
    task_results: dict[tuple[int, str], dict[str, Any]],
    excluded: list[int],
    surplus: list[int],
) -> dict[str, Any]:
    candidate_wins = 0
    parent_wins = 0
    gains = 0
    losses = 0
    ties = 0
    seat_net = {"p0": 0, "p1": 0}
    natural_terminal_failures = 0
    transport_failures = 0
    matched_pairs = []
    for pair_index in accepted:
        candidate_task = task_results[(pair_index, "candidate")]
        parent_task = task_results[(pair_index, "parent")]
        candidate = base._read_pair(Path(candidate_task["log"]))
        parent = base._read_pair(Path(parent_task["log"]))
        for field in (
            "base_seed",
            "episodes",
            "pair_index",
            "environment_seed",
            "candidate_seats",
        ):
            if candidate.get(field) != parent.get(field):
                raise RuntimeError(f"pair {pair_index} mismatched field {field}")
        candidate_path = Path(candidate_task["log"])
        parent_path = Path(parent_task["log"])
        candidate_natural = _natural_terminal_marker(
            _leg_markers(candidate_path), pair_index
        )
        parent_natural = _natural_terminal_marker(
            _leg_markers(parent_path), pair_index
        )
        natural_terminal_failures += int(not candidate_natural)
        natural_terminal_failures += int(not parent_natural)
        if not _pair_marker(candidate_path, args.base_seed, pair_index):
            transport_failures += 1
        if not _pair_marker(parent_path, args.base_seed, pair_index):
            transport_failures += 1
        seats = candidate["candidate_seats"].split(",")
        candidate_winners = candidate["winners"].split(",")
        parent_winners = parent["winners"].split(",")
        if seats != ["p0", "p1"] or len(candidate_winners) != 2 or len(parent_winners) != 2:
            raise RuntimeError(f"pair {pair_index} has invalid seat or winner fields")
        for seat, candidate_winner, parent_winner in zip(
            seats, candidate_winners, parent_winners
        ):
            candidate_win = candidate_winner == seat
            parent_win = parent_winner == seat
            candidate_wins += int(candidate_win)
            parent_wins += int(parent_win)
            seat_net[seat] += int(candidate_win) - int(parent_win)
            if candidate_win and not parent_win:
                gains += 1
            elif parent_win and not candidate_win:
                losses += 1
            else:
                ties += 1
        matched_pairs.append(
            {
                "pair_index": pair_index,
                "environment_seed": candidate["environment_seed"],
                "candidate_log_sha256": candidate_task["log_sha256"],
                "parent_log_sha256": parent_task["log_sha256"],
            }
        )
    gates = {
        "relative_losses_at_most_gains_plus_20": losses <= gains + 20,
        "candidate_wins_at_least_parent_minus_20": candidate_wins >= parent_wins - 20,
        "p0_candidate_minus_parent_wins_at_least_minus_12": seat_net["p0"] >= -12,
        "p1_candidate_minus_parent_wins_at_least_minus_12": seat_net["p1"] >= -12,
        "all_natural_terminals": natural_terminal_failures == 0,
        "all_transport_checks": transport_failures == 0,
        "exact_target_pairs_matched": len(matched_pairs) == args.target_pairs,
    }
    return {
        "schema": SCHEMA + ".report",
        "formal": args.formal,
        "profile_pairs": args.profile_pairs,
        "base_seed": args.base_seed,
        "target_pairs": args.target_pairs,
        "max_pairs": args.max_pairs,
        "batch_pairs": args.batch_pairs,
        "accepted_pairs": accepted,
        "excluded_pairs": excluded,
        "surplus_unadjudicated_pairs": surplus,
        "matched_pairs": matched_pairs,
        "games": len(accepted) * 2,
        "candidate_wins": candidate_wins,
        "parent_wins": parent_wins,
        "gains": gains,
        "losses": losses,
        "ties": ties,
        "seat_net": seat_net,
        "natural_terminal_failures": natural_terminal_failures,
        "transport_failures": transport_failures,
        "candidate_identity": args.candidate_identity,
        "parent_identity": base.PARENT_IDENTITY,
        "gates": gates,
        "status": "pass" if all(gates.values()) else "fail",
    }


def run(args: argparse.Namespace) -> int:
    if args.evidence_root.exists():
        unexpected = [
            path.name
            for path in args.evidence_root.iterdir()
            if path.name != "manifest.json"
        ]
        if unexpected:
            raise RuntimeError(
                "evidence root may initially contain only manifest.json: "
                + ",".join(sorted(unexpected))
            )
    args.evidence_root.mkdir(parents=True, exist_ok=True)
    (args.evidence_root / "tasks").mkdir()
    for path in (
        args.maven,
        args.mage_repo,
        args.candidate_scorer,
        args.parent_scorer,
        args.candidate_root,
        args.parent_root,
        args.source_database,
    ):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    args.candidate_identity = _candidate_identity(args.candidate_root)
    accepted: list[int] = []
    excluded: list[int] = []
    surplus: list[int] = []
    task_results: dict[tuple[int, str], dict[str, Any]] = {}
    started = time.time()
    state: dict[str, Any] = {}
    for batch_start in range(0, args.max_pairs, args.batch_pairs):
        pair_indices = list(
            range(batch_start, min(batch_start + args.batch_pairs, args.max_pairs))
        )
        tasks = [(arm, pair) for pair in pair_indices for arm in ("candidate", "parent")]
        with concurrent.futures.ThreadPoolExecutor(max_workers=len(tasks)) as executor:
            futures = {
                executor.submit(_run_task, args, worker, arm, pair): (pair, arm)
                for worker, (arm, pair) in enumerate(tasks)
            }
            for future in concurrent.futures.as_completed(futures):
                result = future.result()
                task_results[(result["pair_index"], result["arm"])] = result
        successful = []
        for pair_index in pair_indices:
            both = all(
                task_results[(pair_index, arm)]["status"] == "success"
                for arm in ("candidate", "parent")
            )
            (successful if both else excluded).append(pair_index)
        remaining = args.target_pairs - len(accepted)
        accepted.extend(successful[:remaining])
        surplus.extend(successful[remaining:])
        state = {
            "schema": SCHEMA + ".state",
            "formal": args.formal,
            "profile_pairs": args.profile_pairs,
            "base_seed": args.base_seed,
            "target_pairs": args.target_pairs,
            "max_pairs": args.max_pairs,
            "batch_pairs": args.batch_pairs,
            "candidate_identity": args.candidate_identity,
            "parent_identity": base.PARENT_IDENTITY,
            "accepted_pairs": accepted,
            "excluded_pairs": excluded,
            "surplus_unadjudicated_pairs": surplus,
            "tasks": [task_results[key] for key in sorted(task_results)],
            "outcomes_parsed": False,
            "elapsed_seconds": time.time() - started,
        }
        base._atomic_json(args.evidence_root / "state.json", state)
        if len(accepted) >= args.target_pairs:
            break
    if len(accepted) != args.target_pairs:
        raise RuntimeError(
            f"only {len(accepted)} mutually successful pairs within {args.max_pairs}"
        )
    report = _adjudicate(args, accepted, task_results, excluded, surplus)
    report["elapsed_seconds"] = time.time() - started
    base._atomic_json(args.evidence_root / "report.json", report)
    state["outcomes_parsed"] = True
    state["report_sha256"] = base._sha256(args.evidence_root / "report.json")
    base._atomic_json(args.evidence_root / "state.json", state)
    print(json.dumps(report, sort_keys=True))
    return 0


def self_test() -> int:
    fields = _fields(
        "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=1650001 episodes=2,3 "
        "pair_index=1 environment_seed=00000000000000ab "
        "candidate_seats=p0,p1 opponent=cp7 cp7_skill=7 winners=p1,p0 "
        "turns=4,5 rust_steps=6,7 "
        "physical_decisions=8,9 candidate_priority_projections=1,1 "
        "alignment=selected_action_projection"
    )
    if not _valid_transport_marker(fields, FORMAL_BASE_SEED, 1):
        raise RuntimeError("pair marker self-test failed")
    if _fields("candidate_seat=p0,p1 winners=p1,p0")["winners"] != "p1,p0":
        raise RuntimeError("key-value parser self-test failed")
    print("run_matched_gate_v1: SELF-TEST PASS")
    return 0


def _arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--profile-pairs", type=int)
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--base-seed", type=int)
    parser.add_argument("--max-pairs", type=int)
    parser.add_argument("--batch-pairs", type=int)
    parser.add_argument(
        "--mage-repo",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1"),
    )
    parser.add_argument(
        "--candidate-scorer",
        "--search-scorer",
        dest="candidate_scorer",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"
        ),
    )
    parser.add_argument(
        "--parent-scorer",
        type=Path,
        default=Path(
            r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"
        ),
    )
    parser.add_argument("--candidate-root", "--search-root", dest="candidate_root", type=Path)
    parser.add_argument("--parent-root", type=Path)
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
    args = parser.parse_args(argv)
    if args.self_test:
        return args
    if (
        args.evidence_root is None
        or args.candidate_root is None
        or args.parent_root is None
    ):
        parser.error("--evidence-root, --candidate-root, and --parent-root are required")
    if args.profile_pairs is None:
        if args.base_seed not in (None, FORMAL_BASE_SEED):
            parser.error("formal base seed is fixed to 1650001")
        if args.max_pairs not in (None, FORMAL_MAX_PAIRS):
            parser.error("formal max-pairs is fixed to 1280")
        if args.batch_pairs not in (None, FORMAL_BATCH_PAIRS):
            parser.error("formal batch-pairs is fixed to 8")
        args.formal = True
        args.profile_pairs = None
        args.base_seed = FORMAL_BASE_SEED
        args.target_pairs = FORMAL_TARGET_PAIRS
        args.max_pairs = FORMAL_MAX_PAIRS
        args.batch_pairs = FORMAL_BATCH_PAIRS
    else:
        if not 1 <= args.profile_pairs <= PROFILE_MAX_PAIRS:
            parser.error(f"--profile-pairs must be in [1,{PROFILE_MAX_PAIRS}]")
        if args.base_seed is None:
            args.base_seed = FORMAL_BASE_SEED
        if args.max_pairs is None:
            args.max_pairs = min(
                PROFILE_MAX_PAIRS,
                max(args.profile_pairs + 2, (args.profile_pairs * 5 + 3) // 4),
            )
        if args.batch_pairs is None:
            args.batch_pairs = min(FORMAL_BATCH_PAIRS, args.profile_pairs)
        if not 1 <= args.batch_pairs <= FORMAL_BATCH_PAIRS:
            parser.error("profile batch-pairs must be in [1,8]")
        if not args.profile_pairs <= args.max_pairs <= PROFILE_MAX_PAIRS:
            parser.error("profile requires profile-pairs <= max-pairs <= 64")
        args.formal = False
        args.target_pairs = args.profile_pairs
    return args


if __name__ == "__main__":
    try:
        parsed = _arguments()
        sys.exit(self_test() if parsed.self_test else run(parsed))
    except Exception as error:
        print(f"run_matched_gate_v1: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
