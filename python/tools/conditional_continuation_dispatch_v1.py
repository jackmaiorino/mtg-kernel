"""Guarded fixed-root CPU qualification and 400-continuation measurement.

No remote launch is implemented. Fresh remote availability blocks local-only
qualification until that eligible option is implemented and compared.
"""
import sys
if sys.flags.optimize:
    raise RuntimeError('Optimized execution is prohibited by imported ownership guards')
import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request
import urllib.error

B = Path('E:/mtg-meta-recovery-20260921')
REPO = Path('E:/mtg-kernel-public-stack-features-codex')
REPLAY = B/'continuation-natural-replay-001'
BUILD = B/'continuation-replay-build-001'
spec = importlib.util.spec_from_file_location('continuation_owners', B/'bo3-recorder-owners.py')
owners = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owners)


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def read(path): return json.loads(Path(path).read_bytes())
def pin(path):
    path = Path(path).resolve()
    with path.open('rb') as stream:
        return dict(path=str(path), sha256=hashlib.file_digest(stream, 'sha256').hexdigest())
def checked(ref):
    require(pin(ref['path']) == ref, 'pinned file changed: '+ref['path'])
    return Path(ref['path'])
def write(path, data):
    with Path(path).open('x', encoding='utf8') as stream:
        json.dump(data, stream, indent=2, allow_nan=False)


def fleet():
    local = owners.local_available()
    from trample_owner_inventory_v1 import inventory
    try:
        remote = inventory('haleyspc')
    except Exception as exc:
        remote = dict(error_type=type(exc).__name__, error=str(exc)[:300])
    cloud = dict(checked_unix=time.time(), allocated=False, eligible=False,
                 reason='No paid compute authority', credential_available=bool(os.environ.get('RUNPOD_API_KEY')))
    if cloud['credential_available']:
        try:
            request = urllib.request.Request('https://rest.runpod.io/v1/pods',
                headers={'Authorization':'Bearer '+os.environ['RUNPOD_API_KEY']})
            with urllib.request.urlopen(request, timeout=20) as response:
                cloud.update(http_status=response.status, pod_count=len(json.load(response)))
        except urllib.error.HTTPError as exc:
            cloud['http_status'] = exc.code
        except Exception as exc:
            cloud['error_type'] = type(exc).__name__
    return dict(checked_unix=time.time(), jack=local, haleyspc=remote, runpod=cloud)


def require_local_only(snapshot):
    remote = snapshot['haleyspc']
    require('error_type' in remote or bool(remote.get('active')),
            'HaleysPC is available: implement and compare its placement before local-only launch')


def seed_pair(name, decision, index, formal):
    domain = 'conditional-continuation-v1' if formal else 'conditional-continuation-engineering-v1'
    return [int.from_bytes(hashlib.sha256(
        f'{domain}|20260922|{name}|{decision}|{index}|{seat}'.encode('ascii')).digest()[:8], 'little')
        for seat in (0, 1)]

def derivation_strings(name, decision, index, formal):
    domain = 'conditional-continuation-v1' if formal else 'conditional-continuation-engineering-v1'
    return [f'{domain}|20260922|{name}|{decision}|{index}|{seat}' for seat in (0, 1)]


def prepare_jobs(directory, formal, inputs):
    jobs = []
    per_job = 25
    for cell in (25, 45):
        request = inputs[str(cell)]['request']
        checked(request)
        options = read(checked(inputs[str(cell)]['options']))
        for group in range(8):
            name = f'cell{cell}-batch{group}'
            folder = directory/name
            folder.mkdir()
            opts = dict(options, retain_records=False, policy_seeds=[
                seed_pair(options['match_id'], options['decision_index'], group*per_job+i, formal)
                for i in range(per_job)])
            write(folder/'options.json', opts)
            jobs.append(dict(id=name, request=request, options=pin(folder/'options.json'), n=per_job,
                derivation_strings=[derivation_strings(options['match_id'], options['decision_index'],
                    group*per_job+i, formal) for i in range(per_job)]))
    return jobs


def run_group(directory, jobs, workers, binary):
    owners.local_available()
    directory.mkdir()
    began = time.monotonic()
    def run(job):
        output = directory/(job['id']+'.json')
        command = [str(checked(binary)), '--request', str(checked(job['request'])), '--output',
                   str(output), '--continuation', str(checked(job['options']))]
        started = time.monotonic()
        with (directory/(job['id']+'.log')).open('xb') as log:
            child = subprocess.Popen(command, cwd=REPO, stdout=log, stderr=subprocess.STDOUT,
                creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            write(directory/(job['id']+'-process.json'), dict(pid=child.pid, started_unix=time.time()))
            try:
                code = child.wait(timeout=900)
            except subprocess.TimeoutExpired:
                subprocess.run(['taskkill','/PID',str(child.pid),'/T','/F'], check=True, capture_output=True)
                code = 124
        result = dict(id=job['id'], exit_code=code, seconds=time.monotonic()-started,
                      output=pin(output) if output.exists() else None, n=job['n'])
        write(directory/(job['id']+'-exit.json'), result)
        return result
    with concurrent.futures.ThreadPoolExecutor(workers) as pool:
        results = list(pool.map(run, jobs))
    seconds = time.monotonic()-began
    report = dict(workers=workers, seconds=seconds, jobs=results,
                  complete=all(r['exit_code'] == 0 and r['output'] is not None for r in results))
    write(directory/'report.json', report)
    require(report['complete'], 'incomplete group; preserve outputs, no automatic rerun')
    for job in results:
        result = read(checked(job['output']))
        require(len(result['continuation']['rows']) == job['n'], 'missing continuation rows')
    return report


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('mode', choices=('qualify','run'))
    ap.add_argument('--root', type=Path, required=True)
    args = ap.parse_args()
    root = args.root
    if args.mode == 'qualify':
        snapshot = fleet()
        root.mkdir()
        write(root/'inventory.json', snapshot)
        require_local_only(snapshot)
        build = read(BUILD/'completion.json')
        replay = read(REPLAY/'completion.json')
        require(build['complete'] and replay['complete'], 'engineering incomplete')
        require(all(r['remaining_game_equal'] and r['fresh_byte_replay'] and r['off_on_equal']
                    for r in replay['roots']), 'engineering replay missing')
        binary = build['binary']
        checked(binary)
        (root/'qualification-inputs').mkdir()
        inputs = {str(c):dict(request=pin(REPLAY/f'cell{c}-request.json'),
                             options=pin(REPLAY/f'cell{c}-same-options.json')) for c in (25,45)}
        jobs = prepare_jobs(root/'qualification-inputs', False, inputs)
        manifest = dict(source=pin(__file__), build=pin(BUILD/'completion.json'), binary=binary,
            engineering=pin(REPLAY/'completion.json'), analysis=pin(REPO/'python/tools/analyze_conditional_continuations_v1.py'),
            owners=pin(B/'bo3-recorder-owners.py'),
            eligibility=pin(Path(sys.modules['trample_owner_inventory_v1'].__file__)),
            inputs=inputs, jobs=jobs,
            formal_n=400, domain='conditional-continuation-v1', gpu_ordinal=None,
            batch_shape='Both qualification and formal: 16 batches of 25 continuations, same fixed root replay per batch; different seed domains.',
            allocation='jack CPU BelowNormal; remote eligibility rechecked before formal work')
        write(root/'manifest.json', manifest)
        reports = []
        for workers in [n for n in (1, 2, 4, 8, 16) if n <= (os.cpu_count() or 1)]:
            report = run_group(root/f'workers-{workers}', jobs, workers, binary)
            require(all(read(checked(j['output']))['continuation']['complete'] for j in report['jobs']),
                    'qualification includes incomplete continuations; diagnose before measurement')
            hashes = [r['output']['sha256'] for r in report['jobs']]
            if reports:
                require(hashes == [r['output']['sha256'] for r in reports[0]['jobs']],
                        'parallel outputs differ; formal launch prohibited')
            reports.append(report)
        best = min(reports, key=lambda r:r['seconds'])
        write(root/'choice.json', dict(manifest=pin(root/'manifest.json'), workers=best['workers'],
            report=pin(root/f"workers-{best['workers']}"/'report.json'),
            compared=[pin(root/f"workers-{r['workers']}"/'report.json') for r in reports],
            exact_outputs_equal=True, qualification_seeds_per_root=200))
        print(json.dumps({'qualified':True,'workers':best['workers'],
                          'seconds':{r['workers']:r['seconds'] for r in reports}}))
    else:
        choice = read(root/'choice.json')
        manifest = read(checked(choice['manifest']))
        for key in ('source','build','binary','engineering','analysis','owners','eligibility'):
            checked(manifest[key])
        require(manifest['source'] == pin(__file__), 'launcher changed since qualification')
        reports = [read(checked(p)) for p in choice['compared']]
        best = min(reports, key=lambda r:r['seconds'])
        require(choice['workers'] == best['workers'] and choice['exact_outputs_equal'], 'choice differs')
        require(all(r['complete'] for r in reports), 'incomplete qualification')
        baseline = [j['output']['sha256'] for j in reports[0]['jobs']]
        require(all([j['output']['sha256'] for j in r['jobs']] == baseline for r in reports), 'qualification mismatch')
        snapshot = fleet()
        write(root/'formal-inventory.json', snapshot)
        require_local_only(snapshot)
        (root/'formal-inputs').mkdir()
        jobs = prepare_jobs(root/'formal-inputs', True, manifest['inputs'])
        write(root/'formal-plan.json', dict(jobs=jobs, choice=pin(root/'choice.json'), n=400))
        report = run_group(root/'formal', jobs, choice['workers'], manifest['binary'])
        refs = [j['output'] for j in report['jobs']]
        write(root/'formal-outputs.json', refs)
        from analyze_conditional_continuations_v1 import analyze
        result = analyze(refs)
        write(root/'analysis.json', result)
        print(json.dumps({'complete':result['complete'],'outcome':result['outcome'],'roots':result['roots']}))


if __name__ == '__main__':
    main()
