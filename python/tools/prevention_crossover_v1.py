"""Complete missing cells of a fixed-endpoint, two-panel diagnostic."""
import argparse
import copy
from pathlib import Path

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from evaluation_throughput_v1 import require_choice

PILOTS = {i: Path(f"E:/mtg-postboard-campaign-20260920/state-prevention-pilot-{i:03}") for i in [1, 2]}
EVALUATIONS = {1: Path("E:/mtg-meta-recovery-20260920/state-prevention-bo3-001"),
               2: Path("E:/mtg-meta-recovery-20260920/state-prevention-bo3-replication-002")}
ARMS = ["control", "structured"]
GATES = "published-44ae71e1e126b63d"


def prepare(root):
    manifests = {i: read(p / "manifest.json") for i, p in PILOTS.items()}
    prior = {i: read(p / "evaluation-manifest.json") for i, p in EVALUATIONS.items()}
    for i in [1, 2]:
        result = read(PILOTS[i] / "analysis.json")
        assert result["complete"] and result["matches"] == 2616
        assert result["evaluation"] == pin(EVALUATIONS[i] / "evaluation-manifest.json")
    assert manifests[1]["evaluation_binary"] == manifests[2]["evaluation_binary"]
    assert manifests[1]["evaluation_opponent"] == manifests[2]["evaluation_opponent"]
    root.mkdir()
    assets = {}
    def add(item):
        path = checked(item)
        assets[str(path)] = item
        return path
    endpoints = {str(i): {arm: prior[i]["endpoints"][arm] for arm in ARMS} for i in [1, 2]}
    for i in [1, 2]:
        audit = read(PILOTS[i] / "training-audit.json")
        assert audit["complete"] and audit["full_natural_games"] == 4000
        for arm in ARMS:
            add(endpoints[str(i)][arm]["config"])
            add(endpoints[str(i)][arm]["checkpoint"])
            add(audit["arms"][arm]["optimizer"])
    descriptor = add(manifests[1]["evaluation_opponent"]["source"]["play_import"])
    for value in read(descriptor).values():
        if isinstance(value, dict) and "path" in value and "sha256" in value:
            add(value)
    jobs, samples, reference = [], [], {}
    def create(train, panel, arm, template, source, start, count, prefix):
        command = copy.deepcopy(read(checked(source["request"])))
        command["sources"][template["candidate_seat"]] = endpoints[str(train)][arm]
        command["matches"] = command["matches"][start:start+count]
        command["output_directory"] = "assigned-at-dispatch"
        label = template["label"]
        return dict(id=f"{prefix}-t{train}-p{panel}-{arm}-{label}-n{start:03}",
            arm=arm, label=label, train=train, panel=panel, offset=start,
            candidate_seat=template["candidate_seat"], cases=template["cases"][start:start+count], command=command)
    for train in [1, 2]:
        panel = 3-train
        lookup = {(j["arm"], j["label"]): j for j in prior[panel]["jobs"]}
        for template in manifests[panel]["jobs"]:
            for arm in ARMS:
                source = lookup[arm, template["label"]]
                for start in range(0, len(template["cases"]), 4):
                    jobs.append(create(train, panel, arm, template, source, start, 4, "cross"))
        own_lookup = {(j["arm"], j["label"]): j for j in prior[train]["jobs"]}
        template_lookup = {j["label"]: j for j in manifests[train]["jobs"]}
        for arm in ARMS:
            for seat in [0, 1]:
                selections = [(f"focal-{GATES}-p{seat}", n) for n in [0, 32, 64, 96]]
                selections += [(f"canonical-{name}-p{seat}", 0) for name in ["Terror", "Affinity"]]
                for label, start in selections:
                    source = own_lookup[arm, label]
                    sample = create(train, train, arm, template_lookup[label], source, start, 4, "replay")
                    samples.append(sample)
                    for index in range(4):
                        reference[f"{sample['id']}/{index}"] = pin(Path(source["output_directory"]) / f"match-{start+index:06}.json")
    assert len(jobs) == 872 and sum(len(j["cases"]) for j in jobs) == 3488
    assert len(samples) == 48 and len(reference) == 192
    # Interleave endpoints and panels; prioritize long focal Gates batches.
    jobs.sort(key=lambda j: (not j["label"].startswith(f"focal-{GATES}"), j["offset"], j["label"], j["train"], j["arm"]))
    samples.sort(key=lambda j: (j["offset"], j["label"], j["train"], j["arm"]))
    write(root / "plan.json", dict(schema="prevention-fixed-endpoint-crossover/v1",
        pilots={str(i): pin(p / "manifest.json") for i,p in PILOTS.items()},
        original_evaluations={str(i): pin(p / "evaluation-manifest.json") for i,p in EVALUATIONS.items()},
        binary=manifests[1]["evaluation_binary"], endpoints=endpoints, jobs=jobs,
        qualification_jobs=samples, replay_reference=reference, expected_matches=3488, expected_jobs=872,
        runner=pin(__file__), analysis=pin(Path(__file__).with_name("prevention_crossover_analysis_v1.py")),
        design=pin(Path(__file__).parents[2] / "docs/prevention_crossover_design_20260921.md"),
        bootstrap_replicates=10000, bootstrap_seeds={"focal": [202609210101, 202609210102], "canonical": [202609210201, 202609210202]},
        full_group_wall_cap_seconds=1800, projection_cap_seconds=1800,
        review="Known zero-read Fable HTTP429 until Sep22 07:00EDT; no retry or endorsement. Bounded diagnostic under Jack's execution assignment.",
        non_claim="Retrospective fixed-endpoint diagnostic only. No new training, promotion or human-strength claim; no CP7 selection."))
    remote = prepare_remote(root, list(assets.values()))
    print(dict(prepared=str(root), new_matches=3488, jobs=872, qualification_matches=192, remote_staging_seconds=remote["seconds"]), flush=True)


def qualify(root):
    plan = read(root / "plan.json")
    for key in ["runner", "analysis", "design"]: checked(plan[key])
    remote = read(root / "remote-staging.json")
    availability, stores = {}, {}
    for host, drives in [("jack", ["C", "D", "E"]), ("haleyspc", ["C"])]:
        snapshot = inventory(host)
        assert not snapshot["active"], "preserve other native owners"
        path = root / f"{host}-inventory.json"
        write(path, snapshot)
        availability[host] = dict(checked_at=snapshot["at"], evidence=pin(path), eligible=True,
            reason="Idle native inventory; CPU evaluation with BelowNormal workers within local assignment.")
        for drive in drives:
            part = next(p for p in snapshot["partitions"] if p["DriveLetter"] == drive)
            stores[host, drive] = next(d for d in snapshot["disks"] if d["Number"] == part["DiskNumber"])
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    availability["runpod"] = dict(checked_at=cloud["checked_at"], evidence=pin(cloud_path), eligible=False,
        reason="Latest authenticated inventory HTTP403; no new paid allocation or verified idle pod.")
    def placement(host, drive, workers, weight=1):
        disk = stores[host, drive]
        return dict(drive=drive, disk_serial=disk["SerialNumber"], disk_name=disk["FriendlyName"], workers=workers, job_weight=weight)
    cases = [(f"jack-c-w{w}", dict(jack=placement("jack", "C", w))) for w in [1, 4, 8, 16, 24]]
    cases += [(f"jack-{d.lower()}-w{w}", dict(jack=placement("jack", d, w))) for d in ["D", "E"] for w in [1, 24]]
    cases += [(f"haley-w{w}", dict(haleyspc=placement("haleyspc", "C", w))) for w in [1, 8, 16]]
    cases += [(f"both-weight{weight}", dict(jack=placement("jack", "C", 24, weight),
                 haleyspc=placement("haleyspc", "C", 16))) for weight in [1, 3]]
    write(root / "qualification-design.json", dict(plan=pin(root / "plan.json"), cases=cases,
        matches_per_case=192, executions=192*len(cases), group_wall_seconds=300,
        first_serial_cap_seconds=180, outcome_selection=False, split_bytes_exact=True))
    candidates, projections = [], {}
    for label, allocation in cases:
        report = dispatch(root, label, plan["binary"], plan["qualification_jobs"], allocation, remote, 300)
        result = read(checked(report))
        assert result["fingerprints"] == {name: pin(checked(item))["sha256"] for name, item in plan["replay_reference"].items()}, "split replay differs from original full job"
        projection = (remote["seconds"] if "haleyspc" in allocation else 0) + result["staging_seconds"] + (result["execution_seconds"]+result["recovery_seconds"])*3488/192
        candidates.append(dict(id=label, report=report))
        projections[label] = projection
        print(label, "complete; projected seconds", round(projection, 2), flush=True)
        if len(candidates) == 1:
            assert result["execution_seconds"] < 180, "cheap timing cap failed"
    choice = dict(schema="cpu-bo3-allocation/v1", plan=pin(root / "plan.json"), binary=plan["binary"],
        inventory=availability, eligible_storage=[(h,d,s["SerialNumber"]) for (h,d),s in stores.items()],
        remote_setup_seconds=remote["seconds"], candidates=candidates, selected=min(projections, key=projections.get),
        dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_evaluation_dispatch_v1.py", "public_evaluation_dispatch_v2.py", "evaluation_throughput_v1.py"]])
    write(root / "compute-choice.json", choice)
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    write(root / "qualification.json", dict(complete=True, selected=selected, projections=projections,
        unique_original_cases=192, executed_matches=192*len(cases), exact_original_match_comparisons=192*len(cases)))
    print(selected, flush=True)


def run(root):
    plan = read(root / "plan.json")
    for key in ["runner", "analysis", "design"]: checked(plan[key])
    qualification = read(root / "qualification.json")
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    assert qualification["complete"] and selected == qualification["selected"]
    assert selected["projected_seconds"] < plan["projection_cap_seconds"]
    write(root / "launch.json", dict(plan=pin(root / "plan.json"), qualification=pin(root / "qualification.json"),
        choice=pin(root / "compute-choice.json"), selected=selected, runner=pin(__file__)))
    report = dispatch(root, "full-panel", plan["binary"], plan["jobs"], selected["allocation"],
        read(root / "remote-staging.json"), plan["full_group_wall_cap_seconds"])
    result = read(checked(report))
    assert result["matches"] == 3488 and len(result["jobs"]) == 872
    write(root / "completion.json", dict(complete=True, plan=pin(root / "plan.json"), dispatch=report,
        launch=pin(root / "launch.json"), new_matches=3488, new_jobs=872))
    print(dict(complete=True, matches=3488, jobs=872), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "qualify", "run"])
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables dependency validation")
    {"prepare": prepare, "qualify": qualify, "run": run}[args.mode](args.root)
