"""E supported-choice CE pilot (e-supported-choice-ce-v1-20261012): frozen schedules and run configs.

collab LANES/search-policy-target-audit-20261012, DESIGN.md and CONTRACT.md. Pure functions of
the namespace, lineage, phase and indices: no model is loaded and no outcome is read.

Keys are UTF-8 compact JSON objects with sorted keys. Integer seeds are the first eight
SHA-256 bytes, little-endian, masked to 63 bits; ranks use the full digest. O and T share
every key (no arm field), so both arms see identical exogenous schedules.

CLI (JSON on stdout):
  config  --lineage r1|r2 --arm O|T --phase main|persistence --parent SRC.json --a48 SRC.json
          [--source SRC.json] --workers W --output-directory DIR [--first F --count N] --out CFG
  seeds   --out PATH       every new game seed, checked against the earlier campaigns' lists
  summary                  schedule balance checks (decks, seats, opponents, starting players)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import Counter
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "python" / "tools"))
sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
import nine_deck_baseline_v1 as ndb  # noqa: E402

NS = "e-supported-choice-ce-v1-20261012"
LINEAGES = ("r1", "r2")
ARMS = ("O", "T")
PHASES = ("main", "persistence")
DECKS = 9
FAMILIES = 5
UNITS_PER_UPDATE = 5
UPDATES = DECKS * DECKS * FAMILIES // UNITS_PER_UPDATE  # 81 per phase
PARENT_ID, A48_ID = "initial-parent", ndb.FIXED_OPPONENT_ID
PRIOR_DECK_INDICES = list(range(DECKS))
SELECTION_TRANSITION_CAP = 512_000
MASK = (1 << 63) - 1
RUNTIME_RULES = "resolution-boundary-v1"


def key(role: str, **fields) -> bytes:
    return json.dumps({"namespace": NS, "role": role, **fields}, sort_keys=True,
                      separators=(",", ":"), ensure_ascii=False).encode("utf-8")


def rank(role: str, **fields) -> bytes:
    return hashlib.sha256(key(role, **fields)).digest()


def seed(role: str, **fields) -> int:
    return int.from_bytes(rank(role, **fields)[:8], "little") & MASK


def check(lineage: str, phase: str) -> None:
    if lineage not in LINEAGES or phase not in PHASES:
        raise ValueError(f"unknown lineage/phase {lineage!r}/{phase!r}")


def updates(lineage: str, phase: str) -> list[list[dict]]:
    """81 updates of ten games. Each own deck's 45 opposing-deck/family seat-pair units are
    hash-ordered and cut into nine five-unit batches; own decks interleave in a fixed hashed
    order over nine rounds, so every update holds one own deck. Within a unit, one focal seat
    meets the frozen parent and the other A48, by an independent hash."""
    check(lineage, phase)
    batches = {}
    for own in range(DECKS):
        units = [(other, family) for other in range(DECKS) for family in range(FAMILIES)]
        units.sort(key=lambda u: (rank("unit_order", lineage=lineage, phase=phase, own=own, other=u[0],
                                       family=u[1]), u))
        batches[own] = [units[i * UNITS_PER_UPDATE:(i + 1) * UNITS_PER_UPDATE] for i in range(DECKS)]
    order = sorted(range(DECKS), key=lambda o: (rank("deck_order", lineage=lineage, phase=phase, own=o), o))
    out = []
    for round_ in range(DECKS):
        for own in order:
            entries = []
            for other, family in batches[own][round_]:
                game = seed("episode", lineage=lineage, phase=phase, own=own, other=other, family=family)
                parent_seat = rank("opponent_seat", lineage=lineage, phase=phase, own=own, other=other,
                                   family=family)[0] & 1
                for seat in (0, 1):
                    entries.append({"own": own, "other": other, "family": family, "seat": seat, "seed": game,
                                    "starting_player": (own + other + family) % 2,
                                    "opponent": "parent" if seat == parent_seat else "a48"})
            out.append(entries)
    return out


def case_id(lineage: str, phase: str, update: int, episode: int) -> str:
    return f"{NS}/{lineage}/{phase}/u{update:03d}/e{episode}"


def query_episode(lineage: str, phase: str, update: int) -> int:
    """Outcome-blind choice of the update's queried episode (one per update, so nine per own deck)."""
    return min(range(2 * UNITS_PER_UPDATE),
               key=lambda e: (rank("query_episode", lineage=lineage, phase=phase, update=update, episode=e), e))


def episode(lineage: str, phase: str, update: int, index: int, entry: dict, decks: list[dict]) -> dict:
    own, other, seat = entry["own"], entry["other"], entry["seat"]
    seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
    return {"id": f"{NS}-{lineage}-{phase}-u{update:03d}-o{own}-t{other}-f{entry['family']}-s{seat}",
            "seed": entry["seed"], "starting_player": entry["starting_player"], "learner_seat": seat,
            "registered": [dict(d) for d in seats], "selected": [dict(d) for d in seats], "postboard": False,
            "max_physical_decisions": ndb.MAX_PHYSICAL_DECISIONS, "max_policy_steps": ndb.MAX_POLICY_STEPS,
            "runtime_rules": RUNTIME_RULES}


def config(lineage: str, arm: str, phase: str, parent: dict, a48: dict, source: dict | None, workers: int,
           output_directory: str, decks: list[dict], first: int = 0, count: int = UPDATES) -> dict:
    """One native run of updates first..first+count of the phase. The teacher (the lineage's
    parent, B1 FPU1.5 depth32 in the engine's E method) and its schedule are the same for O
    and T; T trains on the targets, O only records them. Persistence updates query nothing."""
    if arm not in ARMS:
        raise ValueError(f"unknown arm {arm!r}")
    plan = updates(lineage, phase)[first:first + count]
    iterations, slots = [], []
    for offset, entries in enumerate(plan):
        u = first + offset
        iterations.append({"episodes": [
            {"episode": episode(lineage, phase, u, i, e, decks),
             "opponent": {"kind": "fixed", "id": PARENT_ID if e["opponent"] == "parent" else A48_ID}}
            for i, e in enumerate(entries)]})
        slots.append([{"trajectory_index": query_episode(lineage, phase, u),
                       "case_id": case_id(lineage, phase, u, query_episode(lineage, phase, u))}]
                     if phase == "main" else [])
    return {"schema": ndb.SCHEMA, "initial_source": source or parent,
            "opponents": [{"id": A48_ID, "source": a48}, {"id": PARENT_ID, "source": parent}],
            "iterations": iterations, **json.loads(json.dumps(ndb.FROZEN)), "update_backend": {"kind": "cpu"},
            "collection_workers": workers,
            "e_supported": {"teacher": parent, "treatment": arm == "T", "prior_deck_indices": PRIOR_DECK_INDICES,
                            "selection_transition_cap": SELECTION_TRANSITION_CAP, "slots": slots},
            "output_directory": output_directory}


def summary() -> dict:
    out = {}
    for lineage in LINEAGES:
        for phase in PHASES:
            plan = updates(lineage, phase)
            games = [e for u in plan for e in u]
            pairs = Counter((e["own"], e["other"]) for e in games)
            out[f"{lineage}/{phase}"] = {
                "updates": len(plan), "games": len(games),
                "games_per_ordered_pair": sorted(set(pairs.values())),
                "one_own_deck_per_update": all(len({e["own"] for e in u}) == 1 for u in plan),
                "opponents_per_update": sorted({tuple(sorted(Counter(e["opponent"] for e in u).items()))
                                                for u in plan}),
                "seats": dict(Counter(e["seat"] for e in games)),
                "starting_player": dict(Counter(e["starting_player"] for e in games)),
                "queried_per_own_deck": dict(sorted(Counter(plan[u][query_episode(lineage, phase, u)]["own"]
                                                            for u in range(len(plan))).items())),
            }
    return out


def all_seeds() -> dict:
    return {f"{lineage}/{phase}": sorted({e["seed"] for u in updates(lineage, phase) for e in u})
            for lineage in LINEAGES for phase in PHASES}


def registered_seeds() -> set[int]:
    """Seeds of the earlier reset pilot and discovery-efficiency campaigns (their own generators)."""
    import pilot
    found = {x["seed"] for v in pilot.all_seeds().values() for x in v}
    practice_dir = Path(r"D:\mtg-kernel-spy-reset\tools\spy_reset")
    if (practice_dir / "practice.py").exists():
        sys.path.insert(0, str(practice_dir))
        import practice
        found |= {s for v in practice.all_seeds().values() for s in v}
    return found


def write_json(path: Path, value) -> dict:
    data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
    Path(path).write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    c = sub.add_parser("config")
    for k in ("lineage", "arm", "phase", "output-directory"):
        c.add_argument(f"--{k}", required=True)
    for k in ("parent", "a48", "out"):
        c.add_argument(f"--{k}", type=Path, required=True)
    c.add_argument("--source", type=Path)
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--first", type=int, default=0)
    c.add_argument("--count", type=int, default=UPDATES)
    sub.add_parser("seeds").add_argument("--out", type=Path, required=True)
    sub.add_parser("summary")
    a = ap.parse_args(argv)
    if a.op == "summary":
        print(json.dumps(summary(), indent=1))
        return 0
    if a.op == "seeds":
        new = all_seeds()
        flat = [s for v in new.values() for s in v]
        clash = sorted(set(flat) & registered_seeds())
        res = {"count": len(flat), "duplicates": len(flat) - len(set(flat)), "clash_with_registered": clash}
        a.out.write_text(json.dumps({**res, "seeds": new}) + "\n")
        print(json.dumps(res))
        return 0 if not clash and res["duplicates"] == 0 else 1
    decks = ndb.load_decks(REPO / "data" / "runtime_decks_v1.json")
    cfg = config(a.lineage, a.arm, a.phase, json.loads(a.parent.read_text()), json.loads(a.a48.read_text()),
                 json.loads(a.source.read_text()) if a.source else None, a.workers,
                 a.output_directory, decks, a.first, a.count)
    print(json.dumps(write_json(a.out, cfg)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
