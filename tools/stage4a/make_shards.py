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


def completed_roots(path):
    """Read committed rows without changing interrupted output evidence."""
    done = set()
    lines = path.read_bytes().splitlines(keepends=True)
    for index, line in enumerate(lines):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except (json.JSONDecodeError, UnicodeDecodeError):
            # Native output rows are JSON objects terminated by a newline.
            # A killed writer can leave its final object/UTF-8 character cut.
            if index == len(lines) - 1 and not line.endswith(b"\n") and line.lstrip().startswith(b"{"):
                print(f"ignoring interrupted final row in {path}", file=sys.stderr)
                continue
            raise
        if row.get("kind") == "s4a_root":
            done.add(row["root_id"])
    return done


def build_shards(budget, root=ROOT):
    if type(budget) is not int or budget <= 0:
        raise ValueError("CPU budget must be a positive integer")
    jobs, planned = [], []
    for model, base in (("r1", 2026100941), ("r2", 2026100942)):
        done = set()
        for path in (root / "out").glob(f"formal-unordered-{model}*.jsonl"):
            done.update(completed_roots(path))
        roots = [json.loads(line) for line in (root / "frozen-gy" / f"roots-{model}.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
        todo = [row for row in roots if row["root_id"] not in done]
        for index in range(4):
            shard = todo[index::4]
            if not shard:
                continue
            path = root / "frozen-gy" / f"shard-{model}-{index}.jsonl"
            planned.append((path, shard))
            jobs.append({"name": f"formal-unordered-{model}-s{index}", "mode": "s4a-run", "model": model, "workers": 1,
                         "base_seed": base, "roots": path.as_posix(), "group_max_cpu_seconds": budget})
        print(model, "done", len(done), "todo", len(todo))
    # Validate both models before replacing any prepared shard or queue.
    for path, shard in planned:
        path.write_text("".join(json.dumps(row) + "\n" for row in shard), encoding="utf-8", newline="\n")
    (root / "queue-formal-shards.json").write_text(json.dumps([{"parallel": jobs}], indent=1), encoding="utf-8")
    print(len(jobs), "jobs, budget", budget)
    return jobs


if __name__ == "__main__":
    build_shards(int(sys.argv[1]))
