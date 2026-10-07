"""Prepare the fixed matched prevention question; preparation never launches work."""
import argparse
import copy
from collections import Counter, defaultdict
import hashlib
from pathlib import Path
import subprocess

from qualify_state_prevention_evaluation_v1 import CAMPAIGN, COVERAGE, GATES, verify
from qualify_public_evaluation_v1 import read, write, pin

ARMS = ["g115", "control", "structured"]


def seed(text):
    return int.from_bytes(hashlib.sha256(("matched-state-prevention/v1/"+text).encode()).digest()[:8], "little")


def collect_seeds(doc):
    if isinstance(doc, dict):
        for key, value in doc.items():
            if key == "seed" and isinstance(value, int):
                yield value
            else:
                yield from collect_seeds(value)
    elif isinstance(doc, list):
        for item in doc:
            yield from collect_seeds(item)


def prepare(root, qualification, training_binary):
    assert read(qualification/"qualification.json")["complete"]
    qm = read(qualification/"manifest.json")
    old_path = CAMPAIGN/"public-feature-pilot-001/configs/structured.json"
    old = read(old_path)
    exposed = read(CAMPAIGN/"public-state-only-engineering-001/configs/structured.json")
    source, a48 = exposed["source"], exposed["updates"][0][0]["opponent"]
    assert source == old["source"]
    assert source["checkpoint"]["sha256"] == "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
    for model in [source, a48]:
        for field in ["checkpoint", "play_import"]:
            verify(model[field])
    forbidden = set()
    old_inputs = []
    for name in ["public-feature-pilot-001", "broader-exposure-pilot-001", "public-state-only-engineering-001"]:
        for directory in ["configs", "templates"]:
            for path in sorted((CAMPAIGN/name/directory).glob("*.json")):
                old_inputs.append(pin(path))
                forbidden.update(collect_seeds(read(path)))
    forbidden.update(collect_seeds(read(qualification/"manifest.json")))
    for job in qm["jobs"]:
        forbidden.update(collect_seeds(read(verify(job["request"]))))
    roster, matches = {}, {}
    for job in read(COVERAGE/"manifest.json")["jobs"]:
        seat = int(job["label"][-1])
        request = read(verify(job["request"]))
        for match in request["matches"]:
            own, other = match["config"]["deck_ids"][seat], match["config"]["deck_ids"][1-seat]
            assert (own, other, seat) not in matches
            matches[own, other, seat] = match
            for deck in match["registered"]:
                assert deck["label"] not in roster or roster[deck["label"]] == deck
                roster[deck["label"]] = deck
    assert len(matches) == 98 and len(roster) == 7
    canonical = sorted(roster)
    fixture = read(CAMPAIGN/"prevention-correction-replay-001/request-seat0.json")
    roster[GATES] = fixture["registrations"][0]
    assert roster[GATES]["label"] == GATES and roster[GATES]["mainboard"].count(88) == 4
    for batch in old["updates"]:
        for episode in batch:
            for deck in episode["registered"]:
                reference = roster[deck["label"].split("/")[0]]
                assert all(Counter(deck[zone]) == Counter(reference[zone]) for zone in ["mainboard", "sideboard"])
    assert len(old["updates"]) == 200 and all(len(b) == 10 for b in old["updates"])
    root.mkdir()
    for name in ["configs", "templates"]:
        (root/name).mkdir()
    census, joint = Counter(), Counter()
    updates, train_seeds = [], set()
    for index, batch in enumerate(old["updates"]):
        new_batch = []
        for slot, original in enumerate(batch):
            episode = copy.deepcopy(original)
            prior_seat = episode["learner_seat"]
            seat = (index % 2) ^ (slot % 2)
            candidate_starts = ((index//2) % 2) ^ ((slot//2) % 2)
            other_model = ((index//4) % 2) ^ (slot % 2)
            if slot < 4:
                canonical_deck = canonical[(2*index+slot%2) % 7]
                own, other = (GATES, canonical_deck) if slot < 2 else (canonical_deck, GATES)
                registered = [copy.deepcopy(roster[own]), copy.deepcopy(roster[other])]
                if seat == 1:
                    registered.reverse()
                episode["registered"] = registered
                episode["selected"] = copy.deepcopy(registered)
                episode["postboard"] = bool((index+slot) % 2)
                cohort = "focal"
            else:
                if prior_seat != seat:
                    episode["registered"].reverse()
                    episode["selected"].reverse()
                for deck in episode["registered"]+episode["selected"]:
                    deck["label"] = deck["label"].split("/")[0]
                own, other = [episode["registered"][s]["label"] for s in [seat, 1-seat]]
                cohort = "canonical"
            episode.update(id=f"state-prevention-i{index:03}-s{slot:02}", seed=seed(f"train/{index}/{slot}"),
                learner_seat=seat, starting_player=seat if candidate_starts else 1-seat,
                opponent=copy.deepcopy(source if other_model == 0 else a48))
            assert episode["seed"] not in forbidden | train_seeds
            train_seeds.add(episode["seed"])
            for registered, selected in zip(episode["registered"], episode["selected"]):
                assert len(selected["mainboard"]) == 60 and len(selected["sideboard"]) == 15
                assert Counter(registered["mainboard"]+registered["sideboard"]) == Counter(selected["mainboard"]+selected["sideboard"])
            for key in [f"cohort/{cohort}", f"learner/{own}", f"opponent_deck/{other}",
                        f"seat/{seat}", f"candidate_starts/{candidate_starts}", f"opponent_model/{other_model}",
                        f"postboard/{episode['postboard']}"]:
                census[key] += 1
            joint[cohort, seat, candidate_starts, other_model] += 1
            new_batch.append(episode)
        updates.append(new_batch)
    assert census["cohort/focal"] == 800 and census["cohort/canonical"] == 1200
    assert {count for (cohort, *_), count in joint.items() if cohort == "focal"} == {100}
    assert {count for (cohort, *_), count in joint.items() if cohort == "canonical"} == {150}
    configs = {}
    for arm in ["control", "structured"]:
        config = copy.deepcopy(old)
        config.update(updates=updates, inputs_enabled=arm == "structured", projection_mode="state_only")
        write(root/f"configs/{arm}.json", config)
        configs[arm] = pin(root/f"configs/{arm}.json")
    groups = defaultdict(list)
    eval_seeds = set()
    for cohort, repetitions in [("focal", 16), ("canonical", 4)]:
        pairs = [(own, other) for own in sorted(roster) for other in sorted(roster)
                 if (GATES in (own, other)) == (cohort == "focal")]
        assert len(pairs) == (15 if cohort == "focal" else 49)
        for own, other in pairs:
            for rep in range(repetitions):
                env_seed = seed(f"eval/{cohort}/{own}/{other}/{rep}")
                assert env_seed not in forbidden | train_seeds | eval_seeds
                eval_seeds.add(env_seed)
                for seat in [0, 1]:
                    if cohort == "canonical":
                        match = copy.deepcopy(matches[own, other, seat])
                    else:
                        registered = [roster[own], roster[other]]
                        if seat == 1:
                            registered.reverse()
                        match = dict(config=copy.deepcopy(fixture["config"]),
                                     registered=registered, postboard=None)
                        match["config"]["deck_ids"] = [d["label"] for d in registered]
                    match["config"].update(seed=env_seed, game_one_chooser=seat if rep%2 == 0 else 1-seat)
                    case = dict(cohort=cohort, own=own, opponent=other, replicate=rep, seed=env_seed,
                                candidate_seat=seat, candidate_starts=rep%2 == 0)
                    groups[cohort, own, seat].append((case, match))
    jobs = []
    for (cohort, own, seat), cases in sorted(groups.items()):
        label = f"{cohort}-{own}-p{seat}"
        opponent = qm["opponent"]
        request = dict(sources=[None, opponent] if seat == 0 else [opponent, None],
            matches=[item for _, item in cases], cross_generation_evaluation=True,
            capture_decisions=False, output_directory="UNBOUND")
        write(root/f"templates/{label}.json", request)
        jobs.append(dict(label=label, cohort=cohort, candidate_seat=seat,
            cases=[case for case, _ in cases], template=pin(root/f"templates/{label}.json")))
    counts = Counter()
    for job in jobs:
        counts[job["cohort"]] += len(job["cases"])
    assert counts == {"focal": 480, "canonical": 392} and len(eval_seeds) == 436
    control, structured = [read(configs[arm]["path"]) for arm in ["control", "structured"]]
    config_difference = {k for k in control if control[k] != structured[k]}
    assert config_difference == {"inputs_enabled"}
    write(root/"manifest.json", dict(schema="matched-state-prevention-pilot/v1", stage="prepared-not-launched",
        runner=pin(__file__), source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        design=pin(Path("docs/state_prevention_pilot_20260920.md").resolve()),
        dependencies=[pin(Path(__file__).with_name(name)) for name in
            ["qualify_state_prevention_evaluation_v1.py", "qualify_public_evaluation_v1.py"]],
        training_binary=pin(training_binary), evaluation_binary=qm["binary"],
        evaluation_qualification=pin(qualification/"qualification.json"), evaluation_opponent=qm["opponent"],
        source=source, training_configs=configs, schedule_input=pin(old_path), prior_seed_inputs=old_inputs,
        census=dict(census), independent_balance={"/".join(map(str, key)): value for key, value in sorted(joint.items())},
        training_unique_seeds=len(train_seeds), evaluation_unique_seeds=len(eval_seeds), jobs=jobs,
        expected_training_games_per_arm=2000, expected_matches_per_arm=dict(counts), expected_matches=2616,
        gates=dict(primary_focal_net_wins=15, primary_paired_95_lower=0.,
            focal_wins_at_least_g115=True, canonical_retention_paired_95_lower=-.05,
            bootstrap_replicates=10000, focal_bootstrap_seed=202609202130, canonical_bootstrap_seed=202609202131,
            final_update=199, terminal_rewards=True),
        allocation="Not yet qualified. Must bind representative ten-game throughput evidence and wall caps before substantial dispatch.",
        advance="All complete and all gates pass: REPLICATE only. Otherwise NO-ADVANCE. No promotion.",
        review="Fable zero-read HTTP429 until September22 07:00EDT; no repeated retry or endorsement. Bounded continuation under the maintainer's assignment.",
        non_claim="One lineage, one Gates list, familiar V3 development opposition, sampled KeepSevenV2 and static/Keep boards. No CP7 selection or human/league competence claim."))
    print(dict(prepared=str(root), training_unique_seeds=len(train_seeds), evaluation_matches=2616,
        census=dict(census), independent_balance=dict(joint)), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--qualification", type=Path, required=True)
    parser.add_argument("--training-binary", type=Path, required=True)
    args = parser.parse_args()
    prepare(args.root, args.qualification, args.training_binary)
