"""Fit readings and one-factor contrasts of the U collapse diagnostic (collab LANES/spy-u-collapse-20261014).

Each cell is arm U of the Spy archive distillation screen with one change
(DESIGN.md v3 section 2). Offline, from the probes and phase-1 receipts; no
game outcome is read.

The cells and their schedules are the design's (CELLS, schedule): every run
must carry exactly its cell's options, and a count control exactly its
support cell's row count.

Per lineage and cell, the checkpoints before 32 steps are descriptive: their
probe metrics are reported and enter no gate, stop rule, selection, reading
or contrast. The gated checkpoints are read in step order with the screen's
gates (fit.py G1, G2 and G3). G2's references come from the probes' parent
and O-final readouts and must equal the screen's FIT.json values. The
reading stops at the first gated checkpoint failing G2 or G3; later
checkpoints are listed as unread and enter no reading or contrast. A cell
selects its highest-step checkpoint passing all three gates and reports its
earliest G1 pass as its fit dose. Without a selection it gets the screen's
first matching fit reading, with "through 32 epochs" replaced by "through
8,192 steps"; the mixed-choices row applies to all-row cells only.

G1 always reads every tree-chosen line row of the bank. A subset cell also
reports, beside the bank's figures: G1 and discrimination over the line rows
it kept and those it dropped, its kept line rows by decision and by chosen or
declined, and its kept rows' parent disagreement. Every subset reading adds
two competing explanations (SUBSET_EXPLANATIONS).

Contrasts (design section 5), over gated checkpoints both runs read: each
cell against U0 by collapse step, where one run collapsed later than another
when it passed G2 at a read checkpoint at or after the other's first G2
failure, with the G2 metrics at every step both read; U-lr at 4N steps
against U0 at N; each support cell against its count control, on G2 only,
worded by the support cell's own contrast with U0, so that a change is
credited to the support rule only when the support cell itself moved against
U0; and each all-row cell's G2 metrics against its fit to its own rows. A
contrast holds across lineages when both give the same ordering and wording;
a lineage whose contrast is not evidence or not ordered is named rather than
read as lineage-dependent.

CLI:
  exactness --screen-dry DRY.json --dry DRY.json --screen-full PHASE1.json --rerun PHASE1.json --out OUT.json
            (a default dry run and rerun of a screen arm against the screen's records; exits 1 on a mismatch)
  commands  --lineage r1 --parent P.json --games GAMES.jsonl --bank BANK.json --anchor ANCHOR.json
            --root DIR --out-dir CMDS
            (every cell's dry-run, full-run and rerun command, with output directories
             DIR/LIN-CELL-dry, DIR/LIN-CELL and DIR/LIN-CELL-rerun)
  u0-state  --screen-fit SCREEN-FIT.json --full PHASE1.json
            (U0's state at the screen's first U checkpoint; exits 1 on a mismatch)
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
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import fit as screen  # noqa: E402
import phase1  # noqa: E402
from distill_common import LINEAGES, read_json, require, write_json  # noqa: E402

REFERENCE = "U0"
# The screen's first U checkpoint: one epoch of U's rows.
FIRST_U_STEP = {"r1": 189, "r2": 239}
BASE_STEPS = (8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192)
GATED_FROM = screen.REPRODUCTION_STEP
STEP_SIZE_CELL, STEP_SIZE_RATIO = "U-lr", 4
ANCHOR_CELLS = ("U-a4", "U-a16")
COUNT_CONTROLS = {"U-s32": "U-c32", "U-s256": "U-c256"}
# Rows each support floor keeps (the screen's U census, by_support).
SUPPORT_ROWS = {"r1": {32: 3281, 256: 599}, "r2": {32: 3971, 256: 731}}
# Each cell's change from the screen's U; a count control names the floor
# whose row count it takes.
CELLS = {"U0": {}, "U-lr": {"learning_rate": 2.5e-5},
         "U-a4": {"anchor_coefficient": 4.0}, "U-a16": {"anchor_coefficient": 16.0},
         "U-s32": {"min_edge_backups": 32}, "U-s256": {"min_edge_backups": 256},
         "U-c32": {"count_control_of": 32}, "U-c256": {"count_control_of": 256}}
REFERENCES = ("parent", "O-final")
G2_REFERENCES = ("gap_floor", "agreement_floor", "kl_ceiling")


def schedule(lineage: str, cell: str) -> list[int]:
    """The cell's checkpoint steps: the shared schedule with the screen's
    first U step, and for U-lr also four times that step."""
    steps = set(BASE_STEPS) | {FIRST_U_STEP[lineage]}
    if cell == STEP_SIZE_CELL:
        steps.add(STEP_SIZE_RATIO * FIRST_U_STEP[lineage])
    return sorted(steps)


def design_options(lineage: str, cell: str) -> dict:
    """The phase-1 options the design gives a cell in a lineage."""
    require(cell in CELLS, f"unknown cell {cell!r}")
    fields = dict(CELLS[cell])
    if "count_control_of" in fields:
        fields = {"count_control": SUPPORT_ROWS[lineage][fields["count_control_of"]]}
    return {"cell": cell, **fields, "checkpoint_steps": schedule(lineage, cell)}


def f32_bits(value: float) -> int:
    return struct.unpack("<I", struct.pack("<f", value))[0]


def comparable(options: dict) -> dict:
    """Options with the step size as its f32 bits: the engine records the f32
    it used, which JSON may spell as its f64 expansion."""
    out = dict(options)
    if "learning_rate" in out:
        out["learning_rate"] = f32_bits(out["learning_rate"])
    return out


RUNTIME_FIELDS = ("runtime_commit", "runtime_tree", "runtime_binary_sha256")


def without_build(record: dict) -> dict:
    out = json.loads(json.dumps(record))
    for key in RUNTIME_FIELDS:
        out["identity"].pop(key, None)
    return out


def step32(record: dict) -> list:
    return [h["phase1_state_sha256"] for h in record.get("state_hashes", []) if h["step"] == GATED_FROM]


def exactness(screen_dry: dict, dry: dict, screen_full: dict, rerun: dict) -> dict:
    """A default dry run and 32-step rerun of a screen arm under the new
    build against the screen's records (design section 7): the dry run must
    equal the screen's in everything but the build fields, and the rerun's
    step-32 state must equal the screen's full run's."""
    a, b = without_build(screen_dry), without_build(dry)
    full, again = without_build(screen_full), without_build(rerun)
    out = {"lineage": screen_dry["identity"]["lineage"], "arm": screen_dry["identity"]["arm"],
           "dry_run_differs_in": sorted(k for k in set(a) | set(b) if a.get(k) != b.get(k)),
           "dry_run_identity_differs_in": sorted(k for k in set(a["identity"]) | set(b["identity"])
                                                 if a["identity"].get(k) != b["identity"].get(k)),
           "rerun_identity_equal": full["identity"] == again["identity"],
           "screen_step32": step32(screen_full), "rerun_step32": step32(rerun)}
    out["pass"] = (screen_dry.get("dry_run") is True and not out["dry_run_differs_in"]
                   and rerun.get("complete") is True and rerun.get("stop_after_steps") == GATED_FROM
                   and "options" not in rerun["identity"]["recipe"] and out["rerun_identity_equal"]
                   and len(out["screen_step32"]) == 1 and out["screen_step32"] == out["rerun_step32"])
    return out


def u0_state(screen_fit: dict, full: dict) -> dict:
    """U0's state at the screen's first U checkpoint against the screen's."""
    lineage = full["identity"]["lineage"]
    require(cell_options(full)["cell"] == REFERENCE, "not a U0 run")
    return u0_reproduces_screen(screen_fit, {lineage: full["checkpoints"]})[lineage]


def commands(lineage: str, parent: dict, games: Path, bank: Path, anchor: dict, root: str) -> dict:
    """Every cell's dry-run, full-run and rerun phase-1 command."""
    out = {}
    for cell in CELLS:
        options = design_options(lineage, cell)
        base = f"{root.rstrip('/')}/{lineage}-{cell}"
        for suffix, extra in (("-dry", {"dry_run": True}), ("", {}), ("-rerun", {"rerun": True})):
            out[cell + suffix] = phase1.phase1_command(lineage, "U", parent, games, bank, anchor, base + suffix,
                                                       options=options, **extra)
    return out


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


def references_match(readouts: dict, screen_fit: dict, lineage: str) -> dict:
    """The probes' G2 references against the screen's FIT.json values."""
    recorded = screen_fit["fits"][lineage]["U"]["read"][0]["G2"]
    probed = screen.g2(readouts["parent"], readouts["parent"], readouts["O-final"])
    out = {k: {"probe": probed[k], "screen": recorded[k]} for k in G2_REFERENCES}
    out["equal"] = all(probed[k] == recorded[k] for k in G2_REFERENCES)
    return out


def cell_options(record: dict) -> dict:
    options = record["identity"]["recipe"].get("options")
    require(options is not None and record["identity"]["arm"] == "uniform", "not a U collapse diagnostic cell")
    return options


def mean(values: list) -> float | None:
    return sum(values) / len(values) if values else None


def line_figures(rows: list) -> dict:
    """Per line decision: tree-chosen and declined rows, the mean deployed
    probability at the tree-chosen ones (G1's figure) and discrimination."""
    out = {}
    for half, flag, line in (("self", "self_offered", "self_deployed_probability"),
                             ("giant", "giant_offered", "giant_deployed_probability")):
        chosen = [r for r in rows if r[flag] and r["chosen"]]
        declined = [r for r in rows if r[flag] and not r["chosen"]]
        a, b = mean([r[line] for r in chosen]), mean([r[line] for r in declined])
        out[half] = {"chosen": len(chosen), "declined": len(declined),
                     "mean_deployed_probability": mean([r["deployed_probability"] for r in chosen]),
                     "discrimination": None if a is None or b is None else a - b}
    return out


def kept_and_dropped(readout: dict, kept: dict) -> dict:
    """The bank's line rows split into those the cell kept and those it
    dropped, with each part's figures (descriptive only)."""
    rows = readout["archive"]["training"].get("line_rows") or []
    keys = [(r["game"], r["step"]) for r in rows]
    require(len(set(keys)) == len(keys), "the probe lists a line row twice")
    require(set(kept) <= set(keys), "a kept line row is not among the probe's line rows")
    for key, r in zip(keys, rows):
        require(not (r["self_offered"] and r["giant_offered"]), "a line row offers both line decisions")
        flags = kept.get(key)
        require(flags is None or (flags["self_offered"] == r["self_offered"]
                                  and flags["giant_offered"] == r["giant_offered"]
                                  and (flags["self_chosen"] or flags["giant_chosen"]) == r["chosen"]),
                "a kept line row's flags differ between the run's census and the probe")
    return {"kept": line_figures([r for k, r in zip(keys, rows) if k in kept]),
            "dropped": line_figures([r for k, r in zip(keys, rows) if k not in kept])}


def ungated(value):
    """A metric dict without its gate verdicts, for descriptive checkpoints."""
    if isinstance(value, dict):
        return {k: ungated(v) for k, v in value.items() if k != "pass"}
    return value


def metrics(readout: dict, checkpoint: dict, cid: str, exposure: dict, subset: bool, kept: dict,
            parent: dict, o_final: dict) -> dict:
    training = readout["archive"]["training"]
    self_offered = training["groups"].get("self_offered")
    bank_rows = training["groups"].get("all")
    return {"step": checkpoint["step"], "epochs": checkpoint["epochs"], "candidate": cid,
            "source": checkpoint["source"], "state_sha256": checkpoint["state_sha256"],
            "G1": screen.g1(readout), "G2": screen.g2(readout, parent, o_final),
            "exposure": exposure.get(checkpoint["step"]),
            "bank_rows": bank_rows, "training_rows": None if subset else bank_rows,
            "discrimination": training["discrimination"], "tree_chosen_share": training["tree_chosen_share"],
            "self_offered_mean_deployed_self_probability":
                self_offered["mean_deployed_self_probability"] if self_offered else None,
            "line_rows_kept_and_dropped": kept_and_dropped(readout, kept) if subset else None,
            "learner_offers": {k: readout["learner_offers"][k] for k in
                               ("offers", "mean_deployed_self_probability", "mean_raw_self_probability",
                                "self_top_share")},
            "completion": readout["archive"].get("completion")}


def cell_fit(lineage: str, name: str, readouts: dict, full: dict, rerun: dict) -> dict:
    require(full.get("schema") == screen.PHASE1_SCHEMA and full.get("complete") is True
            and full.get("stop_after_steps") is None and full["identity"]["lineage"] == lineage,
            f"{lineage}-{name}: not a complete full run")
    options = cell_options(full)
    require(options["cell"] == name, f"{lineage}-{name}: the run is cell {options['cell']}")
    require(comparable(options) == comparable(design_options(lineage, name)),
            f"{lineage}-{name}: the run's options differ from the design's")
    checkpoints = full["checkpoints"]
    require([c["step"] for c in checkpoints] == full["identity"]["recipe"]["checkpoints"] == schedule(lineage, name),
            f"{lineage}-{name}: the run's checkpoints differ from its schedule")
    parent, o_final = readouts["parent"], readouts["O-final"]
    telemetry = read_json(full["telemetry"]["path"])
    repro = screen.reproduced(full, rerun)
    census = full["census"]
    exposure = {e["step"]: e for e in census["exposure"]}
    subset = "min_edge_backups" in options or "count_control" in options
    kept = {(r["game"], r["step"]): r["line"] for r in census.get("kept_line_rows", [])} if subset else {}
    gated = [c for c in checkpoints if c["step"] >= GATED_FROM]
    descriptive, rows, stop = [], [], None
    for c in checkpoints:
        cid = f"{name}-step-{c['step']}"
        readout = readouts.get(cid)
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{lineage}-{name}: the probe did not read checkpoint step {c['step']}")
        if c["step"] >= GATED_FROM and stop is not None:
            continue
        row = metrics(readout, c, cid, exposure, subset, kept, parent, o_final)
        if c["step"] < GATED_FROM:
            descriptive.append(ungated(row))
            continue
        finite = row["G2"]["finite"] and screen.telemetry_finite(telemetry, c["step"])
        row["G3"] = {"finite": finite, "reproduced": repro, "pass": finite and repro}
        row["pass"] = row["G1"]["pass"] and row["G2"]["pass"] and row["G3"]["pass"]
        rows.append(row)
        if not (row["G2"]["pass"] and row["G3"]["pass"]):
            stop = c["step"]
    passing = [r for r in rows if r["pass"]]
    g1_steps = [r["step"] for r in rows if r["G1"]["pass"]]
    selected = passing[-1] if passing else None
    decisions = ("self_chosen", "self_declined", "giant_chosen", "giant_declined")
    out = {"lineage": lineage, "cell": name, "options": options, "subset": subset,
           "rows_kept": census["arm"]["rows"],
           "line_rows": {"kept": {k: census["arm"][k] for k in decisions},
                         "bank": {k: census["bank"][k] for k in decisions}} if subset else None,
           "parent_disagreement": {"kept": census["arm"]["parent_disagreement"],
                                   "bank": census["bank"]["parent_disagreement"]} if subset else None,
           "gated_checkpoints": [c["step"] for c in gated], "descriptive": descriptive,
           "read": rows, "stopped_at": stop,
           "unread_checkpoints": [c["step"] for c in gated[len(rows):]],
           "selected": None if selected is None else {k: selected[k] for k in
                                                        ("step", "epochs", "candidate", "source", "state_sha256",
                                                         "learner_offers", "completion")},
           "fit_dose": g1_steps[0] if g1_steps else None}
    if selected:
        out["reading"] = PASS
        out["explanations"] = PASS_EXPLANATIONS + (SUBSET_PASS_EXPLANATIONS + SUBSET_EXPLANATIONS if subset else [])
    else:
        out["reading"], out["explanations"] = reading(rows, gated, subset)
    return out


# Readings (DESIGN.md v3 section 5): the screen's fit readings in this cell,
# each with its competing explanations.
PASS = "required fit without collapse in this cell"
PASS_EXPLANATIONS = ["a fit is not transfer"]
SUBSET_PASS_EXPLANATIONS = ["a subset cell can pass G1 by raising the line's probability at declined offers too "
                            "(see discrimination over the bank's and the kept line rows)"]
SUBSET_EXPLANATIONS = ["the bank's line rows include rows this cell did not train on "
                       "(see the kept and dropped line rows)",
                       "the kept rows may agree more with the parent than the bank's rows do "
                       "(see the parent disagreement of the kept rows and the bank)"]
NOT_FIT_TEXT = "required fit not reached by 8,192 steps in this cell"
ANCHOR_SPY_ROWS = "the anchor, including its own Spy rows where the parent declines the line, may outweigh the line rows"
INPUT = "the student's input may not separate the line decisions"
NOT_FIT_ALL = (NOT_FIT_TEXT,
               ["the expected dose limit: by 8,192 steps the tree-chosen line rows' summed weight is about L's at "
                "64 steps, where L had not reached the fit (design section 2)", ANCHOR_SPY_ROWS, INPUT])
NOT_FIT_SUBSET = (NOT_FIT_TEXT,
                  ["the line rows' summed archive-term weight may be too low (see each checkpoint's exposure "
                   "against L's)", "the subset may have dropped the line rows (see the kept line rows)",
                   ANCHOR_SPY_ROWS, "the step size may be too small", INPUT])


def without_mixed(rows: list, gated: list) -> tuple[str, list]:
    """fit.py's readings without its mixed-choices row, whose figures are
    computed over the bank's rows rather than a subset cell's own."""
    if any(not r["G3"]["pass"] for r in rows):
        return screen.ENGINEERING, []
    g2_fail = next((i for i, r in enumerate(rows) if not r["G2"]["pass"]), None)
    g1_first = next((i for i, r in enumerate(rows) if r["G1"]["pass"]), None)
    if g2_fail is not None and g1_first == g2_fail:
        return screen.READINGS["fit_with_collapse"]
    if g2_fail is not None and (g1_first is None or g1_first > g2_fail):
        return screen.READINGS["collapse_first"]
    if g2_fail is None and g1_first is None and len(rows) == len(gated):
        return screen.READINGS["U"]
    return screen.NO_MATCH, []


def reading(rows: list, gated: list, subset: bool) -> tuple[str, list]:
    """The screen's first matching fit reading; its arm-U row becomes "not
    reached by 8,192 steps"."""
    text, explanations = without_mixed(rows, gated) if subset else screen.reading("U", rows, gated)
    if text == screen.READINGS["U"][0]:
        text, explanations = NOT_FIT_SUBSET if subset else NOT_FIT_ALL
    explanations = list(explanations)
    if subset and text not in (screen.ENGINEERING, screen.NO_MATCH):
        explanations += SUBSET_EXPLANATIONS
    return text, explanations


def u0_reproduces_screen(screen_fit: dict, checkpoints: dict) -> dict:
    """U0's state at the screen's first U checkpoint equals the screen's."""
    out = {}
    for lineage, u0 in checkpoints.items():
        recorded = screen_fit["fits"][lineage]["U"]["read"][0]
        at = next((c for c in u0 if c["step"] == recorded["step"]), None)
        out[lineage] = {"step": recorded["step"], "screen_state_sha256": recorded["state_sha256"],
                        "u0_state_sha256": at and at["state_sha256"],
                        "equal": at is not None and at["state_sha256"] == recorded["state_sha256"]}
    return out


def fit_cells(spec: dict) -> dict:
    screen_fit = read_json(spec["screen_fit"])
    fits, references, u0_checkpoints = {}, {}, {}
    for lineage in LINEAGES:
        if lineage not in spec:
            continue
        readouts = merged_readouts(spec[lineage]["probes"], lineage)
        references[lineage] = references_match(readouts, screen_fit, lineage)
        require(references[lineage]["equal"],
                f"engineering: {lineage}: the probes' G2 references differ from the screen's {references[lineage]}")
        require(REFERENCE in spec[lineage]["cells"], f"{lineage}: no U0")
        fits[lineage] = {}
        for name, run in sorted(spec[lineage]["cells"].items()):
            full = read_json(run["full"])
            fits[lineage][name] = cell_fit(lineage, name, readouts, full, read_json(run["rerun"]))
            if name == REFERENCE:
                u0_checkpoints[lineage] = full["checkpoints"]
        for support, control in COUNT_CONTROLS.items():
            if support in fits[lineage] and control in fits[lineage]:
                n = SUPPORT_ROWS[lineage][CELLS[support]["min_edge_backups"]]
                require(fits[lineage][support]["rows_kept"] == fits[lineage][control]["rows_kept"] == n,
                        f"{lineage}: {support} and {control} do not both keep {n} rows")
    check = u0_reproduces_screen(screen_fit, u0_checkpoints)
    require(all(v["equal"] for v in check.values()),
            f"engineering: U0 does not reproduce the screen's first U checkpoint {check}")
    names = sorted({n for cells in fits.values() for n in cells})
    passes = {n: [lin for lin in fits if n in fits[lin] and fits[lin][n]["selected"]] for n in names}
    return {"schema": "spy-u-collapse-v1/fit", "g2_references": references, "u0_reproduces_screen": check,
            "fits": fits, "passes": passes, "lineage_dependent": {n: len(v) == 1 for n, v in passes.items()}}


# Collapse ordering (DESIGN.md v3 section 5).
LATER, EARLIER = "collapsed later", "collapsed earlier"
SAME = "collapsed in the same checkpoint interval"
NEITHER = "neither collapsed through 8,192 steps"
NOT_ORDERED = "not ordered at these checkpoints"
NOT_EVIDENCE = "not evidence: G3 failed before any checkpoint passed"
WORDING = {"anchor": {LATER: "a larger ratio of the anchor term to the archive term delayed the collapse in this "
                              "lineage, at the screen's step size and targets"},
           "step_size": {LATER: "a smaller step delayed the collapse in steps, at the screen's ratio"},
           "count_control": {LATER: "keeping well-supported rows delayed the collapse beyond what fewer rows alone "
                                    "did, in this lineage",
                             SAME: "fewer rows explain the change as well as the support rule does",
                             NEITHER: "fewer rows explain the change as well as the support rule does",
                             EARLIER: "keeping well-supported rows brought the collapse earlier than the same "
                                      "number of random rows, in this lineage"}}
NO_CHANGE = "neither the support rule nor fewer rows changed the collapse at these checkpoints, in this lineage"
# The support cell differs from its control while it stays in U0's interval:
# the change is the control's, not the support rule's.
CONTROL_MOVED = {LATER: "the same number of random rows brought the collapse earlier than U0, and the support rule "
                        "did not, in this lineage",
                 EARLIER: "the same number of random rows delayed the collapse past U0's, and the support rule did "
                          "not, in this lineage"}
COUNT_CONTROL_EXPLANATIONS = ["the support rule may keep rows that agree more with the parent "
                              "(see both cells' parent disagreement)",
                              "the support rule also changes which decisions are trained",
                              "each count control is one random draw"]


def collapse_step(fitted: dict) -> int | None:
    """The first read checkpoint failing G2."""
    return next((r["step"] for r in fitted["read"] if not r["G2"]["pass"]), None)


def held_through(fitted: dict) -> bool:
    return collapse_step(fitted) is None and len(fitted["read"]) == len(fitted["gated_checkpoints"])


def order(a: dict, b: dict) -> str:
    """When run a collapsed relative to run b, at the checkpoints both read."""
    if screen.ENGINEERING in (a["reading"], b["reading"]):
        return NOT_EVIDENCE
    ca, cb = collapse_step(a), collapse_step(b)
    passed = lambda fitted: [r["step"] for r in fitted["read"] if r["G2"]["pass"]]  # noqa: E731
    if cb is not None and any(s >= cb for s in passed(a)):
        return LATER
    if ca is not None and any(s >= ca for s in passed(b)):
        return EARLIER
    if ca is not None and ca == cb:
        return SAME
    if held_through(a) and held_through(b):
        return NEITHER
    return NOT_ORDERED


def g2_side_by_side(a: dict, b: dict) -> list:
    by_step = lambda fitted: {r["step"]: r["G2"] for r in fitted["read"]}  # noqa: E731
    x, y = by_step(a), by_step(b)
    return [{"step": s, "cell": x[s], "reference": y[s]} for s in sorted(set(x) & set(y))]


def contrast(cell: dict, reference: dict, kind: str | None, g2_only: bool = False) -> dict:
    ordering = order(cell, reference)
    out = {"collapse_step": collapse_step(cell), "reference_collapse_step": collapse_step(reference),
           "ordering": ordering, "wording": WORDING.get(kind, {}).get(ordering),
           "g2_at_shared_steps": g2_side_by_side(cell, reference)}
    if not g2_only:
        out["selected"] = cell["selected"] and cell["selected"]["step"]
        out["reference_selected"] = reference["selected"] and reference["selected"]["step"]
    return out


def count_control_wording(ordering: str, support_against_u0: str) -> str | None:
    """A support cell against its count control reads as a change by the
    support rule only where the support cell's own collapse moved against
    U0's in the same direction; otherwise it says whose collapse moved."""
    if ordering == SAME:
        if support_against_u0 == SAME:
            return NO_CHANGE
        if support_against_u0 in (LATER, EARLIER):
            return WORDING["count_control"][SAME]
        return (f"the support rule and the same number of random rows collapsed in the same checkpoint interval; "
                f"against U0 the support cell reads \"{support_against_u0}\"")
    if ordering in (LATER, EARLIER):
        if support_against_u0 == ordering:
            return WORDING["count_control"][ordering]
        if support_against_u0 == SAME:
            return CONTROL_MOVED[ordering]
        return (f"the support cell {ordering} than the same number of random rows; against U0 it reads "
                f"\"{support_against_u0}\"")
    return WORDING["count_control"].get(ordering)


def across(per_lineage: dict) -> str:
    """A contrast holds across lineages when both lineages give the same
    ordering and wording. A lineage whose contrast is not evidence or not
    ordered is named, not read as a difference between lineages."""
    if len(per_lineage) < len(LINEAGES):
        return "one lineage only"
    for ordering, label in ((NOT_EVIDENCE, "not evidence"), (NOT_ORDERED, "not ordered")):
        named = [lin for lin, v in sorted(per_lineage.items()) if v["ordering"] == ordering]
        if named:
            return f"{label} in {' and '.join(named)}"
    readings = {(v["ordering"], v["wording"]) for v in per_lineage.values()}
    return readings.pop()[0] if len(readings) == 1 else "lineage-dependent"


def kind_of(name: str) -> str | None:
    if name in ANCHOR_CELLS:
        return "anchor"
    return "step_size" if name == STEP_SIZE_CELL else None


def contrasts(fitted: dict) -> dict:
    fits = fitted["fits"]
    names = sorted({n for cells in fits.values() for n in cells} - {REFERENCE})
    out = {"schema": "spy-u-collapse-v1/contrasts", "against_u0": {}, "count_controls": {},
           "matched_summed_step_size": {}, "collapse_against_fit": {}}
    for name in names:
        per = {lin: contrast(cells[name], cells[REFERENCE], kind_of(name)) for lin, cells in fits.items()
               if name in cells and REFERENCE in cells}
        out["against_u0"][name] = {"per_lineage": per, "across_lineages": across(per)}
    for support, control in COUNT_CONTROLS.items():
        per = {}
        for lin, cells in fits.items():
            if support in cells and control in cells:
                per[lin] = contrast(cells[support], cells[control], "count_control", g2_only=True)
                per[lin]["support_against_u0"] = order(cells[support], cells[REFERENCE])
                per[lin]["wording"] = count_control_wording(per[lin]["ordering"], per[lin]["support_against_u0"])
                per[lin]["parent_disagreement"] = {support: cells[support]["parent_disagreement"],
                                                   control: cells[control]["parent_disagreement"]}
        if per:
            out["count_controls"][support] = {"control": control, "per_lineage": per,
                                              "across_lineages": across(per),
                                              "explanations": COUNT_CONTROL_EXPLANATIONS}
    for lin, cells in fits.items():
        if STEP_SIZE_CELL in cells and REFERENCE in cells:
            slow = {r["step"]: r["G2"] for r in cells[STEP_SIZE_CELL]["read"]}
            fast = {r["step"]: r["G2"] for r in cells[REFERENCE]["read"]}
            out["matched_summed_step_size"][lin] = [
                {"u0_step": s, "u_lr_step": STEP_SIZE_RATIO * s, "u0": fast[s], "u_lr": slow[STEP_SIZE_RATIO * s]}
                for s in sorted(fast) if STEP_SIZE_RATIO * s in slow]
        for name, cell in sorted(cells.items()):
            if not cell["subset"]:
                out["collapse_against_fit"].setdefault(name, {})[lin] = [
                    {"step": r["step"], "training_top_agreement": r["training_rows"]["top_agreement"],
                     "training_mean_deployed_probability": r["training_rows"]["mean_deployed_probability"],
                     **{k: r["G2"][k] for k in ("median_clamped_gap", "non_spy_top_agreement", "non_spy_kl")}}
                    for r in cell["read"]]
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    e = sub.add_parser("exactness")
    for flag in ("--screen-dry", "--dry", "--screen-full", "--rerun", "--out"):
        e.add_argument(flag, type=Path, required=True)
    u = sub.add_parser("u0-state")
    u.add_argument("--screen-fit", type=Path, required=True)
    u.add_argument("--full", type=Path, required=True)
    m = sub.add_parser("commands")
    m.add_argument("--lineage", required=True, choices=LINEAGES)
    for flag in ("--parent", "--games", "--bank", "--anchor", "--out-dir"):
        m.add_argument(flag, type=Path, required=True)
    m.add_argument("--root", required=True)
    f = sub.add_parser("fit")
    f.add_argument("--spec", type=Path, required=True)
    f.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("contrasts")
    c.add_argument("--fit", type=Path, required=True)
    c.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "exactness":
        result = exactness(read_json(a.screen_dry), read_json(a.dry), read_json(a.screen_full), read_json(a.rerun))
        write_json(a.out, result)
        print(json.dumps(result))
        return 0 if result["pass"] else 1
    if a.op == "u0-state":
        result = u0_state(read_json(a.screen_fit), read_json(a.full))
        print(json.dumps(result))
        return 0 if result["equal"] else 1
    if a.op == "commands":
        written = {name: write_json(a.out_dir / f"{a.lineage}-{name}.command.json", command)
                   for name, command in commands(a.lineage, read_json(a.parent), a.games, a.bank,
                                                 read_json(a.anchor), a.root).items()}
        print(json.dumps(written))
    elif a.op == "fit":
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
