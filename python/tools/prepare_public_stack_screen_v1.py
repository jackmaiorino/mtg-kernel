"""Freeze the bounded stack screen without dispatching training or evaluation."""
import argparse
import copy
import hashlib
from pathlib import Path
from public_training_dispatch_v2 import read,write,pin,checked
from state_prevention_pilot_v1 import collect_seeds
from public_stack_analysis_v1 import GATES,self_check


def prepare(root):
    repo=Path(__file__).resolve().parents[2]
    self_check()
    compute=Path('E:/mtg-meta-recovery-20260921/public-stack-compute-001')
    cm=read(compute/'manifest.json')
    configs=cm['configs']
    canonical={}
    for mode,item in configs.items():
        c=read(checked(item));assert c['input_mode']==mode
        assert len(c['updates'])==200 and all(len(b)==10 for b in c['updates'])
        canonical[mode]={k:v for k,v in c.items() if k!='input_mode'}
    assert canonical['structured']==canonical['disabled']==canonical['permuted']
    c=read(checked(configs['structured']))
    original=Path('E:/mtg-postboard-campaign-20260921/public-balanced-schedule-pilot-001/replica-1/manifest.json')
    base=read(original)
    used=set();forbidden=set();prior={}
    def add(item):
        checked(item)
        if item['path'] not in prior:
            prior[item['path']]=item;forbidden.update(collect_seeds(read(item['path'])))
    for item in configs.values():add(item)
    for replica in [1,2]:
        m=read(original.parents[1]/f'replica-{replica}/manifest.json')
        for item in [*m['training_configs'].values(),*m['prior_seed_inputs'],*[j['template'] for j in m['jobs']]]:add(item)
    namespace='public-stack-screen-20260921/v1/'
    def fresh(label):
        value=int.from_bytes(hashlib.sha256((namespace+label).encode()).digest()[:8],'little')
        assert value not in used and value not in forbidden
        used.add(value);return value
    root.mkdir();(root/'templates').mkdir()
    jobs=[];eval_seeds={}
    for old in base['jobs']:
        command=read(checked(old['template']));cases=copy.deepcopy(old['cases'])
        for case,match in zip(cases,command['matches']):
            key=(case['own'],case['opponent'],case['replicate'])
            if key not in eval_seeds:eval_seeds[key]=fresh('eval/'+repr(key))
            case['seed']=eval_seeds[key];case['cohort']='stack'
            match['config']['seed']=case['seed']
        for offset in range(0,64,8):
            chunk=copy.deepcopy(command);chunk['matches']=chunk['matches'][offset:offset+8]
            label=f"{old['label']}-c{offset//8:02}"
            path=root/'templates'/f'{label}.json';write(path,chunk)
            jobs.append(dict(label=label,candidate_seat=old['candidate_seat'],cases=cases[offset:offset+8],
                template=pin(path),original_template=old['template'],original_indices=list(range(offset,offset+8))))
    assert len(jobs)==128 and sum(len(j['cases']) for j in jobs)==1024 and len(eval_seeds)==512
    native=Path('E:/mtg-meta-recovery-20260921/public-stack-evaluation-tools-002/completion.json')
    qualification=Path('E:/mtg-meta-recovery-20260921/public-stack-evaluation-003')
    opponent=read(qualification/'trained-structured-p0/request.json')['sources'][1]
    assert read(qualification/'transfer-comparison.json')['parameters_and_optimizer_exact']
    manifest=dict(schema='matched-public-stack-screen/v1',stage='prepared-not-launched',runner=pin(__file__),
        design=pin(repo/'docs/public_stack_screen_design_20260921.md'),analysis=pin(Path(__file__).with_name('public_stack_analysis_v1.py')),
        training_configs=configs,training_binary=cm['binary'],training_build=cm['build'],source=c['source'],
        timing_configuration_source=pin(compute/'manifest.json'),allocation_status='Original choice revoked; new uncontended qualification required.',
        evaluation_binary=read(native)['binaries']['public_feature_evaluation_v1'],evaluation_build=pin(native),
        evaluation_qualification=pin(qualification/'result.json'),evaluation_opponent=opponent,
        evaluation_transfer_comparison=pin(qualification/'transfer-comparison.json'),
        jobs=jobs,gates=GATES,bootstrap_seed=fresh('bootstrap'),prior_seed_inputs=list(prior.values()),seed_namespace=namespace,
        training_games=6000,evaluation_bo3=4096,evaluation_unique_seeds=512,training_projection_cap_seconds=3600,
        native_training_wall_cap_seconds=1800,final_update=199,
        review='Fable zero-read HTTP429 throughSeptember22 07:00EDT; bounded preparation under desktop research authority, no endorsement.',
        non_claim='One training seed stream and one development opponent. Pass means replicate, never promotion or human-level strength. CP7 excluded; no outcome-prefix selection.')
    write(root/'manifest.json',manifest)
    print(dict(prepared=str(root),training_games=6000,evaluation_bo3=4096,jobs=512,launched=False))


if __name__=='__main__':
    if not __debug__:raise RuntimeError('Assertions required')
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);a=p.parse_args();prepare(a.root.resolve())
