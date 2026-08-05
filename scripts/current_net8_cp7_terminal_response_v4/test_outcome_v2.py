from __future__ import annotations

import copy
from pathlib import Path
import tempfile
import unittest

from _test_support import shard_rows, write_rows
from outcome_v2 import (
    EXPECTED_CHECKPOINT,
    EXPECTED_PACKAGE_MANIFEST,
    PACKAGE_MANIFEST_SHA256,
    validate_outcome_shard,
)


class OutcomeV2Test(unittest.TestCase):
    def test_exact_shard_passes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "shard.jsonl"
            write_rows(path, shard_rows(pair=7, base_seed=99))
            report = validate_outcome_shard(
                path, base_seed=99, first_pair=7, pair_count=1
            )
            self.assertEqual(report["episode_count"], 2)
            self.assertEqual(report["decision_count"], 2)
            self.assertEqual(report["record_count"], 5)

    def test_wrong_checkpoint_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "shard.jsonl"
            rows = shard_rows(pair=0, base_seed=5)
            rows[1] = copy.deepcopy(rows[1])
            rows[1]["checkpoint"]["loaded_checkpoint_sha256"] = "0" * 64
            write_rows(path, rows)
            with self.assertRaisesRegex(ValueError, "checkpoint identity mismatch"):
                validate_outcome_shard(path, base_seed=5, first_pair=0, pair_count=1)

    def test_non_natural_terminal_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "shard.jsonl"
            rows = shard_rows(pair=0, base_seed=5)
            rows[2] = copy.deepcopy(rows[2])
            rows[2]["terminal"]["terminal_classification"] = "truncated"
            rows[2]["terminal"]["terminal_code"] = "decision_cap"
            rows[2]["terminal"]["terminal_outcome"] = "truncated"
            rows[2]["terminal"]["winner"] = None
            write_rows(path, rows)
            with self.assertRaisesRegex(ValueError, "exact natural outcome"):
                validate_outcome_shard(path, base_seed=5, first_pair=0, pair_count=1)

    def test_package_constants_bind_loaded_checkpoint(self) -> None:
        self.assertEqual(
            EXPECTED_CHECKPOINT["loaded_checkpoint_sha256"],
            PACKAGE_MANIFEST_SHA256,
        )
        self.assertEqual(
            EXPECTED_CHECKPOINT["loaded_train_state_sha256"],
            EXPECTED_PACKAGE_MANIFEST["payload"]["native_state_sha256"],
        )


if __name__ == "__main__":
    unittest.main()
