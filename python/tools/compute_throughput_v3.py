"""Check complete, device-bound allocations for matched public training jobs."""
from datetime import datetime, timezone
from pathlib import Path

from compute_throughput_v1 import checked, nonnegative, read


def queued_makespan(placements, executions, projections):
    totals = {}
    jobs = list(placements)
    for i, job in enumerate(jobs):
        place = placements[job]
        device = (place["host"], place["gpu_uuid"])
        totals[device] = totals.get(device, 0.0) + projections[job]
        for other in jobs[i+1:]:
            second = placements[other]
            if device != (second["host"], second["gpu_uuid"]):
                continue
            x, y = executions[job], executions[other]
            if min(x["finished_unix"], y["finished_unix"]) > max(x["started_unix"], y["started_unix"]):
                raise ValueError("queued learners overlapped on the same GPU")
    return max(totals.values())


def require_allocation(path, binary_sha256, job_configs):
    revocation = Path(path).parent / 'qualification-revocation.json'
    if revocation.exists():
        raise ValueError('qualification revoked: ' + read(revocation)['reason'])
    plan = read(path)
    if plan.get("schema") != "public-training-allocation/v3":
        raise ValueError("unsupported allocation schema")
    if plan["jobs"] != job_configs:
        raise ValueError("allocation jobs differ from the requested campaign")
    inventory = plan["inventory"]
    if set(inventory) != {"desktop", "computehost", "runpod"}:
        raise ValueError("inspect all three placement options")
    eligible = set()
    now = datetime.now(timezone.utc)
    for host, status in inventory.items():
        when = datetime.fromisoformat(status["checked_at"])
        if when.tzinfo is None or not 0 <= (now-when).total_seconds() <= 86400:
            raise ValueError("stale placement inventory")
        checked(status["evidence"])
        if type(status["eligible"]) is not bool or not status.get("reason", "").strip():
            raise ValueError("missing host availability decision")
        ordinals = set()
        for device in status["devices"]:
            ordinal = device["ordinal"]
            if type(ordinal) is not int or not 0 <= ordinal < 16 or ordinal in ordinals:
                raise ValueError("invalid or duplicate inventory device")
            ordinals.add(ordinal)
            if type(device["eligible"]) is not bool or not device.get("reason", "").strip() or not device["uuid"].startswith("GPU-"):
                raise ValueError("missing device availability decision")
            if device["eligible"]:
                if not status["eligible"]:
                    raise ValueError("eligible device on unavailable host")
                eligible.add((host, ordinal, device["uuid"]))
        if status["eligible"] and not any(p[0] == host for p in eligible):
            raise ValueError("eligible host has no device")

    seen_devices, ids = set(), set()
    fingerprints = {job: None for job in job_configs}
    worker_counts = {job: set() for job in job_configs}
    qualified = []
    for candidate in plan["candidates"]:
        if candidate["id"] in ids:
            raise ValueError("duplicate allocation")
        ids.add(candidate["id"])
        group = read(checked(candidate["benchmark"]))
        if group["mode"] not in ["parallel", "sequential", "device_queues"] or set(group["jobs"]) != set(job_configs):
            raise ValueError("allocation benchmark does not cover all jobs")
        placements, projections, executions, counts = {}, [], {}, {}
        job_projections = {}
        for job, report_pin in group["jobs"].items():
            report = read(checked(report_pin))
            execution = read(checked(report["execution"]))
            request = read(checked(report["request"]))
            completion = read(checked(report["completion"]))
            config = read(checked(report["config"]))
            checked(report["binary"])
            if report["binary"]["sha256"] != binary_sha256 or execution["binary_sha256"] != binary_sha256:
                raise ValueError("allocation binary differs")
            if report["config"]["sha256"] != job_configs[job]["config_sha256"] or request["config"] != config:
                raise ValueError("allocation workload differs")
            if execution["request_sha256"] != report["request"]["sha256"] or request.get("resume") is not None:
                raise ValueError("benchmark request or initial parent differs")
            placement = report["placement"]
            device = (placement["host"], placement["gpu_ordinal"], placement["gpu_uuid"])
            workers = placement["workers"]
            if type(placement["gpu_ordinal"]) is not int or device not in eligible or type(workers) is not int or not 1 <= workers <= 64:
                raise ValueError("unavailable device or invalid collector count")
            if execution["placement"] != placement or execution["observed_gpu_uuid"] != placement["gpu_uuid"]:
                raise ValueError("native execution device differs from placement")
            if request.get("execution_gpu_ordinal", config["gpu_ordinal"]) != placement["gpu_ordinal"] or request.get("collector_workers", 1) != workers:
                raise ValueError("request ignores selected placement")
            if execution["exit_code"] != 0 or execution["timeout"]:
                raise ValueError("incomplete native execution")
            if completion["execution_gpu_ordinal"] != placement["gpu_ordinal"] or completion["collector_workers"] != workers:
                raise ValueError("completion ignores selected placement")
            count = completion["next_update"]
            if completion["first_update"] != 0 or not 3 <= count <= job_configs[job]["updates"] or request["stop_after"] != count:
                raise ValueError("benchmark must cover at least three complete initial updates")
            if len(config["updates"]) != job_configs[job]["updates"]:
                raise ValueError("planned update count differs")
            receipts = completion["receipts"]
            if len(receipts) != count or [r["update"] for r in receipts] != list(range(count)):
                raise ValueError("incomplete benchmark update coverage")
            expected = set()
            for update, receipt in enumerate(receipts):
                games = len(config["updates"][update])
                if receipt["episodes"] != games or receipt["natural_games"] != games:
                    raise ValueError("incomplete benchmark game coverage")
                if receipt["execution_gpu_ordinal"] != placement["gpu_ordinal"] or receipt["collector_workers"] != workers:
                    raise ValueError("receipt device or workers differ")
                expected.update(f"{update:04}/{name}" for name in ["checkpoint.json", "optimizer.json"] + [f"episode-{i:03}.json" for i in range(games)])
            output_root = Path(report["local_output_directory"]).resolve()
            if request["output_directory"] != report["source_output_directory"]:
                raise ValueError("recovery source differs from native request")
            if placement["host"] == "desktop":
                if Path(request["output_directory"]).resolve() != output_root:
                    raise ValueError("local outputs cannot be relabeled")
            else:
                recovery = read(checked(report["recovery"]))
                if recovery["source_output_directory"] != request["output_directory"]:
                    raise ValueError("remote recovery mapping differs")
                for name in expected:
                    if recovery["files"].get(name) != report["outputs"].get(name, {}).get("sha256"):
                        raise ValueError("remote recovered bytes differ")
            if Path(report["completion"]["path"]).resolve() != output_root/"completion.json" or set(report["outputs"]) != expected:
                raise ValueError("benchmark output coverage differs")
            fingerprint = []
            for name, item in sorted(report["outputs"].items()):
                if Path(item["path"]).resolve() != output_root/name:
                    raise ValueError("borrowed output from another execution")
                checked(item)
                fingerprint.append((name, item["sha256"]))
            if fingerprints[job] is not None and fingerprints[job] != fingerprint:
                raise ValueError("allocation changes saved learning outputs")
            fingerprints[job] = fingerprint
            elapsed = nonnegative(execution["seconds"])
            steady = sum(nonnegative(r["seconds"]) for r in receipts[1:])/(count-1)
            begin, end = nonnegative(execution["started_unix"]), nonnegative(execution["finished_unix"])
            if elapsed <= 0 or steady <= 0 or end <= begin:
                raise ValueError("invalid execution timing")
            projection = elapsed + (job_configs[job]["updates"]-count)*steady
            projections.append(projection)
            job_projections[job] = projection
            placements[job] = placement
            executions[job] = execution
            counts[job] = count
            seen_devices.add(device)
            worker_counts[job].add(workers)
        # Shared-host parallel throughput must actually have been measured
        # concurrently on distinct devices, not inferred from isolated runs.
        jobs = list(placements)
        if group["mode"] == "parallel":
            for i, first in enumerate(jobs):
                for second in jobs[i+1:]:
                    a, b = placements[first], placements[second]
                    if a["host"] != b["host"]:
                        continue
                    if a["gpu_uuid"] == b["gpu_uuid"]:
                        raise ValueError("parallel learners share one device")
                    x, y = executions[first], executions[second]
                    overlap = min(x["finished_unix"], y["finished_unix"])-max(x["started_unix"], y["started_unix"])
                    if overlap < .8*min(x["finished_unix"]-x["started_unix"],y["finished_unix"]-y["started_unix"]):
                        raise ValueError("shared-host parallel benchmark did not overlap")
        # Setup comes from the actual dispatch. Recovery grows with the number
        # of saved updates, so a tiny export is not priced as a full campaign.
        scale = max(job_configs[job]["updates"]/counts[job] for job in jobs)
        overhead = nonnegative(group["staging_seconds"]) + nonnegative(group["recovery_seconds"])*scale
        native_projection = queued_makespan(placements, executions, job_projections) if group["mode"] == "device_queues" else (max(projections) if group["mode"] == "parallel" else sum(projections))
        projected = native_projection + overhead
        qualified.append(dict(id=candidate["id"], mode=group["mode"], placements=placements, projected_seconds=projected))
    if seen_devices != eligible:
        raise ValueError("eligible device lacks a measured allocation")
    if any(1 not in counts or not any(w > 1 for w in counts) for counts in worker_counts.values()):
        raise ValueError("each job needs serial and parallel collection evidence")
    if not qualified:
        raise ValueError("no qualified allocation")
    fastest = min(qualified, key=lambda c: c["projected_seconds"])
    selected = next((c for c in qualified if c["id"] == plan["selected"]), None)
    if selected is None or selected["projected_seconds"] > fastest["projected_seconds"]:
        raise ValueError("selected allocation is slower than a qualified alternative")
    return selected
