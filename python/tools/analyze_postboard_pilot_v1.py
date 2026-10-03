"""Complete-only paired development analysis; never promotes a model."""
import argparse
from pathlib import Path
import numpy as np
from prepare_postboard_qualification_v1 import read, write, pin
from run_postboard_pilot_v1 import verify_pin, audit_evaluation_arm

ARMS = ('g115', 'preboard', 'mixed')


def interval(samples):
    return np.quantile(samples, [0.025, 0.975], method='linear').tolist()


def bootstrap(values, replicates, seed):
    """[49 matchup strata, 8 seeds, 2 seats, arms, metrics], seats stay paired."""
    assert values.shape == (49, 8, 2, 3, 2)
    rng = np.random.Generator(np.random.PCG64(seed))
    means = np.zeros((replicates, 3, 2))
    for stratum in values:
        indices = rng.integers(0, 8, size=(replicates, 8))
        means += stratum[indices].mean(axis=(1, 2)) / 49
    return means


def analyze(root):
    manifest = read(root / 'manifest.json')
    verify_pin(manifest['analysis_script'])
    assert manifest['analysis_script']['sha256'] == pin(__file__)['sha256']
    records = {}
    worker_seconds = 0
    for stage in ('reference', 'trained'):
        execution = read(root / (stage + '-execution.json'))
        assert all(r['exit_code'] == 0 and not r['timed_out'] for r in execution['results'])
        worker_seconds += sum(r['elapsed_seconds'] for r in execution['results'])
        jobs = read(root / (stage + '-dispatch.json'))['jobs']
        for job in jobs:
            audit = read(root / (job['arm'] + '-evaluation-audit.json'))
            assert audit['complete'] and audit['matches'] == 784
            audited = {p['path']: p for p in audit['match_pins']}
            config = read(verify_pin(job['config']))
            for i, case in enumerate(job['cases']):
                file = Path(config['output_directory']) / f'match-{i:06}.json'
                assert pin(file) == audited[file.resolve().as_posix()]
                match = read(file)
                seat = case['candidate_seat']
                assert match['config']['seed'] == case['seed']
                assert (match['games'][0]['start']['starting_player'] == seat) == case['candidate_starts']
                key = (case['own'], case['opponent'], case['replicate'], seat, job['arm'])
                assert key not in records
                winner = None if match['outcome'] == 'draw' else match['outcome']['winner']['winner']
                g1 = match['games'][0]['winner']
                records[key] = {'seed': case['seed'], 'win': int(winner == seat),
                    'draw': int(winner is None), 'g1_score': 0.5 if g1 is None else int(g1 == seat),
                    'candidate_starts': case['candidate_starts']}
    assert len(records) == manifest['gates']['expected_matches'] == 2352
    assert worker_seconds <= manifest['gates']['max_evaluation_worker_seconds']
    strata = sorted({key[:2] for key in records})
    assert len(strata) == 49
    values = np.empty((49, 8, 2, 3, 2))
    for i, matchup in enumerate(strata):
        for rep in range(8):
            seeds = set()
            for seat in (0, 1):
                for arm_index, arm in enumerate(ARMS):
                    row = records[(*matchup, rep, seat, arm)]
                    seeds.add(row['seed'])
                    values[i, rep, seat, arm_index] = [row['win'], row['g1_score']]
            assert len(seeds) == 1
    samples = bootstrap(values, manifest['gates']['bootstrap_replicates'], manifest['gates']['bootstrap_seed'])
    means = values.mean(axis=(0, 1, 2))
    comparisons = {}
    for a, b in ((2, 1), (2, 0), (1, 0)):
        comparisons[f'{ARMS[a]}-minus-{ARMS[b]}'] = {
            'net_bo3_wins': int(round((means[a, 0] - means[b, 0]) * 784)),
            'bo3_win_fraction_difference': float(means[a, 0] - means[b, 0]),
            'bo3_paired_95_interval': interval(samples[:, a, 0] - samples[:, b, 0]),
            'game_one_score_difference': float(means[a, 1] - means[b, 1]),
            'game_one_paired_95_interval': interval(samples[:, a, 1] - samples[:, b, 1])}
    control = comparisons['mixed-minus-preboard']
    reference = comparisons['mixed-minus-g115']
    primary = control['net_bo3_wins'] >= 24 and control['bo3_paired_95_interval'][0] > 0
    retention = all(c['game_one_paired_95_interval'][0] > -0.05 for c in (control, reference))
    summary = {arm: {'matches': 784, 'wins': sum(v['win'] for k, v in records.items() if k[-1] == arm),
        'draws': sum(v['draw'] for k, v in records.items() if k[-1] == arm),
        'game_one_score': sum(v['g1_score'] for k, v in records.items() if k[-1] == arm)} for arm in ARMS}
    breakdown = []
    for matchup in strata:
        for seat in (0, 1):
            for starts in (False, True):
                for arm in ARMS:
                    rows = [v for k, v in records.items() if k[:2] == matchup and k[3:] == (seat, arm) and v['candidate_starts'] == starts]
                    assert len(rows) == 4
                    breakdown.append({'own': matchup[0], 'opponent': matchup[1], 'seat': seat,
                        'candidate_starts': starts, 'arm': arm, 'matches': 4,
                        'wins': sum(r['win'] for r in rows), 'draws': sum(r['draw'] for r in rows),
                        'game_one_score': sum(r['g1_score'] for r in rows)})
    result = {'complete': True, 'manifest': pin(root / 'manifest.json'), 'matches': len(records),
        'summary': summary, 'comparisons': comparisons, 'breakdown': breakdown,
        'gates': {'primary': primary, 'retention': retention, 'at_least_reference_wins': reference['net_bo3_wins'] >= 0},
        'verdict': 'REPLICATE' if primary and retention and reference['net_bo3_wins'] >= 0 else 'NO-ADVANCE',
        'evaluation_worker_seconds': worker_seconds, 'numpy_version': np.__version__,
        'limitations': manifest['limitations'], 'promotion': False, 'human_strength_claim': False}
    write(root / 'analysis.json', result)
    print({k: result[k] for k in ('verdict', 'summary', 'comparisons', 'gates')})


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    analyze(parser.parse_args().root.resolve())
