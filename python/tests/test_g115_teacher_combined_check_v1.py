"""Targeted ASTRA #022 failure injection; no native child or reservation."""
from contextlib import ExitStack
import json
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
        self.directory = tempfile.TemporaryDirectory(prefix='g115-receipt-check-', dir='D:/e-scratch')
        self.root = Path(self.directory.name).resolve()
        assert self.root.parent == Path('D:/e-scratch').resolve()
        assert self.root.name.startswith('g115-receipt-check-')
        self.addCleanup(self.directory.cleanup)
        self.scratch, self.cold = self.root / 'hot', self.root / 'cold'
        self.scratch.mkdir()
        self.cold.mkdir()
        self.final = self.cold / 'owner-completion.json'

    def test_progress_write_failure_stops_owned_child_and_seals_failure(self):
        child = Mock(pid=12345)
        child.poll.return_value = None
        child.wait.return_value = 1
        spawn = Mock(return_value=(child, {'placement': 'fixture'}))
        reservation = SimpleNamespace(TEST_ROOT_ENV='G115_OFFLINE_UNUSED', now_utc=lambda: 'fixture')
        adapter = SimpleNamespace(check_owner=Mock(return_value='fixture-token'), cleanup_auxiliary=Mock(return_value={}))
        plan = dict(scratch=str(self.scratch), cold=str(self.cold), source_commit='fixture', merge_commit='fixture',
                    source=str(self.root), reserve_bytes=60*packet.GIB, memory_reserve_bytes=32*packet.GIB,
                    cap_bytes=32*packet.GIB, closure_headroom_bytes=256*1024**2, cache_staging_estimate_bytes=0,
                    registry_source=str(self.root / 'no-cache'), environment={'PATH': ''}, auxiliary={},
                    tools={name: {'path': str(self.root / (name + '.exe'))} for name in ('cargo', 'rustc', 'linker')})
        plan.update({key: str(self.scratch / key) for key in ('target', 'temp', 'cargo_home')})
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


if __name__ == '__main__':
    unittest.main()
