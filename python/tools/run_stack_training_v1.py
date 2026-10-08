"""Supported stack trainer launcher. Raw native binary is an internal interface.

Engineering mode permits at most three updates and twelve terminal games.
Substantial local runs require binary/config-bound useful-compute qualification.
This is a launcher check, not an operating-system prohibition on other commands.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from compute_throughput_v2 import require_allocation


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--binary-sha256', required=True)
    p.add_argument('--request', type=Path, required=True)
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--engineering-question')
    p.add_argument('--allocation', type=Path)
    p.add_argument('--config', type=Path)
    p.add_argument('--job', default='stack')
    p.add_argument('--timeout', type=int, default=900)
    a = p.parse_args()
    if sha(a.binary) != a.binary_sha256:
        raise ValueError('binary differs from pin')
    request = json.loads(a.request.read_text())
    config = request['config']
    updates = config['updates']
    last = request.get('stop_after') or len(updates)
    first = 0
    if request.get('resume'):
        pin = request['resume']
        if sha(pin['path']) != pin['sha256']:
            raise ValueError('resume checkpoint differs from pin')
        first = json.loads(Path(pin['path']).read_text())['next_update']
    if not 0 <= first < last <= len(updates):
        raise ValueError('invalid update interval')
    gpu = request.get('execution_gpu_ordinal')
    gpu = config['gpu_ordinal'] if gpu is None else gpu
    workers = request.get('collector_workers', 1)
    selection = None
    if a.engineering_question:
        if not a.engineering_question.strip() or a.allocation:
            raise ValueError('engineering question or mode invalid')
        if last-first > 3 or sum(map(len, updates[first:last])) > 12 or a.timeout > 900:
            raise ValueError('engineering scope exceeds three updates/twelve games/900 seconds')
    else:
        if not a.allocation or not a.config:
            raise ValueError('substantial training requires allocation and pinned config evidence')
        if json.loads(a.config.read_text()) != config:
            raise ValueError('allocation config differs from request')
        selection = require_allocation(a.allocation, a.binary_sha256,
            {a.job: {'config_sha256': sha(a.config), 'updates': len(updates)}})
        placement = selection['placements'][a.job]
        if placement['host'] != 'desktop' or placement['gpu_ordinal'] != gpu or placement['workers'] != workers:
            raise ValueError('selected allocation needs another placement; local launcher will not substitute')
    if os.environ.get('CUDA_VISIBLE_DEVICES') or gpu != 1 or not 1 <= workers <= 64:
        raise ValueError('this local launcher requires original GPU1 mapping and valid collectors')
    if not 1 <= a.timeout <= 86400:
        raise ValueError('invalid timeout')
    inventory = subprocess.check_output(['nvidia-smi', '--query-gpu=index,uuid,utilization.gpu,memory.used', '--format=csv,noheader'], text=True)
    line = next(row for row in inventory.splitlines() if row.startswith('1,'))
    uuid = line.split(',')[1].strip()
    if uuid != 'GPU-0642d3ca-e3d4-ba16-96ab-c561c6da90e3' or int(line.split(',')[2].strip().split()[0]) != 0 or int(line.split(',')[3].strip().split()[0]) >= 100:
        raise ValueError('reserved GPU1 identity or idle state differs')
    if selection and selection['placements'][a.job]['gpu_uuid'] != uuid:
        raise ValueError('qualified GPU differs from live device')
    if Path(request['output_directory']).exists():
        raise ValueError('native output directory already exists')
    a.root.mkdir()
    def write(name, value):
        with (a.root/name).open('x') as f:
            json.dump(value, f, indent=2)
    write('manifest.json', dict(binary=str(a.binary), binary_sha256=a.binary_sha256,
        request=str(a.request), request_sha256=sha(a.request), selection=selection,
        engineering_question=a.engineering_question, gpu_inventory=inventory,
        non_claim='Engineering execution is not playing-strength evidence or campaign qualification.'))
    env = os.environ.copy()
    env['PATH'] = 'C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8/bin;' + env['PATH']
    (a.root/'temp').mkdir()
    env['TEMP'] = env['TMP'] = str(a.root/'temp')
    started = time.time()
    with (a.root/'native.log').open('wb') as log:
        child = subprocess.Popen([str(a.binary), str(a.request)], env=env,
            stdout=log, stderr=subprocess.STDOUT,
            creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
        write('process.json', dict(pid=child.pid, started_unix=started))
        timeout = False
        try:
            code = child.wait(timeout=a.timeout)
        except subprocess.TimeoutExpired:
            subprocess.run(['taskkill', '/PID', str(child.pid), '/T', '/F'], check=True, capture_output=True)
            child.wait()
            code, timeout = 124, True
    write('execution.json', dict(exit_code=code, timeout=timeout, started_unix=started,
        finished_unix=time.time(), seconds=time.time()-started,
        binary_sha256=a.binary_sha256, request_sha256=sha(a.request), observed_gpu_uuid=uuid))
    return code


if __name__ == '__main__':
    raise SystemExit(main())
