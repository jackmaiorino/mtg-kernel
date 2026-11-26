"""Load and validate the cube pod record, `mtg-kernel-cube-pod-record/v2` (m4-interfaces/v2, section 4).

Every field is checked for presence, type and key order, exactly as the frozen interface lists it.
Unknown versions, missing or extra fields and inconsistent ids are rejected. Readers map the
scores +1, 0, -1 to win fractions 1, 0.5, 0. A game with no completed attempt is halted.

Standard library only. Integers are parsed exactly by `json` (u64 values keep every bit).
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402
import seeds_v1  # noqa: E402

POD_RECORD_SCHEMA = "mtg-kernel-cube-pod-record/v2"
INTERFACES_VERSION = "m4-interfaces/v2"
DRAFT_RECORD_SCHEMA = "cube_draft_record/v1"

SEATS = 8
DECK_CARDS = 40
POOL_CARDS = 45
MAX_ATTEMPTS = 4
MAX_STACK_OBJECTS = 128

U8 = (1 << 8) - 1
U16 = (1 << 16) - 1
U32 = (1 << 32) - 1
U64 = (1 << 64) - 1

NAME_RE = re.compile(r"^[a-z0-9-]+$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")

USES = ("training", "shakedown", "dose", "read_a", "read_b", "paired_build", "sizing", "test", "client")
# A read pod is any pod run from a read plan: these uses, plus a `test` pod built from a read plan
# (one whose seeds carry the two source fields of change 3).
READ_USES = ("dose", "read_a", "read_b", "paired_build", "sizing")
CLIENT_NAMESPACE = "client-pods"
CHAIR_ROLES = ("learning", "older", "fixture")
PICKER_KINDS = ("uniform", "learned", "stated")
# Field 11: builder kind uniform, learned or stated for a built deck, dealt for a dealt deck, whose
# pin is {function, key} copied from the deck's `dealt` (change 6).
BUILDER_KINDS = ("uniform", "learned", "stated")
DEALT_BUILDER_KIND = "dealt"
PILOT_KINDS = ("uniform", "checkpoint")
UNIFORM_DECODING = "uniform-index-splitmix64-v1"
DECK_SOURCES = ("build", "dealt")
GAME_ROLES = ("reference", "sibling", "cross", "read_b", "common")
SEAT_RULES = ("coin", "schedule")
PAIRED_KINDS = ("sibling", "candidate_stated", "current_initial", "k1_last_b1_last")
STATUSES = ("complete", "stopped")

ROW_KINDS = ("pick", "count", "pointer", "type_set", "split")
LEARNED_SAMPLER = "softmax-cdf-splitmix64-v2"
UNIFORM_SAMPLER = "uniform-index-splitmix64-v1"
SAMPLERS = (LEARNED_SAMPLER, UNIFORM_SAMPLER)

END_FIELDS = {
    "natural": ("kind", "winner_seat", "reason"),
    "terminal": ("kind", "code", "classification", "reason"),
    "session_error": ("kind", "code", "message"),
    "decision_cap": ("kind",),
    "panic": ("kind", "message"),
}

RECORD_KEYS = (
    "schema",
    "interfaces",
    "use",
    "namespace",
    "run",
    "pod_index",
    "seeds",
    "pins",
    "pilot",
    "caps",
    "draft",
    "chairs",
    "decks",
    "games",
    "paired_builds",
    "halts",
    "status",
    "stop",
)
SEEDS_KEYS = ("draft_seed", "pod_seed", "league_seed")
READ_SEEDS_KEYS = SEEDS_KEYS + ("draft_seed_namespace", "pod_seed_namespace")
PINS_KEYS = ("binary", "cube_list", "fence_record", "card_index", "cube_table")
PILOT_KEYS = ("kind", "pin", "decoding")
CAPS_KEYS = (
    "max_decisions",
    "max_physical_decisions",
    "max_policy_steps",
    "max_stack_objects",
    "watchdog_seconds",
    "max_replays",
)
DRAFT_KEYS = ("schema", "draft_seed", "packs", "picks", "pools")
CHAIR_KEYS = ("chair", "role", "picker", "pick_rows")
KIND_PIN_KEYS = ("kind", "pin")
DECK_KEYS = ("deck", "source", "chair", "build", "builder", "build_seed", "cards", "build_rows", "dealt")
DEALT_KEYS = ("function", "key")
BUILD_ROWS_KEYS = ("count", "pointer", "type_set", "split")
ROW_KEYS = ("kind", "sampler", "options", "chosen", "stream_values", "logits_bits", "p_chosen_bits", "value_bits")
GAME_KEYS = (
    "game",
    "group",
    "role",
    "pairing",
    "decks",
    "chairs",
    "seat_rule",
    "game_seed",
    "episode",
    "attempts",
    "scores",
)
ATTEMPT_KEYS = (
    "attempt",
    "deal_seed",
    "env_seed",
    "action_seed",
    "decisions",
    "physical_decisions",
    "policy_steps",
    "end",
    "halted",
    "stop_kind",
    "last_action_source",
)
PAIRED_KEYS = ("kind", "chair", "deck_x", "deck_y", "opponents", "games_x", "games_y", "q")
HALT_KEYS = ("game", "attempt", "stop_kind", "last_action_source", "deal_seed")
STOP_KEYS = ("game", "pairing", "decks", "attempts")
STOP_ATTEMPT_KEYS = ("deal_seed", "stop_kind", "last_action_source")

WIN_FRACTION = {1: 1.0, 0: 0.5, -1: 0.0}


class RecordError(ValueError):
    """A pod record that does not follow mtg-kernel-cube-pod-record/v1."""


def _fail(path: str, message: str) -> None:
    raise RecordError(f"{path}: {message}")


def _is_int(value: Any) -> bool:
    return type(value) is int


def _uint(value: Any, path: str, limit: int) -> int:
    if not (_is_int(value) and 0 <= value <= limit):
        _fail(path, f"must be an integer in [0, {limit}], got {value!r}")
    return value


def _uint_or_null(value: Any, path: str, limit: int) -> int | None:
    if value is None:
        return None
    return _uint(value, path, limit)


def _string(value: Any, path: str) -> str:
    if not isinstance(value, str):
        _fail(path, f"must be a string, got {value!r}")
    return value


def _enum(value: Any, path: str, allowed: tuple[str, ...]) -> str:
    if value not in allowed or not isinstance(value, str):
        _fail(path, f"must be one of {list(allowed)}, got {value!r}")
    return value


def _sha_or_null(value: Any, path: str) -> str | None:
    if value is None:
        return None
    if not (isinstance(value, str) and SHA256_RE.match(value)):
        _fail(path, f"must be a lowercase sha256 hex string or null, got {value!r}")
    return value


def _object(value: Any, path: str, keys: tuple[str, ...]) -> dict[str, Any]:
    if not isinstance(value, dict):
        _fail(path, f"must be an object, got {type(value).__name__}")
    actual = list(value.keys())
    if actual != list(keys):
        missing = [k for k in keys if k not in value]
        extra = [k for k in actual if k not in keys]
        if missing or extra:
            _fail(path, f"keys differ from the frozen list: missing {missing}, extra {extra}")
        _fail(path, f"keys out of the frozen order: {actual} (expected {list(keys)})")
    return value


def _list(value: Any, path: str) -> list[Any]:
    if not isinstance(value, list):
        _fail(path, f"must be an array, got {type(value).__name__}")
    return value


def _uint_list(value: Any, path: str, limit: int) -> list[int]:
    items = _list(value, path)
    for i, item in enumerate(items):
        _uint(item, f"{path}[{i}]", limit)
    return items


def _validate_row(row: Any, path: str, kind: str | None) -> dict[str, Any]:
    row = _object(row, path, ROW_KEYS)
    row_kind = _enum(row["kind"], f"{path}.kind", ROW_KINDS)
    if kind is not None and row_kind != kind:
        _fail(f"{path}.kind", f"must be {kind!r} here, got {row_kind!r}")
    sampler = _enum(row["sampler"], f"{path}.sampler", SAMPLERS)
    options = _uint_list(row["options"], f"{path}.options", U16)
    if not options:
        _fail(f"{path}.options", "must not be empty")
    if any(b <= a for a, b in zip(options, options[1:])):
        _fail(f"{path}.options", "must be strictly ascending (canonical order)")
    chosen = _uint(row["chosen"], f"{path}.chosen", U16)
    if chosen not in options:
        _fail(f"{path}.chosen", f"{chosen} is not one of the options")
    stream_values = _uint_list(row["stream_values"], f"{path}.stream_values", U64)
    logits = row["logits_bits"]
    p_chosen = row["p_chosen_bits"]
    value_bits = row["value_bits"]
    if sampler == LEARNED_SAMPLER:
        if len(stream_values) != 1:
            _fail(f"{path}.stream_values", "the learned sampler takes exactly one stream value")
        if len(_uint_list(logits, f"{path}.logits_bits", U32)) != len(options):
            _fail(f"{path}.logits_bits", "must hold one f32 bit pattern per option")
        _uint(p_chosen, f"{path}.p_chosen_bits", U64)
        if row_kind == "pick":
            _uint(value_bits, f"{path}.value_bits", U32)
        elif value_bits is not None:
            _fail(f"{path}.value_bits", "is for learned pick rows only")
    else:
        for name in ("logits_bits", "p_chosen_bits", "value_bits"):
            if row[name] is not None:
                _fail(f"{path}.{name}", "must be null for the fixture sampler")
    return row


def _validate_build_rows(value: Any, path: str) -> dict[str, Any] | None:
    if value is None:
        return None
    rows = _object(value, path, BUILD_ROWS_KEYS)
    count = _validate_row(rows["count"], f"{path}.count", "count")
    basics = count["chosen"]
    if basics > DECK_CARDS:
        _fail(f"{path}.count.chosen", f"basics count {basics} exceeds {DECK_CARDS}")
    pointer = _list(rows["pointer"], f"{path}.pointer")
    for i, row in enumerate(pointer):
        _validate_row(row, f"{path}.pointer[{i}]", "pointer")
    if len(pointer) != DECK_CARDS - basics:
        _fail(f"{path}.pointer", f"must hold {DECK_CARDS - basics} steps for {basics} basics, got {len(pointer)}")
    type_set = rows["type_set"]
    split = _list(rows["split"], f"{path}.split")
    if basics == 0:
        if type_set is not None:
            _fail(f"{path}.type_set", "must be null when the basics count is 0")
        if split:
            _fail(f"{path}.split", "must be empty when the basics count is 0")
        return rows
    _validate_row(type_set, f"{path}.type_set", "type_set")
    mask = type_set["chosen"]
    bits = bin(mask).count("1")
    if mask == 0 or mask > 31 or bits > basics:
        _fail(f"{path}.type_set.chosen", f"mask {mask} is not a feasible type set for {basics} basics")
    for i, row in enumerate(split):
        _validate_row(row, f"{path}.split[{i}]", "split")
    expected_split = basics - bits if bits >= 2 else 0
    if len(split) != expected_split:
        _fail(f"{path}.split", f"must hold {expected_split} draws, got {len(split)}")
    return rows


def _validate_end(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict) or "kind" not in value:
        _fail(path, "must be an object with a 'kind'")
    kind = _enum(value["kind"], f"{path}.kind", tuple(END_FIELDS))
    end = _object(value, path, END_FIELDS[kind])
    for key in END_FIELDS[kind]:
        if key == "kind":
            continue
        if key == "winner_seat":
            _uint_or_null(end[key], f"{path}.winner_seat", U8)
        else:
            _string(end[key], f"{path}.{key}")
    return end


def expected_stop_kind(end: dict[str, Any]) -> str | None:
    """`stop_kind` per section 4: null for a natural end; the end kind, with `:` and the code for
    terminal and session_error ends."""
    kind = end["kind"]
    if kind == "natural":
        return None
    if kind in ("terminal", "session_error"):
        return f"{kind}:{end['code']}"
    return kind


def _validate_attempt(value: Any, path: str, index: int) -> dict[str, Any]:
    attempt = _object(value, path, ATTEMPT_KEYS)
    if _uint(attempt["attempt"], f"{path}.attempt", U8) != index:
        _fail(f"{path}.attempt", f"must be {index} (attempts in order)")
    for key in ("deal_seed", "env_seed", "action_seed", "decisions"):
        _uint(attempt[key], f"{path}.{key}", U64)
    for key in ("physical_decisions", "policy_steps"):
        _uint_or_null(attempt[key], f"{path}.{key}", U64)
    end = _validate_end(attempt["end"], f"{path}.end")
    halted = attempt["halted"]
    if type(halted) is not bool:
        _fail(f"{path}.halted", f"must be a boolean, got {halted!r}")
    if halted != (end["kind"] != "natural"):
        _fail(f"{path}.halted", "an attempt is halted exactly when its end is not natural")
    if attempt["stop_kind"] != expected_stop_kind(end):
        _fail(f"{path}.stop_kind", f"must be {expected_stop_kind(end)!r}, got {attempt['stop_kind']!r}")
    _uint_or_null(attempt["last_action_source"], f"{path}.last_action_source", U16)
    return attempt


def game_seats(chairs: list[int | None]) -> list[int]:
    """D5's seat identifiers passed to AttemptSpec::seats: [c0, c1] when both chairs are distinct
    non-null chair ids, otherwise [0, 1]."""
    c0, c1 = chairs
    if c0 is not None and c1 is not None and c0 != c1:
        return [c0, c1]
    return [0, 1]


def _validate_game(value: Any, path: str, deck_by_id: dict[int, dict[str, Any]], max_replays: int) -> dict[str, Any]:
    game = _object(value, path, GAME_KEYS)
    _uint(game["game"], f"{path}.game", U32)
    _uint(game["group"], f"{path}.group", U32)
    _enum(game["role"], f"{path}.role", GAME_ROLES)
    _uint_or_null(game["pairing"], f"{path}.pairing", U8)
    decks = _uint_list(game["decks"], f"{path}.decks", U32)
    if len(decks) != 2:
        _fail(f"{path}.decks", "must hold exactly 2 deck ids (p0, p1)")
    if decks[0] == decks[1]:
        _fail(f"{path}.decks", "a deck cannot play itself")
    for i, deck_id in enumerate(decks):
        if deck_id not in deck_by_id:
            _fail(f"{path}.decks[{i}]", f"deck {deck_id} is not in the record's decks")
    chairs = _list(game["chairs"], f"{path}.chairs")
    if len(chairs) != 2:
        _fail(f"{path}.chairs", "must hold exactly 2 entries")
    for i, chair in enumerate(chairs):
        _uint_or_null(chair, f"{path}.chairs[{i}]", U8)
        if chair != deck_by_id[decks[i]]["chair"]:
            _fail(f"{path}.chairs[{i}]", f"{chair!r} differs from deck {decks[i]}'s chair")
    seat_rule = _enum(game["seat_rule"], f"{path}.seat_rule", SEAT_RULES)
    _uint(game["game_seed"], f"{path}.game_seed", U64)
    _uint(game["episode"], f"{path}.episode", U64)
    attempts = _list(game["attempts"], f"{path}.attempts")
    if not 1 <= len(attempts) <= MAX_ATTEMPTS:
        _fail(f"{path}.attempts", f"must hold 1 to {MAX_ATTEMPTS} attempts, got {len(attempts)}")
    if len(attempts) > max_replays + 1:
        _fail(f"{path}.attempts", f"{len(attempts)} attempts exceed max_replays {max_replays}")
    if seat_rule == "schedule" and len(attempts) != 1:
        _fail(f"{path}.attempts", "a read game (seat_rule schedule) has exactly one attempt")
    for k, attempt in enumerate(attempts):
        _validate_attempt(attempt, f"{path}.attempts[{k}]", k)
    for k, attempt in enumerate(attempts[:-1]):
        if not attempt["halted"]:
            _fail(f"{path}.attempts[{k}]", "a completed attempt must be the game's last")
    final = attempts[-1]
    scores = game["scores"]
    if final["halted"]:
        if scores is not None:
            _fail(f"{path}.scores", "must be null when no attempt completed")
    else:
        if not (isinstance(scores, list) and len(scores) == 2 and all(_is_int(s) for s in scores)):
            _fail(f"{path}.scores", f"must be two integers, got {scores!r}")
        if scores not in ([1, -1], [-1, 1], [0, 0]):
            _fail(f"{path}.scores", f"must be [1, -1], [-1, 1] or [0, 0], got {scores!r}")
        winner = final["end"]["winner_seat"]
        seats = game_seats(chairs)
        if winner is None:
            expected_scores = [0, 0]
        elif winner == seats[0]:
            expected_scores = [1, -1]
        elif winner == seats[1]:
            expected_scores = [-1, 1]
        else:
            _fail(f"{path}.attempts[-1].end.winner_seat",
                  f"winner_seat {winner} is not in game seats {seats}")
            raise AssertionError
        if scores != expected_scores:
            _fail(f"{path}.scores",
                  f"scores {scores} disagree with winner_seat {winner} for game seats {seats}")
    return game


def _validate_deck(value: Any, path: str, index: int, draft_seed: int | None = None) -> dict[str, Any]:
    deck = _object(value, path, DECK_KEYS)
    if _uint(deck["deck"], f"{path}.deck", U32) != index:
        _fail(f"{path}.deck", f"must be its index {index}")
    source = _enum(deck["source"], f"{path}.source", DECK_SOURCES)
    chair = _uint_or_null(deck["chair"], f"{path}.chair", U8)
    build = _uint_or_null(deck["build"], f"{path}.build", U8)
    builder = _object(deck["builder"], f"{path}.builder", KIND_PIN_KEYS)
    if source == "build":
        _sha_or_null(builder["pin"], f"{path}.builder.pin")
    _uint_or_null(deck["build_seed"], f"{path}.build_seed", U64)
    cards = _uint_list(deck["cards"], f"{path}.cards", U16)
    if len(cards) != DECK_CARDS:
        _fail(f"{path}.cards", f"must hold {DECK_CARDS} cards, got {len(cards)}")
    if cards != sorted(cards):
        _fail(f"{path}.cards", "must be in canonical order (ascending registry id)")
    build_rows = _validate_build_rows(deck["build_rows"], f"{path}.build_rows")
    dealt = deck["dealt"]
    if source == "build":
        if chair is None or chair >= SEATS:
            _fail(f"{path}.chair", "a built deck names its chair 0 to 7")
        if build is None:
            _fail(f"{path}.build", "a built deck names its build index k")
        kind = _enum(builder["kind"], f"{path}.builder.kind", BUILDER_KINDS)
        if dealt is not None:
            _fail(f"{path}.dealt", "must be null for a built deck")
        if kind == "stated" and build_rows is not None:
            _fail(f"{path}.build_rows", "the stated builder writes no rows")
        if kind in ("uniform", "learned") and build_rows is None:
            _fail(f"{path}.build_rows", f"a {kind} builder writes its build rows")
        if draft_seed is not None:
            expected_build_seed = seeds_v1.build_seed(draft_seed, chair, build)
            if kind in ("uniform", "learned"):
                if deck["build_seed"] != expected_build_seed:
                    _fail(f"{path}.build_seed",
                          f"build_seed {deck['build_seed']!r} differs from derived stream {expected_build_seed}")
            elif deck["build_seed"] is not None and deck["build_seed"] != expected_build_seed:
                _fail(f"{path}.build_seed",
                      f"stated build_seed {deck['build_seed']!r} differs from derived stream {expected_build_seed}")
    else:
        if chair is not None or build is not None:
            _fail(path, "a dealt deck has null chair and build")
        if builder["kind"] != DEALT_BUILDER_KIND:
            _fail(f"{path}.builder.kind", f"must be {DEALT_BUILDER_KIND!r} for a dealt deck")
        if deck["build_seed"] is not None or build_rows is not None:
            _fail(path, "a dealt deck has null build_seed and build_rows")
        dealt = _object(dealt, f"{path}.dealt", DEALT_KEYS)
        _string(dealt["function"], f"{path}.dealt.function")
        _list(dealt["key"], f"{path}.dealt.key")
        if builder["pin"] != {"function": dealt["function"], "key": dealt["key"]} or list(builder["pin"]) != list(
                DEALT_KEYS):
            _fail(f"{path}.builder.pin", "a dealt deck's builder pin is {function, key}, copied from its dealt")
    return deck


def _validate_draft(value: Any, path: str, draft_seed: int | None) -> dict[str, Any] | None:
    if value is None:
        return None
    draft = _object(value, path, DRAFT_KEYS)
    if draft["schema"] != DRAFT_RECORD_SCHEMA:
        _fail(f"{path}.schema", f"must be {DRAFT_RECORD_SCHEMA!r}, got {draft['schema']!r}")
    _uint(draft["draft_seed"], f"{path}.draft_seed", U64)
    if draft["draft_seed"] != draft_seed:
        _fail(f"{path}.draft_seed", "differs from seeds.draft_seed")
    _list(draft["packs"], f"{path}.packs")
    _list(draft["picks"], f"{path}.picks")
    pools = _list(draft["pools"], f"{path}.pools")
    if len(pools) != SEATS:
        _fail(f"{path}.pools", f"must hold {SEATS} pools")
    for s, pool in enumerate(pools):
        _uint_list(pool, f"{path}.pools[{s}]", U16)
        if len(pool) != POOL_CARDS:
            _fail(f"{path}.pools[{s}]", f"must hold {POOL_CARDS} cards, got {len(pool)}")
    return draft


def pod_seeds(namespace: str, run: str, pod_index: int) -> dict[str, int]:
    """Section 4: pod j's draft, pod and league seeds."""
    return {
        "draft_seed": deal.seed([namespace, "draft", run, pod_index]),
        "pod_seed": deal.seed([namespace, "pod", run, pod_index]),
        "league_seed": deal.seed([namespace, "league", run, pod_index]),
    }


def validate_record(record: Any, path: str = "record") -> dict[str, Any]:
    """Validate one pod record in place and return it. Raises RecordError on any deviation."""
    rec = _object(record, path, RECORD_KEYS)
    if rec["schema"] != POD_RECORD_SCHEMA:
        _fail(f"{path}.schema", f"unknown version {rec['schema']!r}")
    if rec["interfaces"] != INTERFACES_VERSION:
        _fail(f"{path}.interfaces", f"unknown version {rec['interfaces']!r}")
    _enum(rec["use"], f"{path}.use", USES)
    for key in ("namespace", "run"):
        if not (isinstance(rec[key], str) and NAME_RE.match(rec[key])):
            _fail(f"{path}.{key}", f"must match [a-z0-9-]+, got {rec[key]!r}")
    pod_index = _uint(rec["pod_index"], f"{path}.pod_index", U64)

    if rec["use"] == "client" and rec["namespace"] != CLIENT_NAMESPACE:
        _fail(f"{path}.namespace", f"client pods use namespace {CLIENT_NAMESPACE!r}")
    if rec["namespace"] == CLIENT_NAMESPACE and rec["use"] != "client":
        _fail(f"{path}.use", f"namespace {CLIENT_NAMESPACE!r} is reserved for client pods")
    raw_seeds = rec["seeds"]
    read_pod = rec["use"] in READ_USES or (
        rec["use"] == "test" and isinstance(raw_seeds, dict) and "draft_seed_namespace" in raw_seeds)
    seeds = _object(raw_seeds, f"{path}.seeds", READ_SEEDS_KEYS if read_pod else SEEDS_KEYS)
    for key in SEEDS_KEYS:
        _uint_or_null(seeds[key], f"{path}.seeds.{key}", U64)
    if read_pod:
        # Change 3: read seeds come from the plan, which the reader checks; never from pod_index.
        for key in ("draft_seed", "pod_seed"):
            _uint(seeds[key], f"{path}.seeds.{key}", U64)
        for key in ("draft_seed_namespace", "pod_seed_namespace"):
            if not (isinstance(seeds[key], str) and NAME_RE.match(seeds[key])):
                _fail(f"{path}.seeds.{key}", f"must match [a-z0-9-]+, got {seeds[key]!r}")
    elif rec["use"] != "client":
        expected = pod_seeds(rec["namespace"], rec["run"], pod_index)
        for key in SEEDS_KEYS:
            value = seeds[key]
            if value is not None and value != expected[key]:
                _fail(f"{path}.seeds.{key}", f"{value} differs from seed([ns, {key[:-5]!r}, run, {pod_index}])")

    pins = _object(rec["pins"], f"{path}.pins", PINS_KEYS)
    for key in PINS_KEYS:
        _sha_or_null(pins[key], f"{path}.pins.{key}")

    pilot = _object(rec["pilot"], f"{path}.pilot", PILOT_KEYS)
    pilot_kind = _enum(pilot["kind"], f"{path}.pilot.kind", PILOT_KINDS)
    _sha_or_null(pilot["pin"], f"{path}.pilot.pin")
    _string(pilot["decoding"], f"{path}.pilot.decoding")
    if pilot_kind == "uniform" and (pilot["pin"] is not None or pilot["decoding"] != UNIFORM_DECODING):
        _fail(f"{path}.pilot", f"a uniform pilot has a null pin and decoding {UNIFORM_DECODING!r}")
    if pilot_kind == "checkpoint" and pilot["pin"] is None:
        _fail(f"{path}.pilot.pin", "a checkpoint pilot is pinned")

    caps = _object(rec["caps"], f"{path}.caps", CAPS_KEYS)
    for key in CAPS_KEYS:
        limit = U32 if key == "max_stack_objects" else U64
        _uint(caps[key], f"{path}.caps.{key}", limit)
    if caps["max_stack_objects"] != MAX_STACK_OBJECTS:
        _fail(
            f"{path}.caps.max_stack_objects",
            f"must be the signed stack cap {MAX_STACK_OBJECTS}, got {caps['max_stack_objects']}",
        )
    max_replays = caps["max_replays"]
    if max_replays > MAX_ATTEMPTS - 1:
        _fail(f"{path}.caps.max_replays", f"at most {MAX_ATTEMPTS - 1}, got {max_replays}")

    _validate_draft(rec["draft"], f"{path}.draft", seeds["draft_seed"])

    chairs = _list(rec["chairs"], f"{path}.chairs")
    if len(chairs) != SEATS:
        _fail(f"{path}.chairs", f"must hold {SEATS} chairs")
    for i, chair in enumerate(chairs):
        cpath = f"{path}.chairs[{i}]"
        _object(chair, cpath, CHAIR_KEYS)
        if _uint(chair["chair"], f"{cpath}.chair", U8) != i:
            _fail(f"{cpath}.chair", f"must be {i}")
        _enum(chair["role"], f"{cpath}.role", CHAIR_ROLES)
        picker = _object(chair["picker"], f"{cpath}.picker", KIND_PIN_KEYS)
        _enum(picker["kind"], f"{cpath}.picker.kind", PICKER_KINDS)
        _sha_or_null(picker["pin"], f"{cpath}.picker.pin")
        for r, row in enumerate(_list(chair["pick_rows"], f"{cpath}.pick_rows")):
            _validate_row(row, f"{cpath}.pick_rows[{r}]", "pick")

    decks = _list(rec["decks"], f"{path}.decks")
    deck_by_id: dict[int, dict[str, Any]] = {}
    for i, deck in enumerate(decks):
        _validate_deck(deck, f"{path}.decks[{i}]", i, seeds["draft_seed"])
        deck_by_id[i] = deck

    games = _list(rec["games"], f"{path}.games")
    game_by_id: dict[int, dict[str, Any]] = {}
    for i, game in enumerate(games):
        _validate_game(game, f"{path}.games[{i}]", deck_by_id, max_replays)
        if game["game"] != i:
            _fail(f"{path}.games[{i}].game", f"must be {i} (games in logical order)")
        game_by_id[i] = game
    if read_pod:
        if max_replays != 0:
            _fail(f"{path}.caps.max_replays", "reads never replay: max_replays is 0")
        for i, game in enumerate(games):
            if game["seat_rule"] != "schedule":
                _fail(f"{path}.games[{i}].seat_rule", "every game of a read pod uses seat_rule 'schedule'")

    status = _enum(rec["status"], f"{path}.status", STATUSES)
    stopped = status == "stopped"

    pairs = _list(rec["paired_builds"], f"{path}.paired_builds")
    for i, pair in enumerate(pairs):
        _validate_paired(pair, f"{path}.paired_builds[{i}]", deck_by_id, game_by_id, stopped=stopped)
    firsts = [pair["games_x"][0] for pair in pairs]
    if firsts != sorted(firsts) or len(set(firsts)) != len(firsts):
        _fail(f"{path}.paired_builds", "entries are ordered by the first game of games_x (change 4)")

    expected_halts = [
        (game["game"], attempt["attempt"], attempt["stop_kind"], attempt["last_action_source"], attempt["deal_seed"])
        for game in games
        for attempt in game["attempts"]
        if attempt["halted"]
    ]
    halts = _list(rec["halts"], f"{path}.halts")
    actual_halts = []
    for i, halt in enumerate(halts):
        _object(halt, f"{path}.halts[{i}]", HALT_KEYS)
        actual_halts.append(tuple(halt[k] for k in HALT_KEYS))
    if actual_halts != expected_halts:
        _fail(f"{path}.halts", "the ledger differs from the halted attempts of the games, in logical order")

    stop = rec["stop"]
    if status == "complete":
        if stop is not None:
            _fail(f"{path}.stop", "must be null for a complete pod")
    else:
        stop = _object(stop, f"{path}.stop", STOP_KEYS)
        _uint(stop["game"], f"{path}.stop.game", U32)
        _uint_or_null(stop["pairing"], f"{path}.stop.pairing", U8)
        _uint_list(stop["decks"], f"{path}.stop.decks", U32)
        for k, entry in enumerate(_list(stop["attempts"], f"{path}.stop.attempts")):
            _object(entry, f"{path}.stop.attempts[{k}]", STOP_ATTEMPT_KEYS)
            _uint(entry["deal_seed"], f"{path}.stop.attempts[{k}].deal_seed", U64)
            if entry["stop_kind"] is not None:
                _string(entry["stop_kind"], f"{path}.stop.attempts[{k}].stop_kind")
            _uint_or_null(entry["last_action_source"], f"{path}.stop.attempts[{k}].last_action_source", U16)
    return rec


def is_read_pod(record: dict[str, Any]) -> bool:
    """A validated record is a read pod when its seeds carry the plan's source fields."""
    return "draft_seed_namespace" in record["seeds"]


def _validate_paired(
    value: Any,
    path: str,
    deck_by_id: dict[int, dict[str, Any]],
    game_by_id: dict[int, dict[str, Any]],
    *,
    stopped: bool = False,
) -> dict[str, Any]:
    pair = _object(value, path, PAIRED_KEYS)
    _enum(pair["kind"], f"{path}.kind", PAIRED_KINDS)
    chair = _uint(pair["chair"], f"{path}.chair", U8)
    for key in ("deck_x", "deck_y"):
        deck_id = _uint(pair[key], f"{path}.{key}", U32)
        if deck_id not in deck_by_id:
            _fail(f"{path}.{key}", f"deck {deck_id} is not in the record")
        if deck_by_id[deck_id]["chair"] != chair:
            _fail(f"{path}.{key}", f"deck {deck_id} is not chair {chair}'s")
    if pair["deck_x"] == pair["deck_y"]:
        _fail(path, "deck_x and deck_y must differ")
    opponents = _uint_list(pair["opponents"], f"{path}.opponents", U32)
    for deck_id in opponents:
        if deck_id not in deck_by_id:
            _fail(f"{path}.opponents", f"deck {deck_id} is not in the record")
    _uint_or_null(pair["q"], f"{path}.q", U8)
    games_x = _uint_list(pair["games_x"], f"{path}.games_x", U32)
    games_y = _uint_list(pair["games_y"], f"{path}.games_y", U32)
    if len(games_x) != len(games_y) or not games_x:
        _fail(path, "games_x and games_y must be non-empty and of equal length")
    if len(set(games_x)) != len(games_x) or len(set(games_y)) != len(games_y) or (set(games_x) & set(games_y)):
        _fail(path, "games_x and games_y must hold distinct game ids")
    played_len = len(game_by_id)
    for gx, gy in zip(games_x, games_y):
        has_x = gx in game_by_id
        has_y = gy in game_by_id
        if not stopped and not (has_x and has_y):
            missing = gx if not has_x else gy
            _fail(path, f"game {missing} is not in the record")
        if stopped:
            if not has_x and gx < played_len:
                _fail(path, f"game {gx} is missing inside the played prefix")
            if not has_y and gy < played_len:
                _fail(path, f"game {gy} is missing inside the played prefix")
        x_seat = y_seat = None
        if has_x:
            x_game = game_by_id[gx]
            if pair["deck_x"] not in x_game["decks"]:
                _fail(path, f"game {gx} does not hold deck_x {pair['deck_x']}")
            x_seat = x_game["decks"].index(pair["deck_x"])
            if x_game["decks"][1 - x_seat] not in opponents:
                _fail(path, f"game {gx}'s opponent is not listed in opponents")
        if has_y:
            y_game = game_by_id[gy]
            if pair["deck_y"] not in y_game["decks"]:
                _fail(path, f"game {gy} does not hold deck_y {pair['deck_y']}")
            y_seat = y_game["decks"].index(pair["deck_y"])
            if y_game["decks"][1 - y_seat] not in opponents:
                _fail(path, f"game {gy}'s opponent is not listed in opponents")
        if has_x and has_y:
            x_game, y_game = game_by_id[gx], game_by_id[gy]
            if x_seat != y_seat:
                _fail(path, f"matched games {gx} and {gy} put the paired decks in different seats")
            if x_game["game_seed"] != y_game["game_seed"] or x_game["episode"] != y_game["episode"]:
                _fail(path, f"matched games {gx} and {gy} are not coupled (game seed and episode)")
            if x_game["decks"][1 - x_seat] != y_game["decks"][1 - y_seat]:
                _fail(path, f"matched games {gx} and {gy} face different opponents")
    return pair


def win_fraction(game: dict[str, Any], deck_id: int) -> float | None:
    """The win fraction of `deck_id` in a validated game: 1, 0.5 or 0; None if the game is halted
    (no completed attempt)."""
    if game["scores"] is None:
        return None
    position = game["decks"].index(deck_id)
    return WIN_FRACTION[game["scores"][position]]


def load_records(path: str | Path) -> tuple[list[dict[str, Any]], str]:
    """Load one file holding a pod record object or a `pods.json` array of them. Returns the
    validated records and the file's sha256."""
    data = Path(path).read_bytes()
    sha = hashlib.sha256(data).hexdigest()
    try:
        doc = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise RecordError(f"pod record file is not UTF-8 JSON: {exc}") from exc
    records = doc if isinstance(doc, list) else [doc]
    if not records:
        raise RecordError("pod record file holds no record")
    for i, record in enumerate(records):
        validate_record(record, f"record[{i}]")
    return records, sha
