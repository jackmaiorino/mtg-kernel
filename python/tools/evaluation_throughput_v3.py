"""Repeated native timings plus separately verified full-panel recovery cost.

Forked integrity checks preserve v1/v2 artifacts and their pinned interpretation.
Recovery calibration binds a completed reference workload and placement. A new
target must use the same binary and fit that measured output-volume envelope;
its native timings and deterministic replay are always qualified separately.
"""


def _require(condition, message=None):
    """Launch integrity checks must survive Python optimization."""
    if not condition:
        if message is None:
            raise AssertionError()
        raise AssertionError(message())

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
    _require((choice["schema"] == "cpu-bo3-allocation/v3" and choice["plan"] == plan_pin))
    plan = read(checked(plan_pin))
    checked(binary)
    _require((choice["binary"] == binary))
    for dependency in choice["dependencies"]: checked(dependency)
    _require((set(choice["inventory"]) == {"desktop", "computehost", "runpod"}))
    for host, item in choice["inventory"].items():
        when = datetime.fromisoformat(item["checked_at"])
        _require((when.tzinfo and 0 <= (datetime.now(timezone.utc)-when).total_seconds() < 86400))
        checked(item["evidence"])
        _require((item["reason"] and isinstance(item["eligible"], bool)))
    expected_jobs = {j["id"]: j for j in plan["qualification_jobs"]}
    measured, observations, baseline = {}, {}, None
    for candidate in choice["candidates"]:
        report = read(checked(candidate["report"]))
        _require((len(report["jobs"]) == len(expected_jobs)))
        _require((report["matches"] == sum(len(j["command"]["matches"]) for j in expected_jobs.values())))
        actual, seen = {}, set()
        for host, allocation in report["allocation"].items():
            _require((choice["inventory"][host]["eligible"]))
            key = host, allocation["drive"], allocation["disk_serial"]
            _require((key in [tuple(s) for s in choice["eligible_storage"]]))
            worker = read(checked(report["workers"][host]))
            _require((worker["hostname"].upper() == HOSTS[host] and not worker["errors"]))
            _require((worker["workers"] == allocation["workers"]))
            measured.setdefault(key, set()).add(allocation["workers"])
        for job in report["jobs"]:
            _require((job["id"] in expected_jobs and job["id"] not in seen))
            seen.add(job["id"])
            request = read(checked(job["request"]))
            expected = expected_jobs[job["id"]]["command"]
            _require(({k:v for k,v in request.items() if k != "output_directory"} == {k:v for k,v in expected.items() if k != "output_directory"}))
            execution = read(checked(job["execution"]))
            _require((execution["exit_code"] == 0 and not execution["timeout"]))
            _require((execution["binary"] == binary and execution["request"] == job["request"]))
            _require((execution["host"] == job["host"] and execution["storage"] == report["allocation"][job["host"]]))
            _require((execution["native_binary"]["sha256"] == binary["sha256"]))
            _require((execution["native_request"]["sha256"] == job["request"]["sha256"]))
            recovery = read(checked(job["recovery"]))
            _require((recovery["mismatches"] == 0))
            folder = Path(job["output_directory"])
            completion = read(folder/"completion.json")
            _require((completion["matches"] == len(request["matches"]) == len(completion["match_sha256"])))
            games = decisions = 0
            for index, digest in enumerate(completion["match_sha256"]):
                path = folder/f"match-{index:06}.json"
                _require((pin(path)["sha256"] == digest))
                match = read(path)
                _require((match["match"] == request["matches"][index]))
                games += len(match["games"])
                decisions += match["decision_count"]
                actual[f"{job['id']}/{index}"] = digest
            _require((games == completion["natural_games"] and decisions == completion["decisions"]))
        _require((actual == report["fingerprints"]))
        _require((baseline is None or baseline == actual), lambda: ("hardware/storage/worker choice changes gameplay"))
        baseline = actual
        scale = plan["expected_matches"]/report["matches"]
        _require((all(report[field] >= 0 for field in ["staging_seconds", "execution_seconds", "recovery_seconds"])))
        _require((report["execution_seconds"] > 0))
        # Includes input placement and full-result recovery, not just native compute.
        _require((all(math.isfinite(report[field]) for field in ["staging_seconds", "execution_seconds", "recovery_seconds"])))
        observations.setdefault(candidate["allocation_id"], []).append((candidate, report, scale))
    _require((set(measured) == {tuple(row) for row in choice["eligible_storage"]}))
    _require((all(1 in counts and any(c > 1 for c in counts) for counts in measured.values())))
    _require((len({c["id"] for c in choice["candidates"]}) == len(choice["candidates"])))
    options, verified_recovery = [], {}
    for allocation_id, runs in observations.items():
        _require((len(runs) >= 2), lambda: ("each placement needs repeated timing"))
        _require((len({c["report"]["path"] for c, _, _ in runs}) == len(runs)))
        allocation = runs[0][1]["allocation"]
        _require((all(r["allocation"] == allocation for _, r, _ in runs)))
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
                _require((scale*sampled_bytes <= reference_match_bytes[host]), lambda: ("target host output estimate exceeds measured recovery volume"))
        remote_setup = choice["remote_setup_seconds"] if "computehost" in allocation else 0
        _require((math.isfinite(remote_setup) and remote_setup >= 0))
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
    _require((selected["projected_seconds"] == min(o["projected_seconds"] for o in options)))
    return selected


def dispatch_qualified(root, label, choice_path, plan_pin, remote, group_wall_seconds=900):
    """Supported v2 launch: no raw full-panel dispatch without this check."""
    from public_evaluation_dispatch_v1 import inventory
    from public_evaluation_dispatch_v2 import dispatch
    plan = read(checked(plan_pin))
    choice = read(choice_path)
    _require((choice.get("selected")), lambda: ("persist qualification before launch"))
    selected = require_choice(choice_path, plan_pin, plan["binary"])
    fresh = {host:inventory(host) for host in ("desktop", "computehost")}
    _require((all(s["active"] or choice["inventory"][h]["eligible"] for h,s in fresh.items())), lambda: ("availability expanded; qualify new eligible capacity"))
    _require((all(not fresh[h]["active"] for h in selected["allocation"])), lambda: ("preserve active owners"))
    _require((selected["projected_seconds"] <= plan["projection_cap_seconds"]))
    return dispatch(root, label, plan["binary"], plan["jobs"], selected["allocation"], remote, group_wall_seconds)
