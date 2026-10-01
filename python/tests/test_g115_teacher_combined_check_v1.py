"""Targeted ASTRA #022/#033 checks; no native child or reservation."""
from contextlib import ExitStack
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import g115_teacher_combined_check_v1 as packet


class FailurePaths(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix='g115-receipt-check-')
        self.root = Path(self.directory.name).resolve()
        assert self.root.name.startswith('g115-receipt-check-')
        self.addCleanup(self.directory.cleanup)
        self.scratch, self.cold = self.root / 'hot', self.root / 'cold'
        self.scratch.mkdir()
        self.cold.mkdir()
        self.final = self.cold / 'owner-completion.json'

    def build_plan(self, plan):
        plan.update(code_under_check=packet.CODE_UNDER_CHECK, cargo_config=packet.CARGO_CONFIG,
                    caches=[], stage_timeout_seconds=3600,
                    pin_root=str(self.root / 'pins'), publication_allowance_bytes=2*packet.GIB,
                    target=str(self.scratch / 'target'), cargo_home=str(self.scratch / 'cargo-home'),
                    tools={n: {'path': str(self.root / (n + '.exe'))} for n in ('cargo','rustc','linker')})
        plan['stages'] = packet.stages(plan['tools']['cargo']['path'])
        return plan

    def test_progress_write_failure_stops_owned_child_and_seals_failure(self):
        child = Mock(pid=12345)
        child.poll.return_value = None
        child.wait.return_value = 1
        spawn = Mock(return_value=(child, {'placement': 'fixture'}))
        reservation = SimpleNamespace(TEST_ROOT_ENV='G115_OFFLINE_UNUSED', now_utc=lambda: 'fixture')
        adapter = SimpleNamespace(check_owner=Mock(return_value='fixture-token'), cleanup_auxiliary=Mock(return_value={}))
        plan = dict(scratch=str(self.scratch), cold=str(self.cold), source_commit='fixture', merge_commit='fixture',
                    source=str(self.root), reserve_bytes=60*packet.GIB, memory_reserve_bytes=32*packet.GIB,
                    cap_bytes=32*packet.GIB, closure_headroom_bytes=256*1024**2,
                    environment={'PATH': ''}, auxiliary={},
                    temp=str(self.scratch / 'temp'))
        self.build_plan(plan)
        real_save = packet.save

        def fail_progress(path, value):
            if path.name == 'progress.json':
                raise OSError('injected first progress write failure')
            real_save(path, value)

        with ExitStack() as stack:
            stack.enter_context(patch.dict(sys.modules, host_reservation_v1=reservation,
                                          g115_reserved_dispatch_v1=adapter,
                                          windows_held_spawn_v1=SimpleNamespace(spawn_held=spawn)))
            stack.enter_context(patch.object(packet.shutil, 'disk_usage', return_value=SimpleNamespace(free=128*packet.GIB)))
            stack.enter_context(patch.object(packet, 'available_memory', return_value=128*packet.GIB))
            stack.enter_context(patch.object(packet, 'clean'))
            stack.enter_context(patch.object(packet, 'before_cutoff'))
            stack.enter_context(patch.object(packet, 'save', side_effect=fail_progress))
            kill = stack.enter_context(patch.object(packet.subprocess, 'run'))
            self.assertEqual(packet.worker(plan, 'fixture'), 1)
        spawn.assert_called_once()
        kill.assert_called_once_with(['C:/Windows/System32/taskkill.exe', '/PID', '12345', '/T', '/F'],
                                     capture_output=True, timeout=30)
        child.wait.assert_called_once_with(timeout=30)
        receipt = json.loads(self.final.read_text())
        self.assertFalse(receipt['complete'])
        self.assertIn('injected first progress write failure', receipt['error'])
        self.assertEqual(len(receipt['steps']), 1)

    def test_three_stage_worker_preserves_default_features_and_capture(self):
        self.worker_fixture()

    def test_publication_failure_seals_failure_before_test_or_later_builds(self):
        self.worker_fixture(fail_publication=True)

    def worker_fixture(self, fail_publication=False):
        plan = dict(scratch=str(self.scratch), cold=str(self.cold), source_commit='helper-fixture',
                    merge_commit='fixture',
                    source=str(self.root), reserve_bytes=60*packet.GIB, memory_reserve_bytes=32*packet.GIB,
                    cap_bytes=32*packet.GIB, closure_headroom_bytes=256*1024**2,
                    environment={'PATH': '', 'RUST_TEST_NOCAPTURE': '1'}, auxiliary={},
                    temp=str(self.scratch / 'temp'))
        self.build_plan(plan)
        reservation = SimpleNamespace(TEST_ROOT_ENV='G115_OFFLINE_UNUSED', now_utc=lambda: 'fixture')
        adapter = SimpleNamespace(check_owner=Mock(return_value='fixture-token'), cleanup_auxiliary=Mock(return_value=[]))
        calls = []

        def spawn(argv, executable, **kwargs):
            index = len(calls)
            if index == 1:
                self.assertEqual(argv[1:], [packet.AFFECTED_TEST, '--exact'])
                pinned = Path(argv[0])
                self.assertEqual(pinned.parent.parent, Path(plan['pin_root']))
                self.assertEqual(packet.sha(pinned), pinned.parent.name)
                self.assertEqual(pinned.read_bytes(), b'mocked library test')
                output = ('running 1 test\ntest ' + packet.AFFECTED_TEST + ' ... ok\n'
                          'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2513 filtered out; finished in 0.01s\n')
            else:
                stage_index = 0 if index == 0 else index - 1
                self.assertEqual(argv, plan['stages'][stage_index]['argv'])
                output = ''
                if index in (0, 2):
                    binary = Path(plan['target']) / 'release' / ('test.exe' if index == 0 else 'kernel_rl_env.exe')
                    binary.parent.mkdir(parents=True, exist_ok=True)
                    binary.write_bytes(b'mocked library test' if index == 0 else b'mocked JSONL')
                    output = json.dumps({'reason': 'compiler-artifact', 'executable': str(binary),
                                         'profile': {'test': index == 0}, 'target': {'name': 'mtg_kernel'}}) + '\n'
                if index == 3:
                    # JSONL is already durable before the CUDA check starts.
                    jsonl = list(Path(plan['pin_root']).glob('*/kernel_rl_env.exe'))
                    self.assertEqual(len(jsonl), 1)
                    self.assertEqual(jsonl[0].parent.name, packet.sha(jsonl[0]))
            calls.append((argv, kwargs['env'].copy()))
            kwargs['stdout'].write(output.encode())
            child = Mock(pid=12345)
            child.wait.return_value = 0
            child.poll.return_value = 0
            return child, {'placement': 'fixture'}

        with ExitStack() as stack:
            stack.enter_context(patch.dict(os.environ, RUST_TEST_NOCAPTURE='1'))
            stack.enter_context(patch.dict(sys.modules, host_reservation_v1=reservation,
                                          g115_reserved_dispatch_v1=adapter,
                                          windows_held_spawn_v1=SimpleNamespace(spawn_held=spawn)))
            stack.enter_context(patch.object(packet.shutil, 'disk_usage', return_value=SimpleNamespace(free=128*packet.GIB)))
            stack.enter_context(patch.object(packet, 'available_memory', return_value=128*packet.GIB))
            stack.enter_context(patch.object(packet, 'clean'))
            stack.enter_context(patch.object(packet, 'before_cutoff'))
            if fail_publication:
                actual_publish = packet.publish_binary
                def fail_copy(*args):
                    with patch.object(packet.os, 'fsync', side_effect=OSError('injected binary flush failure')):
                        return actual_publish(*args)
                stack.enter_context(patch.object(packet, 'publish_binary', side_effect=fail_copy))
            self.assertEqual(packet.worker(plan, 'fixture'), 1 if fail_publication else 0)
        receipt = json.loads(self.final.read_text())
        if fail_publication:
            self.assertFalse(receipt['complete'])
            self.assertEqual(len(calls), 1)
            self.assertIn('injected binary flush failure', receipt['error'])
            publication = receipt['publications'][0]
            self.assertEqual(publication['accounted_bytes'], 4096)
            self.assertTrue(Path(publication['staging_path']).is_file())
            self.assertFalse(Path(publication['path']).exists())
            return
        self.assertEqual(len(calls), 4)
        for index, (argv, env) in enumerate(calls):
            self.assertEqual('--features' in argv, index == 3)
            self.assertEqual(env['CARGO_BUILD_JOBS'], '4')
            self.assertEqual(env['CARGO_HOME'], plan['cargo_home'])
            self.assertNotIn('RUST_TEST_NOCAPTURE', env)
        self.assertTrue(receipt['complete'])
        self.assertEqual(receipt['selected_passed'], 1)
        self.assertEqual(receipt['stages_passed'], 3)
        self.assertEqual(len(receipt['build_outputs']), 2)
        self.assertEqual(receipt['selected_test_names'], [packet.AFFECTED_TEST])
        self.assertEqual([r['label'] for r in receipt['steps']],
                         ['default-invariance', 'default-invariance-exact', 'default-jsonl', 'cuda-compile'])
        self.assertEqual(sum(r['accounted_bytes'] for r in receipt['publications']), 8192)
        self.assertGreaterEqual(receipt['final_resources']['accounted_bytes'],
                                packet.size(self.scratch / 'target') + 8192)

    def test_existing_pin_reused_without_write_and_mismatch_refused(self):
        binary = self.root / 'example.exe'
        binary.write_bytes(b'fixture')
        root = self.root / 'pins'
        rows, guard = [], Mock()
        pinned = packet.publish_binary(binary, root, rows, guard, 8192)
        before = pinned.stat().st_mtime_ns
        with patch.object(packet.tempfile, 'NamedTemporaryFile', side_effect=AssertionError('must reuse')):
            reused = []
            self.assertEqual(packet.publish_binary(binary, root, reused, guard, 8192), pinned)
        self.assertEqual(pinned.stat().st_mtime_ns, before)
        self.assertTrue(reused[0]['reused'])
        self.assertEqual(reused[0]['accounted_bytes'], 4096)
        pinned.write_bytes(b'corrupt')
        with self.assertRaisesRegex(RuntimeError, 'Existing binary pin differs'):
            packet.publish_binary(binary, root, [], guard, 8192)
        self.assertEqual(pinned.read_bytes(), b'corrupt')

    def test_publication_charged_before_staging_and_budget_refuses(self):
        binary = self.root / 'example.exe'
        binary.write_bytes(b'x'*5000)
        root, rows = self.root / 'pins', []
        calls = []
        def guard(extra=0):
            calls.append((extra, sum(r['accounted_bytes'] for r in rows)))
        packet.publish_binary(binary, root, rows, guard, 8192)
        self.assertEqual(calls[0], (8192, 0))
        self.assertTrue(all(charge == 8192 for _, charge in calls[1:]))
        with self.assertRaisesRegex(RuntimeError, 'publication allowance'):
            packet.publish_binary(binary, root, rows, guard, 8192)
        other = self.root / 'refused'
        with self.assertRaisesRegex(RuntimeError, 'reserve'):
            packet.publish_binary(binary, other, [], Mock(side_effect=RuntimeError('reserve')), 8192)
        self.assertFalse(other.exists())

    def test_interrupted_staging_never_publishes_success_or_failure(self):
        for complete in (True, False):
            with self.subTest(complete=complete):
                destination = self.cold / f'staging-{complete}.json'
                with patch.object(packet.os, 'fsync', side_effect=OSError('injected staging interruption')):
                    with self.assertRaises(OSError):
                        packet.publish_receipt(destination, {'complete': complete})
                self.assertFalse(destination.exists())
                packet.publish_receipt(destination, {'complete': False, 'seal_error': 'staging interrupted'})
                self.assertFalse(json.loads(destination.read_text())['complete'])

    def test_verification_failure_keeps_final_absent_until_failure_published(self):
        with patch.object(Path, 'read_bytes', return_value=b'partial'):
            with self.assertRaisesRegex(RuntimeError, 'staging differs'):
                packet.publish_receipt(self.final, {'complete': True})
        self.assertFalse(self.final.exists())
        packet.publish_receipt(self.final, {'complete': False})
        self.assertFalse(json.loads(self.final.read_text())['complete'])

    def test_failed_rename_keeps_final_absent_until_failure_published(self):
        with patch.object(Path, 'rename', side_effect=OSError('injected rename failure')):
            with self.assertRaises(OSError):
                packet.publish_receipt(self.final, {'complete': True})
        self.assertFalse(self.final.exists())
        packet.publish_receipt(self.final, {'complete': False})
        self.assertFalse(json.loads(self.final.read_text())['complete'])

    def test_success_is_complete_and_sealed_receipt_is_not_overwritten(self):
        value = {'complete': True, 'steps': [{'exit_code': 0}]}
        packet.publish_receipt(self.final, value)
        self.assertEqual(json.loads(self.final.read_text()), value)
        with self.assertRaisesRegex(RuntimeError, 'already sealed'):
            packet.publish_receipt(self.final, {'complete': False})
        self.assertEqual(json.loads(self.final.read_text()), value)

    def test_worker_admission_failure_uses_fixed_paths_and_starts_no_stage(self):
        prior = self.root / 'teacher-combined-check-001'
        prior.mkdir()
        historical = prior / 'owner-completion.json'
        historical.write_bytes(b'preserved failed attempt001\n')
        args = ['packet', '--worker', '--manifest', str(self.root / 'untrusted.json'), '--manifest-sha256', 'a'*64]
        with patch.object(sys, 'argv', args), patch.object(packet, 'SCRATCH', self.scratch), \
                patch.object(packet, 'COLD', self.cold), patch.object(packet, 'validate', side_effect=RuntimeError('Pin differs')), \
                patch.object(packet, 'worker') as worker, patch.object(packet, 'dispatch') as dispatch:
            self.assertEqual(packet.main(), 1)
        worker.assert_not_called()
        dispatch.assert_not_called()
        receipt = json.loads(self.final.read_text())
        self.assertFalse(receipt['complete'])
        self.assertEqual(receipt['phase'], 'worker-admission')
        self.assertEqual(receipt['steps'], [])
        self.assertIn('Pin differs', receipt['error'])
        self.assertLess(self.final.stat().st_size, 16384)
        self.assertEqual(receipt, json.loads((self.scratch / 'owner-completion.json').read_text()))
        self.assertEqual(historical.read_bytes(), b'preserved failed attempt001\n')

    def test_validation_accepts_006_and_refuses_old_paths_scope_and_cache_growth(self):
        self.assertEqual(packet.WORK_ID, 'teacher-combined-check-006')
        inventory = {'files': 1, 'allocated_bytes': 4096, 'inventory_sha256': 'fixture'}
        plan = dict(schema='g115-teacher-combined-check/v1', work_id=packet.WORK_ID,
                    jobs=4, projected_bytes=12*packet.GIB, pins=[], script=packet.__file__,
                    cap_bytes=32*packet.GIB, reserve_bytes=60*packet.GIB, memory_reserve_bytes=32*packet.GIB,
                    closure_headroom_bytes=256*1024**2,
                    scratch=str(packet.SCRATCH), cold=str(packet.COLD),
                    owner_completion=str(packet.COLD / 'owner-completion.json'),
                    active_progress=str(packet.SCRATCH / 'progress.json'),
                    active_dispatch=str(packet.SCRATCH / 'dispatch.json'))
        self.build_plan(plan)
        plan['pin_root'] = str(packet.PIN_ROOT)
        plan['caches']=[{'path': str(packet.CACHE_ROOT / a), 'destination': b, 'inventory': inventory}
                       for a,b in [('target','target'),('cargo-home/registry','cargo-home/registry')]]
        plan.update({key: str(packet.SCRATCH / suffix) for key,suffix in
                     [('temp','temp'),('target','target'),('cargo_home','cargo-home')]})
        manifest = self.root / 'manifest.json'
        with patch.object(packet, 'sha', return_value='fixture'), patch.object(packet, 'clean'), \
                patch.object(packet, 'cache_inventory', return_value=inventory) as census:
            manifest.write_text(json.dumps(plan))
            self.assertEqual(packet.validate(manifest, 'fixture'), plan)
            for key in ('scratch','cold','temp','target','cargo_home','owner_completion','active_progress','active_dispatch'):
                for old in range(1,6):
                    changed = dict(plan); changed[key] = plan[key].replace('check-006', f'check-{old:03}')
                    manifest.write_text(json.dumps(changed))
                    with self.subTest(key=key, old=old), self.assertRaises(RuntimeError):
                        packet.validate(manifest, 'fixture')
            changed = json.loads(json.dumps(plan));changed['stages'][0]['argv'].insert(1, '--features=experimental-burn-net8-packed-cuda-v1')
            manifest.write_text(json.dumps(changed))
            with self.assertRaisesRegex(RuntimeError, 'Three-stage scope'):
                packet.validate(manifest, 'fixture')
            manifest.write_text(json.dumps(plan));census.return_value = dict(inventory, allocated_bytes=8192)
            with self.assertRaisesRegex(RuntimeError, 'Cache inventory changed'):
                packet.validate(manifest, 'fixture')

    def test_dispatch_uses_new_work_id_and_paths_without_native_execution(self):
        reservation = SimpleNamespace(TEST_ROOT_ENV='G115_OFFLINE_UNUSED', now_utc=lambda: 'fixture',
                                      dispatch=Mock(return_value={'state': 'dispatched'}))
        plan = dict(scratch=str(self.scratch), cold=str(self.cold), source_commit='fixture',
                    cap_bytes=32*packet.GIB, reserve_bytes=60*packet.GIB,
                    memory_reserve_bytes=32*packet.GIB, busy_pattern='fixture')
        with patch.dict(sys.modules, host_reservation_v1=reservation), \
                patch.object(packet.shutil, 'disk_usage', return_value=SimpleNamespace(free=128*packet.GIB)), \
                patch.object(packet, 'available_memory', return_value=128*packet.GIB), \
                patch.object(packet, 'before_cutoff'), patch('builtins.print'):
            packet.dispatch(plan, self.cold / 'manifest.json', 'fixture')
        args, kwargs = reservation.dispatch.call_args
        self.assertEqual(args[1], 'teacher-combined-check-006')
        self.assertEqual(args[4], str(self.scratch))
        self.assertEqual(kwargs['transport_record']['owner_completion'], str(self.final))
        self.assertIn(str(self.cold / 'manifest.json'), args[3])


class LinkerHelpExitPolicy(unittest.TestCase):
    def setUp(self):
        self.plan = {'tools': {'linker': {'path': str(packet.LINKER), 'sha256': packet.LINKER_SHA256}}}
        self.argv = [str(packet.LINKER), '/?']
        self.help = (packet.LINKER_BANNER + '\nCopyright (C) Microsoft Corporation.\n\n '
                     + packet.LINKER_USAGE + '\n   options:\n      /ERRORREPORT:{NONE|PROMPT|QUEUE|SEND}\n')

    def check(self, **overrides):
        args = dict(plan=self.plan, label='version-linker', argv=self.argv, code=1100, output=self.help)
        args.update(overrides)
        packet.check_stage_exit(**args)

    def test_exact_pinned_help_allows_1100_and_other_stage_zero_is_unchanged(self):
        with patch.object(packet, 'sha', return_value=packet.LINKER_SHA256) as digest:
            self.check()
            digest.assert_called_once_with(self.argv[0])
        # No executable pin read or new output policy for ordinary exit-zero stages.
        with patch.object(packet, 'sha', side_effect=AssertionError('unexpected pin read')):
            for label in ('version-cargo', 'version-rustc', 'test-build', 'test-00'):
                self.check(label=label, argv=['fixture'], code=0, output='')

    def test_nonzero_exception_rejects_other_stages_executables_arguments_and_codes(self):
        cases = ([{'label': label} for label in ('version-cargo', 'version-rustc', 'test-build', 'test-00')]
                 + [{'argv': ['C:/other/link.exe', '/?']}, {'argv': self.argv + ['/extra']},
                    {'argv': [self.argv[0], '/VERSION']}, {'argv': [self.argv[0]]},
                    {'code': 1}, {'code': 1101}])
        with patch.object(packet, 'sha', return_value=packet.LINKER_SHA256):
            for change in cases:
                with self.subTest(change=change), self.assertRaisesRegex(RuntimeError, 'Stage failed'):
                    self.check(**change)

    def test_exception_requires_manifest_identity_and_actual_executable_hash(self):
        for path, pin, actual in [('C:/other/link.exe', packet.LINKER_SHA256, packet.LINKER_SHA256),
                                  (self.argv[0], 'wrong', packet.LINKER_SHA256),
                                  (self.argv[0], packet.LINKER_SHA256, 'wrong')]:
            with self.subTest(path=path, pin=pin, actual=actual), patch.object(packet, 'sha', return_value=actual):
                with self.assertRaisesRegex(RuntimeError, 'executable pin differs'):
                    self.check(plan={'tools': {'linker': {'path': path, 'sha256': pin}}})

    def test_exception_requires_complete_help_without_actual_error_diagnostics(self):
        bad_outputs = ['', self.help.replace('14.50.35725.0', '14.50.0.0'),
                       self.help.replace(packet.LINKER_USAGE, ''), self.help.replace('options:', '')]
        diagnostics = ['LINK : fatal error LNK1104: cannot open file',
                       'unit.obj : error LNK2001: unresolved external symbol',
                       'error: invalid option', 'link : ERROR LNK9999: failed']
        with patch.object(packet, 'sha', return_value=packet.LINKER_SHA256):
            for output in bad_outputs + [self.help + line + '\n' for line in diagnostics]:
                with self.subTest(output=output), self.assertRaises(RuntimeError):
                    self.check(output=output)


if __name__ == '__main__':
    unittest.main()
