"""Bounded full-update timing on two PCs; never launches full training."""
import argparse,copy,hashlib,os,time
from pathlib import Path
from public_training_dispatch_v2 import read,write,pin,checked,preflight
from public_training_storage_v1 import storage,dispatch,require_storage_choice,ARCHIVE
from public_evaluation_dispatch_v1 import inventory
from qualify_state_prevention_compute_v1 import place,reports


def audit(report,mode):
    completion=read(checked(report['completion']))
    assert completion['first_update']==0 and completion['next_update']==3
    for i,row in enumerate(completion['receipts']):
        assert row['update']==i and row['episodes']==row['natural_games']==10
        saved=read(checked(report['outputs'][f'{i:04}/optimizer.json']))
        checkpoint=read(checked(report['outputs'][f'{i:04}/checkpoint.json']))
        assert saved['legacy_adam_step']==32401+i and saved['stack']['adam_step']==i+1
        assert checkpoint['optimizer_sha256']==report['outputs'][f'{i:04}/optimizer.json']['sha256']
        if mode=='disabled':
            assert all(v in [0,1<<31] for key in ['weight','first','second'] for v in saved['stack'][key])
        for j in range(10):
            item=report['outputs'][f'{i:04}/episode-{j:03}.json'];t=read(checked(item))
            assert t['terminal']['terminal_classification']=='natural' and t['input_mode']==mode
            assert t['optimizer_state_sha256']==row['before_state_sha256']
            assert checkpoint['trajectory_sha256'][j]==item['sha256']
    return dict(natural_games=30,complete_updates=3,checkpoint_links_verified=True)


def run(root):
    repo=Path(__file__).resolve().parents[2]
    root.mkdir();(root/'configs').mkdir();(root/'temp').mkdir()
    cuda=Path('C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8')
    os.environ['CUDA_PATH']=str(cuda);os.environ['PATH']=str(cuda/'bin')+os.pathsep+os.environ['PATH']
    os.environ['TEMP']=os.environ['TMP']=str(root/'temp')
    build=Path('E:/mtg-meta-recovery-20260921/public-stack-trainer-tools-001/completion.json')
    b=read(build);assert b['complete'];binary=dict(path=b['binary'],sha256=b['binary_sha256']);checked(binary)
    source=Path('E:/mtg-postboard-campaign-20260921/public-balanced-schedule-pilot-001/replica-1/configs/balanced.json')
    config=read(source);assert len(config['updates'])==200 and all(len(x)==10 for x in config['updates'])
    for key in ['inputs_enabled','projection_mode','entropy_coefficient']:config.pop(key,None)
    config.update(schema='mtg-kernel-stack-training-config/v1',stack_contract_sha256=pin(repo/'data/public_stack_features_v1/contract.json')['sha256'],permutation_contract_sha256=pin(repo/'data/public_stack_features_v1/permutation.json')['sha256'])
    for i,batch in enumerate(config['updates']):
        for j,episode in enumerate(batch):
            episode['id']=f'public-stack-r1-u{i:03}-e{j:02}'
            episode['seed']=int.from_bytes(hashlib.sha256(('public-stack-timing-schedule-20260921:'+episode['id']).encode()).digest()[:8],'little')
    configs={}
    for mode in ['structured','permuted','disabled']:
        c=copy.deepcopy(config);c['input_mode']=mode;path=root/'configs'/(mode+'.json');write(path,c);configs[mode]=pin(path)
    hardware={host:inventory(host) for host in ['desktop','computehost']}
    available={}
    for host,device in [('desktop',1),('computehost',0)]:
        assert not hardware[host]['active']
        current=preflight(host,[place(host,device,1)]);current['hardware']=hardware[host]
        path=root/(host+'-inventory.json');write(path,current)
        available[host]=dict(checked_at=current['at'],eligible=True,reason='Observed idle eligible training GPU; preserve desktop desktop GPU0.',evidence=pin(path),devices=[dict(ordinal=device,uuid=place(host,device,1)['gpu_uuid'],eligible=True,reason='Observed idle with sufficient VRAM.')])
    cloud=Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json');cloud_value=read(cloud)
    available['runpod']=dict(checked_at=cloud_value['checked_at'],eligible=False,reason='Latest authenticated read-only inventory HTTP403; no new paid allocation authorized.',evidence=pin(cloud),devices=[])
    store=storage(hardware['desktop'],'D')
    cases=[]
    for workers in [1,4,10]:
        for layout in ['a','b']:
            placements={mode:place('desktop' if (mode=='structured')==(layout=='a') else 'computehost',1 if (mode=='structured')==(layout=='a') else 0,workers) for mode in configs}
            cases.append(dict(id=f'{layout}-w{workers}',placements=placements,storage=store))
    dependencies=[pin(Path(__file__).with_name(name)) for name in ['public_training_dispatch_v2.py','public_training_storage_v1.py','public_evaluation_dispatch_v1.py','compute_throughput_v3.py','compute_throughput_v1.py','qualify_state_prevention_compute_v1.py']]
    write(root/'manifest.json',dict(schema='stack-whole-update-qualification/v1',runner=pin(__file__),binary=binary,build=pin(build),schedule_parent=pin(source),configs=configs,dependencies=dependencies,inventory=available,cases=cases,prefix_updates=3,cheap_updates=1,maximum_game_executions=570,total_launch_budget_seconds=1800,native_process_cap_seconds=300,archive_scheme=ARCHIVE,full_training_launched=False,question='Which per-device queue and collector count minimizes full matched structured/permuted/disabled completion time, including staging/recovery, while all saved learning bytes remain exact?',scope='Timing and correctness only. New deterministic matched seeds, inherited balanced deck/seat schedule, terminal rewards. No outcome selection, formal experiment gate or promotion. Full training requires its separate frozen scientific plan.',review='Fable known zero-read HTTP429 through September22 07:00EDT; no retry or endorsement. Bounded compute qualification under desktop research authority.'))
    began=time.monotonic()
    cheap=dispatch(root/(root.name+'-cheap'),binary,configs,cases[0]['placements'],store,1,mode='device_queues',stack=True)
    cheap_reports=reports(cheap)
    for report in cheap_reports.values():
        assert read(checked(report['execution']))['seconds']<90,'cheap native timing exceeds envelope'
    candidates=[];projections={};reference={};comparisons=0;audits={}
    for case in cases:
        assert time.monotonic()-began<1800,'qualification launch budget exhausted'
        group_pin=dispatch(root/(root.name+'-'+case['id']),binary,configs,case['placements'],store,3,mode='device_queues',stack=True)
        group=read(checked(group_pin));per_device={}
        for mode,report in reports(group_pin).items():
            fingerprints={name:pin(checked(item))['sha256'] for name,item in report['outputs'].items()}
            if mode in reference:assert reference[mode]==fingerprints,(case['id'],mode,'saved learning outputs differ');comparisons+=len(fingerprints)
            else:
                for name,item in cheap_reports[mode]['outputs'].items():assert fingerprints[name]==item['sha256'],'cheap and complete prefix differ'
                reference[mode]=fingerprints;audits[mode]=audit(report,mode)
            completion=read(checked(report['completion']));execution=read(checked(report['execution']));steady=sum(r['seconds'] for r in completion['receipts'][1:])/2
            p=report['placement'];device=(p['host'],p['gpu_uuid']);per_device[device]=per_device.get(device,0)+execution['seconds']+197*steady
        projections[case['id']]=max(per_device.values())+group['staging_seconds']+group['recovery_seconds']*200/3
        candidates.append(dict(id=case['id'],benchmark=group_pin));print(dict(completed=case['id'],projected_seconds=projections[case['id']]),flush=True)
    choice=dict(schema='public-training-allocation/v3',jobs={mode:dict(config_sha256=item['sha256'],updates=200) for mode,item in configs.items()},inventory=available,candidates=candidates,selected=min(projections,key=projections.get),archive_scheme=ARCHIVE,storage_dependencies=dependencies,eligible_local_storage=[['D',store['disk_serial']]])
    write(root/'compute-choice.json',choice)
    selected=require_storage_choice(root/'compute-choice.json',binary,configs,stack=True)
    write(root/'result.json',dict(status='STACK-WHOLE-UPDATE-QUALIFICATION-PASS',selected=selected,game_executions=570,unique_arm_games=90,exact_file_comparisons=comparisons,audits=audits,seconds=time.monotonic()-began,full_training_launched=False,non_claim='Projected completion time for pinned workload and current allocation. No playing-strength result or launch authority.'))
    print(selected,flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Qualification requires assertions')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);run(p.parse_args().root.resolve())
