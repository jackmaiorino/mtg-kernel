#!/usr/bin/env python3
"""Publish the selected bounded-logit terminal PPO package."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
from typing import Any

import torch

import screen_v1 as screen


block = screen.block
pipeline = screen.pipeline
base = block.base
CANDIDATE_SCHEMA = "mtg-kernel-structured-policy-successor-candidate/v7"
REPORT_SCHEMA = "mtg-kernel-structured-policy-bounded-logit-residual-report/v1"
PARITY_SCHEMA = "mtg-kernel-structured-policy-bounded-logit-residual-parity-fixture/v1"
ARCHITECTURE = (
    "complete-public-history-structured-policy-bounded-logit-residual-"
    "frozen-parent-value/v1"
)
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-policy-bounded-logit-residual-composite-model/v1"
)


def _fail(message: str) -> None:
    raise ValueError(message)


def _fixture_example(example: dict[str, Any]) -> dict[str, Any]:
    tensor = example["tensor"]
    acting_player = int(example["acting_player"])
    history = []
    for entry in example["history"]:
        self_role = float(int(entry["acting_player"]) == acting_player)
        history.append(
            entry["action_explicit_features"]
            + [self_role, 1.0 - self_role]
            + entry["public_card_histogram"]
        )
    return {
        "state": torch.tensor(tensor["state"], dtype=torch.float32),
        "object_features": torch.tensor(tensor["object_features"], dtype=torch.float32),
        "object_card_ids": torch.tensor(tensor["object_card_ids"], dtype=torch.int64),
        "object_groups": torch.tensor(tensor["object_groups"], dtype=torch.int64),
        "edge_features": torch.tensor(tensor["edge_features"], dtype=torch.float32).reshape(
            -1, pipeline.screen.EDGE_DIM
        ),
        "edge_src": torch.tensor(tensor["edge_source_indices"], dtype=torch.int64),
        "edge_tgt": torch.tensor(tensor["edge_target_indices"], dtype=torch.int64),
        "action_features": torch.tensor(tensor["action_features"], dtype=torch.float32),
        "action_ref_features": torch.tensor(
            tensor["action_ref_features"], dtype=torch.float32
        ).reshape(-1, pipeline.screen.REF_DIM),
        "ref_card_ids": torch.tensor(tensor["action_ref_card_ids"], dtype=torch.int64),
        "ref_action_indices": torch.tensor(
            tensor["action_ref_action_indices"], dtype=torch.int64
        ),
        "ref_node_indices": torch.tensor(
            tensor["action_ref_node_indices"], dtype=torch.int64
        ),
        "candidate_seat": int(example["candidate_seat"]),
        "history_features": torch.tensor(history, dtype=torch.float32).reshape(
            -1, pipeline.distill.HISTORY_FEATURE_DIM
        ),
    }


def _parity_fixture(
    initial_model: Any,
    trained_model: Any,
    clip: float,
    source_path: Path,
) -> dict[str, Any]:
    source = json.loads(source_path.read_text(encoding="utf-8"))
    if len(source.get("examples", [])) != 10:
        _fail("source parity fixture does not contain ten examples")
    examples = []
    initial_model.eval()
    trained_model.eval()
    with torch.no_grad():
        for source_example in source["examples"]:
            row = _fixture_example(source_example)
            initial_logits, initial_value = initial_model._one(row)
            trained_logits, trained_value = trained_model._one(row)
            if initial_value.detach().float().numpy().tobytes() != trained_value.detach().float().numpy().tobytes():
                _fail("parity source states do not preserve the frozen value")
            logits = screen.bounded_logits(initial_logits, trained_logits, clip).float()
            example = dict(source_example)
            example["expected_structured_logits"] = logits.tolist()
            example["expected_value_residual_f32_bits"] = "00000000"
            examples.append(example)
    return {
        "schema": PARITY_SCHEMA,
        "output_semantics": "bounded-structured-logits-and-exact-parent-value/v1",
        "examples": examples,
    }


def package(args: argparse.Namespace) -> dict[str, Any]:
    if args.output_root.exists() or args.parity_output.exists():
        _fail("bounded-logit package output already exists")
    screen_sha256 = pipeline._sha256(args.screen_report)
    report_screen = json.loads(args.screen_report.read_text(encoding="utf-8"))
    if (
        report_screen.get("schema") != screen.SCHEMA
        or report_screen.get("method") != screen.METHOD
        or report_screen.get("mechanism_gate", {}).get("decision") != "PASS"
        or report_screen.get("source", {}).get("pair_count") != 2_048
        or report_screen.get("source", {}).get("base_seed") != 1_660_001
        or report_screen.get("source", {}).get("cache_sha256")
        != base.SOURCE_CACHE_SHA256
    ):
        _fail("formal bounded-logit screen is not qualified")
    clip = float(report_screen["selected_clip"])
    selected = next(
        (row for row in report_screen["rows"] if float(row["clip"]) == clip), None
    )
    if selected is None or not selected["safety_gate"]["pass"]:
        _fail("selected bounded-logit row is absent or unsafe")
    initial_state, trained_state, identity = block._base_states()
    initial_model = pipeline.distill._model()
    trained_model = pipeline.distill._model()
    initial_model.load_state_dict(initial_state["model_state_dict"], strict=True)
    trained_model.load_state_dict(trained_state["model_state_dict"], strict=True)
    initial_payload, initial_parameters = pipeline.initializer._encoded_weights(initial_model)
    trained_payload, trained_parameters = pipeline.initializer._encoded_weights(trained_model)
    if len(initial_payload) != len(trained_payload):
        _fail("source model payload lengths differ")
    parameters = []
    offset = 0
    for prefix, source_parameters in (
        ("initializer", initial_parameters),
        ("trained", trained_parameters),
    ):
        for parameter in source_parameters:
            item = dict(parameter)
            item["name"] = f"{prefix}.{parameter['name']}"
            item["offset_f32"] = offset
            offset += int(item["count_f32"])
            parameters.append(item)
    payload = initial_payload + trained_payload
    if offset * 4 != len(payload):
        _fail("combined parameter layout mismatch")
    args.output_root.mkdir(parents=True)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    parent_manifest = base.PARENT_OUTCOME_ROOT / "checkpoint.json"
    parent_payload = base.PARENT_OUTCOME_ROOT / "checkpoint.state.f32le"
    if (
        pipeline._sha256(parent_manifest) != pipeline.live.PARENT_MANIFEST_SHA256
        or pipeline._sha256(parent_payload) != pipeline.live.PARENT_PAYLOAD_SHA256
    ):
        _fail("bounded-logit retained parent mismatch")
    shutil.copyfile(parent_manifest, parent_output / parent_manifest.name)
    shutil.copyfile(parent_payload, parent_output / parent_payload.name)
    weights_path = args.output_root / "weights.f32le"
    weights_path.write_bytes(payload)
    weights_sha256 = pipeline._sha256(weights_path)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(pipeline.live.PARENT_MODEL_PARAMETER_SHA256)
        + payload
    ).hexdigest()
    source = report_screen["source"]
    report = {
        "schema": REPORT_SCHEMA,
        "initializer": {
            "candidate_json_sha256": identity["candidate_json_sha256"],
            "report_sha256": identity["report_sha256"],
            "weights_sha256": identity["weights_sha256"],
            "composite_model_parameter_sha256": identity[
                "composite_model_parameter_sha256"
            ],
            "model_state_sha256": base.INITIALIZER_STATE_SHA256,
        },
        "source": {
            "cache_sha256": source["cache_sha256"],
            "pair_count": source["pair_count"],
            "base_seed": source["base_seed"],
            "pool_json_sha256": source["pool_json_sha256"],
            "source_commit": args.source_commit,
            "rejected_fit_report_sha256": block.SOURCE_FIT_REPORT_SHA256,
            "rejected_model_state_sha256": block.TRAINED_STATE_SHA256,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": pipeline.VALUE_MODEL,
            "projection_method": screen.METHOD,
            "logit_residual_clip": clip,
            "screen_report_sha256": screen_sha256,
        },
        "movement": selected["metrics"],
        "transport": {
            "maximum_absolute_logit_error": 1.0,
            "parent_value_bit_exact": False,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
    }
    report_path = args.output_root / "report.json"
    report_path.write_bytes(pipeline.history_publish._json_bytes(report))
    report_sha256 = pipeline._sha256(report_path)
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": "json-pretty-sorted-utf8-trailing-lf/v1",
        "parent": {
            "directory": "parent",
            "manifest_sha256": pipeline.live.PARENT_MANIFEST_SHA256,
            "payload_sha256": pipeline.live.PARENT_PAYLOAD_SHA256,
            "native_state_sha256": pipeline.live.PARENT_NATIVE_STATE_SHA256,
            "model_parameter_sha256": pipeline.live.PARENT_MODEL_PARAMETER_SHA256,
            "adam_step": pipeline.live.PARENT_ADAM_STEP,
        },
        "architecture": {
            "identity": ARCHITECTURE,
            "state_dim": pipeline.screen.STATE_DIM,
            "object_dim": pipeline.screen.OBJECT_DIM,
            "edge_dim": pipeline.screen.EDGE_DIM,
            "action_dim": pipeline.screen.ACTION_DIM,
            "ref_dim": pipeline.screen.REF_DIM,
            "hidden_dim": pipeline.distill.DIM,
            "card_vocab": pipeline.distill.CARD_VOCAB,
            "card_embedding_dim": max(8, pipeline.distill.DIM // 2),
            "group_vocab": pipeline.distill.GROUP_VOCAB,
            "group_embedding_dim": max(8, pipeline.distill.DIM // 3),
            "history_length": pipeline.distill.HISTORY_LENGTH,
            "history_feature_dim": pipeline.distill.HISTORY_FEATURE_DIM,
            "history_role_dim": 2,
            "value_model": pipeline.VALUE_MODEL,
        },
        "weights": {
            "filename": weights_path.name,
            "encoding": "ordered-row-major-finite-f32-little-endian/v1",
            "sha256": weights_sha256,
            "byte_count": len(payload),
            "parameter_count": offset,
            "parameters": parameters,
        },
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / pipeline.CANDIDATE_FILENAME
    candidate_path.write_bytes(pipeline.history_publish._json_bytes(candidate))
    parity = _parity_fixture(initial_model, trained_model, clip, args.source_parity)
    pipeline._write_new_json(args.parity_output, parity)
    return {
        "candidate_root": str(args.output_root),
        "selected_clip": clip,
        "candidate_json_sha256": pipeline._sha256(candidate_path),
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "parity_fixture": str(args.parity_output),
        "parity_fixture_sha256": pipeline._sha256(args.parity_output),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--screen-report", type=Path, required=True)
    parser.add_argument("--source-parity", type=Path, required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--parity-output", type=Path, required=True)
    result = package(parser.parse_args())
    print(json.dumps(result, sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
