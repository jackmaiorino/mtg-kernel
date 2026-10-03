import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('exposure',Path(__file__).parents[1]/'public_stack_exposure_v1.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)

class ExposureTest(unittest.TestCase):
    def test_counts_baselines_once_and_choice_substeps_separately(self):
        f=[0.]*312;f[1]=1.;f[15]=1.;f[25]=1.;f[289]=1.
        target=f.copy();target[1]=0.
        aux=dict(schema='mtg-kernel-public-stack-input/v1',contract_sha256='contract',rows=[dict(features=f),dict(features=target)],stack_items=1)
        t=dict(schema='mtg-kernel-public-stack-trajectory/v1',input_mode='structured',
            episode=dict(learner_seat=0,registered=[dict(label='deck')]),
            decisions=[dict(actor=0,logits=[0,0],physical_decision_id=4),dict(actor=0,logits=[0],physical_decision_id=4),dict(actor=1,logits=[0,0],physical_decision_id=5)],
            auxiliary=[aux,aux,None])
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'row.json';p.write_text(json.dumps(t))
            r=m.count_item(dict(file=m.pin(p),arm='structured',contract_sha256='contract',update=0,episode=0))
            self.assertEqual(r['physical_decisions'],1)
            self.assertEqual(r['counts']['all']['kicked_items'],2)
            self.assertEqual(r['counts']['choice']['kicked_items'],1)
            self.assertEqual(r['counts']['choice']['special_decisions'],1)
            self.assertNotIn('mode_nonprimary_decisions',r['counts']['all'])
            self.assertEqual(m.aggregate([r])['structured']['episodes'],1)

    def test_changed_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'row.json';p.write_text('{}');item=m.pin(p);p.write_text('{"altered":true}')
            with self.assertRaises(AssertionError):m.checked(item)

if __name__=='__main__':unittest.main()
