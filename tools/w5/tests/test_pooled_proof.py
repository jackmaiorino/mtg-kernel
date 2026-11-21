"""Pooled-lineage proof comparison tests on synthetic run trees."""
import hashlib
import importlib
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
pp = importlib.import_module("pooled_proof")


def _pin(path: Path) -> dict:
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def write_run(root: Path, recorded_root: str, states, trajectories, backend="pooled-collection-shards-v1",
              planned=None, complete=True, resumed=()):
    """A synthetic run tree. Receipts record paths under `recorded_root` (the run may have been moved
    since); iterations in `resumed` publish their update in attempt-000001 beside the collection
    of attempt-000000, as a crash-resumed iteration does."""
    root.mkdir(parents=True, exist_ok=True)
    planned = planned or len(states)
    plan = [
        {"episodes": [{"episode": {"id": f"game-{index}-{slot}", "seed": slot}}
                      for slot in range(len(trajectories[min(index, len(trajectories) - 1)]))]}
        for index in range(planned)
    ]
    (root / "run.json").write_text(json.dumps({"schema": pp.RUN_SCHEMA, "config": {"iterations": plan}}))

    def recorded(path):
        return {"path": recorded_root + "/" + path.relative_to(root).as_posix(),
                "sha256": _pin(path)["sha256"]}

    def sha(value):
        return hashlib.sha256(value.encode()).hexdigest()

    markers = []
    before = sha("initial")
    for index, (state, hashes) in enumerate(zip(states, trajectories)):
        iteration = root / "iterations" / f"{index:06d}"
        collect = iteration / "attempt-000000" / "collect"
        update_dir = iteration / ("attempt-000001" if index in resumed else "attempt-000000") / "update"
        collect.mkdir(parents=True)
        update_dir.mkdir(parents=True)
        after = sha(state)
        pins = [{"path": f"{recorded_root}/{collect.relative_to(root).as_posix()}/episode-{slot:04d}.json",
                 "sha256": sha(h)} for slot, h in enumerate(hashes)]
        checkpoint = {"path": f"{recorded_root}/{update_dir.relative_to(root).as_posix()}/checkpoint.json",
                      "sha256": sha(f"checkpoint-{state}")}
        (update_dir / "update.json").write_text(json.dumps({
            "complete": True, "before_state_sha256": before, "after_state_sha256": after,
            "checkpoint": checkpoint, "trajectories": pins}))
        (collect / "collection.json").write_text(json.dumps({
            "complete": True, "behavior_state_sha256": before, "trajectories": pins,
            "collection_backend": backend}))
        identity = {"state_sha256": after, "checkpoint_sha256": checkpoint["sha256"]}
        marker = iteration / "complete.json"
        marker.write_text(json.dumps({
            "schema": "mtg-kernel-native-expanded-iteration/v1", "iteration": index,
            "episodes_sha256": sha(f"resolved-episodes-{index}"), "output_identity": identity,
            "update": recorded(update_dir / "update.json"), "collection": recorded(collect / "collection.json")}))
        markers.append(recorded(marker))
        before = after
    (root / "completion.json").write_text(json.dumps({
        "schema": pp.RUN_SCHEMA, "complete": complete, "planned_iterations": planned,
        "completed_iterations": len(states), "iterations": markers, "actual_identity": identity}))


def edit_json(path, edit):
    data = json.loads(path.read_text())
    edit(data)
    path.write_text(json.dumps(data))


def write_ref(*args, **kwargs):
    """A single-process reference run (the control)."""
    kwargs.setdefault("backend", "native-cpu-parallel-episodes-v1")
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

    def test_a_backup_directory_cannot_replace_a_missing_planned_iteration(self):
        for name, writer in [("ref", write_ref), ("pool", write_run)]:
            root = self.root / name
            writer(root, "/r", ["s0", "s1"], [["a"], ["b"]])
            its = root / "iterations"
            shutil.rmtree(its / "000000")
            shutil.copytree(its / "000001", its / "000001.backup")
        with self.assertRaises(FileNotFoundError):
            pp.compare(self.root / "ref", self.root / "pool", same_root=True)

    def test_pins_must_point_to_the_iteration_they_confirm(self):
        ref = self.root / "ref"
        write_ref(ref, "/r", ["s", "s"], [["a"], ["a"]])
        for field in ("complete", "iteration", "collection", "update"):
            with self.subTest(field=field):
                pool = self.root / field
                write_run(pool, "/r", ["s", "s"], [["a"], ["a"]], resumed=(1,))
                completion = pool / "completion.json"
                done = json.loads(completion.read_text())
                if field == "complete":
                    done["iterations"][1] = done["iterations"][0]
                else:
                    its = pool / "iterations"
                    first = json.loads((its / "000000" / "complete.json").read_text())
                    receipt = its / "000001" / "complete.json"
                    second = json.loads(receipt.read_text())
                    second[field] = first[field]
                    receipt.write_text(json.dumps(second))
                    done["iterations"][1]["sha256"] = _pin(receipt)["sha256"]
                completion.write_text(json.dumps(done))
                with self.assertRaises(ValueError):
                    pp.compare(ref, pool, same_root=True)

    def test_an_unfinished_run_fails(self):
        write_ref(self.root / "ref", "/r", ["s0"], [["a"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], complete=False)
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool")["pass"])

    def test_receipts_under_an_ancestor_named_iterations_relocate_to_the_run(self):
        write_ref(self.root / "ref", "/srv/iterations/run", ["s0", "s1"], [["a"], ["b"]])
        write_run(self.root / "pool", "/srv/iterations/run", ["s0", "s1"], [["a"], ["b"]])
        self.assertTrue(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])

    def test_a_crash_resumed_iteration_is_read_through_its_completion_receipt(self):
        write_ref(self.root / "ref", "/srv/run", ["s0", "s1"], [["a"], ["b"]])
        write_run(self.root / "pool", "/srv/run", ["s0", "s1"], [["a"], ["b"]], resumed=(1,))
        self.assertTrue(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])

    def test_a_receipt_whose_pinned_file_changed_is_refused(self):
        write_ref(self.root / "ref", "/srv/run", ["s0"], [["a"]])
        write_run(self.root / "pool", "/srv/run", ["s0"], [["a"]])
        update = self.root / "pool" / "iterations" / "000000" / "attempt-000000" / "update" / "update.json"
        edit_json(update, lambda doc: doc.update(after_state_sha256="9" * 64))
        with self.assertRaises(ValueError):
            pp.compare(self.root / "ref", self.root / "pool")

    def test_a_pooled_tree_collected_locally_fails(self):
        write_ref(self.root / "ref", "/r", ["s0"], [["a"]])
        write_run(self.root / "pool", "/r", ["s0"], [["a"]], backend="native-cpu-parallel-episodes-v1")
        self.assertFalse(pp.compare(self.root / "ref", self.root / "pool", same_root=True)["pass"])


    def test_episode_schedule_matters_even_when_the_state_matches(self):
        ref, pool = self.root / "ref", self.root / "pool"
        write_run(ref, "/r", ["s0"], [["a", "b"]], backend=None)
        write_run(pool, "/r", ["s0"], [["a", "b"]])
        edit_json(pool / "run.json", lambda doc: doc["config"]["iterations"][0]["episodes"].reverse())
        self.assertFalse(pp.compare(ref, pool)["pass"])

    def test_update_and_collection_must_agree_on_ordered_trajectory_pins(self):
        ref, pool = self.root / "ref", self.root / "pool"
        write_run(ref, "/r", ["s0"], [["a", "b"]], backend=None)
        write_run(pool, "/r", ["s0"], [["a", "b"]], resumed=(0,))
        iteration = pool / "iterations" / "000000"
        collection = iteration / "attempt-000000" / "collect" / "collection.json"
        edit_json(collection, lambda doc: doc["trajectories"].reverse())
        edit_json(iteration / "complete.json", lambda doc: doc["collection"].update(
            sha256=_pin(collection)["sha256"]))
        edit_json(pool / "completion.json", lambda doc: doc["iterations"][0].update(
            sha256=_pin(iteration / "complete.json")["sha256"]))
        with self.assertRaisesRegex(ValueError, "trajectory pins"):
            pp.compare(ref, pool, same_root=True)


if __name__ == "__main__":
    unittest.main()
