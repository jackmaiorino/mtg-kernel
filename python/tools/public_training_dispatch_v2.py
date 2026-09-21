"""Device-bound public training dispatch with bounded local and SSH workers.

Production entry: dispatch_qualified. Qualification entry is limited to four
initial updates and 80 games across the group, and does not authorize a campaign.
"""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time
import zipfile

REMOTE = "haley@100.71.75.65"
REMOTE_INPUT = "C:/mtg-node/public-device-placement-001"
HOSTNAMES = {"jack": "DESKTOP-DJ1C40R", "haleyspc": "HALEYSPC"}


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


def worker(spec):
    placement = spec["placement"]
    if platform.node().upper() != HOSTNAMES[placement["host"]]:
        raise ValueError("worker running on the wrong host")
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
    if group["mode"] == "parallel":
        with ThreadPoolExecutor(max_workers=len(group["jobs"])) as pool:
            list(pool.map(worker,group["jobs"].values()))
    else:
        for spec in group["jobs"].values():
            worker(spec)


def export_group(root):
    files = [p for p in sorted((root/"jobs").rglob("*")) if p.is_file()]
    manifest = {p.relative_to(root).as_posix():pin(p)["sha256"] for p in files}
    write(root/"export-manifest.json",manifest)
    with zipfile.ZipFile(root/"results.zip","x",compression=zipfile.ZIP_DEFLATED,compresslevel=1) as archive:
        for path in files:
            archive.write(path,path.relative_to(root).as_posix())
        archive.write(root/"export-manifest.json","export-manifest.json")
    print(json.dumps(dict(files=len(files),zip_sha256=pin(root/"results.zip")["sha256"])))


def ssh_ps(script, timeout=60):
    encoded = base64.b64encode(("$ProgressPreference='SilentlyContinue'\n"+script).encode("utf-16le")).decode()
    result = subprocess.run(["ssh","-o","BatchMode=yes","-o","ConnectTimeout=10",REMOTE,
                             "powershell","-NoProfile","-EncodedCommand",encoded],capture_output=True,text=True,timeout=timeout)
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


def preflight(host, placements):
    script=r'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$active=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '^trainer\.exe$|public_feature_training|learned_sideboard|expanded_deck_training|phase1_native_actor|cargo|rustc'} | Select-Object Name,ProcessId,CommandLine)
[pscustomobject]@{host=$env:COMPUTERNAME;at=(Get-Date).ToUniversalTime().ToString('o');active=$active;gpu=@(& nvidia-smi --query-gpu=index,uuid,memory.free,utilization.gpu --format=csv,noheader,nounits)} | ConvertTo-Json -Depth 3'''
    if host == "haleyspc":
        result=json.loads(ssh_ps(script))
    else:
        encoded=base64.b64encode(script.encode("utf-16le")).decode()
        result=json.loads(subprocess.check_output(["powershell","-NoProfile","-EncodedCommand",encoded],text=True))
    if result["host"].upper() != HOSTNAMES[host] or result["active"]:
        raise ValueError("placement has a different host or competing native owner")
    rows={int(parts[0]):parts for parts in ([x.strip() for x in row.split(',')] for row in result["gpu"])}
    for placement in placements:
        row=rows[placement["gpu_ordinal"]]
        if row[1] != placement["gpu_uuid"] or int(row[2]) < 2048 or int(row[3]) > 5:
            raise ValueError("selected GPU is unavailable; preserve its current work")
    return result


def _dispatch_group(root, binary_pin, configs, placements, mode, stop, wall_seconds):
    staging_started=time.monotonic()
    if mode not in ["parallel","sequential"] or set(configs) != set(placements):
        raise ValueError("invalid dispatch group")
    if not 0 < wall_seconds <= 7200 or not re.fullmatch(r"[a-z0-9-]+",root.name):
        raise ValueError("invalid root name or wall cap")
    binary = checked(binary_pin)
    root.mkdir()
    for host in {p["host"] for p in placements.values()}:
        if host not in HOSTNAMES:
            raise ValueError("no qualified dispatcher for host")
        write(root/f"{host}-preflight.json",preflight(host,[p for p in placements.values() if p["host"]==host]))
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
        remote_root = f"C:/mtg-node/{root.name}" if host == "haleyspc" else str(local)
        native_binary = str(Path(remote_root)/"trainer.exe")
        shutil.copy2(binary,local/"trainer.exe")
        group = dict(mode=mode,jobs={})
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
            group["jobs"][name]=dict(placement=placement,wall_seconds=wall_seconds,
                binary=dict(path=native_binary,sha256=binary_pin["sha256"]),
                request=dict(path=str(Path(native_folder)/"request.json"),sha256=pin(folder/"request.json")["sha256"]))
        write(local/"group.json",group)
        if host == "haleyspc":
            ssh_ps(f"if (Test-Path -LiteralPath '{remote_root}') {{throw 'remote destination exists'}}; New-Item -ItemType Directory -Path '{remote_root}/jobs' | Out-Null")
            shutil.copy2(__file__,local/"worker.py")
            for name in group["jobs"]:
                ssh_ps(f"New-Item -ItemType Directory -Path '{remote_root}/jobs/{name}' | Out-Null")
                subprocess.run(["scp","-q",str(local/"jobs"/name/"request.json"),f"{REMOTE}:{remote_root}/jobs/{name}/request.json"],check=True,timeout=60)
            for name in ["trainer.exe","worker.py","group.json"]:
                subprocess.run(["scp","-q",str(local/name),f"{REMOTE}:{remote_root}/{name}"],check=True,timeout=60)
            ps=f'''$ErrorActionPreference='Stop'
$root='{remote_root}'
$inputRoot='{REMOTE_INPUT}'
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
 & python "$root/worker.py" --worker-group "$root/group.json"
 if ($LASTEXITCODE -ne 0) {{throw 'native worker group failed'}}
}} finally {{ foreach ($drive in $created) {{ & subst "${{drive}}:" /D }} }}
'''
            (local/"run.ps1").write_text(ps)
            subprocess.run(["scp","-q",str(local/"run.ps1"),f"{REMOTE}:{remote_root}/run.ps1"],check=True,timeout=60)
        staged[host]=(local,remote_root,group)

    def run_host(host):
        local, remote_root, group=staged[host]
        if host == "jack":
            worker_group(local/"group.json")
            return 0.0
        # Native children have their own wall cap even if SSH observation is interrupted.
        command=["ssh","-o","BatchMode=yes","-o","ServerAliveInterval=15","-o","ServerAliveCountMax=4",REMOTE,
                 "powershell","-NoProfile","-ExecutionPolicy","Bypass","-File",remote_root+"/run.ps1"]
        before=time.monotonic()
        with (local/"ssh.stdout").open("x") as out,(local/"ssh.stderr").open("x") as err:
            result=subprocess.run(command,stdout=out,stderr=err,timeout=wall_seconds*len(group["jobs"])+120)
        write(local/"ssh-execution.json",dict(exit_code=result.returncode,seconds=time.monotonic()-before))
        if result.returncode:
            raise RuntimeError("remote worker group failed; preserve and inspect remote state")
        recovery_started=time.monotonic()
        exported=subprocess.run(["ssh","-o","BatchMode=yes",REMOTE,"python",remote_root+"/worker.py","--export",remote_root],capture_output=True,text=True,timeout=600)
        if exported.returncode:
            raise RuntimeError(exported.stderr)
        receipt=read_json_text(exported.stdout)
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
        return time.monotonic()-recovery_started

    staging_seconds=time.monotonic()-staging_started
    started=time.monotonic()
    if mode == "parallel":
        with ThreadPoolExecutor(max_workers=len(staged)) as pool:
            recoveries=list(pool.map(run_host,staged))
    else:
        recoveries=[run_host(host) for host in staged]
    reports={}
    for name, placement in placements.items():
        host=placement["host"]
        base=staged[host][0]
        folder=(base/"recovered" if host == "haleyspc" else base)/"jobs"/name
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
        staging_seconds=staging_seconds,recovery_seconds=sum(recoveries)))
    return pin(root/"group-benchmark.json")


def read_json_text(text):
    return json.loads(text)


def dispatch_qualification(root,binary,configs,placements,mode,updates=3):
    if not 1 <= updates <= 4 or len(configs) != 2:
        raise ValueError("qualification is a bounded two-arm prefix only")
    games=sum(len(batch) for item in configs.values() for batch in read(checked(item))["updates"][:updates])
    if games > 80:
        raise ValueError("qualification prefix exceeds 80 games")
    return _dispatch_group(root,binary,configs,placements,mode,updates,300)


def dispatch_qualified(root,binary,configs,choice_path,wall_seconds):
    from compute_throughput_v2 import require_allocation
    jobs={name:dict(config_sha256=item["sha256"],updates=len(read(checked(item))["updates"])) for name,item in configs.items()}
    selected=require_allocation(choice_path,binary["sha256"],jobs)
    return _dispatch_group(root,binary,configs,selected["placements"],selected["mode"],None,wall_seconds)


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--worker-group",type=Path)
    parser.add_argument("--export",type=Path)
    args=parser.parse_args()
    if args.worker_group:
        worker_group(args.worker_group)
    elif args.export:
        export_group(args.export)
    else:
        parser.error("choose a worker operation")
