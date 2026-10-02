"""Guard rejection and live owned-handle QoS checks; no MTG execution."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from g115_d4_timing_contract_v1 import bind_placement, validate_recipe
from windows_owned_child_policy_v1 import configure_owned_child


def recipe():
    episode = dict(id="probe", seed=1, postboard=False, registered=[{}, {}], selected=[{}, {}])
    return dict(schema="mtg-kernel-native-expanded-training-run/v1",
                iterations=[dict(episodes=[dict(episode=dict(episode, id=str(i), seed=i)) for i in range(10)])],
                loss_selection=dict(kind="gae_advantage_value_v1", gamma=1.0,
                                    **{"lambda": 0.9}, entropy_coefficient=0.0),
                max_non_natural_episode_fraction=0, learning_rate=0.0001,
                value_coefficient=0.5, update_backend=dict(kind="cuda", device_ordinal=1))


class ContractTests(unittest.TestCase):
    def test_refuse_oversized_and_partial_batches(self):
        a = recipe()
        a["iterations"] *= 4
        with self.assertRaises(ValueError):
            validate_recipe(a)
        a = recipe()
        a["iterations"][0]["episodes"].pop()
        with self.assertRaises(ValueError):
            validate_recipe(a)

    def test_refuse_changed_learning_and_duplicate_seed(self):
        a = recipe()
        a["loss_selection"]["entropy_coefficient"] = .01
        with self.assertRaises(ValueError):
            validate_recipe(a)
        a = recipe()
        a["iterations"][0]["episodes"][1]["episode"]["seed"] = 0
        with self.assertRaises(ValueError):
            validate_recipe(a)

    def test_placement_preserves_recipe_and_refuses_existing_output(self):
        a = recipe()
        original = copy.deepcopy(a)
        with tempfile.TemporaryDirectory() as temp:
            b = bind_placement(a, Path(temp)/"fresh", 4, 1)
            self.assertEqual(a, original)
            self.assertEqual(b["iterations"], a["iterations"])
            with self.assertRaises(ValueError):
                bind_placement(a, temp, 4, 1)
            with self.assertRaises(ValueError):
                bind_placement(a, Path(temp)/"fresh", 16, 1)


@unittest.skipUnless(os.name == "nt", "Windows handle test")
class OwnedChildTests(unittest.TestCase):
    def child(self, below=True):
        return subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read()"],
                                stdin=subprocess.PIPE,
                                creationflags=subprocess.CREATE_NO_WINDOW | (subprocess.BELOW_NORMAL_PRIORITY_CLASS if below else subprocess.NORMAL_PRIORITY_CLASS))

    def test_live_child_readback_and_wrong_image_rejection(self):
        child = self.child()
        try:
            with self.assertRaises(ValueError):
                configure_owned_child(child, __file__)
            result = configure_owned_child(child, sys.executable)
            self.assertTrue(result["verified"])
            self.assertEqual(result["after"]["pid"], child.pid)
            self.assertEqual(result["after"]["power_state_mask"] & 1, 0)
            self.assertEqual(result["before"]["affinity_mask"], result["after"]["affinity_mask"])
            print(json.dumps(result))
        finally:
            child.communicate(b"", timeout=10)
        with self.assertRaises(ValueError):
            configure_owned_child(child, sys.executable)

    def test_refuse_non_below_normal_child(self):
        child = self.child(False)
        try:
            with self.assertRaises(ValueError):
                configure_owned_child(child, sys.executable)
        finally:
            child.communicate(b"", timeout=10)


if __name__ == "__main__":
    unittest.main()
