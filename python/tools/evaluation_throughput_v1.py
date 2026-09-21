"""Require actual CPU/storage replay and measured allocation before a full panel."""
from datetime import datetime, timezone
from pathlib import Path
from public_evaluation_dispatch_v1 import HOSTS, read, pin, checked


def require_choice(choice_path, plan_pin, binary):
    choice = read(choice_path)
    assert choice["schema"] == "cpu-bo3-allocation/v1" and choice["plan"] == plan_pin
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
    measured, options, baseline = {}, [], None
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
        remote_setup = choice["remote_setup_seconds"] if "haleyspc" in report["allocation"] else 0
        projected = remote_setup+report["staging_seconds"]+scale*(report["execution_seconds"]+report["recovery_seconds"])
        options.append(dict(id=candidate["id"], allocation=report["allocation"], projected_seconds=projected))
    assert set(measured) == {tuple(row) for row in choice["eligible_storage"]}
    assert all(1 in counts and any(c > 1 for c in counts) for counts in measured.values())
    assert len({o["id"] for o in options}) == len(options)
    selected = next(o for o in options if o["id"] == choice["selected"])
    assert selected["projected_seconds"] == min(o["projected_seconds"] for o in options)
    return selected
