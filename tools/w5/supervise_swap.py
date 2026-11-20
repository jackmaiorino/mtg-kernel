"""Helpers of the supervisor swap steps (supervise_swap.sh; docs/W5-SUPERVISE-MEMORY-20261120.md). Not a pinned tool.

  supervise_swap.py pids PLAN                 the supervise loops and supervisors running exactly PLAN
  supervise_swap.py check-tails ROOT          every chain index's last line parses (a missing final newline is fine)
  supervise_swap.py cut-tail INDEX RECORDS    under the chain's index lock, cut an unparseable unterminated last line
  supervise_swap.py signal PLAN SIG PID...    signal through a pidfd, each process's argv checked after the pidfd opens
  supervise_swap.py alive PLAN PID...         the pids still a loop or supervisor for exactly PLAN
  supervise_swap.py guard-id ROOT             GUARD.json's id
  supervise_swap.py after-stop ROOT           T0=<float time> GID=<GUARD id>, recorded right after the stop
  supervise_swap.py watch ROOT GUARD_ID GUARD_ONLY_SECONDS [--mode swap|recovery] [--every 30] [--log LOG]
                    [--restarts RESTARTS_LOG] [--rounds N]
                                              the swap's state every 30 s: SUCCESS (0), ESCALATE (2, swap mode),
                                              DEADLINE (3, recovery mode), 1 after N rounds of none

Process identity (`pids`): a supervisor is a process whose argv is a python running `.../chains.py supervise` with
`--plan` followed by exactly PLAN (read from /proc/<pid>/cmdline, compared as strings, never as a pattern). Its loop
is its parent, if that parent is a bash loop for exactly PLAN: the step file's loop (`bash -c LOOP_TEXT swap-loop
PY W5 PLAN ...`, LOOP_TEXT exactly as in supervise_swap.sh) or the original start script's loop (`bash -c 'while true;
do ... chains.py supervise --plan PLAN ...'`, its words compared one by one). With no supervisor alive, a bash
process that is such a loop for exactly PLAN is a loop. Nothing else is ever listed. PLAN must be spelled exactly
as in the running process's argv (no relative form, no symlink alias, no `--plan=PATH`)."""
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
# LF; and the bytes that end a line for the index reader (str.splitlines: LF, CR, VT, FF, FS, GS, RS), so a
# line of only those is an empty line it skips. Spaces and tabs are not: a line of them fails to parse.
NEWLINE, BLANK = bytes([10]), bytes([10, 13, 11, 12, 28, 29, 30])


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


def argv_of(pid):
    """The argv of /proc/<pid> now, or None (gone, a zombie, unreadable)."""
    try:
        stat = Path(f'/proc/{pid}/stat').read_text()
        if stat[stat.rindex(')') + 2] == 'Z':
            return None
        argv = [a.decode('utf-8', 'replace') for a in Path(f'/proc/{pid}/cmdline').read_bytes().split(b'\0')[:-1]]
    except (OSError, ValueError, IndexError):
        return None
    return argv or None


def pidfd_signal(fd, sig):
    import signal
    signal.pidfd_send_signal(fd, sig)


def signal_verified(plan, sig, pids, opener=None, read_argv=argv_of, send=pidfd_signal, close=os.close):
    """Signals each pid only through a pidfd opened first, and only if the argv of that process, read after the pidfd
    is open, is a loop or a supervisor for exactly PLAN (the pidfd pins the process: its pid cannot be reused while
    the pidfd is open, so the process checked is the process signalled). Without pidfd support nothing is signalled.
    Returns (signalled pids, refused pids)."""
    opener = getattr(os, 'pidfd_open', None) if opener is None else opener
    sent, refused = [], []
    for pid in pids:
        if opener is None:
            print(f'not signalled: {pid}: this python has no pidfd_open (signals go only through a pidfd)',
                  file=sys.stderr)
            refused.append(pid)
            continue
        try:
            fd = opener(pid)
        except ProcessLookupError:
            continue  # already gone
        except OSError as exc:
            print(f'not signalled: {pid}: pidfd_open failed ({exc})', file=sys.stderr)
            refused.append(pid)
            continue
        try:
            argv = read_argv(pid)
            if argv is None:
                continue  # exited after the pidfd was opened
            if not (is_loop(argv, plan) or is_supervisor(argv, plan)):
                print(f'not signalled: {pid} is not a loop or supervisor for {plan}', file=sys.stderr)
                refused.append(pid)
                continue
            try:
                send(fd, sig)
                sent.append(pid)
            except ProcessLookupError:
                pass
        finally:
            close(fd)
    return sent, refused


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


def check_tails(root, block=BLOCK):
    """Each chain index's tail, read under the chain's index lock (a writer's append is never seen half done)."""
    sys.path.insert(0, str(HERE))
    import retention
    bad = 0
    for path in sorted(Path(root).glob('chains/*/chain.index.jsonl')):
        with retention.index_lock(path.with_name('chain.json')):
            state = tail_state(path, block)
        print(f'{state} {path}')
        bad += state == 'torn'
    return 1 if bad else 0


def cut_tail(index_file, records, block=BLOCK, before_lock=None):
    """Under the chain's index lock (the lock `retention.index_lock` takes), held from the first look through the
    truncate: if the last line is unterminated and does not parse, save it to `records` and cut the file back to
    the end of the line before it. A last line that parses is left (the next append adds its newline); a terminated
    line that does not parse is refused (not a torn append)."""
    sys.path.insert(0, str(HERE))
    import retention
    index_file = Path(index_file)
    chain_json = index_file.with_name('chain.json')
    if before_lock is not None:
        before_lock()
    with retention.index_lock(chain_json):
        start, line, terminated = last_line(index_file, block)
        if parses(line) or (not line and start == 0):
            print(f'nothing to cut: the last line of {index_file} parses')
            return 0
        if terminated:
            print(f'stop: the last line of {index_file} ends in a newline and does not parse: not a torn append')
            return 1
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

# The events after which a chain no longer waits, as chains.py logs them: the driver's `segment` (admitted),
# `no-room` (it gave up), `stop` (a STOP file) and `end`, and `run`'s `run-exit` and `run` (a driver ended or
# a new one started).
CLOSING = ('segment', 'no-room', 'stop', 'end', 'run-exit', 'run')


def live_chains(root):
    return sorted(p.parent.name for p in Path(root).glob('chains/*/chain.json')
                  if not (p.parent / 'END.json').exists())


class Waits:
    """Each chain's last driver event in LOG.jsonl, read incrementally (complete lines only: a line still being
    written is read on the next call). A chain waits when its last such event is `guard-wait` (drivers log it once,
    when a wait begins; any of `CLOSING` ends it), whenever that wait began, before the stop included."""

    def __init__(self, root):
        self.path = Path(root) / 'LOG.jsonl'
        self.offset, self.last, self.bad_lines = 0, {}, 0

    def read(self):
        """chain to ('waiting', since) or ('running', None); None when LOG.jsonl cannot be read."""
        try:
            with open(self.path, 'rb') as f:
                f.seek(self.offset)
                data = f.read()
        except OSError:
            return None
        end = data.rfind(b'\n') + 1
        for line in data[:end].splitlines():
            try:
                r = json.loads(line)
            except ValueError:
                self.bad_lines += 1
                continue
            event, chain = r.get('event'), r.get('chain')
            if chain and (event == 'guard-wait' or event in CLOSING):
                self.last[chain] = (event, r.get('unix'))
        self.offset += end
        return {c: ('waiting', unix) if event == 'guard-wait' else ('running', None)
                for c, (event, unix) in self.last.items()}


def admitted_of(cdir):
    """The chain's admission count (0 before its first, as `chains.admitted`), or 'unknown' if unreadable."""
    path = Path(cdir) / 'ADMITTED.json'
    if not path.exists():
        return 0
    doc = read_json(path)
    return doc['count'] if isinstance(doc, dict) and isinstance(doc.get('count'), int) else 'unknown'


def read_json(path):
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return None


def watch(root, guard_id, mode='swap', guard_only_seconds=723, every=30.0, log=None, rounds=None, now=time.time,
          restarts=None):
    """Prints the swap's state every `every` s.

    SUCCESS (exit 0): a GUARD.json whose `id` differs from `guard_id` (the one recorded after the stop), `ok` true,
    younger than GUARD_MAX_AGE, with a ceiling above the admitted count for every live chain, and no failure since
    the watch started: a Traceback in the supervise log, or a line in `restarts` (the loop logs every supervisor
    exit, a refusal that prints `stop:` or `refused:` without a Traceback included). Mode `swap`: ESCALATE (exit 2)
    on a failure, or at the earliest open guard wait + GUARD_WAIT - `guard_only_seconds` - 300 s. Mode `recovery`
    (after guard-only is started): no escalation; DEADLINE (exit 3) once the earliest open guard wait + GUARD_WAIT
    has passed (a driver has exited). Exit 1 after `rounds` rounds of none. What cannot be read is printed as
    unknown and read again next round."""
    sys.path.insert(0, str(HERE))
    import chains
    root = Path(root)
    log_path = Path(log) if log else None
    try:
        log_offset = log_path.stat().st_size if log_path else 0
    except OSError:
        log_offset = 0
    restarts_path = Path(restarts) if restarts else None
    try:
        restarts_offset = restarts_path.stat().st_size if restarts_path else 0
    except OSError:
        restarts_offset = 0
    waits = Waits(root)
    traceback, exits = False, []
    n = 0
    while True:
        t = now()
        g = read_json(root / 'GUARD.json')
        live = live_chains(root)
        admitted = {c: admitted_of(root / 'chains' / c) for c in live}
        state_of = waits.read()
        if log_path is not None and not traceback:
            try:
                with open(log_path, 'rb') as f:
                    f.seek(log_offset)
                    traceback = b'Traceback' in f.read()
            except OSError:
                pass
        if restarts_path is not None:
            try:
                with open(restarts_path, 'rb') as f:
                    f.seek(restarts_offset)
                    exits = [x.decode('utf-8', 'replace') for x in f.read().splitlines() if x.strip()]
            except OSError:
                pass
        failure = [x for x in (['Traceback in supervise.log'] if traceback else []) +
                   ([f'supervisor exits: {exits[-3:]}'] if exits else [])]
        ceilings = (g or {}).get('ceilings') or {}
        chains_out, open_since = {}, []
        for c in live:
            if state_of is None:
                wait = 'unknown (no LOG.jsonl)'
            elif c not in state_of:
                wait = 'unknown (no driver event for this chain)'
            elif state_of[c][0] == 'waiting':
                wait = state_of[c][1]
                if isinstance(wait, (int, float)):
                    open_since.append(wait)
                else:
                    wait = 'unknown (a guard-wait without its time)'
            else:
                wait = 'not waiting'
            ceiling = ceilings.get(c, 'unknown')
            chains_out[c] = {'admitted': admitted[c], 'ceiling': ceiling, 'guard_wait_since': wait,
                             'may_start': ceiling - admitted[c] if isinstance(ceiling, int)
                             and isinstance(admitted[c], int) else 'unknown'}
        first_exit = min(open_since) + chains.GUARD_WAIT if open_since else None
        escalate_at = first_exit - guard_only_seconds - 300 if first_exit is not None else None
        guard = 'unknown (GUARD.json unreadable)' if g is None else {
            'id_changed': g.get('id') != guard_id, 'ok': g.get('ok'), 'age_s': round(t - g.get('unix', 0), 1)}
        usable = g is not None and g.get('id') != guard_id and g.get('ok') is True and \
            t - g.get('unix', 0) < chains.GUARD_MAX_AGE and \
            all(isinstance(x['may_start'], int) and x['may_start'] > 0 for x in chains_out.values())
        if usable and not failure:
            verdict = 'SUCCESS'
        elif mode == 'swap' and (failure or (escalate_at is not None and t >= escalate_at)):
            verdict = 'ESCALATE'
        elif mode == 'recovery' and first_exit is not None and t >= first_exit:
            verdict = 'DEADLINE'
        else:
            verdict = 'WAIT'
        state = {'unix': round(t, 1), 'mode': mode, 'guard': guard, 'chains': chains_out, 'traceback': traceback,
                 'failure': failure,
                 'first_driver_exit_in_s': None if first_exit is None else round(first_exit - t, 1),
                 'escalate_in_s': None if escalate_at is None or mode != 'swap' else round(escalate_at - t, 1),
                 'log_lines_unparsed': waits.bad_lines, 'verdict': verdict}
        print(json.dumps(state, sort_keys=True), flush=True)
        n += 1
        if verdict != 'WAIT':
            return {'SUCCESS': 0, 'ESCALATE': 2, 'DEADLINE': 3}[verdict]
        if rounds is not None and n >= rounds:
            return 1
        time.sleep(every)


def alive(plan, pids):
    """The pids that are still a loop or a supervisor for exactly PLAN (gone, zombie or another process: not)."""
    return [p for p in pids if (lambda a: a is not None and (is_loop(a, plan) or is_supervisor(a, plan)))(argv_of(p))]


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    cmd, args = argv[0], argv[1:]
    if cmd == 'loop-text':
        print(LOOP_TEXT)
        return 0
    if cmd == 'signal':
        sent, refused = signal_verified(args[0], int(args[1]), [int(x) for x in args[2:]])
        print('SIGNALLED' + ''.join(f' {p}' for p in sent) + ' ; REFUSED' + ''.join(f' {p}' for p in refused))
        return 1 if refused else 0
    if cmd == 'alive':
        print(' '.join(str(p) for p in alive(args[0], [int(x) for x in args[1:]])))
        return 0
    if cmd == 'pids':
        loops, pys = select(process_table(), args[0])
        print('LOOP_PIDS' + ''.join(f' {p}' for p in loops) + ' ; PYTHON_PIDS' + ''.join(f' {p}' for p in pys))
        return 0
    if cmd == 'after-stop':  # one clock read, with the GUARD id after the stop
        t0 = time.time()
        g = read_json(Path(args[0]) / 'GUARD.json')
        if g is None or 'id' not in g:
            raise SystemExit('stop: GUARD.json has no readable id')
        print(f'T0={t0:.6f} GID={g["id"]}')
        return 0
    if cmd == 'guard-id':
        g = read_json(Path(args[0]) / 'GUARD.json')
        if g is None or 'id' not in g:
            raise SystemExit('stop: GUARD.json has no readable id')
        print(g['id'])
        return 0
    if cmd == 'check-tails':
        return check_tails(args[0])
    if cmd == 'cut-tail':
        return cut_tail(args[0], args[1])
    if cmd == 'watch':
        every = float(args[args.index('--every') + 1]) if '--every' in args else 30.0
        log = args[args.index('--log') + 1] if '--log' in args else None
        rounds = int(args[args.index('--rounds') + 1]) if '--rounds' in args else None
        mode = args[args.index('--mode') + 1] if '--mode' in args else 'swap'
        restarts = args[args.index('--restarts') + 1] if '--restarts' in args else None
        if mode not in ('swap', 'recovery'):
            raise SystemExit(f'unknown mode {mode}')
        return watch(args[0], int(args[1]), mode, float(args[2]), every, log, rounds, restarts=restarts)
    raise SystemExit(f'unknown step {cmd}')


if __name__ == '__main__':
    sys.exit(main())
