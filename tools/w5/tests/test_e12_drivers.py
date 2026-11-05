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
        block += struct.pack("<3f", 0.5, float(i), 1.0)                 # state
        block += struct.pack("<98f", *([0.25] * 98))                     # one object's features
        block += struct.pack("<3i", 7, 0, 1)                             # its card id, group, node id
        block += struct.pack(f"<{195 * n}f", *([0.5] * (195 * n)))       # action features (no edges, no refs)
        logits = [0.1 * a + (shift if a == 0 else 0.0) for a in range(n)]
        block += struct.pack(f"<{n}f", *logits) + struct.pack("<f", 0.25)
        rows.append([i, n, d["selected"], 1, start, len(block) - start])
    offset = tensors.tell()
    tensors.write(block)
    out.write(json.dumps({"kind": "td_replay", "id": job["id"], "bin_offset": offset, "bin_bytes": len(block),
                          "decisions": rows}) + "\n")
'''


FAKE_SCORER = r"""
import json, os, struct, sys
args = sys.argv[1:]
config = json.load(open(args[args.index("--config") + 1]))
shift = float(os.environ.get("FAKE_SCORER_SHIFT", "0"))
bits = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]
out = sys.stdout.buffer
out.write(json.dumps({"schema": "mtg-kernel-spellbench-scorer-ready/v1", "model_state_sha256": "m",
                      "feature_contract_digest": "c", "feature_encoding_digest": "e",
                      "selection": config["selection"]}).encode() + b"\n")
out.flush()
for line in sys.stdin.buffer:
    assert line.endswith(b"}\n"), line[-4:]
    r = json.loads(line)
    t = r["tensor"]
    n = len(r["row_candidate_ids"])
    step = r["step"]
    # the fake replay's record, field by field (floats as u32 bits)
    assert t["state"] == [bits(0.5), bits(float(step)), bits(1.0)], t["state"]
    assert t["object_features"] == [bits(0.25)] * 98 and t["object_card_ids"] == [7]
    assert t["object_groups"] == [0] and t["object_node_ids"] == [1] and t["edge_features"] == []
    assert t["action_features"] == [bits(0.5)] * (195 * n) and t["action_ref_card_ids"] == []
    assert r["feature_contract_digest"] == "c" and r["seat"] in ("p0", "p1")
    assert (r["sample_seed"] is None) == (config["selection"] == "argmax-first-v1")
    logits = [bits(0.1 * a + (shift if a == 0 else 0.0)) for a in range(n)]
    out.write(json.dumps({"schema": "mtg-kernel-spellbench-scorer-choice/v1", "logits_bits": logits,
                          "value_bits": bits(0.25), "selected_row": 0}).encode() + b"\n")
    out.flush()
"""


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
        self.ckpt = root / "ckpt.bin"
        self.ckpt.write_bytes(b"weights")
        self.source = root / "source.json"
        self.source.write_text(json.dumps({"kind": "checkpoint", "checkpoint": "ckpt.bin"}))
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
        ok = lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "win"))
        self.assertTrue(ok["checks"]["max_abs_logit_within_tolerance"])
        self.assertTrue(ok["checks"]["argmax_equal_except_ties"])
        self.assertFalse(ok["checks"]["states_at_least_10000"])  # a small fixture
        self.assertAlmostEqual(ok["max_abs_logit_diff"], 2e-6, delta=1e-6)
        far = lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "far"))
        self.assertTrue(far["checks"]["input_tensors_identical"])
        with self.assertRaises(lp.ParityError):  # the tolerance cannot be widened
            lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "far"), 0.02)
        self.assertFalse(far["checks"]["max_abs_logit_within_tolerance"])
        self.assertTrue(far["argmax_breaks"])
        with self.assertRaises(lp.ParityError):  # both sides Linux
            lp.compare(lp.load_dump(self.root / "lin"), lp.load_dump(self.root / "lin"))

    def test_ties_within_tolerance_do_not_break_argmax(self):
        a = {"job": "g", "decision": 0, "legal": 2, "input_sha256": "x", "logits": [1.0, 1.0 + 4e-6], "value": 0.0}
        b = dict(a, logits=[1.0 + 4e-6, 1.0])
        rec = {"host_class": "linux-x", "jobs_sha256": "j", "pins": ["p"], "hostname": "", "binary": {},
               "logits_sha256": "", "tensors_sha256": "", "source_identity": "s", "command_files": {}}
        out = lp.compare((rec, [a]), (dict(rec, host_class="windows-y"), [b]))
        self.assertEqual(out["argmax_ties_within_tolerance"], 1)
        self.assertTrue(out["checks"]["argmax_equal_except_ties"])
        # a small top-two gap never excuses a switch to a third action far below the first winner
        c = dict(a, legal=3, logits=[0.0, -1e-6, -11e-6])
        d = dict(c, logits=[-9e-6, -10e-6, -2e-6])
        third = lp.compare((rec, [c]), (dict(rec, host_class="windows-y"), [d]))
        self.assertFalse(third["checks"]["argmax_equal_except_ties"])

    def test_records_must_match_their_layout_and_be_finite(self):
        n = 2
        good = (struct.pack("<8I", 1, 0, n, 0, 3, 0, n, 0) + struct.pack("<3f", 0, 0, 0)
                + struct.pack("<98f", *([0.0] * 98)) + struct.pack("<3i", 1, 0, 0)
                + struct.pack(f"<{195 * n}f", *([0.0] * 195 * n)))
        lp.decode(good + struct.pack("<2f", 0.1, 0.2) + struct.pack("<f", 0.0))
        with self.assertRaises(lp.ParityError):
            lp.decode(good + struct.pack("<2f", 0.1, 0.2))                     # truncated
        with self.assertRaises(lp.ParityError):
            lp.decode(good + struct.pack("<2f", float("nan"), 0.2) + struct.pack("<f", 0.0))

    def test_cli_dump_with_a_command(self):
        import os
        os.environ["FAKE_SHIFT"] = "0"
        command = json.dumps([sys.executable, str(self.fake), "source={source}", "out={out}"])
        code = lp.main(["dump", "--binary", sys.executable, "--source", str(self.source), "--pin", str(self.ckpt),
                        "--jobs", str(self.jobs), "--out", str(self.root / "cli"), "--command", command])
        self.assertEqual(code, 0)
        record = json.loads((self.root / "cli" / "dump.json").read_text())
        self.assertIn(str(self.fake), record["command_files"])

    def test_source_identity_ignores_host_paths_not_settings(self):
        other_dir = self.root / "elsewhere"
        other_dir.mkdir()
        (other_dir / "ckpt.bin").write_bytes(b"weights")
        moved = other_dir / "source.json"
        moved.write_text(json.dumps({"kind": "checkpoint", "checkpoint": "ckpt.bin"}))
        self.assertEqual(lp.source_identity(self.source), lp.source_identity(moved))
        moved.write_text(json.dumps({"kind": "checkpoint", "checkpoint": "ckpt.bin", "temperature": 0.5}))
        self.assertNotEqual(lp.source_identity(self.source), lp.source_identity(moved))

    def test_dump_binds_the_loaded_checkpoint_and_binary(self):
        other = self.root / "other.bin"
        other.write_bytes(b"other weights")
        command = [sys.executable, str(self.fake), "source={source}", "out={out}"]
        with self.assertRaises(lp.ParityError):  # the source does not load this pin
            lp.dump(Path(sys.executable), self.source, [other], self.jobs, self.root / "x", command)
        with self.assertRaises(lp.ParityError):  # the command does not run the pinned binary
            lp.dump(self.fake, self.source, [self.ckpt], self.jobs, self.root / "y", command)


    def scorer_case(self, shift=0.0, selection="sampled-wide-v1", source=None):
        import os
        if not (self.root / "d").exists():
            self.dump("d")
        fake = self.root / "fake_scorer.py"
        fake.write_text(FAKE_SCORER)
        config = self.root / "bot.scorer.json"
        config.write_text(json.dumps({"schema": "mtg-kernel-spellbench-scorer-config/v1",
                                      "source": source or json.loads(self.source.read_text()),
                                      "selection": selection}))
        os.environ["FAKE_SCORER_SHIFT"] = str(shift)
        return lp.scorer_check(self.root / "d", self.jobs, Path(sys.executable), config, [str(fake)])

    def test_scorer_check_requires_the_dumped_forward_bit_for_bit(self):
        ok = self.scorer_case()
        self.assertTrue(ok["pass"])
        self.assertEqual(ok["decisions"], 30)
        self.assertEqual(ok["states"], 20)
        self.assertEqual(ok["max_abs_logit_diff"], 0.0)
        self.assertEqual(ok["ready"]["model_state_sha256"], "m")
        self.assertTrue(self.scorer_case(selection="argmax-first-v1")["pass"])
        off = self.scorer_case(shift=1e-7)  # one float32 step away still fails
        self.assertFalse(off["pass"])
        self.assertFalse(off["checks"]["logits_bit_identical"])
        self.assertTrue(off["checks"]["values_bit_identical"])
        self.assertEqual(off["logit_mismatches"], 30)
        with self.assertRaises(lp.ParityError):  # the scorer must load the dump's source
            self.scorer_case(source={"kind": "checkpoint", "checkpoint": "ckpt.bin", "temperature": 0.5})

    def test_scorer_check_refuses_a_failing_scorer(self):
        fake = self.root / "failing_scorer.py"
        fake.write_text(FAKE_SCORER + "\nsys.exit(17)\n")
        self.dump("d")
        config = self.root / "bot.scorer.json"
        config.write_text(json.dumps({"source": json.loads(self.source.read_text()), "selection": "sampled-wide-v1"}))
        with self.assertRaises(lp.ParityError):
            lp.scorer_check(self.root / "d", self.jobs, Path(sys.executable), config, [str(fake)])

    def test_decode_request_reads_every_field_in_order_with_signs(self):
        objects, edges, actions, refs, state = 1, 1, 2, 1, 2
        f = lambda *xs: struct.pack(f"<{len(xs)}f", *xs)  # noqa: E731
        i = lambda *xs: struct.pack(f"<{len(xs)}i", *xs)  # noqa: E731
        block = (struct.pack("<8I", objects, edges, actions, refs, state, 1, actions, 1)
                 + f(-0.0, 2.5) + f(*([-1.5] * 98)) + i(-1) + i(-2) + i(3)
                 + f(*([0.75] * 41)) + i(0) + i(-1)
                 + f(*([1.0] * 195), *([-2.0] * 195)) + f(*([0.5] * 25)) + i(9) + i(1) + i(-1)
                 + f(0.25, -0.125) + f(-0.5))
        tensor, logits, value, actor = lp.decode_request(block)
        bits = lambda x: struct.unpack("<I", struct.pack("<f", x))[0]  # noqa: E731
        self.assertEqual(tensor["state"], [0x80000000, bits(2.5)])
        self.assertEqual(tensor["object_features"], [bits(-1.5)] * 98)
        self.assertEqual((tensor["object_card_ids"], tensor["object_groups"], tensor["object_node_ids"]),
                         ([-1], [-2], [3]))
        self.assertEqual(tensor["edge_features"], [bits(0.75)] * 41)
        self.assertEqual((tensor["edge_source_indices"], tensor["edge_target_indices"]), ([0], [-1]))
        self.assertEqual(tensor["action_features"], [bits(1.0)] * 195 + [bits(-2.0)] * 195)
        self.assertEqual(tensor["action_ref_features"], [bits(0.5)] * 25)
        self.assertEqual((tensor["action_ref_card_ids"], tensor["action_ref_action_indices"],
                          tensor["action_ref_node_indices"]), ([9], [1], [-1]))
        self.assertEqual((logits, value, actor), ([bits(0.25), bits(-0.125)], bits(-0.5), 1))
        self.assertEqual(len(tensor), len(lp.TENSOR_FIELDS))

    def test_scorer_check_cli(self):
        import os
        self.dump("d")
        fake = self.root / "fake_scorer.py"
        fake.write_text(FAKE_SCORER)
        (self.root / "bot.scorer.json").write_text(json.dumps(
            {"schema": "mtg-kernel-spellbench-scorer-config/v1", "source": json.loads(self.source.read_text()),
             "selection": "sampled-wide-v1"}))
        os.environ["FAKE_SCORER_SHIFT"] = "0"
        code = lp.main(["scorer", "--dump", str(self.root / "d"), "--jobs", str(self.jobs), "--scorer", sys.executable,
                        "--scorer-arg", str(fake), "--config", str(self.root / "bot.scorer.json"),
                        "--out", str(self.root / "scorer.json")])
        self.assertEqual(code, 0)
        self.assertEqual(json.loads((self.root / "scorer.json").read_text())["schema"], "w5-scorer-parity/v1")

def parity(ckpt="c", census="e", linux="linux-zen5-9950x", windows="windows-cpu"):
    return {"schema": "w5-logit-parity/v1", "pass": True, "checkpoint_pins": [ckpt], "max_abs_logit_diff": 0.0,
            "tolerance": 1e-5, "argmax_breaks": [],
            "checks": {"states_at_least_10000": True, "max_abs_logit_within_tolerance": True,
                       "argmax_equal_except_ties": True, "input_tensors_identical": True},
            "states": 10_000, "source_identity": None, "linux": {"host_class": linux}, "windows": {"host_class": windows,
                                                                          "binary": {"sha256": census}}}


class LookTests(unittest.TestCase):
    endpoint = {"schema": hl.ENDPOINT_SCHEMA, "chain": "w5/chain1", "arm": "X8", "look": "50pct",
                "training_host_class": "linux-zen5-9950x", "checkpoint": {"sha256": "c"}}
    panel = {"schema": hl.PANEL_SCHEMA, "namespace": hl.NAMESPACE, "census": {"sha256": "e"}}

    def test_admission(self):
        hl.admit(self.endpoint, self.panel, parity(), "windows-cpu")
        for bad in (parity(ckpt="other"), parity(census="other"), parity(linux="linux-other"),
                    parity(windows="windows-other"), dict(parity(), **{"pass": False}),
                    dict(parity(), tolerance=0.02, max_abs_logit_diff=0.01), dict(parity(), states=9_999)):
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
            endpoint = dict(self.endpoint, checkpoint=ckpt,
                            source=pinned("source.json", {"checkpoint": ckpt["path"]}))
            opponent = pinned("h.json", {"name": "H"})
            decks = pinned("decks.json", [{"label": f"d{i}", "mainboard": [i]} for i in range(9)])
            evaluator, census = {"sha256": "a" * 64}, {"sha256": "e" * 64}
            manifest = pinned("step-c.json", {"pins": [opponent["sha256"], decks["sha256"], evaluator["sha256"],
                                                       census["sha256"]]})
            panel = dict(self.panel, opponent=opponent, decks=decks, evaluator=evaluator, census=census, shards=4,
                         step_c_manifest=manifest)
            files = {}
            for name, value in (("endpoint.json", endpoint), ("panel.json", panel),
                                ("parity.json", dict(parity(ckpt=ckpt["sha256"], census="e" * 64, windows=this_host),
                                                     source_identity=lp.source_identity(Path(endpoint["source"]["path"]))))):
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
            bad_manifest = pinned("step-c-other.json", {"pins": []})
            files["panel.json"].write_text(json.dumps(dict(panel, step_c_manifest=bad_manifest)))
            hl.host_class = lambda: this_host
            try:
                with self.assertRaises(hl.LookError):  # an opponent or evaluator Step C never pinned
                    hl.plan(files["endpoint.json"], files["panel.json"], files["parity.json"], tmp / "look2")
            finally:
                hl.host_class = original


if __name__ == "__main__":
    unittest.main()
