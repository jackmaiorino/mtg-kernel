"""Bounded exposed-game correctness check, not a playing-strength experiment."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

from qualify_public_collectors_v1 import check_gpu_idle

OLD = Path("E:/mtg-postboard-campaign-20260920/public-learning-engineering-001")
FILES = ["checkpoint.json", "optimizer.json", "episode-000.json", "episode-001.json"]


def read(path):
    return json.loads(Path(path).read_bytes())


def pin(path):
    return dict(path=str(path), sha256=hashlib.sha256(Path(path).read_bytes()).hexdigest())


def verify(item):
    assert pin(item["path"]) == item, f"changed input: {item['path']}"
    return Path(item["path"])


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def prepare(root, binary):
    root.mkdir()
    for directory in ["configs", "requests", "logs", "outputs"]:
        (root/directory).mkdir()
    configs = {}
    for arm in ["control", "structured"]:
        config = read(OLD/f"{arm}-config.json")
        config["projection_mode"] = "state_only"
        assert len(config["updates"]) == 4 and all(len(batch) == 2 for batch in config["updates"])
        assert config["inputs_enabled"] == (arm == "structured")
        for source in [config["source"]] + [episode["opponent"] for batch in config["updates"] for episode in batch]:
            for key in ["checkpoint", "play_import"]:
                verify(source[key])
        path = root/f"configs/{arm}.json"
        write(path, config)
        configs[arm] = pin(path)
    write(root/"manifest.json", dict(
        schema="public-state-only-qualification/v1", binary=pin(binary), runner=pin(__file__),
        dependencies=[pin(Path(__file__).with_name("qualify_public_collectors_v1.py"))], configs=configs,
        source=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        question="Can isolated prevention-state weights learn from exposed natural games with exact serial/parallel/restart replay?",
        unique_games=16, executed_games=40, updates_per_arm=4, workers=[1, 2],
        process_cap_seconds=180, total_cap_seconds=600, cheap_four_update_projection_cap_seconds=180,
        gates=dict(natural_games_only=True, multiple_exposed_games_per_arm=True, cost_weights_and_moments_zero=True,
                   state_weights_and_moments_learn=True, serial_parallel_restart_bytes_exact=True,
                   initial_control_treatment_trajectories_equal=True),
        fixed="Untouched g115 full Adam32400, public Adam0, V4, terminal rewards, GAE1/.9, LR.0001, value.5, A48 and original engineering seeds.",
        non_claim="Engineering only. No win-rate selection, promotion, global placement qualification or human-strength claim. Fable review unavailable under known zero-read quota failure."))


def run(root):
    m = read(root/"manifest.json")
    verify(m["runner"])
    for item in m["dependencies"]:
        verify(item)
    binary = verify(m["binary"])
    configs = {arm: read(verify(item)) for arm, item in m["configs"].items()}
    write(root/"gpu-preflight.json", check_gpu_idle())
    started = time.monotonic()

    def launch(label, arm, workers, stop=4, resume=None):
        remaining = m["total_cap_seconds"] - (time.monotonic()-started)
        assert remaining > 0, "qualification wall cap reached"
        request = root/f"requests/{label}.json"
        write(request, dict(config=configs[arm], output_directory=str(root/f"outputs/{label}"),
                            collector_workers=workers, stop_after=stop, resume=resume))
        before = time.monotonic()
        with (root/f"logs/{label}.stdout").open("x") as stdout, (root/f"logs/{label}.stderr").open("x") as stderr:
            child = subprocess.Popen([str(binary), str(request)], stdout=stdout, stderr=stderr,
                                     creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            timeout = False
            try:
                code = child.wait(timeout=min(remaining, m["process_cap_seconds"]))
            except subprocess.TimeoutExpired:
                timeout = True
                child.kill()
                code = child.wait()
        receipt = dict(exit_code=code, timeout=timeout, seconds=time.monotonic()-before, request=pin(request), binary=m["binary"])
        write(root/f"logs/{label}.execution.json", receipt)
        assert code == 0 and not timeout, receipt

    launch("replay-first", "structured", 1, stop=1)
    first = read(root/"outputs/replay-first/0000/receipt.json")["seconds"]
    write(root/"timing.json", dict(first_update_seconds=first, projected_four_updates_seconds=4*first))
    assert 4*first < m["cheap_four_update_projection_cap_seconds"], "cheap timing exceeded gate"
    # Both worker counts execute exactly the same complete four-update workload.
    for arm, workers in [("control", 1), ("control", 2), ("structured", 2), ("structured", 1)]:
        launch(f"{arm}-w{workers}", arm, workers)
    launch("replay-rest", "structured", 2, resume=pin(root/"outputs/replay-first/0000/checkpoint.json"))
    analyze(root)


def analyze(root):
    names = ["control-w1", "control-w2", "structured-w1", "structured-w2", "replay-first", "replay-rest"]
    result = dict(complete=False, unique_games=16, executed_games=40, runs={}, non_claim="Engineering correctness only; no strength claim or candidate adoption.")
    zero = {0, 1 << 31}
    for label in names:
        execution = read(root/f"logs/{label}.execution.json")
        assert execution["exit_code"] == 0 and not execution["timeout"]
        folder = root/f"outputs/{label}"
        completion = read(folder/"completion.json")
        first, last = completion["first_update"], completion["next_update"]
        assert (first, last) == ((0, 1) if label == "replay-first" else (1, 4) if label == "replay-rest" else (0, 4))
        assert len(completion["receipts"]) == last-first
        exposed_games, exposed_rows, cannot_rows = 0, 0, 0
        for update in range(first, last):
            batch = folder/f"{update:04}"
            checkpoint = read(batch/"checkpoint.json")
            assert pin(batch/"optimizer.json")["sha256"] == checkpoint["optimizer_sha256"]
            assert len(checkpoint["trajectory_sha256"]) == 2
            saved = read(batch/"optimizer.json")
            assert saved["legacy_adam_step"] == 32401+update and saved["public"]["adam_step"] == 1+update
            public = saved["public"]
            for key in ["object", "object_first", "object_second"]:
                assert set(public[key]) <= zero, f"cost projection changed: {label}/{update}/{key}"
            if label.startswith("control"):
                for key in ["state", "state_first", "state_second"]:
                    assert set(public[key]) <= zero
            for index, digest in enumerate(checkpoint["trajectory_sha256"]):
                path = batch/f"episode-{index:03}.json"
                assert pin(path)["sha256"] == digest
                trajectory = read(path)
                assert trajectory["terminal"]["terminal_classification"] == "natural"
                assert trajectory["optimizer_state_sha256"] == completion["receipts"][update-first]["before_state_sha256"]
                assert len(trajectory["decisions"]) == len(trajectory["auxiliary"])
                rows = [row["state"] for row in trajectory["auxiliary"] if row is not None]
                count = sum(any(v != 0 for v in row) for row in rows)
                exposed_rows += count
                exposed_games += int(count > 0)
                cannot_rows += sum(row[5] != 0 for row in rows)
        if label in ["control-w1", "structured-w1"]:
            assert exposed_games >= 2 and exposed_rows > 0
        if label.startswith("structured") or label == "replay-rest":
            for key in ["state", "state_first", "state_second"]:
                assert any(v not in zero for v in public[key]), f"no state learning: {key}"
        result["runs"][label] = dict(seconds=execution["seconds"], exposed_games=exposed_games,
                                      exposed_rows=exposed_rows, cannot_prevent_rows=cannot_rows,
                                      update_seconds=sum(r["seconds"] for r in completion["receipts"]))
    compared = 0
    for arm in ["control", "structured"]:
        for update in range(4):
            for name in FILES:
                base = root/f"outputs/{arm}-w1/{update:04}/{name}"
                assert base.read_bytes() == (root/f"outputs/{arm}-w2/{update:04}/{name}").read_bytes()
                compared += 1
                if arm == "structured":
                    replay = "replay-first" if update == 0 else "replay-rest"
                    assert base.read_bytes() == (root/f"outputs/{replay}/{update:04}/{name}").read_bytes()
                    compared += 1
    for index in range(2):
        documents = [read(root/f"outputs/{arm}-w1/0000/episode-{index:03}.json") for arm in ["control", "structured"]]
        for document in documents:
            document.pop("config_sha256")
            document.pop("inputs_enabled")
        assert documents[0] == documents[1], "zero initial projection changed collected game"
    result.update(complete=True, status="STATE-ONLY-LEARNING-REPLAY-ENGINEERING-PASS", exact_file_comparisons=compared,
                  cost_parameters_and_moments_zero=True, state_parameters_and_moments_learned=True,
                  initial_arm_trajectories_equal=True)
    write(root/"qualification.json", result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "run", "analyze"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare":
        prepare(args.root, args.binary)
    elif args.mode == "run":
        run(args.root)
    else:
        analyze(args.root)
