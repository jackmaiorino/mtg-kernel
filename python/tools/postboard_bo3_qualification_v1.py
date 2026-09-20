"""Qualify fixed-draft sideboarding in actual BO3, without strength selection."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from prepare_postboard_qualification_v1 import BASE, REPO, SOURCE, pin, read, write

BINARY = Path('E:/cargo-target-phase1-search/release/learned_sideboard_v1.exe')
BINARY_SHA = '44be09ab138b9d19d4f22fd8ea460e755db31a05665821dca2ee7bf4de304d3c'
OLD_CONFIG = Path('D:/phase1-live/campaign-002/yardsticks/search-rollout-m2-block5-vs-v3-candidate-broad-512-002/configs/chunk-00-end-seat0.json')


def seed(domain, *parts):
    return int.from_bytes(hashlib.sha256(json.dumps([domain, *parts], separators=(',', ':')).encode()).digest()[:8], 'big')


def prepare(root):
    assert not root.exists(), 'Fresh root required'
    assert pin(BINARY)['sha256'] == BINARY_SHA
    draft_path = REPO / 'docs/research/sideboard_plan_inputs_2026-09/plan_table_draft_nine_v2_factchecked.json'
    registrations = read(BASE / 'catalog/registrations.json')
    cards = {c['name']: i for i, c in enumerate(read(REPO / 'data/cards_v1.json')['cards'])}
    decks = {r['archetype']: {'label': r['archetype'], **{z: sorted(cards[n] for n, count in r[z].items()
             for _ in range(count)) for z in ('mainboard', 'sideboard')}} for r in registrations}
    names = sorted(decks)
    rows = []
    for p in read(draft_path)['plans']:
        if p['self_deck_id'] in decks and p['opponent_deck_id'] in decks:
            rows.append({k: p[k] for k in ('self_deck_id', 'opponent_deck_id', 'game_index')} | {
                k: [{'card_id': x['card_id'], 'count': x['count']} for x in p[k]] for k in ('cards_in', 'cards_out')})
    assert len(rows) == 98
    source = read(SOURCE)
    opponent = read(OLD_CONFIG)['model_sources'][1]
    for model in (source, opponent):
        for key in ('play_import', 'checkpoint'):
            if model.get(key):
                assert pin(model[key]['path'])['sha256'] == model[key]['sha256']
    root.mkdir(parents=True)
    (root / 'configs').mkdir()
    (root / 'outputs').mkdir()
    (root / 'receipts').mkdir()
    shutil.copyfile(BINARY, root / BINARY.name)
    write(root / 'static-rows.json', {'rows': rows})
    policy = {'kind': 'static_plan_rows', 'table': pin(root / 'static-rows.json'),
              'teacher_provenance': {'source_kind': 'hand_authored_warm_start',
                  'description': 'Unratified fact-checked draft; fixed environment qualification, no teacher-quality claim.',
                  'artifact': pin(draft_path)}, 'carry_game_three_forward': True}
    jobs = []
    for own in names:
        for seat in (0, 1):
            name = f'{own}-p{seat}'
            matches = []
            for other in names:
                order = [own, other] if seat == 0 else [other, own]
                matches.append({'config': {'deck_ids': order,
                    'seed': seed('postboard-bo3-qualification-20260920-v1', own, other),
                    'game_one_chooser': seat, 'max_physical_games': 6,
                    'max_physical_decisions': 4000, 'max_policy_steps': 40000,
                    'opening_protocol': 'keep_seven_v2'}, 'registered': [decks[x] for x in order]})
            config = {'mode': 'run_population_batch', 'model_sources': [source, opponent] if seat == 0 else [opponent, source],
                      'cross_generation_evaluation': True, 'policies': [policy, policy], 'matches': matches,
                      'output_directory': (root / 'outputs' / name).as_posix()}
            write(root / 'configs' / (name + '.json'), config)
            jobs.append({'name': name, 'candidate_seat': seat, 'learner_family': own,
                         'config': pin(root / 'configs' / (name + '.json')), 'matches': len(matches)})
    replay = copy.deepcopy(read(root / 'configs' / 'Affinity-p0.json'))
    replay['matches'] = replay['matches'][:1]
    replay['output_directory'] = (root / 'outputs' / 'replay').as_posix()
    write(root / 'configs/replay.json', replay)
    write(root / 'manifest.json', {'schema': 'postboard-bo3-qualification/v1', 'binary': pin(root / BINARY.name),
        'source_commit': '412a3a63', 'jobs': jobs, 'replay': pin(root / 'configs/replay.json'),
        'gates': {'matched_cases': 49, 'matches': 98, 'max_job_seconds': 90,
                  'max_worker_seconds': 300, 'workers': 8, 'exact_match_replay_required': True},
        'frozen_source': source, 'opponent_source': opponent, 'draft': pin(draft_path),
        'claim': 'Engineering legality, complete natural BO3 and timing only. Do not use outcomes for experiment selection.',
        'review_gap': 'Fable HTTP429 until September 22 07:00 EDT; zero source reads, no endorsement. Jack assigned continuation.'})
    return len(jobs)


def execute(root, job, manifest):
    config = read(job['config']['path'])
    assert pin(job['config']['path'])['sha256'] == job['config']['sha256']
    command = [manifest['binary']['path'], '--config', job['config']['path']]
    prefix = root / 'receipts' / job['name']
    started = time.monotonic()
    with prefix.with_suffix('.stdout.log').open('xb') as stdout, prefix.with_suffix('.stderr.log').open('xb') as stderr:
        child = subprocess.Popen(command, stdout=stdout, stderr=stderr,
            env=dict(os.environ, TEMP='E:/tmp', TMP='E:/tmp'),
            creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        write(prefix.with_suffix('.start.json'), {'pid': child.pid, 'command': command, 'started_epoch': time.time()})
        timeout = False
        try:
            code = child.wait(timeout=manifest['gates']['max_job_seconds'])
        except subprocess.TimeoutExpired:
            child.kill(); code = child.wait(); timeout = True
    result = {'name': job['name'], 'exit_code': code, 'timed_out': timeout,
              'elapsed_seconds': time.monotonic() - started,
              'config': job['config'], 'output_directory': config['output_directory']}
    write(prefix.with_suffix('.completion.json'), result)
    return result


def prepare_adapter(root, prior, binary, opponent_import):
    """Keep every original case/model/table pin while identifying the new adapter."""
    assert not root.exists()
    original = read(prior / 'manifest.json')
    old_source = read(original['opponent_source']['play_import']['path'])
    new_source = read(opponent_import)
    old_envelope = read(old_source['transfer_envelope']['path'])
    new_envelope = read(new_source['transfer_envelope']['path'])
    old_build = old_envelope['receipt']['destination_build_git_head']
    old_envelope['receipt']['destination_build_git_head'] = new_envelope['receipt']['destination_build_git_head']
    assert old_envelope == new_envelope, 'V3 import must differ only in destination build provenance'
    for key in ('source_checkpoint', 'source_registry'):
        assert old_source[key]['sha256'] == new_source[key]['sha256']
    root.mkdir(parents=True)
    for directory in ('configs', 'outputs', 'receipts'):
        (root / directory).mkdir()
    shutil.copyfile(binary, root / 'learned_sideboard_v1.exe')
    shutil.copyfile(prior / 'static-rows.json', root / 'static-rows.json')
    manifest = copy.deepcopy(original)
    manifest['binary'] = pin(root / 'learned_sideboard_v1.exe')
    manifest['source_commit'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'],
        cwd='E:/mtg-kernel-postboard-eval-codex', text=True).strip()
    manifest['previous_qualification'] = pin(prior / 'manifest.json')
    manifest['v3_forced_action_passthrough'] = True
    manifest['opponent_source']['play_import'] = pin(opponent_import)
    manifest['gates']['all_previously_completed_matches_equal_after_exact_import_provenance_rebinding'] = True
    manifest['import_provenance_rebinding'] = {
        'old_envelope': old_source['transfer_envelope'], 'new_envelope': new_source['transfer_envelope'],
        'only_changed_envelope_field': 'receipt.destination_build_git_head',
        'old_build': old_build, 'new_build': new_envelope['receipt']['destination_build_git_head'],
        'parameters_and_adam_bit_identical': True,
        'only_rebound_match_field': 'play_models[opponent_seat].source_import.appended_rows'}
    for job in manifest['jobs']:
        assert pin(job['config']['path'])['sha256'] == job['config']['sha256']
        config = read(job['config']['path'])
        config['v3_forced_action_passthrough'] = True
        config['model_sources'][1-job['candidate_seat']] = manifest['opponent_source']
        config['output_directory'] = (root / 'outputs' / job['name']).as_posix()
        write(root / 'configs' / (job['name'] + '.json'), config)
        job['config'] = pin(root / 'configs' / (job['name'] + '.json'))
    config = read(original['replay']['path'])
    config['v3_forced_action_passthrough'] = True
    config['model_sources'][1] = manifest['opponent_source']
    config['output_directory'] = (root / 'outputs/replay').as_posix()
    write(root / 'configs/replay.json', config)
    manifest['replay'] = pin(root / 'configs/replay.json')
    write(root / 'manifest.json', manifest)
    return len(manifest['jobs'])


def audit_match(path, config, plans, allow_draws=False):
    match = read(path)
    assert match['config'] == config['config']
    assert match.get('search_usage') is None, 'Privileged search must be disabled'
    assert len(match['explicit_registrations']) == 2
    for actual, expected in zip(match['explicit_registrations'], config['registered']):
        assert {k: actual[k] for k in ('label', 'mainboard', 'sideboard')} == expected
    assert match['games'][0]['mainboard_sha256'] == [r['mainboard_sha256'] for r in match['explicit_registrations']]
    assert 'winner' in match['outcome'] and len(match['games']) >= 2
    # Native finish_natural_v1 rejects any capped/halted game before publication.
    assert allow_draws or all(g['winner'] is not None for g in match['games'])
    sideboard_games = set()
    moves = 0
    for row in match['sideboard_decisions']:
        seat = row['acting_player']
        game = row['input']['next_game_number']
        own, other = match['config']['deck_ids'][seat], match['config']['deck_ids'][1-seat]
        target = plans[(own, other, min(game, 3))]
        expected_main = Counter(config['registered'][seat]['mainboard'])
        expected_side = Counter(config['registered'][seat]['sideboard'])
        for item in target['cards_out']:
            expected_main[item['card_id']] -= item['count']; expected_side[item['card_id']] += item['count']
        for item in target['cards_in']:
            expected_main[item['card_id']] += item['count']; expected_side[item['card_id']] -= item['count']
        main, side = Counter(row['initial_mainboard']), Counter(row['initial_sideboard'])
        assert row['selected_actions'][-1] == {'kind': 'done'}
        for action in row['selected_actions'][:-1]:
            card = action['card_id']
            if action['kind'] == 'move_one_to_sideboard':
                assert main[card] > 0; main[card] -= 1; side[card] += 1
            else:
                assert action['kind'] == 'move_one_to_mainboard' and side[card] > 0
                side[card] -= 1; main[card] += 1
            moves += 1
        assert +main == +expected_main and +side == +expected_side
        assert row['selected_mainboard_sha256'] == match['games'][game-1]['mainboard_sha256'][seat]
        sideboard_games.add((game, seat))
    assert sideboard_games == {(g['start']['game_index'], s) for g in match['games'][1:] for s in (0, 1)}
    assert moves > 0
    return len(match['games']), moves


def run(root):
    manifest = read(root / 'manifest.json')
    assert pin(manifest['binary']['path'])['sha256'] == manifest['binary']['sha256']
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=manifest['gates']['workers']) as pool:
        results = list(pool.map(lambda job: execute(root, job, manifest), manifest['jobs']))
    write(root / 'run-summary.json', {'jobs': results, 'wall_seconds': time.monotonic()-started})
    assert all(x['exit_code'] == 0 and not x['timed_out'] for x in results), 'Incomplete qualification; preserve failures'
    plans = {(x['self_deck_id'], x['opponent_deck_id'], x['game_index']): x for x in read(root / 'static-rows.json')['rows']}
    total_games, total_moves, hashes = 0, 0, []
    for job in manifest['jobs']:
        config = read(job['config']['path'])
        output = Path(config['output_directory'])
        assert read(output / 'completion.json')['completed_matches'] == job['matches']
        files = sorted(output.glob('match-*.json'))
        assert len(files) == job['matches']
        for path, item in zip(files, config['matches']):
            games, moves = audit_match(path, item, plans)
            total_games += games; total_moves += moves; hashes.append(pin(path))
    replay = execute(root, {'name': 'replay', 'config': manifest['replay']}, manifest)
    assert replay['exit_code'] == 0 and not replay['timed_out']
    original = root / 'outputs/Affinity-p0/match-000000.json'
    repeated = root / 'outputs/replay/match-000000.json'
    assert original.read_bytes() == repeated.read_bytes(), 'Actual match store replay differs'
    worker_seconds = sum(x['elapsed_seconds'] for x in results) + replay['elapsed_seconds']
    assert worker_seconds <= manifest['gates']['max_worker_seconds']
    previous_matches, previous_raw_identical = 0, 0
    if manifest.get('previous_qualification'):
        prior = Path(manifest['previous_qualification']['path']).parent
        assert pin(prior / 'manifest.json')['sha256'] == manifest['previous_qualification']['sha256']
        seats = {job['name']: 1-job['candidate_seat'] for job in manifest['jobs']}
        for old in sorted((prior / 'outputs').glob('*/match-*.json')):
            relative = old.relative_to(prior / 'outputs')
            new = root / 'outputs' / relative
            previous_raw_identical += old.read_bytes() == new.read_bytes()
            before, after = read(old), read(new)
            seat = seats[relative.parts[0]]
            old_label = before['play_models'][seat]['source_import']['appended_rows']
            new_label = after['play_models'][seat]['source_import']['appended_rows']
            binding = manifest['import_provenance_rebinding']
            assert old_label.replace(binding['old_envelope']['sha256'], binding['new_envelope']['sha256']) == new_label
            after['play_models'][seat]['source_import']['appended_rows'] = old_label
            assert before == after, 'Previously completed gameplay changed: ' + str(relative)
            previous_matches += 1
        assert previous_matches == 94
    write(root / 'qualification-result.json', {'verdict': 'ENGINEERING-PASS', 'matches': len(hashes),
        'physical_games': total_games, 'applied_sideboard_moves': total_moves,
        'match_pins': hashes, 'replay_sha256': pin(repeated)['sha256'],
        'worker_seconds_including_replay': worker_seconds, 'wall_seconds': time.monotonic()-started,
        'previous_matches_byte_identical': previous_raw_identical,
        'previous_matches_equal_after_import_provenance_rebinding': previous_matches,
        'outcomes_interpreted': False, 'strength_claim': False})
    print(json.dumps({'verdict': 'ENGINEERING-PASS', 'matches': len(hashes), 'games': total_games,
        'moves': total_moves, 'worker_seconds': worker_seconds}), flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('prepare', 'prepare-adapter', 'run'))
    parser.add_argument('--root', required=True, type=Path)
    parser.add_argument('--prior', type=Path)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--opponent-import', type=Path)
    args = parser.parse_args()
    if args.mode == 'prepare':
        print(json.dumps({'jobs_prepared': prepare(args.root.resolve())}))
    elif args.mode == 'prepare-adapter':
        assert args.prior and args.binary and args.opponent_import
        print(json.dumps({'jobs_prepared': prepare_adapter(args.root.resolve(), args.prior.resolve(), args.binary.resolve(), args.opponent_import.resolve())}))
    else:
        run(args.root.resolve())
