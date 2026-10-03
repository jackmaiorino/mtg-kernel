"""Run the frozen pilot in explicit stages; never interpret incomplete outcomes."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import copy
import json
import os
from pathlib import Path
import subprocess
import time

from prepare_postboard_qualification_v1 import pin, read, write
from postboard_bo3_qualification_v1 import execute, audit_match


def verify_pin(value):
    assert pin(value['path'])['sha256'] == value['sha256'], value['path']
    return Path(value['path'])


def endpoint(root, manifest, arm):
    if arm == 'g115':
        return manifest['initial_source']
    completed = read(root / arm / 'training-audit.json')
    assert completed['complete'] and completed['updates'] == 200
    source = completed['source']
    checkpoint = read(verify_pin(source['checkpoint']))
    assert checkpoint['adam_step'] == manifest['final_adam_step']
    assert source['checkpoint']['path'].replace('\\', '/').endswith('/iterations/000199/attempt-000000/update/checkpoint.json')
    assert source['play_import'] == manifest['initial_source']['play_import']
    assert source['feature_transfer'] == manifest['initial_source']['feature_transfer']
    return source


def evaluate(root, arms):
    manifest = read(root / 'manifest.json')
    assert verify_pin(manifest['runner_script']) == Path(__file__).resolve()
    verify_pin(manifest['binary'])
    sources = {arm: endpoint(root, manifest, arm) for arm in arms}
    jobs = []
    for planned in manifest['evaluation_jobs']:
        if planned['arm'] not in arms:
            continue
        template = read(verify_pin(planned['template']))
        template['model_sources'][planned['candidate_seat']] = sources[planned['arm']]
        config_path = root / 'configs' / (planned['name'] + '.json')
        write(config_path, template)
        jobs.append(dict(planned, config=pin(config_path)))
    label = 'reference' if arms == ['g115'] else 'trained'
    write(root / (label + '-dispatch.json'), {'jobs': jobs})
    start = time.monotonic()
    with ThreadPoolExecutor(max_workers=manifest['gates']['workers']) as pool:
        results = list(pool.map(lambda job: execute(root, job, manifest), jobs))
    write(root / (label + '-execution.json'), {'results': results, 'wall_seconds': time.monotonic()-start})
    assert all(x['exit_code'] == 0 and not x['timed_out'] for x in results), 'Incomplete evaluation; do not interpret prefixes or launch dependent work'
    for arm in arms:
        audit_evaluation_arm(root, manifest, arm, jobs)
    worker_seconds = sum(x['elapsed_seconds'] for x in results)
    if label == 'reference':
        assert worker_seconds * 3 <= manifest['gates']['max_evaluation_worker_seconds'], 'Reference cost qualification failed; do not train'
        write(root / 'reference-cost-qualification.json', {'passed': True, 'worker_seconds': worker_seconds,
            'projected_three_arm_worker_seconds': worker_seconds * 3})
    else:
        worker_seconds += sum(x['elapsed_seconds'] for x in read(root / 'reference-execution.json')['results'])
        assert worker_seconds <= manifest['gates']['max_evaluation_worker_seconds'], 'Complete but outside frozen cost envelope'


def audit_evaluation_arm(root, manifest, arm, dispatched):
    source = endpoint(root, manifest, arm)
    snapshot = read(verify_pin(source['checkpoint']))
    pins, physical_games, passthroughs = [], 0, 0
    for job in [j for j in dispatched if j['arm'] == arm]:
        config = read(verify_pin(job['config']))
        directory = Path(config['output_directory'])
        completion = read(directory / 'completion.json')
        assert completion['completed_matches'] == job['matches']
        assert completion['v3_forced_action_passthrough']['per_seat_actions'][job['candidate_seat']] is None
        count = completion['v3_forced_action_passthrough']['per_seat_actions'][1-job['candidate_seat']]
        assert type(count) is int
        passthroughs += count
        models = read(directory / 'play-models.json')
        actual = models[job['candidate_seat']]
        assert actual['checkpoint_sha256'] == source['checkpoint']['sha256']
        assert actual['state_sha256'] == snapshot['state_sha256'] and actual['adam_step'] == snapshot['adam_step']
        assert read(directory / 'run-start.json')['binary']['sha256'] == manifest['binary']['sha256']
        plans = {(r['self_deck_id'], r['opponent_deck_id'], r['game_index']): r for r in read(verify_pin(config['policies'][0]['table']))['rows']}
        files = sorted(directory.glob('match-*.json'))
        assert len(files) == job['matches']
        for file, match_config in zip(files, config['matches']):
            games, _ = audit_match(file, match_config, plans, allow_draws=True)
            assert read(file)['play_models'] == models
            physical_games += games
            pins.append(pin(file))
    assert len(pins) == manifest['gates']['expected_matches_per_arm']
    write(root / (arm + '-evaluation-audit.json'), {'complete': True, 'matches': len(pins),
        'physical_games': physical_games, 'candidate_source': source, 'match_pins': pins,
        'v3_forced_actions': passthroughs, 'strength_outcomes_interpreted': False})
    print(json.dumps({'arm': arm, 'completed_matches': len(pins), 'physical_games': physical_games,
                      'v3_forced_actions': passthroughs, 'outcomes_interpreted': False}), flush=True)


def train(root):
    manifest = read(root / 'manifest.json')
    assert verify_pin(manifest['runner_script']) == Path(__file__).resolve()
    assert read(root / 'g115-evaluation-audit.json')['complete']
    assert read(root / 'reference-cost-qualification.json')['passed']
    verify_pin(manifest['training_binary'])
    for arm in ('preboard', 'mixed'):
        config = verify_pin(manifest['training_configs'][arm])
        command = [manifest['training_binary']['path'], str(config)]
        started = time.monotonic()
        with (root / arm / 'stdout.jsonl').open('xb') as out, (root / arm / 'stderr.log').open('xb') as err:
            child = subprocess.Popen(command, stdout=out, stderr=err,
                env=dict(os.environ, TEMP='E:/tmp', TMP='E:/tmp'),
                creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
            write(root / arm / 'start.json', {'pid': child.pid, 'started_epoch': time.time(),
                'command': command, 'gpu_ordinal': 1, 'priority': 'BelowNormal',
                'binary': manifest['training_binary'], 'config': manifest['training_configs'][arm]})
            timed_out = False
            try:
                code = child.wait(timeout=manifest['gates']['max_training_arm_seconds'])
            except subprocess.TimeoutExpired:
                child.kill(); code = child.wait(); timed_out = True
        write(root / arm / 'execution.json', {'exit_code': code, 'timed_out': timed_out,
            'elapsed_seconds': time.monotonic()-started, 'stdout': pin(root / arm / 'stdout.jsonl'),
            'stderr': pin(root / arm / 'stderr.log')})
        assert code == 0 and not timed_out, 'Training incomplete; preserve run, no prefix interpretation'
        audit_training(root, manifest, arm)


def audit_training(root, manifest, arm):
    config = read(verify_pin(manifest['training_configs'][arm]))
    run = root / arm / 'run'
    final = read(run / 'completion.json')
    assert final['complete'] and final['completed_iterations'] == 200
    assert final['loss_identity'] == 'gae_advantage_value/v1' and final['gpu_ordinal'] == 1
    previous_source = config['initial_source']
    previous_state = read(verify_pin(previous_source['checkpoint']))['state_sha256']
    natural, decisions, postboard = 0, 0, 0
    for index, batch in enumerate(config['iterations']):
        receipt = read(run / 'iterations' / f'{index:06}' / 'complete.json')
        assert receipt['source'] == previous_source
        update = read(verify_pin(receipt['update']))
        collection = read(verify_pin(receipt['collection']))
        assert update['source'] == previous_source and collection['source'] == previous_source
        assert update['before_state_sha256'] == collection['behavior_state_sha256'] == previous_state
        assert update['after_state_sha256'] != previous_state
        assert update['adam_step'] == 32401 + index and update['checkpoint_readback']
        assert update['loss_identity'] == 'gae_advantage_value/v1' and update['gpu_ordinal'] == 1
        assert len(collection['trajectories']) == 10
        for slot, trajectory_pin in enumerate(collection['trajectories']):
            trajectory = read(verify_pin(trajectory_pin))
            expected = copy.deepcopy(batch['episodes'][slot]['episode'])
            assignment = batch['episodes'][slot]['opponent']
            if assignment['kind'] == 'current':
                opponent = previous_source
            elif assignment['kind'] == 'initial':
                opponent = config['initial_source']
            else:
                assert assignment['kind'] == 'fixed'
                opponent = next(x['source'] for x in config['opponents'] if x['id'] == assignment['id'])
            expected['opponent'] = opponent
            assert trajectory['episode'] == expected and trajectory['behavior_state_sha256'] == previous_state
            terminal = trajectory['terminal']
            assert terminal['terminal_classification'] == 'natural'
            assert terminal['terminal_reward'] in ([1, -1], [-1, 1], [0, 0])
            natural += 1; decisions += terminal['physical_decision_count']; postboard += expected['postboard']
        previous_source = dict(config['initial_source'], checkpoint=update['checkpoint'])
        previous_state = update['after_state_sha256']
    assert natural == 2000 and postboard == (1000 if arm == 'mixed' else 0)
    assert final['source'] == previous_source
    checkpoint = read(verify_pin(previous_source['checkpoint']))
    assert checkpoint['adam_step'] == 32600 and checkpoint['state_sha256'] == previous_state
    assert {k: checkpoint[k] for k in ('gamma_bits', 'gae_lambda_bits', 'entropy_coefficient_bits', 'learning_rate_bits', 'value_coefficient_bits')} == {
        'gamma_bits': 1065353216, 'gae_lambda_bits': 1063675494, 'entropy_coefficient_bits': 0,
        'learning_rate_bits': 953267991, 'value_coefficient_bits': 1056964608}
    write(root / arm / 'training-audit.json', {'complete': True, 'updates': 200, 'natural_games': natural,
        'physical_decisions': decisions, 'postboard_games': postboard, 'source': previous_source,
        'final_adam_step': checkpoint['adam_step'], 'full_model_state_sha256': previous_state})
    print(json.dumps({'arm': arm, 'updates': 200, 'natural_games': natural, 'postboard_games': postboard,
                      'adam_step': checkpoint['adam_step']}), flush=True)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('phase', choices=('reference', 'train', 'evaluate'))
    p.add_argument('--root', type=Path, required=True)
    args = p.parse_args()
    root = args.root.resolve()
    if args.phase == 'train':
        train(root)
    else:
        evaluate(root, ['g115'] if args.phase == 'reference' else ['preboard', 'mixed'])
