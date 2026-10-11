"""Live physical-device checks for the explicitly selected desktop-local mode."""
import json
import os
from pathlib import Path
import platform
import subprocess


def device(path):
    path = Path(path)
    if os.name != 'nt' or platform.node().upper() != 'DESKTOP-DJ1C40R':
        raise ValueError('desktop-local recovery requires Jack desktop Windows')
    drive = path.drive.upper()
    if drive not in ('D:', 'E:'):
        raise ValueError('desktop-local recovery requires D: or E:')
    # The drive letter is restricted above, never interpolated from arbitrary input.
    script = ("$p = Get-Partition -DriveLetter " + drive[0] + " -ErrorAction Stop; "
              "$d = $p | Get-Disk -ErrorAction Stop; "
              "[pscustomobject]@{drive=$p.DriveLetter + ':'; partition_guid=[string]$p.Guid; "
              "disk_number=$d.Number; disk_unique_id=[string]$d.UniqueId; "
              "disk_serial=([string]$d.SerialNumber).Trim()} | ConvertTo-Json -Compress")
    result = subprocess.run(['powershell', '-NoProfile', '-NonInteractive', '-Command', script],
                            check=True, capture_output=True, text=True)
    identity = json.loads(result.stdout)
    if not identity['disk_unique_id'] or not identity['partition_guid']:
        raise ValueError('missing physical disk identity')
    identity['host'] = platform.node().upper()
    return identity


def independent(source, destination):
    if (source['host'] != destination['host'] or source['drive'] != 'D:'
            or destination['drive'] != 'E:'
            or source['disk_unique_id'].casefold() == destination['disk_unique_id'].casefold()
            or source['disk_number'] == destination['disk_number']):
        raise ValueError('desktop recovery requires distinct physical D: and E: devices')


def verify(source_root, destination_root, source_identity, destination_identity):
    actual_source, actual_destination = device(source_root), device(destination_root)
    if actual_source != source_identity or actual_destination != destination_identity:
        raise ValueError('desktop physical device identity changed')
    independent(actual_source, actual_destination)
