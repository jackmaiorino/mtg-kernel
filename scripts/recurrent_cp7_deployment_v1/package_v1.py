#!/usr/bin/env python3
"""Build a self-contained recurrent CP7 deployment package."""

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


SCHEMA = "mtg-kernel-recurrent-cp7-deployment/v1"
DOMAIN = b"mtg-kernel-recurrent-cp7-deployment-composite/v1\0"
DEPLOYMENT_SCALE = 0.97
LOG_RATIO_BUDGET = 0.49
MODEL_STATE_SHA256 = "d736296425de2c438bb9be02ab6c89e51da4c17c1408de6ff3309029b2d06dca"
MODEL_FILE_SHA256 = "6c33f6d449b76e24c00bc7d46052b04488ddb9ec574009831d2fa90ea01bd55d"
PARENT_CANDIDATE_SHA256 = "204beb91c1a4b039e0c497f2b420e823b5cc9e2ceb8560f897d0b6251e916b72"
PARENT_WEIGHTS_SHA256 = "ca3c45cd69d8d60f1f921bc78c27b098064ef6b16fe7566b84e5045681781b28"
PARENT_REPORT_SHA256 = "7d854edb46119a611d4283e6cf4630d0207ceb24c12b4089a7d27a43c97fe0b3"
PARENT_COMPOSITE_SHA256 = "47b10c1114efc01f9445c71c0c8c4d8cd4a4b89a2154ac68275f3b0c6ebb9ce3"


def _fail(message: str) -> None:
    raise RuntimeError(message)


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
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


def _composite(
    parent: dict[str, Any], model_sha: str, state_sha: str, worker_sha: str, definition_sha: str
) -> str:
    digest = hashlib.sha256()
    digest.update(DOMAIN)
    for value in (
        parent["composite_model_parameter_sha256"],
        model_sha,
        state_sha,
        worker_sha,
        definition_sha,
    ):
        digest.update(value.encode("ascii") + b"\0")
    digest.update(struct.pack("<d", LOG_RATIO_BUDGET))
    digest.update(struct.pack("<d", DEPLOYMENT_SCALE))
    return digest.hexdigest()


def run(args: argparse.Namespace) -> int:
    repo = Path(__file__).resolve().parents[2]
    if args.output.exists():
        _fail(f"output already exists: {args.output}")
    if _sha256(args.model) != MODEL_FILE_SHA256:
        _fail("model file SHA-256 mismatch")
    parent = _parent_identity(args.parent)
    args.output.mkdir(parents=True)
    shutil.copyfile(args.model, args.output / "model.pt")
    shutil.copyfile(
        repo / "scripts" / "recurrent_structured_learner_v1" / "model_v1.py",
        args.output / "model_v1.py",
    )
    shutil.copyfile(Path(__file__).with_name("worker_v1.py"), args.output / "worker_v1.py")
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
    composite = _composite(
        parent,
        files["model"]["sha256"],
        MODEL_STATE_SHA256,
        files["worker"]["sha256"],
        files["model_definition"]["sha256"],
    )
    manifest = {
        "schema": SCHEMA,
        "architecture": "width128-two-layer-gru-structured-cp7-residual/v1",
        "git_commit": _git_head(repo),
        "deployment_scale": DEPLOYMENT_SCALE,
        "log_ratio_budget": LOG_RATIO_BUDGET,
        "model_state_sha256": MODEL_STATE_SHA256,
        "files": files,
        "parent": {"path": "parent", **parent},
        "identity": {
            "authority_kind": "recurrent-cp7-deployment-v1",
            "model_parameter_sha256": composite,
        },
        "source": {
            "full_refit_report_sha256": "7c333e8bec2d332eb5dfba764f29df39d801211e74c0052bb2fd8555c68455f4",
            "deployment_calibration_report_sha256": "f3fc251dfcda2e742b02bca5d92e4eb38c2e5afe3f203a00b9a2bebfa7fe3b82",
        },
        "non_claims": [
            "CP7 label fit is not playing strength",
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
                "model_file_sha256": files["model"]["sha256"],
                "model_state_sha256": MODEL_STATE_SHA256,
                "model_parameter_sha256": composite,
                "parent_adam_step": parent["adam_step"],
            },
            sort_keys=True,
        )
    )
    return 0


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--model",
        type=Path,
        default=Path(r"D:\mtg-kernel-recurrent-cp7-dagger-v1\full-refit\model.pt"),
    )
    parser.add_argument(
        "--parent",
        type=Path,
        default=Path(r"D:\mtg-kernel-policy-only-structured-successor-v1\candidate"),
    )
    return parser.parse_args()


if __name__ == "__main__":
    try:
        sys.exit(run(arguments()))
    except Exception as error:
        print(f"package_recurrent_cp7: ERROR: {error}", file=sys.stderr)
        sys.exit(1)
