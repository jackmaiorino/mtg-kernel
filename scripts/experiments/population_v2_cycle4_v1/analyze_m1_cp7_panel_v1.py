"""Cycle-4 M1 CP7 transfer analysis (pre-registration sections M1, 4 and 7).

Reads the M1 CP7 panel written by run-m1-cp7-panel.sh: two model groups (A and
B), each 16 disjoint 128-pair shards of run_cp7_store_panel_v2.py (runner of
record) on ONE base seed, so every model has the same 2,048 roots, and computes
the pre-registered within-panel CRN-paired contrasts with the fixed-N
root-cluster estimator:

  one inferential unit = one root = both seat-swapped legs;
  root score of a model = mean over its two legs of (win 1, draw 0.5, loss 0);
  paired delta per root = score(endpoint) - score(reference);
  point = mean delta in percentage points; SE = sample sd / sqrt(N roots);
  one-sided 95 percent lower bound = point - 1.645 * SE (fixed N, no anytime
  correction); milestone = LCB > 0 AND point >= +2.0 pp.

Primary hypothesis: TREATMENT-RB vs g896 (the only contrast gated directly).
Secondaries under Holm (alpha 0.05, one-sided p-values): CONTROL-R vs g896 and
STATIC-RB vs g896; a secondary's milestone is assigned only after the Holm
step. The cycle-3 g2048 contrasts are co-measured context and carry no gate.

Registered N and voids. The frame pins 2,048 non-voided common roots. A root
voided for either model of a gated contrast is dropped from the estimate,
and the contrast is then ALSO evaluated under the pre-registration's
worst-case missing-outcome bound: every dropped root scored as the endpoint
losing both legs and the reference winning both (delta -1). With any voided
root present, the milestone requires BOTH the complete-case gate and the
worst-case gate (this rule is submitted to Jack for ratification; with zero
voids, the expected case after the bridge fault fix, the two coincide).

Provenance, all checked before any statistic is computed (fail-closed):
  - every shard directory holds a published panel-summary.json and
    panel-plan.json of the registered schemas, formal mode, the registered base
    seed, the shard's pair range (pair_start = 128 * index, pairs = 128,
    read_pairs = 2048), the registered scorer, runner and Mage identities and
    the harness's plan SHA-256 recorded by the summary;
  - each model's checkpoint identity (loaded generation, run and checkpoint
    hashes) is identical across every shard that names it;
  - every consumed task outcome file matches the SHA-256 the summary recorded
    for it; files the summary does not name are ignored;
  - voids come from summary["voids"]["per_model"][label]["voided_pair_indices"]
    and are accumulated; terminal rows of voided pairs (the runner keeps a
    completed first leg) are skipped; a model with more than 2 percent of the
    registered roots voided (41 or more of 2,048) fails the registered cap;
  - every model's root universe is exactly range(2048) as outcomes plus voids,
    both legs present on every non-voided root;
  - the repeated model across groups is exactly treatment-rb, with identical
    roots, voids, environment seeds and outcomes; both groups agree on every
    root's environment seed.

Written and reviewed BEFORE any M1 outcome was read.
usage: analyze_m1_cp7_panel_v1.py --group-a <root of m1-a> --group-b <root of m1-b> --output <json>
       analyze_m1_cp7_panel_v1.py --self-test
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import json
import math
import os

PRIMARY = ("treatment-rb", "g896")
SECONDARIES = (("control-r", "g896"), ("static-rb", "g896"))
COMEASURED = (("treatment-rb", "cycle3-g2048"), ("control-r", "cycle3-g2048"), ("static-rb", "cycle3-g2048"))
REQUIRED_MODELS = frozenset({"treatment-rb", "control-r", "static-rb", "g896", "cycle3-g2048"})
REPEATED_MODEL = "treatment-rb"
REGISTERED_ROOTS = 2048
SHARD_PAIRS = 128
VOID_CAP_FRACTION = 0.02
Z_ONE_SIDED_95 = 1.6448536269514722
POINT_FLOOR_PP = 2.0
HOLM_ALPHA = 0.05

ADMITTED = {
    # label: (loaded_generation, loaded_run_sha256, loaded_checkpoint_sha256), from the
    # routing record's endpoint list (f9ad13e9...) and the cycle-3 focal store.
    "control-r": (2048, "21dd8635828af50e2de4deac91898682f98a8a2e2d156562e2e78809355ab904", "258307a6bec61a44ee674a07cef18eed49bdca6be74b81c875b544abb176dd52"),
    "static-rb": (2048, "280733572207156515287d09f07de9fdef95b55e6ef634c70d14f7d2ce3413b8", "959df772eba0f6e7616f02eca3b241ab33af1d5ed47f16923e758ee68e8d1fc6"),
    "treatment-rb": (2048, "d3666c7a054946dfc9c85dac60cf91ba9af8cea38adb6b68ab81a8bc5560625b", "ec23cd71e9af373830f9c210c619d02d3ae9c68dced67aac1381232d61c7ea58"),
    "g896": (896, "f25a63d0a2968016c2d44220b02d46b642fad5c4d524cd7ed82d699dbfda83a1", "5bdebb31fa9e916121c8138f1a1b854a514c491f3398f4f2a13b9333d3df7545"),
    "cycle3-g2048": (2048, "f25a63d0a2968016c2d44220b02d46b642fad5c4d524cd7ed82d699dbfda83a1", "8d038f7b67ec2f2c8106e916c9d0490c05943e08fe89dfa963cc53ff6f975951"),
}

REGISTERED = {
    "summary_schema": "mtg-kernel-population-store-cp7-panel/v3",
    "plan_schema": "mtg-kernel-population-store-cp7-panel-plan/v2",
    "mode": "formal",
    "base_seed": 2026090501,
    "read_pairs": REGISTERED_ROOTS,
    "scorer_sha256": "f5a9a0aa95f9a4f823d23a5e06f29b8c1626e427f51b15c7769c2aaed6d3de6d",
    "runner_sha256": "7c0c0fb68c814dcda20086caf9201550c5ae0b35e78e6d8d7feb5716927fc9dd",
    "mage_commit": "f89c68fc1f08aca79cfa3f990e965f31c61b7086",
    "admitted": ADMITTED,
}


class M1AnalysisError(Exception):
    pass


def sha256_file(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def bind_shard(shard: str, index: int, registered: dict) -> tuple[dict, dict]:
    """Validates one shard's published summary and plan against the registered
    contract and returns (summary, plan)."""
    summary_path = os.path.join(shard, "panel-summary.json")
    plan_path = os.path.join(shard, "panel-plan.json")
    if not os.path.isfile(summary_path):
        raise M1AnalysisError(f"{shard}: no published panel-summary.json (shard not completed by the runner)")
    if not os.path.isfile(plan_path):
        raise M1AnalysisError(f"{shard}: no panel-plan.json")
    summary = json.load(open(summary_path, encoding="utf-8"))
    plan = json.load(open(plan_path, encoding="utf-8"))
    checks = {
        "summary schema": (summary.get("schema"), registered["summary_schema"]),
        "plan schema": (plan.get("schema"), registered["plan_schema"]),
        "mode": (summary.get("mode"), registered["mode"]),
        "base_seed": (summary.get("base_seed"), registered["base_seed"]),
        "pair_start": (summary.get("pair_start"), index * SHARD_PAIRS),
        "pairs": (summary.get("pairs"), SHARD_PAIRS),
        "plan pair_start": ((plan.get("panel") or {}).get("pair_start"), index * SHARD_PAIRS),
        "plan pair_count": ((plan.get("panel") or {}).get("pair_count"), SHARD_PAIRS),
        "plan read_pairs": ((plan.get("panel") or {}).get("read_pairs"), registered["read_pairs"]),
        "plan mode": ((plan.get("panel") or {}).get("mode"), registered["mode"]),
        "plan base_seed": ((plan.get("panel") or {}).get("base_seed"), registered["base_seed"]),
        "scorer": ((plan.get("inputs") or {}).get("scorer_sha256"), registered["scorer_sha256"]),
        "runner": ((plan.get("inputs") or {}).get("runner_sha256"), registered["runner_sha256"]),
        "mage": ((plan.get("inputs") or {}).get("mage_commit"), registered["mage_commit"]),
    }
    for what, (actual, expected) in checks.items():
        if actual != expected:
            raise M1AnalysisError(f"{shard}: {what} is {actual!r}, registered {expected!r}")
    recorded_plan = summary.get("plan") or {}
    if recorded_plan.get("sha256") != sha256_file(plan_path):
        raise M1AnalysisError(f"{shard}: panel-plan.json does not match the SHA-256 the summary recorded")
    if summary.get("tolerate_engine_faults") is not True:
        raise M1AnalysisError(f"{shard}: tolerate_engine_faults is not recorded as true")
    return summary, plan


def read_group(group_root: str, registered: dict, registered_roots: int) -> dict:
    """Reads and binds every shard under a group root.

    Returns {"models": {label: {pair: {"seed": hex, "legs": {seat: reward}}}},
             "voids": {label: set(pair)}, "identities": {label: identity},
             "shards": [names]}."""
    shard_dirs = sorted(d for d in glob.glob(os.path.join(group_root, "shard-*")) if os.path.isdir(d))
    expected_shards = registered_roots // SHARD_PAIRS
    if len(shard_dirs) != expected_shards:
        raise M1AnalysisError(f"{group_root}: {len(shard_dirs)} shard directories, registered {expected_shards}")
    models: dict[str, dict[int, dict]] = {}
    voids: dict[str, set[int]] = {}
    identities: dict[str, dict] = {}
    summary_sha256: dict[str, str] = {}
    for index, shard in enumerate(shard_dirs):
        if os.path.basename(shard) != f"shard-{index:02d}":
            raise M1AnalysisError(f"{shard}: shard directories must be shard-00 .. shard-{expected_shards - 1:02d} without gaps")
        summary, plan = bind_shard(shard, index, registered)
        summary_sha256[os.path.basename(shard)] = sha256_file(os.path.join(shard, "panel-summary.json"))
        plan_models = (plan.get("inputs") or {}).get("models") or {}
        admitted = registered.get("admitted") or {}
        for label, spec in plan_models.items():
            identity = spec.get("checkpoint") if isinstance(spec, dict) else None
            if not isinstance(identity, dict):
                raise M1AnalysisError(f"{shard}: model {label} has no checkpoint identity in the plan")
            if label not in admitted:
                raise M1AnalysisError(f"{shard}: model label {label!r} is not an admitted endpoint")
            gen, run, ckpt = admitted[label]
            actual = (identity.get("loaded_generation"), identity.get("loaded_run_sha256"), identity.get("loaded_checkpoint_sha256"))
            if actual != (gen, run, ckpt):
                raise M1AnalysisError(f"{shard}: {label} loaded identity {actual} is not the admitted {(gen, run, ckpt)}")
            if label in identities and identities[label] != identity:
                raise M1AnalysisError(f"{label}: checkpoint identity differs between shards")
            identities.setdefault(label, identity)
        planned_tasks = {}
        for t in (plan.get("panel") or {}).get("tasks") or []:
            planned_tasks[(t.get("label"), t.get("first_pair"), t.get("pair_count"))] = t.get("stem")
        if not planned_tasks:
            raise M1AnalysisError(f"{shard}: the plan names no tasks")
        per_model_voids = (summary.get("voids") or {}).get("per_model")
        if not isinstance(per_model_voids, dict):
            raise M1AnalysisError(f"{shard}: panel-summary.json carries no voids.per_model map")
        shard_voids: dict[str, set[int]] = {}
        for label, info in per_model_voids.items():
            indices = info.get("voided_pair_indices") if isinstance(info, dict) else None
            if not isinstance(indices, list):
                raise M1AnalysisError(f"{shard}: {label} voids carry no voided_pair_indices list")
            shard_voids[label] = set(int(p) for p in indices)
            voids.setdefault(label, set()).update(shard_voids[label])
        # Consume only the task outcome files the summary names, at the bytes it recorded.
        consumed_tasks = set()
        for task in summary.get("tasks") or []:
            label = task.get("label")
            key = (label, task.get("first_pair"), task.get("pair_count"))
            stem = planned_tasks.get(key)
            if stem is None:
                raise M1AnalysisError(f"{shard}: summary task {key} is not in the plan's task list")
            consumed_tasks.add(key)
            if label not in identities:
                raise M1AnalysisError(f"{shard}: summary task label {label!r} is not a planned model")
            task_first, task_count = key[1], key[2]
            if not isinstance(task_first, int) or not isinstance(task_count, int) or task_count < 1:
                raise M1AnalysisError(f"{shard}: task {key} has a malformed pair range")
            segments = task.get("segments") or []
            if not segments:
                raise M1AnalysisError(f"{shard}: task {key} records no segments")
            # The runner of record splits a task at every tolerated void: segment k
            # covers a contiguous run of pairs that ends at the voided pair (or at the
            # end of the task), and continuation k is named <stem>-void0k. The
            # segments must partition the planned range in order, and the voids they
            # record must be exactly the shard's voids for this model in that range.
            next_first = task_first
            task_voids: set[int] = set()
            for attempt, segment in enumerate(segments):
                seg_first, seg_count = segment.get("first_pair"), segment.get("pair_count")
                if (segment.get("label") != label or not isinstance(seg_first, int)
                        or not isinstance(seg_count, int) or seg_count < 1):
                    raise M1AnalysisError(f"{shard}: task {key} has a segment whose label or range is malformed")
                if segment.get("attempt") != attempt:
                    raise M1AnalysisError(f"{shard}: task {key} segments are not numbered 0..n in order")
                if seg_first != next_first or seg_first + seg_count > task_first + task_count:
                    raise M1AnalysisError(f"{shard}: task {key} segments do not partition the planned range"
                                          f" (segment {attempt} covers {seg_first}+{seg_count})")
                next_first = seg_first + seg_count
                voided = segment.get("voided_pairs")
                if not isinstance(voided, list) or any(not isinstance(p, int) for p in voided):
                    raise M1AnalysisError(f"{shard}: task {key} segment {attempt} lacks a voided_pairs list")
                last_segment = attempt == len(segments) - 1
                if voided not in ([], [seg_first + seg_count - 1]) or (not last_segment and not voided):
                    raise M1AnalysisError(f"{shard}: task {key} segment {attempt} must end at its voided pair, got {voided}")
                task_voids.update(voided)
                outcome_path = segment.get("outcome")
                recorded = segment.get("outcome_sha256")
                if not outcome_path or not recorded:
                    raise M1AnalysisError(f"{shard}: task {label} segment lacks an outcome path or hash")
                basename = os.path.basename(outcome_path.replace("\\", "/"))  # the runner records Windows paths
                expected_name = (stem if attempt == 0 else f"{stem}-void{attempt:02d}") + ".outcome.jsonl"
                if basename != expected_name:
                    raise M1AnalysisError(f"{shard}: outcome file {basename} is not the runner's segment file {expected_name}")
                local = os.path.join(shard, "tasks", basename)
                if not os.path.isfile(local):
                    raise M1AnalysisError(f"{shard}: recorded outcome file missing: {basename}")
                if sha256_file(local) != recorded:
                    raise M1AnalysisError(f"{shard}: {basename} does not match its recorded SHA-256")
                per = models.setdefault(label, {})
                skip = shard_voids.get(label, set())
                header_bound = False
                with open(local, encoding="utf-8") as handle:
                    for line in handle:
                        rec = json.loads(line)
                        if rec.get("record_type") == "header":
                            if rec.get("checkpoint") != identities[label]:
                                raise M1AnalysisError(f"{basename}: outcome header checkpoint is not exactly the planned model {label} identity")
                            header_bound = True
                            continue
                        if rec.get("record_type") != "terminal":
                            continue
                        if not header_bound:
                            raise M1AnalysisError(f"{basename}: terminal record before a bound header")
                        pair = int(rec["pair_index"])
                        if not seg_first <= pair < seg_first + seg_count:
                            raise M1AnalysisError(f"{basename}: terminal row for root {pair} lies outside the segment range {seg_first}+{seg_count}")
                        if pair in skip:
                            continue  # the runner keeps a completed first leg of a voided pair
                        seat = rec["candidate_seat"]
                        if seat not in ("p0", "p1"):
                            raise M1AnalysisError(f"{basename}: root {pair} carries an unknown candidate seat {seat!r}")
                        reward = int(rec["candidate_terminal_reward"])
                        if reward not in (-1, 0, 1):
                            raise M1AnalysisError(f"{basename}: root {pair} seat {seat} carries a terminal reward outside -1, 0, 1")
                        entry = per.setdefault(pair, {"seed": rec["pair_environment_seed_u64_hex"], "legs": {}})
                        if entry["seed"] != rec["pair_environment_seed_u64_hex"]:
                            raise M1AnalysisError(f"{label}: root {pair} carries two environment seeds")
                        if seat in entry["legs"]:
                            raise M1AnalysisError(f"{label}: root {pair} seat {seat} recorded twice")
                        entry["legs"][seat] = reward
            if next_first != task_first + task_count:
                raise M1AnalysisError(f"{shard}: task {key} segments stop at pair {next_first},"
                                      f" before the planned end {task_first + task_count}")
            in_range = {p for p in shard_voids.get(label, set()) if task_first <= p < task_first + task_count}
            if task_voids != in_range:
                raise M1AnalysisError(f"{shard}: task {key} segment voids {sorted(task_voids)} differ from"
                                      f" the summary's per-model voids {sorted(in_range)}")
        if set(planned_tasks) != consumed_tasks:
            raise M1AnalysisError(f"{shard}: summary tasks {sorted(consumed_tasks)} do not cover the plan's tasks {sorted(planned_tasks)}")
    for label in models:
        voids.setdefault(label, set())
    return {"models": models, "voids": voids, "identities": identities,
            "shards": [os.path.basename(s) for s in shard_dirs], "summary_sha256": summary_sha256}


def validate_universe(models: dict, voids: dict, registered_roots: int) -> dict:
    cap = int(math.floor(VOID_CAP_FRACTION * registered_roots))
    report = {}
    universe = set(range(registered_roots))
    for label, per in models.items():
        voided = voids.get(label, set())
        if not voided <= universe:
            raise M1AnalysisError(f"{label}: voided roots outside the registered universe")
        with_outcomes = set(per)
        if with_outcomes & voided:
            raise M1AnalysisError(f"{label}: outcome counted for a voided root {sorted(with_outcomes & voided)[:3]}")
        if with_outcomes | voided != universe:
            missing = sorted(universe - with_outcomes - voided)[:5]
            extra = sorted((with_outcomes | voided) - universe)[:5]
            raise M1AnalysisError(f"{label}: root universe is not exactly range({registered_roots}); missing {missing} extra {extra}")
        incomplete = [p for p, e in per.items() if set(e["legs"]) != {"p0", "p1"}]
        if incomplete:
            raise M1AnalysisError(f"{label}: roots without both legs {sorted(incomplete)[:5]}")
        if len(voided) > cap:
            raise M1AnalysisError(f"{label}: {len(voided)} voided roots exceed the registered cap of {cap} ({VOID_CAP_FRACTION:.0%} of {registered_roots})")
        report[label] = {"roots_with_outcomes": len(with_outcomes), "voided_roots": len(voided), "void_cap": cap}
    return report


def merge_groups(a: dict, b: dict, registered_roots: int) -> tuple[dict, dict, dict]:
    repeated = set(a["models"]) & set(b["models"])
    if repeated != {REPEATED_MODEL}:
        raise M1AnalysisError(f"repeated model set must be exactly {{{REPEATED_MODEL}}}, got {sorted(repeated)}")
    ra, rb = a["models"][REPEATED_MODEL], b["models"][REPEATED_MODEL]
    if a["identities"].get(REPEATED_MODEL) != b["identities"].get(REPEATED_MODEL):
        raise M1AnalysisError(f"{REPEATED_MODEL}: checkpoint identity differs between groups")
    if set(ra) != set(rb):
        raise M1AnalysisError(f"{REPEATED_MODEL}: root coverage differs between groups")
    if a["voids"][REPEATED_MODEL] != b["voids"][REPEATED_MODEL]:
        raise M1AnalysisError(f"{REPEATED_MODEL}: voided roots differ between groups")
    for p in ra:
        if ra[p] != rb[p]:
            raise M1AnalysisError(f"{REPEATED_MODEL}: outcomes differ between groups at root {p}")
    seeds: dict[int, str] = {}
    for group in (a, b):
        for per in group["models"].values():
            for p, e in per.items():
                if seeds.setdefault(p, e["seed"]) != e["seed"]:
                    raise M1AnalysisError(f"models disagree on root {p} environment seed")
    models = dict(a["models"])
    voids = dict(a["voids"])
    for label, per in b["models"].items():
        models.setdefault(label, per)
        voids.setdefault(label, b["voids"][label])
    missing = REQUIRED_MODELS - set(models)
    if missing:
        raise M1AnalysisError(f"missing models: {sorted(missing)}")
    extra = set(models) - REQUIRED_MODELS
    if extra:
        raise M1AnalysisError(f"unexpected models: {sorted(extra)}")
    universe = validate_universe(models, voids, registered_roots)
    identities = dict(a["identities"])
    identities.update({k: v for k, v in b["identities"].items() if k not in identities})
    return models, voids, {"repeated_model": REPEATED_MODEL, "repeated_roots_identical": len(ra),
                           "universe": universe, "identities": identities}


def root_score(legs: dict[str, int]) -> float:
    return sum((r + 1) / 2.0 for r in legs.values()) / 2.0


def summarize(deltas: list[float]) -> dict:
    n = len(deltas)
    if n < 2:
        raise M1AnalysisError(f"too few paired roots ({n})")
    mean = sum(deltas) / n
    var = sum((d - mean) ** 2 for d in deltas) / (n - 1)
    se = math.sqrt(var / n)
    point_pp, se_pp = 100.0 * mean, 100.0 * se
    lcb_pp = point_pp - Z_ONE_SIDED_95 * se_pp
    if se_pp > 0:
        p_one_sided = 0.5 * math.erfc((point_pp / se_pp) / math.sqrt(2))
    else:
        p_one_sided = 0.0 if point_pp > 0 else 1.0
    return {"roots": n, "point_pp": point_pp, "se_pp": se_pp, "one_sided_lcb95_pp": lcb_pp, "p_one_sided": p_one_sided}


def contrast(models: dict, voids: dict, endpoint: str, reference: str, registered_roots: int) -> dict:
    a, b = models[endpoint], models[reference]
    excluded = sorted(voids[endpoint] | voids[reference])
    roots = sorted((set(a) & set(b)) - set(excluded))
    deltas = []
    for p in roots:
        if a[p]["seed"] != b[p]["seed"]:
            raise M1AnalysisError(f"{endpoint} vs {reference}: root {p} environment seeds differ")
        deltas.append(root_score(a[p]["legs"]) - root_score(b[p]["legs"]))
    complete = summarize(deltas)
    result = {"endpoint": endpoint, "reference": reference, "registered_roots": registered_roots,
              "paired_roots": complete["roots"], "excluded_voided_roots": len(excluded),
              "point_pp": complete["point_pp"], "se_pp": complete["se_pp"],
              "one_sided_lcb95_pp": complete["one_sided_lcb95_pp"], "p_one_sided": complete["p_one_sided"]}
    if excluded:
        # Worst-case missing-outcome bound: every excluded root scored as the
        # endpoint losing both legs and the reference winning both.
        worst = summarize(deltas + [-1.0] * len(excluded))
        result["worst_case_bound"] = {"roots": worst["roots"], "point_pp": worst["point_pp"],
                                      "one_sided_lcb95_pp": worst["one_sided_lcb95_pp"], "p_one_sided": worst["p_one_sided"]}
    # The statistic Holm ranks and tests: the conservative (larger) one-sided
    # p-value when a worst-case bound exists, else the complete-case one.
    result["p_for_holm"] = max(result["p_one_sided"], result.get("worst_case_bound", {}).get("p_one_sided", 0.0))
    return result


def gate(c: dict) -> bool:
    """Milestone gate; also records the two component gates on the contrast so a
    report-only reading of the worst-case rule can be taken from the same output."""
    complete = bool(c["one_sided_lcb95_pp"] > 0.0 and c["point_pp"] >= POINT_FLOOR_PP)
    worst = c.get("worst_case_bound")
    worst_gate = None if worst is None else bool(worst["one_sided_lcb95_pp"] > 0.0 and worst["point_pp"] >= POINT_FLOOR_PP)
    c["gate_complete_case"] = complete
    c["gate_worst_case"] = worst_gate
    return complete and (worst_gate is None or worst_gate)


def winrate(models: dict, voids: dict, label: str) -> dict:
    wins = draws = losses = 0
    for p, entry in models[label].items():
        if p in voids[label]:
            continue
        for r in entry["legs"].values():
            if r > 0:
                wins += 1
            elif r < 0:
                losses += 1
            else:
                draws += 1
    games = wins + draws + losses
    return {"wins": wins, "draws": draws, "losses": losses, "games": games,
            "winrate_half_credit_draws": (wins + 0.5 * draws) / games if games else None}


def analyze(group_a_root: str, group_b_root: str, registered_roots: int = REGISTERED_ROOTS,
            registered: dict | None = None) -> dict:
    registered = registered or REGISTERED
    a = read_group(group_a_root, registered, registered_roots)
    b = read_group(group_b_root, registered, registered_roots)
    models, voids, checks = merge_groups(a, b, registered_roots)
    primary = contrast(models, voids, *PRIMARY, registered_roots)
    primary["milestone"] = gate(primary)
    secondaries = [contrast(models, voids, e, r, registered_roots) for e, r in SECONDARIES]
    ranked = sorted(secondaries, key=lambda c: c["p_for_holm"])
    still_significant = True
    for i, c in enumerate(ranked):
        threshold = HOLM_ALPHA / (len(ranked) - i)
        c["holm_threshold"] = threshold
        c["holm_significant"] = bool(still_significant and c["p_for_holm"] <= threshold)
        if not c["holm_significant"]:
            still_significant = False
        c["milestone_under_holm"] = bool(c["holm_significant"] and gate(c))
    comeasured = [contrast(models, voids, e, r, registered_roots) for e, r in COMEASURED]
    return {
        "schema": "mtg-kernel-cycle4-m1-cp7-analysis/v1",
        "registered": {**registered, "roots": registered_roots, "shard_pairs": SHARD_PAIRS,
                       "void_cap_fraction": VOID_CAP_FRACTION, "point_floor_pp": POINT_FLOOR_PP},
        "shards": {"group_a": a["shards"], "group_b": b["shards"],
                   "summary_sha256": {"group_a": a["summary_sha256"], "group_b": b["summary_sha256"]}},
        "checks": checks,
        "winrates_vs_cp7": {m: winrate(models, voids, m) for m in sorted(models)},
        "primary": primary,
        "secondaries": secondaries,
        "comeasured_vs_cycle3_g2048": comeasured,
        "non_claims": "M1 informs the continue/escalate decision; it cannot alter the routing record. Co-measured contrasts carry no gate.",
    }


def self_test() -> None:
    import random
    import tempfile

    registered = dict(REGISTERED)
    registered["base_seed"] = 7
    n, sp = 256, SHARD_PAIRS

    def identity_of(label):
        return {"loaded_generation": 2048, "loaded_run_sha256": label, "loaded_checkpoint_sha256": label + "-ckpt"}

    def write_group(root, labels, shift, void=None, corrupt_task=False):
        for s in range(n // sp):
            shard = os.path.join(root, f"shard-{s:02d}")
            os.makedirs(os.path.join(shard, "tasks"))
            per_model, tasks = {}, []
            for label in labels:
                voided = sorted(p for p in (void or {}).get(label, []) if s * sp <= p < (s + 1) * sp)
                per_model[label] = {"voided_pair_indices": voided}
                stem = f"{label}-p{s * sp:06d}-n{sp:03d}"
                # The runner ends a segment at each voided pair and continues in <stem>-voidNN.
                ranges, start = [], s * sp
                for v in voided:
                    ranges.append((start, v - start + 1, [v]))
                    start = v + 1
                if start < (s + 1) * sp:
                    ranges.append((start, (s + 1) * sp - start, []))
                segments = []
                for attempt, (first, count, seg_voids) in enumerate(ranges):
                    name = (stem if attempt == 0 else f"{stem}-void{attempt:02d}") + ".outcome.jsonl"
                    path = os.path.join(shard, "tasks", name)
                    with open(path, "w", encoding="utf-8") as h:
                        h.write(json.dumps({"record_type": "header", "checkpoint": identity_of(label)}) + "\n")
                        for p in range(first, first + count):
                            seed = format(p * 7919 + 17, "016x")
                            legs = ("p0",) if p in seg_voids else ("p0", "p1")  # a voided pair keeps its first leg
                            for seat in legs:
                                draw = random.Random(f"{label}:{p}:{seat}").random()
                                reward = 1 if draw < 0.5 + shift.get(label, 0.0) else -1
                                h.write(json.dumps({"record_type": "terminal", "pair_index": p, "candidate_seat": seat,
                                                    "candidate_terminal_reward": reward,
                                                    "pair_environment_seed_u64_hex": seed}) + "\n")
                    corrupted = corrupt_task and label == labels[0] and s == 0 and attempt == 0
                    if corrupted:
                        with open(path, "a", encoding="utf-8") as h:
                            h.write("\n")
                    segments.append({"label": label, "first_pair": first, "pair_count": count, "attempt": attempt,
                                     "voided_pairs": seg_voids, "outcome": path,
                                     "outcome_sha256": "0" * 64 if corrupted else sha256_file(path)})
                tasks.append({"label": label, "first_pair": s * sp, "pair_count": sp, "segments": segments})
            plan = {"schema": registered["plan_schema"],
                    "panel": {"pair_start": s * sp, "pair_count": sp, "read_pairs": n, "mode": "formal", "base_seed": 7,
                              "tasks": [{"label": label, "first_pair": s * sp, "pair_count": sp, "stem": f"{label}-p{s * sp:06d}-n{sp:03d}"} for label in labels]},
                    "inputs": {"scorer_sha256": registered["scorer_sha256"], "runner_sha256": registered["runner_sha256"],
                               "mage_commit": registered["mage_commit"],
                               "models": {label: {"checkpoint": identity_of(label)} for label in labels}}}
            plan_path = os.path.join(shard, "panel-plan.json")
            json.dump(plan, open(plan_path, "w"))
            json.dump({"schema": registered["summary_schema"], "mode": "formal", "base_seed": 7, "pair_start": s * sp, "pairs": sp,
                       "tolerate_engine_faults": True, "plan": {"sha256": sha256_file(plan_path)},
                       "voids": {"per_model": per_model}, "tasks": tasks}, open(os.path.join(shard, "panel-summary.json"), "w"))

    registered_local = dict(registered)
    registered_local["read_pairs"] = n
    registered_local["admitted"] = {label: (2048, label, label + "-ckpt") for label in ("treatment-rb", "control-r", "g896", "static-rb", "cycle3-g2048")}
    shift = {"treatment-rb": 0.2}
    A = ["treatment-rb", "control-r", "g896"]; B = ["static-rb", "cycle3-g2048", "treatment-rb"]
    with tempfile.TemporaryDirectory() as tmp:
        a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        write_group(a, A, shift, void={"g896": [3]}); write_group(b, B, shift)
        out = analyze(a, b, registered_roots=n, registered=registered_local)
        assert out["primary"]["paired_roots"] == n - 1 and out["primary"]["excluded_voided_roots"] == 1
        assert "worst_case_bound" in out["primary"] and out["primary"]["worst_case_bound"]["roots"] == n
        assert out["primary"]["point_pp"] > 0
        assert all("milestone" not in c for c in out["comeasured_vs_cycle3_g2048"])
        assert all("milestone_under_holm" in c and "milestone" not in c for c in out["secondaries"])
        assert out["winrates_vs_cp7"]["g896"]["games"] == 2 * (n - 1)
        assert out["checks"]["identities"]["treatment-rb"]["loaded_run_sha256"] == "treatment-rb"
    def expect_refusal(label, build, needle):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
            build(a, b)
            try:
                analyze(a, b, registered_roots=n, registered=registered_local)
            except M1AnalysisError as error:
                assert needle in str(error), (label, str(error))
                return
            raise AssertionError(f"{label}: must refuse")
    def missing_summary(a, b):
        write_group(a, A, shift); write_group(b, B, shift); os.remove(os.path.join(b, "shard-01", "panel-summary.json"))
    def void_cap(a, b):
        write_group(a, A, shift, void={"control-r": list(range(6))}); write_group(b, B, shift)
    def repeat_mismatch(a, b):
        write_group(a, A, shift); write_group(b, B, {"treatment-rb": 0.1})
    def corrupt(a, b):
        write_group(a, A, shift, corrupt_task=True); write_group(b, B, shift)
    def wrong_seed(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(p)); d["base_seed"] = 8; json.dump(d, open(p, "w"))
    expect_refusal("missing summary", missing_summary, "not completed")
    expect_refusal("void cap", void_cap, "cap")
    expect_refusal("repeat mismatch", repeat_mismatch, "differ between groups")
    expect_refusal("corrupt task", corrupt, "recorded SHA-256")
    expect_refusal("wrong seed", wrong_seed, "base_seed")

    def swapped_labels(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(p))
        for t in d["tasks"]:
            if t["label"] == "control-r":
                t["label"] = "g896"
                for seg in t["segments"]:
                    seg["label"] = "g896"
        json.dump(d, open(p, "w"))
    expect_refusal("swapped labels", swapped_labels, "is not the runner's segment file")

    def wrong_identity(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-plan.json"); d = json.load(open(p))
        d["inputs"]["models"]["g896"]["checkpoint"]["loaded_generation"] = 896
        json.dump(d, open(p, "w"))
        sp_ = os.path.join(a, "shard-00", "panel-summary.json"); sd = json.load(open(sp_)); sd["plan"]["sha256"] = sha256_file(p); json.dump(sd, open(sp_, "w"))
    expect_refusal("wrong identity", wrong_identity, "not the admitted")

    def short_partition(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(p))
        seg = d["tasks"][0]["segments"][-1]
        seg["pair_count"] -= 1
        dropped_root = seg["first_pair"] + seg["pair_count"]
        rows = [l for l in open(seg["outcome"], encoding="utf-8").read().split("\n") if l]
        kept = [l for l in rows if json.loads(l).get("record_type") != "terminal" or int(json.loads(l)["pair_index"]) != dropped_root]
        open(seg["outcome"], "w", encoding="utf-8").write("\n".join(kept) + "\n")
        seg["outcome_sha256"] = sha256_file(seg["outcome"])
        json.dump(d, open(p, "w"))
    expect_refusal("short partition", short_partition, "segments stop at pair")

    def row_outside_segment(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(p))
        d["tasks"][0]["segments"][-1]["pair_count"] -= 1
        json.dump(d, open(p, "w"))
    expect_refusal("row outside segment", row_outside_segment, "outside the segment range")

    def misnamed_retry(a, b):
        write_group(a, A, shift, void={"control-r": [5]}); write_group(b, B, shift)
        p = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(p))
        task = next(t for t in d["tasks"] if t["label"] == "control-r")
        assert len(task["segments"]) == 2 and os.path.basename(task["segments"][1]["outcome"]).endswith("-void01.outcome.jsonl")
        task["segments"][1]["attempt"] = 0
        json.dump(d, open(p, "w"))
    expect_refusal("misnamed retry", misnamed_retry, "not numbered")

    def reduced_header(a, b):
        write_group(a, A, shift); write_group(b, B, shift)
        sp_ = os.path.join(a, "shard-00", "panel-summary.json"); d = json.load(open(sp_))
        seg = d["tasks"][0]["segments"][0]
        lines = open(seg["outcome"], encoding="utf-8").read().split("\n")
        lines[0] = json.dumps({"record_type": "header", "checkpoint": {"loaded_generation": 2048}})
        open(seg["outcome"], "w", encoding="utf-8").write("\n".join(lines))
        seg["outcome_sha256"] = sha256_file(seg["outcome"])
        json.dump(d, open(sp_, "w"))
    expect_refusal("reduced header", reduced_header, "not exactly the planned")
    print("self-test ok")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--group-a")
    parser.add_argument("--group-b")
    parser.add_argument("--output")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    if not (args.group_a and args.group_b and args.output):
        parser.error("--group-a, --group-b and --output are required")
    try:
        result = analyze(args.group_a, args.group_b)
    except M1AnalysisError as error:
        print(f"analyze_m1_cp7_panel_v1: REFUSED: {error}")
        return 3
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(result, handle, indent=2, sort_keys=True)
    p = result["primary"]
    print(f"primary {p['endpoint']} vs {p['reference']}: point {p['point_pp']:+.2f} pp, LCB95 {p['one_sided_lcb95_pp']:+.2f} pp, roots {p['paired_roots']}, milestone {p['milestone']}")
    for c in result["secondaries"]:
        print(f"secondary {c['endpoint']} vs {c['reference']}: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}, Holm {c['holm_significant']}, milestone {c['milestone_under_holm']}")
    for c in result["comeasured_vs_cycle3_g2048"]:
        print(f"co-measured {c['endpoint']} vs cycle3-g2048: point {c['point_pp']:+.2f} pp, LCB95 {c['one_sided_lcb95_pp']:+.2f}")
    for m, w in result["winrates_vs_cp7"].items():
        print(f"winrate vs CP7 {m}: {w['winrate_half_credit_draws']:.4f} over {w['games']} games")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
