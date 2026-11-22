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
"""
from __future__ import annotations

import base64
import hashlib
import json
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
import uuid
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath, PureWindowsPath

SCHEMA = "mtg-kernel-pooled-collect-hosts/v1"
FETCH_SECONDS = 900.0


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
        found.add((name, sha256_file(Path(name))))
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
        if pin not in top and not (path.is_file() and sha256_file(path) == pin[1]):
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
                        "ok" if Path(p[0]).is_file() and sha256_file(Path(p[0])) == p[1] else "differs")
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
        self.pushed: dict[str, dict] = {}  # host -> {path: sha256} this process uploaded
        self._lock = threading.Lock()
        self._input_locks: dict[tuple[str, str], threading.Lock] = {}

    def note(self, **event):
        event["t"] = round(time.time(), 3)
        with self._lock:
            self.log.append(event)

    def distribute(self, host: Host, command: dict) -> int:
        """Makes every pin of `command` (and the pins nested in its small JSON inputs) present on
        `host`; returns bytes pushed. A file at a pinned path with other bytes is never replaced:
        the host fails and its shard moves on. One upload per host and input at a time."""
        transport = self.transports[host.name]
        pushed = 0
        for pin, state in sorted(transport.pin_status(input_closure(command)).items()):
            if state == "ok":
                continue
            if state == "differs":
                raise RuntimeError(f"{host.name}: {pin[0]} exists with other bytes; not replaced")
            with self._lock:
                lock = self._input_locks.setdefault((host.name, pin[0]), threading.Lock())
            with lock:
                if transport.pin_status({pin})[pin] == "ok":  # another thread uploaded it
                    continue
                local = Path(pin[0])
                if not local.is_file() or sha256_file(local) != pin[1]:
                    raise RuntimeError(f"learner lacks pinned input {pin[0]}")
                created = transport.push(local, pin[0], pin[1])
                pushed += local.stat().st_size
                if created and host.transport == "agent":  # ownership exists only on agent hosts
                    with self._lock:
                        self.pushed.setdefault(host.name, {})[pin[0]] = pin[1]
        return pushed

    @staticmethod
    def owned_dir(host: Host, path: str) -> str:
        """Every remote directory this dispatcher deletes lies directly under the host's scratch
        root and carries the `pooled-` tag."""
        if PurePosixPath(path).parent != PurePosixPath(host.scratch) or \
                not PurePosixPath(path).name.startswith("pooled-"):
            raise RuntimeError(f"{host.name}: {path} is not a pooled scratch directory")
        return path

    def run_shard(self, host: Host, command: dict, tag: str, staging: Path) -> Path:
        transport = self.transports[host.name]
        # The collector is pinned per host and never pushed: a host without its
        # pinned binary fails, and its shard moves on.
        collector = (host.collector, host.collector_sha256)
        # In agent mode the learner's expected collector hash travels with run_shard and the agent
        # refuses a mismatch; its pin_status answers only for the lineage's pin roots.
        if not isinstance(transport, AgentTransport) and transport.pin_status({collector})[collector] != "ok":
            raise RuntimeError(f"{host.name}: collector binary missing or not its pin")
        out = self.owned_dir(host, command["output_directory"])
        transport.remove_dir(out)
        pushed = self.distribute(host, command)
        command_path = str(PurePosixPath(host.scratch) / f"{tag}.json")
        try:
            transport.put_text(command_path, json.dumps(command))
            started = time.time()
            result = transport.run(host.prefix + [host.collector, "--execute-collect-shard", command_path],
                                   self.timeout)
            if result.returncode != 0:
                raise RuntimeError(f"{host.name}: shard {tag} exit {result.returncode}: {result.stderr[-400:]}")
            played = time.time() - started
            local = staging / tag
            transport.fetch_dir(out, local)
            try:
                transport.remove_dir(out)
            except Exception as error:  # noqa: BLE001  the shard is fetched: cleanup never reruns it
                self.note(event="scratch-cleanup-failed", host=host.name, tag=tag, error=str(error)[-400:])
        finally:
            try:
                transport.remove_file(command_path)
            except Exception as error:  # noqa: BLE001  never replaces the shard's own outcome
                self.note(event="scratch-cleanup-failed", host=host.name, tag=tag, error=str(error)[-400:])
        self.note(event="shard", host=host.name, tag=tag, first=command["shard"]["first_slot"],
                  episodes=len(command["episodes"]), seconds=round(played, 3), pushed_bytes=pushed,
                  fetched_bytes=sum(p.stat().st_size for p in local.iterdir()))
        return local

    def run_with_failover(self, start: int, whole: dict, first: int, count: int, tag: str,
                          staging: Path, failed: set) -> tuple[Path, Host]:
        """Runs one shard on host `start`, else the next live host. A rerun gives the same
        bytes, so failover never changes the collection."""
        errors = []
        for step in range(len(self.hosts)):
            host = self.hosts[(start + step) % len(self.hosts)]
            with self._lock:
                if host.name in failed:
                    continue
            output = str(PurePosixPath(host.scratch) / tag)
            command = shard_command_for(whole, first, count, host.workers, output)
            try:
                return self.run_shard(host, command, tag, staging), host
            except Exception as error:  # noqa: BLE001  any host failure moves the shard
                errors.append(f"{host.name}: {error}")
                self.note(event="host-failed", host=host.name, tag=tag, error=str(error)[-400:])
                with self._lock:
                    failed.add(host.name)
        raise RuntimeError(f"shard {tag} failed on every host: {errors}")

    def collect(self, command_path: Path, run_merge) -> dict:
        command_path = Path(command_path)
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
        started = time.time()
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join()
        if errors:
            raise RuntimeError("; ".join(errors))
        collected = time.time() - started
        self.audit(whole, iteration, jobs, results, staging, tag_root, failed)
        directories = [results[index][0] for index, _, _ in jobs]
        merged = run_merge(command_path, directories)
        for directory in directories:  # the merge copied every trajectory into collect/
            shutil.rmtree(directory)
        staging.rmdir()
        receipt = {"schema": "mtg-kernel-pooled-collect-dispatch/v1", "iteration": iteration,
                   "episodes": total, "shards": [[i, f, s, results[i][1].name] for i, f, s in jobs],
                   "collection_seconds": round(collected, 3), "failed_hosts": sorted(failed),
                   "events": self.log, "merged": merged}
        (staging_root / "dispatch.json").write_text(json.dumps(receipt, indent=1))
        self.prune(input_closure(whole))
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
            return f"{self.storage_key(host)}@{self.transports[host.name].storage_id()}"
        except Exception as error:  # noqa: BLE001
            self.note(event="storage-marker-unreadable", host=host.name, error=str(error)[-200:])
            return None

    def _prune_locked(self, state_path: Path, current: set) -> None:
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        keep_paths = {p for p, _ in current}
        for host in self.hosts:
            key = self.storage_key_now(host) if host.prune_pushed and host.transport == "agent" else self.storage_key(host)
            if key is None:
                continue
            previous = state.get(key, {})
            if not isinstance(previous, dict):
                previous = {}  # an older state format holds no hashes: forget, never delete
            now = self.pushed.get(host.name, {})
            merged = dict(previous) | now
            if host.prune_pushed and host.transport == "agent":
                for path in sorted(set(merged) - set(now) - keep_paths):
                    # Delete only the very file this dispatcher installed (its ownership witness).
                    try:
                        outcome = self.transports[host.name].remove_owned(path, merged[path])
                    except Exception as error:  # noqa: BLE001  kept for the next collection
                        self.note(event="prune-failed", host=host.name, path=path, error=str(error)[-200:])
                        continue
                    self.note(event="pruned" if outcome == "removed" else "not-owned", host=host.name, path=path)
                    del merged[path]
            state[key] = dict(sorted(merged.items()))
        stage = state_path.with_name(f"{state_path.name}.{uuid.uuid4().hex}.stage")
        stage.write_text(json.dumps(state, indent=1))
        os.replace(stage, state_path)

    def audit(self, whole, iteration, jobs, results, staging, tag_root, failed):
        audit = self.config.get("audit", {})
        slots = audit_slots(iteration, len(whole["episodes"]), int(audit.get("episodes", 0)),
                            int(audit.get("every", 0)))
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
            tag = f"{tag_root}-audit{slot:04d}"
            command = shard_command_for(whole, slot, 1, host.workers,
                                        str(PurePosixPath(host.scratch) / tag))
            replay = self.run_shard(host, command, tag, staging)
            replayed = trajectory_pins(replay)[0]["sha256"]
            self.note(event="audit", slot=slot, played_on=played_on.name, replayed_on=host.name,
                      equal=replayed == original)
            if replayed != original:
                raise RuntimeError(f"cross-host divergence at slot {slot}: {played_on.name} ({played_on.arch}) "
                                   f"{original} against {host.name} ({host.arch}) {replayed}")
            shutil.rmtree(replay)


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
# treats that host as failed for the collection and moves the shard on.
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
    return rules


def agent_rules(config: dict) -> list:
    return _system_rules(config) + [(root, LL_READ | LL_WRITE, True)
                                    for root in [*config["pin_roots"], config["scratch"]]]


def collector_rules(config: dict, command_file: str, run_dir: str) -> list:
    return (_system_rules(config) + [(root, LL_READ, True) for root in config["pin_roots"]]
            + [(command_file, LL_READ_FILE, True), (run_dir, LL_READ | LL_WRITE, True)])


def agent_sandbox(config: dict) -> str:
    """Applies the configured sandbox to this process; returns the effective one ("none" or
    "landlock-abiN"). Refuses when Landlock is configured and the kernel has none."""
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
    ruleset = landlock_ruleset(agent_rules(config), abi)
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
    return a == b or a.startswith(b + os.sep) or b.startswith(a + os.sep)


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
    config["max_seconds"] = int(config.get("max_seconds", 3600))
    config["request_seconds"] = int(config.get("request_seconds", 900))
    for key, default, least in (("min_free_bytes", AGENT_MIN_FREE_BYTES, 0), ("max_concurrent_runs", 2, 1)):
        config[key] = config.get(key, default)
        if not isinstance(config[key], int) or isinstance(config[key], bool) or config[key] < least:
            raise ValueError(f"agent {key} must be an integer of at least {least}")
    return config


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


def _hash_nofollow(path: str) -> str | None:
    """The sha256 of a regular file opened without following a final symlink; None if absent;
    '' for anything else (a symlink, a directory, a file the sandbox denies)."""
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


def _confined_inputs(command: dict, config: dict) -> None:
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
        found = _hash_nofollow(path)
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
                                   preexec_fn=preexec, cwd=run_dir)  # relative names resolve in its run dir
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
        _require_floor(config, [scratch, *config["pin_roots"]])
        with _run_slot(config):  # held until the collector is reaped; no free slot refuses
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
            private_dir = os.path.join(scratch, ".run")
            os.makedirs(private_dir, mode=0o700, exist_ok=True)
            private = os.path.join(private_dir, f"{tag}.{uuid.uuid4().hex}.json")
            body = json.dumps(command).encode()
            if len(body) > AGENT_SHARD_BYTES:
                raise AgentRefusal("shard command exceeds its bound once serialized")
            _require_floor(config, [private_dir])
            _write_new(private, body, AGENT_SHARD_BYTES)
            try:
                code, tail = _run_collector(config, private, run_dir, seconds)
                reply({"code": code, "stderr": tail})
            finally:
                os.unlink(private)
    elif op == "fetch_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
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
    if len(argv) != 2:
        print("usage: pooled_collect.py ATTEMPT/collect-command.json (hosts file in MTG_POOLED_COLLECT_CONFIG)"
              " | pooled_collect.py agent AGENT_CONFIG.json (request in SSH_ORIGINAL_COMMAND)", file=sys.stderr)
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
