"""Prepare desktop ABBA requests or seal verified choices. Never execute them."""
import argparse
import copy
from datetime import datetime, timezone
import importlib
import json
from pathlib import Path
import sys
from prepare_desktop import ROOT, PYTHON, CORES, LAUNCHERS, pin, write, require

BASE = ROOT.parent
GIB = 1024**3

def read(path):
    return json.loads(Path(path).read_bytes())

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('stage', choices=['prepare', 'seal'])
    args = parser.parse_args()
    sys.path.insert(0, str(ROOT / 'candidate/python/tools'))
    dispatch = importlib.import_module('native_expanded_dispatch_v1')
    source_pin = pin(ROOT / 'source-config.json')
    frozen = read(ROOT / 'qualification.plan.json')
    require(source_pin == frozen['source_config'], 'source changed')
    source = read(source_pin['path'])
    family = dispatch.workload(source, 'training')
    require(family == frozen['workload_family'], 'source family changed')
    native = ROOT / 'measure/hot/matched-native'
    require(not native.exists(), 'common native path already exists')
    formal = ROOT / 'formal-v2'
    formal.mkdir(exist_ok=True)
    controller = pin(ROOT / 'formal_case_driver.py')
    adapter = pin(ROOT / 'formal_launcher_adapter.py')
    storage = {'accounting_roots': [str(ROOT)], 'max_logical_bytes': 160*GIB,
               'reserve_bytes': 60*GIB, 'projected_additional_bytes': 16*GIB,
               'projected_volume_bytes': {'D:': 16*GIB}}
    observed = dispatch.validate_storage(storage)
    require(observed['logical_bytes'] + 32*GIB <= 160*GIB, 'logical projection exceeds cap')
    commands = []
    if args.stage == 'prepare':
        inventory_pin = pin(BASE / 'inventory-20261010-status.json')
        exclusion = pin(BASE / 'allocation-attempt2-receipts-20261010.json')
        inventory = read(inventory_pin['path'])
        require(set(inventory) == {'desktop', 'computehost', 'runpod'}, 'three host inventory required')
        require(not inventory['desktop']['reservations'], 'desktop was reserved')
        now = datetime.now(timezone.utc)
        for host in ('desktop', 'computehost'):
            age = (now-datetime.fromisoformat(inventory[host]['observed_utc'].replace('Z','+00:00'))).total_seconds()
            require(0 <= age <= 86400, 'stale inventory')
        require('403' in inventory['runpod'].get('message',''), 'RunPod limitation absent')
        for item in frozen['input_dependencies']:
            require(pin(item['original']['path'])['sha256'] == item['original']['sha256'], 'source input changed')
        choices = {}
        canonical = read(ROOT/'measure/hot/qual-baseline-w1/report.json')['fingerprint']
        for variant in ('baseline','candidate'):
            launcher = ROOT/variant/'python/tools/native_expanded_dispatch_v1.py'
            require(pin(launcher)['sha256'] == LAUNCHERS[variant], 'frozen launcher changed')
            refs = [pin(ROOT/f'measure/hot/qual-{variant}-w{w}/report.json') for w in (1,2,4,8)]
            reports = [read(ref['path']) for ref in refs]
            for w, report in zip((1,2,4,8), reports):
                require(report['complete'] and report['qualification'] and report['completed_updates']==1
                        and report['completed_games']==10 and report['placement']['workers']==w
                        and report['fingerprint']==canonical, 'qualification coverage or parity changed')
            best = min(range(4), key=lambda i: dispatch.projected_seconds(reports[i],source,'training'))
            status = {}
            for host in inventory:
                status[host] = {'eligible': host=='desktop',
                    'checked_at': inventory.get(host,{}).get('observed_utc',inventory['desktop']['observed_utc']),
                    'evidence': exclusion if host=='computehost' else inventory_pin, 'cpu_affinity': CORES if host=='desktop' else [],
                    'transport_seconds': read(ROOT/'transport-sample-20261010.json')['projected_transport_seconds'] if host=='desktop' else 0, 'transport_evidence': pin(ROOT/'transport-sample-20261010.json') if host=='desktop' else exclusion,
                    'reason': {'desktop':'free qualified eight P cores; CI18-23 preserved',
                               'computehost':'measured 15669643620 physical bytes exceed 6820810752 headroom above reserve; pinned allocation evidence',
                               'runpod':'API403 and no paid authority'}[host]}
            choice = {'schema':dispatch.CHOICE,'inventory':status,'qualifications':refs,'selected':{family:refs[best]}}
            choices[variant]=(write(formal/(variant+'.choice.json'),choice),reports[best]['placement']['workers'])
        for index, variant in enumerate(('baseline','candidate','candidate','baseline'),1):
            label=f'block-{index:02d}-{variant}'
            hot,cold=ROOT/'measure/hot'/label,ROOT/'measure/cold'/label
            require(not hot.exists() and not cold.exists(),'output already exists')
            choice, workers=choices[variant]
            config=copy.deepcopy(source)
            config.update(output_directory=str(native),collection_workers=workers,preparation_workers=workers)
            require(dispatch.workload(config,'training')==family,'formal family changed')
            request={'schema':dispatch.SCHEMA,'kind':'training','runtime':pin(ROOT/'runtimes'/(variant+'.json')),
                     'config':write(formal/(label+'.config.json'),config),'root':str(hot),'cold_root':str(cold),
                     'lane':'codex-training-speedups-20261009','placement':{'host':'desktop','workers':workers,
                     'preparation_workers':workers,'cpu_affinity':CORES,'memory_bytes':32*GIB},
                     'wall_seconds':3600,'storage':storage,'comparison_logical_projection_bytes':32*GIB,
                     'choice':choice,'choice_verification_output':str(formal/(label+'.verified.json'))}
            if source.get('max_non_natural_episode_fraction',0):
                request['non_natural_tolerance']=source['max_non_natural_episode_fraction']
            ref=write(formal/(label+'.json'),request)
            commands.append({'label':label,'variant':variant,'workers':workers,'request':ref,
                             'argv':[str(PYTHON),'-B',str(ROOT/variant/'python/tools/native_expanded_dispatch_v1.py'),
                                     'check-choice',ref['path']]})
        plan={'schema':'training-speedup-desktop-formal-staging/v1','stage':'prepare','staging_only':True,
              'source_config':source_pin,'workload_family':family,'inventory':inventory_pin,'placement_exclusion':exclusion,
              'controller':controller,'adapter':adapter,'helper':pin(__file__),'storage':storage,
              'logical_projection_bytes':32*GIB,'haley_reserved_logical_bytes':32*GIB,'transport_evidence':pin(ROOT/'transport-sample-20261010.json'),'supersedes_unexecuted':pin(ROOT/'formal/prepare.plan.json'),'commands':commands,
              'seal_argv':[str(PYTHON),'-B',str(Path(__file__)),'seal'],
              'remaining_prerequisites':'Parent review; execute public check-choice; seal; review guarded formal launch; serial inspect/copy/retain between cases. No automatic launches or retries.'}
    else:
        original=read(formal/'prepare.plan.json')
        require(original['controller']==controller and original['adapter']==adapter,'formal guard helper changed')
        for case in original['commands']:
            ref=case['request']; require(pin(ref['path'])==ref,'prepared request changed')
            request=read(ref['path']); verification=pin(request['choice_verification_output'])
            result=read(verification['path'])
            require(result['outputs_verified'] and result['choice']==request['choice'],'choice verification differs')
            request['choice_verification']=verification
            final=write(formal/(case['label']+'.dispatch.json'),request)
            commands.append({'label':case['label'],'request':final,'argv':[str(PYTHON),'-B',controller['path'],
                '--launcher-root',str(ROOT/case['variant']),'--request',final['path'],'--action','dispatch',
                '--receipt',str(ROOT/'controllers'/(case['label']+'.json')),'--declare-desktop-cores']})
        plan={'schema':'training-speedup-desktop-formal-staging/v1','stage':'seal','staging_only':True,
              'prepared_plan':pin(formal/'prepare.plan.json'),'controller':controller,'adapter':adapter,'commands':commands}
    output=write(formal/(args.stage+'.plan.json'),plan)
    print(json.dumps({'plan':output,'commands':len(commands),'stage':args.stage}))

if __name__=='__main__':
    main()
