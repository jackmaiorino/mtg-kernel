import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("select_anchor_arm_v1.py")
SPEC = importlib.util.spec_from_file_location("select_anchor_arm_v1", MODULE_PATH)
SELECTOR = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(SELECTOR)


class SelectorTests(unittest.TestCase):
    def make_report(self, root: Path, name: str, mean_tv: float, eligible: bool = True):
        contract = SELECTOR.ARMS[name]
        report_path = root / f"{name}.json"
        package_root = root / f"{name}-package"
        payload = f"payload-{name}".encode()
        payload_sha = hashlib.sha256(payload).hexdigest()
        p99_log = 0.70 if eligible else 0.80
        max_log = 0.90 if eligible else 1.10
        above_count = 0 if eligible else 1
        report = {
            "schema": contract["schema"],
            "source": {
                "authority_kind": SELECTOR.SOURCE_AUTHORITY,
                "source_result_sha256": SELECTOR.SOURCE_RESULT_SHA256,
                "manifest_sha256": SELECTOR.SOURCE_MANIFEST_SHA256,
                "payload_sha256": SELECTOR.SOURCE_PAYLOAD_SHA256,
                "native_state_sha256": SELECTOR.SOURCE_NATIVE_STATE_SHA256,
                "model_parameter_sha256": SELECTOR.SOURCE_MODEL_PARAMETER_SHA256,
                "adam_step": SELECTOR.SOURCE_ADAM_STEP,
            },
            "corpus": {
                "sha256": SELECTOR.CORPUS_SHA256,
                "pair_indices": list(range(64)),
                "pair_count": 64,
                "episode_count": 128,
                "decision_row_count": 4769,
                "physical_group_count": 4173,
                "terminal_return_counts_loss_draw_win": [80, 0, 48],
            },
            "training": {
                "reward": "natural_terminal_win_draw_loss_only",
                "objective": SELECTOR.OBJECTIVE,
                "learning_rate_f32_bits": SELECTOR.LEARNING_RATE_F32_BITS,
                "value_coefficient_f32_bits": 0,
                "ppo_clip_epsilon_f32_bits": SELECTOR.PPO_CLIP_F32_BITS,
                "epochs": 4,
                "starting_adam_step": 520,
                "ending_adam_step": 524,
                "advantage_transform": {
                    "identity": "terminal_reinforce_frozen_source_value_standardized_by_candidate_seat_episode_balanced/v1"
                },
                "policy_anchor": {
                    "identity": SELECTOR.ANCHOR_IDENTITY,
                    "direction": SELECTOR.ANCHOR_DIRECTION,
                    "old_policy_source": SELECTOR.ANCHOR_SOURCE,
                    "scope": SELECTOR.ANCHOR_SCOPE,
                    "coefficient": contract["beta"],
                    "coefficient_f32_bits": contract["beta_f32_bits"],
                },
            },
            "candidate": {
                "adam_step": 524,
                "payload_byte_count": len(payload),
                "scorer_bias_anchor_f32_bits": 123,
                "payload_sha256": payload_sha,
                "parameters_sha256": "3" * 64,
                "first_moments_sha256": "4" * 64,
                "second_moments_sha256": "5" * 64,
                "native_state_sha256": "1" * 64,
                "model_parameter_sha256": "2" * 64,
                "parameter_l2_from_gae8": 0.3,
                "movement": {
                    "minimum_likelihood_ratio": 0.8,
                    "maximum_likelihood_ratio": 1.2,
                    "mean_likelihood_ratio": 1.0,
                    "mean_absolute_log_likelihood_ratio": 0.02,
                    "mean_action_total_variation": mean_tv,
                    "p90_action_total_variation_nearest_rank": 0.03,
                    "maximum_absolute_joint_log_likelihood_ratio": max_log,
                    "mean_old_to_current_forward_kl": 0.002,
                    "mean_policy_surrogate": 0.001,
                },
                "tail_shape": {
                    "p99_action_total_variation_nearest_rank": 0.10,
                    "maximum_action_total_variation": 0.20,
                    "p99_absolute_joint_log_likelihood_ratio_nearest_rank": p99_log,
                    "above_absolute_joint_log_ratio_1_count": above_count,
                },
                "p99_action_total_variation": 0.10,
                "maximum_action_total_variation": 0.20,
                "p99_absolute_joint_log_likelihood_ratio": p99_log,
                "above_absolute_joint_log_ratio_1_count": above_count,
            },
            "publication_gate": {
                "finite": True,
                "parameter_l2_cap": 0.75,
                "mean_action_total_variation_floor": 0.010,
                "mean_action_total_variation_cap": 0.050,
                "p90_action_total_variation_cap": 0.150,
                "p99_action_total_variation_cap": 0.150,
                "p99_absolute_joint_log_likelihood_ratio_cap": 0.75,
                "maximum_absolute_joint_log_likelihood_ratio_cap": 1.0,
                "above_absolute_joint_log_ratio_1_count_cap": 0,
                "v3_tail_caps_required": True,
                "pass": eligible,
            },
        }
        report_path.write_text(json.dumps(report), encoding="utf-8")
        if eligible:
            package_root.mkdir()
            payload_path = package_root / "checkpoint.state.f32le"
            payload_path.write_bytes(payload)
            package_manifest = {
                "schema": "mtg-kernel-xmage-fixed-native-state/v1",
                "authority_kind": contract["authority"],
                "source_result_sha256": SELECTOR.sha256(report_path),
                "payload": {
                    "filename": "checkpoint.state.f32le",
                    "byte_count": len(payload),
                    "adam_step": 524,
                    "scorer_bias_anchor_f32_bits": 123,
                    "payload_sha256": payload_sha,
                    "parameters_sha256": "3" * 64,
                    "first_moments_sha256": "4" * 64,
                    "second_moments_sha256": "5" * 64,
                    "native_state_sha256": "1" * 64,
                    "model_parameter_sha256": "2" * 64,
                },
            }
            (package_root / "fixed_native_state.json").write_text(
                json.dumps(package_manifest), encoding="utf-8"
            )
        return report_path, package_root

    def make_manifest(self, root: Path, reports: dict[str, Path], packages: dict[str, Path | None]):
        manifest_path = root / "manifest.json"
        manifest = {
            "schema": SELECTOR.MANIFEST_SCHEMA,
            "inputs": {
                "corpus_sha256": SELECTOR.CORPUS_SHA256,
                "source_manifest_sha256": SELECTOR.SOURCE_MANIFEST_SHA256,
                "source_payload_sha256": SELECTOR.SOURCE_PAYLOAD_SHA256,
                "source_native_state_sha256": SELECTOR.SOURCE_NATIVE_STATE_SHA256,
                "source_adam_step": SELECTOR.SOURCE_ADAM_STEP,
            },
            "common_training": {
                "reward": "natural-terminal-win-draw-loss-only",
                "learning_rate_f32_bits": SELECTOR.LEARNING_RATE_F32_BITS,
                "value_coefficient_f32_bits": 0,
                "epochs": 4,
                "ppo_clip_epsilon_f32_bits": SELECTOR.PPO_CLIP_F32_BITS,
                "pi_old_adam_step": 520,
                "expected_ending_adam_step": 524,
                "anchor_identity": SELECTOR.ANCHOR_IDENTITY,
                "anchor_direction": SELECTOR.ANCHOR_DIRECTION,
                "anchor_source": SELECTOR.ANCHOR_SOURCE,
                "anchor_scope": SELECTOR.ANCHOR_SCOPE,
            },
            "arms": [
                {
                    "name": name,
                    "beta_f32_bits": SELECTOR.ARMS[name]["beta_f32_bits"],
                    "authority_kind": SELECTOR.ARMS[name]["authority"],
                    "report_path": str(reports[name]),
                    "package_root": None if packages[name] is None else str(packages[name]),
                }
                for name in SELECTOR.ARM_ORDER
            ],
            "selection": {
                "basis": "movement-only",
                "order": "eligible-only-then-higher-mean-tv-then-higher-beta-on-exact-tie",
            },
        }
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        return manifest_path

    def test_four_arm_selection_and_beta_tie_break(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reports = {}
            packages = {}
            for name in SELECTOR.ARM_ORDER:
                reports[name], packages[name] = self.make_report(root, name, 0.02)
            manifest = self.make_manifest(root, reports, packages)
            info = SELECTOR.validate_manifest(manifest, reports)
            arms = [
                SELECTOR.validate_arm(name, reports[name], info["arms"][name]["package_root"])
                for name in SELECTOR.ARM_ORDER
            ]
            arms[0]["mean_action_total_variation"] = 0.03
            arms[1]["mean_action_total_variation"] = 0.03
            self.assertEqual(SELECTOR.select(arms), "beta-1.0")

    def test_failed_arm_must_have_no_package_and_all_four_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reports = {}
            packages = {}
            for name in SELECTOR.ARM_ORDER:
                reports[name], packages[name] = self.make_report(root, name, 0.02, eligible=name != "beta-10.0")
            manifest = self.make_manifest(root, reports, packages)
            info = SELECTOR.validate_manifest(manifest, reports)
            arms = [
                SELECTOR.validate_arm(name, reports[name], info["arms"][name]["package_root"])
                for name in SELECTOR.ARM_ORDER
            ]
            self.assertEqual(SELECTOR.select(arms), "beta-3.0")
            packages["beta-10.0"].mkdir()
            with self.assertRaisesRegex(ValueError, "unexpectedly has a package"):
                SELECTOR.validate_arm("beta-10.0", reports["beta-10.0"], packages["beta-10.0"])
            with self.assertRaisesRegex(ValueError, "exactly four arms"):
                SELECTOR.select(arms[:3])

    def test_tail_gate_and_authority_binding_are_recomputed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report, package = self.make_report(root, "beta-3.0", 0.02)
            value = json.loads(report.read_text(encoding="utf-8"))
            value["candidate"]["tail_shape"]["p99_absolute_joint_log_likelihood_ratio_nearest_rank"] = 0.76
            value["candidate"]["p99_absolute_joint_log_likelihood_ratio"] = 0.76
            value["publication_gate"]["pass"] = False
            report.write_text(json.dumps(value), encoding="utf-8")
            for path in package.iterdir():
                path.unlink()
            package.rmdir()
            self.assertFalse(SELECTOR.validate_arm("beta-3.0", report, package)["eligible"])
            value["source"]["authority_kind"] = "wrong"
            report.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "source binding mismatch"):
                SELECTOR.validate_arm("beta-3.0", report, package)

    def test_manifest_report_binding_is_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reports = {}
            packages = {}
            for name in SELECTOR.ARM_ORDER:
                reports[name], packages[name] = self.make_report(root, name, 0.02)
            manifest = self.make_manifest(root, reports, packages)
            wrong = dict(reports)
            wrong["beta-0.3"] = root / "other.json"
            with self.assertRaisesRegex(ValueError, "report binding mismatch"):
                SELECTOR.validate_manifest(manifest, wrong)

    def test_training_objective_is_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report, package = self.make_report(root, "beta-1.0", 0.02)
            value = json.loads(report.read_text(encoding="utf-8"))
            value["training"]["objective"] = "other"
            report.write_text(json.dumps(value), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "training recipe mismatch"):
                SELECTOR.validate_arm("beta-1.0", report, package)


if __name__ == "__main__":
    unittest.main()
