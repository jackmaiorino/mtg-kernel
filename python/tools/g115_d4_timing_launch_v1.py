"""Bounded expanded-training timing only; no formal or resume mode.

Invoke through g115_d3_windows_dispatch.ps1. No allocation or promotion follows
from completion. This runner preserves per-case native stores and fails on the
first timeout, changed learning state across placements, or preflight refusal.
"""
import argparse
import copy
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from g115_d4_timing_contract_v1 import bind_placement, pinned_json, validate_recipe
from windows_owned_child_policy_v1 import configure_owned_child


def pin(path):
    path = Path(path)
    return dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest())


def checked(item):
    if pin(item['path'])['sha256'] != item['sha256']:
        raise ValueError('changed pinned input')
    return Path(item['path'])


def write(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, indent=2)


def admission(manifest, host):
    if manifest.get('schema') != 'g115-d4-bounded-timing/v1' or host not in ('jack', 'haleyspc'):
        raise ValueError('only bounded D4 Windows timing supported')
    if set(manifest['configs']) != {'control', 'broader'} or manifest['workers'] != [1, 4, 8]:
        raise ValueError('timing must use two arms and declared serial/4/8 matrix')
    if manifest['native_timeout_seconds'] != 300 or manifest['total_timeout_seconds'] != 3600:
        raise ValueError('timing bounds differ')
    if Path(manifest['binary']['path']).name != 'native_expanded_training_run_v1.exe':
        raise ValueError('only the expanded trainer binary is supported')
    expected_dependencies = {
        Path(__file__).with_name('g115_d4_timing_contract_v1.py').resolve(),
        Path(__file__).with_name('windows_owned_child_policy_v1.py').resolve(),
    }
    if {Path(p['path']).resolve() for p in manifest['dependencies']} != expected_dependencies:
        raise ValueError('timing helper pins incomplete')
    for item in [manifest['binary'], manifest['documents']['launcher'], manifest['transport']['dispatcher'], *manifest['dependencies']]:
        checked(item)
    if checked(manifest['documents']['launcher']).resolve() != Path(__file__).resolve():
        raise ValueError('launcher identity differs')
    configs = {arm: pinned_json(item) for arm, item in manifest['configs'].items()}
    for config in configs.values():
        if validate_recipe(config) != dict(updates=3, games=30):
            raise ValueError('timing requires three complete thirty-game prefixes')
        for source in [config['initial_source']] + [o['source'] for o in config['opponents']]:
            if source.get('checkpoint'):
                checked(source['checkpoint'])
            descriptor = pinned_json(source['play_import'])
            if descriptor.get('schema') != 'mtg-kernel-fresh-initialization-source/v1':
                raise ValueError('unsupported initialization descriptor')
            for name in ('initialization', 'parameters'):
                checked(descriptor[name])
    placement = manifest['hosts'][host]
    if placement['computer_name'].lower() != os.environ.get('COMPUTERNAME', '').lower():
        raise ValueError('manifest belongs to another computer')
    minimum = (32 if host == 'jack' else 8) * 1024**3
    if placement['minimum_free_memory_bytes'] < minimum or placement['minimum_free_disk_bytes'] < 60 * 1024**3:
        raise ValueError('resource reserve lowered')
    if host == 'jack' and placement['gpu_ordinal'] != 1:
        raise ValueError('Jack timing preserves GPU1 reservation')
    root = Path(placement['worker_root'])
    if not root.is_absolute() or root.exists() or not root.parent.is_dir():
        raise ValueError('fresh absolute root with existing parent required')
    return configs, placement


def inventory(placement):
    if os.name != 'nt' or os.environ.get('CUDA_VISIBLE_DEVICES'):
        raise ValueError('Windows with original CUDA ordinal mapping required')
    command = "$ErrorActionPreference='Stop'; $o=Get-CimInstance Win32_OperatingSystem; $p=@(Get-CimInstance Win32_Process | Where-Object {$_.Name -match '^(native_expanded_training_run_v1|public_feature_evaluation_v1|public_feature_training_v1|public_stack_training_v1|trainer|cargo|rustc|mtg_kernel.*)\\.exe$'}); [pscustomobject]@{free_memory_bytes=([long]$o.FreePhysicalMemory*1024); active=@($p | Select-Object ProcessId,Name)} | ConvertTo-Json -Compress -Depth 4"
    data = json.loads(subprocess.check_output(['powershell.exe', '-NoProfile', '-NonInteractive', '-Command', command], text=True, creationflags=subprocess.CREATE_NO_WINDOW, timeout=30))
    if data['active'] or data['free_memory_bytes'] < placement['minimum_free_memory_bytes']:
        raise ValueError('native contention or memory reserve refusal')
    data['free_disk_bytes'] = shutil.disk_usage(Path(placement['worker_root']).parent).free
    if data['free_disk_bytes'] < placement['minimum_free_disk_bytes']:
        raise ValueError('disk reserve refusal')
    gpu = subprocess.check_output(['nvidia-smi', '--query-gpu=index,uuid,utilization.gpu,memory.used', '--format=csv,noheader,nounits'], text=True, timeout=30)
    row = next((x.split(',') for x in gpu.splitlines() if int(x.split(',')[0]) == placement['gpu_ordinal']), None)
    if row is None or row[1].strip() != placement['gpu_uuid'] or int(row[2]) != 0 or int(row[3]) >= 100:
        raise ValueError('selected GPU identity or idle state differs')
    data['gpu_inventory'] = gpu
    return data


def audit(config):
    root = Path(config['output_directory'])
    final = json.loads((root/'completion.json').read_text())
    if not final['complete'] or final['completed_iterations'] != 3:
        raise ValueError('incomplete timing prefix')
    source = config['initial_source']
    state = pinned_json(source['checkpoint'])['state_sha256']
    states, games = [], 0
    for index, batch in enumerate(config['iterations']):
        receipt = json.loads((root/'iterations'/f'{index:06}'/'complete.json').read_text())
        update, collection = pinned_json(receipt['update']), pinned_json(receipt['collection'])
        if receipt['source'] != source or update['source'] != source or collection['source'] != source:
            raise ValueError('optimizer/behavior continuation differs')
        if update['before_state_sha256'] != state or collection['behavior_state_sha256'] != state:
            raise ValueError('stale behavior state')
        if update['adam_step'] != 32401 + index or not update['checkpoint_readback'] or update['after_state_sha256'] == state:
            raise ValueError('missing actual optimizer update')
        if update['loss_identity'] != 'gae_advantage_value/v1' or update['gpu_ordinal'] != config['update_backend']['device_ordinal']:
            raise ValueError('loss/device differs')
        if len(collection['trajectories']) != 10:
            raise ValueError('incomplete synchronous batch')
        for slot, item in enumerate(collection['trajectories']):
            trajectory = pinned_json(item)
            expected = copy.deepcopy(batch['episodes'][slot]['episode'])
            assignment = batch['episodes'][slot]['opponent']
            opponent = source if assignment['kind'] == 'current' else config['initial_source']
            if assignment['kind'] == 'fixed':
                opponent = next(o['source'] for o in config['opponents'] if o['id'] == assignment['id'])
            if assignment['kind'] not in ('current', 'initial', 'fixed'):
                raise ValueError('unknown opponent assignment')
            expected['opponent'] = opponent
            if trajectory['episode'] != expected or trajectory['behavior_state_sha256'] != state or trajectory['terminal']['terminal_classification'] != 'natural':
                raise ValueError('episode, terminal or frozen batch state differs')
            games += 1
        source = dict(config['initial_source'], checkpoint=update['checkpoint'])
        state = update['after_state_sha256']
        checkpoint = pinned_json(source['checkpoint'])
        if checkpoint['state_sha256'] != state or checkpoint['adam_step'] != 32401 + index:
            raise ValueError('checkpoint continuation differs')
        states.append(state)
    if final['source'] != source or games != 30:
        raise ValueError('final timing output differs')
    return dict(full_learning_state_sha256=states, games=games, checkpoint=source['checkpoint'])


def run_cases(manifest, configs, placement, root):
    root.mkdir()
    started = time.monotonic()
    results, references = [], {}
    complete = False
    try:
        for workers in manifest['workers']:
            for arm in ('control', 'broader'):
                live = inventory(placement)
                remaining = 3600 - (time.monotonic()-started)
                if remaining < 300:
                    raise ValueError('insufficient remaining timing wall budget')
                case = root/f'w{workers}-{arm}'
                case.mkdir()
                config = bind_placement(configs[arm], case/'native', workers, placement['gpu_ordinal'])
                write(case/'config.json', config)
                write(case/'inventory.json', live)
                (case/'temp').mkdir()
                env = dict(os.environ, TEMP=str(case/'temp'), TMP=str(case/'temp'))
                env['PATH'] = placement['cuda_bin'] + ';' + env['PATH']
                child = None
                record = dict(complete=False, arm=arm, workers=workers, started_epoch=time.time())
                try:
                    with (case/'stdout.log').open('xb') as out, (case/'stderr.log').open('xb') as err:
                        child = subprocess.Popen([str(checked(manifest['binary'])), str(case/'config.json')], env=env, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
                        record['pid'] = child.pid
                        record['scheduling'] = configure_owned_child(child, manifest['binary']['path'])
                        write(case/'process.json', record)
                        record['exit_code'] = child.wait(timeout=300)
                    record['native_seconds'] = time.time()-record['started_epoch']
                    if record['exit_code'] != 0:
                        raise ValueError('native timing failed; no retry')
                    record['audit'] = audit(config)
                    reference = references.setdefault(arm, record['audit']['full_learning_state_sha256'])
                    if reference != record['audit']['full_learning_state_sha256']:
                        raise ValueError('serial/parallel full learning states differ')
                    record['complete'] = True
                except Exception as error:
                    record['error'] = repr(error)
                    raise
                finally:
                    if child is not None and child.poll() is None:
                        child.kill()
                        child.wait(timeout=10)
                    record['finished_epoch'] = time.time()
                    write(case/'execution.json', record)
                results.append(record)
        complete = True
    finally:
        write(root/'completion.json', dict(complete=complete, cases=results, seconds=time.monotonic()-started, scope='timing only; native stores retained; recovery cost and formal allocation not qualified'))


def run(manifest, configs, placement, root):
    # Kernel-owned lock disappears if this launcher dies. Never remove another
    # process's output or stale lock directory to obtain admission.
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.CreateMutexW.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p]
    kernel.CreateMutexW.restype = ctypes.c_void_p
    kernel.CloseHandle.argtypes = [ctypes.c_void_p]
    kernel.CloseHandle.restype = ctypes.c_int
    handle = kernel.CreateMutexW(None, False, 'Local\\mtg-g115-d4-bounded-timing-v1')
    if not handle:
        raise ctypes.WinError(ctypes.get_last_error())
    existed = ctypes.get_last_error() == 183
    try:
        if existed:
            raise ValueError('another D4 timing launcher owns this session')
        run_cases(manifest, configs, placement, root)
    finally:
        kernel.CloseHandle(handle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--host', choices=['jack', 'haleyspc'], required=True)
    parser.add_argument('--root', type=Path)
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding='utf-8-sig'))
    configs, placement = admission(manifest, args.host)
    live = inventory(placement)
    if args.check_only:
        print(json.dumps(dict(admitted=True, inventory=live, native_started=False)))
        return
    if args.root is None or args.root.resolve() != Path(placement['worker_root']).resolve():
        raise ValueError('worker root differs from pinned manifest')
    run(manifest, configs, placement, args.root)


if __name__ == '__main__':
    main()
