"""Hold Windows native children until owned-handle QoS readback succeeds."""
import os
import subprocess
import threading
from windows_owned_child_policy_v1 import configure_owned_child

_INSTALL_LOCK = threading.Lock()
_HELD = threading.local()


def _install():
    import _winapi
    with _INSTALL_LOCK:
        if getattr(_winapi.CreateProcess, '_g115_held', False):
            return
        real = _winapi.CreateProcess

        def create(*args, **kwargs):
            result = real(*args, **kwargs)
            if getattr(_HELD, 'active', False):
                try:
                    me = _winapi.GetCurrentProcess()
                    _HELD.thread = _winapi.DuplicateHandle(me, result[1], me, 0, False, _winapi.DUPLICATE_SAME_ACCESS)
                except BaseException:
                    # Popen does not own these handles yet. Reap before closing.
                    try:
                        _winapi.TerminateProcess(result[0], 1)
                        _winapi.WaitForSingleObject(result[0], _winapi.INFINITE)
                    finally:
                        _winapi.CloseHandle(result[1])
                        _winapi.CloseHandle(result[0])
                    raise
            return result
        create._g115_held = True
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
    _HELD.active, _HELD.thread = True, None
    child = None
    try:
        child = subprocess.Popen(command, **kwargs)
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
        raise
    finally:
        _HELD.active = False
        if _HELD.thread is not None:
            _winapi.CloseHandle(_HELD.thread)
            _HELD.thread = None
