from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import re
import struct
import sys
import types
import unittest


ROOT = Path(__file__).resolve().parents[2]
GOLDENS = ROOT / "data" / "flat_policy_v2" / "goldens_v2.json"
INVENTORY = ROOT / "data" / "flat_policy_v2" / "feature_inventory_v2.json"
FEATURES = ROOT / "python" / "mtg_kernel_rl" / "features.py"
TOPOLOGY_AUDIT = ROOT / "data" / "flat_policy_v2" / "ordered_topology_audit_v2.md"


def canonical_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")


def load_features_without_torch() -> types.ModuleType:
    module_name = "_flat_policy_v2_golden_features"
    module_spec = importlib.util.spec_from_file_location(module_name, FEATURES)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError("failed to load the authoritative features module")
    module = importlib.util.module_from_spec(module_spec)
    prior_torch = sys.modules.get("torch")
    sys.modules[module_name] = module
    sys.modules["torch"] = types.ModuleType("torch")
    try:
        module_spec.loader.exec_module(module)
    finally:
        if prior_torch is None:
            sys.modules.pop("torch", None)
        else:
            sys.modules["torch"] = prior_torch
        sys.modules.pop(module_name, None)
    return module


class FlatPolicyV2GoldenTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.golden = json.loads(GOLDENS.read_text(encoding="utf-8"))

    def test_inventory_and_golden_payload_digests(self) -> None:
        inventory = json.loads(INVENTORY.read_text(encoding="utf-8"))
        self.assertEqual(
            hashlib.sha256(canonical_json_bytes(inventory)).hexdigest(),
            self.golden["inventory_sha256"],
        )
        payload = dict(self.golden)
        expected = payload.pop("payload_sha256")
        self.assertEqual(hashlib.sha256(canonical_json_bytes(payload)).hexdigest(), expected)
        self.assertEqual(
            hashlib.sha256(TOPOLOGY_AUDIT.read_bytes()).hexdigest(),
            inventory["topology_audit"]["source_sha256"],
        )

    def test_topology_audit_enumerates_every_observation_list(self) -> None:
        features = load_features_without_torch()
        paths: list[str] = []

        def walk(spec: object, path: tuple[str, ...]) -> None:
            if isinstance(spec, features.ListSpec):
                paths.append(".".join(path))
                walk(spec.item, path + ("[]",))
            elif isinstance(spec, features.OptionalSpec):
                walk(spec.item, path)
            elif isinstance(spec, features.TupleSpec):
                for index, child in enumerate(spec.items):
                    walk(child, path + (str(index),))
            elif isinstance(spec, features.ObjectSpec):
                for field, child in sorted(spec.fields.items()):
                    walk(child, path + (field,))
            elif isinstance(spec, features.VariantSpec):
                for variant, child in sorted(spec.variants.items()):
                    walk(child, path + (f"<{variant}>",))

        walk(features.OBSERVATION_SPEC, ("observation",))
        self.assertEqual(len(paths), 66)
        self.assertEqual(len(paths), len(set(paths)))
        audit = TOPOLOGY_AUDIT.read_text(encoding="utf-8")
        table_paths = re.findall(r"^\| `([^`]+)` \|", audit, flags=re.MULTILINE)
        self.assertEqual(len(table_paths), len(set(table_paths)))
        self.assertEqual(set(table_paths), set(paths))

    def test_absent_present_empty_and_mapping_order_are_distinct(self) -> None:
        cases = self.golden["canonical_cases"]
        absent = cases["absent"]
        forward = cases["present_empty_forward"]
        reverse = cases["present_empty_reverse"]
        self.assertNotEqual(absent.encode("utf-8"), forward.encode("utf-8"))
        self.assertNotEqual(forward.encode("utf-8"), reverse.encode("utf-8"))
        for payload in (absent, forward, reverse):
            self.assertEqual(canonical_json_bytes(json.loads(payload)), payload.encode("utf-8"))
        self.assertEqual(
            self.golden["blocked_order_by_ordered_attacker"],
            {
                "absent": [0, None],
                "present_empty_forward": [0, 1],
                "present_empty_reverse": [1, 0],
            },
        )

    def test_red_pair_sha512_words_and_binary32_bits(self) -> None:
        cases = self.golden["canonical_cases"]
        for side, case_name in (("left", "present_empty_forward"), ("right", "present_empty_reverse")):
            payload = cases[case_name].encode("utf-8")
            expected = self.golden["red_pair"][side]
            blocks: list[str] = []
            words: list[int] = []
            f32_bits: list[str] = []
            for counter in range(6):
                digest = hashlib.sha512(
                    b"observation-state" + counter.to_bytes(4, "little") + payload
                ).digest()
                blocks.append(digest.hex())
                for offset in range(0, len(digest), 4):
                    word = int.from_bytes(digest[offset : offset + 4], "little")
                    words.append(word)
                    feature = (float(word) / float(0xFFFF_FFFF)) * 2.0 - 1.0
                    f32_bits.append(f"{int.from_bytes(struct.pack('<f', feature), 'little'):08x}")
            self.assertEqual(blocks, expected["sha512_blocks_hex"])
            self.assertEqual(words, expected["u32_words"])
            self.assertEqual(f32_bits, expected["f32_bits_hex"])
            self.assertEqual(len(words), 96)

    def test_authoritative_python_card_token_u16_boundary(self) -> None:
        features = load_features_without_torch()
        boundary = self.golden["card_token_boundary"]
        self.assertEqual(features.CARD_TOKEN_VOCAB_SIZE, 65_537)
        self.assertEqual(
            features._card_token({"card_db_id": boundary["u16_minus_one_card_db_id"]}),
            boundary["u16_minus_one_v2_token"],
        )
        self.assertEqual(
            features._card_token({"card_db_id": boundary["u16_max_card_db_id"]}),
            boundary["u16_max_v2_token"],
        )
        with self.assertRaises(features.FeatureSchemaError):
            features._card_token({"card_db_id": 65_536})


if __name__ == "__main__":
    unittest.main()
