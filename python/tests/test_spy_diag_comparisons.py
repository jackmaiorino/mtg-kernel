"""Comparison input checks: never label missing/failed evidence as equality."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "tools/spy_diag" / f"{name}.py")
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


compare = module("compare_runs")
views = module("diff_views")


class ComparisonInputs(unittest.TestCase):
    def test_matching_complete_panel_is_admitted(self):
        panel = {"a": {"role": "cast"}, "b": {"role": "self"}}
        compare.validate_panel(panel, panel, dict.fromkeys(panel), dict.fromkeys(panel))

    def test_missing_or_extra_analysis_or_rows_are_refused(self):
        panel = {"a": {"role": "cast"}}
        for index in range(4):
            for wrong in ({}, {**panel, "b": {"role": "self"}}):
                inputs = [panel] * 4
                inputs[index] = wrong
                with self.assertRaises(ValueError):
                    compare.validate_panel(*inputs)

    def test_changed_role_and_empty_panel_are_refused(self):
        with self.assertRaises(ValueError):
            compare.validate_panel({"a": {"role": "cast"}}, {"a": {"role": "self"}}, {"a": {}}, {"a": {}})
        with self.assertRaises(ValueError):
            compare.validate_panel({}, {}, {}, {})

    def test_duplicate_root_across_files_is_refused(self):
        with tempfile.TemporaryDirectory() as folder:
            row = {"kind": "s4a_diag_root", "root_id": "a", "diag": {"trace": "receipt"}}
            for name in ("one.jsonl", "two.jsonl"):
                (Path(folder) / name).write_text(json.dumps(row) + "\n", encoding="utf-8")
            with self.assertRaises(ValueError):
                compare.rows(folder)

    def test_complete_empty_observations_and_menus_are_valid(self):
        row = {"view": {"canon_obs": {}, "canon_menu": [], "raw_obs": {}, "raw_menu": []}}
        self.assertIs(views.validate_capture(row), row)

    def test_failed_or_absent_view_component_is_refused(self):
        complete = {"canon_obs": {}, "canon_menu": [], "raw_obs": {}, "raw_menu": []}
        for part in complete:
            with self.assertRaises(ValueError):
                views.validate_capture({"view": {**complete, part: None}})
            missing = {key: value for key, value in complete.items() if key != part}
            with self.assertRaises(ValueError):
                views.validate_capture({"view": missing})
        with self.assertRaises(ValueError):
            views.validate_capture({})


if __name__ == "__main__":
    unittest.main()
