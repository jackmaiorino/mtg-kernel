"""Small immutable package and receipt helpers. No work on import."""
from __future__ import annotations
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import time
import uuid


def require(ok, message):
    if not ok:
        raise ValueError(message)


def unique_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result


def read(path):
    def invalid(value):
        raise ValueError('non-finite JSON number')
    return json.loads(Path(path).read_bytes(), object_pairs_hook=unique_pairs,
                      parse_constant=invalid)


def encoded(value):
    return (json.dumps(value, sort_keys=True, indent=2, allow_nan=False) + '\n').encode()


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1048576), b''):
            digest.update(chunk)
    return digest.hexdigest()


def pin(path, expected=None):
    path = Path(path)
    require(path.is_file() and not path.is_symlink(), 'regular input file required')
    digest = sha(path)
    require(expected is None or digest == expected, 'input SHA256 differs: ' + str(path))
    return {'path': str(path), 'sha256': digest, 'bytes': path.stat().st_size}


def write(path, value, replace=False):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex + '.partial')
    with temporary.open('xb') as stream:
        stream.write(encoded(value)); stream.flush(); os.fsync(stream.fileno())
    if replace:
        os.replace(temporary, path)
    else:
        try:
            os.link(temporary, path)
        finally:
            temporary.unlink()


def relative(value):
    result = PurePosixPath(value)
    require(not result.is_absolute() and '..' not in result.parts and
            str(result) not in ('', '.') and '\\' not in value, 'safe relative path required')
    return result


# ---- Funding snapshot freshness --------------------------------------------
# Shared by prepare_volume.py and prepare_lease.py's --execute paths. Kept here
# rather than in either module so neither must import the other (prepare_volume
# already imports KEY_PLACEHOLDER from prepare_lease).
FUNDING_SNAPSHOT_MAX_AGE_SECONDS = 30 * 60


def funding_snapshot_age_seconds(snapshot, now):
    require(isinstance(snapshot, dict) and isinstance(snapshot.get('observed_utc'), str),
            'funding snapshot missing observed_utc')
    try:
        observed = datetime.fromisoformat(snapshot['observed_utc'])
    except ValueError:
        raise ValueError('funding snapshot observed_utc is not a valid ISO-8601 timestamp') from None
    require(observed.tzinfo is not None, 'funding snapshot observed_utc must be timezone-aware')
    return now - observed.timestamp()


def require_fresh_funding_snapshot(path, now=None):
    """Refuse to proceed without a funding snapshot read within the last
    thirty minutes. Mirrors lease_guard.funding_status's conservative
    freshness discipline. Never inspects or returns the API key."""
    now = time.time() if now is None else now
    snapshot = read(path)
    age = funding_snapshot_age_seconds(snapshot, now)
    require(0 <= age <= FUNDING_SNAPSHOT_MAX_AGE_SECONDS,
            'funding snapshot is stale (older than thirty minutes) or has an invalid future timestamp')
    return snapshot
