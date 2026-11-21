"""Pooled-lineage proof comparison tests on synthetic run trees."""
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import pooled_proof as pp  # noqa: E402


def write_run(root: Path, recorded_root: str, states, trajectories, backend="native-cpu-parallel-episodes-v1"):
    for index, (state, hashes) in enumerate(zip(states, trajectories)):
        attempt = root / "iterations" / f"{index:06d}" / "attempt-000000"
        (attempt / "update").mkdir(parents=True)
        (attempt / "collect").mkdir()
        (attempt / "update" / "update.json").write_text(json.dumps({
            "after_state_sha256": state,
            "checkpoint": {"path": f"{recorded_root}/iterations/{index:06d}/attempt-000000/update/checkpoint.json"},
            "trajectories": [{"path": "x", "sha256": h} for h in hashes]}))
        (attempt / "collect" / "collection.json").write_text(json.dumps({"collection_backend": backend}))


class PooledProofTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-proof-test-"))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_same_root_requires_equal_states_and_trajectories(self):
        write_run(self.root / "ref", "/srv/run", ["s0", "s1"], [["a", "b"], ["c", "d"]])
        write_run(self.root / "pool", "/srv/run", ["s0", "s1"], [["a", "b"], ["c", "d"]], "pooled-collection-shards-v1")
        report = pp.compare(self.root / "ref", self.root / "pool", same_root=True)
        self.assertTrue(report["pass"])
        self.assertTrue(all(r["trajectories_equal"] for r in report["rows"]))
        self.assertEqual(report["rows"][0]["pooled_backend"], "pooled-collection-shards-v1")
        write_run(self.root / "pool2", "/srv/run", ["s0", "s1"], [["a", "b"], ["c", "X"]])
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool2", same_root=True)["pass"])
        # A retried attempt records other paths under the same root; the claim still binds.
        write_run(self.root / "pool4", "/srv/run-attempt-1", ["s0", "s1"], [["a", "b"], ["c", "X"]])
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool4", same_root=True)["pass"])

    def test_different_roots_require_equal_states_only(self):
        write_run(self.root / "ref", "/srv/runA", ["s0", "s1"], [["a"], ["c"]])
        write_run(self.root / "pool", "/srv/runB", ["s0", "s1"], [["a"], ["other"]])
        report = pp.compare(self.root / "ref", self.root / "pool")
        self.assertTrue(report["pass"])
        self.assertFalse(report["rows"][1]["trajectories_equal"])
        write_run(self.root / "pool3", "/srv/runB", ["s0", "sX"], [["a"], ["c"]])
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool3")["pass"])

    def test_a_missing_iteration_fails(self):
        write_run(self.root / "ref", "/r", ["s0", "s1"], [["a"], ["b"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]])
        report = pp.compare(self.root / "ref", self.root / "pool")
        self.assertFalse(report["pass"])
        self.assertEqual(report["rows"][1]["missing_in"], "pooled")


if __name__ == "__main__":
    unittest.main()
