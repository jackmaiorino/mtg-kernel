"""Replay the fixed final training update, including the remote treatment on Jack."""
import argparse
from pathlib import Path
from public_training_dispatch_v2 import read, write, checked, pin, preflight, worker
from public_evaluation_dispatch_v1 import inventory
from qualify_state_prevention_compute_v1 import place


def run(root, pilot):
    if not __debug__:
        raise RuntimeError("replay requires Python checks enabled")
    m = read(pilot/"manifest.json")
    training = read(pilot/"training-audit.json")
    if not training["complete"] or training["full_natural_games"] != 4000:
        raise ValueError("requires completed matched training")
    current = inventory("jack")
    if current["active"]:
        raise ValueError("wait for existing native owners")
    root.mkdir()
    write(root/"manifest.json", dict(pilot=pin(pilot/"manifest.json"), training=pin(pilot/"training-audit.json"),
        binary=m["training_binary"], runner=pin(__file__), updates=[199], maximum_games=20,
        purpose="Fixed final-update replay. Treatment moves from Haley GPU0 to Jack GPU1. No outcome selection or model changes.",
        process_wall_cap_seconds=180, inventory=current))
    results = {}
    for arm in ["control", "structured"]:
        report = read(checked(training["arms"][arm]["report"]))
        cfg = read(checked(m["training_configs"][arm]))
        if len(cfg["updates"]) != 200 or len(cfg["updates"][199]) != 10:
            raise ValueError("fixed replay envelope differs")
        folder = root/arm
        folder.mkdir()
        placement = place("jack", 1, 10)
        write(folder/"preflight.json", preflight("jack", [placement]))
        resume = report["outputs"]["0198/checkpoint.json"]
        checked(resume)
        request = dict(config=cfg, output_directory=str(folder/"outputs"), resume=resume,
            stop_after=200, collector_workers=10, execution_gpu_ordinal=1)
        write(folder/"request.json", request)
        worker(dict(placement=placement, wall_seconds=180, binary=m["training_binary"], request=pin(folder/"request.json")))
        completion = read(folder/"outputs/completion.json")
        if completion["first_update"] != 199 or completion["next_update"] != 200 or len(completion["receipts"]) != 1:
            raise ValueError("final-update replay coverage differs")
        if completion["receipts"][0]["natural_games"] != 10:
            raise ValueError("final-update replay did not complete ten natural games")
        comparisons = {}
        for name, original in report["outputs"].items():
            if not name.startswith("0199/"):
                continue
            checked(original)
            replay = pin(folder/"outputs"/name)
            if replay["sha256"] != original["sha256"]:
                raise ValueError(f"final-update replay differs: {arm}/{name}")
            comparisons[name] = dict(original=original, replay=replay)
        if len(comparisons) != 12:
            raise ValueError("missing scientific replay files")
        results[arm] = dict(original_placement=report["placement"], replay_placement=placement,
            natural_games=10, exact_files=12, comparisons=comparisons, execution=pin(folder/"execution.json"))
    result = dict(complete=True, natural_games=20, exact_files=24, arms=results,
        non_claim="Determinism and late-update cross-device compatibility only, not playing strength.")
    write(root/"result.json", result)
    print(dict(complete=True, natural_games=20, exact_files=24), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    args = parser.parse_args()
    run(args.root, args.pilot)
