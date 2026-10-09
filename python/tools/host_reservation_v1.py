"""Atomic host reservation for supported Windows and Linux launchers (v1).

One reservation per work host, held in one canonical lock file on the machine
that does the work: C:/mtg-node/host-lock/<HOST>.lock, the same for every user,
worktree and transport (a lock on the primary desktop reserves nothing on the compute host; a
compute host reservation is taken by running this module on the compute host). Windows
and Linux, standard library only (Win32 calls through ctypes; /proc on Linux).
The contract below is stated for Windows; the Linux backend at the end of
these notes keeps its CLI, files and rules with weaker containment.

Contract (collab GOALS/opus-search-opponent-20260927.md, amendment 01:35):
1. Target-host authority: the lock path above; HOST is this machine's name.
   The root can move only for tests (MTG_HOST_RESERVATION_TEST_ROOT).
2. Acquire: the record is written to a temp file created with O_CREAT|O_EXCL
   and renamed into place; the rename fails if a lock exists, so exactly one
   acquirer wins and a second fails closed, printing the holder. The record:
   token (unique per reservation), generation, lane, host, work_id,
   owner_pid, owner_process_creation_time, boot_id, acquired_at,
   transport_record, release_condition. The lock file is never rewritten.
3. Spawn handoff: acquire before WMI creation; right after creation the
   caller records the created process (handoff). The supervisor also records
   itself when it starts (adopt), so a lost creation response still leaves a
   lock that names the work; the caller then notes spawn-unconfirmed and does
   not dispatch again until status shows the work terminal. A supervisor
   whose token no longer holds the lock refuses to run its work.
4. Durable lifetime and containment: the target-side supervisor (the
   WMI-created `supervise` process) owns the lock's lifetime. Before it
   adopts, it puts itself in a new job object that kills every member when
   its last handle closes and allows no breakaway, so every descendant it
   ever creates, through any chain of exited intermediates, stays in that
   job and dies with it. Its own job is created by the supervisor, not
   inherited from the caller (WMI-created processes are not assumed to share
   it). The supervisor releases only when its job holds no process but
   itself; if the supervisor dies, the kernel ends the whole job, so a
   supervisor absent by pid and creation time proves its work absent.
5. Release (owner only: the token, and no live recorded work other than the
   caller): success and failure by the supervisor once its job is empty;
   cancelled by a token holder once no recorded work is live; spawn-failed
   after a definite creation or containment failure. Reboot: a changed boot
   id is positive evidence for reclaim. Reclaim is the recovery authority.
6. Reclaim renames the old lock to a dated reclaim record (a second reclaimer
   finds it gone or held by the winner) and acquires a new generation. It
   needs positive evidence: a changed boot id, or every recorded process
   (supervisors, their direct children, nested supervisors) absent by pid
   and creation time. When Windows refuses to open a recorded pid (a service
   or another user's process now holds it), the system process list decides:
   a pid missing from it, or listed with another creation time, is absent.
   The process-tree walk is an extra conservative check, never proof of
   absence. An unreadable or incomplete record or event means unknown, never
   dead; age never counts.
7. Coverage: native tests, throughput checks, builds during announced quiet
   windows and formal runs acquire; status never acquires. Nested wrappers
   share the reservation through MTG_HOST_RESERVATION_TOKEN; a nested
   dispatch creates `supervise --nested` through WMI with the token on its
   command line (WMI passes no environment), which contains and adopts itself
   like the outer supervisor and holds the outer lock until it ends, but
   never releases it.
8. Ordering in every launcher: acquire, then its own Get-CimInstance busy
   refusal (exempting `exempt-pids` of its token), then spawn.

Every state change (adopt, handoff, descendant, note, release, reclaim)
holds the lock file open with share mode read-only, so no other change can
rename it meanwhile; status reads share everything and never block a change.
Events are files under <HOST>.events/<token>/, each written to a temp name
and renamed into place. Released and reclaimed locks stay in the root as
<HOST>.gNNNNNNNN.<token>.<fate>.json.

CLI (JSON on stdout; exit 0 ok, 3 held, 4 reclaimable, 5 unknown, 6 refused):
  acquire --lane L --work-id W --release-condition TEXT [--transport-record JSON] [--owner-pid PID]
  handoff --token T --pid PID          (the created supervisor only; adoption is the
                                        supervisor's own act, after it contains itself)
  note --token T --kind KIND [--detail TEXT]
  release --token T --outcome success|failure|cancelled|spawn-failed [--detail TEXT]
  reclaim --lane L --work-id W --release-condition TEXT [--transport-record JSON] [--owner-pid PID]
  status [--token T] [--line]          exempt-pids --token T
  supervise --token T [--nested] [--cwd DIR] -- COMMAND...

Linux backend (same CLI, records, events, exit codes and rules):
- Root: $MTG_HOST_LOCK_ROOT, else /var/lib/mtg-node/host-lock (the test
  root still wins). Keep it on a local filesystem with hard links and flock.
- Identity: pid plus start time, /proc/<pid>/stat field 22 (clock ticks
  after boot); an exited, unreaped (zombie) process is absent. A pid with no
  /proc entry is absent only if kill(pid, 0) also finds none. Boot id:
  /proc/sys/kernel/random/boot_id.
- Acquire links the complete temp file to the lock name (link fails if the
  name exists; POSIX rename would replace it). State changes hold an
  exclusive flock on the lock file in place of the read-only share mode; the
  flock binds only processes that use this module.
- Spawn: a detached process in a new session (stdin, stdout and stderr on
  /dev/null, the caller's environment without the token) in place of WMI.
  Busy refusal: image names (the executable's file name) from /proc.
- Containment: the supervisor becomes a child subreaper and starts the work
  in a new session. Its job is that session and process group plus every
  live descendant of the supervisor (orphans are reparented to it, also
  after setsid). It releases once the job is empty; SIGTERM, SIGINT, SIGHUP
  or its own exit kills the job. Weaker than a job object: SIGKILL of the
  supervisor kills nothing. Session members then keep the reservation held
  (session ids are never reused while a member lives), but a descendant that
  called setsid itself is no longer seen, so absence of the recorded
  processes is weaker evidence than on Windows.
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
import sys
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path

if os.name == "nt":
    import ctypes
    import struct
    import winreg
    from ctypes import wintypes
elif sys.platform.startswith("linux"):
    import atexit
    import collections
    import ctypes
    import fcntl
    import shlex
    import signal
else:
    raise SystemExit("host_reservation_v1 supports Windows and Linux only")

SCHEMA = "mtg-host-reservation/v1"
EVENT_SCHEMA = "mtg-host-reservation-event/v1"
CANONICAL_ROOT = "C:/mtg-node/host-lock" if os.name == "nt" else "/var/lib/mtg-node/host-lock"
ROOT_ENV = "MTG_HOST_LOCK_ROOT"  # Linux only: moves the canonical root (a rented host's disk layout)
TEST_ROOT_ENV = "MTG_HOST_RESERVATION_TEST_ROOT"
TOKEN_ENV = "MTG_HOST_RESERVATION_TOKEN"
OUTCOMES = ("success", "failure", "cancelled", "spawn-failed")
EXIT_OK, EXIT_USAGE, EXIT_HELD, EXIT_RECLAIMABLE, EXIT_UNKNOWN, EXIT_REFUSED = 0, 2, 3, 4, 5, 6
TRANSITION_WAIT_SECONDS = 20.0


class Held(Exception):
    """The host is reserved by someone else."""

    def __init__(self, message: str, holder: dict | None):
        super().__init__(message)
        self.holder = holder


class Refused(Exception):
    """A release, reclaim, handoff or adopt was refused."""


# ---------------------------------------------------------------- Win32 layer

if os.name == "nt":
    _k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    HANDLE = wintypes.HANDLE
    INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
    PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    SYNCHRONIZE = 0x00100000
    PROCESS_TERMINATE = 0x0001
    WAIT_OBJECT_0, WAIT_TIMEOUT = 0x0, 0x102
    ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND = 2, 3
    ERROR_ACCESS_DENIED, ERROR_SHARING_VIOLATION, ERROR_INVALID_PARAMETER = 5, 32, 87
    GENERIC_READ, DELETE = 0x80000000, 0x00010000
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_SHARE_DELETE = 1, 2, 4
    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL = 3, 0x80
    FILE_RENAME_INFO_CLASS = 3
    TH32CS_SNAPPROCESS = 0x2
    JOB_OBJECT_BASIC_PROCESS_ID_LIST = 3
    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000


    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", wintypes.DWORD),
            ("cntUsage", wintypes.DWORD),
            ("th32ProcessID", wintypes.DWORD),
            ("th32DefaultHeapID", ctypes.c_size_t),
            ("th32ModuleID", wintypes.DWORD),
            ("cntThreads", wintypes.DWORD),
            ("th32ParentProcessID", wintypes.DWORD),
            ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", wintypes.DWORD),
            ("szExeFile", ctypes.c_wchar * 260),
        ]


    class FILE_RENAME_INFO(ctypes.Structure):
        _fields_ = [
            ("ReplaceIfExists", wintypes.BOOLEAN),
            ("RootDirectory", HANDLE),
            ("FileNameLength", wintypes.DWORD),
            ("FileName", wintypes.WCHAR * 1),
        ]


    class JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", wintypes.DWORD),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", wintypes.DWORD),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", wintypes.DWORD),
            ("SchedulingClass", wintypes.DWORD),
        ]


    class IO_COUNTERS(ctypes.Structure):
        _fields_ = [(name, ctypes.c_ulonglong) for name in (
            "ReadOperationCount", "WriteOperationCount", "OtherOperationCount",
            "ReadTransferCount", "WriteTransferCount", "OtherTransferCount")]


    class JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", JOBOBJECT_BASIC_LIMIT_INFORMATION),
            ("IoInfo", IO_COUNTERS),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]


    def _fn(name, restype, *argtypes):
        f = getattr(_k32, name)
        f.restype, f.argtypes = restype, argtypes
        return f


    _FT = ctypes.POINTER(wintypes.FILETIME)
    _OpenProcess = _fn("OpenProcess", HANDLE, wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    _GetProcessTimes = _fn("GetProcessTimes", wintypes.BOOL, HANDLE, _FT, _FT, _FT, _FT)
    _WaitForSingleObject = _fn("WaitForSingleObject", wintypes.DWORD, HANDLE, wintypes.DWORD)
    _TerminateProcess = _fn("TerminateProcess", wintypes.BOOL, HANDLE, wintypes.UINT)
    _CloseHandle = _fn("CloseHandle", wintypes.BOOL, HANDLE)
    _GetCurrentProcess = _fn("GetCurrentProcess", HANDLE)
    _CreateToolhelp32Snapshot = _fn("CreateToolhelp32Snapshot", HANDLE, wintypes.DWORD, wintypes.DWORD)
    _Process32FirstW = _fn("Process32FirstW", wintypes.BOOL, HANDLE, ctypes.POINTER(PROCESSENTRY32W))
    _Process32NextW = _fn("Process32NextW", wintypes.BOOL, HANDLE, ctypes.POINTER(PROCESSENTRY32W))
    _CreateFileW = _fn("CreateFileW", HANDLE, wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                       wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, HANDLE)
    _ReadFile = _fn("ReadFile", wintypes.BOOL, HANDLE, wintypes.LPVOID, wintypes.DWORD,
                    ctypes.POINTER(wintypes.DWORD), wintypes.LPVOID)
    _SetFileInformationByHandle = _fn("SetFileInformationByHandle", wintypes.BOOL, HANDLE, ctypes.c_int,
                                      wintypes.LPVOID, wintypes.DWORD)
    _CreateJobObjectW = _fn("CreateJobObjectW", HANDLE, wintypes.LPVOID, wintypes.LPCWSTR)
    _SetInformationJobObject = _fn("SetInformationJobObject", wintypes.BOOL, HANDLE, ctypes.c_int,
                                   wintypes.LPVOID, wintypes.DWORD)
    _AssignProcessToJobObject = _fn("AssignProcessToJobObject", wintypes.BOOL, HANDLE, HANDLE)
    _QueryInformationJobObject = _fn("QueryInformationJobObject", wintypes.BOOL, HANDLE, ctypes.c_int,
                                     wintypes.LPVOID, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD))


    def _filetime(ft: wintypes.FILETIME) -> int:
        return (ft.dwHighDateTime << 32) | ft.dwLowDateTime


    def _creation_of(handle) -> int | None:
        c, e, k, u = (wintypes.FILETIME() for _ in range(4))
        if not _GetProcessTimes(handle, ctypes.byref(c), ctypes.byref(e), ctypes.byref(k), ctypes.byref(u)):
            return None
        return _filetime(c)


    def self_identity() -> tuple[int, int]:
        """This process's pid and creation time (FILETIME, 100 ns since 1601)."""
        creation = _creation_of(_GetCurrentProcess())
        if creation is None:
            raise OSError(ctypes.get_last_error(), "GetProcessTimes failed on the current process")
        return os.getpid(), creation


    _ntdll = ctypes.WinDLL("ntdll")
    _NtQuerySystemInformation = _ntdll.NtQuerySystemInformation
    _NtQuerySystemInformation.restype = ctypes.c_long
    _NtQuerySystemInformation.argtypes = (ctypes.c_ulong, wintypes.LPVOID, ctypes.c_ulong,
                                          ctypes.POINTER(ctypes.c_ulong))
    SYSTEM_PROCESS_INFORMATION = 5
    STATUS_INFO_LENGTH_MISMATCH = ctypes.c_long(0xC0000004).value


    def system_creation_times() -> dict[int, int] | None:
        """{pid: creation time} of every live process, from the system process
        list (NtQuerySystemInformation, SystemProcessInformation). It opens no
        process, so it also covers processes OpenProcess refuses (services,
        other users). None if the query fails or the layout is unknown."""
        if ctypes.sizeof(ctypes.c_void_p) != 8:
            return None  # the offsets below are the 64-bit layout
        size = 1 << 20
        for _ in range(8):
            buf = ctypes.create_string_buffer(size)
            needed = ctypes.c_ulong(0)
            status = _NtQuerySystemInformation(SYSTEM_PROCESS_INFORMATION, buf, size, ctypes.byref(needed))
            if status == STATUS_INFO_LENGTH_MISMATCH:
                size = max(2 * size, needed.value + (1 << 16))
                continue
            if status != 0:
                return None
            out, offset = {}, 0
            while True:
                (next_entry,) = struct.unpack_from("<I", buf, offset)
                (created,) = struct.unpack_from("<q", buf, offset + 0x20)  # CreateTime
                (pid,) = struct.unpack_from("<Q", buf, offset + 0x50)  # UniqueProcessId
                out[pid] = created
                if not next_entry:
                    return out
                offset += next_entry
        return None


    def _listed_state(pid: int, creation: int) -> str:
        """State of a process OpenProcess refuses, from the system process list:
        a live process is always listed, so a pid missing from the list or listed
        with another creation time is absent; a match is alive."""
        listed = system_creation_times()
        if listed is None:
            return "unknown"
        return "alive" if listed.get(pid) == creation else "absent"


    def creation_time(pid: int) -> int | None:
        """Creation time of the process now holding this pid, or None if absent or unreadable."""
        handle = _OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
        if not handle:
            if ctypes.get_last_error() == ERROR_INVALID_PARAMETER:
                return None
            return (system_creation_times() or {}).get(pid)
        try:
            return _creation_of(handle)
        finally:
            _CloseHandle(handle)


    def process_state(pid, creation) -> str:
        """'alive', 'absent' or 'unknown' for the process named by pid and creation time.
        Absent needs positive evidence: no such pid, the pid names a process created
        at another time, or the process has exited. When OpenProcess refuses the pid,
        the system process list decides (see _listed_state). Anything else is unknown."""
        if not isinstance(pid, int) or not isinstance(creation, int) or pid <= 0 or creation <= 0:
            return "unknown"
        handle = _OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, False, pid)
        if not handle:
            return "absent" if ctypes.get_last_error() == ERROR_INVALID_PARAMETER else _listed_state(pid, creation)
        try:
            actual = _creation_of(handle)
            if actual is None:
                return _listed_state(pid, creation)
            if actual != creation:
                return "absent"
            wait = _WaitForSingleObject(handle, 0)
            return {WAIT_OBJECT_0: "absent", WAIT_TIMEOUT: "alive"}.get(wait, "unknown")
        finally:
            _CloseHandle(handle)


    def terminate(pid: int, creation: int) -> bool:
        """Terminate the process named by pid and creation time (tests and cancellation)."""
        handle = _OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE, False, pid)
        if not handle:
            return False
        try:
            return _creation_of(handle) == creation and bool(_TerminateProcess(handle, 1))
        finally:
            _CloseHandle(handle)


    def process_table() -> list[tuple[int, int, str]]:
        """(pid, parent pid, image name) of every process (Toolhelp snapshot)."""
        snap = _CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
        if snap == INVALID_HANDLE_VALUE or not snap:
            raise OSError(ctypes.get_last_error(), "CreateToolhelp32Snapshot failed")
        try:
            entry = PROCESSENTRY32W()
            entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
            rows = []
            ok = _Process32FirstW(snap, ctypes.byref(entry))
            while ok:
                rows.append((int(entry.th32ProcessID), int(entry.th32ParentProcessID), entry.szExeFile))
                ok = _Process32NextW(snap, ctypes.byref(entry))
            return rows
        finally:
            _CloseHandle(snap)


    def live_descendants(roots, exclude=()) -> list[dict]:
        """Live processes descended from any (pid, creation) root, also through
        exited intermediates (an orphan keeps its dead parent's pid). A child
        counts only if it was created after its parent; if the root's pid now
        names a newer process, children created after that newer process are its
        own. A child whose creation time cannot be read counts (unknown is live)."""
        children: dict[int, list[tuple[int, str]]] = {}
        for pid, parent, image in process_table():
            if pid != parent:
                children.setdefault(parent, []).append((pid, image))
        found, seen = [], set(exclude)
        frontier = [(pid, creation) for pid, creation in roots if isinstance(pid, int) and isinstance(creation, int)]
        while frontier:
            pid, creation = frontier.pop()
            holder = creation_time(pid)
            newer = holder if holder is not None and holder != creation else None
            for child, image in children.get(pid, []):
                if child in seen:
                    continue
                born = creation_time(child)
                if born is not None and (born < creation or (newer is not None and born >= newer)):
                    continue
                seen.add(child)
                found.append({"pid": child, "image": image, "creation_time": born})
                if born is not None:
                    frontier.append((child, born))
        return found


    def contain_self():
        """Put this process in a new job that kills every member when its last
        handle closes and allows no breakaway: every descendant created from now
        on stays in the job (through exited intermediates too) and dies with
        this process. The handle is not inheritable, so only this process keeps
        the job alive. Returns the job handle."""
        job = _CreateJobObjectW(None, None)
        if not job:
            raise OSError(ctypes.get_last_error(), "CreateJobObjectW failed")
        info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION()
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if not _SetInformationJobObject(job, JOB_OBJECT_EXTENDED_LIMIT_INFORMATION, ctypes.byref(info),
                                        ctypes.sizeof(info)):
            err = ctypes.get_last_error()
            _CloseHandle(job)
            raise OSError(err, "SetInformationJobObject failed")
        if not _AssignProcessToJobObject(job, _GetCurrentProcess()):
            err = ctypes.get_last_error()
            _CloseHandle(job)
            raise OSError(err, "AssignProcessToJobObject failed")
        return job


    def job_members(job=None) -> list[int]:
        """Pids of the live processes in a job (None: the caller's own job)."""
        capacity = 8192
        size = 8 + 8 * capacity
        buf = ctypes.create_string_buffer(size)
        if not _QueryInformationJobObject(job, JOB_OBJECT_BASIC_PROCESS_ID_LIST, buf, size, None):
            raise OSError(ctypes.get_last_error(), "QueryInformationJobObject failed")
        _, listed = struct.unpack_from("<II", buf, 0)
        return list(struct.unpack_from(f"<{listed}Q", buf, 8))


    def boot_id() -> str:
        """Windows' boot counter (Memory Management PrefetchParameters BootId)."""
        key = r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management\PrefetchParameters"
        try:
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key) as k:
                value, _ = winreg.QueryValueEx(k, "BootId")
            return f"bootid:{int(value)}"
        except OSError:
            return "unavailable"


    class _Handle:
        """A Win32 file handle; transition handles share read only, so while one is
        open no other process can rename, delete or write the file."""

        def __init__(self, path: Path, transition: bool):
            self.path = path
            access = GENERIC_READ | (DELETE if transition else 0)
            share = FILE_SHARE_READ if transition else FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
            deadline = time.monotonic() + (TRANSITION_WAIT_SECONDS if transition else 5.0)
            while True:
                h = _CreateFileW(str(path), access, share, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None)
                if h != INVALID_HANDLE_VALUE and h:
                    self.handle = h
                    return
                err = ctypes.get_last_error()
                if err in (ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND):
                    raise FileNotFoundError(str(path))
                if err not in (ERROR_SHARING_VIOLATION, ERROR_ACCESS_DENIED) or time.monotonic() > deadline:
                    raise OSError(err, f"cannot open {path} (Win32 error {err})")
                time.sleep(0.05)

        def read(self) -> bytes:
            chunks, buf, got = [], ctypes.create_string_buffer(65536), wintypes.DWORD()
            while True:
                if not _ReadFile(self.handle, buf, len(buf), ctypes.byref(got), None):
                    raise OSError(ctypes.get_last_error(), f"ReadFile failed on {self.path}")
                if got.value == 0:
                    return b"".join(chunks)
                chunks.append(buf.raw[: got.value])

        def rename(self, new_name: str) -> None:
            """Rename this exact file within its directory (fails if the name exists).
            The target is a full path: a bare name resolves against the current
            directory, not the file's."""
            target = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(self.path)), new_name))
            encoded = target.encode("utf-16-le")
            size = ctypes.sizeof(FILE_RENAME_INFO) + len(encoded)
            buf = ctypes.create_string_buffer(size)
            info = ctypes.cast(buf, ctypes.POINTER(FILE_RENAME_INFO)).contents
            info.ReplaceIfExists = False
            info.RootDirectory = None
            info.FileNameLength = len(encoded)
            ctypes.memmove(ctypes.addressof(info) + FILE_RENAME_INFO.FileName.offset, encoded, len(encoded))
            if not _SetFileInformationByHandle(self.handle, FILE_RENAME_INFO_CLASS, buf, size):
                raise OSError(ctypes.get_last_error(), f"rename of {self.path} to {new_name} failed")

        def close(self) -> None:
            if self.handle:
                _CloseHandle(self.handle)
                self.handle = None

        def __enter__(self):
            return self

        def __exit__(self, *_):
            self.close()

    _publish = os.rename  # fails if the lock exists
    command_line = subprocess.list2cmdline

    def _child_creation(child) -> int | None:
        """The work child's creation time. The Popen handle keeps the process
        object, so it is readable even if the child has already exited."""
        return _creation_of(HANDLE(int(child._handle)))

    def _spawn_contained(job, command, cwd, env):
        """Start the work; it is created in the supervisor's job."""
        return subprocess.Popen(command, cwd=cwd, env=env)

else:
    # ------------------------------------------------------------ Linux layer
    # A process is (pid, start time from /proc/<pid>/stat field 22). State
    # changes take an exclusive flock where Windows opens with a read-only
    # share mode; the work runs in its own session, watched by a subreaper
    # supervisor, where Windows uses a job object (see the module notes).

    EXITED = ("Z", "X", "x")  # zombie or dead: the process has exited
    STOP_SIGNALS = (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)
    PR_SET_CHILD_SUBREAPER = 36
    BOOT_ID_PATH = "/proc/sys/kernel/random/boot_id"
    TERMINATE_GRACE_SECONDS = 10.0
    KILL_WAIT_SECONDS = 5.0
    _Proc = collections.namedtuple("_Proc", "pid state ppid pgrp session start comm")
    _detached: list = []  # created supervisors, kept so exited ones are reaped

    def _stat(pid: int) -> _Proc:
        """One /proc/<pid>/stat row. FileNotFoundError or ProcessLookupError when
        no such pid is visible; ValueError or IndexError when malformed."""
        text = Path(f"/proc/{pid}/stat").read_bytes().decode("utf-8", "replace")
        head, _, tail = text.rpartition(")")  # comm may itself contain ")"
        if not head:
            raise ValueError(f"malformed /proc/{pid}/stat")
        f = tail.split()
        return _Proc(pid, f[0], int(f[1]), int(f[2]), int(f[3]), int(f[19]), head.partition("(")[2])

    def _rows(exited: bool = False) -> list:
        """Every visible process; exited (unreaped) ones only if asked."""
        rows = []
        for name in os.listdir("/proc"):
            if name.isdigit():
                try:
                    row = _stat(int(name))
                except (OSError, ValueError, IndexError):
                    continue  # gone since the listing, or unreadable
                if exited or row.state not in EXITED:
                    rows.append(row)
        return rows

    def self_identity() -> tuple[int, int]:
        """This process's pid and start time (clock ticks after boot)."""
        return os.getpid(), _stat(os.getpid()).start

    def creation_time(pid: int) -> int | None:
        """Start time of the process now holding this pid (also an exited,
        unreaped one), or None if absent or unreadable."""
        try:
            return _stat(pid).start
        except (OSError, ValueError, IndexError):
            return None

    def process_state(pid, creation) -> str:
        """'alive', 'absent' or 'unknown', by the same rules as on Windows: absent
        needs no such pid, another start time, or an exited process."""
        if not isinstance(pid, int) or not isinstance(creation, int) or pid <= 0 or creation <= 0:
            return "unknown"
        try:
            row = _stat(pid)
        except (FileNotFoundError, ProcessLookupError):
            try:
                os.kill(pid, 0)  # /proc can hide a live pid (hidepid); the kernel cannot
            except ProcessLookupError:
                return "absent"
            except (OSError, OverflowError):
                return "unknown"
            return "unknown"
        except (OSError, ValueError, IndexError):
            return "unknown"
        if row.start != creation or row.state in EXITED:
            return "absent"
        return "alive"

    def terminate(pid: int, creation: int) -> bool:
        """Stop the process named by pid and start time (tests and cancellation):
        SIGTERM, so a supervisor kills its job first, then SIGKILL after
        TERMINATE_GRACE_SECONDS. False if that process is not alive."""
        if process_state(pid, creation) != "alive":
            return False
        try:
            os.kill(pid, signal.SIGTERM)
        except OSError:
            return False
        deadline = time.monotonic() + TERMINATE_GRACE_SECONDS
        while process_state(pid, creation) == "alive":
            if time.monotonic() > deadline:
                try:
                    os.kill(pid, signal.SIGKILL)
                except OSError:
                    pass
                break
            time.sleep(0.05)
        return True

    def live_descendants(roots, exclude=()) -> list[dict]:
        """Live processes descended from any (pid, creation) root, and every live
        member of a session or process group such a process leads. Linux
        reparents an orphan, so after an intermediate exits only its session
        still names it (unless it called setsid). A session or group id is never
        reused while a member lives. A process counts only if it started no
        earlier than its root; if the root's pid now names a newer process,
        processes started after that one are its own."""
        rows = {r.pid: r for r in _rows()}
        linked: dict[int, set[int]] = {}
        for r in rows.values():
            for key in (r.ppid, r.session, r.pgrp):
                linked.setdefault(key, set()).add(r.pid)
        found, seen = [], set(exclude)
        frontier = [(pid, creation) for pid, creation in roots if isinstance(pid, int) and isinstance(creation, int)]
        while frontier:
            pid, creation = frontier.pop()
            holder = creation_time(pid)
            newer = holder if holder is not None and holder != creation else None
            for child in sorted(linked.get(pid, ())):
                if child == pid or child in seen:
                    continue
                born = rows[child].start
                if born < creation or (newer is not None and born >= newer):
                    continue
                seen.add(child)
                found.append({"pid": child, "image": rows[child].comm, "creation_time": born})
                frontier.append((child, born))
        return found

    class _SessionJob:
        """The supervisor's job on Linux: the work's session and process group
        (named by the work child's pid) and every live descendant of the
        supervisor, which is a child subreaper."""

        def __init__(self):
            self.sid = self.start = None

    def contain_self():
        """Make this process a child subreaper, so every orphaned descendant
        (also one that called setsid) is reparented to it while it lives, and
        return its job; the work starts in a new session (_spawn_contained).
        Weaker than a job object: nothing kills the job if this process is
        killed with SIGKILL."""
        try:
            prctl = ctypes.CDLL(None, use_errno=True).prctl
        except (OSError, AttributeError) as exc:
            raise OSError(f"prctl unavailable: {exc}") from None
        if prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_CHILD_SUBREAPER) failed")
        return _SessionJob()

    def _job_rows(job) -> list:
        """Live members of a _SessionJob, never the supervisor itself. Reaps
        the exited orphans it adopted, never the work child (Popen waits)."""
        me = os.getpid()
        live, children = {}, {}
        for r in _rows(exited=True):
            if r.state in EXITED:
                if r.ppid == me and job.sid is not None and r.pid != job.sid:
                    try:
                        os.waitpid(r.pid, os.WNOHANG)
                    except ChildProcessError:
                        pass
                continue
            live[r.pid] = r
            children.setdefault(r.ppid, []).append(r.pid)
        # The work's session id names another session only if its pid now
        # holds a different process (it is not reused while a member lives).
        ours = job.sid is not None and creation_time(job.sid) in (None, job.start)
        members = {p for p, r in live.items() if ours and job.sid in (r.session, r.pgrp)}
        tree, frontier = set(), [me]
        while frontier:
            for child in children.get(frontier.pop(), ()):
                if child not in tree:
                    tree.add(child)
                    frontier.append(child)
        members = (members | tree) - {me}
        return [live[p] for p in sorted(members)]

    def job_members(job=None) -> list[int]:
        """Pids of the live processes in a job (None: the caller's own session,
        which is its supervisor's work session)."""
        if job is None:
            sid = os.getsid(0)
            return sorted(r.pid for r in _rows() if r.session == sid)
        return [r.pid for r in _job_rows(job)]

    def _kill_job(job) -> None:
        """SIGKILL every member of the job, again while members fork, until none
        is live or KILL_WAIT_SECONDS pass."""
        deadline = time.monotonic() + KILL_WAIT_SECONDS
        while True:
            rows = _job_rows(job)
            if not rows or time.monotonic() > deadline:
                return
            if any(r.pgrp == job.sid for r in rows):
                try:
                    os.killpg(job.sid, signal.SIGKILL)
                except OSError:
                    pass
            for r in rows:
                if creation_time(r.pid) == r.start:
                    try:
                        os.kill(r.pid, signal.SIGKILL)
                    except OSError:
                        pass
            time.sleep(0.02)

    def _spawn_contained(job, command, cwd, env):
        """Start the work in a new session, so its session and process group are
        named by its pid. From now on a stop signal (SIGTERM, SIGINT, SIGHUP)
        or this process's exit kills the whole job, as closing a job does."""
        def stop(signum, _frame):
            for each in STOP_SIGNALS:
                signal.signal(each, signal.SIG_IGN)
            _kill_job(job)
            os._exit(128 + signum)

        for each in STOP_SIGNALS:
            signal.signal(each, stop)
        atexit.register(_kill_job, job)
        child = subprocess.Popen(command, cwd=cwd, env=env, start_new_session=True)
        job.sid, job.start = child.pid, creation_time(child.pid)
        return child

    def _child_creation(child) -> int | None:
        """The work child's start time; an exited child stays readable until
        Popen reaps it."""
        return creation_time(child.pid)

    def boot_id() -> str:
        """The kernel's boot id, a random UUID new at every boot."""
        try:
            value = Path(BOOT_ID_PATH).read_text(encoding="ascii").strip()
        except (OSError, UnicodeDecodeError):
            return "unavailable"
        return f"bootid:{value}" if value else "unavailable"

    class _Handle:
        """A lock file opened for reading. A transition handle also holds an
        exclusive flock and is open on the file the path names (checked after
        the flock), so while one is open no other change by this module can
        rename or replace it. Readers take no lock and never block a change."""

        def __init__(self, path: Path, transition: bool):
            self.path, self.handle = path, None
            deadline = time.monotonic() + TRANSITION_WAIT_SECONDS
            while True:
                fd = os.open(path, os.O_RDONLY)  # FileNotFoundError when free
                if not transition:
                    self.handle = fd
                    return
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    if os.path.samestat(os.fstat(fd), os.stat(path)):
                        self.handle = fd
                        return
                except BlockingIOError:
                    pass
                except BaseException:
                    os.close(fd)
                    raise
                os.close(fd)  # held by another change, or renamed meanwhile
                if time.monotonic() > deadline:
                    raise OSError(f"cannot open {path} for a state change: another change holds it")
                time.sleep(0.05)

        def read(self) -> bytes:
            chunks = []
            while True:
                chunk = os.read(self.handle, 65536)
                if not chunk:
                    return b"".join(chunks)
                chunks.append(chunk)

        def rename(self, new_name: str) -> None:
            """Rename this exact file within its directory (fails if the name
            exists). Only a transition handle renames; its flock keeps every other
            change out, so the check and the atomic rename cannot interleave."""
            target = os.path.join(os.path.dirname(os.path.abspath(self.path)), new_name)
            if not os.path.samestat(os.fstat(self.handle), os.stat(self.path)):
                raise OSError(f"{self.path} no longer names the file this handle holds")
            if os.path.lexists(target):
                raise FileExistsError(target)
            os.rename(self.path, target)

        def close(self) -> None:
            if self.handle is not None:
                os.close(self.handle)
                self.handle = None

        def __enter__(self):
            return self

        def __exit__(self, *_):
            self.close()

    def _publish(tmp, target) -> None:
        """Give the complete temp file the lock's name, failing if that name
        exists (os.rename would replace it on POSIX)."""
        os.link(tmp, target)
        os.unlink(tmp)

    command_line = shlex.join

    def spawn_detached(command_line: str, cwd: str, timeout: float = 60.0) -> dict:
        """The Linux wmi_create: start a supervisor_command line as a detached
        process in a new session, so it survives the caller and no signal to the
        caller's terminal or group reaches it. stdin, stdout and stderr are
        /dev/null, as for a hidden WMI process; it inherits the caller's
        environment without the token. A failed creation is definite (the
        errno as return_value), never a lost response."""
        env = dict(os.environ)
        env.pop(TOKEN_ENV, None)
        try:
            proc = subprocess.Popen(shlex.split(command_line), cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        except OSError as exc:
            return {"return_value": exc.errno or 1, "pid": 0}
        _detached[:] = [p for p in _detached if p.poll() is None] + [proc]
        return {"return_value": 0, "pid": proc.pid}

    def _image_name(row) -> str:
        """The executable's file name, as Win32_Process.Name gives it: from
        /proc/<pid>/exe, else argv[0], else the kernel's (truncated) comm."""
        try:
            return os.path.basename(os.readlink(f"/proc/{row.pid}/exe")).removesuffix(" (deleted)")
        except OSError:
            pass
        try:
            argv0 = Path(f"/proc/{row.pid}/cmdline").read_bytes().split(b"\0", 1)[0]
        except OSError:
            argv0 = b""
        return os.path.basename(argv0.decode("utf-8", "replace")) or row.comm

    def proc_busy(pattern: str, exempt: list[int]) -> list[dict]:
        """The Linux cim_busy: live processes whose image name matches the regex
        (case-insensitively, as PowerShell's -match does), other than the
        exempt pids. Rows keep cim_busy's keys."""
        regex, skip, rows = re.compile(pattern, re.IGNORECASE), set(exempt), []
        for row in _rows():
            if row.pid not in skip:
                name = _image_name(row)
                if regex.search(name):
                    rows.append({"ProcessId": row.pid, "Name": name})
        return rows


# ---------------------------------------------------------------- records

HOST = re.sub(r"[^A-Za-z0-9-]", "-", platform.node()).upper()
RECORD_FIELDS = {
    "schema": str, "token": str, "generation": int, "lane": str, "host": str, "work_id": str,
    "owner_pid": int, "owner_process_creation_time": int, "boot_id": str, "acquired_at": str,
    "transport_record": dict, "release_condition": str,
}
FATE_NAME = re.compile(r"^(?P<host>[A-Z0-9-]+)\.g(?P<generation>\d{8})\.(?P<token>[0-9a-f]{32})\.(?P<fate>released|reclaimed-\d{8}T\d{6}Z)\.json$")


def root_dir() -> Path:
    if os.name == "nt":
        return Path(os.environ.get(TEST_ROOT_ENV) or CANONICAL_ROOT)
    return Path(os.environ.get(TEST_ROOT_ENV) or os.environ.get(ROOT_ENV) or CANONICAL_ROOT)


def lock_path() -> Path:
    return root_dir() / f"{HOST}.lock"


def events_dir(token: str) -> Path:
    return root_dir() / f"{HOST}.events" / token


def now_utc() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def _dump(value) -> bytes:
    return (json.dumps(value, indent=1, sort_keys=True) + "\n").encode("utf-8")


def parse_record(body: bytes) -> dict | None:
    """The lock record, or None when it is unreadable or incomplete (unknown)."""
    try:
        record = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return None
    if not isinstance(record, dict) or record.get("schema") != SCHEMA:
        return None
    for field, kind in RECORD_FIELDS.items():
        value = record.get(field)
        if not isinstance(value, kind) or isinstance(value, bool) or (kind is str and not value):
            return None
    if not re.fullmatch(r"[0-9a-f]{32}", record["token"]) or record["generation"] < 1:
        return None
    if record["owner_pid"] <= 0 or record["owner_process_creation_time"] <= 0 or record["host"] != HOST:
        return None
    return record


def read_events(token: str) -> tuple[list[dict], list[str]]:
    """Events of a reservation in order, and the names of unreadable ones."""
    directory = events_dir(token)
    events, unreadable = [], []
    if not directory.is_dir():
        return events, unreadable
    for path in sorted(directory.glob("*.json")):
        try:
            event = json.loads(path.read_text(encoding="utf-8"))
            if not isinstance(event, dict) or event.get("schema") != EVENT_SCHEMA or event.get("token") != token:
                raise ValueError("not an event of this reservation")
            events.append(event)
        except (OSError, ValueError, UnicodeDecodeError):
            unreadable.append(path.name)
    return events, unreadable


def _write_event(token: str, kind: str, **fields) -> dict:
    directory = events_dir(token)
    directory.mkdir(parents=True, exist_ok=True)
    pid, creation = self_identity()
    event = {"schema": EVENT_SCHEMA, "token": token, "kind": kind, "at": now_utc(),
             "by_pid": pid, "by_process_creation_time": creation, **fields}
    stamp = f"{time.time_ns():020d}-{pid}-{kind}"
    tmp = directory / f".{stamp}.tmp"
    fd = os.open(tmp, os.O_CREAT | os.O_EXCL | os.O_WRONLY | getattr(os, "O_BINARY", 0), 0o644)
    with os.fdopen(fd, "wb") as f:
        f.write(_dump(event))
        f.flush()
        os.fsync(f.fileno())
    os.rename(tmp, directory / f"{stamp}.json")
    return event


def _work_roots(events: list[dict]) -> list[tuple[int, int]]:
    """Processes of the recorded work: handed-off and adopted supervisors,
    their direct children, and the job members a supervisor listed."""
    roots = []
    for e in events:
        if e.get("kind") in ("handoff", "adopt", "descendant"):
            roots.append((e.get("pid"), e.get("creation_time")))
        elif e.get("kind") == "members":
            roots.extend((m.get("pid"), m.get("creation_time")) for m in e.get("members", []))
    # handoff-unverified names a pid whose process had already exited; the
    # supervisor's adopt event identifies the work instead.
    return roots


def assess(record: dict | None, events: list[dict], unreadable: list[str], exclude_self: bool = False) -> dict:
    """Liveness of a reservation. dead=True only with positive evidence: every
    recorded process absent by pid and creation time. That covers the whole
    work because each supervisor contains itself (and so every descendant)
    in a kill-on-close job before it adopts or spawns; the process-tree walk
    below can only add refusals, never prove absence."""
    if record is None:
        return {"dead": False, "state": "unknown", "why": "lock record unreadable or incomplete"}
    if unreadable:
        return {"dead": False, "state": "unknown", "why": f"unreadable events {unreadable}"}
    current = boot_id()
    if record["boot_id"].startswith("bootid:") and current.startswith("bootid:") and record["boot_id"] != current:
        return {"dead": True, "state": "reclaimable", "why": f"boot id changed ({record['boot_id']} to {current})"}
    me = self_identity() if exclude_self else None
    owner = (record["owner_pid"], record["owner_process_creation_time"])
    work = _work_roots(events)
    # Until the work is recorded (handoff or adopt) the acquirer owns the
    # reservation; after that the supervisor and the recorded work do.
    roots = [(f"work:{i}", w) for i, w in enumerate(work)] if work else [("owner", owner)]
    states = {}
    for label, (pid, creation) in roots:
        if me is not None and (pid, creation) == me:
            continue
        states[label] = {"pid": pid, "creation_time": creation, "state": process_state(pid, creation)}
    exclude = [me[0]] if me else []
    descendants = live_descendants(work, exclude=exclude)
    alive = [k for k, v in states.items() if v["state"] == "alive"]
    unknown = [k for k, v in states.items() if v["state"] == "unknown"]
    out = {"processes": states, "live_descendants": descendants}
    if alive or descendants:
        return {**out, "dead": False, "state": "held", "why": f"live: {alive} descendants {[d['pid'] for d in descendants]}"}
    if unknown:
        return {**out, "dead": False, "state": "unknown", "why": f"unknown process state: {unknown}"}
    return {**out, "dead": True, "state": "reclaimable",
            "why": "owner and recorded work absent by pid and creation time; no live descendant"}


def _history(fate: str) -> list[dict]:
    rows = []
    for path in root_dir().glob(f"{HOST}.g*.json"):
        m = FATE_NAME.match(path.name)
        if m and (fate is None or m.group("fate").startswith(fate)):
            rows.append({"path": str(path), **m.groupdict()})
    return rows


def _next_generation() -> int:
    top = max((int(r["generation"]) for r in _history(None)), default=0)
    try:
        with _Handle(lock_path(), transition=False) as h:
            record = parse_record(h.read())
        if record:
            top = max(top, record["generation"])
    except FileNotFoundError:
        pass
    return top + 1


def read_lock() -> tuple[dict | None, bytes | None]:
    """Non-acquiring read of the current lock: (record or None, raw bytes or None if free)."""
    try:
        with _Handle(lock_path(), transition=False) as h:
            body = h.read()
    except FileNotFoundError:
        return None, None
    return parse_record(body), body


# ---------------------------------------------------------------- operations


def acquire(lane: str, work_id: str, release_condition: str, transport_record: dict | None = None,
            owner_pid: int | None = None) -> dict:
    """Reserve this host. The owner is the process that keeps the reservation
    until handoff (default: this process). Raises Held with the holder."""
    nested = os.environ.get(TOKEN_ENV)
    if nested:
        record, body = read_lock()
        if record is not None and record["token"] == nested:
            return {**record, "shared": True}
        raise Held(f"{TOKEN_ENV}={nested} does not hold {HOST}", record)
    if not (lane and work_id and release_condition):
        raise ValueError("lane, work_id and release_condition are required")
    if owner_pid is None:
        owner_pid, owner_creation = self_identity()
    else:
        owner_creation = creation_time(owner_pid)
        if owner_creation is None:
            raise ValueError(f"owner pid {owner_pid} is not a readable live process")
    root = root_dir()
    root.mkdir(parents=True, exist_ok=True)
    token = uuid.uuid4().hex
    record = {
        "schema": SCHEMA, "token": token, "generation": _next_generation(), "lane": lane, "host": HOST,
        "work_id": work_id, "owner_pid": owner_pid, "owner_process_creation_time": owner_creation,
        "boot_id": boot_id(), "acquired_at": now_utc(), "transport_record": transport_record or {},
        "release_condition": release_condition,
    }
    tmp = root / f".{HOST}.acquire.{token}.tmp"
    fd = os.open(tmp, os.O_CREAT | os.O_EXCL | os.O_WRONLY | getattr(os, "O_BINARY", 0), 0o644)
    with os.fdopen(fd, "wb") as f:
        f.write(_dump(record))
        f.flush()
        os.fsync(f.fileno())
    try:
        _publish(tmp, lock_path())
    except FileExistsError:
        os.remove(tmp)
        holder, _ = read_lock()
        raise Held(f"{HOST} is reserved", holder) from None
    return record


def _transition(token: str | None):
    """Open the lock for a state change and verify the token (None: any)."""
    handle = _Handle(lock_path(), transition=True)
    try:
        record = parse_record(handle.read())
        if token is not None and (record is None or record["token"] != token):
            raise Refused(f"token {token} does not hold {HOST}")
        return handle, record
    except BaseException:
        handle.close()
        raise


def _change(token: str, kind: str, **fields) -> dict:
    try:
        handle, _ = _transition(token)
    except FileNotFoundError:
        raise Refused(f"no reservation on {HOST}") from None
    with handle:
        return _write_event(token, kind, **fields)


def handoff(token: str, pid: int) -> dict:
    """Record the process the caller created (right after WMI creation). It
    must be a supervisor made by supervisor_command, which contains itself; a
    handoff of an unsupervised process is outside the contract. A process that
    already exited cannot be identified by creation time; it is noted as
    handoff-unverified, and the supervisor's own adopt event names it."""
    creation = creation_time(pid)
    if creation is None:
        return _change(token, "handoff-unverified", pid=pid)
    return _change(token, "handoff", pid=pid, creation_time=creation)


def adopt(token: str, nested: bool = False, contained: bool = False) -> dict:
    """The supervisor records itself; refused if the token no longer holds the lock."""
    pid, creation = self_identity()
    return _change(token, "adopt", pid=pid, creation_time=creation, nested=nested, contained=contained)


def record_descendant(token: str, pid: int, creation: int | None = None) -> dict:
    creation = creation if creation is not None else creation_time(pid)
    if creation is None:
        raise Refused(f"descendant {pid} is not readable; it cannot be recorded by creation time")
    return _change(token, "descendant", pid=pid, creation_time=creation)


def note(token: str, kind: str, detail: str = "") -> dict:
    if kind in ("handoff", "handoff-unverified", "adopt", "descendant", "members", "nested-done", "release", "reclaim"):
        raise ValueError(f"{kind} is not a note")
    return _change(token, kind, detail=detail)


def release(token: str, outcome: str, detail: str = "") -> dict:
    """Owner-only release: the token must hold the lock and no recorded work
    other than the caller may be live (a surviving descendant keeps it held)."""
    if outcome not in OUTCOMES:
        raise ValueError(f"outcome must be one of {OUTCOMES}")
    try:
        handle, record = _transition(token)
    except FileNotFoundError:
        raise Refused(f"no reservation on {HOST}") from None
    with handle:
        events, unreadable = read_events(token)
        me = self_identity()
        work = [w for w in _work_roots(events) if w != me]
        live = [w for w in work if process_state(*w) != "absent"]
        descendants = live_descendants(work, exclude=[me[0]])
        if unreadable or live or descendants:
            raise Refused(f"work still live or unknown: processes {live}, descendants "
                          f"{[d['pid'] for d in descendants]}, unreadable events {unreadable}")
        _write_event(token, "release", outcome=outcome, detail=detail)
        handle.rename(f"{HOST}.g{record['generation']:08d}.{token}.released.json")
    return {"token": token, "outcome": outcome, "generation": record["generation"]}


def reclaim(lane: str, work_id: str, release_condition: str, transport_record: dict | None = None,
            owner_pid: int | None = None) -> dict:
    """Recovery authority: retire an abandoned reservation on positive evidence
    and acquire a new generation. A second reclaimer finds the lock gone or
    held by the winner and is refused."""
    try:
        handle, record = _transition(None)
    except FileNotFoundError:
        raise Refused(f"no reservation on {HOST} to reclaim") from None
    with handle:
        if record is None:
            raise Refused("lock record unreadable or incomplete: unknown, never dead")
        events, unreadable = read_events(record["token"])
        verdict = assess(record, events, unreadable)
        if not verdict["dead"]:
            raise Refused(f"no positive evidence the owner is dead: {verdict['why']}")
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        _write_event(record["token"], "reclaim", evidence=verdict["why"], reclaimer_lane=lane,
                     reclaimer_work_id=work_id)
        handle.rename(f"{HOST}.g{record['generation']:08d}.{record['token']}.reclaimed-{stamp}.json")
    fresh = acquire(lane, work_id, release_condition, transport_record, owner_pid)
    return {**fresh, "reclaimed": {"token": record["token"], "generation": record["generation"],
                                   "evidence": verdict["why"]}}


def status(token: str | None = None) -> dict:
    """Non-acquiring status of this host, or of one reservation's fate."""
    record, body = read_lock()
    out = {"host": HOST, "root": str(root_dir()), "test_root": bool(os.environ.get(TEST_ROOT_ENV)),
           "boot_id": boot_id()}
    if body is None:
        out["state"] = "free"
    elif record is None:
        out.update(state="unknown", why="lock record unreadable or incomplete")
    else:
        events, unreadable = read_events(record["token"])
        verdict = assess(record, events, unreadable)
        out.update(state=verdict["state"], why=verdict["why"], record=record, events=events,
                   processes=verdict.get("processes"), live_descendants=verdict.get("live_descendants"))
    if token is not None:
        if record is not None and record["token"] == token:
            out["token_fate"] = "holds"
        else:
            fates = [r["fate"] for r in _history(None) if r["token"] == token]
            out["token_fate"] = fates[0] if fates else "unknown"
            if fates:
                out["token_events"] = read_events(token)[0]
    return out


def exempt_pids(token: str) -> list[int]:
    """Pids a launcher's busy refusal must not count: this reservation's live
    recorded work and its live descendants."""
    record, _ = read_lock()
    if record is None or record["token"] != token:
        return []
    events, _ = read_events(token)
    work = _work_roots(events)
    pids = [pid for pid, creation in work if process_state(pid, creation) == "alive"]
    pids += [d["pid"] for d in live_descendants(work)]
    if os.environ.get(TOKEN_ENV) == token:
        try:
            pids += job_members(None)  # a nested caller's own supervisor job
        except OSError:
            pass
    return sorted(set(pids))


def _finish(token: str, nested: bool, outcome: str, detail: str) -> None:
    """Outer supervisors release; nested ones only record that they are done."""
    try:
        if nested:
            _change(token, "nested-done", outcome=outcome, detail=detail)
        else:
            release(token, outcome, detail=detail)
    except Refused as exc:
        print(json.dumps({"finish_refused": str(exc)}), file=sys.stderr)


def supervise(token: str, command: list[str], cwd: str | None = None, nested: bool = False) -> int:
    """The target-side supervisor: contain itself, adopt the reservation, run
    the work with the token in its environment, record the child, wait for
    the child and then for its whole job to empty, then release (outer) or
    record nested-done (nested). An outer supervisor keeps retrying the
    release while other recorded work (a nested supervisor) lives."""
    try:
        job = contain_self()
    except OSError as exc:
        # Uncontained work could outlive this process unseen: never run it.
        if not nested:
            _finish(token, False, "spawn-failed", f"containment failed: {exc}")
        print(json.dumps({"refused": f"containment failed: {exc}"}), file=sys.stderr)
        return EXIT_REFUSED
    try:
        adopt(token, nested=nested, contained=True)
    except Refused as exc:
        print(json.dumps({"refused": str(exc)}), file=sys.stderr)
        return EXIT_REFUSED
    env = dict(os.environ, **{TOKEN_ENV: token})
    try:
        child = _spawn_contained(job, command, cwd, env)
    except OSError as exc:
        _finish(token, nested, "spawn-failed", f"{type(exc).__name__}: {exc}")
        return 127
    # Read through the child's own handle, so an exited child is readable too.
    record_descendant(token, child.pid, _child_creation(child))
    code = child.wait()
    outcome, detail = ("success" if code == 0 else "failure"), f"exit code {code}"
    me, listed = os.getpid(), False
    while True:
        others = [pid for pid in job_members(job) if pid != me]
        if not others:
            break
        if not listed:
            members = [{"pid": pid, "creation_time": creation_time(pid)} for pid in others]
            try:
                _change(token, "members", members=members, detail="job members after the child exited")
            except Refused:
                pass
            listed = True
        time.sleep(1.0)
    if nested:
        _finish(token, True, outcome, detail)
        return code
    while True:
        try:
            release(token, outcome, detail=detail)
            return code
        except Refused as exc:
            if "work still live" not in str(exc):
                print(json.dumps({"release_refused": str(exc)}), file=sys.stderr)
                return code
        time.sleep(2.0)  # recorded nested work keeps the host reserved


def wmi_create(command_line: str, cwd: str, timeout: float = 60.0) -> dict:
    """Create a hidden process through WMI (Win32_Process.Create). Returns
    {"return_value", "pid"}; raises TimeoutError or ValueError when the
    creation response is lost or unreadable (the caller must not retry)."""
    script = (
        "$ErrorActionPreference='Stop';"
        "$s=New-CimInstance -ClassName Win32_ProcessStartup -ClientOnly -Property @{ShowWindow=[uint16]0};"
        "$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments "
        "@{CommandLine=$env:HOST_RESERVATION_WMI_COMMAND;CurrentDirectory=$env:HOST_RESERVATION_WMI_CWD;"
        "ProcessStartupInformation=$s};"
        "[pscustomobject]@{return_value=[int]$r.ReturnValue;pid=[int]$r.ProcessId}|ConvertTo-Json -Compress"
    )
    powershell = os.path.join(os.environ.get("SystemRoot", "C:/Windows"), "System32/WindowsPowerShell/v1.0/powershell.exe")
    env = dict(os.environ, HOST_RESERVATION_WMI_COMMAND=command_line, HOST_RESERVATION_WMI_CWD=cwd)
    env.pop(TOKEN_ENV, None)
    done = subprocess.run([powershell, "-NoProfile", "-NonInteractive", "-Command", script], env=env,
                          capture_output=True, text=True, timeout=timeout)
    reply = json.loads(done.stdout.strip().splitlines()[-1])
    return {"return_value": int(reply["return_value"]), "pid": int(reply["pid"])}


def cim_busy(pattern: str, exempt: list[int]) -> list[dict]:
    """A Get-CimInstance busy refusal: live processes whose image name matches
    the regex, other than the exempt pids (the reservation's own work)."""
    script = ("$ErrorActionPreference='Stop';@(Get-CimInstance Win32_Process | Where-Object { $_.Name -match "
              "$env:HOST_RESERVATION_BUSY_PATTERN } | Select-Object ProcessId,Name) | ConvertTo-Json -Compress")
    powershell = os.path.join(os.environ.get("SystemRoot", "C:/Windows"), "System32/WindowsPowerShell/v1.0/powershell.exe")
    env = dict(os.environ, HOST_RESERVATION_BUSY_PATTERN=pattern)
    done = subprocess.run([powershell, "-NoProfile", "-NonInteractive", "-Command", script], env=env,
                          capture_output=True, text=True, timeout=120, check=True)
    text = done.stdout.strip()
    rows = json.loads(text) if text else []
    rows = rows if isinstance(rows, list) else [rows]
    return [r for r in rows if int(r["ProcessId"]) not in set(exempt)]


# This platform's process creation and busy refusal (Linux: see the Linux layer).
create_process = wmi_create if os.name == "nt" else spawn_detached
busy_processes = cim_busy if os.name == "nt" else proc_busy


def dispatch(lane: str, work_id: str, release_condition: str, command: list[str], cwd: str,
             busy_pattern: str | None = None, transport_record: dict | None = None, create=None,
             python: str | None = None) -> dict:
    """The launcher sequence: acquire, the busy refusal (exempting this
    reservation's own work), then WMI creation of the supervisor (Linux:
    spawn_detached) and the handoff. Acquire in the process that dispatches: it is the owner until
    the supervisor adopts, so a separate acquire whose process exits first
    leaves a lock that is reclaimable (its late supervisor then refuses).
    Inside a reservation (nested) the supervisor is created with --nested
    and the shared token on its command line: it contains and adopts itself,
    so the outer supervisor holds the host until it ends even if this caller
    dies or never hears the creation response; it never releases the outer
    lock. A lost creation response is never retried."""
    create = create or create_process
    python = supervisor_python(python)  # refused before anything is acquired
    record = acquire(lane, work_id, release_condition, transport_record)
    token, nested = record["token"], bool(record.get("shared"))
    if busy_pattern:
        try:
            busy = busy_processes(busy_pattern, exempt_pids(token))
        except BaseException:
            if not nested:
                release(token, "cancelled", detail="busy refusal could not run")
            raise
        if busy:
            if not nested:
                release(token, "cancelled", detail=f"busy refusal: {busy}")
            raise Held(f"busy refusal on {HOST}: {busy}", None)
    line = supervisor_command(token, command, python=python, nested=nested)
    result = {"token": token, "generation": record["generation"], "nested": nested, "command_line": line}
    try:
        created = create(line, cwd)
    except Exception as exc:  # lost or unreadable creation response: never retried
        note(token, "spawn-unconfirmed", detail=f"{type(exc).__name__}: {exc}")
        return {**result, "state": "spawn-unconfirmed"}
    if created["return_value"] != 0:
        if not nested:
            release(token, "spawn-failed", detail=f"WMI return value {created['return_value']}")
        return {**result, "state": "spawn-failed", "return_value": created["return_value"]}
    try:
        handoff(token, created["pid"])
    except Refused:
        fate = status(token).get("token_fate", "unknown")
        if not fate.startswith("released"):
            raise
        return {**result, "state": "finished-before-handoff", "pid": created["pid"], "fate": fate}
    return {**result, "state": "dispatched", "pid": created["pid"]}


def supervisor_python(python: str | None = None) -> str:
    """Use the base interpreter for this environment's supervisor. Windows
    virtual-environment python.exe is a redirector: recording its pid would
    leave a live parent waiting on the supervisor while release waits on the
    parent. Work commands retain the caller's requested interpreter. Explicit
    other interpreters are preserved, and app-execution aliases are refused."""
    path = os.path.abspath(python or sys.executable)
    if "\\windowsapps\\" in os.path.normcase(path):
        raise ValueError(f"{path} is an app-execution alias that WMI cannot start; use a regular Python install")
    if os.path.normcase(path) == os.path.normcase(os.path.abspath(sys.executable)):
        path = os.path.abspath(getattr(sys, "_base_executable", None) or sys.executable)
    if "\\windowsapps\\" in os.path.normcase(path):
        raise ValueError(f"{path} is an app-execution alias that WMI cannot start; use a regular Python install")
    return path


def supervisor_command(token: str, command: list[str], python: str | None = None, nested: bool = False) -> str:
    """The WMI command line that runs `command` under this module's supervisor.
    WMI-created processes do not inherit the caller's environment, so the
    token (and a test root) travel on the command line. On Linux it is a
    POSIX-quoted line for spawn_detached."""
    parts = [supervisor_python(python), os.path.abspath(__file__)]
    if os.environ.get(TEST_ROOT_ENV):
        parts += ["--test-root", os.environ[TEST_ROOT_ENV]]
    parts += ["supervise", "--token", token] + (["--nested"] if nested else []) + ["--", *command]
    return command_line(parts)


# ---------------------------------------------------------------- CLI


def _transport(value: str | None) -> dict:
    if not value:
        return {}
    text = Path(value[1:]).read_text(encoding="utf-8") if value.startswith("@") else value
    parsed = json.loads(text)
    if not isinstance(parsed, dict):
        raise ValueError("transport record must be a JSON object")
    return parsed


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    command = []
    if "--" in argv:
        split = argv.index("--")
        argv, command = argv[:split], argv[split + 1:]
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--test-root", help=argparse.SUPPRESS)  # tests only; production uses the canonical root
    sub = parser.add_subparsers(dest="op", required=True)
    for op in ("acquire", "reclaim"):
        p = sub.add_parser(op)
        p.add_argument("--lane", required=True)
        p.add_argument("--work-id", required=True)
        p.add_argument("--release-condition", required=True)
        p.add_argument("--transport-record")
        p.add_argument("--owner-pid", type=int, help="default: the calling process (the CLI's parent)")
    for op in ("handoff", "note", "release", "exempt-pids", "supervise"):
        p = sub.add_parser(op)
        p.add_argument("--token", required=True)
        if op == "handoff":
            p.add_argument("--pid", type=int, required=True)
        if op == "note":
            p.add_argument("--kind", required=True)
            p.add_argument("--detail", default="")
        if op == "release":
            p.add_argument("--outcome", required=True, choices=OUTCOMES)
            p.add_argument("--detail", default="")
        if op == "supervise":
            p.add_argument("--cwd")
            p.add_argument("--nested", action="store_true")
    p = sub.add_parser("status")
    p.add_argument("--token")
    p.add_argument("--line", action="store_true", help="one census line instead of JSON")
    args = parser.parse_args(argv)
    if args.test_root:
        os.environ[TEST_ROOT_ENV] = args.test_root
    try:
        if args.op in ("acquire", "reclaim"):
            owner = args.owner_pid if args.owner_pid is not None else os.getppid()
            fn = acquire if args.op == "acquire" else reclaim
            result = fn(args.lane, args.work_id, args.release_condition, _transport(args.transport_record), owner)
        elif args.op == "handoff":
            result = handoff(args.token, args.pid)
        elif args.op == "note":
            result = note(args.token, args.kind, args.detail)
        elif args.op == "release":
            result = release(args.token, args.outcome, args.detail)
        elif args.op == "exempt-pids":
            print(" ".join(str(p) for p in exempt_pids(args.token)))
            return EXIT_OK
        elif args.op == "supervise":
            if not command:
                parser.error("supervise needs -- COMMAND")
            return supervise(args.token, command, args.cwd, nested=args.nested)
        else:
            result = status(args.token)
            code = {"free": EXIT_OK, "held": EXIT_HELD, "reclaimable": EXIT_RECLAIMABLE}.get(result["state"], EXIT_UNKNOWN)
            if args.line:
                r = result.get("record") or {}
                print(f"host-reservation {HOST} state={result['state']} lane={r.get('lane', '-')} "
                      f"work_id={r.get('work_id', '-')} generation={r.get('generation', '-')} "
                      f"since={r.get('acquired_at', '-')}")
            else:
                print(json.dumps(result, indent=1, sort_keys=True))
            return code
    except Held as exc:
        print(json.dumps({"held": str(exc), "holder": exc.holder}, indent=1, sort_keys=True))
        return EXIT_HELD
    except Refused as exc:
        print(json.dumps({"refused": str(exc)}, indent=1, sort_keys=True))
        return EXIT_REFUSED
    print(json.dumps(result, indent=1, sort_keys=True))
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
