"""Small synthetic file fixtures only. No real model, native binary or runtime certificate."""
import copy
import io
from pathlib import Path
import tarfile
import tempfile
import unittest

from common import encoded, pin, read, write
from pack_bo3 import FEATURE_CONSTANTS, FEATURE_FILES, RUNTIME_FIELDS, prepare


def fixture(root):
    def save(name, value):
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(value, bytes):
            path.write_bytes(value)
        else:
            write(path, value)
        # Match the Linux manifest's forward-slash spelling while keeping the
        # synthetic file locally readable by Windows Path during offline tests.
        return {**pin(path), 'path': path.as_posix()}
    commit, card_hash = 'a' * 40, '1234567890abcdef'
    files = {}
    files['feature_source'] = save('build/features_v6.py', b'# synthetic packaging fixture, never imported\n')
    fields = {'feature_schema_version': 'fixture-v6', 'feature_registry_version': 'fixture-registry',
        'feature_contract_digest': '1' * 64, 'feature_encoding_digest': '2' * 64,
        'features_source_sha256': files['feature_source']['sha256']}
    descriptor = {**fields, 'features_source_path': FEATURE_FILES['feature_source']}
    files['feature_descriptor'] = save('build/feature_contract_v3.json', descriptor)
    fields['feature_descriptor_sha256'] = files['feature_descriptor']['sha256']
    identity = ''.join('pub(crate) const %s: &str = "%s";\n' % (constant, fields[key])
        for key, constant in FEATURE_CONSTANTS.items())
    files['feature_identity'] = save('build/feature_identity.rs', identity.encode())
    files['source_registry'] = save('build/cards_v1.json', {'synthetic_test_only': True})
    files['toolchain'] = save('build/rust-toolchain.toml', b'# synthetic toolchain file\n')
    files['cargo_lock'] = save('build/Cargo.lock', b'# synthetic lock file\n')
    files['generated_card_defs'] = save('build/card_defs.rs', ('pub const KERNEL_CARDDB_HASH: u64 = 0x%s;\n' % card_hash).encode())
    files['binary'] = save('build/phase1_bo3_trainable_v1', b'\x7fELF-not-an-executable-test-fixture')
    compiled = {'MTG_KERNEL_BUILD_GIT_HEAD': commit, 'MTG_KERNEL_BUILD_GIT_CLEAN': 'true',
        'MTG_KERNEL_BUILD_TRACKED_TREE_SHA256': '3' * 64, 'MTG_KERNEL_BUILD_TRACKED_TREE_CONTRACT': 'synthetic-tree-contract'}
    artifact = {'reason': 'compiler-artifact', 'package_id': 'synthetic-unit-fixture',
        'target': {'name': 'phase1_bo3_trainable_v1', 'kind': ['bin']},
        'features': ['native-training-store-v2-production'],
        'profile': {'test': False, 'debug_assertions': False, 'opt_level': '3'},
        'executable': '/synthetic/release/phase1_bo3_trainable_v1'}
    files['compiler_artifact'] = save('build/compiler-artifact.json', artifact)
    files['build_script'] = save('build/build-script.json', {'reason': 'build-script-executed',
        'package_id': artifact['package_id'], 'env': list(map(list, compiled.items())), 'out_dir': '/synthetic/build/out'})
    files['build_receipt'] = save('build/build-receipt.json', {'build_exit_code': 0, 'error': None,
        'source_commit': commit, 'actual_artifact_path': artifact['executable']})
    files['build_script_output'] = save('build/build-output.txt', b'synthetic-build-output\n')
    files['toolchain_measurement'] = save('build/toolchain.json', {'synthetic_test_only': True})
    archive = root / 'build/source.tar'
    with tarfile.open(archive, 'w') as handle:
        for key, name in FEATURE_FILES.items():
            data = Path(files[key]['path']).read_bytes()
            info = tarfile.TarInfo(name); info.size = len(data)
            handle.addfile(info, io.BytesIO(data))
    files['source_archive'] = pin(archive)
    build = {'schema': 'phase1-bo3-trainable-linux-build/v1', 'profile': 'release', 'build_exit_code': 0,
        'native_execution': False, 'source_commit': commit, 'compiled_fields': compiled,
        'binaries': {'phase1_bo3_trainable_v1': files['binary']}, **files,
        'generated_card_defs_origin': {'path': '/synthetic/build/out/card_defs.rs',
            'sha256': files['generated_card_defs']['sha256'], 'bytes': files['generated_card_defs']['bytes']}}
    build_pin = save('build/manifest.json', build)
    source_registry = save('origin/registry.json', {'synthetic_original_registry': True})
    run = save('origin/run.json', {'synthetic_source_run': True})
    blob = save('origin/export/parameters.f32le', b'\x00\x00\x80\x3f')
    metadata = save('origin/export/metadata.json', {'parameter_section_sha256': blob['sha256'],
        'identity': {'loaded_run_sha256': run['sha256'], 'model_parameter_sha256': '4' * 64}})
    imported = {'export_directory': str(root / 'origin/export'), 'source_run_path': run['path'],
        'source_registry_path': source_registry['path'], 'expected_metadata_sha256': metadata['sha256'],
        'expected_model_parameter_sha256': '4' * 64, 'expected_source_registry_sha256': source_registry['sha256'],
        'source_registry_git_commit': '5' * 40, 'expected_destination_card_db_hash': card_hash}
    import_pin = save('origin/import.json', imported)
    source_identity = {'destination_registry_sha256': files['source_registry']['sha256'], 'synthetic_test_only': True}
    checkpoint = {'schema': 'mtg-kernel-expanded-deck-checkpoint/v1', 'loss_identity': 'terminal_reinforce_value/v3',
        'source_import': source_identity, 'state_sha256': '6' * 64, 'adam_step': 483, 'card_db_hash': card_hash,
        'feature_contract_digest': fields['feature_contract_digest'], 'feature_encoding_digest': fields['feature_encoding_digest'],
        'learning_rate_bits': 925353388, 'value_coefficient_bits': 1056964608,
        'scorer_bias_anchor_bits': 0, 'parameters': [{'name': 'fixture', 'shape': [2], 'values': [0, 1065353216]}],
        'first_moments': [{'name': 'fixture', 'shape': [2], 'values': [1, 2]}],
        'second_moments': [{'name': 'fixture', 'shape': [2], 'values': [3, 4]}]}
    checkpoint_pin = save('origin/checkpoint.json', checkpoint)
    behavior = {'source': {'play_import': import_pin, 'checkpoint': checkpoint_pin,
        'feature_transfer': {'expected_feature_contract_digest': fields['feature_contract_digest'],
            'expected_feature_encoding_digest': fields['feature_encoding_digest']}},
        'identity': {'schema': 'mtg-kernel-expanded-deck-inference/v1', 'checkpoint_sha256': checkpoint_pin['sha256'],
            'source_import': source_identity, 'state_sha256': checkpoint['state_sha256'], 'adam_step': 483,
            'model': {'card_db_hash': card_hash, 'feature_contract_digest': fields['feature_contract_digest'],
                'feature_encoding_digest': fields['feature_encoding_digest']},
            **{key: fields[key] for key in ('feature_schema_version', 'feature_registry_version', 'features_source_sha256', 'feature_descriptor_sha256')}}}
    package = {'schema': 'mtg-kernel-complete-agent-package/v1', 'runtime': dict.fromkeys(RUNTIME_FIELDS, 'old Windows declaration, not a certificate'),
        'gameplay': behavior, 'gameplay_sampler_identity': 'f32-q8-expq63-hamilton-splitmix64-wide-v1',
        'opening': {'kind': 'existing', 'protocol': 'keep_seven_v2'}, 'play_draw': {'kind': 'fixed', 'choice': 'play'},
        'sideboard': {'kind': 'keep'}, 'search': {'kind': 'disabled'}}
    game = {'schema': 'mtg-kernel-bo3-collection-config/v1', 'match_id': 'synthetic-file-test', 'seed': 42,
        'initial_chooser': 'p0', 'deck_ids': ['fixture-a', 'fixture-b'],
        'registrations': [{'mainboard': [0] * 60, 'sideboard': []}] * 2,
        'summary_tags': {'requires_target': [], 'is_counterspell': []}, 'max_physical_games': 6,
        'max_physical_decisions': 2, 'max_policy_steps': 3, 'max_decision_records': 100, 'max_decision_json_bytes': 1024}
    config = {'schema': 'mtg-kernel-native-bo3-training-run/v1', 'initial_learner': package,
        'initial_input': {'kind': 'ordinary_checkpoint_transition', 'learner': behavior}, 'previous_progress': None,
        'learning_rate_bits': checkpoint['learning_rate_bits'], 'value_coefficient_bits': checkpoint['value_coefficient_bits'],
        'opponents': [{'id': 'unused-fixed', 'package': copy.deepcopy(package)}],
        'batches': [{'matches': [{'config': game, 'learner_seat': 'p0', 'opponent': {'kind': 'current'},
            'capture_limits': {'max_payload_bytes': 1024, 'max_json_bytes': 1024}, 'max_result_bytes': 4096}]}],
        'preparation_limits': {'max_input_bytes': 8192, 'max_prepared_payload_bytes': 1024},
        'collection_workers': 1, 'output_directory': str(root / 'unused-native-output')}
    config_pin = save('run.json', config)
    return {'schema': 'phase1-bo3-cloud-package-spec/v1', 'run_request': config_pin,
        'linux_build': build_pin, 'remote_root': '/opt/phase1/synthetic-file-test'}


class Bo3PackagingTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.spec = fixture(self.root)

    def config(self, mutate):
        value = read(self.spec['run_request']['path']); mutate(value)
        write(self.spec['run_request']['path'], value, replace=True)
        self.spec['run_request'] = pin(self.spec['run_request']['path'])

    def test_exact_checkpoint_and_identity_preserved_with_actual_build_field_derivation(self):
        original = read(self.spec['run_request']['path'])
        checkpoint = Path(original['initial_learner']['gameplay']['source']['checkpoint']['path']).read_bytes()
        result = prepare(self.spec, self.root / 'package')
        manifest = read(result['manifest']['path'])
        relocated = read(self.root / 'package/payload/config/run.json')
        self.assertEqual(relocated['initial_learner']['gameplay']['identity'], original['initial_learner']['gameplay']['identity'])
        self.assertEqual(relocated['batches'], original['batches'])
        self.assertEqual(relocated['learning_rate_bits'], original['learning_rate_bits'])
        remote = relocated['initial_learner']['gameplay']['source']['checkpoint']['path']
        relative = remote.removeprefix(self.spec['remote_root'] + '/')
        self.assertEqual((self.root / 'package/payload' / relative).read_bytes(), checkpoint)
        self.assertEqual(manifest['runtime']['card_db_hash'], '1234567890abcdef')
        self.assertEqual(manifest['runtime']['engine_commit'], 'a' * 40)
        self.assertFalse(manifest['runtime_verified'])
        self.assertFalse(manifest['optimizer_restore_verified'])
        self.assertEqual(len(manifest['model_source_relocations']), 1)
        self.assertTrue((self.root / 'package/payload/provenance/helpers/semantic.py').is_file())

    def test_existing_bo3_input_and_prior_progress_reject(self):
        self.config(lambda value: value.update(previous_progress={'path': 'forbidden', 'sha256': '0' * 64}))
        with self.assertRaisesRegex(ValueError, 'without prior progress'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())
        self.config(lambda value: (value.update(previous_progress=None), value['initial_input'].update(kind='bo3_checkpoint')))
        with self.assertRaisesRegex(ValueError, 'BO3 checkpoint transport'):
            prepare(self.spec, self.root / 'package')

    def test_unused_fixed_roster_member_without_checkpoint_rejects_before_copy(self):
        self.config(lambda value: value['opponents'][0]['package']['gameplay']['source'].update(checkpoint=None))
        with self.assertRaisesRegex(ValueError, 'checkpoint required'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_recorded_scalar_mismatch_rejects(self):
        self.config(lambda value: value.update(learning_rate_bits=1065353216))
        with self.assertRaisesRegex(ValueError, 'optimizer scalars differ'):
            prepare(self.spec, self.root / 'package')

    def test_generated_origin_must_match_actual_build_script_route(self):
        path = self.spec['linux_build']['path']; build = read(path)
        build['generated_card_defs_origin']['path'] = '/unrelated/card_defs.rs'
        write(path, build, replace=True); self.spec['linux_build'] = pin(path)
        with self.assertRaisesRegex(ValueError, 'exact build-script origin'):
            prepare(self.spec, self.root / 'package')

    def test_multi_binary_build_selects_only_exact_bo3_artifact(self):
        path = self.spec['linux_build']['path']; build = read(path)
        build['binaries']['native_expanded_training_run_v1'] = {
            'path': '/historical/unused/native_expanded_training_run_v1', 'sha256': 'e' * 64}
        write(path, build, replace=True); self.spec['linux_build'] = pin(path)
        result = prepare(self.spec, self.root / 'package')
        manifest = read(result['manifest']['path'])
        self.assertEqual([name for name in manifest['files'] if name.startswith('bin/')],
                         ['bin/phase1_bo3_trainable_v1'])
        self.assertEqual(manifest['runtime']['executable']['sha256'], build['binary']['sha256'])

    def test_multi_binary_build_cannot_substitute_another_bo3_entry(self):
        path = self.spec['linux_build']['path']; build = read(path)
        build['binaries']['phase1_bo3_trainable_v1'] = {**build['binary'], 'sha256': 'e' * 64}
        write(path, build, replace=True); self.spec['linux_build'] = pin(path)
        with self.assertRaisesRegex(ValueError, 'exact Linux BO3 binary'):
            prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())

    def test_old_manifest_without_new_runtime_evidence_rejects(self):
        path = self.spec['linux_build']['path']; build = read(path)
        del build['generated_card_defs']
        write(path, build, replace=True); self.spec['linux_build'] = pin(path)
        with self.assertRaisesRegex(ValueError, 'old build manifests are unsupported'):
            prepare(self.spec, self.root / 'package')

    def test_foreign_feature_archive_bytes_reject_even_with_a_repinned_archive(self):
        build = read(self.spec['linux_build']['path']); path = Path(build['source_archive']['path'])
        with tarfile.open(path, 'w') as archive:
            for key, name in FEATURE_FILES.items():
                data = Path(build[key]['path']).read_bytes()
                if key == 'feature_source':
                    data = data.replace(b'never', b'XXXXX')
                member = tarfile.TarInfo(name); member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
        build['source_archive'] = pin(path)
        write(self.spec['linux_build']['path'], build, replace=True)
        self.spec['linux_build'] = pin(self.spec['linux_build']['path'])
        with self.assertRaisesRegex(ValueError, 'differs from clean archive'):
            prepare(self.spec, self.root / 'package')


if __name__ == '__main__':
    unittest.main(verbosity=2)
