#!/usr/bin/env python3
"""Scratch segment work: deterministic fixtures, operation counts and timings for chains.py's reaper (`reap`),
segment persistence (`persist`, `compress_config`) and driver-start recovery (`scratch_settle`).
mtg-kernel#601 (T7) milestone 1: measurement only; chains.py runs unchanged.

Every scenario calls chains.reap, persist, compress_config and scratch_settle as the in-box driver calls them, on
temporary-directory segments, with the scratch bind mount simulated as in test_w5_scratch.py (`chains.mounted` and
`chains.unmount` are replaced). Here the segment path is a symlink to its scratch directory while "mounted", so a
walk of the segment reads the scratch tree as it does through the mount on a node.

**Fixture** (`Segment`). A scratch segment as native_expanded_training_run_v1 publishes it: run.json first; per
iteration `iterations/<i:06d>/attempt-000000/` with collect-command.json, collect/episode-<j:04d>.json (one per
episode), collect/collection.json, update-input.json, update-command.json, update/checkpoint.json and
update/update.json, then `iterations/<i:06d>/complete.json` last; completion.json after the last iteration. While
iteration i is reaped, iteration i + 1 is in flight (its attempt, collection command and half its trajectories).
Member counts and bytes come from the profile; every byte derives from the seed.

**Scenarios.** For a segment of N updates (a scratch segment length is a multiple of 64 dividing 4096; C's
default is 256), one worker process per sample, the fixture built untimed in it:
- reap: after each iteration completes, a productive tick (`reap` deletes the bulk of the iteration before it)
  and an idle tick (nothing new) are measured. The driver's reaper ticks every REAP_EVERY = 0.5 s, so a C update
  of about half a second sees about one tick, and a longer update also sees idle ones;
- persist: then `persist` (its own reap, the copy with the regenerable records gzipped, an fsync per file and per
  staging directory, the marker, the swap under the index lock, the parent fsync, the scratch removal) and
  `compress_config` of the segment config, as `drive_locked` runs them after the trainer exits. The persisted
  segment's logical members (gzip members decompressed; the marker excluded) are checked against the fixture;
- settle (`--settle`): `scratch_settle` with nothing to do, after an interrupted swap, with an ended segment to
  persist late, and with an unfinished segment to reset.

**Counts** (`Ops`, full mode). Hooks on Python's os, io, shutil, glob and fcntl entry points count directory scans
(scandir, listdir) and the entries they return, stat and lstat calls, opens, unlinks, renames, mkdirs, rmdirs and
fsyncs (file or directory), with sendfile and gzip input bytes; reap ticks attribute each operation to the iteration
it touches (already reaped, newly reaped, newest, in flight, or the iterations listing), persist to the tree (the
segment path, the scratch directory, the staging copy, its marker, the chain directory, the index lock). Counts are
deterministic for one Python version and file system type (run twice per condition to show it). The hooks perturb
time, so timings come from separate runs with light hooks (fsync, lock, copy, gzip, rmtree and unlink timers only).

**Timings.** Rounds run every condition, the order reversed every other round; with two variants each condition
runs A-B-B-A. Every sample is kept. `--candidate-src` runs a second tools/w5 tree (B) beside --src (A, by default
this checkout's), e.g. a git worktree at the baseline commit; passing the same tree twice measures the run-to-run
spread (A/A). Scratch and work directories default to /dev/shm (tmpfs, as the scratch root on a node) and the
system temporary directory; caches stay warm (the fixture was just written, and tmpfs has no cold state).

  python3 tools/w5/tests/scratch_bench.py --out RECEIPT.json [--sizes 64,256] [--rounds 4] [--profile small]
      [--scratch-dir /dev/shm] [--work-dir DIR] [--src TOOLS_W5] [--candidate-src TOOLS_W5] [--settle]
      [--max-wall-seconds 3600] [--max-fixture-bytes 2000000000] [--seed 601]

Not measured: a real bind mount, `umount` (a process on a node; simulated here) and `os.path.ismount`, the trainer's
own publication and its concurrency with the reaper, and any node device: timings are this host's, and the receipt
names its file systems. Linux only (as scratch segments).
"""
import argparse
import builtins
import collections
import gzip
import hashlib
import io
import json
import os
import platform
import random
import re
import shutil
import stat
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCHEMA = 'w5-scratch-bench/v1'
WORKER = '_worker'

# Per-iteration records (kept by the reaper, copied by persist): path in the iteration -> bytes. Sizes are fixture
# parameters (the receipt states them), not measurements of a real run.
RECORDS = {'attempt-000000/collect-command.json': 8192, 'attempt-000000/collect/collection.json': 3072,
           'attempt-000000/update-input.json': 3072, 'attempt-000000/update-command.json': 2048,
           'attempt-000000/update/update.json': 2048}
COMPLETE_BYTES = 3072
PROFILES = {
    # Real names and member counts with small bulk: what the reaper walks does not depend on the bytes it deletes.
    'small': {'episodes': 10, 'trajectory_bytes': 4096, 'checkpoint_bytes': 65536, 'records': RECORDS,
              'complete_bytes': COMPLETE_BYTES, 'run_bytes': 32768, 'completion_bytes': 1024,
              'config_bytes_per_update': 4096},
    # A plain-JSON C update as qualified (tools/ex1/stagef.py BYTES_PER_UPDATE: trajectories up to 32 MB,
    # checkpoint 21 MB): persist copies the newest checkpoint.
    'c-plain': {'episodes': 10, 'trajectory_bytes': 3_200_000, 'checkpoint_bytes': 21_000_000, 'records': RECORDS,
                'complete_bytes': COMPLETE_BYTES, 'run_bytes': 32768, 'completion_bytes': 1024,
                'config_bytes_per_update': 4096},
}
ITERATION = re.compile(r'[\\/]iterations[\\/](\d{6})(?=[\\/]|$)')
REAP_OPS = ('scandir', 'entry', 'stat', 'lstat', 'unlink', 'open')
REAP_CATEGORIES = ('reaped', 'new', 'newest', 'in_flight', 'listing', 'other')
DETERMINISTIC = [('persist', f) for f in ('counts', 'bytes', 'fsyncs', 'members', 'logical_digest', 'physical_bytes',
                                          'returned_bytes', 'identical')] + \
    [('compress_config', f) for f in ('counts', 'in_bytes', 'out_bytes')]


# ---------------------------------------------------------------- operation counts

class _Scan:
    """A counting os.scandir iterator (iteration, context manager and close, as callers use it)."""

    def __init__(self, it, ops, path):
        self._it, self._ops, self._path = it, ops, path

    def __iter__(self):
        return self

    def __next__(self):
        entry = next(self._it)
        self._ops.add('entry', self._path)
        return entry

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self._it.close()

    def close(self):
        self._it.close()


class Ops:
    """Counts file system operations made through Python's os, io, shutil, glob and fcntl entry points while
    entered and active. `full`: every counted operation; otherwise only the timed ones (fsync, flock, copy2,
    copyfileobj/gzip, rmtree, unlink). `classify(path)` names a category; a count's key is `category:op`.
    `events`: also keep (op, category, path) in order. Time accrues to the outermost timed operation only."""

    def __init__(self, full=True, classify=None, events=False):
        self.full = full
        self.classify = classify or (lambda path: '')
        self.n = collections.Counter()
        self.ns = collections.Counter()
        self.bytes = collections.Counter()
        self.events = [] if events else None
        self.active = True
        self.fds = {}
        self._held = {}
        self._timing = 0
        self._saved = []

    # -- bookkeeping
    def path(self, p, dir_fd=None):
        if isinstance(p, int):
            return self.fds.get(p)
        if p is None:
            return None
        p = os.fsdecode(os.fspath(p))
        if dir_fd is not None and not os.path.isabs(p) and self.fds.get(dir_fd):
            p = os.path.join(self.fds[dir_fd], p)
        return p

    def add(self, op, path, k=1):
        if not self.active:
            return
        cat = self.classify(path) if path is not None else 'other'
        self.n[f'{cat}:{op}' if cat else op] += k
        if self.events is not None:
            self.events.append((op, cat, path))

    def total(self, op=None, category=None):
        out = 0
        for key, v in self.n.items():
            cat, _, name = key.rpartition(':')
            if (op is None or name == op) and (category is None or cat == category):
                out += v
        return out

    def _timed(self, name, fn, *args, **kwargs):
        if not self.active or self._timing:
            return fn(*args, **kwargs)
        self._timing += 1
        t = time.perf_counter_ns()
        try:
            return fn(*args, **kwargs)
        finally:
            self.ns[name] += time.perf_counter_ns() - t
            self._timing -= 1

    # -- hooks
    def _patch(self, owner, name, value):
        self._saved.append((owner, name, vars(owner)[name] if isinstance(owner, type) else getattr(owner, name)))
        setattr(owner, name, value)

    def __enter__(self):
        global _ACTIVE
        ops = self
        real = {n: getattr(os, n) for n in ('fsync', 'unlink', 'remove')}
        if self.full:
            real.update({n: getattr(os, n) for n in ('scandir', 'listdir', 'stat', 'lstat', 'open', 'rename',
                                                      'replace', 'mkdir', 'rmdir', 'sendfile', 'link', 'symlink',
                                                      'utime', 'chmod')})

        def fsync(fd):
            num = fd if isinstance(fd, int) else fd.fileno()
            op = 'fsync'
            if ops.full and ops.active:
                op = 'fsync_dir' if stat.S_ISDIR(os.fstat(num).st_mode) else 'fsync_file'
            ops.add(op, ops.fds.get(num))
            return ops._timed('fsync', real['fsync'], fd)

        def unlink_hook(name):
            def unlink(path, *, dir_fd=None):
                ops.add('unlink', ops.path(path, dir_fd))
                return ops._timed('unlink', real[name], path, dir_fd=dir_fd)
            return unlink

        self._patch(os, 'fsync', fsync)
        self._patch(os, 'unlink', unlink_hook('unlink'))
        self._patch(os, 'remove', unlink_hook('remove'))
        try:
            import fcntl
            flock_real = fcntl.flock

            def flock(fd, operation):
                num = fd if isinstance(fd, int) else fd.fileno()
                if operation & fcntl.LOCK_UN:
                    ops.add('funlock', ops.fds.get(num))
                    t0 = ops._held.pop(num, None)
                    if t0 is not None and ops.active:
                        ops.ns['lock_hold'] += time.perf_counter_ns() - t0
                    return flock_real(fd, operation)
                ops.add('flock', ops.fds.get(num))
                t = time.perf_counter_ns()
                r = flock_real(fd, operation)
                if ops.active:
                    ops.ns['lock_wait'] += time.perf_counter_ns() - t
                    ops._held[num] = time.perf_counter_ns()
                return r
            self._patch(fcntl, 'flock', flock)
        except ImportError:  # not Linux: nothing to lock here
            pass
        copy2_real, copyfileobj_real, rmtree_real = shutil.copy2, shutil.copyfileobj, shutil.rmtree

        def copy2(src, dst, **kwargs):
            ops.add('copy2', ops.path(src))
            return ops._timed('copy2', copy2_real, src, dst, **kwargs)

        def copyfileobj(fsrc, fdst, *args, **kwargs):
            name = 'gzip' if isinstance(fdst, gzip.GzipFile) else 'copyfileobj'
            try:
                start = fsrc.tell()
            except (AttributeError, OSError):
                start = None
            r = ops._timed(name, copyfileobj_real, fsrc, fdst, *args, **kwargs)
            if start is not None and ops.active:
                ops.bytes[f'{name}_in'] += fsrc.tell() - start
            return r

        def rmtree(path, *args, **kwargs):
            ops.add('rmtree', ops.path(path))
            return ops._timed('rmtree', rmtree_real, path, *args, **kwargs)

        self._patch(shutil, 'copy2', copy2)
        self._patch(shutil, 'copyfileobj', copyfileobj)
        self._patch(shutil, 'rmtree', rmtree)
        if self.full:
            self._enter_full(real)
        _ACTIVE = self
        return self

    def _enter_full(self, real):
        ops = self
        import glob

        def scandir(path='.'):
            it = real['scandir'](path)
            p = ops.path(path)
            ops.add('scandir', p)
            return _Scan(it, ops, p)

        def listdir(path='.'):
            out = real['listdir'](path)
            p = ops.path(path)
            ops.add('scandir', p)
            ops.add('entry', p, len(out))
            return out

        def stat_(path, *, dir_fd=None, follow_symlinks=True):
            ops.add('stat' if follow_symlinks else 'lstat', ops.path(path, dir_fd))
            return real['stat'](path, dir_fd=dir_fd, follow_symlinks=follow_symlinks)

        def lstat(path, *, dir_fd=None):
            ops.add('lstat', ops.path(path, dir_fd))
            return real['lstat'](path, dir_fd=dir_fd)

        def open_(file, mode='r', *args, **kwargs):
            p = ops.path(file)
            ops.add('open', p)
            f = open_real(file, mode, *args, **kwargs)
            try:
                ops.fds[f.fileno()] = p
            except (AttributeError, OSError, ValueError):
                pass
            return f

        def os_open(path, flags, mode=0o777, *, dir_fd=None):
            p = ops.path(path, dir_fd)
            ops.add('open', p)
            fd = real['open'](path, flags, mode, dir_fd=dir_fd)
            ops.fds[fd] = p
            return fd

        def two(name):
            def f(src, dst, *args, **kwargs):
                ops.add(name, ops.path(src, kwargs.get('src_dir_fd') or kwargs.get('dir_fd')))
                return real[name](src, dst, *args, **kwargs)
            return f

        def one(name, op):
            def f(path, *args, **kwargs):
                ops.add(op, ops.path(path, kwargs.get('dir_fd')))
                return real[name](path, *args, **kwargs)
            return f

        def sendfile(out_fd, in_fd, offset, count):
            n = real['sendfile'](out_fd, in_fd, offset, count)
            if ops.active:
                ops.bytes['sendfile'] += n
            return n

        open_real = builtins.open
        self._patch(os, 'scandir', scandir)
        self._patch(os, 'listdir', listdir)
        self._patch(os, 'stat', stat_)
        self._patch(os, 'lstat', lstat)
        self._patch(os, 'open', os_open)
        self._patch(builtins, 'open', open_)
        self._patch(io, 'open', open_)
        self._patch(os, 'sendfile', sendfile)
        for name in ('rename', 'replace', 'link', 'symlink'):
            self._patch(os, name, two(name))
        for name, op in (('mkdir', 'mkdir'), ('rmdir', 'rmdir'), ('utime', 'meta'), ('chmod', 'meta')):
            self._patch(os, name, one(name, op))
        for name in ('listxattr', 'getxattr', 'setxattr'):
            if hasattr(os, name):
                real[name] = getattr(os, name)
                self._patch(os, name, one(name, 'xattr'))
        # Python 3.13's pathlib globbing binds os.scandir and os.lstat at import (glob._StringGlobber).
        globber = getattr(glob, '_StringGlobber', None)
        if globber is not None and all(isinstance(vars(globber).get(k), staticmethod) for k in ('scandir', 'lstat')):
            self._patch(globber, 'scandir', staticmethod(scandir))
            self._patch(globber, 'lstat', staticmethod(lstat))

    def __exit__(self, *exc):
        global _ACTIVE
        for owner, name, value in reversed(self._saved):
            setattr(owner, name, value)
        self._saved = []
        _ACTIVE = None


_ACTIVE = None


class suspended:
    """Operations made inside (the simulation's own) are not counted or timed."""

    def __enter__(self):
        self.ops, self.was = _ACTIVE, getattr(_ACTIVE, 'active', None)
        if self.ops is not None:
            self.ops.active = False

    def __exit__(self, *exc):
        if self.ops is not None:
            self.ops.active = self.was


# ---------------------------------------------------------------- fixture

def doc(tag, size, fields=None):
    """Deterministic JSON of about `size` bytes: `fields`, then entries of ids, trajectory paths, seeds and sha256
    pins (the trainer's receipts and commands are of this kind)."""
    out = dict(fields or {})
    pad = out.setdefault('pad', [])
    n, k = len(json.dumps(out)), 0
    while n < size:
        h = hashlib.sha256(f'{tag}/{k}'.encode()).hexdigest()
        item = {'id': f'{tag}-j{k}', 'path': f'collect/episode-{k % 10000:04d}.json', 'sha256': h,
                'seed': int(h[:15], 16), 'seat': k % 2}
        pad.append(item)
        n += len(json.dumps(item)) + 2
        k += 1
    return (json.dumps(out, sort_keys=True) + '\n').encode()


class Segment:
    """A scratch segment of a chain `c1` under `work` (the root volume) with its scratch directory under `scratch`,
    named as chains.scratch_dir names it. `expected` maps each relative path persist must keep to the sha256 of its
    logical bytes."""

    def __init__(self, ch, work, scratch, profile, seed, index=0):
        self.ch, self.p, self.seed = ch, profile, seed
        self.root = Path(work) / 'root'
        self.cdir = self.root / 'chains' / 'c1'
        self.native = self.cdir / f'seg-{index:05d}'
        self.arm = {'arm': 'C', 'scratch_root': Path(scratch).as_posix()}
        self.target = ch.scratch_dir(self.arm, self.cdir, self.native)
        self.config = self.cdir / 'segments' / f'seg-{index:05d}.json'
        self.tmp = self.cdir / f'.{self.native.name}.persist'
        self.expected, self.bulk, self.done, self.written = {}, {}, 0, 0
        self._block = random.Random(f'{seed}/bulk').randbytes(max(profile['trajectory_bytes'],
                                                                 profile['checkpoint_bytes']))

    def put(self, rel, data, keep=True):
        path = self.target / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        with open(path, 'wb') as f:
            f.write(data)
        self.written += len(data)
        digest = hashlib.sha256(data).hexdigest()
        if keep:
            self.expected[rel] = digest
        return digest

    def blob(self, tag, size):
        head = hashlib.sha256(f'{self.seed}/{tag}'.encode()).digest()
        return (head + self._block)[:size]

    def mount(self, sim):
        self.target.mkdir(parents=True, exist_ok=True)
        self.cdir.mkdir(parents=True, exist_ok=True)
        sim.mount(self.native, self.target)

    def begin(self, updates):
        """run.json (the trainer publishes it before any iteration) and the segment config on the root volume."""
        self.put('run.json', doc(f'{self.seed}/run', self.p['run_bytes'], {'schema': 'fixture-run/v1'}))
        self.config.parent.mkdir(parents=True, exist_ok=True)
        self.config.write_bytes(doc(f'{self.seed}/config', self.p['config_bytes_per_update'] * updates,
                                    {'output_directory': self.native.name}))

    def start(self, i):
        """Iteration i in flight: its collection command and the first half of its trajectories."""
        it = f'iterations/{i:06d}/attempt-000000'
        self.put(f'{it}/collect-command.json', doc(f'{self.seed}/{i}/collect-command', self.p['records'][
            'attempt-000000/collect-command.json']))
        self.bulk[i] = []
        for j in range(self.p['episodes'] // 2):
            self._episode(i, j)

    def _episode(self, i, j):
        rel = f'iterations/{i:06d}/attempt-000000/collect/episode-{j:04d}.json'
        self.put(rel, self.blob(f'{i}/episode-{j}', self.p['trajectory_bytes']), keep=False)
        self.bulk[i].append(rel)

    def finish(self, i):
        """The rest of iteration i, complete.json last. Persist keeps its records, and the newest checkpoint."""
        it = f'iterations/{i:06d}/attempt-000000'
        for j in range(self.p['episodes'] // 2, self.p['episodes']):
            self._episode(i, j)
        for rel, size in self.p['records'].items():
            if not rel.endswith(('collect-command.json', 'update.json')):
                self.put(f'iterations/{i:06d}/{rel}', doc(f'{self.seed}/{i}/{rel}', size))
        checkpoint = f'{it}/update/checkpoint.json'
        digest = self.put(checkpoint, self.blob(f'{i}/checkpoint', self.p['checkpoint_bytes']), keep=False)
        self.bulk[i].append(checkpoint)
        update = f'{it}/update/update.json'  # segment-relative (a run's are absolute): the bytes are location-free
        self.put(update, doc(f'{self.seed}/{i}/update', self.p['records']['attempt-000000/update/update.json'],
                             {'after_state_sha256': digest, 'checkpoint': {'path': checkpoint, 'sha256': digest}}))
        self.put(f'iterations/{i:06d}/complete.json', doc(f'{self.seed}/{i}/complete', self.p['complete_bytes'], {
            'iteration': i, 'update': {'path': update}, 'output_identity': {'checkpoint_sha256': digest}}))
        if self.done:
            self.expected.pop(f'iterations/{self.done - 1:06d}/attempt-000000/update/checkpoint.json', None)
        self.expected[checkpoint] = digest
        self.done = i + 1

    def end(self):
        self.put('completion.json', doc(f'{self.seed}/completion', self.p['completion_bytes'], {'complete': True}))

    def build(self, updates, sim, reap_upto=None):
        """A whole segment, untimed: every iteration but the last `updates - reap_upto` reaped (the reaper's
        state at the segment end: all but the newest, or all but the last two if its last tick came first)."""
        self.mount(sim)
        self.begin(updates)
        for i in range(updates):
            self.start(i)
            self.finish(i)
        self.end()
        upto = updates - 1 if reap_upto is None else reap_upto
        for i in range(upto):
            for rel in self.bulk.pop(i, []):
                (self.target / rel).unlink()

    def fixture_digest(self):
        return digest_map(self.expected)

    def persisted(self):
        """(logical digest map, members, logical bytes, physical bytes) of the persisted segment: gzip members
        decompressed under their plain names; the marker is not a member."""
        out, logical, physical = {}, 0, 0
        for f in sorted(self.native.rglob('*')):
            if not f.is_file() or f.name == self.ch.PERSIST_MARKER:
                continue
            rel = f.relative_to(self.native).as_posix()
            data = f.read_bytes()
            physical += len(data)
            if f.name.endswith('.gz') and f.name[:-3] in self.ch.SCRATCH_GZIP:
                data, rel = gzip.decompress(data), rel[:-3]
            out[rel] = hashlib.sha256(data).hexdigest()
            logical += len(data)
        return out, len(out), logical, physical


def digest_map(m):
    return hashlib.sha256(''.join(f'{k}\t{m[k]}\n' for k in sorted(m)).encode()).hexdigest()


class MountSim:
    """`chains.mounted` and `chains.unmount` for symlink mounts, installed on the chains module while entered. The
    unmount (a `umount` process on a node) leaves the empty directory the real one leaves; it is not counted."""

    def __init__(self, ch):
        self.ch, self.mounts = ch, set()

    def __enter__(self):
        self._saved = (self.ch.mounted, self.ch.unmount)
        self.ch.mounted = lambda p: Path(p) in self.mounts
        self.ch.unmount = self.unmount
        return self

    def __exit__(self, *exc):
        self.ch.mounted, self.ch.unmount = self._saved

    def mount(self, native, target):
        os.symlink(target, native, target_is_directory=True)
        self.mounts.add(Path(native))

    def unmount(self, native, tries=20):
        with suspended():
            native = Path(native)
            if native in self.mounts:
                self.mounts.discard(native)
                os.unlink(native)
                os.mkdir(native)


# ---------------------------------------------------------------- measurement

def proc_io():
    try:
        with open('/proc/self/io', encoding='ascii') as f:
            return {k: int(v) for k, v in (line.split(':') for line in f if ':' in line)}
    except OSError:
        return {}


def rusage():
    import resource
    r = resource.getrusage(resource.RUSAGE_SELF)
    return {'user_ns': int(r.ru_utime * 1e9), 'sys_ns': int(r.ru_stime * 1e9), 'inblock': r.ru_inblock,
            'oublock': r.ru_oublock, 'nvcsw': r.ru_nvcsw, 'nivcsw': r.ru_nivcsw, 'minflt': r.ru_minflt,
            'maxrss_kb': r.ru_maxrss}


def measured(fn, full=False, classify=None, detail=False, memory=False):
    """Runs fn under Ops; returns (result, sample). `detail`: rusage and /proc/self/io deltas too."""
    io0 = proc_io() if detail else None
    ru0 = rusage() if detail else None
    if memory:
        import tracemalloc
        tracemalloc.start()
    with Ops(full=full, classify=classify) as ops:
        c0, t0 = time.process_time_ns(), time.perf_counter_ns()
        result = fn()
        t1, c1 = time.perf_counter_ns(), time.process_time_ns()
    s = {'wall_ns': t1 - t0, 'cpu_ns': c1 - c0, 'ops': ops}
    if memory:
        import tracemalloc
        s['py_peak_bytes'] = tracemalloc.get_traced_memory()[1]
        tracemalloc.stop()
    if detail:
        io1, ru1 = proc_io(), rusage()
        s['io'] = {k: io1[k] - io0[k] for k in io1 if k in io0}
        s['rusage'] = {k: ru1[k] - ru0[k] for k in ru1 if k != 'maxrss_kb'}
    return result, s


def tick_classifier(state):
    def classify(path):
        m = ITERATION.search(path)
        if m is None:
            return 'listing' if re.search(r'[\\/]iterations[\\/]?$', path) else 'other'
        i = int(m.group(1))
        if i < state['reaped']:
            return 'reaped'
        if i == state.get('new'):
            return 'new'
        if i == state['newest']:
            return 'newest'
        return 'in_flight'
    return classify


def persist_classifier(seg):
    native, target, tmp = str(seg.native), str(seg.target), str(seg.tmp)
    cdir, config = str(seg.cdir), str(seg.config)

    def classify(path):
        if path.startswith(tmp):
            return 'marker' if path.endswith(seg.ch.PERSIST_MARKER) else 'staging'
        if path == native or path.startswith(native + os.sep):
            return 'segment'
        if path == target or path.startswith(target + os.sep):
            return 'scratch'
        if path.endswith('.index.lock'):
            return 'lock'
        if path == cdir:
            return 'chain_dir'
        if path.startswith(config):
            return 'config'
        return 'other'
    return classify


def tick_counts(ops):
    row = {op: ops.total(op) for op in REAP_OPS}
    row.update({cat: ops.total(category=cat) for cat in REAP_CATEGORIES})
    return row


def op_summary(ops):
    """Counts and bytes (deterministic), apart from the times in `ops.ns`."""
    return {'counts': dict(sorted(ops.n.items())), 'bytes': dict(sorted(ops.bytes.items()))}


def run_segment(ch, n, profile, seed, scratch, work, count):
    """The reap ticks of a live segment of n updates, then its persist and config compression (one sample)."""
    rows = {'productive': collections.defaultdict(list), 'idle': collections.defaultdict(list)}
    with MountSim(ch) as sim:
        seg = Segment(ch, work, scratch, profile, seed)
        seg.mount(sim)
        seg.begin(n)
        seg.start(0)
        last = {}
        for i in range(n):
            seg.finish(i)
            if i + 1 < n:
                seg.start(i + 1)
            else:
                seg.end()
            for kind in ('productive', 'idle'):
                state = {'reaped': i if kind == 'idle' else max(i - 1, 0), 'new': i - 1 if kind == 'productive'
                         else None, 'newest': i}
                deleted, s = measured(lambda: ch.reap(seg.native), full=count,
                                      classify=tick_classifier(state) if count else None)
                if deleted != (len(seg.bulk[i - 1]) if kind == 'productive' and i else 0):
                    raise SystemExit(f'{kind} tick {i} deleted {deleted} files')
                r = rows[kind]
                if count:
                    r['deleted'].append(deleted)
                    for k, v in tick_counts(s['ops']).items():
                        r[k].append(v)
                    if i == n - 1:
                        last[kind] = op_summary(s['ops'])
                else:
                    r['wall_ns'].append(s['wall_ns'])
                    r['cpu_ns'].append(s['cpu_ns'])
            if i >= 1:
                for rel in seg.bulk.pop(i - 1):  # the productive tick deleted exactly this bulk
                    if (seg.target / rel).exists():
                        raise SystemExit(f'reap left {rel}')
        out = {'reap': {k: dict(v) for k, v in rows.items()}, 'fixture_bytes_written': seg.written}
        if count:
            out['reap']['last_tick'] = last
        expected = dict(seg.expected)
        returned, s = measured(lambda: ch.persist(seg.arm, seg.native, seg.cdir), full=count,
                               classify=persist_classifier(seg), detail=True, memory=count)
        out['persist'] = persist_row(seg, returned, s, count)
        members, n_members, logical, physical = seg.persisted()
        out['persist'].update({'members': n_members, 'logical_bytes': logical, 'physical_bytes': physical,
                               'logical_digest': digest_map(members), 'fixture_digest': digest_map(expected),
                               'identical': members == expected})
        size = seg.config.stat().st_size
        z, s = measured(lambda: ch.compress_config(seg.config), full=count, classify=persist_classifier(seg))
        out['compress_config'] = {'in_bytes': size, 'out_bytes': z, 'wall_ns': s['wall_ns'], 'cpu_ns': s['cpu_ns']}
        if count:
            out['compress_config'].update(op_summary(s['ops']))
    return out


def persist_row(seg, returned, s, count):
    row = {'returned_bytes': returned, 'wall_ns': s['wall_ns'], 'cpu_ns': s['cpu_ns'], 'io': s['io'],
           'rusage': s['rusage'], 'ns': dict(sorted(s['ops'].ns.items()))}
    if count:
        row.update(op_summary(s['ops']))
        row['py_peak_bytes'] = s['py_peak_bytes']
        row['fsyncs'] = {'file': s['ops'].total('fsync_file'), 'dir': s['ops'].total('fsync_dir')}
    else:
        row['fsyncs'] = s['ops'].total('fsync')
    return row


def run_settle(ch, n, profile, seed, scratch, work, count):
    """scratch_settle in its four states, each measured once (the cases are recovery paths: counts matter)."""
    out = {}
    cases = ('steady', 'swap_finish', 'late_persist', 'reset')
    for case in cases:
        base_w, base_s = Path(work) / case, Path(scratch) / case
        with MountSim(ch) as sim:
            segs = [Segment(ch, base_w, base_s, profile, seed, index=k) for k in range(2)]
            man = {'segments': [{'root': s.native.as_posix(), 'config': s.config.as_posix(), 'offset': k * n,
                                 'updates': n} for k, s in enumerate(segs)]}
            first, second = segs
            with suspended():
                if case in ('steady', 'swap_finish'):
                    first.build(2, sim)
                    ch.persist(first.arm, first.native, first.cdir)
                    second.build(2, sim)
                    ch.persist(second.arm, second.native, second.cdir)
                    if case == 'swap_finish':  # the driver stopped between the unmount and the rename
                        second.native.rename(second.tmp)
                        second.native.mkdir()
                elif case == 'late_persist':
                    first.build(2, sim)
                    ch.persist(first.arm, first.native, first.cdir)
                    second.build(n, sim)
                else:  # reset: the second segment unfinished (the trainer stopped in its last iteration)
                    first.build(2, sim)
                    ch.persist(first.arm, first.native, first.cdir)
                    second.mount(sim)
                    second.begin(n)
                    for i in range(n - 1):
                        second.start(i)
                        second.finish(i)
                    second.start(n - 1)
                    for i in range(n - 2):
                        for rel in second.bulk.pop(i, []):
                            (second.target / rel).unlink()
            root = first.root
            _, s = measured(lambda: ch.scratch_settle(first.arm, root, first.cdir, man), full=count,
                            classify=persist_classifier(second), detail=True)
            row = {'wall_ns': s['wall_ns'], 'cpu_ns': s['cpu_ns'], 'ns': dict(sorted(s['ops'].ns.items()))}
            if count:
                row.update(op_summary(s['ops']))
            row['outcome'] = {'segment_persisted': (second.native / ch.PERSIST_MARKER).exists(),
                              'mounted': ch.mounted(second.native),
                              'scratch_left': second.target.exists()}
            out[case] = row
    return out


def worker(argv):
    """One sample in this process: `_worker JSON` (src, scenario, n, profile, seed, scratch, work, count)."""
    a = json.loads(argv[0])
    sys.path.insert(0, a['src'])
    import chains as ch
    if Path(ch.__file__).resolve().parent != Path(a['src']).resolve():
        raise SystemExit(f'chains loaded from {ch.__file__}, not {a["src"]}')
    profile = PROFILES[a['profile']]
    scratch = tempfile.mkdtemp(prefix='w5-scratch-bench-', dir=a['scratch'])
    work = tempfile.mkdtemp(prefix='w5-scratch-bench-', dir=a['work'])
    try:
        fn = run_segment if a['scenario'] == 'segment' else run_settle
        out = fn(ch, a['n'], profile, a['seed'], scratch, work, a['count'])
    finally:
        shutil.rmtree(scratch, ignore_errors=True)
        shutil.rmtree(work, ignore_errors=True)
    out['maxrss_kb'] = rusage()['maxrss_kb']
    out['chains'] = str(Path(ch.__file__).resolve())
    sys.stdout.write(json.dumps(out, sort_keys=True) + '\n')
    return 0


# ---------------------------------------------------------------- orchestration

def file_sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def git(src, *args):
    try:
        r = subprocess.run(['git', '-C', str(src), *args], capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def source_pins(src):
    src = Path(src).resolve()
    return {'path': src.as_posix(), 'commit': git(src, 'rev-parse', 'HEAD'),
            'dirty_paths': (git(src, 'status', '--porcelain', '--', '.') or '').splitlines(),
            'files': {name: file_sha(src / name) for name in ('chains.py', 'retention.py')}}


def mount_of(path):
    """(mount point, file system type, source, options) of the mount holding `path`, from /proc/self/mounts."""
    path, best = os.path.realpath(path), None
    try:
        lines = Path('/proc/self/mounts').read_text().splitlines()
    except OSError:
        return None
    for line in lines:
        src, point, fstype, opts = line.split()[:4]
        point = point.replace('\\040', ' ')
        if (path == point or path.startswith(point.rstrip('/') + '/')) and (best is None or len(point) >= len(best[0])):
            best = (point, fstype, src, opts)
    return None if best is None else dict(zip(('mount', 'fstype', 'source', 'options'), best))


def machine():
    info = {'platform': platform.platform(), 'machine': platform.machine(), 'cpus': os.cpu_count(),
            'affinity': len(os.sched_getaffinity(0)) if hasattr(os, 'sched_getaffinity') else None,
            'python': sys.version, 'python_executable': sys.executable}
    try:
        cpu = [line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
               if line.startswith('model name')]
        info['cpu_model'] = cpu[0] if cpu else None
        mem = dict(line.split(':', 1) for line in Path('/proc/meminfo').read_text().splitlines() if ':' in line)
        info['mem_total_kb'] = int(mem['MemTotal'].split()[0])
    except OSError:
        pass
    return info


def fixture_bytes(profile, n):
    """Upper bound on one sample's live bytes: every record, two iterations' bulk and the staging copy."""
    p = profile
    records = sum(p['records'].values()) + p['complete_bytes']
    bulk = p['episodes'] * p['trajectory_bytes'] + p['checkpoint_bytes']
    return 2 * (n * records + p['run_bytes'] + p['completion_bytes'] + p['checkpoint_bytes']) + 3 * bulk + \
        n * p['config_bytes_per_update']


def schedule(sizes, variants, rounds):
    """Rounds over every (size, variant); odd rounds reverse both orders, so each size runs A-B-B-A."""
    out = []
    for r in range(rounds):
        fwd = r % 2 == 0
        for n in (sizes if fwd else sizes[::-1]):
            for v in (variants if fwd else variants[::-1]):
                out.append((r, n, v))
    return out


def spawn(args, timeout):
    t0 = time.perf_counter_ns()
    r = subprocess.run([sys.executable, str(Path(__file__).resolve()), WORKER, json.dumps(args)],
                       capture_output=True, text=True, timeout=timeout)
    if r.returncode != 0:
        raise SystemExit(f'worker failed ({r.returncode}): {args}\n{r.stderr[-4000:]}')
    out = json.loads(r.stdout.strip().splitlines()[-1])
    out['process_wall_ns'] = time.perf_counter_ns() - t0
    return out


def spread(xs):
    xs = list(xs)
    if not xs:
        return None
    m = statistics.fmean(xs)
    return {'n': len(xs), 'mean': m, 'min': min(xs), 'max': max(xs),
            'sd': statistics.stdev(xs) if len(xs) > 1 else 0.0,
            'range_over_mean': (max(xs) - min(xs)) / m if m else None}


def fit(xs, ys):
    """Least squares y = a + b x."""
    mx, my = statistics.fmean(xs), statistics.fmean(ys)
    sxx = sum((x - mx) ** 2 for x in xs)
    b = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sxx if sxx else 0.0
    return {'a': my - b * mx, 'b': b, 'max_abs_residual': max(abs(y - (my - b * mx) - b * x) for x, y in zip(xs, ys))}


def summarize(receipt):
    """Per variant and size: deterministic counts, then timing spread over rounds; with two variants, the paired
    (same round and size) ratios B / A."""
    out = {v: {str(n): {} for n in receipt['sizes']} for v in receipt['variants']}
    for v, by_n in receipt['counts'].items():
        for n_key, c in by_n.items():
            n = int(n_key)
            prod, idle = c['reap']['productive'], c['reap']['idle']
            hist = list(range(n))  # idle tick i: iterations 0..i-1 already reaped
            ops = [sum(idle[k][i] for k in ('scandir', 'entry', 'stat', 'lstat', 'unlink', 'open')) for i in range(n)]
            row = out[v][n_key]
            row['counts'] = {
                'idle_tick_ops_vs_reaped': fit(hist, ops) if n > 1 else None,
                'last_idle_tick': {k: idle[k][-1] for k in REAP_OPS + REAP_CATEGORIES},
                'last_productive_tick': {k: prod[k][-1] for k in REAP_OPS + REAP_CATEGORIES},
                'segment_totals': {kind: {k: sum(c['reap'][kind][k]) for k in REAP_OPS + REAP_CATEGORIES}
                                   for kind in ('productive', 'idle')},
                'persist_fsyncs': c['persist']['fsyncs'],
                'persist_members': c['persist']['members'],
                'persist_physical_bytes': c['persist']['physical_bytes'],
                'persist_logical_bytes': c['persist']['logical_bytes'],
                'persist_identical': c['persist']['identical'],
                'deterministic': c['deterministic'],
            }
    for v in out:
        for n_key, row in out[v].items():
            samples = [s for s in receipt['samples'] if s['variant'] == v and str(s['n']) == n_key]
            if not samples:
                continue
            n = int(n_key)
            reap = lambda s, kind, k: s['reap'][kind][k]  # noqa: E731
            row['time'] = {
                'reap_productive_total_ns': spread(sum(reap(s, 'productive', 'wall_ns')) for s in samples),
                'reap_idle_total_ns': spread(sum(reap(s, 'idle', 'wall_ns')) for s in samples),
                'reap_idle_cpu_total_ns': spread(sum(reap(s, 'idle', 'cpu_ns')) for s in samples),
                'reap_idle_last_tick_ns': spread(statistics.median(reap(s, 'idle', 'wall_ns')[-max(1, n // 16):])
                                                 for s in samples),
                'reap_idle_first_tick_ns': spread(statistics.median(reap(s, 'idle', 'wall_ns')[:max(1, n // 16)])
                                                  for s in samples),
                'reap_idle_ns_per_reaped_iteration': spread(
                    fit(list(range(n)), reap(s, 'idle', 'wall_ns'))['b'] for s in samples) if n > 1 else None,
                'persist_wall_ns': spread(s['persist']['wall_ns'] for s in samples),
                'persist_cpu_ns': spread(s['persist']['cpu_ns'] for s in samples),
                'persist_fsync_ns': spread(s['persist']['ns'].get('fsync', 0) for s in samples),
                'persist_gzip_ns': spread(s['persist']['ns'].get('gzip', 0) for s in samples),
                'persist_copy2_ns': spread(s['persist']['ns'].get('copy2', 0) for s in samples),
                'persist_rmtree_ns': spread(s['persist']['ns'].get('rmtree', 0) for s in samples),
                'persist_lock_hold_ns': spread(s['persist']['ns'].get('lock_hold', 0) for s in samples),
                'compress_config_wall_ns': spread(s['compress_config']['wall_ns'] for s in samples),
                'worker_maxrss_kb': spread(s['maxrss_kb'] for s in samples),
            }
    variants = list(receipt['variants'])
    if len(variants) == 2:
        out['paired'] = paired(receipt, *variants)
    return out


def compact(receipt):
    """The receipt as written, after its summary: per-tick reap times in whole microseconds beside per-sample
    totals, and the count pass's per-tick rows (equal in both runs; fitted under `summary`) as first, last and
    totals."""
    for s in receipt['samples']:
        for kind in ('productive', 'idle'):
            r = s['reap'][kind]
            s['reap'][kind] = {'wall_us': [round(x / 1000) for x in r['wall_ns']], 'wall_ns_total': sum(r['wall_ns']),
                               'cpu_ns_total': sum(r['cpu_ns'])}
    for by_n in receipt['counts'].values():
        for c in by_n.values():
            for kind in ('productive', 'idle'):
                rows = c['reap'][kind]
                c['reap'][kind] = {'ticks': len(rows['deleted']), 'first': {k: v[0] for k, v in rows.items()},
                                   'last': {k: v[-1] for k, v in rows.items()},
                                   'totals': {k: sum(v) for k, v in rows.items()}}
    return receipt


def dumps(doc):
    """Indented JSON with each list of numbers on one line."""
    text = json.dumps(doc, indent=1, sort_keys=True)
    return re.sub(r'\[\n\s*(-?[0-9][^\[\]{}"]*?)\n\s*\]',
                  lambda m: '[' + re.sub(r',\n\s*', ', ', m.group(1)) + ']', text) + '\n'


PAIRED = {'reap_ns': lambda s: sum(s['reap']['productive']['wall_ns']) + sum(s['reap']['idle']['wall_ns']),
          'persist_ns': lambda s: s['persist']['wall_ns']}


def paired(receipt, a, b):
    """Per size and metric: both arms' samples by round, B / A, and a verdict: a difference of means within either
    arm's own range over rounds is inconclusive (a single round always is)."""
    by_size = {}
    for n in receipt['sizes']:
        pairs = []
        for r in range(receipt['rounds']):
            sa = [s for s in receipt['samples'] if (s['variant'], s['n'], s['round']) == (a, n, r)]
            sb = [s for s in receipt['samples'] if (s['variant'], s['n'], s['round']) == (b, n, r)]
            if sa and sb:
                pairs.append((sa[0], sb[0]))
        if not pairs:
            continue
        by_size[str(n)] = {}
        for name, f in PAIRED.items():
            xa, xb = [f(p[0]) for p in pairs], [f(p[1]) for p in pairs]
            ma, mb = statistics.fmean(xa), statistics.fmean(xb)
            noise = max(max(xa) - min(xa), max(xb) - min(xb))
            verdict = 'inconclusive' if len(pairs) < 2 or abs(mb - ma) <= noise else \
                ('b_faster' if mb < ma else 'b_slower')
            by_size[str(n)][name] = {'a': xa, 'b': xb, 'ratio_of_means_b_over_a': mb / ma,
                                     'paired_ratios_b_over_a': [y / x for x, y in zip(xa, xb)],
                                     'difference_of_means_ns': mb - ma, 'larger_arm_range_ns': noise,
                                     'verdict': verdict}
    return {'a': a, 'b': b, 'by_size': by_size}


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == [WORKER]:
        return worker(argv[1:])
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('--out', required=True)
    ap.add_argument('--sizes', default='64,256', help='segment lengths (scratch: multiples of 64 dividing 4096)')
    ap.add_argument('--rounds', type=int, default=4)
    ap.add_argument('--profile', default='small', choices=sorted(PROFILES))
    ap.add_argument('--seed', type=int, default=601)
    ap.add_argument('--scratch-dir', default='/dev/shm' if os.path.isdir('/dev/shm') else tempfile.gettempdir())
    ap.add_argument('--work-dir', default=tempfile.gettempdir())
    ap.add_argument('--src', default=str(HERE.parent), help='tools/w5 directory of variant A (this checkout)')
    ap.add_argument('--candidate-src', help='tools/w5 directory of variant B')
    ap.add_argument('--settle', action='store_true', help='also measure scratch_settle in its four states')
    ap.add_argument('--no-counts', action='store_true', help='timings only (counts take a slower hooked pass)')
    ap.add_argument('--max-wall-seconds', type=float, default=3600.0)
    ap.add_argument('--max-fixture-bytes', type=int, default=2 * 10**9)
    ap.add_argument('--quiet', action='store_true', help='no progress lines on stderr')
    a = ap.parse_args(argv)
    say = (lambda *_: None) if a.quiet else (lambda text: print(text, file=sys.stderr))
    if not sys.platform.startswith('linux'):
        raise SystemExit('scratch segments are Linux only')
    sizes = [int(x) for x in a.sizes.split(',')]
    if any(n < 2 for n in sizes):
        raise SystemExit('every size is at least 2 updates (a segment with something to reap)')
    profile = PROFILES[a.profile]
    need = max(fixture_bytes(profile, n) for n in sizes)
    if need > a.max_fixture_bytes:
        raise SystemExit(f'a sample needs up to {need} bytes, over --max-fixture-bytes {a.max_fixture_bytes}')
    for d in {a.scratch_dir, a.work_dir}:
        free = shutil.disk_usage(d).free
        if free < 2 * need:
            raise SystemExit(f'{d} has {free} bytes free; a sample needs up to {need} (twice that is required)')
    variants = {'A': source_pins(a.src)}
    if a.candidate_src:
        variants['B'] = source_pins(a.candidate_src)
    srcs = {'A': a.src, 'B': a.candidate_src}
    t_start = time.monotonic()
    receipt = {'schema': SCHEMA, 'purpose': 'mtg-kernel#601 milestone 1: scratch reaper and persistence work',
               'argv': argv, 'created_unix': round(time.time(), 3), 'complete': False,
               'bench': {'path': Path(__file__).resolve().as_posix(), 'sha256': file_sha(__file__),
                         'commit': git(HERE, 'rev-parse', 'HEAD'),
                         'dirty_paths': (git(HERE, 'status', '--porcelain', '--', '.') or '').splitlines()},
               'machine': machine(), 'dirs': {'scratch': mount_of(a.scratch_dir), 'work': mount_of(a.work_dir)},
               'profile': {'name': a.profile, **profile}, 'seed': a.seed, 'sizes': sizes, 'rounds': a.rounds,
               'variants': variants, 'max_fixture_bytes_per_sample': need, 'counts': {}, 'samples': [],
               'settle': {}}

    def remaining():
        left = a.max_wall_seconds - (time.monotonic() - t_start)
        if left <= 0:
            raise TimeoutError
        return left

    def base(v, n, scenario, count):
        return {'src': str(Path(srcs[v]).resolve()), 'scenario': scenario, 'n': n, 'profile': a.profile,
                'seed': a.seed, 'scratch': a.scratch_dir, 'work': a.work_dir, 'count': count}

    try:
        if not a.no_counts:
            for v in variants:
                receipt['counts'][v] = {}
                for n in sizes:
                    first = spawn(base(v, n, 'segment', True), remaining())
                    second = spawn(base(v, n, 'segment', True), remaining())
                    same = first['reap'] == second['reap'] and all(
                        first[k][f] == second[k][f] for k, f in DETERMINISTIC)
                    first['deterministic'] = same
                    receipt['counts'][v][str(n)] = first
                    say(f'counts {v} n={n}: deterministic={same}')
        receipt['order'] = [{'round': r, 'n': n, 'variant': v} for r, n, v in schedule(sizes, list(variants),
                                                                                       a.rounds)]
        for pos, (r, n, v) in enumerate(schedule(sizes, list(variants), a.rounds)):
            s = spawn(base(v, n, 'segment', False), remaining())
            s.update({'round': r, 'position': pos, 'variant': v, 'n': n})
            receipt['samples'].append(s)
            say(f'round {r} n={n} {v}: reap {sum(s["reap"]["productive"]["wall_ns"]) / 1e6:.1f} + '
                f'{sum(s["reap"]["idle"]["wall_ns"]) / 1e6:.1f} ms, persist {s["persist"]["wall_ns"] / 1e6:.1f} ms')
        if a.settle:
            for v in variants:
                receipt['settle'][v] = {'counts': None if a.no_counts else
                                        spawn(base(v, max(sizes), 'settle', True), remaining()),
                                        'time': spawn(base(v, max(sizes), 'settle', False), remaining())}
        receipt['complete'] = True
    except TimeoutError:
        receipt['stopped'] = f'--max-wall-seconds {a.max_wall_seconds} reached'
    receipt['wall_seconds'] = round(time.monotonic() - t_start, 3)
    receipt['summary'] = summarize(receipt)
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_name(out.name + '.tmp')
    tmp.write_text(dumps(compact(receipt)), encoding='utf-8')
    os.replace(tmp, out)
    say(f'wrote {out} (complete={receipt["complete"]}, {receipt["wall_seconds"]} s)')
    return 0 if receipt['complete'] else 3


if __name__ == '__main__':
    sys.exit(main())
