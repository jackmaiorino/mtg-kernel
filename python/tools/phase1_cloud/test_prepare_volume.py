"""Offline/mock coverage for prepare_volume.py. No provider calls, no real credentials.

Mirrors test_cloud.py / test_lease_failures.py: a Mock or patched urlopen stands
in for the live client, and --execute is exercised only through that mock.
"""
from datetime import datetime, timezone
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

import prepare_lease
import prepare_volume as pv
from common import read, write


NOW = 1_800_000_000.0


def snapshot_path(root, age_seconds=0.0, now=NOW, name='funding-snapshot-test.json'):
    observed = datetime.fromtimestamp(now - age_seconds, tz=timezone.utc).isoformat()
    path = Path(root) / name
    write(path, {'observed_utc': observed, 'account': {'data': {'myself': {
        'clientBalance': 31.664830402, 'currentSpendPerHr': 0.035,
        'isAutoPayEnabled': False, 'pods': []}}}})
    return path


def spec(**changes):
    return {'schema': pv.SPEC_SCHEMA, 'name': 'phase1-volume-qualification-test'} | changes


def volume(**changes):
    return {'id': 'agv6w2qcg7', 'name': 'phase1-volume-qualification-test',
            'size': 2, 'dataCenterId': 'EU-RO-1'} | changes


class TemplateTests(unittest.TestCase):
    def test_template_targets_documented_endpoint_and_shape(self):
        template = pv.build_template('phase1-volume-test', 5)
        self.assertEqual(template['method'], 'POST')
        self.assertEqual(template['url'], 'https://rest.runpod.io/v1/networkvolumes')
        self.assertEqual(template['body'], {'name': 'phase1-volume-test', 'size': 5,
                                             'dataCenterId': 'EU-RO-1'})

    def test_template_targets_same_data_center_as_prepare_lease(self):
        template = pv.build_template('phase1-volume-test', 5)
        self.assertEqual(template['body']['dataCenterId'], pv.DATA_CENTER_ID)
        self.assertEqual(pv.DATA_CENTER_ID, 'EU-RO-1')
        with self.assertRaisesRegex(ValueError, 'same data center'):
            pv.build_template('phase1-volume-test', 5, data_center_id='US-KS-2')

    def test_template_rejects_invalid_name_or_out_of_bound_size(self):
        with self.assertRaisesRegex(ValueError, 'invalid exact volume name'):
            pv.build_template('has spaces', 5)
        for bad_size in (0, -1, pv.VOLUME_SIZE_GB_CAP + 1, 5.5, True):
            with self.subTest(bad_size=bad_size), self.assertRaises(ValueError):
                pv.build_template('phase1-volume-test', bad_size)

    def test_template_accepts_boundary_sizes(self):
        pv.build_template('phase1-volume-test', 1)
        pv.build_template('phase1-volume-test', pv.VOLUME_SIZE_GB_CAP)

    def test_prepare_writes_template_and_pinned_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'prepared'
            result = pv.prepare(spec(size_gb=3), output, now=NOW)
            self.assertEqual(result['size_gb'], 3)
            self.assertFalse(result['size_default_used'])
            self.assertFalse(result['allocated'])
            saved = read(output / 'create-request-template.json')
            self.assertEqual(saved['body']['name'], 'phase1-volume-qualification-test')
            self.assertEqual(saved['body']['size'], 3)
            self.assertEqual(result['create_request_template']['path'],
                              str(output / 'create-request-template.json'))

    def test_prepare_refuses_existing_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'prepared'
            pv.prepare(spec(), output, now=NOW)
            with self.assertRaisesRegex(ValueError, 'fresh volume output required'):
                pv.prepare(spec(), output, now=NOW)

    def test_prepare_rejects_wrong_schema(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, 'wrong volume preparation schema'):
                pv.prepare(spec(schema='other'), Path(temporary) / 'prepared', now=NOW)

    def test_prepared_template_matches_pinned_hash(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'prepared'
            result = pv.prepare(spec(), output, now=NOW)
            template = pv.prepared_template(output)
            self.assertEqual(template, read(Path(result['create_request_template']['path'])))


class SizeBoundTests(unittest.TestCase):
    def test_default_size_matches_documented_formula(self):
        expected_bytes = pv.PACKAGE_KINDS_PER_LEASE * pv.RECOVERY_HEADROOM_MULTIPLIER * pv.PACKAGE_BYTES_ESTIMATE
        import math
        expected_gb = max(1, min(math.ceil(expected_bytes / 1_000_000_000), pv.VOLUME_SIZE_GB_CAP))
        self.assertEqual(pv.default_size_gb(), expected_gb)
        self.assertEqual(pv.default_size_gb(), 2)

    def test_default_size_is_derived_from_about_230mb_per_package(self):
        # FRESH-CLOUD-RESULTS-001.md: 1,391,298,940 exported bytes / 6 hot roots.
        self.assertAlmostEqual(pv.PACKAGE_BYTES_ESTIMATE, 1_391_298_940 / 6, delta=1)
        self.assertAlmostEqual(pv.PACKAGE_BYTES_ESTIMATE / 1_000_000, 230, delta=5)

    def test_default_size_never_exceeds_the_fifty_gb_cap(self):
        self.assertLessEqual(pv.default_size_gb(), 50)
        self.assertEqual(pv.VOLUME_SIZE_GB_CAP, 50)

    def test_prepare_uses_default_size_when_spec_omits_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = pv.prepare(spec(), Path(temporary) / 'prepared', now=NOW)
            self.assertEqual(result['size_gb'], pv.default_size_gb())
            self.assertTrue(result['size_default_used'])

    def test_prepare_rejects_a_requested_size_above_the_cap(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError):
                pv.prepare(spec(size_gb=51), Path(temporary) / 'prepared', now=NOW)


class PlaceholderTests(unittest.TestCase):
    def test_placeholder_is_the_exact_same_object_as_prepare_lease(self):
        self.assertIs(pv.KEY_PLACEHOLDER, prepare_lease.KEY_PLACEHOLDER)
        self.assertEqual(pv.KEY_PLACEHOLDER, '__INJECT_AT_POST_DO_NOT_SAVE__')

    def test_saved_template_carries_the_placeholder_not_a_real_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'prepared'
            pv.prepare(spec(), output, now=NOW)
            raw_text = (output / 'create-request-template.json').read_text()
            self.assertIn(pv.KEY_PLACEHOLDER, raw_text)
            self.assertIn('Bearer ' + pv.KEY_PLACEHOLDER, raw_text)
            # The body has no env/key field at all: a volume has no runtime of its own.
            body = json.loads(raw_text)['body']
            self.assertNotIn('env', body)
            self.assertNotIn('RUNPOD_API_KEY', json.dumps(body))

    def test_placeholder_never_reaches_the_live_client(self):
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = volume()
            template = pv.build_template('phase1-volume-test', 2)
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pv.execute_create(template, temporary, snapshot, api=api, now=NOW)
            name, size, data_center = api.create.call_args.args
            self.assertNotIn('fake-sensitive-key', (name, size, data_center))
            self.assertNotIn(pv.KEY_PLACEHOLDER, (name, size, data_center))


class SanitizedVerifyTests(unittest.TestCase):
    def test_sanitize_keeps_only_the_four_documented_fields(self):
        raw = volume(createdAt='2026-09-15T00:00:00Z', dataCenter={'id': 'EU-RO-1'}, extraField='drop-me')
        sanitized = pv.sanitize_volume(raw)
        self.assertEqual(sanitized, {'id': 'agv6w2qcg7', 'name': 'phase1-volume-qualification-test',
                                      'size_gb': 2, 'data_center_id': 'EU-RO-1'})

    def test_sanitize_rejects_missing_or_malformed_fields(self):
        with self.assertRaises(ValueError):
            pv.sanitize_volume({'id': 'x', 'name': 'y', 'dataCenterId': 'EU-RO-1'})
        with self.assertRaises(ValueError):
            pv.sanitize_volume(volume(size='2'))
        with self.assertRaises(ValueError):
            pv.sanitize_volume(volume(size=True))

    def test_verify_writes_only_sanitized_fields_to_disk(self):
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = snapshot_path(temporary)
            api = Mock()
            api.get.return_value = volume(extraField='must-not-be-saved', ownerId='acct-secret')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                result = pv.verify_volume('agv6w2qcg7', temporary, snapshot, api=api, now=NOW)
            self.assertEqual(result['id'], 'agv6w2qcg7')
            self.assertEqual(result['size_gb'], 2)
            self.assertEqual(result['data_center_id'], 'EU-RO-1')
            self.assertTrue(result['matches_prepare_lease_data_center'])
            api.get.assert_called_once_with('agv6w2qcg7')
            saved_text = (Path(temporary) / 'verify-agv6w2qcg7.json').read_text()
            self.assertNotIn('must-not-be-saved', saved_text)
            self.assertNotIn('ownerId', saved_text)
            self.assertNotIn('acct-secret', saved_text)
            self.assertNotIn('fake-sensitive-key', saved_text)

    def test_verify_rejects_id_mismatch_and_missing_volume(self):
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = snapshot_path(temporary)
            api = Mock(); api.get.return_value = volume(id='different-id')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'differs from the requested id'):
                    pv.verify_volume('agv6w2qcg7', temporary, snapshot, api=api, now=NOW)
            api2 = Mock(); api2.get.return_value = None
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'volume not found'):
                    pv.verify_volume('agv6w2qcg7', temporary, snapshot, api=api2, now=NOW)

    def test_verify_flags_a_data_center_other_than_prepare_lease_without_raising(self):
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = snapshot_path(temporary)
            api = Mock(); api.get.return_value = volume(dataCenterId='US-KS-2')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                result = pv.verify_volume('agv6w2qcg7', temporary, snapshot, api=api, now=NOW)
            self.assertFalse(result['matches_prepare_lease_data_center'])


class FundingSnapshotGateTests(unittest.TestCase):
    def test_execute_create_refuses_without_a_funding_snapshot_path(self):
        api = Mock()
        template = pv.build_template('phase1-volume-test', 2)
        with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
            with self.assertRaisesRegex(ValueError, 'funding-snapshot is required'):
                pv.execute_create(template, tempfile.gettempdir(), None, api=api, now=NOW)
        api.create.assert_not_called()

    def test_verify_refuses_without_a_funding_snapshot_path(self):
        api = Mock()
        with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
            with self.assertRaisesRegex(ValueError, 'funding-snapshot is required'):
                pv.verify_volume('agv6w2qcg7', tempfile.gettempdir(), None, api=api, now=NOW)
        api.get.assert_not_called()

    def test_execute_create_refuses_a_stale_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            stale = snapshot_path(temporary, age_seconds=31 * 60)
            api = Mock()
            template = pv.build_template('phase1-volume-test', 2)
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'stale'):
                    pv.execute_create(template, temporary, stale, api=api, now=NOW)
            api.create.assert_not_called()

    def test_verify_refuses_a_stale_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            stale = snapshot_path(temporary, age_seconds=45 * 60)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'stale'):
                    pv.verify_volume('agv6w2qcg7', temporary, stale, api=api, now=NOW)
            api.get.assert_not_called()

    def test_execute_create_refuses_a_future_dated_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            future = snapshot_path(temporary, age_seconds=-600)
            api = Mock()
            template = pv.build_template('phase1-volume-test', 2)
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaises(ValueError):
                    pv.execute_create(template, temporary, future, api=api, now=NOW)
            api.create.assert_not_called()

    def test_execute_create_accepts_a_snapshot_just_inside_thirty_minutes(self):
        with tempfile.TemporaryDirectory() as temporary:
            fresh = snapshot_path(temporary, age_seconds=29 * 60 + 59)
            api = Mock(); api.create.return_value = volume()
            template = pv.build_template('phase1-volume-test', 2)
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pv.execute_create(template, temporary, fresh, api=api, now=NOW)
            api.create.assert_called_once()

    def test_execute_create_and_verify_refuse_without_the_environment_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            fresh = snapshot_path(temporary)
            api = Mock()
            template = pv.build_template('phase1-volume-test', 2)
            with patch.dict('os.environ', {}, clear=False):
                import os
                os.environ.pop('RUNPOD_API_KEY', None)
                with self.assertRaisesRegex(ValueError, 'RUNPOD_API_KEY must be set'):
                    pv.execute_create(template, temporary, fresh, api=api, now=NOW)
                with self.assertRaisesRegex(ValueError, 'RUNPOD_API_KEY must be set'):
                    pv.verify_volume('agv6w2qcg7', temporary, fresh, api=api, now=NOW)
            api.create.assert_not_called(); api.get.assert_not_called()


class CostNoteTests(unittest.TestCase):
    def test_cost_note_converts_monthly_storage_price_to_an_hourly_figure(self):
        note = pv.cost_note(2)
        self.assertEqual(note['storage_usd_per_gb_month'], 0.07)
        self.assertAlmostEqual(note['monthly_storage_usd'], 0.14)
        self.assertAlmostEqual(note['storage_usd_hour'], 0.14 / pv.HOURS_PER_MONTH)
        self.assertGreater(note['storage_usd_hour'], 0)

    def test_cost_note_switches_tier_over_one_terabyte(self):
        self.assertEqual(pv.storage_rate_usd_per_gb_month(1000), 0.07)
        self.assertEqual(pv.storage_rate_usd_per_gb_month(1001), 0.05)

    def test_prepare_receipt_carries_a_cost_note_usable_on_the_lease_storage_line(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = pv.prepare(spec(size_gb=5), Path(temporary) / 'prepared', now=NOW)
            self.assertIn('storage_usd_hour', result['cost_note'])
            self.assertGreater(result['cost_note']['storage_usd_hour'], 0)


class EndToEndExecuteTests(unittest.TestCase):
    def test_main_offline_then_execute_writes_created_volume_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            spec_path = Path(temporary) / 'spec.json'
            write(spec_path, spec(size_gb=2))
            output = Path(temporary) / 'prepared'
            argv = ['prepare_volume.py', '--spec', str(spec_path), '--output', str(output)]
            with patch.object(sys, 'argv', argv):
                pv.main()
            self.assertTrue((output / 'create-request-template.json').exists())

            # main() has no --now hook and calls the real clock, so this snapshot
            # must be fresh against wall time, not the fixed NOW used elsewhere.
            snapshot = snapshot_path(temporary, now=time.time())
            fake_api = Mock(); fake_api.create.return_value = volume()
            argv = ['prepare_volume.py', '--output', str(output), '--execute',
                    '--funding-snapshot', str(snapshot)]
            with patch.object(sys, 'argv', argv), \
                 patch.object(pv, 'Provider', return_value=fake_api) as provider_cls, \
                 patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pv.main()
            provider_cls.assert_called_once_with('fake-sensitive-key')
            fake_api.create.assert_called_once_with('phase1-volume-qualification-test', 2, 'EU-RO-1')
            created = read(output / 'created-volume.json')
            self.assertEqual(created['id'], 'agv6w2qcg7')
            self.assertNotIn('fake-sensitive-key', json.dumps(created))

    def test_main_verify_requires_execute_flag(self):
        with tempfile.TemporaryDirectory() as temporary:
            argv = ['prepare_volume.py', '--verify', 'agv6w2qcg7', '--output', temporary]
            with patch.object(sys, 'argv', argv):
                with self.assertRaisesRegex(ValueError, '--verify has no offline action'):
                    pv.main()

    def test_main_verify_with_execute_uses_mocked_client(self):
        with tempfile.TemporaryDirectory() as temporary:
            snapshot = snapshot_path(temporary, now=time.time())
            fake_api = Mock(); fake_api.get.return_value = volume()
            argv = ['prepare_volume.py', '--verify', 'agv6w2qcg7', '--output', temporary,
                    '--execute', '--funding-snapshot', str(snapshot)]
            with patch.object(sys, 'argv', argv), \
                 patch.object(pv, 'Provider', return_value=fake_api), \
                 patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pv.main()
            fake_api.get.assert_called_once_with('agv6w2qcg7')
            self.assertTrue((Path(temporary) / 'verify-agv6w2qcg7.json').exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
