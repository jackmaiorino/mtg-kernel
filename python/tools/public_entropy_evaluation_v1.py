"""Evaluate both frozen entropy replicas together; analyze each independently."""
import argparse
import copy
from pathlib import Path

from public_training_dispatch_v2 import read, write, pin, checked
from public_evaluation_dispatch_v1 import inventory, prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from public_training_storage_v1 import storage
from evaluation_throughput_v1 import require_choice
from public_entropy_analysis_v1 import ARMS, bound_opponent, analyze


def prepare(root, experiment):
    top = read(experiment / "manifest.json")
    assert top["training_games"] == 8000 and top["evaluation_bo3"] == 6144
    replicas, jobs, samples, assets = [], [], [], {}
    binary = None
    def add(item):
        checked(item)
        assets[item["path"]] = item
    for pilot_pin in top["replicas"]:
        pilot = checked(pilot_pin).parent
        manifest = read(checked(pilot_pin))
        training = read(pilot / "training-audit.json")
        assert training["complete"] and training["full_natural_games"] == 4000
        launch = read(pilot / "training-launch.json")
        for name in ["analysis", "bootstrap_implementation"]:
            checked(launch[name])
        assert launch["analysis"] == pin(Path(__file__).with_name("public_entropy_analysis_v1.py"))
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
    assert len(jobs) == len(samples) == 96
    assert sum(len(job["command"]["matches"]) for job in jobs) == 6144
    # Interleave replicas/endpoints to avoid assigning one condition wholesale
    # to the slower host under weighted deterministic dispatch.
    jobs.sort(key=lambda j: (j["label"], j["arm"], j["replica"]))
    samples.sort(key=lambda j: (j["label"], j["arm"], j["replica"]))
    root.mkdir()
    write(root / "plan.json", dict(experiment=pin(experiment / "manifest.json"), replicas=replicas,
        binary=binary, jobs=jobs, qualification_jobs=samples, expected_matches=6144, expected_jobs=96,
        runner=pin(__file__), analysis=pin(Path(__file__).with_name("public_entropy_analysis_v1.py")),
        bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        full_group_wall_cap_seconds=1800, projection_cap_seconds=1800,
        qualification_matches=96, no_prefix_selection=True, no_paid_compute=True))
    remote = prepare_remote(root, list(assets.values()))
    print(dict(prepared=str(root), matches=6144, qualification_matches=96, staging_seconds=remote["seconds"]), flush=True)


def qualify(root):
    plan = read(root / "plan.json")
    for name in ["runner", "analysis", "bootstrap_implementation"]:
        checked(plan[name])
    remote = read(root / "remote-staging.json")
    availability, stores = {}, {}
    for host, drives in [("jack", ["C", "D", "E"]), ("haleyspc", ["C"])]:
        snapshot = inventory(host)
        assert not snapshot["active"], "preserve competing native owners"
        path = root / f"{host}-inventory.json"
        write(path, snapshot)
        availability[host] = dict(checked_at=snapshot["at"], evidence=pin(path), eligible=True,
            reason="Current idle native inventory; bounded BelowNormal CPU evaluation.")
        for drive in drives:
            stores[host, drive] = storage(snapshot, drive)
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    availability["runpod"] = dict(checked_at=cloud["checked_at"], evidence=pin(cloud_path), eligible=False,
        reason="Latest authenticated inventory HTTP403; no new paid compute authorized.")
    def placement(host, drive, workers, weight=1):
        store = stores[host, drive]
        return dict(drive=drive, disk_serial=store["disk_serial"], disk_name=store["disk_name"], workers=workers, job_weight=weight)
    cases = [(f"jack-{drive.lower()}-w{count}", dict(jack=placement("jack", drive, count)))
             for drive in ["C", "D", "E"] for count in [1, 16]]
    cases += [("jack-d-w24", dict(jack=placement("jack", "D", 24)))]
    cases += [(f"haley-w{count}", dict(haleyspc=placement("haleyspc", "C", count))) for count in [1, 8, 16]]
    cases += [(f"both-weight{weight}", dict(jack=placement("jack", "D", 24, weight),
        haleyspc=placement("haleyspc", "C", 16))) for weight in [1, 3]]
    write(root / "qualification-design.json", dict(plan=pin(root / "plan.json"), cases=cases,
        matches_per_case=96, maximum_executed_matches=1152, group_wall_seconds=300,
        first_serial_cap_seconds=180, no_outcome_selection=True))
    candidates, projections, reference = [], {}, None
    for label, allocation in cases:
        result_pin = dispatch(root, label, plan["binary"], plan["qualification_jobs"], allocation, remote, 300)
        result = read(checked(result_pin))
        assert reference is None or reference == result["fingerprints"], "placement changed gameplay"
        reference = result["fingerprints"]
        if not candidates:
            assert result["execution_seconds"] < 180, "cheap serial timing envelope exceeded"
        projections[label] = (remote["seconds"] if "haleyspc" in allocation else 0) + result["staging_seconds"] + (result["execution_seconds"]+result["recovery_seconds"]) * 64
        candidates.append(dict(id=label, report=result_pin))
        print(label, "complete; projected full-panel seconds", round(projections[label], 2), flush=True)
    choice = dict(schema="cpu-bo3-allocation/v1", plan=pin(root / "plan.json"), binary=plan["binary"],
        inventory=availability, eligible_storage=[(host, drive, value["disk_serial"]) for (host, drive), value in stores.items()],
        remote_setup_seconds=remote["seconds"], candidates=candidates, selected=min(projections, key=projections.get),
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_evaluation_dispatch_v1.py", "public_evaluation_dispatch_v2.py", "evaluation_throughput_v1.py"]])
    write(root / "compute-choice.json", choice)
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    write(root / "qualification.json", dict(complete=True, selected=selected, projections=projections,
        qualification_matches=1152, unique_cases=96, exact_match_file_comparisons=1056))


def run(root):
    plan = read(root / "plan.json")
    for name in ["runner", "analysis", "bootstrap_implementation"]:
        checked(plan[name])
    qualification = read(root / "qualification.json")
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    assert qualification["complete"] and selected == qualification["selected"]
    assert selected["projected_seconds"] < plan["projection_cap_seconds"]
    write(root / "launch.json", dict(plan=pin(root / "plan.json"), choice=pin(root / "compute-choice.json"),
        qualification=pin(root / "qualification.json"), selected=selected))
    result_pin = dispatch(root, "full-panel", plan["binary"], plan["jobs"], selected["allocation"],
                          read(root / "remote-staging.json"), plan["full_group_wall_cap_seconds"])
    result = read(checked(result_pin))
    assert result["matches"] == 6144 and len(result["jobs"]) == 96
    analyses = []
    for replica in plan["replicas"]:
        pilot = checked(replica["pilot"]).parent
        jobs = [job for job in result["jobs"] if job["id"].startswith(f"r{replica['replica']}-")]
        assert len(jobs) == 48
        path = root / f"replica-{replica['replica']}-evaluation.json"
        write(path, dict(pilot=replica["pilot"], binding=replica["binding"], endpoints=replica["endpoints"],
            jobs=jobs, dispatch=result_pin, launch=pin(root / "launch.json")))
        analyze(pilot, path)
        analyses.append(pin(pilot / "analysis.json"))
    both_pass = all(read(checked(item))["verdict"] == "REPLICA-PASS" for item in analyses)
    write(root / "result.json", dict(complete=True, matches=6144, analyses=analyses,
        verdict="REPRODUCIBLE-DEVELOPMENT-IMPROVEMENT" if both_pass else "NO-ADVANCE",
        promotion=False, human_strength_claim=False, pooled_replica_rescue=False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "qualify", "run"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--experiment", type=Path)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    if args.mode == "prepare":
        prepare(args.root, args.experiment)
    elif args.mode == "qualify":
        qualify(args.root)
    else:
        run(args.root)
