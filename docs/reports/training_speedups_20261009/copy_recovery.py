"""Copy one pinned recovery plan to D: and independently verified desktop E:.

Never retries, overwrites, prunes, or executes remotely. Preserve partial copies
and a failure receipt; choose a new case label only after investigating failure.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import platform
import re
import shutil
import subprocess
import time


REMOTE_MEASURE = PureWindowsPath('C:/mtg-node/training-speedups-20261009/measure')
LOCAL_MIRROR = Path('D:/training-speedups-20261009/recovery')
LOCAL_COLD = Path('E:/training-speedups-20261009')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    with Path(path).open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def save(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())


def safe_local(path):
    path = Path(path)
    require(path.is_absolute() and '..' not in path.parts, 'absolute local path required')
    for entry in (path, *path.parents):
        try:
            metadata = entry.lstat()
        except FileNotFoundError:
            continue
        require(not entry.is_symlink() and not entry.is_junction()
                and not (getattr(metadata, 'st_file_attributes', 0) & 0x400),
                'local reparse/link path refused: ' + str(entry))
    return path


def remote_path(value):
    require(isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9_./\\: -]+', value),
            'unsupported remote path spelling')
    path = PureWindowsPath(value)
    require(path.is_absolute() and '..' not in path.parts and path.is_relative_to(REMOTE_MEASURE),
            'remote source outside owned comparison measure root')
    return path


def relative_path(value):
    require(isinstance(value, str), 'relative destination must be text')
    path = PureWindowsPath(value)
    require(not path.is_absolute() and not path.drive and not path.root
            and path.parts and '..' not in path.parts
            and path.parts[0] in ('cold', 'dispatch', 'maintenance')
            and all(re.fullmatch(r'[A-Za-z0-9_. -]+', part) for part in path.parts),
            'invalid relative recovery destination')
    return Path(*path.parts)


def scp(target, source, destination):
    safe_local(destination)
    require(not destination.exists(), 'preserve existing SCP destination')
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(['scp', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
                    f'{target}:{source.as_posix()}', str(destination)], check=True,
                   capture_output=True, text=True)
    safe_local(destination)
    require(destination.is_file(), 'SCP did not publish a regular file')
    with destination.open('r+b') as stream:
        os.fsync(stream.fileno())


def cold_copy(source, destination):
    safe_local(source)
    safe_local(destination)
    require(not destination.exists(), 'preserve existing E: destination')
    destination.parent.mkdir(parents=True, exist_ok=True)
    with source.open('rb') as original, destination.open('xb') as copied:
        shutil.copyfileobj(original, copied, length=1024 * 1024)
        copied.flush()
        os.fsync(copied.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case-label', required=True)
    parser.add_argument('--remote-plan', required=True)
    parser.add_argument('--plan-sha256', required=True)
    parser.add_argument('--ssh-target', default='haley@100.71.75.65')
    args = parser.parse_args()
    require(os.name == 'nt' and platform.node().upper() == 'DESKTOP-DJ1C40R', 'run only on Jack desktop Windows')
    require(re.fullmatch(r'[a-zA-Z0-9][a-zA-Z0-9_-]{0,80}', args.case_label), 'invalid case label')
    require(re.fullmatch(r'[a-f0-9]{64}', args.plan_sha256), 'explicit SHA256 plan pin required')
    require(re.fullmatch(r'[A-Za-z0-9_][A-Za-z0-9_.@:-]*', args.ssh_target), 'invalid SSH target')
    remote_plan = remote_path(args.remote_plan)
    require(remote_plan == REMOTE_MEASURE / 'maintenance' / args.case_label / 'recovery-copy-plan.json',
            'remote plan must belong to the exact case maintenance root')
    mirror = safe_local(LOCAL_MIRROR / args.case_label)
    cold = safe_local(LOCAL_COLD / args.case_label)
    require(not mirror.exists() and not cold.exists(), 'fresh D: and E: case roots required; no overwrite/retry')
    mirror.mkdir(parents=True)
    began = time.monotonic()
    timers = dict(scp_seconds=0.0, d_verify_seconds=0.0, e_copy_fsync_seconds=0.0, e_verify_seconds=0.0)
    receipt = {'schema': 'training-speedup-independent-cold-copy/v1', 'complete': False,
               'plan': {'path': str(remote_plan), 'sha256': args.plan_sha256},
               'destination_host': platform.node(), 'destination_root': str(cold),
               'mirror_root': str(mirror), 'ssh_target': args.ssh_target, 'files': [],
               'started_utc': datetime.now(timezone.utc).isoformat(), 'timing': timers}
    try:
        cold.mkdir(parents=True)
        local_plan = mirror / 'recovery-copy-plan.json'
        phase = time.monotonic()
        scp(args.ssh_target, remote_plan, local_plan)
        timers['scp_seconds'] += time.monotonic() - phase
        phase = time.monotonic()
        plan_pin = pin(local_plan)
        require(plan_pin['sha256'] == args.plan_sha256, 'fetched plan differs from explicit pin')
        plan = json.loads(local_plan.read_bytes())
        timers['d_verify_seconds'] += time.monotonic() - phase
        require(plan['schema'] == 'training-speedup-independent-cold-copy-plan/v1'
                and plan['destination_host'].upper() == platform.node().upper()
                and plan['destination_drive'] == 'E:'
                and plan['source_host'].upper() != platform.node().upper()
                and plan['requires_full_destination_sha256_verification'] is True,
                'plan independent destination identity differs')
        receipt['source_host'] = plan['source_host']
        receipt['local_plan'] = plan_pin
        # Validate the entire manifest before fetching any payload.
        destinations, sources, jobs = set(), set(), []
        require(plan['files'], 'empty recovery plan')
        for item in plan['files']:
            source = remote_path(item['source']['path'])
            relative = relative_path(item['relative_destination'])
            require(re.fullmatch(r'[a-f0-9]{64}', item['source']['sha256'])
                    and type(item['bytes']) is int and item['bytes'] >= 0, 'invalid source pin/size')
            require(str(relative).casefold() not in destinations and str(source).casefold() not in sources,
                    'duplicate recovery source or destination')
            destinations.add(str(relative).casefold()); sources.add(str(source).casefold())
            jobs.append((item, source, relative))
        projected = sum(item['bytes'] for item, _, _ in jobs)
        for folder in (mirror, cold):
            require(shutil.disk_usage(folder).free >= 60 * 1024**3 + projected,
                    'independent copy would violate 60 GiB target reserve: ' + str(folder))
        for item, source, relative in jobs:
            local = safe_local(mirror / relative)
            destination = safe_local(cold / relative)
            phase = time.monotonic()
            scp(args.ssh_target, source, local)
            timers['scp_seconds'] += time.monotonic() - phase
            phase = time.monotonic()
            local_pin = pin(local)
            require(local.stat().st_size == item['bytes'] and local_pin['sha256'] == item['source']['sha256'],
                    'D: recovery mirror checksum/size differs')
            timers['d_verify_seconds'] += time.monotonic() - phase
            phase = time.monotonic()
            cold_copy(local, destination)
            timers['e_copy_fsync_seconds'] += time.monotonic() - phase
            phase = time.monotonic()
            destination_pin = pin(destination)
            require(destination.stat().st_size == item['bytes'] and destination_pin['sha256'] == item['source']['sha256'],
                    'E: independent recovery checksum/size differs')
            timers['e_verify_seconds'] += time.monotonic() - phase
            receipt['files'].append({'source': item['source'], 'bytes': item['bytes'], 'verified': True,
                                     'destination': destination_pin, 'local_mirror': local_pin})
        receipt['complete'] = True
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        if isinstance(error, subprocess.CalledProcessError):
            receipt['stderr'] = error.stderr[-4000:]
        raise
    finally:
        receipt['seconds'] = time.monotonic() - began
        receipt['finished_utc'] = datetime.now(timezone.utc).isoformat()
        name = 'cold-copy-receipt.json' if receipt['complete'] else 'cold-copy-failure.json'
        path = mirror / name
        save(path, receipt)
        print(json.dumps({'complete': receipt['complete'], 'receipt': pin(path)}))


if __name__ == '__main__':
    main()
