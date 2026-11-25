"""The workspace library plan gives every test to exactly one part, by libtest's substring filters."""
from __future__ import annotations

import json
import random
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import rust_workspace_lib_plan as plan  # noqa: E402


def weights(table, other=1.0):
    return lambda name: table.get(name, other)


def assert_parts(case, filters, tests):
    """Every test in exactly one part's libtest selection, and no part without filters."""
    case.assertTrue(all(filters))
    plan.check_partition([plan.selected_by(part, tests) for part in filters], tests)


def random_names(seed, count=120):
    """Module paths with deliberate substring overlaps: `alpha::` inside `beta_alpha::`, names inside names."""
    rng = random.Random(seed)
    modules = ["alpha", "beta_alpha", "alpha_beta", "gamma", "tests", "delta"]
    leaves = ["one", "one_two", "two", "tests_x", "alpha"]
    names = set()
    while len(names) < count:
        path = [rng.choice(modules) for _ in range(rng.randint(1, 3))]
        names.add("::".join(path + [rng.choice(leaves) + rng.choice(["", "_v1", "_v1_x"])]))
    return sorted(names)


class SelectionTests(unittest.TestCase):
    def test_a_filter_selects_every_name_it_is_a_substring_of_and_filters_are_or_ed(self):
        tests = ["alpha::one", "beta_alpha::one", "alphabet::one", "gamma::one"]
        self.assertEqual(plan.selected_by(["alpha::"], tests), ["alpha::one", "beta_alpha::one"])
        self.assertEqual(plan.selected_by(["alpha::", "gamma::"], tests),
                         ["alpha::one", "beta_alpha::one", "gamma::one"])
        self.assertEqual(plan.selected_by(["delta::"], tests), [])

    def test_module_filters_and_children(self):
        self.assertEqual(plan.top_filter("alpha::beta::one"), "alpha::")
        self.assertEqual(plan.top_filter("root_test"), "root_test")
        ordered = sorted(["alpha::beta::one", "alpha::beta::two", "alpha::one", "alpha::gamma::x::y", "alphabet::one"])
        self.assertEqual(plan.child_filters("alpha::", ordered), {"alpha::beta::", "alpha::one", "alpha::gamma::"})


class UnitTests(unittest.TestCase):
    def test_filters_that_share_a_test_are_one_unit(self):
        # `alpha::` selects beta_alpha::one too, so the two modules cannot go to different parts.
        tests = ["alpha::one", "beta_alpha::one", "gamma::one"]
        units = plan.plan_units(tests, weights({}), cap=1e9)
        self.assertEqual(sorted(units), [(["alpha::", "beta_alpha::"], ["alpha::one", "beta_alpha::one"]),
                                         (["gamma::"], ["gamma::one"])])

    def test_a_heavy_module_is_split_into_its_children(self):
        tests = ["big::a::one", "big::a::two", "big::b::one", "big::leaf", "small::one"]
        units = plan.plan_units(tests, weights({"big::a::one": 50, "big::b::one": 50}), cap=60)
        self.assertEqual(sorted(filters for filters, _ in units),
                         [["big::a::"], ["big::b::"], ["big::leaf"], ["small::"]])

    def test_a_whole_name_inside_another_keeps_both_in_one_unit(self):
        # `big::one` is a substring of `big::one_more`: whatever the weights, they stay together.
        tests = ["big::one", "big::one_more", "big::two"]
        units = plan.plan_units(tests, weights({"big::one": 100, "big::one_more": 100}), cap=1)
        self.assertIn((["big::one", "big::one_more"], ["big::one", "big::one_more"]), units)

    def test_a_listed_name_twice_is_refused(self):
        with self.assertRaisesRegex(ValueError, "twice"):
            plan.plan_units(["a::one", "a::one"], weights({}), cap=1)


class PlanTests(unittest.TestCase):
    def test_overlapping_modules_in_different_parts_would_double_a_test(self):
        # Without units, `alpha::` and `beta_alpha::` could go to different parts and beta_alpha::one
        # would run in both.
        tests = ["alpha::one", "beta_alpha::one", "gamma::one", "delta::one"]
        filters, _ = plan.plan_parts(tests, 2, weights({}), threads=1)
        assert_parts(self, filters, tests)
        (holder,) = [part for part in filters if "alpha::" in part]
        self.assertIn("beta_alpha::", holder)

    def test_every_random_inventory_is_split_into_exact_parts(self):
        planned = 0
        for seed in range(40):
            tests = random_names(seed)
            rng = random.Random(seed)
            table = {name: rng.choice([0.01, 0.5, 3, 40, 300]) for name in tests if rng.random() < 0.7}
            for count in (1, 2, 3, 4):
                with self.subTest(seed=seed, count=count):
                    try:
                        filters, walls = plan.plan_parts(tests, count, weights(table, 2.0))
                    except ValueError as error:
                        # Only when the overlaps leave fewer units than parts.
                        self.assertIn(f"cannot fill {count} parts", str(error))
                        continue
                    planned += 1
                    assert_parts(self, filters, tests)
                    self.assertEqual(len(walls), count)
        self.assertGreater(planned, 120)

    def test_the_heaviest_unit_goes_to_part_0(self):
        tests = ["a::one", "b::one", "c::one", "d::one", "e::slow"]
        filters, walls = plan.plan_parts(tests, 4, weights({"e::slow": 500}))
        self.assertEqual(filters[0], ["e::slow"])
        self.assertEqual(walls[0], max(walls))

    def test_a_part_without_a_unit_is_refused(self):
        # A part with no filters would run every test.
        with self.assertRaisesRegex(ValueError, "cannot fill 3 parts"):
            plan.plan_parts(["alpha::one", "beta_alpha::one"], 3, weights({}))
        with self.assertRaises(ValueError):
            plan.plan_parts(["a::one"], 0, weights({}))

    def test_the_plan_does_not_depend_on_the_list_order(self):
        tests = random_names(7)
        table = {name: (index * 37) % 11 for index, name in enumerate(tests)}
        self.assertEqual(plan.plan_parts(tests, 4, weights(table)), plan.plan_parts(tests[::-1], 4, weights(table)))

    def test_a_slow_test_keeps_a_part_while_the_others_have_room(self):
        tests = ["z::slow"] + [f"a::{index}" for index in range(6)]
        filters, walls = plan.plan_parts(tests, 2, weights({"z::slow": 100}, 10), threads=2)
        self.assertEqual(filters, [["z::slow"], [f"a::{index}" for index in range(6)]])
        self.assertEqual(walls, [100.0, 30.0])

    def test_simulated_wall_follows_name_order_on_each_free_thread(self):
        table = {"a": 10, "b": 1, "c": 1, "d": 5}
        self.assertEqual(plan.simulated_wall(["d", "c", "b", "a"], weights(table), threads=2), 10)
        self.assertEqual(plan.simulated_wall(["a", "b", "c", "d"], weights(table), threads=1), 17)
        self.assertEqual(plan.simulated_wall([], weights(table)), 0)
        # Names start in sorted order: a slow test sorted after two short ones waits for a thread.
        slow = {"a": 1, "b": 1, "z": 10, "y": 10}
        self.assertEqual(plan.simulated_wall(["z", "a", "b"], weights(slow), threads=2), 11)
        self.assertEqual(plan.simulated_wall(["y", "z1", "z2"], weights(slow, 1), threads=2), 10)


class PartitionTests(unittest.TestCase):
    TESTS = ["a::one", "a::two", "b::one"]

    def test_exactly_once_passes(self):
        plan.check_partition([["a::one", "a::two"], ["b::one"]], self.TESTS)

    def test_a_missing_doubled_or_foreign_test_fails(self):
        for selections, message in (([["a::one"], ["b::one"]], "not in exactly one part"),
                                    ([["a::one", "a::two"], ["a::two", "b::one"]], "not in exactly one part"),
                                    ([["a::one", "a::two", "c::one"], ["b::one"]], "outside the plan"),
                                    ([["a::one", "a::one", "a::two"], ["b::one"]], "selects a test twice")):
            with self.subTest(selections=selections), self.assertRaisesRegex(ValueError, message):
                plan.check_partition(selections, self.TESTS)
        with self.assertRaisesRegex(ValueError, "twice"):
            plan.check_partition([["a::one"]], ["a::one", "a::one"])


def log(lines, start="2026-11-24T10:00:00.0000000Z"):
    return [start + " running 4 tests"] + [f"2026-11-24T10:{minute:02d}:{second:02d}.0000000Z {text}"
                                           for minute, second, text in lines] + ["2026-11-24T11:00:00.0Z test result: ok."]


class SecondsFromLogTests(unittest.TestCase):
    # Two threads: a and b start at 0; b ends at 10 s and c starts; a ends at 70 s and d starts; c ends
    # at 80 s, d at 85 s.
    LINES = [(0, 10, "test b ... ok"), (1, 0, "test a has been running for over 60 seconds"),
             (1, 10, "test a ... ok"), (1, 10, "test c has been running for over 60 seconds"),
             (1, 20, "test c ... ok"), (1, 25, "test d ... ignored")]

    def test_each_test_runs_from_its_turn_in_name_order_to_its_result(self):
        self.assertEqual(plan.seconds_from_log(log(self.LINES), threads=2), {"a": 70.0, "b": 10.0, "c": 70.0})

    def test_a_wrong_thread_count_is_refused(self):
        with self.assertRaisesRegex(ValueError, "does not fit 3 test threads"):
            plan.seconds_from_log(log(self.LINES), threads=3)

    def test_colour_codes_and_a_second_process_are_ignored(self):
        lines = log(self.LINES)
        lines[1] = lines[1].replace("test b", "\x1b[1mtest b\x1b[0m")
        lines += ["2026-11-24T11:00:01.0Z running 1 test", "2026-11-24T11:00:02.0Z test e ... ok"]
        self.assertEqual(plan.seconds_from_log(lines, threads=2), {"a": 70.0, "b": 10.0, "c": 70.0})

    def test_a_log_without_a_run_is_refused(self):
        with self.assertRaises(ValueError):
            plan.seconds_from_log(["no tests here"])

    def test_the_table_names_tests_over_the_threshold_and_averages_the_rest(self):
        entry = plan.table_entry({"a": 0.5, "b": 1.5, "c": 0.25, "d": 12.34}, "job 1", 1.0)
        self.assertEqual(entry, {"source": "job 1", "other": 0.375, "seconds": {"b": 1.5, "d": 12.3}})


class TableTests(unittest.TestCase):
    def test_an_unnamed_test_weighs_its_systems_other(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "seconds.json"
            path.write_text(json.dumps({"systems": {"Linux": {"other": 0.5, "seconds": {"a": 3}},
                                                    "Windows": {"other": 0.25, "seconds": {"a": 9}}}}))
            self.assertEqual((plan.load_weights("Windows", path)("a"), plan.load_weights("Windows", path)("b")),
                             (9, 0.25))
            # A system without times of its own uses Linux's.
            self.assertEqual(plan.load_weights("Darwin", path)("a"), 3)
            path.write_text(json.dumps({"systems": {"Linux": {"other": 0.5, "seconds": {"a": -1}}}}))
            with self.assertRaises(ValueError):
                plan.load_weights("Linux", path)

    def test_the_checked_in_table_has_both_hosted_systems(self):
        table = json.loads(plan.SECONDS_PATH.read_text(encoding="utf-8"))
        self.assertEqual(sorted(table["systems"]), ["Linux", "Windows"])
        self.assertEqual(table["threads"], plan.THREADS)
        for system in ("Linux", "Windows"):
            weight = plan.load_weights(system)
            # The longest test on Windows (about 10 minutes), which the plan gives a part of its own there.
            self.assertGreater(weight("infoset_v1::harness::l13_over_the_ac1_engine_families"), 300)
            self.assertLess(weight("an::unrecorded::test"), 1)


if __name__ == "__main__":
    unittest.main()
