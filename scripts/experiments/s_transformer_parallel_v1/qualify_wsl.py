"""Hold Haley's Windows reservation through one bounded, owned WSL qualification.

The pinned plan names a fresh distro, rootfs and build script. The operator
releases the build's disk admission phases and publishes qualification-ready
as a pin to the guest qualification script after inspecting the build receipt.
Both scripts run inside the same recorded WSL VM. Guest CPUs are vCPUs.
"""
from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import traceback

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "python" / "tools"))
import host_reservation_v1 as reservations
import host_slots_v1 as slots
import native_expanded_dispatch_v1 as dispatch

ROOT = Path("C:/mtg-node/s-transformer-linux-20261108")
GUEST_ROOT = "/mnt/c/mtg-node/s-transformer-linux-20261108"
LANE = "codex-s-transformer-linux"
GIB = 1024**3
C_FLOOR = 60 * GIB
C_PREEMPT = C_FLOOR + 768 * 1024**2
RAM_FLOOR = 2 * GIB
WALL_SECONDS = 3600
CONFIG = b"[wsl2]\nprocessors=4\nmemory=10GB\nswap=0\n"
CONFIG_SHA = hashlib.sha256(CONFIG).hexdigest()


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def write_json(path, document):
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def physical_free():
    # MEMORYSTATUSEX has two DWORDs followed by seven DWORDLONGs.
    class MemoryStatus(ctypes.Structure):
        _fields_ = [("length", ctypes.c_uint32), ("load", ctypes.c_uint32),
                    ("total", ctypes.c_uint64), ("available", ctypes.c_uint64),
                    ("other", ctypes.c_uint64 * 5)]

    memory = MemoryStatus()
    memory.length = ctypes.sizeof(memory)
    require(ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(memory)),
            "cannot read available Windows RAM")
    return {"c_free_bytes": shutil.disk_usage("C:/").free, "ram_available_bytes": memory.available}


def vm_pids():
    return [pid for pid, _, image in reservations.process_table()
            if image.casefold() in ("vmmemwsl", "vmmemwsl.exe")]


def decode_wsl(data):
    return data.decode("utf-16-le" if b"\0" in data else "utf-8-sig").lstrip("\ufeff")


def guest_path(pin):
    path = dispatch.checked(pin).resolve()
    return f"{GUEST_ROOT}/{path.relative_to(ROOT.resolve()).as_posix()}"


class Guardian:
    def __init__(self, plan):
        self.deadline = time.monotonic() + WALL_SECONDS
        self.distro = plan["distro"]
        require(self.distro.casefold() == "mtg-s-parallel-q001-20261108", "unexpected task-owned WSL distro name")
        self.rootfs = dispatch.checked(plan["rootfs"]).resolve()
        self.build = guest_path(plan["build_script"])
        self.install = ROOT / "distro"
        self.config = Path.home() / ".wslconfig"
        self.wsl = str(Path(os.environ["SystemRoot"]) / "System32" / "wsl.exe")
        self.token = os.environ[reservations.TOKEN_ENV]
        self.vm = None
        self.children = []
        self.config_created = False
        self.import_started = False
        self.imported = False
        self.anchor = None
        self.boot_id = None
        self.last_status = 0.0

    def wsl_command(self, *args, timeout=10):
        result = subprocess.run([self.wsl, *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=timeout, creationflags=subprocess.CREATE_NO_WINDOW)
        require(result.returncode == 0,
                f"wsl {args[0]} exited {result.returncode}: "
                f"{decode_wsl(result.stderr or result.stdout).strip()}")
        return decode_wsl(result.stdout)

    def distros(self, running=False, timeout=10):
        args = ["--list", "--quiet"] + (["--running"] if running else [])
        return {line.strip().casefold() for line in self.wsl_command(*args, timeout=timeout).splitlines()
                if line.strip()}

    def status(self, phase, resources, **detail):
        write_json(ROOT / "guardian-status.json", {
            "phase": phase, "unix_seconds": time.time(), "distro": self.distro,
            "vm_identity": self.vm, "remaining_seconds": max(0, int(self.deadline - time.monotonic())),
            **resources, **detail,
        })

    def check(self, phase):
        now = time.monotonic()
        resources = physical_free()
        sample_status = now - self.last_status >= 5
        require(now < self.deadline, "Windows guardian wall limit reached")
        require(not (ROOT / "STOP").exists(), "task STOP is present")
        require(resources["c_free_bytes"] >= C_PREEMPT, "physical C: reached the preemption threshold")
        require(resources["ram_available_bytes"] >= RAM_FLOOR, "available Windows RAM fell below 2 GiB")
        if self.vm is not None:
            require(vm_pids() == [self.vm[0]] and reservations.process_state(*self.vm) == "alive",
                    "the recorded WSL VM disappeared or changed")
            require(self.anchor.poll() is None, "the WSL idle anchor exited")
            if sample_status:
                require(self.distros(running=True, timeout=1) == {self.distro.casefold()},
                        "the owned WSL distro is no longer the only running distro")
        if sample_status:
            self.status(phase, resources)
            self.last_status = now

    def start(self, phase, *args):
        self.check(phase)
        with (ROOT / f"guardian-{phase}.log").open("xb") as log:
            proc = subprocess.Popen([self.wsl, *args], stdout=log, stderr=subprocess.STDOUT,
                                    stdin=subprocess.DEVNULL, creationflags=subprocess.CREATE_NO_WINDOW)
        self.children.append(proc)
        return proc

    def wait(self, phase, proc):
        while proc.poll() is None:
            self.check(phase)
            time.sleep(1)
        require(proc.returncode == 0, f"{phase} exited {proc.returncode}")

    def preflight(self):
        status = reservations.status(self.token)
        require(status.get("token_fate") == "holds" and status["record"]["lane"] == LANE
                and "cores" not in status["record"], "whole-host Windows reservation required")
        host = slots.status()
        require(host["timed"]["held"] and host["timed"]["cores"] is None
                and not host["claims"] and not host["queue"], "Windows claims or queue are present")
        require(not self.config.exists() and not self.config.is_symlink(), ".wslconfig already exists")
        require(not self.install.exists() and self.distro.casefold() not in self.distros(),
                "distro name and import directory must be fresh")
        require(not (ROOT / "wslconfig-restored.ini").exists(), "restoration receipt must be fresh")
        require(not self.distros(running=True) and not vm_pids(), "a WSL distro or VM is already live")
        require(not (ROOT / "qualification-ready").exists(), "qualification-ready must be created after build")
        self.check("preflight")
        write_json(ROOT / "guardian-before.json", {**physical_free(), "c_floor_bytes": C_FLOOR,
                   "preempt_bytes": C_PREEMPT, "wslconfig": "absent", "guest_vcpus": 4,
                   "guest_memory_gib": 10, "guest_swap_gib": 0})

    def boot(self):
        with self.config.open("xb") as stream:
            self.config_created = True
            stream.write(CONFIG)
            stream.flush()
            os.fsync(stream.fileno())
        proc = self.start("import", "--import", self.distro, str(self.install), str(self.rootfs), "--version", "2")
        self.import_started = True
        self.wait("import", proc)
        self.imported = True
        idle = '''
set -eu
printf 'mtg-s-idle %s %s %s %s %s\\n' "$(cat /proc/sys/kernel/random/boot_id)" \\
  "$(getconf _NPROCESSORS_ONLN)" "$(awk '/^MemTotal:/ { print $2 }' /proc/meminfo)" \\
  "$(awk '/^SwapTotal:/ { print $2 }' /proc/meminfo)" "$(readlink /proc/1/exe)"
exec sleep "$1"
'''
        self.anchor = self.start("idle", "-d", self.distro, "-u", "root", "--exec", "/bin/bash", "-c",
                                 idle, "s-guardian", str(WALL_SECONDS))
        deadline = min(self.deadline, time.monotonic() + 60)
        anchor_log = ROOT / "guardian-idle.log"
        while time.monotonic() < deadline:
            self.check("idle")
            require(self.anchor.poll() is None, "WSL idle rendezvous exited")
            idle_line = re.search(r"^mtg-s-idle ([0-9a-f-]{36}) (\d+) (\d+) (\d+) (\S+)\s*$",
                                  decode_wsl(anchor_log.read_bytes()), re.M)
            pids = vm_pids()
            if idle_line and pids:
                boot, cpus, ram, swap, init = idle_line.groups()
                guest = {"guest_boot_id": boot, "guest_vcpus": int(cpus), "guest_memory_kib": int(ram),
                         "guest_swap_kib": int(swap), "guest_init": init}
                require(len(pids) == 1 and self.distros(running=True) == {self.distro.casefold()},
                        "WSL idle rendezvous is not the only running distro and VM")
                pid = pids[0]
                creation = reservations.creation_time(pid) or (reservations.system_creation_times() or {}).get(pid)
                require(creation and reservations.process_state(pid, creation) == "alive",
                        "cannot read the live WSL VM identity")
                reservations.record_descendant(self.token, pid, creation)
                self.vm, self.boot_id = (pid, creation), boot
                self.check("vm-recorded")
                self.status("vm-recorded", physical_free(), **guest)
                require(cpus == "4" and 9 * GIB <= int(ram) * 1024 <= 10 * GIB
                        and swap == "0" and init == "/init", "WSL guest limits or init differ")
                write_json(ROOT / "guardian-vm.json", {"vm_identity": self.vm, "distro": self.distro, **guest})
                return
            time.sleep(1)
        raise RuntimeError("WSL idle rendezvous timed out before VM registration")

    def run_guest(self, phase, limit, *argv):
        seconds = min(limit, int(self.deadline - time.monotonic()) - 15)
        require(seconds > 0, f"insufficient time remaining for {phase}")
        proc = self.start(phase, "-d", self.distro, "-u", "root", "--exec", "bash", "-c",
                          'read -r boot < /proc/sys/kernel/random/boot_id; [ "$boot" = "$1" ] || exit 80; '
                          'duration=$2; shift 2; unset MTG_HOST_RESERVATION_TOKEN; '
                          'exec timeout -k 15s "$duration" nice -n 10 "$@"',
                          "s-guardian", self.boot_id, f"{seconds}s", *argv)
        self.wait(phase, proc)
        self.check(phase)

    def cleanup(self):
        terminate_attempted = False
        terminated = False
        while True:
            try:
                # Closing wsl.exe prevents late launches. Its exit does not prove the guest exited.
                for proc in self.children:
                    if proc.poll() is None:
                        proc.kill()
                        proc.wait(timeout=10)
                if self.import_started:
                    if self.imported and not terminated and (not terminate_attempted
                            or self.distro.casefold() in self.distros(running=True)):
                        terminate_attempted = True
                        self.wsl_command("--terminate", self.distro, timeout=30)
                        terminated = True
                    running = self.distros(running=True)
                    require(not running and not vm_pids() and
                            (self.vm is None or reservations.process_state(*self.vm) == "absent"),
                            "WSL distro or VM drain is unverified")
                if self.config_created:
                    restored = ROOT / "wslconfig-restored.ini"
                    if self.config.exists():
                        require(self.config.read_bytes() == CONFIG, ".wslconfig bytes changed during the run")
                        self.config.rename(restored)
                    require(not self.config.exists() and not self.config.is_symlink()
                            and restored.read_bytes() == CONFIG,
                            ".wslconfig restoration is unverified")
                resources = physical_free()
                write_json(ROOT / "guardian-cleanup.json", {**resources, "vm_identity": self.vm,
                           "wsl_drained": self.import_started, "wslconfig_restored": self.config_created,
                           "wslconfig_written_sha256": CONFIG_SHA if self.config_created else None})
                self.status("drained", resources)
                return
            except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
                # Keep the guardian alive and the canonical reservation held until verified drain.
                self.status("cleanup-held", physical_free(), error=str(error))
                time.sleep(5)

    def run(self):
        try:
            self.preflight()
            self.boot()
            self.run_guest("build", WALL_SECONDS, "/bin/bash", self.build)
            phase = dispatch.read(ROOT / "phase.json")
            require(phase.get("phase") == "build" and phase.get("state") == "complete"
                    and (ROOT / "build-manifest.json").is_file(), "guest build receipt is incomplete")
            ready = ROOT / "qualification-ready"
            while not ready.is_file():
                self.check("awaiting-qualification")
                time.sleep(1)
            self.run_guest("qualification", 1200, "/usr/local/bin/python3.12", "-B",
                           guest_path(dispatch.read(ready)))
        except BaseException:
            (ROOT / "guardian-failure.log").write_text(traceback.format_exc(), encoding="utf-8")
            raise
        finally:
            self.cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("launch", "_guard"))
    parser.add_argument("plan", type=Path)
    parser.add_argument("--plan-sha256")
    args = parser.parse_args()
    require(os.name == "nt" and reservations.HOST.casefold() == "haleyspc", "Haley Windows host required")
    require(not os.environ.get(reservations.TEST_ROOT_ENV), "a canonical reservation is required")
    path = args.plan.resolve(strict=True)
    require(path.is_relative_to(ROOT.resolve()), "guardian plan must be under the task root")
    pin = dispatch.pin(path)
    if args.action == "_guard":
        require(pin["sha256"] == args.plan_sha256, "guardian plan changed")
        Guardian(dispatch.read(path)).run()
        return
    require(not os.environ.get(reservations.TOKEN_ENV), "a fresh canonical whole-host reservation is required")
    require(not (ROOT / "guardian-launch.json").exists(), "this guardian attempt already exists")
    result = reservations.dispatch(
        lane=LANE, work_id=ROOT.name, release_condition="owned WSL distro and VM drained and configuration restored",
        command=[sys.executable, "-B", str(Path(__file__).resolve()), "_guard", str(path),
                 "--plan-sha256", pin["sha256"]], cwd=str(REPO),
        busy_pattern=dispatch.busy_pattern() + r"|^vmmemwsl(?:\.exe)?$|^wsl(?:host)?\.exe$",
    )
    write_json(ROOT / "guardian-launch.json", result)
    print(json.dumps({k: result[k] for k in ("state", "pid") if k in result}))


if __name__ == "__main__":
    main()
