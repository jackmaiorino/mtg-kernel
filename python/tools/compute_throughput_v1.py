"""Require a measured allocation before substantial public-feature training.

This is a launch check, not a security boundary against an agent that edits or
bypasses it. Existing frozen runs use their original launchers unchanged.
"""
import hashlib
import json
import math
from datetime import datetime, timezone
from pathlib import Path


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def checked(pin):
    path = Path(pin["path"])
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    if digest != pin["sha256"]:
        raise ValueError(f"compute evidence changed: {path}")
    return path


def nonnegative(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
        raise ValueError("invalid compute timing")
    return value


def require_choice(path, binary_sha256, config_sha256, planned_updates):
    """Validate measured evidence and return the selected host/worker count.

    Each candidate points to a completed benchmark with the same binary,
    full training config, first three or more updates and exact saved outputs.
    The report binds the native execution, completion and every optimizer,
    checkpoint and trajectory. Timings come from those execution receipts.
    """
    plan = read(path)  # Absence deliberately prevents a substantial launch.
    if plan.get("schema") != "public-training-compute-choice/v1":
        raise ValueError("unsupported compute choice")
    if plan.get("planned_updates") != planned_updates:
        raise ValueError("compute choice covers a different run length")
    inventory = plan["inventory"]
    if set(inventory) != {"jack", "haleyspc", "runpod"}:
        raise ValueError("inspect Jack's PC, HaleysPC and RunPod before placement")
    now = datetime.now(timezone.utc)
    for host, status in inventory.items():
        checked_at = datetime.fromisoformat(status["checked_at"])
        if checked_at.tzinfo is None or not 0 <= (now-checked_at).total_seconds() <= 86400:
            raise ValueError(f"refresh resource inventory: {host}")
        if not isinstance(status["eligible"], bool) or not status.get("reason", "").strip():
            raise ValueError(f"record availability and resource decision: {host}")

    candidates = []
    fingerprints = []
    identifiers = set()
    for candidate in plan["candidates"]:
        if candidate["id"] in identifiers:
            raise ValueError("duplicate compute candidate")
        identifiers.add(candidate["id"])
        host, workers = candidate["host"], candidate["workers"]
        if host not in inventory or not inventory[host]["eligible"]:
            raise ValueError("benchmark host is not currently eligible")
        if type(workers) is not int or not 1 <= workers <= 64:
            raise ValueError("invalid collector count")
        report = read(checked(candidate["benchmark"]))
        config_path = checked(report["config"])
        if report["config"]["sha256"] != config_sha256:
            raise ValueError("benchmark workload differs from training config")
        config = read(config_path)
        execution = read(checked(report["execution"]))
        checked(execution["binary"])
        if execution["binary"]["sha256"] != binary_sha256:
            raise ValueError("requalify the changed training binary")
        if execution["exit_code"] != 0 or execution["timeout"]:
            raise ValueError("incomplete benchmark cannot qualify compute")
        request = read(checked(execution["request"]))
        if request["config"] != config or request.get("resume") is not None:
            raise ValueError("benchmark must start from the pinned parent/config")
        if request.get("collector_workers", 1) != workers or report["host"] != host:
            raise ValueError("benchmark worker or host binding differs")
        completion = read(checked(report["completion"]))
        output_root = Path(request["output_directory"]).resolve()
        if Path(report["completion"]["path"]).resolve() != output_root/"completion.json":
            raise ValueError("completion is not from the measured execution")
        count = completion["next_update"]
        if completion["first_update"] != 0 or not 3 <= count <= planned_updates or request["stop_after"] != count:
            raise ValueError("benchmark needs at least three complete initial updates")
        receipts = completion["receipts"]
        if len(receipts) != count or [r["update"] for r in receipts] != list(range(count)):
            raise ValueError("benchmark update coverage is incomplete")
        expected_names = set()
        for update, receipt in enumerate(receipts):
            games = len(config["updates"][update])
            if receipt["episodes"] != games or receipt["natural_games"] != games:
                raise ValueError("benchmark game coverage is incomplete")
            expected_names.update(f"{update:04}/{name}" for name in
                                  ["checkpoint.json", "optimizer.json"] + [f"episode-{i:03}.json" for i in range(games)])
        if set(report["outputs"]) != expected_names:
            raise ValueError("benchmark must bind every completed training output")
        fingerprint = []
        for name, pin in sorted(report["outputs"].items()):
            if Path(pin["path"]).resolve() != output_root/name:
                raise ValueError("output is not from the measured execution")
            checked(pin)
            fingerprint.append((name, pin["sha256"]))
        fingerprints.append(fingerprint)
        elapsed = nonnegative(execution["seconds"])
        steady = sum(nonnegative(r["seconds"]) for r in receipts[1:]) / (count-1)
        if elapsed <= 0 or steady <= 0:
            raise ValueError("benchmark timings must be positive")
        overhead = sum(nonnegative(candidate[key]) for key in
                       ["setup_seconds", "transfer_seconds", "recovery_seconds"])
        projected = overhead + elapsed + (planned_updates-count)*steady
        candidates.append(dict(id=candidate["id"], host=host, workers=workers,
                               projected_seconds=projected))

    if not candidates or not any(c["workers"] == 1 for c in candidates) or not any(c["workers"] > 1 for c in candidates):
        raise ValueError("a serial timing alone is insufficient; qualify parallel collection")
    if any(f != fingerprints[0] for f in fingerprints[1:]):
        raise ValueError("serial/parallel saved training outputs are not byte-identical")
    for host, status in inventory.items():
        if status["eligible"] and not any(c["host"] == host for c in candidates):
            raise ValueError(f"eligible placement has no throughput measurement: {host}")
    fastest = min(candidates, key=lambda c: c["projected_seconds"])
    chosen = next((c for c in candidates if c["id"] == plan["selected"]), None)
    if chosen is None or chosen["projected_seconds"] > fastest["projected_seconds"]:
        raise ValueError("selected allocation is slower than a qualified eligible alternative")
    return chosen
