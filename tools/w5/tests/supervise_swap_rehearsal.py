"""Rehearsal of moving a running supervisor to new W5 tools (docs/W5-SUPERVISE-MEMORY-20261120.md), on Linux, in a
throwaway root: a chain driver keeps running segments (stand-in trainer) under an old-tools supervise loop started
as node1's start script starts it; the swap then follows the procedure step by step (stage the tree, write and check
the supervisor's plan copy, stop the old loop between cycles and prove it exited, start the new loop with an
absolute python), and the forward recovery is exercised: a new supervisor that raises on every cycle is replaced by
`supervise --guard-only`. Every step and check is printed as a transcript line.

  python3 supervise_swap_rehearsal.py OLD_TOOLS NEW_TOOLS SCRATCH   (tools: directories holding w5/ and ex1/)

Times are scaled down: supervise --loop 5 and a 3 s restart pause (node1: 60 and 30)."""
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

PY = '/usr/bin/python3'
LOOP, PAUSE = 5, 3
T0 = time.time()


def say(step, **detail):
    print(json.dumps(dict({'t': round(time.time() - T0, 1), 'step': step}, **detail), default=str), flush=True)


def sh(cmd, check=True):
    run = subprocess.run(['bash', '-c', cmd], capture_output=True, text=True)
    if check and run.returncode:
        raise SystemExit(f'failed: {cmd}\n{run.stdout}\n{run.stderr}')
    return run.stdout.strip()


STARTED = []


def pids(pattern):
    """Live processes whose command line holds `pattern` (read from /proc; no shell of our own matches)."""
    out = []
    for d in Path('/proc').iterdir():
        if d.name.isdigit() and int(d.name) != os.getpid():
            try:
                if pattern.encode() in (d / 'cmdline').read_bytes().replace(b'\0', b' '):
                    out.append(int(d.name))
            except OSError:
                pass
    return sorted(out)


def start_loop(c, tree, plan, label, extra=''):
    """The supervise restart loop, as start-node1.sh line 9 starts it (absolute python, scaled times)."""
    cmd = (f'while true; do {PY} {tree}/w5/chains.py supervise --plan {plan} --loop {LOOP} {extra} >> '
           f'{c}/records/supervise.log 2>&1; echo "$(date -Is) supervise exited $?" >> '
           f'{c}/records/supervise-restarts.log; sleep {PAUSE}; done')
    proc = subprocess.Popen(['setsid', 'nohup', 'bash', '-c', cmd], stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)  # setsid and nohup exec: one pid
    STARTED.append(proc.pid)
    time.sleep(1)
    cmdline = Path(f'/proc/{proc.pid}/cmdline').read_bytes().split(b'\0')
    say('loop started', label=label, loop_pid=proc.pid, loop=[x.decode()[:60] for x in cmdline[:2]],
        supervisor=supervisor_python(proc.pid), python=PY, extra=extra)
    return proc.pid


def supervisor_python(loop_pid):
    out = sh(f'pgrep -P {loop_pid} || true', check=False)
    return [int(p) for p in out.split()]


def guard_age(root):
    g = json.loads((root / 'GUARD.json').read_text())
    return round(time.time() - g['unix'], 1), g


def wait_cycle_end(root, label):
    sup = next((root / 'plan').glob('*/SUPERVISE.json'))
    t0 = sup.stat().st_mtime_ns
    while sup.stat().st_mtime_ns == t0:
        time.sleep(0.2)
    time.sleep(0.5)
    say('a cycle just ended', label=label)


def stop_loop(root, loop_pid, label):
    """Procedure step 3: between cycles, the loop first, then its supervisor; both must be gone."""
    wait_cycle_end(root, label)
    children = supervisor_python(loop_pid)
    os.kill(loop_pid, signal.SIGTERM)
    for p in children:
        os.kill(p, signal.SIGTERM)
    for p in [loop_pid] + children:
        for _ in range(100):
            if not Path(f'/proc/{p}').exists() or 'Z' in Path(f'/proc/{p}/stat').read_text().split()[2]:
                break
            time.sleep(0.1)
    alive = [p for p in [loop_pid] + children if Path(f'/proc/{p}').exists()
             and 'Z' not in Path(f'/proc/{p}/stat').read_text().split()[2]]
    say('loop stopped', label=label, killed=[loop_pid] + children, still_alive=alive)
    if alive:
        raise SystemExit('the old loop or supervisor is still alive: abort (it could take SUPERVISE.lock back)')


def plan_copy(plan, plan2, tree):
    """Procedure step 2, the same code."""
    code = r'''
import json, sys
sys.path.insert(0, sys.argv[3]); import chains
plan = json.load(open(sys.argv[1]))
new = chains.tools_shas()
changed = sorted(k for k in new if plan['tools'].get(k) != new[k])
assert changed == ['chains.py', 'replay_sample.py', 'retention.py'] and set(new) == set(plan['tools']), changed
print(changed)
plan['tools'] = new
with open(sys.argv[2], 'x') as f:
    f.write(json.dumps(plan, indent=1) + '\n')
chains.load_plan(sys.argv[2])
print('ok')
'''
    out = subprocess.run([PY, '-B', '-', str(plan), str(plan2), f'{tree}/w5'], input=code, capture_output=True,
                         text=True)
    say('plan copy', plan=str(plan2), stdout=out.stdout.strip(), stderr=out.stderr.strip()[-300:], exit=out.returncode)
    if out.returncode:
        raise SystemExit('plan copy failed')


def stage_tree(old, new, dest, files=('chains.py', 'retention.py', 'replay_sample.py')):
    """Procedure step 1: the old tree with the three files replaced; the diff must list exactly those."""
    shutil.copytree(old, dest, ignore=shutil.ignore_patterns('__pycache__'))
    for f in files:
        shutil.copy2(Path(new, 'w5', f), Path(dest, 'w5', f))
    diff = sh(f'diff -rq -x __pycache__ {old} {dest} || true', check=False)
    say('tree staged', tree=str(dest), sha256=sh(f'cd {dest}/w5 && sha256sum {" ".join(files)}').splitlines(),
        diff=diff.splitlines())


def driver_state(c, name='c1'):
    cdir = c / 'oc' / 'chains' / name
    idx = cdir / 'chain.index.jsonl'
    return {'admitted': json.loads((cdir / 'ADMITTED.json').read_text())['count'] if (cdir / 'ADMITTED.json').exists()
            else 0, 'index_rows': sum(1 for _ in open(idx)) if idx.exists() else 0,
            'segments': len(json.loads((cdir / 'chain.json').read_text())['segments'])}


def watch(c, root, label, seconds):
    t_end = time.time() + seconds
    first = driver_state(c)
    while time.time() < t_end:
        time.sleep(2)
    restarts = (c / 'records' / 'supervise-restarts.log')
    log = (c / 'records' / 'supervise.log').read_text().splitlines()
    say('check', label=label, guard_age_seconds=guard_age(root)[0], guard_ok=guard_age(root)[1]['ok'],
        driver_before=first, driver_after=driver_state(c),
        restarts=restarts.read_text().splitlines()[-3:] if restarts.exists() else [],
        tracebacks=sum('Traceback' in x for x in log), last_report=(log[-1][:300] if log else None))


def main():
    old, new, scratch = (Path(p).resolve() for p in sys.argv[1:4])
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True)
    c = scratch / 'chains-rehearsal'
    (c / 'records').mkdir(parents=True)
    t_old = c / 'tools-src'
    shutil.copytree(old, t_old, ignore=shutil.ignore_patterns('__pycache__'))
    sys.path[:0] = [str(t_old / 'w5' / 'tests'), str(t_old / 'w5')]
    os.environ.update({'FAKE_CPU_STEP': '1000000', 'FAKE_CPU_STEP_X': '3000000', 'FAKE_SLEEP': '0.5'})
    import chains
    from test_w5_chains import Node
    (c / 'oc-inputs').mkdir()
    node = Node(c / 'oc-inputs')
    node.root = c / 'oc'
    path, plan = node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'},
                                 'c2': {'arm': 'C', 'slot': 1, 'seed_root': 'oc/c2'}},
                           c_wall_seconds=10**7, independent=True, label='oc-rehearsal')
    root = Path(plan['root'])
    say('plan', plan=str(path), root=str(root), tools=plan['tools'])
    node.ledger_source('c1')
    old_loop = start_loop(c, t_old, path, 'old')
    time.sleep(LOOP + 2)
    jsrc = root / 'state' / 'c1' / 'ledger-source.json'
    arm = plan['arms']['C']['file']['path']
    driver = subprocess.Popen([PY, str(t_old / 'w5' / 'chains.py'), '_drive', '--arm-file', arm, '--chain', 'c1',
                               '--seed-root', 'oc/c1', '--state', str(root / 'state' / 'c1'), '--ledger-source',
                               str(jsrc)], stdout=open(c / 'records' / 'run-c1.log', 'w'), stderr=subprocess.STDOUT,
                              start_new_session=True)
    STARTED.append(driver.pid)
    say('driver started', pid=driver.pid, tree='old')
    watch(c, root, 'old supervisor running', 25)
    # procedure step 1 and 2
    t_new = c / 'tools-mem-new'
    stage_tree(t_old, new, t_new)
    p2 = path.with_name('CHAINS-PLAN.supervise-new.json')
    plan_copy(path, p2, t_new)
    # procedure step 2a: the no-write dry cycle of the new tools against the live plan
    dry = subprocess.run([PY, '-B', str(Path(__file__).resolve().with_name('supervise_dry_cycle.py')), '--plan',
                          str(path), '--tools-src', f'{t_old}/w5', '--new-tools', f'{t_new}/w5'],
                         capture_output=True, text=True)
    say('dry cycle', exit=dry.returncode,
        result=json.loads(dry.stdout) if dry.returncode == 0 and dry.stdout.strip() else dry.stderr[-1500:])
    # procedure step 3: swap
    stop_loop(root, old_loop, 'old')
    age, _ = guard_age(root)
    say('gap begins', guard_age_seconds=age, driver=driver_state(c), driver_alive=driver.poll() is None)
    new_loop = start_loop(c, t_new, p2, 'new')
    watch(c, root, 'new supervisor, 3-minute check (scaled)', 15)
    watch(c, root, 'new supervisor, 10-minute check (scaled)', 30)
    # forward recovery: a new supervisor that raises on every cycle (retention.plan made to raise)
    stop_loop(root, new_loop, 'new')
    t_bad = c / 'tools-mem-faulty'
    shutil.copytree(t_new, t_bad, ignore=shutil.ignore_patterns('__pycache__'))
    rpy = t_bad / 'w5' / 'retention.py'
    rpy.write_text(rpy.read_text().replace(
        'def plan(chain_path, snapshot_from=None, cache=None):\n',
        'def plan(chain_path, snapshot_from=None, cache=None):\n    raise RuntimeError("rehearsal fault")\n', 1))
    p3 = path.with_name('CHAINS-PLAN.supervise-faulty.json')
    plan_copy(path, p3, t_bad)
    bad_loop = start_loop(c, t_bad, p3, 'faulty')
    watch(c, root, 'faulty supervisor (raises every cycle)', 20)
    os.kill(bad_loop, signal.SIGTERM)
    for p in supervisor_python(bad_loop):
        os.kill(p, signal.SIGTERM)
    time.sleep(1)
    say('faulty loop stopped', alive=pids(f'supervise --plan {p3}'))
    files_before = sorted(str(p) for p in (root / 'chains').rglob('*') if p.is_file())
    guard_loop = start_loop(c, t_bad, p3, 'guard-only', extra='--guard-only')
    watch(c, root, 'guard-only recovery on the faulty tree', 25)
    files_after = sorted(str(p) for p in (root / 'chains').rglob('*') if p.is_file())
    say('guard-only deleted nothing', deleted=len(set(files_before) - set(files_after)),
        added=len(set(files_after) - set(files_before)))
    stop_loop(root, guard_loop, 'guard-only')
    final = start_loop(c, t_new, p2, 'new again')
    watch(c, root, 'back on the new tree', 20)
    stop_loop(root, final, 'new again')
    driver.send_signal(signal.SIGTERM)
    driver.wait(30)
    say('done', driver=driver_state(c), log_tail=(c / 'records' / 'supervise.log').read_text().splitlines()[-1][:300])


if __name__ == '__main__':
    try:
        main()
    finally:  # nothing of the rehearsal outlives it
        for pid in STARTED:
            try:
                os.killpg(pid, signal.SIGTERM)
            except OSError:
                pass
