#!/usr/bin/env python3
"""Dispatcher overhead bench: real agents behind a fake ssh that adds a fixed latency per connection,
a fake collector that writes `--bytes-per-game` per episode, and a merge that reads, hashes and copies
every trajectory (as the trainer's --merge-collection-shards does). Measures the dispatcher's wall per
update for one dispatcher module (pass the old and the new file to compare).

    python3 pooled_dispatch_bench.py MODULE.py --episodes 60 --hosts 3 --latency 0.044 --updates 6

The fake collector plays nothing, so the wall is the dispatcher's own cost plus data movement on this
machine: what the real run adds on top of play. POSIX only (the agent's run slots and the fake ssh)."""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import statistics
import sys
import tempfile
import textwrap
import time
from pathlib import Path

COLLECTOR = textwrap.dedent('''
    import hashlib, json, os, sys
    command = json.load(open(sys.argv[2]))
    out = command["output_directory"]
    os.makedirs(out)
    size = int(os.environ.get("BENCH_BYTES", "1000"))
    pins = []
    for offset, episode in enumerate(command["episodes"]):
        name = "episode-%04d.json" % (command["shard"]["first_slot"] + offset)
        seed = hashlib.sha256(("%s:%s" % (episode["id"], episode["seed"])).encode()).digest()
        data = (seed * (size // 32 + 1))[:size]
        open(os.path.join(out, name), "wb").write(data)
        pins.append({"path": os.path.join(out, name), "sha256": hashlib.sha256(data).hexdigest()})
    json.dump({"trajectories": pins, "shard": command["shard"]}, open(os.path.join(out, "collection.json"), "w"))
''')

SSH = textwrap.dedent('''
    import os, subprocess, sys, time
    time.sleep(float(os.environ.get("BENCH_LATENCY", "0")))
    with open(os.environ["BENCH_CALLS"], "a") as log:
        log.write("1\\n")
    env = dict(os.environ, SSH_ORIGINAL_COMMAND=sys.argv[2])
    sys.exit(subprocess.run([sys.executable, "-I", os.environ["BENCH_AGENT"], "agent", sys.argv[1]], env=env).returncode)
''')


def merge(command_path, directories):
    out = Path(command_path).parent / "collect"
    out.mkdir()
    count = 0
    for directory in directories:
        for pin in json.loads((directory / "collection.json").read_text())["trajectories"]:
            name = Path(pin["path"]).name
            data = (directory / name).read_bytes()
            assert hashlib.sha256(data).hexdigest() == pin["sha256"]
            (out / name).write_bytes(data)
            count += 1
    (out / "collection.json").write_text("{}")
    return {"trajectories": count}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("module")
    parser.add_argument("--episodes", type=int, default=60)
    parser.add_argument("--hosts", type=int, default=3)
    parser.add_argument("--latency", type=float, default=0.044)
    parser.add_argument("--bytes-per-game", type=int, default=100_000)
    parser.add_argument("--checkpoint-bytes", type=int, default=15_000_000)
    parser.add_argument("--updates", type=int, default=6)
    parser.add_argument("--audit-every", type=int, default=0)
    parser.add_argument("--root", default=None)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location("bench_dispatcher", args.module)
    pc = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = pc  # dataclasses look their module up
    spec.loader.exec_module(pc)
    root = Path(tempfile.mkdtemp(prefix="pooled-bench-", dir=args.root)).resolve()
    try:
        learner, pins = root / "learner", root / "pins"
        learner.mkdir()
        pins.mkdir()
        collector = root / "collector.py"
        collector.write_text(COLLECTOR)
        ssh = root / "ssh.py"
        ssh.write_text(SSH)
        os.environ.update(BENCH_LATENCY=str(args.latency), BENCH_CALLS=str(root / "calls"),
                          BENCH_AGENT=str(Path(args.module).resolve()), BENCH_BYTES=str(args.bytes_per_game))
        hosts = []
        for index in range(args.hosts):
            scratch = root / f"scratch-{index}"
            scratch.mkdir()
            config = root / f"agent-{index}.json"
            config.write_text(json.dumps({
                "schema": pc.AGENT_SCHEMA, "collector": str(collector), "collector_sha256": pc.sha256_file(collector),
                "scratch": str(scratch), "pin_roots": [str(pins)], "prefix": [sys.executable], "max_seconds": 600,
                "min_free_bytes": 0, "sandbox": "none", "max_concurrent_runs": 4}))
            hosts.append(pc.Host(name=f"h{index}", kind="ssh", collector=str(collector),
                                 collector_sha256=pc.sha256_file(collector), workers=4, weight=1.0,
                                 scratch=str(scratch), arch=f"arch{index}", ssh=[sys.executable, str(ssh), str(config)],
                                 prefix=[sys.executable], transport="agent"))
        # One machine is one path namespace: the hosts see the learner's inputs in place, so nothing is
        # pushed here; a real update adds one push of the new checkpoint per host (old and new code alike).
        opponent = pins / "opponent.bin"
        opponent.write_bytes(os.urandom(args.checkpoint_bytes))
        dispatcher = pc.Dispatcher({"hosts": hosts, "audit": {"every": args.audit_every, "episodes": 2}},
                                   timeout=600)
        walls, calls = [], []
        for update in range(args.updates):
            checkpoint = pins / "run" / f"checkpoint-{update}.bin"
            checkpoint.parent.mkdir(exist_ok=True)
            checkpoint.write_bytes(os.urandom(args.checkpoint_bytes))
            command = {"mode": "collect_parallel", "workers": 4,
                       "episodes": [{"id": f"e{i}", "seed": update * 10000 + i} for i in range(args.episodes)],
                       "source": {"checkpoint": {"path": str(checkpoint), "sha256": pc.sha256_file(checkpoint)}},
                       "opponent": {"checkpoint": {"path": str(opponent), "sha256": pc.sha256_file(opponent)}},
                       "output_directory": "unused"}
            attempt = root / "run" / "iterations" / f"{update:06d}" / "attempt-000000"
            attempt.mkdir(parents=True)
            path = attempt / "collect-command.json"
            path.write_text(json.dumps(command))
            (root / "calls").write_text("")
            started = time.perf_counter()
            receipt = dispatcher.collect(path, merge)
            walls.append(time.perf_counter() - started)
            calls.append(len((root / "calls").read_text().split()))
            print(json.dumps({"update": update, "wall": round(walls[-1], 3), "ssh_calls": calls[-1],
                              "timing": receipt.get("timing"), "shards_s": receipt["collection_seconds"]}))
            shutil.rmtree(attempt.parent)
            checkpoint.unlink()
        steady = walls[1:] or walls
        print(json.dumps({"module": args.module, "episodes": args.episodes, "hosts": args.hosts,
                          "latency": args.latency, "bytes_per_game": args.bytes_per_game,
                          "first_wall": round(walls[0], 3), "steady_mean_wall": round(statistics.mean(steady), 3),
                          "steady_calls": statistics.mean(calls[1:] or calls)}))
    finally:
        shutil.rmtree(root, ignore_errors=True)


if __name__ == "__main__":
    main()
