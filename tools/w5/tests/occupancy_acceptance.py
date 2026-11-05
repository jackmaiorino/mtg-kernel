"""E10 occupancy acceptance (amendment A1's default) on a real cgroup v2 host (run as root; CI runs it).

Two boxes. Chain `a` burns in box 0; chain `b` pauses in box 1, whose filler is made to die (a marker file):
- cycle 1: the filler dies; the outage opens and `a` freezes at once; verified restarts fail; the box faults and
  escalates at once (plan stage 1b); a refill while the filler is still broken fails and changes nothing; a refill
  after the repair is verified, restores the box and `a` thaws and burns;
- cycle 2: the healthy filler is killed and dies again on restart: a second outage, fault, escalation and refill;
- cycle 3: the healthy filler is killed and its restart is verified: the outage closes by restart, `a` thaws;
- an operator `node-freeze` and `node-thaw` freeze and thaw `a`;
- `b` resumes and both chains finish normally.
Checks the event counts, the affected chains, exactly-once copying of node events into `a`'s ledger, the
escalation file, and that the node ends clear. Writes a JSON receipt.

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

    def events(kind):
        if not node.path.exists():
            return []
        with node.locked():
            records, _ = node.read()
        return [r for r in records if r["event"] == kind]

    def frozen_steady(seconds=1.5):
        first = a_cg.usage_usec()
        time.sleep(seconds)
        return a_cg.frozen() and a_cg.usage_usec() == first

    def burning(seconds=1.5):
        wait_for(lambda: not a_cg.frozen(), 30, "a thawed")
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
            wait_for(lambda: len(events("outage-start")) == cycle, 120, f"outage {cycle}")
            wait_for(lambda: a_cg.frozen(), 30, f"freeze at outage {cycle}")
            wait_for(lambda: len(events("fault")) == cycle, 120, f"fault {cycle}")
            checks[f"cycle{cycle}_neighbour_frozen"] = frozen_steady()
            checks[f"cycle{cycle}_escalates_in_stage_1b"] = len(events("escalate")) == cycle
            if cycle == 1:  # a refill before the repair is refused by its own verification
                launcher("refill", *common, "--chain", "b")
                wait_for(lambda: len(events("refill-failed")) == 1, 60, "failed refill")
                checks["failed_refill_keeps_freeze"] = frozen_steady() and len(events("restored")) == 0
            die.unlink()
            launcher("refill", *common, "--chain", "b")
            wait_for(lambda: len(events("restored")) == cycle, 60, f"restored {cycle}")
            checks[f"cycle{cycle}_refill_verified"] = events("restored")[-1]["verification"]["ok"]
            checks[f"cycle{cycle}_neighbour_thaws"] = burning()
        b_filler.kill()  # cycle 3: a verified restart closes the outage
        wait_for(lambda: len(events("outage-start")) == 3, 60, "outage 3")
        wait_for(lambda: len(events("restored")) == 3, 60, "restored by restart")
        checks["cycle3_restored_by_verified_restart"] = (events("restored")[-1]["how"] == "restart"
                                                         and events("restored")[-1]["attempts"][-1]["ok"])
        checks["cycle3_neighbour_thaws"] = burning()
        launcher("node-freeze", *common, "--reason", "acceptance")
        checks["operator_freeze"] = frozen_steady()
        launcher("node-thaw", *common, "--reason", "acceptance")
        checks["operator_thaw"] = burning()
        launcher("resume", *common, "--chain", "b", "--payload", "{}")
        checks["chains_finish"] = sup_a.wait(timeout=300) == 0 and sup_b.wait(timeout=300) == 0
        with node.locked():
            records, torn = node.read()
        starts = [r for r in records if r["event"] == "outage-start"]
        restored = [r for r in records if r["event"] == "restored"]
        checks["event_counts"] = (len(starts) == 3 and len(restored) == 3 and len(events("fault")) == 2
                                  and len(events("refill-failed")) == 1 and len(events("escalate")) == 2
                                  and [r["how"] for r in restored] == ["refill", "refill", "restart"])
        checks["outages_name_affected_chains"] = all({o["chain"] for o in r["affected"]} == {"a", "b"}
                                                     for r in starts)
        copied = [r["index"] for r in led_a.records() if r["event"] == "node-event"]
        checks["node_events_copied_exactly_once"] = copied == list(range(len(copied))) and len(copied) <= len(records)
        checks["escalation_file"] = (work / "state" / "ESCALATE.json").exists()
        checks["node_clear_at_end"] = not w5box.NodeLog.state(records, torn)["frozen"]
        checks["ledgers_close_cleanly"] = led_a.summary()["open"] is None and led_b.summary()["open"] is None
        receipt.update({"node_events": [r["event"] for r in records], "restored": restored})
    finally:
        for box in ("box0", "box1"):
            for leaf in ("chain", "filler"):
                cg = w5box.Cgroup(cgroup_root / box / leaf)
                if cg.exists():
                    cg.write("cgroup.freeze", 0)
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
