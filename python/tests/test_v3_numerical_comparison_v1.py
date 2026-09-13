"""Regression checks for the interpretation errors this report must expose."""
import importlib.util
import copy
from pathlib import Path
import struct
import unittest
from unittest.mock import patch

PATH = Path(__file__).resolve().parents[1]/"tools/compare_v3_numerical_probe_v1.py"
SPEC = importlib.util.spec_from_file_location("v3_compare", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

def tensor(value):
    bits = struct.unpack("<I", struct.pack("<f", value))[0]
    return [{"name": f"layer{i}", "shape": [1], "values": [bits]} for i in range(33)]

class ComparisonTests(unittest.TestCase):
    def test_missing_small_update_is_visible_in_delta_error(self):
        initial, cpu, unchanged = tensor(1.0), tensor(1.00001), tensor(1.0)
        final = MODULE.named_metrics(cpu, unchanged, 1e-12)
        delta = MODULE.named_metrics(cpu, unchanged, 1e-12, initial)
        self.assertLess(final[0]["max_abs_error"], 0.002)
        self.assertEqual(delta[0]["normalized_rms_error"], 1.0)

    def test_names_shapes_nonfinite_and_missing_tensors_reject(self):
        for change in [lambda x: x.pop(), lambda x: x[1].update(name="layer0"),
                       lambda x: x[0].update(shape=[2]),
                       lambda x: x[0].update(values=[0x7f800000])]:
            right = tensor(1.0)
            change(right)
            with self.assertRaises(ValueError):
                MODULE.named_metrics(tensor(1.0), right, 1e-12)

    def test_zero_reference_uses_explicit_floor(self):
        result = MODULE.metrics([0.0], [1e-8], 1e-6)
        self.assertTrue(result["normalization_floor_active"])
        self.assertEqual(result["normalized_rms_error"], 0.01)

    def test_policy_comparison_is_gauge_invariant_and_keeps_singletons(self):
        ordinary = MODULE.selected_policy([1.0, -1.0, 17.0], [0, 2, 3], [1, 0])
        shifted = MODULE.selected_policy([101.0, 99.0, -73.0], [0, 2, 3], [1, 0])
        self.assertEqual(ordinary, shifted)
        self.assertEqual(ordinary[0][1], 1.0)
        self.assertEqual(ordinary[1][1], 0.0)

    def test_coherent_report_and_loss_origin_checks(self):
        b = lambda value: struct.unpack("<I", struct.pack("<f", value))[0]
        initial = {"state_sha256": "common", "adam_step": 0, "scorer_bias_anchor_bits": 0,
                   "parameters": tensor(1), "first_moments": tensor(0), "second_moments": tensor(0)}
        after = copy.deepcopy(initial)
        after["adam_step"] = 1
        result = {"adam_step": 1, "loss_bits": b(.1), "gradients": tensor(0)}
        device = {"device_ordinal": 1, "normalization_group_count": 1, "value_coefficient": .5,
                  "action_offsets": [0,1], "group_first_substeps": [0], "substep_group_indices": [0],
                  "selected_action_indices": [0], "terminal_returns": [1],
                  "logit_outputs": [.1], "value_outputs": [.1],
                  "chunks": [{"group_begin":0,"group_end":1,"substep_begin":0,"substep_end":1,"device_objective":.1}],
                  "device_objective_host_sum": MODULE.binary32(.1)}
        inputs = {"value_coefficient_bits": b(.5), "groups": [{"terminal_return":1,"baseline_bits":0,
                  "rows":[{"selected":0,"logits":[b(.1)],"value":b(.1)}]}]}
        out = {"initial.json":initial,"cpu-after.json":after,"cuda-after.json":copy.deepcopy(after),
               "cpu-result.json":dict(result,loss_source="actual-cpu-forward"),
               "cuda-result.json":dict(result,loss_source="transported-cpu-outputs"),
               "cuda-device.json":device,"inputs.json":inputs}
        completion = {"gpu_ordinal":1,"initial_state_sha256":"common"}
        with patch.object(MODULE, "load_outputs", return_value=(completion,out,"fixture")):
            report = MODULE.compare(Path("unused"),1e-12)
        self.assertEqual(report["logits"]["max_abs_error"],0)
        self.assertEqual(report["objective"]["absolute_error"],0)
        corruptions = [lambda x:x["initial.json"].update(state_sha256="different"),
                       lambda x:x["cuda-device.json"].update(value_coefficient=1),
                       lambda x:x["cuda-device.json"].update(chunks=[]),
                       lambda x:x["cuda-device.json"]["chunks"][0].update(substep_end=2),
                       lambda x:x["cuda-device.json"].update(device_objective_host_sum=.1),
                       lambda x:x["cuda-result.json"].update(adam_step=2),
                       lambda x:x["cuda-result.json"]["gradients"][0].update(name="unrelated")]
        for corrupt in corruptions:
            changed = copy.deepcopy(out)
            corrupt(changed)
            with patch.object(MODULE, "load_outputs", return_value=(completion,changed,"fixture")):
                with self.assertRaises(ValueError):
                    MODULE.compare(Path("unused"),1e-12)

if __name__ == "__main__":
    unittest.main()
