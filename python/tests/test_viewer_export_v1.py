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

class ViewerExportTests(unittest.TestCase):
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
