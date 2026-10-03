"""Complete-panel paired analysis for the fixed two-cohort prevention pilot."""
import argparse
from pathlib import Path
import numpy as np

from public_training_dispatch_v2 import read, write, pin, checked

ARMS = ["g115", "control", "structured"]


def bootstrap(values, replicates, seed):
    # axes: matchup, environment seed, physical seat, arm, score metric.
    assert values.ndim == 5 and values.shape[2:] == (2, 3, 2)
    assert np.all(np.isfinite(values)) and np.all((values >= 0) & (values <= 1))
    rng = np.random.Generator(np.random.PCG64(seed))
    result = np.zeros((replicates, 3, 2))
    for stratum in values:
        indices = rng.integers(0, len(stratum), size=(replicates, len(stratum)))
        result += stratum[indices].mean(axis=(1, 2))/len(values)
    return result


def statistical_checks():
    for strata, seeds in [(15, 16), (49, 4)]:
        values = np.zeros((strata, seeds, 2, 3, 2))
        noise = np.random.default_rng(15).integers(0, 2, size=(strata, seeds, 2, 2))*.5
        values[:, :, :, :] = noise[:, :, :, None, :]
        a = bootstrap(values, 200, 123)
        assert np.array_equal(a[:, 0], a[:, 1]) and np.array_equal(a, bootstrap(values, 200, 123))
        values[:, :, :, 2] += .25
        b = bootstrap(values, 200, 123)
        assert np.allclose(b[:, 2]-b[:, 1], .25, rtol=0, atol=1e-12)
        assert np.max(np.abs((b[:, 2]-b[:, 1])[:, 0]-.25)) < 1e-12


def analyze(root, evaluation_manifest):
    statistical_checks()
    manifest = read(root/"manifest.json")
    evaluation = read(evaluation_manifest)
    assert checked(evaluation["pilot"]).resolve() == (root/"manifest.json").resolve()
    training = read(root/"training-audit.json")
    assert training["complete"] and training["full_natural_games"] == 4000
    assert evaluation["endpoints"]["g115"] == dict(kind="legacy", source=manifest["source"], v3_forced_actions=False)
    for arm in ["control", "structured"]:
        endpoint = training["arms"][arm]
        assert endpoint["updates"] == 200 and endpoint["natural_games"] == 2000
        checked(endpoint["checkpoint"])
        assert evaluation["endpoints"][arm] == dict(kind="public_checkpoint",
            config=manifest["training_configs"][arm], checkpoint=endpoint["checkpoint"])
    expected_jobs = {(arm, job["label"]): job for arm in ARMS for job in manifest["jobs"]}
    assert len(evaluation["jobs"]) == len(expected_jobs)
    seen_jobs, records = set(), {}
    seconds = games_total = 0
    for job in evaluation["jobs"]:
        arm, label = job["arm"], job["label"]
        key = arm, label
        assert key in expected_jobs and key not in seen_jobs
        seen_jobs.add(key)
        template = expected_jobs[key]
        request = read(checked(job["request"]))
        source = read(checked(template["template"]))
        assert request["matches"] == source["matches"] and request["cross_generation_evaluation"]
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
        start = read(folder/"start.json")
        completion = read(folder/"completion.json")
        assert start["command"] == request
        assert completion["matches"] == len(completion["match_sha256"]) == len(template["cases"])
        games = decisions = 0
        for index, case in enumerate(template["cases"]):
            path = folder/f"match-{index:06}.json"
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
            record_key = case["cohort"], case["own"], case["opponent"], case["replicate"], seat, arm
            assert record_key not in records
            records[record_key] = dict(seed=case["seed"], win=int(winner == seat), draw=int(winner is None),
                score=.5 if winner is None else int(winner == seat), g1=.5 if g1 is None else int(g1 == seat),
                starts=case["candidate_starts"], match=pin(path))
        assert games == completion["natural_games"] and decisions == completion["decisions"]
        games_total += games
    assert seen_jobs == set(expected_jobs) and len(records) == manifest["expected_matches"] == 2616
    cohorts, breakdown = {}, []
    for cohort, repeats, count, bootstrap_seed in [("focal", 16, 480, manifest["gates"]["focal_bootstrap_seed"]),
        ("canonical", 4, 392, manifest["gates"]["canonical_bootstrap_seed"])]:
        strata = sorted({k[1:3] for k in records if k[0] == cohort})
        assert len(strata)*repeats*2 == count
        values = np.empty((len(strata), repeats, 2, 3, 2))
        summary = {arm: dict(matches=count, wins=0, draws=0, score=0., game_one_score=0.) for arm in ARMS}
        for i, pair in enumerate(strata):
            for rep in range(repeats):
                seeds = set()
                for seat in [0, 1]:
                    for a, arm in enumerate(ARMS):
                        row = records[cohort, *pair, rep, seat, arm]
                        seeds.add(row["seed"])
                        values[i, rep, seat, a] = row["score"], row["g1"]
                        for dest, src in [("wins", "win"), ("draws", "draw"), ("score", "score"), ("game_one_score", "g1")]:
                            summary[arm][dest] += row[src]
                assert len(seeds) == 1
            for seat in [0, 1]:
                for starts in [False, True]:
                    for arm in ARMS:
                        rows = [records[cohort, *pair, rep, seat, arm] for rep in range(repeats)
                                if records[cohort, *pair, rep, seat, arm]["starts"] == starts]
                        assert len(rows) == repeats//2
                        breakdown.append(dict(cohort=cohort, own=pair[0], opponent=pair[1], seat=seat, starts=starts,
                            arm=arm, matches=len(rows), wins=sum(r["win"] for r in rows), draws=sum(r["draw"] for r in rows),
                            score=sum(r["score"] for r in rows), game_one_score=sum(r["g1"] for r in rows)))
        samples = bootstrap(values, manifest["gates"]["bootstrap_replicates"], bootstrap_seed)
        means = values.mean(axis=(0, 1, 2))
        comparisons = {}
        for a, b in [(2, 1), (2, 0), (1, 0)]:
            comparisons[f"{ARMS[a]}-minus-{ARMS[b]}"] = dict(net_wins=summary[ARMS[a]]["wins"]-summary[ARMS[b]]["wins"],
                score_difference=float(means[a, 0]-means[b, 0]),
                paired_95_interval=np.quantile(samples[:, a, 0]-samples[:, b, 0], [.025, .975]).tolist(),
                game_one_difference=float(means[a, 1]-means[b, 1]),
                game_one_paired_95_interval=np.quantile(samples[:, a, 1]-samples[:, b, 1], [.025, .975]).tolist())
        cohorts[cohort] = dict(summary=summary, comparisons=comparisons)
    focal, canonical = [cohorts[c]["comparisons"] for c in ["focal", "canonical"]]
    gates = dict(primary=focal["structured-minus-control"]["net_wins"] >= manifest["gates"]["primary_focal_net_wins"]
        and focal["structured-minus-control"]["paired_95_interval"][0] > manifest["gates"]["primary_paired_95_lower"],
        at_least_g115_wins=focal["structured-minus-g115"]["net_wins"] >= 0,
        canonical_retention=all(canonical[name]["paired_95_interval"][0] > manifest["gates"]["canonical_retention_paired_95_lower"]
            for name in ["structured-minus-control", "structured-minus-g115"]))
    result = dict(complete=True, matches=2616, natural_games=games_total, process_seconds=seconds,
        cohorts=cohorts, gates=gates, verdict="REPLICATE" if all(gates.values()) else "NO-ADVANCE",
        breakdown=breakdown, pilot=pin(root/"manifest.json"), evaluation=pin(evaluation_manifest),
        non_claim=manifest["non_claim"], review=manifest["review"], promotion=False, human_strength_claim=False)
    write(root/"analysis.json", result)
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--evaluation-manifest", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if args.self_check:
        statistical_checks()
        print("Both cohort pairing, fixed effect, and deterministic resampling checks passed.")
    else:
        result = analyze(args.root, args.evaluation_manifest)
        print({key: result[key] for key in ["verdict", "gates", "cohorts"]})
