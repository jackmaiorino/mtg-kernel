"""Read sets: manifests, executions, prep acceptance, look allocation, restarts and frozen panels.

Implements, for the readers (D7), read plan v3: v1 sections 5 and 7 with the v2 and v3 deltas.

- A set is one directory <root>/<namespace>/<run>/<set>/ with manifest.json, its plan or request
  files and one exec-<e>/ directory per execution.
- Plan sets: execution e = 0, and e = 1 only as the single watchdog rerun (A6.7). The chosen
  execution follows section 7; a watchdog stop or an interruption voids the read.
- Prep sets (v2 change 1): a unit counts as completed in an execution only with a valid marker
  exec-<e>/done/<unit>.json naming exactly the unit's output files with matching hashes. Every
  completed unit must be byte-identical across all executions; a difference stops the set. The
  accepted execution is the lowest-numbered one whose receipt passes v1's checks and whose listed
  units all carry valid markers equal to the receipt.
- Frozen panels (v2 change 2): pinned-panel checks apply to every dose and d13 set except the
  stage-1 prep set `prep-panel`, which its receipt rules alone check.

Marker format (written by D5, read here): {"unit": u, "files": [{"file": "prep/unit-<u>.json",
"sha256": h}]}, the files of the unit in the plan's unit layout, in that order.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402

PLAN_RECEIPT_KEYS = ("execution", "plan_set_sha256", "binary", "platform", "ended", "records")
PREP_RECEIPT_KEYS = ("execution", "plan_set_sha256", "binary", "platform", "ended", "outputs")
RECEIPT_RECORD_KEYS = ("pod_index", "sha256", "status")
RECEIPT_OUTPUT_KEYS = ("unit", "sha256", "status")
MARKER_KEYS = ("unit", "files")
MARKER_FILE_KEYS = ("file", "sha256")
WATCHDOG_STOP_KIND = "terminal:watchdog"


class SetError(ValueError):
    """A set, execution, receipt or panel the readers refuse."""


def _sha_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _load(path: Path) -> Any:
    try:
        return json.loads(path.read_bytes().decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SetError(f"{path.name}: unreadable JSON ({exc})") from exc


def _range(value: Any, label: str) -> list[int]:
    if not (isinstance(value, list) and len(value) == 2 and all(type(v) is int and v >= 0 for v in value)
            and value[0] <= value[1]):
        raise SetError(f"{label} must be [first, end) with first <= end")
    return value


# ---------------------------------------------------------------------------------------------
# Manifests


def load_manifest(set_dir: Path) -> dict[str, Any]:
    """manifest.json with strict keys, schema, set name, entry files and plan_set_sha256."""
    set_dir = Path(set_dir)
    man = _load(set_dir / "manifest.json")
    try:
        man = pod_record._object(man, "manifest", read_plans.MANIFEST_KEYS)
    except pod_record.RecordError as exc:
        raise SetError(str(exc)) from exc
    if man["schema"] != read_plans.PLAN_SCHEMA or man["interfaces"] != read_plans.INTERFACES:
        raise SetError(f"manifest schema or interfaces unknown: {man['schema']!r}, {man['interfaces']!r}")
    if man["kind"] not in ("plans", "prep"):
        raise SetError("manifest kind is plans or prep")
    if (set_dir.name, set_dir.parent.name, set_dir.parent.parent.name) != (man["set"], man["run"], man["namespace"]):
        raise SetError("the set directory is not <namespace>/<run>/<set> of its manifest")
    try:
        read_plans.check_run(man["namespace"], man["run"])
        read_plans.check_set_name(man["namespace"], man["family"], man["set"])
    except read_plans.PlanError as exc:
        raise SetError(str(exc)) from exc
    _range(man["units"], "manifest.units")
    if man["reserved"] is not None:
        _range(man["reserved"], "manifest.reserved")
    for r in man["retired"]:
        _range(r, "manifest.retired entry")
    try:
        read_plans.check_d8_v3_manifest_shape(man)
    except read_plans.PlanError as exc:
        raise SetError(str(exc)) from exc
    index_key = "pod_index" if man["kind"] == "plans" else "unit"
    folder, stem = ("plans", "pod") if man["kind"] == "plans" else ("requests", "unit")
    for i, entry in enumerate(man["plans"]):
        pod_record._object(entry, f"manifest.plans[{i}]", (index_key, "file", "sha256"))
        if entry["file"] != f"{folder}/{stem}-{entry[index_key]}.json":
            raise SetError(f"manifest entry {i} names file {entry['file']!r}")
    if man["plan_set_sha256"] != read_plans.plan_set_sha256(man["plans"]):
        raise SetError("plan_set_sha256 does not match the manifest's plans")
    for i, entry in enumerate(man["executions"]):
        pod_record._object(entry, f"manifest.executions[{i}]", read_plans.EXECUTION_ENTRY_KEYS)
        if entry["execution"] != i:
            raise SetError("execution entries are numbered from 0 in order")
        allowed = ("first",) if i == 0 else (("prep_retry",) if man["kind"] == "prep" else ("watchdog_rerun",))
        if entry["reason"] not in allowed:
            raise SetError(f"execution {i} has reason {entry['reason']!r}")
    if man["kind"] == "plans" and len(man["executions"]) > 2:
        raise SetError("a plan set has at most executions 0 and 1")
    return man


def load_entries(set_dir: Path, man: dict[str, Any]) -> dict[int, tuple[Any, str]]:
    """The plan or request files: exactly the listed files, each hashing to its entry."""
    set_dir = Path(set_dir)
    index_key = "pod_index" if man["kind"] == "plans" else "unit"
    folder = "plans" if man["kind"] == "plans" else "requests"
    listed = {e["file"] for e in man["plans"]}
    present = {f"{folder}/{p.name}" for p in (set_dir / folder).glob("*")} if (set_dir / folder).exists() else set()
    if listed != present:
        raise SetError(f"{folder}/ holds {sorted(present - listed)} unlisted, lacks {sorted(listed - present)}")
    out = {}
    for entry in man["plans"]:
        path = set_dir / entry["file"]
        if _sha_file(path) != entry["sha256"]:
            raise SetError(f"{entry['file']} does not hash to its manifest entry")
        out[entry[index_key]] = (_load(path), entry["sha256"])
    return out


# ---------------------------------------------------------------------------------------------
# Prep sets (v1 section 5 with v2 change 1)


def _valid_marker(exec_dir: Path, unit: int) -> str | None:
    """The output hash of a unit completed in this execution, or None if its marker is absent or
    invalid (missing or extra files, non-u64 unit, or a hash that does not match)."""
    marker_path = exec_dir / "done" / f"{unit}.json"
    if not marker_path.exists():
        return None
    try:
        marker = pod_record._object(_load(marker_path), "marker", MARKER_KEYS)
        marker_unit = marker["unit"]
        if type(marker_unit) is not int or not (0 <= marker_unit <= pod_record.U64) or marker_unit != unit:
            return None
        files = marker["files"]
        expected = [f"prep/unit-{unit}.json"]
        if not isinstance(files, list) or [
                pod_record._object(f, "marker.file", MARKER_FILE_KEYS)["file"] for f in files] != expected:
            return None
        sha = files[0]["sha256"]
        if not read_plans._is_sha(sha):
            return None
        path = exec_dir / expected[0]
        if not path.exists() or _sha_file(path) != sha:
            return None
        return sha
    except (pod_record.RecordError, SetError, KeyError, TypeError):
        return None


def accept_prep_set(set_dir: Path, man: dict[str, Any] | None = None, *,
                    expected_pins: Any = None, expected_picker: Any = None,
                    expected_builders: dict[str, Any] | None = None) -> dict[str, Any]:
    """The accepted execution of a prep set and its outputs {unit: (result, sha256)}."""
    set_dir = Path(set_dir)
    man = man or load_manifest(set_dir)
    if man["kind"] != "prep":
        raise SetError("not a prep set")
    requests = load_entries(set_dir, man)
    first_bindings = None
    for unit, (request, _) in sorted(requests.items()):
        try:
            read_plans.check_prep_request(
                request, expected_pins=expected_pins, expected_picker=expected_picker,
                expected_builders=expected_builders,
            )
            b = read_plans.prep_request_bindings(request)
        except (read_plans.PlanError, pod_record.RecordError) as exc:
            raise SetError(f"request of unit {unit}: {exc}") from exc
        if first_bindings is None:
            first_bindings = b
        elif b != first_bindings:
            raise SetError(f"prep request unit {unit} bindings differ from unit {min(requests)}")
    units = sorted(requests)
    first, end = man["units"]
    if units != list(range(first, end)):
        raise SetError("the prep manifest lists one request per unit of its range")
    exec_dirs = sorted((p for p in set_dir.glob("exec-*") if p.is_dir()), key=lambda p: int(p.name[5:]))
    if [int(p.name[5:]) for p in exec_dirs] != list(range(len(exec_dirs))):
        raise SetError("execution directories are exec-0, exec-1, ... without gaps")
    if len(exec_dirs) > len(man["executions"]):
        raise SetError("an execution directory has no manifest entry")
    # Byte identity across every attempt: a unit's completed outputs must agree.
    completed: dict[int, dict[int, str]] = {}
    for e, exec_dir in enumerate(exec_dirs):
        for unit in units:
            sha = _valid_marker(exec_dir, unit)
            if sha is not None:
                completed.setdefault(unit, {})[e] = sha
    for unit, by_exec in completed.items():
        if len(set(by_exec.values())) > 1:
            raise SetError(f"determinism failure: unit {unit} differs across executions {sorted(by_exec)}; "
                           "the set stops for a fix")
    reasons = []
    for e, exec_dir in enumerate(exec_dirs):
        reason = _prep_receipt_problem(exec_dir, man, e, units, requests)
        if reason is None:
            outputs = {u: (_load(exec_dir / "prep" / f"unit-{u}.json"), completed[u][e]) for u in units}
            for unit, (result, _) in outputs.items():
                try:
                    read_plans.check_prep_result(result, requests[unit][0], requests[unit][1])
                except (read_plans.PlanError, pod_record.RecordError) as exc:
                    raise SetError(f"prep result of unit {unit}: {exc}") from exc
            return {"execution": e, "outputs": outputs, "requests": requests, "manifest": man}
        reasons.append(f"exec-{e}: {reason}")
    raise SetError("no accepted prep execution: " + "; ".join(reasons or ["none ran"]))


def _prep_receipt_problem(exec_dir: Path, man: dict[str, Any], e: int, units: list[int],
                          requests: dict[int, tuple[Any, str]]) -> str | None:
    path = exec_dir / "execution.json"
    if not path.exists():
        return "no receipt"
    try:
        rec = pod_record._object(_load(path), "execution", PREP_RECEIPT_KEYS)
    except pod_record.RecordError as exc:
        return str(exc)
    entry = man["executions"][e]
    if (rec["execution"], rec["binary"], rec["platform"]) != (e, entry["binary"], entry["platform"]):
        return "receipt differs from its manifest entry"
    if rec["plan_set_sha256"] != man["plan_set_sha256"]:
        return "receipt names another plan set"
    if rec["ended"] != "complete":
        return f"ended {rec['ended']!r}"
    listed = [o.get("unit") for o in rec["outputs"]] if isinstance(rec["outputs"], list) else None
    if listed is None or sorted(listed) != units or len(set(listed)) != len(listed):
        return "outputs do not list every unit exactly once"
    prep_files = {p.name for p in (exec_dir / "prep").glob("*")} if (exec_dir / "prep").exists() else set()
    if prep_files != {f"unit-{u}.json" for u in units}:
        return "prep/ holds other files than the listed outputs"
    for out in rec["outputs"]:
        try:
            pod_record._object(out, "output", RECEIPT_OUTPUT_KEYS)
        except pod_record.RecordError as exc:
            return str(exc)
        unit = out["unit"]
        if out["status"] != "complete":
            return f"unit {unit} status {out['status']!r}"
        if _sha_file(exec_dir / "prep" / f"unit-{unit}.json") != out["sha256"]:
            return f"unit {unit} does not hash to its entry"
        if _valid_marker(exec_dir, unit) != out["sha256"]:
            return f"unit {unit} lacks a valid completion marker equal to the receipt"
        result = _load(exec_dir / "prep" / f"unit-{unit}.json")
        if not isinstance(result, dict) or result.get("request_sha256") != requests[unit][1]:
            return f"unit {unit} answers another request"
    return None


# ---------------------------------------------------------------------------------------------
# Plan sets: choosing the execution (v1 section 7)


def _plan_receipt(exec_dir: Path, man: dict[str, Any], e: int) -> dict[str, Any]:
    path = exec_dir / "execution.json"
    if not path.exists():
        raise SetError(f"exec-{e} has no execution.json")
    rec = pod_record._object(_load(path), "execution", PLAN_RECEIPT_KEYS)
    entry = man["executions"][e]
    if (rec["execution"], rec["binary"], rec["platform"]) != (e, entry["binary"], entry["platform"]):
        raise SetError(f"exec-{e}'s receipt differs from its manifest entry")
    if rec["plan_set_sha256"] != man["plan_set_sha256"]:
        raise SetError(f"exec-{e}'s receipt names another plan set")
    if rec["ended"] not in ("complete", "watchdog_stop", "interrupted"):
        raise SetError(f"exec-{e} ended {rec['ended']!r}")
    pods = {p["pod_index"] for p in man["plans"]}
    names = []
    for item in rec["records"]:
        pod_record._object(item, "receipt record", RECEIPT_RECORD_KEYS)
        names.append(item["pod_index"])
    if len(set(names)) != len(names) or not set(names) <= pods:
        raise SetError(f"exec-{e}'s receipt names a pod twice or outside the manifest")
    files = {p.name for p in (exec_dir / "records").glob("*")} if (exec_dir / "records").exists() else set()
    if files != {f"pod-{i}.json" for i in names}:
        raise SetError(f"exec-{e}/records holds other files than the receipt lists")
    records = {}
    for item in rec["records"]:
        path = exec_dir / "records" / f"pod-{item['pod_index']}.json"
        if _sha_file(path) != item["sha256"]:
            raise SetError(f"exec-{e} pod {item['pod_index']} does not hash to its receipt entry")
        record = pod_record.validate_record(_load(path), f"exec-{e}/pod-{item['pod_index']}")
        if record["status"] != item["status"]:
            raise SetError(f"exec-{e} pod {item['pod_index']} status differs from its receipt entry")
        records[item["pod_index"]] = record
    statuses = {r["status"] for r in records.values()}
    if rec["ended"] == "complete" and (set(names) != pods or statuses - {"complete"}):
        raise SetError(f"exec-{e} ended complete without every pod complete")
    if rec["ended"] == "watchdog_stop":
        stopped_pods = [i for i, r in sorted(records.items()) if r["status"] == "stopped"]
        if len(stopped_pods) != 1:
            raise SetError(f"exec-{e} ended watchdog_stop without exactly one stopped pod")
        sp = records[stopped_pods[0]]
        if not sp["games"] or sp["games"][-1]["attempts"][-1]["stop_kind"] != WATCHDOG_STOP_KIND:
            raise SetError(f"exec-{e} watchdog_stop pod {stopped_pods[0]} did not stop on {WATCHDOG_STOP_KIND}")
    return {"receipt": rec, "records": records}


def _expired(records: dict[int, dict[str, Any]]) -> list[list[int]]:
    return [[i, g["game"]] for i in sorted(records) for g in records[i]["games"]
            for a in g["attempts"] if a["stop_kind"] == WATCHDOG_STOP_KIND]


def choose_plan_execution(set_dir: Path, man: dict[str, Any]) -> dict[str, Any]:
    """Section 7. Returns {execution, records, expired: {e: games}, void: None or a reason}."""
    set_dir = Path(set_dir)
    if not man["executions"]:
        raise SetError("the set has no execution")
    e0 = _plan_receipt(set_dir / "exec-0", man, 0)
    expired0 = _expired(e0["records"])
    out = {"execution": 0, "records": e0["records"], "expired": {"0": expired0}, "void": None}
    if e0["receipt"]["ended"] == "interrupted":
        out["void"] = "execution 0 was interrupted; the read restarts in its restart set"
        return out
    if e0["receipt"]["ended"] == "watchdog_stop":
        raise SetError("only the rerun (e = 1) may end watchdog_stop")
    if not expired0:
        if len(man["executions"]) > 1:
            raise SetError("execution 1 exists although execution 0 holds no watchdog halt")
        return out
    # e = 0 holds a watchdog halt: it is void and e = 1 must exist on the same seeds and platform.
    if len(man["executions"]) < 2:
        out["void"] = "execution 0 holds a watchdog halt and its rerun has not run"
        return out
    entry0, entry1 = man["executions"]
    if (entry1["binary"], entry1["platform"]) != (entry0["binary"], entry0["platform"]):
        raise SetError("the watchdog rerun uses another binary or platform")
    if entry1["expired_games"] != expired0:
        raise SetError("the rerun's manifest entry does not list execution 0's expired games")
    e1 = _plan_receipt(set_dir / "exec-1", man, 1)
    expired1 = _expired(e1["records"])
    out.update(execution=1, records=e1["records"])
    out["expired"]["1"] = expired1
    ended = e1["receipt"]["ended"]
    if ended == "interrupted":
        out["void"] = "the rerun was interrupted; the read restarts in its restart set"
    elif ended == "watchdog_stop":
        same = any(g in expired0 for g in expired1)
        out["void"] = ("hang: the rerun expired on the same game (A6.7 restart)" if same
                       else "void infrastructure failure: the rerun expired on a different game")
    elif expired1:
        raise SetError("a rerun that meets a watchdog expiry stops at once")
    return out


# ---------------------------------------------------------------------------------------------
# Look allocation and restarts (v1 section 5, A6.6, A6.7, A6.16)


def check_look_allocation(man: dict[str, Any], look: int, n: int, look1: dict[str, Any] | None = None) -> None:
    """Look 1 has units [f, f + n) and reserves [f, f + 3n); look 2 has [f + n, f + 3n) and names
    the look-1 plan set in pools_with."""
    first, end = man["units"]
    if man["look"] != look:
        raise SetError(f"manifest look is {man['look']!r}, not {look}")
    if look == 1:
        if end - first != n or man["reserved"] != [first, first + 3 * n] or man["pools_with"] is not None:
            raise SetError(f"look 1 has n = {n} units and reserves 3n: units {man['units']}, "
                           f"reserved {man['reserved']}")
        return
    if look1 is None:
        raise SetError("look 2 is read with its look-1 set")
    f = look1["units"][0]
    if man["units"] != [f + n, f + 3 * n] or man["pools_with"] != look1["plan_set_sha256"]:
        raise SetError("look 2 has the 2n fresh units after look 1 and names look 1 in pools_with")


def check_restart(interrupted: dict[str, Any], restart: dict[str, Any]) -> None:
    """A restart set names the interrupted set, starts at the next unreserved index, and retires the
    interrupted set's unused reserved units."""
    if restart["replaces"] != interrupted["set"] or not restart["set"].startswith(interrupted["set"].split(
            "-restart-")[0] + "-restart-"):
        raise SetError("a restart set is <set>-restart-<k> and replaces the interrupted set")
    reserved = interrupted["reserved"] or interrupted["units"]
    unused = [interrupted["units"][1], reserved[1]]
    if restart["units"][0] < reserved[1]:
        raise SetError("a restart starts at the next unreserved index")
    want = [unused] if unused[0] < unused[1] else []
    if restart["retired"] != want:
        raise SetError(f"the restart retires the interrupted set's unused reserved units {want}")


def check_shared_prefix_restart(prior_bc: list[dict[str, Any]], new_bc: dict[str, Any], n_b: int, n_c: int) -> None:
    """A6.7 shared prefix: the new b_c prep manifest reserves [f', f' + 3 max(n_b, n_c)) with f' the
    first read-gate-pools index above every earlier b_c reservation."""
    f_new = max((m["reserved"] or m["units"])[1] for m in prior_bc)
    want_range = [f_new, f_new + 3 * max(n_b, n_c)]
    if new_bc["reserved"] != want_range or new_bc["units"] != want_range:
        raise SetError(
            f"the shared-prefix restart must have units and reserved {want_range} "
            f"(f' = {f_new}, 3 max(n_b, n_c) = {3 * max(n_b, n_c)})"
        )
    check_restart(prior_bc[-1], new_bc)


def _set_bindings(plans: dict[int, dict[str, Any]]) -> dict[str, Any]:
    """Extract and enforce uniform pins, pilot, inputs, picker and builders across a plan set."""
    if not plans:
        raise SetError("plan set holds no plans")
    ref = None
    for pid, plan in sorted(plans.items()):
        pb = read_plans.plan_bindings(plan)
        cur = {
            "pins": plan["pins"],
            "pilot": plan["pilot"],
            "inputs": plan["inputs"],
            "picker": pb["picker"],
            "builders": pb["builders"],
        }
        if ref is None:
            ref = cur
        elif cur != ref:
            raise SetError(f"plan {pid} bindings differ from earlier plans in the same set")
    return ref


def check_look_bindings(look1_set: dict[str, Any], look2_set: dict[str, Any]) -> None:
    """Enforce identical pins, pilot, inputs, picker, builders and prep_set across look 1 and look 2 (A6.2, A6.6)."""
    if look1_set["manifest"]["prep_set"] != look2_set["manifest"]["prep_set"]:
        raise SetError("look 2 prep_set differs from look 1: both looks draw from the same prep set")
    b1 = _set_bindings(look1_set["plans"])
    b2 = _set_bindings(look2_set["plans"])
    for key in ("pins", "pilot", "inputs", "picker", "builders"):
        if b1[key] != b2[key]:
            raise SetError(f"look 2 {key} differs from look 1: policies and inputs must stay fixed across looks")


def _is_restarted_bc_set(opened: dict[str, Any]) -> bool:
    man = opened["manifest"]
    if man["replaces"] is not None:
        return True
    if man["prep_set"] is not None:
        prep_path, _, _ = man["prep_set"].rpartition("/exec-")
        prep_dir = Path(opened["set_dir"]).parents[2] / prep_path
        if (prep_dir / "manifest.json").exists():
            prep_man = load_manifest(prep_dir)
            if prep_man["replaces"] is not None:
                return True
    return False


def check_shared_bc_prefix(b_sets: list[dict[str, Any]], c_sets: list[dict[str, Any]]) -> None:
    """READ-PLAN-v1 section 7, A6.7 & A6.16: for (b) and (c) on the same unit, check equal
    prep_sha256, prep_set, seeds, pools, and candidate cards/bindings. Different starting units are
    permitted only when the earlier gate completed and the later gate restarted disjointly above
    the completed gate's reservation (A6.7)."""
    b_by_unit: dict[int, tuple[dict[str, Any], dict[str, Any]]] = {}
    for s in b_sets:
        if s["manifest"]["family"] != "b":
            raise SetError(f"expected family 'b' in shared (b)/(c) prefix check, got {s['manifest']['family']!r}")
        for plan in s["plans"].values():
            b_by_unit[plan["unit"]] = (plan, s["manifest"])
    c_by_unit: dict[int, tuple[dict[str, Any], dict[str, Any]]] = {}
    for s in c_sets:
        if s["manifest"]["family"] != "c":
            raise SetError(f"expected family 'c' in shared (b)/(c) prefix check, got {s['manifest']['family']!r}")
        for plan in s["plans"].values():
            c_by_unit[plan["unit"]] = (plan, s["manifest"])
    if not b_by_unit or not c_by_unit:
        raise SetError("shared (b)/(c) prefix check requires non-empty (b) and (c) sets")
    if min(b_by_unit) != min(c_by_unit):
        if min(b_by_unit) < min(c_by_unit):
            early_sets, late_sets, late_start = b_sets, c_sets, min(c_by_unit)
        else:
            early_sets, late_sets, late_start = c_sets, b_sets, min(b_by_unit)
        early_completed = all(s.get("void") is None for s in early_sets)
        early_reserved_end = max((s["manifest"]["reserved"] or s["manifest"]["units"])[1] for s in early_sets)
        late_first = min(late_sets, key=lambda s: s["manifest"]["units"][0])
        if not (early_completed and _is_restarted_bc_set(late_first) and late_start >= early_reserved_end):
            raise SetError(f"shared (b)/(c) prefix: (b) starts at unit {min(b_by_unit)}, (c) at unit {min(c_by_unit)}")
    for unit in sorted(set(b_by_unit) & set(c_by_unit)):
        bp, bman = b_by_unit[unit]
        cp, cman = c_by_unit[unit]
        if bman["prep_set"] != cman["prep_set"]:
            raise SetError(f"shared unit {unit}: (b) and (c) cite different prep_set ({bman['prep_set']!r} vs {cman['prep_set']!r})")
        if bp["prep_sha256"] != cp["prep_sha256"]:
            raise SetError(f"shared unit {unit}: (b) and (c) cite different prep_sha256")
        if bp["seeds"] != cp["seeds"]:
            raise SetError(f"shared unit {unit}: (b) and (c) have different seeds")
        if bp["pools"] != cp["pools"]:
            raise SetError(f"shared unit {unit}: (b) and (c) have different pools")
        for s_idx in range(pod_record.SEATS):
            if bp["chairs"][s_idx]["picker"] != cp["chairs"][s_idx]["picker"]:
                raise SetError(f"shared unit {unit} chair {s_idx}: (b) and (c) have different pickers")
            b_cand = bp["decks"][2 * s_idx]
            c_cand = cp["decks"][3 * s_idx]
            if b_cand["builder"] != c_cand["builder"] or b_cand["cards"] != c_cand["cards"]:
                raise SetError(f"shared unit {unit} chair {s_idx}: (b) and (c) have different candidate cards or builder")


def discover_peer_bc_sets(sets: list[dict[str, Any]], peer_family: str, peer_ns: str,
                          declaration_sha: str, *, expected_pins: Any = None) -> list[list[dict[str, Any]]]:
    """Discover active (non-replaced) peer (b)/(c) plan sets on disk under <root>/<peer_ns>/<run>/."""
    if not sets:
        return []
    root = Path(sets[0]["set_dir"]).parents[2]
    run = sets[0]["manifest"]["run"]
    peer_run_dir = root / peer_ns / run
    if not peer_run_dir.is_dir():
        return []
    candidates: list[tuple[Path, dict[str, Any]]] = []
    for s_dir in sorted(peer_run_dir.iterdir()):
        if not (s_dir / "manifest.json").exists():
            continue
        s_man = load_manifest(s_dir)
        if (s_man["kind"] == "plans" and s_man["family"] == peer_family
                and s_man["declaration_sha256"] == declaration_sha):
            candidates.append((s_dir, s_man))
    if not candidates:
        return []
    replaced = {m["replaces"] for _, m in candidates if m["replaces"] is not None}
    active = [(d, m) for d, m in candidates if m["set"] not in replaced]
    if not active:
        return []
    pool, constants, card_index, cube_table = sets[0].get("_dealer", (None, None, None, None))
    opened_all = [
        open_plan_set(d, declaration_sha, pool=pool, constants=constants,
                      card_index=card_index, cube_table=cube_table, expected_pins=expected_pins)
        for d, _ in active
    ]
    by_sha = {o["manifest"]["plan_set_sha256"]: o for o in opened_all}
    look1s = [o for o in opened_all if o["manifest"]["look"] == 1]
    look2s = [o for o in opened_all if o["manifest"]["look"] == 2]
    groups: list[list[dict[str, Any]]] = []
    used: set[str] = set()
    for l1 in look1s:
        sha1 = l1["manifest"]["plan_set_sha256"]
        grp = [l1]
        used.add(sha1)
        for l2 in look2s:
            if l2["manifest"]["pools_with"] == sha1:
                grp.append(l2)
                used.add(l2["manifest"]["plan_set_sha256"])
        groups.append(grp)
    for o in opened_all:
        if o["manifest"]["plan_set_sha256"] not in used and o["manifest"]["pools_with"] not in by_sha:
            groups.append([o])
    return groups


def _merge_ranges(indices: list[int]) -> list[list[int]]:
    """Merge sorted unique integers into contiguous half-open [first, end) ranges."""
    if not indices:
        return []
    out: list[list[int]] = []
    start = prev = indices[0]
    for x in indices[1:]:
        if x == prev + 1:
            prev = x
        else:
            out.append([start, prev + 1])
            start = prev = x
    out.append([start, prev + 1])
    return out


def _exec_dirs_and_indices(set_dir: Path, man: dict[str, Any]) -> list[tuple[int, Path]]:
    found: dict[int, Path] = {}
    for entry in man["executions"]:
        e = entry["execution"]
        found[e] = set_dir / f"exec-{e}"
    if set_dir.exists():
        for p in set_dir.iterdir():
            if not p.is_dir():
                continue
            m = re.fullmatch(r"(?:\.supervisor-)?exec-(\d+)", p.name)
            if m:
                e = int(m.group(1))
                found.setdefault(e, set_dir / f"exec-{e}")
    return sorted(found.items())


def d8_started_units(set_dir: Path, man: dict[str, Any]) -> set[int]:
    """Q2.1 & New 1: union of units started in ANY execution of a D8 set, aligned with D5.
    For plan sets: scans execution.json, records/pod-*.json, timing/pod-*.jsonl AND D5 supervisor
    journal files .supervisor-exec-<e>/pod-<pod_index>.journal.jsonl.
    For prep sets: counts a unit as started/used iff it has a valid completion marker in exec-<e>
    (matching D5 valid_prep_marker and RP v2 change 1 discard of unmarked partial outputs)."""
    set_dir = Path(set_dir)
    started: set[int] = set()
    is_aa = man["family"] == "aa"
    if man["kind"] == "prep":
        units_range = range(man["units"][0], man["units"][1])
        for _, exec_dir in _exec_dirs_and_indices(set_dir, man):
            if not exec_dir.exists():
                continue
            for u in units_range:
                if _valid_marker(exec_dir, u) is not None:
                    started.add(u)
        return started

    for e, exec_dir in _exec_dirs_and_indices(set_dir, man):
        if exec_dir.exists():
            receipt_path = exec_dir / "execution.json"
            if receipt_path.exists():
                try:
                    rec_doc = _load(receipt_path)
                except SetError:
                    rec_doc = None
                if isinstance(rec_doc, dict):
                    for item in rec_doc.get("records", []):
                        if isinstance(item, dict) and type(item.get("pod_index")) is int:
                            pid = item["pod_index"]
                            started.add(pid // 2 if is_aa else pid)
            for folder, pat in (("records", r"pod-(\d+)\.json"), ("timing", r"pod-(\d+)\.jsonl")):
                sub = exec_dir / folder
                if sub.is_dir():
                    for p in sub.iterdir():
                        m = re.fullmatch(pat, p.name)
                        if m:
                            pid = int(m.group(1))
                            started.add(pid // 2 if is_aa else pid)
        sup_dir = set_dir / f".supervisor-exec-{e}"
        if sup_dir.is_dir():
            for p in sup_dir.iterdir():
                m = re.fullmatch(r"pod-(\d+)\.journal\.jsonl", p.name)
                if m:
                    pid = int(m.group(1))
                    started.add(pid // 2 if is_aa else pid)
    return started


def d8_unused_reserved_ranges(set_dir: Path, man: dict[str, Any]) -> list[list[int]]:
    """Q2.1: reserved range minus the union of units started in ANY execution, merged into [first, end)."""
    r0, r1 = man["reserved"]
    started = d8_started_units(set_dir, man)
    unused = [u for u in range(r0, r1) if u not in started]
    return _merge_ranges(unused)


def is_d8_interrupted_set(set_dir: Path, man: dict[str, Any] | None = None) -> tuple[bool, str]:
    """Q2.2: predicate for an existing interrupted set in dose-d8 / read-d8-aa.
    Includes: ended 'interrupted', same-game watchdog hang, (i) execution leaving no execution.json,
    (ii) interrupted prep set (prep-dose-<ordinal>). Excludes (iii) completed sets and
    different-game watchdog infrastructure failures."""
    set_dir = Path(set_dir)
    man = man or load_manifest(set_dir)
    execs = _exec_dirs_and_indices(set_dir, man)
    if man["kind"] == "prep":
        if man["set"] == "prep-panel" or man["set"].startswith("prep-panel-restart-"):
            return False, "prep-panel is never replaced"
        try:
            accept_prep_set(set_dir, man)
            return False, "prep set completed with an accepted execution"
        except SetError:
            pass
        if not execs:
            return False, "prep set has no execution"
        for e, exec_dir in execs:
            if not (exec_dir / "execution.json").exists():
                return True, f"prep execution {e} left no execution.json"
            rec = _load(exec_dir / "execution.json")
            if isinstance(rec, dict) and rec.get("ended") == "interrupted":
                return True, f"prep execution {e} ended interrupted"
        return True, "prep set did not complete"

    if not execs:
        return False, "plan set has no execution"
    exec_map = dict(execs)
    if 0 not in exec_map:
        return False, "plan set has no execution 0"
    if not (exec_map[0] / "execution.json").exists():
        return True, "execution 0 left no execution.json"
    e0 = _plan_receipt(exec_map[0], man, 0)
    ended0 = e0["receipt"]["ended"]
    if ended0 == "interrupted":
        return True, "execution 0 ended interrupted"
    if ended0 == "complete":
        expired0 = _expired(e0["records"])
        if not expired0:
            return False, "set completed and stands on its build"
        if 1 not in exec_map:
            return False, "execution 0 holds a watchdog halt and its rerun has not started"
        if not (exec_map[1] / "execution.json").exists():
            return True, "execution 1 left no execution.json"
        e1 = _plan_receipt(exec_map[1], man, 1)
        ended1 = e1["receipt"]["ended"]
        expired1 = _expired(e1["records"])
        if ended1 == "complete" and not expired1:
            return False, "set completed on watchdog rerun and stands on its build"
        if ended1 == "interrupted":
            return True, "execution 1 ended interrupted"
        if ended1 == "watchdog_stop":
            if any(g in expired0 for g in expired1):
                return True, "same-game watchdog hang"
            return False, "different-game watchdog expiry is a void infrastructure failure, not restartable"
    return False, f"set execution state {ended0!r} is not a restartable interruption"


def _set_base_and_k(set_name: str) -> tuple[str, int]:
    m = read_plans.RESTART_SUFFIX_RE.fullmatch(set_name)
    if m is None:
        return set_name, 0
    return m.group(1), int(m.group(2))


def _is_d8_completed_set(set_dir: Path, man: dict[str, Any]) -> bool:
    """True iff a D8 set completed cleanly and stands on its build."""
    interrupted, reason = is_d8_interrupted_set(set_dir, man)
    if interrupted:
        return False
    return "completed" in reason


def verify_d8_v3_against_root(root: Path, man: dict[str, Any], *,
                              shakedown_manifest: tuple[Path | bytes, str] | None = None) -> None:
    """Enforce READ-PLAN-v3 Change 1 (Q2.1, Q2.2, New 1, New 2) against existing sets under
    `root/<namespace>/` and the D0.3b-pinned shakedown manifest."""
    ns = man["namespace"]
    if ns not in read_plans.D8_RESTART_NAMESPACES:
        return
    try:
        read_plans.check_d8_v3_manifest_shape(man)
    except read_plans.PlanError as exc:
        raise SetError(str(exc)) from exc
    n = read_plans.run_number(man["run"])
    ns_dir = Path(root) / ns

    # Every D8 restart run (v2+) must be explicitly authorized in the D0.3b-pinned shakedown manifest.
    restart_auth: dict[str, Any] | None = None
    if n >= 2:
        if shakedown_manifest is None:
            raise SetError(
                f"{ns}/{man['run']}/{man['set']}: D8 restart run {man['run']} requires a "
                "D0.3b-pinned shakedown manifest authorizing the restart"
            )
        sm = load_shakedown_manifest(shakedown_manifest[0], shakedown_manifest[1])
        restart_auth = sm["restarts"].get((ns, man["run"]))
        if restart_auth is None:
            raise SetError(
                f"{ns}/{man['run']}/{man['set']}: D0.3b shakedown manifest does not authorize "
                f"restart run {ns}/{man['run']}"
            )
        if man["replaces"] is not None:
            if restart_auth["replaces"].get(man["set"]) != man["replaces"]:
                raise SetError(
                    f"{ns}/{man['run']}/{man['set']}: D0.3b shakedown manifest does not authorize "
                    f"replacing {man['replaces']!r} with {man['set']!r} in {ns}/{man['run']}"
                )

    earlier_hwm = 0
    earlier_sets: list[tuple[int, Path, dict[str, Any]]] = []
    all_other_sets: list[tuple[int, Path, dict[str, Any]]] = []
    if ns_dir.is_dir():
        for run_dir in sorted(ns_dir.iterdir()):
            rm = read_plans.RUN_RE.fullmatch(run_dir.name)
            if not rm or not run_dir.is_dir():
                continue
            r_num = int(rm.group(1))
            for s_dir in sorted(run_dir.iterdir()):
                if not (s_dir / "manifest.json").exists():
                    continue
                s_man = load_manifest(s_dir)
                if s_man["run"] == man["run"] and s_man["set"] == man["set"]:
                    continue
                all_other_sets.append((r_num, s_dir, s_man))
                if r_num < n:
                    earlier_sets.append((r_num, s_dir, s_man))
                    if s_man["reserved"] is not None:
                        earlier_hwm = max(earlier_hwm, s_man["reserved"][1])
    if man["units"][0] < earlier_hwm:
        raise SetError(
            f"{ns}/{man['run']}/{man['set']}: units start at {man['units'][0]}, below namespace high-water mark {earlier_hwm}"
        )
    if ns == "read-d8-aa" and n >= 2:
        if earlier_hwm == 0 or man["units"][0] != earlier_hwm:
            raise SetError(
                f"{ns}/{man['run']}/{man['set']}: read-d8-aa restart must start at namespace high-water mark {earlier_hwm}, "
                f"got {man['units'][0]}"
            )
    if man["replaces"] is None:
        if ns == "dose-d8" and n >= 2 and man["set"] != "prep-panel":
            base_name, _ = _set_base_and_k(man["set"])
            is_full_restart = restart_auth is not None and restart_auth.get("restart_kind") == "full_shakedown_restart"
            for _, prev_dir, prev_man in earlier_sets:
                prev_base, _ = _set_base_and_k(prev_man["set"])
                if prev_man["kind"] == man["kind"] and prev_base == base_name:
                    interrupted, _ = is_d8_interrupted_set(prev_dir, prev_man)
                    if interrupted:
                        raise SetError(
                            f"{ns}/{man['run']}/{man['set']}: earlier set {prev_man['run']}/{prev_man['set']} was interrupted, "
                            f"so this ordinal must use a -restart-<k> name and replaces"
                        )
                    if man["kind"] == "plans" and _is_d8_completed_set(prev_dir, prev_man) and not is_full_restart:
                        raise SetError(
                            f"{ns}/{man['run']}/{man['set']}: earlier set {prev_man['run']}/{prev_man['set']} already completed "
                            f"on this build and cannot be replayed under a fresh name"
                        )
                if (man["kind"] == "prep" and base_name.startswith("prep-dose-")
                        and prev_man["kind"] == "plans" and prev_base == base_name[len("prep-"):]):
                    if _is_d8_completed_set(prev_dir, prev_man) and not is_full_restart:
                        raise SetError(
                            f"{ns}/{man['run']}/{man['set']}: earlier plan set {prev_man['run']}/{prev_man['set']} already "
                            f"completed on this build; cannot replay its prep under a fresh name"
                        )
        return

    # No forked restart histories: each predecessor may be replaced at most once across the namespace.
    for _, _, other_man in all_other_sets:
        if other_man["replaces"] == man["replaces"]:
            raise SetError(
                f"{ns}/{man['run']}/{man['set']}: predecessor {man['replaces']!r} is already replaced by "
                f"{other_man['run']}/{other_man['set']} (forked restart history)"
            )

    pred_run, pred_set = man["replaces"].split("/", 1)
    pred_dir = ns_dir / pred_run / pred_set
    if not (pred_dir / "manifest.json").exists():
        raise SetError(f"{ns}/{man['run']}/{man['set']}: replaced set {man['replaces']!r} does not exist")
    pred_man = load_manifest(pred_dir)
    if (pred_man["namespace"], pred_man["family"], pred_man["kind"]) != (ns, man["family"], man["kind"]):
        raise SetError(f"{ns}/{man['run']}/{man['set']}: replaced set {man['replaces']!r} differs in family or kind")
    cur_base, cur_k = _set_base_and_k(man["set"])
    pred_base, pred_k = _set_base_and_k(pred_man["set"])
    if cur_base != pred_base:
        raise SetError(
            f"{ns}/{man['run']}/{man['set']}: base set {cur_base!r} does not match predecessor {pred_man['set']!r}"
        )
    if cur_k != pred_k + 1:
        raise SetError(
            f"{ns}/{man['run']}/{man['set']}: restart index k = {cur_k} must follow predecessor k = {pred_k}"
        )
    # Require replacing the latest earlier instance of (kind, cur_base) across all earlier runs.
    same_base_earlier = [
        (r_num, _set_base_and_k(s_man["set"])[1], s_man)
        for r_num, _, s_man in earlier_sets
        if s_man["kind"] == man["kind"] and _set_base_and_k(s_man["set"])[0] == cur_base
    ]
    if same_base_earlier:
        _, _, latest_man = max(same_base_earlier, key=lambda item: (item[0], item[1]))
        latest_ref = f"{latest_man['run']}/{latest_man['set']}"
        if man["replaces"] != latest_ref:
            raise SetError(
                f"{ns}/{man['run']}/{man['set']}: must replace latest predecessor {latest_ref!r}, not {man['replaces']!r}"
            )
    interrupted, reason = is_d8_interrupted_set(pred_dir, pred_man)
    if not interrupted:
        raise SetError(
            f"{ns}/{man['run']}/{man['set']}: replaced set {man['replaces']!r} is not an interrupted set ({reason})"
        )
    want_retired = d8_unused_reserved_ranges(pred_dir, pred_man)
    if man["retired"] != want_retired:
        raise SetError(
            f"{ns}/{man['run']}/{man['set']}: retired {man['retired']} differs from unused reserved ranges "
            f"{want_retired} of {man['replaces']}"
        )


RESTART_KINDS = ("dose_only_same_build", "full_shakedown_restart", "aa_restart")


def load_shakedown_manifest(manifest: Path | bytes, expected_sha256: str) -> dict[str, Any]:
    """Q2.4 & New 2: Verify the shakedown manifest (D0.3b) against its pinned sha256 BEFORE trusting
    any panel digest or restart authorization inside it. Returns:
      {"panels": {run: panel_sha256}, "restarts": {(ns, run): {"restart_kind": ..., "replaces": {set: pred}}}}."""
    if not read_plans._is_sha(expected_sha256):
        raise SetError("shakedown manifest D0.3b pin must be a 64-char hex sha256")
    raw = manifest if isinstance(manifest, (bytes, bytearray)) else Path(manifest).read_bytes()
    actual_sha = hashlib.sha256(raw).hexdigest()
    if actual_sha != expected_sha256:
        raise SetError(
            f"shakedown manifest sha256 {actual_sha} differs from D0.3b pin {expected_sha256}"
        )
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SetError(f"shakedown manifest is not valid UTF-8: {exc}") from exc
    panels: dict[str, str] = {}
    restarts: dict[tuple[str, str], dict[str, Any]] = {}

    def record_panel(run: str, digest: str) -> None:
        try:
            read_plans.check_run("dose-d8", run)
        except read_plans.PlanError as exc:
            raise SetError(f"shakedown manifest invalid dose-d8 run {run!r}: {exc}") from exc
        if not read_plans._is_sha(digest):
            raise SetError(f"shakedown manifest invalid panel_sha256 for {run}: {digest!r}")
        if run in panels and panels[run] != digest:
            raise SetError(f"shakedown manifest records conflicting panel digests for dose-d8 {run}")
        panels[run] = digest

    def record_restart(ns: str, run: str, restart_kind: str | None = None,
                       replaces_map: dict[str, str] | None = None) -> None:
        if ns not in read_plans.D8_RESTART_NAMESPACES:
            raise SetError(f"shakedown manifest invalid restart namespace {ns!r}")
        try:
            read_plans.check_run(ns, run)
        except read_plans.PlanError as exc:
            raise SetError(f"shakedown manifest invalid run {run!r} for {ns}: {exc}") from exc
        if read_plans.run_number(run) < 2:
            raise SetError(f"shakedown manifest restart run must be v2 or later, got {run!r}")
        default_kind = "aa_restart" if ns == "read-d8-aa" else "dose_only_same_build"
        kind = default_kind if restart_kind is None else restart_kind
        if kind not in RESTART_KINDS:
            raise SetError(f"shakedown manifest unknown restart_kind {kind!r}")
        key = (ns, run)
        slot = restarts.setdefault(key, {"restart_kind": kind, "replaces": {}})
        if restart_kind is not None and slot["restart_kind"] != kind:
            raise SetError(f"shakedown manifest conflicting restart_kind for {ns}/{run}")
        for new_set, pred_ref in (replaces_map or {}).items():
            if not isinstance(new_set, str) or not isinstance(pred_ref, str) or "/" not in pred_ref:
                raise SetError(f"shakedown manifest invalid replaces entry {new_set!r}: {pred_ref!r}")
            if new_set in slot["replaces"] and slot["replaces"][new_set] != pred_ref:
                raise SetError(f"shakedown manifest conflicting replaces for {ns}/{run}/{new_set}")
            slot["replaces"][new_set] = pred_ref

    stripped = text.lstrip()
    if stripped.startswith("{"):
        try:
            doc = json.loads(text)
        except json.JSONDecodeError as exc:
            raise SetError(f"shakedown manifest JSON error: {exc}") from exc
        if not isinstance(doc, dict):
            raise SetError("shakedown manifest JSON must be an object")
        if isinstance(doc.get("dose_panels"), dict):
            for run, digest in doc["dose_panels"].items():
                record_panel(run, digest)
        raw_restarts = doc.get("restarts")
        if isinstance(raw_restarts, list):
            for entry in raw_restarts:
                if not isinstance(entry, dict):
                    raise SetError("shakedown manifest restarts entries must be objects")
                rep = entry.get("replaces")
                rep_map = dict(rep) if isinstance(rep, dict) else {}
                record_restart(entry["namespace"], entry["run"], entry.get("restart_kind"), rep_map)
        elif isinstance(raw_restarts, dict):
            for k, entry in raw_restarts.items():
                if not isinstance(entry, dict):
                    raise SetError("shakedown manifest restarts values must be objects")
                ns = entry.get("namespace") or (k.split("/", 1)[0] if "/" in k else "dose-d8")
                run = entry.get("run") or (k.split("/", 1)[1] if "/" in k else k)
                rep = entry.get("replaces")
                rep_map = dict(rep) if isinstance(rep, dict) else {}
                record_restart(ns, run, entry.get("restart_kind"), rep_map)
        for entry in doc.get("entries", []):
            if not isinstance(entry, dict):
                continue
            ns = entry.get("namespace", "dose-d8")
            run = entry.get("run")
            if ns == "dose-d8" and "panel_sha256" in entry and run is not None:
                record_panel(run, entry["panel_sha256"])
            if entry.get("kind") == "restart" and run is not None:
                rep = entry.get("replaces")
                if isinstance(rep, dict):
                    rep_map = dict(rep)
                elif isinstance(rep, list):
                    rep_map = {item["set"]: item["replaces"] for item in rep if isinstance(item, dict)}
                else:
                    rep_map = {}
                record_restart(ns, run, entry.get("restart_kind"), rep_map)
    else:
        # Markdown SHAKEDOWN-MANIFEST.md format (D0.3b entries 3 and 8).
        for block in re.split(r"(?m)^(?=#{1,6}\s)", text):
            ns_found = [ns for ns in read_plans.D8_RESTART_NAMESPACES if ns in block]
            if not ns_found:
                continue
            rm = re.search(r"\brun\s*[:=]?\s*`?(v[1-9]\d*)`?", block, re.IGNORECASE)
            if not rm:
                continue
            run = rm.group(1)
            if "dose-d8" in ns_found:
                pm = re.search(r"panel(?:\.json|_sha256|\s+digest)(?:\s+sha256)?[\s:=`]*([0-9a-f]{64})", block, re.IGNORECASE)
                if pm:
                    record_panel(run, pm.group(1))
            if read_plans.run_number(run) >= 2 and re.search(r"\brestart\b|\breplaces\b", block, re.IGNORECASE):
                ns = ns_found[0]
                km = re.search(r"\b(full_shakedown_restart|dose_only_same_build|aa_restart)\b", block)
                rkind = km.group(1) if km else None
                rep_map: dict[str, str] = {}
                for m in re.finditer(
                    r"`?([a-z0-9-]+-restart-[1-9]\d*)`?\s*(?:replaces|->|=)\s*`?(v[1-9]\d*/[a-z0-9-]+)`?",
                    block,
                ):
                    rep_map[m.group(1)] = m.group(2)
                record_restart(ns, run, rkind, rep_map)
    if not panels and not restarts:
        raise SetError("shakedown manifest records no dose-d8 panel digest or restart authorization")
    return {"panels": panels, "restarts": restarts}


def load_shakedown_manifest_panels(manifest: Path | bytes, expected_sha256: str) -> dict[str, str]:
    """Q2.4: Verify the shakedown manifest (D0.3b) against its pinned sha256 BEFORE trusting any
    panel digest inside it, then return {run: panel_sha256} for dose-d8."""
    panels = load_shakedown_manifest(manifest, expected_sha256)["panels"]
    if not panels:
        raise SetError("shakedown manifest records no dose-d8 panel digest")
    return panels


def resolve_pinned_panel(man: dict[str, Any], pinned_panel: str | None,
                         shakedown_manifest: tuple[Path | bytes, str] | None) -> str:
    """Resolve the pinned panel digest for a dose or d13 set (Q2.4)."""
    run = man["run"]
    if shakedown_manifest is not None:
        panels = load_shakedown_manifest_panels(shakedown_manifest[0], shakedown_manifest[1])
        if run not in panels:
            raise SetError(f"shakedown manifest has no pinned panel digest for {man['namespace']} run {run}")
        resolved = panels[run]
        if pinned_panel is not None and run == read_plans.RUN and pinned_panel != resolved:
            raise SetError("declaration panel_sha256 disagrees with the D0.3b shakedown manifest for v1")
        return resolved
    if man["namespace"] == "dose-d8" and run != read_plans.RUN:
        raise SetError(
            f"dose-d8 restart run {run} requires a D0.3b-pinned shakedown manifest for its panel digest"
        )
    if pinned_panel is None:
        raise SetError("a dose or d13 set requires a pinned panel digest")
    return pinned_panel


def verify_set_restarts(opened_set: dict[str, Any], *, n_b: int | None = None, n_c: int | None = None,
                        shakedown_manifest: tuple[Path | bytes, str] | None = None) -> None:
    """Verify restart and shared-prefix restart rules from the reader entry point (A6.7, section 7, v3)."""
    man = opened_set["manifest"]
    set_dir = Path(opened_set["set_dir"])
    root = set_dir.parents[2]
    if man["namespace"] in read_plans.D8_RESTART_NAMESPACES:
        verify_d8_v3_against_root(root, man, shakedown_manifest=shakedown_manifest)
        if man["prep_set"] is not None:
            prep_path, _, _ = man["prep_set"].rpartition("/exec-")
            prep_dir = root / prep_path
            prep_man = load_manifest(prep_dir)
            verify_d8_v3_against_root(root, prep_man, shakedown_manifest=shakedown_manifest)
        return
    if man["replaces"] is not None:
        pred_dir = set_dir.parent / man["replaces"]
        if not (pred_dir / "manifest.json").exists():
            raise SetError(f"replaced set {man['replaces']!r} does not exist at {pred_dir}")
        pred_man = load_manifest(pred_dir)
        check_restart(pred_man, man)
    if man["prep_set"] is not None:
        prep_path, _, _ = man["prep_set"].rpartition("/exec-")
        prep_dir = root / prep_path
        prep_man = load_manifest(prep_dir)
        if prep_man["family"] == "b_c" and prep_man["replaces"] is not None:
            if n_b is None or n_c is None:
                raise SetError("shared-prefix restart check requires both n_b and n_c")
            chain: list[dict[str, Any]] = []
            cur = prep_man
            seen = {cur["set"]}
            while cur["replaces"] is not None:
                prev_name = cur["replaces"]
                if prev_name in seen:
                    raise SetError(f"cycle in b_c prep replaces chain at {prev_name!r}")
                seen.add(prev_name)
                prev_dir = prep_dir.parent / prev_name
                if not (prev_dir / "manifest.json").exists():
                    raise SetError(f"replaced b_c prep set {prev_name!r} does not exist")
                cur = load_manifest(prev_dir)
                chain.append(cur)
            chain.reverse()
            # Scan all sibling b_c prep manifests in the same run directory so any competing
            # earlier b_c reservation (even outside this replaces chain) advances f'.
            siblings: dict[str, dict[str, Any]] = {}
            for s_dir in sorted(prep_dir.parent.iterdir()):
                if not (s_dir / "manifest.json").exists():
                    continue
                s_man = load_manifest(s_dir)
                if s_man["kind"] == "prep" and s_man["family"] == "b_c":
                    siblings[s_man["set"]] = s_man
            # Exclude prep_man itself and any later descendants whose replaces chain leads to prep_man.
            descendants = {prep_man["set"]}
            changed = True
            while changed:
                changed = False
                for name, s_man in siblings.items():
                    if name not in descendants and s_man["replaces"] in descendants:
                        descendants.add(name)
                        changed = True
            chain_names = {m["set"] for m in chain}
            extra_prior = [
                s_man for name, s_man in sorted(siblings.items())
                if name not in descendants and name not in chain_names
            ]
            # Place direct predecessor (chain[-1]) last so check_shared_prefix_restart checks replaces against it.
            prior_bc = extra_prior + chain
            check_shared_prefix_restart(prior_bc, prep_man, n_b, n_c)
            if man["look"] == 1 and man["units"][0] != prep_man["units"][0]:
                raise SetError(
                    f"restarted look-1 set starts at unit {man['units'][0]}, expected shared prefix f' = {prep_man['units'][0]}"
                )


# ---------------------------------------------------------------------------------------------
# Frozen panels (v1 section 5, v2 change 2, v3 change 2)


def panel_set_names(namespace: str, set_name: str) -> tuple[str, str]:
    """(prep-panel set, panel set) of a dose or d13 set; in `test` they hang from the fixture name."""
    if namespace == read_plans.TEST:
        base = read_plans.fixture_base(set_name)
        return f"{base}-prep-panel", f"{base}-panel"
    return "prep-panel", "panel"


def verify_panel(root: Path, namespace: str, pinned: str, *, pool_doc: Any, constants_doc: Any,
                 pool_sha256: str, constants_sha256: str, set_name: str = "", run: str = read_plans.RUN,
                 card_index: tuple[Any, str] | None = None, cube_table: tuple[Any, str] | None = None,
                 shakedown_manifest: tuple[Path | bytes, str] | None = None) -> dict[str, Any]:
    """Rebuild panel.json from the accepted prep-panel execution, require identical bytes and the
    pinned digest. Returns the panel. The stage-1 prep-panel set itself is never checked against a
    pinned panel (read plan v2 change 2)."""
    try:
        read_plans.check_run(namespace, run)
    except read_plans.PlanError as exc:
        raise SetError(str(exc)) from exc
    if namespace != read_plans.TEST and (card_index is None or cube_table is None):
        raise SetError(f"live panel verification in {namespace} requires pinned card_index and cube_table")
    prep_name, panel_name = panel_set_names(namespace, set_name)
    panel_path = Path(root) / namespace / run / panel_name / "panel.json"
    if not panel_path.exists():
        raise SetError(f"panel.json is missing at {namespace}/{run}/{panel_name}")
    if _sha_file(panel_path) != pinned:
        raise SetError("panel.json does not hash to the pinned panel digest")
    panel = _load(panel_path)
    if panel.get("run") != run:
        raise SetError(f"panel.json run {panel.get('run')!r} differs from expected run {run!r}")
    prep_dir = Path(root) / namespace / run / prep_name
    prep_man = load_manifest(prep_dir)
    if prep_man["run"] != run:
        raise SetError(f"prep-panel run {prep_man['run']!r} differs from expected run {run!r}")
    if prep_man["panel_sha256"] is not None:
        raise SetError("the stage-1 prep-panel set runs with panel_sha256 null")
    if namespace in read_plans.D8_RESTART_NAMESPACES:
        verify_d8_v3_against_root(Path(root), prep_man, shakedown_manifest=shakedown_manifest)
    accepted = accept_prep_set(prep_dir, prep_man)
    family = prep_man["family"]
    builders = None
    if family == "d13":
        first = panel["drafts"][0]["built"]
        builders = {d["row"]: d["builder"] for d in first if d["row"] != "stated"}
    rebuilt = read_plans.assemble_panel(family, namespace, prep_man, accepted["execution"], accepted["outputs"],
                                        pins=panel["pins"], builders=builders, pool_doc=pool_doc,
                                        constants_doc=constants_doc, pool_sha256=pool_sha256,
                                        constants_sha256=constants_sha256, card_index=card_index,
                                        cube_table=cube_table)
    if read_plans.serialize(rebuilt).encode("utf-8") != panel_path.read_bytes():
        raise SetError("panel.json differs from its rebuild from the accepted prep-panel execution")
    return panel


# ---------------------------------------------------------------------------------------------
# Opening a plan set for reading


def check_prep_dose_set(set_dir: Path, pinned_panel: str | None = None, *, pool: tuple[Any, str],
                        constants: tuple[Any, str],
                        card_index: tuple[Any, str] | None = None,
                        cube_table: tuple[Any, str] | None = None,
                        shakedown_manifest: tuple[Path | bytes, str] | None = None) -> dict[str, Any]:
    """A downstream prep-dose-<ordinal> set: it carries the pinned panel digest, and each accepted
    result's draft pools equal the panel's. (prep-panel is exempt: read plan v2 change 2.)"""
    set_dir = Path(set_dir)
    man = load_manifest(set_dir)
    root = set_dir.parents[2]
    if man["namespace"] in read_plans.D8_RESTART_NAMESPACES:
        verify_d8_v3_against_root(root, man, shakedown_manifest=shakedown_manifest)
    resolved_panel = resolve_pinned_panel(man, pinned_panel, shakedown_manifest)
    if man["panel_sha256"] != resolved_panel:
        raise SetError("a prep-dose set carries the pinned panel digest")
    panel = verify_panel(root, man["namespace"], resolved_panel, pool_doc=pool[0],
                         constants_doc=constants[0], pool_sha256=pool[1], constants_sha256=constants[1],
                         set_name=man["set"], run=man["run"],
                         card_index=card_index, cube_table=cube_table,
                         shakedown_manifest=shakedown_manifest)
    accepted = accept_prep_set(set_dir, man)
    by_unit = {d["unit"]: d for d in panel["drafts"]}
    for unit, (result, _) in accepted["outputs"].items():
        if unit not in by_unit or result["draft"]["pools"] != by_unit[unit]["pools"]:
            raise SetError(f"prep-dose unit {unit}'s pools differ from the panel's")
    return accepted


TIMING_KEYS = ("game", "attempt", "elapsed_ms", "host")


def load_timing_side_files(set_dir: Path, execution: int, records: dict[int, dict[str, Any]]
                           ) -> dict[int, dict[int, dict[str, Any]]] | None:
    """Load and validate exec-<e>/timing/pod-<pod_index>.jsonl side files (READ-PLAN-v1 section 5).
    Returns None when the timing directory is absent; raises SetError on malformed or incomplete files."""
    timing_dir = Path(set_dir) / f"exec-{execution}" / "timing"
    if not timing_dir.exists():
        return None
    if not timing_dir.is_dir():
        raise SetError(f"{timing_dir} is not a directory")
    out: dict[int, dict[int, dict[str, Any]]] = {}
    for pod_index, rec in sorted(records.items()):
        path = timing_dir / f"pod-{pod_index}.jsonl"
        if not path.exists():
            raise SetError(f"missing timing side file {path.name}")
        per_game: dict[int, dict[str, Any]] = {}
        try:
            lines = path.read_bytes().decode("utf-8").splitlines()
        except (OSError, UnicodeDecodeError) as exc:
            raise SetError(f"timing/{path.name} is unreadable: {exc}") from exc
        for lineno, raw in enumerate(lines, 1):
            if not raw.strip():
                continue
            try:
                entry = json.loads(raw)
            except json.JSONDecodeError as exc:
                raise SetError(f"timing/{path.name}:{lineno} is not valid JSON: {exc}") from exc
            if not isinstance(entry, dict) or tuple(entry.keys()) != TIMING_KEYS:
                raise SetError(f"timing/{path.name}:{lineno} must have keys {TIMING_KEYS}")
            game = entry["game"]
            attempt = entry["attempt"]
            elapsed_ms = entry["elapsed_ms"]
            host = entry["host"]
            if type(game) is not int or not (0 <= game <= pod_record.U32):
                raise SetError(f"timing/{path.name}:{lineno}: game must be a u32")
            if type(attempt) is not int or attempt != 0:
                raise SetError(f"timing/{path.name}:{lineno}: read attempt must be 0")
            if type(elapsed_ms) is not int or not (0 <= elapsed_ms <= pod_record.U64):
                raise SetError(f"timing/{path.name}:{lineno}: elapsed_ms must be a u64")
            if not isinstance(host, str) or not host:
                raise SetError(f"timing/{path.name}:{lineno}: host must be a non-empty string")
            if game in per_game:
                raise SetError(f"timing/{path.name}:{lineno}: duplicate timing entry for game {game}")
            per_game[game] = entry
        expected_games = {g["game"] for g in rec["games"]}
        if set(per_game) != expected_games:
            raise SetError(
                f"timing/{path.name} covers games {sorted(per_game)}, expected {sorted(expected_games)}"
            )
        out[pod_index] = per_game
    return out


def open_plan_set(set_dir: Path, declaration_sha: str, *, pool: tuple[Any, str] | None = None,
                  constants: tuple[Any, str] | None = None, pinned_panel: str | None = None,
                  card_index: tuple[Any, str] | None = None, cube_table: tuple[Any, str] | None = None,
                  expected_pins: Any = None, expected_pilot: Any = None, expected_picker: Any = None,
                  expected_builders: dict[str, Any] | None = None,
                  shakedown_manifest: tuple[Path | bytes, str] | None = None) -> dict[str, Any]:
    """Check a plan set and its chosen execution before data: manifest, plans (regenerated), prep
    acceptance, mirrors, panel, then each record against its plan."""
    set_dir = Path(set_dir)
    man = load_manifest(set_dir)
    if man["kind"] != "plans":
        raise SetError("not a plan set")
    if man["declaration_sha256"] != declaration_sha:
        raise SetError("the manifest's declaration_sha256 is not the declaration read")
    root = set_dir.parents[2]
    family = man["family"]
    if read_plans.FAMILIES[family]["layout"] in ("c", "d13") and man["namespace"] != read_plans.TEST:
        if card_index is None or cube_table is None:
            raise SetError(
                f"live common-opponent set {man['namespace']}/{man['run']}/{man['set']} requires "
                "pinned card_index and cube_table"
            )
    if man["namespace"] in read_plans.D8_RESTART_NAMESPACES:
        verify_d8_v3_against_root(root, man, shakedown_manifest=shakedown_manifest)
    plans = load_entries(set_dir, man)
    outputs: dict[str, tuple[dict[str, Any], str]] = {}
    requests_by_sha: dict[str, dict[str, Any]] = {}
    if read_plans.FAMILIES[family].get("prep"):
        if man["prep_set"] is None:
            raise SetError("a plan set with prep names its accepted prep execution")
        prep_path, _, exec_name = man["prep_set"].rpartition("/exec-")
        prep_dir = root / prep_path
        prep_man = load_manifest(prep_dir)
        if man["namespace"] in read_plans.D8_RESTART_NAMESPACES:
            verify_d8_v3_against_root(root, prep_man, shakedown_manifest=shakedown_manifest)
            if prep_man["run"] != man["run"]:
                raise SetError(f"prep_set run {prep_man['run']!r} differs from plan set run {man['run']!r}")
        accepted = accept_prep_set(
            prep_dir, prep_man, expected_pins=expected_pins, expected_picker=expected_picker,
            expected_builders=expected_builders if family != "b" else None,
        )
        if str(accepted["execution"]) != exec_name:
            raise SetError(f"prep_set names exec-{exec_name}, the accepted execution is {accepted['execution']}")
        outputs = {sha: (doc, sha) for doc, sha in accepted["outputs"].values()}
        requests_by_sha = {sha: accepted["requests"][u][0] for u, (_, sha) in accepted["outputs"].items()}
    elif man["prep_set"] is not None:
        raise SetError("this family has no prep")
    panel = None
    if family in ("dose", "d13"):
        resolved_panel = resolve_pinned_panel(man, pinned_panel, shakedown_manifest)
        if man["panel_sha256"] is None or man["panel_sha256"] != resolved_panel:
            raise SetError("a dose or d13 set carries the pinned panel digest")
        panel = verify_panel(root, man["namespace"], resolved_panel, pool_doc=pool[0], constants_doc=constants[0],
                             pool_sha256=pool[1], constants_sha256=constants[1], set_name=man["set"],
                             run=man["run"], card_index=card_index, cube_table=cube_table,
                             shakedown_manifest=shakedown_manifest)
    elif man["panel_sha256"] is not None:
        raise SetError("only dose and d13 sets carry a panel digest")
    first, end = man["units"]
    checked: dict[int, dict[str, Any]] = {}
    for index, (plan, _) in plans.items():
        prep = outputs.get(plan.get("prep_sha256")) if isinstance(plan, dict) else None
        prep_req = requests_by_sha.get(plan.get("prep_sha256")) if isinstance(plan, dict) else None
        if read_plans.FAMILIES[family].get("prep") and prep is None:
            raise SetError(f"plan {index}'s prep_sha256 is not an output of the accepted prep execution")
        try:
            read_plans.check_plan(plan, prep, pool[0] if pool else None, constants[0] if constants else None,
                                  pool[1] if pool else None, constants[1] if constants else None,
                                  card_index=card_index, cube_table=cube_table, prep_request=prep_req,
                                  expected_pins=expected_pins, expected_pilot=expected_pilot,
                                  expected_picker=expected_picker, expected_builders=expected_builders)
        except (read_plans.PlanError, pod_record.RecordError) as exc:
            raise SetError(f"plan {index}: {exc}") from exc
        if (plan["family"] != family or plan["namespace"] != man["namespace"]
                or plan["run"] != man["run"] or plan["pod_index"] != index):
            raise SetError(f"plan {index} is not this set's family, namespace, run and pod index")
        if not first <= plan["unit"] < end:
            raise SetError(f"plan {index}'s unit lies outside the manifest's units")
        if panel is not None:
            read_plans.check_plan_against_panel(plan, panel)
        checked[index] = plan
    _set_bindings(checked)
    layout = read_plans.FAMILIES[family]["layout"]
    per_unit = {"a": 2, "one": 8}.get(layout, 1)
    if len(checked) != per_unit * (end - first):
        raise SetError(f"the set has {len(checked)} plans for {end - first} units")
    if layout == "a":
        for unit in range(first, end):
            read_plans.check_mirrors(checked[2 * unit], checked[2 * unit + 1])
    chosen = choose_plan_execution(set_dir, man)
    for index, record in chosen["records"].items():
        read_plans.check_record_against_plan(record, checked[index])
    if family == "aa" and chosen["void"] is None:
        for unit in range(first, end):
            a, b = chosen["records"].get(2 * unit), chosen["records"].get(2 * unit + 1)
            if a and b and (a["draft"]["pools"] != b["draft"]["pools"]
                            or [d["cards"] for d in a["decks"]] != [d["cards"] for d in b["decks"]]):
                raise SetError(f"A/A pair {unit}: mirrors differ in pools or decks (A6.12)")
    timing = (
        load_timing_side_files(set_dir, chosen["execution"], chosen["records"])
        if chosen["execution"] is not None else None
    )
    basic_ids = (
        sorted(deal.validate_pool(pool[0])["basic_id_by_color"].values())
        if pool is not None else None
    )
    return {"manifest": man, "plans": checked, "execution": chosen["execution"], "records": chosen["records"],
            "expired": chosen["expired"], "void": chosen["void"], "set_dir": set_dir,
            "timing": timing, "basic_ids": basic_ids,
            "checked_card_index": card_index is not None, "checked_cube_table": cube_table is not None,
            "_dealer": (pool, constants, card_index, cube_table)}
