"""Offline create-request preparation, with an on-Pod watchdog in the entrypoint,
plus a strictly gated --execute mode that performs the single live Pod-create POST.

Does not query or mutate RunPod unless --execute is passed. Inject RUNPOD_API_KEY
only in-memory: in the outer request's Authorization header, and (already handled
by prepare()) the placeholder-replaced copy the resident guard uses after boot.
Never save or print a request or response containing the actual key value, and
never persist a raw provider response: only sanitize_pod's six fields are written
to disk.

RunPod API reference relied on (retrieved 2026-09-15T15:44:06Z from
https://docs.runpod.io/llms.txt and the pages it links):

  POST https://rest.runpod.io/v1/pods
    (https://docs.runpod.io/api-reference/pods/POST/pods)
    request body: see prepare()'s `body` dict below.
    response body (Pod): id (string), name (string), desiredStatus (string),
      costPerHr (number), dataCenterId (string), networkVolumeId (string, or
      nested under networkVolume.id), imageName (string), among other fields.
"""
from __future__ import annotations
import argparse
import base64
import json
import os
from pathlib import Path
import re
import time
import urllib.error
import urllib.request
from common import encoded, pin, read, require, write, require_fresh_funding_snapshot
from lease_guard import validate
from runtime_observation import IMAGE_REFERENCE

KEY_PLACEHOLDER = '__INJECT_AT_POST_DO_NOT_SAVE__'

PODS_URL = 'https://rest.runpod.io/v1/pods'
USER_AGENT = 'phase1-lease-execute/1'
CREATED_SCHEMA = 'phase1-cloud-pod-created/v1'
FAILURE_SCHEMA = 'phase1-cloud-pod-create-failure/v1'
# A pinned reference is "<repo>@sha256:<64 hex>"; a mutable tag (e.g. the
# "python:3.12-slim-bookworm" historical default) never matches this shape.
IMAGE_DIGEST_PATTERN = re.compile(r'^[^@\s]+@sha256:[0-9a-f]{64}$')


def require_pinned_image(image_name, allow_mutable_image):
    """Refuse a mutable image tag unless explicitly allowed. The qualification
    burst's runtime-image contract is bound to one exact digest (see
    runtime_observation.IMAGE_REFERENCE and image_contract()); a floating tag
    like "python:3.12-slim-bookworm" can resolve to different bytes on a later
    pull, which cloud_worker.run_locked() would then refuse to certify anyway
    (runtime_unverified), but only after a Pod was already paid for."""
    require(isinstance(image_name, str) and image_name, 'image reference missing')
    require(bool(IMAGE_DIGEST_PATTERN.fullmatch(image_name)) or allow_mutable_image,
            'mutable image tag refused without --allow-mutable-image: ' + image_name)


def prepare(spec, output, now=None, allow_mutable_image=False):
    now = time.time() if now is None else now
    require(spec['schema'] == 'phase1-cloud-lease-preparation/v1', 'wrong lease preparation schema')
    lease = spec['lease']
    projected = validate(lease)
    image_name = spec.get('image_name', IMAGE_REFERENCE)
    require_pinned_image(image_name, allow_mutable_image)
    require(0 <= now - lease['created_epoch'] <= 300, 'fresh allocation timestamp required')
    require(0 <= now - spec['account_observed_epoch'] <= 300, 'refresh account before allocation')
    require(0 <= now - spec['quote_observed_epoch'] <= 300, 'refresh CPU quote before allocation')
    require(spec['observed_pods'] == 0 and spec['autopay_enabled'] is False,
            'this single-Pod qualification expects no existing Pods and no autopay')
    require(spec['quoted_cpu_usd_hour'] <= lease['rate_ceiling_usd_hour'], 'quote above ceiling')
    require(re.fullmatch('ssh-ed25519 [A-Za-z0-9+/=]+(?: [^\r\n]+)?', spec['public_ssh_key']),
            'single public ed25519 key required')
    run_id = lease['name']
    guard_root = '/run/phase1/' + run_id
    durable = '/workspace/phase1/' + run_id
    blobs = {name: base64.b64encode(Path(__file__).with_name(name).read_bytes()).decode()
             for name in ('common.py', 'lease_guard.py')}
    blobs['lease.json'] = base64.b64encode(encoded(lease)).decode()
    # Import the guard from memory. An unwritable /run must not prevent the
    # failure-release path from starting. Only the guard receives the API key.
    identity={key:lease[key] for key in ('name','network_volume_id')}
    guard_program=('BLOBS='+repr(blobs)+'\nGUARD_ROOT='+repr(guard_root)+
        '\nEXPECTED_IDENTITY='+repr(identity)+'\n'+'''import base64,json,os,sys,types
for name in ('common','lease_guard'):
 module=types.ModuleType(name);sys.modules[name]=module
 exec(compile(base64.b64decode(BLOBS[name+'.py']),'<embedded-'+name+'>','exec'),module.__dict__)
guard=sys.modules['lease_guard']
pod_id=os.environ.get('RUNPOD_POD_ID','')
if globals().get('PHASE1_RECOVERY_ONLY',False):
 guard.release_after_failure(guard.Provider(os.environ.get('RUNPOD_API_KEY'),pod_id),EXPECTED_IDENTITY,pod_id,GUARD_ROOT,'bootstrap_guard_failure')
else:
 guard.run(json.loads(base64.b64decode(BLOBS['lease.json'])),GUARD_ROOT,pod_id,expected_identity=EXPECTED_IDENTITY,acquire_lock=True)
''')
    bootstrap = '''import os,pathlib,subprocess,time
env=dict(os.environ);env.pop('RUNPOD_API_KEY',None)
env['DEBIAN_FRONTEND']='noninteractive'
sshd=None
try:
 guard=subprocess.Popen(['python3','-B','-c',GUARD_PROGRAM],stdin=subprocess.DEVNULL,start_new_session=True)
except Exception:
 print('Guard spawn failed; requesting own-Pod release',flush=True)
 exec(GUARD_PROGRAM,{'PHASE1_RECOVERY_ONLY':True})
 raise SystemExit(0)
def setup_run(command,timeout=300,**options):
 child=subprocess.Popen(command,env=env,stdin=subprocess.DEVNULL,**options)
 started=time.monotonic()
 try:
  while child.poll() is None:
   if guard.poll() is not None: raise RuntimeError('guard exited during setup')
   if time.monotonic()-started>=timeout: raise RuntimeError('setup timeout')
   time.sleep(1)
  if child.returncode != 0: raise RuntimeError('setup command failed')
 finally:
  if child.poll() is None:
   child.terminate()
   try: child.wait(timeout=5)
   except subprocess.TimeoutExpired:
    child.kill();child.wait(timeout=5)
try:
 for command in (['apt-get','update'],['apt-get','install','-y','--no-install-recommends','openssh-server','procps','util-linux']):
  setup_run(command)
 ssh=pathlib.Path('/root/.ssh');ssh.mkdir(exist_ok=True);ssh.chmod(0o700)
 (ssh/'authorized_keys').write_text(PUBLIC_KEY+'\\n');(ssh/'authorized_keys').chmod(0o600)
 pathlib.Path('/run/sshd').mkdir(exist_ok=True)
 setup_run(['ssh-keygen','-A'],timeout=30,stdout=subprocess.DEVNULL)
 conf=pathlib.Path('/etc/ssh/sshd_config.d/phase1.conf')
 conf.write_text('PermitRootLogin prohibit-password\\nPasswordAuthentication no\\nKbdInteractiveAuthentication no\\n')
 sshd=subprocess.Popen(['/usr/sbin/sshd','-D','-e'],env=env,stdin=subprocess.DEVNULL)
except Exception:
 print('SSH bootstrap failed; resident guard remains active',flush=True)
try:
 while guard.poll() is None: time.sleep(1)
except (Exception,KeyboardInterrupt):
 exec(GUARD_PROGRAM,{'PHASE1_RECOVERY_ONLY':True})
if guard.returncode != 0:
 print('Guard process failed; requesting own-Pod release',flush=True)
 exec(GUARD_PROGRAM,{'PHASE1_RECOVERY_ONLY':True})
# Successful failure recovery also means provider-confirmed absence.
if sshd is not None:
 try: sshd.terminate()
 except Exception: pass
raise SystemExit(0)
'''
    bootstrap = ('GUARD_PROGRAM='+repr(guard_program)+
                 '\nPUBLIC_KEY=' + repr(spec['public_ssh_key']) + '\n' + bootstrap)
    command = ['python3', '-c', "import base64;exec(base64.b64decode('" +
               base64.b64encode(bootstrap.encode()).decode() + "'))"]
    body = {'name': run_id, 'cloudType': 'SECURE', 'computeType': 'CPU',
            'cpuFlavorIds': ['cpu3c'], 'cpuFlavorPriority': 'custom', 'vcpuCount': 32,
            'containerDiskInGb': 40, 'volumeInGb': 0,
            'imageName': image_name,
            'interruptible': False, 'supportPublicIp': True, 'ports': ['22/tcp'],
            'networkVolumeId': lease['network_volume_id'], 'volumeMountPath': '/workspace',
            'dataCenterIds': ['EU-RO-1'], 'dataCenterPriority': 'custom',
            'dockerEntrypoint': command, 'dockerStartCmd': [],
            'env': {'RUNPOD_API_KEY': KEY_PLACEHOLDER, 'CUDA_VISIBLE_DEVICES': ''}}
    output = Path(output)
    require(not output.exists(), 'fresh lease output required')
    output.mkdir(parents=True)
    write(output / 'lease.json', lease)
    write(output / 'create-request-template.json', body)
    result = {'projected_conservative_usd_including_margin': projected,
              'guard_state': guard_root, 'durable_root': durable,
              'create_request_template': pin(output / 'create-request-template.json'),
              'provider_ttl': False, 'guard_armed': False, 'allocated': False,
              'before_native_launch': ['read fresh guard.json with provider_ok=true',
                  'verify exact returned Pod ID, name, CPU, volume, rate and image digest',
                  'verify lease time and current funded balance still fit',
                  'verify payload and perform required engineering parity'],
              'limitation': 'Resident guard cannot cover failed container boot, host/OS failure, '
                            'or provider API outage. External root observation/release is retained.'}
    write(output / 'preparation.json', result)
    return result


def prepared_template(output):
    """Re-read and re-hash the saved template before any live call, so a live
    POST always matches exactly what prepare() wrote and pinned."""
    path = Path(output) / 'create-request-template.json'
    pin(path)
    return read(path)


# ---- Live provider client (only reached through --execute) -----------------
class Provider:
    """Live Pod-create call. Constructed only inside execute_create, from a key
    already confirmed present in the environment, or injected as a mock in
    tests. prepare() never touches it."""

    def __init__(self, key):
        require(key, 'runtime API key missing')
        self.key = key

    def create(self, body):
        payload = json.dumps(body).encode()
        request = urllib.request.Request(PODS_URL, data=payload, method='POST',
            headers={'Authorization': 'Bearer ' + self.key, 'Content-Type': 'application/json',
                     'User-Agent': USER_AGENT})
        return self._call(request)

    def _call(self, request):
        try:
            with urllib.request.urlopen(request, timeout=15) as response:
                body = response.read()
                return json.loads(body) if body else {}
        except urllib.error.HTTPError as error:
            code = error.code; error.close()
            raise RuntimeError('provider HTTP ' + str(code)) from None
        except (OSError, ValueError):
            raise RuntimeError('provider request unavailable') from None


def sanitize_pod(raw):
    """Keep only the six fields root needs before native launch. The raw
    provider response is never written to disk or printed."""
    require(isinstance(raw, dict), 'pod response malformed')
    for field in ('id', 'name', 'desiredStatus', 'costPerHr'):
        require(field in raw, 'pod response missing ' + field)
    require(isinstance(raw['id'], str) and isinstance(raw['name'], str)
            and isinstance(raw['desiredStatus'], str), 'invalid pod field types')
    require(isinstance(raw['costPerHr'], (int, float)) and not isinstance(raw['costPerHr'], bool)
            and raw['costPerHr'] >= 0, 'invalid pod cost in response')
    data_center = raw.get('dataCenterId')
    require(isinstance(data_center, str) and data_center, 'pod response missing dataCenterId')
    nested_volume = raw.get('networkVolume')
    volume_id = raw.get('networkVolumeId') or (nested_volume.get('id') if isinstance(nested_volume, dict) else None)
    require(isinstance(volume_id, str) and volume_id, 'pod response missing network volume id')
    return {'id': raw['id'], 'name': raw['name'], 'desired_status': raw['desiredStatus'],
            'cost_per_hour_usd': raw['costPerHr'], 'data_center_id': data_center,
            'volume_id': volume_id}


def _require_execute_key():
    key = os.environ.get('RUNPOD_API_KEY')
    require(key, 'RUNPOD_API_KEY must be set in the environment for --execute')
    return key


def execute_create(template, output, funding_snapshot_path, volume_id, api=None, now=None,
                   allow_mutable_image=False):
    """Live Pod-create call. Reachable only when RUNPOD_API_KEY is set, a
    funding snapshot no older than thirty minutes is supplied, and the
    pre-created --volume-id matches what the saved template already targets.
    `api` is exercised only through a mocked client in tests; this function
    never performs a live call outside of --execute.

    On a non-2xx or malformed response, records a sanitized failure receipt
    (never the raw body or the key) and re-raises so the caller exits nonzero.
    """
    now = time.time() if now is None else now
    key = _require_execute_key()
    require(funding_snapshot_path, '--funding-snapshot is required with --execute')
    require_fresh_funding_snapshot(funding_snapshot_path, now)
    require(volume_id, '--volume-id is required with --execute')
    # Unlike prepare_volume.py's template, prepare_lease.py saves the raw Pod
    # create-request body directly (no method/url/headers wrapper): the key
    # goes only in the Authorization header, injected by Provider.create().
    body = template
    require(body.get('networkVolumeId') == volume_id,
            'prepared template volume id differs from --volume-id')
    require_pinned_image(body.get('imageName'), allow_mutable_image)
    output = Path(output)
    api = api or Provider(key)
    try:
        raw = api.create(body)
        sanitized = sanitize_pod(raw)
    except (RuntimeError, ValueError, KeyError, TypeError) as error:
        write(output / 'create-pod-failure.json', {'schema': FAILURE_SCHEMA,
              'failed_epoch': now, 'error_type': type(error).__name__,
              'error': str(error)}, replace=True)
        raise
    result = {'schema': CREATED_SCHEMA, 'created_epoch': now, **sanitized}
    write(output / 'created-pod.json', result, replace=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', help='offline lease-preparation spec path (create-template mode)')
    parser.add_argument('--output', required=True)
    parser.add_argument('--execute', action='store_true',
        help='perform the live Pod-create POST; requires RUNPOD_API_KEY in the environment, '
             '--funding-snapshot and --volume-id against an already-prepared --output')
    parser.add_argument('--funding-snapshot',
        help='path to a funding snapshot no older than thirty minutes; required with --execute')
    parser.add_argument('--volume-id',
        help='the pre-created network volume id the prepared template must target; required with --execute')
    parser.add_argument('--allow-mutable-image', action='store_true',
        help='permit a non-digest-pinned image reference; refused by default')
    args = parser.parse_args()
    result = prepare(read(args.spec), args.output, allow_mutable_image=args.allow_mutable_image) if args.spec else None
    if args.execute:
        result = execute_create(prepared_template(args.output), args.output, args.funding_snapshot,
                                args.volume_id, allow_mutable_image=args.allow_mutable_image)
    require(result is not None, 'nothing to do: pass --spec, or --execute against a prepared --output')
    print(encoded(result).decode())


if __name__ == '__main__':
    main()
