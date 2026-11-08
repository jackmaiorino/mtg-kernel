"""Receipt-binding unit cases only; these do not qualify a native process run."""
from copy import deepcopy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
from qualify_reveal_v4_process import bind_live_point, binds_committed_path, search_receipt


class SearchCommitBindingTests(unittest.TestCase):
    def setUp(self):
        self.context = {"game": "native-p0", "acting_seat": "p0", "group_id": 7,
                        "substep_index": 0, "step": 12}
        self.keys = ["a" * 64, "b" * 64]
        self.request = {"root_id": "root", "budget": 256, "a_units": 1,
                        "payload": {"sha256": "c" * 64}, "path": [1]}
        self.response = {"ok": True, "search": {
            "root_id": "root", "payload_sha256": "c" * 64, "units": 256,
            "charge": 257, "pick": 0, "incomplete": False,
            "menu_sha256": "d" * 64, "excluded": [], "tree_digests": ["e" * 64],
            "merged": [{"n": 1, "wins": 1, "draws": 0}, {"n": 0, "wins": 0, "draws": 0}]}}
        self.point = {"viewer_decision": True, "eligible": True, "keys": self.keys}
        self.commit = {"payload_sha256": "c" * 64,
                       "context": {**self.context, "substep_index": 2, "step": 14},
                       "wire": [2, 0, 1], "path": [1, 0],
                       "expected_keys": ["f" * 64, self.keys[0]],
                       "points": [{"keys": ["0" * 64, "f" * 64], "selected_key": "f" * 64},
                                  {"keys": self.keys, "selected_key": self.keys[0]}]}

    def receipt(self, point=True):
        return search_receipt(self.request, self.response,
                              self.point if point else None, self.context)

    def test_pick_on_the_committed_prefix_binds_across_neutral_substeps(self):
        self.assertTrue(binds_committed_path(self.receipt(), self.commit))

    def test_live_row_permutation_maps_to_the_real_imported_prefix(self):
        point = bind_live_point(list(reversed(self.keys)), 1, self.point)
        self.assertEqual(point["live_pick"], 1)
        self.assertEqual(point["pick"], 0)
        self.assertEqual(point["selected_key"], self.keys[0])
        self.commit["points"][1] = point
        self.commit["path"][1] = point["pick"]
        self.assertTrue(binds_committed_path(self.receipt(), self.commit))

    def test_ambiguous_or_changed_live_menu_cannot_map_an_imported_pick(self):
        for keys in ([self.keys[0], self.keys[0]], [self.keys[0], "9" * 64]):
            with self.subTest(keys=keys), self.assertRaises(AssertionError):
                bind_live_point(keys, 0, self.point)

    def test_same_payload_same_depth_different_path_does_not_bind(self):
        self.request["path"] = [0]
        self.assertFalse(binds_committed_path(self.receipt(), self.commit))

    def test_unmapped_success_then_different_fallback_commit_does_not_bind(self):
        self.response["search"]["pick"] = 1
        row = self.receipt()
        self.assertTrue(row["successful"])
        self.assertEqual(row["selected_key"], self.keys[1])
        self.assertFalse(binds_committed_path(row, self.commit))

    def test_matching_pick_index_with_different_point_keys_does_not_bind(self):
        self.point["keys"] = list(reversed(self.keys))
        self.assertFalse(binds_committed_path(self.receipt(), self.commit))

    def test_matching_selected_key_with_changed_other_row_does_not_bind(self):
        self.point["keys"] = [self.keys[0], "9" * 64]
        self.assertFalse(binds_committed_path(self.receipt(), self.commit))

    def test_same_payload_search_in_another_group_does_not_bind(self):
        self.context["group_id"] += 1
        self.assertFalse(binds_committed_path(self.receipt(), self.commit))

    def test_missing_real_point_cannot_establish_a_committed_key(self):
        self.assertFalse(binds_committed_path(self.receipt(point=False), self.commit))

    def test_capped_or_zero_work_incomplete_results_are_diagnostic_only(self):
        self.response["search"].update(incomplete=True, pick=None, excluded=[0])
        for units in (256, 0):
            with self.subTest(units=units):
                self.response["search"].update(units=units, charge=units + 1)
                row = self.receipt()
                self.assertFalse(row["successful"])
                self.assertIsNone(row["selected_key"])
                self.assertEqual(row["units"], units)
                self.assertFalse(binds_committed_path(row, self.commit))

    def test_incomplete_result_cannot_advertise_a_pick(self):
        self.response["search"]["incomplete"] = True
        with self.assertRaises(AssertionError):
            self.receipt()

    def test_accounting_and_original_completion_are_still_checked(self):
        corrupt = deepcopy(self.commit)
        corrupt["expected_keys"][1] = self.keys[1]
        self.assertFalse(binds_committed_path(self.receipt(), corrupt))
        self.response["search"]["charge"] -= 1
        with self.assertRaises(AssertionError):
            self.receipt()


if __name__ == "__main__":
    unittest.main()
