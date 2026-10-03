"""Replication panel: longer timing batches and longest-job-first dispatch."""
import argparse
import copy
from pathlib import Path
import subprocess

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, prepare_remote, dispatch
from evaluation_throughput_v1 import require_choice

ARMS = ["g115", "control", "structured"]


def prepare(root, pilot):
    training = read(pilot/"training-audit.json")
    assert training["complete"] and training["full_natural_games"] == 4000
    manifest = read(pilot/"manifest.json")
    endpoints = dict(g115=dict(kind="legacy", source=manifest["source"], v3_forced_actions=False))
    assets = {}
    def add(item):
        path = checked(item)
        assets[str(path)] = item
        return path
    for arm in ["control", "structured"]:
        final = training["arms"][arm]
        assert final["updates"] == 200 and final["legacy_adam_step"] == 32600 and final["public_adam_step"] == 200
        endpoints[arm] = dict(kind="public_checkpoint", config=manifest["training_configs"][arm], checkpoint=final["checkpoint"])
        add(manifest["training_configs"][arm]); add(final["checkpoint"]); add(final["optimizer"])
    opponent = manifest["evaluation_opponent"]
    descriptor = add(opponent["source"]["play_import"])
    for value in read(descriptor).values():
        if isinstance(value, dict) and "path" in value and "sha256" in value: add(value)
    root.mkdir()
    jobs = []
    for template in manifest["jobs"]:
        for arm in ARMS:
            command = copy.deepcopy(read(checked(template["template"])))
            command["sources"][template["candidate_seat"]] = endpoints[arm]
            jobs.append(dict(id=f"{arm}-{template['label']}", arm=arm, label=template["label"], command=command))
    assert len(jobs) == 90 and sum(len(j["command"]["matches"]) for j in jobs) == 2616
    jobs.sort(key=lambda job: (-len(job["command"]["matches"]), job["id"]))
    sample_labels = [f"{cohort}-{own}-p{seat}" for cohort, names in [
        ("canonical", ["Affinity", "Faeries", "Terror"]),
        ("focal", ["published-44ae71e1e126b63d", "Rally", "Burn"])] for own in names for seat in [0, 1]]
    sample = []
    for job in jobs:
        if job["label"] in sample_labels:
            item = copy.deepcopy(job)
            item["command"]["matches"] = item["command"]["matches"][:2]
            sample.append(item)
    assert len(sample) == 36 and sum(len(j["command"]["matches"]) for j in sample) == 72
    write(root/"plan.json", dict(pilot=pin(pilot/"manifest.json"), training_audit=pin(pilot/"training-audit.json"),
        binary=manifest["evaluation_binary"], endpoints=endpoints, jobs=jobs, qualification_jobs=sample,
        expected_matches=2616, expected_jobs=90, runner=pin(__file__),
        analysis=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        question="Complete the frozen terminal BO3 comparison using the fastest measured CPU/storage allocation with identical results.",
        qualification="72 preselected matches in 36 two-match jobs spanning all three final endpoints, both physical seats and both cohorts; compare native match bytes, never select on wins.",
        gates="Unchanged pilot manifest. No partial outcome interpretation, repeated timing outputs never pooled.",
        process_wall_cap_seconds=300, full_group_wall_cap_seconds=3600,
        projection_cap_seconds=3600, paid_compute=False, review=manifest["review"]))
    remote = prepare_remote(root, list(assets.values()))
    print(dict(prepared=str(root), full_matches=2616, qualification_matches=72,
        remote_staging_seconds=remote["seconds"]), flush=True)


def qualify(root):
    plan = read(root/"plan.json")
    remote = read(root/"remote-staging.json")
    snapshots, available, stores = {}, {}, []
    for host, drives in [("jack", ["C", "D", "E"]), ("haleyspc", ["C"])]:
        snapshot = inventory(host)
        assert not snapshot["active"], "wait for healthy native training to finish"
        path = root/f"{host}-inventory.json"
        write(path, snapshot)
        snapshots[host] = snapshot
        available[host] = dict(checked_at=snapshot["at"], evidence=pin(path), eligible=True,
            reason="No competing native owner; bounded CPU evaluation within whole-PC assignment, BelowNormal priority.")
        for drive in drives:
            p = next(p for p in snapshot["partitions"] if p["DriveLetter"] == drive)
            disk = next(d for d in snapshot["disks"] if d["Number"] == p["DiskNumber"])
            stores.append((host, drive, disk))
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    available["runpod"] = dict(checked_at=cloud["checked_at"], evidence=pin(cloud_path), eligible=False,
        reason="Latest authenticated inventory HTTP403; no new paid allocation or confirmed available pod.")
    placements = lambda host, drive, disk, workers: {host:dict(drive=drive, disk_serial=disk["SerialNumber"], disk_name=disk["FriendlyName"], workers=workers)}
    cases = [(f"{host}-{drive.lower()}-w{workers}", placements(host, drive, disk, workers))
        for workers in [1, 4, 8, 16] for host, drive, disk in stores]
    disk_for = {(host, drive): disk for host, drive, disk in stores}
    for workers in [8, 16]:
        cases.append((f"both-d-w{workers}", dict(**placements("jack", "D", disk_for["jack", "D"], workers),
            **placements("haleyspc", "C", disk_for["haleyspc", "C"], workers))))
    write(root/"qualification-design.json", dict(plan=pin(root/"plan.json"), cases=cases,
        matches_per_case=72, executions=72*len(cases), outcome_selection=False,
        exact_match_bytes_required=True, maximum_group_seconds=300,
        projection_formula="remote input staging once + measured dispatch staging + (native wall + verified recovery) * 2616/72; conservative because full jobs amortize model loading across more matches"))
    candidates, projections, baseline = [], {}, None
    for label, allocation in cases:
        result_pin = dispatch(root, label, plan["binary"], plan["qualification_jobs"], allocation, remote, 300)
        result = read(checked(result_pin))
        assert baseline is None or baseline == result["fingerprints"], "qualification changed exact gameplay"
        baseline = result["fingerprints"]
        projected = (remote["seconds"] if "haleyspc" in allocation else 0)+result["staging_seconds"]+(result["execution_seconds"]+result["recovery_seconds"])*2616/72
        projections[label] = projected
        candidates.append(dict(id=label, report=result_pin))
        print(label, "complete; projected panel seconds", round(projected, 2), flush=True)
        if len(candidates) == 1:
            assert result["execution_seconds"] < 120, "cheap timing envelope failed"
    choice = dict(schema="cpu-bo3-allocation/v1", plan=pin(root/"plan.json"), binary=plan["binary"],
        inventory=available, eligible_storage=[(host, drive, disk["SerialNumber"]) for host, drive, disk in stores],
        remote_setup_seconds=remote["seconds"], candidates=candidates, selected=min(projections, key=projections.get),
        dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_evaluation_dispatch_v1.py", "evaluation_throughput_v1.py"]])
    write(root/"compute-choice.json", choice)
    selected = require_choice(root/"compute-choice.json", pin(root/"plan.json"), plan["binary"])
    write(root/"qualification.json", dict(complete=True, selected=selected, projections=projections,
        exact_match_file_comparisons=72*(len(candidates)-1), executed_matches=72*len(candidates), unique_cases=72,
        non_claim="Runtime/replay qualification only, not an outcome estimate. Repeated seed outcomes are not pooled."))
    print(selected, flush=True)


def run(root):
    if not __debug__:
        raise RuntimeError("optimized Python disables dependency validation")
    plan = read(root/"plan.json")
    checked(plan["runner"]); checked(plan["analysis"])
    qualification = read(root/"qualification.json")
    assert qualification["complete"]
    selected = require_choice(root/"compute-choice.json", pin(root/"plan.json"), plan["binary"])
    assert selected == qualification["selected"] and selected["projected_seconds"] < plan["projection_cap_seconds"]
    write(root/"launch.json", dict(plan=pin(root/"plan.json"), choice=pin(root/"compute-choice.json"),
        qualification=pin(root/"qualification.json"), selected=selected, runner=pin(__file__),
        group_wall_cap_seconds=plan["full_group_wall_cap_seconds"], expected_matches=2616, expected_jobs=90))
    result_pin = dispatch(root, "full-panel", plan["binary"], plan["jobs"], selected["allocation"],
        read(root/"remote-staging.json"), plan["full_group_wall_cap_seconds"])
    result = read(checked(result_pin))
    assert result["matches"] == 2616 and len(result["jobs"]) == 90
    write(root/"evaluation-manifest.json", dict(pilot=plan["pilot"], endpoints=plan["endpoints"],
        jobs=result["jobs"], dispatch=result_pin, launch=pin(root/"launch.json")))
    print(dict(complete=True, matches=2616, jobs=90, result=str(root/"evaluation-manifest.json")), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "qualify", "run"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables dependency validation")
    if args.mode == "prepare": prepare(args.root, args.pilot)
    elif args.mode == "qualify": qualify(args.root)
    else: run(args.root)
