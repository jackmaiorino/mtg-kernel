"""Rank exposed decisions that are not real choices, from s4a-equiv rows.

Usage: python summarize_equiv.py OUT.json ROWS.jsonl [...]
A decision is "fake for the chooser" when every option leads to the same
chooser view (classes == 1); "order-only" when the exact view differs but
the counter-masked view does not (classes_masked == 1 < classes); and
"partly redundant" when classes < k. Groups by action kinds and source card.
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
        c["fake"] += d["classes"] == 1 and not d["unsettled"]
        c["order_only"] += d["classes_masked"] == 1 and d["classes"] > 1 and not d["unsettled"]
        c["redundant"] += 1 < d["classes"] < d["k"]
        c["forced_complete"] += d["forced_complete"]
        c["unsettled"] += bool(d["unsettled"])
    ranked = sorted(groups.items(), key=lambda kv: -(kv[1]["fake"] + kv[1]["order_only"]))
    overall = Counter()
    for _, c in groups.items():
        overall.update(c)
    out = {"games": games, "decisions": total, "overall": dict(overall),
           "groups": [{"kinds": k[0], "sources": k[1], **dict(c)} for k, c in ranked]}
    json.dump(out, open(sys.argv[1], "w"), indent=1)
    print(f"{games} games, {total} decisions with 2+ options; fake {overall['fake']} "
          f"({overall['fake'] / max(total, 1):.1%}), order-only {overall['order_only']}, "
          f"redundant {overall['redundant']}, forced-complete {overall['forced_complete']}, "
          f"unsettled {overall['unsettled']}")
    for g in out["groups"][:25]:
        print(f"{g['decisions']:7} fake {g['fake']:6} order {g['order_only']:6} red {g['redundant']:6} "
              f"fc {g['forced_complete']:6}  {g['kinds'][:60]} | {g['sources'][:60]}")


if __name__ == "__main__":
    main()
