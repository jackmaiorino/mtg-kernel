"""Weighted CPU dispatch fork; preserve v1 source pinned by completed runs.

Native worker, recovery format and integrity checks are unchanged. Only fixed
host assignment adds an optional integer job_weight. Never pool split matches.
"""


def _require(condition, message=None):
    """Launch integrity checks must survive Python optimization."""
    if not condition:
        if message is None:
            raise AssertionError()
        raise AssertionError(message())

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import json
import re
import shutil
import subprocess
import time
import zipfile
from public_evaluation_dispatch_v1 import (
    read, write, pin, checked, inventory, worker_group, ssh, REMOTE, HOSTS,
)


def dispatch(root, label, binary, jobs, allocation, remote, group_wall_seconds=900):
    _require((re.fullmatch(r"[a-z0-9-]+", label)))
    destination = root/label
    destination.mkdir()
    started = time.monotonic()
    staged = {}
    hosts = sorted(allocation)
    _require((set(hosts) <= set(HOSTS) and hosts))
    # Fixed weighted round robin changes placement only, never case content.
    cycle = []
    for host in hosts:
        weight = allocation[host].get("job_weight", 1)
        if type(weight) is not int or not 1 <= weight <= 8:
            raise ValueError("invalid bounded host weight")
        cycle.extend([host] * weight)
    assigned = {host: [] for host in hosts}
    for index, item in enumerate(jobs):
        assigned[cycle[index % len(cycle)]].append(item)
    if any(not items for items in assigned.values()):
        raise ValueError("allocation has a host without work")
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
        native = Path(f"{drive}:/mtg-state-prevention-eval/{root.name}/{label}") if host == "jack" else Path(remote["native_root"])/label
        if host == "jack":
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
            if host == "jack":
                native_folder.mkdir(parents=True)
                shutil.copy2(folder/"request.json", native_folder/"request.json")
            spec["jobs"].append(dict(id=identifier, native_directory=str(native_folder),
                request=pin(folder/"request.json"),
                native_request=dict(path=str(native_folder/"request.json"), sha256=pin(folder/"request.json")["sha256"])))
        write(canonical/"spec.json", spec)
        if host == "jack":
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
        if host == "jack":
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
        if host == "jack":
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

