"""Design 6.2's correspondence on synthetic trees: containers, derived digests, the allowed classes and refusals.

The exporter is replaced by a stand-in (the real one is the Rust binary): a "container" here is the magic followed
by the v1 JSON it exports to, with the v2 tree's own hashes inside.
"""
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import checkpoint_container_v1 as container  # noqa: E402
import checkpoint_correspondence_v1 as cc  # noqa: E402


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def compact(value) -> bytes:
    return json.dumps(value, separators=(',', ':'), ensure_ascii=False).encode()


def write(root: Path, rel: str, data: bytes):
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    return {'path': f'/runs/A/native/{rel}', 'sha256': sha(data)}


def tree(root: Path, fmt: str, parent: dict, *, telemetry=0.5, leak=None, bad_digest=False):
    """One iteration: the schedule, an episode, the checkpoint (a container in v2), the update and its receipt."""
    write(root, 'run.json', compact({'config': {'record_format': fmt} if fmt == 'v2' else {}, 'git_commit': 'g'}))
    episodes = [{'id': 'e0', 'seed': 7, 'opponent': {'checkpoint': parent}}]
    command = compact({'mode': 'collect', 'episodes': episodes, 'output_directory': '/runs/A/native/i/a/collect'})
    write(root, 'iterations/000000/attempt-000000/collect-command.json', command)
    episode = write(root, 'iterations/000000/attempt-000000/collect/episode-0000.json',
                    compact({'inference': {'checkpoint_sha256': parent['sha256']}, 'decisions': [1, 2]}))
    v1_checkpoint = compact({'state_sha256': 'f' * 64, 'trajectories': [episode], 'parameters': [1, 2, 3]})
    data = container.MAGIC + v1_checkpoint if fmt == 'v2' else v1_checkpoint
    checkpoint = write(root, 'iterations/000000/attempt-000000/update/checkpoint.json', data)
    update_doc = {'after_state_sha256': 'f' * 64, 'checkpoint': checkpoint, 'update_elapsed_seconds': telemetry}
    if leak:
        update_doc['update_elapsed_seconds'] = leak
    update = write(root, 'iterations/000000/attempt-000000/update/update.json', compact(update_doc))
    digest = sha(compact(episodes))
    write(root, 'iterations/000000/complete.json',
          compact({'episodes_sha256': '0' * 64 if bad_digest else digest, 'update': update,
                   'iteration_wall_seconds': telemetry}))


class CorrespondenceTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.real_export = container.export_v1
        # The stand-in exporter: strip the magic.
        container.export_v1 = lambda path: Path(path).read_bytes()[len(container.MAGIC):]
        self.v1_parent = {'path': '/inputs/parent.ckpt', 'sha256': 'a' * 64}
        self.v2_parent = {'path': '/inputs/parent.ckpt', 'sha256': 'b' * 64}

    def tearDown(self):
        container.export_v1 = self.real_export
        self.tmp.cleanup()

    def receipt(self, **v2_options):
        tree(self.root / 'v1', 'v1', self.v1_parent)
        tree(self.root / 'v2', 'v2', self.v2_parent, telemetry=0.75, **v2_options)
        return cc.correspond(self.root / 'v1', self.root / 'v2', {'b' * 64: 'a' * 64})

    def test_corresponding_trees_pass(self):
        out = self.receipt()
        self.assertTrue(out['ok'], out)
        self.assertEqual(out['kinds']['container'], 1)
        self.assertEqual(out['derived_digests'], 1)
        self.assertTrue(out['after_states_equal'])

    def test_without_the_converter_seed_nothing_downstream_matches(self):
        tree(self.root / 'v1', 'v1', self.v1_parent)
        tree(self.root / 'v2', 'v2', self.v2_parent)
        out = cc.correspond(self.root / 'v1', self.root / 'v2')
        self.assertFalse(out['ok'])
        self.assertTrue(out['unmatched'] or out.get('failure'))

    def test_a_hash_in_a_stripped_field_is_unearned(self):
        out = self.receipt(leak='c' * 64)
        self.assertFalse(out['ok'])
        self.assertIn('iterations/000000/attempt-000000/update/update.json', out['unmatched'])

    def test_a_preimage_that_does_not_reproduce_its_digest_fails(self):
        out = self.receipt(bad_digest=True)
        self.assertFalse(out['ok'])
        self.assertIn('preimage', out['failure'])


if __name__ == '__main__':
    unittest.main()
