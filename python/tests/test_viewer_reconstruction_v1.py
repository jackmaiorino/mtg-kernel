import copy
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
from viewer_reconstruction_export_v1 import export_root
from viewer_export_v1 import digest_search

class ReconstructionTests(unittest.TestCase):
    def raw(self):
        return {'live_order':[1,0], 'record':{'budget':64000,'transitions':64000,'completed_natural':18,
            'tree':{'nodes':14,'root':{'frozen_choice':0,'n':[8,10],'wins':[3,0]}}}}

    def test_canonical_order_is_mapped_to_original_action_ids(self):
        result=export_root(self.raw(),1,2)
        self.assertEqual(result['stats'],[
            {'index':0,'visits':10,'valueSum':'0','meanValue':'0'},
            {'index':1,'visits':8,'valueSum':'3000000','meanValue':'375000'}])
        self.assertEqual(result['digest'],digest_search({'selected_index':1,'simulations':18,
            'transitions_used':64000,'tree_node_count':14,'root_action_stats':[
                {'flat_action_index':0,'visits':10,'value_sum':0,'mean_value':0},
                {'flat_action_index':1,'visits':8,'value_sum':3000000,'mean_value':375000}]}))
        self.assertEqual(result['algorithm'],'persistent-uct')
        self.assertNotIn('replay',result)

    def test_rejects_wrong_mapping_selection_and_impossible_outcomes(self):
        for mutate in [lambda r:r.update(live_order=[0,0]),
            lambda r:r['record']['tree']['root'].update(wins=[9,0]),
            lambda r:r['record'].update(completed_natural=19)]:
            raw=copy.deepcopy(self.raw());mutate(raw)
            with self.assertRaises(ValueError):export_root(raw,1,2)
        with self.assertRaises(ValueError):export_root(self.raw(),0,2)

if __name__=='__main__':unittest.main()
