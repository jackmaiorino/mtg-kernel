"""Production segment binding and raw resume transport, without native work."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

import public_training_dispatch_v2 as dispatch


class Segments(unittest.TestCase):
    def fixture(self, root):
        config = dict(gpu_ordinal=1, source=dict(checkpoint=None), entropy_coefficient=0,
                      updates=[[dict(id=str(i))] for i in range(486)], ppo=dict(epochs=2, minibatches=1))
        dispatch.write(root/'config.json', config)
        (root/'trainer.exe').write_bytes(b'never executed')
        binary = dispatch.pin(root/'trainer.exe')
        prior = root/'prior'
        output = prior/'outputs'
        (output/'0002').mkdir(parents=True)
        optimizer = dict(schema='mtg-kernel-public-input-optimizer-state/v1', legacy_adam_step=35646,
                         public=dict(adam_step=6))
        dispatch.write(output/'0002/optimizer.json', optimizer)
        optimizer_ref = dispatch.pin(output/'0002/optimizer.json')
        (output/'0002/episode-000.json').write_bytes(b'prior trajectory')
        trajectory = dispatch.pin(output/'0002/episode-000.json')
        checkpoint = dict(schema='mtg-kernel-public-input-checkpoint/v1', next_update=3,
            config_sha256='native-semantic-config', optimizer_file='optimizer.json',
            optimizer_sha256=optimizer_ref['sha256'], trajectory_sha256=[trajectory['sha256']])
        dispatch.write(output/'0002/checkpoint.json', checkpoint)
        dispatch.write(prior/'request.json', dict(config=config, resume=None, stop_after=3,
            output_directory=str(output), collector_workers=4, execution_gpu_ordinal=1))
        request_ref = dispatch.pin(prior/'request.json')
        dispatch.write(prior/'execution.json', dict(exit_code=0, timeout=False, binary_sha256=binary['sha256'],
            request_sha256=request_ref['sha256']))
        completion = dict(schema='mtg-kernel-public-input-run/v1', first_update=0, next_update=3,
            config_sha256=checkpoint['config_sha256'], receipts=[dict(update=i, episodes=1, natural_games=1,
            legacy_adam_step=35640+2*(i+1), public_adam_step=2*(i+1), after_state_sha256=optimizer_ref['sha256'])
            for i in range(3)])
        dispatch.write(output/'completion.json', completion)
        report = dict(config=dispatch.pin(root/'config.json'), binary=binary, request=request_ref,
            execution=dispatch.pin(prior/'execution.json'), completion=dispatch.pin(output/'completion.json'),
            local_output_directory=str(output), outputs={
                '0002/'+name: dispatch.pin(output/'0002'/name)
                for name in ('checkpoint.json', 'optimizer.json', 'episode-000.json')})
        dispatch.write(root/'prior-report.json', report)
        return config, binary, report, dict(stop_after=5, resume=dispatch.pin(root/'prior-report.json'))

    def test_exact_full_config_and_final_resume_closure(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config, binary, report, segment = self.fixture(root)
            plan = dispatch.segment_inputs(dispatch.pin(root/'config.json'), binary['sha256'], segment)
            self.assertEqual((plan['first_update'], plan['stop_after']), (3, 5))
            self.assertEqual(len(config['updates']), 486)
            request = dict(config=config, resume=plan['inputs']['checkpoint'], stop_after=5)
            dispatch.require_segment(request, binary['sha256'], plan)
            (root/'prior/outputs/0002/optimizer.json').write_bytes(b'changed optimizer')
            with self.assertRaisesRegex(ValueError, 'changed pinned input'):
                dispatch.segment_inputs(dispatch.pin(root/'config.json'), binary['sha256'], segment)

    def test_refuses_wrong_config_failed_execution_bad_boundary_and_empty_progress(self):
        for failure in ('config', 'execution', 'completion', 'optimizer-age', 'checkpoint-digest', 'stop', 'no-progress'):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                config, binary, report, segment = self.fixture(root)
                if failure == 'config':
                    config['updates'][4][0]['id'] = 'changed scientific schedule'
                    (root/'config.json').write_text(json.dumps(config))
                elif failure in ('execution', 'completion'):
                    path = Path(report[failure]['path'])
                    value = dispatch.read(path)
                    value['exit_code' if failure == 'execution' else 'next_update'] = 9
                    path.write_text(json.dumps(value))
                    report[failure] = dispatch.pin(path)
                elif failure == 'optimizer-age':
                    path = Path(report['outputs']['0002/optimizer.json']['path'])
                    value = dispatch.read(path); value['public']['adam_step'] = 1
                    path.write_text(json.dumps(value))
                    report['outputs']['0002/optimizer.json'] = dispatch.pin(path)
                    digest = report['outputs']['0002/optimizer.json']['sha256']
                    cp_path = Path(report['outputs']['0002/checkpoint.json']['path'])
                    checkpoint = dispatch.read(cp_path); checkpoint['optimizer_sha256'] = digest
                    cp_path.write_text(json.dumps(checkpoint))
                    report['outputs']['0002/checkpoint.json'] = dispatch.pin(cp_path)
                    done_path = Path(report['completion']['path'])
                    completion = dispatch.read(done_path); completion['receipts'][-1]['after_state_sha256'] = digest
                    done_path.write_text(json.dumps(completion))
                    report['completion'] = dispatch.pin(done_path)
                elif failure == 'checkpoint-digest':
                    path = Path(report['outputs']['0002/checkpoint.json']['path'])
                    value = dispatch.read(path); value['config_sha256'] = 'another scientific config'
                    path.write_text(json.dumps(value))
                    report['outputs']['0002/checkpoint.json'] = dispatch.pin(path)
                else:
                    segment['stop_after'] = 487 if failure == 'stop' else 3
                (root/'prior-report.json').write_text(json.dumps(report))
                segment['resume'] = dispatch.pin(root/'prior-report.json')
                with self.assertRaises((ValueError, KeyError)):
                    dispatch.segment_inputs(dispatch.pin(root/'config.json'), binary['sha256'], segment)

    def test_cold_relocation_preserves_original_report_bytes_and_config_binding(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config, binary, report, segment = self.fixture(root)
            plan = dispatch.segment_inputs(dispatch.pin(root/'config.json'), binary['sha256'], segment)
            cold = root/'cold'; cold.mkdir()
            refs = {}
            original_report = Path(segment['resume']['path']).read_bytes()
            for name, ref in plan['inputs'].items():
                target = cold/(name+'.json')
                shutil.copyfile(ref['path'], target)
                refs[name] = dispatch.pin(target)
            shutil.rmtree(root/'prior')
            (root/'prior-report.json').unlink()
            segment['resume'] = dict(report=refs.pop('report'), files=refs)
            moved = dispatch.segment_inputs(dispatch.pin(root/'config.json'), binary['sha256'], segment)
            self.assertEqual(moved['first_update'], 3)
            self.assertEqual((cold/'report.json').read_bytes(), original_report)

    def test_full_campaign_allocation_gate_precedes_segment_dispatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config, binary, report, segment = self.fixture(root)
            configs = dict(job=dispatch.pin(root/'config.json'))
            with patch('compute_throughput_v2.require_allocation', side_effect=ValueError('missing allocation')) as gate, \
                    patch.object(dispatch, '_dispatch_group') as launch:
                with self.assertRaisesRegex(ValueError, 'missing allocation'):
                    dispatch.dispatch_qualified(root/'fresh', binary, configs, 'allocation.json', 600, segments=dict(job=segment))
            self.assertEqual(gate.call_args.args[2], dict(job=dict(config_sha256=configs['job']['sha256'], updates=486)))
            launch.assert_not_called()
            limits = dict(explicit='target bounds')
            selected = dict(mode='sequential', placements=dict(job={}))
            with patch('compute_throughput_v2.require_allocation', return_value=selected), patch.object(dispatch, '_dispatch_group') as launch:
                dispatch.dispatch_qualified(root/'fresh', binary, configs, 'allocation.json', 600,
                                            segments=dict(job=segment), target_limits=limits)
            self.assertEqual(launch.call_args.kwargs, dict(segments=dict(job=segment), target_limits=limits))

    def test_remote_stages_raw_resume_files_and_reports_only_exclusive_segment(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            config, binary, prior, segment = self.fixture(root)
            destination = root/'remote-segment'
            local = destination/'computehost'
            archive = root/'remote.zip'
            uploads = []
            def run(command, **kwargs):
                if command[0] == 'scp':
                    if command[2].endswith('/results.zip'):
                        shutil.copyfile(archive, command[3])
                    elif '/resume/' in command[3]:
                        uploads.append(command)
                elif command[0] == 'ssh':
                    group = dispatch.read(local/'group.json')
                    spec = group['jobs']['job']
                    request = dispatch.read(local/'jobs/job/request.json')
                    self.assertEqual(request['config'], config)
                    self.assertEqual(request['execution_gpu_ordinal'], 0)
                    self.assertEqual(request['stop_after'], 5)
                    self.assertEqual(Path(request['resume']['path']).name, 'checkpoint.json')
                    self.assertEqual((local/'jobs/job/resume/report.json').read_bytes(), Path(segment['resume']['path']).read_bytes())
                    staged = copy.deepcopy(spec['segment'])
                    staged['inputs'] = {name: dispatch.pin(local/'jobs/job/resume'/(name+'.json')) for name in staged['inputs']}
                    checked_request = dict(request, resume=staged['inputs']['checkpoint'])
                    dispatch.require_segment(checked_request, binary['sha256'], staged)
                    payloads = {'jobs/job/request.json': (local/'jobs/job/request.json').read_bytes(),
                        'jobs/job/execution.json': b'{}', 'jobs/job/recovery.json': b'{}',
                        'jobs/job/outputs/completion.json': b'{"first_update":3,"next_update":5}'}
                    for update in (3, 4):
                        for name in ('checkpoint.json', 'optimizer.json', 'episode-000.json'):
                            payloads[f'jobs/job/outputs/{update:04}/{name}'] = b'new output'
                    hashes = {name: hashlib.sha256(data).hexdigest() for name, data in payloads.items()}
                    with zipfile.ZipFile(archive, 'x') as output:
                        for name, data in payloads.items(): output.writestr(name, data)
                        output.writestr('export-manifest.json', json.dumps(hashes))
                    kwargs['stdout'].write(json.dumps(dict(complete=True, setup_seconds=1,
                        recovery_and_release_seconds=1, export=dict(zip_sha256=dispatch.pin(archive)['sha256']))))
                else:
                    self.fail('unexpected subprocess '+str(command))
                return SimpleNamespace(returncode=0)
            placement = dict(host='computehost', gpu_ordinal=0, gpu_uuid='GPU-H0', workers=4, cpu_affinity=[0, 2])
            with patch.object(dispatch, 'preflight', return_value={}), patch.object(dispatch, 'ssh_ps'), \
                    patch.object(dispatch.subprocess, 'run', side_effect=run):
                group_pin = dispatch._dispatch_group(destination, binary, dict(job=dispatch.pin(root/'config.json')),
                    dict(job=placement), 'sequential', None, 600, segments=dict(job=segment))
            report = dispatch.read(dispatch.read(group_pin['path'])['jobs']['job']['path'])
            self.assertEqual(len(uploads), 6)
            self.assertEqual(set(report['outputs']), {f'{i:04}/{name}' for i in (3, 4)
                for name in ('checkpoint.json', 'optimizer.json', 'episode-000.json')})
            self.assertEqual(report['segment']['first_update'], 3)
            self.assertEqual(report['config']['sha256'], dispatch.pin(root/'config.json')['sha256'])


if __name__ == '__main__':
    unittest.main()
