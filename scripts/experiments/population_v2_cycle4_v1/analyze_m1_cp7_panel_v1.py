"""Cycle-4 M1 CP7 transfer analysis (pre-registration sections M1 and 4).

Reads the outcome JSONL shards of the M1 CP7 panel (two invocations of
run_cp7_store_panel_v2.py on the same base seed), pairs every model's games by
root (pair index), and computes the pre-registered within-panel CRN-paired
contrasts with the fixed-N root-cluster estimator:

  one inferential unit = one root = both seat-swapped legs;
  root score of a model = mean over its two legs of the candidate terminal
  reward mapped to a win indicator (win 1, draw 0.5, loss 0);
  paired delta per root = score(endpoint) - score(reference);
  point = mean delta in percentage points; SE = sample sd / sqrt(N roots);
  one-sided 95 percent lower bound = point - 1.645 * SE (fixed N, no anytime
  correction); milestone per endpoint = LCB > 0 AND point >= +2.0 pp.

Primary hypothesis: TREATMENT-RB vs g896. Secondaries under Holm (over the
two remaining endpoints vs g896): CONTROL-R vs g896, STATIC-RB vs g896. The
cycle-3 g2048 contrasts (each endpoint vs cycle3-g2048) are reported as the
co-measured comparison, not as milestones. Roots voided for any model in a
contrast are dropped from that contrast (pre-registered void tolerance, cap
2 percent per model, enforced by the runner). The analyzer refuses to
proceed if the two invocations disagree on any root's environment seed or if
the repeated model's outcomes differ between invocations.

Written and reviewed BEFORE any M1 outcome was read (routing record
f9ad13e9... precedes it). usage:
  analyze_m1_cp7_panel_v1.py --root-a <evidence root A> --root-b <evidence root B> --output <json> [--self-test]
"""
from __future__ import annotations

import argparse
import glob
import json
import math
import os
import sys

PRIMARY = ("treatment-rb", "g896")
SECONDARIES = (("control-r", "g896"), ("static-rb", "g896"))
COMEASURED = (("treatment-rb", "cycle3-g2048"), ("control-r", "cycle3-g2048"), ("static-rb", "cycle3-g2048"))
Z_ONE_SIDED_95 = 1.6448536269514722
POINT_FLOOR_PP = 2.0
EXPECTED_ROOTS = 2048


def read_terminals(evidence_root: str) -> dict[str, dict[int, dict[str, object]]]:
    """model label -> pair_index -> {"seed": hex, "legs": {seat: reward}}."""
    out: dict[str, dict[int, dict[str, object]]] = {}
    for path in sorted(glob.glob(os.path.join(evidence_root, "tasks", "*.outcome.jsonl"))):
        label = os.path.basename(path).split("-p")[0]
        per = out.setdefault(label, {})
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
                    raise SystemExit(f"{label}: pair {pair} carries two environment seeds")
                if seat in entry["legs"]:
                    raise SystemExit(f"{label}: pair {pair} seat {seat} recorded twice")
                entry["legs"][seat] = reward
    return out


def read_voids(evidence_root: str) -> dict[str, set[int]]:
    """model label -> voided pair indices, from panel-summary.json."""
    summary_path = os.path.join(evidence_root, "panel-summary.json")
    voids: dict[str, set[int]] = {}
    if not os.path.exists(summary_path):
        return voids
    summary = json.load(open(summary_path, encoding="utf-8"))
    entries = summary.get("voids") or {}
    if isinstance(entries, dict):
        for label, info in entries.items():
            if isinstance(info, dict):
                voids[label] = set(int(p) for p in info.get("voided_pair_indices", []))
    return voids


def root_score(legs: dict[str, int]) -> float | None:
    if set(legs) != {"p0", "p1"}:
        return None
    return sum((r + 1) / 2.0 for r in legs.values()) / 2.0


def contrast(models, voids, endpoint, reference):
    a = models[endpoint]; b = models[reference]
    common = sorted(set(a) & set(b))
    dropped = {p for p in common if p in voids.get(endpoint, set()) or p in voids.get(reference, set())}
    deltas = []
    for p in common:
        if p in dropped:
            continue
        sa = root_score(a[p]["legs"]); sb = root_score(b[p]["legs"])
        if sa is None or sb is None:
            dropped.add(p); continue
        if a[p]["seed"] != b[p]["seed"]:
            raise SystemExit(f"{endpoint} vs {reference}: root {p} environment seeds differ (roots not common)")
        deltas.append(sa - sb)
    n = len(deltas)
    if n < 2:
        raise SystemExit(f"{endpoint} vs {reference}: too few paired roots ({n})")
    mean = sum(deltas) / n
    var = sum((d - mean) ** 2 for d in deltas) / (n - 1)
    se = math.sqrt(var / n)
    point_pp = 100.0 * mean; se_pp = 100.0 * se
    lcb_pp = point_pp - Z_ONE_SIDED_95 * se_pp
    z = point_pp / se_pp if se_pp > 0 else float("inf")
    p_one_sided = 0.5 * math.erfc(z / math.sqrt(2)) if math.isfinite(z) else 0.0
    return {
        "endpoint": endpoint, "reference": reference, "paired_roots": n, "dropped_roots": len(dropped),
        "point_pp": point_pp, "se_pp": se_pp, "one_sided_lcb95_pp": lcb_pp, "p_one_sided": p_one_sided,
        "milestone": bool(lcb_pp > 0.0 and point_pp >= POINT_FLOOR_PP),
    }


def winrate(models, label):
    w = d = l = 0
    for entry in models[label].values():
        for r in entry["legs"].values():
            if r > 0: w += 1
            elif r < 0: l += 1
            else: d += 1
    return {"wins": w, "draws": d, "losses": l, "winrate": w / max(1, w + d + l), "games": w + d + l}


def analyze(root_a: str, root_b: str) -> dict:
    a = read_terminals(root_a); b = read_terminals(root_b)
    voids = read_voids(root_a)
    for label, v in read_voids(root_b).items():
        voids[label] = voids.get(label, set()) | v
    # Cross-invocation checks: common roots and the repeated model's outcomes.
    repeated = sorted(set(a) & set(b))
    for label in repeated:
        for p in sorted(set(a[label]) & set(b[label])):
            if a[label][p] != b[label][p]:
                raise SystemExit(f"repeated model {label} differs between invocations at root {p}")
    seeds_a = {p: e["seed"] for m in a.values() for p, e in m.items()}
    seeds_b = {p: e["seed"] for m in b.values() for p, e in m.items()}
    for p in set(seeds_a) & set(seeds_b):
        if seeds_a[p] != seeds_b[p]:
            raise SystemExit(f"invocations disagree on root {p} environment seed")
    models = dict(a)
    for label, per in b.items():
        models.setdefault(label, per)
    needed = {"treatment-rb", "control-r", "static-rb", "g896", "cycle3-g2048"}
    missing = needed - set(models)
    if missing:
        raise SystemExit(f"missing models: {sorted(missing)}")
    result = {
        "schema": "mtg-kernel-cycle4-m1-cp7-analysis/v1",
        "expected_roots": EXPECTED_ROOTS,
        "roots_per_model": {m: len(models[m]) for m in sorted(models)},
        "repeated_models_identical_across_invocations": repeated,
        "winrates_vs_cp7": {m: winrate(models, m) for m in sorted(models)},
        "primary": contrast(models, voids, *PRIMARY),
        "secondaries": [contrast(models, voids, e, r) for e, r in SECONDARIES],
        "comeasured_vs_cycle3_g2048": [contrast(models, voids, e, r) for e, r in COMEASURED],
    }
    # Holm over the two secondaries (one-sided p-values), alpha 0.05.
    secs = sorted(result["secondaries"], key=lambda c: c["p_one_sided"])
    alpha = 0.05; holm_pass = True
    for i, c in enumerate(secs):
        threshold = alpha / (len(secs) - i)
        c["holm_threshold"] = threshold
        c["holm_significant"] = bool(holm_pass and c["p_one_sided"] <= threshold)
        if not c["holm_significant"]:
            holm_pass = False
        c["milestone_under_holm"] = bool(c["holm_significant"] and c["point_pp"] >= POINT_FLOOR_PP)
    return result


def self_test() -> None:
    import random, tempfile
    rng = random.Random(7)
    def write(root, models, n=64, shift=None):
        os.makedirs(os.path.join(root, "tasks"), exist_ok=True)
        for label in models:
            with open(os.path.join(root, "tasks", f"{label}-p000000-n{n:03d}.outcome.jsonl"), "w", encoding="utf-8") as h:
                h.write(json.dumps({"record_type": "header"}) + "\n")
                for p in range(n):
                    seed = format(p * 7919 + 17, "016x")
                    for seat in ("p0", "p1"):
                        # Deterministic per (model, root, seat), so a model repeated
                        # across invocations reproduces its outcomes exactly.
                        base = random.Random(f"{label}:{p}:{seat}").random()
                        prob = 0.5 + (shift or {}).get(label, 0.0)
                        reward = 1 if base < prob else -1
                        h.write(json.dumps({"record_type": "terminal", "pair_index": p, "candidate_seat": seat,
                                            "candidate_terminal_reward": reward, "pair_environment_seed_u64_hex": seed}) + "\n")
    with tempfile.TemporaryDirectory() as tmp:
        a = os.path.join(tmp, "a"); b = os.path.join(tmp, "b")
        rng = random.Random(7)
        write(a, ["treatment-rb", "control-r", "g896"], shift={"treatment-rb": 0.2})
        rng = random.Random(7)
        write(b, ["static-rb", "cycle3-g2048", "treatment-rb"], shift={"treatment-rb": 0.2})
        out = analyze(a, b)
        assert out["primary"]["paired_roots"] == 64
        assert out["primary"]["point_pp"] > 0 and out["primary"]["one_sided_lcb95_pp"] < out["primary"]["point_pp"]
        assert "treatment-rb" in out["repeated_models_identical_across_invocations"]
        assert all(0 <= c["p_one_sided"] <= 1 for c in out["secondaries"])
    print("self-test ok")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root-a"); parser.add_argument("--root-b"); parser.add_argument("--output")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test(); return 0
    if not (args.root_a and args.root_b and args.output):
        parser.error("--root-a, --root-b and --output are required")
    result = analyze(args.root_a, args.root_b)
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2, sort_keys=True)
    p = result["primary"]
    print(f"primary {p['endpoint']} vs {p['reference']}: point {p['point_pp']:+.2f} pp, LCB95 {p['one_sided_lcb95_pp']:+.2f} pp, roots {p['paired_roots']}, milestone {p['milestone']}")
    for c in result["secondaries"]:
        print(f"secondary {c['endpoint']} vs {c['reference']}: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}, Holm significant {c['holm_significant']}, milestone {c['milestone_under_holm']}")
    for c in result["comeasured_vs_cycle3_g2048"]:
        print(f"co-measured {c['endpoint']} vs cycle3-g2048: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}")
    for m, w in result["winrates_vs_cp7"].items():
        print(f"winrate vs CP7 {m}: {w['winrate']:.4f} over {w['games']} games")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
