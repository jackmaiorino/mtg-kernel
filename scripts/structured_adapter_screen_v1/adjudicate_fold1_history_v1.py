#!/usr/bin/env python3
"""Adjudicate the fixed fold-1 complete-history numerical reproduction."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


SCHEMA = "mtg-kernel-scaled-history-fold1-adjudication/v1"
ORIGINAL_SHA256 = "23091a4ed73ecf7902917de04dc260c5097c7fffc09a0429a8b04ca457a2075e"
CACHE_SHA256 = "721aeeb8389464676edf1190b4e90d74ced286104cc0fb30deb46d36ffbc8090"


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} is not a JSON object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--original", type=Path, required=True)
    parser.add_argument("--reproduction", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    original_sha256 = _sha256(args.original)
    reproduction_sha256 = _sha256(args.reproduction)
    cache_sha256 = _sha256(args.cache)
    if original_sha256 != ORIGINAL_SHA256:
        raise ValueError("original fold-1 SHA-256 mismatch")
    if cache_sha256 != CACHE_SHA256:
        raise ValueError("complete-history cache SHA-256 mismatch")
    original = _load(args.original)
    reproduction = _load(args.reproduction)
    diagnostics = reproduction.get("diagnostics", {})
    float64 = diagnostics.get("float64", {})
    model_state = reproduction.get("model_state")
    state_path = (
        Path(model_state["path"])
        if isinstance(model_state, dict) and isinstance(model_state.get("path"), str)
        else None
    )
    state_sha256 = (
        _sha256(state_path)
        if state_path is not None and state_path.is_file()
        else None
    )
    checks = {
        "fold_is_one": reproduction.get("fold") == 1,
        "training_history_exact": reproduction.get("train_metrics")
        == original.get("train_metrics"),
        "full_heldout_metrics_exact": reproduction.get("heldout")
        == original.get("heldout"),
        "float64_permutation_max_delta_le_1e_10": isinstance(
            float64.get("permutation_max_delta"), (int, float)
        )
        and float64["permutation_max_delta"] <= 1.0e-10,
        "float32_permutation_argmax_changes_zero": diagnostics.get(
            "permutation_argmax_changes"
        )
        == 0,
        "reference_removal_affected_rate_ge_0_20": isinstance(
            diagnostics.get("ref_removal_affected_rate"), (int, float)
        )
        and diagnostics["ref_removal_affected_rate"] >= 0.20,
        "saved_state_exists_and_hashes": state_sha256 is not None
        and state_sha256 == model_state.get("sha256"),
    }
    result = {
        "schema": SCHEMA,
        "advance": all(checks.values()),
        "checks": checks,
        "inputs": {
            "original": {"path": str(args.original), "sha256": original_sha256},
            "reproduction": {
                "path": str(args.reproduction),
                "sha256": reproduction_sha256,
            },
            "cache": {"path": str(args.cache), "sha256": cache_sha256},
            "model_state": {
                "path": str(state_path) if state_path is not None else None,
                "sha256": state_sha256,
            },
        },
        "diagnostics": diagnostics,
        "non_claims": [
            "the original scaled screen remains formally failed",
            "this adjudication provides no live strength evidence",
            "this adjudication provides no promotion or pro-level claim",
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, sort_keys=True, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0 if result["advance"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=__import__("sys").stderr)
        raise SystemExit(2)
