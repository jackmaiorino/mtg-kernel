"""Offline create-request preparation, with an on-Pod watchdog in the entrypoint.

Does not query or mutate RunPod. Inject RUNPOD_API_KEY only in the in-memory POST
request. Never save or print a request containing its actual value.
"""
from __future__ import annotations
import argparse
import base64
from pathlib import Path
import re
import time
from common import encoded, pin, read, require, write
from lease_guard import validate

KEY_PLACEHOLDER = '__INJECT_AT_POST_DO_NOT_SAVE__'


def prepare(spec, output, now=None):
    now = time.time() if now is None else now
    require(spec['schema'] == 'phase1-cloud-lease-preparation/v1', 'wrong lease preparation schema')
    lease = spec['lease']
    projected = validate(lease)
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
            'imageName': spec.get('image_name', 'python:3.12-slim-bookworm'),
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True); parser.add_argument('--output', required=True)
    args = parser.parse_args()
    print(encoded(prepare(read(args.spec), args.output)).decode())


if __name__ == '__main__':
    main()
