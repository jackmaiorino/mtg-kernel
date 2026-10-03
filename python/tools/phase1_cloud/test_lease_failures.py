"""Offline failure injection for own-Pod release with unavailable local state.

No real provider, subprocess, native binary, account query or credential is used.
"""
import ast
import base64
import builtins
import copy
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import Mock, MagicMock, patch
import lease_guard
from prepare_lease import prepare
from test_cloud import lease


def pod(**changes):
    return {'id':'pod123','name':lease()['name'],'cpuFlavorId':'cpu3c','vcpuCount':32,
            'networkVolumeId':'testvolume','costPerHr':.96}|changes


def provider(results):
    api=Mock();api.call.side_effect=results
    api.funds.return_value={'balance_usd':31.,'account_spend_usd_hour':1.15,'under_balance':False}
    return api


def methods(api):return [call.args[0] for call in api.call.call_args_list]


class LeaseFailureTests(unittest.TestCase):
    def test_every_receipt_unwritable_still_deletes_then_verifies_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),pod(),{},None])
            with patch('lease_guard.write',side_effect=OSError('unwritable state')),\
                 patch('lease_guard.time.time',return_value=1000),patch('lease_guard.time.sleep'):
                lease_guard.run(lease(),temporary,'pod123',api)
            self.assertEqual(methods(api),['GET','GET','DELETE','GET'])

    def test_startup_mkdir_failure_does_not_prevent_release(self):
        api=provider([pod(),{},None])
        with patch('pathlib.Path.mkdir',side_effect=PermissionError('denied')),patch('lease_guard.time.sleep'):
            lease_guard.run(lease(),'unused-unwritable-state','pod123',api)
        self.assertEqual(methods(api),['GET','DELETE','GET'])
        api.funds.assert_not_called()

    def test_startup_lock_failure_does_not_prevent_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),{},None])
            fake_fcntl=types.SimpleNamespace(LOCK_EX=1,LOCK_NB=2,flock=lambda *args:None)
            with patch.dict(sys.modules,{'fcntl':fake_fcntl}),\
                 patch('pathlib.Path.open',side_effect=PermissionError('denied')),patch('lease_guard.time.sleep'):
                lease_guard.run(lease(),temporary,'pod123',api,acquire_lock=True)
            self.assertEqual(methods(api),['GET','DELETE','GET'])

    def test_invalid_budget_releases_only_prepared_own_pod(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),{},None])
            with patch('lease_guard.time.sleep'):
                lease_guard.run(lease()|{'increment_cap_usd':11},temporary,'pod123',api)
            self.assertEqual(methods(api),['GET','DELETE','GET'])

    def test_unreadable_previous_state_does_not_abandon_guard(self):
        with tempfile.TemporaryDirectory() as temporary:
            (Path(temporary)/'guard.json').write_text('{invalid')
            api=provider([pod(),{},None])
            with patch('lease_guard.time.sleep'):
                lease_guard.run(lease(),temporary,'pod123',api)
            self.assertEqual(methods(api),['GET','DELETE','GET'])

    def test_unexpected_policy_failure_releases_without_logging_exception_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),pod(),{},None]);out=io.StringIO();err=io.StringIO()
            with patch('lease_guard.decision',side_effect=RuntimeError('SECRET_MUST_NOT_APPEAR')),\
                 patch('lease_guard.time.sleep'),patch('sys.stdout',out),patch('sys.stderr',err):
                lease_guard.run(lease(),temporary,'pod123',api)
            self.assertEqual(methods(api),['GET','GET','DELETE','GET'])
            self.assertNotIn('SECRET_MUST_NOT_APPEAR',out.getvalue()+err.getvalue())
            for file in Path(temporary).glob('*.json'):
                self.assertNotIn('SECRET_MUST_NOT_APPEAR',file.read_text())

    def test_ambiguous_delete_retries_even_when_all_receipts_fail(self):
        api=provider([pod(),RuntimeError('unavailable'),pod(),{},None])
        with patch('lease_guard.write',side_effect=OSError('no disk')),patch('lease_guard.time.sleep') as sleep:
            lease_guard.release_after_failure(api,lease(),'pod123','unused','test_failure')
        self.assertEqual(methods(api),['GET','DELETE','GET','DELETE','GET'])
        self.assertEqual(sleep.call_count,2)

    def test_wrong_id_or_name_never_reaches_delete(self):
        for wrong in (pod(id='other'),pod(name='unrelated-pod')):
            with self.subTest(wrong=wrong['id']+'/'+wrong['name']),tempfile.TemporaryDirectory() as temporary:
                api=provider([wrong,pod(),{},None])
                with patch('lease_guard.time.sleep'):
                    lease_guard.release_after_failure(api,lease(),'pod123',temporary,'test_failure')
                self.assertEqual(methods(api),['GET','GET','DELETE','GET'])

    def test_wrong_cpu_or_volume_still_releases_owned_pod_without_volume_operation(self):
        for wrong in (pod(vcpuCount=64),pod(cpuFlavorId='wrong'),pod(networkVolumeId='different')):
            with self.subTest(shape=wrong),tempfile.TemporaryDirectory() as temporary:
                api=provider([wrong,wrong,{},None])
                with patch('lease_guard.time.sleep'):
                    lease_guard.run(lease(),temporary,'pod123',api)
                self.assertEqual(methods(api),['GET','GET','DELETE','GET'])
                api.funds.assert_not_called()

    def test_failed_absence_receipt_does_not_turn_absence_into_failure(self):
        api=provider([None])
        with patch('lease_guard.write',side_effect=OSError('no disk')):
            lease_guard.release_after_failure(api,lease(),'pod123','unused','test_failure')
        self.assertEqual(methods(api),['GET'])

    def test_malformed_configuration_uses_independent_prepared_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),{},None])
            identity={'name':lease()['name'],'network_volume_id':'testvolume'}
            with patch('lease_guard.time.sleep'):
                lease_guard.run(None,temporary,'pod123',api,expected_identity=identity)
            self.assertEqual(methods(api),['GET','DELETE','GET'])

    def test_missing_ownership_cannot_guess_a_deletion_target(self):
        api=provider([])
        with self.assertRaisesRegex(ValueError,'exact prepared Pod identity unavailable'):
            lease_guard.run(None,'unused','pod123',api)
        api.call.assert_not_called()

    def test_cli_unreadable_config_uses_explicit_prepared_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=provider([pod(),{},None])
            argv=['lease_guard.py','--lease','missing','--state',temporary,
                  '--expected-name',lease()['name'],'--expected-volume','testvolume']
            with patch.object(sys,'argv',argv),patch.dict(os.environ,{'RUNPOD_POD_ID':'pod123','RUNPOD_API_KEY':'fake'}),\
                 patch('lease_guard.read',side_effect=OSError('missing')),patch('lease_guard.Provider',return_value=api),\
                 patch('lease_guard.time.sleep'):
                lease_guard.main()
            self.assertEqual(methods(api),['GET','DELETE','GET'])


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
        responses=[]
        for data in (pod(),{},None):
            response=MagicMock()
            if data is None:
                import urllib.error
                responses.append(urllib.error.HTTPError('https://rest.runpod.io/v1/pods/pod123',404,'absent',{},None))
            else:
                response.__enter__.return_value.read.return_value=json.dumps(data).encode()
                responses.append(response)
        fake_fcntl=types.SimpleNamespace(LOCK_EX=1,LOCK_NB=2,flock=lambda *args:None)
        with patch.dict(sys.modules,{'fcntl':fake_fcntl}),\
             patch.dict(os.environ,{'RUNPOD_POD_ID':'pod123','RUNPOD_API_KEY':'fake-sensitive-key'}),\
             patch('pathlib.Path.mkdir',side_effect=PermissionError('unwritable /run')),\
             patch('urllib.request.urlopen',side_effect=responses) as request,patch('time.sleep'):
            exec(self.guard_program,{})
        self.assertEqual([call.args[0].method for call in request.call_args_list],['GET','DELETE','GET'])
        for call in request.call_args_list:
            self.assertEqual(call.args[0].full_url,'https://rest.runpod.io/v1/pods/pod123')
            self.assertNotIn('fake-sensitive-key',call.args[0].full_url)


if __name__=='__main__':unittest.main(verbosity=2)
