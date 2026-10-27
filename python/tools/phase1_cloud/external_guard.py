"""Independent workstation Pod guard; never touches a volume.

Runs on the controlling workstation beside the Pod-resident lease guard and
covers what that guard cannot: a Pod that never boots or loses its resident
guard, cost and the lease deadline. Native activity and controller freshness
stay with the resident guard once the worker has started. No provider call on
import; the key is read from the environment (or, on Windows, the user's
registry environment), kept in memory and never written.

Ownership: every Pod with this lease's exact name is this lease's own. Each
such ID (the create receipt, a Pod adopted or seen in a census, a duplicate
recorded by create) is retained durably under lease/known-pods/ and acted on
until that ID alone is confirmed absent: two observations at least 30 s apart,
each a 404 for its exact-ID GET and a successful, well-formed census that does
not list it. The primary Pod is the receipt's ID, else one adopted when the
census shows exactly one unreceipted Pod and no create is ambiguous; every
other retained ID is a duplicate and is deleted on every pass. Name prefixes
(lease['study_tag']) are never targets here: the shared tool has no ownership
source for them.

Each pass:
(A) reasons that need no provider data: deadline (deadline - recovery) and
    release-authorized.json;
(B) after the deadline every retained ID is DELETEd first;
(C) census, exact-ID GETs, identity and price parsing, each isolated: any
    failure or malformed provider data is a guard failure that DELETEs the
    retained IDs; a missing, malformed or above-ceiling price releases at once;
(D) DELETE of the targets. Preservation (optional) runs at most once, only for
    controller_release or startup_idle, and only inside the lease time left; it
    never gates a DELETE.
The startup allowance ends the lease without worker-started.json;
marker_probe(pod) may read a Pod-side marker instead ("not ready" on failure).
Completion needs every retained ID confirmed absent and two spaced empty
censuses. A create that may have allocated a Pod no receipt names (a
lease/create-attempt.json or lease/create-ambiguous.json with neither a receipt
nor a definite provider refusal) blocks completion until an operator writes
lease/ambiguous-resolved-by-operator.json; lease/create-ambiguous.json is also
a release reason. Release authorization never proves that a create failed.
released() true (the lease was verified released) ends the guard without
deleting anything. once=True runs a single pass (a watchdog).

usage: external_guard.py CONTROL_DIR [--once]   (CONTROL_DIR/lease/lease.json required)
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sys
import time
import urllib.request

from common import read, write
from lease_guard import (AbsenceConfirmer, Provider, decision, pod_rate, validate,
                         verify_pod_identity)

POLL_SECONDS = 15
PRESERVE_MIN_REMAINING = 900      # no preservation with less than 15 min to the deadline
PRESERVE_MARGIN = 600             # preservation ends at least 10 min before the deadline
PRESERVE_MAX_SECONDS = 300
MARKER_PROBE_SECONDS = 60
CONTROLLER_DONE_SECONDS = 600     # completion.json with nothing created ends an unused lease
OPERATOR_RESOLUTION = 'lease/ambiguous-resolved-by-operator.json'
KNOWN_DIR = 'lease/known-pods'
POD_ID = re.compile('[A-Za-z0-9]+')


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


def valid_id(pod_id):
    return isinstance(pod_id, str) and POD_ID.fullmatch(pod_id) is not None


def ambiguous_unresolved(root):
    root = Path(root)
    return (root / 'lease/create-ambiguous.json').exists() and not (root / 'lease/created-pod.json').exists() \
        and not (root / OPERATOR_RESOLUTION).exists()


def definite_create_failure(root):
    """An explicit provider refusal recorded by create (no Pod can exist)."""
    root = Path(root)
    if (root / 'lease/create-failure.json').exists():
        return True
    try:
        return read(root / 'lease/create-pod-failure.json').get('definite') is True
    except Exception:  # noqa: BLE001 - missing or unreadable: not definite
        return False


def create_unresolved(root):
    """A POST may have allocated a Pod that no receipt names."""
    root = Path(root)
    attempted = (root / 'lease/create-attempt.json').exists() or (root / 'lease/create-ambiguous.json').exists()
    return attempted and not (root / 'lease/created-pod.json').exists() and not definite_create_failure(root) \
        and not (root / OPERATOR_RESOLUTION).exists()


def remember(root, pod_id, role, epoch):
    """Durably retain a Pod ID of this lease (one create-once file per ID, so writers never race)."""
    if not valid_id(pod_id):
        raise ValueError('invalid Pod ID')
    path = Path(root) / KNOWN_DIR / (pod_id + '.json')
    if path.exists():
        return
    try:
        write(path, {'id': pod_id, 'role': role, 'epoch': epoch})
    except FileExistsError:
        pass


def known_pods(root):
    """{id: {'role': ..., 'absent': bool}} from the durable store; raises when it cannot be read."""
    folder = Path(root) / KNOWN_DIR
    if not folder.exists():
        return {}
    result = {}
    for path in folder.iterdir():
        name = path.name
        if not name.endswith('.json') or name.endswith('.absent.json'):
            continue
        pod_id = name[:-len('.json')]
        if not valid_id(pod_id):
            continue
        result[pod_id] = {'role': read(path).get('role'), 'absent': (folder / (pod_id + '.absent.json')).exists()}
    return result


def run(root, key, pods=list_pods, provider=Provider, clock=time.time, sleep=time.sleep,
        once=False, preserve=None, marker_probe=None, released=None):
    root = Path(root); lease = read(root / 'lease/lease.json'); validate(lease)
    highest = lease['rate_ceiling_usd_hour']; preserved = False; deletes = 0; last_probe = None
    census_absence = AbsenceConfirmer(clock)
    id_absence = {}
    memory = {}        # retained ID -> role (survives an unreadable store within this process)
    confirmed = set()  # IDs confirmed absent

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

    def keep(pod_id, role):
        memory.setdefault(pod_id, role)
        try:
            remember(root, pod_id, role, clock())
        except Exception as e:  # noqa: BLE001 - the in-memory copy keeps acting on it
            note('external-known-error.json', {'pod_id': pod_id, 'epoch': clock(), 'error_type': type(e).__name__})

    def mark_absent(pod_id):
        confirmed.add(pod_id)
        try:
            write(root / KNOWN_DIR / (pod_id + '.absent.json'), {'id': pod_id, 'epoch': clock(),
                  'absent_confirmations': 2}, replace=True)
        except Exception:  # noqa: BLE001
            pass

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
                receipt_id = read(root / 'lease/created-pod.json')['id']
                if not valid_id(receipt_id):
                    raise ValueError('invalid receipt Pod ID')
                recorded = receipt_id
                keep(recorded, 'receipt')
        except Exception as e:  # noqa: BLE001
            failures.append('receipt:' + type(e).__name__)
        try:
            for pod_id, info in known_pods(root).items():
                memory.setdefault(pod_id, info['role'])
                if info['absent']:
                    confirmed.add(pod_id)
        except Exception as e:  # noqa: BLE001
            failures.append('known:' + type(e).__name__)

        def primary_id():
            if recorded:
                return recorded
            adopted = sorted(p for p, role in memory.items() if role == 'adopted')
            return adopted[0] if adopted else None
        # (B) after the deadline every retained ID is deleted first, before anything can fail
        if reason == 'deadline':
            for pod_id in sorted(p for p in memory if p not in confirmed):
                delete(pod_id, 'deadline_first')
        # (C1) census: exact-name Pods only; malformed provider data is a guard failure, never absence
        census_ok = False; malformed = False; listed = set()
        try:
            raw = pods(key)
            if not isinstance(raw, list):
                raise ValueError('census is not a list')
            census_ok = True
            for entry in raw:
                if not isinstance(entry, dict):
                    malformed = True; continue
                if entry.get('name') != lease['name']:
                    continue
                if valid_id(entry.get('id')):
                    listed.add(entry['id'])
                else:
                    malformed = True
        except Exception as e:  # noqa: BLE001
            failures.append('census:' + type(e).__name__)
        if malformed:
            failures.append('census:malformed')
        new = sorted(listed - set(memory))
        open_known = [p for p in memory if p not in confirmed]
        adopt = None
        if primary_id() is None and not ambiguous_unresolved(root) and len(listed) == 1 and len(new) == 1 \
                and not open_known:
            adopt = new[0]   # a single exact-name Pod without a receipt is adopted as this lease's Pod
        for pod_id in new:
            keep(pod_id, 'adopted' if pod_id == adopt else 'census')
            if pod_id == adopt:
                note('external-discovered-pod.json', {'id': pod_id, 'name': lease['name'], 'epoch': clock()})
        primary = primary_id()
        # (C2) exact-ID GET of every retained ID not yet confirmed absent
        observed = {}; pod = None
        for pod_id in sorted(p for p in memory if p not in confirmed):
            try:
                got = provider(key, pod_id).call('GET')
            except Exception as e:  # noqa: BLE001
                failures.append('get:' + type(e).__name__); observed[pod_id] = None
                continue
            observed[pod_id] = 'absent' if got is None else 'present'
            if pod_id == primary and got is not None:
                pod = got
        # (C3) identity, price and cost of the primary, each isolated
        status = {}
        if pod is not None:
            try:
                verify_pod_identity(pod, lease, primary)
            except Exception as e:  # noqa: BLE001
                failures.append('identity:' + type(e).__name__)
                note('external-guard-error.json', {'epoch': clock(), 'error_type': type(e).__name__})
            try:
                rate = pod_rate(pod)
                highest = max(highest, rate)
                if rate > lease['rate_ceiling_usd_hour']:
                    reason = reason or 'rate_above_ceiling'
            except Exception:  # noqa: BLE001 - missing or malformed price
                reason = reason or 'price_malformed'
            try:
                status = decision(lease, now, highest)
                if status['reason'] in ('deadline', 'cost_cap'):
                    reason = reason or status['reason']
            except Exception as e:  # noqa: BLE001
                failures.append('decision:' + type(e).__name__); status = {}
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
        open_known = sorted(p for p in memory if p not in confirmed)
        if failures and open_known:
            reason = reason or 'guard_failure'
        ambiguous = primary is None and ambiguous_unresolved(root)
        if ambiguous:
            reason = reason or 'ambiguous_create'
        # per-ID absence: a 404 for the exact ID AND a well-formed census that does not list it
        for pod_id in open_known:
            seen_absent = census_ok and not malformed and pod_id not in listed and observed.get(pod_id) == 'absent'
            if id_absence.setdefault(pod_id, AbsenceConfirmer(clock)).observe(seen_absent):
                mark_absent(pod_id)
        open_known = sorted(p for p in memory if p not in confirmed)
        census_clear = census_ok and not malformed and not listed
        census_done = census_absence.observe(census_clear)
        controller_done = not memory and (root / 'completion.json').exists() and \
            now - lease['created_epoch'] > CONTROLLER_DONE_SECONDS
        may_finish = not open_known if memory else bool(reason or controller_done)
        unresolved = create_unresolved(root)
        if census_done and may_finish and not unresolved and not ambiguous:
            note('external-guard-completion.json', {
                'exact_name': lease['name'], 'pod_id': primary, 'known_pods': sorted(memory),
                'provider_absent': True, 'absent_confirmations': 2, 'reason': reason, 'deletes': deletes,
                'epoch': clock()})
            return
        absent_now = census_clear and all(observed.get(p) == 'absent' for p in open_known)
        note('external-guard.json', dict(pod_id=primary, epoch=now, reason=reason, failures=failures,
             known=sorted(memory), confirmed_absent=sorted(confirmed), absent_now=absent_now,
             ambiguous=ambiguous, create_unresolved=unresolved, pid=os.getpid(), once=once,
             study_tag_ignored=bool(lease.get('study_tag')),
             **{k: v for k, v in status.items() if k != 'reason'}))
        # (D) act: duplicates always (once a primary exists), the primary and the rest on a reason
        targets = []
        for pod_id in open_known:
            if pod_id == primary:
                continue
            if primary is not None or reason:
                targets.append(pod_id)
        if reason and primary in open_known:
            remaining = lease['deadline_epoch'] - clock()
            if (preserve and not preserved and reason in ('controller_release', 'startup_idle')
                    and remaining > PRESERVE_MIN_REMAINING):
                preserved = True
                try:
                    preserve(primary, min(PRESERVE_MAX_SECONDS, remaining - PRESERVE_MARGIN))
                except Exception:  # noqa: BLE001 - preservation never blocks release
                    pass
            targets.insert(0, primary)
        targets = [p for p in targets if not (observed.get(p) == 'absent' and p not in listed)]
        for target in targets:
            delete(target, reason or 'duplicate_pod')
        if reason and clock() >= lease['deadline_epoch'] and (targets or not absent_now):
            note('ALERT-pod-not-released.json', {'targets': targets, 'epoch': clock(), 'reason': reason})
        if once:
            return
        sleep(POLL_SECONDS)


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != '--once'):
        raise SystemExit(__doc__)
    run(sys.argv[1], api_key(), once=len(sys.argv) == 3)
