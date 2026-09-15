"""Offline owned-process fixtures and cloud failure-path mocks. No execution."""
import copy
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch
from common import pin, read, write
import cloud_worker
import runtime_observation
from runtime_observation import ENVIRONMENT, IMAGE_REFERENCE, LIBRARIES, controlled_environment, image_contract, sample, capture
from runtime_compatibility import runtime as consume_runtime
from parity_evidence import Artifacts, source_build
from test_production_checks import Fixture


class ProcessFixture:
    def __init__(self,root):
        self.root=Path(root);self.records={};self.pid=77;self.ticks=12345;self.missing=False
        self.binary=self.file(self.root/'trainer','synthetic binary')
        self.image={'schema':'phase1-runtime-image-pins-v1','platform_reference':IMAGE_REFERENCE,'runtime_files':[]}
        for name in LIBRARIES:
            virtual='/usr/lib/x86_64-linux-gnu/'+name
            record=self.file(self.root/name,'synthetic '+name)
            self.records.pop(record['path'])
            record={**record,'path':virtual};self.records[virtual]=record
            self.image['runtime_files'].append({'resolved_path':virtual,'sha256':record['sha256'],'bytes':record['bytes']})
        self.maps='\n'.join('1000-2000 r-xp 0000 08:01 '+str(record['inode'])+' '+path
            for path,record in self.records.items())
    def file(self,path,contents):
        path.write_text(contents);value=pin(path)|{'inode':len(self.records)+1,'device_major':8,'device_minor':1}
        self.records[str(path)]=value;return value
    def text(self,pid,name):
        if self.missing:raise FileNotFoundError('mock process exited')
        return self.maps if name=='maps' else '77 (fake task) '+' '.join(['0']*19+[str(self.ticks)])
    def executable(self,pid):return self.binary['path']
    def record(self,path):return self.records[path]
    def host(self):return {'cpu_model':'synthetic CPU','cpu_features':['sse2'],'kernel_release':'synthetic kernel'}
    def sampler(self):
        system=types.SimpleNamespace(text=self.text,executable=self.executable,file=self.record,host=self.host)
        return sample(types.SimpleNamespace(pid=self.pid),self.binary,self.image,ENVIRONMENT,
            {'affinity':[0,1],'quota':{'capacity_basis':'synthetic fixture','hidden_ancestors':'unknown'}},system)


class RuntimeObservationTests(unittest.TestCase):
    def setUp(self):self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name);self.fixture=ProcessFixture(self.root)
    def tearDown(self):self.temp.cleanup()

    def test_only_whitelisted_environment_reaches_native_or_receipts(self):
        value=controlled_environment({'RUNPOD_API_KEY':'never-copy-this','PRIVATE_OTHER_KEY':'also-secret','PATH':'old'})
        self.assertEqual(value,ENVIRONMENT);self.assertNotIn('never-copy-this',str(value))
        for key in ('LD_PRELOAD','LD_HWCAP_MASK','LD_ANY_NEW_OPTION','GLIBC_TUNABLES'):
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'override'):
                controlled_environment({key:'secret override value'})
        self.assertEqual(controlled_environment({'LD_PRELOAD':''}),ENVIRONMENT)

    def test_sample_binds_actual_executable_all_four_libraries_and_live_context(self):
        value=self.fixture.sampler()
        self.assertEqual(value['executable']['sha256'],self.fixture.binary['sha256'])
        self.assertEqual(set(value['libraries']),set(LIBRARIES));self.assertEqual(value['proc_starttime_ticks'],12345)
        self.assertEqual(value['cpu_features'],['sse2']);self.assertEqual(value['quota_context']['affinity'],[0,1])
        self.assertEqual(value['maps'],self.fixture.maps)

    def test_missing_or_exited_process_is_unverified_not_a_pre_run_substitute(self):
        self.fixture.maps='\n'.join(line for line in self.fixture.maps.splitlines() if 'libgcc_s' not in line)
        self.assertIsNone(self.fixture.sampler())
        self.fixture.missing=True;self.assertIsNone(self.fixture.sampler())

    def test_wrong_library_bytes_inode_and_deleted_mapping_reject(self):
        path='/usr/lib/x86_64-linux-gnu/libm.so.6';original=copy.deepcopy(self.fixture.records[path])
        for field,value in (('sha256','f'*64),('inode',999)):
            self.fixture.records[path]=original|{field:value}
            with self.subTest(field=field),self.assertRaisesRegex(ValueError,'differ'):
                self.fixture.sampler()
        self.fixture.records[path]=original;self.fixture.maps=self.fixture.maps.replace(path,path+' (deleted)')
        with self.assertRaisesRegex(ValueError,'deleted'):self.fixture.sampler()

    def test_changed_pid_starttime_rejects_reused_process_identity(self):
        original=self.fixture.text;reads=0
        def text(pid,name):
            nonlocal reads
            if name=='stat':
                reads+=1
                if reads>1:self.fixture.ticks+=1
            return original(pid,name)
        self.fixture.text=text
        with self.assertRaisesRegex(ValueError,'identity changed'):self.fixture.sampler()

    def test_runtime_image_requires_libgcc_and_the_accepted_digest(self):
        value=copy.deepcopy(self.fixture.image);value['runtime_files'].pop()
        with self.assertRaisesRegex(ValueError,'libgcc'):image_contract(value)
        value=copy.deepcopy(self.fixture.image);value['platform_reference']='docker.io/example@sha256:'+'a'*64
        with self.assertRaisesRegex(ValueError,'accepted common'):image_contract(value)

    def test_capture_retains_original_mapped_bytes_for_post_release_verification(self):
        source=pin(self.root/'trainer');value={'executable':source,'libraries':{'synthetic':source},'maps':'raw'}
        with patch.object(runtime_observation,'sync_directory') as synced:
            saved=capture(value,self.root/'durable')
        destination=saved['captured_files'][source['path']]
        self.assertEqual(Path(destination['path']).read_bytes(),(self.root/'trainer').read_bytes())
        self.assertEqual(pin(destination['path'])['sha256'],source['sha256']);self.assertEqual(saved['maps'],'raw')
        expected=[Path(destination['path']).parent,Path(destination['path']).parent.parent,self.root/'durable']
        self.assertEqual([call.args[0] for call in synced.call_args_list],expected*2)

    def test_directory_fsync_failure_prevents_verified_capture(self):
        source=pin(self.root/'trainer');value={'executable':source,'libraries':{}}
        with patch.object(runtime_observation,'sync_directory',side_effect=OSError('mock directory fsync failed')):
            with self.assertRaisesRegex(OSError,'directory fsync'):capture(value,self.root/'durable')

    def test_directory_descriptor_is_closed_even_when_fsync_fails(self):
        with patch.object(runtime_observation.os,'O_DIRECTORY',65536,create=True), \
             patch.object(runtime_observation.os,'open',return_value=42) as opened, \
             patch.object(runtime_observation.os,'fsync',side_effect=OSError('mock fsync failed')) as synced, \
             patch.object(runtime_observation.os,'close') as closed:
            with self.assertRaises(OSError):runtime_observation.sync_directory('/owned/runtime-files')
        opened.assert_called_once();synced.assert_called_once_with(42);closed.assert_called_once_with(42)


class WorkerRuntimeFailureTests(unittest.TestCase):
    def exercise(self,observation_failure):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);hot=root/'hot';hot.mkdir();guard=root/'guard';guard.mkdir();durable=root/'durable'
            fixture=ProcessFixture(hot)
            image_path=hot/'runtime-image.json';write(image_path,fixture.image)
            config_path=hot/'training.json';write(config_path,{'update_backend':{'kind':'cpu'},'collection_workers':1})
            manifest={'remote_root':str(hot),'files':{},'runtime_image_pins':pin(image_path),
                'training_config':pin(config_path),'binaries':{'native_expanded_training_run_v1':fixture.binary},'source_commit':'a'*40}
            write(hot/'manifest.json',manifest)
            write(guard/'guard.json',{'pod_id':'test-pod','provider_verified':True,'provider_ok':True,
                'funds_verified':True,'allow_new_dispatch':True,'latched':False,'epoch':1000.,'pid':123})
            child=types.SimpleNamespace(pid=77,returncode=None if observation_failure else 0)
            child.poll=lambda:child.returncode
            fake_os=types.SimpleNamespace(name='posix',environ={'RUNPOD_API_KEY':'secret'},kill=lambda *a:None,
                                          fsync=os.fsync,replace=os.replace)
            actual_path=Path
            def path(value):return root if str(value)=='/proc' else actual_path(value)
            backlog={'pending_completed_iterations':0,'pending_completed_iteration_bytes':0}
            with patch.object(cloud_worker,'os',fake_os),patch.object(cloud_worker,'Path',path), \
                 patch.object(cloud_worker.time,'time',return_value=1000.), \
                 patch.object(cloud_worker.shutil,'disk_usage',return_value=types.SimpleNamespace(free=100*1024**3)), \
                 patch.object(cloud_worker.subprocess,'Popen',return_value=child) as spawn, \
                 patch.object(cloud_worker,'terminate',side_effect=lambda c:setattr(c,'returncode',-15 if c.returncode is None else c.returncode)), \
                 patch.object(cloud_worker,'telemetry',return_value={'affinity':[0,1],'quota':{'eligible_capacity_cpus':2}}), \
                 patch.object(cloud_worker,'runtime_sample',side_effect=ValueError('mock mapping mismatch')), \
                 patch.object(cloud_worker,'recovery_backlog',return_value=backlog), \
                 patch.object(cloud_worker,'export_completed',return_value={'completed_iterations':0}) as exported:
                if observation_failure:
                    with self.assertRaisesRegex(ValueError,'mapping mismatch'):
                        cloud_worker.run_locked(hot,durable,guard,1,pod_id='test-pod')
                else:cloud_worker.run_locked(hot,durable,guard,1,pod_id='test-pod')
                self.assertEqual(spawn.call_args.kwargs['env'],ENVIRONMENT)
                exported.assert_called_once()
            result=read(next((durable/'attempts').glob('*/worker-result.json')))
            self.assertFalse(result['runtime_verified']);self.assertIsNone(result['runtime_observation'])
            self.assertEqual(result['reason'],'worker_exception' if observation_failure else 'runtime_unverified')
            self.assertTrue(read(guard/'progress.json')['finished']);self.assertTrue((guard/'recovery-complete.json').is_file())

    def test_early_exit_without_sample_exports_and_finishes_lease(self):self.exercise(False)
    def test_mapping_failure_terminates_exports_and_finishes_lease(self):self.exercise(True)


class RuntimeConsumerTests(unittest.TestCase):
    def setUp(self):self.temp=tempfile.TemporaryDirectory();self.fixture=Fixture(self.temp.name)
    def tearDown(self):self.temp.cleanup()
    def consume(self,reference):
        f=self.fixture
        return consume_runtime(Artifacts(),reference,source_build(Artifacts(),f.build),
            [f.cloud['launch']],[f.cloud['spec']['run_start'],f.bo3['right']['run_start']],'cloud')

    def test_aggregate_label_cannot_replace_actual_sampled_invocations(self):
        f=self.fixture;value=read(f.runtime['cloud']['path']);value['schema']='phase1-native-runtime/v1'
        with self.assertRaisesRegex(ValueError,'actual sampled'):
            self.consume(f.document('legacy-label.json',value))

    def test_missing_invocation_sample_and_tampered_libgcc_are_rejected(self):
        f=self.fixture;bundle=read(f.runtime['cloud']['path']);row_pin=bundle['invocations'][-1];row=read(row_pin['path'])
        sample_pin=row['runtime_observation'];value=read(sample_pin['path'])
        value['libraries']['libgcc_s.so.1']['sha256']='f'*64
        row['runtime_observation']=f.document('bad-libgcc-sample.json',value)
        bundle['invocations'][-1]=f.document('bad-libgcc-invocation.json',row)
        with self.assertRaisesRegex(ValueError,'mapped library differs'):
            self.consume(f.document('bad-libgcc-bundle.json',bundle))
        row['runtime_observation']=None;bundle['invocations'][-1]=f.document('missing-sample-invocation.json',row)
        with self.assertRaisesRegex(ValueError,'sampled runtime'):
            self.consume(f.document('missing-sample-bundle.json',bundle))

    def test_legacy_actual_mapping_sample_preserves_missing_quota_as_unavailable(self):
        f=self.fixture;bundle=read(f.runtime['local']['path'])
        for i,reference in enumerate(bundle['invocations']):
            row=read(reference['path']);value=read(row['runtime_observation']['path'])
            value['schema']='phase1-sampled-linux-runtime/v1';value.pop('quota_context');value.pop('cpu_model');value.pop('cpu_features')
            row['runtime_observation']=f.document(f'legacy-sample-{i}.json',value)
            bundle['invocations'][i]=f.document(f'legacy-invocation-{i}.json',row)
        result=consume_runtime(Artifacts(),f.document('legacy-actual-bundle.json',bundle),source_build(Artifacts(),f.build),
            [],[f.local['spec']['run_start'],f.bo3['left']['run_start']],'local')
        self.assertIsNone(result['cpu_model'])
        self.assertTrue(all(row['quota_context_source']=='unavailable' for row in result['observations']))


if __name__=='__main__':unittest.main(verbosity=2)
