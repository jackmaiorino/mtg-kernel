"""Prepare pinned native-expanded requests only. Never launches or removes files."""
import argparse
import copy
from datetime import datetime, timezone
import hashlib
import importlib
import json
import math
from pathlib import Path
import stat
import sys

GIB = 1024 ** 3
COUNTS = (1, 2, 4, 8)
VARIANTS = ("baseline", "candidate")
HISTORICAL_BLOCK_WRITE_BYTES = 10_430_088_126 + 3_498_824_608

def require(ok, message):
    if not ok:
        raise ValueError(message)

def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))

def pin(path):
    path = Path(path).resolve()
    require(path.is_file(), "missing required input: " + str(path))
    return {"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

def write_new(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as stream:
        json.dump(value, stream, sort_keys=True, indent=2)
        stream.write("\n")
    return pin(path)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", choices=("qualification", "formal", "seal"))
    parser.add_argument("--root", type=Path, default=Path("C:/mtg-node/training-speedups-20261009"))
    parser.add_argument("--baseline-tools", type=Path, required=True)
    parser.add_argument("--candidate-tools", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, required=True, help="Fresh pinned three-host raw inventory")
    parser.add_argument("--projection-bytes", type=int, required=True,
                        help="Conservative peak additional writes including native, ZIP and metadata; no compression discount without measured evidence")
    parser.add_argument("--logical-projection-bytes", type=int,
                        help="Logical source plus ZIP write projection; defaults to physical projection")
    parser.add_argument("--transport-seconds", type=float, required=True,
                        help="Measured staging plus projected recovery transfer duration")
    parser.add_argument("--transport-evidence", type=Path, required=True)
    parser.add_argument("--wall-seconds", type=int, default=3600)
    args = parser.parse_args()
    require(args.root.is_absolute() and args.baseline_tools.is_absolute() and args.candidate_tools.is_absolute() and args.inventory.is_absolute(), "absolute paths required")
    require(args.projection_bytes > 0 and args.wall_seconds > 0, "positive projections and wall bound required")
    logical_projection = args.logical_projection_bytes if args.logical_projection_bytes is not None else args.projection_bytes
    require(logical_projection >= args.projection_bytes, "logical projection cannot be smaller than physical projection")
    require(math.isfinite(args.transport_seconds) and args.transport_seconds >= 0, "finite nonnegative measured transport required")
    require(args.transport_evidence.is_absolute(), "absolute transport evidence required")
    transport_pin = pin(args.transport_evidence)
    tool_roots = {"baseline": args.baseline_tools, "candidate": args.candidate_tools}
    for tools in tool_roots.values():
        pin(tools / "native_expanded_dispatch_v1.py")
    sys.path.insert(0, str(args.candidate_tools))
    dispatch = importlib.import_module("native_expanded_dispatch_v1")
    root = args.root.resolve()
    common_native = root / "measure" / "hot" / "matched-native"
    if args.stage in ("formal", "seal"):
        require(not common_native.exists(), "matched native path must be absent before preparation; preserve and verify recovery before owner-controlled pruning")
        require(logical_projection >= HISTORICAL_BLOCK_WRITE_BYTES,
                "full-block projection below observed 10,430,088,126 raw plus 3,498,824,608 archive bytes; resolve storage or placement without bypassing reserve")
    inventory_pin = pin(args.inventory)
    inventory = read(args.inventory)
    require(set(inventory) == {"desktop", "computehost", "runpod"}, "all three hosts required")
    now = datetime.now(timezone.utc)
    checked_at = {}
    for host in ("desktop", "computehost"):
        stamp = inventory[host]["observed_utc"].replace("Z", "+00:00")
        age = (now - datetime.fromisoformat(stamp)).total_seconds()
        require(0 <= age <= 86400, "stale host inventory: " + host)
        checked_at[host] = stamp
    require(inventory["desktop"]["reservations"], "desktop exclusion needs observed reservation")
    require(not inventory["computehost"]["reservations"], "Haley has a competing reservation")
    require(inventory["runpod"].get("error") and "403" in inventory["runpod"].get("message", ""), "record actual RunPod access limitation")
    checked_at["runpod"] = min(checked_at.values())
    source = read(root / "source-config.json")
    source_pin = pin(root / "source-config.json")
    require(len(source["iterations"]) == 162 and all(len(i["episodes"]) == 10 for i in source["iterations"]), "expected unchanged 162 x 10 block")
    require(source.get("update_backend", {"kind": "cpu"}) == {"kind": "cpu"}, "CPU recipe required")
    for name in ("hot", "cold"):
        folder = root / "measure" / name
        require(folder.is_dir(), "create NTFS-compressed parent before preparing requests: " + str(folder))
        require(folder.stat().st_file_attributes & stat.FILE_ATTRIBUTE_COMPRESSED,
                "measurement parent is not NTFS compressed: " + str(folder))
    runtimes = {}
    for variant in VARIANTS:
        ref = pin(root / "runtimes" / variant / "runtime.json")
        runtime = read(ref["path"])
        require(len(runtime["engine_commit"]) == 40 and len(runtime["tracked_tree_sha256"]) == 64, "incomplete runtime")
        require(pin(runtime["binary"]["path"])["sha256"] == runtime["binary"]["sha256"], "runtime binary changed")
        runtimes[variant] = ref
    storage = {"accounting_roots": [str(root / "measure")], "max_logical_bytes": 192 * GIB,
               "reserve_bytes": 60 * GIB, "projected_additional_bytes": args.projection_bytes,
               "projected_volume_bytes": {root.drive.upper(): args.projection_bytes}}
    # Read-only reserve/projection admission, including all retained ABBA outputs.
    observed = dispatch.validate_storage(storage)
    require(observed["logical_bytes"] + logical_projection <= storage["max_logical_bytes"],
            "current plus projected logical artifacts exceed allowance")
    requests = root / "requests"
    commands = []
    def launch_command(variant, label, action, path):
        driver = Path(__file__).resolve().with_name("case_driver.py")
        require(driver.is_file(), "case_driver.py must accompany request preparation")
        return {"action": action, "request": str(path), "driver": pin(driver),
                "argv": [sys.executable, "-B", str(driver), "--launcher-root", str(tool_roots[variant].parent.parent),
                         "--request", str(path), "--action", action,
                         "--receipt", str(requests / (label + ".controller.json")),
                         "--compute-host-name", "HALEYSPC"]}
    def request(variant, label, workers):
        hot = root / "measure" / "hot" / label
        cold = root / "measure" / "cold" / label
        require(not hot.exists() and not cold.exists(), "outputs already exist: " + label)
        config = copy.deepcopy(source)
        config["output_directory"] = str(hot / "native" if args.stage == "qualification" else common_native)
        config["collection_workers"] = workers
        config["preparation_workers"] = workers
        config_ref = write_new(requests / (label + ".config.json"), config)
        value = {"schema": dispatch.SCHEMA, "kind": "training", "runtime": runtimes[variant],
                 "config": config_ref, "root": str(hot), "cold_root": str(cold),
                 "lane": "codex-training-speedups-20261009",
                 "placement": {"host": "computehost", "workers": workers, "preparation_workers": workers,
                               "cpu_affinity": list(range(16)), "memory_bytes": 16 * GIB},
                 "wall_seconds": args.wall_seconds, "storage": storage,
                 "comparison_logical_projection_bytes": logical_projection}
        fraction = source.get("max_non_natural_episode_fraction", 0)
        if fraction:
            value["non_natural_tolerance"] = fraction
        return value
    if args.stage == "qualification":
        for variant in VARIANTS:
            for workers in COUNTS:
                label = f"qual-{variant}-w{workers}"
                value = request(variant, label, workers)
                path = requests / (label + ".json")
                write_new(path, value)
                commands.append(launch_command(variant, label, "qualify", path))
    elif args.stage == "formal":
        choices = {}
        for variant in VARIANTS:
            reports = [pin(root / "measure" / "hot" / f"qual-{variant}-w{w}" / "report.json") for w in COUNTS]
            loaded = [read(ref["path"]) for ref in reports]
            for w, report in zip(COUNTS, loaded):
                require(report["complete"] and report["qualification"] and report["completed_updates"] == 1 and report["completed_games"] == 10, "incomplete qualification")
                require(report["placement"]["workers"] == w, "qualification worker count changed")
            require(all(report["fingerprint"] == loaded[0]["fingerprint"] for report in loaded), "serial/parallel output parity failed")
            require(all(report["fingerprint"] == read(root / "measure" / "hot" / "qual-baseline-w1" / "report.json")["fingerprint"] for report in loaded), "baseline/candidate qualification parity failed")
            best = min(range(len(loaded)), key=lambda i: dispatch.projected_seconds(loaded[i], source, "training"))
            statuses = {host: {"eligible": host == "computehost", "checked_at": checked_at[host], "evidence": inventory_pin,
                         "reason": {"desktop": "active stage4a reservation", "computehost": "free observed Haley allocation", "runpod": "API 403 and no paid authority"}[host],
                         "cpu_affinity": list(range(16)) if host == "computehost" else [],
                         "transport_seconds": args.transport_seconds if host == "computehost" else 0,
                         "transport_evidence": transport_pin}
                        for host in inventory}
            family = dispatch.workload(source, "training")
            choice = {"schema": dispatch.CHOICE, "inventory": statuses, "qualifications": reports,
                      "selected": {family: reports[best]}}
            choices[variant] = (write_new(requests / (variant + ".choice.json"), choice), loaded[best]["placement"]["workers"])
        for index, variant in enumerate(("baseline", "candidate", "candidate", "baseline"), 1):
            label = f"block-{index:02d}-{variant}"
            choice_ref, workers = choices[variant]
            value = request(variant, label, workers)
            value["choice"] = choice_ref
            value["choice_verification_output"] = str(requests / (label + ".verified.json"))
            path = requests / (label + ".json")
            write_new(path, value)
            commands.append({"action": "check-choice", "request": str(path), "launcher": str(tool_roots[variant] / "native_expanded_dispatch_v1.py")})
    else:
        for index, variant in enumerate(("baseline", "candidate", "candidate", "baseline"), 1):
            label = f"block-{index:02d}-{variant}"
            path = requests / (label + ".json")
            value = read(path)
            config_ref = value["config"]
            require(pin(config_ref["path"])["sha256"] == config_ref["sha256"], "formal config changed")
            require(read(config_ref["path"])["output_directory"] == str(common_native),
                    "formal config does not use common matched native path")
            verification = pin(value["choice_verification_output"])
            saved = read(verification["path"])
            require(saved["outputs_verified"] and saved["choice"] == value["choice"], "choice snapshot differs")
            value["choice_verification"] = verification
            # Preserve check-choice input and publish a distinct final request.
            final_path = requests / (label + ".dispatch.json")
            write_new(final_path, value)
            commands.append(launch_command(variant, label, "dispatch", final_path))
    plan = {"stage": args.stage, "source_config": source_pin, "inventory": inventory_pin,
            "runtimes": runtimes, "storage": storage, "logical_projection_bytes": logical_projection, "requests_only": True,
            "transport_seconds": args.transport_seconds, "transport_evidence": transport_pin,
            "commands": commands,
            "formal_common_native": str(common_native),
            "formal_sequencing_prerequisite": (
                "Run ABBA cases serially. Before every case, the shared matched-native path must be absent. "
                "After each completed case, parent must copy the complete recovery to Jack E and verify it "
                "before pruning only the owned common native path for the next case. "
                "Unique dispatch receipts and cold roots stay preserved. "
                "Never execute multiple formal requests concurrently. "
                "Full-block comparison remains blocked unless current conservative storage guard admits "
                "measured physical raw plus archive writes while preserving 60 GiB free reserve, and all launches must use the case driver logical preflight.") }
    write_new(requests / (args.stage + ".plan.json"), plan)
    print(json.dumps(plan, indent=2))

if __name__ == "__main__":
    main()
