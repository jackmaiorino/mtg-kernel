"""Search-evidence distillation (SD1): panel analysis and strength readings.

collab LANES/search-evidence-distillation-20261016, design sections 6 and 7.2.
Offline: nothing here plays, searches or trains.

Per lineage that ran phase 2, 25 endpoints each have a fresh panel against T1
and against H under the seed namespace sd1-panel-v1: the parent, D1-0 to
D1-7, D2-0 to D2-7 and O-0 to O-7. The distillation screen's analyzer
(analyze.py) reads every panel, with its row, seed, roster and label checks,
its metrics and its missing-case fills. Its cluster bootstrap runs here with
SD1's own fixed seed: repeat seat-pairs resampled within each ordered deck
pair, one draw of repeat indices per draw shared by every panel, opponent,
checkpoint and lineage, 10,000 draws, bounds at the 2.5% and 97.5% quantiles.

X4 comes first. Before any SD1 panel is read, the screen's analyzer recomputes
every contrast of the screen's RESULT.json from the screen's own panels with
the screen's seed, and each point and interval must be equal.

Custody binds each panel to its endpoint by checkpoint file SHA-256:
  - the parent's recorded checkpoint;
  - each seed's selected D1 (sd1.py select), whose source file must name it;
  - each phase-2 run's final checkpoint;
  - O-0's, which is stage 0's O-s0.
A run counts only as the run its name says. Its plan, the run.json beside
its completion.json, must give every unit its seed's episode id and seed,
unit by unit in the pilot's schedule: O's own for seed 0 (D2-0 and O-0), and
sd1-LIN-repK's for seed K of 1 to 7. D2-k's initial source must be D1-k's
source verbatim, and O-k's the parent's. Each panel's rows and labels are
verified against their pins before the screen's analyzer reads them.
RESULT.json's "inputs" pins everything read: the spec, the screen's index and
result, X4's record and, per lineage that ran phase 2, each D1's selection
and source, each run's completion.json and run.json, and every panel's rows
and labels.

Contrasts, each with eight per-pair values (k = 0 to 7):
  d       D2-k minus O-k (primary)
  e       D1-k minus the parent (secondary)
  f       d_k minus e_k, the change across phase 2
  D2-D1   D2-k minus D1-k (descriptive)

For every contrast, metric and opponent set (T1, H, and pooled: the mean of
the two opponents' values), there are two 95% intervals of the eight-pair
mean:
  - the t interval over the eight per-pair values (7 degrees of freedom);
  - the cluster-bootstrap interval of the eight-pair mean from the joint draws.

Both are computed under two fills of missing cases:
  - least favorable to D: D's missing cases at 0, the control's at 1;
  - most favorable to D: the reverse.
For f, the least favorable fill is d's least favorable minus e's most
favorable, and the reverse.

Readings (section 7.2) cover the nine-deck, eight-deck and Spy scores. Under
each fill, a contrast is:
  - "improvement" when both lower bounds are above 0;
  - "harm" when both upper bounds are below 0;
  - "no difference" otherwise.
The reading then combines the two fills:
  - "improvement established" or "harm established" when both fills agree on it;
  - "inconclusive: missing cases decide" when they differ;
  - "no difference established" otherwise, noting an interval that alone
    excluded 0.

f uses its own words: "effect larger after phase 2 established", "effect
smaller after phase 2 established", "inconclusive: missing cases decide", or
"change across phase 2 not established".

Per-deck scores, J and the line metrics are descriptive.

The primary is positive in a lineage when all three hold:
  - its pooled nine-deck d reads "improvement established";
  - under the fill least favorable to D2, the mean of the eight per-pair
    nine-deck differences against H alone is above 0;
  - under that fill, the pooled eight-deck d does not read "harm".
The method screen is positive only when both lineages ran phase 2 and both
are positive.

CLI (JSON summary on stdout; a failed X4 exits 1 after writing its record):
  x4      --screen-index INDEX.json --screen-result RESULT.json --out X4.json
  analyze --spec SPEC.json --out RESULT.json

SPEC:
  {"screen_index": PIN, "screen_result": PIN,
   "x4": PIN (this tool's x4 output, which must pass on those two files),
   "lineages": {
     "r1": {"select": [PIN, ...] (sd1.py select, seeds 0 to 7),
            "o_s0": PIN (O-s0's completion.json),
            "phase2": {"D2-0": PIN, ..., "D2-7": PIN, "O-1": PIN, ..., "O-7": PIN}
                      (each run's completion.json),
            "panels": [{"checkpoint": "r1-D2-3", "opponent": "T1" | "H",
                        "checkpoint_sha256": SHA, "rows": PIN, "labels": PIN}, ...]},
     "r2": {"not_run": "fit" | "engineering"}}}
PIN is {"path": PATH, "sha256": SHA}.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import bt1  # noqa: E402
import phase2  # noqa: E402
import sd1  # noqa: E402
from distill_common import (LINEAGES, PARENT_CHECKPOINT_SHA256, absolute, pilot, pin, read_json,  # noqa: E402
                            require, write_json)

# The screen's analyzer, this directory's analyze.py, as BT1 loads it by its
# path: other tool directories hold an analyze.py too.
analyze = bt1.analyze
SCHEMA = "sd1-v1/panels"
X4_SCHEMA = "sd1-v1/x4"
# SD1's own bootstrap seed (section 6: "a new fixed seed").
BOOT_SEED = 2026101601
SEEDS = sd1.SEEDS
DECKS, REPEATS, SPY, EIGHT = analyze.DECKS, analyze.REPEATS, analyze.SPY, analyze.EIGHT
OPPONENTS = analyze.OPPONENTS
SETS = (*OPPONENTS, "pooled")
FILLS = ("least", "most")
# Fill -> (D's missing cases, the control's), the screen's bounds.
FILL_VALUES = {"least": analyze.BOUNDS["worst"], "most": analyze.BOUNDS["best"]}
OTHER_FILL = {"least": "most", "most": "least"}
READ_METRICS = ("nine_deck", "eight_deck", "spy_score")
DECK_METRICS = tuple(f"deck_{d}" for d in range(DECKS))
LINE_METRICS = analyze.SPY_METRICS
METRICS = READ_METRICS + DECK_METRICS + LINE_METRICS
CONTRASTS = ("d", "e", "f", "D2-D1")
READ_CONTRASTS = ("d", "e", "f")
# Student's t with 7 degrees of freedom, two-sided 95%: the binary64 value
# nearest the quantile 2.36462425159278534...
T_975_7 = 2.3646242515927853
IMPROVEMENT, HARM, NO_DIFFERENCE = "improvement established", "harm established", "no difference established"
INCONCLUSIVE = analyze.INCONCLUSIVE
# A fill's label -> the reading when both fills give it, per contrast.
WORDS = {"d": {"improvement": IMPROVEMENT, "harm": HARM, "no difference": NO_DIFFERENCE},
         "e": {"improvement": IMPROVEMENT, "harm": HARM, "no difference": NO_DIFFERENCE},
         "f": {"improvement": "effect larger after phase 2 established",
               "harm": "effect smaller after phase 2 established",
               "no difference": "change across phase 2 not established"}}
NOT_SHOWN_AGAINST_H = ("improvement established pooled over T1 and H, not shown against H, the opponent the "
                       "teacher did not model")
NOT_ANSWERED = "not answered"
NOT_RUN_REASONS = ("fit", "engineering")
NOTES = {
    "no_difference": "not evidence that strength held",
    "no_change": "not evidence that the effect persisted",
    "only_at": "significance at one checkpoint, not a change between checkpoints",
    "not_answered": "a lineage that did not run phase 2 is not a negative",
    "lineages": "the r1/r2 difference is itself never read as established",
    "line": "J and the line metrics are behavioral, never a strength claim: more J games can mean that wins the "
            "control already had took the line's route",
    "question": "D2 against O answers whether the distilled start stays ahead after the same ordinary schedule, "
                "not whether the search's compute would do better spent on ordinary training",
    "missing": "a missing case is a technical termination, filled within each draw; the fills cannot bite while "
               "no case is missing",
    "screening": "the panel intervals are screening intervals, not calibrated 95% intervals: readings are at 95% "
                 "per contrast, metric and opponent, uncorrected for multiplicity, and only the primary carries a "
                 "frozen claim"}
EXPLANATIONS = {
    "improvement": [
        "the student may have taken on information the executor has and the policy lacks, not planning: the search "
        "samples worlds from its own remaining deck multiset, and it simulated T1's and A48's actual policies, so "
        "its separations are best responses to them (the per-opponent readings and H's condition in the positive "
        "rule bear on this, without settling it)",
        "one teacher bank",
        "any change of this size could move the score, the more so when the effect is small"],
    "no_difference_or_harm": [
        "T trains only the rows where the search separated a move, and the search's advantage may lie in choices "
        "it did not establish",
        "T trains only root decisions, so the first move of a searched plan can be learned without the reused-tree "
        "moves that continued it",
        "the teacher's per-deck effects differ in sign, so a nine-deck null can combine gains and losses",
        "the dose may be too small or too large",
        "the anchor may hold the policy",
        "the student's input may not represent what made the separated moves worse",
        "phase 2 may wash out a change at D1"],
    "harm": [
        "a few hundred rows can be overfit",
        "the leader is the largest of several noisy means and its bound is computed on the same tries",
        "UCT's adaptive allocation biases rivals low: a rival dropped after early losses was scored on a thin "
        "subtree, and its tries stopped because of those losses",
        "where the search played worse than the plain arm, its separations may be wrong",
        "any change of this size could lower the score"],
    "lineages_differ": [
        "the teacher's margin differed (r2 established, r1 not)",
        "the parents differ",
        "chance"]}


def endpoint_ids(lineage: str) -> list:
    return [f"{lineage}-parent"] + [f"{lineage}-{role}-{k}" for role in ("D1", "D2", "O") for k in SEEDS]


def sides(lineage: str, contrast: str, k: int) -> tuple:
    """A simple contrast's (D, control) endpoints for pair k."""
    return {"d": (f"{lineage}-D2-{k}", f"{lineage}-O-{k}"),
            "e": (f"{lineage}-D1-{k}", f"{lineage}-parent"),
            "D2-D1": (f"{lineage}-D2-{k}", f"{lineage}-D1-{k}")}[contrast]


def t_interval(values: list) -> list:
    """The 95% t interval of the mean of eight values (7 degrees of freedom),
    in binary64 with left-to-right sums."""
    require(len(values) == len(SEEDS), "a t interval over eight pairs")
    m = sd1.naive_mean(values)
    sd = math.sqrt(sd1.naive_sum([(v - m) ** 2 for v in values]) / (len(values) - 1))
    half = T_975_7 * sd / math.sqrt(len(values))
    return [m - half, m + half]


# ------------------------------------------------------------------ values


def panel_values(cases: dict, fill: float) -> dict:
    """One panel's metric values with missing cases at ``fill``
    (analyze.summary)."""
    s = analyze.summary(cases, fill)
    out = {m: s[m] for m in READ_METRICS + LINE_METRICS}
    out.update({f"deck_{d}": s["deck_scores"][d] for d in range(DECKS)})
    return out


def draw_values(score, spy, score_idx, spy_idx) -> dict:
    """One draw's panel values: analyze.draw_stats's expressions, with each
    deck's score besides."""
    import numpy as np
    res = np.take_along_axis(score, score_idx, axis=2)
    decks = np.nanmean(res.reshape(DECKS, -1), axis=1)
    out = {"nine_deck": decks.mean(), "eight_deck": decks[EIGHT].mean(), "spy_score": decks[SPY]}
    out.update({f"deck_{d}": decks[d] for d in range(DECKS)})
    for m, v in spy.items():
        out[m] = np.nanmean(np.take_along_axis(v, spy_idx, axis=1))
    return out


def side_value(get, ck: str, opp_set: str, metric: str, fill: float):
    if opp_set == "pooled":
        return (get(ck, "T1", fill)[metric] + get(ck, "H", fill)[metric]) / 2
    return get(ck, opp_set, fill)[metric]


def pair_value(get, lineage: str, contrast: str, k: int, metric: str, opp_set: str, fill: str):
    """Pair k's value of a contrast under a fill. ``get(ck, opp, value)``
    gives a panel's metric values with missing cases at ``value``: points,
    or arrays over the draws."""
    if contrast == "f":
        return (pair_value(get, lineage, "d", k, metric, opp_set, fill)
                - pair_value(get, lineage, "e", k, metric, opp_set, OTHER_FILL[fill]))
    d, c = sides(lineage, contrast, k)
    fd, fc = FILL_VALUES[fill]
    return side_value(get, d, opp_set, metric, fd) - side_value(get, c, opp_set, metric, fc)


def draw_table(data: dict, seed: int, draws: int) -> dict:
    """table[(ck, opp, fill value)][metric]: the panel's value at every draw.
    Each draw takes one ``rng.integers`` draw of repeat indices, shared by
    every panel, as analyze.bootstrap draws them."""
    import numpy as np
    rng = np.random.default_rng(seed)
    filled = {}
    for key, cases in data.items():
        zero = analyze.arrays(cases, 0.0)
        filled[key] = {0.0: zero, 1.0: analyze.arrays(cases, 1.0) if analyze.missing(cases)["count"] else zero}
    distinct = list({id(arrs): arrs for fills in filled.values() for arrs in fills.values()}.values())
    columns = {id(arrs): {m: np.empty(draws) for m in METRICS} for arrs in distinct}
    for i in range(draws):
        idx = rng.integers(0, REPEATS, size=(DECKS, DECKS, REPEATS))
        score_idx = idx[:, :, :, None].repeat(2, axis=3)
        spy_idx = idx[SPY][:, :, None].repeat(2, axis=2)
        for arrs in distinct:
            values = draw_values(*arrs, score_idx, spy_idx)
            column = columns[id(arrs)]
            for m in METRICS:
                column[m][i] = values[m]
    return {(ck, opp, value): columns[id(arrs)] for (ck, opp), fills in filled.items()
            for value, arrs in fills.items()}


def quantiles(values) -> list:
    """The 2.5% and 97.5% quantiles, as analyze.bootstrap takes them."""
    import numpy as np
    v = np.sort(values)
    n = len(v)
    return [float(v[int(0.025 * n)]), float(v[int(0.975 * n) - 1])]


# ----------------------------------------------------------------- readings


def fill_label(t: list, boot: list) -> str:
    if t[0] > 0 and boot[0] > 0:
        return "improvement"
    if t[1] < 0 and boot[1] < 0:
        return "harm"
    return "no difference"


def reading(entry: dict, contrast: str) -> tuple:
    """Section 7.2's reading of one contrast, metric and opponent set from
    its two fills, with its notes."""
    labels = [entry[f]["label"] for f in FILLS]
    if labels[0] != labels[1]:
        return INCONCLUSIVE, []
    out = WORDS[contrast][labels[0]]
    if labels[0] != "no difference":
        return out, []
    notes = [NOTES["no_change"] if contrast == "f" else NOTES["no_difference"]]
    for f in FILLS:
        for name in ("t", "bootstrap"):
            low, high = entry[f][name]
            if low > 0 or high < 0:
                notes.append(f"the {name} interval alone excluded 0 ({'above' if low > 0 else 'below'}) under the "
                             f"fill {f} favorable to D")
    return out, notes


def only_at(e_reading: str, d_reading: str):
    """Where only one of the D1 and D2 contrasts is established."""
    for established in (IMPROVEMENT, HARM):
        if e_reading == established and d_reading != established:
            return f"{established} only at D1"
        if d_reading == established and e_reading != established:
            return f"{established} only at D2"
    return None


def across_lineages(per: dict, not_run: dict, contrast: str) -> str:
    """One contrast, metric and opponent set across lineages (section 7.2);
    ``per`` holds the readings of the lineages that ran phase 2."""
    words = WORDS[contrast]
    established = {lin: r for lin, r in per.items() if r in (words["improvement"], words["harm"])}
    kinds = set(established.values())
    parts = []
    if len(kinds) == 2:
        parts.append("in opposite directions (lineage-dependent)")
    elif len(established) == len(LINEAGES):
        parts.append(f"{kinds.pop()} in both lineages")
    elif established:
        lin, kind = next(iter(established.items()))
        parts.append(f"{kind} in {lin} only (lineage-dependent: established in one lineage, which is not an "
                     "established difference between lineages)")
    elif per:
        parts.append("in neither")
    for lin in LINEAGES:
        if lin in not_run:
            parts.append(f"not run in {lin} ({not_run[lin]})")
        elif per.get(lin) == INCONCLUSIVE:
            parts.append(f"inconclusive in {lin} (missing cases decide)")
    return "; ".join(parts)


def across_opponents(per: dict, contrast: str):
    """T1 against H, likewise: an established reading against one opponent
    and not the other is not an established difference between opponents."""
    words = WORDS[contrast]
    established = {opp: r for opp, r in per.items() if r in (words["improvement"], words["harm"])}
    if not established or len(set(per.values())) == 1:
        return None
    if len(set(established.values())) == 2:
        return "in opposite directions against T1 and H (opponent-dependent)"
    opp, kind = next(iter(established.items()))
    return f"{kind} against {opp} only (not an established difference between opponents)"


def explanations(read: str) -> list:
    """The keys of the competing explanations kept with a reading."""
    if read in (IMPROVEMENT, NOT_SHOWN_AGAINST_H):
        return ["improvement"]
    if read == HARM:
        return ["no_difference_or_harm", "harm"]
    if read == NO_DIFFERENCE:
        return ["no_difference_or_harm"]
    return []


def headline(contrasts: dict, contrast: str) -> dict:
    """The primary (d) or secondary (e) reading of a lineage: the pooled
    nine-deck reading, with H's mean of the eight per-pair nine-deck
    differences and the pooled eight-deck label, both under the fill least
    favorable to D."""
    pooled = contrasts[contrast]["nine_deck"]["pooled"]["reading"]
    h_mean = contrasts[contrast]["nine_deck"]["H"]["least"]["mean"]
    eight = contrasts[contrast]["eight_deck"]["pooled"]["least"]["label"]
    read = NOT_SHOWN_AGAINST_H if pooled == IMPROVEMENT and not h_mean > 0 else pooled
    return {"reading": read, "pooled_nine_deck": pooled, "h_nine_deck_mean_least_favorable": h_mean,
            "pooled_eight_deck_label_least_favorable": eight,
            "conditions": {"pooled_nine_deck_improvement_established": pooled == IMPROVEMENT,
                           "h_nine_deck_mean_above_0": h_mean > 0,
                           "pooled_eight_deck_not_harm": eight != "harm"},
            "explanations": explanations(read)}


# ----------------------------------------------------------------- custody


def given(entry: dict) -> dict:
    """A spec's pin as recorded: its path as given and its SHA-256."""
    return {"path": entry["path"], "sha256": entry["sha256"]}


def read_plan(completion: dict, what: str) -> tuple:
    """The run.json beside a run's completion.json, which the run driver
    writes before the run's first update, and its pin (in the completion
    pin's spelling), from one read."""
    path = sd1.beside(completion["path"], "run.json")
    try:
        data = Path(path).read_bytes()
    except FileNotFoundError:
        raise ValueError(f"{what}: {path} is missing") from None
    try:
        plan = json.loads(data)
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError(f"{what}: {path} is not readable JSON ({error})") from None
    return plan, {"path": path, "sha256": hashlib.sha256(data).hexdigest()}


def unit_problem(lineage: str, k: int, cfg: dict, plan: list) -> str | None:
    """Why a run's units do not carry seed k's episode ids and seeds, unit
    by unit in the pilot's schedule (sd1.unit_id_seed: O's own for k = 0,
    sd1-LIN-repK's for k of 1 to 7), or None."""
    iterations = cfg.get("iterations")
    if not (isinstance(iterations, list) and len(iterations) == len(plan)):
        return f"it does not hold the schedule's {len(plan)} updates"
    for u, (entries, update) in enumerate(zip(plan, iterations)):
        episodes = update.get("episodes") if isinstance(update, dict) else None
        if not (isinstance(episodes, list) and len(episodes) == len(entries)):
            return f"update {u} does not hold the schedule's units"
        for i, (entry, item) in enumerate(zip(entries, episodes)):
            episode = item.get("episode") if isinstance(item, dict) else None
            got = (episode.get("id"), episode.get("seed")) if isinstance(episode, dict) else None
            if got != sd1.unit_id_seed(lineage, k, entry):
                return f"update {u} unit {i}: its episode id and seed are not seed {k}'s"
    return None


def run_endpoint(entry: dict, what: str, lineage: str, k: int, initial: dict | None, plan: list) -> tuple:
    """A complete 162-update run's final checkpoint file SHA-256 and the pins
    read for it, once the run is the one its name says. The run's plan (the
    run.json beside its completion.json) must give every unit seed k's
    episode id and seed, and its initial source must be ``initial``
    verbatim (D1-k's source, for D2-k) or, with ``initial`` None, the
    parent's (O-k)."""
    run = sd1.read_pinned(entry, what)
    require(run.get("schema") == phase2.RUN_SCHEMA and run.get("complete") is True
            and run.get("completed_iterations") == pilot.UPDATES, f"{what}: not a complete 162-update run")
    recorded, recorded_pin = read_plan(entry, what)
    cfg = recorded.get("config") if isinstance(recorded, dict) else None
    require(isinstance(cfg, dict) and recorded.get("schema") == phase2.RUN_SCHEMA
            and cfg.get("schema") == phase2.RUN_SCHEMA, f"{what}: the run.json beside it is not a run's plan")
    problem = unit_problem(lineage, k, cfg, plan)
    require(problem is None, f"{what}: not seed {k}'s run: {problem}")
    source = cfg.get("initial_source")
    if initial is None:
        require(isinstance(source, dict) and (source.get("checkpoint") or {}).get("sha256")
                == PARENT_CHECKPOINT_SHA256[lineage], f"{what}: its initial source is not the parent")
    else:
        require(sd1.canonical(source) == sd1.canonical(initial),
                f"{what}: its initial source is not D1-{k}'s source verbatim")
    return run["source"]["checkpoint"]["sha256"], {"completion": given(entry), "run": recorded_pin}


def expected_endpoints(lineage: str, entry: dict) -> tuple:
    """Each endpoint's checkpoint file SHA-256, and the pins read for it: the
    parent's recorded checkpoint; each seed's selected D1 checkpoint, with
    its selection and source; each phase-2 run's final checkpoint, D2-k
    from D1-k's source and O-k from the parent, each on seed k's units; and
    O-0's, stage 0's O-s0, from the parent on O's own units."""
    out = {f"{lineage}-parent": PARENT_CHECKPOINT_SHA256[lineage]}
    read, sources = {}, {}
    for item in entry["select"]:
        selection = sd1.read_pinned(item, f"{lineage} selection")
        require(selection.get("schema") == sd1.SELECT_SCHEMA and selection.get("lineage") == lineage,
                f"{lineage}: not this lineage's selection")
        k = selection.get("seed")
        require(k in SEEDS and f"{lineage}-D1-{k}" not in out, f"{lineage}: seed {k!r} is selected twice or unknown")
        require(selection.get("reading") == "passes", f"{lineage}: seed {k} has no D1, so the lineage runs no panels")
        chosen = selection["d1_checkpoint"]
        sources[k] = sd1.read_pinned(chosen["source"], f"{lineage} D1-{k} source")
        require(sources[k].get("checkpoint") == chosen["checkpoint"],
                f"{lineage} D1-{k}: the source's checkpoint is not the selected one")
        out[f"{lineage}-D1-{k}"] = chosen["checkpoint"]["sha256"]
        read[f"{lineage}-D1-{k}"] = {"selection": given(item), "source": given(chosen["source"])}
    require(len(out) == 1 + len(SEEDS), f"{lineage}: every seed's selection is needed")
    runs = entry["phase2"]
    require(set(runs) == {f"D2-{k}" for k in SEEDS} | {f"O-{k}" for k in SEEDS if k},
            f"{lineage}: phase 2 is D2-0 to D2-7 and O-1 to O-7")
    plan = pilot.schedule(lineage)["updates"]
    for name, value in [*sorted(runs.items()), ("O-0", entry["o_s0"])]:
        role, k = name.split("-")
        what = f"{lineage} {'O-s0' if name == 'O-0' else name}"
        out[f"{lineage}-{name}"], read[f"{lineage}-{name}"] = run_endpoint(
            value, what, lineage, int(k), sources[int(k)] if role == "D2" else None, plan)
    require(len(set(out.values())) == len(out), f"{lineage}: two endpoints name the same checkpoint")
    return out, read


def lineage_panels(lineage: str, entry: dict) -> tuple:
    """The lineage's 50 panels, each on its endpoint's checkpoint, with its
    rows and labels verified against their pins (the panels as the screen's
    analyzer loads them: rows and labels by path), and the pins read."""
    expected, read = expected_endpoints(lineage, entry)
    panels = entry["panels"]
    require(sorted((p["checkpoint"], p["opponent"]) for p in panels)
            == sorted((ck, opp) for ck in endpoint_ids(lineage) for opp in OPPONENTS),
            f"{lineage}: the panels must be every endpoint's panel against each opponent, once each")
    loadable, files = [], {}
    for p in panels:
        name = f"{p['checkpoint']}-vs-{p['opponent']}"
        require(p["checkpoint_sha256"] == expected[p["checkpoint"]], f"{name}: not the endpoint's checkpoint")
        require(all(sd1.is_pin(p.get(f)) for f in ("rows", "labels")), f"{name}: its rows and labels are pins")
        files[f"{p['checkpoint']}/{p['opponent']}"] = {f: given(p[f]) for f in ("rows", "labels")}
        loadable.append({**p, **{f: str(sd1.pinned(p[f], f"{name} {f}")) for f in ("rows", "labels")}})
    return loadable, {"endpoints": read, "panels": files}


# --------------------------------------------------------------------- X4


def x4(screen_index: Path, screen_result: Path) -> dict:
    """The screen's analyzer recomputes every contrast of the screen's
    RESULT.json (point and interval, under both bounds) from the screen's
    own panels with the screen's seed; each must be equal, and no contrast
    may be missing on either side."""
    result = read_json(screen_result)
    require(result.get("seed") == analyze.BOOT_SEED and result.get("draws") == analyze.DRAWS,
            "the screen's result used another seed or draw count")
    data = analyze.load(read_json(screen_index))
    todo = analyze.plan(data, analyze.SCREEN + analyze.DIRECT + analyze.DESCRIPTIVE)
    metrics = analyze.METRICS + analyze.EXTRA_METRICS
    point, ci = analyze.points(data, todo, metrics), analyze.bootstrap(data, todo, metrics)
    recomputed = {"/".join(k): {"point": point[k], "ci95": ci[k]} for k in point}
    recorded = result["contrasts"]
    differ = sorted(k for k in set(recomputed) | set(recorded) if recomputed.get(k) != recorded.get(k))
    return {"schema": X4_SCHEMA, "screen_index": pin(Path(screen_index).resolve()),
            "screen_result": pin(Path(screen_result).resolve()), "seed": analyze.BOOT_SEED,
            "compared": len(recorded), "differing": differ, "pass": not differ}


# ----------------------------------------------------------------- analysis


def contrast_entry(get, table: dict, lineage: str, contrast: str, metric: str, opp_set: str) -> dict:
    """Under each fill: the eight per-pair values, their mean, the t and
    bootstrap intervals of the eight-pair mean, and the fill's label."""
    out = {}
    for f in FILLS:
        pairs = [pair_value(get, lineage, contrast, k, metric, opp_set, f) for k in SEEDS]
        total = None
        for k in SEEDS:
            column = pair_value(lambda ck, opp, value: table[(ck, opp, value)], lineage, contrast, k, metric,
                                opp_set, f)
            total = column if total is None else total + column
        t, boot = t_interval(pairs), quantiles(total / len(SEEDS))
        out[f] = {"pairs": pairs, "mean": sd1.naive_mean(pairs), "t": t, "bootstrap": boot,
                  "label": fill_label(t, boot)}
    return out


def lineage_result(data: dict, table: dict, lineage: str) -> dict:
    cache = {}

    def get(ck, opp, value):
        if (ck, opp, value) not in cache:
            cache[(ck, opp, value)] = panel_values(data[(ck, opp)], value)
        return cache[(ck, opp, value)]
    contrasts = {}
    for contrast in CONTRASTS:
        contrasts[contrast] = {}
        for metric in METRICS:
            by_set = {s: contrast_entry(get, table, lineage, contrast, metric, s) for s in SETS}
            if contrast in READ_CONTRASTS and metric in READ_METRICS:
                for entry in by_set.values():
                    entry["reading"], entry["notes"] = reading(entry, contrast)
                by_set["across_opponents"] = across_opponents({s: by_set[s]["reading"] for s in OPPONENTS},
                                                              contrast)
            contrasts[contrast][metric] = by_set
    primary = headline(contrasts, "d")
    primary["positive"] = all(primary["conditions"].values())
    change = {}
    for metric in READ_METRICS:
        for s in SETS:
            change[f"{metric}/{s}"] = {
                "reading": contrasts["f"][metric][s]["reading"],
                "only_at": only_at(contrasts["e"][metric][s]["reading"], contrasts["d"][metric][s]["reading"])}
    return {"primary": primary, "secondary": headline(contrasts, "e"), "change": change, "contrasts": contrasts}


def analysis(spec: dict, seed: int = BOOT_SEED, draws: int | None = None, spec_pin: dict | None = None) -> dict:
    """SPEC as in the module docstring; ``spec_pin`` is the spec file's pin,
    recorded with the pins of everything else read."""
    draws = analyze.DRAWS if draws is None else draws
    sd1.pinned(spec["screen_index"], "screen index")
    sd1.pinned(spec["screen_result"], "screen result")
    first = sd1.read_pinned(spec["x4"], "X4")
    require(first.get("schema") == X4_SCHEMA and first.get("pass") is True
            and first["screen_index"]["sha256"] == spec["screen_index"]["sha256"]
            and first["screen_result"]["sha256"] == spec["screen_result"]["sha256"],
            "X4 did not pass on these screen files before any SD1 panel")
    require(set(spec["lineages"]) == set(LINEAGES), "the spec names both lineages")
    inputs = {"spec": spec_pin, **{f: given(spec[f]) for f in ("screen_index", "screen_result", "x4")},
              "lineages": {}}
    not_run, panels, allowed = {}, [], set()
    for lineage in LINEAGES:
        entry = spec["lineages"][lineage]
        if "not_run" in entry:
            require(entry["not_run"] in NOT_RUN_REASONS and set(entry) == {"not_run"},
                    f"{lineage}: a lineage without phase 2 names why (fit or engineering) and nothing else")
            not_run[lineage] = entry["not_run"]
            continue
        loadable, inputs["lineages"][lineage] = lineage_panels(lineage, entry)
        panels += loadable
        allowed |= set(endpoint_ids(lineage))
    ran = [lin for lin in LINEAGES if lin not in not_run]
    out = {"schema": SCHEMA, "seed": seed, "draws": draws, "x4": first, "inputs": inputs,
           "deck_labels": sd1.deck_names(), "not_run": not_run, "lineages": {}, "notes": NOTES,
           "explanations": EXPLANATIONS}
    if ran:
        data = analyze.load({"panels": panels}, allowed, pilot.SD1_PANEL_NS)
        table = draw_table(data, seed, draws)
        for lineage in ran:
            result = lineage_result(data, table, lineage)
            mine = sorted(key for key in data if key[0].startswith(f"{lineage}-"))
            result["panels"] = {f"{ck}/{opp}": analyze.summary(data[(ck, opp)]) for ck, opp in mine}
            result["missing"] = {f"{ck}/{opp}": analyze.missing(data[(ck, opp)]) for ck, opp in mine}
            result["line_choices"] = {f"{ck}/{opp}": analyze.secondary(data[(ck, opp)]) for ck, opp in mine}
            out["lineages"][lineage] = result
    for lineage, why in not_run.items():
        out["lineages"][lineage] = {"not_run": why, "primary": {"reading": NOT_ANSWERED, "positive": False},
                                    "secondary": {"reading": NOT_ANSWERED}}
    across = {}
    for contrast in READ_CONTRASTS:
        for metric in READ_METRICS:
            for s in SETS:
                per = {lin: out["lineages"][lin]["contrasts"][contrast][metric][s]["reading"] for lin in ran}
                across[f"{contrast}/{metric}/{s}"] = {"reading": across_lineages(per, not_run, contrast),
                                                      "per_lineage": per}
    out["across_lineages"] = across
    out["method_screen_positive"] = (not not_run
                                     and all(out["lineages"][lin]["primary"]["positive"] for lin in LINEAGES))
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    x = sub.add_parser("x4")
    x.add_argument("--screen-index", type=Path, required=True)
    x.add_argument("--screen-result", type=Path, required=True)
    x.add_argument("--out", type=Path, required=True)
    a = sub.add_parser("analyze")
    a.add_argument("--spec", type=Path, required=True)
    a.add_argument("--out", type=Path, required=True)
    args = ap.parse_args(argv)
    if args.op == "x4":
        result = x4(args.screen_index, args.screen_result)
        write_json(args.out, result)
        print(json.dumps({"compared": result["compared"], "differing": len(result["differing"]),
                          "pass": result["pass"]}))
        return 0 if result["pass"] else 1
    data = args.spec.read_bytes()
    result = analysis(json.loads(data), spec_pin={"path": absolute(args.spec),
                                                  "sha256": hashlib.sha256(data).hexdigest()})
    write_json(args.out, result)
    print(json.dumps({"method_screen_positive": result["method_screen_positive"],
                      "primary": {lin: v["primary"]["reading"] for lin, v in result["lineages"].items()}}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
