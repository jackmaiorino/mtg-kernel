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
                'cost_per_hour_usd': 1.28, 'data_center_id': 'EU-RO-1', 'data_center_id_reported': True,
                'volume_id': lease()['network_volume_id']})
            saved_text = (output / 'created-pod.json').read_text()
            self.assertNotIn('must-not-be-saved', saved_text)
            self.assertNotIn('secret-machine', saved_text)
            self.assertNotIn('fake-sensitive-key', saved_text)
            for saved in output.iterdir():
                if saved.is_file():
                    self.assertNotIn('fake-sensitive-key', saved.read_text())

    def test_key_is_injected_into_the_posted_env_in_memory_only(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = pod_response()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            (body,) = api.create.call_args.args
            self.assertEqual(body['env']['RUNPOD_API_KEY'], 'fake-sensitive-key')
            self.assertEqual(pl.prepared_template(output)['env']['RUNPOD_API_KEY'], pl.KEY_PLACEHOLDER)
            for saved in output.iterdir():
                if saved.is_file():
                    self.assertNotIn('fake-sensitive-key', saved.read_text())

    def test_template_without_placeholder_is_refused_before_posting(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); api.create.return_value = pod_response()
            template = pl.prepared_template(output); template['env']['RUNPOD_API_KEY'] = 'already-filled'
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'key placeholder'):
                    pl.execute_create(template, output, snapshot, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()

    def test_missing_data_center_in_response_is_recorded_as_none(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            api = Mock(); response = pod_response(); response['dataCenterId'] = None
            api.create.return_value = response
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                result = pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            self.assertIsNone(result['data_center_id'])
            self.assertFalse(result['data_center_id_reported'])


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


class AmbiguousCreateTests(unittest.TestCase):
    """collab#335 review r1 finding 2: only an explicit 4xx proves that no Pod exists."""

    def attempt(self, temporary, error):
        output = prepared_output(temporary)
        snapshot = snapshot_path(temporary)
        api = Mock(); api.create.side_effect = error
        with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
            with self.assertRaises(type(error)):
                pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
        self.snapshot = snapshot
        return output

    def test_timeout_or_5xx_records_an_ambiguous_create(self):
        for error in (RuntimeError('provider request unavailable'), RuntimeError('provider HTTP 503'),
                      TimeoutError('timed out')):
            with self.subTest(error=str(error)), tempfile.TemporaryDirectory() as temporary:
                output = self.attempt(temporary, error)
                self.assertTrue((output / 'create-attempt.json').exists())
                self.assertTrue((output / 'create-ambiguous.json').exists())
                self.assertFalse(read(output / 'create-pod-failure.json')['definite'])

    def test_explicit_4xx_is_definite_and_not_ambiguous(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = self.attempt(temporary, RuntimeError('provider HTTP 400'))
            self.assertTrue(read(output / 'create-pod-failure.json')['definite'])
            self.assertFalse((output / 'create-ambiguous.json').exists())

    def test_second_create_of_the_same_preparation_refuses_before_posting(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = self.attempt(temporary, RuntimeError('provider request unavailable'))
            snapshot = self.snapshot
            api = Mock()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
                with self.assertRaisesRegex(ValueError, 'earlier create receipt'):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            api.create.assert_not_called()


class RetainFirstTests(unittest.TestCase):
    """Round 3 review finding 5 (I1): a valid POST ID is retained before sanitization or receipts."""

    def run_create(self, temporary, response, write_patch=None):
        output = prepared_output(temporary)
        snapshot = snapshot_path(temporary)
        api = Mock(); api.create.return_value = response
        with patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}):
            if write_patch:
                with patch.object(pl, 'write', write_patch), self.assertRaises(Exception):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
            else:
                with self.assertRaises(Exception):
                    pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW)
        return output

    def test_sanitization_failure_keeps_the_valid_id(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = self.run_create(temporary, pod_response(costPerHr='bad'))
            self.assertEqual(read(output / 'known-pods' / 'pod123.json')['role'], 'receipt')
            self.assertEqual(read(output / 'create-pod-failure.json')['pod_id'], 'pod123')
            self.assertEqual(read(output / 'create-ambiguous.json')['pod_id'], 'pod123')

    def test_receipt_write_failure_keeps_the_valid_id(self):
        original = pl.write

        def failing(path, value, **kwargs):
            if Path(path).name == 'created-pod.json':
                raise OSError('disk full')
            return original(path, value, **kwargs)
        with tempfile.TemporaryDirectory() as temporary:
            output = self.run_create(temporary, pod_response(), write_patch=failing)
            self.assertEqual(read(output / 'known-pods' / 'pod123.json')['role'], 'receipt')   # handed off first

    def test_non_string_id_is_never_coerced(self):
        for bad in (None, True, 123):
            with self.subTest(bad=bad), tempfile.TemporaryDirectory() as temporary:
                output = self.run_create(temporary, pod_response(id=bad))
                self.assertFalse((output / 'known-pods').exists())
                self.assertIsNone(read(output / 'create-pod-failure.json')['pod_id'])


class ResponsibilityTests(unittest.TestCase):
    """Round 4 findings 3 and 5: I1 retains the raw POST ID first; I4 keeps responsibility for it."""

    def attempt(self, temporary, response, world, handoff_fails=False):
        from lease_test_support import FakeClock, no_network
        output = prepared_output(temporary)
        snapshot = snapshot_path(temporary)
        api = Mock(); api.create.return_value = response
        clock = FakeClock(lease()['deadline_epoch'] - 60)   # bounds the release loop to MIN_RELEASE_SECONDS
        release = {'provider': world.provider, 'pods': world.list, 'clock': clock, 'sleep': clock.sleep}
        patches = [patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'})]
        if handoff_fails:
            import external_guard
            patches.append(patch.object(external_guard, 'handoff_in', side_effect=OSError('read-only store')))
        with no_network(), patches[0], (patches[1] if handoff_fails else patch.object(pl, 'MIN_RELEASE_SECONDS', 120)):
            with self.assertRaises(Exception) as raised:
                pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api, now=NOW,
                                  release=release)
        return output, raised.exception

    def test_post_id_is_retained_whatever_the_name(self):
        from lease_test_support import World
        for response in ({'id': 'pod123'}, {'id': 'pod123', 'name': 'another-name'}):
            with self.subTest(response=response), tempfile.TemporaryDirectory() as temporary:
                output, _ = self.attempt(temporary, response, World())
                self.assertEqual(read(output / 'known-pods' / 'pod123.json')['role'], 'receipt')
                self.assertEqual(read(output / 'create-pod-failure.json')['pod_id'], 'pod123')

    def test_failed_handoff_deletes_and_verifies_absence_before_exiting(self):
        from lease_test_support import World
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod_response()])
            output, error = self.attempt(temporary, pod_response(), world, handoff_fails=True)
            self.assertIn('deleted and verified absent', str(error))
            self.assertEqual(world.deletes[0], 'pod123')
            self.assertNotIn('pod123', world.pods)

    def test_recovered_handoff_reports_a_guarded_pod_and_writes_the_receipt(self):
        # round 9: the POST handoff fails, the cleanup's census hands the ID off durably: the Pod is
        # alive and guarded, not deleted, and its receipt is written
        from lease_test_support import FakeClock, World, no_network
        import external_guard
        with tempfile.TemporaryDirectory() as temporary:
            output = prepared_output(temporary)
            snapshot = snapshot_path(temporary)
            world = World([pod_response()])
            api = Mock(); api.create.return_value = pod_response()
            clock = FakeClock(lease()['deadline_epoch'] - 60)
            release = {'provider': world.provider, 'pods': world.list, 'clock': clock, 'sleep': clock.sleep}
            real = external_guard.handoff_in; fired = []

            def handoff(folder, pod_id, role, epoch):
                if not fired:
                    fired.append(pod_id)
                    raise OSError('transient')
                return real(folder, pod_id, role, epoch)
            with no_network(), patch.dict('os.environ', {'RUNPOD_API_KEY': 'fake-sensitive-key'}), \
                    patch.object(external_guard, 'handoff_in', side_effect=handoff):
                result = pl.execute_create(pl.prepared_template(output), output, snapshot, 'testvolume', api=api,
                                           now=NOW, release=release)
            self.assertEqual(result['id'], 'pod123')
            self.assertEqual(world.deletes, [])
            self.assertEqual(read(output / 'created-pod.json')['id'], 'pod123')
            self.assertEqual(read(output / 'known-pods' / 'pod123.json')['role'], 'receipt')
            self.assertIn('alive and guarded', read(output / 'create-handoff-recovered.json')['state'])

    def test_failed_handoff_and_failed_release_fail_loudly(self):
        from lease_test_support import World
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod_response()]); world.delete_failures = -1
            output, error = self.attempt(temporary, pod_response(), world, handoff_fails=True)
            self.assertIn('ALERT', str(error))
            self.assertEqual(read(output / 'ALERT-pod-not-released.json')['pod_ids'], ['pod123'])
            self.assertGreaterEqual(len(world.deletes), 2)


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
