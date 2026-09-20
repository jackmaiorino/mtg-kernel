"""Bounded local serial/parallel full-batch parity and throughput qualification."""
import argparse
import json
import shutil
import subprocess
import time
from pathlib import Path

from public_feature_pilot_v1 import audit_training, execute, read, write, pin, verify

PILOT = Path("E:/mtg-postboard-campaign-20260920/public-feature-pilot-001")
TOOLS = Path("E:/mtg-meta-recovery-20260920/public-collector-tools-001")
ARMS = ["control", "structured"]
WORKERS = [1, 2, 4, 8]


def prepare(root):
    build = read(TOOLS/"build-completion.json")
    assert build["exit_code"] == 0
    binary = verify(build["binary"])
    original = read(PILOT/"manifest.json")
    root.mkdir()
    for name in ["outputs", "requests", "logs", "configs"]:
        (root/name).mkdir()
    shutil.copy2(binary, root/binary.name)
    configs = {}
    for arm in ARMS:
        source = verify(original["training_configs"][arm])
        shutil.copy2(source, root/f"configs/{arm}.json")
        configs[arm] = pin(root/f"configs/{arm}.json")
    write(root/"manifest.json", dict(schema="public-collector-qualification/v1",
        runner=pin(__file__), dependencies=[pin(Path(__file__).with_name(name)) for name in
            ["public_feature_pilot_v1.py", "compute_throughput_v1.py", "qualify_public_evaluation_v1.py"]],
        build=pin(TOOLS/"build-completion.json"), training_binary=pin(root/binary.name),
        original_manifest=pin(PILOT/"manifest.json"), training_configs=configs,
        workers=WORKERS, updates=3, games_per_update=10, native_process_cap_seconds=90,
        total_wall_cap_seconds=480, old_output_comparison=True,
        toolchain={tool: subprocess.check_output([tool, "--version"], text=True).strip()
                   for tool in ["rustc", "cargo"]},
        linker_note="Windows MSVC; installed linker14.50.35725.0 was independently inspected. Exact build-time patch was not separately captured.",
        non_claim="Engineering parity and completed-batch throughput only. No strength, promotion or global resource allocation claim."))


def check_gpu_idle():
    gpu = subprocess.check_output(["nvidia-smi", "--query-gpu=index,uuid", "--format=csv,noheader"], text=True)
    uuid = next(row.split(",")[1].strip() for row in gpu.splitlines() if row.split(",")[0].strip() == "1")
    processes = subprocess.check_output(["nvidia-smi", "--query-compute-apps=pid,gpu_uuid", "--format=csv,noheader"], text=True)
    assert all(uuid not in row for row in processes.splitlines()), "GPU1 still has a compute process; preserve its run"


def run(root):
    m = read(root/"manifest.json")
    assert pin(__file__)["sha256"] == m["runner"]["sha256"]
    for dependency in m["dependencies"]:
        verify(dependency)
    # Formal evidence resolves first; do not compete with its GPU or evaluate
    # outcome prefixes. Reading complete audit counts does not select winners.
    for arm in ARMS:
        assert read(PILOT/f"{arm}-training-audit.json")["complete"]
    assert read(PILOT/"analysis.json")["complete"]
    check_gpu_idle()
    started = time.monotonic()
    results = []
    baseline = {}

    def launch(arm, workers, label, stop=3, resume=None):
        remaining = m["total_wall_cap_seconds"]-(time.monotonic()-started)
        assert remaining > 0, "qualification wall cap reached"
        request = root/f"requests/{label}.json"
        write(request, dict(config=read(verify(m["training_configs"][arm])), collector_workers=workers,
            output_directory=str(root/f"outputs/{label}"), stop_after=stop, resume=resume))
        return execute(root, label, m["training_binary"], request,
                       min(m["native_process_cap_seconds"], remaining))

    for arm in ARMS:
        for workers in m["workers"]:
            label = f"{arm}-w{workers}"
            ex = launch(arm, workers, label)
            audit = audit_training(root, arm, [label], 3)
            folder = root/f"outputs/{label}"
            completion = read(folder/"completion.json")
            outputs = {str(path.relative_to(folder)).replace("\\", "/"): pin(path)
                       for batch in sorted(folder.glob("[0-9][0-9][0-9][0-9]"))
                       for path in sorted(batch.glob("*.json")) if path.name != "receipt.json"}
            assert len(outputs) == 3*12
            fingerprint = {name: item["sha256"] for name,item in outputs.items()}
            if workers == 1:
                baseline[arm] = fingerprint
                for name, digest in fingerprint.items():
                    update = int(name[:4])
                    old = PILOT/f"outputs/{arm+'-prefix' if update < 2 else arm}"/name
                    assert pin(old)["sha256"] == digest, f"new serial output differs from frozen run: {arm}/{name}"
            else:
                assert fingerprint == baseline[arm], f"parallel output differs: {label}"
            report_path = root/f"{label}-benchmark.json"
            write(report_path, dict(host="jack", config=m["training_configs"][arm],
                execution=pin(root/f"logs/{label}.execution.json"), completion=pin(folder/"completion.json"), outputs=outputs))
            steady = sum(row["seconds"] for row in completion["receipts"][1:])/2
            result = dict(arm=arm, workers=workers, native_seconds=ex["seconds"],
                steady_update_seconds=steady, projected_200_update_seconds=ex["seconds"]+197*steady,
                collection_seconds=sum(row["collection_seconds"] for row in completion["receipts"]),
                benchmark=pin(report_path), updates=audit["updates"], games=audit["games"])
            results.append(result)
            print(json.dumps(result), flush=True)

    fastest = min((r for r in results if r["arm"] == "structured"), key=lambda r:r["projected_200_update_seconds"])
    workers = fastest["workers"]
    label = "structured-resume"
    prefix = root/f"outputs/structured-w{workers}/0000/checkpoint.json"
    launch("structured", workers, label, resume=pin(prefix))
    resumed = read(root/f"outputs/{label}/completion.json")
    assert resumed["first_update"] == 1 and resumed["next_update"] == 3
    assert len(resumed["receipts"]) == 2 and all(r["natural_games"] == r["episodes"] == 10 for r in resumed["receipts"])
    for name,digest in baseline["structured"].items():
        if int(name[:4]) >= 1:
            assert pin(root/f"outputs/{label}"/name)["sha256"] == digest, f"resumed output differs: {name}"
    elapsed = time.monotonic()-started
    assert elapsed < m["total_wall_cap_seconds"]
    result = dict(status="PUBLIC-COLLECTOR-PARITY-AND-THROUGHPUT-PASS", results=results,
        executed_updates=26, natural_games=260, resumed_workers=workers, wall_seconds=elapsed,
        serial_equals_frozen_outputs=True, all_worker_counts_byte_identical=True,
        fresh_process_resume_byte_identical=True, global_placement_qualified=False,
        limitations=["Three initial batches per arm; steady timing uses two batches, not a full campaign.",
            "One local GPU and storage path. HaleysPC and RunPod placement remain unqualified.",
            "Order is serial,2,4,8 workers; resource/cache drift can affect timings. No playing-strength claim."])
    write(root/"qualification.json", result)
    print(json.dumps(result), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare", "run"])
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    (prepare if args.mode == "prepare" else run)(args.root.resolve())


if __name__ == "__main__":
    main()
