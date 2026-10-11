"""Training arms of the reset pilot through the supported guarded launcher.

Every arm (lineage x O/P/S) is one native-expanded training run of 162
updates from its parent's complete checkpoint (Adam 35,640). Qualification,
the allocation choice and production all go through
python/tools/native_expanded_dispatch_v1.py; this driver only writes the
requests, waits for receipts inside the held reservation (nested dispatch),
and assembles the choice exactly as that launcher checks it.

CLI:
  runtime   --binary EXE --config CFG --out RUNTIME.json
  requests  --arm r1-P --config CFG --runtime RUNTIME.json --root ROOT --workers 1,2,4,8 --cores 0-15
  wait      REQUEST.json [REQUEST.json ...]     dispatch qualify each, in order, and wait (inside a reservation)
  choice    --arm r1-P --config CFG --root ROOT --cores 0-15 --haley-reason TEXT
  produce   --arm r1-P --config CFG --runtime RUNTIME.json --root ROOT --workers W --cores 0-15
  dispatch  REQUEST.json                        production dispatch and wait (inside a reservation)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TOOLS = REPO / "python" / "tools"
sys.path.insert(0, str(TOOLS))
import native_expanded_dispatch_v1 as dispatch  # noqa: E402

LANE = "claude-spy-reset-pilot-20261010"
DISPATCH = str(TOOLS / "native_expanded_dispatch_v1.py")
GiB = 1024**3


def pin(path) -> dict:
    path = Path(path)
    return {"path": str(path).replace("\\", "/"), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def write(path, value) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=1) + "\n", encoding="utf-8")
    return pin(path)


def cores(text: str) -> list[int]:
    out = []
    for part in text.split(","):
        a, _, b = part.partition("-")
        out += list(range(int(a), int(b or a) + 1))
    return out


def storage(root: Path) -> dict:
    hot, cold = Path(root), Path(str(root).replace("D:", "E:", 1))
    return {"accounting_roots": [str(hot).replace("\\", "/"), str(cold).replace("\\", "/")],
            "max_logical_bytes": 128 * GiB, "reserve_bytes": 60 * GiB,
            "projected_additional_bytes": 24 * GiB, "projected_volume_bytes": {"D:": 16 * GiB, "E:": 8 * GiB}}


def request(arm: str, config: Path, runtime: Path, root: Path, workers: int, cpus: list[int], tag: str,
            wall: int) -> dict:
    root = Path(root)
    return {"schema": dispatch.SCHEMA, "kind": "training", "lane": LANE, "runtime": pin(runtime),
            "config": pin(config), "root": str(root / "hot" / tag).replace("\\", "/"),
            "cold_root": str(Path(str(root).replace("D:", "E:", 1)) / "cold" / tag).replace("\\", "/"),
            "placement": {"host": "desktop", "cpu_affinity": cpus, "workers": workers,
                          "preparation_workers": workers, "memory_bytes": 16 * GiB},
            "storage": storage(root), "wall_seconds": wall,
            "non_natural_tolerance": json.loads(Path(config).read_text())["max_non_natural_episode_fraction"]}


def wait_root(root: Path, timeout: float) -> dict:
    """Waits for the dispatcher's receipt (report.json) or its failure record."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if (root / "report.json").exists():
            return json.loads((root / "report.json").read_text())
        execution = root / "execution.json"
        if execution.exists():
            record = json.loads(execution.read_text())
            if record.get("error"):
                raise RuntimeError(f"{root}: {record['error']}")
        time.sleep(10)
    raise TimeoutError(f"{root}: no receipt within {timeout} s")


def run_and_wait(action: str, path: Path, timeout: float) -> dict:
    req = json.loads(Path(path).read_text())
    done = subprocess.run([sys.executable, "-B", DISPATCH, action, str(Path(path).resolve())],
                          capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"{action} {path}: {done.stderr[-2000:]}")
    return wait_root(Path(req["root"]), timeout)


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    r = sub.add_parser("runtime")
    r.add_argument("--binary", type=Path, required=True)
    r.add_argument("--config", type=Path, required=True)
    r.add_argument("--out", type=Path, required=True)
    q = sub.add_parser("requests")
    p = sub.add_parser("produce")
    for s in (q, p):
        s.add_argument("--arm", required=True)
        s.add_argument("--config", type=Path, required=True)
        s.add_argument("--runtime", type=Path, required=True)
        s.add_argument("--root", type=Path, required=True)
        s.add_argument("--cores", required=True)
    q.add_argument("--workers", required=True)
    p.add_argument("--workers", type=int, required=True)
    for name in ("wait", "dispatch"):
        sub.add_parser(name).add_argument("requests", type=Path, nargs="+")
    c = sub.add_parser("choice")
    c.add_argument("--arm", required=True)
    c.add_argument("--config", type=Path, required=True)
    c.add_argument("--root", type=Path, required=True)
    c.add_argument("--workers", required=True)
    c.add_argument("--haley-reason", required=True)
    a = ap.parse_args(argv)
    if a.op == "runtime":
        out = subprocess.run([str(a.binary), "--validate-config", str(a.config)], capture_output=True, text=True)
        if out.returncode != 0:
            raise SystemExit(out.stderr)
        identity = json.loads(out.stdout)["runtime"]
        print(json.dumps(write(a.out, {"engine_commit": identity["engine_commit"],
                                       "tracked_tree_sha256": identity["tracked_tree_sha256"],
                                       "binary": pin(a.binary)})))
    elif a.op == "requests":
        paths = []
        for w in [int(x) for x in a.workers.split(",")]:
            tag = f"qual-{a.arm}-w{w}"
            paths.append(write(a.root / "requests" / f"{tag}.json",
                               request(a.arm, a.config, a.runtime, a.root, w, cores(a.cores), tag, 1800))["path"])
        print(json.dumps(paths))
    elif a.op == "wait":
        for path in a.requests:
            report = run_and_wait("qualify", path, 3600)
            print(json.dumps({"request": str(path), "seconds": report["seconds"]}), flush=True)
    elif a.op == "choice":
        now = datetime.now(timezone.utc).isoformat()
        state = a.root / "choice"
        census = write(state / "desktop-census.json", {
            "host": "DESKTOP-DJ1C40R", "at": now, "logical_cpus": 24,
            "free": {v: shutil.disk_usage(v).free for v in ("C:/", "D:/", "E:/")},
            "reservation": json.loads(subprocess.run([sys.executable, "-B", str(TOOLS / "host_reservation_v1.py"),
                                                      "status"], capture_output=True, text=True).stdout)})
        haley = write(state / "computehost.json", {"at": now, "reason": a.haley_reason})
        runpod = write(state / "runpod.json", {"eligible": False, "reason": "no paid compute in this pilot"})
        reports = {}
        for w in [int(x) for x in a.workers.split(",")]:
            reports[w] = pin(a.root / "hot" / f"qual-{a.arm}-w{w}" / "report.json")
        seconds = {w: json.loads(Path(r["path"]).read_text())["seconds"] for w, r in reports.items()}
        best = min(seconds, key=seconds.get)
        config = json.loads(a.config.read_text())
        family = dispatch.workload(config, "training")
        first = json.loads(Path(reports[min(reports)]["path"]).read_text())
        choice = {"schema": dispatch.CHOICE, "inventory": {
            "desktop": {"eligible": True, "reason": "declared timed cores qualified at " + a.workers + " workers",
                        "checked_at": now, "evidence": census, "cpu_affinity": first["placement"]["cpu_affinity"],
                        "transport_seconds": 0},
            "computehost": {"eligible": False, "reason": a.haley_reason, "checked_at": now, "evidence": haley},
            "runpod": {"eligible": False, "reason": "no paid compute in this pilot", "checked_at": now,
                       "evidence": runpod}},
            "qualifications": [reports[w] for w in sorted(reports)], "selected": {family: reports[best]}}
        path = write(state / f"choice-{a.arm}.json", choice)
        print(json.dumps({"choice": path, "best_workers": best, "seconds": seconds}))
    elif a.op == "produce":
        tag = f"prod-{a.arm}"
        req = request(a.arm, a.config, a.runtime, a.root, a.workers, cores(a.cores), tag, 4 * 3600)
        state = a.root / "choice"
        req["choice"] = pin(state / f"choice-{a.arm}.json")
        req["choice_verification_output"] = str(state / f"verification-{a.arm}.json")
        check = write(a.root / "requests" / f"check-{a.arm}.json", req)
        done = subprocess.run([sys.executable, "-B", DISPATCH, "check-choice", check["path"]],
                              capture_output=True, text=True)
        if done.returncode != 0:
            raise SystemExit(done.stderr[-2000:])
        req.pop("choice_verification_output")
        req["choice_verification"] = pin(state / f"verification-{a.arm}.json")
        print(json.dumps(write(a.root / "requests" / f"{tag}.json", req)))
    else:
        for path in a.requests:
            report = run_and_wait("dispatch", path, 6 * 3600)
            print(json.dumps({"request": str(path), "seconds": report["seconds"]}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
