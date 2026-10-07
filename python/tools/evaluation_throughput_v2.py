"""Repeated native timings plus separately verified full-panel recovery cost.

Forked integrity checks preserve v1 artifacts and their pinned interpretation.
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
    _require((choice["schema"] == "cpu-bo3-allocation/v2" and choice["plan"] == plan_pin))
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
    options = []
    for allocation_id, runs in observations.items():
        _require((len(runs) >= 2), lambda: ("each placement needs repeated timing"))
        _require((len({c["report"]["path"] for c, _, _ in runs}) == len(runs)))
        allocation = runs[0][1]["allocation"]
        _require((all(r["allocation"] == allocation for _, r, _ in runs)))
        calibration = read(checked(choice["recovery_calibrations"][allocation_id]))
        _require((calibration["schema"] == "full-panel-recovery-calibration/v1"))
        reference_plan = read(checked(calibration["plan"]))
        _require((calibration["binary"] == binary == reference_plan["binary"]))
        _require((calibration["placements"] == {h:{k:v for k,v in a.items() if k not in ("workers", "job_weight")} for h,a in allocation.items()}))
        _require((calibration["allocation_weights"] == {h:a.get("job_weight",1) for h,a in allocation.items()}))
        _require((calibration["matches"] == reference_plan["expected_matches"] >= plan["expected_matches"]))
        _require((calibration["jobs"] == reference_plan["expected_jobs"] >= plan["expected_jobs"]))
        reference_game_cap = max(m["config"]["max_physical_games"]
            for job in reference_plan["jobs"] for m in job["command"]["matches"])
        for target_job in plan["jobs"]:
            _require((target_job["command"]["capture_decisions"] is False))
            _require((all(m["config"]["max_physical_games"] <= reference_game_cap
                for m in target_job["command"]["matches"])))
        source = read(checked(calibration["source_dispatch"]))
        recovered = read(checked(calibration["source_recovery"]))
        _require((source["matches"] == reference_plan["expected_matches"] and len(source["jobs"]) == reference_plan["expected_jobs"]))
        _require(({j["id"] for j in source["jobs"]} == {j["id"] for j in reference_plan["jobs"]}))
        _require((recovered["mismatches"] == 0 and recovered["files"] == calibration["files"]))
        _require((hashlib.sha256(json.dumps(recovered["hashes"],sort_keys=True).encode()).hexdigest() == calibration["source_fingerprint"]))
        samples = calibration["samples"]
        _require((len(samples) >= 2 and len({s["path"] for s in samples}) == len(samples)))
        recovery_times = []
        for sample in samples:
            receipt = read(checked(sample))
            _require((receipt["complete"] and receipt["mismatches"] == 0))
            _require((receipt["plan"] == calibration["plan"]))
            _require((receipt["source_fingerprint"] == calibration["source_fingerprint"]))
            _require((receipt["verified_files"] == calibration["files"]))
            _require((receipt["uncompressed_bytes"] == calibration["uncompressed_bytes"]))
            _require((receipt["stage_seconds"] and all(math.isfinite(v) and v >= 0 for v in receipt["stage_seconds"].values())))
            _require((math.isfinite(receipt["seconds"]) and receipt["seconds"] > 0))
            _require((sum(receipt["stage_seconds"].values()) <= receipt["seconds"] + .01))
            checked(receipt["archive"])
            with zipfile.ZipFile(receipt["archive"]["path"]) as archive:
                reference_match_bytes = sum(i.file_size for i in archive.infolist() if "/outputs/match-" in i.filename)
            # Current target samples, not old outcome data, set its volume
            # estimate. This is a throughput estimate, not a worst-case bound
            # on all possible future game lengths. Production must be monitored.
            for _, report, scale in runs:
                sampled_bytes = sum((Path(job["output_directory"])/f"match-{i:06}.json").stat().st_size
                    for job in report["jobs"] for i in range(len(expected_jobs[job["id"]]["command"]["matches"])))
                _require((scale*sampled_bytes <= reference_match_bytes), lambda: ("target output estimate exceeds measured recovery volume"))
            recovery_times.append(receipt["seconds"])
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
