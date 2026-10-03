"""Full-record learning-signal audit of one archived ten-game update.

Extracted from the checked D4 sample calculation. No games or model execution.
Entropy is continuous softmax; calibration uses eventual realized terminal return.
Decision-weighted descriptions do not provide independent statistical replicates.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import numpy as np
f = np.float32

def bits(x):
    return np.asarray(x, dtype=np.uint32).view(np.float32)

def total(xs):
    s = f(0)
    for x in xs:
        s = f(s + x)
    return s

def normstats(raw):
    n = f(len(raw))
    m = f(total(raw) / n)
    sd = f(np.sqrt(f(total([f(f(x - m) * f(x - m)) for x in raw]) / n)))
    v = [f(f(x - m) / f(sd + f(1e-06))) for x in raw]
    mn = f(total(v) / n)
    return {'mean': float(mn), 'std': float(f(np.sqrt(f(total([f(f(x - mn) * f(x - mn)) for x in v]) / n)))), 'min': float(min(v)), 'max': float(max(v)), 'clip_fraction': 0.0}

def cohort(label):
    if label.startswith('published-44ae71e1e126b63d'):
        return 'Gates'
    if label.split('/')[0] in ['Affinity', 'Burn', 'Elves', 'Faeries', 'Rally', 'Terror', 'Wildfire']:
        return 'canonical'
    return 'additional'

def distance(n):
    return '0' if n == 0 else '1-9' if n < 10 else '10-49' if n < 50 else '50+'

def calculate(endpoint, update, receipt, trajectory, gamma, lam, kinds):
    summaries = []
    episode_rows = []
    raw_all = []
    physical = []
    menus = []
    rec = receipt
    for slot in range(10):
        t = trajectory(update, slot)
        if not t.get('optimizer_state_sha256', t.get('behavior_state_sha256')) == rec['before_state_sha256']:
            raise ValueError('Archived trajectory consistency failed')
        if not t['terminal']['terminal_classification'] == 'natural':
            raise ValueError('Archived trajectory consistency failed')
        seat = t['episode']['learner_seat']
        label = t['episode']['selected'][seat]['label']
        group = cohort(label)
        reward = f(t['terminal']['terminal_reward'][seat])
        groups = []
        i = 0
        while i < len(t['decisions']):
            d = t['decisions'][i]
            if type(d['substep_count']) is not int or d['substep_count'] <= 0:
                raise ValueError('Nonpositive physical decision length')
            rows = t['decisions'][i:i + d['substep_count']]
            if len(rows) != d['substep_count']:
                raise ValueError('Truncated physical decision')
            if not d['substep_index'] == 0:
                raise ValueError('Archived trajectory consistency failed')
            if not all((v['physical_decision_id'] == d['physical_decision_id'] and v['substep_index'] == j for j, v in enumerate(rows))):
                raise ValueError('Archived trajectory consistency failed')
            if d['actor'] == seat:
                groups.append(rows)
            i += d['substep_count']
        nv = f(0)
        na = f(0)
        advantages = []
        for j in reversed(range(len(groups))):
            v = f(bits(groups[j][0]['value']))
            delta = f(f((reward if j == len(groups) - 1 else f(0)) + f(gamma * nv)) - v)
            a = f(delta + f(f(gamma * lam) * na))
            advantages.append(a)
            nv = v
            na = a
        advantages.reverse()
        raw_all.extend(advantages)
        for j, rows in enumerate(groups):
            v = float(bits(rows[0]['value']))
            left = len(groups) - 1 - j
            physical.append({'cohort': group, 'distance': distance(left), 'value': v, 'reward': float(reward), 'advantage': float(advantages[j]), 'terminal_coefficient': float(lam) ** left})
            for row in rows:
                logits = bits(row['logits']).astype(float)
                features = np.asarray(row['tensor']['action_features'], dtype=np.uint32).reshape(len(logits), 195)[:, :27]
                if not np.all((features == 0) | (features == 1065353216)):
                    raise ValueError('Archived trajectory consistency failed')
                if not np.all((features == 1065353216).sum(axis=1) == 1):
                    raise ValueError('Archived trajectory consistency failed')
                types = features.argmax(axis=1)
                signature = '+'.join((kinds[k] for k in sorted(set(types))))
                p = np.exp(logits - logits.max())
                p /= p.sum()
                entropy = float(-(p[p > 0] * np.log(p[p > 0])).sum())
                menus.append({'cohort': group, 'menu_types': signature, 'selected_type': kinds[int(types[row['selected']])], 'forced': len(p) == 1, 'entropy': entropy, 'normalized_entropy': entropy / np.log(len(p)) if len(p) > 1 else 0.0, 'max_probability': float(p.max())})
        episode_rows.append({'endpoint': endpoint, 'update': update, 'slot': slot, 'cohort': group, 'deck': label, 'learner_groups': len(groups), 'terminal_reward': float(reward)})
    if not raw_all or not np.all(np.isfinite(raw_all)):
        raise ValueError('Empty or nonfinite learning signal')
    reconstructed = normstats(raw_all)
    if not all((f(reconstructed[k]).tobytes() == f(rec['advantage_statistics'][k]).tobytes() for k in reconstructed)):
        raise ValueError((endpoint, update, reconstructed, rec['advantage_statistics']))
    if not len(physical) == rec.get('learner_groups', rec.get('physical_decisions')):
        raise ValueError('Archived trajectory consistency failed')
    if not len(menus) == rec.get('learner_substeps', rec.get('policy_substeps')):
        raise ValueError('Archived trajectory consistency failed')
    bymenu = []
    bydistance = []
    calibration = []
    for key in sorted(set(((m['cohort'], m['menu_types'], m['forced']) for m in menus))):
        rows = [m for m in menus if (m['cohort'], m['menu_types'], m['forced']) == key]
        bymenu.append({'cohort': key[0], 'menu_types': key[1], 'forced': key[2], 'count': len(rows), **{k: float(np.mean([v[k] for v in rows])) for k in ['entropy', 'normalized_entropy', 'max_probability']}})
    for key in sorted(set(((p['cohort'], p['distance']) for p in physical))):
        rows = [p for p in physical if (p['cohort'], p['distance']) == key]
        a = np.array([p['advantage'] for p in rows])
        v = np.array([p['value'] for p in rows])
        y = np.array([p['reward'] for p in rows])
        bydistance.append({'cohort': key[0], 'learner_decisions_to_terminal_bin': key[1], 'count': len(rows), 'raw_advantage_mean': float(a.mean()), 'raw_advantage_std': float(a.std()), 'raw_advantage_mean_abs': float(abs(a).mean()), 'critic_bias_vs_terminal': float((v - y).mean()), 'critic_mse_vs_terminal': float(((v - y) ** 2).mean()), 'mean_terminal_reward_coefficient': float(np.mean([p['terminal_coefficient'] for p in rows]))})
    for group in sorted(set((p['cohort'] for p in physical))):
        for bin_id in range(10):
            rows = [p for p in physical if p['cohort'] == group and min(9, max(0, int((p['value'] + 1) * 5))) == bin_id]
            if rows:
                calibration.append({'cohort': group, 'value_bin': bin_id, 'count': len(rows), 'mean_prediction': float(np.mean([p['value'] for p in rows])), 'mean_terminal_return': float(np.mean([p['reward'] for p in rows]))})
    summaries.append({'endpoint': endpoint, 'update': update, 'normalized_statistics_bit_equal': True, 'learner_groups': len(physical), 'learner_substeps': len(menus), 'forced_substeps': sum((m['forced'] for m in menus)), 'menu_entropy': bymenu, 'raw_signal_and_critic': bydistance, 'calibration': calibration})
    return {'summary': summaries[0], 'episodes': episode_rows}


def pinned_bytes(ref, maximum):
    path = Path(ref['path'])
    if not 0 < path.stat().st_size <= maximum:
        raise ValueError('Pinned input exceeds declared read bound')
    with path.open('rb') as stream:
        raw = stream.read(maximum + 1)
    if len(raw) > maximum or hashlib.sha256(raw).hexdigest() != ref['sha256']:
        raise ValueError('Pinned input changed')
    return raw


def validate_trajectory(data):
    """Reject malformed groups before GAE or menu statistics can consume them."""
    decisions = data['decisions']
    index = 0
    while index < len(decisions):
        first = decisions[index]
        count = first['substep_count']
        if type(count) is not int or count <= 0 or index+count > len(decisions):
            raise ValueError('Invalid physical decision length')
        for offset, row in enumerate(decisions[index:index+count]):
            if row['substep_index'] != offset or row['substep_count'] != count or row['physical_decision_id'] != first['physical_decision_id'] or row['actor'] != first['actor']:
                raise ValueError('Inconsistent physical decision group')
            if not np.isfinite(bits(row['value'])):
                raise ValueError('Nonfinite archived critic value')
            logits = bits(row['logits'])
            if len(logits) == 0 or not np.all(np.isfinite(logits)) or type(row['selected']) is not int or not 0 <= row['selected'] < len(logits):
                raise ValueError('Invalid archived action menu')
        index += count


def audit(command):
    """Consume one update from pinned files; the owner supplies resource guards."""
    if command['schema'] != 'g115-d4-learning-signal-command/v1':
        raise ValueError('Unsupported learning-signal command')
    endpoint, update = command['endpoint'], command['update']
    if not isinstance(endpoint, str) or not endpoint or type(update) is not int or not 0 <= update < 200:
        raise ValueError('Invalid endpoint/update')
    refs = command['trajectories']
    if len(refs) != 10 or len({r['sha256'] for r in refs}) != 10:
        raise ValueError('Exactly ten distinct ordered trajectories required')
    cfg = json.loads(pinned_bytes(command['config'], 32*1024**2))
    loss = cfg.get('loss_selection', cfg)
    gamma, lam = f(loss['gamma']), f(loss['lambda'])
    if gamma != f(1) or not np.isfinite(lam) or not 0 <= lam <= 1:
        raise ValueError('This archived audit requires gamma=1 and lambda in [0,1]')
    receipt = json.loads(pinned_bytes(command['receipt'], 4*1024**2))
    if 'update' in receipt and receipt['update'] != update:
        raise ValueError('Receipt update differs')
    if 'complete' in receipt and receipt['complete'] is not True:
        raise ValueError('Incomplete training receipt')
    if 'trajectories' in receipt:
        if [r['sha256'] for r in receipt['trajectories']] != [r['sha256'] for r in refs]:
            raise ValueError('Trajectory order differs from update receipt')
    snapshots = command['action_sources']
    expected = {'policy': 'befcb3425cc00a7b178bc3c71702bcdd8c4fea8f7358855492b71ea2aa946432',
                'tensorizer': '39263a7593f1a24594f769caedcd1f6b78884ad362ac8dbb582207a9e4b66f49'}
    if set(snapshots) != set(expected) or any(snapshots[k]['sha256'] != v for k,v in expected.items()):
        raise ValueError('Archived action encoding differs')
    policy = pinned_bytes(snapshots['policy'], 4*1024**2).decode()
    tensorizer = pinned_bytes(snapshots['tensorizer'], 4*1024**2).decode()
    block = policy.split('pub enum FlatScorerActionKindV2 {')[1].split('}')[0]
    kinds = {int(i):name for name,i in re.findall(r'(\w+)\s*=\s*(\d+)', block)}
    if set(kinds) != set(range(27)) or 'NATIVE_FLAT_ACTION_FEATURE_DIM_V2: usize = 195' not in tensorizer or 'out[kind] = 1.0;' not in tensorizer:
        raise ValueError('Action feature mapping differs')
    consumed = []
    def trajectory(index, slot):
        if index != update:
            raise ValueError('Update changed during read')
        data = json.loads(pinned_bytes(refs[slot], 128*1024**2))
        validate_trajectory(data)
        consumed.append(refs[slot])
        return data
    with np.errstate(over='raise', invalid='raise', divide='raise'):
        result = calculate(endpoint, update, receipt, trajectory, gamma, lam, kinds)
    result.update(schema='g115-d4-learning-signal-result/v1', complete=True,
                  config=command['config'], receipt=command['receipt'],
                  action_sources=snapshots, consumed_trajectories=consumed,
                  numpy_version=np.__version__, gamma=float(gamma), **{'lambda':float(lam)})
    # Refuse nonfinite aggregate output before publishing a successful result.
    json.dumps(result, allow_nan=False)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--command', required=True)
    args = parser.parse_args()
    command = json.loads(Path(args.command).read_bytes())
    target = Path(command['output'])
    if target.exists() or target.with_suffix('.partial').exists() or not target.parent.is_dir():
        raise ValueError('Fresh output under owned job directory required')
    result = audit(command)
    raw = (json.dumps(result, indent=2, allow_nan=False)+'\n').encode()
    if len(raw) > 4*1024**2:
        raise ValueError('Learning-signal output exceeds 4MiB bound')
    temporary = target.with_suffix('.partial')
    with temporary.open('xb') as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, target)


if __name__ == '__main__':
    main()
