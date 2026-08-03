#!/usr/bin/env python3

from __future__ import annotations

import unittest
from pathlib import Path
import tempfile

import run_oracle_v1 as oracle


class OracleTests(unittest.TestCase):
    def test_splitmix_is_repeatable_and_antithetic_candidates_are_bounded(self) -> None:
        first = oracle.SplitMix64(oracle.ORACLE_SEED)
        second = oracle.SplitMix64(oracle.ORACLE_SEED)
        observed = [first.normal_approx() for _ in range(oracle.PARAMETER_COUNT)]
        self.assertEqual(observed, [second.normal_approx() for _ in range(oracle.PARAMETER_COUNT)])
        positive = oracle._clamp_delta([oracle.INITIAL_SIGMA * value for value in observed])
        negative = oracle._clamp_delta([-oracle.INITIAL_SIGMA * value for value in observed])
        self.assertTrue(all(abs(value) <= oracle.MAX_ABS_DELTA for value in positive))
        self.assertEqual(positive, [-value for value in negative])

    def test_f32_bits_round_trip(self) -> None:
        for value in (0.0, -0.0, 0.05, -0.05, 0.001):
            bits = oracle._f32_bits(value)
            self.assertEqual(len(bits), 8)
            self.assertEqual(
                oracle._f32(value),
                oracle.struct.unpack("<f", oracle.struct.pack("<I", int(bits, 16)))[0],
            )

    def test_formal_topology_uses_available_cpu_parallelism(self) -> None:
        self.assertEqual(oracle.POPULATION, oracle.MAX_WORKERS)
        self.assertEqual(oracle.POPULATION % 2, 0)
        self.assertLess(oracle.ELITES, oracle.POPULATION)

    def test_trajectory_digest_ignores_checkpoint_but_tracks_choices(self) -> None:
        decision = {
            "record_type": "decision",
            "base_seed_u64_hex": "01",
            "pair_index": 0,
            "episode_id": 0,
            "step": 0,
            "selected_index": 1,
            "checkpoint": {"candidate_json_sha256": "a" * 64},
            "old_policy_logits_f32_bits": [0, 1],
        }
        terminal = {
            "record_type": "terminal",
            "base_seed_u64_hex": "01",
            "pair_index": 0,
            "episode_id": 0,
            "candidate_terminal_reward": 1,
            "checkpoint": {"candidate_json_sha256": "a" * 64},
        }
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            paths = [root / f"panel-{index}.jsonl" for index in range(3)]
            variants = []
            for checkpoint, selected in (("a", 1), ("b", 1), ("a", 0)):
                current_decision = dict(decision)
                current_terminal = dict(terminal)
                current_decision["checkpoint"] = {"candidate_json_sha256": checkpoint * 64}
                current_decision["old_policy_logits_f32_bits"] = [7, 8]
                current_decision["selected_index"] = selected
                current_terminal["checkpoint"] = {"candidate_json_sha256": checkpoint * 64}
                variants.append((current_decision, current_terminal))
            for path, rows in zip(paths, variants, strict=True):
                path.write_text(
                    "".join(oracle.json.dumps(row) + "\n" for row in rows),
                    encoding="utf-8",
                )
            first, second, changed = [oracle._trajectory_sha256(path) for path in paths]
            self.assertEqual(first, second)
            self.assertNotEqual(first, changed)


if __name__ == "__main__":
    unittest.main()
