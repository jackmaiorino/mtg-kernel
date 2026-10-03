"""Select completed Haley evidence while Jack's independent owner is active."""
import argparse
from pathlib import Path
from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory
from evaluation_throughput_v1 import require_choice


def run(root):
    plan = read(root / "plan.json")
    for key in ["runner", "analysis", "design"]: checked(plan[key])
    availability = {}
    for host in ["jack", "haleyspc"]:
        snapshot = inventory(host)
        write(root / f"{host}-launch-availability.json", snapshot)
        assert bool(snapshot["active"]) == (host == "jack"), "availability changed; reassess before launch"
        availability[host] = dict(checked_at=snapshot["at"], evidence=pin(root / f"{host}-launch-availability.json"),
            eligible=host == "haleyspc", reason=("Kimi native test/build owner is active; preserve its work and exclude this host until it finishes."
                if host == "jack" else "Idle native inventory; full CPU evaluation within the existing local scope."))
        if host == "haleyspc":
            part = next(p for p in snapshot["partitions"] if p["DriveLetter"] == "C")
            disk = next(d for d in snapshot["disks"] if d["Number"] == part["DiskNumber"])
            storage = (host, "C", disk["SerialNumber"])
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    availability["runpod"] = dict(checked_at=cloud["checked_at"], evidence=pin(cloud_path), eligible=False,
        reason="Latest authenticated inventory HTTP403; no paid allocation or verified available pod.")
    write(root / "availability-change.json", dict(plan=pin(root / "plan.json"), runner=pin(__file__),
        reason="Kimi resumed local native work during the remote-only timing phase. The combined-PC preflight rejected overlap before spawning. Preserve all twelve complete allocations, exclude the currently owned workstation, and choose the fastest currently eligible Haley allocation.",
        original_serial_recovery=pin(root / "preflight-recovery.json"),
        completed_historical_local_allocations=9, combined_allocations_unmeasured=2,
        scientific_inputs_and_analysis_unchanged=True, owners={h:a["evidence"] for h,a in availability.items()}))
    remote = read(root / "remote-staging.json")
    candidates, projections = [], {}
    references = {name: pin(checked(item))["sha256"] for name,item in plan["replay_reference"].items()}
    for label in ["haley-w1-cap600", "haley-w8", "haley-w16"]:
        report = pin(root / label / "result.json")
        result = read(checked(report))
        assert set(result["allocation"]) == {"haleyspc"} and result["fingerprints"] == references
        candidates.append(dict(id=label, report=report))
        projections[label] = remote["seconds"]+result["staging_seconds"]+(result["execution_seconds"]+result["recovery_seconds"])*3488/192
    choice = dict(schema="cpu-bo3-allocation/v1", plan=pin(root / "plan.json"), binary=plan["binary"],
        inventory=availability, eligible_storage=[storage], remote_setup_seconds=remote["seconds"],
        candidates=candidates, selected=min(projections, key=projections.get),
        availability_change=pin(root / "availability-change.json"), dependencies=[pin(__file__)]+[
            pin(Path(__file__).with_name(n)) for n in ["public_evaluation_dispatch_v1.py", "public_evaluation_dispatch_v2.py", "evaluation_throughput_v1.py"]])
    write(root / "compute-choice.json", choice)
    selected = require_choice(root / "compute-choice.json", pin(root / "plan.json"), plan["binary"])
    write(root / "qualification.json", dict(complete=True, selected=selected, projections=projections,
        unique_original_cases=192, complete_allocations=12, eligible_complete_allocations=3,
        exact_original_match_comparisons=2304, executed_matches_complete_allocations=2304,
        interrupted_match_files_excluded=189, combined_allocations_unmeasured=2,
        availability_change=pin(root / "availability-change.json")))
    print(selected, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables validation")
    run(args.root)
