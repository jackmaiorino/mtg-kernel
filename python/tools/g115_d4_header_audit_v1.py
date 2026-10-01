"""Bounded WMI owner for read-only D4 trajectory-header extraction on Jack."""
import argparse
import gzip
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

from g115_d4_build_launch_v1 import available_memory, digest, pinned, require, save
from windows_owned_child_policy_v1 import configure_owned_child


def admission(plan, host):
    require(os.name == 'nt' and host == 'jack', 'Header extraction is Jack Windows only')
    require(plan['schema'] == 'g115-d4-header-audit/v1', 'Wrong header schema')
    require(pinned(plan['documents']['launcher']) == Path(__file__).resolve(), 'Wrong owner')
    pinned(plan['transport']['dispatcher'])
    expected = {Path(__file__).with_name(n).resolve() for n in
                ('g115_d4_build_launch_v1.py', 'windows_owned_child_policy_v1.py')}
    require({pinned(ref) for ref in plan['dependencies']} == expected, 'Helper pins differ')
    extractor = pinned(plan['extractor'])
    require(extractor.name == 'extract_headers_v2.py', 'Named read-only extractor required')
    index = pinned(plan['index'])
    retained = pinned(plan['retained'])
    failure = json.loads(pinned(plan['retained_failure']).read_bytes())
    require(not failure['complete'] and failure['rows'] == plan['retained_rows'], 'Wrong partial receipt')
    jobs = [(r['endpoint'], r['update'], s, ref)
            for r in json.loads(index.read_bytes()) if r['host'] == 'jack'
            for s, ref in enumerate(r['trajectories'])]
    require(len(jobs) == 24000, 'Expected the fixed 24,000 Jack game slots')
    count = 0
    with gzip.open(retained, 'rt', encoding='utf-8') as stream:
        for line in stream:
            row = json.loads(line)
            require((row['endpoint'], row['update'], row['slot'], row['trajectory']) == jobs[count],
                    'Retained row differs from original index')
            count += 1
    require(count == plan['retained_rows'] and count < len(jobs), 'Invalid retained count')
    require(120 <= plan['total_seconds'] <= 1800, 'Header owner bound must be 120..1800 s')
    root = Path(plan['worker_root']).resolve()
    require(root.is_absolute() and root.drive.lower() == 'e:' and not root.exists(), 'Fresh E-drive output required')
    require(0 < plan['output_cap_bytes'] <= 1024**3, 'Output bound must be at most 1 GiB')
    require(shutil.disk_usage(root.anchor).free >= 60*1024**3 + plan['output_cap_bytes'], 'Disk reserve unavailable')
    require(available_memory() >= 32*1024**3, 'Memory reserve unavailable')
    return extractor, index, retained, root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root')
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(Path(args.manifest).read_bytes())
    extractor, index, retained, root = admission(plan, args.host)
    if args.check_only:
        print(json.dumps({'admitted': True, 'read_only_inputs': True, 'retained_rows': plan['retained_rows'],
                          'remaining_rows': 24000-plan['retained_rows'], 'native_started': False}))
        return
    require(args.root and Path(args.root).resolve() == root, 'Worker root differs')
    root.mkdir()
    start = time.monotonic()
    result = {'complete': False, 'read_only_inputs': True, 'native_started': False}
    child = None
    try:
        with (root/'stdout.txt').open('w') as out, (root/'stderr.txt').open('w') as err:
            child = subprocess.Popen([sys.executable, '-B', str(extractor), '--index', str(index),
                '--index-sha256', plan['index']['sha256'], '--host', 'jack', '--output', str(root/'headers'),
                '--retained', str(retained), '--retained-sha256', plan['retained']['sha256'],
                '--retained-rows', str(plan['retained_rows']), '--seconds', str(plan['total_seconds']-15)],
                stdout=out, stderr=err, creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            result['scheduling'] = configure_owned_child(child, sys.executable)
            while child.poll() is None:
                require(time.monotonic()-start < plan['total_seconds'], 'Header extraction time bound exceeded')
                require(shutil.disk_usage(root.anchor).free >= 60*1024**3, 'Disk reserve crossed')
                require(sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) <= plan['output_cap_bytes'], 'Output cap crossed')
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    pass
        require(child.returncode == 0, 'Header extraction failed; inspect retained stderr')
        require(sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) <= plan['output_cap_bytes'], 'Completed output cap crossed')
        receipt = json.loads((root/'headers/completion.json').read_bytes())
        require(receipt['complete'] and receipt['rows'] == 24000, 'Header coverage incomplete')
        require(receipt['index_sha256'] == plan['index']['sha256'] and
                receipt['retained_sha256'] == plan['retained']['sha256'], 'Header input binding differs')
        require(digest(root/'headers/headers.jsonl.gz') == receipt['headers']['sha256'], 'Header output digest differs')
        result.update(complete=True, extraction=receipt)
    except Exception as exc:
        result['error'] = repr(exc)
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()
        result['seconds'] = time.monotonic()-start
        save(root/'completion.json', result)
    print(json.dumps(result))
    if not result['complete']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
