"""Bounded CPU BO3 qualification for explicit public-input checkpoints."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time

TRAIN = Path("E:/mtg-postboard-campaign-20260920/public-learning-engineering-001")
CAMPAIGN = TRAIN.parent


def read(p): return json.loads(Path(p).read_bytes())
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def pin(p): return dict(path=str(p), sha256=sha(p))
def write(p, value):
    with Path(p).open("x") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def prepare(root):
    root.mkdir()
    transfer = read(CAMPAIGN / "v3-import-adapter-002/request.json")
    transfer["output_directory"] = str(root / "adapter")
    write(root / "transfer-request.json", transfer)
    original = read(CAMPAIGN / "qualification-001/mixed/config.json")
    # First non-mirror fixed postboard schedule entry, chosen without outcomes.
    episode = next(e["episode"] for update in original["iterations"] for e in update["episodes"]
                   if e["episode"]["postboard"] and e["episode"]["registered"][0]["label"] != e["episode"]["registered"][1]["label"])
    write(root / "postboard-fixture.json", episode)
    write(root / "design.json", dict(question="Can actual learned public-input checkpoints play and replay BO3 through the existing engine, including fixed sideboarding and V3 opposition?",
        cases="Both physical seats; unchanged g115 versus zero-projection warm start; structured/control four-update checkpoints; independent V3; structured full replay",
        intended_matches=12, training=False, gates=dict(all_matches_complete=True, all_games_natural=True,
        initial_reference_warm_gameplay_exact=True, learned_fresh_process_replay_bytes_exact=True,
        postboard_changes_exercised=True, bad_config_and_bad_checkpoint_pins_rejected=True,
        per_process_seconds=180, total_evaluation_seconds=360),
        limitations="Debug CPU engineering timing only. Fixed KeepSevenV2 and static sideboards. Familiar A48 plus one V3. No win-rate, promotion or human-strength inference. Fable quota failure unresolved; no new paid compute."))


def execute(root, label, binary, request, expect_success=True):
    started = time.monotonic()
    with (root / f"{label}.stdout").open("x") as out, (root / f"{label}.stderr").open("x") as err:
        process = subprocess.Popen([str(binary), str(request)], stdout=out, stderr=err,
            creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
        timeout = False
        try: code = process.wait(timeout=180)
        except subprocess.TimeoutExpired:
            timeout = True
            process.kill()
            code = process.wait()
    result = dict(command=[str(binary), str(request)], seconds=time.monotonic()-started,
                  exit_code=code, timeout=timeout, binary=pin(binary), request=pin(request))
    write(root / f"{label}.execution.json", result)
    assert not timeout and (code == 0 if expect_success else code != 0), result
    return result


def run(root, binary, transfer_binary):
    for source in [binary, transfer_binary]:
        shutil.copy2(source, root / source.name)
    binary = root / binary.name
    transfer_binary = root / transfer_binary.name
    execute(root, "transfer", transfer_binary, root / "transfer-request.json")
    old = read(CAMPAIGN / "v3-import-adapter-002/candidate/play-import-source.json")
    new = read(root / "adapter/play-import-source.json")
    old_envelope = read(old["transfer_envelope"]["path"])
    new_envelope = read(new["transfer_envelope"]["path"])
    old_build = old_envelope["receipt"]["destination_build_git_head"]
    old_envelope["receipt"]["destination_build_git_head"] = new_envelope["receipt"]["destination_build_git_head"]
    assert old_envelope == new_envelope, "V3 rebind changed parameters or optimizer"
    for key in ["source_checkpoint", "source_registry"]:
        assert old[key]["sha256"] == new[key]["sha256"]
    write(root / "import-comparison.json", dict(only_changed_field="receipt.destination_build_git_head", old_build=old_build,
          new_build=new_envelope["receipt"]["destination_build_git_head"], old=pin(old["transfer_envelope"]["path"]), new=pin(new["transfer_envelope"]["path"])))
    v3 = copy.deepcopy(read(CAMPAIGN / "bo3-qualification-003/manifest.json")["opponent_source"])
    v3["play_import"] = pin(root / "adapter/play-import-source.json")
    training = read(TRAIN / "structured-config.json")
    initial, opponent = training["source"], training["updates"][0][0]["opponent"]
    legacy = lambda source, forced=False: dict(kind="legacy", source=source, v3_forced_actions=forced)
    checkpoint = lambda arm: dict(kind="public_checkpoint", config=pin(TRAIN / arm / "config.json"), checkpoint=pin(TRAIN / arm / "0003/checkpoint.json"))
    episode = read(root / "postboard-fixture.json")
    requests = []
    for group in ["reference", "warm", "structured", "control", "cross", "replay"]:
        for seat in [0, 1]:
            baseline = read(CAMPAIGN / f"prevention-correction-replay-001/request-seat{seat}.json")
            config = baseline["config"]
            if group in ["reference", "warm"]:
                registered, postboard = baseline["registrations"], None
            else:
                registered = copy.deepcopy(episode["registered"] if seat == 0 else episode["registered"][::-1])
                postboard = copy.deepcopy(episode["selected"] if seat == 0 else episode["selected"][::-1])
                config["deck_ids"] = [d["label"] for d in registered]
                config["seed"] = int.from_bytes(hashlib.sha256(b"public-evaluation-engineering/v1/postboard").digest()[:8], "little")
                config["game_one_chooser"] = seat
            candidate = (legacy(initial) if group == "reference" else dict(kind="public_warm_start", source=initial) if group == "warm"
                         else checkpoint("control" if group == "control" else "structured"))
            other = legacy(v3, True) if group == "cross" else legacy(opponent)
            label = f"{group}-s{seat}"
            command = dict(sources=[candidate, other] if seat == 0 else [other, candidate],
                matches=[dict(config=config, registered=registered, postboard=postboard)], cross_generation_evaluation=group == "cross",
                capture_decisions=True, output_directory=str(root / label))
            write(root / f"{label}-request.json", command)
            requests.append((label, root / f"{label}-request.json"))
    write(root / "manifest.json", dict(binary=pin(binary), transfer_binary=pin(transfer_binary),
        source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        requests={label:pin(request) for label,request in requests}, training_qualification=pin(TRAIN/"qualification.json")))
    seconds = 0
    for label,request in requests:
        seconds += execute(root, label, binary, request)["seconds"]
        assert seconds < 360, "frozen total engineering cost limit reached"
        print(label, round(seconds, 2), flush=True)
    valid = read(root / "structured-s0-request.json")
    for label in ["bad-config", "bad-pin"]:
        command = copy.deepcopy(valid)
        command["output_directory"] = str(root/label)
        if label == "bad-config": command["sources"][0]["config"] = pin(TRAIN/"control/config.json")
        else: command["sources"][0]["checkpoint"]["sha256"] = "0"*64
        request = root / f"{label}-request.json"
        write(request, command)
        execute(root, label, binary, request, expect_success=False)
        assert not (root/label).exists(), "negative model input reached gameplay"
        stderr = (root/f"{label}.stderr").read_text()
        assert ("checkpoint/config identity differs" if label == "bad-config" else "SHA") in stderr, stderr
    analyze(root)


def analyze(root):
    manifest=read(root/"manifest.json")
    groups={}
    for label,request_pin in manifest["requests"].items():
        assert sha(request_pin["path"])==request_pin["sha256"]
        execution=read(root/f"{label}.execution.json")
        assert execution["exit_code"]==0 and not execution["timeout"]
        completion=read(root/label/"completion.json")
        assert completion["matches"]==1 and len(completion["match_sha256"])==1
        path=root/label/"match-000000.json"
        assert sha(path)==completion["match_sha256"][0]
        match=read(path)
        assert completion["natural_games"]==len(match["games"])==len(match["seed_resets"])
        assert completion["decisions"]==match["decision_count"]==len(match["decisions"])
        assert all(0<=r["selected"]<r["legal_action_count"] for r in match["decisions"])
        groups[label]=match
    for seat in [0,1]:
        a,b=groups[f"reference-s{seat}"],groups[f"warm-s{seat}"]
        for key in ["games","outcome","seed_resets","decisions","decision_count"]:assert a[key]==b[key],(seat,key)
        a=root/f"structured-s{seat}/match-000000.json"; b=root/f"replay-s{seat}/match-000000.json"
        assert a.read_bytes()==b.read_bytes(),seat
        for label in ["structured","control","cross"]:
            match=groups[f"{label}-s{seat}"]
            assert match["models"][seat]["public_adam_step"]==4
            assert match["models"][seat]["inputs_enabled"]==(label!="control")
            assert any(len(d["selected_actions"])>1 for d in match["sideboard_decisions"]),"postboard change was not exercised"
        assert groups[f"cross-s{seat}"]["v3_forced_actions"][1-seat]>0
    result=dict(status="PUBLIC-CHECKPOINT-BO3-ENGINEERING-PASS",executed_matches=len(groups),unique_arm_cases=len(groups)-2,
        executed_natural_games=sum(len(m["games"]) for m in groups.values()),executed_decisions=sum(m["decision_count"] for m in groups.values()),
        reference_zero_projection_gameplay_exact=True, learned_full_match_replay_bytes_exact=True, fixed_postboard_exercised=True,
        v3_forced_actions=sum(sum(m["v3_forced_actions"]) for m in groups.values()),
        evaluation_process_seconds=sum(read(root/f"{label}.execution.json")["seconds"] for label in groups),
        non_claim="No win-rate interpretation or promotion. Engineering endpoints, two fixed matchup fixtures, one familiar A48 and one independent V3, fixed sideboards/KeepSevenV2. Debug cost only.")
    write(root/"qualification.json",result)
    print(json.dumps(result,indent=2))


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("mode",choices=["prepare","run","analyze"])
    parser.add_argument("--root",type=Path,required=True)
    parser.add_argument("--binary",type=Path)
    parser.add_argument("--transfer-binary",type=Path)
    args=parser.parse_args()
    if args.mode=="prepare":prepare(args.root)
    elif args.mode=="run":run(args.root,args.binary,args.transfer_binary)
    else:analyze(args.root)


if __name__=="__main__":main()
