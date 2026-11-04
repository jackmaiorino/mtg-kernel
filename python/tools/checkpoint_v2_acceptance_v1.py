"""Pre-registered 16-run acceptance measurement for making checkpoint container v2 the default (design 7.2).

The frozen design is DESIGN-FORMAT.md revision 10, section 7.2 (collaboration lane bookkeeping-format-review-20261031;
container PR #468). This harness implements it and nothing else: every constant below is the design's, and no
reading of contrasts, counters or ordering bits can move a start, a block or a decision.

Commands (run on the measurement host, inside one host_slots claim on the pool cores):

  prepare --spec SPEC.json --root ROOT          stage the 16 runs' v1 and v2 configs and the converted opponents
  run --root ROOT --attempt 1|2                 run one attempt to its recorded outcome (RESULT.json)
  summary --root ROOT --attempt N               print the attempt's state, calibration and result

The estimand. Both arms run the same binary (pinned by hash); arm A with `record_format` v1 and its opponents as v1
checkpoints, arm B with `record_format` v2 and its opponents as v2 containers made by the converter (whose receipts are
pinned). Every block runs the same 16 start checkpoints for one 18-update segment with the same config seeds, so every
block of an attempt must reach the same `after_state_sha256` per run and update; a mismatch voids the attempt as a
defect. Throughput of a block is the sum over runs of 17 x 3600 / (wall of updates 1 to 17), each update's wall being
the gap between consecutive `complete.json` publications (the convention of design 7.1).

Replication. The unit is the couple: an ABBA and a BAAB group back to back, in an order given by a fair bit. At the
attempt's start 64 bits are drawn from the operating system's generator and written to MANIFEST.json before any block;
bit k goes to the k-th couple started (calibration couples first, then comparison couples). A pair is two adjacent
blocks (1, 2) and (3, 4) of a group, with log ratio ln(B / A); a couple's contrast is the mean of its 4 pair logs.

Phases. One warm-up group (ABBA, unused; a contaminated warm-up is rerun before calibration, at most twice). Then
calibration (A against A, both arms v1): 4 clean couples; a couple with a contaminated block is discarded and rerun at
the end with the next bit; a third discard stops the attempt. From the 4 contrasts: the 90% t interval of their mean
must contain 0 (validity), and n is the least of 6 to 10 with t(0.95, n-1) s / sqrt(n) <= ln(1.05) (precision). The D:
read baseline (median over the surviving calibration blocks) and n are written to MANIFEST.json before any v2 block.
Comparison: n couples, never replaced; any contaminated comparison block makes the attempt inconclusive. Accept only
if exp(lower 90% t bound of the mean contrast) > 1.03 and the exact sign-flip guard on d_i = c_i - ln(1.03) gives
p <= 0.025 (and the 6.2 correspondence receipt, pinned in the spec, passed).

Contamination (frozen). A block is contaminated when, over its span, the artifact disk (E:) averages more than 50%
busy, the run disk (D:) averages more than 60 MB/s of reads (or, in comparison, more than twice the larger of the
calibration baseline and a 10 MB/s floor: amendment 1, docs/checkpoint_v2_acceptance_amendment_1.md), another
host_slots claim or a timed hold overlaps the pool cores (4-23 since amendment 2,
docs/checkpoint_v2_acceptance_amendment_2.md), or more than 10% of the sampler's rows are missing. The claim check
polls `host_slots status` every 15 s; a block with a poll gap over 60 s counts as contaminated, because overlap could
not be excluded.

Start precondition. A group starts at the first instant, after the previous group's prune and settle (120 s with no
harness writes), at which the trailing 10 minutes show: E: under 50% busy and D: reads under 60 MB/s (window means,
with at least 90% of the sampler's rows present), no other claim overlapping the pool at any poll, no poll gap over
60 s, and room on the run disk for one group plus the reserve. Nothing else delays a start.

Interruptions. An interruption (a crash, a reboot, a lost claim, a failed run) inside a calibration or comparison
couple makes the attempt inconclusive; inside the warm-up it counts as a contaminated warm-up. A restarted `run`
detects the unfinished group from STATE.json, applies the same retention to its partial outputs, and records that.
The bits are never redrawn. Attempt 2 (the one permitted re-measurement) runs only after attempt 1 ended
inconclusive, with its own 64 bits, and is analysed alone.

Byte budget (ARTIFACT-LAW). One block writes about runs x updates x 57 MB (16.4 GB at full scale). After each group,
and only between groups, both arms' outputs are pruned the same way: episode files and every checkpoint except the
group's last per run are deleted, and every receipt and log is kept. After each couple (and the warm-up) the retained
artifacts move to the artifact root with a move manifest (MOVES.jsonl). Budget, prune and move records and the settle
intervals are in the attempt's records.

Checkpoint fields are read only through `checkpoint_container_v1.py` (design 4.1); the arm guard reads only the first
8 bytes of a published checkpoint to confirm its format.

`--validation` runs the same protocol at reduced scale (fewer runs, shorter segments, shorter precondition and settle);
its records say so and its outcome is never a decision.
"""
from __future__ import annotations

import argparse
import copy
from fractions import Fraction
import hashlib
import itertools
import json
import math
import os
import re
import secrets
import shutil
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import checkpoint_container_v1 as checkpoint_reader  # noqa: E402

SCHEMA = 'mtg-kernel-checkpoint-v2-acceptance/v1'

# ------------------------------------------------------------------ frozen constants (design 7.2, revision 10)

RUNS = 16
RUNS_PER_BASE = {'c': 8, 'v8': 8}
UPDATES = 18
STAGGER_SECONDS = 2.0
# Amendment 2 (docs/checkpoint_v2_acceptance_amendment_2.md): the pool moves from the design's 0-19 to 4-23, the
# 20 cores left free beside a long GPU campaign on 0-3. Only foreign claims overlapping this pool count as overlap.
POOL_CORES = '4-23'
RUN_DISK = 'D'
ARTIFACT_DISK = 'E'
PRECONDITION_SECONDS = 600.0
SETTLE_SECONDS = 120.0
E_BUSY_MAX = 50.0
D_READ_MAX = 60e6
MISSING_MAX = 0.10
RELATIVE_FACTOR = 2.0
# Amendment 1 (docs/checkpoint_v2_acceptance_amendment_1.md): the relative rule's baseline is at least 10 MB/s, so
# page-cache misses of the measurement's own inputs cannot void comparison blocks.
D_READ_BASELINE_FLOOR = 10e6
MARGIN = math.log(1.03)
PRECISION = math.log(1.05)
CALIBRATION_COUPLES = 4
N_MIN, N_MAX = 6, 10
MAX_DISCARDS = 2
MAX_WARMUPS = 3
ORDER_BITS = 64
GUARD_ALPHA = 0.025
SAMPLE_INTERVAL = 5
CLAIM_POLL_SECONDS = 15.0
CLAIM_GAP_MAX = 60.0
BYTES_PER_UPDATE = 57e6
COLLECTION_WORKERS = 4
MAX_ATTEMPTS = 2
BLOCK_TIMEOUT_SECONDS = 3600.0
# A block is scored only after the sampler has read a row stamped at or after its end (rows reach the reader late
# under load; attempt 1 of 7.2 scored two blocks before their last rows arrived). Past this wait the missing rows count.
SAMPLER_CATCHUP_SECONDS = 120.0
# t(0.95, df): the 90% two-sided interval.
T95 = {3: 2.353363, 4: 2.131847, 5: 2.015048, 6: 1.943180, 7: 1.894579, 8: 1.859548, 9: 1.833113}
BELOW_NORMAL_PRIORITY_CLASS = 0x00004000
PATTERNS = ('ABBA', 'BAAB')


class Refused(RuntimeError):
    pass


def require(ok, message):
    if not ok:
        raise Refused(message)


# ------------------------------------------------------------------ small file helpers

def sha256_file(path) -> str:
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def pin(path) -> dict:
    path = Path(path)
    return {'path': path.as_posix(), 'sha256': sha256_file(path)}


def check_pin(entry, what) -> Path:
    path = Path(entry['path'])
    require(path.is_file(), f'{what}: {path} is missing')
    require(sha256_file(path) == entry['sha256'], f'{what}: {path} differs from its pinned sha256')
    return path


def read_json(path):
    with open(path, encoding='utf-8') as f:
        return json.load(f)


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + '.tmp')
    with open(tmp, 'w', encoding='utf-8', newline='\n') as f:
        json.dump(value, f, indent=1, sort_keys=False)
        f.write('\n')
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)
    return pin(path)


def append_jsonl(path, row):
    with open(path, 'a', encoding='utf-8', newline='\n') as f:
        f.write(json.dumps(row, sort_keys=True) + '\n')


def now_iso():
    return datetime.now().astimezone().isoformat(timespec='seconds')


def parse_cores(spec: str) -> set[int]:
    out = set()
    for part in spec.split(','):
        part = part.strip()
        if '-' in part:
            lo, hi = (int(x) for x in part.split('-'))
            out.update(range(min(lo, hi), max(lo, hi) + 1))
        elif part:
            out.add(int(part))
    return out


# ------------------------------------------------------------------ statistics (pure)

def t95(df: int) -> float:
    require(df in T95, f'no t quantile for {df} degrees of freedom')
    return T95[df]


def pair_logs(blocks) -> list[float]:
    """The two pair log ratios ln(B / A) of one group of 4 blocks [(label, throughput), ...], pairs (1, 2), (3, 4)."""
    require(len(blocks) == 4, 'a group has 4 blocks')
    logs = []
    for first, second in (blocks[0:2], blocks[2:4]):
        by = {first[0]: first[1], second[0]: second[1]}
        require(set(by) == {'A', 'B'}, 'a pair holds one A and one B block')
        logs.append(math.log(by['B'] / by['A']))
    return logs


def couple_contrast(group1, group2) -> float:
    return statistics.fmean(pair_logs(group1) + pair_logs(group2))


def mean_interval(values) -> dict:
    n = len(values)
    require(n >= 2, 'an interval needs at least 2 contrasts')
    m = statistics.fmean(values)
    s = statistics.stdev(values)
    half = t95(n - 1) * s / math.sqrt(n)
    return {'n': n, 'mean': m, 'sd': s, 'df': n - 1, 't': t95(n - 1), 'lower': m - half, 'upper': m + half}


def choose_n(s: float):
    """The least n from 6 to 10 with t(0.95, n-1) x s / sqrt(n) <= ln(1.05); None when no such n exists."""
    for n in range(N_MIN, N_MAX + 1):
        if t95(n - 1) * s / math.sqrt(n) <= PRECISION:
            return n
    return None


def sign_flip_p(d) -> float:
    """Exact one-sided sign-flip p: 2^-n x #{e in {-1, +1}^n : sum e_i d_i >= sum d_i}, ties counted."""
    exact = [Fraction(x) for x in d]  # exact binary values: no rounding makes distinct sums ties, or ties distinct
    total = sum(exact)
    hits = sum(1 for signs in itertools.product((1, -1), repeat=len(exact))
               if sum(e * x for e, x in zip(signs, exact)) >= total)
    return hits / 2 ** len(d)


def calibrate(contrasts) -> dict:
    require(len(contrasts) == CALIBRATION_COUPLES, f'calibration has {CALIBRATION_COUPLES} couple contrasts')
    iv = mean_interval(contrasts)
    valid = iv['lower'] <= 0.0 <= iv['upper']
    n = choose_n(iv['sd'])
    return dict(iv, contrasts=list(contrasts), valid=valid, n=n, precision_met=n is not None,
                passed=valid and n is not None)


def decide(contrasts) -> dict:
    iv = mean_interval(contrasts)
    lower_ratio = math.exp(iv['lower'])
    d = [c - MARGIN for c in contrasts]
    p = sign_flip_p(d)
    interval_ok = lower_ratio > 1.03
    guard_ok = p <= GUARD_ALPHA
    return dict(iv, contrasts=list(contrasts), geometric_mean_ratio=math.exp(iv['mean']), lower_ratio=lower_ratio,
                upper_ratio=math.exp(iv['upper']), sign_flip_p=p, interval_condition=interval_ok,
                guard_condition=guard_ok, accept=interval_ok and guard_ok)


# ------------------------------------------------------------------ the schedule (pure state machine)

def new_state(bits) -> dict:
    require(len(bits) == ORDER_BITS and all(b in (0, 1) for b in bits), f'{ORDER_BITS} order bits')
    return {'bits': list(bits), 'next_bit': 0, 'warmups': [], 'calibration_couples': [], 'calibration': None,
            'comparison_couples': [], 'decision': None, 'outcome': None, 'in_group': None, 'groups_run': 0}


def couple_patterns(bit: int) -> list[str]:
    """Bit 1: ABBA first; bit 0: BAAB first."""
    return ['ABBA', 'BAAB'] if bit == 1 else ['BAAB', 'ABBA']


def _open_couple(state, phase):
    couples = state[f'{phase}_couples']
    if couples and len(couples[-1]['groups']) < 2 and not couples[-1].get('aborted'):
        return couples[-1]
    return None


def next_group(state):
    """The next group to run, as {'phase', 'pattern', 'couple'}; None when the attempt has an outcome."""
    if state['outcome'] is not None:
        return None
    if not any(not w['contaminated'] for w in state['warmups']):
        return {'phase': 'warmup', 'pattern': 'ABBA', 'couple': None}
    for phase in ('calibration', 'comparison'):
        if phase == 'comparison' and state['calibration'] is None:
            break
        couple = _open_couple(state, phase)
        if couple is not None:
            return {'phase': phase, 'pattern': couple['patterns'][len(couple['groups'])],
                    'couple': len(state[f'{phase}_couples']) - 1}
        clean = [c for c in state[f'{phase}_couples'] if not c['discarded']]
        target = CALIBRATION_COUPLES if phase == 'calibration' else state['calibration']['n']
        if len(clean) < target:
            require(state['next_bit'] < len(state['bits']), 'the order bits are exhausted')
            bit = state['bits'][state['next_bit']]
            state['next_bit'] += 1
            state[f'{phase}_couples'].append({'bit': bit, 'bit_index': state['next_bit'] - 1,
                                              'patterns': couple_patterns(bit), 'groups': [],
                                              'discarded': False, 'contrast': None})
            return {'phase': phase, 'pattern': state[f'{phase}_couples'][-1]['patterns'][0],
                    'couple': len(state[f'{phase}_couples']) - 1}
    raise Refused('no next group, yet no outcome: the state is inconsistent')


def finish(state, outcome, reason, **extra):
    state['outcome'] = dict({'outcome': outcome, 'reason': reason}, **extra)


def record_group(state, group) -> None:
    """Record one finished group: {'phase', 'pattern', 'couple', 'blocks': [{'label', 'throughput', 'contaminated',
    'reasons', 'd_read_mean'}...]}. Applies the frozen discard, stop and decision rules."""
    state['groups_run'] += 1
    phase = group['phase']
    contaminated = any(b['contaminated'] for b in group['blocks'])
    if phase == 'warmup':
        state['warmups'].append({'contaminated': contaminated, 'group': group['index']})
        if contaminated and len(state['warmups']) >= MAX_WARMUPS:
            finish(state, 'inconclusive', 'a third contaminated warm-up')
        return
    couples = state[f'{phase}_couples']
    couple = couples[group['couple']]
    couple['groups'].append(group['index'])
    couple.setdefault('blocks', []).append([[b['label'], b['throughput']] for b in group['blocks']])
    couple['contaminated'] = couple.get('contaminated', False) or contaminated
    if phase == 'comparison' and contaminated:
        couple['aborted'] = True
        finish(state, 'inconclusive', 'a contaminated comparison block')
        return
    if len(couple['groups']) < 2:
        return
    if couple['contaminated']:
        couple['discarded'] = True
        if sum(c['discarded'] for c in couples) > MAX_DISCARDS:
            finish(state, 'inconclusive', f'more than {MAX_DISCARDS} calibration couples discarded')
        return
    couple['contrast'] = couple_contrast(couple['blocks'][0], couple['blocks'][1])
    clean = [c for c in couples if not c['discarded']]
    if phase == 'calibration' and len(clean) == CALIBRATION_COUPLES:
        state['calibration'] = calibrate([c['contrast'] for c in clean])
        if not state['calibration']['passed']:
            why = 'validity' if not state['calibration']['valid'] else 'precision'
            finish(state, 'inconclusive', f'the calibration failed its {why} condition')
    elif phase == 'comparison' and len(clean) == state['calibration']['n']:
        state['decision'] = decide([c['contrast'] for c in clean])
        finish(state, 'accept' if state['decision']['accept'] else 'not-accepted',
               'both frozen conditions hold' if state['decision']['accept'] else 'a frozen condition fails')


def interrupted(state) -> None:
    """A group was running when the harness stopped: inside a couple the attempt is inconclusive; inside the warm-up
    it counts as a contaminated warm-up."""
    group = state['in_group']
    state['in_group'] = None
    if group['phase'] == 'warmup':
        state['warmups'].append({'contaminated': True, 'group': group['index'], 'interrupted': True})
        if len(state['warmups']) >= MAX_WARMUPS:
            finish(state, 'inconclusive', 'a third contaminated warm-up (interrupted)')
        return
    finish(state, 'inconclusive', f'interrupted during a {group["phase"]} group', group=group['index'])


def couple_open(state) -> bool:
    for phase in ('calibration', 'comparison'):
        couples = state[f'{phase}_couples']
        if couples and len(couples[-1]['groups']) == 1 and not couples[-1].get('aborted'):
            return True
    return False


# ------------------------------------------------------------------ preparation

def _paths(value):
    """Every pinned file ({path, sha256}) inside a config value."""
    if isinstance(value, dict):
        if set(value) >= {'path', 'sha256'} and isinstance(value.get('path'), str):
            yield value
        for v in value.values():
            yield from _paths(v)
    elif isinstance(value, list):
        for v in value:
            yield from _paths(v)


def map_path(path: str, path_map) -> str:
    for old, new in path_map or []:
        if path.replace('\\', '/').lower().startswith(old.lower()):
            return new + path.replace('\\', '/')[len(old):]
    return path


def episode_seed(run: str, update: int, episode: int) -> int:
    digest = hashlib.sha256(f'{SCHEMA}/run/{run}/u{update}/e{episode}'.encode()).digest()
    return int.from_bytes(digest[:8], 'little') & ((1 << 63) - 1)


def run_config(base, run, start, offset, updates, collection_workers):
    """The run's v1 segment config: the base's keys in order, its schedule slice, per-run episode ids and seeds that
    do not depend on the arm or the block, the start checkpoint, a placeholder output directory."""
    cfg = {}
    for key, value in base.items():
        if key == 'iterations':
            iterations = copy.deepcopy(value[offset:offset + updates])
            require(len(iterations) == updates, f'the base has fewer than {offset + updates} iterations')
            for u, iteration in enumerate(iterations):
                for j, item in enumerate(iteration['episodes']):
                    item['episode']['id'] = f'ckpt-v2-acceptance-{run}-u{u:02d}-e{j:02d}'
                    item['episode']['seed'] = episode_seed(run, u, j)
            cfg[key] = iterations
        else:
            cfg[key] = copy.deepcopy(value)
    cfg['initial_source'] = dict(copy.deepcopy(base['initial_source']), checkpoint=dict(start))
    cfg['output_directory'] = '<OUTPUT>'
    cfg['collection_workers'] = collection_workers
    cfg['record_process_cpu'] = True
    return cfg


def v2_config(cfg_v1, containers):
    """Arm B: the same config with `record_format` v2 and every fixed opponent read as its converted container."""
    cfg = copy.deepcopy(cfg_v1)
    for opponent in cfg['opponents']:
        sha = opponent['source']['checkpoint']['sha256']
        require(sha in containers, f'opponent {opponent["id"]} has no converted container')
        opponent['source']['checkpoint'] = dict(containers[sha])
    cfg['record_format'] = 'v2'
    return cfg


def convert_opponent(converter, source, out_dir) -> dict:
    sha = source['checkpoint']['sha256']
    out_dir.mkdir(parents=True, exist_ok=True)
    source_path = out_dir / f'{sha}.source.json'
    write_json(source_path, source)
    out = (out_dir / f'{sha}.ckpt').resolve()
    receipt_path = out_dir / f'{sha}.receipt.json'
    if receipt_path.exists() and out.exists():
        receipt = read_json(receipt_path)
    else:
        require(not out.exists(), f'{out} exists without a receipt')
        result = subprocess.run([converter, 'convert', '--source', str(source_path.resolve()), '--output', str(out)],
                                capture_output=True, text=True)
        require(result.returncode == 0, f'converting {sha}: {result.stderr.strip()}')
        receipt = json.loads(result.stdout)
        write_json(receipt_path, receipt)
    require(receipt.get('direction') == 'v1-to-v2' and receipt['input']['sha256'] == sha,
            f'conversion receipt for {sha} names another input')
    require(receipt['output']['sha256'] == sha256_file(out), f'{out} differs from its conversion receipt')
    return {'container': {'path': out.as_posix(), 'sha256': receipt['output']['sha256']},
            'receipt': pin(receipt_path), 'state_sha256': receipt['state_sha256']}


def stage_input(path, sha, directory) -> str:
    """A verified copy of a pinned input under the hot root (design 7.2: staged inputs live on the run disk)."""
    check_pin({'path': path, 'sha256': sha}, 'input')
    target = Path(directory) / f'{sha[:16]}-{Path(path).name}'
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        tmp = target.with_name(target.name + '.tmp')
        shutil.copyfile(path, tmp)
        os.replace(tmp, target)
    check_pin({'path': str(target), 'sha256': sha}, 'staged input')
    return target.resolve().as_posix()


CORRESPONDENCE_CASES = ('A', 'B', 'conv', 'V7', 'V8')


def correspondence_passed(path) -> bool:
    """Design 6.2's receipt (checkpoint_correspondence_v1 summary): overall ok and every gated case true (V4 cases A
    and B, converted opponents, FlatV7 and FlatV8)."""
    doc = read_json(path)
    return doc.get('ok') is True and all(doc.get(case) is True for case in CORRESPONDENCE_CASES)


def prepare(spec_path, root, validation=False):
    spec_path, root = Path(spec_path), Path(root)
    spec = read_json(spec_path)
    prepared = root / 'prepared'
    require(not (prepared / 'PREPARED.json').exists(), f'{prepared} is already prepared')
    binary = check_pin(spec['binary'], 'binary')
    converter = check_pin(spec['converter'], 'converter')
    starts = spec['starts']
    updates = int(spec.get('updates', UPDATES))
    workers = int(spec.get('collection_workers', COLLECTION_WORKERS))
    offset = int(spec.get('segment_offset', 0))
    if not validation:
        require(len(starts) == RUNS, f'the measurement runs {RUNS} starts')
        require({b: sum(s['base'] == b for s in starts) for b in RUNS_PER_BASE} == RUNS_PER_BASE,
                'the measurement runs 8 c and 8 v8 lineages')
        require(len({s['checkpoint']['sha256'] for s in starts}) == RUNS, 'the 16 start checkpoints are distinct')
        require(updates == UPDATES and workers == COLLECTION_WORKERS, 'the measurement uses 18 updates, 4 workers')
        require('correspondence_receipt' in spec, 'the measurement pins the passed 6.2 correspondence receipt')
    require(updates >= 2, 'a segment needs at least 2 updates')
    require(len({s['run'] for s in starts}) == len(starts), 'run names are unique')
    passed = None
    if 'correspondence_receipt' in spec:
        passed = correspondence_passed(check_pin(spec['correspondence_receipt'], '6.2 correspondence receipt'))
        require(validation or passed, 'the 6.2 correspondence receipt does not record a pass for every gated case')
    bases, inputs = {}, {}
    for name, entry in spec['bases'].items():
        base = read_json(check_pin(entry, f'base {name}'))
        for ref in _paths(base):
            if ref['path'] == '<OUTPUT>':
                continue
            ref['path'] = map_path(ref['path'], spec.get('path_map'))
        for ref in _paths({k: v for k, v in base.items() if k != 'initial_source'}):
            inputs[ref['path']] = ref['sha256']
        inputs[base['initial_source']['play_import']['path']] = base['initial_source']['play_import']['sha256']
        bases[name] = base
    staged = {path: stage_input(path, sha, prepared / 'inputs') for path, sha in sorted(inputs.items())}
    for base in bases.values():
        for ref in _paths({k: v for k, v in base.items() if k != 'initial_source'}):
            ref['path'] = staged[ref['path']]
        play = base['initial_source']['play_import']
        play['path'] = staged[play['path']]
    containers, conversions = {}, {}
    for name, base in bases.items():
        for opponent in base['opponents']:
            source = opponent['source']
            sha = source['checkpoint']['sha256']
            if sha not in conversions:
                conversions[sha] = convert_opponent(str(converter), source, prepared / 'opponents')
                containers[sha] = conversions[sha]['container']
    configs = {}
    for s in starts:
        require(s['base'] in bases, f'{s["run"]}: unknown base {s["base"]}')
        check_pin(s['checkpoint'], f'{s["run"]} start checkpoint')
        start = {'path': stage_input(s['checkpoint']['path'], s['checkpoint']['sha256'], prepared / 'inputs'),
                 'sha256': s['checkpoint']['sha256']}
        require(checkpoint_reader.has_moments(start['path']),
                f'{s["run"]}: the start checkpoint carries no Adam moments')
        cfg1 = run_config(bases[s['base']], s['run'], start, offset, updates, workers)
        cfg2 = v2_config(cfg1, containers)
        configs[s['run']] = {'base': s['base'],
                             'v1': write_json(prepared / 'configs' / f'{s["run"]}-v1.json', cfg1),
                             'v2': write_json(prepared / 'configs' / f'{s["run"]}-v2.json', cfg2)}
    manifest = {
        'schema': SCHEMA, 'prepared_at': now_iso(), 'host': os.environ.get('COMPUTERNAME', ''),
        'validation': bool(validation), 'spec': pin(spec_path), 'harness': pin(__file__),
        'checkpoint_reader': pin(checkpoint_reader.__file__), 'binary': pin(binary), 'converter': pin(converter),
        'correspondence_receipt': spec.get('correspondence_receipt'), 'correspondence_passed': passed,
        'updates': updates,
        'collection_workers': workers, 'segment_offset': offset, 'runs': [s['run'] for s in starts],
        'configs': configs, 'conversions': conversions, 'inputs': inputs,
    }
    write_json(prepared / 'PREPARED.json', manifest)
    return manifest


# ------------------------------------------------------------------ sampler and claims

def parse_typeperf_line(line, header):
    """One typeperf CSV row as (epoch, {column: value}); None for any other line."""
    line = line.strip()
    if not line.startswith('"') or line.startswith('"(PDH-CSV'):
        return None
    cells = [c.strip('"') for c in line.split('","')]
    if len(cells) != len(header):
        return None
    try:
        at = time.mktime(time.strptime(cells[0].split('.')[0], '%m/%d/%Y %H:%M:%S'))
        at += float('0.' + cells[0].split('.')[1]) if '.' in cells[0] else 0.0
    except (ValueError, IndexError):
        return None
    values = {}
    for name, cell in zip(header[1:], cells[1:]):
        try:
            values[name] = float(cell)
        except ValueError:
            pass
    return at, values


def check_typeperf_row_width(line, header):
    """A complete data row whose cell count differs from the header's cannot be mapped to columns; stop the sampler
    (the start precondition then fails loudly) instead of counting every row as missing. A final fragment without
    its newline (typeperf terminated mid-write) is not a complete row and is left to parse_typeperf_line."""
    if not line.endswith('\n'):
        return
    line = line.strip()
    if not line.startswith('"') or line.startswith('"(PDH-CSV'):
        return
    cells = line.split('","')
    require(len(cells) == len(header), f'typeperf row has {len(cells)} cells but the header has {len(header)}')


def disk_columns(header, letter):
    """typeperf columns of the physical disk holding drive `letter`: (idle column, read-bytes column)."""
    if not letter:
        return None, None
    tag = f'{letter.upper()}:'
    idle = read = None
    for name in header:
        if '\\PhysicalDisk(' not in name:
            continue
        instance = name.split('\\PhysicalDisk(')[1].split(')')[0]
        if tag not in instance.split():
            continue
        if name.endswith('\\% Idle Time'):
            idle = name
        elif name.endswith('\\Disk Read Bytes/sec'):
            read = name
    return idle, read


class Samples:
    """Rows reduced to (epoch, run-disk read bytes/s, artifact-disk busy %); thread-safe append and window reads."""

    def __init__(self, interval=SAMPLE_INTERVAL):
        self.interval = interval
        self.rows = []
        self.lock = threading.Lock()

    def add(self, at, d_read, e_busy):
        with self.lock:
            self.rows.append((at, d_read, e_busy))

    def window(self, start, end):
        with self.lock:
            return [r for r in self.rows if start <= r[0] < end]

    def latest(self):
        """Typeperf time of the newest row read, or None before the first."""
        with self.lock:
            return self.rows[-1][0] if self.rows else None


def _finite(x):
    return x is not None and math.isfinite(x)


def valid_rows(rows, artifact_disk=True):
    """Rows whose required counters (run-disk reads, and artifact-disk busy when there is one) are finite."""
    return [r for r in rows if _finite(r[1]) and (not artifact_disk or _finite(r[2]))]


def window_stats(rows, start, end, interval, artifact_disk=True):
    """Counter means and maxima over a span. Presence counts only rows with every required counter finite; the
    expected count is round(span / interval), so a sampler on schedule misses at most one row by phase. Each
    counter's mean and maximum use all of its own finite observations."""
    expected = max(1, round((end - start) / interval))
    good = valid_rows(rows, artifact_disk)
    d = [r[1] for r in rows if _finite(r[1])]  # every finite observation of each counter, not only complete rows
    e = [r[2] for r in rows if _finite(r[2])]
    return {'rows': len(good), 'all_rows': len(rows), 'expected_rows': expected,
            'missing_fraction': max(0.0, 1.0 - len(good) / expected),
            'd_read_mean': statistics.fmean(d) if d else None, 'e_busy_mean': statistics.fmean(e) if e else None,
            'd_read_max': max(d) if d else None, 'e_busy_max': max(e) if e else None}


def start_window_failures(stats, artifact_disk=True):
    """Design 7.2's start precondition on a trailing window: the sampler present (at most 10% of rows missing) and
    every observation under the limits (E: under 50% busy, D: reads under 60 MB/s)."""
    failing = []
    if stats['missing_fraction'] > MISSING_MAX:
        failing.append(f'{stats["missing_fraction"]:.0%} of sampler rows missing')
    if artifact_disk and stats['e_busy_max'] is not None and stats['e_busy_max'] >= E_BUSY_MAX:
        failing.append(f'artifact disk reached {stats["e_busy_max"]:.0f}% busy')
    if stats['d_read_max'] is not None and stats['d_read_max'] >= D_READ_MAX:
        failing.append(f'run disk reads reached {stats["d_read_max"] / 1e6:.0f} MB/s')
    return failing


class Sampler(threading.Thread):
    # No Processor Information counter: on some hosts typeperf leaves it out of the header while every row still
    # carries its value, so no row matched the header and the start precondition saw 100% missing rows. The rule
    # reads only the disk columns.
    COUNTERS = [r'\PhysicalDisk(*)\% Idle Time', r'\PhysicalDisk(*)\Disk Read Bytes/sec',
                r'\PhysicalDisk(*)\Disk Write Bytes/sec', r'\PhysicalDisk(*)\Current Disk Queue Length',
                r'\System\Processor Queue Length']

    def __init__(self, samples, csv_path, run_disk, artifact_disk):
        super().__init__(daemon=True)
        self.samples, self.csv_path = samples, Path(csv_path)
        self.run_disk, self.artifact_disk = run_disk, artifact_disk
        self.proc = None
        self.error = None

    def run(self):
        try:
            self.proc = subprocess.Popen(['typeperf', *self.COUNTERS, '-si', str(self.samples.interval)],
                                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
            header = None
            with open(self.csv_path, 'a', encoding='utf-8', newline='\n') as raw:
                for line in self.proc.stdout:
                    if not line.strip():
                        continue
                    raw.write(line if line.endswith('\n') else line + '\n')
                    raw.flush()
                    if line.startswith('"(PDH-CSV'):
                        header = [c.strip('"') for c in line.strip().split('","')]
                        d_idle, d_read = disk_columns(header, self.run_disk)
                        e_idle, _ = disk_columns(header, self.artifact_disk)
                        require(d_read is not None, f'typeperf has no physical disk for {self.run_disk}:')
                        require(not self.artifact_disk or e_idle is not None,
                                f'typeperf has no physical disk for {self.artifact_disk}:')
                        continue
                    if header is None:
                        continue
                    check_typeperf_row_width(line, header)
                    parsed = parse_typeperf_line(line, header)
                    if parsed is None:
                        continue
                    at, values = parsed
                    idle = values.get(e_idle) if self.artifact_disk else None
                    e_busy = max(0.0, 100.0 - idle) if _finite(idle) else None
                    d = values.get(d_read)
                    self.samples.add(at, d if _finite(d) else None, e_busy)
        except Exception as error:  # noqa: BLE001 - recorded; missing rows then contaminate blocks
            self.error = f'{type(error).__name__}: {error}'

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()


def ancestor_pids() -> set[int]:
    """This process and its ancestors; a claim held by one of them is this measurement's own."""
    try:
        import psutil  # type: ignore
        out, p = set(), psutil.Process()
        while p is not None:
            out.add(p.pid)
            p = p.parent()
        return out
    except ImportError:
        pass
    table = {}
    if os.name == 'nt':
        text = subprocess.run(['powershell', '-NoProfile', '-NonInteractive', '-Command',
                               'Get-CimInstance Win32_Process | ForEach-Object { "$($_.ProcessId) '
                               '$($_.ParentProcessId)" }'], capture_output=True, text=True, timeout=120).stdout
        for row in text.splitlines():
            parts = row.split()
            if len(parts) == 2 and all(p.isdigit() for p in parts):
                table[int(parts[0])] = int(parts[1])
    out, pid = set(), os.getpid()
    while pid and pid not in out:
        out.add(pid)
        pid = table.get(pid)
    return out


def own_claim(status, own_pids):
    """This measurement's claim (held by this process or an ancestor) as (id, cores); None when there is none."""
    for c in status.get('claims', []):
        if c.get('pid') in own_pids:
            return c.get('id'), set(c.get('cores') or [])
    return None


def foreign_claims(status, pool, own_pids):
    """Claims and timed holds overlapping the pool cores that are not this measurement's own."""
    foreign = []
    for c in status.get('claims', []):
        if c.get('pid') in own_pids:
            continue
        if set(c.get('cores') or []) & pool:
            foreign.append({k: c.get(k) for k in ('id', 'lane', 'work_id', 'pid', 'cores')})
    timed = status.get('timed') or {}
    if timed.get('held') and (not timed.get('cores') or set(timed['cores']) & pool):
        foreign.append({'timed': True, 'cores': timed.get('cores')})
    return foreign


class ClaimWatch(threading.Thread):
    def __init__(self, host_slots, pool, own_pids, own_id, log_path):
        super().__init__(daemon=True)
        self.host_slots, self.pool, self.own, self.own_id = host_slots, pool, own_pids, own_id
        self.log_path = Path(log_path)
        self.polls = []  # (epoch, foreign list or None when the poll failed, own claim intact)
        self.lock = threading.Lock()
        self.stopping = threading.Event()

    def poll_once(self):
        at = time.time()
        foreign, intact = None, True
        try:
            out = subprocess.run([sys.executable, '-B', self.host_slots, 'status'], capture_output=True, text=True,
                                 timeout=60)
            if out.returncode == 0:
                status = json.loads(out.stdout)
                foreign = foreign_claims(status, self.pool, self.own)
                mine = own_claim(status, self.own)
                intact = mine is not None and mine[0] == self.own_id and self.pool <= mine[1]
        except Exception:  # noqa: BLE001 - a failed poll is a gap
            foreign = None
        with self.lock:
            self.polls.append((at, foreign, intact))
        if foreign is None or foreign or not intact:
            append_jsonl(self.log_path, {'at': at, 'foreign': foreign, 'failed': foreign is None,
                                         'own_claim_intact': intact})

    def run(self):
        while not self.stopping.is_set():
            self.poll_once()
            self.stopping.wait(CLAIM_POLL_SECONDS)

    def window(self, start, end):
        with self.lock:
            return [p for p in self.polls if start <= p[0] < end]


def claim_stats(polls, start, end):
    ok = sorted(p[0] for p in polls if p[1] is not None)
    edges = [start] + ok + [end]
    gap = max(b - a for a, b in zip(edges, edges[1:])) if end > start else 0.0
    foreign = [p[1] for p in polls if p[1]]
    return {'polls': len(polls), 'failed_polls': sum(1 for p in polls if p[1] is None), 'max_gap': gap,
            'foreign': foreign[:3], 'overlap': bool(foreign),
            'own_claim_lost': any(p[1] is not None and not p[2] for p in polls)}


def relative_read_threshold(baseline):
    """Twice the larger of the calibration baseline and the amendment 1 floor."""
    return RELATIVE_FACTOR * max(baseline, D_READ_BASELINE_FLOOR)


def block_contamination(stats, claims, baseline, interval_ok=True, artifact_disk=True):
    reasons = []
    if stats['missing_fraction'] > MISSING_MAX:
        reasons.append(f'{stats["missing_fraction"]:.0%} of sampler rows missing')
    if artifact_disk and stats['e_busy_mean'] is not None and stats['e_busy_mean'] > E_BUSY_MAX:
        reasons.append(f'artifact disk {stats["e_busy_mean"]:.1f}% busy')
    if stats['d_read_mean'] is not None and stats['d_read_mean'] > D_READ_MAX:
        reasons.append(f'run disk reads {stats["d_read_mean"] / 1e6:.1f} MB/s')
    if baseline is not None and stats['d_read_mean'] is not None:
        threshold = relative_read_threshold(baseline)
        if stats['d_read_mean'] > threshold:
            reasons.append(f'run disk reads {stats["d_read_mean"] / 1e6:.1f} MB/s over the relative threshold '
                           f'{threshold / 1e6:.1f} MB/s')
    if claims['overlap']:
        reasons.append('another claim overlapped the pool')
    if claims['max_gap'] > CLAIM_GAP_MAX:
        reasons.append(f'claim polls left a {claims["max_gap"]:.0f} s gap')
    if claims.get('own_claim_lost'):
        reasons.append('the measurement lost its own claim')
    return reasons


# ------------------------------------------------------------------ one block

def iteration_rows(native: Path, updates: int):
    """Per update: complete.json mtime, after_state_sha256, cumulative driver CPU and the published checkpoint path."""
    rows = []
    for i in range(updates):
        complete = native / 'iterations' / f'{i:06d}' / 'complete.json'
        require(complete.is_file(), f'{complete} is missing')
        receipt = read_json(complete)
        update = read_json(receipt['update']['path'])
        rows.append({'mtime': complete.stat().st_mtime, 'after_state_sha256': update['after_state_sha256'],
                     'cpu': receipt.get('driver_process_cpu_seconds'), 'checkpoint': update['checkpoint']['path']})
    return rows


def run_throughput(rows):
    """(U - 1) x 3600 / wall of updates 1 to U-1, the wall being the gap between complete.json publications."""
    wall = rows[-1]['mtime'] - rows[0]['mtime']
    require(wall > 0, 'a run has no positive wall over updates 1 onward')
    return (len(rows) - 1) * 3600.0 / wall, wall


def checkpoint_is_container(path) -> bool:
    with open(path, 'rb') as f:
        return checkpoint_reader.is_container(f.read(8))


def argv_for(binary, config_path):
    return [str(binary), str(config_path)]


def launch_block(binary, configs, block_dir, stagger, timeout):
    """Starts run k at k x stagger after run 0 (below-normal priority, inheriting the claim's affinity) and waits."""
    block_dir.mkdir(parents=True, exist_ok=False)
    procs, logs = [], []
    t0 = time.monotonic()
    flags = BELOW_NORMAL_PRIORITY_CLASS if os.name == 'nt' else 0
    try:
        for k, (run, cfg) in enumerate(configs):
            run_dir = block_dir / run
            run_dir.mkdir()
            cfg = dict(cfg, output_directory=(run_dir / 'native').resolve().as_posix())
            config_path = run_dir / 'config.json'
            write_json(config_path, cfg)
            delay = t0 + k * stagger - time.monotonic()
            if delay > 0:
                time.sleep(delay)
            log = open(run_dir / 'driver.log', 'wb')
            logs.append(log)
            procs.append(subprocess.Popen(argv_for(binary, config_path.resolve()), stdout=log,
                                          stderr=subprocess.STDOUT, creationflags=flags))
        deadline = time.monotonic() + timeout
        codes = []
        for p in procs:
            try:
                codes.append(p.wait(timeout=max(1.0, deadline - time.monotonic())))
            except subprocess.TimeoutExpired:
                codes.append(None)
        return codes
    finally:
        for p in procs:
            if p.poll() is None:
                p.kill()
                p.wait()
        for log in logs:
            log.close()


def measure_block(block_dir, runs, updates, arm_format):
    """Throughput, identity and format checks of a finished block."""
    per_run, total, walls, cpu = {}, 0.0, {}, []
    for run in runs:
        native = block_dir / run / 'native'
        require((native / 'completion.json').is_file(), f'{run}: no completion.json')
        rows = iteration_rows(native, updates)
        rate, wall = run_throughput(rows)
        total += rate
        walls[run] = wall
        per_run[run] = [r['after_state_sha256'] for r in rows]
        is_v2 = checkpoint_is_container(rows[0]['checkpoint'])
        require(is_v2 == (arm_format == 'v2'), f'{run}: published checkpoints are not record_format {arm_format}')
        steps = [b['cpu'] - a['cpu'] for a, b in zip(rows, rows[1:]) if a['cpu'] is not None and b['cpu'] is not None]
        if steps:
            cpu.append(statistics.fmean(steps))
    return {'throughput': total, 'walls': walls, 'after_states': per_run,
            'cpu_per_update_mean': statistics.fmean(cpu) if cpu else None}


# ------------------------------------------------------------------ retention

def prune_group(group_dir, runs, updates, log_path):
    """Delete episode files and every checkpoint except the group's last per run; keep receipts and logs.

    Per update attempt: the trajectories a published collection.json lists, and update/checkpoint.json. Nothing
    else under collect/ is touched (an interrupted collection keeps its files, receipts and ledgers included). The
    kept checkpoint per run is the latest one a complete.json published in the group (a partial block counts)."""
    blocks = sorted(p for p in group_dir.iterdir() if p.is_dir())
    deleted, freed = 0, 0
    keep = set()
    for run in runs:
        for block in reversed(blocks):
            last = _latest_published(block / run / 'native')
            if last is not None:
                keep.add(last)
                break
    for block in blocks:
        for run in runs:
            native = block / run / 'native'
            if not native.is_dir():
                continue
            doomed = []
            for attempt in native.glob('iterations/*/attempt-*'):
                collection = attempt / 'collect' / 'collection.json'
                if collection.is_file():
                    try:
                        doomed += [Path(t['path']) for t in read_json(collection).get('trajectories', [])]
                    except (OSError, ValueError, KeyError, TypeError):
                        pass
                doomed.append(attempt / 'update' / 'checkpoint.json')
            root = native.resolve()
            for path in sorted({p.resolve() for p in doomed}):
                if path.is_file() and root in path.parents and path.as_posix() not in keep:
                    freed += path.stat().st_size
                    path.unlink()
                    deleted += 1
    row = {'at': now_iso(), 'group': group_dir.name, 'deleted_files': deleted, 'freed_bytes': freed,
           'retained_bytes': tree_bytes(group_dir)}
    append_jsonl(log_path, row)
    return row


def _latest_published(native):
    """The checkpoint of the run's latest complete.json, resolved; None when no update completed."""
    for complete in sorted(native.glob('iterations/*/complete.json'), reverse=True):
        try:
            return Path(read_json(read_json(complete)['update']['path'])['checkpoint']['path']).resolve().as_posix()
        except (OSError, ValueError, KeyError, TypeError):
            continue
    return None


def tree_bytes(path):
    return sum(p.stat().st_size for p in Path(path).rglob('*') if p.is_file())


def tree_manifest(path):
    path = Path(path)
    return sorted((p.relative_to(path).as_posix(), p.stat().st_size) for p in path.rglob('*') if p.is_file())


def move_retained(src, dest_root, log_path):
    """Copy to a staging name, verify every file and size, record the verified manifest beside the destination,
    rename into place, then delete the source. Restartable at every step: a staging copy is discarded and redone;
    a published destination is checked against its recorded manifest (kept in a sibling `<root>-manifests`
    folder, never among the moved trees) and whatever remains of the source is deleted."""
    src = Path(src)
    dest = Path(dest_root) / src.name
    staging = dest.with_name(dest.name + '.partial')
    sidecar = dest.parent.with_name(dest.parent.name + '-manifests') / f'{dest.name}.json'
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        require(sidecar.is_file() and [tuple(x) for x in read_json(sidecar)['files']] == tree_manifest(dest),
                f'{dest} exists without a matching verified manifest')
    else:
        if staging.exists():
            shutil.rmtree(staging)
        append_jsonl(log_path, {'at': now_iso(), 'phase': 'intent', 'from': src.as_posix(), 'to': dest.as_posix()})
        shutil.copytree(src, staging)
        manifest = tree_manifest(src)
        require(manifest == tree_manifest(staging), f'the copy of {src} is incomplete')
        write_json(sidecar, {'from': src.as_posix(), 'to': dest.as_posix(), 'files': manifest})
        os.replace(staging, dest)
    size = tree_bytes(dest)
    if src.exists():
        shutil.rmtree(src)
    row = {'at': now_iso(), 'phase': 'moved', 'from': src.as_posix(), 'to': dest.as_posix(), 'bytes': size}
    append_jsonl(log_path, row)
    return row


# ------------------------------------------------------------------ the attempt

class Measurement:
    def __init__(self, root, attempt, args, prepared):
        self.root, self.attempt, self.args, self.prepared = Path(root), attempt, args, prepared
        self.dir = self.root / f'attempt-{attempt:03d}'
        self.state_path = self.dir / 'STATE.json'
        self.manifest_path = self.dir / 'MANIFEST.json'
        self.events = self.dir / 'EVENTS.jsonl'
        self.validation = prepared['validation']
        self.runs = prepared['runs']
        self.updates = prepared['updates']
        self.configs = {run: {fmt: read_json(check_pin(prepared['configs'][run][fmt], f'{run} {fmt} config'))
                              for fmt in ('v1', 'v2')} for run in self.runs}
        self.binary = check_pin(prepared['binary'], 'binary')
        self.pool = None if args.pool == 'own' else parse_cores(args.pool)
        self.samples = Samples()
        self.artifact_root = Path(args.artifact_root) / f'attempt-{attempt:03d}'

    def event(self, kind, **fields):
        append_jsonl(self.events, dict(fields, at=now_iso(), kind=kind))

    def save(self, state):
        write_json(self.state_path, state)

    # -- timing parameters
    @property
    def precondition_seconds(self):
        return self.args.precondition_seconds if self.validation else PRECONDITION_SECONDS

    @property
    def settle_seconds(self):
        return self.args.settle_seconds if self.validation else SETTLE_SECONDS

    def group_bytes(self):
        return 4 * len(self.runs) * self.updates * BYTES_PER_UPDATE

    def precondition(self):
        failing, stats, claims = self._precondition_at(time.time())
        if failing:
            return failing, stats, claims
        # A passing window is admitted only once the sampler has delivered every row stamped inside it: rows still
        # in transit could carry disk load the window has not seen.
        now = time.time()
        catchup = self.wait_for_sampler(now)
        failing, stats, claims = self._precondition_at(now)
        if not catchup['caught_up']:
            failing.append("the sampler did not deliver the window's rows")
        # A sampler that failed or died during the wait (even after its last row arrived) does not admit a group;
        # wait_for_start's next pass stops on it.
        if self.sampler.error is not None or not self.sampler.is_alive():
            failing.append('the sampler stopped during the start wait')
        return failing, stats, claims

    def _precondition_at(self, now):
        start = now - self.precondition_seconds
        artifact = bool(self.args.artifact_disk)
        stats = window_stats(self.samples.window(start, now), start, now, self.samples.interval, artifact)
        claims = claim_stats(self.claims.window(start, now), start, now)
        require(not claims['own_claim_lost'], 'the measurement lost its own claim')
        free = shutil.disk_usage(self.dir).free
        need = self.group_bytes() + self.args.keep_free_gib * 2 ** 30
        failing = []
        if time.time() - self.started < self.precondition_seconds:
            failing.append('the sampler has not covered the window yet')
        failing += start_window_failures(stats, artifact)
        if claims['overlap']:
            failing.append('another claim overlapped the pool')
        if claims['max_gap'] > CLAIM_GAP_MAX:
            failing.append(f'claim polls left a {claims["max_gap"]:.0f} s gap')
        if free < need:
            failing.append(f'free {free / 2 ** 30:.1f} GiB < {need / 2 ** 30:.1f} GiB')
        return failing, stats, claims

    def wait_for_start(self):
        last = None
        while True:
            require(self.sampler.error is None, f'the sampler failed: {self.sampler.error}')
            require(self.sampler.is_alive() and self.claims.is_alive(), 'the sampler or the claim watch stopped')
            failing, stats, claims = self.precondition()
            if not failing:
                self.event('precondition-met', window=stats, claims=claims)
                return
            kinds = [re.sub(r'[0-9.]+', '#', f) for f in failing]
            if kinds != last:
                self.event('precondition-waiting', failing=failing)
                last = kinds
            time.sleep(1.0)

    def wait_for_sampler(self, end):
        """Wait until the sampler has read a row stamped at or after `end`, so a block's window holds every row typeperf
        took during it. Returns the seconds waited and whether the rows caught up; a dead sampler or the time limit ends
        the wait and the absent rows then count as missing."""
        began = time.time()
        while True:
            latest = self.samples.latest()
            if latest is not None and latest >= end:
                return {'waited': round(time.time() - began, 3), 'caught_up': True}
            if (time.time() - began >= SAMPLER_CATCHUP_SECONDS or self.sampler.error is not None
                    or not self.sampler.is_alive()):
                return {'waited': round(time.time() - began, 3), 'caught_up': False}
            time.sleep(0.25)

    def run_group(self, state, group):
        index = state['groups_run'] + len([w for w in state['warmups'] if w.get('interrupted')])
        group['index'] = index
        name = f'g{index:03d}-{group["phase"]}-{group["pattern"]}'
        group_dir = self.dir / 'groups' / name
        state['in_group'] = dict(group, dir=group_dir.as_posix())
        self.save(state)
        self.event('group-start', group=name)
        blocks = []
        failure = None
        for pos, label in enumerate(group['pattern'], start=1):
            fmt = 'v2' if group['phase'] == 'comparison' and label == 'B' else 'v1'
            block_dir = group_dir / f'b{pos}-{label}-{fmt}'
            start = time.time()
            codes = launch_block(self.binary, [(r, self.configs[r][fmt]) for r in self.runs], block_dir,
                                 STAGGER_SECONDS, BLOCK_TIMEOUT_SECONDS)
            end = time.time()
            if any(c != 0 for c in codes):
                failure = f'block {pos} exit codes {codes}'
                break
            try:
                measured = measure_block(block_dir, self.runs, self.updates, fmt)
            except Refused as error:
                failure = f'block {pos}: {error}'
                break
            artifact = bool(self.args.artifact_disk)
            catchup = self.wait_for_sampler(end)
            stats = window_stats(self.samples.window(start, end), start, end, self.samples.interval, artifact)
            claims = claim_stats(self.claims.window(start, end), start, end)
            if claims['own_claim_lost']:
                failure = f'block {pos}: the measurement lost its own claim'
                break
            baseline = state['calibration']['d_read_baseline'] if group['phase'] == 'comparison' else None
            reasons = block_contamination(stats, claims, baseline, artifact_disk=artifact)
            block = {'position': pos, 'label': label, 'format': fmt, 'start': start, 'end': end,
                     'throughput': measured['throughput'], 'walls': measured['walls'], 'sampler_catchup': catchup,
                     'cpu_per_update_mean': measured['cpu_per_update_mean'], 'counters': stats, 'claims': claims,
                     'contaminated': bool(reasons), 'reasons': reasons, 'd_read_mean': stats['d_read_mean']}
            reference = state.get('after_states')
            if reference is None:
                state['after_states'] = measured['after_states']
            elif reference != measured['after_states']:
                bad = sorted(r for r in self.runs if reference.get(r) != measured['after_states'].get(r))
                blocks.append(block)
                write_json(group_dir / 'GROUP.json', dict(group, blocks=blocks))
                state['in_group'] = None
                finish(state, 'void', 'after_state_sha256 differs between blocks: a defect, not noise', runs=bad,
                       group=index, block=pos)
                self.retain_after(state, group_dir)
                return
            blocks.append(block)
            self.event('block', group=name, position=pos, label=label, format=fmt,
                       throughput=round(block['throughput'], 1), contaminated=block['contaminated'],
                       reasons=reasons)
        group['blocks'] = blocks
        write_json(group_dir / 'GROUP.json', dict(group, failure=failure))
        if failure is not None:
            self.event('group-failed', group=name, failure=failure)
            interrupted(state)
            state.setdefault('failures', []).append({'group': index, 'failure': failure})
            self.retain_after(state, group_dir)
            return
        state['in_group'] = None
        record_group(state, group)
        self.retain_after(state, group_dir)

    def retain_after(self, state, group_dir):
        """Persist the pending retention with the group's outcome in one write, then prune, move and settle. A
        restart finds the pending retention and completes it before scheduling anything else."""
        state['pending_retention'] = Path(group_dir).as_posix()
        self.save(state)
        self.retain(state)

    def retain(self, state):
        group_dir = Path(state['pending_retention'])
        final = state['outcome'] is not None
        if group_dir.is_dir():
            prune = prune_group(group_dir, self.runs, self.updates, self.dir / 'PRUNES.jsonl')
            self.event('pruned', **prune)
        if (final or not couple_open(state)) and (self.dir / 'groups').is_dir():
            for path in sorted(p for p in (self.dir / 'groups').iterdir() if p.is_dir()):
                move_retained(path, self.artifact_root / 'groups', self.dir / 'MOVES.jsonl')
        if not final:
            self.event('settle', seconds=self.settle_seconds)
            time.sleep(self.settle_seconds)
        state['pending_retention'] = None
        self.save(state)

    def calibration_record(self, state):
        cal = state['calibration']
        blocks = []
        for c in state['calibration_couples']:
            if c['discarded']:
                continue
            for gi in c['groups']:
                g = read_json(self._group_record(gi))
                blocks += [b['d_read_mean'] for b in g['blocks'] if b['d_read_mean'] is not None]
        cal['d_read_baseline'] = statistics.median(blocks) if blocks else None
        cal['relative_read_threshold'] = (relative_read_threshold(cal['d_read_baseline'])
                                          if cal['d_read_baseline'] is not None else None)
        manifest = read_json(self.manifest_path)
        manifest['calibration'] = {k: cal[k] for k in ('contrasts', 'mean', 'sd', 'lower', 'upper', 'valid', 'n',
                                                      'd_read_baseline', 'relative_read_threshold')}
        manifest['calibration_recorded_at'] = now_iso()
        write_json(self.manifest_path, manifest)
        self.event('calibration', **manifest['calibration'])

    def _group_record(self, index):
        for parent in (self.dir / 'groups', self.artifact_root / 'groups'):
            if parent.is_dir():
                for path in sorted(parent.glob(f'g{index:03d}-*')):
                    if path.is_dir():
                        return path / 'GROUP.json'
        raise Refused(f'group {index} has no record')

    def execute(self):
        self.dir.mkdir(parents=True, exist_ok=True)
        if self.manifest_path.exists():
            require(not (self.dir / 'RESULT.json').exists(), f'attempt {self.attempt} already has its RESULT.json')
            manifest = read_json(self.manifest_path)
            state = read_json(self.state_path)
            self.event('resumed', outcome=state['outcome'])
        else:
            bits = [secrets.randbits(1) for _ in range(ORDER_BITS)]
            manifest = {'schema': SCHEMA, 'attempt': self.attempt, 'started_at': now_iso(),
                        'host': os.environ.get('COMPUTERNAME', ''), 'validation': self.validation,
                        'prepared': pin(self.root / 'prepared' / 'PREPARED.json'), 'harness': pin(__file__),
                        'order_bits': bits, 'order_bits_source': 'secrets.randbits (the OS generator)',
                        'pool': self.args.pool, 'run_disk': self.args.run_disk,
                        'artifact_disk': self.args.artifact_disk, 'artifact_root': self.artifact_root.as_posix(),
                        'host_slots': self.args.host_slots, 'keep_free_gib': self.args.keep_free_gib,
                        'precondition_seconds': self.precondition_seconds, 'settle_seconds': self.settle_seconds,
                        'byte_budget': {'per_block_bytes': self.group_bytes() / 4,
                                        'per_group_bytes': self.group_bytes(),
                                        'declared_peak_bytes': self.group_bytes() * 1.2 + 1e9},
                        'constants': {k: globals()[k] for k in (
                            'RUNS', 'UPDATES', 'POOL_CORES', 'STAGGER_SECONDS', 'E_BUSY_MAX', 'D_READ_MAX',
                            'MISSING_MAX', 'RELATIVE_FACTOR', 'D_READ_BASELINE_FLOOR', 'CALIBRATION_COUPLES', 'N_MIN',
                            'N_MAX', 'MAX_DISCARDS', 'MAX_WARMUPS', 'GUARD_ALPHA', 'SAMPLE_INTERVAL',
                            'CLAIM_POLL_SECONDS', 'CLAIM_GAP_MAX')}}
            write_json(self.manifest_path, manifest)
            state = new_state(bits)
            self.save(state)
            self.event('attempt-start', bits_pinned=True)
        if state.get('pending_retention'):
            self.event('retention-resumed', group=state['pending_retention'])
            self.retain(state)
        if state['in_group'] is not None:
            group_dir = Path(state['in_group']['dir'])
            self.event('interrupted', group=state['in_group'])
            interrupted(state)
            self.retain_after(state, group_dir)
        elif state['outcome'] is None and couple_open(state):
            finish(state, 'inconclusive', 'interrupted between the two groups of a couple')
            self.save(state)
        if state['outcome'] is None:
            self.measure(state)
        self.conclude(state)
        return state

    def make_watchers(self):
        sampler = Sampler(self.samples, self.dir / f'typeperf-{int(self.started)}.csv', self.args.run_disk,
                          self.args.artifact_disk)
        own_pids = ancestor_pids()
        status = json.loads(subprocess.run([sys.executable, '-B', self.args.host_slots, 'status'], capture_output=True,
                                           text=True, timeout=120, check=True).stdout)
        mine = own_claim(status, own_pids)
        require(mine is not None, 'this process runs inside no host_slots claim')
        own = mine[1]
        if self.pool is None:
            require(own, '--pool own: this process runs inside no host_slots claim')
            self.pool = own
        require(self.pool <= own, f'the pool {sorted(self.pool)} is not inside the claim this measurement runs in '
                                  f'{sorted(own)}')
        self.event('pool', pool=sorted(self.pool), own_claim_cores=sorted(own))
        mine = own_claim(status, own_pids)
        claims = ClaimWatch(self.args.host_slots, self.pool, own_pids, mine[0], self.dir / 'CLAIMS.jsonl')
        return sampler, claims

    def measure(self, state):
        self.started = time.time()
        self.sampler, self.claims = self.make_watchers()
        self.sampler.start()
        self.claims.start()
        try:
            while True:
                if state['calibration'] is not None and 'd_read_baseline' not in state['calibration']:
                    self.calibration_record(state)
                    self.save(state)
                group = next_group(state)
                self.save(state)
                if group is None:
                    break
                self.wait_for_start()
                self.run_group(state, group)
        finally:
            self.claims.stopping.set()
            self.sampler.stop()
            if self.sampler.error:
                self.event('sampler-error', error=self.sampler.error)

    def conclude(self, state):
        if (self.dir / 'groups').is_dir():
            for path in sorted((self.dir / 'groups').iterdir()):
                move_retained(path, self.artifact_root / 'groups', self.dir / 'MOVES.jsonl')
        result = {'schema': SCHEMA, 'attempt': self.attempt, 'validation': self.validation,
                  'decisive': not self.validation, 'ended_at': now_iso(), 'outcome': state['outcome'],
                  'calibration': state['calibration'], 'decision': state['decision'],
                  'groups_run': state['groups_run'], 'order_bits_used': state['next_bit'],
                  'correspondence_receipt': self.prepared.get('correspondence_receipt'),
                  'correspondence_passed': self.prepared.get('correspondence_passed', False),
                  'v2_becomes_default': (not self.validation and (state['outcome'] or {}).get('outcome') == 'accept'
                                         and self.prepared.get('correspondence_passed') is True),
                  'manifest': pin(self.manifest_path), 'state': pin(self.state_path)}
        write_json(self.dir / 'RESULT.json', result)
        self.event('result', outcome=state['outcome'])


def formal_placement(args):
    """The measurement's frozen placement: pool 4-23 (amendment 2; the design's 0-19 is no longer accepted), run root
    on D:, artifact root on E:, both disks sampled, the frozen precondition and settle times. Anything else is
    validation only."""
    require(args.precondition_seconds == PRECONDITION_SECONDS and args.settle_seconds == SETTLE_SECONDS,
            'the measurement uses the frozen precondition and settle times')
    require(parse_cores(args.pool) == parse_cores(POOL_CORES) if args.pool != 'own' else False,
            f'the measurement uses the pool {POOL_CORES}')
    require(args.run_disk == RUN_DISK and args.artifact_disk == ARTIFACT_DISK,
            f'the measurement samples run disk {RUN_DISK}: and artifact disk {ARTIFACT_DISK}:')
    require(Path(args.root).resolve().drive.upper() == f'{RUN_DISK}:', f'the run root is on {RUN_DISK}:')
    require(Path(args.artifact_root).resolve().drive.upper() == f'{ARTIFACT_DISK}:',
            f'the artifact root is on {ARTIFACT_DISK}:')


def attempt_allowed(root, attempt):
    require(1 <= attempt <= MAX_ATTEMPTS, f'attempt {attempt}: at most {MAX_ATTEMPTS} attempts')
    if attempt == 2:
        first = Path(root) / 'attempt-001' / 'RESULT.json'
        require(first.is_file(), 'attempt 2 needs attempt 1 recorded')
        outcome = read_json(first)['outcome']['outcome']
        require(outcome == 'inconclusive', f'attempt 1 ended {outcome}: no re-measurement is allowed')


# ------------------------------------------------------------------ command line

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    p = sub.add_parser('prepare')
    p.add_argument('--spec', required=True)
    p.add_argument('--root', required=True)
    p.add_argument('--validation', action='store_true')
    r = sub.add_parser('run')
    r.add_argument('--root', required=True)
    r.add_argument('--attempt', type=int, required=True)
    r.add_argument('--artifact-root', required=True)
    r.add_argument('--host-slots', required=True)
    r.add_argument('--pool', default=POOL_CORES, help="core list, or 'own': the cores of this process's claim")
    r.add_argument('--run-disk', default=RUN_DISK)
    r.add_argument('--artifact-disk', default=ARTIFACT_DISK, help="drive letter, or '' on a host without one")
    r.add_argument('--keep-free-gib', type=float, default=60.0)
    r.add_argument('--precondition-seconds', type=float, default=PRECONDITION_SECONDS)
    r.add_argument('--settle-seconds', type=float, default=SETTLE_SECONDS)
    s = sub.add_parser('summary')
    s.add_argument('--root', required=True)
    s.add_argument('--attempt', type=int, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == 'prepare':
            manifest = prepare(args.spec, args.root, args.validation)
            print(json.dumps({'prepared': len(manifest['runs']), 'validation': manifest['validation'],
                              'conversions': len(manifest['conversions'])}))
        elif args.command == 'run':
            prepared = read_json(Path(args.root) / 'prepared' / 'PREPARED.json')
            check_pin(prepared['harness'], 'harness (prepared by another version)')
            if not prepared['validation']:
                formal_placement(args)
                attempt_allowed(args.root, args.attempt)
            state = Measurement(args.root, args.attempt, args, prepared).execute()
            print(json.dumps(state['outcome']))
        else:
            d = Path(args.root) / f'attempt-{args.attempt:03d}'
            out = read_json(d / 'RESULT.json') if (d / 'RESULT.json').exists() else read_json(d / 'STATE.json')
            print(json.dumps(out, indent=1))
    except Refused as error:
        print(f'CHECKPOINT_V2_ACCEPTANCE_REFUSED {error}', file=sys.stderr)
        return 2
    return 0


if __name__ == '__main__':
    sys.exit(main())
