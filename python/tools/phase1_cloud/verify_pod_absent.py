"""Independent, read-only confirmation that a RunPod Pod no longer exists.

Performs only GET lookups. Never creates, starts, stops or deletes anything.
Does not query RunPod unless --execute is passed. Inject the real
RUNPOD_API_KEY only in-memory, only inside Provider.lookup (reached only
through --execute), only in the Authorization header of each GET. Never save
or print a request or response containing the actual key value, and never
persist a raw provider response: only sanitize_pod_listing's two fields
(desiredStatus, costPerHr) are written to disk, and only when the Pod is
still listed.

RunPod API reference relied on (retrieved 2026-09-15T16:50:42Z from
https://docs.runpod.io/llms.txt and the page it links):

  GET https://rest.runpod.io/v1/pods/{podId}
    (https://docs.runpod.io/api-reference/pods/GET/pods/podId)
    Authorization: Bearer <key>, same scheme as lease_guard.Provider and
    prepare_lease.Provider.
    200 response body (Pod): id, name, desiredStatus ("RUNNING", "EXITED" or
      "TERMINATED"), costPerHr, adjustedCostPerHr, dataCenterId,
      networkVolumeId, among other fields.
    404 response: "Pod not found" -- the only response this module treats as
      a definitive absence. A 400 ("Invalid ID supplied") or any other
      non-2xx, non-404 status is treated as ambiguous, not as absence.

Runbook discipline: a released Pod is confirmed absent only by a *subsequent*
independent lookup, not by the guard's own in-process detection alone (see
lease_guard.release_after_failure and run_loop, which each write their own
released.json locally on the Pod side or from the guard's own polling loop).
--repeat (default 2) consecutive not-found lookups, --interval (default 30)
seconds apart, must ALL come back not-found before provider_absent becomes
true. Any lookup that is still listed, or that is transient/ambiguous (5xx,
timeout, an unexpected status, or a malformed/incomplete body), ends the
attempt loop immediately: a listing needs no further confirmation that the
Pod still exists, and an ambiguous read must never be papered over by
retrying it into a false confirmation.

Receipt shape (pod-absence-<podid>.json, schema phase1-cloud-pod-absence/v1):
  observed_utc (timezone-aware ISO-8601, set when the decision is final),
  pod_id, provider_absent (true only on every lookup performed coming back
  not-found; false when any lookup found the Pod still listed; null on any
  transient/ambiguous lookup), http_status (the deciding lookup's HTTP
  status, or null for a timeout/unavailable request), endpoint (the exact
  URL requested), repeat_required, attempts_performed, interval_seconds,
  lookups (the full per-attempt trail), pod (sanitized desiredStatus/
  costPerHr, present only when provider_absent is false), reason (present
  only when provider_absent is null), key_saved (always false).

This receipt shape is deliberately compatible with lifecycle_timing.py's
--release-confirmed input: lifecycle_timing.resolve_marker/receipt_epoch
reads one of EPOCH_FIELDS ('epoch', 'created_epoch', 'verified_epoch',
'prepared_epoch') first and otherwise falls back to a timezone-aware
'observed_utc' string, which this receipt always carries; and
assemble_envelope requires release_document.get('provider_absent') is True,
which this receipt's 'provider_absent' field satisfies exactly when every
repeated lookup came back not-found. No logic in lifecycle_timing.py needs
to change for it to accept a pod-absence-<podid>.json receipt as
--release-confirmed.

Without --execute, this module prints the GET request it would make (method,
URL, a placeholder Authorization header) and performs no network call and
writes no file.
"""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
from common import encoded, require, write
from prepare_lease import KEY_PLACEHOLDER

SCHEMA = 'phase1-cloud-pod-absence/v1'
PODS_URL = 'https://rest.runpod.io/v1/pods'
USER_AGENT = 'phase1-pod-absence-verify/1'
POD_ID_PATTERN = '[A-Za-z0-9]+'
DEFAULT_REPEAT = 2
DEFAULT_INTERVAL_SECONDS = 30.0


def endpoint_for(pod_id):
    require(re.fullmatch(POD_ID_PATTERN, pod_id), 'invalid exact pod id')
    return PODS_URL + '/' + pod_id


def sanitize_pod_listing(raw):
    """Keep only the two fields the runbook needs when a Pod is still
    listed. The raw provider response is never written to disk or printed."""
    require(isinstance(raw, dict), 'pod response malformed')
    for field in ('desiredStatus', 'costPerHr'):
        require(field in raw, 'pod response missing ' + field)
    require(isinstance(raw['desiredStatus'], str), 'invalid pod field types')
    require(isinstance(raw['costPerHr'], (int, float)) and not isinstance(raw['costPerHr'], bool)
            and raw['costPerHr'] >= 0, 'invalid pod cost in response')
    return {'desired_status': raw['desiredStatus'], 'cost_per_hour_usd': raw['costPerHr']}


# ---- Live provider client (only reached through --execute) -----------------
class Provider:
    """Single read-only Pod lookup. Constructed only inside verify_absence,
    from a key already confirmed present in the environment, or injected as a
    mock in tests. Only ever issues GET; never POST/PUT/DELETE."""

    def __init__(self, key):
        require(key, 'runtime API key missing')
        self.key = key

    def lookup(self, pod_id):
        """Returns a dict describing exactly what was observed, never
        raising for an HTTP-layer outcome: {'status', 'kind', 'body',
        'reason'}, kind one of 'not_found', 'listed', 'error'."""
        request = urllib.request.Request(endpoint_for(pod_id), method='GET',
            headers={'Authorization': 'Bearer ' + self.key, 'User-Agent': USER_AGENT})
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                status = response.status
                raw = response.read()
        except urllib.error.HTTPError as error:
            status = error.code
            error.close()
        except (OSError, ValueError):
            return {'status': None, 'kind': 'error', 'body': None, 'reason': 'request_unavailable'}
        if status == 404:
            return {'status': status, 'kind': 'not_found', 'body': None, 'reason': None}
        if 200 <= status < 300:
            try:
                body = json.loads(raw) if raw else None
            except ValueError:
                body = None
            if not isinstance(body, dict):
                return {'status': status, 'kind': 'error', 'body': None, 'reason': 'malformed_response_body'}
            return {'status': status, 'kind': 'listed', 'body': body, 'reason': None}
        if 500 <= status < 600:
            return {'status': status, 'kind': 'error', 'body': None, 'reason': 'provider_http_' + str(status)}
        return {'status': status, 'kind': 'error', 'body': None, 'reason': 'unexpected_http_status_' + str(status)}


def _require_execute_key():
    key = os.environ.get('RUNPOD_API_KEY')
    require(key, 'RUNPOD_API_KEY must be set in the environment for --execute')
    return key


def perform_lookups(pod_id, repeat, interval, api, now, sleep):
    """Run up to `repeat` consecutive lookups, `interval` seconds apart.

    Stops immediately on a definitive listing or an ambiguous outcome
    (neither needs, nor can be improved by, a further attempt). Only when
    every one of the `repeat` attempts comes back not-found is absence
    confirmed. Returns (provider_absent, http_status, reason, pod_fields,
    attempts): provider_absent is True/False/None; pod_fields is the
    sanitized listing when provider_absent is False, else None.
    """
    attempts = []
    for attempt in range(1, repeat + 1):
        stamp = now()
        outcome = api.lookup(pod_id)
        status = outcome.get('status')
        kind = outcome.get('kind')
        record = {'attempt': attempt, 'http_status': status,
                   'observed_utc': datetime.fromtimestamp(stamp, tz=timezone.utc).isoformat()}
        if kind == 'not_found':
            attempts.append(record)
            if attempt < repeat:
                sleep(interval)
            continue
        if kind == 'listed':
            try:
                sanitized = sanitize_pod_listing(outcome.get('body'))
            except ValueError:
                record['reason'] = 'malformed_response_body'
                attempts.append(record)
                return None, status, 'malformed_response_body', None, attempts
            record['pod'] = sanitized
            attempts.append(record)
            return False, status, None, sanitized, attempts
        reason = outcome.get('reason') or 'provider_lookup_ambiguous'
        record['reason'] = reason
        attempts.append(record)
        return None, status, reason, None, attempts
    return True, attempts[-1]['http_status'], None, None, attempts


def verify_absence(pod_id, output_dir, repeat=DEFAULT_REPEAT, interval=DEFAULT_INTERVAL_SECONDS,
                   api=None, now=time.time, sleep=time.sleep):
    """Live read-only confirmation. Reachable only when RUNPOD_API_KEY is
    set. `api` is exercised only through a mocked client in tests; this
    function never performs a live call outside of --execute. Always writes
    a receipt, including on an ambiguous/ transient outcome, so a caller can
    never mistake "no receipt yet" for confirmation."""
    require(re.fullmatch(POD_ID_PATTERN, pod_id), 'invalid exact pod id')
    require(isinstance(repeat, int) and not isinstance(repeat, bool) and repeat >= 1,
            'repeat must be a positive integer')
    require(isinstance(interval, (int, float)) and not isinstance(interval, bool) and interval >= 0,
            'interval must be a non-negative number of seconds')
    key = _require_execute_key()
    api = api or Provider(key)
    provider_absent, status, reason, pod_fields, attempts = perform_lookups(
        pod_id, repeat, interval, api, now, sleep)
    result = {'schema': SCHEMA, 'observed_utc': datetime.fromtimestamp(now(), tz=timezone.utc).isoformat(),
              'pod_id': pod_id, 'provider_absent': provider_absent, 'http_status': status,
              'endpoint': endpoint_for(pod_id), 'repeat_required': repeat,
              'attempts_performed': len(attempts), 'interval_seconds': interval,
              'lookups': attempts, 'pod': pod_fields, 'reason': reason, 'key_saved': False}
    output_dir = Path(output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    receipt_path = output_dir / ('pod-absence-' + pod_id + '.json')
    write(receipt_path, result, replace=True)
    return result, receipt_path


def exit_code_for(result):
    """Nonzero exactly when the outcome is ambiguous: a null provider_absent
    can never be mistaken for a confirmed release or a confirmed listing."""
    return 1 if result['provider_absent'] is None else 0


def build_request_preview(pod_id, repeat, interval):
    require(isinstance(repeat, int) and not isinstance(repeat, bool) and repeat >= 1,
            'repeat must be a positive integer')
    require(isinstance(interval, (int, float)) and not isinstance(interval, bool) and interval >= 0,
            'interval must be a non-negative number of seconds')
    return {'method': 'GET', 'url': endpoint_for(pod_id),
            'headers': {'Authorization': 'Bearer ' + KEY_PLACEHOLDER, 'User-Agent': USER_AGENT},
            'repeat': repeat, 'interval_seconds': interval,
            'note': 'read-only Pod lookup; rerun with --execute --output-dir <dir> to perform it live'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--pod-id', required=True)
    parser.add_argument('--output-dir', help='receipt directory; required with --execute')
    parser.add_argument('--execute', action='store_true',
        help='perform the live read-only GET(s); requires RUNPOD_API_KEY in the environment')
    parser.add_argument('--repeat', type=int, default=DEFAULT_REPEAT,
        help='consecutive not-found lookups required before absence is confirmed (default 2)')
    parser.add_argument('--interval', type=float, default=DEFAULT_INTERVAL_SECONDS,
        help='seconds between consecutive lookups (default 30)')
    args = parser.parse_args(argv)
    if not args.execute:
        preview = build_request_preview(args.pod_id, args.repeat, args.interval)
        print(encoded(preview).decode())
        return 0
    require(args.output_dir, '--output-dir is required with --execute')
    result, receipt_path = verify_absence(args.pod_id, args.output_dir,
        repeat=args.repeat, interval=args.interval)
    print(encoded(result).decode())
    return exit_code_for(result)


if __name__ == '__main__':
    raise SystemExit(main())
