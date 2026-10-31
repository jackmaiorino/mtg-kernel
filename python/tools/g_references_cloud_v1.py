"""G(i) lineages on RunPod leases (v1): package, plan, execute, ledger.

Derived from nine_deck_cloud_v1.py, whose package format, staging, lease
cycle (external guard, Pod creation, resident guard, hardware check,
resumable staging, observation, recovery, release and the independent
absence check) and ``plan`` are reused unchanged. Differences (collab
LANES/g-references-20261028/DESIGN.md v1.2, section 6, and FROZEN.md):

* The campaign is the G driver's (g_references_campaign_v1.py): each
  lineage's fresh-initialization descriptor travels as a ``play_import``
  (rewritten so its two pins name the Pod copies), every lineage has its own
  pack slot, and the study stops at ``stop_after_block`` (2 for the
  qualification lease, 494 for the D_b pause).
* Layout (FROZEN disposition 2): native outputs, the trajectories, go on the
  Pod's container disk (``native_root``); checkpoints, records, cold archives
  and dose records on the network volume under /workspace/<campaign root>.
  The volume tree is the storage accounting root, with the dispatcher's 60 GiB
  reserve; the native root is block scratch (AMENDMENT-2.md), admitted by the
  driver against the container disk, which the lease preparation sizes
  (prepare_lease ``container_disk_gb``).
* A qualification trial runs one update, and a pack admits its packed trials'
  storage projections together, so the qualification spec carries its own
  per-trial projection (spec ``trial_projected_additional_bytes``, at most a
  block's; default the block's).
* ``mode`` "qualification" launches g_references_qualify_v1.py (plan then
  run, items 1, 2, 3 and 6 of section 6) with the package's inventory;
  "production" launches the campaign driver, which reads the choice pins on
  the volume before every block.
* The supervisor is started detached with /dev/null for stdin, stdout and
  stderr (host_reservation_v1.spawn_detached on Linux, the equivalent of
  ``nohup ... < /dev/null``), so the SSH call that launches it returns at once.
* A Pod's granted CPUs need not be 0..n-1: a 32-vCPU Pod on a 192-CPU host
  was granted 5, 9, 11, 20, ... When some placement CPU is not granted, the
  lease binds the Pod's sorted granted CPUs to logical CPUs 0..n-1 by position
  (host_slots_v1 CPU_BINDING_ENV in the launch environment): the package, its
  records and every hashed input keep the logical CPUs, only the OS affinity
  calls see the granted ones, and ``cpu-binding.json`` records the map.
* Recovery also brings back the dose records and the qualification folder.
* ``ledger`` sums every lease of the study (control folders under one root):
  pod hours from the lease records, the conservative cost at the lease's rate
  ceiling and the volume charge, against the study caps (60 USD, 60 wall
  hours of lease time). ``execute`` refuses a lease that could cross either.

The API key comes from the environment (the user registry on Windows only),
is held in memory and never written.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import shutil
import tarfile
import tempfile
import time
from pathlib import Path, PurePosixPath

import g_references_campaign_v1 as driver
import g_references_records_v1 as records
import g_references_v1 as g
import host_slots_v1
import nine_deck_cloud_v1 as cloud
from nine_deck_cloud_v1 import Bundle, is_pin, require, safe_name, sha, write, read, encoded, pin

SCHEMA = "g-references-cloud-package/v1"
SPEC_SCHEMA = "g-references-cloud-package-spec/v1"
# The qualification tool starts the pack qualifier by path, not by import.
CLOSURE_ROOTS = cloud.CLOSURE_ROOTS + ("g_references_campaign_v1.py", "g_references_qualify_v1.py",
                                       "native_expanded_pack_v1.py")
ROOTS = ("state", "cold", "retained", "checkpoints", "exposure", "hot", "doses")
TEMPLATE_KEYS = {"recipe", "lane", "decks", "play_imports", "placement", "placement_by_run", "runtime", "storage",
                 "wall_seconds", "choice_pins", "cold_keep_blocks", "roots", "compute_host_name", "host_slots",
                 "lease_block_seconds", "stop_after_block", "ancestry_receipt"}
MODES = ("qualification", "production")
CAP_USD = 60.0
CAP_LEASE_SECONDS = 60 * 3600
VOLUME_USD_PER_GB_MONTH = 0.07


def build_tooling(bundle, base, host_slots=None):
    """nine_deck_cloud_v1.build_tooling with this study's driver and qualification tool in the closure."""
    sources = {name: cloud.TOOLS / name for name in cloud.closure(cloud.TOOLS, CLOSURE_ROOTS)}
    sources[cloud.HOST_SLOTS] = cloud.TOOLS / cloud.HOST_SLOTS
    require(sources[cloud.HOST_SLOTS].is_file(), cloud.HOST_SLOTS + " is in this tree")
    require(not cloud.imported(sources[cloud.HOST_SLOTS], ()), cloud.HOST_SLOTS + " must need only the standard library")
    digests = {name: pin(path)["sha256"] for name, path in sources.items()}
    identity = hashlib.sha256("".join(f"{name} {digests[name]}\n" for name in sorted(digests)).encode()).hexdigest()[:16]
    with tempfile.TemporaryDirectory() as temporary:
        for name, path in sources.items():
            target = Path(temporary, name)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        cloud.require_closure_imports(temporary)
    directory = PurePosixPath("tooling") / identity
    for name, path in sources.items():
        bundle.file(directory / "python/tools" / name, path)
    root = base / directory
    return {"id": identity, "root": str(root), "modules": sorted(sources),
            "host_slots": {"source": "tree", "sha256": digests[cloud.HOST_SLOTS],
                           "path": str(root / "python/tools" / cloud.HOST_SLOTS)}}


def slots(spec_slots, runs):
    """One disjoint pack slot per lineage, in run order."""
    require(len(spec_slots) == len(runs), "one slot per lineage")
    placements = {run: cloud.checked_placement(slot) for run, slot in zip(runs, spec_slots)}
    cpus = [cpu for slot in placements.values() for cpu in slot["cpu_affinity"]]
    require(len(cpus) == len(set(cpus)), "pack slots must be disjoint")
    return placements


def build_inputs(spec, template, base, bundle, runtime_pin, tooling):
    objects = PurePosixPath("inputs/objects")

    def carry(item):
        cloud.checked_pin(item)
        path = objects / item["sha256"] / safe_name(Path(item["path"]).name)
        bundle.file(path, item["path"])
        return {**item, "path": str(base / path)}

    def descriptor(item):  # the fresh-initialization descriptor names its own files: they move with it
        cloud.checked_pin(item)
        value = read(item["path"])
        require(isinstance(value, dict) and value.get("schema") == g.FRESH_SOURCE_SCHEMA,
                "play_import must be a fresh-initialization source descriptor: " + item["path"])
        data = json.dumps({key: carry(entry) if is_pin(entry) else entry for key, entry in value.items()},
                          sort_keys=True, separators=(",", ":")).encode()
        digest = hashlib.sha256(data).hexdigest()
        path = objects / digest / safe_name(Path(item["path"]).name)
        bundle.data(path, data)
        return {"path": str(base / path), "sha256": digest}

    decks = Path(template["decks"])
    pin(decks, g.ndb.RUNTIME_DECKS_SHA256)
    repository = decks.resolve().parents[1]
    g.ndb.load_decks(decks)
    deck_files = {"data/runtime_decks_v1.json": decks} | {name: repository / name for name in cloud.DECK_COMPANIONS}
    digests = {name: pin(path)["sha256"] for name, path in deck_files.items()}
    deck_directory = PurePosixPath("inputs/decks") / hashlib.sha256(encoded(digests)).hexdigest()[:16]
    for name, path in deck_files.items():
        bundle.file(deck_directory / name, path)
    runs = spec["runs"]
    placements = slots(spec["slots"], runs)
    union = sorted({cpu for slot in placements.values() for cpu in slot["cpu_affinity"]})
    native = PurePosixPath(spec["native_root"])
    require(native.is_absolute() and not native.is_relative_to(cloud.WORKSPACE), "native root is on the container disk")
    storage = cloud.checked_storage(spec["storage"], base)
    # AMENDMENT-2.md: the trajectories are block scratch, admitted against the container disk (spec "scratch":
    # the measured per-block peak, its factor and the margin); the cap counts the campaign tree on the volume.
    storage["scratch_roots"] = [str(native)]
    scratch = spec["scratch"]
    require(isinstance(scratch, dict) and set(scratch) == {"block_peak_bytes", "factor", "margin_fraction"},
            "the spec's scratch names the per-block peak, its factor and the margin")
    # A running attempt ends only on a physical shortage (the provisioned volume less its margin, or the
    # container disk's free space below it); the cap gates new dispatches. Sealed block records leave the
    # volume through the copy-back window (g_references_records_v1.py).
    emergency, records = spec["emergency"], spec["records"]
    require(isinstance(emergency, dict) and set(emergency) == {"volume_bytes", "margin_fraction"},
            "the spec's emergency names the provisioned volume and the margin")
    require(isinstance(records, dict) and set(records) == {"window_blocks", "stall_seconds"},
            "the spec's records name the copy-back window and its stall bound")
    storage["emergency"] = emergency
    campaign = {"recipe": g.PREFIX, "lane": template["lane"], "scratch": scratch, "records": records,
                "decks": str(base / deck_directory / "data/runtime_decks_v1.json"),
                "play_imports": {run: descriptor(template["play_imports"][run]) for run in g.RUNS},
                "placement": {**placements[runs[0]], "cpu_affinity": union},
                "placement_by_run": placements, "runtime": runtime_pin, "storage": storage,
                "wall_seconds": template["wall_seconds"], "cold_keep_blocks": template["cold_keep_blocks"],
                "choice_pins": str(base / "campaign" / "qualification" / "choice-pins.json"),
                "roots": {name: str(base / "campaign" / name) for name in ROOTS}
                         | {"hot_by_run": {}, "native": str(native)},
                "stop_after_block": spec["stop_after_block"],
                "host_slots": {"cores": cloud.cores(union),
                               "tool": {"path": tooling["host_slots"]["path"],
                                        "sha256": tooling["host_slots"]["sha256"]}}}
    require(storage["max_logical_bytes"] <= driver.STUDY_CAP_BYTES, "the storage allowance exceeds the study's 120 GB")
    for key in ("ancestry_receipt", "placement_record", "continuation"):
        item = spec.get(key)
        if item:
            require(is_pin(item) and PurePosixPath(item["path"]).is_relative_to(base),
                    key + " is a pin on the volume (qualification records, the placement record, the D_b decision)")
            campaign[key] = item
    require(spec["mode"] == "qualification" or "placement_record" in campaign,
            "production needs the placement record on the volume")
    block = spec.get("lease_block_seconds", template.get("lease_block_seconds"))
    if block is not None:
        cloud.native_dispatch.positive(block, "lease block seconds")
        campaign["lease_block_seconds"] = block
    cloud.native_dispatch.positive(campaign["wall_seconds"], "wall-time limit")
    extra = {}
    if spec["mode"] == "qualification":
        inventory = read(spec["inventory"])
        inventory["inventory"] = {host: {**status, "evidence": carry(status["evidence"])}
                                  for host, status in inventory["inventory"].items()}
        data = encoded(inventory)
        path = objects / hashlib.sha256(data).hexdigest() / "inventory.json"
        bundle.data(path, data)
        # A trial is one update, and a pack admits its packed trials' projections together: the trials take their
        # own per-request projection (spec trial_projected_additional_bytes), at most a block's.
        trial = spec.get("trial_projected_additional_bytes", storage["projected_additional_bytes"])
        require(type(trial) is int and 0 < trial <= storage["projected_additional_bytes"],
                "a trial's projection is positive and at most a block's")
        trial_storage = {**storage, "projected_additional_bytes": trial, "projected_volume_bytes": {"": trial}}
        qualification = {"slots": spec["slots"], "storage": trial_storage, "wall_seconds": template["wall_seconds"],
                         "inventory": {"path": str(base / path), "sha256": hashlib.sha256(data).hexdigest()},
                         "interrupt_at_update": spec.get("interrupt_at_update", 81)}
        data = encoded(qualification)
        path = objects / hashlib.sha256(data).hexdigest() / "qualification-spec.json"
        bundle.data(path, data)
        extra["qualification_spec"] = {"path": str(base / path), "sha256": hashlib.sha256(data).hexdigest()}
    data = encoded(campaign)
    digest = hashlib.sha256(data).hexdigest()
    path = objects / digest / "campaign.json"
    bundle.data(path, data)
    return {"path": str(base / path), "sha256": digest}, campaign, data, extra


def package(spec_path, output):
    spec = read(spec_path)
    require(spec.get("schema") == SPEC_SCHEMA, "package spec schema differs")
    require(spec["mode"] in MODES, "mode is qualification or production")
    name = spec["campaign_root"]
    require(isinstance(name, str) and re.fullmatch("[a-z0-9][a-z0-9-]{0,62}", name), "campaign root must be a short lowercase name")
    runs = spec["runs"]
    require(runs == list(g.RUNS), "every lineage runs in one campaign")
    stop = spec["stop_after_block"]
    require(type(stop) is int and (stop == 2 if spec["mode"] == "qualification" else 2 < stop <= g.BLOCKS),
            "qualification stops after block 2; production after a later block")
    template = read(spec["campaign"])
    require(set(template) <= TEMPLATE_KEYS and template.get("recipe") == g.PREFIX, "not a G(i) campaign template")
    memory = sum(slot["memory_bytes"] for slot in spec["slots"])
    hardware = {"min_memory_bytes": spec["hardware"]["min_memory_bytes"],
                "qualified": cloud.qualified_hardware(spec["hardware"].get("qualified"))}
    require(type(hardware["min_memory_bytes"]) is int and hardware["min_memory_bytes"] >= memory,
            "expected Pod memory must hold every slot's memory limit")
    output = Path(output)
    require(not output.exists(), "fresh package output required")
    base = cloud.WORKSPACE / name
    bundles = {kind: Bundle() for kind in ("runtime", "tooling", "inputs")}
    runtime_pin, runtime = cloud.build_runtime(spec["runtime"], base, bundles["runtime"])
    tooling = build_tooling(bundles["tooling"], base)
    campaign_pin, campaign, data, extra = build_inputs(spec, template, base, bundles["inputs"], runtime_pin, tooling)
    output.mkdir(parents=True)
    archives = {kind: bundle.write(output / f"{kind}.tar.gz") for kind, bundle in bundles.items()}
    with (output / "campaign.json").open("xb") as stream:
        stream.write(data)
    manifest = {"schema": cloud.SCHEMA, "recipe_schema": SCHEMA, "mode": spec["mode"],
                "controller_sha256": sha(cloud.__file__), "recipe_controller_sha256": sha(__file__),
                "spec_sha256": sha(spec_path), "image": cloud.IMAGE_REFERENCE, "workspace": str(cloud.WORKSPACE),
                "campaign_root": name, "pod_root": str(base), "runs": runs, "placement": campaign["placement"],
                "placement_by_run": campaign["placement_by_run"], "storage": campaign["storage"],
                "native_root": campaign["roots"]["native"], "wall_seconds": campaign["wall_seconds"],
                "lease_block_seconds": campaign.get("lease_block_seconds"), "stop_after_block": stop,
                "hardware": hardware, "campaign": campaign_pin, "runtime": runtime_pin,
                "runtime_identity": cloud.native_dispatch.runtime_identity(runtime), "tooling": tooling,
                "volume_inputs": [] if spec["mode"] == "qualification" else
                [{"role": "choice_pins", "path": campaign["choice_pins"], "sha256": spec["choice_pins_sha256"]}]
                + [{"role": key, **campaign[key]} for key in ("placement_record", "continuation", "ancestry_receipt")
                   if key in campaign],
                "production_ready": True, "archives": archives, **extra}
    write(output / "package.json", manifest)
    return manifest


def verified_package(directory):
    manifest = cloud.verified_package(directory)
    require(manifest.get("recipe_schema") == SCHEMA and manifest["recipe_controller_sha256"] == sha(__file__),
            "package was built by a different G(i) controller")
    return manifest


# ---------------------------------------------------------------- the lease

POD_RECOVER = cloud.POD_RECOVER.replace(
    '''    for name in ("retained", "checkpoints", "exposure"):''',
    '''    for name in ("retained", "checkpoints", "exposure", "doses", "qualification"):''')
assert POD_RECOVER != cloud.POD_RECOVER


POD_AFFINITY = '''#pod:affinity
import os
print(json.dumps(sorted(os.sched_getaffinity(0))))
'''


def cpu_binding(placement, granted):
    """The Pod's granted CPUs (sorted) as logical CPUs 0..n-1, or None when every placement CPU is granted (the
    package's CPUs are the OS's own)."""
    cpus, granted = placement["cpu_affinity"], sorted(granted)
    if set(cpus) <= set(granted):
        return None
    require(max(cpus) < len(granted), "placement CPUs unavailable on the Pod")
    return granted


POD_RESULT = '''#pod:result
import pathlib
path = pathlib.Path(ARGS["result"])
print(json.dumps(json.loads(path.read_text()) if path.exists() else None))
'''


POD_QUIESCE_RECORDS = '''import os, time
open(ARGS["quiesce"], "w").close()
deadline = time.monotonic() + ARGS["timeout"]
try:
    import fcntl
    with open(ARGS["lock"], "a") as lock:
        while True:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                quiesced = True
                break
            except BlockingIOError:
                if time.monotonic() > deadline:
                    quiesced = "timed out"
                    break
                time.sleep(1)
except ImportError:
    quiesced = True
print(json.dumps({"quiesced": quiesced}))
'''


class Cycle(cloud.Cycle):
    """nine_deck_cloud_v1.Cycle with the G driver (or the qualification tool) and G statuses."""

    def observe(self):
        """The base observation, with a lineage reported complete only through this package's stop block: a
        state finished at an earlier target (qualification's block 2, or D_b) is pending work for this lease."""
        snapshot = super().observe()
        target = self.manifest["stop_after_block"]
        passed = self.qualification_passed() if self.manifest["mode"] == "qualification" else True
        for state in snapshot["runs"].values():
            if state.get("status") == "complete" and (max(state.get("blocks_done") or [0]) < target or not passed):
                state["status"] = "pending"  # a qualification lease is done only when the tool's run passed
        snapshot["qualification_passed"] = passed
        return snapshot

    def qualification_passed(self) -> bool:
        """The qualification tool's own result (campaign/qualification/RESULT.json), not the lineages' status."""
        result = self.pod.run(POD_RESULT, {"result": str(self.base / "campaign" / "qualification" / "RESULT.json")},
                              read_only=True)
        return bool(result) and result.get("passed") is True

    def check_hardware(self):
        """The base check, with the placement read through the CPU binding (cpu_binding) when one is needed."""
        self.binding = cpu_binding(self.manifest["placement"], self.pod.run(POD_AFFINITY, {}, read_only=True))
        if self.binding is None:
            return super().check_hardware()
        placement = self.manifest["placement"]
        self.manifest["placement"] = {**placement,
                                      "cpu_affinity": [self.binding[cpu] for cpu in placement["cpu_affinity"]]}
        try:
            super().check_hardware()
        finally:
            self.manifest["placement"] = placement
        write(self.control / "cpu-binding.json", {"pod_id": self.created["id"],
                                                  "env": host_slots_v1.CPU_BINDING_ENV, "granted": self.binding,
                                                  "logical": {str(cpu): to for cpu, to in enumerate(self.binding)}})

    def check_volume_inputs(self):
        if self.manifest["mode"] == "production":
            super().check_volume_inputs()

    def launch(self):
        require(time.time() - self.lease["created_epoch"] < self.lease["startup_idle_seconds"] - 60,
                "startup outlasted the guards' startup idle bound")
        env = {"RUNPOD_POD_ID": self.created["id"], cloud.native_dispatch.LEASE_GUARD_ENV: self.guard,
               "MTG_HOST_LOCK_ROOT": cloud.LOCK_ROOT, "HOST_SLOTS_ROOT": cloud.LOCK_ROOT, "PYTHONDONTWRITEBYTECODE": "1"}
        if getattr(self, "binding", None):
            env[host_slots_v1.CPU_BINDING_ENV] = ",".join(map(str, self.binding))
        self.worker = self.pod.run(cloud.POD_LAUNCH, {"control": self.remote_control, "argv": driver_command(self.manifest),
                                                      "cwd": self.manifest["tooling"]["root"], "env": env}, timeout=360)
        write(self.control / "worker-started.json", {**self.worker, "pod_id": self.created["id"]})

    def conclude(self, snapshot):
        """Complete when every lineage completed through the stop block or ended for good (a failed lineage
        is final under DESIGN section 4); otherwise only a lease stop at a block boundary is expected."""
        runs = snapshot["runs"]
        self.count(runs)
        if self.manifest["mode"] == "qualification":
            self.result["qualification_passed"] = snapshot.get("qualification_passed") is True
            require(self.result["qualification_passed"], "the qualification run ended without passing")
            self.result["complete"] = True
            return
        status = {run: (runs.get(run) or {}).get("status") for run in self.manifest["runs"]}
        reasons = {run: runs[run]["stop_reason"] for run in status if (runs.get(run) or {}).get("stop_reason")}
        failed = {run for run in status if status[run] == "stopped" and (runs.get(run) or {}).get("dispositions")}
        self.result.update(run_status=status, stop_reasons=reasons, failed_lineages=sorted(failed))
        if all(value == "complete" or run in failed for run, value in status.items()):
            self.result["complete"] = True
            return
        # Resumable at a block boundary: the lease's time, or storage admission (AMENDMENT-2.md: the record
        # window, its stall latch or a scratch refusal; the next lease drains the window and resumes).
        exhausted = {run for run, reason in reasons.items() if status[run] == "stopped"
                     and (reason == "lease_time_exhausted" or reason.startswith("storage admission: "))}
        require(exhausted and all(status[run] == "complete" or run in exhausted or run in failed for run in status),
                "campaign ended without completing or a lease stop: " + json.dumps(status))
        self.result["lease_stopped"] = True

    def quiesce_records(self, timeout: float = 180.0):
        """Stop the record copy-back before recovery reads the volume, so nothing it lists is deleted under it.
        The follower is asked to start no more moves (records-quiesce.json), then, on the Pod, the quiesce flag
        is set and the copy-back's lock taken: a deletion in progress holds it to its end, and every later one
        sees the flag and refuses. ``timeout`` (one block's deletion takes seconds) leaves the rest of the
        lease's recovery time for recovery itself. Returns True only on positive quiescence, after recording
        the volume's final backlog (records-backlog.json) for the next lease."""
        request = self.control / "records-quiesce.json"
        if not request.exists():
            write(request, {"epoch": time.time()})
        result = self.pod.run(POD_QUIESCE_RECORDS, {"lock": records.POD_LOCK, "quiesce": records.POD_QUIESCE,
                                                    "timeout": timeout}, timeout=timeout + 60)
        self.result["records_quiesced"] = result["quiesced"]
        if result["quiesced"] is not True:
            return False
        records.backlog(self.pod, str(self.base / "campaign" / "state"), self.control)
        return True

    def recover(self):
        """nine_deck_cloud_v1.Cycle.recover with this study's recovery program (dose records, qualification)."""
        if not self.remote_ready:
            return
        if not self.quiesce_records():
            # A deletion may still be running: recovery does not enumerate the volume under it. The volume
            # keeps everything; the next lease recovers from it.
            self.result["recovery"] = "skipped: the record copy-back did not quiesce"
            return
        since = 0 if self.recover_all else None if self.worker is None else self.worker["epoch"] - 60
        packed = self.pod.run(POD_RECOVER, {"control": self.remote_control, "campaign": str(self.base / "campaign"),
                                            "guard": self.guard, "locks": cloud.LOCK_ROOT, "since": since},
                              timeout=cloud.RECOVERY_SECONDS)
        target, recovered = self.control / "results.tar.gz", self.control / "recovered"
        self.pod.get(self.remote_control + "/results.tar.gz", target, timeout=cloud.RECOVERY_SECONDS)
        require(sha(target) == packed["sha256"], "recovered archive differs")
        recovered.mkdir()
        with tarfile.open(target) as tar:
            tar.extractall(recovered, filter="data")
        listed = read(recovered / "RECOVERED.json")
        present = {path.relative_to(recovered).as_posix() for path in recovered.rglob("*") if path.is_file()}
        require(present == set(listed) | {"RECOVERED.json"} and len(listed) == packed["files"] and
                all(sha(recovered / name) == digest for name, digest in listed.items()), "recovered files differ")
        write(self.control / "recovery.json", {**packed, "archive": pin(target)})
        runs = {}
        for run in self.manifest["runs"]:
            path = recovered / "campaign/state" / (run + ".json")
            if path.exists():
                runs[run] = {"blocks_done": [key for key, value in read(path)["blocks"].items() if value.get("done")]}
        self.count(runs)
        self.result["recovered"] = True


def driver_command(manifest):
    tools = PurePosixPath(manifest["tooling"]["root"]) / "python/tools"
    campaign = manifest["campaign"]["path"]
    if manifest["mode"] == "qualification":
        folder = str(PurePosixPath(manifest["pod_root"]) / "campaign" / "qualification")
        return [str(tools / "g_references_qualify_v1.py"), "start", "--campaign", campaign,
                "--spec", manifest["qualification_spec"]["path"], "--out", folder]
    return [str(tools / "g_references_campaign_v1.py"), "launch", "--campaign", campaign,
            "--runs", ",".join(manifest["runs"])]


# ---------------------------------------------------------------- the study's caps


def ledger(root: Path, volume_gb: int, volume_created_epoch: float, now: float | None = None) -> dict:
    """Every lease of the study (``root``/<lease name>/ control folders): lease seconds from creation to the
    confirmed release (or now while unconfirmed), at the lease's rate ceiling, plus the volume charge. A lease the
    research lead closed (``release-closed-by-lead.json``: a create that left no Pod id, so no release can be
    confirmed, with the evidence that no Pod ran or was billed) counts at 0 and cites that record."""
    now = time.time() if now is None else now
    leases, seconds, usd = [], 0.0, 0.0
    for control in sorted(path for path in Path(root).iterdir() if (path / "lease" / "lease.json").exists()):
        lease = read(control / "lease" / "lease.json")
        attempt = control / "lease" / "create-attempt.json"  # written before the POST
        confirmed = control / "release-confirmed.json"
        start = read(attempt)["epoch"] if attempt.exists() else None
        if start is None:
            leases.append({"lease": lease["name"], "created": False})
            continue
        closed = control / "release-closed-by-lead.json"
        if not confirmed.exists() and closed.exists():
            record = read(closed)
            require(record.get("lease") == lease["name"] and record.get("usd") == 0.0 and record.get("closed_by"),
                    "a lead closure names this lease, 0 USD and who closed it")
            leases.append({"lease": lease["name"], "seconds": 0.0, "usd_at_ceiling": 0.0, "release_confirmed": False,
                           "closed_by_lead": pin(closed)})
            continue
        end = read(confirmed)["epoch"] if confirmed.exists() else now
        span = max(0.0, end - start)
        cost = span / 3600 * lease["rate_ceiling_usd_hour"]
        seconds, usd = seconds + span, usd + cost
        leases.append({"lease": lease["name"], "seconds": span, "usd_at_ceiling": cost,
                       "release_confirmed": confirmed.exists()})
    volume = max(0.0, now - volume_created_epoch) / (30 * 86400) * volume_gb * VOLUME_USD_PER_GB_MONTH
    return {"schema": "g-references-v1/lease-ledger", "leases": leases, "lease_seconds": seconds,
            "compute_usd_at_ceiling": usd, "volume_usd": volume, "total_usd": usd + volume,
            "caps": {"usd": CAP_USD, "lease_seconds": CAP_LEASE_SECONDS}}


def require_within_caps(summary: dict, lease: dict, volume_gb: int) -> None:
    """The next lease at its full span and rate ceiling, plus the volume's charge over that span, fits both caps."""
    span = lease["deadline_epoch"] - lease["created_epoch"]
    require(summary["lease_seconds"] + span <= CAP_LEASE_SECONDS, "the next lease could cross 60 wall hours of lease time")
    volume = span / (30 * 86400) * volume_gb * VOLUME_USD_PER_GB_MONTH
    require(summary["total_usd"] + span / 3600 * lease["rate_ceiling_usd_hour"] + volume <= CAP_USD,
            "the next lease could cross the study's 60 USD cap")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="op", required=True)
    packing = commands.add_parser("package")
    packing.add_argument("--spec", type=Path, required=True)
    packing.add_argument("--output", type=Path, required=True)
    for name in ("plan", "execute"):
        command = commands.add_parser(name)
        command.add_argument("--package", type=Path, required=True)
        command.add_argument("--control", type=Path, required=True)
        command.add_argument("--external-guard", type=Path, default=cloud.EXTERNAL_GUARD)
        command.add_argument("--leases", type=Path, required=True, help="the study's lease control folders")
        command.add_argument("--volume-gb", type=int, required=True)
        command.add_argument("--volume-created-epoch", type=float, required=True)
        if name == "execute":
            command.add_argument("--ssh-key", type=Path)
            command.add_argument("--recover-all", action="store_true")
            command.add_argument("--execute", action="store_true", help="allocate the Pod (paid compute)")
    account = commands.add_parser("ledger")
    account.add_argument("--leases", type=Path, required=True)
    account.add_argument("--volume-gb", type=int, required=True)
    account.add_argument("--volume-created-epoch", type=float, required=True)
    args = parser.parse_args(argv)
    if args.op == "package":
        manifest = package(args.spec, args.output)
        print(json.dumps({"package": str(args.output), "mode": manifest["mode"], "pod_root": manifest["pod_root"],
                          "archives": {kind: item["sha256"] for kind, item in manifest["archives"].items()}}))
        return 0
    summary = ledger(args.leases, args.volume_gb, args.volume_created_epoch)
    if args.op == "ledger":
        print(json.dumps(summary))
        return 0
    verified_package(args.package)
    require_within_caps(summary, read(args.control / "lease" / "lease.json"), args.volume_gb)
    if args.op == "plan" or not args.execute:
        print(json.dumps(cloud.plan(args.package, args.control, args.external_guard) | {"ledger": summary}))
        return 0
    require(args.ssh_key is not None, "--ssh-key is required with --execute")
    planned = cloud.plan(args.package, args.control, args.external_guard)
    require(Path(args.ssh_key).is_file(), "SSH key unavailable")
    write(Path(args.control) / "plan.json", planned | {"ledger": summary})
    write(Path(args.control) / "controller-inputs.json", {"package": pin(Path(args.package) / "package.json"),
                                                          "controller": pin(__file__),
                                                          "base_controller": pin(cloud.__file__),
                                                          "external_guard": pin(args.external_guard)})
    key = cloud.api_key()
    os.environ["RUNPOD_API_KEY"] = key
    result = Cycle(args.package, args.control, args.ssh_key, args.external_guard, key, args.recover_all, None, None,
                   None, cloud.subprocess.run, cloud.spawn_detached, time.sleep).run()
    print(json.dumps(result))
    return 0 if (result["complete"] or result["lease_stopped"]) and result["recovered"] and result["release_confirmed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
