"""Synthetic correctness fixtures; no saved-case diagnosis or engine execution."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


PATH = Path(__file__).resolve().parents[2] / "tools/spy_diag/support_chains.py"
SPEC = importlib.util.spec_from_file_location("support_chains", PATH)
support = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(support)


class SupportBindings(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.path = self.root / "fixture.trace.jsonl"
        self.records = [
            {"r": "meta", "schema": "s4a-diag-trace/v1", "root_id": "fixture"},
            {"r": "node", "id": 1, "key": "a" * 64, "parent": None, "dep": 0,
             "n": [1], "w": [1], "visits": 1, "lab": [4], "perm": [0], "edges": ["action"]},
            {"r": "sel", "i": 1, "end": "win", "path": [[1, 0]], "new": 1},
            {"r": "eval", "world": 0, "dec": [
                {"dep": 0, "node": 1, "edge": 0, "src": "tree", "off": 1, "ch": 4}]},
        ]
        self.pin = self.save()
        self.row = {"kind": "s4a_diag_root", "root_id": "fixture",
                    "runtime_rules": "resolution-boundary-v1",
                    "diag": {"trace": {"sha256": self.pin[0]}}}

    def save(self):
        data = ("\n".join(json.dumps(r) for r in self.records) + "\n").encode()
        self.path.write_bytes(data)
        return (hashlib.sha256(data).hexdigest(), "a" * 64, None)

    def load(self, pin=None, row=None):
        return support.load(self.path, "fixture", pin or self.pin, row or self.row, 1)

    def test_bound_case_is_admitted_and_chronology_reconciles(self):
        nodes, sims, worlds = self.load()
        self.assertTrue(support.chronology(nodes, sims, 1)["reconciled_with_saved_counts"])
        self.assertTrue(support.eval_prefix(worlds, 1)[0]["reached_target_node"])

    def test_missing_historical_and_wrong_runtime_rows_are_refused(self):
        for runtime in (None, "historical", "unknown"):
            with self.subTest(runtime=runtime), self.assertRaises(ValueError):
                self.load(row={**self.row, "runtime_rules": runtime})
        for row in ({}, {**self.row, "root_id": "different"},
                    {**self.row, "diag": {"trace": {"sha256": "0" * 64}}}):
            with self.assertRaises(ValueError):
                support.load(self.path, "fixture", self.pin, row, 1)

    def test_changed_bytes_key_parent_and_meta_are_refused(self):
        for pin in (("0" * 64, self.pin[1], None),
                    (self.pin[0], "b" * 64, None), (self.pin[0], self.pin[1], [0, 1])):
            with self.assertRaises(ValueError):
                self.load(pin=pin)
        self.records[0]["root_id"] = "different"
        changed = self.save()
        with self.assertRaises(ValueError):
            self.load(pin=changed, row={**self.row, "diag": {"trace": {"sha256": changed[0]}}})

    def test_duplicate_trace_records_and_unordered_selections_are_refused(self):
        original = copy.deepcopy(self.records)
        for record in original:
            self.records = original + [record]
            self.save()
            with self.assertRaises(ValueError):
                support.load(self.path)
        self.records = original + [{**original[2], "i": 0}]
        self.save()
        with self.assertRaises(ValueError):
            support.load(self.path)

    def test_duplicate_result_roots_are_refused(self):
        for name in ("one.jsonl", "two.jsonl"):
            (self.root / name).write_text(json.dumps(self.row) + "\n", encoding="utf-8")
        with self.assertRaises(ValueError):
            support.load_rows(self.root)

    def test_null_history_miss_is_never_a_reached_target(self):
        miss = {"dep": 2, "node": None, "edge": None, "src": "first_miss_plain", "off": 0, "ch": 0}
        later = {**self.records[-1]["dec"][0], "node": 2}
        worlds = [{"world": 0, "dec": [miss, later]}]
        for nid in (None, 2):
            result = support.eval_prefix(worlds, nid)[0]
            self.assertFalse(result["reached_target_node"])
            self.assertEqual(len(result["prefix"]), 1)

    def test_comparators_follow_plan_wins_then_visits_with_id_ties(self):
        nodes = {i: {"id": i, "lab": [4], "w": [wins], "visits": visits}
                 for i, wins, visits in ((1, 9, 10), (2, 8, 20), (3, 1, 100), (4, 9, 99), (5, 0, 200))}
        self.assertEqual(support.success_nodes(nodes, None), [1, 3])
        self.assertEqual(support.success_nodes(nodes, 1), [4, 3])

    def test_winning_backups_and_visits_must_also_reconcile(self):
        nodes, sims, _ = self.load()
        for field, wrong in (("w", [0]), ("visits", 2)):
            changed = copy.deepcopy(nodes)
            changed[1][field] = wrong
            with self.assertRaises(ValueError):
                support.chronology(changed, sims, 1)

    def test_all_node_reconciliation_refuses_bad_unselected_nodes_and_paths(self):
        nodes, sims, _ = self.load()
        self.assertTrue(support.reconcile_all(nodes, sims))
        nodes[2] = {**nodes[1], "id": 2, "n": [0], "w": [0], "visits": 0}
        self.assertTrue(support.reconcile_all(nodes, sims))
        for field, bad in (("n", [1]), ("w", [1]), ("visits", 1)):
            changed = copy.deepcopy(nodes)
            changed[2][field] = bad
            self.assertFalse(support.reconcile_all(changed, sims))
        for path in ([[3, 0]], [[1, -1]], [[1, 1]]):
            self.assertFalse(support.reconcile_all(nodes, [{**sims[0], "path": path}]))

    def test_count_mismatch_refuses_before_output_and_preserves_existing_output(self):
        rows_dir = self.root / "rows"
        rows_dir.mkdir()
        (rows_dir / "one.jsonl").write_text(json.dumps(self.row) + "\n", encoding="utf-8")
        cases, pins = [("fixture", 1, "fixture")], {"fixture": self.pin}
        output = self.root / "out.json"
        support.write_result(self.root, rows_dir, output, cases, pins)
        previous = output.read_bytes()
        with self.assertRaises(FileExistsError):
            support.write_result(self.root, rows_dir, output, cases, pins)
        self.assertEqual(output.read_bytes(), previous)
        self.records[1]["n"] = [2]
        changed = self.save()
        (rows_dir / "one.jsonl").write_text(json.dumps(
            {**self.row, "diag": {"trace": {"sha256": changed[0]}}}) + "\n", encoding="utf-8")
        for out in (output, self.root / "new.json"):
            with self.assertRaises(ValueError):
                support.write_result(self.root, rows_dir, out, cases, {"fixture": changed})
        self.assertEqual(output.read_bytes(), previous)
        self.assertFalse((self.root / "new.json").exists())


if __name__ == "__main__":
    unittest.main()
