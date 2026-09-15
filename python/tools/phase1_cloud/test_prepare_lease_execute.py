"""Offline/mock coverage for prepare_lease.py's --execute path. No provider
calls, no real credentials, no native processes.

Mirrors test_prepare_volume.py's EndToEndExecuteTests/FundingSnapshotGateTests
style: a Mock stands in for the live client, and --execute is exercised only
through that mock.
"""
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

import prepare_lease as pl
from common import read, write
from runtime_observation import IMAGE_REFERENCE
from test_cloud import lease
from test_prepare_volume import NOW, snapshot_path


def spec(**changes):
    return {'schema': 'phase1-cloud-lease-preparation/v1', 'lease': lease(),
            'account_observed_epoch': 1000., 'quote_observed_epoch': 1000.,
            'quoted_cpu_usd_hour': .96, 'observed_pods': 0, 'autopay_enabled': False,
            'public_ssh_key': 'ssh-ed25519 AAAATEST public'} | changes


def prepared_output(temporary, allow_mutable_image=False, **spec_changes):
    output = Path(temporary) / 'prepared'
    pl.prepare(spec(**spec_changes), output, now=1001, allow_mutable_image=allow_mutable_image)
    return output


def pod_response(**changes):
    return {'id': 'pod123', 'name': lease()['name'], 'desiredStatus': 'RUNNING',
            'costPerHr': 1.28, 'dataCenterId': 'EU-RO-1',
            'networkVolumeId': lease()['network_volume_id'],
            'imageName': IMAGE_REFERENCE} | changes


class ImagePinTests(unittest.TestCase):
    def test_prepare_defaults_the_image_to_the_pinned_digest(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            template = read(output / 'create-request-template.json')
            self.assertEqual(template['imageName'], IMAGE_REFERENCE)

    def test_prepare_refuses_a_mutable_tag_without_the_allow_flag(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, 'mutable image tag refused'):
                pl.prepare(spec(image_name='python:3.12-slim-bookworm'), Path(temporary) / 'prepared', now=1001)

    def test_prepare_accepts_a_mutable_tag_with_the_allow_flag(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary, allow_mutable_image=True, image_name='python:3.12-slim-bookworm')
            template = read(output / 'create-request-template.json')
            self.assertEqual(template['imageName'], 'python:3.12-slim-bookworm')

    def test_execute_re_refuses_a_mutable_tag_even_when_prepare_allowed_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary, allow_mutable_image=True, image_name='python:3.12-slim-bookworm')
            snapshot = snapshot_path(temporary)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'mutable image tag refused'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()

    def test_execute_accepts_a_mutable_tag_with_its_own_allow_flag(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary, allow_mutable_image=True, image_name='python:3.12-slim-bookworm')
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = pod_response(imageName='python:3.12-slim-bookworm')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW,
                                  allow_mutable_image=True)
            api.create.assert_called_once()


class VolumeIdGateTests(unittest.TestCase):
    def test_execute_requires_volume_id(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, '--volume-id is required'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, None, api=api, now=NOW)
            api.create.assert_not_called()

    def test_execute_refuses_when_volume_id_differs_from_prepared_template(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'volume id differs'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'different-volume',
                                      api=api, now=NOW)
            api.create.assert_not_called()


class FundingSnapshotGateTests(unittest.TestCase):
    def test_execute_refuses_without_a_funding_snapshot_path(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'funding-snapshot is required'):
                    pl.execute_create(pl.prepared_template(output), output, None, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()

    def test_execute_refuses_a_stale_snapshot(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            stale = snapshot_path(temporary, age_seconds=31 * 60)
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'stale'):
                    pl.execute_create(pl.prepared_template(output), output, stale, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()

    def test_execute_accepts_a_snapshot_just_inside_thirty_minutes(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            fresh = snapshot_path(temporary, age_seconds=29 * 60 + 59)
            api = Mock(); api.create.return_value = pod_response()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pl.execute_create(pl.prepared_template(output), output, fresh, 'testvolume', api=api, now=NOW)
            api.create.assert_called_once()

    def test_execute_refuses_without_the_environment_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            fresh = snapshot_path(temporary)
            api = Mock()
            with patch.dict('os.environ', {}, clear=False):
                os.environ.pop('RUNPOD_API_KEY', None)
                with self.assertRaisesRegex(ValueError, 'RUNPOD_API_KEY must be set'):
                    pl.execute_create(pl.prepared_template(output), output, fresh, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()


class SanitizedSuccessTests(unittest.TestCase):
    def test_execute_writes_only_the_six_sanitized_fields_and_never_the_raw_body(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock()
            api.create.return_value = pod_response(extraField='must-not-be-saved', machineId='secret-machine')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                result = pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume',
                                           api=api, now=NOW)
            self.assertEqual(result, {'schema': pl.CREATED_SCHEMA, 'created_epoch': NOW,
                'id': 'pod123', 'name': lease()['name'], 'desired_status': 'RUNNING',
                'cost_per_hour_usd': 1.28, 'data_center_id': 'EU-RO-1',
                'volume_id': lease()['network_volume_id']})
            saved_text = (output / 'created-pod.json').read_text()
            self.assertNotIn('must-not-be-saved', saved_text)
            self.assertNotIn('secret-machine', saved_text)
            self.assertNotIn('fake-sensitive-key', saved_text)
            self.assertNotIn('fake-sensitive-key', json.dumps(api.create.call_args.args))

    def test_placeholder_never_reaches_the_live_client(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = pod_response()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            (body,) = api.create.call_args.args
            self.assertEqual(body['env']['RUNPOD_API_KEY'], pl.KEY_PLACEHOLDER)


class FailureRecordingTests(unittest.TestCase):
    def test_http_failure_records_a_sanitized_receipt_and_reraises(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.side_effect = RuntimeError('provider HTTP 500')
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(RuntimeError, 'provider HTTP 500'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            failure = read(output / 'create-pod-failure.json')
            self.assertEqual(failure['schema'], pl.FAILURE_SCHEMA)
            self.assertEqual(failure['error_type'], 'RuntimeError')
            self.assertIn('500', failure['error'])
            self.assertNotIn('fake-sensitive-key', json.dumps(failure))
            self.assertFalse((output / 'created-pod.json').exists())

    def test_malformed_response_records_a_failure_and_reraises(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = {'id': 'pod123'}
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'pod response missing'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            failure = read(output / 'create-pod-failure.json')
            self.assertEqual(failure['error_type'], 'ValueError')
            self.assertFalse((output / 'created-pod.json').exists())


def fresh_spec():
    # main() has no --now hook for the offline --spec step either, so the
    # lease's own created_epoch/account_observed_epoch/quote_observed_epoch
    # must be fresh against real wall time, not the fixed fixture epochs.
    now = time.time()
    base = lease()
    duration = base['deadline_epoch'] - base['created_epoch']
    fresh_lease = base | {'created_epoch': now, 'deadline_epoch': now + duration}
    return spec(lease=fresh_lease, account_observed_epoch=now, quote_observed_epoch=now)


class EndToEndExecuteTests(unittest.TestCase):
    def test_main_offline_then_execute_writes_created_pod_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            spec_path = Path(temporary) / 'spec.json'
            write(spec_path, fresh_spec())
            output = Path(temporary) / 'prepared'
            argv = ['prepare_lease.py', '--spec', str(spec_path), '--output', str(output)]
            with patch.object(sys, 'argv', argv):
                pl.main()
            self.assertTrue((output / 'create-request-template.json').exists())

            # main() has no --now hook and calls the real clock, so this snapshot
            # must be fresh against wall time, not the fixed NOW used elsewhere.
            snapshot = snapshot_path(temporary, now=time.time())
            fake_api = Mock(); fake_api.create.return_value = pod_response()
            argv = ['prepare_lease.py', '--output', str(output), '--execute',
                    '--funding-snapshot', str(snapshot), '--volume-id', lease()['network_volume_id']]
            with patch.object(sys, 'argv', argv), \
                 patch.object(pl, 'Provider', return_value=fake_api) as provider_cls, \
                 patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pl.main()
            provider_cls.assert_called_once_with('fake-sensitive-key')
            created = read(output / 'created-pod.json')
            self.assertEqual(created['id'], 'pod123')
            self.assertNotIn('fake-sensitive-key', json.dumps(created))


if __name__ == '__main__':
    unittest.main(verbosity=2)
