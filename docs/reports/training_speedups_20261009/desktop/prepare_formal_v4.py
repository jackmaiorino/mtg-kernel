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
    parser.add_argument('--inventory', type=Path, required=True, help='Fresh three-host inventory observed by parent; preparation only after desktop free')
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
    formal = ROOT / 'formal-v4'
    formal.mkdir(exist_ok=True)
    controller = pin(ROOT / 'formal_case_driver_v4.py')
    adapter = pin(ROOT / 'formal_launcher_adapter.py')
    storage = {'accounting_roots': [str(ROOT)], 'max_logical_bytes': 160*GIB,
               'reserve_bytes': 60*GIB, 'projected_additional_bytes': 16*GIB,
               'projected_volume_bytes': {'D:': 16*GIB}}
    observed = dispatch.validate_storage(storage)
    require(observed['logical_bytes'] + 32*GIB <= 160*GIB, 'logical projection exceeds cap')
    commands = []
    if args.stage == 'prepare':
        inventory_pin = pin(args.inventory)
        haley_mirror_pin=pin(BASE/'haley-evidence/mirror.json')
        haley_mirror=read(haley_mirror_pin['path']); require(haley_mirror['complete'], 'Haley mirror incomplete')
        haley_transport_pin=pin(BASE/'haley-evidence/transport.json')
        haley_transport=read(haley_transport_pin['path']); require(haley_transport['complete'], 'Haley transport incomplete')
        desktop_transport_pin=pin(ROOT/'transport-sample-20261010.json')
        desktop_transport=read(desktop_transport_pin['path'])
        inventory = read(inventory_pin['path'])
        require(set(inventory) == {'desktop', 'computehost', 'runpod'}, 'three host inventory required')
        require(not inventory['desktop']['reservations'], 'desktop reserved')
        haley_volume=next(v for v in inventory['computehost']['volumes'] if v['DeviceID']=='C:')
        haley_eligible=not inventory['computehost']['reservations'] and haley_volume['FreeSpace']-60*GIB >= 16*GIB
        haley_reason=('free qualified sixteen logical CPUs; fresh C headroom admits16GiB above60GiB reserve' if haley_eligible else 'fresh inventory: Haley reserved' if inventory['computehost']['reservations'] else 'fresh inventory: Haley C headroom below16GiB projection above60GiB reserve')
        now = datetime.now(timezone.utc)
        for host in ('desktop', 'computehost'):
            age = (now-datetime.fromisoformat(inventory[host]['observed_utc'].replace('Z','+00:00'))).total_seconds()
            require(0 <= age <= 600, 'stale inventory')
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
            haley_entries=[{'report':x['report'],'path_mappings':x['path_mappings']} for x in haley_mirror['cases'] if x['variant']==variant]
            require([x['workers'] for x in haley_mirror['cases'] if x['variant']==variant]==[1,2,4,8], 'Haley worker coverage differs')
            for entry in haley_entries:
                require(pin(entry['report']['path'])==entry['report'], 'Haley mirror report changed')
                r=read(entry['report']['path']); require(r['complete'] and r['fingerprint']==canonical,'Haley parity differs')
            entries=refs+(haley_entries if haley_eligible else [])
            transport_by_host={'desktop':desktop_transport['projected_transport_seconds'],'computehost':haley_transport['projected_transport_seconds']}
            scores=[]
            for entry in entries:
                ref=entry.get('report',entry); r=read(ref['path'])
                scores.append(dispatch.projected_seconds(r,source,'training')+transport_by_host[r['placement']['host']])
            best=min(range(len(entries)),key=lambda i:scores[i])
            selected_ref=entries[best].get('report',entries[best]); selected=read(selected_ref['path'])
            require(selected['placement']['host']=='desktop' and selected['placement']['workers']=={'baseline':8,'candidate':4}[variant], 'fastest eligible selection differs; report before staging execution')
            status={}
            for host in inventory:
                status[host]={'eligible':host=='desktop' or (host=='computehost' and haley_eligible),
                    'checked_at':inventory.get(host,{}).get('observed_utc',inventory['desktop']['observed_utc']),
                    'evidence':inventory_pin,'cpu_affinity':CORES if host=='desktop' else list(range(16)) if host=='computehost' else [],
                    'transport_seconds':transport_by_host.get(host,0),
                    'transport_evidence':desktop_transport_pin if host=='desktop' else haley_transport_pin if host=='computehost' else inventory_pin,
                    'reason':{'desktop':'free qualified eight P cores; CI18-23 preserved','computehost':haley_reason,'runpod':'API403 and no paid authority'}[host]}
            choice={'schema':dispatch.CHOICE,'inventory':status,'qualifications':entries,'selected':{family:selected_ref}}
            choices[variant]=(write(formal/(variant+'.choice.json'),choice),selected['placement']['workers'])
        for index, variant in enumerate(('baseline','candidate','candidate','baseline'),1):
            label=f'clean-{index:02d}-{variant}'
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
              'source_config':source_pin,'workload_family':family,'inventory':inventory_pin,'haley_qualification_mirror':haley_mirror_pin,'haley_transport':haley_transport_pin,
              'controller':controller,'adapter':adapter,'helper':pin(__file__),'storage':storage,
              'logical_projection_bytes':32*GIB,'haley_reserved_logical_bytes':32*GIB,'transport_evidence':pin(ROOT/'transport-sample-20261010.json'),'excluded_prior_attempt':pin(ROOT/'formal-v2/coordinator/state.json'),'commands':commands,
              'seal_argv':[str(PYTHON),'-B',str(Path(__file__)),'seal','--inventory',str(args.inventory)],
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
