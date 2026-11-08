"""Build the pinned release on a hosted Linux runner with a private Docker daemon."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import stat
import subprocess
import sys
import time


SOURCE = "c5719e1cf81769f4f30f9e91ae1079d4ee3cbfcf"
IMAGE = "mirror.gcr.io/library/python@sha256:3c3ba8ad4c85eb63b7568f806434f14e33370056985da7fdf2673621cb1d7e93"
GIB = 2**30
FREE_FLOOR = 60 * GIB + GIB // 2
GROWTH = 5 * GIB
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

    def observe(self):
        free = shutil.disk_usage(self.root).free
        ram = mem_available()
        if self.initial_free is None:
            self.initial_free = free
        self.min_free = free if self.min_free is None else min(self.min_free, free)
        self.min_ram = ram if self.min_ram is None else min(self.min_ram, ram)
        self.max_owned = max(self.max_owned, allocated_bytes(self.root))

    def check(self):
        self.observe()
        require(time.monotonic() < self.deadline, "hosted build wall limit reached")
        require(self.min_free >= FREE_FLOOR, "selected volume fell below 60.5 GiB free")
        require(self.max_owned <= GROWTH and self.initial_free - self.min_free <= GROWTH,
                "selected volume growth exceeded 5 GiB")
        require(self.min_ram >= RAM_FLOOR, "host available RAM fell below 1 GiB")
        require(not any((self.root / name).exists() or (self.root / name).is_symlink()
                        for name in ("STOP", "work/STOP")), "hosted build STOP present")

    def pause(self):
        time.sleep(max(0, min(POLL_SECONDS, self.deadline - time.monotonic())))

    def receipt(self):
        return {"initial_free_bytes": self.initial_free, "minimum_free_bytes": self.min_free,
                "minimum_host_available_ram_bytes": self.min_ram, "maximum_owned_allocated_bytes": self.max_owned,
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


def run(root, source, wall):
    resources = Resources(root, wall)
    started = time.monotonic()
    daemon = builder = None
    error = None
    cleanup_errors = []
    code = None
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
        resources.observe()
        require(resources.initial_free >= FREE_FLOOR + GROWTH, "selected volume needs 65.5 GiB free")
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
        builder = start([*docker, "run", "--rm", "--name", CONTAINER, "--network=host", "--platform=linux/amd64",
                         "--cpus=4", "--memory=12g", "--memory-swap=12g", "--sig-proxy=false",
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
        if builder is not None and code != 0:
            if daemon.poll() is None:
                try:
                    with (root / "container.log").open("ab") as output:
                        subprocess.run([*docker, "stop", "--time", "3", CONTAINER], env=env, stdin=subprocess.DEVNULL,
                                       stdout=output, stderr=subprocess.STDOUT, timeout=8, check=True)
                except (OSError, subprocess.SubprocessError) as exc:
                    cleanup_errors.append(f"private container stop: {exc}")
            else:
                cleanup_errors.append("private daemon exited before the owned container stop could be verified")
        try:
            stop_group(daemon)
        except (OSError, ValueError) as exc:
            cleanup_errors.append(str(exc))
        receipt = {"schema": "pinned-release-hosted-build/v1", "pass": error is None and not cleanup_errors,
                   "source_commit": source, "image": IMAGE, "work_root": str(root), "output": str(root / "work/out"),
                   "guard_sha256": guard_sha, "build_helper_sha256": helper_sha,
                   "limits": {"minimum_start_free_bytes": FREE_FLOOR + GROWTH, "free_floor_bytes": FREE_FLOOR,
                              "growth_bytes": GROWTH, "host_available_ram_floor_bytes": RAM_FLOOR,
                              "container_ram_bytes": 12 * GIB, "container_swap_bytes": 0, "container_cpus": 4,
                              "cargo_jobs": 2, "wall_seconds": wall, "resource_poll_seconds": POLL_SECONDS},
                   "network": "host network with private daemon bridge and iptables disabled",
                   "resources": resources.receipt(), "elapsed_seconds": time.monotonic() - started,
                   "build_exit_code": code, "error": error, "cleanup_errors": cleanup_errors}
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
    parser.add_argument("--source-commit", required=True, choices=[SOURCE])
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
