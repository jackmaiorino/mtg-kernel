"""Offline coverage of the workstation-side guard. No provider calls or credentials."""
import tempfile
import unittest
from pathlib import Path

from common import read, write
from external_guard import run
from test_cloud import lease


class Stop(BaseException):
    pass


class FakeProvider:
    def __init__(self, pods):
        self.pods = list(pods); self.calls = []

    def __call__(self, key, pod_id):
        self.calls.append(('new', key, pod_id)); return self

    def call(self, method):
        self.calls.append((method,))
        return self.pods.pop(0) if method == 'GET' else {}


class ExternalGuardTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        write(self.root / 'lease/lease.json', lease())
        self.now = 1100.

    def pod(self, **changes):
        return {'id': 'pod1', 'name': lease()['name'], 'costPerHr': 0.96, **changes}

    def guard(self, provider, pods=lambda key: [], sleeps=1):
        count = [0]
        def sleep(seconds):
            count[0] += 1
            if count[0] >= sleeps:
                raise Stop()
        try:
            run(self.root, 'k', pods=pods, provider=provider, clock=lambda: self.now, sleep=sleep)
        except Stop:
            pass

    def test_releases_pod_that_never_starts_its_worker(self):
        write(self.root / 'lease/created-pod.json', {'id': 'pod1'})
        self.now = 2000.
        provider = FakeProvider([self.pod(), None])
        self.guard(provider, sleeps=2)
        self.assertIn(('DELETE',), provider.calls)
        self.assertEqual(read(self.root / 'external-guard.json')['reason'], 'startup_idle')
        self.assertTrue(read(self.root / 'external-guard-completion.json')['provider_absent'])

    def test_started_worker_defers_to_resident_guard_until_release_authorized(self):
        write(self.root / 'lease/created-pod.json', {'id': 'pod1'})
        write(self.root / 'worker-started.json', {})
        self.now = 2000.
        provider = FakeProvider([self.pod(), self.pod()])
        self.guard(provider, sleeps=1)
        self.assertNotIn(('DELETE',), provider.calls)
        self.assertIsNone(read(self.root / 'external-guard.json')['reason'])
        write(self.root / 'release-authorized.json', {})
        self.guard(provider, sleeps=1)
        self.assertIn(('DELETE',), provider.calls)
        self.assertEqual(read(self.root / 'external-guard.json')['reason'], 'controller_release')

    def test_deadline_releases_even_with_worker_running(self):
        write(self.root / 'lease/created-pod.json', {'id': 'pod1'})
        write(self.root / 'worker-started.json', {})
        self.now = lease()['deadline_epoch'] - 60
        provider = FakeProvider([self.pod()])
        self.guard(provider)
        self.assertIn(('DELETE',), provider.calls)
        self.assertEqual(read(self.root / 'external-guard.json')['reason'], 'deadline')

    def test_discovers_pod_by_exact_name_and_never_deletes_a_collision(self):
        provider = FakeProvider([self.pod()])
        self.guard(provider, pods=lambda key: [{'id': 'pod1', 'name': lease()['name']},
                                               {'id': 'other', 'name': 'someone-else'}])
        self.assertEqual(read(self.root / 'external-discovered-pod.json')['id'], 'pod1')
        provider = FakeProvider([])
        named = {'id': 'x', 'name': lease()['name']}
        self.root = Path(tempfile.mkdtemp()); write(self.root / 'lease/lease.json', lease())
        self.guard(provider, pods=lambda key: [named, dict(named, id='y')])
        self.assertEqual(provider.calls, [])
        self.assertEqual(read(self.root / 'external-guard-error.json')['error_type'], 'ValueError')

    def test_absent_pod_completes_after_deadline_without_provider_handle(self):
        self.now = lease()['deadline_epoch']
        provider = FakeProvider([])
        self.guard(provider, sleeps=5)
        self.assertEqual(provider.calls, [])
        self.assertTrue(read(self.root / 'external-guard-completion.json')['provider_absent'])

    def test_wrong_pod_identity_is_an_error_not_a_delete(self):
        write(self.root / 'lease/created-pod.json', {'id': 'pod1'})
        self.now = 2000.
        provider = FakeProvider([self.pod(name='renamed')])
        self.guard(provider)
        self.assertNotIn(('DELETE',), provider.calls)
        self.assertEqual(read(self.root / 'external-guard-error.json')['error_type'], 'ValueError')


if __name__ == '__main__':
    unittest.main()
