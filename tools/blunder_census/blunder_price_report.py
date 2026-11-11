"""Blunder pricing report: value-gap tables, gap chart, same-state table and exposure (collab LANES/pricing-20261111).

usage: blunder_price_report.py --root DIR --census DIR --out DIR
  DIR/price-<panel>/rows-price-*.jsonl, DIR/score/rows-score-*.jsonl, census games-bcd.jsonl per checkpoint
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import blunder_price as bp  # noqa: E402

PANELS = ("b512", "c1-u0524288")
CHECKPOINTS = ("b512", "c1-u0065536", "c1-u0131072", "c1-u0262144", "c1-u0524288")
NAMES = {"a": "(a) missed win (control)", "b": "(b) harmful self-target", "c": "(c) pointless chump",
         "d": "(d) lethal with surviving block"}


def quart(xs):
    xs = sorted(xs)
    if not xs:
        return None
    q = lambda f: xs[min(len(xs) - 1, int(f * len(xs)))]
    return [q(0.25), q(0.5), q(0.75)]


def reading(entry, p_good_median):
    lo, hi = entry["ci"]
    if lo > 0.02:
        return "practical starvation" if p_good_median is not None and p_good_median < 0.01 else "credit assignment"
    if lo >= -0.02 and hi <= 0.02:
        return "weak signal or acceptable play"
    if hi < -0.02:
        return "flagged play better than the alternative"
    return "unresolved at this N"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", type=Path, required=True)
    ap.add_argument("--census", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    a.out.mkdir(parents=True, exist_ok=True)
    price = {p: bp.summarize_price(a.root / f"price-{p}", "price") for p in PANELS}
    score_rows = [r for p in PANELS for r in bp.rows(a.root / "score", f"score-{p}") if r["kind"] == "score"]
    with (a.out / "cases.jsonl").open("w", encoding="utf-8") as f:
        for p in PANELS:
            for rec in bp.per_case(a.root / f"price-{p}", "price"):
                f.write(json.dumps({"panel": p, **rec}) + "\n")
    bank_panel = lambda r: "c1-u0524288" if r["id"].startswith("c1-u0524288") else "b512"
    same = {}
    for cls in bp.CLASSES:
        for ck in CHECKPOINTS:
            rs = [r["scores"][ck] for r in score_rows if r["class"] == cls]
            if rs:
                same[f"{cls}/{ck}"] = {"states": len(rs), "margin_q": quart([x["margin"] for x in rs]),
                                       "p_good_q": quart([x["p_good"] for x in rs]),
                                       "share_p_good_below_0.01": sum(x["p_good"] < 0.01 for x in rs) / len(rs),
                                       "mean_p_good": sum(x["p_good"] for x in rs) / len(rs)}
    by_bank = {}
    for p in PANELS:
        for cls in bp.CLASSES:
            for ck in CHECKPOINTS:
                rs = [r["scores"][ck] for r in score_rows if r["class"] == cls and bank_panel(r) == p]
                if rs:
                    by_bank[f"{p}/{cls}/{ck}"] = {"states": len(rs), "p_good_q": quart([x["p_good"] for x in rs]),
                                                  "margin_q": quart([x["margin"] for x in rs]),
                                                  "share_p_good_below_0.01": sum(x["p_good"] < 0.01 for x in rs) / len(rs)}
    own = {}
    for p in PANELS:
        for cls in bp.CLASSES:
            rs = [r["scores"][p]["p_good"] for r in score_rows if r["class"] == cls and bank_panel(r) == p]
            own[f"{p}/{cls}"] = {"median": quart(rs)[1] if rs else None, "mean": sum(rs) / len(rs) if rs else None,
                                 "states": len(rs)}
    exposure = {}
    for p in PANELS:
        games = [json.loads(x) for x in (a.census / p / "games-bcd.jsonl").read_text(encoding="utf-8").splitlines()]
        games = [g for g in games if g["kind"] == "game"]
        n = len(games)
        tot = lambda k: sum(g["counts"].get(k, 0) for g in games)
        cases = [json.loads(x) for x in (a.census / p / "cases.jsonl").read_text(encoding="utf-8").splitlines()]
        flagged = {cls: sum(1 for r in cases if bp.flagged(r) == cls) for cls in bp.CLASSES}
        wg = [json.loads(x) for x in (a.census / p / "games-win.jsonl").read_text(encoding="utf-8").splitlines()]
        wn = sum(1 for g in wg if g["kind"] == "game" and g.get("searched_a"))
        exposure[p] = {"games": n, "blocker_decision_combats_per_game": tot("c_combats_with_blocker_decision") / n,
                       "blocks_per_game": tot("c_blocks") / n, "targeted_casts_per_game": tot("b_targeted_casts") / n,
                       "flagged_per_100": {cls: 100 * flagged[cls] / (wn if cls == "a" else n) for cls in bp.CLASSES},
                       "good_samples_per_100_games": {
                           cls: (100 * flagged[cls] / (wn if cls == "a" else n)) * own[f"{p}/{cls}"]["mean"]
                           if own[f"{p}/{cls}"]["mean"] is not None else None for cls in bp.CLASSES}}
    readings = {}
    for p in PANELS:
        for cls in bp.CLASSES:
            e = price[p].get(f"{cls}/worlds")
            if e and e["ci"]:
                readings[f"{p}/{cls}"] = reading(e, own[f"{p}/{cls}"]["median"])
    summary = {"price": {p: {k: {x: v for x, v in e.items() if x != "gaps"} for k, e in price[p].items()} for p in PANELS},
               "same_state": same, "same_state_by_bank": by_bank, "own_p_good": own, "exposure": exposure, "readings": readings}
    (a.out / "summary.json").write_text(json.dumps(summary, indent=1) + "\n", encoding="utf-8")

    lines = ["| Panel | Class | Mode | Cases (invalid) | Mean gap (95% CI) | Median | Clearly + | Near 0 | Negative | Unresolved | "
             "Decided share | Live mean gap | Mean value (chosen) | Reading |", "|" + "---|" * 14]
    for p in PANELS:
        for cls in bp.CLASSES:
            for mode in ("worlds", "true"):
                e = price[p].get(f"{cls}/{mode}")
                if not e:
                    continue
                lab = e["labels"]
                live = "n/a" if e["live_mean_gap"] is None else f"{e['live_mean_gap']:+.3f}"
                lines.append(f"| {p} | {NAMES[cls]} | {'resampled' if mode == 'worlds' else 'true state'} | {e['cases']} ({e['invalid_cases']}) | "
                             f"{e['mean_gap']:+.3f} ({e['ci'][0]:+.3f}, {e['ci'][1]:+.3f}) | {e['median_gap']:+.3f} | "
                             f"{lab['clearly_positive']:.2f} | {lab['near_zero']:.2f} | {lab['negative']:.2f} | "
                             f"{lab['unresolved']:.2f} | {e['decided_share']:.2f} | {live} | {e['mean_value_chosen']:.3f} | "
                             f"{readings.get(f'{p}/{cls}', '') if mode == 'worlds' else ''} |")
    (a.out / "price-table.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    lines = ["| Class | Checkpoint | States | Margin quartiles (bad minus best other logit) | p(good) quartiles | "
             "Share p(good) < 0.01 |", "|---|---|---:|---|---|---:|"]
    for cls in bp.CLASSES:
        for ck in CHECKPOINTS:
            e = same.get(f"{cls}/{ck}")
            if e:
                m, g = e["margin_q"], e["p_good_q"]
                lines.append(f"| {NAMES[cls]} | {ck} | {e['states']} | {m[0]:+.2f} / {m[1]:+.2f} / {m[2]:+.2f} | "
                             f"{g[0]:.4f} / {g[1]:.4f} / {g[2]:.4f} | {e['share_p_good_below_0.01']:.2f} |")
    lines += ["", "By bank (the panel whose flags the states come from):", "",
              "| Bank | Class | Checkpoint | States | p(good) quartiles | Share p(good) < 0.01 |", "|---|---|---|---:|---|---:|"]
    for k, e in by_bank.items():
        p, cls, ck = k.split("/")
        g = e["p_good_q"]
        lines.append(f"| {p} | {NAMES[cls]} | {ck} | {e['states']} | {g[0]:.4f} / {g[1]:.4f} / {g[2]:.4f} | "
                     f"{e['share_p_good_below_0.01']:.2f} |")
    (a.out / "same-state-table.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import random
    fig, axes = plt.subplots(1, 4, figsize=(13, 4.2), sharey=True)
    colors = {"b512": "#2a6fb0", "c1-u0524288": "#c0612b"}
    for ax, cls in zip(axes, bp.CLASSES):
        for j, p in enumerate(PANELS):
            e = price[p].get(f"{cls}/worlds")
            if not e:
                continue
            rng = random.Random(7)
            xs = [j + rng.uniform(-0.18, 0.18) for _ in e["gaps"]]
            ax.scatter(xs, e["gaps"], s=7, alpha=0.35, color=colors[p], linewidths=0)
            ax.errorbar([j + 0.3], [e["mean_gap"]], yerr=[[e["mean_gap"] - e["ci"][0]], [e["ci"][1] - e["mean_gap"]]],
                        fmt="o", color="#222222", capsize=3, ms=4)
        ax.axhline(0, color="#888888", lw=0.8)
        ax.set_xticks([0, 1])
        ax.set_xticklabels(["b512", "c1 524k"], fontsize=8)
        ax.set_title(NAMES[cls], fontsize=9)
        ax.grid(axis="y", alpha=0.3)
        ax.spines[["top", "right"]].set_visible(False)
    axes[0].set_ylabel("win-probability gap (alternative minus chosen)", fontsize=8)
    fig.suptitle("Forced-alternative value gap per case (resampled worlds; dots = cases, black = class mean, 95% CI)",
                 fontsize=10)
    fig.tight_layout()
    fig.savefig(a.out / "gap-chart.png", dpi=130)
    print(json.dumps({"readings": readings}, indent=1))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
