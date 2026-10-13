"""Run built evaluation panels inside a held reservation (qualifying each reference first if needed).

Usage: python eval_run.py ROOT EVALUATOR CHECKPOINT_ID [CHECKPOINT_ID ...]
Panels live at ROOT/eval/<id>-vs-<T1|H>/plan.json (evaluate.py build). The evaluator is qualified once
per reference on the first listed checkpoint's plan (16 matches at 1, 2, 4 and 8 processes, per-match
hash parity), and production uses the fastest qualified process count. Rows go to each panel's
rows.jsonl; a JSON line per finished panel goes to ROOT/eval/index.jsonl.
"""
from __future__ import annotations

import json
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "python" / "tools"))
import nine_deck_public_panel_v1 as npp  # noqa: E402


def main() -> None:
    root, evaluator, ids = Path(sys.argv[1]), sys.argv[2], sys.argv[3:]
    for ref in ("T1", "H"):
        qual = root / "eval" / f"qual-{ref}-b"
        receipt = qual / "receipt.json"
        if not receipt.exists():
            trials = npp.qualify(root / "eval" / f"{ids[0]}-vs-{ref}" / "plan.json", 16, [1, 2, 4, 8], qual,
                                 evaluator)
            with (root / "eval" / "index.jsonl").open("a") as f:
                f.write(json.dumps({"qualified": ref, "selected_workers": trials["selected_workers"],
                                    "trials": [{k: t[k] for k in ("workers", "seconds", "parity")}
                                               for t in trials["trials"]]}) + "\n")
    for ck in ids:
        for ref in ("T1", "H"):
            plan = root / "eval" / f"{ck}-vs-{ref}" / "plan.json"
            out = plan.parent / "rows.jsonl"
            if out.exists():
                continue
            t0 = time.time()
            result = npp.run(plan, root / "eval" / f"qual-{ref}-b" / "receipt.json", evaluator)
            rows = npp.rows(plan)
            out.write_text("".join(json.dumps(r, sort_keys=True) + "\n" for r in rows))
            with (root / "eval" / "index.jsonl").open("a") as f:
                f.write(json.dumps({"checkpoint": ck, "opponent": ref, "rows": len(rows),
                                    "complete": sum(r["complete"] for r in rows), "workers": result.get("workers"),
                                    "wall_seconds": time.time() - t0}) + "\n")


if __name__ == "__main__":
    main()
