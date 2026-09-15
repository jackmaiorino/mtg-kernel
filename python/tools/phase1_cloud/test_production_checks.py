"""Offline artifact-consumption tests. Synthetic artifacts are not run evidence.

The optional real regression reads frozen Windows checkpoint/recovery artifacts;
it never invokes a native executable or writes into their evidence directory.
"""
import copy
from pathlib import Path
import tempfile
import unittest
from common import pin, read, write
from parity_evidence import Artifacts, checkpoint_state, source_build, training_parity, resume_parity, bo3_parity
from production_check import check
from reference import build as build_reference
from runtime_compatibility import compare as runtime_compare
from throughput import canonical_training_config, qualify, training_contract
from test_cloud import timing_fixture
from workload import compare as workload_compare
from runtime_observation import ENVIRONMENT, IMAGE_REFERENCE, LIBRARIES


class Fixture:
    """Small schema-faithful fake files for testing the reader, never execution."""
    def __init__(self,root):
        self.root=Path(root)
        self.binary=self.raw('trainer','synthetic trainer, not executable')
        self.evaluator=self.raw('evaluator','synthetic evaluator, not executable')
        self.build=self.document('build.json',{'source_commit':'a'*40,
            'binaries':{'native_expanded_training_run_v1':self.binary,'learned_sideboard_v1':self.evaluator},
            'source_archive':self.raw('source.tar','synthetic source'),
            'toolchain':self.document('toolchain.json',{'synthetic_test_only':True})})
        self.play_import=self.document('import.json',{'synthetic_test_only':True})
        self.initial=self.document('initial.json',self.state(483,[],0))
        self.source={'play_import':self.play_import,'checkpoint':self.initial,'feature_transfer':True}
        self.deck={'label':'synthetic registered deck','mainboard':['Mountain']*60,
                   'sideboard':['Mountain']*15}
        self.config={'schema':'mtg-kernel-native-expanded-training-run/v1',
            'initial_source':self.source,'opponents':[{'id':'frozen','source':self.source}],
            'iterations':[],'learning_rate':1e-5,'value_coefficient':.5,
            'collection_workers':4,'output_directory':str(self.root/'unused')}
        for iteration in range(2):
            self.config['iterations'].append({'episodes':[{'episode':{
                'id':f'fixed-{iteration}-{index}','seed':1000+10*iteration+index,
                'registered':[self.deck,self.deck],'selected':[self.deck,self.deck],
                'postboard':False,'starting_player':index%2,'learner_seat':index%2,
                'max_physical_decisions':100,'max_policy_steps':100},
                'opponent':{'kind':'current'} if index%2 else {'kind':'fixed','id':'frozen'}}
                for index in range(10)]})
        self.local=self.training('local',100)
        self.cloud=self.training('cloud',20,cloud=True)
        self.resumed=self.training('resumed',20,first_attempt=1)
        first=self.launch('interrupted.json',self.resumed['config'],1,None,0,False)
        final=self.resumed['launch']
        partial=self.raw('resumed/iterations/000000/attempt-000000/collect/.episode-0002.json.stage',
                         'synthetic incomplete collection')
        interrupted=self.document('interruption.json',{'launch':first,'partial_files':[partial]})
        self.recovery={'uninterrupted':self.local['spec'],
            'resumed':{**self.resumed['spec'],'launch_results':[first,final]},'interruption':interrupted}
        self.bo3={'left':self.match('local-bo3'),'right':self.match('cloud-bo3')}
        self.runtime={'local':self.runtime_record('local-runtime.json','local',
            [self.local['launch'],first,final],[self.local['spec']['run_start'],self.bo3['left']['run_start'],
                                                self.resumed['spec']['run_start']]),
            'cloud':self.runtime_record('cloud-runtime.json','cloud',[self.cloud['launch']],
                [self.cloud['spec']['run_start'],self.bo3['right']['run_start']])}
        self.recovery['interruption']=self.document('interruption.json',{'launch':first,'partial_files':[partial]})
        self.local_ref=self.document('local-reference.json',build_reference(self.local['config'],
            self.local['launch'],self.build))
        self.cloud_ref=self.document('cloud-reference.json',build_reference(self.cloud['config'],
            self.cloud['launch'],self.build))
        local_timing=timing_fixture(self.root,self.local_ref,'local',20)
        cloud_timing=timing_fixture(self.root,self.cloud_ref,'cloud',20)
        self.proof=self.document('qualification.json',qualify(self.local_ref,self.cloud_ref,local_timing,cloud_timing))
        future=copy.deepcopy(self.config);future['iterations']*=2
        # Each copied iteration must own separate episode dictionaries.
        future['iterations']=copy.deepcopy(future['iterations'])
        for i in range(4):
            future['iterations'][i]=copy.deepcopy(future['iterations'][i])
            for j,item in enumerate(future['iterations'][i]['episodes']):
                item['episode']['id']=f'future-{i}-{j}';item['episode']['seed']=2000+10*i+j
        future['output_directory']=str(self.root/'future')
        self.future=future
        self.manifest={'schema':'phase1-production-engineering-run/v1','qualification':self.proof,
            'production_config':self.document('future.json',future),'production_source_build':self.build,
            'max_updates':4,'bo3_parity':self.bo3,'resume_parity':self.recovery,'runtime_parity':self.runtime}

    def raw(self,name,value):
        path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True)
        if not path.exists():path.write_text(value,encoding='utf8')
        return pin(path)

    def document(self,name,value):
        path=self.root/name;write(path,value,replace=True);return pin(path)

    def state(self,adam,trajectories,index):
        return {'schema':'mtg-kernel-expanded-deck-checkpoint/v1','feature_contract_digest':'d'*64,
            'feature_encoding_digest':'e'*64,'card_db_hash':'f'*64,'source_import':{'identity':'synthetic'},
            'state_sha256':str(index)*64,'adam_step':adam,'scorer_bias_anchor_bits':0,
            'parameters':[index,2,3],'first_moments':[index,0,0],'second_moments':[index,0,0],
            'trajectories':trajectories,'loss_identity':'synthetic exact loss identity',
            'learning_rate_bits':925353388,'value_coefficient_bits':1056964608}

    def launch(self,name,config,exit_code,new,before=0,cloud=False,elapsed=20):
        return self.document(name,{'schema':'phase1-cloud-worker-result/v1' if cloud else 'phase1-auxiliary-cpu-result/v1',
            'pod_id':'synthetic-pod' if cloud else None,'config':config,'binary':self.binary,
            'exit_code':exit_code,'reason':'engine_exit' if exit_code==0 else 'injected_collection_interrupt',
            'new_iterations':new,'completed_iterations_before':before,'native_execution_elapsed_seconds':elapsed,
            'synthetic_test_only':True})

    def training(self,name,elapsed,cloud=False,first_attempt=0):
        config=copy.deepcopy(self.config);config['output_directory']=str(self.root/name)
        config_pin=self.document(name+'-config.json',config)
        start=self.document(name+'/run.json',{'schema':'mtg-kernel-native-expanded-run-start/v1',
            'config':canonical_training_config(config),'git_commit':'a'*40,'device':'cpu','gpu_ordinal':None,
            'collection_workers_requested':4,'implementation_sha256':'b'*64,
            'feature_contract_digest':'d'*64,'feature_encoding_digest':'e'*64})
        state='0'*64;source=copy.deepcopy(self.source);completes=[]
        for i,iteration in enumerate(config['iterations']):
            attempt=first_attempt if i==0 else 0
            prefix=f'{name}/iterations/{i:06d}/attempt-{attempt:06d}'
            trajectories=[]
            for j,item in enumerate(iteration['episodes']):
                episode=copy.deepcopy(item['episode']);seat=episode['learner_seat']
                opponent=source if item['opponent']=={'kind':'current'} else self.source
                episode['opponent']=copy.deepcopy(opponent)
                behaviors=[{'source':None,'identity':{}} for _ in range(2)]
                behaviors[seat]['source']=source;behaviors[1-seat]['source']=opponent
                for behavior in behaviors:
                    checkpoint=read(behavior['source']['checkpoint']['path'])
                    behavior['identity']={'checkpoint_sha256':behavior['source']['checkpoint']['sha256'],
                        'state_sha256':checkpoint['state_sha256'],'adam_step':checkpoint['adam_step']}
                trajectories.append(self.document(prefix+f'/collect/episode-{j:04d}.json',{
                    'schema':'mtg-kernel-expanded-deck-trajectory/v2','episode':episode,
                    'behavior_state_sha256':state,'seat_behaviors':behaviors,
                    'decisions':[{'logits':[.125,.75],'selected':1,'value':.25,'visible':'synthetic'}],
                    'terminal':{'winner':0}}))
            collection=self.document(prefix+'/collect/collection.json',{
                'schema':'mtg-kernel-expanded-deck-collection/v1','complete':True,
                'behavior_state_sha256':state,'trajectories':trajectories,'collection_elapsed_seconds':1.})
            saved=self.state(484+i,trajectories,i+1)
            checkpoint=self.document(prefix+'/update/checkpoint.json',saved)
            update=self.document(prefix+'/update/update.json',{
                'schema':'mtg-kernel-expanded-deck-update/v1','complete':True,'before_state_sha256':state,
                'after_state_sha256':saved['state_sha256'],'checkpoint':checkpoint,'update_elapsed_seconds':1.})
            completes.append(self.document(f'{name}/iterations/{i:06d}/complete.json',{
                'schema':'mtg-kernel-native-expanded-iteration/v1','iteration':i,'source':source,
                'collection':collection,'update':update,'output_identity':{
                    'state_sha256':saved['state_sha256']}}))
            state=saved['state_sha256'];source=copy.deepcopy(self.source);source['checkpoint']=checkpoint
        completion=self.document(name+'/completion.json',{'complete':True,'completed_iterations':2,
            'planned_iterations':2,'iterations':completes,'device':'cpu','gpu_ordinal':None,
            'actual_identity':{'state_sha256':state,'adam_step':485},'source':source})
        launch=self.launch(name+'-launch.json',config_pin,0,2,cloud=cloud,elapsed=elapsed)
        return {'config':config_pin,'launch':launch,'spec':{'config':config_pin,'source_build':self.build,
            'run_start':start,'completion':completion}}

    def match(self,name):
        match_config={'deck_ids':['synthetic','synthetic'],'seed':7,'game_one_chooser':0,
            'max_physical_games':3,'max_physical_decisions':100,'max_policy_steps':100}
        config={'mode':'run_population_batch','model_sources':[self.source,self.source],
            'policies':[{'kind':'keep'},{'kind':'keep'}],'output_directory':str(self.root/name),
            'matches':[{'config':match_config,'registered':[self.deck,self.deck]}]}
        cp=self.document(name+'-config.json',config)
        models=[{'checkpoint_sha256':self.initial['sha256'],'state_sha256':'0'*64}]*2
        policies=[{'kind':'keep'}]*2
        start=self.document(name+'/run-start.json',{'schema':'kernel-population-sideboard-cli-start/v1',
            'command':config,'build':{'git_commit':'a'*40},'binary':self.evaluator,
            'execution':{'device':'cpu','gpu_ordinal':None},'play_models':models,
            'sideboard_policy_identities':policies,'inputs':[cp,self.play_import,self.initial],
            'compiled_sources':{'all':'synthetic-identical'},'tag_file_sha256':'a'*64})
        completion=self.document(name+'/completion.json',{'mode':'run_population_batch',
            'completed_matches':1,'physical_games':2,'play_models':models,
            'sideboard_policy_identities':policies,'inputs':[cp,self.play_import,self.initial]})
        full=self.document(name+'/match-000000.json',{'schema':'kernel_population_bo3/v1',
            'config':match_config,'explicit_registrations':[self.deck,self.deck],
            'play_models':models,'sideboard_policy_identities':policies,'outcome':{'winner':{'winner':0}},
            'games':[{'start':{'game_index':i,'chooser':0,'choice':'play','starting_player':0},
                      'environment_seed':7+i,'mainboard_sha256':['a'*64,'a'*64],'winner':0} for i in (1,2)],
            'sideboard_decisions':[{'acting_player':i,'input':{'visible_scalar':.125},
                'initial_mainboard':self.deck['mainboard'],'initial_sideboard':self.deck['sideboard'],
                'selected_actions':['keep'],'selected_mainboard_sha256':'a'*64} for i in (0,1)]})
        return {'config':cp,'source_build':self.build,'run_start':start,'completion':completion,'matches':[full]}

    def runtime_record(self,name,environment,invocations,runs):
        captured={};libraries={};descriptors=[]
        for library in LIBRARIES:
            original='/usr/lib/x86_64-linux-gnu/'+library
            file=self.raw(library,'synthetic '+library)
            captured[original]=file;libraries[library]={**file,'path':original}
            descriptors.append({'resolved_path':original,'sha256':file['sha256'],'bytes':file['bytes']})
        image=self.document('runtime-image.json',{'schema':'phase1-runtime-image-pins-v1',
            'platform_reference':IMAGE_REFERENCE,'runtime_files':descriptors})
        invocations=list(invocations)
        for run in runs:
            if Path(run['path']).name=='run-start.json':
                native=read(run['path'])
                invocation=self.launch(environment+'-bo3-launch.json',native['inputs'][0],0,None,cloud=environment=='cloud')
                row=read(invocation['path']);row['binary']=self.evaluator
                write(invocation['path'],row,replace=True);invocation=pin(invocation['path']);invocations.append(invocation)
        for index,reference in enumerate(invocations):
            row=read(reference['path']);config=read(row['config']['path'])
            native=Path(config['output_directory'])/('run-start.json' if config.get('mode') else 'run.json')
            row['native_run_start']=pin(native) if row['exit_code']==0 else None
            row['pid']=100+index;row['runtime_image_pins']=image
            binary=row['binary'];sample_captured={**captured,binary['path']:binary}
            paths=[binary['path'],*[item['path'] for item in libraries.values()]]
            sample={'schema':'phase1-sampled-linux-runtime/v2','pid':row['pid'],'proc_starttime_ticks':1234,
                'executable':binary,'libraries':libraries,'captured_files':sample_captured,
                'maps':'\n'.join('1000-2000 r-xp 0000 08:01 7 '+path for path in paths),
                'image_reference':IMAGE_REFERENCE,'environment':ENVIRONMENT,
                'cpu_model':'synthetic '+environment,'cpu_features':['sse2'],
                'kernel_release':'synthetic kernel '+environment,
                'quota_context':{'affinity':[0,1,2,3],'quota':{'capacity_basis':'synthetic unit fixture'}}}
            row['runtime_observation']=self.document(environment+f'-runtime-{index}.json',sample)
            write(reference['path'],row,replace=True)
            updated=pin(reference['path']);reference.clear();reference.update(updated)
        return self.document(name,{'schema':'phase1-native-runtime/v2','environment':environment,
            'runtime_image_pins':image,'pod_id':'synthetic-pod' if environment=='cloud' else None,
            'invocations':invocations})

    def run_check(self):return check(self.document('run-manifest.json',self.manifest))

    def alter_last_trajectory(self,spec,mutate):
        """Update every descendant pin, so rejection tests semantic equality."""
        completion=read(spec['completion']['path']);receipt=read(completion['iterations'][-1]['path'])
        collection=read(receipt['collection']['path']);trajectory=read(collection['trajectories'][0]['path'])
        mutate(trajectory);write(collection['trajectories'][0]['path'],trajectory,replace=True)
        collection['trajectories'][0]=pin(collection['trajectories'][0]['path'])
        write(receipt['collection']['path'],collection,replace=True);receipt['collection']=pin(receipt['collection']['path'])
        update=read(receipt['update']['path']);checkpoint=read(update['checkpoint']['path'])
        checkpoint['trajectories']=collection['trajectories'];write(update['checkpoint']['path'],checkpoint,replace=True)
        update['checkpoint']=pin(update['checkpoint']['path']);write(receipt['update']['path'],update,replace=True)
        receipt['update']=pin(receipt['update']['path']);write(completion['iterations'][-1]['path'],receipt,replace=True)
        completion['iterations'][-1]=pin(completion['iterations'][-1]['path'])
        completion['source']['checkpoint']=update['checkpoint'];write(spec['completion']['path'],completion,replace=True)
        spec['completion']=pin(spec['completion']['path'])


class ProductionChecks(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory();self.fixture=Fixture(self.temporary.name)
    def tearDown(self):self.temporary.cleanup()

    def test_actual_artifact_chain_binds_longer_work_but_never_enables_live_production(self):
        result=self.fixture.run_check()
        self.assertTrue(result['computational_prerequisites_satisfied'])
        self.assertFalse(result['production_eligible']);self.assertFalse(result['live_production_enabled'])
        self.assertEqual(result['workload_compatibility']['planned_fresh_games'],40)
        self.assertFalse(result['actual_runtime_compatibility']['hardware_observations_identical'])
        self.assertGreater(result['verified_artifact_count'],50)

    def test_fixed_work_stays_exact_while_explicit_class_supports_longer_fresh_schedule(self):
        f=self.fixture
        self.assertNotEqual(training_contract(f.config),training_contract(f.future))
        self.assertTrue(workload_compare(f.config,f.future,4)['compatible'])
        changes=[('optimizer',lambda c:c.__setitem__('learning_rate',2e-5)),
            ('worker',lambda c:c.__setitem__('collection_workers',10)),
            ('layout',lambda c:c['iterations'][0]['episodes'][0]['episode'].__setitem__('max_policy_steps',101)),
            ('old_seed',lambda c:c['iterations'][0]['episodes'][0]['episode'].__setitem__('seed',1000)),
            ('repeated_seed',lambda c:c['iterations'][0]['episodes'][1]['episode'].__setitem__('seed',2000)),
            ('batch',lambda c:c['iterations'][0]['episodes'].pop()),
            ('source',lambda c:c['initial_source'].__setitem__('feature_transfer',False))]
        for name,mutate in changes:
            value=copy.deepcopy(f.future);mutate(value)
            with self.subTest(name=name),self.assertRaises(ValueError):workload_compare(f.config,value,4)
        with self.assertRaisesRegex(ValueError,'exceeds'):workload_compare(f.config,f.future,3)

    def test_checkpoint_array_difference_is_not_hidden_by_declared_state_hash(self):
        f=self.fixture;original=read(f.initial['path']);changed=copy.deepcopy(original)
        changed['first_moments'][0]=.125
        a,digest_a=checkpoint_state(Artifacts(),f.initial)
        b,digest_b=checkpoint_state(Artifacts(),f.document('altered-state.json',changed))
        self.assertEqual(a['state_sha256'],b['state_sha256']);self.assertNotEqual(digest_a,digest_b)

    def test_changed_checkpoint_arrays_rejected_even_when_every_file_pin_is_updated(self):
        f=self.fixture;spec=f.cloud['spec'];completion=read(spec['completion']['path'])
        complete_pin=completion['iterations'][-1];receipt=read(complete_pin['path'])
        update=read(receipt['update']['path']);state=read(update['checkpoint']['path'])
        state['second_moments'][0]+=.125
        write(update['checkpoint']['path'],state,replace=True);update['checkpoint']=pin(update['checkpoint']['path'])
        write(receipt['update']['path'],update,replace=True);receipt['update']=pin(receipt['update']['path'])
        write(complete_pin['path'],receipt,replace=True);completion['iterations'][-1]=pin(complete_pin['path'])
        completion['source']['checkpoint']=update['checkpoint']
        write(spec['completion']['path'],completion,replace=True);spec['completion']=pin(spec['completion']['path'])
        with self.assertRaisesRegex(ValueError,'full_state_payload'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_full_bo3_numerical_and_decision_records_are_compared(self):
        f=self.fixture;right=f.bo3['right'];reference=right['matches'][0];value=read(reference['path'])
        value['sideboard_decisions'][0]['input']['visible_scalar']=.126
        right['matches'][0]=f.document('cloud-bo3/match-000000.json',value)
        with self.assertRaisesRegex(ValueError,'full_match_semantic'):
            bo3_parity(Artifacts(),f.bo3)

    def test_later_trajectory_numeric_change_rejected_with_all_file_pins_updated(self):
        f=self.fixture;spec=f.cloud['spec']
        f.alter_last_trajectory(spec,lambda t:t['decisions'][0]['logits'].__setitem__(0,.126))
        with self.assertRaisesRegex(ValueError,'all_trajectory_semantic'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_later_unrelated_hash_field_is_not_stripped(self):
        f=self.fixture;spec=f.cloud['spec']
        f.alter_last_trajectory(spec,lambda t:t['decisions'][0].__setitem__('unrelated_path','/changed/path'))
        with self.assertRaisesRegex(ValueError,'all_trajectory_semantic'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_generated_checkpoint_identity_requires_actual_sha_and_adam_cursor(self):
        f=self.fixture;spec=f.cloud['spec']
        f.alter_last_trajectory(spec,lambda t:t['seat_behaviors'][0]['identity'].__setitem__('adam_step',99))
        with self.assertRaisesRegex(ValueError,'state or Adam cursor'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_generated_checkpoint_reference_cannot_point_to_an_unconfigured_copy(self):
        f=self.fixture;spec=f.cloud['spec']
        def mutate(t):t['seat_behaviors'][0]['source']['checkpoint']['path']='/unconfigured/copy.json'
        f.alter_last_trajectory(spec,mutate)
        with self.assertRaisesRegex(ValueError,'current configured policies'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_generated_identity_checkpoint_hash_must_match_the_rehashed_file(self):
        f=self.fixture;spec=f.cloud['spec']
        f.alter_last_trajectory(spec,lambda t:t['seat_behaviors'][0]['identity'].__setitem__('checkpoint_sha256','f'*64))
        with self.assertRaisesRegex(ValueError,'bind its exact checkpoint file'):
            training_parity(Artifacts(),f.local['spec'],spec)

    def test_boolean_parity_summaries_do_not_substitute_for_native_files(self):
        f=self.fixture
        for field in ('bo3_parity','resume_parity','runtime_parity'):
            old=f.manifest[field];f.manifest[field]={'passed':True}
            with self.subTest(field=field),self.assertRaises((KeyError,ValueError)):f.run_check()
            f.manifest[field]=old

    def test_hash_changes_are_rejected_before_reported_success_is_used(self):
        f=self.fixture;path=Path(f.proof['path']);value=read(path);value['passed']=False;write(path,value,replace=True)
        with self.assertRaisesRegex(ValueError,'SHA256 differs'):f.run_check()

    def test_clean_reopen_or_noop_cannot_masquerade_as_interrupted_recovery(self):
        f=self.fixture;spec=copy.deepcopy(f.recovery)
        spec.pop('interruption')
        with self.assertRaisesRegex(ValueError,'actual interrupted'):resume_parity(Artifacts(),spec)
        spec=copy.deepcopy(f.recovery);launch=read(spec['resumed']['launch_results'][-1]['path'])
        launch['new_iterations']=0;spec['resumed']['launch_results'][-1]=f.document('noop.json',launch)
        with self.assertRaisesRegex(ValueError,'no measured work'):resume_parity(Artifacts(),spec)

    def test_partial_snapshot_must_bind_failed_invocation_not_successful_reopen(self):
        f=self.fixture;spec=copy.deepcopy(f.recovery);snapshot=read(spec['interruption']['path'])
        snapshot['launch']=spec['resumed']['launch_results'][-1]
        spec['interruption']=f.document('misbound-interruption.json',snapshot)
        with self.assertRaisesRegex(ValueError,'actual failed invocation'):resume_parity(Artifacts(),spec)

    def test_library_image_and_execution_binding_mismatches_rejected(self):
        f=self.fixture;original=read(f.runtime['cloud']['path'])
        changes=[('image',lambda c:c.__setitem__('image_digest','sha256:'+'b'*64)),
            ('libm',lambda c:c.__setitem__('libm',f.raw('changed-libm.so','different libm'))),
            ('invocation',lambda c:c.__setitem__('invocations',[f.local['launch']])),
            ('native_start',lambda c:c.__setitem__('native_runs',[f.local['spec']['run_start']])),
            ('missing_cpu',lambda c:c.__setitem__('cpu_features',[])),
            ('pod',lambda c:c.__setitem__('pod_id','unrelated-pod'))]
        for name,mutate in changes:
            value=copy.deepcopy(original);mutate(value)
            f.manifest['runtime_parity']['cloud']=f.document('changed-runtime.json',value)
            with self.subTest(name=name),self.assertRaises(ValueError):f.run_check()

    def test_native_only_reference_cannot_create_a_complete_work_qualification(self):
        f=self.fixture;proof=qualify(f.local_ref,f.cloud_ref)
        self.assertFalse(proof['passed']);self.assertIsNone(proof['speedup'])
        f.manifest['qualification']=f.document('native-only-proof.json',proof)
        with self.assertRaisesRegex(ValueError,'complete-work qualification'):f.run_check()

    def test_explicit_file_mapping_preserves_pinned_contents_and_rejects_traversal(self):
        f=self.fixture;reference={'path':'/captured/image/trainer','sha256':f.binary['sha256']}
        reader=Artifacts([{'from':'/captured/image','to':str(f.root)}])
        self.assertEqual(reader.local_pin(reference)['sha256'],f.binary['sha256'])
        with self.assertRaisesRegex(ValueError,'traversal'):reader.resolve('/captured/image/../outside')
        with self.assertRaisesRegex(ValueError,'SHA256 differs'):
            reader.local_pin({**reference,'sha256':'f'*64})


REAL=Path('E:/mtg-kernel-learned-sideboarding-evidence/bo3-post480-preparation-001/phase1-training-qualification-001')

@unittest.skipUnless((REAL/'crash4-interrupted-001.json').is_file(),'frozen real recovery artifacts unavailable')
class RealRecovery(unittest.TestCase):
    def test_real_windows_partial_attempt_and_full_adam_states_are_consumed_read_only(self):
        def spec(name):
            return {'config':pin(REAL/(name+'.json')),'source_build':pin(REAL/'build-cpu-001/manifest.json'),
                'run_start':pin(REAL/(name+'-run/run.json')),'completion':pin(REAL/(name+'-run/completion.json'))}
        resumed=spec('crash4');resumed['launch_results']=[pin(REAL/'crash4-launch-001/result.json'),
                                                       pin(REAL/'crash4-launch-002/result.json')]
        value=resume_parity(Artifacts(),{'uninterrupted':spec('parallel4'),'resumed':resumed,
            'interruption':pin(REAL/'crash4-interrupted-001.json')})
        self.assertTrue(value['passed']);self.assertEqual(value['observed_interruption_phases'],['collect'])
        self.assertEqual(value['verified_partial_files'],2);self.assertEqual(len(value['full_state_payload_sha256s']),2)
        self.assertEqual([len(batch) for batch in value['all_trajectory_semantic_sha256s']],[10,10])
        self.assertEqual([len(batch) for batch in value['resumed_trajectory_raw_pins']],[10,10])
        fields={field for row in value['resumed_normalized_fields'] for episode in row['episodes'] for field in episode['fields']}
        self.assertIn('seat_behaviors[0].identity.checkpoint_sha256',fields)
        self.assertIn('episode.opponent.checkpoint.path',fields)


if __name__=='__main__':unittest.main(verbosity=2)
