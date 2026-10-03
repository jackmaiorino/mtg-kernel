"""Reuse exact remote timing under fresh ownership, with explicit cost revision."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import time

from public_training_dispatch_v2 import read, write, pin, checked, preflight, dispatch_qualified
from public_evaluation_dispatch_v1 import inventory
from compute_throughput_v2 import require_allocation
from qualify_state_prevention_compute_v1 import place
from public_entropy_remote_replica_v1 import jobs_for
from run_public_entropy_training_v1 import audit
from run_state_prevention_training_v1 import snapshot
from public_entropy_analysis_v1 import statistical_checks, bound_opponent


REVISION = Path(__file__).resolve().parents[2] / "docs/public_entropy_execution_revision_20260921.md"


def availability(root):
    hardware = {host: inventory(host) for host in ["jack", "haleyspc"]}
    for host, value in hardware.items():
        write(root / f"{host}-inventory.json", value)
    assert hardware["jack"]["active"], "Jack is free: qualify newly eligible local/cross allocations"
    assert not hardware["haleyspc"]["active"], "preserve active Haley owner"
    gpu = preflight("haleyspc", [place("haleyspc", 0, 1)])
    write(root / "haley-gpu.json", gpu)
    cloud = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    return dict(
        jack=dict(checked_at=hardware["jack"]["at"], evidence=pin(root / "jack-inventory.json"),
            eligible=False, devices=[], reason="Fresh inventory records active native owner; preserve its work and desktop GPU0 reservation."),
        haleyspc=dict(checked_at=hardware["haleyspc"]["at"], evidence=pin(root / "haleyspc-inventory.json"),
            eligible=True, reason="Current idle native inventory and GPU preflight passed.",
            devices=[dict(ordinal=0, uuid=place("haleyspc", 0, 1)["gpu_uuid"], eligible=True,
                reason="Actual current sole GPU passed free-VRAM/device check.")]),
        runpod=dict(checked_at=read(cloud)["checked_at"], evidence=pin(cloud), eligible=False, devices=[],
            reason="Authenticated inventory HTTP403; no new paid compute authority."))


def prepare(root, pilot, previous):
    m = read(pilot / "manifest.json")
    old = read(previous / "manifest.json")
    q = read(previous / "qualification.json")
    assert m["replica"] == 2 and q["complete"]
    assert old["pilot"] == pin(pilot / "manifest.json")
    assert old["configs"] == m["training_configs"] and old["binary"] == m["training_binary"]
    for item in [old["runner"], *old["dependencies"], m["runner"], m["design"], *m["dependencies"]]:
        checked(item)
    choice = copy.deepcopy(read(previous / "compute-choice.json"))
    assert choice["jobs"] == jobs_for(m["training_configs"])
    # Validate the original complete qualification before updating availability.
    before = require_allocation(previous / "compute-choice.json", m["training_binary"]["sha256"], choice["jobs"])
    assert before == q["selected"]
    root.mkdir()
    choice["inventory"] = availability(root)
    old_hardware = read(checked(read(previous / "compute-choice.json")["inventory"]["haleyspc"]["evidence"]))["hardware"]
    new_hardware = read(root / "haleyspc-inventory.json")
    assert old_hardware["host"] == new_hardware["host"]
    assert old_hardware["cpu"] == new_hardware["cpu"]
    assert old_hardware["disks"] == new_hardware["disks"], "storage hardware changed: remeasure"
    write(root / "compute-choice.json", choice)
    selected = require_allocation(root / "compute-choice.json", m["training_binary"]["sha256"], choice["jobs"])
    assert selected == before and selected["projected_seconds"] < 3600
    write(root / "qualification.json", dict(complete=True, selected=selected,
        pilot=pin(pilot / "manifest.json"), source=pin(previous / "qualification.json"),
        source_manifest=pin(previous / "manifest.json"), original_choice=pin(previous / "compute-choice.json"),
        runner=pin(__file__), execution_revision=pin(REVISION), newly_executed_games=0,
        reused_engineering_games=q["executed_games"], unique_cases=q["unique_arm_games"],
        exact_file_comparisons=q["exact_file_comparisons"], projected_seconds_cap=3600,
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_dispatch_v2.py",
            "compute_throughput_v2.py", "public_entropy_remote_replica_v1.py", "run_public_entropy_training_v1.py",
            "public_evaluation_dispatch_v1.py"]]))
    print(dict(qualified=True, selected=selected, newly_executed_games=0), flush=True)


def run(root, pilot, scorer):
    m, q = read(pilot / "manifest.json"), read(root / "qualification.json")
    assert m["replica"] == 2 and q["complete"] and q["pilot"] == pin(pilot / "manifest.json")
    assert not (pilot / "training-launch.json").exists(), "preserve prior launch"
    for item in [q["runner"], q["execution_revision"], *q["dependencies"], m["runner"], m["design"], *m["dependencies"]]:
        checked(item)
    selected = require_allocation(root / "compute-choice.json", m["training_binary"]["sha256"], jobs_for(m["training_configs"]))
    assert selected == q["selected"] and selected["projected_seconds"] < q["projected_seconds_cap"] == 3600
    # Fresh inventory again immediately before handing off to the supported launcher.
    current = root / "launch-availability"
    current.mkdir()
    availability(current)
    sq = read(scorer / "qualification.json")
    assert sq["complete"] and sq["original_gameplay_replays"] == sq["exact_new_entropy_bo3_replays"] == 4
    assert read(scorer / "manifest.json")["binary"] == m["evaluation_binary"]
    comparison = read(checked(sq["import_comparison"]))
    opponent = copy.deepcopy(m["evaluation_opponent"])
    opponent["source"]["play_import"] = comparison["source"]
    write(pilot / "evaluation-binding.json", dict(pilot=pin(pilot / "manifest.json"), binary=m["evaluation_binary"],
        qualification=pin(scorer / "qualification.json"), opponent=opponent))
    bound_opponent(pilot, m)
    statistical_checks()
    write(pilot / "training-launch.json", dict(pilot=pin(pilot / "manifest.json"), compute=pin(root / "qualification.json"),
        choice=pin(root / "compute-choice.json"), selected=selected, runner=pin(__file__),
        execution_revision=q["execution_revision"], analysis=pin(Path(__file__).with_name("public_entropy_analysis_v1.py")),
        bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        evaluation_binding=pin(pilot / "evaluation-binding.json"), expected_natural_games=4000,
        native_wall_cap_seconds=1800, no_prefix_selection=True, no_paid_compute=True, review=m["review"]))
    telemetry = pilot / "training-telemetry"
    telemetry.mkdir()
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(dispatch_qualified, pilot / "entropy-replica-2-training", m["training_binary"],
            m["training_configs"], root / "compute-choice.json", 1800)
        tick = 0
        while not future.done():
            row = dict(elapsed_seconds=time.monotonic()-started)
            try:
                row["haleyspc"] = snapshot("haleyspc")
            except Exception as error:
                row["telemetry_error"] = str(error)
            write(telemetry / f"{tick:04}.json", row)
            print("replica two telemetry", round(row["elapsed_seconds"]), row.get("haleyspc", {}).get("cpu_percent"), flush=True)
            tick += 1
            for _ in range(30):
                if future.done():
                    break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m, pilot, remote_only=True)
    audited["wall_seconds_including_audit"] = time.monotonic()-started
    write(pilot / "training-audit.json", audited)
    print(dict(complete=True, natural_games=4000, root=str(pilot)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "run"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--scorer", type=Path)
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    if args.mode == "prepare":
        prepare(args.root, args.pilot, args.previous)
    else:
        run(args.root, args.pilot, args.scorer)
