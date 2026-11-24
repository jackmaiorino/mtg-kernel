"""Split the workspace library's unit tests into parts, each selecting its tests by filters libtest checks.

The default-feature library test binary holds about 4,200 tests. A part cannot name its tests exactly: about
1,000 names of some 70 characters do not fit one Windows command line, and a part split into several
processes run one after another waits at each process boundary for that process's longest test. A part
therefore passes libtest a few hundred substring filters, as libtest itself takes them: a test runs when any
filter is a substring of its name (several filters are or-ed). A filter can thus select tests of another part
(`alpha::` also selects `beta_alpha::one`), so filters are grouped into units:

- every test starts with the filter of its top-level module (`module::`), or its whole name without one;
- filters whose selections share a test are one unit, so a unit's selection is closed: every test any of its
  filters selects belongs to the unit, and no other unit's filter selects it;
- a unit heavier than one part's share of one test thread is split by replacing its heaviest module filter
  with that module's child filters (`module::child::`, or the whole name of a test directly in it), and the
  units are formed again. Whole names are never split further.

Units go longest first to the part whose simulated wall time grows least. The simulation is libtest's own
schedule: names in sorted order, each started when one of THREADS test threads frees, timed from
rust_workspace_lib_seconds.json. The times only balance the parts; they never decide which test runs.

check_partition() is the rule every part enforces before it runs anything: the selections of all parts,
as the binary lists them for each part's filters, hold every test exactly once.
"""

from __future__ import annotations

import argparse
import bisect
import heapq
import json
import platform
import re
from pathlib import Path
from typing import Callable, Iterable

SECONDS_PATH = Path(__file__).with_name("rust_workspace_lib_seconds.json")
# Test threads per part, as on GitHub's hosted Linux and Windows runners (four CPUs).
THREADS = 4


def load_weights(system: str | None = None, path: Path = SECONDS_PATH) -> Callable[[str], float]:
    """Seconds per test on `system` (default: this one); a test the table does not name weighs its `other`."""
    table = json.loads(path.read_text(encoding="utf-8"))
    systems = table["systems"]
    entry = systems.get(system or platform.system()) or systems["Linux"]
    recorded, other = entry["seconds"], entry["other"]
    if not isinstance(other, (int, float)) or other < 0 or any(
            not isinstance(value, (int, float)) or value < 0 for value in recorded.values()):
        raise ValueError(f"{path.name}: every time must be a nonnegative number of seconds")
    return lambda name: recorded.get(name, other)


def selected_by(filters: Iterable[str], tests: Iterable[str]) -> list[str]:
    """The tests libtest runs for these filters: a filter selects every name it is a substring of."""
    filters = list(filters)
    return [test for test in tests if any(item in test for item in filters)]


def top_filter(name: str) -> str:
    return name.split("::", 1)[0] + "::" if "::" in name else name


def child_filters(prefix: str, ordered: list[str]) -> set[str]:
    """`prefix` (a module filter) replaced by its submodules' filters and the whole names of its own tests."""
    children = set()
    for test in ordered[bisect.bisect_left(ordered, prefix):]:
        if not test.startswith(prefix):
            break
        rest = test[len(prefix):]
        children.add(prefix + rest.split("::", 1)[0] + "::" if "::" in rest else test)
    return children


def plan_units(tests: list[str], weight: Callable[[str], float], cap: float) -> list[tuple[list[str], list[str]]]:
    """[(filters, tests)] units whose selections are disjoint and together hold every test."""
    ordered = sorted(tests)
    if len(set(ordered)) != len(ordered):
        raise ValueError("The binary lists a test name twice")
    blob = "\n" + "\n".join(ordered) + "\n"
    starts = [0] * len(ordered)
    position = 1
    for index, test in enumerate(ordered):
        starts[index] = position
        position += len(test) + 1
    cache: dict[str, list[int]] = {}

    def selection(item: str) -> list[int]:
        # The indices of the names `item` is a substring of (a name holds no newline).
        if item not in cache:
            found, at = set(), blob.find(item)
            while at != -1:
                found.add(bisect.bisect_right(starts, at) - 1)
                at = blob.find(item, at + 1)
            cache[item] = sorted(found)
        return cache[item]

    filters = {top_filter(test) for test in ordered}
    while True:
        parent = list(range(len(ordered)))

        def root(index: int) -> int:
            while parent[index] != index:
                parent[index] = parent[parent[index]]
                index = parent[index]
            return index

        for item in filters:
            first, *rest = selection(item)
            for index in rest:
                parent[root(index)] = root(first)
        grouped: dict[int, list[str]] = {}
        for item in sorted(filters):
            grouped.setdefault(root(selection(item)[0]), []).append(item)
        units = []
        for members in grouped.values():
            indices = sorted({index for item in members for index in selection(item)})
            units.append((members, [ordered[index] for index in indices]))
        heavy = [(sum(map(weight, names)), members) for members, names in units
                 if any(item.endswith("::") for item in members) and sum(map(weight, names)) > cap]
        if not heavy:
            return units
        _, members = max(heavy, key=lambda entry: (entry[0], entry[1]))
        split = max((item for item in members if item.endswith("::")),
                    key=lambda item: (sum(weight(ordered[index]) for index in selection(item)), item))
        filters.remove(split)
        filters |= child_filters(split, ordered)


def simulated_wall(tests: Iterable[str], weight: Callable[[str], float], threads: int = THREADS) -> float:
    """Seconds one libtest process takes: names in sorted order, each started on the first free thread."""
    free = [0.0] * threads
    for name in sorted(tests):
        start = heapq.heappop(free)
        heapq.heappush(free, start + weight(name))
    return max(free)


def plan_parts(tests: list[str], count: int, weight: Callable[[str], float],
               threads: int = THREADS) -> tuple[list[list[str]], list[float]]:
    """(filters of each part, simulated seconds of each part) for `count` parts of `tests`."""
    if count < 1:
        raise ValueError("Part count must be at least 1")
    cap = sum(map(weight, tests)) / (count * threads)
    units = plan_units(tests, weight, cap)
    if len(units) < count:
        # A part without filters would run every test.
        raise ValueError(f"{len(units)} units cannot fill {count} parts")
    members: list[list[str]] = [[] for _ in range(count)]
    filters: list[list[str]] = [[] for _ in range(count)]
    walls = [0.0] * count
    for unit_filters, unit_tests in sorted(units, key=lambda unit: (-sum(map(weight, unit[1])), unit[0])):
        trial = [simulated_wall(members[part] + unit_tests, weight, threads) for part in range(count)]
        target = min(range(count), key=lambda part: (trial[part], part))
        members[target] += unit_tests
        filters[target] += unit_filters
        walls[target] = trial[target]
    return [sorted(part) for part in filters], walls


def check_partition(selections: list[list[str]], tests: list[str]) -> None:
    """Fail unless the parts' selections hold every test exactly once and nothing else."""
    counts = dict.fromkeys(tests, 0)
    if len(counts) != len(tests):
        raise ValueError("The binary lists a test name twice")
    for selection in selections:
        if len(set(selection)) != len(selection):
            raise ValueError("A part selects a test twice")
        for test in selection:
            if test not in counts:
                raise ValueError(f"A part selects a test outside the plan: {test}")
            counts[test] += 1
    wrong = sorted(test for test, seen in counts.items() if seen != 1)
    if wrong:
        raise ValueError(f"{len(wrong)} test(s) not in exactly one part: {wrong[:20]}")


# A job log line: an optional runner timestamp, then libtest's output.
LOG_LINE = re.compile(r"^(?:(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z )?(.*)$")
ESCAPE = re.compile(r"\x1b\[[0-9;]*m")
RESULT = re.compile(r"^test (\S+)(?: - should panic)? \.\.\. (ok|FAILED|ignored)\b")
SLOW = re.compile(r"^test (\S+) has been running for over 60 seconds$")


def seconds_from_log(lines: Iterable[str], threads: int = THREADS) -> dict[str, float]:
    """Seconds of each test that ran in the first libtest process of a timestamped job log.

    libtest starts tests in name order whenever one of its threads frees, so the k-th test (counting from
    zero) starts when the (k - threads)-th result line arrives, and the first `threads` start with the run.
    The "running for over 60 seconds" lines check the thread count: each must come 60 s after its start.
    """
    from datetime import datetime

    started = None
    done: list[tuple[float, str, str]] = []
    slow: dict[str, float] = {}
    for raw in lines:
        match = LOG_LINE.match(ESCAPE.sub("", raw.rstrip("\r\n")))
        stamp, body = match.group(1), match.group(2)
        if stamp is None:
            continue
        moment = datetime.fromisoformat(stamp[:26]).timestamp()
        if started is None:
            if re.fullmatch(r"running \d+ tests?", body):
                started = moment
            continue
        if body.startswith("test result:"):
            break
        result = RESULT.match(body)
        if result:
            done.append((moment, result.group(1), result.group(2)))
        elif SLOW.match(body):
            slow[SLOW.match(body).group(1)] = moment
    if started is None or not done:
        raise ValueError("No libtest run in the log")
    names = sorted(name for _, name, _ in done)
    begin = {name: started for name in names[:threads]}
    for index, (moment, _, _) in enumerate(done):
        if threads + index < len(names):
            begin[names[threads + index]] = moment
    off = [name for name, moment in slow.items() if abs(moment - begin[name] - 60) > 2]
    if off:
        raise ValueError(f"The log does not fit {threads} test threads: {off[:5]}")
    return {name: moment - begin[name] for moment, name, status in done if status != "ignored"}


def table_entry(seconds: dict[str, float], source: str, threshold: float) -> dict:
    """A system's entry: tests at or over `threshold` by name, the mean of the others as `other`."""
    rest = [value for value in seconds.values() if value < threshold]
    return {"source": source, "other": round(sum(rest) / len(rest), 3) if rest else 0.0,
            "seconds": {name: round(value, 1) for name, value in sorted(seconds.items()) if value >= threshold}}


def main() -> int:
    parser = argparse.ArgumentParser(description="Refresh one system's times in rust_workspace_lib_seconds.json "
                                                 "from a hosted job log of the unsplit workspace library tests")
    parser.add_argument("log", type=Path)
    parser.add_argument("--system", required=True, choices=["Linux", "Windows"])
    parser.add_argument("--source", required=True, help="Which job the log is, for the table")
    parser.add_argument("--threshold", type=float, default=1.0)
    args = parser.parse_args()
    seconds = seconds_from_log(args.log.read_text(encoding="utf-8", errors="replace").splitlines())
    table = json.loads(SECONDS_PATH.read_text(encoding="utf-8"))
    table["systems"][args.system] = table_entry(seconds, args.source, args.threshold)
    SECONDS_PATH.write_text(json.dumps(table, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    print(f"{args.system}: {len(seconds)} tests, {len(table['systems'][args.system]['seconds'])} named")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
