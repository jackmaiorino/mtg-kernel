"""Owned Haley staging and logon-local drive adapter for fixed #474 binaries.

No drive or reservation is created by import. The probe lists four tests only.
The global host helpers are used unchanged in the original SSH logon session.
"""
from __future__ import annotations

import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

import drake_history_timing_v1 as timing

CAP = 768 * 1024**2
LOG_CAP = 256 * 1024**2
FLOOR = 60 * 1024**3
PREFIX = Path("C:/mtg-node")


def owned_root(value):
    root = Path(value).absolute()
    timing.require(root.parent == PREFIX and root.name.startswith("codex-drake474-timing-"), "not an owned staging root")
    timing.require(not root.is_symlink() and (not root.exists() or root.resolve() == root), "reparse staging root")
    return root


def bounds(root):
    timing.require(shutil.disk_usage(root).free >= FLOOR, "60 GiB floor")
    total = logs = 0
    for p in root.rglob("*"):
        timing.require(not p.is_symlink() and not p.is_junction(), "reparse staging member")
        if p.is_file():
            size = p.stat().st_size
            total += size
            if "results" in p.relative_to(root).parts:
                logs += size
    timing.require(total <= CAP and logs <= LOG_CAP, "remote storage cap")
    return {"owned_bytes": total, "result_bytes": logs, "free_bytes": shutil.disk_usage(root).free}


def waiter_priority(root):
    status = timing.read("C:/mtg-node/combined-trainer-20261108/queue-status.json")
    usage = bounds(root)
    reason = status.get("reason")
    own_reservation = False
    if reason == "existing timed reservation":
        import host_reservation_v1 as host
        token = os.environ.get(host.TOKEN_ENV)
        current = host.status(token) if token else {}
        own_reservation = (current.get("token_fate") == "holds"
                           and current.get("record", {}).get("lane") == "codex-history474-timing")
    # Count our reclaimable staging too: it must not hide newly available
    # space from the earlier waiter's 60 GiB + 5.5 GiB admission boundary.
    timing.require(status.get("state") == "waiting"
                   and (reason == "less than 5.5 GiB above the 60 GiB disk floor" or own_reservation)
                   and usage["free_bytes"] + usage["owned_bytes"] < FLOOR + int(5.5 * 1024**3),
                   "earlier trainer readiness changed; yield owned timing work")


class Luid(c.Structure):
    _fields_ = [("low", w.DWORD), ("high", w.LONG)]


class Stats(c.Structure):
    _fields_ = [("token", Luid), ("auth", Luid), ("expiry", c.c_longlong),
                ("kind", w.DWORD), ("impersonation", w.DWORD), ("charged", w.DWORD),
                ("available", w.DWORD), ("groups", w.DWORD), ("privileges", w.DWORD), ("modified", Luid)]


class Namespace:
    def __init__(self):
        self.k = c.WinDLL("kernel32", use_last_error=True)
        self.a = c.WinDLL("advapi32", use_last_error=True)
        self.k.GetCurrentProcess.restype = w.HANDLE
        self.k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
        self.k.OpenProcess.restype = w.HANDLE
        self.k.CloseHandle.argtypes = [w.HANDLE]
        self.a.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]
        self.a.GetTokenInformation.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]
        self.a.ConvertSidToStringSidW.argtypes = [c.c_void_p, c.POINTER(w.LPWSTR)]
        self.k.LocalFree.argtypes = [c.c_void_p]
        self.k.QueryDosDeviceW.argtypes = [w.LPCWSTR, w.LPWSTR, w.DWORD]
        self.k.DefineDosDeviceW.argtypes = [w.DWORD, w.LPCWSTR, w.LPCWSTR]

    def identity(self, pid=None):
        process = self.k.GetCurrentProcess() if pid is None else self.k.OpenProcess(0x1000, False, pid)
        timing.require(process, "cannot query process identity")
        token = w.HANDLE()
        try:
            timing.require(self.a.OpenProcessToken(process, 8, c.byref(token)), "cannot query token")
            stats, length = Stats(), w.DWORD()
            timing.require(self.a.GetTokenInformation(token, 10, c.byref(stats), c.sizeof(stats), c.byref(length)), "token statistics")
            user = c.create_string_buffer(4096)
            timing.require(self.a.GetTokenInformation(token, 1, user, len(user), c.byref(length)), "token user")
            sid = w.LPWSTR()
            timing.require(self.a.ConvertSidToStringSidW(c.c_void_p.from_buffer(user), c.byref(sid)), "token SID")
            try:
                return {"auth": f"{stats.auth.high & 0xffffffff:08x}:{stats.auth.low:08x}", "sid": sid.value}
            finally:
                self.k.LocalFree(sid)
        finally:
            if token:
                self.k.CloseHandle(token)
            if pid is not None:
                self.k.CloseHandle(process)

    def query(self):
        result = c.create_unicode_buffer(65536)
        count = self.k.QueryDosDeviceW("D:", result, len(result))
        if not count:
            timing.require(c.get_last_error() == 2, "ambiguous D: lookup")
            return []
        return [s for s in result[:count].split("\0") if s]

    def define(self, flags, target):
        timing.require(self.k.DefineDosDeviceW(flags, "D:", target), "DefineDosDevice failed: " + str(c.get_last_error()))


def verify(root, expected):
    manifest = timing.read(root / "stage.json")
    timing.require(timing.pin(root / "stage.json")["sha256"] == expected, "staging manifest changed")
    timing.require(manifest["schema"] == "drake-remote-stage/v1" and manifest["root"] == str(root), "different staging root")
    for name, ref in manifest["files"].items():
        relative = Path(name)
        timing.require(not relative.is_absolute() and ".." not in relative.parts, "unsafe staging member")
        path = root / relative
        timing.require(path.resolve().is_relative_to(root) and not path.is_symlink(), "staging member escapes")
        timing.require(path.stat().st_size == ref["bytes"] and timing.pin(path)["sha256"] == ref["sha256"], "staging hash differs: " + name)
    bounds(root)
    return manifest


def mapped_identity(root):
    api = Namespace()
    expected = timing.read(root / "active-mapping.json")
    timing.require(api.identity() == expected["identity"] and api.query() == [expected["target"]], "different logon or drive mapping")
    return expected


def probe(root, manifest):
    import host_reservation_v1 as host
    import host_slots_v1 as slots
    mapping = mapped_identity(root)
    token = os.environ.get(host.TOKEN_ENV)
    timing.require(token and os.getpid() in host.exempt_pids(token), "probe not supervised")
    timed = slots.timed_state()
    timing.require(timed.get("token_sha256") == slots.token_hash(token)
                   and timed.get("why") == "timed work declared its cores"
                   and timed.get("cores") == [manifest["probe_core"]], "probe core declaration differs")
    output = root / "results" / "probe"
    output.mkdir()
    results = []
    for label, (_, binary, digest, checkout) in timing.BINARIES.items():
        timing.require(timing.pin(binary)["sha256"] == digest, "mapped binary hash differs")
        # Read every compiled runtime path under the active namespace.
        for name, ref in manifest["files"].items():
            prefix = "drive-d/" + Path(checkout).name + "/"
            if name.startswith(prefix):
                timing.require(timing.pin("D:/" + name.removeprefix("drive-d/"))["sha256"] == ref["sha256"], "mapped runtime hash differs")
        tree = slots.Tree([manifest["probe_core"]], "below_normal")
        log = output / (label + ".log")
        argv = [binary, timing.PREFIX, "--list"]
        started = time.monotonic()
        with log.open("xb") as stream:
            try:
                env = dict(os.environ)
                env.pop("MTG_LIMITED_MATCH_TRACE", None)
                proc = subprocess.Popen(argv, stdout=stream, stderr=subprocess.STDOUT, env=env)
                tree.child = proc
                while proc.poll() is None:
                    timing.require(time.monotonic() - started < 60, "list deadline")
                    bounds(root)
                    mapped_identity(root)
                    time.sleep(0.1)
                timing.require(proc.returncode == 0, "native list failed")
            finally:
                drained = tree.kill_members(timeout=10)
                if tree.child is not None:
                    tree.child.wait(timeout=10)
                timing.require(drained, "probe descendants remain")
        lines = log.read_text().splitlines()
        timing.require({line for line in lines if line.endswith(": test")} == {timing.PREFIX + case + ": test" for case in timing.CASES}
                       and "4 tests, 0 benchmarks" in lines, "wrong native filter inventory")
        results.append({"binary": label, "sha256": digest, "argv": argv, "log": timing.pin(log),
                        "seconds": time.monotonic() - started, "exit": proc.returncode, "children_drained": drained})
    timing.write(output / "receipt.json", {"mapping": mapping, "results": results, "storage": bounds(root),
                                           "claim": "four filters listed per binary; no games and no native runtime-input consumer invoked"})


def mapped(root, expected, mode, plan=None, plan_sha=None, selection=None, overhead=None, overhead_sha=None):
    manifest = verify(root, expected)
    api = Namespace()
    identity = api.identity()
    timing.require(identity["sid"] != "S-1-5-18" and not api.query(), "D: already exists or caller is LocalSystem")
    target = "\\??\\" + str(root / "drive-d")
    made = False
    outcome = {"mode": mode, "identity": identity, "target": target, "started_epoch": time.time(), "children_drained": True}
    try:
        api.define(0x9, target)
        made = True
        timing.require(api.query() == [target], "unexpected mapped target")
        timing.write(root / "active-mapping.json", {"identity": identity, "target": target})
        if mode == "probe":
            payload = [sys.executable, "-B", __file__, "_probe", "--root", str(root), "--manifest-sha", expected]
            cores = [manifest["probe_core"]]
        elif mode == "prepare":
            cores = [manifest["probe_core"]]
            payload = [sys.executable, "-B", str(Path(timing.__file__)), "prepare", "--output", str(root / "plan.json"),
                       "--availability", str(root / "availability.json"), "--placement", "computehost", "--remote-root", str(root),
                       "--cores", ",".join(map(str, manifest["worker_cores"])),
                       "--reserved-cores", ",".join(map(str, manifest["reserved_cores"]))]
        else:
            timing.require(plan and plan_sha, "timing plan required")
            p = timing.load_plan(timing.plan_reference(plan, plan_sha))
            cores = p["reserved_cores"]
            payload = [sys.executable, "-B", str(Path(timing.__file__)), "execute", "--plan", plan,
                       "--sha256", plan_sha, "--output", str(root / "results" / mode), "--stage", mode]
            if mode == "compare":
                timing.require(selection, "portable host selection required")
                payload += ["--qualification", str(root / "results/qualify/qualification.json"), "--selection", selection]
            else:
                timing.require(overhead and overhead_sha, "pinned observed overhead required")
                payload += ["--overhead", overhead, "--overhead-sha", overhead_sha]
        command = [sys.executable, "-B", str(Path(__file__).with_name("host_slots_v1.py")),
                   "timed", "--cores", ",".join(map(str, cores)), "--", *payload]
        import host_slots_v1 as slots
        tree = slots.Tree(cores, "below_normal")
        # Ordinary Popen preserves this SSH token's AuthenticationId. No WMI.
        with (root / "results" / (mode + "-console.log")).open("xb") as log:
            proc = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
            outcome["children_drained"] = False
            try:
                timing.require(api.identity(proc.pid) == identity, "child changed logon")
                while proc.poll() is None:
                    bounds(root)
                    waiter_priority(root)
                    timing.require(api.query() == [target], "mapping changed during work")
                    timing.require(time.time() - outcome["started_epoch"] < (180 if mode in ("probe", "prepare") else 14500), "stage deadline")
                    time.sleep(1)
                outcome["exit"] = proc.returncode
                timing.require(proc.returncode == 0, "guarded stage failed")
            finally:
                # Scoped to our containing supervisor's descendants. Parent
                # supervisor waits for these before releasing the reservation.
                outcome["children_drained"] = tree.kill_members(timeout=10)
                proc.wait(timeout=10)
                timing.require(outcome["children_drained"], "mapped children remain")
    finally:
        if made and outcome["children_drained"]:
            api.define(0xf, target)
        outcome.update(mapping_absent=not api.query(), finished_epoch=time.time(), storage=bounds(root))
        timing.write(root / "results" / (mode + "-cleanup.json"), outcome)
        timing.require(outcome["mapping_absent"], "drive cleanup incomplete")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode", choices=("probe", "prepare", "qualify", "compare", "_mapped", "_probe"))
    p.add_argument("--root", required=True)
    p.add_argument("--manifest-sha", required=True)
    p.add_argument("--stage", choices=("probe", "prepare", "qualify", "compare"))
    p.add_argument("--plan")
    p.add_argument("--plan-sha")
    p.add_argument("--selection")
    p.add_argument("--overhead")
    p.add_argument("--overhead-sha")
    args = p.parse_args()
    root = owned_root(args.root)
    manifest = verify(root, args.manifest_sha)
    if args.mode == "_probe":
        probe(root, manifest)
    elif args.mode == "_mapped":
        mapped(root, args.manifest_sha, args.stage, args.plan, args.plan_sha, args.selection, args.overhead, args.overhead_sha)
    else:
        import host_reservation_v1 as host
        timing.require(not os.environ.get(host.TOKEN_ENV), "dedicated SSH supervisor required")
        waiter_priority(root)
        cores = [manifest["probe_core"]] if args.mode in ("probe", "prepare") else manifest["reserved_cores"]
        record = host.acquire("codex-history474-timing", "haley-" + args.mode,
                              "owned descendants drained and exact logon-local D mapping removed",
                              transport_record={"stage_sha256": args.manifest_sha}, cores=cores)
        command = [sys.executable, "-B", __file__, "_mapped", "--stage", args.mode, "--root", str(root),
                   "--manifest-sha", args.manifest_sha]
        for name in ("plan", "plan_sha", "selection", "overhead", "overhead_sha"):
            if getattr(args, name):
                command += ["--" + name.replace("_", "-"), getattr(args, name)]
        raise SystemExit(host.supervise(record["token"], command, cwd=str(Path(__file__).parent)))


if __name__ == "__main__":
    main()
