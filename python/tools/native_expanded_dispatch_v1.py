"""Reservation-backed CPU admission for native-expanded training and collection.

Qualification runs one complete initial update, or at most 20 evaluation games.
Production requires compatible measured serial/parallel receipts and the fastest
eligible allocation. The runtime binding is independent of the launcher's commit.
Runs on the two Windows hosts and, as the ``runpod`` placement, on a rented
Linux pod with live lease-guard evidence (docs/native_expanded_dispatch_v1.md).
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import hashlib
from functools import lru_cache
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

from public_training_dispatch_v2 import checked, pin, read, write, DISK_RESERVE_BYTES

SCHEMA = "native-expanded-cpu-dispatch/v1"
CHOICE = "native-expanded-cpu-allocation/v1"
# The compute host's machine name is site configuration, not a public constant.
HOSTS = {"desktop": "DESKTOP-DJ1C40R", "computehost": os.environ.get("COMPUTE_HOST_NAME", "COMPUTEHOST").upper()}
PLACEMENTS = ("desktop", "computehost", "runpod")
CAP = 192 * 1024**3
WINDOWS = os.name == "nt"
# A Linux runtime also pins the system libraries its output bits depend on
# (f32::tanh goes through libm): phase1_cloud/runtime_observation.LIBRARIES.
PLATFORM = "linux-x86_64"
LIBRARIES = ("ld-linux-x86-64.so.2", "libc.so.6", "libm.so.6", "libgcc_s.so.1")
# runpod: the pod's resident guard state (phase1_cloud/lease_guard.py --state),
# holding its guard.json and a copy of its lease as lease.json.
LEASE_GUARD_ENV = "MTG_LEASE_GUARD_DIR"
PROC = Path("/proc")


@lru_cache(maxsize=16)
def _inference_shape(path, sha256, size, modified_ns):
    checkpoint = read(checked({"path": path, "sha256": sha256}))
    return {key: checkpoint[key] for key in
            ("schema", "source_import", "feature_contract_digest", "feature_encoding_digest", "card_db_hash")} | {
                "parameters": [(row["name"], row["shape"], len(row["values"])) for row in checkpoint["parameters"]]}


def inference_shape(source):
    item = source["checkpoint"]
    stat = Path(item["path"]).stat()
    return _inference_shape(item["path"], item["sha256"], stat.st_size, stat.st_mtime_ns)


def runtime_identity(runtime):
    identity = {key: runtime[key] for key in ("engine_commit", "tracked_tree_sha256")} | {
        "binary_sha256": runtime["binary"]["sha256"]}
    if "platform" in runtime:  # Linux only, so never equal to a Windows identity
        identity |= {"platform": runtime["platform"],
                     "library_sha256": {name: item["sha256"] for name, item in runtime["libraries"].items()}}
    return identity


def artifact_reader(mappings):
    """Resolve verified recovery copies without rewriting captured native pins."""
    pairs = [(Path(m["source_root"]).resolve(), Path(m["local_root"]).resolve()) for m in mappings]
    require(len({p[0] for p in pairs}) == len(pairs), "duplicate recovery mapping")
    def resolve(item):
        source = Path(item["path"]).resolve()
        matches = [(a, b) for a, b in pairs if source.is_relative_to(a)]
        require(len(matches) <= 1, "ambiguous recovery mapping")
        target = matches[0][1] / source.relative_to(matches[0][0]) if matches else source
        checked({"path": str(target), "sha256": item["sha256"]})
        return target
    return resolve


def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"),
                                     allow_nan=False).encode()).hexdigest()


def positive(value, name):
    require(type(value) in (int, float) and math.isfinite(value) and value > 0,
            "invalid " + name)
    return value


SCHEDULE_FAMILIES = (None, "permuted-units-v1")


def workload(config, kind, keep_seeds=False, family=None):
    """Bind complete scientific schedules while permitting paired seed replicas.

    IDs/seeds and output placement do not alter batch shape. Training keeps
    source bits and optimizer settings. Evaluation permits new admitted weights
    with the same import, feature contract and parameter layout, retaining the
    complete deck/opponent/seat schedule. Actual endpoint pins stay in each job.

    A training request may opt into ``family="permuted-units-v1"`` for chained
    blocks of one balanced schedule. Every block must then hold the identical
    multiset of scheduled episodes (decks, seats, starting player, opponent
    assignment, limits) and the same number of episodes in each update, in a
    block-specific order, and start from admitted weights with the same import,
    feature contract and parameter layout. Everything else stays bound.
    """
    require(family in SCHEDULE_FAMILIES, "unknown schedule family")
    require(family is None or kind == "training", "schedule families apply to training only")
    value = copy.deepcopy(config)
    value.pop("output_directory")
    if kind == "training":
        value.pop("collection_workers", None)
        value.pop("preparation_workers", None)
        for key, default in {"update_backend": {"kind": "cpu"}, "update_backward_execution": "sequential",
                             "max_non_natural_episode_fraction": 0.0,
                             "max_prepared_tensor_mebibytes": 256}.items():
            value.setdefault(key, default)
        episodes = [item["episode"] for update in value["iterations"]
                    for item in update["episodes"]]
    else:
        value.pop("workers", None)
        value["mode"] = "collect"
        if not keep_seeds:
            value["source"]["checkpoint"] = inference_shape(value["source"])
        episodes = value["episodes"]
    if not keep_seeds:
        for episode in episodes:
            episode.pop("id")
            episode.pop("seed")
        if family == "permuted-units-v1":
            value["initial_source"]["checkpoint"] = inference_shape(value["initial_source"])
            shapes = sorted(json.dumps(item, sort_keys=True, separators=(",", ":"))
                            for update in value["iterations"] for item in update["episodes"])
            value["iterations"] = {"sizes": [len(update["episodes"]) for update in value["iterations"]],
                                   "episode_multiset_sha256": digest(shapes)}
    return digest(value)


def validate_storage(storage, extra=None):
    cap = storage["max_logical_bytes"]
    require(type(cap) is int and 0 < cap <= CAP, "storage cap exceeds 192 GiB")
    require(storage["reserve_bytes"] >= DISK_RESERVE_BYTES, "reserve below 60 GiB")
    raw_roots = [Path(path) for path in storage["accounting_roots"]]
    require(all(path.is_absolute() for path in raw_roots), "absolute accounting roots required")
    for raw in raw_roots:
        require(all(not path.is_symlink() and not path.is_junction() for path in (raw, *raw.parents)),
                "accounting root traverses a link")
    roots = [path.resolve() for path in raw_roots]
    require(roots and len(set(roots)) == len(roots), "missing or duplicate accounting roots")
    require(all(not a.is_relative_to(b) for a in roots for b in roots if a != b),
            "overlapping accounting roots")
    logical = 0
    for root in roots:
        if root.exists():
            require(not root.is_symlink() and not root.is_junction(), "accounting root is a link")
            for path in root.rglob("*"):
                require(not path.is_symlink() and not path.is_junction(), "accounting tree contains a link")
                if path.is_file():
                    logical += path.stat().st_size
        anchor = root
        while not anchor.exists():
            anchor = anchor.parent
        volume = anchor.drive.upper()
        prospective = storage["projected_volume_bytes"].get(volume, 0) if extra is None else 0
        require(type(prospective) is int and prospective >= 0, "invalid per-volume projection")
        require(volume in storage["projected_volume_bytes"], "missing per-volume storage projection")
        require(shutil.disk_usage(anchor).free >= storage["reserve_bytes"] + prospective,
                "target-volume reserve unavailable")
    projected = storage["projected_additional_bytes"] if extra is None else extra
    require(type(projected) is int and projected >= 0, "invalid storage projection")
    if extra is None:
        require(sum(storage["projected_volume_bytes"].values()) >= projected,
                "volume projection omits planned writes")
    require(logical + projected <= cap, "retained plus projected storage exceeds allowance")
    return {"logical_bytes": logical, "cap_bytes": cap}


def runtime_for(request, resolve=checked):
    runtime = read(resolve(request["runtime"]))
    require(len(runtime["engine_commit"]) == 40 and len(runtime["tracked_tree_sha256"]) == 64,
            "missing compiled runtime identity")
    require("platform" not in runtime or runtime["platform"] == PLATFORM
            and sorted(runtime["libraries"]) == sorted(LIBRARIES)
            and all(item["path"].startswith("/") and len(item["sha256"]) == 64
                    for item in runtime["libraries"].values()), "Linux runtime needs loader/libc/libm/libgcc_s pins")
    resolve(runtime["binary"])
    return runtime


def host_runtime(runtime):
    """A Linux runtime runs only on Linux with its pinned libraries; a Windows one only on Windows."""
    require(runtime.get("platform") == (None if WINDOWS else PLATFORM), "runtime platform differs from this host")
    if not WINDOWS:
        require(sys.platform.startswith("linux") and platform.machine() == "x86_64",
                "runtime platform differs from this host")
        for item in runtime["libraries"].values():
            checked(item)
        # An interposed or redirected library would change native output bits unseen.
        require(not any(value and (name.startswith("LD_") or name == "GLIBC_TUNABLES")
                        for name, value in os.environ.items()), "dynamic loader environment override")


def lease_guard():
    """phase1_cloud/lease_guard.py (its modules import their siblings by bare name)."""
    folder = str(Path(__file__).resolve().parent / "phase1_cloud")
    if folder not in sys.path:
        sys.path.append(folder)
    import lease_guard
    return lease_guard


def runpod_lease(work_seconds, now=None):
    """Lease evidence for a runpod placement: this Linux pod (RUNPOD_POD_ID), whose
    resident guard in $MTG_LEASE_GUARD_DIR is fresh, unlatched, provider- and
    funds-verified, has no stop request, and leaves work_seconds before the
    lease deadline less recovery_seconds (when the guard latches 'deadline')."""
    now = time.time() if now is None else now
    pod, folder = os.environ.get("RUNPOD_POD_ID"), os.environ.get(LEASE_GUARD_ENV)
    require(not WINDOWS and sys.platform.startswith("linux") and pod and folder,
            "runpod placement needs this pod's lease guard")
    folder = Path(folder)
    lease, guard = read(folder/"lease.json"), read(folder/"guard.json")
    lease_guard().validate(lease)
    require(guard["pod_id"] == pod and guard["name"] == lease["name"], "lease guard belongs to another pod")
    # The guard rewrites guard.json every poll, after up to two 15 s provider requests.
    require(-5 <= now - guard["epoch"] <= 3 * lease["poll_seconds"] + 30, "lease guard is stale")
    require(not guard["latched"] and guard["release_epoch"] is None and guard["provider_ok"] is True
            and guard["allow_new_dispatch"] is True and not (folder/"stop-request.json").exists(),
            "lease guard has stopped new work")
    available = lease["deadline_epoch"] - lease["recovery_seconds"] - now
    require(available >= positive(work_seconds, "lease work seconds"), "lease time exhausted")
    return {"pod_id": pod, "lease": lease["name"], "available_seconds": available}


def require_host(request):
    """desktop/computehost only on that Windows machine; runpod only on its own leased pod."""
    host = request["placement"]["host"]
    require(host in PLACEMENTS, "no paid or unknown placement")
    if host == "runpod":
        wall = positive(request["wall_seconds"], "wall-time limit")
        runpod_lease(min(positive(request.get("lease_work_seconds", wall), "lease work seconds"), wall))
    else:
        require(WINDOWS and HOSTS[host] == platform.node().upper(), "wrong target host")


def validate_request(request, qualification):
    require(request["schema"] == SCHEMA, "unsupported request")
    require(request["kind"] in ("training", "evaluation"), "unsupported native workload")
    runtime = runtime_for(request)
    host_runtime(runtime)
    config = read(checked(request["config"]))
    placement = request["placement"]
    require_host(request)
    workers = placement["workers"]
    require(type(workers) is int and 1 <= workers <= 64, "invalid worker count")
    cpus = placement["cpu_affinity"]
    require(cpus and len(set(cpus)) == len(cpus)
            and all(type(c) is int and 0 <= c < (os.cpu_count() or 1) for c in cpus),
            "invalid eligible CPU set")
    require(workers <= len(cpus), "workers exceed eligible CPUs")
    positive(placement["memory_bytes"], "memory limit")
    positive(request["wall_seconds"], "wall-time limit")
    root, cold = Path(request["root"]), Path(request["cold_root"])
    require(root.is_absolute() and cold.is_absolute() and root.resolve() != cold.resolve(),
            "distinct absolute hot/recovery roots required")
    accounting = [Path(p).resolve() for p in request["storage"]["accounting_roots"]]
    outputs = (root, cold) if qualification else (root, cold, Path(config["output_directory"]))
    for path in outputs:
        require(any(path.resolve().is_relative_to(p) for p in accounting),
                "output is outside storage accounting")
    family = request.get("schedule_family")
    require(family in SCHEDULE_FAMILIES and (family is None or request["kind"] == "training"),
            "unknown or non-training schedule family")
    # Tolerance is opt-in: the request must restate the config's fraction, so
    # an old request can never admit a tolerant config by accident.
    fraction = config.get("max_non_natural_episode_fraction", 0)
    tolerance = request.get("non_natural_tolerance")
    require(fraction == 0 and tolerance is None
            or request["kind"] == "training" and type(fraction) in (int, float) and 0 < fraction < 1
            and tolerance == fraction, "non-natural game tolerance must be declared by a training request")
    if request["kind"] == "training":
        require(config.get("update_backend", {"kind": "cpu"}) == {"kind": "cpu"}, "CPU runtime required")
        prep = placement["preparation_workers"]
        require(type(prep) is int and 1 <= prep <= len(cpus), "invalid preparation workers")
        require(config["iterations"] and all(i["episodes"] for i in config["iterations"]),
                "empty training schedule")
        if qualification:
            require(len(config["iterations"][0]["episodes"]) <= 32,
                    "qualification exceeds one bounded complete batch")
    else:
        require(config["mode"] in ("collect", "collect_parallel"), "collection only")
        require(config["episodes"], "empty evaluation schedule")
        if qualification:
            indices = request["episode_indices"]
            require(2 <= len(indices) <= 20 and len(set(indices)) == len(indices)
                    and indices == sorted(indices)
                    and all(type(i) is int and 0 <= i < len(config["episodes"]) for i in indices),
                    "invalid bounded evaluation sample")
    validate_storage(request["storage"])
    return config, runtime


def placed_config(request, config, qualification):
    result = copy.deepcopy(config)
    p = request["placement"]
    if qualification:
        result["output_directory"] = str(Path(request["root"])/"native")
    if request["kind"] == "training":
        for field, count in (("collection_workers", p["workers"]),
                             ("preparation_workers", p["preparation_workers"])):
            # Native serde omits serial defaults in run.json, but the input
            # parser permits explicit defaults. Preserve the input spelling.
            if count == 1 and field not in result:
                continue
            result[field] = count
    else:
        result["mode"] = "collect" if p["workers"] == 1 else "collect_parallel"
        result.pop("workers", None)
        if p["workers"] > 1:
            result["workers"] = p["workers"]
        if qualification:
            result["episodes"] = [result["episodes"][i] for i in request["episode_indices"]]
    if not qualification:
        require(result == config, "formal configuration does not implement qualified placement")
    return result


def collection_fingerprint(path, expected, resolve=checked, ledgers=None):
    """Ordered trajectory hashes of a complete, all-natural collection.

    A tolerant training collection may also pin a non-natural ledger of
    discarded attempts (the kept retries are still natural trajectories).
    Its SHA256 is appended to ``ledgers`` so serial/parallel parity covers the
    discarded attempts too; without ``ledgers`` any ledger is refused.
    """
    collection = read(resolve(path))
    require(collection["complete"] and len(collection["trajectories"]) == expected,
            "incomplete ordered collection")
    if "non_natural_ledger" in collection:
        require(ledgers is not None, "non-natural collection attempt")
        ledger = read(resolve(collection["non_natural_ledger"]))
        require(ledger["schema"] == "mtg-kernel-non-natural-collection-ledger/v1"
                and all(0 <= entry["slot"] < expected for entry in ledger["entries"]),
                "invalid non-natural ledger")
        ledgers.append(collection["non_natural_ledger"]["sha256"])
    elif ledgers is not None:
        ledgers.append(None)
    hashes = []
    for item in collection["trajectories"]:
        trajectory = read(resolve(item))
        require(trajectory["terminal"]["terminal_classification"] == "natural",
                "non-natural terminal")
        hashes.append(item["sha256"])
    return hashes


def output_fingerprint(config, kind, result, runtime, qualification, resolve=checked, collection_ref=None, run_ref=None):
    if kind == "evaluation":
        return {"trajectories": collection_fingerprint(
            collection_ref or pin(Path(config["output_directory"])/"collection.json"), len(config["episodes"]), resolve)}
    root = Path(config["output_directory"])
    run = read(resolve(run_ref)) if run_ref else read(root/"run.json")
    require(run["git_commit"] == runtime["engine_commit"]
            and run["tracked_tree_sha256"] == runtime["tracked_tree_sha256"],
            "compiled native runtime differs from binding")
    count = 1 if qualification else len(config["iterations"])
    require(result["completed_iterations"] == count
            and result["planned_iterations"] == len(config["iterations"])
            and len(result["iterations"]) == count
            and result["complete"] == (count == len(config["iterations"])),
            "incomplete native update coverage")
    tolerant = config.get("max_non_natural_episode_fraction", 0) > 0
    fingerprints = []
    for index, item in enumerate(result["iterations"]):
        receipt = read(resolve(item))
        require(receipt["iteration"] == index, "unordered update receipts")
        ledgers = [] if tolerant else None
        trajectories = collection_fingerprint(receipt["collection"],
                                              len(config["iterations"][index]["episodes"]), resolve, ledgers)
        update = read(resolve(receipt["update"]))
        require(update["complete"], "incomplete optimizer update")
        checkpoint = read(resolve(update["checkpoint"]))
        require([t["sha256"] for t in checkpoint["trajectories"]] == trajectories,
                "optimizer used different ordered trajectories")
        # Read the complete parameter and Adam tensors. Only the output-root
        # metadata in trajectory pins differs between serial/parallel trials.
        checkpoint["trajectories"] = trajectories
        entry = {"trajectories": trajectories, "checkpoint_bits": digest(checkpoint)}
        if tolerant:
            entry["non_natural_ledger"] = ledgers[0]
        fingerprints.append(entry)
    return {"iterations": fingerprints}


def projected_seconds(report, config, kind):
    units = len(config["iterations"]) if kind == "training" else len(config["episodes"])
    measured = report["completed_updates"] if kind == "training" else report["completed_games"]
    return positive(report["seconds"], "completed-work seconds") * units / measured


def require_choice(path, request, verify_outputs=False):
    choice = read(checked(path))
    require(choice["schema"] == CHOICE, "unsupported throughput choice")
    config, runtime = validate_request(request, False)
    family = workload(config, request["kind"], family=request.get("schedule_family"))
    require(set(choice["inventory"]) == {"desktop", "computehost", "runpod"},
            "inspect all three placement options")
    eligible = set()
    for host, status in choice["inventory"].items():
        checked(status["evidence"])
        age = (datetime.now(timezone.utc) - datetime.fromisoformat(status["checked_at"])).total_seconds()
        require(0 <= age <= 86400 and type(status["eligible"]) is bool and status["reason"],
                "stale or incomplete placement inventory")
        if status["eligible"]:
            require(host in PLACEMENTS and status["cpu_affinity"], "paid/unknown or empty eligible placement")
            eligible.add(host)
    candidates, worker_counts, fingerprints = [], {}, {}
    for entry in choice["qualifications"]:
        ref = entry.get("report", entry)
        resolve = artifact_reader(entry.get("path_mappings", []))
        report = read(checked(ref))
        if report["workload"] != family:
            continue
        require(report["schema"] == SCHEMA and report["qualification"] and report["complete"],
                "not completed qualification")
        original = read(resolve(report["request"]))
        actual_config = read(resolve(report["executed_config"]))
        original_config = read(resolve(original["config"]))
        other_runtime = runtime_for(original, resolve)
        require(runtime_identity(other_runtime) == runtime_identity(runtime)
                and runtime_identity(report["runtime"]) == runtime_identity(runtime), "qualification runtime differs")
        require(original.get("schedule_family") == request.get("schedule_family")
                and workload(original_config, request["kind"], family=original.get("schedule_family")) == family
                and actual_config == placed_config(original, original_config, True),
                "qualification workload or placement differs")
        actual = report["fingerprint"]
        if verify_outputs:
            result = read(resolve(report["native_result"]))
            actual = output_fingerprint(actual_config, request["kind"], result, runtime, True, resolve,
                                        report.get("collection"), report.get("run"))
            require(actual == report["fingerprint"], "qualification outputs changed")
        placement = report["placement"]
        require(placement == original["placement"] and placement["host"] in eligible,
                "qualification placement no longer eligible")
        status = choice["inventory"][placement["host"]]
        require(set(placement["cpu_affinity"]) <= set(status["cpu_affinity"]),
                "qualification exceeds eligible CPU set")
        # Same exact sample for each family on every candidate host. Replication
        # seeds may differ in production but cannot be changed between trials.
        sample = digest({"configuration": workload(original_config, request["kind"], keep_seeds=True),
                         "indices": original.get("episode_indices")})
        if fingerprints:
            require((sample, actual) == next(iter(fingerprints.values())),
                    "serial/parallel qualification changed sample or saved learning outputs")
        fingerprints[ref["sha256"]] = (sample, actual)
        worker_counts.setdefault(placement["host"], []).append(placement["workers"])
        execution = read(resolve(report["execution"]))
        require(execution["exit_code"] == 0 and execution["error"] is None,
                "qualification native execution failed")
        require(report["seconds"] >= positive(execution["seconds"], "native wall seconds"),
                "qualification omits native wall time")
        expected_games = len(actual_config["iterations"][0]["episodes"]) if request["kind"] == "training" else len(actual_config["episodes"])
        require(report["completed_games"] == expected_games and report["completed_updates"] ==
                (1 if request["kind"] == "training" else 0), "qualification completed-work count differs")
        archive = read(resolve(report["archive"]))
        require(archive["mismatches"] == 0 and len(archive["shards"]) == 2
                and archive["scheme"] == "two-deflate1-shards-full-readback/v1"
                and archive["native_root"] == actual_config["output_directory"],
                "qualification recovery does not match native output")
        if verify_outputs:
            for shard in archive["shards"]:
                resolve(shard["archive"])
        seconds = projected_seconds(report, config, request["kind"])
        # External transport must be measured and retained, never assumed free.
        transport = choice["inventory"][placement["host"]]["transport_seconds"]
        require(type(transport) in (int, float) and math.isfinite(transport) and transport >= 0,
                "invalid measured transport overhead")
        queue = status.get("queue_seconds", 0)
        require(type(queue) in (int, float) and math.isfinite(queue) and queue >= 0,
                "invalid measured remaining queue time")
        if queue:
            checked(status["queue_evidence"])
        candidates.append((seconds + transport + queue, ref, placement))
    require(set(worker_counts) == eligible, "eligible host lacks measured allocation")
    require(all(counts[0] == 1 and counts == sorted(set(counts)) and any(w > 1 for w in counts)
                for counts in worker_counts.values()), "each host needs serial then increasing workers")
    require(candidates, "missing compatible completed-work throughput evidence")
    best = min(candidates, key=lambda item: item[0])
    require(choice["selected"][family] == best[1] and request["placement"] == best[2],
            "production does not use fastest qualified eligible allocation")
    selected = {"qualification": best[1], "projected_seconds": best[0], "workload": family}
    if not verify_outputs:
        verification = read(checked(request["choice_verification"]))
        require(verification["schema"] == CHOICE and verification["choice"] == path
                and verification["selected"] == selected and verification["outputs_verified"],
                "missing or incompatible verified qualification snapshot")
    return selected


def set_affinity(cpus):
    if not WINDOWS:  # inherited by the native child
        os.sched_setaffinity(0, cpus)
        require(os.sched_getaffinity(0) == set(cpus), "cannot bind eligible CPU affinity")
        return
    import ctypes
    kernel = ctypes.windll.kernel32
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
    require(kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), sum(1 << c for c in cpus)),
            "cannot bind eligible CPU affinity")


def process_sample(child):
    if not WINDOWS:
        # VmRSS is the resident set (Windows: working set), absent once the child
        # has exited; utime+stime sum every thread, as GetProcessTimes does.
        status = (PROC/str(child.pid)/"status").read_text()
        rss = next((int(line.split()[1]) * 1024 for line in status.splitlines() if line.startswith("VmRSS:")), 0)
        fields = (PROC/str(child.pid)/"stat").read_text().rsplit(")", 1)[1].split()
        return {"at_unix": time.time(), "pid": child.pid, "rss_bytes": rss,
                "cpu_seconds": (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")}
    import ctypes
    from ctypes import wintypes
    handle = wintypes.HANDLE(int(child._handle))
    class Memory(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD)] + [
            (key, ctypes.c_size_t) for key in ("peak_rss", "rss", "peak_paged", "paged",
                                              "peak_nonpaged", "nonpaged", "pagefile", "peak_pagefile")]
    memory = Memory(); memory.cb = ctypes.sizeof(memory)
    require(ctypes.windll.psapi.GetProcessMemoryInfo(handle, ctypes.byref(memory), memory.cb),
            "cannot read owned native memory")
    times = [wintypes.FILETIME() for _ in range(4)]
    require(ctypes.windll.kernel32.GetProcessTimes(handle, *[ctypes.byref(t) for t in times]),
            "cannot read owned native CPU time")
    return {"at_unix": time.time(), "pid": child.pid, "rss_bytes": memory.rss,
            "cpu_seconds": sum((t.dwHighDateTime << 32) + t.dwLowDateTime for t in times[2:]) / 1e7}


def require_mapped_libraries(child, runtime, timeout=30.0):
    """One /proc/<pid>/maps observation, once the loader has mapped all four pinned
    libraries: each mapping must resolve to its pinned file, else the launch fails."""
    pinned = {name: os.path.realpath(item["path"]) for name, item in runtime["libraries"].items()}
    names = {Path(real).name: name for name, real in pinned.items()} | {name: name for name in pinned}
    deadline = time.monotonic() + timeout
    while True:
        mapped = {}
        for line in (PROC/str(child.pid)/"maps").read_text().splitlines():
            parts = line.split(maxsplit=5)
            name = names.get(Path(parts[-1].removesuffix(" (deleted)")).name) if len(parts) == 6 else None
            if name:
                mapped.setdefault(name, set()).add(parts[-1])
        if set(mapped) == set(pinned):
            break
        require(child.poll() is None and time.monotonic() < deadline, "pinned runtime libraries were not mapped")
        time.sleep(0.01)
    require(all({os.path.realpath(path) for path in paths} == {pinned[name]} for name, paths in mapped.items()),
            "mapped runtime library differs from its pin")
    return mapped


def busy_pattern(binary=None):
    """Image names that make the host busy. Linux image names carry no .exe, so
    there the pinned binary's own file name joins the pattern."""
    pattern = r"native_expanded_training|expanded_deck_training|cargo|rustc|trainer\.exe"
    if WINDOWS:
        return pattern
    return pattern + r"|^trainer$" + ("|^" + re.escape(Path(binary).name) + "$" if binary else "")


def execute(request_path, qualification):
    from public_training_storage_v1 import archive_native
    import host_reservation_v1 as reservations
    started = time.monotonic()
    request = read(request_path)
    config, runtime = validate_request(request, qualification)
    token = os.environ.get("MTG_HOST_RESERVATION_TOKEN")
    require(token and reservations.status(token).get("token_fate") == "holds",
            "canonical host reservation is not held")
    selected = None if qualification else require_choice(request["choice"], request)
    root, cold = Path(request["root"]), Path(request["cold_root"])
    require(not root.exists() and not cold.exists(), "fresh dispatch and recovery roots required")
    root.mkdir(parents=True); cold.mkdir(parents=True)
    placed = placed_config(request, config, qualification)
    require(not Path(placed["output_directory"]).exists(), "fresh native output root required")
    write(root/"config.json", placed)
    set_affinity(request["placement"]["cpu_affinity"])
    command = [str(checked(runtime["binary"])), str((root/"config.json").resolve())]
    if qualification and request["kind"] == "training":
        command.extend(["--max-new-iterations", "1"])
    failure = None
    with (root/"stdout.jsonl").open("x") as stdout, (root/"stderr.log").open("x") as stderr, \
            (root/"telemetry.jsonl").open("x") as telemetry:
        # Linux: nice 10 before exec, so every native thread inherits it.
        options = ({"creationflags": subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW}
                   if WINDOWS else {"preexec_fn": lambda: os.nice(10)})
        child = subprocess.Popen(command, stdout=stdout, stderr=stderr, **options)
        reservations.record_descendant(token, child.pid)
        write(root/"started.json", {"pid": child.pid, "started_unix": time.time(),
                                    "placement": request["placement"], "request": pin(request_path)})
        try:
            if not WINDOWS:
                require_mapped_libraries(child, runtime)
            while child.poll() is None:
                remaining = request["wall_seconds"] - (time.monotonic() - started)
                require(remaining > 0, "native wall-time limit exceeded")
                try:
                    child.wait(timeout=min(30, remaining))
                except subprocess.TimeoutExpired:
                    sample = process_sample(child)
                    sample.update(validate_storage(request["storage"], 0))
                    telemetry.write(json.dumps(sample) + "\n"); telemetry.flush()
                    require(sample["rss_bytes"] <= request["placement"]["memory_bytes"],
                            "native memory limit exceeded")
            require(child.returncode == 0, "native process failed")
        except BaseException as error:
            failure = str(error)
            if child.poll() is None:
                child.kill()
            child.wait()
        finally:
            write(root/"execution.json", {"exit_code": child.returncode, "error": failure,
                                         "seconds": time.monotonic() - started})
    if failure:
        raise RuntimeError(failure + "; preserve failed outputs at " + str(root))
    last = None
    with (root/"stdout.jsonl").open(encoding="utf-8") as stream:
        for line in stream:
            if line.strip():
                last = json.loads(line)
    require(last is not None, "missing native result")
    write(root/"native-result.json", last)
    fingerprint = output_fingerprint(placed, request["kind"], last, runtime, qualification)
    validate_storage(request["storage"], 0)
    # Existing checked, durable, two-shard archive includes full readback. Raw
    # outputs remain. Both locations count against the same campaign allowance.
    archive = archive_native(Path(placed["output_directory"]), cold)
    validate_storage(request["storage"], 0)
    games = len(placed["iterations"][0]["episodes"]) if request["kind"] == "training" and qualification else (
        sum(len(i["episodes"]) for i in placed["iterations"]) if request["kind"] == "training" else len(placed["episodes"]))
    report = {"schema": SCHEMA, "qualification": qualification, "complete": True,
              "request": pin(request_path), "executed_config": pin(root/"config.json"),
              "execution": pin(root/"execution.json"),
              "native_result": pin(root/"native-result.json"), "runtime": runtime,
              "launcher_sha256": pin(__file__)["sha256"], "workload": workload(config, request["kind"], family=request.get("schedule_family")),
              "placement": request["placement"], "fingerprint": fingerprint,
              "completed_games": games, "completed_updates": (1 if qualification else len(config["iterations"]))
                  if request["kind"] == "training" else 0,
              "seconds": time.monotonic() - started, "archive": pin(cold/"archive.json"),
              "native_logical_bytes": archive["source_bytes"], "recovery_bytes": archive["compressed_bytes"],
              "selected": selected}
    if request["kind"] == "evaluation":
        report["collection"] = pin(Path(placed["output_directory"])/"collection.json")
    else:
        report["run"] = pin(Path(placed["output_directory"])/"run.json")
    write(root/"report.json", report)
    print(json.dumps({"complete": True, "report": pin(root/"report.json")}))


def main():
    require(not sys.flags.optimize, "optimized execution bypasses imported archive checks")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("qualify", "dispatch", "_qualify", "_dispatch", "check-choice"))
    parser.add_argument("request", type=Path)
    # WMI-created supervisors inherit no environment: the launching shell's
    # COMPUTE_HOST_NAME travels to the guarded child on its command line.
    parser.add_argument("--compute-host-name")
    args = parser.parse_args()
    if args.compute_host_name:
        HOSTS["computehost"] = args.compute_host_name.upper()
    require(args.request.is_absolute(), "absolute request path required")
    request = read(args.request)
    qualification = args.action in ("qualify", "_qualify")
    if args.action.startswith("_"):
        execute(args.request, qualification)
    elif args.action == "check-choice":
        selected = require_choice(request["choice"], request, verify_outputs=True)
        write(request["choice_verification_output"], {"schema": CHOICE, "choice": request["choice"],
              "selected": selected, "outputs_verified": True, "launcher": pin(__file__)})
        print(json.dumps(selected))
    else:
        _, runtime = validate_request(request, qualification)
        if not qualification:
            require_choice(request["choice"], request)
        import host_reservation_v1 as reservations
        result = reservations.dispatch(
            lane=request["lane"], work_id=Path(request["root"]).name,
            release_condition="native-expanded process, durable archive and guarded receipt complete or fail",
            command=[sys.executable, "-B", str(Path(__file__).resolve()), "_" + args.action, str(args.request)]
                    + ["--compute-host-name", HOSTS["computehost"]],
            cwd=str(Path(__file__).resolve().parents[2]),
            busy_pattern=busy_pattern(runtime["binary"]["path"]))
        print(json.dumps(result))


if __name__ == "__main__":
    main()
