#!/usr/bin/env python3
"""Build the canonical t=0 population refresh with historical fallbacks."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path


PROGRAM_COMMIT = "838920e359c7a1152d97c450f4575c6be2309f22"
PROGRAM_DOCUMENT_SHA256 = "b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38"
RETEST_MANIFEST_SHA256 = "f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8"
ROLES = (
    "anchor-0",
    "anchor-1",
    "historical-0",
    "historical-1",
    "current-0",
    "current-1",
    "exploiter-0",
    "exploiter-1",
)
EXPECTED_ANCHORS = (
    {
        "seed": 920012,
        "generation": 384,
        "run": "2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae",
        "checkpoint": "4bd38cf3a9af3fb03fb04428fbc4286d4635007e848c7b9f0740122e430cbba8",
        "sidecar": "7511c0377edd4e8d918fa5843f89a0270a8264e5466c329f6b4ef18bbf9e76bb",
        "state": "a6c87366b2da9fc33923abab3c0e22d70c884cd9420477df3a475117be6beb99",
        "model": "db58dbe3f1f76b5bdf3bae4de657711dc818393b2bf1eeae88c02d8866b4d01d",
    },
    {
        "seed": 920005,
        "generation": 512,
        "run": "8bc06b6cf2e26df8002b5cece2784e0cd165cdd6bbd199a835e06c17e8d5de5c",
        "checkpoint": "03f0e226f884f51bf7128f70bec189bd6ac2c8f231ced8886f2cb7d3e936cc90",
        "sidecar": "c56a8ba1361ab172c669307084c4522ee06ac79e39b7cf4a306f11effe36b031",
        "state": "2904dd7b899c21234c64925440277dbfa8d6f552d8f620b153bc8d16c44f523a",
        "model": "0635d2defb8facd700ede34789434956fc4a2fd3b5058cc2df5dd820398b4c22",
    },
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8-sig") as stream:
        return json.load(stream)


def slot(root: Path, seed: int, generation: int, role: str, occupant: str) -> dict:
    prefix = root / "checkpoints" / f"update-{generation:08d}"
    run_path = root / "run.json"
    checkpoint_path = Path(f"{prefix}.checkpoint.json")
    sidecar_path = Path(f"{prefix}.sidecar.json")
    state_path = Path(f"{prefix}.state.f32le")
    for path in (run_path, checkpoint_path, sidecar_path, state_path):
        if not path.is_file():
            raise ValueError(f"missing slot artifact: {path}")
    run = load_json(run_path)
    checkpoint = load_json(checkpoint_path)
    if run.get("schedule", {}).get("base_seed") != seed:
        raise ValueError(f"base-seed mismatch for {root}")
    if checkpoint.get("generation_index") != generation:
        raise ValueError(f"generation mismatch for {checkpoint_path}")
    return {
        "available_by_global_generation": 512,
        "checkpoint_sha256": sha256_file(checkpoint_path),
        "model_parameter_sha256": checkpoint["train_state"]["model_parameter_sha256"],
        "occupant_class": occupant,
        "role": role,
        "sidecar_sha256": sha256_file(sidecar_path),
        "slot_index": ROLES.index(role),
        "source_base_seed": seed,
        "source_generation": generation,
        "source_run_sha256": sha256_file(run_path),
        "state_sha256": sha256_file(state_path),
        "weight_units": 125000,
    }


def check_anchor(record: dict, expected: dict) -> None:
    mapping = {
        "source_base_seed": "seed",
        "source_generation": "generation",
        "source_run_sha256": "run",
        "checkpoint_sha256": "checkpoint",
        "sidecar_sha256": "sidecar",
        "state_sha256": "state",
        "model_parameter_sha256": "model",
    }
    for actual_key, expected_key in mapping.items():
        if record[actual_key] != expected[expected_key]:
            raise ValueError(f"anchor mismatch for {actual_key}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--anchor-0", required=True, type=Path)
    parser.add_argument("--anchor-1", required=True, type=Path)
    parser.add_argument("--seed-970001", required=True, type=Path)
    parser.add_argument("--seed-970002", required=True, type=Path)
    parser.add_argument("--seed-970003", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--fallback-record", required=True, type=Path)
    args = parser.parse_args()

    slots = [
        slot(args.anchor_0, 920012, 384, "anchor-0", "policy"),
        slot(args.anchor_1, 920005, 512, "anchor-1", "policy"),
        slot(args.seed_970003, 970003, 256, "historical-0", "policy"),
        slot(args.seed_970003, 970003, 128, "historical-1", "policy"),
        slot(args.seed_970001, 970001, 512, "current-0", "policy"),
        slot(args.seed_970002, 970002, 512, "current-1", "policy"),
        slot(args.seed_970003, 970003, 64, "exploiter-0", "historical-fallback"),
        slot(args.seed_970003, 970003, 384, "exploiter-1", "historical-fallback"),
    ]
    check_anchor(slots[0], EXPECTED_ANCHORS[0])
    check_anchor(slots[1], EXPECTED_ANCHORS[1])
    models = {record["model_parameter_sha256"] for record in slots}
    if len(models) != 8:
        raise ValueError("initial population slots must have eight unique model hashes")
    document = {
        "availability_generation": 512,
        "global_generation": 512,
        "program_commit": PROGRAM_COMMIT,
        "program_document_sha256": PROGRAM_DOCUMENT_SHA256,
        "program_update": 0,
        "refresh_index": 0,
        "retest_manifest_sha256": RETEST_MANIFEST_SHA256,
        "schema": "mtg-kernel-scaled-selfplay-population-refresh/v1",
        "slots": slots,
        "weight_total_units": 1000000,
    }
    encoded = json.dumps(document, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(encoded, encoding="utf-8")
    fallback = {
        "schema": "scaled-selfplay-population-fallback-record/v1",
        "program_update": 0,
        "reason": "response-exploiter build lane unavailable",
        "disposition": "use deterministic historical fallbacks; no exploiter-robustness claim",
        "archive_order": [
            {"seed": 970003, "generation": 64},
            {"seed": 970003, "generation": 384},
        ],
        "selection_uses_terminal_outcomes": False,
        "refresh_manifest_sha256": hashlib.sha256(encoded.encode()).hexdigest(),
    }
    args.fallback_record.write_text(
        json.dumps(fallback, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
