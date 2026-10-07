"""Freeze one 200-update matched development pilot, without executing it."""
import argparse
from collections import Counter
import copy
import json
from pathlib import Path
import shutil
import subprocess

from prepare_postboard_qualification_v1 import BASE, BINARY, BINARY_SHA, SOURCE, pin, read, write
from postboard_bo3_qualification_v1 import seed
from phase1_breadth_v1.schedule_v1 import compile_block_v1


def prepare(root, qualification):
    assert not root.exists()
    q = read(qualification / 'qualification-result.json')
    assert q['verdict'] == 'ENGINEERING-PASS' and q['previous_matches_equal_after_import_provenance_rebinding'] == 94
    qm = read(qualification / 'manifest.json')
    assert qm['v3_forced_action_passthrough'] is True
    training_qualification = Path('E:/mtg-postboard-campaign-20260920/qualification-001')
    tq = read(training_qualification / 'qualification-result.json')
    assert tq['verdict'] == 'ENGINEERING-PASS'
    source = read(SOURCE)
    assert pin(source['checkpoint']['path'])['sha256'] == source['checkpoint']['sha256']
    assert pin(BINARY)['sha256'] == BINARY_SHA
    assert pin(qm['binary']['path'])['sha256'] == qm['binary']['sha256']
    root.mkdir(parents=True)
    for directory in ('configs', 'templates', 'receipts', 'outputs', 'preboard', 'mixed'):
        (root / directory).mkdir()
    shutil.copyfile(BINARY, root / BINARY.name)
    shutil.copyfile(qm['binary']['path'], root / 'learned_sideboard_v1.exe')
    catalog = read(training_qualification / 'inputs/catalog.json')
    settings = read(BASE / 'catalog/g-settings.json')
    old = read(BASE / 'campaign-002-g-block115-cuda.json')
    excluded = set(settings['excluded_seeds'])
    excluded.update(x['episode']['seed'] for batch in old['iterations'] for x in batch['episodes'])
    excluded.update(x['seed'] for x in read(training_qualification / 'inputs/paired-schedule.json'))
    settings.update(seed=202609201230, lineage_id='g115-postboard-development-20260920', block_index=0,
                    phase='mixed', initial_source=source, excluded_seeds=sorted(excluded),
                    output_directory=(root / 'mixed/run').as_posix())
    compiled = compile_block_v1(catalog, settings)
    mixed = compiled['native']
    assert len(mixed['iterations']) == 200
    mixed['loss_selection'] = old['loss_selection']
    mixed['update_backend'] = old['update_backend']
    mixed['max_non_natural_episode_fraction'] = 0.0
    plans = {(p['self_deck_id'], p['opponent_deck_id']): p for p in read(qualification / 'static-rows.json')['rows'] if p['game_index'] == 2}
    pairs = []
    for i, batch in enumerate(mixed['iterations']):
        for j, item in enumerate(batch['episodes']):
            e = item['episode']
            families = [r['label'].split('/')[0] for r in e['registered']]
            if e['postboard']:
                for seat in (0, 1):
                    main = Counter(e['registered'][seat]['mainboard'])
                    side = Counter(e['registered'][seat]['sideboard'])
                    plan = plans[(families[seat], families[1-seat])]
                    for row in plan['cards_out']:
                        main[row['card_id']] -= row['count']; side[row['card_id']] += row['count']
                    for row in plan['cards_in']:
                        main[row['card_id']] += row['count']; side[row['card_id']] -= row['count']
                    assert min(main.values()) >= 0 and min(side.values()) >= 0
                    assert sum(main.values()) == 60 and sum(side.values()) == 15
                    e['selected'][seat] = {'label': e['registered'][seat]['label'],
                        'mainboard': sorted(main.elements()), 'sideboard': sorted(side.elements())}
                    assert e['selected'][seat]['mainboard'] != e['registered'][seat]['mainboard']
            pairs.append({'iteration': i, 'slot': j, 'seed': e['seed'], 'postboard': e['postboard'],
                          'learner_seat': e['learner_seat'], 'starting_player': e['starting_player'],
                          'seat_families': families, 'opponent': item['opponent']})
        assert sum(x['episode']['postboard'] for x in batch['episodes']) == 5
    control = copy.deepcopy(mixed)
    control['output_directory'] = (root / 'preboard/run').as_posix()
    for batch in control['iterations']:
        for item in batch['episodes']:
            item['episode']['postboard'] = False
            item['episode']['selected'] = copy.deepcopy(item['episode']['registered'])
    for name, config in [('preboard', control), ('mixed', mixed)]:
        write(root / name / 'config.json', config)
    write(root / 'paired-training-schedule.json', pairs)
    names = sorted(plans_key[0] for plans_key in plans if plans_key[0] == plans_key[1])
    by_name = {r['label']: r for job in qm['jobs'] for m in read(job['config']['path'])['matches'] for r in m['registered']}
    policies = read(qm['jobs'][0]['config']['path'])['policies']
    evaluation_jobs = []
    eval_seeds = set()
    for arm in ('g115', 'preboard', 'mixed'):
        for own in names:
            for seat in (0, 1):
                name = f'{arm}-{own}-p{seat}'
                matches, cases = [], []
                for other in names:
                    for replicate in range(8):
                        case_seed = seed('postboard-development-bo3-20260920-v1', own, other, replicate)
                        assert case_seed not in excluded and case_seed not in {r['seed'] for r in pairs}
                        eval_seeds.add(case_seed)
                        order = [own, other] if seat == 0 else [other, own]
                        matches.append({'config': {'deck_ids': order, 'seed': case_seed,
                            'game_one_chooser': seat if replicate % 2 == 0 else 1-seat,
                            'max_physical_games': 6, 'max_physical_decisions': 4000, 'max_policy_steps': 40000,
                            'opening_protocol': 'keep_seven_v2'}, 'registered': [by_name[x] for x in order]})
                        cases.append({'own': own, 'opponent': other, 'replicate': replicate,
                                      'seed': case_seed, 'candidate_seat': seat, 'candidate_starts': replicate % 2 == 0})
                config = {'mode': 'run_population_batch', 'model_sources': [source, qm['opponent_source']] if seat == 0 else [qm['opponent_source'], source],
                    'cross_generation_evaluation': True, 'v3_forced_action_passthrough': True,
                    'policies': policies, 'matches': matches, 'output_directory': (root / 'outputs' / name).as_posix()}
                path = root / 'templates' / (name + '.json')
                write(path, config)
                evaluation_jobs.append({'name': name, 'arm': arm, 'candidate_seat': seat,
                    'matches': len(matches), 'template': pin(path), 'cases': cases})
    assert len(eval_seeds) == 392 and len(evaluation_jobs) == 42
    write(root / 'manifest.json', {'schema': 'g115-postboard-development-pilot/v1',
        'orchestration_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'analysis_script': pin(Path(__file__).with_name('analyze_postboard_pilot_v1.py')),
        'runner_script': pin(Path(__file__).with_name('run_postboard_pilot_v1.py')),
        'training_source_commit': '5239e656175b7f6d19b260a484b71a3719a5361f',
        'evaluation_build_commit': qm['source_commit'],
        'toolchain': {'rustc': '1.94.1', 'linker': '14.50.35725.0', 'gpu_ordinal': 1},
        'training_binary': pin(root / BINARY.name), 'binary': pin(root / 'learned_sideboard_v1.exe'),
        'training_configs': {a: pin(root / a / 'config.json') for a in ('preboard', 'mixed')},
        'initial_source': source, 'initial_adam_step': 32400, 'final_adam_step': 32600,
        'training_schedule': pin(root / 'paired-training-schedule.json'), 'evaluation_jobs': evaluation_jobs,
        'qualifications': [pin(qualification / 'qualification-result.json'), pin(training_qualification / 'qualification-result.json')],
        'gates': {'updates_per_arm': 200, 'games_per_update': 10, 'max_training_arm_seconds': 1200,
                  'max_job_seconds': 180, 'max_evaluation_worker_seconds': 3600, 'workers': 8,
                  'expected_matches_per_arm': 784, 'expected_matches': 2352,
                  'primary_minimum_net_wins': 24, 'primary_paired_95_lower_bound_minimum': 0.0,
                  'game_one_retention_paired_95_lower_bound_minimum': -0.05,
                  'bootstrap_replicates': 10000, 'bootstrap_seed': 202609201231},
        'analysis': {'unit': '392 common-seed ordered-matchup cases, each with both physical-seat orientations',
            'resampling': 'Paired stratified bootstrap within each of 49 ordered matchups; keep the two seats and all model arms together',
            'primary': 'Mixed minus preboard-control BO3 wins, at least 24 net wins out of 784 and paired 95% lower bound > 0',
            'retention': 'Mixed minus control AND mixed minus untouched g115 game-one scores each have paired 95% lower bound > -0.05',
            'advance': 'Primary plus retention gates, and mixed BO3 wins at least untouched g115 BO3 wins; permits replication, never promotion',
            'other_results': 'Report estimates and uncertainty without selecting intermediate endpoints or tuning from prefixes',
            'endpoint_binding': 'Only the checkpoint from iteration 199, Adam 32600, replaces the candidate source in that arm template; verify complete training first'},
        'limitations': ['One initial g115 lineage, one V3-reference opponent, seven already-trained exact registrations; no field or external-strength claim.',
            'Fixed known-archetype sideboard plans and Keep7 are supplied to both seats; this does not measure learned sideboarding, mulligans or closed-list deployment.',
            'The V3 singleton adapter is explicitly enabled, and multi-choice encoder failures still invalidate incomplete evaluation.',
            'Run untouched g115 evaluation first. Do not train if the reference evaluator fails. No partial outcome interpretation.',
            'Fable review failed HTTP429 before reads until September 22 07:00 EDT; no retry or endorsement, proceeding under desktop authority.',
            'Local GPU 1 only, no paid compute or broader campaign. CP7 outcomes excluded from selection.']})
    return {'training_episodes': 4000, 'updates_per_arm': 200, 'evaluation_matches': 2352, 'paired_cases': 392}


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--root', required=True, type=Path)
    p.add_argument('--qualification', required=True, type=Path)
    a = p.parse_args()
    print(json.dumps(prepare(a.root.resolve(), a.qualification.resolve())))
