"""Matched, bounded engineering timing of sealed evaluators on valid games."""
import argparse
import copy
from pathlib import Path
import statistics

from prepare_postboard_qualification_v1 import read, write, pin
from postboard_bo3_qualification_v1 import execute
from postboard_repaired_evaluation_v1 import compare_store, check_adapter_receipts
from run_postboard_pilot_v1 import verify_pin


def run(root):
    pilot = Path('E:/mtg-postboard-campaign-20260920/pilot-001')
    qualification = Path('E:/mtg-postboard-campaign-20260920/spell-target-adapter-qualification-001')
    old_manifest = read(pilot / 'manifest.json')
    new_manifest = read(qualification / 'manifest.json')
    old_job = next(j for j in read(pilot / 'reference-dispatch.json')['jobs']
                   if j['name'] == 'g115-Affinity-p0')
    original = read(verify_pin(old_job['config']))
    verify_pin(old_manifest['binary']); verify_pin(new_manifest['binary'])
    root.mkdir()
    for name in ('configs', 'receipts', 'outputs'):
        (root / name).mkdir()
    # Latin-square order controls simple monotonic warm-up/drift. One worker.
    orders = [('old', 'new-off', 'new-on'), ('new-off', 'new-on', 'old'),
              ('new-on', 'old', 'new-off')]
    jobs = []
    for repeat, order in enumerate(orders):
        for condition in order:
            config = copy.deepcopy(original)
            config['matches'] = config['matches'][:8]
            if condition != 'old':
                config['v3_spell_target_reference_adapter'] = condition == 'new-on'
                config['model_sources'][1] = new_manifest['opponent_source']
            name = f'{repeat}-{condition}'
            config['output_directory'] = (root / 'outputs' / name).as_posix()
            path = root / 'configs' / (name + '.json')
            write(path, config)
            jobs.append({'name': name, 'condition': condition, 'repeat': repeat,
                         'config': pin(path)})
    manifest = {
        'schema': 'postboard-adapter-timing/v1', 'script': pin(__file__),
        'question': 'Does the opt-in adapter cause the valid-domain slowdown, or does it persist with the same new binary and adapter disabled?',
        'binary_by_condition': {'old': old_manifest['binary'],
                               'new-off': new_manifest['binary'], 'new-on': new_manifest['binary']},
        'source_config': old_job['config'], 'import_binding': new_manifest['import_binding'],
        'jobs': jobs, 'matches_per_job': 8, 'total_matches': 72, 'workers': 1,
        'gates': {'max_job_seconds': 30, 'max_worker_seconds': 270},
        'correctness': 'All eight match bytes equal historical output for old; exact gameplay equality after only verified import metadata rebinding for new. New-on has zero repair decisions.',
        'analysis': 'Report all elapsed times and median within-repeat ratios; engineering diagnosis only, no win-rate analysis or statistical strength claim.',
        'limitations': 'One Affinity mirror batch, three repetitions, no CPU affinity pin; does not establish production throughput across the meta.',
    }
    write(root / 'manifest.json', manifest)
    results = []
    for job in jobs:
        local = dict(manifest, binary=manifest['binary_by_condition'][job['condition']])
        result = execute(root, job, local)
        results.append(dict(result, condition=job['condition'], repeat=job['repeat']))
        assert result['exit_code'] == 0 and not result['timed_out'], result
        directory = Path(result['output_directory'])
        assert read(directory / 'completion.json')['completed_matches'] == 8
        assert len(list(directory.glob('match-*.json'))) == 8
        if job['condition'] == 'new-on':
            assert check_adapter_receipts(directory, 1, 8) == 0
        for i in range(8):
            before = pilot / 'outputs' / old_job['name'] / f'match-{i:06}.json'
            after = directory / before.name
            if job['condition'] == 'old':
                assert before.read_bytes() == after.read_bytes()
            else:
                compare_store(before, after, 1, manifest['import_binding'])
        print({'job': job['name'], 'seconds': result['elapsed_seconds'],
               'matches_equal': 8}, flush=True)
    write(root / 'execution.json', results)
    times = {(r['repeat'], r['condition']): r['elapsed_seconds'] for r in results}
    ratios = {label: [times[(i, numerator)] / times[(i, denominator)] for i in range(3)]
              for label, numerator, denominator in [
                  ('new_on_over_new_off', 'new-on', 'new-off'),
                  ('new_off_over_old', 'new-off', 'old'),
                  ('new_on_over_old', 'new-on', 'old')]}
    seconds = sum(r['elapsed_seconds'] for r in results)
    assert seconds <= manifest['gates']['max_worker_seconds']
    result = {'verdict': 'ENGINEERING-PASS', 'completed_matches': 72,
              'all_matches_equal': True, 'repair_decisions': 0,
              'worker_seconds': seconds, 'within_repeat_ratios': ratios,
              'median_ratios': {k: statistics.median(v) for k, v in ratios.items()},
              'outcomes_analyzed': False}
    write(root / 'result.json', result)
    print(result, flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    run(parser.parse_args().root.resolve())
