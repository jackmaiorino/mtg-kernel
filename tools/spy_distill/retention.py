"""Spy line retention trace (collab LANES/spy-line-retention-20261015, DESIGN.md sections 3 to 5).

Offline, from the runs' receipts and update records, the probes, the traces and
the panel rows; nothing is played or trained here.

- ``updates``: a complete phase-2 run's 162 updates in order, each with the
  source after it, its output state and Adam step, and its update record's X1
  fields;
- ``trace-command``: the engine's ``line_retention_trace`` command for one
  complete run;
- ``configs``: the config gate (A-s0's config against the screen's D2L
  production config, each replicate's against A-s0's, each N config against
  its A config, worker counts and output directories aside);
- ``probes``: the probe commands of one lineage. Every probe holds the parent
  and D1L, then at most 62 run checkpoints (the engine reads at most 64
  candidates): updates 6, 12, ..., 162 of A-s0 and N-s0, and updates 18, 36,
  ..., 162 of the other six runs. With no runs, one reference probe of the
  parent and D1L alone, read before the first dispatch;
- ``x3``: X3 (the parent's readouts identical in every probe, D1L's equal to
  the screen's FIT.json ``L-step-1024``), first on the reference probe;
- ``exactness``: X1 (A-s0's endpoint state and Adam step against the screen's
  D2L checkpoint and, while the screen's update records are on disk, every
  update record's X1 fields against the screen's) and, given the lineage's
  probes, X3. The first wave checks X1 alone; ``analyze`` checks X3 again
  before any reading;
- ``analyze``: the config gate on the production configs, X1's result, X3,
  each run bound to its role (its trace's completion, its manifest's episode
  ids and seeds, its Spy learner episodes and its panels' endpoint), then the
  paths, the traces and every panel, X4 first;
- ``paths``: per run, the self path s(c) (the offer set's mean deployed
  self-target probability) and the Giant path g(c)
  (``archive.training.groups.giant_chosen.mean_deployed_probability``: the
  probability of the tree's chosen Dread-Return target at G1's Giant rows,
  rows L was fit on), with the summed Giant mass and the context readouts
  beside them; the retained share rho = path(endpoint) / path(D1L) and its
  class;
- ``panels``: X4 (the screen's D2L-D1L points and intervals for every panel
  metric, against H, against T1 and pooled, reproduced exactly), then the
  screen's analyzer on every endpoint's panels: its direct contrast with D1L
  over the same panel games;
- ``readings``: section 5's readings in the design's words.

Run roles: A-s0 to A-s3 keep the Spy learner's units and N-s0 to N-s3 drop
them; s0 has the screen's unit seeds. A-s0's panels are D2L's (its endpoint
must equal D2L's state), so its panel role is the screen's ``D2L``. Panels of
the other runs are named ``{lineage}-{role}-vs-{opponent}``.

CLI (JSON on stdout):
  trace-command --run-directory RUN --output-directory DIR --out CMD.json
            (RUN holds the run's run.json and completion.json)
  configs   --lineage r1 --screen SCREEN_D2L/CONFIG-prod.json --config A-s0=CFG.json
            [--config ROLE=CFG.json ...] (all eight) --out CONFIGS.json
            (exits 1 when the gate fails)
  probes    --lineage r1 --parent P.json --d1l D1L.json --games GAMES.jsonl --bank BANK.json
            --completion-games GAMES.jsonl --completion-bank BANK.json --anchor ANCHOR.json
            --offers OFFERS.json [--run A-s0=RUN/completion.json --run ROLE=...]
            --output-root DIR --out-dir CMDS
            (no --run: the reference probe, {lineage}-probe-ref)
  x3        --lineage r1 --screen-fit FIT.json --probe PROBE.json [--probe ...] --out X3.json
            (exits 1 when X3 fails)
  exactness --lineage r1 --rerun RUN/completion.json --d2l-checkpoint D2L.json
            [--screen SCREEN_RUN/completion.json]
            [--screen-fit FIT.json --probe PROBE.json [--probe ...]] --out X.json
            (exits 1 when X1, or X3 when probes are given, fails)
  analyze   --spec SPEC.json --out RESULT.json
SPEC: {"screen_fit": FIT.json, "screen_index": INDEX.json, "screen_result": RESULT.json,
       "r1": {"screen_config": SCREEN_D2L/CONFIG-prod.json,
              "configs": {"A-s0": RUN/CONFIG-prod.json, ...}, "x1": X.json,
              "runs": {"A-s0": RUN/completion.json, ...}, "probes": [PROBE.json, ...],
              "traces": {"A-s0": TRACE/trace.json, ...}, "panels": [PANEL_ENTRY, ...]},
       "r2": {...}}
X.json is A-s0's ``exactness`` output. A PANEL_ENTRY is an analyzer index
entry for checkpoint ``{lineage}-{role}``, one per opponent for every run but
A-s0.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import re
import statistics
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import phase1  # noqa: E402
import phase2  # noqa: E402
from distill_common import (LINEAGES, absolute, check_parent, pilot, pin, read_json, require,  # noqa: E402
                            write_json)


def screen_analyzer():
    """The screen's analyzer, this directory's analyze.py, loaded by its path:
    other tool directories hold an analyze.py too, and a bare import returns
    whichever one the process imported first."""
    spec = importlib.util.spec_from_file_location("spy_distill_screen_analyze",
                                                  Path(__file__).resolve().parent / "analyze.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


analyze = screen_analyzer()

RUN_SCHEMA = "mtg-kernel-native-expanded-training-run/v1"
ITERATION_SCHEMA = "mtg-kernel-native-expanded-iteration/v1"
PROBE_SCHEMA = "mtg-kernel-archive-distillation-probe/v1"
TRACE_SCHEMA = "mtg-kernel-line-retention-trace/v1"
UPDATES = pilot.UPDATES
A_RUNS = ("A-s0", "A-s1", "A-s2", "A-s3")
N_RUNS = ("N-s0", "N-s1", "N-s2", "N-s3")
RUNS = A_RUNS + N_RUNS
DENSE = ("A-s0", "N-s0")
DENSE_EVERY, SPARSE_EVERY = 6, 18
D1L = "D1L"
# The screen's selected L checkpoint (D1L) in FIT.json.
D1L_CANDIDATE = "L-step-1024"
# The screen's D2L checkpoints (its evidence/index.json), A-s0's endpoints.
D2L_CHECKPOINT_SHA256 = {"r1": "b8b570d591664bb73aaa73f88482c72d1ab56b51458861b1a4cd85ed6672de4d",
                         "r2": "b98565e4e615e35352b47b26d3b78391c0fedd87b164987b9d28dff7d9fec42b"}
# Update record fields X1 compares; checkpoint, collection, episode and
# receipt hashes carry paths and are not compared.
X1_FIELDS = ("after_state_sha256", "adam_step", "physical_decisions", "policy_substeps", "loss", "policy_sum",
             "value_sum", "advantage_statistics")
PROBE_CANDIDATES = 64
RUN_SLOTS = PROBE_CANDIDATES - 2
KEPT, LOST = 0.5, 0.2
CLASSES = ("lost", "partly kept", "kept")
DECISIONS = ("self", "giant")
CHOICE = {"self": "the self-target choice", "giant": "Dread Return on the Giant"}
MIN_MENUS = 10
MIN_EPISODES = 10
WILSON_Z = 1.959963984540054
# Panel role of each run: A-s0's panels are D2L's.
PANEL_ROLE = {**{run: run for run in RUNS}, "A-s0": "D2L"}
PANEL_METRICS = ("J", "combo_resolved", "spy_score", "eight_deck", "nine_deck")
# Opponent kind by repeat j (phase2.py: 0 to 2 the current policy, 3 to 7 the
# named parent, 8 and 9 A48).
EPISODE_J = re.compile(r"-j(\d+)-s[01]$")


def read_pinned(value: dict):
    data = Path(value["path"]).read_bytes()
    require(hashlib.sha256(data).hexdigest() == value["sha256"], f"{value['path']} differs from its pin")
    return json.loads(data)


def pairs(text: str) -> tuple[str, str]:
    role, _, path = text.partition("=")
    require(role in RUNS and path, f"--run takes ROLE=PATH with ROLE one of {RUNS}")
    return role, path


# ---------------------------------------------------------------- updates


def updates(completion: dict) -> list[dict]:
    """A complete 162-update run's updates in order: the update count n, the
    source after it, its output state and Adam step, and its record's X1
    fields."""
    require(completion.get("schema") == RUN_SCHEMA and completion.get("complete") is True
            and completion.get("completed_iterations") == UPDATES == len(completion["iterations"]),
            "not a complete 162-update run")
    receipts = [read_pinned(pin) for pin in completion["iterations"]]
    out = []
    for index, receipt in enumerate(receipts):
        require(receipt.get("schema") == ITERATION_SCHEMA and receipt.get("iteration") == index,
                "iteration receipt out of order")
        after = receipts[index + 1]["source"] if index + 1 < len(receipts) else completion["source"]
        update = read_pinned(receipt["update"])
        require(after["checkpoint"] == update["checkpoint"], "a receipt's next source is not its update's checkpoint")
        identity = receipt["output_identity"]
        require(identity["state_sha256"] == update["after_state_sha256"]
                and identity["adam_step"] == update["adam_step"],
                "a receipt's output identity differs from its update record")
        out.append({"update": index + 1, "source": after, "state_sha256": identity["state_sha256"],
                    "adam_step": identity["adam_step"], "record": {f: update[f] for f in X1_FIELDS}})
    final = completion["actual_identity"]
    require(out[-1]["state_sha256"] == final["state_sha256"] and out[-1]["adam_step"] == final["adam_step"],
            "the last receipt is not the run's endpoint")
    return out


def read_updates(role: str) -> tuple[int, ...]:
    """The updates whose checkpoints a run's probe reads."""
    require(role in RUNS, f"unknown run {role!r}")
    every = DENSE_EVERY if role in DENSE else SPARSE_EVERY
    return tuple(range(every, UPDATES + 1, every))


def candidate_id(role: str, update: int) -> str:
    return f"{role}-u{update:03d}"


def run_candidates(role: str, completion: dict) -> list[dict]:
    chosen = set(read_updates(role))
    return [{"id": candidate_id(role, u["update"]), "source": u["source"]}
            for u in updates(completion) if u["update"] in chosen]


def chunks(candidates: list[dict]) -> list[list[dict]]:
    return [candidates[i:i + RUN_SLOTS] for i in range(0, len(candidates), RUN_SLOTS)]


def probe_commands(lineage: str, parent: dict, d1l: dict, games: Path, bank: Path, completion: tuple[Path, Path],
                   anchor: dict, offers: list, runs: dict, output_root: str) -> list[dict]:
    """Probe commands for the runs given (role -> completion record): the s0
    runs' checkpoints first, then the other runs'. With no runs, the
    reference probe: the parent and D1L alone."""
    check_parent(lineage, parent)
    require(set(runs) <= set(RUNS), "unknown run role")
    dense = [c for role in DENSE if role in runs for c in run_candidates(role, runs[role])]
    sparse = [c for role in RUNS if role in runs and role not in DENSE for c in run_candidates(role, runs[role])]
    groups = chunks(dense) + chunks(sparse) if runs else [[]]
    commands = []
    for index, group in enumerate(groups):
        directory = f"{output_root}/{lineage}-probe-{index if runs else 'ref'}"
        command = phase1.probe_command(lineage, parent, d1l, games, bank, anchor, offers, [], directory, completion)
        command["candidates"] = [{"id": "parent", "source": parent}, {"id": D1L, "source": d1l}] + group
        require(len(command["candidates"]) <= PROBE_CANDIDATES, "a probe reads at most 64 candidates")
        commands.append(command)
    return commands


def trace_command(run_directory: Path, output_directory: str) -> dict:
    """The engine's ``line_retention_trace`` command for a complete 162-update
    run: its ``run.json`` and ``completion.json``, pinned."""
    run_directory = Path(run_directory)
    updates(read_json(run_directory / "completion.json"))
    return {"mode": "line_retention_trace", "run": pin(run_directory / "run.json"),
            "completion": pin(run_directory / "completion.json"), "output_directory": absolute(output_directory)}


# --------------------------------------------------------------- configs

# Fields a run's placement sets; the config gate sets them aside.
PLACEMENT_FIELDS = ("output_directory", "collection_workers", "preparation_workers")


def placement_free(cfg: dict) -> dict:
    return {k: v for k, v in cfg.items() if k not in PLACEMENT_FIELDS}


def learner_deck(item: dict) -> dict:
    episode = item["episode"]
    return episode["selected"][episode["learner_seat"]]


def without(cfg: dict, field: str) -> dict:
    return {k: v for k, v in cfg.items() if k != field}


def replicates(lineage: str, k: int, a: dict, base: dict) -> bool:
    """A-sk differs from A-s0 only in each unit's seed and episode ids, which
    are replicate k's."""
    plan = pilot.schedule(lineage)["updates"]
    if without(a, "iterations") != without(base, "iterations") or not (
            len(a["iterations"]) == len(base["iterations"]) == len(plan)):
        return False
    for entries, new_update, old_update in zip(plan, a["iterations"], base["iterations"]):
        if not (set(new_update) == set(old_update) == {"episodes"}
                and len(entries) == len(new_update["episodes"]) == len(old_update["episodes"])):
            return False
        for entry, new, old in zip(entries, new_update["episodes"], old_update["episodes"]):
            if (without(new, "episode") != without(old, "episode")
                    or {f: v for f, v in new["episode"].items() if f not in ("id", "seed")}
                    != {f: v for f, v in old["episode"].items() if f not in ("id", "seed")}
                    or new["episode"]["id"] != phase2.replicate_id(lineage, k, entry)
                    or new["episode"]["seed"] != phase2.replicate_seed(lineage, k, entry["own"], entry["other"],
                                                                       entry["j"])):
                return False
    return True


def drops(n: dict, a: dict, spy: dict) -> bool:
    """N-sk is A-sk without the Spy learner's 180 episodes, its 162 updates in
    order."""
    kept = [{"episodes": [item for item in update["episodes"] if learner_deck(item) != spy]}
            for update in a["iterations"]]
    dropped = sum(learner_deck(item) == spy for update in a["iterations"] for item in update["episodes"])
    return (without(n, "iterations") == without(a, "iterations") and n["iterations"] == kept
            and dropped == phase2.SPY_LEARNER_EPISODES and len(kept) == UPDATES
            and all(update["episodes"] for update in kept))


def configs(lineage: str, screen: dict, cfgs: dict, decks: list[dict] | None = None) -> dict:
    """The config gate (DESIGN.md section 3), worker counts and output
    directories aside: A-s0's config equals the screen's D2L production
    config, a replicate's differs from A-s0's only in unit seeds and episode
    ids, and an N config is its A config without the Spy learner's
    episodes."""
    require(set(cfgs) == set(RUNS), "all eight configs are required")
    spy = (decks if decks is not None else pilot.load_decks())[pilot.SPY]
    free = {role: placement_free(cfg) for role, cfg in cfgs.items()}
    out = {"A-s0 equals the screen's D2L config": free["A-s0"] == placement_free(screen)}
    for k in phase2.REPLICATES:
        out[f"A-s{k} replicates A-s0"] = replicates(lineage, k, free[f"A-s{k}"], free["A-s0"])
    for k in range(4):
        out[f"N-s{k} drops the Spy learner from A-s{k}"] = drops(free[f"N-s{k}"], free[f"A-s{k}"], spy)
    out["pass"] = all(out.values())
    return out


# ------------------------------------------------------------- exactness


def x1(lineage: str, rerun: dict, d2l_checkpoint: Path, screen: dict | None = None) -> dict:
    """A-s0 against the screen's phase 2: its endpoint state and Adam step
    against D2L's checkpoint and, when the screen's run is given, every update
    record's X1 fields against the screen's."""
    end = phase2.check(lineage, rerun, d2l_checkpoint, D2L_CHECKPOINT_SHA256[lineage])
    out = {"endpoint": end}
    if screen is None:
        # The run log says why the screen's run was not given.
        out.update({"compared": "the endpoint only (no screen run given)", "pass": end["reproduced"]})
        return out
    differ = [{"update": u["update"], "fields": [f for f in X1_FIELDS if u["record"][f] != v["record"][f]]}
              for u, v in zip(updates(rerun), updates(screen)) if u["record"] != v["record"]]
    out.update({"compared": "every update record and the endpoint", "updates": UPDATES,
                "differing_updates": differ, "pass": end["reproduced"] and not differ})
    return out


def probe_readouts(probe: dict) -> dict:
    require(probe.get("schema") == PROBE_SCHEMA and probe.get("complete") is True, "not a complete probe")
    out = {c["id"]: c for c in probe["candidates"]}
    require(len(out) == len(probe["candidates"]), "duplicate probe candidate")
    return out


def fit_d1l(screen_fit: dict, lineage: str) -> dict:
    read = screen_fit["fits"][lineage]["L"]["read"]
    entry = [r for r in read if r["candidate"] == D1L_CANDIDATE]
    require(len(entry) == 1, "FIT.json holds no single D1L readout")
    return entry[0]


def bank_group(readout: dict, role: str, name: str) -> dict | None:
    return ((readout["archive"].get(role) or {}).get("groups") or {}).get(name)


def group_value(readout: dict, role: str, name: str, field: str = "mean_deployed_probability"):
    rows = bank_group(readout, role, name)
    return rows[field] if rows else None


def self_path(readout: dict):
    return readout["learner_offers"]["mean_deployed_self_probability"]


def giant_path(readout: dict):
    return group_value(readout, "training", "giant_chosen")


def context(readout: dict) -> dict:
    """Readouts reported beside the paths, entering no reading."""
    held_out = readout["anchor_held_out"]
    return {"giant_summed_mass": group_value(readout, "training", "giant_chosen", "mean_deployed_giant_probability"),
            "g1_self": group_value(readout, "training", "self_chosen"),
            "completion_giant": group_value(readout, "completion", "giant_chosen"),
            "completion_self": group_value(readout, "completion", "self_chosen"),
            "anchor_non_spy_kl": held_out["mean_kl_from_parent_non_spy"],
            "anchor_non_spy_top_agreement": held_out["non_spy_top_agreement"]}


def x3(lineage: str, probes: list[dict], screen_fit: dict) -> dict:
    """The parent's readouts identical in every probe; D1L's state and paths
    equal to the screen's FIT.json readout."""
    readouts = [probe_readouts(p) for p in probes]
    require(readouts, "X3 needs at least one probe")
    parents = [r["parent"] for r in readouts]
    parent_same = all(p == parents[0] for p in parents)
    reference = fit_d1l(screen_fit, lineage)
    d1l_ok = []
    for r in readouts:
        d = r[D1L]
        d1l_ok.append(d["state_sha256"] == reference["state_sha256"]
                      and self_path(d) == reference["learner_offers"]["mean_deployed_self_probability"]
                      and giant_path(d) == reference["G1"]["giant"]["mean_deployed_probability"]
                      and group_value(d, "training", "self_chosen")
                      == reference["G1"]["self"]["mean_deployed_probability"])
    return {"probes": len(probes), "parent_identical": parent_same, "d1l_equals_fit": all(d1l_ok),
            "pass": parent_same and all(d1l_ok)}


# ----------------------------------------------------------------- paths


def retained_class(rho) -> str | None:
    if rho is None:
        return None
    return "kept" if rho >= KEPT else "lost" if rho <= LOST else "partly kept"


def paths(lineage: str, probes: list[dict], runs: dict) -> dict:
    """Per run: s and g with the context at every read update, D1L's s and g,
    rho and class."""
    merged = {}
    for probe in probes:
        require(probe["lineage"] == lineage, "probe from another lineage")
        for cid, readout in probe_readouts(probe).items():
            merged.setdefault(cid, readout)
    d1l = merged[D1L]
    base = {"self": self_path(d1l), "giant": giant_path(d1l)}
    out = {"d1l": base, "d1l_context": context(d1l), "runs": {}}
    for role, completion in runs.items():
        identities = {u["update"]: u for u in updates(completion)}
        series = []
        for n in read_updates(role):
            readout = merged.get(candidate_id(role, n))
            require(readout is not None, f"{role} update {n} was not probed")
            require(readout["state_sha256"] == identities[n]["state_sha256"]
                    and readout["adam_step"] == identities[n]["adam_step"],
                    f"{role} update {n}: the probe read another state")
            series.append({"update": n, "self": self_path(readout), "giant": giant_path(readout),
                           "context": context(readout)})
        end = series[-1]
        rho = {d: (end[d] / base[d] if base[d] and end[d] is not None else None) for d in DECISIONS}
        out["runs"][role] = {"series": series, "endpoint": {d: end[d] for d in DECISIONS}, "rho": rho,
                             "class": {d: retained_class(rho[d]) for d in DECISIONS}}
    return out


def time_course(run_paths: dict, base: dict, trace: dict | None = None) -> dict:
    """First read update n at which rho falls below 0.5 and below 0.2 (s0
    runs), with A-s0's summed push and slope over updates 1 to n: update n's
    step is in the state read at n. Trace iteration i is update i + 1."""
    out = {}
    for d in DECISIONS:
        first = {}
        for name, limit in (("below_0.5", KEPT), ("below_0.2", LOST)):
            n = next((p["update"] for p in run_paths["series"]
                      if base[d] and p[d] is not None and p[d] / base[d] < limit), None)
            entry = {"update": n}
            if trace is not None and n is not None:
                through = [it for it in trace["iterations"] if it["iteration"] < n]
                entry["push_through"] = sum(it["push"][d]["total"] for it in through)
                entry["slope_through"] = sum(it["push"][d]["slope_total"] for it in through)
            first[name] = entry
        out[d] = first
    return out


# ---------------------------------------------------------------- traces


def wilson(k: int, n: int):
    if n == 0:
        return None
    p, z2 = k / n, WILSON_Z * WILSON_Z
    centre = (p + z2 / (2 * n)) / (1 + z2 / n)
    half = WILSON_Z * math.sqrt(p * (1 - p) / n + z2 / (4 * n * n)) / (1 + z2 / n)
    return [centre - half, centre + half]


def won(episode: dict) -> bool:
    """A win; a draw (terminal return 0) counts as not won."""
    return episode["terminal_return"] > 0


def share(episodes: list) -> dict:
    k, n = sum(map(won, episodes)), len(episodes)
    return {"episodes": n, "won": k, "win_share": k / n if n else None, "wilson95": wilson(k, n)}


def read_trace(path) -> tuple[dict, list, list]:
    trace = read_json(path)
    require(trace.get("schema") == TRACE_SCHEMA and trace.get("complete") is True and trace.get("x2") == "passed"
            and len(trace["iterations"]) == UPDATES, "not a complete trace that passed X2")

    def lines(pin):
        data = Path(pin["path"]).read_bytes()
        require(hashlib.sha256(data).hexdigest() == pin["sha256"], f"{pin['path']} differs from its pin")
        return [json.loads(line) for line in data.splitlines() if line.strip()]
    return trace, lines(trace["rows"]), lines(trace["episodes"])


def median(values):
    return statistics.median(values) if values else None


def negative_share(values):
    return sum(v < 0 for v in values) / len(values) if values else None


def menu_summary(rows: list) -> dict:
    """Chosen line menus, split by the game's result."""
    out = {}
    for result, keep in (("won", lambda r: r["terminal_return"] > 0), ("lost", lambda r: r["terminal_return"] <= 0)):
        part = [r for r in rows if r["chosen"] and keep(r)]
        normalized = [r["normalized_advantage"] for r in part]
        raw = [r["raw_advantage"] for r in part]
        out[result] = {"menus": len(part),
                       "median_normalized_advantage": median(normalized),
                       "negative_normalized_share": negative_share(normalized),
                       "median_raw_advantage": median(raw), "negative_raw_share": negative_share(raw),
                       "median_td_error": median([r["td_error"] for r in part]),
                       "median_decisions_left": median([r["decisions_left"] for r in part])}
    return out


def opponent_kind(episode_id: str) -> str:
    match = EPISODE_J.search(episode_id)
    require(match is not None, f"episode id {episode_id!r} names no repeat j")
    j = int(match.group(1))
    return "current" if j <= 2 else "parent" if j <= 7 else "A48"


def splits(chosen: list, declined: list) -> dict:
    """Chosen and declined win shares by opponent kind, learner seat and
    whether the learner went first."""
    keys = {"opponent": lambda e: opponent_kind(e["episode"]),
            "learner_seat": lambda e: str(e["learner_seat"]),
            "learner_first": lambda e: str(e["starting_player"] == e["learner_seat"]).lower()}
    out = {}
    for name, key in keys.items():
        values = sorted({key(e) for e in chosen + declined})
        out[name] = {v: {"chosen": share([e for e in chosen if key(e) == v]),
                         "declined": share([e for e in declined if key(e) == v])} for v in values}
    return out


def push_sums(traces: list[dict]) -> dict:
    parts = ("chosen_won", "chosen_lost", "declined_won", "declined_lost")
    out = {}
    for d in DECISIONS:
        out[d] = {p: {key: sum(it["push"][d][p][key] for t in traces for it in t["iterations"])
                      for key in ("push", "slope", "menus")} for p in parts}
        out[d]["total"] = sum(out[d][p]["push"] for p in parts)
        out[d]["slope_total"] = sum(out[d][p]["slope"] for p in parts)
    return out


def trace_summary(traces: list[tuple[dict, list, list]]) -> dict:
    """Reading 3's pooled description of the A runs' traces. An episode chose
    a decision when it chose it at any offer, and declined it when it was
    offered it and never chose it."""
    episodes = [e for _, _, eps in traces for e in eps]
    rows = [r for _, rs, _ in traces for r in rs]
    out = {"spy_learner_episodes": len(episodes), "push": push_sums([t for t, _, _ in traces])}
    for d in DECISIONS:
        offered = [e for e in episodes if e[d]["offered"] > 0]
        chosen = [e for e in offered if e[d]["chosen"] > 0]
        declined = [e for e in offered if e[d]["chosen"] == 0]
        out[d] = {"offered": share(offered), "chosen": share(chosen), "declined": share(declined),
                  "splits": splits(chosen, declined),
                  "chosen_menus": menu_summary([r for r in rows if r["decision"] == d])}
    out["self"]["line_completed"] = share([e for e in episodes if e["line_completed"]])
    return out


def timeline(trace: dict) -> list[dict]:
    return [{"update": it["iteration"] + 1, "spy_learner_episodes": it["spy_learner_episodes"],
             **{d: {"push": it["push"][d]["total"], "slope": it["push"][d]["slope_total"]} for d in DECISIONS}}
            for it in trace["iterations"]]


# ---------------------------------------------------------------- panels


def panel_ids(lineage: str) -> set:
    return {f"{lineage}-{role}" for role in (D1L, *PANEL_ROLE.values())}


def panel_contrasts(present: set) -> tuple:
    return tuple((f"{PANEL_ROLE[run]}-{D1L}", PANEL_ROLE[run], D1L) for run in RUNS if PANEL_ROLE[run] in present)


def panel_reading(intervals: list) -> str:
    """One endpoint's J against D1L, pooled, under both bounds."""
    if all(high < 0 for _, high in intervals):
        return "behavioral loss of the line established"
    if all(low > 0 for low, _ in intervals):
        return "behavioral gain of the line established"
    return "no change established"


def x4(ci: dict, pts: dict, screen_result: dict, lineages) -> dict:
    """The screen's D2L-D1L points and intervals for every panel metric,
    against H, against T1 and pooled, under both bounds, reproduced
    exactly."""
    keys = [(lineage, f"D2L-{D1L}", metric, opp, bound) for lineage in lineages for metric in PANEL_METRICS
            for opp in (*analyze.OPPONENTS, "pooled") for bound in analyze.BOUNDS]
    differ = ["/".join(key) for key in keys
              if key not in ci or screen_result["contrasts"].get("/".join(key)) != {"point": pts[key], "ci95": ci[key]}]
    return {"compared": len(keys), "differing": differ, "pass": not differ}


def panels(screen_index: dict, screen_result: dict, entries: dict) -> dict:
    """X4, then each endpoint's direct contrast with D1L over the same panel
    games, by the screen's analyzer: J and combo_resolved for the line, and
    the Spy, eight-deck and nine-deck scores as win-rate effects."""
    allowed = set().union(*(panel_ids(lin) for lin in LINEAGES))
    reused = [p for p in screen_index["panels"] if p["checkpoint"].split("-", 1)[1] in (D1L, "D2L")]
    index = {"panels": reused + [e for lin in LINEAGES for e in entries.get(lin, [])]}
    data = analyze.load(index, allowed)
    present = {key[0].split("-", 1)[1] for key in data}
    todo = analyze.plan(data, panel_contrasts(present))
    ci = analyze.bootstrap(data, todo, PANEL_METRICS)
    pts = analyze.points(data, todo, PANEL_METRICS)
    lineages = [lin for lin in LINEAGES if lin in entries]
    exact = x4(ci, pts, screen_result, lineages)
    require(exact["pass"], f"X4 failed: {exact['differing']}")
    out = {"x4": exact}
    for lineage, name, d, _ in todo:
        entry = {"metrics": {}}
        for metric in PANEL_METRICS:
            entry["metrics"][metric] = {bound: {"point": pts[(lineage, name, metric, "pooled", bound)],
                                                "interval": ci[(lineage, name, metric, "pooled", bound)]}
                                        for bound in analyze.BOUNDS}
        entry["reading"] = panel_reading([ci[(lineage, name, "J", "pooled", bound)] for bound in analyze.BOUNDS])
        entry["win_rate"] = {metric: analyze.agree({bound: analyze.effect_reading(
            ci[(lineage, name, metric, "pooled", bound)]) for bound in analyze.BOUNDS})
            for metric in analyze.WIN_RATE_METRICS}
        entry["secondary"] = {opp: analyze.secondary(data[(f"{lineage}-{d}", opp)]) for opp in analyze.OPPONENTS}
        entry["self_target_chosen_games"] = sum(s["self_target"]["chosen_games"] for s in entry["secondary"].values())
        out.setdefault(lineage, {})[d] = entry
    return out


# -------------------------------------------------------------- readings

AT_STATES = "this decision's probability at the fixed states"
READING1 = {"reliably lost": f"ordinary training from D1L reliably lost {AT_STATES}, in this lineage",
            "lost 3 of 4": f"ordinary training from D1L lost {AT_STATES} in 3 of 4 runs, in this lineage",
            "reliably kept": f"ordinary training from D1L reliably kept {AT_STATES}, in this lineage",
            "kept 3 of 4": f"ordinary training from D1L kept {AT_STATES} in 3 of 4 runs, in this lineage",
            "partly kept": f"ordinary training from D1L partly kept {AT_STATES}, in this lineage",
            "other": "run-dependent at four runs, in this lineage"}
LOSS = f"the loss of {AT_STATES}"
READING2 = {"needed": f"{LOSS} needed the Spy learner's own games, in this lineage",
            "not_needed": f"{LOSS} did not need the Spy learner's own games, in this lineage",
            "other": "not separated at four seeds, in this lineage",
            "none": "no loss to attribute"}
READING2_NOTE = "The seed pairs part at update 0, so the rows compare how many runs lost."
READING2_EXPLANATIONS = [
    "dropping the units removes the Spy games' policy term and their value term together, and it shrinks the "
    "updates that held them",
    "each N run's later collections differ from its A run's through the policy",
    "the optimizer state D1L inherits from the parent acts in A and N alike: its first moment carries the "
    "parent's recent gradients into the first updates, and its second moment largely sets every step size",
    "\"needed\" does not say which of these mattered; \"did not need\" does not say which other deck's games or "
    "which part of the update did it"]
READING3_EXPLANATIONS = [
    "the push and the slope are first order: they ignore Adam's per-parameter scaling, shared parameters and the "
    "value term",
    "the learner's states are not the offer set's",
    "a negative normalized advantage in a won game can come from the value head's estimates along the line, from "
    "lambda's discount of the distant terminal reward over many decisions, or from the update's normalization, "
    "which subtracts the update's mean; the trace shows the raw and normalized advantages, the TD errors and the "
    "decisions left side by side without separating them",
    "the line is chosen and declined in different states and against different opponents, so a win-share "
    "difference is not the line's effect; the summary splits the shares by opponent kind, seat and who went first",
    "a win share among the learner's own games is not playing strength: a line win can replace a win by another "
    "route"]
READING3_RAW_EXPLANATIONS = [
    "a negative raw advantage in a won game can come from the value head's estimates along the line or from "
    "lambda's discount of the distant terminal reward over many decisions; the trace does not separate them"]


def class_counts(classes: dict, runs) -> dict:
    values = [classes.get(run) for run in runs]
    return {c: sum(v == c for v in values) for c in CLASSES} | {"unread": sum(v is None for v in values)}


def reading1(classes: dict) -> dict:
    """classes: run -> kept, partly kept or lost (or None)."""
    counts = class_counts(classes, A_RUNS)
    if counts["lost"] == 4:
        key = "reliably lost"
    elif counts["lost"] == 3:
        key = "lost 3 of 4"
    elif counts["kept"] == 4:
        key = "reliably kept"
    elif counts["kept"] == 3:
        key = "kept 3 of 4"
    elif counts["partly kept"] >= 3:
        key = "partly kept"
    else:
        key = "other"
    return {"reading": READING1[key], "counts": counts}


def reading2(classes: dict) -> dict:
    """classes: run -> class, for all eight runs; N-sk pairs with A-sk."""
    counts = {"A": class_counts(classes, A_RUNS), "N": class_counts(classes, N_RUNS)}
    a_lost = [k for k in range(4) if classes.get(f"A-s{k}") == "lost"]
    n_lost = [k for k in range(4) if classes.get(f"N-s{k}") == "lost"]
    if len(a_lost) < 3:
        key = "none"
    elif (sum(classes.get(f"N-s{k}") == "kept" for k in a_lost) >= 2 and not set(n_lost) & set(a_lost)
          and len(n_lost) < len(a_lost)):
        key = "needed"
    elif len(n_lost) >= len(a_lost):
        key = "not_needed"
    else:
        key = "other"
    return {"reading": READING2[key], "counts": counts}


def credited_statement(summary: dict, decision: str) -> str:
    won_menus = summary[decision]["chosen_menus"]["won"]
    if won_menus["menus"] < MIN_MENUS:
        return "too few won line attempts to read"
    if won_menus["median_normalized_advantage"] < 0:
        return f"won line attempts were credited negatively at {CHOICE[decision]}"
    return f"won line attempts were not credited negatively at {CHOICE[decision]}"


def raw_statement(summary: dict, decision: str) -> str:
    won_menus = summary[decision]["chosen_menus"]["won"]
    if won_menus["menus"] < MIN_MENUS:
        return "too few won line attempts to read"
    if won_menus["median_raw_advantage"] < 0:
        return f"won line attempts drew a negative raw advantage at {CHOICE[decision]}"
    return f"won line attempts did not draw a negative raw advantage at {CHOICE[decision]}"


def attempts_statement(summary: dict, decision: str) -> str:
    chosen, declined = summary[decision]["chosen"], summary[decision]["declined"]
    if chosen["episodes"] < MIN_EPISODES or declined["episodes"] < MIN_EPISODES:
        return "too few to read"
    if chosen["wilson95"][1] < declined["wilson95"][0]:
        return f"the learner's line attempts at {CHOICE[decision]} won less than its declines"
    if chosen["wilson95"][0] > declined["wilson95"][1]:
        return f"the learner's line attempts at {CHOICE[decision]} won more than its declines"
    return "not separated"


IN_LINEAGE = ", in this lineage"
# Wordings that say a reading does not apply in a lineage.
NOT_APPLICABLE = (READING2["none"], "too few won line attempts to read", "too few to read")


def across(per: dict) -> str:
    """A reading holds across lineages when both give the same wording, then
    read "in both lineages". Where one lineage's reading does not apply and
    the other's does, it applies in that other lineage only, with its
    wording. Any other difference is lineage-dependent."""
    values = {lin: per.get(lin) for lin in LINEAGES}
    if any(v is None for v in values.values()):
        missing = [lin for lin, v in values.items() if v is None]
        return f"not read in {', '.join(missing)}"
    first = values[LINEAGES[0]]
    if all(v == first for v in values.values()):
        return first.removesuffix(IN_LINEAGE) + ", in both lineages"
    applies = [lin for lin, v in values.items() if v not in NOT_APPLICABLE]
    if len(applies) == 1:
        return f"applies in {applies[0]} only: {values[applies[0]].removesuffix(IN_LINEAGE)}"
    return "lineage-dependent"


def panel_counts(lineage_panels: dict, runs) -> dict:
    readings = [(lineage_panels.get(PANEL_ROLE[run]) or {}).get("reading") for run in runs]
    zero = sum(1 for run in runs
               if (lineage_panels.get(PANEL_ROLE[run]) or {}).get("self_target_chosen_games") == 0)
    return {"loss": sum(r == "behavioral loss of the line established" for r in readings),
            "gain": sum(r == "behavioral gain of the line established" for r in readings),
            "no change": sum(r == "no change established" for r in readings),
            "unread": sum(r is None for r in readings),
            "endpoints choosing the self-target in no panel game": zero}


# ---------------------------------------------------------------- binding


def read_with_sha256(path) -> tuple[dict, str]:
    data = Path(path).read_bytes()
    return json.loads(data), hashlib.sha256(data).hexdigest()


def episode_keys(cfg: dict) -> list:
    """Each update's episode ids and seeds, in order."""
    return [[(item["episode"]["id"], item["episode"]["seed"]) for item in update["episodes"]]
            for update in cfg["iterations"]]


def spy_learner_ids(cfg: dict, spy: dict) -> list:
    return sorted(item["episode"]["id"] for update in cfg["iterations"] for item in update["episodes"]
                  if learner_deck(item) == spy)


def bind_run(role: str, completion_sha256: str, trace: tuple, config: dict, spy: dict) -> None:
    """A run and its trace are its role's: the trace reads this completion,
    the run's manifest plays the role's config's episodes (ids and seeds,
    update by update), and the traced Spy learner episodes are the config's:
    180 in an A run, none in an N run."""
    summary, _, episodes = trace
    require(summary["completion"]["sha256"] == completion_sha256, f"{role}: the trace reads another run")
    manifest = read_pinned(summary["run"])
    require(episode_keys(manifest["config"]) == episode_keys(config),
            f"{role}: the run did not play its config's episodes")
    expected = spy_learner_ids(config, spy)
    require(len(expected) == (phase2.SPY_LEARNER_EPISODES if role in A_RUNS else 0),
            f"{role}: the config is not its arm's")
    require(summary["spy_learner_episodes"] == len(expected) and sorted(e["episode"] for e in episodes) == expected,
            f"{role}: the traced Spy learner episodes are not the run's")


def bind_x1(lineage: str, result: dict, completion_sha256: str) -> dict:
    """A-s0 takes D2L's panels only once X1 holds for this A-s0 run."""
    require(result.get("lineage") == lineage and (result.get("rerun") or {}).get("sha256") == completion_sha256,
            f"{lineage}: the X1 result is not this A-s0 run's")
    require(result["x1"]["pass"], f"{lineage}: X1 failed")
    return {"compared": result["x1"]["compared"], "endpoint": result["x1"]["endpoint"], "pass": True}


def bind_panels(lineage: str, entries: list, runs: dict) -> None:
    """Every run but A-s0 has one new panel per opponent, played by its own
    endpoint."""
    seen = set()
    for entry in entries:
        role = entry["checkpoint"].removeprefix(f"{lineage}-")
        require(entry["checkpoint"] == f"{lineage}-{role}" and role in RUNS and PANEL_ROLE[role] == role,
                f"{entry['checkpoint']}: not one of this lineage's new panels")
        require(entry["checkpoint_sha256"] == runs[role]["source"]["checkpoint"]["sha256"],
                f"{entry['checkpoint']}-vs-{entry['opponent']}: not that run's endpoint")
        seen.add((role, entry["opponent"]))
    missing = [f"{role}-vs-{opp}" for role in RUNS if PANEL_ROLE[role] == role for opp in analyze.OPPONENTS
               if (role, opp) not in seen]
    require(not missing, f"{lineage}: no panel for {missing}")


def analyze_spec(spec: dict, decks: list[dict] | None = None) -> dict:
    screen_fit = read_json(spec["screen_fit"])
    spy = (decks if decks is not None else pilot.load_decks())[pilot.SPY]
    result = {"schema": "spy-line-retention-v1/result", "lineages": {}}
    panel_entries = {}
    for lineage in LINEAGES:
        if lineage not in spec:
            continue
        part = spec[lineage]
        for field in ("configs", "runs", "traces"):
            require(set(part[field]) == set(RUNS), f"{lineage}: every run's {field} are required")
        cfgs = {role: read_json(path) for role, path in part["configs"].items()}
        gate = configs(lineage, read_json(part["screen_config"]), cfgs, decks)
        require(gate["pass"], f"{lineage}: the config gate failed")
        loaded = {role: read_with_sha256(path) for role, path in part["runs"].items()}
        runs = {role: value for role, (value, _) in loaded.items()}
        exact = bind_x1(lineage, read_json(part["x1"]), loaded["A-s0"][1])
        traces = {role: read_trace(path) for role, path in part["traces"].items()}
        for role in RUNS:
            bind_run(role, loaded[role][1], traces[role], cfgs[role], spy)
        bind_panels(lineage, part["panels"], runs)
        probes = [read_json(p) for p in part["probes"]]
        x3_result = x3(lineage, probes, screen_fit)
        require(x3_result["pass"], f"{lineage}: X3 failed")
        p = paths(lineage, probes, runs)
        classes = {d: {role: p["runs"][role]["class"][d] for role in RUNS} for d in DECISIONS}
        summary = trace_summary([traces[role] for role in A_RUNS])
        summary["push_by_run"] = {role: push_sums([traces[role][0]]) for role in A_RUNS}
        result["lineages"][lineage] = {
            "configs": gate, "x1": exact, "x3": x3_result, "paths": p,
            "time_course": {role: time_course(p["runs"][role], p["d1l"],
                                              traces[role][0] if role in A_RUNS else None) for role in DENSE},
            "timeline": timeline(traces["A-s0"][0]),
            "reading1": {d: reading1(classes[d]) for d in DECISIONS},
            "reading2": {d: reading2(classes[d]) for d in DECISIONS},
            "reading2_note": READING2_NOTE, "reading2_explanations": READING2_EXPLANATIONS,
            "reading3": {"summary": summary,
                         **{f"{name}_{d}": statement(summary, d) for d in DECISIONS
                            for name, statement in (("credited", credited_statement), ("raw", raw_statement),
                                                    ("attempts", attempts_statement))},
                         "explanations": READING3_EXPLANATIONS, "raw_explanations": READING3_RAW_EXPLANATIONS}}
        panel_entries[lineage] = part["panels"]
    panel_result = panels(read_json(spec["screen_index"]), read_json(spec["screen_result"]), panel_entries)
    result["x4"] = panel_result["x4"]
    for lineage, entry in result["lineages"].items():
        lineage_panels = panel_result.get(lineage, {})
        unread = [run for run in RUNS if PANEL_ROLE[run] not in lineage_panels]
        require(not unread, f"{lineage}: no panel contrast for {unread}")
        entry["panels"] = lineage_panels
        # J's reading, with combo_resolved and the self-target counts beside
        # it: a change in J can be one in playing the line or in winning
        # with it.
        entry["panel_readings"] = {run: {"J": lineage_panels[PANEL_ROLE[run]]["reading"],
                                         "combo_resolved": lineage_panels[PANEL_ROLE[run]]["metrics"]["combo_resolved"],
                                         "self_target": {opp: s["self_target"] for opp, s in
                                                         lineage_panels[PANEL_ROLE[run]]["secondary"].items()}}
                                   for run in RUNS}
        entry["panel_counts"] = {"A": panel_counts(lineage_panels, A_RUNS), "N": panel_counts(lineage_panels, N_RUNS)}
    per = result["lineages"]
    result["across"] = {f"reading{k}": {d: across({lin: per[lin][f"reading{k}"][d]["reading"] for lin in per})
                                        for d in DECISIONS} for k in (1, 2)}
    result["across"]["reading3"] = {f"{name}_{d}": across({lin: per[lin]["reading3"][f"{name}_{d}"] for lin in per})
                                    for name in ("credited", "raw", "attempts") for d in DECISIONS}
    return result


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    p = sub.add_parser("probes")
    p.add_argument("--lineage", required=True, choices=LINEAGES)
    for name in ("parent", "d1l", "games", "bank", "completion-games", "completion-bank", "anchor", "offers"):
        p.add_argument(f"--{name}", type=Path, required=True)
    p.add_argument("--run", type=pairs, action="append", default=[])
    p.add_argument("--output-root", required=True)
    p.add_argument("--out-dir", type=Path, required=True)
    r = sub.add_parser("x3")
    r.add_argument("--lineage", required=True, choices=LINEAGES)
    r.add_argument("--screen-fit", type=Path, required=True)
    r.add_argument("--probe", type=Path, action="append", required=True)
    r.add_argument("--out", type=Path, required=True)
    t = sub.add_parser("trace-command")
    t.add_argument("--run-directory", type=Path, required=True)
    t.add_argument("--output-directory", required=True)
    t.add_argument("--out", type=Path, required=True)
    c = sub.add_parser("configs")
    c.add_argument("--lineage", required=True, choices=LINEAGES)
    c.add_argument("--screen", type=Path, required=True)
    c.add_argument("--config", type=pairs, action="append", required=True)
    c.add_argument("--out", type=Path, required=True)
    x = sub.add_parser("exactness")
    x.add_argument("--lineage", required=True, choices=LINEAGES)
    x.add_argument("--rerun", type=Path, required=True)
    x.add_argument("--d2l-checkpoint", type=Path, required=True)
    x.add_argument("--screen", type=Path)
    x.add_argument("--screen-fit", type=Path)
    x.add_argument("--probe", type=Path, action="append")
    x.add_argument("--out", type=Path, required=True)
    a_ = sub.add_parser("analyze")
    a_.add_argument("--spec", type=Path, required=True)
    a_.add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "trace-command":
        print(json.dumps(write_json(a.out, trace_command(a.run_directory, a.output_directory))))
        return 0
    if a.op == "configs":
        cfgs = dict(a.config)
        require(len(cfgs) == len(a.config), "a run role is given twice")
        result = {"lineage": a.lineage, **configs(a.lineage, read_json(a.screen),
                                                  {r: read_json(path) for r, path in cfgs.items()})}
        print(json.dumps(write_json(a.out, result)))
        return 0 if result["pass"] else 1
    if a.op == "probes":
        runs = dict(a.run)
        require(len(runs) == len(a.run), "a run role is given twice")
        commands = probe_commands(a.lineage, read_json(a.parent), read_json(a.d1l), a.games, a.bank,
                                  (a.completion_games, a.completion_bank), read_json(a.anchor),
                                  read_json(a.offers), {r: read_json(path) for r, path in runs.items()},
                                  a.output_root)
        pins = [write_json(a.out_dir / f"{c['output_directory'].rsplit('/', 1)[-1]}.command.json", c)
                for c in commands]
        print(json.dumps({"commands": pins}))
        return 0
    if a.op == "x3":
        result = {"lineage": a.lineage, "probes": [pin(p) for p in a.probe],
                  "x3": x3(a.lineage, [read_json(p) for p in a.probe], read_json(a.screen_fit))}
        result["pass"] = result["x3"]["pass"]
        print(json.dumps(write_json(a.out, result)))
        return 0 if result["pass"] else 1
    if a.op == "exactness":
        require(bool(a.probe) == bool(a.screen_fit), "X3 takes the probes and the screen's FIT.json together")
        screen = read_json(a.screen) if a.screen else None
        result = {"lineage": a.lineage, "rerun": pin(a.rerun), "screen": pin(a.screen) if a.screen else None,
                  "x1": x1(a.lineage, read_json(a.rerun), a.d2l_checkpoint, screen)}
        # The first wave checks X1 alone; X3 needs the lineage's probes.
        if a.probe:
            result["x3"] = x3(a.lineage, [read_json(p) for p in a.probe], read_json(a.screen_fit))
        result["pass"] = all(part["pass"] for part in (result["x1"], result.get("x3", {"pass": True})))
        print(json.dumps(write_json(a.out, result)))
        return 0 if result["pass"] else 1
    print(json.dumps(write_json(a.out, analyze_spec(read_json(a.spec)))))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
