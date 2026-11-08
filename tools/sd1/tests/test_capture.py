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
                        'frozen_choice': 1, 'pick_v2': 1, 'visits': 16,
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


if __name__ == '__main__':
    unittest.main()
