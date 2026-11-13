"""Bounded #474 replay comparison, inside the existing Windows timing guard.

prepare and analyze are offline. execute never acquires or bypasses a reservation.
The two qualification cases run unchanged with one and two workers; compare
reuses the faster qualified batch and runs only the remaining two cases.
"""
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

BASE = "384c8a4e5103f40384928adb69720268e4ecd8cb"
HEAD = "17856aa9b6e6c116440b64acab0dcbfe4306e03a"
ROOT = Path(__file__).resolve().parents[2]
PREFIX = "drake_hatcher_v1::prefix_memo_tests::slow_drake_staging_"
SCHEMA = "drake-history-timing/v2"
GIB = 1024 ** 3
HOSTS = {"desktop": "DESKTOP-DJ1C40R", "computehost": "HALEYSPC"}
CASES = {
    "train_241": ("train", 241, ["FDN_top_15301_BG", "FDN_top_01766_UR"], "P1", 1344),
    "train_436": ("train", 436, ["FDN_top_07760_UBpG", "FDN_top_02819_WU"], "P1", 1575),
    "train_597": ("train", 597, ["FDN_top_04406_BR", "FDN_top_01766_UR"], "P1", 1657),
    "eval_458": ("eval", 458, ["FDN_top_04752_WG", "FDN_top_03718_UR"], "P0", 1322),
}
BINARIES = {
    "old": (BASE, "D:/mtg-kernel-actor454-target/release/deps/mtg_kernel-043b4aa14fe1562f.exe",
            "9dd77e1e512809cd9401faa39b4ac085272b5f4cb24beedfbb3864d3d4c7f9e3",
            "D:/mtg-kernel-actor454-validation-codex"),
    "new": (HEAD, "D:/mtg-kernel-history474-target/release/deps/mtg_kernel-043b4aa14fe1562f.exe",
            "948d5f4684deb050e6152da4202903c9f167d3781abc2a5bceaad5eac6207ad5",
            "D:/mtg-kernel-history474-codex"),
}


def require(value, message):
    if not value:
        raise ValueError(message)


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def write(path, value):
    path = Path(path)
    stage = path.with_name(path.name + ".tmp")
    with stage.open("x", encoding="utf-8", newline="\n") as output:
        json.dump(value, output, sort_keys=True, indent=2, allow_nan=False)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.replace(stage, path)


def pin(path):
    path = Path(path).resolve(strict=True)
    with path.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    return {"path": str(path), "sha256": digest}


def checked(reference):
    require(pin(reference["path"]) == reference, "pinned file changed: " + reference["path"])
    return Path(reference["path"])


def plan_reference(path, digest):
    # CLI callers may use forward slashes; receipts use pin()'s canonical path.
    return {"path": str(Path(path).resolve(strict=True)), "sha256": digest}


def positive(value):
    return type(value) in (int, float) and math.isfinite(value) and value > 0


def inventory(reference, placement="desktop", bundle=None):
    value = reference_json(reference, bundle)
    require(set(value) == {"desktop", "computehost", "runpod"}, "inspect all three placements")
    for name, item in value.items():
        require(type(item["eligible"]) is bool and item["reason"].strip(), "missing placement decision")
        require(0 <= time.time() - item["checked_epoch"] <= 86400, "stale availability")
        reference_bytes(item["evidence"], bundle)
    require(placement in HOSTS and value[placement]["eligible"], "selected host is not eligible")
    return value


def validate_placement(cores, reserved_cores):
    require(len(cores) == 2 and len(set(cores)) == 2 and all(type(c) is int and 0 <= c < 64 for c in cores),
            "choose two distinct eligible worker CPUs")
    require(len(reserved_cores) == len(set(reserved_cores))
            and all(type(c) is int and 0 <= c < 64 for c in reserved_cores)
            and set(cores) <= set(reserved_cores), "reserve every worker CPU and its SMT sibling")


def prepare(path, cores, reserved_cores, availability, placement="desktop", remote_root=None):
    require(not path.exists(), "plan already exists")
    validate_placement(cores, reserved_cores)
    availability = pin(availability)
    inventory(availability, placement)
    require(platform.node().casefold() == HOSTS[placement].casefold(), "placement host differs")
    require((remote_root is not None) == (placement == "computehost"), "remote custody root required only on Haley")
    binaries, datasets = {}, []
    for label, (source, binary, digest, checkout) in BINARIES.items():
        executable = pin(binary)
        require(executable["sha256"] == digest, "retained binary identity differs")
        # SAMPLE uses CARGO_MANIFEST_DIR, so changing cwd cannot redirect it.
        sample = Path(checkout) / "data/limited/draftzero_sample_v1"
        files = [sample / "pool.tsv", *sorted((sample / "decks").glob("*.json"))]
        require(len(files) > 4, "missing runtime deck pool")
        inputs = {str(file.relative_to(sample)): pin(file) for file in files}
        datasets.append({name: ref["sha256"] for name, ref in inputs.items()})
        binaries[label] = {"source": source, "binary": executable, "sample": str(sample), "inputs": inputs}
    require(datasets[0] == datasets[1], "old/new runtime inputs differ")
    plan = {"schema": SCHEMA, "host": platform.node(), "placement": placement, "remote_root": remote_root,
            "cores": cores, "reserved_cores": reserved_cores,
            "python": pin(sys.executable), "availability": availability, "binaries": binaries,
            "tools": [pin(__file__), pin(ROOT / "python/tools/host_slots_v1.py"),
                      pin(ROOT / "python/tools/host_reservation_v1.py"), pin(ROOT / "python/tools/drake_history_remote_v1.py")],
            "cases": list(CASES), "qualification_cases": list(CASES)[:2],
            "job_seconds": 1800, "batch_seconds": 7200, "output_cap_bytes": GIB // 8,
            "free_floor_bytes": 60 * GIB, "priority": "below_normal",
            "analysis": "all windows; normalized partial windows marked, no automatic flat-cost verdict"}
    write(path, plan)
    return pin(path)


def load_plan(reference):
    plan = read(checked(reference))
    require(plan["schema"] == SCHEMA and plan["host"] == platform.node(), "different schema or host")
    require(plan["cases"] == list(CASES) and plan["qualification_cases"] == list(CASES)[:2], "changed cases")
    require(plan["job_seconds"] == 1800 and plan["batch_seconds"] == 7200
            and plan["output_cap_bytes"] == GIB // 8 and plan["free_floor_bytes"] == 60 * GIB
            and plan["priority"] == "below_normal", "changed resource bounds")
    validate_placement(plan["cores"], plan["reserved_cores"])
    require(plan["placement"] in HOSTS and plan["host"].casefold() == HOSTS[plan["placement"]].casefold(), "placement differs")
    require(checked(plan["python"]) == Path(sys.executable).resolve(), "different Python")
    require(plan["tools"] == [pin(__file__), pin(ROOT / "python/tools/host_slots_v1.py"),
                              pin(ROOT / "python/tools/host_reservation_v1.py"), pin(ROOT / "python/tools/drake_history_remote_v1.py")], "changed runner or guards")
    for label, (source, binary, digest, checkout) in BINARIES.items():
        item = plan["binaries"][label]
        require(item["source"] == source and item["binary"] == {"path": str(Path(binary).resolve()), "sha256": digest},
                "changed fixed binary")
        checked(item["binary"])
        sample = Path(checkout) / "data/limited/draftzero_sample_v1"
        expected = {str(p.relative_to(sample)): pin(p) for p in [sample / "pool.tsv", *sorted((sample / "decks").glob("*.json"))]}
        require(item["inputs"] == expected, "runtime input pool changed")
    return plan


def reports(text, case):
    summaries = re.findall(r"^test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M)
    require(summaries == [("1", "0", "0")], "expected exactly one successful nonignored test")
    require(re.search(r"^test " + re.escape(PREFIX + case) + r" \.\.\. ", text, re.M), "wrong test")
    # --nocapture prints the first report after libtest's unfinished test-name
    # line; the final 'ok' may consequently occupy a line of its own.
    marker = "drake_staging_timing_v1 "
    rows = [json.loads(line.split(marker, 1)[1]) for line in text.splitlines() if marker in line]
    require(len(rows) == 2 and {r["staging"] for r in rows} == {"borrowed", "owned"}, "expected both staging receipts")
    split, pair, decks, winner, decisions = CASES[case]
    for row in rows:
        require(set(row) == {"schema", "split", "pair", "decks", "winner", "decisions", "seed", "leg",
                             "staging", "final_state_sha256", "replay_executions", "windows"}, "unexpected report fields")
        require((row["schema"], row["split"], row["pair"], row["decks"], row["winner"], row["decisions"], row["seed"], row["leg"])
                == ("drake-staging-timing/v1", split, pair, decks, f"PlayerId({winner[1]})", decisions, 1, 0), "receipt configuration differs")
        require(re.fullmatch("[a-f0-9]{64}", row["final_state_sha256"]) and row["replay_executions"] > 0, "missing state/replay proof")
        expected_first, history, drake = 1, 0, 0
        for window in row["windows"]:
            require(set(window) == {"first_decision", "last_decision", "elapsed_ns", "history_len", "drake_rows"}
                    and all(type(n) is int for n in window.values()), "unexpected window shape")
            last = min(expected_first + 99, decisions)
            require(window["first_decision"] == expected_first and window["last_decision"] == last,
                    "missing, overlapping or oversized decision window")
            require(type(window["elapsed_ns"]) is int and window["elapsed_ns"] > 0, "invalid elapsed time")
            require(type(window["history_len"]) is int and window["history_len"] >= history
                    and type(window["drake_rows"]) is int and window["drake_rows"] >= drake,
                    "invalid history counters")
            history, drake = window["history_len"], window["drake_rows"]
            expected_first = last + 1
        require(expected_first == decisions + 1, "incomplete timing windows")
    require(rows[0]["final_state_sha256"] == rows[1]["final_state_sha256"], "staging paths changed final bytes")
    return rows


def guarded(plan):
    import host_reservation_v1 as host
    import host_slots_v1 as slots
    require(os.name == "nt", "this pinned executable plan requires Windows")
    token = os.environ.get(host.TOKEN_ENV)
    require(token, "use the existing host supervisor and host_slots timed wrapper")
    held = host.status(token)
    require(held.get("token_fate") == "holds" and held["state"] == "held"
            and os.getpid() in host.exempt_pids(token), "not a supervised reservation descendant")
    timed = slots.timed_state()
    require(timed["held"] and timed.get("token_sha256") == slots.token_hash(token)
            and timed.get("why") == "timed work declared its cores"
            and set(timed["cores"] or []) == set(plan["reserved_cores"])
            and set(slots.parse_cores(os.environ.get(slots.CORES_ENV, ""))) == set(plan["reserved_cores"]),
            "need the exact two-core timed declaration")
    require(not (Path(plan["binaries"]["new"]["sample"]).parents[2] / "STOP").exists(), "STOP exists")
    if plan["remote_root"] is not None:
        import drake_history_remote_v1 as remote
        root = remote.owned_root(plan["remote_root"])
        remote.mapped_identity(root)
        remote.waiter_priority(root)
    return slots


def storage(plan, output):
    require(shutil.disk_usage(output).free >= plan["free_floor_bytes"], "free-space floor reached")
    require(sum(p.stat().st_size for p in output.rglob("*") if p.is_file()) <= plan["output_cap_bytes"], "output cap reached")


def child(plan_ref, output, label, case, core):
    plan = load_plan(plan_ref)
    slots = guarded(plan)
    require(label in BINARIES and case in CASES and core in plan["cores"], "unknown job")
    tree = slots.Tree([core], "below_normal")
    env = dict(os.environ)
    env.pop("MTG_LIMITED_MATCH_TRACE", None)
    name = f"{case}-{label}"
    log = output / (name + ".log")
    argv = [plan["binaries"][label]["binary"]["path"], PREFIX + case,
            "--exact", "--ignored", "--nocapture", "--test-threads=1"]
    started = time.monotonic()
    result = {"label": label, "case": case, "core": core, "argv": argv, "started_epoch": time.time()}
    try:
        with log.open("xb") as stream:
            process = subprocess.Popen(argv, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
            tree.child = process
            while process.poll() is None:
                require(time.monotonic() - started < plan["job_seconds"], "job deadline")
                storage(plan, output)
                guarded(plan)
                time.sleep(5)
            result["exit"] = process.returncode
        require(result["exit"] == 0, "native test failed")
        result["reports"] = reports(log.read_text(encoding="utf-8"), case)
        load_plan(plan_ref)  # Detect changes to binaries or runtime inputs during execution.
    finally:
        drained = tree.kill_members(timeout=10)
        if tree.child is not None:
            tree.child.wait(timeout=10)
        result.update(children_drained=drained, seconds=time.monotonic() - started,
                      finished_epoch=time.time(), log=pin(log) if log.exists() else None)
        write(output / (name + ".json"), result)
        require(drained, "child cleanup incomplete")


def batch(plan_ref, root, name, cases, workers):
    plan = load_plan(plan_ref)
    slots = guarded(plan)
    output = root / name
    output.mkdir()
    # One case per stable lane. The serial and parallel modes use the SAME
    # physical core for each case, and old/new always share that core.
    lanes = [[(label, case) for label in ("old", "new")] for case in cases]
    tree = slots.Tree(plan["reserved_cores"], "below_normal")
    running, finished = {}, []
    started = time.monotonic()
    result = {"plan": plan_ref, "workers": workers, "cases": cases, "jobs": [], "complete": False}
    error = None
    last_guard = 0.0
    try:
        while any(lanes) or running:
            if time.monotonic() - last_guard >= 5:
                guarded(plan)
                storage(plan, root)
                last_guard = time.monotonic()
            require(time.monotonic() - started < plan["batch_seconds"], "batch deadline")
            for lane in range(len(lanes)):
                if lane in running or not lanes[lane] or len(running) >= workers:
                    continue
                label, case = lanes[lane].pop(0)
                command = [sys.executable, "-B", str(Path(__file__).resolve()), "_child",
                           "--plan", plan_ref["path"], "--sha256", plan_ref["sha256"], "--output", str(output),
                           "--label", label, "--case", case, "--core", str(plan["cores"][lane])]
                stream = (output / f"{case}-{label}-worker.log").open("xb")
                process = subprocess.Popen(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
                running[lane] = (process, stream, output / f"{case}-{label}.json")
            for lane, (process, stream, receipt) in list(running.items()):
                if process.poll() is not None:
                    stream.close()
                    require(process.returncode == 0, "worker failed: " + str(receipt))
                    finished.append(pin(receipt))
                    del running[lane]
            time.sleep(0.25)
        result.update(complete=True, jobs=sorted(finished, key=lambda item: item["path"]))
    except BaseException as exc:
        error = str(exc)
        raise
    finally:
        drained = tree.kill_members(timeout=10)
        for process, stream, _ in running.values():
            process.wait(timeout=10)
            stream.close()
        result.update(children_drained=drained, seconds=time.monotonic() - started,
                      error=error, jobs=sorted(finished, key=lambda item: item["path"]))
        write(output / "batch.json", result)
        require(drained, "batch cleanup incomplete")
    validate_batch(plan_ref, pin(output / "batch.json"), cases, workers)
    return pin(output / "batch.json")


def reference_bytes(reference, bundle=None):
    if bundle is None:
        return checked(reference).read_bytes()
    data = bundle["files"][reference["path"]].encode("utf-8")
    require(hashlib.sha256(data).hexdigest() == reference["sha256"], "bundled reference hash differs")
    return data


def reference_json(reference, bundle=None):
    return json.loads(reference_bytes(reference, bundle))


def validate_batch(plan_ref, reference, cases, workers, bundle=None):
    plan = reference_json(plan_ref, bundle)
    value = reference_json(reference, bundle)
    require(value["plan"] == plan_ref and value["cases"] == cases and value["workers"] == workers
            and value["complete"] and value["children_drained"] and positive(value["seconds"]), "incomplete/incompatible batch")
    actual = {}
    for reference in value["jobs"]:
        item = reference_json(reference, bundle)
        key = item["case"], item["label"]
        require(key not in actual and item["exit"] == 0 and item["children_drained"], "invalid/duplicate job")
        require(key[0] in cases and key[1] in BINARIES, "unexpected job")
        require(item["core"] == plan["cores"][cases.index(key[0])]
                and item["argv"] == [plan["binaries"][key[1]]["binary"]["path"], PREFIX + key[0],
                                     "--exact", "--ignored", "--nocapture", "--test-threads=1"], "changed placement or command")
        require(positive(item["seconds"]) and item["seconds"] <= plan["job_seconds"] + 30
                and positive(item["finished_epoch"] - item["started_epoch"]), "invalid job timing")
        rows = reports(reference_bytes(item["log"], bundle).decode("utf-8"), item["case"])
        require(item["reports"] == rows, "receipt differs from native log")
        actual[key] = item
    require(set(actual) == {(case, label) for case in cases for label in BINARIES}, "missing jobs")
    for case in cases:
        require(actual[case, "old"]["reports"][0]["final_state_sha256"]
                == actual[case, "new"]["reports"][0]["final_state_sha256"], "old/new final receipts differ")
    overlap = any(min(a["finished_epoch"], b["finished_epoch"]) > max(a["started_epoch"], b["started_epoch"])
                  for a in actual.values() for b in actual.values() if a["core"] != b["core"])
    require(overlap == (workers == 2), "declared worker count does not match actual overlap")
    return value, actual


def qualification(plan_ref, reference, bundle=None):
    value = reference_json(reference, bundle)
    require(value["plan"] == plan_ref, "qualification for another plan")
    batches = [validate_batch(plan_ref, value[str(w)], list(CASES)[:2], w, bundle) for w in (1, 2)]
    for key in batches[0][1]:
        require(batches[0][1][key]["reports"][0]["final_state_sha256"]
                == batches[1][1][key]["reports"][0]["final_state_sha256"], "parallel execution changed outcomes")
    selected = min((1, 2), key=lambda w: batches[w - 1][0]["seconds"])
    require(value["selected_workers"] == selected, "allocation is not the measured faster choice")
    return selected, value[str(selected)]


def portable_inputs(plan):
    return {"schema": plan["schema"], "cases": plan["cases"], "qualification_cases": plan["qualification_cases"],
            "tools": [p["sha256"] for p in plan["tools"]],
            "binaries": {label: {"source": b["source"], "sha256": b["binary"]["sha256"],
                                  "inputs": {k: v["sha256"] for k, v in b["inputs"].items()}}
                         for label, b in plan["binaries"].items()}}


def observed_overhead(overhead_ref):
    overhead = reference_json(overhead_ref)
    require(set(overhead) == {"setup_seconds", "transfer_seconds", "wait_seconds", "evidence"}
            and all(type(overhead[k]) in (int, float) and math.isfinite(overhead[k]) and overhead[k] >= 0
                    for k in ("setup_seconds", "transfer_seconds", "wait_seconds")), "invalid observed overhead")
    reference_bytes(overhead["evidence"])
    return overhead


def bundle_qualification(plan_ref, qual_ref, overhead_ref, output):
    load_plan(plan_ref)
    qualification(plan_ref, qual_ref)
    overhead = observed_overhead(overhead_ref)
    refs = [plan_ref, qual_ref, overhead_ref, overhead["evidence"]]
    q = reference_json(qual_ref)
    for w in (1, 2):
        refs.append(q[str(w)])
        for job in reference_json(q[str(w)])["jobs"]:
            refs.extend([job, reference_json(job)["log"]])
    value = {"schema": SCHEMA, "plan": plan_ref, "qualification": qual_ref, "overhead": overhead_ref,
             "files": {ref["path"]: reference_bytes(ref).decode("utf-8") for ref in refs}}
    qualification(plan_ref, qual_ref, value)
    write(output, value)


def select_placement(availability_ref, bundle_refs, forecast_ref, output=None, proofs=None):
    available = reference_json(availability_ref, proofs)
    forecasts = reference_json(forecast_ref, proofs)
    refs = [availability_ref, forecast_ref, *bundle_refs,
            *[v["evidence"] for v in available.values()], *[v["evidence"] for v in forecasts.values()]]
    candidates, common, outcomes = {}, None, None
    for reference in bundle_refs:
        bundle = reference_json(reference, proofs)
        plan = reference_json(bundle["plan"], bundle)
        placement = plan["placement"]
        require(placement not in candidates and plan["schema"] == SCHEMA
                and plan["host"].casefold() == HOSTS[placement].casefold(), "duplicate/different qualification host")
        inputs = portable_inputs(plan)
        require(common is None or common == inputs, "placements did not run identical native inputs/helpers")
        common = inputs
        workers, selected = qualification(bundle["plan"], bundle["qualification"], bundle)
        batch_value, jobs = validate_batch(bundle["plan"], selected, list(CASES)[:2], workers, bundle)
        states = {case: jobs[case, "old"]["reports"][0]["final_state_sha256"] for case in list(CASES)[:2]}
        require(outcomes is None or outcomes == states, "host placement changed terminal outcomes")
        outcomes = states
        forecast = forecasts[placement]
        require(set(forecast) == {"remaining_setup_seconds", "remaining_transfer_seconds", "remaining_wait_seconds", "evidence"}
                and all(type(v) in (int, float) and math.isfinite(v) and v >= 0 for k, v in forecast.items() if k != "evidence"),
                "invalid remaining overhead forecast")
        reference_bytes(forecast["evidence"], proofs)
        observed = reference_json(bundle["overhead"], bundle)
        reference_bytes(observed["evidence"], bundle)
        # Decision-weighted forecast of the two remaining fixed cases. Past
        # setup/qualification is reported, not charged again as future work.
        ratio = sum(CASES[c][4] for c in list(CASES)[2:]) / sum(CASES[c][4] for c in list(CASES)[:2])
        remaining = batch_value["seconds"] * ratio + sum(v for k, v in forecast.items() if k != "evidence")
        candidates[placement] = {"bundle": reference, "plan": bundle["plan"], "workers": workers,
                                 "observed_overhead": observed, "remaining_forecast": forecast,
                                 "projected_remaining_seconds": remaining}
    expected = {key for key, value in available.items() if value["eligible"]}
    require(set(candidates) == expected and expected <= set(HOSTS), "every eligible placement needs completed qualification")
    for name in expected:
        inventory(availability_ref, name, proofs)
    winner = min(candidates, key=lambda key: (candidates[key]["projected_remaining_seconds"], key))
    result = {"schema": SCHEMA, "availability": availability_ref, "forecast": forecast_ref,
              "candidates": candidates, "selected": winner, "basis": "measured completed batch and declared remaining overhead",
              "proofs": {"files": {r["path"]: reference_bytes(r, proofs).decode("utf-8") for r in refs}}}
    if output is not None:
        write(output, result)
    return result


def require_selection(plan_ref, selection_ref, qualification_ref):
    selected = reference_json(selection_ref)
    # Recompute from retained primary receipts; do not trust a chosen host label.
    rebuilt = select_placement(selected["availability"], [v["bundle"] for v in selected["candidates"].values()],
                               selected["forecast"], proofs=selected["proofs"])
    require(rebuilt == selected, "selection differs from primary receipts")
    chosen = selected["candidates"][selected["selected"]]
    require(chosen["plan"] == plan_ref, "this host/plan was not the fastest qualified placement")
    bundle = reference_json(chosen["bundle"], selected["proofs"])
    require(bundle["qualification"] == qualification_ref, "comparison would reuse a different qualification batch")


def normalized_window(case, label, staging, window):
    count = window["last_decision"] - window["first_decision"] + 1
    return {"case": case, "binary": label, "staging": staging, **window,
            "decisions": count, "partial": count != 100,
            "seconds_per_100": window["elapsed_ns"] / 1e9 * 100 / count}


def analyze(plan_ref, references, output):
    jobs = {}
    for reference, cases, workers in references:
        _, actual = validate_batch(plan_ref, reference, cases, workers)
        require(not jobs.keys() & actual.keys(), "duplicate comparison cases")
        jobs.update(actual)
    require(set(jobs) == {(case, label) for case in CASES for label in BINARIES}, "need all four cases")
    windows, comparisons = [], []
    for case in CASES:
        for label in BINARIES:
            for report in jobs[case, label]["reports"]:
                for window in report["windows"]:
                    windows.append(normalized_window(case, label, report["staging"], window))
        before = {(r["staging"], r["first_decision"]): r for r in windows if r["case"] == case and r["binary"] == "old"}
        for after in [r for r in windows if r["case"] == case and r["binary"] == "new"]:
            old = before[after["staging"], after["first_decision"]]
            comparisons.append({"case": case, "staging": after["staging"], "first_decision": after["first_decision"],
                                "last_decision": after["last_decision"], "partial": after["partial"],
                                "old_seconds_per_100": old["seconds_per_100"], "new_seconds_per_100": after["seconds_per_100"],
                                "new_over_old": after["seconds_per_100"] / old["seconds_per_100"]})
    for name, rows in (("windows", windows), ("paired_windows", comparisons)):
        write(output / (name + ".json"), rows)
        with (output / (name + ".csv")).open("x", newline="", encoding="utf-8") as stream:
            writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
            writer.writeheader()
            writer.writerows(rows)
    write(output / "comparison.json", {"schema": SCHEMA, "plan": plan_ref, "batches": references,
          "all_outcomes_and_final_state_receipts_equal": True, "flat_cost_verdict": "requires inspecting all timing windows",
          "windows": windows, "paired_windows": comparisons})


def execute(plan_ref, output, stage, qualification_ref=None, selection_ref=None, overhead_ref=None):
    plan = load_plan(plan_ref)
    guarded(plan)
    inventory(plan["availability"], plan["placement"])
    if stage == "compare":
        require(selection_ref is not None, "comparison requires host selection")
        require_selection(plan_ref, selection_ref, qualification_ref)
    else:
        require(overhead_ref is not None, "qualification requires pinned observed overhead")
        observed_overhead(overhead_ref)  # Refuse missing evidence before games.
    require(not output.exists(), "fresh output directory required")
    output.mkdir(parents=True)
    storage(plan, output)
    if stage == "qualify":
        refs = {str(w): batch(plan_ref, output, f"workers-{w}", list(CASES)[:2], w) for w in (1, 2)}
        choice = min((1, 2), key=lambda w: read(refs[str(w)]["path"])["seconds"])
        write(output / "qualification.json", {"schema": SCHEMA, "plan": plan_ref, **refs, "selected_workers": choice})
        qualification(plan_ref, pin(output / "qualification.json"))
        # Export before the remote wrapper drains descendants and unmaps D:.
        bundle_qualification(plan_ref, pin(output / "qualification.json"), overhead_ref, output / "bundle.json")
    else:
        require(qualification_ref is not None, "comparison requires throughput evidence")
        workers, reused = qualification(plan_ref, qualification_ref)
        remaining = batch(plan_ref, output, "remaining", list(CASES)[2:], workers)
        analyze(plan_ref, [(reused, list(CASES)[:2], workers), (remaining, list(CASES)[2:], workers)], output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    prep = sub.add_parser("prepare")
    prep.add_argument("--output", type=Path, required=True)
    prep.add_argument("--cores", required=True, help="two worker logical CPUs, comma-separated")
    prep.add_argument("--reserved-cores", required=True, help="worker CPUs and their SMT siblings, comma-separated")
    prep.add_argument("--availability", type=Path, required=True)
    prep.add_argument("--placement", choices=tuple(HOSTS), default="desktop")
    prep.add_argument("--remote-root")
    bundle = sub.add_parser("bundle")
    bundle.add_argument("--plan", required=True)
    bundle.add_argument("--sha256", required=True)
    bundle.add_argument("--qualification", type=Path, required=True)
    bundle.add_argument("--overhead", type=Path, required=True)
    bundle.add_argument("--output", type=Path, required=True)
    select = sub.add_parser("select")
    select.add_argument("--availability", type=Path, required=True)
    select.add_argument("--bundle", type=Path, action="append", required=True)
    select.add_argument("--forecast", type=Path, required=True)
    select.add_argument("--output", type=Path, required=True)
    for mode in ("execute", "analyze", "_child"):
        p = sub.add_parser(mode)
        p.add_argument("--plan", required=True)
        p.add_argument("--sha256", required=True)
        p.add_argument("--output", type=Path, required=True)
        if mode in ("execute", "analyze"):
            if mode == "execute":
                p.add_argument("--stage", choices=("qualify", "compare"), required=True)
                p.add_argument("--selection", type=Path)
                p.add_argument("--overhead", type=Path)
                p.add_argument("--overhead-sha")
            else:
                p.add_argument("--remaining", type=Path, required=True)
            p.add_argument("--qualification", type=Path, required=mode == "analyze")
        else:
            p.add_argument("--label", required=True)
            p.add_argument("--case", required=True)
            p.add_argument("--core", type=int, required=True)
    args = parser.parse_args()
    if args.mode == "prepare":
        print(json.dumps(prepare(args.output.resolve(), [int(x) for x in args.cores.split(",")],
                                 [int(x) for x in args.reserved_cores.split(",")], args.availability,
                                 args.placement, args.remote_root)))
    elif args.mode == "bundle":
        bundle_qualification(plan_reference(args.plan, args.sha256), pin(args.qualification), pin(args.overhead), args.output)
    elif args.mode == "select":
        select_placement(pin(args.availability), [pin(p) for p in args.bundle], pin(args.forecast), args.output)
    elif args.mode == "_child":
        child(plan_reference(args.plan, args.sha256), args.output.resolve(), args.label, args.case, args.core)
    elif args.mode == "analyze":
        reference = plan_reference(args.plan, args.sha256)
        load_plan(reference)
        workers, reused = qualification(reference, pin(args.qualification))
        args.output.mkdir()
        analyze(reference, [(reused, list(CASES)[:2], workers),
                            (pin(args.remaining), list(CASES)[2:], workers)], args.output)
    else:
        require((args.overhead is None) == (args.overhead_sha is None), "pass overhead path and digest together")
        execute(plan_reference(args.plan, args.sha256), args.output.resolve(), args.stage,
                pin(args.qualification) if args.qualification else None, pin(args.selection) if args.selection else None,
                plan_reference(args.overhead, args.overhead_sha) if args.overhead else None)


if __name__ == "__main__":
    main()
