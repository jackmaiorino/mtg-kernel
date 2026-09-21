"""Replicate full-batch timing across storage, collectors and both PCs."""
import argparse
from pathlib import Path
import time
from public_training_dispatch_v2 import read,write,pin,checked,preflight,worker
from public_evaluation_dispatch_v1 import inventory
from public_training_storage_v1 import storage,dispatch,require_storage_choice,ARCHIVE
from qualify_state_prevention_compute_v1 import place,audit,reports


def idle_preflight(host, assignments):
    for attempt in range(5):
        try:
            return preflight(host, assignments)
        except ValueError as error:
            if str(error) != "selected GPU is unavailable; preserve its current work" or attempt == 4:
                raise
            print(host, "GPU idle preflight retry", attempt+1, flush=True)
            time.sleep(2)


def run(root,pilot):
    root.mkdir()
    m=read(pilot/"manifest.json"); binary=m["training_binary"]; configs=m["training_configs"]
    checks=read(pilot/"replication-check.json"); assert checks["complete"]
    hardware={host:inventory(host) for host in ["jack","haleyspc"]}
    assert all(not item["active"] for item in hardware.values())
    placements={}
    for host,devices in [("jack",[0,1]),("haleyspc",[0])]:
        snapshot=idle_preflight(host,[place(host,d,1) for d in devices])
        snapshot["hardware"]=hardware[host]
        path=root/f"{host}-inventory.json";write(path,snapshot)
        placements[host]=dict(checked_at=snapshot["at"],eligible=True,reason="Idle native/GPU resources under whole-PC assignment.",
            evidence=pin(path),devices=[dict(ordinal=d,uuid=place(host,d,1)["gpu_uuid"],eligible=True,reason="Available GPU.") for d in devices])
    cloud_path=Path("E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json")
    cloud=read(cloud_path)
    placements["runpod"]=dict(checked_at=cloud["checked_at"],eligible=False,evidence=pin(cloud_path),devices=[],
        reason="Latest read-only authenticated inventory HTTP403, no paid allocation.")
    stores={drive:storage(hardware["jack"],drive) for drive in ["C","D","E"]}
    cases=[]
    for drive,counts in [("C",[1,10]),("D",[1,4,10]),("E",[1,10])]:
        for n in counts:
            cases.append((f"local-{drive.lower()}-w{n}",stores[drive],dict(control=place("jack",1,n),structured=place("jack",0,n))))
    for n in [1,10]:
        cases.append((f"cross-d-w{n}",stores["D"],dict(control=place("jack",1,n),structured=place("haleyspc",0,n))))
    cases.append(("cross-fast-d-w10",stores["D"],dict(control=place("jack",0,10),structured=place("haleyspc",0,10))))
    write(root/"manifest.json",dict(pilot=pin(pilot/"manifest.json"),runner=pin(__file__),binary=binary,configs=configs,
        dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_training_storage_v1.py","compute_throughput_v2.py","public_training_dispatch_v2.py","qualify_state_prevention_compute_v1.py"]],
        cases=[dict(id=name,storage=s,placements=p) for name,s,p in cases],prefix_updates=3,
        native_case_games=60,maximum_executed_games=640,unique_arm_games=60,
        process_wall_cap_seconds=300,archive_scheme=ARCHIVE,
        non_claim="Engineering timing, exposure and exact replay only. No prefix outcome selection or full training launch."))
    started=time.monotonic(); candidates=[]; projections={}; reference={}; exposure={}; comparisons=0
    first_reports=None
    for label,store,assignment in cases:
        group_pin=dispatch(root/f"{root.name}-{label}",binary,configs,assignment,store,3)
        group=read(checked(group_pin)); times=[]
        current=reports(group_pin)
        if first_reports is None:first_reports=current
        for arm,report in current.items():
            fingerprint={name:pin(checked(item))["sha256"] for name,item in report["outputs"].items()}
            if arm in reference:
                assert reference[arm]==fingerprint,(label,arm)
                comparisons+=len(fingerprint)
            else:
                reference[arm]=fingerprint;exposure[arm]=audit(report,arm)
            completion=read(checked(report["completion"]));execution=read(checked(report["execution"]))
            if len(candidates)==0:assert execution["seconds"]<90,"cheap timing envelope exceeded"
            steady=sum(r["seconds"] for r in completion["receipts"][1:])/2
            times.append(execution["seconds"]+197*steady)
        projections[label]=max(times)+group["staging_seconds"]+group["recovery_seconds"]*200/3
        candidates.append(dict(id=label,benchmark=group_pin))
        print(label,"complete; archive-inclusive projected seconds",round(projections[label],2),flush=True)
    # Independently restart from update0, then compare updates1 and2 exactly.
    restart=root/"restart";restart.mkdir(); replay_comparisons=0
    for arm in ["control","structured"]:
        folder=restart/arm;folder.mkdir();p=place("jack",1,10)
        write(folder/"preflight.json",idle_preflight("jack",[p]))
        request=dict(config=read(checked(configs[arm])),output_directory=str(folder/"outputs"),
            resume=first_reports[arm]["outputs"]["0000/checkpoint.json"],stop_after=3,collector_workers=10,execution_gpu_ordinal=1)
        write(folder/"request.json",request)
        worker(dict(placement=p,wall_seconds=300,binary=binary,request=pin(folder/"request.json")))
        for name,digest in reference[arm].items():
            if name.startswith("0000/"):continue
            assert pin(folder/"outputs"/name)["sha256"]==digest
            replay_comparisons+=1
    choice=dict(schema="public-training-allocation/v2",jobs={arm:dict(config_sha256=item["sha256"],updates=200) for arm,item in configs.items()},
        inventory=placements,candidates=candidates,selected=min(projections,key=projections.get),archive_scheme=ARCHIVE,
        eligible_local_storage=[(s["drive"],s["disk_serial"]) for s in stores.values()],
        storage_dependencies=[pin(Path(__file__).with_name(name)) for name in ["public_training_storage_v1.py","public_evaluation_dispatch_v1.py"]])
    write(root/"compute-choice.json",choice)
    selected=require_storage_choice(root/"compute-choice.json",binary,configs)
    result=dict(complete=True,selected=selected,projections=projections,exposure=exposure,
        exact_allocation_file_comparisons=comparisons,exact_restart_file_comparisons=replay_comparisons,
        seconds=time.monotonic()-started,full_training_launched=False,archive_scheme=ARCHIVE,
        non_claim="Exact training/storage/recovery engineering only. No win-rate inference or promotion.")
    write(root/"qualification.json",result);print(result,flush=True)


if __name__=="__main__":
    p=argparse.ArgumentParser();p.add_argument("--root",type=Path,required=True);p.add_argument("--pilot",type=Path,required=True)
    a=p.parse_args();run(a.root,a.pilot)
