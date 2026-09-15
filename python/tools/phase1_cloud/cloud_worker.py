"""Linux-only co-located native trainer, durable checkpoint export and telemetry.

No work on import. Root explicitly invokes stage, run, or restore on a Pod.
The independently booted lease guard owns provider termination, not this process.
"""
from __future__ import annotations
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time
import uuid
from common import pin, read, relative, require, sha, write
from throughput import (cgroup_limits, finite, production_reference, ThroughputPolicy,
                        preparation_workers, validate_preparation_update)
from runtime_observation import controlled_environment, image_contract, sample as runtime_sample, capture as capture_runtime

GIB = 1024 ** 3
COLLECTION_TIMINGS = ('collection_elapsed_seconds', 'collection_initialization_seconds')
UPDATE_TIMINGS = ('update_elapsed_seconds', 'input_read_seconds', 'behavior_replay_seconds',
                  'learner_update_seconds', 'checkpoint_io_seconds')


def receipt_timings(hot, receipt):
    """Use explicit native stage fields; older missing instrumentation is null."""
    hot = Path(hot).resolve()
    result = {'iteration': receipt.get('iteration'), 'collection': {}, 'update': {}}
    for kind, fields in (('collection', COLLECTION_TIMINGS), ('update', UPDATE_TIMINGS)):
        document = {}
        reference = receipt.get(kind)
        if reference:
            path = Path(reference['path']).resolve()
            require(path.is_relative_to(hot / 'run'), 'native stage receipt is outside the run')
            pin(path, reference['sha256']); document = read(path)
        for key in fields:
            value = document.get(key)
            require(value is None or finite(value), 'invalid native timing field')
            result[kind][key] = value
        if kind=='update' and 'update_preparation' in document:
            row=document['update_preparation']
            require(isinstance(row,dict),'invalid completed update preparation telemetry')
            validate_preparation_update({'preparation_workers':row.get('requested_workers')},document)
            for key,value in row.items():
                if key.endswith('_seconds'):
                    require(value is None or finite(value),'invalid native preparation timing field')
            for worker in row.get('worker_timings',[]):
                require(isinstance(worker,dict) and finite(worker.get('busy_wall_seconds')),
                        'invalid native preparation worker timing')
            result['update_preparation']=row
    result['update_elapsed_excludes_final_receipt_publication'] = True
    return result


def stage(archive, expected_sha, root):
    pin(archive, expected_sha)
    root = Path(root)
    require(not root.exists(), 'fresh local hot-storage root required')
    with tarfile.open(archive, 'r:') as handle:
        members = handle.getmembers()
        names = [str(relative(member.name)) for member in members]
        require(len(names) == len(set(names)) and all(m.isfile() for m in members),
                'archive must contain unique regular files only')
        require('manifest.json' in names, 'manifest missing')
        manifest = json.load(handle.extractfile(handle.getmember('manifest.json')))
        require(set(names) == set(manifest['files']) | {'manifest.json'}, 'inventory differs')
        require(manifest['remote_root'] == str(root), 'hot root differs from manifest')
        root.mkdir(parents=True)
        for member in members:
            target = root / member.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as dest, handle.extractfile(member) as source:
                shutil.copyfileobj(source, dest, 1048576)
                dest.flush(); os.fsync(dest.fileno())
            if member.name != 'manifest.json':
                row = manifest['files'][member.name]
                require(sha(target) == row['sha256'] and target.stat().st_size == row['bytes'],
                        'staged file hash differs')
                target.chmod(row['mode'])
    return {'manifest': pin(root / 'manifest.json'), 'file_count': len(names)}


def copy_exact(source, target):
    source = Path(source); target = Path(target)
    require(source.is_file() and not source.is_symlink(), 'nonregular recovery source')
    if target.exists():
        require(sha(source) == sha(target), 'existing durable bytes differ')
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(target.name + '.' + uuid.uuid4().hex + '.partial')
    with source.open('rb') as inp, temporary.open('xb') as out:
        shutil.copyfileobj(inp, out, 1048576); out.flush(); os.fsync(out.fileno())
    require(sha(source) == sha(temporary), 'recovery copy hash differs')
    os.replace(temporary, target)


def recovery_backlog(hot, durable):
    """Count distinct committed iterations absent from the durable prefix."""
    source=Path(hot)/'run/iterations';target=Path(durable)/'run/iterations'
    hot_count=durable_count=pending_bytes=0
    for directory in sorted(source.glob('*')) if source.exists() else []:
        receipt=directory/'complete.json'
        if not receipt.is_file():break
        require(directory.name==f'{hot_count:06d}','hot recovery prefix has a gap')
        hot_count+=1
        published=target/directory.name/'complete.json'
        if published.exists():
            require(durable_count==hot_count-1 and sha(receipt)==sha(published),
                    'durable recovery prefix differs')
            durable_count+=1
        else:
            for file in directory.rglob('*'):
                require(not file.is_symlink(),'recovery backlog contains symlink')
                if file.is_file():pending_bytes+=file.stat().st_size
    return {'hot_completed_iterations':hot_count,'durable_completed_iterations':durable_count,
            'pending_completed_iterations':hot_count-durable_count,
            'pending_completed_iteration_bytes':pending_bytes,
            'current_in_progress_iteration_excluded':True}


def export_completed(hot, durable):
    """Copy only immutable completed-iteration trees, receipt published last.

    Full trajectories are retained so native resume validation can reuse a
    complete prefix; exporting only checkpoint.json would be insufficient.
    """
    hot = Path(hot); durable = Path(durable)
    source = hot / 'run'; target = durable / 'run'
    target.mkdir(parents=True, exist_ok=True)
    if (source / 'run.json').is_file():
        copy_exact(source / 'run.json', target / 'run.json')
    count = 0; output_bytes = 0
    for directory in sorted((source / 'iterations').glob('*')) if (source / 'iterations').exists() else []:
        receipt = directory / 'complete.json'
        if not receipt.is_file():
            break
        require(directory.name == f'{count:06d}', 'completed iterations are not a contiguous prefix')
        published = target / receipt.relative_to(source)
        if published.exists():
            require(sha(receipt) == sha(published), 'completed durable receipt changed')
            count += 1
            continue
        for file in sorted(directory.rglob('*')):
            require(not file.is_symlink(), 'recovery tree contains symlink')
            if file.is_file() and file != receipt:
                copy_exact(file, target / file.relative_to(source))
                output_bytes += file.stat().st_size
        timing = receipt_timings(hot, read(receipt))
        timing_path = durable / 'timing' / (directory.name + '.json')
        if timing_path.exists():
            require(read(timing_path) == timing, 'previous derived timing differs')
        else:
            write(timing_path, timing)
        copy_exact(receipt, target / receipt.relative_to(source))
        count += 1
    if (source / 'completion.json').exists():
        completion = read(source / 'completion.json')
        require(completion['completed_iterations'] >= count, 'completion prefix differs')
        # Native can finish more work while this snapshot is copied. Publish the
        # final receipt only after every referenced iteration is durable.
        if completion['completed_iterations'] == count:
            copy_exact(source / 'completion.json', target / 'completion.json')
    status = {'completed_iterations': count, 'newly_copied_iteration_bytes': output_bytes,
              'epoch': time.time(), 'hot_root': str(hot)}
    write(durable / 'recovery.json', status, replace=True)
    return status


def export_with_backlog_limit(hot,durable,child,cap):
    before=recovery_backlog(hot,durable)
    exceeded=before['pending_completed_iterations']>cap
    if exceeded:terminate(child)
    exported=export_completed(hot,durable)
    after=recovery_backlog(hot,durable)
    if after['pending_completed_iterations']>cap:
        exceeded=True;terminate(child)
    return exported,{'before_export':before,'after_export':after,
                     'max_pending_completed_iterations':cap,'cap_exceeded':exceeded}


def restore(durable, hot):
    durable = Path(durable); hot = Path(hot)
    require(not (hot / 'run').exists(), 'restore requires a fresh run directory')
    saved = read(durable / 'recovery.json')
    require(saved['hot_root'] == str(hot), 'native receipt paths require identical hot root')
    require((hot / 'manifest.json').exists(), 'stage verified payload before restoring')
    require(sha(hot / 'manifest.json') == sha(durable / 'manifest.json'), 'package identity differs')
    for file in sorted((durable / 'run').rglob('*')):
        require(not file.is_symlink(), 'durable tree contains symlink')
        if file.is_file() and not file.name.endswith('.partial'):
            copy_exact(file, hot / 'run' / file.relative_to(durable / 'run'))
    return saved


def telemetry(pid):
    result = {'epoch': time.time(), 'pid': pid, 'affinity': sorted(os.sched_getaffinity(pid)),
              'cpu_max': None, 'memory_max': None, 'rss_bytes': None, 'cpu_seconds': None}
    result['quota'] = cgroup_limits(Path(f'/proc/{pid}/cgroup').read_text(),
        Path(f'/proc/{pid}/mountinfo').read_text(), lambda path: Path(path).read_text(), result['affinity'])
    try:
        fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
        result['cpu_seconds'] = (int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK')
        result['rss_bytes'] = int(fields[21]) * os.sysconf('SC_PAGE_SIZE')
        result['threads'] = int(fields[17])
        result['io'] = Path(f'/proc/{pid}/io').read_text()
    except FileNotFoundError:
        result['exited_during_sample'] = True
    return result


def terminate(child):
    if child.poll() is not None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        child.wait(timeout=15)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL); child.wait(timeout=10)


def phase_state(hot,config,capacity,evaluation=None):
    if evaluation:
        completed=len(list((Path(hot)/'evaluation'/evaluation).glob('match-*.json')))
        return {'phase':'evaluation','completed_games':completed,
                'algorithmically_exploitable_cpus':min(capacity,1.),
                'sequential_reason':'one packaged sequential evaluation group'}
    preparation=preparation_workers(config)
    run=Path(hot)/'run'; complete=0
    for index,item in enumerate(config['iterations']):
        directory=run/'iterations'/f'{index:06d}'
        if (directory/'complete.json').exists():
            complete+=len(item['episodes'])
            continue
        attempts=sorted(directory.glob('attempt-*'))
        if not attempts:
            return {'phase':'initialization','completed_games':complete,
                    'algorithmically_exploitable_cpus':min(capacity,1.),
                    'sequential_reason':'serial model initialization or prefix validation'}
        current=attempts[-1]
        if (current/'update-command.json').exists() or (current/'collect/collection.json').exists():
            if preparation>1:
                return {'phase':'update','subphase':'prepared_replay_pending_or_unknown',
                        'completed_games':complete,'algorithmically_exploitable_cpus':None,
                        'configured_preparation_workers':preparation,'sequential_reason':None,
                        'subphase_limit':'No durable in-progress marker; completed timings do not identify the active subphase'}
            return {'phase':'update','completed_games':complete,
                    'algorithmically_exploitable_cpus':min(capacity,1.),
                    'sequential_reason':'serial learner replay/update/checkpoint publication'}
        remaining=max(1,len(item['episodes'])-len(list((current/'collect').glob('episode-*.json'))))
        return {'phase':'collection','completed_games':complete,
                'algorithmically_exploitable_cpus':min(capacity,remaining),
                'configured_collection_workers':config.get('collection_workers',1),
                'remaining_episodes':remaining,
                'sequential_reason':'fixed batch width or batch tail' if remaining<capacity else None}
    return {'phase':'completion','completed_games':complete,
            'algorithmically_exploitable_cpus':min(capacity,1.),
            'sequential_reason':'serial final validation and publication'}


def failure_notice(durable, guard_state, pod_id, error):
    """Best effort reporting cannot prevent the outer guard from ending the lease."""
    stamp=time.time()
    failure={'pod_id':pod_id,'epoch':stamp,'failure_type':type(error).__name__,
             'recovery_complete':False,'finish_requested':True}
    for path in (Path(durable)/'supervisor-failures'/(str(int(stamp))+'-'+uuid.uuid4().hex+'.json'),
                 Path(guard_state)/'worker-failure.json'):
        try: write(path,failure,replace=True)
        except Exception: pass
    try:
        write(Path(guard_state)/'progress.json',{'pod_id':pod_id,'epoch':stamp,
            'last_productive_epoch':stamp,'last_activity_epoch':stamp,'native_alive':False,
            'queued_work':False,'finished':True,'recovery_failed':True},replace=True)
    except Exception:
        # If /run itself is unwritable, the guard's stale-controller timer remains.
        pass


def prepare_evaluation_output(hot, evaluation, config_pin):
    """Create only the verified parent required by the native create_dir call."""
    hot=Path(hot).resolve(strict=True)
    require(isinstance(evaluation,str) and evaluation and evaluation.isascii() and
            all(c.isalnum() or c in '_-' for c in evaluation),'invalid packaged evaluation name')
    pin(config_pin['path'],config_pin['sha256'])
    output=Path(read(config_pin['path'])['output_directory'])
    parent=hot/'evaluation'
    require(output==parent/evaluation,'evaluation output must remain in its exact packaged hot root')
    require(not parent.is_symlink() and parent.resolve()==parent and
            (not parent.exists() or parent.is_dir()),'evaluation parent is not an ordinary hot directory')
    require(not output.exists() and not output.is_symlink(),'evaluation output already exists')
    parent.mkdir(exist_ok=True)
    require(parent.is_dir() and not parent.is_symlink() and parent.resolve()==parent,
            'evaluation parent changed during setup')
    return output


def run(hot, durable, guard_state, max_new_iterations, finish_lease=False, evaluation=None,
        mode='qualification', proof_pin=None, local_pin=None, pod_id=None, cpu_reserve=0,
        max_pending_completed_iterations=2):
    import fcntl
    with (Path(guard_state) / 'worker.lock').open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        try:
            return run_locked(hot, durable, guard_state, max_new_iterations, finish_lease, evaluation,
                              mode, proof_pin, local_pin, pod_id, cpu_reserve,max_pending_completed_iterations)
        except BaseException as error:
            failure_notice(durable,guard_state,pod_id or os.environ.get('RUNPOD_POD_ID'),error)
            raise


def run_locked(hot, durable, guard_state, max_new_iterations, finish_lease=False, evaluation=None,
               mode='qualification', proof_pin=None, local_pin=None, pod_id=None, cpu_reserve=0,
               max_pending_completed_iterations=2):
    # This first preparation release can measure qualification fixtures only.
    # A short exact-work proof does not cover a longer curriculum, and actual
    # BO3/recovery evidence is not yet consumed by the production launcher.
    require(mode == 'qualification',
            'production execution unavailable: bounded production workload binding and '
            'actual BO3/recovery qualification must be implemented before production')
    require(os.name == 'posix' and Path('/proc').is_dir(), 'Linux runtime required')
    hot = Path(hot); durable = Path(durable); guard_state = Path(guard_state)
    pod_id = pod_id or os.environ.get('RUNPOD_POD_ID')
    status = read(guard_state / 'guard.json')
    require(status['pod_id'] == pod_id and status['provider_verified'] and status['provider_ok']
            and status.get('funds_verified') is True and status.get('allow_new_dispatch') is True
            and not status['latched'] and time.time() - status['epoch'] <= 60,
            'independent Pod guard is not freshly armed')
    os.kill(status['pid'], 0)
    manifest = read(hot / 'manifest.json')
    require(manifest['remote_root'] == str(hot), 'hot root differs')
    for name, entry in manifest['files'].items():
        pin(hot / name, entry['sha256'])
    runtime_image_pin=manifest.get('runtime_image_pins')
    require(runtime_image_pin,'packaged common runtime descriptor is required before native dispatch')
    pin(runtime_image_pin['path'],runtime_image_pin['sha256'])
    runtime_image=read(runtime_image_pin['path']);image_contract(runtime_image)
    durable.mkdir(parents=True, exist_ok=True)
    require(shutil.disk_usage(hot).free >= 16 * GIB and
            shutil.disk_usage(durable).free >= 16 * GIB, '16 GiB free-space reserve required')
    copy_exact(hot / 'manifest.json', durable / 'manifest.json')
    config = read(manifest['training_config']['path'])
    preparation=preparation_workers(config)
    completed_before=len(list((hot/'run/iterations').glob('*/complete.json'))) if evaluation is None else 0
    require(config.get('update_backend', {'kind': 'cpu'}) == {'kind': 'cpu'}, 'CPU only')
    require(mode in ('qualification','production'), 'explicit qualification or production mode required')
    require(finite(cpu_reserve), 'invalid explicit CPU reserve')
    require(type(max_pending_completed_iterations) is int and 1<=max_pending_completed_iterations<=2,
            'pending committed recovery cap must be one or two iterations')
    reference=None
    if mode=='production':
        require(evaluation is None and proof_pin is not None and local_pin is not None,
                'production training requires pinned qualification and local reference')
        reference=production_reference(proof_pin,local_pin,manifest,config.get('collection_workers',1),preparation)
    policy=ThroughputPolicy(mode,reference)
    if evaluation is None:
        require(max_new_iterations > 0, 'positive bounded iteration count required')
        argv = [manifest['binaries']['native_expanded_training_run_v1']['path'],
                manifest['training_config']['path'], '--max-new-iterations', str(max_new_iterations)]
    else:
        require(evaluation in manifest['evaluation_configs'], 'unknown packaged evaluation')
        eval_pin = manifest['evaluation_configs'][evaluation]
        argv = [manifest['binaries']['learned_sideboard_v1']['path'], '--config', eval_pin['path']]
        eval_output = prepare_evaluation_output(hot,evaluation,eval_pin)
    environment=controlled_environment(os.environ)
    binary=manifest['binaries']['native_expanded_training_run_v1' if evaluation is None else 'learned_sideboard_v1']
    runtime=None;runtime_pin=None
    start = time.time(); runtime_start=time.monotonic()
    productive = activity = start; last_count = -1; reason = None
    backlog_peak_count=backlog_peak_bytes=0;backlog_last=None
    previous_cpu=previous_io=0.
    attempt_name = str(int(start)) + '-' + uuid.uuid4().hex[:12]
    attempt = hot / 'attempts' / attempt_name; attempt.mkdir(parents=True)
    receipt_root = durable / 'attempts' / attempt_name
    write(guard_state / 'progress.json', {'pod_id': pod_id, 'epoch': start,
          'last_productive_epoch': productive, 'last_activity_epoch':activity,
          'native_alive':False,'queued_work':False,'finished': False}, replace=True)
    with (attempt / 'native.stdout.log').open('xb') as out, (attempt / 'native.stderr.log').open('xb') as err:
        def native_limits():
            import resource
            resource.setrlimit(resource.RLIMIT_AS, (8 * GIB, 8 * GIB))
        write(receipt_root / 'worker-start.json', {'schema':'phase1-cloud-worker-start/v1',
                'pid': None, 'pod_id': pod_id,
                'epoch': start, 'argv': argv, 'collection_workers': config.get('collection_workers', 1),
                'preparation_workers':preparation if evaluation is None else None,
                'mode':mode,'cpu_reserve':cpu_reserve,'qualification':proof_pin,'local_reference':local_pin,
                'runtime_image_pins':runtime_image_pin,'runtime_environment':environment,
                'max_pending_completed_iterations':max_pending_completed_iterations})
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                 env=environment, cwd=hot, start_new_session=True, preexec_fn=native_limits)
        try:
            while child.poll() is None:
                observation = telemetry(child.pid)
                if observation.get('exited_during_sample'):
                    break
                if runtime is None:
                    sampled=runtime_sample(child,binary,runtime_image,environment,
                        {'affinity':observation['affinity'],'quota':observation['quota'],
                         'source':'owned-child live /proc quota and affinity sample'})
                    if sampled is not None:
                        sampled['sample_elapsed_seconds']=time.monotonic()-runtime_start
                        runtime=capture_runtime(sampled,durable)
                        write(receipt_root/'runtime.json',runtime)
                        runtime_pin=pin(receipt_root/'runtime.json')
                capacity=observation['quota']['eligible_capacity_cpus']-cpu_reserve
                require(capacity>0,'CPU reserve consumes all quota/allowed capacity')
                native_phase=phase_state(hot,config,capacity,evaluation)
                observation['phase']=native_phase
                observation['capacity_basis']=observation['quota'].get('capacity_basis','unqualified')
                observation['occupancy_scope']='visible quota/affinity upper bound; hidden ancestors unknown'
                cpu=observation['cpu_seconds']
                io_count=sum(int(line.split(':',1)[1]) for line in observation.get('io','').splitlines()
                             if line.startswith(('read_bytes:','write_bytes:')))
                if cpu>previous_cpu or io_count>previous_io:
                    activity=time.time()
                previous_cpu=cpu;previous_io=io_count
                if observation['quota'].get('visible_hierarchy_occupancy_eligible') is True:
                    observation['policy']=policy.observe(observation['epoch'],cpu,capacity,
                        native_phase['algorithmically_exploitable_cpus'],native_phase['completed_games'],
                        True,native_phase['sequential_reason'])
                else:
                    policy=ThroughputPolicy(mode,reference)
                    observation['policy']={'action':None,'mode':mode,
                        'diagnosis':'CPU quota visibility incomplete; occupancy diagnosis unavailable'}
                exported,backlog=export_with_backlog_limit(hot,durable,child,max_pending_completed_iterations)
                before=backlog['before_export'];after=backlog['after_export'];backlog_last=after
                backlog_peak_count=max(backlog_peak_count,before['pending_completed_iterations'])
                backlog_peak_bytes=max(backlog_peak_bytes,before['pending_completed_iteration_bytes'])
                backlog_peak_count=max(backlog_peak_count,after['pending_completed_iterations'])
                backlog_peak_bytes=max(backlog_peak_bytes,after['pending_completed_iteration_bytes'])
                observation['recovery_backlog']=backlog
                with (attempt / 'telemetry.jsonl').open('a', encoding='utf-8') as log:
                    log.write(json.dumps(observation, sort_keys=True) + '\n')
                if backlog['cap_exceeded']:reason='recovery_backlog_cap'
                completed = len(list(eval_output.glob('match-*.json'))) if evaluation else exported['completed_iterations']
                if completed > last_count:
                    productive = time.time(); last_count = completed
                write(guard_state / 'progress.json', {'pod_id': pod_id, 'epoch': time.time(),
                    'last_productive_epoch': productive, 'completed_work_items': last_count,
                    'last_activity_epoch':activity,'native_alive':True,'queued_work':True,
                    'finished': False}, replace=True)
                current_guard = read(guard_state / 'guard.json')
                if reason:
                    break
                if time.time() - current_guard['epoch'] > 90:
                    reason = 'guard_stale'
                elif (guard_state / 'stop-request.json').exists():
                    reason = 'guard_stop'
                elif observation.get('rss_bytes', 0) and observation['rss_bytes'] > 8 * GIB:
                    reason = 'process_memory_cap'
                elif min(shutil.disk_usage(hot).free, shutil.disk_usage(durable).free) < 16 * GIB:
                    reason = 'storage_reserve'
                elif observation['policy']['action']:
                    reason=observation['policy']['action']
                if reason:
                    break
                time.sleep(.05 if runtime is None else 5)
        except BaseException:
            reason=reason or 'worker_exception'
            raise
        finally:
            terminate(child)
            if runtime_pin is None:reason=reason or 'runtime_unverified'
            native_elapsed=time.monotonic()-runtime_start
            before_final=recovery_backlog(hot,durable)
            backlog_peak_count=max(backlog_peak_count,before_final['pending_completed_iterations'])
            backlog_peak_bytes=max(backlog_peak_bytes,before_final['pending_completed_iteration_bytes'])
            if before_final['pending_completed_iterations']>max_pending_completed_iterations:
                reason=reason or 'recovery_backlog_cap'
            exported = export_completed(hot, durable)
            backlog_last=recovery_backlog(hot,durable)
            if evaluation and eval_output.exists():
                for file in sorted(eval_output.rglob('*')):
                    if file.is_file():
                        copy_exact(file, durable / 'evaluation' / evaluation / file.relative_to(eval_output))
            for name in ('native.stdout.log', 'native.stderr.log', 'telemetry.jsonl'):
                if (attempt / name).exists():
                    copy_exact(attempt / name, receipt_root / name)
            native_start=(hot/'run/run.json') if evaluation is None else eval_output/'run-start.json'
            native_completion=(hot/'run/completion.json') if evaluation is None else eval_output/'completion.json'
            write(receipt_root / 'worker-result.json', {'schema':'phase1-cloud-worker-result/v1',
                'pod_id': pod_id, 'pid':child.pid,'exit_code': child.returncode,
                'reason': reason or 'engine_exit', 'elapsed_seconds': time.time() - start,
                'native_execution_elapsed_seconds':native_elapsed,
                'measurement_scope':'worker_invocation_only_excludes_allocation_transfer_and_release',
                'recovery_backlog':{'final':backlog_last,
                    'observed_peak_pending_completed_iterations':backlog_peak_count,
                    'observed_peak_pending_completed_iteration_bytes':backlog_peak_bytes,
                    'max_pending_completed_iterations':max_pending_completed_iterations,
                    'limit_is_observed_at_poll_and_copy_boundaries':True},
                'recovered': exported, 'evaluation': evaluation, 'mode':mode,'strength_claim': False,
                'config':manifest['training_config'] if evaluation is None else manifest['evaluation_configs'][evaluation],
                'binary':manifest['binaries']['native_expanded_training_run_v1' if evaluation is None else 'learned_sideboard_v1'],
                'source_commit':manifest['source_commit'],
                'preparation_workers':preparation if evaluation is None else None,
                'runtime_verified':runtime_pin is not None,'runtime_observation':runtime_pin,
                'runtime_image_pins':runtime_image_pin,'runtime_environment':environment,
                'native_run_start':pin(native_start) if native_start.exists() else None,
                'completion':pin(native_completion) if native_completion.exists() else None,
                'completed_iterations_before':completed_before,
                'new_iterations':exported['completed_iterations']-completed_before if evaluation is None else None,
                'max_new_iterations':max_new_iterations if evaluation is None else None})
            finished = bool(finish_lease or reason or child.returncode != 0)
            write(guard_state / 'progress.json', {'pod_id': pod_id, 'epoch': time.time(),
                'last_productive_epoch': productive, 'last_activity_epoch':time.time(),
                'native_alive':False,'queued_work':False,'finished': finished}, replace=True)
            if finished:
                write(guard_state / 'recovery-complete.json', {'pod_id': pod_id, 'epoch': time.time()}, replace=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    stage_p = sub.add_parser('stage')
    stage_p.add_argument('--archive', required=True); stage_p.add_argument('--sha256', required=True)
    stage_p.add_argument('--hot', required=True)
    restore_p = sub.add_parser('restore')
    restore_p.add_argument('--hot', required=True); restore_p.add_argument('--durable', required=True)
    run_p = sub.add_parser('run')
    run_p.add_argument('--hot', required=True); run_p.add_argument('--durable', required=True)
    run_p.add_argument('--guard-state', required=True)
    run_p.add_argument('--max-new-iterations', required=True, type=int)
    run_p.add_argument('--finish-lease', action='store_true')
    run_p.add_argument('--mode',required=True,choices=('qualification','production'))
    run_p.add_argument('--qualification');run_p.add_argument('--qualification-sha256')
    run_p.add_argument('--local-reference');run_p.add_argument('--local-reference-sha256')
    run_p.add_argument('--pod-id',required=True);run_p.add_argument('--cpu-reserve',type=float,default=0)
    run_p.add_argument('--max-pending-completed-iterations',type=int,default=2,choices=(1,2))
    eval_p = sub.add_parser('evaluate')
    eval_p.add_argument('--hot', required=True); eval_p.add_argument('--durable', required=True)
    eval_p.add_argument('--guard-state', required=True); eval_p.add_argument('--name', required=True)
    eval_p.add_argument('--finish-lease', action='store_true')
    eval_p.add_argument('--pod-id',required=True);eval_p.add_argument('--cpu-reserve',type=float,default=0)
    args = parser.parse_args()
    if args.action == 'stage':
        result = stage(args.archive, args.sha256, args.hot)
    elif args.action == 'restore':
        result = restore(args.durable, args.hot)
    elif args.action == 'evaluate':
        result = run(args.hot, args.durable, args.guard_state, 0, args.finish_lease, args.name,
                     'qualification',pod_id=args.pod_id,cpu_reserve=args.cpu_reserve)
    else:
        proof={'path':args.qualification,'sha256':args.qualification_sha256} if args.qualification else None
        local={'path':args.local_reference,'sha256':args.local_reference_sha256} if args.local_reference else None
        result = run(args.hot, args.durable, args.guard_state, args.max_new_iterations,args.finish_lease,
                     mode=args.mode,proof_pin=proof,local_pin=local,pod_id=args.pod_id,cpu_reserve=args.cpu_reserve,
                     max_pending_completed_iterations=args.max_pending_completed_iterations)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
