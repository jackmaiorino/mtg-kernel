"""Mulligan monitor for a London arm (collab LANES/mull-20261114 TOOLING-T5 item 8): a descriptive reading of what
the learned seats do in the pregame, per segment, with the keep offset in force.

It decides nothing. Nothing here stops, tunes or gates a chain, and nothing a controller reads is written: the
hook below appends to one log inside a scratch segment, and the reader only reads (its optional JSON goes outside
the chain directory).

**Source.** A trajectory (`collect/episode-*.json`) carries `episode` (`opening`, `london_keep_offset`, the
registered decks) and, for an episode with an `opening`, `opening_record` (per seat: `mode`, `mulligans_taken`,
`kept_hand_size`, `exposed_pregame_decisions`, `forced_zero_card_keep`). The scratch reaper deletes trajectories as
iterations complete, so an arm with `mull_monitor: true` (chains.py; scratch arms only) has `reap_iteration` call
`record_iteration` first: one line per iteration is appended to `<segment>/opening.jsonl` and nothing else is kept.

**Line** (`v` 2; `v` 1 lines, written by earlier tools, still read; compact JSON, about 0.9 KB per iteration with nine
decks, and one inode per segment, whatever the batch size): `it` (the iteration's index in its segment; the global
update is the segment's manifest offset plus `it`), `n` (episodes with an opening), `off` (the `london_keep_offset`
every opening episode carried: a number, or with a per-count lean vector the list of 7; null when none did; `offs`
lists the distinct values, null included, only if the episodes disagreed), `learned` (seats in
`learned` mode), `kept` (those seats by kept hand size, index 0 to 7), `exp` (their exposed pregame decisions),
`decks` (the same histogram per registered deck name), `fixed` (seats in `keep_seven` mode), and when nonzero `plain`
(episodes without an opening), `norec` (opening episodes without a record) and `fixed_not7` (a `keep_seven` seat
that did not keep seven: never expected). A failed iteration gets `{"v": 2, "it": k, "error": "..."}` instead, and a
later pass may replace it. The hook is idempotent by `it`, never raises into the reaper and never keeps a file.

**Per-count fields** (new in `v` 2; the number of mulligans the acting seat has already taken is its count k, 0 to 6):
`dk` (7 ints: announcements answered by learned seats at own count k), `ma` (7 ints: of those, the mulligans), `rk`
and `rn` (8 floats and 8 ints: the sum of the seat's terminal reward, and the number of seats counted, by kept hand
size 0 to 7; a seat whose game has no terminal reward is left out of both), and, read from the pregame decision rows
of the trajectory, `vn` (7 ints: announcements at count k whose row was read), `vk` (7 floats: the sum of the value
head's estimate at those announcements) and `vr` (7 floats: the sum of the terminal reward of the same seats), so the
head's pregame price at each count can be set against the realized result. `dk`, `ma`, `rk` and `rn` follow from the
opening record alone; `vn`, `vk` and `vr` need the rows (action column 63 is the acting seat's own count over 16, never
column 66, which is the other seat's) and are left out of a line whose rows could not be read (`annbad` counts the
episodes whose rows did not match their opening record).

**Reading** (`mull_monitor.py CHAIN_DIR`): per segment and in total, from the lines alone. Per learned seat, hand
sizes follow the London rule (`mulligans_taken` = 7 - `kept_hand_size`); a seat answers one announcement per
mulligan it takes and one more to keep, unless it is forced to keep zero cards (7 mulligans), so announcements =
mulligans + seats - zero-card seats, and the mulligan rate per announcement is mulligans / announcements (as
LANES/mull-20261114/analyze_probe.py). Also: the share of seats that mulligan the opening seven, the share that keep
zero cards, the kept-size distribution and mean, exposed pregame decisions per seat, and the offset in force. When the
chain's recipe carries a keep offset, the recorded offset of each update is compared with the schedule (descriptive:
the number of updates that differ is reported, nothing acts on it). Gaps in a segment's iterations are listed.

  mull_monitor.py CHAIN_DIR [--json OUT.json|-] [--per-update] [--last N] [--no-decks]
"""
import argparse
import json
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
LOG_NAME = 'opening.jsonl'
LINE_VERSION = 2
READ_VERSIONS = (1, 2)  # v1 lines (no per-count fields) still read
SCHEMA = 'w5-mull-monitor/v2'
SIZES = 8  # a kept hand has 0 to 7 cards
COUNTS = 7  # the acting seat's own mulligans taken at an announcement: 0 to 6 (a seventh forces a zero-card keep)
MODES = ('learned', 'keep_seven')
PREGAME_STATE_COLUMN = 89   # state column: 1.0 on every pregame decision (announcements and put-backs)
OWN_COUNT_COLUMN = 63       # action column: the acting seat's own mulligans taken over 16 (never column 66)
KIND_COLUMNS = (15, 69)     # action columns: (1, 1) is the mulligan row, (1, 0) the keep row of an announcement
_DECODER = json.JSONDecoder()


def _qual():
    """tools/ex1/qual.py (record envelopes, the keep-offset schedule); on the path already when chains.py runs."""
    ex1 = str(HERE.parent / 'ex1')
    if ex1 not in sys.path:
        sys.path.insert(0, ex1)
    import qual
    return qual


def opening_log_path(segment):
    return Path(segment) / LOG_NAME


# ---------------------------------------------------------------- reading one trajectory

def _member(text, key, last):
    """The value of the first (or last) object member `key` in the JSON text, parsed alone, or None. A match whose
    opening quote is escaped sits inside a string and is skipped."""
    needle = f'"{key}":'
    at = text.rfind(needle) if last else text.find(needle)
    while at > 0 and text[at - 1] == '\\':
        at = text.rfind(needle, 0, at) if last else text.find(needle, at + 1)
    if at < 0:
        return None
    at += len(needle)
    while at < len(text) and text[at] in ' \t\r\n':
        at += 1
    return _DECODER.raw_decode(text, at)[0]


def _memory_decoder():
    """Whether qual.open_record opens a sealed record in memory here (Python 3.14's compression.zstd or the
    zstandard package). Without one it runs the zstd tool through a temporary file per record, which the hook
    never does: see open_records."""
    try:
        from compression import zstd  # noqa: F401
        return True
    except ImportError:
        pass
    try:
        import zstandard  # noqa: F401
        return True
    except ImportError:
        return False


def open_records(files):
    """The record bytes of each trajectory file, in order. A plain file is its own bytes. Sealed files
    (`record_envelope` zstd1: one zstd frame of the record's exact bytes) are opened in memory where a decoder
    module exists. Where none does, ONE `zstd -d -c` call reads all of them from their own paths and its output is
    cut by each frame's declared content size: no file is created, and an iteration costs one process, not one per
    trajectory. It fails closed as qual.open_record does: one frame per file, a declared size within the record
    cap, output of exactly the declared lengths and nothing past them, content not itself sealed."""
    qual = _qual()
    files = [Path(f) for f in files]
    datas = [f.read_bytes() for f in files]
    sealed = [i for i, data in enumerate(datas) if data.startswith(qual.ZSTD_MAGIC)]
    if not sealed:
        return datas
    if _memory_decoder():
        return [qual.open_record(data) for data in datas]
    sizes = [qual.zstd_frame_content_size(datas[i]) for i in sealed]
    total = sum(sizes)
    try:
        proc = subprocess.Popen(['zstd', '-q', '-d', '-c', '--', *[str(files[i]) for i in sealed]],
                                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    except OSError as exc:  # no zstd tool: an error line, never mistaken for an empty iteration
        raise ValueError(f'no zstd decoder for the sealed trajectories ({type(exc).__name__})') from exc
    try:
        content = proc.stdout.read(total + 1)  # never more than one byte past the declared sizes
        if len(content) > total:
            proc.kill()
    finally:
        proc.stdout.close()
        code = proc.wait()
    if code != 0 or len(content) != total:
        raise ValueError('zstd could not decode the sealed trajectories to their declared sizes')
    at = 0
    for i, size in zip(sealed, sizes):
        datas[i] = content[at:at + size]
        at += size
        if datas[i].startswith(qual.ZSTD_MAGIC):
            raise ValueError('a record envelope holds unsealed record bytes only')
    return datas


def _tensor_float(x):
    """A tensor entry or value as a float: the trainer writes float32 values as their bit patterns (integers)."""
    if isinstance(x, bool) or not isinstance(x, (int, float)):
        raise ValueError(f'not a tensor number: {x!r}')
    if isinstance(x, int):
        if not 0 <= x < 1 << 32:
            raise ValueError(f'not a float32 bit pattern: {x!r}')
        return struct.unpack('<f', struct.pack('<I', x))[0]
    return x


def announcement_of(row):
    """`(actor, own count k, value)` of one pregame decision row if it is a keep-or-mulligan announcement, None for
    any other pregame row (a put-back). k is read from the own-count column of the action rows (column 63, the acting
    seat's own mulligans taken over 16, exactly k/16 in float32 for an integer k in 0 to 6; never column 66, the other
    seat's count), both rows agreeing; anything else raises ValueError. The value is the head's estimate on the row."""
    logits = row.get('logits')
    if not (isinstance(logits, list) and len(logits) == 2):
        return None
    flat = row['tensor']['action_features']
    if not isinstance(flat, list) or len(flat) % 2:
        raise ValueError('an announcement row without two action rows')
    width = len(flat) // 2
    if width <= max(OWN_COUNT_COLUMN, *KIND_COLUMNS):
        raise ValueError('an action row too narrow for the pregame columns')
    kinds = sorted((_tensor_float(flat[r * width + KIND_COLUMNS[0]]), _tensor_float(flat[r * width + KIND_COLUMNS[1]]))
                   for r in (0, 1))
    if kinds != [(1.0, 0.0), (1.0, 1.0)]:
        return None
    own = [_tensor_float(flat[r * width + OWN_COUNT_COLUMN]) for r in (0, 1)]
    k = round(own[0] * 16)
    if own[0] != own[1] or not 0 <= k < COUNTS or own[0] != struct.unpack('<f', struct.pack('<f', k / 16))[0]:
        raise ValueError(f'the own-count column of an announcement is {own}, not k/16 for one k in 0..{COUNTS - 1}')
    actor = row['actor']
    if type(actor) is not int or actor not in (0, 1):
        raise ValueError(f'a pregame row of actor {actor!r}')
    return actor, k, _tensor_float(row['value'])


def pregame_announcements(text):
    """The announcements `(actor, k, value)` of a trajectory's text, in order, read from its leading pregame decision
    rows only (state column 89 equal to 1.0); the first other row ends the scan, so a trajectory of thousands of
    rows costs a handful of row parses. None when the decision rows cannot be read."""
    at = text.find('"decisions":')
    at = text.find('[', at) if at >= 0 else -1
    if at < 0:
        return None
    at += 1
    out = []
    while at < len(text):
        while at < len(text) and text[at] in ' \t\r\n,':
            at += 1
        if at >= len(text) or text[at] == ']':
            break
        row, at = _DECODER.raw_decode(text, at)
        tensor = row.get('tensor') if isinstance(row, dict) else None
        state = tensor.get('state') if isinstance(tensor, dict) else None
        if not isinstance(state, list):
            return None
        if len(state) <= PREGAME_STATE_COLUMN or _tensor_float(state[PREGAME_STATE_COLUMN]) != 1.0:
            break
        found = announcement_of(row)
        if found is not None:
            out.append(found)
    return out


def _reward_of(terminal):
    reward = terminal.get('terminal_reward') if isinstance(terminal, dict) else None
    ok = isinstance(reward, list) and reward and all(isinstance(r, (int, float)) and not isinstance(r, bool)
                                                     for r in reward)
    return [float(r) for r in reward] if ok else None


def read_episode(data):
    """What the monitor needs of one trajectory file's bytes (plain or sealed): `{'opening': bool, 'offset': the
    episode's london_keep_offset (a number, a list of 7, or None), 'decks': [deck name per seat], 'seats': the opening
    record's seats or None, 'reward': the terminal reward per seat or None, 'ann': the pregame announcements
    (`announcement_of`) or None where they could not be read}`. A trajectory holds thousands of decision rows, so only
    the `episode`, `opening_record` and `terminal` members and the leading pregame rows are parsed (found by key,
    checked for their shape); anything that does not look right falls back to a full parse (the announcements then
    stay None if the rows are unreadable; they are never a reason to refuse the episode)."""
    text = _qual().open_record(data).decode('utf-8')
    episode, record, terminal = None, None, None
    try:
        episode, record = _member(text, 'episode', False), _member(text, 'opening_record', True)
        terminal = _member(text, 'terminal', True)
    except ValueError:
        episode = None
    if not (isinstance(episode, dict) and isinstance(episode.get('registered'), list) and 'id' in episode and
            'seed' in episode and (record is None or (isinstance(record, dict) and isinstance(record.get('seats'), list)))):
        doc = json.loads(text)
        episode, record, terminal = doc['episode'], doc.get('opening_record'), doc.get('terminal')
    try:
        ann = pregame_announcements(text) if episode.get('opening') is not None else []
    except (ValueError, KeyError, TypeError):
        ann = None
    return {'opening': episode.get('opening') is not None, 'offset': episode.get('london_keep_offset'),
            'decks': [str(r.get('label', '')).split('/')[0] for r in episode['registered']],
            'seats': None if record is None else record['seats'], 'reward': _reward_of(terminal), 'ann': ann}


def check_seat(seat):
    """A seat of the opening record as the trainer validates it; ValueError otherwise."""
    kept, taken, exposed = seat.get('kept_hand_size'), seat.get('mulligans_taken'), seat.get('exposed_pregame_decisions')
    ok = (seat.get('mode') in MODES and all(type(v) is int for v in (kept, taken, exposed)) and 0 <= kept < SIZES
          and kept + taken == 7 and exposed >= 0 and seat.get('forced_zero_card_keep') is (taken == 7))
    if not ok:
        raise ValueError(f'an opening record seat is not a valid London seat: {seat}')


def _offset_key(o):
    """An episode's offset as a comparable value: a number stays, a per-count list becomes a tuple, None stays."""
    return tuple(o) if isinstance(o, list) else o


def _offset_json(o):
    return list(o) if isinstance(o, tuple) else o


def _announcements_match(seats, ann):
    """Whether the announcement rows are exactly those the opening record implies: per learned seat, one row at each
    count 0 to min(mulligans taken, 6), in order, and no row of any other seat."""
    seen = {}
    for actor, k, _ in ann:
        seen.setdefault(actor, []).append(k)
    want = {i: list(range(min(7 - s['kept_hand_size'], COUNTS - 1) + 1)) for i, s in enumerate(seats)
            if s['mode'] == 'learned'}
    return seen == want


def iteration_line(it, episodes):
    """The log line (a dict) for iteration `it` from `episodes` (`read_episode` results; the keys `reward` and `ann`
    may be missing, which leaves their fields at zero)."""
    kept, decks = [0] * SIZES, {}
    learned = exposed = fixed = fixed_not7 = plain = norec = opened = 0
    dk, ma, vn = [0] * COUNTS, [0] * COUNTS, [0] * COUNTS
    vk, vr = [0.0] * COUNTS, [0.0] * COUNTS
    rk, rn = [0.0] * SIZES, [0] * SIZES
    norew = annbad = 0
    offsets = []
    for e in episodes:
        if not e['opening']:
            plain += 1
            continue
        opened += 1
        offsets.append(_offset_key(e['offset']))
        if e['seats'] is None:
            norec += 1
            continue
        if len(e['seats']) != len(e['decks']):
            raise ValueError('the opening record and the registered decks disagree on the seat count')
        reward = e.get('reward')
        for i, (seat, deck) in enumerate(zip(e['seats'], e['decks'])):
            check_seat(seat)
            if seat['mode'] == 'learned':
                learned += 1
                exposed += seat['exposed_pregame_decisions']
                size, taken = seat['kept_hand_size'], seat['mulligans_taken']
                kept[size] += 1
                decks.setdefault(deck, [0] * SIZES)[size] += 1
                # a seat answers at counts 0..taken (the keep), unless forced to zero cards: then at 0..6, all mulligans
                for k in range(min(taken, COUNTS - 1) + 1):
                    dk[k] += 1
                for k in range(min(taken, COUNTS)):
                    ma[k] += 1
                if reward is not None and i < len(reward):
                    rk[size] += reward[i]
                    rn[size] += 1
                else:
                    norew += 1
            else:
                fixed += 1
                fixed_not7 += seat['kept_hand_size'] != 7
        if 'ann' in e:
            if e['ann'] is None or reward is None or not _announcements_match(e['seats'], e['ann']):
                annbad += 1
            else:
                for actor, k, value in e['ann']:
                    vn[k] += 1
                    vk[k] += value
                    vr[k] += reward[actor]
    values = sorted({o for o in offsets if o is not None}, key=lambda o: o if isinstance(o, tuple) else (o,))
    mixed = len(values) + (any(o is None for o in offsets)) > 1
    line = {'v': LINE_VERSION, 'it': it, 'n': opened, 'off': None if mixed or not values else _offset_json(values[0]),
            'learned': learned, 'kept': kept, 'exp': exposed, 'fixed': fixed, 'decks': decks,
            'dk': dk, 'ma': ma, 'rk': [round(x, 6) for x in rk], 'rn': rn, 'vn': vn,
            'vk': [round(x, 6) for x in vk], 'vr': [round(x, 6) for x in vr]}
    for key, value in (('plain', plain), ('norec', norec), ('fixed_not7', fixed_not7), ('norew', norew),
                       ('annbad', annbad)):
        if value:
            line[key] = value
    if mixed:
        line['offs'] = [_offset_json(o) for o in values] + ([None] if any(o is None for o in offsets) else [])
    return line


# ---------------------------------------------------------------- the hook (called by chains.reap_iteration)

def read_log(path):
    """`(lines, errors, bad)`: the last line of each iteration by `it`, the error text of each iteration whose last
    line is an error, and the number of unreadable lines (a torn last line, say). A missing file reads as empty."""
    lines, errors, bad = {}, {}, 0
    try:
        raw = Path(path).read_bytes()
    except FileNotFoundError:
        return lines, errors, bad
    for chunk in raw.split(b'\n'):
        if not chunk.strip():
            continue
        try:
            doc = json.loads(chunk)
            it = doc['it']
            if type(it) is not int or doc.get('v') not in READ_VERSIONS:
                raise ValueError('not a monitor line')
        except (ValueError, KeyError, TypeError):
            bad += 1
            continue
        if 'error' in doc:
            errors[it] = doc['error']
            lines.pop(it, None)
        else:
            lines[it] = doc
            errors.pop(it, None)
    return lines, errors, bad


def _is_link(path):
    p = Path(path)
    return p.is_symlink() or (hasattr(p, 'is_junction') and p.is_junction())


def append_line(path, line):
    """One compact line appended (a torn last line first gets its newline, so it stays one unreadable line). The
    log must be an ordinary file of its own: a link (a symlink or junction), a directory or a file with a second
    hard link (a checkpoint, say) is refused with an OSError, so the monitor never appends to anything else. The
    segment directory holding it must not be a link either. Directories above the segment are the plan's (the
    scratch root, where the trainer writes every file of the segment) and are not this function's to judge."""
    if _is_link(Path(path).parent):
        raise OSError(f'{Path(path).parent} is a link; the monitor writes only into a plain segment directory')
    if _is_link(path) or (os.path.lexists(path) and not (os.path.isfile(path) and os.stat(path).st_nlink == 1)):
        raise OSError(f'{path} is not an ordinary file of its own; the monitor appends to nothing else')
    data = (json.dumps(line, sort_keys=True, separators=(',', ':')) + '\n').encode('utf-8')
    try:
        with open(path, 'rb') as f:
            f.seek(-1, os.SEEK_END)
            if f.read(1) != b'\n':
                data = b'\n' + data
    except OSError:  # no file yet, or an empty one
        pass
    flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT | getattr(os, 'O_NOFOLLOW', 0) | getattr(os, 'O_BINARY', 0)
    fd = os.open(path, flags, 0o666)  # never through a link planted since the check above (where the OS has the flag)
    with os.fdopen(fd, 'ab') as f:
        f.write(data)


def trajectory_files(iteration):
    """The trajectories of a completed iteration's attempt (the one its complete.json names), in order."""
    iteration = Path(iteration)
    complete = json.loads((iteration / 'complete.json').read_text(encoding='utf-8'))
    name = [p for p in str(complete['update']['path']).replace('\\', '/').split('/') if p.startswith('attempt-')][0]
    return sorted((iteration / name / 'collect').glob('episode-*.json'))


def read_pregame_scale(iteration):
    """The scaled-step fields of one iteration's update receipt (`pregame_announcement_step`, written only under the
    trainer's `pregame_announcement_step_scale`): `{'s', 'n', 'ga', 'gr'}` (the scale, the announcement rows whose
    policy term went to the second gradient buffer, and the L2 norms of that buffer and of the rest), or None (no
    scale, no receipt, or anything unreadable: the monitor never fails on it)."""
    try:
        complete = json.loads((Path(iteration) / 'complete.json').read_text(encoding='utf-8'))
        update = json.loads(Path(complete['update']['path']).read_text(encoding='utf-8'))
        receipt = update.get('pregame_announcement_step') or {}
        if receipt.get('scale') is None:
            return None
        pas = {'s': receipt['scale'], 'n': receipt['routed_groups']}
        for key, name in (('ga', 'announcement_gradient_l2'), ('gr', 'rest_gradient_l2')):
            value = receipt.get(name)
            pas[key] = None if value is None else float(f'{value:.6g}')
        return pas
    except (OSError, ValueError, KeyError, TypeError):
        return None


def record_iteration(iteration, log_path):
    """Appends the line of one completed iteration to `log_path`, once. Returns 'recorded', 'skipped' (already in
    the log, or not an iteration directory), 'empty' (no trajectories left: nothing is written), 'error' (an error
    line was written: the data was unreadable) or 'failed' (the log could not be written). It never raises and
    never touches the iteration: the caller deletes the trajectories whatever the outcome."""
    iteration = Path(iteration)
    try:
        it = int(iteration.name)
    except ValueError:
        return 'skipped'
    try:
        if it in read_log(log_path)[0]:
            return 'skipped'
    except OSError:
        return 'failed'
    try:
        files = trajectory_files(iteration)
        if not files:
            return 'empty'
        line = iteration_line(it, [read_episode(data) for data in open_records(files)])
        pas = read_pregame_scale(iteration)
        if pas is not None:  # absent otherwise: every line of an unscaled run keeps its bytes
            line['pas'] = pas
        status = 'recorded'
    except FileNotFoundError:
        return 'empty'
    except Exception as exc:  # a monitor never stops the reaper: any unreadable iteration becomes an error line
        line, status = {'v': LINE_VERSION, 'it': it, 'error': f'{type(exc).__name__}: {exc}'[:160]}, 'error'
    try:
        append_line(log_path, line)
    except OSError:
        return 'failed'
    return status


def record_all(segment, log_path):
    """`record_iteration` for every completed iteration of a segment directory (persist's pass, which also reaches
    the newest iteration, whose trajectories the reaper keeps and persist then drops); returns counts by status."""
    counts = {}
    its = Path(segment) / 'iterations'
    for d in sorted(p for p in its.iterdir() if (p / 'complete.json').exists()) if its.exists() else []:
        status = record_iteration(d, log_path)
        counts[status] = counts.get(status, 0) + 1
    return counts


# ---------------------------------------------------------------- reading the logs

COUNT_LISTS = (('dk', COUNTS), ('ma', COUNTS), ('vn', COUNTS), ('vk', COUNTS), ('vr', COUNTS), ('rk', SIZES),
               ('rn', SIZES))


def empty_sums():
    sums = {'iterations': 0, 'n': 0, 'plain': 0, 'norec': 0, 'learned': 0, 'kept': [0] * SIZES, 'exp': 0,
            'fixed': 0, 'fixed_not7': 0, 'decks': {}, 'v2_iterations': 0, 'norew': 0, 'annbad': 0,
            'pas_iterations': 0, 'pas_rows': 0, 'pas_ga_sum': 0.0, 'pas_gr_sum': 0.0}
    sums.update({key: [0] * size for key, size in COUNT_LISTS})
    return sums


def add_line(sums, line):
    sums['iterations'] += 1
    for key in ('n', 'plain', 'norec', 'learned', 'exp', 'fixed', 'fixed_not7', 'norew', 'annbad'):
        sums[key] += line.get(key, 0)
    for i, c in enumerate(line['kept']):
        sums['kept'][i] += c
    for deck, hist in line.get('decks', {}).items():
        mine = sums['decks'].setdefault(deck, [0] * SIZES)
        for i, c in enumerate(hist):
            mine[i] += c
    pas = line.get('pas')
    if pas and pas.get('n'):  # a line of an update run under pregame_announcement_step_scale
        sums['pas_iterations'] += 1
        sums['pas_rows'] += pas['n']
        sums['pas_ga_sum'] += pas.get('ga') or 0.0
        sums['pas_gr_sum'] += pas.get('gr') or 0.0
    if 'dk' in line:  # a v2 line: the per-count fields (a v1 line has none and adds nothing)
        sums['v2_iterations'] += 1
        for key, size in COUNT_LISTS:
            for i, c in enumerate(line.get(key) or [0] * size):
                sums[key][i] += c


def merge_sums(total, sums):
    """`sums` (one segment's tally) added into `total`."""
    total['iterations'] += sums['iterations']
    for key in ('n', 'plain', 'norec', 'learned', 'exp', 'fixed', 'fixed_not7', 'v2_iterations', 'norew', 'annbad',
                'pas_iterations', 'pas_rows', 'pas_ga_sum', 'pas_gr_sum'):
        total[key] += sums[key]
    for key, size in (('kept', SIZES),) + COUNT_LISTS:
        for i in range(size):
            total[key][i] += sums[key][i]
    for deck, hist in sums['decks'].items():
        mine = total['decks'].setdefault(deck, [0] * SIZES)
        for i, c in enumerate(hist):
            mine[i] += c


def _ratio(a, b):
    return None if not b else round(a / b, 6)


def by_count(sums):
    """Per own count k (mulligans the acting seat has already taken): announcements answered by learned seats, the
    mulligans among them, the share that mulligan again, announcements per update, and, where the rows were read, the
    value head's mean estimate against the mean realized terminal reward of the same seats. Only v2 lines count."""
    n = sums['v2_iterations']
    out = []
    for k in range(COUNTS):
        d, m, v = sums['dk'][k], sums['ma'][k], sums['vn'][k]
        out.append({'count': k, 'decisions': d, 'mulligans': m, 'mulligan_again_share': _ratio(m, d),
                    'decisions_per_update': _ratio(d, n), 'value_rows': v, 'mean_value': _ratio(sums['vk'][k], v),
                    'mean_realized': _ratio(sums['vr'][k], v)})
    return out


def ret_by_size(sums):
    """Per kept hand size 0 to 7: learned seats counted and their mean terminal reward (v2 lines only)."""
    return [{'size': s, 'n': sums['rn'][s], 'mean_return': _ratio(sums['rk'][s], sums['rn'][s])}
            for s in range(SIZES)]


def rates(sums):
    """The descriptive numbers of a tally (None where the denominator is zero)."""
    return _rates(sums['kept'], sums['learned'], sums['exp'])


def _rates(kept, seats, exposed=None):
    mulligans = sum((7 - k) * c for k, c in enumerate(kept))
    announcements = mulligans + seats - kept[0]
    out = {'learned_seats': seats, 'announcements': announcements, 'mulligans': mulligans,
           'mulligan_rate': _ratio(mulligans, announcements),
           'first_mulligan_share': _ratio(seats - kept[SIZES - 1], seats), 'zero_card_share': _ratio(kept[0], seats),
           'mean_kept': _ratio(sum(k * c for k, c in enumerate(kept)), seats),
           'kept_count': list(kept), 'kept_share': [_ratio(c, seats) for c in kept]}
    if exposed is not None:
        out['exposed_per_seat'] = _ratio(exposed, seats)
    return out


def _f32(x):
    return struct.unpack('<f', struct.pack('<f', x))[0]


def expected_offset(schedule, update):
    """The schedule's offset at a global update (None from its end on; a list of 7 for a per-count schedule), or
    False if it cannot be computed."""
    try:
        return _qual().keep_offset_value(schedule, update)
    except ValueError:
        return False


def _as_f32(off):
    """A logged offset as the float32 value it was written from (numbers and lists alike; None stays None)."""
    if off is None:
        return None
    return [_f32(x) for x in off] if isinstance(off, list) else _f32(off)


def summarize_segment(seg_dir, offset, planned, schedule, per_update):
    """`(summary, sums, rows)` for one segment directory: its log read, not its trajectories (which are gone)."""
    seg_dir = Path(seg_dir)
    lines, errors, bad = read_log(opening_log_path(seg_dir))
    complete = (seg_dir / 'completion.json').exists()
    span = range(planned) if complete and isinstance(planned, int) else range(max(lines.keys() | errors.keys()) + 1) \
        if lines or errors else range(0)
    sums, rows, mismatch, mixed = empty_sums(), [], 0, 0
    for it in sorted(lines):
        line = lines[it]
        add_line(sums, line)
        update = None if offset is None else offset + it
        mixed += 'offs' in line
        want = expected_offset(schedule, update) if schedule is not None and update is not None else False
        if want is not False and ('offs' in line or _as_f32(line['off']) != want):
            mismatch += 1
        if per_update:
            row = {'update': update, 'segment': seg_dir.name, 'it': it, 'offset': line['off'],
                   'expected_offset': None if want is False else want, 'episodes': line['n']}
            row.update({k: v for k, v in _rates(line['kept'], line['learned'], line['exp']).items()
                        if k not in ('kept_count', 'kept_share', 'announcements', 'mulligans')})
            rows.append(row)
    first, last = (lines[min(lines)]['off'], lines[max(lines)]['off']) if lines else (None, None)
    out = {'segment': seg_dir.name, 'first_update': offset, 'last_update': None if offset is None or not lines
           else offset + max(lines), 'planned_updates': planned, 'complete': complete,
           'log_present': opening_log_path(seg_dir).exists(), 'iterations_logged': len(lines),
           'missing_iterations': [i for i in span if i not in lines and i not in errors],
           'error_iterations': sorted(errors), 'unreadable_lines': bad, 'episodes': sums['n'],
           'plain_episodes': sums['plain'], 'episodes_without_record': sums['norec'], 'offset_first': first,
           'offset_last': last, 'offset_mixed_iterations': mixed,
           'schedule_mismatch_iterations': None if schedule is None else mismatch,
           'fixed_seats': sums['fixed'], 'fixed_seats_not_seven': sums['fixed_not7']}
    out.update(rates(sums))
    out['by_deck'] = by_deck(sums)
    out['by_count'] = by_count(sums)
    out['ret_by_size'] = ret_by_size(sums)
    out['v2_iterations'] = sums['v2_iterations']
    return out, sums, rows


def by_deck(sums):
    return {d: {k: v for k, v in _rates(h, sum(h)).items() if k in ('learned_seats', 'mulligan_rate',
                                                                    'zero_card_share', 'mean_kept')}
            for d, h in sorted(sums['decks'].items())}


def lean_in_force(schedule, segments):
    """For a per-count schedule (`b0` a list of 7): the newest logged update, the 7 values its trajectories carried
    and the 7 the schedule gives there (None from the end update on); None for a scalar or absent schedule."""
    if not (isinstance(schedule, dict) and isinstance(schedule.get('b0'), list)):
        return None
    for s in reversed(segments):
        if s['last_update'] is not None:
            want = expected_offset(schedule, s['last_update'])
            return {'update': s['last_update'], 'logged': s['offset_last'], 'schedule': None if want is False else want,
                    'equal': None if want is False else _as_f32(s['offset_last']) == want}
    return None


def summarize_chain(chain_dir, per_update=False, last=None):
    """The report (a dict) for a chain directory: `chain.json` names the segments and their offsets."""
    cdir = Path(chain_dir)
    man_path = cdir / 'chain.json'
    if not man_path.is_file():
        raise ValueError(f'{man_path} is not a chain manifest')
    man = json.loads(man_path.read_text(encoding='utf-8'))
    segs = man.get('segments') or []
    schedule = (man.get('recipe') or {}).get('keep_offset')
    chosen = list(enumerate(segs))[-last:] if last else list(enumerate(segs))
    total, rows, segments = empty_sums(), [], []
    for s, seg in chosen:
        name = Path(str(seg.get('root') or f'seg-{s:05d}')).name
        summary, sums, seg_rows = summarize_segment(cdir / name, seg.get('offset'), seg.get('updates'), schedule,
                                                    per_update)
        summary['index'] = s
        segments.append(summary)
        rows += seg_rows
        merge_sums(total, sums)
    report = {'schema': SCHEMA, 'chain': man.get('chain', cdir.name), 'arm_name': man.get('arm_name'),
              'keep_offset_schedule': schedule, 'pregame_advantage': (man.get('recipe') or {}).get('pregame_advantage'),
              'segments_in_manifest': len(segs),
              'segments_read': [s['segment'] for s in segments], 'segments': segments,
              'total': dict(iterations_logged=total['iterations'], episodes=total['n'], plain_episodes=total['plain'],
                            fixed_seats=total['fixed'], fixed_seats_not_seven=total['fixed_not7'],
                            v2_iterations=total['v2_iterations'], episodes_without_reward_seats=total['norew'],
                            episodes_with_unmatched_rows=total['annbad'], by_count=by_count(total),
                            ret_by_size=ret_by_size(total), **rates(total), by_deck=by_deck(total))}
    if total['pas_iterations']:  # absent for an unscaled run: its report keeps its keys
        rows_scaled = total['pas_rows']
        report['pregame_scale'] = {'scale': (man.get('recipe') or {}).get('pregame_announcement_step_scale'),
                                   'updates': total['pas_iterations'], 'announcement_rows': rows_scaled,
                                   'announcement_gradient_l2_mean': total['pas_ga_sum'] / total['pas_iterations'],
                                   'rest_gradient_l2_mean': total['pas_gr_sum'] / total['pas_iterations']}
    force = lean_in_force(schedule, segments)
    if force is not None:
        report['lean_vector_in_force'] = force
    if per_update:
        report['per_update'] = rows
    return report


# ---------------------------------------------------------------- text

def _pct(x, digits=1):
    return '-' if x is None else f'{100 * x:.{digits}f}'


def _num(x):
    return '-' if x is None else f'{x:.4g}'


def table(headers, rows):
    """Right-aligned columns, two spaces apart."""
    cells = [headers] + rows
    widths = [max(len(str(r[i])) for r in cells) for i in range(len(headers))]
    return '\n'.join('  '.join(str(v).rjust(w) for v, w in zip(r, widths)).rstrip() for r in cells)


def _shares(r):
    s = r['kept_share']
    low = None if r['learned_seats'] == 0 else sum(r['kept_count'][:4]) / r['learned_seats']
    return [_pct(s[7]), _pct(s[6]), _pct(s[5]), _pct(s[4]), _pct(low)]


def _offcell(x):
    """A logged offset in the segment table: a number, or entry 0 of a per-count vector marked with a star."""
    return f'{_num(x[0])}*' if isinstance(x, list) else _num(x)


def format_by_count(report):
    """The per-count table, the kept-size return table and the lean vector in force (v2 lines only)."""
    t = report['total']
    if not t.get('v2_iterations'):
        return ['per count: no v2 lines in the segments read']
    out = [f'per own count k (mulligans already taken), {t["v2_iterations"]} v2 updates read',
           table(['k', 'decisions', 'per update', 'mulligans', 'again %', 'value rows', 'mean V', 'mean realized'],
                 [[r['count'], r['decisions'], _num(r['decisions_per_update']), r['mulligans'],
                   _pct(r['mulligan_again_share']), r['value_rows'], _num(r['mean_value']), _num(r['mean_realized'])]
                  for r in t['by_count']]),
           'terminal return by kept hand size (learned seats)',
           table(['size', 'seats', 'mean return'], [[r['size'], r['n'], _num(r['mean_return'])]
                                                    for r in t['ret_by_size']])]
    if t.get('episodes_with_unmatched_rows') or t.get('episodes_without_reward_seats'):
        out.append(f'episodes whose pregame rows did not match their record: {t.get("episodes_with_unmatched_rows", 0)};'
                   f' learned seats without a terminal reward: {t.get("episodes_without_reward_seats", 0)}')
    scale = report.get('pregame_scale')
    if scale:
        out.append(f'pregame announcement step scale {_num(scale["scale"])}: {scale["announcement_rows"]} announcement '
                   f'rows in {scale["updates"]} updates, mean gradient L2 '
                   f'{_num(scale["announcement_gradient_l2_mean"])} (announcement buffer) and '
                   f'{_num(scale["rest_gradient_l2_mean"])} (rest)')
    force = report.get('lean_vector_in_force')
    if force:
        fmt = lambda v: '-' if v is None else '[' + ', '.join(_num(x) for x in v) + ']'  # noqa: E731
        out.append(f'lean vector in force at update {force["update"]}: logged {fmt(force["logged"])}, '
                   f'schedule {fmt(force["schedule"])}, equal {force["equal"]}')
    return out


def format_text(report, decks=True):
    head = f'chain {report["chain"]}' + (f' ({report["arm_name"]})' if report.get('arm_name') else '')
    sched = report['keep_offset_schedule']
    head += ('; keep offset b0=%s, end_update=%s, %s' % (sched['b0'], sched['end_update'], sched['shape'])
             if sched else '; no keep offset in the recipe')
    if report.get('pregame_advantage'):
        head += f'; pregame advantage {report["pregame_advantage"]}'
    out = [head, f'{len(report["segments"])} of {report["segments_in_manifest"]} segments read, '
           f'{report["total"]["iterations_logged"]} iterations logged']
    cols = ['seg', 'updates', 'iters', 'seats', 'mull/ann %', 'mull@7 %', 'zero %', 'mean kept', 'k7 %', 'k6 %', 'k5 %',
            'k4 %', 'k<=3 %', 'offset first..last', 'notes']
    rows = []
    for s in report['segments']:
        notes = []
        if not s['log_present']:
            notes.append('no log')
        if not s['complete'] and s is report['segments'][-1]:
            notes.append('in flight or cut')
        for key, label in (('missing_iterations', 'missing'), ('error_iterations', 'errors')):
            if s[key]:
                notes.append(f'{label} {len(s[key])}')
        if s['unreadable_lines']:
            notes.append(f'unreadable {s["unreadable_lines"]}')
        if s['schedule_mismatch_iterations']:
            notes.append(f'offset differs from schedule {s["schedule_mismatch_iterations"]}')
        if s['fixed_seats_not_seven']:
            notes.append(f'keep_seven seats not at 7: {s["fixed_seats_not_seven"]}')
        span = '-' if s['last_update'] is None else f'{s["first_update"]}..{s["last_update"]}'
        rows.append([s['segment'], span, s['iterations_logged'], s['learned_seats'], _pct(s['mulligan_rate']),
                     _pct(s['first_mulligan_share']), _pct(s['zero_card_share'], 2), _num(s['mean_kept'])]
                    + _shares(s) + [f'{_offcell(s["offset_first"])}..{_offcell(s["offset_last"])}', '; '.join(notes)])
    t = report['total']
    rows.append(['all', '', t['iterations_logged'], t['learned_seats'], _pct(t['mulligan_rate']),
                 _pct(t['first_mulligan_share']), _pct(t['zero_card_share'], 2), _num(t['mean_kept'])] + _shares(t)
                + ['', ''])
    out.append(table(cols, rows))
    out.extend(format_by_count(report))
    if decks and t['by_deck']:
        out.append('by deck (all segments read): seats, mulligan/announcement %, zero-card %, mean kept')
        out.append(table(['deck', 'seats', 'mull/ann %', 'zero %', 'mean kept'],
                         [[d, v['learned_seats'], _pct(v['mulligan_rate']), _pct(v['zero_card_share'], 2),
                           _num(v['mean_kept'])] for d, v in t['by_deck'].items()]))
    if 'per_update' in report:
        out.append('per update')
        out.append(table(['update', 'seg', 'seats', 'mull/ann %', 'mull@7 %', 'zero %', 'mean kept', 'offset',
                          'schedule'],
                         [[r['update'], r['segment'], r['learned_seats'], _pct(r['mulligan_rate']),
                           _pct(r['first_mulligan_share']), _pct(r['zero_card_share'], 2), _num(r['mean_kept']),
                           _offcell(r['offset']), _offcell(r['expected_offset'])] for r in report['per_update']]))
    return '\n'.join(out)


def write_json(path, report, chain_dir):
    """The report as JSON at `path`, never inside the chain directory (nothing a controller reads is written)."""
    target = Path(path).resolve()
    root = Path(chain_dir).resolve()
    if target == root or root in target.parents:
        raise ValueError(f'{path} is inside the chain directory; the monitor writes nothing there')
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=f'.{target.name}.', dir=target.parent)
    try:
        with os.fdopen(fd, 'w', encoding='utf-8', newline='\n') as f:
            f.write(json.dumps(report, indent=1, sort_keys=True) + '\n')
        os.replace(tmp, target)
    except BaseException:
        Path(tmp).unlink(missing_ok=True)
        raise


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('chain_dir', help='a chain directory (chains/<name>, holding chain.json)')
    ap.add_argument('--json', metavar='OUT', help='write the report as JSON to OUT (outside the chain directory), '
                    'or to stdout with `-` (then no text table)')
    ap.add_argument('--per-update', action='store_true', help='add one row per update')
    ap.add_argument('--last', type=int, metavar='N', help='read only the last N segments')
    ap.add_argument('--no-decks', action='store_true', help='leave the per-deck table out of the text')
    a = ap.parse_args(argv)
    try:
        report = summarize_chain(a.chain_dir, a.per_update, a.last)
        if a.json == '-':
            print(json.dumps(report, indent=1, sort_keys=True))
            return 0
        if a.json:
            write_json(a.json, report, a.chain_dir)
    except (ValueError, OSError) as exc:
        print(f'mull_monitor: {exc}', file=sys.stderr)
        return 2
    print(format_text(report, decks=not a.no_decks))
    return 0


if __name__ == '__main__':
    sys.exit(main())
