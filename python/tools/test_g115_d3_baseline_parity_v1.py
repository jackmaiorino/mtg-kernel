import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from g115_d3_baseline_parity_v1 import normalized_pair
from g115_d3_native_results_v1 import analyze, failure_context


class BaselineParityTests(unittest.TestCase):
    def test_typed_abort_retains_root_step_and_original_evidence(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            failure={'error': 'SearchAbort', 'search': [{'failure': {'game': 2, 'step': 137, 'root': 'abc'}}]}
            (root/'failure.json').write_text(json.dumps({'error': 'SearchAbort'}))
            (root/'search-failure-000000.json').write_text(json.dumps(failure))
            result=failure_context({'base_command': {'matches': [{'config': {'seed': 41}}]}},
                                   {'output_directory': folder})
            self.assertEqual(result['match_input'][0]['config']['seed'],41)
            self.assertEqual(result['native_failure_records'][1]['record'],failure)
            self.assertEqual(len(result['native_failure_records'][1]['sha256']),64)

    def setUp(self):
        self.old = dict(models=[{}, {'identity': {'source_import': {
            'appended_rows': 'method; envelope_sha256='+'1'*64}}}],
            outcome={'winner': {'winner': 0}}, decisions=[], decision_count=123,
            games=[{'winner': 0}], v3_forced_actions=[False, True])
        self.new = copy.deepcopy(self.old)
        self.new['models'][1]['identity']['source_import']['appended_rows'] = 'method; envelope_sha256='+'2'*64
        self.new['terminal_audit_v1'] = {'branches': 0, 'counts': {}, 'roots': []}

    def test_only_declared_metadata_is_normalized(self):
        original = copy.deepcopy(self.new)
        old, new = normalized_pair(self.old, self.new, 0, ['2'*64])
        self.assertEqual(old, new)
        self.assertEqual(self.new, original)

    def test_outcomes_counts_adapter_and_model_changes_remain_mismatches(self):
        for key, value in [('outcome', {'winner': {'winner': 1}}), ('decision_count', 124),
                           ('v3_forced_actions', [False, False]), ('games', [{'winner': 1}]),
                           ('unexpected_field', True)]:
            changed = copy.deepcopy(self.new)
            changed[key] = value
            self.assertNotEqual(*normalized_pair(self.old, changed, 0, ['2'*64]))
        changed = copy.deepcopy(self.new)
        changed['models'][0]['weights'] = 'changed'
        self.assertNotEqual(*normalized_pair(self.old, changed, 0, ['2'*64]))

    def test_unregistered_provenance_and_other_import_changes_rejected(self):
        with self.assertRaises(ValueError):
            normalized_pair(self.old, self.new, 0, ['3'*64])
        self.new['models'][1]['identity']['source_import']['appended_rows'] += '; other change'
        with self.assertRaises(ValueError):
            normalized_pair(self.old, self.new, 0, ['2'*64])

    def test_validity_failure_withholds_statistics_even_with_complete_native_rows(self):
        jobs = [dict(id=f'{c}-{r}-{s}-{a}', cell=c, replica=r, candidate_seat=s, arm=a)
                for c in range(64) for r in range(8) for s in (0, 1) for a in ('baseline', 'search')]
        panel = dict(jobs=jobs)
        execution = dict(jobs=[{'id': j['id']} for j in jobs], source_commit='source', models=[])
        with patch('g115_d3_native_results_v1.read_match', side_effect=lambda *args: {'win': 0, '_timings_ns': []}), \
             patch('g115_d3_analysis_v1.analyze_pair') as statistics:
            result = analyze(panel, execution, validity_failures=[{'error': 'Baseline mismatch'}])
        self.assertEqual(result['completed_jobs'], 2048)
        self.assertFalse(result['complete'])
        self.assertNotIn('statistics', result)
        statistics.assert_not_called()


if __name__ == '__main__':
    unittest.main()
