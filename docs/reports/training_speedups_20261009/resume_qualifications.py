"""One-time bounded resume of eight existing guarded Haley qualifications.

Run only after the parent verifies the former batch process is terminal.
No native executable, acquisition API, retry, deletion or full block is used.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
from datetime import datetime, timezone
import hashlib
import importlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    path = Path(path)
    with path.open("rb") as stream:
        return {"path": str(path), "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write_state(root, phase, **fields):
    value = {"schema": "qualification-resume/v1", "phase": phase,
             "observed_utc": datetime.now(timezone.utc).isoformat(), **fields}
    pending = root / "qualification-resume-state.pending"
    with pending.open("w", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")
    pending.replace(root / "qualification-resume-state.json")


class DirectoryChangeWait:
    """Read-only event handle on the canonical lock's parent directory."""
    def __init__(self, directory):
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.first = self.kernel.FindFirstChangeNotificationW
        self.first.argtypes = [wintypes.LPCWSTR, wintypes.BOOL, wintypes.DWORD]
        self.first.restype = wintypes.HANDLE
        self.next = self.kernel.FindNextChangeNotification
        self.next.argtypes = [wintypes.HANDLE]
        self.next.restype = wintypes.BOOL
        self.close_api = self.kernel.FindCloseChangeNotification
        self.close_api.argtypes = [wintypes.HANDLE]
        self.close_api.restype = wintypes.BOOL
        self.wait_api = self.kernel.WaitForSingleObject
        self.wait_api.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.wait_api.restype = wintypes.DWORD
        # File names and last-write changes; no descendant tree watch.
        self.handle = self.first(str(directory), False, 0x1 | 0x10)
        require(self.handle and self.handle != ctypes.c_void_p(-1).value,
                "canonical lock directory notification handle unavailable")

    def wait(self, milliseconds):
        result = self.wait_api(self.handle, milliseconds)
        require(result in (0, 0x102), "canonical lock directory event wait failed")
        if result == 0:
            require(self.next(self.handle), "cannot rearm canonical lock directory event")
        return result == 0

    def close(self):
        self.close_api(self.handle)


def await_free(root, reservations, deadline, completed, case):
    # Arm before the first status read so a release during inspection cannot
    # be missed. Watching the lock never opens, changes or clears its contents.
    watcher = DirectoryChangeWait(reservations.lock_path().parent)
    last_projection = None
    try:
        status = reservations.status()
        next_refresh = time.monotonic() + 600
        while status.get("state") != "free":
            record = status.get("record")
            require(isinstance(record, dict), "reservation state requires manual investigation")
            safe_owner = {key: record[key] for key in
                          ("host", "generation", "lane", "owner_pid", "owner_process_creation_time")}
            work = [{key: item[key] for key in ("pid", "creation_time", "state")}
                    for item in (status.get("processes") or {}).values()]
            descendants = [{key: item[key] for key in ("pid", "creation_time") if key in item}
                           for item in (status.get("live_descendants") or [])]
            owner_state = reservations.process_state(record["owner_pid"], record["owner_process_creation_time"])
            projection = {"dependency_owner": safe_owner,
                          "dependency_owner_identity_state": owner_state,
                          "dependency_process_alive": owner_state == "alive" or any(p["state"] == "alive" for p in work) or bool(descendants),
                          "canonical_state": status["state"], "recorded_work": work,
                          "live_descendants": descendants}
            if projection != last_projection:
                write_state(root, "waiting-reservation", case=case, completed=completed,
                            **projection, wake_condition="canonical lock directory event, then canonical status is free",
                            wait_limit_seconds=7200, fallback_status_refresh_seconds=600)
                last_projection = projection
            # An absent acquirer is never release permission. Surviving recorded
            # work or descendants remain protected by the canonical lock.
            while True:
                remaining = deadline - time.monotonic()
                require(remaining > 0, "two-hour reservation wait bound reached; no further case launched")
                signaled = watcher.wait(min(60000, max(1, int(remaining * 1000))))
                if signaled or time.monotonic() >= next_refresh:
                    status = reservations.status()
                    next_refresh = time.monotonic() + 600
                    break
    finally:
        watcher.close()


def validate_plan(root, plan):
    require(plan.get("stage") == "qualification", "qualification plan required")
    entries = plan["commands"]
    labels = [f"qual-{variant}-w{workers}" for variant in ("baseline", "candidate") for workers in (1, 2, 4, 8)]
    require(len(entries) == 8 and [Path(e["request"]).stem for e in entries] == labels,
            "expected eight distinct existing qualification requests in prepared order")
    for entry in entries:
        argv = entry["argv"]
        require(entry["action"] == "qualify" and argv[1] == "-B"
                and Path(argv[0]).resolve() == Path(sys.executable).resolve()
                and Path(argv[2]).resolve() == (root / "case_driver.py").resolve(),
                "preserve pinned Python and existing supported case driver")
        require(argv[argv.index("--action") + 1] == "qualify"
                and Path(argv[argv.index("--request") + 1]).resolve() == Path(entry["request"]).resolve(),
                "case argv differs from prepared qualification request")
        request = read(entry["request"])
        require(request["kind"] == "training" and request["placement"]["host"] == "computehost"
                and not Path(request["root"]).exists() and not Path(request["cold_root"]).exists(),
                "qualification request or fresh roots changed; preserve attempts")
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("C:/mtg-node/training-speedups-20261009"))
    args = parser.parse_args()
    root = args.root.resolve()
    require(os.name == "nt" and platform.node().upper() == "HALEYSPC" and sys.version_info[:2] == (3, 13),
            "run on Haley with the pinned Python 3.13 interpreter")
    require(root.is_dir(), "prepared root missing")
    # Exclusive one-time marker prevents duplicate coordinators. It is retained
    # after either success or failure; a failed case is never retried here.
    with (root / "qualification-resume.once").open("x", encoding="utf-8") as marker:
        marker.write(str(os.getpid()) + "\n")
    completed = []
    active = None
    failure_evidence = {}
    try:
        plan_path = root / "requests/qualification.plan.json"
        entries = validate_plan(root, read(plan_path))
        plan_pin = pin(plan_path)
        resumed = []
        for index, entry in enumerate(entries):
            argv = list(entry["argv"])
            receipt_index = argv.index("--receipt") + 1
            label = Path(entry["request"]).stem
            if index == 0:
                prior = Path(argv[receipt_index])
                argv[receipt_index] = str(prior.with_name(prior.stem + ".attempt2" + prior.suffix))
                log_path = root / (label + ".controller.attempt2.log")
            else:
                log_path = root / (label + ".controller.log")
            require(not Path(argv[receipt_index]).exists() and not log_path.exists(),
                    "resume receipt/log already exists; no automatic retry")
            resumed.append({"action": "qualify", "request": entry["request"],
                            "request_pin": pin(entry["request"]), "driver": pin(argv[2]),
                            "argv": argv, "controller_log": str(log_path)})
        resume_plan = root / "requests/qualification.resume-plan.json"
        with resume_plan.open("x", encoding="utf-8") as stream:
            json.dump({"schema": "qualification-resume-plan/v1", "original_plan": plan_pin,
                       "commands": resumed}, stream, indent=2, allow_nan=False)
            stream.write("\n")
        resume_plan_pin = pin(resume_plan)
        sys.path.insert(0, str(root / "candidate/python/tools"))
        reservations = importlib.import_module("host_reservation_v1")
        require(reservations.HOST == "HALEYSPC", "canonical reservation host differs")
        deadline = time.monotonic() + 7200
        for entry in resumed:
            active = Path(entry["request"]).stem
            await_free(root, reservations, deadline, completed, active)
            argv = entry["argv"]
            receipt_index = argv.index("--receipt") + 1
            log_path = Path(entry["controller_log"])
            require(pin(entry["request"]) == entry["request_pin"] and pin(argv[2]) == entry["driver"],
                    "resume request or driver changed; no further launch")
            failure_evidence = {"case": active, "controller_receipt": argv[receipt_index], "controller_log": str(log_path)}
            write_state(root, "qualifying", case=active, completed=completed,
                        plan=plan_pin, resume_plan=resume_plan_pin, request=entry["request_pin"], driver=entry["driver"], **{
                            "controller_receipt": argv[receipt_index], "controller_log": str(log_path)})
            # The released preflight is advisory. The same supported case
            # driver/dispatcher performs the actual atomic reservation guard.
            with log_path.open("x", encoding="utf-8") as log:
                result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
            require(result.returncode == 0, f"qualification case failed with exit {result.returncode}; no retry")
            receipt = read(argv[receipt_index])
            require(receipt.get("complete") is True, "controller receipt is incomplete; no retry")
            completed.append({"case": active, "controller_receipt": pin(argv[receipt_index])})
        for variant in ("baseline", "candidate"):
            active = "measure-allocation-" + variant
            output = root / (variant + "-qualification-allocation.json")
            error_log = root / (variant + "-qualification-allocation.stderr.log")
            failure_evidence = {"case": active, "allocation_report": str(output), "allocation_stderr": str(error_log)}
            write_state(root, "measuring-allocation", variant=variant, completed=completed)
            with output.open("x", encoding="utf-8") as log, error_log.open("x", encoding="utf-8") as errors:
                result = subprocess.run([sys.executable, "-B", str(root / "measure_allocations.py"),
                                         "--root", str(root / "measure"), "--native-root",
                                         str(root / "measure/hot" / f"qual-{variant}-w1" / "native")],
                                        stdout=log, stderr=errors)
            require(result.returncode == 0, f"allocation measurement failed with exit {result.returncode}; no retry")
        write_state(root, "complete", completed=completed, plan=plan_pin, resume_plan=resume_plan_pin,
                    allocation_reports=[pin(root / (v + "-qualification-allocation.json")) for v in ("baseline", "candidate")])
        return 0
    except Exception as error:
        # Never serialize status records, reservation tokens or command lines.
        write_state(root, "failed", completed=completed, case=active,
                    error_type=type(error).__name__, error=str(error), evidence=failure_evidence)
        return 2


if __name__ == "__main__":
    sys.exit(main())
