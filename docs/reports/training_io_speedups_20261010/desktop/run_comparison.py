"""Sequential sealed desktop ABBA orchestration. Execution requires explicit --execute.
Stops permanently on failure; no retries, request edits, or reservation manipulation.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

BASE = Path('D:/training-io-speedups-20261010')
ROOT = BASE / 'desktop'
HELPERS = BASE / 'helpers'
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

def reservation_observation(reservations):
    """Observe canonical generations without acquiring, reclaiming or polling."""
    lock = reservations.lock_path()
    pattern = re.compile(re.escape(lock.stem) + r'\.g([0-9]{8})\.[0-9a-f]{32}\.')
    generations = [int(match.group(1)) for path in lock.parent.iterdir()
                   if (match := pattern.match(path.name))]
    try:
        current = read(lock)
    except FileNotFoundError:
        current = None
    if current is not None:
        generations.append(current['generation'])
        current = {key: current[key] for key in ('generation', 'lane', 'work_id', 'acquired_at')}
    return {'observed_utc': now(), 'current': current,
            'highest_generation': max(generations, default=0)}

def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--plan', type=Path, default=ROOT/'formal/seal.plan.json')
    ap.add_argument('--decks', type=Path, required=True)
    ap.add_argument('--t1', type=Path, required=True)
    ap.add_argument('--execute', action='store_true')
    args = ap.parse_args()
    plan_pin = pin(args.plan); plan = checked(plan_pin)
    require(pin(sys.executable)==plan['python_pin'],'pinned coordinator Python required')
    require(plan['stage'] == 'seal' and len(plan['commands']) == 4, 'sealed four-case plan required')
    require([x['label'] for x in plan['commands']] == ['io-01-baseline','io-02-candidate','io-03-candidate','io-04-baseline'], 'ABBA order differs')
    for name in ('controller','adapter'): checked(plan[name]) if plan[name]['path'].endswith('.json') else require(pin(plan[name]['path']) == plan[name], 'helper changed')
    post = HELPERS/'postprocess_case.py'; copier = HELPERS/'copy_recovery.py'
    allocation = BASE/'measure_allocations.py'
    sources = [pin(x) for x in (Path(__file__), post, copier, HELPERS/'desktop_devices.py', allocation, args.decks, args.t1)]
    sources += [plan['controller'], plan['adapter'], plan_pin,pin(ROOT/'observe_native_affinity.py'),pin(ROOT/'wait_canonical_free.py'),pin(BASE/'resume_qualifications.py')]
    sources += list(plan.get('helpers',{}).values())
    sources += [ref for refs in plan.get('tools',{}).values() for ref in refs]
    sources += plan['supporting_helpers']
    for ref in plan['supporting_helpers']:
        require(pin(ref['path'])==ref,'sealed supporting helper changed')
    sys.path.insert(0,str(ROOT))
    import wait_canonical_free
    reservations = wait_canonical_free.reservation_api()
    dependencies=BASE/'desktop/maintenance-dependencies-20261010.json'
    dependency_records=read(dependencies)
    sources.append(pin(dependencies))
    for record in dependency_records:
        ref={'path':record['path'],'sha256':record['sha256']}
        require(pin(ref['path'])==ref,'staged maintenance dependency changed')
        sources.append(ref)
    frozen_inputs=read(ROOT/'qualification.plan.json')['input_dependencies']
    require(len(frozen_inputs)==8,'eight frozen input pins required')
    for record in frozen_inputs:
        require(pin(record['original']['path'])['sha256']==record['original']['sha256'],'frozen source input changed')
        sources.append(pin(record['original']['path']))
    # Exercise the real maintenance loader before any guarded dispatch.
    sys.path.insert(0,str(ROOT/'baseline/python/tools'))
    import nine_deck_baseline_v1
    require(len(nine_deck_baseline_v1.load_decks(args.decks))==9,'nine registered decks required')
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
        require(request['placement']['workers'] == config['collection_workers'] == config['preparation_workers'], 'frozen selected workers changed')
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
        state['initial_allocation_measurement']=pin(out/'before.allocation.json'); save()
        for index,(case,request) in enumerate(zip(plan['commands'],requests)):
            stable()
            def queue_changed(value):
                state['queue_waiting']={'case':case['label'],**value};save()
            queue=wait_canonical_free.wait_free(queue_changed)
            state.setdefault('queue_waits',[]).append({'case':case['label'],**queue});state.pop('queue_waiting',None);save()
            case_start=time.monotonic()
            case_started_utc=now()
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
            maintenance_started_utc=now()
            reservation_before=reservation_observation(reservations)
            common=[python,'-B',str(post),'--launcher-root',str(ROOT/label.split('-')[-1]),'--comparison-root',str(ROOT),'--request',case['request']['path'],'--decks',str(args.decks),'--t1',str(args.t1),'--out',str(maintenance),'--recovery-mode','desktop-local']
            run(label,'inspect',common[:3]+['inspect']+common[3:])
            inspection=read(maintenance/'inspect.json'); recovery=inspection['recovery_copy_plan']; checked(recovery)
            exposure=checked(inspection['exposure'])
            # Only the case-specific output filename differs; rows are exact bytes.
            rows=exposure['rows']; checked_rows={'path':rows['path'],'sha256':rows['sha256']}
            require(pin(rows['path'])==checked_rows,'exposure rows changed')
            exposure['rows']={key:value for key,value in rows.items() if key!='path'}
            parity={'exposure':exposure,'embedding_gate':checked(inspection['embedding_gate'])}
            if index==0: state['maintenance_parity']=parity;save()
            else: require(parity==state['maintenance_parity'],'exposure or embedding outputs differ')
            run(label,'copy',[python,'-B',str(copier),'--mode','desktop-local','--case-label',label,'--local-plan',recovery['path'],'--plan-sha256',recovery['sha256']])
            receipt=pin(maintenance/'cold-copy-receipt.json'); require(checked(receipt)['complete'],'copy incomplete')
            run(label,'retain',common[:3]+['retain']+common[3:]+['--cold-copy-receipt',receipt['path'],'--cold-copy-sha256',receipt['sha256']])
            retention=read(maintenance/'retain.json'); require(retention['complete'] and not COMMON.exists(),'retention did not free common path')
            # Copy late inspection/retention metadata after its creation, with fsync and full readback.
            stable(); start=time.monotonic(); late=[]
            sys.path.insert(0,str(HELPERS)); import copy_recovery
            cold=Path('E:/training-io-speedups-20261010')/label/'late'
            require(not cold.exists(),'late metadata destination exists')
            import desktop_devices
            recovery_plan=checked(recovery)
            desktop_devices.verify(ROOT,cold,recovery_plan['source_device'],recovery_plan['destination_device'])
            inputs=[ROOT/'controllers'/(label+'.json')]+list(maintenance.rglob('*'))
            inputs=[p for p in inputs if p.is_file()]
            require(shutil.disk_usage('E:/').free >= 60*GIB+sum(p.stat().st_size for p in inputs),'late copy reserve insufficient')
            variant=label.split('-')[-1]
            mode='individual-files' if variant=='baseline' else 'store-bundle'
            bundle_receipt=None
            if variant=='candidate':
                sys.path.insert(0,str(ROOT/'candidate/python/tools'))
                import training_recovery_bundle_v1 as bundles
                bundle_receipt=bundles.create_bundle(ROOT,inputs,cold/'payload.zip',reserve_bytes=60*GIB)
                require(bundle_receipt['complete'],'bundle recovery incomplete')
                late=[dict(source={'path':str(ROOT/item['relative_path']),'sha256':item['sha256']},relative_path=item['relative_path'],bytes=item['bytes']) for item in bundle_receipt['inventory']]
            else:
                for source in inputs:
                    copy_recovery.safe_local(source)
                    destination=cold/source.relative_to(ROOT)
                    expected=pin(source); copy_recovery.cold_copy(source,destination)
                    actual=pin(destination); require(actual['sha256']==expected['sha256'],'late metadata readback differs')
                    late.append(dict(source=expected,destination=actual,relative_path=source.relative_to(ROOT).as_posix(),bytes=source.stat().st_size))
            desktop_devices.verify(ROOT,cold,recovery_plan['source_device'],recovery_plan['destination_device'])
            late_receipt=dict(schema='training-io-speedup-late-copy/v1',complete=True,mode=mode,case=label,request=case['request'],report=controller['report'],retain=pin(maintenance/'retain.json'),files=late,bundle=bundle_receipt,source_device=recovery_plan['source_device'],destination_device=recovery_plan['destination_device'],seconds=time.monotonic()-start,finished_utc=now(),accounting='Complete payload copy, fsync and independent destination container/member SHA readback included; receipt self-copy is separate metadata envelope overhead included in full case wall.')
            latepath=out/(label+'.late-copy.json'); latepath.write_text(json.dumps(late_receipt,indent=2),encoding='utf-8')
            metadata_start=time.monotonic()
            latecold=cold/'late-copy-receipt.json'
            copy_recovery.cold_copy(latepath,latecold)
            require(pin(latecold)['sha256']==pin(latepath)['sha256'],'late copy receipt readback differs')
            metadata_seconds=time.monotonic()-metadata_start
            reservation_after=reservation_observation(reservations)
            dispatch_generation=controller['dispatch']['generation']
            observations={'before':reservation_before,'after':reservation_after,
                          'dispatch_generation':dispatch_generation,
                          'additional_generations_observed':any(x['highest_generation']>dispatch_generation for x in (reservation_before,reservation_after)),
                          'scope':'Canonical reservation generations only. Includes short completed reservations through archived filenames. Does not rule out shared-slot or unreserved background work; does not reject or exclude a case.'}
            completed=dict(label=label,report=controller['report'],inspect=pin(maintenance/'inspect.json'),transport=receipt,retain=pin(maintenance/'retain.json'),late_copy=pin(latepath),late_receipt_metadata_seconds=metadata_seconds,late_copy_cold_receipt=pin(latecold),started_utc=case_started_utc,maintenance_started_utc=maintenance_started_utc,maintenance_reservation_observations=observations)
            wall=time.monotonic()-case_start
            audit=state['first_block_reconciliation']['seconds'] if index==0 else 0.0
            completed.update(coordinator_case_wall_seconds=wall,matched_case_wall_seconds=wall-audit,excluded_one_time_allocation_audit_seconds=audit,finished_utc=now(),timing_boundary='Through final recovery receipt SHA reads and canonical generation observation; subsequent durable coordinator-state write is outside case wall and included in coordinator elapsed.')
            state['completed'].append(completed); save()
        state.update(complete=True,finished_utc=now()); save()
        finalcold=Path('E:/training-io-speedups-20261010')/'formal-final-state.json'
        require(not finalcold.exists(),'final metadata destination exists')
        copy_recovery.cold_copy(statepath,finalcold)
        require(pin(finalcold)['sha256']==pin(statepath)['sha256'],'final state readback differs')
    except BaseException as error:
        state.update(complete=False,error=type(error).__name__+': '+str(error),finished_utc=now()); save(); raise

if __name__=='__main__': main()
