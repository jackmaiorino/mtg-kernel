"""Distinct schema-7 trusted two-seat GraveyardReturn host client.

Validation checks closed public DTOs and issued prompt identity. It does not
accept engine state, grant Partial deck admission, or establish runtime evidence.
"""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U32, I32, ROOT, ANNOTATION, COUNTERS
from limited_next_five_session_v6 import _shape, _validate_next_five_visible, _validate_play_context

PROTOCOL = "kernel_limited_graveyard_return_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-graveyard-return-decision/v7"
FAMILY = ("enum", ("Alesha", "NineLives"))
NONNEG = ("int", 0, (1 << 31) - 1)
GR_COUNTERS = {**COUNTERS, "revival": NONNEG}
TARGET = {"original": REF, "current_exact_graveyard_card": ("optional", REF), "legal_now": bool}
PROGRAM = ("variant", {
    "AleshaAttackCounter": {"plus_one": I32},
    "AleshaRaid": {"power": {"amount": I32, "source": ("enum", ("CurrentOriginalIncarnation", "ExactDepartureLki"))},
        "captured_controller_attacked_this_turn": bool, "may_return_at_resolution": bool,
        "optional_choice": ("variant", {"BeforeResolution": None, "AwaitingCostlessReturnChoice": None,
                                       "Answered": {"accept": bool}})},
    "NineLivesDeath": {"departure_revival": NONNEG, "return_base_before_replacements": NONNEG},
    "NineLivesDelayedReturn": {"departure_revival": NONNEG, "return_base_before_replacements": NONNEG},
})
TRIGGER = {"family": FAMILY, "stage": ("enum", ("Held", "Published", "Resolving")), "position": U32,
           "source": ROOT, "program": PROGRAM, "targets": ("list", TARGET)}
DEATH = {"occurrence": U32, "source": ROOT, "departure_revival": NONNEG, "return_base_before_replacements": NONNEG,
         "collection_completed": bool, "exact_graveyard_card_now": ("optional", REF)}
SCHEDULE = {k: v for k, v in DEATH.items() if k != "collection_completed"}
SCHEDULE["next_future_any_player_end_step"] = bool
CANDIDATE = {"drake_activation": ("optional", ANNOTATION), "programs": ("list", TRIGGER),
             "selected_graveyard_target": ("optional", TARGET), "nine_lives_deaths": ("list", DEATH),
             "nine_lives_schedules": ("list", SCHEDULE)}


def _expected_graveyard_return_meanings() -> list[dict[str, Any]]:
    def entry(revival):
        return {"EnterUnderOriginalSpellController": {
            "active_player_main_phase_empty_stack_with_priority": True, "grants_additional_cast_permission": False,
            "revival_entry_if_genuinely_cast": revival,
            "countered_spell_goes_to_physical_owner_graveyard_subject_to_replacements": True}}
    def flags(*keys):
        return dict.fromkeys(keys, True)
    alesha = ["PayPrintedMana", entry(None),
        {"OnAttackPutCounterOnOriginalSource": {"plus_one": 1, "exact_original_battlefield_incarnation": True,
                                               "counter_replacements_apply": True}},
        {"OnCapturedControllerEndStepIfRaid": flags(
            "intervening_condition_at_collection_and_resolution", "actual_attack_declaration_this_turn_not_combat_damage",
            "one_target_creature_card_in_captured_controller_graveyard", "target_selection_mandatory_before_response_window",
            "captured_controller_may_decline_return_at_resolution_without_payment",
            "mana_value_at_most_original_source_current_power_or_departure_lki",
            "exact_announced_target_incarnation_rechecked_at_resolution", "return_under_physical_card_owner", "entry_replacements_apply")}]
    nine = ["PayPrintedMana", entry({"count": 8, "cast_from_any_already_permitted_zone": True,
        "uncast_spell_copy_qualifies": False, "counters_before_entry_observers": True, "current_entry_replacements_apply_once": True}),
        {"OnDeathWithPositiveDepartingRevival": flags("ability_active_on_departing_effective_printing",
            "exact_departing_counter_lki", "departing_counter_condition_rechecked_at_death_trigger_resolution",
            "death_trigger_resolution_creates_delayed_trigger", "countered_death_trigger_creates_no_schedule")},
        {"AtNextFutureAnyPlayerEndStep": flags("consume_schedule_once_when_triggered", "already_begun_end_step_waits_for_next",
                                             "countered_delayed_trigger_does_not_retry")},
        {"ReturnOriginalGraveyardIncarnation": {**flags("captured_death_controller_not_physical_owner",
            "current_entry_replacements_apply_once", "counters_before_entry_observers", "stale_graveyard_incarnation_is_no_op",
            "token_ceases_before_later_return", "return_is_not_cast"), "subtract_from_departure_count_before_entry_replacements": 1}}]
    return [dict(family=family, printed=dict(generic_mana=1, colored_mana=colors, mana_value=3, types=["Creature"],
        subtypes=subtypes, legendary=legendary, colors=list(dict.fromkeys(colors)), power=power, toughness=power,
        keyword_bits=bits), ordered_rules=rules) for family, colors, subtypes, legendary, power, bits, rules in (
            ("Alesha", ["B", "R"], ["Human", "Warrior"], True, 2, 32, alesha),
            ("NineLives", ["B", "B"], ["Cat"], False, 1, 0, nine))]


def _validate_references(value: Any) -> None:
    seen = {}
    pending = [value]
    while pending:
        item = pending.pop()
        if type(item) is dict:
            if set(item) & {"arena_id", "zone_change_count", "object_id", "card_db_id", "private_world",
                    "original_world", "operation_world", "original_stack", "event_history", "private_binding",
                    "executable_binding", "original_index", "debug", "journal_index"}:
                raise LimitedSessionError("raw native authority in schema-7 prompt")
            if set(item) == set(REF):
                _shape(item, REF, "safe reference")
                if not _same_json_value(item, seen.setdefault(item["handle"], item)):
                    raise LimitedSessionError("schema-7 safe handle identity changed")
            pending.extend(item.values())
        elif type(item) is list:
            pending.extend(item)


def _validate_graveyard_return_visible(visible: Any, actor: str, annotations: Any, actions: Any) -> None:
    def fail(message):
        raise LimitedSessionError(message)
    if type(visible) is not dict or set(visible) != {"common", "prepared", "next_five", "graveyard_return"}:
        fail("missing complete GraveyardReturn visible state")
    _shape(annotations, ("list", CANDIDATE), "action_meanings")
    if type(actions) is not list or len(annotations) != len(actions):
        fail("schema-7 candidate annotations are not aligned")
    _validate_next_five_visible({k: visible[k] for k in ("common", "prepared", "next_five")}, actor,
                               [r["drake_activation"] for r in annotations], actions)
    f = visible["graveyard_return"]
    fields = {"viewer", "meanings", "authorized", "current", "battlefield_counters", "casts", "triggers",
              "alesha_departures", "nine_lives_deaths", "nine_lives_schedules"}
    if type(f) is not dict or set(f) != fields or f["viewer"] != actor:
        fail("invalid GraveyardReturn actor or fields")
    if not _same_json_value(f["meanings"], _expected_graveyard_return_meanings()):
        fail("complete ordered GraveyardReturn printing changed")
    for key, spec in (
        ("authorized", ("list", {"family": FAMILY, "card": REF})),
        ("current", ("list", {"family": FAMILY, "card": REF, "printed_abilities_active": bool,
                               "power": ("optional", I32), "toughness": ("optional", I32)})),
        ("battlefield_counters", ("list", ("list", {"card": REF, "counters": GR_COUNTERS}))),
        ("casts", ("list", {"family": FAMILY, "position": U32, "source": REF, "original_controller": SEAT,
            "is_uncast_copy": bool, "payment_pending": bool, "genuine_cast_finalized": bool})),
        ("triggers", ("list", TRIGGER)), ("alesha_departures", ("list", {"occurrence": U32, "source": ROOT, "power": I32})),
        ("nine_lives_deaths", ("list", DEATH)), ("nine_lives_schedules", ("list", SCHEDULE))):
        _shape(f[key], spec, key)
    full = {row["source"]["handle"]: row for rows in visible["prepared"]["drake"]["battlefield"] for row in rows}
    if len(f["battlefield_counters"]) != 2:
        fail("missing two-seat full counter roster")
    for seat, rows, inherited in zip(("p0", "p1"), f["battlefield_counters"], visible["prepared"]["drake"]["battlefield"]):
        if len(rows) != len(inherited):
            fail("incomplete GraveyardReturn counter roster")
        for row, old in zip(rows, inherited):
            if (not _same_json_value(row["card"], old["source"]) or row["card"]["controller"] != seat
                    or any(row["counters"][k] != old["counters"][k] for k in COUNTERS)):
                fail("GraveyardReturn counters differ from inherited battlefield")
    def source_root(r):
        old, current = r["original_source"], r["current_original_source"]
        if old["zone"] != "Battlefield" or old["controller"] != r["captured_controller"]:
            fail("changed original GraveyardReturn source/controller")
        if current is not None and (current["handle"] not in full or current["owner"] != old["owner"]
                                   or not _same_json_value(current, full[current["handle"]]["source"])):
            fail("changed current GraveyardReturn incarnation")
    def target(r):
        old, current = r["original"], r["current_exact_graveyard_card"]
        if old["zone"] != "Graveyard" or (current is not None and not _same_json_value(old, current)) or (r["legal_now"] and current is None):
            fail("changed original graveyard target")
    def death(r):
        source_root(r["source"])
        if r["departure_revival"] < 1 or r["return_base_before_replacements"] != r["departure_revival"] - 1:
            fail("Nine-Lives departure subtraction changed")
        current = r["exact_graveyard_card_now"]
        if current is not None and (current["zone"] != "Graveyard" or current["owner"] != r["source"]["original_source"]["owner"]):
            fail("Nine-Lives exact graveyard binding changed")
        if "next_future_any_player_end_step" in r and not r["next_future_any_player_end_step"]:
            fail("Nine-Lives future schedule changed")
    def trigger(r):
        source_root(r["source"])
        tag, program = next(iter(r["program"].items()))
        if (r["family"] == "Alesha") != tag.startswith("Alesha"):
            fail("GraveyardReturn family/program mismatch")
        for t in r["targets"]:
            target(t)
        if tag == "AleshaAttackCounter":
            if program["plus_one"] != 1 or r["targets"]:
                fail("Alesha attack program changed")
        elif tag == "AleshaRaid":
            if not program["may_return_at_resolution"] or len(r["targets"]) > 1:
                fail("Alesha optional return changed")
            if (program["power"]["source"] == "CurrentOriginalIncarnation") != (r["source"]["current_original_source"] is not None):
                fail("Alesha current/departure power distinction changed")
            choice = program["optional_choice"]
            if r["stage"] != "Resolving" and choice != "BeforeResolution":
                fail("Alesha choice precedes resolution")
        else:
            if r["targets"] or program["departure_revival"] < 1 or program["return_base_before_replacements"] != program["departure_revival"] - 1:
                fail("Nine-Lives return program changed")
    for r in f["current"]:
        old = full.get(r["card"]["handle"])
        if old is None or not _same_json_value(old["source"], r["card"]) or any(not _same_json_value(r[k], old["form"][k]) for k in ("power", "toughness", "printed_abilities_active")):
            fail("GraveyardReturn current form differs from native battlefield")
    for r in f["casts"]:
        if r["source"]["zone"] != "Stack" or r["source"]["controller"] != r["original_controller"] or r["genuine_cast_finalized"] != (not r["is_uncast_copy"] and not r["payment_pending"]):
            fail("GraveyardReturn actual cast stage changed")
    for r in f["alesha_departures"]:
        source_root(r["source"])
    for r in f["triggers"]:
        trigger(r)
    for r in f["nine_lives_deaths"] + f["nine_lives_schedules"]:
        death(r)
    for row in annotations:
        for key, facts_key in (("programs", "triggers"), ("nine_lives_deaths", "nine_lives_deaths"),
                               ("nine_lives_schedules", "nine_lives_schedules")):
            if not _same_json_value(row[key], f[facts_key]):
                fail("candidate context differs from complete GraveyardReturn facts")
        for r in row["programs"]:
            trigger(r)
        if row["selected_graveyard_target"] is not None:
            target(row["selected_graveyard_target"])
            if (not row["selected_graveyard_target"]["legal_now"]
                    or row["selected_graveyard_target"]["original"]["owner"] != actor):
                fail("candidate graveyard target is not currently selectable by actor")
        for r in row["nine_lives_deaths"] + row["nine_lives_schedules"]:
            death(r)
    _validate_references([visible, annotations])


class LimitedGraveyardReturnClientV7(LimitedClientV1):
    def __init__(self, command: Sequence[str], *, expected_card_db_hash: int,
                 timeout_s: float = 10.0) -> None:
        self.expected_card_db_hash = _uint(expected_card_db_hash, "card_db_hash")
        if not math.isfinite(timeout_s) or timeout_s <= 0:
            raise ValueError("timeout_s must be finite and positive")
        self._issued_state: dict[str, Any] | None = None
        self._authority_generation: int | None = None
        self._episode_id: int | None = None
        self._snapshots: set[int] = set()
        self._producer_identity: tuple[str, int, str] | None = None
        super().__init__(command, timeout_s=timeout_s)
        self.schema_version = 7

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 7):
            raise LimitedSessionError("unexpected Limited common protocol identity")
        if reply.get("request_id") != request["request_id"]:
            raise LimitedSessionError("response request_id mismatch")
        catalog = _uint(reply.get("card_db_hash"), "card_db_hash")
        if catalog != self.expected_card_db_hash:
            raise LimitedSessionError("unexpected catalog identity")
        for field, expected in (("priority_mode", "engine_windows_v1"),
                                ("combat_rules", "foundations_v1"),
                                ("mulligan_rules", "london_v1")):
            if reply.get(field) != expected:
                raise LimitedSessionError(f"unexpected {field}")
        if (type(reply.get("kernel_version")) is not str or not reply["kernel_version"]
                or _uint(reply.get("card_count"), "card_count") == 0
                or type(reply.get("producer_sha256")) is not str
                or len(reply["producer_sha256"]) != 64
                or any(c not in "0123456789abcdef" for c in reply["producer_sha256"])):
            raise LimitedSessionError("invalid GraveyardReturn producer metadata")
        identity = (reply["kernel_version"], reply["card_count"], reply["producer_sha256"])
        if self._producer_identity is not None and identity != self._producer_identity:
            raise LimitedSessionError("schema-7 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-7 reply fields")
        if kind == "error":
            if not isinstance(reply.get("code"), str) or not reply["code"]:
                raise LimitedSessionError("invalid error code")
            return kind
        if kind != "state":
            raise LimitedSessionError("unexpected state response")
        state = reply.get("state")
        if not isinstance(state, dict):
            raise LimitedSessionError("missing common state")
        episode = _uint(state.get("episode_id"), "episode_id")
        expected_episode = request.get("episode_id") if request["request_type"] == "reset" else self._episode_id
        if expected_episode is not None and episode != expected_episode:
            raise LimitedSessionError("response changed the owning episode")
        state_type = state.get("state_type")
        expected_state={"decision":{"state_type","episode_id","authority_generation","decision"},
                        "terminal":{"state_type","episode_id","outcome","classification","code","winner","policy_steps","physical_decisions"},
                        "unsupported":{"state_type","episode_id","code"}}
        if type(state_type) is not str or state_type not in expected_state or set(state) != expected_state[state_type]:
            raise LimitedSessionError("unexpected schema-7 state fields")
        if state_type == "decision":
            generation = _uint(state.get("authority_generation"), "authority_generation")
            if self._authority_generation is not None:
                if request["request_type"] in ("reset", "restore"):
                    if generation <= self._authority_generation:
                        raise LimitedSessionError("restore/reset did not advance owner generation")
                elif generation != self._authority_generation:
                    raise LimitedSessionError("owner generation changed outside reset/restore")
            decision = state.get("decision")
            if not isinstance(decision, dict) or decision.get("schema") != DECISION_SCHEMA:
                raise LimitedSessionError("unexpected common decision schema")
            if set(decision) != {"schema","actor","prompt_seq","state","london_mulligans","foundations_combat","actions","action_meanings"}:
                raise LimitedSessionError("unexpected schema-7 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_graveyard_return_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            _validate_play_context(decision)
            _validate_references([decision["state"], decision["action_meanings"], decision["foundations_combat"]])
            actions = decision.get("actions")
            if not isinstance(actions, list) or not actions:
                raise LimitedSessionError("decision has no legal actions")
            for index, action in enumerate(actions):
                if (not isinstance(action, dict)
                        or set(action) != {"action_index","label"}
                        or type(action.get("action_index")) is not int
                        or action["action_index"] != index
                        or not isinstance(action.get("label"), str)
                        or not action["label"]):
                    raise LimitedSessionError("invalid common action identity")
        elif state_type == "terminal":
            terminal_codes = {"natural": "natural_game_over",
                              "truncated": "decision_cap", "halted": "fail_closed"}
            classification = state.get("classification")
            if (not isinstance(classification, str) or classification not in terminal_codes
                    or state.get("code") != terminal_codes[classification]):
                raise LimitedSessionError("inconsistent terminal classification/code")
            if state.get("winner") not in (None, "p0", "p1"):
                raise LimitedSessionError("invalid terminal winner")
            outcomes = {"natural": {"p0_win": "p0", "p1_win": "p1", "draw": None},
                        "truncated": {"truncated": None}, "halted": {"halted": None}}
            outcome = state.get("outcome")
            if (type(outcome) is not str or outcome not in outcomes[classification]
                    or state.get("winner") != outcomes[classification][outcome]):
                raise LimitedSessionError("inconsistent terminal outcome/winner")
            _uint(state.get("policy_steps"), "policy_steps")
            _uint(state.get("physical_decisions"), "physical_decisions")
        elif state_type == "unsupported":
            if not isinstance(state.get("code"), str) or not state["code"]:
                raise LimitedSessionError("invalid unsupported code")
        else:
            raise LimitedSessionError("unknown common state type")
        receipt = reply.get("receipt")
        if request["request_type"] == "step":
            if (type(receipt) is not dict or set(receipt) != {"prompt_seq", "action_index"}
                    or not _same_json_value(receipt["prompt_seq"], request["prompt_seq"])
                    or not _same_json_value(receipt["action_index"], request["action_index"])):
                raise LimitedSessionError("receipt differs from issued action")
        elif receipt is not None:
            raise LimitedSessionError("unexpected action receipt")
        if request["request_type"] != "snapshot" and "snapshot_id" in reply:
            raise LimitedSessionError("unsolicited snapshot handle")
        if request["request_type"] == "snapshot":
            if _uint(reply.get("snapshot_id"), "snapshot_id") == 0:
                raise LimitedSessionError("invalid snapshot handle")
        return kind

    def _server_error(self, reply: dict[str, Any]) -> str:
        return reply["code"]

    def exchange(self, payload: dict[str, Any]) -> dict[str, Any]:
        reply = super().exchange(payload)
        self._issued_state = deepcopy(reply["state"])
        self._episode_id = reply["state"]["episode_id"]
        if reply["state"].get("state_type") == "decision":
            self._authority_generation = reply["state"]["authority_generation"]
        if payload["request_type"] == "reset":
            self._snapshots.clear()
        if payload["request_type"] == "snapshot":
            self._snapshots.add(reply["snapshot_id"])
        return reply

    def reset(self, decks: tuple[ImportedDeck, ImportedDeck], *, episode_id: int = 0,
              env_seed: int = 1, starting_player: str = "p0",
              max_steps: int = 4096) -> dict[str, Any]:
        if len(decks) != 2 or starting_player not in ("p0", "p1"):
            raise LimitedSessionError("provide two decks and a valid starting seat")
        _uint(episode_id, "episode_id")
        _uint(env_seed, "env_seed")
        if _uint(max_steps, "max_steps") == 0:
            raise LimitedSessionError("max_steps must be positive")
        return self.exchange({
            "request_type": "reset", "expected_card_db_hash": self.expected_card_db_hash,
            "episode_id": episode_id, "env_seed": env_seed,
            "starting_player": starting_player,
            "max_physical_decisions": max_steps, "max_policy_steps": max_steps,
            "decks": [{"cards": [{"name": row.name, "count": row.count}
                                 for row in deck.mainboard]} for deck in decks],
        })

    def current(self) -> dict[str, Any]:
        return self.exchange({"request_type": "current"})

    def step(self, reply: dict[str, Any], action_index: int) -> dict[str, Any]:
        state = self._issued_state
        if (state is None or state.get("state_type") != "decision"
                or type(reply) is not dict or not _same_json_value(reply.get("state"), state)):
            raise LimitedSessionError("stale or unavailable common prompt")
        actions = state["decision"]["actions"]
        if type(action_index) is not int or not 0 <= action_index < len(actions):
            raise LimitedSessionError("action index is outside the current menu")
        decision = state["decision"]
        return self.exchange({
            "request_type": "step", "episode_id": state["episode_id"],
            "authority_generation": state["authority_generation"],
            "actor": decision["actor"], "prompt_seq": decision["prompt_seq"],
            "action_index": action_index,
        })

    def snapshot(self) -> dict[str, Any]:
        return self.exchange({"request_type": "snapshot"})

    def restore(self, snapshot_id: int) -> dict[str, Any]:
        _uint(snapshot_id, "snapshot_id")
        if snapshot_id not in self._snapshots:
            raise LimitedSessionError("snapshot does not belong to the current game")
        return self.exchange({"request_type": "restore", "snapshot_id": snapshot_id})
