"""Independent workstation Pod guard; never touches a volume.

Runs on the controlling workstation beside the Pod-resident lease guard and
covers what that guard cannot: a Pod that never boots or loses its resident
guard, cost and the lease deadline. Native activity and controller freshness
stay with the resident guard once the worker has started. No provider call on
import; the key is read from the environment (or, on Windows, the user's
registry environment), kept in memory and never written.

Ownership: every Pod with this lease's exact name is this lease's own. Five
invariants hold on every path that obtains a Pod ID (POST in prepare_lease and
cloud_lease, census, GET, adoption, ambiguity records, restart load) and every
path that ends supervision (completion, deadline, release, verify-absent,
creator exit):

I1 retain first: a Pod ID is validated as a raw string (valid_id, no
   coercion) and handed off before any other field is validated or any other
   receipt is written. A POST ID is retained whatever the response's name; each
   valid census entry is retained on its own before malformed entries are
   handled.
I2 fail closed: an unreadable or unwritable receipt or known-Pod record blocks
   completion and verify-absent; an ID whose record is not yet durable is
   retried every pass and blocks completion until it is.
I3 primary recovery: the primary Pod is the readable receipt's ID, else a
   retained record whose role marks it as the create result ('receipt', then
   'adopted').
I4 responsibility: a Pod ID never leaves a live process's responsibility until
   a durable handoff (lease/known-pods/<id>.json written and read back) or
   verified absence. A creator that cannot hand off deletes the Pod itself and
   verifies its absence before exiting (release_unhanded), and fails loudly when
   it cannot.
I5 evidence: absence is established only by evidence observed in the current
   process: two observations at least 30 s apart, each a 404 for the exact-ID
   GET and a successful, well-formed census that does not list the ID under any
   name. Nothing persisted confirms absence or release (no absence marker, no
   verify-absent.json): after a restart every known ID is unconfirmed until
   fresh evidence confirms it. Confirmation is never terminal: any later
   presence reactivates the ID, in the guard, release_unhanded and verify-absent.

All provider calls go through LeaseClient, which retains every owned ID a
census, GET or POST response carries before returning (I1, I4). Every ending
path (completion, a --once pass, a creator's exit, verify-absent) resolves IDs
that are neither durable nor verified absent with release_unhanded, or fails
loudly (resolve_or_alert: ALERT-pod-not-released.json and an error).

Every retained ID other than the primary is a duplicate, deleted on every pass
once a primary exists (or on any release reason). Name prefixes
(lease['study_tag']) are never targets: the shared tool has no ownership source
for them.

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
Completion needs every known ID confirmed absent in this process, two spaced
empty censuses, every known ID durable and no store error. A create attempt
(lease/create-attempt.json or lease/create-ambiguous.json) is resolved only by
readable evidence: a valid receipt, a retained 'receipt' record, a definite
provider refusal, or lease/ambiguous-resolved-by-operator.json; until then
completion is blocked, and lease/create-ambiguous.json is also a release
reason. Release authorization never proves that a create failed. once=True
runs a single pass (a watchdog).

usage: external_guard.py CONTROL_DIR [--once]   (CONTROL_DIR/lease/lease.json required)
"""
from __future__ import annotations

import contextvars
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
RELEASE_POLL_SECONDS = 5          # a creator's own release loop (I4)
ONCE_RELEASE_SECONDS = 180        # a watchdog pass's own release bound (I4)
OPERATOR_RESOLUTION = 'lease/ambiguous-resolved-by-operator.json'
KNOWN_DIR = 'lease/known-pods'
CREATE_ROLES = ('receipt', 'adopted')   # roles that mark a retained ID as the create result (I3)
POD_ID = re.compile('[A-Za-z0-9]+')


def list_pods(key):
    require_boundary('Pod census')
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
    """A Pod ID is a raw string of letters and digits; nothing is coerced (I1)."""
    return isinstance(pod_id, str) and POD_ID.fullmatch(pod_id) is not None


def census_entries(census, name):
    """(owned IDs in census order, every listed ID, malformed) from a raw census.

    Each valid exact-name entry is kept on its own; a malformed entry never
    discards the valid ones (I1). Raises when the census itself is not a list."""
    if not isinstance(census, list):
        raise ValueError('census is not a list')
    owned, listed, malformed = [], set(), False
    for entry in census:
        if not isinstance(entry, dict):
            malformed = True
            continue
        pod_id = entry.get('id')
        if valid_id(pod_id):
            listed.add(pod_id)
        if entry.get('name') == name:
            if valid_id(pod_id):
                if pod_id not in owned:
                    owned.append(pod_id)
            else:
                malformed = True
    return owned, listed, malformed


def receipt_state(root):
    """(Pod ID, error) for lease/created-pod.json: (None, None) when absent, an error when unreadable or invalid."""
    path = Path(root) / 'lease/created-pod.json'
    if not path.exists():
        return None, None
    try:
        value = read(path)
    except Exception as e:  # noqa: BLE001
        return None, 'receipt:' + type(e).__name__
    pod_id = value.get('id') if isinstance(value, dict) else None
    return (pod_id, None) if valid_id(pod_id) else (None, 'receipt:invalid')


def create_evidence(root):
    """Readable evidence naming the create's Pod: a valid receipt or a retained 'receipt' record."""
    if receipt_state(root)[0]:
        return True
    try:
        return any(role == 'receipt' for role in known_pods(root, []).values())
    except Exception:  # noqa: BLE001 - unreadable store: no evidence
        return False


def ambiguous_unresolved(root):
    root = Path(root)
    return (root / 'lease/create-ambiguous.json').exists() and not create_evidence(root) \
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
    """A POST may have allocated a Pod that no readable record names."""
    root = Path(root)
    attempted = (root / 'lease/create-attempt.json').exists() or (root / 'lease/create-ambiguous.json').exists()
    return attempted and not create_evidence(root) and not definite_create_failure(root) \
        and not (root / OPERATOR_RESOLUTION).exists()


def remember_in(folder, pod_id, role, epoch):
    """Write a Pod ID's known-Pod record (one create-once file per ID; a create role outranks others)."""
    if not valid_id(pod_id):
        raise ValueError('invalid Pod ID')
    path = Path(folder) / (pod_id + '.json')
    value = {'id': pod_id, 'role': role, 'epoch': epoch}
    if path.exists():
        if role not in CREATE_ROLES:
            return
        try:
            existing = read(path).get('role')
        except Exception:  # noqa: BLE001 - unreadable: rewrite it with the create role
            existing = None
        if existing == 'receipt' or existing == role:
            return
        write(path, value, replace=True)
        return
    try:
        write(path, value)
    except FileExistsError:
        remember_in(folder, pod_id, role, epoch)


def handoff_in(folder, pod_id, role, epoch):
    """I4: write the record and read it back; raises unless it now durably names this ID (and a create role)."""
    remember_in(folder, pod_id, role, epoch)
    value = read(Path(folder) / (pod_id + '.json'))
    if not isinstance(value, dict) or value.get('id') != pod_id:
        raise ValueError('known-Pod record does not read back')
    if role in CREATE_ROLES and value.get('role') not in CREATE_ROLES:
        raise ValueError('known-Pod record lost its create role')
    return True


def remember(root, pod_id, role, epoch):
    remember_in(Path(root) / KNOWN_DIR, pod_id, role, epoch)


def handoff(root, pod_id, role, epoch):
    return handoff_in(Path(root) / KNOWN_DIR, pod_id, role, epoch)


def known_pods(root, errors=None):
    """{id: role} from the durable store. Absence is never read from disk (I5).

    Each record is read on its own. With an `errors` list, an unreadable record
    keeps its ID (role None, still a delete target) and is reported there;
    without one, the first unreadable record raises."""
    folder = Path(root) / KNOWN_DIR
    if not folder.exists():
        return {}
    result = {}
    for path in folder.iterdir():
        name = path.name
        if not name.endswith('.json') or name.endswith('.absent.json'):
            continue   # .absent.json files of earlier versions are ignored: absence needs fresh evidence
        pod_id = name[:-len('.json')]
        if not valid_id(pod_id):
            continue
        try:
            value = read(path)
            if not isinstance(value, dict) or value.get('id') != pod_id:
                raise ValueError('known-Pod record malformed')
            role = value.get('role')
        except Exception as e:  # noqa: BLE001
            if errors is None:
                raise
            errors.append(pod_id + ':' + type(e).__name__); role = None
        result[pod_id] = role
    return result


_BOUNDARY = contextvars.ContextVar('lease_provider_boundary', default=False)


class AlertError(RuntimeError):
    """Owned Pods were neither handed off nor verified absent; ALERT-pod-not-released.json names them."""


def require_boundary(what):
    """Runtime half of the provider boundary: the raw clients run only inside a LeaseClient call."""
    if not _BOUNDARY.get():
        raise RuntimeError(what + ' must go through LeaseClient (the provider boundary)')


class PodApi(Provider):
    """The raw workstation Pod GET/DELETE client. It runs only inside a LeaseClient call; the resident
    guard on the Pod keeps using lease_guard.Provider for its own runtime ID."""

    def call(self, method):
        require_boundary('Pod ' + method)
        return Provider.call(self, method)


class LeaseClient:
    """The provider boundary of the lease tools, used as a context manager by every entry point (a test
    enforces both).

    Inside it, every census and GET response retains each owned (exact-name) Pod ID it carries, and
    every POST response its raw ID whatever its name, before the caller sees the data (I1). Retention is
    a durable handoff, written and read back (I4); an ID whose handoff fails stays in `unpersisted` and
    in memory, and nothing ever leaves `retained`. Leaving the block, on any exit (exceptions included),
    runs resolve_or_alert: IDs that are neither durable nor verified absent are deleted and verified, or
    an ALERT record is written and RuntimeError raised. The raw clients refuse to run outside it."""

    def __init__(self, folder, name, key, provider=None, pods=None, clock=None, sleep=None, alert_path=None,
                 lease_deadline=0.0, min_release_seconds=120):
        self.folder = Path(folder); self.name = name; self.key = key
        self._provider = provider or PodApi
        self._pods = pods or list_pods
        self.clock = clock or time.time
        self.sleep = sleep or time.sleep
        self.alert_path = Path(alert_path) if alert_path else self.folder.parent / 'ALERT-pod-not-released.json'
        self.lease_deadline = lease_deadline; self.min_release_seconds = min_release_seconds
        self.retained = {}          # Pod ID -> role
        self.unpersisted = set()
        self.durable = set()        # IDs named by another readable record (a receipt), durable without a handoff
        self.verified_absent = set()
        self.persisted = set()      # IDs whose own known-Pod record was written (or read) back
        self.errors = []            # retention failures since the caller last cleared it
        self._active = False

    def __enter__(self):
        self._active = True
        self.load_durable()
        return self

    def receipt_ids(self):
        """IDs named by this lease's create receipt and ambiguity records, when readable and valid."""
        found = set()
        for name, field in (('created-pod.json', 'id'), ('create-ambiguous.json', 'pod_id'),
                            ('create-pod-failure.json', 'pod_id')):
            try:
                value = read(self.folder.parent / name)
                if isinstance(value, dict) and valid_id(value.get(field)):
                    found.add(value[field])
            except Exception:  # noqa: BLE001 - missing or unreadable: names nothing
                pass
        return found

    def record_reads_back(self, pod_id):
        try:
            value = read(self.folder / (pod_id + '.json'))
            return isinstance(value, dict) and value.get('id') == pod_id
        except Exception:  # noqa: BLE001
            return False

    def load_durable(self):
        """What is already handed off is never an obligation: known-Pod records that read back and IDs
        named by readable receipts start resolved."""
        try:
            names = [path.name for path in self.folder.iterdir()] if self.folder.exists() else []
        except Exception:  # noqa: BLE001
            names = []
        for name in names:
            if name.endswith('.json') and not name.endswith('.absent.json'):
                pod_id = name[:-len('.json')]
                if valid_id(pod_id) and self.record_reads_back(pod_id):
                    self.mark_persisted(pod_id)
        for pod_id in self.receipt_ids():
            self.mark_durable(pod_id)

    def durable_now(self, pod_id):
        """Reconciled from disk at the moment of asking: a known-Pod record that reads back, or a
        readable receipt naming the ID. Destructive cleanup never touches such an ID."""
        if pod_id in self.durable or self.record_reads_back(pod_id) or pod_id in self.receipt_ids():
            self.persisted.add(pod_id); self.unpersisted.discard(pod_id)
            return True
        return False

    def open_obligations(self):
        """The unresolved obligations now: registered, not verified absent, not durable on disk."""
        return sorted(p for p in sorted(self.unpersisted - self.verified_absent) if not self.durable_now(p))

    def __exit__(self, exc_type, exc, tb):
        """Runs on every exit, BaseException included (KeyboardInterrupt, SystemExit): release and
        verify every open obligation, or write an ALERT naming every unresolved ID. An exception that
        was already propagating is re-raised unchanged; otherwise a failed resolution raises."""
        try:
            try:
                resolve_or_alert(self, self.release_deadline(), self.alert_path, self.sleep)
            except AlertError:
                if exc is None:   # the ALERT record naming the unresolved IDs is already written
                    raise
            except BaseException:
                # built from the obligations open now, including IDs the cleanup itself discovered
                self.alert(self.open_obligations(), 'interrupted before release completed')
                if exc is None:
                    raise
        finally:
            self._active = False
        return False

    def alert(self, pod_ids, reason):
        try:
            write(self.alert_path, {'pod_ids': pod_ids, 'reason': reason, 'epoch': self.clock()}, replace=True)
        except Exception:  # noqa: BLE001 - the raised error or the original exception still propagates
            pass

    def release_deadline(self):
        return max(self.lease_deadline, self.clock() + self.min_release_seconds)

    def _require_active(self):
        if not self._active:
            raise RuntimeError('LeaseClient used outside its with block')

    def _raw(self, function, *args):
        token = _BOUNDARY.set(True)
        try:
            return function(*args)
        finally:
            _BOUNDARY.reset(token)

    def register(self, pod_ids, presence=True):
        """The obligation comes first: every new ID joins `unpersisted` in one set update, before any
        handoff I/O, so an interrupt at any later point leaves it for the exit to resolve."""
        fresh = [p for p in pod_ids if valid_id(p) and p not in self.persisted and p not in self.durable]
        self.unpersisted.update(fresh)
        if presence:
            self.verified_absent.difference_update(pod_ids)

    def retain(self, pod_id, role, presence=True):
        """Register, then hand off; the obligation is cleared only after the durable readback."""
        if not valid_id(pod_id):
            return False
        self.register([pod_id], presence)
        if role in CREATE_ROLES and self.retained.get(pod_id) not in CREATE_ROLES:
            self.retained[pod_id] = role
        self.retained.setdefault(pod_id, role)
        try:
            handoff_in(self.folder, pod_id, self.retained[pod_id], self.clock())
        except Exception as e:  # noqa: BLE001 - the obligation stays; the exit resolves it
            self.errors.append('retain:' + pod_id + ':' + type(e).__name__)
            return False
        self.persisted.add(pod_id)
        self.unpersisted.discard(pod_id)
        return True

    def mark_persisted(self, pod_id):
        """The ID was read from its own durable known-Pod record (a restart load)."""
        self.persisted.add(pod_id); self.unpersisted.discard(pod_id)

    def mark_durable(self, pod_id):
        """The ID is named by a readable record the guard rereads (create receipt or ambiguity record)."""
        self.durable.add(pod_id); self.unpersisted.discard(pod_id)

    def retry(self):
        for pod_id in sorted(self.unpersisted):
            self.retain(pod_id, self.retained.get(pod_id, 'census'), presence=False)

    def census(self):
        """(raw census, owned IDs, every listed ID, malformed); each owned ID is retained first."""
        self._require_active()
        raw = self._raw(self._pods, self.key)
        owned, listed, malformed = census_entries(raw, self.name)
        self.register(owned)            # every owned ID becomes an obligation before any handoff
        for pod_id in owned:
            self.retain(pod_id, 'census')
        return raw, owned, listed, malformed

    def get(self, pod_id):
        self._require_active()
        got = self._raw(lambda: self._provider(self.key, pod_id).call('GET'))
        if isinstance(got, dict) and got.get('name') == self.name and valid_id(got.get('id')):
            self.register([got['id']])
            self.retain(got['id'], 'census')
        return got

    def delete(self, pod_id):
        self._require_active()
        return self._raw(lambda: self._provider(self.key, pod_id).call('DELETE'))

    def post(self, body, poster=None, api=None):
        """POST through the given poster function, the given create client, or the default one."""
        self._require_active()
        if poster is None:
            if api is None:
                from prepare_lease import Provider as PostApi   # lazy: prepare_lease imports this module
                api = PostApi(self.key)
            returned = self._raw(lambda: api.create(body))
        else:
            returned = self._raw(poster, body)
        if isinstance(returned, dict) and valid_id(returned.get('id')):
            self.register([returned['id']])
            self.retain(returned['id'], 'receipt')
        return returned


def release_unhanded(client, pod_ids, deadline, sleep=None):
    """I4 for an ending path that holds IDs it could not hand off: DELETE each and verify absence (I5)
    until the deadline. Confirmation is never terminal: every target is checked on every pass, any
    later presence reactivates it, and any owned ID that turns up unpersisted joins the targets.
    Returns the IDs not confirmed absent in the final pass; the caller must fail loudly on any."""
    sleep = sleep or client.sleep
    targets = set(pod_ids) | client.unpersisted
    confirmers = {}
    while True:
        try:
            _, _, listed, malformed = client.census()
            census_ok = not malformed
        except Exception:  # noqa: BLE001 - never absence
            listed, census_ok = set(), False
        # recomputed after this pass's census and reconciled with disk before any DELETE: an ID whose
        # handoff is durable is the guardian's, never deleted here
        targets = {p for p in targets | (client.unpersisted - client.verified_absent) if not client.durable_now(p)}
        if not targets:
            return []
        state = {}
        for pod_id in sorted(targets):
            try:
                client.delete(pod_id)
            except Exception:  # noqa: BLE001 - retried
                pass
            try:
                absent = client.get(pod_id) is None
            except Exception:  # noqa: BLE001 - a failed or malformed GET is never absence
                absent = False
            seen = census_ok and pod_id not in listed and absent
            state[pod_id] = confirmers.setdefault(pod_id, AbsenceConfirmer(client.clock)).observe(seen)
        targets |= client.unpersisted - client.verified_absent
        if all(state.get(pod_id) for pod_id in targets):
            client.verified_absent |= targets
            return []
        if client.clock() >= deadline:
            return sorted(pod_id for pod_id in targets if not state.get(pod_id))
        sleep(RELEASE_POLL_SECONDS)


def resolve_or_alert(client, deadline, alert_path, sleep=None, also=()):
    """Every ending path runs this (LeaseClient's exit does): IDs that are neither durable nor verified
    absent are released here (release_unhanded); if any remain, an ALERT record is written and
    RuntimeError('ALERT ...') raised. Returns the IDs this call released."""
    targets = {p for p in (set(also) | client.unpersisted) - client.verified_absent if not client.durable_now(p)}
    if not targets:
        return set()
    pending = release_unhanded(client, targets, deadline, sleep)
    if pending:
        named = sorted(set(pending) | set(client.open_obligations()))
        try:
            write(alert_path, {'pod_ids': named, 'reason': 'not handed off and not verified absent',
                               'epoch': client.clock()}, replace=True)
        except Exception:  # noqa: BLE001 - the raised error still names them
            pass
        raise AlertError('ALERT: Pods ' + ','.join(named) + ' were neither handed off nor verified absent')
    return targets & client.verified_absent


def run(root, key, pods=None, provider=None, clock=time.time, sleep=time.sleep,
        once=False, preserve=None, marker_probe=None):
    root = Path(root); lease = read(root / 'lease/lease.json'); validate(lease)
    # Leaving this block (completion, a --once pass, any exception) resolves every ID that is neither
    # durable nor verified absent, or fails loudly (I4).
    with LeaseClient(root / KNOWN_DIR, lease['name'], key, provider, pods, clock, sleep,
                     root / 'ALERT-pod-not-released.json', min_release_seconds=ONCE_RELEASE_SECONDS) as client:
        return _guard(root, lease, client, clock, sleep, once, preserve, marker_probe)


def _guard(root, lease, client, clock, sleep, once, preserve, marker_probe):
    highest = lease['rate_ceiling_usd_hour']; preserved = False; deletes = 0; last_probe = None
    census_absence = AbsenceConfirmer(clock)
    id_absence = {}
    memory = client.retained      # known ID -> role; nothing leaves it (I4)
    confirmed = set()             # IDs confirmed absent by evidence seen in this process (I5)

    def note(name, value):
        try:
            write(root / name, value, replace=True)
        except Exception:  # noqa: BLE001 - a receipt never blocks a DELETE
            pass

    def delete(pod_id, why):
        nonlocal deletes
        try:
            client.delete(pod_id); deletes += 1
            return True
        except Exception as e:  # noqa: BLE001
            note('external-delete-error.json', {'pod_id': pod_id, 'why': why, 'epoch': clock(),
                                                'error_type': type(e).__name__})
            return False

    def reactivate(pod_id, evidence):
        """Positive presence invalidates an absence confirmation: the ID is a delete target again."""
        confirmed.discard(pod_id); id_absence.pop(pod_id, None)
        note('external-reactivated-pod.json', {'pod_id': pod_id, 'evidence': evidence, 'epoch': clock()})

    while True:
        failures = []
        store_errors = []
        del client.errors[:]
        now = clock()
        # (A) reasons from the clock and local files only; persisted results never end the guard (I5)
        reason = None
        if now >= lease['deadline_epoch'] - lease['recovery_seconds']:
            reason = 'deadline'
        elif (root / 'release-authorized.json').exists():
            reason = 'controller_release'
        # local records: the durable store (restart load), the receipt, fallback IDs of create records
        try:
            for pod_id, role in known_pods(root, store_errors).items():
                if role in CREATE_ROLES and memory.get(pod_id) not in CREATE_ROLES:
                    memory[pod_id] = role
                memory.setdefault(pod_id, role)
                if role is not None:
                    client.mark_persisted(pod_id)   # its own record read back (an unreadable one stays open)
        except Exception as e:  # noqa: BLE001
            store_errors.append('store:' + type(e).__name__)
        recorded, receipt_error = receipt_state(root)
        if receipt_error:
            store_errors.append(receipt_error)
        if recorded:
            client.mark_durable(recorded)
            client.retain(recorded, 'receipt', presence=False)
        for name in ('lease/create-ambiguous.json', 'lease/create-pod-failure.json'):
            try:
                if (root / name).exists():
                    fallback = read(root / name).get('pod_id')
                    if valid_id(fallback):
                        client.mark_durable(fallback)
                        client.retain(fallback, 'receipt', presence=False)
            except Exception as e:  # noqa: BLE001
                store_errors.append(name + ':' + type(e).__name__)
        client.retry()                              # every unpersisted ID, every pass (I2, I4)

        def primary_id():
            """I3: the readable receipt, else a retained create-result record."""
            if recorded:
                return recorded
            for role in CREATE_ROLES:
                candidates = sorted(p for p, r in memory.items() if r == role)
                if candidates:
                    return candidates[0]
            return None
        # (B) after the deadline every known ID is deleted first, before anything can fail
        if reason == 'deadline':
            for pod_id in sorted(p for p in memory if p not in confirmed):
                delete(pod_id, 'deadline_first')
        # (C1) census through the client (each owned ID retained first); every listed ID, whatever
        # its name, is presence; malformed provider data is a guard failure, never absence
        before = set(memory); open_before = [p for p in memory if p not in confirmed]
        census_ok = False; malformed = False; owned = []; census_ids = set()
        try:
            _, owned, census_ids, malformed = client.census()
            census_ok = True
        except Exception as e:  # noqa: BLE001
            failures.append('census:' + type(e).__name__)
        if malformed:
            failures.append('census:malformed')
        listed = set(owned)
        new = [p for p in owned if p not in before]
        if primary_id() is None and not ambiguous_unresolved(root) and len(owned) == 1 and len(new) == 1 \
                and not open_before:
            client.retain(new[0], 'adopted')        # a single unreceipted exact-name Pod is adopted
            note('external-discovered-pod.json', {'id': new[0], 'name': lease['name'], 'epoch': clock()})
        primary = primary_id()
        for pod_id in sorted(confirmed & census_ids):
            reactivate(pod_id, 'census')
        # (C2) exact-ID GET of every known ID, confirmed ones included (a reappearance reactivates it)
        observed = {}; pod = None
        for pod_id in sorted(memory):
            try:
                got = client.get(pod_id)
            except Exception as e:  # noqa: BLE001
                failures.append('get:' + type(e).__name__); observed[pod_id] = None
                continue
            observed[pod_id] = 'absent' if got is None else 'present'
            if got is not None and pod_id in confirmed:
                reactivate(pod_id, 'get')
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
        store_errors.extend(client.errors)
        open_known = sorted(p for p in memory if p not in confirmed)
        if store_errors:
            failures.append('store:' + ','.join(store_errors))
        if failures and open_known:
            reason = reason or 'guard_failure'
        ambiguous = primary is None and ambiguous_unresolved(root)
        if ambiguous:
            reason = reason or 'ambiguous_create'
        # I5: per-ID absence from this process's evidence only
        for pod_id in open_known:
            seen_absent = census_ok and not malformed and pod_id not in census_ids and observed.get(pod_id) == 'absent'
            if id_absence.setdefault(pod_id, AbsenceConfirmer(clock)).observe(seen_absent):
                confirmed.add(pod_id)
        open_known = sorted(p for p in memory if p not in confirmed)
        census_clear = census_ok and not malformed and not listed
        census_done = census_absence.observe(census_clear)
        controller_done = not memory and (root / 'completion.json').exists() and \
            now - lease['created_epoch'] > CONTROLLER_DONE_SECONDS
        may_finish = (not open_known if memory else bool(reason or controller_done)) \
            and not store_errors and not client.unpersisted
        unresolved = create_unresolved(root)
        if census_done and may_finish and not unresolved and not ambiguous:
            note('external-guard-completion.json', {
                'exact_name': lease['name'], 'pod_id': primary, 'known_pods': sorted(memory),
                'provider_absent': True, 'absent_confirmations': 2, 'reason': reason, 'deletes': deletes,
                'epoch': clock()})
            return
        absent_now = census_clear and all(observed.get(p) == 'absent' for p in open_known)
        note('external-guard.json', dict(pod_id=primary, epoch=now, reason=reason, failures=failures,
             store_errors=list(store_errors), unpersisted=sorted(client.unpersisted), known=sorted(memory),
             confirmed_absent=sorted(confirmed), absent_now=absent_now, ambiguous=ambiguous,
             create_unresolved=unresolved, pid=os.getpid(), once=once,
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
        targets = [p for p in targets if not (observed.get(p) == 'absent' and p not in census_ids)]
        for target in targets:
            delete(target, reason or 'duplicate_pod')
        if reason and clock() >= lease['deadline_epoch'] and (targets or not absent_now):
            note('ALERT-pod-not-released.json', {'targets': targets, 'epoch': clock(), 'reason': reason})
        if once:
            # I4: the LeaseClient exit resolves any ID that is neither durable nor verified absent
            client.retry()
            return
        sleep(POLL_SECONDS)


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != '--once'):
        raise SystemExit(__doc__)
    try:
        run(sys.argv[1], api_key(), once=len(sys.argv) == 3)
    except RuntimeError as error:
        raise SystemExit(str(error))
