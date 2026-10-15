"""Pinned schedule and real Python-child guard checks. No MTG/model execution."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import archive_distillation_dispatch_v1 as bounded
import nine_deck_baseline_v1 as ndb
import nine_deck_public_panel_v1 as panel


FAKE = r'''
import ctypes, hashlib, json, pathlib, sys
p = pathlib.Path(sys.argv[1]); request = json.loads(p.read_text())
out = pathlib.Path(request['output_directory']); out.mkdir()
def save(name, value):
    p = out/name; p.write_text(json.dumps(value, sort_keys=True)); return hashlib.sha256(p.read_bytes()).hexdigest()
save('start.json', {'schema':'public-input-evaluation-start/v1', 'command':request})
mask, system = ctypes.c_size_t(), ctypes.c_size_t()
api=ctypes.WinDLL('kernel32',use_last_error=True)
api.GetCurrentProcess.restype=ctypes.c_void_p
api.GetProcessAffinityMask.argtypes=[ctypes.c_void_p,ctypes.POINTER(ctypes.c_size_t),ctypes.POINTER(ctypes.c_size_t)]
assert api.GetProcessAffinityMask(api.GetCurrentProcess(),ctypes.byref(mask),ctypes.byref(system))
hashes=[]
for i, match in enumerate(request['matches']):
    if match['config']['seed'] == 999999:
        save('failure.json', {'error':'A versus B, match seed 999999, game 1, environment seed 123: InvalidReference'})
        sys.exit(1)
    winner = None if match['config']['seed'] % 3 == 0 else match['config']['seed'] % 2
    record = {'schema':'public-input-evaluation-game-one/v1', 'game_one_only':True,
        'match':match, 'games':[{'start':{'starting_player':match['config']['game_one_chooser']}, 'winner':winner}],
        'sideboard_decisions':[], 'affinity':mask.value}
    hashes.append(save(f'match-{i:06}.json', record))
save('completion.json', {'game_one_only':True, 'match_sha256':hashes, 'matches':len(hashes), 'natural_games':len(hashes)})
'''


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.decks = ndb.load_decks(Path(__file__).resolve().parents[3]/'data/runtime_decks_v1.json')
        checkpoint = {'schema':'fixture', 'source_import':{}, 'feature_contract_digest':'contract',
                      'feature_encoding_digest':'encoding', 'card_db_hash':'cards', 'parameters':[]}
        self.candidate = {'checkpoint':self.save('checkpoint.json', checkpoint),
                          'play_import':self.save('import.json', {'fixture':True})}
        self.opponent = {'kind':'legacy', 'source':self.candidate, 'v3_forced_actions':True,
                         'v3_spell_target_reference_adapter':True}
        cases=[]
        for own in range(9):
            for other in range(9):
                for family in range(6):
                    for seat in (0,1):
                        for reference in ('T1','H'):
                            cases.append(dict(own=own, other=other, family=family, seat=seat,
                                reference=reference, seed=1+own*1000+other*100+family,
                                starting_player=(family+own+other)%2))
        self.document = dict(decks=list(ndb.DECK_NAMES), cases=cases, cases_per_endpoint_lineage=1944,
                             lineages=['r1','r2'], endpoints=['parent','candidate'])
        self.schedule = self.save('schedule.json', self.document)
        self.metadata = dict(lineage='r1',endpoint='parent',reference='T1')
        self.qual_cases = self.save('qualification.json', dict(decks=list(ndb.DECK_NAMES), cases=[
            dict(own=d,other=d,family=0,seat=seat,reference=ref,seed=100000+d,starting_player=d%2)
            for d in range(9) for seat in (0,1) for ref in ('T1','H')]))

    def save(self, name, value):
        return panel.write_json(self.root/name,value)

    def build(self, name='panel', **metadata):
        return panel.build_from_cases('fixed',self.candidate,self.opponent,self.decks,self.root/name,2,
                                      self.schedule,{**self.metadata,**metadata})


class Cases(Fixture):
    def test_eight_plans_preserve_exact_7776_keys_without_seed_generation(self):
        keys=set()
        for lineage in ('r1','r2'):
            for endpoint in ('parent','candidate'):
                for reference in ('T1','H'):
                    plan=self.build(f'{lineage}-{endpoint}-{reference}',lineage=lineage,endpoint=endpoint,reference=reference)
                    self.assertEqual(plan['cases'],972)
                    panel.validate_explicit_plan(plan)
                    for item in plan['requests']:
                        for c in item['cases']:
                            key=(lineage,endpoint,c['own'],c['other'],c['family'],c['seat'],c['reference'])
                            self.assertNotIn(key,keys); keys.add(key)
                            self.assertIn(c,self.document['cases'])
        self.assertEqual(len(keys),7776)
        with self.assertRaisesRegex(ValueError,'fresh'):
            self.build('r1-parent-T1')

    def test_schedule_hash_duplicates_holes_and_changed_request_are_refused(self):
        plan=self.build()
        changed=copy.deepcopy(plan); changed['requests'][0]['cases'].pop()
        with self.assertRaisesRegex(ValueError,'coverage'):
            panel.validate_explicit_plan(changed)
        item=plan['requests'][0]
        request=panel.request_for_plan(plan,item); request['matches'][0]['config']['seed']+=1
        item['request']=self.save('changed-request.json',request)
        with self.assertRaisesRegex(ValueError,'native request'):
            panel.validate_explicit_plan(plan)
        changed=copy.deepcopy(self.document); changed['cases'].append(changed['cases'][0])
        duplicate=self.save('duplicate.json',changed)
        with self.assertRaisesRegex(ValueError,'duplicate'):
            panel.explicit_cases(duplicate,self.metadata)
        changed=copy.deepcopy(self.document); changed['cases'].pop(); changed['cases_per_endpoint_lineage']-=1
        with self.assertRaisesRegex(ValueError,'incomplete'):
            panel.explicit_cases(self.save('hole.json',changed),self.metadata)
        Path(self.schedule['path']).write_text('{}')
        with self.assertRaisesRegex(ValueError,'pinned file changed'):
            panel.explicit_cases(self.schedule,self.metadata)

    def test_explicit_execution_requires_guard_before_any_spawn(self):
        self.build()
        with patch.object(panel,'run_requests',side_effect=AssertionError('must not spawn')):
            with self.assertRaisesRegex(ValueError,'resource guard'):
                panel.qualify(self.root/'panel/plan.json',None,[1,2],self.root/'qual','unused')
            with self.assertRaisesRegex(ValueError,'resource guard'):
                panel.run(self.root/'panel/plan.json',self.root/'missing.json','unused')

    def test_recovery_refuses_foreign_output_before_any_mutation(self):
        foreign=self.root/'foreign'; foreign.mkdir(); (foreign/'sentinel').write_text('preserve')
        request=self.save('foreign-request.json',dict(output_directory=str(foreign)))
        guard=SimpleNamespace(check=lambda:None,root=self.root/'owned',
                              plan=dict(storage=dict(accounting_roots=[str(self.root)])))
        with self.assertRaisesRegex(ValueError,'escapes guarded attempt'):
            panel.recover('unused',Path(request['path']),guard)
        self.assertEqual((foreign/'sentinel').read_text(),'preserve')
        self.assertEqual(sorted(p.name for p in foreign.iterdir()),['sentinel'])


@unittest.skipUnless(os.name=='nt','Windows job containment')
class Guarded(Fixture):
    def setUp(self):
        super().setUp()
        self.cores=bounded.slots.parse_cores(os.environ.get(bounded.slots.CORES_ENV,'0,1'))[:2]
        if len(self.cores)<2:
            self.skipTest('two host_slots cores needed for the scaling fixture')
        self.script=self.root/'fake_native.py'; self.script.write_text(FAKE)
        self.guard_plan=dict(schema=panel.GUARD_SCHEMA,evaluator=panel.pin(Path(sys.executable)),cpu_affinity=self.cores,
            qualification_limits=dict(cpu_seconds=60,wall_seconds=60),run_limits=dict(cpu_seconds=60,wall_seconds=60),
            storage=dict(accounting_roots=[str(self.root)],max_physical_bytes=50*1024**2,
                         projected_additional_bytes=10*1024**2,reserve_bytes=bounded.DISK_RESERVE_BYTES),
            stop_files=[str(self.root/'STOP')])
        self.guard_ref=self.save('guard.json',self.guard_plan)

    def guarded(self, root, qualifying, callback):
        spawn=bounded.Guard.spawn
        def fake_spawn(guard, command, stdout, stderr):
            return spawn(guard,[sys.executable,str(self.script),command[1]],stdout,stderr)
        with patch.object(bounded.Guard,'spawn',new=fake_spawn), \
             patch.object(bounded,'resource_context'), \
             patch.object(bounded.shutil,'disk_usage',return_value=SimpleNamespace(free=200*1024**3)):
            return panel.with_guard(Path(self.guard_ref['path']),root,qualifying,callback)

    def qualify(self):
        self.build()
        return self.guarded(self.root/'qualification',True,lambda guard:panel.qualify(
            self.root/'panel/plan.json',None,[1,2],guard.root,sys.executable,guard,self.qual_cases))

    def test_complete_representative_scaling_and_production_route_through_real_job(self):
        receipt=self.qualify()
        self.assertEqual([t['matches'] for t in receipt['trials']],[18,18])
        self.assertTrue(all(t['parity'] for t in receipt['trials']))
        self.assertEqual({c['seat'] for c in receipt['case_order']},{0,1})
        self.assertEqual({c['own'] for c in receipt['case_order']},set(range(9)))
        for trial in receipt['trials']:
            request=json.loads(panel.checked(trial['requests'][0]).read_text())
            result=json.loads((Path(request['output_directory'])/'match-000000.json').read_text())
            self.assertEqual(result['affinity'],sum(1<<core for core in self.cores))
        result=self.guarded(self.root/'panel/native',False,lambda guard:panel.run(
            self.root/'panel/plan.json',self.root/'qualification/receipt.json',sys.executable,guard))
        self.assertEqual(result['workers'],receipt['selected_workers'])
        rows=panel.rows(self.root/'panel/plan.json')
        self.assertEqual(len(rows),972)
        self.assertTrue(all(r['family'] in range(6) and r['lineage']=='r1' and r['reference']=='T1' for r in rows))
        final=json.loads((self.root/'panel/native/guard-result.json').read_text())
        self.assertTrue(final['complete']); self.assertEqual(final['guard']['active_processes'],0)
        self.assertGreater(final['guard']['cpu_seconds'],0)

    def test_missing_guard_evidence_and_changed_measurement_refuse_before_production(self):
        receipt=self.qualify()
        bad=copy.deepcopy(receipt); bad.pop('guard_binding')
        path=Path(self.save('uncovered.json',bad)['path'])
        with self.assertRaisesRegex(ValueError,'guarded evidence'):
            self.guarded(self.root/'missing-evidence',False,lambda guard:panel.run(
                self.root/'panel/plan.json',path,sys.executable,guard))
        self.assertFalse((self.root/'panel/native').exists())
        Path(receipt['trials'][0]['completions'][0]['path']).write_text('{}')
        with self.assertRaisesRegex(ValueError,'pinned file changed'):
            self.guarded(self.root/'changed-evidence',False,lambda guard:panel.run(
                self.root/'panel/plan.json',self.root/'qualification/receipt.json',sys.executable,guard))
        self.assertFalse((self.root/'panel/native').exists())

    def test_qualification_uses_separate_broad_cases_and_guard_stop(self):
        self.build()
        with self.assertRaisesRegex(ValueError,'separate case schedule'):
            self.guarded(self.root/'bad-qual',True,lambda guard:panel.qualify(
                self.root/'panel/plan.json',None,[1,2],guard.root,sys.executable,guard,self.schedule))
        narrow=self.save('narrow.json',dict(decks=list(ndb.DECK_NAMES),cases=[dict(
            own=0,other=0,family=0,seat=seat,reference='T1',seed=100000,starting_player=0) for seat in (0,1)]))
        with self.assertRaisesRegex(ValueError,'both seats and all decks'):
            self.guarded(self.root/'narrow-qual',True,lambda guard:panel.qualify(
                self.root/'panel/plan.json',None,[1,2],guard.root,sys.executable,guard,narrow))
        (self.root/'STOP').touch()
        with self.assertRaisesRegex(ValueError,'STOP'):
            self.guarded(self.root/'stopped',True,lambda guard:None)
        self.assertFalse((self.root/'stopped').exists())

    def test_multiple_qualifications_share_one_attempt_and_publish_both_pins(self):
        self.build('first',reference='T1'); self.build('second',reference='H')
        def measure(guard):
            return [panel.qualify(self.root/name/'plan.json',None,[1,2],guard.root/name,
                                  sys.executable,guard,self.qual_cases) for name in ('first','second')]
        receipts=self.guarded(self.root/'shared',True,measure)
        completion=json.loads((self.root/'shared/guard-result.json').read_text())
        self.assertTrue(completion['complete'])
        self.assertEqual(len(completion['qualifications']),2)
        self.assertEqual({r['guard_result'] for r in receipts},{str(self.root/'shared/guard-result.json')})
        self.assertEqual({r['case_order'][0]['reference'] for r in receipts},{'T1','H'})

    def test_recovery_is_guarded_and_retains_missing_physical_case(self):
        root=self.root/'recover'; root.mkdir()
        request_path=root/'request.json'
        case=dict(own=0,other=0,family=0,seat=0,reference='T1',seed=999999,starting_player=0)
        command=dict(sources=[panel.candidate_source(self.candidate),self.opponent],
                     matches=[panel.case_match(case,self.decks),panel.case_match(dict(case,seed=123),self.decks)],
                     game_one_only=True,cross_generation_evaluation=True,
                     capture_decisions=False,output_directory=str(root/'native/failed'))
        panel.write_json(request_path,command)
        def attempt(guard):
            self.assertFalse(panel.run_one(sys.executable,request_path,guard=guard))
            receipt=panel.recover(sys.executable,request_path,guard)
            before=panel.checked(receipt).read_bytes()
            self.assertEqual(panel.recover('unused',request_path,guard),receipt)
            self.assertEqual(panel.checked(receipt).read_bytes(),before)
            return receipt
        receipt=self.guarded(root/'native',False,attempt)
        recovered=json.loads(panel.checked(receipt).read_text())
        self.assertEqual(recovered['errors'][0]['index'],0)
        self.assertEqual(recovered['errors'][0]['seed'],999999)
        self.assertEqual(len(recovered['segments']),2)
        self.assertEqual([segment['count'] for segment in recovered['segments']],[0,1])


if __name__=='__main__':
    unittest.main()
