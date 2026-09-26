"""Single-owner storage accounting for retained, compressed E-drive audit copies.

The owner reserves each writable tree's maximum footprint before dispatch and
keeps that reservation until the tree is sealed and measured. Concurrent tasks
share this object. Failed trees are retained and stop further admission. This
is a software accounting guard, not an OS quota or a D-drive allocation guard.
"""
import ctypes
from ctypes import wintypes as W
import json
import os
from pathlib import Path
import shutil
import threading
import time

GIB = 1024**3


def require(ok, message):
    if not ok:
        raise ValueError(message)


def charge(stored_bytes, files):
    """File allocation rounded by caller, plus 15% and per-file metadata margin."""
    require(type(stored_bytes) is int and stored_bytes >= 0
            and type(files) is int and files >= 0, 'Invalid measured storage')
    return (stored_bytes * 115 + 99)//100 + files*4096


class Ledger:
    """Pure accounting state. Locking, durable recording and measurements outside."""
    def __init__(self, cap, control_allowance):
        require(type(cap) is int and type(control_allowance) is int
                and 0 < control_allowance < cap <= 60_000_000_000,
                'Explicit positive cap at most 60GB required')
        self.cap = cap
        self.committed = control_allowance
        self.pending = {}
        self.sealed = {}
        self.stopped = False

    def reserve(self, name, maximum, free_bytes, reserve_bytes):
        require(not self.stopped, 'Storage owner stopped')
        require(name not in self.pending and name not in self.sealed, 'Tree already admitted')
        require(type(maximum) is int and maximum > 0, 'Positive charged write bound required')
        outstanding = sum(self.pending.values()) + maximum
        require(self.committed + outstanding <= self.cap, 'Aggregate audit cap unavailable')
        require(free_bytes - outstanding >= reserve_bytes, 'Disk reserve plus pending writes unavailable')
        self.pending[name] = maximum

    def seal(self, name, measured_charge, failed=False):
        require(name in self.pending, 'Tree was not reserved')
        require(type(measured_charge) is int and measured_charge >= 0, 'Invalid measured charge')
        maximum = self.pending.pop(name)
        self.committed += measured_charge
        self.sealed[name] = {'charged_bytes': measured_charge, 'reserved_bytes': maximum,
                             'failed': failed}
        if failed or measured_charge > maximum or self.committed+sum(self.pending.values()) > self.cap:
            self.stopped = True
        require(measured_charge <= maximum and self.committed+sum(self.pending.values()) <= self.cap,
                'Measured tree exceeded reservation; admission stopped')


def physical_tree(path, allow_atomic_rename=False):
    """Measure sealed files using Windows allocation data, rejecting linked trees."""
    require(os.name == 'nt', 'Windows allocation measurement required')
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCompressedFileSizeW.argtypes = [W.LPCWSTR, ctypes.POINTER(W.DWORD)]
    kernel.GetCompressedFileSizeW.restype = W.DWORD
    requested = Path(path)
    require(not requested.is_symlink() and not requested.is_junction(), 'Linked audit root refused')
    root = requested.resolve(strict=True)
    logical = stored = count = 0
    for item in root.rglob('*'):
        require(not item.is_symlink() and not item.is_junction(), 'Linked audit paths refused')
        require(item.resolve().is_relative_to(root), 'Audit path escaped owned tree')
        if not item.is_file():
            continue
        try:
            high = W.DWORD()
            ctypes.set_last_error(0)
            low = kernel.GetCompressedFileSizeW(str(item), ctypes.byref(high))
            if low == 0xffffffff and ctypes.get_last_error():
                raise ctypes.WinError(ctypes.get_last_error())
            data_bytes = (high.value << 32) | low
            logical_size = item.stat().st_size
        except FileNotFoundError:
            if allow_atomic_rename:
                continue  # Publication can rename an owned .partial between reads.
            raise
        stored += ((data_bytes+4095)//4096)*4096
        logical += logical_size
        count += 1
    return {'logical_bytes': logical, 'rounded_stored_bytes': stored, 'files': count,
            'charged_bytes': charge(stored, count)}


class AuditStorage:
    """Fresh-root owner; journal survives failure, no resume or deletion API."""
    def __init__(self, root, cap, control_allowance=64*1024**2, drive='e:', retained_roots=()):
        requested = Path(root)
        self.root = requested.resolve()
        require(drive in ('e:', 'c:') and requested.is_absolute() and self.root.drive.lower() == drive
                and not self.root.exists() and self.root.parent.is_dir(), 'Fresh owned host-specific root required')
        self.ledger = Ledger(cap, control_allowance)
        self.retained = {}
        for path in retained_roots:
            retained = Path(path).resolve(strict=True)
            require(retained.drive.lower() == drive and retained.is_dir()
                    and not self.root.is_relative_to(retained) and not retained.is_relative_to(self.root)
                    and all(not retained.is_relative_to(p) and not p.is_relative_to(retained)
                            for p in self.retained), 'Retained trees must be disjoint on the owned drive')
            self.retained[retained] = physical_tree(retained)['charged_bytes']
        retained_charge = sum(self.retained.values())
        self.ledger.committed += retained_charge
        require(self.ledger.committed < cap, 'Retained trees exhaust job cap')
        self.reserve_bytes = 60*GIB
        require(shutil.disk_usage(self.root.parent).free >= self.reserve_bytes+cap-retained_charge,
                'Full cap plus disk reserve unavailable at admission')
        self.lock = threading.RLock()
        self.root.mkdir()
        (self.root/'jobs').mkdir()
        self.control = self.root/'control'
        self.control.mkdir()
        self.control_allowance = control_allowance
        self.journal = (self.control/'storage.jsonl').open('x', encoding='utf-8')
        self._record('created', {'cap': cap, 'control_allowance': control_allowance,
                                'retained': {str(p):n for p,n in self.retained.items()}})

    def _record(self, event, detail):
        line = json.dumps({'event': event, 'detail': detail, 'committed': self.ledger.committed,
                           'pending': self.ledger.pending, 'stopped': self.ledger.stopped},
                          separators=(',', ':'), allow_nan=False)+'\n'
        if self.journal.tell()+len(line.encode('utf-8')) >= self.control_allowance//2:
            self.ledger.stopped = True
            raise ValueError('Control journal allowance reached')
        try:
            self.journal.write(line)
            self.journal.flush()
            os.fsync(self.journal.fileno())
        except BaseException:
            self.ledger.stopped = True
            raise

    def reserve(self, name, maximum):
        with self.lock:
            require(name and len(name) <= 100 and all(c.isalnum() or c in '-_' for c in name),
                    'Single-component job name required')
            self.ledger.reserve(name, maximum, shutil.disk_usage(self.root).free, self.reserve_bytes)
            self._record('reserved', {'name': name, 'maximum': maximum})
            folder = self.root/'jobs'/name
            try:
                folder.mkdir()
            except BaseException:
                self.ledger.stopped = True
                self._record('stopped', {'reason': 'reserved tree creation failed', 'name': name})
                raise
            return folder

    def check(self):
        """Use during owned writes/children; consumer stops its children on failure."""
        began = time.monotonic()
        with self.lock:
            timings = {'lock_wait_seconds': time.monotonic()-began}
            phase = 'stopped_state'
            try:
                require(not self.ledger.stopped, 'Storage owner stopped')
                phase = 'disk_reserve'
                began = time.monotonic()
                require(shutil.disk_usage(self.root).free >= self.reserve_bytes,
                        'Disk reserve reached')
                timings['disk_seconds'] = time.monotonic()-began
                phase = 'retained_trees'
                began = time.monotonic()
                for path, original in self.retained.items():
                    require(physical_tree(path)['charged_bytes'] <= original,
                            'Sealed retained tree grew after admission')
                timings['retained_seconds'] = time.monotonic()-began
                phase = 'control_tree'
                began = time.monotonic()
                control = physical_tree(self.control)
                require(control['charged_bytes'] <= self.control_allowance, 'Control allowance exceeded')
                timings['control_seconds'] = time.monotonic()-began
                phase = 'pending_trees'
                began = time.monotonic()
                for name, bound in self.ledger.pending.items():
                    require(physical_tree(self.root/'jobs'/name, allow_atomic_rename=True)['charged_bytes'] <= bound,
                            'Active tree exceeded reserved write bound')
                timings['pending_seconds'] = time.monotonic()-began
                return timings
            except BaseException as error:
                first_failure = not self.ledger.stopped
                self.ledger.stopped = True
                if first_failure:
                    self._record('stopped', {'reason': 'active storage check failed',
                                            'phase': phase, 'error': repr(error)})
                raise

    def seal(self, name, failed=False):
        """Call only after all writers have exited and content hashes are verified."""
        with self.lock:
            require(name in self.ledger.pending, 'Tree was not reserved')
            try:
                measured = physical_tree(self.root/'jobs'/name)
                self.ledger.seal(name, measured['charged_bytes'], failed)
            except BaseException:
                self.ledger.stopped = True
                self._record('seal_failed', {'name': name})
                raise
            self._record('sealed', {'name': name, 'failed': failed, **measured})
            return measured

    def close(self):
        with self.lock:
            self.ledger.stopped = True
            self._record('closed', {'unfinished_trees': list(self.ledger.pending)})
            self.journal.close()
