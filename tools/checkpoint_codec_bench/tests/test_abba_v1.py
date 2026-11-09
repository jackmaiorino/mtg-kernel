"""The checkpoint codec benchmark's A-B-B-A runner and comparison, with fake benchmark binaries."""
import itertools
import json
import math
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import abba_v1 as abba  # noqa: E402

# A stand-in for `checkpoint_codec_bench_v1 measure`: writes a receipt whose
# stage times are SCALE times the baseline's and whose run-to-run variation
# depends on the attempt index in the label.
FAKE_BENCH = r'''
import json, sys
SCALE = {scale}
PUBLISHED = {published!r}
FAIL = {fail}
args = sys.argv[1:]
assert args[0] == "measure"
options = dict(zip(args[1::2], args[2::2]))
if FAIL:
    sys.exit(3)
attempt = int(options["--label"].rsplit(" ", 1)[1])
jitter = {{0: 1.00, 1: 1.02, 2: 0.99, 3: 1.01}}[attempt % 4]
def summary(value):
    return {{"median": value, "q1": value, "q3": value, "min": value, "max": value}}
receipt = {{
    "schema": "mtg-kernel-checkpoint-codec-bench/v1",
    "config": {{"sections": ["stages"], "reps": 3, "public_pair": options.get("--public-pair")}},
    "fixture": {{"manifest_sha256": "f" * 64}},
    "checks": [{{"state": "warmed", "check": "fake", "after_state_sha256": "a" * 64, "passed": True}}],
    "published": [{{"state": "warmed", "cell": "v2", "sha256": PUBLISHED, "bytes": 10}}],
    "series": [{{"section": "stages", "state": "warmed", "cell": "v2", "stage": "encode",
                 "summary": {{"n": 3, "wall_ns": summary(1e6 * SCALE * jitter),
                              "allocated_bytes": summary(4096.0)}}}}],
    "section_seconds": {{"stages": 2.0 * SCALE}},
    "process_peak_rss_bytes": 1 << 30,
}}
with open(options["--out"], "w") as stream:
    json.dump(receipt, stream)
'''


def receipt(wall_ns, published="b" * 64, passed=True, reps=3, after="a" * 64):
    return {
        "schema": abba.RECEIPT_SCHEMA,
        "config": {"sections": ["stages"], "reps": reps, "public_pair": "/somewhere"},
        "fixture": {"manifest_sha256": "f" * 64},
        "checks": [{"section": "updates", "state": "warmed", "check": "fake", "after_state_sha256": after,
                    "passed": passed}],
        "published": [{"state": "warmed", "cell": "v2", "sha256": published, "bytes": 10}],
        "series": [{"section": "stages", "state": "warmed", "cell": "v2", "stage": "encode",
                    "summary": {"n": 3, "wall_ns": {"median": wall_ns}}}],
        "section_seconds": {"stages": 1.0},
        "process_peak_rss_bytes": 100,
    }


def entries(baseline, candidate, **candidate_fields):
    order = []
    for index, role in enumerate(abba.abba_order(len(baseline) // 2)):
        source = baseline if role == "baseline" else candidate
        value = source[sum(1 for item in order if item[0] == role)]
        fields = candidate_fields if role == "candidate" else {}
        order.append((role, receipt(value, **fields), f"attempt {index}"))
    return order


class ComparisonTests(unittest.TestCase):
    def test_the_order_is_balanced_baseline_candidate_candidate_baseline(self):
        self.assertEqual(abba.abba_order(2), ["baseline", "candidate", "candidate", "baseline"] * 2)
        with self.assertRaises(ValueError):
            abba.abba_order(0)

    def test_the_chance_of_a_separation_counts_the_orderings(self):
        for n, m in ((1, 1), (2, 2), (3, 2), (3, 3), (4, 4)):
            separated = 0
            orderings = 0
            for order in set(itertools.permutations("b" * n + "c" * m)):
                orderings += 1
                text = "".join(order)
                separated += text in ("b" * n + "c" * m, "c" * m + "b" * n)
            self.assertAlmostEqual(abba.chance_separation(n, m), separated / orderings)
        self.assertAlmostEqual(abba.chance_separation(2, 2), 1 / 3)
        self.assertAlmostEqual(abba.chance_separation(6, 6), 2 / math.comb(12, 6))
        self.assertEqual(abba.rounds_needed(0.01), 3)
        self.assertEqual(abba.rounds_needed(0.05), 2)

    def test_only_a_difference_beyond_every_run_is_reported(self):
        baseline = [100, 104, 101, 103, 102, 100]
        lower = abba.compare(entries(baseline, [80, 82, 81, 83, 80, 82]))
        self.assertTrue(lower["decidable"])
        result = lower["rows"][0]["metrics"]["wall_ns"]
        self.assertEqual(result["verdict"], "candidate_lower")
        self.assertEqual(result["baseline_median"], 101.5)
        self.assertAlmostEqual(result["ratio"], 81.5 / 101.5)
        self.assertAlmostEqual(result["chance_separation"], 2 / 924)
        overlap = abba.compare(entries(baseline, [80, 82, 81, 83, 80, 101]))
        self.assertEqual(overlap["rows"][0]["metrics"]["wall_ns"]["verdict"], "inconclusive")
        higher = abba.compare(entries(baseline, [110, 111, 110, 112, 113, 110]))
        self.assertEqual(higher["rows"][0]["metrics"]["wall_ns"]["verdict"], "candidate_higher")
        table = abba.format_table(higher)
        self.assertIn("6 baseline and 6 candidate runs", table.splitlines()[0])
        self.assertIn("1 of 3 rows called", table.splitlines()[-1])

    def test_too_few_runs_never_decide(self):
        for rounds, alpha in ((1, abba.ALPHA), (2, abba.ALPHA), (1, 0.05)):
            baseline = [100 + index for index in range(2 * rounds)]
            candidate = [50 + index for index in range(2 * rounds)]
            comparison = abba.compare(entries(baseline, candidate), alpha)
            self.assertFalse(comparison["decidable"])
            self.assertEqual(comparison["rows"][0]["metrics"]["wall_ns"]["verdict"], "inconclusive")
            self.assertEqual(comparison["process"]["process_peak_rss_bytes"]["verdict"], "inconclusive")
            self.assertIn(f"--rounds {abba.rounds_needed(alpha)} or more", abba.format_table(comparison))
        two_rounds = abba.compare(entries([100, 101, 102, 103], [50, 51, 52, 53]), 0.05)
        self.assertEqual(two_rounds["rows"][0]["metrics"]["wall_ns"]["verdict"], "candidate_lower")

    def test_different_published_bytes_are_refused(self):
        with self.assertRaisesRegex(ValueError, "published bytes differ"):
            abba.compare(entries([100, 104], [80, 82], published="c" * 64))

    def test_a_different_real_update_state_is_refused(self):
        with self.assertRaisesRegex(ValueError, "updates warmed after_state_sha256 differs"):
            abba.compare(entries([100, 104], [80, 82], after="d" * 64))

    def test_a_failed_in_run_check_is_refused(self):
        with self.assertRaisesRegex(ValueError, "in-run checks failed"):
            abba.compare(entries([100, 104], [80, 82], passed=False))

    def test_other_measurement_settings_are_refused(self):
        with self.assertRaisesRegex(ValueError, "other settings"):
            abba.compare(entries([100, 104], [80, 82], reps=4))

    def test_both_roles_are_required(self):
        with self.assertRaisesRegex(ValueError, "baseline and candidate"):
            abba.compare([("baseline", receipt(1), "only")])


class MachineTests(unittest.TestCase):
    def test_macos_records_its_cpu_memory_and_linker(self):
        calls = []

        def output(command):
            calls.append(command)
            return " ".join(command)

        with mock.patch.object(abba.sys, "platform", "darwin"), \
                mock.patch.object(abba, "command_output", output):
            record = abba.machine()
            tools = abba.toolchain()
        self.assertEqual(record["sysctl"]["machdep.cpu.brand_string"], "sysctl -n machdep.cpu.brand_string")
        self.assertEqual(record["sysctl"]["hw.memsize"], "sysctl -n hw.memsize")
        self.assertEqual(tools["ld"], "ld -v")
        self.assertIn(["ld", "-v"], calls)


class RunTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.fixture = self.root / "fixture"
        self.fixture.mkdir()
        (self.fixture / "fixture.json").write_text("{}\n")

    def tearDown(self):
        self.temporary.cleanup()

    def fake(self, name, scale=1.0, published="b" * 64, fail=False):
        path = self.root / f"{name}.py"
        path.write_text(FAKE_BENCH.format(scale=scale, published=published, fail=fail))
        return path

    def run_abba(self, baseline, candidate, rounds=1, **options):
        binaries = {"baseline": abba.pin(baseline), "candidate": abba.pin(candidate)}
        commands = {"baseline": [sys.executable, str(baseline)], "candidate": [sys.executable, str(candidate)]}
        return abba.run_abba(commands, binaries, self.fixture, self.root / "run", rounds, **options)

    def test_a_run_keeps_every_attempt_in_order_and_compares_them(self):
        manifest, comparison = self.run_abba(self.fake("baseline"), self.fake("candidate", scale=0.5),
                                             rounds=3, measure_args=["--public-pair", "/pair"])
        self.assertEqual(manifest["status"], "compared")
        self.assertFalse(manifest["null_comparison"])
        self.assertEqual([attempt["role"] for attempt in manifest["attempts"]],
                         ["baseline", "candidate", "candidate", "baseline"] * 3)
        self.assertTrue(all(attempt["exit_code"] == 0 for attempt in manifest["attempts"]))
        self.assertIn("--public-pair", manifest["attempts"][0]["argv"])
        on_disk = json.loads((self.root / "run" / "run.json").read_text())
        self.assertEqual(on_disk["comparison"]["sha256"],
                         abba.sha256_file(self.root / "run" / "comparison.json"))
        wall = comparison["rows"][0]["metrics"]["wall_ns"]
        self.assertEqual(wall["verdict"], "candidate_lower")
        self.assertEqual(comparison["rows"][0]["metrics"]["allocated_bytes"]["verdict"], "inconclusive")
        self.assertEqual(comparison["process"]["section_seconds.stages"]["verdict"], "candidate_lower")
        self.assertEqual(comparison["process"]["process_peak_rss_bytes"]["verdict"], "inconclusive")
        table = abba.format_table(comparison)
        self.assertIn("encode", table)
        self.assertIn("peak_rss_mib", table)
        self.assertIn("stages_s", table)

    def test_a_same_binary_run_is_a_null_comparison(self):
        same = self.fake("same")
        manifest, comparison = self.run_abba(same, same, rounds=3)
        self.assertTrue(manifest["null_comparison"])
        self.assertTrue(comparison["decidable"])
        self.assertEqual(comparison["rows"][0]["metrics"]["wall_ns"]["verdict"], "inconclusive")

    def test_a_failed_attempt_stops_the_run_and_keeps_it(self):
        manifest, comparison = self.run_abba(self.fake("baseline"), self.fake("candidate", fail=True))
        self.assertIsNone(comparison)
        self.assertEqual(manifest["status"], "incomplete")
        self.assertEqual([attempt["exit_code"] for attempt in manifest["attempts"]], [0, 3])
        self.assertTrue((self.root / "run" / "attempt-01-candidate.stderr").exists())

    def test_different_bytes_make_the_run_invalid(self):
        manifest, comparison = self.run_abba(self.fake("baseline"), self.fake("candidate", published="c" * 64))
        self.assertIsNone(comparison)
        self.assertEqual(manifest["status"], "invalid")
        self.assertIn("published bytes differ", manifest["error"])

    def test_compare_reads_a_finished_run_back(self):
        self.run_abba(self.fake("baseline"), self.fake("candidate", scale=0.5))
        self.assertEqual(abba.main(["compare", "--run-dir", str(self.root / "run")]), 0)


if __name__ == "__main__":
    unittest.main()
