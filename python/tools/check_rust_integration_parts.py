"""Check that the parts of a suite together executed exactly the tests of its binary.

A suite run in parts builds once: the build job writes the names of the
binary's tests (run_rust_integration_tests.py --list-only), and every part
writes the names of the tests it executed (--executed-list). This compares
them after the parts end. It fails unless each part 0..count-1 reported once,
every part saw the build job's list, no test ran in two parts, and the union of
the executed names equals that list. A module or test that is renamed, added
or left out of the plan therefore cannot drop out of the run unnoticed.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def newest(records: list[dict]) -> dict:
    """The record of the latest run attempt (a rerun keeps earlier attempts' files)."""
    return max(records, key=lambda record: record.get("attempt", 0))


def check(listed: list[str], records: list[dict], count: int) -> list[str]:
    """The reasons the parts do not account for the list; empty when they do."""
    problems = []
    if not listed:
        problems.append("the build job listed no tests")
    if len(set(listed)) != len(listed):
        problems.append("the build job's list names a test twice")
    by_shard: dict[int, list[dict]] = {}
    for record in records:
        shard = record.get("shard")
        if not isinstance(shard, list) or len(shard) != 2 or shard[1] != count or not 0 <= shard[0] < count:
            problems.append(f"a part reported shard {shard!r}, expected an index of {count} parts")
            continue
        by_shard.setdefault(shard[0], []).append(record)
    missing_parts = sorted(set(range(count)) - set(by_shard))
    if missing_parts:
        problems.append(f"no report from part(s) {missing_parts} of {count}")
    owner: dict[str, int] = {}
    for index in sorted(by_shard):
        record = newest(by_shard[index])
        if sorted(record.get("listed", [])) != sorted(listed):
            problems.append(f"part {index} ran a binary with a different test list than the build job's")
        if record.get("failed"):
            problems.append(f"part {index} reported {record['failed']} failed unit(s)")
        executed = record.get("executed", [])
        if len(set(executed)) != len(executed):
            problems.append(f"part {index} executed a test twice")
        for test in set(executed):
            if test in owner:
                problems.append(f"{test} ran in parts {owner[test]} and {index}")
            owner[test] = index
    never = sorted(set(listed) - set(owner))
    unknown = sorted(set(owner) - set(listed))
    if never:
        problems.append(f"{len(never)} listed test(s) ran in no part: {never[:20]}")
    if unknown:
        problems.append(f"{len(unknown)} executed test(s) are not in the list: {unknown[:20]}")
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--listed", type=Path, required=True,
                        help="Directory holding the build job's list file(s), one per run attempt")
    parser.add_argument("--parts", type=Path, required=True,
                        help="Directory holding the executed lists of the parts")
    parser.add_argument("--count", type=int, required=True, help="Number of parts")
    args = parser.parse_args()
    lists = sorted(args.listed.rglob("*.json"))
    if not lists:
        raise ValueError(f"No list file under {args.listed}")
    # Every attempt builds the same commit; the newest file is as good as any.
    listed = json.loads(lists[-1].read_text(encoding="utf-8"))["tests"]
    records = [json.loads(path.read_text(encoding="utf-8")) for path in sorted(args.parts.rglob("*.json"))]
    problems = check(listed, records, args.count)
    for problem in problems:
        print(f"Integration parts: {problem}", flush=True)
    if problems:
        return 1
    print(f"Integration parts: {len(listed)} tests of the binary, each executed in exactly one of "
          f"{args.count} parts", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
