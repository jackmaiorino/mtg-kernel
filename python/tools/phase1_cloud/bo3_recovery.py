"""Stopped-process BO3 export/restore. No native or cloud action on import.

The caller owns and reaps the native process before entry and keeps it stopped
until return. Callbacks enforce that ownership, lease/deadline heartbeats and
cumulative storage reserves. This is file recovery, not native state validation.

Each export rehashes the full logical run and existing CAS blobs and rereads
compact progressive ledgers. CAS prevents repeated storage copies, not growing
verification I/O. Repeated full-prefix exports can have quadratic total read
cost across a long run. This bounded qualification implementation does not
qualify production export throughput; prior-index incremental validation needs
a separately reviewed successor. Return values expose actual read/write bytes.
Linux new directory entries and publication parents are fsynced. Windows cannot
make the same directory-entry power-loss durability claim. A blocking filesystem
call can outlast a callback deadline; the independent lease guard remains needed.
"""
from __future__ import annotations

from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import uuid

from common import encoded, require, unique_pairs

MIB = 1024 ** 2
INDEX_SCHEMA = 'phase1-bo3-recovery-index/v1'
HEX = re.compile(r'[0-9a-f]{64}')


@dataclass(frozen=True)
class Limits:
    max_bytes: int = 100 * 1024 ** 3
    max_new_bytes: int = 100 * 1024 ** 3
    max_files: int = 100_000
    max_entries: int = 200_000
    max_file_bytes: int = 512 * MIB
    max_index_bytes: int = 16 * MIB
    chunk_bytes: int = MIB

    def validate(self):
        for value in vars(self).values():
            require(type(value) is int and value > 0, 'positive integer recovery limits required')
        require(self.chunk_bytes <= MIB and self.max_index_bytes <= 32 * MIB,
                'recovery chunk/index metadata limit is too large')


class _Guard:
    def __init__(self, limits, assert_stopped, heartbeat, quota):
        limits.validate()
        require(all(callable(x) for x in (assert_stopped, heartbeat, quota)),
                'stopped-process, heartbeat and cumulative quota callbacks are required')
        self.limits = limits
        self.assert_stopped, self.heartbeat, self.quota = assert_stopped, heartbeat, quota
        self.read_bytes = self.written_bytes = 0
        self.observed_sources = {}

    def check(self, phase, path=None, next_write_bytes=0):
        self.assert_stopped()
        self.heartbeat()
        require(self.written_bytes + next_write_bytes <= self.limits.max_new_bytes,
                'per-operation recovery write allowance exhausted')
        self.quota({'phase': phase, 'path': None if path is None else str(path),
                    'read_bytes': self.read_bytes, 'written_bytes': self.written_bytes,
                    'next_write_bytes': next_write_bytes})

    def result(self):
        return {'read_bytes': self.read_bytes, 'written_bytes': self.written_bytes,
                'io_strategy': 'full historical source/CAS verification and progressive metadata reads',
                'long_stream_export_throughput_qualified': False,
                'callback_scope': 'before/after streaming chunks and publication; blocking OS calls are not interruptible'}


def _entry(path):
    try:
        info = path.lstat()
    except FileNotFoundError:
        return None
    require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0)
            & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0), 'recovery links/reparse points are unsupported')
    return info


def _directory(path, create=False, guard=None):
    path = Path(path)
    require(path.is_absolute(), 'absolute recovery root required')
    require('..' not in path.parts, 'canonical recovery root required')
    for component in reversed([path, *path.parents]):
        info = _entry(component)
        if info is None:
            require(create, 'recovery directory is missing: ' + str(component))
            require(guard is not None, 'new recovery directories require supervision')
            guard.check('directory-create', component)
            component.mkdir()
            _sync_directory(component.parent, guard)
        else:
            require(stat.S_ISDIR(info.st_mode), 'recovery parent is not a directory')
    return path


def _relative(text):
    require(isinstance(text, str) and text and '\\' not in text and ':' not in text,
            'unsafe recovery relative path')
    path = PurePosixPath(text)
    require(str(path) != '.' and not path.is_absolute() and '..' not in path.parts and '.' not in path.parts
            and str(path) == text, 'noncanonical recovery relative path')
    return text


def _same_root(a, b):
    # Path comparison handles Windows separators without rewriting stored JSON.
    return Path(a) == Path(b) and Path(a).is_absolute()


def _open_source(path, maximum):
    _directory(path.parent)
    info = _entry(path)
    require(info is not None and stat.S_ISREG(info.st_mode) and info.st_size <= maximum,
            'bounded regular recovery file required: ' + str(path))
    stream = path.open('rb')
    opened = os.fstat(stream.fileno())
    if (opened.st_dev, opened.st_ino, opened.st_size) != (info.st_dev, info.st_ino, info.st_size):
        stream.close()
        raise ValueError('recovery source changed while opening')
    return stream, info


def _signature(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


def _hash(path, guard, maximum, collect=False):
    guard.check('hash-open', path)
    stream, before = _open_source(path, maximum)
    if path in guard.observed_sources and guard.observed_sources[path] != _signature(before):
        stream.close()
        raise ValueError('source changed between stopped recovery reads')
    guard.observed_sources[path] = _signature(before)
    hasher, chunks, size = hashlib.sha256(), [], 0
    with stream:
        while True:
            guard.check('hash-read', path)
            chunk = stream.read(guard.limits.chunk_bytes)
            if not chunk:
                break
            size += len(chunk)
            require(size <= maximum, 'recovery file grew beyond limit')
            hasher.update(chunk)
            if collect:
                chunks.append(chunk)
            guard.read_bytes += len(chunk)
            guard.check('hash-complete-chunk', path)
        require(_signature(os.fstat(stream.fileno())) == _signature(before),
                'source changed during stopped recovery read')
    require(size == before.st_size and _signature(_entry(path)) == _signature(before),
            'source changed during stopped recovery read')
    row = {'sha256': hasher.hexdigest(), 'bytes': size}
    return (row, b''.join(chunks)) if collect else row


def _json(path, guard, maximum=32 * MIB, expected=None):
    row, content = _hash(path, guard, maximum, True)
    require(expected is None or row == expected, 'metadata pin differs')
    def invalid(_):
        raise ValueError('non-finite recovery JSON')
    return json.loads(content, object_pairs_hook=unique_pairs, parse_constant=invalid), row


def _sync_directory(path, guard):
    guard.check('directory-sync', path)
    if os.name != 'nt':
        fd = os.open(path, os.O_RDONLY | getattr(os, 'O_DIRECTORY', 0))
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    # Windows intentionally makes no directory-entry power-loss guarantee.
    guard.check('directory-synced', path)


def _copy(source, target, row, temporary_root, guard):
    """No replace. Temporary files are outside logical native/index paths."""
    guard.check('copy-start', target)
    if _entry(target) is not None:
        require(_hash(target, guard, guard.limits.max_file_bytes) == row,
                'existing recovery destination differs: ' + str(target))
        return 0
    _directory(target.parent, True, guard)
    _directory(temporary_root, True, guard)
    temporary = temporary_root / (uuid.uuid4().hex + '.partial')
    stream, before = _open_source(source, guard.limits.max_file_bytes)
    digest, size = hashlib.sha256(), 0
    with stream, temporary.open('xb') as output:
        while True:
            guard.check('copy-read', source)
            chunk = stream.read(guard.limits.chunk_bytes)
            if not chunk:
                break
            require(size + len(chunk) <= row['bytes'], 'copy source grew beyond indexed bytes')
            guard.read_bytes += len(chunk)
            guard.check('copy-write', target, len(chunk))
            require(output.write(chunk) == len(chunk), 'short recovery write')
            guard.written_bytes += len(chunk)
            digest.update(chunk)
            size += len(chunk)
            guard.check('copy-written', target)
        require(_signature(os.fstat(stream.fileno())) == _signature(before), 'copy source changed')
        require({'sha256': digest.hexdigest(), 'bytes': size} == row, 'copy source pin differs')
        guard.check('copy-fsync', temporary)
        output.flush()
        os.fsync(output.fileno())
        guard.check('copy-fsynced', temporary)
    require(_signature(_entry(source)) == _signature(before), 'copy source was replaced')
    # Read back even a newly written file under the same callbacks.
    require(_hash(temporary, guard, guard.limits.max_file_bytes) == row, 'copied bytes differ on readback')
    guard.check('copy-publish', target)
    created = True
    try:
        os.link(temporary, target)
    except FileExistsError:
        require(_hash(target, guard, guard.limits.max_file_bytes) == row, 'publication collision differs')
        created = False
    _sync_directory(target.parent, guard)
    temporary.unlink()  # Only this invocation's verified copy, never old debris.
    _sync_directory(temporary_root, guard)
    return size if created else 0


def _publish_index(path, value, temporary_root, guard):
    content = encoded(value)
    require(len(content) <= guard.limits.max_index_bytes, 'recovery index exceeds metadata limit')
    row = {'sha256': hashlib.sha256(content).hexdigest(), 'bytes': len(content)}
    if _entry(path) is not None:
        require(_hash(path, guard, guard.limits.max_index_bytes) == row, 'existing index differs')
        return row
    _directory(path.parent, True, guard)
    _directory(temporary_root, True, guard)
    temporary = temporary_root / (uuid.uuid4().hex + '.index-partial')
    with temporary.open('xb') as stream:
        for offset in range(0, len(content), guard.limits.chunk_bytes):
            chunk = content[offset:offset + guard.limits.chunk_bytes]
            guard.check('index-write', path, len(chunk))
            require(stream.write(chunk) == len(chunk), 'short index write')
            guard.written_bytes += len(chunk)
        guard.check('index-fsync', path)
        stream.flush()
        os.fsync(stream.fileno())
    guard.check('index-publish', path)
    os.link(temporary, path)  # A final index is never overwritten.
    _sync_directory(path.parent, guard)
    temporary.unlink()
    _sync_directory(temporary_root, guard)
    return row


def _excluded(relative):
    return relative == 'run.lock' or relative == 'telemetry' or relative.startswith('telemetry/')


def _inventory(run, guard, hashes=True):
    files, directories, total, entries = {}, [], 0, 0
    if _entry(run) is None:
        return files, directories
    _directory(run)
    pending = [run]
    while pending:
        directory = pending.pop()
        guard.check('inventory-directory', directory)
        for child in sorted(directory.iterdir()):
            relative = child.relative_to(run).as_posix()
            if _excluded(relative):
                continue
            _relative(relative)
            entries += 1
            require(entries <= guard.limits.max_entries, 'recovery directory-entry limit exceeded')
            info = _entry(child)
            require(info is not None, 'stopped recovery entry disappeared')
            if stat.S_ISDIR(info.st_mode):
                directories.append(relative)
                pending.append(child)
            else:
                require(stat.S_ISREG(info.st_mode), 'nonregular recovery entry')
                require(len(files) < guard.limits.max_files, 'recovery file-count limit exceeded')
                total += info.st_size
                require(total <= guard.limits.max_bytes, 'recovery indexed byte limit exceeded')
                if hashes:
                    files[relative] = _hash(child, guard, guard.limits.max_file_bytes)
                else:
                    require(child in guard.observed_sources and guard.observed_sources[child] == _signature(info),
                            'source membership/content metadata changed before index publication')
                    files[relative] = info.st_size
    return files, sorted(directories)


def _pin_relative(pin, logical_run, files):
    require(isinstance(pin, dict) and set(pin) == {'path', 'sha256'}, 'strict native pin required')
    path = Path(pin['path'])
    require(path.is_absolute(), 'native pin must be absolute')
    try:
        relative = path.relative_to(logical_run).as_posix()
    except ValueError as error:
        raise ValueError('native output pin is outside stable run root') from error
    _relative(relative)
    require(relative in files and pin['sha256'] == files[relative]['sha256'], 'native output pin not in exact inventory')
    return relative


def _integer(value, maximum=65536):
    require(type(value) is int and 0 <= value <= maximum, 'invalid BO3 progress counter')
    return value


def _inspect(run, logical_run, files, config, guard):
    """Saved structural receipts only; native owns tensors, replay and strength."""
    require(config['schema'] == 'mtg-kernel-native-bo3-training-run/v1'
            and _same_root(config['output_directory'], logical_run), 'native config/root differs')
    require(config['previous_progress'] is None
            and config['initial_input']['kind'] == 'ordinary_checkpoint_transition',
            'current BO3 cloud package requires its original ordinary-origin chain')
    learner, previous = config['initial_input']['learner'], None
    require(learner == config['initial_learner']['gameplay'], 'initial learner differs')
    batches = config['batches']
    require(1 <= len(batches) <= 4096, 'invalid finite BO3 schedule')
    def document(relative, maximum=32 * MIB):
        require(relative in files, 'missing indexed native metadata: ' + relative)
        return _json(run / relative, guard, maximum, files[relative])[0]
    if 'run.json' in files:
        saved = document('run.json', 16 * MIB)
        require(saved['schema'] == 'mtg-kernel-native-bo3-run-manifest/v1'
                and saved['config'] == config, 'native saved plan differs from packaged request')
    completed = sorted(name for name in files if re.fullmatch(r'batches/\d{6}/complete.json', name))
    require(completed == [f'batches/{i:06d}/complete.json' for i in range(len(completed))]
            and len(completed) <= len(batches) and (not completed or 'run.json' in files),
            'BO3 completed batches are not a contiguous planned prefix')
    updates = attempted_batches = attempted_matches = 0
    complete_matches = incomplete_matches = eligible_matches = 0
    ledger, receipt_pins, dispositions = [], [], []
    for index, relative in enumerate(completed):
        receipt = document(relative, 16 * MIB)
        require(receipt['schema'] == 'mtg-kernel-native-bo3-batch/v1' and receipt['batch'] == index
                and receipt['input']['learner'] == learner and receipt['previous_progress'] == previous,
                'BO3 batch input/progress edge differs')
        require(receipt['input']['kind'] == ('bo3_checkpoint' if updates else 'ordinary_checkpoint_transition'),
                'BO3 objective transition tag differs')
        require(_pin_relative(receipt['manifest'], logical_run, files) == 'run.json', 'batch manifest pin differs')
        attempts = receipt['attempts']
        require(len(attempts) == len(batches[index]['matches']) and 1 <= len(attempts) <= 32,
                'attempt count differs from scheduled matches')
        for item in attempts:
            _pin_relative(item['request'], logical_run, files)
            _pin_relative(item['result'], logical_run, files)
            require(item['learner_seat'] in ('p0', 'p1'), 'invalid learner seat')
        preparation = document(_pin_relative(receipt['preparation_request'], logical_run, files), MIB)
        operation = document(_pin_relative(receipt['update_request'], logical_run, files), MIB)
        require(preparation['schema'] == 'mtg-kernel-bo3-gameplay-preparation/v1'
                and preparation['learner'] == learner and preparation['attempts'] == attempts
                and preparation['limits'] == config['preparation_limits'], 'preparation edge differs')
        require(operation['schema'] == 'mtg-kernel-bo3-gameplay-update-request/v1'
                and operation['input'] == receipt['input'] and operation['previous_progress'] == previous
                and operation['preparation_request'] == receipt['preparation_request']
                and operation['learning_rate_bits'] == config['learning_rate_bits']
                and operation['value_coefficient_bits'] == config['value_coefficient_bits'], 'update edge differs')
        result = receipt['result']
        progress_relative = _pin_relative(result['progress'], logical_run, files)
        require(Path(operation['output_directory']) / 'progress.json' == logical_run / progress_relative,
                'progress location differs')
        progress = document(progress_relative)
        original_operation = document(_pin_relative(progress['operation_request'], logical_run, files), MIB)
        require(progress['schema'] == 'mtg-kernel-bo3-gameplay-progress/v1'
                and progress['request'] == operation == original_operation
                and progress['result'] == result['learner'], 'saved native progress/result differs')
        report, next_ledger = progress['attempt_progress']['preparation'], progress['attempt_progress']['ledger']
        require(report['disposition'] in ('ready', 'no_update') and type(result['optimizer_updated']) is bool
                and result['optimizer_updated'] == (report['disposition'] == 'ready'), 'Ready/NoUpdate disposition differs')
        ready = result['optimizer_updated']
        n_complete, n_incomplete, n_eligible = (_integer(report[name], 32)
                for name in ('complete_matches', 'incomplete_matches', 'eligible_matches'))
        require(len(report['attempts']) == len(attempts) and (n_eligible > 0) == ready
                and n_complete + n_incomplete == len(attempts) and n_eligible <= n_complete,
                'preparation disposition/attempt counts differ')
        complete_matches += n_complete; incomplete_matches += n_incomplete; eligible_matches += n_eligible
        require(len(next_ledger) == len(ledger) + len(attempts) and next_ledger[:len(ledger)] == ledger,
                'attempt ledger does not extend exact prefix')
        for triple, item in zip(next_ledger[len(ledger):], attempts):
            require(isinstance(triple, list) and len(triple) == 3
                    and triple[:2] == [item['request']['sha256'], item['result']['sha256']]
                    and all(isinstance(x, str) and HEX.fullmatch(x) for x in triple), 'attempt ledger suffix differs')
        require(all(len({triple[column] for triple in next_ledger}) == len(next_ledger) for column in range(3)),
                'duplicate attempted identity in saved progress')
        updates += int(ready); attempted_batches += 1; attempted_matches += len(attempts)
        require((_integer(result['completed_bo3_updates']), _integer(result['attempted_batches']), _integer(result['attempted_matches']))
                == (updates, attempted_batches, attempted_matches)
                and progress['completed_bo3_updates'] == updates
                and progress['attempt_progress']['attempted_batches'] == attempted_batches,
                'attempted batches/matches were conflated with optimizer updates')
        require(result['learner']['identity']['adam_step'] == learner['identity']['adam_step'] + int(ready)
                and (ready or result['learner'] == learner), 'NoUpdate changed state or Adam delta differs')
        if ready:
            _pin_relative(result['learner']['source']['checkpoint'], logical_run, files)
            _pin_relative(result['learner']['source']['play_import'], logical_run, files)
        dispositions.append(report['disposition'])
        learner, previous, ledger = result['learner'], result['progress'], next_ledger
        receipt_pins.append({'path': str(logical_run / relative), 'sha256': files[relative]['sha256']})
    final = 'completion.json' in files
    if final:
        result = document('completion.json', 16 * MIB)
        require(result['schema'] == 'mtg-kernel-native-bo3-run-result/v1' and result['complete'] is True
                and len(completed) == len(batches) == result['completed_batches'] == result['planned_batches']
                and result['learner'] == learner and result['previous_progress'] == previous
                and (result['completed_bo3_updates'], result['attempted_batches'], result['attempted_matches'])
                    == (updates, attempted_batches, attempted_matches), 'native final completion differs')
        require(len(result['batch_receipts']) == len(receipt_pins), 'final receipt count differs')
        for actual, expected in zip(result['batch_receipts'], receipt_pins):
            require(_same_root(actual['path'], expected['path']) and actual['sha256'] == expected['sha256'],
                    'final batch receipt pin differs')
    pending = [name for name in files if name.startswith(f'batches/{len(completed):06d}/')]
    require(all(int(match[1]) <= len(completed) for name in files
                if (match := re.match(r'batches/(\d{6})/', name))), 'future batch artifacts exceed first incomplete batch')
    return {'completed_batches': len(completed), 'planned_batches': len(batches),
            'completed_bo3_updates': updates, 'attempted_batches': attempted_batches,
            'attempted_matches': attempted_matches, 'adam_step': learner['identity']['adam_step'],
            'complete_matches': complete_matches, 'incomplete_matches': incomplete_matches,
            'eligible_matches': eligible_matches, 'committed_match_totals_only': True,
            'batch_dispositions': dispositions, 'complete': final,
            'pending_checkpoint_files': sorted(name for name in pending if name.endswith('/checkpoint.json')),
            'pending_progress_files': sorted(name for name in pending if name.endswith('/progress.json')),
            'native_state_validation': 'required after restore; not performed by file recovery'}


def _rank(relative):
    name = PurePosixPath(relative).name
    if relative == 'completion.json':
        return 3, relative
    if name == 'complete.json':
        return 2, relative
    if name in ('intent.json', 'progress.json') or 'receipt' in name:
        return 1, relative
    return 0, relative


def _package(hot, package_sha256, guard):
    require(isinstance(package_sha256, str) and HEX.fullmatch(package_sha256), 'original package SHA256 required')
    manifest, row = _json(hot / 'manifest.json', guard, 16 * MIB)
    require(row['sha256'] == package_sha256 and manifest['schema'] == 'phase1-bo3-cloud-payload/v1'
            and _same_root(manifest['remote_root'], hot), 'original staged package identity or stable hot root differs')
    request_path = Path(manifest['run_request']['path'])
    require(request_path.is_relative_to(hot) and request_path.is_absolute(), 'packaged request is outside hot root')
    config, request_row = _json(request_path, guard, 16 * MIB)
    require(request_row['sha256'] == manifest['run_request']['sha256'], 'packaged run request differs')
    return config, row


def export_stopped(hot_root, durable_root, generation, package_sha256, *, limits=Limits(),
                   assert_stopped, heartbeat, quota):
    """Publish one fresh CAS-backed snapshot index after a reaped invocation.

    quota(event) must include prior cumulative storage and free-space reserves.
    event['written_bytes'] includes this call's own surviving temporary writes;
    next_write_bytes is checked before each new write. Callbacks may raise.
    """
    guard = _Guard(limits, assert_stopped, heartbeat, quota)
    guard.check('export-start')
    require(isinstance(generation, str) and re.fullmatch('[a-z0-9][a-z0-9-]{0,63}', generation), 'safe snapshot generation required')
    hot, durable = _directory(hot_root), _directory(durable_root, True, guard)
    require(not hot.is_relative_to(durable) and not durable.is_relative_to(hot), 'hot and durable roots must be separate')
    config, package = _package(hot, package_sha256, guard)
    files, directories = _inventory(hot / 'run', guard)
    require(sum(row['bytes'] for row in files.values()) + package['bytes'] <= limits.max_bytes, 'indexed payload limit exceeded')
    summary = _inspect(hot / 'run', hot / 'run', files, config, guard)
    index = {'schema': INDEX_SCHEMA, 'generation': generation, 'hot_root': str(hot),
             'package_manifest': package, 'directories': directories, 'files': files,
             'indexed_bytes': sum(row['bytes'] for row in files.values()),
             'file_count': len(files), 'summary': summary}
    require(len(encoded(index)) <= limits.max_index_bytes, 'recovery index exceeds metadata limit')
    blobs, temporary = durable / 'blobs', durable / '.copy-tmp'
    _directory(blobs, True, guard)
    new_blob_bytes = _copy(hot / 'manifest.json', blobs / package['sha256'], package, temporary, guard)
    for relative in sorted(files, key=_rank):
        row = files[relative]
        new_blob_bytes += _copy(hot / 'run' / relative, blobs / row['sha256'], row, temporary, guard)
    guard.check('export-inventory-final')
    final_files, final_directories = _inventory(hot / 'run', guard, False)
    require(final_files == {name: row['bytes'] for name, row in files.items()}
            and final_directories == directories, 'source tree changed before index publication')
    index_path = durable / 'indices' / (generation + '.json')
    index_pin = _publish_index(index_path, index, temporary, guard)
    guard.check('export-complete')
    return {'index': {'path': str(index_path), **index_pin}, 'summary': summary,
            'indexed_bytes': index['indexed_bytes'], 'new_blob_bytes': new_blob_bytes, **guard.result()}


def _index(durable, generation, expected_sha256, guard):
    require(isinstance(generation, str) and re.fullmatch('[a-z0-9][a-z0-9-]{0,63}', generation), 'safe snapshot generation required')
    require(isinstance(expected_sha256, str) and HEX.fullmatch(expected_sha256), 'pinned recovery index required')
    value, pin = _json(durable / 'indices' / (generation + '.json'), guard, guard.limits.max_index_bytes)
    require(pin['sha256'] == expected_sha256 and value['schema'] == INDEX_SCHEMA and value['generation'] == generation,
            'recovery index identity differs')
    require(set(value) == {'schema', 'generation', 'hot_root', 'package_manifest', 'directories', 'files',
                           'indexed_bytes', 'file_count', 'summary'}, 'unknown recovery index fields')
    files, directories = value['files'], value['directories']
    require(isinstance(files, dict) and isinstance(directories, list) and len(files) <= guard.limits.max_files
            and len(files) + len(directories) <= guard.limits.max_entries
            and len(set(directories)) == len(directories), 'recovery inventory dimensions differ')
    for relative in [*files, *directories]:
        _relative(relative)
        require(not _excluded(relative), 'mutable telemetry/lock was indexed')
    require(not set(files) & set(directories), 'file/directory inventory collision')
    names = [*files, *directories]
    require(len({os.path.normcase(name) for name in names}) == len(names), 'host path aliases in recovery inventory')
    total = 0
    for row in [value['package_manifest'], *files.values()]:
        require(isinstance(row, dict) and set(row) == {'sha256', 'bytes'}
                and isinstance(row['sha256'], str) and HEX.fullmatch(row['sha256'])
                and type(row['bytes']) is int and 0 <= row['bytes'] <= guard.limits.max_file_bytes,
                'invalid indexed blob pin')
        total += row['bytes']
    require(total <= guard.limits.max_bytes and value['indexed_bytes'] == total - value['package_manifest']['bytes']
            and value['file_count'] == len(files), 'indexed byte/file counts differ')
    for name in [*files, *directories]:
        parent = PurePosixPath(name).parent
        while str(parent) != '.':
            require(str(parent) in directories and str(parent) not in files, 'missing or conflicting indexed directory')
            parent = parent.parent
    return value


def restore_stopped(durable_root, generation, index_sha256, hot_root, package_sha256, *, limits=Limits(),
                    assert_stopped, heartbeat, quota):
    """Restore only one pinned index, at its original absolute hot namespace."""
    guard = _Guard(limits, assert_stopped, heartbeat, quota)
    guard.check('restore-start')
    hot, durable = _directory(hot_root), _directory(durable_root)
    require(not hot.is_relative_to(durable) and not durable.is_relative_to(hot), 'hot and durable roots must be separate')
    index = _index(durable, generation, index_sha256, guard)
    require(_same_root(index['hot_root'], hot), 'restore requires the exact original absolute hot root')
    config, package = _package(hot, package_sha256, guard)
    require(package == index['package_manifest'], 'restored staged package differs from original export')
    require(_hash(durable / 'blobs' / package['sha256'], guard, 16 * MIB) == package,
            'durable original package manifest differs')
    files, directories = index['files'], index['directories']
    run = hot / 'run'
    existing, existing_directories = _inventory(run, guard)
    require(set(existing) <= set(files) and set(existing_directories) <= set(directories),
            'unindexed native files or directories exist; preserve them and stop restore')
    for relative, row in existing.items():
        require(row == files[relative], 'existing native destination differs')
    _directory(run, True, guard)
    for relative in sorted(directories, key=lambda value: (len(PurePosixPath(value).parts), value)):
        guard.check('restore-directory', relative)
        _directory(run / relative, True, guard)
    copied_bytes = 0
    for relative in sorted(files, key=_rank):
        row = files[relative]
        # Existing files must not bypass source-blob verification or heartbeat.
        source = durable / 'blobs' / row['sha256']
        require(_hash(source, guard, limits.max_file_bytes) == row, 'durable indexed blob differs')
        copied_bytes += _copy(source, run / relative, row, hot / '.bo3-restore-tmp', guard)
    summary = _inspect(run, run, files, config, guard)
    require(summary == index['summary'], 'restored native prefix/counters differ from export')
    guard.check('restore-complete')
    return {'summary': summary, 'indexed_bytes': index['indexed_bytes'], 'copied_bytes': copied_bytes,
            'index_sha256': index_sha256, **guard.result()}
