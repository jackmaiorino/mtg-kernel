"""Offline prepared-replay binding; synthetic bytes never execute native code."""
import copy
from pathlib import Path
import tempfile
import unittest
from common import pin, read, write
from cloud_worker import phase_state, receipt_timings
from pack import prepare
from parity_evidence import Artifacts, training_run, training_parity, resume_parity
from production_check import bind_reference, reference_run
from reference import build as build_reference
from throughput import (canonical_training_config, preparation_workers, training_contract, qualify,
    update_backward_execution, max_non_natural_episode_fraction, validate_preparation_runner,
    validate_preparation_update, validate_preparation_result, ThroughputPolicy)
from workload import compare, layouts
import test_cloud
from test_production_checks import Fixture


def execution(workers=4,jobs=3):
    return {'schema':'mtg-kernel-ordered-update-preparation/v1','requested_workers':workers,
            'started_workers':min(workers,jobs),'physical_group_jobs':jobs}


def make_prepared(run,workers=4):
    """Change only synthetic execution metadata, then repin the complete chain."""
    config=read(run['config']['path']);config['preparation_workers']=workers
    write(run['config']['path'],config,replace=True);run['config']=pin(run['config']['path'])
    spec=run['spec'];spec['config']=run['config']
    native={'preparation_backend':'native-cpu-ordered-physical-groups-v1',
            'preparation_workers_requested':workers}
    start=read(spec['run_start']['path']);start['config']=canonical_training_config(config);start.update(native)
    write(spec['run_start']['path'],start,replace=True);spec['run_start']=pin(spec['run_start']['path'])
    completion=read(spec['completion']['path']);completion.update(native)
    for index,receipt_pin in enumerate(completion['iterations']):
        receipt=read(receipt_pin['path']);update=read(receipt['update']['path'])
        update['update_preparation']=execution(workers,index+3)
        write(receipt['update']['path'],update,replace=True);receipt['update']=pin(receipt['update']['path'])
        write(receipt_pin['path'],receipt,replace=True);completion['iterations'][index]=pin(receipt_pin['path'])
    write(spec['completion']['path'],completion,replace=True);spec['completion']=pin(spec['completion']['path'])
    launch=read(run['launch']['path']);launch.update(config=run['config'],preparation_workers=workers,
        native_run_start=spec['run_start'],completion=spec['completion'])
    write(run['launch']['path'],launch,replace=True);run['launch']=pin(run['launch']['path'])


class PreparedConfigurationTests(unittest.TestCase):
    def test_default_and_explicit_one_have_same_exact_contract(self):
        config={'initial_source':{},'opponents':[],'iterations':[],'output_directory':'unused',
                'learning_rate':1e-5,'value_coefficient':.5}
        self.assertEqual(canonical_training_config(config),canonical_training_config(config|{'preparation_workers':1}))
        self.assertEqual(training_contract(config),training_contract(config|{'preparation_workers':1}))
        self.assertNotEqual(training_contract(config),training_contract(config|{'preparation_workers':4}))
        self.assertEqual(canonical_training_config(config|{'preparation_workers':32})['preparation_workers'],32)

    def test_backward_execution_default_and_explicit_sequential_have_same_exact_contract(self):
        """Mirrors `test_default_and_explicit_one_have_same_exact_contract` above
        for `update_backward_execution` (mtg-kernel `UpdateBackwardExecutionV1`):
        a contract computed with the explicit or implicit 'sequential' default
        is byte-identical; only 'fixed_partition_4' changes it, matching the
        Rust-side convention (`canonical_training_config`'s own docstring)."""
        config={'initial_source':{},'opponents':[],'iterations':[],'output_directory':'unused',
                'learning_rate':1e-5,'value_coefficient':.5}
        self.assertEqual(canonical_training_config(config),
                          canonical_training_config(config|{'update_backward_execution':'sequential'}))
        self.assertEqual(training_contract(config),
                          training_contract(config|{'update_backward_execution':'sequential'}))
        self.assertNotEqual(training_contract(config),
                             training_contract(config|{'update_backward_execution':'fixed_partition_4'}))
        self.assertEqual(canonical_training_config(
            config|{'update_backward_execution':'fixed_partition_4'})['update_backward_execution'],
            'fixed_partition_4')
        self.assertNotIn('update_backward_execution', canonical_training_config(config))

    def test_backward_execution_rejects_unknown_values(self):
        config={'learning_rate':1e-5,'value_coefficient':.5}
        for execution in ('Sequential','fixed_partition_1',4,None,''):
            with self.subTest(execution=execution), self.assertRaisesRegex(ValueError, 'update_backward_execution'):
                update_backward_execution(config|{'update_backward_execution':execution})

    def test_max_non_natural_episode_fraction_default_and_explicit_zero_have_same_exact_contract(self):
        """Mirrors `test_backward_execution_default_and_explicit_sequential_have_same_exact_contract`
        above for `max_non_natural_episode_fraction` (mtg-kernel
        `NativeExpandedTrainingRunV1.max_non_natural_episode_fraction`): a
        contract computed with the explicit or implicit 0.0 default is
        byte-identical; any positive fraction changes it, matching the
        Rust-side convention (`canonical_training_config`'s own docstring)."""
        config={'initial_source':{},'opponents':[],'iterations':[],'output_directory':'unused',
                'learning_rate':1e-5,'value_coefficient':.5}
        self.assertEqual(canonical_training_config(config),
                          canonical_training_config(config|{'max_non_natural_episode_fraction':0.0}))
        self.assertEqual(training_contract(config),
                          training_contract(config|{'max_non_natural_episode_fraction':0.0}))
        self.assertNotEqual(training_contract(config),
                             training_contract(config|{'max_non_natural_episode_fraction':0.2}))
        self.assertAlmostEqual(canonical_training_config(
            config|{'max_non_natural_episode_fraction':0.2})['max_non_natural_episode_fraction'],
            0.2,places=6)
        self.assertNotIn('max_non_natural_episode_fraction', canonical_training_config(config))
        # An integer 0 and a value that only rounds to 0.0 at f32 precision
        # both strip exactly like the float default (the Rust field is f32).
        self.assertNotIn('max_non_natural_episode_fraction',
                          canonical_training_config(config|{'max_non_natural_episode_fraction':0}))

    def test_max_non_natural_episode_fraction_rejects_invalid_values(self):
        config={'learning_rate':1e-5,'value_coefficient':.5}
        for fraction in (-0.1,1.0,1.5,float('nan'),float('inf'),float('-inf'),None,'0.2',True):
            with self.subTest(fraction=fraction), self.assertRaisesRegex(ValueError, 'max_non_natural_episode_fraction'):
                max_non_natural_episode_fraction(config|{'max_non_natural_episode_fraction':fraction})

    def test_invalid_counts_fail_before_default_normalization(self):
        config={'learning_rate':1e-5,'value_coefficient':.5}
        for workers in (0,33,-1,True,False,1.0,'4',None,float('nan')):
            with self.subTest(workers=workers),self.assertRaisesRegex(ValueError,'preparation worker count'):
                canonical_training_config(config|{'preparation_workers':workers})

    def test_native_runner_absence_and_exact_execution_mode(self):
        validate_preparation_runner({},{});validate_preparation_runner({'preparation_workers':1},{})
        row={'preparation_backend':'native-cpu-ordered-physical-groups-v1','preparation_workers_requested':4}
        validate_preparation_runner({'preparation_workers':4},row)
        for config,record in [({},row),({'preparation_workers':4},{}),({'preparation_workers':4},row|{'preparation_workers_requested':3}),
                              ({'preparation_workers':4},row|{'preparation_backend':'serial'}),
                              ({},{'preparation_workers_requested':None})]:
            with self.subTest(config=config,record=record),self.assertRaises(ValueError):
                validate_preparation_runner(config,record)

    def test_update_receipt_binds_requested_started_and_jobs(self):
        for jobs in (1,3,4,65536):
            self.assertEqual(validate_preparation_update({'preparation_workers':4},{'update_preparation':execution(4,jobs)})['started_workers'],min(4,jobs))
        for changes in ({'schema':'other'},{'requested_workers':3},{'requested_workers':True},
                        {'started_workers':4},{'started_workers':3.0},{'physical_group_jobs':0},
                        {'physical_group_jobs':65537},{'physical_group_jobs':True}):
            with self.subTest(changes=changes),self.assertRaises(ValueError):
                validate_preparation_update({'preparation_workers':4},{'update_preparation':execution()|changes})
        for config,row in [({}, {'update_preparation':None}),({}, {'update_preparation':execution()}),
                           ({'preparation_workers':4},{}),({'preparation_workers':4},{'update_preparation':None})]:
            with self.assertRaises(ValueError):validate_preparation_update(config,row)

    def test_supervisor_missing_is_only_serial_and_count_cannot_be_relabelled(self):
        validate_preparation_result({},{});validate_preparation_result({}, {'preparation_workers':1})
        validate_preparation_result({'preparation_workers':4},{'preparation_workers':4})
        for config,row in [({'preparation_workers':4},{}),({}, {'preparation_workers':4}),
                           ({'preparation_workers':4},{'preparation_workers':2})]:
            with self.assertRaises(ValueError):validate_preparation_result(config,row)

    def test_pack_preserves_preparation_count_and_exact_contract(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);spec=test_cloud.PackagingTests().fixture(root)
            config=read(spec['training_config']['path']);config['preparation_workers']=4
            write(spec['training_config']['path'],config,replace=True);spec['training_config']=pin(spec['training_config']['path'])
            result=prepare(spec,root/'package')
            self.assertEqual(read(root/'package/payload/config/training.json')['preparation_workers'],4)
            self.assertEqual(read(result['manifest']['path'])['training_contract_sha256'],training_contract(config))


class PreparedArtifactTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.root=Path(self.temporary.name);self.fixture=Fixture(self.root)

    def tearDown(self):self.temporary.cleanup()

    def test_longer_workload_requires_same_preparation_count(self):
        f=self.fixture
        self.assertEqual(compare(f.config,f.future|{'preparation_workers':1},4)['workload_class']['preparation_workers'],1)
        self.assertEqual(compare(f.config|{'preparation_workers':4},f.future|{'preparation_workers':4},4)['workload_class']['preparation_workers'],4)
        with self.assertRaises(ValueError):compare(f.config,f.future|{'preparation_workers':4},4)
        with self.assertRaises(ValueError):layouts(f.config|{'preparation_workers':33})

    def test_layouts_accepts_fixed_partition_backward_and_rejects_unknown_values(self):
        f=self.fixture
        self.assertTrue(layouts(f.config|{'update_backward_execution':'fixed_partition_4'}))
        self.assertTrue(layouts(f.config|{'update_backward_execution':'sequential'}))
        with self.assertRaises(ValueError):layouts(f.config|{'update_backward_execution':'bogus'})

    def test_layouts_accepts_tolerant_collection_fraction_and_rejects_invalid_values(self):
        f=self.fixture
        self.assertTrue(layouts(f.config|{'max_non_natural_episode_fraction':0.2}))
        self.assertTrue(layouts(f.config|{'max_non_natural_episode_fraction':0.0}))
        with self.assertRaises(ValueError):layouts(f.config|{'max_non_natural_episode_fraction':1.0})
        with self.assertRaises(ValueError):layouts(f.config|{'max_non_natural_episode_fraction':'bogus'})

    def test_old_serial_proof_omission_normalizes_without_enabling_production(self):
        f=self.fixture;proof=read(f.proof['path']);proof.pop('preparation_workers')
        write(f.proof['path'],proof,replace=True);f.manifest['qualification']=pin(f.proof['path'])
        result=f.run_check()
        self.assertTrue(result['computational_prerequisites_satisfied'])
        self.assertFalse(result['production_eligible']);self.assertFalse(result['live_production_enabled'])

    def test_reference_consumes_complete_prepared_execution_chain(self):
        f=self.fixture;make_prepared(f.local)
        reference=build_reference(f.local['config'],f.local['launch'],f.build)
        self.assertEqual(reference['preparation_workers'],4)
        self.assertEqual([row['started_workers'] for row in reference['update_preparation_executions']],[3,4])
        actual=training_run(Artifacts(),f.local['spec']);bind_reference(reference,actual)
        self.assertEqual(actual['preparation_workers'],4)
        with self.assertRaises(ValueError):bind_reference(reference|{'preparation_workers':1},actual)
        reference_run(Artifacts(),reference)

    def test_exact_parity_does_not_waive_preparation_count_difference(self):
        f=self.fixture;make_prepared(f.local)
        with self.assertRaisesRegex(ValueError,'exact_training_contract'):
            training_parity(Artifacts(),f.local['spec'],f.cloud['spec'])
        make_prepared(f.cloud)
        left,right=training_parity(Artifacts(),f.local['spec'],f.cloud['spec'])
        self.assertEqual(left['full_state_payload_sha256s'],right['full_state_payload_sha256s'])

    def test_prepared_run_rejects_missing_or_relabelled_update_metadata(self):
        f=self.fixture;make_prepared(f.local)
        completion=read(f.local['spec']['completion']['path']);entry=completion['iterations'][0]
        receipt=read(entry['path']);update=read(receipt['update']['path']);update.pop('update_preparation')
        write(receipt['update']['path'],update,replace=True);receipt['update']=pin(receipt['update']['path'])
        write(entry['path'],receipt,replace=True);completion['iterations'][0]=pin(entry['path'])
        write(f.local['spec']['completion']['path'],completion,replace=True)
        f.local['spec']['completion']=pin(f.local['spec']['completion']['path'])
        with self.assertRaisesRegex(ValueError,'telemetry unavailable'):training_run(Artifacts(),f.local['spec'])
        with self.assertRaisesRegex(ValueError,'telemetry unavailable'):build_reference(f.local['config'],f.local['launch'],f.build)

    def test_recovery_checks_failed_and_successful_invocation_preparation_counts(self):
        f=self.fixture;make_prepared(f.local);make_prepared(f.resumed)
        old_failed=f.recovery['resumed']['launch_results'][0]
        failed=read(old_failed['path']);failed['config']=f.resumed['config'];failed['preparation_workers']=4
        write(old_failed['path'],failed,replace=True);failed_pin=pin(old_failed['path'])
        snapshot=read(f.recovery['interruption']['path']);snapshot['launch']=failed_pin
        write(f.recovery['interruption']['path'],snapshot,replace=True)
        recovery={'uninterrupted':f.local['spec'],'resumed':f.resumed['spec']|{
            'launch_results':[failed_pin,f.resumed['launch']]},'interruption':pin(f.recovery['interruption']['path'])}
        result=resume_parity(Artifacts(),recovery);self.assertEqual(result['preparation_workers'],4)
        failed.pop('preparation_workers');write(old_failed['path'],failed,replace=True)
        recovery['resumed']['launch_results'][0]=pin(old_failed['path'])
        with self.assertRaisesRegex(ValueError,'supervisor preparation worker'):resume_parity(Artifacts(),recovery)


class PreparedTelemetryTests(unittest.TestCase):
    def test_unfinished_update_never_claims_serial_or_known_parallel_stage(self):
        with tempfile.TemporaryDirectory() as temporary:
            hot=Path(temporary);attempt=hot/'run/iterations/000000/attempt-000000';attempt.mkdir(parents=True)
            config={'iterations':[{'episodes':[{}]*10}],'preparation_workers':4}
            write(attempt/'update-command.json',{'mode':'update_prepared','preparation_workers':4})
            for completed_timing in (False,True):
                if completed_timing:write(attempt/'update/update.json',{'update_preparation':execution(),'update_elapsed_seconds':10})
                phase=phase_state(hot,config,8)
                self.assertEqual(phase['subphase'],'prepared_replay_pending_or_unknown')
                self.assertIsNone(phase['algorithmically_exploitable_cpus']);self.assertIsNone(phase['sequential_reason'])
                self.assertEqual(phase['configured_preparation_workers'],4)

    def test_unknown_exploitability_retains_full_capacity_occupancy_only(self):
        policy=ThroughputPolicy('qualification')
        policy.observe(0,0,8,None,0,True)
        first=policy.observe(60,60,8,None,0,True)
        final=policy.observe(120,120,8,None,0,True)
        for row in (first,final):
            self.assertEqual(row['occupancy_window']['eligible_capacity_cpu_occupancy'],.125)
            self.assertIsNone(row['occupancy_window']['algorithmically_exploitable_cpu_occupancy'])
            self.assertIsNone(row['occupancy_window']['avoidable'])
            self.assertIsNone(row['action'])
        self.assertIn('unknown_update_subphase',final['disposition'])

    def test_completed_preparation_timings_do_not_replace_missing_values(self):
        with tempfile.TemporaryDirectory() as temporary:
            hot=Path(temporary);path=hot/'run/iterations/000000/attempt-000000/update/update.json'
            row=execution()|{'parallel_replay_seconds':.75,'worker_timings':[{'worker':0,'completed_groups':3,'busy_wall_seconds':.7}]}
            write(path,{'update_preparation':row})
            result=receipt_timings(hot,{'iteration':0,'update':pin(path)})
            self.assertEqual(result['update_preparation'],row)
            self.assertIsNone(result['update']['behavior_replay_seconds'])
            row['parallel_replay_seconds']=-1;write(path,{'update_preparation':row},replace=True)
            with self.assertRaisesRegex(ValueError,'preparation timing'):receipt_timings(hot,{'update':pin(path)})

    def test_qualification_rejects_relabelled_preparation_count_even_with_same_contract(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);local,cloud=test_cloud.ThroughputTests().refs(root)
            row=read(cloud['path']);row['preparation_workers']=4;write(cloud['path'],row,replace=True)
            with self.assertRaisesRegex(ValueError,'preparation worker'):qualify(local,pin(cloud['path']))


if __name__=='__main__':unittest.main(verbosity=2)
