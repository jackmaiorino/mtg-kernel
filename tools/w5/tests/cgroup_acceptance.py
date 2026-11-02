"""E10 acceptance on a real cgroup v2 host (run as root; CI runs it on an Ubuntu runner).

Through the real launcher (`w5box.py run`, qualification mode), in one box:
- chain `t1`, instances 0 and 1 (a restart): workers burn, the chain pauses (instance 0 at an "update boundary"
  by the trainer handshake, instance 1 by the controller's `pause --now`), the launcher freezes it and starts the filler in the same box, the filler
  burns for FILLER_SECONDS while the chain's cgroup is sampled, `w5box.py resume` freezes and kills the filler and
  thaws the chain, more workers burn, exit.
- chain `t2`: an OOM kill is recorded as a chain failure.
- chain `t3`: the supervisor is SIGKILLed mid-run; after the chain is killed, the next run closes the instance
  exactly from the surviving cgroup and continues.
- chain `t4`: as t3, but the chain cgroup is removed (a reboot): the next run records a lower bound and refuses
  to continue until `--accept-lower-bound` is given.
Checks: t1's ledger within 1% of the summed per-process CPU (wait4 of every process), the ledger unchanged across
each pause and filler run while the filler burned CPU in the box, chain processes confined to the box, and the
t2 to t4 outcomes. Writes a JSON receipt.

  sudo python3 cgroup_acceptance.py --receipt OUT.json [--isolation partition-root|none]
"""
import argparse
import json
import os
import signal
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
SECONDS = 3.0
WORKERS = 2
FILLER_SECONDS = 6.0


def launcher(*args, check=True):
    proc = subprocess.run([sys.executable, str(LAUNCHER), *args], capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise RuntimeError(f"w5box {args[0]} failed {proc.returncode}: {proc.stderr}")
    return proc


def start(*args):
    return subprocess.Popen([sys.executable, str(LAUNCHER), "run", *args])


def wait_for(predicate, timeout, what):
    deadline = time.monotonic() + timeout
    while not predicate():
        if time.monotonic() > deadline:
            raise RuntimeError(f"timed out waiting for {what}")
        time.sleep(0.05)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--isolation", default="partition-root")
    args = parser.parse_args()
    work = Path(tempfile.mkdtemp(prefix="w5box-accept-"))
    layout = w5box.compute_layout(w5box.read_topology(), 1, 1, 1)
    detected = w5box.host_class()
    cgroup_root = Path(f"/sys/fs/cgroup/w5accept{os.getpid()}")
    out = work / "procs"
    ref = {"path": str(BURN), "sha256": w5box.sha256_file(BURN)}
    py = {"path": sys.executable, "sha256": w5box.sha256_file(sys.executable)}

    def chain(argv, memory):
        return {"box_slot": 0, "arm": "test", "argv": [sys.executable, str(BURN), *argv], "seed_root": "w5/test",
                "memory_max_bytes": memory, "binary": py, "config": ref}

    burn = ["chain", "--out", str(out), "--seconds", str(SECONDS), "--workers", str(WORKERS), "--pause"]
    plan = {"schema": w5box.PLAN_SCHEMA, "host_class": detected, "layout": layout,
            "cgroup_root": str(cgroup_root), "state_root": str(work / "state"), "isolation": args.isolation,
            "chains": {"t1": chain(burn, 1 << 30),
                       "t2": chain(["alloc", "--mib", "256"], 64 << 20),
                       "t3": chain(["chain", "--out", str(work / "t3"), "--seconds", "4", "--workers", "1"], 1 << 30),
                       "t4": chain(["chain", "--out", str(work / "t4"), "--seconds", "4", "--workers", "1"], 1 << 30)},
            "filler": {"argv": [sys.executable, str(BURN), "filler", "--workers", "2"], "memory_max_bytes": 1 << 30,
                       "seed_root_template": "w5/filler{j}", "binary": py, "config": ref}}
    plan_path = work / "plan.json"
    plan_path.write_text(json.dumps(plan, indent=2))
    common = ["--plan", str(plan_path)]
    qual = ["--qualification", "e10-acceptance", "--max-wall-seconds", "600"]
    box = cgroup_root / "box0"
    chain_cg, filler_cg = w5box.Cgroup(box / "chain"), w5box.Cgroup(box / "filler")
    ledger = w5box.Ledger(work / "state" / "t1" / "ledger.jsonl")
    receipt = {"host_class": detected, "kernel": os.uname().release, "layout": layout, "isolation": args.isolation}
    try:
        pauses = []
        source_path = work / "state" / "t1" / "ledger-source.json"
        for instance in (0, 1):
            if instance == 1:  # the restart pauses by the controller (freeze now), not the trainer handshake
                plan["chains"]["t1"]["argv"] = plan["chains"]["t1"]["argv"][:-1]
                plan_path.write_text(json.dumps(plan, indent=2))
            sup = start(*common, "--chain", "t1", *qual)
            if instance == 1:
                wait_for(lambda: chain_cg.exists() and chain_cg.usage_usec() > 1_500_000, 60, "instance 1 running")
                launcher("pause", *common, "--chain", "t1", "--now")
            wait_for(lambda: any(r["event"] == "pause" and r["instance"] == instance for r in ledger.records()),
                     120, f"the pause of instance {instance}")
            paused = [r for r in ledger.records() if r["event"] == "pause"][-1]["usage_usec"]
            wait_for(filler_cg.exists, 30, "the filler cgroup")
            samples, t0 = [], time.monotonic()
            while time.monotonic() - t0 < FILLER_SECONDS:
                samples.append(chain_cg.usage_usec())
                time.sleep(0.5)
            filler_cpus = w5box.parse_cpu_list(filler_cg.read("cpuset.cpus.effective"))
            source_before = json.loads(source_path.read_text())
            launcher("resume", *common, "--chain", "t1", "--payload", json.dumps({"stop_at_usec": 987654321}))
            assert sup.wait(timeout=300) == 0, f"t1 instance {instance} failed"
            source_after = json.loads(source_path.read_text())
            resume = [r for r in ledger.records() if r["event"] == "resume"][-1]
            pauses.append({"instance": instance, "paused_usec": paused, "samples": samples,
                           "resume_usec": resume["usage_usec"], "unchanged": resume["unchanged_since_pause"],
                           "filler_usage_usec": resume["filler_usage_usec"], "filler_cpus": filler_cpus,
                           "ledger_source_before": source_before, "ledger_source_after": source_after})
        # t2: an OOM kill is a chain failure
        oom = launcher("run", *common, "--chain", "t2", *qual, check=False)
        oom_close = [r for r in w5box.Ledger(work / "state" / "t2" / "ledger.jsonl").records()
                     if r["event"] == "close"][-1]
        # t3: supervisor killed, chain killed, exact recovery from the surviving cgroup
        recov = {}
        for name, lose_cgroup in (("t3", False), ("t4", True)):
            led = w5box.Ledger(work / "state" / name / "ledger.jsonl")
            sup = start(*common, "--chain", name, *qual)
            wait_for(lambda: chain_cg.exists() and chain_cg.usage_usec() > 1_000_000, 60, f"{name} running")
            os.kill(sup.pid, signal.SIGKILL)
            sup.wait()
            refused_live = launcher("run", *common, "--chain", name, *qual, check=False)
            killed_at = chain_cg.usage_usec()
            chain_cg.kill()
            wait_for(lambda: not chain_cg.populated(), 30, "the killed chain")
            final_after_kill = chain_cg.usage_usec()
            if lose_cgroup:
                chain_cg.rmdir()
            second = launcher("run", *common, "--chain", name, *qual, check=False)
            third = None
            if lose_cgroup:
                third = launcher("run", *common, "--chain", name, *qual, "--accept-lower-bound",
                                 "acceptance test", check=False)
            closes = [r for r in led.records() if r["event"] == "close"]
            recov[name] = {"refused_while_running": refused_live.returncode == w5box.EXIT_REFUSED,
                           "usage_at_kill": killed_at, "final_after_kill": final_after_kill,
                           "second_exit": second.returncode, "second_stderr": second.stderr[-300:],
                           "third_exit": None if third is None else third.returncode,
                           "closes": closes, "summary": led.summary()}

        records = ledger.records()
        closes = [r for r in records if r["event"] == "close"]
        summary = ledger.summary()
        reports = [json.loads(p.read_text()) for p in sorted(out.glob("proc-*.json"))]
        roots = [r for r in reports if r["role"] == "root"]
        self_sum = sum(r["usec"] for r in reports)
        wait4_sum = sum(c["root_rusage_usec"] for c in closes)
        total = summary["closed_total_usec"]
        t3, t4 = recov["t3"], recov["t4"]
        receipt.update({
            "instances": len(closes), "ledger_total_usec": total, "per_instance_final_usec":
            [c["final_usec"] for c in closes], "wait4_sum_usec": wait4_sum, "self_report_sum_usec": self_sum,
            "rel_diff_vs_wait4": (total - wait4_sum) / wait4_sum, "rel_diff_vs_self_reports": (total - self_sum) / self_sum,
            "pauses": pauses, "box_cpus": layout["boxes"][0]["cpus"], "chain_affinities": [r["affinity"] for r in roots],
            "oom": {"exit": oom.returncode, "close": oom_close}, "recovery": recov})
        box_cpus = layout["boxes"][0]["cpus"]
        checks = {
            "two_instances_each_paused": len(closes) == 2 and len(pauses) == 2,
            "ledger_within_1pct_of_wait4": abs(receipt["rel_diff_vs_wait4"]) <= 0.01,
            "ledger_within_1pct_of_self_reports": abs(receipt["rel_diff_vs_self_reports"]) <= 0.01,
            "unchanged_across_pause_and_filler": all(
                p["unchanged"] and p["resume_usec"] == p["paused_usec"] and all(s == p["paused_usec"] for s in p["samples"])
                for p in pauses),
            "filler_burned_in_box": all((p["filler_usage_usec"] or 0) >= 0.5 * FILLER_SECONDS * 1e6
                                        and p["filler_cpus"] == box_cpus for p in pauses),
            "chain_confined_to_box": len(roots) == 2 and all(r["affinity"] == box_cpus for r in roots),
            "ledger_source_for_the_trainer": all(
                p["ledger_source_before"]["schema"] == "w5-chain-ledger-source/v1"
                and p["ledger_source_before"]["instance"] == p["instance"]
                and p["ledger_source_after"]["stop_at_usec"] == 987654321
                and p["ledger_source_before"]["cpu_stat"].endswith("/chain/cpu.stat") for p in pauses)
            and pauses[1]["ledger_source_before"]["closed_usec"] == closes[0]["final_usec"],
            "oom_is_chain_failure": oom.returncode == w5box.EXIT_OOM and oom_close["oom_kill"] > 0
            and oom_close["chain_failure"] is True,
            "recovery_exact_from_cgroup": t3["refused_while_running"] and t3["second_exit"] == 0
            and t3["closes"][0]["recovered"] == "cgroup" and not t3["closes"][0]["lower_bound"]
            and t3["closes"][0]["final_usec"] == t3["final_after_kill"] and len(t3["closes"]) == 2,
            "lost_cgroup_needs_acceptance": t4["second_exit"] == w5box.EXIT_REFUSED
            and t4["closes"][0]["lower_bound"] is True and t4["third_exit"] == 0
            and t4["summary"]["unresolved_lower_bounds"] == [] and len(t4["closes"]) == 2,
        }
        receipt["checks"] = checks
        receipt["pass"] = all(checks.values())
    finally:
        for leaf in ("chain", "filler"):
            cg = w5box.Cgroup(box / leaf)
            if cg.exists():
                cg.kill()
                cg.wait_empty(10)
                cg.rmdir()
        for path in (box, cgroup_root):
            if path.exists():
                os.rmdir(path)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({k: receipt.get(k) for k in ("ledger_total_usec", "wait4_sum_usec", "rel_diff_vs_wait4",
                                                  "rel_diff_vs_self_reports", "checks", "pass")}, indent=2))
    return 0 if receipt.get("pass") else 1


if __name__ == "__main__":
    raise SystemExit(main())
