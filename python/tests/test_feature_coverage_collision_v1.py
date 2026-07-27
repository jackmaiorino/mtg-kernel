from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "python" / "tools" / "audit_feature_coverage_collision_v1.py"
REPORT = (
    ROOT
    / "data"
    / "flat_policy_v2"
    / "feature_coverage_collision_audit_v1.json"
)


def load_tool():
    module_name = "_feature_coverage_collision_audit_v1_test"
    module_spec = importlib.util.spec_from_file_location(module_name, TOOL)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError("failed to load feature coverage/collision audit")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_name] = module
    try:
        module_spec.loader.exec_module(module)
    finally:
        sys.modules.pop(module_name, None)
    return module


class FeatureCoverageCollisionAuditV1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.audit = load_tool()

    def test_checked_report_matches_authorities_and_honest_status(self) -> None:
        report = self.audit.build_report()
        self.assertEqual(REPORT.read_bytes(), self.audit.pretty_json(report))
        self.assertEqual(
            report["schema"],
            "mtg-kernel-feature-coverage-collision-audit/v1",
        )
        self.assertEqual(report["status"], "COVERAGE-INCOMPLETE")
        self.assertEqual(report["decision"]["status"], "COVERAGE-INCOMPLETE")
        self.assertEqual(
            report["decision"]["precedence"],
            [
                "INVALID",
                "COLLISION-DETECTED",
                "COVERAGE-INCOMPLETE",
                "HASH-DEPENDENCE-CANDIDATE",
                "STRUCTURED-DISTINGUISHABLE",
            ],
        )
        self.assertEqual(
            report["corpus"],
            {"action_case_count": 115, "observation_case_count": 18},
        )
        for collision_level in report["collisions"].values():
            self.assertEqual(
                collision_level,
                {"action": [], "observation": []},
            )
        self.assertEqual(
            report["equivalences"]["observed_intentional_groups"],
            {
                "action": [
                    [
                        "actor-p0-relative-self",
                        "actor-p1-relative-self",
                        "metadata-invariance-a",
                        "metadata-invariance-b",
                    ],
                    [
                        "boolean-optional-use-true",
                        "primary-choose_optional_cost_use",
                    ],
                    [
                        "optional-choice-SacrificeLand",
                        "primary-choose_optional_cost_which",
                    ],
                ],
                "observation": [
                    ["burn-mirror-opening", "synthetic-actor-seat-swap"]
                ],
            },
        )
        observation = report["coverage"]["observation"]
        action = report["coverage"]["action"]
        self.assertEqual(
            (
                observation["covered_model_input_atoms"],
                observation["declared_model_input_atoms"],
            ),
            (342, 564),
        )
        self.assertEqual(
            (
                action["covered_model_input_atoms"],
                action["declared_model_input_atoms"],
            ),
            (200, 202),
        )
        self.assertFalse(observation["required_coverage_complete"])
        self.assertFalse(action["required_coverage_complete"])

    def test_variant_and_optional_atom_identities_do_not_collapse(self) -> None:
        features = self.audit._load_features_without_torch()
        atoms = self.audit._walk_declared_atoms(
            features,
            features.LEGAL_ACTION_SPEC,
            ("legal_action",),
        )
        ids = [atom.atom_id for atom in atoms]
        self.assertEqual(len(ids), len(set(ids)))
        self.assertIn(
            "legal_action.semantic.<action_kind=choose_kicker>.pay",
            ids,
        )
        self.assertIn(
            "legal_action.semantic.<action_kind=choose_spell_copy_payment>.pay",
            ids,
        )
        self.assertIn(
            "legal_action.semantic.<action_kind=activate_mana_ability>."
            "mana_choice.<present>",
            ids,
        )
        self.assertIn(
            "legal_action.semantic.<action_kind=activate_mana_ability>."
            "mana_choice",
            ids,
        )

    def test_distinct_canonical_collision_is_detected_at_each_key(self) -> None:
        for key in ("raw_digest", "quantized", "complete"):
            records = [
                {
                    "canonical": b'{"value":1}',
                    "name": "left",
                    key: b"same-signature",
                },
                {
                    "canonical": b'{"value":2}',
                    "name": "right",
                    key: b"same-signature",
                },
            ]
            groups = self.audit._collision_groups(records, key)
            self.assertEqual(len(groups), 1)
            self.assertEqual(groups[0]["cases"], ["left", "right"])

        intentional_equivalence = [
            {
                "canonical": b'{"value":1}',
                "name": "left",
                "raw_digest": b"same-signature",
            },
            {
                "canonical": b'{"value":1}',
                "name": "right",
                "raw_digest": b"same-signature",
            },
        ]
        self.assertEqual(
            self.audit._collision_groups(
                intentional_equivalence, "raw_digest"
            ),
            [],
        )

    def test_expected_equivalence_groups_are_fail_closed(self) -> None:
        observed = {
            scope: sorted([sorted(group) for group in groups])
            for scope, groups in self.audit.EXPECTED_EQUIVALENCE_GROUPS.items()
        }
        self.audit._validate_equivalence_groups(observed)
        changed = copy.deepcopy(observed)
        changed["action"].append(["unexpected-a", "unexpected-b"])
        with self.assertRaises(self.audit.AuditError):
            self.audit._validate_equivalence_groups(changed)

    def test_unconsumed_action_ref_card_ids_do_not_enter_representation_keys(
        self,
    ) -> None:
        features = self.audit._load_features_without_torch()
        full = self.audit._load_json_strict(self.audit.FULL_GOLDEN_PATH)
        observation = full["cases"][0]
        changed_observation = copy.deepcopy(observation)
        changed_observation["tensors"]["action_ref_card_ids"] = [987654321]
        dimensions = {
            "state_dim": features.STATE_FEATURE_DIM,
            "state_direct_dim": (
                features.STATE_FEATURE_DIM - features.STATE_HASH_DIM
            ),
            "action_dim": features.ACTION_FEATURE_DIM,
            "action_direct_dim": (
                features.ACTION_FEATURE_DIM - features.ACTION_HASH_DIM
            ),
        }
        self.assertEqual(
            self.audit._observation_representation_keys(
                observation,
                **dimensions,
            ),
            self.audit._observation_representation_keys(
                changed_observation,
                **dimensions,
            ),
        )

        actions = self.audit._load_json_strict(self.audit.ACTION_GOLDEN_PATH)
        action = actions["cases"][0]
        changed_action = copy.deepcopy(action)
        changed_action["action_ref_card_ids"] = [987654321]
        action_dimensions = {
            "action_dim": features.ACTION_FEATURE_DIM,
            "action_direct_dim": (
                features.ACTION_FEATURE_DIM - features.ACTION_HASH_DIM
            ),
        }
        self.assertEqual(
            self.audit._action_representation_keys(
                action,
                **action_dimensions,
            ),
            self.audit._action_representation_keys(
                changed_action,
                **action_dimensions,
            ),
        )

    def test_strict_json_and_payload_integrity_fail_closed(self) -> None:
        with self.assertRaises(self.audit.AuditError):
            self.audit._loads_json_strict(
                '{"duplicate":1,"duplicate":2}',
                label="duplicate-test",
            )
        with self.assertRaises(self.audit.AuditError):
            self.audit._loads_json_strict('{"value":NaN}', label="nan-test")

        action = self.audit._load_json_strict(self.audit.ACTION_GOLDEN_PATH)
        action["cases"][0]["name"] = "mutated-without-rehash"
        with self.assertRaises(self.audit.AuditError):
            self.audit._validate_payload_hash(
                action,
                label="mutated-action-golden",
            )

    def test_payload_sha256_covers_every_report_field(self) -> None:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        expected = report.pop("payload_sha256")
        self.assertEqual(
            self.audit._sha256(self.audit.canonical_bytes(report)),
            expected,
        )


if __name__ == "__main__":
    unittest.main()
