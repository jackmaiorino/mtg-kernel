"""Repeated native timings plus separately verified full-panel recovery cost.

Forked integrity checks preserve v1/v2 artifacts and their pinned interpretation.
Recovery calibration binds a completed reference workload and placement. A new
target must use the same binary and fit that measured output-volume envelope;
its native timings and deterministic replay are always qualified separately.
"""
from datetime import datetime, timezone
from pathlib import Path
import math
import statistics
import hashlib
import json
import zipfile
from public_evaluation_dispatch_v1 import HOSTS, read, pin, checked


def require_choice(choice_path, plan_pin, binary):
    choice = read(choice_path)
    assert choice["schema"] == "cpu-bo3-allocation/v3" and choice["plan"] == plan_pin
    plan = read(checked(plan_pin))
    checked(binary)
    assert choice["binary"] == binary
    for dependency in choice["dependencies"]: checked(dependency)
    assert set(choice["inventory"]) == {"jack", "haleyspc", "runpod"}
    for host, item in choice["inventory"].items():
        when = datetime.fromisoformat(item["checked_at"])
        assert when.tzinfo and 0 <= (datetime.now(timezone.utc)-when).total_seconds() < 86400
        checked(item["evidence"])
        assert item["reason"] and isinstance(item["eligible"], bool)
    expected_jobs = {j["id"]: j for j in plan["qualification_jobs"]}
    measured, observations, baseline = {}, {}, None
    for candidate in choice["candidates"]:
        report = read(checked(candidate["report"]))
        assert len(report["jobs"]) == len(expected_jobs)
        assert report["matches"] == sum(len(j["command"]["matches"]) for j in expected_jobs.values())
        actual, seen = {}, set()
        for host, allocation in report["allocation"].items():
            assert choice["inventory"][host]["eligible"]
            key = host, allocation["drive"], allocation["disk_serial"]
            assert key in [tuple(s) for s in choice["eligible_storage"]]
            worker = read(checked(report["workers"][host]))
            assert worker["hostname"].upper() == HOSTS[host] and not worker["errors"]
            assert worker["workers"] == allocation["workers"]
            measured.setdefault(key, set()).add(allocation["workers"])
        for job in report["jobs"]:
            assert job["id"] in expected_jobs and job["id"] not in seen
            seen.add(job["id"])
            request = read(checked(job["request"]))
            expected = expected_jobs[job["id"]]["command"]
            assert {k:v for k,v in request.items() if k != "output_directory"} == {k:v for k,v in expected.items() if k != "output_directory"}
            execution = read(checked(job["execution"]))
            assert execution["exit_code"] == 0 and not execution["timeout"]
            assert execution["binary"] == binary and execution["request"] == job["request"]
            assert execution["host"] == job["host"] and execution["storage"] == report["allocation"][job["host"]]
            assert execution["native_binary"]["sha256"] == binary["sha256"]
            assert execution["native_request"]["sha256"] == job["request"]["sha256"]
            recovery = read(checked(job["recovery"]))
            assert recovery["mismatches"] == 0
            folder = Path(job["output_directory"])
            completion = read(folder/"completion.json")
            assert completion["matches"] == len(request["matches"]) == len(completion["match_sha256"])
            games = decisions = 0
            for index, digest in enumerate(completion["match_sha256"]):
                path = folder/f"match-{index:06}.json"
                assert pin(path)["sha256"] == digest
                match = read(path)
                assert match["match"] == request["matches"][index]
                games += len(match["games"])
                decisions += match["decision_count"]
                actual[f"{job['id']}/{index}"] = digest
            assert games == completion["natural_games"] and decisions == completion["decisions"]
        assert actual == report["fingerprints"]
        assert baseline is None or baseline == actual, "hardware/storage/worker choice changes gameplay"
        baseline = actual
        scale = plan["expected_matches"]/report["matches"]
        assert all(report[field] >= 0 for field in ["staging_seconds", "execution_seconds", "recovery_seconds"])
        assert report["execution_seconds"] > 0
        # Includes input placement and full-result recovery, not just native compute.
        assert all(math.isfinite(report[field]) for field in ["staging_seconds", "execution_seconds", "recovery_seconds"])
        observations.setdefault(candidate["allocation_id"], []).append((candidate, report, scale))
    assert set(measured) == {tuple(row) for row in choice["eligible_storage"]}
    assert all(1 in counts and any(c > 1 for c in counts) for counts in measured.values())
    assert len({c["id"] for c in choice["candidates"]}) == len(choice["candidates"])
    options, verified_recovery = [], {}
    for allocation_id, runs in observations.items():
        assert len(runs) >= 2, "each placement needs repeated timing"
        assert len({c["report"]["path"] for c, _, _ in runs}) == len(runs)
        allocation = runs[0][1]["allocation"]
        assert all(r["allocation"] == allocation for _, r, _ in runs)
        from evaluation_recovery_fixture_v1 import validate_calibration
        calibration_pin = choice["recovery_calibrations"][allocation_id]
        cache_key = json.dumps([calibration_pin, {h:{k:v for k,v in a.items() if k != 'workers'}
                                                for h,a in allocation.items()}], sort_keys=True)
        if cache_key not in verified_recovery:
            verified_recovery[cache_key] = validate_calibration(calibration_pin, allocation, plan, binary)
        samples, recovery_times, reference_match_bytes = verified_recovery[cache_key]
        for _, report, scale in runs:
            for host in allocation:
                sampled_bytes = sum((Path(job["output_directory"])/f"match-{i:06}.json").stat().st_size
                    for job in report["jobs"] if job['host'] == host
                    for i in range(len(expected_jobs[job["id"]]["command"]["matches"])))
                assert scale*sampled_bytes <= reference_match_bytes[host], "target host output estimate exceeds measured recovery volume"
        remote_setup = choice["remote_setup_seconds"] if "haleyspc" in allocation else 0
        assert math.isfinite(remote_setup) and remote_setup >= 0
        native_projection = statistics.median(scale*r["execution_seconds"] for _,r,scale in runs)
        # Full-output recovery is measured directly, not extrapolated per match.
        recovery = max(recovery_times)
        projected = remote_setup + statistics.median(r["staging_seconds"] for _,r,_ in runs) + native_projection + recovery
        options.append(dict(id=allocation_id, allocation=allocation, projected_seconds=projected,
                            scaled_execution_seconds=native_projection, full_recovery_seconds=recovery,
                            timing_repeats=len(runs), recovery_repeats=len(samples)))
    if choice.get("selected") is None:
        # Qualification can ask for the recomputed minimum; launch requires a
        # recorded selected id and checks it again below in dispatch_qualified.
        return min(options, key=lambda o:o["projected_seconds"])
    selected = next(o for o in options if o["id"] == choice["selected"])
    assert selected["projected_seconds"] == min(o["projected_seconds"] for o in options)
    return selected


def dispatch_qualified(root, label, choice_path, plan_pin, remote, group_wall_seconds=900):
    """Supported v2 launch: no raw full-panel dispatch without this check."""
    from public_evaluation_dispatch_v1 import inventory
    from public_evaluation_dispatch_v2 import dispatch
    plan = read(checked(plan_pin))
    choice = read(choice_path)
    assert choice.get("selected"), "persist qualification before launch"
    selected = require_choice(choice_path, plan_pin, plan["binary"])
    fresh = {host:inventory(host) for host in ("jack", "haleyspc")}
    assert all(s["active"] or choice["inventory"][h]["eligible"] for h,s in fresh.items()), "availability expanded; qualify new eligible capacity"
    assert all(not fresh[h]["active"] for h in selected["allocation"]), "preserve active owners"
    assert selected["projected_seconds"] <= plan["projection_cap_seconds"]
    return dispatch(root, label, plan["binary"], plan["jobs"], selected["allocation"], remote, group_wall_seconds)
