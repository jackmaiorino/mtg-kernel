"""Stage fresh qualification, allocation checks and sealed ABBA commands only.

Never launches, reserves, retries, copies evidence or prunes files. Prepare
requires a fresh desktop/computehost/runpod inventory and measured transport
JSON, keyed by host, with complete=true and projected_transport_seconds for
each eligible host, or variant then host when overhead differs by variant.
Optional Haley qualification JSON contains cases with
variant, workers, report pin and optional verified path_mappings.
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import hashlib
import importlib
import json
import math
import os
from pathlib import Path
import stat
import sys

GIB = 1024**3
VARIANTS = ("baseline", "candidate")
WORKERS = (1, 2, 4, 8)
CORES = list(range(0, 16, 2))
DEFAULT_ROOT = Path("D:/training-io-speedups-20261010/desktop")
DEFAULT_PYTHON = Path("D:/mtg-kernel-uv-python-019f63a2/cpython-3.13.14-windows-x86_64-none/python.exe")
BASELINE_COMMIT = "b3bd1c1c0d7b5b6766359a3901549de768bcc175"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def pin(path):
    path = Path(path)
    require(path.is_absolute() and path.is_file(), "absolute existing file required: " + str(path))
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "sha256": digest}


def checked(ref):
    require(pin(ref["path"]) == ref, "pinned file changed: " + ref["path"])
    return Path(ref["path"])


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, separators=(",", ":"), allow_nan=False)
        stream.write("\n")
    return pin(path)


def fresh_inventory(ref, max_age=600):
    inventory = read(checked(ref))
    require(set(inventory) == {"desktop", "computehost", "runpod"}, "fresh three-host inventory required")
    now = datetime.now(timezone.utc)
    for host, row in inventory.items():
        stamp = row.get("observed_utc", inventory["desktop"]["observed_utc"])
        age = (now - datetime.fromisoformat(stamp.replace("Z", "+00:00"))).total_seconds()
        require(0 <= age <= max_age, "stale inventory: " + host)
    return inventory


def volume_free(row, device):
    volumes = [v for v in row["volumes"] if v["DeviceID"].upper() == device.upper()]
    require(len(volumes) == 1, "missing unique volume inventory: " + device)
    free = int(volumes[0]["FreeSpace"])
    require(free >= 0, "invalid free-space inventory")
    return free


def native_inputs(source):
    refs = {}

    def add(ref):
        checked(ref)
        previous = refs.setdefault(ref["path"], ref)
        require(previous == ref, "conflicting native input pins")

    for model in [source["initial_source"], *(item["source"] for item in source["opponents"])]:
        for name in ("play_import", "checkpoint"):
            ref = model.get(name)
            if ref is None:
                continue
            add(ref)
            if name == "play_import":
                descriptor = read(ref["path"])
                for nested in ("initialization", "parameters"):
                    if nested in descriptor:
                        add(descriptor[nested])
    require(len(refs) == 8, "expected eight actual native source dependencies")
    return [{"original": ref, "bytes": Path(ref["path"]).stat().st_size,
             "usage": "native reads this SHA-verified absolute path"}
            for ref in refs.values()]


def bindings(root, python):
    tools, runtimes = {}, {}
    for variant in VARIANTS:
        folder = root / variant / "python/tools"
        tools[variant] = [pin(path) for path in sorted(folder.rglob("*.py"))]
        require(any(Path(ref["path"]).name == "native_expanded_dispatch_v1.py"
                    for ref in tools[variant]), "missing frozen dispatcher: " + variant)
        ref = pin(root / "runtimes" / (variant + ".json"))
        runtime = read(ref["path"])
        require(len(runtime["engine_commit"]) == 40 and len(runtime["tracked_tree_sha256"]) == 64,
                "incomplete runtime identity")
        checked(runtime["binary"])
        require("platform" not in runtime, "desktop Windows runtime required")
        if variant == "baseline":
            require(runtime["engine_commit"] == BASELINE_COMMIT, "baseline must include accepted PR200")
        runtimes[variant] = ref
    helpers = {name: pin(root / name) for name in (
        "formal_case_driver.py", "formal_launcher_adapter.py", "observe_native_affinity.py",
        "wait_canonical_free.py", "run_comparison.py", "run_qualifications.py")}
    supporting = [pin(root.parent / name) for name in (
        "helpers/postprocess_case.py", "helpers/copy_recovery.py", "helpers/desktop_devices.py",
        "measure_allocations.py", "resume_qualifications.py")]
    return {"tools": tools, "runtimes": runtimes, "helpers": helpers,
            "supporting_helpers": supporting,
            "controller": helpers["formal_case_driver.py"],
            "adapter": helpers["formal_launcher_adapter.py"],
            "runtime_decks": pin(root / "runtime_decks_v1.json"),
            "python_pin": pin(python), "python": str(python), "helper": pin(Path(__file__).resolve())}


def storage_preflight(dispatch, root, qualification):
    physical = (1 if qualification else 16) * GIB
    logical = (1 if qualification else 32) * GIB
    storage = {"accounting_roots": [str(root)], "max_logical_bytes": 160 * GIB,
               "reserve_bytes": 60 * GIB, "projected_additional_bytes": physical,
               "projected_volume_bytes": {root.drive.upper(): physical}}
    for name in ("hot", "cold"):
        parent = root / "measure" / name
        require(parent.is_dir(), "create measurement parent before staging: " + str(parent))
        if os.name == "nt":
            require(parent.stat().st_file_attributes & stat.FILE_ATTRIBUTE_COMPRESSED,
                    "measurement parent must inherit NTFS compression: " + str(parent))
    observed = dispatch.validate_storage(storage)
    require(observed["logical_bytes"] + logical <= storage["max_logical_bytes"], "logical projection exceeds allowance")
    return storage, logical


def make_request(dispatch, root, source, runtime, variant, label, workers, storage, logical, qualification):
    hot, cold = root / "measure/hot" / label, root / "measure/cold" / label
    require(not hot.exists() and not cold.exists(), "preserve existing case output: " + label)
    config = copy.deepcopy(source)
    config.update(output_directory=str(hot / "native" if qualification else root / "measure/hot/matched-native"),
                  collection_workers=workers, preparation_workers=workers)
    require(dispatch.workload(config, "training") == dispatch.workload(source, "training"), "placement changed workload")
    directory = root / ("requests" if qualification else "formal")
    result = {"schema": dispatch.SCHEMA, "kind": "training", "runtime": runtime,
              "config": write(directory / (label + ".config.json"), config),
              "root": str(hot), "cold_root": str(cold), "lane": "codex-training-io-speedups-20261010",
              "placement": {"host": "desktop", "workers": workers, "preparation_workers": workers,
                            "cpu_affinity": CORES, "memory_bytes": 32 * GIB},
              "wall_seconds": 3600, "storage": storage, "comparison_logical_projection_bytes": logical}
    if source.get("max_non_natural_episode_fraction", 0):
        result["non_natural_tolerance"] = source["max_non_natural_episode_fraction"]
    return result


def driver_argv(root, python, variant, label, request, action):
    return [str(python), "-B", str(root / "formal_case_driver.py"),
            "--launcher-root", str(root / variant), "--request", request["path"], "--action", action,
            "--receipt", str(root / "controllers" / (label + ".json")), "--declare-desktop-cores"]


def eligible_hosts(inventory, desktop_drive="D:"):
    desktop = inventory["desktop"]
    require(not desktop["reservations"], "desktop has a current reservation")
    require(volume_free(desktop, desktop_drive) >= 60 * GIB + 16 * GIB, "desktop projection/reserve unavailable")
    haley = inventory["computehost"]
    eligible = not haley["reservations"] and volume_free(haley, "C:") >= 60 * GIB + 16 * GIB
    reason = ("fresh C headroom admits 16 GiB above 60 GiB reserve" if eligible else
              "Haley has a current reservation" if haley["reservations"] else
              "fresh C headroom below 16 GiB projection above 60 GiB reserve")
    return {"desktop": (True, "free qualified eight desktop P cores; CI CPUs 18-23 preserved"),
            "computehost": (eligible, reason),
            "runpod": (False, "fresh RunPod inventory recorded; this CPU comparison has no paid execution authority")}


def qualification_entries(dispatch, root, variant, runtime, family, canonical, eligible, haley):
    entries = [pin(root / f"measure/hot/qual-{variant}-w{w}/report.json") for w in WORKERS]
    if eligible["computehost"][0]:
        require(haley is not None, "eligible Haley needs fresh runtime-compatible qualification at workers 1,2,4,8")
        cases = [case for case in haley["cases"] if case["variant"] == variant]
        require([case["workers"] for case in cases] == list(WORKERS), "Haley worker coverage differs")
        entries += [{"report": case["report"], "path_mappings": case.get("path_mappings", [])} for case in cases]
    for ordinal, entry in enumerate(entries):
        ref = entry.get("report", entry)
        report = read(checked(ref))
        host = "desktop" if ordinal < len(WORKERS) else "computehost"
        workers = WORKERS[ordinal % len(WORKERS)]
        require(report["schema"] == dispatch.SCHEMA and report["complete"] is True
                and report["qualification"] is True and report["completed_updates"] == 1
                and report["completed_games"] == 10 and report["workload"] == family
                and report["placement"]["host"] == host and report["placement"]["workers"] == workers
                and report["placement"]["preparation_workers"] == workers
                and report["placement"]["cpu_affinity"] == (CORES if host == "desktop" else list(range(16)))
                and report["fingerprint"] == canonical, "qualification coverage, affinity or parity differs")
        require(dispatch.runtime_identity(report["runtime"]) == dispatch.runtime_identity(runtime),
                "fresh qualification runtime incompatible: " + variant)
        if host == "desktop":
            controller = read(checked(pin(root / "controllers" / (f"qual-{variant}-w{workers}.json"))))
            require(controller["complete"] is True and controller["report"] == ref
                    and controller["request"] == report["request"], "qualification controller differs")
            require(controller["controller_source"] == pin(root / "formal_case_driver.py")
                    and controller["launcher_adapter"] == pin(root / "formal_launcher_adapter.py")
                    and controller["launcher"] == pin(root / variant / "python/tools/native_expanded_dispatch_v1.py")
                    and report["launcher_sha256"] == controller["launcher"]["sha256"],
                    "qualification used different frozen helpers")
            affinity = controller["native_affinity_observation"]
            require(affinity["verified"] is True and affinity["actual_mask"] == affinity["expected_mask"] == 21845,
                    "qualification actual native affinity differs")
        # The public check-choice later performs full output/archive readback.
    return entries


def prepare(args, dispatch, root, source, common):
    require(args.inventory is not None and args.transport is not None, "prepare requires --inventory and --transport")
    inventory_ref, transport_ref = pin(args.inventory), pin(args.transport)
    inventory = fresh_inventory(inventory_ref)
    transport = read(transport_ref["path"])
    eligible = eligible_hosts(inventory, root.drive)
    haley = read(args.haley_qualifications) if args.haley_qualifications else None
    canonical = read(root / "measure/hot/qual-baseline-w1/report.json")["fingerprint"]
    choices, scores = {}, {}
    family = common["workload_family"]
    # Complete both selections before writing any formal request.
    for variant in VARIANTS:
        status = {}
        for host, (allowed, reason) in eligible.items():
            row = transport.get(variant, transport).get(host, {})
            seconds = row.get("projected_transport_seconds", 0)
            if allowed:
                require(row.get("complete") is True and type(seconds) in (int, float)
                        and math.isfinite(seconds) and seconds >= 0,
                        "measured complete transport required: " + variant + "/" + host)
            status[host] = {"eligible": allowed, "reason": reason, "evidence": inventory_ref,
                            "checked_at": inventory[host].get("observed_utc", inventory["desktop"]["observed_utc"]),
                            "cpu_affinity": CORES if host == "desktop" else list(range(16)) if host == "computehost" else [],
                            "transport_seconds": seconds if allowed else 0, "transport_evidence": transport_ref}
        runtime = read(common["runtimes"][variant]["path"])
        entries = qualification_entries(dispatch, root, variant, runtime, family, canonical, eligible, haley)
        scored = []
        for entry in entries:
            ref = entry.get("report", entry)
            report = read(ref["path"])
            score = dispatch.projected_seconds(report, source, "training") + status[report["placement"]["host"]]["transport_seconds"]
            scored.append((score, ref, report["placement"]))
        best = min(scored, key=lambda item: item[0])
        require(best[2]["host"] == "desktop", "fastest eligible allocation is Haley; stage that host before formal execution: "
                + json.dumps({"variant": variant, "report": best[1], "placement": best[2], "projected_seconds": best[0]}))
        scores[variant] = [{"report": ref, "placement": placement, "projected_seconds": seconds} for seconds, ref, placement in scored]
        choices[variant] = ({"schema": dispatch.CHOICE, "inventory": status,
                             "qualifications": entries, "selected": {family: best[1]}}, best[2]["workers"])
    storage, logical = storage_preflight(dispatch, root, False)
    commands = []
    for variant, (choice, workers) in choices.items():
        choices[variant] = (write(root / "formal" / (variant + ".choice.json"), choice), workers)
    for index, variant in enumerate(("baseline", "candidate", "candidate", "baseline"), 1):
        label = f"io-{index:02d}-{variant}"
        choice, workers = choices[variant]
        request = make_request(dispatch, root, source, common["runtimes"][variant], variant, label, workers, storage, logical, False)
        request.update(choice=choice, choice_verification_output=str(root / "formal" / (label + ".verified.json")))
        ref = write(root / "formal" / (label + ".json"), request)
        commands.append({"label": label, "variant": variant, "workers": workers, "request": ref,
                         "argv": [str(args.python), "-B", str(root / variant / "python/tools/native_expanded_dispatch_v1.py"), "check-choice", ref["path"]]})
    return common | {"stage": "prepare", "inventory": inventory_ref, "transport": transport_ref,
                     "haley_qualifications": pin(args.haley_qualifications) if args.haley_qualifications else None,
                     "allocation_scores": scores, "storage": storage, "logical_projection_bytes": logical,
                     "commands": commands, "seal_argv": [str(args.python), "-B", str(Path(__file__).resolve()), "seal", "--root", str(root), "--python", str(args.python)]}


def seal(args, root, common):
    prepared_ref = pin(root / "formal/prepare.plan.json")
    prepared = read(prepared_ref["path"])
    for key in common:
        require(prepared[key] == common[key], "prepared source binding changed: " + key)
    fresh_inventory(prepared["inventory"])
    checked(prepared["transport"])
    commands = []
    for case in prepared["commands"]:
        request = read(checked(case["request"]))
        choice = read(checked(request["choice"]))
        for entry in choice["qualifications"]:
            checked(entry.get("report", entry))
        verification = pin(request["choice_verification_output"])
        result = read(verification["path"])
        selected = choice["selected"][common["workload_family"]]
        expected = next(row for row in prepared["allocation_scores"][case["variant"]] if row["report"] == selected)
        require(result["schema"] == choice["schema"] and result["outputs_verified"] is True
                and result["choice"] == request["choice"]
                and result["selected"] == {"qualification": selected, "projected_seconds": expected["projected_seconds"],
                                           "workload": common["workload_family"]}, "choice verification differs")
        checked(request["config"])
        checked(request["runtime"])
        request["choice_verification"] = verification
        final = write(root / "formal" / (case["label"] + ".dispatch.json"), request)
        commands.append({"label": case["label"], "variant": case["variant"], "workers": case["workers"],
                         "request": final, "argv": driver_argv(root, args.python, case["variant"], case["label"], final, "dispatch")})
    return common | {"stage": "seal", "prepared_plan": prepared_ref, "commands": commands}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("qualify", "prepare", "seal"))
    parser.add_argument("--root", type=Path, default=DEFAULT_ROOT)
    parser.add_argument("--python", type=Path, default=DEFAULT_PYTHON)
    parser.add_argument("--inventory", type=Path)
    parser.add_argument("--transport", type=Path)
    parser.add_argument("--haley-qualifications", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    require(args.root.is_absolute() and args.python.is_absolute(), "absolute root and pinned Python required")
    output = root / ("qualification.plan.json" if args.stage == "qualify" else f"formal/{args.stage}.plan.json")
    require(not output.exists(), "preserve existing staging plan: " + str(output))
    if args.stage != "qualify":
        require(not (root / "measure/hot/matched-native").exists(), "common native output already exists")
    labels = ([f"qual-{variant}-w{workers}" for variant in VARIANTS for workers in WORKERS]
              if args.stage == "qualify" else [f"io-{index:02d}-{variant}"
                  for index, variant in enumerate(("baseline", "candidate", "candidate", "baseline"), 1)])
    directory = root / ("requests" if args.stage == "qualify" else "formal")
    outputs = ([directory / (label + suffix) for label in labels for suffix in (".config.json", ".json")]
               if args.stage != "seal" else [directory / (label + ".dispatch.json") for label in labels])
    if args.stage == "prepare":
        outputs += [directory / (variant + ".choice.json") for variant in VARIANTS]
    require(all(not path.exists() for path in outputs), "preserve existing staging files; use a fresh attempt root")
    current = bindings(root, args.python)
    sys.path.insert(0, str(root / "candidate/python/tools"))
    dispatch = importlib.import_module("native_expanded_dispatch_v1")
    source_ref = pin(root / "source-config.json")
    source = read(source_ref["path"])
    require(len(source["iterations"]) == 162 and all(len(item["episodes"]) == 10 for item in source["iterations"]), "unchanged 162 x 10 schedule required")
    require(source.get("update_backend", {"kind": "cpu"}) == {"kind": "cpu"}, "CPU comparison required")
    common = {"schema": "training-io-speedup-desktop-staging/v1", "staging_only": True,
              "source_config": source_ref, "workload_family": dispatch.workload(source, "training"),
              "input_dependencies": native_inputs(source), "cpu_affinity": CORES,
              "native_affinity_mask": 21845, **current}
    if args.stage != "qualify":
        frozen = read(root / "qualification.plan.json")
        for key in common:
            require(frozen[key] == common[key], "qualification source binding changed: " + key)
    (root / "controllers").mkdir(exist_ok=True)
    if args.stage == "qualify":
        storage, logical = storage_preflight(dispatch, root, True)
        commands = []
        for variant in VARIANTS:
            for workers in WORKERS:
                label = f"qual-{variant}-w{workers}"
                request = make_request(dispatch, root, source, common["runtimes"][variant], variant, label, workers, storage, logical, True)
                ref = write(root / "requests" / (label + ".json"), request)
                commands.append({"label": label, "variant": variant, "workers": workers, "request": ref,
                                 "argv": driver_argv(root, args.python, variant, label, ref, "qualify")})
        plan = common | {"stage": "qualify", "storage": storage, "logical_projection_bytes": logical, "commands": commands}
    elif args.stage == "prepare":
        plan = prepare(args, dispatch, root, source, common)
    else:
        plan = seal(args, root, common)
    plan["observed_utc"] = datetime.now(timezone.utc).isoformat()
    print(json.dumps({"stage": args.stage, "plan": write(output, plan), "commands": len(plan["commands"])}))


if __name__ == "__main__":
    main()
