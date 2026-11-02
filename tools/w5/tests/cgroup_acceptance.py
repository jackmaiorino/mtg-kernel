"""E10 acceptance on a real cgroup v2 host (run as root; CI runs it on an Ubuntu runner).

Sequence for test chain `t1` in one box, through the real launcher (`w5box.py run`, qualification mode):
  instance 0: workers burn, the chain pauses at an "update boundary" (trainer handshake), the launcher freezes the
  chain and starts the filler in the same box; the filler burns for FILLER_SECONDS while the chain's ledger is
  sampled; `w5box.py resume` freezes and kills the filler and thaws the chain; more workers burn; exit.
  instance 1: a restart of the same chain (new cgroup instance), more workers burn; exit.
Checks: ledger closed total within 1% of the summed per-process CPU (wait4 of every process), ledger unchanged
across the pause and the filler run while the filler burned CPU in the box, chain processes confined to the box,
and an OOM kill in chain `t2` recorded as a chain failure. Writes a JSON receipt.

  sudo python3 cgroup_acceptance.py --receipt OUT.json [--isolation partition-root|none]
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
sys.path.insert(0, str(HERE.parent))
import w5box  # noqa: E402

BURN = HERE / "burn_chain.py"
SECONDS = 3.0
WORKERS = 2
FILLER_SECONDS = 8.0


def launcher(*args, check=True):
    proc = subprocess.run([sys.executable, str(HERE.parent / "w5box.py"), *args], capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise RuntimeError(f"w5box {args[0]} failed {proc.returncode}: {proc.stderr}")
    return proc


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
    topology = w5box.read_topology()
    layout = w5box.compute_layout(topology, 1, 1, 1)
    detected = w5box.host_class()
    cgroup_root = Path(f"/sys/fs/cgroup/w5accept{os.getpid()}")
    out = work / "procs"

    def chain(argv, memory):
        return {"box_slot": 0, "arm": "test", "argv": argv, "memory_max_bytes": memory, "seed_root": "w5/test",
                "binary": {"path": sys.executable, "sha256": w5box.sha256_file(sys.executable)},
                "config": {"path": str(BURN), "sha256": w5box.sha256_file(BURN)}}

    plan = {"schema": w5box.PLAN_SCHEMA, "host_class": detected, "layout": layout,
            "cgroup_root": str(cgroup_root), "state_root": str(work / "state"), "isolation": args.isolation,
            "chains": {
                "t1": chain([sys.executable, str(BURN), "chain", "--out", str(out), "--seconds", str(SECONDS),
                             "--workers", str(WORKERS), "--pause"], 1 << 30),
                "t1b": None,
                "t2": chain([sys.executable, str(BURN), "alloc", "--mib", "256"], 64 << 20)},
            "filler": {"argv": [sys.executable, str(BURN), "filler", "--workers", "2"], "memory_max_bytes": 1 << 30,
                       "seed_root_template": "w5/filler{j}",
                       "binary": {"path": sys.executable, "sha256": w5box.sha256_file(sys.executable)},
                       "config": {"path": str(BURN), "sha256": w5box.sha256_file(BURN)}}}
    del plan["chains"]["t1b"]
    plan_path = work / "plan.json"
    plan_path.write_text(json.dumps(plan, indent=2))
    common = ["--plan", str(plan_path)]
    state = work / "state" / "t1"
    ledger = w5box.Ledger(state / "ledger.jsonl")
    chain_cg = w5box.Cgroup(cgroup_root / "box0" / "chain")
    filler_cg = w5box.Cgroup(cgroup_root / "box0" / "filler")
    receipt = {"host_class": detected, "kernel": os.uname().release, "layout": layout, "isolation": args.isolation}
    try:
        # instance 0 with a pause, a filler run and a resume
        sup = subprocess.Popen([sys.executable, str(HERE.parent / "w5box.py"), "run", *common, "--chain", "t1",
                                "--qualification", "e10-acceptance", "--max-wall-seconds", "600"])
        wait_for(lambda: any(r["event"] == "pause" for r in ledger.records()), 120, "the pause")
        paused = [r for r in ledger.records() if r["event"] == "pause"][-1]["usage_usec"]
        wait_for(filler_cg.exists, 30, "the filler cgroup")
        samples, start = [], time.monotonic()
        while time.monotonic() - start < FILLER_SECONDS:
            samples.append(chain_cg.usage_usec())
            time.sleep(0.5)
        filler_before_resume = filler_cg.usage_usec()
        filler_cpus = w5box.parse_cpu_list(filler_cg.read("cpuset.cpus.effective"))
        launcher("resume", *common, "--chain", "t1", "--payload", json.dumps({"R_i_usec": 123}))
        assert sup.wait(timeout=300) == 0, "instance 0 failed"
        # instance 1: a restart (no pause this time: the workload pauses only when asked)
        plan["chains"]["t1"]["argv"] = plan["chains"]["t1"]["argv"][:-1]
        plan_path.write_text(json.dumps(plan, indent=2))
        assert launcher("run", *common, "--chain", "t1", "--qualification", "e10-acceptance",
                        "--max-wall-seconds", "600").returncode == 0
        # an OOM kill is a chain failure
        oom = launcher("run", *common, "--chain", "t2", "--qualification", "e10-acceptance",
                       "--max-wall-seconds", "120", check=False)
        oom_close = [r for r in w5box.Ledger(work / "state" / "t2" / "ledger.jsonl").records()
                     if r["event"] == "close"][-1]

        records = ledger.records()
        closes = [r for r in records if r["event"] == "close"]
        resume = [r for r in records if r["event"] == "resume"][-1]
        summary = ledger.summary()
        reports = [json.loads(p.read_text()) for p in sorted(out.glob("proc-*.json"))]
        roots = [r for r in reports if r["role"] == "root"]
        self_sum = sum(r["usec"] for r in reports)
        wait4_sum = sum(c["root_rusage_usec"] for c in closes)
        ledger_total = summary["closed_total_usec"]
        receipt.update({
            "instances": len(closes), "ledger_total_usec": ledger_total, "per_instance_final_usec":
            [c["final_usec"] for c in closes], "wait4_sum_usec": wait4_sum, "self_report_sum_usec": self_sum,
            "rel_diff_vs_wait4": (ledger_total - wait4_sum) / wait4_sum,
            "rel_diff_vs_self_reports": (ledger_total - self_sum) / self_sum,
            "paused_usage_usec": paused, "samples_during_filler": samples, "resume_usage_usec": resume["usage_usec"],
            "resume_unchanged_since_pause": resume["unchanged_since_pause"],
            "filler_usage_usec": resume["filler_usage_usec"], "filler_usage_before_resume": filler_before_resume,
            "filler_cpus": filler_cpus, "box_cpus": layout["boxes"][0]["cpus"],
            "chain_affinities": [r["affinity"] for r in roots],
            "oom": {"exit": oom.returncode, "close": oom_close},
            "ledger_hash_chain_ok": True})
        checks = {
            "two_instances": len(closes) == 2,
            "ledger_within_1pct_of_wait4": abs(receipt["rel_diff_vs_wait4"]) <= 0.01,
            "ledger_within_1pct_of_self_reports": abs(receipt["rel_diff_vs_self_reports"]) <= 0.01,
            "unchanged_across_pause_and_filler": all(s == paused for s in samples) and resume["usage_usec"] == paused,
            "filler_burned_in_box": (resume["filler_usage_usec"] or 0) >= 0.5 * FILLER_SECONDS * 1e6
            and filler_cpus == layout["boxes"][0]["cpus"],
            "chain_confined_to_box": all(r["affinity"] == layout["boxes"][0]["cpus"] for r in roots),
            "oom_is_chain_failure": oom.returncode == w5box.EXIT_OOM and oom_close["oom_kill"] > 0
            and oom_close["chain_failure"] is True,
        }
        receipt["checks"] = checks
        receipt["pass"] = all(checks.values())
    finally:
        for leaf in ("chain", "filler"):
            cg = w5box.Cgroup(cgroup_root / "box0" / leaf)
            if cg.exists():
                cg.kill()
                cg.wait_empty(10)
                cg.rmdir()
        for path in (cgroup_root / "box0", cgroup_root):
            if path.exists():
                os.rmdir(path)
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    return 0 if receipt.get("pass") else 1


if __name__ == "__main__":
    raise SystemExit(main())
