"""Keep a hosted Linux CI job from starving its runner of memory, and record what the job used.

A hosted runner whose job uses up memory and swap stops answering GitHub. Ten minutes later the job is
failed with "The hosted runner lost communication with the server" and has no log, so the cause is not
visible (2026-11-15 and 16: workspace-targets and trace at 48 min, the shared Limited build at 50 and 51
min, eternal at 65 min; always a whole minute after the job started, because the runner renews its job
lease every 60 s and the lease lasts 10 min).

`watch` runs beside the job. Every few seconds it reads /proc/meminfo; when available memory and free swap
are both under their floors it kills the process holding the most memory, so the running step fails at
once and the job keeps its log. Once a minute it records memory, swap, memory pressure and the largest
processes. `report` prints that record and its peaks at the end of the job.

Standard library only: the hosted image's own python3 runs it before any toolchain is set up.
"""
from __future__ import annotations

import argparse
import ctypes
import os
import signal
import sys
import time
from pathlib import Path

MIB = 1024  # /proc reports kB
# Never chosen as the process to kill: the runner itself and the services it needs to report the job.
PROTECTED_NAMES = frozenset({
    "Runner.Listener", "Runner.Worker", "provjobd", "hosted-compute-", "systemd", "systemd-journal",
    "sshd", "dockerd", "containerd", "dbus-daemon", "sudo",
})
# A smaller process is not what exhausted the machine; killing it would not help.
MIN_VICTIM_KB = 1024 * 1024
KILLED = "KILLED"


def parse_meminfo(text: str) -> dict[str, int]:
    """/proc/meminfo as {field: kB}."""
    fields = {}
    for line in text.splitlines():
        name, _, rest = line.partition(":")
        value = rest.split()
        if value and value[0].isdigit():
            fields[name.strip()] = int(value[0])
    return fields


def parse_pressure(text: str) -> float:
    """The 10 s average of /proc/pressure/memory's `full` line (percent of time every task was stalled)."""
    for line in text.splitlines():
        if line.startswith("full "):
            for field in line.split()[1:]:
                key, _, value = field.partition("=")
                if key == "avg10":
                    return float(value)
    return 0.0


def starved(meminfo: dict[str, int], memory_floor: float, swap_floor: float) -> bool:
    """Available memory and free swap are both under their floors (percent of their totals)."""
    if meminfo.get("MemAvailable", 0) * 100 >= meminfo.get("MemTotal", 0) * memory_floor:
        return False
    swap_total = meminfo.get("SwapTotal", 0)
    return swap_total == 0 or meminfo.get("SwapFree", 0) * 100 < swap_total * swap_floor


def parse_status(text: str) -> tuple[str, int, int] | None:
    """(name, resident kB, swapped kB) from /proc/PID/status, or None for a kernel thread."""
    fields = {}
    for line in text.splitlines():
        name, _, rest = line.partition(":")
        if name in ("Name", "VmRSS", "VmSwap"):
            fields[name] = rest.strip()
    if "VmRSS" not in fields:
        return None
    return fields.get("Name", "?"), int(fields["VmRSS"].split()[0]), int(fields.get("VmSwap", "0 kB").split()[0])


def processes(proc: Path = Path("/proc")) -> list[tuple[int, str, int, int]]:
    """Every user process as (pid, name, resident kB, swapped kB)."""
    rows = []
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            status = parse_status((entry / "status").read_text())
        except OSError:
            continue  # the process ended while we were reading
        if status:
            rows.append((int(entry.name), *status))
    return rows


def choose_victim(rows: list[tuple[int, str, int, int]], own_pid: int) -> tuple[int, str, int, int] | None:
    """The unprotected process holding the most memory (resident plus swapped), if it is large enough."""
    candidates = [row for row in rows if row[0] not in (1, own_pid) and row[1] not in PROTECTED_NAMES]
    if not candidates:
        return None
    victim = max(candidates, key=lambda row: row[2] + row[3])
    return victim if victim[2] + victim[3] >= MIN_VICTIM_KB else None


def largest(rows: list[tuple[int, str, int, int]], count: int = 3) -> str:
    top = sorted(rows, key=lambda row: row[2] + row[3], reverse=True)[:count]
    return ",".join(f"{name}:{rss // MIB}+{swap // MIB}" for _, name, rss, swap in top)


def record_line(clock: str, meminfo: dict[str, int], low_available: int, high_swap: int, pressure: float,
                rows: list[tuple[int, str, int, int]]) -> str:
    """One record: the interval's lowest available memory and highest swap use, in MiB."""
    return (f"{clock} available_min={low_available // MIB} swap_used_max={high_swap // MIB} "
            f"memory_total={meminfo.get('MemTotal', 0) // MIB} swap_total={meminfo.get('SwapTotal', 0) // MIB} "
            f"pressure_full10={pressure:.0f} largest_rss+swap={largest(rows)}")


def harden() -> None:
    """Stay runnable while the machine is starved: lock our pages, refuse the kernel's own killer, and run
    ahead of ordinary processes. Each needs privileges the job may lack, so each failure is ignored."""
    try:
        ctypes.CDLL(None, use_errno=True).mlockall(3)  # MCL_CURRENT | MCL_FUTURE
    except (OSError, AttributeError):
        pass
    try:
        Path("/proc/self/oom_score_adj").write_text("-1000")
    except OSError:
        pass
    try:
        os.sched_setscheduler(0, os.SCHED_FIFO, os.sched_param(10))
    except (OSError, AttributeError):
        pass


def watch(log: Path, interval: float, record_every: float, memory_floor: float, swap_floor: float, *,
          proc: Path = Path("/proc"), kill=os.kill, sleep=time.sleep, rounds: int | None = None) -> int:
    """Guard until killed (or for `rounds` checks: tests pass that, with a stand-in /proc, kill and sleep)."""
    meminfo_path, pressure_path, own_pid = proc / "meminfo", proc / "pressure" / "memory", os.getpid()

    def write(line: str) -> None:
        with log.open("a", encoding="utf-8") as out:
            out.write(line + "\n")

    write(f"{time.strftime('%H:%M:%S')} watching: kill the largest process when available memory is under "
          f"{memory_floor:g}% and free swap is under {swap_floor:g}%")
    low_available, high_swap, next_record = None, 0, time.monotonic() + record_every
    while rounds is None or rounds > 0:
        rounds = None if rounds is None else rounds - 1
        meminfo = parse_meminfo(meminfo_path.read_text())
        available = meminfo.get("MemAvailable", 0)
        swap_used = meminfo.get("SwapTotal", 0) - meminfo.get("SwapFree", 0)
        low_available = available if low_available is None else min(low_available, available)
        high_swap = max(high_swap, swap_used)
        if starved(meminfo, memory_floor, swap_floor):
            rows = processes(proc)
            victim = choose_victim(rows, own_pid)
            if victim:
                pid, name, rss, swap = victim
                try:
                    kill(pid, getattr(signal, "SIGKILL", 9))  # before any log write can block on a starved disk
                    outcome = "killed"
                except OSError as error:
                    outcome = f"kill failed ({error.strerror})"
                write(f"{time.strftime('%H:%M:%S')} {KILLED} pid={pid} name={name} rss={rss // MIB} "
                      f"swap={swap // MIB} outcome={outcome} available={available // MIB} "
                      f"swap_free={meminfo.get('SwapFree', 0) // MIB} largest_rss+swap={largest(rows, 5)}")
                sleep(max(interval, 5.0))  # let the kernel return the memory before looking again
                continue
        if time.monotonic() >= next_record:
            try:
                pressure = parse_pressure(pressure_path.read_text())
            except OSError:
                pressure = 0.0
            write(record_line(time.strftime("%H:%M:%S"), meminfo, low_available, high_swap, pressure,
                              processes(proc)))
            low_available, high_swap, next_record = None, 0, time.monotonic() + record_every
        sleep(interval)
    return 0


def summarize(lines: list[str]) -> tuple[list[str], list[str]]:
    """(summary lines, kill lines) of a watch log."""
    lowest, highest, totals, kills = None, 0, "", []
    for line in lines:
        if f" {KILLED} " in line:
            kills.append(line)
            continue
        fields = dict(field.split("=", 1) for field in line.split()[1:] if "=" in field)
        if "available_min" not in fields:
            continue
        available, swap = int(fields["available_min"]), int(fields["swap_used_max"])
        lowest = available if lowest is None else min(lowest, available)
        highest = max(highest, swap)
        totals = f" of {fields['memory_total']} MiB memory and {fields['swap_total']} MiB swap"
    if lowest is None:
        summary = ["Memory guard: no record was written."]
    else:
        summary = [f"Memory guard: lowest available memory {lowest} MiB, highest swap use {highest} MiB{totals}."]
    if kills:
        summary.append(f"Memory guard killed {len(kills)} process(es) because memory and swap ran out; "
                       "the failing step is that process, not a test result.")
    return summary, kills


def report(log: Path) -> int:
    if not log.exists():
        print(f"Memory guard: no log at {log}.")
        return 0
    lines = log.read_text(encoding="utf-8", errors="replace").splitlines()
    print("\n".join(lines))
    summary, kills = summarize(lines)
    print("\n".join(summary))
    for line in kills:
        print(f"::error title=Memory guard::{line}")
    target = os.environ.get("GITHUB_STEP_SUMMARY")
    if target:
        with open(target, "a", encoding="utf-8") as out:
            out.write("\n".join(summary + [f"- `{line}`" for line in kills]) + "\n")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    watching = commands.add_parser("watch", help="guard the job until killed")
    watching.add_argument("--log", type=Path, required=True)
    watching.add_argument("--interval", type=float, default=2.0, help="seconds between checks")
    watching.add_argument("--record-every", type=float, default=60.0, help="seconds between record lines")
    watching.add_argument("--memory-floor", type=float, default=8.0, help="percent of memory still available")
    watching.add_argument("--swap-floor", type=float, default=8.0, help="percent of swap still free")
    reporting = commands.add_parser("report", help="print the record and its peaks")
    reporting.add_argument("--log", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.command == "watch":
        harden()
        return watch(args.log, args.interval, args.record_every, args.memory_floor, args.swap_floor)
    return report(args.log)


if __name__ == "__main__":
    sys.exit(main())
