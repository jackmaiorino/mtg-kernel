"""Run the decision-equivalence census (`s4a-equiv`) as parallel game ranges.

Usage: python equiv_queue.py ROOT BINARY SOURCES FIRST GAMES SHARDS CPUS [BUDGET_CPU_SECONDS]
Splits games FIRST..FIRST+GAMES into SHARDS contiguous ranges, one process
each (workers=1), pinned to the comma-separated logical CPUs, BelowNormal
with EcoQoS off. Kills every live shard if summed CPU exceeds the budget.
Writes ROOT/out/equiv-<first>-<n>.jsonl, a CPU ledger and a log.
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

ROOT, BIN, SRC = Path(sys.argv[1]), sys.argv[2], sys.argv[3].rstrip("/")
FIRST, GAMES, SHARDS = int(sys.argv[4]), int(sys.argv[5]), int(sys.argv[6])
CPUS = [int(x) for x in sys.argv[7].split(",")]
BUDGET = float(sys.argv[8]) if len(sys.argv) > 8 else 7200.0
OUT = ROOT / "out"
OUT.mkdir(parents=True, exist_ok=True)
K32 = ctypes.WinDLL("kernel32", use_last_error=True)
K32.OpenProcess.restype = ctypes.c_void_p
K32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
K32.SetProcessInformation.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
K32.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(ctypes.c_ulonglong)] * 4
K32.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]


def log(text):
    with (OUT / "equiv-queue.log").open("a", encoding="utf-8") as s:
        s.write(f"{datetime.now(timezone.utc).isoformat()} {text}\n")


def cpu(h):
    t = [ctypes.c_ulonglong() for _ in range(4)]
    return (t[2].value + t[3].value) / 1e7 if K32.GetProcessTimes(h, *[ctypes.byref(x) for x in t]) else 0.0


class Eco(ctypes.Structure):
    _fields_ = [("Version", ctypes.c_ulong), ("ControlMask", ctypes.c_ulong), ("StateMask", ctypes.c_ulong)]


def main():
    bsha = hashlib.sha256(Path(BIN).read_bytes()).hexdigest()
    env = {k: v for k, v in os.environ.items() if k not in ("S4A_LIMITS", "ROOTS", "S4A_TRACE")}
    env.update(OPPONENTS=f"T1={SRC}/t1-source.json,A48={SRC}/a48-source.json", S4A_MODEL="r1",
               CUDA_VISIBLE_DEVICES="")
    per = -(-GAMES // SHARDS)
    runs = []
    for i in range(SHARDS):
        first, n = FIRST + i * per, min(per, FIRST + GAMES - (FIRST + i * per))
        if n <= 0:
            break
        out = OUT / f"equiv-{first}-{n}.jsonl"
        cmd = [BIN, f"source={SRC}/r1-source.json", f"out={out.as_posix()}", "mode=s4a-equiv", "workers=1",
               "base_seed=2026100941", "decks=0,1,2,3,4,5,6,7,8", f"first_game={first}", f"games={n}"]
        stream = (OUT / f"equiv-{first}-{n}.log").open("a")
        p = subprocess.Popen(cmd, env=env, stdout=stream, stderr=subprocess.STDOUT,
                             creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        h = K32.OpenProcess(0x1000 | 0x0200, False, p.pid)
        e = Eco(1, 1, 0)
        ok = K32.SetProcessInformation(h, 4, ctypes.byref(e), ctypes.sizeof(e))
        K32.SetProcessAffinityMask(h, 1 << CPUS[i % len(CPUS)])
        log(f"shard {first}+{n} pid {p.pid} cpu {CPUS[i % len(CPUS)]} ecoqos off {bool(ok)} binary {bsha}")
        runs.append({"p": p, "h": h, "out": out, "first": first, "n": n, "stream": stream, "t": time.monotonic()})
    killed = False
    while any(r["p"].poll() is None for r in runs):
        if sum(cpu(r["h"]) for r in runs) > BUDGET:
            for r in runs:
                if r["p"].poll() is None:
                    r["p"].kill()
            killed = True
            log(f"cpu budget {BUDGET} reached; live shards killed")
        time.sleep(10)
    total = 0.0
    with (OUT / "EQUIV-CPU-LEDGER.jsonl").open("a") as f:
        for r in runs:
            r["stream"].close()
            c = cpu(r["h"])
            total += c
            rec = {"shard": f"{r['first']}+{r['n']}", "exit": r["p"].returncode, "killed": killed, "cpu_seconds": c,
                   "wall_seconds": round(time.monotonic() - r["t"], 1), "binary_sha256": bsha,
                   "sha256": hashlib.sha256(r["out"].read_bytes()).hexdigest() if r["out"].exists() else None}
            f.write(json.dumps(rec) + "\n")
    log(f"done: {len(runs)} shards, cpu {round(total, 1)} s, killed {killed}")


if __name__ == "__main__":
    main()
