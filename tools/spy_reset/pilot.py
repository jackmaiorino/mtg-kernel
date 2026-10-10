"""Terminal-only reset curriculum pilot (spy-reset-pilot-v1): frozen schedules.

collab LANES/spy-learning-pilot-plan-20261010, RUNNER.md sections 3-4.
Pure functions of the namespace, lineage and indices: no model is loaded and
no outcome is read. Seeds are the first eight SHA-256 bytes of the compact
JSON key, little-endian, masked to 2**63 - 1 (``seed``); anchor draws use all
64 bits (``draw``).

CLI (JSON on stdout):
  decks      --out PATH                 nine registered decks (label, mainboard)
  schedule   --out PATH                 the frozen training schedule, both lineages
  bank-spec  --lineage r1 --decks PATH --out PATH
  seeds      --out PATH                 every concrete game seed, for the
                                        disjointness check against earlier panels
"""
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from fractions import Fraction
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "python" / "tools"))
import nine_deck_baseline_v1 as ndb  # noqa: E402

NS = "spy-reset-pilot-v1"
LINEAGES = ("r1", "r2")
DECKS = 9
REPEATS = 10
UNITS_PER_UPDATE = 5
UPDATES = DECKS * DECKS * REPEATS // UNITS_PER_UPDATE  # 162
RESETS_PER_PAIR = 5
Q = (Fraction(1), Fraction(3, 4), Fraction(1, 2), Fraction(1, 4), Fraction(0))
SPY, CAWGATES = 4, 7
MASK = (1 << 63) - 1
MAX_PHYSICAL_DECISIONS = ndb.MAX_PHYSICAL_DECISIONS
MAX_POLICY_STEPS = ndb.MAX_POLICY_STEPS
RUNTIME_RULES = "resolution-boundary-v1"
PANEL_REPEATS = 12


def key(parts: list) -> bytes:
    return json.dumps(parts, separators=(",", ":")).encode()


def sha(parts: list) -> str:
    return hashlib.sha256(key(parts)).hexdigest()


def draw(parts: list) -> int:
    return int.from_bytes(hashlib.sha256(key(parts)).digest()[:8], "little")


def seed(parts: list) -> int:
    return draw(parts) & MASK


def check_lineage(lineage: str) -> None:
    if lineage not in LINEAGES:
        raise ValueError(f"unknown lineage {lineage!r}")


# ---- guide sources (RUNNER section 4) ----

def guide_opponent(own: int, other: int, seat: int) -> str:
    return "T1" if (own + other + seat) % 2 == 0 else "A48"


def source_id(lineage: str, own: int, other: int, seat: int) -> str:
    return f"{lineage}-o{own}-t{other}-s{seat}"


def sources(lineage: str, domain: str = "") -> list[dict]:
    """Guide sources. ``domain="qualification"`` gives the separate
    engineering sources (same layout, ``q-`` ids, qualification seeds)."""
    check_lineage(lineage)
    prefix = f"{NS}/{domain}/" if domain else f"{NS}/"
    out = []
    for own in range(DECKS):
        for other in range(DECKS):
            for seat in (0, 1):
                base = [lineage, own, other, seat]
                sid = ("q-" if domain else "") + source_id(lineage, own, other, seat)
                out.append({
                    "id": sid, "own": own, "other": other, "seat": seat,
                    "opp": guide_opponent(own, other, seat),
                    "seed": seed([f"{prefix}source", *base]),
                    "starting_player": own % 2,
                    "source_policy_seeds": [seed([f"{prefix}source", *base, "policy", k]) for k in (0, 1)],
                    "anchor_draw": draw([f"{prefix}anchor", *base]),
                    "guide_policy_seeds": [seed([f"{prefix}guide", *base, "policy", k]) for k in (0, 1)],
                    "search_root": f"{prefix}search/{sid}",
                })
    return out


def guide_crosstab(lineage: str) -> dict:
    """Per learner deck: guide opponent x focal on play/draw."""
    table = {}
    for s in sources(lineage):
        on_play = s["starting_player"] == s["seat"]
        cell = table.setdefault(str(s["own"]), {"T1": [0, 0], "A48": [0, 0]})
        cell[s["opp"]][0 if on_play else 1] += 1
    return table


def bank_spec(lineage: str, decks: list[dict], qualification: int = 0) -> dict:
    """The formal bank spec, or with ``qualification=N`` N engineering
    sources in a fixed hash order (qualification domain)."""
    if qualification:
        srcs = sorted(sources(lineage, "qualification"), key=lambda s: sha([NS, "qualification-order", s["id"]]))
        srcs = srcs[:qualification]
    else:
        srcs = sources(lineage)
    return {"schema": "spy-reset-pilot-v1/bank-spec", "namespace": NS, "lineage": lineage,
            "runtime_rules": RUNTIME_RULES, "select_rule": "fpu-1.5",
            "max_physical_decisions": MAX_PHYSICAL_DECISIONS, "max_policy_steps": MAX_POLICY_STEPS,
            "decks": [{"label": d["label"], "mainboard": d["mainboard"]} for d in decks],
            "sources": srcs}


# ---- training schedule (RUNNER section 3) ----

def opponent_kind(j: int) -> str:
    return ndb.opponent_kind(j)  # 0-2 current, 3-7 initial (the parent), 8-9 fixed A48


def unit_seed(lineage: str, own: int, other: int, j: int) -> int:
    return seed([f"{NS}/train", lineage, own, other, j])


def reset_repeats(lineage: str, own: int, other: int) -> list[int]:
    """The five j values of (own, other) that are reset units in P and S."""
    ranked = sorted(range(REPEATS), key=lambda j: sha([NS, "cut", lineage, own, other, j]))
    return sorted(ranked[:RESETS_PER_PAIR])


def updates(lineage: str) -> list[list[tuple[int, int, int]]]:
    check_lineage(lineage)
    units = [(o, t, j) for o in range(DECKS) for t in range(DECKS) for j in range(REPEATS)]
    units.sort(key=lambda u: sha([NS, lineage, *u]))
    return [units[i:i + UNITS_PER_UPDATE] for i in range(0, len(units), UNITS_PER_UPDATE)]


def schedule(lineage: str) -> dict:
    """Unit order, reset assignment and each guide's q sequence in training order."""
    resets = {(o, t): reset_repeats(lineage, o, t) for o in range(DECKS) for t in range(DECKS)}
    uses: dict[tuple[int, int, int], list] = {}
    plan = []
    for u, update in enumerate(updates(lineage)):
        row = []
        for o, t, j in update:
            is_reset = j in resets[(o, t)]
            for seat in (0, 1):
                entry = {"own": o, "other": t, "j": j, "seat": seat, "opponent": opponent_kind(j),
                         "seed": unit_seed(lineage, o, t, j), "starting_player": (j + o + t) % 2,
                         "reset": is_reset}
                if is_reset:
                    g = uses.setdefault((o, t, seat), [])
                    entry["guide"] = source_id(lineage, o, t, seat)
                    entry["use"] = len(g)
                    entry["q"] = str(Q[len(g)])
                    g.append(u)
                row.append(entry)
        plan.append(row)
    assert all(len(v) == RESETS_PER_PAIR for v in uses.values()) and len(uses) == DECKS * DECKS * 2
    return {"lineage": lineage, "updates": plan,
            "reset_repeats": {f"{o},{t}": v for (o, t), v in sorted(resets.items())}}


def cut_index(q: Fraction, m: int) -> int:
    if m < 1:
        raise ValueError("a usable guide has at least its anchor")
    return int(q * (m - 1))  # floor, exact (q is dyadic)


def panel_seed(own: int, other: int, repeat: int) -> int:
    return seed([f"{NS}/eval", own, other, repeat])


def all_seeds() -> dict:
    out = {"source": [], "train": [], "eval": []}
    for lineage in LINEAGES:
        out["source"] += [{"lineage": lineage, "id": s["id"], "seed": s["seed"]} for s in sources(lineage)]
        out["train"] += [{"lineage": lineage, "own": o, "other": t, "j": j, "seed": unit_seed(lineage, o, t, j)}
                         for o in range(DECKS) for t in range(DECKS) for j in range(REPEATS)]
    out["eval"] = [{"own": o, "other": t, "repeat": k, "seed": panel_seed(o, t, k)}
                   for o in range(DECKS) for t in range(DECKS) for k in range(PANEL_REPEATS)]
    return out


# ---- banks, cuts and training configs (RUNNER sections 4-5) ----

ARMS = ("O", "P", "S")
FIXED_OPPONENT_ID = ndb.FIXED_OPPONENT_ID


def read_bank(path: Path, lineage: str) -> dict:
    """Source rows by id; refuses errors, duplicates and other lineages."""
    rows = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("kind") != "bank_source" or row.get("lineage") != lineage:
            raise ValueError(f"bank row is not a {lineage} source: {line[:200]}")
        if row["id"] in rows:
            raise ValueError(f"duplicate bank source {row['id']}")
        rows[row["id"]] = row
    expected = {s["id"] for s in sources(lineage)}
    if set(rows) != expected:
        raise ValueError(f"bank is incomplete: {len(expected - set(rows))} sources missing")
    return rows


def cut_of(row: dict, arm: str, q: Fraction) -> dict:
    """The reset start of guide ``arm`` at quantile ``q``."""
    guide = row["guides"][arm]
    starts = guide["starts"]
    start = starts[cut_index(q, len(starts))]
    anchor = row["anchor"]["step"]
    offset = start["step"] - anchor
    if offset < 0 or start["step"] != anchor + offset:
        raise ValueError("cut before the anchor")
    prefix = row["source_prefix"] + guide["actions"][:offset]
    if len(prefix) != start["step"]:
        raise ValueError("cut prefix length differs from its step")
    return {"guide": f"{row['id']}/{arm}/q{q}", "game_seed": row["seed"], "prefix": prefix,
            "cut_physical_decision_id": start["physical_decision_id"],
            "cut_observation_sha256": start["obs_hash"], "cut_menu_sha256": start["menu_hash"],
            "cut_state_hash": start["state_hash"]}


def episode(lineage: str, decks: list[dict], entry: dict, reset: dict | None, starting_player: int) -> dict:
    own, other, seat = entry["own"], entry["other"], entry["seat"]
    seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
    value = {"id": f"{NS}-{lineage}-o{own}-t{other}-j{entry['j']}-s{seat}", "seed": entry["seed"],
             "starting_player": starting_player, "learner_seat": seat,
             "registered": [dict(d) for d in seats], "selected": [dict(d) for d in seats],
             "postboard": False, "max_physical_decisions": MAX_PHYSICAL_DECISIONS,
             "max_policy_steps": MAX_POLICY_STEPS, "runtime_rules": RUNTIME_RULES}
    if reset is not None:
        value["reset"] = reset
    return value


def opponent_assignment(kind: str) -> dict:
    return {"kind": "fixed", "id": FIXED_OPPONENT_ID} if kind == "fixed" else {"kind": kind}


def iterations(lineage: str, arm: str, decks: list[dict], bank: dict | None) -> tuple[list[dict], dict]:
    """The arm's 162 updates and an exposure summary. O plays every unit as a
    full game; P and S play their reset units from their own guides unless
    the guide pair is unusable, when the scheduled full game stands in."""
    if arm not in ARMS:
        raise ValueError(f"unknown arm {arm!r}")
    plan = schedule(lineage)
    out, summary = [], {"full": 0, "reset": 0, "fallback_full": 0, "unique_cuts": set(), "repeated_cuts": 0}
    for update in plan["updates"]:
        items = []
        for entry in update:
            reset = None
            if arm != "O" and entry["reset"]:
                row = bank[entry["guide"]]
                if row["usable"]:
                    reset = cut_of(row, arm, Fraction(entry["q"]))
                else:
                    summary["fallback_full"] += 1
            if reset is None:
                summary["full"] += 1
                start = entry["starting_player"]
            else:
                summary["reset"] += 1
                key_ = (reset["guide"].rsplit("/q", 1)[0], len(reset["prefix"]))
                if key_ in summary["unique_cuts"]:
                    summary["repeated_cuts"] += 1
                summary["unique_cuts"].add(key_)
                start = bank[entry["guide"]]["starting_player"]
            items.append({"episode": episode(lineage, decks, entry, reset, start),
                          "opponent": opponent_assignment(entry["opponent"])})
        out.append({"episodes": items})
    summary["unique_cuts"] = len(summary["unique_cuts"])
    return out, summary


def training_config(lineage: str, arm: str, decks: list[dict], bank: dict | None, parent: dict,
                    a48: dict, workers: int, output_directory: str) -> tuple[dict, dict]:
    its, summary = iterations(lineage, arm, decks, bank)
    config = {"schema": ndb.SCHEMA, "initial_source": parent,
              "opponents": [{"id": FIXED_OPPONENT_ID, "source": a48}], "iterations": its,
              **json.loads(json.dumps(ndb.FROZEN)), "update_backend": {"kind": "cpu"},
              "collection_workers": workers, "preparation_workers": workers,
              "output_directory": output_directory}
    return config, summary


def load_decks() -> list[dict]:
    return ndb.load_decks(REPO / "data" / "runtime_decks_v1.json")


def write_json(path: Path, value) -> dict:
    data = (json.dumps(value, separators=(",", ":")) + "\n").encode()
    path.write_bytes(data)
    return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="op", required=True)
    for name in ("decks", "schedule", "seeds"):
        sub.add_parser(name).add_argument("--out", type=Path, required=True)
    c = sub.add_parser("config")
    c.add_argument("--lineage", required=True)
    c.add_argument("--arm", required=True, choices=ARMS)
    c.add_argument("--decks", type=Path, required=True)
    c.add_argument("--bank", type=Path)
    c.add_argument("--parent", type=Path, required=True)
    c.add_argument("--a48", type=Path, required=True)
    c.add_argument("--workers", type=int, required=True)
    c.add_argument("--output-directory", required=True)
    c.add_argument("--updates", type=int, default=UPDATES, help="first N updates only (qualification)")
    c.add_argument("--out", type=Path, required=True)
    b = sub.add_parser("bank-spec")
    b.add_argument("--lineage", required=True)
    b.add_argument("--decks", type=Path, required=True)
    b.add_argument("--out", type=Path, required=True)
    b.add_argument("--qualification", type=int, default=0)
    a = ap.parse_args(argv)
    if a.op == "decks":
        print(json.dumps(write_json(a.out, load_decks())))
    elif a.op == "schedule":
        value = {"schema": "spy-reset-pilot-v1/schedule", "namespace": NS, "q": [str(q) for q in Q],
                 "lineages": {lin: schedule(lin) for lin in LINEAGES},
                 "guide_crosstab": {lin: guide_crosstab(lin) for lin in LINEAGES}}
        print(json.dumps(write_json(a.out, value)))
    elif a.op == "config":
        decks = json.loads(a.decks.read_text())
        bank = read_bank(a.bank, a.lineage) if a.arm != "O" else None
        config, summary = training_config(a.lineage, a.arm, decks, bank, json.loads(a.parent.read_text()),
                                          json.loads(a.a48.read_text()), a.workers, a.output_directory)
        config["iterations"] = config["iterations"][:a.updates]
        print(json.dumps({**write_json(a.out, config), "summary": summary}))
    elif a.op == "seeds":
        print(json.dumps(write_json(a.out, all_seeds())))
    else:
        decks = json.loads(a.decks.read_text())
        print(json.dumps(write_json(a.out, bank_spec(a.lineage, decks, a.qualification))))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
