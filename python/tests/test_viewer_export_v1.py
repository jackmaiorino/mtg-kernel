import importlib.util
import json
import sys
import unittest
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / 'tools'
sys.path.insert(0, str(TOOLS))
import viewer_export_v1 as export
import viewer_demo_pack_v1 as pack
import viewer_capture_launcher_v1 as capture
import viewer_search_export_v1 as replay_export

class ViewerExportTests(unittest.TestCase):
    def test_replay_redacts_sampled_hands_keys_and_hidden_menu_labels(self):
        observation={'acting_player':'p0','own_hand':[{'stable':{'arena_id':99,'card_db_id':999,'owner':'p0','controller':'p0'}}],
          'projection':{'turn':1,'phase':'main1','active_player':'p0','priority_player':'p0','life_totals':[20,20],
           'hand_counts':[7,7],'library_counts':[53,53],'mana_pools':[[0]*6,[0]*6],'battlefield':[[],[]],'graveyards':[[],[]],'exile':[],'stack':[]}}
        root_actions=[{'index':i,'label':label,'kind':'pass','refs':[]} for i,label in enumerate(['Pass','Other move'])]
        menu=[{'action_kind':'cast_spell','source':{'arena_id':99,'card_db_id':999}}]
        contexts=[{'simulation':0,'node':0,'action':0,'actor':'p0','board':observation},
          {'simulation':0,'node':1,'action':0,'actor':'p1','board':observation,'actions':menu,'labels':['SECRET CARD']},
          {'simulation':0,'leaf':True,'reason':'expanded','board':observation},
          {'simulation':1,'node':0,'action':1,'actor':'p0','board':observation},
          {'simulation':1,'leaf':True,'reason':'coverage','board':observation}]
        raw={'schema':'mtg-kernel-search-capture/v1','complete':True,'contexts':contexts,
          'nodes':[{'key':'PRIVATE KEY','actor':0,'visits':2,'priors':[700000,300000],'children':[[1],[]],'edge_visits':[1,1],'edge_value_sums':[900,-500]},
          {'key':'PRIVATE KEY','actor':1,'visits':1,'priors':[1000000],'children':[[]],'edge_visits':[1],'edge_value_sums':[900]}],
          'trace':[{'ordinal':0,'value':900,'choices':[{'node':0,'action':0,'mode':'puct','scores':[None,None]},{'node':1,'action':0,'mode':'unvisited','scores':[None]}]},
          {'ordinal':1,'value':-500,'choices':[{'node':0,'action':1,'mode':'unvisited','scores':['1000',None]}]}]}
        search={'simulations':2,'stats':[{'index':0,'visits':1,'valueSum':'900'},{'index':1,'visits':1,'valueSum':'-500'}]}
        exporter=export.Exporter({'999':{'name':'SECRET CARD'}},'p0')
        result=replay_export.export_replay(raw,exporter,{'episode':1,'step':1,'physical':1,'substep':0,'actor':'p0'},root_actions,search)
        self.assertNotIn('SECRET',json.dumps(result));self.assertNotIn('PRIVATE',json.dumps(result));self.assertEqual(exporter.cards,{})
        self.assertEqual(result['nodes'][1]['actions'][0]['label'],'Cast spell')
        import copy
        compact=copy.deepcopy(raw);compact['snapshots']=[]
        for context in compact['contexts']:
            snapshot={'board':context.pop('board')}
            if 'actions' in context:snapshot['actions']=context.pop('actions')
            context['snapshot']=len(compact['snapshots']);compact['snapshots'].append(snapshot)
        self.assertEqual(replay_export.export_replay(compact,exporter,result['key'],root_actions,search),result)
        compact['contexts'][0]['snapshot']=999
        with self.assertRaisesRegex(ValueError,'snapshot'):replay_export.export_replay(compact,exporter,result['key'],root_actions,search)
        raw['nodes'][0]['edge_visits'][0]=2
        with self.assertRaisesRegex(ValueError,'statistics'):replay_export.export_replay(raw,exporter,result['key'],root_actions,search)

    def test_missing_or_interrupted_capture_is_incomplete(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'capture.jsonl'
            result = {'exit_code':0, 'capture':str(path)}
            self.assertFalse(capture.capture_complete(result))
            path.write_text('{"kind":"decision"')
            self.assertFalse(capture.capture_complete(result))
            path.write_text('{"kind":"decision","terminal":{"terminal_classification":"natural"}}\n{"kind":"footer","complete":true}\n')
            self.assertTrue(capture.capture_complete(result))
            path.write_text('{"kind":"footer","complete":true}\n')
            self.assertFalse(capture.capture_complete(result))

    def test_reference_walk_keeps_only_object_identity(self):
        self.assertEqual(export.references({'source':{'arena_id':4,'card_db_id':12},'target':[{'arena_id':8}]}), ['o4','o8'])

    def test_fixed_perspective_excludes_opposing_hand(self):
        observation = {'acting_player':'p1','own_hand':[{'stable':{'arena_id':2,'card_db_id':99,'owner':'p1','controller':'p1'}}],
          'projection':{'turn':1,'phase':'main1','active_player':'p1','priority_player':'p1','life_totals':[20,20],
           'hand_counts':[7,7],'library_counts':[53,53],'mana_pools':[[0]*6,[0]*6],'battlefield':[[],[]],'graveyards':[[],[]],'exile':[],'stack':[]}}
        exporter = export.Exporter({'99':{'name':'SECRET CARD'}}, 'p0')
        board = exporter.board(observation)
        self.assertEqual(board['players'][1]['hand'], [])
        self.assertNotIn('SECRET CARD', json.dumps(board))
        self.assertEqual(exporter.cards, {})

    def test_digest_commits_to_search_table_and_work(self):
        result = {'root_action_stats':[{'flat_action_index':0,'visits':1,'value_sum':9,'mean_value':9}],
          'selected_index':0,'transitions_used':1,'simulations':1,'tree_node_count':1}
        first = export.digest_search(result)
        result['transitions_used'] = 2
        self.assertNotEqual(first, export.digest_search(result))

    def test_truncation_is_not_a_natural_game(self):
        with self.assertRaises(ValueError):
            export.terminal({'terminal_classification':'truncated','winner':'p0'})

    def test_legacy_outcome_preserves_winner(self):
        self.assertEqual(export.terminal({'classification':'natural','outcome':'p1_win'})['winner'],'p1')

    def test_release_paths_cannot_escape_on_either_platform(self):
        root=Path.cwd()
        for value in ('C:/outside.json','C:outside.json','//server/share/a','\\\\server\\share\\a','../outside.json','assets/../../a'):
            with self.subTest(path=value),self.assertRaises(ValueError):pack.safe_path(root,value)
        self.assertEqual(pack.safe_path(root,'assets/card.jpg'),root/'assets/card.jpg')

if __name__ == '__main__': unittest.main()
