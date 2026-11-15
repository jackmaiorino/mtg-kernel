#!/usr/bin/env python3
"""A stand-in for the native trainer's collect step, for the London mulligan measurement tools' tests. It reads a
one-iteration config and writes, per episode, a trajectory shaped like the trainer's (pregame announcement and put-back
rows with float32 bit patterns, an opening record, a terminal block) and the `complete.json` / collection record layout
`mull_probe.collect` reads.

The simulated policy (everything is a pure function of the episode seed, the seat and the count, so reruns and the
paired lines agree exactly): a `learned` seat sees a hand of quality q in [0, 1) at each count (q is the first hand's
for count 0, a fresh draw after a mulligan) and its mulligan-minus-keep gap at count k is `gaps[k] - slope * (q - 0.5)`
plus noise; the lean the episode carries (`london_keep_offset`, a number or a list of 7 by own count) is subtracted
from the mulligan logit when it chooses. A `keep_seven` seat keeps seven and is never asked. The learner's win chance is
`0.5 + 0.3 (q_final - 0.5) - 0.02 * taken`, drawn with a uniform shared by every line of the same seed. Knobs live in the
config's `fake` block: gaps (7), slope, noise, value_per_card, bottom_scale.
"""
import hashlib
import json
import math
import struct
import sys
from pathlib import Path


def u01(*parts):
    h = hashlib.sha256('/'.join(str(p) for p in parts).encode()).digest()
    return int.from_bytes(h[:8], 'little') / 2 ** 64


def bits(x):
    return struct.unpack('<I', struct.pack('<f', x))[0]


def sigmoid(x):
    return 1 / (1 + math.exp(-x)) if x >= 0 else math.exp(x) / (1 + math.exp(x))


def lean(offset, k):
    if offset is None:
        return 0.0
    return float(offset[k]) if isinstance(offset, list) else float(offset)


def row(actor, state_pregame, action, logits, value):
    state = [0] * 100
    if state_pregame:
        state[89] = bits(1.0)
    return {'actor': actor, 'logits': [bits(x) for x in logits], 'value': bits(value), 'step': 0,
            'tensor': {'state': state, 'action_features': action}}


def announcement(actor, k, other_count, gap, swap, value, width=70):
    action = [0] * (2 * width)
    keep_row, mull_row = (1, 0) if swap else (0, 1)
    for r, is_mull in ((keep_row, 0.0), (mull_row, 1.0)):
        action[r * width + 15] = bits(1.0)
        action[r * width + 69] = bits(is_mull)
        action[r * width + 63] = bits(k / 16)
        action[r * width + 66] = bits(other_count / 16)
    logits = [0.0, 0.0]
    logits[keep_row] = 0.0
    logits[mull_row] = gap
    return row(actor, True, action, logits, value)


def play(ep, cfg):
    fake = cfg.get('fake') or {}
    gaps = fake.get('gaps', [6.0] * 7)
    slope, noise = fake.get('slope', 4.0), fake.get('noise', 1.0)
    per_card = fake.get('value_per_card', 0.33)
    seed, modes, offset = ep['seed'], ep['mulligan_modes'], ep.get('london_keep_offset')
    rows, seats, qfinal = [], [], {}
    taken_by_seat = {}
    for seat in (0, 1):
        if modes[seat] == 'keep_seven':
            taken_by_seat[seat] = 0
            qfinal[seat] = u01(seed, seat, 'hand', 0)
            seats.append({'mode': 'keep_seven', 'mulligans_taken': 0, 'kept_hand_size': 7,
                          'exposed_pregame_decisions': 0, 'forced_zero_card_keep': False})
            continue
        taken, exposed = 0, 0
        while True:
            q = u01(seed, seat, 'hand', taken)
            if taken == 7:
                break
            k = taken
            gap = gaps[k] - slope * (q - 0.5) + noise * (u01(seed, seat, 'noise', k) - 0.5)
            value = 0.6 - per_card * k
            other = 0 if modes[1 - seat] == 'keep_seven' else min(k + 1, 3)
            rows.append(announcement(seat, k, other, gap, u01(seed, seat, 'swap', k) < 0.5, value))
            exposed += 1
            p = sigmoid(gap - lean(offset, k))
            if u01(seed, seat, 'mull', k) < p:
                taken += 1
            else:
                break
        kept = 7 - taken
        qfinal[seat] = u01(seed, seat, 'hand', taken)
        for j in range(taken if kept else 0):
            n = 7 - j
            rows.append(row(seat, True, [bits(1.0)] * (n * 70), [u01(seed, seat, 'bottom', j, c) for c in range(n)], 0.0))
            exposed += 1
        taken_by_seat[seat] = taken
        seats.append({'mode': 'learned', 'mulligans_taken': taken, 'kept_hand_size': kept,
                      'exposed_pregame_decisions': exposed, 'forced_zero_card_keep': taken == 7})
    for seat in (0, 1):
        rows.append(row(seat, False, [0] * 140, [1.0, 2.0], 0.25))
    learner = ep['learner_seat']
    p = max(0.05, min(0.95, 0.5 + 0.3 * (qfinal[learner] - 0.5) - 0.02 * taken_by_seat[learner]))
    win = u01(seed, 'game') < p
    reward = [0, 0]
    reward[learner], reward[1 - learner] = (1, -1) if win else (-1, 1)
    return {'episode': ep, 'opening_record': {'opening': 'london-v1', 'seats': seats}, 'decisions': rows,
            'terminal': {'terminal_reward': reward, 'terminal_classification': 'natural', 'terminal_outcome': 'x'},
            'feature_contract_digest': 'f' * 64}


def main():
    cfg_path = Path(sys.argv[1])
    cfg = json.loads(cfg_path.read_text())
    if (cfg.get('fake') or {}).get('fail'):
        return 3
    root = Path(cfg['output_directory'])
    it = root / 'iterations' / '000000'
    collect = it / 'attempt-000000' / 'collect'
    collect.mkdir(parents=True)
    trajs = []
    for n, entry in enumerate(cfg['iterations'][0]['episodes']):
        data = json.dumps(play(entry['episode'], cfg), separators=(',', ':')).encode()
        path = collect / f'episode-{n:04d}.json'
        path.write_bytes(data)
        trajs.append({'path': path.as_posix(), 'sha256': hashlib.sha256(data).hexdigest()})
    coll = it / 'attempt-000000' / 'collect' / 'collection.json'
    coll.write_text(json.dumps({'trajectories': trajs}))
    (it / 'complete.json').write_text(json.dumps({'collection': {'path': coll.as_posix()}, 'update': {'path': 'none'}}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
