"""Pod-resident cost/deadline guard. Runs without the workstation after boot.

No provider action on import. DELETE is confined to the exact runtime Pod ID.
This is not a provider-enforced TTL: failed container boot, host failure and
unreachable provider control plane require an independent external guard.

Release rules (ported from the reviewed ordinary-training lease adapter, 2026-10-25):
- a missing or malformed costPerHr/adjustedCostPerHr, or max(costPerHr,
  adjustedCostPerHr) above the lease ceiling, is an invalid allocation and is
  released at once (pod_rate, verify_pod);
- from DELETE_MARGIN_SECONDS before the deadline, and after any latched release
  or guard failure, every loop DELETEs the runtime ID first; DELETE is never
  gated on a GET, an identity match or a receipt write (release_after_failure);
- receipts are best-effort and can never skip a DELETE;
- release completes only after two absent observations at least
  ABSENT_CONFIRM_SECONDS apart (AbsenceConfirmer); a failed GET is never absence;
- the expected allocation size is lease['vcpu_count'] (default 32).
"""
from __future__ import annotations
import argparse
import contextlib
import json
import math
import os
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
from common import read, require, write


def validate(lease):
    require(lease['schema'] == 'phase1-cloud-lease/v1', 'lease schema differs')
    require(re.fullmatch('[a-zA-Z0-9-]+', lease['name']), 'invalid exact Pod name')
    for key in ('created_epoch', 'deadline_epoch', 'increment_cap_usd', 'total_cap_usd',
                'prior_conservative_usd', 'rate_ceiling_usd_hour', 'storage_usd_hour',
                'postrun_storage_reserve_usd', 'recovery_reserve_usd', 'funded_balance_usd'):
        require(isinstance(lease[key], (int, float)) and not isinstance(lease[key], bool)
                and math.isfinite(lease[key]) and lease[key] >= 0, 'invalid numeric lease value')
    require(0 < lease['increment_cap_usd'] <= 10 and lease['total_cap_usd'] <= 200,
            'qualification ceiling exceeds user scope')
    require(0 < lease['deadline_epoch'] - lease['created_epoch'] <= 8 * 3600,
            'lease duration must be at most eight hours')
    require(math.isfinite(lease['safety_multiplier']) and lease['safety_multiplier'] >= 1.2,
            'at least twenty percent cost margin required')
    reserve = lease['recovery_reserve_usd'] + lease['postrun_storage_reserve_usd']
    projected = lease['safety_multiplier'] * (reserve + (
        lease['deadline_epoch'] - lease['created_epoch']) / 3600 * (
        lease['rate_ceiling_usd_hour'] + lease['storage_usd_hour']))
    require(projected <= lease['increment_cap_usd'], 'full lease exceeds increment cap')
    require(lease['prior_conservative_usd'] + projected <= lease['total_cap_usd'],
            'full lease exceeds cumulative cap')
    require(projected <= lease['funded_balance_usd'], 'insufficient funded balance')
    require(lease['startup_idle_seconds'] > 0 and 0 < lease['work_idle_seconds'] <= 300 and
            lease['recovery_seconds'] >= 30 and lease['poll_seconds'] in range(1, 61),
            'invalid guard intervals')
    # The expected allocation size is part of the lease (32 or 64 vCPU, default 32).
    require(lease.get('vcpu_count', 32) in (32, 64), 'unsupported vCPU count')
    return projected


ABSENT_CONFIRM_SECONDS = 30
DELETE_MARGIN_SECONDS = 600       # the resident guard starts deleting 10 min before the deadline


def pod_rate(pod):
    """max(costPerHr, adjustedCostPerHr) as a finite positive number, else ValueError.

    A missing or malformed costPerHr, or a malformed adjustedCostPerHr, is a guard
    failure (the caller releases at once); it never blocks deadline handling.
    """
    def parse(value, required):
        if value is None and not required:
            return 0.0
        if isinstance(value, bool) or not isinstance(value, (int, float, str)):
            raise ValueError('provider rate malformed')
        rate = float(value)
        if not math.isfinite(rate) or rate < 0:
            raise ValueError('provider rate malformed')
        return rate
    rate = max(parse(pod.get('costPerHr'), True), parse(pod.get('adjustedCostPerHr'), False))
    if rate <= 0:
        raise ValueError('provider rate unavailable')
    return rate


class AbsenceConfirmer:
    """Two absent observations at least ABSENT_CONFIRM_SECONDS apart, with no presence between."""

    def __init__(self, clock=None):
        self.clock = clock; self.first = None

    def observe(self, absent):
        now = self.clock() if self.clock else time.time()
        if not absent:
            self.first = None
            return False
        if self.first is None:
            self.first = now
            return False
        return now - self.first >= ABSENT_CONFIRM_SECONDS


def decision(lease, now, highest_rate, progress=None):
    elapsed = max(0, now - lease['created_epoch'])
    rate = max(lease['rate_ceiling_usd_hour'], highest_rate) + lease['storage_usd_hour']
    reserve = lease['recovery_reserve_usd'] + lease['postrun_storage_reserve_usd']
    cost = lease['safety_multiplier'] * (reserve + elapsed * rate / 3600)
    forecast = cost + lease['safety_multiplier'] * lease['recovery_seconds'] * rate / 3600
    reason = None
    if now >= lease['deadline_epoch'] - lease['recovery_seconds']:
        reason = 'deadline'
    elif forecast >= lease['increment_cap_usd'] or (
            lease['prior_conservative_usd'] + forecast >= lease['total_cap_usd']):
        reason = 'cost_cap'
    elif progress and progress.get('finished'):
        reason = 'worker_finished'
    elif progress and now - progress.get('epoch', progress['last_productive_epoch']) > 90:
        reason = 'controller_lost'
    elif progress and not progress.get('native_alive', False) and progress.get('queued_work', False):
        reason = 'native_worker_lost'
    elif progress and now - progress.get('last_activity_epoch', progress['last_productive_epoch']) >= lease['work_idle_seconds']:
        reason = 'native_work_stalled_or_idle'
    elif not progress and elapsed >= lease['startup_idle_seconds']:
        reason = 'startup_idle'
    return {'reason': reason, 'increment_conservative_usd': cost,
            'cumulative_conservative_usd': lease['prior_conservative_usd'] + cost,
            'shutdown_forecast_usd': forecast}


class Provider:
    def __init__(self, key, pod_id):
        require(key and re.fullmatch('[A-Za-z0-9]+', pod_id), 'runtime Pod ID/key missing')
        self.key = key; self.pod_id = pod_id

    def call(self, method):
        require(method in ('GET', 'DELETE'), 'guard method rejected')
        req = urllib.request.Request('https://rest.runpod.io/v1/pods/' + self.pod_id,
            method=method, headers={'Authorization': 'Bearer ' + self.key,
                                   'User-Agent': 'Mozilla/5.0 g115-lease-guard/1'})
        try:
            with urllib.request.urlopen(req, timeout=15) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except urllib.error.HTTPError as error:
            code=error.code;error.close()
            if code == 404:
                return None
            raise RuntimeError('provider HTTP ' + str(code)) from None
        except (OSError, ValueError):
            raise RuntimeError('provider request unavailable') from None

    def funds(self):
        # Explicit read-only GraphQL query, matching the established local helper.
        # The API key appears only in Authorization, never a URL or artifact.
        body=json.dumps({'query':'query Phase1GuardFunds { myself { clientBalance currentSpendPerHr underBalance } }'}).encode()
        req=urllib.request.Request('https://api.runpod.io/graphql',data=body,method='POST',
            headers={'Authorization':'Bearer '+self.key,'Content-Type':'application/json',
                     'User-Agent':'Mozilla/5.0 g115-lease-guard/1'})
        try:
            with urllib.request.urlopen(req,timeout=15) as response:
                value=json.load(response)
            require(not value.get('errors'),'account query unavailable')
            account=value['data']['myself']
            balance=float(account['clientBalance']);spend=float(account['currentSpendPerHr'])
            require(math.isfinite(balance) and balance>=0 and math.isfinite(spend) and spend>=0,
                    'account funding fields unavailable')
            require(type(account['underBalance']) is bool,'account balance state unavailable')
            return {'balance_usd':balance,'account_spend_usd_hour':spend,
                    'under_balance':account['underBalance']}
        except urllib.error.HTTPError as error:
            code=error.code;error.close()
            raise RuntimeError('account HTTP '+str(code)) from None
        except (OSError,ValueError,KeyError,TypeError):
            raise RuntimeError('account funds unavailable') from None


def funding_status(lease,now,highest_rate,observation):
    """Alert at six hours of account burn, or the remaining stage if shorter."""
    if observation is None:
        return {'funds_verified':False,'allow_new_dispatch':False,'funding_alert':'funds_unverified'}
    age=now-observation['epoch']
    if not 0<=age<=75:
        return {'funds_verified':False,'allow_new_dispatch':False,'funding_alert':'funds_unverified'}
    rate=max(observation['account_spend_usd_hour'],highest_rate+lease['storage_usd_hour'])
    # Discount the last observation conservatively until the next fresh read.
    balance=max(0,observation['balance_usd']-age*rate*lease['safety_multiplier']/3600)
    hours=min(6,max(0,lease['deadline_epoch']-now)/3600)
    reserve=lease['recovery_reserve_usd']+lease['postrun_storage_reserve_usd']
    threshold=lease['safety_multiplier']*(hours*rate+reserve)
    alert=observation['under_balance'] or balance<=threshold
    recovery=lease['safety_multiplier']*(lease['recovery_seconds']*rate/3600+reserve)
    return {'funds_verified':True,'funds_observed_epoch':observation['epoch'],
            'funded_balance_conservative_usd':balance,'account_spend_usd_hour':observation['account_spend_usd_hour'],
            'funding_alert_horizon_hours':hours,'funding_alert_threshold_usd':threshold,
            'funding_alert':'funded_horizon_below_six_hours_or_remaining_stage' if alert else None,
            'allow_new_dispatch':not alert,'recovery_funding_available':balance>=recovery}


def verify_pod_identity(pod, lease, runtime_id):
    # Shape and attached volume are allocation validity, not Pod ownership.
    # An allocation error must still permit release of this exact named Pod.
    require(pod['id'] == runtime_id and pod['name'] == lease['name'], 'Pod identity differs')


def verify_pod(pod, lease, runtime_id):
    verify_pod_identity(pod,lease,runtime_id)
    require(pod.get('cpuFlavorId') == 'cpu3c' and pod.get('vcpuCount') == lease.get('vcpu_count', 32),
            'CPU allocation differs')
    volume_id = pod.get('networkVolumeId') or (pod.get('networkVolume') or {}).get('id')
    require(volume_id == lease['network_volume_id'], 'network volume differs')
    rate = pod_rate(pod)
    # Explicit ceiling: an allocation priced above the lease's ceiling is invalid (released at once).
    require(rate <= lease['rate_ceiling_usd_hour'], 'provider rate above ceiling')
    return rate


def best_effort_receipt(path,value):
    """Receipt failure must never prevent a provider request or retry."""
    try:
        write(path,value,replace=True)
        return True
    except Exception:
        # No exception body, request headers, or environment is logged.
        return False


def release_after_failure(api,identity,pod_id,state,reason):
    """Keep retrying own-Pod release even when local state is unwritable.

    Every loop DELETEs the runtime Pod ID first, then GETs for the two spaced
    absence observations, then writes best-effort receipts; retries are five
    seconds apart and requests time out after fifteen seconds. The ID is only
    ever this container's RUNPOD_POD_ID, never a file-supplied or guessed one.
    `identity` is kept for the call signature and receipts.
    """
    try:
        state=Path(state)
    except Exception:  # noqa: BLE001
        state=Path('/tmp')
    if not (isinstance(pod_id,str) and re.fullmatch('[A-Za-z0-9]+',pod_id)):
        # Without the runtime ID there is nothing safe to delete; the workstation guards cover this.
        raise SystemExit('runtime Pod ID unavailable for failure recovery')
    absence=AbsenceConfirmer()
    while True:
        # DELETE of the runtime ID (RUNPOD_POD_ID, this container's own Pod) comes first in
        # every loop and is never gated on a GET, an identity match or a receipt write.
        deleted=False
        try:
            api.call('DELETE'); deleted=True
        except Exception:  # noqa: BLE001 - retried next loop
            pass
        try:
            pod=api.call('GET')
            if absence.observe(pod is None):
                best_effort_receipt(state/'released.json',{'pod_id':pod_id,'epoch':time.time(),
                    'provider_absent':True,'absent_confirmations':2,'failure_recovery_reason':reason})
                return
        except Exception:  # noqa: BLE001 - a failed GET is never absence
            absence.observe(False)
        best_effort_receipt(state/'release-requested.json',{'pod_id':pod_id,'epoch':time.time(),
            'delete_accepted':deleted,'reason':reason})
        time.sleep(5)


def run(lease,state,pod_id,api=None,expected_identity=None,acquire_lock=False):
    """Guard all startup/runtime failures using the prepared own-Pod identity."""
    api=api or Provider(os.environ.get('RUNPOD_API_KEY'),pod_id)
    identity=expected_identity
    if identity is None and isinstance(lease,dict):
        identity={key:lease.get(key) for key in ('name','network_volume_id')}
    try:
        require(isinstance(lease,dict) and all(lease.get(key)==identity[key]
                for key in ('name','network_volume_id')),'lease differs from prepared Pod identity')
        with contextlib.ExitStack() as resources:
            if acquire_lock:
                import fcntl
                Path(state).mkdir(parents=True,exist_ok=True)
                lock=resources.enter_context((Path(state)/'guard.lock').open('a+b'))
                fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            return run_loop(lease,state,pod_id,api)
    except (Exception,KeyboardInterrupt) as error:
        # A malformed config, unreadable prior state, failed lock/mkdir/write,
        # or unexpected runtime error cannot silently abandon the paid Pod.
        return release_after_failure(api,identity,pod_id,state,'guard_failure_'+type(error).__name__)


def run_loop(lease, state, pod_id, api):
    validate(lease)
    state = Path(state); state.mkdir(parents=True, exist_ok=True)
    if (state / 'guard.json').exists():
        previous = read(state / 'guard.json')
        require(previous['pod_id'] == pod_id and previous['name'] == lease['name'],
                'existing guard belongs to another Pod')
    highest = lease['rate_ceiling_usd_hour']; verified = False; latched = None
    recovery_until = None; consecutive_errors = 0; funds=None;last_funds_attempt=None
    absence = AbsenceConfirmer()
    hard_stop = lease['deadline_epoch'] - DELETE_MARGIN_SECONDS
    while True:
        stamp = time.time(); provider_ok = False; ownership_verified=False
        if stamp >= hard_stop:
            # From (deadline - margin) on, DELETE first in every loop, never gated on GET or receipts.
            return release_after_failure(api,lease,pod_id,state,'deadline_margin')
        try:
            pod = api.call('GET')
            if absence.observe(pod is None):
                best_effort_receipt(state / 'released.json', {'pod_id': pod_id, 'epoch': stamp,
                    'provider_absent': True, 'absent_confirmations': 2})
                return
            if pod is None:
                time.sleep(lease['poll_seconds']); continue
            verify_pod_identity(pod,lease,pod_id)
            ownership_verified=True
            try:
                highest = max(highest, verify_pod(pod, lease, pod_id))
            except (ValueError,KeyError,TypeError):
                # No workload should start on an invalid allocation. Release the
                # owned Pod immediately; never delete its attached volume.
                return release_after_failure(api,lease,pod_id,state,'allocation_invalid')
            verified = provider_ok = True; consecutive_errors = 0
        except (ValueError, RuntimeError):
            consecutive_errors += 1
            if consecutive_errors >= 3:
                latched = latched or 'provider_unavailable_or_identity_mismatch'
        progress = None
        if (state / 'progress.json').exists():
            try:
                progress = read(state / 'progress.json')
                require(progress['pod_id'] == pod_id and
                    lease['created_epoch'] <= progress['last_productive_epoch'] <= stamp + 5,
                    'invalid progress record')
            except (ValueError, KeyError, OSError):
                latched = latched or 'invalid_progress'
                progress = None
        status = decision(lease, stamp, highest, progress)
        if last_funds_attempt is None or stamp-last_funds_attempt>=60:
            last_funds_attempt=stamp
            try:
                funds=api.funds()|{'epoch':stamp}
            except (RuntimeError,ValueError,TypeError):
                funds=None
        funding=funding_status(lease,stamp,highest,funds)
        if not funding['funds_verified']:
            latched=latched or 'funds_unverified'
        elif not funding['recovery_funding_available']:
            latched=latched or 'recovery_funds_insufficient'
        if funding['funding_alert']:
            best_effort_receipt(state/'funding-alert.json',{'pod_id':pod_id,'epoch':stamp,**funding})
        if highest > lease['rate_ceiling_usd_hour']:
            latched = latched or 'rate_above_ceiling'
        latched = latched or status['reason']
        # Deletion starts DELETE_MARGIN_SECONDS before the deadline at the latest, whatever latched;
        # receipts are best-effort and can never skip a DELETE.
        if latched and recovery_until is None:
            recovery_until = min(stamp + lease['recovery_seconds'], hard_stop)
            best_effort_receipt(state / 'stop-request.json', {'pod_id': pod_id, 'reason': latched,
                'epoch': stamp, 'release_epoch': recovery_until})
        best_effort_receipt(state / 'guard.json', {**status, **funding, 'pod_id': pod_id, 'name': lease['name'],
            'epoch': stamp, 'pid': os.getpid(), 'provider_verified': verified,
            'provider_ok': provider_ok, 'highest_rate_usd_hour': highest,
            'latched': latched, 'release_epoch': recovery_until})
        if latched and (stamp >= recovery_until or (state / 'recovery-complete.json').exists()):
            # The release decision hands over to the DELETE-first loop (never gated on GET).
            return release_after_failure(api,lease,pod_id,state,'latched_'+str(latched))
        time.sleep(lease['poll_seconds'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lease', required=True); parser.add_argument('--state', required=True)
    parser.add_argument('--expected-name');parser.add_argument('--expected-volume')
    args = parser.parse_args()
    pod_id=os.environ.get('RUNPOD_POD_ID','')
    identity={'name':args.expected_name,'network_volume_id':args.expected_volume} if args.expected_name and args.expected_volume else None
    try:
        lease=read(args.lease)
    except (OSError,ValueError):
        # Bootstrap always supplies the prepared identity independently of disk.
        # A manual invocation without it cannot safely guess which Pod to delete.
        return release_after_failure(Provider(os.environ.get('RUNPOD_API_KEY'),pod_id),
                                     identity,pod_id,args.state,'guard_configuration_unreadable')
    run(lease,args.state,pod_id,expected_identity=identity,acquire_lock=True)


if __name__ == '__main__':
    main()
