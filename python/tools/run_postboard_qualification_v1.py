"""Execute and audit the fixed four-update engineering qualification."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from prepare_postboard_qualification_v1 import pin, read, write


def checked_pin(value):
    assert pin(value['path'])['sha256'] == value['sha256'], value['path']
    return Path(value['path'])


def execute(root, manifest, arm, tag, limit=None):
    config = checked_pin(manifest['configs'][arm])
    binary = checked_pin(manifest['binary'])
    command = [str(binary), str(config)]
    if limit is not None:
        command += ['--max-new-iterations', str(limit)]
    output = root / arm / (tag + '.stdout.jsonl')
    error = root / arm / (tag + '.stderr.log')
    started = time.time()
    env = dict(os.environ, TEMP='E:/tmp', TMP='E:/tmp')
    with output.open('xb') as out, error.open('xb') as err:
        process = subprocess.Popen(command, stdout=out, stderr=err, env=env,
            creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        write(root / arm / (tag + '.start.json'), {'pid': process.pid, 'command': command,
            'started_epoch': started, 'priority': 'BelowNormal', 'gpu_ordinal': 1,
            'config': manifest['configs'][arm], 'binary': manifest['binary']})
        timed_out = False
        try:
            code = process.wait(timeout=manifest['gates']['max_arm_wall_seconds'])
        except subprocess.TimeoutExpired:
            # Popen retains the exact launched process handle; no PID-name kill.
            process.kill()
            code = process.wait()
            timed_out = True
    elapsed = time.time() - started
    receipt = {'exit_code': code, 'timed_out': timed_out, 'elapsed_seconds': elapsed,
               'stdout': pin(output), 'stderr': pin(error)}
    write(root / arm / (tag + '.execution.json'), receipt)
    assert code == 0 and not timed_out, {'arm': arm, 'tag': tag, **receipt}
    documents = [json.loads(x) for x in output.read_text().splitlines() if x.startswith('{')]
    last = documents[-1]
    assert last['completed_iterations'] == (1 if limit == 1 else 4)
    assert last['complete'] == (limit != 1)
    print(json.dumps({'arm': arm, 'stage': tag, 'elapsed_seconds': elapsed,
                      'completed_iterations': last['completed_iterations']}), flush=True)
    return receipt


def audit_arm(root, manifest, arm):
    config = read(checked_pin(manifest['configs'][arm]))
    run = root / arm / 'run'
    completed = read(run / 'completion.json')
    assert completed['complete'] and completed['completed_iterations'] == 4
    assert completed['loss_identity'] == 'gae_advantage_value/v1'
    assert completed['gpu_ordinal'] == 1
    previous_state = manifest['initial_state_sha256']
    previous_source = config['initial_source']
    iterations, trajectory_hashes = [], []
    for index, batch in enumerate(config['iterations']):
        directory = run / 'iterations' / f'{index:06}'
        receipt = read(directory / 'complete.json')
        update = read(checked_pin(receipt['update']))
        collection = read(checked_pin(receipt['collection']))
        assert receipt['source'] == previous_source
        assert update['source'] == previous_source
        assert update['before_state_sha256'] == previous_state
        assert collection['behavior_state_sha256'] == previous_state
        assert update['adam_step'] == 32401 + index
        assert update['loss_identity'] == 'gae_advantage_value/v1'
        assert update['gpu_ordinal'] == 1 and update['checkpoint_readback']
        checkpoint = read(checked_pin(update['checkpoint']))
        assert checkpoint['state_sha256'] == update['after_state_sha256']
        assert checkpoint['adam_step'] == 32401 + index
        assert checkpoint['gamma_bits'] == 1065353216
        assert checkpoint['gae_lambda_bits'] == 1063675494
        assert checkpoint['entropy_coefficient_bits'] == 0
        assert all(checkpoint[k] for k in ('parameters', 'first_moments', 'second_moments'))
        assert checkpoint['state_sha256'] != previous_state
        trajectories = collection['trajectories']
        assert len(trajectories) == 10
        decisions = 0
        for slot, value in enumerate(trajectories):
            path = checked_pin(value)
            trajectory = read(path)
            expected_episode = copy.deepcopy(batch['episodes'][slot]['episode'])
            assignment = batch['episodes'][slot]['opponent']
            if assignment['kind'] == 'current':
                opponent = previous_source
            elif assignment['kind'] == 'initial':
                opponent = config['initial_source']
            else:
                assert assignment['kind'] == 'fixed'
                opponent = next(x['source'] for x in config['opponents'] if x['id'] == assignment['id'])
            expected_episode['opponent'] = opponent
            assert trajectory['episode'] == expected_episode
            assert trajectory['behavior_state_sha256'] == previous_state
            terminal = trajectory['terminal']
            assert terminal['terminal_classification'] == 'natural'
            assert terminal['terminal_reward'] in ([1, -1], [-1, 1], [0, 0])
            assert all(step is not None for step in trajectory['decisions'])
            decisions += terminal['physical_decision_count']
            trajectory_hashes.append(value['sha256'])
        previous_state = checkpoint['state_sha256']
        previous_source = dict(config['initial_source'], checkpoint=update['checkpoint'])
        iterations.append({'iteration': index, 'adam_step': checkpoint['adam_step'],
            'state_sha256': checkpoint['state_sha256'], 'checkpoint': update['checkpoint'],
            'physical_decisions': decisions,
            'optimizer_payload_sha256': hashlib.sha256(json.dumps({k: checkpoint[k] for k in
                ('parameters', 'first_moments', 'second_moments', 'adam_step', 'scorer_bias_anchor_bits')},
                sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
            'timing': {k: update[k] for k in ('learner_update_seconds', 'checkpoint_io_seconds',
                'behavior_replay_seconds', 'update_elapsed_seconds') if k in update}})
    return {'complete': True, 'iterations': iterations, 'trajectory_hashes': trajectory_hashes,
            'natural_episodes': 40, 'physical_decisions': sum(x['physical_decisions'] for x in iterations)}


def compare_replay(root, arms):
    pairs = list(zip(arms['mixed']['iterations'], arms['mixed-replay']['iterations']))
    replacements = {}
    for first, second in pairs:
        assert first['state_sha256'] == second['state_sha256']
        assert first['optimizer_payload_sha256'] == second['optimizer_payload_sha256']
        replacements[second['checkpoint']['path']] = first['checkpoint']['path']
        replacements[second['checkpoint']['sha256']] = first['checkpoint']['sha256']
    def rebind(document):
        # Restrict changes to the named checkpoint-provenance fields. All other
        # fields, including sampled actions, observations, logits and rewards,
        # must compare exactly after optimizer equality has been established.
        result = copy.deepcopy(document)
        for seat in result['seat_behaviors']:
            checkpoint = seat['source'].get('checkpoint')
            if checkpoint:
                for key in ('path', 'sha256'):
                    checkpoint[key] = replacements.get(checkpoint[key], checkpoint[key])
            identity = seat['identity']
            if identity.get('checkpoint_sha256'):
                value = identity['checkpoint_sha256']
                identity['checkpoint_sha256'] = replacements.get(value, value)
        checkpoint = result['episode'].get('opponent', {}).get('checkpoint')
        if checkpoint:
            for key in ('path', 'sha256'):
                checkpoint[key] = replacements.get(checkpoint[key], checkpoint[key])
        return result
    identical, equivalent = 0, 0
    for path in sorted((root / 'mixed/run/iterations').glob('*/attempt-000000/collect/episode-*.json')):
        other = root / 'mixed-replay/run/iterations' / path.relative_to(root / 'mixed/run/iterations')
        identical += path.read_bytes() == other.read_bytes()
        assert read(path) == rebind(read(other)), str(path)
        equivalent += 1
    # The project requires one actual byte-identical seed replay. The complete
    # first batch has identical source provenance and must pass without rebinding.
    assert arms['mixed']['trajectory_hashes'][:10] == arms['mixed-replay']['trajectory_hashes'][:10]
    assert identical >= 10 and equivalent == 40
    return {'byte_identical_trajectories': identical, 'trajectories_equal_after_checkpoint_provenance_rebinding': equivalent,
            'exact_parameter_and_adam_updates': 4,
            'limitation': 'Later checkpoint files include output-root trajectory pins. Rebinding those proven-equal model references is not raw file equality.'}


def audit(root):
    manifest = read(root / 'manifest.json')
    results = {arm: read(root / arm / 'complete.execution.json') for arm in ('preboard', 'mixed')}
    for stage in ('prefix', 'resume'):
        results['mixed-replay-' + stage] = read(root / 'mixed-replay' / (stage + '.execution.json'))
    for execution in results.values():
        assert execution['exit_code'] == 0 and not execution['timed_out']
        checked_pin(execution['stdout'])
        checked_pin(execution['stderr'])
    arms = {arm: audit_arm(root, manifest, arm) for arm in ('preboard', 'mixed', 'mixed-replay')}
    comparison = compare_replay(root, arms)
    replay_seconds = results['mixed-replay-prefix']['elapsed_seconds'] + results['mixed-replay-resume']['elapsed_seconds']
    assert max(results[a]['elapsed_seconds'] for a in ('preboard', 'mixed')) <= manifest['gates']['max_arm_wall_seconds']
    assert replay_seconds <= manifest['gates']['max_arm_wall_seconds']
    audit_result = {'schema': 'g115-postboard-qualification-result/v1', 'verdict': 'ENGINEERING-PASS',
             'arms': arms, 'executions': results, 'replay': comparison, 'paid_compute': False,
             'strength_claim': False, 'bounded_strength_pilot_launched': False}
    write(root / 'qualification-result.json', audit_result)
    print(json.dumps({'verdict': audit_result['verdict'], 'replay': comparison, 'strength_claim': False}), flush=True)


def main(root):
    manifest = read(root / 'manifest.json')
    for value in manifest['inputs']:
        checked_pin(value)
    validations = {}
    for arm, value in manifest['configs'].items():
        result = subprocess.run([str(checked_pin(manifest['binary'])), '--validate-config', str(checked_pin(value))],
                                text=True, capture_output=True, check=True)
        validations[arm] = json.loads(result.stdout)
    write(root / 'native-validation.json', validations)
    gpu = subprocess.check_output(['nvidia-smi', '--query-gpu=index,uuid,name,driver_version,utilization.gpu,memory.used', '--format=csv'], text=True)
    write(root / 'environment.json', {'observed_epoch': time.time(), 'gpu': gpu,
        'rustc': subprocess.check_output(['rustc', '-Vv'], text=True),
        'cargo': subprocess.check_output(['cargo', '-V'], text=True),
        'linker': {'path': 'C:/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/MSVC/14.50.35717/bin/Hostx64/x64/link.exe',
                   'observed_file_version': '14.50.35725.0'},
        'note': 'Current environment observed; exact reused producer executable is pinned separately. No rebuild.'})
    results = {}
    for arm in ('preboard', 'mixed'):
        results[arm] = execute(root, manifest, arm, 'complete')
    # A real process boundary exercises restoration of the full optimizer.
    results['mixed-replay-prefix'] = execute(root, manifest, 'mixed-replay', 'prefix', 1)
    results['mixed-replay-resume'] = execute(root, manifest, 'mixed-replay', 'resume')
    audit(root)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--audit-only', action='store_true')
    args = parser.parse_args()
    (audit if args.audit_only else main)(args.root.resolve())
