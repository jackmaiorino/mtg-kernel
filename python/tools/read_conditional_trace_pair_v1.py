"""Descriptive, fixed two-trace read. No engine invocation or causal estimate."""
import hashlib
import json
from pathlib import Path

B = Path('E:/mtg-meta-recovery-20260921/conditional-continuation-traces-001')
CARDS = Path('E:/mtg-kernel-public-stack-features-codex/data/cards_v1.json')
cards = json.loads(CARDS.read_bytes())['cards']


def named(value):
    if isinstance(value, dict):
        result = {k: named(v) for k, v in value.items()}
        if 'card_db_id' in value:
            result['card_name'] = cards[value['card_db_id']]['name']
        return result
    if isinstance(value, list):
        return [named(v) for v in value]
    return value


def describe(record):
    behavior = record['behavior']
    if behavior['kind'] != 'hamilton_q64':
        raise ValueError('unexpected behavior representation')
    masses = [int(m) for m in behavior['mass_numerators']]
    selected = behavior['selected_index']
    maxima = [i for i, m in enumerate(masses) if m == max(masses)]
    actions = record['visible']['ordered_actions']
    obs = record['visible']['observation']['projection']
    return dict(decision=record['decision_index'], actor=record['actor'], turn=obs['turn'],
        phase=obs['phase'], life=obs['life_totals'], selected_index=selected,
        selected_probability=masses[selected]/2**64,
        selected_action=named(actions[selected]), argmax_indices=maxima,
        argmax_actions=[named(actions[i]) for i in maxima],
        outside_argmax=selected not in maxima)


receipt = json.loads((B/'completion.json').read_bytes())
if not receipt['complete']:
    raise ValueError('replay incomplete')
loaded = []
summary = []
for check in receipt['checks']:
    ref = check['replay']
    raw = Path(ref['path']).read_bytes()
    if hashlib.sha256(raw).hexdigest() != ref['sha256']:
        raise ValueError('trace changed')
    row = json.loads(raw)['continuation']['rows'][0]
    records = row['records']
    loaded.append(records)
    descriptions = [describe(r) for r in records]
    deviations = [d for d in descriptions if d['outside_argmax']]
    summary.append(dict(index=check['index'], winner=row['terminal']['winner'],
        record_count=len(records), deviations_by_seat={s:sum(d['actor']==s for d in deviations) for s in ('p0','p1')},
        deviations=deviations, decisions=descriptions, input=ref))

first = None
for loss, win in zip(*loaded):
    if loss != win:
        first = dict(loss=describe(loss), win=describe(win),
            same_actor_visible_input=loss['visible']==win['visible'],
            same_behavior_masses=loss['behavior']['mass_numerators']==win['behavior']['mass_numerators'])
        break
result = dict(complete=True, first_divergence=first, traces=summary,
    scope='Selected pair resamples both seats. Behavior argmax deviations are descriptive, not causes of loss or proof of greedy superiority.',
    cards=dict(path=str(CARDS),sha256=hashlib.sha256(CARDS.read_bytes()).hexdigest()))
with (B/'descriptive-analysis.json').open('x', encoding='utf8') as stream:
    json.dump(result, stream, indent=2)
print(json.dumps(dict(first_divergence=first, traces=[{k:v for k,v in r.items() if k not in ('decisions','deviations','input')} for r in summary])))
