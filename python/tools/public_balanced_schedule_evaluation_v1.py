"""Evaluate complete replica panels as available; interpret only after both finish."""
import argparse
import copy
from pathlib import Path

from public_training_dispatch_v2 import read, write, pin, checked
from public_evaluation_dispatch_v1 import inventory, prepare_remote
from evaluation_throughput_v2 import require_choice, dispatch_qualified
from public_balanced_schedule_analysis_v1 import ARMS, bound_opponent, analyze


def prepare(root, experiment, only_replica=None):
    top = read(experiment / "manifest.json")
    assert top["schema"] == "two-replica-public-balanced-schedule/v1"
    assert top["training_games"] == 8000 and top["evaluation_bo3"] == 6144
    replicas, jobs, samples, assets = [], [], [], {}
    binary = None
    def add(item):
        checked(item)
        assets[item["path"]] = item
    for pilot_pin in top["replicas"]:
        pilot = checked(pilot_pin).parent
        manifest = read(checked(pilot_pin))
        assert manifest["schema"] == "matched-public-balanced-schedule/v1"
        if only_replica is not None and manifest["replica"] != only_replica:
            continue
        training = read(pilot / "training-audit.json")
        assert training["complete"] and training["full_natural_games"] == 4000
        launch = read(pilot / "training-launch.json")
        for name in ["analysis", "bootstrap_implementation"]:
            checked(launch[name])
        assert launch["analysis"] == pin(Path(__file__).with_name("public_balanced_schedule_analysis_v1.py"))
        opponent = bound_opponent(pilot, manifest)
        endpoints = dict(g115=dict(kind="legacy", source=manifest["source"], v3_forced_actions=False))
        for arm in ARMS[1:]:
            final = training["arms"][arm]
            assert final["updates"] == 200 and final["natural_games"] == 2000
            endpoints[arm] = dict(kind="public_checkpoint", config=manifest["training_configs"][arm], checkpoint=final["checkpoint"])
            for item in [manifest["training_configs"][arm], final["checkpoint"], final["optimizer"]]:
                add(item)
        descriptor = opponent["source"]["play_import"]
        add(descriptor)
        for item in read(checked(descriptor)).values():
            if isinstance(item, dict) and "path" in item and "sha256" in item:
                add(item)
        assert binary is None or binary == manifest["evaluation_binary"]
        binary = manifest["evaluation_binary"]
        own_labels = sorted({case["own"] for job in manifest["jobs"] for case in job["cases"]})
        for template in manifest["jobs"]:
            seat = template["candidate_seat"]
            own = template["cases"][0]["own"]
            # One fixed case from each endpoint/own-deck/seat/replica, selected
            # by schedule only. Rotating opponent indices covers all eight decks.
            index = 8 * ((own_labels.index(own) + seat) % 8) + manifest["replica"] - 1
            for arm in ARMS:
                command = read(checked(template["template"]))
                command["sources"][seat] = endpoints[arm]
                command["sources"][1-seat] = opponent
                job = dict(id=f"r{manifest['replica']}-{arm}-{template['label']}", arm=arm,
                    label=template["label"], replica=manifest["replica"], command=command)
                jobs.append(job)
                sample = copy.deepcopy(job)
                sample["command"]["matches"] = [sample["command"]["matches"][index]]
                samples.append(sample)
        replicas.append(dict(replica=manifest["replica"], pilot=pilot_pin,
            training=pin(pilot / "training-audit.json"), binding=pin(pilot / "evaluation-binding.json"), endpoints=endpoints))
    expected_jobs, expected_matches = 48 * len(replicas), 3072 * len(replicas)
    assert len(replicas) in [1, 2] and len(jobs) == len(samples) == expected_jobs
    assert sum(len(job["command"]["matches"]) for job in jobs) == expected_matches
    # Interleave replicas/endpoints to avoid assigning one condition wholesale
    # to the slower host under weighted deterministic dispatch.
    jobs.sort(key=lambda j: (j["label"], j["arm"], j["replica"]))
    samples.sort(key=lambda j: (j["label"], j["arm"], j["replica"]))
    root.mkdir()
    write(root / "plan.json", dict(experiment=pin(experiment / "manifest.json"), replicas=replicas,
        binary=binary, jobs=jobs, qualification_jobs=samples, expected_matches=expected_matches, expected_jobs=expected_jobs,
        runner=pin(__file__), analysis=pin(Path(__file__).with_name("public_balanced_schedule_analysis_v1.py")),
        bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        full_group_wall_cap_seconds=1800, projection_cap_seconds=1800,
        qualification_matches=len(samples), no_prefix_selection=True, no_paid_compute=True,
        analysis_waits_for_both_replicas=True))
    remote = prepare_remote(root, list(assets.values()))
    print(dict(prepared=str(root), matches=expected_matches, qualification_matches=len(samples), staging_seconds=remote["seconds"]), flush=True)


def run(root, compute=None):
    plan = read(root / "plan.json")
    for name in ["runner", "analysis", "bootstrap_implementation"]:
        checked(plan[name])
    compute = compute or root
    qualification = read(compute / "qualification.json")
    selected = require_choice(compute / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    assert qualification["complete"] and selected == qualification["selected"]
    assert selected["projected_seconds"] < plan["projection_cap_seconds"]
    choice = read(compute / "compute-choice.json")
    fresh = {host: inventory(host) for host in ["desktop", "computehost"]}
    assert all(snapshot["active"] or choice["inventory"][host]["eligible"] for host, snapshot in fresh.items()), "availability expanded: qualify newly available hosts before launch"
    write(root / "launch.json", dict(plan=pin(root / "plan.json"), choice=pin(compute / "compute-choice.json"),
        qualification=pin(compute / "qualification.json"), selected=selected, current_inventory=fresh))
    result_pin = dispatch_qualified(root, "full-panel", compute / "compute-choice.json", pin(root / "plan.json"),
                          read(root / "remote-staging.json"), plan["full_group_wall_cap_seconds"])
    result = read(checked(result_pin))
    assert result["matches"] == plan["expected_matches"] and len(result["jobs"]) == plan["expected_jobs"]
    outputs = []
    for replica in plan["replicas"]:
        pilot = checked(replica["pilot"]).parent
        jobs = [job for job in result["jobs"] if job["id"].startswith(f"r{replica['replica']}-")]
        assert len(jobs) == 48
        path = root / f"replica-{replica['replica']}-evaluation.json"
        write(path, dict(pilot=replica["pilot"], binding=replica["binding"], endpoints=replica["endpoints"],
            jobs=jobs, dispatch=result_pin, launch=pin(root / "launch.json")))
        outputs.append(pin(path))
        write(pilot / "evaluation-completion.json", dict(complete=True, matches=3072,
            evaluation=pin(path), plan=pin(root / "plan.json"), outcome_analysis_performed=False))
    write(root / "completion.json", dict(complete=True, matches=result["matches"], evaluations=outputs,
        dispatch=result_pin, outcome_analysis_performed=False))
    print(dict(complete=True, matches=result["matches"], analysis="held until both replicas complete"), flush=True)


def analyze_both(root, experiment):
    top = read(experiment / "manifest.json")
    assert len(top["replicas"]) == 2 and top["evaluation_bo3"] == 6144
    pending = []
    for item in top["replicas"]:
        pilot = checked(item).parent
        completion = read(pilot / "evaluation-completion.json")
        assert completion["complete"] and completion["matches"] == 3072
        evaluation = checked(completion["evaluation"])
        plan = read(checked(completion["plan"]))
        for key in ["analysis", "bootstrap_implementation"]:
            checked(plan[key])
        pending.append((pilot, evaluation))
    root.mkdir()
    analyses = []
    for pilot, evaluation in pending:
        analyze(pilot, evaluation)
        analyses.append(pin(pilot / "analysis.json"))
    both_pass = all(read(checked(item))["verdict"] == "REPLICA-PASS" for item in analyses)
    write(root / "result.json", dict(complete=True, matches=6144, analyses=analyses,
        verdict="REPRODUCIBLE-DEVELOPMENT-IMPROVEMENT" if both_pass else "NO-ADVANCE",
        promotion=False, human_strength_claim=False, pooled_replica_rescue=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "run", "analyze"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--experiment", type=Path)
    parser.add_argument("--replica", type=int, choices=[1, 2])
    parser.add_argument("--compute", type=Path)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    if args.mode == "prepare":
        prepare(args.root, args.experiment, args.replica)
    elif args.mode == "run":
        run(args.root, args.compute)
    else:
        analyze_both(args.root, args.experiment)
