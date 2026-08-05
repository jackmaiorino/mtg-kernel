from __future__ import annotations

import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("select_checkpoint_v4.py")
SPEC = importlib.util.spec_from_file_location("select_checkpoint_v4", MODULE_PATH)
SELECTOR = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(SELECTOR)


class SelectorV4Tests(unittest.TestCase):
    def corpus(self) -> dict:
        return {
            "path": "corpus.jsonl",
            "sha256": SELECTOR.CORPUS_SHA256,
            "base_seed_u64_hex": SELECTOR.CORPUS_BASE_SEED_HEX,
            "pair_indices": SELECTOR.CORPUS_PAIR_INDICES,
            "pair_count": SELECTOR.CORPUS_PAIR_COUNT,
            "episode_count": SELECTOR.CORPUS_EPISODE_COUNT,
            "decision_row_count": SELECTOR.CORPUS_DECISION_ROW_COUNT,
            "physical_group_count": SELECTOR.CORPUS_PHYSICAL_GROUP_COUNT,
            "terminal_return_counts_loss_draw_win": SELECTOR.CORPUS_TERMINAL_RETURN_COUNTS,
        }

    def checkpoint(self, update: int, mean_tv: float, eligible: bool = True) -> dict:
        max_log = 0.5 if eligible else 1.1
        movement = {
            "minimum_likelihood_ratio": 0.8,
            "maximum_likelihood_ratio": 1.2,
            "mean_likelihood_ratio": 1.0,
            "mean_absolute_log_likelihood_ratio": 0.02,
            "maximum_absolute_joint_log_likelihood_ratio": max_log,
            "mean_old_to_current_forward_kl": 0.001,
            "mean_action_total_variation": mean_tv,
            "p90_action_total_variation_nearest_rank": 0.04,
            "mean_policy_surrogate": 0.001,
        }
        tail = {
            "p99_action_total_variation_nearest_rank": 0.08,
            "maximum_action_total_variation": 0.10,
            "p99_absolute_joint_log_likelihood_ratio_nearest_rank": 0.5,
            "above_absolute_joint_log_ratio_1_count": 0,
            "worst_group": {
                "signed_joint_log_likelihood_ratio": 0.5,
                "absolute_joint_log_likelihood_ratio": 0.5,
                "likelihood_ratio": 1.6487212707,
            },
        }
        return {
            "update": update,
            "adam_step": update,
            "parameter_l2_from_gae8": 0.3,
            "movement": movement,
            "tail_shape": tail,
            "gate": {"finite": True, "pass": eligible},
        }

    def make_arm(
        self,
        root: Path,
        name: str,
        mean_tvs: list[float],
        eligible: list[bool] | None = None,
    ) -> tuple[Path, Path]:
        if eligible is None:
            eligible = [True] * 4
        checkpoints = [
            self.checkpoint(update, mean_tv, passes)
            for update, mean_tv, passes in zip(
                SELECTOR.CHECKPOINT_UPDATES, mean_tvs, eligible
            )
        ]
        passing = [row for row in checkpoints if row["gate"]["pass"]]
        selected = (
            None
            if not passing
            else max(
                passing,
                key=lambda row: (
                    row["movement"]["mean_action_total_variation"],
                    -row["update"],
                ),
            )
        )
        report_path = root / f"{name}.report.json"
        candidate_root = root / f"{name}.candidate"
        candidate = None
        if selected is not None:
            payload_bytes = f"payload-{name}-update-{selected['update']}".encode()
            payload_sha = hashlib.sha256(payload_bytes).hexdigest()
            candidate = {
                "selected_update": selected["update"],
                "payload_byte_count": len(payload_bytes),
                "payload_sha256": payload_sha,
                "parameters_sha256": "1" * 64,
                "first_moments_sha256": "2" * 64,
                "second_moments_sha256": "3" * 64,
                "model_parameter_sha256": "4" * 64,
                "native_state_sha256": "5" * 64,
                "adam_step": selected["update"],
                "scorer_bias_anchor_f32_bits": 123,
                "parameter_l2_from_gae8": selected["parameter_l2_from_gae8"],
                "movement": selected["movement"],
                "tail_shape": selected["tail_shape"],
            }
        contract = SELECTOR.ARMS[name]
        report = {
            "schema": contract["report_schema"],
            "source": SELECTOR.SOURCE,
            "corpus": self.corpus(),
            "training": {
                "reward": "natural_terminal_win_draw_loss_only",
                "objective": SELECTOR.OBJECTIVE,
                "value_coefficient_f32_bits": SELECTOR.f32_bits(0.0),
                "ppo_clip_epsilon_f32_bits": SELECTOR.f32_bits(SELECTOR.PPO_CLIP),
                "learning_rate_schedule_f32_bits": [
                    SELECTOR.f32_bits(value) for value in SELECTOR.LEARNING_RATES
                ],
                "checkpoint_updates": SELECTOR.CHECKPOINT_UPDATES,
                "advantage_transform": {"identity": SELECTOR.ADVANTAGE_IDENTITY},
                "optimizer_reset": {
                    "source_adam_step": SELECTOR.SOURCE["adam_step"],
                    "starting_adam_step": 0,
                    "parameters_preserved_bit_exact": True,
                    "first_and_second_moments_all_positive_zero": True,
                },
                "policy_anchor": {
                    "identity": SELECTOR.ANCHOR_IDENTITY,
                    "direction": "old_to_current",
                    "old_policy_source": SELECTOR.ANCHOR_SOURCE,
                    "scope": "every_legal_action_distribution",
                    "coefficient": contract["beta"],
                    "coefficient_f32_bits": contract["beta_f32_bits"],
                },
                "updates": [
                    {
                        "update": update,
                        "learning_rate_f32_bits": SELECTOR.f32_bits(rate),
                        "adam_step_before": update - 1,
                        "adam_step_after": update,
                    }
                    for update, rate in enumerate(SELECTOR.LEARNING_RATES, 1)
                ],
            },
            "checkpoints": checkpoints,
            "candidate": candidate,
            "arm_eligibility_pass": candidate is not None,
        }
        report_path.write_text(json.dumps(report), encoding="utf-8")
        if candidate is not None:
            candidate_root.mkdir()
            payload_path = candidate_root / SELECTOR.PAYLOAD_FILENAME
            payload_path.write_bytes(payload_bytes)
            package_manifest = {
                "schema": SELECTOR.PACKAGE_SCHEMA,
                "authority_kind": contract["candidate_authority"],
                "source_result_sha256": SELECTOR.sha256(report_path),
                "payload": {
                    "filename": SELECTOR.PAYLOAD_FILENAME,
                    "byte_count": candidate["payload_byte_count"],
                    "adam_step": candidate["adam_step"],
                    "scorer_bias_anchor_f32_bits": 123,
                    "payload_sha256": candidate["payload_sha256"],
                    "parameters_sha256": candidate["parameters_sha256"],
                    "first_moments_sha256": candidate["first_moments_sha256"],
                    "second_moments_sha256": candidate["second_moments_sha256"],
                    "model_parameter_sha256": candidate["model_parameter_sha256"],
                    "native_state_sha256": candidate["native_state_sha256"],
                },
                "non_claims": SELECTOR.PACKAGE_NON_CLAIMS,
            }
            (candidate_root / SELECTOR.PACKAGE_MANIFEST_FILENAME).write_text(
                json.dumps(package_manifest), encoding="utf-8"
            )
        return report_path, candidate_root

    def make_manifest(
        self,
        root: Path,
        arm_paths: dict[str, tuple[Path, Path]],
        selection_report: Path,
        final_package: Path,
    ) -> Path:
        manifest = {
            "schema": SELECTOR.MANIFEST_SCHEMA,
            "git_commit": "a" * 40,
            "toolchain": {"rustc": "rustc test", "cargo": "cargo test", "linker": "link test"},
            "inputs": {"source": SELECTOR.SOURCE, "corpus": self.corpus()},
            "common_training": {
                "reward": "natural_terminal_win_draw_loss_only",
                "objective": SELECTOR.OBJECTIVE,
                "value_coefficient_f32_bits": SELECTOR.f32_bits(0.0),
                "ppo_clip_epsilon_f32_bits": SELECTOR.f32_bits(SELECTOR.PPO_CLIP),
                "learning_rate_schedule_f32_bits": [
                    SELECTOR.f32_bits(value) for value in SELECTOR.LEARNING_RATES
                ],
                "checkpoint_updates": SELECTOR.CHECKPOINT_UPDATES,
                "optimizer_starting_adam_step": 0,
                "policy_anchor_identity": SELECTOR.ANCHOR_IDENTITY,
                "policy_anchor_source": SELECTOR.ANCHOR_SOURCE,
            },
            "selection": {
                "basis": "movement-only",
                "order": "eligible-only-then-higher-mean-tv-then-higher-beta-then-earlier-update",
                "trusted_metrics": "rust-arm-reports",
                "gate_decision_recomputed": True,
            },
            "arms": [
                {
                    "name": name,
                    "beta_f32_bits": SELECTOR.ARMS[name]["beta_f32_bits"],
                    "report_path": str(arm_paths[name][0]),
                    "candidate_package_root": str(arm_paths[name][1]),
                }
                for name in SELECTOR.ARM_ORDER
            ],
            "outputs": {
                "selection_report_path": str(selection_report),
                "final_package_root": str(final_package),
            },
        }
        path = root / "manifest.json"
        path.write_text(json.dumps(manifest), encoding="utf-8")
        return path

    def test_global_tie_selects_larger_beta_and_only_final_is_runnable(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arms = {
                "beta-0.3": self.make_arm(root, "beta-0.3", [0.012, 0.020, 0.018, 0.017]),
                "beta-1.0": self.make_arm(root, "beta-1.0", [0.020, 0.019, 0.018, 0.017]),
            }
            selection_report = root / "selection.json"
            final_package = root / "final"
            manifest = self.make_manifest(root, arms, selection_report, final_package)
            summary = SELECTOR.authorize(
                manifest,
                {name: paths[0] for name, paths in arms.items()},
                {name: paths[1] for name, paths in arms.items()},
                selection_report,
                final_package,
            )
            self.assertEqual(summary["selected_arm"], "beta-1.0")
            self.assertEqual(summary["selected_update"], 2)
            final_manifest_path = final_package / SELECTOR.PACKAGE_MANIFEST_FILENAME
            final_manifest_text = final_manifest_path.read_text()
            final_manifest = json.loads(final_manifest_text)
            selected_candidate_manifest = json.loads(
                (arms["beta-1.0"][1] / SELECTOR.PACKAGE_MANIFEST_FILENAME).read_text()
            )
            self.assertEqual(
                list(final_manifest),
                list(selected_candidate_manifest),
            )
            self.assertEqual(
                list(final_manifest["payload"]),
                list(selected_candidate_manifest["payload"]),
            )
            self.assertEqual(
                final_manifest_text,
                json.dumps(final_manifest, indent=2, sort_keys=False, allow_nan=False) + "\n",
            )
            self.assertEqual(
                final_manifest["authority_kind"],
                SELECTOR.ARMS["beta-1.0"]["final_authority"],
            )
            for name, (_, candidate_root) in arms.items():
                candidate_manifest = json.loads(
                    (candidate_root / SELECTOR.PACKAGE_MANIFEST_FILENAME).read_text()
                )
                self.assertEqual(
                    candidate_manifest["authority_kind"],
                    SELECTOR.ARMS[name]["candidate_authority"],
                )

    def test_earlier_local_checkpoint_payload_is_published(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arms = {
                "beta-0.3": self.make_arm(root, "beta-0.3", [0.012, 0.025, 0.020, 0.018]),
                "beta-1.0": self.make_arm(root, "beta-1.0", [0.011, 0.012, 0.013, 0.014]),
            }
            selection_report = root / "selection.json"
            final_package = root / "final"
            manifest = self.make_manifest(root, arms, selection_report, final_package)
            summary = SELECTOR.authorize(
                manifest,
                {name: paths[0] for name, paths in arms.items()},
                {name: paths[1] for name, paths in arms.items()},
                selection_report,
                final_package,
            )
            self.assertEqual((summary["selected_arm"], summary["selected_update"]), ("beta-0.3", 4))
            self.assertEqual(
                (final_package / SELECTOR.PAYLOAD_FILENAME).read_bytes(),
                b"payload-beta-0.3-update-4",
            )

    def test_both_ineligible_stops_without_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arms = {
                name: self.make_arm(root, name, [0.012] * 4, [False] * 4)
                for name in SELECTOR.ARM_ORDER
            }
            selection_report = root / "selection.json"
            final_package = root / "final"
            manifest = self.make_manifest(root, arms, selection_report, final_package)
            summary = SELECTOR.authorize(
                manifest,
                {name: paths[0] for name, paths in arms.items()},
                {name: paths[1] for name, paths in arms.items()},
                selection_report,
                final_package,
            )
            self.assertFalse(summary["publication_pass"])
            self.assertIsNone(summary["selected_arm"])
            self.assertFalse(final_package.exists())

    def test_gate_disagreement_and_candidate_authority_tamper_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path, candidate_root = self.make_arm(
                root, "beta-0.3", [0.012, 0.020, 0.018, 0.017]
            )
            report = json.loads(report_path.read_text())
            report["checkpoints"][1]["gate"]["pass"] = False
            report_path.write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "gate disagreement"):
                SELECTOR.validate_arm("beta-0.3", report_path, candidate_root, self.corpus())

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path, candidate_root = self.make_arm(
                root, "beta-1.0", [0.012, 0.020, 0.018, 0.017]
            )
            manifest_path = candidate_root / SELECTOR.PACKAGE_MANIFEST_FILENAME
            manifest = json.loads(manifest_path.read_text())
            manifest["authority_kind"] = SELECTOR.ARMS["beta-1.0"]["final_authority"]
            manifest_path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "candidate package binding mismatch"):
                SELECTOR.validate_arm("beta-1.0", report_path, candidate_root, self.corpus())

    def test_worst_group_nonfinite_or_absent_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path, candidate_root = self.make_arm(
                root, "beta-0.3", [0.012, 0.020, 0.018, 0.017]
            )
            report = json.loads(report_path.read_text())
            report["checkpoints"][1]["tail_shape"]["worst_group"]["likelihood_ratio"] = None
            report_path.write_text(json.dumps(report))
            with self.assertRaisesRegex(ValueError, "gate disagreement"):
                SELECTOR.validate_arm("beta-0.3", report_path, candidate_root, self.corpus())

    def test_manifest_rejects_exact_corpus_pin_tamper(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            arms = {
                name: self.make_arm(root, name, [0.012, 0.020, 0.018, 0.017])
                for name in SELECTOR.ARM_ORDER
            }
            selection_report = root / "selection.json"
            final_package = root / "final"
            manifest_path = self.make_manifest(root, arms, selection_report, final_package)
            manifest = json.loads(manifest_path.read_text())
            manifest["inputs"]["corpus"]["physical_group_count"] += 1
            manifest_path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "corpus panel mismatch"):
                SELECTOR.validate_manifest(
                    manifest_path,
                    {name: paths[0] for name, paths in arms.items()},
                    {name: paths[1] for name, paths in arms.items()},
                    selection_report,
                    final_package,
                )


if __name__ == "__main__":
    unittest.main()
