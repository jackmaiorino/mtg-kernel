"""Search probe driver: validity of paired gaps, CRN pairing, fingerprinted resume (collab LANES/searchprobe-20261111)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(REPO / "tools" / "blunder_census")]
import search_probe as sp  # noqa: E402


def price_row(id_, index, x, y, chosen, alt, invalid=(0, 0), unnatural=(0, 0)):
    arms = {"chosen": chosen, "alt": alt, "invalid": list(invalid), "unnatural": list(unnatural)}
    return {"kind": "price", "id": id_, "class": f"ordinary|{x}|{y}", "index": index, "worlds": arms, "true": arms}


class Validity(unittest.TestCase):
    def test_any_nonnatural_rollout_invalidates_the_pair(self):
        self.assertEqual(sp.paired([1, 1, 0, 1], [0, 1, 0, 0]), 0.5)
        # 3 of 4 pairs usable used to pass; the base mode's rule rejects it.
        self.assertIsNone(sp.paired([1, None, 0, 1], [0, 1, 0, 0]))
        self.assertIsNone(sp.paired([1, 1, 0, 1], [0, 1, 0, None]))
        self.assertIsNone(sp.paired([1], [0]))
        self.assertIsNone(sp.paired([1, 1], [0, 0, 0]))

    def test_null_scores_must_match_the_counts_and_repeats_reproduce(self):
        rows = [price_row("g", 3, 0, 2, [1, 0], [None, 1], unnatural=(0, 1)),
                price_row("g", 3, 4, 0, [0, 0], [1, 0])]
        scored, repeats = sp.action_scores(rows, "worlds")
        self.assertEqual(repeats, 1)
        self.assertEqual(scored[("g", 3)][2], [None, 1])
        with self.assertRaises(SystemExit):
            sp.action_scores([price_row("g", 3, 0, 2, [1, 0], [None, 1])], "worlds")
        with self.assertRaises(SystemExit):
            sp.action_scores([price_row("g", 3, 0, 2, [1, 0], [0, 1]), price_row("g", 3, 4, 0, [0, 0], [0, 1])],
                             "worlds")

    def test_pairs_share_the_state_id_and_index(self):
        row = {"id": "g", "index": 7, "class": "ordinary", "argmax": 1, "sampled": 1,
               "choosers": {"a": {"pick": 3}, "b": {"pick": 1}, "c": {"pick": 5}}}
        agree = {**row, "id": "h", "choosers": {"a": {"pick": 1}}}
        with tempfile.TemporaryDirectory() as d:
            out, singles = Path(d) / "p.jsonl", Path(d) / "s.jsonl"
            got = sp.build_price_cases([row, agree], out, singles)
            cases = [json.loads(x) for x in out.read_text().splitlines()]
            self.assertEqual(got["multi_cases"], 2)
            self.assertEqual({(c["id"], c["index"]) for c in cases}, {("g", 7)})
            self.assertEqual([c["class"] for c in cases], ["ordinary|1|3", "ordinary|5|1"])
            self.assertEqual(json.loads(singles.read_text())["class"], "ordinary|1|1")


class Resume(unittest.TestCase):
    def test_done_markers_from_another_configuration_stop_the_pass(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            exe = d / "exe.bin"
            exe.write_bytes(b"one")
            jobs = d / "jobs.jsonl"
            jobs.write_text("{}\n")
            states = d / "states.jsonl"
            states.write_text(json.dumps({"id": "g", "class": "ordinary", "index": 1}) + "\n")
            out = d / "out"
            out.mkdir()
            env = {"PRICE_MODE": "price", "PRICE_N": "64"}
            fp = sp.fingerprint(str(exe), jobs, env)
            p = out / "states-t-0000.jsonl"
            p.write_text(states.read_text())
            rows = out / "rows-t-0000.jsonl"
            rows.write_text(json.dumps({"kind": "price", "id": "g", "class": "ordinary", "index": 1}) + "\n")
            marker = {"chunk": 0, "file": rows.name, "states_file": p.name, "exit": 0, "states": 1, "rows": 1,
                      "errors": 0, "seconds": 0.0, "complete": True, "fingerprint": fp,
                      "states_sha256": sp.file_sha(p)}
            (out / "done-t-0000.json").write_text(json.dumps(marker))
            # Same configuration: the chunk is reused, nothing runs.
            rec = sp.run_chunks(str(exe), states, jobs, out, "t", 1, env, 1)
            self.assertTrue(rec["complete"])
            self.assertEqual(rec["fingerprint"], fp)
            # Another N, another executable, or a marker without a fingerprint: refused.
            for change in ({"env": {**env, "PRICE_N": "96"}}, {"exe": b"two"}, {"strip": True}):
                if "exe" in change:
                    exe.write_bytes(change["exe"])
                if "strip" in change:
                    exe.write_bytes(b"one")
                    (out / "done-t-0000.json").write_text(json.dumps({k: v for k, v in marker.items()
                                                                       if k != "fingerprint"}))
                with self.assertRaises(SystemExit):
                    sp.run_chunks(str(exe), states, jobs, out, "t", 1, change.get("env", env), 1)


if __name__ == "__main__":
    unittest.main()
