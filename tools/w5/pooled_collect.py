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
import io
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
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


def pins_of(value) -> set[tuple[str, str]]:
    """Every {path, sha256} pin inside a command document."""
    found = set()
    if isinstance(value, dict):
        if set(value) >= {"path", "sha256"} and isinstance(value.get("path"), str):
            found.add((value["path"], value["sha256"]))
        for item in value.values():
            found |= pins_of(item)
    elif isinstance(value, list):
        for item in value:
            found |= pins_of(item)
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


# The ownership witness: run by the push script right after it created the destination.
WITNESS_LINK = ("    w=Path(scratch)/'.pooled-owned'/hashlib.sha256(sys.argv[1].encode()).hexdigest()\n"
                "    w.parent.mkdir(parents=True,exist_ok=True)\n"
                "    w.unlink(missing_ok=True)  # a stale witness: its file was removed by someone else\n"
                "    try:\n"
                "      os.link(dest,w); print('created')\n"
                "    except OSError:\n"
                "      print('created-unowned')\n")

REMOVE_OWNED = """import hashlib,os,stat,sys,uuid
dest,scratch,expected=sys.argv[1:4]
own=os.path.join(scratch,'.pooled-owned')
w=os.path.join(own,hashlib.sha256(dest.encode()).hexdigest())
def ident(p):
  try: s=os.lstat(p)
  except FileNotFoundError: return None
  return (s.st_dev,s.st_ino) if stat.S_ISREG(s.st_mode) else 'other'
def digest(p):
  h=hashlib.sha256()
  with open(p,'rb') as f:
    for b in iter(lambda: f.read(1<<20),b''): h.update(b)
  return h.hexdigest()
mine=ident(w)
if mine is None or mine=='other':
  print('not-owned'); sys.exit(0)
if ident(dest)!=mine:
  os.unlink(w); print('not-owned'); sys.exit(0)
aside=os.path.join(own,'aside-'+uuid.uuid4().hex)
os.rename(dest,aside)
if ident(aside)==mine and digest(aside)==expected:
  os.unlink(aside); os.unlink(w); print('removed'); sys.exit(0)
try:
  os.link(aside,dest); os.unlink(aside)
except FileExistsError:
  os.rename(aside,os.path.join(own,'kept-'+uuid.uuid4().hex))
print('not-owned')
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
        Returns True only when this call created the destination and linked its ownership witness
        (`<scratch>/.pooled-owned/<sha256 of the path>`, a second hard link to the same file); only
        then does the dispatcher own it. The collector's filesystem needs hard links; a pin on another
        filesystem than the scratch root is installed but never owned, so never pruned."""
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
        script = ("import hashlib,os,shutil,sys\n"
                  "from pathlib import Path\n"
                  "dest,stage,expected,scratch=Path(sys.argv[1]),Path(sys.argv[2]),sys.argv[3],sys.argv[4]\n"
                  "def digest(path):\n"
                  "  h=hashlib.sha256()\n"
                  "  with path.open('rb') as f:\n"
                  "    for block in iter(lambda: f.read(1 << 20), b''): h.update(block)\n"
                  "  return h.hexdigest()\n"
                  "dest.parent.mkdir(parents=True,exist_ok=True)\n"
                  "f=stage.open('xb')\n"
                  "try:\n"
                  "  with f: shutil.copyfileobj(sys.stdin.buffer,f)\n"
                  "  if digest(stage)!=expected: sys.exit('staged pin differs: '+str(dest))\n"
                  "  try:\n"
                  "    os.link(stage,dest)\n"
                  "  except FileExistsError:\n"
                  "    if not dest.is_file() or digest(dest)!=expected:\n"
                  "      sys.exit('existing pin differs: '+str(dest))\n"
                  "    print('existing')\n"
                  "  else:\n"
                  + WITNESS_LINK +
                  "finally: stage.unlink(missing_ok=True)\n")
        command = " ".join(shlex.quote(arg) for arg in ("python3", "-c", script, remote, stage_name, digest,
                                                        self.host.scratch))
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

    def remove_owned(self, remote: str, digest: str) -> str:
        """Deletes `remote` only if it is still the file this dispatcher installed: the same file
        (device and inode) as its ownership witness, with the expected bytes. The file is first moved
        aside atomically into the witness directory, checked there and only then deleted; anything else
        is put back. A restored or replaced copy with the same bytes is another file and is kept; a
        witness whose file is gone is dropped. Returns "removed" or "not-owned"."""
        if self.host.kind == "local":
            result = subprocess.run([sys.executable, "-c", REMOVE_OWNED, remote, self.host.scratch, digest],
                                    capture_output=True, text=True, timeout=300)
        else:
            command = " ".join(shlex.quote(a) for a in ("python3", "-c", REMOVE_OWNED, remote, self.host.scratch, digest))
            result = subprocess.run(self._ssh([command]), capture_output=True, text=True, timeout=300,
                                    stdin=subprocess.DEVNULL)
        outcome = result.stdout.strip().splitlines()[-1:] or [""]
        if result.returncode != 0 or outcome[0] not in ("removed", "not-owned"):
            raise RuntimeError(f"{self.host.name}: owned removal of {remote} failed: {result.stderr.strip()[-200:]}")
        return outcome[0]

    def storage_id(self) -> str:
        """The collector storage's generation marker, `<scratch>/.pooled-storage-id`, created once
        with a random value. Replaced storage (a recreated container, a reimaged disk) gets a new one,
        so it inherits no ownership records."""
        script = ("import os,sys,uuid\n"
                  "d=sys.argv[1]; p=os.path.join(d,'.pooled-storage-id')\n"
                  "os.makedirs(d,exist_ok=True)\n"
                  "try:\n"
                  "  fd=os.open(p,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o644); os.write(fd,uuid.uuid4().hex.encode()); os.close(fd)\n"
                  "except FileExistsError: pass\n"
                  "print(open(p).read().strip())\n")
        if self.host.kind == "local":
            result = subprocess.run([sys.executable, "-c", script, self.host.scratch], capture_output=True,
                                    text=True, timeout=60)
        else:
            command = " ".join(shlex.quote(arg) for arg in ("python3", "-c", script, self.host.scratch))
            result = subprocess.run(self._ssh([command]), capture_output=True, text=True, timeout=120,
                                    stdin=subprocess.DEVNULL)
        marker = result.stdout.strip()
        if result.returncode != 0 or not re.fullmatch(r"[0-9a-f]{32}", marker):
            raise RuntimeError(f"{self.host.name}: storage marker unreadable")
        return marker

    def remove_pin(self, path: str, digest: str) -> bool:
        """Deletes the file only while it still holds `digest`'s bytes."""
        if self.pin_status({(path, digest)})[(path, digest)] != "ok":
            return False
        self.remove_file(path)
        return True

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
                if created and host.kind == "ssh":
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
        # In agent mode the agent checks its own configured collector before every run, and its
        # pin_status answers only for the lineage's pin roots.
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
            transport.remove_dir(out)
        finally:
            try:
                transport.remove_file(command_path)
            except (OSError, subprocess.SubprocessError) as error:
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
            key = self.storage_key_now(host) if host.prune_pushed and host.kind == "ssh" else self.storage_key(host)
            if key is None:
                continue
            previous = state.get(key, {})
            if not isinstance(previous, dict):
                previous = {}  # an older state format holds no hashes: forget, never delete
            now = self.pushed.get(host.name, {})
            merged = dict(previous) | now
            if host.prune_pushed and host.kind == "ssh":
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

AGENT_SCHEMA = "mtg-kernel-pooled-collect-agent/v1"
AGENT_REQUEST_SCHEMA = "pooled-collect-agent-request/v1"
AGENT_REQUEST_BYTES = 64 * 1024  # the encoded request in SSH_ORIGINAL_COMMAND
AGENT_SHARD_BYTES = 16 * 1024 * 1024  # a shard command, as the trainer's config bound
AGENT_PIN_BYTES = 4 * 1024 * 1024 * 1024  # one pushed input
AGENT_TAG = re.compile(r"^pooled-[0-9]{6}-[0-9a-f]{32}(-s[0-9]{3}|-audit[0-9]{4})$")
AGENT_OPS = ("pin_status", "push", "put_shard", "run_shard", "fetch_shard", "remove_shard", "remove_pin")


class AgentRefusal(Exception):
    """A request the agent does not serve. Exit 2, nothing done."""


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
    except (ValueError, UnicodeDecodeError) as error:
        raise AgentRefusal(f"malformed request: {error}") from None
    if not isinstance(request, dict) or set(request) != {"schema", "op", "args"} \
            or request["schema"] != AGENT_REQUEST_SCHEMA or not isinstance(request["args"], dict):
        raise AgentRefusal("request is not a pooled-collect-agent-request/v1 object")
    if request["op"] not in AGENT_OPS:
        raise AgentRefusal(f"operation not served: {request['op']!r}")
    return request["op"], request["args"]


def load_agent_config(path: Path) -> dict:
    config = json.loads(Path(path).read_text())
    if config.get("schema") != AGENT_SCHEMA:
        raise ValueError(f"agent config schema is not {AGENT_SCHEMA}")
    for key in ("collector", "collector_sha256", "scratch", "pin_roots", "prefix"):
        if key not in config:
            raise ValueError(f"agent config lacks {key}")
    if not config["pin_roots"] or not all(_canonical(root) == root for root in config["pin_roots"] + [config["scratch"]]):
        raise ValueError("agent roots must be canonical absolute paths")
    return config


def _canonical(path) -> str | None:
    """`path` when it is an absolute, normalized path whose existing part has no symlink; else None."""
    if not isinstance(path, str) or not path or "\x00" in path:
        return None
    if not os.path.isabs(path) or os.path.normpath(path) != path.rstrip("/\\") or path != path.rstrip("/\\"):
        return None
    probe = path
    while probe and not os.path.lexists(probe):
        parent = os.path.dirname(probe)
        if parent == probe:
            break
        probe = parent
    if probe and os.path.realpath(probe) != probe:
        return None
    return path


def _under(path: str, roots) -> bool:
    return any(path == root or path.startswith(root.rstrip("/\\") + os.sep) for root in roots)


def _pin_arg(args: dict, config: dict) -> tuple[str, str]:
    path, digest = args.get("path"), args.get("sha256")
    if _canonical(path) is None or not _under(path, config["pin_roots"]):
        raise AgentRefusal(f"path outside the pin roots: {path!r}")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise AgentRefusal("sha256 must be 64 hex digits")
    return path, digest


def _tag_arg(args: dict) -> str:
    tag = args.get("tag")
    if not isinstance(tag, str) or not AGENT_TAG.fullmatch(tag):
        raise AgentRefusal(f"bad tag: {tag!r}")
    return tag


def _no_more_input(stdin) -> None:
    """One request per connection: an operation without a payload takes no input at all."""
    if stdin.read(1):
        raise AgentRefusal("one request per connection: unexpected input after the request")


def _read_payload(stdin, cap: int) -> bytes:
    data = stdin.read(cap + 1)
    if len(data) > cap:
        raise AgentRefusal("payload exceeds its bound")
    return data


def agent_serve(config: dict, encoded, stdin, stdout) -> None:
    """Serves one request. Raises AgentRefusal for anything outside the seven operations and their
    confinement; writes the result (JSON, or a tar for fetch_shard) to `stdout`."""
    op, args = decode_request(encoded)
    allowed = {"pin_status": {"pins"}, "push": {"path", "sha256"}, "put_shard": {"tag"}, "run_shard": {"tag"},
               "fetch_shard": {"tag"}, "remove_shard": {"tag"}, "remove_pin": {"path", "sha256"}}[op]
    if set(args) != allowed:
        raise AgentRefusal(f"{op} takes exactly {sorted(allowed)}")
    scratch = config["scratch"]

    def reply(value) -> None:
        stdout.write((json.dumps(value) + "\n").encode())

    if op == "pin_status":
        pins = args["pins"]
        if not isinstance(pins, list) or len(pins) > 4096:
            raise AgentRefusal("pins must be a list of at most 4096")
        checked = [_pin_arg({"path": p[0], "sha256": p[1]}, config)
                   if isinstance(p, list) and len(p) == 2 else None for p in pins]
        if None in checked:
            raise AgentRefusal("a pin is not [path, sha256]")
        _no_more_input(stdin)
        reply(["absent" if not os.path.lexists(p) else
               "ok" if os.path.isfile(p) and sha256_file(Path(p)) == d else "differs" for p, d in checked])
    elif op == "push":
        path, digest = _pin_arg(args, config)
        data = _read_payload(stdin, AGENT_PIN_BYTES)
        if hashlib.sha256(data).hexdigest() != digest:
            raise AgentRefusal("payload differs from its sha256")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        stage = f"{path}.{uuid.uuid4().hex}.stage"
        try:
            with open(stage, "xb") as handle:
                handle.write(data)
            try:
                os.link(stage, path)
                reply("created")
            except FileExistsError:
                if not os.path.isfile(path) or sha256_file(Path(path)) != digest:
                    raise AgentRefusal(f"existing pin differs: {path}") from None
                reply("existing")
        finally:
            if os.path.lexists(stage):
                os.unlink(stage)
    elif op == "put_shard":
        tag = _tag_arg(args)
        data = _read_payload(stdin, AGENT_SHARD_BYTES)
        try:
            command = json.loads(data)
        except (ValueError, UnicodeDecodeError):
            raise AgentRefusal("shard command is not JSON") from None
        if not isinstance(command, dict) or command.get("mode") != "collect_parallel" \
                or not isinstance(command.get("shard"), dict) \
                or not isinstance(command.get("output_directory"), str)                 or os.path.normpath(command["output_directory"]) != os.path.join(scratch, tag):
            raise AgentRefusal("not a collect_parallel shard command writing to its own scratch directory")
        os.makedirs(scratch, exist_ok=True)
        with open(os.path.join(scratch, f"{tag}.json"), "xb") as handle:
            handle.write(data)
        reply("stored")
    elif op == "run_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
        command_path = os.path.join(scratch, f"{tag}.json")
        if not os.path.isfile(command_path):
            raise AgentRefusal("no stored shard command for this tag")
        if not os.path.isfile(config["collector"]) or sha256_file(Path(config["collector"])) != config["collector_sha256"]:
            raise AgentRefusal("collector binary differs from its pin")
        result = subprocess.run(list(config["prefix"]) + [config["collector"], "--execute-collect-shard", command_path],
                                stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                timeout=float(config.get("max_seconds", 3600)))
        reply({"code": result.returncode, "stderr": result.stderr.decode(errors="replace")[-4096:]})
    elif op == "fetch_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
        directory = os.path.join(scratch, tag)
        if not os.path.isdir(directory) or os.path.islink(directory):
            raise AgentRefusal("no shard output for this tag")
        with tarfile.open(fileobj=stdout, mode="w|") as archive:
            for name in sorted(os.listdir(directory)):
                member = os.path.join(directory, name)
                if os.path.isfile(member) and not os.path.islink(member):
                    archive.add(member, arcname=name, recursive=False)
    elif op == "remove_shard":
        tag = _tag_arg(args)
        _no_more_input(stdin)
        shutil.rmtree(os.path.join(scratch, tag), ignore_errors=True)
        command_path = os.path.join(scratch, f"{tag}.json")
        if os.path.lexists(command_path):
            os.unlink(command_path)
        reply("removed")
    elif op == "remove_pin":
        path, digest = _pin_arg(args, config)
        _no_more_input(stdin)
        if os.path.isfile(path) and not os.path.islink(path) and sha256_file(Path(path)) == digest:
            os.unlink(path)
            reply("removed")
        else:
            reply("kept")


def agent_main(config_path: str, environ, stdin, stdout, stderr) -> int:
    """The forced command's entry point. Exit 0 served, 2 refused, 1 failed."""
    try:
        config = load_agent_config(Path(config_path))
        agent_serve(config, environ.get("SSH_ORIGINAL_COMMAND"), stdin, stdout)
        stdout.flush()
        return 0
    except AgentRefusal as error:
        stderr.write(f"refused: {error}\n")
        return 2
    except Exception as error:  # noqa: BLE001  report, never a traceback with paths to the caller
        stderr.write(f"failed: {type(error).__name__}: {error}\n")
        return 1


class AgentTransport:
    """The dispatcher's side of agent mode: every call is one ssh connection whose only content is
    an encoded request (plus payload on stdin). With the forced command the collector's sshd puts
    the request in SSH_ORIGINAL_COMMAND and runs the agent. `agent_wrap` (proofs over an ordinary
    login, no forced command) is a format string such as
    "SSH_ORIGINAL_COMMAND={req} /usr/bin/python3 -I /srv/.../pooled_collect.py agent /srv/.../agent.json"."""

    def __init__(self, host: Host):
        self.host = host

    def _call(self, op: str, payload: bytes | None = None, timeout: float = 600, **args):
        request = encode_request(op, **args)
        remote = self.host.agent_wrap.format(req=request) if self.host.agent_wrap else request
        return subprocess.run(self.host.ssh + [remote], input=payload if payload is not None else b"",
                              capture_output=True, timeout=timeout)

    def _json(self, op: str, payload: bytes | None = None, timeout: float = 600, **args):
        result = self._call(op, payload, timeout, **args)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host.name}: agent {op}: {result.stderr.decode(errors='replace').strip()[-400:]}")
        return json.loads(result.stdout.decode())

    @staticmethod
    def _tag(path: str) -> str:
        name = PurePosixPath(path).name
        return name[:-5] if name.endswith(".json") else name

    def pin_status(self, pins: set[tuple[str, str]]) -> dict:
        ordered = sorted(pins)
        states = self._json("pin_status", pins=[list(p) for p in ordered])
        return dict(zip(ordered, states))

    def push(self, local: Path, remote: str, digest: str) -> bool:
        return self._json("push", Path(local).read_bytes(), path=remote, sha256=digest) == "created"

    def put_text(self, remote: str, text: str) -> None:
        self._json("put_shard", text.encode(), tag=self._tag(remote))

    def run(self, argv: list[str], timeout: float) -> subprocess.CompletedProcess:
        outcome = self._json("run_shard", timeout=timeout + 60, tag=self._tag(argv[-1]))
        return subprocess.CompletedProcess(argv, outcome["code"], "", outcome["stderr"])

    def fetch_dir(self, remote: str, local: Path) -> None:
        if local.exists():
            shutil.rmtree(local)
        local.mkdir(parents=True)
        result = self._call("fetch_shard", timeout=FETCH_SECONDS, tag=self._tag(remote))
        if result.returncode != 0:
            shutil.rmtree(local, ignore_errors=True)
            raise RuntimeError(f"{self.host.name}: agent fetch_shard: {result.stderr.decode(errors='replace')[-400:]}")
        with tarfile.open(fileobj=io.BytesIO(result.stdout), mode="r:") as archive:
            for member in archive.getmembers():
                if not member.isfile() or "/" in member.name or member.name in ("", ".", ".."):
                    raise RuntimeError(f"{self.host.name}: unexpected entry in a shard archive: {member.name!r}")
            archive.extractall(local, filter="data") if hasattr(tarfile, "data_filter") else archive.extractall(local)

    def remove_dir(self, remote: str) -> None:
        self._json("remove_shard", tag=self._tag(remote))

    def remove_file(self, remote: str) -> None:
        if AGENT_TAG.fullmatch(self._tag(remote)):
            self._json("remove_shard", tag=self._tag(remote))
        else:
            raise RuntimeError(f"{self.host.name}: agent mode removes inputs only by pin")

    def remove_pin(self, path: str, digest: str) -> bool:
        return self._json("remove_pin", path=path, sha256=digest) == "removed"


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
