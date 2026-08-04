#!/usr/bin/env python3
"""Persistent CPU inference worker for the recurrent CP7 deployment candidate."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
from typing import Any

import numpy as np
import torch


REQUEST_SCHEMA = "mtg-kernel-recurrent-cp7-inference-request/v1"
RESPONSE_SCHEMA = "mtg-kernel-recurrent-cp7-inference-response/v1"
READY_SCHEMA = "mtg-kernel-recurrent-cp7-inference-ready/v1"
PACKAGE_SCHEMA = "mtg-kernel-recurrent-cp7-deployment/v1"
STATE_DIM = 219
OBJECT_DIM = 98
EDGE_DIM = 41
ACTION_DIM = 195
REF_DIM = 25
HISTORY_DIM = 237
BISECTION_STEPS = 16


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _state_sha256(state: dict[str, torch.Tensor]) -> str:
    digest = hashlib.sha256()
    for name, tensor in sorted(state.items()):
        value = tensor.detach().cpu().contiguous()
        digest.update(name.encode("utf-8") + b"\0")
        digest.update(str(value.dtype).encode("ascii") + b"\0")
        digest.update(str(tuple(value.shape)).encode("ascii") + b"\0")
        digest.update(value.numpy().tobytes())
    return digest.hexdigest()


def _f32(bits: list[int], width: int | None = None) -> torch.Tensor:
    array = np.asarray(bits, dtype="<u4").view("<f4").copy()
    if width is not None:
        if width <= 0 or array.size % width:
            _fail("flat tensor shape mismatch")
        array = array.reshape(-1, width)
    value = torch.from_numpy(array)
    if not bool(torch.isfinite(value).all()):
        _fail("non-finite tensor input")
    return value


def _bits(values: torch.Tensor) -> list[int]:
    array = values.detach().cpu().contiguous().numpy().astype("<f4", copy=False)
    return array.view("<u4").reshape(-1).astype(np.uint32).tolist()


def _project(
    parent_logits: torch.Tensor,
    raw_logits: torch.Tensor,
    action_mask: torch.Tensor,
    substep_count: torch.Tensor,
    budget: float,
) -> tuple[torch.Tensor, torch.Tensor]:
    parent_logp = torch.log_softmax(parent_logits, dim=1)
    delta = raw_logits - parent_logits
    low = torch.zeros((raw_logits.shape[0], 1), dtype=raw_logits.dtype)
    high = torch.ones_like(low)
    per_substep = (
        budget / substep_count.to(raw_logits.dtype).clamp_min(1.0)
    ).unsqueeze(1)
    for _ in range(BISECTION_STEPS):
        middle = (low + high) * 0.5
        candidate = parent_logits + middle * delta
        candidate_logp = torch.log_softmax(candidate, dim=1)
        maximum = (
            (candidate_logp - parent_logp)
            .abs()
            .masked_fill(~action_mask, 0.0)
            .max(dim=1, keepdim=True)
            .values
        )
        within = maximum <= per_substep
        low = torch.where(within, middle, low)
        high = torch.where(within, high, middle)
    return parent_logits + low * delta, low.squeeze(1)


class Worker:
    def __init__(self, package_root: Path) -> None:
        manifest_path = package_root / "recurrent_cp7_deployment.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if manifest.get("schema") != PACKAGE_SCHEMA:
            _fail("package schema mismatch")
        model_path = package_root / "model.pt"
        definition_path = package_root / "model_v1.py"
        worker_path = package_root / "worker_v1.py"
        expected = manifest["files"]
        for name, path in (
            ("model", model_path),
            ("model_definition", definition_path),
            ("worker", worker_path),
        ):
            if _sha256(path) != expected[name]["sha256"]:
                _fail(f"{name} SHA-256 mismatch")
        sys.path.insert(0, str(package_root))
        from model_v1 import RecurrentStructuredActorCritic, pack_rows

        payload = torch.load(model_path, map_location="cpu", weights_only=False)
        state = payload.get("model_state_dict")
        if not isinstance(state, dict):
            _fail("model state missing")
        if _state_sha256(state) != manifest["model_state_sha256"]:
            _fail("model state SHA-256 mismatch")
        torch.set_num_threads(1)
        torch.set_num_interop_threads(1)
        self.model = RecurrentStructuredActorCritic(128)
        self.model.load_state_dict(state, strict=True)
        self.model.eval()
        self.pack_rows = pack_rows
        self.budget = float(manifest["log_ratio_budget"])
        self.deployment_scale = float(manifest["deployment_scale"])
        if struct.pack("<d", self.budget) != struct.pack("<d", 0.49):
            _fail("log-ratio budget mismatch")
        if struct.pack("<d", self.deployment_scale) != struct.pack("<d", 0.97):
            _fail("deployment scale mismatch")
        self.model_file_sha256 = expected["model"]["sha256"]
        self.model_state_sha256 = manifest["model_state_sha256"]

    def score(self, request: dict[str, Any]) -> dict[str, Any]:
        if set(request) != {
            "schema",
            "sequence",
            "acting_player",
            "substep_count",
            "tensor",
            "history_f32_bits",
            "parent_logits_f32_bits",
            "parent_value_f32_bits",
        } or request["schema"] != REQUEST_SCHEMA:
            _fail("request schema mismatch")
        if request["acting_player"] not in (0, 1):
            _fail("acting player mismatch")
        tensor = request["tensor"]
        row = {
            "state": _f32(tensor["state_f32_bits"]),
            "object_features": _f32(
                tensor["object_features_f32_bits"], OBJECT_DIM
            ),
            "object_card_ids": torch.tensor(
                tensor["object_card_ids"], dtype=torch.long
            ),
            "object_groups": torch.tensor(
                tensor["object_groups"], dtype=torch.long
            ),
            "edge_features": _f32(tensor["edge_features_f32_bits"], EDGE_DIM),
            "edge_src": torch.tensor(
                tensor["edge_source_indices"], dtype=torch.long
            ),
            "edge_tgt": torch.tensor(
                tensor["edge_target_indices"], dtype=torch.long
            ),
            "action_features": _f32(
                tensor["action_features_f32_bits"], ACTION_DIM
            ),
            "action_ref_features": _f32(
                tensor["action_ref_features_f32_bits"], REF_DIM
            ),
            "ref_action_indices": torch.tensor(
                tensor["action_ref_action_indices"], dtype=torch.long
            ),
            "ref_node_indices": torch.tensor(
                tensor["action_ref_node_indices"], dtype=torch.long
            ),
            "old_logits": _f32(request["parent_logits_f32_bits"]),
            "old_value": _f32([request["parent_value_f32_bits"]]).reshape(()),
            "selected_index": 0,
            "substep_count": int(request["substep_count"]),
            "history_features": _f32(
                request["history_f32_bits"], HISTORY_DIM
            ),
        }
        if row["state"].numel() != STATE_DIM:
            _fail("state width mismatch")
        action_count = int(row["action_features"].shape[0])
        if action_count < 1 or row["old_logits"].numel() != action_count:
            _fail("action count mismatch")
        packed = self.pack_rows([row], torch.device("cpu"))
        with torch.inference_mode():
            residual, _ = self.model(packed)
            raw = torch.where(
                packed.action_mask,
                packed.parent_logits + residual,
                packed.parent_logits,
            )
            projected, scale = _project(
                packed.parent_logits,
                raw,
                packed.action_mask,
                packed.substep_count,
                self.budget,
            )
            logits = packed.parent_logits + self.deployment_scale * (
                projected - packed.parent_logits
            )
        candidate_logp = torch.log_softmax(logits, dim=1)
        parent_logp = torch.log_softmax(packed.parent_logits, dim=1)
        maximum = float((candidate_logp - parent_logp).abs().max())
        if not bool(torch.isfinite(logits).all()) or maximum > self.budget + 1.0e-5:
            _fail("candidate output violates deployment envelope")
        return {
            "schema": RESPONSE_SCHEMA,
            "sequence": request["sequence"],
            "logits_f32_bits": _bits(logits[0, :action_count]),
            "projection_scale": float(scale[0]) * self.deployment_scale,
            "maximum_absolute_log_ratio": maximum,
        }


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-root", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = arguments()
    worker = Worker(args.package_root.resolve())
    print(
        json.dumps(
            {
                "schema": READY_SCHEMA,
                "model_file_sha256": worker.model_file_sha256,
                "model_state_sha256": worker.model_state_sha256,
                "torch": torch.__version__,
                "device": "cpu",
            },
            separators=(",", ":"),
            sort_keys=True,
        ),
        flush=True,
    )
    expected_sequence = 0
    for line in sys.stdin:
        request = json.loads(line)
        if request.get("sequence") != expected_sequence:
            _fail("request sequence mismatch")
        response = worker.score(request)
        print(
            json.dumps(response, separators=(",", ":"), sort_keys=True),
            flush=True,
        )
        expected_sequence += 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print(f"recurrent_cp7_worker: ERROR: {error}", file=sys.stderr, flush=True)
        sys.exit(1)
