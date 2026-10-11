"""Verify the fresh four-case I/O comparison and publish durable analysis.

Never executes training, prunes evidence, reads old campaign results, or replaces
an existing analysis. Full recovery readbacks occur only when explicitly run.
"""
from __future__ import annotations

import argparse
import copy
import csv
from datetime import datetime
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import statistics
import sys
import types
import zipfile


BASE = Path("D:/training-io-speedups-20261010")
COLD = Path("E:/training-io-speedups-20261010")
ROOT = BASE / "desktop"
LABELS = ["io-01-baseline", "io-02-candidate", "io-03-candidate", "io-04-baseline"]
SCHEMA = "training-io-speedups-analysis/v1"
FIELDS = ("matched_case_wall_seconds", "raw_case_wall_seconds", "dispatch_seconds",
          "child_seconds", "inspect_seconds", "exposure_seconds", "archive_seconds",
          "copy_seconds", "retain_seconds", "late_copy_seconds", "late_receipt_seconds")
UPDATE_TIMERS = ("input_read_seconds", "behavior_replay_seconds", "learner_update_seconds",
                 "checkpoint_io_seconds", "update_elapsed_seconds")
PREPARATION_TIMERS = ("validation_and_planning_seconds", "opponent_loading_seconds",
                      "behavior_binding_seconds", "parallel_replay_seconds", "preparation_elapsed_seconds")
PREPARATION_PLACEMENT = ("requested_workers", "started_workers", "worker_stack_reservation_bytes", "worker_timings")
PREPARATION_FIELDS = {"schema", "physical_group_jobs", "distinct_opponent_sources", "opponent_load_calls",
                      "opponent_cache_hits", "current_model_reuses", "decoded_tensor_payload_bytes",
                      "max_decoded_tensor_bytes", "timing_scope", "memory_scope",
                      *PREPARATION_TIMERS, *PREPARATION_PLACEMENT}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def seconds(value, name):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0,
            "invalid finite seconds: " + name)
    return value


def safe(path):
    path = Path(path)
    require(path.is_absolute() and ".." not in path.parts, "absolute nontraversing path required")
    for item in (path, *path.parents):
        try:
            metadata = item.lstat()
        except FileNotFoundError:
            continue
        require(not item.is_symlink() and not item.is_junction()
                and not getattr(metadata, "st_file_attributes", 0) & 0x400,
                "reparse/link path refused: " + str(item))
    return path


def pin(path):
    path = safe(path)
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "sha256": digest}


def same_path(first, second):
    return str(first).replace("\\", "/").rstrip("/").casefold() == str(second).replace("\\", "/").rstrip("/").casefold()


class Reader:
    def __init__(self):
        self.inputs = {}

    def verify(self, ref, size=None):
        require(isinstance(ref, dict) and set(ref) >= {"path", "sha256"}
                and re.fullmatch("[a-f0-9]{64}", ref["sha256"]), "explicit SHA pin required")
        actual = pin(ref["path"])
        require(actual["sha256"] == ref["sha256"], "changed pinned input: " + ref["path"])
        if size is not None:
            require(type(size) is int and size >= 0 and Path(ref["path"]).stat().st_size == size,
                    "pinned size differs: " + ref["path"])
        self.inputs[str(Path(ref["path"]))] = actual
        return actual

    def bytes(self, ref):
        payload = safe(ref["path"]).read_bytes()
        require(hashlib.sha256(payload).hexdigest() == ref["sha256"], "changed pinned input: " + ref["path"])
        self.inputs[str(Path(ref["path"]))] = {"path": ref["path"], "sha256": ref["sha256"]}
        return payload

    def json(self, ref):
        return json.loads(self.bytes(ref))

    def module(self, ref, name):
        module = types.ModuleType(name)
        module.__file__ = ref["path"]
        exec(compile(self.bytes(ref), ref["path"], "exec"), module.__dict__)
        return module


def relative_member(name):
    value = PurePosixPath(name)
    require(isinstance(name, str) and value.parts and not value.is_absolute()
            and value.as_posix() == name and ".." not in value.parts
            and not re.search(r'[\\:\x00-\x1f]', name), "unsafe ZIP member: " + str(name))
    return name


def canonical_digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def update_comparison(update, workers):
    """Preserve all fields except enumerated timing and placement diagnostics."""
    require(update["schema"] == "mtg-kernel-expanded-deck-update/v1" and update["complete"] is True
            and update["checkpoint_readback"] is True and update["numerical_backend"] == "native-cpu-sequential",
            "complete CPU update receipt required")
    require(set(update) >= {"source", "trajectories", "checkpoint", "before_state_sha256", "after_state_sha256",
                           "adam_step", "physical_decisions", "policy_substeps", "loss", "policy_sum",
                           "value_sum", "loss_identity", "claim", *UPDATE_TIMERS}, "update scientific fields absent")
    scientific = copy.deepcopy(update)
    excluded = []
    for field in UPDATE_TIMERS:
        seconds(scientific.pop(field), "update." + field)
        excluded.append(field)
    if "gae_phase_profile" in scientific:
        profile = scientific["gae_phase_profile"]
        require(set(profile) == {"update_elapsed_ns", "records"}, "unknown GAE profile field")
        nonnegative_integer(profile.pop("update_elapsed_ns"), "GAE profile elapsed ns")
        excluded.append("gae_phase_profile.update_elapsed_ns")
        for record in profile["records"]:
            require(set(record) == {"phase", "elapsed_ns"}, "unknown GAE phase record field")
            nonnegative_integer(record.pop("elapsed_ns"), "GAE phase elapsed ns")
        excluded.append("gae_phase_profile.records[*].elapsed_ns")
    preparation = scientific.pop("update_preparation", None)
    require((preparation is not None) == (workers > 1), "preparation receipt does not match selected workers")
    prepared_digest = None
    if preparation is not None:
        require(set(preparation) == PREPARATION_FIELDS
                and preparation["schema"] == "mtg-kernel-ordered-update-preparation/v1"
                and preparation["requested_workers"] == workers, "unknown/mismatched preparation receipt")
        require(len(preparation["worker_timings"]) == preparation["started_workers"], "worker timing coverage differs")
        groups = 0
        for record in preparation["worker_timings"]:
            require(set(record) == {"worker", "completed_groups", "busy_wall_seconds"}, "unknown worker timing field")
            nonnegative_integer(record["worker"], "worker index")
            groups += nonnegative_integer(record["completed_groups"], "worker groups")
            seconds(record["busy_wall_seconds"], "worker busy wall")
        require(groups == preparation["physical_group_jobs"], "preparation worker group coverage differs")
        for field in PREPARATION_TIMERS:
            seconds(preparation.pop(field), "preparation." + field)
        for field in PREPARATION_PLACEMENT:
            preparation.pop(field)
        prepared_digest = canonical_digest(preparation)
        excluded += ["update_preparation." + field for field in (*PREPARATION_TIMERS, *PREPARATION_PLACEMENT)]
    return {"scientific_sha256": canonical_digest(scientific), "preparation_deterministic_sha256": prepared_digest,
            "excluded_fields": excluded,
            "preparation_presence": "present" if preparation is not None else "absent for serial placement"}


def nonnegative_integer(value, name):
    require(type(value) is int and value >= 0, "invalid nonnegative integer: " + name)
    return value


def physical_stats(stats):
    require(stats["method"] == "GetCompressedFileSizeW" and stats["excludes_directory_and_volume_metadata"] is True,
            "physical receipt measurement method differs")
    for field in ("files", "logical_bytes", "allocated_file_bytes"):
        nonnegative_integer(stats[field], "physical." + field)
    return copy.deepcopy(stats)


def allocation_inventory(record, native):
    require(record["schema"] == "training-speedups-allocation-inventory/v1" and record["complete"] is True
            and not record["errors"] and not record["skipped_reparse_points"]
            and same_path(record["root"], ROOT) and same_path(record["native_root"], native),
            "complete allocation receipt for exact roots required")
    classes = record["classes"]
    require(set(classes) == {"raw_native", "zip", "receipts", "other"}, "allocation classes differ")
    totals = {field: 0 for field in ("files", "logical_bytes", "allocated_bytes")}
    for row in classes.values():
        for field in totals:
            totals[field] += nonnegative_integer(row[field], "allocation class " + field)
    files = record["files"]
    require(len(files) == totals["files"] and len({item["path"].casefold() for item in files}) == len(files),
            "allocation file coverage differs")
    aggregate = {name: {field: 0 for field in totals} for name in classes}
    representative = {field: 0 for field in totals}
    for item in files:
        path = safe(item["path"])
        require(path.is_relative_to(ROOT) and item["class"] in classes, "allocation file outside owned root/class")
        aggregate[item["class"]]["files"] += 1
        if path.is_relative_to(native):
            representative["files"] += 1
        for field in ("logical_bytes", "allocated_bytes"):
            value = nonnegative_integer(item[field], "allocation file " + field)
            aggregate[item["class"]][field] += value
            if path.is_relative_to(native):
                representative[field] += value
    require(all(all(row[field] == classes[name][field] for field in totals) for name, row in aggregate.items())
            and representative == record["representative_native"] and representative["files"] > 0
            and canonical_digest(files) == record["metadata_snapshot_sha256"], "allocation aggregates/digest differ")
    return {"totals": totals, "classes": classes, "representative_native": representative}


def physical_reconciliation(reader, state, first_request):
    reconciliation = state["first_block_reconciliation"]
    before_ref = state["initial_allocation_measurement"]
    require(same_path(before_ref["path"], ROOT / "formal/coordinator/before.allocation.json"),
            "initial allocation receipt belongs to another coordinator")
    before = allocation_inventory(reader.json(before_ref), ROOT / "measure/hot/qual-baseline-w1/native")
    after = allocation_inventory(reader.json(reconciliation["measurement"]), ROOT / "measure/hot/matched-native")
    growth = {field: after["totals"][field] - before["totals"][field] for field in ("logical_bytes", "allocated_bytes")}
    require(growth == reconciliation["growth"], "recorded allocation growth differs from before/after receipts")
    logical = nonnegative_integer(first_request["comparison_logical_projection_bytes"], "logical projection")
    physical = nonnegative_integer(first_request["storage"]["projected_volume_bytes"]["D:"], "physical projection")
    require(0 <= growth["logical_bytes"] <= logical and 0 <= growth["allocated_bytes"] <= physical,
            "measured allocation growth exceeded sealed projection")
    return {"before": before_ref, "after": reconciliation["measurement"], "before_allocation": before,
            "after_allocation": after, "growth": growth, "logical_projection_bytes": logical,
            "physical_projection_bytes": physical, "within_sealed_projections": True,
            "scope": "Historical GetCompressedFileSizeW receipt reconciliation; excludes directory/volume metadata. Both receipt hashes are bound by coordinator state."}


def verify_native_archives(reader, archive, recovery_files, workers):
    """Hash D and E containers, then read every native member from E once."""
    require(archive["scheme"] == "two-deflate1-shards-full-readback/v1"
            and archive["mismatches"] == 0 and len(archive["shards"]) == 2,
            "complete two-shard native recovery required")
    recovered = {str(Path(item["source"]["path"])): item for item in recovery_files}
    seen, total_bytes, updates = set(), 0, {}
    for shard in archive["shards"]:
        original = shard["archive"]
        require(str(Path(original["path"])) in recovered, "native archive absent from independent recovery")
        copied = recovered[str(Path(original["path"]))]
        require(copied["source"] == original and copied["destination"]["sha256"] == original["sha256"],
                "independent native archive SHA binding differs")
        with zipfile.ZipFile(safe(copied["destination"]["path"])) as stored:
            names = stored.namelist()
            require(len(names) == len(set(names)) and set(names) == set(shard["files"]),
                    "native recovery member inventory differs")
            shard_bytes = 0
            for name, expected in shard["files"].items():
                relative_member(name)
                require(name not in seen and re.fullmatch("[a-f0-9]{64}", expected),
                        "duplicate/invalid native recovery member")
                seen.add(name)
                with stored.open(name) as stream:
                    if re.fullmatch(r"iterations/[0-9]{6}/attempt-[0-9]{6}/update/update.json", name):
                        require(stored.getinfo(name).file_size <= 16 * 1024**2, "oversized update receipt")
                        payload = stream.read()
                        digest = hashlib.sha256(payload).hexdigest()
                        updates[name] = update_comparison(json.loads(payload), workers)
                    else:
                        digest = hashlib.file_digest(stream, "sha256").hexdigest()
                require(digest == expected, "native recovery member SHA differs: " + name)
                shard_bytes += stored.getinfo(name).file_size
            require(shard_bytes == shard["source_bytes"], "native shard source size differs")
            total_bytes += shard_bytes
    require(len(seen) == archive["source_files"] and total_bytes == archive["source_bytes"],
            "native archive total coverage differs")
    return {"member_count": len(seen), "source_bytes": total_bytes,
            "member_inventory_sha256": canonical_digest(sorted(seen - {"exposure-inputs.json"})),
            "comparison_member_count": len(seen - {"exposure-inputs.json"}),
            "exposure_inputs_sidecar": "exposure-inputs.json" in seen, "updates": updates,
            "scope": "D and independent E container SHA verified; every E native member decompressed and SHA verified"}


def verify_transport(reader, plan, transport, case, devices):
    require(plan["schema"] == "training-speedup-independent-cold-copy-plan/v2"
            and transport["schema"] == "training-speedup-independent-cold-copy/v2"
            and plan["mode"] == transport["mode"] == "desktop-local"
            and plan["requires_full_destination_sha256_verification"] is True
            and transport["complete"] is True, "complete desktop independent recovery required")
    expected_root = COLD / case
    require(same_path(transport["destination_root"], expected_root)
            and plan["source_device"] == transport["source_device"]
            and plan["destination_device"] == transport["destination_device"]
            and plan["source_host"] == transport["source_host"]
            and plan["destination_host"].upper() == transport["destination_host"].upper(),
            "independent recovery device/host binding differs")
    devices.verify(plan["source_root"], expected_root, plan["source_device"], plan["destination_device"])
    require(len(plan["files"]) == len(transport["files"]) and plan["files"], "independent recovery coverage differs")
    destinations, sources = set(), set()
    for planned, copied in zip(plan["files"], transport["files"]):
        require(copied["source"] == planned["source"] and copied["bytes"] == planned["bytes"]
                and copied["verified"] is True and copied["destination"]["sha256"] == planned["source"]["sha256"],
                "independent recovery entry binding differs")
        destination = safe(copied["destination"]["path"])
        require(destination.is_relative_to(expected_root)
                and same_path(destination, expected_root / planned["relative_destination"]),
                "independent recovery destination escapes case")
        require(str(destination).casefold() not in destinations and planned["source"]["path"].casefold() not in sources,
                "duplicate independent recovery entry")
        destinations.add(str(destination).casefold())
        sources.add(planned["source"]["path"].casefold())
        reader.verify(planned["source"], planned["bytes"])
        reader.verify(copied["destination"], copied["bytes"])


def verify_late(reader, late, case, controller_pin, maintenance, devices, bundles):
    require(late["schema"] == "training-io-speedup-late-copy/v1" and late["complete"] is True
            and late["case"] == case, "complete case late recovery required")
    selected = [safe(controller_pin["path"])] + sorted(safe(p) for p in maintenance.rglob("*") if p.is_file())
    records = late["files"]
    require(len(records) == len(selected) and {str(Path(r["source"]["path"])) for r in records}
            == {str(p) for p in selected}, "late recovery omits or duplicates selected payload")
    cold = COLD / case / "late"
    devices.verify(ROOT, cold, late["source_device"], late["destination_device"])
    inventory = []
    for entry in records:
        source = safe(entry["source"]["path"])
        require(source.is_relative_to(ROOT) and entry["relative_path"] == source.relative_to(ROOT).as_posix(),
                "late source relative path differs")
        reader.verify(entry["source"], entry["bytes"])
        inventory.append({"relative_path": entry["relative_path"], "bytes": entry["bytes"], "sha256": entry["source"]["sha256"]})
    if case.endswith("baseline"):
        require(late["mode"] == "individual-files" and late["bundle"] is None, "baseline recovery mode differs")
        for entry in records:
            require(same_path(entry["destination"]["path"], cold / entry["relative_path"])
                    and entry["destination"]["sha256"] == entry["source"]["sha256"], "baseline late destination differs")
            reader.verify(entry["destination"], entry["bytes"])
    else:
        require(late["mode"] == "store-bundle", "candidate recovery mode differs")
        bundle = late["bundle"]
        require(bundle["schema"] == "training-recovery-bundle/v1" and bundle["complete"] is True
                and same_path(bundle["source_root"], ROOT)
                and same_path(bundle["bundle"]["path"], cold / "payload.zip")
                and bundle["inventory"] == sorted(inventory, key=lambda item: item["relative_path"])
                and bundle["file_count"] == len(records)
                and bundle["payload_bytes"] == sum(item["bytes"] for item in records)
                and bundle["verification"]["complete"] is True
                and bundle["verification"]["independent_destination_readback"] is True,
                "candidate late bundle coverage/binding differs")
        reader.verify(bundle["bundle"], bundle["bundle"]["bytes"])
        bundles.verify_bundle(bundle["bundle"]["path"], inventory,
                              expected_bundle_sha256=bundle["bundle"]["sha256"])
    return {"mode": late["mode"], "files": len(records), "payload_bytes": sum(item["bytes"] for item in records),
            "full_source_and_independent_destination_readback": True}


def read_case(reader, entry, sealed, devices, bundles):
    label = entry["label"]
    request = reader.json(sealed["request"])
    config = reader.json(request["config"])
    runtime = reader.json(request["runtime"])
    reader.verify(runtime["binary"])
    require(request["kind"] == "training" and same_path(request["root"], ROOT / "measure/hot" / label)
            and same_path(request["cold_root"], ROOT / "measure/cold" / label)
            and same_path(config["output_directory"], ROOT / "measure/hot/matched-native"),
            "request must belong to exact fresh case roots")
    require(len(config["iterations"]) == 162 and all(len(i["episodes"]) == 10 for i in config["iterations"]),
            "full 162 x 10 schedule required")
    report = reader.json(entry["report"])
    require(report["schema"] == "native-expanded-cpu-dispatch/v1" and report["complete"] is True
            and report["qualification"] is False and report["completed_updates"] == 162
            and report["completed_games"] == 1620 and report["request"] == sealed["request"]
            and len(report["fingerprint"]["iterations"]) == 162 and report["runtime"] == runtime
            and report["placement"] == request["placement"], "dispatch coverage/runtime/request differs")
    for iteration, fingerprint in zip(config["iterations"], report["fingerprint"]["iterations"]):
        require(len(fingerprint["trajectories"]) == len(iteration["episodes"])
                and re.fullmatch("[a-f0-9]{64}", fingerprint["checkpoint_bits"]), "full fingerprint coverage differs")
    require(reader.json(report["executed_config"]) == config, "executed formal configuration differs")
    native_result = reader.json(report["native_result"])
    require(native_result["complete"] is True and native_result["completed_iterations"] == 162
            and native_result["planned_iterations"] == 162 and len(native_result["iterations"]) == 162,
            "native result coverage differs")
    controller_pin = pin(ROOT / "controllers" / (label + ".json"))
    controller = reader.json(controller_pin)
    require(controller["complete"] is True and controller["action"] == "dispatch"
            and controller["request"] == sealed["request"] and controller["report"] == entry["report"]
            and controller["launcher"]["sha256"] == report["launcher_sha256"], "controller binding differs")
    reader.verify(controller["launcher"])
    for key in ("controller_source", "launcher_adapter"):
        reader.verify(controller[key])
    affinity = reader.json(controller["native_affinity_receipt"])
    require(affinity == controller["native_affinity_observation"] and affinity["verified"] is True
            and affinity["actual_mask"] == affinity["expected_mask"] == 21845
            and same_path(affinity["process_image"], runtime["binary"]["path"])
            and sum(1 << cpu for cpu in request["placement"]["cpu_affinity"]) == 21845,
            "actual native affinity differs")
    started_pin = next(item["source"] for item in reader.json(entry["transport"])["files"]
                       if same_path(item["source"]["path"], Path(request["root"]) / "started.json"))
    started = reader.json(started_pin)
    require(started["request"] == sealed["request"] and started["placement"] == request["placement"]
            and started["pid"] == affinity["pid"], "native affinity observation belongs to another process")
    execution = reader.json(report["execution"])
    require(execution["exit_code"] == 0 and execution["error"] is None, "native execution failed")
    inspection = reader.json(entry["inspect"])
    retention = reader.json(entry["retain"])
    transport = reader.json(entry["transport"])
    require(inspection["schema"] == "training-speedup-maintenance-inspect/v1" and inspection["complete"] is True
            and inspection["request"] == sealed["request"] and inspection["report"] == entry["report"]
            and retention["schema"] == "training-speedup-maintenance-retain/v1" and retention["complete"] is True
            and retention["request"] == sealed["request"] and retention["inspection"] == entry["inspect"]
            and retention["independent_cold_copy"] == entry["transport"]
            and transport["plan"] == inspection["recovery_copy_plan"], "maintenance receipt binding differs")
    recovery_plan = reader.json(inspection["recovery_copy_plan"])
    require(recovery_plan["report"] == entry["report"] and recovery_plan["request"] == sealed["request"],
            "recovery plan belongs to another case")
    verify_transport(reader, recovery_plan, transport, label, devices)
    archive = reader.json(report["archive"])
    require(archive["native_root"] == inspection["native_root"] == config["output_directory"], "native root differs")
    native_recovery = verify_native_archives(reader, archive, transport["files"], report["placement"]["preparation_workers"])
    updates = native_recovery["updates"]
    require(len(updates) == 162 and {int(name.split("/")[1]) for name in updates} == set(range(162)),
            "all 162 recovered update receipts required")
    archive_hashes = {name: sha for shard in archive["shards"] for name, sha in shard["files"].items()}
    if sealed["variant"] == "candidate":
        sidecar = report["exposure_inputs"]
        require(native_recovery["exposure_inputs_sidecar"] and same_path(sidecar["path"], Path(config["output_directory"]) / "exposure-inputs.json")
                and archive_hashes["exposure-inputs.json"] == sidecar["sha256"], "candidate sidecar archive binding differs")
    else:
        require(not native_recovery["exposure_inputs_sidecar"] and not report.get("exposure_inputs"),
                "baseline must have no candidate exposure sidecar")
    require(set(inspection["physical"]) == {"native", "cold", "dispatch", "maintenance"}, "inspection physical coverage differs")
    measured_physical = {name: physical_stats(stats) for name, stats in inspection["physical"].items()}
    require(measured_physical["native"]["files"] == archive["source_files"]
            and measured_physical["native"]["logical_bytes"] == archive["source_bytes"], "native physical receipt/archive coverage differs")
    exposure = reader.json(inspection["exposure"])
    gate = reader.json(inspection["embedding_gate"])
    reader.verify(inspection["final_checkpoint"])
    for key in ("decks", "t1"):
        reader.verify(inspection[key])
    for ref in inspection["maintenance_sources"].values():
        reader.verify(ref)
    reader.verify({key: exposure["rows"][key] for key in ("path", "sha256")})
    require(exposure["verdict"] == "pass" and exposure["rows"]["count"] == 1620
            and len(exposure["updates"]) == 162 and all(check["pass"] is True for check in exposure["checks"].values())
            and gate["pass"] is True and exposure["final_checkpoint_sha256"] == inspection["final_checkpoint"]["sha256"],
            "exposure/embedding/final-checkpoint acceptance failed")
    parity = {"exposure": copy.deepcopy(exposure), "embedding_gate": gate}
    parity["exposure"]["rows"].pop("path")
    kept = reader.json(retention["retained_manifest"])
    require(kept["native_root"] == inspection["native_root"] and kept["retained_updates"] == exposure["retained_updates"],
            "retained manifest differs")
    kept_root = Path(retention["retained_manifest"]["path"]).parent
    retained_sources, retained_targets = set(), set()
    for item in kept["files"]:
        source, target = Path(item["from"]), safe(item["to"])
        require(source.is_relative_to(Path(config["output_directory"])) and target.is_relative_to(kept_root)
                and source.relative_to(Path(config["output_directory"])) == target.relative_to(kept_root)
                and str(source).casefold() not in retained_sources and str(target).casefold() not in retained_targets,
                "retained mapping escapes or duplicates case")
        relative = source.relative_to(Path(config["output_directory"])).as_posix()
        require(archive_hashes.get(relative) == item["sha256"], "retained source SHA differs from full native archive")
        reader.verify({"path": str(target), "sha256": item["sha256"]})
        retained_sources.add(str(source).casefold()); retained_targets.add(str(target).casefold())
    reader.verify(retention["prune_log"])
    late = reader.json(entry["late_copy"])
    require(late["request"] == sealed["request"] and late["report"] == entry["report"] and late["retain"] == entry["retain"]
            and late["source_device"] == recovery_plan["source_device"]
            and late["destination_device"] == recovery_plan["destination_device"], "late receipt binding differs")
    late_recovery = verify_late(reader, late, label, controller_pin, Path(entry["inspect"]["path"]).parent, devices, bundles)
    reader.verify(entry["late_copy_cold_receipt"])
    require(entry["late_copy_cold_receipt"]["sha256"] == entry["late_copy"]["sha256"]
            and same_path(entry["late_copy_cold_receipt"]["path"], COLD / label / "late/late-copy-receipt.json"),
            "late self-receipt recovery differs")
    audit = seconds(entry["excluded_one_time_allocation_audit_seconds"], "excluded audit")
    raw = seconds(entry["coordinator_case_wall_seconds"], "raw case wall")
    matched = seconds(entry["matched_case_wall_seconds"], "matched case wall")
    require(abs(raw - matched - audit) < 1e-6 and matched > 0, "only one-time audit may be subtracted")
    row = {"label": label, "variant": sealed["variant"], "workers": report["placement"]["workers"],
           "preparation_workers": report["placement"]["preparation_workers"], "runtime": runtime,
           "completed_updates": 162, "completed_games": 1620, "report": entry["report"],
           "matched_case_wall_seconds": matched, "raw_case_wall_seconds": raw,
           "excluded_allocation_audit_seconds": audit,
           "dispatch_seconds": seconds(report["seconds"], "dispatch"),
           "child_seconds": seconds(execution["child_seconds"], "native child"),
           "inspect_seconds": seconds(inspection["seconds"], "inspect"),
           "exposure_seconds": seconds(inspection["timing"]["exposure_seconds"], "exposure"),
           "archive_seconds": seconds(archive["seconds"], "archive"),
           "copy_seconds": seconds(transport["seconds"], "copy"),
           "retain_seconds": seconds(retention["seconds"], "retain"),
           "late_copy_seconds": seconds(late["seconds"], "late copy"),
           "late_receipt_seconds": seconds(entry["late_receipt_metadata_seconds"], "late self-receipt"),
           "native_recovery": native_recovery, "late_recovery": late_recovery,
           "physical": measured_physical, "retained_physical": physical_stats(retention["retained_physical"]),
           "started_utc": entry["started_utc"], "maintenance_started_utc": entry["maintenance_started_utc"],
           "finished_utc": entry["finished_utc"], "timing_boundary": entry["timing_boundary"],
           "maintenance_reservation_observations": entry["maintenance_reservation_observations"],
           "artifact_timing": report.get("artifact_timing"), "archive_shard_timing": [s.get("timing") for s in archive["shards"]],
           "inspection_timing": inspection["timing"], "retention_timing": retention["timing"],
           "transport_timing": transport["timing"], "bundle_timing": late["bundle"]["timing"] if late["bundle"] else None}
    completed_phases = sum(row[key] for key in ("dispatch_seconds", "inspect_seconds", "copy_seconds", "retain_seconds", "late_copy_seconds"))
    require(completed_phases <= matched + .01 and row["child_seconds"] <= row["dispatch_seconds"] + .01
            and row["archive_seconds"] <= row["dispatch_seconds"] + .01
            and row["exposure_seconds"] <= row["inspect_seconds"] + .01, "nested/phase timers exceed total")
    row["completed_phase_seconds"] = completed_phases
    row["case_envelope_seconds"] = matched - completed_phases
    science = copy.deepcopy(config)
    for key in ("collection_workers", "preparation_workers", "output_directory"):
        science.pop(key, None)
    row["scientific_config_sha256"] = hashlib.sha256(json.dumps(science, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()
    return row, report["fingerprint"], parity


def compare_recovered_outputs(rows):
    recoveries = [row["native_recovery"] for row in rows]
    require(len({item["member_inventory_sha256"] for item in recoveries}) == 1
            and len({item["comparison_member_count"] for item in recoveries}) == 1,
            "native recovery member coverage differs beyond candidate exposure sidecar")
    update_names = set(recoveries[0]["updates"])
    require(all(set(item["updates"]) == update_names for item in recoveries), "recovered update coverage differs")
    for name in sorted(update_names):
        records = [item["updates"][name] for item in recoveries]
        require(len({record["scientific_sha256"] for record in records}) == 1, "update scientific output differs: " + name)
        prepared = {record["preparation_deterministic_sha256"] for record in records
                    if record["preparation_deterministic_sha256"] is not None}
        require(len(prepared) <= 1, "deterministic preparation output differs: " + name)
    return {"scientific_update_parity": True, "deterministic_preparation_parity_when_present": True,
            "update_count_per_case": len(update_names), "native_member_coverage_parity": True,
            "native_member_inventory_sha256": recoveries[0]["member_inventory_sha256"],
            "comparison_member_count": recoveries[0]["comparison_member_count"],
            "coverage_exception": "Only candidate exposure-inputs.json is excluded from cross-case member-name coverage; its exact SHA is bound to report.exposure_inputs and independently recovered.",
            "definition": "All update.json fields, recursively and with original paths/pins/numbers, except the enumerated timers and preparation placement fields reported per update. Unknown scientific fields remain compared. GAE profile phase labels remain compared. Deterministic preparation fields are compared among all prepared cases; serial placement has no preparation receipt. No path, float tolerance, loss or gauge normalization."}


def comparison(rows, field):
    baseline = statistics.mean(row[field] for row in rows if row["variant"] == "baseline")
    candidate = statistics.mean(row[field] for row in rows if row["variant"] == "candidate")
    require(baseline > 0 and candidate > 0, "comparison requires positive times: " + field)
    return {"baseline_mean_seconds": baseline, "candidate_mean_seconds": candidate,
            "speedup_ratio_of_means": baseline / candidate, "percent_time_reduction": 100 * (1 - candidate / baseline),
            "adjacent_paired_speedups": [rows[0][field] / rows[1][field], rows[3][field] / rows[2][field]]}


def retry_provenance(reader, plan, state):
    keys = ("helper", "qualification_plan", "qualification_state", "retry_staging_loader")
    if not any(key in plan for key in keys[1:]):
        return None
    require(all(key in plan for key in keys), "partial sealed retry provenance")
    refs = {key: plan[key] for key in keys}
    for ref in refs.values():
        require(ref in state["sources"], "sealed retry pin absent from execution source graph: " + ref["path"])
        reader.verify(ref)
    require(same_path(refs["qualification_plan"]["path"], ROOT / "qualification.plan.json"),
            "retry must bind original qualification plan")
    frozen = reader.json(refs["qualification_plan"])
    index = reader.json(refs["qualification_state"])
    labels = [f"qual-{variant}-w{workers}" for variant in ("baseline", "candidate") for workers in (1, 2, 4, 8)]
    require(frozen["stage"] == "qualify" and frozen["helper"] == refs["helper"]
            and [item["label"] for item in frozen["commands"]] == labels
            and index["complete"] is True and not index.get("error") and index["plan"] == refs["qualification_plan"]
            and [item["label"] for item in index["completed"]] == labels, "sealed qualification index binding differs")
    runner = index["runner_source"]
    require(runner in state["sources"] and same_path(runner["path"], BASE / "resume_qualifications_retry.py"),
            "qualification admission runner absent from execution source graph")
    reader.verify(runner)
    implementation = next((ref for ref in state["sources"] if same_path(ref["path"], ROOT / "run_comparison_retry.py")), None)
    require(implementation is not None, "retry coordinator implementation pin absent")
    reader.verify(implementation)
    for item, command in zip(index["completed"], frozen["commands"]):
        for ref in (item["controller"], item["report"], command["request"]):
            require(ref in state["sources"], "qualification child pin absent from execution source graph")
        controller, report, request = reader.json(item["controller"]), reader.json(item["report"]), reader.json(command["request"])
        require(controller["complete"] is True and controller["request"] == command["request"]
                and controller["report"] == item["report"] and report["request"] == command["request"],
                "qualification receipt index binding differs: " + item["label"])
        for ref in (request["config"], request["runtime"]):
            require(ref in state["sources"], "qualification input pin absent from execution source graph")
            reader.verify(ref)
        runtime = reader.json(request["runtime"])
        require(runtime["binary"] in state["sources"], "qualification binary pin absent from execution source graph")
        reader.verify(runtime["binary"])
    return {**refs, "coordinator_implementation": implementation, "admission_runner": runner,
            "scope": "All retry refs and nested qualification receipts/inputs verified against sealed pins and execution source graph. Original coordinator/helper bytes preserved; advisory admission remains outside case timing."}


def analyze(state_path):
    reader = Reader()
    state_pin = pin(state_path)
    state = reader.json(state_pin)
    reader.verify({"path": str(COLD / "formal-final-state.json"), "sha256": state_pin["sha256"]})
    require(state["schema"] == "training-speedup-desktop-formal-coordinator/v1" and state["complete"] is True
            and state["sequential_nonoverlapping"] is True and not state.get("error")
            and [entry["label"] for entry in state["completed"]] == LABELS, "four complete sequential ioABBA cases required")
    plan = reader.json(state["plan"])
    require(plan["stage"] == "seal" and [entry["label"] for entry in plan["commands"]] == LABELS,
            "sealed ioABBA plan required")
    require([entry["variant"] for entry in plan["commands"]] == ["baseline", "candidate", "candidate", "baseline"],
            "sealed variants differ from ioABBA order")
    for ref in state["sources"]:
        reader.verify(ref)
    retry_refs = retry_provenance(reader, plan, state)
    reader.verify(plan["python_pin"])
    require(pin(sys.executable)["sha256"] == plan["python_pin"]["sha256"], "pinned comparison Python required for analysis")
    devices_ref = next(ref for ref in state["sources"] if same_path(ref["path"], BASE / "helpers/desktop_devices.py"))
    bundle_ref = next(ref for ref in state["sources"] if same_path(ref["path"], ROOT / "candidate/python/tools/training_recovery_bundle_v1.py"))
    devices = reader.module(devices_ref, "analysis_desktop_devices")
    bundles = reader.module(bundle_ref, "analysis_frozen_recovery_bundle")
    rows, fingerprints, parities = [], [], []
    for entry, sealed in zip(state["completed"], plan["commands"]):
        row, fingerprint, parity = read_case(reader, entry, sealed, devices, bundles)
        rows.append(row); fingerprints.append(fingerprint); parities.append(parity)
    require(all(item == fingerprints[0] == state["canonical_fingerprint"] for item in fingerprints), "full fingerprint parity failed")
    require(all(item == parities[0] == state["maintenance_parity"] for item in parities), "exposure/gate parity failed")
    recovered_output_parity = compare_recovered_outputs(rows)
    require(len({row["report"]["sha256"] for row in rows}) == 4, "report reused across cases")
    require(len({row["scientific_config_sha256"] for row in rows}) == 1, "schedule/seeds/optimizer configuration differs")
    for variant in ("baseline", "candidate"):
        require(len({json.dumps(row["runtime"], sort_keys=True) for row in rows if row["variant"] == variant}) == 1,
                "variant runtime changed between cases")
    audit = seconds(state["first_block_reconciliation"]["seconds"], "one-time allocation audit")
    reconciliation = physical_reconciliation(reader, state, reader.json(plan["commands"][0]["request"]))
    native_allocation = reconciliation["after_allocation"]["representative_native"]
    native_inspection = rows[0]["physical"]["native"]
    require(native_allocation == {"files": native_inspection["files"], "logical_bytes": native_inspection["logical_bytes"],
                                  "allocated_bytes": native_inspection["allocated_file_bytes"]},
            "first-block allocation and inspection native receipts differ")
    require(abs(rows[0]["excluded_allocation_audit_seconds"] - audit) < 1e-6
            and all(row["excluded_allocation_audit_seconds"] == 0 for row in rows[1:]), "audit exclusion differs")
    expected_phases = [(label, phase) for label in LABELS for phase in ("dispatch", "inspect", "copy", "retain")]
    require([(p["case"], p["phase"]) for p in state["phases"]] == expected_phases
            and all(p["exit_code"] == 0 for p in state["phases"]), "coordinator phase completion differs")
    phase_seconds = sum(seconds(p["seconds"], "coordinator phase") for p in state["phases"])
    elapsed = (datetime.fromisoformat(state["finished_utc"]) - datetime.fromisoformat(state["started_utc"])).total_seconds()
    seconds(elapsed, "coordinator elapsed")
    require([w["case"] for w in state["queue_waits"]] == LABELS, "queue wait coverage differs")
    queue = sum(seconds(wait["seconds"], "canonical resource queue") for wait in state["queue_waits"])
    raw_sum = sum(row["raw_case_wall_seconds"] for row in rows)
    require(elapsed + .1 >= raw_sum + queue, "coordinator elapsed smaller than cases plus queue")
    result = {"schema": SCHEMA, "complete": True, "state": state_pin, "cases": rows,
              "completed_updates": 648, "completed_games": 6480,
              "fingerprint_sha256": hashlib.sha256(json.dumps(fingerprints[0], sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
              "full_fingerprint_parity": True, "exposure_and_embedding_gate_parity": True,
              "recovered_output_parity": recovered_output_parity, "physical_reconciliation": reconciliation,
              "retry_provenance": retry_refs,
              "comparisons": {field: comparison(rows, field) for field in FIELDS},
              "coordinator": {"started_utc": state["started_utc"], "finished_utc": state["finished_utc"],
                              "elapsed_seconds": elapsed, "raw_case_wall_sum_seconds": raw_sum,
                              "matched_case_wall_sum_seconds": sum(row["matched_case_wall_seconds"] for row in rows),
                              "canonical_queue_wait_seconds": queue, "one_time_allocation_audit_seconds": audit,
                              "outside_case_and_queue_seconds": elapsed - raw_sum - queue,
                              "recorded_subprocess_phase_seconds": phase_seconds},
              "maintenance_reservations": {"cases": [{"label": row["label"], "started_utc": row["started_utc"],
                                                       "maintenance_started_utc": row["maintenance_started_utc"],
                                                       "finished_utc": row["finished_utc"], "timing_boundary": row["timing_boundary"],
                                                       **row["maintenance_reservation_observations"]} for row in rows],
                                           "scope": "Before/after canonical-generation observations are diagnostics only. Report every higher generation without rejecting or excluding cases; they do not establish full maintenance isolation."},
              "accounting": "Primary matched case wall equals raw case wall minus only the first-case one-time allocation audit. It includes controller envelopes, late payload recovery, late receipt self-copy, final receipt SHA reads and canonical generation observation. Subsequent durable completed-state writes are outside case wall and included in coordinator elapsed. Archive/child/exposure timers are nested diagnostics and must not be added to totals. Coordinator queues and outside-case overhead are separate. Final coordinator-state cold copy and analysis are outside the completed coordinator clock.",
              "scope": "Combined package and qualified per-variant allocation comparison; two observed matched ABBA pairs. Every case retained. No outlier exclusion, individual-patch causality, confidence interval, playing-strength or holdout claim.",
              "inputs": sorted(reader.inputs.values(), key=lambda item: item["path"])}
    return result, devices


def save_bytes(path, payload):
    with path.open("xb") as stream:
        stream.write(payload); stream.flush(); os.fsync(stream.fileno())
    return pin(path)


def publish(result, devices, out, recovery_out):
    require(out.is_relative_to(BASE) and recovery_out.is_relative_to(COLD), "analysis must use fresh D/E roots")
    safe(out); safe(recovery_out)
    require(not out.exists() and not recovery_out.exists(), "preserve existing analysis or recovery attempt")
    source_device, destination_device = devices.device(out), devices.device(recovery_out)
    devices.independent(source_device, destination_device)
    result["analyzer"] = pin(Path(__file__).resolve())
    analysis_bytes = (json.dumps(result, indent=2, allow_nan=False) + "\n").encode()
    for path in (out, recovery_out):
        require(shutil.disk_usage(path.anchor).free >= 60 * 1024**3 + 2 * len(analysis_bytes) + 1024**2,
                "analysis publication would violate 60 GiB reserve")
    out.mkdir(parents=True); recovery_out.mkdir(parents=True)
    refs = [save_bytes(out / "analysis.json", analysis_bytes)]
    columns = ["label", "variant", "workers", *FIELDS, "completed_phase_seconds", "case_envelope_seconds", "excluded_allocation_audit_seconds"]
    with (out / "cases.csv").open("x", newline="", encoding="utf-8") as stream:
        writer = csv.DictWriter(stream, fieldnames=columns, extrasaction="ignore")
        writer.writeheader(); writer.writerows(result["cases"]); stream.flush(); os.fsync(stream.fileno())
    refs.append(pin(out / "cases.csv"))
    lines = ["# Matched training I/O comparison", "", result["scope"], "", "| Case | Matched wall s | Raw wall s | Dispatch s | Child s | Inspect s | Copy s | Retain s | Late copy s |", "|---|---:|---:|---:|---:|---:|---:|---:|---:|"]
    for row in result["cases"]:
        fields = ("matched_case_wall_seconds", "raw_case_wall_seconds", "dispatch_seconds", "child_seconds", "inspect_seconds", "copy_seconds", "retain_seconds", "late_copy_seconds")
        lines.append("| " + row["label"] + " | " + " | ".join(f"{row[key]:.3f}" for key in fields) + " |")
    lines += ["", "| Region | Baseline mean s | Candidate mean s | Ratio of means | Adjacent ratios |", "|---|---:|---:|---:|---:|"]
    for name, row in result["comparisons"].items():
        lines.append(f"| {name} | {row['baseline_mean_seconds']:.3f} | {row['candidate_mean_seconds']:.3f} | {row['speedup_ratio_of_means']:.6f} | " + ", ".join(f"{x:.6f}" for x in row["adjacent_paired_speedups"]) + " |")
    lines += ["", result["accounting"], "", "```json", json.dumps(result["coordinator"], indent=2), "```", ""]
    if result["retry_provenance"] is not None:
        lines += ["Retry qualification and coordinator provenance:", "", "```json",
                  json.dumps(result["retry_provenance"], indent=2), "```", ""]
    lines += [result["recovered_output_parity"]["definition"], "", result["recovered_output_parity"]["coverage_exception"], "",
              "Update timing/placement exclusions by case (all other fields compared):"]
    for row in result["cases"]:
        exclusions = sorted({field for record in row["native_recovery"]["updates"].values() for field in record["excluded_fields"]})
        lines.append("- " + row["label"] + ": " + ", ".join(exclusions))
    lines += ["", "Physical allocation from historical receipts:", "", "```json",
              json.dumps({"first_block": result["physical_reconciliation"],
                          "cases": [{"label": row["label"], "physical": row["physical"], "retained_physical": row["retained_physical"]} for row in result["cases"]]}, indent=2),
              "```", "", result["maintenance_reservations"]["scope"], "", "```json",
              json.dumps(result["maintenance_reservations"]["cases"], indent=2), "```", ""]
    refs.append(save_bytes(out / "summary.md", "\n".join(lines).encode()))
    copies = []
    for ref in refs:
        destination = recovery_out / Path(ref["path"]).name
        with safe(ref["path"]).open("rb") as original, destination.open("xb") as copied:
            shutil.copyfileobj(original, copied, 1024 * 1024); copied.flush(); os.fsync(copied.fileno())
        recovered = pin(destination)
        require(recovered["sha256"] == ref["sha256"], "analysis independent recovery SHA differs")
        copies.append({"source": ref, "destination": recovered, "verified": True})
    devices.verify(out, recovery_out, source_device, destination_device)
    receipt = {"schema": "training-io-speedups-analysis-recovery/v1", "complete": True,
               "source_device": source_device, "destination_device": destination_device,
               "files": copies, "full_readback": True, "durability": "Every output and independent E copy fsynced before exact SHA readback"}
    payload = (json.dumps(receipt, indent=2, allow_nan=False) + "\n").encode()
    local_receipt = save_bytes(out / "recovery.json", payload)
    cold_receipt = save_bytes(recovery_out / "recovery.json", payload)
    require(local_receipt["sha256"] == cold_receipt["sha256"], "analysis recovery receipt differs")
    return {"analysis": refs[0], "recovery_receipt": local_receipt, "independent_recovery_receipt": cold_receipt}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", type=Path, default=ROOT / "formal/coordinator/state.json")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--recovery-out", type=Path)
    args = parser.parse_args()
    state = safe(args.state)
    require(state.is_relative_to(ROOT / "formal") and state.name == "state.json", "fresh comparison coordinator state required")
    out = safe(args.out)
    recovery_out = safe(args.recovery_out or (COLD / "analysis" / out.relative_to(BASE)))
    require(out.is_relative_to(BASE) and recovery_out.is_relative_to(COLD)
            and not out.exists() and not recovery_out.exists(), "fresh nonoverwriting D/E analysis paths required")
    result, devices = analyze(state)
    print(json.dumps(publish(result, devices, out, recovery_out)))


if __name__ == "__main__":
    main()
