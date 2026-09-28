"""Offline ownership and ordering tests. No OS child is started."""
import sys
from pathlib import Path
import threading
import types
import unittest
from unittest.mock import Mock, patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import windows_held_spawn_v1 as held


class HeldTests(unittest.TestCase):
    def api(self):
        create = Mock(return_value=(10, 20, 30, 40)); create._g115_held = False
        return types.SimpleNamespace(CreateProcess=create,
            GetCurrentProcess=lambda: 1, DuplicateHandle=Mock(return_value=21),
            DUPLICATE_SAME_ACCESS=2, INFINITE=0xffffffff, TerminateProcess=Mock(),
            WaitForSingleObject=Mock(), CloseHandle=Mock())

    def test_duplicate_failure_reaps_raw_child(self):
        api = self.api(); api.DuplicateHandle.side_effect = OSError('injected')
        with patch.dict(sys.modules, {'_winapi': api}):
            held._install(); held._HELD.active = True
            try:
                with self.assertRaises(OSError): api.CreateProcess()
            finally: held._HELD.active = False
        api.TerminateProcess.assert_called_once_with(10, 1)
        api.WaitForSingleObject.assert_called_once_with(10, api.INFINITE)
        self.assertEqual([c.args[0] for c in api.CloseHandle.call_args_list], [20, 10])

    def test_concurrent_capture_and_single_install(self):
        api = self.api(); barrier = threading.Barrier(4)
        api.CreateProcess.side_effect = lambda: (10, threading.get_ident(), 30, 40)
        api.DuplicateHandle.side_effect = lambda me, thread, *args: thread
        results = []; errors = []
        def run():
            try:
                barrier.wait(); held._install(); held._HELD.active = True
                api.CreateProcess(); results.append((threading.get_ident(), held._HELD.thread))
            except BaseException as e: errors.append(e)
            finally: held._HELD.active = False
        with patch.dict(sys.modules, {'_winapi': api}):
            threads = [threading.Thread(target=run) for _ in range(4)]
            for t in threads: t.start()
            for t in threads: t.join()
        self.assertFalse(errors); self.assertEqual(len(results), 4)
        self.assertTrue(all(a == b for a, b in results))
        self.assertEqual(api.DuplicateHandle.call_count, 4)

    def test_policy_failure_never_resumes_and_reaps(self):
        self.exercise(True)

    def test_readback_precedes_resume(self):
        self.exercise(False)

    def exercise(self, fail):
        api = self.api(); child = Mock(pid=30); events = []
        kernel = Mock(); kernel.GetProcessIdOfThread.return_value = 30
        kernel.ResumeThread.side_effect = lambda h: (events.append('resume') or 1)
        def popen(*args, **kwargs):
            self.assertTrue(kwargs['creationflags'] & 4)
            held._HELD.thread = 21
            return child
        def policy(*args):
            events.append('readback')
            if fail: raise ValueError('injected policy failure')
            return {'verified': True}
        with patch.dict(sys.modules, {'_winapi': api}), patch.object(held, '_install'), patch.object(held.subprocess, 'Popen', side_effect=popen), patch.object(held, 'configure_owned_child', side_effect=policy), patch('ctypes.WinDLL', return_value=kernel):
            if fail:
                with self.assertRaises(ValueError): held.spawn_held(['fixture'], 'fixture')
                child.kill.assert_called_once(); child.wait.assert_called_once()
                self.assertEqual(events, ['readback'])
            else:
                result, receipt = held.spawn_held(['fixture'], 'fixture')
                self.assertIs(result, child); self.assertTrue(receipt['verified'])
                self.assertEqual(events, ['readback', 'resume']); child.kill.assert_not_called()
        api.CloseHandle.assert_called_once_with(21)
        self.assertFalse(held._HELD.active); self.assertIsNone(held._HELD.thread)


if __name__ == '__main__': unittest.main()
