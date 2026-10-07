"""Complete-only matched opponent sensitivity profile with fixed g115."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
from functools import cache
from pathlib import Path
import numpy as np

from prepare_postboard_qualification_v1 import BASE, read, write, pin
from postboard_bo3_qualification_v1 import execute, seed
from run_postboard_pilot_v1 import verify_pin
from postboard_repaired_evaluation_v1 import check_adapter_receipts

BASELINE = Path('E:/mtg-postboard-campaign-20260920/pilot-001')
VARIANTS = Path('E:/mtg-postboard-campaign-20260920/published-variant-qualification-001')
QUALIFIED = Path('E:/mtg-postboard-campaign-20260920/spell-target-adapter-qualification-001')
REFERENCES = ('a48', 'v3')


@cache
def checkpoint_metadata(path, sha256):
    checkpoint = read(verify_pin({'path': path, 'sha256': sha256}))
    return {k: checkpoint[k] for k in ('state_sha256', 'source_import')}


def provenance(candidate, opponent):
    entries = {}
    for name, source in [('g115', candidate), ('a48', opponent)]:
        checkpoint = read(verify_pin(source['checkpoint']))
        descriptor = read(verify_pin(source['play_import']))
        initialization = read(verify_pin(descriptor['initialization']))
        verify_pin(descriptor['parameters'])
        assert checkpoint['source_import']['initialization_manifest_sha256'] == descriptor['initialization']['sha256']
        assert checkpoint['source_import']['initial_weights_sha256'] == descriptor['parameters']['sha256']
        assert checkpoint['source_import']['model_init_seed'] == initialization['initializer']['model_init_seed']
        entries[name] = {'source': source, 'initialization': descriptor['initialization'],
                         'initial_parameters': descriptor['parameters'],
                         'initializer': initialization['initializer'], 'lineage_id': initialization['lineage_id'],
                         'state_sha256': checkpoint['state_sha256'], 'adam_step': checkpoint['adam_step']}
    assert entries['g115']['initial_parameters']['sha256'] != entries['a48']['initial_parameters']['sha256']
    original_training = BASE / 'campaign-002-g-block115-cuda.json'
    training = read(original_training)
    assert any(x['source'] == opponent for x in training['opponents'])
    return {'models': entries, 'different_fresh_initializations': True,
            'g115_training_configuration': pin(original_training),
            'a48_in_g115_training_opponents': True, 'held_out_opponent': False,
            'ancestry_scope': 'Checkpoint source-import binding verified against distinct fresh initialization payloads. No claim of independent training experience or untouched experiment selection.'}


def prepare(root):
    prior = read(VARIANTS / 'manifest.json')
    assert read(VARIANTS / 'result.json')['verdict'] == 'ENGINEERING-PASS'
    q = read(QUALIFIED / 'manifest.json')
    source = read(QUALIFIED / 'configs/valid-control.json')['model_sources'][0]
    a48 = read(BASELINE / 'preboard/config.json')['opponents'][0]['source']
    lineage = provenance(source, a48)
    v3 = q['opponent_source']
    verify_pin(v3['play_import']); verify_pin(q['binary'])
    known = {}
    variants = {}
    for job in prior['jobs']:
        c = read(verify_pin(job['config']))
        for match in c['matches']:
            for deck in match['registered']:
                (variants if deck['label'].startswith('published-') else known)[deck['label']] = deck
    assert len(known) == 7 and len(variants) == 8
    decks = dict(sorted(known.items())) | dict(sorted(variants.items()))
    root.mkdir()
    for name in ('configs', 'outputs', 'receipts'):
        (root / name).mkdir()
    write(root / 'opponent-provenance.json', lineage)
    jobs = []
    for own, deck in decks.items():
        for reference in REFERENCES:
            opponent = a48 if reference == 'a48' else v3
            matches, cases = [], []
            for other, other_deck in sorted(known.items()):
                for replicate in range(2):
                    environment = seed('g115-opponent-profile-20260920-v1', own, other, replicate)
                    for seat in (0, 1):
                        order = [deck, other_deck] if seat == 0 else [other_deck, deck]
                        matches.append({'registered': order, 'config': {
                            'deck_ids': [d['label'] for d in order], 'seed': environment,
                            'game_one_chooser': seat if replicate == 0 else 1-seat,
                            'max_physical_games': 6, 'max_physical_decisions': 4000,
                            'max_policy_steps': 40000, 'opening_protocol': 'keep_seven_v2'}})
                        cases.append({'own': own, 'other': other, 'replicate': replicate,
                                      'seat': seat, 'seed': environment, 'reference': reference})
            # One config has fixed policy seats, so divide cases into two jobs.
            for seat in (0, 1):
                name = f'{own}-{reference}-p{seat}'
                configuration = {'mode': 'run_population_batch',
                    'model_sources': [source, opponent] if seat == 0 else [opponent, source],
                    'policies': [{'kind': 'keep'}] * 2, 'cross_generation_evaluation': reference == 'v3',
                    'matches': matches[seat::2], 'output_directory': (root / 'outputs' / name).as_posix()}
                if reference == 'v3':
                    configuration.update(v3_forced_action_passthrough=True, v3_spell_target_reference_adapter=True)
                file = root / 'configs' / (name + '.json'); write(file, configuration)
                jobs.append({'name': name, 'config': pin(file), 'cases': cases[seat::2],
                             'reference': reference, 'seat': seat})
    assert len(jobs) == 60 and sum(len(j['cases']) for j in jobs) == 840
    preflight = []
    for reference in REFERENCES:
        for seat in (0, 1):
            original = next(j for j in jobs if j['reference'] == reference and j['seat'] == seat
                            and j['cases'][0]['own'] == next(iter(variants)))
            c = read(original['config']['path']); c['matches'] = c['matches'][:1]
            name = f'preflight-{reference}-p{seat}'
            c['output_directory'] = (root / 'outputs' / name).as_posix()
            file = root / 'configs' / (name + '.json'); write(file, c)
            preflight.append(dict(original, name=name, config=pin(file), cases=original['cases'][:1]))
    replay = copy.deepcopy(preflight[0]); replay['name'] = 'replay'
    c = read(replay['config']['path']); c['output_directory'] = (root / 'outputs/replay').as_posix()
    file = root / 'configs/replay.json'; write(file, c); replay['config'] = pin(file)
    manifest = {'schema': 'g115-opponent-profile/v1', 'script': pin(__file__),
        'binary': q['binary'], 'candidate': source, 'references': {'a48': a48, 'v3': v3},
        'opponent_provenance': pin(root / 'opponent-provenance.json'),
        'variant_qualification': pin(VARIANTS / 'result.json'), 'variant_selection': prior['selection'],
        'jobs': jobs, 'preflight': preflight, 'replay': replay,
        'gates': {'max_job_seconds': 60, 'max_worker_seconds': 1800, 'workers': 4,
                  'preflight_max_worker_seconds': 20, 'expected_matches': 840},
        'analysis': {'primary': 'G1 terminal score, draw=0.5. BO3 score is secondary Keep-sideboard diagnostic.',
            'pairing': '210 environment-seed cases retain both physical seats and both references.',
            'bootstrap': '10000 paired resamples of 14 opponent/seed cases within each of 15 own-registration strata. Includes variation across seven opponent registrations, not a fixed-matchup conditional interval.',
            'seed': 202609201701, 'reported_difference': 'g115 score against V3 minus g115 score against A48',
            'cohorts': 'Report seven canonical and eight additional lists separately; cohort differences are descriptive, not a causal unseen-list treatment effect.',
            'selection': 'No candidate promotion or stopping for win-rate thresholds. No partial analysis; no per-cell significance or family selection.'},
        'limits': ['A48 has a distinct fresh initialization but was a g115 training opponent, not held out.',
            'Seven known opposing registrations and 15 own lists, equally weighted within the specified sample, not meta-weighted.',
            'Four G1 observations per own/opponent/reference cell; marginal own-list summaries contain 28. Sparse profiles cannot establish matchup strength.',
            'Keep7, Keep sideboarding, sampled policy, no search. Does not test learned openings or postboard swaps.',
            'Fable consultation failed HTTP429 with zero reads until Sep22 07:00 EDT; no retry or endorsement. Proceed under desktop bounded local authority.',
            'No CP7 outcome selection, paid compute or training. No human-strength claim.']}
    write(root / 'manifest.json', manifest)


def audit(job, manifest):
    config = read(verify_pin(job['config'])); directory = Path(config['output_directory'])
    n = len(config['matches'])
    assert read(directory / 'completion.json')['completed_matches'] == n
    assert len(list(directory.glob('match-*.json'))) == n
    start = read(directory / 'run-start.json')
    assert start['binary']['sha256'] == manifest['binary']['sha256']
    if job['reference'] == 'v3':
        check_adapter_receipts(directory, 1-job['seat'], n)
    candidate = checkpoint_metadata(**manifest['candidate']['checkpoint'])
    other = checkpoint_metadata(**manifest['references']['a48']['checkpoint'])
    baseline_v3 = read(QUALIFIED / 'outputs/valid-control/match-000000.json')['play_models'][1]
    rows = []
    for i, (item, case) in enumerate(zip(config['matches'], job['cases'])):
        m = read(directory / f'match-{i:06}.json'); seat = case['seat']
        assert m['config'] == item['config'] and m.get('search_usage') is None
        expected_generations = ['v4', 'v4']; expected_generations[1-seat] = 'v3' if job['reference'] == 'v3' else 'v4'
        assert m['seat_generations'] == expected_generations
        assert m['play_models'][seat]['checkpoint_sha256'] == manifest['candidate']['checkpoint']['sha256']
        assert m['play_models'][seat]['state_sha256'] == candidate['state_sha256']
        if job['reference'] == 'v3':
            assert m['play_models'][1-seat] == baseline_v3
        else:
            assert m['play_models'][1-seat]['checkpoint_sha256'] == manifest['references']['a48']['checkpoint']['sha256']
            assert m['play_models'][1-seat]['state_sha256'] == other['state_sha256']
            assert m['play_models'][1-seat]['source_import'] == other['source_import']
        for actual, expected in zip(m['explicit_registrations'], item['registered']):
            assert {k: actual[k] for k in ('label', 'mainboard', 'sideboard')} == expected
        hashes = [r['mainboard_sha256'] for r in m['explicit_registrations']]
        assert 2 <= len(m['games']) <= 6
        assert all(g['mainboard_sha256'] == hashes for g in m['games'])
        assert all(r['selected_actions'] == [{'kind': 'done'}] for r in m['sideboard_decisions'])
        assert {(r['input']['next_game_number'], r['acting_player']) for r in m['sideboard_decisions']} == {(g['start']['game_index'], s) for g in m['games'][1:] for s in (0, 1)}
        assert (m['games'][0]['start']['starting_player'] == seat) == (case['replicate'] == 0)
        winner = None if m['outcome'] == 'draw' else m['outcome']['winner']['winner']
        g1 = m['games'][0]['winner']
        rows.append(dict(case, g1_score=0.5 if g1 is None else float(g1 == seat),
                         bo3_score=0.5 if winner is None else float(winner == seat),
                         bo3_draw=winner is None, games=len(m['games']),
                         match=pin(directory / f'match-{i:06}.json')))
    return rows


def run(root):
    m = read(root / 'manifest.json'); verify_pin(m['script']); verify_pin(m['binary'])
    assert pin(__file__) == m['script']
    checks = [execute(root, j, m) for j in m['preflight'] + [m['replay']]]
    assert all(r['exit_code'] == 0 and not r['timed_out'] for r in checks)
    for j in m['preflight'] + [m['replay']]:
        audit(j, m)
    a = Path(read(m['preflight'][0]['config']['path'])['output_directory']) / 'match-000000.json'
    b = root / 'outputs/replay/match-000000.json'; assert a.read_bytes() == b.read_bytes()
    seconds = sum(r['elapsed_seconds'] for r in checks)
    projection = seconds / 5 * 840
    write(root / 'preflight-result.json', {'worker_seconds': seconds, 'projected_panel_worker_seconds': projection,
                                          'byte_identical_replay': pin(b), 'results': checks})
    assert seconds <= 20 and projection <= 1800
    print({'preflight': 'PASS', 'projected_worker_seconds': projection}, flush=True)
    with ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(lambda j: execute(root, j, m), m['jobs']))
    write(root / 'execution.json', results)
    assert all(r['exit_code'] == 0 and not r['timed_out'] for r in results), 'Incomplete; preserve outputs and do not analyze'
    rows = [r for j in m['jobs'] for r in audit(j, m)]
    assert len(rows) == 840 and len({(r['own'], r['other'], r['replicate'], r['seat'], r['reference']) for r in rows}) == 840
    seconds += sum(r['elapsed_seconds'] for r in results); assert seconds <= 1800
    write(root / 'audited-matches.json', {'rows': rows, 'worker_seconds_including_preflight': seconds})
    print({'complete_matches': len(rows), 'natural_games': sum(r['games'] for r in rows), 'worker_seconds': seconds}, flush=True)


def analyze(root):
    m = read(root / 'manifest.json'); assert pin(__file__) == m['script']
    a = read(root / 'audited-matches.json'); rows = a['rows']; assert len(rows) == 840
    for row in rows: verify_pin(row['match'])
    keys = {tuple(r[k] for k in ('own', 'other', 'replicate', 'seat', 'reference')): r for r in rows}
    owns = sorted({r['own'] for r in rows}); others = sorted({r['other'] for r in rows})
    values = np.empty((15, 14, 2, 2, 2))
    for i, own in enumerate(owns):
        for j, other in enumerate(others):
            for rep in range(2):
                seeds = set()
                for seat in (0, 1):
                    for refindex, ref in enumerate(REFERENCES):
                        row = keys[(own, other, rep, seat, ref)]; seeds.add(row['seed'])
                        values[i, j*2+rep, seat, refindex] = [row['g1_score'], row['bo3_score']]
                assert len(seeds) == 1
    rng = np.random.Generator(np.random.PCG64(m['analysis']['seed']))
    samples = np.empty((10000, 15, 2, 2))
    for i in range(15):
        indices = rng.integers(0, 14, size=(10000, 14))
        samples[:, i] = values[i, indices].mean(axis=(1, 2))
    cohorts = {}
    for group, selected in [('all', list(range(15))), ('canonical', [i for i,n in enumerate(owns) if not n.startswith('published-')]), ('additional', [i for i,n in enumerate(owns) if n.startswith('published-')])]:
        v = values[selected]; b = samples[:, selected].mean(axis=1)
        means = v.mean(axis=(0, 1, 2)); total = len(selected)*28
        cohorts[group] = {'matches_per_reference': total,
            'references': {r: {'g1_score': float(v[:,:,:,j,0].sum()), 'bo3_score': float(v[:,:,:,j,1].sum()),
                'g1_fraction': float(means[j,0]), 'g1_paired_cluster_95': np.quantile(b[:,j,0],[.025,.975]).tolist()} for j,r in enumerate(REFERENCES)},
            'v3_minus_a48_g1': float(means[1,0]-means[0,0]),
            'v3_minus_a48_paired_95': np.quantile(b[:,1,0]-b[:,0,0],[.025,.975]).tolist()}
    breakdown = [{'own':own,'reference':ref,'matches':28,
                  'g1_score':float(values[i,:,:,j,0].sum()),'bo3_score':float(values[i,:,:,j,1].sum())}
                 for i,own in enumerate(owns) for j,ref in enumerate(REFERENCES)]
    result = {'complete': True, 'matches':840, 'natural_games':sum(r['games'] for r in rows),
              'worker_seconds_including_preflight':a['worker_seconds_including_preflight'],
              'cohorts':cohorts,'per_own_registration':breakdown,'manifest':pin(root/'manifest.json'),
              'promotion':False, 'human_strength_claim':False, 'limits':m['limits']}
    write(root/'analysis.json', result)
    print({'cohorts':cohorts}, flush=True)


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('mode',choices=['prepare','run','analyze']); p.add_argument('--root',required=True,type=Path)
    args=p.parse_args(); globals()[args.mode](args.root.resolve())
