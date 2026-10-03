"""Small fixed-input correctness ablation; no new trajectories or learning."""
import argparse
import copy
import math
from pathlib import Path
import statistics
import struct

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, prepare_remote
from public_replay_dispatch_v1 import dispatch


def run(root, source):
    source_plan = read(source/"plan.json")
    source_analysis = read(source/"analysis.json")
    assert source_analysis["complete"] and source_analysis["trajectories"] == 240
    source_result = read(source/"panel/result.json")
    source_outputs = Path(source_result["output_directory"])
    root.mkdir()
    fixtures = root/"fixtures"
    fixtures.mkdir()
    assets, trajectories, mapping = {}, [], []
    def add(item):
        path = checked(item)
        assets[str(path)] = item
        return path
    for model in source_plan["models"]:
        add(model["config"])
        checkpoint_path = add(model["checkpoint"])
        checkpoint = read(checkpoint_path)
        add(dict(path=str(checkpoint_path.parent/checkpoint["optimizer_file"]), sha256=checkpoint["optimizer_sha256"]))
        cfg = read(model["config"]["path"])
        for v in cfg["source"].values():
            if isinstance(v,dict) and "path" in v and "sha256" in v:
                path = add(v)
                if path.suffix == ".json":
                    data = read(path)
                    for nested in data.values() if isinstance(data,dict) else []:
                        if isinstance(nested,dict) and "path" in nested and "sha256" in nested: add(nested)
    row_count = 0
    for original_index,item in enumerate(source_plan["panel"]):
        old_path = source_outputs/f"trajectory-{original_index:03}.json"
        assert pin(old_path)["sha256"] == source_result["fingerprints"][old_path.name]
        scored = read(old_path)
        original = read(checked(item["trajectory"]))
        # Public rows are learner-only in this schema; other opponent schemas also fill opponent rows.
        assert original["schema"] == "mtg-kernel-public-input-trajectory/v1"
        chosen = [r for r in scored["rows"] if any(original["auxiliary"][r["archive_row"]]["state"])]
        if not chosen: continue
        assert len(chosen) <= 64
        fixture = dict(original, decisions=[original["decisions"][r["archive_row"]] for r in chosen],
            auxiliary=[original["auxiliary"][r["archive_row"]] for r in chosen])
        start = len(trajectories)
        for mode in ["visible", "zero-state"]:
            data = copy.deepcopy(fixture)
            if mode == "zero-state":
                for row in data["auxiliary"]: row["state"] = [0.0]*6
            path = fixtures/f"source-{original_index:03}-{mode}.json"
            write(path,data)
            item_pin = pin(path)
            add(item_pin)
            trajectories.append(item_pin)
        mapping.append(dict(original_index=original_index, original_trajectory=item["trajectory"],
            original_scored=pin(old_path), fixture_indices=[start,start+1], archive_rows=[r["archive_row"] for r in chosen]))
        row_count += len(chosen)
    assert row_count == source_analysis["summaries"]["prevention_active/True"]["rows"] == 95
    assert len(mapping) == 27 and len(trajectories) == 54
    plan = dict(schema="prevention-feature-mask-correctness/v1", runner=pin(__file__),
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_replay_dispatch_v1.py", "public_evaluation_dispatch_v1.py"]],
        source_plan=pin(source/"plan.json"), source_analysis=pin(source/"analysis.json"), source_result=pin(source/"panel/result.json"),
        binary=source_plan["binary"], models=source_plan["models"], trajectories=trajectories, mapping=mapping,
        unique_decisions=95, scored_input_variants=190, workers_to_compare=[1,4],
        question="Change only the six public prevention bits on previously sampled exposed rows; same tensors, legal menu and fixed checkpoints. Does the direct projection change scores/top action?",
        limits="Synthetic input ablation, not natural engine states or new matches. 95 exposed decisions on 27 late development trajectories; no white/cannot-prevent exposure, one blue. No strength or causal-win claim.",
        review=source_plan["review"], no_new_training=True)
    write(root/"plan.json",plan)
    hosts=[]
    for host in ["jack", "haleyspc"]:
        current=inventory(host)
        write(root/f"{host}-inventory.json",current)
        if not current["active"]: hosts.append(host)
    assert hosts
    # This is a 190-input correctness check, not a substantive rollout campaign.
    # Reuse the scorer's existing qualified host; compare serial/parallel outputs here too.
    prior_choice=read(source/"qualification.json")["selected"]
    host=prior_choice["host"]
    assert host in hosts
    remote=prepare_remote(root,list(assets.values()))
    baseline=None
    measurements=[]
    for workers in plan["workers_to_compare"]:
        result=dispatch(root,f"check-w{workers}",plan["binary"],dict(models=plan["models"],trajectories=trajectories,
            workers=workers,max_choice_rows_per_trajectory=64,minimum_behavior_replay_rows=0),host,remote)
        assert baseline is None or baseline==result["fingerprints"]
        baseline=result["fingerprints"]
        measurements.append(dict(workers=workers,seconds=result["seconds"],report=pin(root/f"check-w{workers}/result.json")))
    output=Path(result["output_directory"])
    per_model={m["label"]:[] for m in plan["models"]}
    def f32(bits): return struct.unpack('<f',struct.pack('<I',bits))[0]
    for item in mapping:
        old=read(checked(item["original_scored"]))
        old_rows={r["archive_row"]:r for r in old["rows"]}
        a,b=[read(output/f"trajectory-{i:03}.json")["rows"] for i in item["fixture_indices"]]
        assert len(a)==len(b)==len(item["archive_rows"])
        for index,(visible,zero) in enumerate(zip(a,b)):
            original_row=old_rows[item["archive_rows"][index]]
            assert visible["models"]==original_row["models"] and visible["comparisons"]==original_row["comparisons"]
            assert visible["tensor_sha256"]==zero["tensor_sha256"]==original_row["tensor_sha256"]
            for left,right in zip(visible["models"],zero["models"]):
                assert left["label"]==right["label"]
                if left["label"].endswith("control"): assert left==right, "disabled-input control changed under masking"
                values=[abs(f32(x)-f32(y)) for x,y in zip(left["logits_bits"],right["logits_bits"])]
                assert all(math.isfinite(v) for v in values)
                per_model[left["label"]].append(dict(original_trajectory=item["original_index"],archive_row=item["archive_rows"][index],
                    logits_changed=left["logits_bits"]!=right["logits_bits"],value_changed=left["value_bits"]!=right["value_bits"],
                    top_action_changed=left["top_action"]!=right["top_action"],max_abs_logit_change=max(values),
                    top_probability_change=left["top_probability"]-right["top_probability"],
                    entropy_change=left["entropy"]-right["entropy"],value_change=f32(left["value_bits"])-f32(right["value_bits"])))
    summary={}
    for label,rows in per_model.items():
        assert len(rows)==95
        summary[label]=dict(decisions=95,logits_changed=sum(v["logits_changed"] for v in rows),
            values_changed=sum(v["value_changed"] for v in rows),top_actions_changed=sum(v["top_action_changed"] for v in rows),
            mean_max_abs_logit_change=statistics.mean(v["max_abs_logit_change"] for v in rows),
            max_abs_logit_change=max(v["max_abs_logit_change"] for v in rows),
            mean_top_probability_change=statistics.mean(v["top_probability_change"] for v in rows),
            max_abs_top_probability_change=max(abs(v["top_probability_change"]) for v in rows),
            mean_entropy_change=statistics.mean(v["entropy_change"] for v in rows))
    write(root/"analysis.json",dict(complete=True,plan=pin(root/"plan.json"),exact_visible_replay=True,
        exact_control_mask_invariance=True,parallel_outputs_exact=True,models=summary,rows=per_model,
        measurements=measurements,remote_setup_seconds=remote["seconds"],limits=plan["limits"],strength_claim=False))
    print(summary,flush=True)


if __name__=="__main__":
    if not __debug__: raise RuntimeError("Correctness checks require assertions")
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--source",type=Path,required=True)
    args=parser.parse_args()
    run(args.root.resolve(),args.source.resolve())
