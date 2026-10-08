"""Full-batch placement timing and exposure audit; never starts full training."""
import argparse
import copy
from pathlib import Path
import time

from public_training_dispatch_v2 import (pin, read, write, checked, dispatch_qualification,
    preflight, worker)
from compute_throughput_v2 import require_allocation

UUIDS = {"desktop": {0: "GPU-3502709e-6aef-8ed7-4abe-562838793e3d", 1: "GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3"},
         "computehost": {0: "GPU-17eeab7a-ad2e-5a59-4c42-8cd26a0d7223"}}


def place(host, device, workers):
    return dict(host=host, gpu_ordinal=device, gpu_uuid=UUIDS[host][device], workers=workers)


def reports(group_pin):
    return {arm: read(checked(item)) for arm, item in read(checked(group_pin))["jobs"].items()}


def audit(report, arm):
    completion = read(checked(report["completion"]))
    folder = Path(report["local_output_directory"])
    exposures = [0]*6
    exposed_games = active_rows = 0
    zero = {0, 1 << 31}
    for index, receipt in enumerate(completion["receipts"]):
        assert receipt["update"] == index and receipt["episodes"] == receipt["natural_games"] == 10
        checkpoint = read(checked(report["outputs"][f"{index:04}/checkpoint.json"]))
        saved = read(checked(report["outputs"][f"{index:04}/optimizer.json"]))
        assert saved["legacy_adam_step"] == 32401+index and saved["public"]["adam_step"] == index+1
        assert pin(folder/f"{index:04}/optimizer.json")["sha256"] == checkpoint["optimizer_sha256"]
        public = saved["public"]
        for field in ["object", "object_first", "object_second"]:
            assert set(public[field]) <= zero
        if arm == "control":
            for field in ["state", "state_first", "state_second"]:
                assert set(public[field]) <= zero
        for episode in range(10):
            path = checked(report["outputs"][f"{index:04}/episode-{episode:03}.json"])
            assert pin(path)["sha256"] == checkpoint["trajectory_sha256"][episode]
            trajectory = read(path)
            # Public rows are learner-only in this schema; other opponent schemas also fill opponent rows.
            assert trajectory["schema"] == "mtg-kernel-public-input-trajectory/v1"
            assert trajectory["terminal"]["terminal_classification"] == "natural"
            assert trajectory["optimizer_state_sha256"] == receipt["before_state_sha256"]
            rows = [item["state"] for item in trajectory["auxiliary"] if item is not None]
            active = sum(any(v != 0 for v in row) for row in rows)
            active_rows += active
            exposed_games += int(active > 0)
            for flag in range(6):
                exposures[flag] += sum(row[flag] != 0 for row in rows)
    assert exposed_games >= 2 and active_rows > 0, "fresh schedule lacks multiple exposed natural games"
    learned = {field: sum(v not in zero for v in public[field]) for field in ["state", "state_first", "state_second"]}
    if arm == "structured":
        assert all(learned.values()), "state projection did not learn"
    return dict(exposed_games=exposed_games, active_rows=active_rows,
        flags_white_blue_black_red_green_cannot_prevent=exposures, nonzero_parameters_and_moments=learned,
        natural_games=10*len(completion["receipts"]))


def run(root, pilot):
    root.mkdir()
    m = read(pilot/"manifest.json")
    binary, configs = m["training_binary"], m["training_configs"]
    for item in [binary, *configs.values()]:
        checked(item)
    jobs = {arm: dict(config_sha256=item["sha256"], updates=200) for arm, item in configs.items()}
    inventory = {}
    for host, devices in [("desktop", [0, 1]), ("computehost", [0])]:
        snapshot = preflight(host, [place(host, d, 1) for d in devices])
        path = root/f"{host}-inventory.json"
        write(path, snapshot)
        inventory[host] = dict(checked_at=snapshot["at"], eligible=True,
            reason="Idle and available under whole-PC assignment; normal desktop use preserved.", evidence=pin(path),
            devices=[dict(ordinal=d, uuid=UUIDS[host][d], eligible=True, reason="Idle with adequate free VRAM.") for d in devices])
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    inventory["runpod"] = dict(checked_at=cloud["checked_at"], eligible=False,
        reason="Latest authenticated read-only inventory returned HTTP403; no paid allocation.", evidence=pin(cloud_path), devices=[])
    # Ten games bound useful collectors to ten. Each stream stays on one host.
    cases = []
    for workers in [1, 4, 8, 10]:
        cases.append((f"local-w{workers}", dict(control=place("desktop", 1, workers), structured=place("desktop", 0, workers))))
        cases.append((f"cross-w{workers}", dict(control=place("desktop", 1, workers), structured=place("computehost", 0, workers))))
    write(root/"manifest.json", dict(pilot=pin(pilot/"manifest.json"), runner=pin(__file__),
        dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_training_dispatch_v2.py", "compute_throughput_v2.py", "compute_throughput_v1.py"]],
        configs=configs, binary=binary, allocations=[dict(label=label, placements=p) for label, p in cases],
        prefix_updates=3, cheap_updates=1, expected_unique_arm_games=60, maximum_executed_games=540,
        total_wall_cap_seconds=1200, maximum_native_process_seconds=300,
        question="Which measured allocation finishes the unchanged ten-game synchronous workload fastest, with actual prevention exposure and exact saved learning outputs?",
        no_outcome_selection=True, full_training_launched=False))
    started = time.monotonic()
    cheap = dispatch_qualification(root/"state-prevention-cheap-001", binary, configs, cases[0][1], "parallel", updates=1)
    assert max(read(checked(r["execution"]))["seconds"] for r in reports(cheap).values()) < 90, "cheap runtime exceeds qualification envelope"
    candidates, projections, reference, exposure = [], {}, {}, {}
    compared = 0
    for label, placements in cases:
        assert time.monotonic()-started < 900, "remaining qualification budget too small"
        group_pin = dispatch_qualification(root/f"state-prevention-{label}-001", binary, configs, placements, "parallel", updates=3)
        group = read(checked(group_pin))
        times = []
        for arm, report in reports(group_pin).items():
            fingerprints = {name: pin(checked(item))["sha256"] for name, item in report["outputs"].items()}
            if arm in reference:
                assert fingerprints == reference[arm], (label, arm, "saved learning outputs differ")
                compared += len(fingerprints)
            else:
                reference[arm] = fingerprints
                exposure[arm] = audit(report, arm)
            completion = read(checked(report["completion"]))
            execution = read(checked(report["execution"]))
            steady = sum(row["seconds"] for row in completion["receipts"][1:])/2
            times.append(execution["seconds"]+197*steady)
        projections[label] = max(times)+group["staging_seconds"]+group["recovery_seconds"]*200/3
        candidates.append(dict(id=label, benchmark=group_pin))
        print(label, "complete, projected group seconds", round(projections[label], 2), flush=True)
    # Replay updates1..2 from the independently collected cheap checkpoint.
    replay = root/"restart"
    replay.mkdir()
    cheap_reports = reports(cheap)
    replay_comparisons = 0
    for arm in ["control", "structured"]:
        folder = replay/arm
        folder.mkdir()
        placement = place("desktop", 1, 8)
        write(folder/"preflight.json", preflight("desktop", [placement]))
        request = dict(config=read(checked(configs[arm])), output_directory=str(folder/"outputs"),
            resume=cheap_reports[arm]["outputs"]["0000/checkpoint.json"], stop_after=3,
            collector_workers=8, execution_gpu_ordinal=1)
        write(folder/"request.json", request)
        worker(dict(placement=placement, wall_seconds=300, binary=binary, request=pin(folder/"request.json")))
        completion = read(folder/"outputs/completion.json")
        assert completion["first_update"] == 1 and completion["next_update"] == 3
        for name, digest in reference[arm].items():
            if name.startswith("0000/"):
                assert cheap_reports[arm]["outputs"][name]["sha256"] == digest
            else:
                assert pin(folder/"outputs"/name)["sha256"] == digest
            replay_comparisons += 1
    assert time.monotonic()-started < 1200
    choice = dict(schema="public-training-allocation/v2", jobs=jobs, inventory=inventory,
                  candidates=candidates, selected=min(projections, key=projections.get))
    write(root/"compute-choice.json", choice)
    selected = require_allocation(root/"compute-choice.json", binary["sha256"], jobs)
    result = dict(complete=True, selected=selected, projections=projections, exposure=exposure,
        exact_allocation_file_comparisons=compared, exact_restart_file_comparisons=replay_comparisons,
        seconds=time.monotonic()-started, full_training_launched=False,
        non_claim="Timing and learning-path qualification only. No win-rate interpretation, endpoint selection or promotion. Three initial full batches do not guarantee constant runtime later; monitor production.")
    write(root/"qualification.json", result)
    print(result, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    args = parser.parse_args()
    run(args.root, args.pilot)
