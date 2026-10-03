"""Offline/mock coverage. No provider calls, native processes or real credentials."""
import base64
import copy
import io
import json
from pathlib import Path
import tarfile
import tempfile
import types
import unittest
from unittest.mock import patch
from common import encoded, pin, read, sha, write
from lease_guard import decision, Provider, validate, verify_pod, run as run_guard, funding_status
from pack import prepare, Packager
from prepare_lease import prepare as prepare_lease, KEY_PLACEHOLDER
from cloud_worker import export_completed, restore, stage, receipt_timings, run as run_worker, recovery_backlog, export_with_backlog_limit
from throughput import cgroup_limits, qualify, production_reference, ThroughputPolicy, canonical_training_config, TIMING_PHASES, complete_work_seconds
from reference import build as build_reference
from semantic import ALGORITHM, trajectory_digest, validated_map


def timing_fixture(root, reference_pin, environment, overhead=0):
    reference=read(reference_pin['path']);duration=reference['elapsed_seconds']
    evidence=root/(environment+'-timing-observation.json')
    write(evidence,{'synthetic_test_only':True,'provider_absent':environment=='cloud'},replace=True)
    begin=1000.;native_begin=begin+overhead;end=native_begin+duration
    phases={name:{'start_seconds':begin,'end_seconds':native_begin,'evidence':[pin(evidence)]}
            for name in TIMING_PHASES}
    phases['native_invocation']={'start_seconds':native_begin,'end_seconds':end,'evidence':[pin(evidence)]}
    for name in ('checkpoint_export','output_transfer','release_confirmation'):
        phases[name]={'start_seconds':end,'end_seconds':end,'evidence':[pin(evidence)]}
    if environment=='local':
        for name in ('allocation','image_startup','release_confirmation'):
            phases[name]={'not_applicable':True,'reason':'existing local machine in synthetic unit fixture',
                          'evidence':[pin(evidence)]}
    value={'schema':'phase1-complete-work-timing/v1','complete':True,'environment':environment,
           'reference':reference_pin,'clock':'single_controller_monotonic','clock_session':'synthetic-unit-test',
           'start_seconds':begin,'end_seconds':end,'phases':phases}
    path=root/(environment+'-timing.json');write(path,value,replace=True)
    return pin(path)


def lease():
    return {'schema': 'phase1-cloud-lease/v1', 'name': 'phase1-qualification-test',
        'created_epoch': 1000., 'deadline_epoch': 15400., 'increment_cap_usd': 10.,
        'total_cap_usd': 200., 'prior_conservative_usd': 56.,
        'rate_ceiling_usd_hour': 1.1, 'storage_usd_hour': .05,
        'postrun_storage_reserve_usd': .84, 'recovery_reserve_usd': 1.,
        'funded_balance_usd': 31., 'safety_multiplier': 1.2,
        'startup_idle_seconds': 900, 'work_idle_seconds': 300,
        'recovery_seconds': 180, 'poll_seconds': 5,
        'network_volume_id': 'testvolume'}


class GuardTests(unittest.TestCase):
    def test_full_duration_includes_storage_recovery_and_margin(self):
        self.assertAlmostEqual(validate(lease()), 7.728)

    def test_reject_caps_funding_and_missing_margin(self):
        for changes in ({'increment_cap_usd': 11}, {'funded_balance_usd': 5},
                        {'prior_conservative_usd': 198}, {'safety_multiplier': 1.1},
                        {'deadline_epoch': float('nan')}):
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                validate(lease() | changes)

    def test_deadline_allows_recovery(self):
        self.assertEqual(decision(lease(), 15400 - 180, 1.1)['reason'], 'deadline')

    def test_idle_means_no_completed_work(self):
        self.assertEqual(decision(lease(), 1901, 1.1)['reason'], 'startup_idle')
        self.assertEqual(decision(lease(), 2500, 1.1,
            {'epoch':2500,'last_productive_epoch':1599,'last_activity_epoch':1599,
             'native_alive':True,'queued_work':True})['reason'], 'native_work_stalled_or_idle')
        self.assertIsNone(decision(lease(), 2500, 1.1,
            {'last_productive_epoch': 2490})['reason'])

    def test_long_iteration_with_active_cpu_is_not_idle(self):
        self.assertIsNone(decision(lease(),2500,1.1,{'epoch':2500,'last_productive_epoch':1000,
            'last_activity_epoch':2499,'native_alive':True,'queued_work':True})['reason'])

    def test_controller_loss_remains_bounded_even_if_last_native_work_active(self):
        self.assertEqual(decision(lease(),2500,1.1,{'epoch':2000,'last_productive_epoch':2000,
            'last_activity_epoch':2499,'native_alive':True,'queued_work':True})['reason'],'controller_lost')

    def test_completion_releases_early(self):
        self.assertEqual(decision(lease(), 1100, 1.1,
            {'finished': True, 'last_productive_epoch': 1100})['reason'], 'worker_finished')

    def test_provider_identity_and_no_gpu_route(self):
        pod = {'id': 'pod123', 'name': lease()['name'], 'cpuFlavorId': 'cpu3c',
               'vcpuCount': 32, 'networkVolumeId': 'testvolume', 'costPerHr': '.96'}
        self.assertEqual(verify_pod(pod, lease(), 'pod123'), .96)
        for changes in ({'id': 'other'}, {'vcpuCount': 64}, {'networkVolumeId': 'other'}):
            with self.assertRaises(ValueError):
                verify_pod(pod | changes, lease(), 'pod123')

    def test_api_mutation_is_bound_to_runtime_pod(self):
        provider = Provider('fake-test-key', 'pod123')
        response = unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value = b'{}'
        with patch('urllib.request.urlopen', return_value=response) as call:
            provider.call('DELETE')
            request = call.call_args.args[0]
            self.assertEqual(request.full_url, 'https://rest.runpod.io/v1/pods/pod123')
            self.assertEqual(request.method, 'DELETE')
        with self.assertRaises(ValueError):
            provider.call('POST')

    def test_bootstrap_template_contains_no_key_and_guard_precedes_install(self):
        with tempfile.TemporaryDirectory() as temporary:
            spec = {'schema': 'phase1-cloud-lease-preparation/v1', 'lease': lease(),
                'account_observed_epoch': 1000, 'quote_observed_epoch': 1000,
                'quoted_cpu_usd_hour': .96, 'observed_pods': 0,
                'autopay_enabled': False, 'public_ssh_key': 'ssh-ed25519 AAAATEST public'}
            result = prepare_lease(spec, Path(temporary) / 'prepared', now=1001)
            body = read(result['create_request_template']['path'])
            self.assertEqual(body['env']['RUNPOD_API_KEY'], KEY_PLACEHOLDER)
            boot = base64.b64decode(body['dockerEntrypoint'][2].split("'")[1]).decode()
            compile(boot, '<bootstrap>', 'exec')
            self.assertLess(boot.index('guard=subprocess.Popen'), boot.index("['apt-get','update']"))
            self.assertIn('resident guard remains active', boot)
            self.assertFalse(result['guard_armed'])

    def test_guard_deadline_requests_delete_then_verifies_absence(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = unittest.mock.Mock()
            pod = {'id':'pod123', 'name':lease()['name'], 'cpuFlavorId':'cpu3c',
                   'vcpuCount':32, 'networkVolumeId':'testvolume', 'costPerHr':'.96'}
            api.call.side_effect = [pod, {}, None]
            api.funds.return_value={'balance_usd':31.,'account_spend_usd_hour':1.15,'under_balance':False}
            with patch('lease_guard.time.time', side_effect=[15400,15405]), patch('lease_guard.time.sleep'):
                run_guard(lease(), temporary, 'pod123', api)
            self.assertEqual([call.args for call in api.call.call_args_list],
                             [('GET',), ('DELETE',), ('GET',)])
            self.assertTrue(read(Path(temporary)/'released.json')['provider_absent'])

    def test_idle_over_five_minutes_is_not_configurable(self):
        with self.assertRaises(ValueError):validate(lease()|{'work_idle_seconds':301})

    def test_funding_alert_and_unavailable_reads_block_new_dispatch(self):
        self.assertFalse(funding_status(lease(),1100,1.1,None)['allow_new_dispatch'])
        observation={'epoch':1100,'balance_usd':3.,'account_spend_usd_hour':1.15,'under_balance':False}
        row=funding_status(lease(),1100,1.1,observation)
        self.assertTrue(row['funds_verified']);self.assertFalse(row['allow_new_dispatch'])
        self.assertLess(row['funding_alert_horizon_hours'],6)
        self.assertFalse(funding_status(lease(),1200,1.1,observation)['funds_verified'])

    def test_account_query_is_readonly_and_credential_is_header_only(self):
        provider=Provider('fake-test-key','pod123')
        response=unittest.mock.MagicMock()
        response.__enter__.return_value.read.return_value=json.dumps({'data':{'myself':{
            'clientBalance':31,'currentSpendPerHr':1.15,'underBalance':False}}}).encode()
        with patch('urllib.request.urlopen',return_value=response) as call:
            result=provider.funds();request=call.call_args.args[0]
            self.assertEqual(request.full_url,'https://api.runpod.io/graphql')
            self.assertTrue(json.loads(request.data)['query'].startswith('query '))
            self.assertNotIn('fake-test-key',request.full_url)
            self.assertNotIn('fake-test-key',json.dumps(result))

    def test_periodic_account_failure_is_not_reported_as_fresh_funds(self):
        with tempfile.TemporaryDirectory() as temporary:
            api=unittest.mock.Mock()
            pod={'id':'pod123','name':lease()['name'],'cpuFlavorId':'cpu3c',
                 'vcpuCount':32,'networkVolumeId':'testvolume','costPerHr':'.96'}
            api.call.side_effect=[pod,pod,pod,None]
            api.funds.side_effect=[{'balance_usd':31.,'account_spend_usd_hour':1.15,
                                    'under_balance':False},RuntimeError('unavailable')]
            with patch('lease_guard.time.time',side_effect=[1000,1030,1060,1065]),patch('lease_guard.time.sleep'):
                run_guard(lease(),temporary,'pod123',api)
            self.assertEqual(api.funds.call_count,2)
            status=read(Path(temporary)/'guard.json')
            self.assertFalse(status['funds_verified']);self.assertFalse(status['allow_new_dispatch'])
            self.assertEqual(status['latched'],'funds_unverified')
            self.assertEqual(read(Path(temporary)/'stop-request.json')['reason'],'funds_unverified')


class PackagingTests(unittest.TestCase):
    def fixture(self, root):
        def data(name, value):
            path = root / name; write(path, value); return pin(path)
        run = data('source-run.json', {'source': 'original'})
        registry = data('registry.json', {'cards': ['test']})
        export = root / 'export'; export.mkdir()
        (export / 'parameters.f32le').write_bytes(b'fake parameter bytes')
        metadata = data('export/metadata.json', {'parameter_section_sha256': sha(export/'parameters.f32le'),
            'identity': {'loaded_run_sha256': run['sha256']}})
        imported = data('import.json', {'export_directory': str(export), 'source_run_path': run['path'],
            'source_registry_path': registry['path'], 'expected_metadata_sha256': metadata['sha256'],
            'expected_source_registry_sha256': registry['sha256']})
        checkpoint = data('checkpoint.json', {'model': 'preserve full state', 'adam_step': 483})
        model = {'play_import': imported, 'checkpoint': checkpoint,
                 'feature_transfer': {'digest': 'preserve'}}
        config = data('training.json', {'schema': 'mtg-kernel-native-expanded-training-run/v1',
            'initial_source': model, 'opponents': [{'id': 'reference', 'source': model}],
            'iterations': [{'episodes': [{'seed': 123, 'opponent': {'kind': 'current'}}]}],
            'learning_rate': .00001, 'value_coefficient': .5,
            'collection_workers': 4, 'update_backend': {'kind': 'cpu'}, 'output_directory': 'old'})
        binary = root / 'fake-elf'; binary.write_bytes(b'\x7fELFfake binary, never executed')
        source = root / 'source.tar'; source.write_bytes(b'fake source archive')
        toolchain = data('toolchain.json', {'source_commit': 'a'*40, 'rustc': '1.94.1'})
        return {'schema': 'phase1-cloud-package-spec/v1', 'source_commit': 'a'*40,
            'remote_root': '/opt/phase1/test', 'training_config': config,
            'binaries': {name: pin(binary) for name in
                ('native_expanded_training_run_v1', 'learned_sideboard_v1')},
            'source_archive': pin(source), 'toolchain': toolchain}

    def test_transitive_relocation_preserves_source_and_workers(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); spec = self.fixture(root)
            original = read(spec['training_config']['path'])
            result = prepare(spec, root / 'package')
            manifest = read(result['manifest']['path'])
            relocated = read(root / 'package/payload/config/training.json')
            self.assertEqual(relocated['collection_workers'], 4)
            self.assertEqual(relocated['iterations'], original['iterations'])
            self.assertEqual(relocated['initial_source']['checkpoint']['sha256'],
                             original['initial_source']['checkpoint']['sha256'])
            imported = relocated['initial_source']['play_import']
            self.assertNotEqual(imported['sha256'], original['initial_source']['play_import']['sha256'])
            relpath = imported['path'].removeprefix('/opt/phase1/test/')
            nested = read(root / 'package/payload' / relpath)
            for field in ('source_run_path', 'source_registry_path', 'export_directory'):
                self.assertTrue(nested[field].startswith('/opt/phase1/test/'))
            self.assertEqual(read(spec['training_config']['path']), original)
            self.assertEqual(len(list((root/'package/payload/imports').iterdir())), 1)
            self.assertFalse(result['engine_executed'])
            self.assertIn('same-seed final full-state parity', manifest['qualification_required'])

    def test_wrong_pin_and_gpu_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); spec = self.fixture(root)
            spec['binaries']['learned_sideboard_v1']['sha256'] = '0'*64
            with self.assertRaises(ValueError):
                prepare(spec, root/'package')

    def test_population_evaluation_relocation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); spec=self.fixture(root)
            config=read(spec['training_config']['path'])
            evaluation={'mode':'run_population_batch','model_sources':[config['initial_source']]*2,
                        'policies':[{'kind':'keep'}]*2,'matches':[{'seed':99}],
                        'output_directory':'old'}
            write(root/'evaluation.json',evaluation)
            spec['evaluation_configs']={'parity001':pin(root/'evaluation.json')}
            prepare(spec, root/'package')
            relocated=read(root/'package/payload/config/evaluation-parity001.json')
            self.assertEqual(relocated['matches'],evaluation['matches'])
            self.assertEqual(relocated['output_directory'],'/opt/phase1/test/evaluation/parity001')
            self.assertTrue(relocated['model_sources'][0]['play_import']['path'].startswith('/opt/phase1/'))

    def test_archive_traversal_rejected_before_extraction(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); archive = root/'bad.tar'
            with tarfile.open(archive, 'w') as handle:
                member = tarfile.TarInfo('../outside'); member.size=1
                handle.addfile(member, io.BytesIO(b'x'))
            with self.assertRaises(ValueError):
                stage(archive, sha(archive), root/'hot')
            self.assertFalse((root/'outside').exists())

    def test_recovery_excludes_uncommitted_iteration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); hot=root/'hot'; durable=root/'durable'
            write(hot/'run/run.json', {'config': 'same'})
            write(hot/'run/iterations/000000/update/checkpoint.json', {'adam_step':484})
            write(hot/'run/iterations/000000/complete.json', {'iteration':0})
            write(hot/'run/iterations/000001/update/checkpoint.json', {'adam_step':485})
            first=export_completed(hot,durable)
            self.assertEqual(first['completed_iterations'],1)
            self.assertFalse((durable/'run/iterations/000001').exists())
            second=export_completed(hot,durable)
            self.assertEqual(second['newly_copied_iteration_bytes'],0)

    def test_backlog_counts_unexported_completed_prefix_once(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);hot=root/'hot';durable=root/'durable'
            for index in range(3):
                write(hot/f'run/iterations/{index:06d}/complete.json',{'iteration':index})
                write(hot/f'run/iterations/{index:06d}/checkpoint.json',{'state':index})
            write(hot/'run/iterations/000003/partial.json',{'in_progress':True})
            before=recovery_backlog(hot,durable)
            self.assertEqual(before['pending_completed_iterations'],3)
            self.assertGreater(before['pending_completed_iteration_bytes'],0)
            export_completed(hot,durable)
            after=recovery_backlog(hot,durable)
            self.assertEqual(after['pending_completed_iterations'],0)
            self.assertEqual(after['hot_completed_iterations'],3)
            self.assertEqual(after['durable_completed_iterations'],3)
            self.assertEqual(after['pending_completed_iteration_bytes'],0)

    def test_backlog_cap_stops_native_before_or_after_blocking_copy(self):
        def row(count):return {'pending_completed_iterations':count,'pending_completed_iteration_bytes':count*100}
        for before,after in ((3,0),(1,3)):
            with self.subTest(before=before,after=after),patch('cloud_worker.recovery_backlog',side_effect=[row(before),row(after)]),\
                 patch('cloud_worker.terminate') as stop,patch('cloud_worker.export_completed') as export:
                child=object()
                def copied(*args):
                    self.assertEqual(stop.call_count,1 if before>2 else 0)
                    return {'completed_iterations':3}
                export.side_effect=copied
                result,status=export_with_backlog_limit('hot','durable',child,2)
                self.assertTrue(status['cap_exceeded']);stop.assert_called_once_with(child)
                self.assertEqual(result['completed_iterations'],3)

    def test_missing_stage_timings_are_unavailable(self):
        with tempfile.TemporaryDirectory() as temporary:
            row=receipt_timings(temporary,{'iteration':0})
            self.assertIsNone(row['update']['learner_update_seconds'])
            self.assertIsNone(row['collection']['collection_elapsed_seconds'])

    def test_native_stage_timings_are_preserved(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            write(root/'run/update.json',{'update_elapsed_seconds':2.5,'learner_update_seconds':1.2})
            write(root/'run/collection.json',{'collection_elapsed_seconds':4.1,
                'collection_initialization_seconds':.2})
            row=receipt_timings(root,{'iteration':0,'update':pin(root/'run/update.json'),
                'collection':pin(root/'run/collection.json')})
            self.assertEqual(row['update']['learner_update_seconds'],1.2)
            self.assertEqual(row['collection']['collection_initialization_seconds'],.2)
            self.assertIsNone(row['update']['behavior_replay_seconds'])

    def test_nonfinite_stage_timings_reject(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); (root/'run').mkdir()
            (root/'run/update.json').write_text('{"learner_update_seconds":1e999}')
            with self.assertRaises(ValueError):
                receipt_timings(root,{'update':pin(root/'run/update.json')})

    def test_recovery_failure_still_notifies_guard(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);guard=root/'guard';guard.mkdir()
            fake_fcntl=types.SimpleNamespace(LOCK_EX=1,LOCK_NB=2,flock=lambda *args:None)
            with patch.dict('sys.modules',{'fcntl':fake_fcntl}),patch('cloud_worker.run_locked',side_effect=OSError('recovery failure')):
                with self.assertRaises(OSError):
                    run_worker(root/'hot',root/'durable',guard,2,pod_id='testpod')
            notice=read(guard/'progress.json')
            self.assertTrue(notice['finished']);self.assertTrue(notice['recovery_failed'])
            self.assertFalse((guard/'recovery-complete.json').exists())


class ThroughputTests(unittest.TestCase):
    def refs(self,root):
        common={'schema':'phase1-fixed-work-training/v1','complete':True,'source_commit':'a'*40,
            'training_contract_sha256':'b'*64,'fixed_work_sha256':'c'*64,
            'initial_state_sha256':'d'*64,'final_state_sha256':'e'*64,
            'iteration_state_sha256s':['f'*64,'e'*64],'first_batch_episode_sha256s':['1'*64],
            'trajectory_semantics':ALGORITHM,'first_batch_semantic_sha256s':['2'*64],
            'completed_games':20,'completed_iterations':2,'max_iteration_elapsed_seconds':60}
        write(root/'local.json',common|{'elapsed_seconds':120,'collection_workers':4})
        write(root/'cloud.json',common|{'elapsed_seconds':60,'collection_workers':10})
        return pin(root/'local.json'),pin(root/'cloud.json')

    def test_nested_v2_parent_quota_is_effective(self):
        files={'/sys/fs/cgroup/tenant/job/cpu.max':'max 100000',
               '/sys/fs/cgroup/tenant/cpu.max':'200000 100000',
               '/sys/fs/cgroup/cpu.max':'400000 100000',
               '/sys/fs/cgroup/tenant/memory.max':'2147483648'}
        value=cgroup_limits('0::/tenant/job','12 3 0:20 / /sys/fs/cgroup rw - cgroup2 cgroup rw',files.__getitem__,range(32))
        self.assertEqual(value['eligible_capacity_cpus'],2)
        self.assertEqual(value['visible_memory_limit_bytes'],2147483648)
        self.assertIn('/sys/fs/cgroup/tenant/cpu.max',value['visible_cgroup_values'])

    def test_v1_quota_and_affinity_minimum(self):
        files={'/sys/fs/cgroup/cpu/jobs/cpu.cfs_quota_us':'300000',
               '/sys/fs/cgroup/cpu/jobs/cpu.cfs_period_us':'100000'}
        value=cgroup_limits('2:cpu,cpuacct:/jobs','12 3 0:20 / /sys/fs/cgroup/cpu rw - cgroup cgroup rw,cpu,cpuacct',files.__getitem__,range(2))
        self.assertEqual(value['eligible_capacity_cpus'],2)

    def test_qualification_requires_real_matched_state_not_passed_boolean(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            proof=qualify(local,cloud,timing_fixture(root,local,'local'),timing_fixture(root,cloud,'cloud'))
            self.assertTrue(proof['passed'])
            write(root/'proof.json',proof)
            manifest={'source_commit':'a'*40,'training_contract_sha256':'b'*64}
            self.assertEqual(production_reference(pin(root/'proof.json'),local,manifest,10)['completed_games'],20)
            changed=read(cloud['path']);changed['final_state_sha256']='0'*64
            write(root/'different.json',changed)
            with self.assertRaises(ValueError):qualify(local,pin(root/'different.json'))
            with self.assertRaises(ValueError):production_reference(pin(root/'proof.json'),local,manifest,4)

    def test_substantive_speedup_floor(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            changed=read(cloud['path']);changed['elapsed_seconds']=100
            write(root/'slower.json',changed)
            self.assertFalse(qualify(local,pin(root/'slower.json'))['passed'])

    def test_native_only_two_x_is_unqualified_without_lifecycle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            proof=qualify(local,cloud)
            self.assertEqual(proof['native_stage_speedup'],2)
            self.assertIsNone(proof['speedup']);self.assertFalse(proof['passed'])
            self.assertEqual(proof['timing_status'],'runtime_only_unqualified')
            write(root/'proof.json',proof)
            with self.assertRaises(ValueError):
                production_reference(pin(root/'proof.json'),local,
                    {'source_commit':'a'*40,'training_contract_sha256':'b'*64},10)

    def test_lifecycle_overhead_can_reverse_native_positive(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            proof=qualify(local,cloud,timing_fixture(root,local,'local',10),
                          timing_fixture(root,cloud,'cloud',60))
            self.assertEqual(proof['native_stage_speedup'],2)
            self.assertAlmostEqual(proof['speedup'],130/120)
            self.assertFalse(proof['passed'])

    def test_missing_overhead_or_release_and_wrong_binding_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            lp=timing_fixture(root,local,'local');cp=timing_fixture(root,cloud,'cloud')
            original=read(cp['path'])
            for name in TIMING_PHASES:
                value=copy.deepcopy(original);del value['phases'][name]
                write(root/'bad.json',value,replace=True)
                with self.subTest(phase=name),self.assertRaises(ValueError):
                    qualify(local,cloud,lp,pin(root/'bad.json'))
            value=copy.deepcopy(original);value['reference']=local
            write(root/'bad.json',value,replace=True)
            with self.assertRaises(ValueError):qualify(local,cloud,lp,pin(root/'bad.json'))
            value=copy.deepcopy(original)
            value['phases']['release_confirmation']={'not_applicable':True,'reason':'cannot observe',
                                                     'evidence':[pin(root/'cloud-timing-observation.json')]}
            write(root/'bad.json',value,replace=True)
            with self.assertRaises(ValueError):qualify(local,cloud,lp,pin(root/'bad.json'))

    def test_raw_relocation_hashes_are_evidence_semantics_must_still_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=self.refs(root)
            value=read(cloud['path']);value['first_batch_episode_sha256s']=['9'*64]
            write(root/'relocated.json',value)
            proof=qualify(local,pin(root/'relocated.json'))
            self.assertFalse(proof['raw_first_batch_hashes_equal'])
            value['first_batch_semantic_sha256s']=['8'*64]
            write(root/'different.json',value)
            with self.assertRaises(ValueError):qualify(local,pin(root/'different.json'))

    def test_qualification_reports_low_full_capacity_without_fake_100_percent(self):
        policy=ThroughputPolicy('qualification')
        policy.observe(0,0,32,1,0,True,'serial update')
        policy.observe(60,60,32,1,0,True,'serial update')
        result=policy.observe(120,120,32,1,0,True,'serial update')
        self.assertAlmostEqual(result['occupancy_window']['eligible_capacity_cpu_occupancy'],1/32)
        self.assertEqual(result['occupancy_window']['algorithmically_exploitable_cpu_occupancy'],1)
        self.assertIsNone(result['action']);self.assertIn('diagnosis',result)

    def test_production_stops_avoidable_underutilization(self):
        reference={'completed_games':20,'elapsed_seconds':120,'max_iteration_elapsed_seconds':60}
        policy=ThroughputPolicy('production',reference)
        policy.observe(0,0,8,8,0,True)
        policy.observe(60,60,8,8,20,True)
        result=policy.observe(120,120,8,8,40,True)
        self.assertEqual(result['action'],'stop_and_requalify_avoidable_underutilization')

    def test_production_sustained_speedup_uses_iteration_sized_windows(self):
        reference={'completed_games':20,'elapsed_seconds':600,'max_iteration_elapsed_seconds':300}
        policy=ThroughputPolicy('production',reference)
        policy.observe(0,0,1,1,0,True,'serial update')
        result=policy.observe(120,120,1,1,0,True,'serial update')
        self.assertNotIn('throughput_window',result)
        policy.observe(600,600,1,1,10,True)
        result=policy.observe(1200,1200,1,1,20,True)
        self.assertEqual(result['action'],'stop_and_release_sustained_sub_1_2x')


class NativeReferenceTests(unittest.TestCase):
    def fixture(self,root):
        binary=root/'trainer';binary.write_bytes(b'fixture, never executable')
        write(root/'initial.json',{'state_sha256':'a'*64,'adam_step':483})
        source={'checkpoint':pin(root/'initial.json'),'play_import':{'path':'unused','sha256':'b'*64}}
        cfg={'schema':'mtg-kernel-native-expanded-training-run/v1','initial_source':source,
             'opponents':[],'iterations':[{'episodes':[{'seed':10}]}],
             'learning_rate':.00001,'value_coefficient':.5,'output_directory':str(root/'native')}
        write(root/'config.json',cfg);write(root/'native/run.json',{'config':canonical_training_config(cfg)})
        base=root/'native/iterations/000000/attempt-000000'
        write(base/'collect/episode-0000.json',{'schema':'mtg-kernel-expanded-deck-trajectory/v2',
              'episode':{'seed':10,'opponent':source},'seat_behaviors':[{'source':source}]*2,
              'decisions':[],'fixture_only':True})
        write(base/'collect/collection.json',{'complete':True,
            'trajectories':[pin(base/'collect/episode-0000.json')],
            'collection_elapsed_seconds':1.})
        write(base/'update/checkpoint.json',{'state_sha256':'c'*64,'adam_step':484})
        write(base/'update/update.json',{'complete':True,'before_state_sha256':'a'*64,
            'after_state_sha256':'c'*64,'checkpoint':pin(base/'update/checkpoint.json'),
            'update_elapsed_seconds':.5})
        write(base.parent/'complete.json',{'iteration':0,'collection':pin(base/'collect/collection.json'),
            'update':pin(base/'update/update.json'),'output_identity':{'state_sha256':'c'*64}})
        write(root/'native/completion.json',{'complete':True,'completed_iterations':1,
            'iterations':[pin(base.parent/'complete.json')],'actual_identity':{'state_sha256':'c'*64}})
        write(root/'result.json',{'schema':'phase1-auxiliary-cpu-result/v1',
            'exit_code':0,'reason':'engine_exit','config':pin(root/'config.json'),
            'binary':pin(binary),'native_active_seconds':2.,'completed_iterations_before':0,'new_iterations':1})
        write(root/'build.json',{'source_commit':'d'*40,'binary':pin(binary)})
        return pin(root/'config.json'),pin(root/'result.json'),pin(root/'build.json')

    def test_reference_is_derived_from_completed_native_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args=self.fixture(root)
            result=build_reference(*args)
            self.assertEqual(result['initial_state_sha256'],'a'*64)
            self.assertEqual(result['final_state_sha256'],'c'*64)
            self.assertEqual(result['completed_games'],1)
            self.assertEqual(result['elapsed_seconds'],2)
            self.assertEqual(len(result['first_batch_episode_sha256s']),1)
            self.assertTrue(result['stage_instrumentation_complete'])

    def test_reference_rejects_modified_native_trajectory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args=self.fixture(root)
            (root/'native/iterations/000000/attempt-000000/collect/episode-0000.json').write_bytes(b'changed')
            with self.assertRaises(ValueError):build_reference(*args)

    def test_reference_rejects_unfinished_native_prefix(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args=self.fixture(root)
            path=root/'native/completion.json';value=read(path);value['complete']=False
            write(path,value,replace=True)
            with self.assertRaises(ValueError):build_reference(*args)

    def test_reference_rejects_noop_or_unrecorded_invocation_delta(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);args=self.fixture(root)
            for changes in ({'completed_iterations_before':1,'new_iterations':0},{'new_iterations':None}):
                value=read(root/'result.json')|changes
                write(root/'changed-result.json',value,replace=True)
                with self.assertRaises(ValueError):
                    build_reference(args[0],pin(root/'changed-result.json'),args[2])

    def test_native_default_omission_and_f32_normalization(self):
        value={'collection_workers':1,'update_backend':{'kind':'cpu'},
               'learning_rate':1e-5,'value_coefficient':.5}
        canonical=canonical_training_config(value)
        self.assertNotIn('collection_workers',canonical);self.assertNotIn('update_backend',canonical)
        self.assertEqual(canonical['learning_rate'],9.999999747378752e-6)


if __name__ == '__main__':
    unittest.main(verbosity=2)
