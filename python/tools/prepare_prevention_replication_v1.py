"""An exact protocol replication with disjoint seeds and the original parent."""
import argparse
import copy
import hashlib
from pathlib import Path
import subprocess
from public_training_dispatch_v2 import read, write, pin, checked
from state_prevention_pilot_v1 import collect_seeds

NAMESPACE = "matched-state-prevention-replication-002/v1/"


def seed(label):
    return int.from_bytes(hashlib.sha256((NAMESPACE+label).encode()).digest()[:8], "little")


def prepare(root, original):
    old = read(original/"manifest.json")
    result = read(original/"analysis.json")
    assert result["complete"] and result["verdict"] == "REPLICATE"
    assert all(result["gates"].values())
    root.mkdir()
    (root/"configs").mkdir(); (root/"templates").mkdir()
    forbidden = set()
    for item in list(old["training_configs"].values())+[job["template"] for job in old["jobs"]]:
        forbidden.update(collect_seeds(read(checked(item))))
    for item in old["prior_seed_inputs"]:
        forbidden.update(collect_seeds(read(checked(item))))
    train_seeds, eval_seeds, configs = set(), set(), {}
    for arm, source_pin in old["training_configs"].items():
        base = read(checked(source_pin))
        config = copy.deepcopy(base)
        for index, batch in enumerate(config["updates"]):
            for slot, episode in enumerate(batch):
                episode["seed"] = seed(f"train/{index}/{slot}")
                episode["id"] = f"state-prevention-replication-i{index:03}-s{slot:02}"
                assert episode["seed"] not in forbidden
                train_seeds.add(episode["seed"])
                a,b=copy.deepcopy(episode),copy.deepcopy(base["updates"][index][slot])
                for key in ["seed", "id"]: a.pop(key); b.pop(key)
                assert a == b, "replication changed the exposure schedule"
        assert config["source"] == old["source"]
        write(root/f"configs/{arm}.json", config)
        configs[arm] = pin(root/f"configs/{arm}.json")
    assert len(train_seeds) == 2000
    jobs = copy.deepcopy(old["jobs"])
    for job in jobs:
        command = read(checked(job["template"]))
        for case, match in zip(job["cases"], command["matches"]):
            value = seed(f"eval/{case['cohort']}/{case['own']}/{case['opponent']}/{case['replicate']}")
            assert value not in forbidden | train_seeds
            case["seed"] = match["config"]["seed"] = value
            eval_seeds.add(value)
        write(root/f"templates/{job['label']}.json", command)
        job["template"] = pin(root/f"templates/{job['label']}.json")
    assert len(eval_seeds) == 436
    manifest = copy.deepcopy(old)
    manifest.update(schema="matched-state-prevention-replication/v1", stage="prepared-not-launched",
        runner=pin(__file__), source_commit=subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),
        dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_training_dispatch_v2.py", "state_prevention_pilot_v1.py"]],
        training_configs=configs, jobs=jobs, seed_namespace=NAMESPACE,
        discovery_manifest=pin(original/"manifest.json"), discovery_analysis=pin(original/"analysis.json"),
        replication="Both arms restart from untouched g115. No continuation from the successful treatment. Only training/evaluation seeds and episode IDs change; all scientific gates remain identical.",
        allocation="Must qualify SSD native training plus verified E archive/recovery; no allocation yet.")
    assert manifest["gates"] == old["gates"]
    write(root/"manifest.json", manifest)
    write(root/"replication-check.json", dict(complete=True, independent_training_seeds=2000,
        independent_evaluation_seeds=436, original_parent_unchanged=True,
        exposure_schedule_unchanged_except_ids_and_seeds=True, scientific_gates_identical=True,
        bootstrap_seeds_unchanged=True, no_training_or_evaluation_launched=True))
    print(dict(prepared=str(root), training_games_per_arm=2000, evaluation_matches=2616,
        seed_namespace=NAMESPACE, gates="unchanged"),flush=True)


if __name__ == "__main__":
    p=argparse.ArgumentParser();p.add_argument("--root",type=Path,required=True);p.add_argument("--original",type=Path,required=True)
    a=p.parse_args();prepare(a.root,a.original)
