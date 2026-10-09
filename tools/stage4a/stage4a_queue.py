"""Stage 4a job queue for one host. Runs inside the host reservation under
`host_slots_v1.py timed --cores <declared>` (see launch_stage4a.py).

Usage: python stage4a_queue.py ROOT BINARY QUEUE.json

QUEUE.json: [{"name", "mode": "s4a-corpus"|"s4a-run", "model": "r1"|"r2",
  "workers", "base_seed", and for corpus "first_game", "games"; for run
  "roots" and optionally "limits" ("select_cap,eval_worlds,eval_cap", cost-only
  engineering checks)}]. Jobs run in order. Each process runs at BelowNormal
priority with Windows power throttling (EcoQoS) switched off, so it uses the
declared P-cores; priority is unchanged. An `s4a-run` job resumes: the binary
skips roots already present in its output. Only exit codes, row counts by
kind, sha256 and wall time are logged; no outcome field is read.
"""
import ctypes
import hashlib
import json
import os
import subprocess
import sys
import time
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

ROOT, BIN, QUEUE = Path(sys.argv[1]), sys.argv[2], json.loads(Path(sys.argv[3]).read_text())
SRC = (ROOT / "sources").as_posix()
OUT = ROOT / "out"
OUT.mkdir(exist_ok=True)
LOG = OUT / "queue.log"
DECKS = "0,1,2,3,4,5,6,7,8"


def log(text):
    with LOG.open("a", encoding="utf-8") as s:
        s.write(f"{datetime.now(timezone.utc).isoformat()} {text}\n")


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def no_ecoqos(pid):
    if os.name != "nt":
        return None

    class State(ctypes.Structure):
        _fields_ = [("Version", ctypes.c_ulong), ("ControlMask", ctypes.c_ulong), ("StateMask", ctypes.c_ulong)]
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    h = k32.OpenProcess(0x0200, False, pid)
    s = State(1, 1, 0)
    ok = k32.SetProcessInformation(h, 4, ctypes.byref(s), ctypes.sizeof(s))
    k32.CloseHandle(h)
    return bool(ok)


def run_job(item):
    out = OUT / f"{item['name']}.jsonl"
    env = {k: v for k, v in os.environ.items() if k not in ("S4A_LIMITS", "ROOTS")}
    env.update(OPPONENTS=f"T1={SRC}/t1-source.json,A48={SRC}/a48-source.json", S4A_MODEL=item["model"],
               CUDA_VISIBLE_DEVICES="")
    command = [BIN, f"source={SRC}/{item['model']}-source.json", f"out={out.as_posix()}", f"mode={item['mode']}",
               f"workers={item['workers']}", f"base_seed={item['base_seed']}", f"decks={DECKS}"]
    if item["mode"] == "s4a-corpus":
        command += [f"first_game={item['first_game']}", f"games={item['games']}"]
    else:
        env["ROOTS"] = item["roots"]
        if item.get("limits"):
            env["S4A_LIMITS"] = item["limits"]
    started = time.monotonic()
    log(f"{item['name']} start: {json.dumps(item)}; binary sha {sha(BIN)}")
    with (OUT / f"{item['name']}.log").open("a") as stream:
        flags = subprocess.BELOW_NORMAL_PRIORITY_CLASS if os.name == "nt" else 0
        p = subprocess.Popen(command, env=env, stdout=stream, stderr=subprocess.STDOUT, creationflags=flags)
        log(f"{item['name']} pid {p.pid} ecoqos off {no_ecoqos(p.pid)}")
        code = p.wait()
    kinds = Counter()
    if out.exists():
        for line in out.read_text(encoding="utf-8").splitlines():
            if line.strip():
                kinds[json.loads(line).get("kind")] += 1
    record = {"job": item["name"], "exit": code, "rows_by_kind": dict(kinds), "error_rows": kinds.get("error", 0),
              "sha256": sha(out) if out.exists() else None, "wall_seconds": round(time.monotonic() - started, 1)}
    log(f"{item['name']} end: {json.dumps(record)}")
    return record


def main():
    log(f"queue start: binary {BIN} sha {sha(BIN)}, {len(QUEUE)} jobs")
    results = []
    for item in QUEUE:
        record = run_job(item)
        results.append(record)
        (OUT / "QUEUE-RESULTS.json").write_text(json.dumps(results, indent=1))
        if record["exit"] != 0:
            log(f"{item['name']} exited {record['exit']}; queue stopped")
            break
    log("queue done")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        log(f"ERROR {type(error).__name__}: {error}")
        raise
