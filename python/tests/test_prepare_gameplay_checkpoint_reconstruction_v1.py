import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).parents[1]/'tools/prepare_gameplay_checkpoint_reconstruction_v1.py'
spec = importlib.util.spec_from_file_location('reconstruction_prepare', SCRIPT)
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class PreparationTests(unittest.TestCase):
    def fixture(self, root):
        registrations = [{'mainboard': list(range(60)), 'sideboard': []} for _ in range(2)]
        rows = [{'decision_index': 0, 'actor': 'p0', 'visible': {'kind': 'play_draw'}}]
        for seat in (1, 0):
            rows.append({'decision_index': len(rows), 'actor': 'p'+str(seat),
                         'behavior': {'selected_index': 0}, 'visible': {'kind': 'mulligan',
                         'input': {'mulligans_taken': 0, 'own_configuration': registrations[seat]}}})
        for step in range(9):
            rows.append({'decision_index': len(rows), 'actor': 'p0',
                         'behavior': {'selected_index': 1, 'mass_numerators': ['ignored']},
                         'visible': {'kind': 'gameplay', 'observation': {'step_index': step},
                         'ordered_actions': [{'kind': 'a'}, {'kind': 'b'}]}})
        doc = {'config': {'registrations': registrations, 'deck_ids': ['a', 'b']},
               'packages': [{'opening': {'kind': 'existing', 'protocol': 'keep_seven_v2'},
                             'sideboard': {'kind': 'keep'}} for _ in range(2)],
               'collected': {'games': [{'environment_seed': 123}], 'trajectory': {'games': [
                   {'game_index': 1, 'start': {'starting_player': 0, 'choice': 'play'}, 'decisions': rows}]}}}
        source = root/'source.json'; source.write_text(json.dumps(doc))
        case = {'id': 'C011', 'actor': 'p0', 'decision_key': 11, 'native_game_ordinal': 1,
                'pointer': '/collected/trajectory/games/0/decisions/11'}
        return doc, source, case

    def test_prefix_preserves_selected_semantics_without_probabilities(self):
        with tempfile.TemporaryDirectory() as tmp:
            _, source, case = self.fixture(Path(tmp))
            result = prepare.bo3_prefix('case1-g115-seat0', prepare.pin(source), [case])
            self.assertEqual(result['action_ceiling'], 8)
            self.assertEqual(len(result['decisions']), 9)
            self.assertEqual(result['targets'][0]['step'], 8)
            self.assertEqual(result['decisions'][0]['ordered_actions'][1], {'kind': 'b'})
            self.assertNotIn('mass_numerators', json.dumps(result))

    def test_rejects_noncontiguous_timeline_and_outside_menu(self):
        with tempfile.TemporaryDirectory() as tmp:
            doc, source, case = self.fixture(Path(tmp))
            rows = doc['collected']['trajectory']['games'][0]['decisions']
            rows[4]['visible']['observation']['step_index'] = 9
            source.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, 'noncontiguous'):
                prepare.bo3_prefix('case1-g115-seat0', prepare.pin(source), [case])
            rows[4]['visible']['observation']['step_index'] = 1
            rows[4]['behavior']['selected_index'] = 2
            source.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, 'selection'):
                prepare.bo3_prefix('case1-g115-seat0', prepare.pin(source), [case])

    def test_refuses_changed_opening(self):
        with tempfile.TemporaryDirectory() as tmp:
            doc, source, case = self.fixture(Path(tmp))
            doc['collected']['trajectory']['games'][0]['decisions'][1]['visible']['input']['mulligans_taken'] = 1
            source.write_text(json.dumps(doc))
            with self.assertRaisesRegex(ValueError, 'keep seven'):
                prepare.bo3_prefix('case1-g115-seat0', prepare.pin(source), [case])

    def test_create_only_preparation_refuses_before_reading_sources(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)/'request.json'; output.write_text('preserved')
            with self.assertRaisesRegex(ValueError, 'preserve'):
                prepare.prepare(Path(tmp)/'absent', Path(tmp)/'absent2', output)
            self.assertEqual(output.read_text(), 'preserved')


if __name__ == '__main__':
    unittest.main()
