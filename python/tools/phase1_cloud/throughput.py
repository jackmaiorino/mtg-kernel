"""Pure qualification checks and Linux quota-aware throughput policy."""
from __future__ import annotations
import argparse
import hashlib
import math
import struct
from pathlib import Path, PurePosixPath
from common import encoded, pin, read, require, write
from semantic import ALGORITHM


def finite(value, positive=False):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and (
        value > 0 if positive else value >= 0)


def preparation_workers(config):
    workers=config.get('preparation_workers',1)
    require(type(workers) is int and 1<=workers<=32,'invalid preparation worker count; integer 1..32 required')
    return workers


def update_backward_execution(config):
    """Fresh-V4-lineage-only option (mtg-kernel `UpdateBackwardExecutionV1`);
    a bare string on the wire, mirroring `preparation_workers` above, not a
    `{'kind':...}` mapping like `update_backend`. Default 'sequential' keeps
    every existing config and the V3 lineage unchanged."""
    execution=config.get('update_backward_execution','sequential')
    require(execution in ('sequential','fixed_partition_4'),
            "invalid update_backward_execution; 'sequential' or 'fixed_partition_4' required")
    return execution


def max_non_natural_episode_fraction(config):
    """mtg-kernel `NativeExpandedTrainingRunV1.max_non_natural_episode_fraction`
    (and the same-named field on `ExpandedTrainingCommandV1::Collect`/
    `CollectParallel`): a bare float on the wire, mirroring
    `preparation_workers`/`update_backward_execution` above. Default 0.0
    keeps today's fatal-on-first-non-Natural-terminal collection behavior
    and every existing config unchanged; the admitted range is the
    half-open [0.0, 1.0), matching the Rust-side validator
    (`validate_max_non_natural_episode_fraction_v1`) exactly."""
    fraction=config.get('max_non_natural_episode_fraction',0.0)
    require(finite(fraction) and fraction<1.0,
            'invalid max_non_natural_episode_fraction; float in [0.0, 1.0) required')
    return fraction


def max_prepared_tensor_mebibytes(config):
    """mtg-kernel `NativeExpandedTrainingRunV1.max_prepared_tensor_mebibytes`
    (and the same-named field on `ExpandedTrainingCommandV1::UpdatePrepared`):
    a bare integer on the wire, mirroring `preparation_workers` above.
    Default 256 keeps every existing config unchanged; admitted range
    64..=4096, matching the Rust-side validator exactly; a non-default value
    requires preparation_workers > 1 (prepared updates only)."""
    mebibytes=config.get('max_prepared_tensor_mebibytes',256)
    require(type(mebibytes) is int and 64<=mebibytes<=4096,
            'invalid max_prepared_tensor_mebibytes; integer in 64..=4096 required')
    require(mebibytes==256 or preparation_workers(config)>1,
            'max_prepared_tensor_mebibytes applies to prepared updates only (preparation_workers > 1)')
    return mebibytes


def validate_preparation_runner(config,document):
    workers=preparation_workers(config)
    if workers==1:
        require('preparation_backend' not in document and 'preparation_workers_requested' not in document,
                'serial native run contains prepared execution metadata')
    else:
        actual=document.get('preparation_workers_requested')
        require(type(actual) is int and actual==workers and
                document.get('preparation_backend')=='native-cpu-ordered-physical-groups-v1',
                'native preparation execution differs from config')


def validate_preparation_update(config,document):
    workers=preparation_workers(config)
    if workers==1:
        require('update_preparation' not in document,'serial run contains a prepared update')
        return None
    row=document.get('update_preparation')
    require(isinstance(row,dict),'prepared update execution telemetry unavailable')
    requested=row.get('requested_workers');started=row.get('started_workers');jobs=row.get('physical_group_jobs')
    require(row.get('schema')=='mtg-kernel-ordered-update-preparation/v1' and
            type(requested) is int and requested==workers and type(jobs) is int and 1<=jobs<=65536 and
            type(started) is int and started==min(workers,jobs),
            'update preparation execution differs from config or physical job count')
    return {'requested_workers':requested,'started_workers':started,'physical_group_jobs':jobs}


def validate_preparation_result(config,document):
    require(preparation_workers(document)==preparation_workers(config),
            'supervisor preparation worker setting differs from config')


def canonical_training_config(config):
    import copy
    value=copy.deepcopy(config)
    if preparation_workers(value)==1:value.pop('preparation_workers',None)
    if value.get('collection_workers')==1:value.pop('collection_workers')
    if value.get('update_backend')=={'kind':'cpu'}:value.pop('update_backend')
    # Same convention as preparation_workers/update_backend above: stripped
    # only at the byte-identical default, so a contract computed with
    # fixed_partition_4 is supposed to differ from one computed with the
    # (explicit or implicit) sequential default.
    if update_backward_execution(value)=='sequential':value.pop('update_backward_execution',None)
    # Same convention: stripped only at the byte-identical default of 256.
    if max_prepared_tensor_mebibytes(value)==256:value.pop('max_prepared_tensor_mebibytes',None)
    # Same convention again: stripped only at the byte-identical 0.0 default.
    # f32-round-tripped like learning_rate/value_coefficient below (it is an
    # f32 field on the Rust side too) before that comparison, so an explicit
    # value that rounds to exactly 0.0 strips the same as an absent one, and
    # a kept value is hashed at the exact bit pattern mtg-kernel uses.
    fraction=struct.unpack('<f',struct.pack('<f',max_non_natural_episode_fraction(value)))[0]
    if fraction==0.0:
        value.pop('max_non_natural_episode_fraction',None)
    else:
        value['max_non_natural_episode_fraction']=fraction
    for key in ('learning_rate','value_coefficient'):
        value[key]=struct.unpack('<f',struct.pack('<f',value[key]))[0]
    return value


def training_contract(config):
    import copy
    value = canonical_training_config(config)
    value.pop('output_directory')
    value.pop('collection_workers', None)
    # Path-bearing import hashes differ after relocation. Package this digest
    # once from the exact original config and bind both machines to that digest.
    for source in [value['initial_source'], *[o['source'] for o in value['opponents']]]:
        for key in ('play_import', 'checkpoint'):
            if source.get(key):
                source[key] = {'sha256': source[key]['sha256']}
    return hashlib.sha256(encoded(value)).hexdigest()


TIMING_PHASES = ('input_preparation', 'allocation', 'image_startup', 'input_transfer',
                 'stage_verification', 'native_invocation', 'checkpoint_export',
                 'output_transfer', 'release_confirmation')


def complete_work_seconds(envelope_pin, reference_pin, reference, environment, artifacts=None):
    """Validate a pinned lifecycle measurement, never infer unrecorded overhead.

    The outer interval includes every wait and overlap. Phase intervals need not
    be disjoint and are not summed. One controller clock measures all boundaries;
    receipts from other clocks are evidence, not substituted timing boundaries.
    """
    checked=artifacts.checked if artifacts is not None else lambda item:read(pin(item['path'],item['sha256'])['path'])
    value=checked(envelope_pin)
    require(value['schema']=='phase1-complete-work-timing/v1' and value['complete'] is True,
            'complete lifecycle timing required')
    require(value['environment']==environment and value['reference']==reference_pin,
            'timing envelope references different measured work')
    require(value['clock']=='single_controller_monotonic' and value['clock_session'],
            'one explicit controller clock session required')
    begin,end=value['start_seconds'],value['end_seconds']
    require(finite(begin) and finite(end) and end>begin, 'invalid lifecycle duration')
    require(set(value['phases'])==set(TIMING_PHASES), 'missing lifecycle phase is unavailable, not zero')
    measured={}
    for name,row in value['phases'].items():
        require(row.get('evidence'), 'each phase requires pinned timing evidence: '+name)
        evidence=[checked(item) for item in row['evidence']]
        if row.get('not_applicable') is True:
            require(environment=='local' and name in ('allocation','image_startup','release_confirmation')
                    and row.get('reason') and 'start_seconds' not in row and 'end_seconds' not in row,
                    'unmeasured overhead cannot be called inapplicable: '+name)
            continue
        a,b=row['start_seconds'],row['end_seconds']
        require(finite(a) and finite(b) and begin<=a<=b<=end, 'phase outside lifecycle: '+name)
        measured[name]=(a,b)
        if name=='release_confirmation' and environment=='cloud':
            require(any(item.get('provider_absent') is True for item in evidence),
                    'cloud timing requires confirmed provider release')
    require(min(a for a,b in measured.values())==begin and max(b for a,b in measured.values())==end,
            'lifecycle boundaries must include first setup and last completed phase')
    require(measured['input_preparation'][0]==begin, 'timing must start before input preparation')
    a,b=measured['native_invocation']
    require(b-a>=reference['elapsed_seconds'], 'native invocation exceeds measured phase')
    require(measured['checkpoint_export'][1]>=b and
            measured['output_transfer'][1]>=measured['checkpoint_export'][1],
            'timing excludes final checkpoint export or output transfer')
    if environment=='cloud':
        require(measured['release_confirmation'][1]==end and
                measured['release_confirmation'][1]>=measured['output_transfer'][1],
                'cloud lifecycle must end after export and confirmed release')
    else:
        require(measured['output_transfer'][1]==end, 'local lifecycle must include output delivery')
    return end-begin


def qualify(local_pin, cloud_pin, local_timing_pin=None, cloud_timing_pin=None, artifacts=None):
    checked=artifacts.checked if artifacts is not None else lambda item:read(pin(item['path'],item['sha256'])['path'])
    local = checked(local_pin)
    cloud = checked(cloud_pin)
    for item in (local, cloud):
        preparation_workers(item)
        require(item['schema'] == 'phase1-fixed-work-training/v1', 'fixed-work schema differs')
        require(item['complete'] is True, 'fixed work did not complete')
        for key in ('completed_games', 'completed_iterations', 'elapsed_seconds',
                    'max_iteration_elapsed_seconds', 'collection_workers'):
            require(finite(item[key], True), 'invalid fixed-work measurement: ' + key)
        require(item['max_iteration_elapsed_seconds'] <= item['elapsed_seconds'],
                'iteration duration exceeds complete measurement')
    for key in ('source_commit', 'training_contract_sha256', 'fixed_work_sha256',
                'initial_state_sha256', 'final_state_sha256', 'iteration_state_sha256s',
                'trajectory_semantics', 'first_batch_semantic_sha256s', 'completed_games', 'completed_iterations'):
        require(local[key] == cloud[key], 'unmatched fixed work or training state: ' + key)
    require(preparation_workers(local)==preparation_workers(cloud),
            'unmatched fixed work: preparation worker settings differ')
    require(local['trajectory_semantics']==ALGORITHM, 'unknown trajectory comparison algorithm')
    native_speedup = local['elapsed_seconds'] / cloud['elapsed_seconds']
    local_complete=complete_work_seconds(local_timing_pin,local_pin,local,'local',artifacts) if local_timing_pin else None
    cloud_complete=complete_work_seconds(cloud_timing_pin,cloud_pin,cloud,'cloud',artifacts) if cloud_timing_pin else None
    measured=local_complete is not None and cloud_complete is not None
    speedup=local_complete/cloud_complete if measured else None
    return {'schema': 'phase1-cloud-training-qualification/v1',
            'local_reference': local_pin, 'cloud_reference': cloud_pin,
            'source_commit': cloud['source_commit'],
            'training_contract_sha256': cloud['training_contract_sha256'],
            'fixed_work_sha256': cloud['fixed_work_sha256'],
            'cloud_collection_workers': cloud['collection_workers'],
            'preparation_workers':preparation_workers(cloud),
            'local_timing':local_timing_pin,'cloud_timing':cloud_timing_pin,
            'timing_status':'complete_work_measured' if measured else 'runtime_only_unqualified',
            'local_complete_work_seconds':local_complete,'cloud_complete_work_seconds':cloud_complete,
            'native_stage_speedup':native_speedup,
            'raw_first_batch_hashes_equal':local['first_batch_episode_sha256s']==cloud['first_batch_episode_sha256s'],
            'trajectory_comparison':ALGORITHM,
            'speedup': speedup, 'minimum_substantive_speedup': 1.5,
            'passed': measured and speedup >= 1.5, 'engineering_only': True}


def production_reference(proof_pin, local_pin, manifest, workers, preparation=1):
    proof = read(pin(proof_pin['path'], proof_pin['sha256'])['path'])
    require(proof['schema'] == 'phase1-cloud-training-qualification/v1', 'wrong proof schema')
    require(proof['local_reference']['sha256'] == local_pin['sha256'], 'local reference differs')
    # Recompute the decision from both pinned measurements. A passed boolean
    # cannot grant production eligibility on behalf of different evidence.
    actual = qualify(local_pin, proof['cloud_reference'],proof.get('local_timing'),proof.get('cloud_timing'))
    require(proof|{'preparation_workers':preparation_workers(proof)} == actual and actual['passed'],
            'qualification does not establish 1.5x speedup')
    require(actual['source_commit'] == manifest['source_commit'] and
            actual['training_contract_sha256'] == manifest['training_contract_sha256'],
            'qualification source or training contract differs')
    require(actual['cloud_collection_workers'] == workers, 'qualified worker setting differs')
    require(preparation_workers(actual)==preparation_workers({'preparation_workers':preparation}),
            'qualified preparation worker setting differs')
    return read(local_pin['path'])


def cgroup_limits(cgroup_text, mountinfo_text, read_text, affinity):
    """Resolve the task's actual cgroup and all visible ancestors, v1 or v2.

    The compatibility capacity field is an upper bound, not measured entitlement.
    Complete visible quota reads qualify only the visible-hierarchy occupancy
    estimate. Mount roots can themselves be namespaced, including a root named
    '/'; these inputs cannot establish the absence of hidden ancestor limits.
    """
    memberships = {}
    for line in cgroup_text.splitlines():
        _, controllers, path = line.split(':', 2)
        for controller in controllers.split(','):
            memberships[controller] = PurePosixPath(path)
    # In a hybrid host, the unified hierarchy can exist without owning CPU or
    # memory. Explicit v1 membership selects that controller's active hierarchy;
    # an absent file in inactive v2 is not missing active-controller evidence.
    controller_hierarchies = {
        controller: 'v1' if controller in memberships else 'v2' if '' in memberships else 'unobserved'
        for controller in ('cpu', 'memory')}
    observed = {}; quotas = []; memory = []; cpus = set(affinity)
    failures = []; boundaries = []; visible_reasons = []; valid_cpu_files = set()
    cpu_limit_observations = 0

    def reason(code):
        if code not in visible_reasons:
            visible_reasons.append(code)

    def invalid(path, controller):
        failures.append({'path': path, 'controller': controller,
                         'kind': 'invalid_value'})
        if controller == 'cpu':
            reason('visible_cpu_quota_invalid')

    for line in mountinfo_text.splitlines():
        before, after = line.split(' - ', 1)
        fields = before.split(); kind, _, options = after.split()[:3]
        root, mount = PurePosixPath(fields[3]), PurePosixPath(fields[4])
        controllers = options.split(',')
        if kind == 'cgroup2':
            key = ''
            quota_files = ('cpu.max',) if controller_hierarchies['cpu'] == 'v2' else ()
            memory_file = 'memory.max' if controller_hierarchies['memory'] == 'v2' else None
            if not quota_files and memory_file is None:
                continue
        elif kind == 'cgroup' and 'cpu' in controllers and controller_hierarchies['cpu'] == 'v1':
            key = 'cpu'; quota_files = ('cpu.cfs_quota_us', 'cpu.cfs_period_us')
            memory_file = ('memory.limit_in_bytes' if 'memory' in controllers and
                           memberships.get('memory') == memberships['cpu'] else None)
        elif kind == 'cgroup' and 'memory' in controllers and controller_hierarchies['memory'] == 'v1':
            key = 'memory'; quota_files = (); memory_file = 'memory.limit_in_bytes'
        else:
            continue
        member = memberships.get(key)
        boundary = None
        if quota_files:
            boundary = {'cgroup_version': 2 if kind == 'cgroup2' else 1,
                        'mount_root': str(root), 'mount_point': str(mount),
                        'membership': str(member) if member is not None else None,
                        'resolved_task_path': None, 'walked_paths': [],
                        'ancestors_above_visible_root': 'unknown'}
            boundaries.append(boundary)
        if member is None:
            if quota_files:
                reason('cpu_membership_unresolved')
            continue
        # A membership outside the observable namespace must not become a path
        # traversal or an ancestor walk that never reaches the mounted root.
        if any(not path.is_absolute() or '..' in path.parts for path in (root, mount, member)):
            if quota_files:
                reason('cpu_membership_unresolved')
            continue
        try:
            suffix = member.relative_to(root)
        except ValueError:
            # Namespace-relative membership '/' can refer to this mounted root.
            if str(member) == '/':
                suffix = PurePosixPath('.')
            else:
                if quota_files:
                    reason('cpu_membership_unresolved')
                continue
        current = mount / suffix
        if boundary is not None:
            boundary['resolved_task_path'] = str(current)
        while True:
            if boundary is not None:
                boundary['walked_paths'].append(str(current))

            def fetch(name, controller):
                path = str(current / name)
                try:
                    value = read_text(path).strip(); observed[path] = value; return value
                except (OSError, KeyError, UnicodeError) as error:
                    failures.append({'path': path, 'controller': controller,
                        'kind': 'missing' if isinstance(error, (FileNotFoundError, KeyError)) else 'unreadable',
                        'error_type': type(error).__name__, 'errno': getattr(error, 'errno', None)})
                    if controller == 'cpu':
                        reason('visible_cpu_quota_unreadable')
                    return None
            if kind == 'cgroup2' and quota_files:
                value = fetch('cpu.max', 'cpu')
                if value is not None:
                    path = str(current / 'cpu.max')
                    try:
                        quota, period = value.split()
                        require(int(period) > 0, 'invalid CPU quota period')
                        if quota != 'max':
                            require(int(quota) > 0, 'invalid CPU quota')
                            quotas.append(int(quota) / int(period))
                        valid_cpu_files.add(path)
                        cpu_limit_observations += 1
                    except (ValueError, OverflowError):
                        invalid(path, 'cpu')
            elif quota_files:
                values = [fetch(name, 'cpu') for name in quota_files]
                parsed = []
                for name, value in zip(quota_files, values):
                    path = str(current / name)
                    if value is None:
                        parsed.append(None)
                        continue
                    try:
                        number = int(value)
                        require(number > 0 or (name == 'cpu.cfs_quota_us' and number == -1),
                                'invalid CPU quota or period')
                        parsed.append(number); valid_cpu_files.add(path)
                    except ValueError:
                        invalid(path, 'cpu'); parsed.append(None)
                quota, period = parsed
                if quota is not None and period is not None:
                    cpu_limit_observations += 1
                    if quota > 0:
                        quotas.append(quota / period)
            if memory_file:
                value = fetch(memory_file, 'memory')
                if value and value != 'max':
                    try:
                        memory.append(int(value))
                    except ValueError:
                        invalid(str(current / memory_file), 'memory')
            if current == mount:
                break
            current = current.parent
    require(cpus, 'empty CPU affinity')
    if not boundaries:
        reason('cpu_hierarchy_unobserved')
    visible_eligible = bool(boundaries) and not visible_reasons
    capacity = min([len(cpus), *quotas])
    return {'eligible_capacity_cpus': capacity,
            'capacity_upper_bound_cpus': capacity,
            'capacity_basis': ('visible_quota_and_affinity_upper_bound' if cpu_limit_observations
                               else 'affinity_only_upper_bound'),
            'capacity_is_measured_entitlement': False,
            'visible_hierarchy_occupancy_eligible': visible_eligible,
            'production_occupancy_eligible': False,
            'cpu_quota_observability': {
                'status': ('visible_hierarchy_complete' if visible_eligible else
                           'visible_hierarchy_incomplete' if boundaries else 'cpu_hierarchy_unobserved'),
                'eligibility_scope': 'visible_hierarchy_only',
                'controller_hierarchies': controller_hierarchies,
                'valid_cpu_quota_files': sorted(valid_cpu_files),
                'read_failures': failures,
                'visible_root_boundaries': boundaries,
                'visible_hierarchy_rejection_reasons': visible_reasons,
                'production_rejection_reasons': [*visible_reasons, 'hidden_ancestor_limits_unknown']},
            'visible_memory_limit_bytes': min(memory) if memory else None,
            'visible_cgroup_values': observed, 'affinity': sorted(cpus),
            'unobservable_ancestor_limits': 'unavailable'}


class ThroughputPolicy:
    def __init__(self, mode, local_reference=None):
        require(mode in ('qualification', 'production'), 'explicit work mode required')
        require(mode != 'production' or local_reference is not None, 'local reference required')
        self.mode = mode; self.reference = local_reference; self.last = None
        self.occupancy_start = self.throughput_start = None
        self.cpu = self.eligible = self.capacity = 0.
        self.exploitability_known = True
        self.low_occupancy = self.avoidable_low_occupancy = self.low_throughput = 0
        self.throughput_start_games = 0
        self.throughput_window_seconds = max(60., 2 * local_reference['max_iteration_elapsed_seconds']) if local_reference else None

    def observe(self, stamp, cpu_seconds, capacity_cpus, exploitable_cpus, completed_games, queued_work,
                sequential_reason=None):
        require(all(finite(v) for v in (stamp,cpu_seconds,capacity_cpus,completed_games)) and
                (exploitable_cpus is None or finite(exploitable_cpus)),
                'non-finite policy observation')
        record = {'action': None, 'mode': self.mode}
        if self.last is None:
            self.occupancy_start = self.throughput_start = stamp
            self.throughput_start_games = completed_games
        else:
            elapsed = stamp - self.last['stamp']; used = cpu_seconds - self.last['cpu']
            require(elapsed >= 0 and used >= 0, 'nonmonotonic telemetry')
            if self.last['queued']:
                self.cpu += used
                if self.last['exploitable'] is None:self.exploitability_known=False
                else:self.eligible += elapsed * self.last['exploitable']
                self.capacity += elapsed * self.last['capacity']
        self.last = {'stamp': stamp, 'cpu': cpu_seconds, 'exploitable': exploitable_cpus,
                     'capacity': capacity_cpus, 'queued': queued_work}
        if stamp - self.occupancy_start >= 60:
            occupancy = self.cpu / self.capacity if self.capacity else None
            avoidable = (self.eligible > stamp-self.occupancy_start and
                         self.cpu / self.eligible < .9) if self.exploitability_known else None
            record['occupancy_window'] = {'seconds': stamp - self.occupancy_start,
                'eligible_capacity_cpu_occupancy': occupancy,
                'algorithmically_exploitable_cpu_occupancy': self.cpu / self.eligible if self.eligible and self.exploitability_known else None,
                'algorithmic_capacity_status':'known' if self.exploitability_known else 'unavailable_subphase_unknown',
                'sequential_reason': sequential_reason, 'avoidable': avoidable}
            self.low_occupancy = self.low_occupancy + 1 if occupancy is not None and occupancy < .9 and queued_work else 0
            self.avoidable_low_occupancy = self.avoidable_low_occupancy + 1 if self.low_occupancy and avoidable else 0
            self.occupancy_start = stamp; self.cpu = self.eligible = self.capacity = 0.
            self.exploitability_known=True
            if self.low_occupancy >= 2:
                record['diagnosis'] = 'two_full_capacity_windows_below_90_percent'
                record['disposition'] = ('diagnose_unknown_update_subphase_before_assigning_sequential_or_parallel_cause'
                    if avoidable is None else 'diagnose_avoidable_underutilization' if avoidable else
                    'resize_or_coschedule_already_planned_independent_work_or_report_sequential_gap')
                if self.mode == 'production' and self.avoidable_low_occupancy >= 2:
                    record['action'] = 'stop_and_requalify_avoidable_underutilization'
        if self.mode == 'production' and stamp - self.throughput_start >= self.throughput_window_seconds:
            elapsed = stamp - self.throughput_start
            rate = (completed_games - self.throughput_start_games) / elapsed
            baseline = self.reference['completed_games'] / self.reference['elapsed_seconds']
            speedup = rate / baseline
            record['throughput_window'] = {'seconds': elapsed, 'speedup_vs_local': speedup,
                'measurement_scope':'runtime_only_including_recurring_export_waits',
                'completed_games': completed_games - self.throughput_start_games}
            self.low_throughput = self.low_throughput + 1 if speedup < 1.2 and queued_work else 0
            self.throughput_start = stamp; self.throughput_start_games = completed_games
            if self.low_throughput >= 2:
                record['action'] = 'stop_and_release_sustained_sub_1_2x'
        return record


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--local-reference',required=True);parser.add_argument('--cloud-reference',required=True)
    parser.add_argument('--local-timing');parser.add_argument('--cloud-timing')
    parser.add_argument('--output',required=True)
    args=parser.parse_args()
    result=qualify(pin(args.local_reference),pin(args.cloud_reference),
                   pin(args.local_timing) if args.local_timing else None,
                   pin(args.cloud_timing) if args.cloud_timing else None)
    write(args.output,result)
    print(encoded(result).decode())


if __name__=='__main__': main()
