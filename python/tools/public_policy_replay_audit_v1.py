"""Bounded same-decision comparison; choose trajectories by schedule, not outcomes."""
import argparse
import json
import statistics
import time
from collections import defaultdict
from pathlib import Path

from public_feature_pilot_v1 import execute, pin, read, verify, write

PILOT = Path("E:/mtg-postboard-campaign-20260920/public-feature-pilot-001")
TOOLS = Path("E:/mtg-meta-recovery-20260920/public-policy-replay-tools-001")


def prepare(root):
    assert read(PILOT/"analysis.json")["complete"]
    build = read(TOOLS/"build-completion.json")
    assert build["exit_code"] == 0
    verify(build["binary"])
    root.mkdir()
    for name in ["requests", "outputs", "logs", "fixtures"]:
        (root/name).mkdir()
    original = read(PILOT/"manifest.json")
    config = read(verify(original["training_configs"]["control"]))
    selected = {}
    for update, batch in enumerate(config["updates"]):
        for index, episode in enumerate(batch):
            key = (episode["registered"][episode["learner_seat"]]["label"], episode["postboard"],
                   episode["learner_seat"], episode["opponent"]["checkpoint"]["sha256"])
            selected.setdefault(key, (update, index))
    assert len(selected) == 7*2*2*2
    panel = []
    for arm in ["control", "structured"]:
        arm_config = read(verify(original["training_configs"][arm]))
        assert arm_config["updates"] == config["updates"]
        for key, (update, index) in sorted(selected.items()):
            folder = PILOT/f"outputs/{arm+'-prefix' if update < 2 else arm}/{update:04}"
            trajectory = pin(folder/f"episode-{index:03}.json")
            assert trajectory["sha256"] == read(folder/"checkpoint.json")["trajectory_sha256"][index]
            panel.append(dict(source_arm=arm, own=key[0], postboard=key[1], seat=key[2],
                opponent_checkpoint=key[3], update=update, episode_index=index, trajectory=trajectory))
    parent = dict(kind="parent", label="g115", source=config["source"])
    models = [parent]+[dict(kind="checkpoint", label=arm, config=original["training_configs"][arm],
        checkpoint=read(PILOT/f"{arm}-training-audit.json")["checkpoint"]) for arm in ["control", "structured"]]
    qualification_models = [parent, dict(parent, label="g115-duplicate")]+[
        dict(kind="checkpoint",label=label,config=original["training_configs"]["control"],
             checkpoint=pin(PILOT/"outputs/control/0198/checkpoint.json"))
        for label in ["control-before-final", "control-before-final-duplicate"]]
    qualification_trajectories = [pin(PILOT/name) for name in [
        "outputs/control-prefix/0000/episode-000.json",
        "outputs/structured-prefix/0000/episode-000.json",
        "outputs/control/0199/episode-000.json"]]
    manifest = dict(schema="public-policy-replay-audit-panel/v1", runner=pin(__file__),
        dependency=pin(Path(__file__).with_name("public_feature_pilot_v1.py")), binary=build["binary"],
        original_manifest=pin(PILOT/"manifest.json"), models=models, panel=panel,
        qualification_models=qualification_models, qualification_trajectories=qualification_trajectories,
        selection="First scheduled episode in each own-deck/pre-postboard/seat/opponent-model stratum, both source arms; outcomes not read",
        max_choice_rows=64, workers_to_compare=[1,4], process_cap_seconds=120, total_cap_seconds=480,
        limitations="Development training states, primarily early continuation; no playing-strength or exploitability estimate. Equally spaced row subsample when >64 choices.")
    write(root/"manifest.json", manifest)


def run(root):
    m = read(root/"manifest.json")
    assert pin(__file__)["sha256"] == m["runner"]["sha256"]
    verify(m["dependency"])
    started = time.monotonic()

    def launch(label, models, trajectories, workers, minimum=0):
        remaining = m["total_cap_seconds"]-(time.monotonic()-started)
        assert remaining > 0
        request = root/f"requests/{label}.json"
        write(request, dict(models=models, trajectories=trajectories, workers=workers,
            output_directory=str(root/f"outputs/{label}"),
            max_choice_rows_per_trajectory=m["max_choice_rows"], minimum_behavior_replay_rows=minimum))
        return execute(root,label,m["binary"],request,min(remaining,m["process_cap_seconds"]))

    launch("behavior-replay",m["qualification_models"],m["qualification_trajectories"],1,2)
    for i in range(3):
        result = read(root/f"outputs/behavior-replay/trajectory-{i:03}.json")
        assert result["behavior_replay_rows"] == 2*len(result["rows"])
        for row in result["rows"]:
            for a,b in [(0,1),(2,3)]:
                assert row["models"][a]["logits_bits"] == row["models"][b]["logits_bits"]
                assert row["models"][a]["value_bits"] == row["models"][b]["value_bits"]
                pair = next(p for p in row["comparisons"] if p["from"] == row["models"][a]["label"] and p["to"] == row["models"][b]["label"])
                assert pair["forward_kl"] == pair["total_variation"] == 0 and not pair["top_action_changed"]

    original = read(verify(m["qualification_trajectories"][0]))
    corrupt = json.loads(json.dumps(original))
    row = next(r for r in corrupt["decisions"] if r["actor"] == corrupt["episode"]["learner_seat"] and len(r["logits"]) > 1)
    row["value"] ^= 1
    write(root/"fixtures/corrupt-behavior.json",corrupt)
    try:
        launch("corrupt-rejection",m["qualification_models"],[pin(root/"fixtures/corrupt-behavior.json")],1,2)
    except AssertionError:
        receipt = read(root/"logs/corrupt-rejection.execution.json")
        assert receipt["exit_code"] != 0 and not receipt["timeout"]
        assert "same-state policy replay differs" in (root/"logs/corrupt-rejection.stderr").read_text()
        assert not (root/"outputs/corrupt-rejection").exists()
    else:
        raise AssertionError("corrupted behavior was accepted")

    irrelevant = json.loads(json.dumps(original))
    for deck in irrelevant["episode"]["selected"]:
        deck["mainboard"].reverse()
    irrelevant["terminal"]["terminal_reward"] = [-v for v in irrelevant["terminal"]["terminal_reward"]]
    write(root/"fixtures/unused-metadata.json",irrelevant)
    launch("unused-metadata",m["qualification_models"],[pin(root/"fixtures/unused-metadata.json")],1,2)
    assert read(root/"outputs/unused-metadata/trajectory-000.json")["rows"] == read(root/"outputs/behavior-replay/trajectory-000.json")["rows"]

    # Same eight positions and three model loads in both timing candidates.
    mini = [m["panel"][i]["trajectory"] for i in range(0,112,14)]
    times = {}
    for workers in m["workers_to_compare"]:
        label = f"timing-w{workers}"
        times[workers] = launch(label,m["models"],mini,workers)["seconds"]
    for path in sorted((root/"outputs/timing-w1").glob("*.json")):
        assert pin(path)["sha256"] == pin(root/"outputs/timing-w4"/path.name)["sha256"]
    workers = min(times,key=times.get)
    write(root/"qualification.json",dict(behavior_scores_bit_exact=True, duplicate_model_kl_zero=True,
        corrupt_behavior_rejected=True, unused_archive_metadata_invariant=True,
        parallel_outputs_byte_identical=True, timing_seconds=times, selected_workers=workers,
        note="Bounded CPU diagnostic. This does not qualify remote placement or a training campaign."))
    launch("panel",m["models"],[r["trajectory"] for r in m["panel"]],workers)
    completion = read(root/"outputs/panel/completion.json")
    assert completion["trajectories"] == len(m["panel"]) == 112
    cells = defaultdict(list)
    for i, item in enumerate(m["panel"]):
        result = read(root/f"outputs/panel/trajectory-{i:03}.json")
        assert result["trajectory"] == item["trajectory"]
        for row in result["rows"]:
            for comparison in row["comparisons"]:
                cells[(item["source_arm"],item["own"],item["postboard"],comparison["from"],comparison["to"])].append(comparison)
    aggregates=[]
    for (source, own, postboard, a, b), rows in sorted(cells.items()):
        kl=sorted(r["forward_kl"] for r in rows)
        aggregates.append(dict(source_arm=source,own=own,postboard=postboard,from_model=a,to_model=b,
            choices=len(rows),top_action_changes=sum(r["top_action_changed"] for r in rows),
            mean_kl=statistics.mean(kl),median_kl=statistics.median(kl),
            p95_kl=kl[int(.95*(len(kl)-1))],mean_total_variation=statistics.mean(r["total_variation"] for r in rows)))
    write(root/"analysis.json",dict(complete=True,trajectories=112,choice_rows=completion["choice_rows"],
        aggregates=aggregates,wall_seconds=time.monotonic()-started,
        limitations=m["limitations"],strength_claim=False))
    print(json.dumps(dict(complete=True,trajectories=112,choice_rows=completion["choice_rows"],workers=workers)),flush=True)


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode",choices=["prepare","run"])
    parser.add_argument("--root",type=Path,required=True)
    args=parser.parse_args()
    (prepare if args.mode=="prepare" else run)(args.root.resolve())
