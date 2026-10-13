#!/usr/bin/env python3
"""Offline best-line rescoring of logged E searches (Step 0 of the
search-scoring proposal; collab LANES/bestline-step0-20261013).

Input: a directory of `S4A_PATH_LOG` files (`*.search.json` and, for guides,
`*.actual.json`) written by `mtg-kernel/src/regret_census_v1/stage4a/pathlog.rs`.
Nothing is searched or played; every number comes from the logged attempts.

Rules compared at every node:

- current: an edge's value is its mean result; the pick is the greatest mean
  among edges with at least 8 backups, ties to the node's seeded order
  (`tree.rs` `frozen_choice`).
- best line (threshold m): an edge's value averages what followed it, the
  value of each next tree node it led to and each result that no policy choice
  of ours touched after the tree path. Results the rollout policy played after
  the tree path count only while the edge has fewer than m such outcomes. A
  node's value is the best value among its edges with at least m tries, or its
  mean result when none has m. The pick takes the same floor of 8 backups as
  the current rule (`floor 8`), and, as a secondary reading, a floor of m.

Stop rules are evaluated on the root after every attempt (the bounds rule,
whose bounds are costlier, at every attempt for small searches and at
`--checkpoints` evenly spaced attempts otherwise):

- strict: every root edge has been tried and the leader's worst case (all
  remaining attempts go to it and lose) beats every rival's best case (all
  remaining attempts go to that rival and win), on the best-line value and
  try counts;
- bounds: the 95% Clopper-Pearson lower bound of the leader's line beats the
  upper bound of every rival's line (an untried rival's bound is 1);
- proven-k: the leader's value is 1.0 and its line has at least k wins with
  no rollout-policy choice after the tree path.

A line count takes the attempts through a root edge whose later tree choices
all equal the best-line pick (floor m) of their node at that checkpoint.
Remaining attempts are the remaining transitions over the mean transitions per
natural attempt so far.

Values and line counts are kept incrementally (an attempt changes only the
nodes on its path), so a search costs time linear in its attempts.
"""

import argparse
import functools
import glob
import gzip
import json
import math
import multiprocessing
import os
import random
import sys
from collections import defaultdict

CURRENT_FLOOR = 8
EPS = 1e-12


def _betacf(a, b, x):
    """Continued fraction of the incomplete beta function (modified Lentz)."""
    tiny, qab, qap, qam = 1e-300, a + b, a + 1.0, a - 1.0
    c, d = 1.0, 1.0 - qab * x / qap
    d = 1.0 / (d if abs(d) > tiny else tiny)
    h = d
    for m in range(1, 1000):
        m2 = 2 * m
        aa = m * (b - m) * x / ((qam + m2) * (a + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c
        c = c if abs(c) > tiny else tiny
        h *= d * c
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        d = 1.0 + aa * d
        d = 1.0 / (d if abs(d) > tiny else tiny)
        c = 1.0 + aa / c
        c = c if abs(c) > tiny else tiny
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 1e-15:
            break
    return h


def betainc(a, b, x):
    """Regularized incomplete beta I_x(a, b)."""
    if x <= 0.0:
        return 0.0
    if x >= 1.0:
        return 1.0
    front = math.exp(math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b)
                     + a * math.log(x) + b * math.log1p(-x))
    if x < (a + 1.0) / (a + b + 2.0):
        return front * _betacf(a, b, x) / a
    return 1.0 - front * _betacf(b, a, 1.0 - x) / b


@functools.lru_cache(maxsize=1 << 16)
def cp_low(s, n, alpha=0.05):
    """One-sided Clopper-Pearson lower bound for s successes in n."""
    if n == 0 or s == 0:
        return 0.0
    lo, hi = 0.0, 1.0
    for _ in range(60):
        p = (lo + hi) / 2
        # P(X >= s) at p is I_p(s, n - s + 1).
        if betainc(s, n - s + 1, p) < alpha:
            lo = p
        else:
            hi = p
    return lo


def cp_up(s, n, alpha=0.05):
    if n == 0:
        return 1.0
    return 1.0 - cp_low(n - s, n, alpha)


def load_json(path):
    if path.endswith(".gz"):
        with gzip.open(path, "rt") as f:
            return json.load(f)
    with open(path) as f:
        return json.load(f)


def find(dirs, kind):
    """`*.<kind>.json` and `*.<kind>.json.gz` files under the directories."""
    out = set()
    for d in dirs:
        for ext in ("json", "json.gz"):
            out.update(glob.glob(os.path.join(d, "**", f"*.{kind}.{ext}"), recursive=True))
    return sorted(out)


class Search:
    """One logged selection: nodes in creation order, attempts in order."""

    def __init__(self, path):
        d = load_json(path)
        self.path = path
        self.meta = {k: v for k, v in d.items() if k not in ("nodes", "sims")}
        self.nodes = d["nodes"]
        self.sims = d["sims"]
        self.key_index = {n["key"]: i for i, n in enumerate(self.nodes)}
        self.perm = [n.get("perm") or list(range(n["k"])) for n in self.nodes]
        self.skip = [n.get("skip") or [False] * n["k"] for n in self.nodes]
        self.root = next((i for i, n in enumerate(self.nodes) if n["parent"] == -1), None)
        self.cap = d.get("select_cap") or 0

    def ident(self):
        return {
            "file": os.path.basename(self.path),
            "model": self.meta.get("model"),
            "root": self.meta.get("root"),
            "focal": self.meta.get("focal"),
            "root_step": self.meta.get("root_step"),
            "select_cap": self.cap,
            "tree_hash": self.meta.get("tree_hash"),
            "attempts": len(self.sims),
            "tree_nodes": len(self.nodes),
        }


class State:
    """Counts after the first T natural attempts of a search, with each
    threshold m's best-line values, picks (floor m) and line counts kept
    incrementally: an attempt changes only the nodes on its path."""

    def __init__(self, s, m_values):
        self.s = s
        K = [node["k"] for node in s.nodes]
        N = len(K)
        zeros = lambda: [[0] * k for k in K]  # noqa: E731
        self.n, self.w = zeros(), zeros()
        self.thr = [[0, 0] for _ in range(N)]  # through: tries, wins
        # Direct results at a path's last edge: with no rollout-policy choice
        # of ours after it (clean), and after one (dirty): tries, wins.
        self.ct, self.cw, self.dt, self.dw = zeros(), zeros(), zeros(), zeros()
        self.ms = list(m_values)
        self.Q = {m: [[0.0] * k for k in K] for m in self.ms}
        self.V = {m: [0.0] * N for m in self.ms}
        self.P = {m: [None] * N for m in self.ms}  # best-line pick, floor m
        # Per edge: sum of visits times value over its child nodes.
        self.KS = {m: [[0.0] * k for k in K] for m in self.ms}
        self.KV = zeros()  # per edge: sum of its child nodes' visits
        # Line counts (wins, tries, world-only wins) of the attempts through a
        # node whose choices from it down follow the picks, and per edge the
        # sum over its child nodes.
        self.L = {m: [(0, 0, 0)] * N for m in self.ms}
        self.LK = {m: [[(0, 0, 0)] * k for k in K] for m in self.ms}
        self.T = 0
        self.spent = 0

    def add(self, sim):
        _, win, transitions, rollout_choices, flat = sim
        path = [(flat[i], flat[i + 1]) for i in range(0, len(flat), 2)]
        if not path or any(n < 0 for n, _ in path):
            raise ValueError(f"{self.s.path}: broken path in attempt {sim[0]}")
        for node, a in path:
            self.n[node][a] += 1
            self.w[node][a] += win
            t = self.thr[node]
            t[0] += 1
            t[1] += win
        for node, a in path[:-1]:
            self.KV[node][a] += 1
        last, la = path[-1]
        if rollout_choices == 0:
            self.ct[last][la] += 1
            self.cw[last][la] += win
        else:
            self.dt[last][la] += 1
            self.dw[last][la] += win
        for m in self.ms:
            self._values(path, m)
        self.T += 1
        self.spent += transitions

    def _values(self, path, m):
        s = self.s
        Q, V, P, KS, L, LK = self.Q[m], self.V[m], self.P[m], self.KS[m], self.L[m], self.LK[m]
        carry = None
        for i, a in reversed(path):
            if carry is not None:
                vb, va, lb, la = carry
                KS[i][a] += va - vb
                x = LK[i][a]
                LK[i][a] = (x[0] + la[0] - lb[0], x[1] + la[1] - lb[1], x[2] + la[2] - lb[2])
            cnt = self.KV[i][a] + self.ct[i][a]
            tot = KS[i][a] + self.cw[i][a]
            if cnt >= m:
                Q[i][a] = tot / cnt
            else:
                Q[i][a] = (tot + self.dw[i][a]) / (cnt + self.dt[i][a])
            n_i, q_i, skip = self.n[i], Q[i], s.skip[i]
            best, pick = None, None
            for e in s.perm[i]:
                if skip[e] or n_i[e] < m:
                    continue
                if best is None or q_i[e] > best:
                    best, pick = q_i[e], e
            visits, wins = self.thr[i]
            old_v, old_l = V[i], L[i]
            V[i] = best if best is not None else wins / visits
            P[i] = pick
            lw = lt = lc = 0
            for e in ((pick,) if pick is not None else range(len(n_i))):
                if n_i[e] == 0:
                    continue
                k = LK[i][e]
                lw += self.cw[i][e] + self.dw[i][e] + k[0]
                lt += self.ct[i][e] + self.dt[i][e] + k[1]
                lc += self.cw[i][e] + k[2]
            L[i] = (lw, lt, lc)
            carry = ((visits - 1) * old_v, visits * V[i], old_l, L[i])

    def best_line(self, m):
        """Q per (node, edge) and V per node under the best-line rule."""
        return self.Q[m], self.V[m]

    def pick(self, i, value, floor):
        """Greatest value among edges with at least `floor` backups, ties to
        the seeded order; None when no edge qualifies."""
        best = None
        for a in self.s.perm[i]:
            if self.s.skip[i][a] or self.n[i][a] < floor:
                continue
            v = value(a)
            if best is None or v > best[1]:
                best = (a, v)
        return best[0] if best else None

    def current_pick(self, i):
        return self.pick(i, lambda a: self.w[i][a] / self.n[i][a], CURRENT_FLOOR)

    def bestline_pick(self, i, Q, floor):
        return self.pick(i, lambda a: Q[i][a], floor)

    def line_counts(self, m):
        """Per root edge: (wins, tries, world-only wins) of attempts whose later
        tree choices all follow the best-line pick (floor m)."""
        r = self.s.root
        out = {}
        for a in range(len(self.n[r])):
            if self.n[r][a]:
                k = self.LK[m][r][a]
                out[a] = (self.cw[r][a] + self.dw[r][a] + k[0],
                          self.ct[r][a] + self.dt[r][a] + k[1], self.cw[r][a] + k[2])
        return out


def checkpoints(total, limit):
    if total <= limit:
        return list(range(1, total + 1))
    return sorted({max(1, round(total * (j + 1) / limit)) for j in range(limit)})


def stop_rules(s, m_values, ks, limit, any_line_win=False):
    """Fire point and leader of each stop rule over the search's attempts,
    per threshold m."""
    st = State(s, m_values)
    marks = set(checkpoints(len(s.sims), limit))
    names = ["strict", "bounds"] + [f"proven{k}" for k in ks]
    fired = {m: {} for m in m_values}
    r = s.root
    edges = [a for a in range(len(st.n[r])) if not s.skip[r][a]]
    for t, sim in enumerate(s.sims, 1):
        st.add(sim)
        for m in m_values:
            f = fired[m]
            if len(f) == len(names):
                continue
            Q = st.Q[m]
            lead = st.bestline_pick(r, Q, m)
            if lead is None:
                continue
            rivals = [a for a in edges if a != lead]
            ql, nl = Q[r][lead], st.n[r][lead]
            if "strict" not in f and all(st.n[r][a] > 0 for a in edges):
                mean_tr = st.spent / st.T
                R = max(0.0, (s.cap - st.spent) / mean_tr) if mean_tr and s.cap else 0.0
                worst = ql * nl / (nl + R)
                if all(worst > (Q[r][c] * st.n[r][c] + R) / (st.n[r][c] + R) for c in rivals):
                    f["strict"] = (t, st.spent, lead)
            check_bounds = "bounds" not in f and t in marks
            check_proven = ql >= 1 - EPS and any(f"proven{k}" not in f for k in ks)
            if not (check_bounds or check_proven):
                continue
            lines = st.line_counts(m)
            lw, ln, lworld = lines.get(lead, (0, 0, 0))
            if any_line_win:
                lworld = lw
            if check_bounds and ln:
                ups = [cp_up(*lines[c][:2]) if c in lines else 1.0 for c in rivals]
                if cp_low(lw, ln) > max(ups, default=0.0):
                    f["bounds"] = (t, st.spent, lead)
            if check_proven:
                for k in ks:
                    if f"proven{k}" not in f and lworld >= k:
                        f[f"proven{k}"] = (t, st.spent, lead)
    return {m: {n: fired[m].get(n) for n in names} for m in m_values}, st


def integrity(s, st):
    """Counts rebuilt from the attempts must equal the logged final tree."""
    bad = 0
    for i, node in enumerate(s.nodes):
        if "n" in node and (node["n"] != st.n[i] or node["wins"] != st.w[i]):
            bad += 1
    return bad


def labelled(s, i, name):
    lab = s.nodes[i].get("labels") or {}
    return lab.get(name) or []


def node_kind(s, i):
    """'spy_target' when the Spy target menu offers us and another target,
    'dr_target' when Dread Return offers the Giant and another target."""
    kinds = []
    if labelled(s, i, "spy_self") and labelled(s, i, "spy_other"):
        kinds.append("spy_target")
    if labelled(s, i, "dr_giant") and labelled(s, i, "dr_other"):
        kinds.append("dr_target")
    return kinds


def choice_class(s, i, a):
    if a is None:
        return None
    if a in labelled(s, i, "spy_self"):
        return "self"
    if a in labelled(s, i, "spy_other"):
        return "other"
    if a in labelled(s, i, "dr_giant"):
        return "giant"
    if a in labelled(s, i, "dr_other"):
        return "other"
    return "else"


def mana_split(s, st, i):
    """Visits across one node's colour-choice mana options, per source and
    cost permanent: total tries, options offered and tried, the most tried
    option's share, and tries outside the option the best line would pick."""
    groups = defaultdict(list)
    for row in labelled(s, i, "mana"):
        e, card, colour, src, cost = row
        groups[(card, src, cost)].append((e, colour))
    out = []
    for (card, src, cost), opts in groups.items():
        if len(opts) < 2:
            continue
        tries = [st.n[i][e] for e, _ in opts]
        out.append({"card": card, "options": len(opts), "tried": sum(1 for x in tries if x),
                    "tries": sum(tries), "top_share": (max(tries) / sum(tries)) if sum(tries) else None,
                    "by_colour": {c: st.n[i][e] for e, c in opts},
                    "wins_by_colour": {c: st.w[i][e] for e, c in opts}})
    return out


def analyse(s, args, actual_hits):
    m_values = args.m
    row = s.ident()
    if s.root is None or not s.sims:
        row["skipped"] = "no root or no natural attempts"
        return row, []
    out_nodes = []
    stops, st = stop_rules(s, m_values, args.k, args.checkpoints, args.any_line_win)
    row["integrity_mismatched_nodes"] = integrity(s, st)
    r = s.root
    cur = st.current_pick(r)
    row["current_pick"] = cur
    row["current_value"] = st.w[r][cur] / st.n[r][cur] if cur is not None else None
    row["root_mean"] = st.thr[r][1] / st.thr[r][0]
    row["by_m"] = {}
    hit_ids = {s.key_index[k] for k in actual_hits if k in s.key_index}
    current = [st.current_pick(i) for i in range(len(s.nodes))]
    for m in m_values:
        Q, V = st.best_line(m)
        p8 = st.bestline_pick(r, Q, CURRENT_FLOOR)
        pm = st.bestline_pick(r, Q, m)
        rec = {"pick_floor8": p8, "pick_floor_m": pm,
               "changed_floor8": p8 != cur, "changed_floor_m": pm != cur,
               "value_pick_floor8": Q[r][p8] if p8 is not None else None,
               "root_value": V[r], "stops": {}}
        for name, f in stops[m].items():
            if f is None:
                rec["stops"][name] = None
            else:
                t, spent, lead = f
                rec["stops"][name] = {"attempt": t, "transitions": spent,
                                      "saved_share": 1 - spent / s.cap if s.cap else None,
                                      "pick": lead, "equals_full_bestline": lead == pm,
                                      "equals_full_current": lead == cur}
        # Every tree node with a supported current pick is counted; the root,
        # the actual history's nodes and Spy / Dread Return target nodes are
        # also listed.
        supported = changed = 0
        for i, c in enumerate(current):
            if c is None:
                continue
            p = st.bestline_pick(i, Q, CURRENT_FLOOR)
            supported += 1
            changed += p != c
            kinds = node_kind(s, i)
            if i == r or i in hit_ids or kinds:
                out_nodes.append({"search": row["file"], "m": m, "node": i, "depth": s.nodes[i]["depth"],
                                  "root": i == r, "actual": i in hit_ids, "kinds": kinds,
                                  "current": c, "bestline": p, "current_class": choice_class(s, i, c),
                                  "bestline_class": choice_class(s, i, p),
                                  "current_value": st.w[i][c] / st.n[i][c],
                                  "bestline_value": Q[i][p] if p is not None else None})
        rec["supported_nodes"] = supported
        rec["supported_nodes_changed"] = changed
        row["by_m"][str(m)] = rec
    mana = []
    for i in range(len(s.nodes)):
        for g in mana_split(s, st, i):
            g.update({"node": i, "depth": s.nodes[i]["depth"], "search_attempts": st.T})
            mana.append(g)
    row["mana"] = mana
    return row, out_nodes


def load_actual(dirs):
    """Root identity -> outcome and matched node keys of guide histories."""
    by_root = {}
    for d in dirs:
        for p in find([d], "actual"):
            a = load_json(p)
            for srch in a.get("searches", []):
                k = srch["search"]
                root = a["search_root"] if k == 0 else f"{a['search_root']}/refresh{k}"
                keys = [h[2] for h in a.get("hits", []) if h[0] == k]
                by_root[(a.get("lineage"), root, srch.get("tree_hash"))] = {
                    "game": a.get("id"), "end": a.get("end"), "valid": a.get("valid"),
                    "keys": keys, "seat": a.get("seat")}
    return by_root


def brier(rows, key):
    xs = [(r[key] - r["outcome"]) ** 2 for r in rows if r.get(key) is not None]
    return sum(xs) / len(xs) if xs else None


def calibration(rows, seed=0, reps=2000):
    """Brier scores of each value against the actual game's result, and the
    paired difference with a bootstrap over games."""
    keys = sorted({k for r in rows for k in r if k.startswith("v_")})
    out = {"searches": len(rows), "games": len({r["game"] for r in rows})}
    for k in keys:
        vs = [r for r in rows if r.get(k) is not None]
        out[k] = {"brier": brier(vs, k), "mean_value": sum(r[k] for r in vs) / len(vs) if vs else None,
                  "mean_outcome": sum(r["outcome"] for r in vs) / len(vs) if vs else None,
                  "bins": bins(vs, k)}
    games = defaultdict(list)
    for r in rows:
        games[r["game"]].append(r)
    names = list(games)
    rng = random.Random(seed)
    for k in keys:
        if k == "v_current":
            continue
        diffs = []
        for _ in range(reps):
            sample = [x for g in (rng.choice(names) for _ in names) for x in games[g]]
            b1, b0 = brier(sample, k), brier(sample, "v_current")
            if b1 is not None and b0 is not None:
                diffs.append(b1 - b0)
        diffs.sort()
        if diffs:
            out[k]["brier_minus_current"] = brier(rows, k) - brier(rows, "v_current")
            out[k]["ci95"] = [diffs[int(0.025 * len(diffs))], diffs[int(0.975 * len(diffs)) - 1]]
    return out


def bins(rows, key, edges=(0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0 + EPS)):
    out = []
    for lo, hi in zip(edges, edges[1:]):
        xs = [r for r in rows if lo <= r[key] < hi]
        if xs:
            out.append({"range": [lo, min(hi, 1.0)], "n": len(xs),
                        "mean_value": sum(r[key] for r in xs) / len(xs),
                        "outcome_rate": sum(r["outcome"] for r in xs) / len(xs)})
    return out


def summarise(rows, nodes, m_values, ks):
    out = {"searches": len(rows), "skipped": sum(1 for r in rows if r.get("skipped"))}
    rows = [r for r in rows if not r.get("skipped")]
    out["integrity_bad_searches"] = sum(1 for r in rows if r["integrity_mismatched_nodes"])
    for m in map(str, m_values):
        rs = [r["by_m"][m] for r in rows]
        with_pick = [r for r in rows if r["current_pick"] is not None]
        sm = {"root_pick_changed_floor8": sum(1 for r in with_pick if r["by_m"][m]["changed_floor8"]),
              "root_pick_changed_floor_m": sum(1 for r in with_pick if r["by_m"][m]["changed_floor_m"]),
              "roots_with_current_pick": len(with_pick)}
        for name in ["strict", "bounds"] + [f"proven{k}" for k in ks]:
            fs = [x["stops"][name] for x in rs if x["stops"].get(name)]
            sm[name] = {"fired": len(fs), "of": len(rs),
                        "early_pick_equals_full_bestline": sum(1 for f in fs if f["equals_full_bestline"]),
                        "early_pick_equals_full_current": sum(1 for f in fs if f["equals_full_current"]),
                        "mean_saved_share": (sum(f["saved_share"] for f in fs if f["saved_share"] is not None) / len(fs)) if fs else None}
        ns = [x for x in nodes if str(x["m"]) == m]
        def tally(sel):
            xs = [x for x in ns if sel(x)]
            moves = defaultdict(int)
            for x in xs:
                if x["current"] != x["bestline"]:
                    moves[f"{x['current_class']}->{x['bestline_class']}"] += 1
            return {"nodes": len(xs), "changed": sum(1 for x in xs if x["current"] != x["bestline"]),
                    "changes": dict(moves)}
        sm["all_supported_nodes"] = {"nodes": sum(x["supported_nodes"] for x in rs),
                                     "changed": sum(x["supported_nodes_changed"] for x in rs)}
        sm["listed_nodes"] = tally(lambda x: True)
        sm["actual_history_nodes"] = tally(lambda x: x["actual"])
        sm["spy_target_nodes"] = tally(lambda x: "spy_target" in x["kinds"])
        sm["spy_target_actual"] = tally(lambda x: "spy_target" in x["kinds"] and x["actual"])
        sm["dr_target_nodes"] = tally(lambda x: "dr_target" in x["kinds"])
        sm["dr_target_actual"] = tally(lambda x: "dr_target" in x["kinds"] and x["actual"])
        out[f"m{m}"] = sm
    mana = [g for r in rows for g in r["mana"]]
    by_card = defaultdict(lambda: {"nodes": 0, "tries": 0, "outside_top": 0, "options": 0})
    for g in mana:
        c = by_card[g["card"]]
        c["nodes"] += 1
        c["tries"] += g["tries"]
        c["options"] += g["options"]
        c["outside_top"] += g["tries"] - max(g["by_colour"].values())
    out["mana_colour_split"] = dict(by_card)
    out["mana_attempts_total"] = sum(r["attempts"] for r in rows)
    return out


_JOB = {}


def _init(args, actual, extra):
    _JOB.update(args=args, actual=actual, extra=extra)


def _one(p):
    """Rescores one search log: its row, listed nodes and calibration rows."""
    args, actual, extra = _JOB["args"], _JOB["actual"], _JOB["extra"]
    s = Search(p)
    a = actual.get((s.meta.get("model"), s.meta.get("root"), s.meta.get("tree_hash")))
    row, ns = analyse(s, args, a["keys"] if a else [])
    cal = []
    if row.get("skipped"):
        return row, ns, cal
    outcomes = None
    if a and a.get("valid") and a.get("end") in ("win", "loss"):
        outcomes = [1.0 if a["end"] == "win" else 0.0]
        game = a["game"]
    elif f"{s.meta.get('model')}\t{s.meta.get('root')}" in extra:
        outcomes = extra[f"{s.meta.get('model')}\t{s.meta.get('root')}"]
        game = s.meta.get("root")
    for o in outcomes or []:
        c = {"game": game, "outcome": o, "v_current": row["current_value"], "v_root_mean": row["root_mean"]}
        for m in map(str, args.m):
            c[f"v_bestline_m{m}"] = row["by_m"][m]["value_pick_floor8"]
            c[f"v_bestline_root_m{m}"] = row["by_m"][m]["root_value"]
        cal.append(c)
    return row, ns, cal


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("logs", nargs="+", help="directories of S4A_PATH_LOG files (.json or .json.gz)")
    ap.add_argument("--out", required=True, help="output directory")
    ap.add_argument("--m", type=int, nargs="+", default=[3, 8])
    ap.add_argument("--k", type=int, nargs="+", default=[5, 8])
    ap.add_argument("--checkpoints", type=int, default=400,
                    help="evaluate the bounds rule at every attempt up to this many, else at this many points")
    ap.add_argument("--any-line-win", action="store_true",
                    help="proven-k counts every line win (for logs without rollout-choice counts)")
    ap.add_argument("--outcomes", help="optional JSON {model + '\\t' + root: [results 0/1]} for searches without guides")
    ap.add_argument("--jobs", type=int, default=1, help="worker processes, one search log each")
    args = ap.parse_args(argv)
    actual = load_actual(args.logs)
    extra = load_json(args.outcomes) if args.outcomes else {}
    files = find(args.logs, "search")
    rows, nodes, cal = [], [], []
    if args.jobs > 1:
        pool = multiprocessing.Pool(args.jobs, initializer=_init, initargs=(args, actual, extra))
        results = pool.imap(_one, files)
    else:
        pool = None
        _init(args, actual, extra)
        results = map(_one, files)
    for row, ns, c in results:
        rows.append(row)
        nodes.extend(ns)
        cal.extend(c)
        print(f"{row['file']}: attempts {row['attempts']}", file=sys.stderr)
    if pool:
        pool.close()
        pool.join()
    os.makedirs(args.out, exist_ok=True)
    summary = summarise(rows, nodes, args.m, args.k)
    summary["calibration"] = calibration([c for c in cal if c["v_current"] is not None]) if cal else None
    with open(os.path.join(args.out, "searches.jsonl"), "w") as f:
        for r in rows:
            f.write(json.dumps(r) + "\n")
    with open(os.path.join(args.out, "nodes.jsonl"), "w") as f:
        for x in nodes:
            f.write(json.dumps(x) + "\n")
    with open(os.path.join(args.out, "summary.json"), "w") as f:
        json.dump(summary, f, indent=1)
    print(json.dumps(summary, indent=1))


if __name__ == "__main__":
    main()
