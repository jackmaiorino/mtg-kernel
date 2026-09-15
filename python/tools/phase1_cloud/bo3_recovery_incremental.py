"""Opt-in live-owner prefix reuse; frozen full verification remains the release boundary.

No execution on import. Cache-derived indices describe content identities but
are explicitly ineligible for Pod release/handoff until finish_full succeeds.
"""
from __future__ import annotations

import copy
import hashlib
import os
from pathlib import Path
import re
import stat
import threading

import bo3_recovery as full
from common import encoded, require

FROZEN_SHA256 = 'd7fb74a51f93e6060801cdf54b7bb30ac781f42350938c37162c04155e2340a3'


def signature(info):
    require(info is not None, 'verified entry disappeared')
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns


class OwnerLock:
    """Caller-owned continuous hot-root lock, held across native invocations.

    The native run.lock is separate and must also be held by assert_stopped at
    each export boundary. Direct out-of-band unlock/mutation is outside this
    trusted-owner API; this token cannot attest arbitrary code in its process.
    """
    def __init__(self, hot_root):
        require(os.name == 'posix', 'incremental owner/cache is POSIX-only; use frozen full recovery on Windows')
        self.hot = full._directory(hot_root)
        self.path = self.hot / '.bo3-worker.lock'
        self.handle = None
        self.used = False
        self.pid, self.thread = os.getpid(), threading.get_ident()

    def __enter__(self):
        require(not self.used and self.handle is None and os.getpid() == self.pid and threading.get_ident() == self.thread,
                'fresh owner lock in its original process/thread required')
        self.used = True
        info = full._entry(self.path)
        require(info is None or stat.S_ISREG(info.st_mode), 'owner lock is not a regular file')
        handle = self.path.open('a+b')
        try:
            import fcntl
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.handle = handle
            info = os.fstat(handle.fileno())
            self.identity = info.st_dev, info.st_ino
            self.check()
        except BaseException:
            handle.close()
            self.handle = None
            raise
        return self

    def check(self):
        require(self.handle is not None and not self.handle.closed and os.getpid() == self.pid
                and threading.get_ident() == self.thread, 'continuous owner lock token was lost or transferred')
        info = full._entry(self.path)
        require(info is not None and stat.S_ISREG(info.st_mode) and (info.st_dev, info.st_ino) == self.identity
                and (os.fstat(self.handle.fileno()).st_dev, os.fstat(self.handle.fileno()).st_ino) == self.identity,
                'owner lock path/descriptor was replaced')

    def __exit__(self, *_):
        if self.handle is not None:
            self.handle.close()
            self.handle = None


class Operation:
    """Expose monotonic total byte counters across frozen and successor phases."""
    def __init__(self, session):
        self.session = session
        self.read = self.written = self.base_read = self.base_written = 0
        self.hits = self.reused_bytes = 0

    def quota(self, event):
        self.read = self.base_read + event['read_bytes']
        self.written = self.base_written + event['written_bytes']
        require(self.written + event['next_write_bytes'] <= self.session.limits.max_new_bytes,
                'whole incremental operation write allowance exhausted')
        self.session.quota({**event, 'read_bytes': self.read, 'written_bytes': self.written})

    def stopped(self):
        self.session.check()
        self.session.assert_stopped()

    def keywords(self):
        self.base_read, self.base_written = self.read, self.written
        return {'limits': self.session.limits, 'assert_stopped': self.stopped,
                'heartbeat': self.session.heartbeat, 'quota': self.quota}

    def guard(self):
        return full._Guard(**self.keywords())

    def metrics(self, full_verified):
        return {'read_bytes': self.read, 'written_bytes': self.written,
                'cached_file_reuses': self.hits, 'cached_logical_bytes_reused': self.reused_bytes,
                'io_strategy': 'full source/CAS verification' if full_verified else 'live-owner committed-prefix metadata reuse',
                'full_source_and_cas_verified_this_export': full_verified,
                'release_eligible': full_verified, 'handoff_eligible': full_verified,
                'long_stream_export_throughput_qualified': False,
                'callback_scope': 'every hash/copy chunk and metadata step; blocking OS calls are not interruptible'}


def empty_tip(config):
    return {'learner': copy.deepcopy(config['initial_input']['learner']), 'previous': None,
            'ledger': [], 'receipts': [], 'updates': 0, 'attempted_batches': 0, 'attempted_matches': 0,
            'complete_matches': 0, 'incomplete_matches': 0, 'eligible_matches': 0, 'dispositions': []}


def inspect_suffix(run, files, config, guard, previous_tip, verified_run_manifest):
    """The frozen _inspect checks, starting from the exact private verified tip.

    This retains only the latest ledger, not decoded historical progress files.
    New/pending bytes have already been fully hashed by the inventory step.
    """
    require(config['schema'] == 'mtg-kernel-native-bo3-training-run/v1'
            and full._same_root(config['output_directory'], run), 'native config/root differs')
    require(config['previous_progress'] is None and config['initial_input']['kind'] == 'ordinary_checkpoint_transition'
            and config['initial_input']['learner'] == config['initial_learner']['gameplay'], 'ordinary origin differs')
    require(1 <= len(config['batches']) <= 4096, 'invalid finite BO3 schedule')
    tip = copy.deepcopy(previous_tip)
    def document(relative, maximum=32 * full.MIB):
        require(relative in files, 'missing indexed native metadata: ' + relative)
        return full._json(run / relative, guard, maximum, files[relative])[0]
    if 'run.json' in files and not verified_run_manifest:
        saved = document('run.json', 16 * full.MIB)
        require(saved['schema'] == 'mtg-kernel-native-bo3-run-manifest/v1' and saved['config'] == config,
                'native saved plan differs from packaged request')
    completed = sorted(name for name in files if re.fullmatch(r'batches/\d{6}/complete.json', name))
    require(completed == [f'batches/{i:06d}/complete.json' for i in range(len(completed))]
            and tip['attempted_batches'] <= len(completed) <= len(config['batches'])
            and (not completed or 'run.json' in files), 'BO3 prefix regressed or has a gap')
    for index in range(tip['attempted_batches'], len(completed)):
        relative = completed[index]
        receipt = document(relative, 16 * full.MIB)
        require(receipt['schema'] == 'mtg-kernel-native-bo3-batch/v1' and receipt['batch'] == index
                and receipt['input']['learner'] == tip['learner'] and receipt['previous_progress'] == tip['previous'],
                'new batch input/progress differs from verified prefix')
        require(receipt['input']['kind'] == ('bo3_checkpoint' if tip['updates'] else 'ordinary_checkpoint_transition'),
                'BO3 objective transition tag differs')
        require(full._pin_relative(receipt['manifest'], run, files) == 'run.json', 'batch manifest pin differs')
        attempts = receipt['attempts']
        require(len(attempts) == len(config['batches'][index]['matches']) and 1 <= len(attempts) <= 32,
                'attempt count differs from schedule')
        for item in attempts:
            full._pin_relative(item['request'], run, files)
            full._pin_relative(item['result'], run, files)
            require(item['learner_seat'] in ('p0', 'p1'), 'invalid learner seat')
        preparation = document(full._pin_relative(receipt['preparation_request'], run, files), full.MIB)
        operation = document(full._pin_relative(receipt['update_request'], run, files), full.MIB)
        require(preparation['schema'] == 'mtg-kernel-bo3-gameplay-preparation/v1'
                and preparation['learner'] == tip['learner'] and preparation['attempts'] == attempts
                and preparation['limits'] == config['preparation_limits'], 'preparation edge differs')
        require(operation['schema'] == 'mtg-kernel-bo3-gameplay-update-request/v1'
                and operation['input'] == receipt['input'] and operation['previous_progress'] == tip['previous']
                and operation['preparation_request'] == receipt['preparation_request']
                and operation['learning_rate_bits'] == config['learning_rate_bits']
                and operation['value_coefficient_bits'] == config['value_coefficient_bits'], 'update edge differs')
        result = receipt['result']
        progress_relative = full._pin_relative(result['progress'], run, files)
        require(Path(operation['output_directory']) / 'progress.json' == run / progress_relative, 'progress location differs')
        progress = document(progress_relative)
        original = document(full._pin_relative(progress['operation_request'], run, files), full.MIB)
        require(progress['schema'] == 'mtg-kernel-bo3-gameplay-progress/v1' and progress['request'] == operation == original
                and progress['result'] == result['learner'], 'native progress/result differs')
        report, next_ledger = progress['attempt_progress']['preparation'], progress['attempt_progress']['ledger']
        require(report['disposition'] in ('ready', 'no_update') and type(result['optimizer_updated']) is bool
                and result['optimizer_updated'] == (report['disposition'] == 'ready'), 'Ready/NoUpdate differs')
        ready = result['optimizer_updated']
        counts = {name: full._integer(report[name], 32) for name in ('complete_matches', 'incomplete_matches', 'eligible_matches')}
        require(len(report['attempts']) == len(attempts) and (counts['eligible_matches'] > 0) == ready
                and counts['complete_matches'] + counts['incomplete_matches'] == len(attempts)
                and counts['eligible_matches'] <= counts['complete_matches'], 'new match accounting differs')
        old_ledger = tip['ledger']
        require(len(next_ledger) == len(old_ledger) + len(attempts) and next_ledger[:len(old_ledger)] == old_ledger,
                'new attempt ledger does not preserve verified prefix')
        for triple, item in zip(next_ledger[len(old_ledger):], attempts):
            require(isinstance(triple, list) and len(triple) == 3
                    and triple[:2] == [item['request']['sha256'], item['result']['sha256']]
                    and all(isinstance(x, str) and full.HEX.fullmatch(x) for x in triple), 'new attempt ledger suffix differs')
        require(all(len({triple[column] for triple in next_ledger}) == len(next_ledger) for column in range(3)),
                'duplicate attempted identity')
        tip['updates'] += int(ready); tip['attempted_batches'] += 1; tip['attempted_matches'] += len(attempts)
        require(tuple(full._integer(result[key]) for key in ('completed_bo3_updates', 'attempted_batches', 'attempted_matches'))
                == (tip['updates'], tip['attempted_batches'], tip['attempted_matches'])
                and progress['completed_bo3_updates'] == tip['updates']
                and progress['attempt_progress']['attempted_batches'] == tip['attempted_batches'], 'native counters differ')
        require(result['learner']['identity']['adam_step'] == tip['learner']['identity']['adam_step'] + int(ready)
                and (ready or result['learner'] == tip['learner']), 'NoUpdate changed state or Adam delta differs')
        if ready:
            full._pin_relative(result['learner']['source']['checkpoint'], run, files)
            full._pin_relative(result['learner']['source']['play_import'], run, files)
        for name, count in counts.items():
            tip[name] += count
        tip['dispositions'].append(report['disposition'])
        tip['learner'], tip['previous'], tip['ledger'] = result['learner'], result['progress'], next_ledger
        tip['receipts'].append({'path': str(run / relative), 'sha256': files[relative]['sha256']})
    final = 'completion.json' in files
    if final:
        result = document('completion.json', 16 * full.MIB)
        require(result['schema'] == 'mtg-kernel-native-bo3-run-result/v1' and result['complete'] is True
                and len(completed) == len(config['batches']) == result['completed_batches'] == result['planned_batches']
                and result['learner'] == tip['learner'] and result['previous_progress'] == tip['previous']
                and (result['completed_bo3_updates'], result['attempted_batches'], result['attempted_matches'])
                == (tip['updates'], tip['attempted_batches'], tip['attempted_matches']), 'final completion differs')
        require(len(result['batch_receipts']) == len(tip['receipts']), 'final receipt count differs')
        for actual, expected in zip(result['batch_receipts'], tip['receipts']):
            require(full._same_root(actual['path'], expected['path']) and actual['sha256'] == expected['sha256'],
                    'final receipt pin differs')
    pending = [name for name in files if name.startswith(f'batches/{len(completed):06d}/')]
    require(all(int(match[1]) <= len(completed) for name in files if (match := re.match(r'batches/(\d{6})/', name))),
            'future batch artifacts exceed first incomplete batch')
    summary = {'completed_batches': len(completed), 'planned_batches': len(config['batches']),
        'completed_bo3_updates': tip['updates'], 'attempted_batches': tip['attempted_batches'],
        'attempted_matches': tip['attempted_matches'], 'adam_step': tip['learner']['identity']['adam_step'],
        **{name: tip[name] for name in ('complete_matches', 'incomplete_matches', 'eligible_matches')},
        'committed_match_totals_only': True, 'batch_dispositions': tip['dispositions'], 'complete': final,
        'pending_checkpoint_files': sorted(name for name in pending if name.endswith('/checkpoint.json')),
        'pending_progress_files': sorted(name for name in pending if name.endswith('/progress.json')),
        'native_state_validation': 'required after restore; not performed by file recovery'}
    require(len(encoded(tip)) <= 32 * full.MIB, 'latest private prefix metadata exceeds 32 MiB')
    return summary, tip


class VerifiedPrefixSession:
    def __init__(self, owner, durable_root, package_sha256, *, limits=full.Limits(), assert_stopped, heartbeat, quota):
        require(isinstance(owner, OwnerLock), 'continuously held OwnerLock token required')
        owner.check(); limits.validate()
        require(all(callable(x) for x in (assert_stopped, heartbeat, quota)), 'supervision callbacks required')
        self.owner, self.hot, self.durable = owner, owner.hot, Path(durable_root)
        self.package_sha256, self.limits = package_sha256, limits
        self.assert_stopped, self.heartbeat, self.quota = assert_stopped, heartbeat, quota
        self.pid, self.thread, self.usable = os.getpid(), threading.get_ident(), True
        self.index = self.index_pin = self.config = self.tip = None
        self.cached_files, self.cached_dirs, self.cached_blobs = {}, {}, {}

    def check(self):
        require(os.name == 'posix' and self.usable and self.pid == os.getpid() and self.thread == threading.get_ident(),
                'cache is invalid or crossed a process/thread boundary; full verification required')
        self.owner.check()

    def _helper(self, guard):
        require(full._hash(Path(full.__file__), guard, full.MIB)['sha256'] == FROZEN_SHA256,
                'frozen recovery helper source differs')

    def _remember(self, index, tip, config, guard):
        count = index['summary']['completed_batches']
        def committed(name):
            match = re.match(r'batches/(\d{6})(?:/|$)', name)
            return name == 'run.json' or bool(match and int(match[1]) < count)
        files, directories, blobs = {}, {}, {}
        for name, row in index['files'].items():
            guard.check('cache-remember-file', name)
            if committed(name):
                files[name] = {'row': copy.deepcopy(row), 'signature': signature(full._entry(self.hot / 'run' / name))}
                blob = self.durable / 'blobs' / row['sha256']
                blobs[row['sha256']] = {'row': copy.deepcopy(row), 'signature': signature(full._entry(blob))}
        for name in index['directories']:
            guard.check('cache-remember-directory', name)
            if committed(name):
                directories[name] = signature(full._entry(self.hot / 'run' / name))
        self.cached_files, self.cached_dirs, self.cached_blobs = files, directories, blobs
        self.index, self.tip, self.config = copy.deepcopy(index), copy.deepcopy(tip), copy.deepcopy(config)

    def _full(self, generation):
        operation = Operation(self)
        guard = operation.guard(); guard.check('full-start'); self._helper(guard)
        if self.index is not None:
            previous = full._index(self.durable, self.index['generation'], self.index_pin['sha256'], guard)
            require(previous == self.index, 'prior immutable index changed before final full verification')
        result = full.export_stopped(self.hot, self.durable, generation, self.package_sha256, **operation.keywords())
        guard = operation.guard()
        index = full._index(self.durable, generation, result['index']['sha256'], guard)
        if self.index is not None:
            # Fresh full hashes must also preserve this session's already verified
            # committed bytes, including otherwise unreferenced inherited stages.
            count = self.index['summary']['completed_batches']
            def old_scope(name):
                match = re.match(r'batches/(\d{6})(?:/|$)', name)
                return (name == 'run.json' and name in self.cached_files) or bool(match and int(match[1]) < count)
            require({name: row for name, row in index['files'].items() if old_scope(name)}
                    == {name: item['row'] for name, item in self.cached_files.items()}
                    and {name for name in index['directories'] if old_scope(name)} == set(self.cached_dirs),
                    'full verification found altered committed prefix; new index retained but not accepted')
        config, package = full._package(self.hot, self.package_sha256, guard)
        require(package == index['package_manifest'], 'full export package differs')
        summary, tip = inspect_suffix(self.hot / 'run', index['files'], config, guard, empty_tip(config), False)
        require(summary == index['summary'], 'successor prefix validator differs from frozen full exporter')
        self._remember(index, tip, config, guard)
        self.index_pin = copy.deepcopy(result['index'])
        guard.check('full-finished')
        return {**result, **operation.metrics(True)}

    def start_full(self, generation):
        require(self.index is None, 'session already started')
        try:
            return self._full(generation)
        except BaseException:
            self.usable = False
            raise

    def finish_full(self, generation):
        require(self.index is not None, 'initial full verification required')
        try:
            return self._full(generation)
        except BaseException:
            self.usable = False
            raise

    def _inventory(self, guard, operation):
        run = self.hot / 'run'
        files, directories, identities, total = {}, [], {}, 0
        if full._entry(run) is None:
            require(not self.cached_files, 'verified run disappeared')
            return files, directories, identities
        pending = [full._directory(run)]
        while pending:
            directory = pending.pop()
            guard.check('incremental-directory', directory)
            for child in sorted(directory.iterdir()):
                name = child.relative_to(run).as_posix()
                if full._excluded(name):
                    continue
                full._relative(name)
                require(len(identities) < self.limits.max_entries, 'incremental entry limit exceeded')
                info = full._entry(child); identities[name] = signature(info)
                if stat.S_ISDIR(info.st_mode):
                    directories.append(name); pending.append(child)
                else:
                    require(stat.S_ISREG(info.st_mode) and len(files) < self.limits.max_files
                            and info.st_size <= self.limits.max_file_bytes, 'bounded regular run file required')
                    total += info.st_size
                    require(total <= self.limits.max_bytes, 'incremental byte cap exceeded')
                    if name in self.cached_files:
                        cached = self.cached_files[name]
                        require(identities[name] == cached['signature'], 'committed source changed: ' + name)
                        files[name] = copy.deepcopy(cached['row'])
                        guard.observed_sources[child] = full._signature(info)
                        operation.hits += 1; operation.reused_bytes += info.st_size
                    else:
                        files[name] = full._hash(child, guard, self.limits.max_file_bytes)
        count = self.index['summary']['completed_batches']
        def old_scope(name):
            match = re.match(r'batches/(\d{6})(?:/|$)', name)
            return (name == 'run.json' and name in self.cached_files) or bool(match and int(match[1]) < count)
        require({name for name in files if old_scope(name)} == set(self.cached_files)
                and {name for name in directories if old_scope(name)} == set(self.cached_dirs),
                'committed prefix membership changed')
        for name, old in self.cached_dirs.items():
            require(identities[name] == old, 'committed directory changed: ' + name)
        return files, sorted(directories), identities

    def export_next(self, generation, prior_index_pin):
        require(self.index is not None, 'initial full verification required')
        try:
            return self._next(generation, prior_index_pin)
        except BaseException:
            self.usable = False
            raise

    def _next(self, generation, prior_index_pin):
        operation = Operation(self); guard = operation.guard()
        guard.check('incremental-start'); self._helper(guard)
        require(prior_index_pin == self.index_pin, 'exact last immutable index pin required')
        previous = full._index(self.durable, self.index['generation'], self.index_pin['sha256'], guard)
        require(previous == self.index, 'prior immutable index changed')
        require(isinstance(generation, str) and re.fullmatch('[a-z0-9][a-z0-9-]{0,63}', generation)
                and generation != self.index['generation'], 'fresh incremental generation required')
        full._directory(self.hot); full._directory(self.durable)
        require(not self.hot.is_relative_to(self.durable) and not self.durable.is_relative_to(self.hot), 'separate roots required')
        config, package = full._package(self.hot, self.package_sha256, guard)
        require(config == self.config and package == self.index['package_manifest'], 'same namespace/package/config required')
        files, directories, identities = self._inventory(guard, operation)
        require(sum(row['bytes'] for row in files.values()) + package['bytes'] <= self.limits.max_bytes,
                'incremental indexed payload limit exceeded')
        summary, tip = inspect_suffix(self.hot / 'run', files, config, guard, self.tip, 'run.json' in self.cached_files)
        index = {'schema': full.INDEX_SCHEMA, 'generation': generation, 'hot_root': str(self.hot),
                 'package_manifest': package, 'directories': directories, 'files': files,
                 'indexed_bytes': sum(row['bytes'] for row in files.values()), 'file_count': len(files), 'summary': summary}
        require(len(encoded(index)) <= self.limits.max_index_bytes, 'incremental index exceeds bound')
        temporary, blobs = self.durable / '.copy-tmp', self.durable / 'blobs'
        new_bytes = full._copy(self.hot / 'manifest.json', blobs / package['sha256'], package, temporary, guard)
        checked_blobs = {}
        for name in sorted(files, key=full._rank):
            row = files[name]; target = blobs / row['sha256']
            guard.check('incremental-blob', target)
            if row['sha256'] in self.cached_blobs:
                cached = self.cached_blobs[row['sha256']]
                actual = signature(full._entry(target))
                require(cached['row'] == row and actual == cached['signature'], 'verified CAS blob changed')
                checked_blobs[target] = actual
                operation.hits += 1; operation.reused_bytes += row['bytes']
            else:
                new_bytes += full._copy(self.hot / 'run' / name, target, row, temporary, guard)
                checked_blobs[target] = signature(full._entry(target))
        final_files, final_dirs = full._inventory(self.hot / 'run', guard, False)
        require(final_files == {name: row['bytes'] for name, row in files.items()} and final_dirs == directories,
                'run membership changed before incremental publication')
        for name, before in identities.items():
            guard.check('incremental-source-final', name)
            require(signature(full._entry(self.hot / 'run' / name)) == before, 'source changed during export')
        for path, before in checked_blobs.items():
            guard.check('incremental-blob-final', path)
            require(signature(full._entry(path)) == before, 'CAS changed during export')
        path = self.durable / 'indices' / (generation + '.json')
        require(full._entry(path) is None, 'fresh index publication required')
        pin = full._publish_index(path, index, temporary, guard)
        self._remember(index, tip, config, guard)
        self.index_pin = {'path': str(path), **pin}
        guard.check('incremental-finished')
        return {'index': copy.deepcopy(self.index_pin), 'summary': copy.deepcopy(summary),
                'indexed_bytes': index['indexed_bytes'], 'new_blob_bytes': new_bytes, **operation.metrics(False)}
