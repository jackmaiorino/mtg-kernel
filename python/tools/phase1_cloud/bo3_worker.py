"""Guarded CPU BO3 qualification stream. No work, provider access or allocation on import.

Each owned native invocation advances at most one batch. Only a joined process
group and successful stopped-tree export permit the next invocation.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import shutil
import signal
import stat
import subprocess
import time

from common import encoded, require, unique_pairs, write
from bo3_recovery import Limits, export_stopped
from lease_guard import validate as validate_lease
from runtime_observation import ProcReader, controlled_environment, image_contract, sample as runtime_sample
from throughput import ThroughputPolicy, cgroup_limits, finite

MIB = 1024 ** 2
GIB = 1024 ** 3
RESERVE = 16 * GIB
MAX_STAGE = 8 * 3600
MAX_BYTES = 100 * GIB
FINAL_RECEIPT_RESERVE = 16 * MIB
HEX = re.compile(r'[0-9a-f]{64}')


def durable_write(path, value):
    require(len(encoded(value)) <= 16 * MIB, 'worker receipt exceeds 16 MiB')
    write(path, value)
    for parent in [Path(path).parent, *Path(path).parent.parents]:
        descriptor = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def heartbeat_fields(pod_id, now, productive, activity, native_alive, phase):
    return {'pod_id': pod_id, 'epoch': now, 'last_productive_epoch': productive,
            'last_activity_epoch': activity, 'native_alive': native_alive,
            'queued_work': native_alive, 'phase': phase, 'finished': False}


def receipt_charge(current, size, cap, reserve, final=False):
    require(all(type(value) is int and value >= 0 for value in (current, size, cap, reserve)),
            'integer receipt accounting required')
    require((not final or size <= reserve) and current + size <= cap - (0 if final else reserve),
            'receipt exceeds retained allowance or consumes reserved final-result headroom')
    return current + size


def require_disk_headroom(free_values, extra, written=0, final=False):
    require(free_values and all(type(value) is int and value >= 0 for value in [*free_values, extra, written]),
            'actual nonnegative disk free/write observations required')
    required = RESERVE + extra + (0 if final else FINAL_RECEIPT_RESERVE)
    require(all(free - written >= required for free in free_values), '16 GiB filesystem/final-result reserve reached')


def absolute(value):
    path = Path(value)
    require(path.is_absolute() and '..' not in path.parts and str(path) == value,
            'explicit canonical absolute path required')
    for part in reversed([path, *path.parents]):
        if part.exists():
            require(not part.is_symlink(), 'symlink in owned path')
    return path


def json_file(path, maximum=MIB, expected=None, tick=lambda: None, activity=lambda: None):
    row, data = file_pin(path, maximum, expected, tick, collect=True, activity=activity)
    def invalid(_):
        raise ValueError('non-finite JSON')
    return json.loads(data, object_pairs_hook=unique_pairs, parse_constant=invalid), row


def file_pin(path, maximum, expected=None, tick=lambda: None, collect=False, activity=lambda: None):
    path = Path(path)
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= maximum, 'bounded regular input required')
    digest, chunks, count = hashlib.sha256(), [], 0
    with path.open('rb') as stream:
        require((os.fstat(stream.fileno()).st_dev, os.fstat(stream.fileno()).st_ino)
                == (before.st_dev, before.st_ino), 'input changed while opening')
        while True:
            tick()
            chunk = stream.read(MIB)
            if not chunk:
                break
            count += len(chunk)
            require(count <= maximum, 'input grew beyond bound')
            digest.update(chunk)
            activity()
            if collect:
                chunks.append(chunk)
        after = path.stat()
        require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
                == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
                and count == before.st_size, 'input changed during hashing')
    row = {'path': str(path), 'sha256': digest.hexdigest(), 'bytes': count}
    require(expected is None or row['sha256'] == expected, 'input SHA256 differs: ' + str(path))
    return (row, b''.join(chunks)) if collect else row


def guard_dispatch(lease, status, pod_id, now, stop_exists=False):
    """Pure admission decision. A supplied dictionary is not a runtime certificate."""
    require(status.get('pod_id') == pod_id and status.get('name') == lease['name'],
            'guard belongs to another Pod or lease name')
    require(finite(status.get('epoch')) and -5 <= now - status['epoch'] <= 60, 'guard is stale')
    require(status.get('provider_verified') is True and status.get('provider_ok') is True,
            'provider ownership/allocation is not freshly verified')
    require(status.get('funds_verified') is True and status.get('allow_new_dispatch') is True
            and status.get('recovery_funding_available') is True, 'funding does not permit dispatch')
    require(not status.get('latched') and not stop_exists and status.get('release_epoch') is None,
            'guard has stopped new dispatch')
    require(now < lease['deadline_epoch'] - lease['recovery_seconds'], 'lease is in recovery window')
    require(type(status.get('pid')) is int and status['pid'] > 1, 'guard PID missing')


def remaining_admission(config, completed, maximum):
    require(config.get('schema') == 'mtg-kernel-native-bo3-training-run/v1'
            and isinstance(config.get('batches'), list) and 1 <= len(config['batches']) <= 4096,
            'finite native BO3 plan required')
    require(type(completed) is int and 0 <= completed <= len(config['batches'])
            and type(maximum) is int and 1 <= maximum <= 4096, 'invalid batch admission bound')
    return min(maximum, len(config['batches']) - completed)


def native_command(manifest):
    root = absolute(manifest['remote_root'])
    expected = [str(root / 'bin/phase1_bo3_trainable_v1'), 'run', '--request', str(root / 'config/run.json')]
    require(manifest.get('schema') == 'phase1-bo3-cloud-payload/v1'
            and manifest.get('argv') == expected and manifest['runtime']['executable']['path'] == expected[0]
            and manifest['run_request']['path'] == expected[3], 'exact packaged BO3 CPU command required')
    return expected + ['--max-new-batches', '1']


def tree_bytes(root, tick=lambda: None):
    total, count = 0, 0
    if not root.exists():
        return 0
    for directory, names, files in os.walk(root, followlinks=False):
        tick()
        for name in names + files:
            info = (Path(directory) / name).lstat()
            require(not stat.S_ISLNK(info.st_mode), 'owned storage contains symlink')
            count += 1
            require(count <= 200_000, 'owned storage entry cap exceeded')
            if stat.S_ISREG(info.st_mode):
                total += info.st_size
            else:
                require(stat.S_ISDIR(info.st_mode), 'owned storage contains special file')
    return total


def memory_available():
    fields = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines())
    return int(fields['MemAvailable'].strip().split()[0]) * 1024


class NativeCapacityWindows:
    """One qualification policy over CPU-ready seconds, preserving history across children.

    Export/input gaps are measured in real wall time separately and contribute
    neither CPU-ready capacity nor fabricated completed work to these windows.
    """
    def __init__(self):
        self.policy = ThroughputPolicy('qualification')
        self.ready_seconds = 0.
        self.last_wall = None
        self.active = False

    def observe(self, wall, cpu, capacity, completed, active):
        previous_active = self.active
        if self.last_wall is not None:
            require(wall >= self.last_wall, 'nonmonotonic native-capacity clock')
            if previous_active:
                self.ready_seconds += wall - self.last_wall
        self.last_wall, self.active = wall, active
        if not active and not previous_active:
            return {'phase': 'no_cpu_ready_work', 'cpu_ready_seconds': self.ready_seconds}
        row = self.policy.observe(self.ready_seconds, cpu, capacity, None, completed, True,
            'bounded native prefix/collection/replay/update/checkpoint work; exact subphase unavailable')
        row['window_clock'] = 'cumulative CPU-ready native wall seconds across children; exported gaps excluded'
        row['actual_monotonic_seconds'] = wall
        return row


class Session:
    def __init__(self, args, lease, status):
        self.args, self.lease, self.status = args, lease, status
        self.hot, self.durable, self.guard = map(absolute, (args.hot_root, args.durable_root, args.guard_state))
        self.started, self.epoch = time.monotonic(), time.time()
        self.deadline = self.started + args.stage_seconds
        self.last_check = self.last_beat = self.last_space = 0.
        self.last_productive = self.last_activity = self.epoch
        self.native_alive = False
        self.phase = 'input_verification'
        self.dispatch_stop = None
        self.release_epoch = min(lease['deadline_epoch'], self.epoch + args.stage_seconds)
        self.observed_costs = []
        self.recovery_baseline = 0
        self.recovery_written = 0
        self.space_snapshot = []
        self.space_written = 0
        self.groups = []
        self.policy = NativeCapacityWindows()
        self.completed_matches = self.completed_updates = 0
        self.quota_activity = (0, 0)
        self.receipt_written = 0

    def work(self):
        self.last_activity = time.time()

    def check(self, dispatch=False, beat=True):
        mono, now = time.monotonic(), time.time()
        require(mono < self.deadline and now < self.release_epoch - 2, 'whole-stage or recovery deadline reached')
        if mono - self.last_check >= 1 or dispatch:
            self.last_check = mono
            try:
                status, _ = json_file(self.guard / 'guard.json')
                require(status.get('pod_id') == self.args.pod_id and status.get('name') == self.lease['name'],
                        'guard identity changed')
                self.status = status
                if status.get('release_epoch') is not None:
                    require(finite(status['release_epoch']), 'invalid guard release deadline')
                    self.release_epoch = min(self.release_epoch, status['release_epoch'])
                stop_path = self.guard / 'stop-request.json'
                if stop_path.exists():
                    stop, _ = json_file(stop_path)
                    require(stop.get('pod_id') == self.args.pod_id and finite(stop.get('release_epoch')),
                            'stop request differs from own Pod')
                    self.release_epoch = min(self.release_epoch, stop['release_epoch'])
                guard_dispatch(self.lease, status, self.args.pod_id, now, stop_path.exists())
                os.kill(status['pid'], 0)
            except (OSError, ValueError, KeyError, TypeError) as error:
                self.dispatch_stop = self.dispatch_stop or str(error)
                self.release_epoch = min(self.release_epoch, now + self.lease['recovery_seconds'])
            require(now < self.release_epoch - 2, 'guard release window exhausted')
            if len(self.observed_costs) == 0 or now - self.observed_costs[-1]['observed_epoch'] >= 60:
                self.observed_costs.append({'observed_epoch': now, 'guard_epoch': self.status.get('epoch'),
                    **{key: self.status.get(key) for key in ('increment_conservative_usd', 'cumulative_conservative_usd',
                       'shutdown_forecast_usd', 'funded_balance_conservative_usd', 'account_spend_usd_hour',
                       'highest_rate_usd_hour', 'funding_alert')},
                    'scope': 'guard observations, not provider settlement'})
        if dispatch:
            require(self.dispatch_stop is None, 'new dispatch stopped: ' + str(self.dispatch_stop))
            require(mono + self.args.native_seconds + self.lease['recovery_seconds'] < self.deadline
                    and now + self.args.native_seconds + self.lease['recovery_seconds'] < self.release_epoch,
                    'insufficient whole-stage time for child ceiling and recovery reserve')
        if beat and self.args.execute and mono - self.last_beat >= 2:
            self.last_beat = mono
            write(self.guard / 'progress.json', heartbeat_fields(self.args.pod_id, now,
                  self.last_productive, self.last_activity, self.native_alive, self.phase), replace=True)

    def space(self, extra=0, recovery=False):
        self.check()
        if recovery:
            require(self.recovery_baseline + self.recovery_written + self.receipt_written + extra
                    <= self.args.max_retained_bytes - FINAL_RECEIPT_RESERVE,
                    'cumulative hot/durable retained byte cap exhausted')
        now = time.monotonic()
        if now - self.last_space >= 2 or not self.space_snapshot:
            self.last_space = now
            self.space_snapshot = [shutil.disk_usage(root).free for root in (self.hot, self.durable)]
            self.space_written = self.recovery_written + self.receipt_written
        newly_written = max(0, self.recovery_written + self.receipt_written - self.space_written)
        require_disk_headroom(self.space_snapshot, extra, newly_written)

    def stopped_baseline(self, final=False):
        require(not self.native_alive and all(group.stopped for group in self.groups),
                'retained baseline requires a stopped native boundary')
        tick = (lambda: None) if final else self.check
        self.recovery_baseline = tree_bytes(self.hot, tick) + tree_bytes(self.durable, tick)
        require(self.recovery_baseline <= self.args.max_retained_bytes - (0 if final else FINAL_RECEIPT_RESERVE),
                'retained bytes exceed stopped-boundary allowance')
        self.recovery_written = 0
        self.receipt_written = 0
        self.quota_activity = (0, 0)
        self.space_snapshot = []
        if not final:
            self.space()

    def publish(self, path, value, final=False):
        size = len(encoded(value))
        receipt_charge(self.recovery_baseline + self.recovery_written + self.receipt_written,
                       size, self.args.max_retained_bytes, FINAL_RECEIPT_RESERVE, final)
        if final:
            # Fresh physical-space check, deliberately independent of the work deadline.
            # The actual final bytes may use remaining reserved artifact/disk headroom.
            require_disk_headroom([shutil.disk_usage(root).free for root in (self.hot, self.durable)],
                                  size, final=True)
        else:
            self.space(size, recovery=True)
        # Charge before writing; partial publication remains conservatively owned.
        self.receipt_written += size
        durable_write(path, value)
        self.work()

    def quota(self, event):
        current = event['read_bytes'], event['written_bytes']
        if current != self.quota_activity:
            self.work()
        self.quota_activity = current
        self.recovery_written = event['written_bytes']
        self.space(event['next_write_bytes'], recovery=True)


class ObservedFiles(ProcReader):
    def __init__(self, session):
        self.session = session

    def file(self, path):
        value = file_pin(path, 512 * MIB, tick=self.session.check, activity=self.session.work)
        info = Path(path).stat()
        return value | {'inode': info.st_ino, 'device_major': os.major(info.st_dev), 'device_minor': os.minor(info.st_dev)}


def process_sample(pid):
    affinity = sorted(os.sched_getaffinity(pid))
    quota = cgroup_limits(Path(f'/proc/{pid}/cgroup').read_text(), Path(f'/proc/{pid}/mountinfo').read_text(),
                         lambda path: Path(path).read_text(), affinity)
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
    require(int(fields[16]) >= 10, 'actual native nice priority differs')
    headroom = []
    for path, value in quota['visible_cgroup_values'].items():
        if value == 'max':
            continue
        name = Path(path).name
        if name in ('memory.max', 'memory.limit_in_bytes'):
            usage = Path(path).with_name('memory.current' if name == 'memory.max' else 'memory.usage_in_bytes')
            headroom.append(int(value) - int(usage.read_text().strip()))
    require(not headroom or min(headroom) >= RESERVE, '16 GiB visible cgroup-memory reserve reached')
    return {'epoch': time.time(), 'pid': pid, 'affinity': affinity, 'quota': quota, 'nice': int(fields[16]),
            'visible_cgroup_memory_headroom_bytes': min(headroom) if headroom else None,
            'cpu_seconds': (int(fields[11]) + int(fields[12])) / os.sysconf('SC_CLK_TCK'),
            'rss_bytes': int(fields[21]) * os.sysconf('SC_PAGE_SIZE'), 'threads': int(fields[17]),
            'io': Path(f'/proc/{pid}/io').read_text()}


class OwnedGroup:
    """Fresh-session group, with wait4 CPU charged once and adopted orphans reaped."""
    def __init__(self, argv, environment):
        def limits():
            import resource
            resource.setrlimit(resource.RLIMIT_AS, (8 * GIB, 8 * GIB))
            os.nice(10)
        self.child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE, env=environment, start_new_session=True, preexec_fn=limits)
        self.pid = self.child.pid
        self.returncode, self.cpu_seconds, self.reaped, self.stopped = None, 0., [], False
        self.cleanup_deadline = None

    def reap(self):
        while True:
            try:
                pid, status, usage = os.wait4(-self.pid, os.WNOHANG)
            except ChildProcessError:
                break
            if pid == 0:
                break
            self.reaped.append(pid)
            self.cpu_seconds += usage.ru_utime + usage.ru_stime
            if pid == self.pid:
                self.returncode = os.waitstatus_to_exitcode(status)
                self.child.returncode = self.returncode

    def group_exists(self):
        if self.stopped:
            return False
        try:
            os.killpg(self.pid, 0)
            return True
        except ProcessLookupError:
            return False

    def stop(self):
        if self.stopped:
            return
        if self.cleanup_deadline is None:
            self.cleanup_deadline = time.monotonic() + 10
        deadline = self.cleanup_deadline
        self.reap()
        if self.group_exists():
            try:
                os.killpg(self.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        kill_at = time.monotonic() + 2
        while True:
            self.reap()
            if not self.group_exists():
                break
            require(time.monotonic() < deadline, 'owned process group did not stop within ten seconds')
            if time.monotonic() >= kill_at:
                try:
                    os.killpg(self.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            time.sleep(.05)
        self.reap()
        require(self.returncode is not None, 'owned leader was not reaped')
        self.stopped = True

    def assert_stopped(self):
        require(self.stopped and self.returncode is not None, 'native process group is not joined and reaped')


def validate_args(args):
    require(args.mode == 'qualification', 'production BO3 cloud execution remains rejected')
    require(os.name == 'posix' and Path('/proc/self/stat').exists(), 'Linux /proc environment required')
    require(args.pod_id and os.environ.get('RUNPOD_POD_ID') == args.pod_id, 'exact actual RUNPOD_POD_ID required')
    require(re.fullmatch('[a-z0-9][a-z0-9-]{0,47}', args.name) is not None, 'simple fresh invocation name required')
    require(30 < args.stage_seconds <= MAX_STAGE and 0 < args.native_seconds <= 600
            and 1 <= args.max_new_batches <= 4096, 'bounded stage/native/batch ceilings required')
    require(0 < args.max_output_bytes <= MAX_BYTES and FINAL_RECEIPT_RESERVE < args.max_retained_bytes <= MAX_BYTES
            and MIB <= args.max_log_bytes <= 16 * MIB, 'explicit bounded output/retained/log limits required')
    roots = [absolute(value) for value in (args.hot_root, args.durable_root, args.guard_state)]
    require(all(root.is_dir() for root in roots), 'pre-existing staged hot/durable/armed guard roots required')
    require(all(not a.is_relative_to(b) and not b.is_relative_to(a)
                for i, a in enumerate(roots) for b in roots[i + 1:]), 'hot/durable/guard roots must be disjoint')
    require(absolute(args.package_manifest) == roots[0] / 'manifest.json', 'staged manifest location differs')
    for value in (args.package_sha256, args.lease_sha256, args.runtime_image_sha256):
        require(HEX.fullmatch(value) is not None, 'exact input SHA256 required')
    lease, lease_pin = json_file(absolute(args.lease), expected=args.lease_sha256)
    validate_lease(lease)
    status, _ = json_file(roots[2] / 'guard.json')
    guard_dispatch(lease, status, args.pod_id, time.time(), (roots[2] / 'stop-request.json').exists())
    os.kill(status['pid'], 0)
    require(not any((roots[2] / name).exists() for name in ('released.json', 'release-requested.json', 'recovery-complete.json')),
            'lease is already finishing or released')
    return lease, lease_pin, status


def inputs(args, session):
    manifest, manifest_pin = json_file(args.package_manifest, 16 * MIB, args.package_sha256, session.check, session.work)
    command = native_command(manifest)
    require(manifest['remote_root'] == args.hot_root, 'staged hot namespace differs')
    require(len(manifest['files']) <= 100_000, 'package inventory exceeds bound')
    for name, row in manifest['files'].items():
        require(isinstance(name, str) and '\\' not in name and str(Path(name)) == name
                and not Path(name).is_absolute() and '..' not in Path(name).parts, 'unsafe package inventory path')
        path = absolute(str(session.hot / name))
        actual = file_pin(path, 512 * MIB, row['sha256'], session.check, activity=session.work)
        require(actual['bytes'] == row['bytes'] and stat.S_IMODE(path.stat().st_mode) == row['mode'],
                'staged package size/mode differs')
    config, request_pin = json_file(manifest['run_request']['path'], 16 * MIB,
                                    manifest['run_request']['sha256'], session.check, session.work)
    remaining_admission(config, 0, args.max_new_batches)
    require(config['output_directory'] == str(session.hot / 'run')
            and config['previous_progress'] is None and config['initial_input']['kind'] == 'ordinary_checkpoint_transition',
            'only packaged ordinary origin in stable hot namespace is admitted')
    image, image_pin = json_file(absolute(args.runtime_image), MIB, args.runtime_image_sha256, session.check, session.work)
    image_contract(image)
    helper_pins = [file_pin(Path(__file__).with_name(name), 2 * MIB, tick=session.check, activity=session.work)
                   for name in ('bo3_worker.py', 'bo3_recovery.py', 'common.py', 'lease_guard.py',
                                'runtime_observation.py', 'throughput.py', 'semantic.py', 'fresh_source.py')]
    return manifest, config, image, command, {'manifest': manifest_pin, 'request': request_pin,
                                            'runtime_image': image_pin, 'helpers': helper_pins}


@contextlib.contextmanager
def native_stopped_lock(session, group=None):
    """The same Linux flock used by native File::try_lock, held for every stopped read/export."""
    import fcntl
    require(not session.native_alive, 'native marked alive at stopped lock boundary')
    if group is not None:
        group.assert_stopped()
    run_root = session.hot / 'run'
    run_root.mkdir(exist_ok=True)
    path = absolute(str(run_root / 'run.lock'))
    with path.open('a+b') as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        def held():
            require(not handle.closed and not session.native_alive, 'stopped native lock lost')
            if group is not None:
                group.assert_stopped()
        yield held


def copy_observed(source, destination, session):
    """Retain actual mapped bytes/telemetry with chunk heartbeats and quota reservation."""
    source = file_pin(source['path'], 512 * MIB, source['sha256'], session.check, activity=session.work)
    if destination.exists():
        return file_pin(destination, 512 * MIB, source['sha256'], session.check, activity=session.work)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(destination.name + '.partial')
    with Path(source['path']).open('rb') as inp, temporary.open('xb') as out:
        while True:
            session.check()
            block = inp.read(MIB)
            if not block:
                break
            session.space(len(block), recovery=True)
            out.write(block)
            session.recovery_written += len(block)
            session.work()
        out.flush(); os.fsync(out.fileno())
    file_pin(temporary, 512 * MIB, source['sha256'], session.check, activity=session.work)
    os.link(temporary, destination)
    temporary.unlink()
    # Every newly created directory lies below the explicit durable root.
    for directory in [destination.parent, *destination.parent.parents]:
        descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        if directory == session.durable:
            break
    return file_pin(destination, 512 * MIB, source['sha256'], session.check, activity=session.work)


def retain_native_observations(session, sampled, old_telemetry=frozenset()):
    session.stopped_baseline()
    retained = {'runtime_files': [], 'native_telemetry': []}
    if sampled:
        for item in [sampled['executable'], *sampled['libraries'].values()]:
            target = session.durable / 'runtime-files' / item['sha256'] / Path(item['path']).name
            retained['runtime_files'].append(copy_observed(item, target, session))
    for path in sorted((session.hot / 'run/telemetry').glob('*.json')):
        item = file_pin(path, 16 * MIB, tick=session.check, activity=session.work)
        target = session.durable / 'native-telemetry' / path.name
        copied = copy_observed(item, target, session)
        if path.name not in old_telemetry:
            retained['native_telemetry'].append(copied)
    return retained


def stage_timings(retained, tick):
    result = {'collection_seconds': None, 'preparation_update_checkpoint_combined_seconds': None,
              'prefix_verification_seconds': None, 'tip_validation_seconds': None,
              'separate_update_seconds': None, 'separate_checkpoint_io_seconds': None,
              'source': 'native wall receipt fields, not CPU time'}
    fields = {'mtg-kernel-bo3-collection-wall/v1': {'collection_recovery_and_publication_wall_seconds': 'collection_seconds'},
              'mtg-kernel-bo3-batch-wall/v1': {'preparation_update_checkpoint_readback_combined_seconds': 'preparation_update_checkpoint_combined_seconds'},
              'mtg-kernel-bo3-run-wall/v1': {'prefix_stream_hash_and_metadata_seconds': 'prefix_verification_seconds',
                  'latest_tip_validation_seconds': 'tip_validation_seconds'}}
    for pin in retained['native_telemetry']:
        value, _ = json_file(pin['path'], 16 * MIB, pin['sha256'], tick)
        for source, target in fields.get(value.get('schema'), {}).items():
            require(finite(value.get(source)), 'native timing unavailable or invalid')
            result[target] = (result[target] or 0.) + value[source]
    return result


def export_receipt(exported):
    """Keep the growing exact history in its immutable index, not quadratic worker JSON."""
    if exported is None:
        return None
    summary = exported['summary']
    return {'index': exported['index'], 'summary': {key: summary[key] for key in
        ('completed_batches', 'planned_batches', 'completed_bo3_updates', 'attempted_batches',
         'attempted_matches', 'complete_matches', 'incomplete_matches', 'eligible_matches', 'adam_step', 'complete')},
        'pending_checkpoint_count': len(summary['pending_checkpoint_files']),
        'pending_progress_count': len(summary['pending_progress_files']),
        'read_bytes': exported['read_bytes'], 'written_bytes': exported['written_bytes'],
        'new_blob_bytes': exported['new_blob_bytes'], 'indexed_bytes': exported['indexed_bytes'],
        'long_stream_export_throughput_qualified': False}


def release_eligible(current_export, group_stopped, observations_retained):
    return current_export is not None and group_stopped and observations_retained


def run_native(session, manifest, config, image, command, folder):
    session.check(dispatch=True)
    session.space()
    require(memory_available() >= RESERVE, '16 GiB available-memory reserve required before dispatch')
    environment = controlled_environment(os.environ)
    started, sampled, error, observations = time.monotonic(), None, None, 0
    cpu_base = sum(child.cpu_seconds for child in session.groups)
    group = None
    stop_reason = None
    log_bytes = 0
    telemetry_bytes = 0
    logs_durable = False
    last_poll = 0.
    last_cpu, last_io, last_output, capacity = 0., None, 0, None
    policy_started = False
    policy_visibility_lost = False
    session.phase, session.native_alive = 'native_one_batch', True
    try:
        with (folder / 'native.log').open('xb') as log, (folder / 'telemetry.jsonl').open('xb') as records:
            group = OwnedGroup(command, environment)
            session.groups.append(group)
            selector = selectors.DefaultSelector()
            try:
                for pipe in (group.child.stdout, group.child.stderr):
                    os.set_blocking(pipe.fileno(), False)
                    selector.register(pipe, selectors.EVENT_READ)
                while True:
                    session.check()
                    if session.dispatch_stop:
                        raise ValueError('guard stopped child: ' + session.dispatch_stop)
                    require(time.monotonic() - started < session.args.native_seconds, 'native child deadline reached')
                    for key, _ in selector.select(.1):
                        block = os.read(key.fd, 65536)
                        if not block:
                            selector.unregister(key.fileobj)
                            continue
                        require(log_bytes + len(block) <= session.args.max_log_bytes, 'native log cap reached')
                        log.write(block)
                        log_bytes += len(block)
                        session.work()
                    group.reap()
                    if group.returncode is not None:
                        break
                    if time.monotonic() - last_poll >= 1:
                        last_poll = time.monotonic()
                        session.space()
                        require(memory_available() >= RESERVE, '16 GiB available-memory reserve reached')
                        output_bytes = tree_bytes(session.hot / 'run', session.check)
                        retained = tree_bytes(session.hot, session.check) + tree_bytes(session.durable, session.check)
                        require(output_bytes <= session.args.max_output_bytes
                                and retained <= session.args.max_retained_bytes - FINAL_RECEIPT_RESERVE,
                                'sampled output or retained byte cap reached')
                        try:
                            row = process_sample(group.pid)
                        except (FileNotFoundError, ProcessLookupError):
                            continue
                        if sampled is None:
                            sampled = runtime_sample(group.child, manifest['runtime']['executable'], image,
                                                     environment, row, ObservedFiles(session))
                        if row['cpu_seconds'] > last_cpu or (last_io is not None and row['io'] != last_io) or output_bytes > last_output:
                            session.work()
                        last_cpu, last_io, last_output = row['cpu_seconds'], row['io'], output_bytes
                        capacity = row['quota']['eligible_capacity_cpus']
                        if policy_started and not row['quota']['visible_hierarchy_occupancy_eligible']:
                            # Never carry a prior visible denominator through an unknown interval.
                            policy_visibility_lost = True
                            session.policy.active = False
                            session.dispatch_stop = 'visible CPU quota changed to unavailable; occupancy interval discarded'
                        if row['quota']['visible_hierarchy_occupancy_eligible'] and not policy_started:
                            session.policy.observe(started, cpu_base, capacity, session.completed_matches, True)
                            policy_started = True
                        row['policy'] = (session.policy.observe(time.monotonic(), cpu_base + row['cpu_seconds'],
                            capacity, session.completed_matches, True)
                            if row['quota']['visible_hierarchy_occupancy_eligible'] else
                            {'action': None, 'occupancy_unavailable': 'visible quota hierarchy is incomplete'})
                        row['phase'] = session.phase
                        row['committed_complete_matches'] = session.completed_matches
                        row['committed_bo3_updates'] = session.completed_updates
                        row['output_bytes'] = output_bytes
                        row['retained_bytes'] = retained
                        data = (json.dumps(row, sort_keys=True, allow_nan=False) + '\n').encode()
                        require(telemetry_bytes + len(data) <= 16 * MIB, 'telemetry byte cap reached')
                        records.write(data)
                        telemetry_bytes += len(data)
                        observations += 1
                        if row['policy'].get('diagnosis'):
                            raise ValueError('two low CPU windows require diagnosis before another batch')
                        require(not policy_visibility_lost, session.dispatch_stop)
                # Drain only currently available bytes. Native output files are the semantic evidence.
                for pipe in (group.child.stdout, group.child.stderr):
                    while True:
                        try:
                            block = os.read(pipe.fileno(), 65536)
                        except BlockingIOError:
                            break
                        if not block:
                            break
                        require(log_bytes + len(block) <= session.args.max_log_bytes, 'native log cap reached')
                        log.write(block)
                        log_bytes += len(block)
            finally:
                selector.close()
                # Errors take this path too; close alone only flushes Python buffers.
                log.flush(); os.fsync(log.fileno())
                records.flush(); os.fsync(records.fileno())
                logs_durable = True
    except BaseException as caught:
        error = type(caught).__name__ + ': ' + str(caught)
        stop_reason = error
    finally:
        try:
            if group is not None:
                group.stop()
                if capacity is not None and session.policy.active and not policy_visibility_lost:
                    final_policy = session.policy.observe(time.monotonic(), cpu_base + group.cpu_seconds,
                                                          capacity, session.completed_matches, False)
                    if final_policy.get('diagnosis'):
                        session.dispatch_stop = 'two low CPU windows require diagnosis before another batch'
            session.native_alive, session.phase = False, 'stopped_export'
        finally:
            if group is not None:
                for pipe in (group.child.stdout, group.child.stderr):
                    pipe.close()
    require(group is not None, 'native process could not be created: ' + str(error))
    return group, sampled, {'exit_code': group.returncode, 'native_wall_seconds': time.monotonic() - started,
        'native_cpu_seconds': group.cpu_seconds, 'cpu_accounting': 'wait4 reaped owned group, each PID once',
        'reaped_pids': group.reaped, 'error': error, 'stop_reason': stop_reason, 'runtime_observed': sampled is not None,
        'telemetry_samples': observations, 'log_bytes': log_bytes, 'telemetry_bytes': telemetry_bytes,
        'diagnostic_logs_durable': logs_durable}


def run(args):
    lease, lease_pin, status = validate_args(args)
    session = Session(args, lease, status)
    folder = session.durable / 'worker' / args.name
    require(not folder.exists(), 'fresh worker invocation name required')
    if not args.execute:
        manifest, config, image, command, pins = inputs(args, session)
        return {'schema': 'phase1-bo3-worker-validation/v1', 'native_execution': False, 'inputs': pins,
                'command_per_invocation': command, 'maximum_new_batches': args.max_new_batches}
    import fcntl
    with (session.guard / 'worker.lock').open('a+b') as lock, (session.hot / '.bo3-worker.lock').open('a+b') as hot_lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        fcntl.flock(hot_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # This standalone worker adopts only descendants of its own newly created groups.
        libc = ctypes.CDLL(None, use_errno=True)
        require(libc.prctl(36, 1, 0, 0, 0) == 0, 'Linux child subreaper setup failed')
        folder.mkdir(parents=True)
        result = {'schema': 'phase1-bo3-worker-result/v1', 'pod_id': args.pod_id, 'name': args.name,
                  'mode': 'qualification', 'complete': False, 'recovery_complete': False, 'invocations': [],
                  'lease': lease_pin, 'native_cpu_seconds': 0., 'error': None, 'production_admitted': False}
        exported, last_saved, observations_retained = None, None, False
        group = None
        try:
            manifest, config, image, command, pins = inputs(args, session)
            result['inputs'] = pins
            session.stopped_baseline()
            session.publish(folder / 'start.json', {'schema': 'phase1-bo3-worker-start/v1', 'epoch': session.epoch,
                  'arguments': vars(args), 'inputs': pins, 'lease': lease_pin, 'command_per_invocation': command,
                  'resource_limits': {'native_address_space_bytes': 8 * GIB, 'nice': 10, 'free_reserve_bytes': RESERVE}})
            # A stopped existing prefix is exported before any new dispatch as well.
            session.phase = 'initial_stopped_export'
            session.stopped_baseline()
            with native_stopped_lock(session) as initial_stopped:
                exported = export_stopped(session.hot, session.durable, args.name + '-initial', args.package_sha256,
                    limits=Limits(max_bytes=args.max_output_bytes, max_new_bytes=args.max_retained_bytes),
                    assert_stopped=initial_stopped, heartbeat=session.check, quota=session.quota)
                retained_initial = retain_native_observations(session, None)
                session.publish(folder / 'initial-observations.json', retained_initial)
                observations_retained = True
            initial = exported['summary']['completed_batches']
            session.completed_matches = exported['summary']['complete_matches']
            session.completed_updates = exported['summary']['completed_bo3_updates']
            result['initial_export'] = export_receipt(exported)
            last_saved = export_receipt(exported)
            for index in range(remaining_admission(config, initial, args.max_new_batches)):
                session.check(dispatch=True)
                attempt = folder / f'native-{index:06d}'
                attempt.mkdir()
                old_telemetry = {path.name for path in (session.hot / 'run/telemetry').glob('*.json')}
                # Prior successful export cannot authorize release after this child mutates hot state.
                exported, observations_retained = None, False
                group, sampled, row = run_native(session, manifest, config, image, command, attempt)
                result['invocations'].append(row)
                export_start = time.monotonic()
                with native_stopped_lock(session, group) as stopped:
                    session.stopped_baseline()
                    exported = export_stopped(session.hot, session.durable, f'{args.name}-{index:06d}', args.package_sha256,
                        limits=Limits(max_bytes=args.max_output_bytes, max_new_bytes=args.max_retained_bytes),
                        assert_stopped=stopped, heartbeat=session.check, quota=session.quota)
                    retained = retain_native_observations(session, sampled, old_telemetry)
                    row['stage_timings'] = stage_timings(retained, session.check)
                row['export'] = export_receipt(exported)
                last_saved = row['export']
                row['export_wall_seconds'] = time.monotonic() - export_start
                session.publish(attempt / 'native-result.json', row)
                session.publish(attempt / 'observations.json', retained)
                if sampled is not None:
                    session.publish(attempt / 'runtime-observation.json', sampled)
                observations_retained = row['diagnostic_logs_durable']
                session.last_productive = time.time()
                session.completed_matches = exported['summary']['complete_matches']
                session.completed_updates = exported['summary']['completed_bo3_updates']
                require(row['exit_code'] == 0 and row['error'] is None and row['runtime_observed'],
                        'native failed, stopped, or actual runtime observation unavailable; stopped files were exported')
                require(exported['summary']['completed_batches'] == initial + index + 1,
                        'one native invocation did not complete exactly one planned batch')
                # Package inputs are immutable; hashing is bounded and heartbeated between children.
                _, _, _, _, current_pins = inputs(args, session)
                require(current_pins == pins, 'packaged inputs or supervisor helper bytes changed during invocation')
                if exported['summary']['complete']:
                    break
            result['complete'] = exported['summary']['complete']
            result['worker_limit_reached'] = not result['complete']
        except BaseException as error:
            result['error'] = type(error).__name__ + ': ' + str(error)
        finally:
            result['final_export'] = export_receipt(exported)
            result['last_successful_export'] = last_saved
            result['native_cpu_seconds'] = sum(child.cpu_seconds for child in session.groups)
            result['owned_groups_drained'] = all(child.stopped for child in session.groups)
            result['recovery_complete'] = release_eligible(exported,
                not session.native_alive and result['owned_groups_drained'], observations_retained)
            result['wall_seconds'] = time.monotonic() - session.started
            result['guard_cost_observations'] = session.observed_costs
            result['limitations'] = ['qualification only; no 1.5x acceleration claim or playing-strength result',
                'combined native preparation/update/checkpoint timing remains combined where the native receipt says so',
                'storage and available-memory limits are sampled; native address space is a hard 8 GiB limit',
                'runtime map observation is not numerical parity; CPU quota excludes hidden ancestor entitlement',
                'stage deadline checks cannot interrupt a blocking filesystem call; independent lease guard still owns release']
            # Publish the durable outcome before signalling that the guard may release the Pod.
            result['finish_lease_after_worker'] = True
            if result['owned_groups_drained'] and not session.native_alive:
                # A stopped final accounting read is allowed after the work deadline.
                session.stopped_baseline(final=True)
            result['retained_accounting'] = {'stopped_baseline_bytes': session.recovery_baseline,
                'unaccounted_copy_bytes': session.recovery_written, 'unaccounted_receipt_bytes': session.receipt_written,
                'final_result_reserved_bytes': FINAL_RECEIPT_RESERVE,
                'guard_control_plane': 'outside disjoint hot/durable artifact total; bounded small progress/release receipts',
                'native_output_limit': 'sampled, not a filesystem hard quota'}
            session.publish(folder / 'result.json', result, final=True)
            write(session.guard / 'progress.json', {'pod_id': args.pod_id, 'epoch': time.time(),
                  'last_productive_epoch': session.last_productive, 'last_activity_epoch': time.time(),
                  'native_alive': session.native_alive, 'queued_work': False, 'finished': True,
                  'recovery_failed': not result['recovery_complete']}, replace=True)
            if result['recovery_complete']:
                durable_write(session.guard / 'recovery-complete.json', {'pod_id': args.pod_id, 'epoch': time.time(),
                      'export_index': exported['index'], 'worker_result': file_pin(folder / 'result.json', 16 * MIB)})
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=('qualification', 'production'), required=True)
    for name in ('package-manifest', 'package-sha256', 'runtime-image', 'runtime-image-sha256',
                 'hot-root', 'durable-root', 'guard-state', 'lease', 'lease-sha256', 'pod-id', 'name'):
        parser.add_argument('--' + name, required=True)
    for name in ('stage-seconds', 'native-seconds', 'max-new-batches', 'max-output-bytes', 'max-retained-bytes', 'max-log-bytes'):
        parser.add_argument('--' + name, type=int, required=True)
    parser.add_argument('--execute', action='store_true')
    def interrupted(signum, _frame):
        raise KeyboardInterrupt('worker received signal ' + str(signum))
    signal.signal(signal.SIGTERM, interrupted)
    result = run(parser.parse_args())
    print(encoded(result).decode())
    if result.get('error'):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
