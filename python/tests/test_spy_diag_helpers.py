"""Correctness fixtures only; no search, scoring, native child or experiment."""
import importlib
import contextlib
import io
import json
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

TOOLS = Path(__file__).resolve().parents[2] / 'tools/spy_diag'
sys.path.insert(0, str(TOOLS))
coverage = importlib.import_module('panel_coverage')
analysis = importlib.import_module('analyze')
queue = importlib.import_module('diag_queue')


class PanelTests(unittest.TestCase):
    def test_analyzer_accepts_complete_panel_and_refuses_missing_or_duplicate_inputs(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            panel = root/'panel.json'
            panel.write_text(json.dumps({'roots': [{'root_id': rid, 'role': 'fixture'}
                                                  for rid in ('a', 'b')]}))
            traces = []
            for rid in ('a', 'b'):
                trace = root/(rid + '.jsonl')
                trace.write_text(json.dumps({'r': 'meta', 'root_id': rid, 'cast_root': True,
                                             'stratum': 'fixture', 'opp_model': 'fixture'}) + '\n')
                traces.append(str(trace))
            for label, inputs, valid in [('complete', traces, True), ('missing', traces[:1], False),
                                          ('duplicate', traces + traces[:1], False)]:
                output = root/label
                args = ['analyze.py', '--panel', str(panel), '--out', str(output)] + inputs
                with self.subTest(case=label), patch.object(sys, 'argv', args), \
                        contextlib.redirect_stdout(io.StringIO()):
                    if valid:
                        analysis.main()
                        self.assertEqual(set(json.loads((output/'analysis.json').read_text())), {'a', 'b'})
                    else:
                        with self.assertRaises(ValueError):
                            analysis.main()
                        self.assertFalse(output.exists())

    def test_missing_duplicate_and_extra_roots_refuse(self):
        expected = coverage.expected_roots({'roots': [{'root_id': 'a'}, {'root_id': 'b'}]})
        for observed in (['a'], ['a', 'a'], ['a', 'b', 'b'], ['a', 'b', 'c']):
            with self.subTest(observed=observed), self.assertRaises(ValueError):
                coverage.require_coverage(observed, expected)
        coverage.require_coverage(['b', 'a'], expected)

    def test_subset_is_explicit_and_membership_checked(self):
        panel = {'roots': [{'root_id': 'a'}, {'root_id': 'b'}]}
        expected = coverage.expected_roots(panel, ['a'])
        coverage.require_coverage(['a'], expected)
        for selected in (['unknown'], ['a', 'a'], []):
            with self.assertRaises(ValueError):
                coverage.expected_roots(panel, selected)
        with self.assertRaises(ValueError):
            coverage.expected_roots({'roots': [{'root_id': 'a'}, {'root_id': 'a'}]})

    def test_display_rounding_does_not_create_seeded_tie(self):
        node = {'perm': [1, 0], 'lab': [2, 0], 'n': [199, 201], 'w': [99, 100]}
        verdict = analysis.compat_verdict(node, 2, 1, {}, 0)
        self.assertEqual(verdict['compatible'][0]['mean'], verdict['frozen_choice']['mean'])
        self.assertEqual(verdict['verdict'], 'qualified compatible edge loses on mean')
        node.update(n=[200, 200], w=[100, 100])
        self.assertEqual(analysis.compat_verdict(node, 2, 1, {}, 0)['verdict'],
                         'qualified compatible edge ties, loses seeded order')


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        root = Path(self.scratch.name)
        for name, value in {'OUT': root, 'TRACES': root/'traces', 'LOG': root/'queue.log',
                            'LEDGER': root/'ledger.jsonl', 'BIN_SHA': 'fixture'}.items():
            context = patch.object(queue, name, value)
            context.start()
            self.addCleanup(context.stop)

    def run_fixture(self, name):
        process = Mock()
        process.poll.return_value = None
        process.wait.return_value = -1
        out = queue.OUT / (name + '.jsonl')
        out.write_text('{"partial":', encoding='utf-8')
        return {'job': {'name': name}, 'p': process, 'h': 1, 'out': out,
                'stream': Mock(), 'started': time.monotonic(), 'record': None}

    def test_invalid_phase_and_nonfinite_budgets_refuse(self):
        for phase, budget in [('typo', 100), ('panel', 'NaN'), ('qualification', 'Infinity'),
                              ('panel', 0), ('panel', -1)]:
            with self.subTest(phase=phase, budget=budget), self.assertRaises(ValueError):
                queue.validate_queue({'phase': phase, 'budget_cpu_seconds': budget, 'groups': []})
        self.assertEqual(queue.validate_queue({'phase': 'panel', 'budget_cpu_seconds': 100, 'groups': []}),
                         ('panel', 100))

    def test_duplicate_attempt_names_refuse(self):
        job = {'name': 'same'}
        with self.assertRaises(ValueError):
            queue.validate_queue({'phase': 'panel', 'budget_cpu_seconds': 100, 'groups': [[job], [job]]})

    def test_malformed_output_still_persists_cpu_charge(self):
        run = self.run_fixture('partial')
        with patch.object(queue, 'cpu_seconds', return_value=3.5), patch.object(queue, 'K32'):
            record = queue.finish(run, 'panel', True)
        self.assertIn('output_error', record)
        self.assertEqual(queue.ledger_total('panel'), 3.5)
        self.assertTrue(run['stream'].close.called)

    def test_unknown_cpu_is_preserved_and_blocks_budget_reuse(self):
        run = self.run_fixture('unknown')
        with patch.object(queue, 'cpu_seconds', return_value=None), patch.object(queue, 'K32'):
            record = queue.finish(run, 'panel', True)
        self.assertIsNone(record['cpu_seconds'])
        self.assertIsNone(json.loads(queue.LEDGER.read_text())['cpu_seconds'])
        with self.assertRaises(RuntimeError):
            queue.ledger_total('panel')
        with self.assertRaises(RuntimeError):
            queue.ledger_total('qualification')

    def test_cleanup_accounts_every_child_despite_one_output_failure(self):
        runs = [self.run_fixture('one'), self.run_fixture('two')]
        with patch.object(queue, 'cpu_seconds', return_value=2), patch.object(queue, 'K32'):
            queue.cleanup(runs, 'panel')
        for run in runs:
            run['p'].kill.assert_called_once()
            run['p'].wait.assert_called_once()
        self.assertEqual(queue.ledger_total('panel'), 4)

    def test_cleanup_continues_after_an_accounting_exception(self):
        runs = [self.run_fixture('one'), self.run_fixture('two')]
        with patch.object(queue, 'finish', side_effect=[OSError('ledger'), {}]) as finish:
            with self.assertRaises(RuntimeError):
                queue.cleanup(runs, 'panel')
        self.assertEqual(finish.call_count, 2)
        for run in runs:
            run['p'].kill.assert_called_once()

    def test_existing_attempt_refuses_before_spawning(self):
        (queue.OUT/'existing.jsonl').write_text('preserved')
        with patch.object(queue.subprocess, 'Popen') as spawn:
            with self.assertRaises(ValueError):
                queue.start({'name': 'existing'}, [])
        spawn.assert_not_called()
        self.assertEqual((queue.OUT/'existing.jsonl').read_text(), 'preserved')

    def test_second_start_failure_cleans_up_already_started_child(self):
        root = queue.OUT
        (root/'BINARY.json').write_text(json.dumps({'sha256': 'fixture'}))
        spec = root/'queue.json'
        spec.write_text(json.dumps({'phase': 'qualification', 'budget_cpu_seconds': 100,
                                    'groups': [[{'name': 'one'}, {'name': 'two'}]]}))
        run = self.run_fixture('one')

        def start(job, runs):
            if job['name'] == 'two':
                raise OSError('second spawn failed')
            runs.append(run)
            return run

        with patch.object(queue, 'sha', return_value='fixture'), patch.object(queue, 'K32'), \
                patch.object(queue, 'start', side_effect=start), \
                patch.object(queue, 'cpu_seconds', return_value=2):
            with self.assertRaisesRegex(OSError, 'second spawn failed'):
                queue.main([str(root), 'fixture-binary', str(spec)])
        run['p'].kill.assert_called_once()
        run['p'].wait.assert_called_once()
        self.assertEqual(queue.ledger_total('qualification'), 2)


if __name__ == '__main__':
    unittest.main()
