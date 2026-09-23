"""Formal D3 cloud controller. Review/placement checks precede Pod creation.

The on-host worker repeats full validation. Existing resident and independent
lease guards own the cost/deadline limits; this controller recovers and releases.
Without --execute this performs planning validation only and allocates nothing.
"""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import shlex
import subprocess
import sys
import tarfile
import time
import urllib.error

from g115_d3_launch_v1 import validate_plan
from g115_d3_payload_v1 import checked, read, require, sha
from g115_d3_cloud_host_v1 import CGROUP_FILES, profile, require_compatible

COMMON = '/workspace/g115-d3-e258daf3'


def logical(value):
    if isinstance(value, list):
        return [logical(v) for v in value]
    if isinstance(value, dict):
        return {k: logical(v) for k, v in value.items() if not (k == 'path' and 'sha256' in value)}
    return value


def execute(manifest, package, control):
    # This must complete before loading credentials or starting the paid guard.
    _, bound, allocation, _ = validate_plan(manifest, 'runpod')
    control = Path(control)
    sys.path.insert(0, str(Path(__file__).parent / 'phase1_cloud'))
    from common import write, pin
    from prepare_lease import execute_create, prepared_template
    from lease_guard import Provider, validate as validate_lease, verify_pod, verify_pod_identity
    lease = read(control / 'lease/lease.json')
    validate_lease(lease)
    require(0 <= time.time() - lease['created_epoch'] <= 300, 'Refresh lease preparation')
    require(allocation['lease_name'] == lease['name'] and
            allocation['guard_directory'] == '/run/phase1/' + lease['name'], 'Lease differs from allocation')
    require(time.time() + allocation['shard_timeout_seconds'] + lease['recovery_seconds'] + 600 <
            lease['deadline_epoch'], 'Lease cannot fit staging, measurement bound and recovery')
    runtime = checked(package['runtime_bundle'])
    runtime_manifest = checked(package['runtime_manifest'])
    require(read(runtime_manifest)['source_commit'] == manifest['source_commit'], 'Wrong runtime bundle source')
    binding_dir = Path(package['portable_binding_directory'])
    support_dir = Path(package['portable_support_directory'])
    transport = read(binding_dir / 'transport.json')
    support = read(support_dir / 'manifest.remote.json')
    support_transport = read(support_dir / 'transport.json')
    require(sha(support_dir / 'manifest.remote.json') == support_transport['manifest_sha256'],
            'Portable support manifest changed')
    for name, digest in support_transport['tools'].items():
        require(sha(support_dir / 'tools' / name) == digest == sha(Path(__file__).parent / name),
                'Portable worker differs from current launcher: ' + name)
    for field in manifest:
        if field not in ('bindings', 'payloads'):
            require(logical(support[field]) == logical(manifest[field]), 'Transport changed manifest: ' + field)
    require(transport['source_execution']['sha256'] == manifest['bindings']['runpod']['sha256'],
            'Portable binding belongs to another input set')
    for item in transport['files']:
        require(sha(binding_dir / item['path']) == item['sha256'], 'Portable input changed')
    destination = PurePosixPath(transport['destination'])
    support_destination = PurePosixPath(support_transport['destination'])
    require(destination.parent == PurePosixPath(COMMON) and support_destination.parent == destination.parent,
            'Unexpected remote destination')
    require(support['bindings']['runpod']['sha256'] == sha(binding_dir / 'execution.remote.json') and
            support['payloads']['runpod']['sha256'] == sha(binding_dir / 'payload/sources.json'),
            'Support points to different binding/payload')
    hardware = allocation['hardware_receipts']
    qualified = profile(read(checked(hardware['host'])), read(checked(hardware['cgroups'])))
    external = checked(package['external_guard'])
    ssh_key = Path(package['ssh_private_key'])
    require(ssh_key.is_file(), 'SSH key unavailable')
    archive = control / 'formal-inputs.tar.gz'
    with tarfile.open(archive, 'x:gz') as tar:
        for directory, name in ((binding_dir, destination.name), (support_dir, support_destination.name)):
            require(not any(p.is_symlink() for p in directory.rglob('*')), 'Unexpected symlink in input package')
            tar.add(directory, arcname=name)
    write(control / 'formal-manifest.json', manifest)
    write(control / 'controller-inputs.json', dict(package=package, archive=pin(archive), controller=pin(__file__)))
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as handle:
        key = os.environ.get('RUNPOD_API_KEY') or winreg.QueryValueEx(handle, 'RUNPOD_API_KEY')[0]
    os.environ['RUNPOD_API_KEY'] = key
    remote_control = COMMON + '/' + control.name
    api = created = ssh = scp = worker = None
    remote_ready = False
    started = time.monotonic()
    result = dict(complete=False, formal_measurement=False, formal_verdict=None,
                  durable_output_directory=str(destination / 'outputs'), volume_id=lease['network_volume_id'])

    def remote(code, timeout=20, read_only=False):
        for attempt in range(3 if read_only else 1):
            try:
                return subprocess.check_output(ssh + ['python3 -c ' + shlex.quote(code)],
                                               text=True, timeout=timeout, stderr=subprocess.STDOUT)
            except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
                if not read_only or attempt == 2:
                    raise
                time.sleep(2)

    def recover():
        if ssh is None or not remote_ready:
            return
        code = ('import pathlib,tarfile,hashlib,json;control=pathlib.Path(' + repr(remote_control) +
                ');panel=pathlib.Path(' + repr(str(destination)) + ');out=control/"results.tar.gz";'
                't=tarfile.open(out,"w:gz");'
                '[t.add(control/name,arcname=name) for name in ("worker","worker.log") if (control/name).exists()];'
                't.add(panel/"outputs",arcname="outputs") if (panel/"outputs").exists() else None;'
                't.close();print(json.dumps(dict(sha256=hashlib.sha256(out.read_bytes()).hexdigest(),bytes=out.stat().st_size)))')
        packed = json.loads(remote(code, timeout=120))
        target = control / 'results.tar.gz'
        subprocess.run(scp + ['root@' + endpoint['ip'] + ':' + remote_control + '/results.tar.gz', str(target)],
                       check=True, timeout=120)
        require(sha(target) == packed['sha256'], 'Recovered archive differs')
        recovered = control / 'recovered'
        recovered.mkdir()
        with tarfile.open(target) as tar:
            tar.extractall(recovered, filter='data')
        write(control / 'recovery.json', dict(**packed, archive=pin(target)))
        result['recovered'] = True
        completed = recovered / 'worker/completion.json'
        if completed.exists():
            result['formal_measurement'] |= bool(read(completed).get('rows'))

    def observation_gap(error):
        write(control / 'observation-gap.json', dict(epoch=time.time(), error_type=type(error).__name__,
                                                    interpretation='Observation unavailable, not proof of stopped work'), replace=True)
        require(time.time() < lease['deadline_epoch'] - lease['recovery_seconds'], 'Reached mandatory recovery window')
        time.sleep(20)

    def stop_worker():
        if worker is None or result['complete']:
            return
        code = ('import pathlib,json,os,time;g=pathlib.Path(' + repr(allocation['guard_directory']) + ');'
                's=json.loads((g/"guard.json").read_bytes());assert s["pod_id"]==' + repr(created['id']) + ';'
                'p=g/"stop-request.json";tmp=g/"stop-request.controller.partial";'
                'tmp.write_text(json.dumps(dict(pod_id=s["pod_id"],reason="controller_interrupted",epoch=time.time())));'
                'os.replace(tmp,p);root=pathlib.Path(' + repr(remote_control + '/worker') + ');'
                'deadline=time.monotonic()+30\n'
                'while not (root/"completion.json").exists() and time.monotonic()<deadline:time.sleep(1)\n'
                'assert (root/"completion.json").exists(),"Worker did not stop within recovery bound"')
        remote(code, timeout=40)

    try:
        guard = subprocess.Popen([sys.executable, str(external), str(control)],
                                 creationflags=subprocess.CREATE_NO_WINDOW | subprocess.DETACHED_PROCESS |
                                 subprocess.CREATE_NEW_PROCESS_GROUP,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        write(control / 'external-guard-process.json', dict(pid=guard.pid, started_epoch=time.time()))
        created = execute_create(prepared_template(control / 'lease'), control / 'lease',
                                 control / 'funding.json', lease['network_volume_id'])
        api = Provider(key, created['id'])
        endpoint = None
        for _ in range(60):
            pod = api.call('GET')
            require(pod is not None, 'Pod absent during startup')
            require(verify_pod(pod, lease, created['id']) <= lease['rate_ceiling_usd_hour'], 'Rate above guard cap')
            port = (pod.get('portMappings') or {}).get('22')
            if pod.get('publicIp') and port:
                endpoint = dict(ip=pod['publicIp'], port=int(port))
                break
            time.sleep(5)
        require(endpoint is not None, 'SSH endpoint not ready')
        write(control / 'endpoint.json', endpoint)
        options = ['-i', str(ssh_key), '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
                   '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2',
                   '-o', 'UserKnownHostsFile=' + str(control / 'known_hosts')]
        ssh = ['ssh', *options, '-p', str(endpoint['port']), '-o', 'StrictHostKeyChecking=accept-new', 'root@' + endpoint['ip']]
        scp = ['scp', '-q', *options, '-P', str(endpoint['port']), '-o', 'StrictHostKeyChecking=yes']
        for _ in range(30):
            try:
                state = json.loads(remote('import pathlib;print(pathlib.Path(' +
                                         repr(allocation['guard_directory'] + '/guard.json') + ').read_text())'))
                if state['provider_ok'] and state['allow_new_dispatch'] and not state['latched']:
                    break
            except (subprocess.SubprocessError, json.JSONDecodeError):
                pass
            time.sleep(5)
        else:
            raise TimeoutError('Resident guard did not become ready')
        write(control / 'guard-armed.json', state)
        code = ('import pathlib,os,json,shutil;p=pathlib.Path;'
                'host=dict(cpu_info=p("/proc/cpuinfo").read_text(),affinity=sorted(os.sched_getaffinity(0)),'
                'memory=p("/proc/meminfo").read_text(),disk=shutil.disk_usage("/workspace")._asdict());'
                'groups={name:p(name).read_text() for name in ' + repr(CGROUP_FILES) + ' if p(name).exists()};'
                'print(json.dumps([host,groups]))')
        observed = json.loads(remote(code, read_only=True))
        actual = profile(*observed)
        write(control / 'actual-hardware.json', dict(host=observed[0], cgroups=observed[1], profile=actual))
        require_compatible(actual, qualified)
        remote('import pathlib;pathlib.Path(' + repr(remote_control) + ').mkdir(parents=True)')
        remote_ready = True
        for file, name in ((runtime, 'runtime.tar.gz'), (archive, 'inputs.tar.gz')):
            subprocess.run(scp + [str(file), 'root@' + endpoint['ip'] + ':' + remote_control + '/' + name],
                           check=True, timeout=120)
        code = ('import pathlib,hashlib,tarfile;root=pathlib.Path(' + repr(COMMON) + ');'
                'control=pathlib.Path(' + repr(remote_control) + ');'
                'assert hashlib.sha256((control/"runtime.tar.gz").read_bytes()).hexdigest()==' + repr(sha(runtime)) + ';'
                'assert hashlib.sha256((control/"inputs.tar.gz").read_bytes()).hexdigest()==' + repr(sha(archive)) + ';'
                'm=root/"manifest.json";assert not m.exists() or hashlib.sha256(m.read_bytes()).hexdigest()==' + repr(sha(runtime_manifest)) + ';'
                'tarfile.open(control/"runtime.tar.gz").extractall(root,filter="data") if not m.exists() else None;'
                'assert not (root/' + repr(destination.name) + ').exists();'
                'assert not (root/' + repr(support_destination.name) + ').exists();'
                'tarfile.open(control/"inputs.tar.gz").extractall(root,filter="data")')
        remote(code, timeout=120)
        write(control / 'staging.json', dict(seconds=time.monotonic()-started))
        argv = ['python3', '-u', str(support_destination / 'tools/g115_d3_launch_v1.py'), '--manifest',
                str(support_destination / 'manifest.remote.json'), '--host', 'runpod', '--root', remote_control + '/worker']
        # The worker runs its full on-host checks before any native subprocess.
        code = ('import pathlib,os,subprocess,json,time;root=pathlib.Path(' + repr(remote_control) + ');'
                'env=os.environ.copy();env.pop("RUNPOD_API_KEY",None);env["RUNPOD_POD_ID"]=' + repr(created['id']) + ';'
                'log=(root/"worker.log").open("wb");child=subprocess.Popen(' + repr(argv) +
                ',env=env,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,start_new_session=True);'
                'print(json.dumps(dict(pid=child.pid,epoch=time.time())))')
        worker = json.loads(remote(code))
        write(control / 'worker-started.json', worker)
        while True:
            try:
                pod = api.call('GET')
            except (OSError, TimeoutError, urllib.error.URLError) as error:
                observation_gap(error)
                continue
            require(pod is not None, 'Pod absent before recovery; volume retained')
            code = ('import pathlib,json,os;root=pathlib.Path(' + repr(remote_control + '/worker') + ');out={}\n'
                    'for name in ("progress.json","completion.json"):\n'
                    ' p=root/name\n if p.exists():out[name]=json.loads(p.read_bytes())\n'
                    'try:os.kill(' + str(worker['pid']) + ',0);out["worker_alive"]=True\n'
                    'except ProcessLookupError:out["worker_alive"]=False\nprint(json.dumps(out))')
            try:
                snapshot = json.loads(remote(code, read_only=True))
            except (subprocess.SubprocessError, json.JSONDecodeError) as error:
                observation_gap(error)
                continue
            write(control / 'worker-observation.json', snapshot, replace=True)
            progress = snapshot.get('progress.json', {})
            completion = snapshot.get('completion.json')
            result['formal_measurement'] |= bool(progress.get('native_alive') or
                                                 completion and completion.get('rows'))
            if completion is not None:
                result['complete'] = completion['complete']
                break
            require(snapshot['worker_alive'], 'Worker stopped before completion')
            time.sleep(20)
    except Exception as error:
        result.update(error_type=type(error).__name__, error=str(error))
    finally:
        if api is not None:
            try:
                stop_worker()
            except Exception as error:
                result['stop_error_type'] = type(error).__name__
            try:
                recover()
            except Exception as error:
                result.update(recovery_error_type=type(error).__name__, recovery_error=str(error))
            write(control / 'release-authorized.json', dict(epoch=time.time()), replace=True)
            try:
                for _ in range(12):
                    pod = api.call('GET')
                    if pod is None:
                        write(control / 'release-confirmed.json', dict(pod_id=created['id'], provider_absent=True, epoch=time.time()))
                        result['release_confirmed'] = True
                        break
                    verify_pod_identity(pod, lease, created['id'])
                    api.call('DELETE')
                    time.sleep(5)
            except Exception as error:
                result['release_error_type'] = type(error).__name__
        result['seconds'] = time.monotonic()-started
        write(control / 'completion.json', result)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--package', type=Path)
    parser.add_argument('--control', type=Path)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    manifest = read(args.manifest)
    if not args.execute:
        _, _, _, jobs = validate_plan(manifest, 'runpod')
        print(json.dumps(dict(planning_valid=True, jobs=len(jobs), allocated=False, native_dispatched=False)))
        return
    require(args.package is not None and args.control is not None, 'Package and prepared lease required')
    result = execute(manifest, read(args.package), args.control)
    print(json.dumps(result))
    raise SystemExit(0 if result['complete'] and result.get('recovered') and result.get('release_confirmed') else 1)


if __name__ == '__main__':
    main()
