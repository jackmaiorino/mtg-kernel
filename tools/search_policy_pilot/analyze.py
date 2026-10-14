"""Readouts for the E supported-choice CE pilot (DESIGN.md "Readouts"; no tuning, fixed rules).

Usage: python analyze.py ROOT OUT.json
- Strength: equal-weight nine-deck signed terminal score (2*score-1) per checkpoint, lineage and reference.
  T-O differences pair both focal seats of the same deck pair, seed family and reference (same seeds in
  both arms) and bootstrap whole paired units within deck-pair/reference strata (10,000 draws, registered
  seed), marginal 95% intervals. Screening signal: final T-O macro lower bound above 0 in each lineage
  (equal-weight reference aggregate) and non-Spy retention lower bound above -0.02 versus O.
- Dose: scheduled, available, supported and policy-disagreeing targets that reached each arm's updates, and
  the auxiliary/ordinary gradient ratios recorded per update.
- Probes and the teacher diagnostic are summarized when their outputs exist.
"""
from __future__ import annotations

import glob
import hashlib
import json
import random
import sys
from collections import defaultdict
from pathlib import Path

NS = "e-supported-choice-ce-v1-20261012"
SPY, DECKS, DRAWS = 4, 9, 10_000
REFS = ("T1", "H")


def rows(root: Path, ck: str, ref: str) -> dict | None:
    path = root / "eval" / f"{ck}-vs-{ref}" / "rows.jsonl"
    if not path.exists():
        return None
    out = {}
    for line in path.read_text().splitlines():
        r = json.loads(line)
        if r["complete"]:
            out[(r["own"], r["other"], r["repeat"], r["seat"])] = 2 * r["score"] - 1
    return out


def macro(scores: dict, decks=range(DECKS)) -> float:
    per = defaultdict(list)
    for (own, _other, _k, _seat), v in scores.items():
        per[own].append(v)
    return sum(sum(per[d]) / len(per[d]) for d in decks) / len(list(decks))


def units(t: dict, o: dict, ref: str) -> dict:
    """(own, other, ref) -> list of per-family paired differences (mean over both seats)."""
    out = defaultdict(list)
    for own in range(DECKS):
        for other in range(DECKS):
            for k in range(4):
                keys = [(own, other, k, s) for s in (0, 1)]
                if all(key in t and key in o for key in keys):
                    out[(own, other, ref)].append(sum(t[key] - o[key] for key in keys) / 2)
    return out


def macro_units(strata: dict, decks) -> float:
    per = defaultdict(list)
    for (own, _other, _ref), diffs in strata.items():
        per[own].extend(diffs)
    return sum(sum(per[d]) / len(per[d]) for d in decks) / len(decks)


def bootstrap(strata: dict, decks, label: str) -> dict:
    rng = random.Random(int.from_bytes(hashlib.sha256(f"{NS}/bootstrap/{label}".encode()).digest()[:8], "little"))
    point = macro_units(strata, decks)
    draws = []
    for _ in range(DRAWS):
        sample = {key: [rng.choice(v) for _ in v] for key, v in strata.items()}
        draws.append(macro_units(sample, decks))
    draws.sort()
    return {"point": point, "lower95": draws[int(0.025 * DRAWS)], "upper95": draws[int(0.975 * DRAWS) - 1],
            "units": sum(len(v) for v in strata.values())}


def strength(root: Path) -> dict:
    out = {"checkpoints": {}, "comparisons": {}}
    for ck in [f"{lin}-parent" for lin in ("r1", "r2")] + [f"{lin}-{arm}-{ph}" for lin in ("r1", "r2")
                                                           for arm in ("O", "T") for ph in ("main", "final")]:
        for ref in REFS:
            data = rows(root, ck, ref)
            if data is not None:
                out["checkpoints"][f"{ck}/{ref}"] = {"cases": len(data), "macro": macro(data),
                                                     "per_deck": {d: macro(data, [d]) for d in range(DECKS)}}
    non_spy = [d for d in range(DECKS) if d != SPY]
    for lin in ("r1", "r2"):
        for ph in ("main", "final"):
            strata, ok = {}, True
            for ref in REFS:
                t, o = rows(root, f"{lin}-T-{ph}", ref), rows(root, f"{lin}-O-{ph}", ref)
                if t is None or o is None:
                    ok = False
                    break
                strata.update(units(t, o, ref))
            if not ok:
                continue
            label = f"{lin}/{ph}"
            out["comparisons"][label] = {
                "all_decks": bootstrap(strata, list(range(DECKS)), label + "/all"),
                "non_spy": bootstrap(strata, non_spy, label + "/non-spy"),
                "per_reference": {ref: bootstrap({k: v for k, v in strata.items() if k[2] == ref},
                                                 list(range(DECKS)), f"{label}/{ref}") for ref in REFS},
                "per_deck": {d: macro_units(strata, [d]) for d in range(DECKS)}}
    finals = [out["comparisons"].get(f"{lin}/final") for lin in ("r1", "r2")]
    if all(finals):
        out["screening_signal"] = {
            "rule": "final T-O macro lower95 > 0 and non-Spy lower95 > -0.02, in each lineage",
            "met": all(f["all_decks"]["lower95"] > 0 and f["non_spy"]["lower95"] > -0.02 for f in finals)}
    return out


def dose(root: Path) -> dict:
    out = {}
    for arm in ("r1-T", "r1-O", "r2-T", "r2-O"):
        totals = defaultdict(float)
        ratios = []
        for path in glob.glob(str(root / "train" / f"{arm}-main" / "native*" / "iterations" / "*" / "attempt-*" /
                                  "update" / "update.json")):
            u = json.loads(Path(path).read_text())
            e = u.get("e_supported")
            if not e:
                continue
            packet = json.loads(Path(e["packet"]["path"]).read_text())
            totals["updates"] += 1
            totals["scheduled"] += packet["scheduled"]
            totals["valid"] += packet["valid"]
            for slot in packet["slots"]:
                totals["available"] += isinstance(slot.get("row_ordinal"), int)
                readout = slot.get("readout") or {}
                totals["disagreeing"] += bool(readout.get("disagreement"))
                if readout:
                    totals["deployed_mass_sum"] += readout["deployed_mass"]
                    totals["ce_sum"] += readout["ce"]
            ge = e.get("gradient_exposure")
            if ge and ge.get("gradient_components"):
                ratios.append(ge["gradient_components"]["all"]["ratio"])
        if totals["updates"]:
            n = max(totals["valid"], 1)
            out[arm] = {**{k: v for k, v in totals.items() if not k.endswith("_sum")},
                        "mean_teacher_mass_under_student": totals["deployed_mass_sum"] / n,
                        "mean_ce": totals["ce_sum"] / n,
                        "aux_to_ordinary_gradient_ratio_median": sorted(ratios)[len(ratios) // 2] if ratios else None}
    return out


def diagnostic(root: Path) -> dict:
    out = {}
    for lin in ("r1", "r2"):
        path = root / "readouts" / f"teacher-diagnostic-{lin}" / "diagnostic.json"
        if path.exists():
            out[lin] = json.loads(path.read_text())["totals"]
    return out


def main() -> None:
    root, target = Path(sys.argv[1]), Path(sys.argv[2])
    result = {"schema": f"{NS}/readouts", "strength": strength(root), "dose": dose(root),
              "teacher_diagnostic": diagnostic(root)}
    target.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({"comparisons": {k: {kk: (vv["point"], vv["lower95"], vv["upper95"])
                                          for kk, vv in v.items() if kk in ("all_decks", "non_spy")}
                                      for k, v in result["strength"]["comparisons"].items()},
                      "screening": result["strength"].get("screening_signal"), "dose": result["dose"]}, indent=1))


if __name__ == "__main__":
    main()
