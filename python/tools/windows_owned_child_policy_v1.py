"""Readback-verified QoS for a Popen-owned Windows child; never enumerate PIDs.

Call immediately after spawn. This does not suspend the child and makes no claim
that its first instructions ran under the requested QoS. The caller must stop
its owned child on failure and retain the error rather than continue measurement.
Microsoft contract: https://learn.microsoft.com/en-us/windows/win32/api/
processthreadsapi/nf-processthreadsapi-setprocessinformation
"""
import ctypes
from ctypes import wintypes as W
import os
from pathlib import Path
import subprocess


class PowerState(ctypes.Structure):
    _fields_ = [("Version", W.DWORD), ("ControlMask", W.DWORD), ("StateMask", W.DWORD)]


def configure_owned_child(child, expected_executable):
    if os.name != "nt" or not isinstance(child, subprocess.Popen):
        raise ValueError("Windows Popen-owned child required")
    if child.poll() is not None:
        raise ValueError("owned child already exited")
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    signatures = {
        "GetProcessId": ([W.HANDLE], W.DWORD),
        "GetPriorityClass": ([W.HANDLE], W.DWORD),
        "GetProcessAffinityMask": ([W.HANDLE, ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t)], W.BOOL),
        "QueryFullProcessImageNameW": ([W.HANDLE, W.DWORD, W.LPWSTR, ctypes.POINTER(W.DWORD)], W.BOOL),
        "GetProcessInformation": ([W.HANDLE, ctypes.c_int, ctypes.c_void_p, W.DWORD], W.BOOL),
        "SetProcessInformation": ([W.HANDLE, ctypes.c_int, ctypes.c_void_p, W.DWORD], W.BOOL),
    }
    for name, (args, result) in signatures.items():
        getattr(kernel, name).argtypes = args
        getattr(kernel, name).restype = result
    handle = W.HANDLE(int(child._handle))

    def check(ok):
        if not ok:
            raise ctypes.WinError(ctypes.get_last_error())

    def snapshot():
        pid = kernel.GetProcessId(handle)
        check(pid)
        priority = kernel.GetPriorityClass(handle)
        check(priority)
        process_mask, system_mask = ctypes.c_size_t(), ctypes.c_size_t()
        check(kernel.GetProcessAffinityMask(handle, ctypes.byref(process_mask), ctypes.byref(system_mask)))
        size = W.DWORD(32768)
        name = ctypes.create_unicode_buffer(size.value)
        check(kernel.QueryFullProcessImageNameW(handle, 0, name, ctypes.byref(size)))
        power = PowerState(1, 0, 0)
        check(kernel.GetProcessInformation(handle, 4, ctypes.byref(power), ctypes.sizeof(power)))
        return dict(pid=pid, executable=name.value, priority_class=priority,
                    affinity_mask=process_mask.value, system_affinity_mask=system_mask.value,
                    power_version=power.Version, power_control_mask=power.ControlMask,
                    power_state_mask=power.StateMask)

    before = snapshot()
    if before["pid"] != child.pid or not Path(before["executable"]).samefile(expected_executable):
        raise ValueError("owned handle identity differs from expected executable/PID")
    if before["priority_class"] != subprocess.BELOW_NORMAL_PRIORITY_CLASS:
        raise ValueError("owned child was not spawned BelowNormal")
    # Preserve other throttling mechanisms; change execution-speed control only.
    requested = PowerState(1, before["power_control_mask"] | 1, before["power_state_mask"] & ~1)
    check(kernel.SetProcessInformation(handle, 4, ctypes.byref(requested), ctypes.sizeof(requested)))
    after = snapshot()
    for key in ["pid", "executable", "priority_class", "affinity_mask", "system_affinity_mask"]:
        if before[key] != after[key]:
            raise RuntimeError(f"owned child changed {key} during QoS application")
    if after["power_control_mask"] != requested.ControlMask or after["power_state_mask"] != requested.StateMask:
        raise RuntimeError("owned child power policy readback differs")
    return dict(schema="windows-owned-child-policy/v1", before=before, after=after,
                verified=True, scope="Popen process handle only; no process enumeration")
