"""Independent exact-Pod deadline/boot guard; never touches a volume.

Runs on the controlling workstation beside the Pod-resident lease guard and
covers what that guard cannot: a Pod that never boots or loses its resident
guard, cost and the lease deadline. Native activity and controller freshness
stay with the resident guard once the worker has started.

Behaviour is that of the out-of-repository script the g115 D3 cloud attempts
002 and 003 ran (sha256 a9df4854689b0dc9e8223d93c85c8902c88b21da7df970ad634639aa3d9e17c5),
with imports resolved next to this file and the key read from the environment
(or, on Windows, the user's registry environment). No provider call on import.

usage: external_guard.py CONTROL_DIR   (CONTROL_DIR/lease/lease.json required)
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.request
from pathlib import Path

from common import read, write
from lease_guard import Provider, decision, validate, verify_pod_identity

POLL_SECONDS = 15


def list_pods(key):
    request = urllib.request.Request('https://rest.runpod.io/v1/pods', headers={
        'Authorization': 'Bearer ' + key, 'User-Agent': 'Mozilla/5.0 g115-external-guard/1'})
    with urllib.request.urlopen(request, timeout=15) as response:
        return json.load(response)


def api_key():
    key = os.environ.get('RUNPOD_API_KEY')
    if not key and os.name == 'nt':
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as handle:
            key = winreg.QueryValueEx(handle, 'RUNPOD_API_KEY')[0]
    if not key:
        raise SystemExit('RUNPOD_API_KEY unavailable')
    return key


def run(root, key, pods=list_pods, provider=Provider, clock=time.time, sleep=time.sleep):
    root = Path(root); lease = read(root / 'lease/lease.json'); validate(lease)
    created = None; api = None; highest = lease['rate_ceiling_usd_hour']
    while True:
        try:
            if api is None:
                if (root / 'lease/created-pod.json').exists():
                    created = read(root / 'lease/created-pod.json')
                else:
                    matches = [p for p in pods(key) if p['name'] == lease['name']]
                    if len(matches) == 1:
                        created = {'id': matches[0]['id']}
                    elif matches:
                        raise ValueError('Exact-name collision; do not delete ambiguous ownership')
                    elif clock() >= lease['deadline_epoch'] or (
                            (root / 'completion.json').exists() and clock() - lease['created_epoch'] > 600):
                        write(root / 'external-guard-completion.json', {
                            'exact_name': lease['name'], 'provider_absent': True, 'epoch': clock()}, replace=True)
                        return
                if created is None:
                    sleep(POLL_SECONDS); continue
                api = provider(key, created['id'])
                write(root / 'external-discovered-pod.json',
                      {'id': created['id'], 'name': lease['name'], 'epoch': clock()}, replace=True)
            pod = api.call('GET')
            if pod is None:
                write(root / 'external-guard-completion.json',
                      {'pod_id': created['id'], 'provider_absent': True, 'epoch': clock()}, replace=True)
                return
            verify_pod_identity(pod, lease, created['id']); highest = max(highest, float(pod['costPerHr']))
            status = decision(lease, clock(), highest)
            reason = status['reason']
            # The resident guard owns native activity and controller freshness.
            # This independent guard covers only boot failure, cost and deadline.
            if reason == 'startup_idle' and (root / 'worker-started.json').exists():
                reason = None
            if not (root / 'worker-started.json').exists() and \
                    clock() - lease['created_epoch'] > lease['startup_idle_seconds']:
                reason = 'startup_idle'
            if (root / 'release-authorized.json').exists():
                reason = 'controller_release'
            write(root / 'external-guard.json', dict(pod_id=created['id'], epoch=clock(), reason=reason,
                  **{k: v for k, v in status.items() if k != 'reason'}), replace=True)
            if reason:
                api.call('DELETE')
        except Exception as e:  # noqa: BLE001 - the guard must outlive any single failure
            write(root / 'external-guard-error.json', {'epoch': clock(), 'error_type': type(e).__name__},
                  replace=True)
        sleep(POLL_SECONDS)


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    run(sys.argv[1], api_key())
