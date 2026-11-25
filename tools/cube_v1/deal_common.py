"""The common opponent C(P) of the paired build comparison (reads (b') and (c), dose reads).

A new pinned file beside deal.py, so deal.py keeps its hash and read (b)'s dealer
(`deal_read_b_opponent`, A1.5) stays untouched.

C(P) is one deck dealt by the same rule `deal_read_b_opponent` uses for one seat (colours drawn,
eligible spells whose colour set is a subset of the deck's colours, nonbasic lands, basics split
by the constants), with the same constants, from the active pool minus all 45 cards of the seat's
pool P. Basic lands are exempt from the exclusion. C(P) depends on P and the seeds only, never on
a build of P, so every compared build of P faces one opponent that shares no card with either.

Dealer key (A6.17 and read plan v3 change 3; run-free by design):
  ["cube_deal_common_v1", namespace, draft, seat, opponent]
  The dealer appends ["colors"], ["spells"] or ["nonbasic_lands"] before each A1.4 item key.
Game seeds (A6.17 and read plan v3, section 3; they carry the run):
  opponent 0 of (c), (b'), dose: seed([namespace, "game", run, draft, seat, 0, j]) for j = 0 to 3;
  D13: seed([namespace, "game", run, draft, seat, o, j, q]) for o, j in 0, 1 and contrast q.
The focal deck is session player 0 (on the play) at even j and player 1 at odd j.
"""

from __future__ import annotations

from pathlib import Path
import sys
from typing import Any, Iterable

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402

COMMON_KEY = "cube_deal_common_v1"
COMMON_GAMES = 4
POOL_CARDS = 45
SEATS = 8
U8 = (1 << 8) - 1
U64 = (1 << 64) - 1


def _is_int(value: Any) -> bool:
    return type(value) is int


def _check_key(namespace: Any, draft: Any, seat: Any, opponent: Any) -> None:
    if not isinstance(namespace, str) or not namespace:
        raise ValueError(f"namespace must be a non-empty string, got {namespace!r}")
    if not (_is_int(draft) and 0 <= draft <= U64):
        raise ValueError(f"draft must be a u64 integer, got {draft!r}")
    if not (_is_int(seat) and 0 <= seat < SEATS):
        raise ValueError(f"seat must be an integer in [0, {SEATS - 1}], got {seat!r}")
    if not (_is_int(opponent) and 0 <= opponent <= U8):
        raise ValueError(f"opponent must be a u8 integer, got {opponent!r}")


def extract_card_ids(source: Any, label: str = "card source") -> set[int]:
    """Extract registry card ids from a set/list of ints, a cube table JSON, or a card index JSON."""
    if isinstance(source, (set, frozenset)):
        ids = set()
        for cid in source:
            if not _is_int(cid):
                raise ValueError(f"{label}: card id must be an integer, got {cid!r}")
            ids.add(cid)
        return ids
    if isinstance(source, list):
        ids = set()
        for item in source:
            if _is_int(item):
                ids.add(item)
            elif isinstance(item, dict) and _is_int(item.get("card_id")):
                ids.add(item["card_id"])
            else:
                raise ValueError(f"{label}: unsupported card entry {item!r}")
        return ids
    if isinstance(source, dict):
        if isinstance(source.get("cards"), list):
            ids = set()
            for entry in source["cards"]:
                if not isinstance(entry, dict):
                    raise ValueError(f"{label}: cards entry must be an object, got {entry!r}")
                cid = entry.get("card_id")
                if cid is not None:
                    if not _is_int(cid):
                        raise ValueError(f"{label}: card_id must be an integer, got {cid!r}")
                    ids.add(cid)
            return ids
        if isinstance(source.get("card_ids"), list):
            return extract_card_ids(source["card_ids"], label)
    raise ValueError(f"{label}: cannot extract card ids from {type(source).__name__}")


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
    """A6.17's complete key prefix for C(P), as serialized in `dealt.key` and hashed by the dealer."""
    _check_key(namespace, draft, seat, opponent)
    return [COMMON_KEY, namespace, draft, seat, opponent]


def deal_common_opponent(
    pool_state: dict[str, Any],
    constants: dict[str, Any],
    pool_cards: list[int],
    namespace: str,
    draft: int,
    seat: int,
    opponent: int,
    *,
    card_index: Any = None,
    cube_table: Any = None,
) -> dict[str, Any]:
    """Deal C(P) for pool P (the seat's 45 card ids) at (namespace, draft, seat, opponent).

    `pool_state` and `constants` are deal.validate_pool's and deal.validate_constants' results.
    When `card_index` and/or `cube_table` are given, asserts that every dealt nonbasic card has a
    row in each pinned source as well as in the active pool and outside P (A6.17, D0.4).
    """
    _check_key(namespace, draft, seat, opponent)
    pool_ids = validate_pool_cards(pool_state, pool_cards)
    active_nonbasics = {card["card_id"] for card in pool_state["spells"]} | {
        card["card_id"] for card in pool_state["nonbasic_lands"]
    }
    basic_ids = set(pool_state["basic_id_by_color"].values())
    excluded = {cid for cid in pool_ids if cid not in basic_ids}
    index_ids = None if card_index is None else extract_card_ids(card_index, "card_index")
    table_ids = None if cube_table is None else extract_card_ids(cube_table, "cube_table")

    key_prefix = common_key(namespace, draft, seat, opponent)

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

    dealt_nonbasics = [card["card_id"] for card in spells] + [card["card_id"] for card in lands]
    for cid in dealt_nonbasics:
        if cid not in active_nonbasics:
            raise ValueError(f"common opponent dealt nonbasic {cid} outside active pool")
        if cid in excluded:
            raise ValueError(f"common opponent dealt nonbasic {cid} inside seat pool P")
        if index_ids is not None and cid not in index_ids:
            raise ValueError(f"common opponent dealt nonbasic {cid} has no row in pinned card_index")
        if table_ids is not None and cid not in table_ids:
            raise ValueError(f"common opponent dealt nonbasic {cid} has no row in pinned cube_table")

    basics_ids = deal._split_basics(colors, basic_count, constants["basic_split"], pool_state["basic_id_by_color"])
    mainboard = sorted(dealt_nonbasics + basics_ids)
    return {
        "label": f"cube-common-d{draft}-s{seat}-o{opponent}-{''.join(colors)}",
        "mainboard": mainboard,
        "sideboard": [],
    }


def common_game_seed(namespace: str, run: str, draft: int, seat: int, opponent: int, slot: int,
                     contrast: int | None = None) -> int:
    """The game seed of slot j against C(P) at opponent o; D13 games append the contrast q."""
    _check_key(namespace, draft, seat, opponent)
    if not isinstance(run, str) or not run:
        raise ValueError(f"run must be a non-empty string, got {run!r}")
    if not (_is_int(slot) and slot >= 0):
        raise ValueError(f"slot must be an integer >= 0, got {slot!r}")
    parts: list[Any] = [namespace, "game", run, draft, seat, opponent, slot]
    if contrast is not None:
        if not (_is_int(contrast) and contrast >= 0):
            raise ValueError(f"contrast must be an integer >= 0, got {contrast!r}")
        parts.append(contrast)
    return deal.seed(parts)
