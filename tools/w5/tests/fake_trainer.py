#!/usr/bin/env python3
"""A stand-in for native_expanded_training_run_v1 in the W5 chain controller tests (Windows units and the Ubuntu
cgroup acceptance). Like the native trainer it resumes after the completed prefix, refuses a changed source
checkpoint, keeps each iteration's source and output checkpoint, reads `chain_ledger.source` at every update
boundary (closed_usec + cpu.stat usage_usec) and stops after the first boundary at or past `stop_at_usec`. A
checkpoint's content depends only on its source checkpoint's hash, the episode ids and the search block, so a
segmented chain and one whole run reach the same states.

Knobs (the config's `fake` block, which the test base carries, then FAKE_<NAME> in the environment): burn and
burn_x (CPU seconds per update), sleep and sleep_x (seconds per update), cpu_step and cpu_step_x (usec added to
the ledger source's cpu.stat file per update, for hosts without cgroups), fail_at (exit 1 before that global
update), publish_delay and publish_delay_x (seconds between a run's last receipt and its completion.json: a slow
final publication). FAKE_LEARNER_STATE (environment only): each update also publishes a wide learner's container
(`w_learner.state` in update.json), so a full state is a checkpoint and its container.

`CONFIG --follow SPOOL` runs consecutive configs in one process as the native trainer does (mtg-kernel
`native_expanded_training_run_v1::follow_v1`): after a complete run it writes `done-NNNNNN.json`, then waits for
`STOP` (exit 0), its parent's end (exit 0) or `next.json` (read and removed), which it admits only if every field
equals the last config's except output_directory, iterations and initial_source.checkpoint, and that checkpoint is
the last run's last output checkpoint; anything else exits 3 before any work. Knobs: follow_fail_at (exit 1 at that
global update, only in a run that is not its process's first) and FAKE_FOLLOW_REFUSE (environment only: refuse
every next config).
"""
import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path


def h(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def knob(cfg, name, x):
    key = f'{name}_x' if x and name != 'fail_at' else name
    value = os.environ.get(f'FAKE_{key.upper()}', (cfg.get('fake') or {}).get(key))
    return None if value is None else float(value)


def burn(seconds):
    target = time.process_time() + seconds
    v = 0
    while time.process_time() < target:
        for i in range(20000):
            v = (v * 31 + i) & 0xFFFFFFFF


def ledger(cfg, step):
    if not cfg.get('chain_ledger'):
        return None, None
    source = json.loads(Path(cfg['chain_ledger']['source']).read_text())
    stat = Path(source['cpu_stat'])
    usage = int(dict(line.split() for line in stat.read_text().splitlines() if line.strip())['usage_usec'])
    if step:
        usage += int(step)
        stat.write_text(f'usage_usec {usage}\n')
    record = {'schema': 'w5-chain-ledger/v1', 'usec': source['closed_usec'] + usage,
              'closed_usec': source['closed_usec'], 'live_usec': usage, 'instance': source['instance'],
              'unix_seconds': time.time()}
    return record, source.get('stop_at_usec')


FOLLOW_REFUSED = 3


def run(cfg, followed=False):
    """One run; returns (exit code, the source after its last completed iteration)."""
    root = Path(cfg['output_directory'])
    x = bool(cfg.get('ex1'))
    source = cfg['initial_source']
    recovered_stop = (json.loads(Path(cfg['chain_ledger']['source']).read_text()).get('stop_at_usec')
                      if cfg.get('chain_ledger') else None)
    stopped = False
    for i, item in enumerate(cfg['iterations']):
        d = root / 'iterations' / f'{i:06d}'
        a = d / 'attempt-000000'
        if (d / 'complete.json').exists():
            done = json.loads((d / 'complete.json').read_text())
            source = dict(done['source'], checkpoint={'path': (a / 'update' / 'checkpoint.json').as_posix(),
                                                      'sha256': done['output_identity']['checkpoint_sha256']})
            if recovered_stop is not None and done.get('chain_ledger') and \
                    done['chain_ledger']['usec'] >= recovered_stop:
                stopped = True  # a recovered boundary at or past the stop value: no new work
            continue
        if stopped:
            break
        ids = [e['episode']['id'] for e in item['episodes']]
        update = int(re.search(r'-u(\d+)-', ids[0]).group(1))
        fail_at = knob(cfg, 'fail_at', x)
        follow_fail_at = knob(cfg, 'follow_fail_at', x) if followed else None
        if (fail_at is not None and update >= fail_at) or (follow_fail_at is not None and update == follow_fail_at):
            (a / 'collect').mkdir(parents=True, exist_ok=True)  # a partial attempt, as a crash leaves one
            return 1, source
        # An S source at its initial train state has no checkpoint; `s_learner.initial` pins it (s_learner_v1.rs).
        # A fresh net-8 start has neither: the play import (the fresh-initialization parameters) is the pin.
        pinned = source['checkpoint'] or (source.get('s_learner') or {}).get('initial') or source['play_import']
        assert h(pinned['path']) == pinned['sha256'], 'source differs'
        # A named opponent with a checkpoint (a CG ladder member): its pin is checked, as the native collection
        # boundary does, and its hash enters the output checkpoint, so a chain's states depend on its roster.
        named = {o['id']: o.get('source') for o in cfg.get('opponents', [])}
        played = []
        for e in item['episodes']:
            src = named.get(e['opponent'].get('id')) if e['opponent'].get('kind') == 'fixed' else None
            if src and src.get('checkpoint'):
                assert h(src['checkpoint']['path']) == src['checkpoint']['sha256'], 'opponent differs'
                played.append(src['checkpoint']['sha256'])
        burn(knob(cfg, 'burn', x) or 0.0)
        time.sleep(knob(cfg, 'sleep', x) or 0.0)
        (a / 'collect').mkdir(parents=True, exist_ok=True)
        (a / 'update').mkdir(parents=True, exist_ok=True)
        ep = a / 'collect' / 'episode-0000.json'
        ep.write_text(json.dumps({'ids': ids, 'seeds': [e['episode']['seed'] for e in item['episodes']]}))
        if x:  # EX1's search-target sidecar beside the trajectory
            (a / 'collect' / 'ex1-0000.json').write_text(json.dumps({'ids': ids, 'targets': [[0.25, 0.75]]}))
        ck = a / 'update' / 'checkpoint.json'
        content = {'from': pinned['sha256'], 'ids': ids, 'search': (cfg.get('ex1') or {}).get('w5')}
        if played:
            content['opponents'] = played
        ck.write_text(json.dumps(content))
        c = a / 'collect' / 'collection.json'
        collection = {'collection_elapsed_seconds': 0.1, 'trajectories': [{'path': ep.as_posix(), 'sha256': h(ep)}]}
        if x:  # the sidecars' pins, as the native collector records them
            side = a / 'collect' / 'ex1-0000.json'
            collection['ex1'] = {'options': {}, 'episodes': [{'path': side.as_posix(), 'sha256': h(side)}]}
        c.write_text(json.dumps(collection))
        u = a / 'update' / 'update.json'
        update_doc = {'after_state_sha256': h(ck), 'checkpoint': {'path': ck.as_posix(), 'sha256': h(ck)}}
        if os.environ.get('FAKE_LEARNER_STATE'):  # a wide learner's container beside the checkpoint (one state)
            st = a / 'update' / 'w-train-state.bin'
            st.write_bytes(b'container of ' + h(ck).encode())
            update_doc['w_learner'] = {'state': {'path': st.as_posix(), 'sha256': h(st)}}
        u.write_text(json.dumps(update_doc))
        record, stop = ledger(cfg, knob(cfg, 'cpu_step', x))
        receipt = {'iteration': i, 'collection': {'path': c.as_posix()},
                   'update': {'path': u.as_posix(), 'sha256': h(u)},
                   'source': source, 'output_identity': {'checkpoint_sha256': h(ck)}}
        if record is not None:
            receipt['chain_ledger'] = record
        tmp = d / 'complete.json.tmp'
        tmp.write_text(json.dumps(receipt))
        os.replace(tmp, d / 'complete.json')
        source = dict(source, checkpoint={'path': ck.as_posix(), 'sha256': h(ck)})
        print(json.dumps({'completed_iteration': i}), flush=True)
        stopped = stop is not None and record['usec'] >= stop
    if len(list((root / 'iterations').glob('*/complete.json'))) == len(cfg['iterations']):
        time.sleep(knob(cfg, 'publish_delay', x) or 0.0)
        (root / 'completion.json').write_text('{}')  # as native: a stop at the last update completes the run
    return 0, source


def continues(previous, source, nxt):
    """follow_v1::check_continuation_v1's rule."""
    if nxt.get('output_directory') == previous.get('output_directory'):
        return False
    a, b = json.loads(json.dumps(previous)), json.loads(json.dumps(nxt))
    for d in (a, b):
        d.pop('output_directory', None)
        d.pop('iterations', None)
        d.get('initial_source', {}).pop('checkpoint', None)
    return a == b and (nxt.get('initial_source') or {}).get('checkpoint') == source.get('checkpoint')


def wait_spool(spool, parent):
    while True:
        if (spool / 'STOP').exists() or os.getppid() != parent:
            return None
        nxt = spool / 'next.json'
        if nxt.exists():
            data = nxt.read_bytes()
            nxt.unlink()
            return json.loads(data)
        time.sleep(0.01)


def main():
    if sys.argv[1:] == ['--settle-vector-tanh']:
        return 0
    cfg = json.loads(Path(sys.argv[1]).read_text())
    if sys.argv[2:3] != ['--follow']:
        return run(cfg)[0]
    spool, parent = Path(sys.argv[3]), os.getppid()
    index = 0
    while True:
        code, source = run(cfg, followed=index > 0)
        root = Path(cfg['output_directory'])
        if code or not (root / 'completion.json').exists():
            return code
        tmp = spool / f'.done-{index:06d}.tmp'
        tmp.write_text(json.dumps({'index': index, 'output_directory': cfg['output_directory'], 'source': source}))
        os.replace(tmp, spool / f'done-{index:06d}.json')
        index += 1
        nxt = wait_spool(spool, parent)
        if nxt is None:
            return 0
        if os.environ.get('FAKE_FOLLOW_REFUSE') or not continues(cfg, source, nxt):
            print('follow refused the next config', file=sys.stderr, flush=True)
            return FOLLOW_REFUSED
        cfg = nxt


if __name__ == '__main__':
    sys.exit(main())
