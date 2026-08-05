import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("select_arm_v1.py")
SPEC = importlib.util.spec_from_file_location("select_arm_v1", MODULE_PATH)
SELECTOR = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(SELECTOR)


class SelectorTests(unittest.TestCase):
    def make_arm(
        self,
        root: Path,
        name: str,
        mean_tv: float,
        eligible: bool = True,
    ) -> tuple[Path, Path]:
        contract = SELECTOR.ARMS[name]
        report_path = root / f"{name}.json"
        package_root = root / f"{name}-package"
        payload_bytes = f"payload-{name}".encode()
        payload_sha = hashlib.sha256(payload_bytes).hexdigest()
        max_log_ratio = 0.8 if eligible else 1.1
        report = {
            "schema": contract["schema"],
            "source": {
                "authority_kind": "current-net8-gae8-v1",
                "manifest_sha256": SELECTOR.SOURCE_MANIFEST_SHA256,
                "payload_sha256": SELECTOR.SOURCE_PAYLOAD_SHA256,
                "native_state_sha256": SELECTOR.SOURCE_NATIVE_STATE_SHA256,
                "adam_step": SELECTOR.SOURCE_ADAM_STEP,
            },
            "corpus": {
                "sha256": SELECTOR.CORPUS_SHA256,
                "pair_indices": list(range(64)),
                "pair_count": 64,
                "episode_count": 128,
                "decision_row_count": 4769,
                "terminal_return_counts_loss_draw_win": [80, 0, 48],
            },
            "training": {
                "reward": "natural_terminal_win_draw_loss_only",
                "learning_rate_f32_bits": SELECTOR.LEARNING_RATE_F32_BITS,
                "value_coefficient_f32_bits": contract["value_coefficient_f32_bits"],
                "ppo_clip_epsilon_f32_bits": SELECTOR.PPO_CLIP_F32_BITS,
                "epochs": 4,
                "starting_adam_step": SELECTOR.SOURCE_ADAM_STEP,
                "ending_adam_step": SELECTOR.ENDING_ADAM_STEP,
                "advantage_transform": {
                    "identity": "terminal_reinforce_frozen_source_value_standardized_by_candidate_seat_episode_balanced/v1"
                },
            },
            "candidate": {
                "adam_step": SELECTOR.ENDING_ADAM_STEP,
                "payload_byte_count": len(payload_bytes),
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
                    "maximum_absolute_joint_log_likelihood_ratio": max_log_ratio,
                    "mean_old_to_current_forward_kl": 0.002,
                    "mean_policy_surrogate": 0.001,
                },
            },
            "publication_gate": {
                "finite": True,
                "parameter_l2_cap": 0.75,
                "mean_action_total_variation_floor": 0.010,
                "mean_action_total_variation_cap": 0.050,
                "p90_action_total_variation_cap": 0.150,
                "maximum_absolute_joint_log_likelihood_ratio_cap": 1.0,
                "pass": eligible,
            },
        }
        report_path.write_text(json.dumps(report), encoding="utf-8")
        if eligible:
            package_root.mkdir()
            (package_root / "checkpoint.state.f32le").write_bytes(payload_bytes)
            manifest = {
                "schema": "mtg-kernel-xmage-fixed-native-state/v1",
                "authority_kind": contract["authority"],
                "source_result_sha256": SELECTOR.sha256(report_path),
                "payload": {
                    "filename": "checkpoint.state.f32le",
                    "byte_count": len(payload_bytes),
                    "adam_step": SELECTOR.ENDING_ADAM_STEP,
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
                json.dumps(manifest), encoding="utf-8"
            )
        return report_path, package_root

    def test_validation_and_selection_are_movement_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy_paths = self.make_arm(root, "policy-only", 0.012)
            low_paths = self.make_arm(root, "low-value", 0.015)
            policy = SELECTOR.validate_arm("policy-only", *policy_paths)
            low = SELECTOR.validate_arm("low-value", *low_paths)
            self.assertEqual(SELECTOR.select(policy, low), "low-value")
            low["mean_action_total_variation"] = policy["mean_action_total_variation"]
            self.assertEqual(SELECTOR.select(policy, low), "policy-only")

    def test_failing_arm_requires_absent_package_and_cannot_advance(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            policy_paths = self.make_arm(root, "policy-only", 0.012, eligible=False)
            low_paths = self.make_arm(root, "low-value", 0.012, eligible=False)
            policy = SELECTOR.validate_arm("policy-only", *policy_paths)
            low = SELECTOR.validate_arm("low-value", *low_paths)
            self.assertIsNone(SELECTOR.select(policy, low))

    def test_recipe_tamper_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path, package_root = self.make_arm(root, "low-value", 0.012)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["training"]["value_coefficient_f32_bits"] = 0
            report_path.write_text(json.dumps(report), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "training recipe mismatch"):
                SELECTOR.validate_arm("low-value", report_path, package_root)

    def test_secondary_package_digest_tamper_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report_path, package_root = self.make_arm(root, "policy-only", 0.012)
            manifest_path = package_root / "fixed_native_state.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["payload"]["first_moments_sha256"] = "9" * 64
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "package binding mismatch"):
                SELECTOR.validate_arm("policy-only", report_path, package_root)


if __name__ == "__main__":
    unittest.main()
