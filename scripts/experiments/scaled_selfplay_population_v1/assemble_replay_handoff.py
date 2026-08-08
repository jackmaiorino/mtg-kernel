#!/usr/bin/env python3
"""Assemble the terminal-blind three-lineage replay handoff manifest."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path

from validate_replay_handoff import (
    EXPECTED_LINEAGES,
    INPUT_SCHEMA,
    PROGRAM_COMMIT,
    PROGRAM_DOCUMENT_SHA256,
    RETEST_DISPOSITION,
    RETEST_FORMAL_MANIFEST_PATH,
    RETEST_FORMAL_MANIFEST_SHA256,
    SEEDS,
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def store_tree_sha256(root: Path) -> str:
    digest = hashlib.sha256()
    files = sorted((path for path in root.rglob("*") if path.is_file()), key=lambda path: str(path).lower())
    for path in files:
        relative = path.relative_to(root).as_posix()
        frame = f"{relative}\n{path.stat().st_size}\n{sha256_file(path)}\n".encode()
        digest.update(frame)
    return digest.hexdigest()


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def successor_record(seed: int, root: Path) -> dict:
    run_path = root / "run.json"
    latest_path = root / "latest.json"
    prefix = root / "checkpoints" / "update-00000512"
    checkpoint_path = Path(f"{prefix}.checkpoint.json")
    sidecar_path = Path(f"{prefix}.sidecar.json")
    state_path = Path(f"{prefix}.state.f32le")
    for path in (run_path, latest_path, checkpoint_path, sidecar_path, state_path):
        if not path.is_file():
            raise ValueError(f"missing replay Store artifact: {path}")

    run = load_json(run_path)
    latest = load_json(latest_path)
    checkpoint = load_json(checkpoint_path)
    if latest.get("generation_index") != 512 or checkpoint.get("generation_index") != 512:
        raise ValueError(f"seed {seed} did not end at generation 512")
    if run.get("schedule", {}).get("base_seed") != seed:
        raise ValueError(f"seed {seed} Run base seed mismatch")
    program = run.get("contracts", {}).get("population_program_v1", {})
    if program.get("expected_base_seed") != seed:
        raise ValueError(f"seed {seed} population authority mismatch")
    lineages = {item.get("base_seed"): item for item in program.get("source_lineages", [])}
    source = EXPECTED_LINEAGES[seed]
    bound = lineages.get(seed, {})
    expected_bound = {
        "store_tree_sha256": source["store_tree_sha256"],
        "run_sha256": source["run_sha256"],
        "checkpoint_sha256": source["checkpoint_sha256"],
        "sidecar_sha256": source["sidecar_sha256"],
        "state_sha256": source["native_state_sha256"],
        "model_parameter_sha256": source["model_parameter_sha256"],
    }
    for key, expected in expected_bound.items():
        if bound.get(key) != expected:
            raise ValueError(f"seed {seed} Run source binding mismatch for {key}")

    progress = checkpoint.get("progress", {})
    safe_progress = {
        "completed_episode_count": progress.get("completed_episode_count"),
        "next_episode_index": progress.get("next_episode_index"),
        "successful_update_count": progress.get("successful_update_count"),
    }
    return {
        "store_root": str(root.resolve()),
        "store_tree_sha256": store_tree_sha256(root),
        "run_sha256": sha256_file(run_path),
        "checkpoint_sha256": sha256_file(checkpoint_path),
        "sidecar_sha256": sha256_file(sidecar_path),
        "native_state_sha256": sha256_file(state_path),
        "model_parameter_sha256": checkpoint.get("train_state", {}).get("model_parameter_sha256"),
        "bound_source_store_tree_sha256": bound.get("store_tree_sha256"),
        "bound_source_run_sha256": bound.get("run_sha256"),
        "bound_retest_manifest_sha256": program.get("retest_manifest_sha256"),
        "adam_step": checkpoint.get("train_state", {}).get("adam_step"),
        "generation": checkpoint.get("generation_index"),
        "progress": safe_progress,
    }


def parse_lineage(value: str) -> tuple[int, Path]:
    raw_seed, separator, raw_path = value.partition("=")
    if not separator:
        raise argparse.ArgumentTypeError("lineage must be SEED=STORE_ROOT")
    return int(raw_seed), Path(raw_path)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--lineage", action="append", required=True, type=parse_lineage)
    args = parser.parse_args()
    roots = dict(args.lineage)
    if tuple(sorted(roots)) != SEEDS:
        raise ValueError(f"lineage seeds must be exactly {SEEDS}")
    document = {
        "schema": INPUT_SCHEMA,
        "global_target_generation": 1536,
        "replay_end_generation": 512,
        "program_updates": 1024,
        "terminal_outcomes_read": False,
        "authorities": {
            "program_commit": PROGRAM_COMMIT,
            "program_document_sha256": PROGRAM_DOCUMENT_SHA256,
            "retest_formal_manifest_path": RETEST_FORMAL_MANIFEST_PATH,
            "retest_formal_manifest_sha256": RETEST_FORMAL_MANIFEST_SHA256,
            "retest_disposition": RETEST_DISPOSITION,
        },
        "lineages": [
            {
                "seed": seed,
                "source": copy.deepcopy(EXPECTED_LINEAGES[seed]),
                "successor": successor_record(seed, roots[seed]),
            }
            for seed in SEEDS
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")),
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
