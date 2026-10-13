"""Fit check of the Spy archive distillation screen (DESIGN.md section 5 gates; section 7 fit readings).

Offline, from the probe and phase-1 receipts; no game outcome is read.

Per lineage and arm, in dose order, each phase-1 checkpoint gets:
  G1  mean deployed target probability >= 0.5 over the in-sample tree-chosen
      self-target rows, and the same over the tree-chosen Dread-Return-to-Giant
      rows (a group with no rows cannot pass);
  G2  median clamped top-two gap on held-out anchor rows >= half the parent's,
      non-Spy top-action agreement >= O-final's - 0.02, and non-Spy mean KL to
      the parent <= O-final's;
  G3  the run's per-step losses and the gate metrics are finite numbers, and
      the 32-step rerun reproduces the run's step-32 phase-1 state.
The run stops at the first checkpoint failing G2 or G3; later checkpoints are
not read. The arm selects the highest-dose checkpoint passing all three, and
reports the earliest G1 pass as the absorption dose. An arm with no selection
gets the first matching fit reading:
  engineering   G3 failed (not a table row: the run is not evidence);
  teacher split self-target G1 fails at every read checkpoint with G2 holding,
                the tree declined self-target at >= 1/3 of its in-sample
                supported offers, discrimination stays < 0.25 at every read
                checkpoint, and at the last one the mean self-target
                probability over all in-sample supported offers is >= half
                the tree's chosen share there;
  collapse      G1 first passes at the checkpoint where G2 first fails;
  flattening    G2 fails before any checkpoint passes G1;
  diluted       U fails G1 with G2 holding through 32 epochs;
  capacity      L fails G1 with G2 holding through 1,024 steps.
An arm passing in one lineage only reads lineage-dependent absorption.

CLI: fit --spec SPEC.json --out FIT.json
SPEC: {"r1": {"probe": PROBE.json, "runs": {"U": {"full": PHASE1.json, "rerun": PHASE1.json},
       "L": {...}}}, "r2": {...}}; an arm may be absent.
"""
from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from distill_common import ARM_LETTERS, ARMS, LINEAGES, read_json, require, write_json  # noqa: E402

PROBE_SCHEMA = "mtg-kernel-archive-distillation-probe/v1"
PHASE1_SCHEMA = "mtg-kernel-archive-distillation-phase1/v1"
G1_FLOOR = 0.5
GAP_SHARE = 0.5
AGREEMENT_MARGIN = 0.02
REPRODUCTION_STEP = 32
SPLIT_DECLINED_SHARE = 1 / 3
SPLIT_DISCRIMINATION = 0.25
SPLIT_MOVEMENT = 0.5
MIN_OFFERS = 100
MIN_COMPLETION_LINE_ROWS = 3
CHECKPOINTS = 6


def number(value) -> bool:
    return type(value) in (int, float) and math.isfinite(value)


def group(readout: dict, role: str, name: str) -> dict | None:
    return ((readout["archive"].get(role) or {}).get("groups") or {}).get(name)


def g1(readout: dict) -> dict:
    out = {}
    for half, name in (("self", "self_chosen"), ("giant", "giant_chosen")):
        rows = group(readout, "training", name)
        value = rows["mean_deployed_probability"] if rows else None
        out[half] = {"rows": rows["rows"] if rows else 0, "mean_deployed_probability": value,
                     "pass": number(value) and value >= G1_FLOOR}
    out["pass"] = out["self"]["pass"] and out["giant"]["pass"]
    return out


def g2(readout: dict, parent: dict, o_final: dict) -> dict:
    a, p, o = readout["anchor_held_out"], parent["anchor_held_out"], o_final["anchor_held_out"]
    gap, agree, kl = (a["median_clamped_top_two_gap"], a["non_spy_top_agreement"], a["mean_kl_from_parent_non_spy"])
    ref = (p["median_clamped_top_two_gap"], o["non_spy_top_agreement"], o["mean_kl_from_parent_non_spy"])
    require(all(number(x) for x in ref), "parent or O-final collapse readings are not finite")
    out = {"median_clamped_gap": gap, "gap_floor": GAP_SHARE * ref[0],
           "non_spy_top_agreement": agree, "agreement_floor": ref[1] - AGREEMENT_MARGIN,
           "non_spy_kl": kl, "kl_ceiling": ref[2]}
    out["finite"] = all(number(x) for x in (gap, agree, kl))
    out["pass"] = out["finite"] and gap >= out["gap_floor"] and agree >= out["agreement_floor"] and kl <= ref[2]
    return out


def telemetry_finite(telemetry: list, last: int) -> bool:
    for entry in telemetry[:last]:
        if not (number(entry["anchor_loss"]) and number(entry["archive_loss"])):
            return False
        for key in ("anchor_components", "archive_components"):
            if not all(number(part["auxiliary_l2"]) for part in entry[key].values()):
                return False
    return True


def reproduced(full: dict, rerun: dict) -> bool:
    def step32(record):
        return [h["phase1_state_sha256"] for h in record["state_hashes"] if h["step"] == REPRODUCTION_STEP]
    return (rerun.get("schema") == PHASE1_SCHEMA and rerun.get("complete") is True
            and rerun.get("stop_after_steps") == REPRODUCTION_STEP and rerun["identity"] == full["identity"]
            and len(step32(full)) == 1 and step32(full) == step32(rerun))


def arm_fit(lineage: str, arm: str, probe: dict, full: dict, rerun: dict) -> dict:
    require(full.get("schema") == PHASE1_SCHEMA and full.get("complete") is True
            and full.get("stop_after_steps") is None and full["identity"]["lineage"] == lineage
            and ARM_LETTERS[full["identity"]["arm"]] == arm, f"{lineage}-{arm}: not a complete full run")
    checkpoints = full["checkpoints"]
    require(len(checkpoints) == CHECKPOINTS, f"{lineage}-{arm}: expected six checkpoints")
    readouts = {c["id"]: c for c in probe["candidates"]}
    parent, o_final = readouts["parent"], readouts["O-final"]
    telemetry = read_json(full["telemetry"]["path"])
    repro = reproduced(full, rerun)
    rows, stop = [], None
    for c in checkpoints:
        readout = readouts.get(f"{arm}-step-{c['step']}")
        require(readout is not None and readout["state_sha256"] == c["state_sha256"],
                f"{lineage}-{arm}: the probe did not read checkpoint step {c['step']}")
        first, second = g1(readout), g2(readout, parent, o_final)
        finite = second["finite"] and telemetry_finite(telemetry, c["step"])
        training = readout["archive"]["training"]
        self_offered = training["groups"].get("self_offered")
        row = {"step": c["step"], "epochs": c["epochs"], "candidate": f"{arm}-step-{c['step']}",
               "source": c["source"], "state_sha256": c["state_sha256"], "G1": first, "G2": second,
               "G3": {"finite": finite, "reproduced": repro, "pass": finite and repro},
               "discrimination": training["discrimination"],
               "tree_chosen_share": training["tree_chosen_share"],
               "self_offered_mean_deployed_self_probability":
                   self_offered["mean_deployed_self_probability"] if self_offered else None,
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
    out = {"lineage": lineage, "arm": arm, "read": rows, "stopped_at": stop,
           "unread_checkpoints": [c["step"] for c in checkpoints[len(rows):]],
           "selected": None if selected is None else {k: selected[k] for k in
                                                        ("step", "epochs", "candidate", "source", "state_sha256",
                                                         "learner_offers")},
           "absorption_dose": g1_steps[0] if g1_steps else None}
    out["reading"] = "passes the fit check" if selected else reading(arm, rows, checkpoints)
    return out


def reading(arm: str, rows: list, checkpoints: list) -> str:
    if any(not r["G3"]["pass"] for r in rows):
        return "engineering: G3 failed, so the run is not evidence"
    last = rows[-1]
    share = last["tree_chosen_share"]["self"]
    held = all(r["G2"]["pass"] for r in rows)
    if (held and all(not r["G1"]["self"]["pass"] for r in rows)
            and number(share) and 1 - share >= SPLIT_DECLINED_SHARE
            and all(number(r["discrimination"]["self"]) and r["discrimination"]["self"] < SPLIT_DISCRIMINATION
                    for r in rows)
            and number(last["self_offered_mean_deployed_self_probability"])
            and last["self_offered_mean_deployed_self_probability"] >= SPLIT_MOVEMENT * share):
        return "teacher split at the line decision"
    g2_fail = next((i for i, r in enumerate(rows) if not r["G2"]["pass"]), None)
    g1_first = next((i for i, r in enumerate(rows) if r["G1"]["pass"]), None)
    if g2_fail is not None and g1_first == g2_fail:
        return "absorbed only with collapse"
    if g2_fail is not None and (g1_first is None or g1_first > g2_fail):
        return "one-hot distillation flattens the policy before it absorbs the line"
    if held and g1_first is None and len(rows) == len(checkpoints):
        return ("uniform distillation leaves the line too diluted to absorb at this dose" if arm == "U"
                else "the policy cannot absorb its own search's line choices even in isolation")
    return "no fit-reading row matches"


def offer_summary(fits: dict) -> dict:
    """D1's held-out learner-offer probability per arm: per lineage, pooled
    across lineages when either has fewer than 100 offers."""
    out = {}
    for arm in ARMS:
        per = {lin: fits[lin][arm]["selected"]["learner_offers"] for lin in LINEAGES
               if arm in fits.get(lin, {}) and fits[lin][arm]["selected"]}
        entry = {"per_lineage": per, "pooled": None}
        if len(per) == len(LINEAGES) and any(v["offers"] < MIN_OFFERS for v in per.values()):
            n = sum(v["offers"] for v in per.values())
            entry["pooled"] = {"offers": n, "mean_deployed_self_probability":
                               sum(v["mean_deployed_self_probability"] * v["offers"] for v in per.values()) / n
                               if n else None}
        out[arm] = entry
    return out


def completion_summary(fits: dict) -> dict:
    """Line-row generalization on the held-out completion bank at each
    selected checkpoint; pooled across lineages when a lineage has fewer than
    3 tree-chosen line rows there."""
    out = {}
    for arm in ARMS:
        per = {}
        for lin in LINEAGES:
            fit = fits.get(lin, {}).get(arm)
            if not fit or not fit["selected"]:
                continue
            row = next(r for r in fit["read"] if r["step"] == fit["selected"]["step"])
            if row["completion"] is not None:
                per[lin] = row["completion"]
        entry = {"per_lineage": per, "pooled": None}
        chosen = {lin: [r for r in c["line_rows"] if r["chosen"]] for lin, c in per.items()}
        if per and any(len(v) < MIN_COMPLETION_LINE_ROWS for v in chosen.values()):
            rows = [r for v in chosen.values() for r in v]
            entry["pooled"] = {"tree_chosen_line_rows": len(rows), "mean_deployed_probability":
                               sum(r["deployed_probability"] for r in rows) / len(rows) if rows else None}
        out[arm] = entry
    return out


def fit(spec: dict) -> dict:
    fits = {}
    for lineage in LINEAGES:
        if lineage not in spec:
            continue
        probe = read_json(spec[lineage]["probe"])
        require(probe.get("schema") == PROBE_SCHEMA and probe.get("complete") is True
                and probe["lineage"] == lineage, f"{lineage}: not a complete probe")
        fits[lineage] = {arm: arm_fit(lineage, arm, probe, read_json(run["full"]), read_json(run["rerun"]))
                         for arm, run in sorted(spec[lineage]["runs"].items())}
    lineage_dependent = {arm: [lin for lin in fits if arm in fits[lin] and fits[lin][arm]["selected"]]
                         for arm in ARMS}
    return {"schema": "spy-archive-distillation-v1/fit", "fits": fits,
            "passes": lineage_dependent,
            "lineage_dependent": {arm: len(v) == 1 for arm, v in lineage_dependent.items()},
            "learner_offers": offer_summary(fits), "completion": completion_summary(fits)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    f = sub.add_parser("fit")
    f.add_argument("--spec", type=Path, required=True)
    f.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    result = fit(read_json(a.spec))
    write_json(a.out, result)
    print(json.dumps({"passes": result["passes"], "readings": {
        f"{lin}-{arm}": v["reading"] for lin, arms in result["fits"].items() for arm, v in arms.items()},
        "selected": {f"{lin}-{arm}": v["selected"] and v["selected"]["step"]
                     for lin, arms in result["fits"].items() for arm, v in arms.items()}}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
