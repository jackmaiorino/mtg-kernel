"""Qualify/run replica two on idle Haley while replica one owns Jack's GPU1."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import time

from public_training_dispatch_v2 import read, write, pin, checked, preflight, dispatch_qualification, dispatch_qualified
from public_evaluation_dispatch_v1 import inventory
from compute_throughput_v2 import require_allocation
from qualify_state_prevention_compute_v1 import place, reports
from qualify_public_entropy_compute_v1 import audit as audit_prefix
from run_public_entropy_training_v1 import audit
from run_state_prevention_training_v1 import snapshot
from public_entropy_analysis_v1 import statistical_checks, bound_opponent


def jobs_for(configs):
    return {arm: dict(config_sha256=item["sha256"], updates=200) for arm, item in configs.items()}


def qualify(root, pilot, other):
    manifest = read(pilot / "manifest.json")
    assert manifest["replica"] == 2 and read(other / "manifest.json")["replica"] == 1
    checked(read(other / "training-launch.json")["pilot"])
    local = inventory("jack")
    dispatch_request = read(other / "entropy-replica-1-training/dispatch-0.json")
    native = Path(dispatch_request["native_root"])
    owned = []
    for started in (native / "jack/jobs").glob("*/started.json"):
        receipt = read(started)
        for process in local["active"]:
            if process["ProcessId"] == receipt["pid"] and str(started.parent).replace("\\", "/").lower() in process["CommandLine"].replace("\\", "/").lower():
                owned.append(process)
    assert owned, "replica one is not confirmed live; re-evaluate all currently eligible hosts"
    remote = inventory("haleyspc")
    assert not remote["active"]
    gpu = preflight("haleyspc", [place("haleyspc", 0, 1)])
    root.mkdir()
    write(root / "jack-inventory.json", local)
    write(root / "haley-inventory.json", dict(hardware=remote, gpu=gpu))
    cloud = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    available = dict(
        jack=dict(checked_at=local["at"], evidence=pin(root / "jack-inventory.json"), eligible=False,
            reason="Identity-verified healthy frozen replica one owns formal GPU1; preserve its execution. GPU0 retains desktop reservation.", devices=[]),
        haleyspc=dict(checked_at=remote["at"], evidence=pin(root / "haley-inventory.json"), eligible=True,
            reason="Idle sole GPU and native CPU; independent replica avoids waiting for Jack's ongoing measurement.",
            devices=[dict(ordinal=0, uuid=place("haleyspc", 0, 1)["gpu_uuid"], eligible=True, reason="Current device idle/free-VRAM check passed.")]),
        runpod=dict(checked_at=read(cloud)["checked_at"], evidence=pin(cloud), eligible=False,
            reason="Latest authenticated inventory HTTP403; no new paid compute authorized.", devices=[]))
    configs, binary = manifest["training_configs"], manifest["training_binary"]
    write(root / "manifest.json", dict(pilot=pin(pilot / "manifest.json"), concurrent_pilot=pin(other / "training-launch.json"),
        runner=pin(__file__), binary=binary, configs=configs, own_live_processes=owned,
        workers=[1, 4, 10], initial_updates=3, unique_arm_games=60, maximum_executed_games=180,
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_dispatch_v2.py", "compute_throughput_v2.py", "qualify_public_entropy_compute_v1.py"]],
        non_claim="Availability-conditioned placement. Jack is occupied by a verified healthy measurement, not assumed permanently unavailable."))
    reference, projections, candidates, learning = {}, {}, [], {}
    for count in [1, 4, 10]:
        placement = {arm: place("haleyspc", 0, count) for arm in configs}
        group_pin = dispatch_qualification(root / f"{root.name}-w{count}", binary, configs, placement, "sequential", updates=3)
        group = read(checked(group_pin))
        estimate = 0
        for arm, report in reports(group_pin).items():
            fingerprints = {name: pin(checked(item))["sha256"] for name, item in report["outputs"].items()}
            if arm in reference:
                assert reference[arm] == fingerprints, "collector scaling changes learning outputs"
            else:
                reference[arm] = fingerprints
                learning[arm] = audit_prefix(report)
            completion = read(checked(report["completion"]))
            execution = read(checked(report["execution"]))
            if count == 1:
                assert execution["seconds"] < 120, "cheap timing envelope exceeded"
            estimate += execution["seconds"] + 197 * sum(row["seconds"] for row in completion["receipts"][1:]) / 2
        label = f"haley-w{count}"
        projections[label] = estimate + group["staging_seconds"] + group["recovery_seconds"] * 200 / 3
        candidates.append(dict(id=label, benchmark=group_pin))
        print(label, "complete; projected replica seconds", round(projections[label], 2), flush=True)
    choice = dict(schema="public-training-allocation/v2", jobs=jobs_for(configs), inventory=available,
        candidates=candidates, selected=min(projections, key=projections.get))
    write(root / "compute-choice.json", choice)
    selected = require_allocation(root / "compute-choice.json", binary["sha256"], jobs_for(configs))
    write(root / "qualification.json", dict(complete=True, selected=selected, projections=projections,
        learning=learning, executed_games=180, unique_arm_games=60, exact_file_comparisons=144,
        full_training_launched=False, concurrent_pilot=pin(other / "training-launch.json")))


def run(root, pilot, scorer):
    m = read(pilot / "manifest.json")
    q = read(root / "qualification.json")
    qm = read(root / "manifest.json")
    assert q["complete"] and qm["pilot"] == pin(pilot / "manifest.json")
    for item in [qm["runner"], *qm["dependencies"], m["runner"], m["design"], *m["dependencies"]]:
        checked(item)
    # Recheck that the competing allocation still exists before treating Jack
    # as unavailable. If it ended, another local comparison can now be useful.
    other = checked(q["concurrent_pilot"]).parent
    local = inventory("jack")
    native = Path(read(other / "entropy-replica-1-training/dispatch-0.json")["native_root"])
    assert any(str(native).replace("\\", "/").lower() in p["CommandLine"].replace("\\", "/").lower() for p in local["active"]), "Jack's measured run ended; reconsider availability before launch"
    selected = require_allocation(root / "compute-choice.json", m["training_binary"]["sha256"], jobs_for(m["training_configs"]))
    assert selected == q["selected"] and selected["projected_seconds"] < 2700
    sq = read(scorer / "qualification.json")
    assert sq["complete"] and sq["original_gameplay_replays"] == sq["exact_new_entropy_bo3_replays"] == 4
    assert read(scorer / "manifest.json")["binary"] == m["evaluation_binary"]
    comparison = read(checked(sq["import_comparison"]))
    opponent = copy.deepcopy(m["evaluation_opponent"])
    opponent["source"]["play_import"] = comparison["source"]
    write(pilot / "evaluation-binding.json", dict(pilot=pin(pilot / "manifest.json"), binary=m["evaluation_binary"],
        qualification=pin(scorer / "qualification.json"), opponent=opponent))
    bound_opponent(pilot, m); statistical_checks()
    write(pilot / "training-launch.json", dict(pilot=pin(pilot / "manifest.json"), compute=pin(root / "qualification.json"),
        choice=pin(root / "compute-choice.json"), selected=selected, runner=pin(__file__),
        analysis=pin(Path(__file__).with_name("public_entropy_analysis_v1.py")),
        bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        evaluation_binding=pin(pilot / "evaluation-binding.json"), expected_natural_games=4000,
        native_wall_cap_seconds=1800, no_prefix_selection=True, no_paid_compute=True, review=m["review"]))
    telemetry = pilot / "training-telemetry"; telemetry.mkdir()
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
                if future.done(): break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m, pilot, remote_only=True)
    audited["wall_seconds_including_audit"] = time.monotonic()-started
    write(pilot / "training-audit.json", audited)
    print(dict(complete=True, natural_games=4000, root=str(pilot)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["qualify", "run"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    parser.add_argument("--other", type=Path)
    parser.add_argument("--scorer", type=Path)
    args = parser.parse_args()
    if not __debug__: raise RuntimeError("run with Python validation enabled")
    if args.mode == "qualify": qualify(args.root, args.pilot, args.other)
    else: run(args.root, args.pilot, args.scorer)
