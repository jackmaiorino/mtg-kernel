"""EX1 Stage F launcher (design v3.3 section 5 "Stage F, the study"; collab LANES/spy-primary-direction-20261028
stagef/STAGE-F-PLAN.md section 6 and the lead's rulings R1 to R6). Nothing here decides a rule step or reads an
outcome: it trains Stage F's eight chains and records what the lead reads.

  stagef.py plan --qualification Q.json --calibration CALIBRATION.json --binary EXE --root ROOT --out PLAN.json
                 --parent r2=DOSE.json --parent r1=DOSE.json --base r2=BASE.json --base r1=BASE.json
                 --budget BUDGET.json --o-timing O-TIMING.json --namespace NS
                 --place CHAIN=HOST:CORES[:GROUP] [...] [--wrapper HOST=JSON_LIST] [--reserve HOST=GIB]
                 [--byte-cap BYTES] [--regeneration-receipt RECEIPT.json]
  stagef.py throughput --plan PLAN.json --host HOST --arm x|o [--pack K [--repeats R] [--stagger S]]
  stagef.py run --plan PLAN.json --chain CHAIN [--chain CHAIN ...]
  stagef.py thin --plan PLAN.json [--apply] [--loop SECONDS]
  stagef.py purge --plan PLAN.json [--apply]
  stagef.py status --plan PLAN.json

Chains: per lineage (r1, r2) and pair i (1, 2), an X chain `<lineage>-p<i>-x` (RT1 plus ex1-target-v1 rows at the
selected dose) and an O chain `<lineage>-p<i>-o` (ordinary RT1). X chain i and O chain i are forked from the same
parent at the same seed root: their configs share every episode id and deal seed (key `<lineage>-p<i>` in the
plan's namespace) and differ only in the `ex1` block, `preparation_workers` and the output folder, which `plan`
checks. Pairs and lineages have disjoint keys.

Frozen inputs the plan pins: the qualified binary; the qualification manifest; P2's calibration (ruling R1: c* per
lineage at r* = 1.0, kappa = 2); the parents and RT1 bases; workers 10. The extent is section 5's formula
  U = floor(0.8 x B_train x 3600 / (rate x sum over chains of C_i))
with rate = 0.07 USD per CPU-h (ruling R2), B_train = the budget snapshot's allowance minus its listed spend (ruling
R3), C_i = the qualification's mean CPU-s per X update for X chains and the O timing record's per-lineage value
for O chains (ruling R4). Every chain runs U updates.

`run` refuses to start unless every pin matches, the host has a throughput record for the chain's arm, binary,
workers, cores and pack size whose state equals the qualified one, and the volume has room. Owned hosts only:
every chain is metered at 0 USD (ruling R5). The in-Stage-F stop: every chain stops at update 300 (the hold) until
the lead writes HOLD-300-CLEARED.json in the root; STAGEF-STOPPED.json, or a STOP file, stops everything. An
interrupted iteration is moved aside and rerun fresh (mtg-kernel#336).

`thin` is the rolling storage plan (ruling R6; ARTIFACT-LAW clauses 3, 4 and 6). It moves only uncited bulk, the
episode trajectories and the update checkpoint of an iteration, into a same-volume holding folder with a manifest,
after checking each file against the hash its iteration's records pin. It keeps every record file, every
iteration's files at multiples of keep_every, at the hold and at U, and the last keep_last completed iterations.
`purge` deletes held files after hold_days, or at once when the plan pins a regeneration receipt.
"""
import argparse
import json
import math
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import time
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import qual  # noqa: E402
import stagep  # noqa: E402

read, write, sha, pin, check_pin, log = stagep.read, stagep.write, stagep.sha, stagep.pin, stagep.check_pin, stagep.log
command, stop_tree, quarantine, completed = stagep.command, stagep.stop_tree, stagep.quarantine, stagep.completed

SCHEMA = "ex1-stagef-plan/v1"
BUDGET_SCHEMA = "ex1-stagef-budget/v1"
O_TIMING_SCHEMA = "ex1-stagef-o-timing/v1"
CALIBRATION_SCHEMA = "ex1-p2-calibration/v1"
R_STAR = 1.0                  # the larger dose, the only one Stage F can select (design section 5 step 4)
KAPPA = 2.0
RATE_USD_PER_CPU_H = 0.07     # ruling R2 (design section 9)
RESERVE_FACTOR = 0.8          # design section 5
HOLD = 300                    # the in-Stage-F stop at X update 300 (design section 5)
WORKERS = 10
LINEAGES = ("r2", "r1")
PAIRS = (1, 2)
ARMS = ("x", "o")
CHAINS = {f"{lin}-p{i}-{arm}": {"lineage": lin, "pair": i, "arm": arm}
          for lin in LINEAGES for i in PAIRS for arm in ARMS}
PAIR_ONLY_KEYS = {"ex1", "preparation_workers", "output_directory"}
BYTES_PER_UPDATE = 53_000_000          # qualification: trajectories up to 32 MB, checkpoint 21 MB
BYTE_CAP = 400_000_000_000             # ruling R6: at most 400 GB across D: and E:
RESERVE_GIB = 60.0
THIN = {"keep_every": 100, "keep_last": 20, "hold_days": 7}
POLL_S = 30
EXIT_STOP = 5
EXIT_HOLD = 6
HOLD_CLEARED = "HOLD-300-CLEARED.json"
STOPPED = "STAGEF-STOPPED.json"
# The O throughput update: the qualification's hook-off run off-o (update 100 of ex1-qual-det-r2, hook off), whose
# state the qualification records as checks.hook_on_off.o_state.
O_THROUGHPUT = {"namespace": "ex1-qual-20261029", "run": "ex1-qual-det-r2", "offset": 100}


# ---------------------------------------------------------------- plan

def parse_pairs(items, what):
    return stagep.parse_pairs(items, what)


def b_train(budget):
    if budget.get("schema") != BUDGET_SCHEMA:
        raise SystemExit("refused: not a Stage F budget snapshot")
    spend = budget.get("spend", [])
    if any(not isinstance(s.get("usd"), (int, float)) or s["usd"] < 0 or not s.get("item") for s in spend):
        raise SystemExit("refused: every budget row needs an item and a USD amount of at least 0")
    value = budget["allowance_usd"] - math.fsum(s["usd"] for s in spend)
    if value <= 0:
        raise SystemExit(f"refused: B_train is {value} USD")
    return value


def extent(b, costs):
    """Section 5: U = floor(0.8 x B_train x 3600 / (rate x sum of C_i))."""
    total = math.fsum(costs)
    if not total > 0:
        raise SystemExit("refused: the chains' CPU per update must sum above 0")
    return math.floor(RESERVE_FACTOR * b * 3600 / (RATE_USD_PER_CPU_H * total))


def calibrated_dose(cal):
    if cal.get("schema") != CALIBRATION_SCHEMA or not cal.get("pass"):
        raise SystemExit("refused: the calibration did not pass")
    if cal["dose"]["r_star"] != R_STAR or cal["dose"]["kappa"] != KAPPA:
        raise SystemExit("refused: the calibration is not at r* = 1.0, kappa = 2")
    dose = {}
    for lin in LINEAGES:
        lineage = cal["lineages"][lin]
        if not lineage["k2"]["pass"] or lineage["k2"]["coefficient"] != lineage["k1"]["c_star"] \
                or lineage["k2"]["kappa"] != KAPPA:
            raise SystemExit(f"refused: the {lin} check did not pass at c*")
        dose[lin] = lineage["k1"]["c_star"]
    return dose


def pair_difference(x_cfg, o_cfg):
    keys = set(x_cfg) | set(o_cfg)
    return sorted(k for k in keys if k not in PAIR_ONLY_KEYS and x_cfg.get(k) != o_cfg.get(k))


def make_plan(args):
    q = read(args.qualification)
    if sha(args.binary) != q["engine"]["binaries"]["native_expanded_training_run_v1"]:
        raise SystemExit("refused: the binary is not the qualified one")
    failed = [name for name, check in q["checks"].items() if check.get("pass") is False]
    if failed:
        raise SystemExit(f"refused: qualification checks failed: {failed}")
    cal = read(args.calibration)
    if cal.get("binary", {}).get("sha256") != q["engine"]["binaries"]["native_expanded_training_run_v1"]:
        raise SystemExit("refused: the calibration ran another binary")
    dose = calibrated_dose(cal)
    budget = read(args.budget)
    b = b_train(budget)
    timing = read(args.o_timing)
    if timing.get("schema") != O_TIMING_SCHEMA or \
            timing.get("binary_sha256") != q["engine"]["binaries"]["native_expanded_training_run_v1"]:
        raise SystemExit("refused: the O timing record is not for the qualified binary")
    c_x = q["measurements"]["cpu_per_x_update"]["mean"]
    c_o = {lin: timing["cpu_seconds_per_o_update"][lin] for lin in LINEAGES}
    cost = {name: (c_x if c["arm"] == "x" else c_o[c["lineage"]]) for name, c in CHAINS.items()}
    updates = extent(b, cost.values())
    if updates < HOLD:
        raise SystemExit(f"refused: U = {updates} is below the hold at update {HOLD}")
    parents, bases = parse_pairs(args.parent, "--parent"), parse_pairs(args.base, "--base")
    wrappers = {k: json.loads(v) for k, v in parse_pairs(args.wrapper, "--wrapper").items()}
    reserves = {k: float(v) for k, v in parse_pairs(args.reserve, "--reserve").items()}
    places = parse_pairs(args.place, "--place")
    if set(places) != set(CHAINS):
        raise SystemExit(f"refused: place every chain exactly once ({sorted(CHAINS)})")
    if not re.fullmatch(r"[a-z0-9-]+", args.namespace) or args.namespace.startswith(("ex1-stagep", "ex1-p2")):
        raise SystemExit("refused: Stage F needs its own fresh namespace")
    root = Path(args.root)
    chains, groups, cfgs = {}, {}, {}
    for name, c in CHAINS.items():
        fields = places[name].split(":")
        if len(fields) not in (2, 3):
            raise SystemExit(f"refused: --place {name}=HOST:CORES[:GROUP] (owned hosts at 0 USD, ruling R5)")
        host, cores = fields[0], int(fields[1])
        if cores < 1:
            raise SystemExit(f"refused: {name} needs at least one core")
        if len(fields) == 3:
            if c["arm"] != "x":
                raise SystemExit(f"refused: only X chains pack (an O update is about 2 CPU-s): {name}")
            groups.setdefault(fields[2], []).append(name)
        key = f"{c['lineage']}-p{c['pair']}"
        native = root / "chains" / name / "native"
        lin = c["lineage"]
        cfg = qual.build_config(read(bases[lin]), read(parents[lin]), native, key, args.namespace, 0, updates,
                                WORKERS, dose[lin] if c["arm"] == "x" else None, KAPPA if c["arm"] == "x" else None)
        cfgs[name] = cfg
        config_path = root / "configs" / f"{name}.json"
        write(config_path, cfg)
        chains[name] = {"lineage": lin, "pair": c["pair"], "arm": c["arm"], "seed_key": key,
                        "namespace": args.namespace, "updates": updates,
                        "coefficient": dose[lin] if c["arm"] == "x" else None,
                        "kappa": KAPPA if c["arm"] == "x" else None, "workers": WORKERS, "host": host,
                        "cores": cores, "usd_per_hour": 0.0, "budget_usd": 0.0,
                        "cpu_seconds_per_update": cost[name], "config": pin(config_path),
                        "parent": pin(parents[lin]), "base": pin(bases[lin]), "native": native.as_posix()}
    for lin in LINEAGES:
        for i in PAIRS:
            diff = pair_difference(cfgs[f"{lin}-p{i}-x"], cfgs[f"{lin}-p{i}-o"])
            if diff:
                raise SystemExit(f"refused: pair {lin}-p{i} differs outside the ex1 block: {diff}")
    for group, members in groups.items():
        if not group or len({(chains[m]["host"], chains[m]["cores"]) for m in members}) != 1:
            raise SystemExit(f"refused: pack group {group!r} needs a name and one host and core count")
        for m in members:
            chains[m].update(group=group, pack=len(members))
    hosts = sorted({c["host"] for c in chains.values()})
    plan = {"schema": SCHEMA, "design": q["design"], "qualification": pin(args.qualification),
            "calibration": pin(args.calibration), "binary": pin(args.binary), "root": root.as_posix(),
            "namespace": args.namespace,
            "extent": {"formula": "floor(0.8 x B_train x 3600 / (rate x sum C_i))", "b_train_usd": b,
                       "budget": pin(args.budget), "o_timing": pin(args.o_timing),
                       "rate_usd_per_cpu_h": RATE_USD_PER_CPU_H, "reserve_factor": RESERVE_FACTOR,
                       "cpu_seconds_per_update": cost, "updates": updates},
            "hold": {"update": HOLD, "cleared_file": HOLD_CLEARED, "stopped_file": STOPPED},
            "byte_budget": {"bytes_per_update": BYTES_PER_UPDATE,
                            "projected_bytes_unthinned": BYTES_PER_UPDATE * updates * len(CHAINS),
                            "cap_bytes": int(args.byte_cap)},
            "thin": dict(THIN, regeneration_receipt=pin(args.regeneration_receipt)
                         if args.regeneration_receipt else None),
            "hosts": {h: {"wrapper": wrappers.get(h, []), "reserve_gib": max(RESERVE_GIB, reserves.get(h, 0.0))}
                      for h in hosts},
            "chains": chains, "allotted_usd": 0.0}
    write(args.out, plan)
    print(json.dumps({"plan": Path(args.out).as_posix(), "updates": updates, "b_train_usd": b, "hosts": hosts}))


def load_plan(path):
    plan = read(path)
    if plan.get("schema") != SCHEMA:
        raise SystemExit("refused: not an EX1 Stage F plan")
    for key, what in (("qualification", "the qualification manifest"), ("binary", "the binary"),
                      ("calibration", "the calibration")):
        check_pin(plan[key], what)
    return plan


# ---------------------------------------------------------------- throughput

def throughput_path(plan, host, arm, pack):
    return Path(plan["root"]) / "throughput" / (f"THROUGHPUT-{host}-{arm}.json" if pack == 1
                                                else f"THROUGHPUT-{host}-{arm}-k{pack}.json")


def o_throughput_config(plan, native):
    r2 = next(c for c in plan["chains"].values() if c["lineage"] == "r2")
    return qual.build_config(read(r2["base"]["path"]), read(r2["parent"]["path"]), native, O_THROUGHPUT["run"],
                             O_THROUGHPUT["namespace"], O_THROUGHPUT["offset"], 1, WORKERS)


def throughput(args):
    plan = load_plan(args.plan)
    chains = [c for c in plan["chains"].values() if c["host"] == args.host and c["arm"] == args.arm]
    if not chains:
        raise SystemExit(f"no {args.arm.upper()} chain is placed on {args.host}")
    if args.arm == "o" and args.pack != 1:
        raise SystemExit("refused: O chains do not pack")
    cores = max(c["cores"] for c in chains)
    if args.pack > 1:
        sizes = {c["cores"] for c in chains if c.get("pack", 1) == args.pack}
        if len(sizes) != 1:
            raise SystemExit(f"refused: {args.host} needs X pack groups of size {args.pack} at one core count")
        cores = sizes.pop()
    q = read(plan["qualification"]["path"])
    out = Path(plan["root"]) / "throughput" / (f"{args.host}-{args.arm}-c{cores}-k{args.pack}-"
                                               f"{time.strftime('%Y%m%dT%H%M%S')}-{uuid.uuid4().hex[:8]}")
    out.mkdir(parents=True, exist_ok=False)
    config_path = out / "config.json"
    if args.arm == "x":
        write(config_path, stagep.throughput_config(plan, out / "native"))
        expected = q["checks"]["resume_determinism"]["states"][0]
    else:
        write(config_path, o_throughput_config(plan, out / "native"))
        expected = q["checks"]["hook_on_off"]["o_state"]
    plan["hosts"][args.host]["wrapper"] = [part.replace("{cores}", str(cores)).replace("{chain}", "throughput")
                                           for part in plan["hosts"][args.host]["wrapper"]]
    windows = platform.system() == "Windows"
    if args.pack == 1:
        started = time.time()
        code = subprocess.call(command(plan, args.host, str(config_path)))
        wall = time.time() - started
        rows = list(qual.iterations(out / "native"))
        receipt, update, collection = rows[0] if rows else ({}, {}, {})
        record = {"schema": "ex1-stagef-throughput/v1", "host": args.host, "hostname": socket.gethostname(),
                  "platform": platform.platform(), "binary_sha256": plan["binary"]["sha256"], "arm": args.arm,
                  "cores": cores, "workers": WORKERS, "pack": 1, "exit": code if rows else (code or 1),
                  "wall_seconds": wall, "collection_wall_seconds": collection.get("collection_elapsed_seconds"),
                  "process_cpu_seconds": receipt.get("driver_process_cpu_seconds"),
                  "after_state_sha256": update.get("after_state_sha256"), "expected_state_sha256": expected,
                  "state_equal": update.get("after_state_sha256") == expected, "state_required": windows}
    else:
        record = stagep.packed_throughput(plan, args, out, config_path, cores, expected, windows)
        record.update(schema="ex1-stagef-throughput/v1", arm="x")
    path = throughput_path(plan, args.host, args.arm, args.pack)
    write(path, record)
    print(json.dumps(record, indent=1))
    if record["exit"] != 0 or (windows and not record["state_equal"]):
        raise SystemExit("throughput run failed or its state differs from the qualification's")


def pack_size(plan, chain):
    group = chain.get("group")
    members = [c for c in plan["chains"].values() if group is not None and c.get("group") == group]
    pack = len(members) if group is not None else 1
    if chain.get("pack", 1) != pack or any(c.get("pack", 1) != pack for c in members):
        raise SystemExit(f"refused: pack group {group!r} has {pack} members but its plan says otherwise")
    if len({(c["host"], c["cores"], c["arm"]) for c in members}) > 1:
        raise SystemExit(f"refused: pack group {group!r} needs one host, core count and arm")
    return pack


def guard(plan, name, chain):
    """COMPUTE-POLICY item 5: refuse without a matching throughput record for this host and arm."""
    pack = pack_size(plan, chain)
    path = throughput_path(plan, chain["host"], chain["arm"], pack)
    if not path.exists():
        raise SystemExit(f"refused: no {chain['arm'].upper()} throughput record for {chain['host']} at pack "
                         f"{pack}; run `stagef.py throughput --arm {chain['arm']}"
                         f"{'' if pack == 1 else f' --pack {pack}'}` there first")
    t = read(path)
    problems = []
    if t.get("arm") != chain["arm"]:
        problems.append(f"recorded for arm {t.get('arm')}, not {chain['arm']}")
    if t.get("pack", 1) != pack:
        problems.append(f"recorded at pack {t.get('pack', 1)}, not {pack}")
    if t["hostname"] != socket.gethostname():
        problems.append(f"recorded on {t['hostname']}, not {socket.gethostname()}")
    if t["binary_sha256"] != plan["binary"]["sha256"]:
        problems.append("recorded with another binary")
    if t["workers"] != chain["workers"] or t["cores"] < chain["cores"]:
        problems.append("recorded at other workers or fewer cores")
    if pack > 1 and t["cores"] != chain["cores"]:
        problems.append(f"packed record at {t['cores']} cores, not the group's {chain['cores']}")
    if t["exit"] != 0 or (t["state_required"] and not t["state_equal"]):
        problems.append("its update failed or its state differs from the qualification's")
    if problems:
        raise SystemExit(f"refused: throughput record for {chain['host']}: {'; '.join(problems)}")
    return t


# ---------------------------------------------------------------- run

def held(plan, name):
    """True when the chain has reached the hold and the lead has not cleared it."""
    root = Path(plan["root"])
    return completed(plan["chains"][name]["native"]) >= plan["hold"]["update"] and \
        not (root / plan["hold"]["cleared_file"]).exists()


def check(plan, name, allow_held=False):
    """A chain's pins, throughput record and byte share; returns the bytes it still has to write."""
    chain = plan["chains"][name]
    root = Path(plan["root"])
    for key, what in (("config", "the chain config"), ("parent", "the parent"), ("base", "the base config")):
        check_pin(chain[key], what)
    guard(plan, name, chain)
    if (root / plan["hold"]["stopped_file"]).exists():
        raise SystemExit(f"refused: Stage F is stopped ({plan['hold']['stopped_file']})")
    if held(plan, name) and not allow_held:
        raise SystemExit(f"refused: {name} is held at update {plan['hold']['update']} for the lead's steps 1 and 2 "
                         f"read; the lead writes {plan['hold']['cleared_file']} to continue")
    native = chain["native"]
    written = sum(f.stat().st_size for f in Path(native).rglob("*") if f.is_file()) if Path(native).exists() else 0
    if written > plan["byte_budget"]["cap_bytes"] / len(plan["chains"]):
        raise SystemExit(f"refused: {name} holds {written / 1e9:.1f} GB, past its share of the byte cap")
    return (chain["updates"] - completed(native)) * plan["byte_budget"]["bytes_per_update"]


def group_checks(plan, names):
    chain = plan["chains"][names[0]]
    group = chain.get("group")
    members = sorted(n for n, c in plan["chains"].items() if group is not None and c.get("group") == group)
    if group is None and len(names) > 1:
        raise SystemExit("refused: only the chains of one pack group run together")
    if group is not None and sorted(names) != members:
        raise SystemExit(f"refused: pack group {group} runs all its chains together: {members}")
    need = sum(check(plan, name, allow_held=group is not None) for name in names)
    if group is not None and all(held(plan, name) for name in names):
        raise SystemExit(f"refused: pack group {group} is held at update {plan['hold']['update']} for the lead's "
                         f"steps 1 and 2 read; the lead writes {plan['hold']['cleared_file']} to continue")
    # Thinning keeps a chain's footprint far below its unthinned bytes; the reserve check counts the hold's worth.
    need = min(need, len(names) * plan["thin"]["keep_last"] * plan["byte_budget"]["bytes_per_update"] * 2)
    free = shutil.disk_usage(Path(plan["root"])).free
    reserve = plan["hosts"][chain["host"]]["reserve_gib"] * (1 << 30)
    if free < need + reserve:
        raise SystemExit(f"refused: {free / 1e9:.1f} GB free, need {need / 1e9:.1f} GB plus the host reserve")
    return chain


def run(args):
    plan = load_plan(args.plan)
    names = list(dict.fromkeys(args.chain))
    chain = group_checks(plan, names)
    group = chain.get("group")
    wrapper = [part.replace("{cores}", str(chain["cores"])).replace("{chain}", group or names[0])
               for part in plan["hosts"][chain["host"]]["wrapper"]]
    plan["hosts"][chain["host"]]["wrapper"] = wrapper
    if group is None:
        raise SystemExit(supervise(plan, names, command(plan, chain["host"], chain["config"]["path"]),
                                   args.fast_poll))
    results = Path(plan["root"]) / "chains" / f"group-{group}" / "members.json"
    inner = wrapper + [sys.executable, str(Path(__file__).resolve()), "_members",
                       "--plan", str(Path(args.plan).resolve()), "--plan-sha256", sha(args.plan),
                       "--results", str(results)]
    for name in names:
        inner += ["--chain", name]
    if args.fast_poll:
        inner.append("--fast-poll")
    raise SystemExit(supervise(plan, names, inner, args.fast_poll, results))


def members(args):
    if not Path(args.plan).is_absolute() or sha(args.plan) != args.plan_sha256:
        raise SystemExit("refused: _members needs the launching plan's absolute path and hash")
    plan = load_plan(args.plan)
    names = list(dict.fromkeys(args.chain))
    chain = group_checks(plan, names)
    if chain.get("group") is None:
        raise SystemExit("refused: _members runs a pack group")
    stagep.require_claim(plan, chain["host"])
    binary = plan["binary"]["path"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]   # .py: test stand-ins only
    procs, codes = {}, {}
    try:
        for name in names:
            if held(plan, name):
                codes[name] = EXIT_HOLD          # a member already at the hold waits for the others
            else:
                procs[name] = subprocess.Popen(exe + [plan["chains"][name]["config"]["path"]])
    except BaseException:
        for proc in procs.values():
            stop_tree(proc)
        raise
    # Each member stops at the hold on its own; the others keep their cores and run on.
    while any(proc.poll() is None for proc in procs.values()):
        for name, proc in procs.items():
            if proc.poll() is None and name not in codes and held(plan, name):
                stop_tree(proc)
                codes[name] = EXIT_HOLD
        time.sleep(POLL_S if not args.fast_poll else 0.2)
    for name, proc in procs.items():
        codes.setdefault(name, proc.wait())
    write(args.results, codes)
    raise SystemExit(0 if not any(codes.values()) else 1)


def supervise(plan, names, cmd, fast_poll, results=None):
    """Runs the named chains; stops them at the hold (every chain, until the lead clears it), on STOP or on
    STAGEF-STOPPED.json. Every chain is metered at 0 USD (owned hosts, ruling R5)."""
    root = Path(plan["root"])
    chains = {name: plan["chains"][name] for name in names}
    paths = {name: root / "chains" / name / "STATE.json" for name in names}
    states = {name: read(p) if p.exists() else {"metered_seconds": 0.0, "sessions": []} for name, p in paths.items()}
    sessions = {}
    for name, chain in chains.items():
        done = completed(chain["native"])
        moved = quarantine(chain["native"], root, name)
        log(root, chain=name, event="start", completed=done, moved=moved, cmd=cmd)
        sessions[name] = {"started": time.time(), "completed_at_start": done, "moved": moved}
    if results is not None:
        Path(results).unlink(missing_ok=True)
    proc = subprocess.Popen(cmd)
    outcome = None
    last = time.time()
    while proc.poll() is None:
        time.sleep(POLL_S if not fast_poll else 0.2)
        now = time.time()
        for name in chains:
            states[name]["metered_seconds"] += now - last
            write(paths[name], states[name])
        last = now
        if (root / "STOP").exists() or (root / plan["hold"]["stopped_file"]).exists():
            outcome = "stop"
        elif results is None and held(plan, names[0]):
            outcome = "hold"                 # one chain: stopped here; a pack group's members stop in _members
        if outcome:
            stop_tree(proc)
            break
    code = proc.wait()
    member = read(results) if results is not None and Path(results).exists() else {}
    codes = {name: member.get(name, code) for name in names}
    if outcome is None and results is not None and all(codes[n] == EXIT_HOLD for n in names):
        outcome = "hold"
    for name, chain in chains.items():
        native = chain["native"]
        states[name]["metered_seconds"] += time.time() - last
        session = sessions[name]
        session.update(ended=time.time(), exit=codes[name], outcome=outcome, completed=completed(native))
        states[name]["sessions"].append(session)
        write(paths[name], states[name])
        log(root, chain=name, event="end", exit=codes[name], outcome=outcome, completed=session["completed"],
            usd=0.0)
        if outcome or codes[name] == EXIT_HOLD:
            session["moved_after"] = quarantine(native, root, name)
            write(paths[name], states[name])
        elif codes[name] == 0 and Path(native, "completion.json").exists()                 and completed(native) == chain["updates"]:
            write(root / "chains" / name / "DONE.json",
                  {"chain": name, "arm": chain["arm"], "completed_updates": session["completed"], "metered_usd": 0.0,
                   "completion": pin(Path(native, "completion.json"))})
    if outcome == "stop":
        return EXIT_STOP
    if outcome == "hold":
        return EXIT_HOLD
    return next((c for c in [code, *codes.values()] if c != 0), 0)


# ---------------------------------------------------------------- storage (ruling R6)

def keep_set(plan, chain, done):
    """1-based updates whose bulk stays: multiples of keep_every, the hold, U, and the last keep_last completed."""
    t = plan["thin"]
    keep = {u for u in range(t["keep_every"], chain["updates"] + 1, t["keep_every"])}
    keep |= {plan["hold"]["update"], chain["updates"]}
    keep |= set(range(max(1, done - t["keep_last"] + 1), done + 1))
    return keep


def bulk_files(iteration):
    """An iteration's uncited bulk, each with the hash its own records pin: trajectories (pinned by collection.json)
    and the update checkpoint (pinned by complete.json's output identity). Record files are never listed."""
    complete = read(iteration / "complete.json")
    attempt = iteration / [p for p in re.split(r"[\\/]", complete["update"]["path"]) if p.startswith("attempt-")][0]
    collection = read(attempt / "collect" / Path(re.split(r"[\\/]", complete["collection"]["path"])[-1]))
    out = []
    for p in collection.get("trajectories", []):
        out.append((attempt / "collect" / re.split(r"[\\/]", p["path"])[-1], p["sha256"]))
    out.append((attempt / "update" / "checkpoint.json", complete["output_identity"]["checkpoint_sha256"]))
    return out


def is_link(path):
    p = Path(path)
    return p.is_symlink() or (hasattr(p, "is_junction") and p.is_junction())


def thin_once(plan, apply):
    root = Path(plan["root"])
    report, moved_bytes = {}, 0
    for name, chain in plan["chains"].items():
        native = Path(chain["native"])
        if not (native / "iterations").exists() or is_link(native) or is_link(native / "iterations"):
            continue
        done = completed(native)
        keep = keep_set(plan, chain, done)
        hold_dir = root / "_hold-thin" / time.strftime("%Y%m%d") / name
        rows = []
        for it in sorted(p for p in (native / "iterations").iterdir() if p.is_dir() and not is_link(p)):
            if not (it / "complete.json").exists() or int(it.name) + 1 in keep:
                continue
            for path, digest in bulk_files(it):
                if not path.exists():
                    continue
                if is_link(path):
                    raise SystemExit(f"refused: {path} is a link")
                if sha(path) != digest:
                    raise SystemExit(f"stop: {path} does not match the hash its records pin; nothing more is moved")
                size = path.stat().st_size
                dest = hold_dir / path.relative_to(native)
                rows.append({"chain": name, "from": path.as_posix(), "to": dest.as_posix(), "bytes": size,
                             "sha256": digest})
                if apply:
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    os.replace(path, dest)           # same volume: a rename, nothing copied
                    with open(root / "THIN-MANIFEST.jsonl", "a", encoding="utf-8", newline="\n") as f:
                        f.write(json.dumps(dict(rows[-1], at=time.strftime("%Y-%m-%dT%H:%M:%S%z"))) + "\n")
                moved_bytes += size
        report[name] = {"completed": done, "files": len(rows), "bytes": sum(r["bytes"] for r in rows)}
    return {"apply": apply, "chains": report, "bytes": moved_bytes}


def thin(args):
    plan = load_plan(args.plan)
    while True:
        print(json.dumps(thin_once(plan, args.apply)))
        if not args.loop:
            return
        if (Path(plan["root"]) / "STOP").exists():
            return
        time.sleep(args.loop)


def purge(args):
    """Deletes holding folders older than hold_days, or every holding folder when the plan pins a regeneration
    receipt (ARTIFACT-LAW clause 6). Junction-safe: links are never followed or deleted through."""
    plan = load_plan(args.plan)
    receipt = plan["thin"].get("regeneration_receipt")
    if receipt:
        check_pin(receipt, "the regeneration receipt")
    base = Path(plan["root"]) / "_hold-thin"
    cutoff = time.time() - plan["thin"]["hold_days"] * 86400
    out = []
    for day in sorted(p for p in base.iterdir() if p.is_dir() and not is_link(p)) if base.exists() else []:
        try:
            stamp = time.mktime(time.strptime(day.name, "%Y%m%d"))
        except ValueError:
            continue
        if not receipt and stamp > cutoff:
            continue
        files = size = 0
        for dirpath, dirnames, filenames in os.walk(day, topdown=False, followlinks=False):
            for f in filenames:
                p = Path(dirpath) / f
                if is_link(p):
                    continue
                size += p.stat().st_size
                files += 1
                if args.apply:
                    p.unlink()
            for d in dirnames:
                p = Path(dirpath) / d
                if args.apply and not is_link(p):
                    try:
                        p.rmdir()
                    except OSError:
                        pass
        if args.apply:
            try:
                day.rmdir()
            except OSError:
                pass
        out.append({"folder": day.as_posix(), "files": files, "bytes": size, "deleted": args.apply})
    print(json.dumps({"receipt": bool(receipt), "folders": out}))
    return out


def status(args):
    plan = read(args.plan)
    root = Path(plan["root"])
    out = {}
    for name, chain in plan["chains"].items():
        native = Path(chain["native"])
        done = completed(native)
        out[name] = {"arm": chain["arm"], "host": chain["host"], "completed": done, "of": chain["updates"],
                     "held": done >= plan["hold"]["update"] and not (root / plan["hold"]["cleared_file"]).exists(),
                     "done": (root / "chains" / name / "DONE.json").exists(),
                     "bytes": sum(f.stat().st_size for f in native.rglob("*") if f.is_file()) if native.exists()
                     else 0}
    print(json.dumps(out, indent=1))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    for flag in ("--qualification", "--calibration", "--binary", "--root", "--out", "--budget", "--o-timing",
                 "--namespace"):
        p.add_argument(flag, required=True)
    for flag in ("--parent", "--base", "--place", "--wrapper", "--reserve"):
        p.add_argument(flag, action="append")
    p.add_argument("--byte-cap", type=float, default=BYTE_CAP)
    p.add_argument("--regeneration-receipt")
    t = sub.add_parser("throughput")
    t.add_argument("--plan", required=True)
    t.add_argument("--host", required=True)
    t.add_argument("--arm", required=True, choices=ARMS)
    t.add_argument("--pack", type=int, default=1)
    t.add_argument("--repeats", type=int, default=None)
    t.add_argument("--stagger", type=float)
    r = sub.add_parser("run")
    r.add_argument("--plan", required=True)
    r.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    r.add_argument("--fast-poll", action="store_true", help=argparse.SUPPRESS)
    m = sub.add_parser("_members")
    m.add_argument("--plan", required=True)
    m.add_argument("--results", required=True)
    m.add_argument("--plan-sha256", required=True)
    m.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    m.add_argument("--fast-poll", action="store_true")
    th = sub.add_parser("thin")
    th.add_argument("--plan", required=True)
    th.add_argument("--apply", action="store_true")
    th.add_argument("--loop", type=float, default=0.0, help="repeat every this many seconds until STOP")
    pg = sub.add_parser("purge")
    pg.add_argument("--plan", required=True)
    pg.add_argument("--apply", action="store_true")
    s = sub.add_parser("status")
    s.add_argument("--plan", required=True)
    args = ap.parse_args(argv)
    if args.cmd == "throughput":
        if args.pack < 1:
            ap.error("--pack is at least 1")
        if args.pack == 1 and (args.repeats not in (None, 1) or args.stagger is not None):
            ap.error("--repeats and --stagger are for --pack 2 or more")
        if args.repeats is None:
            args.repeats = 1 if args.pack == 1 else 3
    {"plan": make_plan, "throughput": throughput, "run": run, "_members": members, "thin": thin, "purge": purge,
     "status": status}[args.cmd](args)


if __name__ == "__main__":
    main()
