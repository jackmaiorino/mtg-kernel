"""Build single-worker shard jobs for the not-yet-done formal roots.

python make_shards.py BUDGET_CPU_SECONDS  -> queue-formal-shards.json
Each model's remaining roots are dealt round-robin into 4 shards (frozen
order); each shard runs as its own one-worker process with its own output
file. All 8 jobs share one CPU budget. Per-root results do not depend on
the process or worker count (qualification checked identical hashes).
"""
import json
import sys
from pathlib import Path

ROOT = Path("D:/stage4a-20261009")
budget = int(sys.argv[1])
jobs = []
for model, base in (("r1", 2026100941), ("r2", 2026100942)):
    done = set()
    for f in (ROOT / "out").glob(f"formal-unordered-{model}*.jsonl"):
        for line in f.read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                if r.get("kind") == "s4a_root":
                    done.add(r["root_id"])
    roots = [json.loads(l) for l in (ROOT / "frozen-gy" / f"roots-{model}.jsonl").read_text().splitlines() if l.strip()]
    todo = [r for r in roots if r["root_id"] not in done]
    for s in range(4):
        shard = todo[s::4]
        if not shard:
            continue
        p = ROOT / "frozen-gy" / f"shard-{model}-{s}.jsonl"
        p.write_text("".join(json.dumps(r) + "\n" for r in shard), encoding="utf-8", newline="\n")
        jobs.append({"name": f"formal-unordered-{model}-s{s}", "mode": "s4a-run", "model": model, "workers": 1,
                     "base_seed": base, "roots": p.as_posix(), "group_max_cpu_seconds": budget})
    print(model, "done", len(done), "todo", len(todo))
(ROOT / "queue-formal-shards.json").write_text(json.dumps([{"parallel": jobs}], indent=1))
print(len(jobs), "jobs, budget", budget)
