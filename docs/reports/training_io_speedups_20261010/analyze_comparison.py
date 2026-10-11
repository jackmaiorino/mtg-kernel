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


def verify_native_archives(reader, archive, recovery_files):
    """Hash D and E containers, then read every native member from E once."""
    require(archive["scheme"] == "two-deflate1-shards-full-readback/v1"
            and archive["mismatches"] == 0 and len(archive["shards"]) == 2,
            "complete two-shard native recovery required")
    recovered = {str(Path(item["source"]["path"])): item for item in recovery_files}
    seen, total_bytes = set(), 0
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
                    digest = hashlib.file_digest(stream, "sha256").hexdigest()
                require(digest == expected, "native recovery member SHA differs: " + name)
                shard_bytes += stored.getinfo(name).file_size
            require(shard_bytes == shard["source_bytes"], "native shard source size differs")
            total_bytes += shard_bytes
    require(len(seen) == archive["source_files"] and total_bytes == archive["source_bytes"],
            "native archive total coverage differs")
    return {"member_count": len(seen), "source_bytes": total_bytes,
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
    native_recovery = verify_native_archives(reader, archive, transport["files"])
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
    archive_hashes = {name: sha for shard in archive["shards"] for name, sha in shard["files"].items()}
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


def comparison(rows, field):
    baseline = statistics.mean(row[field] for row in rows if row["variant"] == "baseline")
    candidate = statistics.mean(row[field] for row in rows if row["variant"] == "candidate")
    require(baseline > 0 and candidate > 0, "comparison requires positive times: " + field)
    return {"baseline_mean_seconds": baseline, "candidate_mean_seconds": candidate,
            "speedup_ratio_of_means": baseline / candidate, "percent_time_reduction": 100 * (1 - candidate / baseline),
            "adjacent_paired_speedups": [rows[0][field] / rows[1][field], rows[3][field] / rows[2][field]]}


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
    require(len({row["report"]["sha256"] for row in rows}) == 4, "report reused across cases")
    require(len({row["scientific_config_sha256"] for row in rows}) == 1, "schedule/seeds/optimizer configuration differs")
    for variant in ("baseline", "candidate"):
        require(len({json.dumps(row["runtime"], sort_keys=True) for row in rows if row["variant"] == variant}) == 1,
                "variant runtime changed between cases")
    audit = seconds(state["first_block_reconciliation"]["seconds"], "one-time allocation audit")
    reader.verify(state["first_block_reconciliation"]["measurement"])
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
              "comparisons": {field: comparison(rows, field) for field in FIELDS},
              "coordinator": {"started_utc": state["started_utc"], "finished_utc": state["finished_utc"],
                              "elapsed_seconds": elapsed, "raw_case_wall_sum_seconds": raw_sum,
                              "matched_case_wall_sum_seconds": sum(row["matched_case_wall_seconds"] for row in rows),
                              "canonical_queue_wait_seconds": queue, "one_time_allocation_audit_seconds": audit,
                              "outside_case_and_queue_seconds": elapsed - raw_sum - queue,
                              "recorded_subprocess_phase_seconds": phase_seconds},
              "accounting": "Primary matched case wall equals raw case wall minus only the first-case one-time allocation audit. It includes controller envelopes, late payload recovery and late receipt self-copy. Archive/child/exposure timers are nested diagnostics and must not be added to totals. Coordinator queues and outside-case overhead are separate. Final coordinator-state cold copy and analysis are outside the completed coordinator clock.",
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
