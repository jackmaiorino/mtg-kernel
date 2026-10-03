"""Offline admission of the bounded D4 evaluation timing panel, never execution."""
from collections import Counter


def validate_panel(manifest, configs):
    if manifest.get('schema') != 'g115-d4-eval-timing-workload/v1':
        raise ValueError('only evaluation timing workload supported')
    arms = {'g115', 'control', 'broader'}
    canonical = set(manifest['canonical'])
    additional = set(manifest['additional'])
    if len(canonical) != 7 or len(additional) != 8 or canonical & additional:
        raise ValueError('requires seven canonical and eight additional lists')
    if set(manifest['sources']) != arms or len(configs) != 90 or len(manifest['jobs']) != 90:
        raise ValueError('requires all three endpoints and ninety jobs')
    counts, seeds = Counter(), {}
    for job, config in zip(manifest['jobs'], configs):
        arm, own, seat = job['arm'], job['own'], job['seat']
        if arm not in arms or own not in canonical | additional or seat not in (0, 1):
            raise ValueError('unknown panel cell')
        counts[arm, own, seat] += 1
        sources = [manifest['sources'][arm], manifest['opponent']]
        if seat:
            sources.reverse()
        if config['model_sources'] != sources:
            raise ValueError('model or seat assignment changed')
        if (config['mode'] != 'run_population_batch' or config['cross_generation_evaluation']
                or config['policies'] != [{'kind': 'keep'}, {'kind': 'keep'}]
                or len(config['matches']) != 7):
            raise ValueError('evaluation mode, policy or batch differs')
        others = []
        for match in config['matches']:
            c = match['config']
            other = match['registered'][1-seat]['label']
            if other not in canonical:
                raise ValueError('opponent population changed')
            others.append(other)
            order = [own, other] if seat == 0 else [other, own]
            if match['registered'] != [manifest['registrations'][label] for label in order]:
                raise ValueError('registered deck bytes changed')
            expected = dict(deck_ids=order, seed=c['seed'], game_one_chooser=seat,
                            max_physical_games=6, max_physical_decisions=4000,
                            max_policy_steps=40000, opening_protocol='keep_seven_v2')
            if c != expected or set(match) != {'config', 'registered'}:
                raise ValueError('native match limits or schema changed')
            pair = (own, other)
            if pair in seeds and seeds[pair] != c['seed']:
                raise ValueError('arms and seats must share seed blocks')
            seeds[pair] = c['seed']
        if set(others) != canonical:
            raise ValueError('opponent cell omitted or repeated')
    expected_cells = {(a, d, s) for a in arms for d in canonical | additional for s in (0, 1)}
    if set(counts) != expected_cells or set(counts.values()) != {1}:
        raise ValueError('endpoint, own-list or seat omitted or repeated')
    if len(set(seeds.values())) != 105:
        raise ValueError('independent matchup seed collision')
    return dict(jobs=90, matches=630, shared_seed_blocks=105, endpoints=3)
