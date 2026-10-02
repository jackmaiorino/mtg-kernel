"""Offline/mock coverage for verify_pod_absent.py. No provider calls, no real
credentials.

Mirrors test_prepare_volume.py: a Mock stands in for the live client at the
`api=` injection point, and --execute is exercised only through that mock.
A second, smaller group of tests patches urllib.request.urlopen directly to
cover Provider.lookup's own HTTP-status/body classification without ever
opening a socket.
"""
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
import urllib.error
from unittest.mock import Mock, patch

import verify_pod_absent as vpa
from common import read

FAKE_KEY = 'fake-sensitive-key-should-never-be-written'
NOW = 1_800_000_000.0


def make_clock(start):
    """A deterministic clock: each call advances by one second."""
    state = {'stamp': start - 1.0}

    def clock():
        state['stamp'] += 1.0
        return state['stamp']
    return clock


def fail_on_sleep(seconds):
    raise AssertionError('sleep should not be called: ' + repr(seconds))


class FakeResponse:
    def __init__(self, status, body_bytes):
        self.status = status
        self._body = body_bytes

    def read(self):
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *exc_info):
        return False


def http_error(code):
    return urllib.error.HTTPError('https://rest.runpod.io/v1/pods/abc123', code, 'reason', {}, None)


class NotFoundConfirmationTests(unittest.TestCase):
    def test_two_consecutive_not_found_confirms_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.side_effect = [
                {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None},
                {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None},
            ]
            sleeps = []
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, path = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=sleeps.append)
            self.assertIs(result['provider_absent'], True)
            self.assertEqual(result['attempts_performed'], 2)
            self.assertEqual(result['repeat_required'], 2)
            self.assertEqual(sleeps, [30.0])
            self.assertIsNone(result['reason'])
            self.assertIsNone(result['pod'])
            self.assertFalse(result['key_saved'])
            self.assertEqual(vpa.exit_code_for(result), 0)
            self.assertEqual(api.lookup.call_count, 2)
            self.assertTrue(path.is_file())
            self.assertEqual(path.name, 'pod-absence-abc123.json')
            saved = read(path)
            self.assertEqual(saved, result)
            self.assertNotIn(FAKE_KEY, path.read_text())

    def test_a_listing_between_not_found_lookups_stops_early(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.side_effect = [
                {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None},
                {'status': 200, 'kind': 'listed',
                 'body': {'id': 'abc123', 'desiredStatus': 'RUNNING', 'costPerHr': 0.2}, 'reason': None},
            ]
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, _ = vpa.verify_absence('abc123', temporary, repeat=3, api=api,
                    now=make_clock(NOW), sleep=lambda s: None)
            self.assertIs(result['provider_absent'], False)
            self.assertEqual(result['attempts_performed'], 2)
            self.assertEqual(api.lookup.call_count, 2)


class ListedPodTests(unittest.TestCase):
    def test_listed_pod_returns_false_with_only_sanitized_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.return_value = {'status': 200, 'kind': 'listed',
                'body': {'id': 'abc123', 'name': 'phase1-qual', 'desiredStatus': 'RUNNING',
                         'costPerHr': 0.14, 'ownerId': 'acct-secret-owner', 'extra': 'drop-me'},
                'reason': None}
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, path = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=fail_on_sleep)
            self.assertIs(result['provider_absent'], False)
            self.assertEqual(result['pod'], {'desired_status': 'RUNNING', 'cost_per_hour_usd': 0.14})
            self.assertEqual(result['attempts_performed'], 1)
            self.assertEqual(result['http_status'], 200)
            self.assertIsNone(result['reason'])
            self.assertEqual(vpa.exit_code_for(result), 0)
            api.lookup.assert_called_once_with('abc123')
            saved_text = path.read_text()
            self.assertNotIn('acct-secret-owner', saved_text)
            self.assertNotIn('ownerId', saved_text)
            self.assertNotIn('drop-me', saved_text)
            self.assertNotIn('phase1-qual', saved_text)
            self.assertNotIn(FAKE_KEY, saved_text)


class AmbiguousOutcomeTests(unittest.TestCase):
    def test_5xx_is_ambiguous_and_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.return_value = {'status': 503, 'kind': 'error', 'body': None,
                                        'reason': 'provider_http_503'}
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, path = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=fail_on_sleep)
            self.assertIsNone(result['provider_absent'])
            self.assertEqual(result['http_status'], 503)
            self.assertEqual(result['reason'], 'provider_http_503')
            self.assertIsNone(result['pod'])
            self.assertEqual(vpa.exit_code_for(result), 1)
            self.assertTrue(path.is_file())
            self.assertNotIn(FAKE_KEY, path.read_text())

    def test_timeout_is_ambiguous_and_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.return_value = {'status': None, 'kind': 'error', 'body': None,
                                        'reason': 'request_unavailable'}
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, _ = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=fail_on_sleep)
            self.assertIsNone(result['provider_absent'])
            self.assertIsNone(result['http_status'])
            self.assertEqual(result['reason'], 'request_unavailable')
            self.assertEqual(vpa.exit_code_for(result), 1)

    def test_malformed_listed_body_is_ambiguous(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.return_value = {'status': 200, 'kind': 'listed',
                'body': {'id': 'abc123'}, 'reason': None}
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, _ = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=fail_on_sleep)
            self.assertIsNone(result['provider_absent'])
            self.assertEqual(result['reason'], 'malformed_response_body')
            self.assertEqual(result['http_status'], 200)
            self.assertEqual(vpa.exit_code_for(result), 1)

    def test_a_not_found_lookup_followed_by_an_ambiguous_one_never_confirms(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.side_effect = [
                {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None},
                {'status': 500, 'kind': 'error', 'body': None, 'reason': 'provider_http_500'},
            ]
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}):
                result, _ = vpa.verify_absence('abc123', temporary, api=api,
                    now=make_clock(NOW), sleep=lambda s: None)
            self.assertIsNone(result['provider_absent'])
            self.assertEqual(result['reason'], 'provider_http_500')


class ExecuteKeyGateTests(unittest.TestCase):
    def test_execute_refuses_without_the_environment_key(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            with patch.dict('os.environ', {}, clear=False):
                import os
                os.environ.pop('RUNPOD_API_KEY', None)
                with self.assertRaisesRegex(ValueError, 'RUNPOD_API_KEY must be set'):
                    vpa.verify_absence('abc123', temporary, api=api)
            api.lookup.assert_not_called()

    def test_key_never_appears_in_receipt_or_stdout_through_the_cli(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = Mock()
            api.lookup.return_value = {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None}
            captured = io.StringIO()
            with patch.dict('os.environ', {'RUNPOD_API_KEY': FAKE_KEY}), \
                 patch('verify_pod_absent.Provider', return_value=api), \
                 patch('sys.stdout', captured):
                exit_code = vpa.main(['--pod-id', 'abc123', '--output-dir', temporary,
                                      '--execute', '--repeat', '1'])
            self.assertEqual(exit_code, 0)
            self.assertNotIn(FAKE_KEY, captured.getvalue())
            receipt = Path(temporary) / 'pod-absence-abc123.json'
            self.assertNotIn(FAKE_KEY, receipt.read_text())


class NoExecuteTests(unittest.TestCase):
    def test_no_execute_prints_the_request_and_touches_no_network(self):
        captured = io.StringIO()
        with patch('verify_pod_absent.urllib.request.urlopen') as mocked_urlopen, \
             patch('sys.stdout', captured):
            exit_code = vpa.main(['--pod-id', 'abc123', '--repeat', '4', '--interval', '5'])
        self.assertEqual(exit_code, 0)
        mocked_urlopen.assert_not_called()
        printed = captured.getvalue()
        self.assertIn('"method": "GET"', printed)
        self.assertIn(vpa.PODS_URL + '/abc123', printed)
        self.assertIn(vpa.KEY_PLACEHOLDER, printed)
        self.assertNotIn(FAKE_KEY, printed)

    def test_no_execute_writes_no_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            captured = io.StringIO()
            with patch('sys.stdout', captured):
                vpa.main(['--pod-id', 'abc123', '--output-dir', temporary])
            self.assertEqual(list(Path(temporary).iterdir()), [])


class ProviderHttpClassificationTests(unittest.TestCase):
    """Provider.lookup's own status/body classification, with urlopen patched
    so no socket is ever opened."""

    def test_404_is_not_found(self):
        with patch('verify_pod_absent.urllib.request.urlopen', side_effect=http_error(404)):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome, {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None})

    def test_503_is_an_error(self):
        with patch('verify_pod_absent.urllib.request.urlopen', side_effect=http_error(503)):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome['kind'], 'error')
        self.assertEqual(outcome['status'], 503)
        self.assertEqual(outcome['reason'], 'provider_http_503')

    def test_unexpected_400_is_an_error_not_absence_or_presence(self):
        with patch('verify_pod_absent.urllib.request.urlopen', side_effect=http_error(400)):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome['kind'], 'error')
        self.assertEqual(outcome['reason'], 'unexpected_http_status_400')

    def test_malformed_200_body_is_an_error(self):
        with patch('verify_pod_absent.urllib.request.urlopen',
                   return_value=FakeResponse(200, b'not-json')):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome['kind'], 'error')
        self.assertEqual(outcome['reason'], 'malformed_response_body')

    def test_listed_200_body_is_parsed(self):
        body = json.dumps({'id': 'abc123', 'desiredStatus': 'RUNNING', 'costPerHr': 0.1}).encode()
        with patch('verify_pod_absent.urllib.request.urlopen',
                   return_value=FakeResponse(200, body)):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome['kind'], 'listed')
        self.assertEqual(outcome['status'], 200)
        self.assertEqual(outcome['body']['desiredStatus'], 'RUNNING')

    def test_request_unavailable_on_os_error(self):
        with patch('verify_pod_absent.urllib.request.urlopen', side_effect=OSError('no route')):
            outcome = vpa.Provider(FAKE_KEY).lookup('abc123')
        self.assertEqual(outcome, {'status': None, 'kind': 'error', 'body': None,
                                    'reason': 'request_unavailable'})


class SanitizePodListingTests(unittest.TestCase):
    def test_keeps_only_the_two_documented_fields(self):
        sanitized = vpa.sanitize_pod_listing({'id': 'x', 'desiredStatus': 'EXITED',
                                              'costPerHr': 0.0, 'extra': 'drop'})
        self.assertEqual(sanitized, {'desired_status': 'EXITED', 'cost_per_hour_usd': 0.0})

    def test_rejects_missing_or_malformed_fields(self):
        with self.assertRaises(ValueError):
            vpa.sanitize_pod_listing({'desiredStatus': 'RUNNING'})
        with self.assertRaises(ValueError):
            vpa.sanitize_pod_listing({'desiredStatus': 'RUNNING', 'costPerHr': '0.1'})
        with self.assertRaises(ValueError):
            vpa.sanitize_pod_listing({'desiredStatus': 'RUNNING', 'costPerHr': True})


if __name__ == '__main__':
    unittest.main()
