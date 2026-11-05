"""W5 chain controller acceptance on a real cgroup v2 host (run as root; CI runs it on an Ubuntu runner).

Through the real tools (`chains.py plan`, `chains.py run` under `w5box.py run` in qualification mode, and
`chains.py supervise --loop`), with the stand-in trainer (fake_trainer.py, which burns CPU and reads the real
chain ledger from the chain cgroup's cpu.stat) in two one-core boxes:

- pair A (c1, x1): C at about half a box, X at a full box. Supervise opens the snapshot window, pauses X at
  1.10 R-hat (`pause --now`: the chain cgroup freezes and the frozen filler runs in X's box), C ends at its wall,
  X is past R_i, so X ends and is killed; the pair selects X's first boundary at or past R_i, and the PRUNE moves
  go to the hold folder. X's chain cgroup does not move while it is paused and the filler burns.
- pair B (c2, x2): C ends first; X runs alone; supervise pauses it and resumes it with `stop_at_usec = R_i`, and
  X stops at the first boundary at or past R_i.

  sudo python3 chains_acceptance.py --receipt OUT.json
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
sys.path.insert(0, str(TOOLS))
import chains  # noqa: E402
import retention  # noqa: E402
import w5box  # noqa: E402

FAKE = HERE / 'fake_trainer.py'
CONTAINER = HERE / 'fake_container.py'


def jwrite(path, doc):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(doc, indent=1))
    return Path(path)


def tool(*args):
    return [sys.executable, str(TOOLS / 'chains.py'), *args]


def wait_for(predicate, timeout, what):
    deadline = time.monotonic() + timeout
    while not predicate():
        if time.monotonic() > deadline:
            raise RuntimeError(f'timed out waiting for {what}')
        time.sleep(0.2)


def layout():
    topology = w5box.read_topology()
    for offbox in (1, 0):
        try:
            return w5box.compute_layout(topology, 1, offbox, 2)
        except w5box.Refusal:
            continue
    raise RuntimeError('this host has no two one-core boxes')


def inputs(t, root, cgroup_root, lay, label, specs, c_wall):
    ck = t / 'b512.ckpt'
    ck.write_text('b512 weights')
    dose = jwrite(t / 'b512-dose.json', {'source': {'checkpoint': chains.pin(ck), 'play_import': {
        'path': '/imports/flat-v4.json', 'sha256': '1' * 64}}})
    base = jwrite(t / 'base.json', {
        'schema': 'acceptance-base', 'opponents': [{'id': 'd6107922'}],
        'fake': {'burn': 0.15, 'sleep': 0.15, 'burn_x': 0.4, 'sleep_x': 0.0},
        'iterations': [{'episodes': [{'episode': {'learner_seat': 0}, 'opponent': {'kind': 'current'}}]}]})
    lay_path = jwrite(t / 'layout.json', dict(lay, host_class=w5box.host_class()))
    arm = {'parent': str(dose), 'workers': 1, 'memory_max_bytes': 1 << 30, 'bytes_per_update': 10_000}
    doc = {'schema': chains.INPUTS_SCHEMA, 'stage': '0', 'label': label, 'node': 'ci', 'root': str(root),
           'cgroup_root': str(cgroup_root), 'isolation': 'none', 'host_class': w5box.host_class(),
           'layout': str(lay_path), 'python': sys.executable, 'trainer': str(FAKE),
           'container_tool': str(CONTAINER), 'base': str(base), 'c_wall_seconds': c_wall,
           'byte_cap': 50 * 10**9,
           'arms': {'C': dict(arm, segment_updates=8), 'F': dict(arm, segment_updates=8),
                    'X8': dict(arm, segment_updates=4, budget=16384, coefficient=0.5, dose='primary')},
           'chains': specs}
    path = jwrite(t / f'inputs-{label}.json', doc)
    out = subprocess.run(tool('plan', '--inputs', str(path)), capture_output=True, text=True)
    if out.returncode:
        raise RuntimeError(f'plan failed: {out.stderr}')
    return root / 'plan' / label / 'CHAINS-PLAN.json'


def run_chain(plan, name):
    return subprocess.Popen(tool('run', '--plan', str(plan), '--chain', name, '--qualification', 'w5-chains-acc',
                                 '--max-wall-seconds', '900'))


def records(root, name):
    return retention.index(root / 'chains' / name / 'chain.json')[1]


def end(root, name):
    return json.loads((root / 'chains' / name / 'END.json').read_text())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--receipt', type=Path, required=True)
    args = ap.parse_args()
    os.chmod(CONTAINER, 0o755)
    os.chmod(FAKE, 0o755)
    t = Path(tempfile.mkdtemp(prefix='w5chains-accept-'))
    root = t / 'node'
    cgroup_root = Path(f'/sys/fs/cgroup/w5chains{os.getpid()}')
    lay = layout()
    receipt = {'host_class': w5box.host_class(), 'kernel': os.uname().release, 'layout': lay}
    supervise = None
    try:
        # ---- pair A: snapshot window, pause with the filler, C ends, X past R_i is selected
        plan_a = inputs(t, root, cgroup_root, lay, 'acc-a', {
            'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/acc1'},
            'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/acc1'}}, 40.0)
        supervise = subprocess.Popen(tool('supervise', '--plan', str(plan_a), '--loop', '1'),
                                     stdout=open(t / 'supervise-a.log', 'w'))
        wait_for(lambda: (root / 'GUARD.json').exists(), 60, 'the first guard verdict')
        c1, x1 = run_chain(plan_a, 'c1'), run_chain(plan_a, 'x1')
        x_ledger_path = root / 'state' / 'x1' / 'ledger.jsonl'
        wait_for(lambda: x_ledger_path.exists() and any(e['event'] == 'pause' for e in
                                                        w5box.Ledger(x_ledger_path).records()), 120, 'the pause')
        x_cg = w5box.Cgroup(cgroup_root / f'box{lay["boxes"][1]["slot"]}' / 'chain')
        samples = []
        for _ in range(6):  # the frozen chain's own cgroup, while the filler burns in its box
            samples.append(x_cg.usage_usec())
            time.sleep(0.5)
        filler_cg = w5box.Cgroup(cgroup_root / f'box{lay["boxes"][1]["slot"]}' / 'filler')
        filler_usage = filler_cg.usage_usec() if filler_cg.exists() else None
        pair_a = root / 'pairs' / 'c1--x1' / 'PAIR.json'
        wait_for(lambda: pair_a.exists() and json.loads(pair_a.read_text()).get('done'), 300, 'pair A')
        codes_a = {'c1': c1.wait(timeout=120), 'x1': x1.wait(timeout=120)}
        def fills_running():
            return [k for k in range(2) if (root / 'state' / f'box{k}.fill.json').exists()]
        wait_for(lambda: not fills_running(), 120, 'the fills of pair A to stop')
        supervise.terminate()
        supervise.wait(timeout=60)
        supervise = None
        pa = json.loads(pair_a.read_text())
        c_rec, x_rec = records(root, 'c1'), records(root, 'x1')
        r = c_rec[-1]['ledger_usec']
        x_ledger = w5box.Ledger(root / 'state' / 'x1' / 'ledger.jsonl').records()
        pause = [e for e in x_ledger if e['event'] == 'pause']
        close = [e for e in x_ledger if e['event'] == 'close']
        endpoint = pa['endpoints']['x1']
        by_b = {rec['boundary']: rec for rec in x_rec}
        log_a = [json.loads(line) for line in (root / 'LOG.jsonl').read_text().splitlines()]
        receipt['pair_a'] = {'pair': pa, 'exits': codes_a, 'r_usec': r, 'x_pause': pause, 'x_close': close,
                             'paused_samples': samples, 'filler_usage_usec': filler_usage,
                             'c_end': end(root, 'c1'), 'x_end': end(root, 'x1'),
                             'snapshots': len(retention.snapshot_index_path(root / 'chains' / 'x1' / 'chain.json')
                                              .read_text().splitlines())
                             if retention.snapshot_index_path(root / 'chains' / 'x1' / 'chain.json').exists() else 0,
                             'filler_runs': [p.as_posix() for p in (root / 'fillers').glob('slot*/run-*')]}
        checks = {
            'a_c_ended_at_its_wall': end(root, 'c1')['reason'] == 'c-wall' and codes_a['c1'] == 0,
            'a_x_paused_once_by_the_controller': len(pause) == 1 and pause[0]['request'].get('at') == 'controller',
            'a_filler_ran_in_the_paused_box': bool(receipt['pair_a']['filler_runs']),
            'a_ledger_unchanged_while_paused': len(pause) == 1 and all(v == pause[0]['usage_usec'] for v in samples)
            and (filler_usage or 0) > 0,
            'a_snapshot_window_opened': receipt['pair_a']['snapshots'] > 0,
            'a_x_ended_reached_r': end(root, 'x1')['reason'] == 'reached-r',
            'a_endpoint_first_at_or_past_r': by_b[endpoint]['ledger_usec'] >= r and
            (endpoint == 1 or by_b[endpoint - 1]['ledger_usec'] < r),
            'a_matched_and_pruned': pa['label'] == 'matched' and pa['done'] and
            (root / 'hold' / 'MOVES.jsonl').exists(),
            'a_decisions_logged': {e.get('action') for e in log_a if e.get('event') == 'decide'} >=
            {'snapshot', 'pause', 'select'},
        }
        # ---- pair B: C ends first; X is paused and resumed with stop_at_usec = R_i
        plan_b = inputs(t, root, cgroup_root, lay, 'acc-b', {
            'c2': {'arm': 'C', 'slot': 0, 'pair': 2, 'seed_root': 'w5/acc2'},
            'x2': {'arm': 'X8', 'slot': 1, 'pair': 2, 'seed_root': 'w5/acc2'}}, 30.0)
        supervise = subprocess.Popen(tool('supervise', '--plan', str(plan_b), '--loop', '1'),
                                     stdout=open(t / 'supervise-b.log', 'w'))
        c2 = run_chain(plan_b, 'c2')
        codes_b = {'c2': c2.wait(timeout=300)}
        x2 = run_chain(plan_b, 'x2')
        pair_b = root / 'pairs' / 'c2--x2' / 'PAIR.json'
        wait_for(lambda: pair_b.exists() and json.loads(pair_b.read_text()).get('done'), 300, 'pair B')
        codes_b['x2'] = x2.wait(timeout=120)
        wait_for(lambda: not fills_running(), 120, 'the fills of pair B to stop')
        pb = json.loads(pair_b.read_text())
        log_b = [json.loads(line) for line in (root / 'LOG.jsonl').read_text().splitlines()]
        fill_starts = [e for e in log_b if e.get('event') == 'fill-start' and e.get('ended') == 'c2']
        node_log = w5box.NodeLog(root / 'state').read()[0]
        fill_outages = [e for e in node_log if e['event'] == 'outage-start' and e['box_slot'] == 0
                        and e.get('chain') == 'c2' and e.get('handoff')]
        restored = {e['outage_id']: e for e in node_log if e['event'] == 'restored'}
        verified = [restored[o['id']] for o in fill_outages if restored.get(o['id'], {}).get('how') == 'fill-verified']
        r2 = records(root, 'c2')[-1]['ledger_usec']
        x2_rec = records(root, 'x2')
        x2_ledger = w5box.Ledger(root / 'state' / 'x2' / 'ledger.jsonl').records()
        resumes = [e for e in x2_ledger if e['event'] == 'resume']
        source = json.loads((root / 'state' / 'x2' / 'ledger-source.json').read_text())
        receipt['pair_b'] = {'pair': pb, 'exits': codes_b, 'r_usec': r2, 'x_resumes': resumes,
                             'fill_starts': fill_starts, 'fill_outages': fill_outages, 'fill_verified': verified,
                             'x_end': end(root, 'x2'), 'ledger_source': source}
        checks.update({
            'b_resumed_once_with_stop_at_r': len(resumes) == 1 and resumes[0]['payload'].get('stop_at_usec') == r2
            and resumes[0]['unchanged_since_pause'] and source.get('stop_at_usec') == r2,
            'b_x_stopped_at_r': end(root, 'x2')['reason'] == 'x-stop-at' and codes_b['x2'] == 0 and
            x2_rec[-1]['ledger_usec'] >= r2 and (len(x2_rec) == 1 or x2_rec[-2]['ledger_usec'] < r2),
            'b_matched': pb['label'] == 'matched' and pb['done'] and pb['endpoints']['x2'] == x2_rec[-1]['boundary'],
            'b_ended_box_was_an_outage_from_close_until_its_filler_was_verified': bool(fill_starts) and
            len(fill_outages) == 1 and bool(verified) and verified[0]['unix'] < end(root, 'x2')['unix'],
            'b_x_never_ran_beside_the_empty_box': bool(verified) and all(
                e['started_frozen'] or e['unix'] >= verified[0]['unix'] for e in x2_ledger if e['event'] == 'start'),
            'b_fill_stopped_after_the_stage': not (root / 'state' / 'box0.fill.json').exists() and
            not (cgroup_root / f'box{lay["boxes"][0]["slot"]}' / 'filler').exists(),
        })
        receipt['checks'] = checks
        receipt['pass'] = all(checks.values())
    finally:
        if supervise is not None:
            supervise.terminate()
            supervise.wait(timeout=60)
        for box in sorted(cgroup_root.glob('box*')) if cgroup_root.exists() else []:
            for leaf in ('chain', 'filler'):
                cg = w5box.Cgroup(box / leaf)
                if cg.exists():
                    cg.kill()
                    cg.wait_empty(10)
                    cg.rmdir()
            os.rmdir(box)
        if cgroup_root.exists():
            os.rmdir(cgroup_root)
    args.receipt.write_text(json.dumps(receipt, indent=2, default=str) + '\n')
    print(json.dumps({k: receipt.get(k) for k in ('checks', 'pass')}, indent=2))
    shutil.rmtree(t, ignore_errors=True)
    return 0 if receipt.get('pass') else 1


if __name__ == '__main__':
    raise SystemExit(main())
