"""Offline coverage for lifecycle_timing.py. No provider calls, no native
processes, no real credentials. Every receipt below is a small synthetic
fixture, not a retained artifact.
"""
from datetime import datetime, timezone
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

import lifecycle_timing as lt
from common import read, write
from throughput import TIMING_PHASES, complete_work_seconds, qualify


def iso(epoch):
    return datetime.fromtimestamp(epoch, tz=timezone.utc).isoformat()


def reference_fixture(root, elapsed_seconds, name='reference.json'):
    path = Path(root) / name
    write(path, {'schema': 'phase1-fixed-work-training/v1', 'complete': True,
                 'elapsed_seconds': elapsed_seconds})
    return path


def cloud_receipts(root):
    root = Path(root)
    write(root / 'funding-snapshot.json', {'observed_utc': iso(1000.)})
    write(root / 'created-volume.json', {'created_epoch': 1010.})
    write(root / 'created-pod.json', {'created_epoch': 1020.})
    write(root / 'guard.json', {'epoch': 1030.})
    write(root / 'worker-start.json', {'epoch': 1050.})
    write(root / 'worker-result.json', {'native_execution_elapsed_seconds': 60.})
    write(root / 'recovery.json', {'epoch': 1130.})
    write(root / 'released.json', {'epoch': 1140., 'provider_absent': True})
    return {'funding_snapshot': str(root / 'funding-snapshot.json'),
            'volume_create': str(root / 'created-volume.json'),
            'pod_create': str(root / 'created-pod.json'),
            'guard_armed': str(root / 'guard.json'),
            'staging_complete': iso(1040.),
            'run_start': str(root / 'worker-start.json'),
            'export_complete': str(root / 'recovery.json'),
            'release_confirmed': str(root / 'released.json')}


def local_receipts(root):
    root = Path(root)
    write(root / 'worker-start.json', {'epoch': 2020.})
    write(root / 'worker-result.json', {'native_execution_elapsed_seconds': 50.})
    write(root / 'recovery.json', {'epoch': 2090.})
    return {'input_preparation_start': iso(2000.),
            'staging_complete': iso(2010.),
            'run_start': str(root / 'worker-start.json'),
            'export_complete': str(root / 'recovery.json')}


LOCAL_NOT_APPLICABLE = {'allocation': 'existing local machine; no Pod is allocated for the on-premises reference run',
                        'image_startup': 'no container image is pulled for a local process',
                        'release_confirmation': 'no provider Pod exists to release'}


class CloudEnvelopeTests(unittest.TestCase):
    def test_synthetic_complete_cloud_envelope_validates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            worker_result = root / 'worker-result.json'
            envelope, reference_pin, reference_document = lt.assemble_envelope(
                'cloud', reference_path, 'test-session-1', values, {}, root / 'receipts',
                worker_result_path=worker_result)
            self.assertEqual(set(envelope['phases']), set(TIMING_PHASES))
            self.assertEqual(envelope['start_seconds'], 1000.)
            self.assertEqual(envelope['end_seconds'], 1140.)
            envelope_pin = lt.write_envelope(envelope, root / 'cloud-timing.json')
            duration = lt.validate(envelope_pin, reference_pin, reference_document, 'cloud')
            self.assertEqual(duration, 140.)
            # release_confirmation evidence must carry the confirmed-absence receipt.
            evidence = [read(item['path']) for item in envelope['phases']['release_confirmation']['evidence']]
            self.assertTrue(any(item.get('provider_absent') is True for item in evidence))

    def test_run_end_computed_from_worker_result_matches_manual_computation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            envelope, _, _ = lt.assemble_envelope('cloud', reference_path, 'test-session-1', values, {},
                                                   root / 'receipts', worker_result_path=root / 'worker-result.json')
            start, end = envelope['phases']['native_invocation']['start_seconds'], envelope['phases']['native_invocation']['end_seconds']
            self.assertEqual(start, 1050.)
            self.assertEqual(end, 1050. + 60.)

    def test_release_confirmed_must_show_provider_absent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            write(root / 'released.json', {'epoch': 1140., 'provider_absent': False}, replace=True)
            with self.assertRaisesRegex(ValueError, 'provider_absent: true'):
                lt.assemble_envelope('cloud', reference_path, 'test-session-1', values, {}, root / 'receipts',
                                     worker_result_path=root / 'worker-result.json')

    def test_release_confirmed_must_be_a_receipt_file_not_a_bare_timestamp(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            values['release_confirmed'] = iso(1140.)
            with self.assertRaisesRegex(ValueError, 'must be a JSON receipt file'):
                lt.assemble_envelope('cloud', reference_path, 'test-session-1', values, {}, root / 'receipts',
                                     worker_result_path=root / 'worker-result.json')

    def test_not_applicable_phases_are_rejected_for_cloud(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            with self.assertRaisesRegex(ValueError, 'not-applicable phases only apply to environment local'):
                lt.assemble_envelope('cloud', reference_path, 'test-session-1', values,
                                     {'allocation': 'not real'}, root / 'receipts',
                                     worker_result_path=root / 'worker-result.json')


class LocalEnvelopeTests(unittest.TestCase):
    def test_synthetic_complete_local_envelope_validates(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root)
            envelope, reference_pin, reference_document = lt.assemble_envelope(
                'local', reference_path, 'test-session-2', values, LOCAL_NOT_APPLICABLE, root / 'receipts',
                worker_result_path=root / 'worker-result.json')
            self.assertEqual(set(envelope['phases']), set(TIMING_PHASES))
            for name in ('allocation', 'image_startup', 'release_confirmation'):
                self.assertTrue(envelope['phases'][name]['not_applicable'])
                self.assertNotIn('start_seconds', envelope['phases'][name])
                self.assertNotIn('end_seconds', envelope['phases'][name])
            envelope_pin = lt.write_envelope(envelope, root / 'local-timing.json')
            duration = lt.validate(envelope_pin, reference_pin, reference_document, 'local')
            self.assertEqual(duration, 90.)

    def test_cloud_only_markers_are_rejected_for_local(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root) | {'guard_armed': iso(2005.)}
            with self.assertRaisesRegex(ValueError, 'only applies to environment cloud'):
                lt.assemble_envelope('local', reference_path, 'test-session-2', values, LOCAL_NOT_APPLICABLE,
                                     root / 'receipts', worker_result_path=root / 'worker-result.json')


class MissingStageTests(unittest.TestCase):
    def test_a_missing_not_applicable_stage_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root)
            incomplete = {k: v for k, v in LOCAL_NOT_APPLICABLE.items() if k != 'image_startup'}
            envelope, reference_pin, reference_document = lt.assemble_envelope(
                'local', reference_path, 'test-session-2', values, incomplete, root / 'receipts',
                worker_result_path=root / 'worker-result.json')
            self.assertNotIn('image_startup', envelope['phases'])
            envelope_pin = lt.write_envelope(envelope, root / 'local-timing.json')
            with self.assertRaisesRegex(ValueError, 'missing lifecycle phase'):
                lt.validate(envelope_pin, reference_pin, reference_document, 'local')

    def test_a_missing_measured_stage_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            envelope, reference_pin, reference_document = lt.assemble_envelope(
                'cloud', reference_path, 'test-session-1', values, {}, root / 'receipts',
                worker_result_path=root / 'worker-result.json')
            del envelope['phases']['checkpoint_export']
            envelope_pin = lt.write_envelope(envelope, root / 'cloud-timing.json')
            with self.assertRaisesRegex(ValueError, 'missing lifecycle phase'):
                lt.validate(envelope_pin, reference_pin, reference_document, 'cloud')


class NonMonotonicTests(unittest.TestCase):
    def test_non_monotonic_cloud_markers_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=55.)
            values = cloud_receipts(root)
            # Pod created before the volume it depends on: not a real ordering.
            write(root / 'created-pod.json', {'created_epoch': 1005.}, replace=True)
            with self.assertRaisesRegex(ValueError, 'not monotonic'):
                lt.assemble_envelope('cloud', reference_path, 'test-session-1', values, {}, root / 'receipts',
                                     worker_result_path=root / 'worker-result.json')

    def test_non_monotonic_local_markers_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root)
            values['run_start'] = str(root / 'worker-start.json')
            # Export completing before the native run even starts.
            write(root / 'recovery.json', {'epoch': 1900.}, replace=True)
            with self.assertRaisesRegex(ValueError, 'not monotonic'):
                lt.assemble_envelope('local', reference_path, 'test-session-2', values, LOCAL_NOT_APPLICABLE,
                                     root / 'receipts', worker_result_path=root / 'worker-result.json')

    def test_run_end_before_run_start_is_rejected_even_within_one_phase(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=1.)
            values = cloud_receipts(root)
            values['run_end'] = iso(1049.)  # before run_start (1050.)
            with self.assertRaisesRegex(ValueError, 'not monotonic'):
                lt.assemble_envelope('cloud', reference_path, 'test-session-1', values, {}, root / 'receipts')


class ReceiptEpochTests(unittest.TestCase):
    def test_unrecognized_receipt_shape_is_rejected(self):
        with self.assertRaisesRegex(ValueError, 'no recognized timestamp field'):
            lt.receipt_epoch({'nothing_useful': True}, 'staging_complete')

    def test_naive_iso_timestamp_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, 'timezone-aware'):
                lt.resolve_marker('2026-09-15T12:00:00', 'staging_complete', temporary)


class QualifyIntegrationTests(unittest.TestCase):
    def test_lifecycle_timing_envelopes_feed_qualify_end_to_end(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            local_reference = Path(root / 'local') ; local_reference.mkdir()
            cloud_reference = Path(root / 'cloud'); cloud_reference.mkdir()
            common = {'schema': 'phase1-fixed-work-training/v1', 'complete': True, 'source_commit': 'a' * 40,
                'training_contract_sha256': 'b' * 64, 'fixed_work_sha256': 'c' * 64,
                'initial_state_sha256': 'd' * 64, 'final_state_sha256': 'e' * 64,
                'iteration_state_sha256s': ['f' * 64, 'e' * 64], 'first_batch_episode_sha256s': ['1' * 64],
                'trajectory_semantics': 'expanded-trajectory-v2-v3-exact-model-source-relocation/v2',
                'first_batch_semantic_sha256s': ['2' * 64], 'completed_games': 20, 'completed_iterations': 2,
                'max_iteration_elapsed_seconds': 60}
            local_path = root / 'local.json'; write(local_path, common | {'elapsed_seconds': 90., 'collection_workers': 4})
            cloud_path = root / 'cloud.json'; write(cloud_path, common | {'elapsed_seconds': 60., 'collection_workers': 10})
            from common import pin
            local_pin = pin(local_path); cloud_pin = pin(cloud_path)

            local_values = local_receipts(local_reference)
            # local_receipts()'s default 50-second native run is too short for
            # this fixture's own local.json elapsed_seconds (90.); replace the
            # worker result and export receipt with ones that fit.
            write(local_reference / 'worker-result.json', {'native_execution_elapsed_seconds': 95.}, replace=True)
            write(local_reference / 'recovery.json', {'epoch': 2200.}, replace=True)
            local_envelope, _, _ = lt.assemble_envelope('local', local_path, 'session-local', local_values,
                LOCAL_NOT_APPLICABLE, local_reference / 'receipts', worker_result_path=local_reference / 'worker-result.json')
            local_timing_pin = lt.write_envelope(local_envelope, local_reference / 'local-timing.json')

            cloud_values = cloud_receipts(cloud_reference)
            cloud_envelope, _, _ = lt.assemble_envelope('cloud', cloud_path, 'session-cloud', cloud_values, {},
                cloud_reference / 'receipts', worker_result_path=cloud_reference / 'worker-result.json')
            cloud_timing_pin = lt.write_envelope(cloud_envelope, cloud_reference / 'cloud-timing.json')

            proof = qualify(local_pin, cloud_pin, local_timing_pin, cloud_timing_pin)
            self.assertEqual(proof['timing_status'], 'complete_work_measured')
            self.assertAlmostEqual(proof['local_complete_work_seconds'], 200.)
            self.assertAlmostEqual(proof['cloud_complete_work_seconds'], 140.)
            self.assertAlmostEqual(proof['speedup'], 200. / 140.)


class CommandLineTests(unittest.TestCase):
    def test_dry_run_prints_and_validates_without_writing_output_or_receipts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root)
            before = sorted(p.name for p in root.iterdir())
            argv = ['lifecycle_timing.py', '--environment', 'local', '--reference', str(reference_path),
                    '--clock-session', 'cli-dry-run', '--staging-complete', values['staging_complete'],
                    '--input-preparation-start', values['input_preparation_start'],
                    '--run-start', values['run_start'], '--worker-result', str(root / 'worker-result.json'),
                    '--export-complete', values['export_complete'],
                    '--allocation-not-applicable', LOCAL_NOT_APPLICABLE['allocation'],
                    '--image-startup-not-applicable', LOCAL_NOT_APPLICABLE['image_startup'],
                    '--release-confirmation-not-applicable', LOCAL_NOT_APPLICABLE['release_confirmation'],
                    '--dry-run']
            out = io.StringIO()
            with patch.object(sys, 'argv', argv), patch('sys.stdout', out):
                lt.main()
            printed = out.getvalue()
            self.assertIn('"phase1-complete-work-timing/v1"', printed)
            after = sorted(p.name for p in root.iterdir())
            self.assertEqual(before, after)  # nothing new was written next to the receipts

    def test_main_writes_a_validated_envelope_to_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            reference_path = reference_fixture(root, elapsed_seconds=45.)
            values = local_receipts(root)
            output = root / 'local-timing.json'
            argv = ['lifecycle_timing.py', '--environment', 'local', '--reference', str(reference_path),
                    '--clock-session', 'cli-write', '--staging-complete', values['staging_complete'],
                    '--input-preparation-start', values['input_preparation_start'],
                    '--run-start', values['run_start'], '--worker-result', str(root / 'worker-result.json'),
                    '--export-complete', values['export_complete'],
                    '--allocation-not-applicable', LOCAL_NOT_APPLICABLE['allocation'],
                    '--image-startup-not-applicable', LOCAL_NOT_APPLICABLE['image_startup'],
                    '--release-confirmation-not-applicable', LOCAL_NOT_APPLICABLE['release_confirmation'],
                    '--output', str(output)]
            with patch.object(sys, 'argv', argv):
                lt.main()
            self.assertTrue(output.exists())
            saved = read(output)
            self.assertEqual(saved['schema'], lt.TIMING_SCHEMA)
            self.assertTrue((root / 'local-timing.receipts').is_dir())


if __name__ == '__main__':
    unittest.main(verbosity=2)
