"""Storage/throughput-guarded entropy continuation with complete output audit."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import os
import shutil
import time

from public_training_dispatch_v2 import read, write, pin, checked
from public_training_storage_v1 import require_storage_choice, dispatch_qualified
from run_state_prevention_training_v1 import snapshot
from public_entropy_analysis_v1 import statistical_checks, bound_opponent


def audit(group_pin, manifest, root):
    group = read(checked(group_pin))
    assert set(group["jobs"]) == {"control", "entropy"}
    archived = read(checked(group["archive"]))
    assert archived["mismatches"] == 0
    for shard in archived["shards"]:
        checked(shard["archive"])
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
        for index, receipt in enumerate(completion["receipts"]):
            assert receipt["update"] == index and receipt["episodes"] == receipt["natural_games"] == 10
            expected.update(f"{index:04}/{name}" for name in ["checkpoint.json", "optimizer.json"] + [f"episode-{i:03}.json" for i in range(10)])
            checkpoint = read(checked(report["outputs"][f"{index:04}/checkpoint.json"]))
            assert checkpoint["optimizer_sha256"] == report["outputs"][f"{index:04}/optimizer.json"]["sha256"]
            assert checkpoint["trajectory_sha256"] == [report["outputs"][f"{index:04}/episode-{i:03}.json"]["sha256"] for i in range(10)]
        assert set(report["outputs"]) == expected
        for item in report["outputs"].values():
            checked(item)
        final = read(checked(report["outputs"]["0199/optimizer.json"]))
        assert final["legacy_adam_step"] == 32600 and final["public"]["adam_step"] == 200
        for field in ["object", "object_first", "object_second", "state", "state_first", "state_second"]:
            assert set(final["public"][field]) <= {0, 1 << 31}
        endpoint = root / "endpoints" / arm
        endpoint.mkdir(parents=True)
        pins = {}
        for field in ["checkpoint", "optimizer"]:
            original = report["outputs"][f"0199/{field}.json"]
            shutil.copy2(checked(original), endpoint / f"{field}.json")
            pins[field] = pin(endpoint / f"{field}.json")
            assert pins[field]["sha256"] == original["sha256"]
        arms[arm] = dict(complete=True, updates=200, natural_games=2000, legacy_adam_step=32600,
            public_adam_step=200, **pins, report=report_pin, seconds=execution["seconds"], placement=report["placement"])
    return dict(complete=True, full_natural_games=4000, arms=arms, group=group_pin, archive=group["archive"],
        non_claim="Completed learning only. Do not select or claim strength before both complete independent BO3 panels.")


def run(root, compute, scorer_qualification):
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    m = read(root / "manifest.json")
    assert m["schema"] == "matched-public-entropy/v1"
    qm = read(compute / "manifest.json")
    q = read(compute / "qualification.json")
    assert q["complete"] and not q["full_training_launched"]
    assert qm["pilot"] == pin(root / "manifest.json")
    for item in [m["runner"], m["design"], *m["dependencies"], qm["runner"], *qm["dependencies"]]:
        checked(item)
    scorer = read(scorer_qualification / "qualification.json")
    scorer_manifest = read(scorer_qualification / "manifest.json")
    assert scorer["complete"] and scorer["original_gameplay_replays"] == scorer["exact_new_entropy_bo3_replays"] == 4
    assert scorer_manifest["binary"] == m["evaluation_binary"]
    checked(scorer_manifest["binary"])
    selected = require_storage_choice(compute / "compute-choice.json", m["training_binary"], m["training_configs"])
    assert selected == q["selected"]
    assert selected["projected_seconds"] < 1350, "reserve time for second replica and evaluation within bounded design"
    statistical_checks()
    comparison = read(checked(scorer["import_comparison"]))
    opponent = copy.deepcopy(m["evaluation_opponent"])
    opponent["source"]["play_import"] = comparison["source"]
    write(root / "evaluation-binding.json", dict(pilot=pin(root / "manifest.json"), binary=m["evaluation_binary"],
        qualification=pin(scorer_qualification / "qualification.json"), opponent=opponent,
        note="Native writer regenerated build-bound import; source weights, optimizer, feature mapping and frozen matches unchanged."))
    bound_opponent(root, m)
    cuda = Path("C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8")
    os.environ["CUDA_PATH"] = str(cuda)
    os.environ["PATH"] = str(cuda / "bin") + os.pathsep + os.environ["PATH"]
    temp = root / "native-temp"
    temp.mkdir()
    os.environ["TEMP"] = os.environ["TMP"] = str(temp)
    write(root / "training-launch.json", dict(pilot=pin(root / "manifest.json"), compute=pin(compute / "qualification.json"),
        choice=pin(compute / "compute-choice.json"), selected=selected, scorer=pin(scorer_qualification / "qualification.json"),
        runner=pin(__file__), analysis=pin(Path(__file__).with_name("public_entropy_analysis_v1.py")),
        evaluation_binding=pin(root / "evaluation-binding.json"),
        bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        draw_rule="Draws count as zero wins in both BO3 and game-one win-rate gates and are reported separately.",
        expected_natural_games=4000, native_wall_cap_seconds=1800, full_measurement_started=True,
        no_prefix_selection=True, no_paid_compute=True, review=m["review"]))
    telemetry = root / "training-telemetry"
    telemetry.mkdir()
    dispatch_root = root / f"entropy-replica-{m['replica']}-training"
    hosts = sorted({p["host"] for p in selected["placements"].values()})
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(dispatch_qualified, dispatch_root, m["training_binary"], m["training_configs"],
                             compute / "compute-choice.json", 1800)
        tick = 0
        while not future.done():
            record = dict(elapsed_seconds=time.monotonic()-started, hosts={})
            for host in hosts:
                try:
                    record["hosts"][host] = snapshot(host)
                except Exception as error:
                    record["hosts"][host] = dict(telemetry_error=str(error))
            write(telemetry / f"{tick:04}.json", record)
            print("training telemetry", round(record["elapsed_seconds"]),
                  {host: value.get("cpu_percent", "unavailable") for host, value in record["hosts"].items()}, flush=True)
            tick += 1
            for _ in range(30):
                if future.done():
                    break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m, root)
    audited["wall_seconds_including_audit"] = time.monotonic()-started
    write(root / "training-audit.json", audited)
    print(dict(complete=True, natural_games=4000, root=str(root)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--compute", type=Path, required=True)
    parser.add_argument("--scorer-qualification", type=Path, required=True)
    args = parser.parse_args()
    run(args.root, args.compute, args.scorer_qualification)
