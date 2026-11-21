"""Pooled-lineage proof comparison tests on synthetic run trees."""
import hashlib
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import pooled_proof as pp  # noqa: E402


def _pin(path: Path) -> dict:
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def write_run(root: Path, recorded_root: str, states, trajectories, backend="pooled-collection-shards-v1",
              planned=None, complete=True, resumed=()):
    """A synthetic run tree. Receipts record paths under `recorded_root` (the run may have been moved
    since); iterations in `resumed` publish their update in attempt-000001 beside the collection
    of attempt-000000, as a crash-resumed iteration does."""
    root.mkdir(parents=True, exist_ok=True)
    (root / "completion.json").write_text(json.dumps({
        "complete": complete, "planned_iterations": planned or len(states), "completed_iterations": len(states)}))
    for index, (state, hashes) in enumerate(zip(states, trajectories)):
        iteration = root / "iterations" / f"{index:06d}"
        collect = iteration / "attempt-000000" / "collect"
        update_dir = iteration / ("attempt-000001" if index in resumed else "attempt-000000") / "update"
        collect.mkdir(parents=True)
        update_dir.mkdir(parents=True)
        (update_dir / "update.json").write_text(json.dumps({
            "after_state_sha256": state,
            "checkpoint": {"path": f"{recorded_root}/iterations/{index:06d}/attempt-000000/update/checkpoint.json"},
            "trajectories": [{"path": "x", "sha256": h} for h in hashes]}))
        (collect / "collection.json").write_text(json.dumps({"collection_backend": backend}))
        def recorded(path):
            return {"path": recorded_root + "/" + path.relative_to(root).as_posix(),
                    "sha256": _pin(path)["sha256"]}
        (iteration / "complete.json").write_text(json.dumps({
            "update": recorded(update_dir / "update.json"), "collection": recorded(collect / "collection.json")}))


LOCAL = "native-cpu-parallel-episodes-v1"


def write_ref(*args, **kwargs):
    """A single-process reference run (the control)."""
    kwargs.setdefault("backend", LOCAL)
    write_run(*args, **kwargs)


class PooledProofTests(unittest.TestCase):
    def test_a_pooled_reference_is_refused(self):
        write_run(self.root / "ref", "/srv/run", ["s0"], [["a"]])
        write_run(self.root / "pool", "/srv/run", ["s0"], [["a"]])
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])


    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="pooled-proof-test-"))

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def test_same_root_requires_equal_states_and_trajectories(self):
        write_ref(self.root / "ref", "/srv/run", ["s0", "s1"], [["a", "b"], ["c", "d"]])
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
        write_ref(self.root / "ref", "/srv/runA", ["s0", "s1"], [["a"], ["c"]])
        write_run(self.root / "pool", "/srv/runB", ["s0", "s1"], [["a"], ["other"]])
        report = pp.compare(self.root / "ref", self.root / "pool")
        self.assertTrue(report["pass"])
        self.assertFalse(report["rows"][1]["trajectories_equal"])
        write_run(self.root / "pool3", "/srv/runB", ["s0", "sX"], [["a"], ["c"]])
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool3")["pass"])

    def test_a_missing_iteration_fails(self):
        write_ref(self.root / "ref", "/r", ["s0", "s1"], [["a"], ["b"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], planned=2)
        report = pp.compare(self.root / "ref", self.root / "pool")
        self.assertFalse(report["pass"])
        self.assertEqual(report["rows"][1]["missing_in"], "pooled")

    def test_the_same_iteration_missing_from_both_runs_fails(self):
        write_ref(self.root / "ref", "/r", ["s0"], [["a"]], planned=2)
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], planned=2)
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])

    def test_an_unfinished_run_fails(self):
        write_ref(self.root / "ref", "/r", ["s0"], [["a"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], complete=False)
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool")["pass"])

    def test_a_crash_resumed_iteration_is_read_through_its_completion_receipt(self):
        write_ref(self.root / "ref", "/srv/run", ["s0", "s1"], [["a"], ["b"]])
        write_run(self.root / "pool", "/srv/run", ["s0", "s1"], [["a"], ["b"]], resumed=(1,))
        self.assertTrue(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])

    def test_a_receipt_whose_pinned_file_changed_is_refused(self):
        write_ref(self.root / "ref", "/srv/run", ["s0"], [["a"]])
        write_run(self.root / "pool", "/srv/run", ["s0"], [["a"]])
        update = self.root / "pool" / "iterations" / "000000" / "attempt-000000" / "update" / "update.json"
        update.write_text(update.read_text().replace("s0", "s9"))
        with self.assertRaises(ValueError):
            pp.compare(self.root / "ref", self.root / "pool")

    def test_a_pooled_tree_collected_locally_fails(self):
        write_ref(self.root / "ref", "/r", ["s0"], [["a"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], backend="native-cpu-parallel-episodes-v1")
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])


if __name__ == "__main__":
    unittest.main()
