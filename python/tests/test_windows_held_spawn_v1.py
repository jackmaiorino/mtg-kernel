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
        close = Mock()
        return types.SimpleNamespace(CreateProcess=create,
            GetCurrentProcess=lambda: 1, DuplicateHandle=Mock(side_effect=lambda me, h, *args: h+1),
            DUPLICATE_SAME_ACCESS=2, INFINITE=0xffffffff, TerminateProcess=Mock(),
            WaitForSingleObject=Mock(), CloseHandle=close, closed=close)

    def test_duplicate_failure_reaps_raw_child(self):
        api = self.api(); api.DuplicateHandle.side_effect = OSError('injected')
        with patch.dict(sys.modules, {'_winapi': api}):
            held._install(); held._HELD.active = True
            try:
                with self.assertRaises(OSError): api.CreateProcess()
            finally: held._HELD.active = False
        api.TerminateProcess.assert_called_once_with(10, 1)
        api.WaitForSingleObject.assert_called_once_with(10, api.INFINITE)
        self.assertEqual([c.args[0] for c in api.closed.call_args_list], [20, 10])

    def test_thread_duplicate_failure_closes_process_duplicate(self):
        api = self.api(); api.DuplicateHandle.side_effect = [11, OSError('injected')]
        def initialize(partial, *args, **kwargs):
            partial._child_created = False
            api.CreateProcess()
        with patch.dict(sys.modules, {'_winapi': api}), patch.object(held.os, 'name', 'nt'), \
                patch.object(held.subprocess.Popen, '__init__', initialize):
            with self.assertRaises(OSError): held.spawn_held(['fixture'], 'fixture')
        api.TerminateProcess.assert_called_once_with(10, 1)
        api.WaitForSingleObject.assert_called_once_with(10, api.INFINITE)
        self.assertEqual([c.args[0] for c in api.closed.call_args_list], [11, 20, 10])

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
        self.assertEqual(api.DuplicateHandle.call_count, 8)

    def test_constructor_failure_after_capture_before_handle_transfer(self):
        self.constructor_failure(False)

    def test_constructor_failure_after_popen_handle_transfer(self):
        self.constructor_failure(True)

    def constructor_failure(self, transferred):
        api = self.api(); kernel = Mock(); partial_handle = Mock()
        partials = []
        partial_handle.Close.side_effect = lambda: api.CloseHandle(10)
        def initialize(partial, *args, **kwargs):
            partial._child_created = False
            self.assertIs(held._HELD.constructing, partial)
            partials.append(partial)
            api.CreateProcess()
            if transferred:
                partial._child_created = True
                partial.returncode = None
                partial._handle = partial_handle
                api.CloseHandle(20)
            raise OSError('after successful capture')
        with patch.dict(sys.modules, {'_winapi': api}), patch.object(held.os, 'name', 'nt'), \
                patch.object(held.subprocess.Popen, '__init__', initialize), \
                patch('ctypes.WinDLL', return_value=kernel, create=True):
            with self.assertRaisesRegex(OSError, 'after successful capture'):
                held.spawn_held(['fixture'], 'fixture')
        api.TerminateProcess.assert_called_once_with(11, 1)
        api.WaitForSingleObject.assert_called_once_with(11, api.INFINITE)
        kernel.ResumeThread.assert_not_called()
        self.assertEqual(sorted(c.args[0] for c in api.closed.call_args_list), [10, 11, 20, 21])
        self.assertEqual(partial_handle.Close.call_count, int(transferred))
        self.assertFalse(partials[0]._child_created)
        with patch.object(partials[0], '_internal_poll', side_effect=AssertionError('closed handle polled')) as poll:
            partials[0].__del__()
            poll.assert_not_called()
        self.assertFalse(held._HELD.active)
        for name in ('process', 'thread', 'raw_process', 'raw_thread', 'constructing'):
            self.assertIsNone(getattr(held._HELD, name))

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
            held._HELD.thread, held._HELD.process = 21, 11
            return child
        def policy(*args):
            events.append('readback')
            if fail: raise ValueError('injected policy failure')
            return {'verified': True}
        with patch.dict(sys.modules, {'_winapi': api}), patch.object(held, '_install'), patch.object(held, '_HeldPopen', side_effect=popen), patch.object(held, 'configure_owned_child', side_effect=policy), patch('ctypes.WinDLL', return_value=kernel, create=True):
            if fail:
                with self.assertRaises(ValueError): held.spawn_held(['fixture'], 'fixture')
                child.kill.assert_called_once(); child.wait.assert_called_once()
                self.assertEqual(events, ['readback'])
            else:
                result, receipt = held.spawn_held(['fixture'], 'fixture')
                self.assertIs(result, child); self.assertTrue(receipt['verified'])
                self.assertEqual(events, ['readback', 'resume']); child.kill.assert_not_called()
        self.assertEqual([c.args[0] for c in api.closed.call_args_list], [11, 21])
        self.assertFalse(held._HELD.active); self.assertIsNone(held._HELD.thread)


if __name__ == '__main__': unittest.main()
