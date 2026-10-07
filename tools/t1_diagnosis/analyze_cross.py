#!/usr/bin/env python3
"""Summarize `regret_census_v1 mode=cross` rows (stage 3).

Cells: initial action (T1's sampled root action, or the condition-B search
alternative) x subsequent focal policy (T1, improved through the current
turn, improved through the game). Outcome per root and cell is the share of
paired evaluation playouts that ended in a natural focal win.

Contrasts (paired over roots, 95% bootstrap over roots):
- initial correction only: alt|t1 - t1|t1
- turn-long vs T1 follow-up: X|improved:turn - X|t1, for X in t1, alt
- game-long vs turn-long: X|improved:game - X|improved:turn

Every logged deviation (a focal decision where improved play chose another
action than T1's sample) is classified by keywords into mana allocation,
creature preservation, attacks, Gate targets or colours, prevention, card
selection or other. Each deviation inherits the paired playout result: success
when the improved playout scored above the T1-follow-up playout from the same
initial action and determinization, failure when below, neutral otherwise.

Usage: analyze_cross.py CROSS.jsonl [--boot 2000] [--seed 7] [--examples 3] [--json OUT]
"""

import argparse
import json
import random
from collections import Counter, defaultdict

T1 = "t1"
TURN = "improved:turn"
GAME = "improved:game"
CLASSES = [
    "prevention",
    "gate_targets_colours",
    "mana_allocation",
    "attacks",
    "creature_preservation",
    "card_selection",
    "other",
]


def mean(xs):
    xs = list(xs)
    return sum(xs) / len(xs) if xs else float("nan")


def win(scores):
    return mean(1.0 if s == 1.0 else 0.0 for s in scores)


def boot_ci(vals, n_boot, rng):
    n = len(vals)
    if n == 0:
        return (float("nan"), float("nan"))
    stats = sorted(mean(vals[rng.randrange(n)] for _ in range(n)) for _ in range(n_boot))
    return (stats[int(0.025 * n_boot)], stats[min(n_boot - 1, int(0.975 * n_boot))])


def fmt(x):
    return "   n/a" if x != x else f"{100 * x:6.1f}"


def classify(dev):
    """Keyword classifier over both semantics strings of a deviation."""
    text = f"{dev.get('sampled_sem') or ''} {dev.get('chosen_sem') or ''}"
    kinds = set()
    for s in (dev.get("sampled_sem"), dev.get("chosen_sem")):
        try:
            kinds.add(json.loads(s).get("action_kind", ""))
        except (TypeError, ValueError, AttributeError):
            pass
    if "Prismatic Strands" in text or "prevent" in text.lower():
        return "prevention"
    if "Gate" in text or "choose_effect_color" in kinds:
        return "gate_targets_colours"
    if "activate_mana_ability" in kinds or "mana_choice" in text:
        return "mana_allocation"
    if kinds & {"choose_attacker_inclusion", "declare_attackers"}:
        return "attacks"
    if kinds & {"choose_blocker_inclusion", "declare_blockers_for_attacker", "choose_cost_target"} or "sacrific" in text.lower():
        return "creature_preservation"
    if kinds & {"cast_spell", "play_land", "discard", "choose_target", "choose_effect_target",
                "choose_london_bottom", "activate_ability", "pass"}:
        return "card_selection"
    return "other"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cross")
    ap.add_argument("--boot", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--examples", type=int, default=3)
    ap.add_argument("--json")
    a = ap.parse_args()
    rows, errors = [], []
    with open(a.cross) as f:
        for line in f:
            r = json.loads(line)
            if r.get("kind") == "cross":
                rows.append(r)
            elif r.get("kind") == "error":
                errors.append(r)
    rows.sort(key=lambda r: r["root_index"])
    print("T1 TOOLING-QUALIFICATION RESULTS - not research conclusions (T1 never trained on Spy or CawGates).")
    print(f"rows {len(rows)}, error rows {len(errors)}")
    for e in errors:
        print(f"  error root {e.get('game')}: {str(e.get('error'))[:200]}")
    if not rows:
        return
    out = {"errors": len(errors), "roots": len(rows)}
    cells = rows[0]["cells"]
    per = []
    print(f"\n{'root':<12}{'tag':<22}{'alt forced':>11}" + "".join(f"{c:>20}" for c in cells))
    for r in rows:
        jobs = r["eval"]["jobs"]
        v = {c: win(jobs[i]["scores"]) for i, c in enumerate(cells)}
        per.append(v)
        print(f"{r.get('root_id', r['root_index']):<12}{str(r.get('mechanism_tag', '')):<22}{str(r['alt_forced']):>11}"
              + "".join(f"{fmt(v[c]):>20}" for c in cells))
    print("mean win%   " + " " * 33 + "".join(f"{fmt(mean(v[c] for v in per)):>20}" for c in cells))
    out["cells"] = {c: mean(v[c] for v in per) for c in cells}
    rng = random.Random(a.seed)
    contrasts = [("initial correction only", f"alt|{T1}", f"t1|{T1}")]
    for x in ("t1", "alt"):
        contrasts.append((f"turn-long vs T1 follow-up ({x})", f"{x}|{TURN}", f"{x}|{T1}"))
    for x in ("t1", "alt"):
        contrasts.append((f"game-long vs turn-long ({x})", f"{x}|{GAME}", f"{x}|{TURN}"))
    print("\ncontrasts (pp, 95% bootstrap over roots):")
    out["contrasts"] = {}
    for name, p, q in contrasts:
        d = [v[p] - v[q] for v in per]
        lo, hi = boot_ci(d, a.boot, rng)
        out["contrasts"][name] = {"diff": mean(d), "lo": lo, "hi": hi}
        print(f"  {name:<36} {fmt(mean(d))}  [{fmt(lo)}, {fmt(hi)}]")

    # Deviations, labelled by the paired playout result.
    counts = defaultdict(Counter)
    examples = defaultdict(list)
    per_cell = Counter()
    for r in rows:
        jobs = r["eval"]["jobs"]
        idx = {c: i for i, c in enumerate(cells)}
        for c in cells:
            init, follow = c.split("|", 1)
            if follow == T1:
                continue
            base = jobs[idx[f"{init}|{T1}"]]["scores"]
            mine = jobs[idx[c]]["scores"]
            for dev in jobs[idx[c]]["deviations"]:
                e = dev["playout"]
                delta = mine[e] - base[e] if e < len(mine) and e < len(base) else 0.0
                label = "success" if delta > 0 else "failure" if delta < 0 else "neutral"
                cls = classify(dev)
                counts[cls][label] += 1
                counts[cls]["total"] += 1
                if dev.get("all_equal"):
                    counts[cls]["tie"] += 1
                per_cell[c] += 1
                if len(examples[cls]) < a.examples and not dev.get("all_equal"):
                    examples[cls].append({"root": r.get("root_id"), "cell": c, "turn": dev["turn"],
                                          "phase": dev["phase"], "own_turn": dev.get("own_turn"),
                                          "t1": dev.get("sampled_sem"), "improved": dev.get("chosen_sem"),
                                          "means": dev.get("means"), "result": label})
    print("\ndeviations per cell: " + ", ".join(f"{c}: {n}" for c, n in sorted(per_cell.items())))
    print("(tie = every candidate had the same inner mean, so the higher-probability rule, not the")
    print(" search, made the choice differ from T1's sample)")
    print(f"\n{'class':<24}{'total':>7}{'success':>9}{'failure':>9}{'neutral':>9}{'tie':>7}")
    for cls in CLASSES:
        c = counts[cls]
        print(f"{cls:<24}{c['total']:>7}{c['success']:>9}{c['failure']:>9}{c['neutral']:>9}{c['tie']:>7}")
    for cls in CLASSES:
        for ex in examples[cls]:
            print(f"  [{cls}] {ex['root']} {ex['cell']} t{ex['turn']} {ex['phase']} own={ex['own_turn']} "
                  f"{ex['result']} means={[round(m, 2) for m in ex['means'] or []]}\n"
                  f"      T1:       {str(ex['t1'])[:200]}\n      improved: {str(ex['improved'])[:200]}")
    out["deviations"] = {cls: dict(counts[cls]) for cls in CLASSES}
    out["deviations_per_cell"] = dict(per_cell)
    out["examples"] = {cls: examples[cls] for cls in CLASSES}

    print("\ncost per root:")
    for r in rows:
        jobs = r["eval"]["jobs"]
        parts = []
        for i, c in enumerate(cells):
            t = mean(jobs[i]["transitions"])
            w = mean(jobs[i]["wall"])
            parts.append(f"{c}={t:.0f}tr/{w:.1f}s")
        print(f"  {r.get('root_id')}: sel {r['selection_wall']:.1f}s, eval {r['eval']['actual_wall']:.1f}s, "
              f"failures {r['eval']['failed_playouts']}, inner failures {sum(j['inner_failures'] for j in jobs)}; "
              + ", ".join(parts))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(out, f, indent=1)


if __name__ == "__main__":
    main()
