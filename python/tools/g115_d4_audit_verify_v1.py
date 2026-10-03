"""Bounded WMI owner for read-only verification of a completed D4 audit shard."""
import argparse
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
    require(os.name == 'nt' and host in ('jack', 'haleyspc'), 'Windows audit verification only')
    require(plan['schema'] == 'g115-d4-audit-verification/v1', 'Wrong verification schema')
    require(pinned(plan['documents']['launcher']) == Path(__file__).resolve(), 'Wrong owner')
    pinned(plan['transport']['dispatcher'])
    expected = {Path(__file__).with_name(n).resolve() for n in
                ('g115_d4_build_launch_v1.py', 'windows_owned_child_policy_v1.py')}
    require({pinned(p) for p in plan['dependencies']} == expected, 'Helper pins differ')
    verifier = pinned(plan['verifier'])
    require(verifier.name == 'verify-d4-reuse-audit-001.py', 'Named existing verifier required')
    source = Path(plan['source_worker']).resolve(strict=True)
    completion_path = pinned(plan['source_completion'])
    require(completion_path == source/'control/completion.json', 'Wrong source receipt')
    completion = json.loads(completion_path.read_bytes())
    require(completion['complete'] and not completion['pending'], 'Source shard not complete')
    require(120 <= plan['total_seconds'] <= 3600, 'Verification bound must be 120..3600 seconds')
    root = Path(plan['worker_root']).resolve()
    require(root.is_absolute() and root.drive.lower() == ('e:' if host == 'jack' else 'c:')
            and not root.exists() and not root.is_relative_to(source), 'Fresh separate host root required')
    require(0 < plan['output_cap_bytes'] <= 2*1024**3, 'Bounded verification outputs required')
    require(shutil.disk_usage(root.anchor).free >= 60*1024**3 + plan['output_cap_bytes'], 'Disk reserve unavailable')
    require(available_memory() >= (32 if host == 'jack' else 8)*1024**3, 'Memory reserve unavailable')
    return verifier, source, root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--root')
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    plan = json.loads(Path(args.manifest).read_bytes())
    verifier, source, root = admission(plan, args.host)
    if args.check_only:
        print(json.dumps({'admitted': True, 'read_only_source': True, 'native_started': False}))
        return
    require(args.root and Path(args.root).resolve() == root, 'Worker root differs')
    root.mkdir()
    began = time.monotonic()
    result = {'complete': False, 'read_only_source': True, 'source_completion': plan['source_completion']}
    child = None
    try:
        with (root/'stdout.txt').open('w') as out, (root/'stderr.txt').open('w') as err:
            child = subprocess.Popen([sys.executable, '-B', str(verifier), str(source), str(root/'verified')],
                                     stdout=out, stderr=err,
                                     creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            result['scheduling'] = configure_owned_child(child, sys.executable)
            while child.poll() is None:
                require(time.monotonic()-began < plan['total_seconds'], 'Verification time bound exceeded')
                require(shutil.disk_usage(root.anchor).free >= 60*1024**3, 'Disk reserve crossed')
                require(sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) <= plan['output_cap_bytes'],
                        'Verification output cap crossed')
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    pass
        require(child.returncode == 0, 'Verifier failed; retained stderr has the cause')
        require(sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) <= plan['output_cap_bytes'],
                'Completed verification output cap crossed')
        bundle = json.loads((root/'verified/bundle.json').read_bytes())
        require(bundle['complete'] and bundle['completion_sha256'] == plan['source_completion']['sha256'],
                'Verified source identity differs')
        require(digest(root/'verified/results.zip') == bundle['bundle_sha256'], 'Bundle digest differs')
        require(digest(source/'control/completion.json') == plan['source_completion']['sha256'], 'Source receipt changed')
        result.update(complete=True, verification=bundle)
    except Exception as error:
        result['error'] = repr(error)
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()
        result['seconds'] = time.monotonic()-began
        save(root/'completion.json', result)
    print(json.dumps(result))
    if not result['complete']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
