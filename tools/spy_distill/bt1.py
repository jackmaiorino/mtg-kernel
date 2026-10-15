"""Best-line evidence targets (BT1) for the Spy archive distillation line (collab LANES/bestline-target-test-20261015).

Offline: reads the training banks, the replays' path logs (``S4A_PATH_LOG``),
phase-1 receipts, probes and panels. Nothing is played or trained here, and no
Spy label selects a row or shapes a target: labels are read only to check the
join, to report, and for the design's stop rule.

Evidence rule (DESIGN.md section 3). Each tree-executed decision of a replayed
archive game maps, through its single actual-log hit, to one node of one search
tree, read at that search's final counts (the counts E executed from). At that
node, tools/stage4a/bestline_rescore.py's State gives each edge's line counts at
threshold m: the attempts through the edge whose later tree choices all follow
their own node's best-line pick (floor m; every attempt counts below a node
with no pick), and their wins. The leader is the node's best-line pick at floor
8, ties to the seeded order; another edge with at least 8 tries whose value is
within the rescorer's TIE_EPS of the leader's is tied with it. An edge other
than the leader is separated when the one-sided 95% Clopper-Pearson upper bound
of its line counts is below the lower bound of the leader's; an edge with no
line tries has upper bound 1, so it is never separated. An evidence row has at
least one separated edge. The primary rule is m = 8. Beside it, in the census
only: best line at m = 3 (values and line counts follow floor-3 picks; the
leader is the floor-8 pick), and the mean rule (raw tries and wins, with E's
executed edge as the leader).

Targets. Cell T keeps every non-separated edge at the parent's ratio; cell P
keeps the leader and the non-separated edges tied with it. A keep-set file
lists each evidence row's kept live actions; the engine builds the target from
it and the bank's parent logits (``ArchiveDistillationKeepSetsV1``). Live
actions come from the actual log's ``hit_live`` map, which the recorder writes
beside each hit.

Voids. A game whose replay is not exact, or whose join fails at any of its
tree-executed rows (its hit, node, counts, labels or live map), is void: its
rows leave both cells. More than one void game in twenty stops the step as
engineering (TA1 Part B's rule).

Commands (JSON summary on stdout):
  replays  --spec SPEC.json --out REPLAYS.json
      stage 1: collect each replayed game's check-game verdict (TA1's
      audit_b.py check-game), require every kept bank game once, and, for the
      games TA1 replayed, require every search log equal to TA1's and each
      actual log equal to TA1's, field for field and in order, apart from the
      live-index field.
  census   --spec SPEC.json --out-dir DIR
      stage 0 (no live indices; TA1's retained logs) or stage 2 (live indices,
      the target measures and the keep-set files), each with the stop rule.
  commands --lineage r1 --parent P.json --games GAMES.jsonl --bank BANK.json --anchor ANCHOR.json
           --census CENSUS.json --root DIR --out-dir CMDS
      stage 3: each cell's dry-run, full-run and rerun phase-1 command, with
      output directories DIR/LIN-BT1-T-dry, DIR/LIN-BT1-T, DIR/LIN-BT1-T-rerun
      and the same for BT1-P
  fit      --spec SPEC.json --out FIT.json      sections 5, 7.2, 7.3 and 7.5
  x4       --screen-index INDEX.json --screen-result RESULT.json --out X4.json
      the screen's D1L-parent points and intervals, reproduced from its own
      panels before any BT1 panel is read (exits 1 when X4 fails)
  panels   --spec SPEC.json --fit FIT.json --out PANELS.json   sections 6 and 7.4
SPEC files are described at each command's function.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import importlib.util
import json
import math
import struct
import sys
from collections import Counter
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import cells  # noqa: E402
import fit as screen  # noqa: E402
import phase1  # noqa: E402
from distill_common import LINEAGES, pin, read_json, require, write_json  # noqa: E402


def _load(name: str, path: Path):
    """A module loaded by its path (other tool directories hold modules of the
    same name)."""
    if name in sys.modules:
        return sys.modules[name]
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


RESCORE = _load("bt1_bestline_rescore", HERE.parent / "stage4a" / "bestline_rescore.py")
analyze = _load("spy_distill_screen_analyze", HERE / "analyze.py")

M, BESIDE_M, LEADER_FLOOR = 8, 3, 8
TIE_EPS = RESCORE.TIE_EPS
PRIMARY = "bestline8"
RULES = {"bestline8": M, "bestline3": BESIDE_M, "mean": None}
BANK_SCHEMA = "mtg-kernel-archive-distillation-bank/v1"
KEEP_SETS_SCHEMA = "mtg-kernel-archive-distillation-keep-sets/v1"
ACTUAL_V1, ACTUAL_V2 = "s4a-path-log/actual-v1", "s4a-path-log/actual-v2"
REPLAYS_SCHEMA = "bestline-target-test-v1/replays"
CENSUS_SCHEMA = "bestline-target-test-v1/census"
FIT_SCHEMA = "bestline-target-test-v1/fit"
X4_SCHEMA = "bestline-target-test-v1/x4"
PANELS_SCHEMA = "bestline-target-test-v1/panels"
CLASSES = ("self_chosen", "self_declined", "giant_chosen", "giant_declined", "cast_offered", "other")
CHOOSERS = ("new_root", "reused")
CELLS = ("T", "P")
CELL_LABELS = {"T": "BT1-T", "P": "BT1-P"}
LINE_LABELS = (("self", "self_offered", "self_chosen", "spy_self", "self_actions"),
               ("giant", "giant_offered", "giant_chosen", "dr_giant", "giant_actions"))
# Self line, anti-line and Giant line rows: the target's mass on the menu's
# self-target, opponent-target or Giant actions, against LINE_ROW_FLOOR.
LINE_SETS = (("self", "self_mass"), ("anti", "other_mass"), ("giant", "giant_mass"))
LINE_ROW_FLOOR = 0.5
FEW_LINE_ROWS = 3
VOID_RATIO = 20


# ------------------------------------------------------------------ files


def sha256_file(path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def pinned(entry: dict, what: str) -> Path:
    path = Path(entry["path"])
    require(path.is_file() and sha256_file(path) == entry["sha256"], f"{what}: {path} is missing or differs")
    return path


def read_pinned(entry: dict, what: str):
    return json.loads(pinned(entry, what).read_text(encoding="utf-8"))


def load_log(path: Path):
    if path.name.endswith(".gz"):
        with gzip.open(path, "rt", encoding="utf-8") as f:
            return json.load(f)
    return json.loads(path.read_text(encoding="utf-8"))


def logs(d: Path, kind: str) -> list:
    return sorted(set(d.rglob(f"*.{kind}.json")) | set(d.rglob(f"*.{kind}.json.gz")))


def log_hashes(d: Path) -> dict:
    return {p.relative_to(d).as_posix(): sha256_file(p) for p in logs(d, "search") + logs(d, "actual")}


def load_bank(entry: dict) -> dict:
    lineage = entry["lineage"]
    bank = read_pinned({"path": entry["bank"], "sha256": entry["bank_sha256"]}, f"{lineage} bank")
    require(bank.get("schema") == BANK_SCHEMA and bank.get("lineage") == lineage, f"{lineage}: not a {lineage} bank")
    keys = [(r["game"], r["step"]) for r in bank["rows"]]
    require(len(set(keys)) == len(keys), f"{lineage}: bank rows repeat a game and step")
    return bank


def banks(spec: dict) -> dict:
    out = {}
    for entry in spec["banks"]:
        require(entry["lineage"] in LINEAGES and entry["lineage"] not in out, "one bank per lineage")
        out[entry["lineage"]] = load_bank(entry)
    require(set(out) == set(LINEAGES), "the spec names both lineages' banks")
    return out


def kept_games(bank: dict) -> set:
    return {g["id"] for g in bank["game_status"] if g["kept"]}


def f32(bits: int) -> float:
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def mean(values: list):
    return sum(values) / len(values) if values else None


# ------------------------------------------------------------- evidence


def frozen_choice(n: list, wins: list, perm: list):
    """E's executed edge: greatest mean among edges with at least eight tries,
    ties to the seeded order (tree.rs frozen_choice; exact comparison)."""
    best = None
    for a in perm:
        if n[a] < LEADER_FLOOR:
            continue
        if best is None or wins[a] * n[best] > wins[best] * n[a]:
            best = a
    return best


def edge_counts(st, i: int, m) -> list:
    """Per edge (wins, tries): line counts at threshold m (bestline_rescore's
    line_counts, read at node i), or the raw counts when m is None."""
    if m is None:
        return [(st.w[i][a], st.n[i][a]) for a in range(len(st.n[i]))]
    out = []
    for a in range(len(st.n[i])):
        if st.n[i][a] == 0:
            out.append((0, 0))
            continue
        k = st.LK[m][i][a]
        out.append((st.cw[i][a] + st.dw[i][a] + k[0], st.ct[i][a] + st.dt[i][a] + k[1]))
    return out


def edge_values(st, i: int, m) -> list:
    """Per edge, the value its rule ranks by: the best-line value at threshold
    m, or the mean when m is None; None for an untried edge."""
    if m is None:
        return [st.w[i][a] / st.n[i][a] if st.n[i][a] else None for a in range(len(st.n[i]))]
    return [st.Q[m][i][a] if st.n[i][a] else None for a in range(len(st.n[i]))]


def evidence(st, i: int, x: int, m) -> dict:
    """One rule at node i: its leader, the edges tied with it, per-edge
    counts, the separated edges, and the separated edges whose line counts
    are their raw counts."""
    counts = edge_counts(st, i, m)
    leader = x if m is None else st.bestline_pick(i, st.Q[m], LEADER_FLOOR)
    require(leader is not None, "a tree-executed node has no leader")
    if m == M:
        require(leader == st.P[M][i], "the leader differs from the rescorer's pick at floor 8")
    values = edge_values(st, i, m)
    tied = [a for a in range(len(counts)) if a != leader and st.n[i][a] >= LEADER_FLOOR and not st.s.skip[i][a]
            and abs(values[a] - values[leader]) <= TIE_EPS]
    low = RESCORE.cp_low(*counts[leader])
    separated = [a for a, (w, t) in enumerate(counts) if a != leader and RESCORE.cp_up(w, t) < low]
    raw = [a for a in separated if counts[a] == (st.w[i][a], st.n[i][a])]
    return {"leader": leader, "tied": tied, "counts": [list(c) for c in counts], "separated": separated,
            "raw_separated": raw, "evidence": bool(separated)}


def decision_class(line: dict) -> str:
    if line["self_chosen"]:
        return "self_chosen"
    if line["self_offered"]:
        return "self_declined"
    if line["giant_chosen"]:
        return "giant_chosen"
    if line["giant_offered"]:
        return "giant_declined"
    return "cast_offered" if line["cast_offered"] else "other"


class Void(Exception):
    """A game whose join fails: it is listed as void, and its rows leave both
    cells."""


def void_unless(condition, message: str) -> None:
    if not condition:
        raise Void(message)


def check_labels(at: str, labels: dict, row: dict, x: int, live_map) -> None:
    """The node's Spy labels against the bank row's flags and, with a live
    map, the labelled edges' live actions against the row's."""
    for _, offered, chosen, label, actions in LINE_LABELS:
        edges = set(labels.get(label) or [])
        void_unless(bool(edges) == bool(row["line"][offered]) and (x in edges) == bool(row["line"][chosen])
                    and len(edges) == len(row[actions]), f"{at}: the node's labels differ from the row's {label} flags")
        if live_map is not None:
            void_unless(sorted(live_map[e] for e in edges) == sorted(row[actions]),
                        f"{at}: the {label} edges' live actions differ from the row's")
    if live_map is not None:
        void_unless(not {live_map[e] for e in labels.get("spy_other") or []} & set(row["self_actions"]),
                    f"{at}: an opponent-target edge maps to a self-target action")


def search_state(s, at: str):
    st = RESCORE.State(s, [BESIDE_M, M])
    try:
        for sim in s.sims:
            st.add(sim)
    except ValueError as e:
        raise Void(f"{at}: {e}") from None
    void_unless(RESCORE.integrity(s, st) == 0, f"{at}: the sims do not rebuild the counts")
    return st


def game_rows(lineage: str, gid: str, bank_rows: dict, d: Path, live: bool) -> list:
    """Each tree-executed bank row of one exact replay, joined to its node,
    with every rule's evidence (and, with ``live``, its live map). Every row
    is checked; any failed check raises Void for the whole game."""
    actual = [a for a in (load_log(p) for p in logs(d, "actual")) if a.get("id") == gid and a.get("lineage") == lineage]
    void_unless(len(actual) == 1, f"{lineage} {gid}: {len(actual)} actual logs")
    actual = actual[0]
    hit_live = actual.get("hit_live")
    if live:
        void_unless(actual.get("schema") == ACTUAL_V2 and isinstance(hit_live, list)
                    and len(hit_live) == len(actual["hits"]), f"{lineage} {gid}: the actual log has no live maps")
    by_identity: dict = {}
    for p in logs(d, "search"):
        s = RESCORE.Search(str(p))
        by_identity.setdefault((s.meta.get("model"), s.meta.get("root"), s.meta.get("tree_hash")), []).append(s)
    trees = []
    for k, rec in enumerate(actual["searches"]):
        name = actual["search_root"] if k == 0 else f"{actual['search_root']}/refresh{k}"
        found = by_identity.get((lineage, name, rec["tree_hash"]), [])
        void_unless(rec["search"] == k and len(found) == 1,
                    f"{lineage} {gid}: search {k}: {len(found)} search logs with its root and tree hash")
        trees.append(found[0])
    hits: dict = {}
    for index, (k, step, key) in enumerate(actual["hits"]):
        hits.setdefault(step, []).append((k, key, hit_live[index] if hit_live is not None else None))
    states, out = {}, []
    for step, row in sorted(bank_rows.items()):
        at = f"{lineage} {gid} step {step}"
        found = hits.get(step, [])
        void_unless(len(found) == 1, f"{at}: {len(found)} actual-log hits")
        k, key, live_map = found[0]
        void_unless(type(k) is int and 0 <= k < len(trees), f"{at}: the hit names no search")
        s = trees[k]
        i = s.key_index.get(key)
        void_unless(i is not None, f"{at}: the hit's node is not in search {k}'s log")
        node = s.nodes[i]
        void_unless("n" in node and "wins" in node, f"{at}: the hit's node has no counts")
        n, wins, perm = node["n"], node["wins"], node.get("perm")
        void_unless(node["k"] == row["k"] == len(n) == len(wins) and isinstance(perm, list)
                    and sorted(perm) == list(range(len(n))) and node["visits"] == sum(n),
                    f"{at}: the node's arrays differ from the row's menu or from each other")
        void_unless(not any(s.skip[i]), f"{at}: the node has skipped edges")
        void_unless(row["chooser"] != "new_root" or node["parent"] == -1,
                    f"{at}: a new_root hit is not its search's root")
        x = frozen_choice(n, wins, perm)
        void_unless(x is not None and n[x] == row["edge_backups"],
                    f"{at}: the frozen choice's tries differ from edge_backups")
        if live:
            void_unless(isinstance(live_map, list) and all(type(v) is int for v in live_map)
                        and sorted(live_map) == list(range(row["k"])), f"{at}: the live map is not a permutation")
            void_unless(live_map[x] == row["target"], f"{at}: the executed edge's live action differs from the target")
        labels = node.get("labels") or {}
        check_labels(at, labels, row, x, live_map if live else None)
        if k not in states:
            states[k] = search_state(s, f"{lineage} {gid}: search {k}")
        st = states[k]
        out.append({"game": gid, "step": step, "chooser": row["chooser"], "class": decision_class(row["line"]),
                    "k": row["k"], "search": k, "key": key, "visits": node["visits"], "n": n, "wins": wins, "x": x,
                    "labels": {name: labels[name] for name in ("spy_self", "spy_other", "dr_giant", "dr_other")
                               if labels.get(name)},
                    "live": live_map if live else None,
                    "rules": {name: evidence(st, i, x, m) for name, m in RULES.items()}})
    return out


# --------------------------------------------------------------- census


def self_offer(rule: dict, labels: dict) -> str:
    """At a self-target offer: which target leads, and whether the other side
    is separated."""
    own, other = set(labels.get("spy_self", [])), set(labels.get("spy_other", []))
    separated = set(rule["separated"])
    if rule["leader"] in own and other and other <= separated:
        return "self_leads_other_separated"
    if rule["leader"] in other and own and own <= separated:
        return "other_leads_self_separated"
    return "neither"


def giant_offer(rule: dict, labels: dict, n: list) -> dict:
    giant, other = set(labels.get("dr_giant", [])), set(labels.get("dr_other", []))
    separated = set(rule["separated"])
    return {"giant_leads": rule["leader"] in giant, "evidence": rule["evidence"],
            "other_separated": len(other & separated),
            "other_tried_not_separated": sum(1 for e in other if e not in separated and n[e] > 0),
            "other_untried": sum(1 for e in other if n[e] == 0),
            "giant_edges": len(giant), "giant_untried": sum(1 for e in giant if n[e] == 0)}


def anti_on_raw_counts(rule: dict, labels: dict) -> bool:
    """An opponent-led self-target offer whose separated self-target edge is
    compared by its raw counts (no pick below it)."""
    return bool(set(labels.get("spy_self", [])) & set(rule["raw_separated"]))


def summarize(rows: list, rule: str) -> dict:
    """One lineage's census under one rule (section 4, stage 0 measures)."""
    ev = [r for r in rows if r["rules"][rule]["evidence"]]
    tried = sum(sum(1 for a in range(r["k"]) if a != r["rules"][rule]["leader"] and r["n"][a] > 0) for r in ev)
    separated = sum(len(r["rules"][rule]["separated"]) for r in ev)
    out = {"rows": len(rows), "evidence_rows": len(ev),
           "by_class": {c: {"rows": sum(r["class"] == c for r in rows),
                            "evidence_rows": sum(r["class"] == c for r in ev)} for c in CLASSES},
           "by_chooser": {c: {"rows": sum(r["chooser"] == c for r in rows),
                              "evidence_rows": sum(r["chooser"] == c for r in ev)} for c in CHOOSERS},
           "leader_differs_from_executed": sum(r["rules"][rule]["leader"] != r["x"] for r in ev),
           "tied_leader_evidence_rows": {c: sum(1 for r in ev if r["class"] == c and r["rules"][rule]["tied"])
                                         for c in CLASSES},
           "separated_edges": separated,
           "separated_on_raw_counts": None if RULES[rule] is None else
           {c: sum(len(r["rules"][rule]["raw_separated"]) for r in ev if r["class"] == c) for c in CLASSES},
           "separated_per_evidence_row": {str(k): v for k, v in
                                          sorted(Counter(len(r["rules"][rule]["separated"]) for r in ev).items())},
           "tried_rivals_at_evidence_rows": tried,
           "tried_rivals_separated_share": separated / tried if tried else None,
           "self_offers": {}, "giant_offers": {}}
    for verdict, cls in (("chosen", "self_chosen"), ("declined", "self_declined")):
        offers = [r for r in rows if r["class"] == cls]
        found = Counter(self_offer(r["rules"][rule], r["labels"]) for r in offers)
        out["self_offers"][verdict] = {
            "offers": len(offers),
            **{k: found.get(k, 0) for k in ("self_leads_other_separated", "other_leads_self_separated", "neither")},
            "other_leads_self_separated_on_raw_counts": None if RULES[rule] is None else sum(
                1 for r in offers if self_offer(r["rules"][rule], r["labels"]) == "other_leads_self_separated"
                and anti_on_raw_counts(r["rules"][rule], r["labels"]))}
    for verdict, cls in (("chosen", "giant_chosen"), ("declined", "giant_declined")):
        offers = [giant_offer(r["rules"][rule], r["labels"], r["n"]) for r in rows if r["class"] == cls]
        out["giant_offers"][verdict] = {"offers": len(offers),
                                        "giant_leads": sum(o["giant_leads"] for o in offers),
                                        "giant_leads_rival_separated": sum(o["giant_leads"] and o["evidence"]
                                                                           for o in offers),
                                        **{k: sum(o[k] for o in offers) for k in
                                           ("other_separated", "other_tried_not_separated", "other_untried",
                                            "giant_edges", "giant_untried")}}
    return out


def softmax(bits: list) -> list:
    z = [f32(b) for b in bits]
    top = max(z)
    e = [math.exp(v - top) for v in z]
    total = sum(e)
    return [v / total for v in e]


def first_max(bits: list) -> int:
    z = [f32(b) for b in bits]
    return z.index(max(z))


def keep_sets(row: dict) -> dict:
    """The primary rule's kept live actions per cell at an evidence row: T
    every non-separated edge, P the leader and the non-separated edges tied
    with it. Neither reads a label."""
    rule, live = row["rules"][PRIMARY], row["live"]
    separated = set(rule["separated"])
    best = {rule["leader"], *rule["tied"]} - separated
    return {"T": sorted(live[e] for e in range(row["k"]) if e not in separated), "P": sorted(live[e] for e in best)}


def target_measures(row: dict, bank_row: dict) -> dict:
    """The parent's softmax (binary64 from its binary32 logits) at an
    evidence row: its probability on the leader, whether its top action
    differs from the leader, and per cell the forward KL from the parent to
    the target and the target's mass on the line actions, the opponent
    target, the leader and the untried edges (descriptive; the engine's dry
    run records its own exact values)."""
    p = softmax(bank_row["parent_logits"])
    rule, live = row["rules"][PRIMARY], row["live"]
    leader = live[rule["leader"]]
    untried = {live[e] for e in range(row["k"]) if row["n"][e] == 0}
    other = {live[e] for e in row["labels"].get("spy_other", [])} if bank_row["line"]["self_offered"] else set()
    sets = keep_sets(row)
    out = {"parent_on_leader": p[leader], "parent_top_differs": first_max(bank_row["parent_logits"]) != leader,
           "tied_leader": bool(rule["tied"]), "t_equals_p": sets["T"] == sets["P"], "cells": {}}
    for cell, keep in sets.items():
        total = sum(p[a] for a in keep)
        q = {a: p[a] / total for a in keep}
        line = {name: sum(q.get(a, 0.0) for a in bank_row[actions]) if bank_row[actions] else None
                for name, _, _, _, actions in LINE_LABELS}
        out["cells"][cell] = {"keep": keep, "kl": -math.log(total), "self_mass": line["self"],
                              "giant_mass": line["giant"],
                              "other_mass": sum(q.get(a, 0.0) for a in other) if other else None,
                              "leader_mass": q.get(leader, 0.0), "untried_mass": sum(q.get(a, 0.0) for a in untried)}
    return out


def target_summary(rows: list, measures: dict) -> dict:
    """Stage 2's target measures over a lineage's evidence rows, with each
    cell's self line, anti-line and Giant line rows."""
    ev = [r for r in rows if r["rules"][PRIMARY]["evidence"]]
    key = lambda r: (r["game"], r["step"])  # noqa: E731
    keyed = [measures[key(r)] for r in ev]
    out = {"evidence_rows": len(ev),
           "mean_parent_on_leader": mean([m["parent_on_leader"] for m in keyed]),
           "parent_top_differs": sum(m["parent_top_differs"] for m in keyed),
           "tied_leader_rows": sum(m["tied_leader"] for m in keyed),
           "t_equals_p_rows": sum(m["t_equals_p"] for m in keyed), "cells": {}}
    for cell in CELLS:
        at = lambda r: measures[key(r)]["cells"][cell]  # noqa: E731
        entry = {"kl_by_class": {c: sum(at(r)["kl"] for r in ev if r["class"] == c) for c in CLASSES}}
        entry["kl_total"] = sum(entry["kl_by_class"].values())
        entry["kl_where_t_equals_p"] = sum(at(r)["kl"] for r in ev if measures[key(r)]["t_equals_p"])
        entry["kl_elsewhere"] = entry["kl_total"] - entry["kl_where_t_equals_p"]
        entry["mean_leader_mass"] = mean([at(r)["leader_mass"] for r in ev])
        entry["mean_untried_mass"] = mean([at(r)["untried_mass"] for r in ev])
        for name in ("self", "giant"):
            masses = [at(r)[f"{name}_mass"] for r in ev if at(r)[f"{name}_mass"] is not None]
            entry[f"{name}_offer_rows"] = len(masses)
            entry[f"mean_{name}_mass"] = mean(masses)
        entry["line_rows"] = {}
        for name, field in LINE_SETS:
            hits = [r for r in ev if at(r)[field] is not None and at(r)[field] >= LINE_ROW_FLOOR]
            entry["line_rows"][name] = {"rows": len(hits), "mean_target_mass": mean([at(r)[field] for r in hits]),
                                        "keys": [list(key(r)) for r in hits]}
        entry["line_rows"]["anti"]["on_raw_counts"] = sum(
            1 for r in ev if at(r)["other_mass"] is not None and at(r)["other_mass"] >= LINE_ROW_FLOOR
            and anti_on_raw_counts(r["rules"][PRIMARY], r["labels"]))
        out["cells"][cell] = entry
    return out


def stop_rule(lineages: dict) -> dict:
    """Section 4: BT1 stops when either lineage has no self-target offer where
    self-target leads with the opponent target separated. Beside it, the
    reverse count, the Giant offers where a Giant edge leads with a rival
    separated, and the lineages with fewer than three self line rows (where
    the line half is a weak test)."""
    def total(lin, offers, field):
        return sum(lineages[lin]["rules"][PRIMARY][offers][v][field] for v in ("chosen", "declined"))
    leads = {lin: total(lin, "self_offers", "self_leads_other_separated") for lin in LINEAGES}
    return {"self_leads_other_separated": leads, "stop": not all(leads.values()),
            "few_self_line_rows": [lin for lin in LINEAGES if 0 < leads[lin] < FEW_LINE_ROWS],
            "other_leads_self_separated": {lin: total(lin, "self_offers", "other_leads_self_separated")
                                           for lin in LINEAGES},
            "giant_leads_rival_separated": {lin: total(lin, "giant_offers", "giant_leads_rival_separated")
                                            for lin in LINEAGES}}


def census(spec: dict, out_dir: Path) -> dict:
    """SPEC: {"stage": 0 | 2,
              "banks": [{"lineage": LIN, "bank": PATH, "bank_sha256": SHA}, ...],
              "verdicts": {"path": REPLAYS.json, "sha256": SHA},
              "path_logs": {"LIN/GAME": DIR, ...}}
    Stage 0 reads TA1's REPLAYS.json and its retained logs; stage 2 reads
    BT1's REPLAYS.json, which lists every kept bank game. Every log read must
    hash as its verdict records. A game whose verdict is not exact, or whose
    join fails, is void; more than one void game in twenty stops the census as
    engineering before it writes anything. Writes ROWS-{lineage}.jsonl and
    CENSUS.json, and at stage 2 KEEP-{T,P}-{lineage}.json."""
    stage = spec["stage"]
    require(stage in (0, 2), "the census is stage 0 or stage 2")
    live = stage == 2
    out_dir = Path(out_dir).resolve()
    bank_by = banks(spec)
    verdicts = read_pinned(spec["verdicts"], "verdicts")["replays"]
    joined, games, voids = {}, 0, 0
    for lineage in LINEAGES:
        kept = kept_games(bank_by[lineage])
        mine = [v for v in verdicts if v["lineage"] == lineage]
        ids = [v["id"] for v in mine]
        require(len(set(ids)) == len(ids) and set(ids) <= kept, f"{lineage}: a verdict repeats or names no kept game")
        if live:
            require(set(ids) == kept, f"{lineage}: stage 2 lists every kept bank game")
        rows_by_game: dict = {}
        for r in bank_by[lineage]["rows"]:
            rows_by_game.setdefault(r["game"], {})[r["step"]] = r
        rows, void = [], []
        for v in sorted(mine, key=lambda v: v["id"]):
            if not v["exact"]:
                void.append({"id": v["id"], "reason": "the replay is not exact"})
                continue
            d = Path(spec["path_logs"][f"{lineage}/{v['id']}"])
            require(log_hashes(d) == v["path_logs_sha256"],
                    f"{lineage} {v['id']}: the path logs differ from the verdict")
            try:
                rows += game_rows(lineage, v["id"], rows_by_game.get(v["id"], {}), d, live)
            except Void as e:
                void.append({"id": v["id"], "reason": str(e)})
        games, voids = games + len(mine), voids + len(void)
        joined[lineage] = (mine, rows, void, rows_by_game)
    require(voids * VOID_RATIO <= games, f"engineering: {voids} of {games} games are void")
    out_dir.mkdir(parents=True, exist_ok=True)
    result = {"schema": CENSUS_SCHEMA, "stage": stage, "verdicts": spec["verdicts"],
              "banks": {e["lineage"]: {"path": e["bank"], "sha256": e["bank_sha256"]} for e in spec["banks"]},
              "lineages": {}}
    for lineage in LINEAGES:
        mine, rows, void, rows_by_game = joined[lineage]
        bank = bank_by[lineage]
        rows_path = out_dir / f"ROWS-{lineage}.jsonl"
        with rows_path.open("w", encoding="utf-8", newline="\n") as f:
            for r in rows:
                f.write(json.dumps(r, separators=(",", ":")) + "\n")
        entry = {"games": len(mine) - len(void), "void_games": void,
                 "void_rows": sum(len(rows_by_game.get(g["id"], {})) for g in void), "rows_file": pin(rows_path),
                 "rules": {name: summarize(rows, name) for name in RULES}}
        if live:
            bank_rows = {(r["game"], r["step"]): r for r in bank["rows"]}
            ev = [r for r in rows if r["rules"][PRIMARY]["evidence"]]
            measures = {(r["game"], r["step"]): target_measures(r, bank_rows[(r["game"], r["step"])]) for r in ev}
            entry["targets"] = target_summary(rows, measures)
            order = {key: i for i, key in enumerate(bank_rows)}
            ev.sort(key=lambda r: order[(r["game"], r["step"])])
            entry["keep_sets"] = {}
            for cell in CELLS:
                value = {"schema": KEEP_SETS_SCHEMA, "lineage": lineage, "bank_rows_sha256": bank["rows_sha256"],
                         "rows": [{"game": r["game"], "step": r["step"],
                                   "keep": measures[(r["game"], r["step"])]["cells"][cell]["keep"]} for r in ev]}
                entry["keep_sets"][cell] = write_json(out_dir / f"KEEP-{cell}-{lineage}.json", value)
        result["lineages"][lineage] = entry
    result["stop_rule"] = stop_rule(result["lineages"])
    write_json(out_dir / "CENSUS.json", result)
    return result


# -------------------------------------------------------------- replays


def without_live(actual: dict) -> dict:
    """An actual-v2 log as the actual-v1 recorder writes it, in its order."""
    out = {k: v for k, v in actual.items() if k != "hit_live"}
    out["schema"] = ACTUAL_V1
    return out


def same_fields(a, b) -> bool:
    """Equal values, with every object's keys in the same order."""
    return json.dumps(a, separators=(",", ":")) == json.dumps(b, separators=(",", ":"))


def replays(spec: dict) -> dict:
    """SPEC: {"banks": [...as for census...],
              "checks": PATH (JSONL: one TA1 check-game output per replayed game),
              "path_logs": {"LIN/GAME": DIR, ...},
              "ta1": {"verdicts": {"path": TA1 REPLAYS.json, "sha256": SHA},
                      "path_logs": {"LIN/GAME": DIR, ...}}}
    Every kept bank game appears once. An exact game's logs must hash as its
    check-game output records. For each game TA1 replayed, an exact replay's
    search logs must equal TA1's file for file, and its actual log must equal
    TA1's, field for field and in order, once the live-index field is removed;
    any difference stops stage 1. A game that is not exact is listed as void,
    and more than one void game in twenty stops stage 1 as engineering."""
    bank_by = banks(spec)
    lines = Path(spec["checks"]).read_text(encoding="utf-8").splitlines()
    checks = [json.loads(line) for line in lines if line.strip()]
    ta1 = {(v["lineage"], v["id"]): v for v in read_pinned(spec["ta1"]["verdicts"], "TA1 verdicts")["replays"]}
    out, seen, identity = [], set(), {"games": 0, "search_logs": 0}
    for c in checks:
        key = (c["lineage"], c["id"])
        require(key not in seen and c["lineage"] in LINEAGES and c["id"] in kept_games(bank_by[c["lineage"]]),
                f"{key}: repeated, or not a kept bank game")
        seen.add(key)
        verdict = {k: c[k] for k in ("lineage", "id", "rows", "rows_sha256", "exact", "first_difference")}
        if c["exact"]:
            d = Path(spec["path_logs"][f"{c['lineage']}/{c['id']}"])
            hashes = log_hashes(d)
            require(hashes == c["path_logs_sha256"], f"{key}: the path logs differ from the check's record")
            verdict["path_logs_sha256"] = hashes
            if key in ta1:
                old = ta1[key]
                require(old["exact"], f"{key}: TA1's replay was not exact")
                old_d = Path(spec["ta1"]["path_logs"][f"{c['lineage']}/{c['id']}"])
                require(log_hashes(old_d) == old["path_logs_sha256"],
                        f"{key}: TA1's retained logs differ from its record")
                searches = {k: v for k, v in hashes.items() if ".search.json" in k}
                require(searches == {k: v for k, v in old["path_logs_sha256"].items() if ".search.json" in k},
                        f"{key}: a search log differs from TA1's")
                new_actual, old_actual = logs(d, "actual"), logs(old_d, "actual")
                require(len(new_actual) == len(old_actual) == 1, f"{key}: not one actual log each")
                require(same_fields(without_live(load_log(new_actual[0])), load_log(old_actual[0])),
                        f"{key}: the actual log differs from TA1's beyond the live-index field")
                identity["games"] += 1
                identity["search_logs"] += len(searches)
        out.append(verdict)
    for lineage in LINEAGES:
        missing = kept_games(bank_by[lineage]) - {i for lin, i in seen if lin == lineage}
        require(not missing, f"{lineage}: kept games with no check: {sorted(missing)[:5]}")
    require(identity["games"] == sum(1 for key in ta1 if key in seen and next(
        v["exact"] for v in out if (v["lineage"], v["id"]) == key)), "a TA1 game was not compared")
    void = [{"lineage": v["lineage"], "id": v["id"], "first_difference": v["first_difference"]}
            for v in out if not v["exact"]]
    require(len(void) * VOID_RATIO <= len(out), f"engineering: {len(void)} of {len(out)} replays are not exact")
    return {"schema": REPLAYS_SCHEMA, "replays": sorted(out, key=lambda v: (v["lineage"], v["id"])),
            "void": void, "ta1_identity": identity,
            "games": {lin: sum(1 for v in out if v["lineage"] == lin) for lin in LINEAGES}}


# ------------------------------------------------------------- commands

BASE_STEPS = cells.BASE_STEPS
GATED_FROM = cells.GATED_FROM


def cell_options(census_record: dict, lineage: str, cell: str) -> dict:
    """A cell's phase-1 options (section 5): its label, its keep-set file
    from the stage 2 census and the shared checkpoint steps. Nothing else
    changes from the screen's arm U recipe."""
    require(census_record.get("schema") == CENSUS_SCHEMA and census_record["stage"] == 2, "not the stage 2 census")
    keep = census_record["lineages"][lineage]["keep_sets"][cell]
    pinned(keep, f"{lineage}-{cell} keep sets")
    return {"cell": CELL_LABELS[cell], "keep_sets": keep, "checkpoint_steps": list(BASE_STEPS)}


def same_rows(census_record: dict, lineage: str) -> list:
    """The rows both cells' keep-set files list, which must be the same."""
    rows = {cell: [(r["game"], r["step"]) for r in read_pinned(census_record["lineages"][lineage]["keep_sets"][cell],
                                                               f"{lineage}-{cell} keep sets")["rows"]]
            for cell in CELLS}
    require(rows["T"] == rows["P"], f"{lineage}: T and P list different rows")
    return rows["T"]


def commands(lineage: str, parent: dict, games: Path, bank: Path, anchor: dict, census_record: dict,
             root: str) -> dict:
    """Each cell's dry-run, full-run and rerun phase-1 command, on the bank
    the census read, once both cells' files list the same rows."""
    require(census_record.get("schema") == CENSUS_SCHEMA and census_record["stage"] == 2, "not the stage 2 census")
    require(sha256_file(bank) == census_record["banks"][lineage]["sha256"], f"{lineage}: not the census's bank")
    same_rows(census_record, lineage)
    out = {}
    for cell in CELLS:
        options, label = cell_options(census_record, lineage, cell), CELL_LABELS[cell]
        base = f"{root.rstrip('/')}/{lineage}-{label}"
        for suffix, extra in (("-dry", {"dry_run": True}), ("", {}), ("-rerun", {"rerun": True})):
            out[label + suffix] = phase1.phase1_command(lineage, "U", parent, games, bank, anchor, base + suffix,
                                                       options=options, **extra)
    return out


# ------------------------------------------------------------------- fit

NO_SELF_LINE_ROW = (
    "No self-target line row: at no self-target offer in this lineage did self-target lead with the opponent "
    "target separated",
    ["E's tries at those offers may be too few to separate the two targets, or self-target may be untried there "
     "(TA1: at most declines settled by the seeded order, self-target was never tried)",
     "the search's own evidence may favour the opponent target at these offers (see the census's anti-line rows)",
     "one-sided 95% bounds may be too strict for these counts"])
FIT_WITH_COLLAPSE = (
    "Required fit reached only together with collapse, at this dose, optimizer, targets and anchor",
    ["the targets at the non-line evidence rows may move shared parameters",
     "the anchor may be too weak for these targets", "the step size may be too large"])
COLLAPSE_FIRST = {
    "T": ("Collapse before the required fit, at this dose, optimizer, targets and anchor",
          ["the targets at the non-line evidence rows may move shared parameters before the line rows fit",
           "the step size may be too large", "the anchor may be too weak"]),
    "P": ("Collapse before the required fit, at this dose, optimizer, targets and anchor",
          ["the targets at the non-line evidence rows may move shared parameters before the line rows fit",
           "one-hot targets may flatten the policy", "the step size may be too large",
           "the anchor may be too weak"])}
ANTI_LINE = ("the cell's anti-line rows train the opponent target at offers like its self line rows "
             "(see target discrimination)")
NOT_FIT = {
    "T": ("Required fit not reached at this dose, optimizer, targets and anchor",
          ["the line rows may be too few among the cell's evidence rows at this dose",
           "the anchor may outweigh them",
           "the student's input may not separate the line rows from similar rows",
           "at Giant rows, T may keep much of the parent's mass on untried targets (see the census's target mass)",
           ANTI_LINE]),
    "P": ("Required fit not reached at this dose, optimizer, targets and anchor",
          ["the line rows may be too few among the cell's evidence rows at this dose",
           "the anchor may outweigh them",
           "the student's input may not separate the line rows from similar rows", ANTI_LINE])}
FEW_ROWS = ("a pass on so few self line rows is in-sample fit on those rows; the held-out learner-offer "
            "probability reports what generalized")
PASSES = "Passes"


def line_rows(dry: dict) -> tuple:
    """A cell's self and Giant line rows (its evidence rows whose target puts
    at least 0.5 on the menu's self-target, or Giant, actions) from the
    engine's own dry-run census, with the mean target mass on each."""
    rows = dry["census"]["keep_rows"]
    keys, masses = {}, {}
    for name in ("self", "giant"):
        hits = [r for r in rows if r[f"target_{name}_mass"] is not None and r[f"target_{name}_mass"] >= LINE_ROW_FLOOR]
        keys[name] = sorted((r["game"], r["step"]) for r in hits)
        masses[name] = mean([r[f"target_{name}_mass"] for r in hits])
    return keys, masses


def deployed(readout: dict, keys, name: str):
    """The mean deployed self-target (or Giant) probability over these rows."""
    probe = {(r["game"], r["step"]): r for r in readout["archive"]["training"].get("line_rows") or []}
    require(all(tuple(k) in probe for k in keys), f"a {name} line row is not among the probe's line rows")
    return mean([probe[tuple(k)][f"{name}_deployed_probability"] for k in keys])


def g1_prime(readout: dict, lines: dict, masses: dict) -> dict:
    """Mean deployed self-target probability over the self line rows, and the
    same for Giant over the Giant line rows when the cell has any, with the
    halves read, their row counts and their mean target masses."""
    out = {}
    for name in ("self", "giant"):
        keys = lines[name]
        value = deployed(readout, keys, name)
        out[name] = {"rows": len(keys), "mean_target_mass": masses[name], "mean_deployed_probability": value,
                     "pass": None if not keys else (screen.number(value) and value >= LINE_ROW_FLOOR)}
    out["halves"] = [name for name in ("self", "giant") if lines[name]]
    out["pass"] = bool(out["self"]["pass"]) and out["giant"]["pass"] is not False
    return out


def discrimination(readout: dict, lines: dict, anti: list):
    """BT1's target discrimination: the mean deployed self-target probability
    over the self line rows minus that over the anti-line rows."""
    if not lines["self"] or not anti:
        return None
    return deployed(readout, lines["self"], "self") - deployed(readout, anti, "self")


def cell_fit(lineage: str, cell: str, readouts: dict, keep_pin: dict, targets: dict, dry: dict, full: dict,
             rerun: dict) -> dict:
    label = CELL_LABELS[cell]
    for record, what in ((full, "full"), (dry, "dry")):
        require(record.get("schema") == screen.PHASE1_SCHEMA and record["identity"]["lineage"] == lineage
                and record["identity"]["arm"] == "uniform",
                f"{lineage}-{cell}: the {what} run is not this lineage's arm U")
    require(full.get("complete") is True and full.get("stop_after_steps") is None, f"{lineage}-{cell}: not a full run")
    require(dry.get("dry_run") is True and dry["identity"] == full["identity"],
            f"{lineage}-{cell}: the dry run's identity differs from the full run's")
    options = full["identity"]["recipe"].get("options") or {}
    require(set(options) == {"cell", "keep_sets", "checkpoint_steps"} and options["cell"] == label
            and options["keep_sets"]["sha256"] == keep_pin["sha256"] and options["checkpoint_steps"] == list(BASE_STEPS)
            and "update_scope" not in full["identity"]["recipe"],
            f"{lineage}-{cell}: the run's options differ from the design's")
    keep = read_pinned(keep_pin, f"{lineage}-{cell} keep sets")
    require([(r["game"], r["step"], r["keep"]) for r in dry["census"]["keep_rows"]]
            == [(r["game"], r["step"], r["keep"]) for r in keep["rows"]],
            f"{lineage}-{cell}: the dry run's keep rows differ from the keep-set file")
    require(full["census"]["arm"]["rows"] == len(keep["rows"]), f"{lineage}-{cell}: the run trains other rows")
    require(full["census"]["exposure"] == dry["census"]["exposure"],
            f"{lineage}-{cell}: the full run's exposure differs from the dry run's")
    checkpoints = full["checkpoints"]
    require([c["step"] for c in checkpoints] == list(BASE_STEPS),
            f"{lineage}-{cell}: checkpoints differ from the schedule")
    lines, masses = line_rows(dry)
    census_lines = targets["line_rows"]
    for name in ("self", "giant"):
        require(sorted(map(tuple, census_lines[name]["keys"])) == lines[name],
                f"{lineage}-{cell}: the census's {name} line rows differ from the engine's")
    anti = sorted(map(tuple, census_lines["anti"]["keys"]))
    parent, o_final = readouts["parent"], readouts["O-final"]
    telemetry = read_json(full["telemetry"]["path"])
    repro = screen.reproduced(full, rerun)
    gated = [c for c in checkpoints if c["step"] >= GATED_FROM]
    descriptive, rows, stop = [], [], None
    for c in checkpoints:
        cid = f"{label}-step-{c['step']}"
        readout = readouts.get(cid)
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{lineage}-{cell}: the probe did not read checkpoint step {c['step']}")
        if c["step"] >= GATED_FROM and stop is not None:
            continue
        training = readout["archive"]["training"]
        row = {"step": c["step"], "candidate": cid, "checkpoint": c["checkpoint"], "source": c["source"],
               "state_sha256": c["state_sha256"], "G1'": g1_prime(readout, lines, masses),
               "G2": screen.g2(readout, parent, o_final),
               "target_discrimination": discrimination(readout, lines, anti),
               "bank_rows": training["groups"].get("all"), "discrimination": training["discrimination"],
               "learner_offers": {k: readout["learner_offers"][k] for k in
                                  ("offers", "mean_deployed_self_probability", "mean_raw_self_probability",
                                   "self_top_share")},
               "completion": readout["archive"].get("completion")}
        if c["step"] < GATED_FROM:
            descriptive.append(cells.ungated(row))
            continue
        finite = row["G2"]["finite"] and screen.telemetry_finite(telemetry, c["step"])
        row["G3"] = {"finite": finite, "reproduced": repro, "pass": finite and repro}
        row["pass"] = row["G1'"]["pass"] and row["G2"]["pass"] and row["G3"]["pass"]
        rows.append(row)
        if not (row["G2"]["pass"] and row["G3"]["pass"]):
            stop = c["step"]
    passing = [r for r in rows if r["pass"]]
    g1_steps = [r["step"] for r in rows if r["G1'"]["pass"]]
    selected = passing[-1] if passing else None
    out = {"lineage": lineage, "cell": cell, "label": label, "keep_sets": keep_pin, "evidence_rows": len(keep["rows"]),
           "line_rows": {"self": len(lines["self"]), "giant": len(lines["giant"]), "anti": len(anti)},
           "line_row_masses": {"self": masses["self"], "giant": masses["giant"],
                               "anti": census_lines["anti"]["mean_target_mass"]},
           "line_row_keys": {"self": [list(k) for k in lines["self"]], "giant": [list(k) for k in lines["giant"]],
                             "anti": [list(k) for k in anti]},
           "gated_checkpoints": [c["step"] for c in gated], "descriptive": descriptive, "read": rows,
           "stopped_at": stop, "unread_checkpoints": [c["step"] for c in gated[len(rows):]],
           "selected": None if selected is None else {k: selected[k] for k in
                                                        ("step", "candidate", "checkpoint", "source", "state_sha256",
                                                         "learner_offers", "completion")},
           "fit_dose": g1_steps[0] if g1_steps else None}
    out["reading"], out["explanations"] = fit_reading(cell, lines, rows, gated, selected, out["fit_dose"])
    if not lines["self"]:
        out["g2_beside"] = [{"step": r["step"], "G2": r["G2"]} for r in rows]
    return out


def fit_reading(cell: str, lines: dict, rows: list, gated: list, selected, fit_dose) -> tuple:
    """Section 7.2's first matching row, with its competing explanations; on
    fewer than three self line rows, the reading names their count (and a
    pass says it is in-sample fit)."""
    n = len(lines["self"])
    if not n:
        return NO_SELF_LINE_ROW
    if selected is not None:
        reading = f"{PASSES}: selected step {selected['step']}, fit dose {fit_dose}"
        explanations = [FEW_ROWS] if n < FEW_LINE_ROWS else []
    elif any(not r["G3"]["pass"] for r in rows):
        return screen.ENGINEERING, []
    else:
        g2_fail = next((i for i, r in enumerate(rows) if not r["G2"]["pass"]), None)
        g1_first = next((i for i, r in enumerate(rows) if r["G1'"]["pass"]), None)
        if g2_fail is not None and g1_first == g2_fail:
            reading, explanations = FIT_WITH_COLLAPSE
        elif g2_fail is not None and (g1_first is None or g1_first > g2_fail):
            reading, explanations = COLLAPSE_FIRST[cell]
        elif g2_fail is None and g1_first is None and len(rows) == len(gated):
            reading, explanations = NOT_FIT[cell]
        else:
            return screen.NO_MATCH, []
    if n < FEW_LINE_ROWS:
        reading = f"{reading}, on {n} self line row{'' if n == 1 else 's'}"
    return reading, list(explanations)


ORDER_WORDING = {
    cells.LATER: "On the same rows, the truncated targets held G2 longer than the targets at the best line's move, "
                 "at this dose, optimizer and anchor",
    cells.EARLIER: "On the same rows, the targets at the best line's move held G2 longer than the truncated targets, "
                   "at this dose, optimizer and anchor",
    cells.NEITHER: "Neither target collapsed on these rows by 8,192 steps",
    cells.SAME: "Both collapsed in the same checkpoint interval"}
ORDER_EXPLANATIONS = [
    "T moves less probability mass than P on the same rows (the census's summed KL), so the size of the push, "
    "not its form, may explain the ordering",
    "the targets differ only at rows where some moves were left unresolved or untried, which P empties and T keeps",
    "the cells share one batch order; another order could change when the gates fail"]


def t_against_p(fits: dict) -> dict:
    """Section 7.3: T against P per lineage over the gated checkpoints both
    read; the ordering of cells.order (T relative to P)."""
    per = {}
    for lineage, by_cell in fits.items():
        if set(CELLS) <= set(by_cell):
            t, p = by_cell["T"], by_cell["P"]
            ordering = cells.order(t, p)
            per[lineage] = {"ordering": ordering, "wording": ORDER_WORDING.get(ordering),
                            "t_collapse_step": cells.collapse_step(t), "p_collapse_step": cells.collapse_step(p),
                            "g2_at_shared_steps": cells.g2_side_by_side(t, p)}
    return {"per_lineage": per, "across_lineages": cells.across(per), "explanations": ORDER_EXPLANATIONS}


def cross_cell(readouts: dict, t: dict, p: dict) -> list:
    """Section 7.5, at every gated checkpoint both runs read: each cell's
    deployed self and Giant probability over the other cell's line rows, and
    both cells on their shared self line rows, where their targets are
    equal."""
    keys = lambda f, name: [tuple(k) for k in f["line_row_keys"][name]]  # noqa: E731
    shared = sorted(set(keys(t, "self")) & set(keys(p, "self")))
    out = []
    for step in sorted({r["step"] for r in t["read"]} & {r["step"] for r in p["read"]}):
        at = {cell: readouts[f"{CELL_LABELS[cell]}-step-{step}"] for cell in CELLS}
        out.append({"step": step,
                    "T_on_P_line_rows": {name: deployed(at["T"], keys(p, name), name) for name in ("self", "giant")},
                    "P_on_T_line_rows": {name: deployed(at["P"], keys(t, name), name) for name in ("self", "giant")},
                    "shared_self_line_rows": len(shared),
                    "on_shared_self_line_rows": {cell: deployed(at[cell], shared, "self") for cell in CELLS}})
    return out


def fit(spec: dict) -> dict:
    """SPEC: {"screen_fit": SCREEN-FIT.json, "census": CENSUS.json (stage 2),
              "r1": {"probes": [PROBE.json, ...],
                     "cells": {"T": {"dry": DRY.json, "full": PHASE1.json, "rerun": PHASE1.json},
                               "P": {...}}},
              "r2": {...}}
    The probes must read the parent and O-final exactly as the screen did (G2's
    references), and every checkpoint of each cell."""
    screen_fit = read_json(spec["screen_fit"])
    census_record = read_json(spec["census"])
    require(census_record.get("schema") == CENSUS_SCHEMA and census_record["stage"] == 2, "not the stage 2 census")
    fits, references, beside = {}, {}, {}
    for lineage in LINEAGES:
        if lineage not in spec:
            continue
        readouts = cells.merged_readouts(spec[lineage]["probes"], lineage)
        references[lineage] = cells.references_match(readouts, screen_fit, lineage)
        require(references[lineage]["equal"],
                f"engineering: {lineage}: the probes' G2 references differ from the screen's")
        record = census_record["lineages"][lineage]
        if set(CELLS) <= set(spec[lineage]["cells"]):
            # Same rows, so the same batches: the cells differ only in their targets.
            same_rows(census_record, lineage)
            dry = {cell: read_json(spec[lineage]["cells"][cell]["dry"])["census"] for cell in CELLS}
            require(all(dry["T"][k] == dry["P"][k] for k in ("arm", "exposure")),
                    f"{lineage}: T and P differ in rows or exposure")
        fits[lineage] = {cell: cell_fit(lineage, cell, readouts, record["keep_sets"][cell],
                                        record["targets"]["cells"][cell], read_json(run["dry"]),
                                        read_json(run["full"]), read_json(run["rerun"]))
                         for cell, run in sorted(spec[lineage]["cells"].items())}
        f = fits[lineage]
        beside[lineage] = {
            "L_line_rows": screen_fit["fits"][lineage]["L"]["read"][0]["training_rows"]["rows"],
            "L_learner_offer_probability": (screen_fit["fits"][lineage]["L"]["selected"] or {}).get(
                "learner_offers", {}).get("mean_deployed_self_probability"),
            "cells": {cell: {"line_rows": v["line_rows"], "line_row_masses": v["line_row_masses"],
                             "learner_offer_probability": v["selected"] and
                             v["selected"]["learner_offers"]["mean_deployed_self_probability"]}
                      for cell, v in f.items()},
            "cross_cell": cross_cell(readouts, f["T"], f["P"]) if set(CELLS) <= set(f) else None}
    passes = {cell: [lin for lin in fits if cell in fits[lin] and fits[lin][cell]["selected"]] for cell in CELLS}
    return {"schema": FIT_SCHEMA, "g2_references": references, "fits": fits, "passes": passes,
            "fit_check": {cell: ("passes in both lineages" if len(v) == len(LINEAGES) else
                                 "lineage-dependent fit" if v else "passes in neither lineage")
                          for cell, v in passes.items()},
            "t_against_p": t_against_p(fits), "beside": beside}


# ---------------------------------------------------------------- panels

ROLES = ("parent", "D1L", "D1T", "D1P")
CONTROLS = ("parent", "D1L")
READ = (("D1T-parent", "D1T", "parent"), ("D1P-parent", "D1P", "parent"))
DESCRIPTIVE = (("D1T-D1L", "D1T", "D1L"), ("D1P-D1L", "D1P", "D1L"), ("D1T-D1P", "D1T", "D1P"))
X4_CONTRAST = ("D1L-parent", "D1L", "parent")
METRICS = analyze.METRICS + analyze.EXTRA_METRICS
DESCRIPTIVE_METRICS = ("J", "spy_score")
IDENTITY_PANELS = ("r1-parent", "r2-D1L")
LINE_READINGS = {2: "behavioral transfer established at D1", 1: "lineage-dependent", 0: "not established"}
NOT_STRENGTH = ("A positive J result is behavioral transfer: the policy wins through the line more often. It is not a "
                "playing-strength claim, because more winning Spy sequences can mean that wins the parent already "
                "had took the line's route.")
NO_GIANT_LINE_ROW = ("the cell has no Giant line row, so J may fail at the line's second step, which the parent "
                     "chose in few offered panel games")


def statistics(index: dict, contrasts) -> tuple:
    """The screen's analyzer over these panels and contrasts, with the
    screen's own bootstrap seed: one draw of repeat indices per draw, shared
    by every panel, so added panels leave the screen's contrasts as they
    were."""
    allowed = {f"{lin}-{role}" for lin in LINEAGES for role in ROLES}
    data = analyze.load(index, allowed)
    todo = analyze.plan(data, contrasts)
    return data, todo, analyze.bootstrap(data, todo, METRICS), analyze.points(data, todo, METRICS)


def x4(ci: dict, pts: dict, screen_result: dict) -> dict:
    """The screen's D1L-parent points and intervals for every panel metric,
    against T1, against H and pooled, under both bounds, in both lineages,
    reproduced exactly."""
    keys = [(lin, X4_CONTRAST[0], metric, opp, bound) for lin in LINEAGES for metric in METRICS
            for opp in (*analyze.OPPONENTS, "pooled") for bound in analyze.BOUNDS]
    differ = ["/".join(key) for key in keys
              if key not in ci or screen_result["contrasts"].get("/".join(key)) != {"point": pts[key], "ci95": ci[key]}]
    return {"compared": len(keys), "differing": differ, "pass": not differ}


def controls_of(index: dict) -> list:
    return [p for p in index["panels"] if p["checkpoint"].split("-", 1)[1] in CONTROLS]


def screen_x4(screen_index: Path, screen_result: Path) -> dict:
    """X4 from the screen's own parent and D1L panels alone, read before any
    BT1 panel."""
    result = read_json(screen_result)
    require(result.get("seed") == analyze.BOOT_SEED and result.get("draws") == analyze.DRAWS,
            "the screen's result used another seed or draw count")
    _, _, ci, pts = statistics({"panels": controls_of(read_json(screen_index))}, (X4_CONTRAST,))
    return {"schema": X4_SCHEMA, "screen_index": pin(Path(screen_index).resolve()),
            "screen_result": pin(Path(screen_result).resolve()), "seed": analyze.BOOT_SEED,
            **x4(ci, pts, result)}


def identity(spec: dict, screen_index: dict) -> dict:
    """Section 6: the screen's parent and D1L panels stand when BT1's
    evaluator reproduces r1-parent and r2-D1L against T1 byte for byte, and
    against the rebound H apart from its provenance hash (labels byte for
    byte; rows byte for byte once the recorded envelope hash is restored, the
    screen's H identity check)."""
    record = read_pinned(spec["panel_identity"], "panel identity")
    h = read_pinned(spec["h_identity"], "H identity")
    recorded = {(p["checkpoint"], p["opponent"]): p for p in screen_index["panels"]}
    pairs = {}
    for ck in IDENTITY_PANELS:
        for opp in analyze.OPPONENTS:
            name = f"{ck}-vs-{opp}"
            pair, panel = record["pairs"][name], recorded[(ck, opp)]
            require(pair["rows.jsonl"]["recorded"] == sha256_file(panel["rows"])
                    and pair["labels.jsonl"]["recorded"] == sha256_file(panel["labels"]),
                    f"{name}: the identity record does not compare the screen's panel")
            labels = pair["labels.jsonl"]["new"] == pair["labels.jsonl"]["recorded"]
            if opp == "T1":
                pairs[name] = labels and pair["rows.jsonl"]["new"] == pair["rows.jsonl"]["recorded"]
            else:
                check = h["pairs"].get(name) or {}
                pairs[name] = (labels and h.get("passed") is True and check.get("rows_bytes_equal") is True
                               and check.get("failure_count") == 0
                               and check.get("restored_rows_sha256") == pair["rows.jsonl"]["recorded"])
    return {"pairs": pairs, "holds": all(pairs.values())}


def panel_index(spec: dict, fitted: dict, screen_index: dict, holds: bool) -> dict:
    """The control panels (the screen's parent and D1L panels when identity
    holds, else the reruns on BT1's evaluator, one per lineage, role and
    opponent) and BT1's D1 panels. A D1 panel must name its cell's selected
    checkpoint file."""
    if holds:
        controls = controls_of(screen_index)
    else:
        controls = [e for entries in spec.get("controls", {}).values() for e in entries]
        names = sorted((e["checkpoint"], e["opponent"]) for e in controls)
        require(names == sorted((f"{lin}-{role}", opp) for lin in LINEAGES for role in CONTROLS
                                for opp in analyze.OPPONENTS),
                "identity failed: every parent and D1L panel must be rerun on BT1's evaluator")
    new = []
    for lineage, entries in spec.get("entries", {}).items():
        for e in entries:
            role = e["checkpoint"].split("-", 1)[1]
            cell = role[-1]
            require(e["checkpoint"].split("-", 1)[0] == lineage and role in ("D1T", "D1P"),
                    f"{e['checkpoint']}: not a BT1 endpoint")
            selected = fitted["fits"][lineage][cell]["selected"]
            require(selected is not None and e["checkpoint_sha256"] == selected["checkpoint"]["sha256"],
                    f"{e['checkpoint']}: not the cell's selected checkpoint")
            new.append(e)
    return {"panels": controls + new}


def panels(spec: dict, fitted: dict) -> dict:
    """SPEC: {"screen_index": {"path": INDEX.json, "sha256": SHA} (the screen's panel index),
              "screen_result": {"path": RESULT.json, "sha256": SHA} (the screen's analysis),
              "x4": {"path": X4.json, "sha256": SHA} (this tool's x4 output, which must pass),
              "identity": {"panel_identity": PIN, "h_identity": PIN},
              "controls": {"r1": [entries], ...} (only when identity fails),
              "entries": {"r1": [{"checkpoint": "r1-D1T", "opponent": "T1" | "H",
                                  "checkpoint_sha256": SHA, "rows": PATH, "labels": PATH}, ...], ...}}
    The screen's analyzer reads every panel (checks, metrics, missing-case
    bounds, cluster bootstrap of 10,000 draws with the screen's seed). When
    the screen's panels stand, X4 is checked again in the same bootstrap."""
    screen_index = read_pinned(spec["screen_index"], "screen index")
    screen_result = read_pinned(spec["screen_result"], "screen result")
    first = read_pinned(spec["x4"], "X4")
    require(first.get("schema") == X4_SCHEMA and first["pass"] is True
            and first["screen_index"]["sha256"] == spec["screen_index"]["sha256"]
            and first["screen_result"]["sha256"] == spec["screen_result"]["sha256"],
            "X4 did not pass on these screen files before the panels")
    same = identity(spec["identity"], screen_index)
    data, todo, ci, pts = statistics(panel_index(spec, fitted, screen_index, same["holds"]),
                                     READ + DESCRIPTIVE + (X4_CONTRAST,))
    if same["holds"]:
        again = x4(ci, pts, screen_result)
        require(again["pass"], f"X4 failed: {again['differing']}")
    else:
        again = None
    out = {"schema": PANELS_SCHEMA, "seed": analyze.BOOT_SEED, "draws": analyze.DRAWS, "identity": same,
           "x4": {"before": first, "with_panels": again},
           "panels": {f"{ck}/{opp}": analyze.summary(cases) for (ck, opp), cases in sorted(data.items())},
           "missing": {f"{ck}/{opp}": analyze.missing(cases) for (ck, opp), cases in sorted(data.items())},
           "secondary": {f"{ck}/{opp}": analyze.secondary(cases) for (ck, opp), cases in sorted(data.items())},
           "contrasts": {"/".join(k): {"point": pts[k], "ci95": ci[k]} for k in pts},
           "cells": {}, "note": NOT_STRENGTH}
    for cell in CELLS:
        name = f"D1{cell}-parent"
        lineages = [lin for lin in LINEAGES if (lin, name, "J", "pooled", "worst") in ci]
        entry = {"lineages": lineages, "fit_check": fitted["fit_check"][cell], "per_lineage": {}}
        for lin in lineages:
            fitted_cell = fitted["fits"][lin][cell]
            entry["per_lineage"][lin] = {
                "J_transfer_at_D1": analyze.agree({b: analyze.j_above(ci, lin, name, b) for b in analyze.BOUNDS}),
                "guards": analyze.agree({b: analyze.guard_reading(ci, lin, name, b) for b in analyze.BOUNDS}),
                "J": {b: ci[(lin, name, "J", "pooled", b)] for b in analyze.BOUNDS},
                "line_rows": fitted_cell["line_rows"], "line_row_masses": fitted_cell["line_row_masses"],
                "note": NO_GIANT_LINE_ROW if not fitted_cell["line_rows"]["giant"] else None}
        if len(lineages) == len(LINEAGES):
            holds = {b: all(analyze.j_above(ci, lin, name, b) and analyze.guards_hold(ci, lin, name, b)
                            for lin in LINEAGES) for b in analyze.BOUNDS}
            entry["screen_at_D1"] = True if holds["worst"] else analyze.INCONCLUSIVE if holds["best"] else False
            entry["line_reading"] = analyze.agree({b: LINE_READINGS[sum(analyze.j_above(ci, lin, name, b)
                                                                        for lin in LINEAGES)]
                                                   for b in analyze.BOUNDS})
        else:
            entry["screen_at_D1"] = None
            entry["line_reading"] = ("descriptive only: the cell passed the fit check in one lineage" if lineages
                                     else "not run: the cell passed the fit check in neither lineage")
        out["cells"][cell] = entry
    out["win_rates"] = analyze.win_rates(ci, [name for name, _, _ in READ])
    out["descriptive"] = {"/".join((lin, name, metric)): {b: {"point": pts[(lin, name, metric, "pooled", b)],
                                                            "interval": ci[(lin, name, metric, "pooled", b)]}
                                                        for b in analyze.BOUNDS}
                          for lin, name, _, _ in todo if name in {n for n, _, _ in DESCRIPTIVE}
                          for metric in DESCRIPTIVE_METRICS}
    return out


# ------------------------------------------------------------------ main


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    r = sub.add_parser("replays")
    r.add_argument("--spec", type=Path, required=True)
    r.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("census")
    c.add_argument("--spec", type=Path, required=True)
    c.add_argument("--out-dir", type=Path, required=True)
    m = sub.add_parser("commands")
    m.add_argument("--lineage", required=True, choices=LINEAGES)
    for flag in ("--parent", "--games", "--bank", "--anchor", "--census", "--out-dir"):
        m.add_argument(flag, type=Path, required=True)
    m.add_argument("--root", required=True)
    f = sub.add_parser("fit")
    f.add_argument("--spec", type=Path, required=True)
    f.add_argument("--out", type=Path, required=True)
    x = sub.add_parser("x4")
    x.add_argument("--screen-index", type=Path, required=True)
    x.add_argument("--screen-result", type=Path, required=True)
    x.add_argument("--out", type=Path, required=True)
    p = sub.add_parser("panels")
    p.add_argument("--spec", type=Path, required=True)
    p.add_argument("--fit", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "replays":
        result = replays(read_json(a.spec))
        write_json(a.out, result)
        print(json.dumps({"games": result["games"], "void": result["void"], "ta1_identity": result["ta1_identity"]}))
    elif a.op == "census":
        result = census(read_json(a.spec), a.out_dir)
        print(json.dumps({"stage": result["stage"], "stop_rule": result["stop_rule"],
                          "evidence_rows": {lin: v["rules"][PRIMARY]["evidence_rows"]
                                            for lin, v in result["lineages"].items()},
                          "void_games": {lin: len(v["void_games"]) for lin, v in result["lineages"].items()}}))
    elif a.op == "commands":
        written = {name: write_json(a.out_dir / f"{a.lineage}-{name}.command.json", command)
                   for name, command in commands(a.lineage, read_json(a.parent), a.games, a.bank,
                                                 read_json(a.anchor), read_json(a.census), a.root).items()}
        print(json.dumps(written))
    elif a.op == "fit":
        result = fit(read_json(a.spec))
        write_json(a.out, result)
        print(json.dumps({"fit_check": result["fit_check"],
                          "readings": {f"{lin}-{cell}": v["reading"] for lin, by in result["fits"].items()
                                       for cell, v in by.items()},
                          "t_against_p": result["t_against_p"]["across_lineages"]}))
    elif a.op == "x4":
        result = screen_x4(a.screen_index, a.screen_result)
        write_json(a.out, result)
        print(json.dumps({"compared": result["compared"], "pass": result["pass"]}))
        return 0 if result["pass"] else 1
    else:
        result = panels(read_json(a.spec), read_json(a.fit))
        write_json(a.out, result)
        print(json.dumps({cell: {"line_reading": v["line_reading"], "screen_at_D1": v["screen_at_D1"]}
                          for cell, v in result["cells"].items()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
