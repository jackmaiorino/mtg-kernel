"""W4 Step A inputs (collab LANES/w4-combat-20261030/DESIGN.md section 4). Measurement only.

usage: prep.py --win-check DIR --out DIR

Writes, from the win-check capture:
- jobs.jsonl: the label jobs of the verified games (the Spy sample's and the non-Spy capture's), sorted by id;
- split.json: each game's half, by SHA-256 of the game id (even last byte: fit, odd: read);
- sources.json: per panel, the candidate and opponent model sources of the capture plan, checked against the
  plan's own requests (the plan's pin overrides re-point moved inputs by hash);
- accept-jobs.jsonl: the first 5 read-half games by id, for the driver acceptance check.
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import json
from pathlib import Path


def half(game_id: str) -> str:
    return "read" if hashlib.sha256(game_id.encode()).digest()[-1] % 2 else "fit"


def verified_ids(win_check: Path) -> set[str]:
    ids = set()
    for pattern in ("native-sample-360/rows-*.jsonl", "capture-check/rows-*.jsonl"):
        for path in glob.glob(str(win_check / pattern)):
            for line in open(path, encoding="utf-8"):
                try:
                    row = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if row.get("kind") == "game":
                    ids.add(row["id"])
    return ids


def pins(source: dict) -> list:
    """What identifies a source apart from file locations."""
    inner = source["source"]
    files = [inner.get(k) and inner[k]["sha256"] for k in ("play_import", "checkpoint")]
    return [source["kind"], files, inner["feature_transfer"], source.get("v3_forced_actions"),
            bool(source.get("v3_spell_target_reference_adapter"))]


def sources(win_check: Path) -> dict:
    out = {}
    for plan_path in sorted(glob.glob(str(win_check / "capture" / "*" / "plan.json"))):
        plan = json.loads(Path(plan_path).read_text(encoding="utf-8"))
        panel = Path(plan_path).parent.name
        candidate = {"kind": "legacy", "source": plan["candidate"], "v3_forced_actions": False}
        entry = {"candidate": candidate, "opponent": plan["opponent"]}
        for seat in (0, 1):
            request = json.loads((Path(plan_path).parent / "requests" / f"seat{seat}-capture.json").read_text())
            want = [candidate, plan["opponent"]] if seat == 0 else [plan["opponent"], candidate]
            if [pins(s) for s in request["sources"]] != [pins(s) for s in want]:
                raise SystemExit(f"{panel}: plan sources differ from request seat{seat}")
        out[panel] = entry
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--win-check", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    ids = verified_ids(a.win_check)
    jobs = {}
    paths = [a.win_check / "native-sample-360" / "jobs-360.jsonl",
             *map(Path, sorted(glob.glob(str(a.win_check / "capture" / "*" / "label-jobs.jsonl"))))]
    for path in paths:
        for line in open(path, encoding="utf-8"):
            if line.strip():
                job = json.loads(line)
                if job["id"] in ids:
                    jobs[job["id"]] = line.strip()
    missing = ids - set(jobs)
    if missing:
        raise SystemExit(f"no label job for {len(missing)} verified games")
    order = sorted(jobs)
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "jobs.jsonl").write_text("".join(jobs[i] + "\n" for i in order), encoding="utf-8")
    split = {i: half(i) for i in order}
    (a.out / "split.json").write_text(json.dumps(split, indent=0) + "\n", encoding="utf-8")
    (a.out / "sources.json").write_text(json.dumps(sources(a.win_check), indent=1) + "\n", encoding="utf-8")
    first = [i for i in order if split[i] == "read"][:5]
    (a.out / "accept-jobs.jsonl").write_text("".join(jobs[i] + "\n" for i in first), encoding="utf-8")
    print(json.dumps({"games": len(order), "fit": sum(v == "fit" for v in split.values()),
                      "read": sum(v == "read" for v in split.values()), "accept": first}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
