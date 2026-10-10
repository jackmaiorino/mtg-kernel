"""Nine-deck campaign on one RunPod CPU lease (v1): package, plan and execute.

package  offline. A Linux runtime (the trainer and a runtime.json pinning the
         image's loader/libc/libm/libgcc_s), the python/tools closure the
         campaign driver, dispatcher, host reservation, host_slots and lease
         guard need on the Pod, and the campaign rewritten for the Pod with
         every input it references. Each archive is a tar.gz of regular files;
         package.json pins every member. Pod paths live under
         /workspace/<campaign root> on the network volume, so a later lease
         resumes the same campaign.
plan     a package against a prepared lease (phase1_cloud/prepare_lease.py):
         lease validity and age, the eight-hour cap, startup and recovery
         bounds, the pinned image, the Pod shape, hardware expectations and
         that a block of every run fits. Allocates nothing.
execute  only with --execute: the D3 lease cycle (g115_d3_cloud_v1.execute)
         for training. External guard, Pod, resident guard, hardware,
         resumable staging, the campaign driver, observation, recovery,
         release and an independent absence check. Without --execute it plans.

The API key comes from the environment (the user registry on Windows only), is
held in memory and never written. See docs/nine_deck_cloud_v1.md.
"""
from __future__ import annotations

import argparse
import ast
import base64
import contextlib
import copy
import gzip
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
from pathlib import Path, PurePosixPath

import native_expanded_dispatch_v1 as native_dispatch
import nine_deck_baseline_v1 as ndb
import nine_deck_campaign_v1 as driver
from g115_d3_cloud_host_v1 import CGROUP_FILES, profile, require_compatible

native_dispatch.lease_guard()  # phase1_cloud joins sys.path; its modules import their siblings by bare name
import lease_guard
import prepare_lease
import verify_pod_absent
from common import (
    encoded,
    pin,
    read,
    relative,
    require,
    require_fresh_funding_snapshot,
    sha,
    write,
)
from runtime_observation import IMAGE_REFERENCE

TOOLS = Path(__file__).resolve().parent
EXTERNAL_GUARD = TOOLS / 'phase1_cloud' / 'external_guard.py'
SCHEMA = 'nine-deck-cloud-package/v1'
SPEC_SCHEMA = 'nine-deck-cloud-package-spec/v1'
WORKSPACE = PurePosixPath('/workspace')  # the prepared Pod's network-volume mount
GUARD_PARENT = '/run/phase1/'  # prepare_lease's resident guard state, one directory per lease
LOCK_ROOT = '/var/lib/mtg-node/host-lock'  # Pod-local: a reservation never outlives its Pod
# The pinned image (IMAGE_REFERENCE: Debian bookworm, glibc 2.36), from its base layer.
LIBRARY_DIRECTORY = '/usr/lib/x86_64-linux-gnu/'
IMAGE_LIBRARIES = {
    'ld-linux-x86-64.so.2': '02bcda52c1a5dfc236f94d9e5255b4a0e26347d8a372a5223b650e31f291ce3c',
    'libc.so.6': '6b4a45352fd0c540a9c7c718f35ce8c8e46a4e482f9d3885a910c32d1a0e1421',
    'libm.so.6': '7f2ca87f652f56b094462474b076749e90e689d0ecb9cb63c7679820b271b4e7',
    'libgcc_s.so.1': '2bd1552c47799ef67e701e81d4383061fd76059868e446e63560f0dd0d5ec14e'}
CLOSURE_ROOTS = ('nine_deck_campaign_v1.py', 'native_expanded_dispatch_v1.py', 'host_reservation_v1.py',
                 'phase1_cloud/lease_guard.py')
HOST_SLOTS = 'host_slots_v1.py'
# Required external input while host_slots_v1.py is not in this tree: spellbench
# e7db6861, byte-identical to mtg-kernel 3d00a457.
HOST_SLOTS_SHA256 = 'ae0e5b92be8fce9a4dfe944388c55321322497eb3ec0954baefb121e6393cf16'
DECK_COMPANIONS = ('data/cards_v1.json', ndb.REGISTRATIONS_PATH)  # load_decks reads them beside the decks file
DESCRIPTORS = ('play_import',)  # input descriptors whose own pins move with them; other inputs keep their bytes
ROOTS = ('state', 'cold', 'retained', 'checkpoints', 'exposure', 'hot')
TEMPLATE_KEYS = {'lane', 'decks', 't1_source', 'a48_source', 'placement', 'runtime', 'storage', 'wall_seconds',
                 'choice', 'choice_verification', 'cold_keep_blocks', 'roots', 'compute_host_name', 'host_slots',
                 'lease_block_seconds'}
POD_SHAPE = {'computeType': 'CPU', 'cpuFlavorIds': ['cpu3c'], 'vcpuCount': 32, 'interruptible': False}  # verify_pod
LEASE_CAP_SECONDS = 8 * 3600
FRESH_SECONDS = 300  # prepared lease age accepted before allocation, as prepare_lease and the D3 controller
STARTUP_SECONDS = 900  # boot, SSH, resident guard, hardware check and staging before the driver starts
RECOVERY_SECONDS = 600  # stop, archive, transfer and verify outputs, then release
POLL_SECONDS = 20


def is_pin(value):
    return isinstance(value, dict) and isinstance(value.get('path'), str) and isinstance(value.get('sha256'), str)


def safe_name(name):
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', name), 'unsafe input file name: ' + name)
    return name


def checked_pin(item):
    """A local input pin: a regular file with the pinned SHA256 (and size, when pinned)."""
    actual = pin(item['path'], item['sha256'])
    require('bytes' not in item or item['bytes'] == actual['bytes'], 'input size differs: ' + item['path'])
    return actual


@contextlib.contextmanager
def tar_writer(path):
    """A deterministic tar.gz: regular files only, fixed owner and times."""
    with open(path, 'xb') as raw, gzip.GzipFile(filename='', fileobj=raw, mode='wb', mtime=0) as stream, \
            tarfile.open(fileobj=stream, mode='w', format=tarfile.PAX_FORMAT) as tar:
        yield tar


def tar_info(name, size, mode):
    info = tarfile.TarInfo(name)
    info.size, info.mode, info.mtime = size, mode, 0
    return info


def verify_archive(path, members):
    """Every member is a regular file named in the manifest, with its size and SHA256, and none is missing."""
    expected, seen = {item['path']: item for item in members}, set()
    with tarfile.open(path, 'r:gz') as tar:
        for info in tar:
            require(info.isreg() and info.name in expected and info.name not in seen,
                    'unexpected archive member: ' + info.name)
            digest = hashlib.sha256()
            with tar.extractfile(info) as stream:
                for chunk in iter(lambda: stream.read(1 << 20), b''):
                    digest.update(chunk)
            item = expected[info.name]
            require(digest.hexdigest() == item['sha256'] and info.size == item['bytes'],
                    'archive member differs: ' + info.name)
            seen.add(info.name)
    require(seen == set(expected), 'archive lacks members: ' + ', '.join(sorted(set(expected) - seen)[:5]))


class Bundle:
    """One package archive's members, by Pod path relative to the campaign root."""

    def __init__(self):
        self.members = {}

    def file(self, path, source, mode=0o644):
        item = pin(source)
        return self.add(path, item['sha256'], item['bytes'], mode, source=Path(source))

    def data(self, path, value, mode=0o644):
        return self.add(path, hashlib.sha256(value).hexdigest(), len(value), mode, data=value)

    def add(self, path, digest, size, mode, source=None, data=None):
        name = str(relative(str(path)))
        previous = self.members.get(name)
        require(previous is None or previous[0]['sha256'] == digest, 'two inputs claim one Pod path: ' + name)
        entry = {'path': name, 'sha256': digest, 'bytes': size, 'mode': mode}
        self.members[name] = (entry, source, data)
        return entry

    def write(self, path):
        with tar_writer(path) as tar:
            for name in sorted(self.members):
                entry, source, data = self.members[name]
                info = tar_info(name, entry['bytes'], entry['mode'])
                if data is None:
                    with open(source, 'rb') as stream:
                        tar.addfile(info, stream)
                else:
                    tar.addfile(info, io.BytesIO(data))
        members = [self.members[name][0] for name in sorted(self.members)]
        verify_archive(path, members)  # also catches a source that changed while it was read
        return {'file': Path(path).name, 'sha256': sha(path), 'bytes': Path(path).stat().st_size, 'members': members}


# ---------------------------------------------------------------- tooling


def module_file(name, search):
    for base in search:
        path = base.joinpath(*name.split('.'))
        if path.with_suffix('.py').is_file():
            return path.with_suffix('.py')
        if (path / '__init__.py').is_file():
            return path / '__init__.py'
    return None


def imported(path, search):
    """Local files one file imports (lazy imports included); anything else must be standard library."""
    found = []
    for node in ast.walk(ast.parse(Path(path).read_bytes(), str(path))):
        if isinstance(node, ast.Import):
            names = [alias.name for alias in node.names]
        elif isinstance(node, ast.ImportFrom):
            require(not node.level and node.module, 'relative import in the Pod tooling: ' + str(path))
            names = [node.module] + [node.module + '.' + alias.name for alias in node.names]
        else:
            continue
        for name in names:
            parts = name.split('.')
            files = [module_file('.'.join(parts[:count]), search) for count in range(1, len(parts) + 1)]
            if any(files):
                found.extend(file for file in files if file)
            else:
                require(parts[0] in sys.stdlib_module_names or parts[0] == '__future__',
                        'Pod tooling imports a module outside python/tools and the standard library: ' + name)
    return found


def closure(tools=TOOLS, roots=CLOSURE_ROOTS):
    """python/tools files the Pod entry points reach by import. The search path is the
    Pod's: python/tools, then phase1_cloud (appended by native_expanded_dispatch_v1)."""
    search, found, pending = (tools, tools / 'phase1_cloud'), set(), [tools / root for root in roots]
    while pending:
        path = pending.pop()
        if path not in found:
            found.add(path)
            pending.extend(imported(path, search))
    return sorted(path.relative_to(tools).as_posix() for path in found)


CLOSURE_PROBE = '''import importlib, json, sys
from pathlib import Path
tools, before = Path(sys.argv[1]).resolve(), set(sys.modules)
sys.path[:0] = [str(tools), str(tools / "phase1_cloud")]
for name in json.loads(sys.argv[2]):
    importlib.import_module(name)
print(json.dumps(sorted(name for name, module in list(sys.modules.items())
                        if name not in before and getattr(module, "__file__", None)
                        and name.split(".")[0] not in sys.stdlib_module_names
                        and not Path(module.__file__).resolve().is_relative_to(tools))))
'''


def module_names(files):
    names = []
    for name in files:
        name = name.removeprefix('phase1_cloud/').removesuffix('.py').removesuffix('/__init__')
        if name != 'host_slots_v1':  # run as a script; its imports are checked separately
            names.append(name.replace('/', '.'))
    return names


def require_closure_imports(tools):
    """Import every closure module from a tree that holds only the closure, in an isolated interpreter."""
    files = sorted(path.relative_to(tools).as_posix() for path in Path(tools).rglob('*.py'))
    done = subprocess.run([sys.executable, '-I', '-S', '-B', '-c', CLOSURE_PROBE, str(tools), json.dumps(module_names(files))],
                          capture_output=True, text=True, timeout=120, check=False)
    require(done.returncode == 0, 'Pod tooling closure is incomplete: ' + done.stderr.strip()[-800:])
    outside = json.loads(done.stdout.strip().splitlines()[-1])
    require(not outside, 'Pod tooling loads modules from outside its closure: ' + ', '.join(outside))
    return files


def build_tooling(bundle, base, host_slots=None):
    sources = {name: TOOLS / name for name in closure()}
    if (TOOLS / HOST_SLOTS).is_file():
        sources[HOST_SLOTS], provenance = TOOLS / HOST_SLOTS, 'tree'
    else:
        require(host_slots, HOST_SLOTS + ' is not in this tree: pass its pinned copy with --host-slots')
        pin(host_slots, HOST_SLOTS_SHA256)
        sources[HOST_SLOTS], provenance = Path(host_slots), 'external'
    require(not imported(sources[HOST_SLOTS], ()), HOST_SLOTS + ' must need only the standard library')
    digests = {name: pin(path)['sha256'] for name, path in sources.items()}
    identity = hashlib.sha256(''.join(f'{name} {digests[name]}\n' for name in sorted(digests)).encode()).hexdigest()[:16]
    with tempfile.TemporaryDirectory() as temporary:
        for name, path in sources.items():
            target = Path(temporary, name)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        require_closure_imports(temporary)
    directory = PurePosixPath('tooling') / identity
    for name, path in sources.items():
        bundle.file(directory / 'python/tools' / name, path)
    root = base / directory
    return {'id': identity, 'root': str(root), 'modules': sorted(sources),
            'host_slots': {'source': provenance, 'sha256': digests[HOST_SLOTS],
                           'path': str(root / 'python/tools' / HOST_SLOTS)}}


# ---------------------------------------------------------------- runtime and inputs


def build_runtime(spec, base, bundle):
    require(re.fullmatch('[0-9a-f]{40}', spec['engine_commit']) and re.fullmatch('[0-9a-f]{64}', spec['tracked_tree_sha256']),
            'engine commit and tracked tree SHA256 required')
    binary = Path(spec['binary'])
    item = pin(binary)
    with binary.open('rb') as stream:
        head = stream.read(20)
    require(len(head) == 20 and head[:6] == b'\x7fELF\x02\x01' and head[18:20] == b'\x3e\x00',
            'Linux x86-64 trainer binary required')
    directory = PurePosixPath('runtime') / item['sha256']
    name = safe_name(binary.name)
    bundle.file(directory / name, binary, mode=0o755)
    runtime = {'engine_commit': spec['engine_commit'], 'tracked_tree_sha256': spec['tracked_tree_sha256'],
               'binary': {'path': str(base / directory / name), 'sha256': item['sha256']},
               'platform': native_dispatch.PLATFORM,
               'libraries': {name: {'path': LIBRARY_DIRECTORY + name, 'sha256': digest}
                             for name, digest in IMAGE_LIBRARIES.items()}}
    require(sorted(runtime['libraries']) == sorted(native_dispatch.LIBRARIES), 'runtime library set differs')
    entry = bundle.data(directory / 'runtime.json', encoded(runtime))
    return {'path': str(base / directory / 'runtime.json'), 'sha256': entry['sha256']}, runtime


def cores(cpus):
    cpus = sorted(cpus)
    if len(cpus) == 1:
        return str(cpus[0])
    return f'{cpus[0]}-{cpus[-1]}' if cpus == list(range(cpus[0], cpus[-1] + 1)) else ','.join(map(str, cpus))


def checked_placement(placement):
    cpus, workers, preparation = placement['cpu_affinity'], placement['workers'], placement['preparation_workers']
    require(set(placement) == {'cpu_affinity', 'workers', 'preparation_workers', 'memory_bytes'}, 'placement fields differ')
    require(cpus and len(set(cpus)) == len(cpus) and all(type(cpu) is int and 0 <= cpu for cpu in cpus),
            'invalid placement CPU set')
    require(type(workers) is int and 1 <= workers <= min(64, len(cpus)) and type(preparation) is int and
            1 <= preparation <= len(cpus), 'invalid placement worker counts')
    require(type(placement['memory_bytes']) is int and placement['memory_bytes'] > 0, 'invalid memory limit')
    return {'host': 'runpod', **placement}


def checked_storage(storage, base):
    require(set(storage) == {'max_logical_bytes', 'reserve_bytes', 'projected_additional_bytes'}, 'storage fields differ')
    require(all(type(value) is int and value >= 0 for value in storage.values()), 'invalid storage numbers')
    require(0 < storage['max_logical_bytes'] <= native_dispatch.CAP, 'storage cap exceeds 192 GiB')
    require(storage['reserve_bytes'] >= native_dispatch.DISK_RESERVE_BYTES, 'reserve below 60 GiB')
    return {**storage, 'accounting_roots': [str(base / 'campaign')],
            'projected_volume_bytes': {'': storage['projected_additional_bytes']}}


def build_inputs(spec, template, base, bundle, runtime_pin, tooling):
    objects = PurePosixPath('inputs/objects')

    def carry(item):  # byte for byte, content-addressed
        checked_pin(item)
        path = objects / item['sha256'] / safe_name(Path(item['path']).name)
        bundle.file(path, item['path'])
        return {**item, 'path': str(base / path)}

    def descriptor(item):  # an import descriptor names its own files: they move with it
        checked_pin(item)
        value = read(item['path'])
        require(isinstance(value, dict), 'input descriptor must be a JSON object: ' + item['path'])
        data = json.dumps({key: carry(entry) if is_pin(entry) else entry for key, entry in value.items()},
                          sort_keys=True, separators=(',', ':')).encode()
        digest = hashlib.sha256(data).hexdigest()
        path = objects / digest / safe_name(Path(item['path']).name)
        bundle.data(path, data)
        return {**item, 'path': str(base / path), 'sha256': digest} | ({'bytes': len(data)} if 'bytes' in item else {})

    def source(value):
        return {key: (descriptor if key in DESCRIPTORS else carry)(entry) if is_pin(entry) else copy.deepcopy(entry)
                for key, entry in value.items()}

    decks = Path(template['decks'])
    pin(decks, ndb.RUNTIME_DECKS_SHA256)
    repository = decks.resolve().parents[1]
    pin(repository / ndb.REGISTRATIONS_PATH, ndb.REGISTRATIONS_SHA256)
    ndb.load_decks(decks)  # the nine registered decks resolve from these files
    deck_files = {'data/runtime_decks_v1.json': decks} | {name: repository / name for name in DECK_COMPANIONS}
    digests = {name: pin(path)['sha256'] for name, path in deck_files.items()}
    deck_directory = PurePosixPath('inputs/decks') / hashlib.sha256(encoded(digests)).hexdigest()[:16]
    for name, path in deck_files.items():
        bundle.file(deck_directory / name, path)
    volume = []
    for key in ('choice', 'choice_verification'):
        item = spec.get(key)
        if item is not None:
            path = PurePosixPath(item['path']) if is_pin(item) else None
            require(path is not None and path.is_relative_to(base) and '..' not in path.parts
                    and re.fullmatch('[0-9a-f]{64}', item['sha256']),
                    key + ' must be a pin on the volume under ' + str(base) + ' (from the qualification lease)')
            volume.append({'role': key, 'path': str(path), 'sha256': item['sha256']})
    require(len(volume) in (0, 2), 'pin both the choice and its verification, or neither')
    placement = checked_placement(spec['placement'])
    campaign = {'lane': template['lane'], 'decks': str(base / deck_directory / 'data/runtime_decks_v1.json'),
                't1_source': source(template['t1_source']), 'a48_source': source(template['a48_source']),
                'placement': placement, 'runtime': runtime_pin, 'storage': checked_storage(spec['storage'], base),
                'wall_seconds': template['wall_seconds'], 'choice': spec.get('choice'),
                'choice_verification': spec.get('choice_verification'), 'cold_keep_blocks': template['cold_keep_blocks'],
                'roots': {name: str(base / 'campaign' / name) for name in ROOTS} | {'hot_by_run': {}},
                'host_slots': {'cores': cores(placement['cpu_affinity']),
                               'tool': {'path': tooling['host_slots']['path'], 'sha256': tooling['host_slots']['sha256']}}}
    block = spec.get('lease_block_seconds', template.get('lease_block_seconds'))
    if block is not None:
        native_dispatch.positive(block, 'lease block seconds')
        campaign['lease_block_seconds'] = block
    native_dispatch.positive(campaign['wall_seconds'], 'wall-time limit')
    data = encoded(campaign)
    digest = hashlib.sha256(data).hexdigest()
    path = objects / digest / 'campaign.json'
    bundle.data(path, data)
    return {'path': str(base / path), 'sha256': digest}, campaign, data, volume


def qualified_hardware(path):
    """A previous lease's actual-hardware.json (this controller's receipt), recomputed."""
    if path is None:
        return None
    receipt = read(path)
    actual = profile(receipt['host'], receipt['cgroups'])
    require(actual == receipt['profile'], 'qualified hardware receipt does not reproduce its profile')
    return {'sha256': sha(path), 'profile': actual, 'affinity': receipt['host']['affinity']}


def package(spec_path, output, host_slots=None):
    spec = read(spec_path)
    require(spec.get('schema') == SPEC_SCHEMA, 'package spec schema differs')
    name = spec['campaign_root']
    require(isinstance(name, str) and re.fullmatch('[a-z0-9][a-z0-9-]{0,62}', name), 'campaign root must be a short lowercase name')
    runs = spec['runs']
    require(runs and len(set(runs)) == len(runs) and set(runs) <= set(ndb.RUNS), 'runs must be distinct labels r1..r6')
    template = read(spec['campaign'])
    require(set(template) <= TEMPLATE_KEYS, 'unknown campaign fields: ' + ', '.join(sorted(set(template) - TEMPLATE_KEYS)))
    hardware = {'min_memory_bytes': spec['hardware']['min_memory_bytes'],
                'qualified': qualified_hardware(spec['hardware'].get('qualified'))}
    require(type(hardware['min_memory_bytes']) is int and
            hardware['min_memory_bytes'] >= len(runs) * spec['placement']['memory_bytes'],
            'expected Pod memory must hold every run at the placement memory limit')
    output = Path(output)
    require(not output.exists(), 'fresh package output required')
    base = WORKSPACE / name
    bundles = {kind: Bundle() for kind in ('runtime', 'tooling', 'inputs')}
    runtime_pin, runtime = build_runtime(spec['runtime'], base, bundles['runtime'])
    tooling = build_tooling(bundles['tooling'], base, host_slots or spec.get('host_slots'))
    campaign_pin, campaign, data, volume = build_inputs(spec, template, base, bundles['inputs'], runtime_pin, tooling)
    output.mkdir(parents=True)
    archives = {kind: bundle.write(output / f'{kind}.tar.gz') for kind, bundle in bundles.items()}
    with (output / 'campaign.json').open('xb') as stream:
        stream.write(data)
    manifest = {'schema': SCHEMA, 'controller_sha256': sha(__file__), 'spec_sha256': sha(spec_path),
                'image': IMAGE_REFERENCE, 'workspace': str(WORKSPACE), 'campaign_root': name, 'pod_root': str(base),
                'runs': runs, 'placement': campaign['placement'], 'storage': campaign['storage'],
                'wall_seconds': campaign['wall_seconds'], 'lease_block_seconds': campaign.get('lease_block_seconds'),
                'hardware': hardware, 'campaign': campaign_pin, 'runtime': runtime_pin,
                'runtime_identity': native_dispatch.runtime_identity(runtime), 'tooling': tooling,
                'volume_inputs': volume, 'production_ready': bool(volume), 'archives': archives}
    write(output / 'package.json', manifest)
    return manifest


def verified_package(directory):
    directory = Path(directory)
    manifest = read(directory / 'package.json')
    require(manifest.get('schema') == SCHEMA, 'package schema differs')
    require(manifest['controller_sha256'] == sha(__file__), 'package was built by a different controller')
    members = {}
    for kind, archive in manifest['archives'].items():
        path = directory / archive['file']
        require(sha(path) == archive['sha256'], 'package archive changed: ' + kind)
        verify_archive(path, archive['members'])
        members.update({item['path']: item for item in archive['members']})
    base = PurePosixPath(manifest['pod_root'])
    for item in (manifest['campaign'], manifest['runtime']):
        name = str(PurePosixPath(item['path']).relative_to(base))
        require(members.get(name, {}).get('sha256') == item['sha256'], 'package manifest differs from its archives')
    require(sha(directory / 'campaign.json') == manifest['campaign']['sha256'], 'package campaign copy differs')
    return manifest


# ---------------------------------------------------------------- plan


def assigned(source, name):
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == name
                                                for target in node.targets):
            return ast.literal_eval(node.value)
    raise ValueError(name + ' not found')


def embedded_lease(template):
    """The lease bytes the resident guard runs: prepare_lease embeds them in the Pod entrypoint."""
    try:
        call = ast.parse(template['dockerEntrypoint'][2]).body[-1].value
        bootstrap = base64.b64decode(call.args[0].args[0].value).decode()
        return base64.b64decode(assigned(assigned(bootstrap, 'GUARD_PROGRAM'), 'BLOBS')['lease.json'])
    except (AttributeError, IndexError, KeyError, TypeError, ValueError, SyntaxError):
        raise ValueError('Pod entrypoint carries no resident guard lease') from None


def plan(package_dir, control, external_guard=EXTERNAL_GUARD, now=None):
    """Package against a prepared lease. Reads local files only; allocates nothing."""
    now = time.time() if now is None else now
    manifest = verified_package(package_dir)
    prepared = Path(control) / 'lease'
    lease = read(prepared / 'lease.json')
    projected = lease_guard.validate(lease)  # caps, eight hours, margin, funded balance, intervals
    preparation = read(prepared / 'preparation.json')
    template = prepare_lease.prepared_template(prepared)
    require(manifest['production_ready'], 'package lacks the choice and its verification from the qualification lease')
    require(sha(prepared / 'create-request-template.json') == preparation['create_request_template']['sha256'],
            'create request changed after lease preparation')
    require(0 <= now - lease['created_epoch'] <= FRESH_SECONDS, 'lease preparation expired: prepare a fresh lease')
    require(lease['deadline_epoch'] - lease['created_epoch'] <= LEASE_CAP_SECONDS, 'lease exceeds eight hours')
    require(template['imageName'] == manifest['image'] == IMAGE_REFERENCE, 'Pod image differs from the runtime image')
    require(template['name'] == lease['name'] and template['networkVolumeId'] == lease['network_volume_id']
            and template['volumeMountPath'] == manifest['workspace'] == str(WORKSPACE)
            and all(template.get(key) == value for key, value in POD_SHAPE.items()),
            'create request differs from the lease, volume or Pod shape')
    require(preparation['guard_state'] == GUARD_PARENT + lease['name'], 'resident guard state differs')
    require(embedded_lease(template) == (prepared / 'lease.json').read_bytes(),
            'lease differs from the one the resident guard runs')
    require(lease['startup_idle_seconds'] >= STARTUP_SECONDS, 'lease startup bound below the staging allowance')
    require(lease['recovery_seconds'] >= RECOVERY_SECONDS, 'lease recovery window below the recovery bound')
    cpus, qualified = manifest['placement']['cpu_affinity'], manifest['hardware']['qualified']
    require(len(cpus) <= POD_SHAPE['vcpuCount'], 'placement CPUs exceed the Pod')
    if qualified:
        require(set(cpus) <= set(qualified['affinity']) and qualified['profile']['effective_cpu_capacity'] >= len(cpus)
                and qualified['profile']['memory_limit_bytes'] >= manifest['hardware']['min_memory_bytes'],
                'placement exceeds the qualified Pod hardware')
    estimate = manifest['lease_block_seconds'] or manifest['wall_seconds']
    available = lease['deadline_epoch'] - lease['recovery_seconds'] - now - STARTUP_SECONDS
    require(available >= estimate + driver.LEASE_MARGIN_SECONDS, 'a block cannot finish inside the lease')
    require_fresh_funding_snapshot(Path(control) / 'funding.json', now)
    require(Path(external_guard).is_file(), 'external guard unavailable: ' + str(external_guard))
    return {'planning_valid': True, 'allocated': False, 'native_dispatched': False, 'lease': lease['name'],
            'pod_root': manifest['pod_root'], 'runs': manifest['runs'], 'image': template['imageName'],
            'block_estimate_seconds': estimate, 'available_block_seconds': available,
            'projected_conservative_usd': projected, 'volume_inputs': manifest['volume_inputs'],
            'external_guard_sha256': sha(external_guard)}


# ---------------------------------------------------------------- Pod programs
# Each runs as `python3 -B -` over SSH (stdin, so nothing needs shell or Windows
# quoting) after PRELUDE, and prints one JSON line. A failed need() exits nonzero.

PRELUDE = '''import json
ARGS = json.loads(%r)
def need(ok, message):
    if not ok:
        raise SystemExit(message)
def digest(path):
    import hashlib
    value = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()
def alive(pid, ticks):
    try:
        fields = open("/proc/%%d/stat" %% pid).read().rsplit(")", 1)[1].split()
        return fields[0] not in ("Z", "X") and int(fields[19]) == ticks
    except (OSError, IndexError, ValueError):
        return False
'''

POD_GUARD = '''#pod:guard
import pathlib
print(json.dumps(json.loads(pathlib.Path(ARGS["guard"], "guard.json").read_text())))
'''

POD_HARDWARE = '''#pod:hardware
import os, pathlib, platform, shutil
p = pathlib.Path
host = {"cpu_info": p("/proc/cpuinfo").read_text(), "affinity": sorted(os.sched_getaffinity(0)),
        "memory": p("/proc/meminfo").read_text(), "disk": shutil.disk_usage(ARGS["workspace"])._asdict(),
        "cpu_count": os.cpu_count(), "python": platform.python_version(), "kernel": platform.release()}
groups = {name: p(name).read_text() for name in ARGS["cgroups"] if p(name).exists()}
libraries = {name: [os.path.realpath(path), digest(path) if p(path).is_file() else None]
             for name, path in ARGS["libraries"].items()}
print(json.dumps([host, groups, libraries]))
'''

POD_CONTROL = '''#pod:control
import pathlib
pathlib.Path(ARGS["control"]).mkdir(parents=True)
print(json.dumps({"control": ARGS["control"]}))
'''

POD_PROBE = '''#pod:probe
import pathlib
root, present, conflicts = pathlib.Path(ARGS["root"]), [], []
for item in ARGS["members"]:
    target = root / item["path"]
    if not target.exists() and not target.is_symlink():
        continue
    if target.is_symlink() or not target.is_file() or not target.resolve().is_relative_to(root.resolve()):
        conflicts.append(item["path"])
    else:
        (present if digest(target) == item["sha256"] else conflicts).append(item["path"])
print(json.dumps({"present": present, "conflicts": conflicts}))
'''

POD_STAGE = '''#pod:stage
import hashlib, os, pathlib, tarfile, uuid
root, archive = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["archive"])
need(digest(archive) == ARGS["archive_sha256"], "staged archive differs")
wanted = {item["path"]: item for item in ARGS["missing"]}
with tarfile.open(archive, "r:gz") as tar:
    for info in tar:
        item = wanted.pop(info.name, None)
        need(item is not None and info.isreg() and info.size == item["bytes"], "unexpected archive member " + info.name)
        target = root / info.name
        target.parent.mkdir(parents=True, exist_ok=True)
        need(target.parent.resolve().is_relative_to(root.resolve()) and not target.exists() and not target.is_symlink(),
             "refusing to replace " + info.name)
        partial, value = target.with_name(target.name + "." + uuid.uuid4().hex + ".partial"), hashlib.sha256()
        with tar.extractfile(info) as source, partial.open("xb") as sink:
            for chunk in iter(lambda: source.read(1 << 20), b""):
                value.update(chunk)
                sink.write(chunk)
            sink.flush()
            os.fsync(sink.fileno())
        need(value.hexdigest() == item["sha256"], "archive member differs " + info.name)
        partial.chmod(item["mode"])
        os.replace(partial, target)
need(not wanted, "archive lacks " + ", ".join(sorted(wanted)))
archive.unlink()
bad = [item["path"] for item in ARGS["all"] if not (root / item["path"]).is_file() or digest(root / item["path"]) != item["sha256"]]
need(not bad, "staged files differ: " + ", ".join(bad[:5]))
print(json.dumps({"extracted": len(ARGS["missing"]), "verified": len(ARGS["all"])}))
'''

POD_LEASE = '''#pod:lease
import os, pathlib
guard = pathlib.Path(ARGS["guard"])
state = json.loads((guard / "guard.json").read_text())
need(state["pod_id"] == ARGS["pod_id"] and state["name"] == ARGS["name"], "resident guard belongs to another Pod")
partial = guard / "lease.json.controller.partial"
partial.write_bytes(ARGS["lease"].encode())
os.replace(partial, guard / "lease.json")
need(digest(guard / "lease.json") == ARGS["sha256"], "lease copy differs")
print(json.dumps({"lease": str(guard / "lease.json")}))
'''

POD_OBSERVE = '''#pod:observe
import pathlib, time
out = {"runs": {}, "epoch": time.time()}
for run in ARGS["runs"]:
    path = pathlib.Path(ARGS["state"], run + ".json")
    if path.exists():
        state = json.loads(path.read_text())
        out["runs"][run] = {"status": state.get("status"), "stop_reason": state.get("stop_reason"),
                            "error": state.get("error"),
                            "blocks_done": sorted(int(k) for k, v in state["blocks"].items() if v.get("done")),
                            "dispositions": {k: v["disposition"] for k, v in state["blocks"].items() if v.get("disposition")}}
for name in ("guard.json", "progress.json"):
    path = pathlib.Path(ARGS["guard"], name)
    out[name] = json.loads(path.read_text()) if path.exists() else None
out["stop_request"] = pathlib.Path(ARGS["guard"], "stop-request.json").exists()
out["alive"] = bool(ARGS["pid"]) and alive(ARGS["pid"], ARGS["start_ticks"])
print(json.dumps(out))
'''

POD_LAUNCH = '''#pod:launch
import os, pathlib, subprocess, sys, time
control = pathlib.Path(ARGS["control"])
env = {name: value for name, value in os.environ.items() if not name.startswith("RUNPOD_")}  # no provider key
env.update(ARGS["env"])
with (control / "launch.log").open("ab") as log:
    done = subprocess.run([sys.executable, "-B", *ARGS["argv"]], cwd=ARGS["cwd"], env=env, stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=log, timeout=300, check=False)
text = done.stdout.decode(errors="replace")
(control / "launch.stdout").write_text(text)
lines = text.strip().splitlines()
result = json.loads(lines[-1]) if done.returncode == 0 and lines else {}
need(result.get("state") == "dispatched", "campaign launch failed: " + text[-500:])
try:
    ticks = int(pathlib.Path("/proc/%d/stat" % result["pid"]).read_text().rsplit(")", 1)[1].split()[19])
except (OSError, IndexError, ValueError):
    ticks = None
print(json.dumps({"pid": result["pid"], "start_ticks": ticks, "token": result["token"], "epoch": time.time()}))
'''

POD_STOP = '''#pod:stop
import os, signal, time
if alive(ARGS["pid"], ARGS["start_ticks"]):
    os.kill(ARGS["pid"], signal.SIGTERM)  # the reservation supervisor ends its whole job
deadline = time.monotonic() + ARGS["seconds"]
while alive(ARGS["pid"], ARGS["start_ticks"]) and time.monotonic() < deadline:
    time.sleep(1)
need(not alive(ARGS["pid"], ARGS["start_ticks"]), "campaign supervisor did not stop")
print(json.dumps({"stopped": True}))
'''

POD_RECOVER = '''#pod:recover
import io, pathlib, tarfile, time
control, campaign = pathlib.Path(ARGS["control"]), pathlib.Path(ARGS["campaign"])
since = ARGS["since"] if ARGS["since"] is not None else time.time()
out, members = control / "results.tar.gz", {}
def tree(tar, root, prefix, since=None, skip=(), top=False):
    if not root.is_dir():
        return
    for path in sorted(root.rglob("*")):
        rel = path.relative_to(root)
        recent = since is None or top and len(rel.parts) == 1 or path.stat().st_mtime >= since
        if (path.is_symlink() or not path.is_file() or any(part in skip for part in rel.parts)
                or path.name.endswith((".tar.gz", ".partial")) or not recent):
            continue
        data = path.read_bytes()  # one read for both the archive and its SHA256
        name = prefix + "/" + rel.as_posix()
        info = tarfile.TarInfo(name)
        info.size, info.mtime, info.mode = len(data), int(path.stat().st_mtime), 0o644
        tar.addfile(info, io.BytesIO(data))
        members[name] = __import__("hashlib").sha256(data).hexdigest()
with tarfile.open(out, "w:gz") as tar:
    tree(tar, campaign / "state", "campaign/state", since, top=True)  # run states, events and PRUNE always
    for name in ("retained", "checkpoints", "exposure"):
        tree(tar, campaign / name, "campaign/" + name, since)
    tree(tar, campaign / "hot", "campaign/hot", since, skip=("native",))
    tree(tar, pathlib.Path(ARGS["guard"]), "guard")
    tree(tar, control, "control")
    tree(tar, pathlib.Path(ARGS["locks"]), "reservation")
    listing = json.dumps(members, sort_keys=True).encode()
    info = tarfile.TarInfo("RECOVERED.json")
    info.size = len(listing)
    tar.addfile(info, io.BytesIO(listing))
print(json.dumps({"sha256": digest(out), "bytes": out.stat().st_size, "files": len(members), "since_epoch": since}))
'''


class PodUnavailable(RuntimeError):
    """An SSH program did not complete; never evidence that work stopped."""


class Pod:
    """ssh/scp to the Pod as root, from Windows OpenSSH or Linux."""

    def __init__(self, endpoint, key, known_hosts, runner, sleep):
        options = ['-i', str(key), '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10',
                   '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2',
                   '-o', 'UserKnownHostsFile=' + str(known_hosts)]
        self.host = 'root@' + endpoint['ip']
        self.ssh = ['ssh', *options, '-p', str(endpoint['port']), '-o', 'StrictHostKeyChecking=accept-new', self.host]
        self.scp = ['scp', '-q', *options, '-P', str(endpoint['port']), '-o', 'StrictHostKeyChecking=yes']
        self.runner, self.sleep = runner, sleep

    def run(self, program, args, timeout=30, read_only=False):
        code = (PRELUDE % json.dumps(args) + program).encode()
        for attempt in range(3 if read_only else 1):
            try:
                done = self.runner([*self.ssh, 'python3', '-B', '-'], input=code, capture_output=True,
                                   timeout=timeout, check=True)
                return json.loads(done.stdout.decode().strip().splitlines()[-1])
            except (subprocess.SubprocessError, ValueError, IndexError) as error:
                if not read_only or attempt == 2:
                    detail = getattr(error, 'stderr', None) or b''
                    raise PodUnavailable(program.split('\n', 1)[0] + ': ' + type(error).__name__ + ' ' +
                                         detail.decode(errors='replace').strip()[-400:]) from error
                self.sleep(2)
        raise AssertionError('unreachable')

    def put(self, local, remote, timeout=600):
        self.runner([*self.scp, str(local), self.host + ':' + remote], capture_output=True, timeout=timeout, check=True)

    def get(self, remote, local, timeout=600):
        self.runner([*self.scp, self.host + ':' + remote, str(local)], capture_output=True, timeout=timeout, check=True)


# ---------------------------------------------------------------- execute


def api_key():
    key = os.environ.get('RUNPOD_API_KEY')
    if not key and os.name == 'nt':
        import winreg
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, 'Environment') as handle:
            key = winreg.QueryValueEx(handle, 'RUNPOD_API_KEY')[0]
    require(key, 'RUNPOD_API_KEY unavailable')
    return key


def spawn_detached(argv):
    options = ({'creationflags': subprocess.CREATE_NO_WINDOW | subprocess.DETACHED_PROCESS |
                subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else {'start_new_session': True})
    return subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, **options)


def driver_command(manifest):
    """The campaign driver's launch (it takes the host reservation and detaches its supervisor)."""
    tools = PurePosixPath(manifest['tooling']['root']) / 'python/tools'
    return [str(tools / 'nine_deck_campaign_v1.py'), 'launch', '--campaign', manifest['campaign']['path'],
            '--runs', ','.join(manifest['runs'])]


def write_delta(source, target, names):
    with tarfile.open(source, 'r:gz') as tar, tar_writer(target) as out:
        for info in tar:
            if info.name in names:
                with tar.extractfile(info) as stream:
                    out.addfile(tar_info(info.name, info.size, info.mode), stream)


class Cycle:
    """One lease, following g115_d3_cloud_v1.execute. Recovery and release follow any failure."""

    def __init__(self, package_dir, control, ssh_key, external_guard, key, recover_all, provider, create_api,
                 lookup_api, runner, spawn, sleep):
        self.package_dir, self.control, self.ssh_key = Path(package_dir), Path(control), Path(ssh_key)
        self.external_guard, self.key, self.recover_all = Path(external_guard), key, recover_all
        self.provider, self.create_api, self.lookup_api = provider or lease_guard.Provider, create_api, lookup_api
        self.runner, self.spawn, self.sleep = runner, spawn, sleep
        self.manifest = read(self.package_dir / 'package.json')
        self.lease = read(self.control / 'lease/lease.json')
        self.guard = GUARD_PARENT + self.lease['name']
        self.base = PurePosixPath(self.manifest['pod_root'])
        self.remote_control = str(self.base / 'control' / self.lease['name'])
        self.created = self.api = self.pod = self.worker = None
        self.remote_ready = self.ended = False
        self.before = {}
        self.result = {'complete': False, 'lease_stopped': False, 'recovered': False, 'release_confirmed': False,
                       'lease': self.lease['name'], 'volume_id': self.lease['network_volume_id'],
                       'pod_root': str(self.base), 'runs': self.manifest['runs']}

    def run(self):
        started = time.monotonic()
        try:
            self.start_external_guard()
            self.create()
            self.connect()
            self.wait_guard()
            self.check_hardware()
            self.pod.run(POD_CONTROL, {'control': self.remote_control})
            self.remote_ready = True
            self.stage()
            self.check_volume_inputs()
            self.place_lease()
            self.before = self.observe()['runs']
            self.count(self.before)
            if all((self.before.get(run) or {}).get('status') == 'complete' for run in self.manifest['runs']):
                self.result['complete'] = self.ended = True  # nothing left: recover and release
            else:
                self.launch()
                self.watch()
        except Exception as error:  # noqa: BLE001 - recovery and release follow any failure
            self.result.update(error_type=type(error).__name__, error=str(error))
        finally:
            for step, field in ((self.stop_worker, 'stop'), (self.recover, 'recovery'), (self.release, 'release')):
                try:
                    step()
                except Exception as error:  # noqa: BLE001 - each later step still runs
                    self.result.update({field + '_error_type': type(error).__name__, field + '_error': str(error)})
            self.result['seconds'] = time.monotonic() - started
            write(self.control / 'completion.json', self.result, replace=True)
        return self.result

    def start_external_guard(self):
        process = self.spawn([sys.executable, str(self.external_guard), str(self.control)])
        write(self.control / 'external-guard-process.json',
              {'pid': process.pid, 'started_epoch': time.time(), 'script': pin(self.external_guard)})

    def create(self):
        self.created = prepare_lease.execute_create(
            prepare_lease.prepared_template(self.control / 'lease'), self.control / 'lease',
            self.control / 'funding.json', self.lease['network_volume_id'], api=self.create_api)
        self.result['pod_id'] = self.created['id']
        self.api = self.provider(self.key, self.created['id'])

    def connect(self):
        endpoint = None
        for _ in range(60):
            try:
                pod = self.api.call('GET')
            except RuntimeError:
                self.sleep(5)
                continue
            require(pod is not None, 'Pod absent during startup')
            require(lease_guard.verify_pod(pod, self.lease, self.created['id']) <= self.lease['rate_ceiling_usd_hour'],
                    'rate above guard cap')
            port = (pod.get('portMappings') or {}).get('22')
            if pod.get('publicIp') and port:
                endpoint = {'ip': pod['publicIp'], 'port': int(port)}
                break
            self.sleep(5)
        require(endpoint is not None, 'SSH endpoint not ready')
        write(self.control / 'endpoint.json', endpoint)
        self.pod = Pod(endpoint, self.ssh_key, self.control / 'known_hosts', self.runner, self.sleep)

    def wait_guard(self):
        for _ in range(30):
            try:
                state = self.pod.run(POD_GUARD, {'guard': self.guard}, timeout=20)
                if state['provider_ok'] and state['allow_new_dispatch'] and not state['latched'] \
                        and state['pod_id'] == self.created['id']:
                    break
            except (PodUnavailable, KeyError, TypeError):
                pass
            self.sleep(5)
        else:
            raise TimeoutError('resident guard did not become ready')
        write(self.control / 'guard-armed.json', state)

    def check_hardware(self):
        host, groups, libraries = self.pod.run(
            POD_HARDWARE, {'workspace': self.manifest['workspace'], 'cgroups': CGROUP_FILES,
                           'libraries': {name: LIBRARY_DIRECTORY + name for name in IMAGE_LIBRARIES}}, read_only=True)
        actual = profile(host, groups)
        write(self.control / 'actual-hardware.json', {'host': host, 'cgroups': groups, 'libraries': libraries,
                                                      'profile': actual})
        require(libraries == {name: [LIBRARY_DIRECTORY + name, digest] for name, digest in IMAGE_LIBRARIES.items()},
                'Pod image libraries differ from the runtime pins')
        cpus, hardware, storage = self.manifest['placement']['cpu_affinity'], self.manifest['hardware'], self.manifest['storage']
        require(set(cpus) <= set(host['affinity']) and actual['effective_cpu_capacity'] >= len(cpus),
                'placement CPUs unavailable on the Pod')
        require(actual['memory_limit_bytes'] >= hardware['min_memory_bytes'], 'Pod memory below the expectation')
        require(host['disk']['free'] >= storage['reserve_bytes'] + storage['projected_volume_bytes'][''],
                'volume free space below the dispatcher reserve plus projection')
        if hardware['qualified']:
            require_compatible(actual, hardware['qualified']['profile'])

    def stage(self):
        """Upload only members the volume lacks; never replace different bytes."""
        started, record = time.monotonic(), {}
        for kind in ('runtime', 'tooling', 'inputs'):
            archive = self.manifest['archives'][kind]
            members = archive['members']
            probe = self.pod.run(POD_PROBE, {'root': str(self.base), 'members': members}, timeout=600)
            require(not probe['conflicts'], 'volume holds different bytes at ' + ', '.join(probe['conflicts'][:5]))
            present = set(probe['present'])
            missing = [item for item in members if item['path'] not in present]
            record[kind] = {'members': len(members), 'present': len(present), 'uploaded': len(missing)}
            if missing:
                delta = self.control / f'stage-{kind}.tar.gz'
                write_delta(self.package_dir / archive['file'], delta, {item['path'] for item in missing})
                remote = self.remote_control + '/' + delta.name
                self.pod.put(delta, remote, timeout=1800)
                self.pod.run(POD_STAGE, {'root': str(self.base), 'archive': remote, 'archive_sha256': sha(delta),
                                         'missing': missing, 'all': members}, timeout=1800)
                record[kind]['archive'] = pin(delta)
        write(self.control / 'staging.json', {'seconds': time.monotonic() - started, **record})

    def check_volume_inputs(self):
        inputs = [{'path': str(PurePosixPath(item['path']).relative_to(self.base)), 'sha256': item['sha256']}
                  for item in self.manifest['volume_inputs']]
        probe = self.pod.run(POD_PROBE, {'root': str(self.base), 'members': inputs}, timeout=120)
        absent = [item['path'] for item in inputs if item['path'] not in probe['present']]
        require(not absent, 'required volume inputs missing or different: ' + ', '.join(absent))

    def place_lease(self):
        """The dispatcher reads the guard's exact lease beside guard.json."""
        path = self.control / 'lease/lease.json'
        self.pod.run(POD_LEASE, {'guard': self.guard, 'pod_id': self.created['id'], 'name': self.lease['name'],
                                 'lease': path.read_bytes().decode(), 'sha256': sha(path)})

    def observe(self):
        worker = self.worker or {}
        return self.pod.run(POD_OBSERVE, {'runs': self.manifest['runs'], 'guard': self.guard,
                                          'state': str(self.base / 'campaign/state'), 'pid': worker.get('pid'),
                                          'start_ticks': worker.get('start_ticks')}, read_only=True)

    def count(self, runs):
        before = self.before
        self.result['blocks_completed'] = {run: len((runs.get(run) or {}).get('blocks_done', []))
                                           for run in self.manifest['runs']}
        self.result['blocks_completed_this_lease'] = {
            run: count - len((before.get(run) or {}).get('blocks_done', []))
            for run, count in self.result['blocks_completed'].items()}

    def launch(self):
        require(time.time() - self.lease['created_epoch'] < self.lease['startup_idle_seconds'] - 60,
                'startup outlasted the guards\' startup idle bound')
        env = {'RUNPOD_POD_ID': self.created['id'], native_dispatch.LEASE_GUARD_ENV: self.guard,
               'MTG_HOST_LOCK_ROOT': LOCK_ROOT, 'HOST_SLOTS_ROOT': LOCK_ROOT, 'PYTHONDONTWRITEBYTECODE': '1'}
        self.worker = self.pod.run(POD_LAUNCH, {'control': self.remote_control, 'argv': driver_command(self.manifest),
                                                'cwd': self.manifest['tooling']['root'], 'env': env}, timeout=360)
        write(self.control / 'worker-started.json', {**self.worker, 'pod_id': self.created['id']})

    def gap(self, error):
        write(self.control / 'observation-gap.json',
              {'epoch': time.time(), 'error_type': type(error).__name__,
               'interpretation': 'Observation unavailable, not proof of stopped work'}, replace=True)
        require(time.time() < self.lease['deadline_epoch'] - self.lease['recovery_seconds'],
                'reached mandatory recovery window')
        self.sleep(POLL_SECONDS)

    def watch(self):
        while True:
            try:
                pod = self.api.call('GET')
            except (OSError, RuntimeError, urllib.error.URLError) as error:
                self.gap(error)
                continue
            require(pod is not None, 'Pod absent before recovery; volume retained')
            lease_guard.verify_pod_identity(pod, self.lease, self.created['id'])
            try:
                snapshot = self.observe()
            except PodUnavailable as error:
                self.gap(error)
                continue
            write(self.control / 'worker-observation.json', snapshot, replace=True)
            self.count(snapshot['runs'])
            if not snapshot['alive'] or (snapshot['progress.json'] or {}).get('finished'):
                for _ in range(30):  # the driver marks progress finished just before it exits
                    if not snapshot['alive']:
                        break
                    self.sleep(2)
                    snapshot = self.observe()
                self.ended = not snapshot['alive']
                return self.conclude(snapshot)
            latched = (snapshot['guard.json'] or {}).get('latched')
            require(not latched, 'resident guard latched: ' + str(latched))
            require(time.time() < self.lease['deadline_epoch'] - self.lease['recovery_seconds'],
                    'reached mandatory recovery window')
            self.sleep(POLL_SECONDS)

    def conclude(self, snapshot):
        runs = snapshot['runs']
        self.count(runs)
        status = {run: (runs.get(run) or {}).get('status') for run in self.manifest['runs']}
        reasons = {run: runs[run]['stop_reason'] for run in status if (runs.get(run) or {}).get('stop_reason')}
        self.result.update(run_status=status, stop_reasons=reasons)
        if all(value == 'complete' for value in status.values()):
            self.result['complete'] = True
            return
        require(not any(value in (None, 'pending', 'running') for value in status.values()),
                'campaign driver stopped before its runs finished: ' + json.dumps(status))
        exhausted = {run for run, reason in reasons.items() if reason == 'lease_time_exhausted' and status[run] == 'stopped'}
        require(exhausted and all(status[run] == 'complete' or run in exhausted for run in status),
                'campaign ended without completing or a lease stop: ' + json.dumps(
                    {run: runs.get(run) for run in status}))
        self.result['lease_stopped'] = True

    def stop_worker(self):
        if self.worker is not None and not self.ended:
            self.pod.run(POD_STOP, {'pid': self.worker['pid'], 'start_ticks': self.worker['start_ticks'],
                                    'seconds': 30}, timeout=60)
            self.ended = True

    def recover(self):
        if not self.remote_ready:
            return
        since = 0 if self.recover_all else None if self.worker is None else self.worker['epoch'] - 60
        packed = self.pod.run(POD_RECOVER, {'control': self.remote_control, 'campaign': str(self.base / 'campaign'),
                                            'guard': self.guard, 'locks': LOCK_ROOT, 'since': since},
                              timeout=RECOVERY_SECONDS)
        target, recovered = self.control / 'results.tar.gz', self.control / 'recovered'
        self.pod.get(self.remote_control + '/results.tar.gz', target, timeout=RECOVERY_SECONDS)
        require(sha(target) == packed['sha256'], 'recovered archive differs')
        recovered.mkdir()
        with tarfile.open(target) as tar:
            tar.extractall(recovered, filter='data')
        listed = read(recovered / 'RECOVERED.json')
        present = {path.relative_to(recovered).as_posix() for path in recovered.rglob('*') if path.is_file()}
        require(present == set(listed) | {'RECOVERED.json'} and len(listed) == packed['files'] and
                all(sha(recovered / name) == digest for name, digest in listed.items()), 'recovered files differ')
        write(self.control / 'recovery.json', {**packed, 'archive': pin(target)})
        runs = {}
        for run in self.manifest['runs']:
            path = recovered / 'campaign/state' / (run + '.json')
            if path.exists():
                state = read(path)
                runs[run] = {'blocks_done': [key for key, value in state['blocks'].items() if value.get('done')]}
        self.count(runs)
        self.result['recovered'] = True

    def release(self):
        """Authorize the external guard, DELETE the exact Pod, then confirm absence independently."""
        write(self.control / 'release-authorized.json', {'epoch': time.time()}, replace=True)
        if self.created is None:
            return  # a lost create response still leaves the external guard its exact-name release
        api = self.api or self.provider(self.key, self.created['id'])
        for _ in range(12):
            try:
                pod = api.call('GET')
                if pod is None:
                    break
                lease_guard.verify_pod_identity(pod, self.lease, self.created['id'])
                api.call('DELETE')
            except RuntimeError as error:
                self.result['release_retry_error'] = str(error)
            self.sleep(5)
        receipt, path = verify_pod_absent.verify_absence(self.created['id'], self.control, api=self.lookup_api,
                                                         sleep=self.sleep)
        if receipt['provider_absent'] is True:
            write(self.control / 'release-confirmed.json', {'pod_id': self.created['id'], 'provider_absent': True,
                                                            'receipt': pin(path), 'epoch': time.time()})
            self.result['release_confirmed'] = True


def execute(package_dir, control, ssh_key, external_guard=EXTERNAL_GUARD, recover_all=False, provider=None,
            create_api=None, lookup_api=None, runner=subprocess.run, spawn=spawn_detached, sleep=time.sleep):
    # Planning completes before any credential is read or the external guard starts.
    planned = plan(package_dir, control, external_guard)
    require(Path(ssh_key).is_file(), 'SSH key unavailable')
    write(Path(control) / 'plan.json', planned)
    write(Path(control) / 'controller-inputs.json', {'package': pin(Path(package_dir) / 'package.json'),
                                                     'controller': pin(__file__), 'external_guard': pin(external_guard)})
    key = api_key()
    os.environ['RUNPOD_API_KEY'] = key  # memory only: execute_create, the external guard and the absence check
    return Cycle(package_dir, control, ssh_key, external_guard, key, recover_all, provider, create_api, lookup_api,
                 runner, spawn, sleep).run()


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest='op', required=True)
    packing = commands.add_parser('package', help='offline: runtime, tooling and inputs archives')
    packing.add_argument('--spec', type=Path, required=True)
    packing.add_argument('--output', type=Path, required=True)
    packing.add_argument('--host-slots', type=Path, help='pinned host_slots_v1.py while it is not in this tree')
    for name in ('plan', 'execute'):
        command = commands.add_parser(name)
        command.add_argument('--package', type=Path, required=True)
        command.add_argument('--control', type=Path, required=True, help='holds lease/ (prepare_lease) and funding.json')
        command.add_argument('--external-guard', type=Path, default=EXTERNAL_GUARD)
        if name == 'execute':
            command.add_argument('--ssh-key', type=Path)
            command.add_argument('--recover-all', action='store_true', help='recover every output, not this lease\'s')
            command.add_argument('--execute', action='store_true', help='allocate the Pod (paid compute)')
    args = parser.parse_args(argv)
    if args.op == 'package':
        manifest = package(args.spec, args.output, args.host_slots)
        print(json.dumps({'package': str(args.output), 'pod_root': manifest['pod_root'],
                          'production_ready': manifest['production_ready'],
                          'archives': {kind: item['sha256'] for kind, item in manifest['archives'].items()}}))
        return 0
    if args.op == 'plan' or not args.execute:  # without --execute nothing is allocated
        print(json.dumps(plan(args.package, args.control, args.external_guard)))
        return 0
    require(args.ssh_key is not None, '--ssh-key is required with --execute')
    result = execute(args.package, args.control, args.ssh_key, args.external_guard, args.recover_all)
    print(json.dumps(result))
    return 0 if (result['complete'] or result['lease_stopped']) and result['recovered'] and result['release_confirmed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
