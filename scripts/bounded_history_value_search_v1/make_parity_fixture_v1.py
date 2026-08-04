#!/usr/bin/env python3
"""Build a five-bucket Python parity fixture for the bounded value package."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
STRUCTURED_DIR = SCRIPT_DIR.parent / "structured_adapter_screen_v1"
if str(STRUCTURED_DIR) not in sys.path:
    sys.path.insert(0, str(STRUCTURED_DIR))

import run_screen as screen  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


SCHEMA = "mtg-kernel-structured-history-residual-parity-fixture/v1"
SOURCE_SCHEMA = "mtg-kernel-structured-policy-successor-parity-fixture/v1"
SOURCE_PARITY_SHA256 = (
    "f603d97e72fdaae0e6a483eb331c0ead94e999231220614b21d157818606fdb3"
)
MODEL_STATE_SHA256 = (
    "cae8e19ef825325508de351b883b2df3863dc66f0288be06ad2ccf868e3d7d7c"
)
BUCKETS = (0, 1, 4, 8, 16)


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _read_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        _fail("source parity root is not an object")
    return value


def _tensor(value: Any, *, integer: bool = False) -> torch.Tensor:
    return torch.tensor(value, dtype=torch.int64 if integer else torch.float32)


def _model_row(example: dict[str, Any]) -> dict[str, Any]:
    acting_player = int(example["acting_player"])
    tensor = example["tensor"]
    history_rows = []
    for entry in example["history"]:
        entry_actor = int(entry["acting_player"])
        if entry_actor not in (0, 1):
            _fail("history actor is outside the fixed two-seat contract")
        role = [1.0, 0.0] if entry_actor == acting_player else [0.0, 1.0]
        history_rows.append(
            entry["action_explicit_features"]
            + role
            + entry["public_card_histogram"]
        )
    return {
        "state": _tensor(tensor["state"]),
        "object_features": _tensor(tensor["object_features"]),
        "object_card_ids": _tensor(tensor["object_card_ids"], integer=True),
        "object_groups": _tensor(tensor["object_groups"], integer=True),
        "edge_features": _tensor(tensor["edge_features"]),
        "edge_src": _tensor(tensor["edge_source_indices"], integer=True),
        "edge_tgt": _tensor(tensor["edge_target_indices"], integer=True),
        "action_features": _tensor(tensor["action_features"]),
        "action_ref_features": _tensor(tensor["action_ref_features"]),
        "ref_card_ids": _tensor(tensor["action_ref_card_ids"], integer=True),
        "ref_action_indices": _tensor(
            tensor["action_ref_action_indices"], integer=True
        ),
        "ref_node_indices": _tensor(tensor["action_ref_node_indices"], integer=True),
        "history_features": torch.tensor(history_rows, dtype=torch.float32).reshape(
            -1, distill.HISTORY_FEATURE_DIM
        ),
        "candidate_seat": int(example["candidate_seat"]),
    }


def build(args: argparse.Namespace) -> dict[str, Any]:
    if args.output.exists() or args.output.is_symlink():
        _fail(f"refusing to overwrite {args.output}")
    if _sha256(args.source_parity) != SOURCE_PARITY_SHA256:
        _fail("qualified-policy source parity SHA-256 mismatch")
    if _sha256(args.model_state) != MODEL_STATE_SHA256:
        _fail("bounded-value model-state SHA-256 mismatch")
    source = _read_json(args.source_parity)
    if source.get("schema") != SOURCE_SCHEMA:
        _fail("qualified-policy source parity schema mismatch")
    selected = {
        int(example["history_length_bucket"]): example
        for example in source.get("examples", [])
        if int(example.get("acting_player", -1)) == 0
    }
    if tuple(sorted(selected)) != BUCKETS:
        _fail("source parity does not contain the exact current-actor-zero buckets")

    payload = torch.load(args.model_state, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict") if isinstance(payload, dict) else None
    if not isinstance(state, dict):
        _fail("bounded-value model state lacks model_state_dict")
    model = distill._model()
    model.load_state_dict(state, strict=True)
    model.eval()

    examples = []
    with torch.no_grad():
        for bucket in BUCKETS:
            source_example = selected[bucket]
            logits, raw_value = model._one(_model_row(source_example))  # noqa: SLF001
            if not bool(torch.isfinite(logits).all()) or not bool(
                torch.isfinite(raw_value)
            ):
                _fail("bounded-value Python parity output is non-finite")
            examples.append(
                {
                    "lane": "value",
                    "history_length_bucket": bucket,
                    "acting_player": 0,
                    "tensor": source_example["tensor"],
                    "history": source_example["history"],
                    "expected_residual_logits": logits.tolist(),
                    "expected_residual_value": float(raw_value),
                }
            )
    result = {
        "schema": SCHEMA,
        "acting_player_convention": "fixture-current-actor-is-zero/v1",
        "examples": examples,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(result, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    return {
        "output": str(args.output),
        "sha256": _sha256(args.output),
        "examples": len(examples),
        "history_length_buckets": list(BUCKETS),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-parity", type=Path, required=True)
    parser.add_argument("--model-state", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    print(json.dumps(build(parser.parse_args()), sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, TypeError, ValueError) as error:
        print(f"make_parity_fixture_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
