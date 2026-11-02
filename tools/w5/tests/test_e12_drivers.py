"""E12 drivers: the logit parity dump, extraction and comparison (with a fake replay binary), and the H look's
admission and plan."""
import json
import struct
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import h_look as hl  # noqa: E402
import logit_parity as lp  # noqa: E402

FAKE_REPLAY = r'''
import json, os, struct, sys
args = dict(a.split("=", 1) for a in sys.argv[1:])
shift = float(os.environ.get("FAKE_SHIFT", "0"))
out = open(args["out"], "w")
tensors = open(os.environ["TD_BIN"], "ab")
for line in open(os.environ["LABEL_JOBS"]):
    job = json.loads(line)
    block, rows = b"", []
    for i, d in enumerate(job["decisions"]):
        if d["actor"] != job["focal"]:
            continue
        n = d["legal_action_count"]
        start = len(block)
        block += struct.pack("<8I", 1, 0, n, 0, 3, d["selected"], n, job["focal"])
        block += struct.pack("<3f", 0.5, float(i), 1.0)
        logits = [0.1 * a + (shift if a == 0 else 0.0) for a in range(n)]
        block += struct.pack(f"<{n}f", *logits) + struct.pack("<f", 0.25)
        rows.append([i, n, d["selected"], 1, start, len(block) - start])
    offset = tensors.tell()
    tensors.write(block)
    out.write(json.dumps({"kind": "td_replay", "id": job["id"], "bin_offset": offset, "bin_bytes": len(block),
                          "decisions": rows}) + "\n")
'''


def jobs(count, per_game=6):
    return [{"id": f"g{i:03d}", "focal": i % 2,
             "decisions": [{"actor": d % 2, "legal_action_count": 1 + d % 4, "selected": 0, "step": d,
                            "physical_decision_id": d} for d in range(per_game)]} for i in range(count)]


class FixedStatesTests(unittest.TestCase):
    def test_jobs_in_id_order_until_the_minimum(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "jobs.jsonl"
            path.write_text("".join(json.dumps(j) + "\n" for j in reversed(jobs(40))))
            chosen, states = lp.fixed_states([path], 20)
            self.assertEqual([j["id"] for j in chosen], [f"g{i:03d}" for i in range(len(chosen))])
            self.assertGreaterEqual(states, 20)
            self.assertLess(states - lp.scored_states(chosen[-1]), 20)
            with self.assertRaises(lp.ParityError):
                lp.fixed_states([path], 10_000)


class DumpCompareTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        self.fake = root / "fake_replay.py"
        self.fake.write_text(FAKE_REPLAY)
        self.jobs = root / "fixed.jsonl"
        self.jobs.write_text("".join(json.dumps(j) + "\n" for j in jobs(10)))
        self.source = root / "source.json"
        self.source.write_text("{}")
        self.ckpt = root / "ckpt.bin"
        self.ckpt.write_bytes(b"weights")
        self.root = root

    def tearDown(self):
        self.tmp.cleanup()

    def dump(self, name, shift=0.0, host=None):
        import os
        os.environ["FAKE_SHIFT"] = str(shift)
        command = [sys.executable, str(self.fake), "source={source}", "out={out}"]
        record = lp.dump(Path(sys.executable), self.source, [self.ckpt], self.jobs, self.root / name, command)
        if host:  # CI runs one OS: label the sides for the comparison test
            record["host_class"] = host
            (self.root / name / "dump.json").write_text(json.dumps(record))
        return record

    def test_extract_reads_logits_value_and_inputs(self):
        record = self.dump("a")
        states = [json.loads(x) for x in (self.root / "a" / "logits.jsonl").read_text().splitlines()]
        self.assertEqual(record["decisions"], len(states))
        first = states[0]
        self.assertEqual(first["legal"], 1)
        self.assertAlmostEqual(first["value"], 0.25)
        self.assertEqual(len(states[1]["logits"]), states[1]["legal"])

    def test_compare_pass_shift_and_refusals(self):
        self.dump("lin", host="linux-zen5-9950x")
        self.dump("win", shift=2e-6, host="windows-amd-ryzen-7-3700x")
        self.dump("far", shift=0.5, host="windows-amd-ryzen-7-3700x")
        ok = lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "win"), 1e-5)
        self.assertTrue(ok["checks"]["max_abs_logit_within_tolerance"])
        self.assertTrue(ok["checks"]["argmax_equal_except_ties"])
        self.assertFalse(ok["checks"]["states_at_least_10000"])  # a small fixture
        self.assertAlmostEqual(ok["max_abs_logit_diff"], 2e-6, delta=1e-6)
        far = lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "far"), 1e-5)
        self.assertFalse(far["checks"]["max_abs_logit_within_tolerance"])
        self.assertTrue(far["argmax_breaks"])
        with self.assertRaises(lp.ParityError):  # both sides Linux
            lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "lin"), 1e-5)

    def test_ties_within_tolerance_do_not_break_argmax(self):
        a = {"job": "g", "decision": 0, "legal": 2, "input_sha256": "x", "logits": [1.0, 1.0 + 4e-6], "value": 0.0}
        b = dict(a, logits=[1.0 + 4e-6, 1.0])
        rec = {"host_class": "linux-x", "jobs_sha256": "j", "pins": ["p"], "hostname": "", "binary": {},
               "logits_sha256": "", "tensors_sha256": ""}
        out = lp.compare((rec, [a]), (dict(rec, host_class="windows-y"), [b]), 1e-5)
        self.assertEqual(out["argmax_ties_within_tolerance"], 1)
        self.assertTrue(out["checks"]["argmax_equal_except_ties"])


def parity(ckpt="c", census="e", linux="linux-zen5-9950x", windows="windows-cpu"):
    return {"schema": "w5-logit-parity/v1", "pass": True, "checkpoint_pins": [ckpt], "max_abs_logit_diff": 0.0,
            "states": 10_000, "linux": {"host_class": linux}, "windows": {"host_class": windows,
                                                                          "binary": {"sha256": census}}}


class LookTests(unittest.TestCase):
    endpoint = {"schema": hl.ENDPOINT_SCHEMA, "chain": "w5/chain1", "arm": "X8", "look": "50pct",
                "training_host_class": "linux-zen5-9950x", "checkpoint": {"sha256": "c"}}
    panel = {"schema": hl.PANEL_SCHEMA, "namespace": hl.NAMESPACE, "census": {"sha256": "e"}}

    def test_admission(self):
        hl.admit(self.endpoint, self.panel, parity(), "windows-cpu")
        for bad in (parity(ckpt="other"), parity(census="other"), parity(linux="linux-other"),
                    parity(windows="windows-other"), dict(parity(), **{"pass": False})):
            with self.assertRaises(hl.LookError):
                hl.admit(self.endpoint, self.panel, bad, "windows-cpu")
        with self.assertRaises(hl.LookError):
            hl.admit(self.endpoint, dict(self.panel, namespace="w4c-panel-20261031b"), parity(), "windows-cpu")
        with self.assertRaises(hl.LookError):
            hl.admit(self.endpoint, self.panel, parity(), "linux-zen5-9950x")

    def test_plan_builds_the_frozen_panel(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)

            def pinned(name, value):
                path = tmp / name
                path.write_text(json.dumps(value) if not isinstance(value, bytes) else "")
                return {"path": str(path), "sha256": lp.sha256_file(path)}
            ckpt = pinned("ckpt.bin", {"w": 1})
            this_host = "windows-test-cpu"
            endpoint = dict(self.endpoint, checkpoint=ckpt, source=pinned("source.json", {"checkpoint": "x"}))
            panel = dict(self.panel, opponent=pinned("h.json", {"name": "H"}), shards=4,
                         decks=pinned("decks.json", [{"label": f"d{i}", "mainboard": [i]} for i in range(9)]))
            files = {}
            for name, value in (("endpoint.json", endpoint), ("panel.json", panel),
                                ("parity.json", parity(ckpt=ckpt["sha256"], windows=this_host))):
                files[name] = tmp / name
                files[name].write_text(json.dumps(value))
            original = hl.host_class
            hl.host_class = lambda: this_host
            try:
                look = hl.plan(files["endpoint.json"], files["panel.json"], files["parity.json"], tmp / "look")
            finally:
                hl.host_class = original
            plan = json.loads((tmp / "look" / "plan.json").read_text())
            self.assertEqual(plan["seed_namespace"], hl.NAMESPACE)
            self.assertEqual(plan["cases"], 1944)
            self.assertEqual(plan["opponent_name"], "H")
            self.assertEqual(look["host_class"], this_host)
            self.assertIn("w5_look", plan)


if __name__ == "__main__":
    unittest.main()
