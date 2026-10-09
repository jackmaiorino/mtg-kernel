"""Read-only exact-parity and nonoverlapping timing summary for matched ABBA.

Manifest: {"sequential_nonoverlapping": true, "cases": [
 {"id":"A1", "variant":"baseline", "report": <file>, "execution": <file>,
  "archive": <file>, "inspect": <file>, "transport": <file>, "retain": <file>,
  "scheduler_stdout": <optional JSONL>, "update_reports": [<optional 162 files>],
  "controller": <optional case_driver receipt>,
  "retained_root": <optional retained block root>, "retained_manifest": <required RETAINED.json when retained_root>}], "qualifications": [<optional cases>]}
Each file is a path or {"path": ..., "sha256": ...}; local paths may be relative
 to --root. Explicit local files avoid remapping captured remote source paths.
Case phases must be sequential, report-bound, and complete. Controller polling
is displayed separately and never enters primary complete_case_seconds.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys


def require(ok, message):
    if not ok:
        raise ValueError(message)


def seconds(value, label, positive=False):
    require(type(value) in (int, float) and math.isfinite(value) and value >= 0
            and (not positive or value > 0), "invalid seconds: " + label)
    return value


class Reader:
    def __init__(self, root):
        self.root = root
        self.inputs = []
    def bytes(self, ref):
        path = Path(ref["path"] if isinstance(ref, dict) else ref)
        if not path.is_absolute():
            path = self.root / path
        require(path.is_file(), "missing input: " + str(path))
        payload = path.read_bytes()
        digest = hashlib.sha256(payload).hexdigest()
        if isinstance(ref, dict) and "sha256" in ref:
            require(digest == ref["sha256"], "changed input: " + str(path))
        self.inputs.append({"path": str(path.absolute()), "sha256": digest})
        return payload, digest
    def json(self, ref):
        payload, digest = self.bytes(ref)
        return json.loads(payload), digest


def linked(pin, digest, label):
    require(isinstance(pin, dict) and pin.get("sha256") == digest, "receipt binding differs: " + label)


def stage_timers(document):
    timers = document.get("timing", {})
    require(isinstance(timers, dict), "stage timing must be an object")
    return {key: seconds(value, key) for key, value in timers.items()}


def scheduler(reader, ref, count):
    payload, _ = reader.bytes(ref)
    records = [json.loads(line) for line in payload.decode("utf-8-sig").splitlines() if line.strip()]
    records = [entry for entry in records if "scheduler_timing" in entry]
    require(len(records) == count and [r["completed_iteration"] for r in records] == list(range(count)),
            "scheduler timing does not cover ordered newly completed updates")
    totals = {}
    for entry in records:
        timer = entry["scheduler_timing"]
        require(timer["schema"] == "phase1-scheduler-wall/v1", "unknown scheduler timing")
        require(timer["scope"] == "newly_completed_iteration_excludes_initialization_and_completed_prefix_validation", "unknown scheduler scope")
        for key in ("iteration_wall_seconds", "collection_execution_seconds", "update_execution_seconds",
                    "collection_validation_seconds", "update_validation_seconds"):
            totals[key] = totals.get(key, 0) + seconds(timer[key], key)
        require(sum(timer[k] for k in ("collection_execution_seconds", "update_execution_seconds",
                "collection_validation_seconds", "update_validation_seconds")) <= timer["iteration_wall_seconds"] + .001,
                "scheduler nested stages exceed enclosing iteration")
    return {"seconds": totals, "scope": records[0]["scheduler_timing"]["scope"],
            "nested": "collection/update/validation timers partition iteration wall; do not add to dispatch or child"}


def captured_path(value):
    # Captured Windows paths remain source identities even in a local mirror.
    return str(value).replace("\\", "/").rstrip("/").casefold()


def retained_update_refs(reader, local_root, kept, manifest_pin, count):
    native = captured_path(kept["native_root"])
    captured_retained = captured_path(manifest_pin["path"]).rsplit("/", 1)[0]
    mapping = {}
    targets = set()
    for item in kept["files"]:
        source, target = captured_path(item["from"]), captured_path(item["to"])
        require(source not in mapping and target not in targets, "duplicate retained mapping")
        require(source.startswith(native + "/") and target.startswith(captured_retained + "/"),
                "retained mapping escapes case roots")
        require(source[len(native):] == target[len(captured_retained):], "retained mapping changes relative path")
        mapping[source] = item
        targets.add(target)
    def selected(relative):
        source = native + "/" + relative.casefold()
        require(source in mapping, "case retained manifest omits selected receipt: " + relative)
        item = mapping[source]
        require(captured_path(item["to"]) == captured_retained + "/" + relative.casefold(),
                "retained target differs from selected receipt")
        return {"path": str(local_root / Path(relative)), "sha256": item["sha256"]}
    refs = []
    for index in range(count):
        prefix = f"iterations/{index:06d}/"
        completion, _ = reader.json(selected(prefix + "complete.json"))
        require(completion["iteration"] == index, "retained completion index differs")
        update_pin = completion["update"]
        source = captured_path(update_pin["path"])
        require(source.startswith(native + "/" + prefix), "retained update belongs to another case/iteration")
        relative = source[len(native) + 1:]
        require(".." not in Path(relative).parts, "unsafe retained relative path")
        ref = selected(relative)
        require(ref["sha256"] == update_pin["sha256"], "completion update pin differs from case retained mapping")
        # Reading the pinned selected receipt happens before aggregating timing.
        reader.bytes(ref)
        refs.append(ref)
    return refs


def read_case(reader, case, formal):
    report, report_sha = reader.json(case["report"])
    execution, execution_sha = reader.json(case["execution"])
    archive, archive_sha = reader.json(case["archive"])
    require(report.get("schema") == "native-expanded-cpu-dispatch/v1", "unknown dispatch schema")
    require(report.get("complete") is True and report.get("qualification") is (not formal), "incorrect completion/qualification")
    updates, games = (162, 1620) if formal else (1, 10)
    require(report["completed_updates"] == updates and report["completed_games"] == games, "incorrect completed-work count")
    require(len(report["fingerprint"]["iterations"]) == updates, "missing full optimizer fingerprint coverage")
    linked(report["execution"], execution_sha, "execution")
    linked(report["archive"], archive_sha, "archive")
    require(execution["exit_code"] == 0 and execution["error"] is None, "failed native execution")
    require(archive["scheme"] == "two-deflate1-shards-full-readback/v1" and archive["mismatches"] == 0
            and len(archive["shards"]) == 2, "incomplete recovery archive")
    dispatch_time = seconds(report["seconds"], "dispatch", True)
    execution_time = seconds(execution["seconds"], "execution", True)
    archive_time = seconds(archive["seconds"], "archive")
    require(dispatch_time >= execution_time and dispatch_time >= archive_time, "nested timers exceed dispatch")
    result = {"id": case["id"], "variant": case["variant"], "report_sha256": report_sha,
              "completed_updates": updates, "completed_games": games,
              "workers": report["placement"]["workers"],
              "preparation_workers": report["placement"]["preparation_workers"],
              "dispatch_seconds": dispatch_time, "native_execution_seconds": execution_time,
              "child_seconds": seconds(execution["child_seconds"], "child", True) if "child_seconds" in execution else None,
              "archive_seconds": archive_time, "fingerprint": report["fingerprint"],
              "runtime": report["runtime"], "artifact_timing": report.get("artifact_timing"),
              "archive_shard_timing": [s.get("timing") for s in archive["shards"]]}
    if result["child_seconds"] is not None:
        require(result["child_seconds"] <= execution_time + .001, "child exceeds execution")
    if formal:
        inspect, inspect_sha = reader.json(case["inspect"])
        retain, _ = reader.json(case["retain"])
        transport, transport_sha = reader.json(case["transport"])
        require(inspect["schema"] == "training-speedup-maintenance-inspect/v1" and inspect["complete"] is True, "incomplete maintenance inspection")
        require(retain["schema"] == "training-speedup-maintenance-retain/v1" and retain["complete"] is True, "incomplete retention")
        require(transport["schema"] == "training-speedup-independent-cold-copy/v1" and transport["complete"] is True, "incomplete actual cold-copy transport")
        linked(inspect["report"], report_sha, "inspection report")
        linked(retain["inspection"], inspect_sha, "retention inspection")
        linked(retain["independent_cold_copy"], transport_sha, "retention cold-copy receipt")
        linked(transport["plan"], inspect["recovery_copy_plan"]["sha256"], "transport plan")
        phases = {"inspect_seconds": seconds(inspect["seconds"], "inspect", True),
                  "transport_seconds": seconds(transport["seconds"], "transport", True),
                  "retain_seconds": seconds(retain["seconds"], "retain", True)}
        result.update(phases)
        result["complete_case_seconds"] = dispatch_time + sum(phases.values())
        result["maintenance_stages"] = {"inspect": stage_timers(inspect), "retain": stage_timers(retain)}
        result["transport_stages"] = stage_timers(transport)
        if "additional_sequential_phases" in case:
            additional = []
            for ref in case["additional_sequential_phases"]:
                doc, _ = reader.json(ref)
                require(doc["complete"] is True and doc.get("report", {}).get("sha256") == report_sha,
                        "extra phase must bind this report")
                additional.append({"schema": doc["schema"], "seconds": seconds(doc["seconds"], "extra phase", True),
                                   "timing": stage_timers(doc)})
            result["additional_sequential_phases"] = additional
            result["complete_case_seconds"] += sum(r["seconds"] for r in additional)
    if "scheduler_stdout" in case:
        result["scheduler"] = scheduler(reader, case["scheduler_stdout"], updates)
    update_refs = case.get("update_reports")
    if "retained_root" in case:
        require(formal, "retained timing requires formal case retention")
        require(update_refs is None, "choose retained_root or explicit update_reports")
        require("retained_manifest" in case, "case retained_manifest required with retained_root")
        kept, kept_sha = reader.json(case["retained_manifest"])
        linked(retain["retained_manifest"], kept_sha, "case retained manifest")
        require(kept["native_root"] == inspect["native_root"] == archive["native_root"],
                "retained manifest native root differs from case")
        retained = Path(case["retained_root"])
        if not retained.is_absolute():
            retained = reader.root / retained
        update_refs = retained_update_refs(reader, retained, kept, retain["retained_manifest"], updates)
    if update_refs is not None:
        require(len(update_refs) == updates, "partial learner timing coverage")
        totals = {}
        for ref in update_refs:
            require(isinstance(ref, dict) and "sha256" in ref, "update timing source pin required")
            doc, _ = reader.json(ref)
            require(doc["complete"] is True, "incomplete learner receipt")
            for key in ("input_read_seconds", "behavior_replay_seconds", "learner_update_seconds",
                        "checkpoint_io_seconds", "update_elapsed_seconds"):
                require(key in doc, "missing full update timing field: " + key)
                totals[key] = totals.get(key, 0) + seconds(doc[key], key)
            require(sum(doc[k] for k in ("input_read_seconds", "behavior_replay_seconds",
                    "learner_update_seconds", "checkpoint_io_seconds")) <= doc["update_elapsed_seconds"] + .001,
                    "nested update stages exceed update wall")
        if "scheduler" in result:
            require(totals["update_elapsed_seconds"] <= result["scheduler"]["seconds"]["update_execution_seconds"] + .01 * updates,
                    "inner update timers exceed scheduler update execution")
        result["learner_nested_seconds"] = totals
    if "controller" in case:
        controller, _ = reader.json(case["controller"])
        require(controller["complete"] is True, "incomplete case controller")
        linked(controller["report"], report_sha, "controller case report")
        result["controller_envelope_seconds"] = seconds(
            controller["controller_seconds_including_observation"], "controller including observation", True)
    return result


def compare(cases, field):
    baseline = statistics.mean(c[field] for c in cases if c["variant"] == "baseline")
    candidate = statistics.mean(c[field] for c in cases if c["variant"] == "candidate")
    return {"baseline_mean_seconds": baseline, "candidate_mean_seconds": candidate,
            "speedup": baseline / candidate, "percent_time_reduction": (1 - candidate / baseline) * 100,
            "adjacent_paired_speedups": [cases[0][field] / cases[1][field], cases[3][field] / cases[2][field]],
            "statistical_scope": "Two observed matched ABBA pairs; no confidence interval or playing-strength claim."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--case-manifest", type=Path, required=True)
    args = parser.parse_args()
    reader = Reader(args.root.absolute())
    manifest, _ = reader.json(args.case_manifest)
    require(manifest.get("sequential_nonoverlapping") is True, "explicit sequential nonoverlapping phase declaration required")
    inputs = manifest["cases"]
    require(len(inputs) == 4 and [c["variant"] for c in inputs] == ["baseline", "candidate", "candidate", "baseline"], "exact four-case ABBA required")
    require(len({c["id"] for c in inputs}) == 4, "unique cases required")
    cases = [read_case(reader, case, True) for case in inputs]
    require(all(case["fingerprint"] == cases[0]["fingerprint"] for case in cases), "full ABBA trajectory/optimizer/ledger fingerprint mismatch")
    require(len({case["report_sha256"] for case in cases}) == 4, "duplicate report reused as separate case")
    for variant in ("baseline", "candidate"):
        require(len({json.dumps(c["runtime"], sort_keys=True) for c in cases if c["variant"] == variant}) == 1, "variant runtime changed")
    qualifications = [read_case(reader, c, False) for c in manifest.get("qualifications", [])]
    if qualifications:
        for variant in ("baseline", "candidate"):
            rows = [c for c in qualifications if c["variant"] == variant]
            require([c["workers"] for c in rows] == [1, 2, 4, 8], "qualification must show serial then 1/2/4/8 per variant")
        require(all(c["fingerprint"] == qualifications[0]["fingerprint"] for c in qualifications), "qualification trajectory/optimizer/ledger fingerprint mismatch")
    result = {"schema": "training-speedups-matched-summary/v1", "complete": True,
              "full_fingerprint_parity": True, "dispatch_comparison": compare(cases, "dispatch_seconds"),
              "complete_case_comparison": compare(cases, "complete_case_seconds"),
              "accounting": "complete_case = dispatch + inspect + actual transport + retain + explicitly supplied extra sequential phases. Archive/child/scheduler/learner timers are nested diagnostics and never added again. Controller polling envelope is separate.",
              "cases": cases, "qualifications": qualifications, "inputs": reader.inputs}
    print(json.dumps(result, indent=2, allow_nan=False))
    print("case | variant | workers/prep | dispatch s | inspect s | transfer s | retain s | complete s", file=sys.stderr)
    for c in cases:
        print(f"{c['id']} | {c['variant']} | {c['workers']}/{c['preparation_workers']} | {c['dispatch_seconds']:.3f} | {c['inspect_seconds']:.3f} | {c['transport_seconds']:.3f} | {c['retain_seconds']:.3f} | {c['complete_case_seconds']:.3f}", file=sys.stderr)
    for label in ("dispatch_comparison", "complete_case_comparison"):
        value = result[label]
        print(f"{label}: {value['speedup']:.4f}x, {value['percent_time_reduction']:.2f}% less time", file=sys.stderr)

if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError) as error:
        print(json.dumps({"complete": False, "error": str(error)}), file=sys.stderr)
        sys.exit(2)
