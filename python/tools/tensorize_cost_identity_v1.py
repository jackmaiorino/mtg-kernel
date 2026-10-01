"""Cross-build training identity for the tensorize-cost lane.

Compares the Store outputs of a qualification produced by the new executable
with those of PR #107's round-2 qualification (base executable), run by run,
using the launcher's own adapter (`output_digests`, the file set and rules the
sentinel compares). Checks the 12-update serial goldens of every run and the
full-length serial sentinel references.

    python python/tools/tensorize_cost_identity_v1.py \
        --base D:/multirun-qualified-v1-work/q6/qualification \
        --new D:/tensorize-cost-v1-work/launcher/q-final \
        --workload D:/tensorize-cost-v1-work/launcher/workload-final.json \
        --out docs/reports/tensorize_cost_v1/after/training-identity.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import multirun_launcher_v1 as launcher  # noqa: E402


def compare(adapter, base_root: Path, new_root: Path) -> dict:
    base = adapter.output_digests(base_root)
    new = adapter.output_digests(new_root)
    differing = sorted(path for path in set(base) | set(new) if base.get(path) != new.get(path))
    return {
        "base_root": str(base_root),
        "new_root": str(new_root),
        "outputs": len(base),
        "identical": bool(base) and not differing,
        "differing": differing[:20],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--new", type=Path, required=True)
    parser.add_argument("--workload", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    workload = launcher.load_workload(args.workload)
    adapter = workload.adapter
    record: dict = {"schema": "tensorize-cost-v1/training-identity", "serial_golden": {}, "sentinel_serial": {}}
    for kind in ("serial_golden", "sentinel_serial"):
        directory = kind.replace("_", "-")
        base_parent = args.base / directory
        new_parent = args.new / directory
        if not base_parent.is_dir() or not new_parent.is_dir():
            record[kind] = {"missing": [str(base_parent), str(new_parent)]}
            continue
        for run_root in sorted(path for path in base_parent.iterdir() if path.is_dir()):
            counterpart = new_parent / run_root.name
            if counterpart.is_dir():
                record[kind][run_root.name] = compare(adapter, run_root, counterpart)
    rows = [row for kind in ("serial_golden", "sentinel_serial") for row in record[kind].values()
            if isinstance(row, dict) and "identical" in row]
    record["all_identical"] = bool(rows) and all(row["identical"] for row in rows)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(record, indent=1) + "\n", encoding="utf-8")
    for kind in ("serial_golden", "sentinel_serial"):
        for run, row in record[kind].items():
            if isinstance(row, dict) and "identical" in row:
                print(f"{kind} {run}: outputs={row['outputs']} identical={row['identical']}")
    print(f"all_identical={record['all_identical']}")
    return 0 if record["all_identical"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
