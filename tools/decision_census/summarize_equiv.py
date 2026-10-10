"""Rank exposed decisions that are not real choices, from s4a-equiv rows.

Usage: python summarize_equiv.py OUT.json ROWS.jsonl [...]
Candidate statistics of a fixed-continuation probe (equiv.rs), not
established equivalences: after the tried option, every later choice takes
option 0 until the chooser's next decision of another kind, in one sampled
game. "single_view" = every option reached the same chooser view (E's
canonical bytes, which already sort graveyards); "masked_single" = views
differ only in fields the mask drops (engine counters, handles,
attachments, object-list order), so these are candidates, not proven
order-only; "partly_matching" = fewer view classes than options. Capped
(unsettled) menus are excluded from all three and counted separately.
"""
import json
import sys
from collections import Counter, defaultdict


def main():
    games, rows = 0, []
    for f in sys.argv[2:]:
        for line in open(f, encoding="utf-8"):
            g = json.loads(line)
            if g.get("kind") != "s4a_equiv_game":
                continue
            games += 1
            rows.extend(g["decisions"])
    total = len(rows)
    groups = defaultdict(Counter)
    for d in rows:
        key = ("+".join(d["kinds"]), "+".join(d["sources"]) or "-")
        c = groups[key]
        c["decisions"] += 1
        c["options"] += d["k"]
        c["classes"] += d["classes"]
        ok = not d["unsettled"]
        c["single_view"] += ok and d["classes"] == 1
        c["masked_single"] += ok and d["classes_masked"] == 1 and d["classes"] > 1
        c["partly_matching"] += ok and 1 < d["classes"] < d["k"]
        c["forced_complete"] += d["forced_complete"]
        c["unsettled"] += bool(d["unsettled"])
    ranked = sorted(groups.items(), key=lambda kv: -(kv[1]["single_view"] + kv[1]["masked_single"]))
    overall = Counter()
    for _, c in groups.items():
        overall.update(c)
    out = {"games": games, "decisions": total, "overall": dict(overall),
           "groups": [{"kinds": k[0], "sources": k[1], **dict(c)} for k, c in ranked]}
    json.dump(out, open(sys.argv[1], "w"), indent=1)
    print(f"{games} games, {total} decisions with 2+ options; single view {overall['single_view']} "
          f"({overall['single_view'] / max(total, 1):.1%}), masked single {overall['masked_single']}, "
          f"partly matching {overall['partly_matching']}, choose-all shape {overall['forced_complete']}, "
          f"unsettled {overall['unsettled']}")
    for g in out["groups"][:25]:
        print(f"{g['decisions']:7} single {g['single_view']:6} masked {g['masked_single']:6} "
              f"partly {g['partly_matching']:6} choose-all {g['forced_complete']:6}  "
              f"{g['kinds'][:60]} | {g['sources'][:60]}")


if __name__ == "__main__":
    main()
