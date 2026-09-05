"""Cycle-4 M1 CP7 transfer analysis (pre-registration sections M1, 4 and 7).

Reads the M1 CP7 panel written by run-m1-cp7-panel.sh: two model groups (A and
B), each 16 disjoint 128-pair shards of run_cp7_store_panel_v2.py on ONE base
seed, so every model has the same 2,048 roots. It then computes the
pre-registered within-panel CRN-paired contrasts with the fixed-N root-cluster
estimator:

  one inferential unit = one root = both seat-swapped legs;
  root score of a model = mean over its two legs of (win 1, draw 0.5, loss 0);
  paired delta per root = score(endpoint) - score(reference);
  point = mean delta in percentage points; SE = sample sd / sqrt(N roots);
  one-sided 95 percent lower bound = point - 1.645 * SE (fixed N, no anytime
  correction); milestone = LCB > 0 AND point >= +2.0 pp.

Primary hypothesis: TREATMENT-RB vs g896 (the only contrast gated directly).
Secondaries under Holm (alpha 0.05, one-sided p-values): CONTROL-R vs g896 and
STATIC-RB vs g896; a secondary's milestone is assigned only after the Holm
step. The cycle-3 g2048 contrasts are co-measured context and carry no gate.

Fail-closed rules, all checked before any statistic is computed:
  - every shard directory holds a published panel-summary.json (a shard the
    runner did not complete is refused);
  - voids come from summary["voids"]["per_model"][label]["voided_pair_indices"]
    and are accumulated across shards; a model with more than 2 percent of the
    registered roots voided (41 or more of 2,048) fails the registered cap and
    the analysis refuses;
  - every model's root universe must be exactly range(2048): the pairs with
    outcomes plus its voided pairs, with no outcome counted for a voided pair;
  - the repeated model across groups must be exactly treatment-rb, with
    identical root coverage, identical voids, identical environment seeds and
    identical outcomes in both groups;
  - both groups must agree on every root's environment seed.
Contrast N is the number of roots not voided for either model of the pair.

Written and reviewed BEFORE any M1 outcome existed (the routing record
f9ad13e9... precedes it).
usage: analyze_m1_cp7_panel_v1.py --group-a <root of m1-a> --group-b <root of m1-b> --output <json>
       analyze_m1_cp7_panel_v1.py --self-test
"""
from __future__ import annotations

import argparse
import glob
import json
import math
import os

PRIMARY = ("treatment-rb", "g896")
SECONDARIES = (("control-r", "g896"), ("static-rb", "g896"))
COMEASURED = (("treatment-rb", "cycle3-g2048"), ("control-r", "cycle3-g2048"), ("static-rb", "cycle3-g2048"))
REQUIRED_MODELS = frozenset({"treatment-rb", "control-r", "static-rb", "g896", "cycle3-g2048"})
REPEATED_MODEL = "treatment-rb"
REGISTERED_ROOTS = 2048
VOID_CAP_FRACTION = 0.02
Z_ONE_SIDED_95 = 1.6448536269514722
POINT_FLOOR_PP = 2.0
HOLM_ALPHA = 0.05


class M1AnalysisError(Exception):
    pass


def read_group(group_root: str) -> dict:
    """Reads every shard under a group root.

    Returns {"models": {label: {pair: {"seed": hex, "legs": {seat: reward}}}},
             "voids": {label: set(pair)}, "shards": [shard dirs]}."""
    shard_dirs = sorted(d for d in glob.glob(os.path.join(group_root, "shard-*")) if os.path.isdir(d))
    if not shard_dirs:
        raise M1AnalysisError(f"{group_root}: no shard directories")
    models: dict[str, dict[int, dict]] = {}
    voids: dict[str, set[int]] = {}
    for shard in shard_dirs:
        summary_path = os.path.join(shard, "panel-summary.json")
        if not os.path.isfile(summary_path):
            raise M1AnalysisError(f"{shard}: no published panel-summary.json (shard not completed by the runner)")
        summary = json.load(open(summary_path, encoding="utf-8"))
        per_model = (summary.get("voids") or {}).get("per_model")
        if not isinstance(per_model, dict):
            raise M1AnalysisError(f"{shard}: panel-summary.json carries no voids.per_model map")
        for label, info in per_model.items():
            indices = info.get("voided_pair_indices")
            if not isinstance(indices, list):
                raise M1AnalysisError(f"{shard}: {label} voids carry no voided_pair_indices list")
            voids.setdefault(label, set()).update(int(p) for p in indices)
        for path in sorted(glob.glob(os.path.join(shard, "tasks", "*.outcome.jsonl"))):
            label = os.path.basename(path).split("-p")[0]
            per = models.setdefault(label, {})
            with open(path, encoding="utf-8") as handle:
                for line in handle:
                    rec = json.loads(line)
                    if rec.get("record_type") != "terminal":
                        continue
                    pair = int(rec["pair_index"])
                    seat = rec["candidate_seat"]
                    reward = int(rec["candidate_terminal_reward"])
                    entry = per.setdefault(pair, {"seed": rec["pair_environment_seed_u64_hex"], "legs": {}})
                    if entry["seed"] != rec["pair_environment_seed_u64_hex"]:
                        raise M1AnalysisError(f"{label}: root {pair} carries two environment seeds")
                    if seat in entry["legs"]:
                        raise M1AnalysisError(f"{label}: root {pair} seat {seat} recorded twice")
                    entry["legs"][seat] = reward
    for label in models:
        voids.setdefault(label, set())
    return {"models": models, "voids": voids, "shards": shard_dirs}


def validate_universe(models: dict, voids: dict, registered_roots: int) -> dict:
    """Every model: outcomes on exactly the non-voided roots of range(N), both
    legs present, void count within the registered cap."""
    cap = int(math.floor(VOID_CAP_FRACTION * registered_roots))
    report = {}
    universe = set(range(registered_roots))
    for label, per in models.items():
        voided = voids.get(label, set())
        if not voided <= universe:
            raise M1AnalysisError(f"{label}: voided roots outside the registered universe")
        with_outcomes = set(per)
        if with_outcomes & voided:
            raise M1AnalysisError(f"{label}: outcome counted for a voided root {sorted(with_outcomes & voided)[:3]}")
        if with_outcomes | voided != universe:
            missing = sorted(universe - with_outcomes - voided)[:5]
            extra = sorted((with_outcomes | voided) - universe)[:5]
            raise M1AnalysisError(f"{label}: root universe is not exactly range({registered_roots}); missing {missing} extra {extra}")
        incomplete = [p for p, e in per.items() if set(e["legs"]) != {"p0", "p1"}]
        if incomplete:
            raise M1AnalysisError(f"{label}: roots without both legs {sorted(incomplete)[:5]}")
        if len(voided) > cap:
            raise M1AnalysisError(f"{label}: {len(voided)} voided roots exceed the registered cap of {cap} ({VOID_CAP_FRACTION:.0%} of {registered_roots})")
        report[label] = {"roots_with_outcomes": len(with_outcomes), "voided_roots": len(voided), "void_cap": cap, "void_cap_ok": True}
    return report


def merge_groups(a: dict, b: dict, registered_roots: int) -> tuple[dict, dict, dict]:
    repeated = set(a["models"]) & set(b["models"])
    if repeated != {REPEATED_MODEL}:
        raise M1AnalysisError(f"repeated model set must be exactly {{{REPEATED_MODEL}}}, got {sorted(repeated)}")
    ra, rb = a["models"][REPEATED_MODEL], b["models"][REPEATED_MODEL]
    if set(ra) != set(rb):
        raise M1AnalysisError(f"{REPEATED_MODEL}: root coverage differs between groups")
    if a["voids"][REPEATED_MODEL] != b["voids"][REPEATED_MODEL]:
        raise M1AnalysisError(f"{REPEATED_MODEL}: voided roots differ between groups")
    for p in ra:
        if ra[p] != rb[p]:
            raise M1AnalysisError(f"{REPEATED_MODEL}: outcomes differ between groups at root {p}")
    seeds_a = {}
    for per in a["models"].values():
        for p, e in per.items():
            seeds_a.setdefault(p, e["seed"])
            if seeds_a[p] != e["seed"]:
                raise M1AnalysisError(f"group A models disagree on root {p} environment seed")
    for per in b["models"].values():
        for p, e in per.items():
            if p in seeds_a and seeds_a[p] != e["seed"]:
                raise M1AnalysisError(f"groups disagree on root {p} environment seed")
    models = dict(a["models"])
    voids = dict(a["voids"])
    for label, per in b["models"].items():
        models.setdefault(label, per)
        voids.setdefault(label, b["voids"][label])
    missing = REQUIRED_MODELS - set(models)
    if missing:
        raise M1AnalysisError(f"missing models: {sorted(missing)}")
    extra = set(models) - REQUIRED_MODELS
    if extra:
        raise M1AnalysisError(f"unexpected models: {sorted(extra)}")
    universe = validate_universe(models, voids, registered_roots)
    return models, voids, {"repeated_model": REPEATED_MODEL, "repeated_roots_identical": len(ra), "universe": universe}


def root_score(legs: dict[str, int]) -> float:
    return sum((r + 1) / 2.0 for r in legs.values()) / 2.0


def contrast(models: dict, voids: dict, endpoint: str, reference: str) -> dict:
    a, b = models[endpoint], models[reference]
    excluded = voids[endpoint] | voids[reference]
    roots = sorted((set(a) & set(b)) - excluded)
    deltas = []
    for p in roots:
        if a[p]["seed"] != b[p]["seed"]:
            raise M1AnalysisError(f"{endpoint} vs {reference}: root {p} environment seeds differ")
        deltas.append(root_score(a[p]["legs"]) - root_score(b[p]["legs"]))
    n = len(deltas)
    if n < 2:
        raise M1AnalysisError(f"{endpoint} vs {reference}: too few paired roots ({n})")
    mean = sum(deltas) / n
    var = sum((d - mean) ** 2 for d in deltas) / (n - 1)
    se = math.sqrt(var / n)
    point_pp, se_pp = 100.0 * mean, 100.0 * se
    lcb_pp = point_pp - Z_ONE_SIDED_95 * se_pp
    if se_pp > 0:
        z = point_pp / se_pp
        p_one_sided = 0.5 * math.erfc(z / math.sqrt(2))
    else:
        p_one_sided = 0.0 if point_pp > 0 else 1.0
    return {
        "endpoint": endpoint, "reference": reference, "paired_roots": n, "excluded_voided_roots": len(excluded),
        "point_pp": point_pp, "se_pp": se_pp, "one_sided_lcb95_pp": lcb_pp, "p_one_sided": p_one_sided,
    }


def gate(c: dict) -> bool:
    return bool(c["one_sided_lcb95_pp"] > 0.0 and c["point_pp"] >= POINT_FLOOR_PP)


def winrate(models: dict, voids: dict, label: str) -> dict:
    wins = draws = losses = 0
    for p, entry in models[label].items():
        if p in voids[label]:
            continue
        for r in entry["legs"].values():
            if r > 0:
                wins += 1
            elif r < 0:
                losses += 1
            else:
                draws += 1
    games = wins + draws + losses
    return {"wins": wins, "draws": draws, "losses": losses, "games": games,
            "winrate_half_credit_draws": (wins + 0.5 * draws) / games if games else None}


def analyze(group_a_root: str, group_b_root: str, registered_roots: int = REGISTERED_ROOTS) -> dict:
    a = read_group(group_a_root)
    b = read_group(group_b_root)
    models, voids, checks = merge_groups(a, b, registered_roots)
    primary = contrast(models, voids, *PRIMARY)
    primary["milestone"] = gate(primary)
    secondaries = [contrast(models, voids, e, r) for e, r in SECONDARIES]
    ranked = sorted(secondaries, key=lambda c: c["p_one_sided"])
    still_significant = True
    for i, c in enumerate(ranked):
        threshold = HOLM_ALPHA / (len(ranked) - i)
        c["holm_threshold"] = threshold
        c["holm_significant"] = bool(still_significant and c["p_one_sided"] <= threshold)
        if not c["holm_significant"]:
            still_significant = False
        c["milestone_under_holm"] = bool(c["holm_significant"] and c["point_pp"] >= POINT_FLOOR_PP)
    comeasured = [contrast(models, voids, e, r) for e, r in COMEASURED]
    return {
        "schema": "mtg-kernel-cycle4-m1-cp7-analysis/v1",
        "registered_roots": registered_roots,
        "shards": {"group_a": [os.path.basename(s) for s in a["shards"]], "group_b": [os.path.basename(s) for s in b["shards"]]},
        "checks": checks,
        "winrates_vs_cp7": {m: winrate(models, voids, m) for m in sorted(models)},
        "primary": primary,
        "secondaries": secondaries,
        "comeasured_vs_cycle3_g2048": comeasured,
        "non_claims": "M1 informs the continue/escalate decision; it cannot alter the routing record. Co-measured contrasts carry no gate.",
    }


def self_test() -> None:
    import random
    import tempfile

    def write_group(root, labels, roots, shard_pairs, shift, void=None):
        for s in range(roots // shard_pairs):
            shard = os.path.join(root, f"shard-{s:02d}")
            os.makedirs(os.path.join(shard, "tasks"))
            per_model = {}
            for label in labels:
                voided = [p for p in (void or {}).get(label, []) if s * shard_pairs <= p < (s + 1) * shard_pairs]
                per_model[label] = {"voided_pair_indices": voided}
                with open(os.path.join(shard, "tasks", f"{label}-p{s * shard_pairs:06d}-n{shard_pairs:03d}.outcome.jsonl"), "w", encoding="utf-8") as h:
                    h.write(json.dumps({"record_type": "header"}) + "\n")
                    for p in range(s * shard_pairs, (s + 1) * shard_pairs):
                        if p in voided:
                            continue
                        seed = format(p * 7919 + 17, "016x")
                        for seat in ("p0", "p1"):
                            draw = random.Random(f"{label}:{p}:{seat}").random()
                            reward = 1 if draw < 0.5 + shift.get(label, 0.0) else -1
                            h.write(json.dumps({"record_type": "terminal", "pair_index": p, "candidate_seat": seat,
                                                "candidate_terminal_reward": reward,
                                                "pair_environment_seed_u64_hex": seed}) + "\n")
            json.dump({"voids": {"per_model": per_model}}, open(os.path.join(shard, "panel-summary.json"), "w"))

    n, sp = 256, 128
    shift = {"treatment-rb": 0.2}
    with tempfile.TemporaryDirectory() as tmp:
        a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        write_group(a, ["treatment-rb", "control-r", "g896"], n, sp, shift, void={"g896": [3]})
        write_group(b, ["static-rb", "cycle3-g2048", "treatment-rb"], n, sp, shift)
        out = analyze(a, b, registered_roots=n)
        assert out["primary"]["paired_roots"] == n - 1, out["primary"]
        assert out["primary"]["point_pp"] > 0
        assert "milestone" in out["primary"] and all("milestone" not in c for c in out["comeasured_vs_cycle3_g2048"])
        assert all("milestone_under_holm" in c and "milestone" not in c for c in out["secondaries"])
        assert out["winrates_vs_cp7"]["g896"]["games"] == 2 * (n - 1)
    # Refusals: a missing summary, a void over the cap, a repeated-model mismatch.
    with tempfile.TemporaryDirectory() as tmp:
        a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        write_group(a, ["treatment-rb", "control-r", "g896"], n, sp, shift)
        write_group(b, ["static-rb", "cycle3-g2048", "treatment-rb"], n, sp, shift)
        os.remove(os.path.join(b, "shard-01", "panel-summary.json"))
        try:
            analyze(a, b, registered_roots=n); raise AssertionError("missing summary must refuse")
        except M1AnalysisError:
            pass
    with tempfile.TemporaryDirectory() as tmp:
        a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        write_group(a, ["treatment-rb", "control-r", "g896"], n, sp, shift, void={"control-r": list(range(6))})
        write_group(b, ["static-rb", "cycle3-g2048", "treatment-rb"], n, sp, shift)
        try:
            analyze(a, b, registered_roots=n); raise AssertionError("void cap must refuse")
        except M1AnalysisError as error:
            assert "cap" in str(error)
    with tempfile.TemporaryDirectory() as tmp:
        a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        write_group(a, ["treatment-rb", "control-r", "g896"], n, sp, shift)
        write_group(b, ["static-rb", "cycle3-g2048", "treatment-rb"], n, sp, {"treatment-rb": 0.1})
        try:
            analyze(a, b, registered_roots=n); raise AssertionError("repeat mismatch must refuse")
        except M1AnalysisError as error:
            assert "differ between groups" in str(error)
    print("self-test ok")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--group-a")
    parser.add_argument("--group-b")
    parser.add_argument("--output")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    if not (args.group_a and args.group_b and args.output):
        parser.error("--group-a, --group-b and --output are required")
    try:
        result = analyze(args.group_a, args.group_b)
    except M1AnalysisError as error:
        print(f"analyze_m1_cp7_panel_v1: REFUSED: {error}")
        return 3
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2, sort_keys=True)
    p = result["primary"]
    print(f"primary {p['endpoint']} vs {p['reference']}: point {p['point_pp']:+.2f} pp, LCB95 {p['one_sided_lcb95_pp']:+.2f} pp, roots {p['paired_roots']}, milestone {p['milestone']}")
    for c in result["secondaries"]:
        print(f"secondary {c['endpoint']} vs {c['reference']}: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}, Holm {c['holm_significant']}, milestone {c['milestone_under_holm']}")
    for c in result["comeasured_vs_cycle3_g2048"]:
        print(f"co-measured {c['endpoint']} vs cycle3-g2048: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}")
    for m, w in result["winrates_vs_cp7"].items():
        print(f"winrate vs CP7 {m}: {w['winrate_half_credit_draws']:.4f} over {w['games']} games")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
