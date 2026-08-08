from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[2]
TOOL = (
    ROOT
    / "python"
    / "tools"
    / "audit_action_ingress_static_admission_v1.py"
)
REPORT = (
    ROOT
    / "data"
    / "action_ingress_admission_v1"
    / "static-admission.json"
)


def load_tool():
    module_name = "_action_ingress_static_admission_v1_test"
    module_spec = importlib.util.spec_from_file_location(module_name, TOOL)
    if module_spec is None or module_spec.loader is None:
        raise AssertionError("failed to load action-ingress static audit")
    module = importlib.util.module_from_spec(module_spec)
    sys.modules[module_name] = module
    try:
        module_spec.loader.exec_module(module)
    finally:
        sys.modules.pop(module_name, None)
    return module


class ActionIngressStaticAdmissionV1Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tool = load_tool()
        cls.audit = cls.tool._load_audit_module()

    def test_checked_report_is_exact_and_admitted(self) -> None:
        report = self.tool.build_report()
        self.assertEqual(REPORT.read_bytes(), self.audit.pretty_json(report))
        self.assertEqual(
            report["schema"],
            "mtg-kernel-action-ingress-static-admission/v1",
        )
        self.assertEqual(report["status"], "STATIC-ADMITTED")
        self.assertEqual(report["decision"]["status"], "STATIC-ADMITTED")
        self.assertEqual(
            report["decision"]["precedence"],
            [
                "INVALID",
                "COLLISION-DETECTED",
                "COVERAGE-INCOMPLETE",
                "STRUCTURED-ALIAS-DETECTED",
                "INVARIANT-FAILED",
                "STATIC-ADMITTED",
            ],
        )
        self.assertTrue(all(report["admission_criteria"].values()))
        self.assertEqual(
            report["frozen_no_repair"],
            {"byte_identical_case_count": 115, "case_count": 115},
        )

    def test_combined_identity_coverage_and_equivalences_are_exact(self) -> None:
        report = self.tool.build_report()
        corpus = report["corpus"]
        self.assertEqual(
            (
                corpus["frozen_case_count"],
                corpus["supplemental_case_count"],
                corpus["combined_case_count"],
            ),
            (115, 1, 116),
        )
        self.assertEqual(
            corpus["combined_identity"],
            "net8-action-ingress-static-checked-116-v1",
        )
        expected_identity_sha256 = (
            "7a8cc702393253fdb2dfe61bcca648cbc"
            "8684cd1527e0d30a37b242baeb20a6e"
        )
        self.assertEqual(
            self.tool.EXPECTED_COMBINED_IDENTITY_SHA256,
            expected_identity_sha256,
        )
        self.assertEqual(
            corpus["expected_combined_identity_sha256"],
            expected_identity_sha256,
        )
        self.assertEqual(
            corpus["combined_identity_sha256"],
            expected_identity_sha256,
        )
        self.assertEqual(
            self.audit._sha256(
                self.audit.canonical_bytes(corpus["case_identity_rows"])
            ),
            corpus["combined_identity_sha256"],
        )
        with mock.patch.object(
            self.tool,
            "EXPECTED_COMBINED_IDENTITY_SHA256",
            "0" * 64,
        ):
            with self.assertRaisesRegex(
                self.tool.AdmissionError,
                "combined 116-case identity SHA-256 mismatch",
            ):
                self.tool.build_report()
        coverage = report["coverage"]["augmented_116_case"]
        self.assertEqual(
            (
                coverage["covered_model_input_atoms"],
                coverage["declared_model_input_atoms"],
            ),
            (202, 202),
        )
        self.assertEqual(coverage["unwitnessed_model_input_atoms"], [])
        self.assertEqual(coverage["boolean_polarity_gaps"], [])
        self.assertEqual(coverage["optional_presence_gaps"], [])
        self.assertTrue(coverage["required_coverage_complete"])
        self.assertEqual(
            report["canonical_equivalences"]["observed_groups"],
            report["canonical_equivalences"]["expected_groups"],
        )
        self.assertEqual(
            report["canonical_equivalences"]["unexpected_groups"],
            [],
        )

    def test_supplemental_case_is_the_predeclared_frozen_encoder_read(self) -> None:
        report = self.tool.build_report()
        supplemental = report["supplemental_case"]
        self.assertEqual(
            supplemental["name"],
            "supplemental-primary-choose-effect-target-player-self-v1",
        )
        canonical = json.loads(supplemental["canonical_json"])
        semantic = canonical["semantic"]
        self.assertEqual(semantic["action_kind"], "choose_effect_target")
        self.assertEqual(semantic["actor"], "self")
        self.assertEqual(
            semantic["source"],
            {
                "card_db_id": 40,
                "controller": "self",
                "owner": "self",
                "zone": "Hand",
            },
        )
        self.assertEqual(semantic["target"], {
            "player": "self",
            "target_kind": "player",
        })
        self.assertEqual(
            (
                semantic["selected_count"],
                semantic["min_targets"],
                semantic["max_targets"],
            ),
            (1, 1, 3),
        )
        core = supplemental["flat_input"]["core"]
        self.assertEqual(
            (core["target_kind"], core["target_player"], core["ref_len"]),
            (1, 1, 1),
        )
        self.assertEqual(len(supplemental["flat_input"]["objects"]), 1)
        self.assertEqual(len(supplemental["flat_input"]["refs"]), 1)
        tensors = supplemental["tensors"]
        self.assertEqual(tensors["action_features"]["shape"], [195])
        self.assertEqual(tensors["action_ref_features"]["shape"], [1, 25])
        self.assertEqual(
            tensors["action_ref_card_ids"],
            {"shape": [1], "u32_values": [41]},
        )
        self.assertEqual(
            tensors["action_ref_node_indices"],
            {"shape": [1], "u32_values": [0]},
        )
        canonical_bytes = self.audit.canonical_bytes(canonical)
        blocks = self.audit._digest_blocks("legal-action", canonical_bytes)
        self.assertEqual(
            supplemental["digests"]["sha512_blocks_hex"],
            [block.hex() for block in blocks],
        )
        action_bytes = bytes.fromhex(
            tensors["action_features"]["f32_le_hex"]
        )
        self.assertEqual(
            action_bytes[99 * 4 :],
            self.audit._digest_f32_tail("legal-action", canonical_bytes),
        )

    def test_repair_removes_only_the_known_combat_boolean_aliases(self) -> None:
        report = self.tool.build_report()
        self.assertEqual(
            [row["cases"] for row in report["structured_aliases"]["frozen_expected"]],
            [
                ["boolean-attacker-false", "boolean-attacker-true"],
                ["boolean-blocker-false", "boolean-blocker-true"],
            ],
        )
        self.assertEqual(report["structured_aliases"]["repaired"], [])
        for groups in report["collisions"].values():
            self.assertEqual(groups, [])
        for pair in report["combat_boolean_repair"]["pairs"]:
            self.assertTrue(pair["invariants_passed"])
            self.assertTrue(pair["frozen_direct_prefix_equal"])
            self.assertEqual(
                pair["repaired_direct_differing_coordinates"],
                [69],
            )
            self.assertEqual(pair["false_case_changed_coordinates"], [])
            self.assertEqual(pair["true_case_changed_coordinates"], [69])
            self.assertEqual(pair["slot69_false_f32_le_hex"], "00000000")
            self.assertEqual(pair["slot69_true_f32_le_hex"], "0000803f")
            self.assertTrue(all(pair["preservation"].values()))
        self.assertEqual(
            report["combat_boolean_repair"]["case_mutations"],
            [
                {"changed_coordinates": [69], "name": "boolean-attacker-true"},
                {"changed_coordinates": [69], "name": "boolean-blocker-true"},
                {
                    "changed_coordinates": [69],
                    "name": "primary-choose_attacker_inclusion",
                },
                {
                    "changed_coordinates": [69],
                    "name": "primary-choose_blocker_inclusion",
                },
            ],
        )
        for row in report["corpus"]["case_identity_rows"]:
            changed = row["name"] in {
                "boolean-attacker-true",
                "boolean-blocker-true",
                "primary-choose_attacker_inclusion",
                "primary-choose_blocker_inclusion",
            }
            self.assertEqual(
                row["frozen_action_features_sha256"]
                != row["repaired_action_features_sha256"],
                changed,
            )

    def test_adapter_and_status_logic_fail_closed(self) -> None:
        canonical = {
            "semantic": {
                "action_kind": "choose_attacker_inclusion",
                "include": True,
            }
        }
        frozen = bytes(195 * 4)
        self.assertIsNot(
            self.tool.adapt_action_features(frozen, canonical, repair=False),
            frozen,
        )
        self.assertEqual(
            self.tool.adapt_action_features(frozen, canonical, repair=False),
            frozen,
        )
        repaired = self.tool.adapt_action_features(
            frozen,
            canonical,
            repair=True,
        )
        self.assertEqual(
            [index for index in range(195)
             if repaired[index * 4 : index * 4 + 4]
             != frozen[index * 4 : index * 4 + 4]],
            [69],
        )
        with self.assertRaises(self.tool.AdmissionError):
            self.tool.adapt_action_features(
                bytes(194 * 4),
                canonical,
                repair=True,
            )
        malformed = copy.deepcopy(canonical)
        malformed["semantic"]["include"] = 1
        with self.assertRaises(self.tool.AdmissionError):
            self.tool.adapt_action_features(
                frozen,
                malformed,
                repair=True,
            )
        self.assertEqual(
            self.tool._decision_status(
                collision_count=1,
                coverage_admitted=False,
                repaired_alias_count=1,
                invariants_passed=False,
            ),
            "COLLISION-DETECTED",
        )
        self.assertEqual(
            self.tool._decision_status(
                collision_count=0,
                coverage_admitted=False,
                repaired_alias_count=1,
                invariants_passed=False,
            ),
            "COVERAGE-INCOMPLETE",
        )
        self.assertEqual(
            self.tool._decision_status(
                collision_count=0,
                coverage_admitted=True,
                repaired_alias_count=1,
                invariants_passed=False,
            ),
            "STRUCTURED-ALIAS-DETECTED",
        )

    def test_strict_json_authority_and_payload_integrity_fail_closed(self) -> None:
        with self.assertRaises(self.audit.AuditError):
            self.audit._loads_json_strict(
                '{"duplicate":1,"duplicate":2}',
                label="duplicate-test",
            )
        with self.assertRaises(self.audit.AuditError):
            self.audit._loads_json_strict(
                '{"value":NaN}',
                label="nan-test",
            )
        action = self.audit._load_json_strict(self.tool.ACTION_GOLDEN_PATH)
        action["cases"][0]["name"] = "mutated-without-rehash"
        with self.assertRaises(self.audit.AuditError):
            self.audit._validate_payload_hash(
                action,
                label="mutated-action-golden",
            )
        with tempfile.TemporaryDirectory() as temp_dir:
            copied = Path(temp_dir) / "action.json"
            copied.write_bytes(self.tool.ACTION_GOLDEN_PATH.read_bytes())
            with self.assertRaises(self.tool.AdmissionError):
                self.tool.build_report(action_path=copied)

    def test_payload_sha256_covers_every_report_field_and_cli_check_passes(
        self,
    ) -> None:
        report = json.loads(REPORT.read_text(encoding="utf-8"))
        expected = report.pop("payload_sha256")
        self.assertEqual(
            self.audit._sha256(self.audit.canonical_bytes(report)),
            expected,
        )
        completed = subprocess.run(
            [sys.executable, str(TOOL), "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"static admission check failed:\n"
            f"stdout={completed.stdout}\nstderr={completed.stderr}",
        )
        self.assertIn(
            "ACTION_INGRESS_STATIC_ADMISSION_V1_OK",
            completed.stdout,
        )


if __name__ == "__main__":
    unittest.main()
