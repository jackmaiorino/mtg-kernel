"""Complete balanced paired analysis; no partial panels or replica pooling."""
import argparse
from pathlib import Path
import numpy as np

from public_training_dispatch_v2 import read, write, pin, checked
from state_prevention_analysis_v1 import bootstrap

ARMS = ["g115", "control", "entropy"]


def statistical_checks():
    values = np.zeros((64, 8, 2, 3, 2))
    noise = np.random.default_rng(720).integers(0, 2, size=(64, 8, 2, 2)) * .5
    values[:] = noise[:, :, :, None, :]
    samples = bootstrap(values, 300, 20260921)
    assert np.array_equal(samples[:, 0], samples[:, 2])
    assert np.array_equal(samples, bootstrap(values, 300, 20260921))
    values[:, :, :, 2] += .25
    shifted = bootstrap(values, 300, 20260921)
    assert np.allclose(shifted[:, 2] - shifted[:, 1], .25, rtol=0, atol=1e-12)
    # Both seats must share a resample: this anti-correlated fixture has zero
    # sampling variance only when the environment-seed pair remains intact.
    values.fill(0)
    values[:, :, 0, :, :] = np.arange(8)[None, :, None, None] % 2
    values[:, :, 1] = 1 - values[:, :, 0]
    assert np.array_equal(bootstrap(values, 300, 20260921), np.full((300, 3, 2), .5))


def analyze(root, evaluation_manifest):
    statistical_checks()
    manifest = read(root / "manifest.json")
    assert manifest["schema"] == "matched-public-entropy/v1"
    evaluation = read(evaluation_manifest)
    assert checked(evaluation["pilot"]).resolve() == (root / "manifest.json").resolve()
    training = read(root / "training-audit.json")
    assert training["complete"] and training["full_natural_games"] == 4000
    assert evaluation["endpoints"]["g115"] == dict(kind="legacy", source=manifest["source"], v3_forced_actions=False)
    for arm in ARMS[1:]:
        endpoint = training["arms"][arm]
        assert endpoint["updates"] == 200 and endpoint["natural_games"] == 2000
        checked(endpoint["checkpoint"])
        assert evaluation["endpoints"][arm] == dict(kind="public_checkpoint",
            config=manifest["training_configs"][arm], checkpoint=endpoint["checkpoint"])
    expected_jobs = {(arm, job["label"]): job for arm in ARMS for job in manifest["jobs"]}
    assert len(expected_jobs) == len(evaluation["jobs"]) == 48
    seen, records = set(), {}
    seconds = games_total = 0
    for job in evaluation["jobs"]:
        arm, label = job["arm"], job["label"]
        key = arm, label
        assert key in expected_jobs and key not in seen
        seen.add(key)
        template = expected_jobs[key]
        request = read(checked(job["request"]))
        original = read(checked(template["template"]))
        assert request["matches"] == original["matches"] and request["cross_generation_evaluation"]
        assert request["capture_decisions"] is False
        seat = template["candidate_seat"]
        assert request["sources"][1-seat] == manifest["evaluation_opponent"]
        assert request["sources"][seat] == evaluation["endpoints"][arm]
        execution = read(checked(job["execution"]))
        assert execution["exit_code"] == 0 and not execution["timeout"]
        assert execution["binary"]["sha256"] == manifest["evaluation_binary"]["sha256"]
        checked(execution["binary"])
        assert execution["request"] == job["request"]
        seconds += execution["seconds"]
        folder = Path(job["output_directory"])
        start, completion = [read(folder / name) for name in ["start.json", "completion.json"]]
        assert start["command"] == request
        assert completion["matches"] == len(completion["match_sha256"]) == len(template["cases"]) == 64
        games = decisions = 0
        for index, case in enumerate(template["cases"]):
            path = folder / f"match-{index:06}.json"
            assert pin(path)["sha256"] == completion["match_sha256"][index]
            match = read(path)
            assert match["match"] == request["matches"][index] and match["models"] == start["models"]
            assert match["match"]["config"]["seed"] == case["seed"]
            assert (match["games"][0]["start"]["starting_player"] == seat) == case["candidate_starts"]
            assert not match["decisions"] and len(match["games"]) == len(match["seed_resets"])
            assert match["diagnostic_spell_target_repairs"] == [0, 0]
            games += len(match["games"])
            decisions += match["decision_count"]
            winner = None if match["outcome"] == "draw" else match["outcome"]["winner"]["winner"]
            g1 = match["games"][0]["winner"]
            record_key = case["own"], case["opponent"], case["replicate"], seat, arm
            assert record_key not in records
            records[record_key] = dict(seed=case["seed"], win=int(winner == seat), draw=int(winner is None),
                g1=int(g1 == seat), g1_draw=int(g1 is None), starts=case["candidate_starts"], match=pin(path))
        assert games == completion["natural_games"] and decisions == completion["decisions"]
        games_total += games
    assert seen == set(expected_jobs) and len(records) == manifest["expected_matches"] == 3072
    pairs = sorted({key[:2] for key in records})
    assert len(pairs) == 64
    values = np.empty((64, 8, 2, 3, 2))
    summary = {arm: dict(matches=1024, wins=0, draws=0, game_one_wins=0, game_one_draws=0) for arm in ARMS}
    breakdown = []
    for i, pair in enumerate(pairs):
        for repeat in range(8):
            seeds = set()
            for seat in [0, 1]:
                for a, arm in enumerate(ARMS):
                    row = records[*pair, repeat, seat, arm]
                    seeds.add(row["seed"])
                    values[i, repeat, seat, a] = row["win"], row["g1"]
                    for dest, src in [("wins", "win"), ("draws", "draw"), ("game_one_wins", "g1"), ("game_one_draws", "g1_draw")]:
                        summary[arm][dest] += row[src]
            assert len(seeds) == 1
        for seat in [0, 1]:
            for starts in [False, True]:
                for arm in ARMS:
                    rows = [records[*pair, repeat, seat, arm] for repeat in range(8)
                            if records[*pair, repeat, seat, arm]["starts"] == starts]
                    assert len(rows) == 4
                    breakdown.append(dict(own=pair[0], opponent=pair[1], seat=seat, starts=starts, arm=arm,
                        matches=4, wins=sum(r["win"] for r in rows), draws=sum(r["draw"] for r in rows),
                        game_one_wins=sum(r["g1"] for r in rows), game_one_draws=sum(r["g1_draw"] for r in rows)))
    gate = manifest["gates"]
    samples = bootstrap(values, gate["bootstrap_replicates"], gate["bootstrap_seed"])
    means = values.mean(axis=(0, 1, 2))
    comparisons = {}
    for a, b in [(2, 1), (2, 0), (1, 0)]:
        comparisons[f"{ARMS[a]}-minus-{ARMS[b]}"] = dict(
            net_wins=summary[ARMS[a]]["wins"]-summary[ARMS[b]]["wins"],
            win_rate_difference=float(means[a, 0]-means[b, 0]),
            paired_95_interval=np.quantile(samples[:, a, 0]-samples[:, b, 0], [.025, .975]).tolist(),
            game_one_difference=float(means[a, 1]-means[b, 1]),
            game_one_paired_95_interval=np.quantile(samples[:, a, 1]-samples[:, b, 1], [.025, .975]).tolist())
    diff = comparisons["entropy-minus-control"]
    gates = dict(net_wins=diff["net_wins"] >= gate["net_bo3_wins_over_control"],
        paired_positive=diff["paired_95_interval"][0] > gate["paired_bo3_95_lower"],
        at_least_g115_wins=comparisons["entropy-minus-g115"]["net_wins"] >= 0,
        game_one_retention=all(comparisons[name]["game_one_paired_95_interval"][0] > gate["game_one_paired_95_lower"]
            for name in ["entropy-minus-control", "entropy-minus-g115"]))
    write(root / "analysis.json", dict(complete=True, replica=manifest["replica"], matches=3072,
        natural_games=games_total, native_process_seconds=seconds, summary=summary, comparisons=comparisons,
        gates=gates, verdict="REPLICA-PASS" if all(gates.values()) else "NO-ADVANCE", breakdown=breakdown,
        pilot=pin(root / "manifest.json"), evaluation=pin(evaluation_manifest),
        analysis=pin(__file__), bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        draw_rule="Draws count as zero wins in BO3/game-one win-rate gates; draw counts reported separately.",
        review=manifest["review"], non_claim=manifest["non_claim"], promotion=False))
    print(dict(replica=manifest["replica"], gates=gates, comparisons=comparisons), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--evaluation-manifest", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    if args.self_check:
        statistical_checks()
        print("Matched endpoint, fixed-effect, deterministic and anti-correlated seat checks passed.")
    else:
        analyze(args.root, args.evaluation_manifest)
