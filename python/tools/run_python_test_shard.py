"""Run one deterministic shard of the Python test suite.

The full suite is discovered exactly like ``python -m unittest discover -s
python/tests``, flattened to individual test cases, and ordered by stable test
id. Shard ``i`` of ``n`` runs exactly the tests whose sorted index is
congruent to ``i`` modulo ``n``; the shards therefore partition the complete
suite with no omission or overlap by construction, and per-test interleaving
keeps long modules balanced across shards. Any discovery error or empty shard
fails closed.
"""

from __future__ import annotations

import argparse
import sys
import unittest


def flatten(suite: unittest.TestSuite) -> list[unittest.TestCase]:
    cases: list[unittest.TestCase] = []
    for entry in suite:
        if isinstance(entry, unittest.TestSuite):
            cases.extend(flatten(entry))
        else:
            cases.append(entry)
    return cases


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shard-index", type=int, required=True)
    parser.add_argument("--shard-count", type=int, required=True)
    parser.add_argument("--tests-dir", default="python/tests")
    arguments = parser.parse_args()

    if arguments.shard_count < 1:
        raise SystemExit("shard-count must be at least 1")
    if not 0 <= arguments.shard_index < arguments.shard_count:
        raise SystemExit("shard-index must be in [0, shard-count)")

    loader = unittest.TestLoader()
    discovered = loader.discover(start_dir=arguments.tests_dir)
    if loader.errors:
        for error in loader.errors:
            print(error, file=sys.stderr)
        raise SystemExit("test discovery reported errors")

    cases = flatten(discovered)
    if not cases:
        raise SystemExit(f"no tests discovered under {arguments.tests_dir}")
    for case in cases:
        if case.__class__.__module__.startswith("unittest.loader"):
            raise SystemExit(f"discovery produced a loader failure: {case.id()}")

    cases.sort(key=lambda case: case.id())
    selected = [
        case
        for index, case in enumerate(cases)
        if index % arguments.shard_count == arguments.shard_index
    ]
    if not selected:
        raise SystemExit(
            f"shard {arguments.shard_index}/{arguments.shard_count} selected no tests"
        )

    print(
        f"shard {arguments.shard_index}/{arguments.shard_count}: "
        f"{len(selected)} of {len(cases)} tests"
    )

    suite = unittest.TestSuite(selected)
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
