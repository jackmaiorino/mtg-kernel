"""Complete schedule comparison with explicit count-based deck retention gates."""
import argparse
from pathlib import Path
import numpy as np

from public_training_dispatch_v2 import read, write, pin, checked
from state_prevention_analysis_v1 import bootstrap

ARMS = ["g115", "control", "balanced"]


def bound_opponent(root, manifest):
    binding = read(root / "evaluation-binding.json")
    assert binding["pilot"] == pin(root / "manifest.json")
    assert binding["binary"] == manifest["evaluation_binary"]
    checked(binding["binary"])
    qualification = read(checked(binding["qualification"]))
    assert qualification["complete"] and qualification["original_gameplay_replays"] == 4
    comparison = read(checked(qualification["import_comparison"]))
    assert comparison["complete"] and comparison["only_changed_field"] == "receipt.destination_build_git_head"
    original_source = read(checked(manifest["evaluation_opponent"]["source"]["play_import"]))
    new_source = read(checked(comparison["source"]))
    assert original_source["transfer_envelope"] == comparison["old"]
    assert new_source["transfer_envelope"] == comparison["new"]
    old, new = read(checked(comparison["old"])), read(checked(comparison["new"]))
    old["receipt"]["destination_build_git_head"] = new["receipt"]["destination_build_git_head"]
    assert old == new, "opponent weights or optimizer changed"
    for field in ["source_checkpoint", "source_registry"]:
        checked(original_source[field]); checked(new_source[field])
        assert original_source[field]["sha256"] == new_source[field]["sha256"]
    import copy
    opponent = copy.deepcopy(manifest["evaluation_opponent"])
    opponent["source"]["play_import"] = comparison["source"]
    assert opponent == binding["opponent"]
    return opponent


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
    breadth_checks()
    assert not (root / "analysis.json").exists(), "preserve frozen analysis"
    experiment = read(root.parent / "manifest.json")
    assert experiment["schema"] == "two-replica-public-balanced-schedule/v1"
    assert len(experiment["replicas"]) == 2 and experiment["both_replicas_required"]
    for pilot in experiment["replicas"]:
        folder = checked(pilot).parent
        completion = read(folder / "evaluation-completion.json")
        assert completion["complete"] and completion["matches"] == 3072
        checked(completion["evaluation"])
    manifest = read(root / "manifest.json")
    assert manifest["schema"] == "matched-public-balanced-schedule/v1"
    assert manifest["analysis"] == pin(__file__)
    assert manifest["bootstrap_implementation"] == pin(Path(__file__).with_name("state_prevention_analysis_v1.py"))
    opponent = bound_opponent(root, manifest)
    evaluation = read(evaluation_manifest)
    assert checked(evaluation["pilot"]).resolve() == (root / "manifest.json").resolve()
    assert evaluation["binding"] == pin(root / "evaluation-binding.json")
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
        assert request["sources"][1-seat] == opponent
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
    diff = comparisons["balanced-minus-control"]
    own_decks = {}
    for own in sorted({pair[0] for pair in pairs}):
        selected = values[[i for i, pair in enumerate(pairs) if pair[0] == own]]
        assert selected.shape == (8, 8, 2, 3, 2)
        deck_samples = bootstrap(selected, gate["bootstrap_replicates"], gate["bootstrap_seed"])
        deck_comparisons = {}
        for a, b in [(2, 1), (2, 0)]:
            deck_comparisons[f"{ARMS[a]}-minus-{ARMS[b]}"] = dict(
                net_wins=int(selected[:, :, :, a, 0].sum()-selected[:, :, :, b, 0].sum()),
                paired_95_interval=np.quantile(deck_samples[:, a, 0]-deck_samples[:, b, 0], [.025, .975]).tolist())
        own_decks[own] = dict(matches=128, comparisons=deck_comparisons,
            interval_scope="Post hoc descriptive per-deck intervals, not simultaneous retention confidence.")
    breadth, retention = deck_count_gates(own_decks, gate)
    gates = dict(net_wins=diff["net_wins"] >= gate["net_bo3_wins_over_control"],
        paired_positive=diff["paired_95_interval"][0] > gate["paired_bo3_95_lower"],
        at_least_g115_wins=comparisons["balanced-minus-g115"]["net_wins"] >= 0,
        game_one_retention=all(comparisons[name]["game_one_paired_95_interval"][0] > gate["game_one_paired_95_lower"]
            for name in ["balanced-minus-control", "balanced-minus-g115"]),
        own_deck_breadth=breadth, own_deck_count_retention=retention)
    write(root / "analysis.json", dict(complete=True, replica=manifest["replica"], matches=3072,
        natural_games=games_total, native_process_seconds=seconds, summary=summary, comparisons=comparisons,
        gates=gates, verdict="REPLICA-PASS" if all(gates.values()) else "NO-ADVANCE", breakdown=breakdown,
        own_decks=own_decks, breadth_gate_scope="Count-based development guard, not simultaneous statistical retention proof.",
        pilot=pin(root / "manifest.json"), evaluation=pin(evaluation_manifest),
        analysis=pin(__file__), bootstrap_implementation=pin(Path(__file__).with_name("state_prevention_analysis_v1.py")),
        draw_rule="Draws count as zero wins in BO3/game-one win-rate gates; draw counts reported separately.",
        review=manifest["review"], non_claim=manifest["non_claim"], promotion=False))
    print(dict(replica=manifest["replica"], gates=gates, comparisons=comparisons), flush=True)


def deck_count_gates(own_decks, gate):
    assert len(own_decks) == 8 and all(d["matches"] == 128 for d in own_decks.values())
    rows = [[d["comparisons"][name]["net_wins"] for d in own_decks.values()]
            for name in ["balanced-minus-control", "balanced-minus-g115"]]
    assert all(type(n) is int and -128 <= n <= 128 for row in rows for n in row)
    return (all(sum(n >= 0 for n in row) >= gate["minimum_nonnegative_own_decks"] for row in rows),
            all(min(row) >= -gate["maximum_own_deck_net_loss"] for row in rows))


def breadth_checks():
    import copy
    fixture = {str(i): dict(matches=128, comparisons={name: dict(net_wins=value)
        for name in ["balanced-minus-control", "balanced-minus-g115"]})
        for i, value in enumerate([0, 0, 1, 2, 3, 4, -8, -8])}
    gate = dict(minimum_nonnegative_own_decks=6, maximum_own_deck_net_loss=8)
    assert deck_count_gates(fixture, gate) == (True, True)
    changed = copy.deepcopy(fixture)
    changed["0"]["comparisons"]["balanced-minus-g115"]["net_wins"] = -1
    assert deck_count_gates(changed, gate) == (False, True)
    changed = copy.deepcopy(fixture)
    changed["7"]["comparisons"]["balanced-minus-control"]["net_wins"] = -9
    assert deck_count_gates(changed, gate) == (True, False)


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
        breadth_checks()
        print("Pairing, deterministic resampling, anti-correlated seats and deck-gate boundary checks passed.")
    else:
        analyze(args.root, args.evaluation_manifest)
