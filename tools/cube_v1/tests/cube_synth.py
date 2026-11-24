"""Synthetic pools, plans and pod records for the reader tests (namespace `test` only).

Records are written to the frozen schema (mtg-kernel-cube-pod-record/v1) with uniform seats and a
uniform pilot. Game results are chosen by the test, so every expected answer is known exactly.
"""

from __future__ import annotations

import copy
import random
from pathlib import Path
import sys
from typing import Any, Callable

CUBE_V1_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(CUBE_V1_DIR))

import deal  # noqa: E402
import pod_record  # noqa: E402
import read_plans  # noqa: E402

NS = "test"
RUN = "synth"
COLORS = deal.COLORS
BASIC_BASE = 1000
UNIFORM = {"kind": "uniform", "pin": None}
STATED = {"kind": "stated", "pin": None}
EVEN = (0, 2, 4, 6)
ODD = (1, 3, 5, 7)

CONSTANTS = {
    "deck_size": 40,
    "colours_per_seat": 2,
    "nonland_spells": 23,
    "nonbasic_lands": 1,
    "basic_split": "even_first_colour_extra",
    "pairs_per_update": 2,
}


def pool_doc() -> dict[str, Any]:
    """A synthetic active pool large enough for 8 disjoint 45-card pools and every deal."""
    spells = []
    cid = 1
    for color in COLORS:
        for k in range(60):
            spells.append({"card_id": cid, "name": f"Synth {color} {k}", "colors": [color], "is_land": False})
            cid += 1
    for k in range(20):
        spells.append({"card_id": cid, "name": f"Synth Colourless {k}", "colors": [], "is_land": False})
        cid += 1
    for k in range(20):
        pair = [COLORS[k % 5], COLORS[(k + 1 + k // 5) % 5]]
        if pair[0] == pair[1]:
            pair = [COLORS[k % 5], COLORS[(k + 2) % 5]]
        spells.append({"card_id": cid, "name": f"Synth Gold {k}", "colors": pair, "is_land": False})
        cid += 1
    lands = []
    for k in range(30):
        lands.append({"card_id": cid, "name": f"Synth Land {k}", "colors": [], "is_land": True})
        cid += 1
    basics = [
        {"card_id": BASIC_BASE + i, "name": f"Synth Basic {c}", "colors": [c], "is_land": True, "color": c}
        for i, c in enumerate(COLORS)
    ]
    return {"kernel_carddb_hash": "00000000synthetic", "spells": spells, "nonbasic_lands": lands, "basics": basics}


def draft_pools(draft: int) -> list[list[int]]:
    """8 disjoint 45-card pools for a synthetic draft, in a seeded order."""
    doc = pool_doc()
    ids = [c["card_id"] for c in doc["spells"]] + [c["card_id"] for c in doc["nonbasic_lands"]]
    rng = random.Random(deal.seed(["synth-pools", draft]))
    picked = rng.sample(ids, 8 * 45)
    return [picked[45 * s: 45 * (s + 1)] for s in range(8)]


def build_from_pool(pool: list[int], variant: int) -> list[int]:
    """A 40-card deck of a pool: 23 nonbasic cards of the pool and 17 basics."""
    nonbasic = sorted(pool)
    chosen = nonbasic[:23] if variant == 0 else nonbasic[-23:]
    basics = [BASIC_BASE + (variant % 5)] * 9 + [BASIC_BASE + ((variant + 1) % 5)] * 8
    return sorted(chosen + basics)


def _row(kind: str, options: list[int], chosen: int, stream: int) -> dict[str, Any]:
    return {
        "kind": kind,
        "sampler": pod_record.UNIFORM_SAMPLER,
        "options": options,
        "chosen": chosen,
        "stream_values": [stream],
        "logits_bits": None,
        "p_chosen_bits": None,
        "value_bits": None,
    }


def uniform_build_rows(cards: list[int], pool: list[int]) -> dict[str, Any]:
    """BuildRows of the shape the uniform four-step builder writes, for a deck of `pool`."""
    basics = [c for c in cards if c >= BASIC_BASE]
    nonbasic = [c for c in cards if c < BASIC_BASE]
    b = len(basics)
    remaining = sorted(set(pool))
    pointer = []
    for i, card in enumerate(nonbasic):
        pointer.append(_row("pointer", list(remaining), card, i))
        remaining.remove(card)
    mask = 0
    for c in basics:
        mask |= 1 << (c - BASIC_BASE)
    bits = bin(mask).count("1")
    type_set = None
    split = []
    if b:
        options = [m for m in range(1, 32) if bin(m).count("1") <= b]
        type_set = _row("type_set", options, mask, 99)
        if bits >= 2:
            t_bits = [1 << i for i in range(5) if mask & (1 << i)]
            split = [_row("split", t_bits, t_bits[i % len(t_bits)], 100 + i) for i in range(b - bits)]
    return {"count": _row("count", list(range(41)), b, 98), "pointer": pointer, "type_set": type_set, "split": split}


def _record_head(use: str, pod_index: int, pools: list[list[int]] | None, max_replays: int = 0) -> dict[str, Any]:
    seeds = pod_record.pod_seeds(NS, RUN, pod_index)
    draft = None
    if pools is not None:
        draft = {"schema": "cube_draft_record/v1", "draft_seed": seeds["draft_seed"], "packs": [], "picks": [],
                 "pools": [list(p) for p in pools]}
    return {
        "schema": pod_record.POD_RECORD_SCHEMA,
        "interfaces": pod_record.INTERFACES_VERSION,
        "use": use,
        "namespace": NS,
        "run": RUN,
        "pod_index": pod_index,
        "seeds": {"draft_seed": seeds["draft_seed"], "pod_seed": seeds["pod_seed"], "league_seed": None},
        "pins": {"binary": None, "cube_list": None, "fence_record": None, "card_index": None, "cube_table": None},
        "pilot": {"kind": "uniform", "pin": None, "decoding": pod_record.UNIFORM_DECODING},
        "caps": {"max_decisions": 16384, "max_physical_decisions": 16384, "max_policy_steps": 32768,
                 "watchdog_seconds": 600, "max_replays": max_replays},
        "draft": draft,
        "chairs": [],
        "decks": [],
        "games": [],
        "paired_builds": [],
        "halts": [],
        "status": "complete",
        "stop": None,
    }


def _attempt(game_seed: int, result: str) -> dict[str, Any]:
    """One attempt; `result` is p0, p1, draw, or halt (a decision-cap stop)."""
    if result == "halt":
        end = {"kind": "decision_cap"}
    else:
        winner = {"p0": 0, "p1": 1, "draw": None}[result]
        end = {"kind": "natural", "winner_seat": winner, "reason": "synthetic"}
    return {
        "attempt": 0,
        "deal_seed": game_seed,
        "env_seed": deal.seed(["env", game_seed]),
        "action_seed": deal.seed(["act", game_seed]),
        "decisions": 100,
        "physical_decisions": None,
        "policy_steps": None,
        "end": end,
        "halted": result == "halt",
        "stop_kind": pod_record.expected_stop_kind(end),
        "last_action_source": None,
    }


SCORES = {"p0": [1, -1], "p1": [-1, 1], "draw": [0, 0], "halt": None}


def side_result(result: str, position: int) -> str:
    """Turn a result for the deck at `position` (win, loss, draw, halt) into p0, p1, draw or halt."""
    if result in ("draw", "halt"):
        return result
    winner = position if result == "win" else 1 - position
    return f"p{winner}"


def _finish(record: dict[str, Any]) -> dict[str, Any]:
    record["halts"] = [
        {"game": g["game"], "attempt": a["attempt"], "stop_kind": a["stop_kind"],
         "last_action_source": a["last_action_source"], "deal_seed": a["deal_seed"]}
        for g in record["games"] for a in g["attempts"] if a["halted"]
    ]
    return pod_record.validate_record(record)


def _uniform_chairs(learning: tuple[int, ...] | None = None) -> list[dict[str, Any]]:
    return [
        {"chair": c, "role": "learning" if learning is None or c in learning else "fixture",
         "picker": dict(UNIFORM), "pick_rows": []}
        for c in range(8)
    ]


def record_from_plan(plan: dict[str, Any], result_of: Callable[[dict[str, Any], int], str]) -> dict[str, Any]:
    """Play a plan synthetically. `result_of(game, build_position)` gives the built deck's result
    (win, loss, draw or halt), where build_position is the built deck's session seat."""
    record = _record_head(plan["use"], plan["pod_index"], plan["pools"])
    record["chairs"] = _uniform_chairs()
    for deck in plan["decks"]:
        entry = copy.deepcopy(deck)
        rows = None
        if deck["source"] == "build" and deck["builder"]["kind"] != "stated":
            rows = uniform_build_rows(deck["cards"], plan["pools"][deck["chair"]])
        record["decks"].append({
            "deck": entry["deck"], "source": entry["source"], "chair": entry["chair"], "build": entry["build"],
            "builder": entry["builder"], "build_seed": None if rows is None else deal.seed(["build", deck["deck"]]),
            "cards": entry["cards"], "build_rows": rows, "dealt": entry["dealt"],
        })
    for game in plan["games"]:
        position = next(i for i, d in enumerate(game["decks"]) if plan["decks"][d]["source"] == "build")
        result = side_result(result_of(game, position), position)
        entry = copy.deepcopy(game)
        entry["attempts"] = [_attempt(game["game_seed"], result)]
        entry["scores"] = SCORES[result]
        record["games"].append(entry)
    record["paired_builds"] = copy.deepcopy(plan["paired_builds"])
    return _finish(record)


def paired_plan(draft: int, use: str = "paired_build") -> dict[str, Any]:
    pools = draft_pools(draft)
    x = [build_from_pool(p, 0) for p in pools]
    y = [build_from_pool(p, 1) for p in pools]
    if use == "paired_build":
        return read_plans.build_read_c_plan(pool_doc(), CONSTANTS, NS, RUN, draft, pools, x, y, UNIFORM)
    return read_plans.build_paired_plan(pool_doc(), CONSTANTS, use, NS, RUN, draft, pools, x, y, UNIFORM,
                                        {"kind": "uniform", "pin": "1" * 64})


def read_b_plan(draft: int) -> dict[str, Any]:
    pools = draft_pools(draft)
    return read_plans.build_read_b_plan(pool_doc(), CONSTANTS, NS, RUN, draft, pools,
                                        [build_from_pool(p, 0) for p in pools], UNIFORM)


def pairings() -> list[tuple[int, int]]:
    return [(i, j) for i in range(8) for j in range(i + 1, 8)]


def read_a_pair(pair: int, result_of: Callable[[str, str, int, int], str],
                cards_of: Callable[[str, int, str], list[int]] | None = None) -> list[dict[str, Any]]:
    """The two pods of a read (a) pair: learned seats in the even chairs, then in the odd chairs.
    `result_of(orientation, arm, pairing, learned_position)` is the learned seat's result.
    `cards_of(orientation, chair, arm)` gives a deck's cards (default: the same in both pods)."""
    pods = []
    for orientation, learning in (("even", EVEN), ("odd", ODD)):
        record = _record_head("read_a", pair, None)
        record["chairs"] = _uniform_chairs(learning)
        seeds = record["seeds"]
        for chair in range(8):
            for arm_index, arm in enumerate(("candidate", "stated")):
                cards = (cards_of or (lambda o, c, a: _a_cards(c, a)))(orientation, chair, arm)
                rows = uniform_build_rows(cards, [c for c in cards if c < BASIC_BASE] + [900 + chair]) \
                    if arm == "candidate" else None
                record["decks"].append({
                    "deck": 2 * chair + arm_index, "source": "build", "chair": chair, "build": 0,
                    "builder": dict(UNIFORM if arm == "candidate" else STATED),
                    "build_seed": None if rows is None else deal.seed(["build", pair, chair]),
                    "cards": cards, "build_rows": rows, "dealt": None,
                })
        game_id = 0
        for arm_index, arm in enumerate(("candidate", "stated")):
            for g, (i, j) in enumerate(pairings()):
                if (i in learning) == (j in learning):
                    continue
                game_seed = deal.seed([seeds["pod_seed"], g])
                chairs = [i, j] if deal.seed(["coin", game_seed]) % 2 == 0 else [j, i]
                learned_position = 0 if chairs[0] in learning else 1
                result = side_result(result_of(orientation, arm, g, learned_position), learned_position)
                record["games"].append({
                    "game": game_id, "group": g, "role": "cross", "pairing": g,
                    "decks": [2 * chairs[0] + arm_index, 2 * chairs[1] + arm_index], "chairs": chairs,
                    "seat_rule": "coin", "game_seed": game_seed, "episode": g,
                    "attempts": [_attempt(game_seed, result)], "scores": SCORES[result],
                })
                game_id += 1
        pods.append(_finish(record))
    return pods


def _a_cards(chair: int, arm: str) -> list[int]:
    start = 1 + 23 * chair + (0 if arm == "candidate" else 200)
    return sorted(list(range(start, start + 23)) + [BASIC_BASE] * 9 + [BASIC_BASE + 1] * 8)


def unit_draw(*parts: Any) -> float:
    """A deterministic uniform draw in [0, 1) from deal.seed."""
    return deal.seed(["synth-draw", *parts]) / float(1 << 63)
