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
import heapq
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
import tarfile
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
HALEYSPC_ADDRESS = "haley@100.71.75.65"
INVENTORY_MAX_AGE_SECONDS = 24 * 3600
MINIMUM_QUALIFICATION_UPDATES = 3
# A candidate that fails to beat the best measured aggregate throughput by this
# fraction is "not useful" (policy item 2: extend only while useful throughput
# improves). Two such candidates in a row end the sweep.
USEFUL_GAIN_FRACTION = 0.05
IDLE_WINDOW_SECONDS = 60.0
# Free GPU memory every fit decision leaves on a device, in MiB.
FIT_MARGIN_MIB = 512
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
    # Resumed segment: run root holding the parent Store at the resume generation.
    parent: Path | None = None


# Machine-local fields: they locate files and never enter the workload identity.
MACHINE_FIELDS = ("executable", "data_root", "remote_mirror_root", "remote_runtime_libraries", "remote_cuda_root",
                  "parents")


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
        """Identity of the scientific workload: everything but machine paths.

        A resumed segment's parent Stores enter by content (their outputs
        through the resume generation), never by path; refresh-chain manifests
        named in knobs enter by content too.
        """
        identity = {key: value for key, value in self.raw.items() if key not in MACHINE_FIELDS}
        if self.resume_generation is not None:
            identity["parent_digests"] = {run.id: sha256_bytes(canonical(self.adapter.output_digests(
                run.parent, through_generation=self.resume_generation))) for run in self.runs}
        manifests = {}
        for knobs in [self.raw.get("knobs", {})] + [a.get("knobs", {}) for a in (self.raw.get("arms") or {}).values()]:
            for name, value in knobs.items():
                if name.endswith("_REFRESH_CHAIN"):
                    for item in filter(None, value.split(";")):
                        manifests[item] = sha256_file(Path(item)) if Path(item).is_file() else None
        if manifests:
            identity["refresh_chain_sha256"] = manifests
        return sha256_bytes(canonical(identity))

    @property
    def resume_generation(self) -> int | None:
        segment = self.raw.get("segment")
        return segment["resume_generation"] if segment else None

    @property
    def start_generation(self) -> int:
        return self.resume_generation or 0

    @property
    def target_generation(self) -> int:
        """Generation every run must reach: the segment stop, or the record length."""
        segment = self.raw.get("segment")
        return segment["stop_generation"] if segment else self.planned_updates

    @property
    def span(self) -> int:
        """Updates each run trains in this workload."""
        return self.target_generation - self.start_generation

    @property
    def launch_stop(self) -> int | None:
        """Stop generation a full (non-prefix) execution passes to the process."""
        return self.target_generation if self.raw.get("segment") else None

    def arms(self) -> list[str | None]:
        seen: list[str | None] = []
        for run in self.runs:
            if run.arm not in seen:
                seen.append(run.arm)
        return seen

    def data_tree_sha256(self) -> str | None:
        """Digest of every file under ``data_root`` (runtime inputs the executable reads by path)."""
        if not self.raw.get("data_root"):
            return None
        root = Path(self.raw["data_root"])
        if not root.is_dir():
            raise LaunchRefused(f"data_root is not a directory: {root}")
        return sha256_bytes(canonical({path.relative_to(root).as_posix(): sha256_file(path)
                                       for path in sorted(root.rglob("*")) if path.is_file()}))

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
    segment = raw.get("segment")
    if segment is not None:
        if not isinstance(segment, dict) or set(segment) != {"resume_generation", "stop_generation"} or not all(
                type(segment[key]) is int for key in segment) or not \
                0 < segment["resume_generation"] < segment["stop_generation"] <= planned:
            raise LaunchRefused("segment needs integer 0 < resume_generation < stop_generation <= planned_updates")
        parents = raw.get("parents")
        if not isinstance(parents, dict) or set(parents) != {run.id for run in runs}:
            raise LaunchRefused("a resumed segment names a parent run root for every run")
        runs = [RunSpec(run.id, run.seed, run.arm, Path(parents[run.id])) for run in runs]
    elif raw.get("parents") is not None:
        raise LaunchRefused("parents are only meaningful for a resumed segment")
    workload = Workload(raw, adapter, executable, pinned, planned, runs)
    adapter.validate(raw)
    if segment is not None:
        adapter.validate_segment(workload)
    return workload


def read_log(path: Path) -> str:
    """A process log as text, whether the writer used UTF-8 or UTF-16."""
    data = Path(path).read_bytes() if Path(path).is_file() else b""
    if data.startswith((b"\xff\xfe", b"\xfe\xff")) or b"\x00" in data[:200]:
        return data.decode("utf-16", errors="replace")
    return data.decode("utf-8", errors="replace")


class Adapter:
    """How one training executable is invoked and what its outputs are."""

    name = ""
    episodes_per_update = 0
    # File-name regex whose first group is a generation (remote watchdog).
    generation_name_regex = r"^$"

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

    def validate_prefix(self, updates: int, span: int) -> None:
        """``updates`` past the start generation, within the ``span`` each run trains."""
        if not MINIMUM_QUALIFICATION_UPDATES <= updates <= span:
            raise LaunchRefused("qualification needs at least three updates and no more than the run's span")

    def validate_segment(self, workload: "Workload") -> None:
        """Each parent must hold exactly the resume generation as its latest output."""
        for run in workload.runs:
            latest = max(self.completed_generations(run.parent), default=0)
            if latest != workload.resume_generation:
                raise LaunchRefused(f"parent of {run.id} ends at generation {latest}, "
                                    f"not the resume generation {workload.resume_generation}")

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

    def completed_generations(self, run_root: Path, after: int = 0) -> dict[int, float]:
        """Generation -> first publication time (mtime) observed on disk, for generations past ``after``."""
        root = self.output_root(run_root)
        seen: dict[int, float] = {}
        if not root.is_dir():
            return seen
        for path in root.rglob("*"):
            if path.is_file():
                generation = self.generation_of(path.relative_to(root).as_posix())
                if generation is not None and generation > after:
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
        # Population and response-exploiter runtimes (the harness validates
        # their combinations and resolves slots through Store authority).
        "MULTIRUN_POPULATION_AUTHORITY", "MULTIRUN_POPULATION_RUNTIME", "MULTIRUN_POPULATION_REFRESH_CHAIN",
        "MULTIRUN_POPULATION_SLOT_ROOTS", "MULTIRUN_RESPONSE_EXPLOITER_RUNTIME",
        "MULTIRUN_RESPONSE_EXPLOITER_DENOVO", "MULTIRUN_RESPONSE_EXPLOITER_REFRESH_CHAIN",
        "MULTIRUN_RESPONSE_EXPLOITER_SLOT_ROOTS",
        # Opt-in CUDA memory pool page cap (qualification/cubecl-cuda-pagecap):
        # allocation bookkeeping only, but part of the qualified configuration.
        "MTG_KERNEL_CUBECL_MAX_PAGE_MIB",
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
        if workload.resume_generation is not None:
            env["MULTIRUN_EXPECT_RESUME_GENERATION"] = str(workload.resume_generation)
        return env

    def output_root(self, run_root: Path) -> Path:
        return Path(run_root) / "parent" / "run-0"

    # Store file names a remote watchdog matches to find the latest generation.
    generation_name_regex = r"^(?:update|segment)-0*(\d+)\."

    def generation_of(self, relative: str) -> int | None:
        match = self.generation_pattern.search(relative)
        return int(match.group(1)) if match else None

    def excluded(self, relative: str) -> bool:
        return relative in PILOT_EXCLUDED_OUTPUTS

    def static(self, relative: str) -> bool:
        return relative == "store/run.json"

    def validate_prefix(self, updates: int, span: int) -> None:
        super().validate_prefix(updates, span)
        if updates % self.checkpoint_interval or updates < 2 * self.checkpoint_interval:
            raise LaunchRefused(f"the pilot stops only at checkpoint boundaries: qualify a multiple of "
                                f"{self.checkpoint_interval} updates, at least {2 * self.checkpoint_interval}")

    def validate_segment(self, workload: "Workload") -> None:
        segment = workload.raw["segment"]
        if segment["resume_generation"] % self.checkpoint_interval or (
                segment["stop_generation"] % self.checkpoint_interval
                and segment["stop_generation"] != workload.planned_updates):
            raise LaunchRefused("the pilot resumes and stops only at checkpoint boundaries")
        super().validate_segment(workload)

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
            "stop_after": str(stop_after if stop_after is not None else workload.target_generation),
            "resume": str(workload.start_generation),
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
        bound.generation_name_regex = template["generation_regex"]
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


# Variables that change what a run does must come from the workload, never
# from the caller's shell.
INHERITED_ENV_BLOCKLIST = ("MTG_KERNEL_PILOT_CUDA_ORDINAL", "CUDA_VISIBLE_DEVICES", "MTG_KERNEL_CUBECL_MAX_PAGE_MIB")


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
        if run.parent is not None:
            # A resumed segment starts from a private copy of its parent Store.
            shutil.copytree(adapter.output_root(run.parent), adapter.output_root(run_root))
        start = self.workload.start_generation
        argv = adapter.argv(self.workload, str(self.workload.executable), run, str(run_root), device, stop_after)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("MULTIRUN_") and key not in INHERITED_ENV_BLOCKLIST}
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
                if stop_after is not None and max(adapter.completed_generations(run_root, start),
                                                  default=0) > stop_after:
                    process.kill()
                    process.wait()
                    overran = True
        finished = time.time()
        code = process.returncode
        if overran:
            code = self.OVERRAN
        elif code == 0 and not adapter.log_succeeded(read_log(log_path)):
            code = self.RAN_NOTHING
        return RunResult(run.id, self.host, device, code, started, finished,
                         adapter.completed_generations(run_root, start), str(log_path))


class SshPowerShellExecutor:
    """Runs one process per run on a Windows host over Tailscale SSH.

    The executable bakes absolute data paths at compile time, and the remote
    host may lack the local drive letters. Every local path P = X:/rest is
    therefore staged at <mirror_root>/X/rest on the remote, and each run's
    PowerShell session maps X: onto <mirror_root>/X with ``subst`` before
    starting the process, so the process sees exactly the local paths. The
    NVIDIA runtime libraries the binary loads dynamically (nvrtc, cuBLAS) are
    staged next to the executable when the host has no CUDA toolkit. Outputs
    come back as an uncompressed tar and are compared exactly like local
    outputs; the remote copy is then deleted.
    """

    def __init__(self, workload: Workload, host: str, address: str, data_root: Path, mirror_root: str,
                 runtime_libraries: Iterable[Path] = (), cuda_root: Path | None = None,
                 runner: Callable[..., subprocess.CompletedProcess] = subprocess.run):
        self.workload = workload
        self.host = host
        self.address = address
        self.data_root = Path(data_root)
        self.mirror_root = mirror_root.rstrip("/\\").replace("\\", "/")
        self.runtime_libraries = [Path(path) for path in runtime_libraries]
        self.cuda_root = Path(cuda_root) if cuda_root else None
        self.runner = runner
        self.staged: dict | None = None
        self.stage_lock = threading.Lock()
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

    def scp(self, source: str, target: str, timeout: float = 3600) -> None:
        result = self.runner(["scp", "-q", "-r", source, target], capture_output=True, text=True, timeout=timeout)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host}: scp failed: {result.stderr.strip()[:400]}")

    @staticmethod
    def local(path: Path | str) -> str:
        text = str(path).replace("\\", "/")
        if not re.match(r"^[A-Za-z]:/", text) or " " in text:
            raise RuntimeError(f"remote placement needs absolute drive paths without spaces: {text}")
        return text

    def physical(self, path: Path | str) -> str:
        """Where a local absolute path lives on the remote disk."""
        text = self.local(path)
        return f"{self.mirror_root}/{text[0].upper()}/{text[3:]}"

    def mirror_path(self, letter: str) -> str:
        return f"{self.mirror_root}/{letter}".replace("/", "\\")

    def letters(self, *paths: Path | str) -> list[str]:
        return sorted({self.local(path)[0].upper() for path in paths})

    def gpus(self) -> list[dict]:
        """This host's GPU readings (same shape as the local gpu_inventory)."""
        result = self.runner(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", self.address] + GPU_QUERY,
                             capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            raise RuntimeError(f"{self.host}: nvidia-smi failed: {result.stderr.strip()[:200]}")
        return parse_gpu_csv(result.stdout)

    def remote_hash(self, path: Path | str) -> str:
        return self.ssh(f"(Get-FileHash -Algorithm SHA256 -LiteralPath '{self.physical(path)}').Hash", 300) \
            .strip().lower()

    def stage(self) -> dict:
        """Copy the executable, runtime libraries and data once per host; verify hashes."""
        with self.stage_lock:
            if self.staged is not None:
                return self.staged
            began = time.monotonic()
            executable = self.workload.executable
            directories = {self.physical(executable.parent), self.physical(self.data_root.parent)}
            self.ssh("\n".join(f"New-Item -ItemType Directory -Force -Path '{d}' > $null" for d in sorted(directories)),
                     120)
            self.scp(str(executable), f"{self.address}:{self.physical(executable)}")
            if self.remote_hash(executable) != self.workload.executable_sha256:
                raise RuntimeError(f"{self.host}: staged executable hash differs")
            libraries = {}
            for library in self.runtime_libraries:
                target = f"{self.physical(executable.parent)}/{library.name}"
                self.scp(str(library), f"{self.address}:{target}")
                digest = sha256_file(library)
                remote = self.ssh(f"(Get-FileHash -Algorithm SHA256 -LiteralPath '{target}').Hash", 300)
                if remote.strip().lower() != digest:
                    raise RuntimeError(f"{self.host}: staged runtime library hash differs: {library.name}")
                libraries[library.name] = digest
            self.scp(str(self.data_root), f"{self.address}:{self.physical(self.data_root.parent)}")
            if self.cuda_root is not None:
                # Kernel JIT (nvrtc) includes <cuda_runtime.h> from $CUDA_PATH/include.
                self.ssh(f"New-Item -ItemType Directory -Force -Path '{self.physical(self.cuda_root)}' > $null", 120)
                self.scp(str(self.cuda_root / "include"), f"{self.address}:{self.physical(self.cuda_root)}")
            seconds = time.monotonic() - began
            self.transfer_seconds += seconds
            self.staged = {"host": self.host, "mirror_root": self.mirror_root,
                           "executable_sha256": self.workload.executable_sha256,
                           "runtime_libraries_sha256": libraries, "stage_seconds": seconds}
            return self.staged

    def run(self, run: RunSpec, run_root: Path, device: int, stop_after: int | None,
            extra_env: dict[str, str] | None = None) -> RunResult:
        self.stage()
        run_root = Path(run_root).resolve()
        run_root.mkdir(parents=True, exist_ok=False)
        adapter = self.workload.adapter
        executable = self.local(self.workload.executable)
        argv = adapter.argv(self.workload, executable, run, self.local(run_root), device, stop_after)
        env = adapter.environment(self.workload, run, self.local(run_root), device, stop_after)
        env.update(extra_env or {})
        paths = [self.workload.executable, self.data_root, run_root]
        if self.cuda_root is not None:
            paths.append(self.cuda_root)
            env["CUDA_PATH"] = self.local(self.cuda_root)
        ticket = env.get("MULTIRUN_LAUNCH_TICKET")
        began = time.monotonic()
        self.ssh(f"New-Item -ItemType Directory -Force -Path '{self.physical(run_root)}' > $null", 120)
        output_root = adapter.output_root(run_root)
        if run.parent is not None:
            parent = adapter.output_root(run.parent)
            if parent.name != output_root.name:
                raise RuntimeError("parent and run output roots must share a directory name")
            self.ssh(f"New-Item -ItemType Directory -Force -Path '{self.physical(output_root.parent)}' > $null", 120)
            self.scp(str(parent), f"{self.address}:{self.physical(output_root.parent)}")
        if ticket:
            paths.append(ticket)
            self.ssh(f"New-Item -ItemType Directory -Force -Path '{self.physical(Path(ticket).parent)}' > $null", 120)
            self.scp(ticket, f"{self.address}:{self.physical(ticket)}")
        self.transfer_seconds += time.monotonic() - began
        # Map each drive letter onto the mirror (per SSH logon session), reusing
        # an existing mapping to the same mirror and refusing a real drive.
        mappings = "\n".join(
            f"$m = (subst | Select-String -SimpleMatch '{letter}:\\: => {self.mirror_path(letter)}')\n"
            f"if (-not $m) {{ if (Test-Path '{letter}:\\') {{ throw 'drive {letter}: exists on {self.host}' }};"
            f" subst {letter}: '{self.mirror_path(letter)}' }}"
            for letter in self.letters(*paths))
        assignments = "\n".join(f"$env:{key}='{value}'" for key, value in sorted(env.items()))
        log = f"{self.local(run_root)}/process.log"
        # cmd.exe redirection keeps the log in the process's own encoding
        # (Windows PowerShell 5.1 '*>' would write UTF-16).
        command = " ".join([argv[0].replace("/", "\\")] + argv[1:]) + f" > {log.replace('/', chr(92))} 2>&1"
        script = (
            f"{mappings}\n"
            f"Remove-Item Env:CUDA_VISIBLE_DEVICES -ErrorAction SilentlyContinue\n{assignments}\n"
            f"Set-Location '{self.local(run_root)}'\n"
            f"$started = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0\n"
            f"$p = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', '\"{command}\"' -NoNewWindow -PassThru\n"
            f"$null = $p.Handle\n"
            f"$overran = $false\n"
            # Watchdog, as locally: a process past its stop generation would
            # silently train the whole record.
            f"while (-not $p.HasExited) {{\n"
            f"  Start-Sleep -Seconds 2\n"
            f"  if ({-1 if stop_after is None else stop_after} -ge 0) {{\n"
            f"    $max = 0\n"
            f"    Get-ChildItem -LiteralPath '{self.local(output_root)}' -Recurse -File -ErrorAction SilentlyContinue |"
            f" ForEach-Object {{ if ($_.Name -match '{adapter.generation_name_regex}') {{"
            f" $g = [int64]$Matches[1]; if ($g -gt $max) {{ $max = $g }} }} }}\n"
            f"    if ($max -gt {-1 if stop_after is None else stop_after}) {{"
            f" & taskkill.exe /PID $p.Id /T /F > $null; $overran = $true; break }}\n"
            f"  }}\n"
            f"}}\n"
            f"$p.WaitForExit()\n"
            f"$code = if ($overran) {{ {LocalExecutor.OVERRAN} }} else {{ $p.ExitCode }}\n"
            f"$finished = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() / 1000.0\n"
            f"Set-Location '{self.mirror_root}'\n"
            f"& tar.exe -cf '{self.physical(run_root)}.tar' -C '{self.physical(run_root)}' .\n"
            f"if ($LASTEXITCODE -ne 0) {{ throw 'tar failed' }}\n"
            f"[pscustomobject]@{{code=$code; started=$started; finished=$finished}} | ConvertTo-Json -Compress"
        )
        write_json(run_root / "invocation.json",
                   {"host": self.host, "argv": argv, "env": env, "mirror_root": self.mirror_root})
        output = self.ssh(script, timeout=7 * 24 * 3600)
        receipt = json.loads(output.strip().splitlines()[-1])
        began = time.monotonic()
        archive = run_root.with_name(run_root.name + ".tar")
        self.scp(f"{self.address}:{self.physical(run_root)}.tar", str(archive))
        with tarfile.open(archive) as bundle:
            if hasattr(tarfile, "data_filter"):
                bundle.extractall(run_root, filter="data")
            else:  # Python < 3.11.4
                bundle.extractall(run_root)
        archive.unlink()
        self.ssh(f"Remove-Item -Recurse -Force -LiteralPath '{self.physical(run_root)}', "
                 f"'{self.physical(run_root)}.tar'", 600)
        pulled = time.monotonic() - began
        self.transfer_seconds += pulled
        code = int(receipt["code"])
        log_text = read_log(run_root / "process.log")
        if code == 0 and not adapter.log_succeeded(log_text):
            code = LocalExecutor.RAN_NOTHING
        # Remote clock throughout (mtimes survive the tar); the pull counts as tail.
        return RunResult(run.id, self.host, device, code, float(receipt["started"]),
                         float(receipt["finished"]) + pulled,
                         adapter.completed_generations(run_root, self.workload.start_generation),
                         str(run_root / "process.log"))


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


GPU_QUERY = ["nvidia-smi", "--query-gpu=index,name,uuid,memory.total,memory.used,utilization.gpu",
             "--format=csv,noheader,nounits"]


def parse_gpu_csv(text: str) -> list[dict]:
    devices = []
    for line in text.strip().splitlines():
        parts = [part.strip() for part in line.split(",")]
        if len(parts) != 6 or not parts[0].isdigit():
            continue
        index, name, uuid, total, used, utilization = parts
        devices.append({"index": int(index), "name": name, "uuid": uuid, "memory_total_mib": int(total),
                        "memory_used_mib": int(used), "utilization_percent": int(utilization)})
    return devices


def gpu_inventory(runner=subprocess.run) -> list[dict]:
    try:
        result = runner(GPU_QUERY, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return []
    return parse_gpu_csv(result.stdout)


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
                                        "the allocation width, not CPU, limits throughput (check "
                                        "GPU memory headroom and per-slot capacity)"})
            streak = 0
    return events


class Monitor(threading.Thread):
    """Samples CPU and GPU use during a leg or launch (policy item 6)."""

    def __init__(self, path: Path, waiting: Callable[[], int], interval: float = 5.0,
                 remote: dict[str, Callable[[], list[dict]]] | None = None):
        super().__init__(daemon=True)
        self.path = path
        self.waiting = waiting
        self.interval = interval
        self.remote = remote or {}
        self.stop_event = threading.Event()
        self.samples: list[dict] = []

    def run(self) -> None:
        cpu = CpuSampler()
        tick = 0
        with self.path.open("a", encoding="utf-8") as log:
            while not self.stop_event.wait(self.interval):
                sample = {"t": time.time(), "cpu_percent": cpu.sample(), "waiting_runs": self.waiting(),
                          "gpus": [{k: d[k] for k in ("index", "memory_used_mib", "utilization_percent")}
                                   for d in gpu_inventory()]}
                tick += 1
                if self.remote and tick % 3 == 1:  # an SSH round trip per host: sample every third tick
                    sample["remote_gpus"] = {}
                    for host, read in self.remote.items():
                        try:
                            sample["remote_gpus"][host] = [
                                {k: d[k] for k in ("index", "memory_used_mib", "utilization_percent")}
                                for d in read()]
                        except Exception as error:  # a missed sample is recorded, not fatal
                            sample["remote_gpus"][host] = repr(error)
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
                key = f"jack:{gpu['index']}"
                peaks[key] = max(peaks.get(key, 0), gpu["memory_used_mib"])
            for host, gpus in (sample.get("remote_gpus") or {}).items():
                for gpu in gpus if isinstance(gpus, list) else []:
                    key = f"{host}:{gpu['index']}"
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


# RunPod pods are Linux. The repository pins different training bytes per
# target: native_trainer_v1.rs keeps separate x86_64-pc-windows-msvc and
# x86_64-unknown-linux-gnu BurnPairNumericalWitnessV1 values whose
# train_state_sha256 differ, so a pod cannot reproduce a Windows serial golden.
RUNPOD_PLATFORM_REASON = ("account reachable, but RunPod pods are Linux and this Windows-built workload's "
                          "training bytes differ across targets (per-target train_state_sha256 witnesses in "
                          "mtg-kernel/src/native_trainer_v1.rs); qualifying it needs a Linux build with Linux "
                          "goldens and a lease guard, which this launcher does not implement")


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


def leg_statistics(results: list[RunResult], wall: float, stop_after: int, episodes_per_update: int,
                   start: int = 0) -> dict:
    """Completed work and the per-run timing model used for projection (updates counted past ``start``)."""
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
        startup.append(max(0.0, points[0][1] - result.started - (points[0][0] - start) * run_steady))
        tail.append(result.finished - points[-1][1])
    completed = sum(max((g for g in result.generations if g <= stop_after), default=start) - start
                    for result in results if result.exit_code == 0)
    episodes = completed * episodes_per_update
    mean = lambda values: sum(values) / len(values) if values else None  # noqa: E731
    return {"wall_seconds": wall, "completed_updates": completed, "episodes": episodes,
            "episodes_per_second": episodes / wall if wall > 0 else None,
            "startup_seconds": mean(startup), "steady_update_seconds": mean(steady), "tail_seconds": mean(tail)}


def project_seconds(host_statistics: dict[str, dict], slots: list[Slot], run_count: int, planned_updates: int,
                    overhead_seconds: float) -> float | None:
    """Completion time of the whole experiment on ``slots``.

    Simulates the launcher's scheduler: each run takes the first free
    process lane, and a run on host h lasts startup + planned x steady + tail
    as measured for h in the leg. Returns None if a host was not measured.
    """
    lanes = []
    for slot in slots:
        statistics = host_statistics.get(slot.host, {})
        parts = [statistics.get(key) for key in ("startup_seconds", "steady_update_seconds", "tail_seconds")]
        if any(part is None for part in parts):
            return None
        startup, steady, tail = parts
        lanes.extend([startup + planned_updates * steady + tail] * slot.capacity)
    free = [(0.0, index) for index in range(len(lanes))]
    heapq.heapify(free)
    finish = 0.0
    for _ in range(run_count):
        at, lane = heapq.heappop(free)
        done = at + lanes[lane]
        finish = max(finish, done)
        heapq.heappush(free, (done, lane))
    return overhead_seconds + finish


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


def settled_gpu_inventory(timeout: float = 60.0, interval: float = 2.0,
                          read: Callable[[], list[dict]] | None = None) -> list[dict]:
    """GPU readings once memory use has stopped changing.

    A process that just exited can hold its device memory for several seconds
    in nvidia-smi's view; deciding what fits from such a reading would reject
    allocations that do fit. Waits until three consecutive readings agree per
    device (or the timeout passes) and returns the last reading.
    """
    read = read or gpu_inventory
    readings = [read()]
    deadline = time.monotonic() + timeout
    while readings[-1] and time.monotonic() < deadline:
        time.sleep(interval)
        readings.append(read())
        used = [[gpu["memory_used_mib"] for gpu in reading] for reading in readings[-3:]]
        if len(used) == 3 and used[0] == used[1] == used[2]:
            break
    return readings[-1]


def device_fits(slots: list[Slot], per_process_mib: Callable[[str, int], float], margin_mib: int = FIT_MARGIN_MIB,
                inventories: Callable[[str], list[dict]] | None = None) -> list[str]:
    """Reasons an allocation cannot fit on its hosts' GPUs right now (empty if it fits).

    Local and remote slots follow the same rule: capacity x the measured
    per-process footprint of that host's device must fit in its free memory
    minus the margin. A remote device that oversubscribes spills into shared
    memory and slows every run on it.
    """
    inventories = inventories or (lambda host: settled_gpu_inventory() if host == "jack" else [])
    reasons = []
    cache: dict[str, dict[int, dict]] = {}
    for slot in slots:
        need = slot.capacity * (per_process_mib(slot.host, slot.device) or 0.0)
        if need <= 0:
            continue  # no measured device memory use: nothing to reserve
        if slot.host not in cache:
            cache[slot.host] = {gpu["index"]: gpu for gpu in inventories(slot.host)}
        gpu = cache[slot.host].get(slot.device)
        name = f"{slot.host}:{slot.device}"
        if gpu is None:
            reasons.append(f"device {name} not present")
            continue
        room = gpu["memory_total_mib"] - gpu["memory_used_mib"] - margin_mib
        if need > room:
            reasons.append(f"device {name}: {slot.capacity} processes need ~{need:.0f} MiB, "
                           f"{room:.0f} MiB free after margin")
    return reasons


def grow_allocation(slots: list[Slot], per_process: Callable[[int], float], margin_mib: int = FIT_MARGIN_MIB,
                    devices: list[dict] | None = None) -> list[Slot] | None:
    """The allocation plus one local process on the device with the most spare room, or None if none fits."""
    devices = gpu_inventory() if devices is None else devices
    capacity = {slot.device: slot.capacity for slot in slots if slot.host == "jack"}
    best = None
    for gpu in devices:
        need = per_process(gpu["index"])
        if need <= 0:
            continue
        room = gpu["memory_total_mib"] - gpu["memory_used_mib"] - margin_mib
        spare = room - need * (capacity.get(gpu["index"], 0) + 1)
        if spare >= 0 and (best is None or spare > best[0]):
            best = (spare, gpu["index"])
    if best is None:
        return None
    device = best[1]
    grown = [Slot(s.host, s.device, s.capacity + (s.host == "jack" and s.device == device)) for s in slots]
    if device not in capacity:
        grown.append(Slot("jack", device, 1))
    return sorted(grown, key=lambda s: (s.host != "jack", s.host, s.device))


def sentinel_entries(workload: Workload, slots: list[Slot]) -> list[tuple[str, RunSpec, Slot]]:
    """Full-length sentinel placements: every arm on every slot, every lane busy.

    Each slot gets max(capacity, arm count) entries cycling through the arms,
    each entry the arm's first run, so the sentinel leg runs at the selected
    width (the memory pressure the launch will see) and covers every
    placement-by-arm pair.
    """
    arms = workload.arms()
    first = {arm: next(run for run in workload.runs if run.arm == arm) for arm in arms}
    entries = []
    for slot in slots:
        for lane in range(max(slot.capacity, len(arms))):
            run = first[arms[lane % len(arms)]]
            entries.append((f"{run.id}--{slot.host}-{slot.device}-{lane}", run, slot))
    return entries


def execute_pinned(entries: list[tuple[str, RunSpec, Slot]], root: Path, stop_after: int | None,
                   executors: dict[str, object], extra_env: Callable[[str, RunSpec], dict] | None = None
                   ) -> tuple[dict[str, RunResult], float]:
    """Run labelled entries on their own slots, at most ``capacity`` at once per slot."""
    gates = {slot: threading.Semaphore(slot.capacity) for _, _, slot in entries}
    results: dict[str, RunResult] = {}
    began = time.time()

    def worker(label: str, run: RunSpec, slot: Slot) -> None:
        with gates[slot]:
            try:
                result = executors[slot.host].run(run, root / label, slot.device, stop_after,
                                                   extra_env(label, run) if extra_env else None)
            except Exception as error:  # recorded, never retried silently
                result = RunResult(run.id, slot.host, slot.device, None, time.time(), time.time(), {}, repr(error))
        results[label] = result

    threads = [threading.Thread(target=worker, args=entry, daemon=True) for entry in entries]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    return results, time.time() - began


def qualify(workload: Workload, root: Path, candidates: list[list[Slot]], qualification_updates: int,
            inventory: dict, executors: dict[str, object], overheads: dict[str, dict] | None = None,
            stop_on_saturation: bool = True, per_process_mib: float | None = None,
            auto_devices: list[int] | None = None) -> dict:
    """Serial goldens, then candidate allocations in increasing concurrency.

    With ``auto_devices`` the candidates are generated instead: serial on the
    first device, one process on every listed device (which measures each
    device's memory footprint), then one more process at a time on the
    device with the most spare memory, until nothing fits or throughput stops
    improving.
    """
    workload.adapter.validate_prefix(qualification_updates, workload.span)
    start = workload.start_generation
    prefix_stop = start + qualification_updates
    if root.exists():
        raise LaunchRefused(f"qualification root already exists: {root}")
    if auto_devices:
        if candidates or len(auto_devices) != len(set(auto_devices)):
            raise LaunchRefused("give either explicit allocations or distinct auto devices")
        candidates = [[Slot("jack", auto_devices[0], 1)]]
        candidates.append([Slot("jack", device, 1) for device in auto_devices] if len(auto_devices) > 1
                          else [Slot("jack", auto_devices[0], 2)])
    root.mkdir(parents=True)
    candidates = sorted(candidates, key=concurrency)
    serial = [candidate for candidate in candidates if concurrency(candidate) == 1]
    # One run is inherently sequential for a launcher that parallelizes across
    # runs (COMPUTE-POLICY item 4); every other workload must measure both.
    if not serial or (len(workload.runs) > 1 and not any(concurrency(candidate) > 1 for candidate in candidates)):
        raise LaunchRefused("qualification needs the serial allocation and at least one parallel allocation")
    overheads = dict(overheads or {})
    staging: dict[str, dict] = {}

    # Per-device GPU memory one process adds, learned from each measured leg
    # (the CUDA runtime's pool pages scale with the device's total memory).
    footprint: dict[tuple[str, int], float] = {}

    def remote_readers() -> dict[str, Callable[[], list[dict]]]:
        return {host: executor.gpus for host, executor in executors.items()
                if host != "jack" and hasattr(executor, "gpus")}

    def inventories(host: str) -> list[dict]:
        if host == "jack":
            return settled_gpu_inventory()
        reader = remote_readers().get(host)
        return settled_gpu_inventory(read=reader, interval=5.0) if reader else []

    def leg(label: str, slots: list[Slot]) -> tuple[list[RunResult], float, dict]:
        leg_root = root / label
        leg_root.mkdir()
        baseline = {(host, gpu["index"]): gpu["memory_used_mib"]
                    for host in {slot.host for slot in slots} for gpu in inventories(host)}
        waiting = {"count": len(workload.runs)}

        def on_event(kind, _payload):
            if kind == "run-started":
                waiting["count"] -= 1

        monitor = Monitor(leg_root / "monitor.jsonl", lambda: waiting["count"],
                          remote={h: r for h, r in remote_readers().items() if h in {s.host for s in slots}})
        monitor.start()
        results, wall = execute_allocation(
            workload, slots, leg_root, prefix_stop, executors,
            extra_env=qualification_tickets(workload, leg_root, allocation_text(slots), prefix_stop),
            on_event=on_event)
        usage = monitor.stop()
        for slot in slots:
            key = (slot.host, slot.device)
            peak = usage["gpu_peak_memory_mib"].get(f"{slot.host}:{slot.device}")
            if peak is not None and key in baseline:
                estimate = max(0.0, peak - baseline[key]) / slot.capacity
                footprint[key] = max(footprint.get(key, 0.0), estimate)
        usage["baseline_memory_mib"] = {f"{h}:{d}": v for (h, d), v in baseline.items()}
        return results, wall, usage

    def per_process(host: str, device: int) -> float:
        if per_process_mib is not None:
            return per_process_mib
        # An unmeasured device is assumed as large as the largest measured one.
        return footprint.get((host, device), max(footprint.values(), default=0.0))

    # The serial leg is the golden: each run alone on the first serial slot.
    golden_slots = serial[0]
    golden_results, golden_wall, golden_usage = leg("serial-golden", golden_slots)
    goldens = {}
    for result in golden_results:
        if result.exit_code != 0 or max(result.generations, default=start) != prefix_stop:
            raise LaunchRefused(f"serial golden for {result.run_id} did not complete "
                                f"(exit {result.exit_code}); see {result.log}")
        goldens[result.run_id] = workload.adapter.output_digests(root / "serial-golden" / result.run_id)
    # Serial repeat of the first run: proves the outputs are a function of the inputs at all.
    repeat_root = root / "serial-repeat"
    repeat_root.mkdir()
    repeat = executors[golden_slots[0].host].run(
        workload.runs[0], repeat_root / workload.runs[0].id, golden_slots[0].device, prefix_stop,
        qualification_tickets(workload, repeat_root, allocation_text(golden_slots),
                              prefix_stop)(workload.runs[0]))
    repeat_digests = workload.adapter.output_digests(repeat_root / workload.runs[0].id)
    if repeat.exit_code != 0 or repeat_digests != goldens[workload.runs[0].id]:
        differing = sorted(k for k in set(repeat_digests) | set(goldens[workload.runs[0].id])
                           if repeat_digests.get(k) != goldens[workload.runs[0].id].get(k))
        raise LaunchRefused(f"serial repeat is not byte-identical; nondeterministic outputs: {differing[:20]}")

    records = []

    def record(slots, results, wall, usage, status_override=None, reasons=(), leg_root=None):
        statistics = leg_statistics(results, wall, prefix_stop, workload.adapter.episodes_per_update, start)
        leg_root = leg_root or root / label_of(slots)
        per_run = compare_to_goldens(workload, results, leg_root, goldens) if results else {}
        reasons = list(reasons)
        if results and not all(entry["byte_identical"] and entry["exit_code"] == 0 for entry in per_run.values()):
            reasons.append("a concurrent run differs from its serial golden or failed")
        overhead = sum(overheads.get(host, {}).get(key, 0.0)
                       for host in {slot.host for slot in slots}
                       for key in ("setup_seconds", "transfer_seconds", "recovery_seconds"))
        status = status_override or ("qualified" if not reasons else "disqualified")
        host_statistics = {host: leg_statistics([r for r in results if r.host == host], wall, prefix_stop,
                                                workload.adapter.episodes_per_update, start)
                           for host in {slot.host for slot in slots}}
        projected = project_seconds(host_statistics, slots, len(workload.runs), workload.span,
                                    overhead) if status == "qualified" else None
        if status == "qualified" and projected is None:
            # Some host ran too little to time; never rank what was not measured.
            status = "unrankable"
            reasons.append("no timing for every host in the allocation")
        entry = {"id": label_of(slots), "allocation": allocation_text(slots), "concurrency": concurrency(slots),
                 "hosts": sorted({slot.host for slot in slots}), "status": status, "reasons": reasons,
                 "overhead_seconds": overhead, "projected_seconds": projected, "usage": usage,
                 "host_statistics": {h: {k: v for k, v in st.items() if k.endswith("_seconds")}
                                     for h, st in host_statistics.items()},
                 "per_run": per_run, **statistics}
        records.append(entry)
        return entry

    record(golden_slots, golden_results, golden_wall, golden_usage, leg_root=root / "serial-golden")
    best = records[0]["episodes_per_second"] or 0.0
    not_useful = 0
    pending = [slots for slots in candidates if slots != golden_slots]
    while pending:
        slots = pending.pop(0)
        fit = device_fits(slots, per_process, inventories=inventories) \
            if any(per_process(s.host, s.device) > 0 for s in slots) else []
        missing = [slot.host for slot in slots if not inventory["hosts"].get(slot.host, {}).get("eligible")]
        wider = [f"{concurrency(slots)} lanes for {len(workload.runs)} runs would leave a slot unmeasured"]             if concurrency(slots) > len(workload.runs) else []
        if missing or fit or wider:
            record(slots, [], 0.0, {}, "capacity-skipped",
                   [f"host not eligible: {host}" for host in sorted(set(missing))] + fit + wider)
            continue
        results, wall, usage = leg(label_of(slots), slots)
        for host in {slot.host for slot in slots}:
            staged = getattr(executors.get(host), "staged", None)
            if staged:
                # A launch stages again in its own process: count it once per launch.
                overheads.setdefault(host, {})["setup_seconds"] = staged["stage_seconds"]
                staging[host] = staged
        entry = record(slots, results, wall, usage)
        throughput = entry["episodes_per_second"] or 0.0
        if entry["status"] == "qualified" and throughput > best * (1 + USEFUL_GAIN_FRACTION):
            best, not_useful = throughput, 0
        else:
            not_useful += 1
            if stop_on_saturation and not_useful >= 2:
                break
        if auto_devices and entry["status"] == "qualified" and not pending:
            grown = grow_allocation(slots, lambda device: per_process("jack", device),
                                    devices=settled_gpu_inventory())
            if grown is not None:
                pending.append(grown)

    # Full-length sentinel (verdict M1): the fastest qualified allocation must
    # also reproduce full-length serial runs byte for byte on every placement
    # and arm, at its own width; otherwise it is disqualified and the next
    # fastest is tried.
    target = workload.target_generation
    serial_root = root / "sentinel-serial"
    serial_full: dict[str, dict[str, str]] = {}
    golden_slot = golden_slots[0]

    def sentinel_ticket(directory: Path, allocation: str):
        def env(label: str, run: RunSpec) -> dict:
            ticket = issue_ticket(directory / f"{label}.ticket.json", workload, run, allocation, "sentinel",
                                  stop_after=workload.launch_stop)
            return {"MULTIRUN_LAUNCH_TICKET": str(ticket)}
        return env

    def run_sentinel(candidate: dict) -> dict:
        slots = parse_allocation(candidate["allocation"])
        entries = sentinel_entries(workload, slots)
        for run in {run.id: run for _, run, _ in entries}.values():
            if run.id in serial_full:
                continue
            serial_root.mkdir(exist_ok=True)
            result = executors[golden_slot.host].run(
                run, serial_root / run.id, golden_slot.device, workload.launch_stop,
                sentinel_ticket(serial_root, allocation_text([golden_slot]))(run.id, run))
            if result.exit_code != 0 or max(result.generations, default=start) != target:
                raise LaunchRefused(f"full-length serial sentinel for {run.id} did not complete "
                                    f"(exit {result.exit_code}); see {result.log}")
            serial_full[run.id] = workload.adapter.output_digests(serial_root / run.id)
        sentinel_root = root / f"sentinel-{candidate['id']}"
        sentinel_root.mkdir()
        hosts_used = {slot.host for slot in slots}
        totals = {f"{host}:{gpu['index']}": gpu["memory_total_mib"] for host in hosts_used for gpu in inventories(host)}
        monitor = Monitor(sentinel_root / "monitor.jsonl", lambda: 0,
                          remote={h: r for h, r in remote_readers().items() if h in hosts_used})
        monitor.start()
        results, wall = execute_pinned(entries, sentinel_root, workload.launch_stop, executors,
                                       sentinel_ticket(sentinel_root, candidate["allocation"]))
        usage = monitor.stop()
        # Device residency grows with depth, so a width that fit at the prefix
        # can crowd a device at full length (an oversubscribed device spills to
        # shared memory and slows every run on it): the full-length peak must
        # stay outside the fit margin too.
        peaks = usage["gpu_peak_memory_mib"]
        crowded = sentinel_memory_problems(slots, peaks, totals,
                                           {f"{host}:{device}": mib for (host, device), mib in footprint.items()})
        rows = []
        for label, run, slot in entries:
            result = results[label]
            digests = workload.adapter.output_digests(sentinel_root / label)
            reference = serial_full[run.id]
            rows.append({"label": label, "run_id": run.id, "arm": run.arm, "host": slot.host, "device": slot.device,
                         "exit_code": result.exit_code,
                         "completed_generation": max(result.generations, default=start),
                         "output_count": len(digests), "digest_set_sha256": sha256_bytes(canonical(digests)),
                         "byte_identical": result.exit_code == 0 and bool(digests) and digests == reference,
                         "differing_outputs": sorted(k for k in set(digests) | set(reference)
                                                     if digests.get(k) != reference.get(k))[:20]})
        passed = all(row["byte_identical"] and row["completed_generation"] == target for row in rows)             and not crowded
        return {"allocation": candidate["allocation"], "candidate": candidate["id"], "root": str(sentinel_root),
                "wall_seconds": wall, "passed": passed, "memory_crowded": crowded,
                "gpu_peak_memory_mib": peaks, "gpu_total_memory_mib": totals, "entries": rows}

    attempts = []
    while True:
        qualified = [entry for entry in records if entry["status"] == "qualified" and entry["projected_seconds"]]
        if not qualified:
            raise LaunchRefused("no allocation passed both the prefix qualification and the full-length sentinel")
        selected = min(qualified, key=lambda entry: entry["projected_seconds"])
        attempt = run_sentinel(selected)
        attempts.append(attempt)
        if attempt["passed"]:
            break
        selected["status"] = "disqualified"
        selected["reasons"].append("full-length sentinel differs from the serial reference, did not finish, "
                                   "or crowded a GPU: " + "; ".join(attempt["memory_crowded"]))
        selected["projected_seconds"] = None
    sentinel = dict(attempts[-1], serial={
        run_id: {"root": str(serial_root / run_id), "output_count": len(digests),
                 "digest_set_sha256": sha256_bytes(canonical(digests))}
        for run_id, digests in serial_full.items()})
    choice = {
        "schema": CHOICE_SCHEMA,
        "created_at": iso(utc_now()),
        "adapter": workload.adapter.name,
        "workload_sha256": workload.sha256,
        "workload": {k: v for k, v in workload.raw.items() if k not in MACHINE_FIELDS},
        "executable_sha256": workload.executable_sha256,
        "data_tree_sha256": workload.data_tree_sha256(),
        "launcher_sha256": sha256_file(Path(__file__)),
        "planned_updates": workload.planned_updates,
        "start_generation": start,
        "target_generation": target,
        "qualification_updates": qualification_updates,
        "run_ids": [run.id for run in workload.runs],
        "gpu_footprint_mib": {f"{host}:{device}": mib for (host, device), mib in sorted(footprint.items())},
        "remote_staging": staging,
        "inventory": inventory,
        "golden": {"allocation": allocation_text(golden_slots), "root": str(root / "serial-golden"),
                   "serial_repeat": {"run_id": workload.runs[0].id, "root": str(repeat_root),
                                     "digest_set_sha256": sha256_bytes(canonical(repeat_digests))},
                   "runs": {run_id: {"digests": digests, "digest_set_sha256": sha256_bytes(canonical(digests))}
                            for run_id, digests in goldens.items()}},
        "candidates": records,
        "selected": selected["id"],
        "sentinel": sentinel,
        "sentinel_failures": [{k: a[k] for k in ("candidate", "allocation", "root", "memory_crowded")}
                              for a in attempts[:-1]],
        "selection_rule": "minimum projected completion seconds among byte-identical, fully completed "
                          "allocations; projection simulates the scheduler with each host's measured "
                          "startup + planned_updates * steady_update + tail per run, plus staging overhead",
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
    if choice.get("launcher_sha256") != sha256_file(Path(__file__)):
        raise LaunchRefused("requalify: the launcher changed since this receipt was written")
    if choice.get("data_tree_sha256") != workload.data_tree_sha256():
        raise LaunchRefused("requalify: the runtime data the executable reads changed")
    if choice.get("workload_sha256") != workload.sha256 or choice.get("adapter") != workload.adapter.name:
        raise LaunchRefused("requalify: the workload (knobs, runs, length or adapter) changed")
    if choice.get("run_ids") != [run.id for run in workload.runs]:
        raise LaunchRefused("the receipt covers a different run set")
    if choice.get("planned_updates") != workload.planned_updates or \
            choice.get("start_generation") != workload.start_generation or \
            choice.get("target_generation") != workload.target_generation:
        raise LaunchRefused("the receipt covers a different run length or segment")
    updates = choice.get("qualification_updates")
    if type(updates) is not int:
        raise LaunchRefused("qualification prefix is missing")
    workload.adapter.validate_prefix(updates, workload.span)

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
    golden_runs = golden.get("runs", {})
    if set(golden_runs) != {run.id for run in workload.runs}:
        raise LaunchRefused("every run needs a serial golden")
    repeat = golden.get("serial_repeat") or {}
    if repeat.get("run_id") not in golden_runs or \
            repeat.get("digest_set_sha256") != golden_runs[repeat["run_id"]].get("digest_set_sha256"):
        raise LaunchRefused("serial outputs were never shown to be reproducible (repeat digests differ or missing)")
    if verify_golden_files and sha256_bytes(canonical(workload.adapter.output_digests(
            Path(repeat["root"]) / repeat["run_id"]))) != repeat["digest_set_sha256"]:
        raise LaunchRefused("serial repeat outputs on disk no longer match the receipt")
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
    if not any(c.get("concurrency") == 1 for c in measured) or (
            len(workload.runs) > 1 and not any(c.get("concurrency", 0) > 1 for c in measured)):
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
        # Measured means a run actually executed there, not merely a listed slot.
        if status["eligible"] and not any(entry.get("host") == host for c in measured
                                          for entry in c.get("per_run", {}).values()):
            raise LaunchRefused(f"eligible placement has no throughput measurement: {host}")
    fastest = min(qualified, key=lambda c: c["projected_seconds"])
    selected = next((c for c in qualified if c["id"] == choice.get("selected")), None)
    if selected is None or selected["projected_seconds"] > fastest["projected_seconds"]:
        raise LaunchRefused("the selected allocation is slower than a qualified alternative")
    require_sentinel(choice.get("sentinel") or {}, selected, workload, verify_golden_files,
                     choice.get("gpu_footprint_mib") or {})
    return selected


def sentinel_memory_problems(slots: list[Slot], peaks: dict, totals: dict, footprint: dict) -> list[str]:
    """Verdict N1: every GPU the allocation uses needs a full-length reading outside the fit margin.

    A device counts as used when qualification measured a nonzero per-process
    footprint on it (a workload that never touches a GPU needs no reading).
    A used device with no peak or no total reading is a problem, never a pass.
    """
    problems = []
    for key in sorted({f"{slot.host}:{slot.device}" for slot in slots}):
        if footprint.get(key, 0) <= 0:
            continue
        if key not in peaks or key not in totals:
            problems.append(f"{key}: no full-length memory reading")
        elif peaks[key] > totals[key] - FIT_MARGIN_MIB:
            problems.append(f"{key}: full-length peak {peaks[key]} MiB of {totals[key]} MiB")
    return problems


def require_sentinel(sentinel: dict, selected: dict, workload: Workload, verify_files: bool,
                     footprint: dict | None = None) -> None:
    """Verdict M1: full-length identity on every placement and arm of the selected allocation."""
    if sentinel.get("allocation") != selected["allocation"] or sentinel.get("passed") is not True:
        raise LaunchRefused("no passing full-length sentinel for the selected allocation")
    problems = sentinel_memory_problems(parse_allocation(selected["allocation"]),
                                        sentinel.get("gpu_peak_memory_mib") or {},
                                        sentinel.get("gpu_total_memory_mib") or {}, footprint or {})
    if sentinel.get("memory_crowded") != [] or problems:
        raise LaunchRefused("the full-length sentinel crowded a GPU or lacks a memory reading: "
                            + "; ".join(problems or sentinel.get("memory_crowded") or ["no memory check"]))
    serial = sentinel.get("serial", {})
    target = workload.target_generation
    for run_id, reference in serial.items():
        if verify_files and sha256_bytes(canonical(workload.adapter.output_digests(
                Path(reference["root"])))) != reference.get("digest_set_sha256"):
            raise LaunchRefused(f"full-length serial sentinel outputs on disk no longer match: {run_id}")
    covered = set()
    for row in sentinel.get("entries", []):
        reference = serial.get(row.get("run_id"))
        if reference is None or row.get("digest_set_sha256") != reference.get("digest_set_sha256") or \
                row.get("byte_identical") is not True or row.get("exit_code") != 0 or \
                row.get("completed_generation") != target:
            raise LaunchRefused(f"full-length sentinel entry {row.get('label')} is not identical to its serial run")
        covered.add((row["host"], row["device"], row.get("arm")))
    needed = {(slot.host, slot.device, arm) for slot in parse_allocation(selected["allocation"])
              for arm in workload.arms()}
    if not needed <= covered:
        raise LaunchRefused(f"full-length sentinel misses placements x arms: {sorted(map(str, needed - covered))}")


def recorded_gpus(detail: dict) -> dict[int, tuple[str, str]]:
    """GPU index -> (name, uuid) from an inventory record (local dicts or remote CSV lines)."""
    found = {}
    for gpu in detail.get("gpus") or []:
        if isinstance(gpu, dict):
            found[int(gpu["index"])] = (gpu.get("name", ""), gpu.get("uuid", ""))
        else:
            parts = [part.strip() for part in str(gpu).split(",")]
            if len(parts) >= 3 and parts[0].isdigit():
                found[int(parts[0])] = (parts[1], parts[2])
    return found


def check_gpu_identity(choice: dict, slots: list[Slot], current: Callable[[str], dict[int, tuple[str, str]]]) -> None:
    """Verdict M4: every device the allocation uses is the device the receipt measured."""
    hosts = choice["inventory"]["hosts"]
    footprint = choice.get("gpu_footprint_mib", {})
    live: dict[str, dict] = {}
    for slot in slots:
        recorded = recorded_gpus(hosts.get(slot.host, {}).get("detail", {}))
        if slot.device not in recorded:
            if recorded or footprint.get(f"{slot.host}:{slot.device}", 0) > 0 or slot.host != "jack":
                raise LaunchRefused(f"the receipt records no GPU {slot.device} on {slot.host}")
            continue  # a workload that measured no GPU use on a GPU-less inventory
        if slot.host not in live:
            live[slot.host] = current(slot.host)
        if live[slot.host].get(slot.device) != recorded[slot.device]:
            raise LaunchRefused(f"GPU {slot.device} on {slot.host} is not the device the receipt measured "
                                f"({live[slot.host].get(slot.device)} vs {recorded[slot.device]})")


def live_gpus(haleys_address: str) -> Callable[[str], dict[int, tuple[str, str]]]:
    def current(host: str) -> dict[int, tuple[str, str]]:
        if host == "jack":
            return recorded_gpus({"gpus": gpu_inventory()})
        return recorded_gpus(probe_ssh(haleys_address)["detail"])
    return current


def issue_ticket(path: Path, workload: Workload, run: RunSpec, allocation: str, kind: str,
                 choice_sha256: str | None = None, stop_after: int | None = None) -> Path:
    """Per-run ticket the harness checks (mtg-kernel/src/multirun_launch_ticket_v1.rs).

    It binds this executable, the run's seed, the planned length and the
    stop point; a launch ticket also binds the compute-choice receipt.
    """
    write_json(path, {"schema": TICKET_SCHEMA, "kind": kind, "executable_sha256": workload.executable_sha256,
                      "compute_choice_sha256": choice_sha256, "workload_sha256": workload.sha256,
                      "run_id": run.id, "seed": run.seed, "planned_updates": workload.planned_updates,
                      "stop_after_generation": stop_after,
                      "expected_resume_generation": workload.resume_generation,
                      "allocation": allocation, "issued_at": iso(utc_now())})
    return path


def qualification_tickets(workload: Workload, leg_root: Path, allocation: str,
                          stop_after: int) -> Callable[[RunSpec], dict]:
    def env(run: RunSpec) -> dict:
        ticket = issue_ticket(leg_root / f"{run.id}.ticket.json", workload, run, allocation, "qualification",
                              stop_after=stop_after)
        return {"MULTIRUN_LAUNCH_TICKET": str(ticket)}
    return env


def launch(workload: Workload, choice_path: Path, root: Path, executors: dict[str, object],
           now: datetime | None = None, verify_golden_files: bool = True,
           gpus: Callable[[str], dict[int, tuple[str, str]]] | None = None) -> dict:
    selected = require_choice(choice_path, workload, now, verify_golden_files)
    choice = read_json(choice_path)
    choice_sha256 = sha256_file(choice_path)
    slots = parse_allocation(selected["allocation"])
    if root.exists():
        raise LaunchRefused(f"launch root already exists: {root}")
    check_gpu_identity(choice, slots, gpus or live_gpus(HALEYSPC_ADDRESS))
    # Current reservations win: refuse rather than squeeze in beside other GPU work.
    footprint = choice.get("gpu_footprint_mib", {})

    def launch_inventory(host: str) -> list[dict]:
        if host == "jack":
            return settled_gpu_inventory()
        reader = getattr(executors.get(host), "gpus", None)
        return settled_gpu_inventory(read=reader, interval=5.0) if reader else []

    crowded = device_fits(slots, lambda host, device: footprint.get(f"{host}:{device}",
                                                                    max(footprint.values(), default=0.0)),
                          inventories=launch_inventory)
    if crowded:
        raise LaunchRefused("not enough free GPU memory for the qualified allocation right now: " + "; ".join(crowded))
    runs_root = root / "runs"
    runs_root.mkdir(parents=True)
    manifest_path = root / "experiment-manifest.json"
    manifest = {
        "schema": MANIFEST_SCHEMA, "created_at": iso(utc_now()), "workload_sha256": workload.sha256,
        "executable": str(workload.executable), "executable_sha256": workload.executable_sha256,
        "data_tree_sha256": workload.data_tree_sha256(), "launcher_sha256": sha256_file(Path(__file__)),
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
                     choice_sha256=choice_sha256, stop_after=workload.launch_stop)
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

    monitor = Monitor(root / "monitor.jsonl", lambda: waiting["count"],
                      remote={s.host: executors[s.host].gpus for s in slots
                              if s.host != "jack" and hasattr(executors.get(s.host), "gpus")})
    monitor.start()
    results, wall = execute_allocation(
        workload, slots, runs_root, workload.launch_stop, executors,
        extra_env=lambda run: {"MULTIRUN_LAUNCH_TICKET": str(runs_root / f"{run.id}.ticket.json")},
        on_event=on_event)
    usage = monitor.stop()
    golden_runs = choice["golden"]["runs"]
    prefix = workload.start_generation + choice["qualification_updates"]
    start, target = workload.start_generation, workload.target_generation
    for result in results:
        run_root = runs_root / result.run_id
        final = workload.adapter.output_digests(run_root)
        shared = workload.adapter.output_digests(run_root, through_generation=prefix)
        golden_prefix = {k: v for k, v in golden_runs[result.run_id]["digests"].items()
                         if workload.adapter.static(k)
                         or ((g := workload.adapter.generation_of(k)) is not None and g <= prefix)}
        entry = manifest["runs"][result.run_id]
        completed = max(result.generations, default=start)
        prefix_identical = bool(golden_prefix) and shared == golden_prefix
        # Verdict M2: a run is complete only if it finished AND its prefix
        # still matches its serial golden; otherwise the experiment fails.
        failure = ("exit" if result.exit_code != 0 else "incomplete" if completed != target
                   else "prefix-differs-from-serial-golden" if not prefix_identical else None)
        entry.update({
            "run_root": str(run_root), "exit_code": result.exit_code,
            "completed_generation": completed,
            "status": "complete" if failure is None else "failed", "failure": failure,
            "wall_seconds": result.finished - result.started,
            "output_digest_set_sha256": sha256_bytes(canonical(final)),
            "golden_prefix_generation": prefix,
            "golden_prefix_identical": prefix_identical,
            "differing_prefix_outputs": sorted(k for k in set(shared) | set(golden_prefix)
                                               if shared.get(k) != golden_prefix.get(k))[:20],
            "log": result.log,
        })
        write_json(run_root / "run-manifest.json", {"schema": RUN_MANIFEST_SCHEMA, "run_id": result.run_id,
                                                     **entry, "experiment_manifest": str(manifest_path),
                                                     "compute_choice_sha256": choice_sha256})
    episodes = sum(e.get("completed_generation", start) - start for e in manifest["runs"].values()) \
        * workload.adapter.episodes_per_update
    manifest.update(status="complete" if all(e["status"] == "complete" for e in manifest["runs"].values())
                    else "failed", wall_seconds=wall, episodes=episodes,
                    episodes_per_second=episodes / wall if wall else None, usage=usage,
                    finished_at=iso(utc_now()))
    save()
    return manifest


def verify(workload: Workload, choice_path: Path, launch_root: Path, run_ids: list[str], root: Path,
           executor: LocalExecutor, device: int = 0) -> dict:
    """Rerun launched runs one at a time at full length and compare every output byte.

    The qualification proves concurrency preserves the golden prefix; this
    extends the check to the whole record for chosen runs. Each rerun is a
    guarded launch of the same run (the receipt must still validate), so it
    carries a launch ticket bound to the same receipt.
    """
    require_choice(choice_path, workload)
    choice_sha256 = sha256_file(choice_path)
    manifest = read_json(launch_root / "experiment-manifest.json")
    if manifest.get("compute_choice_sha256") != choice_sha256 or manifest.get("workload_sha256") != workload.sha256:
        raise LaunchRefused("the launch manifest was produced from another receipt or workload")
    if root.exists():
        raise LaunchRefused(f"verification root already exists: {root}")
    root.mkdir(parents=True)
    runs = {run.id: run for run in workload.runs}
    records = {}
    for run_id in run_ids:
        entry = manifest["runs"].get(run_id, {})
        if run_id not in runs or entry.get("status") != "complete":
            raise LaunchRefused(f"{run_id} is not a completed run of this launch")
        ticket = issue_ticket(root / f"{run_id}.ticket.json", workload, runs[run_id], f"1@jack:{device}",
                              "launch", choice_sha256=choice_sha256, stop_after=workload.launch_stop)
        result = executor.run(runs[run_id], root / run_id, device, workload.launch_stop,
                              {"MULTIRUN_LAUNCH_TICKET": str(ticket)})
        serial = workload.adapter.output_digests(root / run_id)
        concurrent = workload.adapter.output_digests(Path(entry["run_root"]))
        records[run_id] = {
            "exit_code": result.exit_code, "wall_seconds": result.finished - result.started,
            "serial_completed_generation": max(result.generations, default=workload.start_generation),
            "output_count": len(serial), "serial_digest_set_sha256": sha256_bytes(canonical(serial)),
            "concurrent_digest_set_sha256": sha256_bytes(canonical(concurrent)),
            "byte_identical": result.exit_code == 0 and bool(serial) and serial == concurrent,
            "differing_outputs": sorted(k for k in set(serial) | set(concurrent)
                                        if serial.get(k) != concurrent.get(k))[:20],
        }
    record = {"schema": "mtg-kernel-multirun-full-length-verification/v1", "created_at": iso(utc_now()),
              "compute_choice_sha256": choice_sha256, "launch_manifest": str(launch_root / "experiment-manifest.json"),
              "launch_manifest_sha256": sha256_file(launch_root / "experiment-manifest.json"),
              "device": device, "runs": records}
    write_json(root / "verification.json", record)
    return record


def resume_equivalence(workload: Workload, resumed_launch: Path, uninterrupted_launch: Path,
                       through_generation: int) -> dict:
    """Compare a resumed segment's launched runs with uninterrupted runs of the same seeds.

    Every output of generations up to ``through_generation`` (plus static
    outputs) must match byte for byte; the outputs trained after the resume
    point are counted so the comparison cannot pass on copied parents alone.
    """
    if workload.resume_generation is None or not workload.start_generation < through_generation:
        raise LaunchRefused("resume equivalence needs a segment workload and a generation past its resume point")
    manifests = {name: read_json(root / "experiment-manifest.json")
                 for name, root in (("resumed", resumed_launch), ("uninterrupted", uninterrupted_launch))}
    rows = {}
    for run in workload.runs:
        resumed = workload.adapter.output_digests(resumed_launch / "runs" / run.id, through_generation)
        straight = workload.adapter.output_digests(uninterrupted_launch / "runs" / run.id, through_generation)
        trained = [k for k in resumed if (g := workload.adapter.generation_of(k)) is not None
                   and g > workload.start_generation]
        rows[run.id] = {
            "resumed_status": manifests["resumed"]["runs"].get(run.id, {}).get("status"),
            "uninterrupted_status": manifests["uninterrupted"]["runs"].get(run.id, {}).get("status"),
            "outputs_compared": len(resumed), "outputs_trained_after_resume": len(trained),
            "identical": bool(trained) and resumed == straight,
            "differing_outputs": sorted(k for k in set(resumed) | set(straight)
                                        if resumed.get(k) != straight.get(k))[:20],
        }
    return {"schema": "mtg-kernel-multirun-resume-equivalence/v2", "created_at": iso(utc_now()),
            "segment": workload.raw["segment"], "through_generation": through_generation,
            "launcher_sha256": sha256_file(Path(__file__)),
            "resumed_launch_manifest_sha256": sha256_file(resumed_launch / "experiment-manifest.json"),
            "uninterrupted_launch_manifest_sha256": sha256_file(uninterrupted_launch / "experiment-manifest.json"),
            "compared": "every output of generations up to through_generation, plus static outputs",
            "runs": rows}


# --------------------------------------------------------------------------
# CLI


def build_executors(workload: Workload, haleys_address: str) -> dict[str, object]:
    """Local executor, plus HaleysPC when the workload names the files a remote run needs.

    ``data_root`` is the repository data directory the executable reads by
    path; ``remote_mirror_root`` is where the remote host keeps the mirrored
    drives; ``remote_runtime_libraries`` are NVIDIA runtime DLLs to stage
    next to the executable and ``remote_cuda_root`` a directory whose
    ``include`` holds the CUDA headers the kernel JIT needs (all
    machine-local, outside the workload identity).
    """
    executors: dict[str, object] = {"jack": LocalExecutor(workload)}
    raw = workload.raw
    if raw.get("data_root") and raw.get("remote_mirror_root"):
        executors["haleyspc"] = SshPowerShellExecutor(
            workload, "haleyspc", haleys_address, Path(raw["data_root"]), raw["remote_mirror_root"],
            [Path(path) for path in raw.get("remote_runtime_libraries", [])], raw.get("remote_cuda_root"))
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
    qualify_parser.add_argument("--allocation", action="append", default=[],
                                help="e.g. 1@0 (serial), 1@0+1@1, 3@0+1@1, 2@0+2@haleyspc:0")
    qualify_parser.add_argument("--auto", help="local devices to sweep adaptively, e.g. 0,1 (instead of "
                                               "--allocation)")
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
    verify_parser = commands.add_parser("verify", help="rerun launched runs serially at full length and compare")
    verify_parser.add_argument("--workload", type=Path, required=True)
    verify_parser.add_argument("--choice", type=Path, required=True)
    verify_parser.add_argument("--launch-root", type=Path, required=True)
    verify_parser.add_argument("--run", action="append", required=True)
    verify_parser.add_argument("--root", type=Path, required=True)
    verify_parser.add_argument("--device", type=int, default=0)
    equivalence = commands.add_parser("equivalence", help="compare a resumed segment with uninterrupted runs")
    equivalence.add_argument("--workload", type=Path, required=True)
    equivalence.add_argument("--resumed-launch", type=Path, required=True)
    equivalence.add_argument("--uninterrupted-launch", type=Path, required=True)
    equivalence.add_argument("--through", type=int, required=True)
    equivalence.add_argument("--out", type=Path, required=True)
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
            auto = [int(device) for device in arguments.auto.split(",")] if arguments.auto else None
            choice = qualify(workload, arguments.root, [parse_allocation(text) for text in arguments.allocation],
                             arguments.updates, inventory_record, executors,
                             stop_on_saturation=not arguments.no_early_stop, auto_devices=auto)
            print(json.dumps({"selected": choice["selected"], "candidates": [
                {k: c[k] for k in ("id", "status", "episodes_per_second", "projected_seconds")}
                for c in choice["candidates"]]}, indent=2))
            return 0
        if arguments.command == "check":
            print(json.dumps(require_choice(arguments.choice, workload)["id"]))
            return 0
        if arguments.command == "equivalence":
            record = resume_equivalence(workload, arguments.resumed_launch, arguments.uninterrupted_launch,
                                        arguments.through)
            write_json(arguments.out, record)
            print(json.dumps({run: row["identical"] for run, row in record["runs"].items()}))
            return 0 if all(row["identical"] for row in record["runs"].values()) else 1
        if arguments.command == "verify":
            record = verify(workload, arguments.choice, arguments.launch_root, arguments.run, arguments.root,
                            LocalExecutor(workload), arguments.device)
            print(json.dumps({run: entry["byte_identical"] for run, entry in record["runs"].items()}))
            return 0 if all(entry["byte_identical"] for entry in record["runs"].values()) else 1
        manifest = launch(workload, arguments.choice, arguments.root,
                          build_executors(workload, arguments.haleyspc), gpus=live_gpus(arguments.haleyspc))
        print(json.dumps({"status": manifest["status"], "manifest": str(arguments.root / "experiment-manifest.json")}))
        return 0 if manifest["status"] == "complete" else 1
    except LaunchRefused as refusal:
        print(f"launch refused: {refusal}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
