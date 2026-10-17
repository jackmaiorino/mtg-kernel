"""Device-bound public training dispatch with bounded local and SSH workers.

Production entry: dispatch_qualified. Qualification entry is limited to four
initial updates and 80 games across the group, and does not authorize a campaign.
Remote placements supply one shared cpu_affinity list. Their staged pinned
helpers acquire the target reservation and supervise host_slots timed before
drive/runtime setup, native workers and export. Desktop ownership is unchanged.
"""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import platform
import re
import shutil
import subprocess
import sys
import time
import zipfile

REMOTE = os.environ.get('COMPUTE_HOST_SSH', 'compute-host')
REMOTE_INPUT = "C:/mtg-node/public-device-placement-001"
HOSTNAMES = {"desktop": "DESKTOP-DJ1C40R", "computehost": "COMPUTEHOST"}
DISK_RESERVE_BYTES = 60 * 1024**3
REMOTE_INPUT_ENV = "MTG_PUBLIC_TRAINING_REMOTE_INPUT_ROOT"
BUSY_PATTERN = (r"^(cargo|rustc|public_.*|native_.*|expanded_deck_.*|learned_sideboard.*|"
                r"stack_feature_training.*|stack_policy_replay.*|own_registration_training.*|"
                r"phase1_native_actor.*|trainer|mtg_kernel.*)\.exe$")


def host_binding(host, placements):
    """Resolve an optional remote Windows binding, without changing old receipts.

    A computehost placement may supply remote_binding={expected_hostname,
    input_root}. The SSH alias still selects transport; this binding checks the
    actual destination and selects its already-staged inputs/runtime.
    """
    default = dict(expected_hostname=HOSTNAMES[host], input_root=REMOTE_INPUT)
    bindings = []
    for placement in placements:
        if "remote_binding" not in placement:
            bindings.append(default)
            continue
        binding = placement["remote_binding"]
        if host != "computehost" or not isinstance(binding, dict) or set(binding) != {"expected_hostname", "input_root"}:
            raise ValueError("explicit remote hostname and input root required")
        hostname, root = binding["expected_hostname"], binding["input_root"]
        if not isinstance(hostname, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]{0,62}", hostname):
            raise ValueError("invalid expected remote Windows hostname")
        if not isinstance(root, str) or any(ord(c) < 32 for c in root):
            raise ValueError("invalid remote input root")
        path = PureWindowsPath(root)
        if not path.is_absolute() or not re.fullmatch(r"[A-Za-z]:", path.drive) or ".." in path.parts:
            raise ValueError("absolute local Windows remote input root required")
        bindings.append(dict(expected_hostname=hostname.upper(), input_root=path.as_posix()))
    if any(binding != bindings[0] for binding in bindings[1:]):
        raise ValueError("remote placements disagree on host/input binding")
    return bindings[0] if bindings else default


def target_cores(placements):
    """Execution-only affinity shared by one reserved remote worker group."""
    cores = [placement.get("cpu_affinity") for placement in placements]
    if (not cores or any(not isinstance(row, list) or not row
                         or any(type(c) is not int or not 0 <= c < 64 for c in row)
                         or len(set(row)) != len(row) for row in cores)
            or any(sorted(row) != sorted(cores[0]) for row in cores[1:])):
        raise ValueError("remote group requires one explicit distinct CPU affinity")
    return sorted(cores[0])


def target_helpers(runtime):
    # Hash before importing; a copied helper must also be the module in use.
    paths = {key: checked(runtime[key]) for key in ("host_reservation", "host_slots", "worker", "script")}
    import host_reservation_v1 as reservations
    import host_slots_v1 as slots
    if (paths["host_reservation"].resolve() != Path(reservations.__file__).resolve()
            or paths["host_slots"].resolve() != Path(slots.__file__).resolve()
            or paths["worker"].resolve() != Path(__file__).resolve()):
        raise ValueError("remote canonical helper/worker paths differ")
    return reservations, slots


GUARD_MODULES = ("archive_distillation_dispatch_v1", "native_expanded_dispatch_v1",
                 "g115_d4_audit_storage_v1", "public_training_dispatch_v2")


def target_guard_plan(limits, cores, root):
    """Validate execution limits without changing scientific configuration."""
    if set(limits) != {"group_root", "cpu_seconds", "wall_seconds", "storage", "stop_files"}:
        raise ValueError("explicit target limits fields required")
    group = PureWindowsPath(limits["group_root"])
    storage = limits["storage"]
    if set(storage) != {"accounting_roots", "max_physical_bytes", "projected_additional_bytes", "reserve_bytes"}:
        raise ValueError("explicit target storage fields required")
    roots = [PureWindowsPath(p) for p in storage["accounting_roots"]]
    base = PureWindowsPath("C:/mtg-node")
    if (not re.fullmatch(r"C:/mtg-node/[a-z0-9/-]+", group.as_posix())
            or str(group) != str(PureWindowsPath(root)) or not group.is_absolute()
            or group == base or not group.is_relative_to(base) or not roots
            or any(not r.is_absolute() or r == base or not r.is_relative_to(base) for r in roots)
            or not any(group.is_relative_to(r) for r in roots)
            or any(a.is_relative_to(b) for i, a in enumerate(roots) for j, b in enumerate(roots) if i != j)):
        raise ValueError("target group must be inside nonoverlapping C:/mtg-node accounting roots")
    for value in (limits["cpu_seconds"], limits["wall_seconds"], storage["max_physical_bytes"],
                  storage["projected_additional_bytes"], storage["reserve_bytes"]):
        if type(value) not in (int, float) or not 0 < value < float("inf"):
            raise ValueError("positive finite target bounds required")
    if (storage["reserve_bytes"] < DISK_RESERVE_BYTES
            or storage["projected_additional_bytes"] > storage["max_physical_bytes"]
            or not limits["stop_files"] or any(not PureWindowsPath(p).is_absolute() for p in limits["stop_files"])):
        raise ValueError("target reserve, projection or STOP declaration differs")
    return dict(cpu_affinity=cores, run_limits={k: limits[k] for k in ("cpu_seconds", "wall_seconds")},
                storage=storage, stop_files=limits["stop_files"])


def target_guard_class(runtime):
    # Check the entire import closure before any Guard import executes.
    paths = {name: checked(runtime[name]) for name in GUARD_MODULES}
    target_helpers(runtime)
    import importlib
    modules = {name: importlib.import_module(name) for name in GUARD_MODULES}
    if any(Path(modules[name].__file__).resolve() != paths[name].resolve() for name in GUARD_MODULES):
        raise ValueError("target Guard dependency path differs")
    return modules[GUARD_MODULES[0]].Guard


def guarded_target(path):
    """Inside the existing timed owner, contain setup, training and export."""
    path, group = Path(path), read(path)
    root = path.parent
    guard, result = None, dict(complete=False, group=pin(path))
    try:
        cores = target_cores([job["placement"] for job in group["jobs"].values()])
        plan = target_guard_plan(group["target_limits"], cores, root)
        Guard = target_guard_class(group["target_runtime"])
        reservations, _ = target_helpers(group["target_runtime"])
        guard = Guard(plan, root/"guard", False)
        script = checked(group["target_runtime"]["script"])
        command = [str(Path(os.environ.get("SystemRoot", "C:/Windows"))/"System32/WindowsPowerShell/v1.0/powershell.exe"),
                   "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(script),
                   "-Python", reservations.supervisor_python(sys.executable)]
        with (root/"guard"/"stdout").open("x") as out, (root/"guard"/"stderr").open("x") as err:
            child = guard.spawn(command, out, err)
            result["exit_code"] = child.wait()
            guard.quiet()
        if result["exit_code"] != 0:
            raise RuntimeError("contained target script failed")
        result["sample"] = guard.check()
        result["complete"] = True
    except BaseException as error:
        result["error"] = str(error)
        raise
    finally:
        try:
            if guard is not None:
                try:
                    result["final_sample"] = guard.terminate_and_drain()
                finally:
                    guard.close()
        except BaseException as error:
            result.update(complete=False, cleanup_error=str(error))
            raise
        finally:
            write(root/"guard-result.json", result)


def require_target_context(spec):
    runtime = spec.get("target_runtime")
    if not isinstance(runtime, dict):
        raise ValueError("remote worker requires pinned canonical runtime")
    reservations, slots = target_helpers(runtime)
    cores = target_cores([spec["placement"]])
    token = os.environ.get(reservations.TOKEN_ENV)
    timed = slots.timed_state()
    if (not token or reservations.status(token).get("token_fate") != "holds"
            or timed.get("token_sha256") != slots.token_hash(token)
            or timed.get("why") != "timed work declared its cores"
            or timed.get("cores") != cores
            or slots.parse_cores(os.environ.get(slots.CORES_ENV, "")) != cores
            or slots.current_affinity() != cores):
        raise ValueError("remote worker requires its held reservation and exact timed affinity")
    return dict(reservation_sha256=slots.token_hash(token), cpu_affinity=cores,
                helpers={key: runtime[key] for key in ("host_reservation", "host_slots")})


def wait_target_dispatch(result, reservations, timeout, previous_events=()):
    """Observe existing canonical state only; never retry, reclaim or release."""
    if result["state"] not in ("dispatched", "finished-before-handoff"):
        raise RuntimeError("remote dispatch unconfirmed or refused; do not redispatch: " + result["state"])
    boundary = {json.dumps(event, sort_keys=True) for event in previous_events}
    deadline = time.monotonic()+timeout
    while True:
        state = reservations.status(result["token"])
        fate = state.get("token_fate")
        events = state.get("events", []) if fate == "holds" else state.get("token_events", [])
        if result["nested"]:
            fresh = [event for event in events if json.dumps(event, sort_keys=True) not in boundary]
            identities = {event["creation_time"] for event in fresh
                          if event["kind"] in ("handoff", "adopt") and event.get("pid") == result["pid"]
                          and type(event.get("creation_time")) is int}
            if len(identities) > 1:
                raise RuntimeError("nested supervisor creation identity is ambiguous; preserve and inspect")
            completed = [event for event in fresh if event["kind"] == "nested-done"
                         and event["by_pid"] == result["pid"] and event["by_process_creation_time"] in identities]
            if completed:
                event = completed[-1]
                if reservations.process_state(event["by_pid"], event["by_process_creation_time"]) == "absent":
                    return event
        elif fate == "released":
            completed = [event for event in events if event["kind"] == "release"]
            if completed:
                return completed[-1]
        if fate not in ("holds", "released"):
            raise RuntimeError("remote reservation outcome is " + str(fate) + "; preserve and inspect")
        if time.monotonic() >= deadline:
            raise TimeoutError("remote reservation still nonterminal; preserve owner and do not redispatch")
        time.sleep(1)


def reserve_worker_group(path):
    """Target-host adapter over canonical dispatch, supervision and timed slots."""
    started, unix_start = time.monotonic(), time.time()
    path, group = Path(path), read(path)
    root = path.parent
    result = dict(complete=False, group=pin(path), started_unix=unix_start)
    try:
        placements = [job["placement"] for job in group["jobs"].values()]
        if not placements or any(p["host"] != "computehost" for p in placements):
            raise ValueError("reserved adapter accepts one remote worker group only")
        binding = host_binding("computehost", placements)
        if platform.node().upper() != binding["expected_hostname"]:
            raise ValueError("reserved adapter running on the wrong host")
        cores = target_cores(placements)
        reservations, slots = target_helpers(group["target_runtime"])
        parent_token = os.environ.get(reservations.TOKEN_ENV)
        previous_events = []
        if parent_token:
            timed = slots.timed_state()
            if (timed.get("token_sha256") == slots.token_hash(parent_token)
                    and timed.get("why") == "timed work declared its cores"):
                raise ValueError("outer reservation already has a timed declaration; run the worker group inside that owner")
            previous_events, unreadable = reservations.read_events(parent_token)
            if unreadable:
                raise ValueError("cannot bind nested dispatch to an incomplete canonical event log")
            result["prior_event_count"] = len(previous_events)
        python = reservations.supervisor_python(sys.executable)
        command = [python, "-B", str(checked(group["target_runtime"]["host_slots"])), "timed",
                   "--cores", slots.format_cores(cores), "--",
                   str(Path(os.environ.get("SystemRoot", "C:/Windows"))/"System32/WindowsPowerShell/v1.0/powershell.exe"),
                   "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File",
                   str(checked(group["target_runtime"]["script"])), "-Python", python]
        if "target_limits" in group:
            target_guard_plan(group["target_limits"], cores, root)
            command = command[:command.index("--")+1]+[python, "-B", str(Path(__file__).resolve()),
                                                        "--guarded-target", str(path)]
        created = {}
        def create(line, cwd):
            created.update(reservations.create_process(line, cwd))
            write(root/"reservation-created.json", created)
            return created
        result["dispatch"] = reservations.dispatch(
            lane="public-training", work_id=root.name,
            release_condition="Release after worker group, export and all descendants finish, including failure",
            command=command, cwd=str(root), busy_pattern=BUSY_PATTERN, create=create, python=python,
            transport_record=dict(group=result["group"], cpu_affinity=cores))
        write(root/"reservation-dispatch.json", result["dispatch"])
        observed = dict(result["dispatch"])
        if "pid" not in observed and "pid" in created:
            observed["pid"] = created["pid"]
        limit = sum(job["wall_seconds"] for job in group["jobs"].values())+720
        result["terminal"] = wait_target_dispatch(observed, reservations, limit, previous_events)
        if result["terminal"].get("outcome") != "success":
            raise RuntimeError("canonical remote supervisor reported failure")
        if "target_limits" in group:
            result["guard"] = read(root/"guard-result.json")
            if not result["guard"].get("complete"):
                raise RuntimeError("target Guard did not complete")
        executions = [read(Path(job["request"]["path"]).parent/"execution.json") for job in group["jobs"].values()]
        first, last = min(e["started_unix"] for e in executions), max(e["finished_unix"] for e in executions)
        result["startup_seconds"] = first-unix_start
        result["recovery_and_release_seconds"] = time.time()-last
        # The allocation model already prices native execution. Charge the
        # remaining measured setup/stagger gaps too, without charging it twice.
        durations = [e["finished_unix"]-e["started_unix"] for e in executions]
        if group["mode"] == "parallel":
            native = max(durations)
        elif group["mode"] == "device_queues":
            queues = {}
            for job, duration in zip(group["jobs"].values(), durations):
                device = job["placement"]["gpu_uuid"]
                queues[device] = queues.get(device, 0)+duration
            native = max(queues.values())
        else:
            native = sum(durations)
        result["inter_job_seconds"] = max(0, last-first-native)
        result["setup_seconds"] = result["startup_seconds"]+result["inter_job_seconds"]
        if result["startup_seconds"] < 0 or result["recovery_and_release_seconds"] < 0:
            raise ValueError("remote timing clock moved backwards")
        result["export"] = read(root/"export-execution.json")
        result["complete"] = True
    except BaseException as error:
        result["error"] = str(error)
        print(json.dumps(result), flush=True)
        raise
    finally:
        result["seconds"] = time.monotonic()-started
        write(root/"reservation-execution.json", result)
    return result


def read(path):
    return json.loads(Path(path).read_bytes())


def pin(path):
    path = Path(path)
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return dict(path=str(path), sha256=digest)


def checked(item):
    if pin(item["path"])["sha256"] != item["sha256"]:
        raise ValueError("changed pinned input: "+item["path"])
    return Path(item["path"])


def write(path, value):
    with Path(path).open("x") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)


def gpu_uuid(ordinal):
    output = subprocess.check_output(["nvidia-smi", f"--id={ordinal}", "--query-gpu=uuid", "--format=csv,noheader"], text=True)
    return output.strip()


def process_telemetry(child, ordinal):
    """Read only the owned native process and its selected GPU."""
    import ctypes
    from ctypes import wintypes
    handle=wintypes.HANDLE(int(child._handle))
    times=[wintypes.FILETIME() for _ in range(4)]
    if not ctypes.windll.kernel32.GetProcessTimes(handle,*[ctypes.byref(t) for t in times]):
        raise OSError("cannot read owned native process CPU time")
    ticks=lambda t: (t.dwHighDateTime << 32)+t.dwLowDateTime
    class IO(ctypes.Structure):
        _fields_=[(name,ctypes.c_ulonglong) for name in ['read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes']]
    io=IO()
    if not ctypes.windll.kernel32.GetProcessIoCounters(handle,ctypes.byref(io)):
        raise OSError("cannot read owned native process I/O")
    class Memory(ctypes.Structure):
        _fields_=[('cb',wintypes.DWORD),('faults',wintypes.DWORD)]+[(name,ctypes.c_size_t) for name in ['peak_rss','rss','peak_paged','paged','peak_nonpaged','nonpaged','pagefile','peak_pagefile']]
    memory=Memory();memory.cb=ctypes.sizeof(memory)
    if not ctypes.windll.psapi.GetProcessMemoryInfo(handle,ctypes.byref(memory),memory.cb):
        raise OSError("cannot read owned native process memory")
    gpu=subprocess.check_output(['nvidia-smi',f'--id={ordinal}','--query-gpu=uuid,utilization.gpu,memory.used,power.draw','--format=csv,noheader,nounits'],text=True).strip()
    return dict(at_unix=time.time(),pid=child.pid,cpu_seconds=(ticks(times[2])+ticks(times[3]))/1e7,
                rss_bytes=memory.rss,read_bytes=io.read_bytes,write_bytes=io.write_bytes,gpu=gpu)


def worker(spec):
    placement = spec["placement"]
    binding = host_binding(placement["host"], [placement])
    if platform.node().upper() != binding["expected_hostname"]:
        raise ValueError("worker running on the wrong host")
    if "remote_binding" in placement:
        root = PureWindowsPath(binding["input_root"])
        if (PureWindowsPath(os.environ.get(REMOTE_INPUT_ENV, "")) != root
                or PureWindowsPath(os.environ.get("CUDA_PATH", "")) != root/"runtime"):
            raise ValueError("worker remote input/runtime binding differs")
    ownership = require_target_context(spec) if placement["host"] == "computehost" else None
    if os.environ.get("CUDA_VISIBLE_DEVICES"):
        raise ValueError("explicit CUDA remapping needs separate qualification")
    if gpu_uuid(placement["gpu_ordinal"]) != placement["gpu_uuid"]:
        raise ValueError("device UUID no longer matches placement")
    binary = checked(spec["binary"])
    request_path = checked(spec["request"])
    request = read(request_path)
    if request["execution_gpu_ordinal"] != placement["gpu_ordinal"] or request["collector_workers"] != placement["workers"]:
        raise ValueError("request does not implement placement")
    folder = request_path.parent
    started = time.monotonic()
    unix_start = time.time()
    observed, failure, timed_out = None, None, False
    with (folder/"stdout").open("x") as stdout, (folder/"stderr").open("x") as stderr:
        child = subprocess.Popen([str(binary), str(request_path)], stdout=stdout, stderr=stderr,
                                 creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
        write(folder/"started.json",dict(pid=child.pid,placement=placement,started_unix=unix_start))
        try:
            # Bind the actual native PID's CUDA context, not just a requested index.
            while child.poll() is None and observed is None and time.monotonic()-started < 30:
                contexts = subprocess.check_output(["nvidia-smi", "--query-compute-apps=pid,gpu_uuid", "--format=csv,noheader"], text=True)
                uuids = {row.split(",",1)[1].strip() for row in contexts.splitlines()
                         if row.split(",",1)[0].strip() == str(child.pid)}
                if uuids:
                    if uuids != {placement["gpu_uuid"]}:
                        raise ValueError("native process attached to a different GPU")
                    observed = next(iter(uuids))
                else:
                    time.sleep(.25)
            if observed is None:
                raise ValueError("native GPU context was not observed")
            remaining = spec["wall_seconds"]-(time.monotonic()-started)
            if remaining <= 0:
                raise subprocess.TimeoutExpired(str(binary),spec["wall_seconds"])
            if spec.get("telemetry",False):
                with (folder/"telemetry.jsonl").open("x") as telemetry:
                    while child.poll() is None:
                        try:
                            sample=process_telemetry(child,placement["gpu_ordinal"])
                        except OSError:
                            if child.poll() is not None: break
                            raise
                        telemetry.write(json.dumps(sample)+"\n");telemetry.flush()
                        remaining=spec["wall_seconds"]-(time.monotonic()-started)
                        if remaining <= 0: raise subprocess.TimeoutExpired(str(binary),spec["wall_seconds"])
                        try:child.wait(timeout=min(5,remaining))
                        except subprocess.TimeoutExpired:pass
                code=child.wait()
            else:
                code = child.wait(timeout=remaining)
        except Exception as error:
            timed_out = isinstance(error, subprocess.TimeoutExpired)
            failure = str(error)
            if child.poll() is None:
                child.kill()
            code = child.wait()
    execution = dict(exit_code=code, timeout=timed_out, error=failure, seconds=time.monotonic()-started,
                     started_unix=unix_start, finished_unix=time.time(), placement=placement,
                     observed_gpu_uuid=observed, binary_sha256=spec["binary"]["sha256"], request_sha256=spec["request"]["sha256"])
    if ownership is not None:
        execution["target_ownership"] = ownership
    write(folder/"execution.json",execution)
    if code != 0 or failure:
        raise RuntimeError(f"native execution failed: {folder}: {execution}")
    outputs = Path(request["output_directory"])
    if not (outputs/"completion.json").is_file():
        raise ValueError("missing native completion")
    write(folder/"recovery.json",dict(source_output_directory=str(outputs),
          files={p.relative_to(outputs).as_posix():pin(p)["sha256"] for p in sorted(outputs.rglob("*.json"))}))
    return execution


def worker_group(path):
    group = read(path)
    if "target_runtime" in group:
        group["jobs"] = {name: dict(job, target_runtime=group["target_runtime"]) for name, job in group["jobs"].items()}
    if group["mode"] == "device_queues":
        queues = {}
        for spec in group["jobs"].values():
            queues.setdefault(spec["placement"]["gpu_uuid"], []).append(spec)
        def drain(jobs):
            for spec in jobs: worker(spec)
        with ThreadPoolExecutor(max_workers=len(queues)) as pool:
            list(pool.map(drain, queues.values()))
    elif group["mode"] == "parallel":
        with ThreadPoolExecutor(max_workers=len(group["jobs"])) as pool:
            list(pool.map(worker,group["jobs"].values()))
    else:
        for spec in group["jobs"].values():
            worker(spec)


def export_group(root):
    started = time.monotonic()
    files = [p for p in sorted((root/"jobs").rglob("*")) if p.is_file()]
    manifest = {p.relative_to(root).as_posix():pin(p)["sha256"] for p in files}
    write(root/"export-manifest.json",manifest)
    with zipfile.ZipFile(root/"results.zip","x",compression=zipfile.ZIP_DEFLATED,compresslevel=1) as archive:
        for path in files:
            archive.write(path,path.relative_to(root).as_posix())
        archive.write(root/"export-manifest.json","export-manifest.json")
    result = dict(files=len(files), zip_sha256=pin(root/"results.zip")["sha256"])
    write(root/"export-execution.json", dict(**result, seconds=time.monotonic()-started))
    print(json.dumps(result))


def ssh_ps(script, timeout=60):
    encoded = base64.b64encode(("$ProgressPreference='SilentlyContinue'\n"+script).encode("utf-16le")).decode()
    result = subprocess.run(["ssh","-o","BatchMode=yes","-o","ConnectTimeout=10",REMOTE,
                             "powershell","-NoProfile","-EncodedCommand",encoded],capture_output=True,text=True,timeout=timeout)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


def require_disk_reserve(free_bytes):
    if type(free_bytes) is not int or free_bytes < DISK_RESERVE_BYTES:
        raise ValueError("60 GiB target-volume reserve unavailable")


def preflight(host, placements, target_drive):
    binding = host_binding(host, placements)
    drive = str(target_drive).upper()
    if not re.fullmatch(r"[A-Z]", drive):
        raise ValueError("single target drive letter required")
    script=r'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$active=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '__BUSY_PATTERN__'} | Select-Object Name,ProcessId,CommandLine)
$disk=Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='__TARGET_DRIVE__:'"
if (-not $disk) {throw 'target volume unavailable'}
[pscustomobject]@{host=$env:COMPUTERNAME;at=(Get-Date).ToUniversalTime().ToString('o');active=$active;disk_free_bytes=[int64]$disk.FreeSpace;gpu=@(& nvidia-smi --query-gpu=index,uuid,memory.free,utilization.gpu --format=csv,noheader,nounits)} | ConvertTo-Json -Depth 3'''.replace("__TARGET_DRIVE__",drive).replace("__BUSY_PATTERN__",BUSY_PATTERN)
    if host == "computehost":
        result=json.loads(ssh_ps(script))
    else:
        encoded=base64.b64encode(script.encode("utf-16le")).decode()
        result=json.loads(subprocess.check_output(["powershell","-NoProfile","-EncodedCommand",encoded],text=True))
    if result["host"].upper() != binding["expected_hostname"] or result["active"]:
        raise ValueError("placement has a different host or competing native owner")
    require_disk_reserve(result.get("disk_free_bytes"))
    rows={int(parts[0]):parts for parts in ([x.strip() for x in row.split(',')] for row in result["gpu"])}
    for placement in placements:
        row=rows[placement["gpu_ordinal"]]
        if row[1] != placement["gpu_uuid"] or int(row[2]) < 2048 or int(row[3]) > 5:
            raise ValueError("selected GPU is unavailable; preserve its current work")
    return result


def _dispatch_group(root, binary_pin, configs, placements, mode, stop, wall_seconds, target_limits=None):
    root = Path(root)
    staging_started=time.monotonic()
    if mode not in ["parallel","sequential","device_queues"] or set(configs) != set(placements):
        raise ValueError("invalid dispatch group")
    if not 0 < wall_seconds <= 7200 or not re.fullmatch(r"[a-z0-9-]+",root.name):
        raise ValueError("invalid root name or wall cap")
    binary = checked(binary_pin)
    preflights = {}
    for host in {p["host"] for p in placements.values()}:
        if host not in HOSTNAMES:
            raise ValueError("no qualified dispatcher for host")
        if host == "computehost":
            target_cores([p for p in placements.values() if p["host"] == host])
        target_drive = root.drive.rstrip(":") if host == "desktop" else "C"
        preflights[host] = preflight(host,[p for p in placements.values() if p["host"]==host],target_drive)
    if target_limits is not None:
        remote = [p for p in placements.values() if p["host"] == "computehost"]
        if not remote:
            raise ValueError("target limits require a remote placement")
        target_guard_plan(target_limits, target_cores(remote), target_limits["group_root"])
    root.mkdir()
    for host, report in preflights.items():
        write(root/f"{host}-preflight.json",report)
    for name in configs:
        if not re.fullmatch(r"[a-z0-9-]+",name):
            raise ValueError("invalid job label")
        checked(configs[name])
    staged = {}
    for host in sorted({p["host"] for p in placements.values()}):
        if host not in HOSTNAMES:
            raise ValueError("no qualified dispatcher for host")
        local = root/host
        (local/"jobs").mkdir(parents=True)
        remote_root = f"C:/mtg-node/{root.name}" if host == "computehost" else str(local)
        if host == "computehost" and target_limits is not None:
            remote_root = target_limits["group_root"]
        native_binary = str(Path(remote_root)/"trainer.exe")
        shutil.copy2(binary,local/"trainer.exe")
        group = dict(mode=mode,jobs={})
        if host == "computehost" and target_limits is not None:
            group["target_limits"] = target_limits
        for name, placement in placements.items():
            if placement["host"] != host:
                continue
            folder = local/"jobs"/name
            folder.mkdir()
            native_folder = str(Path(remote_root)/"jobs"/name)
            config = read(configs[name]["path"])
            request = dict(config=config,output_directory=str(Path(native_folder)/"outputs"),resume=None,
                           stop_after=stop if stop is not None else len(config["updates"]),
                           collector_workers=placement["workers"],execution_gpu_ordinal=placement["gpu_ordinal"])
            write(folder/"request.json",request)
            group["jobs"][name]=dict(placement=placement,wall_seconds=wall_seconds,telemetry=mode=="device_queues",
                binary=dict(path=native_binary,sha256=binary_pin["sha256"]),
                request=dict(path=str(Path(native_folder)/"request.json"),sha256=pin(folder/"request.json")["sha256"]))
        if host == "computehost":
            binding = host_binding(host, [job["placement"] for job in group["jobs"].values()])
            input_root = binding["input_root"].replace("'", "''")
            binding_environment = (f"$env:{REMOTE_INPUT_ENV}=$inputRoot\n"
                                   if any("remote_binding" in job["placement"] for job in group["jobs"].values()) else "")
            shutil.copy2(__file__,local/"worker.py")
            ps=f'''param([Parameter(Mandatory=$true)][string]$Python)
$ErrorActionPreference='Stop'
$root='{remote_root}'
$inputRoot='{input_root}'
{binding_environment}# Only the Guard-contained script removes its inherited CPU-only mask.
{("Remove-Item Env:CUDA_VISIBLE_DEVICES -ErrorAction SilentlyContinue" if target_limits is not None else "")}
$created=@()
try {{
 foreach ($drive in @('D','E')) {{
  if (Test-Path -LiteralPath "${{drive}}:/") {{throw 'existing drive mapping; preserve owner'}}
  & subst "${{drive}}:" "$inputRoot/$drive"
  if ($LASTEXITCODE -ne 0) {{throw 'drive mapping failed'}}
  $created+=$drive
 }}
 $env:CUDA_PATH="$inputRoot/runtime"
 $env:PATH="$inputRoot/runtime/bin;$env:PATH"
 New-Item -ItemType Directory -Path "$root/temp" | Out-Null
 $env:TEMP="$root/temp"; $env:TMP="$root/temp"
 & $Python -B "$root/worker.py" --worker-group "$root/group.json"
 if ($LASTEXITCODE -ne 0) {{throw 'native worker group failed'}}
 & $Python -B "$root/worker.py" --export "$root"
 if ($LASTEXITCODE -ne 0) {{throw 'owned export failed'}}
}} finally {{ foreach ($drive in $created) {{ & subst "${{drive}}:" /D }} }}
'''
            (local/"run.ps1").write_text(ps)
            runtime = {}
            for key, name in (("host_reservation", "host_reservation_v1.py"), ("host_slots", "host_slots_v1.py"),
                              ("worker", "worker.py"), ("script", "run.ps1")):
                if key in ("host_reservation", "host_slots"):
                    shutil.copy2(Path(__file__).with_name(name), local/name)
                runtime[key] = dict(path=str(Path(remote_root)/name), sha256=pin(local/name)["sha256"])
            if target_limits is not None:
                for name in GUARD_MODULES:
                    shutil.copy2(Path(__file__).with_name(name+".py"), local/(name+".py"))
                    runtime[name] = dict(path=str(Path(remote_root)/(name+".py")), sha256=pin(local/(name+".py"))["sha256"])
            group["target_runtime"] = runtime
        write(local/"group.json",group)
        if host == "computehost":
            ssh_ps(f"if (Test-Path -LiteralPath '{remote_root}') {{throw 'remote destination exists'}}; New-Item -ItemType Directory -Path '{remote_root}/jobs' | Out-Null")
            for name in group["jobs"]:
                ssh_ps(f"New-Item -ItemType Directory -Path '{remote_root}/jobs/{name}' | Out-Null")
                subprocess.run(["scp","-q",str(local/"jobs"/name/"request.json"),f"{REMOTE}:{remote_root}/jobs/{name}/request.json"],check=True,timeout=60)
            for name in ("trainer.exe", "worker.py", "group.json", "run.ps1", "host_reservation_v1.py", "host_slots_v1.py") + (tuple(n+".py" for n in GUARD_MODULES) if target_limits is not None else ()):
                subprocess.run(["scp","-q",str(local/name),f"{REMOTE}:{remote_root}/{name}"],check=True,timeout=60)
        staged[host]=(local,remote_root,group)

    def run_host(host):
        local, remote_root, group=staged[host]
        if host == "desktop":
            worker_group(local/"group.json")
            return dict(startup_seconds=0.0, recovery_seconds=0.0)
        # Native children have their own wall cap even if SSH observation is interrupted.
        command=["ssh","-o","BatchMode=yes","-o","ServerAliveInterval=15","-o","ServerAliveCountMax=4",REMOTE,
                 "python", "-B", remote_root+"/worker.py", "--reserve-worker-group", remote_root+"/group.json"]
        before=time.monotonic()
        with (local/"ssh.stdout").open("x") as out,(local/"ssh.stderr").open("x") as err:
            result=subprocess.run(command,stdout=out,stderr=err,timeout=wall_seconds*len(group["jobs"])+720)
        write(local/"ssh-execution.json",dict(exit_code=result.returncode,seconds=time.monotonic()-before))
        if result.returncode:
            raise RuntimeError("remote worker group failed; preserve and inspect remote state")
        recovery_started=time.monotonic()
        reserved = read_json_text((local/"ssh.stdout").read_text())
        if not reserved.get("complete"):
            raise RuntimeError("remote reservation completion missing")
        write(local/"reservation-execution.json", reserved)
        receipt=reserved["export"]
        before=time.monotonic()
        subprocess.run(["scp","-q",f"{REMOTE}:{remote_root}/results.zip",str(local/"results.zip")],check=True,timeout=600)
        if pin(local/"results.zip")["sha256"] != receipt["zip_sha256"]:
            raise ValueError("remote export archive hash differs")
        recovered=local/"recovered"
        recovered.mkdir()
        with zipfile.ZipFile(local/"results.zip") as archive:
            for item in archive.namelist():
                target=(recovered/item).resolve()
                if not target.is_relative_to(recovered.resolve()):
                    raise ValueError("unsafe export path")
            archive.extractall(recovered)
        files=read(recovered/"export-manifest.json")
        for name,digest in files.items():
            if pin(recovered/name)["sha256"] != digest:
                raise ValueError("recovered remote file differs")
        write(local/"recovery-verification.json",dict(files=len(files),mismatches=0,download_seconds=time.monotonic()-before,archive=receipt))
        return dict(startup_seconds=reserved["setup_seconds"],
                    recovery_seconds=reserved["recovery_and_release_seconds"]+time.monotonic()-recovery_started)

    staging_seconds=time.monotonic()-staging_started
    started=time.monotonic()
    if mode in ["parallel", "device_queues"]:
        with ThreadPoolExecutor(max_workers=len(staged)) as pool:
            recoveries=list(pool.map(run_host,staged))
    else:
        recoveries=[run_host(host) for host in staged]
    reports={}
    for name, placement in placements.items():
        host=placement["host"]
        base=staged[host][0]
        folder=(base/"recovered" if host == "computehost" else base)/"jobs"/name
        request=read(folder/"request.json")
        output=folder/"outputs"
        completion=read(output/"completion.json")
        outputs={f"{i:04}/{file}":pin(output/f"{i:04}"/file)
                 for i in range(completion["first_update"],completion["next_update"])
                 for file in ["checkpoint.json","optimizer.json"]+[f"episode-{j:03}.json" for j in range(len(request["config"]["updates"][i]))]}
        report=dict(placement=placement,config=configs[name],binary=binary_pin,execution=pin(folder/"execution.json"),
                    request=pin(folder/"request.json"),completion=pin(output/"completion.json"),outputs=outputs,
                    source_output_directory=request["output_directory"],local_output_directory=str(output),
                    recovery=pin(folder/"recovery.json"))
        path=root/f"{name}-benchmark.json"
        write(path,report)
        reports[name]=pin(path)
    write(root/"group-benchmark.json",dict(mode=mode,jobs=reports,seconds=time.monotonic()-started,
        staging_seconds=staging_seconds+sum(r["startup_seconds"] for r in recoveries),
        reservation_setup_seconds=sum(r["startup_seconds"] for r in recoveries),
        recovery_seconds=sum(r["recovery_seconds"] for r in recoveries)))
    return pin(root/"group-benchmark.json")


def read_json_text(text):
    return json.loads(text)


def dispatch_qualification(root,binary,configs,placements,mode,updates=3,*,target_limits=None):
    if not 1 <= updates <= 4 or len(configs) != 2:
        raise ValueError("qualification is a bounded two-arm prefix only")
    games=sum(len(batch) for item in configs.values() for batch in read(checked(item))["updates"][:updates])
    if games > 80:
        raise ValueError("qualification prefix exceeds 80 games")
    return _dispatch_group(root,binary,configs,placements,mode,updates,300,target_limits=target_limits)


def dispatch_stack_qualification(root,binary,configs,placements,updates=3):
    """Fixed whole-batch timing prefix, never a substitute for full-run authority."""
    if updates not in [1,3] or not 1 <= len(configs) <= 3:
        raise ValueError("stack timing requires one cheap or three complete initial updates")
    values = [read(checked(item)) for item in configs.values()]
    if any(c.get("schema") != "mtg-kernel-stack-training-config/v1" or len(c["updates"]) < updates for c in values):
        raise ValueError("stack timing config or prefix differs")
    if any(not 1 <= len(batch) <= 10 for c in values for batch in c["updates"][:updates]):
        raise ValueError("stack timing batches exceed ten games")
    return _dispatch_group(root,binary,configs,placements,"device_queues",updates,300)


def dispatch_stack_qualified(root,binary,configs,choice_path,wall_seconds):
    from compute_throughput_v3 import require_allocation
    jobs={name:dict(config_sha256=item["sha256"],updates=len(read(checked(item))["updates"])) for name,item in configs.items()}
    selected=require_allocation(choice_path,binary["sha256"],jobs)
    if selected["mode"] != "device_queues":
        raise ValueError("stack production requires qualified per-device queues")
    return _dispatch_group(root,binary,configs,selected["placements"],selected["mode"],None,wall_seconds)


def dispatch_qualified(root,binary,configs,choice_path,wall_seconds):
    from compute_throughput_v2 import require_allocation
    jobs={name:dict(config_sha256=item["sha256"],updates=len(read(checked(item))["updates"])) for name,item in configs.items()}
    selected=require_allocation(choice_path,binary["sha256"],jobs)
    return _dispatch_group(root,binary,configs,selected["placements"],selected["mode"],None,wall_seconds)


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--worker-group",type=Path)
    parser.add_argument("--reserve-worker-group",type=Path)
    parser.add_argument("--export",type=Path)
    parser.add_argument("--guarded-target",type=Path)
    args=parser.parse_args()
    if args.guarded_target:
        guarded_target(args.guarded_target)
    elif args.reserve_worker_group:
        print(json.dumps(reserve_worker_group(args.reserve_worker_group)))
    elif args.worker_group:
        worker_group(args.worker_group)
    elif args.export:
        export_group(args.export)
    else:
        parser.error("choose a worker operation")
