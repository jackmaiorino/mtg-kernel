"""Reuse the exact historical V3 failure with the explicit successor evaluator."""
import argparse
import copy
import json
from pathlib import Path
import shutil
import subprocess

from qualify_public_evaluation_v1 import read, pin, sha, write, execute, CAMPAIGN

OLD = CAMPAIGN / "spell-target-adapter-qualification-001"
PREVIOUS = CAMPAIGN / "public-evaluation-engineering-002"


def prepare(root):
    root.mkdir()
    request = read(CAMPAIGN / "v3-import-adapter-003/request.json")
    request["output_directory"] = str(root / "adapter")
    write(root / "transfer-request.json", request)
    write(root / "design.json", dict(question="Does the existing public unique-Spell authority repair retain its behavior in the successor evaluator?",
        matches=7, cases=["historical failed multi-choice match and replay", "historical valid control with repair on/off",
                          "learned successor versus repaired V3 in both seats and seat0 replay"],
        gates=dict(all_natural=True, historical_failure_repairs=1, valid_control_repairs=0,
                   gameplay_parity_with_historical_repaired_output=True, fresh_process_replay_bytes_exact=True,
                   total_seconds=360, per_process_seconds=180),
        non_claim="Engineering only. No retraining, outcome selection, promotion, paid compute or broad campaign. Fable quota gap remains."))


def selected_zones(item, policies):
    result = copy.deepcopy(item["registered"])
    for seat, policy in enumerate(policies):
        assert policy["kind"] == "static_plan_rows" and policy["carry_game_three_forward"]
        assert sha(policy["table"]["path"]) == policy["table"]["sha256"]
        row = next(r for r in read(policy["table"]["path"])["rows"]
                   if r["self_deck_id"] == result[seat]["label"] and r["opponent_deck_id"] == result[1-seat]["label"] and r["game_index"] == 2)
        for key, src, dst in [("cards_out", "mainboard", "sideboard"), ("cards_in", "sideboard", "mainboard")]:
            for card in row[key]:
                for _ in range(card["count"]):
                    result[seat][src].remove(card["card_id"])
                    result[seat][dst].append(card["card_id"])
        result[seat]["mainboard"].sort()
        result[seat]["sideboard"].sort()
    return result


def run(root, binary, transfer_binary):
    for src in [binary, transfer_binary]: shutil.copy2(src, root/src.name)
    binary, transfer_binary = root/binary.name, root/transfer_binary.name
    execute(root, "transfer", transfer_binary, root/"transfer-request.json")
    old = read(CAMPAIGN/"v3-import-adapter-003/candidate/play-import-source.json")
    new = read(root/"adapter/play-import-source.json")
    a,b=read(old["transfer_envelope"]["path"]),read(new["transfer_envelope"]["path"])
    old_build=a["receipt"]["destination_build_git_head"]
    a["receipt"]["destination_build_git_head"]=b["receipt"]["destination_build_git_head"]
    assert a==b
    for field in ["source_checkpoint","source_registry"]: assert old[field]["sha256"]==new[field]["sha256"]
    write(root/"import-comparison.json",dict(only_changed_field="receipt.destination_build_git_head",old_build=old_build,new_build=b["receipt"]["destination_build_git_head"],old=pin(old["transfer_envelope"]["path"]),new=pin(new["transfer_envelope"]["path"])))
    requests={}
    for label, original, enabled in [("failed", "failed-case", True), ("failed-replay", "failed-case", True),
                                     ("valid-on", "valid-control", True), ("valid-off", "valid-control", False)]:
        previous=read(OLD/f"configs/{original}.json")
        item=previous["matches"][0]
        sources=[]
        for source in previous["model_sources"]:
            source=copy.deepcopy(source)
            v3=source["feature_transfer"]["expected_feature_contract_digest"].startswith("9319")
            if v3:source["play_import"]=pin(root/"adapter/play-import-source.json")
            sources.append(dict(kind="legacy",source=source,v3_forced_actions=v3,v3_spell_target_reference_adapter=v3 and enabled))
        requests[label]=dict(sources=sources,matches=[dict(config=item["config"],registered=item["registered"],postboard=selected_zones(item,previous["policies"]))],
            cross_generation_evaluation=True,capture_decisions=True,output_directory=str(root/label))
    for label, seat in [("structured-s0",0),("structured-s1",1),("structured-replay",0)]:
        command=read(PREVIOUS/f"cross-s{seat}-request.json")
        command["output_directory"]=str(root/label)
        other=command["sources"][1-seat]
        other["source"]["play_import"]=pin(root/"adapter/play-import-source.json")
        other["v3_spell_target_reference_adapter"]=True
        requests[label]=command
    for label, command in requests.items():write(root/f"{label}-request.json",command)
    write(root/"manifest.json",dict(binary=pin(binary),transfer_binary=pin(transfer_binary),
        commit=subprocess.check_output(["git","rev-parse","HEAD"],text=True).strip(),
        requests={label:pin(root/f"{label}-request.json") for label in requests}, historical=pin(OLD/"manifest.json")))
    seconds=0
    for label in requests:
        seconds+=execute(root,label,binary,root/f"{label}-request.json")["seconds"]
        assert seconds<360,"fixed total engineering cost gate reached"
        print(label,round(seconds,2),flush=True)
    analyze(root)


def analyze(root):
    docs={}
    manifest=read(root/"manifest.json")
    for label,request in manifest["requests"].items():
        assert sha(request["path"])==request["sha256"]
        execution=read(root/f"{label}.execution.json")
        assert execution["exit_code"]==0 and not execution["timeout"]
        completion=read(root/label/"completion.json")
        assert completion["matches"]==1 and len(completion["match_sha256"])==1
        path=root/label/"match-000000.json"
        assert sha(path)==completion["match_sha256"][0]
        doc=docs[label]=read(path)
        assert completion["natural_games"]==len(doc["games"])==len(doc["seed_resets"])
        assert completion["decisions"]==doc["decision_count"]==len(doc["decisions"])
        assert all(0<=row["selected"]<row["legal_action_count"] for row in doc["decisions"])
    assert sum(docs["failed"]["v3_spell_target_repairs"])==1
    assert sum(docs["valid-on"]["v3_spell_target_repairs"])==sum(docs["valid-off"]["v3_spell_target_repairs"])==0
    for a,b in [("failed","failed-replay"),("structured-s0","structured-replay")]:
        assert (root/a/"match-000000.json").read_bytes()==(root/b/"match-000000.json").read_bytes()
    for key in ["games","outcome","seed_resets","decisions","decision_count"]:
        assert docs["valid-on"][key]==docs["valid-off"][key],key
    for label,original in [("failed","failed-case"),("valid-on","valid-control")]:
        old=read(OLD/f"outputs/{original}/match-000000.json")
        for key in ["games","outcome","sideboard_decisions"]:assert old[key]==docs[label][key],(label,key)
    result=dict(status="PUBLIC-EVALUATOR-SPELL-REPAIR-ENGINEERING-PASS",executed_matches=len(docs),
        natural_games=sum(len(d["games"]) for d in docs.values()),decisions=sum(d["decision_count"] for d in docs.values()),
        repaired_decisions=sum(sum(d["v3_spell_target_repairs"]) for d in docs.values()),
        historical_gameplay_exact=True,valid_on_off_decisions_exact=True,two_fresh_replays_bytes_exact=True,
        process_seconds=sum(read(root/f"{label}.execution.json")["seconds"] for label in docs),
        non_claim="Local engineering verification, not playing-strength or promotion evidence.")
    write(root/"qualification.json",result);print(json.dumps(result,indent=2))


def main():
    p=argparse.ArgumentParser();p.add_argument("mode",choices=["prepare","run","analyze"]);p.add_argument("--root",type=Path,required=True)
    p.add_argument("--binary",type=Path);p.add_argument("--transfer-binary",type=Path);a=p.parse_args()
    if a.mode=="prepare":prepare(a.root)
    elif a.mode=="analyze":analyze(a.root)
    else:run(a.root,a.binary,a.transfer_binary)


if __name__=="__main__":main()
