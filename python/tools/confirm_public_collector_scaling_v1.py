"""Counterbalanced collector replay timing at fixed early/middle/late batches."""
import argparse
import json
import statistics
import subprocess
import time
from pathlib import Path

from public_feature_pilot_v1 import execute, pin, read, verify, write
from qualify_public_collectors_v1 import check_gpu_idle, PILOT


def competing_builds():
    command = "@(Get-CimInstance Win32_Process | Where-Object { $_.Name -in @('cargo.exe','rustc.exe') } | Select-Object ProcessId,CreationDate,Name,CommandLine) | ConvertTo-Json -Compress"
    raw = subprocess.check_output(["powershell", "-NoProfile", "-Command", command], text=True).strip()
    return json.loads(raw) if raw else []


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    assert read(PILOT/"analysis.json")["complete"]
    prior = Path("E:/mtg-meta-recovery-20260920/public-collector-qualification-002")
    assert read(prior/"qualification.json")["all_worker_counts_byte_identical"]
    m = read(prior/"manifest.json")
    binary = m["training_binary"]
    verify(binary)
    config = m["training_configs"]["structured"]
    verify(config)
    root.mkdir()
    for directory in ["requests", "outputs", "logs"]:
        (root/directory).mkdir()
    schedule = [(0, [1, 4, 8]), (99, [4, 8, 1]), (197, [8, 1, 4])]
    manifest = dict(schema="public-collector-counterbalanced-timing/v1", runner=pin(__file__),
        binary=binary, config=config, schedule=schedule, process_cap_seconds=90,
        total_wall_cap_seconds=600, prior=pin(prior/"qualification.json"),
        purpose="Replay engineering only, no new training endpoint or outcome selection",
        dependencies=[pin(Path(__file__).with_name(name)) for name in
                      ["public_feature_pilot_v1.py", "qualify_public_collectors_v1.py"]])
    write(root/"manifest.json", manifest)
    started = time.monotonic()
    results = []
    for first, workers_list in schedule:
        for workers in workers_list:
            label = f"u{first:03}-w{workers}"
            gpu = check_gpu_idle()
            competitors = competing_builds()
            assert not competitors, "Wait for competing build before qualification; preserve partial root"
            write(root/f"logs/{label}.resources.json", dict(gpu=gpu, before=competitors))
            request = root/f"requests/{label}.json"
            resume = pin(PILOT/f"outputs/structured/{first-1:04}/checkpoint.json") if first else None
            write(request, dict(config=read(verify(config)), collector_workers=workers,
                output_directory=str(root/f"outputs/{label}"), stop_after=first+3, resume=resume))
            remaining = 600-(time.monotonic()-started)
            assert remaining > 0
            ex = execute(root, label, binary, request, min(90, remaining))
            after = competing_builds()
            completion = read(root/f"outputs/{label}/completion.json")
            assert completion["first_update"] == first and completion["next_update"] == first+3
            receipts = completion["receipts"]
            assert len(receipts) == 3
            for offset, receipt in enumerate(receipts):
                update = first+offset
                assert receipt["update"] == update and receipt["episodes"] == receipt["natural_games"] == 10
                original = PILOT/f"outputs/{'structured-prefix' if update < 2 else 'structured'}/{update:04}"
                for name in ["checkpoint.json", "optimizer.json"]+[f"episode-{i:03}.json" for i in range(10)]:
                    assert pin(root/f"outputs/{label}/{update:04}/{name}")["sha256"] == pin(original/name)["sha256"], f"replay differs: {label}/{update}/{name}"
            result = dict(first_update=first, workers=workers, process_seconds=ex["seconds"],
                update_seconds=sum(r["seconds"] for r in receipts),
                collection_seconds=sum(r["collection_seconds"] for r in receipts),
                competing_builds_after=after, all_36_outputs_byte_identical=True)
            results.append(result)
            write(root/f"{label}.result.json", result)
            print(json.dumps(result), flush=True)
    aggregates = {str(w): dict(process_seconds=sum(r["process_seconds"] for r in results if r["workers"] == w),
        update_seconds=sum(r["update_seconds"] for r in results if r["workers"] == w)) for w in [1,4,8]}
    clean = all(not r["competing_builds_after"] for r in results)
    write(root/"result.json", dict(complete=True, natural_games=270, updates=27, results=results,
        aggregates=aggregates, checked_builds_absent=clean, wall_seconds=time.monotonic()-started,
        global_placement_qualified=False,
        limitations="Three fixed windows, one execution per cell. Order balanced across windows. Before/after build inventory can miss brief contention; other processes remain possible. No strength or remote placement claim."))
    print(json.dumps(aggregates), flush=True)


if __name__ == "__main__":
    main()
