"""Read plans, prep requests, manifests and frozen panels: `mtg-kernel-cube-read-plan/v3`.

Implements the signed read plan v3 (READ-PLAN-v1 sections 1 to 5 with the v2 and v3 deltas) on
`m4-interfaces/v2`. A plan tells the pod runner (D5) how to play one read pod and tells the
readers (D7) what its record must hold.

- Section 1: every file is `json.dumps(doc, indent=2, ensure_ascii=True)` plus one LF; integers only.
- Section 2: the 24 plan keys, in order (PLAN_KEYS).
- Section 3: the families, their seed sources, pod indices, chairs, decks, games and episodes.
- Section 4: prep requests and results (pools and builds without play).
- Section 5: manifests, plan-set digests and the two-stage frozen panel (`panel.json`).

`check_plan` regenerates a plan from its own identity fields, its bindings, its prep result and
the dealer inputs, and requires identical bytes, so every seed, seat, game seed, episode, dealt deck
and key is recomputed rather than trusted.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402
import deal_common  # noqa: E402
import pod_record  # noqa: E402
import seeds_v1  # noqa: E402

PLAN_SCHEMA = "mtg-kernel-cube-read-plan/v3"
INTERFACES = pod_record.INTERFACES_VERSION
RUN = "v1"
TEST = "test"
SEATS = pod_record.SEATS
POOL_SEED_NAMESPACE = "read-gate-pools"
CAPS = {
    "max_decisions": 16384,
    "max_physical_decisions": 16384,
    "max_policy_steps": 32768,
    "max_stack_objects": pod_record.MAX_STACK_OBJECTS,
    "watchdog_seconds": 600,
    "max_replays": 0,
}
UNIFORM = {"kind": "uniform", "pin": None}
STATED = {"kind": "stated", "pin": None}
UNIFORM_PILOT = {"kind": "uniform", "pin": None, "decoding": pod_record.UNIFORM_DECODING}

PLAN_KEYS = ("schema", "interfaces", "family", "use", "namespace", "run", "unit", "pod_index", "mirror",
             "mirror_pods", "rotation", "mark", "seeds", "pins", "pilot", "caps", "seat_rule", "inputs",
             "prep_sha256", "chairs", "pools", "decks", "games", "paired_builds")
PLAN_CHAIR_KEYS = ("chair", "role", "picker")
PLAN_DECK_KEYS = ("deck", "source", "chair", "build", "builder", "cards", "dealt", "row")
PLAN_GAME_KEYS = ("game", "group", "role", "pairing", "decks", "chairs", "seat_rule", "game_seed", "episode")
INPUT_KEYS = ("pool_sha256", "constants_sha256")
MARK_KEYS = ("kind", "label", "ordinal")
PREP_REQUEST_KEYS = ("schema", "kind", "family", "namespace", "run", "unit", "seeds", "pins", "chairs", "builds")
PREP_BUILD_KEYS = ("chair", "build", "builder", "row")
PREP_RESULT_KEYS = ("schema", "kind", "request_sha256", "draft", "builds")
PREP_RESULT_BUILD_KEYS = PREP_BUILD_KEYS + ("build_seed", "cards")
MANIFEST_KEYS = ("schema", "kind", "family", "namespace", "run", "set", "interfaces", "declaration_sha256", "look",
                 "units", "reserved", "pools_with", "panel_sha256", "prep_set", "replaces", "retired", "plans",
                 "plan_set_sha256", "executions")
EXECUTION_ENTRY_KEYS = ("execution", "reason", "expired_games", "binary", "platform")
PANEL_KEYS = ("schema", "kind", "namespace", "run", "units", "inputs", "pins", "prep_set_sha256",
              "prep_execution", "prep_outputs", "drafts")

# Section 3. layout: a (role-swapped pairs), one (one-seat companion), all (all-learned companion),
# b (A1.5 opponent), c (paired builds against C(P)), d13 (the panel's three contrasts).
FAMILIES: dict[str, dict[str, Any]] = {
    "aa": {"use": "read_a", "namespaces": ("read-d8-aa",), "layout": "a"},
    "a": {"use": "read_a", "namespaces": ("read-gate-a",), "layout": "a"},
    "a_early": {"use": "read_a", "namespaces": ("read-early-a",), "layout": "a"},
    "size_a": {"use": "sizing", "namespaces": ("size-gate-a",), "layout": "a"},
    "size_a_early": {"use": "sizing", "namespaces": ("size-early-a",), "layout": "a"},
    "a_one": {"use": "read_a", "namespaces": ("read-gate-a-one",), "layout": "one", "parent": "read-gate-a"},
    "a_early_one": {"use": "read_a", "namespaces": ("read-early-a-one",), "layout": "one",
                    "parent": "read-early-a"},
    "a_all": {"use": "read_a", "namespaces": ("read-gate-a-all",), "layout": "all", "parent": "read-gate-a"},
    "a_early_all": {"use": "read_a", "namespaces": ("read-early-a-all",), "layout": "all",
                    "parent": "read-early-a"},
    "b": {"use": "read_b", "namespaces": ("read-gate-b",), "layout": "b", "prep": True},
    "c": {"use": "paired_build", "namespaces": ("read-gate-c",), "layout": "c", "prep": True},
    "size_b": {"use": "sizing", "namespaces": ("size-gate-b",), "layout": "b", "prep": True},
    "size_c": {"use": "sizing", "namespaces": ("size-gate-c",), "layout": "c", "prep": True},
    "dose": {"use": "dose", "namespaces": ("dose-d8", "dose-m5"), "layout": "c", "prep": True, "mark": "dose"},
    "d13": {"use": "paired_build", "namespaces": ("panel-k1",), "layout": "d13", "prep": True, "mark": "pilot"},
}
# Builder rows per layout (section 3), in deck order within a chair; build index per row (v2 change 7).
ROWS = {"a": ("candidate", "stated"), "one": ("candidate",), "all": ("candidate",), "b": ("candidate",),
        "c": ("candidate", "stated"), "dose": ("current", "initial"),
        "d13": ("b1_last", "initial", "k1_last", "stated")}
BUILD_INDEX = {"candidate": 0, "current": 0, "initial": 0, "b1_last": 0, "k1_last": 0, "stated": 1}
DECK_ROWS = ("candidate", "stated", "current", "initial", "b1_last", "k1_last", "opponent")
# D13 contrasts (v2 change 4): q, kind, x row, y row.
D13_CONTRASTS = ((0, "current_initial", "b1_last", "initial"), (1, "k1_last_b1_last", "k1_last", "b1_last"),
                 (2, "candidate_stated", "k1_last", "stated"))
D13_ROW_DECK = {"b1_last": 0, "initial": 1, "k1_last": 2, "stated": 3}
EVEN = (0, 2, 4, 6)
ODD = (1, 3, 5, 7)
FIXTURE_CASES = {
    "a": ("swap", "planted", "halt-rows"), "a_early": ("swap", "planted", "halt-rows"),
    "aa": ("swap", "planted", "halt-rows", "mirrored-halt", "unmirrored"),
    "b": ("a15", "score", "halt-pool"), "c": ("shared", "halt-candidate", "halt-stated"),
    "a_one": ("rotation",), "a_all": ("all-count",), "dose": ("provisional",), "d13": ("panel",),
}
EDGE_CASES = ("edge-unequal", "edge-empty", "edge-all-halted", "edge-undefined", "edge-cut", "edge-extension",
              "edge-sizing", "edge-reject")


class PlanError(ValueError):
    """A plan, prep file, manifest or panel that breaks the signed read plan."""


# ---------------------------------------------------------------------------------------------
# Serialization and hashing (section 1)


def serialize(doc: Any) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=True) + "\n"


serialize_plan = serialize


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def doc_sha256(doc: Any) -> str:
    return sha256_text(serialize(doc))


def plan_set_sha256(plans: list[dict[str, Any]]) -> str:
    return sha256_text(json.dumps(plans, separators=(",", ":"), ensure_ascii=True))


# ---------------------------------------------------------------------------------------------
# Small checks


def _layout(family: str) -> str:
    if family not in FAMILIES:
        raise PlanError(f"unknown family {family!r}")
    return FAMILIES[family]["layout"]


def _rows(family: str) -> tuple[str, ...]:
    return ROWS["dose"] if family == "dose" else ROWS[_layout(family)]


def _check_namespace(family: str, namespace: str) -> None:
    if namespace != TEST and namespace not in FAMILIES[family]["namespaces"]:
        raise PlanError(f"family {family} plays in {list(FAMILIES[family]['namespaces'])} or {TEST!r}, "
                        f"not {namespace!r}")


def _is_sha(value: Any) -> bool:
    return isinstance(value, str) and bool(pod_record.SHA256_RE.match(value))


def _check_pins(pins: Any) -> dict[str, str]:
    if not isinstance(pins, dict) or list(pins) != list(pod_record.PINS_KEYS):
        raise PlanError(f"pins must be an object with keys {list(pod_record.PINS_KEYS)} in order")
    if not all(_is_sha(v) for v in pins.values()):
        raise PlanError("every read plan pin is a non-null sha256")
    return dict(pins)


def _check_pilot(pilot: Any) -> dict[str, Any]:
    if not isinstance(pilot, dict) or list(pilot) != list(pod_record.PILOT_KEYS):
        raise PlanError("pilot must be {kind, pin, decoding}")
    if pilot["kind"] == "uniform":
        if pilot != UNIFORM_PILOT:
            raise PlanError("a uniform pilot has pin null and decoding 'uniform-index-splitmix64-v1'")
    elif pilot["kind"] == "checkpoint":
        if not _is_sha(pilot["pin"]) or not isinstance(pilot["decoding"], str) or not pilot["decoding"]:
            raise PlanError("a checkpoint pilot has its sha256 and its decoding identity")
    else:
        raise PlanError(f"pilot kind must be uniform or checkpoint, got {pilot['kind']!r}")
    return dict(pilot)


def _check_policy(binding: Any, label: str, kinds: tuple[str, ...]) -> dict[str, Any]:
    if not isinstance(binding, dict) or list(binding) != ["kind", "pin"]:
        raise PlanError(f"{label} must be {{kind, pin}}")
    kind, pin = binding["kind"], binding["pin"]
    if kind not in kinds:
        raise PlanError(f"{label}.kind must be one of {list(kinds)}, got {kind!r}")
    if kind == "learned" and not _is_sha(pin):
        raise PlanError(f"{label}: a learned policy is pinned by its checkpoint sha256")
    if kind in ("uniform", "stated") and pin is not None:
        raise PlanError(f"{label}: a {kind} policy has pin null")
    return {"kind": kind, "pin": pin}


def _check_mark(family: str, mark: Any) -> dict[str, Any] | None:
    want = FAMILIES[family].get("mark")
    if want is None:
        if mark is not None:
            raise PlanError(f"family {family} has mark null")
        return None
    if not isinstance(mark, dict) or list(mark) != list(MARK_KEYS):
        raise PlanError(f"family {family} needs a mark {{kind, label, ordinal}}")
    if mark["kind"] != want or not isinstance(mark["label"], str) or not mark["label"]:
        raise PlanError(f"family {family}'s mark kind is {want!r} with a label")
    if not (type(mark["ordinal"]) is int and 0 <= mark["ordinal"] <= pod_record.U32):
        raise PlanError("mark ordinal is a u32")
    return dict(mark)


def _u64(value: Any, label: str) -> int:
    if not (type(value) is int and 0 <= value <= pod_record.U64):
        raise PlanError(f"{label} must be a u64, got {value!r}")
    return value


# ---------------------------------------------------------------------------------------------
# Section 3: identity, seeds and chairs


def seed_sources(family: str, namespace: str) -> tuple[str, str]:
    """(draft_seed_namespace, pod_seed_namespace) of a family. In `test` every source is `test`."""
    if namespace == TEST:
        return TEST, TEST
    layout = _layout(family)
    if family in ("b", "c"):
        return POOL_SEED_NAMESPACE, POOL_SEED_NAMESPACE
    if layout in ("one", "all"):
        return FAMILIES[family]["parent"], namespace
    return namespace, namespace


def plan_seeds(family: str, namespace: str, unit: int, rotation: int | None = None) -> dict[str, Any]:
    src_draft, src_pod = seed_sources(family, namespace)
    pod_parts: list[Any] = [src_pod, "pod", RUN, unit]
    if _layout(family) == "one":
        pod_parts.append(rotation)
    return {
        "draft_seed": deal.seed([src_draft, "draft", RUN, unit]),
        "pod_seed": deal.seed(pod_parts),
        "league_seed": None,
        "draft_seed_namespace": src_draft,
        "pod_seed_namespace": src_pod,
    }


def pod_index(family: str, unit: int, mirror: int | None, rotation: int | None) -> int:
    layout = _layout(family)
    if layout == "a":
        return 2 * unit + mirror
    if layout == "one":
        return 8 * unit + rotation
    return unit


def learning_chairs(family: str, mirror: int | None, rotation: int | None) -> tuple[int, ...]:
    layout = _layout(family)
    if layout == "a":
        return EVEN if mirror == 0 else ODD
    if layout == "one":
        return (rotation,)
    if family == "dose":
        return ()
    return tuple(range(SEATS))


def plan_chairs(family: str, mirror: int | None, rotation: int | None, picker: dict[str, Any] | None
                ) -> list[dict[str, Any]]:
    learning = learning_chairs(family, mirror, rotation)
    return [{"chair": c, "role": "learning" if c in learning else "fixture",
             "picker": dict(picker) if c in learning else dict(UNIFORM)} for c in range(SEATS)]


# ---------------------------------------------------------------------------------------------
# Section 3: decks and games


def _game(game: int, group: int, role: str, pairing: int | None, decks: list[int], chairs: list[int | None],
          game_seed: int, episode: int) -> dict[str, Any]:
    return {"game": game, "group": group, "role": role, "pairing": pairing, "decks": decks, "chairs": chairs,
            "seat_rule": "schedule", "game_seed": game_seed, "episode": episode}


def _built(deck_id: int, chair: int, row: str, builder: dict[str, Any], cards: list[int] | None) -> dict[str, Any]:
    return {"deck": deck_id, "source": "build", "chair": chair, "build": BUILD_INDEX[row],
            "builder": dict(builder), "cards": None if cards is None else list(cards), "dealt": None, "row": row}


def _dealt(deck_id: int, function: str, key: list[Any], cards: list[int]) -> dict[str, Any]:
    return {"deck": deck_id, "source": "dealt", "chair": None, "build": None,
            "builder": {"kind": "dealt", "pin": {"function": function, "key": list(key)}}, "cards": list(cards),
            "dealt": {"function": function, "key": list(key)}, "row": "opponent"}


def _focal(slot: int, focal: int, opponent: int, chair: int) -> tuple[list[int], list[int | None]]:
    """The focal deck is session player 0 at even slots and player 1 at odd slots."""
    if slot % 2 == 0:
        return [focal, opponent], [chair, None]
    return [opponent, focal], [None, chair]


def pairing_games(family: str, pod_seed: int, rotation: int | None) -> list[dict[str, Any]]:
    """(a) layouts and companions: games over pod_pairings() with the ordered-chair coin; episode g."""
    layout = _layout(family)
    out = []
    if layout == "a":
        cross = [g for g, (lo, hi) in enumerate(seeds_v1.pod_pairings()) if (lo % 2) != (hi % 2)]
        for k, g in enumerate(cross):
            g_seed, seats = seeds_v1.pairing_seats(pod_seed, g)
            for row in (0, 1):
                out.append(_game(2 * k + row, k, "cross", g, [2 * seats[0] + row, 2 * seats[1] + row], seats,
                                 g_seed, g))
        return out
    if layout == "one":
        chosen = [g for g, pair in enumerate(seeds_v1.pod_pairings()) if rotation in pair]
    else:
        chosen = list(range(len(seeds_v1.pod_pairings())))
    for k, g in enumerate(chosen):
        g_seed, seats = seeds_v1.pairing_seats(pod_seed, g)
        out.append(_game(k, k, "cross", g, list(seats), seats, g_seed, g))
    return out


def dealt_decks(family: str, namespace: str, unit: int, pools: list[list[int]] | None,
                built_cards: dict[tuple[int, str], list[int]] | None, pool_state: dict[str, Any] | None,
                constants: dict[str, Any] | None, *, card_index: Any = None, cube_table: Any = None
                ) -> dict[int, dict[str, Any]]:
    """The dealt opponent decks of a plan, by deck id."""
    layout = _layout(family)
    out: dict[int, dict[str, Any]] = {}
    if layout in ("a", "one", "all"):
        return out
    if pool_state is None or constants is None:
        raise PlanError(f"family {family} deals opponents: the pool and constants are required")
    for s in range(SEATS):
        if layout == "b":
            x = SEATS * unit + s
            opp = deal.deal_read_b_opponent(pool_state, constants, built_cards[(s, "candidate")], namespace, RUN, x)
            out[2 * s + 1] = _dealt(2 * s + 1, deal.READ_B_KEY, [namespace, RUN, x], opp["mainboard"])
        else:
            for o in ((0,) if layout == "c" else (0, 1)):
                deck_id = 3 * s + 2 if layout == "c" else 6 * s + 4 + o
                opp = deal_common.deal_common_opponent(
                    pool_state, constants, pools[s], namespace, unit, s, o,
                    card_index=card_index, cube_table=cube_table,
                )
                out[deck_id] = _dealt(deck_id, deal_common.COMMON_KEY,
                                      deal_common.common_key(namespace, unit, s, o), opp["mainboard"])
    return out


A15_GOLDEN_PATH = Path(__file__).resolve().parent / "tests" / "fixtures" / "a15_read_b_golden.json"
A15_GOLDEN_SHA256 = "1930ce67a7fb8403f6612c6f6af74818375b12fb4299c6d7b465da5abfd992e0"


def verify_a15_golden(path: Path | None = None, *, expected_sha256: str | None = None) -> dict[str, Any]:
    """Verify the pinned frozen A1.5 episode set (A6.9, A6.13, A6.20): exclusions, full key,
    opponent mainboard, game seeds, u64 episode slots, seat order, expanded libraries and opening hands."""
    target = A15_GOLDEN_PATH if path is None else Path(path)
    want_sha = A15_GOLDEN_SHA256 if (path is None and expected_sha256 is None) else expected_sha256
    if not target.exists():
        raise PlanError(f"frozen A1.5 golden fixture is missing at {target}")
    raw = target.read_bytes()
    actual_sha = hashlib.sha256(raw).hexdigest()
    if want_sha is not None and actual_sha != want_sha:
        raise PlanError(f"frozen A1.5 golden hash {actual_sha} differs from pinned {want_sha}")
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PlanError(f"frozen A1.5 golden is unreadable JSON: {exc}") from exc
    if not isinstance(doc, dict) or doc.get("schema") != "mtg-kernel-cube-a15-golden/v1":
        raise PlanError("frozen A1.5 golden has unexpected schema")
    ns, run = doc["namespace"], doc["run"]
    unit, chair, x = doc["unit"], doc["chair"], doc["dealer_index"]
    if x != SEATS * unit + chair or run != RUN:
        raise PlanError("frozen A1.5 golden index or run disagrees with A6.13")
    pool_state = deal.validate_pool(doc["pool"])
    const_state = deal.validate_constants(doc["constants"])
    drafted = doc["drafted_deck"]
    expected = doc["expected"]

    # Check A1.5 deal.build_read_b AND read_plans' own (b) dealer/game schedule against the golden.
    ref = deal.build_read_b(doc["pool"], doc["constants"], drafted, ns, run, x)
    episodes = [e["episode"] for e in ref["iterations"][0]["episodes"]]
    built_cards = {(s, "candidate"): drafted for s in range(SEATS)}
    dealt = dealt_decks("b", ns, unit, None, built_cards, pool_state, const_state)
    plan_games, _ = dealer_games("b", ns, unit)
    opp_deck = dealt[2 * chair + 1]
    want_dealt = {"function": deal.READ_B_KEY, "key": [ns, run, x]}
    if opp_deck["dealt"] != want_dealt or expected["dealt"] != want_dealt:
        raise PlanError("frozen A1.5 golden dealt key mismatch")
    opp_cards = opp_deck["cards"]
    if opp_cards != expected["opponent_mainboard"]:
        raise PlanError("frozen A1.5 golden opponent mainboard mismatch")
    basic_ids = set(pool_state["basic_id_by_color"].values())
    drafted_nonbasics = set(drafted) - basic_ids
    opp_nonbasics = set(opp_cards) - basic_ids
    if drafted_nonbasics & opp_nonbasics:
        raise PlanError("frozen A1.5 golden violates nonbasic exclusion between drafted deck and opponent")
    chair_games = plan_games[4 * chair: 4 * chair + 4]
    exp_games = expected["games"]
    if len(episodes) != 4 or len(chair_games) != 4 or len(exp_games) != 4:
        raise PlanError("frozen A1.5 golden must hold 4 games")
    play_count = 0
    for j, (ep, pg, eg) in enumerate(zip(episodes, chair_games, exp_games)):
        if eg["slot"] != j or pg["episode"] != j or eg["episode"] != j:
            raise PlanError(f"frozen A1.5 golden game {j} episode mismatch")
        focal_seat = pg["decks"].index(2 * chair)
        if focal_seat != ep["learner_seat"] or focal_seat != eg["focal_seat"]:
            raise PlanError(f"frozen A1.5 golden game {j} focal seat mismatch")
        if focal_seat == 0:
            play_count += 1
        g_seed = pg["game_seed"]
        if g_seed != ep["seed"] or g_seed != eg["game_seed"]:
            raise PlanError(f"frozen A1.5 golden game {j} game_seed mismatch")
        aseeds = seeds_v1.attempt_seeds(g_seed)
        if aseeds["env_seed"] != eg["env_seed"] or aseeds["action_seed"] != eg["action_seed"]:
            raise PlanError(f"frozen A1.5 golden game {j} attempt seed mismatch")
        deck_map = {2 * chair: drafted, 2 * chair + 1: opp_cards}
        p0_ids = deck_map[pg["decks"][0]]
        p1_ids = deck_map[pg["decks"][1]]
        if p0_ids != ep["selected"][0]["mainboard"] or p1_ids != ep["selected"][1]["mainboard"]:
            raise PlanError(f"frozen A1.5 golden game {j} selected decks disagree with build_read_b")
        if p0_ids != eg["p0_deck_ids"] or p1_ids != eg["p1_deck_ids"]:
            raise PlanError(f"frozen A1.5 golden game {j} deck ids disagree with golden")
        replayed = seeds_v1.opening_deal(aseeds["env_seed"], p0_ids, p1_ids)
        for field in ("p0_expanded_library", "p1_expanded_library", "p0_opening_hand",
                      "p1_opening_hand", "p0_remaining_library", "p1_remaining_library"):
            if replayed[field] != eg[field]:
                raise PlanError(f"frozen A1.5 golden game {j} {field} mismatch")
    if play_count != 2:
        raise PlanError("frozen A1.5 golden must place focal deck 2 on the play and 2 on the draw")
    return {"sha256": actual_sha, "games": 4, "namespace": ns, "dealer_index": x}


def dealer_games(family: str, namespace: str, unit: int) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Games and paired builds of the dealer layouts (b, c, dose, d13)."""
    layout = _layout(family)
    games: list[dict[str, Any]] = []
    pairs: list[dict[str, Any]] = []
    for s in range(SEATS):
        if layout == "b":
            x = SEATS * unit + s
            for j in range(deal.READ_B_GAMES):
                decks, chairs = _focal(j, 2 * s, 2 * s + 1, s)
                games.append(_game(4 * s + j, 4 * s + j, "read_b", None, decks, chairs,
                                   deal.seed([deal.READ_B_KEY, namespace, RUN, x, j]), j))
        elif layout == "c":
            for j in range(deal_common.COMMON_GAMES):
                g_seed = deal_common.common_game_seed(namespace, RUN, unit, s, 0, j)
                for arm in (0, 1):
                    decks, chairs = _focal(j, 3 * s + arm, 3 * s + 2, s)
                    games.append(_game(8 * s + 2 * j + arm, 4 * s + j, "common", None, decks, chairs, g_seed, j))
            pairs.append({"kind": "current_initial" if family == "dose" else "candidate_stated", "chair": s,
                          "deck_x": 3 * s, "deck_y": 3 * s + 1, "opponents": [3 * s + 2],
                          "games_x": [8 * s + 2 * j for j in range(4)],
                          "games_y": [8 * s + 2 * j + 1 for j in range(4)], "q": None})
        else:
            for q, kind, x_row, y_row in D13_CONTRASTS:
                gx, gy = [], []
                for o in (0, 1):
                    for j in (0, 1):
                        group = 12 * s + 4 * q + 2 * o + j
                        g_seed = deal_common.common_game_seed(namespace, RUN, unit, s, o, j, q)
                        for arm, row in enumerate((x_row, y_row)):
                            decks, chairs = _focal(j, 6 * s + D13_ROW_DECK[row], 6 * s + 4 + o, s)
                            games.append(_game(2 * group + arm, group, "common", None, decks, chairs, g_seed, j))
                            (gx if arm == 0 else gy).append(2 * group + arm)
                pairs.append({"kind": kind, "chair": s, "deck_x": 6 * s + D13_ROW_DECK[x_row],
                              "deck_y": 6 * s + D13_ROW_DECK[y_row], "opponents": [6 * s + 4, 6 * s + 5],
                              "games_x": gx, "games_y": gy, "q": q})
    return games, pairs


def plan_decks(family: str, builders: dict[str, dict[str, Any]], built_cards: dict[tuple[int, str], list[int]] | None,
               dealt: dict[int, dict[str, Any]]) -> list[dict[str, Any]]:
    layout = _layout(family)
    rows = _rows(family)
    decks: list[dict[str, Any]] = []
    for s in range(SEATS):
        def cards(row: str, s: int = s) -> list[int] | None:
            return None if built_cards is None else built_cards[(s, row)]

        if layout == "a":
            decks += [_built(2 * s, s, "candidate", builders["candidate"], cards("candidate")),
                      _built(2 * s + 1, s, "stated", builders["stated"], cards("stated"))]
        elif layout in ("one", "all"):
            decks.append(_built(s, s, "candidate", builders["candidate"], cards("candidate")))
        elif layout == "b":
            decks += [_built(2 * s, s, "candidate", builders["candidate"], cards("candidate")), dealt[2 * s + 1]]
        elif layout == "c":
            decks += [_built(3 * s, s, rows[0], builders[rows[0]], cards(rows[0])),
                      _built(3 * s + 1, s, rows[1], builders[rows[1]], cards(rows[1])), dealt[3 * s + 2]]
        else:
            decks += [_built(6 * s + D13_ROW_DECK[r], s, r, builders[r], cards(r)) for r in ROWS["d13"]]
            decks += [dealt[6 * s + 4], dealt[6 * s + 5]]
    return decks


# ---------------------------------------------------------------------------------------------
# Section 4: prep requests and results

PREP_ROWS = {"b_c": ("candidate", "stated"), "size_b": ("candidate",), "size_c": ("candidate", "stated"),
             "dose": ("current", "initial"), "dose_panel": (), "d13": ROWS["d13"]}
PREP_PLAN_FAMILY = {"b_c": "c", "size_b": "size_b", "size_c": "size_c", "dose": "dose", "dose_panel": "dose",
                    "d13": "d13"}


def build_prep_request(prep_family: str, namespace: str, unit: int, pins: Any, picker: Any,
                       builders: dict[str, Any]) -> dict[str, Any]:
    """A prep request (section 4). `namespace` is the draft seed source: read-gate-pools for b_c,
    the family's own namespace otherwise, `test` in fixtures. prep_family `dose` is a
    prep-dose-<ordinal> request (current and initial builds); `dose_panel` the stage-1 panel request
    (no builds), written with family `dose`; `d13` the D13 stage-1 request (four builds)."""
    if prep_family not in PREP_ROWS:
        raise PlanError(f"unknown prep family {prep_family!r}")
    rows = PREP_ROWS[prep_family]
    plan_family = PREP_PLAN_FAMILY[prep_family]
    if prep_family == "b_c":
        if namespace not in (POOL_SEED_NAMESPACE, TEST):
            raise PlanError(f"the b_c prep request draws on {POOL_SEED_NAMESPACE!r}")
        seeds_ns = TEST if namespace == TEST else "read-gate-c"
    else:
        _check_namespace(plan_family, namespace)
        seeds_ns = namespace
    pins = _check_pins(pins)
    pick = None if plan_family == "dose" else _check_policy(picker, "picker", ("uniform", "learned"))
    builds = []
    for c in range(SEATS):
        for row in rows:
            b = STATED if row == "stated" else _check_policy(builders[row], f"builders.{row}", ("uniform", "learned"))
            builds.append({"chair": c, "build": BUILD_INDEX[row], "builder": dict(b), "row": row})
    return {
        "schema": PLAN_SCHEMA,
        "kind": "prep_request",
        "family": "dose" if prep_family == "dose_panel" else prep_family,
        "namespace": namespace,
        "run": RUN,
        "unit": unit,
        "seeds": plan_seeds(plan_family, seeds_ns, unit),
        "pins": pins,
        "chairs": plan_chairs(plan_family, None, None, pick),
        "builds": builds,
    }


def prep_request_bindings(request: dict[str, Any]) -> dict[str, Any]:
    """The declared picker and builder bindings carried by a prep request."""
    learning = [c["picker"] for c in request["chairs"] if c["role"] == "learning"]
    if request["family"] == "dose":
        picker = None
    elif not learning or any(p != learning[0] for p in learning):
        raise PlanError("every learning chair of a prep request uses one picker")
    else:
        picker = learning[0]
    builders: dict[str, Any] = {}
    for build in request["builds"]:
        if build["row"] != "stated":
            if builders.setdefault(build["row"], build["builder"]) != build["builder"]:
                raise PlanError(f"prep request row {build['row']} has two builders")
    return {"pins": request["pins"], "picker": picker, "builders": builders}


def check_prep_request(request: Any, *, expected_pins: Any = None, expected_picker: Any = None,
                       expected_builders: dict[str, Any] | None = None) -> dict[str, Any]:
    obj = pod_record._object
    request = obj(request, "prep_request", PREP_REQUEST_KEYS)
    if request["schema"] != PLAN_SCHEMA or request["kind"] != "prep_request":
        raise PlanError("prep request has the read-plan schema and kind 'prep_request'")
    for i, chair in enumerate(pod_record._list(request["chairs"], "prep_request.chairs")):
        obj(chair, f"prep_request.chairs[{i}]", PLAN_CHAIR_KEYS)
        obj(chair["picker"], f"prep_request.chairs[{i}].picker", ("kind", "pin"))
    for i, build in enumerate(pod_record._list(request["builds"], "prep_request.builds")):
        obj(build, f"prep_request.builds[{i}]", PREP_BUILD_KEYS)
        obj(build["builder"], f"prep_request.builds[{i}].builder", ("kind", "pin"))
    bindings = prep_request_bindings(request)
    if expected_pins is not None and request["pins"] != expected_pins:
        raise PlanError("prep request pins differ from declared pins")
    if expected_picker is not None and bindings["picker"] != expected_picker:
        raise PlanError("prep request picker differs from declared picker")
    if expected_builders is not None and bindings["builders"] != expected_builders:
        raise PlanError("prep request builders differ from declared builders")
    prep_family = "dose_panel" if request["family"] == "dose" and not request["builds"] else request["family"]
    try:
        expected = build_prep_request(prep_family, request["namespace"], request["unit"], request["pins"],
                                      bindings["picker"], bindings["builders"])
    except (ValueError, KeyError, TypeError) as exc:
        raise PlanError(f"prep request cannot be regenerated: {exc}") from exc
    if serialize(expected) != serialize(request):
        raise PlanError(f"prep request differs from its regeneration at {first_diff(request, expected, 'prep_request')}")
    return request


def check_prep_result(result: Any, request: dict[str, Any], request_sha: str) -> dict[str, Any]:
    """A prep result answers its request: same builds in order, each with build_seed and 40 cards,
    and a draft record on the request's draft seed."""
    obj = pod_record._object
    result = obj(result, "prep_result", PREP_RESULT_KEYS)
    if result["schema"] != PLAN_SCHEMA or result["kind"] != "prep_result":
        raise PlanError("prep result has the read-plan schema and kind 'prep_result'")
    if result["request_sha256"] != request_sha:
        raise PlanError("prep result answers a different request")
    if result["draft"] is None:
        raise PlanError("prep result carries its draft record")
    pod_record._validate_draft(result["draft"], "prep_result.draft", request["seeds"]["draft_seed"])
    builds = pod_record._list(result["builds"], "prep_result.builds")
    if len(builds) != len(request["builds"]):
        raise PlanError("prep result has one build per requested build")
    draft_seed = request["seeds"]["draft_seed"]
    for i, (got, want) in enumerate(zip(builds, request["builds"])):
        obj(got, f"prep_result.builds[{i}]", PREP_RESULT_BUILD_KEYS)
        if {k: got[k] for k in PREP_BUILD_KEYS} != want:
            raise PlanError(f"prep result build {i} differs from its request")
        b_seed = pod_record._uint_or_null(got["build_seed"], f"prep_result.builds[{i}].build_seed", pod_record.U64)
        expected_seed = seeds_v1.build_seed(draft_seed, want["chair"], want["build"])
        if want["builder"]["kind"] in ("uniform", "learned"):
            if b_seed != expected_seed:
                raise PlanError(
                    f"prep result build {i} build_seed {b_seed!r} differs from derived stream {expected_seed}"
                )
        elif b_seed is not None and b_seed != expected_seed:
            raise PlanError(
                f"prep result stated build {i} build_seed {b_seed!r} differs from derived stream {expected_seed}"
            )
        cards = pod_record._uint_list(got["cards"], f"prep_result.builds[{i}].cards", pod_record.U16)
        if len(cards) != pod_record.DECK_CARDS or cards != sorted(cards):
            raise PlanError(f"prep result build {i} holds 40 cards in canonical order")
    return result


def prep_cards(result: dict[str, Any]) -> dict[tuple[int, str], list[int]]:
    return {(b["chair"], b["row"]): list(b["cards"]) for b in result["builds"]}


# ---------------------------------------------------------------------------------------------
# The plan


def build_plan(family: str, namespace: str, unit: int, *, pins: Any, pilot: Any, picker: Any = None,
               builders: dict[str, Any] | None = None, mirror: int | None = None, rotation: int | None = None,
               mark: Any = None, prep: tuple[dict[str, Any], str] | None = None, pool_doc: Any = None,
               constants_doc: Any = None, pool_sha256: str | None = None,
               constants_sha256: str | None = None, card_index: tuple[Any, str] | None = None,
               cube_table: tuple[Any, str] | None = None) -> dict[str, Any]:
    """One read plan (sections 2 and 3). `picker` binds the learning chairs; `builders` binds each
    builder row except `stated`. Families with prep take (prep result, its sha256); families with
    dealt decks take the pool and constants documents and their sha256 values."""
    spec = FAMILIES.get(family)
    if spec is None:
        raise PlanError(f"unknown family {family!r}")
    layout = spec["layout"]
    _check_namespace(family, namespace)
    if layout == "b" and namespace != TEST:
        verify_a15_golden()
    _u64(unit, "unit")
    if layout == "a":
        if mirror not in (0, 1) or rotation is not None:
            raise PlanError("an (a) layout plan names mirror 0 or 1 and no rotation")
    elif layout == "one":
        if not (type(rotation) is int and 0 <= rotation < SEATS) or mirror is not None:
            raise PlanError("a one-seat companion names rotation 0 to 7 and no mirror")
    elif mirror is not None or rotation is not None:
        raise PlanError(f"family {family} has neither mirror nor rotation")
    mark = _check_mark(family, mark)
    pins = _check_pins(pins)
    pilot = _check_pilot(pilot)
    if family == "dose":
        if picker is not None:
            raise PlanError("dose chairs are fixture chairs with the uniform picker; pass no picker")
        pick = None
    else:
        pick = _check_policy(picker, "picker", ("uniform",) if family == "aa" else ("uniform", "learned"))
    builders = dict(builders or {})
    bound: dict[str, dict[str, Any]] = {}
    for row in _rows(family):
        if row == "stated":
            if builders.pop("stated", STATED) != STATED:
                raise PlanError("the stated row uses the stated builder")
            bound[row] = dict(STATED)
        else:
            if row not in builders:
                raise PlanError(f"family {family} needs a builder for row {row!r}")
            kinds = ("uniform",) if family == "aa" else ("uniform", "learned")
            bound[row] = _check_policy(builders.pop(row), f"builders.{row}", kinds)
    if builders:
        raise PlanError(f"family {family} has no rows {sorted(builders)}")

    seeds = plan_seeds(family, namespace, unit, rotation)
    pools = built_cards = prep_sha = None
    if spec.get("prep"):
        if prep is None:
            raise PlanError(f"family {family} is planned from a prep result")
        result, prep_sha = prep
        if not _is_sha(prep_sha):
            raise PlanError("prep sha256 must be a sha256 string")
        if result["draft"]["draft_seed"] != seeds["draft_seed"]:
            raise PlanError("the prep result's draft is not this unit's draft")
        pools = [list(p) for p in result["draft"]["pools"]]
        built_cards = prep_cards(result)
        for s in range(SEATS):
            for row in _rows(family):
                if (s, row) not in built_cards:
                    raise PlanError(f"the prep result lacks chair {s}'s {row} build")
    elif prep is not None:
        raise PlanError(f"family {family} has no prep")

    pool_state = constants = None
    inputs = {"pool_sha256": None, "constants_sha256": None}
    if layout in ("b", "c", "d13"):
        if not (_is_sha(pool_sha256) and _is_sha(constants_sha256)):
            raise PlanError(f"family {family} deals decks: pool and constants sha256 are required")
        pool_state = deal.validate_pool(pool_doc)
        constants = deal.validate_constants(constants_doc)
        inputs = {"pool_sha256": pool_sha256, "constants_sha256": constants_sha256}
    elif pool_doc is not None or constants_doc is not None:
        raise PlanError(f"family {family} deals no deck")

    card_index_doc = cube_table_doc = None
    if card_index is not None:
        card_index_doc, card_index_sha = card_index
        if card_index_sha != pins["card_index"]:
            raise PlanError("card_index does not hash to pins.card_index")
    if cube_table is not None:
        cube_table_doc, cube_table_sha = cube_table
        if cube_table_sha != pins["cube_table"]:
            raise PlanError("cube_table does not hash to pins.cube_table")

    dealt = dealt_decks(family, namespace, unit, pools, built_cards, pool_state, constants,
                        card_index=card_index_doc, cube_table=cube_table_doc)
    if layout in ("a", "one", "all"):
        games, pairs = pairing_games(family, seeds["pod_seed"], rotation), []
    else:
        games, pairs = dealer_games(family, namespace, unit)
    return {
        "schema": PLAN_SCHEMA,
        "interfaces": INTERFACES,
        "family": family,
        "use": TEST if namespace == TEST else spec["use"],
        "namespace": namespace,
        "run": RUN,
        "unit": unit,
        "pod_index": pod_index(family, unit, mirror, rotation),
        "mirror": mirror,
        "mirror_pods": [2 * unit, 2 * unit + 1] if layout == "a" else None,
        "rotation": rotation,
        "mark": mark,
        "seeds": seeds,
        "pins": pins,
        "pilot": pilot,
        "caps": dict(CAPS),
        "seat_rule": "schedule",
        "inputs": inputs,
        "prep_sha256": prep_sha,
        "chairs": plan_chairs(family, mirror, rotation, pick),
        "pools": pools,
        "decks": plan_decks(family, bound, built_cards, dealt),
        "games": games,
        "paired_builds": pairs,
    }


def first_diff(a: Any, b: Any, path: str = "plan") -> str | None:
    if type(a) is not type(b):
        return path
    if isinstance(a, dict):
        if list(a) != list(b):
            return f"{path} (keys)"
        for key in a:
            found = first_diff(a[key], b[key], f"{path}.{key}")
            if found:
                return found
        return None
    if isinstance(a, list):
        if len(a) != len(b):
            return f"{path} (length)"
        for i, (x, y) in enumerate(zip(a, b)):
            found = first_diff(x, y, f"{path}[{i}]")
            if found:
                return found
        return None
    return None if a == b else path


def _shape(plan: Any) -> dict[str, Any]:
    """Strict key order at every level and the identity types, before regeneration."""
    obj = pod_record._object
    plan = obj(plan, "plan", PLAN_KEYS)
    if plan["schema"] != PLAN_SCHEMA:
        raise PlanError(f"plan schema {plan['schema']!r} is unknown")
    if plan["interfaces"] != INTERFACES:
        raise PlanError(f"plan interfaces {plan['interfaces']!r} is unknown")
    if plan["family"] not in FAMILIES:
        raise PlanError(f"plan family {plan['family']!r} is unknown")
    obj(plan["seeds"], "plan.seeds", pod_record.READ_SEEDS_KEYS)
    obj(plan["pins"], "plan.pins", pod_record.PINS_KEYS)
    obj(plan["pilot"], "plan.pilot", pod_record.PILOT_KEYS)
    caps = obj(plan["caps"], "plan.caps", pod_record.CAPS_KEYS)
    pod_record._uint(caps["max_stack_objects"], "plan.caps.max_stack_objects", pod_record.U32)
    if caps["max_stack_objects"] != pod_record.MAX_STACK_OBJECTS:
        raise PlanError(
            f"plan.caps.max_stack_objects must be {pod_record.MAX_STACK_OBJECTS}, got {caps['max_stack_objects']}"
        )
    obj(plan["inputs"], "plan.inputs", INPUT_KEYS)
    if plan["mark"] is not None:
        obj(plan["mark"], "plan.mark", MARK_KEYS)
    for i, chair in enumerate(pod_record._list(plan["chairs"], "plan.chairs")):
        obj(chair, f"plan.chairs[{i}]", PLAN_CHAIR_KEYS)
        obj(chair["picker"], f"plan.chairs[{i}].picker", ("kind", "pin"))
    for i, deck in enumerate(pod_record._list(plan["decks"], "plan.decks")):
        obj(deck, f"plan.decks[{i}]", PLAN_DECK_KEYS)
        obj(deck["builder"], f"plan.decks[{i}].builder", ("kind", "pin"))
        if deck["row"] not in DECK_ROWS:
            raise PlanError(f"plan.decks[{i}].row {deck['row']!r} is unknown")
    for i, game in enumerate(pod_record._list(plan["games"], "plan.games")):
        obj(game, f"plan.games[{i}]", PLAN_GAME_KEYS)
    for i, pair in enumerate(pod_record._list(plan["paired_builds"], "plan.paired_builds")):
        obj(pair, f"plan.paired_builds[{i}]", pod_record.PAIRED_KEYS)
    for key in ("unit", "pod_index"):
        _u64(plan[key], f"plan.{key}")
    for key in ("mirror", "rotation"):
        if plan[key] is not None and not (type(plan[key]) is int and 0 <= plan[key] <= pod_record.U8):
            raise PlanError(f"plan.{key} is a u8 or null")
    return plan


def plan_bindings(plan: dict[str, Any]) -> dict[str, Any]:
    """The bindings a plan declares: the learning chairs' picker and the builder of each row."""
    learning = [c["picker"] for c in plan["chairs"] if c["role"] == "learning"]
    if plan["family"] == "dose":
        picker = None
    elif not learning or any(p != learning[0] for p in learning):
        raise PlanError("every learning chair of a plan uses one picker")
    else:
        picker = learning[0]
    builders: dict[str, Any] = {}
    for deck in plan["decks"]:
        if deck["source"] == "build" and deck["row"] != "stated":
            if builders.setdefault(deck["row"], deck["builder"]) != deck["builder"]:
                raise PlanError(f"row {deck['row']} has two builders")
    return {"picker": picker, "builders": builders}


def check_plan(plan: Any, prep: tuple[dict[str, Any], str] | None = None, pool_doc: Any = None,
               constants_doc: Any = None, pool_sha256: str | None = None,
               constants_sha256: str | None = None, card_index: tuple[Any, str] | None = None,
               cube_table: tuple[Any, str] | None = None, *, expected_pins: Any = None,
               expected_pilot: Any = None, expected_picker: Any = None,
               expected_builders: dict[str, Any] | None = None) -> dict[str, Any]:
    """Strict validation: the plan must equal, byte for byte, the plan regenerated from its identity
    fields, bindings, prep result and dealer inputs (whose hashes must equal the plan's)."""
    plan = _shape(plan)
    family = plan["family"]
    if FAMILIES[family].get("prep") and (prep is None or prep[1] != plan["prep_sha256"]):
        raise PlanError("the plan's prep_sha256 is not the given prep result's")
    if plan["inputs"]["pool_sha256"] is not None and (pool_sha256, constants_sha256) != (
            plan["inputs"]["pool_sha256"], plan["inputs"]["constants_sha256"]):
        raise PlanError("the dealer inputs do not hash to the plan's inputs")
    bindings = plan_bindings(plan)
    if expected_pins is not None and plan["pins"] != expected_pins:
        raise PlanError("plan pins differ from declared pins")
    if expected_pilot is not None and plan["pilot"] != expected_pilot:
        raise PlanError("plan pilot differs from declared pilot")
    if expected_picker is not None and bindings["picker"] != expected_picker:
        raise PlanError("plan picker differs from declared picker")
    if expected_builders is not None and bindings["builders"] != expected_builders:
        raise PlanError("plan builders differ from declared builders")
    try:
        expected = build_plan(family, plan["namespace"], plan["unit"], pins=plan["pins"], pilot=plan["pilot"],
                              picker=bindings["picker"], builders=bindings["builders"], mirror=plan["mirror"],
                              rotation=plan["rotation"], mark=plan["mark"], prep=prep, pool_doc=pool_doc,
                              constants_doc=constants_doc, pool_sha256=pool_sha256, constants_sha256=constants_sha256,
                              card_index=card_index, cube_table=cube_table)
    except (ValueError, KeyError, TypeError) as exc:
        raise PlanError(f"plan cannot be regenerated: {exc}") from exc
    if serialize(expected) != serialize(plan):
        raise PlanError(f"plan differs from its regeneration at {first_diff(plan, expected)}")
    return plan


def check_mirrors(even: dict[str, Any], odd: dict[str, Any]) -> None:
    """Mirror plans are equal except for mirror, pod_index, chair roles and pickers (section 7)."""
    if even["mirror"] != 0 or odd["mirror"] != 1 or even["unit"] != odd["unit"]:
        raise PlanError("a pair is mirror 0 then mirror 1 of one unit")
    a, b = json.loads(json.dumps(even)), json.loads(json.dumps(odd))
    for doc in (a, b):
        doc["mirror"] = doc["pod_index"] = None
        for chair in doc["chairs"]:
            chair["role"] = chair["picker"] = None
    if a != b:
        raise PlanError(f"mirror plans differ at {first_diff(a, b)}")


# ---------------------------------------------------------------------------------------------
# Section 5: manifests and the frozen panel


SECTION5_SET = r"(look-1|look-2|fixed|dose-\d+|pilot-\d+|prep-\d+-\d+|prep-dose-\d+|prep-panel|panel)(-restart-[1-9]\d*)?"


def fixture_cases(family: str) -> tuple[str, ...]:
    if family == "b_c":
        return FIXTURE_CASES["b"] + FIXTURE_CASES["c"] + EDGE_CASES
    return FIXTURE_CASES.get(family, ()) + EDGE_CASES


def check_set_name(namespace: str, family: str, set_name: str) -> None:
    """Set names (section 5): look-1, look-2, fixed, dose-<n>, pilot-<n>, prep-<first>-<end>,
    prep-dose-<n>, prep-panel, panel, each with an optional -restart-<k>. In `test` a set is
    fx-<family>-<case>-<binding> with an A6.20 case and binding d0 or d15; a fixture's own prep
    and panel sets append one section 5 name (for example fx-dose-provisional-d0-prep-panel)."""
    if namespace != TEST:
        if not re.fullmatch(SECTION5_SET, set_name):
            raise PlanError(f"set name {set_name!r} is not a section 5 name")
        return
    prefix = f"fx-{family}-"
    if set_name.startswith(prefix):
        rest = set_name[len(prefix):]
        for case in fixture_cases(family):
            for binding in ("d0", "d15"):
                head = f"{case}-{binding}"
                if rest == head or (rest.startswith(head + "-") and re.fullmatch(SECTION5_SET, rest[len(head) + 1:])):
                    return
    raise PlanError(f"fixture set {set_name!r} is not fx-{family}-<case>-<binding> with an A6.20 case")


def fixture_base(set_name: str) -> str:
    """The fixture name a test set's auxiliary prep or panel set hangs from."""
    m = re.match(r"(fx-.+?-(?:d0|d15))(?:-|$)", set_name)
    return m.group(1) if m else set_name


def build_manifest(kind: str, family: str, namespace: str, set_name: str, declaration_sha256: str,
                   entries: list[tuple[int, str]], *, look: int | None = None, units: list[int],
                   reserved: list[int] | None = None, pools_with: str | None = None,
                   panel_sha256: str | None = None, prep_set: str | None = None, replaces: str | None = None,
                   retired: list[list[int]] | None = None) -> dict[str, Any]:
    """A manifest (section 5). `entries` are (pod_index or unit, file sha256); files are named
    plans/pod-<pod_index>.json or requests/unit-<unit>.json."""
    if kind not in ("plans", "prep"):
        raise PlanError("manifest kind is plans or prep")
    check_set_name(namespace, family, set_name)
    index_key = "pod_index" if kind == "plans" else "unit"
    folder, stem = ("plans", "pod") if kind == "plans" else ("requests", "unit")
    plans = [{index_key: i, "file": f"{folder}/{stem}-{i}.json", "sha256": sha} for i, sha in entries]
    return {
        "schema": PLAN_SCHEMA, "kind": kind, "family": family, "namespace": namespace, "run": RUN,
        "set": set_name, "interfaces": INTERFACES, "declaration_sha256": declaration_sha256, "look": look,
        "units": list(units), "reserved": None if reserved is None else list(reserved), "pools_with": pools_with,
        "panel_sha256": panel_sha256, "prep_set": prep_set, "replaces": replaces,
        "retired": [list(r) for r in (retired or [])], "plans": plans, "plan_set_sha256": plan_set_sha256(plans),
        "executions": [],
    }


def write_set(root: Path, manifest: dict[str, Any], files: dict[str, Any]) -> Path:
    """Write a set directory <root>/<namespace>/<run>/<set>/ with its manifest and files."""
    set_dir = Path(root) / manifest["namespace"] / manifest["run"] / manifest["set"]
    for rel, doc in files.items():
        path = set_dir / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(serialize(doc).encode("utf-8"))
    (set_dir / "manifest.json").write_bytes(serialize(manifest).encode("utf-8"))
    return set_dir


def assemble_panel(family: str, namespace: str, prep_manifest: dict[str, Any], prep_execution: int,
                   outputs: dict[int, tuple[dict[str, Any], str]], *, pins: Any, builders: dict[str, Any] | None,
                   pool_doc: Any, constants_doc: Any, pool_sha256: str, constants_sha256: str,
                   card_index: tuple[Any, str] | None = None, cube_table: tuple[Any, str] | None = None
                   ) -> dict[str, Any]:
    """Stage 2 of a frozen panel (section 5): panel.json from the accepted stage-1 outputs alone.
    For each unit: seeds, pools, the dealt opponents and games of the family's plans, and for d13
    the built decks (`builders` binds their rows)."""
    if family not in ("dose", "d13"):
        raise PlanError("panels exist for dose and d13")
    pins = _check_pins(pins)
    card_index_doc = cube_table_doc = None
    if card_index is not None:
        card_index_doc, card_index_sha = card_index
        if card_index_sha != pins["card_index"]:
            raise PlanError("card_index does not hash to pins.card_index")
    if cube_table is not None:
        cube_table_doc, cube_table_sha = cube_table
        if cube_table_sha != pins["cube_table"]:
            raise PlanError("cube_table does not hash to pins.cube_table")
    pool_state = deal.validate_pool(pool_doc)
    constants = deal.validate_constants(constants_doc)
    first, end = prep_manifest["units"]
    drafts = []
    for unit in range(first, end):
        result, _ = outputs[unit]
        pools = [list(p) for p in result["draft"]["pools"]]
        cards = prep_cards(result) if family == "d13" else None
        dealt = dealt_decks(family, namespace, unit, pools, cards, pool_state, constants,
                            card_index=card_index_doc, cube_table=cube_table_doc)
        games, _ = dealer_games(family, namespace, unit)
        built = None
        if family == "d13":
            bound = {r: (dict(STATED) if r == "stated" else _check_policy(builders[r], r, ("uniform", "learned")))
                     for r in ROWS["d13"]}
            built = [d for d in plan_decks(family, bound, cards, dealt) if d["source"] == "build"]
        drafts.append({"unit": unit, "seeds": plan_seeds(family, namespace, unit), "pools": pools,
                       "opponents": [dealt[k] for k in sorted(dealt)], "games": games, "built": built})
    return {
        "schema": PLAN_SCHEMA, "kind": "panel", "namespace": namespace, "run": RUN, "units": [first, end],
        "inputs": {"pool_sha256": pool_sha256, "constants_sha256": constants_sha256}, "pins": _check_pins(pins),
        "prep_set_sha256": prep_manifest["plan_set_sha256"], "prep_execution": prep_execution,
        "prep_outputs": [{"unit": u, "sha256": outputs[u][1]} for u in range(first, end)], "drafts": drafts,
    }


def check_plan_against_panel(plan: dict[str, Any], panel: dict[str, Any]) -> None:
    """Each dose or d13 plan's seeds, pools, dealt decks and games equal its unit's panel entry, and
    for d13 so do its built decks."""
    entries = [d for d in panel["drafts"] if d["unit"] == plan["unit"]]
    if len(entries) != 1:
        raise PlanError(f"unit {plan['unit']} is not in the panel")
    entry = entries[0]
    dealt = [d for d in plan["decks"] if d["source"] == "dealt"]
    built = [d for d in plan["decks"] if d["source"] == "build"]
    if (plan["seeds"], plan["pools"], dealt, plan["games"]) != (entry["seeds"], entry["pools"], entry["opponents"],
                                                                entry["games"]):
        raise PlanError(f"plan of unit {plan['unit']} differs from its panel entry")
    if plan["family"] == "d13" and built != entry["built"]:
        raise PlanError(f"d13 plan of unit {plan['unit']} differs from the panel's built decks")


# ---------------------------------------------------------------------------------------------
# A record against its plan (section 7)


def check_record_against_plan(record: dict[str, Any], plan: dict[str, Any]) -> None:
    """A validated record played exactly its plan (read plan section 7): identity, seeds, pins,
    pilot, caps, draft, chairs, decks (except `row`; cards when planned), games, one attempt per
    game on seeds derived from G, paired builds; a stopped pod holds a prefix of the plan's games."""
    where = f"record {record['namespace']}/{record['run']}/{record['pod_index']}"

    def fail(message: str) -> None:
        raise pod_record.RecordError(f"{where}: {message}")

    for key in ("use", "namespace", "run", "pod_index", "seeds", "pins", "pilot", "caps"):
        if record[key] != plan[key]:
            fail(f"{key} differs from the plan")
    if record["draft"] is None or record["draft"]["draft_seed"] != plan["seeds"]["draft_seed"]:
        fail("draft.draft_seed differs from the plan")
    if plan["pools"] is not None and record["draft"]["pools"] != plan["pools"]:
        fail("draft.pools differ from the plan")
    for rec_chair, plan_chair in zip(record["chairs"], plan["chairs"]):
        if {k: rec_chair[k] for k in PLAN_CHAIR_KEYS} != plan_chair:
            fail(f"chair {plan_chair['chair']} differs from the plan")
    if len(record["decks"]) != len(plan["decks"]):
        fail(f"{len(record['decks'])} decks, the plan has {len(plan['decks'])}")
    for rec_deck, plan_deck in zip(record["decks"], plan["decks"]):
        for key in PLAN_DECK_KEYS:
            if key == "row" or (key == "cards" and plan_deck["cards"] is None):
                continue
            if rec_deck[key] != plan_deck[key]:
                fail(f"deck {plan_deck['deck']} {key} differs from the plan")
        if plan_deck["source"] == "build":
            expected_seed = seeds_v1.build_seed(plan["seeds"]["draft_seed"], plan_deck["chair"], plan_deck["build"])
            if plan_deck["builder"]["kind"] in ("uniform", "learned"):
                if rec_deck["build_seed"] != expected_seed:
                    fail(f"deck {plan_deck['deck']} build_seed differs from derived stream")
            elif rec_deck["build_seed"] is not None and rec_deck["build_seed"] != expected_seed:
                fail(f"deck {plan_deck['deck']} stated build_seed differs from derived stream")
    games = record["games"]
    if len(games) > len(plan["games"]):
        fail("more games than the plan")
    if record["status"] == "complete" and len(games) != len(plan["games"]):
        fail(f"{len(games)} games, the plan has {len(plan['games'])}")
    for rec_game, plan_game in zip(games, plan["games"]):
        for key in PLAN_GAME_KEYS:
            if rec_game[key] != plan_game[key]:
                fail(f"game {plan_game['game']} {key} differs from the plan")
        if len(rec_game["attempts"]) != 1:
            fail(f"game {plan_game['game']} has a replay attempt")
        attempt = rec_game["attempts"][0]
        want = seeds_v1.attempt_seeds(plan_game["game_seed"])
        if {k: attempt[k] for k in want} != want:
            fail(f"game {plan_game['game']}'s attempt seeds are not derived from its game seed")
    if record["paired_builds"] != plan["paired_builds"]:
        fail("paired_builds differ from the plan")
    if record["status"] == "stopped":
        stop = record["stop"]
        plan_games = plan["games"]
        if not plan_games:
            fail("stopped pod has no planned games")
        stop_game = stop["game"]
        played_count = len(games)
        on_last_played = played_count > 0 and stop_game == games[-1]["game"]
        next_idx = played_count if played_count < len(plan_games) else len(plan_games) - 1
        on_next_unplayed = stop_game == plan_games[next_idx]["game"]
        if not (on_last_played or on_next_unplayed):
            fail(f"stop.game {stop_game} does not match the played prefix length {played_count}")
        pg = plan_games[stop_game]
        if stop["pairing"] != pg["pairing"] or stop["decks"] != pg["decks"]:
            fail(f"stop report metadata for game {stop_game} differs from the plan")
        if not stop["attempts"]:
            fail("stop.attempts must not be empty")
        if on_last_played and games[-1]["attempts"][-1]["halted"]:
            want_attempts = [
                {"deal_seed": a["deal_seed"], "stop_kind": a["stop_kind"],
                 "last_action_source": a["last_action_source"]}
                for a in games[-1]["attempts"]
            ]
            if stop["attempts"] != want_attempts:
                fail("stop.attempts differs from the halted last played game")
        else:
            if not on_next_unplayed:
                fail("stop on a completed last played game must name the interrupted next game")
            if stop["attempts"][0]["deal_seed"] != pg["game_seed"] or not stop["attempts"][0]["stop_kind"]:
                fail("interrupted stop report attempt does not match the planned game seed and stop kind")


# ---------------------------------------------------------------------------------------------
# Loading and the command line


def load_json(path: str | Path) -> tuple[Any, str]:
    data = Path(path).read_bytes()
    return json.loads(data.decode("utf-8")), hashlib.sha256(data).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Write a read plan, prep request or manifest (read plan v1).")
    sub = parser.add_subparsers(dest="command", required=True)
    plan_p = sub.add_parser("plan", help="one plan of any family, from a JSON spec")
    plan_p.add_argument("--spec", required=True, type=Path,
                        help="JSON: family, namespace, unit, pins, pilot, picker, builders, mirror, rotation, mark")
    plan_p.add_argument("--prep", type=Path, help="the prep result (families with prep)")
    plan_p.add_argument("--pool", type=Path)
    plan_p.add_argument("--constants", type=Path)
    plan_p.add_argument("--out", type=Path)
    req_p = sub.add_parser("prep-request", help="one prep request, from a JSON spec")
    req_p.add_argument("--spec", required=True, type=Path,
                       help="JSON: prep_family, namespace, unit, pins, picker, builders")
    req_p.add_argument("--out", type=Path)
    man_p = sub.add_parser("manifest", help="a manifest over the plan or request files of a set directory")
    man_p.add_argument("--spec", required=True, type=Path,
                       help="JSON: kind, family, namespace, set, declaration_sha256, look, units, reserved, "
                            "pools_with, panel_sha256, prep_set, replaces, retired")
    man_p.add_argument("--set-dir", required=True, type=Path)
    args = parser.parse_args(argv)

    spec, _ = load_json(args.spec)
    if args.command == "plan":
        prep = load_json(args.prep) if args.prep else None
        pool = load_json(args.pool) if args.pool else (None, None)
        constants = load_json(args.constants) if args.constants else (None, None)
        doc = build_plan(spec["family"], spec["namespace"], spec["unit"], pins=spec["pins"], pilot=spec["pilot"],
                         picker=spec.get("picker"), builders=spec.get("builders"), mirror=spec.get("mirror"),
                         rotation=spec.get("rotation"), mark=spec.get("mark"), prep=prep, pool_doc=pool[0],
                         constants_doc=constants[0], pool_sha256=pool[1], constants_sha256=constants[1])
    elif args.command == "prep-request":
        doc = build_prep_request(spec["prep_family"], spec["namespace"], spec["unit"], spec["pins"],
                                 spec.get("picker"), spec.get("builders", {}))
    else:
        folder, stem = ("plans", "pod") if spec["kind"] == "plans" else ("requests", "unit")
        paths = sorted((args.set_dir / folder).glob(f"{stem}-*.json"), key=lambda p: int(p.stem.split("-")[1]))
        entries = [(int(p.stem.split("-")[1]), hashlib.sha256(p.read_bytes()).hexdigest()) for p in paths]
        doc = build_manifest(spec["kind"], spec["family"], spec["namespace"], spec["set"],
                             spec["declaration_sha256"], entries, look=spec.get("look"), units=spec["units"],
                             reserved=spec.get("reserved"), pools_with=spec.get("pools_with"),
                             panel_sha256=spec.get("panel_sha256"), prep_set=spec.get("prep_set"),
                             replaces=spec.get("replaces"), retired=spec.get("retired"))
        (args.set_dir / "manifest.json").write_bytes(serialize(doc).encode("utf-8"))
        return 0
    text = serialize(doc)
    if args.out is None:
        sys.stdout.write(text)
    else:
        args.out.write_bytes(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
