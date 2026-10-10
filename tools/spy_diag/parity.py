"""Exact E-projection parity of diagnosis rows against the archived Stage 4a rows.

Usage: python parity.py --plan PLANDIR --archive STAGE4A_ROOT ROWS.jsonl [...]
Uses the plan's own `e_projection` and canonical JSON (select_panel.py).
For every s4a_diag_root row: its projection hash must equal PANEL.json's
pinned e_projection_sha256, and its projection must equal the archived
row's projection field by field (the first differing path is printed).
Prints one JSON record per row and a summary; exits 1 on any mismatch.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path
from panel_coverage import expected_roots, require_coverage


def first_diff(a, b, path="$"):
    if type(a) is not type(b):
        return path
    if isinstance(a, dict):
        for k in sorted(set(a) | set(b)):
            if k not in a or k not in b:
                return f"{path}.{k}"
            d = first_diff(a[k], b[k], f"{path}.{k}")
            if d:
                return d
        return None
    if isinstance(a, list):
        if len(a) != len(b):
            return f"{path}[len]"
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_diff(x, y, f"{path}[{i}]")
            if d:
                return d
        return None
    return None if a == b else path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--plan", required=True)
    ap.add_argument("--archive", required=True)
    ap.add_argument("--root-id", action="append", help="explicit qualification subset; default checks the complete panel")
    ap.add_argument("rows", nargs="+")
    a = ap.parse_args()
    sys.path.insert(0, a.plan)
    import select_panel as sp
    document = json.loads((Path(a.plan) / "PANEL.json").read_text())
    expected = expected_roots(document, a.root_id)
    panel = {r["root_id"]: r for r in document["roots"]}
    ok = True
    n = 0
    observed = []
    for f in a.rows:
        for line in Path(f).read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            if row.get("kind") != "s4a_diag_root":
                print(json.dumps({"file": f, "non_row": row.get("kind"), "detail": str(row)[:300]}))
                ok = False
                continue
            n += 1
            observed.append(row["root_id"])
            if row["root_id"] not in expected:
                ok = False
                continue
            ref = panel[row["root_id"]]
            archived = json.loads((Path(a.archive) / ref["row_file"].replace("\\", "/")).read_text(
                encoding="utf-8").splitlines()[ref["line"] - 1])
            if archived.get("root_id") != row["root_id"]:
                raise ValueError('archived row differs from the pinned root identity')
            got = sp.e_projection(row)
            want = sp.e_projection(archived)
            h = hashlib.sha256(sp.canonical_bytes(got)).hexdigest()
            rec = {"file": f, "root_id": row["root_id"], "projection_sha256": h,
                   "pinned": ref["e_projection_sha256"], "hash_equal": h == ref["e_projection_sha256"],
                   "archived_equal": sp.canonical_bytes(got) == sp.canonical_bytes(want),
                   "first_diff": first_diff(got, want), "trace": row.get("diag", {}).get("trace"),
                   "tree_hash": row["arms"]["E"]["selection"]["extra"]["tree"]["hash"]}
            ok &= rec["hash_equal"] and rec["archived_equal"]
            print(json.dumps(rec))
    try:
        # A qualification subset repeats roots (trace on and off) by design.
        require_coverage(sorted(set(observed)) if a.root_id else observed, expected)
    except ValueError as error:
        print(json.dumps({"coverage_error": str(error)}))
        ok = False
    print(json.dumps({"rows": n, "all_exact": ok, "expected_roots": sorted(expected),
                      "scope": "qualification_subset" if a.root_id else "full_panel"}))
    return 0 if ok and n else 1


if __name__ == "__main__":
    sys.exit(main())
