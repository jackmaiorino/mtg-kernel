"""Spy execution diagnosis job queue for one host (collab
LANES/spy-execution-diagnosis-plan-20261010). Runs inside the host
reservation under `host_slots_v1.py timed --cores <declared>` (launch_diag.py).

Usage: python diag_queue.py ROOT BINARY QUEUE.json

QUEUE.json: {"budget_cpu_seconds": B, "phase": "qualification"|"panel",
  "groups": [[job, ...], ...]}, job = {"name", "model": "r1"|"r2",
  "roots": path, "trace": bool, "cpu": logical CPU index or null}.
Groups run in order; the jobs of a group run together, one process per job,
each pinned to its own logical CPU (one per physical core). Mode s4a-diag
(E only), one worker per process.

Guards (the plan's resource limits, not advice):
- the binary must hash to ROOT/BINARY.json's pin;
- a panel queue needs ROOT/THROUGHPUT.json for this binary with
  "qualified": true (parity and scaling evidence from the qualification);
- CPU: user+kernel seconds of every child, read from its own process handle
  (exact after exit, polled every 10 s while live), are appended to
  ROOT/out/CPU-LEDGER.jsonl; the queue refuses to start, and kills its live
  jobs, once the ledger total for its phase plus live CPU would exceed
  budget_cpu_seconds. Killed and failed attempts stay in the ledger.
Each process runs at BelowNormal priority with EcoQoS off. Only exit codes,
row counts, hashes and CPU are logged; no outcome field is read.
"""
import ctypes
import hashlib
import json
import os
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT, BIN = Path(sys.argv[1]), sys.argv[2]
QUEUE = json.loads(Path(sys.argv[3]).read_text())
SRC = (ROOT / "sources").as_posix()
OUT = ROOT / "out"
OUT.mkdir(exist_ok=True)
TRACES = ROOT / "traces"
TRACES.mkdir(exist_ok=True)
LOG = OUT / "queue.log"
LEDGER = OUT / "CPU-LEDGER.jsonl"
DECKS = "0,1,2,3,4,5,6,7,8"
BASE_SEED = {"r1": 2026100941, "r2": 2026100942}


def log(text):
    with LOG.open("a", encoding="utf-8") as s:
        s.write(f"{datetime.now(timezone.utc).isoformat()} {text}\n")


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


K32 = ctypes.WinDLL("kernel32", use_last_error=True) if os.name == "nt" else None
if K32:
    K32.OpenProcess.restype = ctypes.c_void_p
    K32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
    K32.SetProcessInformation.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
    K32.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(ctypes.c_ulonglong)] * 4
    K32.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    K32.CloseHandle.argtypes = [ctypes.c_void_p]


def open_handle(pid):
    # PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION; the handle
    # keeps the process object, so its CPU times stay readable after exit.
    return K32.OpenProcess(0x1000 | 0x0200, False, pid)


def no_ecoqos(h):
    class State(ctypes.Structure):
        _fields_ = [("Version", ctypes.c_ulong), ("ControlMask", ctypes.c_ulong), ("StateMask", ctypes.c_ulong)]
    s = State(1, 1, 0)
    return bool(K32.SetProcessInformation(h, 4, ctypes.byref(s), ctypes.sizeof(s)))


def cpu_seconds(h):
    t = [ctypes.c_ulonglong() for _ in range(4)]
    ok = K32.GetProcessTimes(h, *[ctypes.byref(x) for x in t])
    return (t[2].value + t[3].value) / 1e7 if ok else None


def ledger_total(phase):
    if not LEDGER.exists():
        return 0.0
    return sum(json.loads(l)["cpu_seconds"] for l in LEDGER.read_text().splitlines()
               if l.strip() and json.loads(l)["phase"] == phase)


def start(job):
    out = OUT / f"{job['name']}.jsonl"
    env = {k: v for k, v in os.environ.items() if k not in ("S4A_LIMITS", "ROOTS", "S4A_TRACE")}
    env.update(OPPONENTS=f"T1={SRC}/t1-source.json,A48={SRC}/a48-source.json", S4A_MODEL=job["model"],
               CUDA_VISIBLE_DEVICES="", ROOTS=job["roots"])
    if job["trace"]:
        env["S4A_TRACE"] = TRACES.as_posix()
    command = [BIN, f"source={SRC}/{job['model']}-source.json", f"out={out.as_posix()}", "mode=s4a-diag",
               "workers=1", f"base_seed={BASE_SEED[job['model']]}", f"decks={DECKS}"]
    stream = (OUT / f"{job['name']}.log").open("a")
    p = subprocess.Popen(command, env=env, stdout=stream, stderr=subprocess.STDOUT,
                         creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS)
    h = open_handle(p.pid)
    eco = no_ecoqos(h) if h else False
    # cpu None: the enclosing host_slots claim already pins the whole tree.
    pinned = job.get("cpu") is None or (bool(h) and bool(K32.SetProcessAffinityMask(h, 1 << job["cpu"])))
    log(f"{job['name']} start pid {p.pid} cpu {job['cpu']} ecoqos off {eco} pinned {pinned}: {json.dumps(job)}")
    if not (eco and pinned):
        p.kill()
        p.wait()
        log(f"{job['name']} killed: could not switch EcoQoS off or pin")
    return {"job": job, "p": p, "h": h, "out": out, "stream": stream, "started": time.monotonic()}


def finish(run, phase, killed):
    code = run["p"].wait()
    cpu = cpu_seconds(run["h"]) if run["h"] else None
    if run["h"]:
        K32.CloseHandle(run["h"])
    run["stream"].close()
    kinds = {}
    if run["out"].exists():
        for line in run["out"].read_text(encoding="utf-8").splitlines():
            if line.strip():
                k = json.loads(line).get("kind")
                kinds[k] = kinds.get(k, 0) + 1
    rec = {"utc": datetime.now(timezone.utc).isoformat(), "phase": phase, "job": run["job"]["name"],
           "exit": code, "killed_at_cap": killed, "cpu_seconds": cpu or 0.0,
           "wall_seconds": round(time.monotonic() - run["started"], 1), "rows_by_kind": kinds,
           "sha256": sha(run["out"]) if run["out"].exists() else None, "binary_sha256": BIN_SHA}
    with LEDGER.open("a") as f:
        f.write(json.dumps(rec) + "\n")
    log(f"{run['job']['name']} end: {json.dumps(rec)}")
    return rec


def main():
    phase, budget = QUEUE["phase"], float(QUEUE["budget_cpu_seconds"])
    pin = json.loads((ROOT / "BINARY.json").read_text())
    if BIN_SHA != pin["sha256"]:
        raise SystemExit(f"binary {BIN_SHA} differs from pin {pin['sha256']}")
    if phase == "panel":
        tp = json.loads((ROOT / "THROUGHPUT.json").read_text()) if (ROOT / "THROUGHPUT.json").exists() else {}
        if tp.get("binary_sha256") != BIN_SHA or tp.get("qualified") is not True:
            raise SystemExit("panel refused: no qualified throughput/parity evidence for this binary")
    log(f"queue start: phase {phase}, budget {budget} cpu-s, ledger {ledger_total(phase)}, binary {BIN} {BIN_SHA}")
    results = []
    for group in QUEUE["groups"]:
        if ledger_total(phase) >= budget:
            log("budget exhausted before group; queue stopped")
            break
        runs = [start(j) for j in group]
        done, killed = {}, False
        while len(done) < len(runs):
            live = 0.0
            for i, r in enumerate(runs):
                if i in done:
                    continue
                if r["p"].poll() is not None:
                    done[i] = finish(r, phase, False)
                else:
                    live += cpu_seconds(r["h"]) or 0.0
            if len(done) < len(runs) and ledger_total(phase) + live > budget:
                killed = True
                for i, r in enumerate(runs):
                    if i not in done:
                        r["p"].kill()
                        done[i] = finish(r, phase, True)
                log(f"phase {phase} cpu budget {budget} reached; live jobs killed, incomplete")
            time.sleep(10)
        results.extend(done.values())
        (OUT / f"QUEUE-RESULTS-{phase}.json").write_text(json.dumps(results, indent=1))
        if killed or any(r["exit"] != 0 for r in done.values()):
            log("a job was killed or exited non-zero; queue stopped")
            break
    log(f"queue done: ledger {phase} total {ledger_total(phase)} cpu-s")


BIN_SHA = sha(BIN)

if __name__ == "__main__":
    try:
        main()
    except BaseException as error:
        log(f"ERROR {type(error).__name__}: {error}")
        raise
