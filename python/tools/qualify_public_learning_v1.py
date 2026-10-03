"""Small real-game qualification for the public-input collector/update loop."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import time

REPO=Path(__file__).resolve().parents[2]


def read(path):return json.loads(Path(path).read_bytes())
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def write(path,data):
    with Path(path).open("x") as stream:json.dump(data,stream,indent=2,allow_nan=False)


def prepare(root):
    old=read("E:/mtg-postboard-campaign-20260920/qualification-001/preboard/config.json")
    gate=read("E:/mtg-postboard-campaign-20260920/prevention-correction-replay-001/request-seat0.json")
    source=old["initial_source"]
    opponent=old["opponents"][0]["source"]
    for model in [source,opponent]:
        for key in ["play_import","checkpoint"]:assert sha(model[key]["path"])==model[key]["sha256"]
    pairs=[gate["registrations"],old["iterations"][0]["episodes"][0]["episode"]["registered"],
        old["iterations"][2]["episodes"][1]["episode"]["registered"],gate["registrations"]]
    updates=[]
    for index,pair in enumerate(pairs):
        seed=int.from_bytes(hashlib.sha256(f"public-real-qualification/v1/{index}".encode()).digest()[:8],"little")
        episodes=[]
        for seat in [0,1]:
            decks=copy.deepcopy(pair if seat==0 else pair[::-1])
            episodes.append(dict(id=f"public-real-qualification-i{index}-s{seat}",seed=seed,starting_player=(seat+index)%2,learner_seat=seat,
                opponent=opponent,registered=decks,selected=decks,postboard=False,max_physical_decisions=40000,max_policy_steps=80000))
        updates.append(episodes)
    for label,enabled in [("control",False),("structured",True)]:
        config=dict(source=source,updates=updates,inputs_enabled=enabled,learning_rate=.0001,value_coefficient=.5,gamma=1.,lambda_=.9,gpu_ordinal=1,max_chunk_substeps=128)
        config["lambda"]=config.pop("lambda_")
        write(root/f"{label}-config.json",config)
        write(root/f"{label}-request.json",dict(config=config,output_directory=str(root/label),resume=None,stop_after=None))
    structured=read(root/"structured-config.json")
    write(root/"replay-first-request.json",dict(config=structured,output_directory=str(root/"replay-first"),resume=None,stop_after=1))
    write(root/"design.json",dict(question="Does real terminal-reward collection/update/resume work with the new public inputs and the zero-input control?",
        updates_per_arm=4,games_per_update=2,arms=["control","structured"],replay="Structured arm interrupted after one update, remaining three in a fresh process",
        gates=dict(all_games_natural=True,zero_unresolved_failures=True,first_arm_pair_same_trajectories_except_declared_identity=True,
            replay_checkpoint_optimizer_and_trajectory_bytes_exact=True,first_update_projection_limit_seconds=180,per_process_wall_seconds=600,
            formal_strength_claim=False),
        fixed="g115 full Adam, GAE gamma1/lambda.9, terminal reward, LR.0001, value.5, entropy0, fixed A48, matched physical seeds, same runtime and padding guard",
        non_claim="Eight games per arm are an engineering workload, not a win-rate comparison or promotion. No new paid compute. Fable quota gap remains."))


def execute(root,binary,label):
    request=root/f"{label}-request.json"
    if label=="replay-rest":
        checkpoint=root/"replay-first/0000/checkpoint.json"
        write(request,dict(config=read(root/"structured-config.json"),output_directory=str(root/label),resume=dict(path=str(checkpoint),sha256=sha(checkpoint)),stop_after=None))
    command=[str(binary),str(request)]
    started=time.monotonic()
    with (root/f"{label}.stdout").open("x") as stdout,(root/f"{label}.stderr").open("x") as stderr:
        child=subprocess.Popen(command,stdout=stdout,stderr=stderr,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
        timed_out=False
        try:code=child.wait(timeout=600)
        except subprocess.TimeoutExpired:
            timed_out=True;child.kill();code=child.wait()
    receipt=dict(command=command,binary_sha256=sha(binary),request_sha256=sha(request),exit_code=code,timeout=timed_out,seconds=time.monotonic()-started)
    write(root/f"{label}.execution.json",receipt)
    assert code==0 and not timed_out,receipt
    return receipt


def analyze(root):
    runs={label:read(root/label/"completion.json") for label in ["control","structured","replay-first","replay-rest"]}
    for label,run in runs.items():
        receipt=read(root/f"{label}.execution.json")
        assert receipt["exit_code"]==0 and not receipt["timeout"]
        for update in range(run["first_update"],run["next_update"]):
            folder=root/label/f"{update:04}"
            checkpoint=read(folder/"checkpoint.json")
            assert sha(folder/"optimizer.json")==checkpoint["optimizer_sha256"]
            for index,expected in enumerate(checkpoint["trajectory_sha256"]):
                assert sha(folder/f"episode-{index:03}.json")==expected
                trajectory=read(folder/f"episode-{index:03}.json")
                assert trajectory["terminal"]["terminal_classification"]=="natural"
                assert len(trajectory["decisions"])==len(trajectory["auxiliary"])
                assert trajectory["optimizer_state_sha256"]==run["receipts"][update-run["first_update"]]["before_state_sha256"]
            assert len(checkpoint["trajectory_sha256"])==2
            state=read(folder/"optimizer.json")
            assert state["legacy_adam_step"]==32401+update and state["public"]["adam_step"]==1+update
    for update in range(4):
        replay="replay-first" if update==0 else "replay-rest"
        for name in ["checkpoint.json","optimizer.json","episode-000.json","episode-001.json"]:
            assert (root/"structured"/f"{update:04}"/name).read_bytes()==(root/replay/f"{update:04}"/name).read_bytes(),(update,name)
    for index in range(2):
        a=read(root/f"control/0000/episode-{index:03}.json");b=read(root/f"structured/0000/episode-{index:03}.json")
        for document in [a,b]:
            document.pop("config_sha256");document.pop("inputs_enabled")
        assert a==b,"zero initial projections changed first collected game"
    control=read(root/"control/0003/optimizer.json")["public"]
    structured=read(root/"structured/0003/optimizer.json")["public"]
    for name in ["object","object_first","object_second","state","state_first","state_second"]:
        assert all(value in [0,1<<31] for value in control[name])
    assert any(value not in [0,1<<31] for value in structured["object"])
    result=dict(status="REAL-TERMINAL-REWARD-UPDATE-RESUME-ENGINEERING-PASS",unique_updates=8,unique_games=16,executed_updates=12,executed_games=24,
        replay_all_optimizer_checkpoint_and_trajectory_bytes_exact=True,initial_arm_trajectories_equal=True,
        structured_state_projection_changed=any(v not in [0,1<<31] for v in structured["state"]),
        final_structured_state_sha256=sha(root/"structured/0003/optimizer.json"),
        runs={label:dict(seconds=read(root/f"{label}.execution.json")["seconds"],groups=sum(r["learner_groups"] for r in run["receipts"])) for label,run in runs.items()},
        non_claim="No formal evaluation, win-rate inference, promotion or human-strength claim from this engineering workload.")
    write(root/"qualification.json",result);print(json.dumps(result,indent=2))


def main():
    parser=argparse.ArgumentParser();parser.add_argument("mode",choices=["prepare","run","analyze"]);parser.add_argument("--root",type=Path,required=True);parser.add_argument("--binary",type=Path)
    args=parser.parse_args()
    if args.mode=="prepare":prepare(args.root)
    elif args.mode=="analyze":analyze(args.root)
    else:
        execute(args.root,args.binary,"replay-first")
        first=read(args.root/"replay-first/0000/receipt.json")
        assert first["seconds"]*4<180,"cheap four-update timing projection exceeds qualification gate"
        write(args.root/"timing.json",dict(first_update_seconds=first["seconds"],projected_four_updates_seconds=first["seconds"]*4,limit_seconds=180))
        for label in ["control","structured","replay-rest"]:execute(args.root,args.binary,label)
        analyze(args.root)

if __name__=="__main__":main()
