"""Qualified concurrent launcher for K independent training runs.

One experiment is a fixed set of independent runs (one seed and one Store per
run) of one training executable. This launcher runs them concurrently across
placement slots (a host plus a device ordinal, each with a process capacity)
and enforces C:/Users/Jack/COMPUTE-POLICY.md items 2 to 5 at the launch point:

``inventory``  records Jack's PC, HaleysPC and RunPod availability.
``qualify``    runs every run of the experiment for a short prefix, first one
               at a time (the serial goldens), then under each candidate
               allocation in increasing concurrency. It compares completed-work
               throughput, projects completion time for the planned length,
               checks every concurrent run byte-for-byte against its serial
               golden and writes a compute-choice receipt naming the fastest
               qualified allocation.
``launch``     refuses to spawn anything unless that receipt matches this
               exact executable, workload and run set, has fresh inventory,
               measured both serial and parallel collection, every eligible
               host was measured and the selected allocation is the fastest
               qualified one. It then runs the experiment on that allocation,
               writes one manifest entry per run, monitors utilization and
               audits each finished run's prefix against its serial golden.

Every run is its own process with its own Store and seed, so concurrency never
shares weights, batches or optimizer state between runs. This is a launch
check, not a security boundary against an agent that edits or bypasses it.
"""

from __future__ import annotations

import argparse
import base64
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Callable, Iterable

WORKLOAD_SCHEMA = "mtg-kernel-multirun-workload/v1"
CHOICE_SCHEMA = "mtg-kernel-multirun-compute-choice/v1"
MANIFEST_SCHEMA = "mtg-kernel-multirun-experiment-manifest/v1"
RUN_MANIFEST_SCHEMA = "mtg-kernel-multirun-run-manifest/v1"
TICKET_SCHEMA = "mtg-kernel-multirun-launch-ticket/v1"
INVENTORY_SCHEMA = "mtg-kernel-multirun-inventory/v1"

HOSTS = ("jack", "haleyspc", "runpod")
INVENTORY_MAX_AGE_SECONDS = 24 * 3600
MINIMUM_QUALIFICATION_UPDATES = 3
# A candidate that fails to beat the best measured aggregate throughput by this
# fraction is "not useful" (policy item 2: extend only while useful throughput
# improves). Two such candidates in a row end the sweep.
USEFUL_GAIN_FRACTION = 0.05
IDLE_WINDOW_SECONDS = 60.0
IDLE_CPU_PERCENT = 60.0


class LaunchRefused(Exception):
    """The evidence does not license this launch."""


# --------------------------------------------------------------------------
# Small utilities


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def iso(moment: datetime) -> str:
    return moment.astimezone(timezone.utc).isoformat()


def canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> dict:
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path: Path, value: object) -> str:
    """Atomically write pretty JSON; return the SHA-256 of the written bytes."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    data = (json.dumps(value, sort_keys=True, indent=2) + "\n").encode()
    temporary = path.with_name(path.name + ".partial")
    temporary.write_bytes(data)
    os.replace(temporary, path)
    return sha256_bytes(data)


def finite_nonnegative(value: object, name: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise LaunchRefused(f"invalid measurement: {name}")
    return float(value)


# --------------------------------------------------------------------------
# Workload and adapters


@dataclass(frozen=True)
class RunSpec:
    id: str
    seed: int
    arm: str | None = None


# Machine-local fields: they locate files and never enter the workload identity.
MACHINE_FIELDS = ("executable", "data_root")


@dataclass
class Workload:
    raw: dict
    adapter: "Adapter"
    executable: Path
    executable_sha256: str
    planned_updates: int
    runs: list[RunSpec]

    @property
    def sha256(self) -> str:
        """Identity of the scientific workload: everything but machine paths."""
        identity = {key: value for key, value in self.raw.items() if key not in MACHINE_FIELDS}
        return sha256_bytes(canonical(identity))

    def knobs(self, run: RunSpec) -> dict[str, str]:
        """Knobs every run shares, overlaid with the run's arm knobs."""
        knobs = dict(self.raw.get("knobs", {}))
        if run.arm is not None:
            knobs.update(self.raw["arms"][run.arm].get("knobs", {}))
        return knobs


RUN_ID = re.compile(r"^[a-z0-9][a-z0-9-]{0,62}$")


def load_workload(path: Path) -> Workload:
    raw = read_json(path)
    if raw.get("schema") != WORKLOAD_SCHEMA:
        raise LaunchRefused("unsupported workload schema")
    adapter = adapter_for(raw)
    executable = Path(raw["executable"])
    pinned = raw.get("executable_sha256")
    if not isinstance(pinned, str) or not re.fullmatch(r"[0-9a-f]{64}", pinned):
        raise LaunchRefused("workload must pin executable_sha256")
    if not executable.is_file() or sha256_file(executable) != pinned:
        raise LaunchRefused("executable is missing or differs from the workload pin")
    planned = raw.get("planned_updates")
    if type(planned) is not int or planned < MINIMUM_QUALIFICATION_UPDATES:
        raise LaunchRefused("planned_updates must be an integer >= 3")
    arms = raw.get("arms")
    if arms is not None and (not isinstance(arms, dict) or not arms or not all(
            isinstance(arm, dict) and set(arm) <= {"knobs"} and RUN_ID.fullmatch(name)
            for name, arm in arms.items())):
        raise LaunchRefused("arms must map lowercase names to {\"knobs\": {...}}")
    runs = []
    for entry in raw.get("runs", []):
        expected = {"id", "seed", "arm"} if arms is not None else {"id", "seed"}
        if set(entry) != expected or not RUN_ID.fullmatch(str(entry["id"])):
            raise LaunchRefused("each run needs exactly a lowercase id and a seed"
                                + (" and an arm" if arms is not None else ""))
        if type(entry["seed"]) is not int or not 0 <= entry["seed"] < 2**63:
            raise LaunchRefused("run seed must be a u63 integer")
        if arms is not None and entry["arm"] not in arms:
            raise LaunchRefused(f"run {entry['id']} names an unknown arm")
        runs.append(RunSpec(entry["id"], entry["seed"], entry.get("arm")))
    if not runs:
        raise LaunchRefused("workload has no runs")
    if len({run.id for run in runs}) != len(runs) or len({run.seed for run in runs}) != len(runs):
        raise LaunchRefused("run ids and seeds must be distinct")
    adapter.validate(raw)
    return Workload(raw, adapter, executable, pinned, planned, runs)


class Adapter:
    """How one training executable is invoked and what its outputs are."""

    name = ""
    episodes_per_update = 0

    def validate(self, raw: dict) -> None:
        raise NotImplementedError

    def argv(self, workload: Workload, executable: str, run: RunSpec, run_root: str, device: int,
             stop_after: int | None) -> list[str]:
        raise NotImplementedError

    def environment(self, workload: Workload, run: RunSpec, run_root: str, device: int,
                    stop_after: int | None) -> dict[str, str]:
        raise NotImplementedError

    def output_root(self, run_root: Path) -> Path:
        raise NotImplementedError

    def generation_of(self, relative: str) -> int | None:
        """Update generation a published output belongs to, if any."""
        raise NotImplementedError

    def excluded(self, relative: str) -> bool:
        return False

    def static(self, relative: str) -> bool:
        """A generation-less output a longer run writes identically (kept in prefix audits)."""
        return False

    def validate_prefix(self, updates: int, planned_updates: int) -> None:
        if not MINIMUM_QUALIFICATION_UPDATES <= updates <= planned_updates:
            raise LaunchRefused("qualification needs at least three updates and no more than the planned length")

    def log_succeeded(self, log_text: str) -> bool:
        """Whether a zero exit status really means the workload ran."""
        return True

    # Shared behaviour -----------------------------------------------------

    def output_digests(self, run_root: Path, through_generation: int | None = None) -> dict[str, str]:
        """SHA-256 of every compared output, keyed by POSIX path under the output root.

        With ``through_generation`` only outputs of generations up to it are
        kept, plus static outputs (the prefix a longer run shares with its
        qualification golden); other generation-less files are dropped
        because a longer run rewrites them.
        """
        root = self.output_root(run_root)
        digests = {}
        if not root.is_dir():
            return digests
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            relative = path.relative_to(root).as_posix()
            if self.excluded(relative):
                continue
            if through_generation is not None and not self.static(relative):
                generation = self.generation_of(relative)
                if generation is None or generation > through_generation:
                    continue
            digests[relative] = sha256_file(path)
        return digests

    def completed_generations(self, run_root: Path) -> dict[int, float]:
        """Generation -> first publication time (mtime) observed on disk."""
        root = self.output_root(run_root)
        seen: dict[int, float] = {}
        if not root.is_dir():
            return seen
        for path in root.rglob("*"):
            if path.is_file():
                generation = self.generation_of(path.relative_to(root).as_posix())
                if generation is not None and generation > 0:
                    stamp = path.stat().st_mtime
                    seen[generation] = min(stamp, seen.get(generation, stamp))
        return seen


class NativeSciencePilotAdapterV1(Adapter):
    """The native science loop via the ``multirun_pilot_v1`` harness, one run per process.

    The harness reads its configuration from MULTIRUN_* variables. The
    launcher owns the per-run ones; the workload may set only the scientific
    knobs below, which then apply to every run identically.
    """

    name = "native-science-loop-pilot-v1"
    episodes_per_update = 64
    test_name = "native_science_loop_v1::windows_science_loop_tests::multirun_pilot_v1"
    owned = {
        "MULTIRUN_RUNS", "MULTIRUN_UPDATES", "MULTIRUN_BASE_SEED", "MULTIRUN_SEED_OFFSET",
        "MULTIRUN_STORE_PARENT", "MULTIRUN_STOP_AFTER_GENERATION", "MULTIRUN_EXPECT_RESUME_GENERATION",
        "MTG_KERNEL_PILOT_CUDA_ORDINAL", "CUDA_VISIBLE_DEVICES", "MULTIRUN_LAUNCH_TICKET",
    }
    knobs = {
        "MULTIRUN_WORKERS", "MULTIRUN_SESSIONS", "MULTIRUN_BROKER_TARGET",
        "MULTIRUN_ENVIRONMENT_RANDOMIZATION_V2", "MULTIRUN_RECORD_ONLY", "MULTIRUN_WIDE",
        "MULTIRUN_LADDER", "MULTIRUN_LADDER_POOL_DIR", "MULTIRUN_LADDER_INIT_STORE",
        "MULTIRUN_LADDER_INIT_GEN", "MULTIRUN_POLICY_ANCHOR_BETA",
    }
    # The fixture's record publishes a checkpoint every four updates, and the
    # harness honours a stop generation only at a checkpoint boundary (any
    # other stop point silently trains the whole record).
    checkpoint_interval = 4
    # Store outputs named for their generation: checkpoints/, heads/, refs/
    # update-<gen>.* and segments/segment-<gen>[.continuation-<n>].json.
    generation_pattern = re.compile(r"(?:^|/)(?:update|segment)-0*(\d+)\.[^/]+$")

    def validate(self, raw: dict) -> None:
        for knobs in [raw.get("knobs", {})] + [arm.get("knobs", {}) for arm in (raw.get("arms") or {}).values()]:
            if not isinstance(knobs, dict) or not all(isinstance(v, str) for v in knobs.values()):
                raise LaunchRefused("knobs must map names to strings")
            unknown = set(knobs) - self.knobs
            if unknown:
                raise LaunchRefused(f"workload may not set {sorted(unknown)}")

    def argv(self, workload, executable, run, run_root, device, stop_after):
        return [executable, "--exact", self.test_name, "--ignored", "--nocapture", "--test-threads=1"]

    def environment(self, workload, run, run_root, device, stop_after):
        env = workload.knobs(run)
        env.update({
            "MULTIRUN_RUNS": "1",
            "MULTIRUN_UPDATES": str(workload.planned_updates),
            "MULTIRUN_BASE_SEED": str(run.seed),
            "MULTIRUN_SEED_OFFSET": "0",
            "MULTIRUN_STORE_PARENT": str(Path(run_root) / "parent"),
            "MTG_KERNEL_PILOT_CUDA_ORDINAL": str(device),
        })
        if stop_after is not None:
            env["MULTIRUN_STOP_AFTER_GENERATION"] = str(stop_after)
        return env

    def output_root(self, run_root: Path) -> Path:
        return Path(run_root) / "parent" / "run-0"

    def generation_of(self, relative: str) -> int | None:
        match = self.generation_pattern.search(relative)
        return int(match.group(1)) if match else None

    def excluded(self, relative: str) -> bool:
        return relative in PILOT_EXCLUDED_OUTPUTS

    def static(self, relative: str) -> bool:
        return relative == "store/run.json"

    def validate_prefix(self, updates: int, planned_updates: int) -> None:
        super().validate_prefix(updates, planned_updates)
        if updates % self.checkpoint_interval or updates < 2 * self.checkpoint_interval:
            raise LaunchRefused(f"the pilot stops only at checkpoint boundaries: qualify a multiple of "
                                f"{self.checkpoint_interval} updates, at least {2 * self.checkpoint_interval}")

    def log_succeeded(self, log_text: str) -> bool:
        return "test result: ok. 1 passed" in log_text


# Outputs of the pilot Store that legitimately differ between two executions of
# the same run. Filled from the measured determinism probe; each entry carries
# its reason in docs/multirun_qualified_launcher_v1.md.
PILOT_EXCLUDED_OUTPUTS: frozenset[str] = frozenset()


class CommandTemplateAdapterV1(Adapter):
    """Any executable described by an argv/env template (migration path for other trainers)."""

    name = "command-template-v1"

    def validate(self, raw: dict) -> None:
        template = raw.get("template")
        if not isinstance(template, dict):
            raise LaunchRefused("command-template-v1 needs a template")
        required = {"argv", "env", "output_dir", "generation_regex", "episodes_per_update"}
        if set(template) - (required | {"exclude_regex"}) or required - set(template):
            raise LaunchRefused("template keys are argv, env, output_dir, generation_regex, "
                                "episodes_per_update and optional exclude_regex")
        if type(template["episodes_per_update"]) is not int or template["episodes_per_update"] < 1:
            raise LaunchRefused("episodes_per_update must be a positive integer")
        re.compile(template["generation_regex"])
        for pattern in template.get("exclude_regex", []):
            re.compile(pattern)

    def _fill(self, text: str, fields: dict[str, str]) -> str:
        return text.format(**fields)

    def _fields(self, workload, executable, run, run_root, device, stop_after):
        return {
            "executable": executable, "seed": str(run.seed), "run_id": run.id, "run_root": str(run_root),
            "arm": run.arm or "",
            "output_dir": str(Path(run_root) / workload.raw["template"]["output_dir"]),
            "planned_updates": str(workload.planned_updates), "device": str(device),
            "stop_after": str(stop_after if stop_after is not None else workload.planned_updates),
        }

    def argv(self, workload, executable, run, run_root, device, stop_after):
        fields = self._fields(workload, executable, run, run_root, device, stop_after)
        return [self._fill(part, fields) for part in workload.raw["template"]["argv"]]

    def environment(self, workload, run, run_root, device, stop_after):
        fields = self._fields(workload, "", run, run_root, device, stop_after)
        env = {key: self._fill(value, fields) for key, value in workload.raw["template"]["env"].items()}
        env.update(workload.knobs(run))
        return env

    def bind(self, raw: dict) -> "CommandTemplateAdapterV1":
        bound = CommandTemplateAdapterV1()
        template = raw["template"]
        bound.episodes_per_update = template["episodes_per_update"]
        bound._output_dir = template["output_dir"]
        bound._generation = re.compile(template["generation_regex"])
        bound._exclude = [re.compile(pattern) for pattern in template.get("exclude_regex", [])]
        return bound

    def output_root(self, run_root: Path) -> Path:
        return Path(run_root) / self._output_dir

    def generation_of(self, relative: str) -> int | None:
        match = self._generation.search(relative)
        return int(match.group(1)) if match else None

    def excluded(self, relative: str) -> bool:
        return any(pattern.search(relative) for pattern in self._exclude)


def adapter_for(raw: dict) -> Adapter:
    name = raw.get("adapter")
    if name == NativeSciencePilotAdapterV1.name:
        return NativeSciencePilotAdapterV1()
    if name == CommandTemplateAdapterV1.name:
        CommandTemplateAdapterV1().validate(raw)
        return CommandTemplateAdapterV1().bind(raw)
    raise LaunchRefused(f"unknown adapter {name!r}")


# --------------------------------------------------------------------------
# Placement


@dataclass(frozen=True)
class Slot:
    host: str
    device: int
    capacity: int

    @property
    def label(self) -> str:
        return f"{self.capacity}@{self.host}:{self.device}"


SLOT_TEXT = re.compile(r"^(\d+)@(?:(jack|haleyspc|runpod):)?(\d+)$")


def parse_allocation(text: str) -> list[Slot]:
    """``3@0+1@1`` or ``3@jack:0+2@haleyspc:0`` -> slots (host defaults to jack)."""
    slots = []
    for part in text.split("+"):
        match = SLOT_TEXT.fullmatch(part.strip())
        if not match or int(match.group(1)) < 1:
            raise LaunchRefused(f"bad allocation {text!r}")
        slots.append(Slot(match.group(2) or "jack", int(match.group(3)), int(match.group(1))))
    keys = [(slot.host, slot.device) for slot in slots]
    if len(set(keys)) != len(keys):
        raise LaunchRefused(f"allocation names a device twice: {text!r}")
    return slots


def allocation_text(slots: Iterable[Slot]) -> str:
    return "+".join(f"{slot.capacity}@{slot.host}:{slot.device}" for slot in slots)


def concurrency(slots: Iterable[Slot]) -> int:
    return sum(slot.capacity for slot in slots)


# --------------------------------------------------------------------------
# Execution


@dataclass
class RunResult:
    run_id: str
    host: str
    device: int
    exit_code: int | None
    started: float
    finished: float
    generations: dict[int, float] = field(default_factory=dict)
    log: str = ""


class LocalExecutor:
    """Runs one process per run on this machine."""

    host = "jack"

    def __init__(self, workload: Workload):
        self.workload = workload

    # Exit codes the launcher assigns when the process status alone would mislead.
    RAN_NOTHING = -2   # exited 0 but the log does not show the workload running
    OVERRAN = -3       # trained past its stop generation and was killed

    def run(self, run: RunSpec, run_root: Path, device: int, stop_after: int | None,
            extra_env: dict[str, str] | None = None) -> RunResult:
        # Absolute: the process runs with the run root as its working directory.
        run_root = Path(run_root).resolve()
        run_root.mkdir(parents=True, exist_ok=False)
        adapter = self.workload.adapter
        argv = adapter.argv(self.workload, str(self.workload.executable), run, str(run_root), device, stop_after)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("MULTIRUN_") and key not in ("MTG_KERNEL_PILOT_CUDA_ORDINAL", "CUDA_VISIBLE_DEVICES")}
        env.update(adapter.environment(self.workload, run, str(run_root), device, stop_after))
        env.update(extra_env or {})
        log_path = run_root / "process.log"
        write_json(run_root / "invocation.json", {"argv": argv, "env": {k: env[k] for k in sorted(env)
                                                                        if k.startswith(("MULTIRUN_", "MTG_KERNEL_"))}})
        started = time.time()
        overran = False
        with log_path.open("wb") as log:
            process = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT, cwd=run_root)
            while process.poll() is None:
                time.sleep(1.0)
                # Watchdog: a process that passes its stop generation would
                # silently train the whole record.
                if stop_after is not None and max(adapter.completed_generations(run_root), default=0) > stop_after:
                    process.kill()
                    process.wait()
                    overran = True
        finished = time.time()
        code = process.returncode
        if overran:
            code = self.OVERRAN
        elif code == 0 and not adapter.log_succeeded(log_path.read_text(encoding="utf-8", errors="replace")):
            code = self.RAN_NOTHING
        return RunResult(run.id, self.host, device, code, started, finished,
                         adapter.completed_generations(run_root), str(log_path))


class SshPowerShellExecutor:
    """Runs one process per run on a Windows host over Tailscale SSH.

    The executable is compiled with absolute data paths, so the remote side
    mirrors the executable and the repository ``data`` directory at the same
    absolute paths. Outputs are archived remotely and copied back into the
    local run root, where they are compared exactly like local outputs.
    """

    def __init__(self, workload: Workload, host: str, address: str, data_root: Path,
                 runner: Callable[..., subprocess.CompletedProcess] = subprocess.run):
        self.workload = workload
        self.host = host
        self.address = address
        self.data_root = data_root
        self.runner = runner
        self.staged = False
        self.transfer_seconds = 0.0

    def ssh(self, script: str, timeout: float) -> str:
        prologue = "$ErrorActionPreference='Stop'\n$ProgressPreference='SilentlyContinue'\n"
        encoded = base64.b64encode((prologue + script).encode("utf-16le")).decode()
        result = self.runner(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", self.address,
                              "powershell", "-NoProfile", "-EncodedCommand", encoded],
                             capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host}: {result.stderr.strip()[:400]}")
        return result.stdout

    def scp(self, source: str, target: str, timeout: float = 600) -> None:
        result = self.runner(["scp", "-q", "-r", source, target], capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host}: scp failed: {result.stderr.strip()[:400]}")

    @staticmethod
    def remote(path: Path | str) -> str:
        return Path(path).as_posix()

    def stage(self) -> None:
        if self.staged:
            return
        began = time.monotonic()
        executable = self.workload.executable
        for directory in {executable.parent, self.data_root.parent}:
            self.ssh(f"New-Item -ItemType Directory -Force -Path '{self.remote(directory)}' | Out-Null", 60)
        self.scp(str(executable), f"{self.address}:{self.remote(executable)}")
        remote_hash = self.ssh(f"(Get-FileHash -Algorithm SHA256 -LiteralPath '{self.remote(executable)}').Hash", 120)
        if remote_hash.strip().lower() != self.workload.executable_sha256:
            raise RuntimeError(f"{self.host}: staged executable hash differs")
        self.scp(str(self.data_root), f"{self.address}:{self.remote(self.data_root.parent)}")
        self.transfer_seconds += time.monotonic() - began
        self.staged = True

    def run(self, run: RunSpec, run_root: Path, device: int, stop_after: int | None,
            extra_env: dict[str, str] | None = None) -> RunResult:
        self.stage()
        run_root.mkdir(parents=True, exist_ok=False)
        adapter = self.workload.adapter
        remote_root = self.remote(run_root)
        argv = adapter.argv(self.workload, self.remote(self.workload.executable), run, remote_root, device, stop_after)
        env = adapter.environment(self.workload, run, remote_root, device, stop_after)
        env.update(extra_env or {})
        assignments = "\n".join(f"$env:{key}='{value}'" for key, value in sorted(env.items()))
        quoted = " ".join("'" + part.replace("'", "''") + "'" for part in argv[1:])
        script = (
            f"Remove-Item Env:CUDA_VISIBLE_DEVICES -ErrorAction SilentlyContinue\n{assignments}\n"
            f"New-Item -ItemType Directory -Force -Path '{remote_root}' | Out-Null\n"
            f"Set-Location '{remote_root}'\n"
            f"& '{argv[0]}' {quoted} *> '{remote_root}/process.log'\n"
            f"$code=$LASTEXITCODE\n"
            f"Compress-Archive -Force -Path '{remote_root}/*' -DestinationPath '{remote_root}.zip'\n"
            f"Write-Output $code"
        )
        write_json(run_root / "invocation.json", {"host": self.host, "argv": argv, "env": env})
        started = time.time()
        output = self.ssh(script, timeout=7 * 24 * 3600)
        finished = time.time()
        began = time.monotonic()
        archive = run_root.with_name(run_root.name + ".zip")
        self.scp(f"{self.address}:{remote_root}.zip", str(archive))
        shutil.unpack_archive(str(archive), str(run_root))
        archive.unlink()
        self.transfer_seconds += time.monotonic() - began
        code = int(output.strip().splitlines()[-1])
        # Remote mtimes survive the archive; they time the remote updates.
        return RunResult(run.id, self.host, device, code, started, finished,
                         adapter.completed_generations(run_root), str(run_root / "process.log"))


def execute_allocation(workload: Workload, slots: list[Slot], root: Path, stop_after: int | None,
                       executors: dict[str, object], extra_env: Callable[[RunSpec], dict] | None = None,
                       on_event: Callable[[str, dict], None] | None = None) -> tuple[list[RunResult], float]:
    """Run every run once under ``slots``; a run starts as soon as a slot has room.

    Runs start in workload order and each takes the first slot (in allocation
    order) with free capacity, so the placement is a pure function of the
    allocation and finish order.
    """
    pending = list(workload.runs)
    free = {slot: slot.capacity for slot in slots}
    results: list[RunResult] = []
    lock = threading.Condition()
    threads = []
    began = time.time()

    def worker(run: RunSpec, slot: Slot) -> None:
        run_root = root / run.id
        if on_event:
            on_event("run-started", {"run": run.id, "host": slot.host, "device": slot.device})
        try:
            result = executors[slot.host].run(run, run_root, slot.device, stop_after,
                                               extra_env(run) if extra_env else None)
        except Exception as error:  # a failed run is recorded, never retried silently
            result = RunResult(run.id, slot.host, slot.device, None, time.time(), time.time(), {}, repr(error))
        with lock:
            results.append(result)
            free[slot] += 1
            lock.notify_all()
        if on_event:
            on_event("run-finished", {"run": run.id, "exit_code": result.exit_code})

    with lock:
        while pending:
            slot = next((slot for slot in slots if free[slot] > 0), None)
            if slot is None:
                lock.wait()
                continue
            free[slot] -= 1
            run = pending.pop(0)
            thread = threading.Thread(target=worker, args=(run, slot), daemon=True)
            threads.append(thread)
            thread.start()
    for thread in threads:
        thread.join()
    order = {run.id: index for index, run in enumerate(workload.runs)}
    return sorted(results, key=lambda result: order[result.run_id]), time.time() - began


# --------------------------------------------------------------------------
# Machine observation


def gpu_inventory(runner=subprocess.run) -> list[dict]:
    try:
        result = runner(["nvidia-smi", "--query-gpu=index,name,uuid,memory.total,memory.used,utilization.gpu",
                         "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return []
    devices = []
    for line in result.stdout.strip().splitlines():
        index, name, uuid, total, used, utilization = [part.strip() for part in line.split(",")]
        devices.append({"index": int(index), "name": name, "uuid": uuid, "memory_total_mib": int(total),
                        "memory_used_mib": int(used), "utilization_percent": int(utilization)})
    return devices


class CpuSampler:
    """Whole-machine CPU busy fraction between successive calls."""

    def __init__(self):
        self.previous = self._times()

    @staticmethod
    def _times() -> tuple[int, int] | None:
        if sys.platform == "win32":
            idle, kernel, user = (ctypes.c_ulonglong() for _ in range(3))
            if not ctypes.windll.kernel32.GetSystemTimes(ctypes.byref(idle), ctypes.byref(kernel), ctypes.byref(user)):
                return None
            return idle.value, kernel.value + user.value  # kernel time includes idle time
        try:
            fields = [int(value) for value in Path("/proc/stat").read_text().split("\n")[0].split()[1:]]
        except OSError:
            return None
        return fields[3] + fields[4], sum(fields)

    def sample(self) -> float | None:
        current = self._times()
        previous, self.previous = self.previous, current
        if current is None or previous is None or current[1] == previous[1]:
            return None
        return 100.0 * (1 - (current[0] - previous[0]) / (current[1] - previous[1]))


def memory_inventory() -> dict:
    if sys.platform == "win32":
        class Status(ctypes.Structure):
            _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong), ("total", ctypes.c_ulonglong),
                        ("available", ctypes.c_ulonglong), ("page_total", ctypes.c_ulonglong),
                        ("page_available", ctypes.c_ulonglong), ("virtual_total", ctypes.c_ulonglong),
                        ("virtual_available", ctypes.c_ulonglong), ("extended", ctypes.c_ulonglong)]
        status = Status()
        status.length = ctypes.sizeof(Status)
        ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status))
        return {"total_gib": round(status.total / 2**30, 1), "available_gib": round(status.available / 2**30, 1)}
    return {}


def competing_processes(runner=subprocess.run) -> list[str]:
    """Names of running training/evaluation executables that are not ours."""
    if sys.platform != "win32":
        return []
    result = runner(["tasklist", "/fo", "csv", "/nh"], capture_output=True, text=True, timeout=30)
    names = []
    for line in result.stdout.splitlines():
        name = line.split(",")[0].strip('"')
        if re.match(r"(mtg_kernel|cycle4_arm|mtg-kernel|kernel_rl_env|java)", name, re.IGNORECASE):
            names.append(name)
    return sorted(names)


def idle_capacity_events(samples: list[dict], window_seconds: float = IDLE_WINDOW_SECONDS,
                         cpu_threshold: float = IDLE_CPU_PERCENT) -> list[dict]:
    """Policy item 6: two consecutive windows with runs waiting while the CPU sat mostly idle.

    Samples are grouped into consecutive ``window_seconds`` windows from the
    first sample. A window is idle when every sample in it had at least one
    run waiting for a slot and the mean CPU busy percent was below the
    threshold. Each pair of consecutive idle windows yields one event.
    """
    if not samples:
        return []
    start = samples[0]["t"]
    windows: dict[int, list[dict]] = {}
    for sample in samples:
        windows.setdefault(int((sample["t"] - start) // window_seconds), []).append(sample)
    events, streak = [], 0
    for index in range(max(windows) + 1):
        window = windows.get(index, [])
        busy = [s["cpu_percent"] for s in window if s.get("cpu_percent") is not None]
        idle = bool(busy) and all(s["waiting_runs"] > 0 for s in window) and sum(busy) / len(busy) < cpu_threshold
        streak = streak + 1 if idle else 0
        if streak == 2:
            events.append({"window_start": start + (index - 1) * window_seconds,
                           "mean_cpu_percent": round(sum(busy) / len(busy), 1),
                           "diagnosis": "runs waited for a slot while the CPU was below "
                                        f"{cpu_threshold:.0f} percent busy for two consecutive windows: "
                                        "the allocation caps concurrency below available capacity"})
            streak = 0
    return events


class Monitor(threading.Thread):
    """Samples CPU and GPU use during a leg or launch (policy item 6)."""

    def __init__(self, path: Path, waiting: Callable[[], int], interval: float = 5.0):
        super().__init__(daemon=True)
        self.path = path
        self.waiting = waiting
        self.interval = interval
        self.stop_event = threading.Event()
        self.samples: list[dict] = []

    def run(self) -> None:
        cpu = CpuSampler()
        with self.path.open("a", encoding="utf-8") as log:
            while not self.stop_event.wait(self.interval):
                sample = {"t": time.time(), "cpu_percent": cpu.sample(), "waiting_runs": self.waiting(),
                          "gpus": [{k: d[k] for k in ("index", "memory_used_mib", "utilization_percent")}
                                   for d in gpu_inventory()]}
                self.samples.append(sample)
                log.write(json.dumps(sample) + "\n")
                log.flush()

    def stop(self) -> dict:
        self.stop_event.set()
        self.join()
        cpu = [s["cpu_percent"] for s in self.samples if s["cpu_percent"] is not None]
        peaks: dict[str, int] = {}
        for sample in self.samples:
            for gpu in sample["gpus"]:
                key = str(gpu["index"])
                peaks[key] = max(peaks.get(key, 0), gpu["memory_used_mib"])
        return {"samples": len(self.samples), "cpu_mean_percent": sum(cpu) / len(cpu) if cpu else None,
                "gpu_peak_memory_mib": peaks, "idle_capacity_events": idle_capacity_events(self.samples)}


# --------------------------------------------------------------------------
# Inventory


def probe_local() -> dict:
    gpus = gpu_inventory()
    competing = competing_processes()
    free_disk = shutil.disk_usage(Path.cwd().anchor).free / 2**30
    detail = {"cpu_logical": os.cpu_count(), "gpus": gpus, "memory": memory_inventory(),
              "free_disk_gib_cwd_drive": round(free_disk, 1), "competing_processes": competing}
    eligible = bool(gpus)
    reason = (f"{os.cpu_count()} logical CPUs, {len(gpus)} GPUs, "
              f"{len(competing)} competing training processes" if eligible else "no CUDA device visible")
    return {"eligible": eligible, "reason": reason, "detail": detail}


def probe_ssh(address: str, runner=subprocess.run) -> dict:
    script = ("$ErrorActionPreference='Stop'\n"
              "$g = & nvidia-smi --query-gpu=index,name,uuid,memory.total,memory.used --format=csv,noheader,nounits\n"
              "$c = (Get-CimInstance Win32_Processor | Measure-Object NumberOfLogicalProcessors -Sum).Sum\n"
              "$m = [math]::Round((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory/1MB,1)\n"
              "$p = @(Get-Process | Where-Object { $_.ProcessName -match '^(mtg_kernel|cycle4_arm|java)' }).Count\n"
              "[pscustomobject]@{gpus=@($g); cpus=$c; free_memory_gib=$m; competing=$p} | ConvertTo-Json -Compress")
    encoded = base64.b64encode(script.encode("utf-16le")).decode()
    try:
        result = runner(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", address,
                         "powershell", "-NoProfile", "-EncodedCommand", encoded],
                        capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"eligible": False, "reason": f"ssh probe failed: {error!r}", "detail": {}}
    if result.returncode != 0:
        return {"eligible": False, "reason": f"unreachable over ssh: {result.stderr.strip()[:200]}", "detail": {}}
    detail = json.loads(result.stdout)
    if detail.get("competing"):
        return {"eligible": False, "reason": f"{detail['competing']} training processes already running there "
                                             "(preserve the remote owner's reservation)", "detail": detail}
    return {"eligible": bool(detail.get("gpus")), "reason": "reachable; idle; GPUs present"
            if detail.get("gpus") else "reachable but no CUDA device", "detail": detail}


# RunPod pods are Linux. The repository documents Windows and Linux training
# bytes as different by design (docs/audits/windows_golden_drift_rootcause_20260921.md,
# Portability), so a pod cannot reproduce a Windows serial golden.
RUNPOD_PLATFORM_REASON = ("account reachable, but RunPod pods are Linux and this Windows-built workload's "
                          "outputs are not byte-reproducible across platforms (Windows/Linux training bytes "
                          "differ by design); qualifying it needs a Linux build with Linux goldens and a lease "
                          "guard, which this launcher does not implement")


def probe_runpod(runner=subprocess.run) -> dict:
    """Read-only account query (balance, spend limit, running pods); never creates a pod."""
    key = os.environ.get("RUNPOD_API_KEY")
    if not key:
        return {"eligible": False, "detail": {}, "reason": "RUNPOD_API_KEY is not set in this environment"}
    query = json.dumps({"query": "query { myself { clientBalance spendLimit pods { id desiredStatus } } }"})
    try:
        # Cloudflare rejects requests without a browser-like User-Agent (error 1010).
        result = runner(["curl", "-s", "-m", "20", "-A", "Mozilla/5.0", "-H", "Content-Type: application/json",
                         "-H", f"Authorization: Bearer {key}", "--data", query, "https://api.runpod.io/graphql"],
                        capture_output=True, text=True, timeout=40)
        account = json.loads(result.stdout)["data"]["myself"]
    except (OSError, subprocess.TimeoutExpired, ValueError, KeyError, TypeError) as error:
        return {"eligible": False, "detail": {}, "reason": f"account query failed: {error!r}"}
    detail = {"client_balance_usd": account.get("clientBalance"), "spend_limit_usd": account.get("spendLimit"),
              "running_pods": len([p for p in account.get("pods") or [] if p.get("desiredStatus") == "RUNNING"])}
    return {"eligible": False, "detail": detail, "reason": RUNPOD_PLATFORM_REASON}


def take_inventory(haleys_address: str) -> dict:
    now = iso(utc_now())
    hosts = {"jack": probe_local(), "haleyspc": probe_ssh(haleys_address), "runpod": probe_runpod()}
    for status in hosts.values():
        status["checked_at"] = now
    return {"schema": INVENTORY_SCHEMA, "hosts": hosts}


# --------------------------------------------------------------------------
# Qualification


def leg_statistics(results: list[RunResult], wall: float, stop_after: int, episodes_per_update: int) -> dict:
    """Completed work and the per-run timing model used for projection."""
    startup, steady, tail = [], [], []
    for result in results:
        # Generations publish at checkpoint boundaries, so timings are per
        # boundary gap divided by the updates in it.
        points = sorted((g, t) for g, t in result.generations.items() if g <= stop_after)
        if len(points) < 2:
            continue
        per_update = [(t1 - t0) / (g1 - g0) for (g0, t0), (g1, t1) in zip(points, points[1:])]
        run_steady = sum(per_update) / len(per_update)
        steady.extend(per_update)
        startup.append(max(0.0, points[0][1] - result.started - points[0][0] * run_steady))
        tail.append(result.finished - points[-1][1])
    completed = sum(max((g for g in result.generations if g <= stop_after), default=0) for result in results
                    if result.exit_code == 0)
    episodes = completed * episodes_per_update
    mean = lambda values: sum(values) / len(values) if values else None  # noqa: E731
    return {"wall_seconds": wall, "completed_updates": completed, "episodes": episodes,
            "episodes_per_second": episodes / wall if wall > 0 else None,
            "startup_seconds": mean(startup), "steady_update_seconds": mean(steady), "tail_seconds": mean(tail)}


def project_seconds(statistics: dict, run_count: int, slots_concurrency: int, planned_updates: int,
                    overhead_seconds: float) -> float | None:
    """Completion time for ``run_count`` runs of ``planned_updates`` in waves of the allocation's width."""
    parts = [statistics.get(key) for key in ("startup_seconds", "steady_update_seconds", "tail_seconds")]
    if any(part is None for part in parts):
        return None
    startup, steady, tail = parts
    waves = math.ceil(run_count / slots_concurrency)
    return overhead_seconds + waves * (startup + planned_updates * steady + tail)


def compare_to_goldens(workload: Workload, results: list[RunResult], root: Path,
                       goldens: dict[str, dict[str, str]]) -> dict[str, dict]:
    per_run = {}
    for result in results:
        digests = workload.adapter.output_digests(root / result.run_id)
        golden = goldens.get(result.run_id)
        per_run[result.run_id] = {
            "host": result.host, "device": result.device, "exit_code": result.exit_code,
            "generations": sorted(result.generations),
            "digest_set_sha256": sha256_bytes(canonical(digests)),
            "output_count": len(digests),
            "byte_identical": golden is not None and bool(digests) and digests == golden,
            "differing_outputs": sorted(k for k in set(digests) | set(golden or {})
                                        if digests.get(k) != (golden or {}).get(k))[:20],
        }
    return per_run


def device_fits(slots: list[Slot], per_process_mib: float, margin_mib: int = 512) -> list[str]:
    """Reasons an allocation cannot fit on the local GPUs right now (empty if it fits)."""
    reasons = []
    devices = {gpu["index"]: gpu for gpu in gpu_inventory()}
    for slot in slots:
        if slot.host != "jack":
            continue
        gpu = devices.get(slot.device)
        if gpu is None:
            reasons.append(f"device {slot.device} not present")
            continue
        need = slot.capacity * per_process_mib
        room = gpu["memory_total_mib"] - gpu["memory_used_mib"] - margin_mib
        if need > room:
            reasons.append(f"device {slot.device}: {slot.capacity} processes need ~{need:.0f} MiB, "
                           f"{room:.0f} MiB free after margin")
    return reasons


def qualify(workload: Workload, root: Path, candidates: list[list[Slot]], qualification_updates: int,
            inventory: dict, executors: dict[str, object], overheads: dict[str, dict] | None = None,
            stop_on_saturation: bool = True, per_process_mib: float | None = None) -> dict:
    workload.adapter.validate_prefix(qualification_updates, workload.planned_updates)
    if root.exists():
        raise LaunchRefused(f"qualification root already exists: {root}")
    root.mkdir(parents=True)
    candidates = sorted(candidates, key=concurrency)
    serial = [candidate for candidate in candidates if concurrency(candidate) == 1]
    if not serial or not any(concurrency(candidate) > 1 for candidate in candidates):
        raise LaunchRefused("qualification needs the serial allocation and at least one parallel allocation")
    overheads = overheads or {}

    def leg(label: str, slots: list[Slot]) -> tuple[list[RunResult], float, dict]:
        leg_root = root / label
        leg_root.mkdir()
        waiting = {"count": len(workload.runs)}

        def on_event(kind, _payload):
            if kind == "run-started":
                waiting["count"] -= 1

        monitor = Monitor(leg_root / "monitor.jsonl", lambda: waiting["count"])
        monitor.start()
        results, wall = execute_allocation(
            workload, slots, leg_root, qualification_updates, executors,
            extra_env=qualification_tickets(workload, leg_root, allocation_text(slots), qualification_updates),
            on_event=on_event)
        usage = monitor.stop()
        return results, wall, usage

    # The serial leg is the golden: each run alone on the first serial slot.
    golden_slots = serial[0]
    baseline_mib = {str(gpu["index"]): gpu["memory_used_mib"] for gpu in gpu_inventory()}
    golden_results, golden_wall, golden_usage = leg("serial-golden", golden_slots)
    goldens = {}
    for result in golden_results:
        if result.exit_code != 0 or max(result.generations, default=0) != qualification_updates:
            raise LaunchRefused(f"serial golden for {result.run_id} did not complete "
                                f"(exit {result.exit_code}); see {result.log}")
        goldens[result.run_id] = workload.adapter.output_digests(root / "serial-golden" / result.run_id)
    # Serial repeat of the first run: proves the outputs are a function of the inputs at all.
    repeat_root = root / "serial-repeat"
    repeat_root.mkdir()
    repeat = executors[golden_slots[0].host].run(
        workload.runs[0], repeat_root / workload.runs[0].id, golden_slots[0].device, qualification_updates,
        qualification_tickets(workload, repeat_root, allocation_text(golden_slots),
                              qualification_updates)(workload.runs[0]))
    repeat_digests = workload.adapter.output_digests(repeat_root / workload.runs[0].id)
    if repeat.exit_code != 0 or repeat_digests != goldens[workload.runs[0].id]:
        differing = sorted(k for k in set(repeat_digests) | set(goldens[workload.runs[0].id])
                           if repeat_digests.get(k) != goldens[workload.runs[0].id].get(k))
        raise LaunchRefused(f"serial repeat is not byte-identical; nondeterministic outputs: {differing[:20]}")

    if per_process_mib is None:
        # One process alone on the golden device: its peak above the pre-leg baseline.
        device = str(golden_slots[0].device)
        peak = golden_usage["gpu_peak_memory_mib"].get(device)
        per_process_mib = max(0.0, peak - baseline_mib.get(device, peak)) if peak is not None else 0.0
    records = []

    def record(slots, results, wall, usage, status_override=None, reasons=(), leg_root=None):
        statistics = leg_statistics(results, wall, qualification_updates, workload.adapter.episodes_per_update)
        leg_root = leg_root or root / label_of(slots)
        per_run = compare_to_goldens(workload, results, leg_root, goldens) if results else {}
        reasons = list(reasons)
        if results and not all(entry["byte_identical"] and entry["exit_code"] == 0 for entry in per_run.values()):
            reasons.append("a concurrent run differs from its serial golden or failed")
        overhead = sum(overheads.get(slot.host, {}).get(key, 0.0)
                       for slot in {Slot(s.host, 0, 1) for s in slots}
                       for key in ("setup_seconds", "transfer_seconds", "recovery_seconds"))
        status = status_override or ("qualified" if not reasons else "disqualified")
        projected = project_seconds(statistics, len(workload.runs), concurrency(slots), workload.planned_updates,
                                    overhead) if status == "qualified" else None
        entry = {"id": label_of(slots), "allocation": allocation_text(slots), "concurrency": concurrency(slots),
                 "hosts": sorted({slot.host for slot in slots}), "status": status, "reasons": reasons,
                 "overhead_seconds": overhead, "projected_seconds": projected, "usage": usage,
                 "per_run": per_run, **statistics}
        records.append(entry)
        return entry

    record(golden_slots, golden_results, golden_wall, golden_usage, leg_root=root / "serial-golden")
    best = records[0]["episodes_per_second"] or 0.0
    not_useful = 0
    for slots in candidates:
        if slots == golden_slots:
            continue
        fit = device_fits(slots, per_process_mib) if per_process_mib > 0 else []
        missing = [slot.host for slot in slots if not inventory["hosts"].get(slot.host, {}).get("eligible")]
        if missing or fit:
            record(slots, [], 0.0, {}, "capacity-skipped",
                   [f"host not eligible: {host}" for host in sorted(set(missing))] + fit)
            continue
        results, wall, usage = leg(label_of(slots), slots)
        entry = record(slots, results, wall, usage)
        throughput = entry["episodes_per_second"] or 0.0
        if entry["status"] == "qualified" and throughput > best * (1 + USEFUL_GAIN_FRACTION):
            best, not_useful = throughput, 0
        else:
            not_useful += 1
            if stop_on_saturation and not_useful >= 2:
                break

    qualified = [entry for entry in records if entry["status"] == "qualified" and entry["projected_seconds"]]
    selected = min(qualified, key=lambda entry: entry["projected_seconds"])
    choice = {
        "schema": CHOICE_SCHEMA,
        "created_at": iso(utc_now()),
        "adapter": workload.adapter.name,
        "workload_sha256": workload.sha256,
        "workload": {k: v for k, v in workload.raw.items() if k not in MACHINE_FIELDS},
        "executable_sha256": workload.executable_sha256,
        "planned_updates": workload.planned_updates,
        "qualification_updates": qualification_updates,
        "run_ids": [run.id for run in workload.runs],
        "inventory": inventory,
        "golden": {"allocation": allocation_text(golden_slots), "root": str(root / "serial-golden"),
                   "serial_repeat_identical": True,
                   "runs": {run_id: {"digests": digests, "digest_set_sha256": sha256_bytes(canonical(digests))}
                            for run_id, digests in goldens.items()}},
        "candidates": records,
        "selected": selected["id"],
        "selection_rule": "minimum projected completion seconds among byte-identical, fully completed "
                          "allocations; projection = overhead + ceil(runs/concurrency) * "
                          "(startup + planned_updates * steady_update + tail)",
    }
    write_json(root / "compute-choice.json", choice)
    return choice


def label_of(slots: list[Slot]) -> str:
    return "alloc-" + allocation_text(slots).replace("@", "at").replace(":", "d").replace("+", "_")


# --------------------------------------------------------------------------
# Launch guard


def require_choice(choice_path: Path, workload: Workload, now: datetime | None = None,
                   verify_golden_files: bool = True) -> dict:
    """Validate the compute-choice receipt for this exact launch; return the selected candidate.

    Raises LaunchRefused on any missing or incompatible evidence. Nothing is
    spawned before this returns.
    """
    now = now or utc_now()
    if not Path(choice_path).is_file():
        raise LaunchRefused("no compute-choice receipt: run `qualify` first (COMPUTE-POLICY items 2-5)")
    choice = read_json(choice_path)
    if choice.get("schema") != CHOICE_SCHEMA:
        raise LaunchRefused("unsupported compute-choice schema")
    if choice.get("executable_sha256") != workload.executable_sha256:
        raise LaunchRefused("requalify: the training executable changed")
    if choice.get("workload_sha256") != workload.sha256 or choice.get("adapter") != workload.adapter.name:
        raise LaunchRefused("requalify: the workload (knobs, runs, length or adapter) changed")
    if choice.get("run_ids") != [run.id for run in workload.runs]:
        raise LaunchRefused("the receipt covers a different run set")
    if choice.get("planned_updates") != workload.planned_updates:
        raise LaunchRefused("the receipt covers a different run length")
    updates = choice.get("qualification_updates")
    if type(updates) is not int:
        raise LaunchRefused("qualification prefix is missing")
    workload.adapter.validate_prefix(updates, workload.planned_updates)

    hosts = choice.get("inventory", {}).get("hosts", {})
    if set(hosts) != set(HOSTS):
        raise LaunchRefused("inventory must record Jack's PC, HaleysPC and RunPod")
    for host, status in hosts.items():
        checked = datetime.fromisoformat(status.get("checked_at", ""))
        if checked.tzinfo is None or not 0 <= (now - checked).total_seconds() <= INVENTORY_MAX_AGE_SECONDS:
            raise LaunchRefused(f"refresh the resource inventory: {host}")
        if not isinstance(status.get("eligible"), bool) or not str(status.get("reason", "")).strip():
            raise LaunchRefused(f"inventory must record availability and a reason: {host}")

    golden = choice.get("golden", {})
    if golden.get("serial_repeat_identical") is not True:
        raise LaunchRefused("serial outputs were never shown to be reproducible")
    golden_runs = golden.get("runs", {})
    if set(golden_runs) != {run.id for run in workload.runs}:
        raise LaunchRefused("every run needs a serial golden")
    for run_id, entry in golden_runs.items():
        digests = entry.get("digests", {})
        if not digests or sha256_bytes(canonical(digests)) != entry.get("digest_set_sha256"):
            raise LaunchRefused(f"golden digest set is malformed: {run_id}")
        if verify_golden_files:
            on_disk = workload.adapter.output_digests(Path(golden["root"]) / run_id)
            if on_disk != digests:
                raise LaunchRefused(f"golden outputs on disk no longer match the receipt: {run_id}")

    candidates = choice.get("candidates", [])
    measured = [c for c in candidates if c.get("status") in ("qualified", "disqualified")]
    if not any(c.get("concurrency") == 1 for c in measured) or not any(c.get("concurrency", 0) > 1 for c in measured):
        raise LaunchRefused("a serial timing alone is insufficient; qualify parallel collection")
    qualified = []
    for candidate in candidates:
        if candidate.get("status") != "qualified":
            continue
        per_run = candidate.get("per_run", {})
        if set(per_run) != set(golden_runs):
            raise LaunchRefused(f"candidate {candidate.get('id')} did not run every run")
        for run_id, entry in per_run.items():
            if entry.get("exit_code") != 0 or entry.get("byte_identical") is not True:
                raise LaunchRefused(f"candidate {candidate.get('id')} is marked qualified but {run_id} "
                                    "failed or differs from its golden")
            if entry.get("digest_set_sha256") != golden_runs[run_id]["digest_set_sha256"]:
                raise LaunchRefused(f"candidate {candidate.get('id')} digest differs from golden for {run_id}")
        for key in ("wall_seconds", "episodes_per_second", "projected_seconds"):
            finite_nonnegative(candidate.get(key), f"{candidate.get('id')}.{key}")
        if not candidate["episodes"] or candidate["episodes"] != (
                len(golden_runs) * updates * workload.adapter.episodes_per_update):
            raise LaunchRefused(f"candidate {candidate.get('id')} did not complete every qualification update")
        for host in candidate.get("hosts", []):
            if not hosts.get(host, {}).get("eligible"):
                raise LaunchRefused(f"candidate {candidate.get('id')} uses an ineligible host: {host}")
        qualified.append(candidate)
    if not qualified:
        raise LaunchRefused("no allocation qualified")
    for host, status in hosts.items():
        if status["eligible"] and not any(host in c.get("hosts", []) for c in measured):
            raise LaunchRefused(f"eligible placement has no throughput measurement: {host}")
    fastest = min(qualified, key=lambda c: c["projected_seconds"])
    selected = next((c for c in qualified if c["id"] == choice.get("selected")), None)
    if selected is None or selected["projected_seconds"] > fastest["projected_seconds"]:
        raise LaunchRefused("the selected allocation is slower than a qualified alternative")
    return selected


def issue_ticket(path: Path, workload: Workload, run: RunSpec, allocation: str, kind: str,
                 choice_sha256: str | None = None, stop_after: int | None = None) -> Path:
    """Per-run ticket the harness checks (mtg-kernel/src/multirun_launch_ticket_v1.rs).

    It binds this executable, the run's seed, the planned length and the
    stop point; a launch ticket also binds the compute-choice receipt.
    """
    write_json(path, {"schema": TICKET_SCHEMA, "kind": kind, "executable_sha256": workload.executable_sha256,
                      "compute_choice_sha256": choice_sha256, "workload_sha256": workload.sha256,
                      "run_id": run.id, "seed": run.seed, "planned_updates": workload.planned_updates,
                      "stop_after_generation": stop_after, "allocation": allocation, "issued_at": iso(utc_now())})
    return path


def qualification_tickets(workload: Workload, leg_root: Path, allocation: str,
                          stop_after: int) -> Callable[[RunSpec], dict]:
    def env(run: RunSpec) -> dict:
        ticket = issue_ticket(leg_root / f"{run.id}.ticket.json", workload, run, allocation, "qualification",
                              stop_after=stop_after)
        return {"MULTIRUN_LAUNCH_TICKET": str(ticket)}
    return env


def launch(workload: Workload, choice_path: Path, root: Path, executors: dict[str, object],
           now: datetime | None = None, verify_golden_files: bool = True) -> dict:
    selected = require_choice(choice_path, workload, now, verify_golden_files)
    choice = read_json(choice_path)
    choice_sha256 = sha256_file(choice_path)
    slots = parse_allocation(selected["allocation"])
    if root.exists():
        raise LaunchRefused(f"launch root already exists: {root}")
    runs_root = root / "runs"
    runs_root.mkdir(parents=True)
    manifest_path = root / "experiment-manifest.json"
    manifest = {
        "schema": MANIFEST_SCHEMA, "created_at": iso(utc_now()), "workload_sha256": workload.sha256,
        "executable": str(workload.executable), "executable_sha256": workload.executable_sha256,
        "planned_updates": workload.planned_updates, "compute_choice": str(choice_path),
        "compute_choice_sha256": choice_sha256, "allocation": selected["allocation"],
        "allocation_evidence": {key: selected[key] for key in
                                ("id", "concurrency", "episodes_per_second", "projected_seconds",
                                 "steady_update_seconds", "startup_seconds", "tail_seconds")},
        "alternatives": [{key: c.get(key) for key in ("id", "status", "episodes_per_second", "projected_seconds",
                                                      "reasons")} for c in choice["candidates"]],
        "inventory_checked_at": {h: s["checked_at"] for h, s in choice["inventory"]["hosts"].items()},
        "status": "running", "runs": {},
    }
    manifest_lock = threading.Lock()

    def save():
        with manifest_lock:
            write_json(manifest_path, manifest)

    for run in workload.runs:
        manifest["runs"][run.id] = {"seed": run.seed, "status": "queued"}
        issue_ticket(runs_root / f"{run.id}.ticket.json", workload, run, selected["allocation"], "launch",
                     choice_sha256=choice_sha256)
    save()
    waiting = {"count": len(workload.runs)}

    def on_event(kind: str, payload: dict) -> None:
        entry = manifest["runs"][payload["run"]]
        if kind == "run-started":
            waiting["count"] -= 1
            entry.update(status="running", host=payload["host"], device=payload["device"], started_at=iso(utc_now()))
        else:
            entry.update(status="finished", exit_code=payload["exit_code"], finished_at=iso(utc_now()))
        save()

    monitor = Monitor(root / "monitor.jsonl", lambda: waiting["count"])
    monitor.start()
    results, wall = execute_allocation(
        workload, slots, runs_root, None, executors,
        extra_env=lambda run: {"MULTIRUN_LAUNCH_TICKET": str(runs_root / f"{run.id}.ticket.json")},
        on_event=on_event)
    usage = monitor.stop()
    golden_runs = choice["golden"]["runs"]
    prefix = choice["qualification_updates"]
    for result in results:
        run_root = runs_root / result.run_id
        final = workload.adapter.output_digests(run_root)
        shared = workload.adapter.output_digests(run_root, through_generation=prefix)
        golden_prefix = {k: v for k, v in golden_runs[result.run_id]["digests"].items()
                         if workload.adapter.static(k)
                         or ((g := workload.adapter.generation_of(k)) is not None and g <= prefix)}
        entry = manifest["runs"][result.run_id]
        entry.update({
            "run_root": str(run_root), "exit_code": result.exit_code,
            "completed_generation": max(result.generations, default=0),
            "status": "complete" if result.exit_code == 0 and max(result.generations, default=0)
            == workload.planned_updates else "failed",
            "wall_seconds": result.finished - result.started,
            "output_digest_set_sha256": sha256_bytes(canonical(final)),
            "golden_prefix_generation": prefix,
            "golden_prefix_identical": bool(golden_prefix) and shared == golden_prefix,
            "log": result.log,
        })
        write_json(run_root / "run-manifest.json", {"schema": RUN_MANIFEST_SCHEMA, "run_id": result.run_id,
                                                     **entry, "experiment_manifest": str(manifest_path),
                                                     "compute_choice_sha256": choice_sha256})
    episodes = sum(e.get("completed_generation", 0) for e in manifest["runs"].values()) \
        * workload.adapter.episodes_per_update
    manifest.update(status="complete" if all(e["status"] == "complete" for e in manifest["runs"].values())
                    else "failed", wall_seconds=wall, episodes=episodes,
                    episodes_per_second=episodes / wall if wall else None, usage=usage,
                    finished_at=iso(utc_now()))
    save()
    return manifest


# --------------------------------------------------------------------------
# CLI


def build_executors(workload: Workload, haleys_address: str) -> dict[str, object]:
    executors: dict[str, object] = {"jack": LocalExecutor(workload)}
    data_root = Path(workload.raw.get("data_root", "")) if workload.raw.get("data_root") else None
    if data_root is not None:
        executors["haleyspc"] = SshPowerShellExecutor(workload, "haleyspc", haleys_address, data_root)
    return executors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    inventory = commands.add_parser("inventory", help="record placement options")
    inventory.add_argument("--out", type=Path, required=True)
    inventory.add_argument("--haleyspc", default="haley@100.71.75.65")
    qualify_parser = commands.add_parser("qualify", help="serial goldens plus scaling comparison")
    qualify_parser.add_argument("--workload", type=Path, required=True)
    qualify_parser.add_argument("--inventory", type=Path, required=True)
    qualify_parser.add_argument("--root", type=Path, required=True)
    qualify_parser.add_argument("--updates", type=int, default=4)
    qualify_parser.add_argument("--allocation", action="append", required=True,
                                help="e.g. 1@0 (serial), 1@0+1@1, 3@0+1@1, 2@0+2@haleyspc:0")
    qualify_parser.add_argument("--haleyspc", default="haley@100.71.75.65")
    qualify_parser.add_argument("--no-early-stop", action="store_true")
    launch_parser = commands.add_parser("launch", help="run the experiment on the qualified allocation")
    launch_parser.add_argument("--workload", type=Path, required=True)
    launch_parser.add_argument("--choice", type=Path, required=True)
    launch_parser.add_argument("--root", type=Path, required=True)
    launch_parser.add_argument("--haleyspc", default="haley@100.71.75.65")
    check = commands.add_parser("check", help="validate a receipt against a workload without launching")
    check.add_argument("--workload", type=Path, required=True)
    check.add_argument("--choice", type=Path, required=True)
    arguments = parser.parse_args(argv)
    try:
        if arguments.command == "inventory":
            write_json(arguments.out, take_inventory(arguments.haleyspc))
            print(arguments.out)
            return 0
        workload = load_workload(arguments.workload)
        if arguments.command == "qualify":
            inventory_record = read_json(arguments.inventory)
            executors = build_executors(workload, arguments.haleyspc)
            choice = qualify(workload, arguments.root, [parse_allocation(text) for text in arguments.allocation],
                             arguments.updates, inventory_record, executors,
                             stop_on_saturation=not arguments.no_early_stop)
            print(json.dumps({"selected": choice["selected"], "candidates": [
                {k: c[k] for k in ("id", "status", "episodes_per_second", "projected_seconds")}
                for c in choice["candidates"]]}, indent=2))
            return 0
        if arguments.command == "check":
            print(json.dumps(require_choice(arguments.choice, workload)["id"]))
            return 0
        manifest = launch(workload, arguments.choice, arguments.root,
                          build_executors(workload, arguments.haleyspc))
        print(json.dumps({"status": manifest["status"], "manifest": str(arguments.root / "experiment-manifest.json")}))
        return 0 if manifest["status"] == "complete" else 1
    except LaunchRefused as refusal:
        print(f"launch refused: {refusal}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
