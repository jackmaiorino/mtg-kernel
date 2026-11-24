"""The common opponent C(P) of the paired build comparison (reads (b') and (c), dose reads).

A new pinned file beside deal.py, so deal.py keeps its hash and read (b)'s dealer
(`deal_read_b_opponent`, A1.5) stays untouched.

C(P) is one deck dealt by the same rule `deal_read_b_opponent` uses for one seat (colours drawn,
eligible spells whose colour set is a subset of the deck's colours, nonbasic lands, basics split
by the constants), with the same constants, from the active pool minus all 45 cards of the seat's
pool P. Basic lands are exempt from the exclusion. C(P) depends on P and the seeds only, never on
a build of P, so every compared build of P faces one opponent that shares no card with either.

Key prefix [proposed for A6]:
  ["cube_deal_common_v1", namespace, draft, seat, opponent,
   seed(["cube_deal_common_v1", namespace, draft, seat, opponent])]
Game seeds of its 4 games [proposed for A6]:
  seed(["cube_deal_common_v1", namespace, draft, seat, opponent, slot]) for slot 0 to 3; the
  compared build sits in session seat 0 (on the play) in slots 0 and 2 and in seat 1 (on the draw)
  in slots 1 and 3.
"""

from __future__ import annotations

from pathlib import Path
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402

COMMON_KEY = "cube_deal_common_v1"
COMMON_GAMES = 4
POOL_CARDS = 45
SEATS = 8


def _is_int(value: Any) -> bool:
    return type(value) is int


def _check_key(namespace: Any, draft: Any, seat: Any, opponent: Any) -> None:
    if not isinstance(namespace, str) or not namespace:
        raise ValueError(f"namespace must be a non-empty string, got {namespace!r}")
    if not (_is_int(draft) and draft >= 0):
        raise ValueError(f"draft must be an integer >= 0, got {draft!r}")
    if not (_is_int(seat) and 0 <= seat < SEATS):
        raise ValueError(f"seat must be an integer in [0, {SEATS - 1}], got {seat!r}")
    if not (_is_int(opponent) and opponent >= 0):
        raise ValueError(f"opponent must be an integer >= 0, got {opponent!r}")


def validate_pool_cards(pool_state: dict[str, Any], pool_cards: Any) -> list[int]:
    """P is the seat's 45 cards: distinct ids, each a card of the active pool or a basic land."""
    if not isinstance(pool_cards, list) or len(pool_cards) != POOL_CARDS:
        raise ValueError(f"a seat's pool must be a list of {POOL_CARDS} card ids")
    known = {card["card_id"] for card in pool_state["spells"]} | {
        card["card_id"] for card in pool_state["nonbasic_lands"]
    }
    basic_ids = set(pool_state["basic_id_by_color"].values())
    for cid in pool_cards:
        if not _is_int(cid):
            raise ValueError(f"pool card id must be an integer, got {cid!r}")
        if cid not in known and cid not in basic_ids:
            raise ValueError(f"pool card id {cid} is neither a card of the active pool nor a basic land")
    nonbasic = [cid for cid in pool_cards if cid not in basic_ids]
    if len(set(nonbasic)) != len(nonbasic):
        raise ValueError("a seat's pool holds a nonbasic card twice")
    return list(pool_cards)


def common_key(namespace: str, draft: int, seat: int, opponent: int) -> list[Any]:
    """The key tuple of C(P), as the pod record's `dealt.key` holds it."""
    _check_key(namespace, draft, seat, opponent)
    return [namespace, draft, seat, opponent]


def deal_common_opponent(
    pool_state: dict[str, Any],
    constants: dict[str, Any],
    pool_cards: list[int],
    namespace: str,
    draft: int,
    seat: int,
    opponent: int,
) -> dict[str, Any]:
    """Deal C(P) for pool P (the seat's 45 card ids) at (namespace, draft, seat, opponent).

    `pool_state` and `constants` are deal.validate_pool's and deal.validate_constants' results.
    """
    _check_key(namespace, draft, seat, opponent)
    pool_ids = validate_pool_cards(pool_state, pool_cards)
    basic_ids = set(pool_state["basic_id_by_color"].values())
    excluded = {cid for cid in pool_ids if cid not in basic_ids}

    deal_seed = deal.seed([COMMON_KEY, namespace, draft, seat, opponent])
    key_prefix = [COMMON_KEY, namespace, draft, seat, opponent, deal_seed]

    nonland_spells_count = constants["nonland_spells"]
    nonbasic_lands_count = constants["nonbasic_lands"]
    basic_count = constants["deck_size"] - nonland_spells_count - nonbasic_lands_count

    colors = deal._draw_ordered(
        list(deal.COLORS), constants["colours_per_seat"], key_prefix + ["colors"], lambda c: c
    )
    allowed = set(colors)
    eligible = [
        card
        for card in pool_state["spells"]
        if card["card_id"] not in excluded and set(card["colors"]).issubset(allowed)
    ]
    if len(eligible) < nonland_spells_count:
        raise ValueError(
            f"common opponent draft {draft} seat {seat} opponent {opponent} ({''.join(colors)}): "
            f"need {nonland_spells_count} eligible spells, only {len(eligible)} remain"
        )
    spells = deal._draw_ordered(eligible, nonland_spells_count, key_prefix + ["spells"], lambda c: c["card_id"])

    eligible_lands = [card for card in pool_state["nonbasic_lands"] if card["card_id"] not in excluded]
    if len(eligible_lands) < nonbasic_lands_count:
        raise ValueError(
            f"common opponent draft {draft} seat {seat} opponent {opponent}: "
            f"need {nonbasic_lands_count} nonbasic lands, only {len(eligible_lands)} remain"
        )
    lands = deal._draw_ordered(
        eligible_lands, nonbasic_lands_count, key_prefix + ["nonbasic_lands"], lambda c: c["card_id"]
    )

    basics_ids = deal._split_basics(colors, basic_count, constants["basic_split"], pool_state["basic_id_by_color"])
    mainboard = sorted([card["card_id"] for card in spells] + [card["card_id"] for card in lands] + basics_ids)
    return {
        "label": f"cube-common-d{draft}-s{seat}-o{opponent}-{''.join(colors)}",
        "mainboard": mainboard,
        "sideboard": [],
    }


def common_games(namespace: str, draft: int, seat: int, opponent: int) -> list[dict[str, int]]:
    """The 4 games every compared build of P plays against C(P): slot, game seed and the compared
    build's session seat (0 on the play in slots 0 and 2, 1 on the draw in slots 1 and 3)."""
    _check_key(namespace, draft, seat, opponent)
    return [
        {
            "slot": slot,
            "game_seed": deal.seed([COMMON_KEY, namespace, draft, seat, opponent, slot]),
            "build_seat": slot % 2,
        }
        for slot in range(COMMON_GAMES)
    ]
