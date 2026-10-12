"""Search scheduler track (discovery-efficiency-v1): schedule shape, controller math,
update determinism and parity with the Rust schema. No games or search run."""
import copy
import importlib.util
import json
import math
import re
import tempfile
import unittest
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("allocator", REPO / "tools/spy_reset/allocator.py")
alloc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(alloc)
spec = importlib.util.spec_from_file_location("allocator_report", REPO / "tools/spy_reset/allocator_report.py")
report = importlib.util.module_from_spec(spec)
spec.loader.exec_module(report)


def lcg(seed):
    state = seed
    while True:
        state = (state * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        yield (state >> 11) / float(1 << 53)


def random_controller(seed):
    r = lcg(seed)
    return [[v + next(r) - 0.5 for v in row] for row in alloc.init_weights()]


def random_row(seed):
    r = lcg(seed)
    return [1.0] + [next(r) for _ in range(alloc.NF - 1)]


def fake_row(gid, weights_bytes, end, decisions, seed):
    """A learned training row whose logged pi/mu match the weights."""
    w = alloc.read_weights(weights_bytes)["weights"]
    r = lcg(seed)
    dec = []
    for i in range(decisions):
        remaining = [512_000, 100_000, 10_000, 64_000][i % 4]
        x = random_row(seed * 31 + i)
        cs = alloc.choices(remaining)
        pi = alloc.pi_of(w, x)
        mu = alloc.mixture(pi)
        mu_c = [sum(mu[a] for a in m) for _, m in cs]
        u, acc, chosen = next(r), 0.0, len(cs) - 1
        for j, p in enumerate(mu_c):
            acc += p
            if u < acc:
                chosen = j
                break
        dec.append({"chooser": "plain", "scheduler": {
            "index": i, "x": x, "remaining": remaining, "amounts": [c[0] for c in cs],
            "members": [c[1] for c in cs], "pi": [sum(pi[a] for a in m) for _, m in cs], "mu": mu_c,
            "u": u, "chosen": chosen, "amount": cs[chosen][0]}})
        dec.append({"chooser": "reused"})
    return {"kind": "allocator_game", "id": gid, "arm": "learned", "cap": alloc.CAP,
            "controller": {"features": list(alloc.FEATURES), "weights_sha256": alloc.sha(weights_bytes)},
            "game": {"valid": True, "end": end, "decisions": dec, "searches": [], "transitions_used": 0},
            "wall": 1.0 + seed}


class Schedule(unittest.TestCase):
    def test_training_roster(self):
        files, batches = alloc.training()
        self.assertEqual(sum(len(v) for v in files.values()), 144)
        self.assertEqual(len(batches), 24)
        refs = [tuple(g) for b in batches for g in b["games"]]
        self.assertEqual(len(refs), 144)
        self.assertEqual(len(set(refs)), 144)
        for b in batches:
            self.assertEqual(len(b["games"]), 6)
            self.assertEqual(len(b["units"]), 3)
        for lin, rows in files.items():
            by_own = {}
            for c in rows:
                s = c["case"]
                by_own.setdefault(s["own"], {}).setdefault(s["other"], set()).add(s["opp"])
                fam = int(re.search(r"-f(\d)-", s["id"]).group(1))
                self.assertEqual(s["starting_player"], (s["own"] + s["other"] + fam) % 2)
                self.assertEqual(c["arm"], "learned")
            self.assertEqual(sorted(by_own), list(range(9)))
            for own, others in by_own.items():
                self.assertEqual(len(others), 2, "two distinct opposing decks")
                self.assertEqual(sorted(p for v in others.values() for p in v), ["A48", "T1"])
        # Batches keep both seats of a unit together.
        for b in batches:
            for i in (0, 2, 4):
                (l0, i0), (l1, i1) = b["games"][i], b["games"][i + 1]
                c0, c1 = files[l0][i0]["case"], files[l1][i1]["case"]
                self.assertEqual((l0, c0["own"], c0["other"], c0["seed"] != c1["seed"]),
                                 (l1, c1["own"], c1["other"], True))
                self.assertEqual((c0["seat"], c1["seat"]), (0, 1))

    def test_heldout_roster(self):
        total = 0
        for lin in alloc.LINEAGES:
            rows, units = alloc.heldout(lin)
            self.assertEqual(len(rows), 34 * 3)
            self.assertEqual(len(units), 17)
            total += len(rows)
            spy = [r for r in rows if r["case"]["own"] == alloc.SPY]
            self.assertEqual(len(spy), 54)
            self.assertEqual(sorted({r["case"]["other"] for r in spy}), list(range(9)))
            ctrl = Counter(r["case"]["own"] for r in rows if r["case"]["own"] != alloc.SPY)
            self.assertEqual(sorted(ctrl), [o for o in range(9) if o != alloc.SPY])
            self.assertTrue(all(v == 6 for v in ctrl.values()))
            for u in units:
                arms = [rows[i]["arm"] for i in u["indices"]]
                self.assertEqual(sorted(set(arms)), sorted(alloc.ARMS))
                cases = {json.dumps(rows[i]["case"], sort_keys=True) for i in u["indices"]}
                self.assertEqual(len(cases), 2, "arms share each seat's case and seeds")
                self.assertEqual(len({rows[i]["controller_seed"] for i in u["indices"]}), 2)
            policy = Counter(r["case"]["opp"] for r in rows if r["case"]["own"] != alloc.SPY)
            self.assertEqual(policy, Counter({"T1": 24, "A48": 24}))
        self.assertEqual(total, 204)

    def test_seeds_disjoint_and_63_bit(self):
        with tempfile.TemporaryDirectory() as d:
            s = alloc.write_schedule(Path(d))
            self.assertEqual(s["overlap_between_domains"], 0)
            seeds = []
            for name in ("train-r1.jsonl", "train-r2.jsonl"):
                for l in (Path(d) / name).read_text().splitlines():
                    seeds += alloc.case_seeds(json.loads(l))
            self.assertTrue(all(0 <= v < 2 ** 63 for v in seeds))
            again = Path(d) / "again"
            alloc.write_schedule(again)
            for name in ("train-r1.jsonl", "heldout-r2.jsonl", "train-batches.json"):
                self.assertEqual((Path(d) / name).read_bytes(), (again / name).read_bytes())

    def test_qualification_cases(self):
        for lin in alloc.LINEAGES:
            rows = alloc.qualification(lin)
            self.assertEqual(len(rows), 6)
            self.assertEqual(Counter(r["arm"] for r in rows), Counter({"current": 3, "random": 3}))
            self.assertEqual(rows[0]["case"]["own"], alloc.SPY)


class ControllerMath(unittest.TestCase):
    def test_rust_schema_parity(self):
        src = (REPO / "mtg-kernel/src/regret_census_v1/stage4a/allocator.rs").read_text()
        block = src[src.index("pub(super) const FEATURES"):]
        block = block[:block.index("];")]
        self.assertEqual(tuple(re.findall(r'"([a-z0-9_]+)"', block)), alloc.FEATURES)
        self.assertIn('WEIGHTS_SCHEMA: &str = "%s"' % alloc.WEIGHTS_SCHEMA, src)
        self.assertIn('FEATURE_SCHEMA: &str = "%s"' % alloc.FEATURE_SCHEMA, src)
        self.assertIn("Q: [f64; NA] = [0.95, 0.0125, 0.0125, 0.0125, 0.0125]", src)
        self.assertIn("EPSILON: f64 = 0.2", src)

    def test_coalescing(self):
        self.assertEqual(alloc.choices(100_000), [(0, [0]), (16_000, [1]), (64_000, [2]), (100_000, [3, 4])])
        self.assertEqual(alloc.choices(5), [(0, [0]), (5, [1, 2, 3, 4])])
        self.assertEqual(len(alloc.choices(alloc.CAP)), 5)

    def test_init_is_q(self):
        w = alloc.init_weights()
        for s in range(5):
            pi = alloc.pi_of(w, random_row(s))
            for a in range(alloc.NA):
                self.assertAlmostEqual(pi[a], alloc.Q[a], places=15)
                self.assertAlmostEqual(alloc.mixture(pi)[a], alloc.Q[a], places=15)

    def test_gradient_finite_differences(self):
        for s, remaining in ((1, 512_000), (2, 100_000), (3, 10_000), (4, 1)):
            w, x = random_controller(s), random_row(s + 50)
            for amount, _ in alloc.choices(remaining):
                g = alloc.grad_log_mu(w, x, remaining, amount)
                for j in range(alloc.NA):
                    for k in range(alloc.NF):
                        hstep = 1e-6
                        up, dn = copy.deepcopy(w), copy.deepcopy(w)
                        up[j][k] += hstep
                        dn[j][k] -= hstep
                        fd = (alloc.log_mu(up, x, remaining, amount) - alloc.log_mu(dn, x, remaining, amount)) / (2 * hstep)
                        self.assertLess(abs(fd - g[j][k]), 1e-7, (remaining, amount, j, k))


class Update(unittest.TestCase):
    def batch(self, w0, ends=("win", "loss", "draw", "win", "loss", "loss")):
        return [fake_row(f"g{i}", w0, end, decisions=i, seed=i + 1) for i, end in enumerate(ends)]

    def test_update_matches_reinforce_with_baseline_and_clip(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        rows = self.batch(w0)
        new, baseline, rec = alloc.update(w0, rows, 0.25, 0)
        # Reference: explicit mean over games of (R - b) * sum of grads, clipped to norm 1.
        w = alloc.init_weights()
        grad = [[0.0] * alloc.NF for _ in range(alloc.NA)]
        for r in rows:
            reward = alloc.REWARD[r["game"]["end"]]
            for d in r["game"]["decisions"]:
                s = d.get("scheduler")
                if s:
                    g = alloc.grad_log_mu(w, s["x"], s["remaining"], s["amount"])
                    for j in range(alloc.NA):
                        for k in range(alloc.NF):
                            grad[j][k] += (reward - 0.25) * g[j][k] / 6
        norm = math.sqrt(sum(v * v for row in grad for v in row))
        scale = min(1.0, 1.0 / norm)
        got = json.loads(new)["weights"]
        for j in range(alloc.NA):
            for k in range(alloc.NF):
                self.assertAlmostEqual(got[j][k], w[j][k] + 0.01 * scale * grad[j][k], places=14)
        mean_r = sum(alloc.REWARD[r["game"]["end"]] for r in rows) / 6
        self.assertAlmostEqual(baseline, 0.9 * 0.25 + 0.1 * mean_r, places=15)
        self.assertEqual(rec["returns"], [1, -1, 0, 1, -1, -1])
        self.assertEqual(rec["decisions"], [0, 1, 2, 3, 4, 5], "zero-decision games count")

    def test_zero_decision_batch_leaves_weights(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        rows = [fake_row(f"g{i}", w0, "win", 0, i) for i in range(6)]
        new, baseline, _ = alloc.update(w0, rows, 0.0, 0)
        self.assertEqual(json.loads(new)["weights"], alloc.init_weights())
        self.assertAlmostEqual(baseline, 0.1)

    def test_replay_is_byte_identical(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        a1, b1, _ = alloc.update(w0, self.batch(w0), 0.0, 0)
        a2, b2, _ = alloc.update(w0, self.batch(w0), 0.0, 0)
        self.assertEqual(a1, a2)
        # Timing fields do not enter the primary digest.
        rows = self.batch(w0)
        r2 = copy.deepcopy(rows[0])
        r2["wall"] = 99.0
        self.assertEqual(alloc.primary_sha(rows[0]), alloc.primary_sha(r2))
        # Two chained updates through the ledger replay.
        with tempfile.TemporaryDirectory() as d:
            ledger, weights, baseline = Path(d) / "TRAIN-LEDGER.jsonl", w0, 0.0
            for k in range(2):
                rows = [fake_row(f"u{k}g{i}", weights, ("win", "loss")[(i + k) % 2], i, 10 * k + i) for i in range(6)]
                files = []
                for i, r in enumerate(rows):
                    p = Path(d) / f"t{k:02d}-{i}.jsonl"
                    p.write_text(json.dumps(r) + "\n")
                    files.append(str(p))
                weights, baseline, rec = alloc.update(weights, rows, baseline, k)
                rec.update(kind="update", row_files=files)
                with ledger.open("a") as f:
                    f.write(json.dumps(rec) + "\n")
            out = alloc.replay(ledger)
            self.assertEqual(out["mismatches"], [])
            self.assertEqual(out["final_sha256"], alloc.sha(weights))

    def test_mismatched_logs_and_technical_games_refuse(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        rows = self.batch(w0)
        bad = copy.deepcopy(rows)
        bad[3]["game"]["decisions"][0]["scheduler"]["mu"][0] += 1e-6
        with self.assertRaises(ValueError):
            alloc.update(w0, bad, 0.0, 0)
        tech = copy.deepcopy(rows)
        tech[2]["game"]["valid"] = False
        tech[2]["game"]["end"] = "nonnatural"
        with self.assertRaises(alloc.TechnicalNonterminal):
            alloc.update(w0, tech, 0.0, 0)
        with self.assertRaises(alloc.TechnicalNonterminal):
            alloc.update(w0, rows[:5] + [{"kind": "error", "id": "x", "error": "panic"}], 0.0, 0)
        with self.assertRaises(ValueError):
            alloc.update(w0, rows[:5], 0.0, 0)


class Report(unittest.TestCase):
    def test_bounds_and_contrast(self):
        s = report.bounded([1.0, None, 0.0, 0.5])
        self.assertEqual((s["point"], s["worst"], s["best"], s["unknown"]), (0.5, 0.375, 0.625, 1))
        a = {"nine_deck": {"point": 0.6, "worst": 0.5, "best": 0.7}}
        b = {"nine_deck": {"point": 0.4, "worst": 0.4, "best": 0.4}}
        c = report.contrast(a, b)
        self.assertAlmostEqual(c["point"], 0.2)
        self.assertAlmostEqual(c["lower"], 0.1)
        self.assertAlmostEqual(c["upper"], 0.3)

    def test_feature_diagnostics_at_init_are_zero_tv(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        rows = [fake_row("g", w0, "win", 4, 3)]
        d = report.feature_diagnostics(rows, alloc.init_weights())
        self.assertEqual(d["rows"], 4)
        self.assertLess(d["mean_tv_pi_q"], 1e-12)
        self.assertAlmostEqual(d["mean_expected_request_pi"], d["mean_expected_request_q"], places=6)

    def test_report_end_to_end_on_fake_rows(self):
        w0 = alloc.weights_bytes(alloc.init_weights(), 0, 0.0, None)
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            alloc.write_schedule(d / "s")
            (d / "rows").mkdir()
            (d / "w.json").write_bytes(w0)
            ends = ("win", "loss", "draw")
            for lin in alloc.LINEAGES:
                for i, line in enumerate((d / "s" / f"heldout-{lin}.jsonl").read_text().splitlines()):
                    c = json.loads(line)
                    if lin == "r2" and i == 5:
                        continue  # one missing game: unknown, never a loss
                    r = fake_row(c["id"], w0, ends[i % 3], 2 if c["arm"] != "current" else 0, i)
                    r.update(arm=c["arm"])
                    (d / "rows" / f"h-{lin}-{i}.jsonl").write_text(json.dumps(r) + "\n")
            report.main(["--schedule", str(d / "s"), "--rows", str(d / "rows"), "--weights", str(d / "w.json"),
                         "--out", str(d / "out.json")])
            out = json.loads((d / "out.json").read_text())
        self.assertEqual(sum(out["statuses"].values()), 204)
        self.assertEqual(sum(v for k, v in out["statuses"].items() if k.endswith(":unstarted")), 1)
        r2 = out["lineages"]["r2"]
        self.assertEqual(sum(u["unknown"] for arm in r2["arms"].values() for u in arm["decks"].values()), 1)
        for lin in alloc.LINEAGES:
            self.assertEqual(len(out["lineages"][lin]["units"]), 17)
            self.assertEqual(out["lineages"][lin]["spy_bootstrap"]["learned-random"]["units"], 9)
        self.assertLess(out["feature_diagnostics"]["heldout"]["mean_tv_pi_q"], 1e-12)

    def test_spy_bootstrap_is_seeded(self):
        units = [{"learned": [1.0, 0.0], "random": [0.0, 0.0], "current": [0.5, 0.5]} for _ in range(9)]
        units[0]["learned"] = [0.0, 0.0]
        a = report.spy_bootstrap(units, 7)
        self.assertEqual(a, report.spy_bootstrap(units, 7))
        self.assertAlmostEqual(a["learned-random"]["point"], 8 / 18)


if __name__ == "__main__":
    unittest.main()
