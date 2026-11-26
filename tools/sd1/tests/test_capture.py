"""Tests for tools/sd1/capture.py on a fake run tree: records in episode order, the panel (first N reached states,
one per game, hashed before any arm runs) and convert's check of the arms pass against the panel pass."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import capture  # noqa: E402


def capture_doc(reached, step, logits, arms=False):
    if not reached:
        return {'reached': False}
    doc = {'reached': True, 'schema': capture.VERSION, 'step': step, 'width': len(logits),
           'policy': {'logit_bits': logits}, 'features': {'turn': 3}, 'arms': []}
    if arms:
        doc['arms'] = [{'arm': 'D64', 'world': 'fresh', 'search_seed': 0, 'seed_root': 'r', 'status': 'ok',
                        'cpu_s': 1.5, 'n': [8, 8], 'wins': [4, 5], 'draws': [0, 0], 'perm_live': [1, 0],
                        'frozen_choice': 1, 'pick_v2': 1, 'visits': 16, 'fallback': 0,
                        'search': {'budget': 64000, 'completed': 16, 'wall': 9.0, 'nodes': 3}}]
        doc['references'] = {'half_rollouts': 2}
    return doc


def fake_run(root, episodes, arms=False, mutate=None):
    """episodes: per update, a list of (reached, step, logits)."""
    jobs = []
    for u, eps in enumerate(episodes):
        out = Path(root, 'runs', f'u{u:06d}')
        out.mkdir(parents=True)
        pins = []
        for slot, (reached, step, logits) in enumerate(eps):
            c = capture_doc(reached, step, logits, arms)
            if mutate:
                c = mutate(u, slot, c)
            side = out / f'ep{slot}.json'
            side.write_text(json.dumps({'slot': slot, 'attempts': [{
                'natural': True, 'episode_id': f'sd1/panel-u{u}-j{slot}', 'seed': 100 * u + slot,
                'options': {'run': 'sd1/panel', 'update': u}, 'learner_deck': 'x',
                'w5': {'placement_index': 5}, 'capture_0b': c}]}))
            pins.append({'path': side.as_posix()})
        (out / 'collection.json').write_text(json.dumps({'ex1': {'episodes': pins}}))
        job = Path(root, 'jobs', f'u{u:06d}.json')
        job.parent.mkdir(parents=True, exist_ok=True)
        job.write_text(json.dumps({'output_directory': out.as_posix()}))
        capture.stamp_path(out).write_text(capture.sha256_file(job) + '\n')
        jobs.append(job.as_posix())
    Path(root, 'jobs', 'manifest.json').write_text(json.dumps({
        'chain': 'sd1/panel', 'source_checkpoint_sha256': 'c' * 64, 'job_set_sha256': 'j' * 64, 'jobs': jobs}))


EPISODES = [[(True, 3, [1, 2]), (False, 0, []), (True, 7, [3, 4])],
            [(True, 2, [5, 6]), (True, 9, [7, 8]), (False, 0, [])]]


class Capture(unittest.TestCase):
    def test_panel_and_convert(self):
        with tempfile.TemporaryDirectory() as d:
            fake_run(Path(d, 'panel'), EPISODES)
            fake_run(Path(d, 'arms'), EPISODES, arms=True)
            panel = capture.panel(Path(d, 'panel'), 3, 2)
            self.assertEqual([s[0] for s in panel['states']], [0, 2, 3])
            self.assertEqual(panel['last_episode'], 3)
            self.assertEqual(len(panel['panel_sha256']), 64)
            recs = capture.convert(Path(d, 'arms'), panel)
            self.assertEqual([r['episode'] for r in recs], [0, 2, 3])
            arm = recs[0]['arms'][0]
            self.assertNotIn('wall', arm['search'])
            self.assertEqual(arm['frozen_choice'], 1)
            self.assertEqual(arm['fallback'], 0)
            self.assertEqual(recs[0]['references'], {'half_rollouts': 2})
            with self.assertRaises(SystemExit):
                capture.panel(Path(d, 'panel'), 5, 2)

    def test_convert_refuses_a_state_that_differs_from_the_panel_pass(self):
        def change(u, slot, c):
            if (u, slot) == (0, 2) and c['reached']:
                c['step'] = 8
            return c
        with tempfile.TemporaryDirectory() as d:
            fake_run(Path(d, 'panel'), EPISODES)
            fake_run(Path(d, 'arms'), EPISODES, arms=True, mutate=change)
            panel = capture.panel(Path(d, 'panel'), 3, 2)
            with self.assertRaises(SystemExit):
                capture.convert(Path(d, 'arms'), panel)


def labelled(head_compare=False):
    """A mutate that labels reached states `features.decision_kind` and adds a head comparison."""
    def change(u, slot, c):
        if c['reached']:
            c['features']['decision_kind'] = 'game'
            if head_compare:
                c['head_compare'] = {'schema': 'ss1-head-compare-v1', 'status': 'ok', 'worlds': 16,
                                     'seed': 20261125, 'actions': [0, 1]}
        return c
    return change


class Screen(unittest.TestCase):
    """The search screen's options (collab LANES/lead/reviews/search-screen-20261125): the decision kind and the
    head comparison reach the jobs and the records."""

    def test_sd1_options_keep_existing_bytes_and_add_only_what_is_asked(self):
        arms = {'version': capture.VERSION, 'half_rollouts': 64, 'coverage_max': 16, 'root_value': True,
                'arms': [{'label': 'F16', 'world': 'fresh', 'budget': 16000, 'search_seeds': [0, 1]}]}
        self.assertEqual(capture.sd1_options(arms, False),
                         {'version': capture.VERSION, 'arms': arms['arms'], 'half_rollouts': 64,
                          'coverage_max': 16, 'root_value': True})
        self.assertEqual(capture.sd1_options(arms, True),
                         {'version': capture.VERSION, 'arms': [], 'half_rollouts': 0, 'coverage_max': 16})
        source = {'play_import': {'path': 'x', 'sha256': 'a' * 64}}
        with tempfile.TemporaryDirectory() as d:
            src = Path(d, 'b512-v7-source.json')
            src.write_text(json.dumps(source))
            dose = Path(d, 'b512-dose.json')
            dose.write_text(json.dumps({'source': source, 'sha256': 'b' * 64}))
            screen = dict(arms, arms=[], half_rollouts=0, decision_kind=True,
                          head_compare={'second_source': src.as_posix(), 'worlds': 16, 'seed': 20261125})
            full = capture.sd1_options(screen, False)
            self.assertEqual(full['head_compare'], {'second_source': source, 'worlds': 16, 'seed': 20261125})
            self.assertTrue(full['decision_kind'])
            from_dose = dict(screen, head_compare=dict(screen['head_compare'], second_source=dose.as_posix()))
            self.assertEqual(capture.sd1_options(from_dose, False)['head_compare']['second_source'], source)
            inline = dict(screen, head_compare=dict(screen['head_compare'], second_source=source))
            self.assertEqual(capture.sd1_options(inline, False)['head_compare']['second_source'], source)
            panel_pass = capture.sd1_options(screen, True)
            self.assertNotIn('head_compare', panel_pass)
            self.assertTrue(panel_pass['decision_kind'])
            for bad in (dict(screen, head_compare=dict(screen['head_compare'], extra=1)),
                        dict(screen, head_compare=dict(screen['head_compare'], second_source={'x': 1}))):
                with self.assertRaises(SystemExit):
                    capture.sd1_options(bad, False)

    def test_records_keep_the_decision_kind_and_the_head_comparison(self):
        with tempfile.TemporaryDirectory() as d:
            fake_run(Path(d, 'panel'), EPISODES, mutate=labelled())
            fake_run(Path(d, 'arms'), EPISODES, arms=True, mutate=labelled(head_compare=True))
            panel = capture.panel(Path(d, 'panel'), 3, 2)
            recs = capture.convert(Path(d, 'arms'), panel)
            self.assertEqual([r['features']['decision_kind'] for r in recs], ['game'] * 3)
            self.assertEqual(recs[0]['head_compare']['worlds'], 16)


class Stamps(unittest.TestCase):
    def test_an_output_of_another_command_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            fake_run(Path(d, 'arms'), EPISODES, arms=True)
            job = Path(d, 'arms', 'jobs', 'u000000.json')
            self.assertTrue(capture.finished(job.as_posix()))
            job.write_text(json.dumps({'output_directory': Path(d, 'arms', 'runs', 'u000000').as_posix(),
                                       'changed': True}))
            with self.assertRaises(SystemExit):
                capture.finished(job.as_posix())
            with self.assertRaises(SystemExit):
                list(capture.records(Path(d, 'arms')))

    def test_fresh_job_is_pending_and_a_partial_output_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            out = Path(d, 'runs', 'u000000')
            job = Path(d, 'jobs', 'u000000.json')
            job.parent.mkdir(parents=True)
            job.write_text(json.dumps({'output_directory': out.as_posix()}))
            self.assertFalse(capture.finished(job.as_posix()))
            self.assertFalse(out.exists())
            out.mkdir(parents=True)
            with self.assertRaises(SystemExit):
                capture.finished(job.as_posix())


if __name__ == '__main__':
    unittest.main()
