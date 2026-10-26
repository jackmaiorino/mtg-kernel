"""Independent workstation Pod guard; never touches a volume.

Runs on the controlling workstation beside the Pod-resident lease guard and
covers what that guard cannot: a Pod that never boots or loses its resident
guard, cost and the lease deadline. Native activity and controller freshness
stay with the resident guard once the worker has started. No provider call on
import; the key is read from the environment (or, on Windows, the user's
registry environment), kept in memory and never written.

Each pass (rules ported from the reviewed ordinary-training lease adapter,
2026-10-25):
(A) reasons that need no provider data: deadline (deadline - recovery),
    release-authorized.json, and the startup allowance without
    worker-started.json;
(B) after the deadline the recorded Pod ID is DELETEd first, before anything
    that can fail;
(C) census, duplicate DELETEs, exact-ID GET and price parsing, each isolated;
    a failed census or GET is a guard failure that DELETEs the recorded ID; a
    recorded ID whose Pod carries another name is an error and is never deleted
    on that evidence alone (before the deadline);
(D) DELETE of every target while a reason exists.
Ownership: a Pod with this lease's exact name is always this guard's own
(duplicates and the late Pod of an ambiguous create), even when no ledger can
be read. With lease['study_tag'] set, Pods whose names carry the tag are also
targets unless foreign_ids() names them (Pods recorded by other leases of the
study); when foreign_ids() fails, only exact-name Pods are touched in that pass.
A missing or malformed price, or max(costPerHr, adjustedCostPerHr) above the
ceiling, releases at once. Completion needs two absent observations at least 30
s apart (exact ID absent and no owned Pod in a successful census); a failed
census is never absence. An ambiguous create (lease/create-ambiguous.json and
no recorded Pod) never resolves automatically: it is a release reason and blocks
completion until an operator writes lease/ambiguous-resolved-by-operator.json.
released() true (the lease was verified released or voided) ends the guard
without deleting anything. marker_probe(pod) may read a Pod-side worker-started
marker; a probe that cannot reach the Pod means "not ready". once=True runs a
single pass (a watchdog).

usage: external_guard.py CONTROL_DIR [--once]   (CONTROL_DIR/lease/lease.json required)
"""
from __future__ import annotations

import json
import os
import sys
import time
import urllib.request
from pathlib import Path

from common import read, write
from lease_guard import (AbsenceConfirmer, Provider, decision, pod_rate, validate,
                         verify_pod_identity)

POLL_SECONDS = 15
PRESERVE_MIN_REMAINING = 900      # no preservation with less than 15 min to the deadline
PRESERVE_MARGIN = 600             # preservation ends at least 10 min before the deadline
MARKER_PROBE_SECONDS = 60
OPERATOR_RESOLUTION = 'lease/ambiguous-resolved-by-operator.json'


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


def ambiguous_unresolved(root):
    root = Path(root)
    return (root / 'lease/create-ambiguous.json').exists() and not (root / 'lease/created-pod.json').exists() \
        and not (root / OPERATOR_RESOLUTION).exists()


def run(root, key, pods=list_pods, provider=Provider, clock=time.time, sleep=time.sleep,
        once=False, preserve=None, marker_probe=None, foreign_ids=None, released=None):
    root = Path(root); lease = read(root / 'lease/lease.json'); validate(lease)
    tag = lease.get('study_tag')
    absence = AbsenceConfirmer(clock)
    highest = lease['rate_ceiling_usd_hour']; preserved = False; deletes = 0; last_probe = None

    def note(name, value):
        try:
            write(root / name, value, replace=True)
        except Exception:  # noqa: BLE001 - a receipt never blocks a DELETE
            pass

    def delete(pod_id, why):
        nonlocal deletes
        try:
            provider(key, pod_id).call('DELETE'); deletes += 1
            return True
        except Exception as e:  # noqa: BLE001
            note('external-delete-error.json', {'pod_id': pod_id, 'why': why, 'epoch': clock(),
                                                'error_type': type(e).__name__})
            return False

    def carries_tag(p):
        name = str(p.get('name', ''))
        return name == lease['name'] or (bool(tag) and name.startswith(tag))

    while True:
        try:
            if released is not None and released():
                note('external-guard-exit.json', {'epoch': clock(), 'why': 'own lease verified released'})
                return
        except Exception:  # noqa: BLE001 - unknown: keep guarding
            pass
        failures = []
        now = clock()
        # (A) reasons from the clock and local files only
        reason = None
        if now >= lease['deadline_epoch'] - lease['recovery_seconds']:
            reason = 'deadline'
        elif (root / 'release-authorized.json').exists():
            reason = 'controller_release'
        recorded = None
        try:
            if (root / 'lease/created-pod.json').exists():
                recorded = read(root / 'lease/created-pod.json')['id']
        except Exception as e:  # noqa: BLE001
            failures.append('receipt:' + type(e).__name__)
        # (B) after the deadline the recorded ID is deleted first, before anything can fail
        if reason == 'deadline' and recorded:
            delete(recorded, 'deadline_first')
        # (C) census, ownership, duplicates, GET, price: each isolated
        matches = None
        try:
            matches = [p for p in pods(key) if carries_tag(p)]
        except Exception as e:  # noqa: BLE001
            failures.append('census:' + type(e).__name__)
        exact = [p for p in (matches or []) if p.get('name') == lease['name']]
        if recorded is None and len(exact) == 1 and not ambiguous_unresolved(root):
            # a single exact-name Pod without a receipt is adopted as this lease's Pod
            recorded = exact[0].get('id')
            note('external-discovered-pod.json', {'id': recorded, 'name': lease['name'], 'epoch': clock()})
        foreign = set()
        foreign_ok = True
        if foreign_ids is not None:
            try:
                foreign = set(foreign_ids()) - ({recorded} if recorded else set()) - {p.get('id') for p in exact}
            except Exception as e:  # noqa: BLE001 - ownership unknown: touch only exact-name Pods this pass
                foreign_ok = False
                failures.append('ownership:' + type(e).__name__)
        own_named = [p for p in exact if p.get('id') != recorded]
        other_unowned = [p for p in (matches or []) if p.get('id') != recorded and p.get('name') != lease['name']
                         and p.get('id') not in foreign] if foreign_ok else []
        unowned = own_named + other_unowned
        for extra in [p for p in unowned if recorded is not None]:
            if delete(extra['id'], 'duplicate_or_unowned_pod'):
                note('external-duplicate-delete.json', {'pod_id': extra['id'], 'epoch': clock()})
        pod = None; get_ok = False; identity_ok = True
        if recorded:
            try:
                pod = provider(key, recorded).call('GET'); get_ok = True
            except Exception as e:  # noqa: BLE001
                failures.append('get:' + type(e).__name__)
        status = {}
        if pod is not None:
            try:
                verify_pod_identity(pod, lease, recorded)
            except Exception as e:  # noqa: BLE001 - another name at the recorded ID: never deleted on this evidence
                identity_ok = False
                note('external-guard-error.json', {'epoch': clock(), 'error_type': type(e).__name__})
            if identity_ok:
                try:
                    rate = pod_rate(pod)
                    highest = max(highest, rate)
                    if rate > lease['rate_ceiling_usd_hour']:
                        reason = reason or 'rate_above_ceiling'
                except ValueError:
                    reason = reason or 'price_malformed'
                status = decision(lease, now, highest)
                if status['reason'] in ('deadline', 'cost_cap'):
                    reason = reason or status['reason']
        # startup timeout, unless the Pod-side or local marker shows productive work
        if reason is None and not (root / 'worker-started.json').exists() and pod is not None and marker_probe \
                and (last_probe is None or now - last_probe >= MARKER_PROBE_SECONDS):
            last_probe = now
            try:
                if marker_probe(pod):
                    note('worker-started.json', {'epoch': clock(), 'source': 'pod marker via guardian probe'})
            except Exception as e:  # noqa: BLE001 - "not ready"; only the startup allowance can end the lease
                note('external-probe-error.json', {'epoch': clock(), 'error_type': type(e).__name__})
        if reason is None and not (root / 'worker-started.json').exists() and \
                now - lease['created_epoch'] > lease['startup_idle_seconds']:
            reason = 'startup_idle'
        if failures and recorded:
            reason = reason or 'guard_failure'
        ambiguous = recorded is None and ambiguous_unresolved(root)
        if ambiguous:
            reason = reason or 'ambiguous_create'
        # absence: exact ID absent (successful GET or nothing recorded) AND a successful census with no owned Pod
        absent_now = matches is not None and foreign_ok and not unowned and \
            (recorded is None or (get_ok and pod is None))
        if absence.observe(absent_now) and (recorded or reason) and not ambiguous:
            note('external-guard-completion.json', {
                'exact_name': lease['name'], 'study_tag': tag, 'pod_id': recorded, 'provider_absent': True,
                'absent_confirmations': 2, 'reason': reason, 'deletes': deletes, 'epoch': clock()})
            return
        note('external-guard.json', dict(pod_id=recorded, epoch=now, reason=reason, failures=failures,
             absent_now=absent_now, ambiguous=ambiguous, foreign=sorted(foreign), pid=os.getpid(), once=once,
             **{k: v for k, v in status.items() if k != 'reason'}))
        # (D) act; a recorded ID whose Pod carries another name is deleted only by step (B) after the deadline
        if reason and not absent_now:
            remaining = lease['deadline_epoch'] - clock()
            if (preserve and not preserved and reason in ('controller_release', 'startup_idle')
                    and recorded and identity_ok and remaining > PRESERVE_MIN_REMAINING):
                preserved = True
                try:
                    preserve(recorded, min(300, remaining - PRESERVE_MARGIN))
                except Exception:  # noqa: BLE001 - preservation never blocks release
                    pass
            targets = ([recorded] if recorded and identity_ok else []) + [p['id'] for p in unowned]
            for target in targets:
                delete(target, reason)
            if clock() >= lease['deadline_epoch']:
                note('ALERT-pod-not-released.json', {'targets': targets, 'epoch': clock(), 'reason': reason})
        if once:
            return
        sleep(POLL_SECONDS)


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != '--once'):
        raise SystemExit(__doc__)
    run(sys.argv[1], api_key(), once=len(sys.argv) == 3)
