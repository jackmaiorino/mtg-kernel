"""Fit readings and one-factor contrasts of the U collapse diagnostic (collab LANES/spy-u-collapse-20261014).

Each cell is arm U of the Spy archive distillation screen with one change
(phase1.py ``--cell``). Offline, from the probes and phase-1 receipts; no
game outcome is read.

Per lineage and cell, checkpoints are read in step order with the screen's
gates (fit.py G1, G2 and G3, and its parent and O-final references). The
reading stops at the first checkpoint failing G2 or G3; later checkpoints are
listed as unread and enter no reading or contrast. A cell selects its
highest-step checkpoint passing all three gates and reports its earliest G1
pass as its fit dose. Without a selection it gets the screen's first matching
fit reading, with "through 32 epochs" replaced by "through the cell's last
scheduled checkpoint".

G1 always reads every in-sample tree-chosen line row of the bank. For a cell
that keeps a subset, the tree-chosen line rows' mean deployed probability is
also split into kept and dropped rows (descriptive only).

Contrasts compare each cell with U0 at the same checkpoint steps: the step
of the first G2 or G3 failure, and the G2 metrics at every step both read.
U-lr is also set against U0 at a quarter of its steps (matched summed step
size), and each support cell against its count control.

CLI:
  fit       --spec SPEC.json --out CELLS-FIT.json
  contrasts --fit CELLS-FIT.json --out CONTRASTS.json
SPEC: {"screen_fit": SCREEN-FIT.json,
       "r1": {"probes": [PROBE.json, ...],
              "cells": {"U0": {"full": PHASE1.json, "rerun": PHASE1.json}, ...}},
       "r2": {...}}
U0 must hold a checkpoint at the screen's first U checkpoint step whose state
equals the screen's recorded one; otherwise the fit refuses (engineering).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fit as screen  # noqa: E402
from distill_common import LINEAGES, read_json, require, write_json  # noqa: E402

REFERENCE = "U0"
STEP_SIZE_CELL, STEP_SIZE_RATIO = "U-lr", 4
COUNT_CONTROLS = {"U-s32": "U-c32", "U-s256": "U-c256"}
REFERENCES = ("parent", "O-final")


def merged_readouts(paths: list, lineage: str) -> dict:
    """Candidate readouts of one lineage's probes. Split probes must agree
    exactly on the parent's and O-final's readouts."""
    readouts, reference = {}, None
    for path in paths:
        probe = read_json(path)
        require(probe.get("schema") == screen.PROBE_SCHEMA and probe.get("complete") is True
                and probe["lineage"] == lineage, f"{lineage}: not a complete probe")
        candidates = {c["id"]: c for c in probe["candidates"]}
        require(all(r in candidates for r in REFERENCES), f"{lineage}: a probe lacks the parent or O-final")
        refs = [candidates[r] for r in REFERENCES]
        if reference is None:
            reference = refs
        require(json.dumps(refs, sort_keys=True) == json.dumps(reference, sort_keys=True),
                f"{lineage}: the parent's or O-final's readouts differ between probes")
        for cid, readout in candidates.items():
            if cid in REFERENCES:
                continue
            require(cid not in readouts, f"{lineage}: candidate {cid} read by two probes")
            readouts[cid] = readout
    require(reference is not None, f"{lineage}: no probe")
    readouts.update(zip(REFERENCES, reference))
    return readouts


def cell_options(record: dict) -> dict:
    options = record["identity"]["recipe"].get("options")
    require(options is not None and record["identity"]["arm"] == "uniform", "not a U collapse diagnostic cell")
    return options


def split_line_rows(readout: dict, kept: set) -> dict:
    """Tree-chosen line rows' mean deployed probability, kept and dropped,
    per decision (descriptive only)."""
    out = {}
    for half, flag in (("self", "self_offered"), ("giant", "giant_offered")):
        rows = [r for r in (readout["archive"]["training"].get("line_rows") or [])
                if r["chosen"] and r[flag]]
        for part, members in (("kept", [r for r in rows if (r["game"], r["step"]) in kept]),
                              ("dropped", [r for r in rows if (r["game"], r["step"]) not in kept])):
            out[f"{half}_{part}"] = {"rows": len(members), "mean_deployed_probability":
                                     sum(r["deployed_probability"] for r in members) / len(members)
                                     if members else None}
    return out


def cell_fit(lineage: str, name: str, readouts: dict, full: dict, rerun: dict) -> dict:
    require(full.get("schema") == screen.PHASE1_SCHEMA and full.get("complete") is True
            and full.get("stop_after_steps") is None and full["identity"]["lineage"] == lineage,
            f"{lineage}-{name}: not a complete full run")
    options = cell_options(full)
    require(options["cell"] == name, f"{lineage}-{name}: the run is cell {options['cell']}")
    checkpoints = full["checkpoints"]
    require(checkpoints and [c["step"] for c in checkpoints] == full["identity"]["recipe"]["checkpoints"],
            f"{lineage}-{name}: the run's checkpoints differ from its schedule")
    parent, o_final = readouts["parent"], readouts["O-final"]
    telemetry = read_json(full["telemetry"]["path"])
    repro = screen.reproduced(full, rerun)
    exposure = {e["step"]: e for e in full["census"]["exposure"]}
    subset = "min_edge_backups" in options or "count_control" in options
    kept = {(r["game"], r["step"]) for r in full["census"].get("kept_line_rows", [])}
    rows, stop = [], None
    for c in checkpoints:
        cid = f"{name}-step-{c['step']}"
        readout = readouts.get(cid)
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{lineage}-{name}: the probe did not read checkpoint step {c['step']}")
        first, second = screen.g1(readout), screen.g2(readout, parent, o_final)
        finite = second["finite"] and screen.telemetry_finite(telemetry, c["step"])
        training = readout["archive"]["training"]
        self_offered = training["groups"].get("self_offered")
        row = {"step": c["step"], "epochs": c["epochs"], "candidate": cid, "source": c["source"],
               "state_sha256": c["state_sha256"], "G1": first, "G2": second,
               "G3": {"finite": finite, "reproduced": repro, "pass": finite and repro},
               "exposure": exposure.get(c["step"]),
               "bank_rows": training["groups"].get("all"),
               "discrimination": training["discrimination"],
               "tree_chosen_share": training["tree_chosen_share"],
               "self_offered_mean_deployed_self_probability":
                   self_offered["mean_deployed_self_probability"] if self_offered else None,
               "line_rows_kept_and_dropped": split_line_rows(readout, kept) if subset else None,
               "learner_offers": {k: readout["learner_offers"][k] for k in
                                  ("offers", "mean_deployed_self_probability", "mean_raw_self_probability",
                                   "self_top_share")},
               "completion": readout["archive"].get("completion")}
        row["pass"] = first["pass"] and second["pass"] and row["G3"]["pass"]
        rows.append(row)
        if not (second["pass"] and row["G3"]["pass"]):
            stop = c["step"]
            break
    passing = [r for r in rows if r["pass"]]
    g1_steps = [r["step"] for r in rows if r["G1"]["pass"]]
    selected = passing[-1] if passing else None
    out = {"lineage": lineage, "cell": name, "options": options,
           "rows_kept": full["census"]["arm"]["rows"], "kept_line_rows": len(kept) if subset else None,
           "arm_census": full["census"]["arm"], "read": rows, "stopped_at": stop,
           "unread_checkpoints": [c["step"] for c in checkpoints[len(rows):]],
           "selected": None if selected is None else {k: selected[k] for k in
                                                        ("step", "epochs", "candidate", "source", "state_sha256",
                                                         "learner_offers", "completion")},
           "fit_dose": g1_steps[0] if g1_steps else None}
    out["reading"], out["explanations"] = (("required fit without collapse in this cell", []) if selected
                                           else reading(rows, checkpoints))
    return out


NOT_FIT = ("required fit not reached by the last scheduled checkpoint in this cell",
           ["the line rows' summed archive-term weight may be too low (see each checkpoint's exposure)",
            "a subset may have dropped the line rows (see the kept line rows)",
            "the anchor, including its own Spy rows where the parent declines the line, may outweigh the line rows",
            "the step size may be too small",
            "the student's input may not separate the line decisions"])


def reading(rows: list, checkpoints: list) -> tuple[str, list]:
    """The screen's first matching fit reading; its arm-U row becomes "not
    reached by the last scheduled checkpoint"."""
    text, explanations = screen.reading("U", rows, checkpoints)
    if text == screen.READINGS["U"][0]:
        return NOT_FIT
    return text, explanations


def u0_reproduces_screen(fits: dict, screen_fit: dict) -> dict:
    """U0's state at the screen's first U checkpoint equals the screen's."""
    out = {}
    for lineage, cells in fits.items():
        recorded = screen_fit["fits"][lineage]["U"]["read"][0]
        u0 = cells.get(REFERENCE)
        require(u0 is not None, f"{lineage}: no U0")
        at = next((c for c in u0["_checkpoints"] if c["step"] == recorded["step"]), None)
        out[lineage] = {"step": recorded["step"], "screen_state_sha256": recorded["state_sha256"],
                        "u0_state_sha256": at and at["state_sha256"],
                        "equal": at is not None and at["state_sha256"] == recorded["state_sha256"]}
    return out


def fit_cells(spec: dict) -> dict:
    fits = {}
    for lineage in LINEAGES:
        if lineage not in spec:
            continue
        readouts = merged_readouts(spec[lineage]["probes"], lineage)
        fits[lineage] = {}
        for name, run in sorted(spec[lineage]["cells"].items()):
            full = read_json(run["full"])
            fitted = cell_fit(lineage, name, readouts, full, read_json(run["rerun"]))
            fitted["_checkpoints"] = full["checkpoints"]
            fits[lineage][name] = fitted
    check = u0_reproduces_screen(fits, read_json(spec["screen_fit"]))
    require(all(v["equal"] for v in check.values()),
            f"engineering: U0 does not reproduce the screen's first U checkpoint {check}")
    for cells in fits.values():
        for fitted in cells.values():
            del fitted["_checkpoints"]
    names = sorted({n for cells in fits.values() for n in cells})
    passes = {n: [lin for lin in fits if n in fits[lin] and fits[lin][n]["selected"]] for n in names}
    return {"schema": "spy-u-collapse-v1/fit", "u0_reproduces_screen": check, "fits": fits,
            "passes": passes,
            "lineage_dependent": {n: len(v) == 1 for n, v in passes.items()}}


def first_failure(fitted: dict) -> int | None:
    return fitted["stopped_at"]


def collapse_contrast(cell: dict, reference: dict) -> dict:
    """When each run's reading stopped (its first G2 or G3 failure), and the
    G2 metrics at every step both read."""
    mine, theirs = first_failure(cell), first_failure(reference)
    if theirs is None:
        reading = "the reference held G2 through its schedule: no collapse to compare"
    elif mine is None:
        reading = "held G2 through its schedule, where the reference had failed"
    elif mine > theirs:
        reading = "held G2 longer than the reference"
    elif mine == theirs:
        reading = "failed G2 or G3 at the same checkpoint as the reference"
    else:
        reading = "failed G2 or G3 earlier than the reference"
    shared = sorted({r["step"] for r in cell["read"]} & {r["step"] for r in reference["read"]})
    by_step = lambda fitted: {r["step"]: r["G2"] for r in fitted["read"]}  # noqa: E731
    a, b = by_step(cell), by_step(reference)
    return {"stopped_at": mine, "reference_stopped_at": theirs, "reading": reading,
            "selected": cell["selected"] and cell["selected"]["step"],
            "reference_selected": reference["selected"] and reference["selected"]["step"],
            "g2_at_shared_steps": [{"step": s, "cell": a[s], "reference": b[s]} for s in shared]}


def across(per_lineage: dict) -> str:
    readings = {v["reading"] for v in per_lineage.values()}
    if len(per_lineage) < len(LINEAGES):
        return "one lineage only"
    return readings.pop() if len(readings) == 1 else "lineage-dependent"


def contrasts(fitted: dict) -> dict:
    fits = fitted["fits"]
    names = sorted({n for cells in fits.values() for n in cells} - {REFERENCE})
    out = {"schema": "spy-u-collapse-v1/contrasts", "against_u0": {}, "count_controls": {},
           "matched_summed_step_size": {}}
    for name in names:
        per = {lin: collapse_contrast(cells[name], cells[REFERENCE]) for lin, cells in fits.items()
               if name in cells and REFERENCE in cells}
        out["against_u0"][name] = {"per_lineage": per, "across_lineages": across(per)}
    for support, control in COUNT_CONTROLS.items():
        per = {lin: collapse_contrast(cells[support], cells[control]) for lin, cells in fits.items()
               if support in cells and control in cells}
        if per:
            out["count_controls"][support] = {"control": control, "per_lineage": per,
                                              "across_lineages": across(per)}
    for lin, cells in fits.items():
        if STEP_SIZE_CELL not in cells or REFERENCE not in cells:
            continue
        slow = {r["step"]: r["G2"] for r in cells[STEP_SIZE_CELL]["read"]}
        fast = {r["step"]: r["G2"] for r in cells[REFERENCE]["read"]}
        out["matched_summed_step_size"][lin] = [
            {"u0_step": s, "u_lr_step": STEP_SIZE_RATIO * s, "u0": fast[s], "u_lr": slow[STEP_SIZE_RATIO * s]}
            for s in sorted(fast) if STEP_SIZE_RATIO * s in slow]
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    f = sub.add_parser("fit")
    f.add_argument("--spec", type=Path, required=True)
    f.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("contrasts")
    c.add_argument("--fit", type=Path, required=True)
    c.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "fit":
        result = fit_cells(read_json(a.spec))
        write_json(a.out, result)
        print(json.dumps({"passes": result["passes"], "readings": {
            f"{lin}-{name}": v["reading"] for lin, cells in result["fits"].items() for name, v in cells.items()},
            "stopped_at": {f"{lin}-{name}": v["stopped_at"]
                           for lin, cells in result["fits"].items() for name, v in cells.items()}}))
    else:
        result = contrasts(read_json(a.fit))
        write_json(a.out, result)
        print(json.dumps({name: v["across_lineages"] for name, v in result["against_u0"].items()}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
