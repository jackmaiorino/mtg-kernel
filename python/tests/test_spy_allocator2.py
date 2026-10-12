"""Allocator v2 (allocator-redesign-v1): schedule shape, controller math, PPO gradients,
update determinism and parity with the Rust schema. No games or search run."""
import copy
import importlib.util
import json
import re
import tempfile
import unittest
from collections import Counter
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[2]


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, REPO / "tools/spy_reset" / file)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


A = load("allocator2", "allocator2.py")
PARENT = {"lineage": "r1", "weights_sha256": "w", "feature_contract_digest": "c", "feature_encoding_digest": "e"}


def fake_rows(wbytes, n_games, seed, ends=None, decisions=None, arm=None):
    """Rows collected with the Python mirror of the Rust consultation."""
    w = A.read_weights(wbytes)
    arm = arm or w["arm"]
    rng = np.random.default_rng(seed)
    rows = []
    for g in range(n_games):
        remaining, recs = A.CAP, []
        for i in range(decisions[g] if decisions else int(rng.integers(0, 6))):
            if remaining <= 0:
                break
            rec = A.consult_record(w, rng.uniform(0, 1, len(A.S_FEATURES)).tolist(),
                                   rng.uniform(-1, 1, 3 * A.REP).tolist(), remaining, A.CAP,
                                   float(rng.uniform()), i)
            remaining -= rec["amount"]
            recs.append(rec)
        end = ends[g] if ends else ("win", "loss", "draw")[int(rng.integers(3))]
        rows.append(A.synthetic_row(f"g{seed}-{g}", arm, "r1", wbytes, recs, end))
    return rows


class RustParity(unittest.TestCase):
    def test_schema_constants_match_rust(self):
        src = (REPO / "mtg-kernel/src/regret_census_v1/stage4a/allocator2.rs").read_text()
        block = src[src.index("pub(super) const S_FEATURES"):]
        block = block[:block.index("];")]
        self.assertEqual(tuple(re.findall(r'"([a-z0-9_]+)"', block)), A.S_FEATURES)
        self.assertIn(f"pub(super) const NS: usize = {len(A.S_FEATURES)};", src)
        self.assertIn('WEIGHTS_SCHEMA: &str = "%s"' % A.WEIGHTS_SCHEMA, src)
        self.assertIn('FEATURE_SCHEMA: &str = "%s"' % A.FEATURE_SCHEMA, src)
        self.assertIn("NOMINAL: [u64; NA] = [0, 32_000, 128_000, 512_000]", src)
        self.assertIn('ACTION_LABELS: [&str; NA] = ["0", "32000", "128000", "512000"]', src)
        self.assertIn("POSITIVE_AT_FULL: f64 = 0.5", src)
        self.assertIn("POSITIVE_SPLIT: [f64; 3] = [0.5, 0.4, 0.1]", src)
        self.assertIn("EPSILON: f64 = 0.2", src)
        self.assertIn("HIDDEN: usize = 64", src)
        self.assertIn("CAP: u64 = 2_048_000", src)
        scratch = (REPO / "mtg-kernel/src/native_policy_value_net_v1/scratch_v1.rs").read_text()
        self.assertIn('"%s"' % A.POOLING_SCHEMA, scratch.replace("\n    ", ""))


class Schedule(unittest.TestCase):
    def test_development_cycle_roster(self):
        rows, batches = A.training_batches("dev-c1")
        self.assertEqual(len(rows), 324)
        self.assertEqual(len(batches), 9)
        seen = set()
        for b in batches:
            for lin in A.LINEAGES:
                idx = b["games"][lin]
                self.assertEqual(len(idx), 18)
                cases = [rows[i] for i in idx]
                self.assertEqual({c["family"] for c in cases}, {b["batch"]})
                self.assertEqual(Counter((c["case"]["other"], c["case"]["seat"]) for c in cases),
                                 Counter({(o, s): 1 for o in range(9) for s in (0, 1)}))
                self.assertTrue(all(c["case"]["own"] == A.SPY and c["lineage"] == lin for c in cases))
                seen.update(idx)
        self.assertEqual(seen, set(range(324)))
        for c in rows:
            self.assertEqual(c["case"]["opp"], "T1" if (c["case"]["other"] + A.LINEAGES.index(c["lineage"])) % 2 == 0
                             else "A48")

    def test_panels_and_qualification(self):
        self.assertEqual(len(A.domain_rows("devval")[0]), 108)
        self.assertEqual(len(A.domain_rows("fresh-panel")[0]), 216)
        rows, batches = A.domain_rows("fresh-train-rep2")
        self.assertEqual((len(rows), len(batches)), (648, 18))
        q = A.qualification()
        self.assertEqual(len(q), 18)
        self.assertEqual(Counter(c["lineage"] for c in q), Counter({"r1": 9, "r2": 9}))
        self.assertEqual(len(A.smoke()), 2)

    def test_seeds_disjoint_63_bit_and_deterministic(self):
        with tempfile.TemporaryDirectory() as d:
            s = A.write_schedule(Path(d))
            self.assertEqual(s["overlap_between_cases"], 0)
            seeds = []
            for name in A.DOMAINS:
                for ln in (Path(d) / f"cases-{name}.jsonl").read_text().splitlines():
                    seeds += A.case_seeds(json.loads(ln))
            self.assertTrue(all(0 <= v < 2 ** 63 for v in seeds))
            again = Path(d) / "again"
            s2 = A.write_schedule(again, [Path(d) / "cases-devval.jsonl"])
            devval = {v for ln in (Path(d) / "cases-devval.jsonl").read_text().splitlines()
                      for v in A.case_seeds(json.loads(ln))}
            self.assertEqual(s2["overlap_with_excluded"], len(devval))
            for name in ("cases-dev-c1.jsonl", "batches-dev-c1.json", "cases-fresh-panel.jsonl"):
                self.assertEqual((Path(d) / name).read_bytes(), (again / name).read_bytes())

    def test_case_lines_pair_arms(self):
        c = A.domain_rows("devval")[0][5]
        lines = [A.case_line(c, arm) for arm in A.ARMS]
        self.assertEqual(len({json.dumps(x["case"], sort_keys=True) for x in lines}), 1)
        self.assertEqual(len({x["controller_seed"] for x in lines}), 1)
        self.assertEqual([x["arm"] for x in lines], list(A.ARMS))

    def test_probe_cases_pair_continuations(self):
        w0 = A.init_weights("S", PARENT, None, "t")
        row = fake_rows(w0, 1, 3, decisions=[3])[0]
        row.update(id="dev-c1-r1-v2-f0-s1-S", own=4, other=2, seat=1, opp="T1", seed=9, starting_player=0,
                   live_policy_seeds=[1, 2], search_root="x", controller_seed=5)
        lines = A.probe_cases([{"row": row, "at": 2}])
        self.assertEqual(len(lines), 8)
        by = Counter(x["probe"]["refresh"] for x in lines)
        self.assertEqual(by, Counter({0: 4, 128_000: 4}))
        pairs = {}
        for x in lines:
            pairs.setdefault(tuple(x["probe"]["continuation_seeds"]), set()).add(x["probe"]["refresh"])
        self.assertTrue(all(v == {0, 128_000} for v in pairs.values()))
        self.assertEqual(lines[0]["case"]["id"], "dev-c1-r1-v2-f0-s1")


class ControllerMath(unittest.TestCase):
    def test_base_q_and_coalescing(self):
        self.assertEqual(A.base_q(A.CAP, A.CAP).tolist(), [0.5, 0.25, 0.2, 0.05])
        self.assertEqual(A.choices(100_000), [(0, [0]), (32_000, [1]), (100_000, [2, 3])])
        self.assertEqual(A.choices(5), [(0, [0]), (5, [1, 2, 3])])
        q = A.base_q(100_000, A.CAP)
        self.assertAlmostEqual(A.coalesce(q, A.choices(100_000))[2], q[2] + q[3], places=17)

    def test_init_is_q_and_critic_is_not_zero(self):
        for arm in A.LEARNED:
            w = A.read_weights(A.init_weights(arm, PARENT, None, "x"))
            nf = len(A.feature_names(arm))
            self.assertEqual(w["params"]["w1"].shape, (A.HIDDEN, nf))
            bound = (6 / (nf + A.HIDDEN)) ** 0.5
            self.assertTrue(np.all(np.abs(w["params"]["w1"]) <= bound))
            self.assertTrue(np.all(w["params"]["wa"] == 0) and np.all(w["params"]["ba"] == 0))
            self.assertTrue(np.any(w["params"]["wv"] != 0))
            self.assertTrue(np.all(w["params"]["b1"] == 0) and w["params"]["bv"][0] == 0)
            for remaining in (A.CAP, 300_000, 7):
                x = np.linspace(0, 1, nf)
                _, _, actor, value = A.forward(w["params"], w["norm"], x)
                self.assertTrue(np.all(actor == 0))
                self.assertNotEqual(value, 0.0)
                q = A.base_q(remaining, A.CAP)
                self.assertTrue(np.allclose(A.policy(q, actor), q, atol=1e-15))
            # Seeded: the same label reproduces the bytes, another label differs.
            self.assertEqual(A.init_weights(arm, PARENT, None, "x"), A.init_weights(arm, PARENT, None, "x"))
            self.assertNotEqual(A.init_weights(arm, PARENT, None, "x"), A.init_weights(arm, PARENT, None, "y"))

    def test_consult_record_mixture_and_pick(self):
        w0 = A.init_weights("H", PARENT, None, "m")
        w = A.read_weights(w0)
        w["params"]["wa"] = np.random.default_rng(1).normal(0, 0.3, size=w["params"]["wa"].shape)
        rec = A.consult_record(w, [0.3] * len(A.S_FEATURES), [0.1] * (3 * A.REP), 100_000, A.CAP, 0.5, 0)
        q = A.base_q(100_000, A.CAP)
        self.assertTrue(np.allclose(rec["mu"], 0.8 * np.array(rec["pi"]) + 0.2 * np.array(rec["q"]), atol=1e-15))
        self.assertAlmostEqual(sum(rec["mu"]), 1.0, places=12)
        self.assertEqual(rec["members"], [[0], [1], [2, 3]])
        self.assertAlmostEqual(rec["q"][2], q[2] + q[3], places=16)
        self.assertEqual(A.pick([0.5, 0.25, 0.25], 0.5), 1)
        self.assertEqual(A.pick([0.3, 0.3], 0.99), 1)


def games_from(rows, wbytes):
    w = A.read_weights(wbytes)
    out = []
    for r in rows:
        reward, dec = A.game_terms(r, w, A.sha(wbytes))
        for d in dec:
            d["adv"] = reward - d["critic_b"]
        out.append({"reward": reward, "decisions": dec})
    return w, out


class Gradients(unittest.TestCase):
    def total_loss(self, params, norm, games):
        _, st = A.minibatch_grad(params, norm, games)
        return st["actor_loss"] + A.CRITIC_COEF * st["critic_loss"]

    def test_minibatch_gradient_matches_finite_differences(self):
        for arm in A.LEARNED:
            wbytes = A.init_weights(arm, PARENT, None, "fd")
            rows = fake_rows(wbytes, 4, 11, decisions=[3, 0, 5, 2])
            w, games = games_from(rows, wbytes)
            params = {k: v.copy() for k, v in w["params"].items()}
            rng = np.random.default_rng(2)
            params["wa"] = rng.normal(0, 0.005, size=params["wa"].shape)
            params["ba"] = rng.normal(0, 0.005, size=params["ba"].shape)
            grad, st = A.minibatch_grad(params, w["norm"], games)
            self.assertEqual(st["clipped"], 0)
            for k in A.PARAMS:
                flat = params[k].reshape(-1)
                for idx in rng.choice(flat.size, min(flat.size, 40), replace=False):
                    up, dn = copy.deepcopy(params), copy.deepcopy(params)
                    up[k].reshape(-1)[idx] += 1e-6
                    dn[k].reshape(-1)[idx] -= 1e-6
                    fd = (self.total_loss(up, w["norm"], games) - self.total_loss(dn, w["norm"], games)) / 2e-6
                    self.assertLess(abs(fd - grad[k].reshape(-1)[idx]), 1e-7, (arm, k, idx))

    def test_ratio_uses_logged_coalesced_behavior(self):
        """At the collection weights rho = 1 and the actor gradient is the coalesced-mixture
        policy gradient: -(1/G) sum A * d log mu_E(theta), mu_E = 0.8 pi_E + 0.2 q_E."""
        wbytes = A.init_weights("S", PARENT, None, "rho")
        w = A.read_weights(wbytes)
        w["params"]["wa"] = np.random.default_rng(4).normal(0, 0.2, size=w["params"]["wa"].shape)
        wbytes = A.weights_bytes("S", w["params"], w["norm"], PARENT, 0, 1, None)
        rows = fake_rows(wbytes, 3, 5, decisions=[4, 2, 3])
        w, games = games_from(rows, wbytes)
        for g in games:
            for d in g["decisions"]:
                d["adv"] = 0.7
        # The critic never reaches the actor output layer: grad["wa"] is the actor's alone.
        grad, _ = A.minibatch_grad(w["params"], w["norm"], games)

        def actor_obj(wa):
            total = 0.0
            for g in games:
                for d in g["decisions"]:
                    p = dict(w["params"], wa=wa)
                    _, _, actor, _ = A.forward(p, w["norm"], d["x"])
                    pi = A.coalesce(A.policy(d["q"], actor), d["cs"])
                    mu = 0.8 * pi + 0.2 * A.coalesce(d["q"], d["cs"])
                    total += d["adv"] * np.log(mu[d["chosen"]])
            return -total / len(games)

        wa = w["params"]["wa"]
        for idx in [(0, 0), (1, 5), (2, 63), (3, 17)]:
            up, dn = wa.copy(), wa.copy()
            up[idx] += 1e-6
            dn[idx] -= 1e-6
            fd = (actor_obj(up) - actor_obj(dn)) / 2e-6
            self.assertLess(abs(fd - grad["wa"][idx]), 1e-8)

    def test_zero_decision_games_stay_in_the_denominator(self):
        wbytes = A.init_weights("S", PARENT, None, "z")
        rows = fake_rows(wbytes, 1, 6, decisions=[4])
        w, games = games_from(rows, wbytes)
        g1, _ = A.minibatch_grad(w["params"], w["norm"], games)
        g2, _ = A.minibatch_grad(w["params"], w["norm"], games + [{"reward": 1, "decisions": []}])
        for k in A.PARAMS:
            self.assertTrue(np.allclose(g2[k] * 2, g1[k], rtol=1e-12, atol=1e-15))

    def test_clipped_branch_has_no_actor_gradient(self):
        wbytes = A.init_weights("S", PARENT, None, "c")
        rows = fake_rows(wbytes, 2, 8, decisions=[3, 3])
        w, games = games_from(rows, wbytes)
        for g in games:
            for d in g["decisions"]:
                d["adv"] = 1.0
                d["mu_b"] = d["mu_b"] * 0.5  # rho = 2 > 1.2 with a positive advantage: clipped
        grad, st = A.minibatch_grad(w["params"], w["norm"], games)
        self.assertEqual(st["clipped"], 6)
        self.assertEqual(st["gradient_active"], 0)
        self.assertTrue(np.all(grad["wa"] == 0) and np.all(grad["ba"] == 0))


class Update(unittest.TestCase):
    def test_update_is_deterministic_and_replays(self):
        w0 = A.init_weights("H", PARENT, None, "u")
        rows = fake_rows(w0, 18, 21)
        a1, s1, r1 = A.update(w0, None, rows, 0, "r1")
        a2, s2, r2 = A.update(w0, None, copy.deepcopy(rows), 0, "r1")
        self.assertEqual((a1, s1), (a2, s2))
        self.assertEqual(r1["epochs_run"], 4)
        self.assertEqual(len(r1["epochs"][0]["steps"]), 3)
        self.assertEqual(sorted(i for st in r1["epochs"][0]["steps"] for i in st["games"]), list(range(18)))
        self.assertEqual(json.loads(s1)["t"], 12)
        self.assertGreater(r1["tv_from_q_after"]["mean"], 0.0)
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            ledger = d / "TRAIN-LEDGER.jsonl"
            weights, adam = w0, None
            for k in range(2):
                rows = fake_rows(weights, 18, 30 + k)
                files = []
                for i, r in enumerate(rows):
                    p = d / f"t{k}-{i}.jsonl"
                    p.write_text(json.dumps(r) + "\n")
                    files.append(str(p))
                (d / f"w{k}.json").write_bytes(weights)
                if adam is not None:
                    (d / f"a{k}.json").write_bytes(adam)
                new, adam_new, rec = A.update(weights, adam, rows, k, "r1")
                rec.update(kind="update", row_files=files, weights_in=str(d / f"w{k}.json"),
                           adam_in=str(d / f"a{k}.json") if adam is not None else None)
                with ledger.open("a") as f:
                    f.write(json.dumps(rec) + "\n")
                weights, adam = new, adam_new
            self.assertEqual(A.replay(ledger), {"updates": 2, "mismatches": []})
            self.assertEqual(json.loads(adam)["weights_sha256"], A.sha(weights))

    def test_kl_stop_ends_epochs(self):
        w0 = A.init_weights("S", PARENT, None, "kl")
        rows = fake_rows(w0, 18, 41, decisions=[5] * 18, ends=["win", "loss"] * 9)
        _, _, rec = A.update(w0, None, rows, 0, "r1", lr=0.3)
        self.assertLess(rec["epochs_run"], 4)
        self.assertTrue(rec["kl_stopped"])
        self.assertGreater(rec["epochs"][-1]["kl_after"], A.KL_STOP)

    def test_refusals(self):
        w0 = A.init_weights("S", PARENT, None, "r")
        rows = fake_rows(w0, 18, 51, decisions=[2] * 18)
        bad = copy.deepcopy(rows)
        bad[3]["game"]["decisions"][0]["scheduler"]["mu"][0] += 1e-6
        with self.assertRaises(ValueError):
            A.update(w0, None, bad, 0, "r1")
        bad = copy.deepcopy(rows)
        bad[2]["player_after"] = dict(PARENT, weights_sha256="other")
        with self.assertRaises(ValueError):
            A.update(w0, None, bad, 0, "r1")
        tech = copy.deepcopy(rows)
        tech[2]["game"].update(valid=False, end="nonnatural")
        with self.assertRaises(A.TechnicalNonterminal):
            A.update(w0, None, tech, 0, "r1")
        with self.assertRaises(A.TechnicalNonterminal):
            A.update(w0, None, rows[:17] + [{"kind": "error", "id": "x", "error": "panic"}], 0, "r1")
        with self.assertRaises(ValueError):
            A.update(w0, None, rows[:17], 0, "r1")
        with self.assertRaises(ValueError):
            A.update(w0, None, rows, 0, "r2")

    def test_freeze_norm_and_init_binding(self):
        w0 = A.init_weights("H", PARENT, None, "n")
        rows = fake_rows(w0, 6, 61, decisions=[3] * 6)
        n = A.freeze_norm(rows, "r1", "H")
        self.assertEqual(n["mean"].shape, (len(A.feature_names("H")),))
        self.assertTrue(np.all(n["scale"] > 0))
        # Constant inputs keep scale 1.
        for r in rows:
            for d in r["game"]["decisions"]:
                d["scheduler"]["s"][0] = 1.0
        self.assertEqual(A.freeze_norm(rows, "r1", "H")["scale"][0], 1.0)
        w = A.read_weights(A.init_weights("H", PARENT, n, "n"))
        self.assertTrue(w["norm"]["frozen"])
        self.assertTrue(np.array_equal(w["norm"]["mean"], n["mean"]))


if __name__ == "__main__":
    unittest.main()


R = load("allocator2_report", "allocator2_report.py")


def panel_row(c, label, wbytes, end, rng, spy_at=None, complete=False):
    """A fake natural row of one panel case: scheduler records via the Python mirror."""
    w = A.read_weights(wbytes) if label in A.LEARNED else None
    decs, remaining = [], A.CAP
    for p in range(6):
        rec = None
        before = remaining
        if label != "current" and remaining > 0:
            rec = A.consult_record(w, rng.uniform(0, 1, len(A.S_FEATURES)).tolist(),
                                   rng.uniform(-1, 1, 3 * A.REP).tolist(), remaining, A.CAP,
                                   float(rng.uniform()), p)
            remaining -= rec["amount"]
        decs.append({"chooser": "plain", "lookup": "miss", "focal_physical": p, "remaining_before": before,
                     "remaining_after": remaining, "refreshed": None if not rec or rec["amount"] == 0 else "x",
                     "spy_cast_offered": p == spy_at, "scheduler": rec})
    return {"kind": "allocator2_game", "id": f"{c['id']}-{label}", "arm": label, "cap": A.CAP,
            "wall": 1.0, "scheduler": {"controller_seconds": 0.01},
            "game": {"valid": True, "end": end, "decisions": decs, "transitions_used": A.CAP - remaining,
                     "searches": [{"discovery": {"completions_natural": int(complete)}}],
                     "sequence": {"complete": complete, "cast_offered": spy_at is not None}}}


class Report(unittest.TestCase):
    def test_report_end_to_end_on_fake_rows(self):
        w0 = A.init_weights("S", PARENT, None, "rep")
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            A.write_schedule(d / "s")
            (d / "rows").mkdir()
            cases = A.load_cases(d / "s", "devval")
            rng = np.random.default_rng(0)
            for i, c in enumerate(cases):
                for label in ("q", "S", "current"):
                    if label == "S" and i == 7:
                        continue  # missing: unknown, never a loss
                    end = ("win", "loss", "draw")[int(rng.integers(3))]
                    row = panel_row(c, label, w0, end, rng, spy_at=3 if i % 4 == 0 else None,
                                    complete=(label == "S" and i % 8 == 0))
                    (d / "rows" / f"devval-{label}-{i:04d}.jsonl").write_text(json.dumps(row) + "\n")
            (d / "w0.json").write_bytes(w0)
            out = R.report(d / "s", "devval", [d / "rows"], ["q", "S", "current"], ["S"], "q", "current",
                           controllers={("S", "r1"): d / "w0.json"}, initial={("S", "r1"): d / "w0.json"})
        self.assertEqual(sum(out["statuses"].values()), 324)
        self.assertEqual(out["statuses"]["S:unstarted"], 1)
        c = out["contrasts"]["S-q"]
        self.assertIsNotNone(c["r1"]["point"])
        self.assertLessEqual(c["pooled"]["lower"], c["pooled"]["point"])
        b = out["bootstrap"]["S-q"]
        self.assertLess(b["pooled"]["lo95"], b["pooled"]["hi95"])
        self.assertEqual(sum(b["complete_units"].values()), 53, "one unit lost its S game")
        m = out["mechanism"]["S"]["pooled"]
        self.assertEqual(m["scheduled"], 108)
        self.assertEqual(m["counts"]["unknown"], 1)
        self.assertGreater(m["windows"]["inside"]["decisions"], 0)
        self.assertEqual(sum(v.get("decisions", 0) for v in m["ages"].values()), 107 * 6)
        cd = out["context_dependence"]["S:r1"]
        self.assertGreater(cd["rows"], 0)
        self.assertTrue(all(v["tv_initial_trained"] < 1e-12 for v in cd["cells"].values()))
        self.assertIsNotNone(cd["age_budget_baseline"]["positive"])

    def test_bootstrap_is_seeded_and_joint(self):
        units = {lin: {lab: {(o, f): float((o + f + k) % 2) for o in range(9) for f in range(3)}
                       for k, lab in enumerate(("a", "b"))} for lin in A.LINEAGES}
        x = R.bootstrap(units, [("a", "b")], [0, 1, 2])
        self.assertEqual(x, R.bootstrap(units, [("a", "b")], [0, 1, 2]))
        self.assertEqual(x["a-b"]["complete_units"], {"r1": 27, "r2": 27})

    def test_windows_merge_offer_and_two_before(self):
        dec = [{"spy_cast_offered": p in (5, 6), "focal_physical": p} for p in range(10)]
        self.assertEqual(R.windows(dec), {3, 4, 5, 6})
