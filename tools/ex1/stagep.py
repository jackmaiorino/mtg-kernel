"""EX1 Stage P launcher (design v3.3 section 5; qualification docs/experiments/ex1-20261029). Nothing here decides a
rule step: it trains the four mechanism-pilot chains and records what the lead reads.

  stagep.py plan --qualification Q.json --binary EXE --root ROOT --out PLAN.json
                 --parent r2=DOSE.json --parent r1=DOSE.json --base r2=BASE.json --base r1=BASE.json
                 --place CHAIN=HOST:CORES:USD_PER_HOUR:BUDGET_USD[:GROUP] [...] [--wrapper HOST=JSON_LIST]
                 [--reserve HOST=GIB]
  stagep.py localize --plan PLAN.json --host HOST --export DIR --map OLD=NEW [...]
                                                       a plan for another host and the files to copy there
  stagep.py throughput --plan PLAN.json --host HOST [--pack K [--repeats R] [--stagger S]]
                                                       one EX1 update (the qualification's update 100) on this host at
                                                       the planned cores and workers; its state must equal the
                                                       qualification's on Windows. With --pack, K copies share one
                                                       claim of those cores, each running the update R times
  stagep.py run --plan PLAN.json --chain CHAIN [--chain CHAIN ...]
                                                       the guarded chain run (resumable; run it again after a stop);
                                                       a pack group's chains run together, naming every member
  stagep.py status --plan PLAN.json                     updates done, metered USD and a projected end per chain

Packing: chains placed with the same GROUP (same host and cores) run together in one claim of those cores, K = the
group's size. Each chain's work is unchanged (fixed-work and deterministic), so its outputs are identical; what
changes is that one chain's straggler episode leaves cores to the others. A pack group needs a throughput record
measured at the same K (THROUGHPUT-HOST-kK.json; K = 1 keeps THROUGHPUT-HOST.json).

The plan pins the design's frozen inputs: the qualified binary, the calibrated c* per lineage (read from the
qualification manifest), kappa = 1, 300 updates per chain, the parents, the RT1 bases, and the episode and placement
keys (namespace, run id, global update). `run` refuses to start unless:
- every pin still matches;
- this host has a throughput record for the chain's host, binary, cores, workers and pack size K (COMPUTE-POLICY
  item 5), whose state equals the qualification's in every copy;
- free space covers the chain's remaining bytes plus the host's reserve.
An iteration interrupted after its collection began cannot be resumed (mtg-kernel#336), so `run` moves it aside and
the run reruns it fresh. Each chain meters USD as its host rate times the wall time it runs; at its allotment (the
allotments sum to at most the 25 USD ceiling) the chain stops and records Stage P as inconclusive on budget.
A STOP file in the root stops every chain at once.
"""
import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import socket
import subprocess
import sys
import time
from pathlib import Path

import psutil

sys.path.insert(0, str(Path(__file__).resolve().parent))
import qual  # noqa: E402

SCHEMA = "ex1-stagep-plan/v1"
CEILING_USD = 25.0
UPDATES = 300
KAPPA = 1.0
NAMESPACE = "ex1-stagep-20261029"
CHAINS = {"r2-c1": "r2", "r2-c2": "r2", "r1-c1": "r1", "r1-c2": "r1"}
WORKERS = 10
BYTES_PER_UPDATE = 53_000_000          # qualification: trajectories 32 MB, checkpoint 21 MB, EX1 records 0.1 MB
BYTE_CAP = 70_000_000_000              # ARTIFACT-LAW clause 1: the declared cap over 4 x 300 updates (64 GB projected)
RESERVE_GIB = 60.0                     # ARTIFACT-LAW clause 1: free space every target volume keeps
POLL_S = 30
EXIT_BUDGET = 4
EXIT_STOP = 5
# The throughput update: the qualification's resume check, update 100 of run ex1-qual-det-r2 from the r2 parent.
THROUGHPUT = {"namespace": "ex1-qual-20261029", "run": "ex1-qual-det-r2", "offset": 100, "coefficient": 1.0}
THROUGHPUT_NATIVE = "<THROUGHPUT_NATIVE>"


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=1) + "\n", encoding="utf-8", newline="\n")
    tmp.replace(path)


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def pin(path):
    return {"path": Path(path).as_posix(), "sha256": sha(path)}


def check_pin(p, what):
    if not Path(p["path"]).exists() or sha(p["path"]) != p["sha256"]:
        raise SystemExit(f"refused: {what} differs from its plan pin ({p['path']})")


def log(root, **row):
    row["at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    path = Path(root) / "LAUNCH.jsonl"
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "a", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(row) + "\n")


# ---------------------------------------------------------------- plan

def parse_pairs(items, what):
    out = {}
    for item in items or []:
        key, _, value = item.partition("=")
        if not value:
            raise SystemExit(f"{what} needs NAME=VALUE: {item}")
        out[key] = value
    return out


def make_plan(args):
    q = read(args.qualification)
    binary_sha = sha(args.binary)
    if binary_sha != q["engine"]["binaries"]["native_expanded_training_run_v1"]:
        raise SystemExit("refused: the binary is not the qualified one")
    failed = [name for name, check in q["checks"].items() if check.get("pass") is False]
    if failed:
        raise SystemExit(f"refused: qualification checks failed: {failed}")
    dose = {}
    for lineage, c in q["calibration"].items():
        if not c["k2"]["pass"]:
            raise SystemExit(f"refused: the {lineage} calibration check did not pass")
        dose[lineage] = c["k1"]["c_star"]
    parents, bases = parse_pairs(args.parent, "--parent"), parse_pairs(args.base, "--base")
    wrappers = {k: json.loads(v) for k, v in parse_pairs(args.wrapper, "--wrapper").items()}
    reserves = {k: float(v) for k, v in parse_pairs(args.reserve, "--reserve").items()}
    places = parse_pairs(args.place, "--place")
    if set(places) != set(CHAINS):
        raise SystemExit(f"refused: place every chain exactly once ({sorted(CHAINS)})")
    root = Path(args.root)
    chains, total = {}, 0.0
    groups = {}
    for name, lineage in CHAINS.items():
        fields = places[name].split(":")
        if len(fields) not in (4, 5):
            raise SystemExit(f"refused: --place {name}=HOST:CORES:USD_PER_HOUR:BUDGET_USD[:GROUP]")
        host, cores, rate, budget = fields[:4]
        if len(fields) == 5:
            groups.setdefault(fields[4], []).append(name)
        cores, rate, budget = int(cores), float(rate), float(budget)
        if cores < 1 or rate < 0 or budget < 0 or (rate > 0) != (budget > 0):
            raise SystemExit(f"refused: {name} needs cores >= 1 and a budget exactly when its host costs money")
        total += budget
        run = f"ex1-stagep-{name}"
        native = root / "chains" / name / "native"
        cfg = qual.build_config(read(bases[lineage]), read(parents[lineage]), native, run, NAMESPACE, 0, UPDATES,
                                WORKERS, dose[lineage], KAPPA)
        config_path = root / "configs" / f"{name}.json"
        write(config_path, cfg)
        chains[name] = {"lineage": lineage, "run": run, "namespace": NAMESPACE, "updates": UPDATES,
                        "coefficient": dose[lineage], "kappa": KAPPA, "workers": WORKERS, "host": host,
                        "cores": cores, "usd_per_hour": rate, "budget_usd": budget, "config": pin(config_path),
                        "parent": pin(parents[lineage]), "base": pin(bases[lineage]),
                        "native": native.as_posix()}
    for group, members in groups.items():
        if not group or len({(chains[m]["host"], chains[m]["cores"]) for m in members}) != 1:
            raise SystemExit(f"refused: pack group {group!r} needs a name and one host and core count")
        if any(chains[m]["usd_per_hour"] > 0 for m in members):
            raise SystemExit(f"refused: pack group {group!r} is on a paid host (budgets are metered per chain)")
        for m in members:
            chains[m].update(group=group, pack=len(members))
    if total > CEILING_USD + 1e-9:
        raise SystemExit(f"refused: allotments sum to {total} USD, above the {CEILING_USD} USD ceiling")
    hosts = sorted({c["host"] for c in chains.values()})
    plan = {"schema": SCHEMA, "design": q["design"], "qualification": pin(args.qualification),
            "binary": pin(args.binary), "root": root.as_posix(), "ceiling_usd": CEILING_USD,
            "byte_budget": {"bytes_per_update": BYTES_PER_UPDATE,
                            "projected_bytes": BYTES_PER_UPDATE * UPDATES * len(CHAINS), "cap_bytes": BYTE_CAP},
            "hosts": {h: {"wrapper": wrappers.get(h, []), "reserve_gib": max(RESERVE_GIB, reserves.get(h, 0.0))}
                      for h in hosts},
            "chains": chains, "allotted_usd": total}
    write(args.out, plan)
    print(json.dumps({"plan": Path(args.out).as_posix(), "allotted_usd": total, "hosts": hosts}))


def referenced(value, out):
    """Every {"path", "sha256"} pin inside a config (sources, opponents, imports)."""
    if isinstance(value, dict):
        if set(value) >= {"path", "sha256"} and isinstance(value.get("path"), str):
            out[value["path"]] = value["sha256"]
        for v in value.values():
            referenced(v, out)
    elif isinstance(value, list):
        for v in value:
            referenced(v, out)
    return out


def remap(text, mapping):
    """One pass over the text, longest prefix first, so a mapped path is never mapped again."""
    table = dict(mapping)
    pattern = "|".join(re.escape(old) for old in sorted(table, key=len, reverse=True))
    return re.sub(pattern, lambda m: table[m.group(0)], text)


def localize(args):
    """A plan for another host: path prefixes mapped (OLD=NEW), that host's chain configs rewritten and re-pinned in
    an export folder, and COPY.json listing every file to place on the host (local path, host path, sha256)."""
    plan = load_plan(args.plan)
    mapping = [tuple(m.split("=", 1)) for m in args.map]
    export = Path(args.export)
    copies = {}
    local = json.loads(json.dumps(plan))
    for name, chain in plan["chains"].items():
        if chain["host"] != args.host:
            continue
        cfg_text = Path(chain["config"]["path"]).read_text(encoding="utf-8")
        for path, digest in referenced(json.loads(cfg_text), {}).items():
            if not path.startswith(chain["native"]):
                copies[path] = digest
        new_cfg = export / "configs" / f"{name}.json"
        new_cfg.parent.mkdir(parents=True, exist_ok=True)
        new_cfg.write_text(remap(cfg_text, mapping), encoding="utf-8", newline="\n")
        # The whole plan is remapped below, so the path stays unmapped here.
        local["chains"][name]["config"] = {"path": chain["config"]["path"], "sha256": sha(new_cfg)}
        copies[new_cfg.as_posix()] = local["chains"][name]["config"]["sha256"]
        for key in ("parent", "base"):
            copies[chain[key]["path"]] = chain[key]["sha256"]
    for key in ("qualification", "binary"):
        copies[plan[key]["path"]] = plan[key]["sha256"]
    # The throughput update, prebuilt here where the dose records' paths resolve, with its inputs.
    tcfg = throughput_config(plan, THROUGHPUT_NATIVE)
    for path, digest in referenced(tcfg, {}).items():
        copies[path] = digest
    tpath = export / "throughput-config.json"
    tpath.write_text(remap(json.dumps(tcfg, indent=1) + "\n", mapping), encoding="utf-8", newline="\n")
    copies[tpath.as_posix()] = sha(tpath)
    local = json.loads(remap(json.dumps(local), mapping))
    local["throughput_config"] = {"path": remap(plan["root"], mapping) + "/throughput-config.json",
                                  "sha256": sha(tpath)}
    for name in local["chains"]:
        if plan["chains"][name]["host"] == args.host:
            local["chains"][name]["config"]["sha256"] = sha(export / "configs" / f"{name}.json")
    write(export / "PLAN.json", local)
    rows = []
    for path, digest in sorted(copies.items()):
        target = remap(path, mapping)
        if path == tpath.as_posix():
            target = remap(plan["root"], mapping) + "/throughput-config.json"
        elif path.startswith(export.as_posix()):
            target = remap(plan["root"], mapping) + "/configs/" + Path(path).name
        rows.append({"local": path, "host": target, "sha256": digest})
    write(export / "COPY.json", {"host": args.host, "plan": remap(plan["root"], mapping) + "/PLAN.json", "files": rows})
    print(json.dumps({"export": export.as_posix(), "files": len(rows)}))


def load_plan(path):
    plan = read(path)
    if plan.get("schema") != SCHEMA:
        raise SystemExit("refused: not an EX1 Stage P plan")
    check_pin(plan["qualification"], "the qualification manifest")
    check_pin(plan["binary"], "the binary")
    return plan


# ---------------------------------------------------------------- processes

def command(plan, host, config):
    binary = plan["binary"]["path"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]   # .py: test stand-ins only
    return [part for part in plan["hosts"][host]["wrapper"]] + exe + [config]


def stop_tree(proc):
    """Stops the spawned process and every descendant, by exact PID."""
    try:
        parent = psutil.Process(proc.pid)
        family = parent.children(recursive=True) + [parent]
    except psutil.NoSuchProcess:
        return
    for p in family:
        try:
            p.kill()
        except psutil.NoSuchProcess:
            pass
    psutil.wait_procs(family, timeout=30)


def quarantine(native, root, chain):
    """mtg-kernel#336: an iteration without complete.json is moved aside, so the run reruns it fresh."""
    moved = []
    iterations = Path(native) / "iterations"
    if not iterations.exists():
        return moved
    for d in sorted(p for p in iterations.iterdir() if p.is_dir()):
        if not (d / "complete.json").exists():
            dest = Path(root) / "chains" / chain / "interrupted" / f"{d.name}-{time.strftime('%Y%m%dT%H%M%S')}"
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(d), str(dest))
            moved.append({"from": d.as_posix(), "to": dest.as_posix()})
    return moved


def completed(native):
    return len(list(Path(native, "iterations").glob("*/complete.json")))


# ---------------------------------------------------------------- throughput

def throughput(args):
    plan = load_plan(args.plan)
    chains = [c for c in plan["chains"].values() if c["host"] == args.host]
    if not chains:
        raise SystemExit(f"no chain is placed on {args.host}")
    cores = max(c["cores"] for c in chains)
    q = read(plan["qualification"]["path"])
    out = Path(plan["root"]) / "throughput" / f"{args.host}-c{cores}-{time.strftime('%Y%m%dT%H%M%S')}"
    config_path = out / "config.json"
    if "throughput_config" in plan:
        # A localized plan carries the config prebuilt where the dose records' paths resolve.
        check_pin(plan["throughput_config"], "the throughput config")
        text = Path(plan["throughput_config"]["path"]).read_text(encoding="utf-8")
        config_path.parent.mkdir(parents=True, exist_ok=True)
        config_path.write_text(text.replace(THROUGHPUT_NATIVE, (out / "native").as_posix()), encoding="utf-8",
                               newline="\n")
    else:
        write(config_path, throughput_config(plan, out / "native"))
    wrapper = [part.replace("{cores}", str(cores)).replace("{chain}", "throughput")
               for part in plan["hosts"][args.host]["wrapper"]]
    plan["hosts"][args.host]["wrapper"] = wrapper
    expected = q["checks"]["resume_determinism"]["states"][0]
    windows = platform.system() == "Windows"
    if args.pack == 1 and args.repeats == 1:
        started = time.time()
        code = subprocess.call(command(plan, args.host, str(config_path)))
        wall = time.time() - started
        receipt, update, collection = next(qual.iterations(out / "native"))
        record = {"schema": "ex1-stagep-throughput/v1", "host": args.host, "hostname": socket.gethostname(),
                  "platform": platform.platform(), "binary_sha256": plan["binary"]["sha256"], "cores": cores,
                  "workers": WORKERS, "pack": 1, "exit": code, "wall_seconds": wall,
                  "collection_wall_seconds": collection["collection_elapsed_seconds"],
                  "process_cpu_seconds": receipt.get("driver_process_cpu_seconds"),
                  "after_state_sha256": update["after_state_sha256"], "expected_state_sha256": expected,
                  "state_equal": update["after_state_sha256"] == expected, "state_required": windows,
                  "reference_collection_wall_seconds": {"desktop_10_cores": 218.4, "desktop_5_cores": 363.2}}
    else:
        record = packed_throughput(plan, args, out, config_path, cores, expected, windows)
    path = throughput_path(plan, args.host, args.pack)
    write(path, record)
    print(json.dumps(record, indent=1))
    if record["exit"] != 0 or (windows and not record["state_equal"]):
        raise SystemExit("throughput run failed or its state differs from the qualification's")


def throughput_path(plan, host, pack):
    name = f"THROUGHPUT-{host}.json" if pack == 1 else f"THROUGHPUT-{host}-k{pack}.json"
    return Path(plan["root"]) / "throughput" / name


def packed_throughput(plan, args, out, config_path, cores, expected, windows):
    """K copies of the throughput update in one claim, each running it R times in fresh output folders; copy i
    starts i * S seconds late (S defaults to the K = 1 record's wall over K), so the copies' straggler phases do not
    all line up, as they would not between chains that drifted apart. The group's rate counts every completed
    update over the span from the first start to the last end, ramps included."""
    text = config_path.read_text(encoding="utf-8")
    single = throughput_path(plan, args.host, 1)
    stagger = args.stagger
    if stagger is None:
        stagger = read(single)["wall_seconds"] / args.pack if single.exists() else 0.0
    plan_lanes = []
    for i in range(args.pack):
        configs = []
        for r in range(args.repeats):
            native = out / f"copy-{i}" / f"rep-{r}" / "native"
            cfg = out / f"copy-{i}" / f"rep-{r}" / "config.json"
            cfg.parent.mkdir(parents=True, exist_ok=True)
            # The config already names out/native; each copy and repeat gets its own folder.
            cfg.write_text(text.replace(json.dumps((out / "native").as_posix())[1:-1], native.as_posix()),
                           encoding="utf-8", newline="\n")
            if read(cfg)["output_directory"] != native.as_posix():
                raise SystemExit("refused: could not place a copy's output folder")
            configs.append({"config": cfg.as_posix(), "native": native.as_posix()})
        plan_lanes.append({"delay": i * stagger, "configs": configs})
    spec = out / "pack.json"
    write(spec, {"binary": plan["binary"]["path"], "wrapped": bool(plan["hosts"][args.host]["wrapper"]),
                 "lanes": plan_lanes})
    inner = [part for part in plan["hosts"][args.host]["wrapper"]] + [
        sys.executable, str(Path(__file__).resolve()), "_lanes", "--spec", str(spec)]
    code = subprocess.call(inner)
    result = read(out / "pack-result.json") if (out / "pack-result.json").exists() else {"lanes": []}
    copies, starts, ends, equal, exits = [], [], [], True, [code]
    ran_lanes = result["lanes"] + [[]] * (len(plan_lanes) - len(result["lanes"]))
    for lane, ran in zip(plan_lanes, ran_lanes):
        updates = []
        for item, run_ in zip(lane["configs"], ran):
            exits.append(run_["exit"])
            rows = list(qual.iterations(item["native"]))
            states = [u["after_state_sha256"] for _, u, _ in rows]
            ok = states == [expected]
            equal = equal and ok
            # Counted only when the run left exactly one completed update, whatever its state.
            updates.append({"wall_seconds": run_["end"] - run_["start"], "exit": run_["exit"],
                            "completed": len(rows) == 1,
                            "collection_wall_seconds": rows[0][2]["collection_elapsed_seconds"] if rows else None,
                            "after_state_sha256": states[0] if states else None, "state_equal": ok})
            starts.append(run_["start"])
            ends.append(run_["end"])
        copies.append({"delay_seconds": lane["delay"], "updates": updates,
                       "chain_wall_seconds": (ran[-1]["end"] - ran[0]["start"]) if ran else None,
                       "state_equal": len(updates) == args.repeats and all(u["state_equal"] for u in updates)})
    done = sum(u["completed"] for c in copies for u in c["updates"])
    complete = done == args.pack * args.repeats
    span = (max(ends) - min(starts)) if complete else None
    return {"schema": "ex1-stagep-throughput/v1", "host": args.host, "hostname": socket.gethostname(),
            "platform": platform.platform(), "binary_sha256": plan["binary"]["sha256"], "cores": cores,
            "workers": WORKERS, "pack": args.pack, "repeats": args.repeats, "stagger_seconds": stagger,
            "exit": 0 if complete and not any(exits) else (code or 1), "copies": copies,
            "group_span_seconds": span, "completed_updates": done,
            "group_updates_per_hour": done * 3600 / span if span else None,
            "wall_seconds": span, "expected_state_sha256": expected,
            "state_equal": complete and equal, "state_required": windows}


def lanes(args):
    """Inside a throughput claim: each lane waits its delay, then runs its configs one after another."""
    import threading
    spec = read(args.spec)
    if spec["wrapped"] and not os.environ.get("HOST_SLOTS_CLAIM"):
        raise SystemExit("refused: this internal command runs only inside its host_slots claim")
    binary = spec["binary"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]   # .py: test stand-ins only
    results = [[] for _ in spec["lanes"]]

    def go(i, lane):
        time.sleep(lane["delay"])
        for item in lane["configs"]:
            start = time.time()
            code = subprocess.call(exe + [item["config"]])
            results[i].append({"start": start, "end": time.time(), "exit": code})

    threads = [threading.Thread(target=go, args=(i, lane)) for i, lane in enumerate(spec["lanes"])]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    write(Path(args.spec).with_name("pack-result.json"), {"lanes": results})
    raise SystemExit(0 if all(r["exit"] == 0 for lane in results for r in lane) else 1)


def require_claim(plan, host):
    """The inner commands run only inside the host's claim when the host has a wrapper."""
    if plan["hosts"][host]["wrapper"] and not os.environ.get("HOST_SLOTS_CLAIM"):
        raise SystemExit("refused: this internal command runs only inside its host_slots claim")


def throughput_config(plan, native):
    """The qualification's update 100 (resume check, run ex1-qual-det-r2) from the plan's r2 inputs."""
    r2 = next(c for c in plan["chains"].values() if c["lineage"] == "r2")
    return qual.build_config(read(r2["base"]["path"]), read(r2["parent"]["path"]), native, THROUGHPUT["run"],
                             THROUGHPUT["namespace"], THROUGHPUT["offset"], 1, WORKERS, THROUGHPUT["coefficient"])


def guard(plan, name, chain):
    """COMPUTE-POLICY item 5: refuse without a matching throughput record for this host."""
    pack = chain.get("pack", 1)
    path = throughput_path(plan, chain["host"], pack)
    if not path.exists():
        raise SystemExit(f"refused: no throughput record for {chain['host']} at pack {pack}; run "
                         f"`stagep.py throughput{'' if pack == 1 else f' --pack {pack}'}` there first")
    t = read(path)
    problems = []
    if t.get("pack", 1) != pack:
        problems.append(f"recorded at pack {t.get('pack', 1)}, not {pack}")
    if t["hostname"] != socket.gethostname():
        problems.append(f"recorded on {t['hostname']}, not {socket.gethostname()}")
    if t["binary_sha256"] != plan["binary"]["sha256"]:
        problems.append("recorded with another binary")
    if t["workers"] != chain["workers"] or t["cores"] < chain["cores"]:
        problems.append("recorded at other workers or fewer cores")
    if t["exit"] != 0 or (t["state_required"] and not t["state_equal"]):
        problems.append("its update failed or its state differs from the qualification's")
    if problems:
        raise SystemExit(f"refused: throughput record for {chain['host']}: {'; '.join(problems)}")
    return t


# ---------------------------------------------------------------- run

def check(plan, name):
    """A chain's pins, throughput record, budget and byte share; returns the bytes it still has to write."""
    chain = plan["chains"][name]
    root = Path(plan["root"])
    for key, what in (("config", "the chain config"), ("parent", "the parent"), ("base", "the base config")):
        check_pin(chain[key], what)
    guard(plan, name, chain)
    if (root / "chains" / name / "TRUNCATED.json").exists():
        raise SystemExit(f"{name} stopped at its budget; Stage P is inconclusive on budget")
    native = chain["native"]
    written = sum(f.stat().st_size for f in Path(native).rglob("*") if f.is_file()) if Path(native).exists() else 0
    if written > plan["byte_budget"]["cap_bytes"] / len(plan["chains"]):
        raise SystemExit(f"refused: {name} has written {written / 1e9:.1f} GB, past its share of the byte cap")
    return (chain["updates"] - completed(native)) * plan["byte_budget"]["bytes_per_update"]


def group_checks(plan, names):
    """Every named chain's checks, the pack group's completeness and the disk reserve; returns the first chain."""
    chain = plan["chains"][names[0]]
    group = chain.get("group")
    members = sorted(n for n, c in plan["chains"].items() if group is not None and c.get("group") == group)
    if group is None and len(names) > 1:
        raise SystemExit("refused: only the chains of one pack group run together")
    if group is not None and sorted(names) != members:
        raise SystemExit(f"refused: pack group {group} runs all its chains together: {members}")
    need = sum(check(plan, name) for name in names)
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
    # One claim for the group: inside it, `_members` checks again and starts every member's binary. Metering,
    # STOP and the outcome stay out here, so they still act while the claim waits in the queue or is suspended.
    results = Path(plan["root"]) / "chains" / f"group-{group}" / "members.json"
    inner = wrapper + [sys.executable, str(Path(__file__).resolve()), "_members", "--plan", str(args.plan),
                       "--results", str(results)]
    for name in names:
        inner += ["--chain", name]
    raise SystemExit(supervise(plan, names, inner, args.fast_poll, results))


def members(args):
    """Inside a pack group's claim: the group's checks again, then every member's binary at once; writes each
    member's exit code to --results."""
    plan = load_plan(args.plan)
    names = list(dict.fromkeys(args.chain))
    chain = group_checks(plan, names)
    if chain.get("group") is None:
        raise SystemExit("refused: _members runs a pack group")
    require_claim(plan, chain["host"])
    binary = plan["binary"]["path"]
    exe = [sys.executable, binary] if binary.endswith(".py") else [binary]   # .py: test stand-ins only
    procs = {name: subprocess.Popen(exe + [plan["chains"][name]["config"]["path"]]) for name in names}
    codes = {name: proc.wait() for name, proc in procs.items()}
    write(args.results, codes)
    raise SystemExit(0 if not any(codes.values()) else 1)


def supervise(plan, names, cmd, fast_poll, results=None):
    """Runs the command for the named chains (one chain, or a pack group's claim), metering each chain's USD and
    watching STOP; returns the exit code (any nonzero member code, whatever its sign, makes it nonzero)."""
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
        for name, chain in chains.items():
            states[name]["metered_seconds"] += now - last
            write(paths[name], states[name])
            spent = states[name]["metered_seconds"] / 3600 * chain["usd_per_hour"]
            if chain["usd_per_hour"] > 0 and spent >= chain["budget_usd"]:
                outcome = "budget"
        last = now
        if outcome is None and (root / "STOP").exists():
            outcome = "stop"
        if outcome:
            stop_tree(proc)
            break
    code = proc.wait()
    member = read(results) if results is not None and Path(results).exists() else {}
    codes = {name: member.get(name, code) for name in names}
    for name, chain in chains.items():
        native = chain["native"]
        states[name]["metered_seconds"] += time.time() - last
        session = sessions[name]
        session.update(ended=time.time(), exit=codes[name], outcome=outcome, completed=completed(native))
        states[name]["sessions"].append(session)
        write(paths[name], states[name])
        spent = states[name]["metered_seconds"] / 3600 * chain["usd_per_hour"]
        log(root, chain=name, event="end", exit=codes[name], outcome=outcome, completed=session["completed"],
            usd=spent)
        if outcome:
            session["moved_after"] = quarantine(native, root, name)
        if outcome == "budget":
            write(root / "chains" / name / "TRUNCATED.json",
                  {"chain": name, "completed_updates": session["completed"], "planned_updates": chain["updates"],
                   "metered_usd": spent, "budget_usd": chain["budget_usd"],
                   "stage_p": "inconclusive on budget (design section 5)"})
        elif not outcome and codes[name] == 0 and Path(native, "completion.json").exists():
            write(root / "chains" / name / "DONE.json",
                  {"chain": name, "completed_updates": session["completed"], "metered_usd": spent,
                   "completion": pin(Path(native, "completion.json"))})
    if outcome == "budget":
        return EXIT_BUDGET
    if outcome == "stop":
        return EXIT_STOP
    return next((c for c in [code, *codes.values()] if c != 0), 0)


def status(args):
    plan = read(args.plan)
    root = Path(plan["root"])
    out = {}
    for name, chain in plan["chains"].items():
        native = Path(chain["native"])
        walls = []
        for complete in sorted(native.glob("iterations/*/complete.json"))[-20:]:
            receipt = read(complete)
            walls.append(read(receipt["collection"]["path"])["collection_elapsed_seconds"])
        state_path = root / "chains" / name / "STATE.json"
        metered = read(state_path)["metered_seconds"] if state_path.exists() else 0.0
        done = completed(native)
        per = sum(walls) / len(walls) if walls else None
        out[name] = {"host": chain["host"], "completed": done, "of": chain["updates"],
                     "metered_usd": round(metered / 3600 * chain["usd_per_hour"], 3),
                     "budget_usd": chain["budget_usd"], "recent_seconds_per_update": per,
                     "projected_end": None if per is None else time.strftime(
                         "%Y-%m-%d %H:%M %Z", time.localtime(time.time() + per * (chain["updates"] - done))),
                     "truncated": (root / "chains" / name / "TRUNCATED.json").exists(),
                     "done": (root / "chains" / name / "DONE.json").exists()}
    print(json.dumps(out, indent=1))
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("plan")
    for flag in ("--qualification", "--binary", "--root", "--out"):
        p.add_argument(flag, required=True)
    for flag in ("--parent", "--base", "--place", "--wrapper", "--reserve"):
        p.add_argument(flag, action="append")
    lz = sub.add_parser("localize")
    lz.add_argument("--plan", required=True)
    lz.add_argument("--host", required=True)
    lz.add_argument("--export", required=True)
    lz.add_argument("--map", action="append", required=True, help="OLD_PREFIX=NEW_PREFIX")
    t = sub.add_parser("throughput")
    t.add_argument("--plan", required=True)
    t.add_argument("--host", required=True)
    t.add_argument("--pack", type=int, default=1, help="K copies sharing one claim of the planned cores")
    t.add_argument("--repeats", type=int, default=None, help="updates per copy (packed only; default 3)")
    t.add_argument("--stagger", type=float, help="seconds between copy starts (default: the K = 1 wall over K)")
    r = sub.add_parser("run")
    r.add_argument("--plan", required=True)
    r.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    r.add_argument("--fast-poll", action="store_true", help=argparse.SUPPRESS)
    m = sub.add_parser("_members")
    m.add_argument("--plan", required=True)
    m.add_argument("--results", required=True)
    m.add_argument("--chain", required=True, action="append", choices=sorted(CHAINS))
    m.add_argument("--fast-poll", action="store_true")
    ln = sub.add_parser("_lanes")
    ln.add_argument("--spec", required=True)
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
        if args.repeats < 1:
            ap.error("--repeats is at least 1")
    {"plan": make_plan, "localize": localize, "throughput": throughput, "run": run, "status": status,
     "_members": members, "_lanes": lanes}[args.cmd](args)


if __name__ == "__main__":
    main()
