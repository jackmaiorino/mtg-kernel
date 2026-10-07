"""Admission guard for the line (a) launcher: every refusal happens before any spawn.

Enforces C:/Users/user/COMPUTE-POLICY.md items 2 to 5 with the 2026-09-25
clarification (shortest projected completion among feasible placements; a
missed earlier bound is a recorded shortfall, never a veto), the artifact law's
budget, pinning and scratch clauses (collab/ARTIFACT-LAW.md 1, 2, 4), the
storage ruling's scratch manifest and e-io lock (DIRECTOR-RULINGS-20260926.md
R3(b), (d), (f)), the line (a) order rule (calibration before screen training,
DIRECTOR-RULINGS-20260927.md R10) and one pinned yardstick executable
(CODEX #523 C3). Each check raises ValueError naming the refusal. Nothing here
spawns work, reads outcomes or chooses a workload constant.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import time

THROUGHPUT_SCHEMA = 'g115-line-a-throughput/v1'
WORKSHEET_SCHEMA = 'g115-line-a-byte-worksheet/v1'
SCRATCH_SCHEMA = 'g115-line-a-scratch-manifest/v1'
ORDER_SCHEMA = 'g115-line-a-order/v1'
SCOPE_SCHEMA = 'g115-line-a-scope/v1'
LOCK_SCHEMA = 'collab-e-io-lock/v1'
HOSTS = ('desktop', 'computehost', 'runpod')
WORK_CLASSES = ('bo3-ordinary', 'bo3-search', 'training', 'training-search')
INVENTORY_MAX_AGE_SECONDS = 24 * 3600
DISK_RESERVE_BYTES = 60 * 2**30  # artifact law clause 1: 60 GiB free on the target volume
JOB_SET_CAPS = dict(calibration=12_000_000_000, screen=48_000_000_000)
TOTAL_CAP = 60_000_000_000  # proposal and S2: 60 GB (decimal) across both job sets
WORKSHEET_CATEGORIES = ('outputs', 'scratch', 'staging_copies', 'workspace', 'failed_attempts')
METADATA_PERCENT = 15
SCRATCH_PARENT = 'D:/e-scratch/'
PINNED_ROOT = 'E:/pinned-binaries'
E_IO_LOCK = (Path.home() / 'IdeaProjects/collab/LOCKS/e-io.json').as_posix()


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha256_file(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def read(path):
    return json.loads(Path(path).read_bytes())


def checked(ref):
    require(sha256_file(ref['path']) == ref['sha256'], 'Input hash changed: ' + str(ref['path']))
    return Path(ref['path'])


def normalized(path):
    return str(path).replace('\\', '/').lower()


# Throughput admission (COMPUTE-POLICY items 2 to 5).

def phase_rate(phase):
    require(phase['seconds'] > 0 and phase['completed'] == len(phase['rows']) > 0, 'Phase lacks completed work')
    return phase['completed'] / phase['seconds']


def projected_seconds(evidence, allocation, units):
    """Work split in proportion to measured rates: the slowest host's startup plus units over the summed rate."""
    rates, overheads = [], []
    for host, workers in allocation.items():
        measured = evidence['hosts'].get(host)
        require(measured is not None, 'Placement uses an unmeasured host: ' + host)
        phases = [p for p in measured['phases'] if p['workers'] == workers]
        require(len(phases) == 1, 'Placement uses an unmeasured worker count: %s x %d' % (host, workers))
        rates.append(phase_rate(phases[0]))
        overheads.append(measured['overhead_seconds'])
    return max(overheads) + units / sum(rates)


def require_throughput(evidence, work_class, units, binding, now=None):
    """Refuse dispatch without complete, current, identity-preserving scaling evidence; return the placement."""
    now = time.time() if now is None else now
    require(evidence is not None, 'Throughput evidence missing for ' + work_class)
    require(evidence['schema'] == THROUGHPUT_SCHEMA, 'Wrong throughput evidence schema')
    require(work_class in WORK_CLASSES and evidence['binding'].get('work_class') == work_class,
            'Throughput evidence covers a different work class')
    for key, value in binding.items():
        require(evidence['binding'].get(key) == value, 'Throughput evidence binds a different ' + key)
    inventory = evidence['inventory']
    require(set(inventory) == set(HOSTS), 'Inventory must cover desktop, the compute host and RunPod')
    for host, item in inventory.items():
        require(0 <= now - item['checked_unix'] <= INVENTORY_MAX_AGE_SECONDS, 'Refresh the inventory: ' + host)
        require(isinstance(item['eligible'], bool) and item['reason'].strip(), 'Inventory needs eligibility and reason')
        require(not item['eligible'] or item.get('competing') == [], 'Eligible host has competing work: ' + host)
    serial = None
    for host, measured in evidence['hosts'].items():
        require(inventory[host]['eligible'], 'Measured host is not eligible now: ' + host)
        workers = [p['workers'] for p in measured['phases']]
        require(workers == sorted(set(workers)) and workers[0] == 1 and len(workers) >= 2,
                'Serial and parallel both required, in increasing order: ' + host)
        require(measured['overhead_seconds'] >= 0, 'Startup, transfer and recovery overhead required')
        for phase in measured['phases']:
            phase_rate(phase)
            rows = {row['id']: row['sha256'] for row in phase['rows']}
            require(len(rows) == len(phase['rows']), 'Duplicate qualification row')
            serial = rows if serial is None else serial
            require(rows == serial, 'Parallel or remote output differs from the serial reference')
    for host, item in inventory.items():
        require(not item['eligible'] or host in evidence['hosts'], 'Eligible host was not measured: ' + host)
    placements = {p['id']: p for p in evidence['placements']}
    require(len(placements) == len(evidence['placements']), 'Duplicate placement id')
    for host, measured in evidence['hosts'].items():
        for phase in measured['phases']:
            require(any(p['allocation'] == {host: phase['workers']} for p in placements.values()),
                    'Every measured single-host allocation must be compared')
    eligible = []
    for placement in placements.values():
        if placement['eligible']:
            seconds = projected_seconds(evidence, placement['allocation'], units)
            require(abs(placement['projected_seconds'] - seconds) <= 1e-6 * max(1.0, seconds),
                    'Projection differs from measured rates: ' + placement['id'])
            eligible.append((seconds, placement['id']))
        else:
            require(placement['ineligibility_reason'].strip(), 'Explain the excluded placement: ' + placement['id'])
    require(eligible, 'No feasible placement')
    selected = placements.get(evidence['selected'])
    require(selected is not None and selected['eligible'], 'Selected placement missing or ineligible')
    require(selected['projected_seconds'] <= min(eligible)[0] + 1e-6, 'A faster feasible placement exists')
    # An earlier qualification's wall-time bound is a comparator, never a veto: record the shortfall.
    bound = evidence.get('earlier_bound')
    shortfall = None
    if bound is not None:
        require(bound['seconds'] > 0 and str(bound.get('source', '')).strip(), 'Earlier bound needs its source')
        shortfall = dict(bound_seconds=bound['seconds'], source=bound['source'],
                         shortfall_seconds=max(0.0, selected['projected_seconds'] - bound['seconds']))
    return dict(selected, shortfall=shortfall)


# Byte budget (artifact law clause 1, storage ruling R3(c)).

def projected_bytes(items):
    raw = sum(item['unit_bytes'] * item['units'] for item in items)
    return (raw * (100 + METADATA_PERCENT) + 99) // 100


def require_worksheet(worksheet, job_set, manifest_sha256, free_bytes, committed_other_bytes=0):
    """Refuse dispatch without a measured worksheet that fits the job-set cap, the joint 60 GB and the reserve."""
    require(worksheet is not None, 'Byte-cap worksheet missing for ' + job_set)
    require(worksheet['schema'] == WORKSHEET_SCHEMA and worksheet['job_set'] == job_set, 'Wrong byte worksheet')
    require(worksheet['manifest_sha256'] == manifest_sha256, 'Worksheet belongs to a different manifest')
    require(worksheet['cap_bytes'] == JOB_SET_CAPS[job_set], 'Worksheet cap differs from the proposal cap')
    items = worksheet['items']
    require(sorted(item['category'] for item in items) == sorted(WORKSHEET_CATEGORIES),
            'Worksheet must cover outputs, scratch, staging copies, workspace and failed attempts')
    for item in items:
        require(type(item['unit_bytes']) is int and item['unit_bytes'] >= 0 and type(item['units']) is int and
                item['units'] >= 0, 'Worksheet sizes must be measured integers: ' + item['category'])
        require(item.get('measured_by'), 'Worksheet unit size lacks its qualification receipt: ' + item['category'])
    projected = projected_bytes(items)
    require(worksheet['projected_bytes'] == projected, 'Worksheet projection differs from its items')
    require(projected <= worksheet['cap_bytes'], 'Projected bytes exceed the %s cap' % job_set)
    require(projected + committed_other_bytes <= TOTAL_CAP, 'Projected bytes exceed the joint 60 GB cap')
    require(free_bytes - projected >= DISK_RESERVE_BYTES, 'Target volume would fall below the 60 GiB reserve')
    return projected


def require_reserve_now(path, pending_bytes=0):
    free = shutil.disk_usage(path).free
    require(free - pending_bytes >= DISK_RESERVE_BYTES, 'Target volume below the 60 GiB reserve')
    return free


# Scratch (storage ruling R3(b), (d)).

def require_scratch_manifest(manifest, job, host, worksheet_cap):
    require(manifest is not None and manifest['schema'] == SCRATCH_SCHEMA, 'Scratch manifest must precede first use')
    require(manifest['job'] == job and manifest['host'] == host and manifest['owner'].strip(), 'Scratch owner differs')
    root = normalized(manifest['root']).rstrip('/') + '/'
    require(root == normalized(SCRATCH_PARENT) + job.lower() + '/', 'Scratch root must be D:/e-scratch/<job>/')
    for source in manifest['sources']:
        require(normalized(source['path']).startswith('e:/') and len(source['sha256']) == 64 and
                type(source['bytes']) is int, 'Scratch source must be an E: path with hash and bytes')
    require(isinstance(manifest['disposable'], list), 'Declare disposable intermediates before first use')
    require(0 < manifest['cap_bytes'] <= worksheet_cap, 'Scratch cap must sit inside the job-set cap')
    return manifest


def require_no_scratch_inputs(record, scratch_root):
    """No receipt cites a scratch path as an input of record."""
    root = normalized(scratch_root).rstrip('/') + '/'
    for item in record.get('inputs', []):
        require(not normalized(item['path']).startswith(root), 'Scratch path used as an input of record: ' + item['path'])


# e-io lock (storage ruling R3(f)): advisory, a waiter waits and never breaks it.

def acquire_e_io(owner, phase, expected_seconds, lock_path=None, timeout_seconds=None, poll_seconds=5.0,
                 clock=time.time, sleep=time.sleep):
    lock = Path(E_IO_LOCK if lock_path is None else lock_path)
    lock.parent.mkdir(parents=True, exist_ok=True)
    started = clock()
    while True:
        now = clock()
        record = dict(schema=LOCK_SCHEMA, owner=owner, phase=phase, start_unix=now,
                      expected_end_unix=now + expected_seconds)
        try:
            descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        except FileExistsError:
            try:
                holder = read(lock)
            except (OSError, ValueError):
                holder = {}
            require(holder.get('owner') != owner, 'e-io lock already held by this owner')
            require(timeout_seconds is None or clock() - started < timeout_seconds,
                    'e-io lock held by %s (%s); waiting, never breaking it' % (holder.get('owner'), holder.get('phase')))
            sleep(poll_seconds)
            continue
        with os.fdopen(descriptor, 'w', encoding='utf-8') as handle:
            json.dump(record, handle)
        return record


def release_e_io(owner, lock_path=None):
    lock_path = E_IO_LOCK if lock_path is None else lock_path
    holder = read(lock_path)
    require(holder.get('owner') == owner, 'Only the holder releases the e-io lock')
    os.remove(lock_path)


# Order (R10) and yardstick identity (CODEX #523 C3).

def require_calibration_before_training(order):
    require(order is not None and order['schema'] == ORDER_SCHEMA, 'Calibration completion record required first')
    completion = read(checked(order['calibration_completion']))
    require(completion.get('complete') is True, 'Calibration is not complete')
    usability = order['usability']
    text = checked(usability).read_text(encoding='utf-8')
    require(usability['verdict'] == 'pass' and usability['excerpt'].strip() and usability['excerpt'] in text,
            'The frozen usability rule has not passed on record')
    return order


def require_scope_ruling(scope, composition):
    """Composition E or staged B comes only from the director's scope ruling (R3, R10), never from a missing source."""
    require(scope is not None and scope['schema'] == SCOPE_SCHEMA, 'Director scope ruling record required')
    require(scope['composition'] in ('E', 'B') and scope['composition'] == composition,
            'Manifest composition differs from the director scope ruling')
    ruling = scope['ruling']
    text = checked(ruling).read_text(encoding='utf-8')
    require(ruling['excerpt'].strip() and ruling['excerpt'] in text, 'Scope ruling text absent from its record')
    return scope


def require_one_yardstick(manifests):
    executables = {m.get('yardstick_executable_sha256') for m in manifests}
    require(len(executables) == 1 and None not in executables,
            'Calibration, screen evaluation and confirmation need one pinned yardstick executable')
    return executables.pop()


# Pinned binaries (artifact law clause 4).

def pin_binary(path, pinned_root=None):
    """Copy a binary to <pinned_root>/<sha256>/<name> once, verified; return the pinned path and digest."""
    pinned_root = PINNED_ROOT if pinned_root is None else pinned_root
    source = Path(path)
    digest = sha256_file(source)
    target = Path(pinned_root) / digest / source.name
    if target.exists():
        require(sha256_file(target) == digest, 'Pinned copy differs from its hash: ' + str(target))
        return target, digest
    target.parent.mkdir(parents=True, exist_ok=True)
    partial = target.with_name(target.name + '.partial')
    shutil.copyfile(source, partial)
    require(sha256_file(partial) == digest, 'Pinned copy changed while copying')
    os.replace(partial, target)
    return target, digest


def require_pinned(path, sha256, pinned_root=None):
    pinned_root = PINNED_ROOT if pinned_root is None else pinned_root
    expected = normalized(Path(pinned_root) / sha256 / Path(path).name)
    require(normalized(path) == expected, 'Executable must run from its pinned copy: ' + str(path))
    require(sha256_file(path) == sha256, 'Pinned executable hash changed')
    return Path(path)
