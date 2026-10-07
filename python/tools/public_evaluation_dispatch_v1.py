"""CPU evaluation workers with explicit storage, remote recovery and launch checks."""


def _require(condition, message=None):
    """Launch integrity checks must survive Python optimization."""
    if not condition:
        if message is None:
            raise AssertionError()
        raise AssertionError(message())

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
import time
import zipfile

REMOTE = os.environ.get('COMPUTE_HOST_SSH', 'compute-host')
HOSTS = {"desktop": "DESKTOP-DJ1C40R", "computehost": "COMPUTEHOST"}


def read(path): return json.loads(Path(path).read_bytes())
def pin(path):
    with Path(path).open("rb") as stream:
        return dict(path=str(path), sha256=hashlib.file_digest(stream, "sha256").hexdigest())
def checked(item):
    _require((pin(item["path"])["sha256"] == item["sha256"]), lambda: (item["path"]))
    return Path(item["path"])
def write(path, value):
    with Path(path).open("x") as stream: json.dump(value, stream, indent=2, allow_nan=False)


def ssh(script, timeout=60):
    encoded = base64.b64encode(("$ErrorActionPreference='Stop'\n$ProgressPreference='SilentlyContinue'\n"+script).encode("utf-16le")).decode()
    result = subprocess.run(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", REMOTE,
        "powershell", "-NoProfile", "-EncodedCommand", encoded], capture_output=True, text=True, timeout=timeout)
    _require((result.returncode == 0), lambda: (result.stderr))
    return result.stdout


def inventory(host):
    script = r'''$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
$cpu=Get-CimInstance Win32_Processor
$os=Get-CimInstance Win32_OperatingSystem
$active=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '^trainer\.exe$|public_feature_training|public-evaluator|cargo|rustc'} | Select-Object Name,ProcessId,CreationDate,CommandLine)
[pscustomobject]@{at=(Get-Date).ToUniversalTime().ToString('o');host=$env:COMPUTERNAME;cpu=@($cpu | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors);free_memory_kib=$os.FreePhysicalMemory;active=$active;disks=@(Get-Disk | Select-Object Number,FriendlyName,SerialNumber,BusType,Size);volumes=@(Get-Volume | Where-Object {$_.DriveLetter -in @('C','D','E')} | Select-Object DriveLetter,FileSystem,Size,SizeRemaining);partitions=@(Get-Partition | Where-Object {$_.DriveLetter -in @('C','D','E')} | Select-Object DriveLetter,DiskNumber);gpu=@(& nvidia-smi --query-gpu=index,uuid,utilization.gpu,memory.used --format=csv,noheader,nounits)} | ConvertTo-Json -Depth 5'''
    if host == "computehost":
        result = json.loads(ssh(script))
    else:
        encoded = base64.b64encode(script.encode("utf-16le")).decode()
        result = json.loads(subprocess.check_output(["powershell", "-NoProfile", "-EncodedCommand", encoded], text=True))
    _require((result["host"].upper() == HOSTS[host]))
    return result


def worker_group(spec_path):
    spec = read(spec_path)
    _require((platform.node().upper() == HOSTS[spec["host"]]))
    _require((1 <= spec["workers"] <= os.cpu_count()))
    native_binary = checked(spec["native_binary"])
    _require((spec["native_binary"]["sha256"] == spec["binary"]["sha256"]))
    group_start = time.monotonic()
    def job(item):
        folder = Path(item["native_directory"])
        request_path = checked(item["native_request"])
        _require((item["request"]["sha256"] == item["native_request"]["sha256"]))
        remaining = spec["group_wall_seconds"]-(time.monotonic()-group_start)
        _require((remaining > 0), lambda: ("evaluation group wall cap reached before launch"))
        before = time.monotonic()
        with (folder/"stdout").open("x") as out, (folder/"stderr").open("x") as err:
            child = subprocess.Popen([str(native_binary), str(request_path)], stdout=out, stderr=err,
                creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | subprocess.CREATE_NO_WINDOW)
            write(folder/"started.json", dict(pid=child.pid, host=spec["host"], started_unix=time.time()))
            timeout = False
            try:
                code = child.wait(timeout=min(remaining, spec["job_wall_seconds"]))
            except subprocess.TimeoutExpired:
                timeout = True
                child.kill()
                code = child.wait()
        execution = dict(exit_code=code, timeout=timeout, seconds=time.monotonic()-before,
            binary=spec["binary"], request=item["request"],
            actual_command=[str(native_binary), str(request_path)], native_request=item["native_request"],
            native_binary=spec["native_binary"], host=spec["host"], storage=spec["storage"])
        write(folder/"execution.json", execution)
        _require((code == 0 and not timeout), lambda: ((item["id"], execution)))
    errors = []
    with ThreadPoolExecutor(max_workers=spec["workers"]) as pool:
        futures = [pool.submit(job, item) for item in spec["jobs"]]
        for future in futures:
            try: future.result()
            except Exception as error: errors.append(str(error))
    write(Path(spec_path).parent/"worker-completion.json", dict(host=spec["host"], hostname=platform.node(), workers=spec["workers"],
        jobs=len(spec["jobs"]), seconds=time.monotonic()-group_start, errors=errors))
    _require((not errors), lambda: (errors))


def export(root):
    files = [p for p in sorted(root.rglob("*")) if p.is_file() and p.name not in ["results.zip", "export-manifest.json"]]
    write(root/"export-manifest.json", {p.relative_to(root).as_posix(): pin(p)["sha256"] for p in files})
    with zipfile.ZipFile(root/"results.zip", "x", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for path in files: archive.write(path, path.relative_to(root).as_posix())
        archive.write(root/"export-manifest.json", "export-manifest.json")
    print(json.dumps(dict(files=len(files), sha256=pin(root/"results.zip")["sha256"])))


def prepare_remote(root, assets):
    """Copy known qualified base inputs to a fresh namespace, then add new endpoints."""
    _require((re.fullmatch(r"[a-z0-9-]+", root.name)))
    native = f"C:/mtg-node/{root.name}"
    before = time.monotonic()
    stage = root/"remote-staging"
    stage.mkdir()
    files = {"worker.py": pin(__file__)}
    for item in assets:
        path = checked(item)
        _require((path.drive.upper() in ["D:", "E:"]))
        relative = "inputs/"+path.drive[0].upper()+"/"+"/".join(path.parts[1:])
        files[relative] = item
    archive_path = stage/"inputs.zip"
    with zipfile.ZipFile(archive_path, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for relative, item in files.items(): archive.write(checked(item), relative)
        archive.writestr("new-input-hashes.json", json.dumps({name:item["sha256"] for name,item in files.items()}))
    ssh(f"if (Test-Path -LiteralPath '{native}') {{throw 'remote root exists'}}; New-Item -ItemType Directory -Path '{native}/inputs' | Out-Null")
    subprocess.run(["scp", "-q", str(archive_path), f"{REMOTE}:{native}/inputs.zip"], check=True, timeout=120)
    expected = pin(archive_path)["sha256"]
    response = ssh(f'''$root='{native}'
if ((Get-FileHash -LiteralPath "$root/inputs.zip" -Algorithm SHA256).Hash.ToLower() -ne '{expected}') {{throw 'staged archive differs'}}
foreach ($drive in @('D','E')) {{ Copy-Item -LiteralPath "C:/mtg-node/public-device-placement-001/$drive" -Destination "$root/inputs/$drive" -Recurse }}
Expand-Archive -LiteralPath "$root/inputs.zip" -DestinationPath "$root" -Force
$map=Get-Content -LiteralPath "$root/new-input-hashes.json" -Raw | ConvertFrom-Json
foreach ($entry in $map.PSObject.Properties) {{if ((Get-FileHash -LiteralPath "$root/$($entry.Name)" -Algorithm SHA256).Hash.ToLower() -ne $entry.Value) {{throw 'new input bytes differ'}}}}
[pscustomobject]@{{files=$map.PSObject.Properties.Count;verified=$true}} | ConvertTo-Json''', timeout=180)
    result = dict(native_root=native, seconds=time.monotonic()-before, assets=files,
        archive=pin(archive_path), remote_verification=json.loads(response), base_input_root="C:/mtg-node/public-device-placement-001")
    write(root/"remote-staging.json", result)
    return result


def dispatch(root, label, binary, jobs, allocation, remote, group_wall_seconds=900):
    _require((re.fullmatch(r"[a-z0-9-]+", label)))
    destination = root/label
    destination.mkdir()
    started = time.monotonic()
    staged = {}
    hosts = sorted(allocation)
    _require((set(hosts) <= set(HOSTS) and hosts))
    # Stable alternating job assignment balances arms and representative case groups.
    assigned = {host: jobs[i::len(hosts)] for i,host in enumerate(hosts)}
    for host, settings in allocation.items():
        current = inventory(host)
        _require((not current["active"]), lambda: ("preserve competing native work"))
        write(destination/f"{host}-inventory.json", current)
        drive = settings["drive"]
        partition = next(p for p in current["partitions"] if p["DriveLetter"] == drive)
        disk = next(d for d in current["disks"] if d["Number"] == partition["DiskNumber"])
        _require((disk["SerialNumber"] == settings["disk_serial"] and disk["FriendlyName"] == settings["disk_name"]))
        volume = next(v for v in current["volumes"] if v["DriveLetter"] == drive)
        _require((volume["SizeRemaining"] > 10*1024**3))
        canonical = destination/host
        canonical.mkdir()
        native = Path(f"{drive}:/mtg-state-prevention-eval/{root.name}/{label}") if host == "desktop" else Path(remote["native_root"])/label
        if host == "desktop":
            native.mkdir(parents=True)
            shutil.copy2(checked(binary), native/"public-evaluator.exe")
        else:
            ssh(f"New-Item -ItemType Directory -Path '{native.as_posix()}' | Out-Null")
            subprocess.run(["scp", "-q", str(checked(binary)), f"{REMOTE}:{native.as_posix()}/public-evaluator.exe"], check=True, timeout=60)
        spec = dict(host=host, storage=settings, workers=settings["workers"], binary=binary,
            native_binary=dict(path=str(native/"public-evaluator.exe"), sha256=binary["sha256"]),
            group_wall_seconds=group_wall_seconds, job_wall_seconds=300, jobs=[])
        for item in assigned[host]:
            identifier = item["id"]
            _require((re.fullmatch(r"[a-zA-Z0-9-]+", identifier)))
            folder = canonical/"jobs"/identifier
            folder.mkdir(parents=True)
            native_folder = native/"jobs"/identifier
            request = dict(item["command"], output_directory=str(native_folder/"outputs"))
            write(folder/"request.json", request)
            if host == "desktop":
                native_folder.mkdir(parents=True)
                shutil.copy2(folder/"request.json", native_folder/"request.json")
            spec["jobs"].append(dict(id=identifier, native_directory=str(native_folder),
                request=pin(folder/"request.json"),
                native_request=dict(path=str(native_folder/"request.json"), sha256=pin(folder/"request.json")["sha256"])))
        write(canonical/"spec.json", spec)
        if host == "desktop":
            shutil.copy2(canonical/"spec.json", native/"spec.json")
        else:
            bundle = canonical/"requests.zip"
            with zipfile.ZipFile(bundle, "x", compression=zipfile.ZIP_DEFLATED) as archive:
                archive.write(canonical/"spec.json", "spec.json")
                for item in assigned[host]: archive.write(canonical/"jobs"/item["id"]/"request.json", f"jobs/{item['id']}/request.json")
            subprocess.run(["scp", "-q", str(bundle), f"{REMOTE}:{native.as_posix()}/requests.zip"], check=True, timeout=60)
            ssh(f"Expand-Archive -LiteralPath '{native.as_posix()}/requests.zip' -DestinationPath '{native.as_posix()}'")
        staged[host] = (canonical, native, spec)
    staging_seconds = time.monotonic()-started
    execute_start = time.monotonic()
    def run_host(host):
        canonical, native, spec = staged[host]
        if host == "desktop":
            worker_group(native/"spec.json")
        else:
            script = f'''$created=@()
$inputRoot='{remote['native_root']}/inputs'
try {{
 foreach ($drive in @('D','E')) {{
  if (Test-Path -LiteralPath "${{drive}}:/") {{throw 'existing drive; preserve owner'}}
  & subst "${{drive}}:" "$inputRoot/$drive"
  if ($LASTEXITCODE -ne 0) {{throw 'mapping failed'}}
  $created+=$drive
 }}
 & python '{remote['native_root']}/worker.py' --worker '{native.as_posix()}/spec.json'
 if ($LASTEXITCODE -ne 0) {{throw 'evaluation worker failed'}}
}} finally {{ foreach ($drive in $created) {{ & subst "${{drive}}:" /D }} }}'''
            (canonical/"remote-command.ps1").write_text(script)
            response = ssh(script, timeout=group_wall_seconds+60)
            (canonical/"ssh.stdout").write_text(response)
        return host
    with ThreadPoolExecutor(max_workers=len(hosts)) as pool:
        results = list(pool.map(run_host, hosts))
    execution_seconds = time.monotonic()-execute_start
    recovery_start = time.monotonic()
    output_jobs, fingerprints, workers = [], {}, {}
    for host in results:
        canonical, native, spec = staged[host]
        recovered = canonical/"recovered"
        if host == "desktop":
            shutil.copytree(native, recovered)
            native_hashes = {p.relative_to(native).as_posix():pin(p)["sha256"] for p in native.rglob("*") if p.is_file()}
            _require((all(pin(recovered/name)["sha256"] == digest for name,digest in native_hashes.items())))
            write(canonical/"recovery.json", dict(files=len(native_hashes), mismatches=0, native_directory=str(native), hashes=native_hashes))
        else:
            exported = json.loads(ssh(f"& python '{remote['native_root']}/worker.py' --export '{native.as_posix()}'", timeout=180))
            subprocess.run(["scp", "-q", f"{REMOTE}:{native.as_posix()}/results.zip", str(canonical/"results.zip")], check=True, timeout=180)
            _require((pin(canonical/"results.zip")["sha256"] == exported["sha256"]))
            recovered.mkdir()
            with zipfile.ZipFile(canonical/"results.zip") as archive:
                _require((all((recovered/name).resolve().is_relative_to(recovered.resolve()) for name in archive.namelist())))
                archive.extractall(recovered)
            native_hashes = read(recovered/"export-manifest.json")
            _require((all(pin(recovered/name)["sha256"] == digest for name,digest in native_hashes.items())))
            write(canonical/"recovery.json", dict(files=len(native_hashes), mismatches=0, native_directory=str(native), export=exported, hashes=native_hashes))
        for item in assigned[host]:
            folder = recovered/"jobs"/item["id"]
            request = read(folder/"request.json")
            _require((pin(folder/"request.json")["sha256"] == pin(canonical/"jobs"/item["id"]/"request.json")["sha256"]))
            completion = read(folder/"outputs/completion.json")
            _require((completion["matches"] == len(item["command"]["matches"]) == len(completion["match_sha256"])))
            games = decisions = 0
            for index, digest in enumerate(completion["match_sha256"]):
                path = folder/f"outputs/match-{index:06}.json"
                _require((pin(path)["sha256"] == digest))
                match = read(path)
                _require((match["match"] == request["matches"][index] and not match["decisions"]))
                games += len(match["games"])
                decisions += match["decision_count"]
                fingerprints[f"{item['id']}/{index}"] = digest
            _require((games == completion["natural_games"] and decisions == completion["decisions"]))
            output_jobs.append(dict(id=item["id"], arm=item["arm"], label=item["label"],
                request=pin(canonical/"jobs"/item["id"]/"request.json"), execution=pin(folder/"execution.json"),
                output_directory=str(folder/"outputs"), host=host, recovery=pin(canonical/"recovery.json")))
        workers[host] = pin(recovered/"worker-completion.json")
    result = dict(allocation=allocation, staging_seconds=staging_seconds, execution_seconds=execution_seconds,
        recovery_seconds=time.monotonic()-recovery_start, jobs=output_jobs, workers=workers, matches=len(fingerprints), fingerprints=fingerprints)
    write(destination/"result.json", result)
    return pin(destination/"result.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--worker", type=Path)
    parser.add_argument("--export", type=Path)
    args = parser.parse_args()
    if args.worker: worker_group(args.worker)
    elif args.export: export(args.export)
    else: parser.error("select worker or export")
