"""EX1 Stage F readouts (design v3.3 section 6, frozen): the case lists and the statistics. Outputs only: it applies no
success label; the lead reads section 6's rules from its numbers.

  stagef_read.py cases --out CASES.json            the primary Spy case list and the transfer capture list
  stagef_read.py primary --rows ROWS.jsonl         paired McNemar per lineage, chain consistency, combo_won beside,
                                                   failed replays excluded (primary) and bounded (sensitivity)
  stagef_read.py transfer --rows ROWS.jsonl        per cell and lineage permutation p, conjunction max(p_r1, p_r2),
                                                   Holm over the four cells, conditional miss rate with a cluster
                                                   bootstrap, certified share per arm
  stagef_read.py panel --rows ROWS.jsonl           fixed-state panel: mean p(self) per chain, X minus O per pair,
                                                   bootstrap interval over states

Case lists (fixed before any game). Every chain pair (lineage, pair i) has its own seed namespace; X chain i and O
chain i play the same cases; the case structure is the same in every pair.
- Primary: the Spy focal seat (own deck Spy) against each of the 5 panel opponents (Ha, Hb, R3, T1, TP), each of the
  9 opponent decks, both seats, 4 repeats: 360 cases per chain. Namespace `ex1/eval/v1/<lineage>/pair<i>`.
- Transfer: own decks Burn, Rally and Elves, each against the 5 panel opponents, the 8 other decks and both seats
  (repeat 0): 80 cases per deck per chain. Namespace `ex1/eval/v1/transfer/<lineage>/pair<i>`.
- A case's deal seed is the panel convention, seed([namespace, own, other, repeat]) (tools/spy_reset/pilot.py), so
  both seats of a matchup share a deal, and its starting player is (repeat + own + other) mod 2.

Input rows (JSON lines), one per played case and chain:
- primary: {"lineage", "pair", "arm": "x"|"o", "case": CASE_ID, "replay_ok": bool, "combo_resolved": bool,
  "combo_won": bool}
- transfer: {"lineage", "pair", "arm", "case", "cell", "certified_opportunities": int, "missed": int}, where cell is
  one of CELLS and missed counts missed certified opportunities in that game (unconditional, per scheduled game);
  a scheduled game with no certified opportunity has both counts 0
- panel: {"lineage", "pair", "arm", "state": STATE_ID, "p_self": float}
"""
import argparse
import hashlib
import json
import math
import random
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import qual  # noqa: E402

DECKS = ("Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries")
OPPONENTS = ("Ha", "Hb", "R3", "T1", "TP")
LINEAGES = ("r1", "r2")
PAIRS = (1, 2)
SPY = DECKS.index("Spy")
REPEATS = 4
TRANSFER_DECKS = ("Burn", "Rally", "Elves")
CELLS = ("burn_burn_lethal", "burn_combat_lethal", "rally_rally_elves_kill", "elves_rally_elves_kill")
CELL_DECK = {"burn_burn_lethal": "Burn", "burn_combat_lethal": "Burn", "rally_rally_elves_kill": "Rally",
             "elves_rally_elves_kill": "Elves"}
ALPHA = 0.05
PERMUTATIONS = 100_000
BOOTSTRAPS = 10_000


def namespace(lineage, pair, transfer=False):
    return f"ex1/eval/v1/{'transfer/' if transfer else ''}{lineage}/pair{pair}"


def case_id(opponent, own, other, seat, repeat):
    return f"{opponent}-{DECKS[own]}-{DECKS[other]}-s{seat}-k{repeat}"


def case(ns, opponent, own, other, seat, repeat):
    return {"case": case_id(opponent, own, other, seat, repeat), "opponent": opponent, "own": DECKS[own],
            "other": DECKS[other], "seat": seat, "repeat": repeat, "seed": qual.seed([ns, own, other, repeat]),
            "starting_player": (repeat + own + other) % 2}


def primary_cases(lineage, pair):
    ns = namespace(lineage, pair)
    return [case(ns, opp, SPY, other, seat, k) for opp in OPPONENTS for other in range(len(DECKS))
            for seat in (0, 1) for k in range(REPEATS)]


def transfer_cases(lineage, pair):
    ns = namespace(lineage, pair, transfer=True)
    out = []
    for name in TRANSFER_DECKS:
        own = DECKS.index(name)
        out += [case(ns, opp, own, other, seat, 0) for opp in OPPONENTS for other in range(len(DECKS))
                if other != own for seat in (0, 1)]
    return out


def all_cases():
    return {"schema": "ex1-stagef-cases/v1",
            "pairs": {f"{lin}-p{i}": {"primary_namespace": namespace(lin, i),
                                      "transfer_namespace": namespace(lin, i, True),
                                      "primary": primary_cases(lin, i), "transfer": transfer_cases(lin, i)}
                      for lin in LINEAGES for i in PAIRS}}


# ---------------------------------------------------------------- statistics

def mcnemar_one_sided(b, c):
    """P(Binomial(b + c, 1/2) >= b): the exact one-sided McNemar p for X completing more often than O."""
    n = b + c
    if n == 0:
        return 1.0
    return math.fsum(math.comb(n, k) for k in range(b, n + 1)) / 2 ** n


def mc_seed(tag, lineage, cell):
    text = json.dumps([tag, lineage, cell], separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little")


def paired(rows, keys):
    """{(lineage, pair, case): {"x": row, "o": row}}, refusing duplicates and unpaired rows."""
    out = defaultdict(dict)
    for r in rows:
        k = tuple(r[key] for key in keys)
        if r["arm"] in out[k]:
            raise SystemExit(f"stop: duplicate row {k} {r['arm']}")
        out[k][r["arm"]] = r
    missing = [k for k, v in out.items() if set(v) != {"x", "o"}]
    if missing:
        raise SystemExit(f"stop: {len(missing)} cases are not played by both arms, e.g. {missing[0]}")
    return out


def primary(rows, label="combo_resolved"):
    """Section 6 primary: per lineage, discordant case pairs pooled over its two chain pairs; failed replays are
    excluded (primary) and bounded (O counts as completed, X as not); chain consistency per pair."""
    pairs = paired(rows, ("lineage", "pair", "case"))
    out = {}
    for lin in LINEAGES:
        res = {"primary": {"b": 0, "c": 0, "excluded": 0}, "sensitivity": {"b": 0, "c": 0}, "chains": {}}
        for i in PAIRS:
            cx = co = cx_s = co_s = n = 0
            for (l2, p2, _), v in pairs.items():
                if (l2, p2) != (lin, i):
                    continue
                n += 1
                x, o = v["x"], v["o"]
                xs = x[label] if x["replay_ok"] else False
                os_ = o[label] if o["replay_ok"] else True
                res["sensitivity"]["b"] += xs and not os_
                res["sensitivity"]["c"] += os_ and not xs
                cx_s += xs
                co_s += os_
                if not (x["replay_ok"] and o["replay_ok"]):
                    res["primary"]["excluded"] += 1
                    continue
                res["primary"]["b"] += x[label] and not o[label]
                res["primary"]["c"] += o[label] and not x[label]
                cx += x[label]
                co += o[label]
            res["chains"][f"pair{i}"] = {"cases": n, "x_completions": cx, "o_completions": co,
                                         "x_more_than_o": cx > co, "sensitivity_x": cx_s, "sensitivity_o": co_s}
        for key in ("primary", "sensitivity"):
            res[key]["p_one_sided"] = mcnemar_one_sided(res[key]["b"], res[key]["c"])
            res[key]["p_at_most_alpha"] = res[key]["p_one_sided"] <= ALPHA
        res["chain_consistency"] = all(c["x_more_than_o"] for c in res["chains"].values())
        out[lin] = res
    return {"label": label, "alpha": ALPHA, "lineages": out}


def permutation_p(diffs, seed, n=PERMUTATIONS):
    """One-sided: T = sum(O - X) over matched case pairs; each permutation swaps X and O within a pair, flipping
    that pair's sign. p = (k + 1) / (N + 1), k = permutations with T* >= T."""
    t = sum(diffs)
    rng = random.Random(seed)
    nonzero = [d for d in diffs if d]
    k = 0
    for _ in range(n):
        s = 0
        for d in nonzero:
            s += d if rng.random() < 0.5 else -d
        k += s >= t
    return (k + 1) / (n + 1), t


def holm(pvalues, alpha=ALPHA):
    """Holm step-down: {name: rejected}."""
    order = sorted(pvalues, key=lambda name: (pvalues[name], name))
    out, going = {}, True
    for j, name in enumerate(order):
        going = going and pvalues[name] <= alpha / (len(order) - j)
        out[name] = going
    return out


def percentile(values, q):
    values = sorted(values)
    pos = q * (len(values) - 1)
    lo, hi = math.floor(pos), math.ceil(pos)
    return values[lo] + (values[hi] - values[lo]) * (pos - lo)


def conditional_rate(units, arm):
    opp = sum(u[arm]["certified_opportunities"] for u in units)
    return (sum(u[arm]["missed"] for u in units) / opp) if opp else None


def transfer(rows, permutations=PERMUTATIONS, bootstraps=BOOTSTRAPS):
    pairs = paired(rows, ("lineage", "pair", "case", "cell"))
    cells = {}
    for cell in CELLS:
        per = {}
        for lin in LINEAGES:
            units = [v for (l2, _, _, c2), v in sorted(pairs.items()) if l2 == lin and c2 == cell]
            if not units:
                raise SystemExit(f"stop: no rows for {cell} in {lin}")
            diffs = [u["o"]["missed"] - u["x"]["missed"] for u in units]
            p, t = permutation_p(diffs, mc_seed("ex1/perm", lin, cell), permutations)
            rng = random.Random(mc_seed("ex1/boot", lin, cell))
            boot = []
            for _ in range(bootstraps):
                sample = [units[rng.randrange(len(units))] for _ in units]
                o, x = conditional_rate(sample, "o"), conditional_rate(sample, "x")
                if o is not None and x is not None:
                    boot.append(o - x)
            o, x = conditional_rate(units, "o"), conditional_rate(units, "x")
            per[lin] = {"games_per_arm": len(units), "statistic_sum_o_minus_x": t,
                        "missed_per_game": {"o": sum(u["o"]["missed"] for u in units) / len(units),
                                            "x": sum(u["x"]["missed"] for u in units) / len(units)},
                        "p_one_sided": p,
                        "conditional_miss_rate": {"o": o, "x": x, "o_minus_x": None if o is None or x is None
                                                  else o - x,
                                                  "interval_95": [percentile(boot, 0.025), percentile(boot, 0.975)]
                                                  if boot else None, "resamples_defined": len(boot)},
                        "certified_share_per_game": {arm: sum(u[arm]["certified_opportunities"] > 0 for u in units)
                                                     / len(units) for arm in ("o", "x")}}
        cells[cell] = {"lineages": per, "conjunction_p": max(per[lin]["p_one_sided"] for lin in LINEAGES)}
    rejected = holm({c: v["conjunction_p"] for c, v in cells.items()})
    for c in cells:
        cells[c]["holm_rejects"] = rejected[c]
    return {"permutations": permutations, "bootstraps": bootstraps, "alpha": ALPHA, "cells": cells}


def panel(rows, bootstraps=BOOTSTRAPS):
    pairs = paired(rows, ("lineage", "pair", "state"))
    out = {}
    for lin in LINEAGES:
        for i in PAIRS:
            units = [v for (l2, p2, _), v in sorted(pairs.items()) if (l2, p2) == (lin, i)]
            if not units:
                continue
            diffs = [u["x"]["p_self"] - u["o"]["p_self"] for u in units]
            rng = random.Random(mc_seed("ex1/panel-boot", lin, f"pair{i}"))
            boot = [sum(diffs[rng.randrange(len(diffs))] for _ in diffs) / len(diffs) for _ in range(bootstraps)]
            out[f"{lin}-p{i}"] = {"states": len(units),
                                  "mean_p_self": {"x": sum(u["x"]["p_self"] for u in units) / len(units),
                                                  "o": sum(u["o"]["p_self"] for u in units) / len(units)},
                                  "x_minus_o": sum(diffs) / len(diffs),
                                  "interval_95": [percentile(boot, 0.025), percentile(boot, 0.975)]}
    return {"bootstraps": bootstraps, "pairs": out}


def load_rows(path):
    return [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("cases")
    c.add_argument("--out", required=True)
    for name in ("primary", "transfer", "panel"):
        s = sub.add_parser(name)
        s.add_argument("--rows", required=True)
    args = ap.parse_args(argv)
    if args.cmd == "cases":
        Path(args.out).write_text(json.dumps(all_cases(), indent=1) + "\n", encoding="utf-8", newline="\n")
        return
    rows = load_rows(args.rows)
    if args.cmd == "primary":
        out = {"combo_resolved": primary(rows), "combo_won": primary(rows, "combo_won")}
    else:
        out = {"transfer": transfer, "panel": panel}[args.cmd](rows)
    print(json.dumps(out, indent=1, sort_keys=True))


if __name__ == "__main__":
    main()
