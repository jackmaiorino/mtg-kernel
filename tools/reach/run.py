"""Guarded RS0 driver (collab LANES/reverse-search-20261029/RS0-PLAN.md section 9). Measurement only.

usage:
  run.py qualify --binary EXE --source POLICY.json --roots ROOTS.jsonl --out DIR [--workers 1,2,4,8]
  run.py measure --binary EXE --source POLICY.json --roots ROOTS.jsonl --out DIR --workers N

`qualify` runs section 9's records-on identity check (the first three Spy k1
roots, 64,000, seed 0, plain against traced) and a serial-versus-parallel
throughput comparison on identical work (the first Spy k1 roots at 64,000,
seed 0, one `s4a-reach` process per worker), and writes
DIR/qualification.json with the binary, policy and roots hashes.

`measure` refuses to start unless DIR/qualification.json exists, records an
identity pass, and matches the binary, policy and roots hashes. It then runs
the frozen matrix (64,000 and 256,000 at seeds 0, 1 and 2 for every root;
1,024,000 at seed 0 for the Spy k2 roots, last) as small shards through a
pool of N one-core host_slots claims, each declaring --max-seconds from the
qualified rate. host_slots never preempts, so the cap (40 claimed core-hours
by default, every running claim charged one core for its wall time) is
enforced here: no shard starts if it would be projected past the cap, and
running claims end once it is reached. DIR/manifest.json records the plan
and every shard's exit. Every process writes its own rows file; the readout
is tools/reach/report.py.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

HOST_SLOTS = "D:/mtg-ci/hooks/host_slots_v1.py"
LANE = "reverse-search"
BUDGETS = "64000,256000"
SEEDS = "0,1,2"
LARGE = "1024000"
# EX1's planning estimate (251 to 452 us per transition), upper end, for
# --max-seconds declarations only.
SECONDS_PER_TRANSITION = 452e-6


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def hashes(a) -> dict:
    return {"binary": sha256(a.binary), "source": sha256(a.source), "roots": sha256(a.roots)}


def claim(work_id: str, max_seconds: int, cmd: list[str], env: dict, log: Path) -> subprocess.Popen:
    """One process under its own one-core host_slots claim (backfill places it)."""
    full = [sys.executable, "-B", HOST_SLOTS, "run", "--lane", LANE, "--work-id", work_id, "--cores", "1",
            "--priority", "below_normal", "--max-seconds", str(max_seconds), "--wait", "--"] + cmd
    return subprocess.Popen(full, env=env, stdout=log.open("w"), stderr=subprocess.STDOUT)


def census(a, out: Path, extra: dict, roots: Path | None = None) -> tuple[list[str], dict]:
    env = dict(os.environ)
    for k in list(env):
        if k.startswith("REACH_"):
            del env[k]
    env.update({"REACH_ROOTS": str(roots or a.roots), **extra})
    return [str(a.binary), "mode=s4a-reach", f"out={out}", f"source={a.source}"], env


def rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(x) for x in path.read_text(encoding="utf-8").splitlines() if x.strip()]


def run_all(procs: list[subprocess.Popen]) -> list[int]:
    return [p.wait() for p in procs]


def qualify(a) -> int:
    a.out.mkdir(parents=True, exist_ok=True)
    started = time.time()
    # 1. Records-on identity: plain against traced on the first three Spy k1 roots.
    ident = a.out / "identity.jsonl"
    cmd, env = census(a, ident, {"REACH_IDENTITY": "1", "REACH_ROLES": "spy:k1", "REACH_LIMIT": "3",
                                 "REACH_BUDGETS": "64000", "REACH_SEEDS": "0"})
    code = run_all([claim("rs0-qual-identity", 3 * 2 * 120, cmd, env, a.out / "identity.log")])[0]
    idrows = [r for r in rows(ident) if r["kind"] == "identity"]
    identity_pass = code == 0 and len(idrows) == 3 and all(r["identical"] for r in idrows)
    # 2. Throughput: identical work (the first max(workers) Spy k1 roots, 64,000,
    # seed 0), as one process per worker; completed searches per wall second.
    widths = [int(x) for x in a.workers.split(",")]
    work = max(widths)
    # The identical work set: the first `work` roots carrying the Spy k1 role.
    subset = a.out / "throughput-roots.jsonl"
    lines = [x for x in a.roots.read_text(encoding="utf-8").splitlines()
             if x.strip() and any(r["stratum"] == "spy" and r["role"] == "k1" for r in json.loads(x)["roles"])][:work]
    subset.write_text("".join(x + "\n" for x in lines), encoding="utf-8")
    scaling = []
    for n in widths:
        procs, outs = [], []
        t0 = time.time()
        for i in range(n):
            out = a.out / f"throughput-w{n}-{i}.jsonl"
            # Shard i of n over the same work set.
            cmd, env = census(a, out, {"REACH_BUDGETS": "64000", "REACH_SEEDS": "0", "REACH_SHARD": f"{i}/{n}"},
                              roots=subset)
            procs.append(claim(f"rs0-qual-w{n}-{i}", int(work / n * 64000 * SECONDS_PER_TRANSITION * 3) + 120,
                               cmd, env, a.out / f"throughput-w{n}-{i}.log"))
            outs.append(out)
        codes = run_all(procs)
        wall = time.time() - t0
        done = sum(1 for o in outs for r in rows(o) if r["kind"] == "search")
        cpu = sum(r.get("cpu_seconds", 0.0) for o in outs for r in rows(o) if r["kind"] == "search")
        scaling.append({"workers": n, "exit_codes": codes, "searches": done, "wall": wall,
                        "searches_per_second": done / wall if wall else None, "search_cpu_seconds": cpu})
    record = {"schema": "rs0-qualification-v1", "plan": "rs0-plan-v4", "hashes": hashes(a),
              "identity": {"pass": identity_pass, "exit_code": code, "rows": idrows},
              "throughput": {"work": f"the first {work} Spy k1 roots, 64,000, seed 0, sharded over the workers",
                             "scaling": scaling},
              "host": os.environ.get("COMPUTERNAME"), "started": started, "wall": time.time() - started}
    (a.out / "qualification.json").write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    print(json.dumps({"identity_pass": identity_pass,
                      "scaling": [{k: s[k] for k in ("workers", "searches", "wall", "searches_per_second")} for s in scaling]}))
    return 0 if identity_pass else 1


def cpu_rate(q: dict) -> float:
    """Claimed core-seconds per transition: the slowest qualified width (hyperthread siblings share a core)."""
    return max(x["wall"] * x["workers"] / (x["searches"] * 64000) for x in q["throughput"]["scaling"] if x["searches"])


def measure(a) -> int:
    qpath = a.out / "qualification.json"
    if not qpath.exists():
        raise SystemExit("refused: no qualification.json (run `qualify` first)")
    q = json.loads(qpath.read_text(encoding="utf-8"))
    if not q["identity"]["pass"]:
        raise SystemExit("refused: the records-on identity check did not pass")
    if q["hashes"] != hashes(a):
        raise SystemExit(f"refused: qualification hashes differ: {q['hashes']} vs {hashes(a)}")
    if not q["throughput"]["scaling"]:
        raise SystemExit("refused: no throughput record")
    n, cap = a.workers, a.cap_cpu_hours * 3600
    rate = cpu_rate(q)
    lines = [x for x in a.roots.read_text(encoding="utf-8").splitlines() if x.strip()]
    k2 = sum(1 for x in lines if any(r["stratum"] == "spy" and r["role"] == "k2" for r in json.loads(x)["roles"]))
    # The frozen matrix as small shards (main first: the flags read 64,000 and
    # 256,000 only; the Spy k2 1,024,000 cells are diagnostic), run through a
    # pool of n one-core claims.
    shards = []
    main_n, large_n = a.shards or 4 * n, n
    for i in range(main_n):
        expect = len(lines) / main_n * 3 * 320000 * rate
        shards.append({"name": f"main-{i}", "expect": expect, "extra": {
            "REACH_BUDGETS": BUDGETS, "REACH_SEEDS": SEEDS, "REACH_SHARD": f"{i}/{main_n}"}})
    for i in range(large_n):
        expect = max(1, k2 / large_n) * int(LARGE) * rate
        shards.append({"name": f"large-{i}", "expect": expect, "extra": {
            "REACH_BUDGETS": LARGE, "REACH_SEEDS": "0", "REACH_ROLES": "spy:k2", "REACH_SHARD": f"{i}/{large_n}"}})
    for sh in shards:
        sh["max_seconds"] = int(2 * sh["expect"]) + 600
    manifest = {"schema": "rs0-run-v2", "plan": "rs0-plan-v4", "hashes": hashes(a), "workers": n,
                "qualification": sha256(qpath), "core_seconds_per_transition": rate,
                "cap_core_seconds": cap, "projected_core_seconds": sum(sh["expect"] for sh in shards),
                "shards": [{k: sh[k] for k in ("name", "expect", "max_seconds", "extra")} for sh in shards],
                "host": os.environ.get("COMPUTERNAME"), "started": time.time()}
    mpath = a.out / "manifest.json"
    mpath.write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    # Cap accounting: every running shard is charged one core for its whole
    # wall time (conservative against its process CPU time).
    used, running, pending, results, capped = 0.0, {}, list(shards), [], False
    while pending or running:
        while pending and len(running) < n and not capped:
            sh = pending[0]
            # Charged so far plus what the running shards are still expected to take.
            committed = used + sum(max(time.time() - t, sh2["expect"]) for sh2, t in running.values())
            if committed + sh["expect"] > cap:
                capped = True
                break
            pending.pop(0)
            cmd, env = census(a, a.out / f"{sh['name']}.jsonl", sh["extra"])
            proc = claim(f"rs0-{sh['name']}", sh["max_seconds"], cmd, env, a.out / f"{sh['name']}.log")
            running[proc] = (sh, time.time())
        if capped and not running:
            break
        time.sleep(5)
        live = used + sum(time.time() - t for _, t in running.values())
        for proc, (sh, t) in list(running.items()):
            code = proc.poll()
            if code is None and live > cap:
                proc.terminate()
                code = proc.wait()
                capped = True
            if code is not None:
                used += time.time() - t
                results.append({"name": sh["name"], "exit_code": code, "core_seconds": time.time() - t})
                del running[proc]
    manifest.update({"finished": time.time(), "core_seconds": used, "capped": capped, "results": results,
                     "not_started": [sh["name"] for sh in pending]})
    mpath.write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    return 0 if not capped and all(r["exit_code"] == 0 for r in results) else 1


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["qualify", "measure"])
    ap.add_argument("--binary", type=Path, required=True)
    ap.add_argument("--source", type=Path, required=True)
    ap.add_argument("--roots", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--workers", default="1,2,4,8", help="qualify: comma list; measure: one integer")
    ap.add_argument("--shards", type=int, default=0, help="measure: main-matrix shards (default 4 x workers)")
    ap.add_argument("--cap-cpu-hours", type=float, default=40.0, help="measure: claimed core-hours cap")
    a = ap.parse_args(argv)
    if a.command == "qualify":
        return qualify(a)
    a.workers = int(a.workers)
    return measure(a)


if __name__ == "__main__":
    raise SystemExit(main())
