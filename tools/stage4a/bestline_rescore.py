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

Stop rules are evaluated on the root at checkpoints (every attempt for small
searches; `--checkpoints` evenly spaced attempts otherwise):

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
"""

import argparse
import glob
import json
import math
import os
import random
import sys
from collections import defaultdict

CURRENT_FLOOR = 8
EPS = 1e-12


def cp_low(s, n, alpha=0.05):
    """One-sided Clopper-Pearson lower bound for s successes in n."""
    if n == 0 or s == 0:
        return 0.0
    lo, hi = 0.0, 1.0
    for _ in range(60):
        p = (lo + hi) / 2
        # P(X >= s) at p, through the log-space binomial tail.
        tail = 0.0
        for k in range(s, n + 1):
            tail += math.exp(
                math.lgamma(n + 1) - math.lgamma(k + 1) - math.lgamma(n - k + 1)
                + (k * math.log(p) if k else 0.0)
                + ((n - k) * math.log1p(-p) if n - k else 0.0)
            )
        if tail < alpha:
            lo = p
        else:
            hi = p
    return lo


def cp_up(s, n, alpha=0.05):
    if n == 0:
        return 1.0
    return 1.0 - cp_low(n - s, n, alpha)


class Search:
    """One logged selection: nodes in creation order, attempts in order."""

    def __init__(self, path):
        with open(path) as f:
            d = json.load(f)
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
    """Counts after the first T natural attempts of a search."""

    def __init__(self, s):
        self.s = s
        N = len(s.nodes)
        self.n = [[0] * s.nodes[i]["k"] for i in range(N)]
        self.w = [[0] * s.nodes[i]["k"] for i in range(N)]
        self.thr = [[0, 0] for _ in range(N)]  # through: tries, wins
        self.kids = [defaultdict(lambda: defaultdict(int)) for _ in range(N)]  # a -> child -> count
        self.clean = [defaultdict(lambda: [0, 0]) for _ in range(N)]  # a -> [count, wins]
        self.dirty = [defaultdict(lambda: [0, 0]) for _ in range(N)]
        self.T = 0
        self.spent = 0
        self.attempts = []  # per attempt: (root edge, path, win, world_only)

    def add(self, sim):
        _, win, transitions, rollout_choices, flat = sim
        path = [(flat[i], flat[i + 1]) for i in range(0, len(flat), 2)]
        if any(n < 0 for n, _ in path):
            raise ValueError(f"{self.s.path}: broken path in attempt {sim[0]}")
        world_only = rollout_choices == 0
        for j, (node, a) in enumerate(path):
            self.n[node][a] += 1
            self.w[node][a] += win
            self.thr[node][0] += 1
            self.thr[node][1] += win
            if j + 1 < len(path):
                self.kids[node][a][path[j + 1][0]] += 1
            else:
                bucket = self.clean if world_only else self.dirty
                bucket[node][a][0] += 1
                bucket[node][a][1] += win
        self.T += 1
        self.spent += transitions
        self.attempts.append((path[0][1] if path else None, path, win, world_only))

    def best_line(self, m):
        """Q per (node, edge) and V per node under the best-line rule."""
        s = self.s
        V = [None] * len(s.nodes)
        Q = [dict() for _ in range(len(s.nodes))]
        # Children are created after their parents: descending ids are bottom-up.
        for i in range(len(s.nodes) - 1, -1, -1):
            if self.thr[i][0] == 0:
                continue
            best = None
            for a in range(len(self.n[i])):
                if self.n[i][a] == 0 or s.skip[i][a]:
                    continue
                csum, ccnt = 0.0, 0
                for c, cnt in self.kids[i][a].items():
                    csum += cnt * V[c]
                    ccnt += cnt
                cl = self.clean[i].get(a, (0, 0))
                csum += cl[1]
                ccnt += cl[0]
                if ccnt >= m:
                    q = csum / ccnt
                else:
                    dt = self.dirty[i].get(a, (0, 0))
                    q = (csum + dt[1]) / (ccnt + dt[0])
                Q[i][a] = q
                if self.n[i][a] >= m and (best is None or q > best):
                    best = q
            V[i] = best if best is not None else self.thr[i][1] / self.thr[i][0]
        return Q, V

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

    def line_counts(self, Q, m):
        """Per root edge: (wins, tries, world-only wins) of attempts whose later
        tree choices all follow the best-line pick (floor m)."""
        picks = {}
        out = defaultdict(lambda: [0, 0, 0])
        for a0, path, win, world_only in self.attempts:
            ok = True
            for node, a in path[1:]:
                if node not in picks:
                    picks[node] = self.bestline_pick(node, Q, m)
                p = picks[node]
                if p is not None and p != a:
                    ok = False
                    break
            if ok:
                c = out[a0]
                c[0] += win
                c[1] += 1
                c[2] += win and world_only
        return out


def checkpoints(total, limit):
    if total <= limit:
        return list(range(1, total + 1))
    return sorted({max(1, round(total * (j + 1) / limit)) for j in range(limit)})


def stop_rules(s, m, ks, limit, any_line_win=False):
    """Fire point and leader of each stop rule over the search's attempts."""
    st = State(s)
    marks = set(checkpoints(len(s.sims), limit))
    fired = {}
    names = ["strict", "bounds"] + [f"proven{k}" for k in ks]
    r = s.root
    for t, sim in enumerate(s.sims, 1):
        st.add(sim)
        if t not in marks or len(fired) == len(names):
            continue
        Q, V = st.best_line(m)
        lead = st.bestline_pick(r, Q, m)
        if lead is None:
            continue
        mean_tr = st.spent / st.T if st.T else 0
        R = max(0.0, (s.cap - st.spent) / mean_tr) if mean_tr and s.cap else 0.0
        edges = [a for a in range(len(st.n[r])) if not s.skip[r][a]]
        rivals = [a for a in edges if a != lead]
        ql, nl = Q[r][lead], st.n[r][lead]
        if "strict" not in fired and all(st.n[r][a] > 0 for a in edges):
            worst = ql * nl / (nl + R)
            if all(worst > (Q[r][c] * st.n[r][c] + R) / (st.n[r][c] + R) for c in rivals):
                fired["strict"] = (t, st.spent, lead)
        need_lines = "bounds" not in fired or (
            ql >= 1 - EPS and any(f"proven{k}" not in fired for k in ks)
        )
        if not need_lines:
            continue
        lines = st.line_counts(Q, m)
        lw, ln, lworld = lines.get(lead, (0, 0, 0))
        if any_line_win:
            lworld = lw
        if "bounds" not in fired and ln:
            ups = [cp_up(*lines[c][:2]) if c in lines else 1.0 for c in rivals]
            if cp_low(lw, ln) > max(ups, default=0.0):
                fired["bounds"] = (t, st.spent, lead)
        if ql >= 1 - EPS:
            for k in ks:
                if f"proven{k}" not in fired and lworld >= k:
                    fired[f"proven{k}"] = (t, st.spent, lead)
    return {n: fired.get(n) for n in names}, st


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
    stops = {}
    final = None
    for m in m_values:
        fired, st = stop_rules(s, m, args.k, args.checkpoints, args.any_line_win)
        stops[m] = fired
        final = st
    st = final
    row["integrity_mismatched_nodes"] = integrity(s, st)
    r = s.root
    cur = st.current_pick(r)
    row["current_pick"] = cur
    row["current_value"] = st.w[r][cur] / st.n[r][cur] if cur is not None else None
    row["root_mean"] = st.thr[r][1] / st.thr[r][0]
    row["by_m"] = {}
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
        row["by_m"][str(m)] = rec
        # Every tree node with a supported current pick, and the actual history.
        hit_ids = {s.key_index[k] for k in actual_hits if k in s.key_index}
        for i in range(len(s.nodes)):
            c = st.current_pick(i)
            if c is None:
                continue
            p = st.bestline_pick(i, Q, CURRENT_FLOOR)
            out_nodes.append({"search": row["file"], "m": m, "node": i, "depth": s.nodes[i]["depth"],
                              "root": i == r, "actual": i in hit_ids, "kinds": node_kind(s, i),
                              "current": c, "bestline": p, "current_class": choice_class(s, i, c),
                              "bestline_class": choice_class(s, i, p),
                              "current_value": st.w[i][c] / st.n[i][c],
                              "bestline_value": Q[i][p] if p is not None else None})
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
        for p in glob.glob(os.path.join(d, "**", "*.actual.json"), recursive=True):
            with open(p) as f:
                a = json.load(f)
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
        sm["all_supported_nodes"] = tally(lambda x: True)
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


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("logs", nargs="+", help="directories of S4A_PATH_LOG files")
    ap.add_argument("--out", required=True, help="output directory")
    ap.add_argument("--m", type=int, nargs="+", default=[3, 8])
    ap.add_argument("--k", type=int, nargs="+", default=[5, 8])
    ap.add_argument("--checkpoints", type=int, default=400,
                    help="evaluate stop rules at every attempt up to this many, else at this many points")
    ap.add_argument("--any-line-win", action="store_true",
                    help="proven-k counts every line win (for logs without rollout-choice counts)")
    ap.add_argument("--outcomes", help="optional JSON {model + '\\t' + root: [results 0/1]} for searches without guides")
    args = ap.parse_args(argv)
    actual = load_actual(args.logs)
    extra = {}
    if args.outcomes:
        with open(args.outcomes) as f:
            extra = json.load(f)
    rows, nodes, cal = [], [], []
    files = sorted(p for d in args.logs for p in glob.glob(os.path.join(d, "**", "*.search.json"), recursive=True))
    for p in files:
        s = Search(p)
        a = actual.get((s.meta.get("model"), s.meta.get("root"), s.meta.get("tree_hash")))
        row, ns = analyse(s, args, a["keys"] if a else [])
        rows.append(row)
        nodes.extend(ns)
        if row.get("skipped"):
            continue
        outcomes = None
        if a and a.get("valid") and a.get("end") in ("win", "loss"):
            outcomes = [1.0 if a["end"] == "win" else 0.0]
            game = a["game"]
        elif f"{s.meta.get('model')}\t{s.meta.get('root')}" in extra:
            outcomes = extra[f"{s.meta.get('model')}\t{s.meta.get('root')}"]
            game = s.meta.get("root")
        if outcomes:
            for o in outcomes:
                c = {"game": game, "outcome": o, "v_current": row["current_value"], "v_root_mean": row["root_mean"]}
                for m in map(str, args.m):
                    c[f"v_bestline_m{m}"] = row["by_m"][m]["value_pick_floor8"]
                    c[f"v_bestline_root_m{m}"] = row["by_m"][m]["root_value"]
                cal.append(c)
        print(f"{row['file']}: attempts {row['attempts']}", file=sys.stderr)
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
