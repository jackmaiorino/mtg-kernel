"""Deal deterministic pairs of cube decks and emit episodes in training run-config shape.

Within an episode no non-basic card appears in both decks; basic lands are exempt
(both decks may hold copies of the same basic land type).

Usage:
  deal.py --pool POOL.json --constants CONSTANTS.json --namespace NS --run RUN
          --update-start U0 --update-end U1 [--base BASE.json]
          [--max-physical-decisions N] [--max-policy-steps N] [--out OUT.json]
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys
from typing import Any, Iterable

SCHEDULE_SCHEMA = "cube_deal_schedule/v1"
MASK = (1 << 63) - 1
COLORS = ("W", "U", "B", "R", "G")
REQUIRED_CONSTANT_KEYS = frozenset(
    {
        "deck_size",
        "colours_per_seat",
        "nonland_spells",
        "nonbasic_lands",
        "basic_split",
        "pairs_per_update",
    }
)
ACCEPTED_BASIC_SPLITS = ("even_first_colour_extra",)
MAX_CARD_ID = (1 << 16) - 1
MAX_EPISODE_ID_BYTES = 128
MAX_EPISODE_ID_LEN = MAX_EPISODE_ID_BYTES
MAX_PHYSICAL_DECISIONS_DEFAULT = 100_000
MAX_POLICY_STEPS_DEFAULT = 200_070
MAX_PHYSICAL_DECISIONS_LIMIT = 100_000
MAX_POLICY_STEPS_LIMIT = 1_000_000


def _is_int(value: Any) -> bool:
    return type(value) is int


def seed(parts: list[Any] | tuple[Any, ...]) -> int:
    """Derive a 63-bit integer seed via SHA-256 of compact JSON, matching tools/ex1/qual.py."""
    text = json.dumps(list(parts), separators=(",", ":"))
    return int.from_bytes(hashlib.sha256(text.encode()).digest()[:8], "little") & MASK


def _rank_digest(parts: list[Any]) -> bytes:
    """Full 256-bit SHA-256 digest of compact JSON for uniform, Python-version-independent ordering."""
    text = json.dumps(parts, separators=(",", ":"))
    return hashlib.sha256(text.encode("utf-8")).digest()


def _draw_ordered(items: list[Any], count: int, key_prefix: list[Any], item_key: Any) -> list[Any]:
    """Draw `count` distinct items in a uniformly random order determined by SHA-256 rank keys."""
    if count < 0 or count > len(items):
        raise ValueError(f"cannot draw {count} items from {len(items)} available")
    if count == 0:
        return []
    ranked = sorted(items, key=lambda item: (_rank_digest(key_prefix + [item_key(item)]), item_key(item)))
    return ranked[:count]


def validate_constants(doc: Any) -> dict[str, Any]:
    """Validate the deal-rule constants JSON. Every key is required and none are hard-coded."""
    if not isinstance(doc, dict):
        raise ValueError("constants must be a JSON object")
    missing = sorted(REQUIRED_CONSTANT_KEYS - set(doc.keys()))
    if missing:
        raise ValueError(f"constants file lacks required keys: {missing}")
    unknown = sorted(set(doc.keys()) - REQUIRED_CONSTANT_KEYS)
    if unknown:
        raise ValueError(f"constants file has unknown keys: {unknown}")

    deck_size = doc["deck_size"]
    colours_per_seat = doc["colours_per_seat"]
    nonland_spells = doc["nonland_spells"]
    nonbasic_lands = doc["nonbasic_lands"]
    basic_split = doc["basic_split"]
    pairs_per_update = doc["pairs_per_update"]

    if not (_is_int(deck_size) and deck_size >= 1):
        raise ValueError(f"deck_size must be an integer >= 1, got {deck_size!r}")
    if not (_is_int(colours_per_seat) and 1 <= colours_per_seat <= len(COLORS)):
        raise ValueError(f"colours_per_seat must be an integer in [1, {len(COLORS)}], got {colours_per_seat!r}")
    if not (_is_int(nonland_spells) and nonland_spells >= 0):
        raise ValueError(f"nonland_spells must be an integer >= 0, got {nonland_spells!r}")
    if not (_is_int(nonbasic_lands) and nonbasic_lands >= 0):
        raise ValueError(f"nonbasic_lands must be an integer >= 0, got {nonbasic_lands!r}")
    if nonland_spells + nonbasic_lands > deck_size:
        raise ValueError(
            f"nonland_spells ({nonland_spells}) + nonbasic_lands ({nonbasic_lands}) exceeds deck_size ({deck_size})"
        )
    if basic_split not in ACCEPTED_BASIC_SPLITS:
        raise ValueError(f"basic_split must be one of {ACCEPTED_BASIC_SPLITS}, got {basic_split!r}")
    if not (_is_int(pairs_per_update) and pairs_per_update >= 1):
        raise ValueError(f"pairs_per_update must be an integer >= 1, got {pairs_per_update!r}")
    if pairs_per_update % 2 != 0:
        raise ValueError(
            f"pairs_per_update must be even, got {pairs_per_update}: the learner holds the first-drawn deck "
            "in even pairs and the second-drawn deck in odd pairs, so an update balances only with an even count"
        )

    return {
        "deck_size": deck_size,
        "colours_per_seat": colours_per_seat,
        "nonland_spells": nonland_spells,
        "nonbasic_lands": nonbasic_lands,
        "basic_split": basic_split,
        "pairs_per_update": pairs_per_update,
    }


def validate_pool(doc: Any) -> dict[str, Any]:
    """Validate the pool JSON produced by pool.py and index the five basic lands by colour."""
    if not isinstance(doc, dict):
        raise ValueError("pool must be a JSON object")
    for key in ("kernel_carddb_hash", "spells", "nonbasic_lands", "basics"):
        if key not in doc:
            raise ValueError(f"pool lacks required key {key!r}")
    if not isinstance(doc["kernel_carddb_hash"], str) or not doc["kernel_carddb_hash"].strip():
        raise ValueError("pool lacks a non-empty string 'kernel_carddb_hash'")

    fenced_names: set[str] = set()
    if "fenced" in doc:
        if not isinstance(doc["fenced"], list):
            raise ValueError("pool 'fenced' must be a list")
        for entry in doc["fenced"]:
            if not isinstance(entry, dict) or not isinstance(entry.get("name"), str):
                raise ValueError("pool 'fenced' entry must be an object with string 'name'")
            fenced_names.add(entry["name"])

    valid_color_set = set(COLORS)
    seen_ids: set[int] = set()

    def _check_cards(cards: Any, label: str, expected_land: bool) -> list[dict[str, Any]]:
        if not isinstance(cards, list):
            raise ValueError(f"pool {label!r} must be a list")
        checked: list[dict[str, Any]] = []
        for idx, card in enumerate(cards):
            if not isinstance(card, dict):
                raise ValueError(f"pool {label}[{idx}] must be an object")
            cid = card.get("card_id")
            name = card.get("name")
            colors = card.get("colors")
            is_land = card.get("is_land")
            if not (_is_int(cid) and 0 <= cid <= MAX_CARD_ID):
                raise ValueError(f"pool {label}[{idx}] has invalid card_id: {cid!r}")
            if not isinstance(name, str) or not name.strip():
                raise ValueError(f"pool {label}[{idx}] has invalid name: {name!r}")
            if name in fenced_names:
                raise ValueError(f"pool {label} contains fenced card {name!r}")
            if not isinstance(colors, list) or not all(isinstance(c, str) and c in valid_color_set for c in colors):
                raise ValueError(f"pool {label}[{idx}] ({name!r}) has invalid colors: {colors!r}")
            if is_land is not expected_land:
                raise ValueError(f"pool {label}[{idx}] ({name!r}) has is_land={is_land!r}, expected {expected_land!r}")
            if cid in seen_ids:
                raise ValueError(f"pool contains duplicate card_id {cid}")
            seen_ids.add(cid)
            checked.append(card)
        return checked

    spells = _check_cards(doc["spells"], "spells", expected_land=False)
    nonbasic_lands = _check_cards(doc["nonbasic_lands"], "nonbasic_lands", expected_land=True)
    basics = _check_cards(doc["basics"], "basics", expected_land=True)

    basic_id_by_color: dict[str, int] = {}
    for basic in basics:
        color = basic.get("color")
        if color not in valid_color_set:
            raise ValueError(f"basic land {basic['name']!r} lacks a valid 'color' in {COLORS}")
        if color in basic_id_by_color:
            raise ValueError(f"pool lists duplicate basic land for color {color!r}")
        basic_id_by_color[color] = basic["card_id"]
    missing_basics = [c for c in COLORS if c not in basic_id_by_color]
    if missing_basics:
        raise ValueError(f"pool lacks basic lands for colors: {missing_basics}")

    return {
        "kernel_carddb_hash": doc["kernel_carddb_hash"],
        "spells": spells,
        "nonbasic_lands": nonbasic_lands,
        "basic_id_by_color": basic_id_by_color,
    }


def _split_basics(
    seat_colors: list[str],
    basic_count: int,
    basic_split: str,
    basic_id_by_color: dict[str, int],
) -> list[int]:
    if basic_count == 0:
        return []
    if basic_split == "even_first_colour_extra":
        base, extra = divmod(basic_count, len(seat_colors))
        ids: list[int] = []
        for idx, color in enumerate(seat_colors):
            copies = base + (1 if idx < extra else 0)
            ids.extend([basic_id_by_color[color]] * copies)
        return ids
    raise ValueError(f"unsupported basic_split: {basic_split!r}")


def deal_pair(
    pool_state: dict[str, Any],
    constants: dict[str, Any],
    namespace: str,
    run: str,
    update: int,
    pair_index: int,
) -> tuple[dict[str, Any], dict[str, Any]]:
    """Deal two decks for one (namespace, run, update, pair_index).

    Within an episode no non-basic card appears in both decks; basic lands are
    exempt (both decks may hold copies of the same basic land type).
    """
    pair_seed = seed(["cube_deal_v1", namespace, run, update, pair_index])
    key_prefix = ["cube_deal_v1", namespace, run, update, pair_index, pair_seed]

    colours_per_seat = constants["colours_per_seat"]
    nonland_spells_count = constants["nonland_spells"]
    nonbasic_lands_count = constants["nonbasic_lands"]
    basic_count = constants["deck_size"] - nonland_spells_count - nonbasic_lands_count
    basic_split = constants["basic_split"]

    seat_colors = [
        _draw_ordered(list(COLORS), colours_per_seat, key_prefix + ["seat", seat, "colors"], lambda c: c)
        for seat in (0, 1)
    ]

    used_nonbasic_ids: set[int] = set()
    seat_spells: list[list[dict[str, Any]]] = []
    for seat in (0, 1):
        allowed = set(seat_colors[seat])
        eligible = [
            card
            for card in pool_state["spells"]
            if card["card_id"] not in used_nonbasic_ids and set(card["colors"]).issubset(allowed)
        ]
        if len(eligible) < nonland_spells_count:
            raise ValueError(
                f"update {update} pair {pair_index} seat {seat} ({''.join(seat_colors[seat])}): "
                f"need {nonland_spells_count} eligible spells, only {len(eligible)} remain"
            )
        drawn = _draw_ordered(
            eligible,
            nonland_spells_count,
            key_prefix + ["seat", seat, "spells"],
            lambda c: c["card_id"],
        )
        used_nonbasic_ids.update(card["card_id"] for card in drawn)
        seat_spells.append(drawn)

    seat_lands: list[list[dict[str, Any]]] = []
    for seat in (0, 1):
        eligible_lands = [
            card for card in pool_state["nonbasic_lands"] if card["card_id"] not in used_nonbasic_ids
        ]
        if len(eligible_lands) < nonbasic_lands_count:
            raise ValueError(
                f"update {update} pair {pair_index} seat {seat}: "
                f"need {nonbasic_lands_count} nonbasic lands, only {len(eligible_lands)} remain"
            )
        drawn_lands = _draw_ordered(
            eligible_lands,
            nonbasic_lands_count,
            key_prefix + ["seat", seat, "nonbasic_lands"],
            lambda c: c["card_id"],
        )
        used_nonbasic_ids.update(card["card_id"] for card in drawn_lands)
        seat_lands.append(drawn_lands)

    decks: list[dict[str, Any]] = []
    for seat in (0, 1):
        basics_ids = _split_basics(
            seat_colors[seat],
            basic_count,
            basic_split,
            pool_state["basic_id_by_color"],
        )
        mainboard = sorted(
            [card["card_id"] for card in seat_spells[seat]]
            + [card["card_id"] for card in seat_lands[seat]]
            + basics_ids
        )
        decks.append(
            {
                "label": f"cube-u{update}-p{pair_index}-d{seat}-{''.join(seat_colors[seat])}",
                "mainboard": mainboard,
                "sideboard": [],
            }
        )
    return decks[0], decks[1]


def _validate_run_params(
    namespace: str,
    run: str,
    update_start: int,
    update_end: int,
    max_physical_decisions: int,
    max_policy_steps: int,
) -> None:
    if not isinstance(namespace, str) or not namespace.strip():
        raise ValueError("namespace must be a non-empty string")
    if not isinstance(run, str) or not run.strip():
        raise ValueError("run must be a non-empty string")
    if not (_is_int(update_start) and update_start >= 0):
        raise ValueError(f"update_start must be an integer >= 0, got {update_start!r}")
    if not (_is_int(update_end) and update_end > update_start):
        raise ValueError(f"update_end ({update_end!r}) must be an integer > update_start ({update_start!r})")
    if not (_is_int(max_physical_decisions) and 1 <= max_physical_decisions <= MAX_PHYSICAL_DECISIONS_LIMIT):
        raise ValueError(
            f"max_physical_decisions must be in [1, {MAX_PHYSICAL_DECISIONS_LIMIT}], got {max_physical_decisions!r}"
        )
    if not (_is_int(max_policy_steps) and 1 <= max_policy_steps <= MAX_POLICY_STEPS_LIMIT):
        raise ValueError(f"max_policy_steps must be in [1, {MAX_POLICY_STEPS_LIMIT}], got {max_policy_steps!r}")


def format_episode_id(namespace: str, run: str, update: int, slot: int, learner_seat: int) -> str:
    """Format an unambiguous episode id within Rust's 128-byte UTF-8 limit.

    Uses UTF-8 byte-length prefixes for `namespace` and `run` so arbitrary strings
    never collide across different `(namespace, run, update, slot, learner_seat)` tuples.
    """
    try:
        ns_bytes = namespace.encode("utf-8")
        run_bytes = run.encode("utf-8")
    except UnicodeEncodeError as exc:
        raise ValueError(f"namespace and run must be valid UTF-8 strings: {exc}") from exc
    episode_id = f"{len(ns_bytes)}:{namespace}/{len(run_bytes)}:{run}/u{update}/j{slot}/s{learner_seat}"
    byte_len = len(episode_id.encode("utf-8"))
    if byte_len > MAX_EPISODE_ID_BYTES:
        raise ValueError(
            f"episode id exceeds {MAX_EPISODE_ID_BYTES} UTF-8 bytes ({byte_len} bytes): {episode_id!r}"
        )
    return episode_id


def _make_episode_entry(
    namespace: str,
    run: str,
    update: int,
    slot: int,
    learner_seat: int,
    decks: list[dict[str, Any]],
    max_physical_decisions: int,
    max_policy_steps: int,
    extra_episode_fields: dict[str, Any] | None = None,
) -> dict[str, Any]:
    episode_id = format_episode_id(namespace, run, update, slot, learner_seat)
    episode: dict[str, Any] = {
        "id": episode_id,
        "seed": seed([namespace, run, update, slot]),
        "starting_player": 0,
        "learner_seat": learner_seat,
        "registered": [copy.deepcopy(d) for d in decks],
        "selected": [copy.deepcopy(d) for d in decks],
        "postboard": False,
        "max_physical_decisions": max_physical_decisions,
        "max_policy_steps": max_policy_steps,
    }
    if extra_episode_fields:
        for key, value in extra_episode_fields.items():
            if key == "mulligan_modes":
                if not isinstance(value, list) or len(value) != 2:
                    raise ValueError(f"mulligan_modes must be a 2-element list, got {value!r}")
                modes = copy.deepcopy(value)
                episode[key] = [modes[1], modes[0]] if learner_seat == 1 else modes
            else:
                episode[key] = copy.deepcopy(value)
    return {
        "episode": episode,
        "opponent": {"kind": "current"},
    }


def _extract_base_episode_template(base: dict[str, Any]) -> tuple[int, int, dict[str, Any]]:
    max_phys = MAX_PHYSICAL_DECISIONS_DEFAULT
    max_steps = MAX_POLICY_STEPS_DEFAULT
    extra: dict[str, Any] = {}
    iterations = base.get("iterations")
    if isinstance(iterations, list) and iterations:
        first_episodes = iterations[0].get("episodes")
        if isinstance(first_episodes, list) and first_episodes:
            template = first_episodes[0].get("episode", {})
            if "max_physical_decisions" in template:
                max_phys = template["max_physical_decisions"]
            if "max_policy_steps" in template:
                max_steps = template["max_policy_steps"]
            for opt_key in ("runtime_rules", "opening", "mulligan_modes"):
                if opt_key in template and template[opt_key] is not None:
                    extra[opt_key] = template[opt_key]
            if "mulligan_modes" in extra:
                modes = extra["mulligan_modes"]
                if not isinstance(modes, list) or len(modes) != 2:
                    raise ValueError(f"base episode mulligan_modes must be a 2-element list, got {modes!r}")
                template_seat = template.get("learner_seat", 0)
                if not (_is_int(template_seat) and template_seat in (0, 1)):
                    raise ValueError(f"base episode learner_seat must be 0 or 1, got {template_seat!r}")
                # Store in learner-at-seat-0 order so seat-swapped episodes swap modes with decks.
                extra["mulligan_modes"] = [modes[1], modes[0]] if template_seat == 1 else list(modes)
    return max_phys, max_steps, extra


def build_schedule(
    pool_doc: Any,
    constants_doc: Any,
    namespace: str,
    run: str,
    update_start: int,
    update_end: int,
    pool_sha256: str | None = None,
    constants_sha256: str | None = None,
    base_config: dict[str, Any] | None = None,
    max_physical_decisions: int | None = None,
    max_policy_steps: int | None = None,
) -> dict[str, Any]:
    """Deal episodes across [update_start, update_end) in run-config iterations shape.

    Within each episode, no non-basic card appears in both decks; basic lands are
    exempt (both decks may hold copies of the same basic land type).
    """
    pool_state = validate_pool(pool_doc)
    constants = validate_constants(constants_doc)

    extra_episode_fields: dict[str, Any] = {}
    if base_config is not None:
        if not isinstance(base_config, dict):
            raise ValueError("base config must be a JSON object")
        base_phys, base_steps, extra_episode_fields = _extract_base_episode_template(base_config)
        eff_phys = base_phys if max_physical_decisions is None else max_physical_decisions
        eff_steps = base_steps if max_policy_steps is None else max_policy_steps
    else:
        eff_phys = MAX_PHYSICAL_DECISIONS_DEFAULT if max_physical_decisions is None else max_physical_decisions
        eff_steps = MAX_POLICY_STEPS_DEFAULT if max_policy_steps is None else max_policy_steps

    _validate_run_params(namespace, run, update_start, update_end, eff_phys, eff_steps)

    pairs_per_update = constants["pairs_per_update"]
    iterations: list[dict[str, Any]] = []
    for update in range(update_start, update_end):
        episodes: list[dict[str, Any]] = []
        for pair_index in range(pairs_per_update):
            deck_a, deck_b = deal_pair(pool_state, constants, namespace, run, update, pair_index)
            # deck_a is drawn first, from the fuller pool. The learner holds deck_a in both slots of
            # an even pair and deck_b in both slots of an odd pair. Seat 0 starts, so each deck is
            # on the play once per pair.
            learner_deck, opponent_deck = (deck_a, deck_b) if pair_index % 2 == 0 else (deck_b, deck_a)
            slot_a = 2 * pair_index
            slot_b = 2 * pair_index + 1
            episodes.append(
                _make_episode_entry(
                    namespace=namespace,
                    run=run,
                    update=update,
                    slot=slot_a,
                    learner_seat=0,
                    decks=[learner_deck, opponent_deck],
                    max_physical_decisions=eff_phys,
                    max_policy_steps=eff_steps,
                    extra_episode_fields=extra_episode_fields,
                )
            )
            episodes.append(
                _make_episode_entry(
                    namespace=namespace,
                    run=run,
                    update=update,
                    slot=slot_b,
                    learner_seat=1,
                    decks=[opponent_deck, learner_deck],
                    max_physical_decisions=eff_phys,
                    max_policy_steps=eff_steps,
                    extra_episode_fields=extra_episode_fields,
                )
            )
        iterations.append({"episodes": episodes})

    if base_config is not None:
        out = {k: copy.deepcopy(v) for k, v in base_config.items() if k != "iterations"}
        out["iterations"] = iterations
        return out

    result: dict[str, Any] = {
        "schema": SCHEDULE_SCHEMA,
        "namespace": namespace,
        "run": run,
        "update_start": update_start,
        "update_end": update_end,
        "kernel_carddb_hash": pool_state["kernel_carddb_hash"],
        "constants": constants,
        "iterations": iterations,
    }
    if pool_sha256 is not None:
        result["pool_sha256"] = pool_sha256
    if constants_sha256 is not None:
        result["constants_sha256"] = constants_sha256
    return result


def card_names_by_id(pool_doc: Any) -> dict[int, str]:
    """Map every card id of a validated pool (spells, nonbasic lands and basics) to its name."""
    validate_pool(pool_doc)
    return {card["card_id"]: card["name"] for key in ("spells", "nonbasic_lands", "basics") for card in pool_doc[key]}


def deck_card_names(card_ids: Iterable[int], names_by_id: dict[int, str]) -> list[str]:
    """Translate a deck's card ids to card names; an id the map lacks is an error."""
    names: list[str] = []
    for cid in card_ids:
        if not _is_int(cid) or cid not in names_by_id:
            raise ValueError(f"deck card id {cid!r} is not in the pool")
        names.append(names_by_id[cid])
    return names


def canonical_deck_text(card_names: Iterable[str]) -> str:
    """Canonical text of one deck: a `name count` line per distinct card name, sorted by name
    (Unicode code point order), each line ending in LF. Card order and file formatting do not matter."""
    counts: dict[str, int] = {}
    for name in card_names:
        if not isinstance(name, str) or not name.strip():
            raise ValueError(f"deck card name must be a non-empty string, got {name!r}")
        if "\n" in name or "\r" in name:
            raise ValueError(f"deck card name contains a line break: {name!r}")
        counts[name] = counts.get(name, 0) + 1
    if not counts:
        raise ValueError("a deck must hold at least one card")
    return "".join(f"{name} {counts[name]}\n" for name in sorted(counts))


def canonical_decks_sha256(decks: Iterable[Iterable[str]]) -> str:
    """SHA-256 (lowercase hex) of a list of decks, each given as card names (repeats count).

    The hashed text is, for each deck in list order, its canonical_deck_text followed by one
    empty line, encoded as UTF-8 with LF line endings. It depends only on the decks' card names
    and counts and on the order of the decks, never on how or where they were stored."""
    text = "".join(canonical_deck_text(deck) + "\n" for deck in decks)
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _read_json_with_sha256(path: Path) -> tuple[Any, str]:
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ValueError(f"failed to parse JSON {path}: {exc}") from exc
    return doc, digest


def build_schedule_from_paths(
    pool_path: Path | str,
    constants_path: Path | str,
    namespace: str,
    run: str,
    update_start: int,
    update_end: int,
    base_path: Path | str | None = None,
    max_physical_decisions: int | None = None,
    max_policy_steps: int | None = None,
) -> dict[str, Any]:
    pool_doc, pool_sha = _read_json_with_sha256(Path(pool_path))
    constants_doc, constants_sha = _read_json_with_sha256(Path(constants_path))
    base_doc = None
    if base_path is not None:
        base_doc, _ = _read_json_with_sha256(Path(base_path))
    return build_schedule(
        pool_doc=pool_doc,
        constants_doc=constants_doc,
        namespace=namespace,
        run=run,
        update_start=update_start,
        update_end=update_end,
        pool_sha256=pool_sha,
        constants_sha256=constants_sha,
        base_config=base_doc,
        max_physical_decisions=max_physical_decisions,
        max_policy_steps=max_policy_steps,
    )


def serialize_json(doc: dict[str, Any]) -> str:
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def write_output(text: str, out: str | Path | None) -> None:
    """Write `text` as UTF-8 bytes with LF line endings on every OS, to `out` or to stdout."""
    data = text.encode("utf-8")
    if out is not None:
        out_path = Path(out)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_bytes(data)
        return
    buffer = getattr(sys.stdout, "buffer", None)
    if buffer is None:
        # A text-only stream such as io.StringIO does no newline translation.
        sys.stdout.write(text)
        return
    sys.stdout.flush()
    buffer.write(data)
    buffer.flush()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--pool", required=True, help="Pool JSON file produced by pool.py")
    parser.add_argument("--constants", required=True, help="Deal-rule constants JSON file")
    parser.add_argument("--namespace", required=True, help="Seed and episode-id namespace string")
    parser.add_argument("--run", required=True, help="Run name string")
    parser.add_argument("--update-start", type=int, required=True, help="First update index (inclusive)")
    parser.add_argument("--update-end", type=int, required=True, help="Last update index (exclusive)")
    parser.add_argument(
        "--base",
        default=None,
        help="Optional base native training run config JSON whose iterations are replaced",
    )
    parser.add_argument(
        "--max-physical-decisions",
        type=int,
        default=None,
        help="Override episode max_physical_decisions",
    )
    parser.add_argument(
        "--max-policy-steps",
        type=int,
        default=None,
        help="Override episode max_policy_steps",
    )
    parser.add_argument("--out", default=None, help="Output JSON path (prints to stdout if omitted)")
    args = parser.parse_args(argv)

    try:
        doc = build_schedule_from_paths(
            pool_path=args.pool,
            constants_path=args.constants,
            namespace=args.namespace,
            run=args.run,
            update_start=args.update_start,
            update_end=args.update_end,
            base_path=args.base,
            max_physical_decisions=args.max_physical_decisions,
            max_policy_steps=args.max_policy_steps,
        )
    except (OSError, ValueError) as exc:
        print(f"deal.py: {exc}", file=sys.stderr)
        return 1

    write_output(serialize_json(doc), args.out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
