"""Bounded CPU replay dispatch with identical inputs and verified result recovery."""
import json
from pathlib import Path
import re
import shutil
import subprocess
import time
import zipfile

from public_evaluation_dispatch_v1 import (
    REMOTE, read, write, pin, checked, inventory, ssh, worker_group,
)


def dispatch(root, label, binary, command, host, remote, expected_failure=False):
    assert host in ["jack", "haleyspc"] and re.fullmatch(r"[a-z0-9-]+", label)
    assert 1 <= command["workers"] <= 8
    current = inventory(host)
    assert not current["active"], "Preserve existing native owners"
    before = time.monotonic()
    destination = root/label
    destination.mkdir()
    write(destination/"inventory.json", current)
    part = next(p for p in current["partitions"] if p["DriveLetter"] == "C")
    disk = next(d for d in current["disks"] if d["Number"] == part["DiskNumber"])
    volume = next(v for v in current["volumes"] if v["DriveLetter"] == "C")
    assert volume["SizeRemaining"] > 10*1024**3
    storage = dict(drive="C", disk_serial=disk["SerialNumber"], disk_name=disk["FriendlyName"])
    native = Path(f"C:/mtg-policy-replay/{root.name}/{label}") if host == "jack" else Path(remote["native_root"])/label
    request = dict(command, output_directory=str(native/"job/outputs"))
    write(destination/"request.json", request)
    binary_path = checked(binary)
    spec = dict(host=host, storage=storage, workers=1, binary=binary,
        native_binary=dict(path=str(native/"public-replay.exe"), sha256=binary["sha256"]),
        group_wall_seconds=180, job_wall_seconds=180, jobs=[dict(id="audit", native_directory=str(native/"job"),
        request=pin(destination/"request.json"), native_request=dict(path=str(native/"job/request.json"), sha256=pin(destination/"request.json")["sha256"]))])
    write(destination/"spec.json", spec)
    if host == "jack":
        (native/"job").mkdir(parents=True)
        shutil.copy2(binary_path, native/"public-replay.exe")
        shutil.copy2(destination/"request.json", native/"job/request.json")
        shutil.copy2(destination/"spec.json", native/"spec.json")
    else:
        bundle = destination/"request.zip"
        with zipfile.ZipFile(bundle, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(binary_path, "public-replay.exe")
            archive.write(destination/"request.json", "job/request.json")
            archive.write(destination/"spec.json", "spec.json")
        ssh(f"if (Test-Path -LiteralPath '{native.as_posix()}') {{throw 'native root exists'}}; New-Item -ItemType Directory -Path '{native.as_posix()}' | Out-Null")
        subprocess.run(["scp", "-q", str(bundle), f"{REMOTE}:{native.as_posix()}/request.zip"], check=True, timeout=60)
        ssh(f"if ((Get-FileHash -LiteralPath '{native.as_posix()}/request.zip' -Algorithm SHA256).Hash.ToLower() -ne '{pin(bundle)['sha256']}') {{throw 'request transfer differs'}}; Expand-Archive -LiteralPath '{native.as_posix()}/request.zip' -DestinationPath '{native.as_posix()}'")
    staging_seconds = time.monotonic()-before
    begin = time.monotonic()
    if host == "jack":
        try:
            worker_group(native/"spec.json")
        except AssertionError:
            if not expected_failure: raise
    else:
        script = f'''$created=@()
$inputRoot='{remote['native_root']}/inputs'
$workerCode=$null
try {{
 foreach ($drive in @('D','E')) {{
  if (Test-Path -LiteralPath "${{drive}}:/") {{throw 'existing drive; preserve owner'}}
  & subst "${{drive}}:" "$inputRoot/$drive"
  if ($LASTEXITCODE -ne 0) {{throw 'mapping failed'}}
  $created+=$drive
 }}
 $worker=Start-Process -FilePath python -ArgumentList @('{remote['native_root']}/worker.py','--worker','{native.as_posix()}/spec.json') -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput '{native.as_posix()}/worker.stdout' -RedirectStandardError '{native.as_posix()}/worker.stderr'
 $workerCode=$worker.ExitCode
}} finally {{ foreach ($drive in $created) {{ & subst "${{drive}}:" /D }} }}
[pscustomobject]@{{exit_code=$workerCode}} | ConvertTo-Json'''
        # Native failure is recovered below, including expected negative qualification.
        (destination/"remote-command.ps1").write_text(script)
        (destination/"ssh.stdout").write_text(ssh(script, timeout=240))
    execution_seconds = time.monotonic()-begin
    begin = time.monotonic()
    recovered = destination/"recovered"
    if host == "jack":
        shutil.copytree(native, recovered)
        hashes = {p.relative_to(native).as_posix():pin(p)["sha256"] for p in native.rglob("*") if p.is_file()}
    else:
        exported = json.loads(ssh(f"& python '{remote['native_root']}/worker.py' --export '{native.as_posix()}'", timeout=180))
        archive_path = destination/"results.zip"
        subprocess.run(["scp", "-q", f"{REMOTE}:{native.as_posix()}/results.zip", str(archive_path)], check=True, timeout=180)
        assert pin(archive_path)["sha256"] == exported["sha256"]
        recovered.mkdir()
        with zipfile.ZipFile(archive_path) as archive:
            assert all((recovered/name).resolve().is_relative_to(recovered.resolve()) for name in archive.namelist())
            archive.extractall(recovered)
        hashes = read(recovered/"export-manifest.json")
    assert all(pin(recovered/name)["sha256"] == digest for name,digest in hashes.items())
    write(destination/"recovery.json", dict(files=len(hashes), mismatches=0, hashes=hashes))
    execution = read(recovered/"job/execution.json")
    assert execution["binary"] == binary and execution["request"] == pin(destination/"request.json")
    assert pin(recovered/"job/request.json")["sha256"] == execution["request"]["sha256"]
    assert not execution["timeout"]
    output = recovered/"job/outputs"
    fingerprints = {}
    if expected_failure:
        assert execution["exit_code"] != 0 and not output.exists()
    else:
        assert execution["exit_code"] == 0
        completion = read(output/"completion.json")
        assert completion["trajectories"] == len(command["trajectories"])
        assert completion["terminal_outcomes_used"] is False
        for i,item in enumerate(command["trajectories"]):
            path = output/f"trajectory-{i:03}.json"
            assert read(path)["trajectory"] == item
            fingerprints[path.name] = pin(path)["sha256"]
        fingerprints["completion.json"] = pin(output/"completion.json")["sha256"]
    result = dict(host=host, workers=command["workers"], binary=binary, storage=storage,
        request=pin(destination/"request.json"), execution=pin(recovered/"job/execution.json"),
        output_directory=str(output), stderr=str(recovered/"job/stderr"), expected_failure=expected_failure,
        fingerprints=fingerprints, recovery=pin(destination/"recovery.json"), staging_seconds=staging_seconds,
        execution_seconds=execution_seconds, recovery_seconds=time.monotonic()-begin, seconds=time.monotonic()-before)
    write(destination/"result.json", result)
    return result
