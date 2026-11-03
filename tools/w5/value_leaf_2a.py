"""W5 Stage 2a: the value-leaf panel read (collab LANES/lead/w5/VALUE-LEAF-DESIGN.md v1.3 section 5 and
VALUE-LEAF-RULINGS.md; work item V6). Measurement and decision tool; it reads terminal rollout outcomes of b512's
continuation (Q_ref) and value-leaf search statistics only: never a rating, panel score or CP7.

Inputs:
- the Stage 0b capture records (`stage0b.py`, `w5-0b-capture/v1`): the panel states, the policy, the frozen
  s4a candidates and Q_ref;
- value-leaf capture records (`w5-2a-vl-capture/v1`, JSON lines in DIR/vl-*.jsonl), one per panel state:
  {"schema", "episode", "game", "host_class", "binary_sha256",
   "configs": [{"horizon": "h0"|"t0"|"t1", "n_sims": N, "search_seed": 0|1, "seed_root", "status": "ok"|"failed",
                "cpu_s", "primary_sha256", and when ok "n": [...], "value_units": [...], "draws": [...]}],
   "comparator": [{"budget": B, "search_seed": 0|1, "cpu_s", "key_memo": "on"}],
   "qref_extra": {"rollouts": 128, "seed_root", "actions": [...], "n": [...], "wins": [...], "draws": [...]}}
  `comparator` re-measures the frozen s4a recipe's cost with the same key memo as the value-leaf searches, same
  host and binary class (ruling 2). `qref_extra` covers only actions outside the 0b Q_ref; an action in both keeps
  its frozen 0b value (ruling 8).

Rule (ruling 5, frozen here):
- Halves: panel states ordered by SHA-256 of compact json(["w5/2a/half", episode, game]); the first 150 are the
  selection half, the rest the confirmation half.
- Selection (selection half only): q from `vl-target-v1` (ex1-target-v1 with Q-hat = value units / (n x 2^20));
  G and c per config as in Stage 0b; eligible iff G's bootstrap 10th percentile > 0 (seed tag "w5/2a/select-boot");
  the selected config maximizes G/c at 4 significant figures, ties to the smaller N, then horizon order h0, t0, t1.
  `select` writes a lock; `confirm` refuses without it and recomputes it.
- Confirmation (confirmation half): pass iff (a) G_vl's 10th percentile > 0, (b) G_vl/c_vl >= 3 G_s/c_s and
  G_vl >= 0.5 G_s at point estimates, and (c) the binding interval clause: the paired state-bootstrap 10th
  percentile of (G_vl/c_vl)/(G_s/c_s) > 1 (seed tag "w5/2a/boot"). A nonpositive comparator G makes the
  multiplier claim inconclusive. Paired intervals are reported for G_vl/c_vl - 3 G_s/c_s and G_vl - 0.5 G_s.
- Class (ruling 3): a read is binding only in the node class (`--binding-class`, default linux-zen5-9950x);
  any other class is labelled a development read. `crossclass` compares primary hashes of the same searches
  run in two classes.

  value_leaf_2a.py select  --captures 0B_DIR --vl VL_DIR --parent-sha256 H --budget B --lock LOCK.json
  value_leaf_2a.py confirm --captures 0B_DIR --vl VL_DIR --parent-sha256 H --budget B --lock LOCK.json [--out F]
  value_leaf_2a.py crossclass --vl VL_DIR --other OTHER_VL_DIR
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import stage0b as s0  # noqa: E402
import target_ref  # noqa: E402  (stage0b puts tools/ex1 on the path)

VL_SCHEMA = "w5-2a-vl-capture/v1"
LOCK_SCHEMA = "w5-2a-lock/v1"
RESULT_SCHEMA = "w5-2a-result/v1"
UNIT = 1 << 20
HORIZONS = ("h0", "t0", "t1")
HALF_TAG = "w5/2a/half"
SELECT_BOOT_TAG = "w5/2a/select-boot"
BOOT_TAG = "w5/2a/boot"
SELECTION_SIZE = 150
BINDING_CLASS = "linux-zen5-9950x"
EFFICIENCY_MULTIPLE = 3.0
GAIN_FRACTION = 0.5
require = s0.require
StageError = s0.StageError


def config_key(c: dict) -> str:
    return f"{c['horizon']}-n{c['n_sims']}"


def config_order(key: str) -> tuple:
    horizon, n = key.split("-n")
    return int(n), HORIZONS.index(horizon)


def vl_target(logit_bits, n, units):
    """ex1-target-v1 with Q-hat = value units / (n x 2^20): the complete target, or ("no_target", reason)."""
    k = len(logit_bits)
    p, logp, _ = target_ref.softmax([target_ref.f32(b) for b in logit_bits])
    support = [a for a in range(k) if n[a] >= target_ref.SUPPORT_MIN]
    if len(support) < 2:
        return ("no_target", "support")
    mass = 0.0
    for a in support:
        mass += p[a]
    if mass < target_ref.MASS_FLOOR:
        return ("no_target", "mass")
    qh = [None] * k
    scaled = []
    for a in support:
        qh[a] = units[a] / (n[a] * UNIT)
        scaled.append(target_ref.INVERSE_TEMPERATURE * qh[a])
    sigma, log_sigma, _ = target_ref.softmax(scaled)
    log_mass = target_ref.ln_v1(mass)
    q, logq = list(p), list(logp)
    for i, a in enumerate(support):
        q[a] = mass * sigma[i]
        logq[a] = log_mass + log_sigma[i]
    return {"q": q, "log_q": logq, "p": p, "mass": mass, "support": support, "qhat": qh}


def validate_vl(vl: dict, panel_record: dict, budget: int) -> None:
    where = f"value-leaf episode {vl.get('episode')}"
    require(vl.get("schema") == VL_SCHEMA, f"{where}: wrong schema")
    require(vl.get("episode") == panel_record["episode"] and vl.get("game") == panel_record["game"],
            f"{where}: does not match its panel state")
    require(isinstance(vl.get("host_class"), str) and vl["host_class"], f"{where}: host class")
    width = len(panel_record["policy"]["logit_bits"])
    keys = set()
    for c in vl["configs"]:
        require(c.get("horizon") in HORIZONS and s0.is_count(c.get("n_sims")) and c["n_sims"] > 0,
                f"{where}: config")
        require(c.get("search_seed") in s0.SEEDS and isinstance(c.get("seed_root"), str), f"{where}: seed")
        require(c.get("status") in ("ok", "failed"), f"{where}: status")
        cpu = c.get("cpu_s")
        require(type(cpu) in (int, float) and math.isfinite(cpu) and cpu >= 0, f"{where}: CPU")
        if c["status"] == "ok":
            n, u, d = c["n"], c["value_units"], c["draws"]
            require(len(n) == len(u) == len(d) == width, f"{where}: widths")
            require(all(s0.is_count(x) for x in n + u + d), f"{where}: counts")
            require(all(ui <= ni * UNIT and di <= ni for ni, ui, di in zip(n, u, d)), f"{where}: sums exceed visits")
        keys.add((config_key(c), c["search_seed"]))
    require(len(keys) == len(vl["configs"]), f"{where}: duplicate config")
    configs = {k for k, _ in keys}
    require(keys == {(k, s) for k in configs for s in s0.SEEDS}, f"{where}: every config needs seeds 0 and 1")
    comp = {(c.get("budget"), c.get("search_seed")) for c in vl["comparator"]}
    require(comp == {(budget, s) for s in s0.SEEDS} and len(comp) == len(vl["comparator"]),
            f"{where}: comparator must be budget {budget} at seeds 0 and 1")
    require(all(c.get("key_memo") == "on" for c in vl["comparator"]), f"{where}: comparator key memo must be on")
    extra = vl.get("qref_extra") or {"actions": [], "n": [], "wins": [], "draws": [], "rollouts": s0.ROLLOUTS}
    s0.check_counts(extra["n"], extra["wins"], extra["draws"], len(extra["actions"]), f"{where} extra Q_ref")
    require(extra.get("rollouts") == s0.ROLLOUTS and all(x == s0.ROLLOUTS for x in extra["n"]),
            f"{where}: extra Q_ref needs {s0.ROLLOUTS} rollouts per action")
    frozen = set(panel_record["qref"]["actions"])
    require(not frozen & set(extra["actions"]), f"{where}: an action with a frozen 0b Q_ref is re-measured")
    roots = {c["seed_root"] for c in vl["configs"]} | {panel_record["qref"]["seed_root"]}
    require(not extra["actions"] or extra.get("seed_root") not in roots, f"{where}: extra Q_ref seeds not independent")


def reference(panel_record: dict, vl: dict) -> dict:
    out = {}
    for src in (panel_record["qref"], vl.get("qref_extra") or {"actions": []}):
        for a, w, d, n in zip(src["actions"], src.get("wins", []), src.get("draws", []), src.get("n", [])):
            out[a] = (w + 0.5 * d) / n
    return out


def state_row(panel_record: dict, vl: dict, budget: int) -> dict:
    """Per config and for the comparator: the state's gain (mean over seeds 0 and 1) and attempted-search CPU."""
    bits = panel_record["policy"]["logit_bits"]
    p = s0.policy(panel_record)
    ref = reference(panel_record, vl)
    row = {}
    by = {}
    for c in vl["configs"]:
        by.setdefault(config_key(c), {})[c["search_seed"]] = c
    for key, seeds in by.items():
        gains, cpu = [], []
        for seed in s0.SEEDS:
            c = seeds[seed]
            cpu.append(float(c["cpu_s"]))
            if c["status"] != "ok":
                gains.append(0.0)
                continue
            t = vl_target(bits, c["n"], c["value_units"])
            if isinstance(t, tuple):
                gains.append(0.0)
                continue
            g = 0.0
            for a in range(len(p)):
                if t["q"][a] != p[a]:
                    require(a in ref, f"episode {panel_record['episode']}: {key} moves action {a} without Q_ref")
                    g += (t["q"][a] - p[a]) * ref[a]
            gains.append(g)
        row[key] = {"gain": (gains[0] + gains[1]) / 2, "cpu_s": cpu}
    s4a = s0.state_gains(panel_record, (budget,))[budget]
    row["s4a"] = {"gain": s4a["gain"], "cpu_s": [float(c["cpu_s"]) for c in
                                                 sorted(vl["comparator"], key=lambda c: c["search_seed"])]}
    return row


def halves(panel: list[dict]) -> tuple[list[dict], list[dict]]:
    def h(r):
        text = json.dumps([HALF_TAG, r["episode"], r["game"]], separators=(",", ":"))
        return hashlib.sha256(text.encode("utf-8")).hexdigest()
    ordered = sorted(panel, key=h)
    return ordered[:SELECTION_SIZE], ordered[SELECTION_SIZE:]


def load_vl(directory: Path) -> dict:
    out = {}
    for path in sorted(Path(directory).glob("vl-*.jsonl")):
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                require(r.get("episode") not in out, f"duplicate value-leaf record for episode {r.get('episode')}")
                out[r["episode"]] = r
    return out


def rows_for(states: list[dict], vl: dict, budget: int) -> tuple[list[dict], set]:
    rows, classes = [], set()
    for r in states:
        require(r["episode"] in vl, f"episode {r['episode']}: no value-leaf record")
        validate_vl(vl[r["episode"]], r, budget)
        classes.add(vl[r["episode"]]["host_class"])
        rows.append(state_row(r, vl[r["episode"]], budget))
    keys = {k for row in rows for k in row}
    require(all(set(row) == keys for row in rows), "every state needs the same configs")
    return rows, classes


def mean(xs):
    return sum(xs) / len(xs)


def resample(n: int, seed: int, replicates: int):
    rng = s0.SplitMix64(seed)
    for _ in range(replicates):
        yield [s0.unbiased_index(n, rng.next_u64) for _ in range(n)]


def summarize(rows: list[dict], key: str) -> dict:
    return {"G": mean([r[key]["gain"] for r in rows]), "c": mean([c for r in rows for c in r[key]["cpu_s"]])}


def select_rows(rows: list[dict], replicates: int) -> dict:
    configs = sorted((k for k in rows[0] if k != "s4a"), key=config_order)
    boots = {k: [] for k in configs}
    for idx in resample(len(rows), s0.tag_seed(SELECT_BOOT_TAG), replicates):
        for k in configs:
            boots[k].append(mean([rows[i][k]["gain"] for i in idx]))
    summary = {}
    for k in configs:
        s = summarize(rows, k)
        s["lower_10pct"] = sorted(boots[k])[s0.lower_index(replicates)]
        s["eligible"] = s["lower_10pct"] > 0
        s["ratio_4sf"] = s0.sig4(s["G"] / s["c"])
        summary[k] = s
    eligible = [k for k in configs if summary[k]["eligible"]]
    if not eligible:
        return {"selected": None, "summary": summary, "reason": "no value-leaf config has G's 10th percentile > 0"}
    best = max(eligible, key=lambda k: (float(summary[k]["ratio_4sf"]), [-x for x in config_order(k)]))
    return {"selected": best, "summary": summary}


def panel_and_vl(args) -> tuple[list[dict], dict]:
    records = s0.load_records(args.captures)
    panel = s0.draw_panel(records, s0.PANEL, s0.PANEL_NAMESPACE, args.parent_sha256)
    return panel, load_vl(args.vl)


def select(panel: list[dict], vl: dict, budget: int, replicates: int = s0.REPLICATES) -> dict:
    """Selection on the selection half only; the confirmation half's records are not read."""
    sel_half, _ = halves(panel)
    rows, classes = rows_for(sel_half, vl, budget)
    out = select_rows(rows, replicates)
    ids = [[r["episode"], r["game"]] for r in sel_half]
    out.update({"schema": LOCK_SCHEMA, "budget": budget, "replicates": replicates, "host_classes": sorted(classes),
                "selection_half_sha256": hashlib.sha256(json.dumps(ids).encode()).hexdigest()})
    body = json.dumps(out, sort_keys=True)
    out["lock_sha256"] = hashlib.sha256(body.encode()).hexdigest()
    return out


def confirm(panel: list[dict], vl: dict, budget: int, lock: dict, binding_class: str = BINDING_CLASS,
            replicates: int = s0.REPLICATES) -> dict:
    require(lock.get("schema") == LOCK_SCHEMA and lock.get("budget") == budget, "not a lock for this budget")
    again = select(panel, vl, budget, lock["replicates"])
    require(again["lock_sha256"] == lock["lock_sha256"], "the lock does not match the selection half's records")
    key = lock["selected"]
    result = {"schema": RESULT_SCHEMA, "budget": budget, "selected": key, "lock_sha256": lock["lock_sha256"]}
    if key is None:
        result.update(verdict="fail", reason=lock["reason"])
        return result
    _, conf_half = halves(panel)
    rows, classes = rows_for(conf_half, vl, budget)
    v, s = summarize(rows, key), summarize(rows, "s4a")
    eff_v, eff_s = v["G"] / v["c"], s["G"] / s["c"]
    ratios, d_eff, d_gain, g_v = [], [], [], []
    for idx in resample(len(rows), s0.tag_seed(BOOT_TAG), replicates):
        gv = mean([rows[i][key]["gain"] for i in idx])
        cv = mean([c for i in idx for c in rows[i][key]["cpu_s"]])
        gs = mean([rows[i]["s4a"]["gain"] for i in idx])
        cs = mean([c for i in idx for c in rows[i]["s4a"]["cpu_s"]])
        g_v.append(gv)
        d_eff.append(gv / cv - EFFICIENCY_MULTIPLE * gs / cs)
        d_gain.append(gv - GAIN_FRACTION * gs)
        ratios.append((gv / cv) / (gs / cs) if gs > 0 else -math.inf)
    lo = s0.lower_index(replicates)

    def interval(xs):
        xs = sorted(xs)
        return {"p2.5": xs[int(0.025 * (len(xs) - 1))], "p10": xs[lo], "p90": xs[len(xs) - 1 - lo],
                "p97.5": xs[int(0.975 * (len(xs) - 1))]}

    comparator_positive = s["G"] > 0
    clauses = {"g_vl_lower_10pct_positive": sorted(g_v)[lo] > 0,
               "efficiency_point": comparator_positive and eff_v >= EFFICIENCY_MULTIPLE * eff_s,
               "gain_point": v["G"] >= GAIN_FRACTION * s["G"],
               "binding_ratio_lower_10pct_above_1": comparator_positive and sorted(ratios)[lo] > 1}
    if not comparator_positive:
        verdict = "inconclusive"
    else:
        verdict = "pass" if all(clauses.values()) else "fail"
    binding = classes == {binding_class}
    result.update({"vl": v, "s4a": s, "efficiency_vl": eff_v, "efficiency_s4a": eff_s,
                   "comparator_positive": comparator_positive, "clauses": clauses, "verdict": verdict,
                   "interval_efficiency_minus_3x": interval(d_eff), "interval_gain_minus_half": interval(d_gain),
                   "interval_efficiency_ratio": interval(ratios) if comparator_positive else None,
                   "host_classes": sorted(classes), "binding": binding,
                   "label": "binding read" if binding else "development read (not binding: host class)",
                   "law": "development measure on terminal rollout outcomes of b512's continuation; "
                          "no rating, panel score or CP7 read"})
    return result


def crossclass(vl_a: dict, vl_b: dict) -> dict:
    """Primary-hash agreement of the same searches (episode, config, seed) run in two host classes."""
    same, differ = 0, []
    for ep in sorted(set(vl_a) & set(vl_b)):
        a = {(config_key(c), c["search_seed"]): c.get("primary_sha256") for c in vl_a[ep]["configs"]}
        b = {(config_key(c), c["search_seed"]): c.get("primary_sha256") for c in vl_b[ep]["configs"]}
        for k in sorted(set(a) & set(b)):
            if a[k] is not None and a[k] == b[k]:
                same += 1
            else:
                differ.append([ep, k[0], k[1]])
    total = same + len(differ)
    return {"compared": total, "equal": same, "differ": differ, "all_equal": total > 0 and not differ}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("select", "confirm"):
        p = sub.add_parser(name)
        p.add_argument("--captures", type=Path, required=True)
        p.add_argument("--vl", type=Path, required=True)
        p.add_argument("--parent-sha256", required=True)
        p.add_argument("--budget", type=int, required=True, choices=s0.BUDGETS, help="Stage 0b's frozen B")
        p.add_argument("--lock", type=Path, required=True)
        if name == "confirm":
            p.add_argument("--binding-class", default=BINDING_CLASS)
            p.add_argument("--out", type=Path)
    p = sub.add_parser("crossclass")
    p.add_argument("--vl", type=Path, required=True)
    p.add_argument("--other", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "crossclass":
            result = crossclass(load_vl(args.vl), load_vl(args.other))
        elif args.command == "select":
            require(not args.lock.exists(), "the lock exists; selection is already locked")
            panel, vl = panel_and_vl(args)
            result = select(panel, vl, args.budget)
            args.lock.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
        else:
            panel, vl = panel_and_vl(args)
            lock = json.loads(args.lock.read_text(encoding="utf-8"))
            result = confirm(panel, vl, args.budget, lock, args.binding_class)
            if args.out:
                args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    except StageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
