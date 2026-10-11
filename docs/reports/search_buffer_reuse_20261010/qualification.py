"""Exact, bounded real-root qualification and ABBA through stage4a_queue.

prepare stages pins only. run must execute inside supported host_slots admission.
No strength/outcome field is projected; primary hashes are opaque equality checks.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

REPO = Path(__file__).resolve().parents[3]
SCRATCH = Path('D:/search-buffer-reuse-20261010')
COLD = Path('E:/search-buffer-reuse-20261010')
BASE = '1d2404775a141db4cd90d7579f8f765db37d55df'
PYTHON = Path('D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe')
GIB = 2**30
CAP = 20 * GIB
ROOT_IDS = ['r1-cast-A48-g3757-s97', 'r1-target-A48-g2506-s133']
LIMITS = '20000,2,65536'


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def require(ok, why):
    if not ok:
        raise ValueError(why)


def pin(path):
    path = Path(path)
    with path.open('rb') as stream:
        return {'path': path.as_posix(), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest(), 'bytes': path.stat().st_size}


def verify(record):
    require(pin(record['path']) == record, 'Pinned input changed: ' + record['path'])


def put(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(value, stream, sort_keys=True, indent=2, allow_nan=False)
        stream.write('\n')
    return pin(path)


def copy_once(source, target):
    source, target = Path(source), Path(target)
    expected = pin(source)
    target.parent.mkdir(parents=True, exist_ok=True)
    if not target.exists():
        shutil.copy2(source, target)
    require(pin(target)['sha256'] == expected['sha256'], 'Recovery copy differs: ' + str(target))
    return pin(target)


def storage():
    require(not (SCRATCH / 'STOP').exists(), 'Lane STOP file present')
    size = 0
    for folder, dirs, files in os.walk(SCRATCH, followlinks=False):
        dirs[:] = [d for d in dirs if not Path(folder, d).is_junction()]
        for name in files:
            path = Path(folder, name)
            if not path.is_symlink():
                try:
                    size += path.stat().st_size
                except FileNotFoundError:
                    pass
    require(size < CAP, 'Owned tree reached 20 GiB cap')
    for volume in ('D:/', 'E:/'):
        require(shutil.disk_usage(volume).free >= 60 * GIB, volume + ' below 60 GiB reserve')
    return size


def prepare(args):
    require(re.fullmatch(r'[a-z0-9][a-z0-9-]*', args.attempt) is not None, 'Unsafe attempt name')
    require(sys.version_info[:3] == (3, 13, 14), 'Pinned Python 3.13.14 required')
    storage()
    hot, cold = SCRATCH / args.attempt, COLD / args.attempt
    require(not hot.exists() and not cold.exists(), 'Keep existing attempt; choose a new attempt name')
    baseline = json.loads((SCRATCH / 'build-baseline.json').read_text())
    require(baseline['complete'] and baseline['git_commit'] == BASE, 'Completed pinned baseline build required')
    verify(baseline['binary'])
    candidate = Path(args.candidate_binary).resolve()
    source = Path(args.candidate_source).resolve()
    commit = subprocess.check_output(['git', '-C', str(source), 'rev-parse', args.candidate_commit], text=True).strip()
    require(subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip() == commit,
            'Candidate commit must be isolated candidate checkout HEAD')
    changed = subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain', '--untracked-files=all', '--',
                                      'mtg-kernel', 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'], text=True)
    require(not changed.strip(), 'Commit candidate runtime source before preparation')
    candidate_pin = pin(candidate)
    candidate_build = json.loads((SCRATCH / 'build-candidate.json').read_text())
    require(candidate_build['complete'] and candidate_build['commit'] == commit, 'Completed candidate build receipt required')
    verify(candidate_build['binary'])
    require(candidate_pin['sha256'] == candidate_build['binary']['sha256'], 'Candidate binary differs from built binary')
    require(candidate_build['toolchain'] == baseline['toolchain'] and candidate_build['linker'] == baseline['linker'],
            'Baseline/candidate build toolchains differ')
    binary_cold = copy_once(candidate, Path('E:/pinned-binaries') / candidate_pin['sha256'] / 'regret_census_v1.exe')
    hot.mkdir(parents=True)
    cold.mkdir(parents=True)
    baseline_receipt = copy_once(SCRATCH / 'build-baseline.json', cold / 'build-baseline.json')
    candidate_receipt = copy_once(SCRATCH / 'build-candidate.json', cold / 'build-candidate.json')
    # Stage direct runtime dependencies only. Checkpoint lineage/episode references
    # are archival provenance, not recursively traversed runtime dependencies.
    provenance, inputs = [], []
    def object_ref(record):
        original = pin(record['path'])
        require(original['sha256'] == record['sha256'], 'Runtime dependency digest mismatch')
        name = original['sha256'] + Path(record['path']).suffix
        staged = copy_once(record['path'], hot / 'inputs/objects' / name)
        sealed = copy_once(record['path'], cold / 'inputs/objects' / name)
        provenance.append({'original': original, 'sealed': sealed, 'staged': staged})
        inputs.append(staged)
        return {'path': staged['path'], 'sha256': staged['sha256']}
    for name in ('r1', 't1', 'a48'):
        original_path = Path('D:/stage4a-20261009/sources') / (name + '-source.json')
        source = json.loads(original_path.read_text())
        source['checkpoint'] = object_ref(source['checkpoint'])
        import_original = pin(source['play_import']['path'])
        require(import_original['sha256'] == source['play_import']['sha256'], 'Import wrapper digest mismatch')
        imported = json.loads(Path(source['play_import']['path']).read_text())
        for key in ('initialization', 'parameters'):
            imported[key] = object_ref(imported[key])
        metadata = put(hot / 'inputs/metadata' / (name + '-import.json'), imported)
        copy_once(metadata['path'], cold / 'inputs/metadata' / (name + '-import.json'))
        inputs.append(metadata)
        source['play_import'] = {'path': metadata['path'], 'sha256': metadata['sha256']}
        staged_source = put(hot / 'sources' / (name + '-source.json'), source)
        inputs.append(staged_source)
        copy_once(staged_source['path'], cold / 'sources' / (name + '-source.json'))
        provenance.append({'original_source': pin(original_path), 'original_import': import_original,
                           'staged_source': staged_source, 'rewritten_paths_only': True})
    roots_path = Path('D:/stage4a-20261009/frozen-gy/roots-engineering-r1.jsonl')
    roots = [json.loads(line) for line in roots_path.read_text().splitlines() if line.strip()]
    require([row['root_id'] for row in roots] == ROOT_IDS, 'Engineering root inventory differs')
    roots_pin = copy_once(roots_path, hot / 'roots.jsonl')
    roots_cold = copy_once(roots_path, cold / 'roots.jsonl')
    inputs.append(roots_pin)
    queue = copy_once(SCRATCH / 'baseline-source/tools/stage4a/stage4a_queue.py', hot / 'stage4a_queue.py')
    copy_once(queue['path'], cold / 'stage4a_queue.py')
    inputs.append(queue)
    bins = {}
    for label, record in (('baseline', baseline['binary']), ('candidate', binary_cold)):
        bins[label] = {'sealed': record, 'staged': copy_once(record['path'], hot / 'bin' / (label + '.exe')),
                       'commit': BASE if label == 'baseline' else commit}
    plan = {'schema': 'search-buffer-reuse-comparison/v1', 'created_utc': now(), 'attempt': args.attempt,
            'hot_root': hot.as_posix(), 'cold_root': cold.as_posix(), 'binaries': bins, 'inputs': inputs,
            'input_provenance': provenance, 'roots': roots_pin, 'source_of_record_roots': roots_cold,
            'root_ids': ROOT_IDS, 'queue': queue, 'python': pin(PYTHON), 'coordinator': pin(__file__),
            'model': 'r1', 'base_seed': 2026100941, 'limits': LIMITS, 'gpu': None,
            'activation': 'ordinary exact default; S4A_FAST_FORWARD unset',
            'toolchain': baseline['toolchain'], 'linker': baseline['linker'], 'linker_version': baseline['linker_version'],
            'baseline_build_receipt': baseline_receipt,
            'candidate_build_receipt': candidate_receipt,
            'storage': {'projected_bytes': 12 * GIB, 'cap_bytes': CAP, 'reserve_bytes': 60 * GIB,
                        'cold_durability': 'Every input/result copied and verified on E and independent D SSD; retain D until successor backup verified.'},
            'qualification': 'baseline1w, candidate1w, baseline2w, candidate2w; identical primary hashes/work counts; choose minimum combined completed-job elapsed',
            'measurement': 'baseline, candidate, candidate, baseline at chosen common workers; two roots and fixed reduced budget per case',
            'max_wall_seconds_per_case': 1200, 'max_native_cpu_seconds_per_case': 2400,
            'scope': 'Engineering throughput and exact semantic equality only. No training, outcome interpretation, CP7 or paid execution.'}
    put(hot / 'plan.json', plan)
    copy_once(hot / 'plan.json', cold / 'plan.json')
    print(json.dumps({'prepared': hot.as_posix(), 'plan': pin(hot / 'plan.json'), 'native_started': False}))


def projection(path, expected_roots):
    result = []
    for line in Path(path).read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        require(row.get('kind') == 's4a_root', 'Unexpected/error output row')
        require(row['invalid'] is False, 'Invalid engineering root')
        limits = row['config']['limits']
        require((limits['select_cap'], limits['eval_worlds'], limits['eval_cap']) == (20000, 2, 65536), 'Budget changed')
        result.append({'root_id': row['root_id'], 'primary_sha256': row['primary_sha256'],
                       'cost': row['cost'], 'root_wall_seconds': row['timing']['root_wall'],
                       'selection_wall_seconds': row['timing']['selection_wall'],
                       'eval_wall_seconds': row['timing']['eval_wall']})
    require(sorted(row['root_id'] for row in result) == sorted(expected_roots), 'Missing/duplicate roots')
    return sorted(result, key=lambda row: row['root_id'])


def identity(rows):
    return [{'root_id': row['root_id'], 'primary_sha256': row['primary_sha256'], 'cost': row['cost']} for row in rows]


def case(plan, phase, label, variant, workers):
    storage()
    hot, cold = Path(plan['hot_root']), Path(plan['cold_root'])
    path, sealed = hot / phase / label, cold / phase / label
    require(not path.exists(), 'Prior attempt preserved: ' + str(path))
    path.mkdir(parents=True)
    for name in ('r1', 't1', 'a48'):
        copy_once(hot / 'sources' / (name + '-source.json'), path / 'sources' / (name + '-source.json'))
    job = {'name': label, 'mode': 's4a-run', 'model': plan['model'], 'workers': workers,
           'base_seed': plan['base_seed'], 'roots': plan['roots']['path'], 'limits': plan['limits'],
           'max_wall_seconds': plan['max_wall_seconds_per_case'], 'max_cpu_seconds': plan['max_native_cpu_seconds_per_case']}
    put(path / 'queue.json', [job])
    env = {key: value for key, value in os.environ.items() if not key.startswith('S4A_') and key != 'ROOTS'}
    command = [str(PYTHON), '-B', plan['queue']['path'], str(path), plan['binaries'][variant]['staged']['path'], str(path / 'queue.json')]
    started = time.perf_counter()
    receipt = {'schema': 'search-buffer-reuse-case/v1', 'label': label, 'variant': variant, 'workers': workers,
               'started_utc': now(), 'admitted_cores': os.environ['HOST_SLOTS_CORES'], 'complete': False}
    try:
        with (path / 'controller.log').open('x', encoding='utf-8') as log:
            completed = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        receipt['whole_queue_elapsed_seconds'] = time.perf_counter() - started
        require(completed.returncode == 0, 'Queue controller failed')
        jobs = json.loads((path / 'out/QUEUE-RESULTS.json').read_text())
        require(len(jobs) == 1 and jobs[0]['exit'] == 0 and jobs[0]['error_rows'] == 0, 'Native case failed')
        require(jobs[0]['rows_by_kind'] == {'s4a_root': len(ROOT_IDS)}, 'Incomplete native output')
        receipt['native_job'] = jobs[0]
        rows_file = path / 'out' / (label + '.jsonl')
        require(pin(rows_file)['sha256'] == jobs[0]['sha256'], 'Terminal output hash mismatch')
        receipt['roots'] = projection(rows_file, plan['root_ids'])
        store = put(path / 'primary-store.json', identity(receipt['roots']))
        receipt['primary_store_sha256'] = store['sha256']
        receipt['complete'] = True
    except Exception as exc:
        receipt['error'] = f'{type(exc).__name__}: {exc}'
    finally:
        receipt['finished_utc'] = now()
        try:
            receipt['actual_owned_tree_bytes'] = storage()
        except Exception as exc:
            receipt['complete'] = False
            receipt['storage_error'] = f'{type(exc).__name__}: {exc}'
        put(path / 'receipt.json', receipt)
        seal_started = time.perf_counter()
        records = []
        recovery = {'schema': 'search-buffer-reuse-recovery/v1', 'sealed_files': records,
                    'independent_ssd_recovery_root': path.as_posix(), 'scratch_prunable': False}
        try:
            for source in sorted(path.rglob('*')):
                if source.is_file():
                    records.append(copy_once(source, sealed / source.relative_to(path)))
        except Exception as exc:
            recovery['error'] = f'{type(exc).__name__}: {exc}'
        recovery['cold_copy_seconds'] = time.perf_counter() - seal_started
        put(path / 'recovery.json', recovery)
        if 'error' not in recovery:
            copy_once(path / 'recovery.json', sealed / 'recovery.json')
    require('error' not in recovery, recovery.get('error', 'Recovery failed'))
    require(receipt['complete'], receipt.get('error', 'Case incomplete'))
    receipt['cold_copy_seconds'] = recovery['cold_copy_seconds']
    return receipt


def run(args):
    require(sys.version_info[:3] == (3, 13, 14), 'Pinned Python 3.13.14 required')
    sys.path.insert(0, str(REPO / 'python/tools'))
    from host_slots_v1 import parse_cores
    require(bool(os.environ.get('HOST_SLOTS_CLAIM')) and len(parse_cores(os.environ.get('HOST_SLOTS_CORES', ''))) == 2,
            'Use supported host_slots admission with two cores')
    hot = SCRATCH / args.attempt
    plan = json.loads((hot / 'plan.json').read_text())
    verify(plan['coordinator'])
    verify(plan['python'])
    for record in plan['inputs']:
        verify(record)
    for item in plan['binaries'].values():
        verify(item['staged'])
        verify(item['sealed'])
    cold = Path(plan['cold_root'])
    if args.phase == 'qualify':
        results = [case(plan, 'qualify', f'{variant}-{workers}w', variant, workers)
                   for workers in (1, 2) for variant in ('baseline', 'candidate')]
        hashes = {row['primary_store_sha256'] for row in results}
        require(len(hashes) == 1, 'Exact primary outputs or work counts differ')
        totals = {workers: sum(row['whole_queue_elapsed_seconds'] for row in results if row['workers'] == workers) for workers in (1, 2)}
        selected = min(totals, key=totals.get)
        result = {'schema': 'search-buffer-reuse-qualification/v1', 'complete': True,
                  'plan_sha256': pin(hot / 'plan.json')['sha256'], 'cases': results,
                  'combined_elapsed_by_workers': totals, 'selected_workers': selected,
                  'serial_over_parallel_ratio': totals[1] / totals[2], 'primary_store_sha256': next(iter(hashes)),
                  'admitted_cores': os.environ['HOST_SLOTS_CORES']}
        put(hot / 'qualification.json', result)
        copy_once(hot / 'qualification.json', cold / 'qualification.json')
    else:
        # This is the throughput gate, before the benchmark queue spawns.
        qual = json.loads((hot / 'qualification.json').read_text())
        require(qual['complete'] and qual['plan_sha256'] == pin(hot / 'plan.json')['sha256'], 'Compatible qualification missing')
        require(qual['admitted_cores'] == os.environ['HOST_SLOTS_CORES'], 'Allocation changed; qualify new attempt')
        workers = qual['selected_workers']
        results = [case(plan, 'abba', f'{index}-{variant}', variant, workers)
                   for index, variant in enumerate(('baseline', 'candidate', 'candidate', 'baseline'), 1)]
        require({row['primary_store_sha256'] for row in results} == {qual['primary_store_sha256']}, 'Exact primary output/work mismatch')
        totals = {variant: sum(row['whole_queue_elapsed_seconds'] for row in results if row['variant'] == variant)
                  for variant in ('baseline', 'candidate')}
        result = {'schema': 'search-buffer-reuse-abba/v1', 'complete': True,
                  'cases': results, 'selected_workers': workers, 'whole_queue_totals': totals,
                  'whole_queue_speedup': totals['baseline'] / totals['candidate'],
                  'adjacent_pair_speedups': [results[0]['whole_queue_elapsed_seconds'] / results[1]['whole_queue_elapsed_seconds'],
                                            results[3]['whole_queue_elapsed_seconds'] / results[2]['whole_queue_elapsed_seconds']],
                  'primary_store_sha256': qual['primary_store_sha256'],
                  'limitation': 'Two pre-existing engineering roots at reduced budgets; fixed-work throughput only, no strength claim.'}
        put(hot / 'result.json', result)
        copy_once(hot / 'result.json', cold / 'result.json')
    print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('prepare')
    p.add_argument('--attempt', default='q001')
    p.add_argument('--candidate-binary', required=True)
    p.add_argument('--candidate-commit', required=True)
    p.add_argument('--candidate-source', default=str(SCRATCH / 'candidate-source'))
    p = sub.add_parser('run')
    p.add_argument('--attempt', default='q001')
    p.add_argument('--phase', choices=('qualify', 'abba'), required=True)
    args = parser.parse_args()
    return prepare(args) if args.command == 'prepare' else run(args)


if __name__ == '__main__':
    main()
