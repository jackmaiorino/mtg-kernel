"""Guard regressions: complete coverage and content identity, not speed claims."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import stack_evaluation_throughput_v1 as guard


class EvaluationGuardTests(unittest.TestCase):
    def setUp(self):
        self.files={};self.binary={'path':'binary','sha256':'B'}
        self.placement={'workers':8};self.jobs=[];self.expected=[];self.fingerprints={}
        def save(path,value):self.files[path]=value;return {'path':path,'sha256':path}
        self.save=save
        worker=save('worker',dict(hostname=guard.HOSTS['jack'],errors=[],workers=8))
        for n in range(32):
            name=f'j{n}';directory=f'job{n}/outputs'
            matches=[{'seed':8*n+i} for i in range(8)]
            request={'matches':matches,'output_directory':directory}
            req=save('req'+name,request)
            self.expected.append(dict(id=name,command={'matches':matches}))
            execution=save('exec'+name,dict(exit_code=0,timeout=False,binary=self.binary,request=req,
                host='jack',storage=self.placement,native_binary={'sha256':'B'},native_request={'sha256':req['sha256']}))
            recovery=save('recover'+name,dict(mismatches=0))
            digests=[]
            for i,m in enumerate(matches):
                path=f'{directory}/match-{i:06}.json';digests.append(path)
                save(path,dict(match=m,decisions=[],models=['model'],games=[{}],decision_count=1))
                self.fingerprints[f'{name}/{i}']=path
            save(directory+'/start.json',dict(command=request,models=['model']))
            save(directory+'/completion.json',dict(matches=8,match_sha256=digests,natural_games=8,decisions=8))
            self.jobs.append(dict(id=name,host='jack',request=req,execution=execution,recovery=recovery,output_directory=directory))
        self.report=dict(jobs=self.jobs,matches=256,allocation={'jack':self.placement},workers={'jack':worker},fingerprints=self.fingerprints)
        self.item=save('report',self.report)
        self.plan=dict(binary=self.binary,qualification_jobs=self.expected)

    def validate(self):
        def name(path):return str(path).replace('\\','/')
        with patch.object(guard,'read',side_effect=lambda p:self.files[name(p)]),patch.object(guard,'checked',side_effect=lambda p:Path(p['path'])),patch.object(guard,'pin',side_effect=lambda p:dict(path=name(p),sha256=name(p))):
            return guard.validate_report(self.item,self.plan)

    def test_complete_panel_is_accepted(self):
        self.assertEqual(len(self.validate()[1]),256)

    def test_duplicate_job_cannot_replace_missing_job(self):
        self.jobs[-1]=copy.deepcopy(self.jobs[0])
        with self.assertRaises(AssertionError):self.validate()

    def test_valid_hash_cannot_substitute_wrong_seed_content(self):
        self.files['job0/outputs/match-000000.json']['match']={'seed':99999}
        with self.assertRaises(AssertionError):self.validate()

    def test_wrong_model_receipt_is_rejected(self):
        self.files['job0/outputs/match-000000.json']['models']=['different-model']
        with self.assertRaises(AssertionError):self.validate()

    def test_revocation_is_checked_before_any_dispatch_input(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'qualification-revocation.json').write_text('{}')
            with self.assertRaisesRegex(AssertionError,'revoked'):
                guard.require_choice(root/'nonexistent-choice.json',{})
