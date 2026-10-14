"""Advance only the lead-approved qualification and cost calibration stages."""
import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time


def run(plan):
    root = Path(plan["root"])
    log = (root / plan.get("log_filename", "LADDER-DRIVER.log")).open("a", buffering=1)
    sys.stdout = sys.stderr = log
    module = importlib.util.spec_from_file_location("frozen_runtime", plan["launcher"])
    runtime = importlib.util.module_from_spec(module)
    module.loader.exec_module(runtime)
    load, publish, require = runtime.load, runtime.publish, runtime.require
    state = {"pid": os.getpid(), "started_epoch": time.time(), "state": "running", "stages": []}
    own_start = time.process_time()
    cli_cpu = 0.0

    def save():
        state["epoch"] = time.time()
        publish(root / "LADDER-DRIVER.json", state)

    def invoke(arguments):
        nonlocal cli_cpu
        child = subprocess.Popen([sys.executable, *arguments], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        clock = runtime.ProcessClock(child)
        while clock.poll() is None:
            time.sleep(.1)
        stdout, stderr = child.communicate()
        cli_cpu += clock.read()
        clock.close()
        require(child.returncode == 0, stderr or stdout)
        return stdout

    def accounted():
        ledger = root / "CPU-LEDGER.jsonl"
        rows = [json.loads(line) for line in ledger.read_text().splitlines() if line.strip()]
        return plan["external_cpu_seconds"] + runtime.accounted_cpu(rows)

    def checkpoint_cpu():
        cpu = time.process_time() - own_start + cli_cpu
        with runtime.ledger_owner(root / "CPU-LEDGER.jsonl"):
            runtime.append_ledger(root / "CPU-LEDGER.jsonl", {"label": plan["control_label"], "phase": "control",
                "cpu_seconds": cpu, "reserved_cpu_seconds": math.ceil(cpu) + 1,
                "accounting_basis": "observed control CPU plus one second finalization reserve", "error": None})

    def inspect(batch, spec, cost):
        require(not batch["error"], "guarded batch failed")
        diagnostics = []
        for job, definition in zip(batch["results"], spec["jobs"]):
            require(job and job.get("exit_code") == 0, "missing completed native job")
            rows = [json.loads(line) for line in Path(job["output"]).read_text().splitlines() if line.strip()]
            require(len(rows) == 1, "expected one root per native process")
            row = rows[0]
            require(row.get("technical_incomplete") is False and row["selection"].get("incomplete") is False
                    and row["selection"].get("faults") == [], "technical search/evaluation failure")
            if cost:
                require(row["evaluation"].get("performed") is False and row["safety_cap"] == row["anchor"]
                        and row["selection"]["transitions"] == row["anchor"] and row["safety_cap_reached"] is True,
                        "baseline did not complete its actual transition anchor")
            else:
                evaluation = row["evaluation"]
                require(row["work_attempts"] == definition["env"]["PROFILE_WORK_LIMIT"] or
                        row["work_attempts"] == int(definition["env"]["PROFILE_WORK_LIMIT"]), "fixed work incomplete")
                require(evaluation["continuation"] == "frozen-search-tree" and evaluation["scheduled"] == 128
                        and len(evaluation["samples"]) == 128 and evaluation["independent_natural"] == 128,
                        "fresh evaluation schedule incomplete")
            teacher = row["teacher"]
            backs = teacher.get("edge_backups", [])
            sharing = row["selection"].get("extra", {}).get("sharing", {})
            diagnostics.append({"id": job["id"], "arm": row["arm"], "native_cpu_seconds": job["cpu_seconds"],
                "wall_seconds": job["wall_seconds"], "work_attempts": row["work_attempts"],
                "transitions": row["selection"]["transitions"], "pure_evaluation_cpu_seconds": row["pure_evaluation_cpu_seconds"],
                "completed_worlds": row["evaluation"].get("independent_natural", 0), "technical_incomplete": False,
                "root_supported": teacher.get("supported"), "root_edges": len(backs),
                "root_edges_at_least_eight_backups": sum(n >= 8 for n in backs),
                "sharing": {k: sharing.get(k) for k in ("probes", "candidate_views", "eligible_endpoints",
                    "cross_history_endpoints", "guidance_hits", "guidance_changes", "fresh_pool_samples", "groups")}})
        return diagnostics

    def execute(manifest, phase, workers, label, cost, already_issued=False):
        spec = load(manifest)
        state.update(current={"label": label, "manifest": str(manifest), "workers": workers,
                              "receipt": str(root / f"DISPATCH-{label}.json")})
        save()
        if not already_issued:
            invoke([plan["launcher"], "dispatch", "--manifest", str(manifest), "--phase", phase,
                    "--workers", str(workers), "--label", label])
        receipt = root / f"DISPATCH-{label}.json"
        while True:
            if receipt.exists():
                dispatch = load(receipt)
                state["current"]["dispatch"] = {k: dispatch.get(k) for k in ("state", "pid", "slots_pid", "claim_id")}
                if dispatch["state"] in ("completed", "failed"):
                    break
            save()
            time.sleep(5)
        require(dispatch["state"] == "completed" and dispatch.get("claim_release_verified") is True,
                "dispatch failed or claim was not released")
        batch = load(dispatch["completion"])
        diagnostics = inspect(batch, spec, cost)
        state["stages"].append({"label": label, "workers": workers, "cpu_seconds": batch["cpu_seconds"],
            "wall_seconds": batch["wall_seconds"], "guard_cpu_seconds": dispatch["guard_cpu_seconds"], "diagnostics": diagnostics})
        checkpoint_cpu()
        state["accounted_cpu_seconds"] = accounted()
        save()
        return Path(dispatch["completion"]), batch

    try:
        selections = {}
        for ladder in plan["ladders"]:
            manifest = Path(ladder["manifest"])
            require(runtime.digest(manifest) == ladder["manifest_sha256"], "approved manifest changed")
            batches = []
            reference = None
            for index, workers in enumerate((1, 4, 8)):
                if index:
                    projection = max(previous["cpu_seconds"], reference["cpu_seconds"]) * 1.5
                    require(projection <= 1200, "projected qualification CPU exceeds phase bound")
                    require(accounted() + projection < 43200, "remaining aggregate CPU cannot fit projected stage")
                label = f"{ladder['label_prefix']}-w{workers}-{plan['attempt']}"
                path, previous = execute(manifest, "qualify", workers, label, ladder["cost_only"],
                    already_issued=ladder.get("adopt_first") is True and index == 0)
                reference = reference or previous
                require([r["primary_sha256"] for r in previous["results"]] ==
                        [r["primary_sha256"] for r in reference["results"]], "serial/parallel primary and evaluation parity failed")
                batches.append(path)
            output = root / ladder["qualification_filename"]
            invoke([plan["launcher"], "qualify", *sum((["--batch", str(p)] for p in batches), []), "--out", str(output)])
            receipt = load(output)
            fastest = int(max(receipt["rates"], key=lambda n: receipt["rates"][n]["completed_per_second"]))
            selections[ladder["name"]] = {"receipt": str(output), "fastest_workers": fastest, "serial_completion": str(batches[0])}
            state["qualified"] = selections
            save()
        remaining = plan["remaining_calibration"]
        output = root / "REMAINING-COST-CALIBRATION-MANIFEST.json"
        arguments = [a.replace("{cost_qualification}", selections["cost"]["receipt"]).replace("{out}", str(output))
                     for a in remaining["prepare_argv"]]
        invoke(arguments)
        spec = load(output)
        for key in ("runtime_commit", "runtime_helper", "toolchain", "binary_preservation"):
            spec[key] = load(plan["ladders"][1]["manifest"])[key]
        spec["inputs"] += remaining["extra_inputs"]
        publish(output, spec)
        path, _ = execute(output, "calibrate", selections["cost"]["fastest_workers"], f"cal-remaining-anchors-{plan['attempt']}", True)
        invoke([plan["calibrator"], "--completion", selections["cost"]["serial_completion"], "--completion", str(path),
                "--out", str(root / "COST-CALIBRATION.json")])
        require(len(load(root / "COST-CALIBRATION.json")["roots"]) == 9, "full calibration missing roots")
        state.update(state="completed-awaiting-formal-review", calibration=str(root / "COST-CALIBRATION.json"))
        checkpoint_cpu()
        save()
    except BaseException as error:
        state.update(state="stopped", error=f"{type(error).__name__}: {error}")
        if not (root / "CPU-LEDGER.jsonl.lock").exists():
            checkpoint_cpu()
        save()
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True)
    args = parser.parse_args()
    run(json.loads(Path(args.plan).read_text()))
