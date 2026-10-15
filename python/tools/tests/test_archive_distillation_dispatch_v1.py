"""Dispatcher guards and file-protocol tests; no MTG/model execution."""
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import archive_distillation_dispatch_v1 as dispatch


FAKE_NATIVE = r'''
import hashlib, json, pathlib, sys, time
p = pathlib.Path(sys.argv[1])
c = json.loads(p.read_text())
out = pathlib.Path(c['output_directory']); out.mkdir()
def pin(p): return {'path': str(p), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
def save(name, value):
    p = out / name; p.write_text(json.dumps(value)); return pin(p)
identity = {k:c[k] for k in ('lineage','arm','parent','games','bank','anchor')}
identity.update(runtime_commit='a'*40, runtime_tree='b'*64,
    runtime_binary_sha256=hashlib.sha256(pathlib.Path(sys.executable).read_bytes()).hexdigest(),
    recipe={k:c[k] for k in ('options','update_scope') if k in c})
identity['recipe']['checkpoints']=c.get('options',{}).get('checkpoint_steps',[64])
if c.get('dry_run'):
    result = dict(schema='mtg-kernel-archive-distillation-phase1/v1', dry_run=True, identity=identity)
else:
    steps = c.get('stop_after_steps') or 64
    h = ('1' if c['lineage']=='r1' else '2')*64
    telemetry = save('telemetry.json', [{'step':i,'loss':i} for i in range(1,steps+1)])
    checkpoints=[]
    if not c.get('stop_after_steps'):
        cp = save('checkpoint.json', {'state_sha256':h})
        source = save('source.json', {'checkpoint':cp})
        checkpoints=[dict(step=steps,checkpoint=cp,source=source,state_sha256=h,phase1_state_sha256=h)]
    result = dict(schema='mtg-kernel-archive-distillation-phase1/v1',complete=True,identity=identity,
        steps=steps,stop_after_steps=c.get('stop_after_steps'),checkpoints=checkpoints,telemetry=telemetry,
        state_hashes=[dict(step=i,phase1_state_sha256=h) for i in sorted({32,steps})])
save('phase1.json',result)
'''


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.artifacts = self.root / 'artifacts'; self.artifacts.mkdir()
        self.counter = 0
        self.cores = dispatch.slots.parse_cores(os.environ.get(dispatch.slots.CORES_ENV, '0,1'))[:2]
        hosts = patch.dict(dispatch.native.HOSTS, {'desktop': dispatch.slots.host()})
        hosts.start(); self.addCleanup(hosts.stop)
        self.script = self.root / 'fake_native.py'; self.script.write_text(FAKE_NATIVE)
        model = self.save({'state_sha256': '3'*64})
        trajectory = self.save({'parent':model})
        bank = self.save({'sources':[trajectory]})
        runtime = self.save({'engine_commit':'a'*40,'tracked_tree_sha256':'b'*64,
                             'binary':dispatch.pin(Path(sys.executable))})
        evidence = self.save({'kind':'unit-test-only placement evidence'})
        inventory = {host:{'eligible':host!='runpod','checked_at':datetime.now(timezone.utc).isoformat(),
                           'reason':'test fixture','evidence':evidence}
                     for host in ('desktop','computehost','runpod')}
        self.plan = {'schema':dispatch.PLAN,'runtime':runtime,'cpu_affinity':self.cores,
            'jobs':[], 'qualification_limits':{'cpu_seconds':100,'wall_seconds':100},
            'run_limits':{'cpu_seconds':1000,'wall_seconds':1000},
            'publication_budget':{'cpu_seconds':1,'wall_seconds':1,'bytes':1024**2},
            'storage':{'accounting_roots':[str(self.artifacts)],'max_physical_bytes':100*1024**2,
                       'projected_additional_bytes':10*1024**2,'reserve_bytes':dispatch.DISK_RESERVE_BYTES},
            'stop_files':[str(self.root/'STOP')], 'inventory':inventory,
            'allocation':{'mode':'formal','host_name':dispatch.slots.host(),'selected_host':'desktop','reason':'test comparison',
                'comparisons':{host:{'absolute_inputs_available':True,'reason':'test cost comparison',
                    'projected_seconds':20,'transfer_seconds':0,'evidence':evidence}
                    for host in ('desktop','computehost')}}}
        for lineage in ('r1','r2'):
            config = {'mode':'archive_distillation_phase1','lineage':lineage,'arm':'uniform',
                'parent':{'checkpoint':model,'play_import':model},'games':trajectory,'bank':bank,
                'anchor':{'training':[trajectory],'held_out':[trajectory]},
                'options':{'cell':'U0','checkpoint_steps':[64]},'update_scope':'scorer_only',
                'output_directory':str(self.root/'unused-original-output')}
            self.plan['jobs'].append({'id':lineage,'steps':64,'command':self.save(config)})

    def save(self, value):
        self.counter += 1
        path = self.root / f'input-{self.counter}.json'
        path.write_text(json.dumps(value))
        return dispatch.pin(path)

    def invoke(self, name, qualify, receipt=None, allocation_mode=None):
        plan = copy.deepcopy(self.plan)
        plan['allocation']['mode'] = allocation_mode or ('evidence_acquisition' if qualify else 'formal')
        plan_path = self.root/f'{name}-plan.json'; plan_path.write_text(json.dumps(plan))
        with patch.object(dispatch.shutil, 'disk_usage', return_value=SimpleNamespace(free=200*1024**3)), \
             patch.object(dispatch,'resource_context'), \
             patch.object(dispatch, 'native_command', side_effect=lambda runtime, command:
                          [sys.executable,str(self.script),str(command)]):
            return dispatch.execute(plan_path,self.artifacts/name,qualify,receipt)


class PureChecks(Fixture):
    def test_required_pin_mutation_is_detected_even_at_same_path_and_size(self):
        value = self.save({'value':11})
        parent = self.save({'child':value})
        dispatch.inputs([parent,value])
        Path(value['path']).write_text('{"value":12}')
        with self.assertRaisesRegex(ValueError,'changed pinned input'):
            dispatch.inputs([parent,value])

    def test_historical_provenance_is_identity_not_a_loader_dependency(self):
        missing={'path':str(self.root/'retired.json'),'sha256':'f'*64}
        original=dispatch.read(dispatch.checked(self.plan['jobs'][0]['command']))
        original['parent']['checkpoint']=self.save({'trajectories':[missing]})
        original['bank']=self.save({'sources':[missing]})
        self.plan['jobs'][0]['command']=self.save(original)
        with patch.object(dispatch.native,'host_runtime'):
            binding,_,_=dispatch.workload(self.plan)
        self.assertNotIn(str(self.root/'retired.json'),binding['inputs'])

    def test_explicit_formal_dose_disagreement_refuses_before_native_execution(self):
        self.plan['jobs'][0]['steps']=65
        with patch.object(dispatch.native,'host_runtime'), self.assertRaisesRegex(ValueError,'declared dose'):
            dispatch.workload(self.plan)

    def test_publication_and_reserve_are_mandatory(self):
        for mutate, reason in ((lambda p:p['publication_budget'].update(bytes=0),'publication byte'),
                               (lambda p:p['storage'].update(reserve_bytes=0),'60 GiB'),
                               (lambda p:p.update(stop_files=[]),'STOP')):
            plan=copy.deepcopy(self.plan); mutate(plan)
            with patch.object(dispatch,'WINDOWS',True), self.assertRaisesRegex(ValueError,reason):
                dispatch.validate_plan(plan)

    def test_eligible_other_host_is_a_comparison_not_an_automatic_refusal(self):
        dispatch.inventory(self.plan)
        self.plan['allocation']['comparisons']['computehost']['projected_seconds']=1
        with self.assertRaisesRegex(ValueError,'slower'):
            dispatch.inventory(self.plan)
        self.plan['allocation']['comparisons']['computehost']['absolute_inputs_available']=False
        dispatch.inventory(self.plan)
        del self.plan['allocation']['comparisons']['computehost']
        with self.assertRaises(KeyError):
            dispatch.inventory(self.plan)

    def test_evidence_acquisition_needs_inputs_not_preexisting_timing(self):
        self.plan['allocation']['mode']='evidence_acquisition'
        for row in self.plan['allocation']['comparisons'].values():
            row.pop('projected_seconds'); row.pop('transfer_seconds')
        dispatch.inventory(self.plan, qualifying=True)
        self.plan['allocation']['comparisons']['desktop']['absolute_inputs_available']=False
        with self.assertRaisesRegex(ValueError,'exact absolute inputs'):
            dispatch.inventory(self.plan, qualifying=True)

    def test_acquisition_still_requires_fresh_inventory_eligibility_and_host_identity(self):
        self.plan['allocation']['mode']='evidence_acquisition'
        self.plan['inventory']['desktop']['eligible']=False
        with self.assertRaisesRegex(ValueError,'ineligible'):
            dispatch.inventory(self.plan, qualifying=True)
        self.plan['inventory']['desktop']['eligible']=True
        self.plan['inventory']['runpod']['checked_at']='2000-01-01T00:00:00+00:00'
        with self.assertRaisesRegex(ValueError,'stale'):
            dispatch.inventory(self.plan, qualifying=True)
        self.plan['inventory']['runpod']['checked_at']=datetime.now(timezone.utc).isoformat()
        self.plan['allocation']['selected_host']='computehost'
        with patch.dict(dispatch.native.HOSTS, {'computehost':'OTHER-HOST'}):
            with self.assertRaisesRegex(ValueError,'local unpaid'):
                dispatch.inventory(self.plan, qualifying=True)

    def test_formal_budgets_can_follow_measurement_without_changing_prefix_binding(self):
        self.plan['run_limits']={'cpu_seconds':None,'wall_seconds':None}
        self.plan['publication_budget']={'cpu_seconds':None,'wall_seconds':None,'bytes':None}
        with patch.object(dispatch,'WINDOWS',True):
            dispatch.validate_plan(self.plan, qualifying=True)
            with self.assertRaisesRegex(ValueError,'invalid .*_seconds'):
                dispatch.validate_plan(self.plan)
        with patch.object(dispatch.native,'host_runtime'):
            binding,_,_=dispatch.workload(self.plan)
            self.plan['run_limits']={'cpu_seconds':1000,'wall_seconds':1000}
            self.plan['publication_budget']={'cpu_seconds':1,'wall_seconds':1,'bytes':1024}
            self.assertEqual(dispatch.workload(self.plan)[0],binding)

    def test_acquisition_plan_and_unmarked_receipt_do_not_authorize_formal_dispatch(self):
        self.plan['allocation']['mode']='evidence_acquisition'
        with self.assertRaisesRegex(ValueError,'allocation mode must be formal'):
            dispatch.inventory(self.plan)
        for purpose in (None,'formal-dispatch'):
            with self.assertRaisesRegex(ValueError,'incompatible'):
                dispatch.verify_qualification({'schema':dispatch.RECEIPT,'complete':True,
                    'purpose':purpose,'binding':{}}, {}, {}, [])

    def test_stale_inventory_refuses(self):
        self.plan['inventory']['desktop']['checked_at']='2000-01-01T00:00:00+00:00'
        with self.assertRaisesRegex(ValueError,'stale'):
            dispatch.inventory(self.plan)

    def test_resource_context_rejects_stale_claim_and_conflicting_reservation(self):
        claim={'state':'running','pid':123,'creation_time':456,'cores':self.cores,'background':False}
        with patch.dict(os.environ,{dispatch.slots.CLAIM_ENV:'f'*32,
                                    dispatch.slots.CORES_ENV:dispatch.slots.format_cores(self.cores)}), \
             patch.object(dispatch.slots,'read_json',return_value=claim), \
             patch.object(dispatch.slots,'process_state',return_value='alive'), \
             patch.object(dispatch.slots,'claim_conflicts',return_value=False):
            self.assertEqual(dispatch.resource_context(self.cores)['cores'],sorted(self.cores))
            with patch.object(dispatch.slots,'claim_conflicts',return_value=True):
                with self.assertRaisesRegex(ValueError,'reservation conflicts'):
                    dispatch.resource_context(self.cores)
            with patch.object(dispatch.slots,'process_state',return_value='absent'):
                with self.assertRaisesRegex(ValueError,'live matching'):
                    dispatch.resource_context(self.cores)

    def test_guard_accounts_retained_attempts_and_checks_cpu_wall_stop_reserve(self):
        guard=dispatch.Guard.__new__(dispatch.Guard)
        guard.plan=self.plan; guard.root=self.artifacts/'attempt'; guard.error=None
        guard.limits=self.plan['run_limits']; guard.sample=lambda:dict(seconds=1,cpu_seconds=1)
        with patch.object(dispatch,'resource_context'), patch.object(dispatch,'physical_tree',
                return_value={'charged_bytes':99*1024**2}), patch.object(dispatch.shutil,'disk_usage',
                return_value=SimpleNamespace(free=200*1024**3)):
            with self.assertRaisesRegex(ValueError,'physical output cap'):
                guard.check(admission=True)
            guard.sample=lambda:dict(seconds=1001,cpu_seconds=1)
            with self.assertRaisesRegex(ValueError,'wall limit'):
                guard.check()
            guard.sample=lambda:dict(seconds=1,cpu_seconds=1001)
            with self.assertRaisesRegex(ValueError,'CPU limit'):
                guard.check()
            guard.sample=lambda:dict(seconds=1,cpu_seconds=1)
            Path(self.plan['stop_files'][0]).touch()
            with self.assertRaisesRegex(ValueError,'STOP'):
                guard.check()
            Path(self.plan['stop_files'][0]).unlink()
            with patch.object(dispatch.shutil,'disk_usage',return_value=SimpleNamespace(free=1)):
                with self.assertRaisesRegex(ValueError,'60 GiB'):
                    guard.check()


@unittest.skipUnless(os.name=='nt','Windows production containment tests')
class FileProtocol(Fixture):
    def test_acquire_without_future_budgets_then_choose_formal_placement(self):
        self.plan['run_limits']={'cpu_seconds':None,'wall_seconds':None}
        self.plan['publication_budget']={'cpu_seconds':None,'wall_seconds':None,'bytes':None}
        for row in self.plan['allocation']['comparisons'].values():
            row.pop('projected_seconds'); row.pop('transfer_seconds')
        qualified=self.invoke('acquire',True)
        self.assertEqual(qualified['purpose'],dispatch.EVIDENCE)
        self.assertNotIn('projection',qualified)
        self.assertFalse((self.artifacts/'acquire'/'formal').exists())
        self.assertEqual(dispatch.read(dispatch.checked(qualified['receipt']))['purpose'],dispatch.EVIDENCE)
        self.plan['run_limits']={'cpu_seconds':1000,'wall_seconds':1000}
        self.plan['publication_budget']={'cpu_seconds':1,'wall_seconds':1,'bytes':1024**2}
        for row in self.plan['allocation']['comparisons'].values():
            row.update(projected_seconds=20,transfer_seconds=0)
        with self.assertRaisesRegex(ValueError,'allocation mode must be formal'):
            self.invoke('acquisition-is-not-formal',False,qualified['receipt'],allocation_mode='evidence_acquisition')
        self.assertFalse((self.artifacts/'acquisition-is-not-formal'/'formal').exists())
        self.assertTrue(self.invoke('formal-after-acquisition',False,qualified['receipt'])['complete'])

    def test_qualification_then_formal_preserves_recipe_and_reconciles_first32(self):
        # A deliberately loose publication extrapolation is recorded, not a
        # veto against actual declared limits which remain enforced live.
        self.plan['publication_budget'].update(cpu_seconds=2000,wall_seconds=2000)
        qualified=self.invoke('qualify',True)
        self.assertTrue(qualified['complete'])
        receipt=qualified['receipt']
        formal=self.invoke('formal',False,receipt)
        self.assertTrue(formal['complete'])
        self.assertIn(formal['selected_workers'],(1,2))
        self.assertGreater(formal['projection']['wall_seconds'],0)
        self.assertGreater(formal['projection']['wall_seconds'],self.plan['run_limits']['wall_seconds'])
        for job in formal['batch']['jobs']:
            command=dispatch.read(dispatch.checked(job['command']))
            original=dispatch.read(dispatch.checked(self.plan['jobs'][0 if job['id']=='r1' else 1]['command']))
            command.pop('output_directory'); original.pop('output_directory')
            self.assertEqual(command,original)
        data=dispatch.read(dispatch.checked(receipt))
        fastest=min(data['trials'],key=lambda t:t['prefix']['seconds'])['prefix']['workers']
        self.assertEqual(formal['selected_workers'],fastest)
        bad=copy.deepcopy(data); bad['binding']['steps'][0]=65
        configs=[dispatch.read(dispatch.checked(j['command'])) for j in self.plan['jobs']]
        for config in configs:
            config.pop('output_directory')
        runtime=dispatch.read(dispatch.checked(self.plan['runtime']))
        with self.assertRaisesRegex(ValueError,'declared formal dose'):
            dispatch.verify_qualification(bad,bad['binding'],runtime,configs)
        # Changed bound input cannot reuse the qualification, before any spawn.
        config=dispatch.read(dispatch.checked(self.plan['jobs'][0]['command']))
        config['options']['cell']='changed'; self.plan['jobs'][0]['command']=self.save(config)
        with self.assertRaisesRegex(ValueError,'incompatible'):
            self.invoke('changed',False,receipt)
        self.assertFalse((self.artifacts/'changed'/'formal').exists())
        self.assertFalse((self.root/'unused-original-output').exists())

    def test_missing_qualification_and_stop_refuse_before_spawn(self):
        with self.assertRaisesRegex(ValueError,'qualification required'):
            self.invoke('missing',False)
        self.assertFalse((self.artifacts/'missing'/'formal').exists())
        Path(self.plan['stop_files'][0]).touch()
        with self.assertRaisesRegex(ValueError,'STOP'):
            self.invoke('stop',True)
        self.assertFalse((self.artifacts/'stop').exists())

    def test_failure_retains_logs_and_stops_new_admission(self):
        self.script.write_text('raise SystemExit(7)')
        with self.assertRaisesRegex(ValueError,'native job failed'):
            self.invoke('failed',True)
        root=self.artifacts/'failed'
        self.assertFalse(dispatch.read(root/'result.json')['complete'])
        self.assertTrue((root/'startup-w1'/'r1'/'stderr.log').exists())
        self.assertFalse((root/'prefix-w1').exists())

    def test_root_exit_zero_with_lingering_descendant_is_not_success(self):
        self.script.write_text(FAKE_NATIVE+'\nimport subprocess\nsubprocess.Popen([sys.executable,"-c","import time; time.sleep(30)"])\n')
        with self.assertRaisesRegex(ValueError,'descendants remain'):
            self.invoke('lingering',True)
        result=dispatch.read(self.artifacts/'lingering'/'result.json')
        self.assertFalse(result['complete'])
        self.assertEqual(result['guard']['active_processes'],0)
        self.assertGreater(result['guard']['cpu_seconds'],0)

    def test_exited_descendant_cpu_is_retained_and_close_kills_remaining_tree(self):
        with patch.object(dispatch.shutil,'disk_usage',return_value=SimpleNamespace(free=200*1024**3)), \
             patch.object(dispatch,'resource_context'):
            guard=dispatch.Guard(self.plan,self.artifacts/'tree',True)
            try:
                # Parent exits; descendant CPU remains in the job's accumulated accounting.
                code='import subprocess,sys; p=subprocess.Popen([sys.executable,"-c","sum(range(300000)); import time; time.sleep(.1)"]); p.wait()'
                with (self.root/'child.log').open('w') as log:
                    child=guard.spawn([sys.executable,'-c',code],log,log)
                    child.wait(timeout=10)
                self.assertGreater(guard.sample()['cpu_seconds'],.001)
                until=time.monotonic()+2
                while guard.sample()['active_processes'] and time.monotonic()<until:
                    time.sleep(.01)  # Job accounting retires exit notifications asynchronously.
                self.assertEqual(guard.sample()['active_processes'],0)
                with (self.root/'sleep.log').open('w') as log:
                    sleeper=guard.spawn([sys.executable,'-c','import time; time.sleep(30)'],log,log)
                    guard.close()
                    self.assertIsNotNone(sleeper.wait(timeout=5))
            finally:
                if guard.tree.job is not None and not guard.done.is_set():
                    guard.close()


if __name__=='__main__':
    unittest.main()
