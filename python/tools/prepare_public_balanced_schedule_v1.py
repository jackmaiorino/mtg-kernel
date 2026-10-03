"""Freeze a schedule-only comparison; no training or evaluation dispatch."""
import argparse
import copy
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import subprocess

from public_training_dispatch_v2 import read, write, pin, checked
from state_prevention_pilot_v1 import collect_seeds

NAMESPACE = 'public-balanced-schedule-20260921/v1/'
REFERENCE = Path('E:/mtg-postboard-campaign-20260921/public-entropy-pilot-001')
FOCAL = 'published-44ae71e1e126b63d'


def digest(label):
    return hashlib.sha256((NAMESPACE + label).encode()).digest()


def census(config):
    rows = [e for batch in config['updates'] for e in batch]
    own, opponent, pairs, nuisance = Counter(), Counter(), Counter(), Counter()
    for e in rows:
        s = e['learner_seat']
        a, b = [e['registered'][x]['label'] for x in (s, 1-s)]
        own[a] += 1; opponent[b] += 1; pairs[f'{a} vs {b}'] += 1
        nuisance[json.dumps([a, b, s, e['starting_player'] == s, e['postboard'],
                             e['opponent']['checkpoint']['sha256']])] += 1
    return dict(games=len(rows), learner=dict(own), opponent=dict(opponent),
                ordered_matchups=dict(pairs), joint_nuisance=dict(nuisance))


def schedule(base):
    episodes = [e for b in base['updates'] for e in b]
    assert len(episodes) == 2000 and all(len(b) == 10 for b in base['updates'])
    roster, boards = {}, {}
    for e in episodes:
        s = e['learner_seat']
        a, b = [e['registered'][x]['label'] for x in (s, 1-s)]
        for deck in e['registered']:
            assert deck['label'] not in roster or roster[deck['label']] == deck
            roster[deck['label']] = deck
        key = (a, b, e['postboard'])
        selected = [e['selected'][s], e['selected'][1-s]]
        assert key not in boards or boards[key] == selected
        boards[key] = selected
    decks = sorted(roster)
    assert len(decks) == 8 and len(boards) == 126
    for postboard in (False, True):
        assert (FOCAL, FOCAL, postboard) not in boards
        boards[FOCAL, FOCAL, postboard] = [roster[FOCAL], roster[FOCAL]]
    assert len(boards) == 128
    # 31 full ordered-pair rounds plus two cyclic matchings: 250 games per deck
    # in both learner and opponent roles; 16 cells have32 games and48 have31.
    bag = [(a, b, repeat) for repeat in range(31) for a in decks for b in decks]
    bag += [(a, decks[(i+offset) % 8], 31) for offset in (0, 1) for i, a in enumerate(decks)]
    bag.sort(key=lambda row: (digest('allocation/' + json.dumps(row)), row))
    assert len(bag) == 2000 and len(set(bag)) == 2000
    treatment = copy.deepcopy(base)
    for index, (a, b, _) in enumerate(bag):
        e = treatment['updates'][index//10][index % 10]
        s = e['learner_seat']
        e['registered'] = copy.deepcopy([roster[a], roster[b]])
        e['selected'] = copy.deepcopy(boards[a, b, e['postboard']])
        if s == 1:
            e['registered'].reverse(); e['selected'].reverse()
        for registration, selected in zip(e['registered'], e['selected']):
            assert len(selected['mainboard']) == 60 and len(selected['sideboard']) == 15
            assert Counter(registration['mainboard']+registration['sideboard']) == Counter(selected['mainboard']+selected['sideboard'])
        original = episodes[index]
        assert {k for k in e if e[k] != original[k]} <= {'registered', 'selected'}
        if not e['postboard']:
            assert e['registered'] == e['selected']
    c = census(treatment)
    assert set(c['learner'].values()) == set(c['opponent'].values()) == {250}
    assert Counter(c['ordered_matchups'].values()) == {31: 48, 32: 16}
    return treatment


def prepare(root):
    assert not root.exists(), 'preserve previous preparation'
    old = read(REFERENCE/'replica-1/manifest.json')
    base = read(checked(old['training_configs']['control']))
    assert not base['inputs_enabled'] and base['entropy_coefficient'] == 0
    assert base['projection_mode'] == 'state_only'
    assert base['source']['checkpoint']['sha256'] == '88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'
    for item in [old['training_binary'], old['evaluation_binary'], old['native_build']]:
        checked(item)
    balanced = schedule(base)
    assert schedule(base) == balanced, 'schedule nondeterminism'
    prior, forbidden, used = {}, set(), set()

    def add_prior(item):
        path = checked(item)
        if str(path) not in prior:
            prior[str(path)] = item
            forbidden.update(collect_seeds(read(path)))

    for replica in (1, 2):
        m = read(REFERENCE/f'replica-{replica}/manifest.json')
        for item in [*m['training_configs'].values(), *m['prior_seed_inputs'],
                     *[j['template'] for j in m['jobs']]]:
            add_prior(item)

    def fresh(label):
        value = int.from_bytes(digest(label)[:8], 'little')
        assert value not in forbidden and value not in used
        used.add(value)
        return value

    root.mkdir(parents=True)
    manifests = []
    design = pin(Path('docs/public_balanced_schedule_design_20260921.md').resolve())
    for replica in (1, 2):
        folder = root/f'replica-{replica}'
        (folder/'configs').mkdir(parents=True); (folder/'templates').mkdir()
        configs = {arm: copy.deepcopy(c) for arm, c in [('control', base), ('balanced', balanced)]}
        for i in range(200):
            for slot in range(10):
                value = fresh(f'replica-{replica}/train/{i}/{slot}')
                for arm, config in configs.items():
                    e = config['updates'][i][slot]
                    e.update(seed=value, id=f'balanced-r{replica}-i{i:03}-s{slot:02}')
                a, b = [configs[arm]['updates'][i][slot] for arm in ('control', 'balanced')]
                assert {k for k in a if a[k] != b[k]} <= {'registered', 'selected'}
        assert {k for k in configs['control'] if configs['control'][k] != configs['balanced'][k]} == {'updates'}
        pins = {}
        for arm, config in configs.items():
            write(folder/f'configs/{arm}.json', config)
            pins[arm] = pin(folder/f'configs/{arm}.json')
        eval_seeds = {}
        jobs = []
        for original in old['jobs']:
            job = copy.deepcopy(original)
            request = read(checked(job['template']))
            for case, match in zip(job['cases'], request['matches']):
                key = (case['own'], case['opponent'], case['replicate'])
                if key not in eval_seeds:
                    eval_seeds[key] = fresh(f'replica-{replica}/eval/' + json.dumps(key))
                case['seed'] = eval_seeds[key]
                match['config']['seed'] = eval_seeds[key]
            write(folder/f"templates/{job['label']}.json", request)
            job['template'] = pin(folder/f"templates/{job['label']}.json")
            jobs.append(job)
        assert len(eval_seeds) == 512
        cells = Counter((c['own'], c['opponent'], c['candidate_seat'], c['candidate_starts'])
                        for j in jobs for c in j['cases'])
        assert len(cells) == 256 and set(cells.values()) == {4}
        gates = dict(net_bo3_wins_over_control=20, paired_bo3_95_lower=0.0,
            wins_at_least_g115=True, game_one_paired_95_lower=-0.05,
            minimum_nonnegative_own_decks=6, maximum_own_deck_net_loss=8,
            bootstrap_replicates=10000, bootstrap_seed=fresh(f'replica-{replica}/bootstrap'), final_update=199)
        m = dict(schema='matched-public-balanced-schedule/v1', stage='prepared-not-launched',
            replica=replica, runner=pin(__file__), design=design,
            analysis=pin(Path(__file__).with_name('public_balanced_schedule_analysis_v1.py')),
            bootstrap_implementation=pin(Path(__file__).with_name('state_prevention_analysis_v1.py')),
            source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
            dependencies=[pin(Path(__file__).with_name(n)) for n in ['public_training_dispatch_v2.py', 'state_prevention_pilot_v1.py']],
            native_build=old['native_build'], training_binary=old['training_binary'],
            evaluation_binary=old['evaluation_binary'], evaluation_opponent=old['evaluation_opponent'],
            source=base['source'], training_configs=pins, schedule_input=old['training_configs']['control'],
            jobs=jobs, prior_seed_inputs=list(prior.values()), seed_namespace=NAMESPACE,
            training_unique_seeds=2000, evaluation_unique_seeds=512,
            training_census={a: census(c) for a, c in configs.items()},
            expected_training_games_per_arm=2000, expected_matches_per_arm=1024, expected_matches=3072,
            gates=gates, allocation='Unqualified; exact throughput/storage and v2 evaluation guards required.',
            review=old['review'],
            non_claim='Fixed curricula with independent environment-seed replicas; no CP7, pooled rescue, automatic promotion or human/league claim.')
        write(folder/'manifest.json', m); manifests.append(pin(folder/'manifest.json'))
    assert len(used) == 5026
    write(root/'manifest.json', dict(schema='two-replica-public-balanced-schedule/v1', replicas=manifests,
        runner=pin(__file__), design=design, training_games=8000, evaluation_bo3=6144,
        unique_training_seeds=4000, unique_evaluation_seeds=1024, independent_bootstrap_seeds=2,
        prior_seed_count=len(forbidden), paired_arms=True, matched_slot_nuisance=True,
        measurement_started=False, both_replicas_required=True, paid_compute=False))
    print(dict(prepared=str(root), training_games=8000, evaluation_bo3=6144, launched=False), flush=True)


if __name__ == '__main__':
    if not __debug__:
        raise RuntimeError('Validation must remain enabled')
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, required=True)
    prepare(parser.parse_args().root)
