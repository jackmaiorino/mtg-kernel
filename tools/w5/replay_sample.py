"""W5 label-blind replay sample (collab LANES/lead/W5-DESIGN.md v1.3 section 7.5.6 and section 9; the lead's
ruling on 7.5.6 of 2026-11-05).

The sample keeps 1 in 16 X updates for offline refits of S, M and L. The rules (the ruling's):
- **Arms:** X8 and XS chains of Stage 1 (1a and 1b). C is not sampled. Stage 0 qualification chains are not sampled.
- **Selection (label-blind):** update u of a chain with seed root R is selected when
  SHA256(json(["w5/replay-sample", R, u])) < 2^256 / 16, with compact UTF-8 JSON as `xact.hash_u64` writes it and the
  digest read big-endian (its first hex digit is 0). It is keyed on the seed root, so X8 chain i and XS chain i
  select the same updates, and it never reads an outcome.
- **Records:** every `collect/episode-*.json` trajectory and `collect/ex1-*.json` search-target sidecar of the
  update's completed attempt, and its small receipts (`complete.json`, `collect/collection.json`,
  `update/update.json`).
- **Compression:** zstd (level 3), one frame per file; every frame is decoded and checked against the raw file's
  SHA-256 when it is staged.
- **Budget:** each arm keeps at most 50 GB of frames on E:, 100 GB in all. When an arm's quota binds, the arm keeps
  the updates with the smallest selection hashes: the longest run of its offered updates, in hash order, whose
  frames fit the quota. Updates past it are evicted or refused (a cutoff hash is recorded), so the kept set does
  not depend on the order in which nodes ship. The 1-in-16 rate is never changed.

**Staging** (`stage`, run by `chains.py supervise` every cycle before rolling retention): each selected update of
a sampled chain is compressed into `<root>/replay-sample/<arm>/<chain>/u<update>/` (inside the node's W5 root, so
the byte guard counts it) with `ENTRY.json` written last; `PROGRESS.json` per chain is the watermark of updates
already considered. Files of a selected update not yet staged are protected from rolling deletion
(`protected`). An update whose files were already gone is recorded as missing and never staged.

**Transfer** (`ship`, run where the node's staging directory and E: are both mounted): under a store lock, each
staged entry is copied to `<store>/<arm>/<chain>/u<update>/`, every frame's SHA-256 checked at the destination,
recorded in `<store>/MANIFEST.jsonl` (hash-chained), the quota applied, and only then is the staged copy deleted.
Evicted frames are deleted with a manifest line (trajectories are regenerable by deterministic replay from the
chain's kept full states).

  replay_sample.py selected SEED_ROOT UPDATE
  replay_sample.py ship --staging DIR --store DIR [--arm-quota-bytes N] [--total-cap-bytes N]
  replay_sample.py status --store DIR
  replay_sample.py verify --store DIR               every kept frame against its recorded hash (and decoded)
"""
import argparse
import contextlib
import hashlib
import json
import os
import shutil
import sys
import time
from pathlib import Path

TAG = 'w5/replay-sample'
RATE_DENOMINATOR = 16
SAMPLED_ARMS = ('X8', 'XS')
SAMPLED_STAGES = ('1a', '1b')
ZSTD_LEVEL = 3
ARM_QUOTA_BYTES = 50 * 10**9
TOTAL_CAP_BYTES = 100 * 10**9
ENTRY_SCHEMA = 'w5-replay-sample-entry/v1'
MANIFEST_SCHEMA = 'w5-replay-sample-manifest/v1'
STAGING = 'replay-sample'
FRAME_OVERHEAD = 1.01  # a zstd frame of incompressible data is at most a little larger than its input


class SampleError(Exception):
    pass


def require(ok, message):
    if not ok:
        raise SampleError(message)


# ---------------------------------------------------------------- selection

def selection_hash(seed_root, update):
    text = json.dumps([TAG, seed_root, int(update)], separators=(',', ':'), ensure_ascii=False)
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def selected(digest):
    """The digest, read as a 256-bit integer, is below 2^256 / 16."""
    return int(digest, 16) < (1 << 256) // RATE_DENOMINATOR


def sampled(stage, arm):
    return stage in SAMPLED_STAGES and arm in SAMPLED_ARMS


# ---------------------------------------------------------------- zstd and files

def zstd_api():
    """(library, compress(data), decompress(frame)) from compression.zstd (Python 3.14) or the zstandard package."""
    try:
        from compression import zstd as z
        return 'compression.zstd', lambda data: z.compress(data, level=ZSTD_LEVEL), z.decompress
    except ImportError:
        pass
    try:
        import zstandard as z
    except ImportError:
        raise SampleError('the replay sample needs zstd: Python 3.14 (compression.zstd) or the zstandard package')
    return ('zstandard', lambda data: z.ZstdCompressor(level=ZSTD_LEVEL).compress(data),
            lambda frame: z.ZstdDecompressor().decompress(frame, max_output_size=1 << 34))


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def write_bytes(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f'{path.name}.{os.getpid()}.{time.time_ns()}.tmp')
    with open(tmp, 'wb') as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def write_json(path, doc):
    write_bytes(path, (json.dumps(doc, indent=1, sort_keys=True) + '\n').encode('utf-8'))


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def update_files(seg_root, iteration):
    """(relative path, absolute path) of the update's sampled files, from its completed attempt; None if its receipt
    is gone."""
    d = Path(seg_root) / 'iterations' / f'{iteration:06d}'
    complete = d / 'complete.json'
    if not complete.exists():
        return None
    receipt = read_json(complete)
    update_json = Path(receipt['update']['path'])
    attempt = update_json.parent.parent
    files = [('complete.json', complete), (update_json.relative_to(d).as_posix(), update_json)]
    collect = attempt / 'collect'
    for path in sorted(collect.glob('*.json')) if collect.exists() else []:
        if path.name == 'collection.json' or path.name.startswith(('episode-', 'ex1-')):
            files.append((path.relative_to(d).as_posix(), path))
    return files


# ---------------------------------------------------------------- staging (on the node, by supervise)

def staging_dir(root):
    return Path(root) / STAGING


def chain_staging(root, arm, chain):
    return staging_dir(root) / arm / chain


def entry_dir(base, arm, chain, update):
    return Path(base) / arm / chain / f'u{update:09d}'


def progress(root, arm, chain):
    path = chain_staging(root, arm, chain) / 'PROGRESS.json'
    return read_json(path) if path.exists() else {'next_update': 0, 'staged': 0, 'missing': []}


def stage_update(root, arm, chain, seed_root, rec, seg_root, api):
    """Compresses one selected update into staging (ENTRY.json last). Returns the entry, or None if its files are
    gone."""
    files = update_files(seg_root, rec['iteration'])
    if files is None or not all(p.exists() for _, p in files):
        return None
    library, compress, decompress = api
    out = entry_dir(staging_dir(root), arm, chain, rec['update'])
    items = []
    for rel, path in files:
        raw = path.read_bytes()
        frame = compress(raw)
        require(decompress(frame) == raw, f'{path}: the zstd frame does not decode to the file')
        write_bytes(out / f'{rel}.zst', frame)
        items.append({'path': rel, 'sha256': sha_bytes(raw), 'bytes': len(raw), 'zst_sha256': sha_bytes(frame),
                      'zst_bytes': len(frame)})
    entry = {'schema': ENTRY_SCHEMA, 'arm': arm, 'chain': chain, 'seed_root': seed_root, 'update': rec['update'],
             'boundary': rec['boundary'], 'selection_sha256': selection_hash(seed_root, rec['update']),
             'after_state_sha256': rec['after_state_sha256'], 'files': items, 'library': library,
             'level': ZSTD_LEVEL, 'raw_bytes': sum(i['bytes'] for i in items),
             'zst_bytes': sum(i['zst_bytes'] for i in items), 'staged_unix': round(time.time(), 3)}
    write_json(out / 'ENTRY.json', entry)
    return entry


def stage(root, arm, chain, seed_root, chain_doc, records, api=None):
    """Stages every selected update past the chain's watermark (records are completed updates, in order). Returns
    the counts for the cycle report."""
    prog = progress(root, arm, chain)
    todo = [r for r in records if r['update'] >= prog['next_update']]
    if not todo:
        return {'staged': 0, 'missing': 0, 'next_update': prog['next_update']}
    staged = missing = 0
    for rec in todo:
        if selected(selection_hash(seed_root, rec['update'])):
            api = api or zstd_api()
            done = entry_dir(staging_dir(root), arm, chain, rec['update']) / 'ENTRY.json'
            if not done.exists():
                if stage_update(root, arm, chain, seed_root, rec, chain_doc['segments'][rec['segment']]['root'],
                                api) is None:
                    prog['missing'].append(rec['update'])
                    missing += 1
                else:
                    staged += 1
        prog['next_update'] = rec['update'] + 1
    prog['staged'] += staged
    write_json(chain_staging(root, arm, chain) / 'PROGRESS.json', prog)
    return {'staged': staged, 'missing': missing, 'next_update': prog['next_update']}


def protected(root, arm, chain, seed_root, chain_doc, records):
    """Absolute paths (posix) of the files of selected updates past the watermark (not staged yet): rolling
    retention keeps them."""
    nxt = progress(root, arm, chain)['next_update']
    out = set()
    for rec in records:
        if rec['update'] >= nxt and selected(selection_hash(seed_root, rec['update'])):
            files = update_files(chain_doc['segments'][rec['segment']]['root'], rec['iteration']) or []
            out |= {Path(p).as_posix() for _, p in files}
    return out


def reservation(root, arm, chain, seed_root, last_update, bytes_per_update):
    """Bytes staging may add before the next guard verdict: the selected updates from the watermark through
    `last_update` (the last update the verdict admits), each at most its raw bytes plus the frame overhead."""
    nxt = progress(root, arm, chain)['next_update']
    count = sum(1 for u in range(nxt, last_update + 1) if selected(selection_hash(seed_root, u)))
    return count * int(bytes_per_update * FRAME_OVERHEAD + 65536)


# ---------------------------------------------------------------- the store on E: (ship)

@contextlib.contextmanager
def store_lock(store):
    path = Path(store) / 'SHIP.lock'
    path.parent.mkdir(parents=True, exist_ok=True)
    f = open(path, 'a+b')
    try:
        if os.name == 'nt':
            import msvcrt
            f.seek(0)
            try:
                msvcrt.locking(f.fileno(), msvcrt.LK_NBLCK, 1)
            except OSError:
                raise SampleError('another ship holds the store')
        else:
            import fcntl
            try:
                fcntl.flock(f.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError:
                raise SampleError('another ship holds the store')
        yield
    finally:
        f.close()


def manifest_events(store):
    path = Path(store) / 'MANIFEST.jsonl'
    out, prev = [], None
    if path.exists():
        for number, raw in enumerate(path.read_bytes().splitlines(), 1):
            event = json.loads(raw)
            require(event.get('prev_sha256') == prev, f'{path}: line {number} breaks the hash chain')
            prev = sha_bytes(raw)
            out.append(event)
    return out


def append_event(store, event):
    path = Path(store) / 'MANIFEST.jsonl'
    events = path.read_bytes().splitlines() if path.exists() else []
    event = dict(event, schema=MANIFEST_SCHEMA, prev_sha256=sha_bytes(events[-1]) if events else None,
                 unix=round(time.time(), 3))
    with open(path, 'ab') as f:
        f.write(json.dumps(event, sort_keys=True, separators=(',', ':')).encode('utf-8') + b'\n')
        f.flush()
        os.fsync(f.fileno())
    return event


def store_state(store):
    """Kept entries per arm (key -> add event), each arm's cutoff hash (None until a quota binds) and every key with
    its fate."""
    kept, cutoff, fate = {}, {}, {}
    for e in manifest_events(store):
        key = (e['arm'], e['chain'], e['update'])
        if e['event'] == 'add':
            kept.setdefault(e['arm'], {})[key] = e
            fate[key] = 'kept'
        elif e['event'] in ('evict', 'refuse'):
            kept.get(e['arm'], {}).pop(key, None)
            fate[key] = e['event']
            if cutoff.get(e['arm']) is None or e['selection_sha256'] < cutoff[e['arm']]:
                cutoff[e['arm']] = e['selection_sha256']
    return kept, cutoff, fate


def apply_quota(store, arm, kept, cutoff, quota):
    """The arm keeps the longest run of its entries in selection-hash order whose frames fit the quota; the rest
    are evicted (frames deleted, a manifest line each) and the cutoff lowered to the first evicted hash."""
    total, evict = 0, []
    for key, e in sorted(kept.items(), key=lambda kv: kv[1]['selection_sha256']):
        if evict or total + e['zst_bytes'] > quota:
            evict.append((key, e))
        else:
            total += e['zst_bytes']
    for key, e in evict:
        shutil.rmtree(entry_dir(store, *key), ignore_errors=True)
        append_event(store, {'event': 'evict', 'arm': arm, 'chain': key[1], 'update': key[2],
                             'selection_sha256': e['selection_sha256'], 'zst_bytes': e['zst_bytes'],
                             'reason': 'arm quota'})
        kept.pop(key)
        if cutoff.get(arm) is None or e['selection_sha256'] < cutoff[arm]:
            cutoff[arm] = e['selection_sha256']
    return [k for k, _ in evict]


def ship(staging, store, arm_quota=ARM_QUOTA_BYTES, total_cap=TOTAL_CAP_BYTES):
    """Copies every staged entry to the store with its frames' hashes checked there, applies the quota, then deletes
    the staged copy (module docstring)."""
    staging, store = Path(staging), Path(store)
    require(arm_quota * len(SAMPLED_ARMS) <= total_cap, 'the arm quotas pass the total cap')
    report = {'shipped': 0, 'refused': 0, 'evicted': 0, 'duplicates': 0, 'failed': []}
    with store_lock(store):
        kept, cutoff, fate = store_state(store)
        for entry_path in sorted(staging.glob('*/*/u*/ENTRY.json')):
            entry = read_json(entry_path)
            require(entry.get('schema') == ENTRY_SCHEMA and entry['arm'] in SAMPLED_ARMS, f'{entry_path}: not an entry')
            require(entry['selection_sha256'] == selection_hash(entry['seed_root'], entry['update']) and
                    selected(entry['selection_sha256']), f'{entry_path}: not a selected update')
            key = (entry['arm'], entry['chain'], entry['update'])
            src = entry_path.parent
            if key in fate:  # shipped before (a rerun after a crash before the staged copy went)
                report['duplicates'] += 1
            elif cutoff.get(entry['arm']) is not None and entry['selection_sha256'] >= cutoff[entry['arm']]:
                append_event(store, {'event': 'refuse', 'arm': key[0], 'chain': key[1], 'update': key[2],
                                     'selection_sha256': entry['selection_sha256'], 'zst_bytes': entry['zst_bytes'],
                                     'reason': 'past the arm cutoff'})
                fate[key] = 'refuse'
                report['refused'] += 1
            else:
                dest = entry_dir(store, *key)
                tmp = dest.with_name(dest.name + '.incoming')
                shutil.rmtree(tmp, ignore_errors=True)
                bad = []
                for item in entry['files']:
                    target = tmp / f'{item["path"]}.zst'
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(src / f'{item["path"]}.zst', target)
                    if sha_file(target) != item['zst_sha256'] or target.stat().st_size != item['zst_bytes']:
                        bad.append(item['path'])
                if bad:
                    shutil.rmtree(tmp, ignore_errors=True)
                    report['failed'].append({'entry': entry_path.as_posix(), 'files': bad})
                    continue  # the staged copy stays for the next ship
                write_json(tmp / 'ENTRY.json', entry)
                shutil.rmtree(dest, ignore_errors=True)
                os.replace(tmp, dest)
                event = append_event(store, dict({k: entry[k] for k in (
                    'arm', 'chain', 'seed_root', 'update', 'boundary', 'selection_sha256', 'after_state_sha256',
                    'zst_bytes', 'raw_bytes', 'files')}, event='add'))
                kept.setdefault(key[0], {})[key] = event
                fate[key] = 'kept'
                report['shipped'] += 1
            shutil.rmtree(src)  # its fate is in the manifest
        for arm in sorted(kept):  # once per ship: the result is the same as after each entry
            for evicted in apply_quota(store, arm, kept[arm], cutoff, arm_quota):
                fate[evicted] = 'evict'
                report['evicted'] += 1
        total = sum(e['zst_bytes'] for arm in kept.values() for e in arm.values())
        require(total <= total_cap, f'the store holds {total} bytes, past the {total_cap} cap')
    report.update(store_summary(store))
    return report


def store_summary(store):
    kept, cutoff, _ = store_state(store)
    return {'arms': {arm: {'updates': len(entries), 'zst_bytes': sum(e['zst_bytes'] for e in entries.values()),
                           'raw_bytes': sum(e['raw_bytes'] for e in entries.values()), 'cutoff': cutoff.get(arm)}
                     for arm, entries in sorted(kept.items())}}


def verify(store):
    """Every kept frame matches its recorded hash and decodes to the recorded raw bytes."""
    _, _, decompress = zstd_api()
    kept, _, _ = store_state(store)
    bad = []
    for arm, entries in kept.items():
        for key, e in entries.items():
            for item in e['files']:
                path = entry_dir(store, *key) / f'{item["path"]}.zst'
                if not path.exists() or sha_file(path) != item['zst_sha256'] or \
                        sha_bytes(decompress(path.read_bytes())) != item['sha256']:
                    bad.append(path.as_posix())
    return {'checked': sum(len(v) for v in kept.values()), 'bad': bad, 'ok': not bad}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('selected')
    p.add_argument('seed_root')
    p.add_argument('update', type=int)
    p = sub.add_parser('ship')
    p.add_argument('--staging', required=True)
    p.add_argument('--store', required=True)
    p.add_argument('--arm-quota-bytes', type=int, default=ARM_QUOTA_BYTES)
    p.add_argument('--total-cap-bytes', type=int, default=TOTAL_CAP_BYTES)
    for name in ('status', 'verify'):
        sub.add_parser(name).add_argument('--store', required=True)
    a = ap.parse_args(argv)
    try:
        if a.cmd == 'selected':
            digest = selection_hash(a.seed_root, a.update)
            out = {'selection_sha256': digest, 'selected': selected(digest)}
        elif a.cmd == 'ship':
            out = ship(a.staging, a.store, a.arm_quota_bytes, a.total_cap_bytes)
        elif a.cmd == 'status':
            out = store_summary(a.store)
        else:
            out = verify(a.store)
    except SampleError as exc:
        print(f'refused: {exc}', file=sys.stderr)
        return 2
    print(json.dumps(out, indent=1))
    return 0 if out.get('ok', True) and not out.get('failed') else 1


if __name__ == '__main__':
    sys.exit(main())
