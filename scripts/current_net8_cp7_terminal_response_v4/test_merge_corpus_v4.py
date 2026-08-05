from __future__ import annotations

import json
import hashlib
from pathlib import Path
import tempfile
import unittest

from _test_support import shard_rows, write_rows
from merge_corpus_v4 import merge
from outcome_v2 import validate_outcome_shard


class MergeCorpusV4Test(unittest.TestCase):
    def make_collection_report(
        self,
        root: Path,
        inputs: Path,
        *,
        base_seed: int,
        first_pair: int,
        pair_count: int,
    ) -> Path:
        panel = {
            "arm": "gae8",
            "opponent": "xmage-cp7",
            "cp7_skill": 7,
            "base_seed": base_seed,
            "pair_start": first_pair,
            "pair_count": pair_count,
            "episode_count": pair_count * 2,
        }
        manifest_path = root / "manifest.json"
        manifest = {
            "schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-manifest/v1",
            "panel": panel,
            "output_root": str(root),
            "collection_prerequisites": {
                "throughput_screen_report": {},
                "revealed_identity_report": {},
                "topology_selection_report": {},
            },
        }
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        tasks = []
        for path in sorted(inputs.glob("*.outcome.jsonl")):
            pair = int(path.name.split("-p", 1)[1].split("-", 1)[0])
            validation = validate_outcome_shard(
                path, base_seed=base_seed, first_pair=pair, pair_count=1
            )
            outcome = {key: value for key, value in validation.items() if key != "header"}
            tasks.append(
                {
                    "first_pair": pair,
                    "pair_count": 1,
                    "outcome": outcome,
                }
            )
        report = {
            "schema": "mtg-kernel-current-net8-cp7-terminal-response-v4-collection/v1",
            "status": "complete",
            "manifest": {
                "path": str(manifest_path),
                "sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
            },
            "panel": panel,
            "tasks": tasks,
        }
        path = root / "report.json"
        path.write_text(json.dumps(report), encoding="utf-8")
        return path

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
            collection_report = self.make_collection_report(
                root, inputs, base_seed=77, first_pair=4, pair_count=2
            )
            report = merge(
                input_root=inputs,
                output_jsonl=output,
                report_path=report_path,
                base_seed=77,
                first_pair=4,
                expected_pairs=2,
                collection_report_path=collection_report,
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
            self.assertEqual(report["source_phase"], "formal-development-collection")
            self.assertIsNotNone(report["source_collection"])
            self.assertEqual(len(report["merger_tool"]["sha256"]), 64)
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
                    collection_report_path=root / "unused-collection-report.json",
                )


if __name__ == "__main__":
    unittest.main()
