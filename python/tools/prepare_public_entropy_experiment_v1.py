"""Freeze two matched entropy replicas and balanced BO3 panels; never launches work."""
import argparse
import copy
from collections import Counter, defaultdict
import hashlib
from pathlib import Path
import subprocess

from public_training_dispatch_v2 import read, write, pin, checked
from state_prevention_pilot_v1 import collect_seeds

NAMESPACE = "public-entropy-two-replicas-20260921/v1/"
CAMPAIGN = Path("E:/mtg-postboard-campaign-20260920")
BUILD = Path("E:/mtg-meta-recovery-20260921/public-entropy-tools-002/build-completion.json")


def seed(label):
    return int.from_bytes(hashlib.sha256((NAMESPACE + label).encode()).digest()[:8], "little")


def prepare(root):
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    build = read(BUILD)
    assert build["exit_code"] == 0
    binaries = {name: {k: item[k] for k in ["path", "sha256"]}
                for name, item in build["binaries"].items()}
    for item in binaries.values():
        checked(item)
    old = read(CAMPAIGN / "state-prevention-pilot-001/manifest.json")
    base = read(checked(old["training_configs"]["control"]))
    assert base["inputs_enabled"] is False and base["projection_mode"] == "state_only"
    assert len(base["updates"]) == 200 and all(len(b) == 10 for b in base["updates"])
    assert base["source"]["checkpoint"]["sha256"] == "88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1"
    forbidden, prior = set(), {}

    def prior_input(item):
        path = checked(item)
        if str(path) not in prior:
            prior[str(path)] = item
            forbidden.update(collect_seeds(read(path)))

    for name in ["state-prevention-pilot-001", "state-prevention-pilot-002"]:
        manifest = read(CAMPAIGN / name / "manifest.json")
        for item in list(manifest["training_configs"].values()) + manifest["prior_seed_inputs"]:
            prior_input(item)
        for job in manifest["jobs"]:
            prior_input(job["template"])
    prior_input(pin(Path("E:/mtg-meta-recovery-20260921/public-entropy-engineering-001/entropy-config.json")))
    matches, roster = {}, set()
    for job in old["jobs"]:
        request = read(checked(job["template"]))
        assert len(request["matches"]) == len(job["cases"])
        for case, match in zip(job["cases"], request["matches"]):
            key = case["own"], case["opponent"], case["candidate_seat"]
            matches.setdefault(key, match)
            roster.update(key[:2])
    assert len(roster) == 8 and len(matches) == 128
    root.mkdir()
    used = set()
    replica_manifests = []
    design = pin(Path("docs/public_entropy_experiment_draft_20260921.md").resolve())
    for replica in [1, 2]:
        folder = root / f"replica-{replica}"
        folder.mkdir(); (folder / "configs").mkdir(); (folder / "templates").mkdir()

        def fresh(label):
            value = seed(f"replica-{replica}/{label}")
            assert value not in forbidden and value not in used
            used.add(value)
            return value

        config = copy.deepcopy(base)
        census = Counter()
        for index, batch in enumerate(config["updates"]):
            for slot, episode in enumerate(batch):
                episode["seed"] = fresh(f"train/{index}/{slot}")
                episode["id"] = f"entropy-r{replica}-i{index:03}-s{slot:02}"
                original = copy.deepcopy(base["updates"][index][slot])
                current = copy.deepcopy(episode)
                for key in ["seed", "id"]:
                    original.pop(key); current.pop(key)
                assert original == current, "changed exposure schedule"
                seat = episode["learner_seat"]
                census[f"own/{episode['registered'][seat]['label']}"] += 1
                census[f"opponent/{episode['registered'][1-seat]['label']}"] += 1
                census[f"seat/{seat}"] += 1
                census[f"postboard/{episode['postboard']}"] += 1
        configs = {}
        for arm, beta in [("control", 0.0), ("entropy", 0.05)]:
            arm_config = copy.deepcopy(config)
            arm_config["entropy_coefficient"] = beta
            write(folder / f"configs/{arm}.json", arm_config)
            configs[arm] = pin(folder / f"configs/{arm}.json")
        a, b = [read(checked(configs[arm])) for arm in ["control", "entropy"]]
        assert {key for key in a if a[key] != b[key]} == {"entropy_coefficient"}
        groups = defaultdict(list)
        for own in sorted(roster):
            for other in sorted(roster):
                for repeat in range(8):
                    value = fresh(f"eval/{own}/{other}/{repeat}")
                    for seat in [0, 1]:
                        match = copy.deepcopy(matches[own, other, seat])
                        match["config"].update(seed=value, game_one_chooser=seat if repeat % 2 == 0 else 1-seat)
                        case = dict(cohort="balanced", own=own, opponent=other, replicate=repeat,
                            seed=value, candidate_seat=seat, candidate_starts=repeat % 2 == 0)
                        groups[own, seat].append((case, match))
        jobs = []
        for (own, seat), rows in sorted(groups.items()):
            label = f"balanced-{own}-p{seat}"
            opponent = old["evaluation_opponent"]
            request = dict(sources=[None, opponent] if seat == 0 else [opponent, None],
                matches=[match for _, match in rows], cross_generation_evaluation=True,
                capture_decisions=False, output_directory="UNBOUND")
            write(folder / f"templates/{label}.json", request)
            jobs.append(dict(label=label, cohort="balanced", candidate_seat=seat,
                cases=[case for case, _ in rows], template=pin(folder / f"templates/{label}.json")))
        cells = Counter((c["own"], c["opponent"], c["candidate_seat"], c["candidate_starts"])
                        for job in jobs for c in job["cases"])
        assert len(cells) == 256 and set(cells.values()) == {4}
        manifest = dict(schema="matched-public-entropy/v1", stage="prepared-not-launched",
            replica=replica, runner=pin(__file__), design=design,
            source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
            dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_dispatch_v2.py", "state_prevention_pilot_v1.py"]],
            native_build=pin(BUILD), training_binary=binaries["public_feature_training_v1"],
            evaluation_binary=binaries["public_feature_evaluation_v1"],
            evaluation_opponent=old["evaluation_opponent"], source=base["source"],
            training_configs=configs, jobs=jobs, schedule_input=old["training_configs"]["control"],
            prior_seed_inputs=list(prior.values()), seed_namespace=NAMESPACE,
            training_unique_seeds=2000, evaluation_unique_seeds=512, training_census=dict(census),
            expected_training_games_per_arm=2000, expected_matches_per_arm=1024, expected_matches=3072,
            gates=dict(net_bo3_wins_over_control=20, paired_bo3_95_lower=0.0,
                wins_at_least_g115=True, game_one_paired_95_lower=-0.05,
                bootstrap_replicates=10000, bootstrap_seed=fresh("bootstrap"), final_update=199),
            allocation="Unqualified; supported storage/throughput launcher required before substantial work.",
            coefficient_reason="Single beta0.05 hypothesis after analytic gradient and real optimizer qualification; not selected using treatment outcome prefixes.",
            review="Independent Fable review unavailable: known zero-read HTTP429 until Sep22 07:00EDT. No retry or endorsement; bounded work under the maintainer's research authority.",
            non_claim="Both replicas must independently pass; no pooled rescue, automatic promotion, CP7 selection or human/league competence claim. Fixed development opponent, KeepSeven and static/Keep boards remain limitations.")
        write(folder / "manifest.json", manifest)
        replica_manifests.append(pin(folder / "manifest.json"))
    assert len(used) == 5026
    write(root / "manifest.json", dict(schema="two-replica-public-entropy/v1", replicas=replica_manifests,
        runner=pin(__file__), design=design, training_games=8000, evaluation_bo3=6144,
        unique_training_seeds=4000, unique_evaluation_seeds=1024, independent_bootstrap_seeds=2,
        prior_seed_count=len(forbidden), paired_arms=True, schedule_preserved=True,
        measurement_started=False, both_replicas_required=True, paid_compute=False))
    print(dict(prepared=str(root), training_games=8000, evaluation_bo3=6144, launched=False), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    prepare(parser.parse_args().root)
