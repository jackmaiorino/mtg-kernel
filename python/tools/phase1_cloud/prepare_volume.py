"""Offline create-request preparation for a RunPod network volume, plus a
read-only --verify mode and a strictly gated --execute mode.

Does not query or mutate RunPod unless --execute is passed. Inject the real
RUNPOD_API_KEY only in-memory, only inside execute_create/verify_volume, only
in the Authorization header of the single live request. Never save or print a
request or response containing the actual key value, and never persist a raw
provider response: only sanitize_volume's four fields are written to disk.

RunPod API reference relied on (retrieved 2026-09-15T15:44:06Z from
https://docs.runpod.io/llms.txt and the pages it links):

  POST https://rest.runpod.io/v1/networkvolumes
    request body (https://docs.runpod.io/api-reference/network-volumes/POST/networkvolumes):
      name (string, required), size (integer GB, required, 0-4000),
      dataCenterId (string, required, e.g. "EU-RO-1")
    response body: id (string), name (string), size (integer GB), dataCenterId (string)

  GET https://rest.runpod.io/v1/networkvolumes/{networkVolumeId}
    (https://docs.runpod.io/api-reference/network-volumes/GET/networkvolumes/networkVolumeId)
    response body (NetworkVolume): id, name, size (integer GB), dataCenterId

  Pricing (retrieved 2026-09-15T15:44:06Z from https://docs.runpod.io/pricing):
    network volume storage $0.07/GB/month under 1TB, $0.05/GB/month at or over 1TB;
    billed whether the attached Pod is running or stopped.

Both endpoints use the same "Authorization: Bearer <key>" scheme and base URL
as lease_guard.Provider's pod calls. A network volume has no runtime env of its
own (unlike a Pod), so the key placeholder from prepare_lease.py appears in the
saved template's headers, not its body.
"""
from __future__ import annotations
import argparse
import json
import math
import os
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
from datetime import datetime
from common import encoded, pin, read, require, write
from prepare_lease import KEY_PLACEHOLDER

SPEC_SCHEMA = 'phase1-cloud-volume-preparation/v1'
RESULT_SCHEMA = 'phase1-cloud-volume-preparation-result/v1'
CREATED_SCHEMA = 'phase1-cloud-volume-created/v1'
VERIFY_SCHEMA = 'phase1-cloud-volume-verification/v1'

VOLUMES_URL = 'https://rest.runpod.io/v1/networkvolumes'
USER_AGENT = 'phase1-volume-prepare/1'
VOLUME_NAME_PATTERN = '[a-zA-Z0-9-]+'
VOLUME_ID_PATTERN = '[A-Za-z0-9]+'

# Must match prepare_lease.py's body['dataCenterIds'] == ['EU-RO-1']: a network
# volume is created in one fixed data center and a Pod can only attach a volume
# from its own data center, so this tool never accepts a different target.
DATA_CENTER_ID = 'EU-RO-1'

# ---- Size bound -----------------------------------------------------------
# Smallest whole-GB size that fits the staged payloads plus recovery headroom,
# derived from measured evidence rather than guessed, then hard-capped.
#
# PACKAGE_BYTES_ESTIMATE: FRESH-CLOUD-RESULTS-001.md's closing accounting
# ("Closing accounting fresh-cloud-close-001.json ... this tranche added about
# 201.4 native seconds and 1,391,298,940 exported bytes (six hot roots under
# fresh-cloud-checks-001/exported/)") exported six packages totalling
# 1,391,298,940 bytes: 1,391,298,940 / 6 = 231,883,156.67 bytes, about 230 MB,
# per staged package.
PACKAGE_BYTES_ESTIMATE = 231_883_157

# PACKAGE_KINDS_PER_LEASE: the same results file's package table lists both
# ordinary-* and bo3-* labels sharing one run; prepare_lease.py's durable root
# ('/workspace/phase1/' + run_id) can hold an ordinary-training export and its
# BO3 continuation export side by side under one lease, so the staged-payload
# floor is two package-equivalents, not one.
PACKAGE_KINDS_PER_LEASE = 2

# RECOVERY_HEADROOM_MULTIPLIER: cloud_worker.copy_exact keeps existing durable
# bytes in place (it errors rather than overwrites on a hash mismatch) and
# common.write's atomic-replace pattern briefly holds an old file and its full
# replacement at once, so a resumed or retried lease can transiently need
# several payload-equivalents of room. lease_guard.validate caps a lease at
# eight hours (0 < deadline_epoch - created_epoch <= 8*3600), long enough for
# more update iterations than the two-update engineering fixtures measured
# above, so headroom also covers iteration growth within one lease.
RECOVERY_HEADROOM_MULTIPLIER = 4

# Hard ceiling regardless of any requested or computed size. RunPod's own
# documented range is 0-4000 GB; this qualification tool never asks for more
# than 50 GB, matching the project's qualification-scope ceilings elsewhere
# (lease_guard.validate: total_cap_usd <= 200).
VOLUME_SIZE_GB_CAP = 50

SIZING_BASIS = {
    'package_bytes_estimate': PACKAGE_BYTES_ESTIMATE,
    'package_bytes_source': (
        'FRESH-CLOUD-RESULTS-001.md closing accounting: 1,391,298,940 exported '
        'bytes across six hot roots = 231,883,156.67 bytes (about 230 MB) per package'),
    'package_kinds_per_lease': PACKAGE_KINDS_PER_LEASE,
    'recovery_headroom_multiplier': RECOVERY_HEADROOM_MULTIPLIER,
    'cap_gb': VOLUME_SIZE_GB_CAP,
}


def default_size_gb():
    """Smallest whole GB that fits PACKAGE_KINDS_PER_LEASE staged packages with
    RECOVERY_HEADROOM_MULTIPLIER headroom, floored at 1 GB and capped at
    VOLUME_SIZE_GB_CAP."""
    bytes_needed = PACKAGE_KINDS_PER_LEASE * RECOVERY_HEADROOM_MULTIPLIER * PACKAGE_BYTES_ESTIMATE
    size_gb = math.ceil(bytes_needed / 1_000_000_000)
    return max(1, min(size_gb, VOLUME_SIZE_GB_CAP))


# ---- Cost note --------------------------------------------------------------
STORAGE_USD_PER_GB_MONTH_UNDER_1TB = 0.07
STORAGE_USD_PER_GB_MONTH_OVER_1TB = 0.05
STORAGE_TIER_THRESHOLD_GB = 1000
# Conservative (higher) hourly figure: a 30-day month, not the longer average
# calendar month, so the derived storage_usd_hour never understates cost for
# the lease guard's storage line (lease_guard.decision folds storage_usd_hour
# into its own conservative rate).
HOURS_PER_MONTH = 30 * 24

PRICING_DOC_URL = 'https://docs.runpod.io/pricing'
PRICING_RETRIEVED_UTC = '2026-09-15T15:44:06Z'
API_REFERENCE = {
    'retrieved_utc': '2026-09-15T15:44:06Z',
    'create': {'method': 'POST', 'url': VOLUMES_URL,
               'documented_fields': ['name', 'size', 'dataCenterId'],
               'doc_url': 'https://docs.runpod.io/api-reference/network-volumes/POST/networkvolumes'},
    'get': {'method': 'GET', 'url': VOLUMES_URL + '/{networkVolumeId}',
            'documented_fields': ['id', 'name', 'size', 'dataCenterId'],
            'doc_url': 'https://docs.runpod.io/api-reference/network-volumes/GET/networkvolumes/networkVolumeId'},
    'pricing': {'doc_url': PRICING_DOC_URL,
                'storage_usd_per_gb_month_under_1tb': STORAGE_USD_PER_GB_MONTH_UNDER_1TB,
                'storage_usd_per_gb_month_over_1tb': STORAGE_USD_PER_GB_MONTH_OVER_1TB},
}


def storage_rate_usd_per_gb_month(size_gb):
    return (STORAGE_USD_PER_GB_MONTH_UNDER_1TB if size_gb <= STORAGE_TIER_THRESHOLD_GB
            else STORAGE_USD_PER_GB_MONTH_OVER_1TB)


def monthly_storage_usd(size_gb):
    return size_gb * storage_rate_usd_per_gb_month(size_gb)


def cost_note(size_gb):
    """Monthly storage price from the docs, converted to the per-hour figure
    that belongs on the lease guard's storage_usd_hour line."""
    monthly = monthly_storage_usd(size_gb)
    return {'size_gb': size_gb,
            'storage_usd_per_gb_month': storage_rate_usd_per_gb_month(size_gb),
            'hours_per_month_assumption': HOURS_PER_MONTH,
            'monthly_storage_usd': monthly,
            'storage_usd_hour': monthly / HOURS_PER_MONTH,
            'pricing_doc_url': PRICING_DOC_URL,
            'pricing_retrieved_utc': PRICING_RETRIEVED_UTC}


# ---- Offline template -------------------------------------------------------
def build_template(name, size_gb, data_center_id=DATA_CENTER_ID):
    require(re.fullmatch(VOLUME_NAME_PATTERN, name), 'invalid exact volume name')
    require(isinstance(size_gb, int) and not isinstance(size_gb, bool)
            and 1 <= size_gb <= VOLUME_SIZE_GB_CAP,
            'volume size must be a whole number of gigabytes within the qualification cap')
    require(data_center_id == DATA_CENTER_ID,
            'volume must target the same data center as prepare_lease.py (EU-RO-1)')
    return {'method': 'POST', 'url': VOLUMES_URL,
            'headers': {'Authorization': 'Bearer ' + KEY_PLACEHOLDER,
                        'Content-Type': 'application/json', 'User-Agent': USER_AGENT},
            'body': {'name': name, 'size': size_gb, 'dataCenterId': data_center_id}}


def prepare(spec, output, now=None):
    now = time.time() if now is None else now
    require(spec['schema'] == SPEC_SCHEMA, 'wrong volume preparation schema')
    name = spec['name']
    size_gb = spec.get('size_gb', default_size_gb())
    template = build_template(name, size_gb)
    output = Path(output)
    require(not output.exists(), 'fresh volume output required')
    output.mkdir(parents=True)
    write(output / 'create-request-template.json', template)
    result = {'schema': RESULT_SCHEMA, 'prepared_epoch': now,
              'data_center_id': DATA_CENTER_ID, 'requested_name': name,
              'size_gb': size_gb, 'size_default_used': 'size_gb' not in spec,
              'sizing_basis': SIZING_BASIS, 'cost_note': cost_note(size_gb),
              'create_request_template': pin(output / 'create-request-template.json'),
              'api_reference': API_REFERENCE,
              'allocated': False, 'provider_ttl': False,
              'before_execute': ['fetch a fresh funding snapshot (at most thirty minutes old)',
                  'confirm RUNPOD_API_KEY is exported in the shell environment only',
                  'rerun with --execute --funding-snapshot <path> to perform the live POST',
                  'record the returned volume id into the future lease spec as network_volume_id']}
    write(output / 'preparation.json', result)
    return result


def prepared_template(output):
    """Re-read and re-hash the saved template before any live call, so a live
    POST always matches exactly what prepare() wrote and pinned."""
    path = Path(output) / 'create-request-template.json'
    pin(path)
    return read(path)


# ---- Funding snapshot freshness --------------------------------------------
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


# ---- Live provider client (only reached through --execute) -----------------
class Provider:
    """Live network-volume calls. Constructed only inside execute_create and
    verify_volume, from a key already confirmed present in the environment, or
    injected as a mock in tests. prepare() and build_template() never touch it."""

    def __init__(self, key):
        require(key, 'runtime API key missing')
        self.key = key

    def create(self, name, size_gb, data_center_id):
        require(re.fullmatch(VOLUME_NAME_PATTERN, name), 'invalid exact volume name')
        require(isinstance(size_gb, int) and not isinstance(size_gb, bool)
                and 1 <= size_gb <= VOLUME_SIZE_GB_CAP, 'volume size out of qualification bound')
        body = json.dumps({'name': name, 'size': size_gb, 'dataCenterId': data_center_id}).encode()
        request = urllib.request.Request(VOLUMES_URL, data=body, method='POST',
            headers={'Authorization': 'Bearer ' + self.key, 'Content-Type': 'application/json',
                     'User-Agent': USER_AGENT})
        return self._call(request)

    def get(self, volume_id):
        require(re.fullmatch(VOLUME_ID_PATTERN, volume_id), 'invalid exact volume id')
        request = urllib.request.Request(VOLUMES_URL + '/' + volume_id, method='GET',
            headers={'Authorization': 'Bearer ' + self.key, 'User-Agent': USER_AGENT})
        return self._call(request)

    def _call(self, request):
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except urllib.error.HTTPError as error:
            code = error.code; error.close()
            if code == 404:
                return None
            raise RuntimeError('provider HTTP ' + str(code)) from None
        except (OSError, ValueError):
            raise RuntimeError('provider request unavailable') from None


def sanitize_volume(raw):
    """Keep only the four documented fields. The raw provider response is
    never written to disk or printed."""
    require(isinstance(raw, dict), 'volume response malformed')
    for field in ('id', 'name', 'size', 'dataCenterId'):
        require(field in raw, 'volume response missing ' + field)
    require(isinstance(raw['id'], str) and isinstance(raw['name'], str)
            and isinstance(raw['dataCenterId'], str), 'invalid volume field types')
    require(isinstance(raw['size'], int) and not isinstance(raw['size'], bool) and raw['size'] >= 0,
            'invalid volume size in response')
    return {'id': raw['id'], 'name': raw['name'], 'size_gb': raw['size'],
            'data_center_id': raw['dataCenterId']}


def _require_execute_key():
    key = os.environ.get('RUNPOD_API_KEY')
    require(key, 'RUNPOD_API_KEY must be set in the environment for --execute')
    return key


def execute_create(template, output, funding_snapshot_path, api=None, now=None):
    """Live creation call. Reachable only when RUNPOD_API_KEY is set and a
    funding snapshot no older than thirty minutes is supplied. `api` is
    exercised only through a mocked client in tests; this function never
    performs a live call outside of --execute."""
    now = time.time() if now is None else now
    key = _require_execute_key()
    require(funding_snapshot_path, '--funding-snapshot is required with --execute')
    require_fresh_funding_snapshot(funding_snapshot_path, now)
    body = template['body']
    api = api or Provider(key)
    raw = api.create(body['name'], body['size'], body['dataCenterId'])
    sanitized = sanitize_volume(raw)
    result = {'schema': CREATED_SCHEMA, 'created_epoch': now, 'requested': body,
              **sanitized, 'cost_note': cost_note(sanitized['size_gb'])}
    write(Path(output) / 'created-volume.json', result, replace=True)
    return result


def verify_volume(volume_id, output, funding_snapshot_path, api=None, now=None):
    """Live read-only call. Fetches and records only the sanitized size, data
    center and name; the funding-snapshot and key gates apply the same as
    execute_create even though a GET does not itself spend."""
    now = time.time() if now is None else now
    require(re.fullmatch(VOLUME_ID_PATTERN, volume_id), 'invalid exact volume id')
    key = _require_execute_key()
    require(funding_snapshot_path, '--funding-snapshot is required with --execute')
    require_fresh_funding_snapshot(funding_snapshot_path, now)
    api = api or Provider(key)
    raw = api.get(volume_id)
    require(raw is not None, 'volume not found')
    sanitized = sanitize_volume(raw)
    require(sanitized['id'] == volume_id, 'verified volume id differs from the requested id')
    result = {'schema': VERIFY_SCHEMA, 'verified_epoch': now, **sanitized,
              'matches_prepare_lease_data_center': sanitized['data_center_id'] == DATA_CENTER_ID,
              'cost_note': cost_note(sanitized['size_gb'])}
    write(Path(output) / ('verify-' + volume_id + '.json'), result, replace=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', help='offline volume-preparation spec path (create-template mode)')
    parser.add_argument('--verify', metavar='VOLUME_ID',
        help='read-only mode: fetch and record an existing volume by id (requires --execute)')
    parser.add_argument('--output', required=True)
    parser.add_argument('--execute', action='store_true',
        help='perform the live provider call; requires RUNPOD_API_KEY in the environment and --funding-snapshot')
    parser.add_argument('--funding-snapshot',
        help='path to a funding snapshot no older than thirty minutes; required with --execute')
    args = parser.parse_args()
    require(not (args.spec and args.verify), 'pass either --spec or --verify, not both')
    if args.verify:
        require(args.execute, '--verify has no offline action; pass --execute for its read-only live call')
        result = verify_volume(args.verify, args.output, args.funding_snapshot)
    else:
        result = prepare(read(args.spec), args.output) if args.spec else None
        if args.execute:
            result = execute_create(prepared_template(args.output), args.output, args.funding_snapshot)
        require(result is not None, 'nothing to do: pass --spec, --verify, or --execute against a prepared --output')
    print(encoded(result).decode())


if __name__ == '__main__':
    main()
