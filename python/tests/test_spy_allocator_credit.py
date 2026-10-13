"""Paired terminal credit: exact gradients, sparse inclusion and update failure handling."""
import copy
import importlib.util
import itertools
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import numpy as np

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("allocator2", ROOT / "tools/spy_reset/allocator2.py")
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)
PARENT = {"lineage": "r1", "weights_sha256": "fixture", "feature_contract_digest": "c",
          "feature_encoding_digest": "e"}


def collect(wb, count=4, draw=0.99, end="win", baseline="loss", sample_seed=19):
    w = A.read_weights(wb)
    remaining = A.CAP
    records = []
    for i in range(count):
        rec = A.consult_record(w, [0.2] * len(A.S_FEATURES), [], remaining, A.CAP, draw, i)
        remaining -= rec["amount"]
        records.append(rec)
        if not remaining:
            break
    row = A.synthetic_row("fixture", "S", "r1", wb, records, end)
    indices = A.reservoir_indices(sample_seed, len(records), 1)
    row["game"]["paired_credit"] = {
        "schema": "allocator-paired-v1", "coupling": "event-step-v1", "max_roots": 1,
        "sample_seed": sample_seed, "eligible": len(records), "samples": [
            {"index": i, "amount": records[i]["amount"], "inclusion_probability": 1 / len(records),
             "baseline": {"valid": True, "end": end if records[i]["amount"] == 0 else baseline}}
            for i in indices]}
    return row


class PairedCredit(unittest.TestCase):
    def setUp(self):
        self.wb = A.init_weights("S", PARENT, None, "paired-test")
        self.w = A.read_weights(self.wb)

    def test_exact_expected_gradient_matches_terminal_objective_with_aliases(self):
        """Integrate common terminal U exactly, then compare to finite differences."""
        params = self.w["params"]
        params["ba"][:] = [0.3, -0.2, 0.1, 0.5]
        for remaining in (A.CAP, 100_000, 10_000):
            cs, q = A.choices(remaining), A.base_q(remaining, A.CAP)
            x = np.linspace(-0.2, 0.8, len(A.S_FEATURES))
            def probabilities(p):
                actor = A.forward(p, self.w["norm"], x)[2]
                return 0.8 * A.coalesce(A.policy(q, actor), cs) + 0.2 * A.coalesce(q, cs)
            mu = probabilities(params)
            win_prob = np.array([0.45, 0.30, 0.65, 0.51][:len(cs)])
            expected = np.zeros(A.NA)
            boundaries = sorted({0.0, 1.0, *win_prob})
            for a, prob in enumerate(mu):
                for lo, hi in zip(boundaries, boundaries[1:]):
                    u = (lo + hi) / 2
                    delta = 2 * (int(u < win_prob[a]) - int(u < win_prob[0]))
                    d = {"x": x, "q": q, "cs": cs, "chosen": a, "mu_b": mu,
                         "adv": delta, "critic_coef": 0.0}
                    grad, _ = A.minibatch_grad(params, self.w["norm"], [{"reward": 1, "decisions": [d]}])
                    expected += prob * (hi - lo) * grad["ba"]
            def objective(p):
                return -float(probabilities(p) @ (2 * win_prob - 1))
            for a in range(A.NA):
                up, down = copy.deepcopy(params), copy.deepcopy(params)
                up["ba"][a] += 1e-6
                down["ba"][a] -= 1e-6
                numerical = (objective(up) - objective(down)) / 2e-6
                self.assertAlmostEqual(expected[a], numerical, places=8)

    def test_sparse_weighting_recovers_sum_with_action_dependent_lengths(self):
        # Enumerate paths with different action-dependent lengths and every uniform subset.
        for values in ([0.2], [-0.5, 0.7], [0.1, -0.3, 0.9, 0.4]):
            for capacity in (1, 2, 3):
                k = min(len(values), capacity)
                estimates = [sum(values[i] for i in subset) * len(values) / k
                             for subset in itertools.combinations(range(len(values)), k)]
                self.assertAlmostEqual(sum(estimates) / len(estimates), sum(values))
        self.assertEqual(A.reservoir_indices(19, 12, 1), [5])
        self.assertEqual(A.reservoir_indices(19, 12, 3), [1, 6, 10])

    def test_update_replay_and_zero_signal_preserve_parameters_and_momentum(self):
        rows = [collect(self.wb) for _ in range(18)]
        out, adam, _ = A.update(self.wb, None, rows, 0, "r1")
        self.assertEqual((out, adam), A.update(self.wb, None, rows, 0, "r1")[:2])
        quiet = [collect(out, end="loss", baseline="loss") for _ in range(18)]
        after, adam_after, record = A.update(out, adam, quiet, 1, "r1")
        for key in A.PARAMS:
            np.testing.assert_array_equal(A.read_weights(after)["params"][key], A.read_weights(out)["params"][key])
        for key in ("m", "v", "t"):
            import json
            self.assertEqual(json.loads(adam)[key], json.loads(adam_after)[key])
        self.assertEqual(record["mean_advantage"], 0)

    def test_skip_ties_and_empty_games_are_valid(self):
        rows = [collect(self.wb, count=0 if i == 0 else 5, draw=0.0) for i in range(18)]
        after, _, record = A.update(self.wb, None, rows, 0, "r1")
        self.assertEqual(len(record["returns"]), 18)
        for key in A.PARAMS:
            np.testing.assert_array_equal(A.read_weights(after)["params"][key], self.w["params"][key])

    def test_bad_pair_is_not_a_tie_loss_or_missing_sample(self):
        mutations = [
            lambda p: p["samples"].clear(),
            lambda p: p["samples"][0].update(inclusion_probability=1.0),
            lambda p: p["samples"][0].update(amount=123),
            lambda p: p["samples"][0]["baseline"].update(valid=False),
            lambda p: p["samples"][0]["baseline"].update(end="nonnatural"),
        ]
        for mutate in mutations:
            rows = [collect(self.wb) for _ in range(18)]
            mutate(rows[0]["game"]["paired_credit"])
            with self.assertRaises((ValueError, A.TechnicalNonterminal)):
                A.update(self.wb, None, rows, 0, "r1")

    def test_saving_the_budget_for_later_gives_negative_credit_now(self):
        # An early request spends the last 512k. Winning later needs that 512k.
        cap = 512_000
        rec = A.consult_record(self.w, [0.2] * len(A.S_FEATURES), [], cap, cap, 0.99, 0)
        terminal = lambda remaining: "win" if remaining >= cap else "loss"
        row = A.synthetic_row("opportunity-cost", "S", "r1", self.wb, [rec], terminal(cap - rec["amount"]))
        row["game"]["paired_credit"] = {"schema":"allocator-paired-v1", "coupling":"event-step-v1",
            "max_roots":1, "sample_seed":19, "eligible":1, "samples":[
                {"index":0, "amount":rec["amount"], "inclusion_probability":1.0,
                 "baseline":{"valid":True, "end":terminal(cap)}}]}
        reward, decisions = A.game_terms(row, self.w, A.sha(self.wb))
        A.paired_advantages(row, decisions, reward)
        self.assertEqual(decisions[0]["adv"], -2.0)
        gradient, _ = A.minibatch_grad(self.w["params"], self.w["norm"], [{"reward":reward, "decisions":decisions}])
        self.assertGreater(gradient["ba"][3], 0, "descent must reduce the wasteful request logit")

    def test_report_uses_whole_games_and_charges_suffixes(self):
        rspec = importlib.util.spec_from_file_location("credit_report", ROOT / "tools/spy_reset/allocator_credit_report.py")
        r = importlib.util.module_from_spec(rspec)
        rspec.loader.exec_module(r)
        rows = [collect(self.wb), collect(self.wb, end="loss", baseline="win")]
        for row in rows:
            row["game"]["paired_credit"]["samples"][0].update(extra_search_transitions=128, extra_real_transitions=10)
        result = r.report(self.wb, rows, 12)
        self.assertEqual((result["games"], result["sampled"], result["discordant"]), (2, 2, 2))
        self.assertEqual((result["extra_search_transitions"], result["cpu_seconds_per_completed_game"]), (256, 6))
        self.assertGreater(result["variance_times_cpu"], 0)
        for row in rows:
            del row["game"]["paired_credit"]
        self.assertIsNone(r.report(self.wb, rows)["variance_times_cpu"])

    def test_job_identity_rejects_stale_checkpoint_case_or_credit(self):
        c = A.domain_rows("devval")[0][0]
        line = A.configured_case(c, "S", {"credit_roots":1})
        src = line["case"]
        row = {k: src[k] for k in ("own", "other", "seat", "opp", "seed", "starting_player", "search_root")}
        row.update(kind="allocator2_game", id=line["id"], arm="S", lineage=c["lineage"], cap=A.CAP,
                   live_policy_seeds=src["guide_policy_seeds"], controller_seed=line["controller_seed"],
                   credit_config=line["credit"], controller={"weights_sha256":"w"})
        self.assertTrue(A.matches_case(row, line, c["lineage"], A.CAP, "w"))
        for field, value in (("seed", -1), ("cap", 512_000), ("credit_config", None)):
            changed = dict(row, **{field:value})
            self.assertFalse(A.matches_case(changed, line, c["lineage"], A.CAP, "w"))
        self.assertFalse(A.matches_case(row, line, c["lineage"], A.CAP, "new-w"))

    def test_probe_file_cannot_bypass_paired_qualification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cases = root / "probes.jsonl"
            c = A.domain_rows("devval")[0][0]
            line = dict(A.configured_case(c, "q", {"credit_roots":3}), lineage=c["lineage"])
            cases.write_text(json.dumps(line) + "\n")
            plan = {"out_dir":str(root / "out"), "schedule":str(root), "probe_cases":str(cases)}
            with patch.object(A, "admit", return_value=root / "binary"), patch.object(A, "run_jobs") as run:
                with self.assertRaises(ValueError):
                    A.queue(plan)
                run.assert_not_called()

    def test_state_replacement_preserves_previous_state_if_interrupted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "STATE.json"
            A.write_state(path, {"next_update":1})
            with patch.object(A.os, "replace", side_effect=OSError("interrupted")):
                with self.assertRaises(OSError):
                    A.write_state(path, {"next_update":2})
            self.assertEqual(json.loads(path.read_text()), {"next_update":1})

    def test_second_cycle_preserves_adam_and_global_update_index(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cases, batches = A.domain_rows("dev-c2")
            (root / "cases-dev-c2.jsonl").write_bytes(A.jsonl(cases))
            (root / "batches-dev-c2.json").write_text(json.dumps(batches[:1]))
            init, init_adam = {}, {}
            for lineage in A.LINEAGES:
                wb = A.weights_bytes("S", self.w["params"], self.w["norm"], dict(PARENT, lineage=lineage),
                                     9, self.w["doc"]["init_seed"], "previous")
                zeros = {k:np.zeros_like(v) for k, v in self.w["params"].items()}
                adam = A.adam_bytes(zeros, zeros, 7, A.sha(wb))
                init[lineage], init_adam[lineage] = str(root / f"w-{lineage}.json"), str(root / f"a-{lineage}.json")
                Path(init[lineage]).write_bytes(wb)
                Path(init_adam[lineage]).write_bytes(adam)
            plan = {"arm":"S", "domain":"dev-c2", "out_dir":str(root / "out"), "schedule":str(root),
                    "binary_sha256":"fixture", "init":init, "init_adam":init_adam, "credit_roots":1}
            def fill(_plan, _binary, jobs, _slots, _ledger, _phase):
                for job in jobs:
                    row = collect(Path(job["weights"]).read_bytes(), count=1, end="loss", baseline="loss")
                    line, src = job["line"], job["line"]["case"]
                    row.update({k:src[k] for k in ("own", "other", "seat", "opp", "seed", "starting_player", "search_root")})
                    row.update(id=line["id"], lineage=job["lineage"], cap=A.CAP, live_policy_seeds=src["guide_policy_seeds"],
                               controller_seed=line["controller_seed"], credit_config=line["credit"])
                    row["game"]["paired_credit"]["sample_seed"] = line["credit"]["sample_seed"]
                    Path(job["out"]).write_text(json.dumps(row) + "\n")
            with patch.object(A, "admit", return_value=root / "binary"), patch.object(A, "run_jobs", side_effect=fill), \
                    patch.object(A, "BATCHES_PER_CYCLE", 1):
                self.assertEqual(A.train(plan), 0)
            state = json.loads((root / "out/STATE.json").read_text())
            self.assertEqual((state["base_update"], state["next_update"]), (9, 1))
            for lineage in A.LINEAGES:
                saved = state["lineages"][lineage]
                weights = A.read_weights((root / "out" / saved["weights"]).read_bytes())
                adam = json.loads((root / "out" / saved["adam"]).read_text())
                self.assertEqual(weights["doc"]["update"], 10)
                self.assertEqual(adam["t"], 7, "zero paired signal must preserve incoming optimizer history")


if __name__ == "__main__":
    unittest.main()
