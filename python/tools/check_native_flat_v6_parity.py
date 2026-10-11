"""Bitwise thirteen-tensor parity for the real-declaration Native V6 export."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mtg_kernel_rl import features_v9 as features
import torch


def assert_identity(artifact: dict) -> dict:
    descriptor_path = Path(__file__).resolve().parents[2] / "data/flat_policy_v6/feature_contract_v6.json"
    descriptor_raw = descriptor_path.read_bytes()
    descriptor = json.loads(descriptor_raw)
    expected = {
        "feature_schema_version": features.FEATURE_SCHEMA_VERSION,
        "feature_registry_version": features.FEATURE_REGISTRY_VERSION,
        "feature_contract_digest": features.feature_contract_fingerprint(),
        "feature_encoding_digest": features.encoding_contract_fingerprint(),
        "features_source_sha256": hashlib.sha256(Path(features.__file__).read_bytes()).hexdigest(),
        "feature_descriptor_sha256": hashlib.sha256(descriptor_raw).hexdigest(),
        "model_contract_version": features.MODEL_CONTRACT_VERSION,
    }
    for field, value in expected.items():
        if artifact.get(field) != value:
            raise SystemExit(f"Native identity differs: {field}")
        if field != "feature_descriptor_sha256" and descriptor.get(field) != value:
            raise SystemExit(f"Descriptor identity differs: {field}")
    predecessor_hash = hashlib.sha256(Path(features.predecessor.__file__).read_bytes()).hexdigest()
    if descriptor.get("predecessor_source_sha256") != predecessor_hash:
        raise SystemExit("Descriptor predecessor source differs")
    return expected


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    parser.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    raw = args.path.read_bytes()
    artifact = json.loads(raw)
    if artifact["schema"] != "flat-v6-native-python-parity-1":
        raise SystemExit("Unexpected parity schema")
    identity = assert_identity(artifact)
    names = [case["name"] for case in artifact["cases"]]
    expected_names = {f"declared_cleared_p{declarer}_observer_p{actor}"
                      for declarer in (0, 1) for actor in (0, 1)}
    if len(names) != 4 or set(names) != expected_names:
        raise SystemExit(f"Fixture membership differs: {names}")
    fields = ("state", "object_features", "object_card_ids", "object_groups", "object_node_ids",
              "edge_features", "edge_source_indices", "edge_target_indices", "action_features",
              "action_ref_features", "action_ref_card_ids", "action_ref_action_indices", "action_ref_node_indices")
    elements = 0
    for case in artifact["cases"]:
        encoded = features.encode_decision(case["observation"], case["legal_actions"])
        if set(case["tensors"]) != set(fields):
            raise SystemExit(f"Tensor roster differs: {case['name']}")
        for name in fields:
            expected = getattr(encoded, name).flatten()
            actual = torch.tensor(case["tensors"][name], dtype=expected.dtype).flatten()
            if expected.shape != actual.shape:
                raise SystemExit(f"Tensor shape differs: {case['name']} {name}")
            matches = (torch.equal(expected.view(torch.int32), actual.view(torch.int32))
                       if expected.dtype == torch.float32 else torch.equal(expected, actual))
            if not matches:
                raise SystemExit(f"Tensor bits differ: {case['name']} {name}")
            elements += expected.numel()
    receipt = {"schema": "flat-v6-native-python-parity-receipt-1", "result": "pass",
               "cases": names, "tensor_groups_per_case": len(fields), "elements": elements,
               "artifact_sha256": hashlib.sha256(raw).hexdigest(),
               **identity,
               "python": sys.version, "torch": torch.__version__,
               "scope": "Real engine declarations followed by explicit cleared-combat fixture projections, both declaring and observing seats. Encoding parity does not admit cards, models, training or strength."}
    with args.receipt.open("x", encoding="utf-8") as handle:
        handle.write(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    print(json.dumps(receipt, sort_keys=True))


if __name__ == "__main__":
    main()
