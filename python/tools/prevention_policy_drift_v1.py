"""Pinned, bounded same-input comparison of all four prevention endpoints."""
import argparse
import copy
from collections import Counter, defaultdict
import math
from pathlib import Path
import statistics
import time

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, prepare_remote
from public_replay_dispatch_v1 import dispatch

PILOTS = [Path(f"E:/mtg-postboard-campaign-20260920/state-prevention-pilot-{i:03}") for i in (1, 2)]
BUILD = Path("E:/mtg-meta-recovery-20260921/prevention-replay-tools-001/build-completion.json")
ARMS = ["control", "structured"]


def prepare(root):
    build = read(BUILD)
    assert build["exit_code"] == 0
    checked(build["binary"])
    root.mkdir()
    assets, panel, models, qualifications = {}, [], [], []
    def add(item):
        path = checked(item)
        assets[str(path)] = item
        return path
    def add_checkpoint(item):
        path = add(item)
        c = read(path)
        add(dict(path=str(path.parent/c["optimizer_file"]), sha256=c["optimizer_sha256"]))
    original = []
    for training, pilot in enumerate(PILOTS, 1):
        m = read(pilot/"manifest.json")
        audit = read(pilot/"training-audit.json")
        assert audit["complete"] and audit["full_natural_games"] == 4000
        original += [pin(pilot/"manifest.json"), pin(pilot/"training-audit.json")]
        config = read(add(m["training_configs"]["control"]))
        strata = {}
        for update, batch in enumerate(config["updates"]):
            for episode_index, e in enumerate(batch):
                seat = e["learner_seat"]
                key = (e["registered"][seat]["label"], e["postboard"], seat, e["opponent"]["checkpoint"]["sha256"])
                strata[key] = update, episode_index
        assert len(strata) == 60
        for arm in ARMS:
            cfg_pin = m["training_configs"][arm]
            assert read(add(cfg_pin))["updates"] == config["updates"]
            arm_audit = audit["arms"][arm]
            report = read(add(arm_audit["report"]))
            model = dict(kind="checkpoint", label=f"t{training}-{arm}", config=cfg_pin, checkpoint=arm_audit["checkpoint"])
            add_checkpoint(model["checkpoint"])
            models.append(model)
            for key, (update, index) in sorted(strata.items()):
                item = report["outputs"][f"{update:04}/episode-{index:03}.json"]
                checkpoint = read(checked(report["outputs"][f"{update:04}/checkpoint.json"]))
                assert checkpoint["trajectory_sha256"][index] == item["sha256"]
                add(item)
                episode = config["updates"][update][index]
                panel.append(dict(training=training, source_arm=arm, own=key[0], postboard=key[1], seat=key[2],
                    opponent_checkpoint=key[3], opponent=episode["registered"][1-key[2]]["label"],
                    update=update, episode_index=index, trajectory=item))
            before = report["outputs"]["0198/checkpoint.json"]
            add_checkpoint(before)
            qmodel = dict(model, label=f"t{training}-{arm}-before-final", checkpoint=before)
            trajectory = report["outputs"]["0199/episode-000.json"]
            add(trajectory)
            qualifications.append(dict(models=[qmodel, dict(qmodel, label=qmodel["label"]+"-duplicate")], trajectory=trajectory))
        # Base imports have further pinned tensors; the existing remote base also contains them.
        for item in config["source"].values():
            if isinstance(item, dict) and "path" in item and "sha256" in item:
                path = add(item)
                if path.suffix == ".json":
                    value = read(path)
                    for nested in value.values() if isinstance(value, dict) else []:
                        if isinstance(nested, dict) and "path" in nested and "sha256" in nested:
                            add(nested)
    assert len(panel) == 240
    # Corrupt a guaranteed selected first genuine-choice row. Native code must reject it.
    fixtures = root/"fixtures"
    fixtures.mkdir()
    for index, item in enumerate(qualifications):
        value = read(checked(item["trajectory"]))
        corrupt = copy.deepcopy(value)
        row = next(r for r in corrupt["decisions"] if r["actor"] == corrupt["episode"]["learner_seat"] and len(r["logits"]) > 1)
        row["value"] ^= 1
        path = fixtures/f"corrupt-{index}.json"
        write(path, corrupt)
        item["corrupt"] = pin(path)
        add(item["corrupt"])
        for deck in value["episode"]["selected"]:
            deck["mainboard"].reverse()
        value["terminal"]["terminal_reward"] = [-v for v in value["terminal"]["terminal_reward"]]
        path = fixtures/f"metadata-{index}.json"
        write(path, value)
        item["metadata"] = pin(path)
        add(item["metadata"])
    dependencies = [pin(Path(__file__).with_name(name)) for name in ["public_replay_dispatch_v1.py", "public_evaluation_dispatch_v1.py"]]
    plan = dict(schema="prevention-policy-drift/v1", binary=build["binary"], build=pin(BUILD), original=original,
        runner=pin(__file__), dependencies=dependencies,
        design=pin(Path(__file__).parents[2]/"docs/prevention_policy_drift_design_20260921.md"),
        models=models, panel=panel, qualifications=qualifications, max_choice_rows=64,
        timing_indices=list(range(0, 240, 10)), worker_counts=[1, 4, 8], process_cap_seconds=180,
        selection="Last scheduled episode per own/pre-postboard/seat/opponent-model stratum, both source arms and schedules; no outcomes used",
        limitations="Late development training states; unbalanced ordered matchups; deterministic 64-choice subsample; descriptive policy diagnostics, no strength/promotion/causal claim.",
        review="Known zero-read Fable HTTP429 until Sep22 07:00EDT; no repeated retry or endorsement.")
    write(root/"plan.json", plan)
    remote = prepare_remote(root, list(assets.values()))
    print(dict(prepared=str(root), trajectories=len(panel), remote_setup_seconds=remote["seconds"]), flush=True)


def validate_plan(root):
    p = read(root/"plan.json")
    assert p["runner"] == pin(__file__)
    for item in p["dependencies"]+[p["design"], p["build"], p["binary"]]+p["original"]:
        checked(item)
    return p


def command(p, trajectories, workers, models=None, minimum=0):
    return dict(models=models or p["models"], trajectories=trajectories, workers=workers,
        max_choice_rows_per_trajectory=p["max_choice_rows"], minimum_behavior_replay_rows=minimum)


def qualify(root):
    p = validate_plan(root)
    remote = read(root/"remote-staging.json")
    availability = {}
    for host in ["jack", "haleyspc"]:
        current = inventory(host)
        write(root/f"{host}-inventory.json", current)
        availability[host] = dict(evidence=pin(root/f"{host}-inventory.json"), eligible=not current["active"],
            reason="No competing native owner" if not current["active"] else "Existing native owners excluded by current whole-host no-overlap guard; no global shared-host optimum claim")
    cloud = Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    availability["runpod"] = dict(evidence=pin(cloud), eligible=False, reason="Latest authenticated inventory HTTP403; no new paid compute authority")
    write(root/"availability.json", availability)
    hosts = [h for h in ["jack", "haleyspc"] if availability[h]["eligible"]]
    assert hosts, "No idle eligible host"
    host = hosts[0]
    for index, q in enumerate(p["qualifications"]):
        result = dispatch(root, f"behavior-{index}", p["binary"], command(p, [q["trajectory"]], 1, q["models"], 2), host, remote)
        data = read(Path(result["output_directory"])/"trajectory-000.json")
        assert data["behavior_replay_rows"] == 2*len(data["rows"])
        for row in data["rows"]:
            a,b = row["models"]
            assert a["logits_bits"] == b["logits_bits"] and a["value_bits"] == b["value_bits"]
            assert row["comparisons"][0]["forward_kl"] == row["comparisons"][0]["total_variation"] == 0
            assert not row["comparisons"][0]["top_action_changed"]
        failure = dispatch(root, f"corrupt-{index}", p["binary"], command(p, [q["corrupt"]], 1, q["models"], 2), host, remote, expected_failure=True)
        assert "same-state policy replay differs" in Path(failure["stderr"]).read_text()
        altered = dispatch(root, f"metadata-{index}", p["binary"], command(p, [q["metadata"]], 1, q["models"], 2), host, remote)
        assert read(Path(altered["output_directory"])/"trajectory-000.json")["rows"] == data["rows"]
    mini = [p["panel"][i]["trajectory"] for i in p["timing_indices"]]
    measurements = []
    baseline = None
    for host in hosts:
        for workers in p["worker_counts"]:
            label = f"timing-{host}-w{workers}"
            r = dispatch(root, label, p["binary"], command(p, mini, workers), host, remote)
            hashes = r["fingerprints"]
            assert baseline is None or hashes == baseline, "parallel or host change altered diagnostic outputs"
            baseline = hashes
            setup = remote["seconds"] if host == "haleyspc" else 0
            projection = setup+r["staging_seconds"]+len(p["panel"])/len(mini)*(r["execution_seconds"]+r["recovery_seconds"])
            measurements.append(dict(id=label, host=host, workers=workers, report=pin(root/label/"result.json"), projected_seconds=projection))
            print(dict(qualified=label, seconds=r["seconds"], projected_seconds=projection), flush=True)
    selected = min(measurements, key=lambda m:m["projected_seconds"])
    write(root/"qualification.json", dict(plan=pin(root/"plan.json"), availability=pin(root/"availability.json"),
        behavior_replay_exact=True, duplicate_zero=True, corrupted_behavior_rejected=True, unused_metadata_invariant=True,
        all_parallel_outputs_exact=True, measurements=measurements, selected=selected,
        remote_setup_seconds=remote["seconds"], baseline_fingerprints=baseline))


def require_choice(root, p):
    q = read(root/"qualification.json")
    assert q["plan"] == pin(root/"plan.json")
    for key in ["behavior_replay_exact", "duplicate_zero", "corrupted_behavior_rejected", "unused_metadata_invariant", "all_parallel_outputs_exact"]:
        assert q[key] is True
    available = read(checked(q["availability"]))
    assert set(available) == {"jack", "haleyspc", "runpod"}
    for a in available.values(): checked(a["evidence"])
    counts = defaultdict(set)
    for m in q["measurements"]:
        assert available[m["host"]]["eligible"]
        r = read(checked(m["report"]))
        assert r["binary"] == p["binary"] and r["fingerprints"] == q["baseline_fingerprints"]
        request = read(checked(r["request"]))
        expected = command(p, [p["panel"][i]["trajectory"] for i in p["timing_indices"]], m["workers"])
        assert {k:v for k,v in request.items() if k != "output_directory"} == expected
        for name,digest in r["fingerprints"].items():
            assert pin(Path(r["output_directory"])/name)["sha256"] == digest
        execution = read(checked(r["execution"]))
        assert execution["exit_code"] == 0 and not execution["timeout"]
        assert execution["binary"] == p["binary"] and execution["request"] == r["request"]
        projection = (q["remote_setup_seconds"] if m["host"] == "haleyspc" else 0)+r["staging_seconds"]+10*(r["execution_seconds"]+r["recovery_seconds"])
        assert m["projected_seconds"] == projection and 0 < projection < 1800
        counts[m["host"]].add(m["workers"])
    assert all(v == set(p["worker_counts"]) for v in counts.values())
    assert set(counts) == {h for h,a in available.items() if a["eligible"]}
    assert q["selected"] == min(q["measurements"], key=lambda m:m["projected_seconds"])
    return q["selected"]


def run(root):
    p = validate_plan(root)
    choice = require_choice(root, p)
    result = dispatch(root, "panel", p["binary"], command(p, [i["trajectory"] for i in p["panel"]], choice["workers"]),
        choice["host"], read(root/"remote-staging.json"))
    print(dict(complete=True, trajectories=len(p["panel"]), host=choice["host"], workers=choice["workers"], seconds=result["seconds"]), flush=True)


def summarize(rows):
    result = dict(rows=len(rows), trajectories=len({r["trajectory_index"] for r in rows}), pairs=[], models=[])
    for index in range(6):
        values = [r["comparisons"][index] for r in rows]
        by_trajectory = defaultdict(list)
        for row,value in zip(rows,values): by_trajectory[row["trajectory_index"]].append(value)
        result["pairs"].append(dict(from_model=values[0]["from"], to_model=values[0]["to"],
            mean_kl=statistics.mean(v["forward_kl"] for v in values),
            median_kl=statistics.median(v["forward_kl"] for v in values),
            mean_tv=statistics.mean(v["total_variation"] for v in values),
            top_action_disagreement=sum(v["top_action_changed"] for v in values)/len(values),
            trajectory_mean_kl=statistics.mean(statistics.mean(v["forward_kl"] for v in group) for group in by_trajectory.values()),
            trajectory_mean_tv=statistics.mean(statistics.mean(v["total_variation"] for v in group) for group in by_trajectory.values())))
    for index in range(4):
        values = [r["models"][index] for r in rows]
        result["models"].append(dict(label=values[0]["label"], mean_entropy=statistics.mean(v["entropy"] for v in values),
            mean_top_probability=statistics.mean(v["top_probability"] for v in values),
            fraction_top_probability_at_least_99pct=sum(v["top_probability"] >= .99 for v in values)/len(values)))
    return result


def analyze(root):
    p = validate_plan(root)
    result = read(root/"panel/result.json")
    folder = Path(result["output_directory"])
    completion = read(folder/"completion.json")
    assert completion["trajectories"] == len(p["panel"]) == 240 and completion["terminal_outcomes_used"] is False
    assert len(completion["models"]) == 4
    all_rows, coverage = [], []
    for i,item in enumerate(p["panel"]):
        path = folder/f"trajectory-{i:03}.json"
        assert pin(path)["sha256"] == result["fingerprints"][path.name]
        output = read(path)
        assert output["trajectory"] == item["trajectory"]
        for k in ["own", "opponent", "postboard"]: assert output[k] == item[k]
        assert output["learner_seat"] == item["seat"]
        archived = read(checked(item["trajectory"]))
        eligible = [j for j,r in enumerate(archived["decisions"]) if r["actor"] == item["seat"] and len(r["logits"]) > 1]
        chosen = eligible if len(eligible) <= 64 else [eligible[j*(len(eligible)-1)//63] for j in range(64)]
        assert [r["archive_row"] for r in output["rows"]] == chosen
        exposure = Counter()
        for row in output["rows"]:
            state = archived["auxiliary"][row["archive_row"]]["state"]
            assert len(state) == 6 and all(v in [0,1] for v in state)
            assert len(row["models"]) == 4 and len(row["comparisons"]) == 6
            for model in row["models"]:
                assert len(model["logits_bits"]) == row["action_count"] and 0 <= model["top_action"] < row["action_count"]
                assert math.isfinite(model["entropy"]) and 0 <= model["top_probability"] <= 1
            for pair in row["comparisons"]:
                assert math.isfinite(pair["forward_kl"]) and pair["forward_kl"] >= 0 and 0 <= pair["total_variation"] <= 1
            for name,value in zip(["W", "U", "B", "R", "G", "cannot_prevent"], state): exposure[name] += int(value)
            all_rows.append(dict(row, trajectory_index=i, training=item["training"], source_arm=item["source_arm"],
                own=item["own"], opponent=item["opponent"], postboard=item["postboard"], prevention_active=bool(any(state))))
        coverage.append(dict(item, eligible_choices=len(eligible), selected_choices=len(chosen), exposure=dict(exposure)))
    assert len(all_rows) == completion["choice_rows"]
    groups = {"all":all_rows}
    for active in [False, True]: groups[f"prevention_active/{active}"] = [r for r in all_rows if r["prevention_active"] == active]
    for field in ["training", "source_arm", "own", "postboard"]:
        for value in sorted({r[field] for r in all_rows}): groups[f"{field}/{value}"] = [r for r in all_rows if r[field] == value]
    analysis = dict(complete=True, trajectories=240, choice_rows=len(all_rows), coverage=coverage,
        summaries={label:summarize(rows) for label,rows in groups.items() if rows},
        limitations=p["limitations"], strength_claim=False, review=p["review"], panel_result=pin(root/"panel/result.json"))
    write(root/"analysis.json", analysis)
    print(dict(complete=True, trajectories=240, choice_rows=len(all_rows), exposed_rows=len(groups["prevention_active/True"])), flush=True)


if __name__ == "__main__":
    if not __debug__: raise RuntimeError("Diagnostic launch checks require Python assertions enabled")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["prepare", "qualify", "run", "analyze"])
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    globals()[args.mode](args.root.resolve())
