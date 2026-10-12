"""Guarded launcher steps for the E supported-choice CE pilot (one native run per arm and phase).

Every training job enters through python/tools/native_expanded_dispatch_v1.py: this driver only
writes requests, waits for receipts inside an already held reservation (nested dispatch) and
assembles the allocation choice exactly as that launcher checks it. E runs use serial update
preparation (the E command is the sequential CPU GAE update); collection workers vary.

CLI:
  runtime   --binary EXE --config CFG --out RUNTIME.json
  requests  --tag T --config CFG --runtime RUNTIME.json --root ROOT --workers 1,5,10 --cores 0-9 [--host desktop]
  wait      REQUEST.json [...]          qualify each in order and wait for its receipt
  choice    --tag T --config CFG --root ROOT --workers 1,5,10 --inventory INVENTORY.json
  produce   --tag T --config CFG --runtime RUNTIME.json --root ROOT --workers W --cores 0-9 [--host desktop]
  dispatch  REQUEST.json [...]          production dispatch and wait
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TOOLS = REPO / "python" / "tools"
sys.path.insert(0, str(TOOLS))
import native_expanded_dispatch_v1 as dispatch  # noqa: E402

LANE = "claude-search-policy-pilot-20261012"
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


def cold_of(root: Path) -> Path:
    """Hot outputs on the SSD scratch root, the recovery copy on E: (desktop) or a sibling (Haley)."""
    text = str(root)
    return Path("E:" + text[2:]) if text[:2].upper() == "D:" else Path(text + "-cold")


def storage(root: Path, projected: int) -> dict:
    """Each arm root is a quarter of the pilot's 64 GiB physical-copy allowance (hot plus recovery)."""
    hot, cold = Path(root), cold_of(Path(root))
    volumes = {hot.drive.upper(): projected * 3 // 4}
    volumes[cold.drive.upper()] = volumes.get(cold.drive.upper(), 0) + projected - projected * 3 // 4
    return {"accounting_roots": [str(hot).replace("\\", "/"), str(cold).replace("\\", "/")],
            "max_logical_bytes": 16 * GiB, "reserve_bytes": 60 * GiB,
            "projected_additional_bytes": projected, "projected_volume_bytes": volumes}


def request(config: Path, runtime: Path, root: Path, workers: int, cpus: list[int], tag: str, wall: int,
            host: str) -> dict:
    root = Path(root)
    cfg = json.loads(Path(config).read_text())
    return {"schema": dispatch.SCHEMA, "kind": "training", "lane": LANE, "runtime": pin(runtime),
            "config": pin(config), "root": str(root / "hot" / tag).replace("\\", "/"),
            "cold_root": str(cold_of(root) / "cold" / tag).replace("\\", "/"),
            "placement": {"host": host, "cpu_affinity": cpus, "workers": workers,
                          "preparation_workers": 1, "memory_bytes": 16 * GiB},
            "storage": storage(root, GiB if tag.startswith("qual-") else 12 * GiB), "wall_seconds": wall,
            "non_natural_tolerance": cfg["max_non_natural_episode_fraction"]}


def wait_root(root: Path, timeout: float) -> dict:
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
    """A busy refusal (a build on the host) starts nothing; retry the same request for up to two hours."""
    req = json.loads(Path(path).read_text())
    for _ in range(240):
        done = subprocess.run([sys.executable, "-B", DISPATCH, action, str(Path(path).resolve())],
                              capture_output=True, text=True)
        if done.returncode == 0:
            return wait_root(Path(req["root"]), timeout)
        if "busy refusal" not in done.stderr:
            break
        time.sleep(30)
    raise RuntimeError(f"{action} {path}: {done.stderr[-2000:]}")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    r = sub.add_parser("runtime")
    for k in ("binary", "config", "out"):
        r.add_argument(f"--{k}", type=Path, required=True)
    q, p = sub.add_parser("requests"), sub.add_parser("produce")
    for s in (q, p):
        s.add_argument("--tag", required=True)
        for k in ("config", "runtime", "root"):
            s.add_argument(f"--{k}", type=Path, required=True)
        s.add_argument("--cores", required=True)
        s.add_argument("--host", default="desktop")
        s.add_argument("--wall", type=int)
    q.add_argument("--workers", required=True)
    p.add_argument("--workers", type=int, required=True)
    for name in ("wait", "dispatch"):
        sub.add_parser(name).add_argument("requests", type=Path, nargs="+")
    c = sub.add_parser("choice")
    c.add_argument("--tag", required=True)
    for k in ("config", "root", "inventory"):
        c.add_argument(f"--{k}", type=Path, required=True)
    c.add_argument("--reports", nargs="+", type=Path, required=True)
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
            tag = f"qual-{a.tag}-{a.host}-w{w}"
            paths.append(write(a.root / "requests" / f"{tag}.json",
                               request(a.config, a.runtime, a.root, w, cores(a.cores), tag, a.wall or 5400,
                                       a.host))["path"])
        print(json.dumps(paths))
    elif a.op == "wait":
        for path in a.requests:
            report = run_and_wait("qualify", path, 6 * 3600)
            print(json.dumps({"request": str(path), "seconds": report["seconds"]}), flush=True)
    elif a.op == "choice":
        # The inventory (all three placements, with evidence pins) is assembled by the operator from
        # current census files; the fastest qualified report for this workload family is selected.
        inventory = json.loads(a.inventory.read_text())
        reports = [pin(path) for path in a.reports]
        seconds = {r["path"]: json.loads(Path(r["path"]).read_text())["seconds"] for r in reports}
        family = dispatch.workload(json.loads(a.config.read_text()), "training")
        best = min(reports, key=lambda r: seconds[r["path"]])
        choice = {"schema": dispatch.CHOICE, "inventory": inventory, "qualifications": reports,
                  "selected": {family: best}}
        path = write(a.root / "choice" / f"choice-{a.tag}.json", choice)
        report = json.loads(Path(best["path"]).read_text())
        print(json.dumps({"choice": path, "best": best["path"], "placement": report["placement"],
                          "seconds": seconds}))
    elif a.op == "produce":
        tag = f"prod-{a.tag}"
        req = request(a.config, a.runtime, a.root, a.workers, cores(a.cores), tag, a.wall or 20 * 3600, a.host)
        state = a.root / "choice"
        req["choice"] = pin(state / f"choice-{a.tag}.json")
        req["choice_verification_output"] = str(state / f"verification-{a.tag}.json")
        check = write(a.root / "requests" / f"check-{a.tag}.json", req)
        done = subprocess.run([sys.executable, "-B", DISPATCH, "check-choice", check["path"]],
                              capture_output=True, text=True)
        if done.returncode != 0:
            raise SystemExit(done.stderr[-2000:])
        req.pop("choice_verification_output")
        req["choice_verification"] = pin(state / f"verification-{a.tag}.json")
        print(json.dumps(write(a.root / "requests" / f"{tag}.json", req)))
    else:
        for path in a.requests:
            report = run_and_wait("dispatch", path, 24 * 3600)
            print(json.dumps({"request": str(path), "seconds": report["seconds"]}), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
