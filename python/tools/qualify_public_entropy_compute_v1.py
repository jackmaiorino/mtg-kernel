"""Measure complete entropy/control prefixes on both PCs and three local stores."""
import argparse
import os
from pathlib import Path
import time

from public_training_dispatch_v2 import read, write, pin, checked, preflight
from public_evaluation_dispatch_v1 import inventory
from public_training_storage_v1 import storage, dispatch, require_storage_choice, ARCHIVE
from qualify_state_prevention_compute_v1 import place, reports


def audit(report):
    completion = read(checked(report["completion"]))
    assert completion["first_update"] == 0 and completion["next_update"] == 3
    assert len(completion["receipts"]) == 3
    zero = {0, 1 << 31}
    for index, receipt in enumerate(completion["receipts"]):
        assert receipt["update"] == index and receipt["episodes"] == receipt["natural_games"] == 10
        checkpoint = read(checked(report["outputs"][f"{index:04}/checkpoint.json"]))
        optimizer_pin = report["outputs"][f"{index:04}/optimizer.json"]
        optimizer = read(checked(optimizer_pin))
        assert optimizer["legacy_adam_step"] == 32401 + index
        assert optimizer["public"]["adam_step"] == index + 1
        assert checkpoint["optimizer_sha256"] == optimizer_pin["sha256"]
        for field in ["object", "object_first", "object_second", "state", "state_first", "state_second"]:
            assert set(optimizer["public"][field]) <= zero
        for episode in range(10):
            item = report["outputs"][f"{index:04}/episode-{episode:03}.json"]
            trajectory = read(checked(item))
            assert checkpoint["trajectory_sha256"][episode] == item["sha256"]
            assert trajectory["terminal"]["terminal_classification"] == "natural"
            assert trajectory["optimizer_state_sha256"] == receipt["before_state_sha256"]
    return dict(natural_games=30, complete_updates=3, public_disabled_zero=True,
                optimizer_continuation=True, checkpoint_trajectory_links_verified=True)


def run(root, pilot, reuse=None):
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    m = read(pilot / "manifest.json")
    assert m["schema"] == "matched-public-entropy/v1"
    binary, configs = m["training_binary"], m["training_configs"]
    assert set(configs) == {"control", "entropy"}
    for item in [binary, m["runner"], m["design"], *configs.values()]:
        checked(item)
    prior = None
    if reuse is not None:
        prior = read(reuse / "manifest.json")
        assert prior["pilot"] == pin(pilot / "manifest.json") and prior["binary"] == binary
        assert prior["configs"] == configs and prior["prefix_updates"] == 3
        for item in prior["dependencies"]:
            checked(item)
    root.mkdir()
    cuda = Path("C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8")
    os.environ["CUDA_PATH"] = str(cuda)
    os.environ["PATH"] = str(cuda / "bin") + os.pathsep + os.environ["PATH"]
    (root / "temp").mkdir()
    os.environ["TEMP"] = os.environ["TMP"] = str(root / "temp")
    hardware = {host: inventory(host) for host in ["jack", "haleyspc"]}
    assert all(not item["active"] for item in hardware.values()), "preserve competing owners"
    available = {}
    for host, device in [("jack", 1), ("haleyspc", 0)]:
        snapshot = preflight(host, [place(host, device, 1)])
        snapshot["hardware"] = hardware[host]
        path = root / f"{host}-inventory.json"
        write(path, snapshot)
        devices = [dict(ordinal=device, uuid=place(host, device, 1)["gpu_uuid"], eligible=True,
                        reason="Current idle/free-VRAM check passed; dedicated research device.")]
        if host == "jack":
            devices.append(dict(ordinal=0, uuid=place(host, 0, 1)["gpu_uuid"], eligible=False,
                reason="Preserve standing headless GPU1 assignment for formal training; desktop GPU0 not assigned to this run."))
        available[host] = dict(checked_at=snapshot["at"], eligible=True,
            reason="No competing native owner; BelowNormal local work authorized.", evidence=pin(path), devices=devices)
    cloud_path = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud = read(cloud_path)
    available["runpod"] = dict(checked_at=cloud["checked_at"], eligible=False, devices=[], evidence=pin(cloud_path),
        reason="Latest authenticated inventory HTTP403; no new paid compute authorized. No claimed available pod.")
    stores = {drive: storage(hardware["jack"], drive) for drive in ["C", "D", "E"]}
    cases = []
    for drive in ["C", "D", "E"]:
        for count in [1, 10]:
            cases.append((f"local-{drive.lower()}-w{count}", stores[drive],
                {arm: place("jack", 1, count) for arm in configs}, "sequential"))
    for count in [1, 4, 10]:
        cases.append((f"cross-d-w{count}", stores["D"],
            dict(control=place("jack", 1, count), entropy=place("haleyspc", 0, count)), "parallel"))
    write(root / "manifest.json", dict(pilot=pin(pilot / "manifest.json"), runner=pin(__file__), binary=binary,
        configs=configs, dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_storage_v1.py",
            "public_training_dispatch_v2.py", "public_evaluation_dispatch_v1.py", "compute_throughput_v2.py",
            "qualify_state_prevention_compute_v1.py"]],
        cases=[dict(id=name, storage=s, placements=p, mode=mode) for name, s, p, mode in cases],
        prefix_updates=3, games_per_case=60, maximum_executed_games=540, unique_arm_games=60,
        native_process_cap_seconds=300, remaining_case_launch_budget_seconds=1200,
        archive_scheme=ARCHIVE, full_training_launched=False,
        reused_completed_cases_from=None if reuse is None else pin(reuse / "manifest.json"),
        non_claim="Fixed-prefix engineering, no outcome selection. Same config hashes required by full launch guard."))
    started = time.monotonic()
    candidates, projections, reference, learning = [], {}, {}, {}
    comparisons = 0
    for label, store, placements, mode in cases:
        assert time.monotonic() - started < 1200, "qualification launch budget exhausted"
        old_group = None if reuse is None else reuse / f"{reuse.name}-{label}/group-benchmark.json"
        if old_group is not None and old_group.is_file():
            previous_case = next(case for case in prior["cases"] if case["id"] == label)
            assert previous_case["placements"] == placements and previous_case["mode"] == mode
            assert previous_case["storage"]["disk_serial"] == store["disk_serial"]
            group_pin = pin(old_group)
        else:
            # No incomplete case is imported. Fresh roots preserve any prior failure.
            group_pin = dispatch(root / f"{root.name}-{label}", binary, configs, placements, store, 3, mode=mode)
        group = read(checked(group_pin))
        times = []
        for arm, report in reports(group_pin).items():
            fingerprint = {name: pin(checked(item))["sha256"] for name, item in report["outputs"].items()}
            if arm in reference:
                assert reference[arm] == fingerprint, (label, arm, "learning bytes differ")
                comparisons += len(fingerprint)
            else:
                reference[arm] = fingerprint
                learning[arm] = audit(report)
            completion = read(checked(report["completion"]))
            execution = read(checked(report["execution"]))
            if not candidates:
                assert execution["seconds"] < 90, "cheap native timing envelope exceeded"
            steady = sum(row["seconds"] for row in completion["receipts"][1:]) / 2
            times.append(execution["seconds"] + 197 * steady)
        projections[label] = (max(times) if mode == "parallel" else sum(times)) + group["staging_seconds"] + group["recovery_seconds"] * 200 / 3
        candidates.append(dict(id=label, benchmark=group_pin))
        print(label, "complete; projected training/recovery seconds", round(projections[label], 2), flush=True)
    choice = dict(schema="public-training-allocation/v2",
        jobs={arm: dict(config_sha256=item["sha256"], updates=200) for arm, item in configs.items()},
        inventory=available, candidates=candidates, selected=min(projections, key=projections.get), archive_scheme=ARCHIVE,
        eligible_local_storage=[(s["drive"], s["disk_serial"]) for s in stores.values()],
        storage_dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_storage_v1.py", "public_evaluation_dispatch_v1.py"]])
    write(root / "compute-choice.json", choice)
    selected = require_storage_choice(root / "compute-choice.json", binary, configs)
    result = dict(complete=True, selected=selected, projections=projections, learning=learning,
        exact_allocation_file_comparisons=comparisons, executed_games=540, unique_arm_games=60,
        seconds=time.monotonic() - started, full_training_launched=False, archive_scheme=ARCHIVE,
        non_claim="No win-rate interpretation. Three initial batches cannot establish later steady-state cost; production telemetry still required.")
    write(root / "qualification.json", result)
    print(result, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    parser.add_argument("--reuse-complete", type=Path)
    args = parser.parse_args()
    run(args.root, args.pilot, args.reuse_complete)
