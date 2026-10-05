"""Four fresh processes replay one retained development command stream (off/on/on/off).

Run under host_reservation_v1.dispatch, with pinned inputs and a correctly rebound
complete-agent runtime. This is a small correctness replay, never a strength run.
The existing package loader still verifies source, runtime, features and weights.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import statistics
import subprocess
import time


def sha(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(plan, report):
    for pin in plan["pins"].values():
        require(sha(pin["path"]) == pin["sha256"], "input pin differs: " + pin["path"])
    helper = load_module(plan["pins"]["reservation_helper"]["path"], "reservation")
    token = os.environ.get(helper.TOKEN_ENV)
    status = helper.status(token)
    held = status.get("record") or {}
    require(token and status.get("token_fate") == "holds" and held.get("lane") == plan["lane"]
            and held.get("work_id") == plan["run_id"], "replay requires its supported host reservation")
    report["reservation_token"] = token
    root = Path(plan["output_root"])
    require(root.is_absolute() and not root.exists(), "fresh absolute output root required")
    cap = plan["output_cap_bytes"]
    reserve = plan["reserve_bytes"]
    require(0 < cap <= 512 * 1024**2 and reserve >= 60 * 1024**3, "invalid storage bounds")
    require(shutil.disk_usage(root.parent).free - cap >= reserve, "storage reserve would be crossed")
    root.mkdir()
    report["output_root"] = str(root)
    inputs = Path(plan["input_root"])
    original_package = json.loads((inputs / "agent.json").read_bytes())
    package = json.loads((inputs / "agent.json").read_bytes())
    package["runtime"] = plan["runtime"]
    require(package["gameplay"] == original_package["gameplay"], "gameplay source changed during runtime rebind")
    write(root / "bound-agent.json", package)
    config = json.loads((inputs / "config.json").read_bytes())
    config["package"] = {"path": str(root / "bound-agent.json"), "sha256": sha(root / "bound-agent.json")}
    config["journal_path"] = str(root / "current-primary.jsonl")
    write(root / "bound-config.json", config)
    trace_config = json.loads((inputs / "trace-config.json").read_bytes())
    trace_config["path"] = str(root / "current-trace.jsonl")
    write(root / "bound-trace-config.json", trace_config)
    reader = load_module(plan["pins"]["reader"]["path"], "trace_reader")
    report["runtime"] = package["runtime"]
    report["gameplay_identity"] = package["gameplay"]["identity"]
    report["passes"] = passes = []
    events = {"human_command", "model_action", "game_start", "game_result", "incomplete_game"}
    for index, traced in enumerate((False, True, True, False)):
        target = root / f"pass-{index + 1}-{'on' if traced else 'off'}"
        target.mkdir()
        argv = [package["runtime"]["executable"]["path"], "--config", str(root / "bound-config.json")]
        if traced:
            argv += ["--trace", str(root / "bound-trace-config.json")]
        started = time.monotonic()
        with open(inputs / "commands.jsonl", "rb") as commands, open(target / "responses.jsonl", "wb") as output, open(target / "stderr.txt", "wb") as stderr:
            result = subprocess.run(argv, stdin=commands, stdout=output, stderr=stderr,
                                    creationflags=0x00004000 if os.name == "nt" else 0, timeout=120)
        elapsed = time.monotonic() - started
        entry = {"traced": traced, "seconds": elapsed, "exit_code": result.returncode, "argv": argv}
        passes.append(entry)
        require(result.returncode == 0, "replay process failed; stderr retained")
        (root / "current-primary.jsonl").rename(target / "primary.jsonl")
        rows = [json.loads(line) for line in (target / "primary.jsonl").read_bytes().splitlines()]
        selected = [row for row in rows if row["event"] == "model_action"]
        entry.update(primary_sha256=sha(target / "primary.jsonl"), selected_actions=len(selected),
                     selected_actions_sha256=hashlib.sha256(json.dumps(selected, sort_keys=True).encode()).hexdigest(),
                     retained_events_sha256=hashlib.sha256(json.dumps([row for row in rows if row["event"] in events], sort_keys=True).encode()).hexdigest())
        if traced:
            (root / "current-trace.jsonl").rename(target / "trace.jsonl")
            header, decisions, footer = reader.read_trace(target / "trace.jsonl")
            require(footer["complete"], "diagnostic capture is incomplete")
            require(len(decisions) == len(selected), "trace does not cover every executed model action")
            require(header["identity"]["runtime"]["executable_sha256"] == package["runtime"]["executable"]["sha256"], "trace runtime differs")
            entry.update(trace_sha256=sha(target / "trace.jsonl"), trace_bytes=(target / "trace.jsonl").stat().st_size,
                         trace_decisions=len(decisions), trace_footer=footer)
        total = sum(file.stat().st_size for file in root.rglob("*") if file.is_file())
        require(total <= cap and shutil.disk_usage(root).free >= reserve, "replay storage bound crossed")
        write(root / "progress.json", report)
    require(len({p["primary_sha256"] for p in passes}) == 1, "primary store differs with tracing or a fresh process")
    require(len({p["selected_actions_sha256"] for p in passes}) == 1, "selected actions differ")
    require(len({p["trace_sha256"] for p in passes if p["traced"]}) == 1, "fresh-process sidecar differs")
    off = statistics.mean(p["seconds"] for p in passes if not p["traced"])
    on = statistics.mean(p["seconds"] for p in passes if p["traced"])
    report.update(passed=True, primary_equal=True, selected_actions_equal=True, sidecars_equal=True,
                  mean_off_seconds=off, mean_on_seconds=on, overhead_seconds=on-off,
                  overhead_ratio=on/off, actual_bytes=sum(f.stat().st_size for f in root.rglob("*") if f.is_file()),
                  limitations="one retained development seed and command stream; startup-inclusive timing, no strength or throughput qualification")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    plan = json.loads(args.manifest.read_bytes())
    report = {"schema": "gameplay-trace-replay-result/v1", "plan_sha256": sha(args.manifest), "passed": False}
    try:
        run(plan, report)
    except Exception as error:
        report["error"] = str(error)
    write(args.manifest.with_name("replay-result.json"), report)
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
