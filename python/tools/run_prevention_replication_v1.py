"""Run the frozen replication through storage-qualified training and archival."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import subprocess
import shutil
import time

from public_training_dispatch_v2 import read, write, pin, checked
from public_training_storage_v1 import require_storage_choice, dispatch_qualified
from run_state_prevention_training_v1 import audit, snapshot
from state_prevention_analysis_v1 import statistical_checks


def run(root, compute):
    if not __debug__:
        raise RuntimeError("optimized Python disables dependency validation; run without -O")
    m = read(root/"manifest.json")
    if m["schema"] != "matched-state-prevention-replication/v1":
        raise ValueError("requires the independently seeded replication")
    qualification = read(compute/"qualification.json")
    guard_checks = read(compute/"guard-checks.json")
    if not guard_checks["complete"] or not guard_checks["valid_choice_accepted"] or not all(
            item["rejected"] for item in guard_checks["rejected_invalid_choices"].values()):
        raise ValueError("storage launch rejection checks have not passed")
    cm = read(compute/"manifest.json")
    if not qualification["complete"] or qualification["full_training_launched"]:
        raise ValueError("incomplete qualification")
    if checked(cm["pilot"]).resolve() != (root/"manifest.json").resolve():
        raise ValueError("qualification belongs to another pilot")
    for item in [cm["runner"], *cm["dependencies"], m["runner"], *m["dependencies"], m["design"]]:
        checked(item)
    if not all(row["exposed_games"] >= 2 for row in qualification["exposure"].values()):
        raise ValueError("insufficient natural exposure in the fixed prefix")
    selected = require_storage_choice(compute/"compute-choice.json", m["training_binary"], m["training_configs"])
    if selected != qualification["selected"] or selected["projected_seconds"] >= 1800:
        raise ValueError("allocation mismatch or projection exceeds bounded scope")
    statistical_checks()
    dispatch_root = root/"state-prevention-replication-training-002"
    write(root/"training-launch.json", dict(pilot=pin(root/"manifest.json"),
        compute_choice=pin(compute/"compute-choice.json"), compute_qualification=pin(compute/"qualification.json"),
        guard_checks=pin(compute/"guard-checks.json"),
        allocation=selected, runner=pin(__file__), dependencies=[pin(Path(__file__).with_name(name)) for name in [
            "public_training_storage_v1.py", "run_state_prevention_training_v1.py", "state_prevention_analysis_v1.py"]],
        native_wall_cap_seconds=2400, allocation_projection_cap_seconds=1800,
        expected_updates_per_arm=200, expected_natural_games=4000,
        training_binary=m["training_binary"], evaluation_binary=m["evaluation_binary"],
        review=m["review"], no_paid_allocation=True, promotion=False))
    telemetry = root/"training-telemetry"
    telemetry.mkdir()
    hosts = sorted({p["host"] for p in selected["placements"].values()})
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(dispatch_qualified, dispatch_root, m["training_binary"],
            m["training_configs"], compute/"compute-choice.json", 2400)
        tick = 0
        while not future.done():
            record = dict(elapsed_seconds=time.monotonic()-started, hosts={}, local_progress={})
            for host in hosts:
                try:
                    record["hosts"][host] = snapshot(host)
                except Exception as error:
                    record["hosts"][host] = dict(telemetry_error=str(error))
            attempts = sorted(dispatch_root.glob("dispatch-*.json"))
            if attempts:
                native = Path(read(attempts[-1])["native_root"])
                for arm in m["training_configs"]:
                    folder = native/"jack/jobs"/arm/"outputs"
                    receipts = sorted(folder.glob("*/receipt.json"))
                    row = dict(completed_updates=len(receipts))
                    if receipts:
                        last = read(receipts[-1])
                        row.update(last_update_seconds=last["seconds"], last_natural_games=last["natural_games"])
                    record["local_progress"][arm] = row
            try:
                record["disk"] = subprocess.check_output(["powershell", "-NoProfile", "-Command",
                    "Get-CimInstance Win32_PerfFormattedData_PerfDisk_PhysicalDisk | Select-Object Name,DiskWriteBytesPersec,DiskReadBytesPersec,PercentDiskTime,CurrentDiskQueueLength | ConvertTo-Json"],
                    text=True, timeout=20).strip()
            except Exception as error:
                record["disk_error"] = str(error)
            write(telemetry/f"{tick:04}.json", record)
            print("replication progress", round(record["elapsed_seconds"]), record["local_progress"], flush=True)
            tick += 1
            for _ in range(30):
                if future.done():
                    break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m)
    group = read(checked(result))
    archived = read(checked(group["archive"]))
    if archived["mismatches"] != 0:
        raise ValueError("archive verification failed")
    for shard in archived["shards"]:
        checked(shard["archive"])
    # Portable canonical endpoints retain byte identity and their native mapping.
    for arm, evidence in audited["arms"].items():
        folder = root/"endpoints"/arm
        folder.mkdir(parents=True)
        for field in ["checkpoint", "optimizer"]:
            source = evidence[field]
            destination = folder/f"{field}.json"
            shutil.copy2(checked(source), destination)
            copied = pin(destination)
            if copied["sha256"] != source["sha256"]:
                raise ValueError("canonical endpoint copy differs")
            evidence["native_"+field] = source
            evidence[field] = copied
    audited.update(archive=group["archive"], local_storage=selected["local_storage"],
        wall_seconds_including_dispatch_and_audit=time.monotonic()-started)
    write(root/"training-audit.json", audited)
    print(audited, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--compute", type=Path, required=True)
    args = parser.parse_args()
    run(args.root, args.compute)
