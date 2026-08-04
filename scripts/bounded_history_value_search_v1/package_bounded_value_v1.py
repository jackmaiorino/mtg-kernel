#!/usr/bin/env python3
"""Package the confirmed bounded complete-history value model for search."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import shutil
import sys
from pathlib import Path
from typing import Any

import torch


SCRIPT_DIR = Path(__file__).resolve().parent
SCRIPTS_DIR = SCRIPT_DIR.parent
FIT_DIR = SCRIPTS_DIR / "bounded_onpolicy_history_value_v1"
STRUCTURED_DIR = SCRIPTS_DIR / "structured_adapter_screen_v1"
for directory in (FIT_DIR, STRUCTURED_DIR):
    if str(directory) not in sys.path:
        sys.path.insert(0, str(directory))

import fit_and_confirm_v1 as bounded_fit  # noqa: E402
import fit_policy_live_candidate as live  # noqa: E402
import run_screen as screen  # noqa: E402
import run_structured_successor_distillation_v1 as distill  # noqa: E402


CANDIDATE_FILENAME = "structured_bounded_value_candidate.json"
CANDIDATE_SCHEMA = "mtg-kernel-structured-history-bounded-value-residual-candidate/v1"
REPORT_SCHEMA = "mtg-kernel-structured-history-bounded-value-residual-fit/v1"
ARCHITECTURE = (
    "complete-public-history-structured-object-action-attention-policy-value-residual/v1"
)
VALUE_MODEL = "projected-parent-tanh-addition-terminal-value/v1"
COMPOSITE_DOMAIN = (
    b"mtg-kernel-structured-history-bounded-value-residual-composite-model/v1"
)
PUBLICATION_ENCODING = "json-pretty-sorted-utf8-trailing-lf/v1"
WEIGHTS_ENCODING = "ordered-row-major-finite-f32-little-endian/v1"
EXPECTED_PARAMETER_COUNT = 107_378
PROJECTION_EPSILON = 0.001
PARENT_IDENTITY = {
    "manifest_sha256": live.PARENT_MANIFEST_SHA256,
    "payload_sha256": live.PARENT_PAYLOAD_SHA256,
    "native_state_sha256": live.PARENT_NATIVE_STATE_SHA256,
    "model_parameter_sha256": live.PARENT_MODEL_PARAMETER_SHA256,
    "adam_step": live.PARENT_ADAM_STEP,
}


def _fail(message: str) -> None:
    raise ValueError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n"
    ).encode("utf-8")


def _reject_json_constant(value: str) -> None:
    _fail(f"JSON contains non-finite constant {value}")


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            _fail(f"JSON contains duplicate key {key!r}")
        result[key] = value
    return result


def _read_json(path: Path) -> dict[str, Any]:
    if not path.is_file() or path.is_symlink():
        _fail(f"JSON input is not a regular file: {path}")
    value = json.loads(
        path.read_text(encoding="utf-8"),
        object_pairs_hook=_reject_duplicate_keys,
        parse_constant=_reject_json_constant,
    )
    if not isinstance(value, dict):
        _fail(f"JSON root must be an object: {path}")
    return value


def _write_new(path: Path, payload: bytes) -> str:
    if path.exists() or path.is_symlink():
        _fail(f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return _sha256(path)


def _fixed_architecture() -> dict[str, Any]:
    expected = {
        "state_dim": 219,
        "object_dim": 98,
        "edge_dim": 41,
        "action_dim": 195,
        "ref_dim": 25,
        "hidden_dim": 48,
        "card_vocab": 136,
        "card_embedding_dim": 24,
        "group_vocab": 12,
        "group_embedding_dim": 16,
        "history_length": 16,
        "history_feature_dim": 237,
        "history_role_dim": 2,
    }
    observed = {
        "state_dim": screen.STATE_DIM,
        "object_dim": screen.OBJECT_DIM,
        "edge_dim": screen.EDGE_DIM,
        "action_dim": screen.ACTION_DIM,
        "ref_dim": screen.REF_DIM,
        "hidden_dim": distill.DIM,
        "card_vocab": distill.CARD_VOCAB,
        "card_embedding_dim": max(8, distill.DIM // 2),
        "group_vocab": distill.GROUP_VOCAB,
        "group_embedding_dim": max(8, distill.DIM // 3),
        "history_length": distill.HISTORY_LENGTH,
        "history_feature_dim": distill.HISTORY_FEATURE_DIM,
        "history_role_dim": 2,
    }
    if observed != expected:
        _fail("complete-history structured architecture constants drifted")
    return {
        "identity": ARCHITECTURE,
        **expected,
        "value_model": VALUE_MODEL,
    }


def _validate_parent_root(root: Path) -> dict[str, Any]:
    if not root.is_dir() or root.is_symlink():
        _fail("parent root is not a regular directory")
    expected_names = {"checkpoint.json", "checkpoint.state.f32le"}
    observed_names = {entry.name for entry in root.iterdir()}
    if observed_names != expected_names:
        _fail("parent root inventory is not the exact retained checkpoint directory")
    manifest = root / "checkpoint.json"
    payload = root / "checkpoint.state.f32le"
    if (
        not manifest.is_file()
        or manifest.is_symlink()
        or not payload.is_file()
        or payload.is_symlink()
    ):
        _fail("parent root checkpoint files are not regular files")
    if _sha256(manifest) != PARENT_IDENTITY["manifest_sha256"]:
        _fail("parent manifest SHA-256 mismatch")
    if _sha256(payload) != PARENT_IDENTITY["payload_sha256"]:
        _fail("parent payload SHA-256 mismatch")
    return {
        "directory": "parent",
        "manifest_sha256": PARENT_IDENTITY["manifest_sha256"],
        "payload_sha256": PARENT_IDENTITY["payload_sha256"],
        "native_state_sha256": PARENT_IDENTITY["native_state_sha256"],
        "model_parameter_sha256": PARENT_IDENTITY["model_parameter_sha256"],
        "adam_step": PARENT_IDENTITY["adam_step"],
    }


def _load_initializer(path: Path) -> tuple[dict[str, torch.Tensor], str]:
    if not path.is_file() or path.is_symlink():
        _fail("initializer state is not a regular file")
    observed_sha256 = _sha256(path)
    if observed_sha256 != bounded_fit.INITIALIZER_STATE_SHA256:
        _fail("initializer state SHA-256 mismatch")
    payload = torch.load(path, map_location="cpu", weights_only=False)
    if not isinstance(payload, dict) or not isinstance(
        payload.get("model_state_dict"), dict
    ):
        _fail("initializer state lacks model_state_dict")
    state = payload["model_state_dict"]
    model = distill._model()
    try:
        model.load_state_dict(state, strict=True)
    except (RuntimeError, TypeError) as error:
        _fail(f"initializer state layout mismatch: {error}")
    for name, tensor in model.state_dict().items():
        if name.startswith("value_head.") and torch.count_nonzero(tensor).item():
            _fail("initializer value residual is not exactly zero")
    return model.state_dict(), observed_sha256


def _validate_fit(
    fit_report_path: Path,
    model_state_path: Path,
    initializer_sha256: str,
) -> tuple[dict[str, torch.Tensor], dict[str, Any], str]:
    report = _read_json(fit_report_path)
    if report.get("schema") != bounded_fit.FIT_SCHEMA or report.get("status") != "complete":
        _fail("fit report schema or status mismatch")
    if report.get("source", {}).get("cache_sha256") != bounded_fit.DEVELOPMENT_CACHE_SHA256:
        _fail("fit report development-cache provenance mismatch")
    if report.get("initializer", {}).get("sha256") != initializer_sha256:
        _fail("fit report initializer binding mismatch")
    if report.get("parameterization") != "tanh-addition-projected-parent-bounded-value/v1":
        _fail("fit report parameterization mismatch")
    if report.get("config", {}).get("parent_projection_epsilon") != PROJECTION_EPSILON:
        _fail("fit report projection epsilon mismatch")
    if report.get("initial_alignment", {}).get("pass") is not True:
        _fail("fit report does not establish parent-preserving initialization")
    if not model_state_path.is_file() or model_state_path.is_symlink():
        _fail("fitted model state is not a regular file")
    state_sha256 = _sha256(model_state_path)
    if report.get("model_state", {}).get("sha256") != state_sha256:
        _fail("fit report model-state SHA-256 binding mismatch")
    payload = torch.load(model_state_path, map_location="cpu", weights_only=False)
    if not isinstance(payload, dict):
        _fail("fitted model state root is not an object")
    if (
        payload.get("schema") != bounded_fit.FIT_SCHEMA + ".state"
        or payload.get("development_cache_sha256") != bounded_fit.DEVELOPMENT_CACHE_SHA256
        or payload.get("initializer_state_sha256") != initializer_sha256
    ):
        _fail("fitted model state provenance mismatch")
    state = payload.get("model_state_dict")
    if not isinstance(state, dict):
        _fail("fitted model state lacks model_state_dict")
    model = distill._model()
    expected_names = list(model.state_dict())
    if list(state) != expected_names:
        _fail("fitted model state_dict order or names mismatch")
    for name, tensor in state.items():
        if not isinstance(tensor, torch.Tensor) or tensor.dtype != torch.float32:
            _fail(f"fitted state tensor is not float32: {name}")
        if not bool(torch.isfinite(tensor).all()):
            _fail(f"fitted state tensor is non-finite: {name}")
    try:
        model.load_state_dict(state, strict=True)
    except (RuntimeError, TypeError) as error:
        _fail(f"fitted model state layout mismatch: {error}")
    return model.state_dict(), report, state_sha256


def _validate_initializer_parent_identity(
    fitted_state: dict[str, torch.Tensor], initializer_state: dict[str, torch.Tensor]
) -> None:
    for name, tensor in initializer_state.items():
        if name.startswith("policy_head.") and not torch.equal(fitted_state[name], tensor):
            _fail("fitted policy head differs from the qualified initializer")


def _validate_confirmation(
    path: Path,
    fit_report_path: Path,
    model_state_sha256: str,
) -> tuple[dict[str, Any], str]:
    report = _read_json(path)
    if (
        report.get("schema") != bounded_fit.CONFIRM_SCHEMA
        or report.get("status") != "pass"
        or report.get("gates", {}).get("bounded_value_confirmation_pass") is not True
        or report.get("model_state", {}).get("sha256") != model_state_sha256
        or report.get("fit", {}).get("sha256") != _sha256(fit_report_path)
    ):
        _fail("bounded value confirmation binding or pass status mismatch")
    return report, _sha256(path)


def _serialize_state(
    state: dict[str, torch.Tensor],
) -> tuple[bytes, list[dict[str, Any]]]:
    payload = bytearray()
    parameters: list[dict[str, Any]] = []
    offset = 0
    for name, state_tensor in state.items():
        tensor = state_tensor.detach().cpu().contiguous()
        if tensor.dtype != torch.float32:
            _fail(f"state_dict tensor is not float32: {name}")
        if not bool(torch.isfinite(tensor).all()):
            _fail(f"state_dict tensor is non-finite: {name}")
        raw = tensor.numpy().astype("<f4", copy=False).tobytes(order="C")
        count = tensor.numel()
        if len(raw) != count * 4:
            _fail(f"state_dict tensor f32 byte count mismatch: {name}")
        parameters.append(
            {
                "name": name,
                "shape": list(tensor.shape),
                "offset_f32": offset,
                "count_f32": count,
            }
        )
        payload.extend(raw)
        offset += count
    if offset != EXPECTED_PARAMETER_COUNT:
        _fail("bounded value model parameter count mismatch")
    if len(payload) != EXPECTED_PARAMETER_COUNT * 4:
        _fail("bounded value model payload byte count mismatch")
    return bytes(payload), parameters


def package(args: argparse.Namespace) -> dict[str, Any]:
    if args.output_root.exists() or args.output_root.is_symlink():
        _fail(f"refusing to overwrite {args.output_root}")
    architecture = _fixed_architecture()
    parent = _validate_parent_root(args.parent_root)
    initializer_state, initializer_sha256 = _load_initializer(args.initializer_state)
    fitted_state, fit_report, model_state_sha256 = _validate_fit(
        args.fit_report, args.model_state, initializer_sha256
    )
    _, confirmation_sha256 = _validate_confirmation(
        args.confirmation, args.fit_report, model_state_sha256
    )
    _validate_initializer_parent_identity(fitted_state, initializer_state)
    weights, parameters = _serialize_state(fitted_state)
    composite_sha256 = hashlib.sha256(
        COMPOSITE_DOMAIN
        + bytes.fromhex(parent["model_parameter_sha256"])
        + weights
    ).hexdigest()

    args.output_root.mkdir(parents=True, exist_ok=False)
    parent_output = args.output_root / "parent"
    parent_output.mkdir()
    for filename in ("checkpoint.json", "checkpoint.state.f32le"):
        source = args.parent_root / filename
        destination = parent_output / filename
        shutil.copyfile(source, destination)
        if _sha256(destination) != _sha256(source):
            _fail(f"copied parent file SHA-256 mismatch: {filename}")
    weights_path = args.output_root / "weights.f32le"
    weights_sha256 = _write_new(weights_path, weights)
    report = {
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "source": {
            "fit_report_sha256": _sha256(args.fit_report),
            "model_state_sha256": model_state_sha256,
            "confirmation_sha256": confirmation_sha256,
            "initializer_state_sha256": initializer_sha256,
            "development_cache_sha256": bounded_fit.DEVELOPMENT_CACHE_SHA256,
        },
        "config": {
            "architecture": ARCHITECTURE,
            "value_model": VALUE_MODEL,
            "policy_usage": "forbidden-value-only",
            "dim": 48,
            "card_vocab": 136,
            "group_vocab": 12,
            "history_length": 16,
            "history_feature_dim": 237,
            "epochs": 5,
            "batch_size_physical_decisions": 32,
            "seed": 20_260_810,
            "learning_rate": 3.0e-4,
            "weight_decay": 1.0e-4,
            "parent_projection_epsilon": PROJECTION_EPSILON,
        },
        "confirmation_status": "pass",
        "parent": parent,
        "weights": {
            "filename": weights_path.name,
            "encoding": WEIGHTS_ENCODING,
            "sha256": weights_sha256,
            "byte_count": len(weights),
            "parameter_count": EXPECTED_PARAMETER_COUNT,
            "parameters": parameters,
        },
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "nonclaims": [
            "value package only",
            "qualified policy remains the action generator",
            "no search or playing-strength result",
        ],
    }
    report_path = args.output_root / "report.json"
    report_sha256 = _write_new(report_path, _json_bytes(report))
    candidate = {
        "schema": CANDIDATE_SCHEMA,
        "publication_encoding": PUBLICATION_ENCODING,
        "parent": parent,
        "architecture": architecture,
        "weights": report["weights"],
        "report": {"filename": report_path.name, "sha256": report_sha256},
        "composite_model_parameter_sha256": composite_sha256,
    }
    candidate_path = args.output_root / CANDIDATE_FILENAME
    candidate_sha256 = _write_new(candidate_path, _json_bytes(candidate))
    return {
        "candidate_root": str(args.output_root),
        "candidate_json_sha256": candidate_sha256,
        "report_sha256": report_sha256,
        "weights_sha256": weights_sha256,
        "composite_model_parameter_sha256": composite_sha256,
        "parameter_count": EXPECTED_PARAMETER_COUNT,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fit-report", type=Path, required=True)
    parser.add_argument("--model-state", type=Path, required=True)
    parser.add_argument("--initializer-state", type=Path, required=True)
    parser.add_argument("--confirmation", type=Path, required=True)
    parser.add_argument("--parent-root", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(package(args), sort_keys=True, allow_nan=False))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
