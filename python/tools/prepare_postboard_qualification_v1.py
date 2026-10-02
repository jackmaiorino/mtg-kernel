"""Prepare a bounded, matched g115 continuation. Never launches training."""
import argparse
from collections import Counter
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys

from phase1_breadth_v1.catalog_v1 import (
    canonical_zones, combined_key, digest, load_catalog_v1, strict_json, to_native,
)
from phase1_breadth_v1.schedule_v1 import compile_block_v1

REPO = Path(__file__).resolve().parents[2]
BASE = Path('E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001/campaign-002/lineage-g/block115')
SOURCE = Path('E:/mtg-meta-recovery-20260920/fair-opening-timing-4096-001/model-source.json')
BINARY = Path('E:/cargo-target-phase1-gae/release/native_expanded_training_run_v1.exe')
BINARY_SHA = 'a7cc296178407033d677081cbde47446ece6918e0584c09649cd1c043421742e'
SOURCE_COMMIT = '5239e656175b7f6d19b260a484b71a3719a5361f'
MODEL_SHA = '88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1'


def read(path):
    return strict_json(Path(path).read_bytes())


def pin(path):
    path = Path(path).resolve()
    return {'path': path.as_posix(), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def write(path, data):
    with Path(path).open('x', encoding='utf-8', newline='\n') as stream:
        json.dump(data, stream, sort_keys=True, indent=2, allow_nan=False)
        stream.write('\n')


def prepare(root, seed):
    if root.exists():
        raise ValueError('Use a fresh evidence root')
    historical = read(BASE / 'campaign-002-g-block115-cuda.json')
    source = read(SOURCE)
    assert source['checkpoint']['sha256'] == MODEL_SHA
    for model in [source] + [x['source'] for x in historical['opponents']]:
        for key in ('play_import', 'checkpoint'):
            assert pin(model[key]['path'])['sha256'] == model[key]['sha256']
    checkpoint = read(source['checkpoint']['path'])
    assert checkpoint['adam_step'] == 32400
    assert checkpoint['loss_identity'] == 'gae_advantage_value/v1'
    assert pin(BINARY)['sha256'] == BINARY_SHA
    subprocess.run(['git', 'merge-base', '--is-ancestor', SOURCE_COMMIT, 'HEAD'], cwd=REPO, check=True)

    inputs = REPO / 'docs/research/sideboard_plan_inputs_2026-09'
    table_path = inputs / 'plan_table_draft_nine_v2_factchecked.json'
    table = read(table_path)
    draft_registrations = read(inputs / 'registered_nine_75s.json')['decks']
    registrations = read(BASE / 'catalog/registrations.json')
    by_name = {r['archetype']: r for r in registrations}
    registry_path = REPO / 'data/cards_v1.json'
    registry = read(registry_path)
    cards = {c['name']: (i, c) for i, c in enumerate(registry['cards'])}
    named_plans, native_plans, plan_audit = {}, {}, []
    for name, row in by_name.items():
        assert row['split'] == 'train'
        assert row['mainboard'] == draft_registrations[name]['main']
        assert row['sideboard'] == draft_registrations[name]['side']
        assert digest(canonical_zones(row, {})) == row['list_sha256']
        named_plans[row['list_sha256']] = {}
    for plan in table['plans']:
        own, other = plan['self_deck_id'], plan['opponent_deck_id']
        if own not in by_name or other not in by_name or plan['game_index'] != 2:
            continue
        assert (own, other) not in native_plans
        row = by_name[own]
        zones = {z: Counter(row[z]) for z in ('mainboard', 'sideboard')}
        ins, outs = Counter(), Counter()
        for rows, counts in ((plan['cards_in'], ins), (plan['cards_out'], outs)):
            for item in rows:
                assert cards[item['name']][0] == item['card_id']
                assert type(item['count']) is int and item['count'] > 0
                counts[item['name']] += item['count']
        assert not (ins - zones['sideboard']) and not (outs - zones['mainboard'])
        assert sum(ins.values()) == sum(outs.values())
        selected = {'mainboard': dict(zones['mainboard'] - outs + ins),
                    'sideboard': dict(zones['sideboard'] - ins + outs)}
        assert combined_key(selected) == combined_key(row)
        native = to_native(selected, own + '/' + row['list_sha256'][:12], cards)
        native_plans[(own, other)] = native
        named_plans[row['list_sha256']][digest(selected)] = selected
        plan_audit.append({'own': own, 'opponent': other, 'swaps': sum(ins.values()),
                           'selected_sha256': digest(selected), 'registered_sha256': row['list_sha256']})
    assert len(native_plans) == 49

    root.mkdir(parents=True)
    catalog_root = root / 'inputs'
    catalog_root.mkdir()
    shutil.copyfile(BINARY, root / BINARY.name)
    postboards = {'schema': 'phase1-postboard-configurations/v1',
                  'configurations': {k: [v[h] for h in sorted(v)] for k, v in named_plans.items()}}
    write(catalog_root / 'postboards.json', postboards)
    catalog_spec = read(BASE / 'catalog/catalog-source-spec.json')
    catalog_spec['registry'] = pin(registry_path)
    catalog_spec['postboards'] = pin(catalog_root / 'postboards.json')
    catalog_spec['data']['provenance'] = ('Existing seven g115 training registrations, exact zones verified against the unratified '
        'fact-checked draft. This engineering fixture provides no field coverage or sideboard-quality claim.')
    write(catalog_root / 'catalog-source.json', catalog_spec)
    catalog = load_catalog_v1(catalog_spec)
    write(catalog_root / 'catalog.json', catalog)

    settings = read(BASE / 'catalog/g-settings.json')
    settings.update(seed=seed, lineage_id='g115-postboard-qualification-20260920', block_index=0,
                    phase='mixed', initial_source=source, output_directory=(root / 'mixed/run').as_posix())
    settings['excluded_seeds'] = sorted(set(settings['excluded_seeds']) | {
        item['episode']['seed'] for batch in historical['iterations'] for item in batch['episodes']})
    compiled = compile_block_v1(catalog, settings)
    # Retain the existing schedule/RNG and policy mix. Replace uniformly sampled
    # legal sideboards with the frozen opponent-conditioned draft, for both seats.
    mixed = compiled['native']
    mixed['iterations'] = mixed['iterations'][:4]
    mixed['update_backend'] = historical['update_backend']
    mixed['loss_selection'] = historical['loss_selection']
    # New engineering run must retain every matched seed. Any nonnatural game
    # rejects the qualification instead of inheriting g115's resampling tolerance.
    mixed['max_non_natural_episode_fraction'] = 0.0
    for batch in mixed['iterations']:
        for item in batch['episodes']:
            e = item['episode']
            names = [x['label'].split('/')[0] for x in e['registered']]
            if e['postboard']:
                e['selected'] = [copy.deepcopy(native_plans[(names[i], names[1-i])]) for i in (0, 1)]
    control = copy.deepcopy(mixed)
    control['output_directory'] = (root / 'preboard/run').as_posix()
    for batch in control['iterations']:
        for item in batch['episodes']:
            item['episode']['postboard'] = False
            item['episode']['selected'] = copy.deepcopy(item['episode']['registered'])
    replay = copy.deepcopy(mixed)
    replay['output_directory'] = (root / 'mixed-replay/run').as_posix()
    for name, config in [('preboard', control), ('mixed', mixed), ('mixed-replay', replay)]:
        (root / name).mkdir()
        write(root / name / 'config.json', config)
    rows = []
    for i, (a, b) in enumerate(zip(control['iterations'], mixed['iterations'])):
        assert sum(x['episode']['postboard'] for x in b['episodes']) == 5
        for j, (ca, cb) in enumerate(zip(a['episodes'], b['episodes'])):
            common = copy.deepcopy(cb)
            common['episode']['postboard'] = False
            common['episode']['selected'] = copy.deepcopy(common['episode']['registered'])
            assert common == ca
            e = cb['episode']
            rows.append({'iteration': i, 'slot': j, 'id': e['id'], 'seed': e['seed'],
                         'postboard': e['postboard'], 'learner_seat': e['learner_seat'],
                         'starting_player': e['starting_player'], 'opponent': cb['opponent'],
                         'seat_archetypes': [x['label'].split('/')[0] for x in e['registered']],
                         'changed_seats': [e['registered'][k] != e['selected'][k] for k in (0, 1)]})
    write(catalog_root / 'paired-schedule.json', rows)
    write(catalog_root / 'plan-legality.json', {'draft_status': table['status'], 'plans': plan_audit,
        'scope': 'Exact registered 75, 60/15 sizes, available copies, registry ID/name binding and Full support; no strength endorsement'})
    write(root / 'manifest.json', {
        'schema': 'g115-postboard-qualification/v1', 'source_commit': SOURCE_COMMIT,
        'producer_launch': pin(Path('D:/phase1-live/campaign-002/g/block115/start.json')),
        'binary': pin(root / BINARY.name), 'initial_source': source, 'initial_adam_step': 32400,
        'initial_state_sha256': checkpoint['state_sha256'], 'seed': seed,
        'configs': {n: pin(root / n / 'config.json') for n in ('preboard', 'mixed', 'mixed-replay')},
        'inputs': [pin(table_path), pin(inputs / 'registered_nine_75s.json'), pin(BASE / 'catalog/g-settings.json'),
                   pin(BASE / 'campaign-002-g-block115-cuda.json'), pin(SOURCE), pin(catalog_root / 'catalog-source.json'),
                   pin(catalog_root / 'paired-schedule.json'), pin(catalog_root / 'plan-legality.json')],
        'gates': {'updates_per_arm': 4, 'episodes_per_update': 10, 'max_arm_wall_seconds': 120,
                  'natural_episodes_required': True, 'exact_replay_trajectory_hashes_required': True,
                  'same_full_optimizer_state_on_replay_required': True, 'gpu_ordinal': 1},
        'design': 'Four-update engineering qualification of preboard-only versus 50% fixed-draft postboard exposure. Replay mixed in a fresh root, paused after one update and resumed.',
        'limitations': ['One initial g115 lineage and fixed seven training registrations; no independent initialization or current-meta claim.',
          'Draft sideboards are legal but not ratified or demonstrated strong; no learned sideboard or mulligan head.',
          'Standalone terminal games, not match-reward training. No strength outcome is a gate or selection input.',
          'The inherited catalog reserved file is empty. This does not establish a project-wide unseen holdout.',
          'Historical trainer reports build HEAD ad1036cb; launch source is clean commit 5239e656 after the run-harness change. Exact historical binary SHA is retained.',
          'Fable review failed HTTP429 before source reads, reset September 22 07:00 EDT. No retry or endorsement; proceed under Jack authority.'],
    })
    return {'root': root.as_posix(), 'updates_per_arm': 4, 'paired_episodes': len(rows),
            'postboard_episodes': sum(x['postboard'] for x in rows),
            'changed_seat_configurations': sum(sum(x['changed_seats']) for x in rows),
            'learner_families': dict(sorted(Counter(x['seat_archetypes'][x['learner_seat']] for x in rows).items()))}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seed', type=int, default=202609201141)
    args = parser.parse_args()
    print(json.dumps(prepare(args.output.resolve(), args.seed), sort_keys=True))
