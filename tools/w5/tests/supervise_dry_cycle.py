"""A no-write supervise cycle against a real plan (the step before moving a running supervisor to new W5 tools):
times full cycles, then guard-only cycles from a fresh start, to the point where GUARD.json would be published,
and reports peak memory.

  python3 -B supervise_dry_cycle.py --plan CHAINS-PLAN.json --tools-src OLD_TOOLS_W5 [--new-tools NEW_TOOLS_W5]

What it prevents, and what it does not: before any W5 module is imported, an audit hook (`sys.addaudithook`)
refuses opening a path for writing or appending (`open` modes w, a, x, +; `os.open` with a write, append, create
or truncate flag), remove, rename and replace, rmdir, chmod, chown, utime, truncate by path, link, symlink, mkdir of
a directory that does not exist, process creation (subprocess, os.system, posix_spawn, fork, exec) and signals
(os.kill); a refused call raises PermissionError and the run fails, and the run first checks that a write and a
remove are refused. The hook does not see writes through descriptors the process inherited (its stdout and
stderr), nor `flock` on a descriptor opened for reading. The real lock paths are not exercised: the indexes are
read through `IndexCache(read_only=True)` (no lock, no append: receipts past an index's end are kept in memory and
reconciled if the file grows), and the cycle is `chains.cycle` itself with its writers replaced by recorders
(`write`, `write_once`, `log`, `request`, `retention.apply`) and the three steps that take locks or start processes
skipped and reported (`recover_unsupervised`, `release_ended_boxes`, `purge_hold`). The plan is loaded without its
pin checks (`load_plan(check=False)`); the procedure checks the pins on the supervisor's plan copy (its step 2).
Run it as a user that cannot write the chain root, under a memory limit (`ulimit -v`).

`--new-tools` imports chains.py, retention.py and replay_sample.py from there and everything else from
`--tools-src` (the running tree). For a node where nothing may be written, `bundle` makes one script with the three
new modules inside it, to pipe to `python3 -B -`:

  python supervise_dry_cycle.py bundle NEW_TOOLS_W5 > dry.py   (on this PC)
  ssh node 'python3 -B - --plan PLAN --tools-src OLD_TOOLS_W5' < dry.py"""
import argparse
import json
import os
import sys
import time
import types
from pathlib import Path

NEW_MODULES = ('retention', 'replay_sample', 'chains')
WRITE_FLAGS = os.O_WRONLY | os.O_RDWR | os.O_APPEND | os.O_CREAT | os.O_TRUNC
DENIED = {'os.remove', 'os.rename', 'os.rmdir', 'os.chmod', 'os.chown', 'os.utime', 'os.truncate', 'os.link',
          'os.symlink', 'shutil.rmtree', 'shutil.move', 'shutil.copyfile', 'subprocess.Popen', 'os.system',
          'os.posix_spawn', 'os.exec', 'os.fork', 'os.forkpty', 'os.kill', 'os.killpg', 'os.startfile'}
REFUSED = []


def no_writes(event, args):
    deny = False
    if event == 'open':
        path, mode, flags = (tuple(args) + (None, None, None))[:3]
        if isinstance(mode, str):
            deny = any(c in mode for c in 'wax+')
        elif isinstance(flags, int):
            deny = bool(flags & WRITE_FLAGS)
    elif event == 'os.mkdir':
        deny = not os.path.isdir(args[0])
    elif event in DENIED:
        deny = True
    if deny:
        REFUSED.append([event, repr(args)[:300]])
        raise PermissionError(f'dry cycle: {event} refused: {repr(args)[:300]}')


def peak_bytes():
    try:
        status = dict(line.split(':', 1) for line in open('/proc/self/status').read().splitlines() if ':' in line)
        return {k: int(status[k].split()[0]) * 1024 for k in ('VmHWM', 'VmRSS')}
    except OSError:
        return {}


def load(tools_src, sources):
    """The W5 modules: the three new ones from `sources` (name to source text) or `tools_src`, the rest from
    `tools_src` (the running tree), whose path every module takes as its own (HERE)."""
    here = Path(tools_src).resolve()
    sys.path[:0] = [str(here), str(here.parent / 'ex1')]
    for name in NEW_MODULES:
        module = types.ModuleType(name)
        module.__file__ = str(here / f'{name}.py')
        sys.modules[name] = module
        exec(compile(sources[name], f'<new {name}.py>', 'exec'), module.__dict__)
    return sys.modules['chains'], sys.modules['retention']


def dry_cycle(plan_path, chains, retention, cycles=2, guard_only_cycles=1):
    plan = chains.load_plan(plan_path, check=False)
    records, marks = {'writes': [], 'logs': 0, 'requests': [], 'apply': {}}, {}
    t = {'start': None}

    def write(path, doc):
        if Path(path).name == 'GUARD.json':
            marks.setdefault('guard_seconds', []).append(round(time.time() - t['start'], 1))
            marks['guard'] = {k: doc.get(k) for k in ('ok', 'used_bytes', 'next_bytes', 'cap_bytes', 'ceilings')}
        records['writes'].append(Path(path).name)

    def write_once(path, doc):
        records['writes'].append(Path(path).name)
        return False

    def log(root, **row):
        records['logs'] += 1

    def request(plan, name, what, payload=None):
        records['requests'].append([name, what])
        return 0

    def apply(doc, log_path, hold=None):
        records['apply'][doc['chain']] = len(doc['checkpoints']) + len(doc['trajectories'])
        return 0
    chains.write, chains.write_once, chains.log, chains.request = write, write_once, log, request
    retention.apply = apply
    chains.recover_unsupervised = lambda plan, root: []
    chains.release_ended_boxes = lambda plan, root: []
    chains.purge_hold = lambda root, days, now: 0
    out = []
    runs = [('full', False)] * cycles + [('guard-only', True)] * guard_only_cycles
    for n, (mode, guard_only) in enumerate(runs):
        if n == 0 or (guard_only and not runs[n - 1][1]):  # a fresh start: new caches and byte counter
            caches = {Path(c['manifest']).as_posix(): retention.IndexCache(c['manifest'], read_only=True)
                      for c in plan['chains'].values() if c['manifest']}
            counter = retention.ByteCounter()
            if guard_only:
                chains.guard_only_allowed(plan)
        t['start'] = time.time()
        report = chains.cycle(plan, Path(plan_path).resolve(), caches=caches, counter=counter, guard_only=guard_only)
        out.append({'cycle': n + 1, 'mode': mode, 'seconds': round(time.time() - t['start'], 1),
                    'guard_at_seconds': marks.get('guard_seconds', [None])[-1], 'guard': marks.get('guard'),
                    'would_remove': dict(records['apply']), 'retention': report['retention'],
                    'records': {Path(m).parent.name: len(c.records) for m, c in caches.items()},
                    'active': {Path(m).parent.name: len(c.active) for m, c in caches.items()},
                    'unwritten_rows': {Path(m).parent.name: len(c.records.extra) for m, c in caches.items()},
                    'memory': peak_bytes()})
        records['apply'].clear()
    return {'plan': plan['label'], 'cycles': out, 'recorded_writes': sorted(set(records['writes'])),
            'log_lines': records['logs'], 'requests': records['requests'], 'refused': REFUSED,
            'skipped': ['recover_unsupervised', 'release_ended_boxes', 'purge_hold'], 'python': sys.version.split()[0],
            'uid': os.getuid() if hasattr(os, 'getuid') else None}


def bundle(new_tools):
    sources = {name: Path(new_tools, f'{name}.py').read_text(encoding='utf-8') for name in NEW_MODULES}
    me = Path(__file__).read_text(encoding='utf-8')
    return f'SOURCES = {json.dumps(sources)}\n' + me


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if argv and argv[0] == 'bundle':
        sys.stdout.write(bundle(argv[1]))
        return 0
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--plan', required=True)
    ap.add_argument('--tools-src', required=True, help='the running tools/w5 directory (read only)')
    ap.add_argument('--new-tools', help='tools/w5 holding the new chains.py, retention.py, replay_sample.py')
    ap.add_argument('--cycles', type=int, default=2)
    ap.add_argument('--guard-only-cycles', type=int, default=1, help='guard-only cycles from a fresh start, after')
    a = ap.parse_args(argv)
    sources = globals().get('SOURCES')
    if sources is None:
        sources = {name: Path(a.new_tools or a.tools_src, f'{name}.py').read_text(encoding='utf-8')
                   for name in NEW_MODULES}
    sys.dont_write_bytecode = True
    sys.addaudithook(no_writes)
    for attempt in (lambda: open(Path(a.plan).with_name('dry-cycle-probe'), 'a'), lambda: os.remove(a.plan)):
        try:  # the hook must refuse a write before anything else runs
            attempt()
            raise SystemExit('stop: the no-write hook did not refuse a write')
        except PermissionError:
            pass
    del REFUSED[:]
    t0 = time.time()
    chains, retention = load(a.tools_src, sources)
    result = dry_cycle(a.plan, chains, retention, a.cycles, a.guard_only_cycles)
    result['load_seconds'] = round(time.time() - t0, 1)
    print(json.dumps(result, sort_keys=True, default=str), flush=True)
    return 1 if REFUSED else 0


if __name__ == '__main__':
    sys.exit(main())
