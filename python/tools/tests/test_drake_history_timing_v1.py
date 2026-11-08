"""Pure parsing/comparison checks. These never spawn a process or acquire cores."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import drake_history_timing_v1 as timing


def native_rows(case="train_241"):
    split, pair, decks, winner, decisions = timing.CASES[case]
    base = dict(schema="drake-staging-timing/v1", split=split, pair=pair, decks=decks,
                winner=f"PlayerId({winner[1]})", decisions=decisions, seed=1, leg=0,
                final_state_sha256="a" * 64, replay_executions=7)
    windows = [dict(first_decision=i, last_decision=min(i + 99, decisions),
                    elapsed_ns=(min(100, decisions - i + 1) * 10_000_000),
                    history_len=i * 2, drake_rows=i)
               for i in range(1, decisions + 1, 100)]
    return [dict(base, staging=staging, windows=copy.deepcopy(windows)) for staging in ("borrowed", "owned")]


def log(rows, case="train_241"):
    # Actual libtest --nocapture framing: test name and first print share a line.
    return (f"running 1 test\ntest {timing.PREFIX}{case} ... "
            + "\n".join("drake_staging_timing_v1 " + json.dumps(row) for row in rows)
            + "\nok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5900 filtered out; finished in 1.0s\n")


class NativeReceipts(unittest.TestCase):
    def test_real_nocapture_framing_and_partial_window(self):
        rows = timing.reports(log(native_rows()), "train_241")
        self.assertEqual(rows[0]["windows"][-1]["last_decision"], 1344)
        self.assertEqual(rows[0]["windows"][-1]["first_decision"], 1301)

    def test_partial_window_normalizes_by_44_decisions_not_100(self):
        window = native_rows()[0]["windows"][-1]
        value = timing.normalized_window("train_241", "new", "owned", window)
        self.assertEqual(value["decisions"], 44)
        self.assertTrue(value["partial"])
        self.assertEqual(value["seconds_per_100"], 1.0)

    def test_all_declared_cases(self):
        for case in timing.CASES:
            with self.subTest(case=case):
                self.assertEqual(len(timing.reports(log(native_rows(case), case), case)), 2)

    def test_ignored_or_zero_pass_cannot_qualify(self):
        for summary in ("0 passed; 0 failed; 1 ignored;", "0 passed; 0 failed; 0 ignored;",
                        "1 passed; 1 failed; 0 ignored;"):
            with self.subTest(summary=summary), self.assertRaises(ValueError):
                timing.reports(log(native_rows()).replace("1 passed; 0 failed; 0 ignored;", summary), "train_241")

    def test_wrong_test_cannot_qualify(self):
        with self.assertRaises(ValueError):
            timing.reports(log(native_rows()).replace(timing.PREFIX + "train_241", timing.PREFIX + "train_436"), "train_241")

    def test_changed_winner_decisions_or_final_bytes_refuse(self):
        for field, value in (("winner", "P0"), ("decisions", 1343), ("seed", 2),
                             ("final_state_sha256", "b" * 64), ("replay_executions", 0)):
            rows = native_rows()
            rows[1][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                timing.reports(log(rows), "train_241")

    def test_duplicate_staging_and_truncated_windows_refuse(self):
        rows = native_rows()
        rows[1]["staging"] = "borrowed"
        with self.assertRaises(ValueError):
            timing.reports(log(rows), "train_241")
        rows = native_rows()
        rows[1]["windows"].pop()
        with self.assertRaises(ValueError):
            timing.reports(log(rows), "train_241")

    def test_overlapping_nonfinite_and_boolean_windows_refuse(self):
        for field, value in (("first_decision", 99), ("elapsed_ns", -1),
                             ("elapsed_ns", float("inf")), ("history_len", True)):
            rows = native_rows()
            rows[0]["windows"][1][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                timing.reports(log(rows), "train_241")

    def test_pins_detect_mutated_retained_output(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "receipt.json"
            path.write_text("{}")
            reference = timing.pin(path)
            self.assertEqual(timing.checked(reference), path)
            path.write_text('{"changed":true}')
            with self.assertRaises(ValueError):
                timing.checked(reference)


if __name__ == "__main__":
    unittest.main()
