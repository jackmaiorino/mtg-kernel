"""Stage eight desktop qualification requests only; never launch or acquire."""
import copy
from datetime import datetime, timezone
import hashlib
import importlib
import json
from pathlib import Path
import shutil
import stat
import sys

ROOT = Path('D:/training-speedups-20261009/desktop')
PYTHON = Path('D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe')
CORES = list(range(0, 16, 2))
LAUNCHERS = {'baseline': 'fd095cd7e6a74c9b9d9f37c08dd38f114be9f3fefa8a0c1f79ccd69820d6cd56',
             'candidate': '5035a74db72ef61dd81a2def2b0e1d529c8fc05eba510e5cff0e4bab7b28ad88'}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def pin(path):
    with Path(path).open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def write(path, value):
    with Path(path).open('x', encoding='utf-8') as stream:
        json.dump(value, stream, separators=(',', ':'), allow_nan=False)
        stream.write('\n')
    return pin(path)


def main():
    source_path = ROOT / 'source-config.json'
    require(pin(source_path)['sha256'] == '726d432e0b9fde2216e609235efdd6101b745f1cbd121ba35907eb31098e55f3',
            'source config differs from pinned Haley schedule')
    source = json.loads(source_path.read_bytes())
    require(len(source['iterations']) == 162 and all(len(i['episodes']) == 10 for i in source['iterations']), 'full schedule differs')
    inputs = {}
    for model in [source['initial_source'], *(item['source'] for item in source['opponents'] if 'source' in item)]:
        for name in ('checkpoint', 'play_import'):
            if name in model and model[name] is not None:
                item = model[name]
                inputs[item['path']] = item
                if name == 'play_import':
                    descriptor = json.loads(Path(item['path']).read_bytes())
                    for nested in ('initialization', 'parameters'):
                        if nested in descriptor:
                            ref = descriptor[nested]
                            inputs[ref['path']] = ref
    verified = []
    for item in inputs.values():
        original = Path(item['path'])
        require(pin(original)['sha256'] == item['sha256'], 'existing desktop read-only input differs')
        suffix = original.suffix
        mirror = ROOT / 'inputs' / (item['sha256'] + suffix)
        require(not mirror.exists(), 'preserve existing owned input mirror')
        with original.open('rb') as stream, mirror.open('xb') as destination:
            shutil.copyfileobj(stream, destination)
        mirrored = pin(mirror)
        require(mirrored['sha256'] == item['sha256'], 'owned mirror differs')
        verified.append({'original': item, 'mirror': mirrored, 'bytes': original.stat().st_size,
                         'usage': 'native reads original verified absolute path; owned mirror is an additional copy'})
    require(len(verified) == 8, 'expected eight actual native source dependencies')
    for name in ('measure', 'measure/hot', 'measure/cold', 'controllers'):
        (ROOT / name).mkdir(parents=True, exist_ok=True)
    require((ROOT / 'measure/hot').stat().st_file_attributes & stat.FILE_ATTRIBUTE_COMPRESSED,
            'hot parent must inherit NTFS compression')
    require((ROOT / 'measure/cold').stat().st_file_attributes & stat.FILE_ATTRIBUTE_COMPRESSED,
            'cold parent must inherit NTFS compression')
    sys.path.insert(0, str(ROOT / 'candidate/python/tools'))
    dispatch = importlib.import_module('native_expanded_dispatch_v1')
    family = dispatch.workload(source, 'training')
    require(family == '87a756ceecf4c8c7b58221e94cb5629415d06a94639de85fddd2671d59449311',
            'desktop/Haley workload family differs')
    storage = {'accounting_roots': [str(ROOT)], 'max_logical_bytes': 160 * 1024**3,
               'reserve_bytes': 60 * 1024**3, 'projected_additional_bytes': 1024**3,
               'projected_volume_bytes': {'D:': 1024**3}}
    observed = dispatch.validate_storage(storage)
    require(observed['logical_bytes'] + 1024**3 <= storage['max_logical_bytes'], 'supplemental logical guard refuses')
    runtimes = {}
    for variant in ('baseline', 'candidate'):
        launcher = ROOT / variant / 'python/tools/native_expanded_dispatch_v1.py'
        require(pin(launcher)['sha256'] == LAUNCHERS[variant], 'frozen launcher differs from Haley receipts')
        runtime = json.loads((ROOT / 'runtimes' / (variant + '.remote.json')).read_bytes())
        binary = Path('E:/pinned-binaries') / runtime['binary']['sha256'] / 'native_expanded_training_run_v1.exe'
        require(pin(binary)['sha256'] == runtime['binary']['sha256'], 'pinned desktop binary differs')
        runtime['binary']['path'] = str(binary)
        runtimes[variant] = write(ROOT / 'runtimes' / (variant + '.json'), runtime)
    commands = []
    for variant in ('baseline', 'candidate'):
        for workers in (1, 2, 4, 8):
            label = f'qual-{variant}-w{workers}'
            config = copy.deepcopy(source)
            config['output_directory'] = str(ROOT / 'measure/hot' / label / 'native')
            config['collection_workers'] = workers
            config['preparation_workers'] = workers
            require(dispatch.workload(config, 'training') == family, 'qualification workload differs')
            config_pin = write(ROOT / 'requests' / (label + '.config.json'), config)
            request = {'schema': dispatch.SCHEMA, 'kind': 'training', 'config': config_pin,
                       'runtime': runtimes[variant], 'root': str(ROOT / 'measure/hot' / label),
                       'cold_root': str(ROOT / 'measure/cold' / label),
                       'lane': 'codex-training-speedups-20261009',
                       'placement': {'host': 'desktop', 'workers': workers, 'preparation_workers': workers,
                                     'cpu_affinity': CORES, 'memory_bytes': 32 * 1024**3},
                       'storage': storage, 'wall_seconds': 3600,
                       'comparison_logical_projection_bytes': 1024**3}
            if source.get('max_non_natural_episode_fraction', 0):
                request['non_natural_tolerance'] = source['max_non_natural_episode_fraction']
            request_path = ROOT / 'requests' / (label + '.json')
            request_pin = write(request_path, request)
            commands.append({'label': label, 'variant': variant, 'request': request_pin,
                             'argv': [str(PYTHON), '-B', str(ROOT / 'case_driver.py'),
                                      '--launcher-root', str(ROOT / variant), '--request', str(request_path),
                                      '--action', 'qualify', '--receipt', str(ROOT / 'controllers' / (label + '.json')),
                                      '--declare-desktop-cores']})
    manifest = {'schema': 'training-speedup-desktop-qualification-staging/v1', 'staging_only': True,
                'observed_utc': datetime.now(timezone.utc).isoformat(), 'source_config': pin(source_path),
                'workload_family': family, 'input_dependencies': verified, 'runtimes': runtimes,
                'frozen_launcher_sha256': LAUNCHERS, 'cpu_affinity': CORES,
                'physical_core_mapping': 'one SMT sibling of each of eight P cores: 0-1 through 14-15',
                'ci_preserved_logical_cpus': list(range(18, 24)), 'storage': storage,
                'budget': '160 GiB desktop logical allowance leaves 32 GiB of 192 GiB campaign ceiling for preserved Haley evidence; no physical compression discount',
                'guarded_command': 'supported frozen dispatcher acquires standalone host reservation, then adapter declares matching timed cores before its _qualify child',
                'controller': pin(ROOT / 'case_driver.py'), 'adapter': pin(ROOT / 'launcher_adapter.py'),
                'coordinator': pin(ROOT / 'run_qualifications.py'), 'python': str(PYTHON),
                'commands': commands, 'remaining_prerequisite': 'parent review and current resource/inventory admission before launch'}
    output = write(ROOT / 'qualification.plan.json', manifest)
    print(json.dumps({'staged': True, 'manifest': output, 'workload': family, 'requests': len(commands)}))


if __name__ == '__main__':
    main()
