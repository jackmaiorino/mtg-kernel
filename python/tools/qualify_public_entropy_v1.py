"""At most 80 natural engineering games; never launches a training campaign."""
import argparse
import copy
import os
from pathlib import Path
import subprocess
import time

from public_training_dispatch_v2 import read, write, pin, checked, preflight, worker
from public_evaluation_dispatch_v1 import inventory
from qualify_state_prevention_compute_v1 import place

BUILD = Path("E:/mtg-meta-recovery-20260921/public-entropy-tools-002/build-completion.json")
PILOT = Path("E:/mtg-postboard-campaign-20260920/state-prevention-pilot-001")


def run(root):
    build = read(BUILD)
    assert build["exit_code"] == 0
    binaries = {key:dict(path=value["path"],sha256=value["sha256"]) for key,value in build["binaries"].items()}
    for value in binaries.values(): checked(value)
    root.mkdir()
    cuda = Path("C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8")
    os.environ["CUDA_PATH"] = str(cuda)
    os.environ["PATH"] = str(cuda/"bin")+os.pathsep+os.environ["PATH"]
    (root/"temp").mkdir()
    os.environ["TEMP"] = os.environ["TMP"] = str(root/"temp")
    placement = place("desktop",1,10)
    current = preflight("desktop",[placement])
    m = read(PILOT/"manifest.json")
    training = read(PILOT/"training-audit.json")
    reports = {arm:read(checked(training["arms"][arm]["report"])) for arm in ["control","structured"]}
    configs = {arm:read(checked(m["training_configs"][arm])) for arm in reports}
    assert all(len(v["updates"]) == 200 and all(len(batch)==10 for batch in v["updates"]) for v in configs.values())
    plan = dict(schema="public-entropy-engineering/v1",runner=pin(__file__),build=pin(BUILD),binaries=binaries,
        pilot=pin(PILOT/"manifest.json"),training=pin(PILOT/"training-audit.json"),maximum_natural_games=80,
        beta_for_engineering_only=.05,first_updates=[0,1],late_replay_update=199,workers_to_compare=[1,10],
        gpu=placement,inventory=current,process_cap_seconds=180,
        runtime={name:pin(cuda/"bin"/name) for name in ["nvrtc64_120_0.dll","nvrtc-builtins64_128.dll"]},
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["public_training_dispatch_v2.py","public_evaluation_dispatch_v1.py","qualify_state_prevention_compute_v1.py"]],
        question="Verify actual entropy gradients, zero-path compatibility, nonzero optimizer effects, collector parity and fresh-process continuation. No win-rate selection.",
        review="Known Fable zero-read HTTP429 through Sep22 07:00EDT; no retry/endorsement. Bounded implementation under the maintainer's research assignment.")
    write(root/"manifest.json",plan)

    def bounded_command(label,binary,request=None,expected_error=None):
        folder=root/label
        folder.mkdir()
        cmd=[str(checked(binary))]
        if request is not None:
            write(folder/"request.json",request)
            cmd.append(str(folder/"request.json"))
        before=time.monotonic()
        with (folder/"stdout").open("x") as out,(folder/"stderr").open("x") as err:
            child=subprocess.Popen(cmd,stdout=out,stderr=err,creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS|subprocess.CREATE_NO_WINDOW)
            write(folder/"started.json",dict(pid=child.pid,started_unix=time.time(),command=cmd))
            timeout=False
            try: code=child.wait(timeout=180)
            except subprocess.TimeoutExpired:
                timeout=True;child.kill();code=child.wait()
        write(folder/"execution.json",dict(exit_code=code,timeout=timeout,seconds=time.monotonic()-before,binary=binary))
        assert not timeout
        if expected_error:
            assert code!=0 and expected_error in (folder/"stderr").read_text()
        else: assert code==0,(folder,(folder/"stderr").read_text())
        return folder

    probe_folder=bounded_command("gradient-probe",binaries["public_entropy_gradient_probe_v1"])
    probe=read(probe_folder/"stdout")
    assert probe["complete"] and len(probe["cases"])==12 and probe["zero_loss_gradient_bit_exact"]
    write(root/"gradient-probe.json",probe)
    print(dict(gradient_probe="PASS",cases=12),flush=True)
    executions, games = {},0
    def launch(label,config,resume,stop,workers):
        nonlocal games
        first=read(checked(resume))["next_update"] if resume else 0
        scheduled=sum(len(b) for b in config["updates"][first:stop])
        assert games+scheduled<=80 and scheduled<=20
        p=place("desktop",1,workers)
        folder=root/label
        folder.mkdir()
        write(folder/"preflight.json",preflight("desktop",[p]))
        request=dict(config=config,output_directory=str(folder/"outputs"),resume=resume,stop_after=stop,
            collector_workers=workers,execution_gpu_ordinal=1)
        write(folder/"request.json",request)
        execution=worker(dict(placement=p,wall_seconds=180,binary=binaries["public_feature_training_v1"],request=pin(folder/"request.json")))
        completion=read(folder/"outputs/completion.json")
        assert completion["first_update"]==first and completion["next_update"]==stop
        assert len(completion["receipts"])==stop-first
        assert sum(r["natural_games"] for r in completion["receipts"])==scheduled
        for index in range(first,stop):
            cp=read(folder/f"outputs/{index:04}/checkpoint.json")
            assert cp["optimizer_sha256"]==pin(folder/f"outputs/{index:04}/optimizer.json")["sha256"]
            for episode,digest in enumerate(cp["trajectory_sha256"]):
                path=folder/f"outputs/{index:04}/episode-{episode:03}.json"
                assert pin(path)["sha256"]==digest and read(path)["terminal"]["terminal_classification"]=="natural"
        games+=scheduled
        executions[label]=dict(execution=pin(folder/"execution.json"),games=scheduled,seconds=execution["seconds"])
        print(dict(completed=label,natural_games=scheduled,seconds=execution["seconds"]),flush=True)
        return folder/"outputs"

    exact=[]
    for label,arm,explicit in [("zero-control-omitted","control",False),("zero-control-explicit","control",True),("zero-structured-omitted","structured",False)]:
        config=copy.deepcopy(configs[arm])
        if explicit: config["entropy_coefficient"]=0.0
        report=reports[arm]
        output=launch(label,config,report["outputs"]["0198/checkpoint.json"],200,10)
        compared=0
        for name,old in report["outputs"].items():
            if name.startswith("0199/"):
                checked(old)
                new=pin(output/name)
                assert new["sha256"]==old["sha256"],(label,name)
                exact.append(dict(original=old,replay=new));compared+=1
        assert compared==12
    nonzero=copy.deepcopy(configs["control"])
    nonzero["entropy_coefficient"]=.05
    write(root/"entropy-config.json",nonzero)
    serial=launch("entropy-w1",nonzero,None,2,1)
    parallel=launch("entropy-w10",nonzero,None,2,10)
    for path in sorted(serial.rglob("*.json")):
        if path.name in ["completion.json","execution-receipt.json"] or ".execution" in path.name: continue
        # Scientific trajectory/checkpoint/state bytes only; receipt times differ.
        if path.parent.name in ["0000","0001"] and (path.name in ["checkpoint.json","optimizer.json"] or path.name.startswith("episode-")):
            assert pin(path)["sha256"]==pin(parallel/path.relative_to(serial))["sha256"]
    resumed=launch("entropy-resumed",nonzero,pin(parallel/"0000/checkpoint.json"),2,10)
    for path in (parallel/"0001").glob("*.json"):
        if path.name in ["checkpoint.json","optimizer.json"] or path.name.startswith("episode-"):
            assert pin(path)["sha256"]==pin(resumed/"0001"/path.name)["sha256"]
    assert games==80
    # First collection precedes the changed loss: identical behavior/environment,
    # with only the declared scientific-config identity changing.
    for episode in range(10):
        old=read(checked(reports["control"]["outputs"][f"0000/episode-{episode:03}.json"]))
        new=read(parallel/f"0000/episode-{episode:03}.json")
        assert old.pop("config_sha256")!=new.pop("config_sha256")
        assert old==new,"entropy altered collection before its first update"
    before=read(checked(reports["control"]["outputs"]["0000/optimizer.json"]))
    after=read(parallel/"0000/optimizer.json")
    changes={field:sum(x!=y for a,b in zip(before[field],after[field]) for x,y in zip(a["values"],b["values"])) for field in ["parameters","first_moments","second_moments"]}
    assert all(changes.values()),"nonzero entropy did not change actual learner/optimizer"
    for index in [0,1]:
        state=read(parallel/f"{index:04}/optimizer.json")
        assert state["legacy_adam_step"]==32401+index and state["public"]["adam_step"]==index+1
        for field,values in state["public"].items():
            if field!="adam_step": assert set(values)<={0,1<<31}
        assert state["scorer_bias_anchor_bits"]==before["scorer_bias_anchor_bits"]
        for field in ["parameters","first_moments","second_moments"]:
            embedding=next(v for v in state[field] if v["name"]=="card_embedding.weight")
            assert set(embedding["values"][:embedding["shape"][1]])=={0}
    model=dict(kind="checkpoint",label="entropy-before-second",config=pin(root/"entropy-config.json"),checkpoint=pin(parallel/"0000/checkpoint.json"))
    cpu_request=dict(models=[model,dict(model,label="duplicate")],trajectories=[pin(parallel/"0001/episode-000.json")],
        output_directory=str(root/"inference-replay/outputs"),workers=1,max_choice_rows_per_trajectory=8,minimum_behavior_replay_rows=2)
    bounded_command("inference-replay",binaries["public_policy_replay_audit_v1"],cpu_request)
    loaded=read(root/"inference-replay/outputs/completion.json")
    assert loaded["exact_behavior_replay_rows"]==2*loaded["choice_rows"] and loaded["choice_rows"]>0
    for label,value in [("negative-beta",-.1),("large-beta",1.01),("changed-resume-beta",.1)]:
        config=copy.deepcopy(nonzero);config["entropy_coefficient"]=value
        request=dict(config=config,output_directory=str(root/label/"outputs"),resume=pin(parallel/"0000/checkpoint.json"),
            stop_after=2,collector_workers=10,execution_gpu_ordinal=1)
        expected="public resume identity differs" if label=="changed-resume-beta" else "public entropy coefficient must be finite"
        bounded_command(label,binaries["public_feature_training_v1"],request,expected)
        assert not (root/label/"outputs").exists()
    write(root/"result.json",dict(complete=True,natural_games=games,exact_legacy_files=len(exact),legacy_comparisons=exact,
        gradient_probe=pin(root/"gradient-probe.json"),executions=executions,nonzero_parameter_moment_changes=changes,
        collector_outputs_bit_exact=True,fresh_process_resume_bit_exact=True,initial_collection_exact=True,
        coefficient_bound_to_config=True,inference_behavior_replay_rows=loaded["exact_behavior_replay_rows"],
        coefficient=.05,coefficient_is_engineering_only=True,
        non_claim="Loss/update/continuation correctness on fixed games. No strength result, coefficient selection or full host allocation qualification."))
    print(dict(complete=True,natural_games=games,exact_legacy_files=len(exact),nonzero_changes=changes),flush=True)


if __name__=="__main__":
    if not __debug__: raise RuntimeError("Qualification requires assertions enabled")
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root",type=Path,required=True)
    run(parser.parse_args().root.resolve())
