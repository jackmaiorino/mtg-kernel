"""Diff the captured sibling views (S4A_MILLOBS records) field by field.

Usage: python diff_views.py TRACE.jsonl
Prints, per phase, every JSON path whose value differs between the first
captured sibling and each later one, for the canonical observation, the
canonical menu and the raw actor-visible observation. Operator-only.
"""
import json
import sys


def validate_capture(record):
    view = record.get("view")
    if not isinstance(view, dict):
        raise ValueError("capture has no complete view")
    for part in ("canon_obs", "canon_menu", "raw_obs", "raw_menu"):
        if view.get(part) is None:
            raise ValueError(f"capture has missing/failed {part} projection")
    return record


def walk(a, b, path, out):
    if type(a) is not type(b):
        out.append((path, a, b))
    elif isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            walk(a.get(k), b.get(k), f"{path}.{k}", out)
    elif isinstance(a, list):
        if len(a) != len(b):
            out.append((path + "[len]", len(a), len(b)))
        for i, (x, y) in enumerate(zip(a, b)):
            walk(x, y, f"{path}[{i}]", out)
    elif a != b:
        out.append((path, a, b))


def main():
    recs = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if '"r":"millobs"' in l]
    for phase in ("selection_start", "after_selection"):
        rs = [validate_capture(r) for r in recs if r["phase"] == phase]
        if len(rs) < 2:
            continue
        base = rs[0]
        for other in rs[1:]:
            print(f"== {phase}: sim {base['i']} vs sim {other['i']} (depth {base['dep']} vs {other['dep']})")
            for part in ("canon_obs", "canon_menu", "raw_obs"):
                diffs = []
                walk(base["view"][part], other["view"][part], part, diffs)
                print(f"  {part}: {len(diffs)} differing paths")
                for p, x, y in diffs[:12]:
                    print(f"    {p}: {json.dumps(x)[:90]} | {json.dumps(y)[:90]}")


if __name__ == "__main__":
    main()
