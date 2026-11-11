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


def write_pass(d: Path, tag: str, rows: list[dict], fp: dict | None):
    """A complete one-chunk pass as run_chunks writes it; fp None writes it as the pre-fingerprint driver did."""
    d.mkdir(parents=True, exist_ok=True)
    states = d / f"states-{tag}-0000.jsonl"
    states.write_text("".join(json.dumps({"id": r["id"], "class": r["class"], "index": r["index"]}) + "\n"
                              for r in rows))
    (d / f"rows-{tag}-0000.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows))
    chunk = {"chunk": 0, "file": f"rows-{tag}-0000.jsonl", "states_file": states.name, "exit": 0,
             "states": len(rows), "rows": len(rows), "errors": 0, "seconds": 1.0, "complete": True}
    if fp is not None:
        chunk.update({"fingerprint": fp, "states_sha256": sp.file_sha(states)})
    (d / f"done-{tag}-0000.json").write_text(json.dumps(chunk))
    plain = {k: (v["path"] if isinstance(v, dict) else v) for k, v in (fp or {"env": {}})["env"].items()}
    record = {"tag": tag, "expected": len(rows), "chunks": [chunk], "complete": True,
              "exe_sha256": (fp or {}).get("exe_sha256"), "env": plain}
    if fp is not None:
        record["fingerprint"] = fp
    (d / f"run-{tag}.json").write_text(json.dumps(record))


class Fixture:
    def __init__(self, d: Path):
        self.d = d
        self.exe = d / "exe.bin"
        self.exe.write_bytes(b"exe")
        self.jobs = d / "jobs.jsonl"
        self.jobs.write_text("{}\n")
        self.sources = d / "sources.json"
        self.sources.write_text("{}")
        self.choosers = d / "choosers.json"
        self.choosers.write_text("[]")

    def env(self, mode):
        e = {"PRICE_MODE": mode, "PRICE_SOURCES": str(self.sources)}
        if mode == "search":
            e["PROBE_CHOOSERS"] = str(self.choosers)
        else:
            e.update({"PRICE_N": "2", "PRICE_N_TRUE": "2"})
        return e

    def fp(self, mode):
        return sp.fingerprint(str(self.exe), self.jobs, self.env(mode))


def search_row(id_, argmax, picks):
    return {"kind": "search", "id": id_, "class": "ordinary", "index": 1, "argmax": argmax, "sampled": argmax,
            "choosers": {n: {"pick": p, "fallback": False, "cpu": 1.0, "completed": 9, "incomplete": False,
                             "faults": [], "extra": None} for n, p in picks.items()}}


class Provenance(unittest.TestCase):
    def test_consumers_refuse_unattested_or_mismatched_passes(self):
        with tempfile.TemporaryDirectory() as d:
            f = Fixture(Path(d))
            rows = [search_row("g", 0, {"a": 0})]
            ok = f.d / "ok"
            write_pass(ok, "s", rows, f.fp("search"))
            self.assertEqual(len(sp.pass_rows(ok, "s")), 1)
            # A pre-fingerprint pass is refused by the report and case paths until attested.
            legacy = f.d / "legacy"
            write_pass(legacy, "s", rows, None)
            rec = json.loads((legacy / "run-s.json").read_text())
            rec.update({"exe_sha256": sp.file_sha(f.exe), "env": f.env("search")})
            (legacy / "run-s.json").write_text(json.dumps(rec))
            with self.assertRaises(SystemExit):
                sp.pass_rows(legacy, "s")
            with self.assertRaises(SystemExit):
                sp.build_price_cases(sp.pass_rows(legacy, "s"), f.d / "p.jsonl", f.d / "q.jsonl")
            sp.attest(legacy, "s", str(f.exe), f.jobs, f.env("search"), "test")
            self.assertEqual(len(sp.pass_rows(legacy, "s")), 1)
            # An attestation made under other inputs disagrees with the run record.
            sp.attest(legacy, "s", str(f.exe), f.jobs, {**f.env("search"), "PROBE_CHOOSERS": str(f.sources)}, "x")
            with self.assertRaises(SystemExit):
                sp.pass_rows(legacy, "s")
            # A chunk whose states file changed after the pass, or whose marker is gone, is refused.
            (ok / "states-s-0000.jsonl").write_text("{}\n")
            with self.assertRaises(SystemExit):
                sp.pass_rows(ok, "s")
            other = f.d / "other"
            write_pass(other, "s", rows, f.fp("search"))
            (other / "done-s-0000.json").unlink()
            with self.assertRaises(SystemExit):
                sp.pass_rows(other, "s")


class Agreement(unittest.TestCase):
    def test_agreement_counts_zero_only_with_a_clean_baseline(self):
        with tempfile.TemporaryDirectory() as d:
            f = Fixture(Path(d))
            root = f.d / "run"
            srows = [search_row("g", 0, {"a": 0, "b": 1}), search_row("h", 0, {"a": 0, "b": 1})]
            write_pass(root / "search-ordinary", "search-ordinary", srows, f.fp("search"))
            prices = [{**price_row("g", 1, 0, 1, [0, 1], [1, 1]), "class": "ordinary|0|1"},
                      {**price_row("h", 1, 0, 1, [None, 1], [1, 1], unnatural=(1, 0)), "class": "ordinary|0|1"}]
            write_pass(root / "price-ordinary", "price-ordinary", prices, f.fp("price"))
            out = sp.report(root, f.d / "report")
            ch = out["banks"]["ordinary"]["choosers"]
            # h's baseline has a non-natural rollout: invalid for every chooser, agreeing or not.
            self.assertEqual((ch["a"]["worlds"]["states"], ch["a"]["worlds"]["invalid"]), (1, 1))
            self.assertEqual((ch["b"]["worlds"]["states"], ch["b"]["worlds"]["invalid"]), (1, 1))
            self.assertEqual(ch["b"]["worlds"]["mean_gap"], 0.5)
            self.assertEqual(out["banks"]["ordinary"]["contrasts"], {})
            self.assertEqual(out["banks"]["ordinary"]["rollouts_per_action"]["price-ordinary/worlds"], 2)

    def test_a_state_with_an_incomplete_search_is_excluded_for_every_chooser(self):
        with tempfile.TemporaryDirectory() as d:
            f = Fixture(Path(d))
            root = f.d / "run"
            bad = search_row("h", 0, {"a": 0, "b": 1})
            bad["choosers"]["b"]["incomplete"] = True
            write_pass(root / "search-ordinary", "search-ordinary", [search_row("g", 0, {"a": 0, "b": 1}), bad],
                       f.fp("search"))
            prices = [{**price_row(i, 1, 0, 1, [0, 1], [1, 1]), "class": "ordinary|0|1"} for i in ("g", "h")]
            write_pass(root / "price-ordinary", "price-ordinary", prices, f.fp("price"))
            out = sp.report(root, f.d / "report")["banks"]["ordinary"]
            self.assertEqual((out["states"], out["states_searched"]), (1, 2))
            self.assertEqual(out["failed_search_states"], [{"id": "h", "index": 1, "class": "ordinary",
                                                            "choosers": ["b"]}])
            self.assertEqual(out["choosers"]["a"]["worlds"]["states"], 1)
            self.assertEqual(out["choosers"]["b"]["worlds"]["mean_gap"], 0.5)


if __name__ == "__main__":
    unittest.main()
