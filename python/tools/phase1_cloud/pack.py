"""Prepare a Linux training/evaluation payload offline; never uploads or starts work."""
from __future__ import annotations
import argparse
import copy
import hashlib
from pathlib import Path, PurePosixPath
import re
import shutil
import tarfile
from common import encoded, pin, read, relative, require, sha, write
from throughput import training_contract, preparation_workers
from fresh_source import SOURCE_SCHEMA, inspect_source, initial_state, validate_checkpoint_origin


class Packager:
    def __init__(self, output, remote_root):
        self.output = Path(output)
        self.remote = PurePosixPath(remote_root)
        require(str(self.remote).startswith('/opt/phase1/') and '..' not in self.remote.parts,
                'remote hot root must be beneath /opt/phase1')
        require(not self.output.exists(), 'fresh package output required')
        self.output.mkdir(parents=True)
        self.files = {}; self.imports = {}; self.input_pins = []; self.relocations = []

    def put(self, source, destination, expected=None, executable=False):
        source = Path(source)
        evidence = pin(source, expected)
        name = str(relative(destination))
        target = self.output / 'payload' / name
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists():
            require(sha(target) == evidence['sha256'], 'destination collision')
        else:
            shutil.copyfile(source, target)
        self.files[name] = {'sha256': evidence['sha256'], 'bytes': evidence['bytes'],
                            'mode': 0o755 if executable else 0o644}
        self.input_pins.append(evidence)
        return {'path': str(self.remote / name), 'sha256': evidence['sha256']}

    def document(self, value, name):
        target = self.output / 'payload' / name
        write(target, value)
        row = pin(target)
        self.files[name] = {'sha256': row['sha256'], 'bytes': row['bytes'], 'mode': 0o644}
        return {'path': str(self.remote / name), 'sha256': row['sha256']}

    def imported(self, source_pin):
        source = Path(source_pin['path']); digest = source_pin['sha256']
        pin(source, digest)
        if digest in self.imports:
            return copy.deepcopy(self.imports[digest])
        document = read(source)
        expected = {'export_directory', 'source_run_path', 'source_registry_path',
                    'expected_metadata_sha256', 'expected_source_registry_sha256'}
        require(expected <= document.keys(), 'unsupported play-import contract')
        prefix = 'imports/' + digest
        self.put(source, prefix + '/original-play-import.json', digest)
        export = Path(document['export_directory'])
        metadata = read(export / 'metadata.json')
        self.put(export / 'metadata.json', prefix + '/export/metadata.json',
                 document['expected_metadata_sha256'])
        self.put(export / 'parameters.f32le', prefix + '/export/parameters.f32le',
                 metadata['parameter_section_sha256'])
        run = self.put(document['source_run_path'], prefix + '/source-run.json',
                       metadata['identity']['loaded_run_sha256'])
        registry = self.put(document['source_registry_path'], prefix + '/source-registry.json',
                            document['expected_source_registry_sha256'])
        relocated = copy.deepcopy(document)
        relocated['export_directory'] = str(self.remote / prefix / 'export')
        relocated['source_run_path'] = run['path']
        relocated['source_registry_path'] = registry['path']
        result = self.document(relocated, prefix + '/play-import.json')
        self.imports[digest] = result
        return copy.deepcopy(result)

    def model(self, source):
        result = copy.deepcopy(source)
        require('play_import' in result, 'model source has no play import')
        descriptor = read(source['play_import']['path'])
        fresh = descriptor.get('schema') == SOURCE_SCHEMA
        if fresh:
            verified = inspect_source(source['play_import'], source['feature_transfer'])
            digest = source['play_import']['sha256']; prefix = 'imports/' + digest
            if digest not in self.imports:
                self.put(source['play_import']['path'], prefix+'/original-play-import.json', digest)
                moved = copy.deepcopy(verified['descriptor'])
                for field, name in (('initialization','initialization.json'), ('parameters','parameters.f32le')):
                    item = moved[field]
                    moved[field] = self.put(item['path'], prefix+'/'+name, item['sha256'])
                self.imports[digest] = self.document(moved, prefix+'/play-import.json')
            result['play_import'] = copy.deepcopy(self.imports[digest])
        else:
            result['play_import'] = self.imported(result['play_import'])
        checkpoint = result.get('checkpoint')
        if checkpoint is not None:
            if fresh:
                pin(checkpoint['path'], checkpoint['sha256'])
                validate_checkpoint_origin(read(checkpoint['path']), verified['origin'])
            result['checkpoint'] = self.put(checkpoint['path'],
                'checkpoints/' + checkpoint['sha256'] + '.json', checkpoint['sha256'])
        row = {'original': copy.deepcopy(source), 'relocated': copy.deepcopy(result),
               'original_import_file': {'path': str(self.remote / 'imports' /
                   source['play_import']['sha256'] / 'original-play-import.json'),
                   'sha256': source['play_import']['sha256']}}
        require(all(previous['relocated']!=result or previous['original']==source
                    for previous in self.relocations),
                'non-injective model relocation: identical relocated source has unequal original references')
        if row not in self.relocations:
            self.relocations.append(row)
        return result


def prepare(spec, output):
    require(spec['schema'] == 'phase1-cloud-package-spec/v1', 'wrong package spec')
    require(re.fullmatch('[0-9a-f]{40}', spec['source_commit']), 'full source commit required')
    p = Packager(output, spec['remote_root'])
    config_path = spec['training_config']
    pin(config_path['path'], config_path['sha256'])
    config = read(config_path['path'])
    preparation_workers(config)
    require(config.get('update_backend', {'kind': 'cpu'}) == {'kind': 'cpu'},
            'this package qualifies CPU training only')
    original = copy.deepcopy(config)
    inspection = spec.get('initial_inspection')
    fresh_initial = read(config['initial_source']['play_import']['path']).get('schema') == SOURCE_SCHEMA
    if fresh_initial and config['initial_source'].get('checkpoint') is None:
        require(inspection is not None, 'fresh Adam0 training package requires native inspection evidence')
        initial_state(config['initial_source'], inspection)
    if inspection is not None:
        inspection = p.put(inspection['path'], 'provenance/initial-inspection.json', inspection['sha256'])
    config['initial_source'] = p.model(config['initial_source'])
    for opponent in config['opponents']:
        opponent['source'] = p.model(opponent['source'])
    config['output_directory'] = str(p.remote / 'run')
    configuration = p.document(config, 'config/training.json')
    evaluations = {}
    for name, config_pin in spec.get('evaluation_configs', {}).items():
        require(re.fullmatch('[a-zA-Z0-9_-]+', name), 'invalid evaluation name')
        pin(config_pin['path'], config_pin['sha256'])
        evaluation = read(config_pin['path'])
        require(evaluation['mode'] == 'run_population_batch', 'population BO3 config required')
        require(all(policy == {'kind': 'keep'} for policy in evaluation['policies']),
                'initial CPU qualification supports frozen Keep/Keep only')
        evaluation['model_sources'] = [p.model(model) for model in evaluation['model_sources']]
        evaluation['output_directory'] = str(p.remote / 'evaluation' / name)
        evaluations[name] = p.document(evaluation, 'config/evaluation-' + name + '.json')
    binaries = {}
    required = {'native_expanded_training_run_v1', 'learned_sideboard_v1'}
    require(required <= spec['binaries'].keys(), 'trainer and BO3 evaluator required')
    for name, record in spec['binaries'].items():
        require(re.fullmatch('[a-z0-9_]+', name), 'invalid binary name')
        with Path(record['path']).open('rb') as stream:
            require(stream.read(4) == b'\x7fELF', 'Linux ELF binary required')
        binaries[name] = p.put(record['path'], 'bin/' + name, record['sha256'], True)
    # The archive and receipt are supplied by the sole native-build owner.
    # Hashing them does not itself prove that the executable came from this commit.
    source = p.put(spec['source_archive']['path'], 'provenance/source.tar',
                   spec['source_archive']['sha256'])
    toolchain = p.put(spec['toolchain']['path'], 'provenance/toolchain.json',
                      spec['toolchain']['sha256'])
    runtime_image=None
    if spec.get('runtime_image_pins'):
        from runtime_observation import image_contract
        supplied=spec['runtime_image_pins'];pin(supplied['path'],supplied['sha256'])
        image_contract(read(supplied['path']))
        runtime_image=p.put(supplied['path'],'provenance/runtime-image-pins.json',supplied['sha256'])
    for name in ('common.py', 'cloud_worker.py', 'lease_guard.py', 'throughput.py', 'reference.py', 'semantic.py',
                 'workload.py', 'parity_evidence.py', 'runtime_compatibility.py', 'production_check.py',
                 'runtime_observation.py', 'fresh_source.py'):
        p.put(Path(__file__).with_name(name), 'tools/' + name)
    semantics = copy.deepcopy(original)
    semantics.pop('initial_source'); semantics.pop('opponents'); semantics.pop('output_directory')
    manifest = {'schema': 'phase1-cloud-payload/v1', 'source_commit': spec['source_commit'],
                'remote_root': str(p.remote), 'training_config': configuration,
                'evaluation_configs': evaluations,
                'binaries': binaries, 'source_archive': source, 'toolchain': toolchain,
                'runtime_image_pins':runtime_image,
                'original_training_config': config_path,
                'training_contract_sha256': training_contract(original),
                'model_source_relocations': p.relocations,
                'schedule_and_hyperparameters_sha256': hashlib.sha256(encoded(semantics)).hexdigest(),
                'files': p.files, 'input_pins': p.input_pins,
                'qualification_required': ['same-seed final full-state parity',
                    'same-seed BO3 full-match parity', 'full-state resume parity'],
                'strength_claim': False}
    if inspection is not None:
        manifest['initial_inspection'] = inspection
    p.document(manifest, 'manifest.json')
    # Deterministic uncompressed transport; no credentials, live leases, or outputs.
    archive = p.output / 'payload.tar'
    with tarfile.open(archive, 'w') as handle:
        for name in sorted(p.files):
            source_path = p.output / 'payload' / name
            info = handle.gettarinfo(str(source_path), arcname=name)
            info.mtime = 0; info.uid = info.gid = 0; info.uname = info.gname = ''
            info.mode = p.files[name]['mode']
            with source_path.open('rb') as stream:
                handle.addfile(info, stream)
    result = {'manifest': pin(p.output / 'payload/manifest.json'), 'archive': pin(archive),
              'file_count': len(p.files), 'engine_executed': False}
    write(p.output / 'package.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', required=True); parser.add_argument('--output', required=True)
    args = parser.parse_args()
    print(encoded(prepare(read(args.spec), args.output)).decode())


if __name__ == '__main__':
    main()
