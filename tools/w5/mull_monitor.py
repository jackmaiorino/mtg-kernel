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

**Line** (`v` 1, compact JSON, about 0.45 KB per iteration with nine decks, so about 14 KB for a 32-update segment,
and one inode per segment, whatever the batch size): `it` (the iteration's index in its segment; the global update is the segment's manifest
offset plus `it`), `n` (episodes with an opening), `off` (the `london_keep_offset` every opening episode carried, null
when none did; `offs` lists the distinct values, null included, only if the episodes disagreed), `learned` (seats in
`learned` mode), `kept` (those seats by kept hand size, index 0 to 7), `exp` (their exposed pregame decisions),
`decks` (the same histogram per registered deck name), `fixed` (seats in `keep_seven` mode), and when nonzero `plain`
(episodes without an opening), `norec` (opening episodes without a record) and `fixed_not7` (a `keep_seven` seat
that did not keep seven: never expected). A failed iteration gets `{"v": 1, "it": k, "error": "..."}` instead, and a
later pass may replace it. The hook is idempotent by `it`, never raises into the reaper and never keeps a file.

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
LINE_VERSION = 1
SCHEMA = 'w5-mull-monitor/v1'
SIZES = 8  # a kept hand has 0 to 7 cards
MODES = ('learned', 'keep_seven')
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


def read_episode(data):
    """What the monitor needs of one trajectory file's bytes (plain or sealed): `{'opening': bool, 'offset': the
    episode's london_keep_offset or None, 'decks': [deck name per seat], 'seats': the opening record's seats or
    None}`. A trajectory holds thousands of decision rows, so only the `episode` and `opening_record` members are
    parsed (found by key, checked for their shape); anything that does not look right falls back to a full parse."""
    text = _qual().open_record(data).decode('utf-8')
    episode, record = None, None
    try:
        episode, record = _member(text, 'episode', False), _member(text, 'opening_record', True)
    except ValueError:
        episode = None
    if not (isinstance(episode, dict) and isinstance(episode.get('registered'), list) and 'id' in episode and
            'seed' in episode and (record is None or (isinstance(record, dict) and isinstance(record.get('seats'), list)))):
        doc = json.loads(text)
        episode, record = doc['episode'], doc.get('opening_record')
    return {'opening': episode.get('opening') is not None, 'offset': episode.get('london_keep_offset'),
            'decks': [str(r.get('label', '')).split('/')[0] for r in episode['registered']],
            'seats': None if record is None else record['seats']}


def check_seat(seat):
    """A seat of the opening record as the trainer validates it; ValueError otherwise."""
    kept, taken, exposed = seat.get('kept_hand_size'), seat.get('mulligans_taken'), seat.get('exposed_pregame_decisions')
    ok = (seat.get('mode') in MODES and all(type(v) is int for v in (kept, taken, exposed)) and 0 <= kept < SIZES
          and kept + taken == 7 and exposed >= 0 and seat.get('forced_zero_card_keep') is (taken == 7))
    if not ok:
        raise ValueError(f'an opening record seat is not a valid London seat: {seat}')


def iteration_line(it, episodes):
    """The log line (a dict) for iteration `it` from `episodes` (`read_episode` results)."""
    kept, decks = [0] * SIZES, {}
    learned = exposed = fixed = fixed_not7 = plain = norec = opened = 0
    offsets = []
    for e in episodes:
        if not e['opening']:
            plain += 1
            continue
        opened += 1
        offsets.append(e['offset'])
        if e['seats'] is None:
            norec += 1
            continue
        if len(e['seats']) != len(e['decks']):
            raise ValueError('the opening record and the registered decks disagree on the seat count')
        for seat, deck in zip(e['seats'], e['decks']):
            check_seat(seat)
            if seat['mode'] == 'learned':
                learned += 1
                exposed += seat['exposed_pregame_decisions']
                kept[seat['kept_hand_size']] += 1
                decks.setdefault(deck, [0] * SIZES)[seat['kept_hand_size']] += 1
            else:
                fixed += 1
                fixed_not7 += seat['kept_hand_size'] != 7
    values = sorted({o for o in offsets if o is not None})
    mixed = len(values) + (any(o is None for o in offsets)) > 1
    line = {'v': LINE_VERSION, 'it': it, 'n': opened, 'off': None if mixed or not values else values[0],
            'learned': learned, 'kept': kept, 'exp': exposed, 'fixed': fixed, 'decks': decks}
    for key, value in (('plain', plain), ('norec', norec), ('fixed_not7', fixed_not7)):
        if value:
            line[key] = value
    if mixed:
        line['offs'] = values + ([None] if any(o is None for o in offsets) else [])
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
            if type(it) is not int or doc.get('v') != LINE_VERSION:
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

def empty_sums():
    return {'iterations': 0, 'n': 0, 'plain': 0, 'norec': 0, 'learned': 0, 'kept': [0] * SIZES, 'exp': 0,
            'fixed': 0, 'fixed_not7': 0, 'decks': {}}


def add_line(sums, line):
    sums['iterations'] += 1
    for key in ('n', 'plain', 'norec', 'learned', 'exp', 'fixed', 'fixed_not7'):
        sums[key] += line.get(key, 0)
    for i, c in enumerate(line['kept']):
        sums['kept'][i] += c
    for deck, hist in line.get('decks', {}).items():
        mine = sums['decks'].setdefault(deck, [0] * SIZES)
        for i, c in enumerate(hist):
            mine[i] += c


def _ratio(a, b):
    return None if not b else round(a / b, 6)


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
    """The schedule's offset at a global update (None from its end on), or False if it cannot be computed."""
    try:
        return _qual().keep_offset_value(schedule, update)
    except ValueError:
        return False


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
        if want is not False and ('offs' in line or (None if line['off'] is None else _f32(line['off'])) != want):
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
    return out, sums, rows


def by_deck(sums):
    return {d: {k: v for k, v in _rates(h, sum(h)).items() if k in ('learned_seats', 'mulligan_rate',
                                                                    'zero_card_share', 'mean_kept')}
            for d, h in sorted(sums['decks'].items())}


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
        total['iterations'] += sums['iterations']
        for key in ('n', 'plain', 'norec', 'learned', 'exp', 'fixed', 'fixed_not7'):
            total[key] += sums[key]
        for i, c in enumerate(sums['kept']):
            total['kept'][i] += c
        for deck, hist in sums['decks'].items():
            mine = total['decks'].setdefault(deck, [0] * SIZES)
            for i, c in enumerate(hist):
                mine[i] += c
    report = {'schema': SCHEMA, 'chain': man.get('chain', cdir.name), 'arm_name': man.get('arm_name'),
              'keep_offset_schedule': schedule, 'segments_in_manifest': len(segs),
              'segments_read': [s['segment'] for s in segments], 'segments': segments,
              'total': dict(iterations_logged=total['iterations'], episodes=total['n'], plain_episodes=total['plain'],
                            fixed_seats=total['fixed'], fixed_seats_not_seven=total['fixed_not7'],
                            **rates(total), by_deck=by_deck(total))}
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


def format_text(report, decks=True):
    head = f'chain {report["chain"]}' + (f' ({report["arm_name"]})' if report.get('arm_name') else '')
    sched = report['keep_offset_schedule']
    head += ('; keep offset b0=%s, end_update=%s, %s' % (sched['b0'], sched['end_update'], sched['shape'])
             if sched else '; no keep offset in the recipe')
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
                    + _shares(s) + [f'{_num(s["offset_first"])}..{_num(s["offset_last"])}', '; '.join(notes)])
    t = report['total']
    rows.append(['all', '', t['iterations_logged'], t['learned_seats'], _pct(t['mulligan_rate']),
                 _pct(t['first_mulligan_share']), _pct(t['zero_card_share'], 2), _num(t['mean_kept'])] + _shares(t)
                + ['', ''])
    out.append(table(cols, rows))
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
                           _num(r['offset']), _num(r['expected_offset'])] for r in report['per_update']]))
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
