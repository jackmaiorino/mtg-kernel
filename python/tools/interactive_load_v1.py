"""Interactive load on a shared Windows desktop (v1).

CPU used by processes in interactive sessions (Terminal Services session id
other than 0), in cores averaged over a short window. Batch work started over
ssh or WMI runs in session 0, so this measures the desktop owner's own use.
"""
from __future__ import annotations

import ctypes
import os
import time
from ctypes import wintypes

TH32CS_SNAPPROCESS = 0x2
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000


class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ProcessID", wintypes.DWORD),
                ("th32DefaultHeapID", ctypes.c_void_p), ("th32ModuleID", wintypes.DWORD),
                ("cntThreads", wintypes.DWORD), ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", ctypes.c_long), ("dwFlags", wintypes.DWORD), ("szExeFile", ctypes.c_wchar * 260)]


def _interactive_cpu_seconds() -> dict[int, float]:
    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    snapshot = k32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
    entry = PROCESSENTRY32W()
    entry.dwSize = ctypes.sizeof(entry)
    out = {}
    ok = k32.Process32FirstW(snapshot, ctypes.byref(entry))
    while ok:
        pid = entry.th32ProcessID
        session = wintypes.DWORD()
        if pid and k32.ProcessIdToSessionId(pid, ctypes.byref(session)) and session.value != 0:
            handle = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
            if handle:
                times = [wintypes.FILETIME() for _ in range(4)]
                if k32.GetProcessTimes(handle, *[ctypes.byref(t) for t in times]):
                    to_s = lambda t: ((t.dwHighDateTime << 32) | t.dwLowDateTime) / 1e7
                    out[pid] = to_s(times[2]) + to_s(times[3])
                k32.CloseHandle(handle)
        ok = k32.Process32NextW(snapshot, ctypes.byref(entry))
    k32.CloseHandle(snapshot)
    return out


def interactive_cores(window_seconds: float = 5.0) -> float:
    """Average cores used by interactive-session processes over the window (0 off Windows)."""
    if os.name != "nt":
        return 0.0
    before = _interactive_cpu_seconds()
    started = time.monotonic()
    time.sleep(window_seconds)
    after = _interactive_cpu_seconds()
    elapsed = time.monotonic() - started
    used = sum(max(0.0, after[pid] - before.get(pid, 0.0)) for pid in after if pid in before)
    return used / elapsed


if __name__ == "__main__":
    print(round(interactive_cores(10.0), 3))
