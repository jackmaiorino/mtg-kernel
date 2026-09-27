"""Guarded line (a) launch entry, reached only through g115_d3_windows_dispatch.ps1.

Modes:
- throughput: the bounded serial-versus-parallel timing check (COMPUTE-POLICY
  item 2). Every worker count runs the same small representative job list and
  must reproduce the serial match hashes; it writes host evidence for the
  g115-line-a-throughput/v1 record. Small by construction (at most 64 BO3).
- qualification, calibration, screen-evaluation: evaluation job sets admitted
  only through g115_line_a_guard_v1 (throughput evidence for every work class,
  measured byte worksheet, scratch manifest, pinned executable; formal modes
  also need the frozen roster, the director's scope ruling and one yardstick).

Outcomes are never read: a match is checked by its native completion record
and the SHA256 of its match file, never by parsing results. Native children
run at BelowNormal from the pinned copy, one BO3 per request, fresh output
directories only, with per-job byte reservations in the D4 storage Ledger.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import time

import g115_line_a_guard_v1 as guard
import g115_line_a_manifest_v1 as manifest
from g115_d3_qualify_v1 import minimum_reserve
from g115_d4_audit_storage_v1 import Ledger, charge

LAUNCH_SCHEMA = 'g115-line-a-launch/v1'
HOST_EVIDENCE_SCHEMA = 'g115-line-a-host-throughput/v1'
MODES = ('throughput', 'qualification', 'calibration', 'screen-evaluation')
FORMAL_MODES = ('calibration', 'screen-evaluation')
THROUGHPUT_MAX_JOBS = 64
JOB_SETS = {'qualification': 'calibration', 'calibration': 'calibration', 'screen-evaluation': 'screen'}
JOB_SET_SCHEMAS = {'qualification': 'g115-line-a-qualification-manifest/v1',
                   'calibration': manifest.SCHEMAS['calibration'],
                   'screen-evaluation': manifest.SCHEMAS['screen-evaluation']}
CHILD_ENV_KEYS = ('PATH', 'SYSTEMROOT', 'WINDIR', 'TEMP', 'TMP', 'COMSPEC')
COMPLETION_SCHEMA = 'g115-line-a-evaluation-completion/v1'
COLLAB = 'C:/Users/Jack/IdeaProjects/collab'
CATALOG_TOOL = COLLAB + '/tools/artifact_register.py'


def require(ok, message):
    guard.require(ok, message)


def write_json(path, value):
    path = Path(path)
    partial = path.with_name(path.name + '.partial')
    partial.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n', encoding='utf-8')
    os.replace(partial, path)


def tree_bytes(path):
    """Allocated-size proxy for small native outputs: file bytes plus the Ledger's per-file margin."""
    files = [p for p in Path(path).rglob('*') if p.is_file() and not p.is_symlink()]
    return charge(sum(p.stat().st_size for p in files), len(files))


def evaluation_request(job, packet, learner_source, member_source, output_directory):
    """One public_feature_evaluation_v1 request holding exactly one BO3; the learner sits at its seat."""
    sources = [learner_source, member_source] if job['learner_seat'] == 0 else [member_source, learner_source]
    return dict(sources=sources, matches=[manifest.native_match(job, packet)], cross_generation_evaluation=True,
                capture_decisions=False, output_directory=str(output_directory))


def match_hash(output_directory, request, git_head):
    """Completion and hash checks only; the match file is hashed, never parsed."""
    folder = Path(output_directory)
    require(not (folder / 'failure.json').exists(), 'Native failure record exists')
    start = json.loads((folder / 'start.json').read_bytes())
    completion = json.loads((folder / 'completion.json').read_bytes())
    require(start['command'] == request, 'Native start differs from the request')
    require(start['git_head'] == git_head, 'Native build commit differs from the pinned build')
    require(completion['matches'] == 1 and len(completion['match_sha256']) == 1, 'Native completion shape differs')
    digest = guard.sha256_file(folder / 'match-000000.json')
    require(digest == completion['match_sha256'][0], 'Match file differs from its completion hash')
    return digest


class Pool:
    """Worker pool over independent BO3 jobs; the first failure stops new dispatch and is retained."""

    def __init__(self, executable, git_head, root, workers, ledger, reservation_bytes, job_timeout_seconds,
                 disk_path, memory_reserve_bytes=0, prefix=None):
        # `prefix` replaces [executable] only for offline tests that substitute a fake native evaluator.
        self.command = list(prefix) if prefix else [str(executable)]
        self.git_head, self.root = git_head, Path(root)
        self.workers, self.ledger, self.reservation = workers, ledger, reservation_bytes
        self.timeout, self.disk_path, self.memory_reserve = job_timeout_seconds, disk_path, memory_reserve_bytes
        self.stop, self.lock = threading.Event(), threading.Lock()

    def environment(self):
        env = {k: v for k, v in os.environ.items() if k.upper() in CHILD_ENV_KEYS}
        env.update(CUDA_VISIBLE_DEVICES='', OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1')
        return env

    def admit(self, name):
        require(not self.stop.is_set(), 'Pool stopped after a failure')
        with self.lock:
            free = shutil.disk_usage(self.disk_path).free
            self.ledger.reserve(name, self.reservation, free, guard.DISK_RESERVE_BYTES)
        if self.memory_reserve:
            import psutil
            require(psutil.virtual_memory().available >= self.memory_reserve, 'Preserve the memory reserve')

    def run(self, job, request):
        name = job['id']
        row = dict(id=name, complete=False, charged_bytes=0)
        output = Path(request['output_directory'])
        started = time.monotonic()
        try:
            self.admit(name)
            require(not output.exists(), 'Native output already exists; never reuse an attempt')
            request_path = self.root / 'requests' / (name + '.json')
            request_path.write_bytes(json.dumps(request).encode('utf-8'))
            row['request_sha256'] = guard.sha256_file(request_path)
            flags = subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            with (self.root / 'logs' / (name + '.log')).open('xb') as log:
                child = subprocess.Popen(self.command + [str(request_path)], env=self.environment(), stdout=log,
                                         stderr=subprocess.STDOUT, creationflags=flags)
                try:
                    child.wait(timeout=self.timeout)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
                    raise ValueError('Whole-match time bound reached')
            row['exit_code'] = child.returncode
            require(child.returncode == 0, 'Native match failed')
            row['match_sha256'] = match_hash(output, request, self.git_head)
            row['complete'] = True
        except Exception as error:
            row.update(error_type=type(error).__name__, error=str(error))
            self.stop.set()
        finally:
            row['seconds'] = time.monotonic() - started
            if name in self.ledger.pending:
                measured = tree_bytes(output) if output.exists() else 0
                row['charged_bytes'] = measured
                with self.lock:
                    try:
                        self.ledger.seal(name, measured, failed=not row['complete'])
                    except ValueError as error:
                        row.update(complete=False, error_type='ValueError', error=str(error))
                        self.stop.set()
            write_json(self.root / 'executions' / (name + '.json'), row)
        return row

    def map(self, items):
        for folder in ('requests', 'logs', 'executions'):
            (self.root / folder).mkdir(parents=True, exist_ok=True)
        started = time.monotonic()
        with concurrent.futures.ThreadPoolExecutor(self.workers) as executor:
            rows = list(executor.map(lambda item: self.run(*item), items))
        return rows, time.monotonic() - started


def throughput(launch, host, root, now=None, prefix=None):
    """Bounded serial-versus-parallel phases on identical jobs; each phase gets a fresh output tree."""
    spec = launch['throughput']
    jobs = spec['jobs']
    require(0 < len(jobs) <= THROUGHPUT_MAX_JOBS, 'The timing check is limited to 64 BO3')
    counts = spec['worker_counts']
    require(counts == sorted(set(counts)) and counts[0] == 1 and len(counts) >= 2,
            'Serial first, then increasing worker counts')
    executable = guard.require_pinned(launch['executable']['path'], launch['executable']['sha256'])
    phases, reference = [], None
    ledger = Ledger(spec['cap_bytes'], spec['control_allowance_bytes'])
    for workers in counts:
        phase_root = Path(root) / ('phase-%02d-workers' % workers)
        (phase_root / 'native').mkdir(parents=True)
        items = []
        for job in jobs:
            output = phase_root / 'native' / job['id']
            request = evaluation_request(job, launch['deck_packet'], launch['learner_source'],
                                         launch['member_sources'][job['member']], output)
            items.append((dict(job, id='w%02d-%s' % (workers, job['id'])), request))
        pool = Pool(executable, launch['executable']['git_head'], phase_root, workers, ledger,
                    spec['reservation_bytes'], spec['job_timeout_seconds'], str(root), prefix=prefix)
        rows, seconds = pool.map(items)
        require(all(row['complete'] for row in rows), 'Timing phase incomplete; retained for diagnosis')
        hashes = [dict(id=job['id'], sha256=row['match_sha256']) for job, row in zip(jobs, rows)]
        reference = hashes if reference is None else reference
        require(hashes == reference, 'Worker count %d changed match bytes' % workers)
        phases.append(dict(workers=workers, seconds=seconds, completed=len(rows), rows=hashes,
                           charged_bytes=sum(row['charged_bytes'] for row in rows),
                           max_charged_bytes=max(row['charged_bytes'] for row in rows)))
    record = dict(schema=HOST_EVIDENCE_SCHEMA, host=host, work_class=spec['work_class'],
                  binding=launch['binding'], jobs_sha256=manifest.canonical_sha256(jobs), phases=phases,
                  max_charged_bytes_per_bo3=max(p['max_charged_bytes'] for p in phases),
                  measured_unix=time.time() if now is None else now,
                  scope='bounded timing check; outcomes unread')
    write_json(Path(root) / 'host-throughput.json', record)
    return record


def work_class(launch, job):
    source = launch['member_sources'][job['member']]
    return 'bo3-search' if source['kind'] == 'information_set_search_v3' else 'bo3-ordinary'


def dispatch(launch, host, root, placements, jobs, prefix=None):
    """An admitted evaluation job set: each work class at its selected worker count on this host.

    The completion record carries identities, hashes, times and bytes only; no outcome is read.
    """
    root = Path(root)
    write_json(root / 'admission.json', dict(schema='g115-line-a-admission/v1', mode=launch['mode'], host=host,
                                            placements=placements, jobs=len(jobs)))
    worksheet = guard.read(guard.checked(launch['worksheet']))
    ledger = Ledger(worksheet['cap_bytes'], launch['control_allowance_bytes'])
    executable = guard.require_pinned(launch['executable']['path'], launch['executable']['sha256'])
    (root / 'native').mkdir()
    rows = []
    for name in sorted(placements):
        workers = placements[name]['allocation'][host]
        items = [(job, evaluation_request(job, launch['deck_packet'], launch['learner_source'],
                                          launch['member_sources'][job['member']], root / 'native' / job['id']))
                 for job in jobs if work_class(launch, job) == name]
        pool = Pool(executable, launch['executable']['git_head'], root, workers, ledger,
                    launch['reservation_bytes'], launch['job_timeout_seconds'], launch['hosts'][host]['volume'],
                    memory_reserve_bytes=minimum_reserve(host), prefix=prefix)
        class_rows, seconds = pool.map(items)
        rows += [dict(row, work_class=name, workers=workers) for row in class_rows]
        if not all(row['complete'] for row in class_rows):
            break
    complete = len(rows) == len(jobs) and all(row['complete'] for row in rows)
    result = dict(schema=COMPLETION_SCHEMA, complete=complete, mode=launch['mode'], job=launch['job'], host=host,
                  jobs=len(jobs), completed=sum(row['complete'] for row in rows),
                  not_started=[job['id'] for job in jobs if job['id'] not in {row['id'] for row in rows}],
                  rows=[{k: row[k] for k in ('id', 'work_class', 'workers', 'complete', 'seconds', 'charged_bytes',
                                              'match_sha256', 'error') if k in row} for row in rows],
                  ledger=dict(cap_bytes=worksheet['cap_bytes'], committed_bytes=ledger.committed,
                              stopped=ledger.stopped),
                  outcomes_read=False)
    write_json(root / 'completion.json', result)
    return result


def register(root, status, launch, run=subprocess.run):
    """Catalog registration (artifact law clause 9) with a commit limited to the catalog file."""
    verb = ['add', '--path', str(root)] if status == 'live' else ['update', '--id', str(root)]
    retention = 'keep-full' if launch['mode'] in FORMAL_MODES else 'prunable'
    run([sys.executable, CATALOG_TOOL] + verb + [
        '--lane', 'opus-line-a-launcher', '--owner', 'opus-line-a-launcher', '--status', status,
        '--retention', retention, '--purpose', 'line (a) %s job set %s' % (launch['mode'], launch['job']),
        '--doc', 'docs/g115_line_a_launcher_v1.md', '--by', 'opus-line-a-launcher',
        '--regen', 'pinned executable %s, launch manifest and seeds' % launch['executable']['sha256'][:12]],
        check=True)
    run(['git', '-C', COLLAB, 'commit', '-q', '-m', 'ARTIFACTS: %s (%s)' % (root, status), '--',
         'ARTIFACTS/catalog.jsonl'], check=False)


def admitted_jobs(launch):
    """The evaluation jobs a guarded mode may dispatch, from the pinned job-set manifest."""
    job_set = guard.read(guard.checked(launch['job_manifest']))
    require(job_set['manifest_sha256'] == launch['job_manifest_sha256'], 'Job-set manifest identity differs')
    jobs = launch['jobs']
    require(manifest.canonical_sha256(jobs) == job_set['job_list_sha256'], 'Jobs differ from the pinned job list')
    return job_set, jobs


def check(launch, host, now=None):
    """Every refusal before any spawn. Returns the admitted placement and job list."""
    require(launch['schema'] == LAUNCH_SCHEMA and launch['mode'] in MODES, 'Wrong launch manifest')
    here = Path(__file__).resolve().parent
    for name, document in launch['documents'].items():
        guard.checked(document)
    require(guard.sha256_file(here / Path(__file__).name) == launch['documents']['launcher']['sha256'],
            'Launcher changed since the manifest pinned it')
    guard.require_pinned(launch['executable']['path'], launch['executable']['sha256'])
    require(host in launch['hosts'], 'Host has no allocation in this manifest')
    if launch['mode'] == 'throughput':
        return None, launch['throughput']['jobs']
    job_set, jobs = admitted_jobs(launch)
    require(job_set['schema'] == JOB_SET_SCHEMAS[launch['mode']], 'Job-set schema differs from the launch mode')
    classes = {}
    for job in jobs:
        classes[work_class(launch, job)] = classes.get(work_class(launch, job), 0) + 1
    placements = {}
    for name, units in classes.items():
        evidence = launch['throughput_evidence'].get(name)
        placements[name] = guard.require_throughput(
            guard.read(guard.checked(evidence)) if evidence else None, name, units, launch['binding'], now)
    for placement in placements.values():
        require(host in placement['allocation'], 'Selected placement does not use this host')
    worksheet = guard.read(guard.checked(launch['worksheet'])) if launch.get('worksheet') else None
    guard.require_worksheet(worksheet, JOB_SETS[launch['mode']], job_set['manifest_sha256'],
                            shutil.disk_usage(launch['hosts'][host]['volume']).free,
                            launch.get('committed_other_bytes', 0))
    scratch = guard.read(guard.checked(launch['scratch_manifest'])) if launch.get('scratch_manifest') else None
    guard.require_scratch_manifest(scratch, launch['job'], host, worksheet['cap_bytes'])
    if launch['mode'] in FORMAL_MODES:
        require(job_set['launchable'] is True, 'Job-set manifest is not launchable')
        guard.require_scope_ruling(launch.get('scope'), job_set['composition'])
        require(job_set.get('yardstick_executable_sha256') == launch['executable']['sha256'],
                'Evaluator differs from the pinned yardstick executable')
    return placements, jobs


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--host', choices=['jack', 'haleyspc'], required=True)
    parser.add_argument('--root', type=Path)
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    launch = guard.read(args.manifest)
    placements, jobs = check(launch, args.host)
    if args.check_only:
        print(json.dumps(dict(checked=True, mode=launch['mode'], jobs=len(jobs), launched=False)))
        return
    require(args.root is not None, 'Fresh worker root required')
    args.root.mkdir(parents=False)  # never resume or overwrite an attempt
    write_json(args.root / 'launch-manifest.json', launch)
    if launch['mode'] == 'throughput':
        record = throughput(launch, args.host, args.root)
        print(json.dumps(dict(complete=True, phases=[(p['workers'], round(p['seconds'], 3)) for p in record['phases']])))
        return
    register(args.root, 'live', launch)
    result = dispatch(launch, args.host, args.root, placements, jobs)
    register(args.root, 'closed', launch)
    print(json.dumps({k: v for k, v in result.items() if k != 'rows'}))
    raise SystemExit(0 if result['complete'] else 1)


if __name__ == '__main__':
    main()
