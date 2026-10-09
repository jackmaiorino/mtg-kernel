"""Execute an outcome-blind sample of supported published lists beyond seven training lists."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import copy
from pathlib import Path
import sys

from prepare_postboard_qualification_v1 import BASE, REPO, read, write, pin
from phase1_breadth_v1.catalog_v1 import canonical_zones, digest, to_native
from postboard_bo3_qualification_v1 import execute, seed
from run_postboard_pilot_v1 import verify_pin


def distance(a, b):
    return sum(abs(a[z].get(n, 0) - b[z].get(n, 0))
               for z in ('mainboard', 'sideboard') for n in set(a[z]) | set(b[z]))


def audit_keep(path, item, expected_models):
    match = read(path)
    assert match['config'] == item['config']
    assert match.get('search_usage') is None
    assert match['seat_generations'] == ['v4', 'v4']
    assert match['play_models'] == expected_models
    for actual, expected in zip(match['explicit_registrations'], item['registered']):
        assert {k: actual[k] for k in ('label', 'mainboard', 'sideboard')} == expected
    assert 2 <= len(match['games']) <= 6
    assert match['outcome'] == 'draw' or 'winner' in match['outcome']
    hashes = [r['mainboard_sha256'] for r in match['explicit_registrations']]
    for game in match['games']:
        assert game['mainboard_sha256'] == hashes
    seen = set()
    for row in match['sideboard_decisions']:
        seat = row['acting_player']
        assert row['selected_actions'] == [{'kind': 'done'}]
        assert Counter(row['initial_mainboard']) == Counter(item['registered'][seat]['mainboard'])
        assert Counter(row['initial_sideboard']) == Counter(item['registered'][seat]['sideboard'])
        assert row['selected_mainboard_sha256'] == hashes[seat]
        seen.add((row['input']['next_game_number'], seat))
    assert seen == {(g['start']['game_index'], s) for g in match['games'][1:] for s in (0, 1)}
    return len(match['games'])


def run(root):
    inventory_path = Path('E:/mtg-postboard-campaign-20260920/registration-gap-audit-001/inventory.json')
    inventory = read(inventory_path)
    provenance = inventory['provenance']
    snapshot = read(verify_pin(provenance['snapshot']))
    aliases = snapshot['build_provenance']['card_name_aliases']
    rows = read(verify_pin(provenance['registrations']))
    cards_path = verify_pin(provenance['current_registry'])
    implementation = verify_pin(provenance['inventory_implementation'])
    sys.path.insert(0, str(implementation.parent))
    from card_gap_inventory import inventory as recompute_inventory
    recomputed = recompute_inventory(rows, read(cards_path), aliases)
    corrections = {'L\u00c3\u00b3rien Revealed': 'L\u00f3rien Revealed',
                   'Troll of Khazad-d\u00c3\u00bbm': 'Troll of Khazad-d\u00fbm'}
    repaired = copy.deepcopy(inventory['inventory'])
    corrected_paths = []
    for i, card in enumerate(repaired['cards']):
        before = card['source_names']
        after = [corrections.get(n, n) for n in before]
        if after != before:
            corrected_paths.append({'path': f'inventory.cards[{i}].source_names', 'before': before, 'after': after})
            card['source_names'] = after
    assert len(corrected_paths) == 2 and repaired == recomputed, 'Unexpected inventory difference beyond two source-name text encodings'
    assert {corrections.get(k, k): v for k, v in provenance['aliases'].items()} == aliases
    cards = {c['name']: (i, c) for i, c in enumerate(read(cards_path)['cards'])}
    known = {r['archetype']: canonical_zones(r, aliases)
             for r in read(BASE / 'catalog/registrations.json')}
    known_keys = {digest(r) for r in known.values()}
    good = {r['registration_id'] for r in inventory['inventory']['registration_gaps']
            if r['registry_candidate_only'] and r['mainboard_size'] == 60 and r['sideboard_size'] == 15}
    groups = {}
    for row in rows:
        if row['registration_id'] not in good:
            continue
        assert row['split'] == 'unassigned', 'Do not consume assigned confirmation lists'
        zones = canonical_zones(row, aliases)
        key = digest(zones)
        if key in known_keys:
            continue
        to_native(zones, key, cards)  # Also verify copy limits and no tokens.
        group = groups.setdefault(key, {'zones': zones, 'registration_ids': [], 'source_list_sha256': row['list_sha256']})
        assert group['source_list_sha256'] == row['list_sha256']
        group['registration_ids'].append(row['registration_id'])
    # Greedy farthest-point diversity is fixed from card counts only, with hash
    # tie breaks. It is not an archetype classifier or an outcome-based ranking.
    selected = []
    references = list(known.values())
    remaining = dict(groups)
    for _ in range(8):
        key = min(remaining, key=lambda k: (-min(distance(remaining[k]['zones'], r) for r in references), k))
        group = remaining.pop(key)
        selected.append({'zoned_sha256': key, **group,
                         'minimum_zoned_l1_to_training_lists': min(distance(group['zones'], r) for r in known.values())})
        references.append(group['zones'])
    qroot = Path('E:/mtg-postboard-campaign-20260920/spell-target-adapter-qualification-001')
    q = read(qroot / 'manifest.json')
    binary = q['binary']; verify_pin(binary)
    source = read(qroot / 'configs/valid-control.json')['model_sources'][0]
    for field in ('checkpoint', 'play_import'):
        verify_pin(source[field])
    root.mkdir()
    for name in ('configs', 'outputs', 'receipts'):
        (root / name).mkdir()
    write(root / 'selected-registrations.json', selected)
    write(root / 'inventory-text-correction.json', {'original': pin(inventory_path),
        'snapshot': provenance['snapshot'], 'authoritative_aliases': aliases,
        'original_aliases': provenance['aliases'], 'corrected_paths': corrected_paths,
        'all_other_inventory_fields_exactly_equal': True, 'original_preserved': True})
    jobs = []
    for index, group in enumerate(selected):
        own = 'published-' + group['zoned_sha256'][:16]
        own_deck = to_native(group['zones'], own, cards)
        matches = []
        for opponent, zones in sorted(known.items()):
            other = to_native(zones, opponent, cards)
            for seat in (0, 1):
                order = [own_deck, other] if seat == 0 else [other, own_deck]
                matches.append({'registered': order, 'config': {
                    'deck_ids': [d['label'] for d in order],
                    'seed': seed('published-list-runtime-20260920-v1', group['zoned_sha256'], opponent),
                    'game_one_chooser': seat, 'max_physical_games': 6,
                    'max_physical_decisions': 4000, 'max_policy_steps': 40000,
                    'opening_protocol': 'keep_seven_v2'}})
        name = f'variant-{index:02}'
        config = {'mode': 'run_population_batch', 'model_sources': [source, source],
                  'cross_generation_evaluation': False, 'policies': [{'kind': 'keep'}] * 2,
                  'matches': matches, 'output_directory': (root / 'outputs' / name).as_posix()}
        file = root / 'configs' / (name + '.json')
        write(file, config)
        jobs.append({'name': name, 'config': pin(file)})
    first = read(jobs[0]['config']['path'])
    first['matches'] = first['matches'][:2]
    first['output_directory'] = (root / 'outputs/preflight').as_posix()
    write(root / 'configs/preflight.json', first)
    replay = copy.deepcopy(first)
    replay['matches'] = replay['matches'][:1]
    replay['output_directory'] = (root / 'outputs/replay').as_posix()
    write(root / 'configs/replay.json', replay)
    manifest = {'schema': 'published-variant-runtime-qualification/v1', 'script': pin(__file__),
                'binary': binary, 'source': source, 'inventory': pin(inventory_path),
                'registry': pin(cards_path), 'training_registrations': pin(BASE / 'catalog/registrations.json'),
                'aliases_from_snapshot': aliases, 'snapshot': provenance['snapshot'],
                'prior_inventory_alias_provenance_differs': aliases != provenance['aliases'],
                'inventory_reproduced_except_two_recorded_source_name_encodings': True,
                'selection': pin(root / 'selected-registrations.json'), 'candidate_unique_zoned_lists': len(groups),
                'jobs': jobs, 'gates': {'max_job_seconds': 45, 'max_worker_seconds': 360,
                    'preflight_max_worker_seconds': 15, 'workers': 2, 'complete_matches': 112},
                'question': 'Can eight card-count-diverse additional published registrations execute naturally under the actual g115 V4 scorer against all seven training registrations in both seats?',
                'selection_rule': 'Greedy farthest point by zoned card-count L1, starting from seven training lists; lexicographic zoned SHA breaks ties; no tournament or model outcomes used.',
                'limitations': ['Engineering only, no win-rate analysis or promotion.',
                    'These lists are now development-exposed, not a pristine held-out confirmation set; earlier ancestry exposure is not established.',
                    'Keep7 and Keep sideboarding. Registered sideboards are validated but their cards are not necessarily played. This does not qualify postboard competence.',
                    'g115 plays both seats. This does not supply an independent opponent or measure competitive strength.',
                    'Static Full declarations and natural completion do not establish rules parity or whole-field coverage.',
                    'Fable zero-read HTTP429 review gap until September22 07:00 EDT; no retry or endorsement. The maintainer authorized bounded local continuation.'],
                'no_cp7_selection': True, 'paid_compute': False}
    write(root / 'manifest.json', manifest)
    preflight = execute(root, {'name': 'preflight', 'config': pin(root / 'configs/preflight.json')}, manifest)
    assert preflight['exit_code'] == 0 and not preflight['timed_out'], preflight
    repeated = execute(root, {'name': 'replay', 'config': pin(root / 'configs/replay.json')}, manifest)
    assert repeated['exit_code'] == 0 and not repeated['timed_out'], repeated
    a, b = root / 'outputs/preflight/match-000000.json', root / 'outputs/replay/match-000000.json'
    assert a.read_bytes() == b.read_bytes()
    expected = read(a)['play_models']
    # Both seats must bind the same full g115 state as the already audited panel.
    qualified = read(qroot / 'outputs/valid-control/match-000000.json')['play_models'][0]
    assert expected == [qualified, qualified]
    for i, item in enumerate(first['matches']):
        audit_keep(root / 'outputs/preflight' / f'match-{i:06}.json', item, expected)
    seconds = preflight['elapsed_seconds'] + repeated['elapsed_seconds']
    assert seconds <= manifest['gates']['preflight_max_worker_seconds']
    write(root / 'preflight-result.json', {'complete_matches': 3, 'byte_identical_replay': pin(b), 'worker_seconds': seconds})
    print({'preflight': 'PASS', 'worker_seconds': seconds}, flush=True)
    with ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(lambda job: execute(root, job, manifest), jobs))
    write(root / 'execution.json', results)
    assert all(r['exit_code'] == 0 and not r['timed_out'] for r in results), 'Incomplete; preserve all outputs'
    games, matches = 0, 0
    for job in jobs:
        config = read(job['config']['path'])
        directory = Path(config['output_directory'])
        assert read(directory / 'completion.json')['completed_matches'] == 14
        assert len(list(directory.glob('match-*.json'))) == 14
        for i, item in enumerate(config['matches']):
            games += audit_keep(directory / f'match-{i:06}.json', item, expected)
            matches += 1
    seconds += sum(r['elapsed_seconds'] for r in results)
    assert matches == 112 and seconds <= manifest['gates']['max_worker_seconds']
    result = {'verdict': 'ENGINEERING-PASS', 'additional_exact_registrations': 8,
              'matches': matches, 'natural_games': games, 'worker_seconds_including_preflight': seconds,
              'replay_byte_identical': True, 'outcomes_analyzed': False,
              'independent_opponent_qualified': False, 'postboard_play_qualified': False}
    write(root / 'result.json', result)
    print(result, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    run(parser.parse_args().root.resolve())
