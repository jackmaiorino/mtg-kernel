#!/usr/bin/env python3
"""Run a rapid matched XMage gate with outcome-blind mapper exclusions."""

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


CARD_DB_SHA256 = "e7c9825ca95c327461d3fa677adb5f5b0b2d4dc854e2b81ad6d8187d993df4ae"
SEARCH_IDENTITY = {
    "adam_step": "1",
    "manifest": "56c4d38d6c0f0d27b23981d41801226145e6b0f4bf8a003c67e36716a84ad14f",
    "payload": "6e5a8ae39a3267ad48f1e87991792e23fcb4eeefe8440fc33ff10fcc9f58b76e",
    "train_state": "47d6a9491865b6954ca81bcd6037295af24cdbc9a5b79d5db14ce55140c1acf1",
    "model": "70b9e196ac6f7e7c391c2537d7173d6f9a87a7bdd6728e56d695cd83346a3463",
}
PARENT_IDENTITY = {
    "adam_step": "1",
    "manifest": "706b3aa80ec7a3c067d458fef06bb2237320543f202fb2349c5cb885975fdbbb",
    "payload": "eb83be33bcb7418b6f85ec9687da4b7ca5620a1df64721a1942d2793588bbd3c",
    "train_state": "2c55a13abb3157f3f4ba012af663ffa56599c5d6cb90743c1ba6e024ca47a9c8",
    "model": "883e4882d01d9cb55ecd7a4ae00e3c95793b6147baf3df08650ef1fa7f8e9546",
}
BOUNDED_SEARCH_IDENTITY = {
    "adam_step": "1",
    "manifest": "0d883d169fca504e4a413810454565d98cd0e8316cb76e7de4f538187b2865c9",
    "payload": "c55b61678aed544580a692e70a0f72e9df64018ce2d975421e81089d1b3a32d9",
    "train_state": "86f9d6795e8aecd6d32ab8cacb70dbb1e14a33769a3c5ac30a5fce41031408b3",
    "model": "c55b61678aed544580a692e70a0f72e9df64018ce2d975421e81089d1b3a32d9",
}
QUALIFIED_POLICY_IDENTITY = {
    "adam_step": "1",
    "manifest": "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72",
    "payload": "ca3c45cd69d8d60f1f921bc78c27b098064ef6b16fe7566b84e5045681781b28",
    "train_state": "7d854edb46119a611d4283e6cf4630d0207ceb24c12b4089a7d27a43c97fe0b3",
    "model": "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3",
}
PAIR_PREFIX = "XMAGE_RALLY_ANCHOR_PAIR PASS "
DIAGNOSTIC_PREFIXES = {
    "one-step": "NATIVE_ONE_STEP_HISTORY_VALUE ",
    "bounded-candidate-turn": "NATIVE_BOUNDED_CANDIDATE_TURN_VALUE ",
    "depth8": "NATIVE_DEPTH8_HISTORY_VALUE ",
    "depth8-cp7-opponent": "NATIVE_DEPTH8_CP7_OPPONENT_HISTORY_VALUE ",
}
KEY_VALUE = re.compile(r"([a-z_]+)=([^ ]+)")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _atomic_json(path: Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    os.replace(temporary, path)


def _fields(line: str) -> dict[str, str]:
    return dict(KEY_VALUE.findall(line.strip()))


def _pair_marker(path: Path, base_seed: int, pair_index: int) -> bool:
    markers = []
    with path.open("r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if line.startswith(PAIR_PREFIX):
                markers.append(_fields(line))
    if len(markers) != 1:
        return False
    marker = markers[0]
    return (
        marker.get("base_seed") == str(base_seed)
        and marker.get("pair_index") == str(pair_index)
        and marker.get("episodes") == f"{pair_index * 2},{pair_index * 2 + 1}"
        and marker.get("candidate_seats") == "p0,p1"
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
        if _sha256(database) != CARD_DB_SHA256:
            raise RuntimeError(f"worker {worker} card database copy SHA-256 mismatch")
    return root


def _run_task(
    args: argparse.Namespace, worker: int, arm: str, pair_index: int
) -> dict[str, Any]:
    database = _worker_database(args, worker)
    log = args.evidence_root / "tasks" / f"{arm}-pair-{pair_index:04d}.log"
    if log.exists():
        raise RuntimeError(f"refusing to overwrite {log}")
    scorer = args.search_scorer if arm == "search" else args.parent_scorer
    outcome_root = args.search_root if arm == "search" else args.parent_root
    if args.selector == "bounded-candidate-turn":
        identity = (
            BOUNDED_SEARCH_IDENTITY if arm == "search" else QUALIFIED_POLICY_IDENTITY
        )
    else:
        identity = SEARCH_IDENTITY if arm == "search" else PARENT_IDENTITY
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
    if args.selector == "depth8-cp7-opponent":
        environment["MTG_KERNEL_CP7_OPPONENT_ROOT"] = str(args.cp7_opponent_root)
    started = time.time()
    with log.open("x", encoding="utf-8", newline="\n") as output:
        completed = subprocess.run(
            command,
            cwd=args.mage_repo,
            env=environment,
            stdout=output,
            stderr=subprocess.STDOUT,
            check=False,
        )
    marker_valid = completed.returncode == 0 and _pair_marker(
        log, args.base_seed, pair_index
    )
    return {
        "arm": arm,
        "pair_index": pair_index,
        "status": "success" if marker_valid else "failed",
        "return_code": completed.returncode,
        "elapsed_seconds": time.time() - started,
        "log": str(log),
        "log_sha256": _sha256(log),
    }


def _read_pair(path: Path) -> dict[str, str]:
    markers = []
    with path.open("r", encoding="utf-8", errors="strict") as handle:
        for line in handle:
            if line.startswith(PAIR_PREFIX):
                markers.append(_fields(line))
    if len(markers) != 1:
        raise RuntimeError(f"{path} does not contain exactly one pair marker")
    return markers[0]


def _search_diagnostics(path: Path, prefix: str) -> list[dict[str, str]]:
    rows = []
    with path.open("r", encoding="utf-8", errors="strict") as handle:
        for line in handle:
            if line.startswith(prefix):
                rows.append(_fields(line))
    return rows


def _adjudicate(
    args: argparse.Namespace,
    accepted: list[int],
    task_results: dict[tuple[int, str], dict[str, Any]],
    excluded: list[int],
    surplus: list[int],
) -> dict[str, Any]:
    search_wins = 0
    parent_wins = 0
    gains = 0
    losses = 0
    ties = 0
    seat_net = {"p0": 0, "p1": 0}
    eligible = {"p0": 0, "p1": 0}
    ineligible = {"p0": 0, "p1": 0}
    overrides = {"p0": 0, "p1": 0}
    sample_violations = 0
    diagnostic_contract_violations = 0
    matched_pairs = []
    for pair_index in accepted:
        search_task = task_results[(pair_index, "search")]
        parent_task = task_results[(pair_index, "parent")]
        search = _read_pair(Path(search_task["log"]))
        parent = _read_pair(Path(parent_task["log"]))
        for field in ("base_seed", "episodes", "pair_index", "environment_seed", "candidate_seats"):
            if search.get(field) != parent.get(field):
                raise RuntimeError(f"pair {pair_index} mismatched field {field}")
        seats = search["candidate_seats"].split(",")
        search_winners = search["winners"].split(",")
        parent_winners = parent["winners"].split(",")
        if seats != ["p0", "p1"] or len(search_winners) != 2 or len(parent_winners) != 2:
            raise RuntimeError(f"pair {pair_index} has invalid seat or winner fields")
        for seat, search_winner, parent_winner in zip(
            seats, search_winners, parent_winners
        ):
            search_win = search_winner == seat
            parent_win = parent_winner == seat
            search_wins += int(search_win)
            parent_wins += int(parent_win)
            seat_net[seat] += int(search_win) - int(parent_win)
            if search_win and not parent_win:
                gains += 1
            elif parent_win and not search_win:
                losses += 1
            else:
                ties += 1
        diagnostics = _search_diagnostics(
            Path(search_task["log"]), DIAGNOSTIC_PREFIXES[args.selector]
        )
        for row in diagnostics:
            episode = int(row["episode"])
            seat = "p0" if episode % 2 == 0 else "p1"
            row_eligible = row.get("eligible", "true") == "true"
            eligible[seat] += int(row_eligible)
            ineligible[seat] += int(not row_eligible)
            overrides[seat] += int(row.get("override") == "true")
            hashes = row.get("sampled_hashes", "").split(",")
            if row.get("information_set_samples") != "4" or len(hashes) != 4 or len(set(hashes)) != 4:
                sample_violations += 1
            if args.selector.startswith("depth8") and row.get("continuation_steps") != "8":
                diagnostic_contract_violations += 1
            if (
                args.selector == "depth8-cp7-opponent"
                and row.get("opponent_policy") != "cp7_behavior_clone_sample"
            ):
                diagnostic_contract_violations += 1
            if args.selector == "bounded-candidate-turn":
                opponent_successors = int(row.get("opponent_successors", "-1"))
                if (row_eligible and opponent_successors != 0) or (
                    not row_eligible and opponent_successors <= 0
                ):
                    diagnostic_contract_violations += 1
        matched_pairs.append(
            {
                "pair_index": pair_index,
                "environment_seed": search["environment_seed"],
                "search_log_sha256": search_task["log_sha256"],
                "parent_log_sha256": parent_task["log_sha256"],
            }
        )
    gates = {
        "paired_gain": gains >= losses + 2,
        "p0_net": seat_net["p0"] >= -1,
        "p1_net": seat_net["p1"] >= -1,
        "p0_override": overrides["p0"] >= 1,
        "p1_override": overrides["p1"] >= 1,
        "sample_distinctness": sample_violations == 0,
        "diagnostic_contract": diagnostic_contract_violations == 0,
    }
    return {
        "schema": "mtg-kernel-history-value-search-matched-gate-report/v2",
        "selector": args.selector,
        "base_seed": args.base_seed,
        "accepted_pairs": accepted,
        "excluded_pairs": excluded,
        "surplus_unadjudicated_pairs": surplus,
        "matched_pairs": matched_pairs,
        "games": len(accepted) * 2,
        "search_wins": search_wins,
        "parent_wins": parent_wins,
        "gains": gains,
        "losses": losses,
        "ties": ties,
        "seat_net": seat_net,
        "eligible_roots": eligible,
        "ineligible_opponent_successor_roots": ineligible,
        "overrides": overrides,
        "sample_distinctness_violations": sample_violations,
        "diagnostic_contract_violations": diagnostic_contract_violations,
        "gates": gates,
        "status": "pass" if all(gates.values()) else "fail",
    }


def _run(args: argparse.Namespace) -> int:
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
        args.search_scorer,
        args.parent_scorer,
        args.search_root,
        args.parent_root,
        args.source_database,
    ):
        if not path.exists():
            raise RuntimeError(f"required path does not exist: {path}")
    if args.selector == "depth8-cp7-opponent" and not args.cp7_opponent_root.exists():
        raise RuntimeError(
            f"required CP7 opponent root does not exist: {args.cp7_opponent_root}"
        )
    if _sha256(args.source_database) != CARD_DB_SHA256:
        raise RuntimeError("source card database SHA-256 mismatch")
    accepted: list[int] = []
    excluded: list[int] = []
    surplus: list[int] = []
    task_results: dict[tuple[int, str], dict[str, Any]] = {}
    started = time.time()
    for batch_start in range(0, args.max_pairs, args.batch_pairs):
        pair_indices = list(
            range(batch_start, min(batch_start + args.batch_pairs, args.max_pairs))
        )
        tasks = [(arm, pair) for pair in pair_indices for arm in ("search", "parent")]
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
                for arm in ("search", "parent")
            )
            (successful if both else excluded).append(pair_index)
        remaining = args.target_pairs - len(accepted)
        accepted.extend(successful[:remaining])
        surplus.extend(successful[remaining:])
        state = {
            "schema": "mtg-kernel-history-value-search-matched-gate-state/v2",
            "selector": args.selector,
            "base_seed": args.base_seed,
            "target_pairs": args.target_pairs,
            "accepted_pairs": accepted,
            "excluded_pairs": excluded,
            "surplus_unadjudicated_pairs": surplus,
            "tasks": [task_results[key] for key in sorted(task_results)],
            "outcomes_parsed": False,
            "elapsed_seconds": time.time() - started,
        }
        _atomic_json(args.evidence_root / "state.json", state)
        if len(accepted) >= args.target_pairs:
            break
    if len(accepted) != args.target_pairs:
        raise RuntimeError(
            f"only {len(accepted)} mutually successful pairs within {args.max_pairs}"
        )
    report = _adjudicate(args, accepted, task_results, excluded, surplus)
    report["elapsed_seconds"] = time.time() - started
    _atomic_json(args.evidence_root / "report.json", report)
    state["outcomes_parsed"] = True
    state["report_sha256"] = _sha256(args.evidence_root / "report.json")
    _atomic_json(args.evidence_root / "state.json", state)
    print(json.dumps(report, sort_keys=True))
    return 0


def _self_test() -> int:
    fields = _fields(
        "XMAGE_RALLY_ANCHOR_PAIR PASS base_seed=7 episodes=2,3 "
        "pair_index=1 environment_seed=abc candidate_seats=p0,p1 winners=p1,p0"
    )
    if fields["pair_index"] != "1" or fields["winners"] != "p1,p0":
        raise RuntimeError("pair parser self-test failed")
    diagnostic = _fields(
        "NATIVE_ONE_STEP_HISTORY_VALUE episode=2 information_set_samples=4 "
        "sampled_hashes=a,b,c,d override=true"
    )
    if diagnostic["sampled_hashes"].split(",") != ["a", "b", "c", "d"]:
        raise RuntimeError("diagnostic parser self-test failed")
    bounded_diagnostic = _fields(
        "NATIVE_BOUNDED_CANDIDATE_TURN_VALUE episode=3 eligible=false "
        "candidate_successors=4 opponent_successors=1 terminal_successors=3 "
        "information_set_samples=4 sampled_hashes=a,b,c,d override=false"
    )
    if (
        bounded_diagnostic["eligible"] != "false"
        or bounded_diagnostic["opponent_successors"] != "1"
    ):
        raise RuntimeError("bounded diagnostic parser self-test failed")
    cp7_diagnostic = _fields(
        "NATIVE_DEPTH8_CP7_OPPONENT_HISTORY_VALUE episode=2 continuation_steps=8 "
        "opponent_policy=cp7_behavior_clone_sample information_set_samples=4 "
        "sampled_hashes=a,b,c,d override=true"
    )
    if (
        cp7_diagnostic["continuation_steps"] != "8"
        or cp7_diagnostic["opponent_policy"] != "cp7_behavior_clone_sample"
    ):
        raise RuntimeError("CP7-opponent diagnostic parser self-test failed")
    print("run_matched_gate_v2: SELF-TEST PASS")
    return 0


def _arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--selector", choices=tuple(DIAGNOSTIC_PREFIXES), default="one-step"
    )
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--base-seed", type=int)
    parser.add_argument("--target-pairs", type=int, default=8)
    parser.add_argument("--max-pairs", type=int, default=32)
    parser.add_argument("--batch-pairs", type=int, default=4)
    parser.add_argument(
        "--mage-repo",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1"),
    )
    parser.add_argument(
        "--search-scorer",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_history_value_stdio_v1.exe"),
    )
    parser.add_argument(
        "--parent-scorer",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mtg-kernel-structured-successor-screen-v1-codex\target\release\checkpoint_shadow_stdio_v1.exe"),
    )
    parser.add_argument(
        "--search-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-complete-history-live-v1\candidate"),
    )
    parser.add_argument(
        "--parent-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-complete-history-live-v1\candidate\parent"),
    )
    parser.add_argument(
        "--cp7-opponent-root",
        type=Path,
        default=Path(r"D:\mtg-kernel-cp7-bc-train-base970001-grid-strict-v1"),
    )
    parser.add_argument(
        "--source-database",
        type=Path,
        default=Path(r"C:\Users\Jack\IdeaProjects\mage-kernel-anchor-spike-v1\db\cards.h2.mv.db"),
    )
    parser.add_argument(
        "--maven",
        type=Path,
        default=Path(r"C:\Program Files\apache-maven-3.9.8\bin\mvn.cmd"),
    )
    args = parser.parse_args()
    if args.self_test:
        return args
    if args.evidence_root is None or args.base_seed is None:
        parser.error("--evidence-root and --base-seed are required")
    if not (1 <= args.target_pairs <= args.max_pairs <= 128):
        parser.error("require 1 <= target-pairs <= max-pairs <= 128")
    if not (1 <= args.batch_pairs <= 4):
        parser.error("batch-pairs must be in [1,4]")
    return args


if __name__ == "__main__":
    try:
        parsed = _arguments()
        sys.exit(_self_test() if parsed.self_test else _run(parsed))
    except Exception as error:
        print(f"run_matched_gate_v2: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
