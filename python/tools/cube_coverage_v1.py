"""Measure engine card coverage of a cube list.

Input tooling only: no engine build, process or network. A cube list
(`data/cube_v1/*.json`, schema `cube_list/v1`) names each card once. Every
card is classified against the `eternal-fixtures` build, whose registry is the
Pauper prefix, the Standard registry and `data/eternal_v1/cards_v1.json`:

- ``full``: Full in that build.
- ``partial``: defined in that build, but not Full.
- ``fdn_only``: Full only in the FDN registry. Its program exists, so adding
  it to the eternal registry is a copy, not new rules work.
- ``missing``: no definition in that build and no Full FDN definition.

`--check` compares the cards that are not Full with a committed record, so a
change that adds or loses cube coverage has to update that record.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys
from typing import Any

import eternal_meta_census_v1 as census


STATUSES = ("full", "partial", "fdn_only", "missing")
GAPS = STATUSES[1:]


def capabilities() -> tuple[dict[str, str], set[str]]:
    """Each eternal-build card's best capability, and the Full FDN names."""
    best: dict[str, str] = {}
    for path in census.ETERNAL_REGISTRIES:
        for card in json.loads(path.read_text(encoding="utf-8"))["cards"]:
            capability = card.get("engine_capability")
            if capability == "full" or card["name"] not in best:
                best[card["name"]] = capability
    return best, census.full_names(census.FDN_REGISTRY)


def classify(name: str, eternal: dict[str, str], fdn_full: set[str]) -> str:
    capability = eternal.get(name)
    if capability == "full":
        return "full"
    if name in fdn_full:
        return "fdn_only"
    return "missing" if capability is None else "partial"


def coverage(cube: dict[str, Any], eternal: dict[str, str], fdn_full: set[str]) -> dict[str, Any]:
    names = [card["name"] for card in cube["cards"]]
    if len(set(names)) != len(names):
        raise ValueError("cube list names a card twice")
    by_status: dict[str, list[str]] = {status: [] for status in STATUSES}
    by_group: dict[str, dict[str, int]] = {}
    for card in cube["cards"]:
        status = classify(card["name"], eternal, fdn_full)
        by_status[status].append(card["name"])
        group = by_group.setdefault(card.get("group", ""), {key: 0 for key in STATUSES})
        group[status] += 1
    return {
        "schema": "cube_coverage/v1",
        "cube": cube["cube"],
        "effective": cube["effective"],
        "cards": len(names),
        "card_status": {status: len(by_status[status]) for status in STATUSES},
        "groups": by_group,
        **{status: sorted(by_status[status]) for status in GAPS},
    }


def summary(report: dict[str, Any]) -> str:
    status = report["card_status"]
    return (f"{report['cube']} ({report['effective']}): {status['full']} of {report['cards']} Full, "
            f"{status['partial']} partial, {status['fdn_only']} FDN only, {status['missing']} missing")


def markdown(report: dict[str, Any]) -> str:
    lines = [summary(report) + ".", "",
             "| Group | Cards | Full | Partial | FDN only | Missing |",
             "| --- | ---: | ---: | ---: | ---: | ---: |"]
    for group, status in report["groups"].items():
        lines.append(f"| {group} | {sum(status.values())} | {status['full']} | {status['partial']} | "
                     f"{status['fdn_only']} | {status['missing']} |")
    for status in GAPS:
        if report[status]:
            lines += ["", f"### {status}", ""] + [f"- {name}" for name in report[status]]
    return "\n".join(lines) + "\n"


def differences(report: dict[str, Any], expected: dict[str, Any]) -> list[str]:
    """Rows where the cards that are not Full differ from the committed record."""
    rows = []
    for status in GAPS:
        actual, recorded = set(report[status]), set(expected.get(status, []))
        rows += [f"{status}: {name} is not in the record" for name in sorted(actual - recorded)]
        rows += [f"{status}: {name} is recorded but no longer {status}" for name in sorted(recorded - actual)]
    return rows


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("cube", type=Path)
    parser.add_argument("--json-out", type=Path)
    parser.add_argument("--markdown", action="store_true")
    parser.add_argument("--check", type=Path, help="committed record of the cards that are not Full")
    args = parser.parse_args(argv)
    report = coverage(json.loads(args.cube.read_text(encoding="utf-8")), *capabilities())
    if args.json_out:
        args.json_out.write_text(json.dumps(report, indent=1) + "\n", encoding="utf-8")
    sys.stdout.write(markdown(report) if args.markdown else summary(report) + "\n")
    if args.check:
        rows = differences(report, json.loads(args.check.read_text(encoding="utf-8")))
        for row in rows:
            print(row, file=sys.stderr)
        return int(bool(rows))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
