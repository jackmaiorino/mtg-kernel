"""Sequential sealed desktop ABBA orchestration. Execution requires explicit --execute.
Stops permanently on failure; no retries, request edits, or reservation manipulation.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

BASE = Path('D:/training-speedups-20261009')
ROOT = BASE / 'desktop'
HELPERS = BASE / 'desktop_helpers'
COMMON = ROOT / 'measure/hot/matched-native'
GIB = 1024**3

def require(ok, message):
    if not ok: raise ValueError(message)

def pin(path):
    path = Path(path)
    with path.open('rb') as f: h = hashlib.file_digest(f, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': h}

def read(path): return json.loads(Path(path).read_bytes())
def checked(ref):
    require(pin(ref['path']) == ref, 'pin changed: ' + ref['path'])
    return read(ref['path'])

def now(): return datetime.now(timezone.utc).isoformat()

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--plan', type=Path, default=ROOT/'formal-v2/seal.plan.json')
    ap.add_argument('--decks', type=Path, required=True)
    ap.add_argument('--t1', type=Path, required=True)
    ap.add_argument('--execute', action='store_true')
    args = ap.parse_args()
    plan_pin = pin(args.plan); plan = checked(plan_pin)
    require(plan['stage'] == 'seal' and len(plan['commands']) == 4, 'sealed four-case plan required')
    require([x['label'] for x in plan['commands']] == ['block-01-baseline','block-02-candidate','block-03-candidate','block-04-baseline'], 'ABBA order differs')
    for name in ('controller','adapter'): checked(plan[name]) if plan[name]['path'].endswith('.json') else require(pin(plan[name]['path']) == plan[name], 'helper changed')
    post = HELPERS/'postprocess_case.py'; copier = HELPERS/'copy_recovery.py'
    allocation = BASE/'measure_allocations.py'
    sources = [pin(x) for x in (Path(__file__), post, copier, HELPERS/'desktop_devices.py', allocation, args.decks, args.t1)]
    sources += [plan['controller'], plan['adapter'], plan_pin]
    requests = []
    for variant in ('baseline','candidate'):
        sources += [pin(x) for x in (ROOT/variant/'python/tools').glob('*.py')]
    # The exposure collector derives its registration/card-map repository from
    # the decks path. Validate those real dependencies before any native block,
    # then keep them in the same source stability checks as the decks themselves.
    sys.path.insert(0, str(ROOT/'baseline/python/tools'))
    deck_loader = importlib.import_module('nine_deck_baseline_v1')
    deck_repo = args.decks.resolve().parents[1]
    sources += [pin(deck_repo/deck_loader.REGISTRATIONS_PATH), pin(deck_repo/'data/cards_v1.json')]
    deck_loader.load_decks(args.decks)
    for case in plan['commands']:
        request = checked(case['request']); config = checked(request['config'])
        require(Path(config['output_directory']) == COMMON, 'common native directory differs')
        require(len(config['iterations']) == 162 and sum(len(i['episodes']) for i in config['iterations']) == 1620, 'full workload differs')
        require(pin(args.t1)['sha256'] == config['initial_source']['checkpoint']['sha256'], 'T1 differs')
        sources += [case['request'], request['config'], request['runtime'], request['choice'], request['choice_verification']]
        require('--action' in case['argv'] and case['argv'][case['argv'].index('--action')+1] == 'dispatch', 'public guarded dispatch required')
        requests.append(request)
    require(not COMMON.exists(), 'common output exists; preserve it')
    out = args.plan.parent/'coordinator'
    require(not out.exists(), 'coordinator attempt exists; preserve it, no automatic resume')
    if not args.execute:
        print(json.dumps({'ready':True,'execution':False,'plan':plan_pin,'sources':sources,'command_requires':'--execute after parent review'})); return
    out.mkdir()
    state = dict(schema='training-speedup-desktop-formal-coordinator/v1', complete=False, started_utc=now(), plan=plan_pin, sources=sources, sequential_nonoverlapping=True, phases=[], completed=[])
    statepath = out/'state.json'
    def save():
        temporary = out/'state.tmp'
        with temporary.open('w',encoding='utf-8') as f:
            json.dump(state,f,indent=2); f.flush(); os.fsync(f.fileno())
        os.replace(temporary,statepath)
    def stable():
        for ref in sources: require(pin(ref['path']) == ref, 'source/input changed: '+ref['path'])
    def run(label, phase, argv):
        stable(); record=dict(case=label,phase=phase,argv=argv,started_utc=now()); state['phases'].append(record); save()
        start=time.monotonic()
        with (out/(label+'.'+phase+'.stdout')).open('xb') as stdout, (out/(label+'.'+phase+'.stderr')).open('xb') as stderr:
            child=subprocess.run(argv,stdout=stdout,stderr=stderr)
        record.update(seconds=time.monotonic()-start,exit_code=child.returncode,finished_utc=now()); save()
        require(child.returncode == 0, 'phase failed; preserved receipts: '+label+'/'+phase)
    def measure(label, native):
        stable(); p=out/(label+'.allocation.json')
        spec=importlib.util.spec_from_file_location('case_allocation_inventory',allocation)
        inventory=importlib.util.module_from_spec(spec); spec.loader.exec_module(inventory)
        inventory.check_parents(ROOT); inventory.check_parents(native)
        require(ROOT.is_dir() and native.is_dir() and native.is_relative_to(ROOT),'owned allocation roots required')
        result=inventory.measure(ROOT,native,inventory.file_api(),'fail')
        result.update(schema='training-speedups-allocation-inventory/v1',root=str(ROOT),native_root=str(native))
        with p.open('x',encoding='utf-8') as f:
            json.dump(result,f,indent=2); f.flush(); os.fsync(f.fileno())
        require(result['complete'] and not result['skipped_reparse_points'] and result['representative_native']['files']>0,'allocation failed; preserve attempt')
        return result
    save()
    try:
        before=measure('before',ROOT/'measure/hot/qual-baseline-w1/native')
        for index,(case,request) in enumerate(zip(plan['commands'],requests)):
            case_start=time.monotonic()
            label=case['label']; python=case['argv'][0]
            require(not COMMON.exists(),'prior native root remains')
            run(label,'dispatch',case['argv'])
            controller=checked(pin(ROOT/'controllers'/(label+'.json'))); report=checked(controller['report'])
            require(controller['complete'] and report['complete'] and report['completed_updates']==162 and report['completed_games']==1620,'incomplete full block')
            if index == 0:
                state['canonical_fingerprint'] = report['fingerprint']; save()
            else:
                require(report['fingerprint'] == state['canonical_fingerprint'], 'formal full fingerprint differs')
            if index==0:
                audit_start=time.monotonic()
                after=measure('first-block',COMMON)
                growth={kind:sum(v[kind] for v in after['classes'].values())-sum(v[kind] for v in before['classes'].values()) for kind in ('logical_bytes','allocated_bytes')}
                require(growth['logical_bytes'] <= request['comparison_logical_projection_bytes'],'logical measured growth exceeded projection')
                require(growth['allocated_bytes'] <= request['storage']['projected_volume_bytes']['D:'],'physical measured growth exceeded projection')
                require(shutil.disk_usage('D:/').free >= request['storage']['reserve_bytes']+request['storage']['projected_volume_bytes']['D:'],'next block reserve insufficient')
                state['first_block_reconciliation']=dict(seconds=time.monotonic()-audit_start,growth=growth,measurement=pin(out/'first-block.allocation.json'),accounting='One-time allocation audit overhead; excluded from per-case complete elapsed totals'); save()
            maintenance=ROOT/'measure/maintenance'/label
            common=[python,'-B',str(post),'--launcher-root',str(ROOT/label.split('-')[-1]),'--comparison-root',str(ROOT),'--request',case['request']['path'],'--decks',str(args.decks),'--t1',str(args.t1),'--out',str(maintenance),'--recovery-mode','desktop-local']
            run(label,'inspect',common[:3]+['inspect']+common[3:])
            inspection=read(maintenance/'inspect.json'); recovery=inspection['recovery_copy_plan']; checked(recovery)
            run(label,'copy',[python,'-B',str(copier),'--mode','desktop-local','--case-label',label,'--local-plan',recovery['path'],'--plan-sha256',recovery['sha256']])
            receipt=pin(maintenance/'cold-copy-receipt.json'); require(checked(receipt)['complete'],'copy incomplete')
            run(label,'retain',common[:3]+['retain']+common[3:]+['--cold-copy-receipt',receipt['path'],'--cold-copy-sha256',receipt['sha256']])
            retention=read(maintenance/'retain.json'); require(retention['complete'] and not COMMON.exists(),'retention did not free common path')
            # Copy late inspection/retention metadata after its creation, with fsync and full readback.
            stable(); start=time.monotonic(); late=[]
            sys.path.insert(0,str(HELPERS)); import copy_recovery
            cold=Path('E:/training-speedups-20261009')/label/'late'
            require(not cold.exists(),'late metadata destination exists')
            inputs=[ROOT/'controllers'/(label+'.json')]+list(maintenance.rglob('*'))
            inputs=[p for p in inputs if p.is_file()]
            require(shutil.disk_usage('E:/').free >= 60*GIB+sum(p.stat().st_size for p in inputs),'late copy reserve insufficient')
            for source in inputs:
                copy_recovery.safe_local(source)
                destination=cold/source.relative_to(ROOT)
                expected=pin(source); copy_recovery.cold_copy(source,destination)
                actual=pin(destination); require(actual['sha256']==expected['sha256'],'late metadata readback differs')
                late.append(dict(source=expected,destination=actual,bytes=source.stat().st_size))
            late_receipt=dict(schema='training-speedup-late-copy/v1',complete=True,case=label,request=case['request'],report=controller['report'],retain=pin(maintenance/'retain.json'),files=late,seconds=time.monotonic()-start,finished_utc=now(),accounting='Payload copy, fsync and SHA readback included; copying this receipt itself is separate metadata envelope overhead.')
            latepath=out/(label+'.late-copy.json'); latepath.write_text(json.dumps(late_receipt,indent=2),encoding='utf-8')
            metadata_start=time.monotonic()
            latecold=cold/'late-copy-receipt.json'
            copy_recovery.cold_copy(latepath,latecold)
            require(pin(latecold)['sha256']==pin(latepath)['sha256'],'late copy receipt readback differs')
            state['completed'].append(dict(label=label,report=controller['report'],inspect=pin(maintenance/'inspect.json'),transport=receipt,retain=pin(maintenance/'retain.json'),late_copy=pin(latepath),late_receipt_metadata_seconds=time.monotonic()-metadata_start,late_copy_cold_receipt=pin(latecold),coordinator_case_wall_seconds=time.monotonic()-case_start)); save()
        state.update(complete=True,finished_utc=now()); save()
        finalcold=Path('E:/training-speedups-20261009')/'formal-v2-final-state.json'
        require(not finalcold.exists(),'final metadata destination exists')
        copy_recovery.cold_copy(statepath,finalcold)
        require(pin(finalcold)['sha256']==pin(statepath)['sha256'],'final state readback differs')
    except BaseException as error:
        state.update(complete=False,error=type(error).__name__+': '+str(error),finished_utc=now()); save(); raise

if __name__=='__main__': main()
