"""W5 `w5-xact-v1` (collab LANES/lead/W5-DESIGN.md v1.3 section 3; E2): the Python mirror of placement and acting,
segment configs, full-dose calibration and its check, and the mirror check over finished runs.

  xact.py config --base BASE.json --parent DOSE.json --out CONFIG.json --root RUN_DIR --chain CHAIN
                 --offset U --updates N [--workers W] [--budget B --coefficient C --dose primary|fallback]
                 [--ledger-source PATH]
      An RT1 segment config (EX1's qualification construction, tools/ex1/qual.py): with --budget, an X chain
      (`ex1.w5`); without, a C chain. --ledger-source adds the chain-only ledger (E10's source file).
  xact.py calibrate --dose primary|fallback RUN_DIR [...]   c* = r* / median raw ratio over updates with rows
  xact.py check --dose primary|fallback RUN_DIR [...]       median applied ratio within [r*/3, 3 r*]
  xact.py mirror RUN_DIR [...]       every placement index and every played root against this mirror, bit for bit
  xact.py summarize RUN_DIR [...]    per update: searches attempted, delivered targets, burst rows, masked rows

Doses (w5/DOSE-FALLBACK-RULE.md, blob 40b41ec7): primary r* = 1.0, kappa = 2; fallback r* = 0.3, kappa = 1.
"""
import argparse
import hashlib
import json
import statistics
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'ex1'))
import qual  # noqa: E402

RECIPE = 'w5-xact-v1'
PLACEMENT_MAX = 128
TELEMETRY_STRIDE = 16
DOSES = {'primary': (1.0, 2.0), 'fallback': (0.3, 1.0)}


def hash_u64(tag, chain, update, seed, slot):
    """SHA256(json([tag, chain, update, seed, slot])) with compact JSON, first 8 bytes little-endian."""
    text = json.dumps([tag, chain, update, seed, slot], separators=(',', ':'))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], 'little')


def uniform(h):
    return (h >> 11) * 2.0 ** -53


def placement_index(chain, update, seed, slot):
    """k = 1 + floor(128 u); with u's top 53 bits over 2^53 this is 1 plus the hash's top 7 bits."""
    return 1 + (hash_u64('w5/placement', chain, update, seed, slot) >> 57)


def sample_from_q(q, u):
    """The first menu index whose running binary64 sum exceeds u; the last positive index otherwise."""
    if any(not (p >= 0.0) or p == float('inf') for p in q):
        raise ValueError('q has a negative or non-finite entry')
    positive = [a for a, p in enumerate(q) if p > 0.0]
    if not positive:
        raise ValueError('q has no positive mass')
    total = 0.0
    for a, p in enumerate(q):
        total += p
        if p > 0.0 and u < total:
            return a
    return positive[-1]


def played_action(q, chain, update, seed, slot):
    return sample_from_q(q, uniform(hash_u64('w5/act', chain, update, seed, slot)))


def build_config(base, dose_record, root, chain, offset, updates, workers, budget=None, coefficient=None,
                 dose='primary', ledger_source=None):
    """EX1's qualification construction (episode ids and seeds keyed by the chain), with W5's options."""
    cfg = qual.build_config(base, dose_record, root, chain, 'w5', offset, updates, workers,
                            coefficient if budget is not None else None, DOSES[dose][1])
    if budget is not None:
        cfg['ex1']['w5'] = {'budget': budget}
    if ledger_source is not None:
        cfg['chain_ledger'] = {'source': Path(ledger_source).as_posix()}
    return cfg


def bits_to_floats(bits):
    return [struct.unpack('<d', int(b).to_bytes(8, 'little'))[0] for b in bits]


def collections(root):
    for receipt, update, collection in qual.iterations(root):
        ex1 = collection.get('ex1') or {}
        for pin in ex1.get('episodes', []):
            yield receipt, update, ex1['options'], qual.read(pin['path'])


def mirror(roots):
    checked = {'placements': 0, 'played_roots': 0}
    for root in roots:
        for _, _, options, record in collections(root):
            if not options.get('w5'):
                raise SystemExit(f'stop: {root} is not a W5 run')
            chain, update = options['run'], options['update']
            last = record['attempts'][-1]
            slot = record['slot']
            seed = last['seed']
            k = placement_index(chain, update, seed, slot)
            if last['w5']['placement_index'] != k:
                raise SystemExit(f'stop: placement index differs ({root}, update {update}, slot {slot})')
            checked['placements'] += 1
            for row in last['rows']:
                if row['kind'] != 'root':
                    continue
                q = bits_to_floats(row['q_bits'])
                if played_action(q, chain, update, seed, slot) != row['selected']:
                    raise SystemExit(f'stop: played action differs ({root}, update {update}, slot {slot})')
                checked['played_roots'] += 1
    print(json.dumps(dict(checked, mirror='ok'), indent=1))
    return checked


def summarize(roots):
    out = []
    for root in roots:
        for receipt, update, collection in qual.iterations(root):
            ex1 = qual.ex1_receipt(update)
            row = {'run': Path(root).name, 'iteration': receipt['iteration'],
                   'after_state_sha256': update['after_state_sha256'],
                   'ledger_usec': (receipt.get('chain_ledger') or {}).get('usec')}
            if ex1:
                strata = ex1.get('strata', {})
                exposure = ex1.get('gradient_exposure', {})
                row.update({'attempted': ex1.get('w5', {}).get('placements', {}).get('attempted'),
                            'acted': ex1.get('w5', {}).get('placements', {}).get('acted'),
                            'searches_failed': ex1['searches_failed'],
                            'root_rows': sum(c.get('root_rows', 0) for c in strata.values()),
                            'burst_rows': sum(c.get('burst_rows', 0) for c in strata.values()),
                            'rows': ex1['rows'], 'rows_dropped': ex1['rows_dropped'],
                            'pg_masked_rows': exposure.get('pg_masked_rows'),
                            'step_telemetry': exposure.get('step_telemetry'), 'cap': exposure.get('cap')})
            out.append(row)
    print(json.dumps(out, indent=1))
    return out


def ratios(roots, key):
    return qual.ratios(roots, key)


def calibrate(roots, dose):
    r_star = DOSES[dose][0]
    raw = ratios(roots, 'raw_ratio')
    if len(raw) < 8:
        raise SystemExit(f'stop: only {len(raw)} updates have rows')
    median = statistics.median(raw)
    if not median > 0:
        raise SystemExit('stop: the median raw ratio is not positive')
    c_star = float(f'{r_star / median:.3g}')
    out = {'dose': dose, 'r_star': r_star, 'kappa': DOSES[dose][1], 'updates_with_rows': len(raw),
           'median_raw_ratio': median, 'c_star': c_star, 'raw': raw}
    print(json.dumps(out, indent=1))
    return out


def check(roots, dose):
    r_star, kappa = DOSES[dose]
    applied = ratios(roots, 'applied_ratio')
    if len(applied) < 8:
        raise SystemExit(f'stop: only {len(applied)} updates have rows')
    median = statistics.median(applied)
    ok = r_star / 3 <= median <= 3 * r_star
    out = {'dose': dose, 'r_star': r_star, 'kappa': kappa, 'updates_with_rows': len(applied),
           'median_applied_ratio': median, 'band': [r_star / 3, 3 * r_star], 'pass': ok, 'applied': applied}
    print(json.dumps(out, indent=1))
    if not ok:
        raise SystemExit('stop: the median applied ratio is outside [r*/3, 3 r*]')
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    p = sub.add_parser('config')
    for name in ('--base', '--parent', '--out', '--root', '--chain'):
        p.add_argument(name, required=True)
    p.add_argument('--offset', type=int, required=True)
    p.add_argument('--updates', type=int, required=True)
    p.add_argument('--workers', type=int, default=1)
    p.add_argument('--budget', type=int)
    p.add_argument('--coefficient', type=float)
    p.add_argument('--dose', choices=sorted(DOSES), default='primary')
    p.add_argument('--ledger-source')
    for name in ('calibrate', 'check'):
        p = sub.add_parser(name)
        p.add_argument('--dose', choices=sorted(DOSES), required=True)
        p.add_argument('roots', nargs='+')
    for name in ('mirror', 'summarize'):
        sub.add_parser(name).add_argument('roots', nargs='+')
    a = ap.parse_args(argv)
    if a.cmd == 'config':
        if (a.budget is None) != (a.coefficient is None):
            raise SystemExit('stop: an X chain needs both --budget and --coefficient')
        cfg = build_config(qual.read(a.base), qual.read(a.parent), a.root, a.chain, a.offset, a.updates, a.workers,
                           a.budget, a.coefficient, a.dose, a.ledger_source)
        Path(a.out).write_text(json.dumps(cfg, indent=1) + '\n', encoding='utf-8')
    elif a.cmd == 'calibrate':
        calibrate(a.roots, a.dose)
    elif a.cmd == 'check':
        check(a.roots, a.dose)
    elif a.cmd == 'mirror':
        mirror(a.roots)
    else:
        summarize(a.roots)


if __name__ == '__main__':
    main()
