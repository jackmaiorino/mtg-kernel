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
    REPOSITORY_ROOT / "python" / "tools" / "generate_fast_sampler_candidate_vectors_v1.py"
)
FIXTURE_PATH = REPOSITORY_ROOT / "data" / "fast_sampler_candidate_vectors_v1.json"
FIXTURE_SHA256 = "407a08fb9b9bb5012f14d779d0878c986ce0f16530820a89f5bd54c33d5e7456"

SPEC = importlib.util.spec_from_file_location("fast_sampler_candidate_vectors_v1", GENERATOR_PATH)
if SPEC is None or SPEC.loader is None:  # pragma: no cover
    raise RuntimeError("could not load fast sampler candidate vector generator")
generator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generator)


class FastSamplerCandidateVectorsV1Test(unittest.TestCase):
    def test_committed_fixture_is_canonical_and_exact(self) -> None:
        fixture_bytes = FIXTURE_PATH.read_bytes()
        self.assertEqual(hashlib.sha256(fixture_bytes).hexdigest(), FIXTURE_SHA256)
        fixture = json.loads(fixture_bytes)
        self.assertEqual(fixture["vector_schema_version"], 1)
        self.assertEqual(
            fixture["generator_identity"],
            "stdlib-only-independent-integer-bit-reference-v1",
        )
        self.assertEqual(
            fixture_bytes,
            generator.canonical_json_bytes(generator.payload(REPOSITORY_ROOT)),
        )
        self.assertEqual(
            fixture["authority"]["generator_sha256"],
            hashlib.sha256(GENERATOR_PATH.read_bytes()).hexdigest(),
        )
        completed = subprocess.run(
            [sys.executable, str(GENERATOR_PATH), "--check"],
            cwd=REPOSITORY_ROOT,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("FAST_SAMPLER_CANDIDATE_VECTORS: PASS", completed.stdout)

    def test_integer_reference_pins_rounding_apportionment_and_rng(self) -> None:
        self.assertEqual(
            generator.candidate_masses((0x00000000, 0x3F800000, 0x40000000)),
            (
                1_660_770_942_083_389_844,
                4_514_443_473_098_088_106,
                12_271_529_658_528_073_666,
            ),
        )
        self.assertEqual(
            generator.candidate_masses((0x00000000,) * 3),
            (
                6_148_914_691_236_517_206,
                6_148_914_691_236_517_205,
                6_148_914_691_236_517_205,
            ),
        )
        self.assertEqual(generator.quantized_gap_q8(0x00000000, 0xBB000000), 0)
        self.assertEqual(generator.quantized_gap_q8(0x00000000, 0xBBC00000), 2)
        self.assertEqual(generator.quantized_gap_q8(0x00000000, 0xC1800000), 4096)
        self.assertEqual(generator.splitmix64_first(0), 0xE220A8397B1DCDAF)
        self.assertEqual(generator.splitmix64_first((1 << 64) - 1), 0xE4D971771B652C20)

    def test_vector_set_covers_declared_boundaries(self) -> None:
        fixture = json.loads(FIXTURE_PATH.read_bytes())
        cases = {case["name"]: case for case in fixture["cases"]}
        self.assertEqual(len(cases["width-one"]["logit_bits_hex"]), 1)
        self.assertEqual(len(cases["width-two-ordered"]["logit_bits_hex"]), 2)
        self.assertEqual(len(cases["maximum-admitted-width"]["logit_bits_hex"]), 64)
        self.assertEqual(
            cases["q8-exact-ties-to-even"]["logit_bits_hex"],
            ["00000000", "bb000000", "bbc00000"],
        )
        self.assertIn(
            "0xBB000000 is the convention-discriminating witness",
            cases["q8-exact-ties-to-even"]["coverage_note"],
        )
        self.assertIn(
            "ties-to-even=0, half-up=1",
            cases["q8-exact-ties-to-even"]["coverage_note"],
        )
        self.assertIn(
            "ties-to-even=2, half-up=2",
            cases["q8-exact-ties-to-even"]["coverage_note"],
        )
        self.assertIn(
            "pre-existing Rust fast-sampler test recipe",
            cases["maximum-admitted-width"]["input_recipe_provenance"],
        )
        self.assertEqual(
            list(map(int, cases["hamilton-exact-remainder-tie"]["mass_u128"])),
            [
                6_148_914_691_236_517_206,
                6_148_914_691_236_517_205,
                6_148_914_691_236_517_205,
            ],
        )
        self.assertEqual(
            cases["signed-zero-and-subnormal"]["logit_bits_hex"],
            ["00000000", "80000000", "00000001", "80000001"],
        )
        for case in cases.values():
            self.assertEqual(sum(map(int, case["mass_u128"])), 1 << 64)
            self.assertEqual(len(case["draws"]), 7)
            self.assertEqual(case["draws"][-2]["seed_u64"], str((1 << 64) - 1))

        rejections = {case["name"]: case for case in fixture["rejections"]}
        self.assertEqual(rejections["empty-width"]["expected_error"]["code"], "empty")
        self.assertEqual(
            rejections["width-65"]["expected_error"],
            {"code": "width_exceeded", "maximum": 64, "width": 65},
        )
        for name, bits in (
            ("positive-infinity", "7f800000"),
            ("negative-infinity", "ff800000"),
            ("quiet-nan-payload", "7fc00001"),
        ):
            self.assertEqual(
                rejections[name]["expected_error"],
                {"bits_hex": bits, "code": "nonfinite", "index": 1, "width": 2},
            )
        self.assertIn("sign-agnostic", fixture["nonfinite_coverage_note"])


if __name__ == "__main__":
    unittest.main()
