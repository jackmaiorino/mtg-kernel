"""Qualify an explicit V3 adapter, then evaluate preserved pilot endpoints afresh."""
import argparse
import copy
from pathlib import Path
import shutil
import subprocess
from prepare_postboard_qualification_v1 import read, write, pin
from postboard_bo3_qualification_v1 import execute, audit_match
from run_postboard_pilot_v1 import evaluate, verify_pin


def import_binding(old_source, descriptor):
    old = read(verify_pin(old_source['play_import']))
    new = read(descriptor)
    before = read(verify_pin(old['transfer_envelope']))
    after = read(verify_pin(new['transfer_envelope']))
    original_build = before['receipt']['destination_build_git_head']
    before['receipt']['destination_build_git_head'] = after['receipt']['destination_build_git_head']
    assert before == after, 'Import changed beyond producer build provenance'
    for field in ('source_checkpoint', 'source_registry'):
        assert old[field]['sha256'] == new[field]['sha256']
        verify_pin(old[field]); verify_pin(new[field])
    source = dict(old_source, play_import=pin(descriptor))
    return source, {'old_envelope': old['transfer_envelope'], 'new_envelope': new['transfer_envelope'],
        'old_build': original_build, 'new_build': after['receipt']['destination_build_git_head'],
        'only_changed_envelope_field': 'receipt.destination_build_git_head',
        'full_parameters_and_adam_equal': True}


def compare_store(before_path, after_path, opponent_seat, binding):
    before, after = read(before_path), read(after_path)
    old_label = before['play_models'][opponent_seat]['source_import']['appended_rows']
    new_label = after['play_models'][opponent_seat]['source_import']['appended_rows']
    assert old_label.replace(binding['old_envelope']['sha256'], binding['new_envelope']['sha256']) == new_label
    after['play_models'][opponent_seat]['source_import']['appended_rows'] = old_label
    assert before == after, 'Previously completed gameplay differs: ' + str(before_path)


def check_adapter_receipts(directory, seat, expected_matches):
    completion = read(directory / 'completion.json')['v3_spell_target_reference_adapter']
    assert completion['schema'] == 'v3-public-unique-spell-target-reference/v1'
    assert completion['simulation_steps'] == 0 and completion['policy_domain_extended']
    counts = [None, None]; counts[seat] = 0
    files = sorted((directory / 'adapter-receipts').glob('match-*.json'))
    assert len(files) == expected_matches
    for index, file in enumerate(files):
        receipt = read(file)
        assert receipt['schema'] == completion['schema'] and receipt['simulation_steps'] == 0
        match = directory / f'match-{index:06}.json'
        assert receipt['match']['sha256'] == pin(match)['sha256']
        assert Path(receipt['match']['path']).resolve() == match.resolve()
        assert receipt['repairs_before'] == counts
        after = receipt['repairs_after']
        assert after[1-seat] is None and type(after[seat]) is int and after[seat] >= counts[seat]
        counts = after
    assert counts == completion['per_seat_decisions']
    return counts[seat]


def qualify(root, pilot, binary, descriptor):
    root.mkdir()
    for name in ('configs', 'outputs', 'receipts'):
        (root / name).mkdir()
    old = read(pilot / 'manifest.json')
    assert read(pilot / 'incomplete-evaluation.json')['unresolved_cases']
    jobs = read(pilot / 'reference-dispatch.json')['jobs'] + read(pilot / 'trained-dispatch.json')['jobs']
    failed = next(j for j in jobs if j['name'] == 'preboard-Faeries-p1')
    control = next(j for j in jobs if j['name'] == 'g115-Affinity-p0')
    failed_config = read(verify_pin(failed['config']))
    source, binding = import_binding(failed_config['model_sources'][0], descriptor)
    shutil.copyfile(binary, root / 'learned_sideboard_v1.exe')
    configs = []
    for name, old_job, index in [('failed-case', failed, 49), ('valid-control', control, 0), ('replay', failed, 49)]:
        config = read(verify_pin(old_job['config']))
        config['matches'] = [config['matches'][index]]
        config['model_sources'][1-old_job['candidate_seat']] = source
        config['v3_spell_target_reference_adapter'] = True
        config['output_directory'] = (root / 'outputs' / name).as_posix()
        path = root / 'configs' / (name + '.json')
        write(path, config)
        configs.append({'name': name, 'config': pin(path), 'candidate_seat': old_job['candidate_seat']})
    manifest = {'binary': pin(root / 'learned_sideboard_v1.exe'), 'pilot': pin(pilot / 'manifest.json'),
        'opponent_source': source, 'import_binding': binding, 'jobs': configs,
        'script': pin(__file__), 'gates': {'max_job_seconds': 30, 'max_worker_seconds': 90}}
    write(root / 'manifest.json', manifest)
    results = [execute(root, j, manifest) for j in configs]
    write(root / 'execution.json', results)
    assert all(r['exit_code'] == 0 and not r['timed_out'] for r in results)
    counts = {}
    for job in configs:
        config = read(job['config']['path'])
        plans = {(p['self_deck_id'], p['opponent_deck_id'], p['game_index']): p for p in read(verify_pin(config['policies'][0]['table']))['rows']}
        directory = Path(config['output_directory'])
        assert read(directory / 'completion.json')['completed_matches'] == 1
        start = read(directory / 'run-start.json')
        assert start['binary']['sha256'] == manifest['binary']['sha256']
        assert start['build']['git_commit'] == binding['new_build']
        audit_match(directory / 'match-000000.json', config['matches'][0], plans, allow_draws=True)
        counts[job['name']] = check_adapter_receipts(directory, 1-job['candidate_seat'], 1)
    first = root / 'outputs/failed-case/match-000000.json'
    replay = root / 'outputs/replay/match-000000.json'
    assert first.read_bytes() == replay.read_bytes()
    assert counts['failed-case'] >= 1 and counts['replay'] == counts['failed-case'] and counts['valid-control'] == 0
    compare_store(pilot / 'outputs/g115-Affinity-p0/match-000000.json',
        root / 'outputs/valid-control/match-000000.json', 1, binding)
    seconds = sum(r['elapsed_seconds'] for r in results)
    assert seconds <= manifest['gates']['max_worker_seconds']
    write(root / 'qualification-result.json', {'verdict': 'ENGINEERING-PASS', 'matches': 3,
        'repair_counts': counts, 'worker_seconds': seconds, 'replay_sha256': pin(replay)['sha256'],
        'control_equal_after_exact_import_metadata_rebinding': True, 'outcomes_interpreted': False})
    print({'qualification': 'ENGINEERING-PASS', 'repair_counts': counts, 'worker_seconds': seconds}, flush=True)


def prepare_panel(root, pilot, qualification):
    assert read(qualification / 'qualification-result.json')['verdict'] == 'ENGINEERING-PASS'
    q = read(qualification / 'manifest.json')
    assert pin(pilot / 'manifest.json') == q['pilot']
    root.mkdir()
    for name in ('configs', 'templates', 'receipts', 'outputs', 'preboard', 'mixed'):
        (root / name).mkdir()
    manifest = copy.deepcopy(read(pilot / 'manifest.json'))
    manifest['schema'] = 'g115-postboard-repaired-evaluation/v1'
    manifest['evaluation_build_commit'] = q['import_binding']['new_build']
    verify_pin(q['binary'])
    shutil.copyfile(q['binary']['path'], root / 'learned_sideboard_v1.exe')
    manifest['binary'] = pin(root / 'learned_sideboard_v1.exe')
    manifest['previous_incomplete_panel'] = pin(pilot / 'manifest.json')
    manifest['adapter_qualification'] = pin(qualification / 'qualification-result.json')
    manifest['import_binding'] = q['import_binding']
    manifest['repair_script'] = pin(__file__)
    manifest['reused_training'] = {}
    manifest['training_performed_in_this_root'] = False
    for arm in ('preboard', 'mixed'):
        audit = read(pilot / arm / 'training-audit.json')
        assert audit['complete'] and audit['updates'] == 200 and audit['natural_games'] == 2000
        checkpoint = read(verify_pin(audit['source']['checkpoint']))
        assert checkpoint['state_sha256'] == audit['full_model_state_sha256'] and checkpoint['adam_step'] == 32600
        manifest['reused_training'][arm] = pin(pilot / arm / 'training-audit.json')
        for filename in ('training-audit.json', 'execution.json'):
            shutil.copyfile(pilot / arm / filename, root / arm / filename)
    for job in manifest['evaluation_jobs']:
        template = read(verify_pin(job['template']))
        template['model_sources'][1-job['candidate_seat']] = q['opponent_source']
        template['v3_spell_target_reference_adapter'] = True
        template['output_directory'] = (root / 'outputs' / job['name']).as_posix()
        file = root / 'templates' / (job['name'] + '.json')
        write(file, template); job['template'] = pin(file)
    manifest['limitations'].append('Fresh complete evaluation under an explicit public spell-target reference adapter; the old 2345/2352 panel remains incomplete and is not pooled.')
    manifest['limitations'].append('Training timing in this root refers to the two reused, already completed pilot endpoints; no new training was performed here.')
    write(root / 'manifest.json', manifest)
    print({'prepared_matches': 2352, 'training_reused': True}, flush=True)


def run_panel(root):
    manifest = read(root / 'manifest.json')
    assert pin(__file__) == manifest['repair_script']
    evaluate(root, ['g115'])
    evaluate(root, ['preboard', 'mixed'])
    pilot = Path(manifest['previous_incomplete_panel']['path']).parent
    verify_pin(manifest['previous_incomplete_panel'])
    previous, repairs = 0, {}
    for job in manifest['evaluation_jobs']:
        directory = root / 'outputs' / job['name']
        repairs[job['name']] = check_adapter_receipts(directory, 1-job['candidate_seat'], job['matches'])
        for old in sorted((pilot / 'outputs' / job['name']).glob('match-*.json')):
            compare_store(old, directory / old.name, 1-job['candidate_seat'], manifest['import_binding'])
            receipt = read(directory / 'adapter-receipts' / old.name)
            assert receipt['repairs_before'] == receipt['repairs_after'], 'A previously valid match required new repair'
            previous += 1
    assert previous == 2345
    write(root / 'valid-domain-comparison.json', {'previous_matches_equal_after_import_metadata_rebinding': previous,
        'all_new_matches': 2352, 'repair_decisions_by_job': repairs, 'old_panel_pooled': False,
        'new_strength_analysis_not_yet_run': True})
    print({'complete_matches': 2352, 'previous_gameplay_equal': previous, 'repair_decisions': sum(repairs.values())}, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('qualify', 'prepare-panel', 'run-panel'))
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--pilot', type=Path, default=Path('E:/mtg-postboard-campaign-20260920/pilot-001'))
    parser.add_argument('--qualification', type=Path)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--opponent-import', type=Path)
    args = parser.parse_args()
    if args.mode == 'qualify':
        assert args.binary and args.opponent_import
        qualify(args.root.resolve(), args.pilot.resolve(), args.binary.resolve(), args.opponent_import.resolve())
    elif args.mode == 'prepare-panel':
        assert args.qualification
        prepare_panel(args.root.resolve(), args.pilot.resolve(), args.qualification.resolve())
    else:
        run_panel(args.root.resolve())
