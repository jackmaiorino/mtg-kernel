import copy
import sys
import unittest
import json
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
from viewer_reconstruction_export_v1 import export_root
from viewer_export_v1 import digest_search, Exporter, build_recording

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

    def test_public_reconstruction_excludes_private_opponent_menu_and_seed_fields(self):
        observation={'acting_player':'p0','own_hand':[],'projection':{'turn':1,'phase':'main1',
            'active_player':'p1','priority_player':'p1','life_totals':[20,0],'hand_counts':[0,7],
            'library_counts':[50,50],'mana_pools':[[0]*6,[0]*6],'battlefield':[[],[]],
            'graveyards':[[],[]],'exile':[],'stack':[]}}
        rows=[{'kind':'header','perspective':'p0','cards':{'999':{'name':'SECRET CARD'}},
            'runtime':{'engine_commit':'c'*40,'executable_sha256':'d'*64},'decks':['Spy','Wildfire'],
            'focal_checkpoint':'e'*64,'opponent_checkpoint':'f'*64,'original_source':'a'*40,
            'original_runtime':'a'*64,'witness_sha256':'b'*64,'bank_sha256':'c'*64,
            'runtime_rules':'resolution-boundary-v1','source_policy_seeds':[123,456]},
            {'kind':'decision','decision':{'episode_id':1,'step':0,'physical_decision_id':0,'substep_index':0,'acting_player':'p1'},
                'before':observation,'after':observation,'actions':[{'action_kind':'pass'},{'action_kind':'cast_spell','private_name':'SECRET CARD'}],
                'labels':['Pass','Cast SECRET CARD'],'selected':0,'evidence':None,'search':None,
                'terminal':{'terminal_classification':'natural','winner':'p0'}},
            {'kind':'footer','written':1,'complete':True,'anchor_verified':True}]
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'replay.jsonl'
            path.write_text(''.join(json.dumps(dict(r,schema='mtg-kernel-viewer-reconstruction/v1'))+'\n' for r in rows),encoding='utf-8')
            recording=build_recording(path,'privacy-case','Privacy case',Exporter({},'p0'))
            public=json.dumps(recording)
            self.assertNotIn('SECRET',public);self.assertNotIn('source_policy_seeds',public)
            self.assertEqual(recording['frames'][0]['actions'],[{'index':0,'label':'Pass','kind':'pass','refs':[]}])
            rows[-1]['complete']=False
            path.write_text(''.join(json.dumps(dict(r,schema='mtg-kernel-viewer-reconstruction/v1'))+'\n' for r in rows),encoding='utf-8')
            with self.assertRaisesRegex(ValueError,'incomplete'):build_recording(path,'privacy-case','Privacy case',Exporter({},'p0'))

if __name__=='__main__':unittest.main()
