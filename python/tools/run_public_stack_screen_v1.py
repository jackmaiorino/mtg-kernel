"""Launch only the frozen bounded three-arm stack screen through compute guards."""
import argparse
import os
import shutil
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from public_training_dispatch_v2 import read,write,pin,checked
from public_training_storage_v1 import require_storage_choice,dispatch_stack_qualified
from public_evaluation_dispatch_v1 import inventory
from public_stack_analysis_v1 import GATES,self_check


def audit(group_pin,manifest,root):
    group=read(checked(group_pin));assert set(group['jobs'])=={'structured','permuted','disabled'}
    archive=read(checked(group['archive']));assert archive['mismatches']==0
    arms={}
    for arm,item in group['jobs'].items():
        report=read(checked(item));assert report['config']==manifest['training_configs'][arm]
        execution=read(checked(report['execution']))
        assert execution['exit_code']==0 and not execution['timeout']
        assert execution['observed_gpu_uuid']==report['placement']['gpu_uuid']
        completion=read(checked(report['completion']))
        assert completion['first_update']==0 and completion['next_update']==200 and len(completion['receipts'])==200
        expected=set()
        for i,receipt in enumerate(completion['receipts']):
            assert receipt['update']==i and receipt['episodes']==receipt['natural_games']==10
            assert receipt['legacy_adam_step']==32401+i and receipt['stack_adam_step']==i+1
            expected.update(f'{i:04}/{name}' for name in ['checkpoint.json','optimizer.json']+[f'episode-{j:03}.json' for j in range(10)])
            checkpoint=read(checked(report['outputs'][f'{i:04}/checkpoint.json']))
            assert checkpoint['optimizer_sha256']==report['outputs'][f'{i:04}/optimizer.json']['sha256']
            assert checkpoint['trajectory_sha256']==[report['outputs'][f'{i:04}/episode-{j:03}.json']['sha256'] for j in range(10)]
        assert set(report['outputs'])==expected
        for output in report['outputs'].values():checked(output)
        final=read(checked(report['outputs']['0199/optimizer.json']))
        assert final['legacy_adam_step']==32600 and final['stack']['adam_step']==200
        if arm=='disabled':
            assert all(v in [0,1<<31] for k in ['weight','first','second'] for v in final['stack'][k])
        endpoint=root/'endpoints'/arm;endpoint.mkdir(parents=True)
        pins={}
        for kind in ['checkpoint','optimizer']:
            old=report['outputs'][f'0199/{kind}.json'];path=endpoint/f'{kind}.json'
            shutil.copy2(checked(old),path);pins[kind]=pin(path)
            assert pins[kind]['sha256']==old['sha256']
        arms[arm]=dict(updates=200,natural_games=2000,**pins,report=item,placement=report['placement'],native_seconds=execution['seconds'])
    return dict(complete=True,full_natural_games=6000,arms=arms,group=group_pin,archive=group['archive'],
        non_claim='All learning streams complete. No outcome-based selection before the full paired evaluation.')


def run(root,compute):
    m=read(root/'manifest.json')
    assert m['schema']=='matched-public-stack-screen/v1' and m['gates']==GATES
    assert not (root/'training-launch.json').exists(),'preserve prior launch'
    for key in ['runner','design','analysis','training_binary','training_build','evaluation_binary','evaluation_build','evaluation_qualification','evaluation_transfer_comparison']:
        checked(m[key])
    assert m['analysis']==pin(Path(__file__).with_name('public_stack_analysis_v1.py'))
    self_check()
    corrected=read(compute/'result.json')
    assert corrected['status']=='STACK-TIMING-CORRECTION-PASS' and not corrected['full_training_launched']
    selected=require_storage_choice(compute/'compute-choice.json',m['training_binary'],m['training_configs'],stack=True)
    assert selected==corrected['selected'] and selected['projected_seconds']<m['training_projection_cap_seconds']
    current={h:inventory(h) for h in ['jack','haleyspc']}
    assert all(not s['active'] for s in current.values()),'preserve competing native owners'
    cuda=Path('C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8')
    os.environ['CUDA_PATH']=str(cuda);os.environ['PATH']=str(cuda/'bin')+os.pathsep+os.environ['PATH']
    (root/'temp').mkdir();os.environ['TEMP']=os.environ['TMP']=str(root/'temp')
    write(root/'training-launch.json',dict(pilot=pin(root/'manifest.json'),runner=pin(__file__),
        compute=pin(compute/'result.json'),choice=pin(compute/'compute-choice.json'),selected=selected,
        current_inventory=current,expected_natural_games=6000,analysis=m['analysis'],
        native_wall_seconds=m['native_training_wall_cap_seconds'],review=m['review'],no_paid_compute=True,
        telemetry='Owned native worker CPU/RSS/I/O and GPU samples every5seconds; per-update stage receipts. No prefix outcome analysis.'))
    started=time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future=pool.submit(dispatch_stack_qualified,root/'stack-screen-training',m['training_binary'],
            m['training_configs'],compute/'compute-choice.json',m['native_training_wall_cap_seconds'])
        tick=0
        while not future.done():
            print(dict(phase='training-or-recovery',elapsed_seconds=round(time.monotonic()-started),tick=tick),flush=True)
            tick+=1
            for _ in range(30):
                if future.done():break
                time.sleep(1)
        result=future.result()
    audited=audit(result,m,root);audited['wall_seconds_including_audit']=time.monotonic()-started
    write(root/'training-audit.json',audited)
    print(dict(complete=True,natural_games=6000,evaluation_launched=False),flush=True)


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--compute',type=Path,required=True);a=p.parse_args();run(a.root,a.compute)
