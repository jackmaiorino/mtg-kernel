"""Helpers of the supervisor swap steps (supervise_swap.sh; docs/W5-SUPERVISE-MEMORY-20261120.md). Not a pinned tool.

  supervise_swap.py pids PLAN                 the supervise loops and supervisors running exactly PLAN
  supervise_swap.py check-tails ROOT          every chain index's last line parses (a missing final newline is fine)
  supervise_swap.py cut-tail INDEX RECORDS    under the chain's index lock, cut an unparseable unterminated last line
  supervise_swap.py watch ROOT PLAN T0 GUARD_ONLY_SECONDS [--every 30] [--log SUPERVISE_LOG] [--rounds N]
                                              the swap's state every 30 s, until success or escalation (exit 0 or 2;
                                              1 after N rounds of neither)

Process identity (`pids`): a supervisor is a process whose argv is a python running `.../chains.py supervise` with
`--plan` followed by exactly PLAN (read from /proc/<pid>/cmdline, compared as strings, never as a pattern). Its loop
is its parent, if that parent is a bash loop for exactly PLAN: the step file's loop (`bash -c LOOP_TEXT swap-loop
PY W5 PLAN ...`, LOOP_TEXT exactly as in supervise_swap.sh) or the original start script's loop (`bash -c 'while true;
do ... chains.py supervise --plan PLAN ...'`, its words compared one by one). With no supervisor alive, a bash
process that is such a loop for exactly PLAN is a loop. Nothing else is ever listed."""
import json
import os
import shlex
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
LOOP_TEXT = ('while true; do "$1" "$2/chains.py" supervise --plan "$3" --loop "$4" ${7:+"$7"} >> "$5/supervise.log" 2>&1; '
             'rc=$?; echo "$(date -Is) supervise exited $rc" >> "$5/supervise-restarts.log"; sleep "$6"; done')
BLOCK = 1 << 16
NEWLINE, BLANK = bytes([10]), bytes([32, 9, 13, 10])  # LF; blank bytes at a line's end (space, tab, CR, LF)


# ---------------------------------------------------------------- process identity

def is_supervisor(argv, plan):
    """argv runs `python .../chains.py supervise ... --plan PLAN` with exactly this plan."""
    if len(argv) < 5 or not Path(argv[0]).name.startswith('python'):
        return False
    args = argv[1:]
    while args and args[0].startswith('-') and args[0] != '--plan':  # interpreter options (-B, -u)
        args = args[1:]
    if not args or Path(args[0]).name != 'chains.py' or len(args) < 2 or args[1] != 'supervise':
        return False
    rest = args[2:]
    return any(rest[i] == '--plan' and rest[i + 1] == plan for i in range(len(rest) - 1))


def is_loop(argv, plan):
    """argv is a supervise restart loop for exactly this plan: the step file's, or the original start script's."""
    if len(argv) < 3 or Path(argv[0]).name != 'bash' or argv[1] != '-c':
        return False
    if argv[2] == LOOP_TEXT:
        return len(argv) >= 7 and argv[6] == plan  # bash -c TEXT $0 $1=PY $2=W5 $3=PLAN
    text = argv[2]
    if len(argv) != 3 or not text.startswith('while true; do '):
        return False
    try:
        words = shlex.split(text[len('while true; do '):].split(';')[0].split('>>')[0])
    except ValueError:
        return False
    return is_supervisor(words, plan)


def select(table, plan):
    """(loops, supervisors) for exactly PLAN from a process table: pid to (ppid, argv)."""
    pys = sorted(pid for pid, (_, argv) in table.items() if is_supervisor(argv, plan))
    loops = set()
    for pid in pys:
        parent = table[pid][0]
        if parent in table and is_loop(table[parent][1], plan):
            loops.add(parent)
    loops |= {pid for pid, (_, argv) in table.items() if is_loop(argv, plan)}
    return sorted(loops), pys


def process_table():
    table = {}
    for d in Path('/proc').iterdir():
        if not d.name.isdigit():
            continue
        try:
            argv = [a.decode('utf-8', 'replace') for a in (d / 'cmdline').read_bytes().split(b'\0')[:-1]]
            stat = (d / 'stat').read_text()
            if stat[stat.rindex(')') + 2] == 'Z':
                continue
            ppid = int(stat[stat.rindex(')') + 2:].split()[1])
        except (OSError, ValueError, IndexError):
            continue
        if argv:
            table[int(d.name)] = (ppid, argv)
    return table


# ---------------------------------------------------------------- index tails

def last_line(path, block=BLOCK):
    """(start offset of the last non-blank line, its bytes, whether a newline follows it), read backward in blocks
    of `block` bytes (never the whole file). Trailing blank lines are skipped, as index() skips them."""
    with open(path, 'rb') as f:
        pos = f.seek(0, 2)
        end, terminated, tail = None, False, b''
        while pos > 0:
            step = min(block, pos)
            pos -= step
            f.seek(pos)
            chunk = f.read(step)
            if end is None:
                body = chunk.rstrip(BLANK)
                if not body:
                    terminated = terminated or NEWLINE in chunk
                    continue
                terminated = terminated or NEWLINE in chunk[len(body):]
                end, chunk = pos + len(body), body
            cut = chunk.rfind(NEWLINE)
            if cut >= 0:
                return pos + cut + 1, chunk[cut + 1:] + tail, terminated
            tail = chunk + tail
        return (0, tail, terminated) if end is not None else (0, b'', terminated)


def parses(data):
    try:
        json.loads(data.decode('utf-8'))
        return True
    except ValueError:
        return False


def tail_state(path, block=BLOCK):
    """'ok' (the last line parses, with or without its newline: an append adds a missing one), 'empty', or 'torn'
    (an unparseable last line: neither index() nor the cache can read the file)."""
    start, line, terminated = last_line(path, block)
    if not line and start == 0:
        return 'empty'
    return 'ok' if parses(line) else 'torn'


def check_tails(root):
    bad = 0
    for path in sorted(Path(root).glob('chains/*/chain.index.jsonl')):
        state = tail_state(path)
        print(f'{state} {path}')
        bad += state == 'torn'
    return 1 if bad else 0


def cut_tail(index_file, records, block=BLOCK, before_lock=None):
    """Under the chain's index lock (the lock `retention.index_lock` takes), held through the truncate: if the last
    line is still unparseable, save it to `records` and cut the file back to the end of the line before it."""
    sys.path.insert(0, str(HERE))
    import retention
    index_file = Path(index_file)
    chain_json = index_file.with_name('chain.json')
    if tail_state(index_file, block) != 'torn':
        print(f'nothing to cut: the last line of {index_file} parses')
        return 0
    if last_line(index_file, block)[2]:
        print(f'stop: the last line of {index_file} ends in a newline and does not parse: not a torn append')
        return 1
    if before_lock is not None:
        before_lock()
    with retention.index_lock(chain_json):
        start, line, terminated = last_line(index_file, block)
        if parses(line) or (not line and start == 0) or terminated:
            print(f'nothing to cut under the lock: the last line of {index_file} is whole now')
            return 0
        size = os.path.getsize(index_file)
        saved = Path(records) / f'torn-tail-{index_file.parent.name}-{time.strftime("%Y%m%dT%H%M%S")}.bin'
        with open(index_file, 'rb') as f:
            f.seek(start)
            torn = f.read(size - start)
        with open(saved, 'xb') as out:
            out.write(torn)
            out.flush()
            os.fsync(out.fileno())
        with open(index_file, 'r+b') as f:
            f.truncate(start)
            f.flush()
            os.fsync(f.fileno())
        print(f'cut {index_file} from {size} to {start} bytes; the {size - start} cut bytes are in {saved}')
    return 0


# ---------------------------------------------------------------- watching the swap

def live_chains(root):
    return sorted(p.parent.name for p in Path(root).glob('chains/*/chain.json')
                  if not (p.parent / 'END.json').exists())


def guard_waits(root, since, offset):
    """Each chain's open guard wait (its `guard-wait` event after `since` not followed by a `segment` event) from
    LOG.jsonl, read from byte `offset` on."""
    waits = {}
    path = Path(root) / 'LOG.jsonl'
    if not path.exists():
        return waits
    with open(path, 'rb') as f:
        f.seek(offset)
        for line in f:
            try:
                r = json.loads(line)
            except ValueError:
                continue
            if (r.get('unix') or 0) < since:
                continue
            if r.get('event') == 'guard-wait':
                waits.setdefault(r.get('chain'), r['unix'])
            elif r.get('event') == 'segment':
                waits.pop(r.get('chain'), None)
    return waits


def watch(root, plan, t0, guard_only_seconds, every=30.0, log=None, rounds=None, now=time.time):
    """Prints the swap's state every `every` s; returns 0 on success (a GUARD written after T0, ok, with a ceiling
    for every live chain), 2 at the escalation time (the earliest open guard wait + GUARD_WAIT - the measured
    guard-only start - 300 s) or on a Traceback in the supervise log written after the watch started."""
    sys.path.insert(0, str(HERE))
    import chains
    root = Path(root)
    log_path = Path(log) if log else None
    log_offset = log_path.stat().st_size if log_path and log_path.exists() else 0
    events = root / 'LOG.jsonl'
    events_offset = max(0, events.stat().st_size - (8 << 20)) if events.exists() else 0
    n = 0
    while True:
        g = json.loads((root / 'GUARD.json').read_text())
        chains_live = live_chains(root)
        admitted = {c: chains.admitted(root / 'chains' / c) for c in chains_live}
        ceilings = g.get('ceilings', {})
        waits = guard_waits(root, t0, events_offset)
        escalate_at = min(waits.values()) + chains.GUARD_WAIT - guard_only_seconds - 300 if waits else None
        traceback = False
        if log_path and log_path.exists():
            with open(log_path, 'rb') as f:
                f.seek(log_offset)
                traceback = b'Traceback' in f.read()
        t = now()
        success = g['unix'] > t0 and g.get('ok') is True and all(c in ceilings for c in chains_live)
        state = {'unix': round(t, 1), 'guard_age_s': round(t - g['unix'], 1), 'guard_ok': g.get('ok'),
                 'guard_after_t0': g['unix'] > t0,
                 'chains': {c: {'admitted': admitted[c], 'ceiling': ceilings.get(c),
                                'may_start': None if c not in ceilings else ceilings[c] - admitted[c],
                                'guard_wait_since': waits.get(c)} for c in chains_live},
                 'escalate_in_s': None if escalate_at is None else round(escalate_at - t, 1),
                 'traceback': traceback}
        verdict = 'SUCCESS' if success else 'ESCALATE' if traceback or (escalate_at and t >= escalate_at) else 'WAIT'
        print(json.dumps(dict(state, verdict=verdict), sort_keys=True), flush=True)
        n += 1
        if verdict != 'WAIT':
            return 0 if verdict == 'SUCCESS' else 2
        if rounds is not None and n >= rounds:
            return 1
        time.sleep(every)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    cmd, args = argv[0], argv[1:]
    if cmd == 'loop-text':
        print(LOOP_TEXT)
        return 0
    if cmd == 'pids':
        loops, pys = select(process_table(), args[0])
        print('LOOP_PIDS' + ''.join(f' {p}' for p in loops) + ' ; PYTHON_PIDS' + ''.join(f' {p}' for p in pys))
        return 0
    if cmd == 'check-tails':
        return check_tails(args[0])
    if cmd == 'cut-tail':
        return cut_tail(args[0], args[1])
    if cmd == 'watch':
        every = float(args[args.index('--every') + 1]) if '--every' in args else 30.0
        log = args[args.index('--log') + 1] if '--log' in args else None
        rounds = int(args[args.index('--rounds') + 1]) if '--rounds' in args else None
        return watch(args[0], args[1], float(args[2]), float(args[3]), every, log, rounds)
    raise SystemExit(f'unknown step {cmd}')


if __name__ == '__main__':
    sys.exit(main())
