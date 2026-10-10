#!/usr/bin/env python3
"""Compare two `regret_census_v1` search outputs (JSONL) row by row.

Design v3, "Before the formal run", item 3: with no new variable set, a
fixture `cond` row from the current binary must be byte-identical, excluding
wall fields, to the PR #164 output. This tool checks that: rows are paired by
kind and identity (`root_index` for cond/cross rows, game and step for root
rows, game otherwise), wall fields are dropped from both sides, and from the
NEW side only the keys the review-001 tooling adds by default are dropped
(the exact paths in ADDED_KEYS, and the own-turn Strands tag in ADDED_TAGS).
Everything else must match exactly, compared as canonical JSON (sorted
keys, exact number types: 1 and 1.0 differ): every remaining key and value,
and no row may be missing or duplicated. Exit status 0 when all rows
match, 1 on any difference.

ADDED_KEYS are exact paths: a dict key, or `[]` for every element of a list.
The selection conditions are listed by name. No path matches a key at any
other position, so a changed existing field is always reported.

`cond` rows (defaults: BUDGET_STOP, EVAL_CROSS, HORIZON unset):
- top level: `budget_stop`, `config`, `compute`, `spy_labels`;
- `selection.{A,B,C,D}`: `budget_stop`, `cap_hit_playouts`,
  `searched_menu_sum`, `searched_menu_max`;
- `selection.{A,B,C,D}.levels[]`: `spy` (Spy-focal rows only);
- `eval.jobs[]`: `transitions_total`, `cap_hits`, `searched_menu_sum`,
  `searched_menu_max`, `spy` (Spy-focal rows only).

`root` rows (mode `roots`): the new `cawgates_strands_castable_own_turn` tag
(ADDED_TAGS) is removed from `tags`; a row whose tags are then empty had no
tag before, so its `menu` (recorded only for tagged rows) is reset to null.

`cross` rows: the same selection (`b_selection`) and job keys, plus
`horizon_long`, `deviation_cf_transitions` (eval and jobs, with
DEVIATION_EVAL), `deviation_cf_evaluated` and the deviations' `ordinal` and
`counterfactual`. Cross evaluation now has its own seed namespace, so cross
scores are not expected to match the PR #164 output; the keys are listed so
that the report shows those differences and nothing else.

Wall fields (dropped everywhere, both sides): WALL_KEYS. `--wall-only` drops
only these, for comparing two runs of the same binary (determinism across
processes, item 4).

Usage: compare_rows.py OLD.jsonl NEW.jsonl [--wall-only] [--max-report 20]
"""

import argparse
import json
import sys

WALL_KEYS = frozenset({
    "wall",
    "cf_wall",
    "actual_wall",
    "selection_wall",
    "root_wall",
    "replay_secs",
    "deviation_cf_wall",
})

_CONDITIONS = ("A", "B", "C", "D")
_SELECTION_ADDED = ("budget_stop", "cap_hit_playouts", "searched_menu_sum", "searched_menu_max")
_JOB_ADDED = ("transitions_total", "cap_hits", "searched_menu_sum", "searched_menu_max", "spy")

ADDED_KEYS = {
    "cond": (
        [("budget_stop",), ("config",), ("compute",), ("spy_labels",)]
        + [("selection", c, k) for c in _CONDITIONS for k in _SELECTION_ADDED]
        + [("selection", c, "levels", "[]", "spy") for c in _CONDITIONS]
        + [("eval", "jobs", "[]", k) for k in _JOB_ADDED]
    ),
    "cross": (
        [("budget_stop",), ("horizon_long",), ("config",), ("compute",), ("spy_labels",)]
        + [("b_selection", k) for k in _SELECTION_ADDED]
        + [("b_selection", "levels", "[]", "spy")]
        + [("eval", "deviation_cf_transitions")]
        + [("eval", "jobs", "[]", k) for k in _JOB_ADDED + ("deviation_cf_transitions", "deviation_cf_evaluated")]
        + [("eval", "jobs", "[]", "deviations", "[]", k) for k in ("ordinal", "counterfactual")]
    ),
}
ADDED_TAGS = ("cawgates_strands_castable_own_turn",)


def drop_wall(x):
    if isinstance(x, dict):
        return {k: drop_wall(v) for k, v in x.items() if k not in WALL_KEYS}
    if isinstance(x, list):
        return [drop_wall(v) for v in x]
    return x


def drop_path(x, path):
    """Removes the exact `path` from `x` in place (missing steps are fine)."""
    if not path:
        return
    step, rest = path[0], path[1:]
    if step == "[]":
        if isinstance(x, list):
            for v in x:
                drop_path(v, rest)
        return
    if not isinstance(x, dict) or step not in x:
        return
    if rest:
        drop_path(x[step], rest)
    else:
        del x[step]


def normalize_new(row):
    """The NEW row as the PR #164 binary would have written it (minus wall)."""
    kind = row.get("kind")
    for path in ADDED_KEYS.get(kind, ()):
        drop_path(row, path)
    if kind == "root" and any(t in ADDED_TAGS for t in row.get("tags") or []):
        row["tags"] = [t for t in row["tags"] if t not in ADDED_TAGS]
        if not row["tags"]:
            row["menu"] = None
    return row


def row_key(row):
    kind = row.get("kind")
    if "root_index" in row:
        return (kind, "root_index", row["root_index"])
    if kind == "root":
        return (kind, "game_step", row.get("game"), row.get("step"))
    return (kind, "game", row.get("game"))


def load(path):
    rows, dups = {}, []
    with open(path) as f:
        for n, line in enumerate(f, 1):
            if not line.strip():
                continue
            r = json.loads(line)
            k = row_key(r)
            if k in rows:
                dups.append((k, n))
            rows[k] = r
    return rows, dups


def canon(x):
    return json.dumps(x, sort_keys=True, separators=(",", ":"))


def diff(a, b, path, out):
    """Appends every differing path (exact types: 1 and 1.0 differ)."""
    if type(a) is not type(b):
        out.append(f"{path}: {canon(a)[:80]} != {canon(b)[:80]}")
    elif isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in b:
                out.append(f"{path}.{k}: only in OLD")
            elif k not in a:
                out.append(f"{path}.{k}: only in NEW")
            else:
                diff(a[k], b[k], f"{path}.{k}", out)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append(f"{path}: length {len(a)} != {len(b)}")
        for i, (x, y) in enumerate(zip(a, b, strict=False)):
            diff(x, y, f"{path}[{i}]", out)
    elif a != b:
        out.append(f"{path}: {canon(a)[:80]} != {canon(b)[:80]}")


def compare(old_rows, new_rows, wall_only=False):
    """{row key: [differences]} for every row that differs (or is unpaired)."""
    report = {}
    for k in sorted(set(old_rows) | set(new_rows), key=str):
        if k not in new_rows:
            report[k] = ["only in OLD"]
            continue
        if k not in old_rows:
            report[k] = ["only in NEW"]
            continue
        a = drop_wall(old_rows[k])
        b = drop_wall(new_rows[k])
        if not wall_only:
            b = normalize_new(b)
        if canon(a) != canon(b):
            out = []
            diff(a, b, "$", out)
            report[k] = out or ["serialization differs"]
    return report


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--wall-only", action="store_true", help="drop wall fields only (same binary on both sides)")
    ap.add_argument("--max-report", type=int, default=20)
    a = ap.parse_args(argv)
    old_rows, old_dups = load(a.old)
    new_rows, new_dups = load(a.new)
    report = compare(old_rows, new_rows, a.wall_only)
    for name, dups in (("OLD", old_dups), ("NEW", new_dups)):
        for k, n in dups:
            report.setdefault(k, []).append(f"duplicate key in {name} at line {n}")
    print(f"OLD {len(old_rows)} rows, NEW {len(new_rows)} rows; "
          f"{'wall fields only' if a.wall_only else 'wall fields and ADDED_KEYS'} excluded")
    for k, diffs in list(report.items())[: a.max_report]:
        print(f"DIFF {k}:")
        for d in diffs[:10]:
            print(f"  {d}")
        if len(diffs) > 10:
            print(f"  ... {len(diffs) - 10} more")
    if report:
        print(f"{len(report)} rows differ")
        return 1
    print("all rows identical")
    return 0


if __name__ == "__main__":
    sys.exit(main())
