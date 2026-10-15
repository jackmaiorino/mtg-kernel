"""Guarded, local two-lineage archive-distillation batch dispatch.

qualify PLAN --root FRESH uses allocation.mode=evidence_acquisition to measure
startup dry-runs and unchanged recipes' 32-step prefixes at one then two
processes. It needs fresh host/input evidence, not comparative timings or
future formal budgets. Its receipt is throughput evidence only. run PLAN
--root FRESH --qualification RECEIPT requires allocation.mode=formal, exact
prefix parity, fastest feasible placement and nonzero formal/publication
budgets. Full-dose estimates are extrapolations, not measurements.

Run inside host_slots_v1 run, or a held host reservation with timed cores.
This tool neither acquires other hosts nor rents compute. The plan records
fresh evidence for desktop, computehost and RunPod, and the comparative local
allocation decision including transfer and exact absolute input availability.
All attempts, including failed outputs, are retained. No resume/delete API.
"""
from __future__ import annotations

import argparse
import copy
import ctypes
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor

import host_reservation_v1 as reservations
import host_slots_v1 as slots
import native_expanded_dispatch_v1 as native
from g115_d4_audit_storage_v1 import physical_tree
from public_training_dispatch_v2 import checked, pin, read, write, DISK_RESERVE_BYTES

PLAN = "archive-distillation-batch/v1"
RECEIPT = "archive-distillation-qualification/v1"
RESULT = "archive-distillation-batch-result/v1"
PHASE1 = "mtg-kernel-archive-distillation-phase1/v1"
PREFIX = 32
EVIDENCE = "local-throughput-evidence-only"
WINDOWS = os.name == "nt"


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def absolute(path):
    path = Path(path)
    require(path.is_absolute(), "absolute paths required")
    require(all(not p.is_symlink() and not p.is_junction() for p in (path, *path.parents)),
            "linked paths refused")
    return path.resolve()


def inputs(value, seen=None):
    """Verify supplied required pins, treating their contents as identity bytes.

    Historical provenance inside a checkpoint, bank or trajectory is not an
    additional file dependency of phase1's loader.
    """
    seen = {} if seen is None else seen
    if isinstance(value, list):
        for item in value:
            inputs(item, seen)
    elif isinstance(value, dict):
        if set(value) >= {"path", "sha256"}:
            path = absolute(value["path"])
            require(re.fullmatch(r"[0-9a-f]{64}", value["sha256"]) is not None, "invalid input hash")
            key = str(path)
            require(key not in seen or seen[key] == value["sha256"], "conflicting input pins")
            if key not in seen:
                seen[key] = value["sha256"]
                checked(value)
        else:
            for item in value.values():
                inputs(item, seen)
    return seen


def required_inputs(plan, runtime, configs):
    required = inputs([plan["runtime"], runtime, [j["command"] for j in plan["jobs"]]])
    for command in configs:
        inputs([command["parent"]["play_import"], command["parent"]["checkpoint"],
                command["games"], command["bank"], command["anchor"]], required)
        # Transfer descriptors have actual loader dependencies. Ordinary import
        # files and checkpoints do not require their historical trajectories.
        source = native.read_pinned_json(command["parent"]["play_import"])
        if source.get("schema") in ("mtg-kernel-expanded-registry-transfer-source/v1",
                                    "mtg-kernel-expanded-fresh-registry-transfer-source/v1"):
            inputs([source[k] for k in ("source_checkpoint", "source_registry", "transfer_envelope",
                                       "continuation_schedule")], required)
    return required


def validate_plan(plan, qualifying=False):
    require(plan["schema"] == PLAN and WINDOWS, "Windows archive batch plan required")
    cores = plan["cpu_affinity"]
    require(cores and len(set(cores)) == len(cores)
            and all(type(c) is int and 0 <= c < min(64, os.cpu_count() or 1) for c in cores),
            "invalid CPU affinity")
    require(len(cores) >= 2, "two-process qualification requires at least two cores")
    require([j["id"] for j in plan["jobs"]] == ["r1", "r2"], "ordered r1/r2 jobs required")
    for job in plan["jobs"]:
        require(type(job["steps"]) is int and job["steps"] >= PREFIX, "invalid formal step count")
    limits_to_check = [plan["qualification_limits"]]
    if not qualifying:
        limits_to_check += [plan["run_limits"], plan["publication_budget"]]
    for limits in limits_to_check:
        for name in ("wall_seconds", "cpu_seconds"):
            native.positive(limits[name], name)
    storage = plan["storage"]
    require(type(storage["max_physical_bytes"]) is int and storage["max_physical_bytes"] > 0,
            "positive physical storage cap required")
    require(type(storage["projected_additional_bytes"]) is int
            and storage["projected_additional_bytes"] > 0, "positive write projection required")
    if not qualifying:
        require(type(plan["publication_budget"]["bytes"]) is int and plan["publication_budget"]["bytes"] > 0,
                "nonzero final checkpoint/publication byte budget required")
    require(storage["reserve_bytes"] >= DISK_RESERVE_BYTES, "preserve 60 GiB free")
    roots = [absolute(p) for p in storage["accounting_roots"]]
    require(roots and all(not a.is_relative_to(b) for i, a in enumerate(roots)
                         for k, b in enumerate(roots) if i != k), "disjoint accounting roots required")
    require(len({p.drive.upper() for p in roots}) == 1, "one local output volume per batch")
    require(plan["stop_files"], "declare the applicable STOP files")
    for path in plan["stop_files"]:
        absolute(path)


def resource_context(cores):
    requested = set(cores)
    claim_id = os.environ.get(slots.CLAIM_ENV)
    if claim_id:
        claim = slots.read_json(slots.slots_dir() / f"claim-{claim_id}.json")
        require(claim and claim["state"] == "running"
                and slots.process_state(claim["pid"], claim["creation_time"]) == "alive"
                and requested <= set(claim["cores"])
                and requested <= set(slots.parse_cores(os.environ.get(slots.CORES_ENV, ""))),
                "live matching host_slots claim required")
        require(not slots.claim_conflicts(cores, claim.get("background", False)), "current reservation conflicts")
        return {"claim": claim_id, "cores": sorted(requested)}
    token = os.environ.get(slots.TOKEN_ENV)
    timed = slots.timed_state()
    require(token and reservations.status(token).get("token_fate") == "holds"
            and timed.get("token_sha256") == slots.token_hash(token)
            and timed.get("cores") is not None and requested <= set(timed["cores"]),
            "held reservation with explicit timed cores required")
    return {"reservation_sha256": slots.token_hash(token), "cores": sorted(requested)}


def inventory(plan, qualifying=False):
    require(set(plan["inventory"]) == {"desktop", "computehost", "runpod"}, "inspect all three hosts")
    for host, status in plan["inventory"].items():
        checked(status["evidence"])
        age = (datetime.now(timezone.utc) - datetime.fromisoformat(status["checked_at"])).total_seconds()
        require(0 <= age <= 86400 and type(status["eligible"]) is bool and status["reason"],
                "stale or incomplete inventory: " + host)
    decision = plan["allocation"]
    mode = "evidence_acquisition" if qualifying else "formal"
    require(decision.get("mode") == mode, "allocation mode must be " + mode)
    require(decision["host_name"].upper() == slots.host()
            and decision["selected_host"] in ("desktop", "computehost")
            and native.HOSTS[decision["selected_host"]] == slots.host(), "local unpaid allocation required")
    require(plan["inventory"][decision["selected_host"]]["eligible"] and decision["reason"],
            "selected host is ineligible or lacks an allocation reason")
    for host, status in plan["inventory"].items():
        if status["eligible"]:
            comparison = decision["comparisons"][host]
            checked(comparison["evidence"])
            require(type(comparison["absolute_inputs_available"]) is bool and comparison["reason"],
                    "record exact absolute input availability and placement tradeoff")
            if not qualifying:
                native.positive(comparison["projected_seconds"], "measured placement projection")
                require(type(comparison["transfer_seconds"]) in (int, float)
                        and 0 <= comparison["transfer_seconds"] < float("inf"), "record transfer/startup cost")
    feasible = {host: row for host, row in decision["comparisons"].items()
                if plan["inventory"][host]["eligible"] and row["absolute_inputs_available"]}
    require(decision["selected_host"] in feasible, "selected placement lacks exact absolute inputs")
    if not qualifying:
        chosen = feasible[decision["selected_host"]]
        require(chosen["projected_seconds"] + chosen["transfer_seconds"]
                <= min(row["projected_seconds"] + row["transfer_seconds"] for row in feasible.values()),
                "selected placement is slower than a feasible comparative allocation")


def workload(plan):
    runtime = native.runtime_for(plan)
    native.host_runtime(runtime)
    configs = []
    for job in plan["jobs"]:
        command = native.read_pinned_json(job["command"])
        require(command["mode"] == "archive_distillation_phase1" and command["lineage"] == job["id"]
                and not command.get("dry_run") and command.get("stop_after_steps") is None,
                "unchanged formal phase-1 commands required")
        checkpoints = (command.get("options") or {}).get("checkpoint_steps")
        require(checkpoints is None or isinstance(checkpoints, list) and checkpoints
                and checkpoints[-1] == job["steps"], "declared dose differs from explicit checkpoint schedule")
        command = copy.deepcopy(command)
        command.pop("output_directory")
        configs.append(command)
    pins = required_inputs(plan, runtime, configs)
    # Future formal limits/publication budgets do not affect the prefix and
    # can be filled after measuring it. Formal admission validates them again.
    binding = {"runtime": native.runtime_identity(runtime), "inputs": pins, "commands": configs,
               "steps": [j["steps"] for j in plan["jobs"]], "cpu_affinity": plan["cpu_affinity"],
               "host": slots.host(), "launcher_sha256": pin(__file__)["sha256"],
               "storage": plan["storage"], "qualification_limits": plan["qualification_limits"],
               "stop_files": plan["stop_files"]}
    return binding, runtime, configs


class Accounting(ctypes.Structure):
    _fields_ = [(n, ctypes.c_longlong) for n in ("user", "kernel", "period_user", "period_kernel")] + [
        (n, ctypes.c_ulong) for n in ("faults", "total", "active", "terminated")]


class Guard:
    """One contained job accumulates CPU for all descendants, including exited ones.

    New callers may opt into host_slots reservation pauses. The default retains
    immediate conflict failure, including for existing archive dispatch plans.
    Pauses do not reset or subtract from the wall clock or any other limit.
    """
    def __init__(self, plan, root, qualifying, *, pause_on_reservation=False):
        require(type(pause_on_reservation) is bool, "boolean reservation pause mode required")
        self.pause_on_reservation = pause_on_reservation
        self.transition_lock = threading.RLock()
        self.claim_identity = None
        self.paused = False
        self.pause_transitions = []
        self.plan, self.root = plan, absolute(root)
        self.limits = plan["qualification_limits" if qualifying else "run_limits"]
        self.started, self.cpu_started = time.monotonic(), time.process_time()
        self.error = None
        self.done = threading.Event()
        self.tree = slots.Tree(plan["cpu_affinity"], "below_normal", contain_self=False)
        try:
            self.check(admission=True)
            require(not self.root.exists() and self.root.parent.is_dir(), "fresh root with existing parent required")
            self.root.mkdir()
        except BaseException:
            slots._CloseHandle(self.tree.job)
            raise
        self.thread = threading.Thread(target=self.watch, daemon=True)
        self.thread.start()

    def sample(self):
        value = Accounting()
        require(slots._QueryInformationJobObject(self.tree.job, 1, ctypes.byref(value), ctypes.sizeof(value), None),
                "cannot account contained process-tree CPU")
        return {"cpu_seconds": (value.user + value.kernel) / 1e7 + time.process_time() - self.cpu_started,
                "seconds": time.monotonic() - self.started, "active_processes": value.active}

    def check(self, admission=False):
        if self.error:
            raise RuntimeError(self.error)
        if getattr(self, "pause_on_reservation", False):
            with self.transition_lock:
                with slots.Mutex():
                    self._pause_resources()
                sample = self._check_limits(admission)
                # Never resume from an old no-conflict snapshot, or before
                # checking STOP, CPU, the unchanged wall clock and storage.
                with slots.Mutex():
                    self._pause_resources(allow_resume=True)
                return sample
        resource_context(self.plan["cpu_affinity"])
        return self._check_limits(admission)

    def _check_limits(self, admission=False):
        require(not any(Path(p).exists() for p in [*self.plan["stop_files"], self.root / "STOP"]), "STOP requested")
        sample = self.sample()
        require(sample["seconds"] < self.limits["wall_seconds"], "batch wall limit reached")
        require(sample["cpu_seconds"] < self.limits["cpu_seconds"], "aggregate process CPU limit reached")
        storage = self.plan["storage"]
        roots = [absolute(p) for p in storage["accounting_roots"]]
        require(any(self.root.is_relative_to(p) for p in roots), "output outside accounting roots")
        charged = sum(physical_tree(p, allow_atomic_rename=True, use_logical_bound=True)["charged_bytes"]
                      for p in roots if p.exists())
        pending = storage["projected_additional_bytes"] if admission else 0
        require(charged + pending <= storage["max_physical_bytes"], "physical output cap reached")
        anchor = roots[0]
        while not anchor.exists():
            anchor = anchor.parent
        require(shutil.disk_usage(anchor).free >= storage["reserve_bytes"] + pending, "60 GiB reserve unavailable")
        return {**sample, "charged_physical_bound_bytes": charged}

    def _pause_resources(self, allow_resume=False):
        """Slot mutex and transition lock held; balance only this Tree's pauses.

        The outer supported runner may also suspend this supervisor and its
        descendants. Its suspension counts remain independent of this Tree's.
        Opt-in callers must use an ordinary live BelowNormal host_slots claim.
        """
        claim_id = os.environ.get(slots.CLAIM_ENV)
        claim = slots.read_json(slots.slots_dir() / f"claim-{claim_id}.json") if claim_id else None
        require(claim and claim.get("id") == claim_id
                and claim.get("state") in ("running", "suspended")
                and claim.get("priority") == "below_normal"
                and slots.process_state(claim.get("pid"), claim.get("creation_time")) == "alive",
                "live matching BelowNormal host_slots claim required")
        identity = {key: claim[key] for key in ("id", "pid", "creation_time", "cores", "priority", "background")}
        require(self.claim_identity is None or identity == self.claim_identity, "guard claim identity changed")
        self.claim_identity = identity
        requested = set(self.plan["cpu_affinity"])
        require(requested <= set(claim["cores"])
                and requested <= set(slots.parse_cores(os.environ.get(slots.CORES_ENV, ""))),
                "guard cores differ from host_slots claim")
        conflict = slots.claim_conflicts(self.plan["cpu_affinity"], claim["background"])
        paused = conflict or claim["state"] != "running"
        if paused or allow_resume:
            require(self.tree.set_suspended(paused), "guard tree suspension transition incomplete")
            if paused != self.paused:
                self.pause_transitions.append(dict(seconds=time.monotonic() - self.started, paused=paused,
                    reason="reservation conflict" if conflict else "outer claim suspended" if paused else "claim running without conflict"))
            self.paused = paused
        return paused

    def watch(self):
        while not self.done.wait(.5):
            try:
                self.check()
            except BaseException as error:
                self.error = str(error)
                self.tree.kill()
                return

    def spawn(self, command, stdout, stderr):
        if self.pause_on_reservation:
            return self._spawn_pausing(command, stdout, stderr)
        self.check()
        # Assign while suspended so no native work or descendant can escape containment.
        child = subprocess.Popen(command, stdout=stdout, stderr=stderr,
            creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS | 4,
            env=dict(os.environ, CUDA_VISIBLE_DEVICES="", VSCMD_SKIP_SENDTELEMETRY="1"))
        try:
            self.tree.adopt(child.pid, slots.creation_time(child.pid))
            require(slots._NtResumeProcess(int(child._handle)) >= 0, "cannot resume contained native child")
        except BaseException:
            child.kill(); child.wait()
            raise
        return child

    def _spawn_pausing(self, command, stdout, stderr):
        while True:
            self.check()
            with self.transition_lock, slots.Mutex():
                if not self._pause_resources():
                    child = subprocess.Popen(command, stdout=stdout, stderr=stderr,
                        creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS | 4,
                        env=dict(os.environ, CUDA_VISIBLE_DEVICES="", VSCMD_SKIP_SENDTELEMETRY="1"))
                    try:
                        self.tree.adopt(child.pid, slots.creation_time(child.pid))
                        self._pause_resources(allow_resume=True)
                        # Remove CREATE_SUSPENDED's initial count only. A new
                        # conflict has already added its own tracked pause.
                        require(slots._NtResumeProcess(int(child._handle)) >= 0, "cannot resume contained native child")
                    except BaseException:
                        child.kill(); child.wait()
                        raise
                    return child
            time.sleep(.05)

    def close(self):
        self.done.set()
        self.thread.join()
        try:
            self.terminate_and_drain()
        finally:
            slots._CloseHandle(self.tree.job)

    def quiet(self):
        # Windows retires job exit notifications asynchronously. A surviving
        # native descendant must not be accepted as a completed phase1 job.
        until = time.monotonic() + 1
        while self.sample()["active_processes"] and time.monotonic() < until:
            self.check()
            time.sleep(.01)
        require(self.sample()["active_processes"] == 0, "native descendants remain after root exit")
        self.check()

    def terminate_and_drain(self):
        self.tree.kill()
        until = time.monotonic() + 5
        while self.sample()["active_processes"] and time.monotonic() < until:
            time.sleep(.01)
        require(self.sample()["active_processes"] == 0, "contained tree did not terminate")
        return self.sample()  # Includes kernel/user CPU of the now-exited descendants.


def native_command(runtime, command_path):
    return [str(checked(runtime["binary"])), str(command_path)]


def output_fingerprint(path, command, runtime, steps, prefix):
    result = read(path)
    require(result["schema"] == PHASE1 and result.get("complete") is True and not result.get("dry_run")
            and result["steps"] == steps and result.get("stop_after_steps") == (PREFIX if prefix else None),
            "incomplete or wrong-dose phase-1 output")
    identity = result["identity"]
    require(all(identity[k] == command[k] for k in ("lineage", "arm", "parent", "games", "bank", "anchor")),
            "native input identity differs")
    require(identity["runtime_commit"] == runtime["engine_commit"]
            and identity["runtime_tree"] == runtime["tracked_tree_sha256"]
            and identity["runtime_binary_sha256"] == runtime["binary"]["sha256"], "native runtime differs")
    require(identity["recipe"].get("options") == command.get("options")
            and identity["recipe"].get("update_scope", "full_network") == command.get("update_scope", "full_network"),
            "native recipe differs")
    hashes = {entry["step"]: entry["phase1_state_sha256"] for entry in result["state_hashes"]}
    require(PREFIX in hashes and all(re.fullmatch(r"[0-9a-f]{64}", h) for h in hashes.values()),
            "missing exact step32 primary state hash")
    for entry in [result["telemetry"], *(p for c in result["checkpoints"] for p in (c["checkpoint"], c["source"]))]:
        require(absolute(entry["path"]).is_relative_to(path.parent.resolve()), "output pin escapes native root")
        checked(entry)
    telemetry = read(checked(result["telemetry"]))
    require(len(telemetry) == steps and [r["step"] for r in telemetry] == list(range(1, steps + 1)),
            "telemetry step sequence differs")
    if not prefix:
        require(result["checkpoints"] and result["checkpoints"][-1]["step"] == steps
                and result["checkpoints"][-1]["phase1_state_sha256"] == hashes[steps], "missing final checkpoint")
    return {"step32_state_sha256": hashes[PREFIX], "final_state_sha256": hashes[steps],
            "prefix_telemetry_sha256": native.digest(telemetry[:PREFIX])}


def run_batch(plan, runtime, configs, root, workers, mode, guard):
    started = time.monotonic()
    cpu_before = guard.sample()["cpu_seconds"]
    required_inputs(plan, runtime, configs)
    root.mkdir()
    def one(index):
        job = plan["jobs"][index]
        folder = root / job["id"]
        folder.mkdir()
        command = copy.deepcopy(configs[index])
        command["output_directory"] = str(folder / "native")
        if mode == "prefix":
            command["stop_after_steps"] = PREFIX
        elif mode == "dry":
            command["dry_run"] = True
        command_path = folder / "command.json"
        write(command_path, command)
        before = time.monotonic()
        with (folder / "stdout.jsonl").open("x") as out, (folder / "stderr.log").open("x") as err:
            child = guard.spawn(native_command(runtime, command_path), out, err)
            code = child.wait()
        guard.check()
        require(code == 0, "native job failed; preserve " + str(folder))
        result_path = folder / "native" / "phase1.json"
        if mode == "dry":
            require(read(result_path).get("dry_run") is True, "startup probe is not a dry run")
            fingerprint = None
        else:
            fingerprint = output_fingerprint(result_path, command, runtime,
                PREFIX if mode == "prefix" else job["steps"], mode == "prefix")
        return {"id": job["id"], "seconds": time.monotonic() - before,
                "output_bytes": physical_tree(folder)["charged_bytes"],
                "command": pin(command_path), "result": pin(result_path), "fingerprint": fingerprint}
    with ThreadPoolExecutor(max_workers=workers) as pool:
        def guarded(index):
            try:
                return one(index)
            except BaseException as error:
                guard.error = str(error)
                guard.tree.kill()
                raise
        jobs = list(pool.map(guarded, range(2)))
    guard.quiet()
    report = {"workers": workers, "mode": mode, "jobs": jobs, "seconds": time.monotonic() - started,
              "cpu_seconds": guard.sample()["cpu_seconds"] - cpu_before, "guard": guard.check()}
    write(root / "batch.json", report)
    return {**report, "record": pin(root / "batch.json")}


def verify_qualification(ref, binding, runtime, configs):
    receipt = read(checked(ref)) if "path" in ref else ref
    require(receipt["schema"] == RECEIPT and receipt.get("complete") is True
            and receipt.get("purpose") == EVIDENCE
            and receipt["binding"] == binding, "missing or incompatible throughput qualification")
    if "path" in ref:
        completion = read(Path(ref["path"]).parent / "result.json")
        require(completion["complete"] is True and completion["receipt"] == ref
                and completion.get("qualification") is True and completion.get("purpose") == EVIDENCE
                and completion["binding"] == binding and completion["guard"]["active_processes"] == 0,
                "qualification lacks successful drained completion")
    trials = receipt["trials"]
    require([t["prefix"]["workers"] for t in trials] == [1, 2], "serial and two-worker trials required")
    for trial in trials:
        native.positive(trial["prefix"]["seconds"], "measured complete-prefix seconds")
        native.positive(trial["prefix"]["cpu_seconds"], "measured complete-prefix CPU")
        require(trial["startup"]["workers"] == trial["prefix"]["workers"], "startup placement differs")
        for batch in (trial["startup"], trial["prefix"]):
            require(read(checked(batch["record"])) == {k: v for k, v in batch.items() if k != "record"},
                    "measured batch record changed")
        for index, job in enumerate(trial["startup"]["jobs"]):
            command = read(checked(job["command"]))
            command.pop("output_directory")
            require(command == {**configs[index], "dry_run": True}
                    and read(checked(job["result"])).get("dry_run") is True, "startup probe changed")
        for index, job in enumerate(trial["prefix"]["jobs"]):
            actual = output_fingerprint(checked(job["result"]), read(checked(job["command"])), runtime, PREFIX, True)
            command = read(checked(job["command"]))
            command.pop("output_directory")
            require(command == {**configs[index], "stop_after_steps": PREFIX} and job["id"] == ("r1", "r2")[index]
                    and actual == job["fingerprint"], "qualification output/config changed")
            checkpoints = read(checked(job["result"]))["identity"]["recipe"]["checkpoints"]
            require(checkpoints and checkpoints[-1] == binding["steps"][index],
                    "declared formal dose differs from qualified native recipe")
        require(len(trial["prefix"]["jobs"]) == 2, "qualification omits a lineage")
    require([j["fingerprint"] for j in trials[0]["prefix"]["jobs"]]
            == [j["fingerprint"] for j in trials[1]["prefix"]["jobs"]], "serial/parallel primary output parity failed")
    return min(trials, key=lambda t: t["prefix"]["seconds"])


def execute(plan_path, root, qualifying, qualification=None):
    plan = read(plan_path)
    validate_plan(plan, qualifying)
    guard = Guard(plan, root, qualifying)
    outcome = {"schema": RESULT, "complete": False, "plan": pin(plan_path), "qualification": qualifying,
               "purpose": EVIDENCE if qualifying else "formal-dispatch"}
    failure = None
    try:
        inventory(plan, qualifying)
        binding, runtime, configs = workload(plan)
        outcome["binding"] = binding
        if qualifying:
            trials = []
            for workers in (1, 2):
                dry = run_batch(plan, runtime, configs, guard.root / f"startup-w{workers}", workers, "dry", guard)
                prefix = run_batch(plan, runtime, configs, guard.root / f"prefix-w{workers}", workers, "prefix", guard)
                trials.append({"startup": dry, "prefix": prefix})
            receipt = {"schema": RECEIPT, "complete": True, "binding": binding, "trials": trials,
                       "purpose": EVIDENCE,
                       "allocation": plan["allocation"], "inventory": plan["inventory"]}
            selected = verify_qualification(receipt, binding, runtime, configs)
            write(guard.root / "qualification.json", receipt)
            outcome["receipt"] = pin(guard.root / "qualification.json")
        else:
            require(qualification is not None, "throughput qualification required before native spawn")
            selected = verify_qualification(qualification, binding, runtime, configs)
        workers = selected["prefix"]["workers"]
        outcome["selected_workers"] = workers
        if not qualifying:
            startup, prefix = selected["startup"], selected["prefix"]
            marginal = [max(0, p["seconds"] - d["seconds"]) / PREFIX
                        for p, d in zip(prefix["jobs"], startup["jobs"])]
            projected = [d["seconds"] + marginal[i] * plan["jobs"][i]["steps"]
                         for i, d in enumerate(startup["jobs"])]
            overhead = max(0, prefix["seconds"] - (sum(j["seconds"] for j in prefix["jobs"])
                                                   if workers == 1 else max(j["seconds"] for j in prefix["jobs"])))
            wall = overhead + (sum(projected) if workers == 1 else max(projected)) + plan["publication_budget"]["wall_seconds"]
            factor = max(j["steps"] for j in plan["jobs"]) / PREFIX
            cpu = startup["cpu_seconds"] + max(0, prefix["cpu_seconds"] - startup["cpu_seconds"]) * factor
            cpu += plan["publication_budget"]["cpu_seconds"]
            projected_bytes = sum(d["output_bytes"] + max(0, p["output_bytes"] - d["output_bytes"])
                                  * plan["jobs"][i]["steps"] / PREFIX
                                  for i, (p, d) in enumerate(zip(prefix["jobs"], startup["jobs"])))
            projected_bytes += plan["publication_budget"]["bytes"]
            outcome.update(projection={"wall_seconds": wall, "cpu_seconds": cpu,
                "physical_bytes": projected_bytes,
                "startup_seconds": [j["seconds"] for j in selected["startup"]["jobs"]],
                "incremental_seconds_per_step": marginal,
                "repeated_startup_cpu_upper_estimate": prefix["cpu_seconds"] * factor + plan["publication_budget"]["cpu_seconds"],
                "limitation": "32-step extrapolation; startup subtraction is noisy; final publication is budgeted, not measured",
                "publication_budget": plan["publication_budget"]})
            outcome["batch"] = run_batch(plan, runtime, configs, guard.root / "formal", workers, "formal", guard)
            require([{k: j["fingerprint"][k] for k in ("step32_state_sha256", "prefix_telemetry_sha256")}
                     for j in outcome["batch"]["jobs"]]
                    == [{k: j["fingerprint"][k] for k in ("step32_state_sha256", "prefix_telemetry_sha256")}
                        for j in selected["prefix"]["jobs"]], "formal first32 state/telemetry differs from qualification")
        outcome["complete"] = True
    except BaseException as error:
        outcome["error"] = str(error)
        failure = error
    finally:
        try:
            if failure is None:
                try:
                    guard.quiet()
                except BaseException as error:
                    failure = error
            if failure is not None:
                outcome.update(complete=False, error=str(failure))
                guard.terminate_and_drain()
            outcome["guard"] = guard.sample()
            write(guard.root / "result.json", outcome)
            if failure is None:
                try:
                    guard.check()  # Final publication remains inside CPU/wall/storage/STOP checks.
                except BaseException as error:
                    failure = error
                    outcome.update(complete=False, error=str(error), guard=guard.terminate_and_drain())
                    (guard.root / "result.json").rename(guard.root / "result-before-publication-failure.json")
                    write(guard.root / "result.json", outcome)
        finally:
            guard.close()
    if failure is not None:
        raise failure
    return outcome


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("qualify", "run"))
    parser.add_argument("plan", type=Path)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--qualification", type=Path)
    args = parser.parse_args(argv)
    result = execute(args.plan, args.root, args.action == "qualify",
                     pin(args.qualification) if args.qualification else None)
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
