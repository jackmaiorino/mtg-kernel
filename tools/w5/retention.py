"""W5 retention, budget and replay selection (collab LANES/lead/W5-DESIGN.md v1.3 sections 2.2 and 9; E7).

A chain is a sequence of native training segments (`native_expanded_training_run_v1` configs) listed in a chain
manifest (schema `w5-chain/v1`):

  {"schema": "w5-chain/v1", "chain": "w5/chain1", "arm": "X" | "C", "launched_unix": 1.0e9,
   "x_cap_seconds": 907200.0,  (optional; the X cap frozen at Stage 0, default 10.5 days)
   "snapshot_dir": "/abs/dir",  (optional; default <manifest stem>-snapshots beside the manifest)
   "c_wall_seconds": 604800.0,  (optional, C manifests; the projection horizon, default the 7-day C wall)
   "segments": [{"config": "/abs/seg-000.json", "offset": 0}, ...]}

A segment entry may also carry `root` and `updates` (the chain driver, tools/w5/chains.py, writes them from the
config it wrote); then the config is not read again, which keeps a long C chain's manifest cheap to load.

`offset` is the global update index of the segment's first iteration. Boundary b is the state after b updates
(b = 0 is the parent). Every segment receipt records the update's after-state hash; with `chain_ledger` it also
records the chain-only ledger (`w5-chain-ledger/v1`).

Rules (frozen in the design):
- Full states: X chains keep every boundary b with b % 64 == 0. C chains keep b % 4096 == 0, and b % 64 == 0
  within the most recent 4,096 updates. Every other checkpoint is pruned once its segment and the next one are
  complete (a live segment resumes from its own checkpoints and the next segment's source).
- Trajectories (`collect/episode-*.json` and their EX1/W5 search-target sidecars `collect/ex1-*.json`): kept for
  the most recent 64 complete segments (rolling).
- Projection (C chain): R-hat(t) = CPU(t) + rate(last 24 h, or since launch) x (7 days - t).
- Snapshots (X chain): from the first update whose ledger passes 0.85 R-hat, every update keeps a weights-only
  snapshot while R_i is open, if (measured snapshot bytes) x (projected updates between 0.85 and 1.10 R-hat) fits
  30 GB. A snapshot (`checkpoint_container_v1 weights-snapshot`) is the parameters alone, about a third of a full
  state; it is cut from the full checkpoint before that checkpoint is pruned, and checked against the recorded
  after-state hash. For play it is materialized (`checkpoint_container_v1 materialize`): the same parameters with
  zero moments. It never resumes training.
- Pause at 1.10 R-hat while C runs. When C ends, R_i is its final ledger; X's endpoint is the first boundary whose
  ledger is at or past R_i. If X paused below R_i, it resumes with `stop_at_usec = R_i`.
- Selection: the endpoint's checkpoint if kept, else its weights-only snapshot, else a replay from the nearest
  kept full state at or before it, which must reproduce every recorded hash. Then a PRUNE manifest lists every
  non-selected full state and snapshot, by kind.
- S learners (XS): a full state is two files, the S checkpoint record (`checkpoint.json`, schema
  `w5-s-learner-checkpoint/v1`) and the S container it pins (`s-train-state.bin`). The container's pin comes from
  the durable update receipt (`update.json` `s_learner.state`) into the index, never from the record, so a
  missing or damaged record cannot hide or redirect it. Every rule above treats the pair as one state: planned
  container first, pruned, kept, verified and counted together, never a file a surviving state uses, no path
  twice (`checkpoint_parts`).

Commands:
  retention.py index CHAIN.json                    per-update index (cached in INDEX next to the manifest; a
                                                   long-lived caller keeps it in memory with IndexCache)
  retention.py snapshot CHAIN.json BINARY --from B  weights-only snapshots of every unpruned checkpoint from B
  retention.py plan CHAIN.json [--snapshot-from B]  rolling prune plan (checkpoints, trajectories) as JSON
  retention.py apply PLAN.json [--hold DIR]         delete (or move to a same-volume hold folder) and log
  retention.py project C_CHAIN.json [--now T]      R-hat and the C chain's rate
  retention.py decide X_CHAIN.json C_CHAIN.json [--c-ended] [--x-running-seconds S] [--now T]
  retention.py select X_CHAIN.json R_USEC          endpoint, and the replay spec when its state is not kept
  retention.py replay-config X_CHAIN.json SPEC.json OUT.json OUTPUT_DIR
  retention.py verify-replay SPEC.json REPLAY_OUTPUT_DIR
  retention.py prune-manifest CHAIN.json SELECTED_BOUNDARY OUT.json
  retention.py guard --cap-bytes N [--next-bytes M] [--chain CHAIN.json ...] ROOT [ROOT ...]
"""
import argparse
import codecs
import contextlib
import hashlib
import io
import math
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from array import array
from collections.abc import Sequence
from pathlib import Path

FULL_STRIDE = 64
C_LONG_STRIDE = 4096
C_RECENT = 4096
TRAJECTORY_SEGMENTS = 64
# A trajectory and its search-target sidecar roll off together.
TRAJECTORY_GLOBS = ('*/attempt-*/collect/episode-*.json', '*/attempt-*/collect/ex1-*.json')
DAY = 86400.0
HORIZON = 7 * DAY
SNAPSHOT_TRIGGER = 0.85
PAUSE_AT = 1.10
SNAPSHOT_CAP_BYTES = 30 * 10**9
X_CAP_SECONDS = 10.5 * DAY  # default; a chain manifest's `x_cap_seconds` (frozen at Stage 0) overrides it
HISTORY_KINDS = ('initial', 'completed_iteration')
INDEX_SCHEMA = 'w5-chain-index/v1'


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, doc):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(doc, indent=1) + '\n', encoding='utf-8')
    os.replace(tmp, path)


def sha(path):
    with open(path, 'rb') as f:
        if hasattr(hashlib, 'file_digest'):
            return hashlib.file_digest(f, 'sha256').hexdigest()
        h = hashlib.sha256()
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def state_pin(update_doc):
    """The S or wide container an update published (`update.json`'s `s_learner.state` or `w_learner.state`, the
    wide net-8w128 learner's, which retention treats exactly as S's), or None for net-8."""
    return (update_doc.get('s_learner') or update_doc.get('w_learner') or {}).get('state')


def checkpoint_parts(pin, state=None):
    """The files of one full train state, container first: an S update's container (`state`, from the durable
    update receipt, never from the record) and the checkpoint itself. Each part is {'path', 'sha256', 'part'}."""
    parts = []
    if state is not None:
        parts.append({'path': Path(state['path']).as_posix(), 'sha256': state['sha256'], 'part': 's_state'})
    parts.append({'path': Path(pin['path']).as_posix(), 'sha256': pin['sha256'], 'part': 'checkpoint'})
    return parts


def record_parts(rec):
    return checkpoint_parts(rec['checkpoint'], rec.get('state'))


def state_present(pin, state=None):
    """Every file of the state exists."""
    return all(Path(p['path']).exists() for p in checkpoint_parts(pin, state))


def state_present_any(pin, state=None):
    """Some file of the state exists."""
    return any(Path(p['path']).exists() for p in checkpoint_parts(pin, state))


def state_intact(pin, state=None):
    """Every file of the state exists and matches its pin."""
    return all(Path(p['path']).exists() and sha(p['path']) == p['sha256'] for p in checkpoint_parts(pin, state))


def load_chain(path):
    chain = read(path)
    if chain.get('schema') != 'w5-chain/v1' or chain.get('arm') not in ('X', 'C'):
        raise SystemExit('stop: not a w5-chain/v1 manifest with arm X or C')
    offset = 0
    for seg in chain['segments']:
        if seg['offset'] != offset:
            raise SystemExit('stop: segment offsets are not contiguous from 0')
        if 'root' not in seg or 'updates' not in seg:
            cfg = read_config(seg['config'])
            seg['root'] = cfg['output_directory']
            seg['updates'] = len(cfg['iterations'])
        offset += seg['updates']
    return chain


def read_config(path):
    """A segment config, or its `<name>.gz` form (chains.py compresses finished scratch segments' configs)."""
    p = Path(path)
    if not p.exists() and p.with_name(p.name + '.gz').exists():
        import gzip
        with gzip.open(p.with_name(p.name + '.gz'), 'rb') as f:
            return json.loads(f.read().decode('utf-8'))
    return read(p)


def segment_complete(seg):
    return Path(seg['root'], 'completion.json').exists()


def index_path(chain_path):
    return Path(chain_path).with_suffix('.index.jsonl')


def scan(chain, known):
    """Index records of the completed updates from `known` on, in order, stopping at the first update without its
    receipt (a segment's iterations complete in order)."""
    new = []
    for s, seg in enumerate(chain['segments']):
        if seg['offset'] + seg['updates'] <= known:
            continue
        if os.path.ismount(seg['root']):  # an in-flight scratch segment (chains.py): indexed once persisted
            return new
        for i in range(max(0, known - seg['offset']), seg['updates']):
            complete = Path(seg['root'], 'iterations', f'{i:06d}', 'complete.json')
            if not complete.exists():
                return new
            receipt = read(complete)
            doc = read(receipt['update']['path'])
            ledger = receipt.get('chain_ledger') or {}
            new.append({'update': seg['offset'] + i, 'boundary': seg['offset'] + i + 1, 'segment': s, 'iteration': i,
                        'after_state_sha256': doc['after_state_sha256'], 'checkpoint': doc['checkpoint'],
                        'state': state_pin(doc),
                        'ledger_usec': ledger.get('usec'), 'unix_seconds': ledger.get('unix_seconds'),
                        'instance': ledger.get('instance')})
    return new


@contextlib.contextmanager
def index_lock(chain_path):
    """The chain index file's writers (`index`, `IndexCache.refresh`) take turns: each reads, scans and appends under
    this lock, so no two append the same records."""
    path = Path(chain_path).with_suffix('.index.lock')
    with open(path, 'a+b') as f:
        if os.name == 'nt':
            import msvcrt
            f.seek(0)
            while True:
                try:
                    msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError:
                    time.sleep(0.05)
            try:
                yield
            finally:
                f.seek(0)
                msvcrt.locking(f.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(f.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(f.fileno(), fcntl.LOCK_UN)


def index(chain_path):
    """One record per completed update: update, boundary, segment, after-state hash, checkpoint pin, ledger."""
    with index_lock(chain_path):
        return index_locked(chain_path)


def index_locked(chain_path):
    chain = load_chain(chain_path)
    cache = index_path(chain_path)
    records = []
    if cache.exists():
        records = [json.loads(line) for line in cache.read_text(encoding='utf-8').splitlines() if line]
    if any('state' not in r for r in records):
        # Caches written before S containers were tracked: read each record's container pin from its durable
        # update receipt once, then rewrite the cache.
        for rec in records:
            if 'state' not in rec:
                seg = chain['segments'][rec['segment']]
                complete = Path(seg['root'], 'iterations', f"{rec['iteration']:06d}", 'complete.json')
                rec['state'] = state_pin(read(read(complete)['update']['path']))
        tmp = cache.with_suffix(cache.suffix + '.tmp')
        tmp.write_text(''.join(json.dumps(r) + '\n' for r in records), encoding='utf-8')
        os.replace(tmp, cache)
    new = scan(chain, len(records))
    if new:
        with open(cache, 'ab') as f:
            terminate(f)
            f.write(''.join(json.dumps(rec) + '\n' for rec in new).encode('utf-8'))
    return chain, records + new


def terminate(f):
    """Before an append (`f` open in 'ab'): a last line left without its newline (an interrupted append whose record
    is complete, which `index` reads as a line) gets one, so the new lines never join it."""
    size = os.fstat(f.fileno()).st_size
    if size:
        with open(f.name, 'rb') as r:
            r.seek(size - 1)
            if r.read(1) != b'\n':
                f.write(b'\n')


LINE_BREAK = re.compile('[\n\r\x0b\x0c\x1c\x1d\x1e\x85  ]')  # what str.splitlines splits on


def byte_length(text):
    return len(text) if text.isascii() else len(text.encode('utf-8'))


def stream_lines(f, start=0, chunk=1 << 20):
    """(text, byte offset, byte length) of every non-empty line of `f` from byte `start` on, split as `index` splits
    the file (`read_text(encoding='utf-8').splitlines()`, empty lines skipped), holding one chunk and the line being
    read at a time (a CR-only file or a long line costs that line, not the file)."""
    decoder = codecs.getincrementaldecoder('utf-8')()
    f.seek(start)
    pos, carry, carry_cr = start, [], False
    while True:
        block = f.read(chunk)
        final = not block
        new = decoder.decode(block, final=final)
        if not final and not carry_cr and not LINE_BREAK.search(new):
            carry.append(new)  # still inside one line
            continue
        pieces = (''.join(carry) + new).splitlines(keepends=True)
        carry, carry_cr = [], False
        if not final and pieces:
            last = pieces[-1]
            if last.endswith('\r') or not LINE_BREAK.search(last[-1]):  # unfinished, or a CR a LF may follow
                carry, carry_cr = [pieces.pop()], last.endswith('\r')
        for piece in pieces:
            text = piece.splitlines()[0] if piece else ''
            if text:
                yield text, pos, byte_length(text)
            pos += byte_length(piece)
        if final:
            return


def parse_bytes(data, start):
    """(record, offset, length) for every record of index file bytes `data` read from file offset `start`."""
    return [(json.loads(text), start + offset, length) for text, offset, length in stream_lines(io.BytesIO(data))]


class Boundaries:
    """A set of boundaries (`IndexCache.settled`) at one bit per boundary: an integer boundary from 0 below 2**28 is
    a bit (a value equal to one, such as True or 2.0, is the same member, as in a set), anything else is in a set."""
    LIMIT = 1 << 28

    def __init__(self, values=()):
        self.bits, self.count, self.other = bytearray(), 0, set()
        self.update(values)

    def _bit(self, b):
        if type(b) is not int:
            if not isinstance(b, (int, float)) or isinstance(b, float) and not b.is_integer():
                return None
            b = int(b)
        return b if 0 <= b < self.LIMIT else None

    def add(self, b):
        i = self._bit(b)
        if i is None:
            self.other.add(b)
            return
        if i >> 3 >= len(self.bits):
            self.bits.extend(bytes(max((i >> 3) + 1 - len(self.bits), len(self.bits))))
        if not self.bits[i >> 3] & (1 << (i & 7)):
            self.bits[i >> 3] |= 1 << (i & 7)
            self.count += 1

    def update(self, values):
        for b in values:
            self.add(b)

    def __ior__(self, values):
        self.update(values)
        return self

    def __contains__(self, b):
        i = self._bit(b)
        if i is None:
            try:
                return b in self.other
            except TypeError:  # unhashable: never a member
                return False
        return i >> 3 < len(self.bits) and bool(self.bits[i >> 3] & (1 << (i & 7)))

    def __len__(self):
        return self.count + len(self.other)

    def __iter__(self):
        for i, byte in enumerate(self.bits):
            if byte:
                for k in range(8):
                    if byte & (1 << k):
                        yield (i << 3) | k
        yield from self.other


class Missing:
    """A field a record does not have (`IndexRecords` columns)."""
    __slots__ = ()

    def __repr__(self):
        return 'MISSING'


MISSING = Missing()


class IndexChanged(RuntimeError):
    """A cached index line no longer reads as it did: the file was rewritten in place (it is append-only)."""


class IndexRecords:
    """The records of one chain index file as `IndexCache` keeps them: each record's line offset and length in the
    file and the scalar fields that scans over every record read (`FIELDS`) in typed arrays, 56 bytes a record; a
    whole record is parsed again from its line when it is read. A value an array cannot hold exactly (None, an
    integer past 64 bits, a float in an integer field) or a field the record lacks (`MISSING`) is kept beside the
    array for that record alone, so every value reads back exactly as it was parsed.

    The index is append-only. A line read again must hold the record parsed from it: its length, its one line and
    all five cached fields are checked, and a difference raises `IndexChanged` (logged with the chain and boundary)
    before the record is used. On POSIX the indexed file stays open, so a replacement cannot change what is read; on
    Windows, which refuses to replace an open file, it is opened for each read and must still be the indexed file.
    Records a read-only cache found but did not write are kept parsed (`extra`)."""

    FIELDS = (('update', 'q'), ('boundary', 'q'), ('ledger_usec', 'q'), ('unix_seconds', 'd'), ('instance', 'q'))

    def __init__(self, path):
        self.path = Path(path)
        self.offsets, self.lengths = array('q'), array('q')
        self.values = {name: array(code) for name, code in self.FIELDS}
        self.other = {name: {} for name, _ in self.FIELDS}  # record number to a value its array cannot hold
        self.extra = {}  # record number to a record not in the file (read-only caches)
        self.handle, self.ident = None, None

    def __len__(self):
        return len(self.offsets)

    def append(self, rec, offset, length):
        n = len(self.offsets)
        self.offsets.append(offset)
        self.lengths.append(length)
        for name, code in self.FIELDS:
            v = rec.get(name, MISSING)
            if type(v) is (int if code == 'q' else float) and (code == 'd' or -(1 << 63) <= v < 1 << 63):
                self.values[name].append(v)
            else:
                self.values[name].append(0)
                self.other[name][n] = v

    def append_unwritten(self, rec):
        """A record a read-only cache found past the file's end (kept parsed; it is not in the file)."""
        self.extra[len(self.offsets)] = rec
        self.append(rec, -1, -1)

    def attach(self, ident):
        """The records index the file now at the path, whose (st_dev, st_ino) is `ident` (None: no file)."""
        self.ident = ident
        if ident is not None and os.name != 'nt':
            f = open(self.path, 'rb')
            st = os.fstat(f.fileno())
            if (st.st_dev, st.st_ino) != ident:
                f.close()
                raise IndexChanged(f'stop: {self.path} was replaced while it was read')
            self.handle = f

    @contextlib.contextmanager
    def reader(self):
        if self.handle is not None:
            yield self.handle
            return
        if self.ident is None:  # no file: only unwritten records
            yield None
            return
        with open(self.path, 'rb') as f:
            st = os.fstat(f.fileno())
            if (st.st_dev, st.st_ino) != self.ident:
                raise IndexChanged(f'stop: {self.path} was replaced under its cached records')
            yield f

    def value(self, name, n):
        other = self.other[name]
        return other[n] if n in other else self.values[name][n]

    def read(self, f, n):
        if n in self.extra:
            return self.extra[n]
        f.seek(self.offsets[n])
        data = f.read(self.lengths[n])
        try:
            text = data.decode('utf-8')
            rec = json.loads(text) if len(data) == self.lengths[n] and len(text.splitlines()) == 1 else None
        except ValueError:
            rec = None
        bad = rec is None or not isinstance(rec, dict) or any(
            not same(rec.get(name, MISSING), self.value(name, n)) for name, _ in self.FIELDS)
        if bad:
            boundary = self.value('boundary', n)
            print(json.dumps({'event': 'index-changed', 'chain': self.path.parent.name, 'index': self.path.as_posix(),
                              'record': n, 'boundary': None if boundary is MISSING else boundary,
                              'offset': self.offsets[n]}, default=str), file=sys.stderr, flush=True)
            raise IndexChanged(f'stop: {self.path}: the line of record {n} (boundary {boundary}) at byte '
                               f'{self.offsets[n]} changed after it was read; the index is append-only (supervise '
                               f'reads it whole again when restarted)')
        return rec

    def check(self, n):
        """Reads record `n` again (raises `IndexChanged` if its line changed)."""
        with self.reader() as f:
            return self.read(f, n)

    def column(self, name, numbers, absent):
        """The field's values of the given record numbers, in order. A record without the field gives None
        (`absent='get'`, as `rec.get`) or raises KeyError when it is reached (`absent='raise'`, as `rec[name]`)."""
        values, other = self.values[name], self.other[name]
        if isinstance(numbers, range) and numbers.step == 1:
            chunk = values[numbers.start:numbers.stop]
            pairs = zip(numbers, chunk) if other else None
            if pairs is None:
                return iter(chunk)
        else:
            pairs = ((n, values[n]) for n in numbers)
            if not other:
                return (v for _, v in pairs)

        def read():
            for n, v in pairs:
                if n in other:
                    v = other[n]
                    if v is MISSING:
                        if absent == 'raise':
                            raise KeyError(name)
                        v = None
                yield v
        return read()

    def view(self):
        return IndexView(self, range(len(self.offsets)))


def same(a, b):
    """Equal and of one type (True is not 1), NaN equal to itself."""
    return type(a) is type(b) and (a == b or (isinstance(a, float) and a != a and b != b))


class IndexView(Sequence):
    """A fixed list of `IndexRecords` records (what `IndexCache.refresh` returns, and `IndexCache.active`). It
    supports what the supervise cycle and the pair code do with the list `index` returns: len, truth, an integer
    index (negative too), slices (a view), iteration, reversed, `in`, `index`, `count`, and == / != against a list
    or a view (never equal to a tuple, as a list is not). Each read parses the record from its line (a fresh dict
    each time). It has no list mutation (append, extend, sort, del, item assignment raise), and records appended to
    the cache later are not in it, as with a copied list. `column`, `where` and `lookup` serve the compact fields
    without parsing records."""

    def __init__(self, store, numbers):
        self.store, self.numbers = store, numbers

    def __len__(self):
        return len(self.numbers)

    def __getitem__(self, i):
        if isinstance(i, slice):
            return IndexView(self.store, self.numbers[i])
        n = self.numbers[i]
        with self.store.reader() as f:
            return self.store.read(f, n)

    def __iter__(self):
        if not len(self.numbers):  # nothing to read (the index file may not exist yet)
            return
        with self.store.reader() as f:
            for n in self.numbers:
                yield self.store.read(f, n)

    def __reversed__(self):
        for j in range(len(self.numbers) - 1, -1, -1):
            yield self[j]

    def __eq__(self, other):
        if not isinstance(other, (list, IndexView)):
            return NotImplemented
        return len(self) == len(other) and all(a == b for a, b in zip(self, other))

    __hash__ = None

    def __repr__(self):
        return f'IndexView({len(self)} records of {self.store.path})'

    def column(self, name, absent='raise'):
        """The field of every record, in order, without parsing them (see `IndexRecords.column`)."""
        return self.store.column(name, self.numbers, absent)

    def where(self, name, keep, absent='raise'):
        """The records whose field `name` satisfies `keep`, in order, as a view."""
        picked = array('q', (n for n, v in zip(self.numbers, self.column(name, absent)) if keep(v)))
        if picked and picked[-1] - picked[0] + 1 == len(picked) and all(
                b - a == 1 for a, b in zip(picked, picked[1:])):
            return IndexView(self.store, range(picked[0], picked[-1] + 1))
        return IndexView(self.store, picked)

    def pick(self, positions):
        """The records at the given positions of this view, in that order, as a view."""
        return IndexView(self.store, array('q', (self.numbers[j] for j in positions)))

    def lookup(self, name):
        """A mapping from each value of the field to the last record holding it (as a dict comprehension over the
        records builds it), parsing only the records looked up."""
        return LazyRecords(self.store, {v: n for n, v in zip(self.numbers, self.column(name))})


class LazyRecords:
    """`IndexView.lookup`'s mapping: value to record, the record parsed when it is looked up."""

    def __init__(self, store, numbers):
        self.store, self.numbers = store, numbers

    def __contains__(self, key):
        return key in self.numbers

    def __getitem__(self, key):
        with self.store.reader() as f:
            return self.store.read(f, self.numbers[key])

    def get(self, key, default=None):
        return self[key] if key in self.numbers else default


def fields(records, *names, absent='get'):
    """Iterators over the named fields of every record, in order, from an `IndexView`'s compact columns without
    parsing the records (a record without a field: None with `absent='get'`, KeyError with `absent='raise'`); None
    for any other sequence (then read the records)."""
    if not isinstance(records, IndexView):
        return None
    return [records.column(name, absent) for name in names]


def migrate(chain_path, chain):
    """`index_locked`'s one-time migration of a cache written before S containers were tracked (each record's
    container pin read from its durable update receipt, the file rewritten), streamed: the same bytes, one record in
    memory at a time. Every line is parsed first, so a file that does not parse is left as it is."""
    cache = index_path(chain_path)
    with open(cache, 'rb') as f:
        for text, _, _ in stream_lines(f):
            json.loads(text)
    tmp = cache.with_suffix(cache.suffix + '.tmp')
    with open(cache, 'rb') as f, open(tmp, 'w', encoding='utf-8') as out:
        for text, _, _ in stream_lines(f):
            rec = json.loads(text)
            if 'state' not in rec:
                seg = chain['segments'][rec['segment']]
                complete = Path(seg['root'], 'iterations', f"{rec['iteration']:06d}", 'complete.json')
                rec['state'] = state_pin(read(read(complete)['update']['path']))
            out.write(json.dumps(rec) + '\n')
    os.replace(tmp, cache)


class IndexCache:
    """`index` with the records kept across calls (`chains.py supervise --loop`): under the index lock, a refresh
    reads only the cache file's new lines and the receipts past them, and appends what it finds as `index` does, so
    it returns the same records. A cache file that was replaced or shortened, whose last line lacks its newline, or
    that does not end at the cursor, is read whole again (streamed, as `index` reads it) instead.

    The records are kept compactly (`IndexRecords`: line offsets and lengths and the scalar fields scans read) and
    returned as an `IndexView`, which parses a record from its line when it is read, so memory grows by about 56
    bytes a record rather than a parsed record's kilobyte or more. Each refresh reads the last cached line again: a
    line rewritten in place raises `IndexChanged` before the cycle decides anything from it.

    It also keeps what `plan` and `guard` need not look at again:
    - `settled`: boundaries every file of whose full state was found absent. Nothing writes into a completed
      iteration again, so they stay absent; an absent file is never a plan item or a guard count, and it cannot
      shadow an existing one in `plan`'s set of files surviving states use. `active` is the other records (a view),
      rebuilt whenever the records are read whole; boundaries are settled where they always were (`plan`, `guard`).
    - `complete`: segments found with completion.json (never removed).
    - `swept`: complete segments older than the trajectory window whose trajectories were found gone.

    `read_only` (a dry cycle): no lock, nothing written; records found past the file's end are kept parsed."""

    def __init__(self, chain_path, read_only=False):
        self.chain_path = Path(chain_path)
        self.read_only = read_only
        self.records, self.active, self.offset, self.ident = None, None, 0, None
        self.settled, self.complete, self.swept = Boundaries(), set(), set()

    def _stream(self, cache, store):
        """Reads the cache file into `store` one line at a time; False when a record lacks its S-container pin (the
        file needs `index`'s migration)."""
        with open(cache, 'rb') as f:
            for text, offset, length in stream_lines(f):
                rec = json.loads(text)
                if 'state' not in rec:
                    return False
                store.append(rec, offset, length)
        return True

    def _keep(self, view):
        """`active`: the view's records whose boundary is not settled (record numbers, so appends extend it)."""
        settled = self.settled
        return IndexView(view.store, array('q', (n for n, b in zip(view.numbers, view.column('boundary'))
                                                 if b not in settled)))

    def _whole(self):
        """Every record as `index` reads them (the lock is held), streamed; the cursor is then the file's end."""
        chain = load_chain(self.chain_path)
        cache = index_path(self.chain_path)
        store = IndexRecords(cache)
        if cache.exists() and not self._stream(cache, store):
            if self.read_only:
                raise SystemExit(f'stop: {cache} needs its S-container migration, which a read-only cache does not do')
            migrate(self.chain_path, chain)  # a cache written before S containers were tracked (once)
            return self._whole()
        new = scan(chain, len(store))
        if self.read_only:
            st = os.stat(cache) if cache.exists() else None
            self.offset, self.ident = (st.st_size, (st.st_dev, st.st_ino)) if st else (0, None)
            store.attach(self.ident if len(store) else None)
            for rec in new:
                store.append_unwritten(rec)
        else:
            if new:
                lines = [(json.dumps(rec) + '\n').encode('utf-8') for rec in new]
                with open(cache, 'ab') as f:
                    terminate(f)
                    f.flush()
                    at = os.fstat(f.fileno()).st_size
                    f.write(b''.join(lines))
                for rec, line in zip(new, lines):
                    store.append(rec, at, len(line) - 1)
                    at += len(line)
            if cache.exists():
                with open(cache, 'ab') as f:
                    terminate(f)
                    st = os.fstat(f.fileno())
                self.offset, self.ident = st.st_size, (st.st_dev, st.st_ino)
            else:
                self.offset, self.ident = 0, None
            store.attach(self.ident if len(store) else None)
        self.records = store
        self.active = self._keep(store.view())
        return chain, store.view()

    def refresh(self):
        """(chain, records), equal to `index(chain_path)` at the same moment (records: an `IndexView`)."""
        if self.read_only:
            return self._refresh()
        with index_lock(self.chain_path):
            return self._refresh()

    def _refresh(self):
        cache = index_path(self.chain_path)
        if self.records is None:
            return self._whole()
        try:
            f = open(cache, 'rb')
        except FileNotFoundError:
            return self._whole()
        with f:  # every fallback below runs with no handle open (a migration replaces the file)
            st = os.fstat(f.fileno())
            moved = (st.st_dev, st.st_ino) != self.ident or st.st_size < self.offset
            if not moved:
                f.seek(self.offset)
                data = f.read()
        if moved:
            return self._whole()  # replaced or shortened
        if data and not data.endswith(b'\n'):
            return self._whole()  # a last line without its newline: read as index() reads it
        written = len(self.records) - len(self.records.extra)
        if written:
            self.records.check(written - 1)  # the last cached line still reads as cached
        added = parse_bytes(data, self.offset)
        if any('state' not in r for r, _, _ in added):
            return self._whole()  # index() migrates the file first
        chain = load_chain(self.chain_path)
        new = scan(chain, len(self.records) + len(added))
        if new and not self.read_only:
            lines = [(json.dumps(r) + '\n').encode('utf-8') for r in new]
            with open(cache, 'ab') as f:
                before = os.fstat(f.fileno())
                moved = (before.st_dev, before.st_ino) != self.ident or before.st_size != self.offset + len(data)
                if not moved:
                    f.write(b''.join(lines))
                    f.flush()
            if moved:
                return self._whole()  # not the file just read (a writer outside the lock): append nothing
            at = self.offset + len(data)
            for rec, line in zip(new, lines):
                added.append((rec, at, len(line) - 1))
                at += len(line)
            data += b''.join(lines)
        if self.records.ident is None and added:  # the file's first records
            self.records.attach(self.ident)
        self.offset += len(data)
        for rec, offset, length in added:
            n = len(self.records)
            self.records.append(rec, offset, length)
            if rec['boundary'] not in self.settled:
                self.active.numbers.append(n)
        for rec in new if self.read_only else ():
            n = len(self.records)
            self.records.append_unwritten(rec)
            if rec['boundary'] not in self.settled:
                self.active.numbers.append(n)
        return chain, self.records.view()

    def settle(self, boundaries):
        """Records whose full-state files were all found absent leave `active`."""
        if boundaries:
            self.settled |= boundaries
            self.active = self._keep(self.active)


def keeps_full_state(arm, boundary, latest):
    """Whether boundary b keeps its full train state under the arm's strides."""
    if boundary % FULL_STRIDE:
        return False
    if arm == 'X':
        return True
    return boundary % C_LONG_STRIDE == 0 or boundary > latest - C_RECENT


def snapshot_dir(chain_path, chain):
    path = Path(chain_path)
    return Path(chain.get('snapshot_dir') or path.with_name(path.stem + '-snapshots'))


def snapshot_index_path(chain_path):
    return Path(chain_path).with_suffix('.snapshots.jsonl')


def snapshots(chain_path):
    """Weights-only snapshots by boundary (the last record of a boundary wins; files must still exist)."""
    path = snapshot_index_path(chain_path)
    out = {}
    if path.exists():
        for line in path.read_text(encoding='utf-8').splitlines():
            if line:
                rec = json.loads(line)
                out[rec['boundary']] = rec
    return {b: r for b, r in out.items() if Path(r['path']).exists()}


def snapshot(chain_path, binary, snapshot_from, max_new=None, cache=None):
    """Cuts a weights-only snapshot of every unpruned checkpoint at or past `snapshot_from` that keeps no full
    state and has none yet (the binary verifies the checkpoint against its pin), earliest first and at most
    `max_new` of them. Each snapshot must have been cut from the recorded after-state. With an `IndexCache` the
    records come from it (as `index` would return them) and only the boundaries are read until one qualifies."""
    chain, records = index(chain_path) if cache is None else cache.refresh()
    latest = records[-1]['boundary'] if records else 0
    have = snapshots(chain_path)
    directory = snapshot_dir(chain_path, chain)
    directory.mkdir(parents=True, exist_ok=True)
    made = []

    def walk():  # (boundary, its record when wanted), in order
        if isinstance(records, IndexView):
            for j, b in enumerate(records.column('boundary')):
                yield b, (lambda j=j: records[j])
        else:
            for r in records:
                yield r['boundary'], (lambda r=r: r)
    for b, record in walk():
        if max_new is not None and len(made) >= max_new:
            break
        if b < snapshot_from or b in have or keeps_full_state(chain['arm'], b, latest):
            continue
        rec = record()
        if not Path(rec['checkpoint']['path']).exists():
            continue
        out = directory / f'b{b:09d}.weights'
        if out.exists():
            raise SystemExit(f'stop: {out} exists without an index record')
        run = subprocess.run([binary, 'weights-snapshot', '--input', rec['checkpoint']['path'], '--sha256',
                              rec['checkpoint']['sha256'], '--output', out.resolve().as_posix()],
                             capture_output=True, text=True)
        if run.returncode != 0:
            raise SystemExit(f'stop: weights snapshot of boundary {b} failed: {run.stderr.strip()[-400:]}')
        receipt = json.loads(run.stdout)
        if receipt['cut_from_state_sha256'] != rec['after_state_sha256']:
            raise SystemExit(f'stop: the snapshot of boundary {b} was cut from another state')
        entry = {'boundary': b, 'path': Path(receipt['output']['path']).as_posix(),
                 'sha256': receipt['output']['sha256'], 'bytes': receipt['snapshot_bytes'],
                 'cut_from_state_sha256': receipt['cut_from_state_sha256'],
                 'parameters_sha256': receipt['parameters_sha256']}
        touch_segments([out, receipt['output']['path']])  # a snapshot directory inside a finished segment
        with open(snapshot_index_path(chain_path), 'a', encoding='utf-8') as f:
            f.write(json.dumps(entry) + '\n')
        made.append(entry)
    return made


def probe_snapshot_bytes(records, binary):
    """The size of a weights-only snapshot of the latest checkpoint, cut to a temporary file and deleted."""
    if binary is None:
        return None
    for rec in reversed(records):
        path = Path(rec['checkpoint']['path'])
        if not path.exists() or sha(path) != rec['checkpoint']['sha256']:
            continue
        seg = next((q for q in path.parents if SEGMENT_DIR.fullmatch(q.name)), None)
        # Beside the segment, never inside it: a finished segment gains no file (ByteCounter).
        with tempfile.TemporaryDirectory(dir=seg.parent if seg is not None else path.parent) as tmp:
            out = Path(tmp).resolve() / 'probe.weights'
            run = subprocess.run([binary, 'weights-snapshot', '--input', path.as_posix(), '--sha256',
                                  rec['checkpoint']['sha256'], '--output', out.as_posix()],
                                 capture_output=True, text=True)
            if run.returncode == 0:
                return json.loads(run.stdout)['snapshot_bytes']
    return None


def plan(chain_path, snapshot_from=None, cache=None):
    """Rolling prune plan. A checkpoint is pruned when its boundary keeps no full state, its segment and the next
    segment are complete, it is not the latest, and, inside the snapshot window (boundary at or past
    `snapshot_from`), its weights-only snapshot exists. Trajectories of complete segments older than the 64 most
    recent are pruned. With an `IndexCache` the same plan reads only its active records and unswept segments."""
    chain, records = index(chain_path) if cache is None else cache.refresh()
    candidates = records if cache is None else cache.active
    have = snapshots(chain_path)
    # Each boundary's after-state hash (the last candidate of a boundary wins); `verified` asks only inside the
    # snapshot window, so only those boundaries are kept.
    after = {}
    if snapshot_from is not None:
        for r in candidates:
            if r['boundary'] >= snapshot_from:
                after[r['boundary']] = r['after_state_sha256']

    def verified(b):
        snap = have.get(b)
        return bool(snap) and snap['cut_from_state_sha256'] == after.get(b) and sha(snap['path']) == snap['sha256']
    latest = records[-1]['boundary'] if records else 0
    segs = chain['segments']
    if cache is None:
        complete = [segment_complete(s) for s in segs]
    else:
        cache.complete |= {i for i, s in enumerate(segs) if i not in cache.complete and segment_complete(s)}
        complete = [i in cache.complete for i in range(len(segs))]
    checkpoints, trajectories = [], []
    # One pass over the candidates (a cache's are parsed one at a time) finds the prunable ones and their
    # boundaries; the others' files survive unless their boundary is pruned too. A cache's are kept by position.
    view = isinstance(candidates, IndexView)
    prunable, pruned, others = (array('q'), Boundaries(), array('q')) if view else ([], set(), [])
    for j, rec in enumerate(candidates):
        s = rec['segment']
        keep = keeps_full_state(chain['arm'], rec['boundary'], latest) or rec['boundary'] == latest or (
            snapshot_from is not None and rec['boundary'] >= snapshot_from and not verified(rec['boundary']))
        if keep or not (complete[s] and s + 1 < len(segs) and complete[s + 1]):
            others.append(j if view else rec)
            continue
        prunable.append(j if view else rec)
        pruned.add(rec['boundary'])
    surviving = {Path(p['path']).as_posix() for r in (candidates.pick(others) if view else others)
                 if r['boundary'] not in pruned for p in record_parts(r)}
    seen, gone = set(), []
    for rec in candidates.pick(prunable) if view else prunable:
        for part in record_parts(rec):
            path = Path(part['path'])
            if part['path'] in surviving or part['path'] in seen or not path.exists():
                continue
            seen.add(part['path'])
            item = {'boundary': rec['boundary'], 'path': path.as_posix(), 'sha256': part['sha256'],
                    'bytes': path.stat().st_size}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            checkpoints.append(item)
        if cache is not None and not state_present_any(rec['checkpoint'], rec.get('state')):
            gone.append(rec['boundary'])
    done = [i for i, c in enumerate(complete) if c]
    for s in done[:-TRAJECTORY_SEGMENTS] if len(done) > TRAJECTORY_SEGMENTS else []:
        if cache is not None and s in cache.swept:
            continue
        found = sorted(p for pattern in TRAJECTORY_GLOBS
                       for p in Path(segs[s]['root'], 'iterations').glob(pattern))
        for path in found:
            trajectories.append({'segment': s, 'path': path.as_posix(), 'bytes': path.stat().st_size})
        if cache is not None and not found:
            cache.swept.add(s)
    if cache is not None:
        cache.settle(gone)
    return {'schema': 'w5-retention-plan/v1', 'chain': chain['chain'], 'arm': chain['arm'], 'latest': latest,
            'snapshot_from': snapshot_from, 'checkpoints': checkpoints, 'trajectories': trajectories,
            'bytes': sum(c['bytes'] for c in checkpoints) + sum(t['bytes'] for t in trajectories)}


def apply(plan_doc, log_path, hold=None):
    """Deletes (or moves under `hold`, same volume) every planned file, appending one log line per file.
    Checkpoints are regenerable by replay against their recorded hashes; trajectories by recollection."""
    done, touched = 0, []
    try:
        with open(log_path, 'a', encoding='utf-8') as log:
            for kind in ('checkpoints', 'trajectories'):
                for item in plan_doc[kind]:
                    path = Path(item['path'])
                    if not path.exists():
                        continue
                    touched.append(path)
                    if hold:
                        dst = Path(hold) / path.relative_to(path.anchor)
                        dst.parent.mkdir(parents=True, exist_ok=True)
                        os.replace(path, dst)
                        touched.append(dst)  # a hold folder inside a finished segment gains the file
                        item = dict(item, moved_to=dst.as_posix())
                    else:
                        path.unlink()
                    log.write(json.dumps(dict(item, kind=kind, unix_seconds=time.time())) + '\n')
                    done += 1
    finally:
        touch_segments(touched + [log_path])  # a finished segment that lost or gained a file is counted again
    return done


def ledger_series(records):
    columns = fields(records, 'unix_seconds', 'ledger_usec')
    if columns is not None:  # the same pairs from an IndexCache view's compact fields
        return [(t, c) for t, c in zip(*columns) if t is not None and c is not None]
    return [(r['unix_seconds'], r['ledger_usec']) for r in records
            if r.get('unix_seconds') is not None and r.get('ledger_usec') is not None]


def project(c_records, launched, now=None, horizon=HORIZON):
    """R-hat_i(t) = CPU(t) + rate(last 24 h, or since launch if shorter) x (7 days - t), in microseconds (the
    horizon is the C wall: 7 days in Stage 1, a Stage 0 qualification chain's own wall)."""
    series = ledger_series(c_records)
    if not series:
        raise SystemExit('stop: the C chain has no ledger records')
    t_now, cpu = series[-1]
    now = t_now if now is None else now
    elapsed = now - launched
    window_start = max(launched, t_now - DAY)
    base = [(t, c) for t, c in series if t <= window_start]
    t0, c0 = base[-1] if base and window_start > launched else (launched, 0)
    rate = (cpu - c0) / (t_now - t0) if t_now > t0 else 0.0
    rhat = cpu + rate * max(0.0, horizon - elapsed)
    return {'cpu_usec': cpu, 'rate_usec_per_s': rate, 'elapsed_s': elapsed, 'r_hat_usec': rhat,
            'window_s': t_now - t0}


def first_at_or_past(records, r_usec):
    columns = fields(records, 'ledger_usec')
    if columns is not None:  # an IndexCache view: only the record found is parsed
        for j, v in enumerate(columns[0]):
            if v is not None and v >= r_usec:
                return records[j]
        return None
    for rec in records:
        if rec.get('ledger_usec') is not None and rec['ledger_usec'] >= r_usec:
            return rec
    return None


def decide(x_chain, x_records, c_chain, c_records, c_ended, x_running_s=None, now=None,
           snapshot_bytes=None):
    """The pair's next action for X: run, snapshot (cut a weights-only snapshot of every update from now),
    pause, resume_to (R_i), select, or stop_under_budget (X cap). `snapshot_bytes` is the measured size of one
    weights-only snapshot. Ledger values only; never R-hat for recoverability."""
    x_ledger = x_records[-1]['ledger_usec'] if x_records else 0
    x_cap = float(x_chain.get('x_cap_seconds', X_CAP_SECONDS))
    out = {'x_ledger_usec': x_ledger, 'x_boundary': x_records[-1]['boundary'] if x_records else 0,
           'x_cap_seconds': x_cap}
    if c_ended:
        r = c_records[-1]['ledger_usec']
        out['r_usec'] = r
        hit = first_at_or_past(x_records, r)
        if hit:
            return dict(out, action='select', endpoint_boundary=hit['boundary'])
        if x_running_s is not None and x_running_s >= x_cap:
            return dict(out, action='stop_under_budget')
        return dict(out, action='resume_to', stop_at_usec=r)
    horizon = float(c_chain.get('c_wall_seconds') or HORIZON)
    proj = project(c_records, c_chain['launched_unix'], now, horizon)
    out['projection'] = proj
    rhat = proj['r_hat_usec']
    if x_running_s is not None and x_running_s >= x_cap:
        return dict(out, action='stop_under_budget')
    if x_ledger >= PAUSE_AT * rhat:
        return dict(out, action='pause')
    if x_ledger >= SNAPSHOT_TRIGGER * rhat:
        recent = x_records[-65:]
        per_update = ((recent[-1]['ledger_usec'] - recent[0]['ledger_usec']) / (len(recent) - 1)
                      if len(recent) > 1 else None)
        if per_update and snapshot_bytes:
            # Whole boundaries, both ends of the window included.
            projected = math.ceil((PAUSE_AT - SNAPSHOT_TRIGGER) * rhat / per_update) + 1
            fits = snapshot_bytes * projected <= SNAPSHOT_CAP_BYTES
        else:
            projected, fits = None, False
        return dict(out, action='snapshot' if fits else 'run', snapshot_window_updates=projected,
                    snapshot_fits=fits)
    return dict(out, action='run')


def select(chain, records, r_usec, weights=None):
    """The endpoint (first boundary at or past R): its checkpoint if still on disk, else its weights-only snapshot
    (`weights`: boundary to snapshot record), else the replay spec from the nearest kept full state at or before
    it."""
    hit = first_at_or_past(records, r_usec)
    if not hit:
        raise SystemExit('stop: no boundary reaches R yet')
    endpoint = hit['boundary']
    by_boundary = records.lookup('boundary') if isinstance(records, IndexView) else {r['boundary']: r for r in records}
    if state_intact(hit['checkpoint'], hit.get('state')):
        return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'], 'replay': None}
    snap = (weights or {}).get(endpoint)
    if snap and sha(snap['path']) == snap['sha256'] and snap['cut_from_state_sha256'] == hit['after_state_sha256']:
        return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'],
                'weights_snapshot': {k: snap[k] for k in ('path', 'sha256', 'parameters_sha256')}, 'replay': None}
    start = None
    for b in range(endpoint - 1, -1, -1):
        if b == 0:
            start = {'boundary': 0, 'checkpoint': None}
            break
        rec = by_boundary.get(b)
        if rec and state_present(rec['checkpoint'], rec.get('state')):
            start = {'boundary': b, 'checkpoint': rec['checkpoint']}
            break
    expected = [{'boundary': b, 'after_state_sha256': by_boundary[b]['after_state_sha256']}
                for b in range(start['boundary'] + 1, endpoint + 1)]
    return {'endpoint_boundary': endpoint, 'r_usec': r_usec, 'checkpoint': hit['checkpoint'],
            'replay': {'chain': chain['chain'], 'from': start, 'updates': len(expected), 'expected': expected}}


def replay_config(chain, spec, out_dir):
    """A segment config that replays boundaries from+1..endpoint: the chain's own iterations, the kept state as
    its source, the global update offset of its first update, and no ledger. Opponent assignments that name the
    run's own history (`initial`, `completed_iteration`) resolve against a segment's source and iteration
    indices, so a replay that slices them is refused; every segment crossed must share one opponent roster and
    one source import."""
    replay = spec['replay']
    first = replay['from']['boundary']
    last = spec['endpoint_boundary']
    iterations, used = [], []
    for seg in chain['segments']:
        cfg = read_config(seg['config'])
        for i, item in enumerate(cfg['iterations']):
            if first <= seg['offset'] + i < last:
                for episode in item['episodes']:
                    if episode['opponent'].get('kind') in HISTORY_KINDS:
                        raise SystemExit('stop: the replay range uses a history-relative opponent '
                                         f"({episode['opponent']['kind']}); replay its whole segment instead")
                iterations.append(item)
                if not used or used[-1][1] is not seg:
                    used.append((cfg, seg))
    if len(iterations) != last - first or not 1 <= len(iterations) <= 4096:
        raise SystemExit('stop: the replay range is not covered by the chain segments, or exceeds 4096')
    cfg = json.loads(json.dumps(used[0][0]))
    for other, _ in used[1:]:
        if other['opponents'] != cfg['opponents'] or {k: v for k, v in other['initial_source'].items()
                                                       if k != 'checkpoint'} != {
                k: v for k, v in cfg['initial_source'].items() if k != 'checkpoint'}:
            raise SystemExit('stop: the replay crosses segments with different rosters or source imports')
    source = dict(cfg['initial_source'])
    if replay['from']['checkpoint'] is not None:
        source['checkpoint'] = replay['from']['checkpoint']
    elif first != 0:
        raise SystemExit('stop: a replay from a nonzero boundary needs its kept checkpoint')
    else:
        source = dict(read_config(chain['segments'][0]['config'])['initial_source'])
    cfg['initial_source'] = source
    cfg['iterations'] = iterations
    if cfg.get('ex1'):
        cfg['ex1']['update_offset'] = first
    cfg.pop('chain_ledger', None)
    cfg['output_directory'] = Path(out_dir).as_posix()
    return cfg


def verify_replay(spec, replay_root):
    """Every replayed update must reproduce its recorded after-state hash."""
    expected = spec['replay']['expected']
    got, checkpoint, state = [], None, None
    for i, item in enumerate(expected):
        complete = Path(replay_root, 'iterations', f'{i:06d}', 'complete.json')
        doc = read(read(complete)['update']['path']) if complete.exists() else {}
        got.append(doc.get('after_state_sha256'))
        checkpoint = doc.get('checkpoint')
        state = state_pin(doc)
    equal = [g == e['after_state_sha256'] for g, e in zip(got, expected)]
    ok = all(equal) and len(got) == len(expected)
    if ok and not (checkpoint and state_intact(checkpoint, state)):
        ok = False
    return {'schema': 'w5-replay-receipt/v1', 'endpoint_boundary': spec['endpoint_boundary'],
            'from_boundary': spec['replay']['from']['boundary'], 'updates': len(expected),
            'equal': ok, 'first_mismatch': None if all(equal) else expected[equal.index(False)]['boundary'],
            'endpoint_after_state_sha256': got[-1] if got else None,
            'endpoint_checkpoint': checkpoint if ok else None, 'endpoint_state': state if ok else None}


def prune_manifest(chain, records, selected_boundary, replay_receipt=None, weights=None):
    """ARTIFACT-LAW PRUNE manifest after the pair's endpoint is fixed: every full state and every weights-only
    snapshot on disk except the selected endpoint's; receipts (complete.json, update.json, ledgers, the snapshot
    index) are kept. The endpoint is kept as its full state when one is on disk, else its weights-only snapshot,
    else its verified replay (the replayed checkpoint, outside the chain's trees)."""
    prune, keep = [], []
    weights = weights or {}
    if replay_receipt is not None:
        if not (replay_receipt.get('equal') and replay_receipt.get('endpoint_boundary') == selected_boundary
                and replay_receipt.get('endpoint_checkpoint')):
            raise SystemExit('stop: the replay receipt is not a verified replay of the selected boundary')
        pin = replay_receipt['endpoint_checkpoint']
        record = next((r for r in records if r['boundary'] == selected_boundary), None)
        if record is None or replay_receipt.get('endpoint_after_state_sha256') != record['after_state_sha256']:
            raise SystemExit("stop: the replayed endpoint's state is not this chain's recorded state")
        replay_state = replay_receipt.get('endpoint_state')
        # An S chain's replayed endpoint is two files: a receipt without the container's pin (one written
        # before containers were tracked) must be re-verified first.
        if record.get('state') is not None and replay_state is None:
            raise SystemExit("stop: the replay receipt lacks the S endpoint's container; rerun verify-replay")
        if not state_intact(pin, replay_state):
            raise SystemExit('stop: the replayed checkpoint differs from its pin')
        for part in checkpoint_parts(pin, replay_state):
            item = {'boundary': selected_boundary, 'kind': 'full', 'path': part['path'], 'sha256': part['sha256'],
                    'bytes': Path(part['path']).stat().st_size, 'replayed': True}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            keep.append(item)
    full_prune = []
    for rec in records:
        parts = [p for p in record_parts(rec) if Path(p['path']).exists()]
        if not parts:
            continue
        # The endpoint's full state is kept only when every file still matches its pin (as selection requires).
        intact = rec['boundary'] == selected_boundary and not keep and state_intact(rec['checkpoint'],
                                                                                    rec.get('state'))
        for part in parts:
            item = {'boundary': rec['boundary'], 'kind': 'full', 'path': part['path'], 'sha256': part['sha256'],
                    'bytes': Path(part['path']).stat().st_size}
            if part['part'] != 'checkpoint':
                item['part'] = part['part']
            (keep if intact else full_prune).append(item)
    # A file the kept endpoint uses is never pruned, and no file is listed twice.
    kept_paths, seen = {k['path'] for k in keep}, set()
    for item in full_prune:
        if item['path'] not in kept_paths and item['path'] not in seen:
            seen.add(item['path'])
            prune.append(item)
    for b, snap in sorted(weights.items()):
        item = {'boundary': b, 'kind': 'weights', 'path': snap['path'], 'sha256': snap['sha256'],
                'bytes': Path(snap['path']).stat().st_size}
        if b == selected_boundary and not keep:
            record = next((r for r in records if r['boundary'] == b), None)
            if record is None or snap['cut_from_state_sha256'] != record['after_state_sha256'] or \
                    sha(snap['path']) != snap['sha256']:
                raise SystemExit("stop: the selected snapshot is not this chain's recorded endpoint")
            keep.append(item)
        else:
            prune.append(item)
    if not keep and selected_boundary != 0:
        raise SystemExit('stop: the selected endpoint is not on disk (replay it first)')
    by_kind = {k: sum(p['bytes'] for p in prune if p['kind'] == k) for k in ('full', 'weights')}
    return {'schema': 'artifact-prune-manifest/v1', 'lane': 'w5', 'chain': chain['chain'], 'arm': chain['arm'],
            'reason': 'W5 section 2.2: the pair endpoint is fixed; non-selected full states and snapshots pruned',
            'keep': {'selected_checkpoint': keep,
                     'receipts': 'every complete.json, update.json, ledger and the snapshot index'},
            'prune': prune, 'prune_bytes': sum(p['bytes'] for p in prune), 'prune_bytes_by_kind': by_kind}


def tree_bytes(root):
    total = 0
    for dirpath, _, names in os.walk(root):
        for name in names:
            try:
                total += os.path.getsize(os.path.join(dirpath, name))
            except OSError:
                pass
    return total


SEGMENT_DIR = re.compile(r'seg-\d+')
TOUCH_STEP_NS = 2 * 10**9  # survives any timestamp resolution (NTFS 100 ns, coarse kernel ticks, FAT 2 s)
SETTLE_SECONDS = 10.0  # a reused total is from a count made this long after the directory last changed
FULL_RECOUNT_SECONDS = 6 * 3600


def segment_key(path):
    return os.path.normcase(os.path.abspath(path))


def touch_segments(paths):
    """Moves the modification time of every `seg-<n>` directory above one of `paths` forward (each such ancestor,
    along the path as written and as resolved through links), so a ByteCounter counts it again. Every W5 tool that
    deletes, moves or writes a file under a W5 root calls this for the paths it touches: retention.apply (sources,
    hold destinations and its log, the standalone `retention.py apply` included), snapshots and the removal of
    orphan snapshots, chains.py's hold moves."""
    segments = set()
    for p in paths:
        p = Path(os.path.abspath(p))
        for form in (p, Path(os.path.realpath(p.parent)) / p.name, Path(os.path.realpath(p))):
            segments |= {q for q in form.parents if SEGMENT_DIR.fullmatch(q.name)}
    for seg in segments:
        try:
            before = seg.stat().st_mtime_ns
            stamp = max(time.time_ns(), before + TOUCH_STEP_NS)
            os.utime(seg, ns=(stamp, stamp))
        except OSError:
            pass


class ByteCounter:
    """`tree_bytes` for a long-lived caller (`chains.py supervise --loop`) that is not linear in the files of
    finished chain segments.

    **Precondition** (checked for a plan's own output locations by `chains.counter_safe`, which otherwise makes the
    cycle walk everything): a sealed segment, `<root>/chains/<chain>/seg-<n>` (a directory, not a link, holding
    completion.json), receives files only from the native trainer before completion.json appears; afterwards files
    only leave it, and only through the W5 tools, which move the segment directory's modification time forward
    (`touch_segments`). Nothing writes into it by hand.

    Under it a segment's total is reused only while its directory's modification time is the one seen by the two
    consecutive counts that last counted it, the second made SETTLE_SECONDS after that time (so a publisher's
    same-tick cleanup is in it); otherwise it is counted again. Everything else, other `seg-<n>` directories
    included, is walked every call. A call then returns `sum(tree_bytes(r) for r in roots)`. A hand edit inside a
    finished segment breaks the precondition: every FULL_RECOUNT_SECONDS a call recounts every sealed segment and
    records any segment whose total changed without a touch (`mismatches`)."""

    def __init__(self, full_seconds=FULL_RECOUNT_SECONDS):
        self.sealed = {}  # key -> {'mtime': ns, 'bytes': n, 'stable': bool}
        self.full_seconds = full_seconds
        self.last_full = None
        self.mismatches = []
        self.links = []

    def invalidate(self, paths):
        """Paths the caller deleted or moved: their sealed segments are counted again on the next calls."""
        for p in paths:
            for parent in Path(os.path.abspath(p)).parents:
                if self.sealed.pop(segment_key(parent), None) is not None:
                    break

    def segment(self, path, full, now):
        key = segment_key(path)
        mtime = os.stat(path).st_mtime_ns
        have = self.sealed.get(key)
        if have is not None and have['mtime'] == mtime and have['stable'] and not full:
            return key, have['bytes']
        size = tree_bytes(path)
        if full and have is not None and have['stable'] and have['mtime'] == mtime and have['bytes'] != size:
            self.mismatches.append({'segment': key, 'cached': have['bytes'], 'counted': size, 'unix': round(now, 3)})
        # Stable: the same modification time at two counts, the second made SETTLE_SECONDS after it (a publisher's
        # cleanup in the tick of its link is long done by then; one held longer changes the time when it resumes).
        self.sealed[key] = {'mtime': mtime, 'bytes': size,
                            'stable': have is not None and have['mtime'] == mtime and os.stat(path).st_mtime_ns ==
                            mtime and now - mtime / 1e9 >= SETTLE_SECONDS}
        return key, size

    def total(self, roots, now=None):
        """The bytes under `roots`, as `tree_bytes` counts them. A link outside the sealed segments that resolves into
        a chain segment (a way to write into one without touching it) makes the call count every segment
        (`links` records it)."""
        now = time.time() if now is None else now
        full = self.last_full is None or now - self.last_full >= self.full_seconds
        result = self._total(roots, now, full)
        if result is None:
            self.links_unix = now
            result = self._total(roots, now, True, record=False)
        return result

    def _total(self, roots, now, full, record=True):
        seen, total = set(), 0
        chains_dirs = [Path(os.path.realpath(Path(r) / 'chains')) for r in roots]

        def into_segment(path):
            real = Path(os.path.realpath(path))
            return any(q.parent.parent in chains_dirs and SEGMENT_DIR.fullmatch(q.name) for q in [real, *real.parents])
        for root in roots:
            for dirpath, dirnames, names in os.walk(root):
                for name in names:
                    file = os.path.join(dirpath, name)
                    if record and not full and os.path.islink(file) and into_segment(file):
                        self.links.append({'link': file, 'unix': round(now, 3)})
                        return None
                    try:
                        total += os.path.getsize(file)
                    except OSError:
                        pass
                walk = []
                for d in dirnames:
                    path = os.path.join(dirpath, d)
                    if SEGMENT_DIR.fullmatch(d) and Path(dirpath).parent.name == 'chains' and \
                            Path(dirpath).parent.parent == Path(root) and not os.path.islink(path) and \
                            os.path.exists(os.path.join(path, 'completion.json')):
                        try:
                            key, size = self.segment(path, full, now)
                        except OSError:  # removed while counted
                            continue
                        seen.add(key)
                        total += size
                    else:
                        if record and not full and os.path.islink(path) and into_segment(path):
                            self.links.append({'link': path, 'unix': round(now, 3)})
                            return None
                        walk.append(d)
                dirnames[:] = walk
        for key in [k for k in self.sealed if k not in seen]:  # removed segments
            del self.sealed[key]
        if full:
            self.last_full = now
        return total


def under(path, roots):
    path = Path(path).resolve()
    return any(path == Path(r).resolve() or Path(r).resolve() in path.parents for r in roots)


def guard(roots, cap_bytes, next_bytes=0, chains=(), caches=None, counter=None):
    """The per-node byte guard: refuses (exit 1) when the W5 trees plus the next segment would pass the cap. For
    each chain manifest given, its full states and weights-only snapshots on disk are reported by kind, and every
    one of them must lie under a guarded root (a snapshot directory outside the roots would escape the cap).
    `caches` (manifest path to `IndexCache`) gives the same report from the chains' active records, and `counter`
    (a `ByteCounter`) the same used bytes without walking finished segments again."""
    used = sum(tree_bytes(r) for r in roots) if counter is None else counter.total(roots)
    kinds, escaped = [], []
    for chain_path in chains:
        cache = (caches or {}).get(Path(chain_path).as_posix())
        if cache is None:
            chain, records = index(chain_path)
        else:
            chain, _ = cache.refresh()
            records = cache.active
            cache.settle([r['boundary'] for r in records if not state_present_any(r['checkpoint'], r.get('state'))])
            records = cache.active
        full = sorted({Path(p['path']) for r in records for p in record_parts(r) if Path(p['path']).exists()})
        weights = [Path(s['path']) for s in snapshots(chain_path).values()]
        # Where the chain will write (segment roots and its snapshot directory), checked before anything exists.
        destinations = [Path(seg['root']) for seg in chain['segments']] + [snapshot_dir(chain_path, chain)]
        escaped += [p.as_posix() for p in destinations + full + weights if not under(p, roots)]
        kinds.append({'chain': chain['chain'], 'full_state_bytes': sum(p.stat().st_size for p in full),
                      'full_states': len(full), 'weights_bytes': sum(p.stat().st_size for p in weights),
                      'weights_snapshots': len(weights)})
    return {'schema': 'w5-byte-guard/v1', 'roots': [Path(r).as_posix() for r in roots], 'used_bytes': used,
            'next_bytes': next_bytes, 'cap_bytes': cap_bytes, 'by_chain': kinds, 'outside_roots': escaped[:20],
            'ok': used + next_bytes <= cap_bytes and not escaped}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('index'); p.add_argument('chain')
    p = sub.add_parser('snapshot'); p.add_argument('chain'); p.add_argument('binary')
    p.add_argument('--from', dest='snapshot_from', type=int, required=True)
    p = sub.add_parser('plan'); p.add_argument('chain'); p.add_argument('--snapshot-from', type=int)
    p = sub.add_parser('apply'); p.add_argument('plan'); p.add_argument('--log', required=True); p.add_argument('--hold')
    p = sub.add_parser('project'); p.add_argument('chain'); p.add_argument('--now', type=float)
    p = sub.add_parser('decide')
    p.add_argument('x_chain'); p.add_argument('c_chain'); p.add_argument('--c-ended', action='store_true')
    p.add_argument('--x-running-seconds', type=float); p.add_argument('--now', type=float)
    p.add_argument('--binary')
    p = sub.add_parser('select'); p.add_argument('chain'); p.add_argument('r_usec', type=int)
    p = sub.add_parser('replay-config')
    p.add_argument('chain'); p.add_argument('spec'); p.add_argument('out'); p.add_argument('output_dir')
    p = sub.add_parser('verify-replay'); p.add_argument('spec'); p.add_argument('replay_root')
    p = sub.add_parser('prune-manifest'); p.add_argument('chain'); p.add_argument('boundary', type=int); p.add_argument('out')
    p.add_argument('--replay-receipt')
    p = sub.add_parser('guard'); p.add_argument('roots', nargs='+'); p.add_argument('--cap-bytes', type=int, required=True)
    p.add_argument('--next-bytes', type=int, default=0)
    p.add_argument('--chain', dest='chains', action='append', default=[])
    a = ap.parse_args(argv)
    if a.cmd == 'index':
        _, records = index(a.chain)
        out = {'updates': len(records), 'latest_boundary': records[-1]['boundary'] if records else 0}
    elif a.cmd == 'snapshot':
        made = snapshot(a.chain, a.binary, a.snapshot_from)
        out = {'snapshots': len(made), 'bytes': sum(m['bytes'] for m in made)}
    elif a.cmd == 'plan':
        out = plan(a.chain, a.snapshot_from)
    elif a.cmd == 'apply':
        out = {'removed': apply(read(a.plan), a.log, a.hold)}
    elif a.cmd == 'project':
        chain, records = index(a.chain)
        out = project(records, chain['launched_unix'], a.now, float(chain.get('c_wall_seconds') or HORIZON))
    elif a.cmd == 'decide':
        x_chain, x_records = index(a.x_chain)
        c_chain, c_records = index(a.c_chain)
        # A measured snapshot: one already cut, or a probe cut from the latest checkpoint and deleted. Without
        # either the window is not admitted (the endpoint is rebuilt by replay).
        made = snapshots(a.x_chain)
        size, measured = (max(s['bytes'] for s in made.values()), True) if made else (None, False)
        out = decide(x_chain, x_records, c_chain, c_records, a.c_ended, a.x_running_seconds, a.now, size)
        if size is None and out['action'] == 'run' and 'snapshot_fits' in out and a.binary:
            # Only the snapshot admission needs a size: probe now, then decide again.
            size = probe_snapshot_bytes(x_records, a.binary)
            measured = size is not None
            out = decide(x_chain, x_records, c_chain, c_records, a.c_ended, a.x_running_seconds, a.now, size)
        out['snapshot_bytes'] = size
        out['snapshot_bytes_measured'] = measured
    elif a.cmd == 'select':
        chain, records = index(a.chain)
        out = select(chain, records, a.r_usec, snapshots(a.chain))
    elif a.cmd == 'replay-config':
        chain = load_chain(a.chain)
        write(a.out, replay_config(chain, read(a.spec), a.output_dir))
        out = {'config': Path(a.out).as_posix()}
    elif a.cmd == 'verify-replay':
        out = verify_replay(read(a.spec), a.replay_root)
    elif a.cmd == 'prune-manifest':
        chain, records = index(a.chain)
        out = prune_manifest(chain, records, a.boundary, read(a.replay_receipt) if a.replay_receipt else None,
                             snapshots(a.chain))
        write(a.out, out)
    else:
        out = guard(a.roots, a.cap_bytes, a.next_bytes, a.chains)
    print(json.dumps(out, indent=1))
    ok = out.get('ok', True) and out.get('equal', True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
