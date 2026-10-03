"""Unit tests for the fixes that followed the adversarial review of the fresh
Adam0 initialization transport (2026-09): platform-label bounds on the producer
runtime, an independently verified `origins` binding in trajectory_digest,
fresh_source.fresh_origins, the generated-checkpoint origin check in
parity_evidence.training_run, native inspection evidence for reference.build
and pack.prepare's fresh Adam0 branch, and pack_bo3's trained-checkpoint
(Adam step above zero) requirement for every BO3 roster member.

All fixtures here are synthetic byte-transport fixtures, never a trained model
or a claim of playing strength, except the final class, which reads real
frozen Windows evidence read-only and skips cleanly when it is unavailable.
"""
import copy
from pathlib import Path
import tempfile
import unittest

from common import pin, read, write
from fresh_source import TRAJECTORY_SCHEMA, canonical, fresh_origins, model_seed, origin_key, sha
from semantic import trajectory_digest
from parity_evidence import Artifacts, training_run
from reference import build as build_reference
from throughput import canonical_training_config
from pack import prepare as pack_prepare
from pack_bo3 import prepare as bo3_prepare
from test_fresh_cloud import synthetic_transport_fixture
from test_fresh_bo3_cloud import reference as bo3_checkpoint_reference
from test_pack_bo3 import fixture as bo3_base_fixture
from test_production_checks import Fixture as ProductionFixture


def mutated_manifest_source(root, fixture, mutate, suffix):
    """Mutate a copy of the fixture's initialization manifest, re-canonicalize
    it and re-pin a fresh descriptor that points at the changed bytes."""
    descriptor = read(fixture['source']['play_import']['path'])
    manifest = read(descriptor['initialization']['path'])
    mutate(manifest)
    raw = canonical(manifest) + b'\n'
    init_path = root / f'mutated-initialization-{suffix}.json'
    init_path.write_bytes(raw)
    new_descriptor = copy.deepcopy(descriptor)
    new_descriptor['initialization'] = {'path': str(init_path), 'sha256': sha(raw)}
    desc_path = root / f'mutated-descriptor-{suffix}.json'
    write(desc_path, new_descriptor)
    item = pin(desc_path)
    return {'path': item['path'], 'sha256': item['sha256']}


class InspectSourceRuntimePlatformTests(unittest.TestCase):
    """fresh_source.inspect_source's producer runtime platform_system enumeration
    and the 256-character label bound on the four free runtime labels."""

    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.fixture = synthetic_transport_fixture(cls.root / 'fixture')

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def test_darwin_platform_system_rejected(self):
        from fresh_source import inspect_source
        f = self.fixture
        source_pin = mutated_manifest_source(self.root, f,
            lambda m: m['producer']['runtime'].__setitem__('platform_system', 'Darwin'), 'darwin')
        with self.assertRaisesRegex(ValueError, 'platform is outside the declared CPU contract'):
            inspect_source(source_pin, f['source']['feature_transfer'])

    def test_oversized_platform_machine_rejected(self):
        from fresh_source import inspect_source
        f = self.fixture
        source_pin = mutated_manifest_source(self.root, f,
            lambda m: m['producer']['runtime'].__setitem__('platform_machine', 'x' * 300), 'long-machine')
        with self.assertRaisesRegex(ValueError, 'producer runtime label invalid'):
            inspect_source(source_pin, f['source']['feature_transfer'])

    def test_windows_and_linux_platform_system_still_accepted(self):
        from fresh_source import inspect_source
        f = self.fixture
        for name, platform in (('windows-relabel', 'Windows'), ('linux-relabel', 'Linux')):
            with self.subTest(platform=platform):
                source_pin = mutated_manifest_source(self.root, f,
                    lambda m, platform=platform: m['producer']['runtime'].__setitem__('platform_system', platform), name)
                result = inspect_source(source_pin, f['source']['feature_transfer'])
                self.assertEqual(result['origin']['lineage_id'], f['origin']['lineage_id'])


def build_v3_trajectory(f, identity):
    return {'schema': TRAJECTORY_SCHEMA, 'source_import': identity['source_import'],
        'behavior_state_sha256': identity['state_sha256'],
        'episode': {'learner_seat': 0, 'opponent': f['source']},
        'seat_behaviors': [{'source': f['source'], 'identity': identity},
                            {'source': f['source'], 'identity': identity}]}


class TrajectoryDigestOriginsTests(unittest.TestCase):
    """semantic.trajectory_digest's `origins` parameter binds a fresh behavior's
    recorded ancestry to the origin independently derived from its pinned files,
    rather than accepting a self-consistent but forged claim."""

    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory()
        cls.root = Path(cls.temp.name)
        cls.fixture = synthetic_transport_fixture(cls.root / 'fixture')

    @classmethod
    def tearDownClass(cls):
        cls.temp.cleanup()

    def config(self):
        return {'initial_source': self.fixture['source'], 'opponents': []}

    def forged_identity(self):
        f = self.fixture
        forged_origin = copy.deepcopy(f['origin'])
        forged_origin['lineage_id'] = 'forged-lineage-id'
        forged_origin['base_seed'] = f['origin']['base_seed'] + 1
        forged_origin['model_init_seed'] = model_seed(forged_origin['base_seed'])
        identity = copy.deepcopy(f['identity'])
        identity['source_import'] = forged_origin
        return identity

    def test_forged_self_consistent_origin_rejected_against_true_origins_map(self):
        f = self.fixture
        trajectory = build_v3_trajectory(f, self.forged_identity())
        origins = {origin_key(f['source']): f['origin']}
        with self.assertRaisesRegex(ValueError, 'inference initialization origin differs'):
            trajectory_digest(trajectory, self.config(), origins=origins)

    def test_true_origin_digests_successfully(self):
        f = self.fixture
        trajectory = build_v3_trajectory(f, f['identity'])
        origins = {origin_key(f['source']): f['origin']}
        digest = trajectory_digest(trajectory, self.config(), origins=origins)
        self.assertRegex(digest, r'^[0-9a-f]{64}$')

    def test_fresh_source_missing_from_origins_map_rejected(self):
        f = self.fixture
        trajectory = build_v3_trajectory(f, f['identity'])
        with self.assertRaisesRegex(ValueError, 'unproven fresh source origin'):
            trajectory_digest(trajectory, self.config(), origins={})

    def test_origins_none_keeps_old_behavior_and_does_not_catch_forgery(self):
        f = self.fixture
        trajectory = build_v3_trajectory(f, self.forged_identity())
        digest = trajectory_digest(trajectory, self.config(), origins=None)
        self.assertRegex(digest, r'^[0-9a-f]{64}$')


class FreshOriginsMapTests(unittest.TestCase):
    """fresh_source.fresh_origins(config) maps only the configured fresh sources,
    keyed by origin_key, and omits imported (non-fresh) sources entirely."""

    def test_fresh_origins_maps_only_fresh_sources(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            f = synthetic_transport_fixture(root / 'fixture')
            imported_path = root / 'imported-source.json'
            write(imported_path, {'synthetic_test_only': True})
            imported_source = {'play_import': pin(imported_path)}
            config = {'initial_source': f['source'], 'opponents': [{'id': 'imported', 'source': imported_source}]}
            origins = fresh_origins(config)
            self.assertEqual(set(origins), {origin_key(f['source'])})
            self.assertEqual(origins[origin_key(f['source'])], f['origin'])


class TrainingRunGeneratedCheckpointOriginTests(unittest.TestCase):
    """parity_evidence.training_run binds every generated (iteration) checkpoint's
    recorded ancestry to the qualified initial source, not to its own claim."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fixture = ProductionFixture(self.temporary.name)

    def forge_last_iteration_checkpoint_origin(self, spec, forged_origin):
        """Cascade a generated-checkpoint field change up through every pinned
        ancestor receipt, mirroring Fixture.alter_last_trajectory."""
        completion = read(spec['completion']['path'])
        complete_pin = completion['iterations'][-1]
        receipt = read(complete_pin['path'])
        update = read(receipt['update']['path'])
        checkpoint = read(update['checkpoint']['path'])
        checkpoint['source_import'] = forged_origin
        write(update['checkpoint']['path'], checkpoint, replace=True)
        update['checkpoint'] = pin(update['checkpoint']['path'])
        write(receipt['update']['path'], update, replace=True)
        receipt['update'] = pin(receipt['update']['path'])
        write(complete_pin['path'], receipt, replace=True)
        completion['iterations'][-1] = pin(complete_pin['path'])
        completion['source']['checkpoint'] = update['checkpoint']
        write(spec['completion']['path'], completion, replace=True)
        spec['completion'] = pin(spec['completion']['path'])

    def test_forged_generated_checkpoint_origin_rejects(self):
        f = self.fixture
        spec = copy.deepcopy(f.local['spec'])
        training_run(Artifacts(), spec)  # baseline: the unmodified fixture is accepted
        forged_origin = {'identity': 'synthetic', 'forged_lineage_id': 'alternate-checkpoint-lineage'}
        self.forge_last_iteration_checkpoint_origin(spec, forged_origin)
        with self.assertRaisesRegex(ValueError,
                'generated checkpoint origin differs from the qualified initial source'):
            training_run(Artifacts(), spec)


class ReferenceBuildFreshAdamZeroTests(unittest.TestCase):
    """reference.build demands real native inspection evidence for a fresh
    (checkpoint-less) initial source and records it as 'initial_inspection'
    when supplied. Uses the smallest native run tree reference.build accepts,
    adapted from test_cloud.NativeReferenceTests.fixture for a fresh source."""

    def fixture(self, root):
        f = synthetic_transport_fixture(root / 'fresh')
        binary = root / 'trainer'
        binary.write_bytes(b'fixture, never executable')
        cfg = {'schema': 'mtg-kernel-native-expanded-training-run/v1',
            'initial_source': f['source'], 'opponents': [],
            'iterations': [{'episodes': [{'seed': 10}]}],
            'learning_rate': .00001, 'value_coefficient': .5,
            'output_directory': str(root / 'native')}
        write(root / 'config.json', cfg)
        write(root / 'native/run.json', {'config': canonical_training_config(cfg)})
        base = root / 'native/iterations/000000/attempt-000000'
        write(base / 'collect/episode-0000.json', {
            'schema': 'mtg-kernel-expanded-deck-trajectory/v2',
            'episode': {'seed': 10, 'opponent': f['source']},
            'seat_behaviors': [{'source': f['source']}] * 2,
            'decisions': [], 'fixture_only': True})
        write(base / 'collect/collection.json', {'complete': True,
            'trajectories': [pin(base / 'collect/episode-0000.json')],
            'collection_elapsed_seconds': 1.})
        write(base / 'update/checkpoint.json', {'state_sha256': 'c' * 64, 'adam_step': 1})
        write(base / 'update/update.json', {'complete': True,
            'before_state_sha256': f['state']['state_sha256'],
            'after_state_sha256': 'c' * 64, 'checkpoint': pin(base / 'update/checkpoint.json'),
            'update_elapsed_seconds': .5})
        write(base.parent / 'complete.json', {'iteration': 0,
            'collection': pin(base / 'collect/collection.json'),
            'update': pin(base / 'update/update.json'),
            'output_identity': {'state_sha256': 'c' * 64}})
        write(root / 'native/completion.json', {'complete': True, 'completed_iterations': 1,
            'iterations': [pin(base.parent / 'complete.json')],
            'actual_identity': {'state_sha256': 'c' * 64}})
        write(root / 'result.json', {'schema': 'phase1-auxiliary-cpu-result/v1',
            'exit_code': 0, 'reason': 'engine_exit', 'config': pin(root / 'config.json'),
            'binary': pin(binary), 'native_active_seconds': 2.,
            'completed_iterations_before': 0, 'new_iterations': 1})
        write(root / 'build.json', {'source_commit': 'd' * 40, 'binary': pin(binary)})
        args = (pin(root / 'config.json'), pin(root / 'result.json'), pin(root / 'build.json'))
        return args, f

    def test_missing_inspection_raises_clean_value_error(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args, _f = self.fixture(root)
            with self.assertRaisesRegex(ValueError, 'native inspection evidence'):
                build_reference(*args)

    def test_supplied_inspection_succeeds_and_is_recorded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            args, f = self.fixture(root)
            result = build_reference(*args, inspection_pin=f['inspection'])
            self.assertEqual(result['initial_state_sha256'], f['state']['state_sha256'])
            self.assertEqual(result['initial_inspection'], f['inspection'])
            self.assertEqual(result['completed_games'], 1)
            self.assertTrue(result['complete'])


class PackPrepareFreshInitialInspectionTests(unittest.TestCase):
    """pack.prepare's fresh-Adam0 branch: spec['initial_inspection'] is required,
    and when supplied it is relocated to provenance/initial-inspection.json and
    recorded verbatim in manifest['initial_inspection']."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.fresh = synthetic_transport_fixture(self.root / 'fresh')

    def spec(self, inspection=None):
        root = self.root
        binary = root / 'fake-elf'
        binary.write_bytes(b'\x7fELFfake binary, never executed')
        source_archive = root / 'source.tar'
        source_archive.write_bytes(b'fake source archive')
        toolchain_path = root / 'toolchain.json'
        write(toolchain_path, {'source_commit': 'a' * 40, 'rustc': '1.94.1'})
        config = {'schema': 'mtg-kernel-native-expanded-training-run/v1',
            'initial_source': self.fresh['source'], 'opponents': [],
            'iterations': [], 'learning_rate': .00001, 'value_coefficient': .5,
            'output_directory': 'old'}
        config_path = root / 'training.json'
        write(config_path, config)
        value = {'schema': 'phase1-cloud-package-spec/v1', 'source_commit': 'a' * 40,
            'remote_root': '/opt/phase1/fresh-pack-test', 'training_config': pin(config_path),
            'binaries': {name: pin(binary) for name in
                ('native_expanded_training_run_v1', 'learned_sideboard_v1')},
            'source_archive': pin(source_archive), 'toolchain': pin(toolchain_path)}
        if inspection is not None:
            value['initial_inspection'] = inspection
        return value

    def test_missing_inspection_rejects(self):
        with self.assertRaisesRegex(ValueError, 'requires native inspection evidence'):
            pack_prepare(self.spec(), self.root / 'package')

    def test_supplied_inspection_is_relocated_and_recorded(self):
        f = self.fresh
        original_descriptor = read(f['source']['play_import']['path'])
        result = pack_prepare(self.spec(f['inspection']), self.root / 'package')
        manifest = read(result['manifest']['path'])
        relocated_inspection_path = self.root / 'package/payload/provenance/initial-inspection.json'
        self.assertTrue(relocated_inspection_path.is_file())
        self.assertEqual(manifest['initial_inspection'],
            {'path': '/opt/phase1/fresh-pack-test/provenance/initial-inspection.json',
             'sha256': f['inspection']['sha256']})
        self.assertEqual(pin(relocated_inspection_path)['sha256'], f['inspection']['sha256'])
        relocated_config = read(self.root / 'package/payload/config/training.json')
        self.assertIsNone(relocated_config['initial_source']['checkpoint'])
        remote = relocated_config['initial_source']['play_import']['path']
        relative = remote.removeprefix('/opt/phase1/fresh-pack-test/')
        relocated_descriptor = read(self.root / 'package/payload' / relative)
        self.assertEqual(relocated_descriptor['initialization']['sha256'],
            original_descriptor['initialization']['sha256'])
        self.assertEqual(relocated_descriptor['parameters']['sha256'],
            original_descriptor['parameters']['sha256'])


class PackBo3FreshRosterAdamStepTests(unittest.TestCase):
    """pack_bo3.prepare rejects a fresh roster member whose checkpoint is still
    the Adam0 initialization state (never trained), even though every byte and
    hash in it is otherwise self-consistent and correctly pinned."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.spec = bo3_base_fixture(self.root)
        build = read(self.spec['linux_build']['path'])
        descriptor = read(build['feature_descriptor']['path'])
        target = {
            'registry': {'path': 'C:/historical-generator/cards_v1.json', 'sha256': build['source_registry']['sha256']},
            'card_db_hash': '1234567890abcdef',
            'feature_contract_digest': descriptor['feature_contract_digest'],
            'feature_encoding_digest': descriptor['feature_encoding_digest'],
            'features_source_sha256': build['feature_source']['sha256'],
            'feature_descriptor_sha256': build['feature_descriptor']['sha256'],
            'registry_card_count': 184, 'card_token_rule': 'card-token=id+1; padding=0'}
        self.fresh = synthetic_transport_fixture(self.root / 'fresh', target=target)
        self.config = read(self.spec['run_request']['path'])
        # The fixture's own state is already the untrained Adam0 proof: adam_step
        # 0 and all-zero moments, with a correctly pinned state_sha256.
        origin, state = self.fresh['origin'], self.fresh['state']
        self.checkpoint = {
            'schema': 'mtg-kernel-expanded-deck-fresh-checkpoint/v1',
            'source_import': origin, 'card_db_hash': origin['destination_card_db_hash'],
            'feature_contract_digest': origin['feature_contract_digest'],
            'feature_encoding_digest': origin['feature_encoding_digest'],
            **{name: state[name] for name in ('state_sha256', 'adam_step', 'scorer_bias_anchor_bits',
                                            'parameters', 'first_moments', 'second_moments')},
            'loss_identity': 'terminal_reinforce_value/v3', 'trajectories': [],
            'learning_rate_bits': self.config['learning_rate_bits'],
            'value_coefficient_bits': self.config['value_coefficient_bits']}
        self.checkpoint_path = self.root / 'fresh/checkpoint.json'
        write(self.checkpoint_path, self.checkpoint)
        source = copy.deepcopy(self.fresh['source'])
        source['checkpoint'] = bo3_checkpoint_reference(self.checkpoint_path)
        identity = copy.deepcopy(self.fresh['identity'])
        identity['checkpoint_sha256'] = source['checkpoint']['sha256']
        identity['feature_schema_version'] = descriptor['feature_schema_version']
        identity['feature_registry_version'] = descriptor['feature_registry_version']
        fresh_package = copy.deepcopy(self.config['initial_learner'])
        fresh_package['gameplay'] = {'source': source, 'identity': identity}
        # Replace the unused fixed roster member with this untrained fresh member;
        # every roster member is validated whether or not a schedule uses it.
        self.config['opponents'][0]['package'] = fresh_package
        write(self.spec['run_request']['path'], self.config, replace=True)
        self.spec['run_request'] = pin(self.spec['run_request']['path'])

    def test_fresh_adam_zero_roster_member_rejects_as_untrained(self):
        with self.assertRaisesRegex(ValueError, 'trained ordinary checkpoint'):
            bo3_prepare(self.spec, self.root / 'package')
        self.assertFalse((self.root / 'package').exists())


REAL_ROOT = Path('E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001')
FRESH_ROOT = REAL_ROOT / 'fresh-initialization-checks-001'


def _locate_real_fresh_source_build():
    """Look for a build manifest in parity_evidence.source_build's required
    shape (source_commit, binaries{native_expanded_training_run_v1,
    learned_sideboard_v1}, source_archive, toolchain) at the frozen run's
    producer commit. Returns (pin_or_None, reason_string_if_missing)."""
    run_path = FRESH_ROOT / 'a-serial1-run' / 'run.json'
    if not run_path.is_file():
        return None, 'a-serial1-run/run.json is unavailable'
    commit = read(run_path)['git_commit']
    for candidate in (REAL_ROOT / 'build-cpu-001' / 'manifest.json',
                      FRESH_ROOT / 'release-build-001' / 'manifest.json'):
        if not candidate.is_file():
            continue
        value = read(candidate)
        binaries = value.get('binaries', {})
        if (value.get('source_commit') == commit
                and {'native_expanded_training_run_v1', 'learned_sideboard_v1'} <= set(binaries)
                and 'source_archive' in value and 'toolchain' in value):
            return pin(candidate), None
    return None, (
        'no build manifest under build-cpu-001 or fresh-initialization-checks-001/release-build-001 '
        'pins binaries+source_archive+toolchain at the run producer commit ' + commit + '; '
        'build-cpu-001/manifest.json has the right shape but a different source_commit, and '
        'release-build-001 has only binary-pins.json (a list, not a binaries dict) and toolchain.json '
        '(not a path+sha256 pin), with no source_archive pin at all')


SOURCE_BUILD_PIN = None
SOURCE_BUILD_MISSING_REASON = None
if FRESH_ROOT.exists():
    SOURCE_BUILD_PIN, SOURCE_BUILD_MISSING_REASON = _locate_real_fresh_source_build()


@unittest.skipUnless(FRESH_ROOT.exists(), 'frozen fresh-initialization evidence is unavailable on this machine')
class RealFreshTrainingRunEvidenceTests(unittest.TestCase):
    """Reads real frozen Windows fresh-Adam0 training evidence only, read-only,
    under E:/mtg-kernel-learned-sideboarding-evidence. This is an engineering
    equality regression against parity_evidence.training_run; it makes no
    playing-strength claim. Skips cleanly, naming the missing field, if no
    build manifest in the required shape can be found for the run's commit."""

    def test_training_run_matches_frozen_fresh_adam_zero_evidence(self):
        if SOURCE_BUILD_PIN is None:
            self.skipTest('no qualifying source_build manifest: ' + SOURCE_BUILD_MISSING_REASON)
        spec = {
            'config': pin(FRESH_ROOT / 'training-inputs-001/a-serial1.json'),
            'source_build': SOURCE_BUILD_PIN,
            'run_start': pin(FRESH_ROOT / 'a-serial1-run/run.json'),
            'completion': pin(FRESH_ROOT / 'a-serial1-run/completion.json'),
            'initial_inspection': pin(FRESH_ROOT / 'inspection-a/inspection.json')}
        result = training_run(Artifacts(), spec)
        self.assertEqual(result['completed_updates'], 2)
        self.assertEqual(result['completed_fresh_games'], 20)
        comparison_path = FRESH_ROOT / 'training-comparison-001.json'
        expected_final_state = None
        if comparison_path.is_file():
            comparison = read(comparison_path)
            row = next((r for r in comparison.get('rows', []) if r.get('name') == 'a-serial1'), None)
            if row and row.get('checkpoints'):
                expected_final_state = row['checkpoints'][-1].get('state_sha256')
        if expected_final_state:
            self.assertEqual(result['final_state_sha256'], expected_final_state)
        else:
            self.assertRegex(result['final_state_sha256'], r'^[0-9a-f]{64}$')


if __name__ == '__main__':
    unittest.main(verbosity=2)
