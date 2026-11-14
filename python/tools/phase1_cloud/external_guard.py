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
empty censuses, every known ID durable and no store error or unresolved observation. A create attempt
(lease/create-attempt.json or lease/create-ambiguous.json) is resolved only by
readable evidence: a valid receipt, a retained 'receipt' record, a raw POST ID, a definite
provider refusal, or lease/ambiguous-resolved-by-operator.json; until then
completion is blocked, and lease/create-ambiguous.json is also a release
reason. Release authorization never proves that a create failed, and neither
does quiet: account spend is account-wide and RunPod bounds no create delay.
After AMBIGUOUS_QUIET_SECONDS of unbroken clear exact-name censuses (monotonic
time, reset by a failure, a gap or any retained ID not freshly confirmed absent) the guard writes
ambiguous-create-quiet.json (diagnostic evidence for the operator) and
ALERT-ambiguous-create.json, retrying a failed write each pass, and keeps
supervising: only an operator's resolution ends it. The account read runs once
per window on its own thread and writes ambiguous-create-funds.json when it
returns; it never holds a pass. A census entry with an ID but no nonblank name
is malformed, never absence.

Handoffs, create attempts and receipts, conditional release claims, and completion take
one OS lock (lease/known-pods.lock). A handoff of positive presence (an ID a
census, GET or POST returned, known or new) advances that ID's presence
revision (known-pods/<id>.presence.json); a changed revision voids the guard's
absence confirmation of the ID and its census confirmation. A census or GET of an open
store starts with an fsynced intent and a live OS lock unique to that observer. The intent
survives interruption or failed handoff. A guard can resolve abandoned intents only after
a fresh complete census and exact-ID reads, advancing the recovery revision before removing
them so other guards also discard stale absence evidence. A refresh never deletes a Pod
that still belongs to an open guardian. If that guardian closes during the refresh, the
observer keeps the release obligation.

Conditional cleanup claims release durably before DELETE. A claimed ID can no longer be
handed off or given a create receipt. Claims survive restarts and lost DELETE responses;
they authorize release and never certify absence. A guard, including a watchdog after an
earlier closure, observes and releases claim IDs. The raw POST ID is also durable before
handoff, so the guard can recover it even if the exact-name census omits it.

Completion is the store's closure: under the lock it checks that the recovery and presence revisions have not changed
since the pass read it, that the store names no ID beyond
memory and reads cleanly, that no observation is pending, and that the release claims, receipt and create records name only IDs
confirmed absent with no create unresolved; only then does it write
external-guard-completion.json (read back; a failed write leaves the store open).
After it exists a handoff or create attempt is refused (StoreClosed), nothing on
disk counts as handed off, every valid stored ID (readable or not) is held, and a
guard that sees the closure, at start or mid-run, ends through one pass whose exit
must verify every ID it holds absent within main's release bound, or alert (I4). A
completion record never ends supervision while a create is unresolved.

Synchronization failures block an unsafe custody change, observation, conditional
DELETE or completion. They alert the operator and do not themselves authorize a
DELETE. A deadline DELETE of a retained ID still runs before any lock wait.
once=True runs a single pass (a watchdog).

usage: external_guard.py CONTROL_DIR [--once]   (CONTROL_DIR/lease/lease.json required)
"""
from __future__ import annotations

import contextlib
import contextvars
import json
import os
from pathlib import Path
import re
import sys
import threading
import time
import urllib.parse
import urllib.request
import uuid

from common import read, write
from lease_guard import (PODS_URL, AbsenceConfirmer, Provider, decision, list_all, pod_rate, validate,
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
AMBIGUOUS_QUIET_SECONDS = 1800    # unbroken clear censuses before the operator is alerted (never a resolution)
QUIET_GAP_SECONDS = 3 * POLL_SECONDS   # a longer gap between clear censuses starts the quiet window again
QUIET_NOTE = 'ambiguous-create-quiet.json'
QUIET_ALERT = 'ALERT-ambiguous-create.json'
QUIET_FUNDS = 'ambiguous-create-funds.json'   # the account read, written by its own thread when it returns
SYNC_ALERT = 'ALERT-sync-degraded.json'      # the store lock, closure probe or a presence file failed
KNOWN_DIR = 'lease/known-pods'
STORE_LOCK_SECONDS = 20           # a handoff or completion waiting longer for the store lock fails closed
CREATE_ROLES = ('receipt', 'adopted')   # roles that mark a retained ID as the create result (I3)
POD_ID = re.compile('[A-Za-z0-9]+')


def list_pods(key):
    """The full v2 Pod census as a list, walking every page (lease_guard.list_all).

    v2 wraps the list as {"pods": [...], "pagination": {...}} and pages it; a
    malformed page or an unfinished walk raises, so a partial census is never
    taken for a complete one.
    """
    require_boundary('Pod census')

    def fetch(cursor):
        url = PODS_URL + ('?' + urllib.parse.urlencode({'cursor': cursor}) if cursor else '')
        request = urllib.request.Request(url, headers={
            'Authorization': 'Bearer ' + key, 'User-Agent': 'Mozilla/5.0 g115-external-guard/1'})
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.load(response)
    return list_all(fetch, 'pods')


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


def get_evidence_complete(pod_id, value):
    return value is None or (isinstance(value, dict) and value.get('id') == pod_id
                             and isinstance(value.get('name'), str) and bool(value['name'].strip()))


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
        named = entry.get('name')
        if valid_id(pod_id) and not (isinstance(named, str) and named.strip()):
            malformed = True                # an unnamed or blank-named entry may be this lease's Pod: never absence
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
    """Readable evidence naming the create's Pod, even before custody is transferred."""
    if receipt_state(root)[0]:
        return True
    try:
        return returned_post_id(root) is not None or any(role == 'receipt' for role in known_pods(root, []).values())
    except Exception:  # noqa: BLE001 - unreadable store: no evidence
        return False


def returned_post_id(root):
    path = Path(root) / 'lease/create-returned-id.json'
    if not path.exists():
        return None
    value = read(path)
    pod_id = value.get('pod_id') if isinstance(value, dict) else None
    if not valid_id(pod_id):
        raise ValueError('malformed raw create ID record')
    return pod_id


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


class SyncError(OSError):
    """A local synchronization fault blocks unsafe transitions and never authorizes release by itself."""


class LockTimeout(SyncError):
    """The store lock was held past STORE_LOCK_SECONDS."""


class PresenceError(SyncError):
    """A presence revision could not be read or advanced."""


class LockError(SyncError):
    """The store lock could not be set up or acquired."""


class ClosureUnknown(SyncError):
    """The completion-record probe failed: the store is not known to be open."""


class StoreClosed(RuntimeError):
    """The guard completed: no supervisor reads the store any more, so a handoff to it is refused."""


class ReleaseClaimed(RuntimeError):
    """A durable release claim prevents transferring this Pod back to the guard."""


COMPLETION = 'external-guard-completion.json'


def lease_root(folder):
    """The control folder of a known-Pod folder (CONTROL/lease/known-pods)."""
    return Path(folder).parent.parent


def store_state(folder):
    """'closed' once the guard's completion record exists (readable or not), 'open' before, 'unknown'
    when the probe fails. Nothing is handed off to a store that is not known open, and an unknown state
    never discharges an obligation; everywhere else it counts as open (main's behavior), so it never
    shortens supervision, blocks a cleanup or an ALERT, or makes a guardian-owned Pod deletable."""
    try:
        return 'closed' if (lease_root(folder) / COMPLETION).exists() else 'open'
    except Exception:  # noqa: BLE001
        return 'unknown'


def store_closed(folder):
    return store_state(folder) == 'closed'


def presence_path(folder, pod_id):
    return Path(folder) / (pod_id + '.presence.json')   # never read as a known-Pod record (not a valid ID)


def read_presence(folder, pod_id):
    """How many presence handoffs this ID has had (0 before the first); raises when unreadable."""
    path = presence_path(folder, pod_id)
    if not path.exists():
        return 0
    value = read(path).get('revision')
    if not isinstance(value, int) or isinstance(value, bool) or value < 0:
        raise ValueError('presence revision malformed')
    return value


def release_claim_path(folder, pod_id):
    return Path(folder).parent / 'release-claims' / (pod_id + '.json')


def release_claims(folder):
    path = Path(folder).parent / 'release-claims'
    if not path.exists():
        return set()
    return {entry.stem for entry in path.iterdir() if entry.suffix == '.json' and valid_id(entry.stem)}


def observation_dir(folder):
    return Path(folder).parent / 'observation-intents'


def observation_intents(folder):
    path = observation_dir(folder)
    if not path.exists():
        return []
    return sorted(entry for entry in path.iterdir() if entry.suffix == '.json')


def recovery_path(folder):
    return Path(folder).parent / 'observation-recovery.json'


def recovery_revision(folder):
    path = recovery_path(folder)
    if not path.exists():
        return 0
    revision = read(path).get('revision')
    if type(revision) is not int or revision < 1:
        raise ValueError('observation recovery revision malformed')
    return revision


class StoreLock:
    """An exclusive OS lock shared by every handoff, every create attempt and completion, so each either
    lands before completion's final check sees it or meets the closure (I4). The OS releases it when the
    process dies; a wait longer than STORE_LOCK_SECONDS raises (the caller fails closed)."""

    def __init__(self, folder, wait=None):
        self.path = Path(folder).parent / (Path(folder).name + '.lock')
        self.handle = None
        self.wait = wait                # seconds to wait for the lock; None is STORE_LOCK_SECONDS

    def _try(self):
        if os.name == 'nt':
            import msvcrt
            self.handle.seek(0)
            msvcrt.locking(self.handle.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(self.handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    def __enter__(self):
        """Every failure to set up or acquire the lock is a SyncError (never a store fault)."""
        try:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            self.handle = open(self.path, 'a+b')
            deadline = time.monotonic() + (STORE_LOCK_SECONDS if self.wait is None else self.wait)
        except Exception as e:  # noqa: BLE001
            raise LockError('store lock setup: ' + type(e).__name__) from e
        while True:
            try:
                self._try()
                return self
            except Exception:  # noqa: BLE001 - held elsewhere (or not yet acquirable): retried to the bound
                pass
            try:
                expired = time.monotonic() >= deadline
                if not expired:
                    time.sleep(0.05)
            except Exception:  # noqa: BLE001
                expired = True
            if expired:
                self._close()
                raise LockTimeout('store lock held past %s s' % STORE_LOCK_SECONDS)

    def _release(self):
        if os.name == 'nt':
            import msvcrt
            self.handle.seek(0)
            msvcrt.locking(self.handle.fileno(), msvcrt.LK_UNLCK, 1)

    def _close(self):
        try:
            self.handle.close()
        except Exception:  # noqa: BLE001 - the OS releases the lock with the handle or the process
            pass

    def __exit__(self, *exc):
        """Best effort: closing the handle releases the lock, so a teardown failure never replaces the
        body's result or exception."""
        try:
            self._release()
        except Exception:  # noqa: BLE001
            pass
        self._close()
        return False


def degraded(folder, what, error=None):
    """Report a synchronization fault without masking the caller's own result."""
    try:
        write(lease_root(folder) / SYNC_ALERT, {'what': what, 'epoch': time.time(),
              'error_type': type(error).__name__ if error is not None else None}, replace=True)
    except Exception:  # noqa: BLE001
        pass


@contextlib.contextmanager
def locked_store(folder, what, wait=None):
    """A real lock is required for ownership transitions and closure."""
    lock = StoreLock(folder, wait)
    try:
        lock.__enter__()
    except SyncError as error:
        degraded(folder, what + ': store lock', error)
        raise
    try:
        state = store_state(folder)
        if state == 'unknown':
            degraded(folder, what + ': closure probe')
            raise ClosureUnknown(what + ': closure probe failed')
        yield state
    finally:
        lock.__exit__(None, None, None)


def write_presence(folder, pod_id, epoch):
    try:
        revision = read_presence(folder, pod_id) + 1
        write(presence_path(folder, pod_id), {'revision': revision, 'epoch': epoch}, replace=True)
        if read_presence(folder, pod_id) != revision:
            raise ValueError('presence revision did not read back')
    except Exception as error:  # noqa: BLE001 - a failed revision keeps the observation open
        degraded(folder, 'presence revision of ' + pod_id, error)
        raise PresenceError('presence revision of ' + pod_id) from error


def orphan_observations(folder):
    """Snapshot intents whose observers are gone. Never wait on a live observer under the store lock."""
    abandoned = {}
    with locked_store(folder, 'inspect observation intents', wait=0):
        for path in observation_intents(folder):
            try:
                with StoreLock(path.with_suffix(''), wait=0):
                    value = read(path)
                    if value.get('kind') == 'census':
                        abandoned[path] = None
                    elif value.get('kind') == 'get' and valid_id(value.get('pod_id')):
                        abandoned[path] = value['pod_id']
                    else:
                        raise ValueError('observation intent malformed')
            except LockTimeout:
                continue
    return abandoned


def finish_observations(folder, paths, wait=0, *, recovered=False):
    with locked_store(folder, 'settle observation intents', wait=wait):
        revision = None
        if recovered:
            # Other guards may still hold absence evidence from before these
            # intents. Invalidate it durably before removing the barrier.
            revision = recovery_revision(folder) + 1
            write(recovery_path(folder), {'revision': revision}, replace=True)
            if recovery_revision(folder) != revision:
                raise ValueError('observation recovery revision did not read back')
        for path in paths:
            path.unlink(missing_ok=True)
        return revision


def remove_observation_lock(path):
    # Intent tokens are never reused. A missing JSON record no longer names this lock.
    try:
        path.with_suffix('.lock').unlink(missing_ok=True)
    except OSError:
        pass


def handoff_in(folder, pod_id, role, epoch, presence=True, lock_wait=None):
    """Publish custody and any positive presence under the lock. A release claim instead retains
    custody for release, but its observer still advances the presence revision."""
    with locked_store(folder, 'handoff of ' + pod_id, lock_wait) as state:
        if state == 'closed':
            raise StoreClosed('the guard completed; the caller keeps ' + pod_id)
        if release_claim_path(folder, pod_id).exists():
            if presence:
                write_presence(folder, pod_id, epoch)
            raise ReleaseClaimed('release already claimed for ' + pod_id)
        remember_in(folder, pod_id, role, epoch)
        value = read(Path(folder) / (pod_id + '.json'))
        if not isinstance(value, dict) or value.get('id') != pod_id:
            raise ValueError('known-Pod record does not read back')
        if role in CREATE_ROLES and value.get('role') not in CREATE_ROLES:
            raise ValueError('known-Pod record lost its create role')
        if presence:
            write_presence(folder, pod_id, epoch)
    return True


def complete_store(root, memory, basis, settled, errors, record):
    """Completion's state transition, under the store lock: every ID's presence revision is still the
    one in `basis` (no presence handoff since this pass read them), the store names no ID beyond `memory`
    and reads cleanly, and `settled()` (receipt, create and ambiguity records re-read now) holds; only
    then is the completion record, which closes the store, written and read back. True when it was."""
    folder = Path(root) / KNOWN_DIR
    try:
        with locked_store(folder, 'completion', wait=0) as state:
            if state == 'closed':              # another guard completed first: its record stands
                return False
            claims = release_claims(folder)
            if claims != set(record.get('release_claims', ())) or claims - set(memory) or observation_intents(folder):
                return False
            if recovery_revision(folder) != record.get('observation_recovery_revision', 0):
                return False
            for pod_id in memory:
                try:
                    revision = read_presence(folder, pod_id)
                except Exception as e:  # noqa: BLE001 - unreadable revision blocks completion
                    degraded(folder, 'completion: presence revision of ' + pod_id, e)
                    return False
                if revision != basis.get(pod_id, 0):
                    return False
            late = set(known_pods(root, errors)) - set(memory)
            if late or errors or not settled():
                return False
            write(Path(root) / COMPLETION, record, replace=True)
            return read(Path(root) / COMPLETION).get('provider_absent') is True
    except Exception as e:  # noqa: BLE001 - an unreadable store or a lock timeout never permits completion
        errors.append('complete:' + type(e).__name__)
        return False


def closed_and_settled(root):
    """The store is closed and no create is unresolved: supervision may end through the bounded exit. A
    stale or forged completion never ends supervision of a create that may still allocate a Pod."""
    try:
        return store_closed(Path(root) / KNOWN_DIR) and not create_unresolved(root) and not ambiguous_unresolved(root)
    except Exception:  # noqa: BLE001 - an unreadable record means unresolved: supervision goes on
        return False


def record_create_attempt(lease_dir, value, *, funding_observed=None, funding_max_age=120, clock=time.time):
    """A creator's attempt record, written under the store lock; refused once the guard has completed,
    so a create never starts after completion's final check (I4)."""
    folder = Path(lease_dir) / 'known-pods'
    with locked_store(folder, 'create attempt') as state:
        if state == 'closed':
            raise StoreClosed('the guard completed; no create after completion')
        if funding_observed is not None and not 0 <= clock() - funding_observed <= funding_max_age:
            raise ValueError(f'funding observation older than {funding_max_age} s at the create attempt')
        write(Path(lease_dir) / 'create-attempt.json', value)


def record_created_pod(folder, value, writer=write):
    pod_id = value['id']
    if not valid_id(pod_id):
        raise ValueError('invalid Pod ID in create receipt')
    with locked_store(folder, 'create receipt', wait=0) as state:
        if state == 'closed':
            raise StoreClosed('the guard completed; the creator keeps ' + pod_id)
        if release_claim_path(folder, pod_id).exists():
            raise ReleaseClaimed('release already claimed for ' + pod_id)
        writer(Path(folder).parent / 'created-pod.json', value, replace=True)


def remember(root, pod_id, role, epoch):
    handoff_in(Path(root) / KNOWN_DIR, pod_id, role, epoch, presence=False)


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
                 lease_deadline=0.0, min_release_seconds=120, lock_wait=0):
        self.folder = Path(folder); self.name = name; self.key = key
        # A holder of allocated Pods defaults to nonblocking store operations.
        # Conditional release retries within the existing deadline on contention.
        self.lock_wait = lock_wait
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
        self.presence_pending = set()   # IDs seen alive whose presence handoff has not landed: never durable
        self.errors = []            # retention failures since the caller last cleared it
        self.sync_errors = []       # SyncError failures since the caller last cleared it (never store faults)
        self._active = False

    def __enter__(self):
        self._active = True
        self.load_durable()
        return self

    def receipt_ids(self, *, strict=False):
        """IDs named by this lease's create receipt and ambiguity records, when readable and valid."""
        found = set()
        for name, field in (('created-pod.json', 'id'), ('create-ambiguous.json', 'pod_id'),
                            ('create-pod-failure.json', 'pod_id')):
            try:
                value = read(self.folder.parent / name)
                if isinstance(value, dict) and valid_id(value.get(field)):
                    found.add(value[field])
            except FileNotFoundError:
                pass
            except Exception:  # noqa: BLE001 - a strict observation may not rule out a known ID
                if strict:
                    raise
        return found

    def record_reads_back(self, pod_id):
        try:
            value = read(self.folder / (pod_id + '.json'))
            return isinstance(value, dict) and value.get('id') == pod_id
        except Exception:  # noqa: BLE001
            return False

    def load_durable(self):
        """What is already handed off is never an obligation: known-Pod records that read back and IDs
        named by readable receipts start resolved. A closed store hands nothing off: no supervisor remains."""
        if store_closed(self.folder):
            return
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
        readable receipt naming the ID. Destructive cleanup never touches such an ID. After the guard
        closed the store nothing is durable: this process keeps every ID it holds. An ID seen alive whose
        presence handoff has not landed is never durable, whatever older record names it."""
        if store_closed(self.folder) or pod_id in self.presence_pending or release_claim_path(self.folder, pod_id).exists():
            return False
        if pod_id in self.durable or self.record_reads_back(pod_id) or pod_id in self.receipt_ids():
            self.persisted.add(pod_id); self.unpersisted.discard(pod_id)
            return True
        return False

    def guardian_owns(self, pod_id):
        """An open store already names the ID (a record this process read or wrote back, its own record
        reading back now, or a readable receipt): the guardian supervises it, so this process never
        deletes it, even while a presence handoff of it is pending or the record is unreadable now."""
        return (not store_closed(self.folder) and not release_claim_path(self.folder, pod_id).exists()
                and (pod_id in self.durable or pod_id in self.persisted
                     or self.record_reads_back(pod_id) or pod_id in self.receipt_ids()))

    def claim_release(self, pod_id):
        """Choose release under the same lock as every competing custody write."""
        if not valid_id(pod_id):
            raise ValueError('invalid Pod ID for release')
        path = release_claim_path(self.folder, pod_id)
        with locked_store(self.folder, 'claim release of ' + pod_id, self.lock_wait) as state:
            if path.exists():
                return True
            if state == 'open' and (self.guardian_owns(pod_id) or (self.folder / (pod_id + '.json')).exists()):
                return False
            write(path, {'id': pod_id, 'epoch': self.clock()})
            return True

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
        """The exit's release bound, main's on every store: a recoverable outage is retried to the lease
        deadline (closed or not), so no Pod is left alive sooner than main would leave it."""
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
        handoff I/O, so an interrupt at any later point leaves it for the exit to resolve. A positive
        observation (presence) of an ID something already names (a refresh) is an obligation, whatever
        older record or receipt names it, until its own presence handoff lands; a brand-new ID is an
        obligation until any durable record names it. On a closed store an earlier handoff no longer
        covers anything."""
        seen = [p for p in pod_ids if valid_id(p)]
        if presence:
            # obligation first, as a refresh, before any disk read (the closure probe included): an
            # interrupt or a read failure during classification leaves every seen ID held (a named one is
            # reported, never deleted)
            self.presence_pending.update(seen); self.unpersisted.update(seen)
            self.verified_absent.difference_update(pod_ids)
        else:
            self.unpersisted.update(p for p in seen if p not in self.persisted and p not in self.durable)
        if store_closed(self.folder):
            self.persisted.difference_update(seen); self.durable.difference_update(seen)
            self.unpersisted.update(seen)
        if presence:
            receipts = self.receipt_ids()
            self.presence_pending.difference_update(
                p for p in seen if not (p in self.persisted or p in self.durable or self.record_reads_back(p)
                                        or p in receipts))

    def retain(self, pod_id, role, presence=True, *, raise_closed=False):
        """Register, then hand off; the obligation is cleared only after the durable readback."""
        if not valid_id(pod_id):
            return False
        self.register([pod_id], presence)
        if role in CREATE_ROLES and self.retained.get(pod_id) not in CREATE_ROLES:
            self.retained[pod_id] = role
        self.retained.setdefault(pod_id, role)
        try:
            handoff_in(self.folder, pod_id, self.retained[pod_id], self.clock(), presence=presence,
                       lock_wait=self.lock_wait)
        except StoreClosed:   # expected after completion, not a store fault: the obligation stays this process's
            if raise_closed:
                raise
            return False
        except ReleaseClaimed:
            # The durable release claim, and a new presence revision for a positive observation, exist.
            self.presence_pending.discard(pod_id)
            self.unpersisted.add(pod_id)
            return True
        except SyncError as e:   # the obligation stays and blocks completion; not a store fault
            self.sync_errors.append('retain:' + pod_id + ':' + type(e).__name__)
            return False
        except Exception as e:  # noqa: BLE001 - the obligation stays; the exit resolves it
            self.errors.append('retain:' + pod_id + ':' + type(e).__name__)
            return False
        self.persisted.add(pod_id)
        if presence:
            self.presence_pending.discard(pod_id)
        if pod_id not in self.presence_pending:
            self.unpersisted.discard(pod_id)
        return True

    def mark_persisted(self, pod_id, hold=True):
        """The ID was read from its own durable known-Pod record (a restart load). On a closed store it is
        held as an obligation, or with hold=False (a read-only check, which reports what it cannot
        verify) left unmarked."""
        if store_closed(self.folder):
            if hold:
                self.register([pod_id], presence=False)
            return
        self.persisted.add(pod_id)
        if pod_id not in self.presence_pending:   # an older record never discharges a failed presence handoff
            self.unpersisted.discard(pod_id)

    def mark_durable(self, pod_id, hold=True):
        """The ID is named by a readable record the guard rereads (create receipt or ambiguity record); on
        a closed store as mark_persisted."""
        if store_closed(self.folder):
            if hold:
                self.register([pod_id], presence=False)
            return
        self.durable.add(pod_id)
        if pod_id not in self.presence_pending:
            self.unpersisted.discard(pod_id)

    def retry(self):
        for pod_id in sorted(self.unpersisted):
            self.retain(pod_id, self.retained.get(pod_id, 'census'), presence=pod_id in self.presence_pending)

    @contextlib.contextmanager
    def observing(self, kind, pod_id=None):
        """Publish intent before provider I/O, with its liveness lock already held."""
        if store_closed(self.folder):
            yield None
            return
        path = observation_dir(self.folder) / (uuid.uuid4().hex + '.json')
        try:
            with StoreLock(path.with_suffix(''), wait=0):
                with locked_store(self.folder, 'start observation', self.lock_wait) as state:
                    if state == 'open':
                        try:
                            write(path, {'kind': kind, 'pod_id': pod_id, 'epoch': self.clock()})
                        except OSError as error:
                            degraded(self.folder, 'publish observation intent', error)
                            raise SyncError('publish observation intent') from error
                yield path if state == 'open' else None
        finally:
            try:
                if not path.exists():
                    remove_observation_lock(path)
            except OSError:
                pass

    def observed(self, path, complete):
        if path is not None and complete:
            try:
                finish_observations(self.folder, (path,), self.lock_wait)
            except OSError as error:
                degraded(self.folder, 'settle observation intent', error)
                self.sync_errors.append('observation:' + type(error).__name__)

    def known_observation_targets(self):
        """Existing custody makes a matching ID evidence of presence even under another name."""
        try:
            root = lease_root(self.folder)
            raw_post = returned_post_id(root)
            targets = (set(self.retained) | self.persisted | self.durable | self.unpersisted
                       | set(known_pods(root, [])) | release_claims(self.folder) | self.receipt_ids(strict=True))
            if raw_post is not None:
                targets.add(raw_post)
            return targets
        except Exception as error:  # noqa: BLE001 - leave the intent for a later complete observation
            degraded(self.folder, 'read observation targets', error)
            raise SyncError('read observation targets') from error

    def census(self):
        """(raw census, owned IDs, every listed ID, malformed); each owned ID is retained first."""
        self._require_active()
        with self.observing('census') as intent:
            raw = self._raw(self._pods, self.key)
            owned, listed, malformed = census_entries(raw, self.name)
            self.register(owned)        # every owned ID becomes an obligation before any handoff
            other_names = listed - set(owned)
            renamed = sorted(other_names & self.known_observation_targets()) if intent is not None and other_names else []
            if renamed:
                self.register(renamed)
            retained = [self.retain(pod_id, 'census') for pod_id in owned + renamed]
            self.observed(intent, not malformed and all(retained))
        return raw, owned, listed, malformed

    def get(self, pod_id):
        self._require_active()
        with self.observing('get', pod_id) as intent:
            got = self._raw(lambda: self._provider(self.key, pod_id).call('GET'))
            complete = get_evidence_complete(pod_id, got)
            if isinstance(got, dict) and valid_id(got.get('id')):
                seen_id = got['id']
                renamed = intent is not None and seen_id == pod_id and got.get('name') != self.name
                if got.get('name') == self.name or (renamed and seen_id in self.known_observation_targets()):
                    self.register([seen_id])
                    complete = self.retain(seen_id, 'census') and complete
            self.observed(intent, complete)
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
            pod_id = returned['id']
            self.unpersisted.add(pod_id)
            self.verified_absent.discard(pod_id)
            # The locked pre-POST attempt already prevents completion. Record
            # the ID before handoff, including a response with an unexpected
            # name that an exact-name census could not recover. This is not custody.
            try:
                write(self.folder.parent / 'create-returned-id.json', {'pod_id': pod_id})
            except OSError as error:
                degraded(self.folder, 'record raw create ID', error)
            self.retain(pod_id, 'receipt')
        return returned


def settle_refresh(client, pod_id):
    """Retry a guardian-owned refresh. False leaves the caller responsible for release."""
    try:
        retained = client.retain(pod_id, client.retained.get(pod_id, 'census'), presence=True, raise_closed=True)
    except StoreClosed:
        return False
    if retained:
        return client.guardian_owns(pod_id)
    # Only a still-open guardian may take the local obligation. The intent from the
    # provider call remains on disk and prevents completion until reconciled.
    try:
        with locked_store(client.folder, 'settle refresh of ' + pod_id, client.lock_wait) as state:
            if state == 'closed' or not client.guardian_owns(pod_id):
                return False
            degraded(client.folder, 'refresh of ' + pod_id + ' not published')
            client.presence_pending.discard(pod_id)
            client.unpersisted.discard(pod_id)
            return True
    except OSError:
        return False


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
            if client.durable_now(pod_id):
                # handed off since the pass began (for example by its creator while an earlier
                # target's provider call was in flight): the guardian's, never deleted here
                targets.discard(pod_id)
                continue
            if client.guardian_owns(pod_id) and settle_refresh(client, pod_id):
                targets.discard(pod_id)
                continue
            try:
                claimed = client.claim_release(pod_id)
            except OSError as error:
                degraded(client.folder, 'claim release of ' + pod_id, error)
                claimed = False
            if claimed:
                try:
                    client.delete(pod_id)
                except Exception:  # noqa: BLE001 - DELETE is retried, including lost responses
                    pass
            try:
                absent = client.get(pod_id) is None
            except Exception:  # noqa: BLE001 - retried
                absent = False
            seen = census_ok and pod_id not in listed and absent
            state[pod_id] = confirmers.setdefault(pod_id, AbsenceConfirmer(client.clock)).observe(seen)
        targets |= client.unpersisted - client.verified_absent
        if all(state.get(pod_id) for pod_id in targets):
            client.verified_absent |= targets
            return []
        if client.clock() >= deadline:
            client.verified_absent |= {pod_id for pod_id in targets if state.get(pod_id)}
            return sorted(pod_id for pod_id in targets if not state.get(pod_id))
        sleep(RELEASE_POLL_SECONDS)


def resolve_or_alert(client, deadline, alert_path, sleep=None, also=()):
    """Every ending path runs this (LeaseClient's exit does): IDs that are neither durable nor verified
    absent are released here (release_unhanded); if any remain, an ALERT record is written and
    RuntimeError('ALERT ...') raised. Returns the IDs this call released."""
    targets = {p for p in (set(also) | client.unpersisted) - client.verified_absent if not client.durable_now(p)}
    # A still-owned refresh can be settled without a provider call. Closure during
    # that retry leaves the ID in the release set.
    for pod_id in sorted(p for p in targets if p in client.presence_pending and client.guardian_owns(p)):
        if settle_refresh(client, pod_id):
            targets.discard(pod_id)
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


def live_funds(key):
    """Fresh account read (no Pod ID, so outside the Pod provider boundary), as prepare_lease.live_funds."""
    return Provider(key, 'funding').funds()


def run(root, key, pods=None, provider=None, clock=time.time, sleep=time.sleep,
        once=False, preserve=None, marker_probe=None, funds=None, monotonic=time.monotonic):
    root = Path(root); lease = read(root / 'lease/lease.json'); validate(lease)
    once = once or closed_and_settled(root)   # after completion a guard runs one releasing pass
    # Leaving this block (completion, a --once pass, any exception) resolves every ID that is neither
    # durable nor verified absent, or fails loudly (I4).
    with LeaseClient(root / KNOWN_DIR, lease['name'], key, provider, pods, clock, sleep,
                     root / 'ALERT-pod-not-released.json', min_release_seconds=ONCE_RELEASE_SECONDS,
                     lock_wait=0) as client:
        return _guard(root, lease, client, clock, sleep, once, preserve, marker_probe, funds, monotonic)


def _guard(root, lease, client, clock, sleep, once, preserve, marker_probe, funds=None, monotonic=time.monotonic):
    highest = lease['rate_ceiling_usd_hour']; preserved = False; deletes = 0; last_probe = None
    quiet_since = last_clear = None; quiet_censuses = 0  # diagnostics only
    quiet_noted = quiet_funds_started = False     # alert published; account read started (once per window)
    basis = {}                    # ID -> the presence revision this process's confirmation of it rests on
    census_absence = AbsenceConfirmer(clock)
    id_absence = {}
    memory = client.retained      # known ID -> role; nothing leaves it (I4)
    confirmed = set()             # IDs confirmed absent by evidence seen in this process (I5)
    claims = set()                # release authority, never persisted absence
    recovered_revision = 0

    def note(name, value):
        """True when the record was written; a failed write never blocks a DELETE."""
        try:
            write(root / name, value, replace=True)
            return True
        except Exception:  # noqa: BLE001
            return False

    def account_read(read_funds, requested):
        """The quiet window's account read, off the supervision loop; it only ever writes QUIET_FUNDS."""
        try:
            account = read_funds()
        except Exception as e:  # noqa: BLE001 - diagnostic only
            account = {'error_type': type(e).__name__}
        note(QUIET_FUNDS, {'requested_epoch': requested, 'account_now': account})

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
        del client.errors[:]; del client.sync_errors[:]
        sync_errors = []              # this pass's SyncError failures: completion blockers, never release reasons
        now = clock()
        # (A) reasons from the clock and local files only; persisted results never end the guard (I5)
        reason = None
        if now >= lease['deadline_epoch'] - lease['recovery_seconds']:
            reason = 'deadline'
        elif (root / 'release-authorized.json').exists():
            reason = 'controller_release'
        # After completion closed the store nothing is handed off: this single pass (run() makes it one)
        # deletes what it sees and must verify every ID it holds absent at its exit, or alert (I4)
        closed = store_closed(root / KNOWN_DIR)
        if closed:
            reason = reason or 'store_closed'
        # local records: the durable store (restart load), the receipt, fallback IDs of create records
        try:
            for pod_id, role in known_pods(root, store_errors).items():
                if role in CREATE_ROLES and memory.get(pod_id) not in CREATE_ROLES:
                    memory[pod_id] = role
                memory.setdefault(pod_id, role)
                if role is not None or closed:
                    # its own record read back (an unreadable one stays open); on a closed store every
                    # stored ID is held, readable or not
                    client.mark_persisted(pod_id)
        except Exception as e:  # noqa: BLE001
            store_errors.append('store:' + type(e).__name__)
        recorded, receipt_error = receipt_state(root)
        if receipt_error:
            store_errors.append(receipt_error)
        try:
            raw_post = returned_post_id(root)
        except Exception as error:  # noqa: BLE001 - a damaged raw ID must not close the store
            store_errors.append('raw create ID:' + type(error).__name__)
            raw_post = None
        fallbacks = []                              # IDs the create records name, read once this pass
        for name in ('lease/create-ambiguous.json', 'lease/create-pod-failure.json'):
            try:
                if (root / name).exists():
                    fallback = read(root / name).get('pod_id')
                    if valid_id(fallback):
                        fallbacks.append(fallback)
            except Exception as e:  # noqa: BLE001
                store_errors.append(name + ':' + type(e).__name__)
        # every ID this pass knows is retained in memory first (no I/O), so a failed DELETE below never
        # loses one (I4)
        for pod_id in ([recorded] if recorded else []) + fallbacks:
            memory.setdefault(pod_id, 'receipt')
        if raw_post is not None:
            memory[raw_post] = 'receipt'
        try:
            current_claims = release_claims(client.folder)
            if current_claims != claims:
                for pod_id in current_claims - claims:
                    confirmed.discard(pod_id); id_absence.pop(pod_id, None)
                census_absence = AbsenceConfirmer(clock)
            claims = current_claims
        except OSError as error:
            sync_errors.append('release claims:' + type(error).__name__)
        for pod_id in claims:
            memory.setdefault(pod_id, 'release')
        # (B) after the deadline every known ID is deleted first, before any synchronized handoff and
        # before anything can fail
        deleted_first = set()
        if reason == 'deadline':
            for pod_id in sorted(p for p in memory if p not in confirmed):
                delete(pod_id, 'deadline_first'); deleted_first.add(pod_id)
        if recorded:
            client.mark_durable(recorded)
            client.retain(recorded, 'receipt', presence=False)
        if raw_post and raw_post not in claims and not client.guardian_owns(raw_post):
            client.retain(raw_post, 'receipt', presence=False)
        for fallback in fallbacks:
            client.mark_durable(fallback)
            client.retain(fallback, 'receipt', presence=False)
        client.retry()                              # every unpersisted ID, every pass (I2, I4)
        try:
            orphaned = orphan_observations(client.folder)
            observed_revision = recovery_revision(client.folder)
        except Exception as error:  # noqa: BLE001 - an unreadable intent never permits completion
            sync_errors.append('observation intents:' + type(error).__name__)
            orphaned = {}
            observed_revision = recovered_revision
        if orphaned or observed_revision != recovered_revision:
            # A dead observer may have seen presence with no revision write. No
            # absence witness from before this reconciliation can support closure.
            confirmed.clear(); id_absence.clear()
            census_absence = AbsenceConfirmer(clock)
        recovered_revision = observed_revision
        # A presence handoff of an ID since this process last read it (any process saw that Pod alive)
        # voids its absence confirmation and the census confirmation (I5)
        for pod_id in sorted(memory):
            try:
                revision = read_presence(root / KNOWN_DIR, pod_id)
            except Exception as e:  # noqa: BLE001 - main has no presence files: keep the last revision read
                degraded(root / KNOWN_DIR, 'presence revision of ' + pod_id, e); revision = basis.get(pod_id, 0)
            if basis.get(pod_id, 0) != revision:
                confirmed.discard(pod_id); id_absence.pop(pod_id, None); census_absence = AbsenceConfirmer(clock)
            basis[pod_id] = revision

        def primary_id():
            """I3: the readable receipt, else a retained create-result record."""
            if recorded and recorded not in claims:
                return recorded
            for role in CREATE_ROLES:
                candidates = sorted(p for p, r in memory.items() if r == role and p not in claims)
                if candidates:
                    return candidates[0]
            return None
        # (B) and any ID this pass's records added since
        if reason == 'deadline':
            for pod_id in sorted(p for p in memory if p not in confirmed and p not in deleted_first):
                delete(pod_id, 'deadline_first')
        # (C1) census through the client (each owned ID retained first); every listed ID, whatever
        # its name, is presence; malformed provider data is a guard failure, never absence
        before = set(memory); open_before = [p for p in memory if p not in confirmed]
        prior_errors = len(client.errors); prior_sync_errors = len(client.sync_errors)
        census_ok = False; malformed = False; owned = []; census_ids = set()
        try:
            _, owned, census_ids, malformed = client.census()
            census_ok = True
        except SyncError as error:
            sync_errors.append('census:' + type(error).__name__)
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
        observed = {}; pod = None; all_gets_complete = True
        get_ids = set(memory) | {pod_id for pod_id in orphaned.values() if pod_id is not None}
        for pod_id in sorted(get_ids):
            try:
                got = client.get(pod_id)
            except SyncError as error:
                sync_errors.append('get:' + type(error).__name__)
                all_gets_complete = False
                continue
            except Exception as e:  # noqa: BLE001
                failures.append('get:' + type(e).__name__); observed[pod_id] = None
                all_gets_complete = False
                continue
            all_gets_complete = all_gets_complete and get_evidence_complete(pod_id, got)
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
        sync_errors.extend(client.sync_errors)
        fresh_handoffs = len(client.errors) == prior_errors and len(client.sync_errors) == prior_sync_errors
        if orphaned and census_ok and not malformed and all_gets_complete and fresh_handoffs:
            try:
                recovered_revision = finish_observations(client.folder, orphaned, recovered=True)
                for path in orphaned:
                    remove_observation_lock(path)
            except Exception as error:  # noqa: BLE001 - recovery failure leaves closure blocked
                sync_errors.append('recover observation intents:' + type(error).__name__)
        if sync_errors:   # diagnostic; completion stays blocked
            note('external-sync-error.json', {'epoch': clock(), 'errors': list(sync_errors)})
        open_known = sorted(p for p in memory if p not in confirmed)
        if store_errors:
            failures.append('store:' + ','.join(store_errors))
        if failures and open_known:
            reason = reason or 'guard_failure'
        # Quiet after a create that named no Pod is evidence for the operator, never a resolution
        try:
            tick = monotonic()
        except Exception as e:  # noqa: BLE001 - diagnostic timing only: the window starts again, supervision goes on
            tick = None
            note('external-quiet-clock-error.json', {'epoch': clock(), 'error_type': type(e).__name__})
        # every retained ID (a delayed Pod already deleted, say) must hold a fresh absence confirmation
        settled_memory = all(p in confirmed for p in memory)
        if tick is not None and census_ok and not malformed and not listed and not failures and settled_memory \
                and create_unresolved(root):
            if quiet_since is None or tick - last_clear > QUIET_GAP_SECONDS:
                quiet_since, quiet_censuses, quiet_noted, quiet_funds_started = tick, 0, False, False
            quiet_censuses += 1; last_clear = tick
        else:
            quiet_since = last_clear = None; quiet_censuses = 0; quiet_noted = quiet_funds_started = False
        if quiet_since is not None and not quiet_noted and tick - quiet_since >= AMBIGUOUS_QUIET_SECONDS:
            evidence = {'exact_name': lease['name'], 'epoch': clock(), 'quiet_seconds': tick - quiet_since,
                        'clear_censuses': quiet_censuses, 'retained_ids': sorted(memory),
                        'account_now': QUIET_FUNDS if funds is not None else None,
                        'account_before_create': None, 'resolution': 'operator only (lease/' +
                        OPERATOR_RESOLUTION.split('/', 1)[1] + ')'}
            try:
                evidence['account_before_create'] = read(root / 'lease/funding-at-create.json')
            except Exception:  # noqa: BLE001 - absent on older leases
                pass
            # published first and only when both writes land; a failed write is retried next pass
            quiet_noted = note(QUIET_NOTE, evidence) and note(QUIET_ALERT, {
                'reason': 'ambiguous create quiet; operator resolution needed', 'epoch': clock(),
                'evidence': QUIET_NOTE})
            if funds is not None and not quiet_funds_started:
                # once per window, on its own thread: a slow or hung account read never holds a pass
                quiet_funds_started = True
                try:
                    threading.Thread(target=account_read, args=(funds, clock()), daemon=True).start()
                except Exception as e:  # noqa: BLE001 - diagnostic only; supervision goes on
                    note(QUIET_FUNDS, {'requested_epoch': clock(), 'error_type': type(e).__name__})
        ambiguous = primary is None and ambiguous_unresolved(root)
        if ambiguous:
            reason = reason or 'ambiguous_create'
        # I5: per-ID absence from this process's evidence only
        for pod_id in open_known:
            seen_absent = census_ok and not malformed and pod_id not in census_ids and observed.get(pod_id) == 'absent'
            if id_absence.setdefault(pod_id, AbsenceConfirmer(clock)).observe(seen_absent):
                confirmed.add(pod_id)
                if pod_id in claims:
                    client.unpersisted.discard(pod_id); client.presence_pending.discard(pod_id)
                elif pod_id in client.presence_pending and client.guardian_owns(pod_id):
                    # its record reads back and this process's own evidence supersedes the refresh it
                    # could not publish (I2 still holds: the record itself is durable)
                    client.presence_pending.discard(pod_id); client.unpersisted.discard(pod_id)
        open_known = sorted(p for p in memory if p not in confirmed)
        census_clear = census_ok and not malformed and not listed
        census_done = census_absence.observe(census_clear)
        controller_done = not memory and (root / 'completion.json').exists() and \
            now - lease['created_epoch'] > CONTROLLER_DONE_SECONDS
        may_finish = (not open_known if memory else bool(reason or controller_done)) \
            and not store_errors and not sync_errors and not client.unpersisted
        unresolved = create_unresolved(root)
        def settled():
            """Re-read under the store lock: every ID a receipt or create record names is one this process
            confirmed absent, and no create is unresolved."""
            named, error = receipt_state(root)
            if error or (named and named not in memory):
                return False
            if create_unresolved(root) or ambiguous_unresolved(root):
                return False
            # A raw ID can arrive without the store lock, after the pre-POST
            # attempt. Check it after any create-evidence reads above.
            raw = returned_post_id(root)
            if raw and raw not in memory:
                return False
            for name in ('lease/create-ambiguous.json', 'lease/create-pod-failure.json'):
                if (root / name).exists():
                    fallback = read(root / name).get('pod_id')
                    if valid_id(fallback) and fallback not in memory:
                        return False
            return not any(p not in confirmed for p in memory)
        # Completion is the store's closure, written under the store lock after a final check: a concurrent
        # handoff or create attempt either lands before it (and blocks completion) or meets the closure and
        # stays its caller's (I4). A failed write leaves the store open; the next pass tries again.
        if not closed and census_done and may_finish and not unresolved and not ambiguous and complete_store(
                root, memory, basis, settled, store_errors, {
                    'exact_name': lease['name'], 'pod_id': primary, 'known_pods': sorted(memory),
                    'provider_absent': True, 'absent_confirmations': 2, 'reason': reason, 'deletes': deletes,
                    'release_claims': sorted(claims), 'observation_recovery_revision': recovered_revision,
                    'presence_revisions': dict(sorted(basis.items())), 'epoch': clock()}):
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
            if pod_id in claims or primary is not None or reason:
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
            delete(target, reason or ('claimed_release' if target in claims else 'duplicate_pod'))
        if reason and clock() >= lease['deadline_epoch'] and (targets or not absent_now):
            note('ALERT-pod-not-released.json', {'targets': targets, 'epoch': clock(), 'reason': reason})
        if once or closed_and_settled(root):
            # I4: the LeaseClient exit resolves any ID that is neither durable nor verified absent; a
            # closure seen mid-run (another guard completed) ends this one through the same bounded exit,
            # holding every retained ID this process has not itself confirmed absent
            if store_closed(root / KNOWN_DIR):
                client.verified_absent |= confirmed
                client.register(sorted(p for p in memory if p not in confirmed), presence=False)
            client.retry()
            return
        sleep(POLL_SECONDS)


if __name__ == '__main__':
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != '--once'):
        raise SystemExit(__doc__)
    try:
        key = api_key()
        run(sys.argv[1], key, once=len(sys.argv) == 3, funds=lambda: live_funds(key))
    except RuntimeError as error:
        raise SystemExit(str(error))
