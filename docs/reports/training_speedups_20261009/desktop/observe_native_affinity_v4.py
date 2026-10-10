"""Read-only owned native identity/affinity observation. Never changes or kills."""
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import time

def observe(root,request,request_pin):
    started=Path(root)/'started.json';deadline=time.monotonic()+60
    while not started.exists():
        if (Path(root)/'execution.json').exists() or time.monotonic()>=deadline:raise ValueError('native ended or did not publish started receipt before affinity observation')
        time.sleep(.25)
    row=json.loads(started.read_bytes());pid=row['pid']
    if row['request']!=request_pin or row['placement']!=request['placement']:raise ValueError('started receipt belongs to another request/placement')
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    kernel.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD];kernel.OpenProcess.restype=wintypes.HANDLE
    kernel.CloseHandle.argtypes=[wintypes.HANDLE]
    kernel.GetProcessAffinityMask.argtypes=[wintypes.HANDLE,ctypes.POINTER(ctypes.c_size_t),ctypes.POINTER(ctypes.c_size_t)]
    kernel.GetProcessTimes.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4
    kernel.QueryFullProcessImageNameW.argtypes=[wintypes.HANDLE,wintypes.DWORD,wintypes.LPWSTR,ctypes.POINTER(wintypes.DWORD)]
    handle=kernel.OpenProcess(0x1400,False,pid)
    if not handle:raise ValueError('cannot observe started native process')
    try:
        times=[wintypes.FILETIME() for _ in range(4)]
        if not kernel.GetProcessTimes(handle,*[ctypes.byref(x) for x in times]):raise OSError(ctypes.get_last_error())
        creation=(times[0].dwHighDateTime<<32)+times[0].dwLowDateTime
        unix=creation/1e7-11644473600
        if not 0<=row['started_unix']-unix<=15:raise ValueError('started native process identity timestamp differs')
        runtime=json.loads(Path(request['runtime']['path']).read_bytes());expected=Path(runtime['binary']['path']).resolve()
        buffer=ctypes.create_unicode_buffer(32768);length=wintypes.DWORD(len(buffer))
        if not kernel.QueryFullProcessImageNameW(handle,0,buffer,ctypes.byref(length)):raise OSError(ctypes.get_last_error())
        if Path(buffer.value).resolve()!=expected:raise ValueError('started process image differs from pinned native runtime')
        process=ctypes.c_size_t();system=ctypes.c_size_t()
        if not kernel.GetProcessAffinityMask(handle,ctypes.byref(process),ctypes.byref(system)):raise OSError(ctypes.get_last_error())
        mask=sum(1<<x for x in request['placement']['cpu_affinity'])
        if process.value!=mask or mask!=21845:raise ValueError('native actual affinity differs from frozen21845: '+str(process.value))
        return dict(pid=pid,creation_time=creation,actual_mask=process.value,expected_mask=mask,process_image=str(expected),verified=True,scope='Read-only initial native affinity; no affinity mutation')
    finally:kernel.CloseHandle(handle)
