"""Read-only Windows host-control preflight for WSL CI.

Queries native HCS directly, including a known VM when supplied, without
changing VM configuration, launching guest work, or enabling runners.
Successful queries do not qualify CPU isolation or priority enforcement.
"""
from __future__ import annotations

import argparse
import ctypes
import json
import os
import sys
from pathlib import Path
from uuid import UUID


def bind(dll, name, result, *arguments):
    function = getattr(dll, name)
    function.restype = result
    function.argtypes = list(arguments)
    return function


def inspect_vm_process(pid: int) -> dict:
    """Check the exact process through narrow read/control handles; never set anything."""
    kernel = ctypes.WinDLL("kernel32.dll", use_last_error=True)
    open_process = bind(kernel, "OpenProcess", ctypes.c_void_p, ctypes.c_uint32,
                        ctypes.c_int, ctypes.c_uint32)
    close_handle = bind(kernel, "CloseHandle", ctypes.c_int, ctypes.c_void_p)
    get_affinity = bind(kernel, "GetProcessAffinityMask", ctypes.c_int, ctypes.c_void_p,
                        ctypes.POINTER(ctypes.c_size_t), ctypes.POINTER(ctypes.c_size_t))
    get_priority = bind(kernel, "GetPriorityClass", ctypes.c_uint32, ctypes.c_void_p)
    report = {"pid": pid}
    for name, access in (("query", 0x1000), ("control", 0x0200)):
        handle = open_process(access, False, pid)
        report[name] = {"ok": bool(handle), "winerror": 0 if handle else ctypes.get_last_error()}
        if not handle:
            continue
        try:
            if name == "query":
                process_mask, system_mask = ctypes.c_size_t(), ctypes.c_size_t()
                ok = get_affinity(handle, ctypes.byref(process_mask), ctypes.byref(system_mask))
                report["affinity"] = {"ok": bool(ok), "winerror": 0 if ok else ctypes.get_last_error(),
                                      "mask": process_mask.value if ok else None}
                priority = get_priority(handle)
                report["priority"] = {"ok": bool(priority), "value": priority or None,
                                      "winerror": 0 if priority else ctypes.get_last_error()}
        finally:
            close_handle(handle)
    report["accessible"] = all(report.get(name, {}).get("ok", False)
                               for name in ("query", "control", "affinity", "priority"))
    return report


def inspect_host(system_id: str | None = None, vm_pid: int | None = None) -> dict:
    if os.name != "nt":
        raise RuntimeError("HCS host controls require Windows")
    hcs = ctypes.WinDLL("computecore.dll")
    free = bind(ctypes.WinDLL("ole32.dll"), "CoTaskMemFree", None, ctypes.c_void_p)
    query = bind(hcs, "HcsGetServiceProperties", ctypes.c_int32,
                 ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_void_p))
    report = {"schema": "wsl-host-preflight-v1", "qualified": False,
              "load_started": False, "linux_ci_enabled": False, "queries": {}}
    for property_type in ("CpuGroup", "ProcessorTopology", "QoSCapabilities"):
        output = ctypes.c_void_p()
        result = query(json.dumps({"PropertyTypes": [property_type]}), ctypes.byref(output))
        try:
            text = ctypes.wstring_at(output) if output.value else ""
            try:
                payload = json.loads(text) if text else None
            except json.JSONDecodeError:
                payload = {"message": text}
            report["queries"][property_type] = {"hresult": f"0x{result & 0xffffffff:08x}",
                                                "ok": result >= 0, "response": payload}
        finally:
            if output.value:
                free(output)
    if system_id:
        system_id = str(UUID(system_id))
        open_system = bind(hcs, "HcsOpenComputeSystem", ctypes.c_int32, ctypes.c_wchar_p,
                           ctypes.c_uint32, ctypes.POINTER(ctypes.c_void_p))
        close_system = bind(hcs, "HcsCloseComputeSystem", None, ctypes.c_void_p)
        handle = ctypes.c_void_p()
        # The documented requestedAccess is GENERIC_ALL. Opening and closing
        # a handle performs no VM modification or guest execution.
        result = open_system(system_id, 0x10000000, ctypes.byref(handle))
        report["vm_access"] = {"system_id": system_id, "hresult": f"0x{result & 0xffffffff:08x}",
                               "ok": result >= 0}
        if handle.value:
            close_system(handle)
    hcs_accessible = all(item["ok"] for item in report["queries"].values())
    hcs_accessible = hcs_accessible and bool(system_id and report["vm_access"]["ok"])
    report["hcs_accessible"] = hcs_accessible
    if vm_pid is not None:
        report["vm_process"] = inspect_vm_process(vm_pid)
    accessible = hcs_accessible or report.get("vm_process", {}).get("accessible", False)
    report["preflight_accessible"] = accessible
    access_denied = any(item["hresult"] in ("0x8037011b", "0x80070005")
                        for item in report["queries"].values())
    access_denied |= report.get("vm_access", {}).get("hresult") in ("0x8037011b", "0x80070005")
    access_denied |= any(report.get("vm_process", {}).get(name, {}).get("winerror") == 5
                         for name in ("query", "control", "affinity", "priority"))
    if accessible:
        report["next_step"] = ("Verify host CPU placement and host priority under the actual scheduler; "
                               "query access alone is not qualification.")
    elif access_denied:
        report["next_step"] = "Use an elevated Windows execution path to complete the denied host-control preflight."
    else:
        report["next_step"] = "Inspect the recorded host/VM failure; control access has not been established."
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--system-id", help="Known WSL VM GUID; only open/close access is checked")
    parser.add_argument("--vmmem-pid", type=int, help="Known vmmemWSL PID; only query/control handle access is checked")
    parser.add_argument("--output", type=Path, help="Save the read-only preflight result as JSON")
    args = parser.parse_args()
    try:
        if args.vmmem_pid is not None and not 0 < args.vmmem_pid <= 0xffffffff:
            raise ValueError("vmmem PID must be a positive Windows process ID")
        report = inspect_host(args.system_id, args.vmmem_pid)
    except (OSError, RuntimeError, ValueError) as error:
        report = {"schema": "wsl-host-preflight-v1", "qualified": False, "load_started": False,
                  "linux_ci_enabled": False, "preflight_accessible": False, "error": str(error)}
    encoded = json.dumps(report, indent=2)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded + "\n", encoding="utf-8")
    print(encoded)
    return 0 if report["preflight_accessible"] else 6


if __name__ == "__main__":
    sys.exit(main())
