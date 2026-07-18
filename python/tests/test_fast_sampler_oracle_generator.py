from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = REPOSITORY_ROOT / "python" / "tools" / "generate_fast_sampler_oracle_v1.py"
SPEC = importlib.util.spec_from_file_location("fast_sampler_oracle_generator", GENERATOR_PATH)
if SPEC is None or SPEC.loader is None:  # pragma: no cover - import infrastructure guard
    raise RuntimeError("could not load fast sampler oracle generator")
generator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generator)


class FastSamplerOracleGeneratorTest(unittest.TestCase):
    def test_width_evidence_json_rejects_duplicates_and_nonfinite_numbers(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            generator.strict_json_loads(b'{"x":1,"x":2}')
        with self.assertRaisesRegex(ValueError, "nonfinite JSON number"):
            generator.strict_json_loads(b'{"x":NaN}')

    def test_repo_relative_evidence_paths_fail_closed(self) -> None:
        self.assertEqual(
            generator.parse_repo_relative_path(
                "data/rally_all_policy_legal_action_width_histogram_v1.json"
            ).as_posix(),
            "data/rally_all_policy_legal_action_width_histogram_v1.json",
        )
        for rejected in (
            "",
            "/absolute.json",
            "C:/outside.json",
            r"data\windows.json",
            "data/../outside.json",
            ".git/config",
        ):
            with self.subTest(rejected=rejected), self.assertRaises(ValueError):
                generator.parse_repo_relative_path(rejected)

    def test_provisional_profile_is_explicitly_nonclaiming(self) -> None:
        profile, histogram = generator.provisional_synthetic_width_profile()
        self.assertEqual(profile["status"], "provisional_synthetic")
        self.assertFalse(profile["claim_eligible"])
        self.assertEqual(
            profile["scope"], "provisional_synthetic_not_observed_policy_decisions"
        )
        self.assertIsNone(profile["source_artifact"])
        self.assertIsNone(profile["raw_source_artifact_size_bytes"])
        self.assertIsNone(profile["source_attestations_stable"])
        self.assertIsNone(profile["binary_attestations_stable"])
        self.assertIsNone(profile["formal_binary_source_attestation_present"])
        self.assertEqual(profile["final_all_nine_deck_gate"], "deferred")
        self.assertEqual(
            sum(count for _width, count in histogram), profile["statistics"]["sample_count"]
        )

    def test_observed_profile_is_all_policy_workload_only_provenance(self) -> None:
        profile, histogram = generator.observed_width_profile(
            "data/rally_all_policy_legal_action_width_histogram_v1.json"
        )
        self.assertEqual(profile["status"], "observed_provenance_bound")
        self.assertTrue(profile["claim_eligible"])
        self.assertEqual(
            profile["scope"],
            "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only",
        )
        self.assertEqual(profile["statistics"]["sample_count"], 2048)
        self.assertEqual(profile["statistics"]["mean"], 4.2109375)
        self.assertEqual(profile["statistics"]["nearest_rank_p95"], 9)
        self.assertEqual(profile["statistics"]["maximum"], 13)
        self.assertFalse(profile["source_performance_gate_valid"])
        self.assertFalse(profile["source_performance_rates_included"])
        self.assertEqual(
            profile["source_coverage_scope"],
            "rally_vs_rally_only_not_nine_deck_coverage",
        )
        self.assertEqual(profile["raw_source_artifact_size_bytes"], 64_453)
        self.assertEqual(profile["source_manifest_file_count"], 132)
        self.assertEqual(
            profile["source_status_sha256_before"], generator.EMPTY_SHA256
        )
        self.assertEqual(
            profile["source_status_sha256_after"], generator.EMPTY_SHA256
        )
        self.assertTrue(profile["source_attestations_stable"])
        self.assertTrue(profile["binary_attestations_stable"])
        self.assertFalse(profile["formal_binary_source_attestation_present"])
        self.assertFalse(profile["compiled_input_closure_attested"])
        self.assertEqual(sum(count for _width, count in histogram), 2048)

    def test_observed_profile_rejects_provenance_and_performance_tampering(self) -> None:
        source = generator.strict_json_loads(
            (
                REPOSITORY_ROOT
                / "data"
                / "rally_all_policy_legal_action_width_histogram_v1.json"
            ).read_bytes()
        )
        mutations = {
            "deck_sha_mislabel": lambda record: record["matchup"].update(
                {
                    "p0_deck_hash_mislabeled_sha256": "0" * 64,
                }
            ),
            "performance_promotion": lambda record: record.update(
                {"performance_gate_valid": True}
            ),
            "learner_only_scope": lambda record: record.update(
                {"legal_action_width_histogram_scope": "learner_only"}
            ),
            "raw_source_size_drift": lambda record: record.update(
                {"source_artifact_size_bytes": 64_452}
            ),
            "source_stability_promotion": lambda record: record.update(
                {"source_attestations_stable": False}
            ),
            "binary_stability_promotion": lambda record: record.update(
                {"binary_attestations_stable": False}
            ),
            "formal_source_claim_invented": lambda record: record.update(
                {"formal_binary_source_attestation_present": True}
            ),
            "compiled_input_closure_invented": lambda record: record.update(
                {"compiled_input_closure_attested": True}
            ),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name), tempfile.TemporaryDirectory() as temporary_name:
                artifact = copy.deepcopy(source)
                mutate(artifact["record"])
                artifact["aggregate_record_sha256"] = generator.sha256_hex(
                    generator.canonical_json_bytes(artifact["record"])
                )
                temporary_root = Path(temporary_name)
                path = temporary_root / "data" / "evidence.json"
                path.parent.mkdir()
                path.write_text(json.dumps(artifact), encoding="utf-8")
                with mock.patch.object(generator, "REPOSITORY_ROOT", temporary_root):
                    with self.assertRaises(ValueError):
                        generator.observed_width_profile("data/evidence.json")

    def test_histogram_requires_sorted_admitted_positive_counts_and_exact_total(self) -> None:
        self.assertEqual(
            generator.validate_histogram(
                [
                    {"width": 1, "policy_decision_count": 2},
                    {"width": 3, "policy_decision_count": 1},
                ],
                3,
            ),
            [(1, 2), (3, 1)],
        )
        rejected = (
            ([{"width": 0, "policy_decision_count": 1}], 1),
            ([{"width": 65, "policy_decision_count": 1}], 1),
            ([{"width": 1, "policy_decision_count": 0}], 0),
            (
                [
                    {"width": 2, "policy_decision_count": 1},
                    {"width": 1, "policy_decision_count": 1},
                ],
                2,
            ),
            ([{"width": 1, "policy_decision_count": 1}], 2),
        )
        for histogram, total in rejected:
            with self.subTest(histogram=histogram, total=total), self.assertRaises(ValueError):
                generator.validate_histogram(histogram, total)

    def test_python_rng_and_selection_goldens_have_recomputable_digests(self) -> None:
        cases = [
            {
                "name": "equal-two",
                "decimal_mass": [str(1 << 63), str(1 << 63)],
            }
        ]
        goldens = generator.independent_rng_and_selection_goldens(cases)
        draws = goldens["splitmix_first_draws"]
        draw_bytes = bytes.fromhex(draws["bytes_hex"])
        self.assertEqual(len(draw_bytes), 4096 * 8)
        self.assertEqual(generator.sha256_hex(draw_bytes), draws["sha256"])
        self.assertEqual(
            int.from_bytes(draw_bytes[:8], "little"), generator.splitmix64_first(0)
        )
        expected_selected = bytes(
            generator.select_mass(
                [1 << 63, 1 << 63], generator.splitmix64_first(seed)
            )
            for seed in range(4096)
        )
        self.assertEqual(
            generator.sha256_hex(expected_selected),
            goldens["decimal_selected_indices"]["sha256"],
        )

    def test_tracked_fixture_binds_observed_workload_only_provenance(self) -> None:
        fixture_path = REPOSITORY_ROOT / generator.OUTPUT_RELATIVE
        fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
        self.assertEqual(fixture["schema_version"], 2)
        self.assertEqual(
            fixture["workload_width_profile"]["status"], "observed_provenance_bound"
        )
        self.assertTrue(fixture["workload_width_profile"]["claim_eligible"])
        self.assertEqual(
            fixture["workload_width_profile"]["scope"],
            "all_sampled_policy_decisions_in_rally_vs_rally_not_learner_only",
        )
        self.assertFalse(
            fixture["workload_width_profile"]["source_performance_gate_valid"]
        )
        self.assertFalse(
            fixture["workload_width_profile"]["source_performance_rates_included"]
        )
        self.assertEqual(
            fixture["workload_width_profile"]["source_coverage_scope"],
            "rally_vs_rally_only_not_nine_deck_coverage",
        )
        self.assertEqual(
            fixture["workload_width_profile"]["raw_source_artifact_size_bytes"],
            64_453,
        )
        self.assertEqual(
            fixture["workload_width_profile"]["source_manifest_file_count"], 132
        )
        self.assertTrue(
            fixture["workload_width_profile"]["source_attestations_stable"]
        )
        self.assertTrue(
            fixture["workload_width_profile"]["binary_attestations_stable"]
        )
        self.assertFalse(
            fixture["workload_width_profile"][
                "formal_binary_source_attestation_present"
            ]
        )
        self.assertFalse(
            fixture["workload_width_profile"]["compiled_input_closure_attested"]
        )
        self.assertIn("predeclared_candidate_bounds", fixture)

    def test_generator_success_path_is_repo_relative(self) -> None:
        self.assertFalse(generator.OUTPUT_RELATIVE.is_absolute())
        self.assertEqual(
            generator.OUTPUT_RELATIVE.as_posix(),
            "data/fast_sampler_decimal_oracle_v1.json",
        )


if __name__ == "__main__":
    unittest.main()
