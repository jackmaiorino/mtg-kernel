"""Guarded CPU-only search experiment dispatch inside current host_slots claims.

The parent owns the manifest and aggregate allocation. Qualification batches are
bounded correctness/timing work; measurement refuses missing/incompatible
serial versus parallel completed-work evidence. Run `--help` for the interface.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
from contextlib import contextmanager

import psutil

GIB = 1024**3
CPU_CAP = 12 * 3600
OUTPUT_CAP = 4 * GIB
SCHEMA = "search-sharing-allocation-runtime/v1"


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def identity(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def publish(path, value):
    path = Path(path)
    temp = path.with_suffix(path.suffix + ".tmp")
    with temp.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=1, allow_nan=False)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, path)


def append_ledger(path, value):
    with Path(path).open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(value, allow_nan=False) + "\n")
        stream.flush()
        os.fsync(stream.fileno())


def accounted_cpu(rows):
    # An interrupted queue with no final accounting retains its entire reserved
    # allotment. The parent may reconcile it from its retained process evidence.
    latest = {row["label"]: row for row in rows}
    return sum(row.get("reserved_cpu_seconds", row["cpu_seconds"]) for row in latest.values())


def checked(asset):
    require(digest(asset["path"]) == asset["sha256"], f"input hash differs: {asset['path']}")
    return Path(asset["path"])


def output_bytes(root):
    # This walks only the declared owned output root, never a source volume.
    total = 0
    for folder, directories, files in os.walk(root, followlinks=False):
        directories[:] = [d for d in directories if not Path(folder, d).is_symlink()
                           and not Path(folder, d).is_junction()]
        for name in files:
            path = Path(folder, name)
            if not path.is_symlink():
                total += path.stat().st_size
    return total


def storage_guard(spec, extra=0):
    root = Path(spec["root"])
    require(root.is_dir(), "owned scratch root must exist")
    limit = spec["output_cap_bytes"]
    require(0 < limit <= OUTPUT_CAP, "output cap exceeds authorized 4 GiB")
    used = output_bytes(root)
    require(used + extra <= limit, "owned output byte cap reached")
    require(shutil.disk_usage(root).free - extra >= 60 * GIB, "target volume below 60 GiB reserve")
    return used


def compatibility(spec):
    return {"host": platform.node().upper(), "binary_sha256": spec["binary"]["sha256"],
            "workload_sha256": identity(spec["workload"]),
            "input_sha256s": sorted(a["sha256"] for a in spec["inputs"]),
            "storage": spec["storage"]}


def qualification_guard(spec, workers):
    receipt = load(checked(spec["qualification"]))
    require(receipt["compatible"] == compatibility(spec), "throughput qualification incompatible")
    require(receipt["parity"] is True, "serial/parallel primary output parity missing")
    rates = receipt["rates"]
    require("1" in rates and any(int(n) > 1 for n in rates), "serial and parallel throughput required")
    for count, row in rates.items():
        require(int(count) >= 1 and row["completed_jobs"] > 0 and row["wall_seconds"] > 0,
                "qualification has no completed work")
        require(math.isfinite(row["completed_per_second"]) and row["completed_per_second"] > 0,
                "invalid throughput rate")
    require(str(workers) in rates, "requested worker count was not measured")
    eligible = [n for n in rates if int(n) <= spec["eligible_workers"]]
    require(eligible, "no measured eligible allocation")
    best = max(eligible, key=lambda n: rates[n]["completed_per_second"])
    require(workers == int(best), "requested allocation is not fastest qualified eligible count")


def validate(spec, workers, phase):
    require(spec["schema"] == SCHEMA, "unsupported manifest schema")
    require(spec["host"].upper() == platform.node().upper(), "manifest names another host")
    require(1 <= workers <= spec["eligible_workers"] <= (os.cpu_count() or 1), "invalid worker count")
    require(0 < spec["cpu_cap_seconds"] <= CPU_CAP, "CPU cap exceeds authorized 12 aggregate hours")
    require(spec["projected_output_bytes"] >= 0, "negative output projection")
    require(spec.get("external_cpu_seconds", 0) >= 0, "negative external CPU accounting")
    storage_guard(spec, spec["projected_output_bytes"])
    checked(spec["binary"])
    for asset in spec["inputs"]:
        checked(asset)
    require(spec["jobs"] and len({j["id"] for j in spec["jobs"]}) == len(spec["jobs"]), "job IDs must be unique")
    for job in spec["jobs"]:
        require(job["id"].replace("-", "").replace("_", "").isalnum(), "invalid job ID")
        require(0 < job["max_wall_seconds"] and 0 < job["max_cpu_seconds"] <= spec["cpu_cap_seconds"],
                "job lacks bounded CPU and wall limits")
        require(job["expected_rows"] > 0, "expected completed row count required")
        require(job.get("group"), "matched root/seed affinity group required")
        require(not any(a.startswith("out=") or a.startswith("workers=") for a in job["argv"]),
                "launcher owns output path and single-worker process count")
    if phase == "measure":
        qualification_guard(spec, workers)
    else:
        require(0 < spec["qualification_cpu_cap_seconds"] <= 1200, "small qualification CPU cap exceeds 20 minutes")


@contextmanager
def ledger_owner(path):
    """One queue owns this host allotment; never race two aggregate budget readers."""
    lock = Path(str(path) + ".lock")
    with lock.open("x", encoding="utf-8") as stream:
        stream.write(str(os.getpid()))
    try:
        yield
    finally:
        lock.unlink()


class ProcessClock:
    """Keep a Windows handle so final CPU remains readable after process exit."""
    def __init__(self, child):
        self.child = child
        self.last = 0.0
        self.handle = None
        if os.name == "nt":
            self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
            self.kernel.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
            self.kernel.OpenProcess.restype = ctypes.c_void_p
            self.kernel.GetProcessTimes.argtypes = [ctypes.c_void_p] + [ctypes.POINTER(ctypes.c_ulonglong)] * 4
            self.kernel.CloseHandle.argtypes = [ctypes.c_void_p]
            self.handle = self.kernel.OpenProcess(0x1200, False, child.pid)
            require(self.handle, "cannot open process CPU accounting handle")

    def qualify_power(self):
        class State(ctypes.Structure):
            _fields_ = [("version", ctypes.c_ulong), ("control", ctypes.c_ulong), ("state", ctypes.c_ulong)]
        self.kernel.SetProcessInformation.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
        self.kernel.GetProcessInformation.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_ulong]
        state = State(1, 1, 0)
        require(self.kernel.SetProcessInformation(self.handle, 4, ctypes.byref(state), ctypes.sizeof(state)),
                "cannot disable native process EcoQoS")
        actual = State(1, 0, 0)
        require(self.kernel.GetProcessInformation(self.handle, 4, ctypes.byref(actual), ctypes.sizeof(actual)),
                "cannot verify native process power policy")
        require(actual.control & 1 and not actual.state & 1, "native process execution throttling still enabled")
        return {"control_mask": actual.control, "state_mask": actual.state}

    def read(self):
        if self.handle:
            times = [ctypes.c_ulonglong() for _ in range(4)]
            require(self.kernel.GetProcessTimes(self.handle, *[ctypes.byref(t) for t in times]),
                    "process CPU accounting failed")
            self.last = (times[2].value + times[3].value) / 1e7
        else:
            try:
                self.last = sum(psutil.Process(self.child.pid).cpu_times()[:2])
            except psutil.NoSuchProcess:
                pass
        return self.last

    def close(self):
        if self.handle:
            self.kernel.CloseHandle(self.handle)
            self.handle = None


def primary(path, expected):
    rows = [json.loads(line) for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]
    require(len(rows) == expected and all(r.get("kind") == "search_allocation_experiment" for r in rows),
            "job output missing expected completed rows")
    require(all(r.get("primary_sha256") and r.get("evaluation_sha256") for r in rows), "native primary hash missing")
    return identity([(r["primary_sha256"], r["evaluation_sha256"]) for r in rows])


def execute(spec, workers, phase, label):
    """Called only inside a live host_slots claim or timed declaration."""
    require(os.environ.get("HOST_SLOTS_CLAIM") or os.environ.get("MTG_HOST_RESERVATION_TOKEN"),
            "execution requires current host_slots containment")
    require(os.name == "nt", "exact final process CPU accounting currently supports Windows only")
    validate(spec, workers, phase)
    root = Path(spec["root"])
    destination = root / label
    destination.mkdir()  # never overwrite or resume an attempt accidentally
    results = [None] * len(spec["jobs"])
    ledger = Path(spec["ledger"])
    previous = [json.loads(line) for line in ledger.read_text().splitlines() if line.strip()] if ledger.exists() else []
    accounted = spec.get("external_cpu_seconds", 0) + accounted_cpu(previous)
    require(accounted < spec["cpu_cap_seconds"], "aggregate process CPU cap already reached")
    phase_cap = spec["qualification_cpu_cap_seconds"] if phase == "qualify" else spec["cpu_cap_seconds"]
    require(label not in {row["label"] for row in previous}, "attempt label already used in CPU ledger")
    append_ledger(ledger, {"label": label, "phase": phase, "cpu_seconds": 0,
                           "reserved_cpu_seconds": min(phase_cap, spec["cpu_cap_seconds"] - accounted),
                           "pid": os.getpid(), "state": "started"})
    manager_start = time.process_time()
    batch_start = time.monotonic()
    active = {}
    pending = list(range(len(results)))
    cores = sorted(psutil.Process().cpu_affinity())
    require(len(cores) == workers, "current claimed affinity does not match worker allocation")
    groups = list(dict.fromkeys(job["group"] for job in spec["jobs"]))
    group_core = {group: cores[i % workers] for i, group in enumerate(groups)}
    error = None
    last_checkpoint = 0.0
    try:
        while active or pending:
            now_cpu = sum(row["cpu_seconds"] for row in results if row) + sum(p["clock"].read() for p in active.values())
            overhead = time.process_time() - manager_start
            # Stop early enough for at most one poll interval on every active worker.
            margin = workers * .1 + .5
            require(accounted + now_cpu + overhead + margin < spec["cpu_cap_seconds"], "aggregate CPU cap reached")
            require(now_cpu + overhead + margin < phase_cap, "qualification CPU cap reached")
            storage_guard(spec, workers * 1024 * 1024)
            require(not (root / "STOP").exists(), "experiment STOP file present")
            used_cores = {item["core"] for item in active.values()}
            for index in list(pending):
                core = group_core[spec["jobs"][index]["group"]]
                if core in used_cores:
                    continue
                job = spec["jobs"][index]
                folder = destination / job["id"]
                folder.mkdir()
                output = folder / "output.jsonl"
                env = {k: v for k, v in os.environ.items() if not k.startswith(("PROFILE_", "BUDGET_", "S4A_"))
                       and k not in ("ROOTS", "OPPONENTS")}
                env.update(job["env"])
                env["CUDA_VISIBLE_DEVICES"] = ""
                command = [spec["binary"]["path"], *job["argv"], f"out={output.as_posix()}", "workers=1"]
                stream = (folder / "stdout.log").open("wb")
                # Configure the exact affinity/power policy before any native work.
                flags = subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW | 0x4
                child = subprocess.Popen(command, env=env, stdout=stream, stderr=subprocess.STDOUT,
                                         creationflags=flags, cwd=root)
                active[index] = {"child": child, "clock": ProcessClock(child), "stream": stream,
                                 "output": output, "started": time.monotonic(), "command": command, "core": core}
                native = psutil.Process(child.pid)
                native.cpu_affinity([core])
                require(native.cpu_affinity() == [core], "native affinity verification failed")
                active[index]["power"] = active[index]["clock"].qualify_power()
                nt = ctypes.WinDLL("ntdll")
                nt.NtResumeProcess.argtypes = [ctypes.c_void_p]
                require(nt.NtResumeProcess(int(child._handle)) == 0, "cannot resume configured native process")
                pending.remove(index)
                used_cores.add(core)
            for index, item in list(active.items()):
                child, job = item["child"], spec["jobs"][index]
                elapsed = time.monotonic() - item["started"]
                require(elapsed < job["max_wall_seconds"], f"job wall cap: {job['id']}")
                require(item["clock"].read() + .2 < job["max_cpu_seconds"], f"job CPU cap: {job['id']}")
                if child.poll() is None:
                    continue
                cpu = item["clock"].read()
                item["clock"].close()
                item["stream"].close()
                row = {"id": job["id"], "cpu_seconds": cpu, "wall_seconds": elapsed,
                       "exit_code": child.returncode, "command": item["command"], "output": str(item["output"]),
                       "affinity_core": item["core"], "power": item["power"], "group": job["group"]}
                results[index] = row
                del active[index]
                require(child.returncode == 0, f"native job failed: {job['id']}")
                row["primary_sha256"] = primary(item["output"], job["expected_rows"])
                row["output_sha256"] = digest(item["output"])
                publish(destination / "completion.json", {"results": results})
            if time.monotonic() - last_checkpoint >= 5:
                publish(destination / "completion.json", {"state": "running", "label": label,
                        "workers": workers, "cpu_seconds": now_cpu + overhead, "accounted_before": accounted,
                        "results": results, "pending": len(pending),
                        "running": [{"id": spec["jobs"][index]["id"], "pid": item["child"].pid,
                                     "cpu_seconds": item["clock"].read(), "affinity_core": item["core"]}
                                    for index, item in active.items()]})
                last_checkpoint = time.monotonic()
            time.sleep(.1)
    except BaseException as caught:
        error = f"{type(caught).__name__}: {caught}"
        for index, item in active.items():
            item["child"].kill()
            item["child"].wait()
            results[index] = {"id": spec["jobs"][index]["id"], "cpu_seconds": item["clock"].read(),
                              "wall_seconds": time.monotonic() - item["started"], "incomplete": True,
                              "output": str(item["output"]), "command": item["command"]}
            item["clock"].close()
            item["stream"].close()
    finally:
        cpu = sum(row["cpu_seconds"] for row in results if row) + time.process_time() - manager_start
        record = {"label": label, "phase": phase, "workers": workers, "cpu_seconds": cpu,
                  "wall_seconds": time.monotonic() - batch_start, "compatible": compatibility(spec),
                  "results": results, "error": error, "output_bytes": output_bytes(root)}
        publish(destination / "completion.json", record)
        append_ledger(ledger, {k: record[k] for k in ("label", "phase", "cpu_seconds", "error")})
    require(error is None, error)
    return record


def qualify(paths, out):
    batches = [load(path) for path in paths]
    require(len(batches) >= 2, "need serial and parallel batches")
    first = batches[0]
    require(all(not row["error"] and row["compatible"] == first["compatible"] for row in batches),
            "incomplete or incompatible qualification batch")
    canonical = [(r["id"], r["primary_sha256"]) for r in first["results"]]
    require(all([(r["id"], r["primary_sha256"]) for r in row["results"]] == canonical for row in batches),
            "serial/parallel jobs or primary outputs differ")
    rates = {str(row["workers"]): {"completed_jobs": len(row["results"]), "wall_seconds": row["wall_seconds"],
                                  "cpu_seconds": row["cpu_seconds"],
                                  "completed_per_second": len(row["results"]) / row["wall_seconds"]} for row in batches}
    require("1" in rates and any(int(n) > 1 for n in rates), "serial and parallel counts required")
    receipt = {"schema": SCHEMA, "compatible": first["compatible"], "parity": True, "rates": rates,
               "batches": [{"path": str(path), "sha256": digest(path)} for path in paths]}
    publish(out, receipt)
    return receipt


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("dispatch", "worker", "check"):
        sub = commands.add_parser(name)
        sub.add_argument("--manifest", required=True)
        sub.add_argument("--phase", choices=("qualify", "measure"), required=True)
        sub.add_argument("--workers", type=int, required=True)
        sub.add_argument("--label", required=True)
    sub = commands.add_parser("qualify")
    sub.add_argument("--batch", action="append", required=True)
    sub.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    if args.command == "qualify":
        print(json.dumps(qualify(args.batch, args.out)))
        return 0
    spec = load(args.manifest)
    require(args.label.replace("-", "").replace("_", "").isalnum(), "invalid attempt label")
    validate(spec, args.workers, args.phase)
    if args.command == "check":
        print(json.dumps({"valid": True, "compatible": compatibility(spec)}))
        return 0
    if args.command == "worker":
        with ledger_owner(spec["ledger"]):
            print(json.dumps(execute(spec, args.workers, args.phase, args.label)))
        return 0
    # The existing shared allocator performs the live reservation/claim check,
    # pins every descendant, and suspends claims if timed ownership changes.
    slots = checked(spec["host_slots"])
    cold = Path(spec["cold_binary_root"]) / spec["binary"]["sha256"]
    cold.mkdir(parents=True, exist_ok=True)
    target = cold / Path(spec["binary"]["path"]).name
    if not target.exists():
        require(shutil.disk_usage(cold).free - Path(spec["binary"]["path"]).stat().st_size >= 60 * GIB,
                "cold pin volume below reserve")
        shutil.copy2(spec["binary"]["path"], target)
    require(digest(target) == spec["binary"]["sha256"], "cold binary pin differs")
    command = [sys.executable, "-B", str(slots), "run", "--lane", "codex-search-sharing-allocation-20261013",
               "--work-id", args.label, "--cores", str(args.workers), "--priority", "below_normal"]
    if spec.get("wait_for_cores"):
        command += ["--wait", "--wait-timeout", str(spec["wait_timeout_seconds"])]
    command += ["--",
               sys.executable, "-B", str(Path(__file__).resolve()), "worker", "--manifest", str(Path(args.manifest).resolve()),
               "--phase", args.phase, "--workers", str(args.workers), "--label", args.label]
    return subprocess.call(command)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError) as error:
        print(json.dumps({"refused": str(error)}), file=sys.stderr)
        raise SystemExit(2)
