from __future__ import annotations

import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest


REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = (
    REPOSITORY_ROOT
    / "python"
    / "tools"
    / "generate_native_opponent_sampler_vectors_v1.py"
)
FIXTURE_PATH = REPOSITORY_ROOT / "data" / "native_opponent_sampler_vectors_v1.json"
FIXTURE_SHA256 = "9e5898308d30614a4a09cecb584200521b1a3b727606d8cf78dbe70b51106e18"
SEMANTIC_STREAM_SHA256 = (
    "2b65520a528dcf9eba8d7baded50cc9ad50cf507704c2b4410e2afb4b34d7fad"
)

SPEC = importlib.util.spec_from_file_location("native_opponent_sampler_vectors_v1", GENERATOR_PATH)
if SPEC is None or SPEC.loader is None:  # pragma: no cover
    raise RuntimeError("could not load native opponent sampler vector generator")
generator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generator)


class NativeOpponentSamplerVectorsV1Test(unittest.TestCase):
    def test_committed_fixture_is_canonical_independent_and_exact(self) -> None:
        fixture_bytes = FIXTURE_PATH.read_bytes()
        self.assertEqual(hashlib.sha256(fixture_bytes).hexdigest(), FIXTURE_SHA256)
        fixture = json.loads(fixture_bytes)
        self.assertEqual(
            fixture_bytes,
            generator.canonical_json_bytes(generator.payload(REPOSITORY_ROOT)),
        )
        self.assertEqual(
            fixture["authority"]["generator_sha256"],
            hashlib.sha256(GENERATOR_PATH.read_bytes()).hexdigest(),
        )
        self.assertEqual(
            fixture["authority"]["forbidden_dependencies"],
            ["mtg_kernel_rl", "rust-ffi", "java", "numpy", "torch"],
        )
        self.assertEqual(
            hashlib.sha256(generator.semantic_stream_bytes(fixture)).hexdigest(),
            SEMANTIC_STREAM_SHA256,
        )
        self.assertEqual(fixture["semantic_stream"]["sha256"], SEMANTIC_STREAM_SHA256)
        completed = subprocess.run(
            [sys.executable, str(GENERATOR_PATH), "--check"],
            cwd=REPOSITORY_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("NATIVE_OPPONENT_SAMPLER_VECTORS: PASS", completed.stdout)

    def test_point_vectors_pin_boundaries_and_intentional_modulo_bias(self) -> None:
        fixture = json.loads(FIXTURE_PATH.read_bytes())
        points = {point["name"]: point for point in fixture["points"]}
        self.assertEqual(len(points), 33)
        self.assertEqual(fixture["point_count"], 33)
        required_names = {
            "width-one-seed-zero",
            "width-one-seed-u63-max",
            "width-one-seed-u64-max",
            "count-two-adjacent-zero",
            "count-two-adjacent-one",
            "count-two-adjacent-two",
            "count-two-adjacent-three",
            "count-three-k5-minus-one",
            "count-three-k5",
            "count-64-u64-max",
            "large-prime-second-wrap-minus-one",
            "large-prime-second-wrap",
            "large-prime-u64-max",
            "u32-max-u64-max",
        }
        self.assertTrue(required_names.issubset(points))
        self.assertEqual(
            [
                points[f"count-two-adjacent-{name}"]["selected_index_u32"]
                for name in ("zero", "one", "two", "three")
            ],
            [0, 1, 0, 1],
        )
        self.assertEqual(points["count-three-k5-minus-one"]["selected_index_u32"], 2)
        self.assertEqual(points["count-three-k5"]["selected_index_u32"], 0)
        self.assertEqual(points["count-64-u64-max"]["selected_index_u32"], 63)
        self.assertEqual(points["u32-max-u64-max"]["selected_index_u32"], 0)
        self.assertEqual(
            points["large-prime-first-wrap-minus-one"]["legal_count_u32"],
            4_294_967_291,
        )
        self.assertIn("intentional-modulo-bias", fixture["sampler"]["modulo_bias_rule"])
        self.assertIn("no-rejection-sampling", fixture["sampler"]["modulo_bias_rule"])
        self.assertEqual(
            {case["legal_count_u32"] for case in fixture["rejections"]},
            {0},
        )
        self.assertEqual(
            {case["expected_error"]["code"] for case in fixture["rejections"]},
            {"empty-legal-action-set"},
        )

    def test_full_seed_chains_witness_width_one_advancement(self) -> None:
        fixture = json.loads(FIXTURE_PATH.read_bytes())
        self.assertEqual(fixture["chain_count"], 3)
        self.assertEqual(
            fixture["seed_chain"]["schedule_goldens_sha256"],
            "6b2e1edbbe49b4e02f98794f9057f5c2bb8e3079d2ba8cb3e2a4b9ea6c34867c",
        )
        self.assertEqual(
            fixture["seed_chain"]["payload_encodings"],
            {
                "text": "UTF-8 for version, namespace, and field-name payloads",
                "u63": "exactly-8-byte-big-endian",
            },
        )
        witness_rule = fixture["seed_chain"]["witness_rule"]
        self.assertIn("immediate-successor", witness_rule)
        self.assertIn("exclude-a-final-width-one-substep", witness_rule)
        self.assertIn("counterfactual_nonconsuming_next_action_seed_u63", witness_rule)
        for chain in fixture["chains"]:
            self.assertEqual(
                int(chain["opponent_group_seed_u63"]),
                generator.derive_group_seed(
                    int(chain["base_seed_u63"]),
                    int(chain["episode_index_u63"]),
                    int(chain["opponent_physical_decision_index_u63"]),
                ),
            )
            for expected_index, substep in enumerate(chain["substeps"]):
                self.assertEqual(substep["substep_index_u32"], expected_index)
                self.assertEqual(
                    int(substep["action_seed_u63"]),
                    generator.derive_action_seed(
                        int(chain["opponent_group_seed_u63"]), expected_index
                    ),
                )
                self.assertEqual(
                    substep["selected_index_u32"],
                    int(substep["action_seed_u63"]) % substep["legal_count_u32"],
                )
            self.assertEqual(len(chain["width_one_advancement_witnesses"]), 1)
            witness = chain["width_one_advancement_witnesses"][0]
            width_one_index = witness["width_one_substep_index_u32"]
            next_index = witness["next_substep_index_u32"]
            self.assertEqual(next_index, width_one_index + 1)
            self.assertEqual(chain["substeps"][width_one_index]["legal_count_u32"], 1)
            self.assertEqual(
                int(witness["counterfactual_nonconsuming_next_action_seed_u63"]),
                int(chain["substeps"][width_one_index]["action_seed_u63"]),
            )
            self.assertEqual(
                int(witness["next_action_seed_u63"]),
                int(chain["substeps"][next_index]["action_seed_u63"]),
            )
            self.assertNotEqual(
                witness["next_action_seed_u63"],
                witness["counterfactual_nonconsuming_next_action_seed_u63"],
            )

        trailing = next(
            chain
            for chain in fixture["chains"]
            if chain["name"] == "u63-boundary-width-one-before-tail"
        )
        trailing_index = len(trailing["substeps"]) - 1
        self.assertEqual(trailing["substeps"][trailing_index]["legal_count_u32"], 1)
        self.assertNotIn(
            trailing_index,
            {
                witness["width_one_substep_index_u32"]
                for witness in trailing["width_one_advancement_witnesses"]
            },
        )


if __name__ == "__main__":
    unittest.main()
