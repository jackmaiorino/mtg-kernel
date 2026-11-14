"""Runpod REST API v2 surface shared by the lease tools (#692: REST v1 retired 2026-11-15).

Offline: every provider call is mocked. The allowed request fields are the v2
OpenAPI schema's (https://api.runpod.io/v2/openapi.json, retrieved 2026-11-14),
since v2 rejects unknown body fields with a 422.
"""
from __future__ import annotations
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import external_guard  # noqa: E402
import lease_guard  # noqa: E402
from prepare_lease import prepare  # noqa: E402
import prepare_volume  # noqa: E402
from test_cloud import lease  # noqa: E402

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


if __name__ == '__main__':
    unittest.main()
