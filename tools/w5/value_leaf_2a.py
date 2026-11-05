"""W5 Stage 2a: the value-leaf panel read (collab LANES/lead/w5/VALUE-LEAF-DESIGN.md v1.3 section 5 and
VALUE-LEAF-RULINGS.md; work item V6). Measurement and decision tool; it reads terminal rollout outcomes of b512's
continuation (Q_ref) and value-leaf search statistics only: never a rating, panel score or CP7.

Inputs:
- the Stage 0b capture records (`stage0b.py`, `w5-0b-capture/v1`): the panel states, the policy, the frozen
  s4a candidates and Q_ref;
- value-leaf capture records (`w5-2a-vl-capture/v1`, JSON lines in DIR/vl-*.jsonl), one per panel state:
  {"schema", "episode", "game", "host_class", "binary_sha256", "key_memo": "on", "key_memo_limit_bytes",
   "configs": [{"horizon": "h0"|"t0"|"t1", "n_sims": N, "search_seed": 0|1, "seed_root", "status": "ok"|"failed",
                "cpu_s", "primary_sha256", and when ok "n": [...], "value_units": [...], "draws": [...]}],
   "comparator": [{"budget": B, "search_seed": 0|1, "cpu_s", "key_memo": "on", "key_memo_limit_bytes"}],
   "qref_extra": {"rollouts": 128, "seed_root", "actions": [...], "n": [...], "wins": [...], "draws": [...]}}
  `comparator` re-measures the frozen s4a recipe's cost with the same key memo and limit as the value-leaf
  searches, same host and binary class (ruling 2). Configs must be exactly the frozen horizons x N grid. `qref_extra` covers only actions outside the 0b Q_ref; an action in both keeps
  its frozen 0b value (ruling 8).

Rule (ruling 5, frozen here):
- Halves: panel states ordered by SHA-256 of compact json(["w5/2a/half", episode, game]); the first 150 are the
  selection half, the rest the confirmation half.
- Selection (selection half only): q from `vl-target-v1` (ex1-target-v1 with Q-hat = value units / (n x 2^20));
  G and c per config as in Stage 0b; eligible iff G's bootstrap 10th percentile > 0 (seed tag "w5/2a/select-boot");
  the selected config maximizes G/c at 4 significant figures, ties to the smaller N, then horizon order h0, t0, t1.
  `select` reads only identity fields (episode, game, placement, reached) of confirmation-half records and writes a
  lock; `confirm` checks the lock's hash and that a recomputation equals it field for field, then uses it.
- Confirmation (confirmation half): pass iff (a) G_vl's 10th percentile > 0, (b) G_vl/c_vl >= 3 G_s/c_s and
  G_vl >= 0.5 G_s at point estimates, and (c) the binding interval clause: the paired state-bootstrap 10th
  percentile of (G_vl/c_vl)/(G_s/c_s) > 1 (seed tag "w5/2a/boot"). A nonpositive comparator G makes the
  multiplier claim inconclusive; a bootstrap replicate whose comparator gain is not positive counts as -inf in the
  ratio. Paired intervals (floor-index percentiles) are reported for G_vl/c_vl - 3 G_s/c_s and G_vl - 0.5 G_s, and
  both s4a budgets' confirmation-half G/c at Stage 0b's capture costs as a disclosure.
- Class (ruling 3): a read is binding only when every record of both halves is in the node class
  (linux-zen5-9950x) with one binary and one memo limit; anything else is labelled a development read.
  `crossclass` compares primary hashes of named searches run in two classes (every one must be present and ok).

  value_leaf_2a.py select  --captures 0B_DIR --vl VL_DIR --parent-sha256 H --budget B --grid 128,512
                           --horizons h0,t0 --lock LOCK.json
  value_leaf_2a.py confirm --captures 0B_DIR --vl VL_DIR --parent-sha256 H --budget B --lock LOCK.json [--out F]
  value_leaf_2a.py crossclass --vl VL_DIR --other OTHER_VL_DIR --episodes 3,17,40
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


def finite_cost(x) -> bool:
    """A search's CPU: finite and positive (every attempted search, failed or not, costs CPU), so no bootstrap
    replicate can have a zero cost denominator."""
    return type(x) in (int, float) and math.isfinite(x) and x > 0


def validate_vl(vl: dict, panel_record: dict, budget: int, grid: tuple, horizons: tuple) -> None:
    where = f"value-leaf episode {vl.get('episode')}"
    require(vl.get("schema") == VL_SCHEMA, f"{where}: wrong schema")
    require(vl.get("episode") == panel_record["episode"] and vl.get("game") == panel_record["game"],
            f"{where}: does not match its panel state")
    require(isinstance(vl.get("host_class"), str) and vl["host_class"], f"{where}: host class")
    require(isinstance(vl.get("binary_sha256"), str) and len(vl["binary_sha256"]) == 64, f"{where}: binary hash")
    require(vl.get("key_memo") == "on" and s0.is_count(vl.get("key_memo_limit_bytes")) and vl["key_memo_limit_bytes"] > 0,
            f"{where}: value-leaf key memo must be on with its limit")
    width = len(panel_record["policy"]["logit_bits"])
    keys = set()
    for c in vl["configs"]:
        require(c.get("horizon") in horizons and c.get("n_sims") in grid, f"{where}: config outside the frozen grid")
        require(c.get("search_seed") in s0.SEEDS and isinstance(c.get("seed_root"), str) and c["seed_root"],
                f"{where}: seed")
        require(c.get("status") in ("ok", "failed"), f"{where}: status")
        require(finite_cost(c.get("cpu_s")), f"{where}: CPU")
        if c["status"] == "ok":
            n, u, d = c["n"], c["value_units"], c["draws"]
            require(len(n) == len(u) == len(d) == width, f"{where}: widths")
            require(all(s0.is_count(x) for x in n + u + d), f"{where}: counts")
            require(all(ui <= ni * UNIT and di <= ni for ni, ui, di in zip(n, u, d)), f"{where}: sums exceed visits")
        keys.add((c["horizon"], c["n_sims"], c["search_seed"]))
    require(len(keys) == len(vl["configs"]) and keys == {(h, n, s) for h in horizons for n in grid for s in s0.SEEDS},
            f"{where}: configs must be exactly the frozen horizons x grid at seeds 0 and 1")
    comp = vl["comparator"]
    require({(c.get("budget"), c.get("search_seed")) for c in comp} == {(budget, s) for s in s0.SEEDS}
            and len(comp) == len(s0.SEEDS), f"{where}: comparator must be budget {budget} at seeds 0 and 1")
    for c in comp:
        require(finite_cost(c.get("cpu_s")), f"{where}: comparator CPU")
        require(c.get("key_memo") == "on" and c.get("key_memo_limit_bytes") == vl["key_memo_limit_bytes"],
                f"{where}: the comparator must run the same key memo and limit as the value-leaf searches")
    extra = vl.get("qref_extra")
    require(isinstance(extra, dict), f"{where}: qref_extra required (empty actions allowed)")
    actions = extra.get("actions")
    require(isinstance(actions, list) and all(type(a) is int and 0 <= a < width for a in actions)
            and len(set(actions)) == len(actions), f"{where}: extra Q_ref actions must be distinct legal indices")
    s0.check_counts(extra.get("n"), extra.get("wins"), extra.get("draws"), len(actions), f"{where} extra Q_ref")
    require(extra.get("rollouts") == s0.ROLLOUTS and all(x == s0.ROLLOUTS for x in extra["n"]),
            f"{where}: extra Q_ref needs {s0.ROLLOUTS} rollouts per action")
    require(not set(panel_record["qref"]["actions"]) & set(actions),
            f"{where}: an action with a frozen 0b Q_ref is re-measured")
    if actions:
        roots = ({c["seed_root"] for c in vl["configs"]} | {panel_record["qref"]["seed_root"]}
                 | {c["seed_root"] for c in panel_record["candidates"]})
        require(isinstance(extra.get("seed_root"), str) and extra["seed_root"] and extra["seed_root"] not in roots,
                f"{where}: extra Q_ref seeds must be independent (a distinct, nonempty seed root)")


def reference(panel_record: dict, vl: dict) -> dict:
    out = {}
    for src in (panel_record["qref"], vl["qref_extra"]):
        for a, w, d, n in zip(src["actions"], src["wins"], src["draws"], src["n"]):
            out[a] = (w + 0.5 * d) / n
    return out


def state_row(panel_record: dict, vl: dict, budget: int) -> dict:
    """Per config and for the comparator: the state's gain (mean over seeds 0 and 1) and attempted-search CPU."""
    bits = panel_record["policy"]["logit_bits"]
    p = s0.policy(panel_record)
    ref = reference(panel_record, vl)
    row, by = {}, {}
    for c in vl["configs"]:
        by.setdefault(config_key(c), {})[c["search_seed"]] = c
    for key, seeds in by.items():
        gains, cpu = [], []
        for seed in s0.SEEDS:
            c = seeds[seed]
            cpu.append(float(c["cpu_s"]))
            t = vl_target(bits, c["n"], c["value_units"]) if c["status"] == "ok" else ("no_target", "failed")
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
    gains = s0.state_gains(panel_record, s0.BUDGETS)
    row["s4a"] = {"gain": gains[budget]["gain"],
                  "cpu_s": [float(c["cpu_s"]) for c in sorted(vl["comparator"], key=lambda c: c["search_seed"])]}
    for b in s0.BUDGETS:  # disclosure only: Stage 0b's capture costs (its own memo setting), both budgets
        row[f"s4a_{b}_0b"] = {"gain": gains[b]["gain"], "cpu_s": gains[b]["cpu_s"]}
    return row


def panel_identities(records: list[dict], namespace: str = s0.PANEL_NAMESPACE, source_sha256: str = "") -> list[dict]:
    """The Stage 0b panel (first 300 reached states, one per game) from identity fields only: episode, game,
    chain, checkpoint, placement and `reached`. No candidate, Q_ref or policy field is read here."""
    panel, games = [], set()
    for expected, r in enumerate(records):
        where = f"episode {r.get('episode')}"
        require(r.get("schema") == s0.CAPTURE_SCHEMA and r.get("episode") == expected, f"episode gap at {expected}")
        require(r.get("chain") == namespace and r.get("source_checkpoint_sha256") == source_sha256,
                f"{where}: namespace or checkpoint")
        require(all(s0.is_count(r.get(k)) for k in ("update", "seed", "slot", "k")) and type(r.get("reached")) is bool
                and isinstance(r.get("game"), str), f"{where}: identity fields")
        require(r["k"] == s0.placement_index(r["chain"], r["update"], r["seed"], r["slot"]),
                f"{where}: k differs from the placement hash")
        if not r["reached"]:
            continue
        require(r["game"] not in games, f"game {r['game']} gives two states")
        games.add(r["game"])
        panel.append(r)
        if len(panel) == s0.PANEL:
            return panel
    raise StageError(f"only {len(panel)} reached states; the panel needs {s0.PANEL}")


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


def rows_for(states: list[dict], vl: dict, parent_sha256: str, budget: int, grid: tuple, horizons: tuple):
    rows, provenance = [], set()
    for r in states:
        s0.validate_record(r, s0.PANEL_NAMESPACE, parent_sha256, s0.BUDGETS)
        require(r["episode"] in vl, f"episode {r['episode']}: no value-leaf record")
        v = vl[r["episode"]]
        validate_vl(v, r, budget, grid, horizons)
        provenance.add((v["host_class"], v["binary_sha256"], v["key_memo_limit_bytes"]))
        rows.append(state_row(r, v, budget))
    return rows, provenance


def mean(xs):
    return sum(xs) / len(xs)


def resample(n: int, seed: int, replicates: int):
    rng = s0.SplitMix64(seed)
    for _ in range(replicates):
        yield [s0.unbiased_index(n, rng.next_u64) for _ in range(n)]


def summarize(rows: list[dict], key: str) -> dict:
    cpu = [c for r in rows for c in r[key]["cpu_s"]]
    require(sum(cpu) > 0, f"{key}: no positive search CPU recorded")
    return {"G": mean([r[key]["gain"] for r in rows]), "c": mean(cpu)}


def select_rows(rows: list[dict], grid: tuple, horizons: tuple, replicates: int) -> dict:
    configs = [f"{h}-n{n}" for n in grid for h in horizons]
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
    # Configs are listed by N, then horizon order: the first maximum is the tie rule (smaller N, then h0, t0, t1).
    best = max(eligible, key=lambda k: (float(summary[k]["ratio_4sf"]), -configs.index(k)))
    return {"selected": best, "summary": summary}


def select(records: list[dict], vl: dict, parent_sha256: str, budget: int, grid: tuple, horizons: tuple,
           replicates: int = s0.REPLICATES) -> dict:
    """The lock. Reads the selection half's records only; of the confirmation half, identity fields alone."""
    require(budget in s0.BUDGETS, "budget must be Stage 0b's frozen B")
    grid, horizons = tuple(sorted(set(grid))), tuple(h for h in HORIZONS if h in set(horizons))
    require(len(grid) >= 1 and all(type(n) is int and n > 0 for n in grid) and horizons, "frozen grid and horizons")
    sel_half, _ = halves(panel_identities(records, source_sha256=parent_sha256))
    rows, provenance = rows_for(sel_half, vl, parent_sha256, budget, grid, horizons)
    out = select_rows(rows, grid, horizons, replicates)
    ids = [[r["episode"], r["game"]] for r in sel_half]
    out.update({"schema": LOCK_SCHEMA, "budget": budget, "grid": list(grid), "horizons": list(horizons),
                "replicates": replicates, "provenance": sorted(list(p) for p in provenance),
                "selection_half_sha256": hashlib.sha256(json.dumps(ids).encode()).hexdigest()})
    out["lock_sha256"] = hashlib.sha256(json.dumps(out, sort_keys=True).encode()).hexdigest()
    return out


def interval(xs: list[float]) -> dict:
    xs = sorted(xs)
    lo = s0.lower_index(len(xs))
    return {"p2.5": xs[int(0.025 * (len(xs) - 1))], "p10": xs[lo], "p90": xs[len(xs) - 1 - lo],
            "p97.5": xs[int(0.975 * (len(xs) - 1))]}


def confirm(records: list[dict], vl: dict, parent_sha256: str, budget: int, lock: dict,
            replicates: int = s0.REPLICATES) -> dict:
    require(isinstance(lock, dict) and lock.get("schema") == LOCK_SCHEMA, "not a 2a lock")
    body = {k: v for k, v in lock.items() if k != "lock_sha256"}
    require(hashlib.sha256(json.dumps(body, sort_keys=True).encode()).hexdigest() == lock.get("lock_sha256"),
            "the lock's body does not match its hash")
    again = select(records, vl, parent_sha256, budget, tuple(lock["grid"]), tuple(lock["horizons"]),
                   lock["replicates"])
    require(json.dumps(again, sort_keys=True) == json.dumps(lock, sort_keys=True),
            "the lock does not match the selection half's records")
    key = again["selected"]
    result = {"schema": RESULT_SCHEMA, "budget": budget, "selected": key, "lock_sha256": again["lock_sha256"]}
    if key is None:
        result.update(verdict="fail", reason=again["reason"])
        return result
    _, conf_half = halves(panel_identities(records, source_sha256=parent_sha256))
    rows, provenance = rows_for(conf_half, vl, parent_sha256, budget, tuple(lock["grid"]), tuple(lock["horizons"]))
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
        # A replicate whose comparator gain is not positive has no defined ratio: it counts as -inf (a failing
        # replicate), so it can never help the binding clause.
        ratios.append((gv / cv) / (gs / cs) if gs > 0 else -math.inf)
    lo = s0.lower_index(replicates)
    comparator_positive = s["G"] > 0
    clauses = {"g_vl_lower_10pct_positive": sorted(g_v)[lo] > 0,
               "efficiency_point": comparator_positive and eff_v >= EFFICIENCY_MULTIPLE * eff_s,
               "gain_point": v["G"] >= GAIN_FRACTION * s["G"],
               "binding_ratio_lower_10pct_above_1": comparator_positive and sorted(ratios)[lo] > 1}
    verdict = "inconclusive" if not comparator_positive else ("pass" if all(clauses.values()) else "fail")
    selection_prov = {tuple(p) for p in lock["provenance"]}
    all_prov = selection_prov | provenance
    binding = {p[0] for p in all_prov} == {BINDING_CLASS} and len({(p[1], p[2]) for p in all_prov}) == 1
    disclosure = {str(b): summarize(rows, f"s4a_{b}_0b") for b in s0.BUDGETS}
    for d in disclosure.values():
        d["efficiency"] = d["G"] / d["c"]
    result.update({"vl": v, "s4a": s, "efficiency_vl": eff_v, "efficiency_s4a": eff_s,
                   "comparator_positive": comparator_positive, "clauses": clauses, "verdict": verdict,
                   "interval_efficiency_minus_3x": interval(d_eff), "interval_gain_minus_half": interval(d_gain),
                   "interval_efficiency_ratio": interval(ratios) if comparator_positive else None,
                   "s4a_both_budgets_0b_cost_disclosure": disclosure,
                   "provenance": sorted(list(p) for p in all_prov), "binding": binding,
                   "label": "binding read" if binding else "development read (not binding: host class or binary)",
                   "law": "development measure on terminal rollout outcomes of b512's continuation; "
                          "no rating, panel score or CP7 read"})
    return result


def crossclass(vl_a: dict, vl_b: dict, episodes: list[int], grid: tuple, horizons: tuple) -> dict:
    """Primary-hash agreement of the same searches (every frozen config at seeds 0 and 1 of each named episode)
    in two host classes. Every named search must be present and successful in both."""
    require(episodes, "name the sampled episodes")
    expected = {(h, n, s) for h in horizons for n in grid for s in s0.SEEDS}
    require(expected, "frozen grid and horizons")
    same, differ = 0, []
    for ep in episodes:
        require(ep in vl_a and ep in vl_b, f"episode {ep}: missing in one class")
        a, b = vl_a[ep], vl_b[ep]
        require(a["game"] == b["game"], f"episode {ep}: different states")
        require(a["host_class"] != b["host_class"], f"episode {ep}: both records are in class {a['host_class']}")
        ka = {(c["horizon"], c["n_sims"], c["search_seed"]): c for c in a["configs"]}
        kb = {(c["horizon"], c["n_sims"], c["search_seed"]): c for c in b["configs"]}
        require(set(ka) == set(kb) == expected, f"episode {ep}: searches differ from the frozen grid at both seeds")
        for k in sorted(ka):
            x, y = ka[k], kb[k]
            require(x["status"] == y["status"] == "ok" and x["seed_root"] == y["seed_root"],
                    f"episode {ep} {k}: both searches must succeed on the same seed root")
            if x.get("primary_sha256") and x.get("primary_sha256") == y.get("primary_sha256"):
                same += 1
            else:
                differ.append([ep, *k])
    return {"compared": same + len(differ), "equal": same, "differ": differ, "all_equal": not differ}


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
        if name == "select":
            p.add_argument("--grid", required=True, help="the frozen N grid, e.g. 128,512")
            p.add_argument("--horizons", required=True, help="the frozen horizons, e.g. h0,t0")
        else:
            p.add_argument("--out", type=Path)
    p = sub.add_parser("crossclass")
    p.add_argument("--vl", type=Path, required=True)
    p.add_argument("--other", type=Path, required=True)
    p.add_argument("--episodes", required=True, help="the sampled episodes, e.g. 3,17,40")
    p.add_argument("--grid", required=True, help="the frozen N grid, e.g. 128,512")
    p.add_argument("--horizons", required=True, help="the frozen horizons, e.g. h0,t0")
    args = parser.parse_args(argv)
    try:
        if args.command == "crossclass":
            result = crossclass(load_vl(args.vl), load_vl(args.other), [int(x) for x in args.episodes.split(",")],
                                tuple(int(x) for x in args.grid.split(",")), tuple(args.horizons.split(",")))
        elif args.command == "select":
            require(not args.lock.exists(), "the lock exists; selection is already locked")
            result = select(s0.load_records(args.captures), load_vl(args.vl), args.parent_sha256, args.budget,
                            tuple(int(x) for x in args.grid.split(",")), tuple(args.horizons.split(",")))
            args.lock.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
        else:
            lock = json.loads(args.lock.read_text(encoding="utf-8"))
            result = confirm(s0.load_records(args.captures), load_vl(args.vl), args.parent_sha256, args.budget, lock)
            if args.out:
                args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n")
    except StageError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
