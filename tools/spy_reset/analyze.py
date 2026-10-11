"""Frozen analysis of the reset pilot's raw-policy panel (RUNNER.md sections 7-8).

Input: an index {"panels": [{"checkpoint": ID, "opponent": "T1"|"H",
"checkpoint_sha256": SHA, "rows": PATH, "labels": PATH}]}. Copy the expected
checkpoint hash from the existing panel plan's candidate.checkpoint.sha256.
with checkpoint IDs r1-parent, r2-parent, r1-O, r1-P, r1-S, r2-O, r2-P, r2-S and T1.
``rows`` are nine_deck_public_panel_v1 rows (game one score per case); ``labels`` are
``regret_census_v1 mode=s4a-label-replay`` rows for the Spy-own cases.

Outputs, separately by lineage, arm and opponent: deck scores (wins 1, draws 0.5,
losses 0), the 81 ordered-pair tables, equal-weight nine-deck score, seven-deck
retention (excluding Spy and CawGates), CawGates score, and Spy behavior over all
216 Spy cases (winning combo, resolved combo, self-target and Dread Return labels,
raw Spy score). Paired contrasts S-O, S-P, endpoint-parent and endpoint-T1 with
cluster-bootstrap 95% intervals (repeat seat-pairs resampled within each ordered
deck pair, jointly across opponents, checkpoints and lineages; 10,000 draws,
seed 2026101017). Technical terminations are missing with best/worst-case bounds.
Then the fixed interpretation table, applied in order.
"""
from __future__ import annotations

import argparse
import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot  # noqa: E402

DECKS, REPEATS = pilot.DECKS, pilot.PANEL_REPEATS
SPY, CAW = pilot.SPY, pilot.CAWGATES
SEVEN = [d for d in range(DECKS) if d not in (SPY, CAW)]
LINEAGES = ("r1", "r2")
ARMS = ("O", "P", "S")
OPPONENTS = ("T1", "H")
BOOT_SEED, DRAWS, RETENTION_PP = 2026101017, 10_000, 0.02


def load(index: dict) -> dict:
    """data[(checkpoint, opponent)][(own, other, repeat, seat)] = case record."""
    data = {}
    for panel in index["panels"]:
        key = (panel["checkpoint"], panel["opponent"])
        if key in data or key[1] not in OPPONENTS or key[0] not in (
                "T1", *(f"{lin}-{role}" for lin in LINEAGES for role in (*ARMS, "parent"))):
            raise ValueError(f"duplicate or unknown panel {key}")
        expected_panel = f"{key[0]}-vs-{key[1]}"
        expected_hash = panel.get("checkpoint_sha256")
        if (not isinstance(expected_hash, str) or len(expected_hash) != 64
                or any(c not in "0123456789abcdef" for c in expected_hash)):
            raise ValueError(f"{expected_panel}: expected checkpoint_sha256 is required")
        expected_cases = {(o, t, k, s) for o in range(DECKS) for t in range(DECKS)
                          for k in range(REPEATS) for s in (0, 1)}
        cases = {}
        for line in Path(panel["rows"]).read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                case_key = (r["own"], r["other"], r["repeat"], r["seat"])
                if (any(type(v) is not int for v in case_key) or case_key not in expected_cases
                        or case_key in cases):
                    raise ValueError(f"{panel['rows']}: duplicate or unknown case {case_key}")
                if (r.get("panel") != expected_panel or r.get("checkpoint_sha256") != expected_hash
                        or r.get("seed") != pilot.panel_seed(*case_key[:3])):
                    raise ValueError(f"{panel['rows']}: panel, checkpoint or seed differs")
                if (type(r["complete"]) is not bool
                        or (r["complete"] and (type(r["score"]) not in (int, float)
                                               or r["score"] not in (0, 0.5, 1)))
                        or (not r["complete"] and r["score"] is not None)):
                    raise ValueError(f"{panel['rows']}: invalid completion or score")
                cases[case_key] = r
        if cases.keys() != expected_cases:
            raise ValueError(f"{panel['rows']}: exact case roster differs")
        expected_labels = {f"{expected_panel}/o{o}-t{t}-k{k}-s{s}": (o, t, k, s)
                           for (o, t, k, s) in expected_cases if o == SPY}
        labels = {}
        for line in Path(panel["labels"]).read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                if r["id"] not in expected_labels or r["id"] in labels:
                    raise ValueError(f"{panel['labels']}: duplicate or foreign label {r['id']}")
                labels[r["id"]] = r
        for (o, t, k, s), case in cases.items():
            if o != SPY:
                continue
            lab = labels.get(f"{expected_panel}/o{o}-t{t}-k{k}-s{s}")
            if case["complete"] and (lab is None or lab["kind"] != "label_replay"):
                raise ValueError(f"{panel['labels']}: missing label for a complete Spy case o{o}-t{t}-k{k}-s{s}")
            if lab is not None and lab["kind"] == "label_replay":
                winner = lab["winner"]
                if (type(lab["focal"]) is not int or lab["focal"] != s
                        or (winner is not None and (type(winner) is not int or winner not in (0, 1)))
                        or type(lab["win"]) is not bool or lab["win"] != (winner == s)
                        or type(lab["combo_resolved"]) is not bool
                        or type(lab["combo_won"]) is not bool
                        or lab["combo_won"] != (lab["combo_resolved"] and lab["win"])
                        or (case["complete"] and case["score"] != (
                            0.5 if winner is None else float(winner == s)))):
                    raise ValueError(f"{panel['labels']}: focal, winner or score differs")
            case["labels"] = lab
        data[key] = cases
    return data


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def case_value(case: dict, metric: str, fill=None):
    """A case's value for ``metric``, or ``fill`` (None, 0 or 1) when it is missing."""
    if not case["complete"]:
        return fill
    if metric == "score":
        return case["score"]
    lab = case["labels"]
    return {"combo_won": float(lab["combo_won"]), "combo_resolved": float(lab["combo_resolved"]),
            "self_chosen": float(lab["labels"]["self_chosen"]),
            "self_resolved": float(lab["labels"]["self_target_resolved"]),
            "dr_giant_chosen": float(lab["labels"]["dr_giant_chosen"]),
            "dr_giant_offered": float(lab["labels"]["dr_giant_offered"])}[metric]


def deck_mean(cases: dict, deck: int, metric: str, repeats=None, fill=None):
    repeats = repeats or {(deck, t): list(range(REPEATS)) for t in range(DECKS)}
    values = [case_value(cases[(deck, t, k, s)], metric, fill)
              for t in range(DECKS) for k in repeats[(deck, t)] for s in (0, 1)]
    return mean(values) if fill is None else sum(values) / len(values)


def summary(cases: dict, repeats=None, fill=None) -> dict:
    decks = [deck_mean(cases, d, "score", repeats, fill) for d in range(DECKS)]
    out = {"deck_scores": decks,
           "nine_deck": mean(decks), "seven_deck": mean([decks[d] for d in SEVEN]), "cawgates": decks[CAW],
           "spy_score": decks[SPY]}
    for metric in ("combo_won", "combo_resolved", "self_chosen", "self_resolved", "dr_giant_chosen"):
        # Unconditional: every completed Spy game is in the denominator, whether
        # or not the combo was offered; missing games are bounded separately.
        out[metric] = deck_mean(cases, SPY, metric, repeats, fill)
    return out


def pair_tables(cases: dict) -> list[list]:
    return [[mean([case_value(cases[(o, t, k, s)], "score") for k in range(REPEATS) for s in (0, 1)])
             for t in range(DECKS)] for o in range(DECKS)]


def missing(cases: dict) -> dict:
    gone = [key for key, c in cases.items() if not c["complete"]]
    return {"count": len(gone), "denominator": len(cases), "cases": [list(k) for k in gone]}


CONTRASTS = [("S-O", "S", "O"), ("S-P", "S", "P"), ("O-parent", "O", "parent"), ("P-parent", "P", "parent"),
             ("S-parent", "S", "parent"), ("O-T1", "O", "T1"), ("P-T1", "P", "T1"), ("S-T1", "S", "T1")]
METRICS = ("combo_won", "combo_resolved", "spy_score", "seven_deck", "cawgates", "nine_deck")


def ckpt(lineage: str, role: str) -> str:
    return "T1" if role == "T1" else f"{lineage}-{role}"


def contrasts(data: dict, repeats=None) -> dict:
    out = {}
    for lineage in LINEAGES:
        for opp in OPPONENTS:
            s = {role: summary(data[(ckpt(lineage, role), opp)], repeats)
                 for role in ("O", "P", "S", "parent", "T1")}
            for name, a, b in CONTRASTS:
                for metric in METRICS:
                    out[(lineage, opp, name, metric)] = s[a][metric] - s[b][metric]
    return out


def arrays(cases: dict):
    """score[o, t, k, s] and Spy metrics[m][t, k, s]; NaN where a case is missing."""
    import numpy as np
    score = np.full((DECKS, DECKS, REPEATS, 2), np.nan)
    spy = {m: np.full((DECKS, REPEATS, 2), np.nan) for m in ("combo_won", "combo_resolved")}
    for (o, t, k, s), c in cases.items():
        if c["complete"]:
            score[o, t, k, s] = c["score"]
            if o == SPY:
                for m in spy:
                    spy[m][t, k, s] = case_value(c, m)
    return score, spy


def bootstrap(data: dict) -> dict:
    """Seat pairs of one repeat resampled together within each ordered deck pair, the same
    draw for every opponent, checkpoint and lineage; complete cases within a draw."""
    import numpy as np
    rng = np.random.default_rng(BOOT_SEED)
    arr = {key: arrays(cases) for key, cases in data.items()}
    draws = {}
    for _ in range(DRAWS):
        idx = rng.integers(0, REPEATS, size=(DECKS, DECKS, REPEATS))
        stats = {}
        for key, (score, spy) in arr.items():
            res = np.take_along_axis(score, idx[:, :, :, None].repeat(2, axis=3), axis=2)
            decks = np.nanmean(res.reshape(DECKS, -1), axis=1)
            spy_idx = idx[SPY][:, :, None].repeat(2, axis=2)
            stats[key] = {"nine_deck": decks.mean(), "seven_deck": decks[SEVEN].mean(), "cawgates": decks[CAW],
                          "spy_score": decks[SPY],
                          **{m: np.nanmean(np.take_along_axis(v, spy_idx, axis=1)) for m, v in spy.items()}}
        for lineage in LINEAGES:
            for opp in OPPONENTS:
                for name, a, b in CONTRASTS:
                    for metric in METRICS:
                        value = stats[(ckpt(lineage, a), opp)][metric] - stats[(ckpt(lineage, b), opp)][metric]
                        draws.setdefault((lineage, opp, name, metric), []).append(float(value))
    out = {}
    for key, values in draws.items():
        values.sort()
        out[key] = [values[int(0.025 * DRAWS)], values[int(0.975 * DRAWS) - 1]]
    return out


def screens(data: dict, bound: str | None = None) -> dict:
    """Behavior and retention screens. Every screen compares S with another checkpoint, so
    ``bound="worst"`` scores S's missing cases 0 and every other checkpoint's 1, and
    ``bound="best"`` the reverse; None uses completed cases."""
    s = {}
    for lineage in LINEAGES:
        for role in ("O", "P", "S", "parent", "T1"):
            fill = None if bound is None else float((role == "S") == (bound == "best"))
            for opp in OPPONENTS:
                cases = data[(ckpt(lineage, role), opp)]
                s[(lineage, role, opp)] = summary(cases, fill=fill)
    pooled = {(lin, role): (s[(lin, role, "T1")]["combo_won"] + s[(lin, role, "H")]["combo_won"]) / 2
              for lin in LINEAGES for role in ("O", "P", "S", "parent", "T1")}
    behavior = {lin: {"S": pooled[(lin, "S")], "P": pooled[(lin, "P")], "O": pooled[(lin, "O")],
                      "S_gt_O": pooled[(lin, "S")] > pooled[(lin, "O")],
                      "S_gt_P": pooled[(lin, "S")] > pooled[(lin, "P")],
                      "P_gt_O": pooled[(lin, "P")] > pooled[(lin, "O")]} for lin in LINEAGES}
    retention = {}
    for lin in LINEAGES:
        for opp in OPPONENTS:
            for ref in ("O", "parent", "T1"):
                for metric in ("seven_deck", "cawgates"):
                    diff = s[(lin, "S", opp)][metric] - s[(lin, ref, opp)][metric]
                    retention[f"{lin}/{opp}/S-{ref}/{metric}"] = {"diff": diff, "fail": diff < -RETENTION_PP}
    spy_win = {lin: (s[(lin, "S", "T1")]["spy_score"] + s[(lin, "S", "H")]["spy_score"]) / 2
               - (s[(lin, "O", "T1")]["spy_score"] + s[(lin, "O", "H")]["spy_score"]) / 2 for lin in LINEAGES}
    return {"behavior": behavior, "retention": retention, "spy_score_S_minus_O": spy_win}


def classify(sc: dict) -> str:
    if any(v["fail"] for v in sc["retention"].values()):
        return "retention concern"
    b = sc["behavior"]
    s_best = [b[lin]["S_gt_O"] and b[lin]["S_gt_P"] for lin in LINEAGES]
    if all(s_best):
        return "promising transfer"
    if all(b[lin]["P_gt_O"] and b[lin]["S_gt_O"] for lin in LINEAGES):
        return "reset may help; no concordant support for the search expense"
    if sum(s_best) == 1:
        return "mixed"
    return "no concordant transfer"


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--index", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--no-bootstrap", action="store_true", help="engineering checks only")
    a = ap.parse_args(argv)
    data = load(json.loads(a.index.read_text()))
    result = {"panels": {}, "missing": {}}
    for (ck, opp), cases in sorted(data.items()):
        result["panels"][f"{ck}/{opp}"] = {**summary(cases), "pair_tables": pair_tables(cases)}
        result["missing"][f"{ck}/{opp}"] = missing(cases)
    point = contrasts(data)
    intervals = {} if a.no_bootstrap else bootstrap(data)
    result["contrasts"] = {"/".join(k): {"point": v, "ci95": intervals.get(k)} for k, v in point.items()}
    observed = screens(data)
    labels = {"observed": classify(observed)}
    if any(m["count"] for m in result["missing"].values()):
        labels["worst"] = classify(screens(data, "worst"))
        labels["best"] = classify(screens(data, "best"))
    result["screens"] = observed
    result["classification"] = labels
    stable = len(set(labels.values())) == 1
    disposition = labels["observed"] if stable else "inconclusive: missing outcomes change the screen"
    if disposition == "promising transfer" and any(v < 0 for v in observed["spy_score_S_minus_O"].values()):
        disposition = "behavior-only improvement (Spy win score fell)"
    result["disposition"] = disposition
    a.out.write_text(json.dumps(result, indent=1) + "\n")
    print(json.dumps({"disposition": disposition, "classification": labels,
                      "behavior": observed["behavior"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
