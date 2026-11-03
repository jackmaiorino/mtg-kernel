"""EX1 Stage F launcher (design v3.3 section 5 "Stage F, the study"; collab LANES/spy-primary-direction-20261028
stagef/STAGE-F-PLAN.md section 6 and the lead's rulings R1 to R6). Nothing here decides a rule step or reads an
outcome: it trains Stage F's eight chains and records what the lead reads.

  stagef.py plan --qualification Q.json --calibration CALIBRATION.json --binary EXE --root ROOT --out PLAN.json
                 --parent r2=DOSE.json --parent r1=DOSE.json --base r2=BASE.json --base r1=BASE.json
                 --budget BUDGET.json --o-timing O-TIMING.json --namespace NS
                 --place CHAIN=HOST:CORES[:GROUP] [...] [--wrapper HOST=JSON_LIST] [--reserve HOST=GIB]
                 [--byte-cap BYTES] [--regeneration-receipt RECEIPT.json]
  stagef.py throughput --plan PLAN.json --host HOST --arm x|o [--pack K [--repeats R] [--stagger S]]
  stagef.py segcheck --plan PLAN.json --host HOST --arm x|o --out RECEIPT.json
  stagef.py run --plan PLAN.json --chain CHAIN [--chain CHAIN ...]
  stagef.py thin --plan PLAN.json [--apply]
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

Segments. The native trainer revalidates every completed iteration of its run on resume, trajectories and
checkpoints included, so bulk can leave only a finished run. Each chain therefore runs as consecutive native runs
over one pinned full config: segment 0 is updates 1 to 300 (in `chains/<chain>/native`, the Stage P layout, so the
Stage P reader reads the hold unchanged), then segments of 100 updates (`chains/<chain>/seg-NNN/native`). A segment's
config is the full config's slice of iterations with `ex1.update_offset` at the segment's first global update, and
its initial source is the source the next update of the previous segment would read: that segment's last source
with the checkpoint replaced by its last output checkpoint. Episode ids, seeds and placement keys are unchanged, so
a segmented chain trains as one run does; `segcheck` measures that on a host (states and trajectories, path fields
aside) and its record is the regeneration receipt `purge` relies on.

`run` refuses to start unless every pin matches, the host has a throughput record for the chain's arm, binary,
workers, cores and pack size whose state equals the qualified one, the run root holds less than the byte cap, and
the volume has room. Owned hosts only: every chain is metered at 0 USD (ruling R5). The in-Stage-F stop: a chain
stops when segment 0 ends (update 300, exactly) until the lead writes HOLD-300-CLEARED.json in the root;
STAGEF-STOPPED.json, or a STOP file, stops everything. An interrupted iteration is moved aside and rerun fresh
(mtg-kernel#336). The supervisor also stops every chain when the run root passes the byte cap.

Storage (ruling R6; ARTIFACT-LAW clauses 3, 4 and 6). After each segment ends, every finished segment's uncited bulk
(episode trajectories, and update checkpoints except the segment's last) moves into `_hold-thin` on the same
volume, each file first checked against the hash its records pin, with a row in the chain's own
thin/THIN-MANIFEST-<chain>.jsonl. Record files
(complete, collection, update, ex1 and ledger JSON) never move, so every segment's last checkpoint (updates 300, 400,
..., U) stays. `purge` deletes held files listed in the manifests once hold_days have passed since each move, or at
once when the plan pins passing segment checks for both arms; it never walks a tree, never follows a link, and runs
one at a time. Before every segment a driver checks storage admission: the run root's bytes plus every unfinished
chain's remaining segment bytes stay within the cap and the volume reserve; otherwise it exits EXIT_ROOM.
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
import copy
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
THIN = {"hold_days": 7}
SEGMENT = 100                          # updates per segment after the hold
BYTES_CHECK_S = 600
POLL_S = 30
EXIT_STOP = 5
EXIT_HOLD = 6
EXIT_ROOM = 7
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
            "segments": {"first": HOLD, "length": SEGMENT, "count": len(segments(updates))},
            "byte_budget": {"bytes_per_update": BYTES_PER_UPDATE,
                            "projected_bytes_unthinned": BYTES_PER_UPDATE * updates * len(CHAINS),
                            "cap_bytes": int(args.byte_cap)},
            "thin": dict(THIN, regeneration_receipts=[pin(r) for r in args.regeneration_receipt or []]),
            "hosts": {h: {"wrapper": wrappers.get(h, []), "reserve_gib": max(RESERVE_GIB, reserves.get(h, 0.0))}
                      for h in hosts},
            "chains": chains, "allotted_usd": 0.0}
    valid_receipts(plan)
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


# ---------------------------------------------------------------- segments

def segments(updates):
    """[(first global update index, length)]: the hold segment, then segments of SEGMENT updates."""
    out, start = [(0, HOLD)], HOLD
    while start < updates:
        n = min(SEGMENT, updates - start)
        out.append((start, n))
        start += n
    return out


def segment_native(chain, s):
    base = Path(chain["native"])
    return base if s == 0 else base.parent / f"seg-{s:03d}" / "native"


def segment_done(native, n):
    return Path(native, "completion.json").exists() and completed(native) == n


def progress(chain):
    """(completed updates over all segments, index of the first unfinished segment or None)."""
    total = 0
    for s, (_, n) in enumerate(segments(chain["updates"])):
        native = segment_native(chain, s)
        done = completed(native) if native.exists() else 0
        total += done
        if not segment_done(native, n):
            return total, s
    return total, None


def attempt_dir(iteration, complete):
    return iteration / [p for p in re.split(r"[\\/]", complete["update"]["path"]) if p.startswith("attempt-")][0]


def next_source(native):
    """The source the update after this run's last one reads: its last source, with the checkpoint replaced by its
    last output checkpoint (checked against the pinned hash)."""
    its = sorted(p for p in (Path(native) / "iterations").iterdir() if (p / "complete.json").exists())
    last = its[-1]
    complete = read(last / "complete.json")
    checkpoint = attempt_dir(last, complete) / "update" / "checkpoint.json"
    digest = complete["output_identity"]["checkpoint_sha256"]
    if not checkpoint.exists() or sha(checkpoint) != digest:
        raise SystemExit(f"stop: {checkpoint} is missing or differs from its pinned hash")
    source = copy.deepcopy(complete["source"])
    source["checkpoint"] = {"path": checkpoint.as_posix(), "sha256": digest}
    return source


def segment_config(full, start, n, native, source):
    cfg = {k: copy.deepcopy(v) for k, v in full.items() if k != "iterations"}
    cfg["iterations"] = copy.deepcopy(full["iterations"][start:start + n])
    cfg["output_directory"] = Path(native).as_posix()
    cfg["initial_source"] = source
    if "ex1" in cfg:
        cfg["ex1"]["update_offset"] = full["ex1"]["update_offset"] + start
    return cfg


# ---------------------------------------------------------------- storage (ruling R6)

def is_link(path):
    p = Path(path)
    return p.is_symlink() or (hasattr(p, "is_junction") and p.is_junction())


def tree_bytes(root):
    """Bytes of regular files under root; links and junctions are neither followed nor counted."""
    total, stack = 0, [Path(root)]
    while stack:
        d = stack.pop()
        try:
            entries = list(os.scandir(d))
        except FileNotFoundError:
            continue
        for e in entries:
            if e.is_symlink() or (hasattr(e, "is_junction") and e.is_junction()):
                continue
            if e.is_dir(follow_symlinks=False):
                stack.append(Path(e.path))
            elif e.is_file(follow_symlinks=False):
                total += e.stat(follow_symlinks=False).st_size
    return total


def bulk_files(iteration, keep_checkpoint):
    """An iteration's uncited bulk, each with the hash its records pin: trajectories (pinned by collection.json)
    and, unless kept, the update checkpoint (pinned by complete.json's output identity)."""
    complete = read(iteration / "complete.json")
    attempt = attempt_dir(iteration, complete)
    collection = read(attempt / "collect" / re.split(r"[\\/]", complete["collection"]["path"])[-1])
    out = [(attempt / "collect" / re.split(r"[\\/]", p["path"])[-1], p["sha256"])
           for p in collection.get("trajectories", [])]
    if not keep_checkpoint:
        out.append((attempt / "update" / "checkpoint.json", complete["output_identity"]["checkpoint_sha256"]))
    return out


def thin_chain(plan, name, apply):
    """Moves the bulk of every finished segment into the holding folder; returns (files, bytes)."""
    chain, root = plan["chains"][name], Path(plan["root"])
    files = size = 0
    for s, (_, n) in enumerate(segments(chain["updates"])):
        native = segment_native(chain, s)
        if not segment_done(native, n):
            break
        if is_link(native) or is_link(native / "iterations"):
            raise SystemExit(f"refused: {native} is a link")
        for i in range(n):
            it = native / "iterations" / f"{i:06d}"
            for path, digest in bulk_files(it, keep_checkpoint=(i == n - 1)):
                if not path.exists():
                    continue
                if is_link(path) or sha(path) != digest:
                    raise SystemExit(f"stop: {path} is a link or differs from the hash its records pin")
                dest = root / "_hold-thin" / name / path.relative_to(Path(chain["native"]).parent)
                files += 1
                size += path.stat().st_size
                if apply:
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    thin_manifest(root, name).parent.mkdir(parents=True, exist_ok=True)
                    os.replace(path, dest)           # same volume: a rename
                    with open(thin_manifest(root, name), "a", encoding="utf-8", newline="\n") as f:
                        f.write(json.dumps({"chain": name, "from": path.as_posix(), "to": dest.as_posix(),
                                            "bytes": dest.stat().st_size, "sha256": digest,
                                            "moved_at": time.time()}) + "\n")
    return files, size


def thin_manifest(root, name):
    """Each chain's driver appends only to its own manifest, so concurrent drivers never share a file."""
    return Path(root) / "thin" / f"THIN-MANIFEST-{name}.jsonl"


def thin_rows(root):
    return [r for name in CHAINS for r in manifest_rows(thin_manifest(root, name))]


def valid_receipts(plan):
    """The pinned segment-check records, each passing for this plan's binary, together covering both arms."""
    receipts = plan["thin"].get("regeneration_receipts") or []
    arms = set()
    for r in receipts:
        check_pin(r, "a regeneration receipt")
        rec = read(r["path"])
        if rec.get("schema") != "ex1-stagef-segcheck/v1" or rec.get("pass") is not True or \
                rec.get("binary_sha256") != plan["binary"]["sha256"]:
            raise SystemExit(f"refused: {r['path']} is not a passing segment check for this binary")
        arms.add(rec["arm"])
    if receipts and arms != set(ARMS):
        raise SystemExit(f"refused: the regeneration receipts cover arms {sorted(arms)}, not both")
    return bool(receipts)


def lock(path):
    """An exclusive operating-system lock on the file (msvcrt byte-range lock on Windows, flock elsewhere), held by
    this open handle; None while any other handle holds it. The system releases it when the holder exits, so there
    is nothing to reclaim and no owner to inspect."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    f = open(path, "a+b")
    try:
        if os.name == "nt":
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        f.close()
        return None
    return f


def unlock(f):
    try:
        if os.name == "nt":
            import msvcrt
            f.seek(0)
            msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_UN)
    finally:
        f.close()


def manifest_rows(path):
    return [json.loads(x) for x in Path(path).read_text(encoding="utf-8").splitlines() if x.strip()] \
        if Path(path).exists() else []


def purge_held(plan, apply):
    """Deletes the held files the thin manifests list, each once hold_days have passed since its move (or at once
    when the plan pins passing segment checks for both arms), after checking that neither the holding folder nor any
    folder between it and the file is a link, that the file resolves inside the holding folder, and that it still
    has its recorded hash. Nothing is walked; emptied folders are removed bottom-up. One purger at a time (a lock);
    a second caller returns without deleting."""
    root = Path(plan["root"])
    receipt = valid_receipts(plan)
    held_lock = lock(root / "thin" / "PURGE.lock") if apply else None
    if apply and held_lock is None:
        return {"files": 0, "bytes": 0, "deleted": False, "busy": True}
    try:
        hold = root / "_hold-thin"
        if is_link(hold):
            raise SystemExit(f"stop: {hold} is a link")
        real_hold = Path(os.path.realpath(hold))
        done = {r["to"] for r in manifest_rows(root / "thin" / "PURGE-MANIFEST.jsonl")}
        cutoff = time.time() - plan["thin"]["hold_days"] * 86400
        out = {"files": 0, "bytes": 0, "deleted": apply, "receipt": receipt}
        parents = set()
        for row in thin_rows(root):
            if row["to"] in done or (not receipt and row["moved_at"] > cutoff):
                continue
            p = Path(row["to"])
            try:
                rel = p.relative_to(hold)
            except ValueError:
                raise SystemExit(f"stop: held file {p} is not under {hold}")
            # Lexically, every folder from the holding folder down to the file, and the file.
            between = [hold.joinpath(*rel.parts[:k]) for k in range(1, len(rel.parts) + 1)]
            if not p.exists() or any(is_link(q) for q in between) or not p.is_file() or \
                    real_hold not in Path(os.path.realpath(p)).parents or sha(p) != row["sha256"]:
                raise SystemExit(f"stop: held file {p} is missing, linked, outside the holding folder or changed")
            out["files"] += 1
            out["bytes"] += row["bytes"]
            if apply:
                p.unlink()
                parents.update(between[:-1])
                with open(root / "thin" / "PURGE-MANIFEST.jsonl", "a", encoding="utf-8", newline="\n") as f:
                    f.write(json.dumps({"to": row["to"], "bytes": row["bytes"], "sha256": row["sha256"],
                                        "purged_at": time.time(), "receipt": receipt}) + "\n")
        for d in sorted(parents, key=lambda q: len(q.parts), reverse=True):
            try:
                if not is_link(d):
                    d.rmdir()
            except OSError:
                pass
        return out
    finally:
        if held_lock is not None:
            unlock(held_lock)


# ---------------------------------------------------------------- run

def held(plan, name):
    """True when the chain's hold segment is done, the lead has not cleared the hold, and updates remain."""
    chain, root = plan["chains"][name], Path(plan["root"])
    total, unfinished = progress(chain)
    return unfinished is not None and total >= HOLD and not (root / plan["hold"]["cleared_file"]).exists()


def check(plan, name, allow_held=False):
    """A chain's pins and throughput record; returns the bytes its current segment still has to write."""
    chain = plan["chains"][name]
    root = Path(plan["root"])
    for key, what in (("config", "the chain config"), ("parent", "the parent"), ("base", "the base config")):
        check_pin(chain[key], what)
    guard(plan, name, chain)
    if (root / plan["hold"]["stopped_file"]).exists():
        raise SystemExit(f"refused: Stage F is stopped ({plan['hold']['stopped_file']})")
    if held(plan, name) and not allow_held:
        raise SystemExit(f"refused: {name} is held at update {HOLD} for the lead's steps 1 and 2 read; the lead "
                         f"writes {plan['hold']['cleared_file']} to continue")
    total, unfinished = progress(chain)
    if unfinished is None:
        raise SystemExit(f"refused: {name} has finished")
    start, n = segments(chain["updates"])[unfinished]
    native = segment_native(chain, unfinished)
    return (n - (completed(native) if native.exists() else 0)) * plan["byte_budget"]["bytes_per_update"]


def group_checks(plan, names):
    chain = plan["chains"][names[0]]
    group = chain.get("group")
    members = sorted(n for n, c in plan["chains"].items() if group is not None and c.get("group") == group)
    if group is None and len(names) > 1:
        raise SystemExit("refused: only the chains of one pack group run together")
    if group is not None and sorted(names) != members:
        raise SystemExit(f"refused: pack group {group} runs all its chains together: {members}")
    if group is not None and all(held(plan, name) for name in names):
        raise SystemExit(f"refused: pack group {group} is held at update {HOLD} for the lead's steps 1 and 2 read; "
                         f"the lead writes {plan['hold']['cleared_file']} to continue")
    for name in names:
        check(plan, name, allow_held=group is not None)
    reason = room(plan)
    if reason:
        raise SystemExit(f"refused: {reason}")
    return chain


def room(plan):
    """Storage admission for any segment start: the run root's bytes plus every unfinished chain's remaining bytes in
    its current segment (an upper bound on what concurrent drivers can still write before their next check) stay
    within the cap, and the volume keeps that much plus its reserve free. Returns a reason, or None."""
    need = 0
    for chain in plan["chains"].values():
        _, unfinished = progress(chain)
        if unfinished is not None:
            _, n = segments(chain["updates"])[unfinished]
            native = segment_native(chain, unfinished)
            need += (n - (completed(native) if native.exists() else 0)) * plan["byte_budget"]["bytes_per_update"]
    used = tree_bytes(plan["root"])
    if used + need > plan["byte_budget"]["cap_bytes"]:
        return (f"the run root holds {used / 1e9:.1f} GB and unfinished segments may add {need / 1e9:.1f} GB, past "
                f"the {plan['byte_budget']['cap_bytes'] / 1e9:.0f} GB cap")
    free = shutil.disk_usage(Path(plan["root"])).free
    reserve = max(h["reserve_gib"] for h in plan["hosts"].values()) * (1 << 30)
    if free < need + reserve:
        return f"{free / 1e9:.1f} GB free, need {need / 1e9:.1f} GB plus the {reserve / 2**30:.0f} GiB reserve"
    return None


def inner_command(plan_path, name, fast_poll):
    cmd = [sys.executable, str(Path(__file__).resolve()), "_drive", "--plan", str(Path(plan_path).resolve()),
           "--plan-sha256", sha(plan_path), "--chain", name]
    return cmd + (["--fast-poll"] if fast_poll else [])


def run(args):
    plan = load_plan(args.plan)
    names = list(dict.fromkeys(args.chain))
    chain = group_checks(plan, names)
    group = chain.get("group")
    wrapper = [part.replace("{cores}", str(chain["cores"])).replace("{chain}", group or names[0])
               for part in plan["hosts"][chain["host"]]["wrapper"]]
    if group is None:
        raise SystemExit(supervise(plan, names, wrapper + inner_command(args.plan, names[0], args.fast_poll),
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


def checked_plan(args):
    if not Path(args.plan).is_absolute() or sha(args.plan) != args.plan_sha256:
        raise SystemExit("refused: an internal command needs the launching plan's absolute path and hash")
    return load_plan(args.plan)


def members(args):
    """Inside a pack group's claim: one `_drive` per member; each stops at the hold on its own."""
    plan = checked_plan(args)
    names = list(dict.fromkeys(args.chain))
    chain = group_checks(plan, names)
    if chain.get("group") is None:
        raise SystemExit("refused: _members runs a pack group")
    stagep.require_claim(plan, chain["host"])
    procs = {}
    try:
        for name in names:
            procs[name] = subprocess.Popen(inner_command(args.plan, name, args.fast_poll))
    except BaseException:
        for proc in procs.values():
            stop_tree(proc)
        raise
    codes = {name: proc.wait() for name, proc in procs.items()}
    write(args.results, codes)
    raise SystemExit(0 if not any(codes.values()) else 1)


def drive(args):
    """Inside the claim: runs the chain's unfinished segments in order, thinning (and, with a regeneration
    receipt, purging) after each; exits EXIT_HOLD at the hold, EXIT_STOP on a stop file."""
    plan = checked_plan(args)
    name = args.chain
    chain, root = plan["chains"][name], Path(plan["root"])
    stagep.require_claim(plan, chain["host"])
    if held(plan, name):
        raise SystemExit(EXIT_HOLD)
    check(plan, name)
    binary = plan["binary"]["path"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]   # .py: test stand-ins only
    full = read(chain["config"]["path"])
    for s, (start, n) in enumerate(segments(chain["updates"])):
        native = segment_native(chain, s)
        if segment_done(native, n):
            continue
        if (root / "STOP").exists() or (root / plan["hold"]["stopped_file"]).exists():
            raise SystemExit(EXIT_STOP)
        if start >= HOLD and not (root / plan["hold"]["cleared_file"]).exists():
            log(root, chain=name, event="hold", completed=start)
            raise SystemExit(EXIT_HOLD)
        reason = room(plan)
        if reason:
            log(root, chain=name, event="no-room", segment=s, reason=reason)
            raise SystemExit(EXIT_ROOM)
        source = full["initial_source"] if s == 0 else next_source(segment_native(chain, s - 1))
        cfg = segment_config(full, start, n, native, source)
        path = root / "chains" / name / "segments" / f"seg-{s:03d}.json"
        if path.exists() and read(path) != cfg:
            raise SystemExit(f"stop: {path} differs from the segment config derived from the pinned plan")
        write(path, cfg)
        moved = quarantine(native, root, name)
        log(root, chain=name, event="segment", segment=s, first_update=start + 1, updates=n,
            config=pin(path), source=source["checkpoint"], moved=moved)
        code = subprocess.call(exe + [str(path)])
        if code != 0 or not segment_done(native, n):
            log(root, chain=name, event="segment-failed", segment=s, exit=code, completed=completed(native))
            raise SystemExit(code or 1)
        files, size = thin_chain(plan, name, apply=True)
        purged = purge_held(plan, apply=True) if plan["thin"].get("regeneration_receipts") else None
        log(root, chain=name, event="segment-done", segment=s, thinned_files=files, thinned_bytes=size,
            purged=purged)
    raise SystemExit(0)


def supervise(plan, names, cmd, fast_poll, results=None):
    """Runs the named chains; STOP, STAGEF-STOPPED.json or a run root past the byte cap stops them. Every chain is
    metered at 0 USD (owned hosts, ruling R5)."""
    root = Path(plan["root"])
    chains = {name: plan["chains"][name] for name in names}
    paths = {name: root / "chains" / name / "STATE.json" for name in names}
    states = {name: read(p) if p.exists() else {"metered_seconds": 0.0, "sessions": []} for name, p in paths.items()}
    sessions = {}
    for name, chain in chains.items():
        done, _ = progress(chain)
        log(root, chain=name, event="start", completed=done, cmd=cmd)
        sessions[name] = {"started": time.time(), "completed_at_start": done}
    if results is not None:
        Path(results).unlink(missing_ok=True)
    proc = subprocess.Popen(cmd)
    outcome = None
    last = last_bytes = time.time()
    while proc.poll() is None:
        time.sleep(POLL_S if not fast_poll else 0.2)
        now = time.time()
        for name in chains:
            states[name]["metered_seconds"] += now - last
            write(paths[name], states[name])
        last = now
        if (root / "STOP").exists() or (root / plan["hold"]["stopped_file"]).exists():
            outcome = "stop"
        elif fast_poll or now - last_bytes >= BYTES_CHECK_S:
            last_bytes = now
            reserve = max(h["reserve_gib"] for h in plan["hosts"].values()) * (1 << 30)
            if tree_bytes(root) > plan["byte_budget"]["cap_bytes"] or shutil.disk_usage(root).free < reserve:
                outcome = "bytes"
        if outcome:
            stop_tree(proc)
            break
    code = proc.wait()
    member = read(results) if results is not None and Path(results).exists() else {}
    codes = {name: member.get(name, code) for name in names}
    if outcome is None and all(c == EXIT_HOLD for c in codes.values()):
        outcome = "hold"
    if outcome is None and any(c == EXIT_ROOM for c in codes.values()):
        outcome = "room"
    for name, chain in chains.items():
        states[name]["metered_seconds"] += time.time() - last
        done, unfinished = progress(chain)
        session = sessions[name]
        session.update(ended=time.time(), exit=codes[name], outcome=outcome, completed=done)
        if outcome in ("stop", "bytes") and unfinished is not None:
            session["moved_after"] = quarantine(segment_native(chain, unfinished), root, name)
        states[name]["sessions"].append(session)
        write(paths[name], states[name])
        log(root, chain=name, event="end", exit=codes[name], outcome=outcome, completed=done, usd=0.0)
        if outcome is None and codes[name] == 0 and unfinished is None:
            last_native = segment_native(chain, len(segments(chain["updates"])) - 1)
            write(root / "chains" / name / "DONE.json",
                  {"chain": name, "arm": chain["arm"], "completed_updates": done, "metered_usd": 0.0,
                   "segments": len(segments(chain["updates"])),
                   "final_completion": pin(last_native / "completion.json"),
                   "final_checkpoint": next_source(last_native)["checkpoint"]})
    if outcome in ("stop", "bytes"):
        return EXIT_STOP
    if outcome == "room":
        return EXIT_ROOM
    if outcome == "hold":
        return EXIT_HOLD
    return next((c for c in [code, *codes.values()] if c != 0), 0)


# ---------------------------------------------------------------- segment check (the regeneration receipt)

def segcheck(args):
    """On this host, inside its claim wrapper: the first two updates of the r2 pair-1 chain of the arm run once as
    one native run and once as two one-update segments; the after states must be equal and the trajectories equal
    apart from path and timing fields (qual.compare's rule). The record is the regeneration receipt for thinning."""
    plan = load_plan(args.plan)
    name = f"r2-p1-{args.arm}"
    chain = plan["chains"][name]
    full = read(chain["config"]["path"])
    out = Path(plan["root"]) / "segcheck" / f"{args.host}-{args.arm}-{time.strftime('%Y%m%dT%H%M%S')}"
    wrapper = [part.replace("{cores}", str(chain["cores"])).replace("{chain}", "segcheck")
               for part in plan["hosts"][args.host]["wrapper"]]
    binary = plan["binary"]["path"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]

    def go(cfg, label):
        path = out / f"{label}.json"
        write(path, cfg)
        return subprocess.call(wrapper + exe + [str(path)])

    codes = [go(segment_config(full, 0, 2, out / "whole" / "native", full["initial_source"]), "whole"),
             go(segment_config(full, 0, 1, out / "seg-a" / "native", full["initial_source"]), "seg-a")]
    codes.append(go(segment_config(full, 1, 1, out / "seg-b" / "native", next_source(out / "seg-a" / "native")),
                    "seg-b"))
    whole = list(qual.iterations(out / "whole" / "native"))
    split = list(qual.iterations(out / "seg-a" / "native")) + list(qual.iterations(out / "seg-b" / "native"))
    states = ([u["after_state_sha256"] for _, u, _ in whole], [u["after_state_sha256"] for _, u, _ in split])
    trajectories = len(whole) == len(split) and all(
        [qual.strip(read(p["path"]), qual.PATH_KEYS) for p in a["trajectories"]] ==
        [qual.strip(read(p["path"]), qual.PATH_KEYS) for p in b["trajectories"]]
        for (_, _, a), (_, _, b) in zip(whole, split))
    record = {"schema": "ex1-stagef-segcheck/v1", "host": args.host, "hostname": socket.gethostname(),
              "binary_sha256": plan["binary"]["sha256"], "chain": name, "arm": args.arm, "exits": codes,
              "states_whole": states[0], "states_segmented": states[1],
              "pass": not any(codes) and len(whole) == 2 and states[0] == states[1] and trajectories,
              "trajectories_equal_apart_from_paths": trajectories,
              "regeneration_recipe": "pinned binary by hash, the plan's full chain config, and segment_config from "
                                     "the nearest kept checkpoint (every segment's last update) forward"}
    write(args.out, record)
    print(json.dumps(record, indent=1))
    if not record["pass"]:
        raise SystemExit("segment check failed")


def thin(args):
    plan = load_plan(args.plan)
    print(json.dumps({name: dict(zip(("files", "bytes"), thin_chain(plan, name, args.apply)))
                      for name in plan["chains"]}))


def purge(args):
    print(json.dumps(purge_held(load_plan(args.plan), args.apply)))


def status(args):
    plan = read(args.plan)
    root = Path(plan["root"])
    out = {}
    for name, chain in plan["chains"].items():
        done, unfinished = progress(chain)
        out[name] = {"arm": chain["arm"], "host": chain["host"], "completed": done, "of": chain["updates"],
                     "segment": unfinished, "held": held(plan, name),
                     "done": (root / "chains" / name / "DONE.json").exists()}
    out["run_root_bytes"] = tree_bytes(root)
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
    p.add_argument("--regeneration-receipt", action="append", help="a passing segcheck record; one per arm")
    t = sub.add_parser("throughput")
    t.add_argument("--plan", required=True)
    t.add_argument("--host", required=True)
    t.add_argument("--arm", required=True, choices=ARMS)
    t.add_argument("--pack", type=int, default=1)
    t.add_argument("--repeats", type=int, default=None)
    t.add_argument("--stagger", type=float)
    sc = sub.add_parser("segcheck")
    sc.add_argument("--plan", required=True)
    sc.add_argument("--host", required=True)
    sc.add_argument("--arm", required=True, choices=ARMS)
    sc.add_argument("--out", required=True)
    r = sub.add_parser("run")
    r.add_argument("--plan", required=True)
    r.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    r.add_argument("--fast-poll", action="store_true", help=argparse.SUPPRESS)
    m = sub.add_parser("_members")
    m.add_argument("--plan", required=True)
    m.add_argument("--plan-sha256", required=True)
    m.add_argument("--results", required=True)
    m.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    m.add_argument("--fast-poll", action="store_true")
    d = sub.add_parser("_drive")
    d.add_argument("--plan", required=True)
    d.add_argument("--plan-sha256", required=True)
    d.add_argument("--chain", required=True, choices=sorted(CHAINS))
    d.add_argument("--fast-poll", action="store_true")
    th = sub.add_parser("thin")
    th.add_argument("--plan", required=True)
    th.add_argument("--apply", action="store_true")
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
    {"plan": make_plan, "throughput": throughput, "segcheck": segcheck, "run": run, "_members": members,
     "_drive": drive, "thin": thin, "purge": purge, "status": status}[args.cmd](args)


if __name__ == "__main__":
    main()
