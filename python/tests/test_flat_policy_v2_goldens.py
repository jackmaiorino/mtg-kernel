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
ACTION_CONTRACT = ROOT / "data" / "flat_policy_v2" / "action_contract_v2.json"
BASE_GOLDENS = ROOT / "data" / "flat_policy_v1" / "goldens_v1.json"
FEATURES = ROOT / "python" / "mtg_kernel_rl" / "features.py"
TOPOLOGY_AUDIT = ROOT / "data" / "flat_policy_v2" / "ordered_topology_audit_v2.md"
GENERATOR = ROOT / "python" / "tools" / "generate_flat_policy_v2_goldens.py"


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


def load_generator() -> types.ModuleType:
    module_name = "_flat_policy_v2_golden_generator"
    module_spec = importlib.util.spec_from_file_location(module_name, GENERATOR)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError("failed to load the V2 golden generator")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_name] = module
    try:
        module_spec.loader.exec_module(module)
    finally:
        sys.modules.pop(module_name, None)
    return module


class FlatPolicyV2GoldenTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.golden = json.loads(GOLDENS.read_text(encoding="utf-8"))
        cls.action_contract = json.loads(ACTION_CONTRACT.read_text(encoding="utf-8"))

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
        self.assertEqual(
            hashlib.sha256(ACTION_CONTRACT.read_bytes()).hexdigest(),
            inventory["action_contract"]["source_sha256"],
        )
        self.assertEqual(
            hashlib.sha256(canonical_json_bytes(self.action_contract)).hexdigest(),
            inventory["action_contract"]["canonical_sha256"],
        )
        self.assertEqual(
            self.golden["action_contract_binding"],
            {
                "source_sha256": inventory["action_contract"]["source_sha256"],
                "canonical_sha256": inventory["action_contract"]["canonical_sha256"],
            },
        )

    def test_authoritative_generator_reproduces_committed_artifacts(self) -> None:
        generator = load_generator()
        inventory, goldens = generator.build_artifacts()
        self.assertEqual(
            INVENTORY.read_text(encoding="utf-8"),
            generator.pretty_json(inventory),
        )
        self.assertEqual(
            GOLDENS.read_text(encoding="utf-8"),
            generator.pretty_json(goldens),
        )

    def test_action_ref_role_crosswalk_exactly_reuses_v1_authority(self) -> None:
        expected_entries = [
            {"role": "source", "rust_internal_id": 0, "python_projection_id": 0},
            {"role": "candidate", "rust_internal_id": 1, "python_projection_id": 1},
            {"role": "card", "rust_internal_id": 2, "python_projection_id": 2},
            {"role": "attacker", "rust_internal_id": 3, "python_projection_id": 3},
            {"role": "blocker", "rust_internal_id": 4, "python_projection_id": 4},
            {"role": "target_object", "rust_internal_id": 5, "python_projection_id": 5},
            {"role": "cards", "rust_internal_id": 6, "python_projection_id": 6},
            {"role": "pending_sources", "rust_internal_id": 7, "python_projection_id": 9},
        ]
        expected_projection_only = [
            {"role": "attackers", "python_projection_id": 7},
            {"role": "blockers", "python_projection_id": 8},
        ]
        base_crosswalk = json.loads(BASE_GOLDENS.read_text(encoding="utf-8"))[
            "action_ref_role_crosswalk"
        ]
        authority_digest = hashlib.sha256(canonical_json_bytes(base_crosswalk)).hexdigest()
        contract_mapping = self.action_contract["reference_role_mapping"]
        golden_mapping = self.golden["action_ref_role_mapping"]

        self.assertEqual(base_crosswalk["entries"], expected_entries)
        self.assertEqual(base_crosswalk["projection_only"], expected_projection_only)
        self.assertEqual(
            [entry["python_projection_id"] for entry in expected_entries],
            [0, 1, 2, 3, 4, 5, 6, 9],
        )
        self.assertEqual(base_crosswalk["mapping_version"], 1)
        self.assertEqual(contract_mapping["mapping_version"], 1)
        self.assertEqual(contract_mapping["canonical_sha256"], authority_digest)
        self.assertEqual(golden_mapping["canonical_sha256"], authority_digest)
        self.assertEqual(golden_mapping["entries"], expected_entries)
        self.assertEqual(golden_mapping["projection_only"], expected_projection_only)
        self.assertEqual(golden_mapping["internal_to_projection"], [0, 1, 2, 3, 4, 5, 6, 9])

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
        features = load_features_without_torch()
        full_cases = self.golden["full_observation_cases"]
        model_cases = self.golden["model_canonical_cases"]
        observations: dict[str, dict[str, object]] = {}
        for case_name, payload in full_cases.items():
            observation = json.loads(payload)
            features.assert_observation_classified(observation)
            self.assertEqual(canonical_json_bytes(observation), payload.encode("utf-8"))
            canonical = features._canonical_model_value(
                observation,
                features.OBSERVATION_SPEC,
                ("observation",),
                features._CanonicalContext(observation["acting_player"], observation),
            )
            self.assertEqual(canonical_json_bytes(canonical), model_cases[case_name].encode("utf-8"))
            observations[case_name] = observation

        forward = observations["present_empty_forward"]
        reverse = observations["present_empty_reverse"]
        forward_mapping = forward["projection"]["combat"].pop("attacker_to_ordered_blockers")
        reverse_mapping = reverse["projection"]["combat"].pop("attacker_to_ordered_blockers")
        self.assertNotEqual(forward_mapping, reverse_mapping)
        self.assertEqual(forward, reverse)
        self.assertNotEqual(
            model_cases["absent"].encode("utf-8"),
            model_cases["present_empty_forward"].encode("utf-8"),
        )
        self.assertNotEqual(
            model_cases["present_empty_forward"].encode("utf-8"),
            model_cases["present_empty_reverse"].encode("utf-8"),
        )
        self.assertEqual(
            self.golden["blocked_order_by_ordered_attacker"],
            {
                "absent": [0, None],
                "present_empty_forward": [0, 1],
                "present_empty_reverse": [1, 0],
            },
        )

    def test_red_pair_sha512_words_and_binary32_bits(self) -> None:
        cases = self.golden["model_canonical_cases"]
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

    def test_action_contract_serialization_commitment_and_v1_stability(self) -> None:
        contract = self.action_contract
        self.assertEqual(contract["source_digest_semantics"], "raw_utf8_file_bytes_sha256")
        self.assertEqual(contract["semantic_digest_semantics"], "canonical_json_utf8_sha256")
        commitment_contract = contract["candidate_commitment"]
        golden = self.golden["action_commitment"]
        self.assertEqual(
            golden["card_db_hash_authority"],
            {
                "source": "mtg-kernel/src/card_def.rs::card_db_hash_v5_is_frozen",
                "value_hex": "a06fa9566106f0ea",
            },
        )
        full = golden["full_distinct_fields_v2"]

        object_field_names = [row[0] for row in commitment_contract["object_row"]["fields"]]
        action_field_names = [row[0] for row in commitment_contract["action_row"]["fields"]]
        reference_field_names = [row[0] for row in commitment_contract["reference_row"]["fields"]]
        self.assertEqual(object_field_names[1:], list(full["objects"][0]))
        self.assertEqual(action_field_names[1:], list(full["actions"][0]))
        self.assertEqual(reference_field_names[:6], list(full["references"][0]))
        self.assertEqual(
            [row[1] for row in commitment_contract["header_fields"][:4]],
            ["u32_le"] * 4,
        )
        self.assertEqual(
            commitment_contract["stream_order"],
            "header_then_objects_by_object_index_then_each_actions_refs_in_slice_order_then_action",
        )
        self.assertEqual(
            [full["action_count"], full["ref_count"], full["object_count"]],
            [3, 4, 2],
        )
        self.assertEqual(
            [(row["ref_start"], row["ref_len"]) for row in full["actions"]],
            [(0, 0), (0, 3), (3, 1)],
        )

        reconstructed = bytes.fromhex(full["header_hex"])
        reconstructed += b"".join(bytes.fromhex(row) for row in full["object_rows_hex"])
        consumed_refs = 0
        for action_index, (action, action_row) in enumerate(
            zip(full["actions"], full["action_rows_hex"], strict=True)
        ):
            ref_start = action["ref_start"]
            ref_end = ref_start + action["ref_len"]
            self.assertEqual(ref_start, consumed_refs)
            for ref_index in range(ref_start, ref_end):
                self.assertEqual(full["references"][ref_index]["action_index"], action_index)
                reconstructed += bytes.fromhex(full["reference_rows_hex"][ref_index])
            reconstructed += bytes.fromhex(action_row)
            consumed_refs = ref_end
        self.assertEqual(consumed_refs, len(full["references"]))
        self.assertEqual(reconstructed.hex(), full["stream_hex"])
        digest = hashlib.sha256(reconstructed).digest()
        self.assertEqual(digest.hex(), full["sha256_hex"])
        self.assertEqual(digest[:16].hex(), full["commitment_hex"])
        self.assertEqual(
            {row["card_token"] for row in full["objects"]},
            {65_535, 65_536},
        )
        self.assertIn((65_535).to_bytes(4, "little"), reconstructed)
        self.assertIn((65_536).to_bytes(4, "little"), reconstructed)

        frozen_v1 = golden["frozen_v1_comparison"]
        same_v2 = golden["same_representable_rows_v2"]
        self.assertEqual(
            hashlib.sha256(bytes.fromhex(frozen_v1["stream_hex"])).hexdigest(),
            frozen_v1["sha256_hex"],
        )
        self.assertEqual(frozen_v1["commitment_hex"], "208df409eb3dff44ce1980611250948f")
        self.assertEqual(same_v2["commitment_hex"], "dc0bf1ed6b5d073eeae97fe268536a5e")
        self.assertNotEqual(frozen_v1["commitment_hex"], same_v2["commitment_hex"])

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
