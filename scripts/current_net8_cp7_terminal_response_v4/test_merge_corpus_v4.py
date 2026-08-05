from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from _test_support import shard_rows, write_rows
from merge_corpus_v4 import merge


class MergeCorpusV4Test(unittest.TestCase):
    def test_merge_reindexes_and_binds_every_input(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = root / "tasks"
            inputs.mkdir()
            write_rows(
                inputs / "gae8-p000004-n001.outcome.jsonl",
                shard_rows(pair=4, base_seed=77),
            )
            write_rows(
                inputs / "gae8-p000005-n001.outcome.jsonl",
                shard_rows(pair=5, base_seed=77),
            )
            output = root / "corpus.jsonl"
            report_path = root / "corpus.report.json"
            report = merge(
                input_root=inputs,
                output_jsonl=output,
                report_path=report_path,
                base_seed=77,
                first_pair=4,
                expected_pairs=2,
            )
            rows = [json.loads(line) for line in output.read_text().splitlines()]
            self.assertEqual([row["record_ordinal"] for row in rows], list(range(9)))
            decisions = [row for row in rows if row["record_type"] == "decision"]
            terminals = [row for row in rows if row["record_type"] == "terminal"]
            self.assertEqual(
                [row["outcome_decision_ordinal"] for row in decisions],
                [0, 1, 2, 3],
            )
            self.assertEqual(
                [row["first_outcome_decision_ordinal"] for row in terminals],
                [0, 1, 2, 3],
            )
            self.assertEqual(report["pair_count"], 2)
            self.assertEqual(len(report["input_sha256_by_filename"]), 2)
            self.assertTrue(report_path.is_file())

    def test_gap_in_pair_inventory_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs = root / "tasks"
            inputs.mkdir()
            write_rows(
                inputs / "gae8-p000004-n001.outcome.jsonl",
                shard_rows(pair=4, base_seed=77),
            )
            write_rows(
                inputs / "gae8-p000006-n001.outcome.jsonl",
                shard_rows(pair=6, base_seed=77),
            )
            with self.assertRaisesRegex(ValueError, "missing, overlapping, or reordered"):
                merge(
                    input_root=inputs,
                    output_jsonl=root / "corpus.jsonl",
                    report_path=root / "report.json",
                    base_seed=77,
                    first_pair=4,
                    expected_pairs=3,
                )


if __name__ == "__main__":
    unittest.main()
