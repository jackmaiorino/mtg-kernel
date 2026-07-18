from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
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
                and entry["destination"].startswith("FlatActionObjectV1")
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

    def test_destination_registry_is_exact_exhaustive_and_fail_closed(self) -> None:
        authoritative = GENERATOR.classification_registry()
        declared = GENERATOR.DESTINATION_REGISTRY
        self.assertEqual(len(authoritative), 964)
        self.assertEqual(len(declared), 964)
        self.assertEqual(set(declared), set(authoritative))
        for path, classification in authoritative.items():
            declared_classification, destination = declared[path]
            self.assertEqual(declared_classification, classification, path)
            self.assertTrue(destination, path)
            self.assertEqual(
                GENERATOR._destination(path, classification)[0], destination, path
            )

        with self.assertRaisesRegex(AssertionError, "duplicate destination"):
            GENERATOR._build_destination_registry(
                (
                    ("observation.example", GENERATOR.MODEL_INPUT, "FlatGlobalsV1"),
                    ("observation.example", GENERATOR.MODEL_INPUT, "FlatGlobalsV1"),
                )
            )
        with self.assertRaisesRegex(AssertionError, "missing="):
            GENERATOR._assert_destination_coverage(
                authoritative, {k: v for k, v in declared.items() if k != next(iter(declared))}
            )
        with self.assertRaisesRegex(AssertionError, "extra="):
            GENERATOR._assert_destination_coverage(
                authoritative,
                {**declared, "observation.not_authoritative": (GENERATOR.MODEL_INPUT, "x")},
            )
        with self.assertRaisesRegex(AssertionError, "classification drift"):
            GENERATOR._destination(
                "observation.projection.life_totals.[]", GENERATOR.OPERATIONAL_ONLY
            )
        with self.assertRaisesRegex(AssertionError, "undeclared normalized semantic path"):
            GENERATOR._destination("observation.not_authoritative", GENERATOR.MODEL_INPUT)

    def test_fail_closed_registry_checks_survive_optimized_python(self) -> None:
        script = f"""
import importlib.util
from pathlib import Path
import sys

path = Path({str(GENERATOR_PATH)!r})
spec = importlib.util.spec_from_file_location('optimized_flat_policy_goldens', path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
sys.argv = [str(path), '--check']
if module.main() != 0:
    raise SystemExit('optimized generator check did not succeed')
try:
    module._build_destination_registry((
        ('observation.bad', module.MODEL_INPUT, 'FlatActionObjectV1.card_token'),
    ))
except AssertionError:
    pass
else:
    raise SystemExit('optimized registry accepted a private action-object destination')
try:
    module._validate_inventory_entries([
        {{'path': 'observation.bad', 'classification': module.FORBIDDEN,
          'destination': 'FlatGlobalsV1'}}
    ])
except AssertionError:
    pass
else:
    raise SystemExit('optimized inventory accepted a forbidden/destination mismatch')
"""
        env = os.environ.copy()
        python_root = str(ROOT / "python")
        env["PYTHONPATH"] = python_root + (
            os.pathsep + env["PYTHONPATH"] if env.get("PYTHONPATH") else ""
        )
        result = subprocess.run(
            [sys.executable, "-O", "-c", script],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            result.returncode,
            0,
            f"optimized registry check failed:\nstdout={result.stdout}\nstderr={result.stderr}",
        )

    def test_known_semantic_paths_name_their_concrete_typed_derivations(self) -> None:
        expected = {
            "observation.projection.life_totals.[]": "FlatGlobalsV1.players[].life",
            "observation.projection.battlefield.[].[].characteristics.effective_subtype_ids.[]": "FlatObjectSubtypeV1.subtype_id",
            "observation.projection.continuous_effects.[].power_delta": "FlatEffectRelationDataV1.power_delta",
            "<variant:options>.observation.projection.engine_context.pending_effect.choice.structural_path.[]": "FlatContextPathElementV1.value(kind=StructuralPath)",
            "<variant:object>.observation.projection.stack.[].targets.[].object.controller": "FlatStackRelationDataV1.target_object_controller",
            "observation.projection.engine_context.pending_triggers.[].controller": "FlatContextRelationDataV1.controller",
            "observation.projection.engine_context.pending_triggers.[].source.controller": "pending_trigger_source_relation_derivation",
            "<variant:choose_target>.legal_action.semantic.source.card_db_id": "FlatActionRefV1.card_token",
            "<variant:choose_target>.legal_action.semantic.source.owner": "FlatActionRefV1.object_index via FlatActionObjectV1.canonical_key",
            "<variant:order_triggers>.legal_action.semantic.order.[]": "FlatActionRefV1.associated_order",
            "<variant:pass>.legal_action.semantic.actor": "actor_relative_self_constant",
        }
        for path, destination in expected.items():
            self.assertEqual(GENERATOR.DESTINATION_REGISTRY[path][1], destination, path)

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
        mapping_contract = {
            "action_ref_role_crosswalk": goldens["action_ref_role_crosswalk"],
            "enum_maps": goldens["enum_maps"],
        }
        self.assertEqual(
            goldens["mapping_sha256"],
            hashlib.sha256(
                json.dumps(mapping_contract, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        )
        self.assertEqual(
            goldens["hand_authored_vectors"]["trigger_order_lengths"], list(range(8))
        )

    def test_action_ref_role_crosswalk_is_explicit_and_exhaustive(self) -> None:
        goldens = GENERATOR._goldens(GENERATOR._inventory())
        crosswalk = goldens["action_ref_role_crosswalk"]
        self.assertEqual(
            crosswalk,
            {
                "schema": "flat-policy-action-ref-role-crosswalk-v1",
                "mapping_version": 1,
                "rust_internal_width": 8,
                "python_projection_width": 10,
                "entries": [
                    {
                        "role": role,
                        "rust_internal_id": internal_id,
                        "python_projection_id": projection_id,
                    }
                    for internal_id, (role, projection_id) in enumerate(
                        [
                            ("source", 0),
                            ("candidate", 1),
                            ("card", 2),
                            ("attacker", 3),
                            ("blocker", 4),
                            ("target_object", 5),
                            ("cards", 6),
                            ("pending_sources", 9),
                        ]
                    )
                ],
                "projection_only": [
                    {"role": "attackers", "python_projection_id": 7},
                    {"role": "blockers", "python_projection_id": 8},
                ],
            },
        )
        self.assertEqual(
            goldens["enum_maps"]["action_ref_role"],
            {
                "source": 0,
                "candidate": 1,
                "card": 2,
                "attacker": 3,
                "blocker": 4,
                "target_object": 5,
                "cards": 6,
                "attackers": 7,
                "blockers": 8,
                "pending_sources": 9,
            },
        )


if __name__ == "__main__":
    unittest.main()
