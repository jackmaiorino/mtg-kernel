"""Read-only, post hoc localization of complete entropy panels and control training.

No native simulation, checkpoint selection, new significance gate or pooled rescue.
"""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor
import hashlib
import json
from pathlib import Path
import time

ARMS = ('g115', 'control', 'entropy')
FOCAL = 'published-44ae71e1e126b63d'


def load(path, expected=None):
    data = Path(path).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if expected is not None and digest != expected:
        raise ValueError(f'Changed evidence: {path}')
    return json.loads(data), dict(path=str(path), sha256=digest)


def pinned(pin):
    return load(pin['path'], pin['sha256'])[0]


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')


def panel(analysis_pin):
    analysis = pinned(analysis_pin)
    assert analysis['complete'] and analysis['matches'] == 3072
    manifest = pinned(analysis['pilot'])
    evaluation = pinned(analysis['evaluation'])
    assert evaluation['pilot'] == analysis['pilot']
    templates = {j['label']: j for j in manifest['jobs']}
    records, seen = {}, set()
    assert len(evaluation['jobs']) == 48
    natural_games = 0
    for job in evaluation['jobs']:
        arm, label = job['arm'], job['label']
        assert arm in ARMS and (arm, label) not in seen
        seen.add((arm, label))
        template = templates[label]
        seat = template['candidate_seat']
        request = pinned(job['request'])
        execution = pinned(job['execution'])
        assert execution['exit_code'] == 0 and not execution['timeout']
        assert execution['request'] == job['request']
        folder = Path(job['output_directory'])
        completion, _ = load(folder / 'completion.json')
        start, _ = load(folder / 'start.json')
        assert start['command'] == request
        assert completion['matches'] == len(template['cases']) == 64
        games, decisions = 0, 0
        for index, case in enumerate(template['cases']):
            match, match_pin = load(folder / f'match-{index:06}.json', completion['match_sha256'][index])
            assert match['match'] == request['matches'][index]
            assert match['models'] == start['models']
            assert match['match']['config']['seed'] == case['seed']
            assert (match['games'][0]['start']['starting_player'] == seat) == case['candidate_starts']
            assert not match['decisions'] and match['diagnostic_spell_target_repairs'] == [0, 0]
            assert match['outcome'] != 'draw' and all(g['winner'] is not None for g in match['games'])
            key = (case['own'], case['opponent'], case['replicate'], seat, arm)
            assert key not in records
            records[key] = dict(seed=case['seed'], starts=case['candidate_starts'],
                win=int(match['outcome']['winner']['winner'] == seat),
                g1=int(match['games'][0]['winner'] == seat),
                g2=int(match['games'][1]['winner'] == seat),
                g2_boards=match['games'][1]['mainboard_sha256'],
                g2_start=match['games'][1]['start']['starting_player'], match=match_pin)
            games += len(match['games'])
            decisions += match['decision_count']
        assert games == completion['natural_games'] and decisions == completion['decisions']
        natural_games += games
    assert len(records) == 3072 and len(seen) == 48
    assert natural_games == analysis['natural_games']
    decks = sorted({key[0] for key in records})
    assert len(decks) == 8
    for own in decks:
        for opp in decks:
            for rep in range(8):
                cluster = [records[own, opp, rep, seat, arm] for seat in (0, 1) for arm in ARMS]
                assert len({r['seed'] for r in cluster}) == 1
    # Reconcile every existing frozen breakdown cell, not only aggregate wins.
    for cell in analysis['breakdown']:
        rows = [records[cell['own'], cell['opponent'], rep, cell['seat'], cell['arm']]
                for rep in range(8)]
        rows = [r for r in rows if r['starts'] == cell['starts']]
        assert len(rows) == cell['matches'] == 4
        assert sum(r['win'] for r in rows) == cell['wins']
        assert sum(r['g1'] for r in rows) == cell['game_one_wins']

    def compare(keys, treatment, baseline):
        c = Counter(matches=len(keys))
        for key in keys:
            a, b = records[*key, treatment], records[*key, baseline]
            assert a['seed'] == b['seed'] and a['starts'] == b['starts']
            c['treatment_wins'] += a['win']; c['baseline_wins'] += b['win']
            c['net_wins'] += a['win'] - b['win']
            c['game_one_net_wins'] += a['g1'] - b['g1']
            c['gained_matches'] += a['win'] > b['win']
            c['lost_matches'] += a['win'] < b['win']
            same = a['g1'] == b['g1']
            c['same_game_one_cases'] += same
            c['same_game_one_net_wins'] += (a['win'] - b['win']) if same else 0
            c['changed_game_one_net_wins'] += (a['win'] - b['win']) if not same else 0
            c['same_game_one_gains'] += same and a['win'] > b['win']
            c['same_game_one_losses'] += same and a['win'] < b['win']
            c['different_game_two_boards'] += a['g2_boards'] != b['g2_boards']
            c['same_game_one_different_game_two_start'] += same and a['g2_start'] != b['g2_start']
        assert c['net_wins'] == c['gained_matches'] - c['lost_matches']
        assert c['net_wins'] == c['same_game_one_net_wins'] + c['changed_game_one_net_wins']
        return dict(c)

    comparisons = {}
    allkeys = sorted({key[:4] for key in records})
    for treatment, baseline in [('control', 'g115'), ('entropy', 'control'), ('entropy', 'g115')]:
        name = f'{treatment}-minus-{baseline}'
        total = compare(allkeys, treatment, baseline)
        assert total['net_wins'] == analysis['comparisons'][name]['net_wins']
        comparisons[name] = dict(total=total,
            own_deck={d: compare([k for k in allkeys if k[0] == d], treatment, baseline) for d in decks},
            opponent_deck={d: compare([k for k in allkeys if k[1] == d], treatment, baseline) for d in decks},
            seat={str(s): compare([k for k in allkeys if k[3] == s], treatment, baseline) for s in (0, 1)},
            matchups=[dict(own=d, opponent=o, **compare([k for k in allkeys if k[:2] == (d, o)], treatment, baseline))
                      for d in decks for o in decks])
    return dict(replica=analysis['replica'], matches=3072, natural_games=natural_games,
                analysis=analysis_pin, comparisons=comparisons)


def training_update(task):
    update, episodes, outputs, receipt = task
    checkpoint = pinned(outputs[f'{update:04}/checkpoint.json'])
    assert checkpoint['next_update'] == update + 1 and receipt['update'] == update
    rows = []
    for index, expected in enumerate(episodes):
        source = outputs[f'{update:04}/episode-{index:03}.json']
        assert source['sha256'] == checkpoint['trajectory_sha256'][index]
        t = pinned(source)
        assert t['episode'] == expected
        assert t['terminal']['terminal_classification'] == 'natural'
        assert t['config_sha256'] == checkpoint['config_sha256']
        assert t['optimizer_state_sha256'] == receipt['before_state_sha256']
        seat = expected['learner_seat']
        groups = substeps = offset = 0
        while offset < len(t['decisions']):
            first = t['decisions'][offset]
            count = first['substep_count']
            assert count > 0 and first['substep_index'] == 0
            group = t['decisions'][offset:offset+count]
            assert len(group) == count
            assert all(r['actor'] == first['actor'] and r['physical_decision_id'] == first['physical_decision_id']
                       and r['substep_index'] == i and r['substep_count'] == count for i, r in enumerate(group))
            if first['actor'] == seat:
                groups += 1; substeps += count
            offset += count
        assert groups > 0
        rows.append(dict(update=update, episode=index, deck=expected['registered'][seat]['label'],
            groups=groups, substeps=substeps, source=source,
            coefficient_mass=groups/receipt['learner_groups']))
    assert len(rows) == receipt['episodes'] == 10
    assert sum(r['groups'] for r in rows) == receipt['learner_groups']
    assert sum(r['substeps'] for r in rows) == receipt['learner_substeps']
    return rows


def training(root, workers):
    audit, audit_pin = load(root / 'training-audit.json')
    assert audit['complete'] and audit['full_natural_games'] == 4000
    report_pin = audit['arms']['control']['report']
    report = pinned(report_pin)
    config = pinned(report['config'])
    assert len(config['updates']) == 200
    completion = pinned(report['completion'])
    assert completion['first_update'] == 0 and completion['next_update'] == 200
    assert len(completion['receipts']) == 200
    tasks = [(i, episodes, {k: v for k, v in report['outputs'].items() if k.startswith(f'{i:04}/')},
              completion['receipts'][i])
             for i, episodes in enumerate(config['updates'])]
    rows = []
    with ProcessPoolExecutor(max_workers=workers) as pool:
        for completed, result in enumerate(pool.map(training_update, tasks), 1):
            rows.extend(result)
            if completed % 50 == 0:
                print(f'{root.name} saved control trajectories: {completed}/200 updates verified', flush=True)
    totals = defaultdict(Counter)
    for row in rows:
        c = totals[row['deck']]
        c['games'] += 1; c['groups'] += row['groups']; c['substeps'] += row['substeps']
        c['coefficient_mass'] += row['coefficient_mass']
    allgroups = sum(c['groups'] for c in totals.values())
    assert sum(c['games'] for c in totals.values()) == 2000
    assert abs(sum(c['coefficient_mass'] for c in totals.values()) - 200) < 1e-9
    for c in totals.values():
        c['game_share'] = c['games']/2000
        c['group_share'] = c['groups']/allgroups
        c['mean_update_coefficient_share'] = c['coefficient_mass']/200
        c['mean_groups_per_game'] = c['groups']/c['games']
    return dict(audit=audit_pin, report=report_pin, complete=True, games=2000, updates=200,
                decks=dict(totals), episodes=rows)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--joint', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--workers', type=int, default=4)
    args = p.parse_args()
    assert 1 <= args.workers <= 4
    args.output.mkdir(parents=True, exist_ok=False)
    start = time.perf_counter()
    joint, joint_pin = load(args.joint)
    assert joint['complete'] and joint['matches'] == 6144 and len(joint['analyses']) == 2
    result = dict(schema='public-learning-localization/v1', posthoc_descriptive=True,
        formal_gates_unchanged=True, promotion=False, joint=joint_pin,
        script=load_pin(__file__), panels=[], control_training=[])
    for source in joint['analyses']:
        result['panels'].append(panel(source))
        print(f"Verified complete panel {result['panels'][-1]['replica']}", flush=True)
    write(args.output/'paired-outcomes.json', result)
    for source in joint['analyses']:
        result['control_training'].append(training(Path(source['path']).parent, args.workers))
    result.update(complete=True, seconds=time.perf_counter()-start,
        limitations=[
            'Deck and transition decompositions are post hoc descriptions, not new advancement tests.',
            'Same game-one outcome does not imply identical game-one histories or isolate a causal postboard effect.',
            'Third-game participation and starting player can depend on earlier outcomes; no pooled postboard win rate is used.',
            'Group coefficient share is denominator mass before advantage and parameter derivatives, not gradient magnitude or causality.',
            'Only control training is audited; no extrapolation to entropy trajectories.',
            'No independent Fable review: known zero-read quota failure until September 22 07:00 EDT.',
            'No human or league strength claim; no new simulation, training or paid compute.'])
    write(args.output/'result.json', result)
    print(json.dumps(dict(complete=True, seconds=result['seconds'], matches=6144, control_training_games=4000)))


def load_pin(path):
    return dict(path=str(Path(path).resolve()), sha256=hashlib.sha256(Path(path).read_bytes()).hexdigest())


if __name__ == '__main__':
    if not __debug__:
        raise RuntimeError('Validation must remain enabled')
    main()
