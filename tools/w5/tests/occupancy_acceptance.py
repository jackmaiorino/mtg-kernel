"""E10 occupancy acceptance (amendment A1's default) on a real cgroup v2 host (run as root; CI runs it).

Two boxes. Chain `a` burns in box 0; chain `b` pauses in box 1, whose filler is made to die:
- fault 1: the filler dies FILLER_RESTARTS + 1 times (each exit and restart is an outage event), the box faults,
  the node freezes `a` (its CPU stays constant) and escalates at once (plan stage 1b);
- `refill` (after the filler is repaired) verifies the new filler and clears the fault; `a` thaws and burns again;
- fault 2: the healthy filler is killed and dies again on restart: a second fault, freeze and escalation, then a
  second verified refill and clear (repeated fault and clear events);
- an operator `node-freeze` and `node-thaw` freeze and thaw `a`;
- `b` resumes and both chains finish normally.
Writes a JSON receipt.

  sudo python3 occupancy_acceptance.py --receipt OUT.json
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAUNCHER = HERE.parent / "w5box.py"
sys.path.insert(0, str(HERE.parent))
import w5box  # noqa: E402

BURN = HERE / "burn_chain.py"


def launcher(*args, check=True):
    proc = subprocess.run([sys.executable, str(LAUNCHER), *args], capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise RuntimeError(f"w5box {args[0]} failed {proc.returncode}: {proc.stderr}")
    return proc


def wait_for(predicate, timeout, what):
    deadline = time.monotonic() + timeout
    while not predicate():
        if time.monotonic() > deadline:
            raise RuntimeError(f"timed out waiting for {what}")
        time.sleep(0.1)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--receipt", type=Path, required=True)
    args = parser.parse_args()
    work = Path(tempfile.mkdtemp(prefix="w5box-occupancy-"))
    layout = w5box.compute_layout(w5box.read_topology(), 1, 0, 2)
    cgroup_root = Path(f"/sys/fs/cgroup/w5occ{os.getpid()}")
    die = work / "filler-broken"
    py = {"path": sys.executable, "sha256": w5box.sha256_file(sys.executable)}
    ref = {"path": str(BURN), "sha256": w5box.sha256_file(BURN)}

    def chain(slot, argv):
        return {"box_slot": slot, "arm": "test", "argv": [sys.executable, str(BURN), *argv], "seed_root": "w5/t",
                "memory_max_bytes": 1 << 30, "binary": py, "config": ref}

    plan = {"schema": w5box.PLAN_SCHEMA, "host_class": w5box.host_class(), "layout": layout,
            "cgroup_root": str(cgroup_root), "state_root": str(work / "state"), "isolation": "none", "stage": "1b",
            "chains": {"a": chain(0, ["chain", "--out", str(work / "a"), "--seconds", "20", "--workers", "1"]),
                       "b": chain(1, ["chain", "--out", str(work / "b"), "--seconds", "1", "--workers", "1",
                                      "--pause"])},
            "filler": {"argv": [sys.executable, str(BURN), "filler", "--workers", "2", "--die-if", str(die)],
                       "memory_max_bytes": 1 << 30, "seed_root_template": "w5/filler{j}", "binary": py,
                       "config": ref}}
    plan_path = work / "plan.json"
    plan_path.write_text(json.dumps(plan, indent=2))
    common = ["--plan", str(plan_path)]
    qual = ["--qualification", "e10-occupancy", "--max-wall-seconds", "900"]
    node = w5box.NodeLog(work / "state")
    led_a = w5box.Ledger(work / "state" / "a" / "ledger.jsonl")
    led_b = w5box.Ledger(work / "state" / "b" / "ledger.jsonl")
    a_cg = w5box.Cgroup(cgroup_root / "box0" / "chain")
    b_filler = w5box.Cgroup(cgroup_root / "box1" / "filler")

    def node_events(kind):
        return [r for r in node.records() if r["event"] == kind] if node.ledger.path.exists() else []

    def a_events(kind):
        return [r for r in led_a.records() if r["event"] == kind] if led_a.path.exists() else []

    def frozen_steady(seconds=1.5):
        first = a_cg.usage_usec()
        time.sleep(seconds)
        return a_cg.frozen() and a_cg.usage_usec() == first

    def burning(seconds=1.5):
        first = a_cg.usage_usec()
        time.sleep(seconds)
        return not a_cg.frozen() and a_cg.usage_usec() > first

    receipt, checks = {"layout": layout}, {}
    try:
        die.write_text("broken")
        sup_a = subprocess.Popen([sys.executable, str(LAUNCHER), "run", *common, "--chain", "a", *qual])
        sup_b = subprocess.Popen([sys.executable, str(LAUNCHER), "run", *common, "--chain", "b", *qual])
        for cycle in (1, 2):
            if cycle == 2:
                die.write_text("broken")
                b_filler.kill()  # the healthy filler crashes; its restarts die again
            wait_for(lambda: len(node_events("fault")) == cycle, 120, f"fault {cycle}")
            wait_for(lambda: len(a_events("occupancy-freeze")) == cycle, 30, f"freeze {cycle}")
            checks[f"fault{cycle}_freezes_neighbour"] = frozen_steady()
            checks[f"fault{cycle}_escalates_in_stage_1b"] = len(node_events("escalate")) == cycle
            die.unlink()
            launcher("refill", *common, "--chain", "b")
            wait_for(lambda: len(node_events("clear")) == cycle, 60, f"clear {cycle}")
            wait_for(lambda: len(a_events("occupancy-thaw")) == cycle, 30, f"thaw {cycle}")
            checks[f"clear{cycle}_thaws_neighbour"] = burning()
            checks[f"clear{cycle}_refill_verified"] = all(
                v for k, v in node_events("clear")[-1]["verification"].items() if k != "burned_usec")
        launcher("node-freeze", *common, "--reason", "acceptance")
        wait_for(lambda: len(a_events("occupancy-freeze")) == 3, 30, "operator freeze")
        checks["operator_freeze"] = frozen_steady()
        launcher("node-thaw", *common, "--reason", "acceptance")
        wait_for(lambda: len(a_events("occupancy-thaw")) == 3, 30, "operator thaw")
        checks["operator_thaw"] = burning()
        launcher("resume", *common, "--chain", "b", "--payload", "{}")
        checks["chains_finish"] = sup_a.wait(timeout=300) == 0 and sup_b.wait(timeout=300) == 0
        records = node.records()
        starts = [r for r in records if r["event"] == "outage-start"]
        ends = [r for r in records if r["event"] == "outage-end"]
        seen = [r for r in led_a.records() if r["event"] == "node-event"]
        checks["every_outage_recorded"] = (len(starts) == 2 * (w5box.FILLER_RESTARTS + 1)
                                           and len(ends) == 2 * w5box.FILLER_RESTARTS + 2)
        checks["outages_name_affected_chains"] = all(
            {o["chain"] for o in r["affected"]} == {"a", "b"} for r in starts)
        checks["neighbour_ledger_records_every_node_event"] = len(seen) == len(records)
        checks["escalation_file"] = (work / "state" / "ESCALATE.json").exists()
        state = w5box.NodeLog.state(records)
        checks["node_clear_at_end"] = not state["frozen"]
        checks["ledgers_close_cleanly"] = (led_a.summary()["open"] is None and led_b.summary()["open"] is None)
        receipt.update({"node_events": [r["event"] for r in records], "outage_starts": len(starts),
                        "outage_ends": len(ends), "clears": node_events("clear")})
    finally:
        for box in ("box0", "box1"):
            for leaf in ("chain", "filler"):
                cg = w5box.Cgroup(cgroup_root / box / leaf)
                if cg.exists():
                    cg.freeze(False)
                    cg.kill()
                    cg.wait_empty(10)
                    cg.rmdir()
            if (cgroup_root / box).exists():
                os.rmdir(cgroup_root / box)
        if cgroup_root.exists():
            os.rmdir(cgroup_root)
    receipt["checks"] = checks
    receipt["pass"] = bool(checks) and all(checks.values())
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"checks": checks, "pass": receipt["pass"]}, indent=2))
    return 0 if receipt["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
