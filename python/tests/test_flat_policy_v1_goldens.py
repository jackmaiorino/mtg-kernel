from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = ROOT / "python" / "tools" / "generate_flat_policy_v1_goldens.py"
SPEC = importlib.util.spec_from_file_location("flat_policy_v1_goldens", GENERATOR_PATH)
assert SPEC is not None and SPEC.loader is not None
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class FlatPolicyV1GoldenTests(unittest.TestCase):
    def test_checked_in_outputs_match_authoritative_python_contract(self) -> None:
        inventory = GENERATOR._inventory()
        goldens = GENERATOR._goldens(inventory)
        self.assertEqual(
            GENERATOR.INVENTORY_PATH.read_bytes(), GENERATOR._encoded(inventory)
        )
        self.assertEqual(GENERATOR.GOLDENS_PATH.read_bytes(), GENERATOR._encoded(goldens))

    def test_inventory_assigns_every_classified_leaf_exactly_once(self) -> None:
        inventory = GENERATOR._inventory()
        entries = inventory["entries"]
        self.assertEqual(len(entries), 964)
        self.assertEqual(
            inventory["counts"],
            {"model_input": 778, "operational_only": 176, "forbidden": 10},
        )
        paths = [entry["path"] for entry in entries]
        self.assertEqual(len(paths), len(set(paths)))
        self.assertTrue(all(entry["destination"] for entry in entries))
        self.assertTrue(
            all(
                (entry["destination"] == "absent")
                == (entry["classification"] == "forbidden")
                for entry in entries
            )
        )
        self.assertFalse(
            any(
                entry["classification"] == "forbidden"
                and entry["disposition"] == "model_input"
                for entry in entries
            )
        )
        self.assertFalse(
            any(
                entry["classification"] == "model_input"
                and "FlatActionObjectV1" in entry["destination"]
                for entry in entries
            )
        )
        self.assertTrue(
            all(
                entry["destination"] == "actor_relative_self_constant"
                for entry in entries
                if entry["path"].endswith(".legal_action.semantic.actor")
            )
        )

    def test_enum_and_payload_goldens_are_recomputable(self) -> None:
        inventory = GENERATOR._inventory()
        goldens = GENERATOR._goldens(inventory)
        payload_digest = goldens.pop("payload_sha256")
        self.assertEqual(
            payload_digest,
            hashlib.sha256(
                json.dumps(goldens, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        )
        self.assertEqual(
            list(goldens["enum_maps"]["object_group"]), GENERATOR.OBJECT_GROUPS
        )
        self.assertEqual(
            list(goldens["enum_maps"]["relation_role"]), GENERATOR.EDGE_ROLES
        )
        self.assertEqual(len(goldens["enum_maps"]["object_group"]), 20)
        self.assertEqual(len(goldens["enum_maps"]["relation_role"]), 14)
        self.assertEqual(
            goldens["hand_authored_vectors"]["trigger_order_lengths"], list(range(8))
        )


if __name__ == "__main__":
    unittest.main()
