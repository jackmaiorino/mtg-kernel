"""Continue the exact terminal v1 continuation prefix in a second fresh root.

Only cases three and four execute, through their unchanged public drivers.
Frozen sources, requests, prefix evidence and all case timing statements remain
unchanged. No acquisition, exemptions, request edits or automatic retry.
"""
import argparse
import copy
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
PRIOR_STATE = ROOT / 'formal/continuation-coordinator/state.json'
PRIOR_SHA = '46b738146fe7f1d2bb4626b5b1b6c5d39bc8e9762e1896f310b01505f3eedef8'
SEAL_SHA = '4abe40675ad00f6609791ca245d4979f0d0201f43d5593b9e3a229b32126ad0b'
PRIOR_PID = 35252
PRIOR_CREATION = 134361816044058689
TERMINAL_ERROR = 'ValueError: unknown canonical reservation state'
LABELS = ['io-01-baseline', 'io-02-candidate', 'io-03-candidate', 'io-04-baseline']
PREFIX_FIELDS = ('completed', 'phases', 'queue_waits', 'canonical_fingerprint',
                 'maintenance_parity', 'initial_allocation_measurement', 'first_block_reconciliation')
ADMISSION_SECONDS = 6 * 3600
V1_SOURCE_SHA = '508db453bc76771b2471e124189d627d9356ee4b2039d8edb97ca447f321b034'
ORIGINAL_PREFIX_SHA = 'f538cfdd00b665e4d92b5863f8b470ef8d8d083378b442c83d7b11207a2bf614'

def require(ok, message):
    if not ok: raise ValueError(message)

def pin(path):
    spelling = str(path)
    with Path(path).open('rb') as f: h = hashlib.file_digest(f, 'sha256').hexdigest()
    return {'path': spelling, 'sha256': h}

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


def preserved_prefix_digest(state):
    payload = {key: state[key] for key in PREFIX_FIELDS}
    return hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()).hexdigest()


def verify_prior_continuation(prior):
    """Bind the preserved v1 segment to the original terminal prefix."""
    previous = prior['continuation']
    require(previous['schema'] == 'training-io-formal-continuation/v1'
            and previous['implementation']['sha256'] == V1_SOURCE_SHA
            and Path(previous['implementation']['path']) == ROOT / 'run_comparison_continuation.py'
            and previous['implementation'] in prior['sources']
            and previous['prefix_state']['sha256'] == ORIGINAL_PREFIX_SHA
            and Path(previous['prefix_state']['path']) == ROOT / 'formal/coordinator/state.json'
            and 'finished_utc' not in previous, 'exact interrupted v1 continuation required')
    original = checked(previous['prefix_state'])
    require(original['complete'] is False
            and original['error'] == 'ValueError: formal admission deadline reached; no further launch'
            and prior['plan'] == original['plan'] and prior['started_utc'] == original['started_utc']
            and all(prior[key] == original[key] for key in PREFIX_FIELDS)
            and previous['preserved_prefix_sha256'] == preserved_prefix_digest(original)
            and prior['sources'][:len(original['sources'])] == original['sources']
            and len(prior['sources']) == 1328, 'v1 changed the preserved original prefix')
    for key in ('prefix_state', 'implementation', 'prefix_recovery', 'inventory', 'prior_job', 'prior_stderr'):
        require(pin(previous[key]['path']) == previous[key] and previous[key] in prior['sources'],
                'v1 provenance changed: ' + key)
    require(datetime.fromisoformat(prior['finished_utc']) >= datetime.fromisoformat(previous['resumed_utc']),
            'v1 terminal clock precedes admission segment')
    return copy.deepcopy(previous)


def verify_prefix(prior, plan_pin, plan, reservations):
    """Check the exact terminal prefix and durable bindings without execution."""
    require(prior['schema'] == 'training-speedup-desktop-formal-coordinator/v1'
            and prior['complete'] is False and prior['error'] == TERMINAL_ERROR
            and prior['plan'] == plan_pin and prior['sequential_nonoverlapping'] is True,
            'exact terminal formal state required')
    require([entry['label'] for entry in prior['completed']] == LABELS[:2]
            and [(phase['case'], phase['phase']) for phase in prior['phases']]
                == [(label, phase) for label in LABELS[:2] for phase in ('dispatch', 'inspect', 'copy', 'retain')]
            and all(phase['exit_code'] == 0 for phase in prior['phases'])
            and [wait['case'] for wait in prior['queue_waits']] == LABELS[:2]
            and prior['queue_waiting']['case'] == LABELS[2], 'exact completed two-case prefix required')
    process = {'pid': PRIOR_PID, 'creation_time': PRIOR_CREATION,
               'state': reservations.process_state(PRIOR_PID, PRIOR_CREATION), 'observed_utc': now()}
    require(process['state'] == 'absent', 'prior formal process is live or unprovable')
    refs = {}
    def add(ref):
        require(pin(ref['path']) == ref, 'preserved prefix receipt changed: ' + ref['path'])
        refs[(ref['path'], ref['sha256'])] = ref
    for ref in prior['sources']:
        add(ref)
    add(prior['initial_allocation_measurement'])
    add(prior['first_block_reconciliation']['measurement'])
    for entry, command in zip(prior['completed'], plan['commands'][:2]):
        label = entry['label']
        for key in ('report', 'inspect', 'transport', 'retain', 'late_copy', 'late_copy_cold_receipt'):
            add(entry[key])
        controller_ref = pin(ROOT / 'controllers' / (label + '.json'))
        add(controller_ref)
        controller = checked(controller_ref)
        report = checked(entry['report']); inspection = checked(entry['inspect'])
        transport = checked(entry['transport']); retention = checked(entry['retain']); late = checked(entry['late_copy'])
        require(controller['complete'] is True and controller['request'] == command['request']
                and controller['report'] == entry['report'] and report['complete'] is True
                and not report['qualification'] and report['request'] == command['request']
                and report['completed_updates'] == 162 and report['completed_games'] == 1620
                and report['fingerprint'] == prior['canonical_fingerprint'], 'preserved full-case binding differs')
        affinity = controller['native_affinity_observation']
        require(affinity['verified'] is True and affinity['actual_mask'] == affinity['expected_mask'] == 21845,
                'preserved actual affinity differs')
        require(inspection['complete'] is True and inspection['request'] == command['request']
                and inspection['report'] == entry['report'] and transport['complete'] is True
                and retention['complete'] is True and retention['request'] == command['request']
                and retention['inspection'] == entry['inspect'] and retention['independent_cold_copy'] == entry['transport']
                and late['complete'] is True and late['case'] == label and late['request'] == command['request']
                and late['report'] == entry['report'] and late['retain'] == entry['retain']
                and entry['late_copy']['sha256'] == entry['late_copy_cold_receipt']['sha256'],
                'preserved maintenance/recovery receipt binding differs')
        for value in controller.values():
            if isinstance(value, dict) and 'path' in value and 'sha256' in value:
                add(value)
        for value in inspection.values():
            if isinstance(value, dict) and 'path' in value and 'sha256' in value:
                add(value)
        for value in retention.values():
            if isinstance(value, dict) and 'path' in value and 'sha256' in value:
                add(value)
        # run/exposure sidecar paths belong to the successfully pruned common
        # output. Their content is preserved in the pinned archive descriptor;
        # full archive/member validation remains the analyzer's existing duty.
        for key, value in report.items():
            if isinstance(value, dict) and 'path' in value and 'sha256' in value:
                if key in ('run', 'exposure_inputs'):
                    expected = COMMON / ('run.json' if key == 'run' else 'exposure-inputs.json')
                    require(Path(value['path']) == expected, 'preserved pruned output path differs')
                else:
                    add(value)
        add(report['runtime']['binary']); add(transport['plan'])
        execution = checked(report['execution'])
        require(execution['exit_code'] == 0 and execution['error'] is None, 'preserved native execution failed')
        exposure = checked(inspection['exposure'])
        rows = exposure['rows']; add({'path': rows['path'], 'sha256': rows['sha256']})
        exposure['rows'] = {key: value for key, value in rows.items() if key != 'path'}
        require({'exposure': exposure, 'embedding_gate': checked(inspection['embedding_gate'])}
                == prior['maintenance_parity'], 'preserved maintenance parity differs')
        audit = prior['first_block_reconciliation']['seconds'] if label == LABELS[0] else 0.0
        require(entry['excluded_one_time_allocation_audit_seconds'] == audit
                and abs(entry['coordinator_case_wall_seconds'] - entry['matched_case_wall_seconds'] - audit) < 1e-6,
                'preserved individual case wall differs')
    return process, list(refs.values())


def absent_remaining_outputs(plan):
    require(not COMMON.exists(), 'preserve existing common output')
    for case in plan['commands'][2:]:
        request = checked(case['request']); label = case['label']
        paths = [ROOT / 'controllers' / (label + '.json'), Path(request['root']), Path(request['cold_root']),
                 ROOT / 'measure/maintenance' / label, Path('E:/training-io-speedups-20261010') / label]
        require(all(not path.exists() for path in paths), 'preserve existing remaining-case outputs: ' + label)
        for directory in ('coordinator', 'continuation-coordinator'):
            require(not list((ROOT / 'formal' / directory).glob(label + '.*')), 'prior remaining-case phase output exists')
    require(not (Path('E:/training-io-speedups-20261010') / 'formal-final-state.json').exists(),
            'preserve existing final independent state')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--plan', type=Path, default=ROOT/'formal/seal.plan.json')
    ap.add_argument('--decks', type=Path, required=True)
    ap.add_argument('--t1', type=Path, required=True)
    ap.add_argument('--execute', action='store_true')
    ap.add_argument('--prefix-recovery', type=Path, required=True)
    ap.add_argument('--inventory', type=Path, required=True)
    args = ap.parse_args()
    plan_pin = pin(args.plan); plan = checked(plan_pin)
    require(args.plan == ROOT / 'formal/seal.plan.json' and plan_pin['sha256'] == SEAL_SHA, 'exact original sealed plan required')
    prior_ref = {'path': str(PRIOR_STATE), 'sha256': PRIOR_SHA}
    prior = checked(prior_ref)
    previous_continuation = verify_prior_continuation(prior)
    require(args.prefix_recovery.is_absolute() and args.prefix_recovery.is_relative_to(Path('E:/training-io-speedups-20261010'))
            and args.inventory.is_absolute() and args.inventory.is_relative_to(BASE), 'owned D/E continuation provenance paths required')
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
    process, prefix_receipts = verify_prefix(prior, plan_pin, plan, reservations)
    cold_prefix = {'path': str(args.prefix_recovery / 'state.json'), 'sha256': PRIOR_SHA}
    checked(cold_prefix)
    inventory_ref = pin(args.inventory)
    inventory = checked(inventory_ref)
    require(set(inventory) == {'desktop', 'computehost', 'runpod'}, 'three-host continuation inventory required')
    prior_job_ref = pin(args.prefix_recovery / 'formal-continuation-job.json')
    prior_job = checked(prior_job_ref)
    require(prior_job['pid'] == PRIOR_PID and prior_job['creation_filetime'] == PRIOR_CREATION
            and prior_job['plan_sha256'] == SEAL_SHA and Path(prior_job['state']) == PRIOR_STATE,
            'preserved prior job identity differs')
    prior_stderr = pin(args.prefix_recovery / 'formal-continuation-launcher.stderr')
    wrapper_ref = next(ref for ref in prior['sources'] if Path(ref['path']) == ROOT / 'run_comparison_retry.py')
    require(pin(wrapper_ref['path']) == wrapper_ref, 'frozen retry wrapper changed')
    require(Path(prior_job['script']) == Path(previous_continuation['implementation']['path'])
            and prior_job['source_sha256'] == previous_continuation['implementation']['sha256']
            and prior_job['prefix_state_sha256'] == ORIGINAL_PREFIX_SHA
            and Path(prior_job['plan']) == args.plan, 'preserved v1 job source/seal binding differs')
    spec = importlib.util.spec_from_file_location('continuation_frozen_retry', wrapper_ref['path'])
    wrapper = importlib.util.module_from_spec(spec); spec.loader.exec_module(wrapper)
    extra, runner_ref = wrapper.retry_sources(plan)
    sources += extra
    admission = wrapper.load('continuation_frozen_admission', runner_ref)
    watcher_ref = next(ref for ref in plan['supporting_helpers'] if Path(ref['path']) == BASE / 'resume_qualifications.py')
    watcher = wrapper.load('continuation_frozen_watcher', watcher_ref).DirectoryChangeWait
    remaining_wait = wrapper.admission_adapter({**plan, 'commands': plan['commands'][2:]}, admission, watcher)
    wait_canonical_free.wait_free = lambda changed=None: remaining_wait(changed, time.monotonic() + ADMISSION_SECONDS)
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
    absent_remaining_outputs(plan)
    out = args.plan.parent/'continuation-coordinator-v2'
    for ref in sources: require(pin(ref['path']) == ref, 'source/input changed before continuation: ' + ref['path'])
    own_ref = pin(Path(__file__))
    require(all(ref in prior['sources'] or ref == own_ref for ref in sources),
            'continuation preflight differs from frozen source graph')
    # Keep the original stable() source list including its order/duplicates.
    # Prefix output receipts are checked only in preflight, outside case clocks.
    sources = prior['sources'] + [own_ref, prior_ref, cold_prefix, inventory_ref, prior_job_ref, prior_stderr]
    require(not out.exists(), 'coordinator attempt exists; preserve it, no automatic resume')
    if not args.execute:
        print(json.dumps({'ready':True,'execution':False,'plan':plan_pin,'sources':sources,'command_requires':'--execute after parent review'})); return
    out.mkdir()
    resumed = now()
    state = copy.deepcopy(prior)
    for key in ('error', 'finished_utc', 'queue_waiting'):
        state.pop(key, None)
    state['sources'] = sources
    state['continuation'] = {
        'schema': 'training-io-formal-continuation/v2', 'prefix_state': prior_ref,
        'prior_continuation': previous_continuation,
        'implementation': own_ref, 'prior_process': process, 'prefix_recovery': cold_prefix,
        'inventory': inventory_ref, 'prior_job': prior_job_ref, 'prior_stderr': prior_stderr,
        'prefix_started_utc': prior['started_utc'], 'prefix_finished_utc': prior['finished_utc'], 'resumed_utc': resumed,
        'interruption_gap_seconds': (datetime.fromisoformat(resumed) - datetime.fromisoformat(prior['finished_utc'])).total_seconds(),
        'prior_error': prior['error'], 'prefix_completed_labels': LABELS[:2], 'remaining_labels': LABELS[2:],
        'prior_queue_waits': copy.deepcopy(prior['queue_waits']), 'prior_queue_waiting': copy.deepcopy(prior['queue_waiting']),
        'interrupted_interval_started_utc': previous_continuation['resumed_utc'],
        'interrupted_between_case_utc_interval_seconds': (datetime.fromisoformat(prior['finished_utc']) - datetime.fromisoformat(previous_continuation['resumed_utc'])).total_seconds(),
        'interrupted_admission_limit_seconds': previous_continuation['admission_wait_limit_seconds'], 'admission_wait_limit_seconds': ADMISSION_SECONDS,
        'prefix_receipt_sources': prefix_receipts, 'preserved_prefix_sha256': preserved_prefix_digest(prior),
        'source_check_accounting': 'Original ordered source list, including duplicate checks, retained. Only continuation implementation and small immutable prefix/inventory/job evidence pins added to stable checks; completed output receipts verified outside measured case clocks.',
        'scope': 'Interrupted ABBA spanning two admission failures and a long between-pair gap. Cache, storage and background conditions may differ across sessions. Only the observed combined-package matched ratio is supported; no uninterrupted-isolation or individual-patch causal claim.',
        'accounting': 'Original start UTC retained. Successful queue waits and every individual raw/matched case wall preserved. Newest interrupted UTC interval runs from v1 resumed UTC to its terminal UTC and includes state/stable overhead, not measured admission time. The exact nested prior_continuation retains the earlier interrupted interval and gap without recounting them here. New terminal-to-resumed UTC gap is separate; all remain in whole coordinator elapsed and outside measured case walls. New six-hour advisory bound applies independently to each remaining admission; public dispatch remains final.'}
    require(state['continuation']['interruption_gap_seconds'] >= 0, 'continuation clock precedes terminal prefix')
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
        for index,(case,request) in enumerate(zip(plan['commands'][2:],requests[2:]), start=2):
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
        finished = now()
        state['continuation']['finished_utc'] = finished
        state.update(complete=True,finished_utc=finished); save()
        finalcold=Path('E:/training-io-speedups-20261010')/'formal-final-state.json'
        require(not finalcold.exists(),'final metadata destination exists')
        copy_recovery.cold_copy(statepath,finalcold)
        require(pin(finalcold)['sha256']==pin(statepath)['sha256'],'final state readback differs')
    except BaseException as error:
        state.update(complete=False,error=type(error).__name__+': '+str(error),finished_utc=now()); save(); raise

if __name__=='__main__': main()
