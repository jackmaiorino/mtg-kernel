"""Hold Windows native children until owned-handle QoS readback succeeds."""
import os
import subprocess
import threading
from windows_owned_child_policy_v1 import configure_owned_child

_INSTALL_LOCK = threading.Lock()
_HELD = threading.local()


class _HeldPopen(subprocess.Popen):
    def __init__(self, *args, **kwargs):
        # Retain the partial object too: its Handle may already own raw_process
        # when __init__ raises. Closing through Handle avoids a later double close.
        _HELD.constructing = self
        super().__init__(*args, **kwargs)


def _install():
    import _winapi
    with _INSTALL_LOCK:
        if getattr(_winapi.CreateProcess, '_g115_held', False):
            return
        real = _winapi.CreateProcess
        real_close = _winapi.CloseHandle

        def close(handle):
            result = real_close(handle)
            if getattr(_HELD, 'active', False) and handle == getattr(_HELD, 'raw_thread', None):
                _HELD.raw_thread = None
            return result

        def create(*args, **kwargs):
            result = real(*args, **kwargs)
            if getattr(_HELD, 'active', False):
                process = thread = None
                try:
                    me = _winapi.GetCurrentProcess()
                    process = _winapi.DuplicateHandle(me, result[0], me, 0, False, _winapi.DUPLICATE_SAME_ACCESS)
                    thread = _winapi.DuplicateHandle(me, result[1], me, 0, False, _winapi.DUPLICATE_SAME_ACCESS)
                except BaseException:
                    # Popen does not own these handles yet. Reap before closing.
                    try:
                        _winapi.TerminateProcess(result[0], 1)
                        _winapi.WaitForSingleObject(result[0], _winapi.INFINITE)
                    finally:
                        if process is not None:
                            real_close(process)
                        real_close(result[1])
                        real_close(result[0])
                    raise
                _HELD.process, _HELD.thread = process, thread
                _HELD.raw_process, _HELD.raw_thread = result[:2]
            return result
        create._g115_held = True
        _winapi.CloseHandle = close
        _winapi.CreateProcess = create


def spawn_held(command, executable, **kwargs):
    if os.name != 'nt':
        return subprocess.Popen(command, **kwargs), None
    import ctypes
    from ctypes import wintypes as W
    import _winapi
    _install()
    if getattr(_HELD, 'active', False):
        raise RuntimeError('Recursive held spawn is unsupported')
    kwargs['creationflags'] = kwargs.get('creationflags', 0) | 4
    _HELD.active = True
    _HELD.thread = _HELD.process = _HELD.raw_process = _HELD.raw_thread = _HELD.constructing = None
    child = None
    try:
        child = _HeldPopen(command, **kwargs)
        # Popen returned successfully: it now owns process lifetime. The duplicate
        # protected construction only; retain the thread duplicate until resume.
        _winapi.CloseHandle(_HELD.process)
        _HELD.process = _HELD.raw_process = _HELD.constructing = None
        kernel = ctypes.WinDLL('kernel32', use_last_error=True)
        kernel.GetProcessIdOfThread.argtypes = [W.HANDLE]
        kernel.GetProcessIdOfThread.restype = W.DWORD
        kernel.ResumeThread.argtypes = [W.HANDLE]
        kernel.ResumeThread.restype = W.DWORD
        thread = _HELD.thread
        if thread is None or kernel.GetProcessIdOfThread(W.HANDLE(thread)) != child.pid:
            raise RuntimeError('Primary thread ownership differs')
        policy = configure_owned_child(child, executable)
        if kernel.ResumeThread(W.HANDLE(thread)) != 1:
            raise RuntimeError('Primary thread did not resume exactly once')
        return child, dict(policy, barrier='suspended through QoS readback; resumed once')
    except BaseException:
        if child is not None:
            child.kill()
            child.wait()
        elif _HELD.process is not None:
            try:
                _winapi.TerminateProcess(_HELD.process, 1)
                _winapi.WaitForSingleObject(_HELD.process, _winapi.INFINITE)
            finally:
                handle = getattr(_HELD.constructing, '_handle', None)
                if handle is not None:
                    handle.Close()
                else:
                    _winapi.CloseHandle(_HELD.raw_process)
                if _HELD.raw_thread is not None:
                    _winapi.CloseHandle(_HELD.raw_thread)
        raise
    finally:
        _HELD.active = False
        try:
            if _HELD.process is not None:
                _winapi.CloseHandle(_HELD.process)
        finally:
            try:
                if _HELD.thread is not None:
                    _winapi.CloseHandle(_HELD.thread)
            finally:
                _HELD.thread = _HELD.process = _HELD.raw_process = _HELD.raw_thread = _HELD.constructing = None
