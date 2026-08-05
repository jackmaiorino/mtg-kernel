from __future__ import annotations

import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path

from _test_support import shard_rows, write_rows


MODULE_PATH = Path(__file__).with_name("compare_revealed_identity_v4.py")
SPEC = importlib.util.spec_from_file_location("compare_revealed_identity_v4", MODULE_PATH)
COMPARE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(COMPARE)


class RevealedIdentityV4Test(unittest.TestCase):
    BASE_SEED = 1_820_001

    def make_panel(self, root: Path, *, tamper: bool = False) -> tuple[Path, Path]:
        baseline = root / "baseline"
        candidate = root / "candidate"
        baseline.mkdir()
        candidate.mkdir()
        per_pair = []
        for pair in range(2):
            rows = shard_rows(pair, self.BASE_SEED)
            write_rows(baseline / f"gae8-pair-{pair:04d}.outcome.jsonl", rows)
            per_pair.append(rows)

        chunk = [copy.deepcopy(per_pair[0][0])]
        record_ordinal = 1
        decision_offset = 0
        for rows in per_pair:
            for source in rows[1:]:
                row = copy.deepcopy(source)
                row["record_ordinal"] = record_ordinal
                record_ordinal += 1
                if row["record_type"] == "decision":
                    row["outcome_decision_ordinal"] += decision_offset
                else:
                    row["first_outcome_decision_ordinal"] += decision_offset
                chunk.append(row)
            decision_offset += 2
        if tamper:
            chunk[-2]["model_input_sha256"] = "f" * 64
        write_rows(candidate / "gae8-p000000-n002.outcome.jsonl", chunk)
        return baseline, candidate

    def test_batched_shard_matches_revealed_per_pair_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline, candidate = self.make_panel(root)
            report_path = root / "identity.json"
            report = COMPARE.compare(
                baseline_root=baseline,
                candidate_root=candidate,
                report_path=report_path,
                base_seed=self.BASE_SEED,
                first_pair=0,
                pair_count=2,
            )
            self.assertEqual(report["status"], "pass")
            self.assertEqual(len(report["pairs"]), 2)
            self.assertTrue(report_path.is_file())

    def test_nonordinal_difference_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            baseline, candidate = self.make_panel(root, tamper=True)
            with self.assertRaisesRegex(ValueError, "identity mismatch at pair 1"):
                COMPARE.compare(
                    baseline_root=baseline,
                    candidate_root=candidate,
                    report_path=root / "identity.json",
                    base_seed=self.BASE_SEED,
                    first_pair=0,
                    pair_count=2,
                )


if __name__ == "__main__":
    unittest.main()
