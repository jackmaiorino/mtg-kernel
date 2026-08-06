#!/usr/bin/env python3
"""Fail-closed, terminal-blind validation for the scaled self-play replay handoff."""

from __future__ import annotations

import argparse
import json
from collections.abc import Mapping
from pathlib import Path
from typing import Any


INPUT_SCHEMA = "mtg-kernel-scaled-selfplay-replay-manifest/v1"
OUTPUT_SCHEMA = "mtg-kernel-scaled-selfplay-replay-handoff-validation/v1"
PROGRAM_COMMIT = "838920e359c7a1152d97c450f4575c6be2309f22"
PROGRAM_DOCUMENT_SHA256 = "b0e836858379137e9f5068f1ed2d3cb98d0d6507d09170d8272caad2a989ea38"
RETEST_FORMAL_MANIFEST_PATH = (
    r"D:\mtg-kernel-regularized-continuation-retest-v1\development\seed-982001"
    r"\full-horizon-evaluation\attempt-001\formal\full-horizon-evaluation-manifest.json"
)
RETEST_FORMAL_MANIFEST_SHA256 = "f3128e5f700830df2110d6abb06b5b6f7f8f642ac5064c5d3188afac93aed2c8"
RETEST_DISPOSITION = "ADVANCE"
SEEDS = (970001, 970002, 970003)


def _lineage(
    store_root: str,
    tree: str,
    run: str,
    checkpoint: str,
    sidecar: str,
    native: str,
    model: str,
) -> dict[str, Any]:
    return {
        "store_root": store_root,
        "store_tree_sha256": tree,
        "run_sha256": run,
        "checkpoint_sha256": checkpoint,
        "sidecar_sha256": sidecar,
        "native_state_sha256": native,
        "model_parameter_sha256": model,
        "adam_step": 512,
        "generation": 512,
        "progress": {
            "completed_episode_count": 32768,
            "next_episode_index": 32768,
            "successful_update_count": 512,
        },
    }


EXPECTED_LINEAGES = {
    970001: _lineage(
        r"D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training"
        r"\attempt-003\wave-00-seed-970001-gpu0\run-0\store",
        "2d6650f111cebcb8e87271fb3446127306e2c4006da793c45a7aec5d80c7780e",
        "2307caf5a0093bf3f6f9d3673788eac1d73bcd248bfb6fcb3af785a596304cab",
        "21f95221663a7a064d4d5935d19c95dc108a84085513524f48def0b0da21a2bc",
        "2ee82c53afb9c4cd8343ca67411d9a0b5db800215688f809a08a44c8016953a5",
        "e2e3fdb4216a013fdb043bcb90f33f590d5f7d72a77b5999c423919da3ae3b85",
        "a51d05f8f89e3cca652e8c2daaa289a65cfdb317164d07410395430044b54ed0",
    ),
    970002: _lineage(
        r"D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training"
        r"\attempt-003\wave-00-seed-970002-gpu1\run-0\store",
        "bcecb18db197a5ef14c8512642a3f15191f7dd05e389c02c129853c9496deda7",
        "fdbd65dca0660afe1156f4dff49204325064802e7d44606eb44b7529db528ce1",
        "c3aa704e7670c158da82ad4602a20bcec3240f275ecb7aac9ca42fb341f482df",
        "16c834b632e99589c5970dc52164ea12647f954e43e7bfe61b5d4d767133b9aa",
        "304053bdc96ef094d97506f5605fc599aae045c770cbd6fa7efcebfccc9069b6",
        "1e9022105aec341101c0b14ffa4d509b4073a2f80b213e71dd0065f036e701dd",
    ),
    970003: _lineage(
        r"D:\mtg-kernel-regularized-continuation-retest-v1\development\full-horizon-training"
        r"\attempt-003\wave-01-seed-970003-gpu1\run-0\store",
        "1a1bdb75099b50b4d250d3e03ab6d882718f017e2c6d715bc8a67d3022b627ec",
        "9a1c417e6990c54929481f5eee19cf0f9f8d816fa72a3e3a575fdde603364295",
        "814583b210191bc00ec1cf5f485eb6b83ffce2d4c2e632b87874d64e3b62cb3e",
        "50108e3751ab52b6432903cac0b57addb747e287e41bc83f57e0bf9110149788",
        "b3a8811923533bda7b1a8d2dbfa0b5b8ec187b1d40a7029d348a0dabbb04dbc3",
        "861f28ca95316e68d1552986294aae0f7677af64b21f615d5bfcaff01276602c",
    ),
}


def _canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def canonical_output(value: Mapping[str, Any]) -> str:
    """Serialize a validation result deterministically and without whitespace."""

    return _canonical_json(value)


def _result(document: Mapping[str, Any], disposition: str, errors: list[str]) -> dict[str, Any]:
    result: dict[str, Any] = {
        "schema": OUTPUT_SCHEMA,
        "disposition": disposition,
        "continuation_authorized": disposition == "ADVANCE",
        "global_target_generation": document.get("global_target_generation"),
        "replay_end_generation": document.get("replay_end_generation"),
        "program_updates": document.get("program_updates"),
        "seeds": sorted(
            lineage.get("seed")
            for lineage in document.get("lineages", [])
            if isinstance(lineage, Mapping) and isinstance(lineage.get("seed"), int)
        ),
    }
    if errors:
        result["errors"] = errors
    return result


def _check_exact(document: Mapping[str, Any], key: str, expected: Any, errors: list[str]) -> None:
    if document.get(key) != expected:
        errors.append(f"{key} must equal {expected!r}")


def _check_exact_keys(record: Mapping[str, Any], expected: set[str], label: str, errors: list[str]) -> None:
    actual = set(record)
    if actual != expected:
        errors.append(f"{label} keys must be exactly {sorted(expected)!r}")


def _is_sha256(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in "0123456789abcdef" for character in value)


def _check_source(record: Any, expected: Mapping[str, Any], label: str, errors: list[str]) -> None:
    if not isinstance(record, Mapping):
        errors.append(f"{label} must be an object")
        return
    _check_exact_keys(record, set(expected), label, errors)
    for key, value in expected.items():
        if record.get(key) != value:
            errors.append(f"{label}.{key} does not match audited provenance")


def _check_successor(record: Any, expected: Mapping[str, Any], label: str, errors: list[str]) -> None:
    keys = {
        "store_root",
        "store_tree_sha256",
        "run_sha256",
        "checkpoint_sha256",
        "sidecar_sha256",
        "native_state_sha256",
        "model_parameter_sha256",
        "bound_source_store_tree_sha256",
        "bound_source_run_sha256",
        "bound_retest_manifest_sha256",
        "adam_step",
        "generation",
        "progress",
    }
    if not isinstance(record, Mapping):
        errors.append(f"{label} must be an object")
        return
    _check_exact_keys(record, keys, label, errors)
    for key in ("store_tree_sha256", "run_sha256", "checkpoint_sha256", "sidecar_sha256"):
        if not _is_sha256(record.get(key)):
            errors.append(f"{label}.{key} must be lowercase SHA-256")
    if not isinstance(record.get("store_root"), str) or not record["store_root"]:
        errors.append(f"{label}.store_root must be a nonempty string")
    if record.get("store_root") == expected["store_root"]:
        errors.append(f"{label}.store_root must name the successor Store")
    if record.get("store_tree_sha256") == expected["store_tree_sha256"]:
        errors.append(f"{label}.store_tree_sha256 must not reuse the source Store tree")
    exact = {
        "native_state_sha256": expected["native_state_sha256"],
        "model_parameter_sha256": expected["model_parameter_sha256"],
        "bound_source_store_tree_sha256": expected["store_tree_sha256"],
        "bound_source_run_sha256": expected["run_sha256"],
        "bound_retest_manifest_sha256": RETEST_FORMAL_MANIFEST_SHA256,
        "adam_step": 512,
        "generation": 512,
        "progress": expected["progress"],
    }
    for key, value in exact.items():
        if record.get(key) != value:
            errors.append(f"{label}.{key} does not match the handoff contract")


def validate_manifest(document: Any) -> dict[str, Any]:
    """Validate one replay manifest without reading or interpreting outcomes."""

    if not isinstance(document, Mapping):
        return {
            "schema": OUTPUT_SCHEMA,
            "disposition": "FAIL-INVESTIGATE",
            "continuation_authorized": False,
            "errors": ["manifest must be a JSON object"],
        }

    errors: list[str] = []
    _check_exact_keys(
        document,
        {
            "schema",
            "global_target_generation",
            "replay_end_generation",
            "program_updates",
            "terminal_outcomes_read",
            "authorities",
            "lineages",
        },
        "manifest",
        errors,
    )
    _check_exact(document, "schema", INPUT_SCHEMA, errors)
    _check_exact(document, "global_target_generation", 1536, errors)
    _check_exact(document, "replay_end_generation", 512, errors)
    _check_exact(document, "program_updates", 1024, errors)
    _check_exact(document, "terminal_outcomes_read", False, errors)

    authorities = document.get("authorities")
    if not isinstance(authorities, Mapping):
        errors.append("authorities must be an object")
    else:
        authority_values = {
            "program_commit": PROGRAM_COMMIT,
            "program_document_sha256": PROGRAM_DOCUMENT_SHA256,
            "retest_formal_manifest_path": RETEST_FORMAL_MANIFEST_PATH,
            "retest_formal_manifest_sha256": RETEST_FORMAL_MANIFEST_SHA256,
            "retest_disposition": RETEST_DISPOSITION,
        }
        _check_exact_keys(authorities, set(authority_values), "authorities", errors)
        for key, expected in authority_values.items():
            if authorities.get(key) != expected:
                errors.append(f"authorities.{key} does not match bound authority")

    lineages = document.get("lineages")
    if not isinstance(lineages, list):
        errors.append("lineages must be an array")
        return _result(document, "FAIL-INVESTIGATE", errors)
    if len(lineages) != len(SEEDS):
        errors.append("lineages must contain exactly three entries")

    seen: list[int] = []
    for index, lineage in enumerate(lineages):
        label = f"lineages[{index}]"
        if not isinstance(lineage, Mapping):
            errors.append(f"{label} must be an object")
            continue
        seed = lineage.get("seed")
        if not isinstance(seed, int) or isinstance(seed, bool):
            errors.append(f"{label}.seed must be an integer")
            continue
        seen.append(seed)
        expected = EXPECTED_LINEAGES.get(seed)
        if expected is None:
            errors.append(f"{label}.seed {seed} is not one of {SEEDS}")
            continue
        _check_exact_keys(lineage, {"seed", "source", "successor"}, label, errors)
        _check_source(lineage.get("source"), expected, f"{label}.source", errors)
        _check_successor(lineage.get("successor"), expected, f"{label}.successor", errors)

    if sorted(seen) != list(SEEDS):
        errors.append(f"lineage seeds must be exactly {SEEDS}")

    return _result(document, "ADVANCE" if not errors else "FAIL-INVESTIGATE", errors)


def _load(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as stream:
        return json.load(stream)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args(argv)
    try:
        document = _load(args.manifest)
    except (OSError, json.JSONDecodeError) as exc:
        result = {
            "schema": OUTPUT_SCHEMA,
            "disposition": "FAIL-INVESTIGATE",
            "continuation_authorized": False,
            "errors": [f"could not read manifest: {exc}"],
        }
        print(canonical_output(result))
        return 1
    result = validate_manifest(document)
    print(canonical_output(result))
    return 0 if result["disposition"] == "ADVANCE" else 1


if __name__ == "__main__":
    raise SystemExit(main())
