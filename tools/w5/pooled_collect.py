#!/usr/bin/env python3
"""Pooled collection dispatcher (pooled-lineage stage 1).

The trainer runs this as its external collector:

    native_expanded_training_run_v1 CONFIG.json --collect-external /abs/pooled_collect.py

with MTG_POOLED_COLLECT_CONFIG naming a hosts file. For each iteration the
trainer publishes `collect-command.json` and calls this program with its path.
The program:

1. splits the command's episodes into contiguous shards, one per host, sized by
   host weight (the shards `pooled_collection_v1::shard_commands_v1` makes);
2. makes every pinned input of the shard commands (the current checkpoint, the
   play imports, opponents) present at the same absolute path with the same
   SHA-256 on every collector host; that is how the policy reaches collectors;
3. runs each shard with the host's pinned trainer (`--execute-collect-shard`);
4. copies each shard's output back into a staging directory beside the attempt;
5. on a failed or unreachable host, reruns that shard on the next host (a
   shard's bytes do not depend on where it runs);
6. on audit iterations, replays chosen slots on a host of another architecture
   class and requires identical trajectory bytes;
7. merges the shards into the attempt's `collect/` (the trainer's
   `--merge-collection-shards`), which re-hashes every trajectory and applies the non-natural cap.

The trainer then validates the merged collection exactly as a local one and
replays every decision in the update. Nothing here changes a result: a
failure exits non-zero and the trainer's attempt fails closed.

Paths are one namespace across hosts: a trajectory records the absolute paths
of its sources, so every collector must see the recipe's inputs at the paths
the learner uses (a common root such as /srv/work/pooled-20261120 on every
Linux host, WSL guest and the Mac's Linux VM).

Two more entry points run on collector hosts:

    pooled_collect.py agent AGENT_CONFIG.json    the forced command of the learner's restricted key: serves
                                                 one confined request (seven operations) per connection
    pooled_collect.py runner AGENT_CONFIG.json   the box runner: started by root inside a W5 box as the
                                                 collector account, it runs the collectors the agent hands
                                                 it through `<scratch>/.spool`, so they use the box's CPUs
    pooled_collect.py runner-check AGENT_CONFIG.json [MAX_AGE_SECONDS]
                                                 a diagnostic: exit 0 fresh, 1 stale with a runner holding the
                                                 spool, 3 nothing to judge, 6 unknown (not for timers)
    pooled_collect.py runner-watch AGENT_CONFIG.json --plan P --chain C --cgroup DIR [--max-age S]
                                                 root's timer: exit 1 (and the w5box kill to run) only when two
                                                 readings find a held, stale, unfrozen runner; acts on nothing

With the agent config's `"executor": {"kind": "spool"}` the agent's run_shard keeps every check it makes and
hands the run to the runner instead of starting the collector itself (a process started by sshd cannot leave
its session scope for a box). Without `executor` (or with {"kind": "direct"}) nothing changes. The collector,
its argv, working directory, inputs and sandbox are the same either way, so a shard's bytes do not depend on
the executor. The runner stops on its stop file (default `<scratch>/.spool/STOP`) and writes an end record
(default `<scratch>/.spool/END.json`); see the box runner section below.

Connection reuse (no code): an ssh host's `ssh` list is passed to every call unchanged, so OpenSSH connection
multiplexing is configured there, for example

    "ssh": ["ssh", "-o", "BatchMode=yes", "-o", "ControlMaster=auto",
            "-o", "ControlPath=/srv/work/RUN/.ssh-mux/%C", "-o", "ControlPersist=120", "mtg@node1"]

with the ControlPath directory owned by the learner account, mode 700. Each call is still one session and one
request (the agent's rule); only the TCP connection and key exchange are shared (measured node2 to node1:
0.27 s per agent call without, 0.044 s with). The first call starts the master in the background; OpenSSH
points the master's standard streams at /dev/null unless `-v` is given, so never put `-v` in these arguments
(a master holding the call's stderr would hold that call open to its deadline). A master that dies fails the
calls riding on it like any connection failure (the host fails over); the next call starts a new master.
The collector's sshd allows 10 sessions per connection by default (MaxSessions); the dispatcher has at most
a few requests in flight per host (a shard, its cleanup, an audit, a failover shard).

Resident mode: `pooled_collect.py serve SOCKET_DIR` keeps one dispatcher up for a run, and the program the
trainer starts per update hands its command to it (MTG_POOLED_COLLECT_RESIDENT=SOCKET_DIR); see the resident
section at the end.
"""
from __future__ import annotations

import base64
import hashlib
import io
import json
import math
import os
import re
import shlex
import shutil
import signal
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import types
import uuid
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath, PureWindowsPath

SCHEMA = "mtg-kernel-pooled-collect-hosts/v1"
FETCH_SECONDS = 900.0
LOADED_AT = time.time()  # this module's load, for the receipt's start-up timing


@dataclass
class Host:
    name: str
    kind: str  # "local" or "ssh"
    collector: str  # absolute path of the pinned trainer on that host
    collector_sha256: str
    workers: int
    weight: float
    scratch: str  # absolute scratch root on that host
    arch: str  # architecture class, for audits: e.g. "x86-znver5", "x86-portable", "arm64"
    ssh: list = field(default_factory=list)  # e.g. ["ssh", "-o", "BatchMode=yes", "mtg@10.0.0.37"]
    prefix: list = field(default_factory=list)  # e.g. ["nice", "-n", "19", "taskset", "-c", "1,17"]
    # Opt-in remote retention: delete inputs this dispatcher pushed for an earlier iteration (old
    # checkpoints) once a later collection merges, only while they still hold the bytes it pushed.
    # Off by default: an absolute pin can still serve another run or fork on that host (a pruned
    # pin is pushed again when next needed), so enable it only for hosts that serve one lineage.
    # Agent transport only: the agent links the ownership witness and removes owned files under one
    # per-path lock in one process; a shell-transport host asking for it is refused.
    prune_pushed: bool = False
    # Format string around each remote shell command, e.g. "docker exec -i bladder-node sh -c {cmd}".
    remote_wrap: str = ""
    # "shell" (proof tooling: ordinary logins) or "agent" (standing access: a forced command runs
    # `pooled_collect.py agent`, which serves one confined request per connection).
    transport: str = "shell"
    # Agent mode over an ordinary login (proofs only): format string with {req}, see AgentTransport.
    agent_wrap: str = ""


def _absolute(path: str) -> bool:
    return PurePosixPath(path).is_absolute() or PureWindowsPath(path).is_absolute()


def load_hosts(path: Path) -> dict:
    document = json.loads(Path(path).read_text())
    if document.get("schema") != SCHEMA:
        raise ValueError(f"hosts file schema is not {SCHEMA}")
    hosts = []
    names = set()
    for entry in document["hosts"]:
        host = Host(**entry)
        if host.kind not in ("local", "ssh"):
            raise ValueError(f"{host.name}: kind must be local or ssh")
        if host.kind == "ssh" and not host.ssh:
            raise ValueError(f"{host.name}: an ssh host needs its ssh command")
        if host.transport not in ("shell", "agent") or (host.transport == "agent" and host.kind != "ssh"):
            raise ValueError(f"{host.name}: transport is shell, or agent for an ssh host")
        if host.prune_pushed and host.transport != "agent":
            raise ValueError(f"{host.name}: remote retention (prune_pushed) needs the agent transport")
        if host.workers < 2 or host.weight <= 0:
            raise ValueError(f"{host.name}: workers >= 2 and weight > 0 required")
        if not (_absolute(host.collector) and _absolute(host.scratch)):
            raise ValueError(f"{host.name}: collector and scratch must be absolute")
        if host.name in names:
            raise ValueError(f"duplicate host {host.name}")
        names.add(host.name)
        hosts.append(host)
    if not hosts:
        raise ValueError("no hosts")
    document["hosts"] = hosts
    document.setdefault("audit", {"every": 0, "episodes": 0})
    return document


def plan_sizes(total: int, weights: list[float]) -> list[int]:
    """Largest-remainder split of `total` episodes by weight; hosts keep their order.
    The split changes only where games run, never their bytes."""
    if total < 1 or not weights:
        raise ValueError("nothing to split")
    whole = sum(weights)
    quotas = [total * w / whole for w in weights]
    sizes = [int(q) for q in quotas]
    order = sorted(range(len(weights)), key=lambda i: (-(quotas[i] - sizes[i]), i))
    for i in order[: total - sum(sizes)]:
        sizes[i] += 1
    return sizes


PIN_TREE_DEPTH = 128  # far beyond any command or record, far below Python's recursion limit
PIN_TREE_NODES = 1 << 22  # a 16 MiB shard command holds fewer values


class PinTreeTooLarge(ValueError):
    """A document nested deeper or holding more values than any command does."""


def pins_of(value, max_depth: int = PIN_TREE_DEPTH, max_nodes: int = PIN_TREE_NODES) -> set[tuple[str, str]]:
    """Every {path, sha256} pin inside a command document. Iterative with explicit bounds, so a
    hostile document is refused (PinTreeTooLarge) instead of exhausting the stack."""
    found, stack, nodes = set(), [(value, 0)], 0
    while stack:
        item, depth = stack.pop()
        nodes += 1
        if depth > max_depth or nodes > max_nodes:
            raise PinTreeTooLarge(f"document nests deeper than {max_depth} or holds more than {max_nodes} values")
        if isinstance(item, dict):
            if set(item) >= {"path", "sha256"} and isinstance(item.get("path"), str) \
                    and isinstance(item.get("sha256"), str):
                found.add((item["path"], item["sha256"]))
            stack.extend((child, depth + 1) for child in item.values())
        elif isinstance(item, list):
            stack.extend((child, depth + 1) for child in item)
    return found


NESTED_PIN_BYTES = 1 << 20


IMPORT_FILES = {  # FrozenPlayPolicyImportV1 (sideboard_play_policy_v1::load_v1): files it reads
    "export_directory": ("metadata.json", "parameters.f32le"),
}
IMPORT_PATHS = ("source_run_path", "source_registry_path")


def import_pins(document) -> set[tuple[str, str]]:
    """The files an ordinary play import (`export_directory`, `source_run_path`,
    `source_registry_path`) makes the loader read, pinned by the learner's own bytes.
    The loader then checks them against the import's expected hashes on the collector."""
    if not isinstance(document, dict) or not {"export_directory", *IMPORT_PATHS} <= set(document):
        return set()
    root = str(document["export_directory"]).rstrip("/\\")
    names = [f"{root}/{name}" for name in IMPORT_FILES["export_directory"]]  # as the loader joins them
    names += [str(document[key]) for key in IMPORT_PATHS]
    found = set()
    for name in names:
        if not Path(name).is_file():
            raise RuntimeError(f"learner lacks an import file: {name}")
        found.add((name, sha256_file_known(Path(name))))
    return found


def input_closure(command: dict) -> set[tuple[str, str]]:
    """The command's pins plus the pins inside small pinned JSON files (a play import names its
    initialization and parameter files). Checkpoints are larger and name only history, never read.
    A nested pin counts only when the learner holds exactly its bytes: records also carry provenance
    pins (the interpreter that produced a file, another host's paths) that no collector opens, and
    the learner, which reads every input it needs, would have failed on a missing one."""
    top = set(pins_of(command))
    found, queue = set(), list(top)
    while queue:
        pin = queue.pop()
        if pin in found:
            continue
        path = Path(pin[0])
        if pin not in top and not (path.is_file() and sha256_file_known(path) == pin[1]):
            continue
        found.add(pin)
        try:
            if path.is_file() and path.stat().st_size <= NESTED_PIN_BYTES:
                document = json.loads(path.read_bytes())
                queue.extend(pins_of(document))
                for pin_ in import_pins(document):
                    top.add(pin_)  # required: the loader reads it
                    queue.append(pin_)
        except (ValueError, UnicodeDecodeError):
            pass  # not JSON: a leaf
    return found


# Remote retention on agent hosts: the ownership witness and the storage generation marker, used
# in-process by the collector agent (agent_serve). A pin an agent push creates gets a witness, a
# second hard link to the very inode it installed, `<scratch>/.pooled-owned/<sha256 of the path>`,
# linked from the verified stage before publication and renamed into place after it, under the
# path's lock, so it can only ever name the installed inode. remove_pin takes the same lock.

def owned_witness(dest, scratch):
    """The ownership witness of the pin at `dest`."""
    return os.path.join(scratch, ".pooled-owned", hashlib.sha256(dest.encode()).hexdigest())


def remove_owned_file(dest, scratch, expected):
    """Deletes `dest` only if it is still the witnessed file (device and inode) with the expected
    bytes: it is first renamed aside into the witness directory (atomic), checked there, and only then
    deleted; anything else is put back. Returns "removed" or "not-owned"."""
    own = os.path.join(scratch, ".pooled-owned")
    witness = owned_witness(dest, scratch)

    def ident(path):
        try:
            info = os.lstat(path)
        except FileNotFoundError:
            return None
        return (info.st_dev, info.st_ino) if stat.S_ISREG(info.st_mode) else "other"

    def digest(path):
        h = hashlib.sha256()
        with open(path, "rb") as handle:
            for block in iter(lambda: handle.read(1 << 20), b""):
                h.update(block)
        return h.hexdigest()

    mine = ident(witness)
    if mine is None or mine == "other":
        return "not-owned"
    if ident(dest) != mine:
        os.unlink(witness)
        return "not-owned"
    aside = os.path.join(own, "aside-" + uuid.uuid4().hex)
    os.rename(dest, aside)
    if ident(aside) == mine and digest(aside) == expected:
        os.unlink(aside)
        os.unlink(witness)
        return "removed"
    try:
        os.link(aside, dest)
        os.unlink(aside)
    except FileExistsError:
        os.rename(aside, os.path.join(own, "kept-" + uuid.uuid4().hex))
    return "not-owned"


def storage_marker(scratch):
    """The collector storage's generation marker, `<scratch>/.pooled-storage-id`: a random value
    written completely into a private file, then linked into place without replacement, so a reader
    never sees a partial marker; the loser of a first-initialization race reads the winner's."""
    path = os.path.join(scratch, ".pooled-storage-id")
    flags = getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_BINARY", 0)
    if not os.path.lexists(path):
        os.makedirs(scratch, exist_ok=True)
        stage = "%s.%s.stage" % (path, uuid.uuid4().hex)
        fd = os.open(stage, os.O_WRONLY | os.O_CREAT | os.O_EXCL | flags, 0o644)
        try:
            with os.fdopen(fd, "wb") as handle:
                handle.write(uuid.uuid4().hex.encode())
                handle.flush()
                os.fsync(handle.fileno())
            try:
                os.link(stage, path)
            except FileExistsError:
                pass
        finally:
            os.unlink(stage)
    fd = os.open(path, os.O_RDONLY | flags)
    with os.fdopen(fd, "rb") as handle:
        return handle.read(64).decode(errors="replace").strip()


PUSH_PIN = """import hashlib,os,shutil,sys
from pathlib import Path
dest,stage,expected=Path(sys.argv[1]),Path(sys.argv[2]),sys.argv[3]
def digest(path):
  h=hashlib.sha256()
  with path.open('rb') as f:
    for block in iter(lambda: f.read(1 << 20), b''): h.update(block)
  return h.hexdigest()
dest.parent.mkdir(parents=True,exist_ok=True)
f=stage.open('xb')
try:
  with f: shutil.copyfileobj(sys.stdin.buffer,f)
  if digest(stage)!=expected: sys.exit('staged pin differs: '+str(dest))
  try:
    os.link(stage,dest)
  except FileExistsError:
    if not dest.is_file() or digest(dest)!=expected:
      sys.exit('existing pin differs: '+str(dest))
    print('existing')
  else:
    print('created')
finally:
  stage.unlink(missing_ok=True)
"""


class state_lock:
    """An exclusive lock beside the retention state file, held across its whole
    read, prune and publish, so concurrent dispatchers sharing it never lose records."""

    def __init__(self, state_path: Path):
        self.path = Path(f"{state_path}.lock")

    def __enter__(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.handle = open(self.path, "a+b")
        if os.name == "nt":
            import msvcrt
            while True:
                try:
                    self.handle.seek(0)
                    msvcrt.locking(self.handle.fileno(), msvcrt.LK_LOCK, 1)
                    break
                except OSError:
                    time.sleep(0.05)
        else:
            import fcntl
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX)
        return self

    def __exit__(self, *exc):
        if os.name == "nt":
            import msvcrt
            self.handle.seek(0)
            msvcrt.locking(self.handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_UN)
        self.handle.close()
        return False


def preflight(whole: dict) -> None:
    """The ordinary collector's whole-command checks, before any staging or launch (the merge
    checks them again): a whole collect_parallel command, 1 to 1024 episodes with distinct ids,
    2 to 1024 requested workers, no EX1."""
    if whole.get("mode") != "collect_parallel" or whole.get("shard") is not None:
        raise ValueError("expected a whole collect_parallel command")
    episodes, workers = whole.get("episodes"), whole.get("workers")
    if not isinstance(episodes, list) or not 1 <= len(episodes) <= 1024:
        raise ValueError("invalid collection size")
    ids = [e.get("id") for e in episodes if isinstance(e, dict)]
    if len(ids) != len(episodes) or len(set(ids)) != len(ids):
        raise ValueError("duplicate episode id")
    if not isinstance(workers, int) or isinstance(workers, bool) or not 2 <= workers <= 1024:
        raise ValueError("collection workers must be in 2..=1024")
    if whole.get("ex1") is not None:
        raise ValueError("EX1 collection is not available pooled")


def iteration_of(command_path: Path) -> int:
    """.../iterations/000123/attempt-000000/collect-command.json -> 123, read from the trailing
    structure (an ancestor directory may also be called `iterations`)."""
    parts = Path(command_path).parts
    if len(parts) >= 4 and parts[-4] == "iterations" and parts[-3].isdigit() \
            and parts[-2].startswith("attempt-") and parts[-1] == "collect-command.json":
        return int(parts[-3])
    raise ValueError(f"not an iteration's collect command: {command_path}")


def audit_slots(iteration: int, episodes: int, count: int, every: int) -> list[int]:
    """Slots replayed on another architecture: on iteration 0 and every `every`-th one,
    `count` slots spread by a fixed stride. Deterministic, never outcome-chosen."""
    if every <= 0 or count <= 0 or iteration % every != 0:
        return []
    count = min(count, episodes)
    offset = (iteration * 7919) % episodes
    return sorted({(offset + k * episodes // count) % episodes for k in range(count)})


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


_HASHED: dict = {}  # path -> (file signature, sha256): learner-side files hashed by this process
_HASHED_LOCK = threading.Lock()


def _signature(path) -> tuple:
    info = os.stat(path)
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def sha256_file_known(path: Path) -> str:
    """sha256_file of a file on this machine, rehashed only when its signature (device, inode, size,
    modification and change times) differs from the last hash this process took. For the learner's
    own inputs and a local collector: every byte they send is verified again where it lands (the
    push's staged digest, the agent's checks), so the shortcut never admits other bytes there."""
    key = os.path.abspath(path)
    before = _signature(key)
    with _HASHED_LOCK:
        known = _HASHED.get(key)
    if known is not None and known[0] == before:
        return known[1]
    digest = sha256_file(Path(key))
    if _signature(key) == before:  # unchanged while hashed
        with _HASHED_LOCK:
            if len(_HASHED) >= 4096:  # a long resident run names a new checkpoint every update
                _HASHED.clear()
            _HASHED[key] = (before, digest)
    return digest


class Transport:
    """Runs commands and moves files for one host. Tests replace it."""

    def __init__(self, host: Host):
        self.host = host

    def _ssh(self, remote: list[str]) -> list[str]:
        """The ssh argv for one remote shell command. `remote_wrap` (for example
        "docker exec -i NAME sh -c {cmd}") runs it inside a container or VM on that host."""
        (command,) = remote
        if self.host.remote_wrap:
            command = self.host.remote_wrap.format(cmd=shlex.quote(command))
        return self.host.ssh + [command]

    def run(self, argv: list[str], timeout: float) -> subprocess.CompletedProcess:
        if self.host.kind == "local":
            return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        remote = " ".join(shlex.quote(a) for a in argv)
        return subprocess.run(self._ssh([remote]), capture_output=True, text=True,
                              timeout=timeout, stdin=subprocess.DEVNULL)

    def pin_status(self, pins: set[tuple[str, str]]) -> dict:
        """Each pin's state on the host: "ok", "absent" or "differs" (a file with other bytes)."""
        if self.host.kind == "local":
            return {p: ("absent" if not os.path.lexists(p[0]) else
                        "ok" if Path(p[0]).is_file() and sha256_file_known(Path(p[0])) == p[1] else "differs")
                    for p in pins}
        script = ("import hashlib,json,os,sys\n"
                  "out=[]\n"
                  "for p,s in json.load(sys.stdin):\n"
                  "  if not os.path.lexists(p):\n"
                  "    out.append('absent'); continue\n"
                  "  if not os.path.isfile(p):\n"
                  "    out.append('differs'); continue\n"
                  "  h=hashlib.sha256()\n"
                  "  with open(p,'rb') as f:\n"
                  "    for chunk in iter(lambda: f.read(1 << 20), b''): h.update(chunk)\n"
                  "  out.append('ok' if h.hexdigest()==s else 'differs')\n"
                  "print(json.dumps(out))\n")
        ordered = sorted(pins)
        result = subprocess.run(self._ssh(["python3 -c " + shlex.quote(script)]),
                                input=json.dumps(ordered), capture_output=True, text=True, timeout=300)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host.name}: pin check failed: {result.stderr.strip()}")
        return dict(zip(ordered, json.loads(result.stdout)))

    def push(self, local: Path, remote: str, digest: str) -> bool:
        """Installs an immutable pin: a private staged copy, verified, then hard-linked into place.
        An established file is never replaced; a racing copy is accepted only with the same digest.
        Returns True when this call created the destination. The collector's filesystem needs hard
        links."""
        stage_name = f"{remote}.{uuid.uuid4().hex}.stage"
        if self.host.kind == "local":
            destination, stage = Path(remote), Path(stage_name)
            if Path(local) == destination:
                if sha256_file(destination) != digest:
                    raise RuntimeError(f"{self.host.name}: existing pin differs: {remote}")
                return False
            destination.parent.mkdir(parents=True, exist_ok=True)
            try:
                shutil.copyfile(local, stage)
                if sha256_file(stage) != digest:
                    raise RuntimeError(f"{self.host.name}: staged pin differs: {remote}")
                try:
                    os.link(stage, destination)
                    return True
                except FileExistsError:
                    if not destination.is_file() or sha256_file(destination) != digest:
                        raise RuntimeError(f"{self.host.name}: existing pin differs: {remote}") from None
                    return False
            finally:
                stage.unlink(missing_ok=True)
        command = " ".join(shlex.quote(arg) for arg in ("python3", "-c", PUSH_PIN, remote, stage_name, digest))
        with open(local, "rb") as handle:
            result = subprocess.run(self._ssh([command]), stdin=handle, capture_output=True, timeout=600)
        if result.returncode != 0:
            reason = result.stderr.decode(errors="replace").strip().splitlines()[-1:]
            raise RuntimeError(f"{self.host.name}: push of {remote} failed: {' '.join(reason)}")
        return result.stdout.decode(errors="replace").strip().endswith("created")

    def put_text(self, remote: str, text: str) -> None:
        if self.host.kind == "local":
            Path(remote).parent.mkdir(parents=True, exist_ok=True)
            Path(remote).write_text(text)
            return
        parent = str(PurePosixPath(remote).parent)
        result = subprocess.run(self._ssh([f"mkdir -p {shlex.quote(parent)} && cat > {shlex.quote(remote)}"]),
                                input=text, capture_output=True, text=True, timeout=120)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host.name}: write of {remote} failed")

    def fetch_dir(self, remote: str, local: Path) -> None:
        """Copies a shard output directory (flat: trajectories and receipts) to `local`."""
        if local.exists():
            shutil.rmtree(local)
        if self.host.kind == "local":
            try:  # the same files, moved when scratch and staging share a filesystem
                os.rename(remote, local)
            except OSError:
                shutil.copytree(remote, local)
            return
        local.mkdir(parents=True)
        tar = subprocess.Popen(self._ssh([f"tar -C {shlex.quote(remote)} -cf - ."]),
                               stdout=subprocess.PIPE, stdin=subprocess.DEVNULL)
        unpack = subprocess.Popen(["tar", "-C", str(local), "-xf", "-"], stdin=tar.stdout,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        tar.stdout.close()
        deadline = time.time() + FETCH_SECONDS
        try:
            unpack_code = unpack.wait(timeout=max(1.0, deadline - time.time()))
            tar_code = tar.wait(timeout=max(1.0, deadline - time.time()))
        except subprocess.TimeoutExpired:
            for process in (tar, unpack):
                process.kill()
                process.wait()
            shutil.rmtree(local, ignore_errors=True)
            raise RuntimeError(f"{self.host.name}: fetch of {remote} timed out")
        if tar_code != 0 or unpack_code != 0:
            shutil.rmtree(local, ignore_errors=True)
            raise RuntimeError(f"{self.host.name}: fetch of {remote} failed")

    def remove_file(self, remote: str) -> None:
        if self.host.kind == "local":
            Path(remote).unlink(missing_ok=True)
        else:
            subprocess.run(self._ssh([f"rm -f -- {shlex.quote(remote)}"]), capture_output=True, timeout=120)

    def remove_dir(self, remote: str) -> None:
        if self.host.kind == "local":
            shutil.rmtree(remote, ignore_errors=True)
        else:
            subprocess.run(self._ssh([f"rm -rf -- {shlex.quote(remote)}"]), capture_output=True, timeout=300)

    def remove_shard(self, out: str, command_path: str) -> None:
        """A shard's output directory and its command file, in one call."""
        if self.host.kind == "local":
            shutil.rmtree(out, ignore_errors=True)
            Path(command_path).unlink(missing_ok=True)
        else:
            subprocess.run(self._ssh([f"rm -rf -- {shlex.quote(out)} {shlex.quote(command_path)}"]),
                           capture_output=True, timeout=300)


def shard_command_for(whole: dict, first: int, count: int, workers: int, output: str) -> dict:
    """The shard command `pooled_collection_v1::shard_commands_v1` makes, built here so
    audits and reruns get single shards with their own output directory."""
    shard = json.loads(json.dumps(whole))
    shard["episodes"] = whole["episodes"][first:first + count]
    shard["workers"] = workers
    shard["shard"] = {"first_slot": first, "schedule_episodes": len(whole["episodes"])}
    shard["output_directory"] = output
    return shard


def trajectory_pins(shard_dir: Path) -> list[dict]:
    return json.loads((shard_dir / "collection.json").read_text())["trajectories"]


class Dispatcher:
    def __init__(self, config: dict, transports: dict | None = None, timeout: float = 3600.0):
        self.config = config
        self.hosts: list[Host] = config["hosts"]
        self.transports = transports or {h.name: AgentTransport(h) if h.transport == "agent" else Transport(h)
                                          for h in self.hosts}
        self.timeout = timeout
        self.log: list[dict] = []
        self.pushed: dict[str, dict] = {}  # host -> {path: sha256} this collection uploaded
        self._lock = threading.Lock()
        self._input_locks: dict[tuple[str, str], threading.Lock] = {}
        # Pins confirmed present with their bytes, per host. Agent hosts keep them across collections (a
        # resident dispatcher): their agent hashes every input again at put_shard and run_shard, so a stale
        # entry is a refusal there (answered by a full recheck), never other bytes. Other hosts keep them
        # for one collection. Any failure of a host forgets its entries; pruning forgets the pruned paths.
        self.known: dict[str, set] = {}
        self.calls: dict[str, dict] = {}  # host -> op -> [count, seconds], this collection
        self._cleanups: list[threading.Thread] = []
        self.collections = 0

    def note(self, **event):
        event["t"] = round(time.time(), 3)
        with self._lock:
            self.log.append(event)

    def call(self, host: Host, op: str, function, *args):
        """One transport call, counted and timed for the receipt."""
        started = time.monotonic()
        try:
            return function(*args)
        finally:
            seconds = time.monotonic() - started
            with self._lock:
                entry = self.calls.setdefault(host.name, {}).setdefault(op, [0, 0.0])
                entry[0] += 1
                entry[1] += seconds

    def confirm(self, host: Host, pin: tuple[str, str]) -> None:
        with self._lock:
            self.known.setdefault(host.name, set()).add(pin)

    def forget(self, host: Host, paths=None) -> None:
        """Forgets every pin known on `host`, or only those at `paths`."""
        with self._lock:
            if paths is None:
                self.known.pop(host.name, None)
            else:
                self.known[host.name] = {p for p in self.known.get(host.name, ()) if p[0] not in paths}

    def known_on(self, host: Host) -> set:
        with self._lock:
            return set(self.known.get(host.name, ()))

    def distribute(self, host: Host, pins: set, trust_known: bool = True) -> tuple[int, bool]:
        """Makes every pin in `pins` (a command's input closure) present on `host`; returns the bytes
        pushed and whether pins known from earlier were trusted without asking. One pin_status for the
        pins not known, then one push per absent pin. A file at a pinned path with other bytes is never
        replaced: the host fails and its shard moves on. One upload per host and input at a time; a push
        racing another process's install of the same bytes is accepted by the push itself."""
        transport = self.transports[host.name]
        known = self.known_on(host) if trust_known else set()
        unknown = set(pins) - known
        relied = bool(set(pins) & known)
        pushed = 0
        if not unknown:
            return 0, relied
        for pin, state in sorted(self.call(host, "pin_status", transport.pin_status, unknown).items()):
            if state == "ok":
                self.confirm(host, pin)
                continue
            if state == "differs":
                raise RuntimeError(f"{host.name}: {pin[0]} exists with other bytes; not replaced")
            with self._lock:
                lock = self._input_locks.setdefault((host.name, pin[0]), threading.Lock())
            with lock:
                if pin in self.known_on(host):  # another thread of this dispatcher installed it meanwhile
                    continue
                local = Path(pin[0])
                if not local.is_file() or sha256_file_known(local) != pin[1]:
                    raise RuntimeError(f"learner lacks pinned input {pin[0]}")
                created = self.call(host, "push", transport.push, local, pin[0], pin[1])
                pushed += local.stat().st_size
                self.confirm(host, pin)
                if created and host.transport == "agent":  # ownership exists only on agent hosts
                    with self._lock:
                        self.pushed.setdefault(host.name, {})[pin[0]] = pin[1]
        return pushed, relied

    @staticmethod
    def owned_dir(host: Host, path: str) -> str:
        """Every remote directory this dispatcher deletes lies directly under the host's scratch
        root and carries the `pooled-` tag."""
        if PurePosixPath(path).parent != PurePosixPath(host.scratch) or \
                not PurePosixPath(path).name.startswith("pooled-"):
            raise RuntimeError(f"{host.name}: {path} is not a pooled scratch directory")
        return path

    def cleanup(self, host: Host, out: str, command_path: str, tag: str) -> None:
        """Removes a shard's output directory and command file on its host (one call where the transport
        has remove_shard). A failure is noted, never raised: it never reruns or fails a fetched shard."""
        transport = self.transports[host.name]
        if hasattr(transport, "remove_shard"):
            steps = [("remove_shard", transport.remove_shard, (out, command_path))]
        else:
            steps = [("remove_dir", transport.remove_dir, (out,)), ("remove_file", transport.remove_file, (command_path,))]
        for op, function, args in steps:
            try:
                self.call(host, op, function, *args)
            except Exception as error:  # noqa: BLE001
                self.note(event="scratch-cleanup-failed", host=host.name, tag=tag, error=str(error)[-400:])

    def cleanup_later(self, host: Host, out: str, command_path: str, tag: str) -> None:
        """cleanup off the shard's critical path; collect waits for it before its receipt."""
        thread = threading.Thread(target=self.cleanup, args=(host, out, command_path, tag))
        with self._lock:
            self._cleanups.append(thread)
        thread.start()

    def join_cleanups(self) -> None:
        while True:
            with self._lock:
                if not self._cleanups:
                    return
                thread = self._cleanups.pop()
            thread.join()

    def run_shard(self, host: Host, command: dict, tag: str, staging: Path, pins: set | None = None) -> Path:
        transport = self.transports[host.name]
        # The collector is pinned per host and never pushed: a host without its
        # pinned binary fails, and its shard moves on.
        collector = (host.collector, host.collector_sha256)
        # In agent mode the learner's expected collector hash travels with run_shard and the agent
        # refuses a mismatch; its pin_status answers only for the lineage's pin roots.
        if not isinstance(transport, AgentTransport) and collector not in self.known_on(host):
            if self.call(host, "pin_status", transport.pin_status, {collector})[collector] != "ok":
                raise RuntimeError(f"{host.name}: collector binary missing or not its pin")
            self.confirm(host, collector)
        out = self.owned_dir(host, command["output_directory"])
        # The tag is new to this invocation, so its output directory and command file cannot exist on a
        # host yet (the agent and the collector refuse existing ones); a failed run removes both below.
        pushed, relied = self.distribute(host, input_closure(command) if pins is None else pins)
        command_path = str(PurePosixPath(host.scratch) / f"{tag}.json")
        text = json.dumps(command)
        try:
            try:
                self.call(host, "put_shard", transport.put_text, command_path, text)
            except Exception:  # noqa: BLE001
                if not (isinstance(transport, AgentTransport) and relied):
                    raise
                # The agent checks every input when it stores the command: an input known here from an
                # earlier collection is gone (another lineage's retention, a wiped scratch). Ask again.
                self.forget(host)
                self.note(event="pins-rechecked", host=host.name, tag=tag)
                pushed += self.distribute(host, input_closure(command) if pins is None else pins, False)[0]
                self.call(host, "put_shard", transport.put_text, command_path, text)
            started = time.time()
            result = self.call(host, "run_shard", transport.run,
                               host.prefix + [host.collector, "--execute-collect-shard", command_path], self.timeout)
            if result.returncode != 0:
                raise RuntimeError(f"{host.name}: shard {tag} exit {result.returncode}: {result.stderr[-400:]}")
            played = time.time() - started
            local = staging / tag
            self.call(host, "fetch_shard", transport.fetch_dir, out, local)
        except BaseException:
            # Before the shard moves on: a failover host sharing this scratch reuses the tag.
            self.cleanup(host, out, command_path, tag)
            raise
        self.cleanup_later(host, out, command_path, tag)
        self.note(event="shard", host=host.name, tag=tag, first=command["shard"]["first_slot"],
                  episodes=len(command["episodes"]), seconds=round(played, 3), pushed_bytes=pushed,
                  fetched_bytes=sum(p.stat().st_size for p in local.iterdir()))
        return local

    def run_with_failover(self, start: int, whole: dict, first: int, count: int, tag: str,
                          staging: Path, failed: set) -> tuple[Path, Host]:
        """Runs one shard on host `start`, else the next live host. A rerun gives the same
        bytes, so failover never changes the collection."""
        errors, pins = [], None
        for step in range(len(self.hosts)):
            host = self.hosts[(start + step) % len(self.hosts)]
            with self._lock:
                if host.name in failed:
                    continue
            output = str(PurePosixPath(host.scratch) / tag)
            command = shard_command_for(whole, first, count, host.workers, output)
            if pins is None:  # the inputs do not depend on the host
                pins = input_closure(command)
            try:
                return self.run_shard(host, command, tag, staging, pins), host
            except Exception as error:  # noqa: BLE001  any host failure moves the shard
                errors.append(f"{host.name}: {error}")
                self.note(event="host-failed", host=host.name, tag=tag, error=str(error)[-400:])
                self.forget(host)
                with self._lock:
                    failed.add(host.name)
        raise RuntimeError(f"shard {tag} failed on every host: {errors}")

    def begin(self) -> None:
        """A new collection: its own event log, uploads and call counts; non-agent hosts' pins asked again."""
        self.log, self.pushed, self.calls, self._cleanups = [], {}, {}, []
        with self._lock:
            for host in self.hosts:
                if host.transport != "agent":
                    self.known.pop(host.name, None)

    def collect(self, command_path: Path, run_merge) -> dict:
        self.begin()
        try:
            return self._collect(Path(command_path), run_merge)
        finally:
            self.join_cleanups()
            self.collections += 1

    def _collect(self, command_path: Path, run_merge) -> dict:
        timing = {"since_load": round(time.time() - LOADED_AT, 3)} if self.collections == 0 else {}
        clock = time.monotonic()

        def lap(name):
            nonlocal clock
            now = time.monotonic()
            timing[name] = round(now - clock, 3)
            clock = now

        whole = json.loads(command_path.read_text())
        preflight(whole)
        attempt = command_path.parent
        iteration = iteration_of(command_path)
        # Private to this invocation, so runs (or retries) sharing a scratch root never collide.
        tag_root = f"pooled-{iteration:06d}-{uuid.uuid4().hex}"
        staging_root = attempt / "pooled-shards"
        staging = staging_root / tag_root
        staging.mkdir(parents=True)
        total = len(whole["episodes"])
        sizes = plan_sizes(total, [h.weight for h in self.hosts])
        jobs, first = [], 0
        for index, size in enumerate(sizes):
            if size:
                jobs.append((index, first, size))
            first += size
        failed: set = set()
        results: dict[int, tuple[Path, Host]] = {}
        errors: list[str] = []

        def work(job):
            index, first_slot, size = job
            try:
                results[index] = self.run_with_failover(index, whole, first_slot, size,
                                                        f"{tag_root}-s{index:03d}", staging, failed)
            except Exception as error:  # noqa: BLE001
                errors.append(str(error))

        threads = [threading.Thread(target=work, args=(job,)) for job in jobs]
        lap("plan")
        started = time.time()
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        if errors:
            raise RuntimeError("; ".join(errors))
        collected = time.time() - started
        lap("shards")
        self.audit(whole, iteration, jobs, results, staging, tag_root, failed)
        lap("audit")
        directories = [results[index][0] for index, _, _ in jobs]
        merged = run_merge(command_path, directories)
        lap("merge")
        for directory in directories:  # the merge copied every trajectory into collect/
            shutil.rmtree(directory)
        staging.rmdir()
        lap("staging_removal")
        self.join_cleanups()
        lap("cleanup_wait")
        self.prune(input_closure(whole))
        lap("prune")
        receipt = {"schema": "mtg-kernel-pooled-collect-dispatch/v1", "iteration": iteration,
                   "episodes": total, "shards": [[i, f, s, results[i][1].name] for i, f, s in jobs],
                   "collection_seconds": round(collected, 3), "failed_hosts": sorted(failed),
                   "events": self.log, "merged": merged, "timing": timing,
                   "calls": {h: {op: [n, round(s, 3)] for op, (n, s) in sorted(ops.items())}
                             for h, ops in sorted(self.calls.items())}}
        (staging_root / "dispatch.json").write_text(json.dumps(receipt, indent=1))
        return receipt

    def prune(self, current: set) -> None:
        """Remote retention: files pushed for earlier iterations (old checkpoints) are deleted
        once a later collection has merged; current pins and never-pushed files are kept."""
        state_path = self.config.get("state_path")
        if not state_path:
            return
        state_path = Path(state_path)
        with state_lock(state_path):
            self._prune_locked(state_path, current)

    @staticmethod
    def storage_key(host: Host) -> str:
        """Retention records belong to a remote storage, not to a host name: the name plus a hash of
        how it is reached (ssh target, container or VM wrap, transport and scratch). A name reused for
        another machine starts with no ownership there."""
        reach = json.dumps([host.kind, host.ssh, host.remote_wrap, getattr(host, "transport", "shell"),
                            getattr(host, "agent_wrap", ""), host.scratch])
        return f"{host.name}#{hashlib.sha256(reach.encode()).hexdigest()[:16]}"

    def storage_key_now(self, host: Host) -> str | None:
        """storage_key plus the collector storage's generation marker; None when the marker cannot be
        read, in which case nothing is pruned on that host this time."""
        try:
            return f"{self.storage_key(host)}@{self.call(host, 'storage_id', self.transports[host.name].storage_id)}"
        except Exception as error:  # noqa: BLE001
            self.note(event="storage-marker-unreadable", host=host.name, error=str(error)[-200:])
            return None

    def _prune_locked(self, state_path: Path, current: set) -> None:
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        keep_paths = {p for p, _ in current}

        def owned(host):
            return host.prune_pushed and host.transport == "agent"

        # Hosts in parallel (each one's storage marker, then its owned removals); records in host order.
        keys, records = {}, {}

        def key_of(host):
            keys[host.name] = self.storage_key_now(host) if owned(host) else self.storage_key(host)

        def prune_host(host):
            key = keys[host.name]
            previous = state.get(key, {})
            if not isinstance(previous, dict):
                previous = {}  # an older state format holds no hashes: forget, never delete
            now = self.pushed.get(host.name, {})
            merged = dict(previous) | now
            if owned(host):
                for path in sorted(set(merged) - set(now) - keep_paths):
                    # Delete only the very file this dispatcher installed (its ownership witness).
                    try:
                        outcome = self.call(host, "remove_pin", self.transports[host.name].remove_owned,
                                            path, merged[path])
                    except Exception as error:  # noqa: BLE001  kept for the next collection
                        self.note(event="prune-failed", host=host.name, path=path, error=str(error)[-200:])
                        continue
                    finally:
                        self.forget(host, {path})
                    self.note(event="pruned" if outcome == "removed" else "not-owned", host=host.name, path=path)
                    del merged[path]
            records[host.name] = (key, dict(sorted(merged.items())))

        def parallel(step, hosts):
            raised = []

            def guarded(host):
                try:
                    step(host)
                except BaseException as error:  # noqa: BLE001  raised below, as one serial pass would
                    raised.append(error)

            threads = [threading.Thread(target=guarded, args=(host,)) for host in hosts]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()
            if raised:
                raise raised[0]

        parallel(key_of, [h for h in self.hosts if owned(h)])
        for host in self.hosts:
            if not owned(host):
                key_of(host)
        parallel(prune_host, [h for h in self.hosts if owned(h) and keys[h.name] is not None])
        for host in self.hosts:
            if keys[host.name] is None:
                continue  # storage marker unreadable: nothing pruned or recorded on that host this time
            if host.name not in records:
                prune_host(host)
            key, merged = records[host.name]
            state[key] = merged
        stage = state_path.with_name(f"{state_path.name}.{uuid.uuid4().hex}.stage")
        stage.write_text(json.dumps(state, indent=1))
        os.replace(stage, state_path)

    def audit(self, whole, iteration, jobs, results, staging, tag_root, failed):
        """Replays the audit slots on hosts of another architecture class (chosen as before: the
        slot's index among the live other-class hosts). Replays on different hosts run concurrently,
        those on one host one after another; every divergence or failure fails the collection."""
        audit = self.config.get("audit", {})
        slots = audit_slots(iteration, len(whole["episodes"]), int(audit.get("episodes", 0)),
                            int(audit.get("every", 0)))
        plan: dict[str, tuple[Host, list]] = {}
        for slot in slots:
            index = next(i for i, f, s in jobs if f <= slot < f + s)
            first = next(f for i, f, s in jobs if i == index)
            shard_dir, played_on = results[index]
            original = trajectory_pins(shard_dir)[slot - first]["sha256"]
            others = [h for h in self.hosts if h.arch != played_on.arch and h.name not in failed]
            if not others:
                self.note(event="audit-skipped", slot=slot, reason="no other architecture live")
                continue
            host = others[slot % len(others)]
            plan.setdefault(host.name, (host, []))[1].append((slot, played_on, original))
        divergences, failures = [], []

        def replay_on(host, items):
            for slot, played_on, original in items:
                tag = f"{tag_root}-audit{slot:04d}"
                command = shard_command_for(whole, slot, 1, host.workers, str(PurePosixPath(host.scratch) / tag))
                try:
                    replay = self.run_shard(host, command, tag, staging)
                except Exception as error:  # noqa: BLE001  an audit that cannot run fails the collection
                    failures.append(f"audit of slot {slot} on {host.name}: {error}")
                    return
                replayed = trajectory_pins(replay)[0]["sha256"]
                self.note(event="audit", slot=slot, played_on=played_on.name, replayed_on=host.name,
                          equal=replayed == original)
                if replayed != original:
                    divergences.append(f"cross-host divergence at slot {slot}: {played_on.name} ({played_on.arch}) "
                                       f"{original} against {host.name} ({host.arch}) {replayed}")
                shutil.rmtree(replay)

        threads = [threading.Thread(target=replay_on, args=entry) for entry in plan.values()]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        if divergences or failures:
            raise RuntimeError("; ".join(sorted(divergences) + sorted(failures)))


# ---------------------------------------------------------------- collector agent (forced command)
#
# Threat model: the learner's dedicated key may be stolen. Its holder can send any request; the agent
# must keep it to the seven operations and their files. Processes already running as the collector
# account are trusted (they could edit the agent itself).
#
# Kernel enforcement. With "sandbox": "landlock" (the default) Linux Landlock (unprivileged, with
# no_new_privs) confines the agent right after it loads and checks its config, and the collector child
# further, just before it executes:
#   agent      reads beneath the pin roots, the scratch root and the system paths an interpreter and a
#              dynamically linked program need (SYSTEM_DIRS, the interpreter's prefixes, SYSTEM_FILES);
#              executes only beneath those system directories, the collector, the absolute prefix
#              programs and the config's run_paths (for example a binfmt interpreter such as Rosetta);
#              writes, creates, removes, links and renames only beneath the pin roots and the scratch
#              root (and writes /dev/null); never creates a symlink, device, fifo or socket node;
#   collector  the same reads and programs, but the pin roots read-only and nothing else of the scratch
#              root than its private command copy (read) and its run directory `<scratch>/<tag>`
#              (read and write, where it creates its output directory `out`);
#   both       no TCP bind or connect (Landlock ABI 4+); no abstract unix socket or signal beyond their
#              own domain (ABI 6+); truncation confined from ABI 3; cross-directory links and renames
#              from ABI 2 (on ABI 1 they are always denied, so no witness links and nothing is owned).
# So no request, symlink, race or input format makes the agent or the collector open a file outside
# that set, whatever the userspace checks conclude. Not enforced by the kernel: metadata (stat, lstat,
# readlink, whether a path exists) anywhere; reads of the world-readable system files above; which
# files inside the pin roots a request reaches (concurrent allowed requests can substitute one pin for
# another there, which the key holder could push anyway); resource use (see the bounds below); a
# collector outliving a SIGKILLed agent beyond its parent-death signal.
# "sandbox": "none" opts out explicitly, for a platform without Landlock: then only the userspace
# checks hold, which assume the roots' namespace is private to the account, and they do NOT meet the
# confinement contract for the collector's reads: a pinned record the key holder pushed can name, in a
# field the walk below classifies as provenance (or in a document over its 1 MiB parse bound), a file
# the collector's loader opens and hash-checks, so any file the account can read is exposed to a
# guess-the-hash oracle. Standing keys belong on hosts where Landlock (ABI 2 or later for retention)
# is active. With "landlock" configured and unavailable, every request is refused.
#
# Deployment rules the code assumes: the scratch root is exclusive to this agent and disposable (any
# valid tag's directory and command file there may be removed or fetched by the key holder); the pin
# roots hold only this lineage's inputs; memory and CPU limits come from cgroups and the prefix; a
# collector descendant that calls setsid escapes the group kill (a request-scoped cgroup would end it).
#
# Agent-enforced resource bounds (userspace, best effort): a disk floor, min_free_bytes (default 20 GiB,
# 0 disables), checked before push (scratch and the deepest existing directory of the target, so a
# nested mount is measured), put_shard and run_shard (scratch and the pin roots) accept work, and
# during a push on the stage file's own filesystem (first block, every FLOOR_CHECK_BYTES, at the end).
# Inherent to unsynchronized checks: parallel connections admitted together can overshoot by about
# their payloads, and a running collector's output is not watched (only admitted above the floor);
# a filesystem quota or a size-limited volume is the hard bound. And at most max_concurrent_runs
# collectors (default 2), each holding a non-blocking slot lock under `<scratch>/.locks` that the
# kernel releases when its agent dies. A busy or full collector refuses (exit 2); the dispatcher then
# treats that host as failed for the collection and moves the shard on. With the spool executor the agent
# holds its slot while it waits, and the box runner also runs at most max_concurrent_runs at once.
#
# Spool executor: the box runner confines itself right after loading its config with runner_rules (the agent's
# minus writes to the pin roots) and confines every collector with collector_rules, so the kernel enforcement
# above holds for the collector unchanged. The spool
# lies in the scratch root, which the agent's own operations reach only through tag-shaped names; a request
# cannot name, write or remove a spool file, and remove_shard removes only a stale result or cancel of its tag.
#
# The userspace checks stay because they refuse precisely and early: no-follow opens, singly linked
# outputs, exact output and tag paths, printable ASCII paths, inputs within the pin roots with their
# pinned bytes, and revalidation of a stored command just before it runs.
#
# The config path comes only from the forced command's argv (the collector's own disk); the request
# names no config, program or root. Programs are fixed by the config: the collector, run only when its
# sha256 equals both the config's and the learner's expectation, and the prefix (absolute, no PATH
# lookup, not hash pinned: the key holder cannot change it). Remote retention (remove_pin) deletes only
# a file an agent push created and witnessed, under the per-path lock that push held.

AGENT_SCHEMA = "mtg-kernel-pooled-collect-agent/v1"
AGENT_REQUEST_SCHEMA = "pooled-collect-agent-request/v1"
AGENT_REQUEST_BYTES = 64 * 1024  # the encoded request in SSH_ORIGINAL_COMMAND
AGENT_SHARD_BYTES = 16 * 1024 * 1024  # a shard command, as the trainer's config bound
AGENT_PIN_BYTES = 4 * 1024 * 1024 * 1024  # one pushed input
AGENT_FETCH_BYTES = 16 * 1024 * 1024 * 1024  # one fetched shard archive, on the learner
AGENT_REPLY_BYTES = 1 << 20  # one JSON reply, on the learner
AGENT_STDERR_TAIL = 4096  # stderr kept in memory, by the agent (collector) and the learner (ssh)
AGENT_MIN_FREE_BYTES = 20 * 1024 ** 3  # default disk floor (config min_free_bytes; 0 disables)
FLOOR_CHECK_BYTES = 64 * 1024 * 1024  # a push rechecks the floor after every this many bytes
AGENT_TAG = re.compile(r"^pooled-[0-9]{6}-[0-9a-f]{32}(-s[0-9]{3}|-audit[0-9]{4})$")
AGENT_FILE_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,200}$")
WINDOWS_DEVICES = {"CON", "PRN", "AUX", "NUL"} | {f"{d}{i}" for d in ("COM", "LPT") for i in range(10)}
AGENT_OPS = ("pin_status", "push", "put_shard", "run_shard", "fetch_shard", "remove_shard", "remove_pin")
AGENT_ARGS = {"pin_status": {"pins"}, "push": {"path", "sha256"}, "put_shard": {"tag"},
              "run_shard": {"tag", "collector_sha256", "seconds"}, "fetch_shard": {"tag"},
              "remove_shard": {"tag"}, "remove_pin": {"path", "sha256"}}
AGENT_SANDBOXES = ("landlock", "none")
EXECUTORS = ("direct", "spool")
SPOOL_EXECUTOR_KEYS = {"kind", "heartbeat_seconds", "stop_grace_seconds", "watchdog_grace_seconds", "stop_file",
                       "end_file"}
SPOOL_DIR = ".spool"  # under the scratch root, mode 700: the agent's and the box runner's queue
SPOOL_RESERVED = {"runner.lock", "runner.heartbeat"}
SPOOL_REQUEST_SCHEMA = "pooled-collect-spool-request/v1"
SPOOL_RESULT_SCHEMA = "pooled-collect-spool-result/v1"
RUNNER_END_SCHEMA = "pooled-collect-runner-end/v1"
SPOOL_FILE_BYTES = 64 * 1024  # one request or result file
SPOOL_POLL_SECONDS = 0.1
SPOOL_RESULT_GRACE = 30  # the agent waits this long past a run's seconds for the runner's own timeout result
SPOOL_CANCEL_GRACE = 10  # after a cancel, the agent waits this long for the runner to report the kill
SPOOL_SWEEP_SECONDS = 5.0
SPOOL_STAGE_AGE = 60.0  # a stage file older than this was left by a crash
SPOOL_WATCHDOG_GRACE = 30  # default: a collector's own alarm fires this long after its deadline (a hung runner)
SPOOL_WATCHDOG_MAX = 5 * 86400  # the alarm ceiling: far above any job deadline plus grace, far inside alarm()'s range
NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
BINARY = getattr(os, "O_BINARY", 0)


class AgentRefusal(Exception):
    """A request the agent does not serve. Exit 2."""


# ---- Landlock (Linux): the same syscall numbers on every architecture.
LL_CREATE_RULESET, LL_ADD_RULE, LL_RESTRICT_SELF = 444, 445, 446
(LL_EXECUTE, LL_WRITE_FILE, LL_READ_FILE, LL_READ_DIR, LL_REMOVE_DIR, LL_REMOVE_FILE, LL_MAKE_CHAR,
 LL_MAKE_DIR, LL_MAKE_REG, LL_MAKE_SOCK, LL_MAKE_FIFO, LL_MAKE_BLOCK, LL_MAKE_SYM, LL_REFER, LL_TRUNCATE,
 LL_IOCTL_DEV) = (1 << i for i in range(16))
LL_FILE_RIGHTS = LL_EXECUTE | LL_WRITE_FILE | LL_READ_FILE | LL_TRUNCATE | LL_IOCTL_DEV  # a file rule's
LL_READ = LL_READ_FILE | LL_READ_DIR
LL_RUN = LL_READ | LL_EXECUTE
LL_WRITE = LL_WRITE_FILE | LL_REMOVE_DIR | LL_REMOVE_FILE | LL_MAKE_DIR | LL_MAKE_REG | LL_REFER | LL_TRUNCATE
SYSTEM_DIRS = ("/usr", "/lib", "/lib32", "/lib64", "/libx32", "/bin", "/sbin")
SYSTEM_FILES = ("/etc/ld.so.cache", "/etc/ld.so.preload", "/etc/localtime", "/dev/urandom")
_LIBC = None


def _libc():
    global _LIBC
    if _LIBC is None:
        import ctypes
        _LIBC = ctypes.CDLL(None, use_errno=True)
        _LIBC.syscall.restype = ctypes.c_long
    return _LIBC


def _syscall(number: int, *args: int) -> int:
    import ctypes
    result = _libc().syscall(ctypes.c_long(number), *(ctypes.c_long(a) for a in args))
    if result < 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))
    return result


def landlock_abi() -> int:
    """The kernel's Landlock ABI version; 0 where there is none (not Linux, or not enabled)."""
    if not sys.platform.startswith("linux"):
        return 0
    try:
        return _syscall(LL_CREATE_RULESET, 0, 0, 1)  # LANDLOCK_CREATE_RULESET_VERSION
    except (OSError, AttributeError):
        return 0


def _landlock_handled(abi: int) -> tuple[int, int, int]:
    """Every access this ABI can restrict (filesystem, TCP, scopes): what no rule grants is denied."""
    fs = (1 << 13) - 1 | (LL_REFER if abi >= 2 else 0) | (LL_TRUNCATE if abi >= 3 else 0) \
        | (LL_IOCTL_DEV if abi >= 5 else 0)
    return fs, 3 if abi >= 4 else 0, 3 if abi >= 6 else 0


def _landlock_add(ruleset: int, path: str, access: int, handled: int, required: bool) -> None:
    import ctypes
    try:
        target = os.open(path, os.O_PATH | os.O_CLOEXEC)
    except FileNotFoundError:
        if required:
            raise
        return
    try:
        directory = stat.S_ISDIR(os.fstat(target).st_mode)
        allowed = access & handled & (-1 if directory else LL_FILE_RIGHTS)
        if allowed:
            rule = ctypes.create_string_buffer(struct.pack("=Qi", allowed, target), 12)
            _syscall(LL_ADD_RULE, ruleset, 1, ctypes.addressof(rule), 0)  # LANDLOCK_RULE_PATH_BENEATH
    finally:
        os.close(target)


def landlock_ruleset(rules, abi: int) -> int:
    """A ruleset descriptor granting `rules` ((path, access, required) triples) and nothing else."""
    import ctypes
    fs, net, scope = _landlock_handled(abi)
    attr = (ctypes.c_uint64 * 3)(fs, net, scope)
    ruleset = _syscall(LL_CREATE_RULESET, ctypes.addressof(attr), 8 if abi < 4 else 16 if abi < 6 else 24, 0)
    try:
        for path, access, required in rules:
            _landlock_add(ruleset, path, access, fs, required)
    except BaseException:
        os.close(ruleset)
        raise
    return ruleset


def landlock_restrict(ruleset: int) -> None:
    """Confines the calling process (and everything it later starts) to `ruleset`, irreversibly."""
    import ctypes
    if _libc().prctl(ctypes.c_int(38), ctypes.c_ulong(1), ctypes.c_ulong(0), ctypes.c_ulong(0),
                     ctypes.c_ulong(0)) != 0:  # PR_SET_NO_NEW_PRIVS
        raise OSError(ctypes.get_errno(), "no_new_privs")
    _syscall(LL_RESTRICT_SELF, ruleset, 0)


def _system_rules(config: dict) -> list:
    """Reads (and runs) that an interpreter and a dynamically linked program need: system libraries
    and programs, the interpreter's own prefixes (never "/" itself), the loader's cache, the collector
    and the absolute programs of its prefix."""
    prefixes = {p for p in (sys.prefix, sys.base_prefix, sys.exec_prefix, sys.base_exec_prefix)
                if p and os.path.dirname(p) != p}
    rules = [(d, LL_RUN, False) for d in (*SYSTEM_DIRS, *sorted(prefixes))]
    rules += [(f, LL_READ_FILE, False) for f in SYSTEM_FILES]
    rules.append(("/dev/null", LL_READ_FILE | LL_WRITE_FILE | LL_TRUNCATE, False))
    programs = [p for p in config["prefix"] if os.path.isabs(p) and os.path.isfile(p)]
    rules += [(p, LL_RUN, False) for p in programs] + [(config["collector"], LL_RUN, True)]
    rules += [(p, LL_RUN, True) for p in config["run_paths"]]
    if config.get("vector_tanh_dir"):
        rules.append((config["vector_tanh_dir"], LL_READ, True))
    return rules


def _direct_env(config: dict):
    """The direct executor's collector environment: the agent's own (inherited, None) unless a vector-tanh
    record directory is configured, which is then added."""
    if not config.get("vector_tanh_dir"):
        return None
    return dict(os.environ, MTG_KERNEL_VECTOR_TANH_DIR=config["vector_tanh_dir"])


def agent_rules(config: dict) -> list:
    return _system_rules(config) + [(root, LL_READ | LL_WRITE, True)
                                    for root in [*config["pin_roots"], config["scratch"]]]


def collector_rules(config: dict, command_file: str, run_dir: str) -> list:
    return (_system_rules(config) + [(root, LL_READ, True) for root in config["pin_roots"]]
            + [(command_file, LL_READ_FILE, True), (run_dir, LL_READ | LL_WRITE, True)])


def agent_sandbox(config: dict, rules: list | None = None) -> str:
    """Applies the configured sandbox to this process (agent_rules, or the given `rules`); returns the
    effective one ("none" or "landlock-abiN"). Refuses when Landlock is configured and the kernel has none."""
    if config["sandbox"] == "none":
        config["landlock_abi"] = 0
        return "none"
    abi = landlock_abi()
    if abi < 1:
        raise AgentRefusal("sandbox \"landlock\" is configured but this kernel has no Landlock: every "
                           "request is refused (\"sandbox\": \"none\" opts out explicitly)")
    hashlib.sha256(b"").hexdigest()  # OpenSSL reads its configuration now, before /etc closes
    if os.name == "posix":
        import fcntl  # noqa: F401  the per-path locks need it later
    ruleset = landlock_ruleset(agent_rules(config) if rules is None else rules, abi)
    try:
        landlock_restrict(ruleset)
    finally:
        os.close(ruleset)
    config["landlock_abi"] = abi
    return f"landlock-abi{abi}"


def encode_request(op: str, **args) -> str:
    body = json.dumps({"schema": AGENT_REQUEST_SCHEMA, "op": op, "args": args}, separators=(",", ":"))
    return base64.urlsafe_b64encode(body.encode()).decode().rstrip("=")


def decode_request(text) -> tuple[str, dict]:
    if not isinstance(text, str) or not text or len(text) > AGENT_REQUEST_BYTES:
        raise AgentRefusal("missing or oversized request")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", text):
        raise AgentRefusal("request is not one base64url token")
    try:
        body = base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))
        request = json.loads(body)  # one JSON value: trailing data is an error
    except (ValueError, UnicodeDecodeError, RecursionError) as error:
        raise AgentRefusal(f"malformed request: {type(error).__name__}") from None
    if not isinstance(request, dict) or set(request) != {"schema", "op", "args"} \
            or request["schema"] != AGENT_REQUEST_SCHEMA or not isinstance(request["args"], dict):
        raise AgentRefusal("request is not a pooled-collect-agent-request/v1 object")
    if request["op"] not in AGENT_OPS:
        raise AgentRefusal(f"operation not served: {str(request['op'])[:40]!r}")
    if set(request["args"]) != AGENT_ARGS[request["op"]]:
        raise AgentRefusal(f"{request['op']} takes exactly {sorted(AGENT_ARGS[request['op']])}")
    return request["op"], request["args"]


def _canonical(path) -> str | None:
    """`path` if it is printable ASCII, absolute and normalized and its existing part has no
    symlink; else None. ASCII only, so no locale (the key holder may send LANG or LC_* where sshd
    accepts them) can encode a checked name into other bytes than the collector opens."""
    if not isinstance(path, str) or not path or not all(" " <= c <= "~" for c in path):
        return None
    if not os.path.isabs(path) or os.path.normpath(path) != path:
        return None
    probe = path
    while not os.path.lexists(probe):
        parent = os.path.dirname(probe)
        if parent == probe:
            break
        probe = parent
    if os.path.realpath(probe) != probe:
        return None
    return path


def _strictly_under(path: str, roots) -> bool:
    return any(path.startswith(root + os.sep) for root in roots)


def _overlap(a: str, b: str) -> bool:
    """True when one path is the other or lies beneath it ("/" lies above every path)."""
    return a == b or a.startswith(b.rstrip(os.sep) + os.sep) or b.startswith(a.rstrip(os.sep) + os.sep)


def _portable_name(name) -> bool:
    """A plain file name every learner platform stores as a regular file: no separators or drive
    colons, no trailing dot or space, no Windows device stem (CON, NUL.json, com1, ...)."""
    return isinstance(name, str) and AGENT_FILE_NAME.fullmatch(name) is not None \
        and not name.endswith((".", " ")) and name.split(".")[0].upper() not in WINDOWS_DEVICES


def load_agent_config(path: Path) -> dict:
    config = json.loads(Path(path).read_text())
    if config.get("schema") != AGENT_SCHEMA:
        raise ValueError(f"agent config schema is not {AGENT_SCHEMA}")
    for key in ("collector", "collector_sha256", "scratch", "pin_roots", "prefix"):
        if key not in config:
            raise ValueError(f"agent config lacks {key}")
    config["sandbox"] = config.get("sandbox", "landlock")
    if config["sandbox"] not in AGENT_SANDBOXES:
        raise ValueError(f"agent config sandbox must be one of {AGENT_SANDBOXES}")
    roots = list(config["pin_roots"]) + [config["scratch"]]
    if not config["pin_roots"] or not all(_canonical(root) == root for root in roots):
        raise ValueError("agent roots must be canonical absolute paths without symlinks")
    for i, a in enumerate(roots):
        for b in roots[i + 1:]:
            if _overlap(a, b):
                raise ValueError("agent roots overlap")
    own = [os.path.realpath(__file__), os.path.realpath(str(path)), config["collector"]]
    if any(_overlap(root, item) or item.startswith(root + os.sep) for root in roots for item in own):
        raise ValueError("the agent, its config and the collector must lie outside every root")
    if os.name == "posix":
        for root in roots:
            info = os.stat(root)
            if info.st_uid != os.getuid() or info.st_mode & 0o022:
                raise ValueError(f"{root} must be owned by this account and not group or world writable")
    prefix = config["prefix"]
    if not isinstance(prefix, list) or not all(isinstance(a, str) for a in prefix) \
            or (prefix and not os.path.isabs(prefix[0])):
        raise ValueError("agent prefix must be strings starting with an absolute program (no PATH lookup)")
    if not (isinstance(config["collector"], str) and os.path.isabs(config["collector"])):
        raise ValueError("agent collector must be an absolute path")
    config["run_paths"] = config.get("run_paths", [])  # extra read and execute paths (a binfmt interpreter)
    if not isinstance(config["run_paths"], list) or not all(
            isinstance(p, str) and _canonical(p) == p and not any(_overlap(p, root) for root in roots)
            for p in config["run_paths"]):
        raise ValueError("agent run_paths must be canonical absolute paths outside every root")
    # A settled vector-tanh proof record directory (`--settle-vector-tanh` beforehand), read only, passed to the
    # collector as MTG_KERNEL_VECTOR_TANH_DIR by both executors: x86-64 collectors under PlatformLibm then use
    # the proved eight-lane tanh. Same bits either way; without it they call scalar tanhf (slower).
    records = config["vector_tanh_dir"] = config.get("vector_tanh_dir")
    if records is not None and not (
            isinstance(records, str) and _canonical(records) == records and os.path.dirname(records) != records
            and not any(_overlap(records, root) for root in roots)
            and not any(_overlap(records, system) for system in ("/etc", "/proc", "/sys", "/dev", "/home", "/root"))
            and os.path.isdir(records) and not os.path.islink(records)):
        raise ValueError("agent vector_tanh_dir must be an existing directory (no symlink), not \"/\", outside "
                         "and above no root")
    config["max_seconds"] = int(config.get("max_seconds", 3600))
    config["request_seconds"] = int(config.get("request_seconds", 900))
    for key, default, least in (("min_free_bytes", AGENT_MIN_FREE_BYTES, 0), ("max_concurrent_runs", 2, 1)):
        config[key] = config.get(key, default)
        if not isinstance(config[key], int) or isinstance(config[key], bool) or config[key] < least:
            raise ValueError(f"agent {key} must be an integer of at least {least}")
    config["executor"] = _load_executor(config)
    return config


def _load_executor(config: dict) -> dict:
    """The optional `executor`: absent or {"kind": "direct"} runs the collector as the agent's own child (the
    original behaviour); {"kind": "spool", ...} hands each run to the box runner (see BoxRunner)."""
    executor = config.get("executor")
    if executor is None:
        return {"kind": "direct"}
    if not isinstance(executor, dict) or executor.get("kind") not in EXECUTORS or not set(executor) <= (
            {"kind"} if executor.get("kind") == "direct" else SPOOL_EXECUTOR_KEYS):
        raise ValueError(f"agent executor must be {{\"kind\": \"direct\"}} or {{\"kind\": \"spool\"}} with "
                         f"optional {sorted(SPOOL_EXECUTOR_KEYS - {'kind'})}")
    executor = dict(executor)
    if executor["kind"] == "direct":
        return executor
    if os.name != "posix" or not hasattr(os, "waitid"):
        raise ValueError("the spool executor needs a POSIX collector host")
    for key, default, least in (("heartbeat_seconds", 10, 1), ("stop_grace_seconds", 300, 0),
                                ("watchdog_grace_seconds", SPOOL_WATCHDOG_GRACE, 1)):
        executor[key] = executor.get(key, default)
        if not isinstance(executor[key], int) or isinstance(executor[key], bool) or not least <= executor[key] <= 86400:
            raise ValueError(f"agent executor {key} must be an integer from {least} to 86400")
    spool = os.path.join(config["scratch"], SPOOL_DIR)
    for key, default in (("stop_file", "STOP"), ("end_file", "END.json")):
        executor[key] = path = executor.get(key, os.path.join(spool, default))
        name = os.path.basename(path) if isinstance(path, str) else None
        if name is None or os.path.dirname(path) != spool or not _portable_name(name) \
                or name.startswith("pooled-") or name in SPOOL_RESERVED:
            raise ValueError(f"agent executor {key} must be a plain file directly in {spool}")
    if executor["stop_file"] == executor["end_file"]:
        raise ValueError("agent executor stop_file and end_file must differ")
    return executor


def _pin_arg(args: dict, config: dict) -> tuple[str, str]:
    path, digest = args.get("path"), args.get("sha256")
    if _canonical(path) is None or not _strictly_under(path, config["pin_roots"]):
        raise AgentRefusal(f"path outside the pin roots: {str(path)[:200]!r}")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise AgentRefusal("sha256 must be 64 hex digits")
    return path, digest


def _tag_arg(args: dict) -> str:
    tag = args.get("tag")
    if not isinstance(tag, str) or not AGENT_TAG.fullmatch(tag):
        raise AgentRefusal(f"bad tag: {str(tag)[:80]!r}")
    return tag


def _hash_nofollow(path: str, progress=None) -> str | None:
    """The sha256 of a regular file opened without following a final symlink; None if absent;
    '' for anything else (a symlink, a directory, a file the sandbox denies). `progress`, if given, is called
    after every block (the box runner keeps its heartbeat while it hashes)."""
    try:
        fd = os.open(path, os.O_RDONLY | NOFOLLOW | BINARY)
    except FileNotFoundError:
        return None
    except OSError:
        return ""
    with os.fdopen(fd, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode) or os.path.islink(path):
            return ""
        digest = hashlib.sha256()
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
            if progress is not None:
                progress()
        return digest.hexdigest()


def _storage_marker(scratch: str) -> str:
    """This storage's generation marker (see storage_marker): replaced storage gets a new one, so a
    learner's ownership records never carry over to it."""
    data = storage_marker(scratch)
    if not re.fullmatch(r"[0-9a-f]{32}", data):
        raise AgentRefusal("storage marker unreadable")
    return data


def _no_more_input(stdin) -> None:
    """One request per connection: an operation without a payload takes no input at all."""
    if stdin.read(1):
        raise AgentRefusal("one request per connection: unexpected input after the request")


def _read_payload(stdin, cap: int) -> bytes:
    data = stdin.read(cap + 1)
    if len(data) > cap:
        raise AgentRefusal("payload exceeds its bound")
    return data


def _lock(config: dict, key: str):
    """Serializes push and remove_pin on one path, across connections. Striped: 256 fixed lock files
    (paths sharing a stripe only wait for each other), so requests never accumulate lock files."""
    locks = os.path.join(config["scratch"], ".locks")
    os.makedirs(locks, mode=0o700, exist_ok=True)
    return state_lock(Path(locks) / f"path-{hashlib.sha256(key.encode()).hexdigest()[:2]}")


def _existing_ancestor(path: str) -> str:
    """The deepest existing directory on the way to `path`: the filesystem a push lands on, even
    when a directory below a pin root is another mount."""
    probe = os.path.dirname(path)
    while not os.path.isdir(probe):
        probe = os.path.dirname(probe)
    return probe


def _free_bytes(path: str) -> int:
    """Bytes an unprivileged writer can still use on the filesystem holding `path`."""
    if hasattr(os, "statvfs"):
        info = os.statvfs(path)
        return info.f_bavail * info.f_frsize
    return shutil.disk_usage(path).free


def _require_floor(config: dict, paths) -> None:
    """Refuses work while any of `paths` sits on a filesystem with less than min_free_bytes free
    (0 disables). Best effort: concurrent writers can still take space between checks."""
    floor = config["min_free_bytes"]
    for path in paths if floor else ():
        free = _free_bytes(path)
        if free < floor:
            raise AgentRefusal(f"disk floor: {free} bytes free on the filesystem of {path[:200]!r}, "
                               f"below min_free_bytes {floor}")


class _run_slot:
    """One of max_concurrent_runs slots, `<scratch>/.locks/run-slot-<i>`, taken with a non-blocking
    exclusive lock held for the whole run; the kernel releases it when this process dies. No free
    slot refuses the run ("collector busy"); runs are never queued here."""

    def __init__(self, config: dict):
        self.config, self.handle = config, None

    def __enter__(self):
        locks = os.path.join(self.config["scratch"], ".locks")
        os.makedirs(locks, mode=0o700, exist_ok=True)
        for index in range(self.config["max_concurrent_runs"]):
            handle = open(os.path.join(locks, f"run-slot-{index}"), "a+b")
            try:
                if os.name == "nt":
                    import msvcrt
                    handle.seek(0)
                    msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl
                    fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                handle.close()
                continue
            self.handle = handle
            return self
        raise AgentRefusal(f"collector busy: all {self.config['max_concurrent_runs']} run slots are held")

    def __exit__(self, *exc):
        if os.name == "nt":
            import msvcrt
            self.handle.seek(0)
            msvcrt.locking(self.handle.fileno(), msvcrt.LK_UNLCK, 1)
        self.handle.close()  # closing releases a flock
        return False


def _agent_pins_of(document, what: str) -> set:
    try:
        return pins_of(document)
    except PinTreeTooLarge as error:
        raise AgentRefusal(f"{what}: {error}") from None


def _confined_inputs(command: dict, config: dict, progress=None) -> None:
    """Every file the collector will read for this shard lies inside the pin roots: the command's own
    pins, present with their pinned bytes, and the play-import files named by any pinned JSON it reaches
    (the loader reads them), followed transitively. Pins nested inside pinned records are followed only
    when they lie inside the roots and hold their pinned bytes, as the dispatcher's input closure does;
    the others are provenance (the interpreter that produced a file, another host's or a relative source
    path) that the collector never opens, so they are neither followed nor refused. Landlock, not this
    walk, is what confines inputs it cannot inspect (large or binary files, bytes swapped later)."""
    required = _agent_pins_of(command, "shard command")
    for pin in required:
        _pin_arg({"path": pin[0], "sha256": pin[1]}, config)
    for path, digest in sorted(required):
        found = _hash_nofollow(path, progress)
        if found is None:
            raise AgentRefusal(f"pinned input absent: {path[:200]!r}")
        if found != digest:
            raise AgentRefusal(f"pinned input differs from its sha256: {path[:200]!r}")
    queue, seen = sorted(required), set()
    while queue:
        path, digest = queue.pop()
        if path in seen:
            continue
        seen.add(path)
        if _canonical(path) is None or not _strictly_under(path, config["pin_roots"]):
            continue  # provenance only (required pins and import files were checked already)
        try:
            fd = os.open(path, os.O_RDONLY | NOFOLLOW | BINARY)
        except FileNotFoundError:
            continue  # a nested pin the learner does not hold either
        except OSError:
            raise AgentRefusal(f"pinned input is not a regular file: {path[:200]!r}") from None
        with os.fdopen(fd, "rb") as handle:
            if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
                raise AgentRefusal(f"pinned input is not a regular file: {path[:200]!r}")
            data = handle.read(NESTED_PIN_BYTES + 1)
        if len(data) > NESTED_PIN_BYTES:
            continue
        if digest and hashlib.sha256(data).hexdigest() != digest:
            if (path, digest) in required:
                raise AgentRefusal(f"pinned input changed while checked: {path[:200]!r}")
            continue  # other bytes than pinned: the learner does not follow it either
        try:
            document = json.loads(data)
        except (ValueError, UnicodeDecodeError, RecursionError):
            continue
        queue.extend(_agent_pins_of(document, f"pinned input {path[:200]!r}"))
        if isinstance(document, dict) and {"export_directory", *IMPORT_PATHS} <= set(document):
            root = str(document["export_directory"]).rstrip("/")
            names = [f"{root}/{n}" for n in IMPORT_FILES["export_directory"]] + [str(document[k]) for k in IMPORT_PATHS]
            for name in names:
                if _canonical(name) is None or not _strictly_under(name, config["pin_roots"]):
                    raise AgentRefusal(f"a play import names a file outside the pin roots: {name[:200]!r}")
                queue.append((name, ""))


def _valid_shard_command(data: bytes, tag: str, config: dict) -> dict:
    try:
        command = json.loads(data)
    except (ValueError, UnicodeDecodeError, RecursionError):
        raise AgentRefusal("shard command is not JSON") from None
    if not isinstance(command, dict) or command.get("mode") != "collect_parallel" \
            or not isinstance(command.get("shard"), dict) \
            or command.get("output_directory") != config["scratch"] + "/" + tag:
        raise AgentRefusal("not a collect_parallel shard command writing exactly to its own scratch directory")
    workers, episodes = command.get("workers"), command.get("episodes")
    if command.get("ex1") is not None or not isinstance(workers, int) or isinstance(workers, bool) \
            or not 1 <= workers <= 1024 or not isinstance(episodes, list) or not 1 <= len(episodes) <= 1024:
        raise AgentRefusal("a shard has 1 to 1024 episodes and workers and no EX1 hook, as the dispatcher sends")
    _confined_inputs(command, config)
    return command


def _valid_private_command(data: bytes, run_dir: str, config: dict, progress=None) -> dict:
    """The runner's check of an agent's private command copy: what _valid_shard_command requires, with the
    output directory the agent set (`<run_dir>/out`), and every input confined to the pin roots."""
    try:
        command = json.loads(data)
    except (ValueError, UnicodeDecodeError, RecursionError):
        raise ValueError("the job's command is not JSON") from None
    if not isinstance(command, dict) or command.get("mode") != "collect_parallel" \
            or not isinstance(command.get("shard"), dict) \
            or command.get("output_directory") != os.path.join(run_dir, "out"):
        raise ValueError("the job's command is not a collect_parallel shard writing exactly to its run directory")
    workers, episodes = command.get("workers"), command.get("episodes")
    if command.get("ex1") is not None or type(workers) is not int or not 1 <= workers <= 1024 \
            or not isinstance(episodes, list) or not 1 <= len(episodes) <= 1024:
        raise ValueError("the job's command needs 1 to 1024 episodes and workers and no EX1 hook")
    _confined_inputs(command, config, progress)
    return command


def _read_nofollow(path: str, cap: int) -> bytes:
    try:
        fd = os.open(path, os.O_RDONLY | NOFOLLOW | BINARY)
    except FileNotFoundError:
        raise AgentRefusal("no stored shard command for this tag") from None
    except OSError:  # ELOOP: a symlink in place of the stored command
        raise AgentRefusal("stored shard command is not a regular file") from None
    with os.fdopen(fd, "rb") as handle:
        if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode) or os.path.islink(path):
            raise AgentRefusal("stored shard command is not a regular file")
        data = handle.read(cap + 1)
    if len(data) > cap:
        raise AgentRefusal("stored shard command exceeds its bound")
    return data


def _write_new(path: str, data_or_stream, cap: int, floor=None) -> str:
    """Creates `path` (exclusive, no symlink follow) from bytes or a stream; returns its sha256.
    With `floor` (the config), a stream checks the disk floor on `path`'s own filesystem after its
    first block, after every further FLOOR_CHECK_BYTES and once complete, and refuses once free space
    is under it (best effort: a check follows the bytes written, so one interval, and concurrent
    writers between checks, can undershoot the floor). A refused or failed write removes the file."""
    if isinstance(data_or_stream, (bytes, bytearray)) and len(data_or_stream) > cap:
        raise AgentRefusal("payload exceeds its bound")  # before anything is created
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | NOFOLLOW | BINARY, 0o644)
    try:
        return _write_open(fd, data_or_stream, cap, floor, path)
    except BaseException:
        os.unlink(path)
        raise


def _write_open(fd: int, data_or_stream, cap: int, floor, path: str) -> str:
    """_write_new's body, on the descriptor it created."""
    digest, size, checked = hashlib.sha256(), 0, -FLOOR_CHECK_BYTES
    with os.fdopen(fd, "wb") as handle:
        if isinstance(data_or_stream, (bytes, bytearray)):
            handle.write(data_or_stream)
            digest.update(data_or_stream)
        else:
            for block in iter(lambda: data_or_stream.read(1 << 20), b""):
                size += len(block)
                if size > cap:
                    raise AgentRefusal("payload exceeds its bound")
                digest.update(block)
                handle.write(block)
                if floor is not None and size - checked >= FLOOR_CHECK_BYTES:
                    checked = size
                    handle.flush()
                    _require_floor(floor, [path])
            if floor is not None:
                handle.flush()
                _require_floor(floor, [path])
    return digest.hexdigest()


def _drain_tail(stream, tail: bytearray, cap: int = AGENT_STDERR_TAIL) -> None:
    """Reads `stream` to its end, keeping only its last `cap` bytes."""
    for block in iter(lambda: stream.read1(1 << 16), b""):
        tail.extend(block)
        del tail[:-cap]


class _alarm_deferred:
    """Holds the request alarm off while a launched collector is being killed and reaped, so the
    cleanup itself is never interrupted; a pending alarm fires right after."""

    def __enter__(self):
        self.blocked = os.name == "posix" and hasattr(signal, "pthread_sigmask")
        if self.blocked:
            signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGALRM})
        return self

    def __exit__(self, *exc):
        if self.blocked:
            signal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGALRM})


def _wait_exit(process: subprocess.Popen, seconds: float) -> bool:
    """Waits up to `seconds` for the collector to exit without reaping it (Linux), so its process
    group id stays reserved until the group is killed."""
    if os.name != "posix" or not hasattr(os, "waitid"):
        try:
            process.wait(timeout=seconds)
            return True
        except subprocess.TimeoutExpired:
            return False
    deadline = time.monotonic() + seconds
    while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.05)
    return True


def _kill_group(process: subprocess.Popen) -> None:
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)  # the leader is unreaped: the group id is still its
        except (ProcessLookupError, PermissionError):
            pass
    elif process.poll() is None:
        process.kill()


def _collector_preexec(config: dict, command_file: str, run_dir: str):
    """The collector child's last steps before exec (Linux): die with the agent, and (with Landlock)
    confine itself to collector_rules. The ruleset is built here in the parent; the child only
    applies it. Returns (preexec_fn or None, ruleset fd or None)."""
    if not sys.platform.startswith("linux"):
        return None, None
    import ctypes
    libc, parent = _libc(), os.getpid()
    abi = config.get("landlock_abi", 0)
    ruleset = landlock_ruleset(collector_rules(config, command_file, run_dir), abi) if abi else None

    def preexec():
        libc.prctl(ctypes.c_int(1), ctypes.c_ulong(signal.SIGKILL), ctypes.c_ulong(0), ctypes.c_ulong(0),
                   ctypes.c_ulong(0))  # PR_SET_PDEATHSIG
        if os.getppid() != parent:
            os._exit(125)  # the agent already died
        if ruleset is not None:
            landlock_restrict(ruleset)
    return preexec, ruleset


def _run_collector(config: dict, command_file: str, run_dir: str, seconds: int):
    """Runs the collector on `command_file`; returns (exit code or "timeout", stderr tail). On every
    exit path after launch (success, its own deadline, the outer request alarm raised inside the wait,
    any error) its whole process group is killed and the leader reaped before this returns or raises."""
    argv = list(config["prefix"]) + [config["collector"], "--execute-collect-shard", command_file]
    preexec, ruleset = _collector_preexec(config, command_file, run_dir)
    launch = _alarm_deferred().__enter__()  # no alarm between the fork and the guarded wait below
    try:
        process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.PIPE, start_new_session=(os.name == "posix"),
                                   preexec_fn=preexec, cwd=run_dir,  # relative names resolve in its run dir
                                   env=_direct_env(config))
    except BaseException:
        launch.__exit__()
        raise
    finally:
        if ruleset is not None:
            os.close(ruleset)
    tail = bytearray()
    drain = threading.Thread(target=_drain_tail, args=(process.stderr, tail), daemon=True)
    exited = False
    try:
        drain.start()
        launch.__exit__()  # an alarm that came due during the launch fires here, where cleanup follows
        exited = _wait_exit(process, seconds)
    finally:
        with _alarm_deferred():
            _kill_group(process)
            process.wait()
            if drain.ident is not None:
                drain.join(5)  # a descendant that left the group could hold the pipe: never wait on it
            if not drain.is_alive():
                process.stderr.close()
    return (process.returncode if exited else "timeout"), bytes(tail).decode(errors="replace")


# ---------------------------------------------------------------- box runner (executor "spool")
#
# Why: on a W5 node the CPUs that do the work are the boxes (exclusive cgroup v2 partitions that only root
# creates). sshd starts the agent in the user's session scope, on the off-box CPUs, and an unprivileged
# process cannot move itself into a box. So root starts `pooled_collect.py runner CONFIG` inside a box as the
# collector account (as a w5box chain command), and the agent hands each run to it through a queue directory,
# `<scratch>/.spool` (mode 700, owned by the account; no request operation can name or reach it). The collector
# is the same binary with the same argv shape, working directory, inputs and Landlock rules, so a shard's bytes
# do not depend on the executor; only the CPUs it runs on do.
#
# A job is one run_shard: its id is the 32 hex digits of the agent's private command copy
# `<scratch>/.run/<tag>.<job>.json`. Spool files, all for one tag unless named otherwise, all written privately
# and renamed into place within this one directory (Landlock ABI 1 allows that), so each is a singly linked
# regular file and never partial:
#   <tag>.lock     the waiting agent's liveness: an exclusive flock the agent holds from before it creates the
#                  run directory until it leaves (the file appears already locked); the kernel frees it when the
#                  agent dies, so a free lock (probed with a shared flock) means nobody waits for the job: it is
#                  not started, or it is killed
#   <tag>.request  the submission: job, tag, the command copy's sha256, seconds, submission time, the runner
#                  generation it was submitted to, and the binding of the agent's execution config. No path: the
#                  runner derives every path from the tag and the job
#   <tag>.running  the runner's claim (the request renamed)
#   <tag>.result   the answer for that job, published only after the collector's process group is killed and the
#                  collector reaped: exit code or "timeout", last 4 KB of stderr, wall seconds, or an error
#   <tag>.cancel   the agent gave up on that job: the runner kills its process group
#   runner.lock    held by the one runner serving this spool, for its lifetime
#   runner.heartbeat  the runner's generation and identity; touched every heartbeat_seconds / 4 by its loop
#   STOP, END.json the configured stop file (no new job; running jobs finish or are cancelled once
#                  stop_grace_seconds pass; exit 0) and the runner's own end record
# Admission, submission, claim, abandonment and removal of a tag are serialized by the tag's lock (its own
# stripes `<scratch>/.locks/spool-*`, held for metadata operations only). The runner enforces each job's
# deadline from its submission time (queue time included), at most max_concurrent_runs running jobs (a job
# keeps its place until its group is killed and reaped), and drops a request whose agent has gone. It never
# reruns anything: a job claimed when it starts (its predecessor died) and a request submitted to an earlier
# generation are answered as failed. remove_shard refuses while a job of its tag is claimed or its agent holds
# the job lock, and otherwise moves the run directory to a private `.trash-*` name under the lock; fetch_shard
# opens the output directory first and then refuses if a job of its tag is submitted, claimed or waited on.
# Command copies and snapshots a killed agent or runner leaves in `<scratch>/.run` are removed by remove_shard
# and by the runner's sweep once their tag is inactive.
#
# The runner is single threaded (no thread exists when it forks a collector with preexec_fn): one loop polls
# the collectors' stderr pipes, their exits, cancels, departed agents, deadlines, the stop file and the
# heartbeat. It confines itself (runner_rules: the pin roots read only, the scratch root read and write) before
# anything else, after closing inherited descriptors, and each collector further with collector_rules.
#
# What differs for the collector between the executors (none of it is an input to a shard's bytes; an identity
# run on a node compares real shards from both, and the learner's replay checks every decision anyway):
#   environment  direct: the agent's (sshd's session: PATH, HOME, USER, SSH_*, /etc/environment, accepted
#                LANG/LC_*); spool: exactly SPOOL_ENV (PATH, LANG). With the config's vector_tanh_dir both add
#                MTG_KERNEL_VECTOR_TANH_DIR (read only to the collector), so x86-64 collectors under
#                PlatformLibm use the settled eight-lane tanh; without it they call scalar tanhf under either
#                executor (Landlock denies the proof record's usual home). Same bits either way. The collector's
#                other environment reads are diagnostics, checks and caches
#   signal mask  direct: SIGALRM blocked (it is forked under _alarm_deferred); spool: SIGALRM unblocked with
#                the watchdog alarm pending (deadline plus SPOOL_WATCHDOG_GRACE); a collector that used
#                SIGALRM itself would see that (the native trainer does not)
#   umask, rlimits  direct: the sshd PAM session's; spool: w5box's and setpriv's, recorded in the runner's
#                heartbeat and end record. Shard files are fetched with fixed modes, so umask never reaches
#                the learner
#   CPUs         the box's rather than the off-box set; the command fixes the worker count, and a shard's
#                bytes do not depend on the host (failover and cross-architecture audits rely on that)
#
# Root's step (w5box `run`, one named box, as the collector account; the runner refuses uid, gid or a
# supplementary group 0 and any effective or permitted capability):
#   binary  /usr/bin/setpriv (pinned by sha256; w5box spawns as root and requires argv[0] to be the binary)
#   argv    ["/usr/bin/setpriv", "--reuid=mtg", "--regid=mtg", "--init-groups", "--inh-caps=-all",
#            "--bounding-set=-all", "--no-new-privs", "--", "/usr/bin/python3", "-I",
#            "<tools>/pooled_collect.py", "runner", "<agent.json>"]
#   config  the agent config (w5box requires the pinned config to appear in argv); w5box pins neither the
#           script nor the collector: keep both immutable, at versioned paths, for the runner's lifetime
#   no "handoff"; memory_max_bytes sized for max_concurrent_runs collectors.
# Authoritative end: w5box's close record of the instance. The runner's END.json is written when it can (stop
# file, signal, error), not after SIGKILL or a cgroup kill. Before a replacement runner starts after a failure,
# the instance must be closed by w5box, which kills and verifies empty the chain cgroup (collector descendants
# included); `w5box.py run` and `recover` do that. A hung runner shows as a stale heartbeat: agents refuse at
# once and learners fail over. Two watchdogs then end its work: each collector's own alarm kills it (the
# leader; it survives exec) watchdog_grace_seconds after its deadline, and root's timer runs
# `pooled_collect.py runner-watch CONFIG --plan P --chain C --cgroup <root>/box<k>/chain` (say every minute)
# and, on its exit 1 only, the `w5box.py kill` it prints, which kills the chain cgroup with every collector
# descendant. runner-watch acts on nothing itself and kills nothing that is starting, stopped or dead (no runner
# holds the spool's lock), frozen (a w5box pause, a node freeze or an outage freeze), fresh again on its
# confirming second reading, or unknown (a lock holder outside the chain, unreadable cgroup files, a filesystem
# where /proc/locks and fstat disagree on the device). The runner writes its heartbeat before recovering. It
# ignores W5_CONTROL_DIR (no pause handshake). A spool scratch root is never shared with a direct-executor
# config.

SPOOL_ENV = {"PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "LANG": "C.UTF-8"}
SPOOL_REQUEST_KEYS = {"schema", "job", "tag", "command_sha256", "seconds", "submitted_unix", "generation",
                      "binding"}
SPOOL_JOB = re.compile(r"^[0-9a-f]{32}$")


def _execution_binding(config: dict) -> str:
    """What makes a collector run: an agent submits only to a runner whose config binds the same."""
    keys = ("collector", "collector_sha256", "prefix", "run_paths", "pin_roots", "scratch", "sandbox",
            "vector_tanh_dir")
    return hashlib.sha256(json.dumps({k: config.get(k) for k in keys}, sort_keys=True).encode()).hexdigest()


def _spool_env(config: dict) -> dict:
    """The spool executor's collector environment: SPOOL_ENV, plus the vector-tanh record directory if set."""
    env = dict(SPOOL_ENV)
    if config.get("vector_tanh_dir"):
        env["MTG_KERNEL_VECTOR_TANH_DIR"] = config["vector_tanh_dir"]
    return env


def runner_rules(config: dict) -> list:
    """The runner's own confinement: system reads and programs, the pin roots read only (its collectors read
    them), the scratch root read and write (spool, command snapshots, run directories)."""
    return (_system_rules(config) + [(root, LL_READ, True) for root in config["pin_roots"]]
            + [(config["scratch"], LL_READ | LL_WRITE, True)])


def _private_dir(config: dict, name: str) -> str:
    """`<scratch>/<name>`, created mode 700; refused unless it is a directory (not a symlink) of this account
    that no other account can reach."""
    path = os.path.join(config["scratch"], name)
    try:
        os.mkdir(path, 0o700)
    except FileExistsError:
        pass
    info = os.lstat(path)
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise AgentRefusal(f"{name} must be a directory of this account with mode 700")
    return path


def _spool_dir(config: dict) -> str:
    return _private_dir(config, SPOOL_DIR)


def _spool_files(spool: str, tag: str) -> dict:
    return {kind: os.path.join(spool, f"{tag}.{kind}") for kind in ("lock", "request", "running", "result", "cancel")}


def _unlink_quiet(path: str) -> bool:
    try:
        os.unlink(path)
        return True
    except FileNotFoundError:
        return False


def _spool_write(path: str, value: dict) -> None:
    """Publishes a complete JSON file at `path`, written privately and renamed into place."""
    stage = os.path.join(os.path.dirname(path), f".{os.path.basename(path)}.{uuid.uuid4().hex}.tmp")
    _write_new(stage, json.dumps(value).encode(), SPOOL_FILE_BYTES)
    try:
        os.replace(stage, path)
    finally:
        _unlink_quiet(stage)


def _read_single(path: str, cap: int) -> bytes | None:
    """A singly linked regular file's bytes, opened without following a symlink, within `cap`; None if absent."""
    try:
        fd = os.open(path, os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0))  # a FIFO never blocks the open
    except FileNotFoundError:
        return None
    except OSError as error:
        raise ValueError(f"{os.path.basename(path)!r} cannot be opened: {error.strerror}") from None
    try:
        with os.fdopen(fd, "rb") as handle:
            info = os.fstat(handle.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise ValueError(f"{os.path.basename(path)!r} is not a singly linked regular file")
            data = handle.read(cap + 1)
    except OSError as error:
        raise ValueError(f"{os.path.basename(path)!r} cannot be read: {error.strerror}") from None
    if len(data) > cap:
        raise ValueError(f"{os.path.basename(path)!r} exceeds its bound")
    return data


def _spool_read(path: str) -> dict | None:
    data = _read_single(path, SPOOL_FILE_BYTES)
    if data is None:
        return None
    value = json.loads(data)
    if not isinstance(value, dict):
        raise ValueError(f"spool entry {os.path.basename(path)!r} is not a JSON object")
    return value


def _agent_gone(lock_path: str) -> bool:
    """True when no agent waits behind `<tag>.lock`: the file is absent or a shared flock is granted (the
    agent holds an exclusive one; probers share, so two probers never take each other for the agent)."""
    import fcntl
    try:
        fd = os.open(lock_path, os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0))
    except FileNotFoundError:
        return True
    except OSError:
        return False  # no descriptor to spare, or not our file: never taken as a departed agent
    with os.fdopen(fd, "rb") as handle:
        try:
            fcntl.flock(handle.fileno(), fcntl.LOCK_SH | fcntl.LOCK_NB)
        except BlockingIOError:
            return False
        return True  # closing releases it


def _tag_lock(config: dict, tag: str):
    """Serializes submission, claim and removal of one tag across the agent's connections and the runner.
    Striped like _lock, in its own files (`spool-*`), so the runner never waits behind a push hashing a pin."""
    locks = os.path.join(config["scratch"], ".locks")
    os.makedirs(locks, mode=0o700, exist_ok=True)
    return state_lock(Path(locks) / f"spool-{hashlib.sha256(tag.encode()).hexdigest()[:2]}")


def _spool_job_lock(spool: str, tag: str):
    """The agent's `<tag>.lock`, created already flocked (a private file locked, then linked into place), so
    the runner never sees it unlocked while the agent lives. A second job of the tag is refused."""
    import fcntl
    stage = os.path.join(spool, f".{tag}.lock.{uuid.uuid4().hex}.tmp")
    handle = os.fdopen(os.open(stage, os.O_RDWR | os.O_CREAT | os.O_EXCL | NOFOLLOW, 0o600), "r+b")
    try:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            os.link(stage, os.path.join(spool, f"{tag}.lock"))
        except FileExistsError:
            raise AgentRefusal("another job of this tag holds the spool") from None
    except BaseException:
        handle.close()
        raise
    finally:
        _unlink_quiet(stage)
    return handle


def _runner_problem(config: dict, starting: bool) -> str | None:
    """Why no box runner can take (`starting`) or keep serving a job, or None: its stop file exists (starting
    only: a stopping runner still finishes what it holds), or its heartbeat is missing or stale."""
    executor = config["executor"]
    if starting and os.path.lexists(executor["stop_file"]):
        return "the box runner is stopping (its stop file exists)"
    try:
        info = os.lstat(os.path.join(config["scratch"], SPOOL_DIR, "runner.heartbeat"))
    except FileNotFoundError:
        return "no box runner serves this collector's spool"
    if not stat.S_ISREG(info.st_mode):
        return "the box runner's heartbeat is not a regular file"
    age = time.time() - info.st_mtime
    if age > executor["heartbeat_seconds"]:
        return f"the box runner's heartbeat is {age:.0f} s old (limit {executor['heartbeat_seconds']} s)"
    return None


def _spool_take(path: str, tag: str, job: str, generation: str) -> dict | None:
    """Reads and removes the runner's result for this job; None while there is none. A malformed, oversized
    or mismatched result refuses."""
    try:
        result = _spool_read(path)
    except (ValueError, UnicodeDecodeError, RecursionError) as error:
        _unlink_quiet(path)
        raise AgentRefusal(f"malformed box runner result: {str(error)[:200]}") from None
    if result is None:
        return None
    _unlink_quiet(path)
    code, tail, error = result.get("code"), result.get("stderr"), result.get("error")
    if set(result) != {"schema", "job", "tag", "generation", "code", "stderr", "seconds", "error"} \
            or result["schema"] != SPOOL_RESULT_SCHEMA or result["tag"] != tag or result["job"] != job \
            or not isinstance(tail, str) or len(tail) > AGENT_STDERR_TAIL \
            or not (error is None or isinstance(error, str)) \
            or not (code in ("timeout", None) or (type(code) is int)) or (error is None) == (code is None) \
            or not isinstance(result["generation"], str) or not SPOOL_JOB.fullmatch(result["generation"]) \
            or (error is None and result["generation"] != generation) \
            or type(result["seconds"]) not in (int, float) or not math.isfinite(result["seconds"]) \
            or result["seconds"] < 0:
        raise AgentRefusal("malformed or mismatched box runner result")
    return result


def _spool_cancel(config: dict, tag: str, files: dict, job: str) -> None:
    """Withdraws this job's unclaimed request, or asks the runner to kill it once claimed."""
    with _tag_lock(config, tag):
        try:
            pending = _spool_read(files["request"])
        except (ValueError, UnicodeDecodeError, RecursionError):
            pending = None
        if pending is not None and pending.get("job") == job:
            _unlink_quiet(files["request"])
        elif os.path.lexists(files["running"]):
            _spool_write(files["cancel"], {"job": job})


def _spool_admit(config: dict, tag: str):
    """A spooled run_shard's first step, before its run directory or command copy exists: under the tag's lock,
    the agent's job lock (from then on remove_shard refuses the tag and the runner sees a waiting agent) and the
    fast refusals. The caller releases it with _spool_release after everything else."""
    spool = _spool_dir(config)
    files = _spool_files(spool, tag)
    with _tag_lock(config, tag):
        lock = _spool_job_lock(spool, tag)
        admitted = types.SimpleNamespace(tag=tag, files=files, lock=lock, spool=spool)
        try:
            if any(os.path.lexists(files[kind]) for kind in ("request", "running", "result", "cancel")):
                raise AgentRefusal("the spool already holds this tag (remove_shard first)")
            problem = _runner_problem(config, starting=True)
            if problem:
                raise AgentRefusal(problem)
        except BaseException:
            _spool_release(admitted)
            raise
    return admitted


def _spool_release(admitted) -> None:
    _unlink_quiet(admitted.files["lock"])
    admitted.lock.close()


def _spool_run(config: dict, admitted, job: str, command_sha256: str, seconds: int):
    """The agent's side of a spooled job: submits it, waits for its result and returns what _run_collector
    returns, (exit code or "timeout", stderr tail). A result reporting an error (an orphan after a runner
    restart, a stop, a refusal) or a runner whose heartbeat stops refuses, so the learner fails the host over.
    The agent stops waiting at its own deadline (seconds plus SPOOL_RESULT_GRACE, then SPOOL_CANCEL_GRACE for
    the kill to be reported) whether or not its cancel lands; every exit without a result cancels, and its
    death frees its lock, which the runner treats as a cancel."""
    tag, files, spool = admitted.tag, admitted.files, admitted.spool
    with _tag_lock(config, tag):
        if any(os.path.lexists(files[kind]) for kind in ("request", "running", "result", "cancel")):
            raise AgentRefusal("the spool already holds this tag (remove_shard first)")
        problem = _runner_problem(config, starting=True)
        if problem:
            raise AgentRefusal(problem)
        try:
            heartbeat = _spool_read(os.path.join(spool, "runner.heartbeat")) or {}
        except (ValueError, UnicodeDecodeError, RecursionError):
            heartbeat = {}
        generation = heartbeat.get("generation")
        if not isinstance(generation, str) or not SPOOL_JOB.fullmatch(generation):
            raise AgentRefusal("the box runner's heartbeat names no generation")
        if sum(name.endswith(".request") for name in os.listdir(spool)) >= config["max_concurrent_runs"]:
            raise AgentRefusal("collector busy: the box runner's queue is full")
        _spool_write(files["request"], {"schema": SPOOL_REQUEST_SCHEMA, "job": job, "tag": tag,
                                        "command_sha256": command_sha256, "seconds": seconds,
                                        "submitted_unix": time.time(), "generation": generation,
                                        "binding": _execution_binding(config)})
    result = None
    try:
        give_up = time.monotonic() + seconds + SPOOL_RESULT_GRACE
        while result is None:
            result = _spool_take(files["result"], tag, job, generation)
            if result is not None:
                break
            problem = _runner_problem(config, starting=False)
            if problem:
                raise AgentRefusal(f"{problem} during the run; the shard was not rerun")
            if time.monotonic() >= give_up:
                try:
                    _spool_cancel(config, tag, files, job)
                except (OSError, ValueError):
                    pass  # the agent stops waiting either way; its departure frees the lock
                confirm = time.monotonic() + SPOOL_CANCEL_GRACE
                while result is None and time.monotonic() < confirm \
                        and (os.path.lexists(files["running"]) or os.path.lexists(files["result"])):
                    time.sleep(SPOOL_POLL_SECONDS)
                    result = _spool_take(files["result"], tag, job, generation)
                if result is not None and result["error"] is not None:
                    raise AgentRefusal(f"box runner: {result['error'][:300]}")
                return "timeout", result["stderr"] if result else ""
            time.sleep(SPOOL_POLL_SECONDS)
        if result["error"] is not None:
            raise AgentRefusal(f"box runner: {result['error'][:300]}")
        return result["code"], result["stderr"]
    finally:
        with _alarm_deferred():  # the request alarm never interrupts the cancel
            try:
                if result is None:
                    _spool_cancel(config, tag, files, job)
                else:
                    _unlink_quiet(files["cancel"])
            except (OSError, ValueError):
                pass


def _spool_discard(files: dict) -> None:
    for kind in ("result", "cancel", "lock"):
        _unlink_quiet(files[kind])


def _remove_private_copies(config: dict, tag: str) -> None:
    """Unlinks `<scratch>/.run/<tag>.*.json` (command copies and snapshots a killed agent or runner left).
    Only while the tag is inactive, under its lock."""
    run = _private_dir(config, ".run")
    for name in os.listdir(run):
        if name.startswith(tag + ".") and name.endswith(".json"):
            _unlink_quiet(os.path.join(run, name))


def _spool_remove(config: dict, tag: str) -> None:
    """remove_shard with the spool executor. Under the tag's lock: refuses while a job of the tag is claimed
    or an agent holds its job lock (from before its run directory exists until it leaves); otherwise removes
    an abandoned request, a stale result or cancel, the tag's leftover command copies and its stored command,
    and renames its run directory to a private `.trash-*` name. The tree is deleted after the lock is released
    (the lock covers metadata operations only)."""
    spool = _spool_dir(config)
    files = _spool_files(spool, tag)
    scratch = config["scratch"]
    trash = None
    with _tag_lock(config, tag):
        if os.path.lexists(files["running"]) or not _agent_gone(files["lock"]):
            raise AgentRefusal("a job of this tag is active: it ends with its run_shard")
        _unlink_quiet(files["request"])
        _spool_discard(files)
        _remove_private_copies(config, tag)
        _unlink_quiet(os.path.join(scratch, f"{tag}.json"))
        if os.path.lexists(os.path.join(scratch, tag)):
            trash = os.path.join(scratch, f".trash-{uuid.uuid4().hex}")
            os.rename(os.path.join(scratch, tag), trash)
    if trash is not None:
        try:
            _remove_tree(trash)
        except FileNotFoundError:
            pass  # the runner's sweep removed (part of) an old trash tree meanwhile


def _remove_tree(path: str) -> None:
    if os.path.islink(path) or not os.path.isdir(path):
        os.unlink(path)
    else:
        shutil.rmtree(path)


def _spool_active(config: dict, tag: str) -> bool:
    files = _spool_files(_spool_dir(config), tag)
    return os.path.lexists(files["request"]) or os.path.lexists(files["running"]) or not _agent_gone(files["lock"])


def _spool_fetch(config: dict, tag: str, stdout) -> None:
    """fetch_shard with the spool executor: opens `<tag>/out` first (no symlink at either level) and only then
    checks that no job of the tag is submitted, claimed or waited on, so the directory it tars is one no
    collector writes any more; members are opened relative to it."""
    flags = os.O_RDONLY | NOFOLLOW | getattr(os, "O_DIRECTORY", 0)
    try:
        run_fd = os.open(os.path.join(config["scratch"], tag), flags)
    except FileNotFoundError:
        raise AgentRefusal("no shard output for this tag") from None
    except OSError:
        raise AgentRefusal("shard output is not a directory") from None
    try:
        try:
            out_fd = os.open("out", flags, dir_fd=run_fd)
        except FileNotFoundError:
            raise AgentRefusal("no shard output for this tag") from None
        except OSError:
            raise AgentRefusal("shard output is not a directory") from None
    finally:
        os.close(run_fd)
    try:
        if _spool_active(config, tag):
            raise AgentRefusal("a job of this tag is active: its output is not complete")
        with tarfile.open(fileobj=stdout, mode="w|") as archive:
            for name in sorted(os.listdir(out_fd)):
                if not _portable_name(name):
                    raise AgentRefusal(f"unexpected name in shard output: {name[:80]!r}")
                try:
                    fd = os.open(name, os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0), dir_fd=out_fd)
                except OSError:  # ELOOP: a symlink in the shard output
                    raise AgentRefusal(f"shard output {name!r} is not a singly linked regular file") from None
                with os.fdopen(fd, "rb") as handle:
                    info = os.fstat(handle.fileno())
                    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                        raise AgentRefusal(f"shard output {name!r} is not a singly linked regular file")
                    entry = tarfile.TarInfo(name)
                    entry.size, entry.mode, entry.mtime = info.st_size, 0o644, 0
                    archive.addfile(entry, handle)
    finally:
        os.close(out_fd)


def _launch_spooled(config: dict, command_file: str, run_dir: str, watchdog_seconds: int):
    """Starts the collector as _run_collector does (argv shape, working directory, its own process group,
    parent-death signal, collector_rules under Landlock), with SPOOL_ENV and a non-blocking stderr pipe that
    the runner's loop drains (no thread). Its last pre-exec step arms the kernel's own watchdog: SIGALRM
    unblocked and alarm(watchdog_seconds), which survives exec, so the collector ends even if the runner hangs."""
    argv = list(config["prefix"]) + [config["collector"], "--execute-collect-shard", command_file]
    confine, ruleset = _collector_preexec(config, command_file, run_dir)

    def preexec():
        if confine is not None:
            confine()
        signal.signal(signal.SIGALRM, signal.SIG_DFL)
        signal.pthread_sigmask(signal.SIG_UNBLOCK, {signal.SIGALRM})
        signal.alarm(seconds)
    seconds = max(1, min(int(watchdog_seconds), SPOOL_WATCHDOG_MAX))  # never alarm(0) (none) or a wrapped value
    try:
        process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                   stderr=subprocess.PIPE, start_new_session=True, preexec_fn=preexec,
                                   cwd=run_dir, env=_spool_env(config))
    finally:
        if ruleset is not None:
            os.close(ruleset)
    try:
        os.set_blocking(process.stderr.fileno(), False)
    except BaseException:
        _kill_group(process)
        process.wait()
        raise
    return process


def _drain_nonblocking(job) -> bool:
    """Moves what the collector's stderr pipe holds now into the job's tail; True at end of stream."""
    while True:
        try:
            block = os.read(job.process.stderr.fileno(), 1 << 16)
        except BlockingIOError:
            return False
        if not block:
            return True
        job.tail.extend(block)
        del job.tail[:-AGENT_STDERR_TAIL]


def _utc() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


class BoxRunner:
    """Serves one spool from one thread: claims requests, runs each collector in its own process group (at
    most max_concurrent_runs at once), answers each job once, and honours cancels, deadlines, departed agents
    and the stop file. Takes work only from the spool; opens no network connection."""

    def __init__(self, config: dict, identity: dict | None = None):
        self.config, self.executor = config, config["executor"]
        self.spool = _spool_dir(config)
        self.heartbeat = os.path.join(self.spool, "runner.heartbeat")
        self.generation = uuid.uuid4().hex
        self.binding = _execution_binding(config)
        self.identity = identity or {}
        self.jobs: dict[str, types.SimpleNamespace] = {}
        self.counts = {"jobs_run": 0, "jobs_failed": 0, "jobs_refused": 0, "orphans_failed": 0}
        self.signalled = False
        self.stop_seen = None
        self.started_utc = _utc()
        self.end_error = None
        self.beat_every = max(0.2, self.executor["heartbeat_seconds"] / 4)
        self.next_beat = 0.0

    def beat(self) -> None:
        """Touches the heartbeat when due (from the loop, between claims and while hashing a job's inputs)."""
        now = time.monotonic()
        if now >= self.next_beat and os.path.lexists(self.heartbeat):
            os.utime(self.heartbeat)
            self.next_beat = now + self.beat_every

    def serve(self) -> str:
        """Runs until the stop file (reason "stop-file") or a signal ("signal"); returns the reason. Only the
        owner of `runner.lock` touches the heartbeat, claims or the end record, which it writes before it
        releases the lock (also after an error)."""
        import fcntl
        lock = os.fdopen(os.open(os.path.join(self.spool, "runner.lock"), os.O_RDWR | os.O_CREAT | NOFOLLOW, 0o600),
                         "r+b")
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            lock.close()  # the live runner's state (heartbeat, claims, end record) is not this process's to touch
            raise RunnerBusy("another box runner serves this spool") from None
        reason = "error: interrupted"
        try:
            reason = self.serve_owned()
            return reason
        except Exception as error:
            reason = f"error: {type(error).__name__}: {str(error)[:300]}"
            raise
        finally:
            try:
                self.cancel_all("the box runner ended during this run; the shard was not rerun")
            finally:
                _unlink_quiet(self.heartbeat)
                try:
                    self.write_end(reason)
                finally:
                    lock.close()

    def write_end(self, reason: str) -> None:
        try:
            _spool_write(self.executor["end_file"], dict(
                self.counts, schema=RUNNER_END_SCHEMA, reason=reason, started_utc=self.started_utc,
                ended_utc=_utc(), pid=os.getpid(), generation=self.generation, **self.identity))
        except Exception as error:  # noqa: BLE001  reported by runner_main; w5box's record is authoritative
            self.end_error = error

    def serve_owned(self) -> str:
        """The loop, run while owning the spool."""
        if os.path.lexists(self.executor["stop_file"]):
            return "stop-file"  # a late start exits at once; the operator removes the stop file
        _spool_write(self.heartbeat, dict(self.identity, generation=self.generation, pid=os.getpid(),
                                          started_utc=_utc()))  # before recovery: root's check sees a live runner
        self.next_beat = time.monotonic() + self.beat_every
        self.recover()
        next_sweep = 0.0
        while True:
            now = time.monotonic()
            self.beat()
            self.watch()
            if self.signalled:
                self.cancel_all("the box runner was stopped by a signal during this run; the shard was not rerun")
                return "signal"
            stopping = os.path.lexists(self.executor["stop_file"])
            if stopping and self.stop_seen is None:
                self.stop_seen = now
            self.claim(stopping)
            if stopping:
                if not self.jobs:
                    return "stop-file"
                if now - self.stop_seen >= self.executor["stop_grace_seconds"]:
                    self.cancel_all("the box runner stopped (stop file, grace over) during this run; "
                                    "the shard was not rerun")
                    return "stop-file"
            if now >= next_sweep:
                self.sweep()
                next_sweep = now + SPOOL_SWEEP_SECONDS
            self.wait_streams(SPOOL_POLL_SECONDS)

    def wait_streams(self, seconds: float) -> None:
        """Sleeps up to `seconds`, waking to drain any collector's stderr as it arrives."""
        import select
        streams = {job.process.stderr.fileno(): job for job in self.jobs.values() if not job.eof}
        if not streams:
            time.sleep(seconds)
            return
        ready, _, _ = select.select(list(streams), [], [], seconds)
        for fd in ready:
            streams[fd].eof = _drain_nonblocking(streams[fd])

    def answer(self, tag: str, files: dict, job: str | None, error: str) -> None:
        self.finish(tag, files, {"job": job, "code": None, "stderr": "", "seconds": 0.0, "error": error})

    def recover(self) -> None:
        """A job claimed by a runner that died is answered as failed, never run again; requests are handled by
        claim (an earlier generation's are refused, abandoned ones dropped)."""
        for name in sorted(os.listdir(self.spool)):
            tag, _, kind = name.rpartition(".")
            if kind == "running" and AGENT_TAG.fullmatch(tag):
                files = _spool_files(self.spool, tag)  # only a runner writes or removes a claim
                try:
                    job = (_spool_read(files["running"]) or {}).get("job")
                except (ValueError, UnicodeDecodeError, RecursionError):  # OSError arrives as ValueError
                    job = None
                self.counts["orphans_failed"] += 1
                self.answer(tag, files, job if isinstance(job, str) else None,  # finish takes the tag's lock
                            "the box runner restarted during this run; the shard was not rerun")

    def claim(self, stopping: bool) -> None:
        for name in sorted(os.listdir(self.spool)):
            tag, _, kind = name.rpartition(".")
            if kind != "request" or not AGENT_TAG.fullmatch(tag) or tag in self.jobs:
                continue
            if not stopping and len(self.jobs) >= self.config["max_concurrent_runs"]:
                return
            files = _spool_files(self.spool, tag)
            with _tag_lock(self.config, tag):
                if os.path.lexists(files["running"]) or os.path.lexists(files["result"]):
                    continue  # never replaces another job's state
                try:
                    os.rename(files["request"], files["running"])
                except FileNotFoundError:
                    continue  # withdrawn
            self.start(tag, files, "the box runner is stopping; the shard was not run" if stopping else None)
            self.beat()

    def start(self, tag: str, files: dict, refuse: str | None) -> None:
        job_id = None
        try:
            request = _spool_read(files["running"])
            if request is None:
                return
            job_id = request.get("job") if isinstance(request.get("job"), str) else None
            with _tag_lock(self.config, tag):
                if _agent_gone(files["lock"]):  # nobody waits: dropped unrun
                    _unlink_quiet(files["running"])
                    _spool_discard(files)
                    _remove_private_copies(self.config, tag)
                    return
            if refuse:
                raise ValueError(refuse)
            data, run_dir, deadline = self.validate(tag, request)
            snapshot = os.path.join(self.config["scratch"], ".run", f"{tag}.{uuid.uuid4().hex}.snap.json")
            _write_new(snapshot, data, AGENT_SHARD_BYTES)
            try:
                if time.monotonic() >= deadline:
                    raise ValueError("the job's deadline passed before its launch")
                watchdog = math.ceil(deadline - time.monotonic()) + self.executor["watchdog_grace_seconds"]
                process = _launch_spooled(self.config, snapshot, run_dir, watchdog)
            except BaseException:
                os.unlink(snapshot)
                raise
        except Exception as error:  # noqa: BLE001  answered as refused, the runner keeps serving
            self.counts["jobs_refused"] += 1
            self.answer(tag, files, job_id, f"{type(error).__name__}: {str(error)[:300]}")
            return
        self.counts["jobs_run"] += 1
        self.jobs[tag] = types.SimpleNamespace(files=files, job=job_id, process=process, tail=bytearray(), eof=False,
                                               snapshot=snapshot, started=time.monotonic(), deadline=deadline)

    def validate(self, tag: str, request: dict) -> tuple[bytes, str, float]:
        """A claimed request's exact schema, its submission to this generation under the same execution config,
        a deadline from its submission time (queue time included, capped by max_seconds) not yet passed, and the
        command copy derived from tag and job: singly linked, with the submitted sha256, writing exactly to its
        run directory. Returns (the command bytes to snapshot, run directory, monotonic deadline)."""
        if set(request) != SPOOL_REQUEST_KEYS or request["schema"] != SPOOL_REQUEST_SCHEMA or request["tag"] != tag \
                or not isinstance(request["job"], str) or not SPOOL_JOB.fullmatch(request["job"]) \
                or not isinstance(request["command_sha256"], str) \
                or type(request["seconds"]) is not int or request["seconds"] < 1 \
                or type(request["submitted_unix"]) not in (int, float) \
                or not math.isfinite(request["submitted_unix"]):
            raise ValueError("malformed spool request")
        if request["generation"] != self.generation:
            raise ValueError("submitted to an earlier box runner; the shard was not run")
        if request["binding"] != self.binding:
            raise ValueError("the agent's execution config (collector, prefix, roots) differs from this runner's")
        seconds = min(request["seconds"], self.config["max_seconds"])
        remaining = request["submitted_unix"] + seconds - time.time()
        if remaining <= 0:
            raise ValueError("the job's deadline passed while it was queued")
        scratch = self.config["scratch"]
        run_dir = os.path.join(scratch, tag)
        if not stat.S_ISDIR(os.lstat(run_dir).st_mode):
            raise ValueError("the job's run directory is not a directory")
        run = _private_dir(self.config, ".run")
        data = _read_single(os.path.join(run, f"{tag}.{request['job']}.json"), AGENT_SHARD_BYTES)
        if data is None or hashlib.sha256(data).hexdigest() != request["command_sha256"]:
            raise ValueError("the job's command copy is absent or not the submitted bytes")
        _valid_private_command(data, run_dir, self.config, self.beat)
        if _hash_nofollow(self.config["collector"], self.beat) != self.config["collector_sha256"]:
            raise ValueError("collector binary differs from its pin")
        return data, run_dir, time.monotonic() + min(remaining, seconds)

    def watch(self) -> None:
        for tag, job in list(self.jobs.items()):
            exited = _wait_exit(job.process, 0)
            if exited or self.cancelled(job) or _agent_gone(job.files["lock"]) or time.monotonic() >= job.deadline:
                self.end(tag, exited, None)

    @staticmethod
    def cancelled(job) -> bool:
        try:
            return (_spool_read(job.files["cancel"]) or {}).get("job") == job.job
        except (ValueError, UnicodeDecodeError, RecursionError):
            return True  # an unreadable cancel still cancels

    def end(self, tag: str, exited: bool, error: str | None) -> None:
        """Kills the job's whole process group and reaps the collector, then answers (as _run_collector ends);
        the job holds its place until then."""
        job = self.jobs[tag]
        _kill_group(job.process)
        job.process.wait()
        if not job.eof:
            job.eof = _drain_nonblocking(job)  # what is left; a descendant that escaped could hold the pipe open
        job.process.stderr.close()
        _unlink_quiet(job.snapshot)
        code = job.process.returncode if exited else "timeout"
        if error is not None or code != 0:
            self.counts["jobs_failed"] += 1
        del self.jobs[tag]
        self.finish(tag, job.files, {"job": job.job, "code": None if error else code,
                                     "stderr": bytes(job.tail).decode(errors="replace")[-AGENT_STDERR_TAIL:],
                                     "seconds": round(time.monotonic() - job.started, 3), "error": error})

    def finish(self, tag: str, files: dict, fields: dict) -> None:
        with _tag_lock(self.config, tag):
            try:
                _spool_write(files["result"], {"schema": SPOOL_RESULT_SCHEMA, "tag": tag,
                                               "generation": self.generation, **fields})
            finally:
                _unlink_quiet(files["running"])
            if _agent_gone(files["lock"]):  # nobody will read it; the departed agent left its command copy
                _spool_discard(files)
                _remove_private_copies(self.config, tag)

    def cancel_all(self, error: str) -> None:
        for tag, job in list(self.jobs.items()):
            exited = _wait_exit(job.process, 0)
            self.end(tag, exited, None if exited else error)

    def sweep(self) -> None:
        """Removes crash leftovers: old stage files and trash, requests whose agent has gone, results, cancels
        and locks no agent waits on, and command copies or snapshots of inactive tags (a killed agent or runner
        leaves them in `.run`)."""
        now = time.time()
        run = _private_dir(self.config, ".run")
        tags = set()
        for directory in (self.spool, run, self.config["scratch"]):
            for name in os.listdir(directory):
                path = os.path.join(directory, name)
                try:
                    if name.startswith(".") and (name.endswith(".tmp") or name.startswith(".trash-")):
                        if now - os.lstat(path).st_ctime > SPOOL_STAGE_AGE:  # a rename sets ctime
                            _remove_tree(path)
                    elif directory == self.spool:
                        tag, _, kind = name.rpartition(".")
                        if kind in ("request", "result", "cancel", "lock") and AGENT_TAG.fullmatch(tag):
                            tags.add(tag)
                    elif directory == run and name.endswith(".snap.json") \
                            and name.split(".", 1)[0] not in self.jobs:
                        _unlink_quiet(path)  # only this runner snapshots; one it no longer runs is a leftover
                except OSError as error:  # one stubborn entry never ends the runner
                    sys.stderr.write(f"sweep: {name[:80]!r}: {type(error).__name__}: {str(error)[:120]}\n")
                self.beat()
        # An agent's own command copy is removed only for a tag with spool state and no live agent: a scratch
        # root shared with a direct-executor config (never deployed so) keeps that agent's copies.
        for tag in sorted(t for t in tags if t not in self.jobs):
            files = _spool_files(self.spool, tag)
            with _tag_lock(self.config, tag):
                if not os.path.lexists(files["running"]) and _agent_gone(files["lock"]):
                    _unlink_quiet(files["request"])
                    _spool_discard(files)
                    _remove_private_copies(self.config, tag)


class RunnerBusy(RuntimeError):
    """Another runner holds the spool: this one exits without an end record (the record is the live runner's)."""


def _runner_identity() -> dict:
    """Where the runner runs, read before the sandbox closes /proc: its cgroup, CPUs and W5 box variables."""
    identity = {key.lower(): os.environ[key] for key in ("W5_CHAIN", "W5_BOX_SLOT", "W5_BOX_CPUS")
                if key in os.environ}
    try:
        identity["cgroup"] = Path("/proc/self/cgroup").read_text()[:400].strip()
    except OSError:
        pass
    if hasattr(os, "sched_getaffinity"):
        identity["cpus"] = format_cpus(sorted(os.sched_getaffinity(0)))
    identity["umask"] = "%03o" % os.umask(0o022)
    os.umask(int(identity["umask"], 8))
    try:
        import resource
        identity["rlimits"] = {name: list(resource.getrlimit(getattr(resource, "RLIMIT_" + name)))
                               for name in ("NOFILE", "NPROC", "STACK", "AS", "DATA", "CPU", "CORE", "MEMLOCK")
                               if hasattr(resource, "RLIMIT_" + name)}
    except ImportError:
        pass
    try:
        status = Path("/proc/self/status").read_text()
        identity["no_new_privs"] = any(line.split() == ["NoNewPrivs:", "1"] for line in status.splitlines())
    except OSError:
        pass
    return identity


def _require_unprivileged() -> None:
    """The runner runs as the collector account only: no real, effective or saved uid or gid 0, no
    supplementary group 0, and (Linux) no capability in any set."""
    ids = [os.getuid(), os.geteuid(), os.getgid(), os.getegid()]
    if hasattr(os, "getresuid"):
        ids += list(os.getresuid()) + list(os.getresgid())
    if 0 in ids or 0 in os.getgroups():
        raise ValueError("the box runner runs as the collector account, never as root or group root")
    try:
        status = Path("/proc/self/status").read_text()
    except OSError:
        return  # not Linux
    for line in status.splitlines():
        key, _, value = line.partition(":")
        if key in ("CapEff", "CapPrm", "CapInh", "CapAmb") and int(value.strip(), 16):
            raise ValueError(f"the box runner holds capabilities ({key} {value.strip()}): drop them at launch")


def format_cpus(cpus: list[int]) -> str:
    """1,2,3,7 -> "1-3,7"."""
    runs, start = [], None
    for i, cpu in enumerate(cpus):
        if start is None:
            start = cpu
        if i + 1 == len(cpus) or cpus[i + 1] != cpu + 1:
            runs.append(str(start) if start == cpu else f"{start}-{cpu}")
            start = None
    return ",".join(runs)


def _flock_holder(path_fd: int):
    """Who holds an exclusive flock on the open file `path_fd`, from /proc/locks (never by trying one: a probe
    could make a starting runner's own lock fail): ("held", pid), ("free", None), or ("unknown", None) when it
    cannot be told (no /proc/locks, a file that is not a singly linked regular file, or only an inode-number
    match whose device differs, as on btrfs subvolumes or FUSE, where /proc and fstat disagree)."""
    info = os.fstat(path_fd)
    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
        return "unknown", None
    want = "%02x:%02x:%d" % (os.major(info.st_dev), os.minor(info.st_dev), info.st_ino)
    try:
        lines = Path("/proc/locks").read_text().splitlines()
    except OSError:
        return "unknown", None
    inode_only = False
    for line in lines:
        fields = line.split()  # "1: FLOCK ADVISORY WRITE <pid> <maj:min:ino> 0 EOF"; waiters carry "->"
        if len(fields) != 8 or fields[1:4] != ["FLOCK", "ADVISORY", "WRITE"] or not fields[4].isdigit():
            continue
        if fields[5] == want:
            return "held", int(fields[4])
        if fields[5].rsplit(":", 1)[-1] == str(info.st_ino):
            inode_only = True
    return ("unknown", None) if inode_only else ("free", None)


def _open_under_root(path: str) -> int:
    """A directory descriptor for the canonical absolute `path`, opened one component at a time from "/" without
    following any symlink, so no ancestor can be swapped for a link between check and use."""
    flags = os.O_RDONLY | NOFOLLOW | getattr(os, "O_DIRECTORY", 0)
    fd = os.open("/", flags)
    try:
        for part in [p for p in path.split("/") if p]:
            child = os.open(part, flags, dir_fd=fd)
            os.close(fd)
            fd = child
    except BaseException:
        os.close(fd)
        raise
    return fd


def runner_check(config_path: str, max_age: float | None, stdout) -> int:
    """`pooled_collect.py runner-check CONFIG [MAX_AGE_SECONDS]`: a diagnostic of a box runner's heartbeat. It
    does not read the chain's freezer, so root's timer uses runner-watch, which does. Safe as root on a scratch
    root the collector account owns:
      the config is opened without following a symlink, bounded, a regular file nobody else can write, owned
      by root or by the scratch root's owner (by this account when not root); only its scratch and executor
      fields are read and type checked;
      the scratch root (canonical, opened component by component from "/" without following symlinks) and its
      spool must belong to one account, the spool with mode 700; the heartbeat is read relative to them;
      whether a runner holds the spool's lock is read from /proc/locks by device and inode, exclusive locks
      only, never by taking one.
    Exit 0 fresh; 1 stale while a runner (lock_pid) holds the lock: hung or frozen; 3 nothing to judge (no
    spool or heartbeat yet, or no runner holds the lock: starting, stopped or dead, which w5box handles); 6 the
    lock's holder cannot be told (unknown: never a kill); 2 unusable (a missing config or scratch included)."""
    try:
        fd = os.open(config_path, os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0))
        with os.fdopen(fd, "rb") as handle:
            info = os.fstat(handle.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_mode & 0o022:
                raise ValueError("the config must be a regular file that no group or other account can write")
            config_uid, data = info.st_uid, handle.read((1 << 20) + 1)
        if len(data) > 1 << 20:
            raise ValueError("the config exceeds 1 MiB")
        config = json.loads(data)
        if not isinstance(config, dict) or config.get("schema") != AGENT_SCHEMA:
            raise ValueError("not an agent config")
        executor, scratch = config.get("executor"), config.get("scratch")
        if not isinstance(executor, dict) or executor.get("kind") != "spool":
            raise ValueError("not an agent config with the spool executor")
        if not isinstance(scratch, str) or _canonical(scratch) != scratch:
            raise ValueError("the config's scratch is not a canonical absolute path without symlinks")
        beat = executor.get("heartbeat_seconds", 10)
        if type(beat) is not int or beat < 1:
            raise ValueError("the executor's heartbeat_seconds must be an integer of at least 1")
        limit = float(max_age if max_age is not None else beat)
        if not (math.isfinite(limit) and limit > 0):
            raise ValueError("the age limit must be a positive number")
        scratch_fd = _open_under_root(scratch)
        try:
            account = os.fstat(scratch_fd).st_uid
            me = os.geteuid()
            if (me == 0 and config_uid not in (0, account)) or (me != 0 and not config_uid == account == me):
                raise ValueError("the config's owner is neither root nor the scratch root's owner")
            try:
                spool_fd = os.open(SPOOL_DIR, os.O_RDONLY | NOFOLLOW | getattr(os, "O_DIRECTORY", 0), dir_fd=scratch_fd)
            except FileNotFoundError:
                stdout.write(json.dumps({"state": "stopped", "reason": "no spool yet"}) + "\n")
                return 3
        finally:
            os.close(scratch_fd)
        try:
            spool_info = os.fstat(spool_fd)
            if spool_info.st_uid != account or spool_info.st_mode & 0o077:
                raise ValueError("the spool must belong to the scratch root's owner with mode 700")
            report = {"heartbeat": os.path.join(scratch, SPOOL_DIR, "runner.heartbeat"), "limit_seconds": limit}
            try:
                lock_fd = os.open("runner.lock", os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0),
                                  dir_fd=spool_fd)
            except FileNotFoundError:
                holder, lock_pid = "free", None
            else:
                try:
                    holder, lock_pid = _flock_holder(lock_fd)
                finally:
                    os.close(lock_fd)
            report.update(runner_lock=holder, lock_pid=lock_pid)
            try:
                info = os.stat("runner.heartbeat", dir_fd=spool_fd, follow_symlinks=False)
            except FileNotFoundError:
                report["state"] = {"held": "starting", "free": "stopped"}.get(holder, "unknown")
                stdout.write(json.dumps(report) + "\n")
                return 6 if holder == "unknown" else 3
            age = time.time() - info.st_mtime
            fresh = stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and age <= limit
            state = "fresh" if fresh else {"held": "stale", "free": "stopped"}.get(holder, "unknown")
            report.update(age_seconds=round(age, 3), state=state)
            try:
                fd = os.open("runner.heartbeat", os.O_RDONLY | NOFOLLOW | getattr(os, "O_NONBLOCK", 0),
                             dir_fd=spool_fd)
                with os.fdopen(fd, "rb") as handle:
                    content = json.loads(handle.read(SPOOL_FILE_BYTES)) if stat.S_ISREG(
                        os.fstat(handle.fileno()).st_mode) else {}
                if isinstance(content, dict):
                    report.update({k: content[k] for k in ("pid", "generation", "w5_chain", "w5_box_slot")
                                   if isinstance(content.get(k), (str, int)) and not isinstance(content.get(k), bool)})
            except (OSError, ValueError, RecursionError):
                pass
        finally:
            os.close(spool_fd)
    except (OSError, ValueError, TypeError, RecursionError) as error:
        stdout.write(json.dumps({"state": "unusable", "error": f"{type(error).__name__}: {str(error)[:200]}"}) + "\n")
        return 2
    stdout.write(json.dumps(report) + "\n")
    return {"fresh": 0, "stale": 1, "stopped": 3}.get(state, 6)


def _close_inherited() -> None:
    """Closes every descriptor above stderr (listed from /proc/self/fd, else up to the descriptor limit)."""
    try:
        fds = [int(name) for name in os.listdir("/proc/self/fd")]
    except OSError:
        import resource
        soft = resource.getrlimit(resource.RLIMIT_NOFILE)[0]
        fds = range(3, soft if soft > 0 and soft != resource.RLIM_INFINITY else 1 << 20)
    for fd in fds:
        if fd > 2:
            try:
                os.close(fd)
            except OSError:
                pass  # the listing's own descriptor, already closed


def _chain_state(cgroup: str, lock_pid) -> str:
    """The runner's chain cgroup as root's watch needs it: "frozen" (`cgroup.freeze` 1 or `frozen 1` in
    `cgroup.events`), "foreign" (the spool's lock holder is not one of its processes: never killed through this
    chain), "running", or "unknown" when its files cannot be read (a wrong path included: never a kill)."""
    try:
        freeze = Path(cgroup, "cgroup.freeze").read_text().strip()
        events = Path(cgroup, "cgroup.events").read_text().splitlines()
        procs = {int(x) for x in Path(cgroup, "cgroup.procs").read_text().split()}
    except (OSError, ValueError):
        return "unknown"
    if freeze == "1" or any(line.split() == ["frozen", "1"] for line in events):
        return "frozen"
    return "running" if lock_pid in procs else "foreign"


def runner_watch(argv: list[str], stdout) -> int:
    """`pooled_collect.py runner-watch CONFIG --plan PLAN --chain CHAIN --cgroup DIR [--max-age S]
    [--confirm-seconds S]`: the one-shot, stateless command root's timer runs (say every minute, MAX_AGE a few
    heartbeat periods), with DIR the runner's w5box chain cgroup (`<cgroup_root>/box<k>/chain`). It acts on
    nothing itself. Exit 1 only when two readings CONFIRM seconds apart (at least 1, default 5) both find the
    heartbeat stale while a process of that chain holds the spool's runner lock and the chain is not frozen; it
    then prints the JSON report with `kill_command`, w5box's kill of that chain, which root runs. Otherwise:
    0 fresh, 3 nothing to judge (starting, stopped, dead), 4 stale but frozen (a w5box pause, a node or outage
    freeze), 5 stale once but not on the confirming reading (a thaw), 6 unknown (the lock's holder, its chain
    or the freezer cannot be read, or the holder is not in the chain), 2 unusable. Root's timer, for example:
        python3 -I pooled_collect.py runner-watch A.json --plan P.json --chain C --cgroup G > watch.json
        [ $? -eq 1 ] && python3 -I w5box.py kill --plan P.json --chain C
    Root runs root-owned copies of this script, w5box.py and the plan."""
    import argparse
    parser = argparse.ArgumentParser(prog="pooled_collect.py runner-watch")
    parser.add_argument("config")
    parser.add_argument("--plan", required=True)
    parser.add_argument("--chain", required=True)
    parser.add_argument("--cgroup", required=True)
    parser.add_argument("--max-age", type=float, default=None)
    parser.add_argument("--confirm-seconds", type=float, default=5.0)
    try:
        args = parser.parse_args(argv)
    except SystemExit:
        return 2
    if (args.max_age is not None and not (math.isfinite(args.max_age) and args.max_age > 0)) \
            or not (math.isfinite(args.confirm_seconds) and 1 <= args.confirm_seconds <= 600):
        return 2

    def reading():
        report = io.StringIO()
        code = runner_check(args.config, args.max_age, report)
        try:
            result = json.loads(report.getvalue())
        except ValueError:
            result = {}
        if code == 1:
            chain = _chain_state(args.cgroup, result.get("lock_pid"))
            result["chain"] = chain
            if chain == "frozen":
                result["state"], code = "stale-frozen", 4
            elif chain != "running":
                result["state"], code = f"stale-chain-{chain}", 6
        return code, result

    code, result = reading()
    if code == 1:  # confirm: a reading just after a thaw must not kill
        time.sleep(args.confirm_seconds)
        code, result = reading()
        if code != 1:
            result["first_reading"], code = "stale", (code if code in (2, 4, 6) else 5)
    if code == 1:
        here = os.path.dirname(os.path.realpath(__file__))
        result["kill_command"] = [sys.executable, "-I", os.path.join(here, "w5box.py"), "kill",
                                  "--plan", args.plan, "--chain", args.chain]
    stdout.write(json.dumps(result) + "\n")
    return code


def runner_main(config_path: str, stderr) -> int:
    """`pooled_collect.py runner CONFIG`, started by root inside a box as the collector account. Exit 0 after
    the stop file or a signal, 1 on failure, 2 for an unusable config or a privileged launch. The end record is
    written by the owner of the spool's runner lock only, before it releases it; a runner that fails before
    owning it (sandbox, another runner) reports on stderr and through its exit code."""
    _close_inherited()
    try:
        _require_unprivileged()
        config = load_agent_config(Path(config_path))
        if config["executor"]["kind"] != "spool":
            raise ValueError("the box runner needs the agent config's executor {\"kind\": \"spool\"}")
    except Exception as error:  # noqa: BLE001
        stderr.write(f"refused: {type(error).__name__}: {str(error)[:300]}\n")
        return 2
    identity, runner, code, previous = _runner_identity(), None, 1, {}
    try:
        import select  # noqa: F401  loaded before the sandbox closes everything but its rules
        config["sandbox_effective"] = agent_sandbox(config, runner_rules(config))  # before any thread or child
        runner = BoxRunner(config, dict(identity, sandbox=config["sandbox_effective"]))

        def stop(*_):
            runner.signalled = True
        for name in ("SIGTERM", "SIGINT", "SIGHUP"):
            number = getattr(signal, name)
            previous[number] = signal.signal(number, stop)
        runner.serve()
        code = 0
    except RunnerBusy as error:
        stderr.write(f"refused: {error}\n")
        return 1
    except Exception as error:  # noqa: BLE001  never a traceback
        stderr.write(f"failed: {type(error).__name__}: {str(error)[:300]}\n")
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)
    if runner is not None and runner.end_error is not None:
        stderr.write(f"failed: end record not written: {type(runner.end_error).__name__}: "
                     f"{str(runner.end_error)[:200]}\n")
        code = code or 1
    return code


def agent_serve(config: dict, encoded, stdin, stdout) -> None:
    """Serves one request; raises AgentRefusal for anything outside the seven operations and their
    confinement. Results go to `stdout` (JSON, or a tar for fetch_shard)."""
    op, args = decode_request(encoded)
    scratch = config["scratch"]

    def reply(value) -> None:
        stdout.write((json.dumps(value) + "\n").encode())

    if op == "pin_status":
        pins = args["pins"]
        if not isinstance(pins, list) or len(pins) > 4096 or \
                not all(isinstance(p, list) and len(p) == 2 for p in pins):
            raise AgentRefusal("pins must be a list of at most 4096 [path, sha256]")
        checked = [_pin_arg({"path": p[0], "sha256": p[1]}, config) for p in pins]
        _no_more_input(stdin)
        states = []
        for path, digest in checked:
            found = _hash_nofollow(path)
            states.append("absent" if found is None else "ok" if found == digest else "differs")
        reply({"states": states, "storage": _storage_marker(scratch),
               "sandbox": config.get("sandbox_effective", "none")})
    elif op == "push":
        path, digest = _pin_arg(args, config)
        _require_floor(config, [scratch, _existing_ancestor(path)])
        parent = os.path.dirname(path)
        os.makedirs(parent, mode=0o755, exist_ok=True)
        if _canonical(path) is None:
            raise AgentRefusal("path changed under the pin roots")
        stage = os.path.join(parent, f".{os.path.basename(path)}.{uuid.uuid4().hex}.stage")
        try:
            if _write_new(stage, stdin, AGENT_PIN_BYTES, floor=config) != digest:
                raise AgentRefusal("payload differs from its sha256")
            with _lock(config, path):
                # Ownership follows the installed inode: the witness is linked from the verified
                # stage before publication and renamed into place only after this call created the
                # pin. Best effort: another filesystem than the scratch root, or Landlock ABI 1 (no
                # cross-directory links), installs the pin unowned, so it is never pruned.
                witness = owned_witness(path, scratch)
                pending = f"{witness}.{uuid.uuid4().hex}.pending"
                try:
                    os.makedirs(os.path.dirname(witness), mode=0o700, exist_ok=True)
                    os.link(stage, pending)
                except OSError:
                    pending = None
                try:
                    try:
                        os.link(stage, path)
                    except FileExistsError:
                        if _hash_nofollow(path) != digest:
                            raise AgentRefusal("existing pin differs") from None
                        outcome = "existing"
                    else:
                        outcome = "created-unowned"
                        if pending is not None:
                            os.replace(pending, witness)  # replaces only a stale witness
                            pending, outcome = None, "created"
                finally:
                    if pending is not None:
                        os.unlink(pending)
                if _canonical(path) is None or _hash_nofollow(path) != digest:
                    raise AgentRefusal("pin path changed while installing")
            reply(outcome)
        finally:
            if os.path.lexists(stage):
                os.unlink(stage)
    elif op == "put_shard":
        tag = _tag_arg(args)
        _require_floor(config, [scratch, *config["pin_roots"]])
        data = _read_payload(stdin, AGENT_SHARD_BYTES)
        _valid_shard_command(data, tag, config)
        _write_new(os.path.join(scratch, f"{tag}.json"), data, AGENT_SHARD_BYTES)
        reply("stored")
    elif op == "run_shard":
        tag = _tag_arg(args)
        expected, seconds = args["collector_sha256"], args["seconds"]
        if expected != config["collector_sha256"]:
            raise AgentRefusal("the learner expects another collector than this agent runs")
        if not isinstance(seconds, int) or isinstance(seconds, bool) or seconds < 1:
            raise AgentRefusal("seconds must be a positive integer")
        # The remote deadline is the learner's or this agent's cap, whichever is sooner; the learner
        # waits longer than its own request, so the collector is always stopped before it gives up.
        seconds = min(seconds, config["max_seconds"])
        _no_more_input(stdin)
        data = _read_nofollow(os.path.join(scratch, f"{tag}.json"), AGENT_SHARD_BYTES)
        command = _valid_shard_command(data, tag, config)  # again, just before use
        if _hash_nofollow(config["collector"]) != config["collector_sha256"]:
            raise AgentRefusal("collector binary differs from its pin")
        spool = config.get("executor", {}).get("kind") == "spool"
        if spool:  # no live box runner: refused before anything is created, so the learner fails over at once
            problem = _runner_problem(config, starting=True)
            if problem:
                raise AgentRefusal(problem)
        _require_floor(config, [scratch, *config["pin_roots"]])
        with _run_slot(config):  # held until the collector is reaped; no free slot refuses
            # With the spool executor the job lock is taken first: from before the run directory exists until
            # this agent leaves, remove_shard refuses the tag and the runner sees a waiting agent.
            admitted = _spool_admit(config, tag) if spool else None
            try:
                # `<scratch>/<tag>` is the collector's run directory, the only place it may write. It exists
                # before the collector is confined (a Landlock rule needs an existing directory) and the
                # collector refuses an existing output directory, so it publishes into `<tag>/out`, which it
                # creates. Its recorded paths are its own: the merge binds only names and hashes.
                run_dir = os.path.join(scratch, tag)
                try:
                    os.mkdir(run_dir, 0o700)
                except FileExistsError:
                    raise AgentRefusal("this tag's shard output already exists (remove_shard first)") from None
                command["output_directory"] = os.path.join(run_dir, "out")
                # Run a private copy of the validated command, so the file the collector reads is the one checked.
                if spool:
                    private_dir = _private_dir(config, ".run")
                else:
                    private_dir = os.path.join(scratch, ".run")
                    os.makedirs(private_dir, mode=0o700, exist_ok=True)
                job = uuid.uuid4().hex  # the spool executor's job id
                private = os.path.join(private_dir, f"{tag}.{job}.json")
                body = json.dumps(command).encode()
                if len(body) > AGENT_SHARD_BYTES:
                    raise AgentRefusal("shard command exceeds its bound once serialized")
                _require_floor(config, [private_dir])
                body_sha256 = _write_new(private, body, AGENT_SHARD_BYTES)
                try:
                    if spool:  # the same command, run by the box runner on the box's CPUs
                        code, tail = _spool_run(config, admitted, job, body_sha256, seconds)
                    else:
                        code, tail = _run_collector(config, private, run_dir, seconds)
                    reply({"code": code, "stderr": tail})
                finally:
                    if spool:
                        with _alarm_deferred():
                            os.unlink(private)
                    else:
                        os.unlink(private)
            finally:
                if admitted is not None:
                    with _alarm_deferred():
                        _spool_release(admitted)
    elif op == "fetch_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
        if config.get("executor", {}).get("kind") == "spool":
            _spool_fetch(config, tag, stdout)
            return
        directory = os.path.join(scratch, tag, "out")
        for level in (os.path.dirname(directory), directory):
            try:
                info = os.lstat(level)
            except FileNotFoundError:
                raise AgentRefusal("no shard output for this tag") from None
            if not stat.S_ISDIR(info.st_mode):
                raise AgentRefusal("shard output is not a directory")
        with tarfile.open(fileobj=stdout, mode="w|") as archive:
            for name in sorted(os.listdir(directory)):
                member = os.path.join(directory, name)
                if not _portable_name(name):
                    raise AgentRefusal(f"unexpected name in shard output: {name[:80]!r}")
                try:
                    fd = os.open(member, os.O_RDONLY | NOFOLLOW | BINARY)
                except OSError:  # ELOOP: a symlink in the shard output
                    raise AgentRefusal(f"shard output {name!r} is not a singly linked regular file") from None
                with os.fdopen(fd, "rb") as handle:
                    info = os.fstat(handle.fileno())
                    if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or os.path.islink(member):
                        raise AgentRefusal(f"shard output {name!r} is not a singly linked regular file")
                    entry = tarfile.TarInfo(name)
                    entry.size, entry.mode, entry.mtime = info.st_size, 0o644, 0
                    archive.addfile(entry, handle)
    elif op == "remove_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
        if config.get("executor", {}).get("kind") == "spool":
            _spool_remove(config, tag)  # refuses while a job of the tag is active
            reply("removed")
            return
        directory = os.path.join(scratch, tag)
        if os.path.lexists(directory):
            if os.path.islink(directory) or not os.path.isdir(directory):
                os.unlink(directory)
            else:
                shutil.rmtree(directory)
        command_path = os.path.join(scratch, f"{tag}.json")
        if os.path.lexists(command_path):
            os.unlink(command_path)
        reply("removed")
    elif op == "remove_pin":
        # Deletes only the very file an agent push installed (its ownership witness) with the
        # expected bytes, moved aside first and put back otherwise: "removed" or "not-owned".
        path, digest = _pin_arg(args, config)
        _no_more_input(stdin)
        with _lock(config, path):
            reply(remove_owned_file(path, scratch, digest))


def agent_main(config_path: str, environ, stdin, stdout, stderr) -> int:
    """The forced command's entry point. Exit 0 served, 2 refused, 1 failed. The sandbox applies to
    this whole process from the moment the config is loaded."""
    alarm = None
    try:
        config = load_agent_config(Path(config_path))
        config["sandbox_effective"] = agent_sandbox(config)
        if os.name == "posix" and threading.current_thread() is threading.main_thread():
            # A whole-request deadline, beyond a run's own bound (signals need the main thread).
            def expire(*_):
                raise AgentRefusal("request deadline")
            alarm = signal.signal(signal.SIGALRM, expire)
            signal.alarm(config["request_seconds"] + config["max_seconds"])
        agent_serve(config, environ.get("SSH_ORIGINAL_COMMAND"), stdin, stdout)
        stdout.flush()
        return 0
    except AgentRefusal as error:
        stderr.write(f"refused: {error}\n")
        return 2
    except Exception as error:  # noqa: BLE001  report, never a traceback to the caller
        stderr.write(f"failed: {type(error).__name__}: {str(error)[:300]}\n")
        return 1
    finally:
        if alarm is not None:
            signal.alarm(0)
            signal.signal(signal.SIGALRM, alarm)


def _extract_shard_archive(archive_path: Path, local: Path) -> None:
    """Writes a fetched shard archive's members into `local`: portable plain names only (no Windows
    device or trailing-dot alias), regular files only, no duplicates, each created exclusively and
    checked to be a regular file once open. Nothing else is accepted."""
    seen = set()
    with tarfile.open(archive_path, mode="r:") as archive:
        for member in archive:
            if not member.isreg() or not _portable_name(member.name) or member.name.upper() in seen:
                raise RuntimeError(f"unexpected entry in a shard archive: {member.name[:80]!r}")
            seen.add(member.name.upper())  # case-insensitive learner filesystems: one name, one file
            source = archive.extractfile(member)
            fd = os.open(local / member.name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | NOFOLLOW | BINARY, 0o644)
            with os.fdopen(fd, "wb") as handle:
                if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
                    raise RuntimeError(f"shard archive entry is not a regular file here: {member.name[:80]!r}")
                shutil.copyfileobj(source, handle)


def _bounded_process(argv: list, what: str, seconds: float, out_cap: int, stdin=None, payload=None,
                     sink=None) -> tuple[int, bytes, bytes]:
    """Runs `argv` (one ssh connection) under a monotonic deadline that does not depend on any read:
    stdout and stderr are drained concurrently by threads (no select on pipes, so it works on a Windows
    learner), stdout into `sink` or memory up to `out_cap`, stderr into its last AGENT_STDERR_TAIL
    bytes; a payload is fed by its own thread. Past the deadline or the cap the process is killed and
    reaped and RuntimeError raised. Returns (exit code, stdout, stderr tail)."""
    source = stdin if stdin is not None else subprocess.PIPE if payload else subprocess.DEVNULL
    process = subprocess.Popen(argv, stdin=source, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    out, tail, problems = bytearray(), bytearray(), []

    def pump_out():
        size = 0
        try:
            for block in iter(lambda: process.stdout.read1(1 << 16), b""):
                size += len(block)
                if size > out_cap:
                    problems.append("exceeded its output bound")
                    process.kill()
                    return
                if sink is not None:
                    sink.write(block)
                else:
                    out.extend(block)
        except Exception as error:  # noqa: BLE001  a full disk: stop the transfer
            problems.append(f"output failed: {error}")
            process.kill()

    def feed():
        try:
            process.stdin.write(payload)
        except OSError:
            pass  # the far end stopped reading: its exit code tells
        finally:
            try:
                process.stdin.close()
            except OSError:
                pass

    threads = [threading.Thread(target=pump_out, daemon=True),
               threading.Thread(target=_drain_tail, args=(process.stderr, tail), daemon=True)]
    if payload:
        threads.append(threading.Thread(target=feed, daemon=True))
    for thread in threads:
        thread.start()
    deadline = time.monotonic() + seconds
    try:
        process.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        problems.append(f"timed out after {seconds:g} s")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
    for thread in threads[:2]:
        thread.join(max(0.0, deadline - time.monotonic()) + 2.0)
    if any(thread.is_alive() for thread in threads[:2]):
        problems.append("left its streams open past the deadline")
    else:
        process.stdout.close()
        process.stderr.close()
    if problems:
        raise RuntimeError(f"{what} {problems[0]}")
    return process.returncode, bytes(out), bytes(tail)


class AgentTransport:
    """The dispatcher's side of agent mode: every call is one ssh connection whose only content is an
    encoded request (plus payload on stdin). With the forced command the collector's sshd puts the
    request in SSH_ORIGINAL_COMMAND and runs the agent. `agent_wrap` (proofs over an ordinary login,
    no forced command) is a format string such as
    "SSH_ORIGINAL_COMMAND={req} /usr/bin/python3 -I /srv/.../pooled_collect.py agent /srv/.../agent.json".
    Every call has a deadline independent of reads and bounded output (_bounded_process)."""

    def __init__(self, host: Host):
        self.host = host

    def _argv(self, op: str, **args) -> list[str]:
        request = encode_request(op, **args)
        return self.host.ssh + [self.host.agent_wrap.format(req=request) if self.host.agent_wrap else request]

    def _json(self, op: str, payload=b"", timeout: float = 600, **args):
        stream = None if isinstance(payload, (bytes, bytearray)) else payload
        code, out, err = _bounded_process(self._argv(op, **args), f"{self.host.name}: agent {op}", timeout,
                                          AGENT_REPLY_BYTES, stdin=stream,
                                          payload=payload if stream is None else None)
        if code != 0:
            raise RuntimeError(f"{self.host.name}: agent {op}: {err.decode(errors='replace').strip()[-400:]}")
        return json.loads(out.decode())

    @staticmethod
    def _tag(path: str) -> str:
        name = PurePosixPath(path).name
        return name[:-5] if name.endswith(".json") else name

    def pin_status(self, pins: set[tuple[str, str]]) -> dict:
        ordered = sorted(pins)
        return dict(zip(ordered, self._json("pin_status", pins=[list(p) for p in ordered])["states"]))

    def storage_id(self) -> str:
        return self._json("pin_status", pins=[])["storage"]

    def push(self, local: Path, remote: str, digest: str) -> bool:
        """True only when this call installed the pin and linked its ownership witness."""
        with open(local, "rb") as handle:
            return self._json("push", handle, timeout=1800, path=remote, sha256=digest) == "created"

    def put_text(self, remote: str, text: str) -> None:
        self._json("put_shard", text.encode(), tag=self._tag(remote))

    def run(self, argv: list[str], timeout: float) -> subprocess.CompletedProcess:
        seconds = max(1, int(timeout))
        outcome = self._json("run_shard", timeout=seconds + 120, tag=self._tag(argv[-1]),
                             collector_sha256=self.host.collector_sha256, seconds=seconds)
        code = outcome["code"] if isinstance(outcome["code"], int) else 124
        return subprocess.CompletedProcess(argv, code, "", outcome["stderr"])

    def fetch_dir(self, remote: str, local: Path) -> None:
        if local.exists():
            shutil.rmtree(local)
        local.mkdir(parents=True)
        with tempfile.TemporaryDirectory() as temporary:
            archive_path = Path(temporary) / "shard.tar"
            try:
                with open(archive_path, "wb") as sink:
                    code, _, err = _bounded_process(self._argv("fetch_shard", tag=self._tag(remote)),
                                                    f"{self.host.name}: fetch of {remote}", FETCH_SECONDS,
                                                    AGENT_FETCH_BYTES, sink=sink)
                if code != 0:
                    raise RuntimeError(f"{self.host.name}: agent fetch_shard: {err.decode(errors='replace')[-400:]}")
                _extract_shard_archive(archive_path, local)
            except BaseException:
                shutil.rmtree(local, ignore_errors=True)
                raise

    def remove_dir(self, remote: str) -> None:
        self._json("remove_shard", tag=self._tag(remote))

    def remove_shard(self, out: str, command_path: str) -> None:
        """The agent's remove_shard removes a tag's output directory and command file in one call."""
        self._json("remove_shard", tag=self._tag(out))

    def remove_file(self, remote: str) -> None:
        if AGENT_TAG.fullmatch(self._tag(remote)):
            self._json("remove_shard", tag=self._tag(remote))
        else:
            raise RuntimeError(f"{self.host.name}: agent mode removes inputs only by pin")

    def remove_owned(self, remote: str, digest: str) -> str:
        """The agent's remove_pin: deletes `remote` only if it is still the file an agent push
        installed (its ownership witness) with the expected bytes. "removed" or "not-owned"."""
        outcome = self._json("remove_pin", path=remote, sha256=digest)
        if outcome not in ("removed", "not-owned"):
            raise RuntimeError(f"{self.host.name}: owned removal of {remote}: unexpected reply")
        return outcome


def merge_with_binary(binary: str):
    def run_merge(command_path: Path, directories: list[Path]) -> dict:
        result = subprocess.run([binary, "--merge-collection-shards", str(command_path)] + [str(d) for d in directories],
                                capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"merge failed: {result.stderr.strip()[-800:]}")
        merged = json.loads(result.stdout)
        return {"trajectories": len(merged["trajectories"]),
                "behavior_state_sha256": merged["behavior_state_sha256"]}
    return run_merge


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[1] == "agent":
        return agent_main(argv[2], os.environ, sys.stdin.buffer, sys.stdout.buffer, sys.stderr)
    if len(argv) == 3 and argv[1] == "runner":
        return runner_main(argv[2], sys.stderr)
    if len(argv) in (3, 4) and argv[1] == "runner-check":
        try:
            max_age = float(argv[3]) if len(argv) == 4 else None
        except ValueError:
            max_age = -1.0
        if max_age is not None and not (math.isfinite(max_age) and max_age > 0):
            print("runner-check: MAX_AGE_SECONDS must be a positive number", file=sys.stderr)
            return 2
        return runner_check(argv[2], max_age, sys.stdout)
    if len(argv) >= 3 and argv[1] == "runner-watch":
        return runner_watch(argv[2:], sys.stdout)
    if len(argv) != 2:
        print("usage: pooled_collect.py ATTEMPT/collect-command.json (hosts file in MTG_POOLED_COLLECT_CONFIG)"
              " | pooled_collect.py agent AGENT_CONFIG.json (request in SSH_ORIGINAL_COMMAND)"
              " | pooled_collect.py runner AGENT_CONFIG.json (box runner for the executor \"spool\";"
              " stops on its stop file)"
              " | pooled_collect.py runner-check AGENT_CONFIG.json [MAX_AGE_SECONDS] (exit 1 stale, 3 absent)"
              " | pooled_collect.py runner-watch AGENT_CONFIG.json --plan PLAN --chain CHAIN --cgroup DIR"
              " [--max-age S] [--confirm-seconds S] (exit 1: root kills the chain)",
              file=sys.stderr)
        return 2
    config_path = os.environ.get("MTG_POOLED_COLLECT_CONFIG")
    if not config_path:
        print("MTG_POOLED_COLLECT_CONFIG is not set", file=sys.stderr)
        return 2
    config = load_hosts(Path(config_path))
    dispatcher = Dispatcher(config)
    try:
        receipt = dispatcher.collect(Path(argv[1]), merge_with_binary(config["merge_binary"]))
    except Exception as error:  # noqa: BLE001  the trainer's attempt fails closed
        print(f"pooled collection failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({k: receipt[k] for k in ("iteration", "episodes", "collection_seconds", "failed_hosts")}),
          file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
