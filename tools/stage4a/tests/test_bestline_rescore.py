import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import bestline_rescore as B  # noqa: E402


def node(i, parent, edge, k, n, w, labels=None):
    return {"key": f"{i:064x}", "parent": parent, "edge": edge, "depth": 0 if parent == -1 else 1,
            "k": k, "perm": list(range(k)), "labels": labels, "n": n, "wins": w}


def proven_line_hidden_by_exploration():
    """Root: edge 0 leads to our choice node 1, where edge 0 always wins with
    no rollout choice after it and edge 1 always loses through the rollout
    policy; root edge 1 leads to node 2, which won 6 of 10 by rollout."""
    sims = []
    for i in range(10):
        sims.append([len(sims), 1, 1, 0, [0, 0, 1, 0]])
        sims.append([len(sims), 0, 1, 2, [0, 0, 1, 1]])
        sims.append([len(sims), int(i < 6), 1, 3, [0, 1, 2, 0]])
    nodes = [
        node(0, -1, None, 2, [20, 10], [10, 6]),
        node(1, 0, 0, 2, [10, 10], [10, 0], {"spy_self": [0], "spy_other": [1]}),
        node(2, 0, 1, 1, [10], [6]),
    ]
    return {"schema": "s4a-path-log/search-v1", "model": "m", "root": "r", "focal": 0, "root_step": 1,
            "select_cap": len(sims), "tree_hash": "h", "nodes": nodes, "sims": sims}


class BestLine(unittest.TestCase):
    def run_on(self, log, *extra):
        d = tempfile.mkdtemp()
        with open(os.path.join(d, "x.search.json"), "w") as f:
            json.dump(log, f)
        out = os.path.join(d, "out")
        B.main([d, "--out", out, "--m", "3", *extra])
        with open(os.path.join(out, "searches.jsonl")) as f:
            row = json.loads(f.readline())
        with open(os.path.join(out, "summary.json")) as f:
            summary = json.load(f)
        return row, summary

    def test_best_line_replaces_our_exploratory_losses(self):
        row, summary = self.run_on(proven_line_hidden_by_exploration())
        self.assertEqual(row["integrity_mismatched_nodes"], 0)
        self.assertEqual(row["current_pick"], 1, "greatest mean: 0.6 beats 0.5")
        m3 = row["by_m"]["3"]
        self.assertEqual(m3["pick_floor8"], 0)
        self.assertAlmostEqual(m3["root_value"], 1.0)
        self.assertTrue(m3["changed_floor8"])
        self.assertEqual(summary["m3"]["spy_target_nodes"],
                         {"nodes": 1, "changed": 0, "changed_by_tie": 0, "bestline_tied": 0, "changes": {}})
        self.assertFalse(m3["changed_by_tie_floor8"], "1.0 beats node 2's 0.6")
        self.assertEqual(summary["m3"]["root_pick_changed_by_tie_floor8"], 0)
        edges = {e["edge"]: e for e in row["root_edges"]}
        self.assertEqual(set(edges), {0, 1})
        self.assertEqual((edges[0]["n"], edges[0]["wins"]), (20, 10))
        self.assertAlmostEqual(edges[0]["q"]["3"], 1.0)
        self.assertAlmostEqual(edges[1]["q"]["3"], 0.6)

    def test_ties_settled_by_the_seeded_order_are_counted(self):
        # Root edge 1 now wins all 10 of its rollouts: both root edges are
        # worth 1.0, the greatest mean (1.0 > 0.5) picks edge 1 and the best
        # line's seeded order picks edge 0.
        log = proven_line_hidden_by_exploration()
        for s in log["sims"]:
            if s[4][1] == 1:
                s[1] = 1
        log["nodes"][0]["wins"] = [10, 10]
        log["nodes"][2]["wins"] = [10]
        row, summary = self.run_on(log)
        m3 = row["by_m"]["3"]
        self.assertEqual((row["current_pick"], m3["pick_floor8"]), (1, 0))
        self.assertTrue(m3["changed_by_tie_floor8"])
        self.assertTrue(m3["tie_floor8"])
        self.assertFalse(row["current_tie"], "means 0.5 and 1.0 differ")
        self.assertEqual(summary["m3"]["root_pick_changed_by_tie_floor8"], 1)
        [changed] = summary["m3"]["changed_roots"]
        self.assertTrue(changed["changed_by_tie"])

    def test_proven_line_needs_rollout_free_wins(self):
        row, _ = self.run_on(proven_line_hidden_by_exploration())
        stops = row["by_m"]["3"]["stops"]
        self.assertIsNotNone(stops["proven5"])
        self.assertEqual(stops["proven5"]["pick"], 0)
        self.assertIsNone(stops["bounds"], "rival's line is thin: upper bound stays high")
        # The same wins after a rollout-policy choice prove nothing.
        log = proven_line_hidden_by_exploration()
        for s in log["sims"]:
            s[3] = max(s[3], 1)
        row, _ = self.run_on(log)
        self.assertIsNone(row["by_m"]["3"]["stops"]["proven5"])

    def test_rollout_results_count_until_the_edge_has_m_clean_outcomes(self):
        log = proven_line_hidden_by_exploration()
        s = B.Search.__new__(B.Search)
        s.nodes, s.sims, s.path = log["nodes"], log["sims"], "x"
        s.perm = [n["perm"] for n in s.nodes]
        s.skip = [[False] * n["k"] for n in s.nodes]
        st = B.State(s, [3])
        for sim in s.sims[:3]:
            st.add(sim)
        Q, V = st.best_line(3)
        # One clean win and one rollout loss at node 1: fewer than 3 clean.
        self.assertAlmostEqual(Q[1][0], 1.0)
        self.assertAlmostEqual(Q[1][1], 0.0)
        self.assertAlmostEqual(V[1], 0.5, msg="no edge has 3 tries: plain mean")

    def test_clopper_pearson_bounds(self):
        self.assertAlmostEqual(B.cp_low(13, 16), 0.583, places=2)
        self.assertAlmostEqual(B.cp_up(2, 4), 0.902, places=2)


if __name__ == "__main__":
    unittest.main()
