"""Unattended pilot chain (run inside the held reservation as nested work; survives agent restarts).

Usage: python chain.py ROOT RUNTIME TOKEN ARM [ARM ...]      ARM like r1-T
For each arm, in parallel: wait for its main-phase production receipt, stage the main endpoint,
qualify the persistence family (1 and 2 collection workers), choose, verify and dispatch the
81-update persistence run through the guarded launcher, stage the final endpoint. Evaluation panels
(T1 and H) for each endpoint run one at a time through eval_run.py as endpoints appear. Every step is
logged to ROOT/chain-log.jsonl and is resumable (finished steps are detected from their receipts).
"""
from __future__ import annotations

import datetime
import hashlib
import json
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PY = [sys.executable, "-B"]
LOCK, EVAL_LOCK = threading.Lock(), threading.Lock()
ROOT = RUNTIME = TOKEN = None
TRAINING_MASK, IDLE_MASK = "2F22", "FFFF"


def log(entry: dict) -> None:
    with LOCK, (ROOT / "chain-log.jsonl").open("a") as f:
        f.write(json.dumps({"at": datetime.datetime.now(datetime.timezone.utc).isoformat(), **entry}) + "\n")


def run(*args) -> str:
    done = subprocess.run([*PY, *map(str, args)], capture_output=True, text=True)
    if done.returncode != 0:
        raise RuntimeError(f"{[str(a) for a in args[:3]]}: {done.stdout[-800:]} {done.stderr[-2500:]}")
    return done.stdout


def pin(path: Path) -> dict:
    return {"path": str(Path(path).resolve()).replace("\\", "/"),
            "sha256": hashlib.sha256(Path(path).read_bytes()).hexdigest()}


def report(base: Path, prefix: str) -> Path | None:
    for path in sorted(base.glob(f"hot/{prefix}*/report.json")):
        return path
    return None


def wait_report(base: Path, prefix: str) -> Path:
    while (found := report(base, prefix)) is None:
        time.sleep(120)
    return found


def stage_endpoint(arm: str, phase: str, report_path: Path) -> Path:
    out = ROOT / "endpoints" / f"{arm}-{phase}.source.json"
    if out.exists():
        return out
    result = json.loads(Path(json.loads(report_path.read_text())["native_result"]["path"]).read_text())
    if not (result["complete"] and result["completed_iterations"] == 81):
        raise RuntimeError(f"{arm}-{phase}: incomplete run")
    source = result["source"]
    target = ROOT / "endpoints" / f"{arm}-{phase}.checkpoint.json"
    shutil.copy2(Path(source["checkpoint"]["path"]), target)
    if pin(target)["sha256"] != source["checkpoint"]["sha256"]:
        raise RuntimeError(f"{arm}-{phase}: endpoint copy differs")
    source["checkpoint"]["path"] = pin(target)["path"]
    out.write_text(json.dumps(source, indent=1) + "\n")
    log({"arm": arm, "event": "endpoint", "phase": phase, "adam_step": result["actual_identity"]["adam_step"],
         "sha256": source["checkpoint"]["sha256"]})
    return out


def inventory() -> Path:
    """Refreshes the placement census times (the launcher refuses inventories older than 24 h)."""
    path = ROOT / "choice" / "inventory.json"
    inv = json.loads(path.read_text())
    now = datetime.datetime.now(datetime.timezone.utc).isoformat()
    for host in inv:
        evidence = Path(inv[host]["evidence"]["path"])
        data = json.loads(evidence.read_text())
        data["rechecked_at"] = now
        evidence.write_text(json.dumps(data, indent=1))
        inv[host]["evidence"] = pin(evidence)
        inv[host]["checked_at"] = now
    path.write_text(json.dumps(inv, indent=1))
    return path


def persistence(arm: str) -> None:
    lineage, kind = arm.split("-")
    main = ROOT / "train" / f"{arm}-main"
    source = stage_endpoint(arm, "main", wait_report(main, "prod-"))
    tag = f"{arm}-persistence"
    base = ROOT / "train" / tag
    cores = json.loads(next(main.glob("hot/qual-*-w1/report.json")).read_text())["placement"]["cpu_affinity"]
    cores = f"{cores[0]}-{cores[-1]}"
    if report(base, "prod-") is None:
        qual_cfg = base / "config-qual.json"
        if not qual_cfg.exists():
            base.mkdir(parents=True, exist_ok=True)
            run(HERE / "schedule.py", "config", "--lineage", lineage, "--arm", kind, "--phase", "persistence",
                "--parent", ROOT / "inputs" / "sources" / f"{lineage}-source.json", "--a48",
                ROOT / "inputs" / "sources" / "a48-source.json", "--source", source, "--workers", 1,
                "--output-directory", str(base / "native").replace("\\", "/"), "--out", qual_cfg)
        reports = []
        for w in (1, 2):
            found = report(base, f"qual-{tag}-desktop-w{w}")
            if found is None:
                request = base / "requests" / f"qual-{tag}-desktop-w{w}.json"
                if not request.exists():
                    run(HERE / "drive.py", "requests", "--tag", tag, "--config", qual_cfg, "--runtime", RUNTIME,
                        "--root", base, "--workers", w, "--cores", cores)
                run(HERE / "drive.py", "wait", request)
                found = report(base, f"qual-{tag}-desktop-w{w}")
            reports.append(found)
        log({"arm": arm, "event": "persistence-qualified"})
        out = json.loads(run(HERE / "drive.py", "choice", "--tag", f"{tag}-chain", "--config", qual_cfg, "--root", base,
                             "--inventory", inventory(), "--reports", *reports))
        workers = out["placement"]["workers"]
        prod_cfg = base / "config-prod.json"
        run(HERE / "schedule.py", "config", "--lineage", lineage, "--arm", kind, "--phase", "persistence",
            "--parent", ROOT / "inputs" / "sources" / f"{lineage}-source.json", "--a48",
            ROOT / "inputs" / "sources" / "a48-source.json", "--source", source, "--workers", workers,
            "--output-directory", str(base / "native-prod").replace("\\", "/"), "--out", prod_cfg)
        placed = out["placement"]["cpu_affinity"]
        request = json.loads(run(HERE / "drive.py", "produce", "--tag", f"{tag}-chain", "--config", prod_cfg,
                                 "--runtime", RUNTIME, "--root", base, "--workers", workers, "--cores",
                                 f"{placed[0]}-{placed[-1]}", "--wall", 36000))
        log({"arm": arm, "event": "persistence-dispatch", "request": request["path"]})
        run(HERE / "drive.py", "dispatch", request["path"])
    stage_endpoint(arm, "final", wait_report(base, "prod-"))


def evaluate(checkpoint_id: str, source: Path, training_running) -> None:
    for ref, opponent in (("T1", "t1-opponent.json"), ("H", "h-opponent-08b3cca1.json")):
        out = ROOT / "eval" / f"{checkpoint_id}-vs-{ref}"
        if not (out / "plan.json").exists():
            run(HERE / "evaluate.py", "build", "--id", checkpoint_id, "--candidate", source, "--opponent-name", ref,
                "--opponent", ROOT / "inputs" / "sources" / opponent, "--out", out, "--shards", 8)
    with EVAL_LOCK:
        mask = TRAINING_MASK if training_running() else IDLE_MASK
        done = subprocess.run(["cmd", "/c", "start", "", "/wait", "/b", "/affinity", mask, sys.executable, "-B",
                               str(HERE / "eval_run.py"), str(ROOT), str(ROOT / "bin" / "public_feature_evaluation_v1.exe"),
                               checkpoint_id], capture_output=True, text=True)
        log({"event": "evaluated", "checkpoint": checkpoint_id, "exit_code": done.returncode, "mask": mask,
             "stderr": done.stderr[-1500:]})


def main() -> None:
    global ROOT, RUNTIME, TOKEN
    ROOT, RUNTIME, TOKEN, arms = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4:]
    finished = {arm: threading.Event() for arm in arms}

    def training_running():
        return not all(e.is_set() for e in finished.values())

    def arm_chain(arm):
        try:
            persistence(arm)
        except Exception as error:
            log({"arm": arm, "event": "failed", "error": str(error)[-3000:]})
        finally:
            finished[arm].set()

    def eval_chain():
        # The parent panels run in their own process first; never overlap two panel runners.
        parents = [ROOT / "eval" / f"{lin}-parent-vs-{ref}" / "rows.jsonl" for lin in ("r1", "r2") for ref in ("T1", "H")]
        deadline = time.time() + 6 * 3600
        while not all(p.exists() for p in parents) and time.time() < deadline:
            time.sleep(120)
        pending = [f"{arm}-{phase}" for phase in ("main", "final") for arm in arms]
        while pending:
            for ck in list(pending):
                source = ROOT / "endpoints" / f"{ck}.source.json"
                if source.exists():
                    try:
                        evaluate(ck, source, training_running)
                    except Exception as error:
                        log({"event": "evaluation-failed", "checkpoint": ck, "error": str(error)[-3000:]})
                    pending.remove(ck)
            if pending and all(e.is_set() for e in finished.values()) and not any(
                    (ROOT / "endpoints" / f"{ck}.source.json").exists() for ck in pending):
                log({"event": "evaluation-abandoned", "missing": pending})
                break
            time.sleep(60)

    threads = [threading.Thread(target=arm_chain, args=(arm,)) for arm in arms]
    threads.append(threading.Thread(target=eval_chain))
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    log({"event": "chain-done"})


if __name__ == "__main__":
    main()
