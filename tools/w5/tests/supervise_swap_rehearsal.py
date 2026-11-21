"""Rehearsal of moving a running supervisor to new W5 tools (docs/W5-SUPERVISE-MEMORY-20261120.md), on Linux, in a
throwaway root, running the procedure's own step file (tools/w5/supervise_swap.sh): a chain driver keeps running
segments (stand-in trainer) under an old-tools supervise loop; the swap stages the tree, writes and checks the plan
copy, runs the dry cycle, stops the old loop between cycles (it must be asleep), records the after_stop baseline,
checks every index tail (and cuts an injected torn last line), starts the new loop and then watches it with that
baseline, in the procedure's order. Then a supervisor that fails before every GUARD write is started (after its
own stop's baseline) and stopped with the bounded stop for a failing loop, a new baseline is recorded, and
`supervise --guard-only` is started and watched in recovery mode with it (its time to the first GUARD is
measured). Every step is a transcript line. The whole run is under an alarm.

  python3 supervise_swap_rehearsal.py OLD_TOOLS NEW_TOOLS SCRATCH   (tools: directories holding w5/ and ex1/)

Times are scaled down: supervise --loop 12 (the step file refuses less than 10) and a 3 s restart pause (node1: 60
and 30)."""
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

PY = '/usr/bin/python3'
LOOP, PAUSE = 12, 3
T0 = time.time()
STARTED = []


def say(step, **detail):
    print(json.dumps(dict({'t': round(time.time() - T0, 1), 'step': step}, **detail), default=str), flush=True)


def swap(env, step, *args, check=True, timeout=600):
    """One step of the procedure's step file, as the lead runs it."""
    run = subprocess.run(['bash', str(env['SWAP']), step, *map(str, args)], env=dict(os.environ, **env),
                         capture_output=True, text=True, timeout=timeout)
    out = (run.stdout + run.stderr).strip().splitlines()
    say(f'swap.sh {step}', args=[str(a) for a in args], exit=run.returncode, out=out[-12:])
    if check and run.returncode:
        raise SystemExit(f'step {step} failed')
    return run.returncode, out


def guard_age(root):
    g = json.loads((root / 'GUARD.json').read_text())
    return round(time.time() - g['unix'], 1), g


def driver_state(root, name='c1'):
    cdir = root / 'chains' / name
    idx = cdir / 'chain.index.jsonl'
    rows = [json.loads(x) for x in open(idx)] if idx.exists() else []
    return {'admitted': json.loads((cdir / 'ADMITTED.json').read_text())['count'] if (cdir / 'ADMITTED.json').exists()
            else 0, 'index_rows': len(rows),
            'boundaries_contiguous': [r['boundary'] for r in rows] == list(range(1, len(rows) + 1)),
            'segments': len(json.loads((cdir / 'chain.json').read_text())['segments'])}


def watch(c, root, label, seconds):
    first = driver_state(root)
    time.sleep(seconds)
    restarts = c / 'records' / 'supervise-restarts.log'
    log = (c / 'records' / 'supervise.log').read_text().splitlines()
    age, g = guard_age(root)
    say('check', label=label, guard_age_seconds=age, guard_ok=g['ok'], driver_before=first,
        driver_after=driver_state(root), restarts=restarts.read_text().splitlines()[-3:] if restarts.exists() else [],
        tracebacks=sum('Traceback' in x for x in log), last_report=(log[-1][:240] if log else None))


def start(env, w5, plan, extra=None):
    _, out = swap(env, 'start_loop', w5, plan, *([extra] if extra else []))
    pid = int(next(x for x in out if x.startswith('LOOP_PID')).split()[1])
    STARTED.append(pid)
    return pid


def after_stop(env):
    """The procedure's `B=$(bash $S after_stop)`: the one line it prints."""
    return swap(env, 'after_stop')[1][-1]


def watch_after_start(env, plan, baseline, mode):
    """The procedure's `bash $S watch_swap PLAN "$B" MODE`, started after start_loop returned."""
    return subprocess.Popen(['bash', env['SWAP'], 'watch_swap', str(plan), baseline, mode, '1'],
                            env=dict(os.environ, **env), stdout=subprocess.PIPE, text=True)


def watch_result(label, watcher):
    out, _ = watcher.communicate(timeout=300)
    lines = out.strip().splitlines()
    last = json.loads(lines[-1]) if lines else {}
    say(label, exit=watcher.returncode, verdict=last.get('verdict'), failure=last.get('failure'),
        resume_needed=last.get('resume_needed'), drivers={k: v['driver'] for k, v in last.get('chains', {}).items()},
        guard=last.get('guard'), rounds=len(lines))


def wait_for_guard(root, after, limit):
    """Seconds from `after` (the loop's start) to the first GUARD written after it."""
    while time.time() - after < limit:
        if (root / 'GUARD.json').exists():
            unix = json.loads((root / 'GUARD.json').read_text())['unix']
            if unix > after:
                return round(unix - after, 1)
        time.sleep(0.2)
    return None


def main():
    old, new, scratch = (Path(p).resolve() for p in sys.argv[1:4])
    swap_sh = Path(__file__).resolve().parent.parent / 'supervise_swap.sh'
    shutil.rmtree(scratch, ignore_errors=True)
    scratch.mkdir(parents=True)
    c = scratch / 'chains-rehearsal'
    (c / 'records').mkdir(parents=True)
    t_old = c / 'tools-src'
    shutil.copytree(old, t_old, ignore=shutil.ignore_patterns('__pycache__'))
    sys.path[:0] = [str(t_old / 'w5' / 'tests'), str(t_old / 'w5')]
    os.environ.update({'FAKE_CPU_STEP': '1000000', 'FAKE_CPU_STEP_X': '3000000', 'FAKE_SLEEP': '0.5'})
    from test_w5_chains import Node
    (c / 'oc-inputs').mkdir()
    node = Node(c / 'oc-inputs')
    node.root = c / 'oc'
    path, plan = node.plan('0', {'c1': {'arm': 'C', 'slot': 0, 'seed_root': 'oc/c1'},
                                 'c2': {'arm': 'C', 'slot': 1, 'seed_root': 'oc/c2'}},
                           c_wall_seconds=10**7, independent=True, label='oc-rehearsal')
    root = Path(plan['root'])
    env = {'C': str(c), 'ROOT': str(root), 'PY': PY, 'LOOP': str(LOOP), 'PAUSE': str(PAUSE), 'SWAP': str(swap_sh)}
    say('plan', plan=str(path), root=str(root), step_file=str(swap_sh))
    node.ledger_source('c1')
    bystander = subprocess.Popen(['bash', '-c', f"echo 'supervise --plan {path}'; sleep 1400"],
                                 stdout=subprocess.DEVNULL, start_new_session=True)  # must never be signalled
    STARTED.append(bystander.pid)
    old_loop = start(env, t_old / 'w5', path)
    time.sleep(LOOP + 2)
    arm = plan['arms']['C']['file']['path']
    driver = subprocess.Popen([PY, str(t_old / 'w5' / 'chains.py'), '_drive', '--arm-file', arm, '--chain', 'c1',
                               '--seed-root', 'oc/c1', '--state', str(root / 'state' / 'c1'), '--ledger-source',
                               str(root / 'state' / 'c1' / 'ledger-source.json')],
                              stdout=open(c / 'records' / 'run-c1.log', 'w'), stderr=subprocess.STDOUT,
                              start_new_session=True)
    STARTED.append(driver.pid)
    say('driver started', pid=driver.pid, tree='old', old_loop=old_loop)
    watch(c, root, 'old supervisor running', 25)
    # step 1: stage the tree (the old tree with the three files replaced)
    t_new = c / 'tools-mem-new'
    shutil.copytree(t_old, t_new, ignore=shutil.ignore_patterns('__pycache__'))
    for f in ('chains.py', 'retention.py', 'replay_sample.py'):
        shutil.copy2(new / 'w5' / f, t_new / 'w5' / f)
    say('tree staged', diff=subprocess.run(['diff', '-rq', '-x', '__pycache__', str(t_old), str(t_new)],
                                           capture_output=True, text=True).stdout.splitlines())
    # step 2: the plan copy
    p2 = path.with_name('CHAINS-PLAN.supervise-new.json')
    swap(env, 'plan_copy', path, p2, t_new / 'w5')
    # step 2a: the dry cycle, full and guard-only
    dry = subprocess.run([PY, '-B', str(Path(__file__).resolve().with_name('supervise_dry_cycle.py')), '--plan',
                          str(path), '--tools-src', f'{t_old}/w5', '--new-tools', f'{t_new}/w5'],
                         capture_output=True, text=True, timeout=600)
    result = json.loads(dry.stdout) if dry.returncode == 0 and dry.stdout.strip() else dry.stderr[-1500:]
    say('dry cycle', exit=dry.returncode, refused=result.get('refused') if isinstance(result, dict) else result,
        cycles=[{k: x[k] for k in ('cycle', 'mode', 'seconds', 'guard_at_seconds')} for x in result['cycles']]
        if isinstance(result, dict) else None)
    # step 3: a look before the stop (admitted against ceiling, driver states); stop between cycles; baseline;
    # tails; start; then watch with the baseline
    swap(env, 'watch_swap', path, after_stop(env), 'swap', '1', '1', check=False)
    swap(env, 'stop_between_cycles', path)
    b_old = after_stop(env)  # B=$(bash $S after_stop): the old loop's stop
    say('gap begins', guard_age_seconds=guard_age(root)[0], driver=driver_state(root),
        driver_alive=driver.poll() is None)
    index_file = root / 'chains' / 'c1' / 'chain.index.jsonl'
    rows_before = index_file.read_bytes()
    torn = rows_before.splitlines(keepends=True)[-1][:57]
    with open(index_file, 'ab') as f:  # a supervisor killed inside an append leaves a torn last line
        f.write(torn)
    code, _ = swap(env, 'check_tails', check=False)
    say('torn tail injected', check_tails_exit=code)
    code, _ = swap(env, 'start_loop', t_new / 'w5', p2, check=False)
    say('start_loop refuses a torn tail', exit=code, loops=swap(env, 'pids', p2)[1])
    swap(env, 'cut_partial_tail', index_file)
    swap(env, 'check_tails')
    say('tail cut back', same_bytes_as_before=index_file.read_bytes() == rows_before,
        saved=[x.read_bytes() == torn for x in (c / 'records').glob('torn-tail-c1-*.bin')])
    t_start = time.time()
    new_loop = start(env, t_new / 'w5', p2)
    watcher = watch_after_start(env, p2, b_old, 'swap')
    say('new loop: first GUARD', seconds=wait_for_guard(root, t_start, 120))
    watch_result('watch_swap "$B" swap after the new start', watcher)
    watch(c, root, 'new supervisor, 3-minute check (scaled)', 15)
    watch(c, root, 'new supervisor, 10-minute check (scaled)', 30)
    # forward recovery: a supervisor that raises before every GUARD write
    swap(env, 'stop_between_cycles', p2)
    t_bad = c / 'tools-mem-faulty'
    shutil.copytree(t_new, t_bad, ignore=shutil.ignore_patterns('__pycache__'))
    rpy = t_bad / 'w5' / 'retention.py'
    rpy.write_text(rpy.read_text().replace(
        'def plan(chain_path, snapshot_from=None, cache=None):\n',
        'def plan(chain_path, snapshot_from=None, cache=None):\n    time.sleep(6)\n'
        '    raise RuntimeError("rehearsal fault before GUARD")\n',
        1))
    p3 = path.with_name('CHAINS-PLAN.supervise-faulty.json')
    swap(env, 'plan_copy', path, p3, t_bad / 'w5')
    last_good = guard_age(root)[1]['unix']
    b_new = after_stop(env)  # the new loop's stop
    swap(env, 'check_tails')
    start(env, t_bad / 'w5', p3)
    watcher = watch_after_start(env, p3, b_new, 'swap')
    watch(c, root, 'faulty supervisor (raises before every GUARD write)', 20)
    say('faulty loop never published', guard_unchanged=guard_age(root)[1]['unix'] == last_good)
    watch_result('watch_swap "$B" swap on the faulty loop', watcher)
    swap(env, 'stop_failing', p3, timeout=120)
    b_fail = after_stop(env)  # the stop_failing's baseline: the recovery watch takes this one
    swap(env, 'check_tails')
    files_before = sorted(str(p) for p in (root / 'chains').rglob('*') if p.is_file())
    t_start = time.time()
    guard_loop = start(env, t_bad / 'w5', p3, '--guard-only')
    watcher = watch_after_start(env, p3, b_fail, 'recovery')
    say('guard-only: first GUARD', seconds=wait_for_guard(root, t_start, 120))
    watch_result('watch_swap "$B" recovery after guard-only', watcher)
    watch(c, root, 'guard-only recovery on the faulty tree', 25)
    files_after = sorted(str(p) for p in (root / 'chains').rglob('*') if p.is_file())
    say('guard-only deleted nothing', deleted=len(set(files_before) - set(files_after)),
        added=len(set(files_after) - set(files_before)))
    swap(env, 'stop_between_cycles', p3)
    b_guard = after_stop(env)
    swap(env, 'check_tails')
    start(env, t_new / 'w5', p2)
    watch_result('watch_swap "$B" swap back on the new tree', watch_after_start(env, p2, b_guard, 'swap'))
    watch(c, root, 'back on the new tree', 20)
    swap(env, 'stop_between_cycles', p2)
    driver.send_signal(signal.SIGTERM)
    driver.wait(30)
    say('done', driver=driver_state(root), guard_loop=guard_loop, new_loop=new_loop,
        bystander_alive=bystander.poll() is None)


if __name__ == '__main__':
    for sig in (signal.SIGALRM, signal.SIGTERM):  # an alarm or a TERM still runs the cleanup below
        signal.signal(sig, lambda n, f: sys.exit(f'stopped by signal {n}'))
    signal.alarm(1500)  # the rehearsal never outlives 25 minutes
    try:
        main()
    finally:  # nothing of the rehearsal outlives it
        for pid in STARTED:
            try:
                os.killpg(pid, signal.SIGTERM)
            except OSError:
                pass
