"""Read-only descriptive diagnostics of the complete public-input pilot."""
import argparse
import concurrent.futures
import hashlib
import json
import math
import struct
import time
from collections import defaultdict
from pathlib import Path

PILOT = Path("E:/mtg-postboard-campaign-20260920/public-feature-pilot-001")


def f32(bits):
    return struct.unpack("<f", struct.pack("<I", bits))[0]


def inspect(job):
    arm, update, index, path, expected = job
    raw = Path(path).read_bytes()
    assert hashlib.sha256(raw).hexdigest() == expected
    x = json.loads(raw)
    episode = x["episode"]
    seat = episode["learner_seat"]
    terminal = x["terminal"]
    assert terminal["terminal_classification"] == "natural"
    reward = terminal["terminal_reward"][seat]
    counts = defaultdict(float)
    initial_value = None
    for d, auxiliary in zip(x["decisions"], x["auxiliary"], strict=True):
        if d["actor"] != seat:
            continue
        value = f32(d["value"])
        if initial_value is None:
            initial_value = value
        counts["learner_substeps"] += 1
        if auxiliary is not None:
            counts["prevention_substeps"] += any(v != 0 for v in auxiliary["state"][:5])
            counts["cannot_prevent_substeps"] += auxiliary["state"][5] != 0
        if len(d["logits"]) <= 1:
            counts["forced_substeps"] += 1
            continue
        logits = [f32(v) for v in d["logits"]]
        assert all(math.isfinite(v) for v in logits)
        top = max(logits)
        weights = [math.exp(v-top) for v in logits]
        total = sum(weights)
        probabilities = [v/total for v in weights]
        entropy = -sum(p*math.log(p) for p in probabilities if p)
        counts["choice_substeps"] += 1
        counts["normalized_entropy_sum"] += entropy/math.log(len(logits))
        counts["top_probability_sum"] += max(probabilities)
        counts["top_probability_ge_99"] += max(probabilities) >= .99
        counts["logit_range_sum"] += max(logits)-min(logits)
    assert initial_value is not None
    return dict(arm=arm, update=update, index=index, opponent=x["opponent"]["checkpoint_sha256"],
        postboard=episode["postboard"], seat=seat, reward=reward, initial_value=initial_value,
        initial_value_squared_error=(initial_value-reward)**2, **counts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    analysis = json.loads((PILOT/"analysis.json").read_text())
    assert analysis["complete"]
    jobs = []
    for arm in ["control", "structured"]:
        for update in range(200):
            batch = PILOT/f"outputs/{arm+'-prefix' if update < 2 else arm}/{update:04}"
            checkpoint = json.loads((batch/"checkpoint.json").read_text())
            hashes = checkpoint["trajectory_sha256"]
            assert len(hashes) == 10
            for index, expected in enumerate(hashes):
                jobs.append((arm, update, index, str(batch/f"episode-{index:03}.json"), expected))
    started = time.monotonic()
    with concurrent.futures.ProcessPoolExecutor(max_workers=4) as pool:
        rows = list(pool.map(inspect, jobs, chunksize=10))
    groups = defaultdict(list)
    for row in rows:
        # Fixed 50-update bins, both phases and all opponents, no endpoint search.
        key = (row["arm"], row["update"]//50, row["opponent"], row["postboard"])
        groups[key].append(row)
    aggregates = []
    for (arm, window, opponent, postboard), items in sorted(groups.items()):
        n = len(items)
        total = lambda k: sum(r.get(k, 0) for r in items)
        choices = total("choice_substeps")
        aggregates.append(dict(arm=arm, first_update=50*window, last_update=50*window+49,
            opponent=opponent, postboard=postboard, games=n,
            wins=sum(r["reward"] > 0 for r in items), draws=sum(r["reward"] == 0 for r in items),
            mean_initial_value=total("initial_value")/n,
            initial_value_mse=total("initial_value_squared_error")/n,
            choice_substeps=choices, forced_substeps=total("forced_substeps"),
            normalized_softmax_entropy=total("normalized_entropy_sum")/choices,
            mean_top_softmax_probability=total("top_probability_sum")/choices,
            top_softmax_ge_99_fraction=total("top_probability_ge_99")/choices,
            prevention_substeps=total("prevention_substeps"),
            cannot_prevent_substeps=total("cannot_prevent_substeps")))
    result = dict(complete=True, games=len(rows), seconds=time.monotonic()-started,
        source_analysis_sha256=hashlib.sha256((PILOT/"analysis.json").read_bytes()).hexdigest(),
        aggregates=aggregates,
        limitations=["Training trajectories, not an independent strength or exploitability estimate.",
            "Softmax entropy is a diagnostic proxy; exact execution uses quantized sampling.",
            "Non-forced substeps are weighted equally; longer games and macro decisions contribute more rows.",
            "Values compared with noisy single-game terminal returns; this is not causal credit-assignment evidence.",
            "All 200 updates included in fixed bins; no checkpoint selected from these outcomes."])
    args.output.parent.mkdir(exist_ok=True, parents=True)
    assert not args.output.exists()
    args.output.write_text(json.dumps(result, indent=2)+"\n", encoding="utf-8")
    print(json.dumps(dict(complete=True, games=len(rows), seconds=result["seconds"], cells=len(aggregates))))


if __name__ == "__main__":
    main()
