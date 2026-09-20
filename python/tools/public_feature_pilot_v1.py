"""One bounded, matched public-input learning pilot. Never promotes a model."""
import argparse
import copy
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time

import numpy as np
from qualify_public_evaluation_v1 import read, write, sha, pin, CAMPAIGN

ARMS = ("g115", "control", "structured")
TOOLS = CAMPAIGN / "public-production-tools-003"
COVERAGE = CAMPAIGN / "public-canonical-coverage-002"


def seed(text):
    return int.from_bytes(hashlib.sha256(("public-input-pilot/v1/"+text).encode()).digest()[:8], "little")


def verify(p):
    assert sha(p["path"]) == p["sha256"], p["path"]
    return Path(p["path"])


def bootstrap(values, replicates, seed_value):
    assert values.shape == (49, 8, 2, 3, 2)
    rng = np.random.Generator(np.random.PCG64(seed_value))
    means = np.zeros((replicates, 3, 2))
    for stratum in values:
        indices = rng.integers(0, 8, size=(replicates, 8))
        means += stratum[indices].mean(axis=(1, 2)) / 49
    return means


def statistical_checks():
    # Shared case noise must cancel, including both physical seats.
    values = np.zeros((49, 8, 2, 3, 2))
    noise = np.random.default_rng(13).integers(0, 2, size=(49, 8, 2, 2)) * .5
    for arm in range(3): values[:, :, :, arm] = noise
    a = bootstrap(values, 200, 9)
    assert np.array_equal(a[:, 0], a[:, 1]) and np.array_equal(a, bootstrap(values, 200, 9))
    values[:, :, :, 2] += .25
    b = bootstrap(values, 200, 9)
    assert np.allclose(b[:, 2]-b[:, 1], .25, rtol=0, atol=1e-12)


def prepare(root):
    statistical_checks()
    assert read(COVERAGE / "qualification.json")["status"] == "PUBLIC-CANONICAL-COVERAGE-ENGINEERING-PASS"
    build = read(TOOLS / "build-completion.json")
    old_path = CAMPAIGN / "pilot-001/mixed/config.json"
    old = read(old_path)
    prior = read(CAMPAIGN / "public-learning-engineering-001/structured-config.json")
    source, a48 = prior["source"], prior["updates"][0][0]["opponent"]
    assert source["checkpoint"]["sha256"] == "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
    for model in [source, a48]:
        for field in ["checkpoint", "play_import"]: verify(model[field])
    root.mkdir()
    for name in ["configs", "templates", "requests", "outputs", "logs"]: (root/name).mkdir()
    for name in ["public_feature_training_v1.exe", "public_feature_evaluation_v1.exe"]:
        assert sha(TOOLS/name) == build["binaries"][name]
        shutil.copy2(TOOLS/name, root/name)
    updates = []
    census = Counter()
    training_seeds = set()
    old_seeds = {item["episode"]["seed"] for batch in old["iterations"] for item in batch["episodes"]}
    for index, batch in enumerate(old["iterations"]):
        episodes = []
        for slot, item in enumerate(batch["episodes"]):
            episode = copy.deepcopy(item["episode"])
            episode["id"] = f"public-input-pilot-i{index:03}-s{slot:02}"
            episode["seed"] = seed(f"train/{index}/{slot}")
            assert episode["seed"] not in old_seeds | training_seeds
            training_seeds.add(episode["seed"])
            opponent = "g115" if (index+slot)%2 == 0 else "a48"
            episode["opponent"] = source if opponent == "g115" else a48
            episode["max_physical_decisions"] = 40000
            episode["max_policy_steps"] = 80000
            for registered, selected in zip(episode["registered"], episode["selected"]):
                assert len(selected["mainboard"]) == 60 and len(selected["sideboard"]) == 15
                assert Counter(registered["mainboard"]+registered["sideboard"]) == Counter(selected["mainboard"]+selected["sideboard"])
            census[f"opponent/{opponent}"] += 1
            census[f"postboard/{episode['postboard']}"] += 1
            census[f"learner_seat/{episode['learner_seat']}"] += 1
            census[f"learner/{episode['registered'][episode['learner_seat']]['label']}"] += 1
            episodes.append(episode)
        assert len(episodes) == 10 and sum(e["postboard"] for e in episodes) == 5
        updates.append(episodes)
    assert len(updates) == 200 and census["opponent/g115"] == census["opponent/a48"] == 1000
    configs = {}
    for arm, enabled in [("control", False), ("structured", True)]:
        config = dict(source=source, updates=updates, inputs_enabled=enabled, learning_rate=.0001,
                      value_coefficient=.5, gamma=1., gpu_ordinal=1, max_chunk_substeps=128)
        config["lambda"] = .9
        write(root/f"configs/{arm}.json", config)
        configs[arm] = pin(root/f"configs/{arm}.json")
    jobs = []
    eval_seeds = set()
    cm = read(COVERAGE / "manifest.json")
    for job in cm["jobs"]:
        base = read(verify(job["request"]))
        seat = int(job["label"][-1])
        base["sources"][seat] = None  # Bound only to the named endpoint after audit.
        cases, matches = [], []
        for item in base["matches"]:
            own, opponent = item["config"]["deck_ids"][seat], item["config"]["deck_ids"][1-seat]
            for rep in range(8):
                match = copy.deepcopy(item)
                case_seed = seed(f"eval/{own}/{opponent}/{rep}")
                assert case_seed not in training_seeds
                eval_seeds.add(case_seed)
                match["config"]["seed"] = case_seed
                match["config"]["game_one_chooser"] = seat if rep%2 == 0 else 1-seat
                matches.append(match)
                cases.append(dict(own=own, opponent=opponent, replicate=rep, seed=case_seed,
                                  candidate_seat=seat, candidate_starts=rep%2 == 0))
        base["matches"] = matches
        base["output_directory"] = "UNBOUND"
        write(root/f"templates/{job['label']}.json", base)
        jobs.append(dict(label=job["label"], candidate_seat=seat, cases=cases,
                         template=pin(root/f"templates/{job['label']}.json")))
    assert len(jobs) == 14 and len(eval_seeds) == 392
    write(root/"manifest.json", dict(schema="matched-public-input-pilot/v1", runner=pin(__file__),
        dependencies=[pin(Path(__file__).with_name("qualify_public_evaluation_v1.py"))],
        runner_commit=subprocess.check_output(["git","rev-parse","HEAD"], text=True).strip(),
        native_build=pin(TOOLS/"build-completion.json"), toolchain=read(TOOLS/"build-start.json"),
        training_binary=pin(root/"public_feature_training_v1.exe"), evaluation_binary=pin(root/"public_feature_evaluation_v1.exe"),
        coverage=pin(COVERAGE/"qualification.json"), source=source, training_configs=configs,
        schedule_input=pin(old_path), census=dict(census), jobs=jobs, numpy_version=np.__version__,
        opponent_change="Explicit stationary 50% g115 / 50% A48 replaces old dynamic pool in both arms; no silent current-to-initial resolution.",
        gates=dict(updates=200, games_per_update=10, expected_training_games_per_arm=2000,
            maximum_training_arm_seconds=1800, maximum_qualification_process_seconds=300,
            maximum_evaluation_job_seconds=180, maximum_evaluation_worker_seconds=3600, workers=4,
            expected_matches_per_arm=784, expected_matches=2352, primary_net_wins=24,
            primary_paired_95_lower=0., game_one_retention_paired_95_lower=-.05,
            bootstrap_replicates=10000, bootstrap_seed=202609201835),
        timing_formula="prefix process seconds + 198 * second update seconds + 30 seconds fresh-process restart allowance; require <1800 for both arms",
        advance="All complete; primary and both retention gates plus at least g115 BO3 wins; replication only, never promotion.",
        limitations=["One g115 lineage; familiar stationary training pool and development V3 opponent; seven exact registrations.",
            "KeepSevenV2 and supplied static sideboards; not learned openings or sideboarding and not closed-list deployment.",
            "Combined mana-cost and prevention inputs do not identify their separate effects.",
            "Fable HTTP429 zero-read review gap until September22 07:00EDT; no retry or endorsement; Jack authorized bounded local continuation.",
            "No paid compute, CP7 outcome selection, engineering endpoint parents, human-strength claim or candidate promotion."],
        statistics_checks="Paired common-noise cancellation, exact fixed effect and deterministic resampling passed."))
    print(json.dumps(dict(root=str(root), training_games_per_arm=2000, evaluation_matches=2352, census=dict(census)), indent=2))


def manifest(root):
    m = read(root/"manifest.json")
    assert sha(__file__) == m["runner"]["sha256"]
    for p in m["dependencies"]: verify(p)
    return m


def execute(root, label, binary, request, seconds):
    verify(binary)
    started = time.monotonic()
    with (root/f"logs/{label}.stdout").open("x") as out, (root/f"logs/{label}.stderr").open("x") as err:
        child = subprocess.Popen([str(binary["path"]), str(request)], stdout=out, stderr=err,
            creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
        write(root/f"logs/{label}.start.json", dict(pid=child.pid, command=[binary["path"], str(request)],
                                                  request=pin(request), binary=binary))
        timeout = False
        try: code = child.wait(timeout=seconds)
        except subprocess.TimeoutExpired:
            timeout = True
            child.kill()
            code = child.wait()
    result = dict(exit_code=code, timeout=timeout, seconds=time.monotonic()-started, request=pin(request), binary=binary)
    write(root/f"logs/{label}.execution.json", result)
    assert code == 0 and not timeout, (label, result)
    print(f"{label} complete {result['seconds']:.2f}s", flush=True)
    return result


def train_segment(root, m, arm, label, stop, resume, seconds):
    config = read(verify(m["training_configs"][arm]))
    request = root/f"requests/{label}.json"
    write(request, dict(config=config, output_directory=str(root/"outputs"/label), stop_after=stop, resume=resume))
    return execute(root, label, m["training_binary"], request, seconds)


def audit_training(root, arm, labels, last):
    m = read(root/"manifest.json")
    config = read(verify(m["training_configs"][arm]))
    seen = set()
    state_hash = None
    games = 0
    checkpoint_pin = None
    for label in labels:
        receipt = read(root/f"logs/{label}.execution.json")
        assert receipt["exit_code"] == 0 and not receipt["timeout"]
        folder = root/"outputs"/label
        run = read(folder/"completion.json")
        for update in range(run["first_update"], run["next_update"]):
            assert update not in seen
            seen.add(update)
            batch = folder/f"{update:04}"
            checkpoint = read(batch/"checkpoint.json")
            assert sha(batch/"optimizer.json") == checkpoint["optimizer_sha256"]
            r = read(batch/"receipt.json")
            assert r["after_state_sha256"] == checkpoint["optimizer_sha256"]
            assert state_hash is None or r["before_state_sha256"] == state_hash
            state_hash = r["after_state_sha256"]
            assert checkpoint["next_update"] == update+1
            optimizer = read(batch/"optimizer.json")
            assert optimizer["legacy_adam_step"] == 32401+update and optimizer["public"]["adam_step"] == update+1
            if arm == "control":
                assert all(v in [0,1<<31] for name in ["object","object_first","object_second","state","state_first","state_second"] for v in optimizer["public"][name])
            assert len(checkpoint["trajectory_sha256"]) == r["episodes"] == r["natural_games"] == 10
            for index, expected in enumerate(checkpoint["trajectory_sha256"]):
                path = batch/f"episode-{index:03}.json"
                assert sha(path) == expected
                t = read(path)
                assert t["episode"] == config["updates"][update][index]
                assert t["terminal"]["terminal_classification"] == "natural"
                assert t["optimizer_state_sha256"] == r["before_state_sha256"]
                assert len(t["decisions"]) == len(t["auxiliary"])
                games += 1
            checkpoint_pin = pin(batch/"checkpoint.json")
            if (update+1)%25 == 0: print(f"{arm} audited {update+1} updates", flush=True)
    assert seen == set(range(last)) and games == last*10
    return dict(complete=True, updates=last, games=games, checkpoint=checkpoint_pin,
                optimizer_sha256=state_hash, labels=labels,
                seconds=sum(read(root/f"logs/{label}.execution.json")["seconds"] for label in labels))


def qualify(root):
    m = manifest(root)
    for arm in ["control", "structured"]:
        label = arm+"-prefix"
        ex = train_segment(root,m,arm,label,2,None,300)
        audit = audit_training(root,arm,[label],2)
        second = read(root/f"outputs/{label}/0001/receipt.json")["seconds"]
        projected = ex["seconds"] + 198*second + 30
        write(root/f"{arm}-timing.json", dict(projected_arm_seconds=projected, limit=1800, audited=audit))
        assert projected < 1800, "full-batch timing exceeds frozen arm cap"
    for index in range(10):
        a = read(root/f"outputs/control-prefix/0000/episode-{index:03}.json")
        b = read(root/f"outputs/structured-prefix/0000/episode-{index:03}.json")
        for d in [a,b]: d.pop("config_sha256"); d.pop("inputs_enabled")
        assert a == b, "initial zero-projection arm trajectories differ"
    train_segment(root,m,"structured","replay-first",1,None,300)
    train_segment(root,m,"structured","replay-rest",2,pin(root/"outputs/replay-first/0000/checkpoint.json"),300)
    audit_training(root,"structured",["replay-first","replay-rest"],2)
    for update, label in [(0,"replay-first"),(1,"replay-rest")]:
        for name in ["checkpoint.json","optimizer.json"]+[f"episode-{i:03}.json" for i in range(10)]:
            assert (root/f"outputs/structured-prefix/{update:04}/{name}").read_bytes() == (root/f"outputs/{label}/{update:04}/{name}").read_bytes()
    public = read(root/"outputs/structured-prefix/0001/optimizer.json")["public"]
    assert any(v not in [0,1<<31] for v in public["object"]), "structured object projection did not learn"
    write(root/"training-qualification.json", dict(status="FULL-BATCH-REPLAY-PASS", executed_updates=6, executed_games=60,
        initial_arm_batch_exact_except_identity=True, resumed_optimizer_checkpoint_trajectories_bytes_exact=True,
        object_projection_changed=True, state_projection_changed=any(v not in [0,1<<31] for v in public["state"])))


def evaluation_request(root,m,arm,job):
    request = read(verify(job["template"]))
    if arm == "g115": candidate = dict(kind="legacy", source=m["source"], v3_forced_actions=False)
    else:
        audit = read(root/f"{arm}-training-audit.json")
        assert audit["complete"] and audit["updates"] == 200 and audit["games"] == 2000
        candidate = dict(kind="public_checkpoint", config=m["training_configs"][arm], checkpoint=audit["checkpoint"])
    request["sources"][job["candidate_seat"]] = candidate
    label = arm+"-"+job["label"]
    request["output_directory"] = str(root/"outputs"/label)
    path = root/f"requests/{label}.json"
    write(path,request)
    return label,path


def audit_evaluation(root,arm):
    m = read(root/"manifest.json")
    records = []
    seconds = 0
    for job in m["jobs"]:
        label = arm+"-"+job["label"]
        ex = read(root/f"logs/{label}.execution.json")
        assert ex["exit_code"] == 0 and not ex["timeout"]
        request = read(verify(ex["request"]))
        start = read(root/f"outputs/{label}/start.json")
        assert start["command"] == request
        completion = read(root/f"outputs/{label}/completion.json")
        assert completion["matches"] == len(completion["match_sha256"]) == len(job["cases"]) == 56
        seconds += ex["seconds"]
        games = decisions = 0
        for index,case in enumerate(job["cases"]):
            path = root/f"outputs/{label}/match-{index:06}.json"
            assert sha(path) == completion["match_sha256"][index]
            d = read(path)
            assert d["match"] == request["matches"][index] and d["models"] == start["models"]
            assert d["match"]["config"]["seed"] == case["seed"]
            assert (d["games"][0]["start"]["starting_player"] == case["candidate_seat"]) == case["candidate_starts"]
            assert len(d["games"]) == len(d["seed_resets"]) and not d["decisions"]
            games += len(d["games"]); decisions += d["decision_count"]
            records.append(dict(case=case, match=pin(path)))
        assert games == completion["natural_games"] and decisions == completion["decisions"]
    assert len(records) == 784
    return dict(complete=True, matches=784, seconds=seconds, records=records)


def evaluate(root,arms):
    m = manifest(root)
    assert read(root/"training-qualification.json")["status"] == "FULL-BATCH-REPLAY-PASS"
    jobs = [evaluation_request(root,m,arm,job) for arm in arms for job in m["jobs"]]
    errors = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        pending = [pool.submit(execute,root,label,m["evaluation_binary"],request,180) for label,request in jobs]
        for future in pending:
            try: future.result()
            except Exception as error: errors.append(str(error))
    assert not errors, errors
    for arm in arms:
        result = audit_evaluation(root,arm)
        write(root/f"{arm}-evaluation-audit.json",result)
        if arm == "g115": assert result["seconds"]*3 < 3600


def train(root):
    m = manifest(root)
    assert read(root/"training-qualification.json")["status"] == "FULL-BATCH-REPLAY-PASS"
    assert read(root/"g115-evaluation-audit.json")["complete"]
    for arm in ["control","structured"]:
        used = read(root/f"logs/{arm}-prefix.execution.json")["seconds"]
        assert used < 1800
        train_segment(root,m,arm,arm,200,pin(root/f"outputs/{arm}-prefix/0001/checkpoint.json"),1800-used)
        audit = audit_training(root,arm,[arm+"-prefix",arm],200)
        assert audit["seconds"] < 1800
        write(root/f"{arm}-training-audit.json",audit)


def analyze(root):
    m = manifest(root)
    records = {}
    seconds = 0
    for arm in ARMS:
        audit = audit_evaluation(root,arm)
        seconds += audit["seconds"]
        for row in audit["records"]:
            case = row["case"]; d = read(verify(row["match"])); seat = case["candidate_seat"]
            winner = None if d["outcome"] == "draw" else d["outcome"]["winner"]["winner"]
            g1 = d["games"][0]["winner"]
            key = (case["own"],case["opponent"],case["replicate"],seat,arm)
            assert key not in records
            records[key] = dict(seed=case["seed"], win=int(winner==seat), draw=int(winner is None),
                g1=.5 if g1 is None else int(g1==seat), starts=case["candidate_starts"])
    assert len(records) == 2352 and seconds < 3600
    strata = sorted({key[:2] for key in records})
    values = np.empty((49,8,2,3,2))
    for i,matchup in enumerate(strata):
        for rep in range(8):
            seeds = set()
            for seat in [0,1]:
                for arm_index,arm in enumerate(ARMS):
                    row = records[(*matchup,rep,seat,arm)]
                    seeds.add(row["seed"])
                    values[i,rep,seat,arm_index] = [row["win"],row["g1"]]
            assert len(seeds) == 1
    samples = bootstrap(values,m["gates"]["bootstrap_replicates"],m["gates"]["bootstrap_seed"])
    means = values.mean(axis=(0,1,2)); comparisons = {}
    for a,b in [(2,1),(2,0),(1,0)]:
        comparisons[f"{ARMS[a]}-minus-{ARMS[b]}"] = dict(net_wins=int(round((means[a,0]-means[b,0])*784)),
            win_fraction_difference=float(means[a,0]-means[b,0]),
            paired_95_interval=np.quantile(samples[:,a,0]-samples[:,b,0],[.025,.975]).tolist(),
            game_one_difference=float(means[a,1]-means[b,1]),
            game_one_paired_95_interval=np.quantile(samples[:,a,1]-samples[:,b,1],[.025,.975]).tolist())
    c,r = comparisons["structured-minus-control"],comparisons["structured-minus-g115"]
    gates = dict(primary=c["net_wins"]>=24 and c["paired_95_interval"][0]>0,
        retention=all(x["game_one_paired_95_interval"][0]>-.05 for x in [c,r]), at_least_g115_wins=r["net_wins"]>=0)
    summary = {arm:dict(matches=784,wins=sum(r["win"] for k,r in records.items() if k[-1]==arm),
        draws=sum(r["draw"] for k,r in records.items() if k[-1]==arm), game_one_score=sum(r["g1"] for k,r in records.items() if k[-1]==arm)) for arm in ARMS}
    breakdown = []
    for own,opponent in strata:
        for seat in [0,1]:
            for starts in [False,True]:
                for arm in ARMS:
                    rows = [v for k,v in records.items() if k[:2]==(own,opponent) and k[3:]==(seat,arm) and v["starts"]==starts]
                    assert len(rows)==4
                    breakdown.append(dict(own=own,opponent=opponent,seat=seat,starts=starts,arm=arm,matches=4,
                        wins=sum(r["win"] for r in rows),draws=sum(r["draw"] for r in rows),game_one_score=sum(r["g1"] for r in rows)))
    result = dict(complete=True,summary=summary,comparisons=comparisons,gates=gates,
        verdict="REPLICATE" if all(gates.values()) else "NO-ADVANCE",breakdown=breakdown,
        worker_seconds=seconds,manifest=pin(root/"manifest.json"),limitations=m["limitations"],promotion=False,human_strength_claim=False)
    write(root/"analysis.json",result)
    print(json.dumps({k:result[k] for k in ["verdict","summary","comparisons","gates"]},indent=2))


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["prepare","qualify","reference","train","evaluate","analyze"])
    p.add_argument("--root",type=Path,required=True)
    a=p.parse_args(); root=a.root.resolve()
    if a.mode=="prepare": prepare(root)
    elif a.mode=="qualify": qualify(root)
    elif a.mode=="reference": evaluate(root,["g115"])
    elif a.mode=="train": train(root)
    elif a.mode=="evaluate": evaluate(root,["control","structured"])
    else: analyze(root)


if __name__ == "__main__": main()
