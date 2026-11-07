#!/usr/bin/env python3
"""P-grow (CG) smoke on a Linux node with the native trainer, outside the W5 boxes (no w5box, no guard): a fresh
plan under `--root`, then the in-box driver's own segment step (`chains.current_segment`, which builds and checks
each segment's roster from the chain's ladder, then the trainer with the scratch reaper beside it) for `--segments`
segments. `--kill-at U` kills the trainer once, after global update U completes, and the next pass resumes it
(the config is derived again and must equal the stored one). Writes a receipt: every boundary's after-state hash,
each segment's roster (ladder boundaries, drawn members and their hashes), the per-update wall inside each segment
(between consecutive iteration receipts, so segment start is apart) and the segment start overhead.

  pgrow_smoke.py --root DIR --node-inputs PLAN-INPUTS.json --arm CG|C --segments N --segment-updates L
                 [--stride S] [--roster-max K] [--seed-root R] [--kill-at U] [--parent P --history H]
                 --out RECEIPT.json

`--node-inputs` is the node's production plan inputs (python, trainer, container tool, base, layout, the C parent):
only those pins are reused; the root, the arm and the chain are this smoke's own. Run it pinned to spare threads
(taskset); it never touches production trees.
"""
import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
import chains  # noqa: E402


def make(a):
    node = chains.read(a.node_inputs)
    parent = a.parent or node['arms']['C']['parent']
    arm = {'parent': parent, 'workers': a.workers, 'memory_max_bytes': 8 << 30, 'bytes_per_update': 1000000,
           'segment_updates': a.segment_updates}
    arms = {'F': dict(arm, segment_updates=64)}
    if a.arm == 'CG':
        arms['CG'] = dict(arm, ladder={'stride': a.stride, 'roster_max': a.roster_max,
                                       'test_stride': a.stride % 4096 != 0})
        if a.history:
            arms['CG']['ladder']['history'] = a.history
    else:
        arms['C'] = arm
    inputs = {'schema': chains.INPUTS_SCHEMA, 'stage': '0', 'label': 'smoke', 'node': node['node'],
              'root': Path(a.root).as_posix(), 'independent': True, 'cgroup_root': '/nonexistent/smoke',
              'isolation': 'none', 'host_class': node['host_class'], 'layout': node['layout'],
              'python': node['python'], 'trainer': node['trainer'], 'container_tool': node['container_tool'],
              'base': node['base'], 'c_wall_seconds': 86400.0, 'arms': arms,
              'chains': {'g1': {'arm': a.arm, 'slot': 0, 'seed_root': a.seed_root}}}
    path = Path(a.root) / 'smoke-inputs.json'
    chains.write(path, inputs)
    return chains.make_plan(path)


def run_once(arm, path, native, logs, kill_after):
    """The trainer with the reaper beside it (chains.run_reaped); with `kill_after` (iterations of this segment)
    the trainer's process group is killed once that many iterations have completed. Returns (exit, killed), where
    killed means a SIGKILL reached the trainer's group and the trainer then exited by that signal."""
    if kill_after is None:
        code, _, _ = chains.run_reaped(arm, path, None, logs, native)
        return code, False
    stop, sent = threading.Event(), []

    def watch():
        while not stop.wait(0.2):
            if chains.completed(native) >= kill_after:
                for p in Path('/proc').iterdir():
                    try:
                        if p.name.isdigit() and Path(p, 'cmdline').read_bytes().split(b'\0')[-2:-1] == \
                                [str(path).encode()]:
                            os.killpg(int(p.name), signal.SIGKILL)
                            sent.append(int(p.name))
                    except (OSError, IndexError, ProcessLookupError):
                        pass
                return
    t = threading.Thread(target=watch, daemon=True)
    t.start()
    try:
        code, _, _ = chains.run_reaped(arm, path, None, logs, native)
    finally:
        stop.set()
        t.join()
    # A kill counts only when it cut the segment short (a kill after its last update leaves it complete).
    return code, bool(sent) and code == -signal.SIGKILL and not (Path(native) / 'completion.json').exists()


def trim(arm, man, native, s):
    """Keeps the smoke's hot set small: a finished segment's trajectories go, and the previous segment's end
    checkpoint goes unless it is a ladder state (the next roster needs it) or the live source."""
    for f in Path(native).rglob('*'):
        if f.is_file() and f.name.startswith(chains.SCRATCH_BULK):
            f.unlink()
    if s == 0:
        return
    prev = man['segments'][s - 1]
    end = prev['offset'] + prev['updates']
    stride = (arm.get('ladder') or {}).get('stride')
    if stride and end % stride == 0:
        return
    for f in Path(prev['root']).rglob('checkpoint.json'):
        f.unlink()


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--root', required=True)
    ap.add_argument('--node-inputs', required=True)
    ap.add_argument('--arm', choices=('CG', 'C'), default='CG')
    ap.add_argument('--segments', type=int, required=True)
    ap.add_argument('--segment-updates', type=int, required=True)
    ap.add_argument('--stride', type=int, default=4096)
    ap.add_argument('--roster-max', type=int, default=64)
    ap.add_argument('--workers', type=int, default=4)
    ap.add_argument('--seed-root', default='chains-20261107/pgrow-smoke')
    ap.add_argument('--kill-at', type=int)
    ap.add_argument('--parent', help='a fork: the dose record `pgrow.py history` wrote (default: the node C parent)')
    ap.add_argument('--history', help='a fork: the `w5-ladder-history/v1` document (CG only)')
    ap.add_argument('--out', required=True)
    a = ap.parse_args(argv)
    root = Path(a.root).resolve()
    if a.kill_at is not None and not 0 <= a.kill_at < a.segments * a.segment_updates - 1:
        ap.error('--kill-at must name an update inside the smoke, before its last')
    if root.exists():
        ap.error(f'{root} exists: the smoke writes a fresh root only')
    node = chains.read(a.node_inputs)
    owned = [node['root'], node.get('scratch_root')] + [str(Path(node[k]).parent) for k in
                                                         ('trainer', 'base', 'layout', 'container_tool')]
    for other in filter(None, owned):
        o = Path(other).resolve()
        if root == o or o in root.parents or root in o.parents:
            ap.error(f'{root} overlaps {o}, which the node production plan uses')
    _, plan = make(a)
    arm = chains.load_arm(plan['arms'][a.arm]['file']['path'])
    man_path = Path(plan['chains']['g1']['manifest'])
    cdir = man_path.parent
    segs, killed, recovered, cut, t0 = [], False, False, None, time.time()
    while True:
        man = chains.read(man_path)
        done = [s for s in man['segments'] if (Path(s['root']) / 'completion.json').exists()]
        if len(done) >= a.segments:
            break
        start = time.time()
        s, path, native = chains.current_segment(arm, man, man_path, None)
        moved = chains.quarantine(native, cdir)
        kill_after = None
        if a.kill_at is not None and not killed and s * a.segment_updates <= a.kill_at < (s + 1) * a.segment_updates:
            kill_after = a.kill_at - s * a.segment_updates + 1
        code, was_kill = run_once(arm, path, native, cdir / 'logs', kill_after)
        if was_kill:
            cut = s
        if killed and complete and s == cut:
            recovered = True  # the cut segment, reset, completed on a later attempt
        killed = killed or was_kill
        complete = (native / 'completion.json').exists()
        cfg = chains.read(path)
        rows = sorted((native / 'iterations').glob('*/complete.json'))
        mtimes = [p.stat().st_mtime for p in rows]
        segs.append({'segment': s, 'exit': code, 'complete': complete, 'killed': was_kill, 'moved': len(moved),
                     'config_sha256': chains.sha(path),
                     'roster': {o['id']: o['source']['checkpoint']['sha256'] for o in cfg['opponents']
                                if o['id'].startswith(('ladder-u', 'ladder-h'))},
                     'draws': {k: sum(1 for it in cfg['iterations'] for e in it['episodes']
                                      if e['opponent'].get('id') == k) for k in
                               [o['id'] for o in cfg['opponents'] if o['id'].startswith(('ladder-u', 'ladder-h'))]},
                     'wall_seconds': round(time.time() - start, 3),
                     'start_overhead_seconds': round(mtimes[0] - start, 3) if mtimes and not moved else None,
                     'per_update_wall_seconds': round((mtimes[-1] - mtimes[0]) / (len(mtimes) - 1), 4)
                     if len(mtimes) > 1 and not was_kill and not moved else None})
        print(json.dumps(segs[-1]), flush=True)
        if was_kill and not complete:
            shutil.rmtree(native)  # a reaped run cannot resume natively: rerun it, as scratch_settle resets one
            continue
        if not complete:
            break
        trim(arm, man, native, s)
    man = chains.read(man_path)
    states = []
    for seg in man['segments']:
        for p in sorted((Path(seg['root']) / 'iterations').glob('*/complete.json')):
            rec = chains.read(p)
            upd = chains.read(rec['update']['path'])
            states.append({'boundary': seg['offset'] + rec['iteration'] + 1,
                           'after_state_sha256': upd['after_state_sha256']})
    per = [x['per_update_wall_seconds'] for x in segs if x['per_update_wall_seconds']]
    receipt = {'schema': 'w5-pgrow-smoke/v1', 'arm': a.arm, 'arm_file': plan['arms'][a.arm]['file'],
               'ladder': arm.get('ladder'), 'seed_root': a.seed_root, 'segment_updates': a.segment_updates,
               'trainer': arm['trainer'], 'tools': arm['tools'], 'kill_at': a.kill_at, 'killed': killed, 'recovered': recovered,
               'segments': segs, 'states': states,
               'ladder_record': chains.read(cdir / 'LADDER.json') if (cdir / 'LADDER.json').exists() else None,
               'wall_seconds': round(time.time() - t0, 3),
               'mean_per_update_wall_seconds': round(sum(per) / len(per), 4) if per else None,
               'affinity': sorted(os.sched_getaffinity(0))}
    chains.write(a.out, receipt)
    print(json.dumps({k: receipt[k] for k in ('wall_seconds', 'mean_per_update_wall_seconds', 'killed')}))
    ok = all(x['complete'] or x['killed'] for x in segs) and len(states) >= a.segments * a.segment_updates
    return 0 if ok and (a.kill_at is None or (killed and recovered)) else 1


if __name__ == '__main__':
    raise SystemExit(main())
