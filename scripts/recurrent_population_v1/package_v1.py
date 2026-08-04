#!/usr/bin/env python3
"""Package one terminal-trained recurrent policy for native or XMage play."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from typing import Any

import torch


SCHEMA = "mtg-kernel-recurrent-terminal-deployment/v1"
DOMAIN = b"mtg-kernel-recurrent-terminal-deployment-composite/v1\0"
DEPLOYMENT_SCALE = 1.0
LOG_RATIO_BUDGET = 0.20
BASE_FULL_REFIT_REPORT_SHA256 = (
    "7c333e8bec2d332eb5dfba764f29df39d801211e74c0052bb2fd8555c68455f4"
)
BASE_CALIBRATION_REPORT_SHA256 = (
    "f3fc251dfcda2e742b02bca5d92e4eb38c2e5afe3f203a00b9a2bebfa7fe3b82"
)
PARENT_CANDIDATE_SHA256 = (
    "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
)
PARENT_WEIGHTS_SHA256 = (
    "ca3c45cd69d8d60f1f921bc78c27b098064ef6b16fe7566b84e5045681781b28"
)
PARENT_REPORT_SHA256 = (
    "7d854edb46119a611d4283e6cf4630d0207ceb24c12b4089a7d27a43c97fe0b3"
)
PARENT_COMPOSITE_SHA256 = (
    "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3"
)


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


def _git_head(repo: Path) -> str:
    return subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def _write_new(path: Path, value: Any) -> None:
    if path.exists():
        _fail(f"refusing to overwrite {path}")
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _parent_identity(root: Path) -> dict[str, Any]:
    candidate_path = root / "structured_policy_successor.json"
    candidate = json.loads(candidate_path.read_text(encoding="utf-8"))
    identity = {
        "adam_step": int(candidate["parent"]["adam_step"]),
        "candidate_sha256": _sha256(candidate_path),
        "weights_sha256": str(candidate["weights"]["sha256"]),
        "report_sha256": str(candidate["report"]["sha256"]),
        "composite_model_parameter_sha256": str(
            candidate["composite_model_parameter_sha256"]
        ),
    }
    expected = {
        "candidate_sha256": PARENT_CANDIDATE_SHA256,
        "weights_sha256": PARENT_WEIGHTS_SHA256,
        "report_sha256": PARENT_REPORT_SHA256,
        "composite_model_parameter_sha256": PARENT_COMPOSITE_SHA256,
    }
    if any(identity[key] != value for key, value in expected.items()):
        _fail("parent identity mismatch")
    return identity


def _composite(parent: dict[str, Any], files: dict[str, Any], state_sha: str) -> str:
    digest = hashlib.sha256()
    digest.update(DOMAIN)
    for value in (
        parent["composite_model_parameter_sha256"],
        files["model"]["sha256"],
        state_sha,
        files["worker"]["sha256"],
        files["model_definition"]["sha256"],
    ):
        digest.update(value.encode("ascii") + b"\0")
    digest.update(struct.pack("<d", LOG_RATIO_BUDGET))
    digest.update(struct.pack("<d", DEPLOYMENT_SCALE))
    return digest.hexdigest()


def run(args: argparse.Namespace) -> int:
    repo = Path(__file__).resolve().parents[2]
    if args.output.exists():
        _fail(f"output already exists: {args.output}")
    for path in (args.model, args.training_report, args.parent):
        if not path.exists():
            _fail(f"required input does not exist: {path}")
    payload = torch.load(args.model, map_location="cpu", weights_only=False)
    state = payload.get("model_state_dict")
    if not isinstance(state, dict) or not state:
        _fail("model state is missing")
    state_sha = _state_sha256(state)
    if payload.get("model_state_sha256") not in (None, state_sha):
        _fail("model payload state SHA-256 mismatch")
    report = json.loads(args.training_report.read_text(encoding="utf-8"))
    model_sha = _sha256(args.model)
    report_sha = _sha256(args.training_report)
    if report.get("model_state_sha256") != state_sha:
        _fail("training report state SHA-256 mismatch")
    if report.get("model_file_sha256") not in (None, model_sha):
        _fail("training report model file SHA-256 mismatch")
    if report.get("model_file_sha256") is None and report_sha != BASE_FULL_REFIT_REPORT_SHA256:
        _fail("only the exact recurrent initialization may omit model file identity")

    parent = _parent_identity(args.parent)
    args.output.mkdir(parents=True)
    shutil.copyfile(args.model, args.output / "model.pt")
    shutil.copyfile(
        repo / "scripts" / "recurrent_structured_learner_v1" / "model_v1.py",
        args.output / "model_v1.py",
    )
    shutil.copyfile(
        repo / "scripts" / "recurrent_cp7_deployment_v1" / "worker_v1.py",
        args.output / "worker_v1.py",
    )
    shutil.copytree(args.parent, args.output / "parent")
    files = {
        "model": {"path": "model.pt", "sha256": _sha256(args.output / "model.pt")},
        "model_definition": {
            "path": "model_v1.py",
            "sha256": _sha256(args.output / "model_v1.py"),
        },
        "worker": {
            "path": "worker_v1.py",
            "sha256": _sha256(args.output / "worker_v1.py"),
        },
    }
    manifest = {
        "schema": SCHEMA,
        "architecture": "width128-two-layer-gru-structured-cp7-residual/v1",
        "git_commit": _git_head(repo),
        "deployment_scale": DEPLOYMENT_SCALE,
        "log_ratio_budget": LOG_RATIO_BUDGET,
        "model_state_sha256": state_sha,
        "files": files,
        "parent": {"path": "parent", **parent},
        "identity": {
            "authority_kind": "recurrent-terminal-policy-deployment-v1",
            "model_parameter_sha256": _composite(parent, files, state_sha),
        },
        "source": {
            "full_refit_report_sha256": BASE_FULL_REFIT_REPORT_SHA256,
            "deployment_calibration_report_sha256": BASE_CALIBRATION_REPORT_SHA256,
            "terminal_training_report_sha256": report_sha,
        },
        "non_claims": [
            "training diagnostics are not playing strength",
            "terminal win or loss remains the only promotion measure",
        ],
    }
    manifest_path = args.output / "recurrent_cp7_deployment.json"
    _write_new(manifest_path, manifest)
    print(
        json.dumps(
            {
                "package_root": str(args.output),
                "manifest_sha256": _sha256(manifest_path),
                "model_file_sha256": model_sha,
                "model_state_sha256": state_sha,
                "model_parameter_sha256": manifest["identity"][
                    "model_parameter_sha256"
                ],
            },
            sort_keys=True,
        )
    )
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model", type=Path, required=True)
    parser.add_argument("--training-report", type=Path, required=True)
    parser.add_argument(
        "--parent",
        type=Path,
        default=Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate"),
    )
    return parser.parse_args()


if __name__ == "__main__":
    try:
        raise SystemExit(run(arguments()))
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"package_recurrent_population_v1: ERROR: {error}", file=sys.stderr)
        raise SystemExit(2)
