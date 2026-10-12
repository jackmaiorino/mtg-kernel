"""Concurrent guarded qualification of the pilot's workload families (run inside a held reservation).

Usage: python qualify.py PLAN.json
PLAN: {"root": ROOT, "runtime": RUNTIME.json, "parent_sources": {"r1": SRC, "r2": SRC}, "a48": SRC,
       "host": "desktop", "families": [{"tag": "r1-T-main", "lineage": "r1", "arm": "T",
       "phase": "main", "cores": "0-3", "workers": "1,2,4"}, ...],
       "pre": [{"tag": "r1-engineering-o0", "lineage": "r1", "kind": "engineering", "own": 0,
       "cores": "8-9", "workers": "1,2"}, ...]}
Each family (or pre-learning run) qualifies its worker ladder serially on its own declared cores; the
families run concurrently on disjoint cores. Every native job enters through the guarded dispatcher
(drive.py). Results go to ROOT/qualification-log.jsonl.
"""
from __future__ import annotations

import json
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PY = [sys.executable, "-B"]
LOCK = threading.Lock()


def run(*args) -> str:
    done = subprocess.run([*PY, *map(str, args)], capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"{args[:3]}: {done.stdout[-1000:]} {done.stderr[-3000:]}")
    return done.stdout


def log(root: Path, entry: dict) -> None:
    with LOCK, (root / "qualification-log.jsonl").open("a") as f:
        f.write(json.dumps({"at": time.time(), **entry}) + "\n")


SLOTS = None


def job(plan: dict, item: dict, pre: bool) -> None:
    """With PLAN["slots"] (core strings), at most len(slots) jobs run at once, each on a free slot."""
    if SLOTS is None:
        return run_job(plan, item, pre)
    cores = SLOTS.get()
    try:
        return run_job(plan, {**item, "cores": cores}, pre)
    finally:
        SLOTS.put(cores)


def run_job(plan: dict, item: dict, pre: bool) -> None:
    root = Path(plan["root"])
    tag = item["tag"]
    base = root / ("pre" if pre else "train") / tag
    config = base / "config-qual.json"
    config.parent.mkdir(parents=True, exist_ok=True)
    lineage = item["lineage"]
    common = ["--parent", plan["parent_sources"][lineage], "--a48", plan["a48"], "--workers", 1,
              "--output-directory", str(base / "native").replace("\\", "/"), "--out", config]
    try:
        if pre:
            run(HERE / "schedule.py", "pre-config", "--lineage", lineage, "--kind", item["kind"], "--own",
                item["own"], *common)
        else:
            run(HERE / "schedule.py", "config", "--lineage", lineage, "--arm", item["arm"], "--phase",
                item["phase"], *common)
        paths = json.loads(run(HERE / "drive.py", "requests", "--tag", tag, "--config", config, "--runtime",
                               plan["runtime"], "--root", base, "--workers", item["workers"], "--cores",
                               item["cores"], "--host", plan["host"],
                               *(["--wall", plan["wall"]] if plan.get("wall") else [])))
        log(root, {"tag": tag, "event": "requests", "paths": paths, "cores": item["cores"]})
        for path in paths:
            t0 = time.time()
            out = run(HERE / "drive.py", "wait", path)
            log(root, {"tag": tag, "event": "qualified", "request": path, "result": json.loads(out),
                       "wall_seconds": time.time() - t0})
    except Exception as error:  # recorded, never retried silently
        log(root, {"tag": tag, "event": "failed", "error": str(error)[-4000:]})


def main() -> None:
    global SLOTS
    plan = json.loads(Path(sys.argv[1]).read_text())
    if plan.get("slots"):
        import queue
        SLOTS = queue.Queue()
        for cores in plan["slots"]:
            SLOTS.put(cores)
    threads = [threading.Thread(target=job, args=(plan, item, False)) for item in plan.get("families", [])]
    threads += [threading.Thread(target=job, args=(plan, item, True)) for item in plan.get("pre", [])]
    for t in threads:
        t.start()
        time.sleep(2)
    for t in threads:
        t.join()
    log(Path(plan["root"]), {"event": "all-done"})


if __name__ == "__main__":
    main()
