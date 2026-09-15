"""Offline ordinary-origin BO3 payload. No worker, allocation or native execution.

The new Linux build manifest must include actual generated card/feature inputs.
Old BO1 packages and Windows BO3 checkpoint graphs are deliberately unsupported.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import sys
import tarfile

sys.dont_write_bytecode = True
from common import encoded, pin, read, require, sha, write
from fresh_source import inspect_source, is_fresh_origin, validate_checkpoint_origin, validate_inference_identity
from pack import Packager

MIB = 1024 ** 2
RESERVE = 16 * 1024 ** 3
MAX_PAYLOAD = 2 * 1024 ** 3
RUN_SCHEMA = 'mtg-kernel-native-bo3-training-run/v1'
CHECKPOINT_SCHEMA = 'mtg-kernel-expanded-deck-checkpoint/v1'
FRESH_CHECKPOINT_SCHEMA = 'mtg-kernel-expanded-deck-fresh-checkpoint/v1'
FRESH_SOURCE_SCHEMA = 'mtg-kernel-fresh-initialization-source/v1'
FEATURE_FILES = {
    'feature_identity': 'data/flat_policy_v3/feature_identity.rs',
    'feature_descriptor': 'data/flat_policy_v3/feature_contract_v3.json',
    'feature_source': 'python/mtg_kernel_rl/features_v6.py',
    'source_registry': 'data/cards_v1.json',
    'toolchain': 'rust-toolchain.toml',
    'cargo_lock': 'Cargo.lock',
}
FEATURE_CONSTANTS = {
    'feature_schema_version': 'FEATURE_SCHEMA_VERSION_V3',
    'feature_registry_version': 'FEATURE_REGISTRY_VERSION_V3',
    'feature_contract_digest': 'FEATURE_CONTRACT_DIGEST_V3',
    'feature_encoding_digest': 'FEATURE_ENCODING_DIGEST_V3',
    'features_source_sha256': 'FEATURES_SOURCE_SHA256_V3',
    'feature_descriptor_sha256': 'FEATURE_DESCRIPTOR_SHA256_V3',
}
RUNTIME_FIELDS = {'executable', 'toolchain', 'engine_commit', 'tracked_tree_sha256',
    'tracked_tree_contract', 'build_git_clean', 'card_db_hash', 'card_registry_sha256',
    'feature_contract_digest', 'feature_encoding_digest', 'features_source_sha256', 'feature_descriptor_sha256'}


def shape(value, fields, why):
    require(isinstance(value, dict) and set(value) == set(fields), why)


def integer(value, low, high, why):
    require(type(value) is int and low <= value <= high, why)


def local_artifact(path):
    """One concrete host spelling bridge for locally mounted WSL build evidence."""
    if os.name == 'nt':
        match = re.fullmatch(r'/mnt/([a-z])/(.+)', str(path))
        if match:
            return Path(match[1].upper() + ':/' + match[2])
    return Path(path)


def checked(item, maximum=512 * MIB, build_artifact=False):
    require(isinstance(item, dict) and {'path', 'sha256'} <= set(item)
        and re.fullmatch('[0-9a-f]{64}', item['sha256']), 'explicit SHA256 file pin required')
    path = local_artifact(item['path']) if build_artifact else Path(item['path'])
    require(path.is_absolute() and path.is_file() and path.stat().st_size <= maximum, 'bounded absolute input required')
    actual = pin(path, item['sha256'])
    require('bytes' not in item or actual['bytes'] == item['bytes'], 'pinned size differs')
    return actual


def document(item, maximum=MIB, build_artifact=False):
    actual = checked(item, maximum, build_artifact)
    return read(actual['path']), actual


class Bo3Packager(Packager):
    def room(self, extra):
        require(sum(row['bytes'] for row in self.files.values()) + extra <= MAX_PAYLOAD,
            'BO3 package payload exceeds 2 GiB preparation bound')
        require(shutil.disk_usage(self.output).free >= RESERVE + extra,
            'packaging must preserve 16 GiB free space')

    def put(self, source, destination, expected=None, executable=False):
        self.room(0 if destination in self.files else Path(source).stat().st_size)
        return super().put(source, destination, expected, executable)

    def document(self, value, name):
        size = len(encoded(value))
        require(size <= 32 * MIB, 'package metadata exceeds 32 MiB')
        self.room(size)
        return super().document(value, name)


def fields_from_source(build, records):
    script, _ = document(records['build_script'])
    artifact, _ = document(records['compiler_artifact'])
    receipt, _ = document(records['build_receipt'])
    env = script['env']
    require(all(isinstance(row, list) and len(row) == 2 for row in env)
        and len({row[0] for row in env}) == len(env), 'ambiguous build-script environment')
    require(script['reason'] == 'build-script-executed' and dict(env) == build['compiled_fields'], 'compiled environment differs')
    require(artifact['reason'] == 'compiler-artifact' and artifact['package_id'] == script['package_id']
        and artifact['target']['name'] == 'phase1_bo3_trainable_v1' and artifact['target']['kind'] == ['bin']
        and artifact['features'] == ['native-training-store-v2-production']
        and artifact['profile']['test'] is False and artifact['profile']['debug_assertions'] is False
        and artifact['profile']['opt_level'] == '3', 'actual release CPU runner artifact required')
    require(receipt['build_exit_code'] == 0 and receipt['error'] is None
        and receipt['source_commit'] == build['source_commit']
        and receipt['actual_artifact_path'] == artifact['executable'], 'build receipt/artifact differs')
    generated_origin = build['generated_card_defs_origin']
    require(isinstance(generated_origin, dict) and generated_origin['path'] == str(PurePosixPath(script['out_dir']) / 'card_defs.rs')
        and generated_origin['sha256'] == build['generated_card_defs']['sha256'], 'generated card file lacks exact build-script origin')
    # The build helper seals the origin before copying it. Do not require the
    # reusable Cargo target to remain unchanged after that sealed copy exists.
    require(generated_origin.get('bytes', records['generated_card_defs']['bytes']) == records['generated_card_defs']['bytes'],
        'generated card copy size differs from original observation')
    raw_card = Path(records['generated_card_defs']['path']).read_text(encoding='utf-8')
    card_hashes = re.findall(r'^pub const KERNEL_CARDDB_HASH: u64 = 0x([0-9a-f]{16});$', raw_card, re.MULTILINE)
    require(len(card_hashes) == 1, 'actual generated card hash is missing or ambiguous')
    raw_identity = Path(records['feature_identity']['path']).read_text(encoding='utf-8')
    fields = {}
    for name, constant in FEATURE_CONSTANTS.items():
        matches = re.findall(r'^pub\(crate\) const ' + constant + r': &str = "([^"\r\n]+)";$', raw_identity, re.MULTILINE)
        require(len(matches) == 1, 'missing or ambiguous feature constant: ' + constant)
        fields[name] = matches[0]
    descriptor, _ = document(records['feature_descriptor'])
    require(fields['features_source_sha256'] == records['feature_source']['sha256']
        and fields['feature_descriptor_sha256'] == records['feature_descriptor']['sha256'], 'actual feature bytes differ from compiled constants')
    for name in ('feature_schema_version', 'feature_registry_version', 'feature_contract_digest', 'feature_encoding_digest', 'features_source_sha256'):
        require(descriptor[name] == fields[name], 'feature descriptor differs: ' + name)
    require(descriptor['features_source_path'] == FEATURE_FILES['feature_source'], 'feature source path differs')
    # These exact files must come from the sealed clean checkout archive, not
    # merely from files with persuasive names beside the binary.
    with tarfile.open(records['source_archive']['path'], 'r:') as archive:
        names = archive.getnames()
        for key, name in FEATURE_FILES.items():
            require(names.count(name) == 1, 'missing or duplicate source archive member: ' + name)
            member = archive.getmember(name)
            require(member.isfile() and member.size == records[key]['bytes'] and member.size <= 16 * MIB,
                'source archive member is not the expected bounded regular file')
            with archive.extractfile(member) as stream:
                require(hashlib.file_digest(stream, 'sha256').hexdigest() == records[key]['sha256'], 'copied build input differs from clean archive')
    return fields, card_hashes[0]


def validate_run(config):
    shape(config, {'schema', 'initial_learner', 'initial_input', 'previous_progress', 'learning_rate_bits',
        'value_coefficient_bits', 'opponents', 'batches', 'preparation_limits', 'collection_workers', 'output_directory'}, 'strict finite run fields required')
    require(config['schema'] == RUN_SCHEMA and config['previous_progress'] is None, 'only a fresh ordinary origin without prior progress is supported')
    shape(config['initial_input'], {'kind', 'learner'}, 'strict ordinary input required')
    require(config['initial_input']['kind'] == 'ordinary_checkpoint_transition'
        and config['initial_input']['learner'] == config['initial_learner']['gameplay'], 'BO3 checkpoint transport or mismatched optimizer source is unsupported')
    integer(config['collection_workers'], 1, 32, 'worker bound differs')
    integer(len(config['batches']), 1, 4096, 'batch bound differs')
    require(len(config['opponents']) <= 128, 'fixed roster exceeds bound')
    for key in ('learning_rate_bits', 'value_coefficient_bits'):
        integer(config[key], 0, 2 ** 32 - 1, 'scalar bits outside binary32')
        value = struct.unpack('<f', struct.pack('<I', config[key]))[0]
        require(math.isfinite(value) and value > 0, 'positive finite optimizer scalar required')
    shape(config['preparation_limits'], {'max_input_bytes', 'max_prepared_payload_bytes'}, 'strict preparation limits required')
    integer(config['preparation_limits']['max_input_bytes'], 1, 512 * MIB, 'input bound differs')
    integer(config['preparation_limits']['max_prepared_payload_bytes'], 1, 256 * MIB, 'payload bound differs')
    names = set()
    for entry in config['opponents']:
        shape(entry, {'id', 'package'}, 'strict named opponent required')
        require(isinstance(entry['id'], str) and 0 < len(entry['id']) <= 128 and entry['id'] not in names, 'duplicate or invalid opponent id')
        names.add(entry['id'])
    total, matches = 0, set()
    for index, batch in enumerate(config['batches']):
        shape(batch, {'matches'}, 'strict batch required')
        integer(len(batch['matches']), 1, 32, 'match count outside native batch bound')
        total += len(batch['matches']); result_bytes = 0
        for item in batch['matches']:
            shape(item, {'config', 'learner_seat', 'opponent', 'capture_limits', 'max_result_bytes'}, 'strict scheduled match required')
            require(item['learner_seat'] in ('p0', 'p1'), 'unknown learner seat')
            game = item['config']
            shape(game, {'schema', 'match_id', 'seed', 'initial_chooser', 'deck_ids', 'registrations', 'summary_tags',
                'max_physical_games', 'max_physical_decisions', 'max_policy_steps', 'max_decision_records', 'max_decision_json_bytes'}, 'strict collection configuration required')
            require(game['schema'] == 'mtg-kernel-bo3-collection-config/v1' and isinstance(game['match_id'], str)
                and 0 < len(game['match_id']) <= 256 and game['match_id'] not in matches, 'invalid or duplicate match id')
            matches.add(game['match_id'])
            integer(game['seed'], 0, 2 ** 64 - 1, 'seed outside u64')
            require(game['initial_chooser'] in ('p0', 'p1') and len(game['registrations']) == len(game['deck_ids']) == 2,
                'two registrations and explicit chooser required')
            shape(item['capture_limits'], {'max_payload_bytes', 'max_json_bytes'}, 'strict capture limits required')
            for value in item['capture_limits'].values():
                integer(value, 1, 256 * MIB, 'capture bound differs')
            integer(item['max_result_bytes'], 1, 512 * MIB, 'result bound differs')
            result_bytes += item['max_result_bytes']
            opponent = item['opponent']; kind = opponent.get('kind')
            if kind == 'fixed':
                shape(opponent, {'kind', 'id'}, 'strict fixed reference required')
                require(opponent['id'] in names, 'unknown fixed opponent')
            elif kind == 'completed_batch':
                shape(opponent, {'kind', 'index'}, 'strict historical reference required')
                integer(opponent['index'], 0, index - 1, 'future historical opponent')
            else:
                shape(opponent, {'kind'}, 'strict initial/current reference required')
                require(kind in ('initial', 'current'), 'unsupported opponent reference')
        require(result_bytes < config['preparation_limits']['max_input_bytes'], 'result declarations exhaust batch input bound')
    require(total <= 65536, 'finite schedule exceeds exact attempt ledger')


def validate_package(package, fields, card_hash, registry_sha, checkpoint_cache):
    shape(package, {'schema', 'runtime', 'gameplay', 'gameplay_sampler_identity', 'opening', 'play_draw', 'sideboard', 'search'}, 'strict complete package required')
    require(package['schema'] == 'mtg-kernel-complete-agent-package/v1'
        and package['opening'] == {'kind': 'existing', 'protocol': 'keep_seven_v2'}
        and package['sideboard'] == {'kind': 'keep'} and package['search'] == {'kind': 'disabled'}
        and package['play_draw'] in ({'kind': 'fixed', 'choice': 'play'}, {'kind': 'fixed', 'choice': 'draw'}),
        'initial BO3 packaging requires fixed Keep auxiliaries and disabled search')
    shape(package['runtime'], RUNTIME_FIELDS, 'unknown source runtime fields')
    behavior = package['gameplay']; shape(behavior, {'source', 'identity'}, 'strict gameplay behavior required')
    source, identity = behavior['source'], behavior['identity']
    shape(source, {'play_import', 'feature_transfer', 'checkpoint'}, 'ordinary checkpoint source required')
    require(source['checkpoint'] is not None, 'checkpoint required for every roster member')
    require(package['gameplay_sampler_identity'] == 'f32-q8-expq63-hamilton-splitmix64-wide-v1', 'qualified wide sampler required')
    import_doc, import_pin = document(source['play_import'], 4 * MIB)
    fresh = import_doc.get('schema') == FRESH_SOURCE_SCHEMA
    fresh_info = None
    if fresh:
        # Verify only the descriptor's actual two input pins. Historical
        # producer paths inside the immutable initialization are not followed.
        fresh_info = inspect_source(source['play_import'], source['feature_transfer'])
        require(identity['source_import'] == fresh_info['origin'], 'fresh initialization ancestry differs from recorded inference')
        validate_inference_identity(identity, source, expected_origin=fresh_info['origin'])
    else:
        require('schema' not in import_doc and {'export_directory', 'source_run_path', 'source_registry_path',
            'expected_metadata_sha256', 'expected_model_parameter_sha256', 'expected_source_registry_sha256',
            'source_registry_git_commit', 'expected_destination_card_db_hash'} == set(import_doc),
            'only ordinary imported or fresh-initialization sources are supported')
        require(not is_fresh_origin(identity['source_import']), 'imported descriptor cannot carry a fresh inference origin')
    expected_checkpoint_schema = FRESH_CHECKPOINT_SCHEMA if fresh else CHECKPOINT_SCHEMA
    checkpoint_pin = checked(source['checkpoint'], 64 * MIB)
    key = checkpoint_pin['sha256']
    if key not in checkpoint_cache:
        complete = read(checkpoint_pin['path'])
        require(complete['schema'] == expected_checkpoint_schema and 'registry_transfer' not in complete,
            'existing BO3 or registry-transfer checkpoint transport is unsupported')
        if fresh:
            validate_checkpoint_origin(complete, expected_origin=fresh_info['origin'],
                                       expected_weights_sha256=identity['model']['weights_sha256'])
        checkpoint_cache[key] = {name: complete[name] for name in ('schema', 'loss_identity', 'state_sha256',
            'adam_step', 'source_import', 'card_db_hash', 'feature_contract_digest', 'feature_encoding_digest',
            'learning_rate_bits', 'value_coefficient_bits')}
        if fresh:
            # This expected digest was just checked against the actual tensor
            # bytes. Cache it with the header, never the full state arrays.
            checkpoint_cache[key]['actual_weights_sha256'] = identity['model']['weights_sha256']
        del complete
    saved = checkpoint_cache[key]
    if fresh:
        require(saved.get('actual_weights_sha256') == identity['model']['weights_sha256'],
                'fresh inference weights differ from validated checkpoint tensors')
        require(type(saved['adam_step']) is int and saved['adam_step'] > 0,
                'BO3 roster members require a trained ordinary checkpoint (Adam step above zero)')
    require(saved['schema'] == expected_checkpoint_schema and 'registry_transfer' not in saved
        and saved['loss_identity'] == 'terminal_reinforce_value/v3', 'existing BO3 or registry-transfer checkpoint transport is unsupported')
    require(identity['schema'] == 'mtg-kernel-expanded-deck-inference/v' + ('2' if fresh else '1')
        and identity['checkpoint_sha256'] == key and saved['state_sha256'] == identity['state_sha256']
        and saved['adam_step'] == identity['adam_step'] and saved['source_import'] == identity['source_import'],
        'recorded ordinary checkpoint identity differs')
    expected_card_hash = fresh_info['origin']['destination_card_db_hash'] if fresh else import_doc['expected_destination_card_db_hash']
    require(identity['model']['card_db_hash'] == saved['card_db_hash'] == expected_card_hash == card_hash
        and identity['source_import']['destination_registry_sha256'] == registry_sha, 'Linux card registry/content differs from installed source')
    for name in ('feature_contract_digest', 'feature_encoding_digest'):
        require(identity['model'][name] == saved[name] == fields[name], 'Linux feature identity differs from installed source')
    for name in ('feature_schema_version', 'feature_registry_version', 'features_source_sha256', 'feature_descriptor_sha256'):
        require(identity[name] == fields[name], 'Linux inference encoding differs')
    require(source['feature_transfer'] == {'expected_feature_contract_digest': fields['feature_contract_digest'],
        'expected_feature_encoding_digest': fields['feature_encoding_digest']}, 'feature-transfer identity differs')
    if fresh:
        dependencies = fresh_info['dependencies']
    else:
        metadata, metadata_pin = document({'path': str(Path(import_doc['export_directory']) / 'metadata.json'),
            'sha256': import_doc['expected_metadata_sha256']}, 4 * MIB)
        require(metadata['identity']['model_parameter_sha256'] == import_doc['expected_model_parameter_sha256'],
            'ordinary import semantic parameter digest differs')
        dependencies = [metadata_pin,
            checked({'path': str(Path(import_doc['export_directory']) / 'parameters.f32le'), 'sha256': metadata['parameter_section_sha256']}),
            checked({'path': import_doc['source_run_path'], 'sha256': metadata['identity']['loaded_run_sha256']}, 4 * MIB),
            checked({'path': import_doc['source_registry_path'], 'sha256': import_doc['expected_source_registry_sha256']}, 4 * MIB)]
    return saved, [import_pin, checkpoint_pin] + dependencies


def prepare(spec, output):
    shape(spec, {'schema', 'run_request', 'linux_build', 'remote_root'}, 'strict BO3 package spec required')
    require(spec['schema'] == 'phase1-bo3-cloud-package-spec/v1'
        and re.fullmatch(r'/opt/phase1/[a-z0-9][a-z0-9-]{0,63}', spec['remote_root']), 'stable single Linux hot namespace required')
    config, request_pin = document(spec['run_request'], 16 * MIB)
    validate_run(config)
    build, build_pin = document(spec['linux_build'], 2 * MIB, True)
    require(build['schema'] == 'phase1-bo3-trainable-linux-build/v1' and build['profile'] == 'release'
        and type(build['build_exit_code']) is int and build['build_exit_code'] == 0
        and build['native_execution'] is False, 'actual sealed clean Linux CPU build required')
    compiled = build['compiled_fields']
    require(re.fullmatch('[0-9a-f]{40}', build['source_commit'])
        and compiled['MTG_KERNEL_BUILD_GIT_HEAD'] == build['source_commit']
        and compiled['MTG_KERNEL_BUILD_GIT_CLEAN'] == 'true'
        and re.fullmatch('[0-9a-f]{64}', compiled['MTG_KERNEL_BUILD_TRACKED_TREE_SHA256']), 'clean compiled Linux source identity required')
    keys = set(FEATURE_FILES) | {'binary', 'build_script', 'compiler_artifact', 'build_receipt',
        'build_script_output', 'source_archive', 'toolchain_measurement', 'generated_card_defs'}
    require(keys <= set(build) and 'generated_card_defs_origin' in build,
        'Linux manifest lacks sealed generated card/feature inputs; old build manifests are unsupported')
    large = {'binary', 'source_archive'}
    records = {key: checked(build[key], (512 if key in large else 16) * MIB, True) for key in keys}
    require(PurePosixPath(build['binary']['path']).name == 'phase1_bo3_trainable_v1'
        and build['binaries'].get('phase1_bo3_trainable_v1') == build['binary'], 'exact Linux BO3 binary required')
    with Path(records['binary']['path']).open('rb') as stream:
        require(stream.read(4) == b'\x7fELF', 'Linux ELF header required')
    fields, card_hash = fields_from_source(build, records)
    packages = [config['initial_learner']] + [item['package'] for item in config['opponents']]
    cache, original_pins = {}, [request_pin, build_pin] + list(records.values())
    for index, package in enumerate(packages):
        saved, inputs = validate_package(package, fields, card_hash, records['source_registry']['sha256'], cache)
        original_pins.extend(inputs)
        if index == 0:
            require(saved['learning_rate_bits'] == config['learning_rate_bits']
                and saved['value_coefficient_bits'] == config['value_coefficient_bits'], 'initial optimizer scalars differ')
    # Let the shared Packager enforce injective ordinary-source relocation.
    # All roster members are validated above, including unused fixed opponents.
    p = Bo3Packager(output, spec['remote_root'])
    relocated_runtime = {'executable': p.put(records['binary']['path'], 'bin/phase1_bo3_trainable_v1', records['binary']['sha256'], True),
        'toolchain': p.put(records['toolchain']['path'], 'provenance/rust-toolchain.toml', records['toolchain']['sha256']),
        'engine_commit': build['source_commit'], 'tracked_tree_sha256': compiled['MTG_KERNEL_BUILD_TRACKED_TREE_SHA256'],
        'tracked_tree_contract': compiled['MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT'], 'build_git_clean': True,
        'card_db_hash': card_hash, 'card_registry_sha256': records['source_registry']['sha256'],
        **{key: fields[key] for key in ('feature_contract_digest', 'feature_encoding_digest', 'features_source_sha256', 'feature_descriptor_sha256')}}
    mapped_packages = []
    for package in packages:
        new = copy.deepcopy(package)
        new['runtime'] = copy.deepcopy(relocated_runtime)
        new['gameplay']['source'] = p.model(package['gameplay']['source'])
        require(new['gameplay']['identity'] == package['gameplay']['identity'], 'relocation changed inference identity')
        mapped_packages.append(new)
    relocated = copy.deepcopy(config)
    relocated['initial_learner'] = mapped_packages[0]
    relocated['initial_input']['learner'] = copy.deepcopy(mapped_packages[0]['gameplay'])
    for entry, package in zip(relocated['opponents'], mapped_packages[1:]):
        entry['package'] = package
    relocated['output_directory'] = str(p.remote / 'run')
    validate_run(relocated)
    run_pin = p.document(relocated, 'config/run.json')
    require(p.files['config/run.json']['bytes'] <= 16 * MIB, 'relocated run exceeds native parser bound')
    p.put(request_pin['path'], 'provenance/original-run-request.json', request_pin['sha256'])
    p.put(build_pin['path'], 'provenance/linux-build-manifest.json', build_pin['sha256'])
    for key, record in sorted(records.items()):
        if key not in ('binary', 'toolchain'):
            name = 'provenance/' + key + '/' + Path(record['path']).name
            p.put(record['path'], name, record['sha256'])
    for name in ('pack_bo3.py', 'pack.py', 'common.py', 'throughput.py', 'semantic.py', 'fresh_source.py'):
        p.put(Path(__file__).with_name(name), 'provenance/helpers/' + name)
    for item in original_pins + p.input_pins:
        pin(item['path'], item['sha256'])
    for name, row in p.files.items():
        require(pin(p.output / 'payload' / name, row['sha256'])['bytes'] == row['bytes'], 'copied payload changed')
    manifest = {'schema': 'phase1-bo3-cloud-payload/v1', 'status': 'packaged-not-executed',
        'remote_root': str(p.remote), 'run_request': run_pin, 'runtime': relocated_runtime,
        'argv': [relocated_runtime['executable']['path'], 'run', '--request', run_pin['path']],
        'original_run_request': request_pin, 'linux_build': build_pin,
        'source_commit': build['source_commit'], 'generated_card_defs_origin': build['generated_card_defs_origin'],
        'model_source_relocations': p.relocations,
        'package_relocations': [{'original': old, 'relocated': new} for old, new in zip(packages, mapped_packages)],
        'files': copy.deepcopy(p.files), 'input_pins': original_pins + p.input_pins,
        'native_execution': False, 'runtime_verified': False, 'optimizer_restore_verified': False,
        'transport_scope': 'First BO3 transition from unchanged ordinary checkpoint only; preserve stable hot namespace thereafter',
        'pending': ['actual Linux current-runtime verification', 'ordinary full-state restore and update parity',
            'BO3 recovery and numerical qualification', 'BO3 cloud worker/lease/export integration'],
        'claim': 'Offline packaging only; no training dispatch, paid compute, acceleration or playing-strength claim'}
    p.document(manifest, 'manifest.json')
    archive_path = p.output / 'payload.tar'
    archive_bound = sum(row['bytes'] for row in p.files.values()) + len(p.files) * 2048 + 10240
    require(shutil.disk_usage(p.output).free >= RESERVE + archive_bound, 'archive would consume storage reserve')
    with tarfile.open(archive_path, 'x:') as archive:
        for name in sorted(p.files):
            source = p.output / 'payload' / name
            info = archive.gettarinfo(str(source), arcname=name)
            info.mtime = info.uid = info.gid = 0; info.uname = info.gname = ''
            info.mode = p.files[name]['mode']
            with source.open('rb') as stream:
                archive.addfile(info, stream)
    result = {'schema': 'phase1-bo3-cloud-package-result/v1', 'manifest': pin(p.output / 'payload/manifest.json'),
        'archive': pin(archive_path), 'file_count': len(p.files), 'engine_executed': False}
    write(p.output / 'package.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True, type=Path)
    parser.add_argument('--spec-sha256', required=True)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    spec, _ = document({'path': str(args.spec.resolve()), 'sha256': args.spec_sha256})
    print(encoded(prepare(spec, args.output)).decode(), end='')


if __name__ == '__main__':
    main()
