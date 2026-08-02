#!/usr/bin/env python3
"""Republish an existing fit without nondeterministic report timing metadata."""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path
from typing import Any


def _json_bytes(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + "\n").encode("utf-8")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    source: Path = args.source
    output: Path = args.output
    report = json.loads((source / "report.json").read_text(encoding="utf-8"))
    candidate = json.loads((source / "structured_candidate.json").read_text(encoding="utf-8"))
    if report.get("schema") != "mtg-kernel-structured-policy-residual-fit/v1":
        raise ValueError("unexpected source report")
    report.pop("runtime_seconds", None)
    output.mkdir(parents=True, exist_ok=False)
    shutil.copytree(source / "parent", output / "parent")
    shutil.copyfile(source / "weights.f32le", output / "weights.f32le")
    (output / "report.json").write_bytes(_json_bytes(report))
    candidate["report"]["sha256"] = _sha256(output / "report.json")
    (output / "structured_candidate.json").write_bytes(_json_bytes(candidate))
    print(
        json.dumps(
            {
                "candidate_sha256": _sha256(output / "structured_candidate.json"),
                "report_sha256": _sha256(output / "report.json"),
                "weights_sha256": _sha256(output / "weights.f32le"),
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
