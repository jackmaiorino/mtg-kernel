"""Read sets: manifests, executions, prep acceptance, look allocation, restarts and frozen panels.

Implements, for the readers (D7), read plan v2: v1 sections 5 and 7 with the v2 delta.

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
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

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
        read_plans.check_set_name(man["namespace"], man["family"], man["set"])
    except read_plans.PlanError as exc:
        raise SetError(str(exc)) from exc
    _range(man["units"], "manifest.units")
    if man["reserved"] is not None:
        _range(man["reserved"], "manifest.reserved")
    for r in man["retired"]:
        _range(r, "manifest.retired entry")
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


def accept_prep_set(set_dir: Path, man: dict[str, Any] | None = None) -> dict[str, Any]:
    """The accepted execution of a prep set and its outputs {unit: (result, sha256)}."""
    set_dir = Path(set_dir)
    man = man or load_manifest(set_dir)
    if man["kind"] != "prep":
        raise SetError("not a prep set")
    requests = load_entries(set_dir, man)
    for unit, (request, _) in requests.items():
        try:
            read_plans.check_prep_request(request)
        except (read_plans.PlanError, pod_record.RecordError) as exc:
            raise SetError(f"request of unit {unit}: {exc}") from exc
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
            return {"execution": e, "outputs": outputs, "manifest": man}
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
    f_new = max(m["reserved"][1] for m in prior_bc)
    if new_bc["reserved"] != [f_new, f_new + 3 * max(n_b, n_c)] or new_bc["units"][0] != f_new:
        raise SetError(f"the shared-prefix restart starts at f' = {f_new} and reserves 3 max(n_b, n_c)")
    check_restart(prior_bc[-1], new_bc)


# ---------------------------------------------------------------------------------------------
# Frozen panels (v1 section 5, v2 change 2)


def panel_set_names(namespace: str, set_name: str) -> tuple[str, str]:
    """(prep-panel set, panel set) of a dose or d13 set; in `test` they hang from the fixture name."""
    if namespace == read_plans.TEST:
        base = read_plans.fixture_base(set_name)
        return f"{base}-prep-panel", f"{base}-panel"
    return "prep-panel", "panel"


def verify_panel(root: Path, namespace: str, pinned: str, *, pool_doc: Any, constants_doc: Any,
                 pool_sha256: str, constants_sha256: str, set_name: str = "") -> dict[str, Any]:
    """Rebuild panel.json from the accepted prep-panel execution, require identical bytes and the
    pinned digest. Returns the panel. The stage-1 prep-panel set itself is never checked against a
    pinned panel (read plan v2 change 2)."""
    prep_name, panel_name = panel_set_names(namespace, set_name)
    panel_path = Path(root) / namespace / read_plans.RUN / panel_name / "panel.json"
    if not panel_path.exists():
        raise SetError("panel.json is missing")
    if _sha_file(panel_path) != pinned:
        raise SetError("panel.json does not hash to the pinned panel digest")
    panel = _load(panel_path)
    prep_dir = Path(root) / namespace / read_plans.RUN / prep_name
    prep_man = load_manifest(prep_dir)
    if prep_man["panel_sha256"] is not None:
        raise SetError("the stage-1 prep-panel set runs with panel_sha256 null")
    accepted = accept_prep_set(prep_dir, prep_man)
    family = prep_man["family"]
    builders = None
    if family == "d13":
        first = panel["drafts"][0]["built"]
        builders = {d["row"]: d["builder"] for d in first if d["row"] != "stated"}
    rebuilt = read_plans.assemble_panel(family, namespace, prep_man, accepted["execution"], accepted["outputs"],
                                        pins=panel["pins"], builders=builders, pool_doc=pool_doc,
                                        constants_doc=constants_doc, pool_sha256=pool_sha256,
                                        constants_sha256=constants_sha256)
    if read_plans.serialize(rebuilt).encode("utf-8") != panel_path.read_bytes():
        raise SetError("panel.json differs from its rebuild from the accepted prep-panel execution")
    return panel


# ---------------------------------------------------------------------------------------------
# Opening a plan set for reading


def check_prep_dose_set(set_dir: Path, pinned_panel: str, *, pool: tuple[Any, str], constants: tuple[Any, str]
                        ) -> dict[str, Any]:
    """A downstream prep-dose-<ordinal> set: it carries the pinned panel digest, and each accepted
    result's draft pools equal the panel's. (prep-panel is exempt: read plan v2 change 2.)"""
    set_dir = Path(set_dir)
    man = load_manifest(set_dir)
    if man["panel_sha256"] != pinned_panel or pinned_panel is None:
        raise SetError("a prep-dose set carries the pinned panel digest")
    panel = verify_panel(set_dir.parents[2], man["namespace"], pinned_panel, pool_doc=pool[0],
                         constants_doc=constants[0], pool_sha256=pool[1], constants_sha256=constants[1],
                         set_name=man["set"])
    accepted = accept_prep_set(set_dir, man)
    by_unit = {d["unit"]: d for d in panel["drafts"]}
    for unit, (result, _) in accepted["outputs"].items():
        if unit not in by_unit or result["draft"]["pools"] != by_unit[unit]["pools"]:
            raise SetError(f"prep-dose unit {unit}'s pools differ from the panel's")
    return accepted


def open_plan_set(set_dir: Path, declaration_sha: str, *, pool: tuple[Any, str] | None = None,
                  constants: tuple[Any, str] | None = None, pinned_panel: str | None = None) -> dict[str, Any]:
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
    plans = load_entries(set_dir, man)
    outputs: dict[str, tuple[dict[str, Any], str]] = {}
    if read_plans.FAMILIES[family].get("prep"):
        if man["prep_set"] is None:
            raise SetError("a plan set with prep names its accepted prep execution")
        prep_path, _, exec_name = man["prep_set"].rpartition("/exec-")
        accepted = accept_prep_set(root / prep_path)
        if str(accepted["execution"]) != exec_name:
            raise SetError(f"prep_set names exec-{exec_name}, the accepted execution is {accepted['execution']}")
        outputs = {sha: (doc, sha) for doc, sha in accepted["outputs"].values()}
    elif man["prep_set"] is not None:
        raise SetError("this family has no prep")
    panel = None
    if family in ("dose", "d13"):
        if man["panel_sha256"] is None or man["panel_sha256"] != pinned_panel:
            raise SetError("a dose or d13 set carries the pinned panel digest")
        panel = verify_panel(root, man["namespace"], pinned_panel, pool_doc=pool[0], constants_doc=constants[0],
                             pool_sha256=pool[1], constants_sha256=constants[1], set_name=man["set"])
    elif man["panel_sha256"] is not None:
        raise SetError("only dose and d13 sets carry a panel digest")
    first, end = man["units"]
    checked: dict[int, dict[str, Any]] = {}
    for index, (plan, _) in plans.items():
        prep = outputs.get(plan.get("prep_sha256")) if isinstance(plan, dict) else None
        if read_plans.FAMILIES[family].get("prep") and prep is None:
            raise SetError(f"plan {index}'s prep_sha256 is not an output of the accepted prep execution")
        try:
            read_plans.check_plan(plan, prep, pool[0] if pool else None, constants[0] if constants else None,
                                  pool[1] if pool else None, constants[1] if constants else None)
        except (read_plans.PlanError, pod_record.RecordError) as exc:
            raise SetError(f"plan {index}: {exc}") from exc
        if plan["family"] != family or plan["namespace"] != man["namespace"] or plan["pod_index"] != index:
            raise SetError(f"plan {index} is not this set's family, namespace and pod index")
        if not first <= plan["unit"] < end:
            raise SetError(f"plan {index}'s unit lies outside the manifest's units")
        if panel is not None:
            read_plans.check_plan_against_panel(plan, panel)
        checked[index] = plan
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
    return {"manifest": man, "plans": checked, "execution": chosen["execution"], "records": chosen["records"],
            "expired": chosen["expired"], "void": chosen["void"], "set_dir": set_dir}
