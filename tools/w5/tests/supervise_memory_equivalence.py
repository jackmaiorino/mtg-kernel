"""Supervise cycles on a real chain index, for an old/new comparison of `chains.py supervise --loop` outputs and
memory (the IndexCache memory change). Run once per tools tree, at the same work path, then compare the two digests:

  python supervise_memory_equivalence.py run --tools TOOLS_W5 --index chain.index.jsonl --manifest chain.json \
      --prefix /srv/work/.../oc/chains/c4/ --work DIR --out digest.jsonl
  python supervise_memory_equivalence.py compare OLD.jsonl NEW.jsonl

The fixture is an independent Stage 0 plan (as the production chains' plans are) of two C chains built from one
real index: `c1` is live (the index plus updates appended batch by batch as the driver leaves them: receipts,
checkpoints, trajectories, segment completion, sometimes the driver's own index append, once an append cut before
its newline), `c2` has ended. Segment roots, kept checkpoints and recent trajectories are small stand-in files at
the paths the index names (rewritten from `--prefix` to the fixture). Each cycle is `chains.cycle` with the caches
and byte counter `supervise --loop` keeps; the caches are dropped twice (a supervisor restart). The digest has, per
cycle, the report, GUARD.json and SUPERVISE.json, the index files' hashes, LOOK.json, the retention logs, LOG.jsonl
and a listing of every file left under the root (wall-clock fields removed), then function-level reads of the pair
code on the same records (valid records, projection, decide, select, PRUNE manifest, the replay sample's
selection), and the process memory after each cycle (outside the compared fields)."""
import argparse
import contextlib
import ctypes
import gc
import hashlib
import io
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from unittest import mock

BATCHES = (3, 61, 256, 7, 300, 1, 128, 'restart', 512, 40, 'other-writer', 90, 255, 'cut-newline', 2, 777, 'restart',
           64, 5)
SEGMENT = 256
WALL_KEYS = {'unix', 'id'}


def memory():
    """Resident and private bytes of this process now, and the peak resident bytes."""
    if os.name == 'nt':
        class Counters(ctypes.Structure):
            _fields_ = [('cb', ctypes.c_ulong), ('PageFaultCount', ctypes.c_ulong),
                        ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                        ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                        ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                        ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t),
                        ('PrivateUsage', ctypes.c_size_t)]
        c = Counters()
        c.cb = ctypes.sizeof(c)
        k32 = ctypes.WinDLL('kernel32')
        k32.GetCurrentProcess.restype = ctypes.c_void_p
        k32.K32GetProcessMemoryInfo.argtypes = [ctypes.c_void_p, ctypes.POINTER(Counters), ctypes.c_ulong]
        if not k32.K32GetProcessMemoryInfo(k32.GetCurrentProcess(), ctypes.byref(c), c.cb):
            raise OSError('GetProcessMemoryInfo failed')
        return {'rss': c.WorkingSetSize, 'peak_rss': c.PeakWorkingSetSize, 'private': c.PrivateUsage,
                'peak_private': c.PeakPagefileUsage}
    status = dict(line.split(':', 1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    kib = {k: int(status[k].split()[0]) * 1024 for k in ('VmRSS', 'VmHWM', 'VmData')}
    return {'rss': kib['VmRSS'], 'peak_rss': kib['VmHWM'], 'private': kib['VmData'], 'peak_private': None}


def strip(doc, keys=WALL_KEYS):
    if isinstance(doc, dict):
        return {k: strip(v, keys) for k, v in doc.items() if k not in keys}
    if isinstance(doc, list):
        return [strip(v, keys) for v in doc]
    return doc


def digest_bytes(data):
    return hashlib.sha256(data).hexdigest()


def jsonl_digest(path, keys):
    if not path.exists():
        return None
    lines = [json.dumps(strip(json.loads(x), keys), sort_keys=True) for x in path.read_text(encoding='utf-8').splitlines()
             if x]
    return {'lines': len(lines), 'sha256': digest_bytes('\n'.join(lines).encode())}


def attempt(fn, *args):
    try:
        return fn(*args)
    except SystemExit as exc:
        return {'raised': str(exc)}


class Fixture:
    def __init__(self, a):
        self.a = a
        self.work = Path(a.work).resolve()
        self.tools = Path(a.tools).resolve()
        sys.path.insert(0, str(self.tools / 'tests'))
        sys.path.insert(0, str(self.tools))
        import chains
        import retention
        import replay_sample
        from test_w5_chains import Node
        self.chains, self.retention, self.replay_sample = chains, retention, replay_sample
        self.work.mkdir(parents=True)
        self.node = Node(self.work)
        self.root = self.node.root
        self.next_update, self.ledger, self.unix = None, None, None

    def cdir(self, name):
        return self.root / 'chains' / name

    def build(self):
        a, root = self.a, self.root
        prefix = a.prefix.encode()
        with open(a.index, 'rb') as f:
            f.seek(-4096, os.SEEK_END)
            tail = json.loads(f.read().splitlines()[-1])
        r_nom = 2 * (tail['ledger_usec'] + 1000 * 1_190_000)  # the live chain reaches its look ~1,000 updates in
        path, plan = self.node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'},
                                          'c2': {'arm': 'C', 'slot': 1, 'seed_root': 'oc/c2'}},
                                    c_wall_seconds=2419200.0, independent=True, label='mem', r_nom_usec=r_nom)
        lines = 0
        for name in ('c1', 'c2'):
            cdir = self.cdir(name)
            assert Path(plan['chains'][name]['manifest']).resolve() == (cdir / 'chain.json').resolve()
            new_prefix = (cdir.as_posix() + '/').encode()
            with open(a.index, 'rb') as src, open(cdir / 'chain.index.jsonl', 'wb') as dst:
                for k, line in enumerate(src):
                    if name == 'c2' and a.ended_lines and k >= a.ended_lines:
                        break
                    dst.write(line.replace(prefix, new_prefix))
                    lines += name == 'c1'
            real = json.loads(Path(a.manifest).read_text(encoding='utf-8').replace(a.prefix, cdir.as_posix() + '/'))
            man = json.loads((cdir / 'chain.json').read_text(encoding='utf-8'))  # the plan's header
            man.update(launched_unix=real['launched_unix'], segments=real['segments'])
            (cdir / 'chain.json').write_text(json.dumps(man, indent=1) + '\n', encoding='utf-8')
            for seg in man['segments']:
                Path(seg['root']).mkdir(parents=True, exist_ok=True)
                Path(seg['root'], 'completion.json').write_text('{}')
            latest = lines
            last_segs = {len(man['segments']) - k for k in (1, 2, 3)} if name == 'c1' else set()
            with open(cdir / 'chain.index.jsonl', 'rb') as f:
                for line in f:
                    rec = json.loads(line)
                    b = rec['boundary']
                    keep = b % 4096 == 0 or (b > latest - (8192 if name == 'c1' else 4096) and b % 64 == 0) or \
                        rec['segment'] in last_segs
                    if keep:
                        p = Path(rec['checkpoint']['path'])
                        p.parent.mkdir(parents=True, exist_ok=True)
                        p.write_bytes(b'k' * (100 + b % 7))
                    if rec['iteration'] == 0 and rec['segment'] >= len(man['segments']) - 70:
                        t = Path(man['segments'][rec['segment']]['root'], 'iterations', '000000', 'attempt-000000',
                                 'collect', 'episode-000000.json')
                        t.parent.mkdir(parents=True, exist_ok=True)
                        t.write_bytes(b't' * 50)
                    last = rec
            if name == 'c2':
                end = {'arm': 'C', 'by': 'operator', 'chain': 'c2', 'last_valid_boundary': 1_000_000,
                       'ledger_usec': last['ledger_usec'], 'reason': 'operator', 'schema': 'w5-chain-end/v1',
                       'unix': 1794678303.724}
                (cdir / 'END.json').write_text(json.dumps(end, indent=1) + '\n')
            else:
                self.next_update, self.ledger, self.unix = last['update'] + 1, last['ledger_usec'], \
                    last['unix_seconds']
        return path, lines

    def add_updates(self, count):
        """Receipts, checkpoints and trajectories of `count` more updates of c1, as its driver leaves them."""
        cdir = self.cdir('c1')
        man_path = cdir / 'chain.json'
        man = json.loads(man_path.read_text(encoding='utf-8'))
        for _ in range(count):
            u = self.next_update
            s, i = divmod(u, SEGMENT)
            if s == len(man['segments']):
                man['segments'].append({'config': (cdir / 'segments' / f'seg-{s:05d}.json').as_posix(),
                                        'offset': s * SEGMENT, 'root': (cdir / f'seg-{s:05d}').as_posix(),
                                        'updates': SEGMENT})
            seg = Path(man['segments'][s]['root'])
            it = seg / 'iterations' / f'{i:06d}'
            upd = it / 'attempt-000000' / 'update'
            upd.mkdir(parents=True)
            ck = upd / 'checkpoint.json'
            ck.write_bytes(b'c' * (200 + u % 13))
            (it / 'attempt-000000' / 'collect').mkdir(parents=True)
            (it / 'attempt-000000' / 'collect' / 'episode-000000.json').write_bytes(b'e' * 60)
            self.ledger += 1_190_000 + u % 977
            self.unix += 0.43 + (u % 5) / 100
            (upd / 'update.json').write_text(json.dumps({'after_state_sha256': f'{u:064x}', 'checkpoint': {
                'path': ck.as_posix(), 'sha256': hashlib.sha256(ck.read_bytes()).hexdigest()}}))
            (it / 'complete.json').write_text(json.dumps({'update': {'path': (upd / 'update.json').as_posix()},
                                                          'chain_ledger': {'usec': self.ledger,
                                                                           'unix_seconds': self.unix,
                                                                           'instance': 1}}))
            self.next_update = u + 1
            if i == SEGMENT - 1:
                (seg / 'completion.json').write_text('{}')
        man_path.write_text(json.dumps(man, indent=1) + '\n', encoding='utf-8')

    def other_writer(self):
        """The driver's own index() append, in its own process."""
        code = 'import sys; sys.path.insert(0, sys.argv[1]); import retention; retention.index(sys.argv[2])'
        subprocess.run([sys.executable, '-c', code, str(self.tools), str(self.cdir('c1') / 'chain.json')], check=True)

    def cut_newline(self):
        """An append interrupted before its newline: a driver index() append, then the last newline cut."""
        self.add_updates(3)
        self.other_writer()
        index_file = self.cdir('c1') / 'chain.index.jsonl'
        with open(index_file, 'rb+') as f:
            f.seek(-1, os.SEEK_END)
            assert f.read(1) == b'\n'
            f.truncate(f.tell() - 1)

    def observe(self, report):
        root = self.root
        out = {'report': strip(report)}
        for name in ('GUARD.json', 'plan/mem/SUPERVISE.json'):
            p = root / name
            out[name] = strip(json.loads(p.read_text())) if p.exists() else None
        for name in ('c1', 'c2'):
            cdir = self.cdir(name)
            out[f'{name}/index_sha256'] = digest_bytes((cdir / 'chain.index.jsonl').read_bytes())
            look = cdir / 'LOOK.json'
            out[f'{name}/LOOK.json'] = json.loads(look.read_text()) if look.exists() else None
            out[f'{name}/retention_log'] = jsonl_digest(root / 'retention' / f'{name}.jsonl', {'unix_seconds'})
        out['LOG.jsonl'] = jsonl_digest(root / 'LOG.jsonl', {'unix'})
        skip = {root / 'GUARD.json', root / 'LOG.jsonl'}
        listing = sorted(f'{p.relative_to(root).as_posix()} {p.stat().st_size}' for p in root.rglob('*')
                         if p.is_file() and p not in skip and p.parent != root / 'retention'
                         and not p.relative_to(root).as_posix().startswith('plan/'))
        out['tree'] = {'files': len(listing), 'sha256': digest_bytes('\n'.join(listing).encode())}
        return out

    def pair_reads(self):
        """The pair code's reads of the same records (`chains.py` gives them the cache's records)."""
        chains, retention, replay_sample = self.chains, self.retention, self.replay_sample
        caches = {}
        out = {}
        infos = {n: chains.chain_info(self.root, n, caches) for n in ('c1', 'c2')}
        c1, c2 = infos['c1'], infos['c2']
        valid = chains.valid_records(c2)
        out['valid'] = [len(valid), valid[-1]['boundary'], valid[0]['boundary']]
        out['ledger_series'] = digest_bytes(json.dumps(retention.ledger_series(c1['records'])).encode())
        out['project'] = retention.project(valid, c2['chain']['launched_unix'], valid[-1]['unix_seconds'] + 3600,
                                           2419200.0)
        last = c1['records'][-1]['ledger_usec']
        out['decide_running'] = retention.decide(c1['chain'], c1['records'], c2['chain'], valid, False, 1000.0,
                                                 valid[-1]['unix_seconds'] + 3600, 10**6)
        out['decide_c_ended'] = retention.decide(c1['chain'], c1['records'], c2['chain'], valid, True, 1000.0)
        for target in (0, 10**9, last // 2, last, last + 1):
            hit = retention.first_at_or_past(c1['records'], target)
            out[f'first_at_or_past_{target}'] = hit
        weights = retention.snapshots(c1['path'])
        for target in (c1['records'][-5]['ledger_usec'], c1['records'][min(70_000, len(c1['records']) // 3)]['ledger_usec']):
            out[f'select_{target}'] = attempt(retention.select, c1['chain'], c1['records'], target, weights)
        middle = c1['records'][len(c1['records']) // 2]
        try:
            out['ledger_at'] = [middle['boundary'], chains.ledger_at(c1['records'], middle['boundary'])]
        except StopIteration:
            out['ledger_at'] = [middle['boundary'], 'not found']
        keep = c1['records'][-1]['boundary'] // 4096 * 4096
        out['prune_manifest'] = attempt(retention.prune_manifest, c1['chain'], c1['records'], keep, None, weights)
        out['prune_manifest_0'] = digest_bytes(json.dumps(retention.prune_manifest(
            c1['chain'], c1['records'], 0, None, weights), sort_keys=True).encode())
        stage_root = self.work / 'sample'
        progress = replay_sample.chain_staging(stage_root, 'X8', 'c1') / 'PROGRESS.json'
        progress.parent.mkdir(parents=True, exist_ok=True)
        progress.write_text(json.dumps({'next_update': c1['records'][-600]['update'], 'staged': 0, 'missing': []}))
        out['sample_protected'] = sorted(replay_sample.protected(stage_root, 'X8', 'c1', 'oc/c1', c1['chain'],
                                                                 c1['records']))
        out['sample_since'] = [r['update'] for r in replay_sample.since(c1['records'], c1['records'][-40]['update'])] \
            if hasattr(replay_sample, 'since') else [r['update'] for r in c1['records']
                                                      if r['update'] >= c1['records'][-40]['update']]
        return json.loads(json.dumps(out, sort_keys=True, default=str))


REAL_TIME = time.time


def steady_clock():
    """Wall time at a fixed decimal width (whole seconds plus a half), so files that record the time (LOG.jsonl,
    retention logs, GUARD.json) keep the same byte length in both runs and the guard's byte counts compare."""
    return float(int(REAL_TIME())) + 0.5


def run(a):
    with mock.patch('time.time', steady_clock),             mock.patch('time.time_ns', lambda: int(REAL_TIME()) * 10**9 + 500_000_000):
        return run_steady(a)


def run_steady(a):
    fx = Fixture(a)
    t0 = time.perf_counter()
    plan_path, lines = fx.build()
    chains, retention = fx.chains, fx.retention
    gc.collect()
    base = memory()
    print(json.dumps({'built_seconds': round(time.perf_counter() - t0, 1), 'records': lines, 'memory': base}), flush=True)
    out = open(a.out, 'w', encoding='utf-8')
    out.write(json.dumps({'kind': 'setup', 'records_per_chain': lines}) + '\n')
    caches, counter = {}, retention.ByteCounter()
    plan = chains.load_plan(plan_path, check=False)
    n = 0
    for step in (0,) + BATCHES:
        if step == 'restart':
            caches, counter = {}, retention.ByteCounter()
            gc.collect()
            event = 'restart'
        elif step == 'other-writer':
            fx.add_updates(17)
            fx.other_writer()
            event = 'other-writer'
        elif step == 'cut-newline':
            fx.cut_newline()
            event = 'cut-newline'
        else:
            fx.add_updates(step)
            event = f'append {step}'
        t = time.perf_counter()
        with mock.patch.object(chains.subprocess, 'Popen'), contextlib.redirect_stdout(io.StringIO()):
            report = chains.cycle(plan, plan_path, caches=caches, counter=counter)
        seconds = time.perf_counter() - t
        gc.collect()
        mem = memory()
        n += 1
        out.write(json.dumps({'kind': 'cycle', 'n': n, 'event': event, 'observed': fx.observe(report)},
                             sort_keys=True) + '\n')
        out.flush()
        print(json.dumps({'cycle': n, 'event': event, 'seconds': round(seconds, 2), 'memory': mem,
                          'records_c1': fx.next_update}), flush=True)
    t = time.perf_counter()
    reads = fx.pair_reads()
    out.write(json.dumps({'kind': 'pair_reads', 'reads': reads}, sort_keys=True) + '\n')
    out.close()
    print(json.dumps({'pair_reads_seconds': round(time.perf_counter() - t, 1), 'memory': memory()}), flush=True)


def compare(a):
    old = Path(a.old).read_text(encoding='utf-8').splitlines()
    new = Path(a.new).read_text(encoding='utf-8').splitlines()
    differ = [i for i, (x, y) in enumerate(zip(old, new)) if x != y]
    result = {'old_lines': len(old), 'new_lines': len(new), 'differing_lines': differ,
              'equal': not differ and len(old) == len(new)}
    for i in differ[:3]:
        x, y = json.loads(old[i]), json.loads(new[i])
        result.setdefault('first', []).append({'line': i, 'old': x, 'new': y})
    print(json.dumps(result, indent=1, default=str)[:20000])
    return 0 if result['equal'] else 1


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('run')
    for k in ('tools', 'index', 'manifest', 'prefix', 'work', 'out'):
        p.add_argument('--' + k, required=True)
    p.add_argument('--ended-lines', type=int, help='the ended chain c2 keeps only this many index lines')
    p = sub.add_parser('compare')
    p.add_argument('old')
    p.add_argument('new')
    a = ap.parse_args()
    return run(a) if a.cmd == 'run' else compare(a)


if __name__ == '__main__':
    sys.exit(main())
