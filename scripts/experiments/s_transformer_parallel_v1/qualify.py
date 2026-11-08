"""Bounded S learner comparison through the supported native CPU dispatcher.

The plan pins the same real CS prefix and separate baseline/candidate runtimes.
Each runtime first completes one update at 1/2/4 workers. The supported
throughput gate selects its fastest measured allocation before a matched ABBA
comparison of the complete prefix. All outputs and failed attempts are kept.
This is engineering evidence on the named host, not a scientific training run.
"""
from __future__ import annotations

import argparse
import contextlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import traceback

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "python" / "tools"))
import native_expanded_dispatch_v1 as dispatch
import host_reservation_v1 as reservations


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, indent=2, allow_nan=False)
        stream.write("\n")
    return dispatch.pin(path)


def run(plan_path):
    plan = dispatch.read(plan_path)
    root, cold = Path(plan["root"]), Path(plan["cold_root"])
    host = plan.get("host", "desktop")
    if host not in ("desktop", "computehost"):
        raise ValueError("this bounded comparison supports the two Windows PCs")
    host_name = plan.get("compute_host_name")
    if host == "computehost":
        if not isinstance(host_name, str) or not host_name:
            raise ValueError("compute host name required")
        dispatch.HOSTS["computehost"] = host_name.upper()
    config = dispatch.read(dispatch.checked(plan["config"]))
    if not 2 <= len(config["iterations"]) <= 32:
        raise ValueError("engineering comparison needs 2..32 complete updates")
    token = os.environ.get("MTG_HOST_RESERVATION_TOKEN")
    if not token or reservations.status(token).get("token_fate") != "holds":
        raise ValueError("canonical reservation required")
    runtime_pins = plan["runtimes"]
    if set(runtime_pins) != {"baseline", "candidate"}:
        raise ValueError("exactly baseline and candidate runtimes required")
    inventory = dispatch.read(dispatch.checked(plan["inventory"]))
    affinity = plan["cpu_affinity"]
    if len(affinity) < 4:
        raise ValueError("the fixed 1/2/4 comparison needs at least four logical CPUs")
    worker_counts = plan.get("worker_counts", [1, 2, 4])
    if (not isinstance(worker_counts, list) or 1 not in worker_counts
            or len(set(worker_counts)) != len(worker_counts)
            or any(type(w) is not int or not 1 <= w <= min(32, len(affinity)) for w in worker_counts)
            or not any(w > 1 for w in worker_counts)):
        raise ValueError("unique serial and parallel worker counts within the CPU allocation required")
    worker_settings = [(w, w) for w in sorted(worker_counts)]
    # The live CS layout uses four collectors and two preparation workers.
    # Price it explicitly because the best learner and collector counts can differ.
    if plan.get("include_production_workers", False) and (4, 2) not in worker_settings:
        worker_settings.append((4, 2))
    trials, choices, settings, observations = {}, {}, {}, {}

    def execute(path, qualification):
        # Each supported attempt owns fresh monitor/signal state and affinity.
        command = [
            sys.executable, "-B", str(Path(dispatch.__file__).resolve()),
            "_qualify" if qualification else "_dispatch", str(path.resolve()),
        ]
        if host_name:
            command.extend(["--compute-host-name", host_name])
        subprocess.run(command, check=True, stdout=sys.stdout, stderr=sys.stderr)

    def request(label, runtime, workers, prep, choice=None):
        projection = plan.get("projected_additional_bytes", 2 * 1024**3)
        volume_projection = {}
        for directory in (root, cold):
            volume = directory.drive.upper()
            volume_projection[volume] = volume_projection.get(volume, 0) + (projection + 1) // 2
        trial_config = dict(config, output_directory=str(root / label / "native"),
                            collection_workers=workers, preparation_workers=prep)
        config_pin = write(root / "requests" / (label + "-config.json"), trial_config)
        req = {
            "schema": dispatch.SCHEMA, "kind": "training",
            "lane": "codex-s-transformer-parallel", "runtime": runtime,
            "config": config_pin, "root": str(root / label),
            "cold_root": str(cold / label),
            "placement": {"host": host, "cpu_affinity": affinity,
                          "workers": workers, "preparation_workers": prep,
                          "memory_bytes": plan.get("memory_bytes", 12 * 1024**3)},
            "storage": {
                "accounting_roots": [str(root), str(cold)],
                "max_logical_bytes": plan.get("max_logical_bytes", 12 * 1024**3),
                "reserve_bytes": 60 * 1024**3,
                "projected_additional_bytes": projection,
                "projected_volume_bytes": volume_projection},
            "wall_seconds": 600,
        }
        fraction = config.get("max_non_natural_episode_fraction", 0)
        if fraction:
            req["non_natural_tolerance"] = fraction
        if choice is not None:
            req["choice"] = choice
            selected = dispatch.require_choice(choice, req, verify_outputs=True)
            req["choice_verification"] = write(root / "requests" / (label + "-choice.json"), {
                "schema": dispatch.CHOICE, "choice": choice, "selected": selected,
                "outputs_verified": True, "launcher": dispatch.pin(dispatch.__file__),
            })
        path = root / "requests" / (label + ".json")
        write(path, req)
        return path, req

    # No mutation of collection weights or asynchronous batch overlap occurs.
    # The same full initial state and first scheduled batch price all settings.
    for name in ("baseline", "candidate"):
        reports = []
        for workers, prep in worker_settings:
            label = f"qual-{name}-w{workers}-p{prep}"
            path, req = request(label, runtime_pins[name], workers, prep)
            execute(path, True)
            report_path = root / label / "report.json"
            report = dispatch.read(report_path)
            reports.append((dispatch.pin(report_path), report))
        reference = reports[0][1]["fingerprint"]
        if any(report["fingerprint"] != reference for _, report in reports):
            raise ValueError(f"{name}: serial/parallel qualification differs")
        trials[name] = reports
        ranked = [(dispatch.projected_seconds(report, config, "training"), pin, report)
                  for pin, report in reports]
        # The dispatch gate accepts one qualification per collector count.
        # Keep every measured setting and submit each count's fastest preparation setting.
        fastest_by_count = {}
        for row in ranked:
            count = row[2]["placement"]["workers"]
            if count not in fastest_by_count or row[0] < fastest_by_count[count][0]:
                fastest_by_count[count] = row
        canonical = [fastest_by_count[count] for count in sorted(fastest_by_count)]
        _, selected, best = min(canonical, key=lambda row: row[0])
        family = dispatch.workload(config, "training")
        observations[name] = write(root / f"observations-{name}.json", {
            "schema": "s-parallel-qualification-observations/v1", "runtime": runtime_pins[name],
            "workload": family, "fingerprint": reference,
            "settings": [{"report": pin, "placement": report["placement"],
                          "projected_seconds": seconds} for seconds, pin, report in ranked],
            "canonical_reports": [pin for _, pin, _ in canonical],
        })
        choice = {"schema": dispatch.CHOICE, "inventory": inventory,
                  "qualifications": [pin for _, pin, _ in canonical],
                  "selected": {family: selected}}
        choices[name] = write(root / f"choice-{name}.json", choice)
        settings[name] = best["placement"]
    if trials["baseline"][0][1]["fingerprint"] != trials["candidate"][0][1]["fingerprint"]:
        raise ValueError("baseline and candidate first-update scientific outputs differ")

    rows = []
    for ordinal, name in enumerate(("baseline", "candidate", "candidate", "baseline")):
        label = f"abba-{ordinal}-{name}"
        p = settings[name]
        path, req = request(label, runtime_pins[name], p["workers"],
                            p["preparation_workers"], choices[name])
        execute(path, False)
        report = dispatch.read(root / label / "report.json")
        result = dispatch.read(dispatch.checked(report["native_result"]))
        states, state_files, learners = [], [], []
        for item in result["iterations"]:
            receipt = dispatch.read(dispatch.checked(item))
            update = dispatch.read(dispatch.checked(receipt["update"]))
            checkpoint = dispatch.read(dispatch.checked(update["checkpoint"]))
            dispatch.checked(checkpoint["state"])
            states.append(update["after_state_sha256"])
            state_files.append(checkpoint["state"]["sha256"])
            learners.append({key: update[key] for key in (
                "learner_update_seconds", "behavior_replay_seconds", "update_elapsed_seconds",
                "numerical_backend", "s_learner_workers_requested",
            ) if key in update})
        execution = dispatch.read(dispatch.checked(report["execution"]))
        rows.append({"name": name, "report": dispatch.pin(root / label / "report.json"),
                     "native_seconds": execution["child_seconds"],
                     "recovery_inclusive_seconds": report["seconds"],
                     "completed_updates": report["completed_updates"],
                     "completed_games": report["completed_games"],
                     "states": states, "state_files": state_files,
                     "placement": p, "learner_timing": learners})
        write(root / f"{label}-summary.json", rows[-1])
    if any(row["states"] != rows[0]["states"] or row["state_files"] != rows[0]["state_files"]
           for row in rows[1:]):
        raise ValueError("ABBA full parameter/Adam state bytes differ")
    means = {name: statistics.mean(row["native_seconds"] for row in rows if row["name"] == name)
             for name in settings}
    recovery = {name: statistics.mean(row["recovery_inclusive_seconds"]
                                     for row in rows if row["name"] == name)
                for name in settings}
    write(root / "RESULT.json", {
        "schema": "s-parallel-qualification/v1", "complete": True,
        "plan": dispatch.pin(plan_path), "config": plan["config"],
        "runtimes": runtime_pins, "choices": choices, "observations": observations, "trials": rows,
        "native_speedup": means["baseline"] / means["candidate"],
        "recovery_inclusive_speedup": recovery["baseline"] / recovery["candidate"],
        "full_adam_states_bit_identical": True,
        "limitations": ["One fixed CS prefix on the named Windows CPU allocation.",
                        "First-update trajectories compared exactly; later output-root provenance differs.",
                        "No node deployment or playing-strength claim."],
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("launch", "_run"))
    parser.add_argument("plan", type=Path)
    args = parser.parse_args()
    plan = dispatch.read(args.plan)
    root = Path(plan["root"])
    if args.action == "launch":
        if root.exists():
            raise ValueError("fresh comparison root required")
        root.mkdir(parents=True)
        result = reservations.dispatch(
            lane="codex-s-transformer-parallel", work_id=root.name,
            release_condition="bounded comparison complete or failed",
            command=[sys.executable, "-B", str(Path(__file__).resolve()), "_run", str(args.plan.resolve())],
            cwd=str(REPO), busy_pattern=dispatch.busy_pattern(),
        )
        write(root / "launch.json", result)
        print(json.dumps({k: result[k] for k in ("state", "pid") if k in result}))
    else:
        with (root / "driver.log").open("x", encoding="utf-8", buffering=1) as log:
            with contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
                try:
                    run(args.plan.resolve())
                except BaseException:
                    write(root / "FAILURE.json", {"traceback": traceback.format_exc()})
                    raise


if __name__ == "__main__":
    main()
