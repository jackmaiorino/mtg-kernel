"""Best-line evidence targets (tools/spy_distill/bt1.py): fixture tests. Nothing is searched, played or
trained; every search log, bank, receipt, probe and panel here is synthetic."""
import hashlib
import json
import math
import random
import struct
import sys
import tempfile
import unittest
from collections import Counter
from fractions import Fraction
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from python.tests import test_spy_distill_analysis as panels_fixture  # noqa: E402
from tools.spy_distill import bt1, phase1  # noqa: E402
from tools.spy_distill import distill_common as common  # noqa: E402

# The fixture decision's storage edge e binds live action LIVE[e] in r1.
LIVE = [2, 0, 3, 1]
# The parent's raw logits by live action: its top move is r1's edge 1 (live 0).
LOGITS = [3.0, 0.0, 2.0, 0.5]
STEPS = list(bt1.BASE_STEPS)


def f32_bits(values) -> list:
    return [struct.unpack("<I", struct.pack("<f", v))[0] for v in values]


def write(path: Path, value) -> dict:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value), encoding="utf-8")
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def tree(lineage, root, tree_hash, menus, sims, labels=None, perms=None):
    """A search log. ``menus``: (key, parent index, k) per node in creation
    order; ``sims``: (win, [(node, edge), ...], rollout-policy choices after
    the path). Every node's counts are its sims' own."""
    nodes = [{"key": key, "parent": parent, "k": k, "n": [0] * k, "wins": [0] * k, "visits": 0,
              "perm": (perms or {}).get(i, list(range(k))), "labels": (labels or {}).get(i, {})}
             for i, (key, parent, k) in enumerate(menus)]
    out = []
    for t, (win, path, rollout) in enumerate(sims):
        for i, a in path:
            nodes[i]["n"][a] += 1
            nodes[i]["wins"][a] += win
            nodes[i]["visits"] += 1
        out.append([t, win, 3, rollout, [x for pair in path for x in pair]])
    return {"model": lineage, "root": root, "tree_hash": tree_hash, "select_cap": 0, "nodes": nodes, "sims": out}


def line_tree(lineage, root, tree_hash, labels=None, perm=None):
    """Root "aa" (k=4): edge 0 leads to node "ab", whose edge 0 always wins
    and edge 1 always loses (12 tries each); edge 1 wins 6 of 24 with a
    rollout-policy choice after the path; edge 2 is untried; edge 3 wins 5 of
    5. E executes edge 0 (mean 0.5 against 0.25)."""
    sims = ([(1, [(0, 0), (1, 0)], 0)] * 12 + [(0, [(0, 0), (1, 1)], 0)] * 12
            + [(int(j < 6), [(0, 1)], 1) for j in range(24)] + [(1, [(0, 3)], 0)] * 5)
    return tree(lineage, root, tree_hash, [("aa", -1, 4), ("ab", 0, 2)], sims,
                labels={0: labels or {}}, perms={0: perm or [0, 1, 2, 3]})


def flat_tree(lineage, root, tree_hash):
    """Root "ff" (k=2): each edge wins 6 of 12, so nothing is separated."""
    return tree(lineage, root, tree_hash, [("ff", -1, 2)],
                [(int(j < 6), [(0, a)], 0) for a in (0, 1) for j in range(12)])


def random_tree(rng, attempts):
    """A random search in ``tree``'s terms (menus, sims): each attempt walks
    from the root, taking a random edge at each node and moving on to one of
    at most two child nodes of that edge (a new one is visited at once) until
    it stops; it wins more often the higher its last edge, and its
    rollout-policy choices after the path are random."""
    menus, kids, sims = [("n0", -1, rng.randint(2, 4))], {}, []
    for _ in range(attempts):
        i, path = 0, []
        while True:
            a = rng.randrange(menus[i][2])
            path.append((i, a))
            if len(path) == 5 or rng.random() < 0.3:
                break
            nodes = kids.setdefault((i, a), [])
            if not nodes or (len(nodes) < 2 and rng.random() < 0.15):
                menus.append((f"n{len(menus)}", i, rng.randint(2, 4)))
                nodes.append(len(menus) - 1)
                i = nodes[-1]
            else:
                i = rng.choice(nodes)
        sims.append((int(rng.random() < 0.05 + 0.3 * path[-1][1]), path, rng.choice((0, 0, 1, 2))))
    return menus, sims


def recount(menus, sims, perms, m):
    """bestline_rescore's rule at threshold m from scratch, in exact
    fractions: per edge its value and line counts (wins, tries: the attempts
    through it whose later tree choices follow each node's pick, and every
    attempt below a node with no pick), per node its value, its pick (floor
    m, ties to the seeded order), the edges tied at its value, and whether
    no node from it down has a tie."""
    count, K = len(menus), [k for _, _, k in menus]
    n, w, ct, cw, dt, dw = ([[0] * k for k in K] for _ in range(6))
    visits, wins, kids = [0] * count, [0] * count, {}
    for win, path, rollout in sims:
        for i, a in path:
            n[i][a] += 1
            w[i][a] += win
            visits[i] += 1
            wins[i] += win
        for (i, a), (j, _) in zip(path, path[1:]):
            kids.setdefault((i, a), set()).add(j)
        i, a = path[-1]
        (ct if rollout == 0 else dt)[i][a] += 1
        (cw if rollout == 0 else dw)[i][a] += win
    Q = [[None] * k for k in K]
    V, P, tied, L, clean = [None] * count, [None] * count, [()] * count, [None] * count, [True] * count
    LK = [[(0, 0)] * k for k in K]
    for i in reversed(range(count)):  # every child comes after its parent
        for a in range(K[i]):
            if not n[i][a]:
                continue
            below = kids.get((i, a), ())
            cnt = sum(visits[j] for j in below) + ct[i][a]
            tot = sum(visits[j] * V[j] for j in below) + cw[i][a]
            Q[i][a] = Fraction(tot) / cnt if cnt >= m else Fraction(tot + dw[i][a]) / (cnt + dt[i][a])
            LK[i][a] = (sum(L[j][0] for j in below), sum(L[j][1] for j in below))
            clean[i] = clean[i] and all(clean[j] for j in below)
        eligible = [a for a in perms[i] if n[i][a] >= m]
        if eligible:
            top = max(Q[i][a] for a in eligible)
            best = [a for a in eligible if Q[i][a] == top]
            V[i], P[i], tied[i] = top, best[0], tuple(best)
            clean[i] = clean[i] and len(best) == 1
        else:
            V[i] = Fraction(wins[i], visits[i])
        follow = [P[i]] if P[i] is not None else [a for a in range(K[i]) if n[i][a]]
        L[i] = (sum(cw[i][a] + dw[i][a] + LK[i][a][0] for a in follow),
                sum(ct[i][a] + dt[i][a] + LK[i][a][1] for a in follow))
    lines = [[(cw[i][a] + dw[i][a] + LK[i][a][0], ct[i][a] + dt[i][a] + LK[i][a][1]) if n[i][a] else (0, 0)
              for a in range(K[i])] for i in range(count)]
    return {"n": n, "Q": Q, "V": V, "P": P, "tied": tied, "LK": LK, "kids": kids, "clean": clean, "lines": lines}


SELF_LABELS = {"spy_self": [0], "spy_other": [1]}
# Kinds of fixture decision at line_tree's root: its labels, its bank row's
# line flags, and its self-target and Giant edges.
OFFERS = {
    "plain": ({}, {}, [], []),
    "self": (SELF_LABELS, {"self_offered": True, "self_chosen": True}, [0], []),
    "anti": ({"spy_self": [1], "spy_other": [0]}, {"self_offered": True}, [1], []),
    "giant": ({"dr_giant": [0], "dr_other": [1, 2, 3]}, {"giant_offered": True, "giant_chosen": True}, [], [0]),
}


def bank_row(game, step, k, target, backups, chooser="new_root", line=None, self_actions=(), giant_actions=(),
             logits=None):
    flags = {"cast_offered": False, "cast_chosen": False, "self_offered": False, "self_chosen": False,
             "giant_offered": False, "giant_chosen": False, **(line or {})}
    return {"game": game, "step": step, "physical_decision_id": step, "substep": 0, "k": k, "target": target,
            "chooser": chooser, "edge_backups": backups, "game_quartile": 0, "line": flags,
            "self_actions": list(self_actions), "giant_actions": list(giant_actions),
            "parent_logits": f32_bits(logits or [0.0] * k), "parent_value": 0, "tensor_sha256": "0" * 64}


def offer_row(game, step, live, kind):
    """A bank row for a line_tree root decision of this kind, executed on
    edge 0."""
    _, line, own, giant = OFFERS[kind]
    return bank_row(game, step, 4, live[0], 24, line=line, self_actions=[live[e] for e in own],
                    giant_actions=[live[e] for e in giant], logits=LOGITS)


def bank(lineage, rows, games):
    return {"schema": bt1.BANK_SCHEMA, "lineage": lineage, "rows": rows, "rows_sha256": f"{lineage}-rows",
            "game_status": [{"id": g, "kept": True, "reason": None, "rows": sum(r["game"] == g for r in rows)}
                            for g in games]}


def softmax(values):
    e = [math.exp(v - max(values)) for v in values]
    return [v / sum(e) for v in e]


def renormalized(keep):
    p = softmax(LOGITS)
    return {a: p[a] / sum(p[b] for b in keep) for a in keep}


class Temp(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def game(self, d, lineage, gid, searches, hits, live=True, extra=None):
        """One replay's path logs: search k (root ROOT, or ROOT/refreshk) and
        the actual log; ``hits`` are (search, step, key, live map)."""
        root = f"{lineage}/{gid}"
        for k, make in enumerate(searches):
            write(d / f"{gid}-{k}.search.json", make(lineage, root if k == 0 else f"{root}/refresh{k}", f"h{k}"))
        value = {"schema": bt1.ACTUAL_V2 if live else bt1.ACTUAL_V1, "lineage": lineage, "id": gid,
                 "search_root": root, "searches": [{"search": k, "step": 0, "tree_hash": f"h{k}"}
                                                   for k in range(len(searches))],
                 "hit_fields": ["search", "step", "key"], "hits": [[k, step, key] for k, step, key, _ in hits],
                 **(extra or {})}
        if live:
            value["hit_live"] = [m for *_, m in hits]
        write(d / f"{gid}.actual.json", value)
        return d

    def world(self, live=True, tag="new", r1="self", r2="plain", labels=True):
        """r1 game g1: a decision of kind r1 at step 5 (line_tree) and a plain
        reused row at step 9 (flat_tree, search 1). r2 game g2: a decision of
        kind r2 at step 3 (line_tree with the identity map). Without
        ``labels`` the search logs carry none."""
        def first(kind):
            return lambda *a: line_tree(*a, labels=OFFERS[kind][0] if labels else None)
        g1 = self.game(self.root / tag / "r1-g1", "r1", "g1", [first(r1), flat_tree],
                       [(0, 5, "aa", LIVE), (1, 9, "ff", [1, 0])], live)
        g2 = self.game(self.root / tag / "r2-g2", "r2", "g2", [first(r2)], [(0, 3, "aa", [0, 1, 2, 3])], live)
        return {"r1/g1": g1, "r2/g2": g2}

    def banks(self, extra_r2=(), r1="self", r2="plain"):
        values = (bank("r1", [offer_row("g1", 5, LIVE, r1), bank_row("g1", 9, 2, 1, 12, chooser="reused")], ["g1"]),
                  bank("r2", [offer_row("g2", 3, [0, 1, 2, 3], r2)], ["g2", *extra_r2]))
        out = []
        for value in values:
            pin = write(self.root / "banks" / f"{value['lineage']}.json", value)
            out.append({"lineage": value["lineage"], "bank": pin["path"], "bank_sha256": pin["sha256"]})
        return out

    def verdicts(self, dirs, name="REPLAYS.json", void=(), ta1=None, errors=()):
        """TA1's replay verdicts (stage 0) or, given ``ta1`` (the "LIN/GAME"
        keys TA1 replayed), BT1's REPLAYS.json (stage 2); ``void`` games are
        not exact and ``errors`` games ended check-game in a tool error."""
        replays = [{"lineage": key.split("/")[0], "id": key.split("/")[1], "exact": True, "first_difference": None,
                    "path_logs_sha256": bt1.log_hashes(d)} for key, d in dirs.items()]
        replays += [{"lineage": lin, "id": gid, "exact": False, "first_difference": "$.game.result"}
                    for lin, gid in void]
        replays += [{"lineage": lin, "id": gid, "exact": None, "error": "boom"} for lin, gid in errors]
        if ta1 is None:
            return write(self.root / name, {"schema": "verdicts", "replays": replays})
        for v in replays:
            v["ta1"] = f"{v['lineage']}/{v['id']}" in ta1
        return write(self.root / name, {"schema": bt1.REPLAYS_SCHEMA, "partial": False, "replays": replays})


class Evidence(Temp):
    def state(self, perm=None, labels=None):
        path = write(self.root / "s.search.json", line_tree("r1", "r1/g", "h0", labels, perm))["path"]
        search = bt1.RESCORE.Search(path)
        st = bt1.RESCORE.State(search, [bt1.BESIDE_M, bt1.M])
        for sim in search.sims:
            st.add(sim)
        self.assertEqual(bt1.RESCORE.integrity(search, st), 0)
        return search, st

    def test_the_best_line_separates_a_rival_the_mean_cannot(self):
        search, st = self.state()
        node = search.nodes[0]
        x = bt1.frozen_choice(node["n"], node["wins"], node["perm"])
        self.assertEqual((x, node["n"]), (0, [24, 24, 0, 5]))
        best = bt1.evidence(st, 0, x, bt1.M)
        # Only attempts through the child's best move count for edge 0.
        self.assertEqual(best["counts"], [[12, 12], [6, 24], [0, 0], [5, 5]])
        self.assertEqual((best["leader"], best["tied"], best["separated"], best["evidence"]), (0, [], [1], True))
        # Edge 1 has no pick below it: its line counts are its raw counts.
        self.assertEqual(best["raw_separated"], [1])
        self.assertLess(bt1.RESCORE.cp_up(6, 24), bt1.RESCORE.cp_low(12, 12))
        self.assertAlmostEqual(bt1.RESCORE.cp_low(12, 12), 0.05 ** (1 / 12), places=9)
        mean = bt1.evidence(st, 0, x, None)
        self.assertEqual(mean["counts"], [[12, 24], [6, 24], [0, 0], [5, 5]])
        self.assertEqual((mean["leader"], mean["separated"], mean["raw_separated"], mean["evidence"]),
                         (0, [], [], False))

    def test_untried_and_lightly_tried_edges_are_never_separated_and_never_lead(self):
        # Edge 3 (5 wins in 5) ties edge 0's best-line value and comes first in
        # the seeded order: the floor-3 pick takes it, the floor-8 leader not,
        # and with fewer than 8 tries it is not tied with the leader either.
        search, st = self.state(perm=[3, 0, 1, 2])
        self.assertEqual((st.P[bt1.BESIDE_M][0], st.P[bt1.M][0]), (3, 0))
        for m in (bt1.BESIDE_M, bt1.M):
            rule = bt1.evidence(st, 0, 0, m)
            self.assertEqual((rule["leader"], rule["tied"], rule["separated"]), (0, [], [1]))
            self.assertNotIn(2, rule["separated"])
            self.assertNotIn(3, rule["separated"])
        self.assertEqual(bt1.RESCORE.cp_up(0, 0), 1.0)
        with self.assertRaisesRegex(ValueError, "no leader"):
            bt1.evidence(_NoPick(st), 0, 0, bt1.M)

    def test_p_keeps_the_leader_and_the_edges_tied_with_it(self):
        # Edges 0 and 1 each win 6 of 12, edge 2 none of 12, edge 3 is untried.
        sims = [(int(j < 6), [(0, a)], 0) for a in (0, 1) for j in range(12)] + [(0, [(0, 2)], 0)] * 12
        log = write(self.root / "tie.search.json", tree("r1", "r1/g", "h0", [("tt", -1, 4)], sims))["path"]
        st = bt1.search_state(bt1.RESCORE.Search(log), "tie")
        rule = bt1.evidence(st, 0, 0, bt1.M)
        self.assertEqual((rule["leader"], rule["tied"], rule["separated"]), (0, [1], [2]))
        row = {"k": 4, "live": [3, 1, 0, 2], "rules": {bt1.PRIMARY: rule}}
        self.assertEqual(bt1.keep_sets(row), {"T": [1, 2, 3], "P": [1, 3]})

    def test_the_rescorer_matches_a_from_scratch_recount_on_random_trees(self):
        """Values, picks and line counts from bestline_rescore's incremental
        State against an exact recount from the final attempts, at both
        thresholds, and each rule's evidence from those counts. Where exact
        values tie, the State may break the tie either way, so a pick is
        checked against the tied edges, and line counts only where no node
        below has a tie."""
        compared = Counter()
        for seed in range(10):
            rng = random.Random(seed)
            menus, sims = random_tree(rng, 300)
            perms = {i: rng.sample(range(k), k) for i, (_, _, k) in enumerate(menus)}
            log = write(self.root / f"random{seed}.search.json",
                        tree("r1", "r1/g", f"h{seed}", menus, sims, perms=perms))["path"]
            st = bt1.search_state(bt1.RESCORE.Search(log), f"random tree {seed}")
            for m in (bt1.BESIDE_M, bt1.M):
                exact = recount(menus, sims, perms, m)
                for i in range(len(menus)):
                    self.assertAlmostEqual(st.V[m][i], float(exact["V"][i]), delta=1e-9)
                    for a, q in enumerate(exact["Q"][i]):
                        if q is not None:
                            self.assertAlmostEqual(st.Q[m][i][a], float(q), delta=1e-9)
                    self.assertIn(st.P[m][i], exact["tied"][i] or (None,))
                    compared["picks"] += 1
                    for a, tries in enumerate(exact["n"][i]):
                        if tries and all(exact["clean"][j] for j in exact["kids"].get((i, a), ())):
                            self.assertEqual(st.LK[m][i][a][:2], exact["LK"][i][a])
                            self.assertEqual(bt1.edge_counts(st, i, m)[a], exact["lines"][i][a])
                            compared["line counts"] += 1
                    eligible = [a for a in perms[i] if exact["n"][i][a] >= bt1.LEADER_FLOOR]
                    if not eligible:
                        continue
                    top = max(exact["Q"][i][a] for a in eligible)
                    best = [a for a in eligible if exact["Q"][i][a] == top]
                    below = [j for a in range(len(exact["n"][i])) for j in exact["kids"].get((i, a), ())]
                    if len(best) > 1 or not all(exact["clean"][j] for j in below):
                        continue
                    lines, leader = exact["lines"][i], best[0]
                    low = bt1.RESCORE.cp_low(*lines[leader])
                    rule = bt1.evidence(st, i, leader, m)
                    self.assertEqual((rule["leader"], rule["counts"], rule["separated"]),
                                     (leader, [list(c) for c in lines],
                                      [a for a, c in enumerate(lines) if a != leader and bt1.RESCORE.cp_up(*c) < low]))
                    compared["evidence"] += 1
                    compared["separated"] += len(rule["separated"])
        self.assertGreater(compared["picks"], 2000)
        self.assertGreater(compared["line counts"], 3000)
        self.assertGreater(compared["evidence"], 100)
        self.assertGreater(compared["separated"], 10)

    def test_frozen_choice_needs_eight_tries_and_ties_go_to_the_seeded_order(self):
        self.assertEqual(bt1.frozen_choice([7, 8], [7, 0], [0, 1]), 1)
        self.assertIsNone(bt1.frozen_choice([7, 7], [7, 7], [0, 1]))
        self.assertEqual(bt1.frozen_choice([8, 8], [4, 4], [1, 0]), 1)
        self.assertEqual(bt1.frozen_choice([8, 16], [4, 9], [0, 1]), 1)

    def test_offer_classes_read_the_leader_and_the_separated_side(self):
        own = {"spy_self": [0], "spy_other": [1, 2]}
        self.assertEqual(bt1.self_offer({"leader": 0, "separated": [1, 2]}, own), "self_leads_other_separated")
        self.assertEqual(bt1.self_offer({"leader": 0, "separated": [1]}, own), "neither")
        self.assertEqual(bt1.self_offer({"leader": 1, "separated": [0]}, own), "other_leads_self_separated")
        self.assertEqual(bt1.self_offer({"leader": 3, "separated": [0, 1, 2]}, own), "neither")
        self.assertTrue(bt1.anti_on_raw_counts({"raw_separated": [0]}, own))
        self.assertFalse(bt1.anti_on_raw_counts({"raw_separated": [1]}, own))
        giant = bt1.giant_offer({"leader": 0, "separated": [1], "evidence": True},
                                {"dr_giant": [0], "dr_other": [1, 2, 3]}, [10, 10, 4, 0])
        self.assertEqual(giant, {"giant_leads": True, "evidence": True, "other_separated": 1,
                                 "other_tried_not_separated": 1, "other_untried": 1, "giant_edges": 1,
                                 "giant_untried": 0})
        flags = {"self_chosen": False, "self_offered": True, "giant_chosen": True, "giant_offered": True,
                 "cast_offered": True}
        self.assertEqual(bt1.decision_class(flags), "self_declined")
        self.assertEqual(bt1.decision_class({**flags, "self_offered": False}), "giant_chosen")
        self.assertEqual(bt1.decision_class({k: False for k in flags}), "other")


class _NoPick:
    """A rescorer state whose node has no edge with eight tries."""

    def __init__(self, st):
        self.st = st

    def __getattr__(self, name):
        return getattr(self.st, name)

    def bestline_pick(self, i, value, floor):
        return None


class Join(Temp):
    def rows(self, d, rows, live=True):
        return bt1.game_rows("r1", "g1", {r["step"]: r for r in rows}, d, live)

    def setUp(self):
        super().setUp()
        self.d = self.world()["r1/g1"]
        self.r1 = json.loads(Path(self.banks()[0]["bank"]).read_text())["rows"]

    def first_tree(self, labels=SELF_LABELS):
        return lambda *a: line_tree(*a, labels=labels)

    def test_each_tree_executed_row_joins_its_node_and_live_map(self):
        rows = self.rows(self.d, self.r1)
        self.assertEqual([(r["step"], r["search"], r["key"], r["x"], r["class"]) for r in rows],
                         [(5, 0, "aa", 0, "self_chosen"), (9, 1, "ff", 0, "other")])
        self.assertEqual(rows[0]["live"], LIVE)
        self.assertEqual(rows[0]["labels"], SELF_LABELS)
        self.assertEqual({k: v["evidence"] for k, v in rows[0]["rules"].items()},
                         {"bestline8": True, "bestline3": True, "mean": False})
        self.assertFalse(any(v["evidence"] for v in rows[1]["rules"].values()))
        stage0 = self.rows(self.world(live=False, tag="v1")["r1/g1"], self.r1, live=False)
        self.assertEqual([r["live"] for r in stage0], [None, None])
        self.assertEqual([r["rules"] for r in stage0], [r["rules"] for r in rows])

    def test_any_failed_check_at_any_row_voids_the_game(self):
        row, plain = self.r1
        cases = [
            ([{**row, "step": 6}, plain], "0 actual-log hits"),
            ([{**row, "k": 3}, plain], "differ from the row's menu"),
            ([{**row, "edge_backups": 23}, plain], "tries differ from edge_backups"),
            ([{**row, "target": 0}, plain], "live action differs from the target"),
            ([row, {**plain, "target": 0}], "step 9: the executed edge's live action differs"),
            ([row, {**plain, "chooser": "new_root", "k": 2}], None),
        ]
        for rows, message in cases:
            if message is None:
                self.assertEqual(len(self.rows(self.d, rows)), 2)
                continue
            with self.assertRaisesRegex(bt1.Void, message):
                self.rows(self.d, rows)
        with self.assertRaisesRegex(bt1.Void, "no live maps"):
            self.rows(self.world(live=False, tag="v1")["r1/g1"], self.r1)
        games = [
            ("perm", [self.first_tree(), flat_tree], [(0, 5, "aa", [2, 0, 0, 1]), (1, 9, "ff", [1, 0])],
             self.r1, "not a permutation"),
            ("child", [line_tree], [(0, 5, "ab", [0, 1])], [bank_row("g1", 5, 2, 0, 12)],
             "new_root hit is not its search's root"),
            ("missing", [line_tree], [(0, 5, "zz", LIVE)], [row], "not in search 0's log"),
            ("index", [self.first_tree(), flat_tree], [(5, 5, "aa", LIVE), (1, 9, "ff", [1, 0])], self.r1,
             "the hit names no search"),
        ]
        for tag, searches, hits, rows, message in games:
            with self.assertRaisesRegex(bt1.Void, message):
                self.rows(self.game(self.root / tag, "r1", "g1", searches, hits), rows)
        (self.d / "g1-1.search.json").unlink()
        with self.assertRaisesRegex(bt1.Void, "search 1: 0 search logs"):
            self.rows(self.d, self.r1)

    def test_a_label_check_that_fails_stops_bt1_and_voids_nothing(self):
        """No label check removes a game: a failure raises ValueError, which
        no census catches, rather than Void."""
        row, plain = self.r1
        cases = [
            ([{**row, "line": {**row["line"], "self_chosen": False}}, plain],
             "the node's labels differ from the row's spy_self flags; BT1 stops"),
            ([{**row, "self_actions": [3]}, plain], "spy_self edges' live actions differ from the row's; BT1 stops"),
            ([{**row, "line": {**row["line"], "giant_offered": True}}, plain], "row's dr_giant flags"),
        ]
        for rows, message in cases:
            with self.assertRaisesRegex(ValueError, message):
                self.rows(self.d, rows)
        other = self.game(self.root / "other", "r1", "g1",
                          [self.first_tree({"spy_self": [0], "spy_other": [0, 1]}), flat_tree],
                          [(0, 5, "aa", LIVE), (1, 9, "ff", [1, 0])])
        with self.assertRaisesRegex(ValueError, "opponent-target edge maps to a self-target action; BT1 stops"):
            self.rows(other, self.r1)
        self.assertNotIsInstance(bt1.Void("x"), ValueError)

    def test_logs_whose_counts_or_paths_are_broken_void_the_game(self):
        original = json.loads((self.d / "g1-0.search.json").read_text())
        value = json.loads(json.dumps(original))
        value["nodes"][1]["n"] = [12, 13]
        write(self.d / "g1-0.search.json", value)
        with self.assertRaisesRegex(bt1.Void, "do not rebuild the counts"):
            self.rows(self.d, self.r1)
        value["nodes"][0]["skip"] = [False, True, False, False]
        write(self.d / "g1-0.search.json", value)
        with self.assertRaisesRegex(bt1.Void, "skipped edges"):
            self.rows(self.d, self.r1)
        value = json.loads(json.dumps(original))
        value["sims"][0][4] = [-1, 0]
        write(self.d / "g1-0.search.json", value)
        with self.assertRaisesRegex(bt1.Void, "search 0: .*broken path in attempt 0"):
            self.rows(self.d, self.r1)
        value = json.loads(json.dumps(original))
        del value["nodes"][0]["n"]
        write(self.d / "g1-0.search.json", value)
        with self.assertRaisesRegex(bt1.Void, "the hit's node has no counts"):
            self.rows(self.d, self.r1)


class Census(Temp):
    def spec(self, stage, dirs, verdicts, banks=None):
        return {"stage": stage, "banks": banks or self.banks(), "verdicts": verdicts,
                "path_logs": {k: str(v) for k, v in dirs.items()}}

    def test_stage_0_counts_evidence_rows_and_applies_the_stop_rule(self):
        dirs = self.world(live=False)
        out = self.root / "census0"
        result = bt1.census(self.spec(0, dirs, self.verdicts(dirs)), out)
        r1 = result["lineages"]["r1"]["rules"]
        best, mean = r1["bestline8"], r1["mean"]
        self.assertEqual((best["rows"], best["evidence_rows"]), (2, 1))
        self.assertEqual(best["by_class"]["self_chosen"], {"rows": 1, "evidence_rows": 1})
        self.assertEqual(best["by_class"]["other"], {"rows": 1, "evidence_rows": 0})
        self.assertEqual(best["by_chooser"], {"new_root": {"rows": 1, "evidence_rows": 1},
                                              "reused": {"rows": 1, "evidence_rows": 0}})
        self.assertEqual((best["leader_differs_from_executed"], best["separated_edges"]), (0, 1))
        self.assertEqual(best["separated_per_evidence_row"], {"1": 1})
        self.assertEqual(set(best["tied_leader_evidence_rows"].values()), {0})
        self.assertEqual(best["separated_on_raw_counts"], {c: int(c == "self_chosen") for c in bt1.CLASSES})
        self.assertIsNone(mean["separated_on_raw_counts"])
        # Rivals tried at the evidence row: edges 1 and 3; edge 1 separated.
        self.assertEqual((best["tried_rivals_at_evidence_rows"], best["tried_rivals_separated_share"]), (2, 0.5))
        self.assertEqual(best["self_offers"]["chosen"], {"offers": 1, "self_leads_other_separated": 1,
                                                         "other_leads_self_separated": 0, "neither": 0,
                                                         "other_leads_self_separated_on_raw_counts": 0})
        self.assertEqual(best["self_offers"]["declined"]["offers"], 0)
        self.assertEqual(mean["evidence_rows"], 0)
        self.assertEqual(mean["self_offers"]["chosen"]["neither"], 1)
        self.assertIsNone(mean["self_offers"]["chosen"]["other_leads_self_separated_on_raw_counts"])
        self.assertEqual(result["lineages"]["r2"]["rules"]["bestline8"]["evidence_rows"], 1)
        self.assertEqual(result["stop_rule"], {"self_leads_other_separated": {"r1": 1, "r2": 0}, "stop": True,
                                               "few_self_line_rows": ["r1"],
                                               "other_leads_self_separated": {"r1": 0, "r2": 0},
                                               "giant_leads_rival_separated": {"r1": 0, "r2": 0}})
        self.assertNotIn("keep_sets", result["lineages"]["r1"])
        self.assertEqual(json.loads((out / "CENSUS.json").read_text()), result)
        rows = [json.loads(line) for line in (out / "ROWS-r1.jsonl").read_text().splitlines()]
        self.assertEqual([r["step"] for r in rows], [5, 9])
        self.assertEqual(result["lineages"]["r1"]["rows_file"]["sha256"],
                         hashlib.sha256((out / "ROWS-r1.jsonl").read_bytes()).hexdigest())
        self.assertFalse((out / "KEEP-T-r1.json").exists())

    def test_bt1_goes_on_only_when_both_lineages_have_a_self_target_lead(self):
        dirs = self.world(live=False, r2="self")
        result = bt1.census(self.spec(0, dirs, self.verdicts(dirs), self.banks(r2="self")), self.root / "both")
        self.assertEqual(result["stop_rule"]["self_leads_other_separated"], {"r1": 1, "r2": 1})
        self.assertEqual((result["stop_rule"]["stop"], result["stop_rule"]["few_self_line_rows"]),
                         (False, ["r1", "r2"]))
        g1 = self.game(self.root / "flat" / "r1-g1", "r1", "g1",
                       [lambda *a: flat_tree(*a), flat_tree], [(0, 5, "ff", None), (1, 9, "ff", None)], live=False)
        dirs = {"r1/g1": g1, "r2/g2": self.world(live=False, tag="flat-r2", r2="self")["r2/g2"]}
        banks = self.banks(r2="self")
        r1 = json.loads(Path(banks[0]["bank"]).read_text())
        r1["rows"][0] = bank_row("g1", 5, 2, 0, 12)
        banks[0]["bank_sha256"] = write(Path(banks[0]["bank"]), r1)["sha256"]
        result = bt1.census(self.spec(0, dirs, self.verdicts(dirs, "FLAT.json"), banks), self.root / "out")
        self.assertEqual(result["stop_rule"]["self_leads_other_separated"], {"r1": 0, "r2": 1})
        self.assertEqual((result["stop_rule"]["stop"], result["stop_rule"]["few_self_line_rows"]), (True, ["r2"]))

    def test_stage_2_writes_both_cells_keep_sets_and_target_measures(self):
        dirs = self.world()
        out = self.root / "census2"
        result = bt1.census(self.spec(2, dirs, self.verdicts(dirs, ta1=())), out)
        p = softmax(LOGITS)
        targets = result["lineages"]["r1"]["targets"]
        self.assertEqual((targets["evidence_rows"], targets["parent_top_differs"]), (1, 1))
        self.assertEqual((targets["tied_leader_rows"], targets["t_equals_p_rows"]), (0, 0))
        self.assertAlmostEqual(targets["mean_parent_on_leader"], p[2], places=6)
        t, pc = targets["cells"]["T"], targets["cells"]["P"]
        q = renormalized([1, 2, 3])
        self.assertAlmostEqual(t["kl_total"], -math.log(1 - p[0]), places=6)
        self.assertAlmostEqual(t["kl_by_class"]["self_chosen"], t["kl_total"])
        self.assertEqual((t["kl_where_t_equals_p"], t["kl_elsewhere"]), (0, t["kl_total"]))
        self.assertAlmostEqual(pc["kl_total"], -math.log(p[2]), places=6)
        self.assertAlmostEqual(t["mean_self_mass"], q[2], places=6)
        self.assertAlmostEqual(t["mean_leader_mass"], q[2], places=6)
        # Edge 2 (live 3) is untried: T keeps it at the parent's ratio, P drops it.
        self.assertAlmostEqual(t["mean_untried_mass"], q[3], places=6)
        self.assertEqual((pc["mean_leader_mass"], pc["mean_untried_mass"]), (1.0, 0))
        self.assertEqual((t["self_offer_rows"], pc["mean_self_mass"]), (1, 1.0))
        self.assertEqual((t["giant_offer_rows"], t["mean_giant_mass"]), (0, None))
        self.assertEqual(t["line_rows"]["self"]["keys"], [["g1", 5]])
        self.assertAlmostEqual(t["line_rows"]["self"]["mean_target_mass"], q[2], places=6)
        self.assertEqual(pc["line_rows"]["self"], {"rows": 1, "mean_target_mass": 1.0, "keys": [["g1", 5]]})
        self.assertEqual(t["line_rows"]["anti"], {"rows": 0, "mean_target_mass": None, "keys": [],
                                                  "on_raw_counts": 0})
        self.assertEqual(t["line_rows"]["giant"]["rows"], 0)
        keep = {cell: json.loads((out / f"KEEP-{cell}-r1.json").read_text()) for cell in ("T", "P")}
        self.assertEqual(keep["T"], {"schema": bt1.KEEP_SETS_SCHEMA, "lineage": "r1", "bank_rows_sha256": "r1-rows",
                                     "rows": [{"game": "g1", "step": 5, "keep": [1, 2, 3]}]})
        self.assertEqual(keep["P"]["rows"], [{"game": "g1", "step": 5, "keep": [2]}])
        r2 = json.loads((out / "KEEP-T-r2.json").read_text())["rows"]
        self.assertEqual(r2, [{"game": "g2", "step": 3, "keep": [0, 2, 3]}])
        pins = result["lineages"]["r1"]["keep_sets"]
        self.assertEqual(pins["T"]["sha256"], hashlib.sha256((out / "KEEP-T-r1.json").read_bytes()).hexdigest())
        self.assertTrue(Path(pins["P"]["path"]).is_absolute())

    def test_anti_line_and_giant_line_rows_are_counted(self):
        dirs = self.world(r1="giant", r2="anti")
        result = bt1.census(self.spec(2, dirs, self.verdicts(dirs, ta1=()), self.banks(r1="giant", r2="anti")),
                            self.root / "c")
        r1, r2 = result["lineages"]["r1"], result["lineages"]["r2"]
        self.assertEqual(r1["rules"]["bestline8"]["giant_offers"]["chosen"],
                         {"offers": 1, "giant_leads": 1, "giant_leads_rival_separated": 1, "other_separated": 1,
                          "other_tried_not_separated": 1, "other_untried": 1, "giant_edges": 1, "giant_untried": 0})
        self.assertEqual(r2["rules"]["bestline8"]["self_offers"]["declined"],
                         {"offers": 1, "self_leads_other_separated": 0, "other_leads_self_separated": 1,
                          "neither": 0, "other_leads_self_separated_on_raw_counts": 1})
        self.assertEqual(result["stop_rule"]["giant_leads_rival_separated"], {"r1": 1, "r2": 0})
        self.assertEqual(result["stop_rule"]["other_leads_self_separated"], {"r1": 0, "r2": 1})
        self.assertTrue(result["stop_rule"]["stop"])
        giant = r1["targets"]["cells"]["T"]["line_rows"]
        self.assertEqual((giant["giant"]["rows"], giant["self"]["rows"], giant["anti"]["rows"]), (1, 0, 0))
        self.assertAlmostEqual(giant["giant"]["mean_target_mass"], renormalized([1, 2, 3])[2], places=9)
        # r2's map is the identity: T keeps live 0, 2 and 3; the opponent target is live 0.
        for cell, keep in (("T", [0, 2, 3]), ("P", [0])):
            anti = r2["targets"]["cells"][cell]["line_rows"]["anti"]
            self.assertEqual((anti["rows"], anti["keys"], anti["on_raw_counts"]), (1, [["g2", 3]], 1))
            self.assertAlmostEqual(anti["mean_target_mass"], renormalized(keep)[0], places=9)
            self.assertEqual(r2["targets"]["cells"][cell]["line_rows"]["self"]["rows"], 0)

    def test_keep_sets_read_no_label(self):
        """With every label removed from the search logs and the join's label
        check off, the census writes the same keep sets byte for byte; only
        the counts that read labels change."""
        banks = self.banks(r2="anti")
        dirs = self.world(r2="anti")
        labelled = bt1.census(self.spec(2, dirs, self.verdicts(dirs, ta1=()), banks), self.root / "labelled")
        bare = self.world(tag="bare", r2="anti", labels=False)
        with mock.patch.object(bt1, "check_labels") as check:
            unlabelled = bt1.census(self.spec(2, bare, self.verdicts(bare, "BARE.json", ta1=()), banks),
                                    self.root / "bare")
        self.assertEqual(check.call_count, 3)
        for name in ("KEEP-T-r1.json", "KEEP-P-r1.json", "KEEP-T-r2.json", "KEEP-P-r2.json"):
            self.assertEqual((self.root / "labelled" / name).read_bytes(), (self.root / "bare" / name).read_bytes())
        self.assertEqual(labelled["stop_rule"]["self_leads_other_separated"], {"r1": 1, "r2": 0})
        self.assertEqual(labelled["stop_rule"]["other_leads_self_separated"], {"r1": 0, "r2": 1})
        self.assertEqual(unlabelled["stop_rule"]["self_leads_other_separated"], {"r1": 0, "r2": 0})
        self.assertEqual(unlabelled["stop_rule"]["other_leads_self_separated"], {"r1": 0, "r2": 0})

    def test_the_census_reads_only_verified_logs_and_stage_2_needs_every_kept_game(self):
        dirs = self.world()
        verdicts = self.verdicts(dirs, ta1=())
        with self.assertRaisesRegex(ValueError, "every kept bank game"):
            bt1.census(self.spec(2, dirs, verdicts, self.banks(extra_r2=["g3"])), self.root / "a")
        void = self.verdicts(dirs, "VOID.json", void=[("r2", "g3")], errors=[("r2", "g4")], ta1=())
        with mock.patch.object(bt1, "VOID_RATIO", 2):
            result = bt1.census(self.spec(2, dirs, void, self.banks(extra_r2=["g3", "g4"])), self.root / "b")
        self.assertEqual(result["lineages"]["r2"]["void_games"], [{"id": "g3", "reason": "the replay is not exact"},
                                                                  {"id": "g4", "reason": "check-game: boom"}])
        self.assertEqual((result["lineages"]["r2"]["games"], result["lineages"]["r2"]["void_rows"]), (1, 0))
        foreign = self.verdicts(dirs, "FOREIGN.json", void=[("r2", "g9")])
        with self.assertRaisesRegex(ValueError, "names no kept game"):
            bt1.census(self.spec(0, dirs, foreign), self.root / "c")
        with self.assertRaisesRegex(ValueError, "stage 2 reads BT1's full REPLAYS.json"):
            bt1.census(self.spec(2, dirs, self.verdicts(dirs, "TA1-STYLE.json")), self.root / "f")
        value = json.loads((dirs["r1/g1"] / "g1-1.search.json").read_text())
        value["select_cap"] = 7
        write(dirs["r1/g1"] / "g1-1.search.json", value)
        with self.assertRaisesRegex(ValueError, "path logs differ from the verdict"):
            bt1.census(self.spec(2, dirs, verdicts), self.root / "d")
        with self.assertRaisesRegex(ValueError, "stage 0 or stage 2"):
            bt1.census(self.spec(1, dirs, verdicts), self.root / "e")

    def test_void_games_leave_both_cells_and_more_than_one_in_twenty_stops_as_engineering(self):
        dirs = self.world()
        # g1's actual log lost its step-9 hit: the join fails there, so the whole game is void.
        dirs["r1/g1"] = self.game(self.root / "broken" / "r1-g1", "r1", "g1",
                                  [lambda *a: line_tree(*a, labels=SELF_LABELS), flat_tree], [(0, 5, "aa", LIVE)])
        verdicts = self.verdicts(dirs, ta1=())
        with self.assertRaisesRegex(ValueError, "engineering: 1 of 2 games are void"):
            bt1.census(self.spec(2, dirs, verdicts), self.root / "a")
        self.assertFalse((self.root / "a").exists())
        void = self.verdicts(dirs, "VOID.json", void=[("r2", "g3")], ta1=())
        with mock.patch.object(bt1, "VOID_RATIO", 2), self.assertRaisesRegex(ValueError, "2 of 3 games are void"):
            bt1.census(self.spec(2, dirs, void, self.banks(extra_r2=["g3"])), self.root / "b")
        with mock.patch.object(bt1, "VOID_RATIO", 2):
            result = bt1.census(self.spec(2, dirs, verdicts), self.root / "c")
        r1 = result["lineages"]["r1"]
        self.assertEqual(r1["void_games"], [{"id": "g1", "reason": "r1 g1 step 9: 0 actual-log hits"}])
        self.assertEqual((r1["games"], r1["void_rows"], r1["rules"]["bestline8"]["rows"]), (0, 2, 0))
        self.assertEqual(json.loads((self.root / "c" / "KEEP-T-r1.json").read_text())["rows"], [])
        self.assertEqual(result["stop_rule"]["self_leads_other_separated"]["r1"], 0)

    def test_a_failure_at_a_game_ta1_replayed_or_a_failed_label_check_stops_the_census(self):
        """TA1's games hold every line offer, so none of them may leave the
        cells: a join failure there stops the census (at stage 0 every game is
        one of them), and so does a verdict that is not exact. A label check
        that fails stops it anywhere."""
        dirs = self.world()
        broken = self.game(self.root / "broken" / "r1-g1", "r1", "g1",
                           [lambda *a: line_tree(*a, labels=SELF_LABELS), flat_tree], [(0, 5, "aa", LIVE)])
        with mock.patch.object(bt1, "VOID_RATIO", 2):
            with self.assertRaisesRegex(ValueError, "r1 g1 step 9: 0 actual-log hits: the join failed at a game TA1 "
                                                    "replayed; BT1 stops"):
                bt1.census(self.spec(2, {**dirs, "r1/g1": broken},
                                     self.verdicts({**dirs, "r1/g1": broken}, "TA1-GAME.json", ta1={"r1/g1"})),
                           self.root / "a")
            stage0 = self.world(live=False, tag="v1")
            broken0 = self.game(self.root / "broken0" / "r1-g1", "r1", "g1",
                                [lambda *a: line_tree(*a, labels=SELF_LABELS), flat_tree], [(0, 5, "aa", None)],
                                live=False)
            with self.assertRaisesRegex(ValueError, "the join failed at a game TA1 replayed"):
                bt1.census(self.spec(0, {**stage0, "r1/g1": broken0},
                                     self.verdicts({**stage0, "r1/g1": broken0}, "STAGE0.json")), self.root / "b")
            void = self.verdicts(dirs, "VOID.json", void=[("r2", "g3")], ta1={"r2/g3"})
            with self.assertRaisesRegex(ValueError, "r2 g3: the replay is not exact at a game TA1 replayed"):
                bt1.census(self.spec(2, dirs, void, self.banks(extra_r2=["g3"])), self.root / "c")
            for stage, d, verdicts in ((0, stage0, self.verdicts(stage0, "S0.json")),
                                       (2, dirs, self.verdicts(dirs, "S2.json", ta1=()))):
                with self.assertRaisesRegex(ValueError, "the node's labels differ from the row's spy_self flags"):
                    bt1.census(self.spec(stage, d, verdicts, self.banks(r1="anti")), self.root / f"labels{stage}")
        self.assertFalse(any((self.root / name).exists() for name in ("a", "b", "c", "labels0", "labels2")))

    def test_the_cli_prints_the_stop_rule(self):
        dirs = self.world(live=False)
        spec = write(self.root / "spec.json", self.spec(0, dirs, self.verdicts(dirs)))
        with mock.patch("builtins.print") as printed:
            self.assertEqual(bt1.main(["census", "--spec", spec["path"], "--out-dir", str(self.root / "cli")]), 0)
        summary = json.loads(printed.call_args[0][0])
        self.assertEqual(summary["evidence_rows"], {"r1": 1, "r2": 1})
        self.assertEqual(summary["void_games"], {"r1": 0, "r2": 0})
        self.assertTrue(summary["stop_rule"]["stop"])


class Replays(Temp):
    def setUp(self):
        super().setUp()
        self.old = self.world(live=False, tag="ta1")
        self.new = self.world()
        self.new["r2/g3"] = None
        self.ta1 = self.verdicts({"r1/g1": self.old["r1/g1"]}, "TA1.json")
        patcher = mock.patch.object(bt1, "VOID_RATIO", 3)
        patcher.start()
        self.addCleanup(patcher.stop)

    def spec(self, checks, path_logs=None):
        lines = self.root / "checks.jsonl"
        lines.write_text("".join(json.dumps(c) + "\n" for c in checks))
        return {"banks": self.banks(extra_r2=["g3"]), "checks": str(lines),
                "path_logs": {k: str(v) for k, v in (path_logs or self.new).items() if v is not None},
                "ta1": {"verdicts": self.ta1, "path_logs": {"r1/g1": str(self.old["r1/g1"])}}}

    def checks(self):
        out = []
        for key, d in self.new.items():
            lin, gid = key.split("/")
            entry = {"lineage": lin, "id": gid, "rows": f"{gid}.jsonl", "rows_sha256": "r" * 64,
                     "exact": d is not None, "first_difference": None if d else "$.game.result",
                     "executed_decisions": 1, "line_offers": 0}
            if d is not None:
                entry["path_logs_sha256"] = bt1.log_hashes(d)
            out.append(entry)
        return out

    def test_every_kept_game_is_checked_and_ta1s_games_keep_their_logs(self):
        result = bt1.replays(self.spec(self.checks()))
        self.assertEqual(result["games"], {"r1": 1, "r2": 2})
        self.assertEqual(result["void"], [{"lineage": "r2", "id": "g3", "reason": "the replay is not exact",
                                           "first_difference": "$.game.result"}])
        self.assertEqual(result["ta1_identity"], {"games": 1, "search_logs": 2})
        self.assertEqual((result["partial"], result["ta1_verdicts"]), (False, self.ta1))
        self.assertEqual([(v["lineage"], v["id"], v["ta1"]) for v in result["replays"]],
                         [("r1", "g1", True), ("r2", "g2", False), ("r2", "g3", False)])
        self.assertNotIn("path_logs_sha256", result["replays"][2])
        new_actual = json.loads((self.new["r1/g1"] / "g1.actual.json").read_text())
        self.assertEqual(list(bt1.without_live(new_actual)),
                         list(json.loads((self.old["r1/g1"] / "g1.actual.json").read_text())))
        with mock.patch.object(bt1, "VOID_RATIO", 20), \
                self.assertRaisesRegex(ValueError, "engineering: 1 of 3 games are void"):
            bt1.replays(self.spec(self.checks()))

    def test_a_ta1_game_that_is_not_exact_or_fails_check_game_stops_stage_1(self):
        checks = self.checks()
        checks[0] = {**checks[0], "exact": False, "first_difference": "$.game.result"}
        del checks[0]["path_logs_sha256"]
        with self.assertRaisesRegex(ValueError, "TA1 replayed this game exactly and this replay is not exact"):
            bt1.replays(self.spec(checks))
        checks[0] = {"lineage": "r1", "id": "g1", "error": "g1 step 5: a new_root hit is not its search's root"}
        with self.assertRaisesRegex(ValueError, "check-game failed at a game TA1 replayed exactly"):
            bt1.replays(self.spec(checks))
        # The same error at a game TA1 did not replay voids it.
        self.ta1 = self.verdicts({}, "NO-TA1.json")
        with mock.patch.object(bt1, "VOID_RATIO", 1):
            result = bt1.replays(self.spec(checks))
        self.assertEqual([(v["id"], v["reason"]) for v in result["void"]],
                         [("g1", "check-game: g1 step 5: a new_root hit is not its search's root"),
                          ("g3", "the replay is not exact")])
        self.assertEqual(result["ta1_identity"], {"games": 0, "search_logs": 0})

    def test_a_check_game_error_voids_any_other_game_unless_its_label_check_failed(self):
        checks = self.checks()
        checks[2] = {"lineage": "r2", "id": "g3", "error": "g3 step 4: the hit's node is not in search 0's log"}
        result = bt1.replays(self.spec(checks))
        self.assertEqual(result["void"], [{"lineage": "r2", "id": "g3",
                                           "reason": "check-game: g3 step 4: the hit's node is not in search 0's log",
                                           "first_difference": None}])
        self.assertEqual(result["replays"][2], {"lineage": "r2", "id": "g3", "ta1": False, "exact": None,
                                                "error": "g3 step 4: the hit's node is not in search 0's log"})
        checks[1] = {"lineage": "r2", "id": "g2", "error": "g2 step 3: the node's labels differ from the row's "
                                                           "self_target flags"}
        with self.assertRaisesRegex(ValueError, "r2', 'g2'\\): check-game's label check failed"):
            bt1.replays(self.spec(checks))
        with mock.patch.object(bt1, "VOID_RATIO", 20), \
                self.assertRaisesRegex(ValueError, "engineering: 1 of 3 games are void"):
            bt1.replays(self.spec(self.checks()[:2] + [checks[2]]))

    def test_partial_checks_read_only_their_own_games(self):
        """Stage 1's per-game check: the games of a partial checks file, with
        TA1's identity for a TA1 game, no completeness check and no void
        count."""
        checks = self.checks()
        partial = self.root / "partial.jsonl"
        partial.write_text(json.dumps(checks[0]) + "\n")
        result = bt1.replays(self.spec([]), partial)
        self.assertEqual((result["partial"], result["games"], result["ta1_identity"]),
                         (True, {"r1": 1, "r2": 0}, {"games": 1, "search_logs": 2}))
        partial.write_text(json.dumps(checks[2]) + "\n")
        with mock.patch.object(bt1, "VOID_RATIO", 20):
            self.assertEqual(len(bt1.replays(self.spec([]), partial)["void"]), 1)
        d = self.new["r1/g1"]
        value = json.loads((d / "g1-0.search.json").read_text())
        value["select_cap"] = 7
        write(d / "g1-0.search.json", value)
        partial.write_text(json.dumps(self.checks()[0]) + "\n")
        with self.assertRaisesRegex(ValueError, "a search log differs from TA1's"):
            bt1.replays(self.spec([]), partial)
        spec = write(self.root / "spec.json", self.spec([]))
        with mock.patch("builtins.print"), self.assertRaisesRegex(ValueError, "a search log differs from TA1's"):
            bt1.main(["replays", "--spec", spec["path"], "--partial", str(partial), "--out", str(self.root / "o")])
        with mock.patch("builtins.print"), self.assertRaisesRegex(ValueError, "kept games with no check"):
            bt1.main(["replays", "--spec", spec["path"], "--out", str(self.root / "o")])

    def test_any_difference_from_ta1_or_a_missing_game_stops_stage_1(self):
        checks = self.checks()
        with self.assertRaisesRegex(ValueError, "kept games with no check"):
            bt1.replays(self.spec(checks[:2]))
        with self.assertRaisesRegex(ValueError, "repeated, or not a kept bank game"):
            bt1.replays(self.spec(checks + checks[:1]))
        d = self.new["r1/g1"]
        value = json.loads((d / "g1-0.search.json").read_text())
        value["select_cap"] = 7
        write(d / "g1-0.search.json", value)
        with self.assertRaisesRegex(ValueError, "differ from the check's record"):
            bt1.replays(self.spec(checks))
        with self.assertRaisesRegex(ValueError, "a search log differs from TA1's"):
            bt1.replays(self.spec(self.checks()))
        other = self.root / "other"
        self.game(other, "r1", "g1", [lambda *a: line_tree(*a, labels=SELF_LABELS), flat_tree],
                  [(0, 5, "aa", LIVE), (1, 9, "ff", [1, 0])], extra={"note": "x"})
        self.new["r1/g1"] = other
        with self.assertRaisesRegex(ValueError, "beyond the live-index field"):
            bt1.replays(self.spec(self.checks()))
        # The same fields in another order differ too.
        reordered = self.world(tag="order")["r1/g1"]
        value = json.loads((reordered / "g1.actual.json").read_text())
        write(reordered / "g1.actual.json", {"lineage": value.pop("lineage"), **value})
        self.new["r1/g1"] = reordered
        with self.assertRaisesRegex(ValueError, "beyond the live-index field"):
            bt1.replays(self.spec(self.checks()))
        self.new["r1/g1"] = self.world(tag="again")["r1/g1"]
        self.assertEqual(bt1.replays(self.spec(self.checks()))["ta1_identity"]["games"], 1)
        (self.old["r1/g1"] / "g1.actual.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "TA1's retained logs differ"):
            bt1.replays(self.spec(self.checks()))


EXACT = [{"lineage": lin, "arm": "uniform", "pass": True} for lin in ("r1", "r2")]


class Commands(Temp):
    def census_record(self, stage=2, p_rows=None, stop=False):
        rows = [{"game": "g", "step": 1, "keep": [0, 1]}]
        keep = {cell: write(self.root / "census" / f"KEEP-{cell}-r1.json",
                            {"schema": bt1.KEEP_SETS_SCHEMA, "lineage": "r1", "bank_rows_sha256": "x",
                             "rows": p_rows if cell == "P" and p_rows is not None else rows})
                for cell in ("T", "P")}
        bank_pin = write(self.root / "bank.json", {"bank": 1})
        return {"schema": bt1.CENSUS_SCHEMA, "stage": stage, "banks": {"r1": bank_pin},
                "lineages": {"r1": {"keep_sets": keep}}, "stop_rule": {"stop": stop}}

    def source(self):
        return {"play_import": {"path": "p.json", "sha256": "1" * 64}, "feature_transfer": {},
                "checkpoint": {"path": "parent.json", "sha256": common.PARENT_CHECKPOINT_SHA256["r1"]}}

    def test_each_cell_gets_a_dry_run_full_run_and_rerun_with_its_keep_sets(self):
        record = self.census_record()
        games = self.root / "games.jsonl"
        games.write_text('{"id":"g"}\n')
        written = bt1.commands("r1", self.source(), games, self.root / "bank.json", {"training": []}, record, EXACT,
                               str(self.root / "phase1"))
        self.assertEqual(sorted(written), ["BT1-P", "BT1-P-dry", "BT1-P-rerun", "BT1-T", "BT1-T-dry",
                                           "BT1-T-rerun"])
        full, dry, rerun = written["BT1-T"], written["BT1-T-dry"], written["BT1-T-rerun"]
        self.assertEqual(full["options"], {"cell": "BT1-T", "keep_sets": record["lineages"]["r1"]["keep_sets"]["T"],
                                           "checkpoint_steps": STEPS})
        self.assertEqual(written["BT1-P"]["options"]["keep_sets"], record["lineages"]["r1"]["keep_sets"]["P"])
        self.assertTrue(dry["dry_run"] and "dry_run" not in full and "stop_after_steps" not in full)
        self.assertEqual(rerun["stop_after_steps"], 32)
        self.assertEqual({c["arm"] for c in written.values()}, {"uniform"})
        self.assertNotIn("update_scope", full)
        self.assertTrue(rerun["output_directory"].endswith("/phase1/r1-BT1-T-rerun"))
        with self.assertRaisesRegex(ValueError, "not the census's bank"):
            bt1.commands("r1", self.source(), games, games, {}, record, EXACT, "d")
        with self.assertRaisesRegex(ValueError, "not the stage 2 census"):
            bt1.commands("r1", self.source(), games, self.root / "bank.json", {}, self.census_record(0), EXACT, "d")
        with self.assertRaisesRegex(ValueError, "the stop rule binds on stage 2's census: BT1 stops"):
            bt1.commands("r1", self.source(), games, self.root / "bank.json", {}, self.census_record(stop=True),
                         EXACT, "d")
        for exactness in (EXACT[:1], [EXACT[0], {**EXACT[1], "pass": False}], [EXACT[0], EXACT[0]],
                          [EXACT[0], {**EXACT[1], "arm": "line"}], [EXACT[0], {"arm": "uniform", "pass": True}]):
            with self.assertRaisesRegex(ValueError, "phase-1 exactness has not passed in both lineages"):
                bt1.commands("r1", self.source(), games, self.root / "bank.json", {}, record, exactness, "d")
        shifted = self.census_record(p_rows=[{"game": "g", "step": 2, "keep": [0]}])
        with self.assertRaisesRegex(ValueError, "T and P list different rows"):
            bt1.commands("r1", self.source(), games, self.root / "bank.json", {}, shifted, EXACT, "d")
        record = self.census_record()
        (self.root / "census" / "KEEP-P-r1.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "keep sets: .* is missing or differs"):
            bt1.commands("r1", self.source(), games, self.root / "bank.json", {}, record, EXACT, "d")

    def test_the_cli_writes_the_commands_and_phase1_takes_keep_sets_alone(self):
        record = write(self.root / "CENSUS.json", self.census_record())
        games, parent, anchor = self.root / "games.jsonl", self.root / "parent.json", self.root / "anchor.json"
        games.write_text('{"id":"g"}\n')
        write(parent, self.source())
        write(anchor, {"training": []})
        exact = [write(self.root / f"exact-{e['lineage']}.json", e)["path"] for e in EXACT]
        with mock.patch("builtins.print"):
            bt1.main(["commands", "--lineage", "r1", "--parent", str(parent), "--games", str(games), "--bank",
                      str(self.root / "bank.json"), "--anchor", str(anchor), "--census", record["path"],
                      "--exactness", *exact, "--root", str(self.root / "phase1"), "--out-dir", str(self.root / "cmds")])
        command = json.loads((self.root / "cmds" / "r1-BT1-P-dry.command.json").read_text())
        self.assertEqual(command["options"]["cell"], "BT1-P")
        keep = self.root / "census" / "KEEP-T-r1.json"
        options = phase1.cell_options("BT1-T", checkpoint_steps=STEPS, keep_sets=keep)
        self.assertEqual(options, {"cell": "BT1-T", "checkpoint_steps": STEPS, "keep_sets": common.pin(keep.resolve())})
        out = self.root / "cmd.json"
        args = ["phase1", "--lineage", "r1", "--arm", "U", "--parent", str(parent), "--games", str(games),
                "--bank", str(self.root / "bank.json"), "--anchor", str(anchor), "--output-directory", "d",
                "--out", str(out), "--cell", "BT1-T", "--keep-sets", str(keep)]
        with mock.patch("builtins.print"):
            phase1.main(args)
        self.assertEqual(json.loads(out.read_text())["options"]["keep_sets"]["sha256"],
                         common.pin(keep)["sha256"])
        with mock.patch("sys.stderr"), self.assertRaises(SystemExit):
            phase1.main(args + ["--min-edge-backups", "32"])


# ------------------------------------------------------------------- fit

KEEP = {"T": [("g1", 5, [1, 2, 3], 0.74), ("g1", 9, [0, 1], None), ("g2", 3, [0, 2], 0.3)],
        "P": [("g1", 5, [2], 1.0), ("g1", 9, [0], None), ("g2", 3, [0], 0.0)]}
# The census's anti-line rows: there the targets keep the opponent target.
ANTI = [("g2", 3)]


def readout(cid, state, self_p=0.1, anti_p=0.1, gap=4.0, agree=0.95, kl=0.01, line_rows=(("g1", 5), ("g2", 3))):
    """A probe readout. The deployed self-target probability is ``anti_p`` on
    the anti-line rows and ``self_p`` on the other line rows."""
    rows = [{"game": g, "step": s, "chosen": True, "self_offered": True, "giant_offered": False,
             "deployed_probability": p, "raw_probability": p, "raw_gap": 0.0,
             "self_deployed_probability": p, "giant_deployed_probability": None}
            for g, s, p in ((g, s, anti_p if (g, s) in ANTI else self_p) for g, s in line_rows)]
    return {"id": cid, "state_sha256": state,
            "archive": {"training": {"groups": {"all": {"rows": 6000, "mean_deployed_probability": 0.4}},
                                     "discrimination": {"self": 0.1, "giant": None}, "line_rows": rows}},
            "anchor_held_out": {"median_clamped_top_two_gap": gap, "non_spy_top_agreement": agree,
                                "mean_kl_from_parent_non_spy": kl},
            "learner_offers": {"offers": 150, "mean_deployed_self_probability": 0.02,
                               "mean_raw_self_probability": 0.02, "self_top_share": 0.0}}


def census_targets(keep, anti, self_keys=None):
    """A cell's census line rows: its self line rows from the targets' self
    mass (or ``self_keys``), and the given anti-line rows."""
    hits = [(g, s) for g, s, _, mass in keep if mass is not None and mass >= 0.5]
    return {"line_rows": {"self": {"rows": len(hits), "mean_target_mass": None,
                                   "keys": [list(k) for k in (hits if self_keys is None else self_keys)]},
                          "anti": {"rows": len(anti), "mean_target_mass": 0.8, "keys": [list(k) for k in anti],
                                   "on_raw_counts": 0},
                          "giant": {"rows": 0, "mean_target_mass": None, "keys": []}}}


class Fit(Temp):
    def setUp(self):
        super().setUp()
        telemetry = self.root / "telemetry.json"
        telemetry.write_text(json.dumps([{"anchor_loss": 0.1, "archive_loss": 1.0,
                                          "anchor_components": {"all": {"auxiliary_l2": 1.0}},
                                          "archive_components": {"all": {"auxiliary_l2": 1.0}}}] * 8192))
        self.telemetry = {"path": str(telemetry), "sha256": "0" * 64}
        self.lines = (("g1", 5), ("g2", 3))

    def keep_file(self, lineage, cell, keep):
        return write(self.root / "census" / f"KEEP-{cell}-{lineage}.json",
                     {"schema": bt1.KEEP_SETS_SCHEMA, "lineage": lineage, "bank_rows_sha256": "x",
                      "rows": [{"game": g, "step": s, "keep": k} for g, s, k, _ in keep]})

    def cell_run(self, lineage, cell, keep_pin, keep, self_p=None, anti_p=None, collapse_from=None, rerun_hash="h32",
                 options=None, dry_keep=None, arm_rows=None, exposure=None, full_exposure=None):
        label = bt1.CELL_LABELS[cell]
        options = options or {"cell": label, "keep_sets": keep_pin, "checkpoint_steps": STEPS}
        identity = {"lineage": lineage, "arm": "uniform", "recipe": {"checkpoints": STEPS, "options": options}}
        keep_rows = [{"game": g, "step": s, "k": 4, "keep": k, "kept_parent_probability": 0.5,
                      "target_self_mass": mass, "target_giant_mass": None, "kl_target_from_parent": 0.7}
                     for g, s, k, mass in (dry_keep or keep)]
        exposure = exposure or [{"step": 8, "draws_per_row_min": 1}]
        dry = {"schema": bt1.screen.PHASE1_SCHEMA, "dry_run": True, "identity": identity,
               "census": {"arm": {"rows": len(keep)}, "keep_rows": keep_rows, "exposure": exposure}}
        full = {"schema": bt1.screen.PHASE1_SCHEMA, "complete": True, "stop_after_steps": None,
                "identity": identity, "telemetry": self.telemetry,
                "census": {"arm": {"rows": arm_rows or len(keep)}, "keep_rows": keep_rows,
                           "exposure": full_exposure or exposure},
                "checkpoints": [{"step": s, "checkpoint": {"path": f"{label}-{s}.json", "sha256": f"{s:064d}"},
                                 "source": {"path": f"{label}-{s}-source.json", "sha256": "0" * 64},
                                 "state_sha256": f"{lineage}-{cell}-{s}"} for s in STEPS],
                "state_hashes": [{"step": 32, "phase1_state_sha256": "h32"}]}
        rerun = {"schema": bt1.screen.PHASE1_SCHEMA, "complete": True, "stop_after_steps": 32, "identity": identity,
                 "state_hashes": [{"step": 32, "phase1_state_sha256": rerun_hash}]}
        paths = {part: write(self.root / f"{lineage}-{cell}-{part}.json", value)["path"]
                 for part, value in (("dry", dry), ("full", full), ("rerun", rerun))}
        candidates = [readout(f"{label}-step-{s}", f"{lineage}-{cell}-{s}", self_p=(self_p or {}).get(s, 0.1),
                              anti_p=(anti_p or {}).get(s, 0.1), line_rows=self.lines,
                              gap=1.0 if collapse_from and s >= collapse_from else 4.0) for s in STEPS]
        return paths, candidates

    def spec(self, runs, keep=None, screen_floors=None, stage=2, missing=None, census_self=None, stop=False):
        """runs[lineage][cell] = keyword arguments of ``cell_run``."""
        keep = keep or KEEP
        spec, screen_fit = {}, {"fits": {}}
        census = {"schema": bt1.CENSUS_SCHEMA, "stage": stage, "lineages": {}, "stop_rule": {"stop": stop}}
        for lineage, by_cell in runs.items():
            pins = {cell: self.keep_file(lineage, cell, keep[cell]) for cell in bt1.CELLS}
            census["lineages"][lineage] = {
                "keep_sets": pins,
                "targets": {"cells": {cell: census_targets(keep[cell], ANTI, (census_self or {}).get(cell))
                                      for cell in bt1.CELLS}}}
            parent, o_final = readout("parent", "p", gap=4.0), readout("O-final", "o", agree=0.95, kl=0.02)
            candidates, spec[lineage] = [parent, o_final], {"cells": {}}
            for cell, kwargs in by_cell.items():
                paths, found = self.cell_run(lineage, cell, pins[cell], keep[cell], **kwargs)
                spec[lineage]["cells"][cell] = paths
                candidates += [c for c in found if c["id"] != missing]
            probe = write(self.root / f"{lineage}-probe.json", {"schema": bt1.screen.PROBE_SCHEMA, "complete": True,
                                                                "lineage": lineage, "candidates": candidates})
            spec[lineage]["probes"] = [probe["path"]]
            floors = screen_floors or bt1.screen.g2(parent, parent, o_final)
            screen_fit["fits"][lineage] = {
                "U": {"read": [{"G2": {k: floors[k] for k in ("gap_floor", "agreement_floor", "kl_ceiling")}}]},
                "L": {"read": [{"training_rows": {"rows": 50}}],
                      "selected": {"learner_offers": {"mean_deployed_self_probability": 0.03}}}}
        spec["screen_fit"] = write(self.root / "screen-fit.json", screen_fit)["path"]
        spec["census"] = write(self.root / "census.json", census)["path"]
        return spec

    def test_a_cell_selects_its_last_passing_checkpoint_and_t_is_read_against_p(self):
        fitted = {s: 0.6 for s in STEPS if s >= 64}
        result = bt1.fit(self.spec({"r1": {"T": {"self_p": fitted}, "P": {"collapse_from": 128}}}))
        t, p = result["fits"]["r1"]["T"], result["fits"]["r1"]["P"]
        self.assertEqual((t["selected"]["step"], t["fit_dose"], t["stopped_at"]), (8192, 64, None))
        self.assertEqual(t["selected"]["checkpoint"]["sha256"], f"{8192:064d}")
        self.assertEqual((t["reading"], t["explanations"]),
                         ("Passes: selected step 8192, fit dose 64, on 1 self line row", [bt1.FEW_ROWS]))
        self.assertEqual(t["line_rows"], {"self": 1, "giant": 0, "anti": 1})
        self.assertEqual(t["line_row_masses"], {"self": 0.74, "giant": None, "anti": 0.8})
        self.assertEqual(t["read"][1]["G1'"], {
            "self": {"rows": 1, "mean_target_mass": 0.74, "mean_deployed_probability": 0.6, "pass": True},
            "giant": {"rows": 0, "mean_target_mass": None, "mean_deployed_probability": None, "pass": None},
            "halves": ["self"], "pass": True})
        self.assertAlmostEqual(t["read"][1]["target_discrimination"], 0.5)
        self.assertAlmostEqual(t["read"][0]["target_discrimination"], 0.0)
        self.assertEqual([d["step"] for d in t["descriptive"]], [8, 16])
        self.assertNotIn("G3", t["descriptive"][0])
        self.assertNotIn("g2_beside", t)
        self.assertEqual((p["stopped_at"], p["unread_checkpoints"]), (128, STEPS[5:]))
        self.assertEqual((p["reading"], p["explanations"]),
                         (bt1.COLLAPSE_FIRST["P"][0] + ", on 1 self line row", bt1.COLLAPSE_FIRST["P"][1]))
        self.assertEqual(result["passes"], {"T": ["r1"], "P": []})
        self.assertEqual(result["fit_check"], {"T": "lineage-dependent fit", "P": "passes in neither lineage"})
        order = result["t_against_p"]
        self.assertEqual(order["per_lineage"]["r1"]["ordering"], bt1.cells.LATER)
        self.assertEqual(order["per_lineage"]["r1"]["wording"], bt1.ORDER_WORDING[bt1.cells.LATER])
        self.assertEqual(order["per_lineage"]["r1"]["p_collapse_step"], 128)
        self.assertEqual(order["across_lineages"], "one lineage only")
        beside = result["beside"]["r1"]
        self.assertEqual(beside["L_line_rows"], 50)
        self.assertEqual(beside["cells"]["T"]["learner_offer_probability"], 0.02)
        self.assertIsNone(beside["cells"]["P"]["learner_offer_probability"])
        self.assertEqual([c["step"] for c in beside["cross_cell"]], [32, 64, 128])
        self.assertEqual(beside["cross_cell"][1], {
            "step": 64, "T_on_P_line_rows": {"self": 0.6, "giant": None},
            "P_on_T_line_rows": {"self": 0.1, "giant": None}, "shared_self_line_rows": 1,
            "on_shared_self_line_rows": {"T": 0.6, "P": 0.1}})

    def test_both_lineages_passing_reads_passes_in_both(self):
        fitted = {s: 0.6 for s in STEPS if s >= 32}
        runs = {lin: {"T": {"self_p": fitted}, "P": {"self_p": fitted, "collapse_from": 256}} for lin in ("r1", "r2")}
        result = bt1.fit(self.spec(runs))
        self.assertEqual(result["fit_check"], {"T": "passes in both lineages", "P": "passes in both lineages"})
        self.assertEqual(result["fits"]["r2"]["P"]["selected"]["step"], 128)
        self.assertEqual(result["t_against_p"]["across_lineages"], bt1.cells.LATER)

    def test_three_self_line_rows_drop_the_few_row_wording(self):
        self.lines = (("g1", 5), ("g1", 9), ("g1", 11), ("g2", 3))
        many = {"T": [("g1", 5, [1, 2, 3], 0.74), ("g1", 9, [0, 1], 0.6), ("g1", 11, [0], 1.0), ("g2", 3, [0, 2], 0.3)],
                "P": [("g1", 5, [2], 1.0), ("g1", 9, [0], 1.0), ("g1", 11, [0], 1.0), ("g2", 3, [0], 0.0)]}
        fitted = {s: 0.6 for s in STEPS if s >= 32}
        result = bt1.fit(self.spec({"r1": {"T": {"self_p": fitted}}}, keep=many))
        t = result["fits"]["r1"]["T"]
        self.assertEqual((t["reading"], t["explanations"]), ("Passes: selected step 8192, fit dose 32", []))
        self.assertEqual(t["line_rows"], {"self": 3, "giant": 0, "anti": 1})
        self.assertAlmostEqual(t["read"][0]["target_discrimination"], 0.5)

    def test_readings_without_a_selection_follow_section_7_2(self):
        early = {s: 0.6 for s in STEPS if s >= 64}
        result = bt1.fit(self.spec({"r1": {"T": {"self_p": early, "collapse_from": 64}, "P": {}}}))
        self.assertEqual((result["fits"]["r1"]["T"]["reading"], result["fits"]["r1"]["T"]["explanations"]),
                         (bt1.FIT_WITH_COLLAPSE[0] + ", on 1 self line row", bt1.FIT_WITH_COLLAPSE[1]))
        self.assertEqual((result["fits"]["r1"]["P"]["reading"], result["fits"]["r1"]["P"]["explanations"]),
                         (bt1.NOT_FIT["P"][0] + ", on 1 self line row", bt1.NOT_FIT["P"][1]))
        self.assertEqual(result["t_against_p"]["per_lineage"]["r1"]["ordering"], bt1.cells.EARLIER)
        none = {cell: [(g, s, k, None if mass is None else 0.4) for g, s, k, mass in KEEP[cell]] for cell in KEEP}
        result = bt1.fit(self.spec({"r1": {"T": {"self_p": early}, "P": {}}}, keep=none))
        t = result["fits"]["r1"]["T"]
        self.assertEqual((t["reading"], t["explanations"]), bt1.NO_SELF_LINE_ROW)
        self.assertIsNone(t["selected"])
        self.assertEqual([r["step"] for r in t["g2_beside"]], STEPS[2:])
        self.assertIsNone(t["read"][0]["target_discrimination"])
        result = bt1.fit(self.spec({"r1": {"T": {"rerun_hash": "other"}, "P": {"rerun_hash": "other"}}}))
        self.assertEqual((result["fits"]["r1"]["T"]["reading"], result["fits"]["r1"]["T"]["stopped_at"]),
                         (bt1.screen.ENGINEERING, 32))
        self.assertEqual(result["t_against_p"]["per_lineage"]["r1"]["ordering"], bt1.cells.NOT_EVIDENCE)
        result = bt1.fit(self.spec({"r1": {"T": {"collapse_from": 32}, "P": {"collapse_from": 32}}}))
        self.assertEqual(result["t_against_p"]["per_lineage"]["r1"]["ordering"], bt1.cells.SAME)
        self.assertEqual(result["t_against_p"]["per_lineage"]["r1"]["wording"], bt1.ORDER_WORDING[bt1.cells.SAME])
        self.assertEqual((result["fits"]["r1"]["T"]["reading"], result["fits"]["r1"]["T"]["explanations"]),
                         (bt1.COLLAPSE_FIRST["T"][0] + ", on 1 self line row", bt1.COLLAPSE_FIRST["T"][1]))
        result = bt1.fit(self.spec({"r1": {"T": {}}}))
        self.assertEqual((result["fits"]["r1"]["T"]["reading"], result["fits"]["r1"]["T"]["explanations"]),
                         (bt1.NOT_FIT["T"][0] + ", on 1 self line row", bt1.NOT_FIT["T"][1]))
        # Engineering comes first, also for a cell with no self line row.
        result = bt1.fit(self.spec({"r1": {"T": {"rerun_hash": "other"}}}, keep=none))
        self.assertEqual((result["fits"]["r1"]["T"]["reading"], result["fits"]["r1"]["T"]["explanations"]),
                         (bt1.screen.ENGINEERING, []))
        # A pattern no row of 7.2 describes reads as no match.
        read = [{"G1'": {"pass": True}, "G2": {"pass": True}, "G3": {"pass": True}}]
        self.assertEqual(bt1.fit_reading("T", {"self": [("g1", 5)] * 3}, read, [{"step": 32}], None, None),
                         (bt1.screen.NO_MATCH, []))

    def test_every_run_must_be_its_cell_on_its_keep_sets(self):
        cases = [
            ({"T": {"options": {"cell": "BT1-T", "keep_sets": {"sha256": "0" * 64}, "checkpoint_steps": STEPS}}},
             "options differ from the design's"),
            ({"T": {"dry_keep": [("g1", 5, [1, 2], 0.7)] + KEEP["T"][1:]}}, "keep rows differ from the keep-set file"),
            ({"T": {"arm_rows": 7}}, "trains other rows"),
            ({"T": {"full_exposure": [{"step": 9}]}}, "full run's exposure differs from the dry run's"),
            ({"T": {"exposure": [{"step": 9}], "full_exposure": [{"step": 9}]}}, "T and P differ in rows or exposure"),
        ]
        for cells_kwargs, message in cases:
            with self.assertRaisesRegex(ValueError, message):
                bt1.fit(self.spec({"r1": {"P": {}, **cells_kwargs}}))
        with self.assertRaisesRegex(ValueError, "did not read checkpoint step 64"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, missing="BT1-T-step-64"))
        floors = {"gap_floor": 2.5, "agreement_floor": 0.93, "kl_ceiling": 0.02}
        with self.assertRaisesRegex(ValueError, "G2 references differ"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, screen_floors=floors))
        with self.assertRaisesRegex(ValueError, "not the stage 2 census"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, stage=0))
        shifted = {**KEEP, "P": [("g1", 5, [2], 1.0), ("g1", 9, [0], None), ("g2", 4, [0], 0.0)]}
        with self.assertRaisesRegex(ValueError, "T and P list different rows"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, keep=shifted))
        with self.assertRaisesRegex(ValueError, "census's self line rows differ from the engine's"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, census_self={"T": [("g2", 3)]}))
        lost = {**KEEP, "T": [("g1", 7, [1, 2, 3], 0.74)] + KEEP["T"][1:]}
        with self.assertRaisesRegex(ValueError, "not among the probe's line rows"):
            bt1.fit(self.spec({"r1": {"T": {}}}, keep=lost))
        with self.assertRaisesRegex(ValueError, "the stop rule binds on stage 2's census: BT1 stops"):
            bt1.fit(self.spec({"r1": {"T": {}, "P": {}}}, stop=True))

    def test_the_dry_check_reads_both_cells_dry_runs_against_the_census(self):
        spec = self.spec({"r1": {"T": {}, "P": {}}})
        dry = {cell: run["dry"] for cell, run in spec["r1"]["cells"].items()}
        out = self.root / "dry-check.json"
        with mock.patch("builtins.print") as printed:
            self.assertEqual(bt1.main(["dry", "--census", spec["census"], "--lineage", "r1", "--dry-t", dry["T"],
                                       "--dry-p", dry["P"], "--out", str(out)]), 0)
        result = json.loads(out.read_text())
        self.assertEqual((result["schema"], result["lineage"], result["pass"]), (bt1.DRY_SCHEMA, "r1", True))
        self.assertEqual(result["census"]["sha256"], hashlib.sha256(Path(spec["census"]).read_bytes()).hexdigest())
        self.assertEqual({cell: (v["rows"], v["line_rows"]) for cell, v in result["cells"].items()},
                         {"T": (3, {"self": 1, "giant": 0, "anti": 1}), "P": (3, {"self": 1, "giant": 0, "anti": 1})})
        self.assertEqual(result["cells"]["T"]["dry_run"]["sha256"], hashlib.sha256(Path(dry["T"]).read_bytes())
                         .hexdigest())
        self.assertEqual(json.loads(printed.call_args[0][0])["cells"]["P"]["rows"], 3)
        cases = [
            ({"T": {"dry_keep": [("g1", 5, [1, 2], 0.7)] + KEEP["T"][1:]}}, {},
             "keep rows differ from the keep-set file"),
            ({"T": {"options": {"cell": "BT1-P", "keep_sets": {"sha256": "0" * 64}, "checkpoint_steps": STEPS}}}, {},
             "options differ from the design's"),
            ({"T": {"exposure": [{"step": 9}]}}, {}, "T and P differ in rows or exposure"),
            ({}, {"census_self": {"P": [("g2", 3)]}}, "r1-P: the census's self line rows differ from the engine's"),
            ({}, {"stop": True}, "the stop rule binds"),
        ]
        for runs, kwargs, message in cases:
            spec = self.spec({"r1": {"T": {}, "P": {}, **runs}}, **kwargs)
            with self.assertRaisesRegex(ValueError, message):
                bt1.dry_check(Path(spec["census"]), "r1",
                              {cell: Path(run["dry"]) for cell, run in spec["r1"]["cells"].items()})


# ---------------------------------------------------------------- panels


class Panels(panels_fixture.Fixture):
    def setUp(self):
        super().setUp()
        patcher = mock.patch.object(bt1.analyze, "DRAWS", panels_fixture.DRAWS)
        patcher.start()
        self.addCleanup(patcher.stop)
        self.sets = 0

    def panel(self, ck, opp, combo, sha="a" * 64, where="set"):
        return panels_fixture.panel(self.root / f"{where}{self.sets}", ck, opp,
                                    panels_fixture.constant(lambda o, t, k, s: 1, combo), sha)

    def identity(self, base, screen, holds=True):
        """BT1's identity records: T1 panels byte for byte, H labels byte for
        byte and H rows equal once the recorded envelope hash is restored
        (or not, when the identity fails)."""
        recorded = {(p["checkpoint"], p["opponent"]): p for p in screen["panels"]}
        pairs, h_pairs = {}, {}
        for ck in bt1.IDENTITY_PANELS:
            for opp in ("T1", "H"):
                rows, labels = (bt1.sha256_file(recorded[(ck, opp)][f]) for f in ("rows", "labels"))
                pairs[f"{ck}-vs-{opp}"] = {"rows.jsonl": {"new": rows if opp == "T1" else "e" * 64, "recorded": rows},
                                           "labels.jsonl": {"new": labels, "recorded": labels}}
                if opp == "H":
                    h_pairs[f"{ck}-vs-H"] = {"rows_bytes_equal": holds, "new_rows_sha256": "e" * 64,
                                             "restored_rows_sha256": rows if holds else "d" * 64,
                                             "recorded_rows_sha256": rows, "failure_count": 0 if holds else 2}
        return {"panel_identity": write(base / "PANEL-IDENTITY.json", {"pairs": pairs}),
                "h_identity": write(base / "H-IDENTITY-CHECK.json", {"pairs": h_pairs, "passed": holds})}

    def setup_panels(self, cells=("T",), lineages=("r1", "r2"), holds=True):
        self.sets += 1
        base = self.root / f"files{self.sets}"
        never = lambda o, t, k, s: False  # noqa: E731
        half = lambda o, t, k, s: o == panels_fixture.SPY and k < 6  # noqa: E731
        every = lambda o, t, k, s: o == panels_fixture.SPY  # noqa: E731
        screen = {"panels": [self.panel(f"{lin}-{role}", opp, half if role == "D1L" else never)
                             for lin in ("r1", "r2") for role in ("parent", "D1L", "D1U") for opp in ("T1", "H")]}
        index = write(base / "screen-index.json", screen)
        result = write(base / "screen-result.json", bt1.analyze.analyze(screen, None))
        x4 = write(base / "X4.json", bt1.screen_x4(Path(index["path"]), Path(result["path"])))
        fits, entries = {}, {}
        for lin in ("r1", "r2"):
            fits[lin] = {}
            for cell in ("T", "P"):
                sha = ({"r1": "1", "r2": "2"}[lin] + {"T": "a", "P": "b"}[cell]) * 32
                passed = cell in cells and lin in lineages
                fits[lin][cell] = {"selected": {"checkpoint": {"sha256": sha}} if passed else None,
                                   "line_rows": {"self": 2, "giant": 0, "anti": 1},
                                   "line_row_masses": {"self": 0.9, "giant": None, "anti": 0.8}}
                if passed:
                    entries.setdefault(lin, []).extend(self.panel(f"{lin}-D1{cell}", opp, every, sha)
                                                       for opp in ("T1", "H"))
        fit_check = {cell: ("passes in both lineages" if cell in cells and len(lineages) == 2 else
                            "lineage-dependent fit" if cell in cells else "passes in neither lineage")
                     for cell in ("T", "P")}
        passes = {cell: [lin for lin in ("r1", "r2") if cell in cells and lin in lineages] for cell in ("T", "P")}
        spec = {"screen_index": index, "screen_result": result, "x4": x4,
                "identity": self.identity(base, screen, holds), "entries": entries}
        if not holds:
            spec["controls"] = {lin: [self.panel(f"{lin}-{role}", opp, half if role == "D1L" else never,
                                                 where="rerun") for role in bt1.CONTROLS for opp in ("T1", "H")]
                                for lin in ("r1", "r2")}
        return spec, {"fits": fits, "fit_check": fit_check, "passes": passes}

    def test_d1_of_each_passing_cell_is_read_against_the_parent(self):
        spec, fitted = self.setup_panels()
        out = bt1.panels(spec, fitted)
        self.assertEqual((out["seed"], out["draws"]), (bt1.analyze.BOOT_SEED, panels_fixture.DRAWS))
        self.assertEqual(out["identity"], {"pairs": {f"{ck}-vs-{opp}": True for ck in bt1.IDENTITY_PANELS
                                                     for opp in ("T1", "H")}, "holds": True})
        self.assertEqual(out["x4"]["with_panels"], {"compared": 84, "differing": [], "pass": True})
        self.assertTrue(out["x4"]["before"]["pass"])
        self.assertEqual(sorted(out["panels"]), sorted(f"{lin}-{role}/{opp}" for lin in ("r1", "r2")
                                                       for role in ("parent", "D1L", "D1T") for opp in ("T1", "H")))
        t = out["cells"]["T"]
        self.assertEqual((t["line_reading"], t["screen_at_D1"]), ("behavioral transfer established at D1", True))
        self.assertEqual(t["per_lineage"]["r1"]["J_transfer_at_D1"], True)
        self.assertEqual(t["per_lineage"]["r2"]["guards"], "guards hold")
        self.assertEqual(t["per_lineage"]["r1"]["line_rows"], {"self": 2, "giant": 0, "anti": 1})
        self.assertEqual(t["per_lineage"]["r1"]["note"], bt1.NO_GIANT_LINE_ROW)
        self.assertEqual(out["cells"]["P"]["line_reading"], "not run: the cell passed the fit check in neither lineage")
        self.assertIsNone(out["cells"]["P"]["screen_at_D1"])
        self.assertEqual(out["win_rates"]["D1T-parent/spy_score"]["reading"], "no change established")
        self.assertNotIn("D1P-parent/spy_score", out["win_rates"])
        self.assertAlmostEqual(out["descriptive"]["r1/D1T-D1L/J"]["worst"]["point"], 0.5)
        self.assertNotIn("r1/D1T-D1P/J", out["descriptive"])
        self.assertEqual(out["note"], bt1.NOT_STRENGTH)

    def test_without_identity_every_control_panel_is_rerun(self):
        spec, fitted = self.setup_panels(holds=False)
        out = bt1.panels(spec, fitted)
        self.assertFalse(out["identity"]["holds"])
        self.assertEqual(out["identity"]["pairs"], {"r1-parent-vs-T1": True, "r1-parent-vs-H": False,
                                                    "r2-D1L-vs-T1": True, "r2-D1L-vs-H": False})
        self.assertIsNone(out["x4"]["with_panels"])
        self.assertEqual(out["cells"]["T"]["line_reading"], "behavioral transfer established at D1")
        spec["controls"]["r2"] = spec["controls"]["r2"][:3]
        with self.assertRaisesRegex(ValueError, "every parent and D1L panel must be rerun"):
            bt1.panels(spec, fitted)

    def test_x4_must_pass_on_the_screens_files_and_identity_must_compare_its_panels(self):
        spec, fitted = self.setup_panels()
        base = self.root / "x4"
        result = json.loads(Path(spec["screen_result"]["path"]).read_text())
        result["contrasts"]["r1/D1L-parent/J/pooled/worst"]["point"] += 0.01
        altered = write(base / "altered-result.json", result)
        failed = bt1.screen_x4(Path(spec["screen_index"]["path"]), Path(altered["path"]))
        self.assertEqual((failed["compared"], failed["differing"], failed["pass"]),
                         (84, ["r1/D1L-parent/J/pooled/worst"], False))
        with mock.patch("builtins.print"):
            self.assertEqual(bt1.main(["x4", "--screen-index", spec["screen_index"]["path"], "--screen-result",
                                       altered["path"], "--out", str(base / "X4-failed.json")]), 1)
            self.assertEqual(bt1.main(["x4", "--screen-index", spec["screen_index"]["path"], "--screen-result",
                                       spec["screen_result"]["path"], "--out", str(base / "X4-passed.json")]), 0)
        for x4 in (write(base / "failed.json", failed), write(base / "other.json", {**json.loads(
                Path(spec["x4"]["path"]).read_text()), "screen_result": altered})):
            with self.assertRaisesRegex(ValueError, "X4 did not pass on these screen files"):
                bt1.panels({**spec, "x4": x4}, fitted)
        with self.assertRaisesRegex(ValueError, "another seed or draw count"):
            bt1.screen_x4(Path(spec["screen_index"]["path"]),
                          Path(write(base / "seed.json", {**result, "seed": 1})["path"]))
        record = json.loads(Path(spec["identity"]["panel_identity"]["path"]).read_text())
        record["pairs"]["r2-D1L-vs-T1"]["rows.jsonl"]["recorded"] = "c" * 64
        spec["identity"]["panel_identity"] = write(base / "identity.json", record)
        with self.assertRaisesRegex(ValueError, "r2-D1L-vs-T1: the identity record does not compare"):
            bt1.panels(spec, fitted)

    def test_the_h_check_must_have_read_the_identity_panels_new_rows(self):
        spec, fitted = self.setup_panels()
        check = json.loads(Path(spec["identity"]["h_identity"]["path"]).read_text())
        check["pairs"]["r2-D1L-vs-H"]["new_rows_sha256"] = "f" * 64
        spec["identity"]["h_identity"] = write(self.root / "h-check.json", check)
        with self.assertRaisesRegex(ValueError, "r2-D1L-vs-H: the H identity check did not read the identity panel"):
            bt1.panels(spec, fitted)

    def test_every_passing_cell_needs_both_panels_in_every_lineage_it_passed(self):
        spec, fitted = self.setup_panels()
        spec["entries"]["r2"] = [e for e in spec["entries"]["r2"] if e["opponent"] != "H"]
        with self.assertRaisesRegex(ValueError, "every selected checkpoint's panel against each opponent, once each"):
            bt1.panels(spec, fitted)
        spec, fitted = self.setup_panels()
        spec["entries"]["r1"].append(spec["entries"]["r1"][0])
        with self.assertRaisesRegex(ValueError, "once each"):
            bt1.panels(spec, fitted)
        spec, fitted = self.setup_panels(lineages=("r1",))
        fitted["passes"]["T"] = ["r1", "r2"]
        fitted["fits"]["r2"]["T"]["selected"] = {"checkpoint": {"sha256": "2a" * 32}}
        with self.assertRaisesRegex(ValueError, "once each"):
            bt1.panels(spec, fitted)
        # The analysis must hold a contrast for every lineage where the cell passed.
        spec, fitted = self.setup_panels()
        with mock.patch.object(bt1, "panel_index", lambda spec, fitted, index, holds: {
                "panels": bt1.controls_of(index) + spec["entries"]["r1"]}):
            with self.assertRaisesRegex(ValueError, "D1T-parent: the analysis lacks a lineage where the cell passed"):
                bt1.panels(spec, fitted)

    def test_a_cell_passing_in_one_lineage_is_descriptive_and_panels_must_be_the_selected_checkpoint(self):
        spec, fitted = self.setup_panels(lineages=("r1",))
        out = bt1.panels(spec, fitted)
        self.assertEqual(out["cells"]["T"]["lineages"], ["r1"])
        self.assertEqual(out["cells"]["T"]["line_reading"],
                         "descriptive only: the cell passed the fit check in one lineage")
        spec, fitted = self.setup_panels()
        spec["entries"]["r1"][0]["checkpoint_sha256"] = "f" * 64
        with self.assertRaisesRegex(ValueError, "not the cell's selected checkpoint"):
            bt1.panels(spec, fitted)
        spec, fitted = self.setup_panels()
        spec["entries"]["r1"].append(self.panel("r1-D1P", "T1", lambda *_: False, "e" * 64))
        with self.assertRaisesRegex(ValueError, "not the cell's selected checkpoint"):
            bt1.panels(spec, fitted)
        spec, fitted = self.setup_panels()
        spec["entries"]["r2"] = spec["entries"].pop("r1")
        with self.assertRaisesRegex(ValueError, "not a BT1 endpoint"):
            bt1.panels(spec, fitted)


if __name__ == "__main__":
    unittest.main()
