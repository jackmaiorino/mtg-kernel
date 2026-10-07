"""Guarded full training for the already prepared, matched prevention pilot."""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import subprocess
import time

from compute_throughput_v2 import require_allocation
from public_training_dispatch_v2 import read, write, pin, checked, dispatch_qualified, ssh_ps
from state_prevention_analysis_v1 import statistical_checks


def snapshot(host):
    script = r'''$ErrorActionPreference='Stop'
$cpu=Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor | Where-Object Name -eq '_Total'
$os=Get-CimInstance Win32_OperatingSystem
$processes=@(Get-Process -Name trainer -ErrorAction SilentlyContinue | Select-Object Id,CPU,StartTime,Path)
[pscustomobject]@{at=(Get-Date).ToUniversalTime().ToString('o');host=$env:COMPUTERNAME;cpu_percent=$cpu.PercentProcessorTime;free_memory_kib=$os.FreePhysicalMemory;native=$processes;gpu=@(& nvidia-smi --query-gpu=index,uuid,utilization.gpu,memory.used --format=csv,noheader,nounits)} | ConvertTo-Json -Depth 4'''
    if host == "computehost":
        import json
        return json.loads(ssh_ps(script))
    import json
    encoded = base64.b64encode(script.encode("utf-16le")).decode()
    return json.loads(subprocess.check_output(["powershell", "-NoProfile", "-EncodedCommand", encoded], text=True, timeout=30))


def audit(group_pin, manifest):
    group = read(checked(group_pin))
    assert set(group["jobs"]) == {"control", "structured"}
    arms = {}
    for arm, report_pin in group["jobs"].items():
        report = read(checked(report_pin))
        assert report["config"] == manifest["training_configs"][arm]
        execution = read(checked(report["execution"]))
        assert execution["exit_code"] == 0 and not execution["timeout"]
        assert execution["observed_gpu_uuid"] == report["placement"]["gpu_uuid"]
        completion = read(checked(report["completion"]))
        assert completion["first_update"] == 0 and completion["next_update"] == 200
        assert len(completion["receipts"]) == 200
        expected = set()
        zero = {0, 1 << 31}
        for update, receipt in enumerate(completion["receipts"]):
            assert receipt["update"] == update and receipt["episodes"] == receipt["natural_games"] == 10
            names = ["checkpoint.json", "optimizer.json"]+[f"episode-{i:03}.json" for i in range(10)]
            expected.update(f"{update:04}/{name}" for name in names)
            checkpoint = read(checked(report["outputs"][f"{update:04}/checkpoint.json"]))
            assert checkpoint["optimizer_sha256"] == report["outputs"][f"{update:04}/optimizer.json"]["sha256"]
            assert checkpoint["trajectory_sha256"] == [report["outputs"][f"{update:04}/episode-{i:03}.json"]["sha256"] for i in range(10)]
        assert set(report["outputs"]) == expected
        for item in report["outputs"].values():
            checked(item)
        final = read(checked(report["outputs"]["0199/optimizer.json"]))
        assert final["legacy_adam_step"] == 32600 and final["public"]["adam_step"] == 200
        for field in ["object", "object_first", "object_second"]:
            assert set(final["public"][field]) <= zero
        learned = {field: sum(v not in zero for v in final["public"][field])
                   for field in ["state", "state_first", "state_second"]}
        assert all(v > 0 for v in learned.values()) if arm == "structured" else all(v == 0 for v in learned.values())
        arms[arm] = dict(complete=True, updates=200, natural_games=2000, legacy_adam_step=32600,
            public_adam_step=200, checkpoint=report["outputs"]["0199/checkpoint.json"],
            optimizer=report["outputs"]["0199/optimizer.json"], state_learning=learned,
            seconds=execution["seconds"], placement=report["placement"], report=report_pin)
    return dict(complete=True, arms=arms, group=group_pin, full_natural_games=4000,
        non_claim="Completed matched training only. No outcome interpretation before the complete frozen BO3 panel.")


def run(root, compute):
    m = read(root/"manifest.json")
    qualification = read(compute/"qualification.json")
    assert qualification["complete"] and not qualification["full_training_launched"]
    cm = read(compute/"manifest.json")
    assert checked(cm["pilot"]).resolve() == (root/"manifest.json").resolve()
    for item in [cm["runner"], *cm["dependencies"], m["runner"], *m["dependencies"], m["design"]]:
        checked(item)
    assert all(row["exposed_games"] >= 2 for row in qualification["exposure"].values())
    jobs = {arm: dict(config_sha256=item["sha256"], updates=200) for arm, item in m["training_configs"].items()}
    selected = require_allocation(compute/"compute-choice.json", m["training_binary"]["sha256"], jobs)
    assert selected == qualification["selected"]
    assert selected["projected_seconds"] < 1800, "qualified projection exceeds this bounded pilot scope"
    statistical_checks()
    write(root/"training-launch.json", dict(pilot=pin(root/"manifest.json"),
        compute_choice=pin(compute/"compute-choice.json"), compute_qualification=pin(compute/"qualification.json"),
        allocation=selected, runner=pin(__file__), analysis=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        statistical_checks="Both cohort pairing, exact fixed effect and deterministic resampling passed.",
        native_wall_cap_seconds=2400, allocation_projection_cap_seconds=1800,
        expected_updates_per_arm=200, expected_natural_games=4000,
        training_binary=m["training_binary"], evaluation_binary=m["evaluation_binary"],
        native_build=pin(Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/build-completion.json")),
        review=m["review"], no_paid_allocation=True, promotion=False))
    telemetry = root/"training-telemetry"
    telemetry.mkdir()
    hosts = sorted({p["host"] for p in selected["placements"].values()})
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(dispatch_qualified, root/"state-prevention-training-001", m["training_binary"],
            m["training_configs"], compute/"compute-choice.json", 2400)
        tick = 0
        while not future.done():
            record = dict(elapsed_seconds=time.monotonic()-started, hosts={})
            for host in hosts:
                try:
                    record["hosts"][host] = snapshot(host)
                except Exception as error:
                    record["hosts"][host] = dict(telemetry_error=str(error))
            if "desktop" in hosts:
                jobs_root = root/"state-prevention-training-001/desktop/jobs"
                record["local_progress"] = {}
                for arm in m["training_configs"]:
                    receipts = sorted((jobs_root/arm/"outputs").glob("*/receipt.json"))
                    row = dict(completed_updates=len(receipts))
                    if receipts:
                        last = read(receipts[-1])
                        row.update(last_update_seconds=last["seconds"], last_natural_games=last["natural_games"])
                    stderr = jobs_root/arm/"stderr"
                    if stderr.exists():
                        with stderr.open("rb") as stream:
                            stream.seek(max(0, stderr.stat().st_size-2048))
                            row["recent_stage"] = stream.read().decode(errors="replace").splitlines()[-4:]
                    record["local_progress"][arm] = row
            write(telemetry/f"{tick:04}.json", record)
            print("training progress", round(record["elapsed_seconds"]), record.get("local_progress", {}), flush=True)
            tick += 1
            for _ in range(30):
                if future.done():
                    break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m)
    audited["wall_seconds_including_dispatch_and_audit"] = time.monotonic()-started
    write(root/"training-audit.json", audited)
    print(audited, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--compute", type=Path, required=True)
    args = parser.parse_args()
    run(args.root, args.compute)
