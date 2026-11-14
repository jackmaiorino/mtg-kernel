"""Runpod REST API v2 surface shared by the lease tools (#692: REST v1 retired 2026-11-15).

Offline: every provider call is mocked. The allowed request fields are the v2
OpenAPI schema's (https://api.runpod.io/v2/openapi.json, retrieved 2026-11-14),
since v2 rejects unknown body fields with a 422.
"""
from __future__ import annotations
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import external_guard  # noqa: E402
import lease_guard  # noqa: E402
from prepare_lease import prepare, sanitize_pod  # noqa: E402
import prepare_volume  # noqa: E402
from test_cloud import lease  # noqa: E402
import verify_pod_absent  # noqa: E402

# CreatePodRequest = ContainerConfig + its own properties (unevaluatedProperties: false).
CREATE_POD_FIELDS = {'args', 'cmd', 'entrypoint', 'disk', 'env', 'image', 'ports', 'registry',
                     'name', 'cloud', 'cpu', 'dataCenterIds', 'globalNetworking', 'gpu', 'mounts',
                     'startJupyter', 'startSsh', 'templateId', 'queue'}
CREATE_CPU_FIELDS = {'id', 'vcpuCount'}
NETWORK_MOUNT_FIELDS = {'volumeId', 'path'}
CREATE_VOLUME_FIELDS = {'name', 'size', 'dataCenter', 'type'}


def page(pods, next_cursor=None):
    return {'pods': pods, 'pagination': {'nextCursor': next_cursor, 'hasNextPage': next_cursor is not None}}


class ListAllTests(unittest.TestCase):
    def test_walks_every_page_in_order(self):
        pages = {None: page([{'id': 'a'}], 'c1'), 'c1': page([{'id': 'b'}], 'c2'), 'c2': page([{'id': 'c'}])}
        seen = []

        def fetch(cursor):
            seen.append(cursor)
            return pages[cursor]
        self.assertEqual(lease_guard.list_all(fetch, 'pods'), [{'id': 'a'}, {'id': 'b'}, {'id': 'c'}])
        self.assertEqual(seen, [None, 'c1', 'c2'])

    def test_a_malformed_or_unfinished_census_is_never_complete(self):
        bad_pages = (
            [{'id': 'a'}],                                                    # v1's bare list
            {'pods': [{'id': 'a'}]},                                          # no pagination block
            {'pods': None, 'pagination': {'hasNextPage': False, 'nextCursor': None}},
            {'pods': [], 'pagination': {'hasNextPage': 'no', 'nextCursor': None}},
            {'pods': [], 'pagination': {'hasNextPage': True, 'nextCursor': None}},   # more pages, no cursor
            {'pods': [], 'pagination': {'hasNextPage': True, 'nextCursor': ''}},
        )
        for bad in bad_pages:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                lease_guard.list_all(lambda cursor, bad=bad: bad, 'pods')
        with self.assertRaisesRegex(ValueError, 'did not end'):
            lease_guard.list_all(lambda cursor: page([], 'again'), 'pods')

    def test_external_census_reads_the_v2_endpoint_and_follows_the_cursor(self):
        bodies = iter([page([{'id': 'pod1', 'name': 'n'}], 'next+1'), page([{'id': 'pod2', 'name': 'n'}])])
        urls = []

        def urlopen(request, timeout):
            urls.append(request.full_url)
            self.assertEqual(request.get_header('Authorization'), 'Bearer fake-key')
            response = MagicMock()
            response.__enter__.return_value.read.return_value = json.dumps(next(bodies)).encode()
            return response
        with patch.object(external_guard, 'require_boundary'), patch('urllib.request.urlopen', side_effect=urlopen):
            census = external_guard.list_pods('fake-key')
        self.assertEqual([entry['id'] for entry in census], ['pod1', 'pod2'])
        self.assertEqual(urls, ['https://api.runpod.io/v2/pods', 'https://api.runpod.io/v2/pods?cursor=next%2B1'])


class PodShapeTests(unittest.TestCase):
    def test_network_volume_ids(self):
        self.assertEqual(lease_guard.pod_network_volume_ids({}), [])
        self.assertEqual(lease_guard.pod_network_volume_ids({'mounts': {'persistent': {'size': 10, 'path': '/w'}}}), [])
        self.assertEqual(lease_guard.pod_network_volume_ids(
            {'mounts': {'network': [{'volumeId': 'v1', 'path': '/a'}, {'volumeId': 'v2', 'path': '/b'}]}}), ['v1', 'v2'])
        for bad in ({'mounts': []}, {'mounts': {'network': 'v1'}}, {'mounts': {'network': [{'path': '/a'}]}},
                    {'mounts': {'network': [{'volumeId': '', 'path': '/a'}]}}, {'mounts': {'network': [None]}}):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                lease_guard.pod_network_volume_ids(bad)

    def test_ssh_direct_only_while_running_with_a_published_port(self):
        direct = {'host': '192.0.2.1', 'port': 40022, 'username': 'root', 'command': 'ssh ...'}
        self.assertEqual(lease_guard.pod_ssh_direct({'status': 'RUNNING', 'ssh': {'proxy': None, 'direct': direct}}),
                         ('192.0.2.1', 40022))
        for bad in ({'status': 'EXITED', 'ssh': {'direct': direct}}, {'status': 'RUNNING', 'ssh': {'direct': None}},
                    {'status': 'RUNNING'}, {'status': 'RUNNING', 'ssh': {'direct': dict(direct, port='40022')}},
                    {'status': 'RUNNING', 'ssh': {'direct': dict(direct, port=0)}},
                    {'status': 'RUNNING', 'ssh': {'direct': dict(direct, host='a b; rm -rf /')}}, None):
            with self.subTest(bad=bad):
                self.assertIsNone(lease_guard.pod_ssh_direct(bad))


class RequestSchemaTests(unittest.TestCase):
    def test_pod_create_body_carries_only_v2_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            spec = {'schema': 'phase1-cloud-lease-preparation/v1', 'lease': lease(),
                    'account_observed_epoch': 1000, 'quote_observed_epoch': 1000,
                    'quoted_cpu_usd_hour': .96, 'observed_pods': 0,
                    'autopay_enabled': False, 'public_ssh_key': 'ssh-ed25519 AAAATEST public'}
            body = json.loads(Path(prepare(spec, Path(temporary) / 'prepared', now=1001)
                                   ['create_request_template']['path']).read_text())
        self.assertLessEqual(set(body), CREATE_POD_FIELDS)
        self.assertEqual(set(body['cpu']), CREATE_CPU_FIELDS)
        self.assertEqual(body['cpu'], {'id': 'cpu3c', 'vcpuCount': 32})
        self.assertNotIn('gpu', body)
        self.assertEqual(set(body['mounts']), {'network'})
        self.assertEqual(body['mounts']['network'], [{'volumeId': 'testvolume', 'path': '/workspace'}])
        self.assertTrue(all(set(mount) == NETWORK_MOUNT_FIELDS for mount in body['mounts']['network']))
        self.assertEqual(body['cloud'], 'SECURE')
        self.assertIn('22/tcp', body['ports'])
        self.assertTrue(all(isinstance(value, str) for value in body['env'].values()))
        self.assertTrue(isinstance(body['entrypoint'], list) and all(isinstance(a, str) for a in body['entrypoint']))

    def test_volume_create_body_carries_only_v2_fields(self):
        body = prepare_volume.build_template('phase1-volume-test', 10)['body']
        self.assertLessEqual(set(body), CREATE_VOLUME_FIELDS)
        self.assertEqual(body['dataCenter'], 'EU-RO-1')

    def test_no_tool_still_targets_rest_v1(self):
        root = Path(__file__).resolve().parents[3]
        for path in (list((root / 'python/tools/phase1_cloud').glob('*.py'))
                     + [root / 'python/tools/conditional_continuation_dispatch_v1.py',
                        root / 'python/tools/nine_deck_cloud_v1.py',
                        root / 'python/tools/g115_d3_cloud_v1.py',
                        root / 'tools/search_sharing_allocation/cloud_lease.py']):
            if path.name == Path(__file__).name or not path.exists():
                continue
            with self.subTest(path=path.name):
                text = path.read_text(encoding='utf-8')
                self.assertNotIn('rest.runpod.io', text)
                self.assertNotIn("get('portMappings')", text)


class ResourceIdTests(unittest.TestCase):
    def test_v2_prefixed_ids_are_accepted_and_unsafe_ids_rejected(self):
        valid = ('pod_abc123', 'vol_xyz', 'pod-123_abc', 'a', 'abc123')
        invalid = ('', '_pod', '-pod', 'pod_abc123.presence', 'pod_abc123.absent',
                   'pod/evil', 'pod\\evil', 'pod?x=1', 'pod#frag', 'pod%20', 'pod id', None, 123)
        for resource_id in valid:
            with self.subTest(valid=resource_id):
                self.assertTrue(external_guard.valid_id(resource_id))
                self.assertEqual(lease_guard.Provider('fake-key', resource_id).pod_id, resource_id)
                self.assertEqual(verify_pod_absent.endpoint_for(resource_id),
                                 'https://api.runpod.io/v2/pods/' + resource_id)
        for bad in invalid:
            with self.subTest(invalid=bad):
                self.assertFalse(external_guard.valid_id(bad))
                with self.assertRaises(ValueError):
                    lease_guard.Provider('fake-key', bad)
                if isinstance(bad, str):
                    with self.assertRaises(ValueError):
                        verify_pod_absent.endpoint_for(bad)

    def test_sanitize_pod_and_volume_accept_v2_prefixed_ids(self):
        pod = sanitize_pod({
            'id': 'pod_abc123', 'name': 'phase1-test', 'status': 'RUNNING', 'cost': 0.96,
            'dataCenterId': 'EU-RO-1',
            'mounts': {'network': [{'volumeId': 'vol_xyz', 'path': '/workspace'}]},
        })
        self.assertEqual(pod['id'], 'pod_abc123')
        self.assertEqual(pod['volume_id'], 'vol_xyz')
        volume = prepare_volume.sanitize_volume({
            'id': 'vol_xyz', 'name': 'phase1-volume-test', 'size': 20, 'dataCenter': 'EU-RO-1',
        })
        self.assertEqual(volume['id'], 'vol_xyz')
        for bad_id in ('_pod', 'pod.presence', 'pod/evil'):
            with self.subTest(bad_id=bad_id), self.assertRaises(ValueError):
                sanitize_pod({
                    'id': bad_id, 'name': 'phase1-test', 'status': 'RUNNING', 'cost': 0.96,
                    'mounts': {'network': [{'volumeId': 'vol_xyz', 'path': '/workspace'}]},
                })
            with self.subTest(bad_vol=bad_id), self.assertRaises(ValueError):
                prepare_volume.sanitize_volume({
                    'id': bad_id, 'name': 'phase1-volume-test', 'size': 20, 'dataCenter': 'EU-RO-1',
                })

    def test_guards_retain_and_release_underscore_pod_id(self):
        owned, listed, malformed = external_guard.census_entries(
            [{'id': 'pod_abc123', 'name': 'phase1-test'}], 'phase1-test')
        self.assertEqual((owned, listed, malformed), (['pod_abc123'], {'pod_abc123'}, False))

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            known = root / 'lease/known-pods'
            with external_guard.LeaseClient(known, 'phase1-test', 'fake-key', clock=lambda: 1000.0,
                                            sleep=lambda _: None) as client:
                created = client.post({'name': 'phase1-test'},
                                      poster=lambda _: {'id': 'pod_abc123', 'name': 'phase1-test'})
            self.assertEqual(created['id'], 'pod_abc123')
            self.assertTrue((known / 'pod_abc123.json').exists())

            calls = []
            clock = [100.0]
            api = MagicMock()
            api.call.side_effect = lambda method: (calls.append(method), None)[1]
            with patch('time.sleep'), patch('time.time', side_effect=lambda: clock.__setitem__(0, clock[0] + 31.0) or clock[0]):
                lease_guard.release_after_failure(
                    api, {'name': 'phase1-test', 'network_volume_id': 'vol_xyz'},
                    'pod_abc123', root / 'guard', 'test_failure')
            self.assertIn('DELETE', calls)

            snapshot = root / 'funding.json'
            snapshot.write_text(json.dumps({
                'observed_utc': '1970-01-01T00:16:40+00:00',
            }), encoding='utf-8')
            vol_api = MagicMock()
            vol_api.get.return_value = {
                'id': 'vol_xyz', 'name': 'phase1-volume-test', 'size': 20, 'dataCenter': 'EU-RO-1',
            }
            absent_api = MagicMock()
            absent_api.lookup.return_value = {'status': 404, 'kind': 'not_found', 'body': None, 'reason': None}
            with patch.dict(os.environ, {'RUNPOD_API_KEY': 'fake-key'}):
                verified = prepare_volume.verify_volume('vol_xyz', root / 'vol', snapshot, api=vol_api, now=1010)
                absence, _ = verify_pod_absent.verify_absence(
                    'pod_abc123', root / 'absent', repeat=2, interval=0,
                    api=absent_api, now=lambda: 1010.0, sleep=lambda _: None)
            self.assertEqual(verified['id'], 'vol_xyz')
            self.assertTrue(absence['provider_absent'])


if __name__ == '__main__':
    unittest.main()
