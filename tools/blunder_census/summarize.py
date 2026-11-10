"""Blunder census tables from `s4a-blunder-census` rows (collab LANES/census-20261110, DEFINITIONS.md). Measurement only.

usage: summarize.py --root DIR --checkpoints b512=0,c1-u0065536=65536,... --json OUT.json --md OUT.md [--png OUT.png]
                    [--cases-md OUT.md]

Each checkpoint's census directory is DIR/<checkpoint>/census with games-bcd-*.jsonl (counts (b), (c), (d) on every
game) and games-win-*.jsonl ((a) on the seed families searched). Rates are per 100 verified focal games; (a)'s
conversion rate carries a Wilson 95% interval, the per-100 rates a 95% Poisson interval on the event count. A pass
whose game and error rows do not cover its jobs is refused.
"""
from __future__ import annotations

import argparse
import json
import math
from collections import Counter, defaultdict
from pathlib import Path

DECKS = ["Wildfire", "Rally", "Affinity", "Elves", "Spy", "Burn", "Terror", "CawGates", "Faeries"]  # ndb.DECK_NAMES


def wilson(k: int, n: int) -> list[float] | None:
    if n == 0:
        return None
    z, p = 1.959964, k / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return [round(max(0.0, centre - half), 4), round(min(1.0, centre + half), 4)]


def own_deck(job_id: str) -> int:
    return int(job_id.rsplit("/", 1)[1].split("-")[0][1:])


def read(paths):
    games, errors = [], []
    for path in paths:
        for line in Path(path).read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if row["kind"] == "game":
                games.append(row)
            elif row["kind"] == "error":
                errors.append(row)
    return games, errors


def poisson(k: int) -> list[float]:
    """95% interval for a Poisson count (Wilson-Hilferty approximation to the exact Garwood interval)."""
    z = 1.959964
    lo = 0.0 if k == 0 else k * (1 - 1 / (9 * k) - z / (3 * math.sqrt(k))) ** 3
    hi = (k + 1) * (1 - 1 / (9 * (k + 1)) + z / (3 * math.sqrt(k + 1))) ** 3
    return [max(0.0, lo), hi]


def per100(k: int, games: int) -> dict:
    ci = poisson(k) if games else None
    return {"count": k, "per_100": round(100 * k / games, 2) if games else None,
            "ci_per_100": [round(100 * x / games, 2) for x in ci] if ci else None}


def covered(d: Path, tag: str) -> dict:
    """Game and error rows must cover every job the pass was given (census-<tag>.json)."""
    record = json.loads((d / f"census-{tag}.json").read_text(encoding="utf-8"))
    games, errors = read(sorted(d.glob(f"games-{tag}-*.jsonl")))
    if len(games) + len(errors) != record["expected_jobs"]:
        raise SystemExit(f"{d}: {tag} rows cover {len(games) + len(errors)} of {record['expected_jobs']} jobs")
    return record


def summarize(root: Path, name: str) -> dict:
    d = root / name / "census"
    covered(d, "bcd")
    games, errors = read(sorted(d.glob("games-bcd-*.jsonl")))
    n = Counter()
    for g in games:
        n.update(g["counts"])
    g_count = len(games)
    out = {"checkpoint": name, "games": g_count, "replay_errors": len(errors),
           "b_targeted_casts": n["b_targeted_casts"], "b_own_only": per100(n["b_own_only"], g_count),
           "b_harmful": per100(n["b_harmful"], g_count),
           "b_excluded": {k: n[k] for k in ("b_excluded_flicker", "b_excluded_library", "b_excluded_opponent")},
           "b_unlinked_target_steps": n["b_unlinked_target_steps"],
           "c_blocks": n["c_blocks"], "c_combats_with_block": n["c_combats_with_block"], "c_chumps": n["c_chumps"],
           "c_pointless_half": per100(n["c_pointless_half"], g_count),
           "c_pointless_quarter": per100(n["c_pointless_quarter"], g_count),
           "c_forced_include_combats": n["c_forced_include_combats"],
           "c_chumps_in_forced_combats": n["c_chumps_in_forced_combats"],
           "c_chumps_counterfactual_failed": n["c_chumps_counterfactual_failed"],
           "d_combat_losses": n["d_combat_losses"],
           "d_opportunities": n["d_combat_losses_with_block_decision"],
           "d_no_block_decision": n["d_combat_loss_no_block_decision"],
           "d_blunder": per100(n["d_blunder"], g_count), "d_bounded_out": n["d_bounded_out"],
           "d_post_declaration_loss": n["d_post_declaration_loss"], "d_no_survivor": n["d_no_survivor"],
           "d_recorded_continuation_failed": n["d_recorded_continuation_failed"], "d_walk_error": n["d_walk_error"],
           "scoring_failures": sum(g["scoring_failures"] for g in games),
           "focal_score": round(sum(1 for g in games if g["win"]) / g_count, 4) if g_count else None}
    if (d / "census-win.json").exists():
        covered(d, "win")
        wgames, werrors = read(sorted(d.glob("games-win-*.jsonl")))
        wgames = [g for g in wgames if g.get("searched_a")]
        a = Counter()
        by_deck = defaultdict(Counter)
        for g in wgames:
            a.update(g["counts"])
            by_deck[DECKS[own_deck(g["id"])]].update(g["counts"])
        opp, conv = a["a_opportunities"], a["a_converted"]
        searched = a["a_searched"]
        out["a"] = {"games": len(wgames), "replay_errors": len(werrors), "opportunities": opp, "converted": conv,
                    "conversion": round(conv / opp, 4) if opp else None, "conversion_ci": wilson(conv, opp),
                    "missed": per100(opp - conv, len(wgames)), "searched": searched,
                    "certified_share": round((a["a_proven"] + a["a_refuted"]) / searched, 4) if searched else None,
                    "unknown": a["a_unknown"], "low_opportunity_flag": opp < 100,
                    "by_deck": {k: {"opportunities": v["a_opportunities"], "converted": v["a_converted"],
                                    "conversion_ci": wilson(v["a_converted"], v["a_opportunities"])}
                                for k, v in sorted(by_deck.items())}}
    return out


def fmt(x: dict) -> str:
    if x["per_100"] is None:
        return "n/a"
    lo, hi = x["ci_per_100"]
    return f"{x['per_100']:.2f} ({lo:.1f}-{hi:.1f})"


def table(rows: list[dict], updates: dict) -> str:
    head = ("| Checkpoint | Updates | Games | (a) conversion (95% CI) | (a) opp. | (a) missed /100 | "
            "(b) harmful /100 | (b) own-only /100 [targeted casts] | (c) pointless chumps /100 (1/2) | (c) at 1/4 | "
            "(c) blocks | (d) blunders /100 | (d) opp. [bounded] |")
    lines = [head, "|" + "---|" * 13]
    for r in rows:
        a = r.get("a")
        conv = (f"{a['conversion']:.3f} ({a['conversion_ci'][0]:.3f}-{a['conversion_ci'][1]:.3f})"
                if a and a["opportunities"] else "pending")
        lines.append(" | ".join([
            f"| {r['checkpoint']}", f"{updates[r['checkpoint']]:,}", str(r["games"]), conv,
            str(a["opportunities"]) if a else "pending", fmt(a["missed"]) if a else "pending",
            fmt(r["b_harmful"]), f"{fmt(r['b_own_only'])} [{r['b_targeted_casts']}]",
            fmt(r["c_pointless_half"]), fmt(r["c_pointless_quarter"]), str(r["c_blocks"]),
            fmt(r["d_blunder"]), f"{r['d_opportunities']} [{r['d_bounded_out']}]"]) + " |")
    return "\n".join(lines) + "\n"


def chart(rows: list[dict], updates: dict, path: Path) -> None:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    panels = [("(a) win-this-turn conversion", lambda r: (r["a"]["conversion"], r["a"]["conversion_ci"])
               if r.get("a") and r["a"]["opportunities"] else None, "share of opportunities"),
              ("(b) harmful self-target /100 games", lambda r: (r["b_harmful"]["per_100"], r["b_harmful"]["ci_per_100"]),
               "per 100 games"),
              ("(c) pointless chump blocks /100 games", lambda r: (r["c_pointless_half"]["per_100"],
                                                                  r["c_pointless_half"]["ci_per_100"]), "per 100 games"),
              ("(d) lethal with a surviving block /100 games", lambda r: (r["d_blunder"]["per_100"],
                                                                          r["d_blunder"]["ci_per_100"]), "per 100 games")]
    fig, axes = plt.subplots(2, 2, figsize=(10, 6.5))
    xs_all = [updates[r["checkpoint"]] for r in rows]
    for ax, (title, get, ylabel) in zip(axes.flat, panels):
        pts = [(updates[r["checkpoint"]], get(r)) for r in rows if get(r) is not None]
        if pts:
            xs = [x for x, _ in pts]
            ys = [v[0] for _, v in pts]
            lo = [v[0] - v[1][0] for _, v in pts]
            hi = [v[1][1] - v[0] for _, v in pts]
            ax.errorbar(xs, ys, yerr=[lo, hi], fmt="o-", color="#2a6fb0", ecolor="#9bbbd9", capsize=3, lw=1.5)
        ax.set_title(title, fontsize=10)
        ax.set_ylabel(ylabel, fontsize=8)
        ax.set_xscale("symlog", linthresh=65536)
        ax.set_xticks(xs_all)
        ax.set_xticklabels(["b512" if x == 0 else f"{x // 1024}k" for x in xs_all], fontsize=8)
        ax.set_ylim(bottom=0)
        ax.grid(alpha=0.3)
        ax.spines[["top", "right"]].set_visible(False)
    fig.suptitle("Blunder census: chain A c1 against b512 (95% intervals)", fontsize=11)
    fig.tight_layout()
    fig.savefig(path, dpi=130)


def num(x) -> str:
    return "n/a" if x is None else f"{x:.3f}"


def cases_md(root: Path, names: list[str], path: Path, per_kind: int = 6) -> None:
    out = ["# Blunder census cases (first few per checkpoint and kind; full rows in census/cases-*.jsonl)\n"]
    for name in names:
        rows = []
        for tag, kinds in (("bcd", ("b_own_only", "c_chump", "d_combat_loss")), ("win", ("a_opportunity",))):
            for p in sorted((root / name / "census").glob(f"cases-{tag}-*.jsonl")):
                rows += [r for r in map(json.loads, p.read_text(encoding="utf-8").splitlines()) if r["kind"] in kinds]
        out.append(f"\n## {name}\n")
        for kind, keep in (("a_opportunity", lambda r: not r.get("converted")), ("b_own_only", lambda r: r["harmful"]),
                           ("c_chump", lambda r: r["pointless_half"]), ("d_combat_loss", lambda r: r["class"] == "blunder")):
            picked = [r for r in rows if r["kind"] == kind and keep(r)][:per_kind]
            out.append(f"\n### {kind} ({sum(1 for r in rows if r['kind'] == kind and keep(r))} cases)\n")
            for r in picked:
                if kind == "a_opportunity":
                    detail = f"p(chosen) {num(r['p_chosen'])}, p(line first) {num(r.get('p_line_first'))}, line {r['line'][:3]}"
                elif kind == "b_own_only":
                    menu = r["cast_menu"]
                    detail = (f"cast p {num(menu['p_chosen'])} (pass {num(menu['p_pass'])}), "
                              f"{(menu['chosen_action'] or '')[:120]}")
                elif kind == "c_chump":
                    detail = (f"no-block life {r['no_block_life']} of {r['start_life']}, include p "
                              f"{[round(s['p_include'] or 0, 3) for s in r['scans'] if s['include_chosen']]}")
                else:
                    detail = f"leaves {r['leaves']}, surviving path {r['surviving_path_include']}"
                out.append(f"- `{r['id']}` decision {r['index']}, round {r['turn']}: {detail}\n")
    path.write_text("".join(out), encoding="utf-8")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", type=Path, required=True)
    ap.add_argument("--checkpoints", required=True)
    ap.add_argument("--json", type=Path, required=True)
    ap.add_argument("--md", type=Path, required=True)
    ap.add_argument("--png", type=Path)
    ap.add_argument("--cases-md", type=Path)
    a = ap.parse_args(argv)
    updates = {k: int(v) for k, v in (x.split("=") for x in a.checkpoints.split(","))}
    rows = [summarize(a.root, name) for name in updates if (a.root / name / "census").exists()]
    a.json.write_text(json.dumps(rows, indent=1) + "\n", encoding="utf-8")
    a.md.write_text(table(rows, updates), encoding="utf-8")
    if a.png:
        chart(rows, updates, a.png)
    if a.cases_md:
        cases_md(a.root, [r["checkpoint"] for r in rows], a.cases_md)
    print(table(rows, updates))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
