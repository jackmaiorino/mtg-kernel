import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import make_shards


class ShardPreparationTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        (self.root / "out").mkdir()
        (self.root / "frozen-gy").mkdir()
        for model in ("r1", "r2"):
            rows = [{"root_id": f"{model}-{index}", "seed": index, "model": model} for index in range(9)]
            (self.root / "frozen-gy" / f"roots-{model}.jsonl").write_text(
                "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")

    def build(self, budget=123):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return make_shards.build_shards(budget, self.root)

    def test_nonpositive_budget_refuses_before_any_write(self):
        before = {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}
        for budget in (0, -1):
            with self.assertRaisesRegex(ValueError, "positive integer"):
                self.build(budget)
        self.assertEqual(before, {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()})

    def test_interrupted_tail_preserves_evidence_and_round_robin_membership(self):
        path = self.root / "out" / "formal-unordered-r1.jsonl"
        evidence = b'{"kind":"s4a_root","root_id":"r1-0"}\n{"kind":"s4a_root","root_id":"r1-1'
        path.write_bytes(evidence)
        jobs = self.build()
        self.assertEqual(path.read_bytes(), evidence)
        self.assertEqual(len(jobs), 8)
        for job in jobs:
            self.assertEqual(job["workers"], 1)
            self.assertEqual(job["group_max_cpu_seconds"], 123)
            self.assertEqual(job["base_seed"], 2026100941 if job["model"] == "r1" else 2026100942)
            rows = [json.loads(line) for line in Path(job["roots"]).read_text().splitlines()]
            start = 1 if job["model"] == "r1" else 0
            index = int(job["name"][-1])
            self.assertEqual(rows, [{"root_id": f'{job["model"]}-{n}', "seed": n, "model": job["model"]}
                                    for n in list(range(start, 9))[index::4]])

    def test_partial_utf8_tail_and_valid_unterminated_record(self):
        path = self.root / "out" / "formal-unordered-r1.jsonl"
        path.write_bytes(b'{"kind":"s4a_root","root_id":"r1-0"}\n{"note":"\xc3')
        self.assertEqual(make_shards.completed_roots(path), {"r1-0"})
        path.write_bytes(b'{"kind":"s4a_root","root_id":"r1-1"}')
        self.assertEqual(make_shards.completed_roots(path), {"r1-1"})

    def test_malformed_complete_row_refuses_before_prepared_output_changes(self):
        (self.root / "out" / "formal-unordered-r2.jsonl").write_text('{"kind": broken}\n', encoding="utf-8")
        with self.assertRaises(json.JSONDecodeError):
            self.build()
        self.assertFalse((self.root / "queue-formal-shards.json").exists())
        self.assertEqual(list((self.root / "frozen-gy").glob("shard-*.jsonl")), [])


if __name__ == "__main__":
    unittest.main()
