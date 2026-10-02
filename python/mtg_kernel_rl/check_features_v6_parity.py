"""Bitwise native FlatV3 / rich PythonV6 fixture comparison, on CPU only.

Run with PYTHONPATH=python: python -m mtg_kernel_rl.check_features_v6_parity FILE.
The input is native fixture JSONL or captured output containing prefixed rows.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from .features_v6 import encode_decision


FLOAT_FIELDS = ("state", "object_features", "edge_features", "action_features", "action_ref_features")
INTEGER_FIELDS = ("object_card_ids", "object_groups", "object_node_ids", "edge_source_indices",
                  "edge_target_indices", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices")
PREFIX = "NATIVE_FLAT_V3_FIXTURE="


def compare_fixture(fixture: dict) -> dict:
    encoded = encode_decision(fixture["observation"], fixture["legal_actions"])
    expected = fixture["expected"]
    checked = 0
    for name in FLOAT_FIELDS + INTEGER_FIELDS:
        tensor = getattr(encoded, name).detach().cpu().contiguous().numpy()
        actual = tensor.view("uint32").reshape(-1).tolist() if name in FLOAT_FIELDS else tensor.reshape(-1).tolist()
        key = name + "_bits" if name in FLOAT_FIELDS else name
        wanted = expected[key]
        if actual != wanted:
            mismatch = next((i for i, (a, b) in enumerate(zip(actual, wanted)) if a != b), min(len(actual), len(wanted)))
            raise AssertionError(f"{fixture.get('name', '<unnamed>')}: {key} mismatch at {mismatch}; "
                                 f"Python length {len(actual)}, native length {len(wanted)}; "
                                 f"Python {actual[mismatch:mismatch + 1]}, native {wanted[mismatch:mismatch + 1]}")
        checked += len(actual)
    return {"name": fixture.get("name"), "tensor_fields": 13, "checked_scalars": checked, "bitwise_equal": True}


def check_file(path: Path) -> list[dict]:
    results = []
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        if PREFIX in line:
            value = line.split(PREFIX, 1)[1]
        elif line.lstrip().startswith("{"):
            value = line
        else:
            continue
        fixture = json.loads(value)
        if "observation" in fixture and "expected" in fixture:
            results.append(compare_fixture(fixture))
    if not results:
        raise ValueError(f"no native FlatV3 fixtures found in {path}")
    return results


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path)
    arguments = parser.parse_args()
    results = [item for path in arguments.files for item in check_file(path)]
    print(json.dumps({"schema": "flat-v3-python-v6-parity-1", "fixtures": results,
                      "all_bitwise_equal": True}, sort_keys=True))


if __name__ == "__main__":
    main()
