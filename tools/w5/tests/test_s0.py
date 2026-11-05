"""W5 E4 (S0): the split, the recording plan, runner and index (with a fake recorder), the frozen config and
trainer (CPU, synthetic store), the pilot model against the float64 reference, XS's start inputs, the S source and
the qualification floor's reader."""
import hashlib
import importlib.util
import json
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parents[1]
REPO = HERE.parents[1]
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
import runtime_receipt  # noqa: E402
import s0  # noqa: E402
import s0_recipe  # noqa: E402

HAS_TORCH = importlib.util.find_spec("torch") is not None


def synthetic_record(rng, actor=0, max_card=192):
    n, e, a, r = int(rng.integers(1, 7)), int(rng.integers(0, 4)), int(rng.integers(2, 6)), int(rng.integers(0, 4))
    return {"selected": int(rng.integers(0, a)), "actor": actor,
            "state": rng.normal(size=219).astype(np.float32),
            "object_features": rng.normal(size=(n, 98)).astype(np.float32),
            "object_card_ids": rng.integers(0, max_card + 1, n), "object_groups": rng.integers(0, 20, n),
            "object_node_ids": np.arange(n), "edge_features": rng.normal(size=(e, 41)).astype(np.float32),
            "edge_source": rng.integers(0, n, e), "edge_target": rng.integers(0, n, e),
            "action_features": rng.normal(size=(a, 195)).astype(np.float32),
            "action_ref_features": rng.normal(size=(r, 25)).astype(np.float32),
            "action_ref_card_ids": rng.integers(0, max_card + 1, r), "action_ref_action_indices": rng.integers(0, a, r),
            "action_ref_node_indices": rng.integers(0, n, r),
            "logits": (2 * rng.normal(size=a)).astype(np.float32), "value": float(rng.uniform(-1, 1))}


FAKE_RECORDER = r'''
import json, os, struct, sys
import numpy as np
sys.path.insert(0, os.environ["S0_TOOLS"])
import s0
from test_s0 import synthetic_record
args = dict(a.split("=", 1) for a in sys.argv[1:])
first, games, seed = int(args["first_game"]), int(args["games"]), int(args["base_seed"])
fail = os.environ.get("FAKE_FAIL_GAME")
out = open(args["out"], "w")
tensors = open(os.environ["TD_BIN"], "ab")
runtime = os.environ.get("FAKE_RUNTIME", os.environ.get("S4A_RUNTIME"))
limits = [int(x) for x in os.environ["S4A_GAME_LIMITS"].split(",")]
for g in range(first, first + games):
    if fail is not None and g == int(fail):
        sys.exit(1)
    rng = np.random.default_rng([seed % 2**32, g])
    block, rows = b"", []
    for i in range(int(rng.integers(3, 8))):
        rec = synthetic_record(rng, actor=i % 2)
        start = len(block)
        block += s0.encode_record(rec)
        rows.append([i, rec["actor"], len(rec["logits"]), rec["selected"], i // 2, start, len(block) - start])
    offset = tensors.tell()
    tensors.write(block)
    natural = g % 17 != 5
    out.write(json.dumps({"kind": "td_game", "game": g, "seed": g, "decks": ["A", "B"], "focal_seat": g % 2,
                          "opp_model": "self", "starting_player": (g // 2) % 2,
                          "classification": "Natural" if natural else "DecisionLimit",
                          "winner": int(rng.integers(0, 2)) if natural else None, "forced": 0,
                          "bin_offset": offset, "bin_bytes": len(block), "decisions": rows,
                          "runtime_rules": runtime,
                          "limits": {"max_physical_decisions": limits[0], "max_policy_steps": limits[1]}}) + "\n")
    out.flush()
'''


def parent_source(tmp: Path) -> Path:
    ck = tmp / "b512.json"
    ck.write_text('{"fake": "b512"}')
    src = {"checkpoint": {"path": ck.as_posix(), "sha256": hashlib.sha256(ck.read_bytes()).hexdigest()},
           "feature_transfer": {"expected_feature_contract_digest": "c" * 64,
                                "expected_feature_encoding_digest": "e" * 64},
           "play_import": {"path": "import.json", "sha256": "0" * 64}}
    path = tmp / "b512-source.json"
    path.write_text(json.dumps(src))
    return path


class Recorded:
    """A recorded and indexed fake store under a temporary directory."""

    def __init__(self, tmp: Path, games=96, chunk=32, runtime=None):
        self.tmp = tmp
        self.fake = tmp / "fake_recorder.py"
        self.fake.write_text(FAKE_RECORDER)
        os.environ["S0_TOOLS"] = str(HERE)
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        os.environ["PYTHONPATH"] = os.pathsep.join([str(HERE), str(Path(__file__).resolve().parent)])
        if runtime is not None:
            os.environ["FAKE_RUNTIME"] = runtime
        self.plan = s0.plan_record(parent_source(tmp), tmp / "root", games=games, chunk_games=chunk)
        self.plan_path = tmp / "root" / "plan.json"

    def record_and_index(self, parallel=2):
        summary = s0.record(self.plan_path, self.fake, parallel)
        return summary, s0.index_store(self.plan_path)


class SplitAndPlanTests(unittest.TestCase):
    def test_split_is_the_hash_rule_and_about_five_percent(self):
        digest = hashlib.sha256(json.dumps(["w5/s0/split", 7]).encode()).digest()
        self.assertEqual(s0.split_u(7), int.from_bytes(digest[:8], "big") / 2 ** 64)
        share = np.mean([s0.is_validation(g) for g in range(20_000)])
        self.assertAlmostEqual(share, 0.05, delta=0.006)
        self.assertEqual([s0.is_validation(g) for g in range(50)], [s0.is_validation(g) for g in range(50)])

    def test_plan_is_frozen_size_runtime_and_seeds(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            plan = s0.plan_record(parent_source(tmp), tmp / "a")
            self.assertEqual(plan["games"], 100_000)
            self.assertTrue(plan["production"])
            self.assertEqual(len(plan["chunks"]), 391)
            self.assertEqual(plan["chunks"][-1], {"name": "chunk-099840", "first_game": 99_840, "games": 160})
            self.assertEqual(plan["runtime"]["runtime_rules"], "resolution-boundary-v1")
            self.assertEqual((plan["runtime"]["max_physical_decisions"], plan["runtime"]["max_policy_steps"]),
                             (100_000, 200_000))
            self.assertEqual(plan["base_seed"], int.from_bytes(
                hashlib.sha256(b"w5/s0/record/first").digest()[:8], "little"))
            retry = s0.plan_record(parent_source(tmp), tmp / "b", attempt="retry")
            self.assertEqual(retry["games"], 200_000)
            self.assertNotEqual(retry["base_seed"], plan["base_seed"])
            with self.assertRaises(s0.S0Error):
                s0.plan_record(parent_source(tmp), tmp / "a")

    def test_recording_command_runs_the_training_runtime(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            plan = s0.plan_record(parent_source(tmp), tmp / "a", games=10)
            cmd, env = s0.chunk_command(plan, Path("census.exe"), tmp / "x", 0, 10)
            self.assertIn("mode=s4a-tensor-dump", cmd)
            self.assertIn("workers=1", cmd)
            self.assertIn("decks=0,1,2,3,4,5,6,7,8", cmd)
            self.assertEqual(env["S4A_RUNTIME"], "resolution-boundary-v1")
            self.assertEqual(env["S4A_GAME_LIMITS"], "100000,200000")
            self.assertTrue(env["OPPONENTS"].startswith("self="))

    def test_s0_plan_feeds_the_runtime_receipt(self):
        plan = {"schema": "w5-s0-record-plan/v1", "games": 300, "runtime": {"runtime_rules": "resolution-boundary-v1"}}
        self.assertEqual(runtime_receipt.runtime_profile([plan]), {"resolution-boundary-v1": 300})


class RecordAndIndexTests(unittest.TestCase):
    def tearDown(self):
        os.environ.pop("FAKE_RUNTIME", None)
        os.environ.pop("FAKE_FAIL_GAME", None)

    def test_record_index_and_hashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            rec = Recorded(Path(tmp))
            summary, manifest = rec.record_and_index()
            self.assertEqual(summary["failed"], [])
            self.assertEqual(summary["remaining"], 0)
            counts = manifest["counts"]
            self.assertEqual(counts["games"], 96)
            not_natural = sorted(g for g in range(96) if g % 17 == 5)
            self.assertEqual(manifest["value_targets"]["not_natural"], len(not_natural))
            ix, _, root = s0.load_index(Path(tmp) / "root" / "manifest.json")
            self.assertEqual(int(ix["game"].size), counts["decisions_indexed"])
            # Every recorded decision is indexed (policy target); unfinished games carry no value target.
            self.assertEqual(counts["decisions_indexed"], counts["decisions_recorded"])
            self.assertEqual(sorted(set(ix["game"][~ix["has_value"]].tolist())), not_natural)
            self.assertEqual(s0.verify_store(Path(tmp) / "root" / "manifest.json")["chunks"], 3)
            rows = np.nonzero(ix["validation"])[0]
            self.assertEqual(s0.verify_games(ix, root, rows), len(set(ix["game"][rows].tolist())))
            store_bin = root / "store" / "chunk-000032.bin"
            data = bytearray(store_bin.read_bytes())
            data[100] ^= 1
            store_bin.write_bytes(bytes(data))
            with self.assertRaises(s0.S0Error):
                s0.verify_store(Path(tmp) / "root" / "manifest.json")
            data[100] ^= 1
            store_bin.write_bytes(bytes(data))
            self.assertTrue(np.all(np.diff(ix["game"]) >= 0))
            self.assertTrue(np.array_equal(ix["validation"], [s0.is_validation(int(g)) for g in ix["game"]]))
            # Every indexed record decodes at its offset with its row's outcome.
            games = {g["game"]: g for g in s0.read_json(root / "index.games.json")["games"]}
            for r in range(0, ix["game"].size, 7):
                path = root / str(ix["files"][ix["fid"][r]])
                with open(path, "rb") as f:
                    f.seek(int(ix["offset"][r]))
                    buf = f.read(int(ix["length"][r]))
                record, end = s0.decode_record(buf)
                self.assertEqual(end, len(buf))
                self.assertEqual(record["actor"], int(ix["actor"][r]))
                game = games[int(ix["game"][r])]
                if game["has_value"]:
                    self.assertEqual(ix["outcome"][r], float(record["actor"] == game["winner"]))
            # A second record pass does nothing.
            self.assertEqual(s0.record(rec.plan_path, rec.fake, 2)["chunks_run"], 0)

    def test_index_refuses_another_runtime(self):
        with tempfile.TemporaryDirectory() as tmp:
            rec = Recorded(Path(tmp), games=32, runtime="historical")
            s0.record(rec.plan_path, rec.fake, 1)
            with self.assertRaises(s0.S0Error):
                s0.index_store(rec.plan_path)

    def test_a_failed_chunk_is_kept_aside_and_rerun(self):
        with tempfile.TemporaryDirectory() as tmp:
            rec = Recorded(Path(tmp), games=64)
            os.environ["FAKE_FAIL_GAME"] = "40"
            summary = s0.record(rec.plan_path, rec.fake, 2)
            self.assertEqual(summary["failed"], ["chunk-000032"])
            with self.assertRaises(s0.S0Error):
                s0.index_store(rec.plan_path)
            os.environ.pop("FAKE_FAIL_GAME")
            self.assertEqual(s0.record(rec.plan_path, rec.fake, 2)["chunks_run"], 1)
            store = Path(tmp) / "root" / "store"
            self.assertTrue((store / "chunk-000032.attempt-1.log").exists())
            self.assertEqual(s0.index_store(rec.plan_path)["counts"]["games"], 64)

    def test_production_recording_needs_a_qualification(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            plan = s0.plan_record(parent_source(tmp), tmp / "root", games=5000)
            self.assertFalse(plan["production"])
            with self.assertRaises(s0.S0Error):
                s0.record(tmp / "root" / "plan.json", tmp / "fake.py", 1, limit_chunks=0)

    def test_qualification_measures_each_process_count(self):
        with tempfile.TemporaryDirectory() as tmp:
            rec = Recorded(Path(tmp), games=32)
            receipt = s0.qualify_record(rec.plan_path, rec.fake, [1, 2], games=2)
            self.assertEqual([t["processes"] for t in receipt["trials"]], [1, 2])
            self.assertIn(receipt["selected_processes"], (1, 2))
            self.assertFalse((Path(tmp) / "root" / "store").exists())


@unittest.skipUnless(HAS_TORCH, "torch is not installed")
class TrainerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import torch  # noqa: F401
        import s0_train
        cls.T = s0_train

    def tearDown(self):
        os.environ.pop("FAKE_RUNTIME", None)

    def store(self, tmp: Path, games=160):
        rec = Recorded(tmp, games=games, chunk=64)
        rec.record_and_index()
        return rec

    def test_pilot_model_matches_the_float64_reference(self):
        import torch
        sys.path.insert(0, str(REPO / "python" / "tools"))
        import s_transformer_reference_v1 as ref
        model = self.T.build_model()
        model.eval()
        rng = np.random.default_rng(3)
        recs = [synthetic_record(rng) for _ in range(9)]
        # A trained-like point: perturb every parameter so no term is trivially zero.
        with torch.no_grad():
            for p in model.parameters():
                p.add_(0.05 * torch.randn(p.shape, generator=torch.Generator().manual_seed(p.numel())))
            for blk in model.blocks:
                blk.qkv.bias[96:192] = 0
            model.policy[2].bias.zero_()
        bt = self.T.collate(recs, np.zeros(len(recs)), np.ones(len(recs)))
        with torch.no_grad():
            logits, z = model(bt)
        state = {k: v.detach().to(torch.float64) for k, v in model.state_dict().items()}
        ref_logits, ref_z = ref.forward(state, dict(ref.S_CONFIG), ref.collate(recs))
        worst = max(float((logits[i, :len(recs[i]["logits"])].double() - ref_logits[i]).abs().max())
                    for i in range(len(recs)))
        self.assertLess(worst, 1e-4)
        self.assertLess(float((z.double() - ref_z).abs().max()), 1e-4)
        self.assertEqual([k for k, _ in model.named_parameters()][:3], ["ref_bias", "card.weight", "obj_in.0.weight"])

    def test_initializer_is_the_pilot_scheme_and_seeded(self):
        import torch
        a, b = self.T.build_model(), self.T.build_model()
        self.assertEqual(self.T.params_sha(a), self.T.params_sha(b))
        self.assertNotEqual(self.T.params_sha(a), self.T.params_sha(self.T.build_model(1)))
        self.assertEqual(sum(p.numel() for p in a.parameters()), 460_346)
        self.assertAlmostEqual(float(a.blocks[0].qkv.weight.detach().std()), 0.02, delta=0.002)
        self.assertTrue(torch.all(a.blocks[0].qkv.bias == 0))
        self.assertTrue(torch.all(a.ln.weight == 1))

    def test_vsub_mask_over_the_epoch_rows_keeps_eight_per_game(self):
        game = np.repeat(np.arange(30), 9)[:257]
        ix_len = np.ones(257, np.int32)
        rng, vrng = np.random.default_rng(20261102), np.random.default_rng(20261102 + 104729)
        batches = self.T.epoch_batches(np.arange(257), ix_len, 1, rng)
        used = np.sort(np.concatenate(batches))
        self.assertEqual(used.size, 256)
        m = self.T.vsub_mask(257, used, game, vrng)
        for g in range(30):
            self.assertEqual(int(m[game == g].sum()), min(8, int(np.isin(np.nonzero(game == g)[0], used).sum())))
        self.assertFalse(m[np.setdiff1d(np.arange(257), used)].any())

    def test_vsub_mask_takes_eight_per_game(self):
        game = np.repeat(np.arange(5), [3, 8, 12, 20, 1])
        rows = np.arange(game.size)
        m = self.T.vsub_mask(game.size, rows, game, np.random.default_rng(1))
        self.assertEqual([int(m[game == g].sum()) for g in range(5)], [3, 8, 8, 8, 1])

    def test_schedule_is_the_pilot_warmup_cosine(self):
        self.assertEqual(self.T.lr_factor(0, 10, 1000), 0.1)
        self.assertEqual(self.T.lr_factor(9, 10, 1000), 1.0)
        self.assertAlmostEqual(self.T.lr_factor(10, 10, 1000), 1.0)
        self.assertAlmostEqual(self.T.lr_factor(1000, 10, 1000), 0.1)

    def test_kl_is_zero_at_the_teacher_and_ignores_padding(self):
        import torch
        rng = np.random.default_rng(5)
        recs = [synthetic_record(rng) for _ in range(4)]
        bt = self.T.collate(recs, np.ones(4), np.ones(4))

        class Teacher(torch.nn.Module):
            def forward(self, b):
                return (self_pad(b), torch.zeros(4))

        def self_pad(b):
            return self.T.pad_logits(b["teacher_logits"], 4, b["act_batch"], b["act_pos"], b["amax"])
        val, kl, ce = self.T.losses(Teacher(), bt, False)
        self.assertLess(float(kl.abs().max()), 1e-6)
        self.assertTrue(torch.all(torch.isfinite(ce)))
        self.assertTrue(torch.allclose(val, torch.full((4,), float(np.log(2))), atol=1e-6))

    def test_smoke_training_is_deterministic_and_keeps_the_gauge(self):
        import torch
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            rec = self.store(tmp)
            cfg = tmp / "cfg.json"
            s0.make_config(rec.plan_path, cfg, "1.0", smoke=True, eval_every=2, max_steps=4)
            runs = []
            for name in ("a", "b"):
                self.assertEqual(self.T.train(cfg, tmp / name), 0)
                runs.append(s0.read_json(tmp / name / "train.json"))
            self.assertEqual(runs[0]["final_params_sha256"], runs[1]["final_params_sha256"])
            self.assertEqual(runs[0]["best_pt_sha256"], runs[1]["best_pt_sha256"])
            self.assertEqual(runs[0]["steps"], 4)
            self.assertEqual([e["step"] for e in runs[0]["evaluations"]], [2, 4])
            state = torch.load(tmp / "a" / "best.pt", weights_only=True)
            self.assertTrue(torch.all(state["policy.2.bias"] == 0))
            for i in range(3):
                self.assertTrue(torch.all(state[f"blocks.{i}.qkv.bias"][96:192] == 0))
            self.assertFalse(torch.all(state["blocks.0.qkv.bias"][:96] == 0))
            # No clipping is a different run.
            cfg2 = tmp / "cfg2.json"
            s0.make_config(rec.plan_path, cfg2, "none", smoke=True, eval_every=2, max_steps=4)
            self.assertEqual(self.T.train(cfg2, tmp / "c"), 0)
            # A tampered recipe is refused.
            doc = s0.read_json(cfg)
            doc["recipe"]["optimizer"]["lr"] = 3e-4
            bad = tmp / "bad.json"
            bad.write_text(json.dumps(doc))
            with self.assertRaises(s0.S0Error):
                self.T.train(bad, tmp / "d")

    def test_production_config_refuses_overrides_and_smoke_sizes(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            rec = self.store(tmp, games=64)
            with self.assertRaises(s0.S0Error):
                s0.make_config(rec.plan_path, tmp / "c.json", "1.0")
            with self.assertRaises(s0.S0Error):
                s0.make_config(rec.plan_path, tmp / "c.json", "1.0", smoke=False, eval_every=2)


class StartAndSourceTests(unittest.TestCase):
    def test_start_extras_are_the_chain_roots_and_update_zero(self):
        extras = s0.start_extras()
        self.assertEqual([e[0] for e in extras], ["rng/seed_roots", "cursor/first_update"])
        self.assertEqual(bytes.fromhex(extras[0][1]).decode(), json.dumps([f"w5/chain{i}" for i in range(1, 7)],
                                                                           separators=(",", ":")))
        self.assertEqual(bytes.fromhex(extras[1][1]), struct.pack("<Q", 0))

    def test_source_puts_s0_on_the_shell(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            container = tmp / "s0.bin"
            container.write_bytes(b"S0")
            src = s0.make_source(parent_source(tmp), container, tmp / "s0-source.json")
            self.assertIsNone(src["checkpoint"])
            self.assertEqual(src["s_learner"], {"schema": "w5-s-learner/v1", "initial": s0.pin(container)})
            sys.path.insert(0, str(REPO / "python" / "tools"))
            import nine_deck_public_panel_v1 as npp
            self.assertEqual(npp.candidate_pin_sha256(src), s0.sha256_file(container))
            self.assertEqual(npp.candidate_pin_sha256(s0.read_json(parent_source(tmp))),
                             s0.read_json(parent_source(tmp))["checkpoint"]["sha256"])

    def test_freeze_start_needs_a_production_export_unless_smoke(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            container = tmp / "s0.bin"
            container.write_bytes(b"S0")
            fake = tmp / "fake_s.py"
            fake.write_text("import json,sys,hashlib\n"
                            "a=dict(x.split('=',1) for x in sys.argv[2:])\n"
                            "b=open(a['checkpoint'],'rb').read()\n"
                            "print(json.dumps({'checkpoint_sha256':hashlib.sha256(b).hexdigest(),'adam_step':0,"
                            "'state_sha256':'s','starting_state_sha256':hashlib.sha256(b+open(a['extras'],'rb').read()).hexdigest()}))\n")
            exp = {"schema": "w5-s0-export/v1", "passed": True, "smoke": True, "container": s0.pin(container),
                   "parity": {"states": {"decisions": 500}}}
            (tmp / "exp.json").write_text(json.dumps(exp))
            with self.assertRaises(s0.S0Error):
                s0.freeze_start(tmp / "exp.json", fake, tmp / "start.json")
            doc = s0.freeze_start(tmp / "exp.json", fake, tmp / "start.json", smoke=True)
            self.assertTrue(doc["smoke"])
            exp.update(smoke=False)
            (tmp / "exp2.json").write_text(json.dumps(exp))
            with self.assertRaisesRegex(s0.S0Error, "10000-decision"):
                s0.freeze_start(tmp / "exp2.json", fake, tmp / "start2.json")
            exp["parity"]["states"]["decisions"] = 10_000
            (tmp / "exp3.json").write_text(json.dumps(exp))
            doc = s0.freeze_start(tmp / "exp3.json", fake, tmp / "start3.json")
            self.assertFalse(doc["smoke"])
            with self.assertRaises(s0.S0Error):
                s0.freeze_start(tmp / "exp3.json", fake, tmp / "start3.json")

    def test_floor_plan_needs_the_frozen_start_of_this_s0(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            container = tmp / "s0.bin"
            container.write_bytes(b"S0")
            src = tmp / "s0-source.json"
            s0.make_source(parent_source(tmp), container, src)
            start = tmp / "xs-start.json"
            start.write_text(json.dumps({"schema": "w5-xs-start/v1", "smoke": False, "container": {"sha256": "0" * 64}}))
            with self.assertRaisesRegex(s0.S0Error, "frozen start"):
                s0.floor_plan(src, parent_source(tmp), tmp / "panel.json", start, tmp / "eval.exe", tmp / "f")
            start.write_text(json.dumps({"schema": "w5-xs-start/v1", "smoke": True,
                                         "container": s0.pin(container)}))
            with self.assertRaisesRegex(s0.S0Error, "smoke"):
                s0.floor_plan(src, parent_source(tmp), tmp / "panel.json", start, tmp / "eval.exe", tmp / "f")
            start.write_text(json.dumps({"schema": "w5-xs-start/v1", "smoke": False,
                                         "container": s0.pin(container)}))
            continued = s0.read_json(src)
            continued["checkpoint"] = {"path": "record.json", "sha256": "1" * 64}
            (tmp / "continued.json").write_text(json.dumps(continued))
            with self.assertRaisesRegex(s0.S0Error, "checkpoint record"):
                s0.floor_plan(tmp / "continued.json", parent_source(tmp), tmp / "panel.json", start,
                              tmp / "eval.exe", tmp / "f")
            start.write_text(json.dumps({"schema": "other"}))
            with self.assertRaisesRegex(s0.S0Error, "before the floor"):
                s0.floor_plan(src, parent_source(tmp), tmp / "panel.json", start, tmp / "eval.exe", tmp / "f")


def floor_rows(path: Path, panel: str, score_of, missing=()):
    import pilot
    with open(path, "w") as f:
        for o in range(9):
            for t in range(9):
                for k in range(12):
                    for s in (0, 1):
                        key = (o, t, k, s)
                        complete = key not in missing
                        f.write(json.dumps({"panel": panel, "checkpoint_sha256": "a" * 64, "own": o, "other": t,
                                            "repeat": k, "seat": s, "seed": pilot.panel_seed(o, t, k, s0.H_NAMESPACE),
                                            "complete": complete,
                                            "score": score_of(key) if complete else None}) + "\n")


class FloorReaderTests(unittest.TestCase):
    def test_paired_signed_score_on_eight_decks(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            # b512 wins everything; S0 wins all but repeat 0 (a loss): S0 = 1 - 2 / 12 per deck.
            floor_rows(tmp / "b.jsonl", "B-vs-H", lambda key: 1)
            floor_rows(tmp / "s.jsonl", "S-vs-H", lambda key: 0 if key[2] == 0 else 1)
            r = s0.read_floor_rows(tmp / "s.jsonl", tmp / "b.jsonl", "S-vs-H", "B-vs-H")
            self.assertEqual(r["scored_decks"], [0, 1, 2, 3, 5, 6, 7, 8])
            self.assertAlmostEqual(r["difference"], -2 / 12)
            self.assertFalse(r["passed"])
            self.assertEqual(r["games_each"], 1944)
            # Spy (deck 4) as the focal deck is not scored.
            floor_rows(tmp / "s2.jsonl", "S-vs-H", lambda key: 0 if key[0] == 4 else 1)
            r = s0.read_floor_rows(tmp / "s2.jsonl", tmp / "b.jsonl", "S-vs-H", "B-vs-H")
            self.assertEqual(r["difference"], 0.0)
            self.assertTrue(r["passed"])

    def test_floor_boundary_and_missing_games_least_favorable(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            floor_rows(tmp / "b.jsonl", "B-vs-H", lambda key: 0.5)
            # S0 draws all but 3 games per focal deck lost: per deck 2 * (0.5 - 1.5 / 216) - 1.
            lost = lambda key: key[1] == 0 and key[2] < 3 and key[3] == 0  # noqa: E731
            floor_rows(tmp / "s.jsonl", "S-vs-H", lambda key: 0 if lost(key) else 0.5)
            r = s0.read_floor_rows(tmp / "s.jsonl", tmp / "b.jsonl", "S-vs-H", "B-vs-H")
            self.assertAlmostEqual(r["difference"], -3 / 216)
            self.assertTrue(r["passed"])
            # One missing S0 game counts as lost, one missing b512 game as won.
            floor_rows(tmp / "s3.jsonl", "S-vs-H", lambda key: 0.5, missing={(0, 0, 0, 0)})
            floor_rows(tmp / "b3.jsonl", "B-vs-H", lambda key: 0.5, missing={(1, 0, 0, 0)})
            r = s0.read_floor_rows(tmp / "s3.jsonl", tmp / "b3.jsonl", "S-vs-H", "B-vs-H")
            self.assertEqual(r["missing"], {"s0": 1, "b512": 1})
            self.assertAlmostEqual(r["fills"]["least_favorable"]["difference"], -2 * (2 * 0.5 / 216) / 8)
            self.assertAlmostEqual(r["fills"]["most_favorable"]["difference"], 2 * (2 * 0.5 / 216) / 8)

    def test_roster_seed_and_panel_checks(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            floor_rows(tmp / "b.jsonl", "B-vs-H", lambda key: 1)
            lines = (tmp / "b.jsonl").read_text().splitlines()
            (tmp / "short.jsonl").write_text("\n".join(lines[:-1]) + "\n")
            with self.assertRaises(s0.S0Error):
                s0.read_floor_rows(tmp / "short.jsonl", tmp / "b.jsonl", "B-vs-H", "B-vs-H")
            row = json.loads(lines[0])
            row["seed"] += 1
            (tmp / "seed.jsonl").write_text("\n".join([json.dumps(row)] + lines[1:]) + "\n")
            with self.assertRaises(s0.S0Error):
                s0.read_floor_rows(tmp / "seed.jsonl", tmp / "b.jsonl", "B-vs-H", "B-vs-H")
            with self.assertRaises(s0.S0Error):
                s0.read_floor_rows(tmp / "b.jsonl", tmp / "b.jsonl", "S-vs-H", "B-vs-H")


class RecipeTests(unittest.TestCase):
    def test_recipe_names_design_constants_and_needs_the_clip_decision(self):
        r = s0_recipe.recipe(1.0)
        self.assertEqual(r["seed"], 20261102)
        self.assertEqual(r["architecture"], {"d": 96, "layers": 3, "heads": 4, "ffn": 384, "card_dim": 32,
                                             "vocab": 257, "dropout": 0.1})
        self.assertEqual(r["optimizer"]["kind"], "adam")
        self.assertEqual(r["optimizer"]["lr"], 1e-4)
        self.assertEqual(r["optimizer"]["weight_decay"], 0.0)
        self.assertEqual((r["batch_decisions"], r["epochs_max"]), (256, 3))
        self.assertEqual(r["loss"]["vsub_k"], 8)
        self.assertIsNone(s0_recipe.recipe(None)["grad_clip_norm"])
        with self.assertRaises(ValueError):
            s0_recipe.recipe(0)


if __name__ == "__main__":
    unittest.main()
