"""Build the pinned release on a hosted Linux runner with a private Docker daemon."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import stat
import subprocess
import sys
import time


SOURCE = "f933c8b8dfc60945eff683b5125d694231109417"
# The w128 backport before its wide-state I/O fix, built only for the fix's equality check.
PRE_FIX_SOURCE = "411feed252dc842fcbf647c106dab16fe739305b"
SOURCES = (SOURCE, PRE_FIX_SOURCE)
IMAGE = "mirror.gcr.io/library/python@sha256:3c3ba8ad4c85eb63b7568f806434f14e33370056985da7fdf2673621cb1d7e93"
GIB = 2**30
FREE_FLOOR = 60 * GIB + GIB // 2
GROWTH = 5 * GIB
SWAP = 16 * GIB
TOTAL_GROWTH = GROWTH + SWAP
RAM_FLOOR = GIB
MAX_WALL = 6600
POLL_SECONDS = 5
CONTAINER = "mtg-pinned-release-build"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def mount_path(path):
    require(not any(ch in str(path) for ch in ',"\r\n'), f"unsupported Docker mount path: {path}")
    return path


def create_root(path):
    require(path.is_absolute() and path.name not in ("", ".."), "work root must be an absolute fresh path")
    require(path.parent.resolve(strict=True) == path.parent and path.parent.is_dir(),
            "work root needs an existing canonical parent directory")
    mount_path(path)
    require(len(os.fsencode(path / "exec/containerd/containerd.sock")) < 100, "work root is too long for Docker sockets")
    path.mkdir(mode=0o755)


def mem_available():
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    raise ValueError("host MemAvailable is unavailable")


def active_swaps():
    swaps = {}
    for line in Path("/proc/swaps").read_text().splitlines()[1:]:
        name, kind, size, used, priority = line.rsplit(None, 4)
        name = re.sub(r"\\([0-7]{3})", lambda m: chr(int(m[1], 8)), name)
        swaps[name] = {"type": kind, "size_bytes": int(size) * 1024, "used_bytes": int(used) * 1024,
                       "priority": int(priority)}
    return swaps


def file_blocks(path):
    try:
        return path.stat(follow_symlinks=False).st_blocks * 512
    except FileNotFoundError:
        return 0


def allocated_bytes(root):
    """Count owned on-volume allocations once, skipping Docker's live overlay mounts."""
    device = root.stat().st_dev
    pending = [root]
    seen = set()
    total = 0
    while pending:
        path = pending.pop()
        try:
            meta = path.stat(follow_symlinks=False)
            key = (meta.st_dev, meta.st_ino)
            if meta.st_dev != device or key in seen:
                continue
            seen.add(key)
            total += meta.st_blocks * 512
            if stat.S_ISDIR(meta.st_mode):
                pending.extend(path.iterdir())
        except FileNotFoundError:
            continue  # Docker or Cargo removed a temporary path during the scan.
    return total


class Resources:
    def __init__(self, root, wall):
        self.root = root
        self.deadline = time.monotonic() + wall
        self.initial_free = None
        self.min_free = None
        self.min_ram = None
        self.max_owned = 0
        self.max_build = 0

    def observe(self):
        free = shutil.disk_usage(self.root).free
        ram = mem_available()
        if self.initial_free is None:
            self.initial_free = free
        self.min_free = free if self.min_free is None else min(self.min_free, free)
        self.min_ram = ram if self.min_ram is None else min(self.min_ram, ram)
        owned = allocated_bytes(self.root)
        self.max_owned = max(self.max_owned, owned)
        self.max_build = max(self.max_build, owned - min(SWAP, file_blocks(self.root / "swapfile")))

    def check(self):
        self.observe()
        require(time.monotonic() < self.deadline, "hosted build wall limit reached")
        require(self.min_free >= FREE_FLOOR, "selected volume fell below 60.5 GiB free")
        require(self.max_build <= GROWTH, "owned build growth exceeded 5 GiB excluding the swapfile")
        require(self.max_owned <= TOTAL_GROWTH and self.initial_free - self.min_free <= TOTAL_GROWTH,
                "selected volume growth exceeded 21 GiB including the swapfile")
        require(self.min_ram >= RAM_FLOOR, "host available RAM fell below 1 GiB")
        require(not any((self.root / name).exists() or (self.root / name).is_symlink()
                        for name in ("STOP", "work/STOP")), "hosted build STOP present")

    def pause(self):
        time.sleep(max(0, min(POLL_SECONDS, self.deadline - time.monotonic())))

    def receipt(self):
        return {"initial_free_bytes": self.initial_free, "minimum_free_bytes": self.min_free,
                "minimum_host_available_ram_bytes": self.min_ram, "maximum_owned_allocated_bytes": self.max_owned,
                "maximum_owned_build_allocated_bytes": self.max_build,
                "maximum_free_space_decline_bytes": (None if self.initial_free is None else
                                                      self.initial_free - self.min_free)}


def start(command, log, env):
    with log.open("xb") as output:
        return subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=output, stderr=subprocess.STDOUT,
                                start_new_session=True, env=env)


def stop_group(proc):
    if proc is None or proc.poll() is not None:
        return
    for sig, wait in ((signal.SIGTERM, 5), (signal.SIGKILL, 5)):
        try:
            os.killpg(proc.pid, sig)
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=wait)
            return
        except subprocess.TimeoutExpired:
            continue
    raise ValueError(f"owned process group {proc.pid} did not stop")


def executable(name):
    path = shutil.which(name)
    require(path is not None and Path(path).is_absolute() and Path(path).is_file(), f"missing executable: {name}")
    return str(Path(path).resolve(strict=True))


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def swap_command(command, path, log):
    with log.open("ab") as output:
        subprocess.run([executable(command[0]), *command[1:], "--", str(path)], stdin=subprocess.DEVNULL,
                       stdout=output, stderr=subprocess.STDOUT, timeout=30, check=True)


def container_command(docker, env, log, *args, allow_missing=False):
    result = subprocess.run([*docker, "container", *args, CONTAINER], env=env, stdin=subprocess.DEVNULL,
                            capture_output=True, timeout=8)
    with log.open("ab") as output:
        output.write(result.stdout + result.stderr)
    missing_messages = (b"No such container:", b"No such object:")
    if allow_missing and result.returncode and any(message in result.stderr for message in missing_messages):
        return None
    result.check_returncode()
    return result.stdout


def remove_container(docker, env, log, receipt):
    result = container_command(docker, env, log, "inspect", "--format", "{{json .State}}", allow_missing=True)
    receipt["before_cleanup"] = state = None if result is None else json.loads(result)
    if state is None:
        return
    if state["Running"]:
        container_command(docker, env, log, "stop", "--time", "3")
        result = container_command(docker, env, log, "inspect", "--format", "{{json .State}}", allow_missing=True)
        receipt["after_stop"] = state = None if result is None else json.loads(result)
    if state is not None:
        require(not state["Running"], "owned build container did not stop")
        container_command(docker, env, log, "rm", allow_missing=True)


def run(root, source, wall):
    resources = Resources(root, wall)
    started = time.monotonic()
    daemon = builder = None
    error = None
    cleanup_errors = []
    code = None
    swapfile = root / "swapfile"
    swap_log = root / "swap.log"
    swap_created = False
    swap = {"path": str(swapfile), "original": None, "enabled": None, "after_cleanup": None}
    container = {}
    guard = Path(__file__).resolve()
    guard_sha = sha256(guard)
    helper = guard.with_name("build.sh")
    helper_sha = None
    socket = root / "docker.sock"
    host = f"unix://{socket}"
    env = {key: value for key, value in os.environ.items() if not key.startswith("DOCKER_")}
    try:
        mount_path(helper)
        require(helper.is_file() and not helper.is_symlink(), "pinned build helper must be a regular file")
        helper_sha = sha256(helper)
        swap["original"] = active_swaps()
        resources.observe()
        require(resources.initial_free >= FREE_FLOOR + TOTAL_GROWTH, "selected volume needs 81.5 GiB free")
        resources.check()
        require(str(swapfile) not in swap["original"], "owned swap path already active")
        fd = os.open(swapfile, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        swap_created = True
        os.close(fd)
        swap_command(["fallocate", "-l", str(SWAP)], swapfile, swap_log)
        require(swapfile.stat().st_size == SWAP and file_blocks(swapfile) >= SWAP, "swapfile is not fully allocated")
        resources.check()
        swap_command(["mkswap"], swapfile, swap_log)
        swap_command(["swapon"], swapfile, swap_log)
        swap["enabled"] = active_swaps().get(str(swapfile))
        require(swap["enabled"] is not None and swap["enabled"]["type"] == "file"
                and SWAP - 2**20 <= swap["enabled"]["size_bytes"] <= SWAP,
                "owned 16 GiB swapfile was not enabled")
        resources.check()
        work, temporary, client_config = root / "work", root / "tmp", root / "client"
        for path in (work, temporary, client_config):
            path.mkdir()
        env.update(TMPDIR=str(temporary), DOCKER_TMPDIR=str(temporary))
        config = root / "daemon.json"
        config.write_text("{}\n", encoding="ascii")
        docker = [executable("docker"), "--config", str(client_config), "--host", host]
        daemon = start([executable("dockerd"), "--config-file", str(config), "--host", host,
                        "--data-root", str(root / "data"), "--exec-root", str(root / "exec"),
                        "--pidfile", str(root / "dockerd.pid"), "--bridge=none", "--iptables=false",
                        "--ip6tables=false", "--ip-forward=false", "--ip-masq=false", "--live-restore=false"],
                       root / "daemon.log", env)
        while not socket.is_socket():
            resources.check()
            require(daemon.poll() is None, f"private Docker daemon exited {daemon.returncode}")
            resources.pause()
        resources.check()
        builder = start([*docker, "run", "--name", CONTAINER, "--network=host", "--platform=linux/amd64",
                         "--cpus=4", "--memory=12g", "--memory-swap=28g", "--sig-proxy=false",
                         "--mount", f"type=bind,src={work},dst=/work",
                         "--mount", f"type=bind,src={helper},dst=/build.sh,readonly",
                         "--entrypoint", "/bin/bash", IMAGE, "/build.sh", source, "/work/out"],
                        root / "container.log", env)
        while builder.poll() is None:
            resources.check()
            require(daemon.poll() is None, f"private Docker daemon exited {daemon.returncode}")
            resources.pause()
        code = builder.returncode
        resources.check()
        require(code == 0, f"build container exited {code}")
        require((work / "out/DONE").is_file(), "build container returned without its completion marker")
        require(sha256(helper) == helper_sha, "pinned build helper changed during the build")
    except BaseException as exc:
        error = str(exc) or type(exc).__name__
    finally:
        try:
            stop_group(builder)
        except (OSError, ValueError) as exc:
            cleanup_errors.append(str(exc))
        if builder is not None:
            if daemon.poll() is None:
                try:
                    remove_container(docker, env, root / "container.log", container)
                except (OSError, ValueError, subprocess.SubprocessError) as exc:
                    cleanup_errors.append(f"private container cleanup: {exc}")
            else:
                cleanup_errors.append("private daemon exited before the owned container stop could be verified")
        try:
            stop_group(daemon)
        except (OSError, ValueError) as exc:
            cleanup_errors.append(str(exc))
        if swap_created:
            try:
                # The path was exclusively created in this attempt. Never disable a pre-existing swap.
                if str(swapfile) in active_swaps():
                    require(daemon is None or daemon.poll() is not None, "private daemon still running before swapoff")
                    swap_command(["swapoff"], swapfile, swap_log)
            except (OSError, ValueError, subprocess.SubprocessError) as exc:
                cleanup_errors.append(f"owned swapoff: {exc}")
            try:
                swap["after_cleanup"] = active_swaps()
                require(str(swapfile) not in swap["after_cleanup"], "owned swapfile is still active")
                require(swap["after_cleanup"].keys() == swap["original"].keys(), "host swap inventory changed")
                for name, original in swap["original"].items():
                    final = swap["after_cleanup"][name]
                    require(all(original[key] == final[key] for key in ("type", "size_bytes", "priority")),
                            f"pre-existing swap changed: {name}")
            except (OSError, ValueError, subprocess.SubprocessError) as exc:
                cleanup_errors.append(f"owned swap verification: {exc}")
        receipt = {"schema": "pinned-release-hosted-build/v1", "pass": error is None and not cleanup_errors,
                   "source_commit": source, "image": IMAGE, "work_root": str(root), "output": str(root / "work/out"),
                   "guard_sha256": guard_sha, "build_helper_sha256": helper_sha,
                   "limits": {"minimum_start_free_bytes": FREE_FLOOR + TOTAL_GROWTH, "free_floor_bytes": FREE_FLOOR,
                              "growth_bytes": TOTAL_GROWTH, "build_growth_bytes": GROWTH, "owned_swap_bytes": SWAP,
                              "host_available_ram_floor_bytes": RAM_FLOOR,
                              "container_ram_bytes": 12 * GIB, "container_swap_bytes": SWAP, "container_cpus": 4,
                              "cargo_jobs": 2, "wall_seconds": wall, "resource_poll_seconds": POLL_SECONDS},
                   "network": "host network with private daemon bridge and iptables disabled",
                   "resources": resources.receipt(), "elapsed_seconds": time.monotonic() - started,
                   "build_exit_code": code, "container": container, "swap": swap,
                   "error": error, "cleanup_errors": cleanup_errors}
        with (root / "guard.json.tmp").open("x", encoding="utf-8") as output:
            output.write(json.dumps(receipt, indent=2) + "\n")
            output.flush()
            os.fsync(output.fileno())
        (root / "guard.json.tmp").replace(root / "guard.json")
    return receipt


def interrupted(_signum, _frame):
    raise KeyboardInterrupt("hosted build interrupted")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-root", required=True, type=Path)
    parser.add_argument("--source-commit", required=True, choices=list(SOURCES))
    parser.add_argument("--wall-seconds", type=int, default=MAX_WALL)
    args = parser.parse_args(argv)
    try:
        require(0 < args.wall_seconds <= MAX_WALL, f"wall limit must be 1 to {MAX_WALL} seconds")
        require(sys.platform.startswith("linux") and platform.machine() == "x86_64" and os.geteuid() == 0,
                "run the hosted guard as root on Linux x86-64")
        os.umask(0o022)
        create_root(args.work_root)
    except (OSError, ValueError) as exc:
        parser.exit(2, f"{exc}\n")
    signal.signal(signal.SIGTERM, interrupted)
    receipt = run(args.work_root, args.source_commit, args.wall_seconds)
    print(json.dumps({"pass": receipt["pass"], "receipt": str(args.work_root / "guard.json"), "error": receipt["error"]}))
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
