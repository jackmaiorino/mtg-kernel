"""Bank completeness and the exposure feasibility check (RUNNER.md section 4).

Run only after both arms' cuts are frozen (the P/S training configs are
written and hashed): the summary reads guide outcomes.

Exposure check, per lineage: at least two distinct usable Spy-own S guides
with a focal non-forced decision at or after at least one of that guide's
scheduled cuts whose menu offers Balustrade Spy targeting its controller or
Dread Return targeting Lotleth Giant, and at least one such guide with the
Dread Return menu. Witnesses are reported whatever was chosen and whoever
won. Nothing here changes eligibility, cuts or training.

Usage: python bank_report.py --bank r1=BANK.jsonl --bank r2=BANK.jsonl --out REPORT.json
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from fractions import Fraction
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot  # noqa: E402

MIN_USABLE = 146


def guide_cut_steps(lineage: str, row: dict, arm: str) -> list[int]:
    plan = pilot.schedule(lineage)
    qs = [Fraction(e["q"]) for update in plan["updates"] for e in update
          if e.get("guide") == row["id"]]
    starts = row["guides"][arm]["starts"]
    return [starts[pilot.cut_index(q, len(starts))]["step"] for q in qs]


def lineage_report(lineage: str, bank: dict) -> dict:
    rows = list(bank.values())
    usable = [r for r in rows if r["usable"]]
    reasons = Counter(r["unusable_reason"] for r in rows if not r["usable"])
    report = {"sources": len(rows), "usable_paired_guides": len(usable), "unusable": dict(reasons),
              "bank_complete": len(usable) >= MIN_USABLE,
              "source_end": dict(Counter(r["source"]["end"] for r in rows))}
    outcomes = {arm: dict(Counter(r["guides"][arm]["end"] for r in rows if r.get("guides"))) for arm in ("P", "S")}
    report["guide_end"] = outcomes
    qualifying, direct = [], 0
    for r in usable:
        if r["own"] != pilot.SPY:
            continue
        cuts = guide_cut_steps(lineage, r, "S")
        hits = [w for w in r["guides"]["S"]["witnesses"] if any(w["step"] >= c for c in cuts)]
        direct += sum(1 for w in hits if w["substep"] == 0 and w["step"] in cuts)
        if hits:
            qualifying.append({"source": r["id"], "cuts": cuts, "witnesses": hits,
                               "dr_giant": any(w["dr_giant_offered"] for w in hits)})
    report["exposure"] = {"spy_own_usable_s_guides": sum(1 for r in usable if r["own"] == pilot.SPY),
                          "qualifying_guides": len(qualifying),
                          "qualifying_with_dr_giant": sum(q["dr_giant"] for q in qualifying),
                          "direct_menu_resets": direct, "guides": qualifying}
    report["exposure_pass"] = len(qualifying) >= 2 and any(q["dr_giant"] for q in qualifying)
    return report


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--bank", action="append", required=True, help="lineage=path")
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    out = {}
    for item in a.bank:
        lineage, path = item.split("=", 1)
        out[lineage] = lineage_report(lineage, pilot.read_bank(Path(path), lineage))
    out["proceed"] = all(v["bank_complete"] and v["exposure_pass"] for v in out.values())
    a.out.write_text(json.dumps(out, indent=1) + "\n")
    print(json.dumps({k: (v if k == "proceed" else {x: v[x] for x in ("usable_paired_guides", "bank_complete",
                                                                       "exposure_pass")})
                      for k, v in out.items()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
