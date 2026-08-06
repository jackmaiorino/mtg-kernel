#!/usr/bin/env python3
"""Run a terminal-only, three-model CP7 Rally panel from verified StoreV2 checkpoints.

This runner has no promotion decision. It accepts only population Store roots,
binds each requested generation before launching XMage, and creates a new
evidence root for an explicit smoke or formal panel.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
from typing import Any


AUTHORITY_KIND = "population-store-validated-generation"
ENVIRONMENT_CONTRACT = "environment-randomization-v2"
SAMPLER_IDENTITY = "f32-q8-expq63-hamilton-splitmix64-v1"
SAMPLER_CONTRACT = "276407494966b195b7c011caf984d2354484f7532161107b19ecc83388de92b6"
OUTCOME_CONTRACT = "mtg-kernel-xmage-cp7-outcome-jsonl/v2"
CARD_DB_HASH = "b833d6a7b44ad1f7bd6aef9a21d1f2498136ef61e44db0e48e60e5ec471ce09d"


def fail(message: str) -> None:
    raise ValueError(message)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_canonical_json(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    if not raw.endswith(b"\n") or b"\r" in raw:
        fail(f"noncanonical JSON file: {path}")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"invalid JSON {path}: {error}")
    if not isinstance(value, dict):
        fail(f"JSON document is not an object: {path}")
    return value


def require_sha(value: Any, context: str) -> str:
    if not isinstance(value, str) or len(value) != 64 or any(
        character not in "0123456789abcdef" for character in value
    ):
        fail(f"invalid SHA-256 {context}")
    return value


def _version(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        text=True, check=True,
    )
    return completed.stdout.splitlines()[0] if completed.stdout else ""


def _git_commit(repo: Path) -> str:
    return _version(["git", "-c", f"safe.directory={repo}", "-C", str(repo),
                     "rev-parse", "HEAD"])


def load_store_identity(root: Path, generation: int) -> dict[str, Any]:
    if generation < 0 or not root.is_dir():
        fail("population Store root or generation is invalid")
    root = root.resolve()
    stem = f"update-{generation:08d}"
    run_path = root / "run.json"
    checkpoint_path = root / "checkpoints" / f"{stem}.checkpoint.json"
    sidecar_path = root / "checkpoints" / f"{stem}.sidecar.json"
    state_path = root / "checkpoints" / f"{stem}.state.f32le"
    latest_path = root / "latest.json"
    if not all(path.is_file() for path in (run_path, checkpoint_path, sidecar_path, state_path, latest_path)):
        fail(f"population Store generation files are incomplete: {root} g{generation}")
    run = load_canonical_json(run_path)
    checkpoint = load_canonical_json(checkpoint_path)
    sidecar = load_canonical_json(sidecar_path)
    latest = load_canonical_json(latest_path)
    run_sha = sha256(run_path)
    checkpoint_sha = sha256(checkpoint_path)
    sidecar_sha = sha256(sidecar_path)
    state_sha = sha256(state_path)
    if (
        run.get("schema") != "mtg_kernel_native_train_run/v2"
        or run.get("store_identity") != "mtg-kernel-native-training-store-v2"
        or checkpoint.get("schema") != "mtg_kernel_native_train_checkpoint/v3"
        or sidecar.get("schema") != "mtg_kernel_native_train_checkpoint_sidecar/v2"
        or latest.get("schema") != "mtg_kernel_native_train_latest/v2"
        or checkpoint.get("generation_index") != generation
        or sidecar.get("generation_index") != generation
        or latest.get("generation_index") != generation
    ):
        fail("population Store schema or generation mismatch")
    payload = checkpoint.get("payload")
    train_state = checkpoint.get("train_state")
    contracts = run.get("contracts")
    sampler = contracts.get("learner_sampler") if isinstance(contracts, dict) else None
    if not isinstance(payload, dict) or not isinstance(train_state, dict) or not isinstance(sampler, dict):
        fail("population Store checkpoint fields are incomplete")
    if (
        payload.get("byte_count") != state_path.stat().st_size
        or require_sha(payload.get("sha256"), "checkpoint payload") != state_sha
        or require_sha(checkpoint.get("run_sha256"), "checkpoint run") != run_sha
        or require_sha(sidecar.get("run_sha256"), "sidecar run") != run_sha
        or require_sha(sidecar.get("checkpoint_manifest_sha256"), "sidecar checkpoint") != checkpoint_sha
        or require_sha(sidecar.get("checkpoint_payload_sha256"), "sidecar payload") != state_sha
        or require_sha(sidecar.get("train_state_sha256"), "sidecar train state")
            != require_sha(train_state.get("state_sha256"), "checkpoint train state")
        or require_sha(sidecar.get("model_parameter_sha256"), "sidecar model")
            != require_sha(train_state.get("model_parameter_sha256"), "checkpoint model")
        or require_sha(latest.get("run_sha256"), "latest run") != run_sha
        or require_sha(latest.get("identity_bundle_sha256"), "latest identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or require_sha(run.get("contracts", {}).get("identity_bundle_sha256"), "run identity bundle")
            != require_sha(checkpoint.get("identity_bundle_sha256"), "checkpoint identity bundle")
        or sampler.get("identity") != SAMPLER_IDENTITY
        or sampler.get("contract_sha256") != SAMPLER_CONTRACT
    ):
        fail("population Store checkpoint chain mismatch")
    identity = {
        "authority_kind": AUTHORITY_KIND,
        "source_run_sha256": run_sha,
        "source_generation": generation,
        "source_checkpoint_sha256": checkpoint_sha,
        "source_sidecar_sha256": sidecar_sha,
        "source_payload_sha256": state_sha,
        "source_train_state_sha256": train_state["state_sha256"],
        "loaded_run_sha256": run_sha,
        "loaded_generation": generation,
        "loaded_checkpoint_sha256": checkpoint_sha,
        "loaded_payload_sha256": state_sha,
        "loaded_train_state_sha256": train_state["state_sha256"],
        "model_parameter_sha256": train_state["model_parameter_sha256"],
        "environment_trajectory_contract": ENVIRONMENT_CONTRACT,
        "sampler_identity": SAMPLER_IDENTITY,
        "sampler_contract_sha256": SAMPLER_CONTRACT,
    }
    return {
        "root": str(root), "generation": generation, "checkpoint": identity,
        "store_files": {
            "run_json_sha256": run_sha, "checkpoint_json_sha256": checkpoint_sha,
            "sidecar_json_sha256": sidecar_sha, "state_payload_sha256": state_sha,
            "identity_bundle_sha256": checkpoint["identity_bundle_sha256"],
        },
        "payload": {
            "byte_count": payload["byte_count"],
            "parameters_sha256": payload["sections"][0]["sha256"],
            "first_moments_sha256": payload["sections"][1]["sha256"],
            "second_moments_sha256": payload["sections"][2]["sha256"],
        },
    }


def maven_opts(identity: dict[str, Any]) -> str:
    prefix = "-Dxmage.rally.populationStore."
    names = {
        "authorityKind": "authority_kind", "sourceRunSha256": "source_run_sha256",
        "sourceGeneration": "source_generation", "sourceCheckpointSha256": "source_checkpoint_sha256",
        "sourceSidecarSha256": "source_sidecar_sha256", "sourcePayloadSha256": "source_payload_sha256",
        "sourceTrainStateSha256": "source_train_state_sha256", "loadedRunSha256": "loaded_run_sha256",
        "loadedGeneration": "loaded_generation", "loadedCheckpointSha256": "loaded_checkpoint_sha256",
        "loadedPayloadSha256": "loaded_payload_sha256", "loadedTrainStateSha256": "loaded_train_state_sha256",
        "modelParameterSha256": "model_parameter_sha256",
        "environmentTrajectoryContract": "environment_trajectory_contract",
        "samplerIdentity": "sampler_identity", "samplerContractSha256": "sampler_contract_sha256",
    }
    return " ".join(prefix + key + "=" + str(identity[value]) for key, value in names.items())


def anchor_command(args: argparse.Namespace, model: dict[str, Any], pair: int, outcome: Path) -> list[str]:
    execution_args = " ".join((
        "--repo-root", str(args.mage_repo), "--scorer-exe", str(args.scorer_exe),
        "--population-store-root", model["root"], "--generation", str(args.generation),
        "--base-seed", str(args.base_seed), "--first-episode", str(pair * 2),
        "--pairs", "1", "--opponent", "cp7", "--cp7-skill", "7", "--outcome-export", str(outcome),
    ))
    return [str(args.maven), "-o", "-q", "-pl", "Mage.Server.Plugins/Mage.Player.AIRL",
            "-DskipTests", "exec:java", "-Dexec.mainClass=mage.player.ai.rl.XMageRallyAnchorSpike",
            "-Dexec.args=" + execution_args]


def environment(database_root: Path, model: dict[str, Any]) -> dict[str, str]:
    value = os.environ.copy()
    value.update({"MAGE_DB_DIR": str(database_root), "MAGE_DB_AUTO_SERVER": "false",
                  "AI_DETERMINISTIC_TIEBREAKS": "true", "AI_DETERMINISTIC_SEARCH": "true",
                  "AI_DETERMINISTIC_MAX_NODES": "5000", "AI_MAX_THREADS_FOR_SIMULATIONS": "1",
                  "CUDA_VISIBLE_DEVICES": "1", "MAVEN_OPTS": maven_opts(model["checkpoint"])})
    return value


def parse_outcome(path: Path, model: dict[str, Any], base_seed: int, pair: int) -> dict[str, Any]:
    rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    if not rows or rows[0].get("record_type") != "header":
        fail(f"outcome header missing: {path}")
    expected = model["checkpoint"]
    if rows[0].get("export_contract") != OUTCOME_CONTRACT or rows[0].get("checkpoint") != expected:
        fail(f"outcome header identity mismatch: {path}")
    terminals = [row for row in rows if row.get("record_type") == "terminal"]
    if len(terminals) != 2:
        fail(f"expected two terminal outcomes: {path}")
    results: dict[str, str] = {}
    seed = None
    for row in terminals:
        seat, terminal, reward = row.get("candidate_seat"), row.get("terminal"), row.get("candidate_terminal_reward")
        if (seat not in {"p0", "p1"} or row.get("pair_index") != pair
                or row.get("episode_id") != pair * 2 + int(seat[1])
                or row.get("base_seed_u64_hex") != f"{base_seed:016x}"
                or row.get("checkpoint") != expected or not isinstance(terminal, dict)
                or terminal.get("terminal_classification") != "natural"
                or terminal.get("terminal_code") != "natural_game_over" or reward not in {-1, 0, 1}):
            fail(f"terminal contract mismatch: {path}")
        if seed is None:
            seed = row.get("pair_environment_seed_u64_hex")
        elif seed != row.get("pair_environment_seed_u64_hex"):
            fail(f"pair environment seed changed: {path}")
        results[seat] = "win" if reward == 1 else "draw" if reward == 0 else "loss"
    if set(results) != {"p0", "p1"} or not isinstance(seed, str):
        fail(f"terminal seat coverage mismatch: {path}")
    return {"environment_seed": seed, "by_seat": results}


def run_task(args: argparse.Namespace, worker: int, database: Path, label: str,
             model: dict[str, Any], pair: int) -> dict[str, Any]:
    task_root = args.evidence_root / "tasks"
    stem = f"{label}-pair-{pair:04d}"
    log, outcome = task_root / (stem + ".log"), task_root / (stem + ".outcome.jsonl")
    started = time.perf_counter()
    with log.open("x", encoding="utf-8", newline="\n") as handle:
        completed = subprocess.run(anchor_command(args, model, pair, outcome), cwd=args.mage_repo,
                                   env=environment(database, model), stdout=handle,
                                   stderr=subprocess.STDOUT, timeout=args.task_timeout_seconds)
    if completed.returncode != 0 or not outcome.is_file():
        fail(f"panel task failed: {label} pair {pair}")
    parsed = parse_outcome(outcome, model, args.base_seed, pair)
    return {"label": label, "pair_index": pair, "worker": worker,
            "elapsed_seconds": time.perf_counter() - started, "log": str(log),
            "log_sha256": sha256(log), "outcome": str(outcome), "outcome_sha256": sha256(outcome),
            **parsed}


def aggregate_terminal_wdl(results: list[dict[str, Any]], labels: list[str]) -> dict[str, dict[str, Any]]:
    summary: dict[str, dict[str, Any]] = {}
    for label in labels:
        rows = [row for row in results if row["label"] == label]
        counts = {"win": 0, "draw": 0, "loss": 0}
        seats = {seat: {"win": 0, "draw": 0, "loss": 0} for seat in ("p0", "p1")}
        for row in rows:
            for seat, result in row["by_seat"].items():
                if seat not in seats or result not in counts:
                    fail("terminal aggregation input is invalid")
                counts[result] += 1
                seats[seat][result] += 1
        summary[label] = {"overall_wdl": counts, "by_seat_wdl": seats}
    return summary


def run(args: argparse.Namespace) -> dict[str, Any]:
    if args.evidence_root.exists():
        fail(f"evidence root already exists: {args.evidence_root}")
    if args.pairs != (1 if args.mode == "smoke" else 128):
        fail("smoke requires one pair and formal requires 128 pairs")
    if sha256(args.source_database) != CARD_DB_HASH:
        fail("card database hash mismatch")
    models = dict(args.models)
    identities = {label: load_store_identity(root, args.generation) for label, root in models.items()}
    if len({identity["store_files"]["identity_bundle_sha256"] for identity in identities.values()}) != 1:
        fail("population Store models do not share one identity-bundle root")
    args.evidence_root.mkdir(parents=True)
    (args.evidence_root / "tasks").mkdir()
    workers: list[Path] = []
    for worker in range(args.workers):
        root = args.evidence_root / "workers" / f"worker-{worker:02d}" / "db"
        root.mkdir(parents=True)
        target = root / "cards.h2.mv.db"
        shutil.copyfile(args.source_database, target)
        if sha256(target) != CARD_DB_HASH:
            fail("worker card database copy hash mismatch")
        workers.append(root)
    pairs = list(range(args.pair_start, args.pair_start + args.pairs))
    assignments = [(label, pair) for pair in pairs for label in sorted(identities)]
    results: list[dict[str, Any]] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = [executor.submit(run_task, args, index % args.workers, workers[index % args.workers],
                                   label, identities[label], pair)
                   for index, (label, pair) in enumerate(assignments)]
        for future in concurrent.futures.as_completed(futures):
            results.append(future.result())
    results.sort(key=lambda row: (row["pair_index"], row["label"]))
    summary = aggregate_terminal_wdl(results, sorted(identities))
    for pair in pairs:
        seeds = {row["environment_seed"] for row in results if row["pair_index"] == pair}
        if len(seeds) != 1:
            fail(f"models did not share pair environment seed for pair {pair}")
    manifest = {
        "schema": "mtg-kernel-population-store-cp7-panel/v1", "mode": args.mode,
        "base_seed": args.base_seed, "pair_start": args.pair_start, "pairs": args.pairs,
        "workers": args.workers, "task_pairs": args.task_pairs,
        "models": identities, "tasks": results, "terminal_wdl": summary,
        "provenance": {"scorer_sha256": sha256(args.scorer_exe), "mage_commit": _git_commit(args.mage_repo),
                       "card_database_sha256": CARD_DB_HASH, "runner_sha256": sha256(Path(__file__)),
                       "python": sys.version, "rustc": _version(["rustc", "-V"]),
                       "maven": _version([str(args.maven), "-version"])},
        "non_claims": ["terminal win/loss/draw is the only playing-strength outcome",
                       "this external CP7 anchor panel is not a promotion or professional-level claim"],
    }
    output = args.evidence_root / "panel-summary.json"
    with output.open("x", encoding="utf-8", newline="\n") as handle:
        json.dump(manifest, handle, indent=2, sort_keys=True)
        handle.write("\n")
    return manifest


def self_test() -> int:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory) / "store"; (root / "checkpoints").mkdir(parents=True)
        def write_json(path: Path, value: dict[str, Any]) -> None:
            with path.open("w", encoding="utf-8", newline="\n") as handle:
                handle.write(json.dumps(value, sort_keys=True) + "\n")
        generation = 1024; state = b"state"; state_sha = hashlib.sha256(state).hexdigest()
        run = {"schema": "mtg_kernel_native_train_run/v2", "store_identity": "mtg-kernel-native-training-store-v2",
               "contracts": {"identity_bundle_sha256": "a" * 64,
                             "learner_sampler": {"identity": SAMPLER_IDENTITY, "contract_sha256": SAMPLER_CONTRACT}}}
        write_json(root / "run.json", run)
        run_sha = sha256(root / "run.json")
        checkpoint = {"schema": "mtg_kernel_native_train_checkpoint/v3", "generation_index": generation,
                      "identity_bundle_sha256": "a" * 64, "run_sha256": run_sha,
                      "payload": {"byte_count": 5, "sha256": state_sha,
                                  "sections": [{"sha256": "1" * 64}, {"sha256": "2" * 64}, {"sha256": "3" * 64}]},
                      "train_state": {"state_sha256": "4" * 64, "model_parameter_sha256": "5" * 64}}
        checkpoint_path = root / "checkpoints" / "update-00001024.checkpoint.json"
        write_json(checkpoint_path, checkpoint)
        checkpoint_sha = sha256(checkpoint_path)
        sidecar = {"schema": "mtg_kernel_native_train_checkpoint_sidecar/v2", "generation_index": generation,
                   "run_sha256": run_sha, "checkpoint_manifest_sha256": checkpoint_sha,
                   "checkpoint_payload_sha256": state_sha, "train_state_sha256": "4" * 64,
                   "model_parameter_sha256": "5" * 64}
        write_json(root / "checkpoints" / "update-00001024.sidecar.json", sidecar)
        (root / "checkpoints" / "update-00001024.state.f32le").write_bytes(state)
        write_json(root / "latest.json", {"schema": "mtg_kernel_native_train_latest/v2", "generation_index": generation, "run_sha256": run_sha, "identity_bundle_sha256": "a" * 64})
        identity = load_store_identity(root, generation)
        assert identity["checkpoint"]["loaded_generation"] == generation
        try:
            load_store_identity(root, generation + 4)
            raise AssertionError("missing generation was accepted")
        except ValueError:
            pass
        aggregate = aggregate_terminal_wdl([
            {"label": "a", "by_seat": {"p0": "win", "p1": "loss"}},
            {"label": "a", "by_seat": {"p0": "draw", "p1": "draw"}},
        ], ["a"])
        assert aggregate["a"]["overall_wdl"] == {"win": 1, "draw": 2, "loss": 1}
    print("PASS population Store identity and stale-generation rejection")
    return 0


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--self-test", action="store_true")
    value.add_argument("--evidence-root", type=Path)
    value.add_argument("--model", action="append", dest="model_specs", default=[])
    value.add_argument("--generation", type=int)
    value.add_argument("--mode", choices=("smoke", "formal"))
    value.add_argument("--base-seed", type=int)
    value.add_argument("--pair-start", type=int, default=0)
    value.add_argument("--pairs", type=int)
    value.add_argument("--workers", type=int, choices=(1, 8), default=8)
    value.add_argument("--task-pairs", type=int, default=32)
    value.add_argument("--task-timeout-seconds", type=int, default=1800)
    value.add_argument("--scorer-exe", type=Path)
    value.add_argument("--mage-repo", type=Path)
    value.add_argument("--source-database", type=Path)
    value.add_argument("--maven", type=Path)
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.self_test:
        return self_test()
    required = (args.evidence_root, args.generation, args.mode, args.base_seed,
                args.pairs, args.scorer_exe, args.mage_repo, args.source_database, args.maven)
    if any(value is None for value in required) or args.base_seed < 0 or args.pair_start < 0:
        fail("missing or invalid panel arguments")
    parsed: list[tuple[str, Path]] = []
    for spec in args.model_specs:
        if spec.count("=") != 1:
            fail("model must be label=STORE_ROOT")
        label, raw_root = spec.split("=", 1)
        if not label or not label.replace("-", "").replace("_", "").isalnum():
            fail("model label is invalid")
        parsed.append((label, Path(raw_root)))
    if len(parsed) != 3 or len({label for label, _ in parsed}) != 3:
        fail("exactly three uniquely labelled models are required")
    if len({root.resolve() for _, root in parsed}) != 3:
        fail("three distinct population Store roots are required")
    args.models = parsed
    run(args)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"panel runner failed: {error}", file=sys.stderr)
        raise SystemExit(2)
