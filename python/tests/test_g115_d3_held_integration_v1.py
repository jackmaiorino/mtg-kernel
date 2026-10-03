"""Offline runner integration: real receipt writes, no native execution."""
from contextlib import ExitStack
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
import g115_d3_launch_v1 as runner


class HeldIntegrationTests(unittest.TestCase):
    def exercise(self, failure=False):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as stack:
            root = Path(directory) / 'shard'
            jobs = [dict(id=name, request='request.json', native_output_directory=str(root / name))
                    for name in ('first', 'replacement')]
            allocation = dict(workers=1, command_prefix=['engine.exe'], reserve_bytes=0,
                              shard_timeout_seconds=30, job_timeout_seconds=10)
            stack.enter_context(patch.object(runner, 'validate', return_value=(
                dict(jobs=jobs), dict(jobs=jobs, models={}), allocation, [j['id'] for j in jobs])))
            stack.enter_context(patch.object(runner, 'prepare_output_parent'))
            stack.enter_context(patch.object(runner, 'checked', return_value=Path('request.json')))
            stack.enter_context(patch.object(runner, 'free_memory', return_value=1))
            stack.enter_context(patch.object(runner.shutil, 'disk_usage', return_value=Mock(free=32 * 2**30)))
            stack.enter_context(patch.dict(runner.os.environ, {}, clear=True))
            children = [Mock(pid=100+i, returncode=0) for i in range(2)]
            for child in children:
                child.poll.return_value = 0
            policies = [dict(verified=True, child=i) for i in range(2)]
            spawn = stack.enter_context(patch.object(runner, 'spawn_held',
                side_effect=ValueError('policy readback refused') if failure else list(zip(children, policies))))
            match = stack.enter_context(patch.object(runner, 'read_match',
                return_value=dict(sha256='a'*64, games=3, decisions=5)))
            result = runner.launch({}, 'jack', root)
            saved = json.loads((root / 'completion.json').read_text())
            self.assertEqual(result, saved)
            if failure:
                self.assertFalse(saved['complete'])
                self.assertEqual(spawn.call_count, 1)
                match.assert_not_called()
                self.assertEqual(saved['not_started'], ['replacement'])
                self.assertEqual(saved['rows'][0]['error'], 'policy readback refused')
                self.assertFalse(saved['rows'][0]['complete'])
            else:
                self.assertTrue(saved['complete'])
                self.assertEqual(spawn.call_count, 2)
                self.assertEqual([row['placement'] for row in saved['rows']], policies)
                self.assertEqual(saved['not_started'], [])
                self.assertEqual(match.call_count, 2)

    def test_each_replacement_retains_its_policy_receipt(self):
        self.exercise()

    def test_policy_failure_retained_and_next_job_not_spawned(self):
        self.exercise(failure=True)


if __name__ == '__main__':
    unittest.main()
