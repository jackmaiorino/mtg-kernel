"""One-shot read-only fleet snapshot; writes only the adjacent inventory.json.

Does not run host_slots.status(), which can remove stale claim records.
SSH receives the checkout's slot-reader source over stdin, without remote files.
"""
from __future__ import annotations

import concurrent.futures
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
SLOT_SOURCE = (ROOT / "python/tools/host_slots_v1.py").read_text(encoding="utf-8")
PS = r"""
$ErrorActionPreference='Stop'
$cpu = Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,LoadPercentage
$mem = Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize,FreePhysicalMemory
$disk = Get-CimInstance Win32_LogicalDisk -Filter 'DriveType=3' | Select-Object DeviceID,Size,FreeSpace
$gpu = & nvidia-smi --query-gpu=index,name,driver_version,memory.total,memory.used,utilization.gpu --format=csv
@{observed_utc=[DateTime]::UtcNow.ToString('o');cpu=@($cpu);memory=$mem;volumes=@($disk);gpu=@($gpu)} | ConvertTo-Json -Depth 6 -Compress
"""
READ_SLOTS = r"""
config = ns['load_config']()
timed = ns['timed_state']()
claims, queued = [], []
stale = 0
unreadable = 0
for pattern, rows in [('claim-*.json', claims), ('queue-*.json', queued)]:
    for path in sorted(ns['slots_dir']().glob(pattern)):
        row = ns['read_json'](path)
        if row is None:
            unreadable += 1
            continue
        state = ns['process_state'](row.get('pid'), row.get('creation_time'))
        if state == 'absent':
            stale += 1
            continue
        rows.append({**row, 'process_identity_state': state})
keep = ('lane', 'work_id', 'cores', 'priority', 'state', 'acquired_at', 'process_identity_state')
free = ns['available'](config, timed, claims)
data['slots'] = {
    'read_only': True,
    'reader_source_sha256': source_hash,
    'logical_cpus': ns['cpu_total'](),
    'config': config,
    'timed': {k:v for k,v in timed.items() if k != 'token_sha256'},
    'claims': [{k:c.get(k) for k in keep if k in c} for c in claims],
    'queue': [{k:c.get(k) for k in ('lane','work_id','cores_wanted','enqueued_at','process_identity_state') if k in c} for c in queued],
    'free_cores': free if not unreadable else None,
    'stale_records_ignored_without_deletion': stale,
    'unreadable_records': unreadable,
    'limitation': 'A one-shot snapshot is not a reservation; admission must be checked by the supported launcher.'
}
print(json.dumps(data))
"""


def host_code() -> str:
    return (
        "import json, subprocess\n"
        f"p=subprocess.run({['powershell', '-NoProfile', '-Command', PS]!r},text=True,capture_output=True,timeout=40,check=True)\n"
        "data=json.loads(p.stdout)\n"
        "ns={'__name__':'inventory_slot_reader'}\n"
        f"exec(compile({SLOT_SOURCE!r},'host_slots_v1.py','exec'),ns)\n"
        f"source_hash={hashlib.sha256(SLOT_SOURCE.encode()).hexdigest()!r}\n"
        + READ_SLOTS
    )


def capture_host(remote: bool) -> dict:
    if remote:
        command = [
            'ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
            'haley@100.71.75.65',
            'C:/Users/haley/AppData/Local/Programs/Python/Python312/python.exe', '-B', '-'
        ]
    else:
        command = [sys.executable, '-B', '-']
    result = subprocess.run(command, input=host_code(), text=True, capture_output=True, timeout=58, check=True)
    data = json.loads(result.stdout)
    data['connectivity'] = 'ssh_batchmode_ok' if remote else 'local_ok'
    return data


def cloud() -> dict:
    key = os.environ.get('RUNPOD_API_KEY')
    if not key:
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as handle:
            key = winreg.QueryValueEx(handle, 'RUNPOD_API_KEY')[0]
    request = urllib.request.Request('https://rest.runpod.io/v1/pods', headers={'Authorization':'Bearer ' + key})
    with urllib.request.urlopen(request, timeout=30) as response:
        rows = json.load(response)
    return {
        'observed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'read_only_api': 'GET /v1/pods',
        'new_paid_execution_authorized': False,
        'pod_count': len(rows),
        'pods': [{
            key: row.get(key) for key in ('desiredStatus', 'costPerHr', 'vcpuCount', 'memoryInGb', 'gpuCount', 'gpuTypeId', 'containerDiskInGb', 'volumeInGb')
        } for row in rows],
        'limitation': 'API state only; no pod shell, reservation changes, launches, or spend.'
    }


def main() -> None:
    data = {'schema': 'search-throughput-fleet-inventory/v1', 'observed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        jobs = {
            'jacks_pc': pool.submit(capture_host, False),
            'haleys_pc': pool.submit(capture_host, True),
            'runpod': pool.submit(cloud),
        }
        for name, job in jobs.items():
            try:
                data[name] = job.result()
            except Exception as exc:
                # Never print commands, environment values, or request objects.
                data[name] = {'error': type(exc).__name__, 'capture_failed': True}
                if isinstance(getattr(exc, 'code', None), int):
                    data[name]['http_status'] = exc.code
    out = Path(__file__).with_name('inventory.json')
    out.write_text(json.dumps(data, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(data, indent=2))


if __name__ == '__main__':
    main()
