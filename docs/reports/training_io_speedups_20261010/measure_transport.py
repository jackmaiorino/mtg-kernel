"""Qualify representative recovery transport only, never launch training.

The immutable clean4-02 source receipts select the entire original payload.
Fresh D:/E: output roots are mandatory. Without --execute, only receipt and code
pins are inspected; payload verification and copying remain unperformed.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path, PureWindowsPath
import platform
import re
import shutil
import stat
import subprocess
import time


GIB = 1024**3
RESERVE = 60 * GIB
OLD_ROOT = Path("D:/training-speedups-20261009/desktop")
D_ROOT = Path("D:/training-io-speedups-20261010/transport")
E_ROOT = Path("E:/training-io-speedups-20261010/transport")
CASE = "clean4-02-candidate"
CANDIDATE = "172ccb3c818a96f082f2cab44c570134a0b21ee3"
LATE_COUNT, LATE_BYTES = 1188, 151034184


def require(condition, message):
    if not condition:
        raise ValueError(message)


def safe(value):
    path = Path(value)
    require(path.is_absolute() and ".." not in path.parts, "absolute non-traversing path required")
    for entry in (path, *path.parents):
        try:
            metadata = entry.lstat()
        except FileNotFoundError:
            continue
        require(not stat.S_ISLNK(metadata.st_mode)
                and not (getattr(metadata, "st_file_attributes", 0) & 0x400),
                "reparse/link path refused: " + str(entry))
    return path


def relative(value):
    path = PureWindowsPath(value)
    require(path.parts and not path.is_absolute() and not path.drive and not path.root
            and ".." not in path.parts
            and all(re.fullmatch(r"[A-Za-z0-9_. -]+", part)
                    and not part.endswith((".", " ")) for part in path.parts),
            "unsafe recovery relative path")
    return Path(*path.parts)


def pin(path):
    path = safe(path)
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1, "regular single-link file required")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    after = path.lstat()
    require((metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns)
            == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns), "file changed while hashing")
    return {"path": str(path), "sha256": digest}


def checked(ref, size=None):
    require(isinstance(ref, dict) and set(ref) == {"path", "sha256"}
            and re.fullmatch("[a-f0-9]{64}", ref["sha256"]), "invalid source pin")
    require(pin(ref["path"]) == ref, "source SHA differs: " + ref["path"])
    path = Path(ref["path"])
    if size is not None:
        require(type(size) is int and size >= 0 and path.stat().st_size == size, "source size differs")
    return path


def read(ref):
    path = checked(ref)
    require(path.stat().st_size < 16 * 1024**2, "oversized receipt")
    payload = path.read_bytes()
    require(hashlib.sha256(payload).hexdigest() == ref["sha256"], "receipt changed while loading")
    return json.loads(payload)


def save(path, document):
    path = safe(path)
    with path.open("x", encoding="utf-8") as stream:
        json.dump(document, stream, indent=2, allow_nan=False)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    return pin(path)


def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def reserve(path, projected):
    require(shutil.disk_usage(path.anchor).free >= RESERVE + projected,
            "transport would violate 60 GiB free reserve: " + str(path.anchor))


def durable_copy(source, destination, expected, size):
    source, destination = safe(source), safe(destination)
    checked(expected, size)
    require(not destination.exists(), "preserve existing recovery destination")
    destination.parent.mkdir(parents=True, exist_ok=True)
    safe(destination.parent)
    with source.open("rb") as original, destination.open("xb") as output:
        shutil.copyfileobj(original, output, length=1024 * 1024)
        output.flush()
        os.fsync(output.fileno())
    actual = pin(destination)
    require(actual["sha256"] == expected["sha256"] and destination.stat().st_size == size,
            "independent destination SHA/size differs")
    return {"source": expected, "destination": actual, "bytes": size, "verified": True}


def copy_jobs(jobs, root, devices, source_device, destination_device):
    started = time.monotonic()
    devices.verify(OLD_ROOT, root, source_device, destination_device)
    reserve(root, sum(item["bytes"] for item in jobs) + len(jobs) * 4096 + 16 * 1024**2)
    require(not root.exists(), "preserve existing copy root")
    root.mkdir(parents=True)
    copied = []
    for item in jobs:
        copied.append(durable_copy(Path(item["source"]["path"]), root / item["relative_path"],
                                   item["source"], item["bytes"]))
    devices.verify(OLD_ROOT, root, source_device, destination_device)
    require(shutil.disk_usage(root).free >= RESERVE, "copy violated destination reserve")
    return {"complete": True, "files": copied, "file_count": len(copied),
            "payload_bytes": sum(item["bytes"] for item in copied),
            "seconds": time.monotonic() - started,
            "scope": "source SHA, per-file copy, fsync, full destination SHA and physical-device guards"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--late-receipt", type=Path,
                        default=OLD_ROOT / "formal-v4/coordinator" / (CASE + ".late-copy.json"))
    parser.add_argument("--late-receipt-sha256", required=True)
    parser.add_argument("--archive-copy-receipt", type=Path,
                        default=OLD_ROOT / "measure/maintenance" / CASE / "cold-copy-receipt.json")
    parser.add_argument("--archive-copy-receipt-sha256", required=True)
    parser.add_argument("--archive-mode", choices=("reuse", "copy"), default="reuse")
    parser.add_argument("--candidate-root", type=Path,
                        default=Path("C:/Users/Jack/.codex/worktrees/training-io-candidate-20261010/mtg-kernel"))
    parser.add_argument("--device-helper", type=Path,
                        default=Path(__file__).resolve().parent / "helpers/desktop_devices.py")
    parser.add_argument("--attempt", required=True)
    parser.add_argument("--other-host-transport", type=Path)
    parser.add_argument("--other-host-transport-sha256")
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    require(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]{0,80}", args.attempt), "invalid attempt label")
    require(os.name == "nt" and platform.node().upper() == "DESKTOP-DJ1C40R", "Jack desktop required")
    local, cold = safe(D_ROOT / args.attempt), safe(E_ROOT / args.attempt)
    require(not local.exists() and not cold.exists(), "preserve prior transport attempt")
    late_pin = {"path": str(args.late_receipt), "sha256": args.late_receipt_sha256}
    archive_pin = {"path": str(args.archive_copy_receipt), "sha256": args.archive_copy_receipt_sha256}
    late, archive_copy = read(late_pin), read(archive_pin)
    require(late["schema"] == "training-speedup-late-copy/v1" and late["complete"] is True
            and late["case"] == CASE, "complete immutable representative late receipt required")
    require(archive_copy["schema"] == "training-speedup-independent-cold-copy/v2"
            and archive_copy["complete"] is True and archive_copy["mode"] == "desktop-local",
            "complete independent representative archive receipt required")
    plan_pin = archive_copy["plan"]
    plan = read(plan_pin)
    require(plan["schema"] == "training-speedup-independent-cold-copy-plan/v2"
            and plan["mode"] == "desktop-local" and plan["request"] == late["request"]
            and plan["report"] == late["report"]
            and plan["requires_full_destination_sha256_verification"] is True
            and archive_copy["source_device"] == plan["source_device"]
            and archive_copy["destination_device"] == plan["destination_device"], "archive/late source binding differs")
    source_device, destination_device = plan["source_device"], plan["destination_device"]
    jobs, expected, names = [], [], set()
    for item in late["files"]:
        source = safe(item["source"]["path"])
        require(source.is_relative_to(OLD_ROOT), "late source outside immutable desktop root")
        name = relative(source.relative_to(OLD_ROOT).as_posix()).as_posix()
        require(name.casefold() not in names and type(item["bytes"]) is int and item["bytes"] >= 0
                and item["destination"]["sha256"] == item["source"]["sha256"], "invalid late inventory")
        names.add(name.casefold())
        jobs.append({"source": item["source"], "relative_path": name, "bytes": item["bytes"]})
        expected.append({"relative_path": name, "bytes": item["bytes"], "sha256": item["source"]["sha256"]})
    expected.sort(key=lambda item: item["relative_path"])
    require(len(jobs) == LATE_COUNT and sum(item["bytes"] for item in jobs) == LATE_BYTES,
            "representative late inventory must be exactly 1188 files / 151034184 bytes")
    require(len(plan["files"]) == len(archive_copy["files"]), "prior recovery inventory differs")
    archive_jobs, archive_names = [], set()
    old_destination = safe(archive_copy["destination_root"])
    require(old_destination == Path("E:/training-speedups-20261009") / CASE, "prior E recovery root differs")
    for planned, copied in zip(plan["files"], archive_copy["files"]):
        source = safe(planned["source"]["path"])
        name = relative(planned["relative_destination"]).as_posix()
        require(source.is_relative_to(OLD_ROOT / "measure") and name.casefold() not in archive_names
                and copied["source"] == planned["source"] and copied["bytes"] == planned["bytes"]
                and copied["verified"] is True
                and Path(copied["destination"]["path"]) == old_destination / name
                and copied["destination"]["sha256"] == planned["source"]["sha256"], "prior archive copy coverage differs")
        archive_names.add(name.casefold())
        archive_jobs.append({"source": planned["source"], "relative_path": name, "bytes": planned["bytes"]})
    zip_jobs = [item for item in archive_jobs if Path(item["relative_path"]).suffix.lower() == ".zip"]
    require(len(zip_jobs) == 2, "exactly two native recovery archive sources required")
    duration = archive_copy["seconds"]
    require(type(duration) in (int, float) and math.isfinite(duration) and duration > 0, "invalid prior archive duration")
    bundle_path = safe(args.candidate_root) / "python/tools/training_recovery_bundle_v1.py"
    commit = subprocess.run(["git", "-C", str(args.candidate_root), "rev-parse", "HEAD"],
                            check=True, capture_output=True, text=True).stdout.strip()
    require(commit == CANDIDATE, "candidate checkout differs from frozen transport module")
    code_pins = {"script": pin(Path(__file__).resolve()), "bundle_module": pin(bundle_path),
                 "device_helper": pin(args.device_helper)}
    other = None
    if args.other_host_transport is not None:
        require(args.other_host_transport_sha256 is not None, "other-host transport needs explicit SHA pin")
        other_pin = {"path": str(args.other_host_transport), "sha256": args.other_host_transport_sha256}
        other = read(other_pin)
        code_pins["other_host_transport"] = other_pin
    else:
        require(args.other_host_transport_sha256 is None, "other-host SHA has no source")
    archive_bytes = sum(item["bytes"] for item in archive_jobs)
    projected = {"D:": LATE_BYTES + 16 * 1024**2,
                 "E:": 2 * LATE_BYTES + len(jobs) * 8192 + 16 * 1024**2
                 + (archive_bytes if args.archive_mode == "copy" else 0)}
    require(sum(projected.values()) <= 16 * GIB, "transport projection exceeds bounded 16 GiB allocation")
    setup = {"schema": "training-io-transport-qualification/v1", "complete": False,
             "execution": args.execute, "attempt": args.attempt, "candidate_commit": commit,
             "sources": {"late_receipt": late_pin, "archive_copy_receipt": archive_pin,
                         "archive_copy_plan": plan_pin, **code_pins},
             "source_device": source_device, "destination_device": destination_device,
             "late_inventory": expected, "file_count": len(jobs), "payload_bytes": LATE_BYTES,
             "archive_mode": args.archive_mode, "native_archive_sources": zip_jobs,
             "projected_additional_bytes": projected, "free_reserve_bytes": RESERVE,
             "D_output_root": str(local), "E_recovery_root": str(cold),
             "started_utc": datetime.now(timezone.utc).isoformat(),
             "claims": "Representative transport qualification only; no training or production speedup claim."}
    if not args.execute:
        setup["next_action"] = "--execute verifies all payload pins before copying; no payload qualification performed"
        print(json.dumps(setup, allow_nan=False))
        return
    began = time.monotonic()
    devices = load_module("transport_devices", args.device_helper)
    bundles = load_module("transport_bundle", bundle_path)
    devices.verify(OLD_ROOT, cold, source_device, destination_device)
    reserve(local, projected["D:"])
    reserve(cold, projected["E:"])
    local.mkdir(parents=True)
    cold.mkdir(parents=True)
    setup_pin = save(local / "manifest.json", setup)
    result = dict(setup, manifest=setup_pin)
    try:
        phase = time.monotonic()
        for item in jobs:
            checked(item["source"], item["bytes"])
        result["late_source_preflight_seconds"] = time.monotonic() - phase
        if args.archive_mode == "reuse":
            phase = time.monotonic()
            devices.verify(OLD_ROOT, old_destination, source_device, destination_device)
            for planned, copied in zip(plan["files"], archive_copy["files"]):
                checked(planned["source"], planned["bytes"])
                checked(copied["destination"], copied["bytes"])
            devices.verify(OLD_ROOT, old_destination, source_device, destination_device)
            archive_result = {"complete": True, "mode": "compatible-verified-receipt-reuse",
                              "seconds": duration, "payload_bytes": archive_bytes,
                              "file_count": len(archive_jobs), "source_receipt": archive_pin,
                              "plan": plan_pin, "revalidation_seconds": time.monotonic() - phase,
                              "scope": "Prior full initial recovery wall, including both native ZIPs and metadata, source SHA, E copy/fsync, destination SHA and device guards."}
        else:
            archive_result = copy_jobs(archive_jobs, cold / "initial-recovery", devices,
                                       source_device, destination_device)
            archive_result["mode"] = "fresh-full-initial-recovery-copy"
        result["initial_recovery"] = archive_result
        baseline = copy_jobs(jobs, cold / "baseline", devices, source_device, destination_device)
        phase = time.monotonic()
        devices.verify(OLD_ROOT, cold / "candidate", source_device, destination_device)
        bundle = bundles.create_bundle(OLD_ROOT, [item["source"]["path"] for item in jobs],
                                       cold / "candidate/payload.zip", expected_inventory=expected,
                                       reserve_bytes=RESERVE + 16 * 1024**2)
        devices.verify(OLD_ROOT, cold / "candidate", source_device, destination_device)
        candidate = {"complete": True, "seconds": time.monotonic() - phase, "bundle": bundle,
                     "scope": "Complete bundle inventory, source streaming and final SHA, publication/fsync, full independent container/member SHA and physical-device guards."}
        require(bundle["inventory"] == expected and baseline["file_count"] == len(expected), "late payload inventory differs")
        result["baseline_late_copy"], result["candidate_late_copy"] = baseline, candidate
        for variant, measured in (("baseline", baseline), ("candidate", candidate)):
            phase = time.monotonic()
            receipt = save(local / (variant + ".late-copy.json"), measured)
            envelope = durable_copy(Path(receipt["path"]), cold / variant / "late-copy-receipt.json",
                                    receipt, Path(receipt["path"]).stat().st_size)
            measured["receipt_metadata_seconds"] = time.monotonic() - phase
            measured["receipt"] = receipt
            measured["recovery_receipt"] = envelope["destination"]
        phase = time.monotonic()
        reserve(local, LATE_BYTES + 16 * 1024**2)
        extraction = bundles.extract_bundle(Path(bundle["bundle"]["path"]), local / "extracted",
                                             expected, expected_bundle_sha256=bundle["bundle"]["sha256"])
        result["extraction_proof"] = {"complete": True, "receipt": save(local / "extraction-proof.json", extraction),
                                      "seconds": time.monotonic() - phase,
                                      "accounting": "Qualification proof only; expanded duplicate extraction is excluded from normal bundle transport projection."}
        for ref in (late_pin, archive_pin, plan_pin, *code_pins.values()):
            checked(ref)
        devices.verify(OLD_ROOT, cold, source_device, destination_device)
        require(shutil.disk_usage(local).free >= RESERVE and shutil.disk_usage(cold).free >= RESERVE,
                "transport attempt violated free-space reserve")
        for variant, measured in (("baseline", baseline), ("candidate", candidate)):
            result[variant] = {"desktop": {"complete": True,
                "projected_transport_seconds": archive_result["seconds"] + measured["seconds"]
                                               + measured["receipt_metadata_seconds"],
                "initial_recovery_seconds": archive_result["seconds"],
                "late_payload_seconds": measured["seconds"],
                "late_receipt_metadata_seconds": measured["receipt_metadata_seconds"],
                "accounting": "Representative full initial recovery plus complete late payload and receipt envelope; actual full comparison times remain authoritative."}}
            for host in ("computehost", "runpod"):
                row = other.get(variant, other).get(host) if other is not None else None
                if row is not None:
                    seconds = row.get("projected_transport_seconds")
                    require(row.get("complete") is True and type(seconds) in (int, float)
                            and math.isfinite(seconds) and seconds > 0, "invalid measured other-host transport")
                    result[variant][host] = dict(row, evidence=code_pins["other_host_transport"])
                else:
                    result[variant][host] = {"complete": False, "reason": "Separate measured host transport evidence required if this host is eligible."}
        result.update(complete=True, exact_late_payload_parity=True,
                      seconds=time.monotonic() - began, finished_utc=datetime.now(timezone.utc).isoformat())
        result_pin = save(local / "transport.json", result)
        durable_copy(Path(result_pin["path"]), cold / "transport.json", result_pin,
                     Path(result_pin["path"]).stat().st_size)
        print(json.dumps({"complete": True, "transport": result_pin, "recovery": pin(cold / "transport.json")}))
    except BaseException as error:
        result.update(complete=False, error=f"{type(error).__name__}: {error}",
                      seconds=time.monotonic() - began, finished_utc=datetime.now(timezone.utc).isoformat())
        failure = save(local / "failure.json", result)
        print(json.dumps({"complete": False, "failure": failure}))
        raise


if __name__ == "__main__":
    main()
