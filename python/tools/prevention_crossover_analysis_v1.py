"""Retrospective paired diagnostic, conditional on four frozen endpoints."""
import argparse
from collections import defaultdict
from pathlib import Path
import numpy as np

from public_evaluation_dispatch_v1 import read, write, pin, checked

ARMS = ["control", "structured"]


def bootstrap(values, count, seed):
    assert values.ndim == 5 and values.shape[2:] == (2, 4, 2)
    assert np.isfinite(values).all() and ((values >= 0) & (values <= 1)).all()
    rng = np.random.Generator(np.random.PCG64(seed))
    result = np.zeros((count, 4, 2))
    for stratum in values:
        indices = rng.integers(0, len(stratum), size=(count, len(stratum)))
        result += stratum[indices].mean(axis=(1, 2))/len(values)
    return result


def self_check():
    for strata, repeats in [(15, 16), (49, 4)]:
        values = np.zeros((strata, repeats, 2, 4, 2))
        noise = np.random.default_rng(17).integers(0, 2, size=(strata, repeats, 2, 2))*.5
        values[:] = noise[:, :, :, None, :]
        draws = bootstrap(values, 200, 12)
        assert np.array_equal(draws[:, 0], draws[:, 3])
        assert np.array_equal(draws, bootstrap(values, 200, 12))
        values[:, :, :, 1] += .25
        changed = bootstrap(values, 200, 12)
        assert np.allclose(changed[:, 1]-changed[:, 0], .25, atol=1e-12, rtol=0)
    d11, d12, d21, d22 = [.04, .03, -.05, -.06]
    endpoint = ((d21-d11)+(d22-d12))/2
    panel = ((d12-d11)+(d22-d21))/2
    assert abs(endpoint+panel-(d22-d11)) < 1e-12


def interval(point, draws):
    return dict(difference=float(point), paired_95_interval=np.quantile(draws, [.025, .975]).tolist())


def analyze(root):
    self_check()
    plan = read(root / "plan.json")
    assert pin(__file__) == plan["analysis"]
    completion = read(root / "completion.json")
    assert completion["complete"] and completion["plan"] == pin(root / "plan.json")
    full = read(checked(completion["dispatch"]))
    assert len(full["jobs"]) == 872 and full["matches"] == 3488
    manifests = {int(i): read(checked(item)) for i,item in plan["pilots"].items()}
    originals = {int(i): read(checked(item)) for i,item in plan["original_evaluations"].items()}
    for endpoints in plan["endpoints"].values():
        for endpoint in endpoints.values():
            checked(endpoint["checkpoint"]); checked(endpoint["config"])
    assert manifests[1]["evaluation_opponent"] == manifests[2]["evaluation_opponent"]
    records, counters = {}, defaultdict(int)
    def collect(job, train, panel, seat, cases, expected_command, kind):
        request = read(checked(job["request"]))
        assert {k:v for k,v in request.items() if k != "output_directory"} == {k:v for k,v in expected_command.items() if k != "output_directory"}
        assert request["sources"][seat] == plan["endpoints"][str(train)][job["arm"]]
        assert request["sources"][1-seat] == manifests[1]["evaluation_opponent"]
        assert request["cross_generation_evaluation"] and len(request["matches"]) == len(cases)
        execution = read(checked(job["execution"]))
        assert execution["exit_code"] == 0 and not execution["timeout"]
        assert execution["binary"] == plan["binary"] and execution["request"] == job["request"]
        checked(execution["binary"])
        if "recovery" in job:
            assert read(checked(job["recovery"]))["mismatches"] == 0
        folder = Path(job["output_directory"])
        start, done = read(folder / "start.json"), read(folder / "completion.json")
        assert start["command"] == request and done["matches"] == len(cases) == len(done["match_sha256"])
        games = decisions = 0
        for index, case in enumerate(cases):
            file_pin = pin(folder / f"match-{index:06}.json")
            assert file_pin["sha256"] == done["match_sha256"][index]
            match = read(file_pin["path"])
            assert match["match"] == request["matches"][index] and match["models"] == start["models"]
            assert match["match"]["config"]["seed"] == case["seed"]
            assert (match["games"][0]["start"]["starting_player"] == seat) == case["candidate_starts"]
            assert not match["decisions"] and len(match["games"]) == len(match["seed_resets"])
            assert match["diagnostic_spell_target_repairs"] == [0, 0]
            games += len(match["games"])
            decisions += match["decision_count"]
            winner = None if match["outcome"] == "draw" else match["outcome"]["winner"]["winner"]
            g1 = match["games"][0]["winner"]
            key = (train, panel, case["cohort"], case["own"], case["opponent"], case["replicate"], seat, job["arm"])
            assert key not in records
            records[key] = dict(seed=case["seed"], win=int(winner == seat), draw=int(winner is None),
                score=.5 if winner is None else int(winner == seat), game_one=.5 if g1 is None else int(g1 == seat),
                starts=case["candidate_starts"], match=file_pin, evidence_kind=kind)
        assert games == done["natural_games"] and decisions == done["decisions"]
        counters[kind+"_matches"] += len(cases)
        counters[kind+"_natural_games"] += games
    for panel in [1, 2]:
        templates = {j["label"]: j for j in manifests[panel]["jobs"]}
        seen = set()
        for job in originals[panel]["jobs"]:
            if job["arm"] not in ARMS:
                continue
            key = job["arm"], job["label"]
            assert key not in seen
            seen.add(key)
            template = templates[job["label"]]
            command = read(checked(template["template"]))
            command["sources"][template["candidate_seat"]] = plan["endpoints"][str(panel)][job["arm"]]
            collect(job, panel, panel, template["candidate_seat"], template["cases"], command, "reused")
        assert len(seen) == 60
    expected = {j["id"]: j for j in plan["jobs"]}
    seen = set()
    for job in full["jobs"]:
        assert job["id"] in expected and job["id"] not in seen
        seen.add(job["id"])
        template = expected[job["id"]]
        assert job["arm"] == template["arm"] and job["label"] == template["label"]
        collect(job, template["train"], template["panel"], template["candidate_seat"], template["cases"], template["command"], "new")
    assert seen == set(expected) and len(records) == 6976
    assert counters["new_matches"] == counters["reused_matches"] == 3488
    cohorts = {}
    for cohort, repeats, count in [("focal", 16, 480), ("canonical", 4, 392)]:
        samples, means, cells = {}, {}, {}
        for panel in [1, 2]:
            strata = sorted({k[3:5] for k in records if k[1] == panel and k[2] == cohort})
            assert len(strata)*repeats*2 == count
            values = np.empty((len(strata), repeats, 2, 4, 2))
            totals = {train: {arm: dict(matches=count, wins=0, draws=0) for arm in ARMS} for train in [1, 2]}
            for i, pair in enumerate(strata):
                for rep in range(repeats):
                    seeds, starts = set(), {0: set(), 1: set()}
                    for seat in [0, 1]:
                        for train in [1, 2]:
                            for a, arm in enumerate(ARMS):
                                row = records[train, panel, cohort, *pair, rep, seat, arm]
                                seeds.add(row["seed"]); starts[seat].add(row["starts"])
                                values[i, rep, seat, (train-1)*2+a] = row["score"], row["game_one"]
                                totals[train][arm]["wins"] += row["win"]
                                totals[train][arm]["draws"] += row["draw"]
                    assert len(seeds) == 1 and all(len(s) == 1 for s in starts.values())
            samples[panel] = bootstrap(values, plan["bootstrap_replicates"], plan["bootstrap_seeds"][cohort][panel-1])
            means[panel] = values.mean(axis=(0, 1, 2))
            for train in [1, 2]:
                c, t = (train-1)*2, (train-1)*2+1
                cells[f"train{train}-panel{panel}"] = dict(summary=totals[train],
                    net_wins=totals[train]["structured"]["wins"]-totals[train]["control"]["wins"],
                    bo3=interval(means[panel][t,0]-means[panel][c,0], samples[panel][:,t,0]-samples[panel][:,c,0]),
                    game_one=interval(means[panel][t,1]-means[panel][c,1], samples[panel][:,t,1]-samples[panel][:,c,1]))
        d = {(train,panel): means[panel][(train-1)*2+1,0]-means[panel][(train-1)*2,0] for train in [1,2] for panel in [1,2]}
        b = {(train,panel): samples[panel][:,(train-1)*2+1,0]-samples[panel][:,(train-1)*2,0] for train in [1,2] for panel in [1,2]}
        def components(x):
            return ((x[2,1]-x[1,1])+(x[2,2]-x[1,2]))/2, ((x[1,2]-x[1,1])+(x[2,2]-x[2,1]))/2
        ep, pa = components(d)
        eb, pb = components(b)
        assert np.allclose(eb+pb, b[2,2]-b[1,1], atol=1e-12, rtol=0)
        shifts = {f"{arm}-panel{panel}": interval(means[panel][2+a,0]-means[panel][a,0], samples[panel][:,2+a,0]-samples[panel][:,a,0])
                  for panel in [1,2] for a,arm in enumerate(ARMS)}
        cohorts[cohort] = dict(cells=cells, endpoint_component=interval(ep, eb), panel_component=interval(pa, pb),
            diagonal_change=interval(d[2,2]-d[1,1], b[2,2]-b[1,1]), train2_minus_train1=shifts)
    result = dict(complete=True, counts=dict(counters), cohorts=cohorts,
        plan=pin(root / "plan.json"), completion=pin(root / "completion.json"), review=plan["review"],
        non_claim=plan["non_claim"], promotion=False,
        inference_scope="Retrospective and conditional on four endpoints. No estimate of training-run population variance.")
    write(root / "analysis.json", result)
    write(root / "record-provenance.json", [{"key": list(key), **value} for key,value in sorted(records.items())])
    print(dict(counts=result["counts"], cohorts=cohorts), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path)
    parser.add_argument("--self-check", action="store_true")
    args = parser.parse_args()
    if not __debug__:
        raise RuntimeError("optimized Python disables validation")
    if args.self_check:
        self_check()
        print("Four-endpoint matched resampling, deterministic replay and decomposition checks passed.")
    else:
        analyze(args.root)
