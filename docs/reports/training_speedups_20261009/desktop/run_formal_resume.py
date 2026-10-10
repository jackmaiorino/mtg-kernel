"""Explicit continuation of interrupted sealed desktop ABBA orchestration. Execution requires explicit --execute.
Stops permanently on failure; no retries, request edits, or reservation manipulation.
"""
import argparse
from datetime import datetime, timezone
import hashlib
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
    dependencies=BASE/'desktop/maintenance-dependencies-20261010.json'
    dependency_records=read(dependencies)
    sources.append(pin(dependencies))
    for record in dependency_records:
        ref={'path':record['path'],'sha256':record['sha256']}
        require(pin(ref['path'])==ref,'staged maintenance dependency changed')
        sources.append(ref)
    requests = []
    for variant in ('baseline','candidate'):
        sources += [pin(x) for x in (ROOT/variant/'python/tools').glob('*.py')]
    for case in plan['commands']:
        request = checked(case['request']); config = checked(request['config'])
        require(Path(config['output_directory']) == COMMON, 'common native directory differs')
        require(len(config['iterations']) == 162 and sum(len(i['episodes']) for i in config['iterations']) == 1620, 'full workload differs')
        require(pin(args.t1)['sha256'] == config['initial_source']['checkpoint']['sha256'], 'T1 differs')
        sources += [case['request'], request['config'], request['runtime'], request['choice'], request['choice_verification']]
        require('--action' in case['argv'] and case['argv'][case['argv'].index('--action')+1] == 'dispatch', 'public guarded dispatch required')
        requests.append(request)
    require(COMMON.is_dir(), 'completed first native output required')
    require(not (ROOT/'measure/maintenance/block-01-baseline').exists(), 'parent must preserve partial inspection outside canonical maintenance path first')
    out = args.plan.parent/'coordinator-resume'
    require(not out.exists(), 'coordinator attempt exists; preserve it, no automatic resume')
    prior_pin={'path': 'D:\\training-speedups-20261009\\desktop\\formal-v2\\coordinator\\state.json', 'sha256': '8025260f737e38503e798c8339b502d9fc42c9249f2b7af25cc3a5f1f7390592'}
    first_controller_pin={'path': 'D:\\training-speedups-20261009\\desktop\\controllers\\block-01-baseline.json', 'sha256': '58318c491b8909c60b1eeb0eea54357a2627f3a0abe79ef5c453c0affd33b3bf'}
    first_report_pin={'path': 'D:\\training-speedups-20261009\\desktop\\measure\\hot\\block-01-baseline\\report.json', 'sha256': 'e61f64483db199cf9c1352e46bbb1b7ce3739abeb49685224d935168a2b3116e'}
    prior=checked(prior_pin); first_controller=checked(first_controller_pin); first_report=checked(first_report_pin)
    require(prior['complete'] is False and prior.get('error') and not prior['completed'], 'unexpected prior stop state')
    require(prior['plan'] == plan_pin and first_controller['report'] == first_report_pin, 'prior case/plan binding differs')
    require(first_controller['request'] == plan['commands'][0]['request'] and first_controller['complete'] and first_report['complete'], 'first dispatch not complete or bound')
    require(first_report['completed_updates']==162 and first_report['completed_games']==1620, 'first full counts differ')
    require(first_report['fingerprint']==prior['canonical_fingerprint'], 'prior first fingerprint differs')
    checked(first_report['execution']); checked(first_report['archive']); checked(first_report['run'])
    checked(prior['first_block_reconciliation']['measurement'])
    sources += [prior_pin,first_controller_pin,first_report_pin,prior['first_block_reconciliation']['measurement']]
    for ref in prior['sources']:
        require(pin(ref['path']) == ref, 'frozen prior source/config/runtime input changed')
    inspection_argv=prior['phases'][-1]['argv']
    require(pin(args.decks) == next(ref for ref in prior['sources'] if ref['path'] == inspection_argv[inspection_argv.index('--decks')+1]), 'decks changed')
    require(pin(args.t1) == next(ref for ref in prior['sources'] if ref['path'] == inspection_argv[inspection_argv.index('--t1')+1]), 'T1 changed')
    if not args.execute:
        print(json.dumps({'ready':True,'execution':False,'plan':plan_pin,'sources':sources,'command_requires':'--execute after parent review'})); return
    out.mkdir()
    state = dict(schema='training-speedup-desktop-formal-coordinator/v1', complete=False, started_utc=now(), plan=plan_pin, sources=sources, sequential_nonoverlapping=True, phases=[], completed=[])
    state.update(prior_state=prior_pin,reused_first_controller=first_controller_pin,reused_first_report=first_report_pin,canonical_fingerprint=prior['canonical_fingerprint'],first_block_reconciliation=prior['first_block_reconciliation'],interruption_overhead={'failed_inspection_seconds':prior['phases'][-1]['seconds'],'prior_dispatch_controller_seconds':prior['phases'][0]['seconds'],'prior_finished_utc':prior['finished_utc'],'resume_started_utc':state['started_utc'],'stopped_gap_seconds':(datetime.fromisoformat(state['started_utc'])-datetime.fromisoformat(prior['finished_utc'])).total_seconds(),'accounting':'Failed inspection and stopped gap excluded from matched completed phases but explicitly preserved; resumed wall includes resumed recovery and remaining dispatches.'})
    resume_start=time.monotonic()
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
        for index,(case,request) in enumerate(zip(plan['commands'],requests)):
            case_start=time.monotonic()
            label=case['label']; python=case['argv'][0]
            if index == 0:
                controller=checked(first_controller_pin); report=checked(first_report_pin)
                require(COMMON.is_dir(),'completed first native root absent')
            else:
                require(not COMMON.exists(),'prior native root remains')
                run(label,'dispatch',case['argv'])
                controller=checked(pin(ROOT/'controllers'/(label+'.json'))); report=checked(controller['report'])
            require(controller['complete'] and report['complete'] and report['completed_updates']==162 and report['completed_games']==1620,'incomplete full block')
            if index == 0:
                state['canonical_fingerprint'] = report['fingerprint']; save()
            else:
                require(report['fingerprint'] == state['canonical_fingerprint'], 'formal full fingerprint differs')
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
            state['completed'].append(dict(label=label,report=controller['report'],inspect=pin(maintenance/'inspect.json'),transport=receipt,retain=pin(maintenance/'retain.json'),late_copy=pin(latepath),late_receipt_metadata_seconds=time.monotonic()-metadata_start,late_copy_cold_receipt=pin(latecold),coordinator_case_wall_seconds=time.monotonic()-case_start,coordinator_case_wall_scope=('resumed first maintenance only; prior dispatch separately pinned' if index==0 else 'full dispatch through late receipt readback'))); save()
        state.update(complete=True,finished_utc=now(),resumed_coordinator_wall_seconds=time.monotonic()-resume_start); save()
        finalcold=Path('E:/training-speedups-20261009')/'formal-v2-resume-final-state.json'
        require(not finalcold.exists(),'final metadata destination exists')
        copy_recovery.cold_copy(statepath,finalcold)
        require(pin(finalcold)['sha256']==pin(statepath)['sha256'],'final state readback differs')
    except BaseException as error:
        state.update(resumed_coordinator_wall_seconds=time.monotonic()-resume_start,complete=False,error=type(error).__name__+': '+str(error),finished_utc=now()); save(); raise

if __name__=='__main__': main()
