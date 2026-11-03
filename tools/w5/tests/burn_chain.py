"""A test chain for the W5 launcher acceptance: CPU-burning workers in child processes, an optional pause
handshake at an "update boundary", and per-process CPU reports.

  burn_chain.py chain --out DIR --seconds S --workers W [--pause]   two batches of W workers, pause between
  burn_chain.py worker --out DIR --seconds S                        burn S CPU-seconds, report own CPU
  burn_chain.py filler --workers W                                  burn until killed
  burn_chain.py alloc --mib M                                       touch M MiB (an OOM test)
"""
import json
import os
import resource
import sys
import time
from pathlib import Path


def cpu_self() -> float:
    ru = resource.getrusage(resource.RUSAGE_SELF)
    return ru.ru_utime + ru.ru_stime


def burn(seconds: float) -> None:
    x = 0
    while cpu_self() < seconds:
        for i in range(20000):
            x = (x * 31 + i) & 0xFFFFFFFF


def report(out: Path, role: str, extra=None) -> None:
    ru = resource.getrusage(resource.RUSAGE_SELF)
    record = dict(extra or {}, pid=os.getpid(), role=role, usec=round((ru.ru_utime + ru.ru_stime) * 1e6))
    (out / f"proc-{role}-{os.getpid()}.json").write_text(json.dumps(record))


def spawn_batch(out: Path, seconds: float, workers: int) -> list[int]:
    pids = []
    for _ in range(workers):
        pid = os.fork()
        if pid == 0:
            os.execv(sys.executable, [sys.executable, __file__, "worker", "--out", str(out), "--seconds", str(seconds)])
        pids.append(pid)
    reaped = []
    for pid in pids:
        _, status, ru = os.wait4(pid, 0)
        assert os.waitstatus_to_exitcode(status) == 0, status
        reaped.append(round((ru.ru_utime + ru.ru_stime) * 1e6))
    return reaped


def main(argv) -> int:
    mode = argv[0]
    opts = {argv[i].lstrip("-"): argv[i + 1] if i + 1 < len(argv) and not argv[i + 1].startswith("--") else True
            for i in range(1, len(argv)) if argv[i].startswith("--")}
    if mode == "worker":
        burn(float(opts["seconds"]))
        report(Path(opts["out"]), "worker")
        return 0
    if mode == "filler":
        for _ in range(int(opts.get("workers", 2)) - 1):
            if os.fork() == 0:
                break
        while True:
            burn(cpu_self() + 3600)
    if mode == "alloc":
        block = bytearray(int(opts["mib"]) << 20)
        for i in range(0, len(block), 4096):
            block[i] = 1
        time.sleep(1)
        return 0
    out = Path(opts["out"])
    out.mkdir(parents=True, exist_ok=True)
    seconds, workers = float(opts["seconds"]), int(opts["workers"])
    box = os.environ.get("W5_BOX_CPUS", "")
    affinity = sorted(os.sched_getaffinity(0))
    reaped = spawn_batch(out, seconds, workers)
    if opts.get("pause"):
        control = Path(os.environ["W5_CONTROL_DIR"])
        tmp = control / "pause.request.tmp"
        ledger = int(os.environ["W5_LEDGER_BASE_USEC"]) + int(
            dict(line.split() for line in Path(os.environ["W5_LEDGER_CGROUP"], "cpu.stat").read_text().splitlines())
            ["usage_usec"])
        instance = int(os.environ["W5_LEDGER_INSTANCE"])
        tmp.write_text(json.dumps({"instance": instance, "seq": 0, "update": 1, "ledger_usec": ledger}))
        os.replace(tmp, control / "pause.request")
        while not (control / f"resume.{instance}.0.json").exists():
            time.sleep(0.05)
    reaped += spawn_batch(out, seconds, workers)
    report(out, "root", {"affinity": affinity, "box_cpus": box, "reaped_children_usec": reaped,
                         "instance": os.environ.get("W5_LEDGER_INSTANCE")})
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
