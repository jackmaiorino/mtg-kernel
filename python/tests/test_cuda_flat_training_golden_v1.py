from __future__ import annotations

import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
GENERATOR_PATH = ROOT / "python" / "tools" / "generate_cuda_flat_training_golden_v1.py"
FIXTURE_PATH = (
    ROOT
    / "mtg-kernel"
    / "examples"
    / "data"
    / "cuda_flat_training_independent_golden_v1.json"
)

SPEC = importlib.util.spec_from_file_location("cuda_flat_training_golden_v1", GENERATOR_PATH)
assert SPEC is not None and SPEC.loader is not None
GENERATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GENERATOR)


class CudaFlatTrainingGoldenV1Tests(unittest.TestCase):
    def test_checked_fixture_matches_standard_library_generator(self) -> None:
        self.assertTrue(GENERATOR.fixture_matches(FIXTURE_PATH, GENERATOR_PATH))
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        self.assertTrue(GENERATOR.validate_fixture_integrity(fixture))
        self.assertEqual(fixture["provenance"]["third_party_dependencies"], [])
        self.assertEqual(
            fixture["provenance"]["generator_language"],
            "Python 3 standard library only",
        )
        self.assertEqual(len(fixture["expected"]["tensors"]), 14)
        self.assertEqual(
            sum(record["length"] for record in fixture["expected"]["tensors"]),
            fixture["contract"]["parameter_count"],
        )

    def test_integrity_and_byte_check_reject_numeric_corruption(self) -> None:
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        corrupted = copy.deepcopy(fixture)
        corrupted["expected"]["forward"]["logits"][0] += 0.25
        self.assertFalse(GENERATOR.validate_fixture_integrity(corrupted))
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "fixture.json"
            candidate.write_bytes(GENERATOR.canonical_bytes(corrupted))
            self.assertFalse(GENERATOR.fixture_matches(candidate, GENERATOR_PATH))

    def test_contract_and_tensor_order_drift_change_the_fixture(self) -> None:
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
        contract_drift = copy.deepcopy(fixture)
        contract_drift["contract"]["epsilon"] = 1.0e-8
        self.assertFalse(GENERATOR.validate_fixture_integrity(contract_drift))
        tensor_drift = copy.deepcopy(fixture)
        tensor_drift["expected"]["tensors"][0]["name"] = "renamed_state_w1"
        self.assertFalse(GENERATOR.validate_fixture_integrity(tensor_drift))

    def test_order_evidence_rejects_a_summary_preserving_permutation(self) -> None:
        model = GENERATOR.make_model()
        batch = GENERATOR.make_batch()
        activations = GENERATOR.forward(model, batch)
        _, d_logits, d_values = GENERATOR.detached_loss_and_output_gradients(
            activations, batch
        )
        gradient = GENERATOR.backward(
            model, batch, activations, d_logits, d_values
        )["state_w1"]
        permuted = list(gradient)
        permuted[1:1025] = permuted[2:1025] + permuted[1:2]
        self.assertEqual(
            GENERATOR.vector_summary(gradient),
            GENERATOR.vector_summary(permuted),
        )
        self.assertNotEqual(
            GENERATOR.order_evidence(gradient),
            GENERATOR.order_evidence(permuted),
        )


if __name__ == "__main__":
    unittest.main()
