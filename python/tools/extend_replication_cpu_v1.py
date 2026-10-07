"""Measure the maintainer's full logical-core count and launch from the extended choice."""
import argparse
import copy
from pathlib import Path
from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, dispatch
from evaluation_throughput_v1 import require_choice


def extend(root):
    plan = read(root/"plan.json")
    checked(plan["runner"]); checked(plan["analysis"])
    original = read(root/"compute-choice.json")
    previous = read(root/"qualification.json")
    if not previous["complete"]:
        raise ValueError("original allocation qualification is incomplete")
    current = inventory("desktop")
    if current["active"]:
        raise ValueError("preserve active native owners")
    threads = sum(cpu["NumberOfLogicalProcessors"] for cpu in current["cpu"])
    if threads != 24:
        raise ValueError("this extension is the inspected 24-thread workstation only")
    write(root/"logical-core-extension.json", dict(plan=pin(root/"plan.json"), original_choice=pin(root/"compute-choice.json"),
        runner=pin(__file__), inventory=current, drives=["C", "D", "E"], workers=threads,
        fixed_cases=72, executions=216, outcome_selection=False, group_wall_cap_seconds=300))
    choice = copy.deepcopy(original)
    reference = read(checked(original["candidates"][0]["report"]))["fingerprints"]
    projections = dict(previous["projections"])
    for drive in ["C", "D", "E"]:
        part = next(p for p in current["partitions"] if p["DriveLetter"] == drive)
        disk = next(d for d in current["disks"] if d["Number"] == part["DiskNumber"])
        allocation = dict(desktop=dict(drive=drive, disk_serial=disk["SerialNumber"], disk_name=disk["FriendlyName"], workers=threads))
        label = f"desktop-{drive.lower()}-w24"
        report_pin = dispatch(root, label, plan["binary"], plan["qualification_jobs"], allocation, read(root/"remote-staging.json"), 300)
        result = read(checked(report_pin))
        if result["fingerprints"] != reference:
            raise ValueError("logical-core extension changed exact gameplay")
        projection = result["staging_seconds"]+(result["execution_seconds"]+result["recovery_seconds"])*plan["expected_matches"]/result["matches"]
        projections[label] = projection
        choice["candidates"].append(dict(id=label, report=report_pin))
        print(label, "complete; projected panel seconds", round(projection, 2), flush=True)
    choice["selected"] = min(projections, key=projections.get)
    choice["dependencies"].append(pin(__file__))
    write(root/"compute-choice-extended.json", choice)
    selected = require_choice(root/"compute-choice-extended.json", pin(root/"plan.json"), plan["binary"])
    write(root/"qualification-extended.json", dict(complete=True, selected=selected, projections=projections,
        original=pin(root/"qualification.json"), extension=pin(root/"logical-core-extension.json"),
        unique_cases=72, executed_matches=1512, exact_match_file_comparisons=1440,
        non_claim="Fixed repeated engineering cases only. No outcome interpretation or strength claim."))
    print(selected, flush=True)


def run(root):
    plan = read(root/"plan.json")
    checked(plan["runner"]); checked(plan["analysis"])
    qualification = read(root/"qualification-extended.json")
    selected = require_choice(root/"compute-choice-extended.json", pin(root/"plan.json"), plan["binary"])
    if not qualification["complete"] or selected != qualification["selected"] or selected["projected_seconds"] >= plan["projection_cap_seconds"]:
        raise ValueError("incomplete or excessive allocation")
    write(root/"launch.json", dict(plan=pin(root/"plan.json"), choice=pin(root/"compute-choice-extended.json"),
        qualification=pin(root/"qualification-extended.json"), selected=selected, runner=pin(__file__),
        group_wall_cap_seconds=plan["full_group_wall_cap_seconds"], expected_matches=2616, expected_jobs=90))
    result_pin = dispatch(root, "full-panel", plan["binary"], plan["jobs"], selected["allocation"],
        read(root/"remote-staging.json"), plan["full_group_wall_cap_seconds"])
    result = read(checked(result_pin))
    if result["matches"] != 2616 or len(result["jobs"]) != 90:
        raise ValueError("full panel is incomplete")
    write(root/"evaluation-manifest.json", dict(pilot=plan["pilot"], endpoints=plan["endpoints"],
        jobs=result["jobs"], dispatch=result_pin, launch=pin(root/"launch.json")))
    print(dict(complete=True, matches=2616, jobs=90, result=str(root/"evaluation-manifest.json")), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["extend", "run"])
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables validation")
    (extend if args.mode == "extend" else run)(args.root)
