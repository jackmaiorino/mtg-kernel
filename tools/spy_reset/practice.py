"""Discovery-efficiency practice track (discovery-efficiency-v1): bank, priorities, schedules.

collab LANES/discovery-efficiency-plan-20261012, RUNNER.md sections 1-4.
Pure functions of the namespace, the frozen inputs and native replay rows. No
source outcome, combo label or value enters membership, cut choice or the
schedule; outcomes and parent values enter only the M priority.

CLI (JSON summaries on stdout):
  starts-jobs  --lineage L --archive A --bank B --decks D --out JOBS     whole-game replay jobs
  value-jobs   --lineage L --archive A --bank B --decks D --starts S --out JOBS
  priorities   --lineage L --archive A --bank B --starts S --values V --out BANK.json
  config       --lineage L --arm O|U|M --decks D --bank BANK.json --parent P --a48 A
               --workers W --output-directory DIR --first F --count N --out CFG
               (iterations F..F+N of the 810-update schedule; --source SRC overrides
               the initial source for chained blocks)
  panel        --id ID --candidate SRC --opponent-name T1|H --opponent OPP --decks D --out DIR
  seeds        --out PATH       every new seed, checked against the earlier campaigns
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import panel as pilot_panel  # noqa: E402
import pilot  # noqa: E402

NS = "discovery-efficiency-v1"
DECKS, SPY = pilot.DECKS, pilot.SPY
MAIN_REPEATS, WASH_REPEATS = 40, 10
MAIN_UPDATES = DECKS * DECKS * MAIN_REPEATS // pilot.UNITS_PER_UPDATE  # 648
WASH_UPDATES = DECKS * DECKS * WASH_REPEATS // pilot.UNITS_PER_UPDATE  # 162
EVAL_REPEATS = 4
ARMS = ("O", "U", "M")
sha, seed, draw = pilot.sha, pilot.seed, pilot.draw


def kind(j: int) -> str:
    return pilot.opponent_kind(j % 10)  # 3 current, 5 initial, 2 fixed per ten


# ---- sources and cuts (RUNNER section 2) ----

def sources(lineage: str, archive: Path, bank: Path) -> list[dict]:
    """The 245 usable discovery games and the 144 non-Spy plain P guides."""
    out = []
    for line in archive.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if r["lineage"] == lineage and r["usable"]:
                out.append({"id": r["id"], "origin": "archive", "own": r["own"], "other": r["other"],
                            "seat": r["seat"], "seed": r["seed"], "starting_player": r["starting_player"],
                            "actions": r["actions"], "recorded_end": r["end"]})
    rows = pilot.read_bank(bank, lineage)
    for r in sorted(rows.values(), key=lambda r: r["id"]):
        if r["own"] == SPY:
            continue
        if not r["usable"]:
            raise ValueError(f"plain source {r['id']} is unusable: a required cell is missing")
        g = r["guides"]["P"]
        out.append({"id": r["id"], "origin": "plain", "own": r["own"], "other": r["other"], "seat": r["seat"],
                    "seed": r["seed"], "starting_player": r["starting_player"],
                    "actions": r["source_prefix"] + g["actions"], "recorded_end": g["end"]})
    return out


def episode(eid: str, decks: list[dict], own: int, other: int, seat: int, seed_: int, start: int,
            reset: dict | None) -> dict:
    seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
    value = {"id": eid, "seed": seed_, "starting_player": start, "learner_seat": seat,
             "registered": [dict(d) for d in seats], "selected": [dict(d) for d in seats],
             "postboard": False, "max_physical_decisions": pilot.MAX_PHYSICAL_DECISIONS,
             "max_policy_steps": pilot.MAX_POLICY_STEPS, "runtime_rules": pilot.RUNTIME_RULES}
    if reset is not None:
        value["reset"] = reset
    return value


def source_episode(src: dict, decks: list[dict], reset: dict | None = None) -> dict:
    return episode(f"{NS}-source-{src['id']}", decks, src["own"], src["other"], src["seat"], src["seed"],
                   src["starting_player"], reset)


def cut_indices(n: int) -> list[int]:
    """floor((N-1)j/15) for j = 0..15 and the final min(8, N) starts, deduplicated."""
    if n < 1:
        return []
    return sorted({(n - 1) * j // 15 for j in range(16)} | set(range(n - min(8, n), n)))


def read_rows(path: Path, kind_: str) -> dict:
    rows = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            r = json.loads(line)
            if r.get("kind") == "error":
                raise ValueError(f"{path}: {r}")
            if r.get("kind") == kind_:
                rows[r["id"]] = r
    return rows


def reset_of(src: dict, start: dict) -> dict:
    return {"guide": f"{src['id']}/c{start['step']}", "game_seed": src["seed"],
            "prefix": src["actions"][:start["step"]],
            "cut_physical_decision_id": start["physical_decision_id"],
            "cut_observation_sha256": start["obs_hash"], "cut_menu_sha256": start["menu_hash"],
            "cut_state_hash": start["state_hash"]}


def candidate_cuts(srcs: list[dict], starts: dict) -> list[tuple[dict, dict]]:
    out = []
    for s in srcs:
        st = starts[f"src/{s['id']}"]["starts"]
        out += [(s, st[i]) for i in cut_indices(len(st))]
    return out


def source_return(src: dict, starts_row: dict) -> int:
    w = starts_row["winner"]
    r = 0 if w is None else (1 if w == src["seat"] else -1)
    expected = {"win": 1, "loss": -1, "draw": 0}[src["recorded_end"]]
    if r != expected:
        raise ValueError(f"{src['id']}: replayed outcome {r} differs from the recorded {src['recorded_end']}")
    return r


def priorities(lineage: str, srcs: list[dict], starts: dict, values: dict) -> dict:
    """The frozen U and M distributions per matched stratum (RUNNER section 2)."""
    strata: dict = {}
    for s in srcs:
        strata.setdefault((s["own"], s["other"], s["seat"], s["starting_player"]), []).append(s)
    out, report = {}, {"sources": len(srcs), "cuts": 0, "values_nonfinite": 0}
    for key_ in sorted(strata):
        games = sorted(strata[key_], key=lambda s: s["id"])
        cells = []
        for s in games:
            row = starts[f"src/{s['id']}"]
            st = row["starts"]
            idx = cut_indices(len(st))
            R = source_return(s, row)
            for i in idx:
                v = values[f"cut/{s['id']}/{st[i]['step']}"]
                if not v["finite"]:
                    raise ValueError(f"non-finite parent value at {s['id']} step {st[i]['step']}")
                clamped = max(-1.0, min(1.0, v["value"]))
                cells.append({"source": s["id"], "origin": s["origin"], "start": st[i], "start_index": i,
                              "starts": len(st), "R": R, "value": v["value"], "value_clamped": clamped,
                              "gap": max(0.0, R - clamped), "u": 1.0 / len(games) / len(idx)})
        total = sum(c["u"] * c["gap"] for c in cells)
        for c in cells:
            c["p_u"] = c["u"]
            c["p_m"] = c["u"] if total == 0 else 0.5 * c["u"] + 0.5 * c["u"] * c["gap"] / total
        out[",".join(map(str, key_))] = cells
        report["cuts"] += len(cells)
    return {"schema": f"{NS}/practice-bank", "lineage": lineage, "strata": out, "report": report}


# ---- training schedule (RUNNER section 3) ----

def allowed_starts(bank: dict, own: int, other: int) -> list[int]:
    have = [{int(k.split(",")[3]) for k in bank["strata"] if k.startswith(f"{own},{other},{seat},")}
            for seat in (0, 1)]
    allowed = sorted(have[0] & have[1])
    if not allowed:
        raise ValueError(f"no common starting player for ordered pair ({own}, {other})")
    return allowed


def phase_units(lineage: str, bank: dict, phase: str) -> list[dict]:
    repeats = MAIN_REPEATS if phase == "main" else WASH_REPEATS
    units = []
    for o in range(DECKS):
        for t in range(DECKS):
            allowed = allowed_starts(bank, o, t)
            by_kind: dict = {}
            for j in range(repeats):
                by_kind.setdefault(kind(j), []).append(j)
            for k, js in by_kind.items():
                ranked = sorted(js, key=lambda j: sha([NS, phase, "rank", lineage, o, t, j]))
                for rank, j in enumerate(ranked):
                    units.append({"own": o, "other": t, "j": j, "opponent": k,
                                  "starting_player": allowed[rank % len(allowed)],
                                  "reset": phase == "main" and rank < len(js) // 2,
                                  "seed": seed([f"{NS}/{phase}", lineage, o, t, j])})
    units.sort(key=lambda u: sha([NS, phase, "order", lineage, u["own"], u["other"], u["j"]]))
    return units


def pick(cells: list[dict], prob: str, r: float) -> int:
    acc = 0.0
    for i, c in enumerate(cells):
        acc += c[prob]
        if r < acc:
            return i
    return len(cells) - 1


def schedule(lineage: str, bank: dict) -> list[list[dict]]:
    """810 updates of five seat-pair units; reset units carry both arms' draws."""
    out = []
    for phase, n in (("main", MAIN_UPDATES), ("wash", WASH_UPDATES)):
        units = phase_units(lineage, bank, phase)
        assert len(units) == n * pilot.UNITS_PER_UPDATE
        for u in range(n):
            row = []
            for unit in units[u * pilot.UNITS_PER_UPDATE:(u + 1) * pilot.UNITS_PER_UPDATE]:
                for seat in (0, 1):
                    e = {**unit, "seat": seat, "phase": phase}
                    if unit["reset"]:
                        cells = bank["strata"][f"{unit['own']},{unit['other']},{seat},{unit['starting_player']}"]
                        r = draw([NS, "cut", lineage, unit["own"], unit["other"], unit["j"], seat]) / 2**64
                        iu, im = pick(cells, "p_u", r), pick(cells, "p_m", r)
                        e["draw"] = {"r": r, "U": {"index": iu, "p_u": cells[iu]["p_u"], "p_m": cells[iu]["p_m"]},
                                     "M": {"index": im, "p_u": cells[im]["p_u"], "p_m": cells[im]["p_m"]}}
                    row.append(e)
            out.append(row)
    return out


def iterations(lineage: str, arm: str, decks: list[dict], bank: dict, srcs: dict) -> tuple[list, dict]:
    plan = schedule(lineage, bank)
    its, summary = [], {"full": 0, "reset": 0, "unique_sources": set(), "unique_cuts": set(), "repeated_cuts": 0}
    for row in plan:
        items = []
        for e in row:
            reset = None
            if arm != "O" and e.get("draw"):
                cell = bank["strata"][f"{e['own']},{e['other']},{e['seat']},{e['starting_player']}"][e["draw"][arm]["index"]]
                reset = reset_of(srcs[cell["source"]], cell["start"])
                k = (cell["source"], cell["start"]["step"])
                summary["repeated_cuts"] += k in summary["unique_cuts"]
                summary["unique_cuts"].add(k)
                summary["unique_sources"].add(cell["source"])
            summary["reset" if reset else "full"] += 1
            eid = f"{NS}-{lineage}-{e['phase']}-o{e['own']}-t{e['other']}-j{e['j']}-s{e['seat']}"
            items.append({"episode": episode(eid, decks, e["own"], e["other"], e["seat"], e["seed"],
                                             e["starting_player"], reset),
                          "opponent": pilot.opponent_assignment(e["opponent"])})
        its.append({"episodes": items})
    for k in ("unique_sources", "unique_cuts"):
        summary[k] = len(summary[k])
    return its, summary


# ---- evaluation panel (RUNNER section 4) ----

def eval_cases() -> list[dict]:
    return [{"own": o, "other": t, "repeat": k, "seat": seat, "seed": seed([f"{NS}/eval", o, t, k]),
             "starting_player": (k + o + t) % 2}
            for o in range(DECKS) for t in range(DECKS) for k in range(EVAL_REPEATS) for seat in (0, 1)]


def build_panel(checkpoint_id: str, candidate: dict, opponent_name: str, opponent: dict, decks: list[dict],
                out: Path, shards: int = 16) -> dict:
    """pilot panel.build with the new cases and namespace (same evaluator semantics)."""
    original = pilot_panel.cases
    pilot_panel.cases = eval_cases
    try:
        plan = pilot_panel.build(checkpoint_id, candidate, opponent_name, opponent, decks, out, shards)
    finally:
        pilot_panel.cases = original
    plan["schema"] = f"{NS}/public-panel"
    pilot_panel.npp.write_json(Path(out) / "plan.json", plan)
    return plan


def all_seeds() -> dict:
    out = {"eval": [c["seed"] for c in eval_cases() if c["seat"] == 0]}
    for lineage in pilot.LINEAGES:
        for phase, n in (("main", MAIN_REPEATS), ("wash", WASH_REPEATS)):
            out[f"{lineage}/{phase}"] = [seed([f"{NS}/{phase}", lineage, o, t, j])
                                         for o in range(DECKS) for t in range(DECKS) for j in range(n)]
    return out


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    for name in ("starts-jobs", "value-jobs", "priorities"):
        p = sub.add_parser(name)
        p.add_argument("--lineage", required=True)
        p.add_argument("--archive", type=Path, required=True)
        p.add_argument("--bank", type=Path, required=True)
        p.add_argument("--out", type=Path, required=True)
        if name != "priorities":
            p.add_argument("--decks", type=Path, required=True)
        if name != "starts-jobs":
            p.add_argument("--starts", type=Path, required=True)
        if name == "priorities":
            p.add_argument("--values", type=Path, required=True)
    c = sub.add_parser("config")
    for k in ("lineage", "arm", "output-directory"):
        c.add_argument(f"--{k}", required=True)
    for k in ("decks", "bank", "parent", "a48", "out", "archive", "plain"):
        c.add_argument(f"--{k}", type=Path, required=True)
    c.add_argument("--source", type=Path)
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--first", type=int, required=True)
    c.add_argument("--count", type=int, required=True)
    pp = sub.add_parser("panel")
    for k in ("id", "opponent-name"):
        pp.add_argument(f"--{k}", required=True)
    for k in ("candidate", "opponent", "decks", "out"):
        pp.add_argument(f"--{k}", type=Path, required=True)
    sub.add_parser("seeds").add_argument("--out", type=Path, required=True)
    a = ap.parse_args(argv)
    if a.op == "seeds":
        new = all_seeds()
        flat = [s for v in new.values() for s in v]
        old = pilot.all_seeds()
        old_set = {x["seed"] for v in old.values() for x in v}
        clash = sorted(set(flat) & old_set)
        dup = len(flat) - len(set(flat))
        res = {"count": len(flat), "duplicates": dup, "clash_with_reset_pilot": clash}
        a.out.write_text(json.dumps({**res, "seeds": new}) + "\n")
        print(json.dumps(res))
        return 0 if not clash and not dup else 1
    if a.op == "panel":
        decks = json.loads(a.decks.read_text())
        plan = build_panel(a.id, json.loads(a.candidate.read_text()), a.opponent_name,
                           json.loads(a.opponent.read_text()), decks, a.out)
        print(json.dumps({"panel": plan["panel"], "requests": len(plan["requests"]), "cases": plan["cases"]}))
        return 0
    if a.op == "config":
        decks = json.loads(a.decks.read_text())
        bank = json.loads(a.bank.read_text())
        srcs = {s["id"]: s for s in sources(a.lineage, a.archive, a.plain)}
        its, summary = iterations(a.lineage, a.arm, decks, bank, srcs)
        source = json.loads((a.source or a.parent).read_text())
        config = {"schema": pilot.ndb.SCHEMA, "initial_source": source,
                  "opponents": [{"id": pilot.FIXED_OPPONENT_ID, "source": json.loads(a.a48.read_text())}],
                  "iterations": its[a.first:a.first + a.count], **json.loads(json.dumps(pilot.ndb.FROZEN)),
                  "update_backend": {"kind": "cpu"}, "collection_workers": a.workers,
                  "preparation_workers": a.workers, "output_directory": a.output_directory}
        print(json.dumps({**pilot.write_json(a.out, config), "summary": summary}))
        return 0
    srcs = sources(a.lineage, a.archive, a.bank)
    if a.op == "starts-jobs":
        decks = json.loads(a.decks.read_text())
        with a.out.open("w") as f:
            for s in srcs:
                f.write(json.dumps({"id": f"src/{s['id']}", "episode": source_episode(s, decks),
                                    "actions": s["actions"]}, separators=(",", ":")) + "\n")
        print(json.dumps({"jobs": len(srcs), "archive": sum(s["origin"] == "archive" for s in srcs),
                          "plain": sum(s["origin"] == "plain" for s in srcs)}))
        return 0
    starts = read_rows(a.starts, "practice_starts")
    if a.op == "value-jobs":
        decks = json.loads(a.decks.read_text())
        n = 0
        with a.out.open("w") as f:
            for s, st in candidate_cuts(srcs, starts):
                f.write(json.dumps({"id": f"cut/{s['id']}/{st['step']}",
                                    "episode": source_episode(s, decks, reset_of(s, st))},
                                   separators=(",", ":")) + "\n")
                n += 1
        print(json.dumps({"jobs": n}))
        return 0
    values = read_rows(a.values, "practice_value")
    bank = priorities(a.lineage, srcs, starts, values)
    data = pilot.write_json(a.out, bank)
    print(json.dumps({**data, "report": bank["report"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
