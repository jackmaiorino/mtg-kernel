"""W5 box throughput qualification on a real cgroup v2 host (run as root; CI runs it on an Ubuntu runner).

Through the real tools with the stand-in trainer (fake_trainer.py, which burns CPU and reads the real chain ledger
from the chain cgroup's cpu.stat) in two one-core boxes:
- `chains.py throughput` for C and for X8 of a Stage 0 plan: a serial phase (one box) and a full-node phase (both
  boxes), each chain through `chains.py run` and `w5box.py run` in qualification mode, stopped at N updates;
- the records pass w5box's guard: the plan, planned again with them, launches without a qualification label
  (`guarded_supervisor` builds the supervisor), and a record with one changed after-state hash is refused;
- `chains.py stage0-exit` reports utilization, R_nom and days to R_nom from the two records;
- the node is left clear (every handoff outage of the phases closed) and the qualification trees are removed.

  sudo python3 throughput_acceptance.py --receipt OUT.json
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
sys.path.insert(0, str(TOOLS))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import w5box  # noqa: E402
from chains_acceptance import CONTAINER, FAKE, jwrite, layout, tool  # noqa: E402


def plan_inputs(t, root, cgroup_root, lay, records=None):
    ck = t / 'b512.ckpt'
    ck.write_text('b512 weights')
    dose = jwrite(t / 'b512-dose.json', {'source': {'checkpoint': chains.pin(ck), 'play_import': {
        'path': '/imports/flat-v4.json', 'sha256': '1' * 64}}})
    base = jwrite(t / 'base.json', {
        'schema': 'acceptance-base', 'opponents': [{'id': 'd6107922'}],
        'fake': {'burn': 0.15, 'sleep': 0.05, 'burn_x': 0.3, 'sleep_x': 0.0},
        'iterations': [{'episodes': [{'episode': {'learner_seat': 0}, 'opponent': {'kind': 'current'}}]}]})
    lay_path = jwrite(t / 'layout.json', dict(lay, host_class=w5box.host_class()))
    arm = {'parent': str(dose), 'workers': 1, 'memory_max_bytes': 1 << 30, 'bytes_per_update': 10_000}
    arms = {'C': dict(arm, segment_updates=4), 'F': dict(arm, segment_updates=4),
            'X8': dict(arm, segment_updates=2, budget=16384, coefficient=0.5, dose='primary')}
    for a, path in (records or {}).items():
        arms[a]['throughput'] = str(path)
    doc = {'schema': chains.INPUTS_SCHEMA, 'stage': '0', 'label': 'tput-target', 'node': 'ci', 'root': str(root),
           'cgroup_root': str(cgroup_root), 'isolation': 'none', 'host_class': w5box.host_class(),
           'layout': str(lay_path), 'python': sys.executable, 'trainer': str(FAKE),
           'container_tool': str(CONTAINER), 'base': str(base), 'c_wall_seconds': 3600.0, 'byte_cap': 50 * 10**9,
           'arms': arms, 'chains': {'c1': {'arm': 'C', 'slot': 0, 'pair': 1, 'seed_root': 'w5/acc1'},
                                    'x1': {'arm': 'X8', 'slot': 1, 'pair': 1, 'seed_root': 'w5/acc1'}}}
    path = jwrite(t / ('inputs-with-records.json' if records else 'inputs.json'), doc)
    out = subprocess.run(tool('plan', '--inputs', str(path)), capture_output=True, text=True)
    if out.returncode:
        raise RuntimeError(f'plan failed: {out.stderr}')
    return root / 'plan' / 'tput-target' / 'CHAINS-PLAN.json'


def guard(plan_path, chain):
    """w5box's guard for a launch without a qualification label: the supervisor it would run, or the refusal."""
    plan = chains.read(plan_path)
    args = argparse.Namespace(plan=chains.w5box_plan(plan, chain), chain=chain, qualification=None,
                              max_wall_seconds=None, accept_lower_bound=None)
    try:
        sup = w5box.guarded_supervisor(args)
    except w5box.Refusal as exc:
        return {'refused': str(exc)}
    return {'accepted': True, 'throughput': sup.throughput}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--receipt', type=Path, required=True)
    args = ap.parse_args()
    os.chmod(CONTAINER, 0o755)
    os.chmod(FAKE, 0o755)
    t = Path(tempfile.mkdtemp(prefix='w5tput-accept-'))
    root = t / 'node'
    cgroup_root = Path(f'/sys/fs/cgroup/w5tput{os.getpid()}')
    lay = layout()
    receipt = {'host_class': w5box.host_class(), 'kernel': os.uname().release, 'layout': lay}
    try:
        target = plan_inputs(t, root, cgroup_root, lay)
        receipt['unqualified_launch'] = guard(target, 'c1')
        records, runs = {}, {}
        for arm, label, updates in (('C', 'tc', 8), ('X8', 'tx', 4)):
            out = t / f'{arm}-throughput.json'
            proc = subprocess.run(tool('throughput', '--plan', str(target), '--arm', arm, '--label', label,
                                       '--updates', str(updates), '--out', str(out), '--max-wall-seconds', '900'),
                                  capture_output=True, text=True, timeout=900)
            runs[arm] = {'exit': proc.returncode, 'stdout': proc.stdout[-4000:], 'stderr': proc.stderr[-4000:]}
            if proc.returncode == 0:
                records[arm] = out
        receipt['runs'] = runs
        recs = {a: json.loads(p.read_text()) for a, p in records.items()}
        receipt['records'] = recs
        exit_report = t / 'stage0-exit.json'
        if len(records) == 2:
            subprocess.run(tool('stage0-exit', '--record', str(records['C']), '--record', str(records['X8']),
                                '--out', str(exit_report)), check=True)
            receipt['stage0_exit'] = json.loads(exit_report.read_text())
        node_records, torn = w5box.NodeLog(root / 'state').read()
        receipt['node_after'] = {k: v for k, v in w5box.NodeLog.state(node_records, torn).items()
                                 if k in ('open_outages', 'open_faults', 'frozen')}
        if len(records) == 2:
            qualified = plan_inputs(t, root, cgroup_root, lay, records)
            receipt['qualified_launch'] = {c: guard(qualified, c) for c in ('c1', 'x1')}
            bad = json.loads(records['C'].read_text())
            first = sorted(bad['phases'][-1]['boxes'][-1]['identity'])[0]
            bad['phases'][-1]['boxes'][-1]['identity'][first] = '0' * 64
            bad_path = jwrite(t / 'C-tampered.json', bad)
            tampered = plan_inputs(t, root, cgroup_root, lay, {'C': bad_path, 'X8': records['X8']})
            receipt['tampered_launch'] = guard(tampered, 'c1')
        phases = {a: r['phases'] for a, r in recs.items()}
        boxes = [b for ps in phases.values() for p in ps for b in p['boxes']]
        checks = {
            'unqualified_launch_refused': 'throughput record' in receipt['unqualified_launch'].get('refused', ''),
            'both_records_written': set(records) == {'C', 'X8'},
            'serial_then_full_node': all([p['concurrent_boxes'] for p in ps] == [1, 2] for ps in phases.values()),
            'every_box_stopped_cleanly': all(set(p['exits'].values()) == {chains.EXIT_STOP}
                                             for ps in phases.values() for p in ps),
            'identical_states_in_every_box': all(
                len({json.dumps(b['identity'], sort_keys=True) for p in ps for b in p['boxes']}) == 1
                for ps in phases.values()),
            'in_box_cpu_measured': bool(boxes) and all(b['ledger_usec'] > 0 and 0 < b['in_box_utilization'] <= 1.05
                                                       for b in boxes),
            'memory_peak_sampled': bool(boxes) and all((b.get('memory_peak_bytes') or 0) > 0 for b in boxes),
            'stage0_exit_reported': receipt.get('stage0_exit', {}).get('declared_response_reading') is not None,
            'records_pass_the_guard': all(v.get('accepted') and v.get('throughput')
                                          for v in receipt.get('qualified_launch', {'x': {}}).values()),
            'tampered_record_refused': 'single-box reference' in receipt.get('tampered_launch', {}).get('refused', ''),
            'node_left_clear': not receipt['node_after']['frozen'],
            'trees_removed': not list((root / 'chains').glob('t[cx]-*/seg-*')),
        }
        receipt['checks'] = checks
        receipt['pass'] = all(checks.values())
    finally:
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
