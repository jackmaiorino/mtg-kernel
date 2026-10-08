"""Resume timing preflight after the diagnosed compute host serial group-cap expiry."""
import argparse
from pathlib import Path
from prevention_crossover_v1 import read, write, pin, checked, dispatch, require_choice


def run(root):
    plan = read(root / "plan.json")
    for key in ["runner", "analysis", "design"]: checked(plan[key])
    design = read(root / "qualification-design.json")
    assert design["plan"] == pin(root / "plan.json")
    failed = read(root / "interrupted-computehost-worker.json")
    counts = read(root / "interrupted-computehost-counts.json")
    assert counts["completed_jobs"] == 47 and counts["match_files"] == 189 and not counts["active"]
    assert failed["workers"] == 1 and failed["jobs"] == 48 and len(failed["errors"]) == 1
    assert "'timeout': True" in failed["errors"][0] and 300 <= failed["seconds"] < 305
    write(root / "preflight-recovery.json", dict(plan=pin(root / "plan.json"), runner=pin(__file__),
        interrupted_worker=pin(root / "interrupted-computehost-worker.json"), counts=pin(root / "interrupted-computehost-counts.json"),
        reason="Serial group exhausted 300 seconds after 47 jobs; final four-match job received only 3.734 seconds. No stalled-simulation inference. Preserve interrupted outputs and retry only this timing allocation in a fresh namespace with a 600-second group cap.",
        reused_complete_allocations=9, retry_label="computehost-w1-cap600", retry_group_seconds=600,
        other_group_seconds=300, scientific_inputs_and_analysis_unchanged=True,
        interrupted_match_files_excluded_from_choice=189))
    remote = read(root / "remote-staging.json")
    references = {name: pin(checked(item))["sha256"] for name,item in plan["replay_reference"].items()}
    candidates, projections = [], {}
    for label, allocation in design["cases"]:
        if label == "computehost-w1":
            label = "computehost-w1-cap600"
        path = root / label / "result.json"
        if path.exists():
            assert len(candidates) < 9, "unexpected preexisting recovery allocation"
            report = pin(path)
        else:
            assert len(candidates) >= 9, "missing original completed allocation"
            report = dispatch(root, label, plan["binary"], plan["qualification_jobs"], allocation,
                remote, 600 if label == "computehost-w1-cap600" else 300)
        result = read(checked(report))
        assert result["allocation"] == allocation and result["fingerprints"] == references
        projection = (remote["seconds"] if "computehost" in allocation else 0) + result["staging_seconds"] + (result["execution_seconds"]+result["recovery_seconds"])*3488/192
        candidates.append(dict(id=label, report=report))
        projections[label] = projection
        print(label, "verified; projected seconds", round(projection, 2), flush=True)
    availability, stores = {}, []
    for host, drives in [("desktop", ["C", "D", "E"]), ("computehost", ["C"])]:
        snapshot = read(root / f"{host}-inventory.json")
        availability[host] = dict(checked_at=snapshot["at"], evidence=pin(root / f"{host}-inventory.json"), eligible=True,
            reason="Original idle inventory plus fresh owner/storage checks at every dispatch; unchanged CPU qualification.")
        for drive in drives:
            part = next(p for p in snapshot["partitions"] if p["DriveLetter"] == drive)
            disk = next(d for d in snapshot["disks"] if d["Number"] == part["DiskNumber"])
            stores.append((host, drive, disk["SerialNumber"]))
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    availability["runpod"] = dict(checked_at=cloud["checked_at"], evidence=pin(cloud_path), eligible=False,
        reason="Latest authenticated inventory HTTP403; no paid allocation or verified idle pod.")
    choice = dict(schema="cpu-bo3-allocation/v1", plan=pin(root / "plan.json"), binary=plan["binary"],
        inventory=availability, eligible_storage=stores, remote_setup_seconds=remote["seconds"],
        candidates=candidates, selected=min(projections, key=projections.get),
        recovery=pin(root / "preflight-recovery.json"), dependencies=[pin(__file__)]+[
            pin(Path(__file__).with_name(n)) for n in ["public_evaluation_dispatch_v1.py", "public_evaluation_dispatch_v2.py", "evaluation_throughput_v1.py"]])
    write(root / "compute-choice.json", choice)
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    write(root / "qualification.json", dict(complete=True, selected=selected, projections=projections,
        unique_original_cases=192, executed_matches_complete_allocations=2688,
        exact_original_match_comparisons=2688, interrupted_match_files_excluded=189,
        recovery=pin(root / "preflight-recovery.json")))
    print(selected, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables validation")
    run(args.root)
