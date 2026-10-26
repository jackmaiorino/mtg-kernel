"""Offline failure injection for own-Pod release with unavailable local state.

No real provider, subprocess, native binary, account query or credential is used.
Every guard loop runs on a fake clock with a hard sleep cap and a fake provider
with a hard call cap (lease_test_support), so a regression ends the test instead
of looping.
"""
import ast
import base64
import builtins
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
import urllib.error
from unittest import mock
from unittest.mock import Mock, MagicMock, patch
import lease_guard
from lease_test_support import FakeClock, Stop, World, guard_time, no_network
from prepare_lease import prepare
from test_cloud import lease


def pod(**changes):
    return {'id':'pod123','name':lease()['name'],'cpuFlavorId':'cpu3c','vcpuCount':32,
            'networkVolumeId':'testvolume','costPerHr':.96}|changes


def guard(test, value, state, world, start=1100., pod_id='pod123', **options):
    clock = FakeClock(start)
    with no_network(), guard_time(lease_guard, clock):
        lease_guard.run(value, state, pod_id, world.provider('k', pod_id), **options)
    return clock


def release(world, state, clock, pod_id='pod123', reason='test_failure'):
    with no_network(), guard_time(lease_guard, clock):
        lease_guard.release_after_failure(world.provider('k', pod_id), lease(), pod_id, state, reason)
    return clock


class LeaseFailureTests(unittest.TestCase):
    def test_every_receipt_unwritable_still_deletes_then_verifies_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()])
            with patch('lease_guard.write', side_effect=OSError('unwritable state')):
                clock = guard(self, lease() | {'startup_idle_seconds': 60}, temporary, world)
            self.assertIn('DELETE', world.methods())
            self.assertEqual(world.pods, {})
            # startup_idle latches at once; DELETE follows within the recovery window
            self.assertLessEqual(clock.t, 1100 + lease()['recovery_seconds'] + 60)

    def test_startup_mkdir_failure_does_not_prevent_release(self):
        world = World([pod()])
        with patch('pathlib.Path.mkdir', side_effect=PermissionError('denied')):
            guard(self, lease(), 'unused-unwritable-state', world)
        self.assertEqual(world.methods()[0], 'DELETE')
        self.assertEqual(world.funds_calls, 0)

    def test_startup_lock_failure_does_not_prevent_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()])
            fake_fcntl = types.SimpleNamespace(LOCK_EX=1, LOCK_NB=2, flock=lambda *args: None)
            with patch.dict(sys.modules, {'fcntl': fake_fcntl}), \
                    patch('pathlib.Path.open', side_effect=PermissionError('denied')):
                guard(self, lease(), temporary, world, acquire_lock=True)
            self.assertEqual(world.methods()[0], 'DELETE')

    def test_invalid_budget_releases_only_prepared_own_pod(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod(), pod(id='other')])
            guard(self, lease() | {'increment_cap_usd': 11}, temporary, world)
            self.assertEqual(world.methods()[0], 'DELETE')
            self.assertEqual({pod_id for _, pod_id in world.log}, {'pod123'})
            self.assertIn('other', world.pods)

    def test_unreadable_previous_state_does_not_abandon_guard(self):
        with tempfile.TemporaryDirectory() as temporary:
            (Path(temporary) / 'guard.json').write_text('{invalid')
            world = World([pod()])
            guard(self, lease(), temporary, world)
            self.assertEqual(world.methods()[0], 'DELETE')

    def test_unexpected_policy_failure_releases_without_logging_exception_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()]); out = io.StringIO(); err = io.StringIO()
            with patch('lease_guard.decision', side_effect=RuntimeError('SECRET_MUST_NOT_APPEAR')), \
                    patch('sys.stdout', out), patch('sys.stderr', err):
                guard(self, lease(), temporary, world)
            self.assertEqual(world.methods()[:2], ['GET', 'DELETE'])
            self.assertNotIn('SECRET_MUST_NOT_APPEAR', out.getvalue() + err.getvalue())
            for file in Path(temporary).glob('*.json'):
                self.assertNotIn('SECRET_MUST_NOT_APPEAR', file.read_text())

    def test_ambiguous_delete_retries_even_when_all_receipts_fail(self):
        world = World([pod()]); world.delete_failures = 2
        with patch('lease_guard.write', side_effect=OSError('no disk')):
            clock = release(world, 'unused', FakeClock(1100))
        self.assertEqual(world.deletes[:3], ['pod123'] * 3)
        self.assertEqual(world.pods, {})
        self.assertLessEqual(clock.sleeps, 12)

    def test_recovery_deletes_only_the_runtime_id_whatever_the_get_returns(self):
        # The DELETE target is the container's own RUNPOD_POD_ID, never a file-supplied
        # or provider-supplied ID; a present Pod of any name is never absence.
        for wrong in (pod(id='other'), pod(name='unrelated-pod')):
            with self.subTest(wrong=wrong['id'] + '/' + wrong['name']), tempfile.TemporaryDirectory() as temporary:
                world = World([pod()]); world.get_script = iter([wrong, wrong])
                release(world, temporary, FakeClock(1100))
                self.assertEqual(world.methods()[0], 'DELETE')
                self.assertEqual({pod_id for _, pod_id in world.log}, {'pod123'})
                self.assertEqual(json.loads((Path(temporary) / 'released.json').read_text())
                                 ['absent_confirmations'], 2)

    def test_wrong_cpu_or_volume_still_releases_owned_pod_without_volume_operation(self):
        for wrong in (pod(vcpuCount=64), pod(cpuFlavorId='wrong'), pod(networkVolumeId='different')):
            with self.subTest(shape=wrong), tempfile.TemporaryDirectory() as temporary:
                world = World([wrong])
                guard(self, lease(), temporary, world)
                self.assertEqual(world.methods()[:2], ['GET', 'DELETE'])
                self.assertEqual(world.funds_calls, 0)

    def test_failed_absence_receipt_does_not_turn_absence_into_failure(self):
        world = World([])
        with patch('lease_guard.write', side_effect=OSError('no disk')):
            clock = release(world, 'unused', FakeClock(1100))
        self.assertGreaterEqual(clock.t - 1100, lease_guard.ABSENT_CONFIRM_SECONDS)

    def test_malformed_configuration_uses_independent_prepared_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()])
            identity = {'name': lease()['name'], 'network_volume_id': 'testvolume'}
            guard(self, None, temporary, world, expected_identity=identity)
            self.assertEqual(world.methods()[0], 'DELETE')

    def test_missing_runtime_id_cannot_guess_a_deletion_target(self):
        world = World([pod()])
        with self.assertRaisesRegex(SystemExit, 'runtime Pod ID unavailable'):
            guard(self, None, 'unused', world, pod_id='')
        self.assertEqual(world.calls, 0)

    def test_cli_unreadable_config_uses_explicit_prepared_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()]); clock = FakeClock(1100)
            argv = ['lease_guard.py', '--lease', 'missing', '--state', temporary,
                    '--expected-name', lease()['name'], '--expected-volume', 'testvolume']
            with patch.object(sys, 'argv', argv), \
                    patch.dict(os.environ, {'RUNPOD_POD_ID': 'pod123', 'RUNPOD_API_KEY': 'fake'}), \
                    patch('lease_guard.read', side_effect=OSError('missing')), \
                    patch('lease_guard.Provider', side_effect=world.provider), \
                    no_network(), guard_time(lease_guard, clock):
                lease_guard.main()
            self.assertEqual(world.methods()[0], 'DELETE')


class PriceAndReleaseTests(unittest.TestCase):
    """mtg-kernel-collab#335 items 2 and 5 on the resident guard."""

    def test_pod_rate_counts_adjusted_and_rejects_malformed(self):
        self.assertEqual(lease_guard.pod_rate({'costPerHr': .96, 'adjustedCostPerHr': 1.05}), 1.05)
        for bad in ({}, {'costPerHr': None}, {'costPerHr': 'n/a'}, {'costPerHr': 0}, {'costPerHr': float('nan')},
                    {'costPerHr': True}, {'costPerHr': -1}, {'costPerHr': {'x': 1}},
                    {'costPerHr': 1, 'adjustedCostPerHr': 'y'}):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                lease_guard.pod_rate(bad)

    def test_verify_pod_rejects_rate_above_ceiling(self):
        with self.assertRaisesRegex(ValueError, 'above ceiling'):
            lease_guard.verify_pod(pod(adjustedCostPerHr=1.5), lease(), 'pod123')
        self.assertEqual(lease_guard.verify_pod(pod(costPerHr=.96, vcpuCount=64), lease() | {'vcpu_count': 64},
                                                'pod123'), .96)

    def test_malformed_or_missing_price_releases_at_once(self):
        missing = pod(); del missing['costPerHr']
        for bad in (pod(costPerHr='n/a'), pod(costPerHr=None), pod(costPerHr=float('nan')), missing,
                    pod(adjustedCostPerHr=2.0)):
            with self.subTest(bad=bad), tempfile.TemporaryDirectory() as temporary:
                world = World([bad])
                clock = guard(self, lease(), temporary, world)
                self.assertEqual(world.methods()[:2], ['GET', 'DELETE'])
                self.assertLess(clock.t - 1100, 60)
                self.assertEqual(json.loads((Path(temporary) / 'released.json').read_text())
                                 ['absent_confirmations'], 2)

    def test_single_absent_observation_does_not_complete(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([pod()])
            # absent, present (resets), then absent until confirmed
            world.get_script = iter(['absent', 'present', 'absent', 'absent', 'absent', 'absent', 'absent',
                                     'absent', 'absent', 'absent', 'absent'])
            gets = []
            original = world.provider

            def recording(key, pod_id):
                api = original(key, pod_id)
                call = api.call

                def wrapped(method):
                    result = call(method)
                    if method == 'GET':
                        gets.append((lease_guard.time.time(), result is None))
                    return result
                api.call = wrapped
                return api
            clock = FakeClock(1100)
            with no_network(), guard_time(lease_guard, clock):
                lease_guard.release_after_failure(recording('k', 'pod123'), lease(), 'pod123', temporary, 'test')
            last_present = max(t for t, absent in gets if not absent)
            self.assertGreaterEqual(gets[-1][0] - last_present, lease_guard.ABSENT_CONFIRM_SECONDS)
            self.assertTrue(gets[0][1])   # the first absence alone did not end recovery

    def test_one_absent_then_stop_leaves_no_release_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            world = World([]); clock = FakeClock(1100, max_sleeps=1)
            with self.assertRaises(Stop):
                release(world, temporary, clock)
            self.assertFalse((Path(temporary) / 'released.json').exists())

    def test_deadline_margin_deletes_before_the_deadline_despite_productive_work(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary); value = lease()
            world = World([pod()]); clock = FakeClock(value['deadline_epoch'] - 900)
            delete_times = []
            original = world.provider

            def productive(key, pod_id):
                api = original(key, pod_id); call = api.call

                def wrapped(method):
                    (state / 'progress.json').write_text(json.dumps({
                        'pod_id': 'pod123', 'epoch': clock.t, 'last_productive_epoch': clock.t,
                        'last_activity_epoch': clock.t, 'native_alive': True, 'queued_work': True,
                        'finished': False}))
                    if method == 'DELETE':
                        delete_times.append(clock.t)
                    return call(method)
                api.call = wrapped
                return api
            with no_network(), guard_time(lease_guard, clock):
                lease_guard.run(value, state, 'pod123', productive('k', 'pod123'))
            self.assertLessEqual(delete_times[0], value['deadline_epoch'] - lease_guard.DELETE_MARGIN_SECONDS
                                 + value['poll_seconds'])


class IdleTests(unittest.TestCase):
    """#335 item 6: a progress record refreshed on a timer never hides a hung worker."""

    def test_heartbeat_without_activity_releases_after_the_idle_bound(self):
        with tempfile.TemporaryDirectory() as temporary:
            state = Path(temporary); value = lease(); clock = FakeClock(1100)
            world = World([pod()]); original = world.provider

            def heartbeat(key, pod_id):
                api = original(key, pod_id); call = api.call

                def wrapped(method):
                    # The controller is alive and rewrites progress.json every poll, but no work
                    # completes and native CPU time does not advance: activity stays frozen.
                    (state / 'progress.json').write_text(json.dumps({
                        'pod_id': 'pod123', 'epoch': clock.t, 'last_productive_epoch': 1100,
                        'last_activity_epoch': 1100, 'native_alive': True, 'queued_work': True,
                        'finished': False}))
                    return call(method)
                api.call = wrapped
                return api
            with no_network(), guard_time(lease_guard, clock):
                lease_guard.run(value, state, 'pod123', heartbeat('k', 'pod123'))
            self.assertEqual(json.loads((state / 'stop-request.json').read_text())['reason'],
                             'native_work_stalled_or_idle')
            self.assertIn('DELETE', world.methods())
            self.assertLessEqual(clock.t, 1100 + value['work_idle_seconds'] + value['recovery_seconds'] + 60)


class BootstrapFailureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary=tempfile.TemporaryDirectory();root=Path(cls.temporary.name)
        spec={'schema':'phase1-cloud-lease-preparation/v1','lease':lease(),
              'account_observed_epoch':1000,'quote_observed_epoch':1000,
              'quoted_cpu_usd_hour':.96,'observed_pods':0,'autopay_enabled':False,
              'public_ssh_key':'ssh-ed25519 AAAATEST public'}
        result=prepare(spec,root/'prepared',now=1001)
        body=json.loads(Path(result['create_request_template']['path']).read_text())
        cls.command=body['dockerEntrypoint'][2]
        cls.bootstrap=base64.b64decode(cls.command.split("'")[1]).decode()
        tree=ast.parse(cls.bootstrap)
        cls.guard_program=ast.literal_eval(tree.body[0].value)

    @classmethod
    def tearDownClass(cls):cls.temporary.cleanup()

    def instrumented_bootstrap(self):
        # Execute real bootstrap control flow with an inert recovery program.
        tree=ast.parse(self.bootstrap)
        tree.body[0].value=ast.Constant("import builtins;builtins._phase1_test_events.append(globals().get('PHASE1_RECOVERY_ONLY'))")
        return compile(ast.fix_missing_locations(tree),'<bootstrap-regression>','exec')

    def test_embedded_guard_compiles_without_file_staging_and_fits_single_argument(self):
        compile(self.guard_program,'<embedded-guard-program>','exec')
        compile(self.bootstrap,'<bootstrap>','exec')
        self.assertLess(len(self.command.encode()),128*1024)
        self.assertNotIn('RUNPOD_API_KEY=',self.guard_program)
        self.assertIn('EXPECTED_IDENTITY=',self.guard_program)
        self.assertNotIn('root.mkdir',self.bootstrap)

    def test_guard_spawn_failure_recovers_in_parent_without_any_filesystem_write(self):
        events=[]
        with patch.object(builtins,'_phase1_test_events',events,create=True),\
             patch('subprocess.Popen',side_effect=OSError('spawn failed')),\
             patch('pathlib.Path.mkdir',side_effect=PermissionError('no state')),patch('sys.stdout',io.StringIO()):
            with self.assertRaises(SystemExit) as exit:
                exec(self.instrumented_bootstrap(),{})
        self.assertEqual(exit.exception.code,0);self.assertEqual(events,[True])

    def test_guard_death_during_setup_stops_setup_then_recovers(self):
        events=[];guard=Mock();guard.poll.return_value=1;guard.returncode=1
        setup=Mock();setup.poll.return_value=None;setup.wait.return_value=-15
        with patch.object(builtins,'_phase1_test_events',events,create=True),\
             patch('subprocess.Popen',side_effect=[guard,setup]) as spawn,\
             patch.dict(os.environ,{'RUNPOD_API_KEY':'fake-sensitive-key'}),patch('sys.stdout',io.StringIO()):
            with self.assertRaises(SystemExit):exec(self.instrumented_bootstrap(),{})
        self.assertEqual(events,[True]);setup.terminate.assert_called_once()
        self.assertNotIn('RUNPOD_API_KEY',spawn.call_args_list[1].kwargs['env'])

    def test_actual_embedded_guard_releases_with_unwritable_run_directory(self):
        world=World([pod()]);requests=[]

        def urlopen(request,timeout=None):
            requests.append((request.method,request.full_url))
            result=world.provider('k',request.full_url.rsplit('/',1)[1]).call(request.method)
            if result is None:
                raise urllib.error.HTTPError(request.full_url,404,'absent',{},None)
            response=MagicMock()
            response.__enter__.return_value.read.return_value=json.dumps(result).encode()
            return response
        clock=FakeClock(1100)
        fake_fcntl=types.SimpleNamespace(LOCK_EX=1,LOCK_NB=2,flock=lambda *args:None)
        with patch.dict(sys.modules,{'fcntl':fake_fcntl}),\
             patch.dict(os.environ,{'RUNPOD_POD_ID':'pod123','RUNPOD_API_KEY':'fake-sensitive-key'}),\
             patch('pathlib.Path.mkdir',side_effect=PermissionError('unwritable /run')),\
             no_network(),patch('urllib.request.urlopen',urlopen),\
             patch('time.time',clock),patch('time.sleep',clock.sleep):
            exec(self.guard_program,{})
        self.assertEqual(requests[0][0],'DELETE')
        self.assertEqual(world.pods,{})
        for _,url in requests:
            self.assertEqual(url,'https://rest.runpod.io/v1/pods/pod123')
            self.assertNotIn('fake-sensitive-key',url)


if __name__=='__main__':unittest.main(verbosity=2)
