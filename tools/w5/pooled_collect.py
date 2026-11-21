#!/usr/bin/env python3
"""Pooled collection dispatcher (pooled-lineage stage 1).

The trainer runs this as its external collector:

    native_expanded_training_run_v1 CONFIG.json --collect-external /abs/pooled_collect.py

with MTG_POOLED_COLLECT_CONFIG naming a hosts file. For each iteration the
trainer publishes `collect-command.json` and calls this program with its path.
The program:

1. splits the command's episodes into contiguous shards, one per host, sized by
   host weight (`pooled_collection_v1 shard`);
2. makes every pinned input of the shard commands (the current checkpoint, the
   play imports, opponents) present at the same absolute path with the same
   SHA-256 on every collector host; that is how the policy reaches collectors;
3. runs each shard with the host's pinned `expanded_deck_training_v1`;
4. copies each shard's output back into a staging directory beside the attempt;
5. on a failed or unreachable host, reruns that shard on the next host (a
   shard's bytes do not depend on where it runs);
6. on audit iterations, replays chosen slots on a host of another architecture
   class and requires identical trajectory bytes;
7. merges the shards into the attempt's `collect/` (`pooled_collection_v1
   merge`), which re-hashes every trajectory and applies the non-natural cap.

The trainer then validates the merged collection exactly as a local one and
replays every decision in the update. Nothing here changes a result: a
failure exits non-zero and the trainer's attempt fails closed.

Paths are one namespace across hosts: a trajectory records the absolute paths
of its sources, so every collector must see the recipe's inputs at the paths
the learner uses (a common root such as /srv/work/pooled-20261120 on every
Linux host, WSL guest and the Mac's Linux VM).
"""
from __future__ import annotations

import hashlib
import json
import os
import shlex
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath, PureWindowsPath

SCHEMA = "mtg-kernel-pooled-collect-hosts/v1"


@dataclass
class Host:
    name: str
    kind: str  # "local" or "ssh"
    collector: str  # absolute path of expanded_deck_training_v1 on that host
    collector_sha256: str
    workers: int
    weight: float
    scratch: str  # absolute scratch root on that host
    arch: str  # architecture class, for audits: e.g. "x86-znver5", "x86-portable", "arm64"
    ssh: list = field(default_factory=list)  # e.g. ["ssh", "-o", "BatchMode=yes", "mtg@10.0.0.37"]
    prefix: list = field(default_factory=list)  # e.g. ["nice", "-n", "19", "taskset", "-c", "1,17"]
    # Delete inputs this dispatcher pushed for an earlier iteration (old checkpoints) once a
    # later collection succeeds. Only for ssh hosts that are not the learner's own machine.
    prune_pushed: bool = False


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


def iteration_of(command_path: Path) -> int:
    """.../iterations/000123/attempt-000000/collect-command.json -> 123."""
    parts = Path(command_path).parts
    for i, part in enumerate(parts[:-1]):
        if part == "iterations" and i + 1 < len(parts):
            return int(parts[i + 1])
    raise ValueError(f"no iteration index in {command_path}")


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

    def run(self, argv: list[str], timeout: float) -> subprocess.CompletedProcess:
        if self.host.kind == "local":
            return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        remote = " ".join(shlex.quote(a) for a in argv)
        return subprocess.run(self.host.ssh + [remote], capture_output=True, text=True,
                              timeout=timeout, stdin=subprocess.DEVNULL)

    def missing_pins(self, pins: set[tuple[str, str]]) -> list[tuple[str, str]]:
        """Pins whose file is absent or differs on the host."""
        if self.host.kind == "local":
            return sorted(p for p in pins if not Path(p[0]).is_file() or sha256_file(Path(p[0])) != p[1])
        script = ("import hashlib,json,os,sys\n"
                  "out=[]\n"
                  "for p,s in json.load(sys.stdin):\n"
                  "  ok=os.path.isfile(p) and hashlib.sha256(open(p,'rb').read()).hexdigest()==s\n"
                  "  out.append(ok)\n"
                  "print(json.dumps(out))\n")
        ordered = sorted(pins)
        result = subprocess.run(self.host.ssh + ["python3 -c " + shlex.quote(script)],
                                input=json.dumps(ordered), capture_output=True, text=True, timeout=300)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host.name}: pin check failed: {result.stderr.strip()}")
        return [p for p, ok in zip(ordered, json.loads(result.stdout)) if not ok]

    def push(self, local: Path, remote: str) -> None:
        """Copies one file to the same absolute path on the host (staged, then renamed)."""
        if self.host.kind == "local":
            if str(Path(local)) != str(Path(remote)):
                Path(remote).parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(local, remote + ".stage")
                os.replace(remote + ".stage", remote)
            return
        parent = str(PurePosixPath(remote).parent)
        stage = remote + ".stage"
        with open(local, "rb") as handle:
            result = subprocess.run(self.host.ssh + [f"mkdir -p {shlex.quote(parent)} && cat > {shlex.quote(stage)}"
                                                     f" && mv {shlex.quote(stage)} {shlex.quote(remote)}"],
                                    stdin=handle, capture_output=True, timeout=600)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host.name}: push of {remote} failed")

    def put_text(self, remote: str, text: str) -> None:
        if self.host.kind == "local":
            Path(remote).parent.mkdir(parents=True, exist_ok=True)
            Path(remote).write_text(text)
            return
        parent = str(PurePosixPath(remote).parent)
        result = subprocess.run(self.host.ssh + [f"mkdir -p {shlex.quote(parent)} && cat > {shlex.quote(remote)}"],
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
        tar = subprocess.Popen(self.host.ssh + [f"tar -C {shlex.quote(remote)} -cf - ."],
                               stdout=subprocess.PIPE, stdin=subprocess.DEVNULL)
        unpack = subprocess.run(["tar", "-C", str(local), "-xf", "-"], stdin=tar.stdout, capture_output=True)
        tar.stdout.close()
        if tar.wait(timeout=600) != 0 or unpack.returncode != 0:
            raise RuntimeError(f"{self.host.name}: fetch of {remote} failed")

    def remove_file(self, remote: str) -> None:
        if self.host.kind == "ssh":
            subprocess.run(self.host.ssh + [f"rm -f -- {shlex.quote(remote)}"], capture_output=True, timeout=120)

    def remove_dir(self, remote: str) -> None:
        if self.host.kind == "local":
            shutil.rmtree(remote, ignore_errors=True)
        else:
            subprocess.run(self.host.ssh + [f"rm -rf -- {shlex.quote(remote)}"], capture_output=True, timeout=300)


def shard_command_for(whole: dict, first: int, count: int, workers: int, output: str) -> dict:
    """The shard command `pooled_collection_v1 shard` writes, built directly (audits and
    reruns need single shards with their own output directory)."""
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
        self.transports = transports or {h.name: Transport(h) for h in self.hosts}
        self.timeout = timeout
        self.log: list[dict] = []
        self.pushed: dict[str, set] = {}
        self._lock = threading.Lock()

    def note(self, **event):
        event["t"] = round(time.time(), 3)
        with self._lock:
            self.log.append(event)

    def distribute(self, host: Host, command: dict) -> int:
        """Makes every pin of `command` present on `host`; returns bytes pushed."""
        transport = self.transports[host.name]
        pushed = 0
        for path, digest in transport.missing_pins(pins_of(command)):
            local = Path(path)
            if not local.is_file() or sha256_file(local) != digest:
                raise RuntimeError(f"learner lacks pinned input {path}")
            transport.push(local, path)
            pushed += local.stat().st_size
            if host.kind == "ssh":
                with self._lock:
                    self.pushed.setdefault(host.name, set()).add(path)
        return pushed

    def run_shard(self, host: Host, command: dict, tag: str, staging: Path) -> Path:
        transport = self.transports[host.name]
        # The collector is pinned per host and never pushed: a host without its
        # pinned binary fails, and its shard moves on.
        if transport.missing_pins({(host.collector, host.collector_sha256)}):
            raise RuntimeError(f"{host.name}: collector binary missing or not its pin")
        out = command["output_directory"]
        transport.remove_dir(out)
        pushed = self.distribute(host, command)
        command_path = str(PurePosixPath(host.scratch) / f"{tag}.json")
        transport.put_text(command_path, json.dumps(command))
        started = time.time()
        result = transport.run(host.prefix + [host.collector, command_path], self.timeout)
        if result.returncode != 0:
            raise RuntimeError(f"{host.name}: shard {tag} exit {result.returncode}: {result.stderr[-400:]}")
        played = time.time() - started
        local = staging / tag
        transport.fetch_dir(out, local)
        transport.remove_dir(out)
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
        if whole.get("mode") != "collect_parallel" or whole.get("shard") is not None:
            raise ValueError("expected a whole collect_parallel command")
        attempt = command_path.parent
        iteration = iteration_of(command_path)
        staging = attempt / "pooled-shards"
        if staging.exists():
            shutil.rmtree(staging)
        staging.mkdir()
        total = len(whole["episodes"])
        sizes = plan_sizes(total, [h.weight for h in self.hosts])
        tag_root = f"pooled-{attempt.parent.name}-{attempt.name}"
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
        receipt = {"schema": "mtg-kernel-pooled-collect-dispatch/v1", "iteration": iteration,
                   "episodes": total, "shards": [[i, f, s, results[i][1].name] for i, f, s in jobs],
                   "collection_seconds": round(collected, 3), "failed_hosts": sorted(failed),
                   "events": self.log, "merged": merged}
        (staging / "dispatch.json").write_text(json.dumps(receipt, indent=1))
        self.prune(pins_of(whole))
        return receipt

    def prune(self, current: set) -> None:
        """Remote retention: files pushed for earlier iterations (old checkpoints) are deleted
        once a later collection has merged; current pins and never-pushed files are kept."""
        state_path = self.config.get("state_path")
        if not state_path:
            return
        state_path = Path(state_path)
        state = json.loads(state_path.read_text()) if state_path.exists() else {}
        keep_paths = {p for p, _ in current}
        for host in self.hosts:
            previous = set(state.get(host.name, []))
            now = self.pushed.get(host.name, set())
            if host.prune_pushed and host.kind == "ssh":
                for path in sorted(previous - now - keep_paths):
                    self.transports[host.name].remove_file(path)
                    self.note(event="pruned", host=host.name, path=path)
                state[host.name] = sorted(now | (previous & keep_paths))
            else:
                state[host.name] = sorted(previous | now)
        stage = state_path.with_suffix(".stage")
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


def merge_with_binary(binary: str):
    def run_merge(command_path: Path, directories: list[Path]) -> dict:
        result = subprocess.run([binary, "merge", str(command_path)] + [str(d) for d in directories],
                                capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"merge failed: {result.stderr.strip()[-800:]}")
        merged = json.loads(result.stdout)
        return {"trajectories": len(merged["trajectories"]),
                "behavior_state_sha256": merged["behavior_state_sha256"]}
    return run_merge


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: pooled_collect.py ATTEMPT/collect-command.json (hosts file in MTG_POOLED_COLLECT_CONFIG)",
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
