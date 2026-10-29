"""EX1 design v3.3 section 7 item 7: qualification runs of the EX1 engine (collection hook, adapter, update).

Builds RT1 run configs (`native_expanded_training_run_v1`) from an OH1/OH2 base and a dose parent, with episode ids
and deal seeds in a qualification namespace, and summarizes finished runs. Nothing here is a Stage P or Stage F run.

  qual.py config --base BASE.json --parent DOSE.json --out CONFIG.json --root RUN_DIR --run RUN --offset U
                 --updates N [--workers W] [--ex1 --coefficient C --kappa K] [--namespace NS]
  qual.py summarize RUN_DIR [RUN_DIR ...]      per-update CPU, wall, rows, searches, strata, cap receipts
  qual.py strata RUN_DIR [RUN_DIR ...]         realized decisions and triggers per stratum against n(S) and p(S)
  qual.py calibrate RUN_DIR [RUN_DIR ...]      c* = r* / median raw ratio over updates with rows (B2 section 3)
  qual.py check RUN_DIR [RUN_DIR ...]          the check run's median applied ratio against [r*/3, 3 r*]
  qual.py mirror RUN_DIR [RUN_DIR ...]         every recorded target against the Python mirror, bit for bit
  qual.py compare RUN_A RUN_B                  same states, trajectories and EX1 records (timings aside)
  qual.py hookoff RUN_X RUN_O                  identical trajectories with the hook on and off
"""
import argparse
import copy
import hashlib
import json
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import target_ref  # noqa: E402

N_S = {"target": 6.46, "cast_or_activate": 32.17, "other": 38.30}
P_S = {"target": 0.155, "cast_or_activate": 0.0311, "other": 0.0261}
R_STAR = 0.3
KAPPA = 1.0
MASK = (1 << 63) - 1


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def seed(parts):
    text = json.dumps(list(parts), separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little") & MASK


def config(args):
    base = read(args.base)
    dose = read(args.parent)
    source = dose["source"]
    assert sha(source["checkpoint"]["path"]) == source["checkpoint"]["sha256"], "parent differs from its dose pin"
    plan = base["iterations"]
    iterations = []
    for i in range(args.updates):
        update = args.offset + i
        item = copy.deepcopy(plan[update % len(plan)])
        for j, entry in enumerate(item["episodes"]):
            episode = entry["episode"]
            episode["id"] = f"{args.namespace}-{args.run}-u{update}-j{j}-s{episode['learner_seat']}"
            episode["seed"] = seed([args.namespace, args.run, update, j])
        iterations.append(item)
    cfg = {k: copy.deepcopy(v) for k, v in base.items() if k != "iterations"}
    cfg["initial_source"] = source
    cfg["iterations"] = iterations
    cfg["collection_workers"] = args.workers
    cfg["record_process_cpu"] = True
    cfg["output_directory"] = Path(args.root).as_posix()
    if args.ex1:
        cfg["preparation_workers"] = 1
        cfg["ex1"] = {"run": args.run, "update_offset": args.offset, "coefficient": args.coefficient,
                      "cap": {"mode": "off"} if args.kappa is None else {"mode": "ratio", "kappa": args.kappa}}
    Path(args.out).write_text(json.dumps(cfg, indent=1) + "\n", encoding="utf-8")


def iterations(root):
    for complete in sorted(Path(root, "iterations").glob("*/complete.json")):
        receipt = read(complete)
        update = read(receipt["update"]["path"])
        collection = read(receipt["collection"]["path"])
        yield receipt, update, collection


def ex1_receipt(update):
    pin = (update.get("ex1") or {}).get("receipt")
    return read(pin["path"]) if pin else None


def summarize(args):
    out = []
    for root in args.roots:
        previous = None
        for receipt, update, collection in iterations(root):
            cpu = receipt.get("driver_process_cpu_seconds")
            row = {"run": Path(root).name, "iteration": receipt["iteration"],
                   "cpu_seconds": None if cpu is None or previous is None else cpu - previous,
                   "collection_wall": collection.get("collection_elapsed_seconds"),
                   "update_wall": update.get("update_elapsed_seconds"),
                   "after_state_sha256": update["after_state_sha256"]}
            previous = cpu
            ex1 = ex1_receipt(update)
            if ex1:
                exposure = ex1["gradient_exposure"]
                row.update({"searches": ex1["searches"], "searches_failed": ex1["searches_failed"],
                            "search_wall": ex1["search_wall_seconds"], "rows": ex1["rows"],
                            "rows_eligible": ex1["rows_eligible"], "rows_dropped": ex1["rows_dropped"],
                            "cap": exposure.get("cap"), "adam_steps": exposure.get("adam_steps")})
            out.append(row)
    print(json.dumps(out, indent=1))
    return out


def strata(args):
    totals = {s: {"decisions": 0, "triggers": 0, "searches_failed": 0, "root_rows": 0, "burst_rows": 0,
                  "no_target_support": 0, "no_target_mass": 0} for s in N_S}
    episodes = 0
    for root in args.roots:
        for _, update, _ in iterations(root):
            ex1 = ex1_receipt(update)
            if not ex1:
                continue
            episodes += len(ex1["episodes"])
            for stratum, counts in ex1["strata"].items():
                for key in totals[stratum]:
                    totals[stratum][key] += counts.get(key, 0)
    report = {"episodes": episodes, "strata": {}}
    for s, t in totals.items():
        per_episode = t["decisions"] / episodes if episodes else None
        report["strata"][s] = dict(t, decisions_per_episode=per_episode, n_s=N_S[s], p_s=P_S[s],
                                   expected_triggers=t["decisions"] * P_S[s],
                                   triggers_per_episode=t["triggers"] / episodes if episodes else None)
    print(json.dumps(report, indent=1))
    return report


def ratios(roots, key):
    values = []
    for root in roots:
        for _, update, _ in iterations(root):
            ex1 = ex1_receipt(update)
            if not ex1 or ex1["rows"] == 0:
                continue
            value = (ex1["gradient_exposure"].get("cap") or {}).get(key)
            if not isinstance(value, (int, float)):
                raise SystemExit(f"stop: an update with rows has a missing or null {key} ({root})")
            values.append(value)
    return values


def calibrate(args):
    raw = ratios(args.roots, "raw_ratio")
    if len(raw) < 8:
        raise SystemExit(f"stop: only {len(raw)} updates have rows")
    median = statistics.median(raw)
    if not median > 0:
        raise SystemExit("stop: the median raw ratio is not positive")
    c_star = float(f"{R_STAR / median:.3g}")
    print(json.dumps({"r_star": R_STAR, "updates_with_rows": len(raw), "median_raw_ratio": median,
                      "c_star": c_star, "raw": raw}, indent=1))


def check(args):
    applied = ratios(args.roots, "applied_ratio")
    if len(applied) < 8:
        raise SystemExit(f"stop: only {len(applied)} updates have rows")
    median = statistics.median(applied)
    ok = R_STAR / 3 <= median <= 3 * R_STAR
    print(json.dumps({"r_star": R_STAR, "kappa": KAPPA, "updates_with_rows": len(applied),
                      "median_applied_ratio": median, "band": [R_STAR / 3, 3 * R_STAR], "pass": ok,
                      "applied": applied}, indent=1))
    if not ok:
        raise SystemExit("stop: the median applied ratio is outside [r*/3, 3 r*]")


TIMING_KEYS = {"wall", "search_wall_seconds", "teacher_seconds", "seconds"}


def strip(value):
    if isinstance(value, dict):
        return {k: strip(v) for k, v in value.items() if k not in TIMING_KEYS}
    if isinstance(value, list):
        return [strip(v) for v in value]
    return value


def ex1_records(collection):
    return [read(pin["path"]) for pin in (collection.get("ex1") or {}).get("episodes", [])]


def mirror(args):
    checked = 0
    for root in args.roots:
        for _, _, collection in iterations(root):
            trajectories = [read(pin["path"]) for pin in collection["trajectories"]]
            for record in ex1_records(collection):
                trajectory = trajectories[record["slot"]]
                by_step = {d["step"]: d for d in trajectory["decisions"]}
                for row in record["attempts"][-1]["rows"]:
                    logits = by_step[row["step"]]["logits"]
                    result = target_ref.target(logits, row["n"], row["wins"], row["draws"])
                    assert not isinstance(result, tuple), f"mirror gives no target at {root} {row['step']}"
                    q_bits = [target_ref.bits64(x) for x in result["q"]]
                    assert q_bits == row["q_bits"], f"mirror differs at {root} slot {record['slot']} {row['step']}"
                    checked += 1
    print(json.dumps({"rows_checked": checked, "bit_equal": True}))
    return checked


def compare(args):
    a, b = list(iterations(args.roots[0])), list(iterations(args.roots[1]))
    assert len(a) == len(b), "different iteration counts"
    for (ra, ua, ca), (rb, ub, cb) in zip(a, b):
        assert ua["after_state_sha256"] == ub["after_state_sha256"], f"state differs at {ra['iteration']}"
        assert [p["sha256"] for p in ca["trajectories"]] == [p["sha256"] for p in cb["trajectories"]]
        assert strip(ex1_records(ca)) == strip(ex1_records(cb)), f"ex1 records differ at {ra['iteration']}"
        ea, eb = ex1_receipt(ua), ex1_receipt(ub)
        keys = ("strata", "searches", "searches_failed", "rows", "rows_eligible", "rows_dropped", "gradient_exposure")
        assert (ea is None) == (eb is None) and (ea is None or all(strip(ea[k]) == strip(eb[k]) for k in keys))
    print(json.dumps({"iterations": len(a), "identical": True,
                      "states": [u["after_state_sha256"] for _, u, _ in a]}))


def hookoff(args):
    x, o = list(iterations(args.roots[0])), list(iterations(args.roots[1]))
    n = min(len(x), len(o))
    assert n >= 1
    for (_, _, cx), (_, _, co) in zip(x[:n], o[:n]):
        assert [p["sha256"] for p in cx["trajectories"]] == [p["sha256"] for p in co["trajectories"]]
    print(json.dumps({"iterations": n, "trajectories_identical": True,
                      "x_rows": [ex1_receipt(u)["rows"] for _, u, _ in x[:n]],
                      "x_state": [u["after_state_sha256"] for _, u, _ in x[:n]],
                      "o_state": [u["after_state_sha256"] for _, u, _ in o[:n]]}))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("config")
    c.add_argument("--base", required=True)
    c.add_argument("--parent", required=True)
    c.add_argument("--out", required=True)
    c.add_argument("--root", required=True)
    c.add_argument("--run", required=True)
    c.add_argument("--offset", type=int, required=True)
    c.add_argument("--updates", type=int, required=True)
    c.add_argument("--workers", type=int, default=10)
    c.add_argument("--namespace", default="ex1-qual-20261029")
    c.add_argument("--ex1", action="store_true")
    c.add_argument("--coefficient", type=float, default=1.0)
    c.add_argument("--kappa", type=float, default=None)
    for name in ("summarize", "strata", "calibrate", "mirror", "compare", "hookoff"):
        s = sub.add_parser(name)
        s.add_argument("roots", nargs="+")
    k = sub.add_parser("check")
    k.add_argument("roots", nargs="+")
    args = ap.parse_args(argv)
    {"config": config, "summarize": summarize, "strata": strata, "calibrate": calibrate, "check": check,
     "mirror": mirror, "compare": compare, "hookoff": hookoff}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
