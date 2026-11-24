"""Read plans for the pod runner's explicit-plan form: reads (b), (b') and (c), and the dose reads.

One plan per draft. Every game has `seat_rule: "schedule"`, and the runner plays it with
`max_replays` 0 (reads never replay, A2.4). Decks are given by card list in canonical order
(40 rows, id ascending); the runner writes every deck's rows itself.

Plan schema `mtg-kernel-cube-read-plan/v1` (written here; the runner, D5, reads it). Keys in order:
  schema, interfaces, use, namespace, run, pod_index, seeds {draft_seed, pod_seed},
  seat_rule, max_replays, inputs {pool_sha256, constants_sha256}, pools (8 x 45),
  decks   [{deck, source, chair, build, builder {kind, pin}, cards, dealt}],
  games   [{game, group, role, pairing, decks, chairs, seat_rule, game_seed, episode}],
  paired_builds [{kind, chair, deck_x, deck_y, opponents, games_x, games_y}]
Deck, game and paired-build entries use the pod record's field names and meanings (section 4), so
the runner copies them into the record and the reader checks the record against the plan.

Read (b), use `read_b`: for each seat s of draft d, the candidate deck D and its one opponent from
deal.deal_read_b_opponent at index 8d + s [proposed for A6], 4 games with A1.5's game seeds
seed(["cube_deal_b_v1", namespace, run, index, slot]) and seats (D in session seat 0 in slots 0
and 2, seat 1 in slots 1 and 3), exactly as deal.build_read_b writes them. No new seed is derived.

Reads (c) and (b'), use `paired_build`, kind `candidate_stated`; dose reads, use `dose`, kind
`current_initial`: for each seat, the two compared builds (x, y) and C(P) at opponent index 0
(deal_common.py), 8 games: x's 4 and y's 4 on the same seeds and seats, coupled.

Logical order. Read (b): seat s holds decks 2s (D) and 2s + 1 (opponent) and games 4s + slot, in
group 4s + slot. Paired plans: seat s holds decks 3s (x), 3s + 1 (y) and 3s + 2 (C(P)), and games
8s + 2 slot (x) and 8s + 2 slot + 1 (y), both in group 4s + slot (the games sharing a game seed).
`episode` is the slot [proposed for A6], equal in both coupled games.
"""

from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

import deal  # noqa: E402
import deal_common  # noqa: E402
import pod_record  # noqa: E402

PLAN_SCHEMA = "mtg-kernel-cube-read-plan/v1"
SEATS = pod_record.SEATS
DECK_CARDS = pod_record.DECK_CARDS
PLAN_KEYS = (
    "schema",
    "interfaces",
    "use",
    "namespace",
    "run",
    "pod_index",
    "seeds",
    "seat_rule",
    "max_replays",
    "inputs",
    "pools",
    "decks",
    "games",
    "paired_builds",
)
PLAN_SEED_KEYS = ("draft_seed", "pod_seed")
PLAN_INPUT_KEYS = ("pool_sha256", "constants_sha256")
PLAN_DECK_KEYS = ("deck", "source", "chair", "build", "builder", "cards", "dealt")
PLAN_GAME_KEYS = ("game", "group", "role", "pairing", "decks", "chairs", "seat_rule", "game_seed", "episode")
PAIRED_USES = {"paired_build": "candidate_stated", "dose": "current_initial"}
STATED_BUILDER = {"kind": "stated", "pin": None}
DEALT_BUILDER = {"kind": pod_record.DEALT_BUILDER_KIND, "pin": None}


def read_b_index(draft: int, seat: int) -> int:
    """The read (b) index of draft d and seat s: 8d + s [proposed for A6]."""
    return SEATS * draft + seat


def _check_name(value: Any, label: str) -> str:
    if not (isinstance(value, str) and pod_record.NAME_RE.match(value)):
        raise ValueError(f"{label} must match [a-z0-9-]+, got {value!r}")
    return value


def _check_builder(builder: Any, label: str) -> dict[str, Any]:
    if not isinstance(builder, dict) or list(builder.keys()) != ["kind", "pin"]:
        raise ValueError(f"{label} must be an object {{kind, pin}}")
    if builder["kind"] not in pod_record.BUILDER_KINDS:
        raise ValueError(f"{label}.kind must be one of {list(pod_record.BUILDER_KINDS)}")
    pin = builder["pin"]
    if pin is not None and not (isinstance(pin, str) and pod_record.SHA256_RE.match(pin)):
        raise ValueError(f"{label}.pin must be a sha256 hex string or null")
    return {"kind": builder["kind"], "pin": pin}


def _check_pools(pool_state: dict[str, Any], pools: Any) -> list[list[int]]:
    if not isinstance(pools, list) or len(pools) != SEATS:
        raise ValueError(f"pools must be a list of {SEATS} pools")
    return [deal_common.validate_pool_cards(pool_state, p) for p in pools]


def _check_deck(pool_state: dict[str, Any], pool_cards: list[int], deck: Any, label: str) -> list[int]:
    """A built deck of pool P: 40 card ids, each from P (no more copies than P holds) or a basic."""
    if not isinstance(deck, list) or len(deck) != DECK_CARDS:
        raise ValueError(f"{label} must be a list of {DECK_CARDS} card ids")
    basic_ids = set(pool_state["basic_id_by_color"].values())
    available = Counter(pool_cards)
    for cid, count in Counter(deck).items():
        if not deal._is_int(cid):
            raise ValueError(f"{label} card id must be an integer, got {cid!r}")
        if cid in basic_ids:
            continue
        if count > available[cid]:
            raise ValueError(f"{label} holds card {cid} {count} times; the pool holds it {available[cid]} times")
    return sorted(deck)


def _plan_head(
    use: str,
    namespace: str,
    run: str,
    draft: int,
    pools: list[list[int]],
    pool_sha256: str | None,
    constants_sha256: str | None,
) -> dict[str, Any]:
    seeds = pod_record.pod_seeds(namespace, run, draft)
    return {
        "schema": PLAN_SCHEMA,
        "interfaces": pod_record.INTERFACES_VERSION,
        "use": use,
        "namespace": namespace,
        "run": run,
        "pod_index": draft,
        "seeds": {"draft_seed": seeds["draft_seed"], "pod_seed": seeds["pod_seed"]},
        "seat_rule": "schedule",
        "max_replays": 0,
        "inputs": {"pool_sha256": pool_sha256, "constants_sha256": constants_sha256},
        "pools": [list(p) for p in pools],
        "decks": [],
        "games": [],
        "paired_builds": [],
    }


def _deck_entry(deck_id: int, source: str, chair: int | None, builder: dict[str, Any], cards: list[int],
                dealt: dict[str, Any] | None) -> dict[str, Any]:
    return {
        "deck": deck_id,
        "source": source,
        "chair": chair,
        "build": 0 if source == "build" else None,
        "builder": dict(builder),
        "cards": list(cards),
        "dealt": dealt,
    }


def _game_entry(game: int, group: int, role: str, decks: list[int], chairs: list[int | None],
                game_seed: int, episode: int) -> dict[str, Any]:
    return {
        "game": game,
        "group": group,
        "role": role,
        "pairing": None,
        "decks": decks,
        "chairs": chairs,
        "seat_rule": "schedule",
        "game_seed": game_seed,
        "episode": episode,
    }


def _check_draft_args(namespace: Any, run: Any, draft: Any) -> None:
    _check_name(namespace, "namespace")
    _check_name(run, "run")
    if not (deal._is_int(draft) and draft >= 0):
        raise ValueError(f"draft must be an integer >= 0, got {draft!r}")


def build_read_b_plan(
    pool_doc: Any,
    constants_doc: Any,
    namespace: str,
    run: str,
    draft: int,
    pools: Any,
    candidate_decks: Any,
    candidate_builder: Any,
    pool_sha256: str | None = None,
    constants_sha256: str | None = None,
) -> dict[str, Any]:
    """Read (b)'s plan for one draft: 8 candidate decks, each against its A1.5 opponent, 4 games."""
    pool_state = deal.validate_pool(pool_doc)
    constants = deal.validate_constants(constants_doc)
    _check_draft_args(namespace, run, draft)
    pools = _check_pools(pool_state, pools)
    builder = _check_builder(candidate_builder, "candidate_builder")
    if not isinstance(candidate_decks, list) or len(candidate_decks) != SEATS:
        raise ValueError(f"candidate_decks must hold {SEATS} decks")

    plan = _plan_head("read_b", namespace, run, draft, pools, pool_sha256, constants_sha256)
    for seat in range(SEATS):
        cards = _check_deck(pool_state, pools[seat], candidate_decks[seat], f"candidate deck of seat {seat}")
        index = read_b_index(draft, seat)
        opponent = deal.deal_read_b_opponent(pool_state, constants, cards, namespace, run, index)
        d_id, o_id = 2 * seat, 2 * seat + 1
        plan["decks"].append(_deck_entry(d_id, "build", seat, builder, cards, None))
        plan["decks"].append(
            _deck_entry(o_id, "dealt", None, DEALT_BUILDER, opponent["mainboard"],
                        {"function": deal.READ_B_KEY, "key": [namespace, run, index]})
        )
        for slot in range(deal.READ_B_GAMES):
            d_seat = slot % 2
            decks = [d_id, o_id] if d_seat == 0 else [o_id, d_id]
            chairs = [seat, None] if d_seat == 0 else [None, seat]
            plan["games"].append(
                _game_entry(
                    4 * seat + slot,
                    4 * seat + slot,
                    "read_b",
                    decks,
                    chairs,
                    deal.seed([deal.READ_B_KEY, namespace, run, index, slot]),
                    slot,
                )
            )
    return plan


def build_paired_plan(
    pool_doc: Any,
    constants_doc: Any,
    use: str,
    namespace: str,
    run: str,
    draft: int,
    pools: Any,
    decks_x: Any,
    decks_y: Any,
    builder_x: Any,
    builder_y: Any,
    pool_sha256: str | None = None,
    constants_sha256: str | None = None,
) -> dict[str, Any]:
    """A paired build plan for one draft: per seat, builds x and y of pool P each play C(P) at
    opponent index 0 on the same 4 game seeds and seats.

    use `paired_build` (reads (c) and (b')): x the candidate build, y the stated build.
    use `dose`: x the current builder's build, y the run's initial builder's build.
    """
    if use not in PAIRED_USES:
        raise ValueError(f"use must be one of {sorted(PAIRED_USES)}, got {use!r}")
    pool_state = deal.validate_pool(pool_doc)
    constants = deal.validate_constants(constants_doc)
    _check_draft_args(namespace, run, draft)
    pools = _check_pools(pool_state, pools)
    bx = _check_builder(builder_x, "builder_x")
    by = _check_builder(builder_y, "builder_y")
    if use == "paired_build" and (bx["kind"] == "stated" or by != STATED_BUILDER):
        raise ValueError("a candidate_stated plan has a candidate builder x and the stated builder y")
    for label, decks in (("decks_x", decks_x), ("decks_y", decks_y)):
        if not isinstance(decks, list) or len(decks) != SEATS:
            raise ValueError(f"{label} must hold {SEATS} decks")

    opponent_index = 0
    plan = _plan_head(use, namespace, run, draft, pools, pool_sha256, constants_sha256)
    for seat in range(SEATS):
        x_cards = _check_deck(pool_state, pools[seat], decks_x[seat], f"deck x of seat {seat}")
        y_cards = _check_deck(pool_state, pools[seat], decks_y[seat], f"deck y of seat {seat}")
        common = deal_common.deal_common_opponent(
            pool_state, constants, pools[seat], namespace, draft, seat, opponent_index
        )
        x_id, y_id, c_id = 3 * seat, 3 * seat + 1, 3 * seat + 2
        plan["decks"].append(_deck_entry(x_id, "build", seat, bx, x_cards, None))
        plan["decks"].append(_deck_entry(y_id, "build", seat, by, y_cards, None))
        plan["decks"].append(
            _deck_entry(c_id, "dealt", None, DEALT_BUILDER, common["mainboard"],
                        {"function": deal_common.COMMON_KEY,
                         "key": deal_common.common_key(namespace, draft, seat, opponent_index)})
        )
        games_x: list[int] = []
        games_y: list[int] = []
        for spec in deal_common.common_games(namespace, draft, seat, opponent_index):
            slot = spec["slot"]
            for arm, deck_id in enumerate((x_id, y_id)):
                game_id = 8 * seat + 2 * slot + arm
                if spec["build_seat"] == 0:
                    decks, chairs = [deck_id, c_id], [seat, None]
                else:
                    decks, chairs = [c_id, deck_id], [None, seat]
                plan["games"].append(
                    _game_entry(game_id, 4 * seat + slot, "common", decks, chairs, spec["game_seed"], slot)
                )
                (games_x if arm == 0 else games_y).append(game_id)
        plan["paired_builds"].append(
            {
                "kind": PAIRED_USES[use],
                "chair": seat,
                "deck_x": x_id,
                "deck_y": y_id,
                "opponents": [c_id],
                "games_x": games_x,
                "games_y": games_y,
            }
        )
    return plan


def build_read_c_plan(pool_doc: Any, constants_doc: Any, namespace: str, run: str, draft: int, pools: Any,
                      candidate_decks: Any, stated_decks: Any, candidate_builder: Any,
                      pool_sha256: str | None = None, constants_sha256: str | None = None) -> dict[str, Any]:
    """Reads (c) and (b'): the candidate build and the stated build of each pool against C(P)."""
    return build_paired_plan(pool_doc, constants_doc, "paired_build", namespace, run, draft, pools,
                             candidate_decks, stated_decks, candidate_builder, dict(STATED_BUILDER),
                             pool_sha256, constants_sha256)


def validate_plan(plan: Any, path: str = "plan") -> dict[str, Any]:
    """Check a plan's shape and key order strictly (the reader trusts nothing it did not check)."""
    obj = pod_record._object
    plan = obj(plan, path, PLAN_KEYS)
    if plan["schema"] != PLAN_SCHEMA:
        raise pod_record.RecordError(f"{path}.schema: unknown version {plan['schema']!r}")
    if plan["interfaces"] != pod_record.INTERFACES_VERSION:
        raise pod_record.RecordError(f"{path}.interfaces: unknown version {plan['interfaces']!r}")
    if plan["use"] not in ("read_b",) + tuple(PAIRED_USES):
        raise pod_record.RecordError(f"{path}.use: {plan['use']!r} is not a planned read")
    obj(plan["seeds"], f"{path}.seeds", PLAN_SEED_KEYS)
    obj(plan["inputs"], f"{path}.inputs", PLAN_INPUT_KEYS)
    if plan["seat_rule"] != "schedule" or plan["max_replays"] != 0:
        raise pod_record.RecordError(f"{path}: a read plan has seat_rule 'schedule' and max_replays 0")
    for i, deck in enumerate(pod_record._list(plan["decks"], f"{path}.decks")):
        obj(deck, f"{path}.decks[{i}]", PLAN_DECK_KEYS)
    for i, game in enumerate(pod_record._list(plan["games"], f"{path}.games")):
        obj(game, f"{path}.games[{i}]", PLAN_GAME_KEYS)
    for i, pair in enumerate(pod_record._list(plan["paired_builds"], f"{path}.paired_builds")):
        obj(pair, f"{path}.paired_builds[{i}]", pod_record.PAIRED_KEYS)
    return plan


def check_record_against_plan(record: dict[str, Any], plan: dict[str, Any]) -> None:
    """A validated record played exactly the plan: same pod, decks, games and pairings."""
    where = f"record {record['namespace']}/{record['run']}/{record['pod_index']}"
    for key in ("use", "namespace", "run", "pod_index"):
        if record[key] != plan[key]:
            raise pod_record.RecordError(f"{where}: {key} {record[key]!r} differs from the plan's {plan[key]!r}")
    for key in PLAN_SEED_KEYS:
        if record["seeds"][key] != plan["seeds"][key]:
            raise pod_record.RecordError(f"{where}: seeds.{key} differs from the plan")
    if record["caps"]["max_replays"] != plan["max_replays"]:
        raise pod_record.RecordError(f"{where}: caps.max_replays differs from the plan")
    if record["draft"] is not None and record["draft"]["pools"] != plan["pools"]:
        raise pod_record.RecordError(f"{where}: the draft's pools differ from the plan's")
    if len(record["decks"]) != len(plan["decks"]):
        raise pod_record.RecordError(f"{where}: {len(record['decks'])} decks, the plan has {len(plan['decks'])}")
    for rec_deck, plan_deck in zip(record["decks"], plan["decks"]):
        for key in PLAN_DECK_KEYS:
            if rec_deck[key] != plan_deck[key]:
                raise pod_record.RecordError(f"{where}: deck {plan_deck['deck']} {key} differs from the plan")
    if len(record["games"]) != len(plan["games"]):
        raise pod_record.RecordError(f"{where}: {len(record['games'])} games, the plan has {len(plan['games'])}")
    for rec_game, plan_game in zip(record["games"], plan["games"]):
        for key in PLAN_GAME_KEYS:
            if rec_game[key] != plan_game[key]:
                raise pod_record.RecordError(f"{where}: game {plan_game['game']} {key} differs from the plan")
    if record["paired_builds"] != plan["paired_builds"]:
        raise pod_record.RecordError(f"{where}: paired_builds differ from the plan")


def serialize_plan(plan: dict[str, Any]) -> str:
    return json.dumps(plan, indent=2, ensure_ascii=True) + "\n"


def load_plan(path: str | Path) -> tuple[dict[str, Any], str]:
    data = Path(path).read_bytes()
    plan = json.loads(data.decode("utf-8"))
    return validate_plan(plan), hashlib.sha256(data).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("kind", choices=("b", "c", "dose"))
    parser.add_argument("--pool", required=True, type=Path)
    parser.add_argument("--constants", required=True, type=Path)
    parser.add_argument("--namespace", required=True)
    parser.add_argument("--run", required=True)
    parser.add_argument("--draft", required=True, type=int)
    parser.add_argument(
        "--builds",
        required=True,
        type=Path,
        help="JSON object: pools (8 x 45 ids), x (8 decks), y (8 decks; not for b), "
        "builder_x and builder_y ({kind, pin}; builder_y not for b or c)",
    )
    parser.add_argument("--out", type=Path)
    args = parser.parse_args(argv)

    pool_bytes = args.pool.read_bytes()
    constants_bytes = args.constants.read_bytes()
    pool_doc = json.loads(pool_bytes.decode("utf-8"))
    constants_doc = json.loads(constants_bytes.decode("utf-8"))
    builds = json.loads(args.builds.read_text(encoding="utf-8"))
    hashes = (hashlib.sha256(pool_bytes).hexdigest(), hashlib.sha256(constants_bytes).hexdigest())
    if args.kind == "b":
        plan = build_read_b_plan(pool_doc, constants_doc, args.namespace, args.run, args.draft,
                                 builds["pools"], builds["x"], builds["builder_x"], *hashes)
    elif args.kind == "c":
        plan = build_read_c_plan(pool_doc, constants_doc, args.namespace, args.run, args.draft,
                                 builds["pools"], builds["x"], builds["y"], builds["builder_x"], *hashes)
    else:
        plan = build_paired_plan(pool_doc, constants_doc, "dose", args.namespace, args.run, args.draft,
                                 builds["pools"], builds["x"], builds["y"], builds["builder_x"],
                                 builds["builder_y"], *hashes)
    text = serialize_plan(plan)
    if args.out is None:
        sys.stdout.write(text)
    else:
        args.out.write_bytes(text.encode("utf-8"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
