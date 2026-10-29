"""Closed schema8 Call host client. Partial reset remains unavailable.
Mock/parser checks are not native runtime or Full qualification evidence.
"""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U32, I32, ROOT, COUNTERS
from limited_next_five_session_v6 import _shape, _validate_play_context
from limited_graveyard_return_session_v7 import _validate_graveyard_return_visible, _validate_references, CANDIDATE as INHERITED_CANDIDATE
PROTOCOL = "kernel_limited_call_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-call-decision/v8"
CALL_COUNTERS = {**COUNTERS, "revival": ("int",0,(1<<31)-1), "fellowship": ("int",0,(1<<31)-1)}
RELATION = {"object_role":U32,"incarnation_role":U32,"owner":SEAT,"controller":SEAT,"zone":("enum",("Library","Hand","Battlefield","Graveyard","Stack","Exile","Command"))}
FORM = {"names":("list",str),"mana_value":("int",0,65535),"types":("list",("enum",("Creature","Artifact","Enchantment","Instant","Sorcery","Land","Planeswalker"))),"supertypes":("list",("enum",("Basic","Snow","Legendary"))),"subtype_ids":("list",("int",0,65535)),"color_mask":("int",0,255),"base_power":("optional",I32),"base_toughness":("optional",I32),"power":("optional",I32),"toughness":("optional",I32),"keyword_bits":U32,"ward_generic":("int",0,65535),"minimum_blockers":("int",0,(1<<64)-1),"printed_abilities_active":bool,"abilities_removed":bool}
CURRENT={"card":REF,"relation":RELATION,"is_token":bool,"tapped":bool,"summoning_sick":bool,"marked_damage":U32,"counters":CALL_COUNTERS,"form":FORM}
DEATH={"occurrence":U32,"source":ROOT,"departure":RELATION,"original_graveyard":RELATION,"collection_completed":bool,"current_exact_graveyard_card":("optional",REF)}
RETURN={"stage":("enum",("Held","Published","Resolving")),"position":U32,"source":ROOT,"death_occurrence":U32,"original_graveyard":RELATION,"current_exact_graveyard_card":("optional",REF),"return_eligible_now":bool,"under_physical_owner":SEAT}
CAST={"position":U32,"source":REF,"original_controller":SEAT,"is_uncast_copy":bool,"payment_pending":bool,"genuine_cast_finalized":bool}
CANDIDATE={**INHERITED_CANDIDATE,"call_returns":("list",RETURN)}
def _expected_call_program():
    def flags(*keys): return dict.fromkeys(keys,True)
    return ["PayPrintedMana",{"EnterUnderOriginalSpellController":{**flags("active_player_main_phase_empty_stack_with_priority","uncast_permanent_copy_enters_as_token","countered_spell_to_physical_owner_graveyard_subject_to_replacements"),"grants_additional_cast_permission":False}},
        {"OnControlledNontokenNonAngelCreatureDeath":flags("effective_creature_type_and_angel_subtype_before_departure","printed_ability_active_on_original_source_before_departure","source_need_not_be_a_different_object","captured_controller_not_physical_owner","capture_each_actual_simultaneous_death_before_collection","source_departure_or_control_change_does_not_cancel_trigger","mandatory_non_targeting_no_payment_or_optional_choice","capture_collected_once")},
        {"ReturnOriginalGraveyardIncarnation":flags("under_physical_card_owner","stale_incarnation_is_no_op","no_creature_or_controller_recheck_on_graveyard_card","return_is_not_cast","countered_trigger_does_not_return_or_retry")},
        {"InstallAtBattlefieldEntry":{**flags("current_entry_and_counter_replacements_apply_once","counters_before_entry_observers","preserves_other_types_and_subtypes","grants_flying","added_subtype_and_flying_bound_to_returned_incarnation","flying_uses_native_ability_addition_removal_layers"),"plus_one_before_replacements":1,"added_subtype":"Angel"}}]
def _expected_call_meaning(catalog,producer):
    return dict(schema="rules-meaning/valkyries-call/v62",producer_sha256=producer,card_db_hash=catalog,card_id=459,printed=dict(generic_mana=3,colored_mana=["W","W"],mana_value=5,types=["Enchantment"],subtypes=[],colors=["W"],legendary=False,power=None,toughness=None,keyword_bits=0),ordered_rules=_expected_call_program(),full_engine_admitted=False,owning_consumer_admitted=False,model_admitted=False,sampler_admitted=False)
def _validate_call_visible(visible,actor,annotations,actions):
    def fail(message): raise LimitedSessionError(message)
    if type(visible)is not dict or set(visible)!={"common","prepared","next_five","graveyard_return","call"}: fail("missing closed Call visible state")
    _shape(annotations,("list",CANDIDATE),"Call annotations")
    _validate_graveyard_return_visible({k:v for k,v in visible.items()if k!="call"},actor,[{k:v for k,v in r.items()if k!="call_returns"}for r in annotations],actions)
    f=visible["call"]
    fields={"viewer","meaning","visible_printings","battlefield","casts","deaths","returns"}
    if type(f)is not dict or set(f)!=fields or f["viewer"]!=actor: fail("Call viewer/fields changed")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]): fail("Call meaning producer changed")
    _uint(m.get("card_db_hash"),"Call catalog")
    if not _same_json_value(m,_expected_call_meaning(m["card_db_hash"],m["producer_sha256"])): fail("Call complete printed program changed")
    for key,grammar in (("visible_printings",("list",REF)),("battlefield",("list",("list",CURRENT))),("casts",("list",CAST)),("deaths",("list",DEATH)),("returns",("list",RETURN))): _shape(f[key],grammar,"Call "+key)
    if len(f["battlefield"])!=2: fail("Call battlefield seat count")
    relations={};incarnations={};seen=set();current_roles=set()
    def relation(r):
        obj=r["object_role"];inc=r["incarnation_role"]
        if incarnations.setdefault(inc,(obj,r["zone"]))!=(obj,r["zone"]): fail("Call incarnation changed object role/zone")
        if relations.setdefault(obj,r["owner"])!=r["owner"]: fail("Call physical owner changed")
    for seat,rows in enumerate(f["battlefield"]):
        inherited=visible["graveyard_return"]["battlefield_counters"][seat]
        if len(rows)!=len(inherited): fail("Call mandatory17 roster count")
        for r,prefix in zip(rows,inherited):
            if r["card"]!=prefix["card"]or {k:v for k,v in r["counters"].items()if k!="fellowship"}!=prefix["counters"]or r["relation"]["controller"]!=("p0","p1")[seat]: fail("Call mandatory17 roster binding")
            if r["card"]["handle"]in seen: fail("Call duplicate battlefield member")
            role=r["relation"]
            if role["object_role"]in current_roles or any(role[k]!=r["card"][k]for k in ("owner","controller","zone")): fail("Call current relation alias/metadata changed")
            current_roles.add(role["object_role"])
            seen.add(r["card"]["handle"]);relation(r["relation"])
    deaths={}
    for d in f["deaths"]:
        if d["occurrence"]in deaths: fail("Call duplicate death ordinal")
        deaths[d["occurrence"]]=d;relation(d["departure"]);relation(d["original_graveyard"])
        if d["departure"]["object_role"]!=d["original_graveyard"]["object_role"]: fail("Call death physical relation")
    for r in f["returns"]:
        relation(r["original_graveyard"]);d=deaths.get(r["death_occurrence"])
        if d is None or r["source"]!=d["source"]or r["original_graveyard"]!=d["original_graveyard"]or r["under_physical_owner"]!=r["original_graveyard"]["owner"]or r["return_eligible_now"]!=(r["current_exact_graveyard_card"]is not None): fail("Call original death/return binding")
    if any(not _same_json_value(r["call_returns"],f["returns"])for r in annotations): fail("Call candidate program binding")
    _validate_references([visible,annotations])


class LimitedCallClientV8(LimitedClientV1):
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
        self.schema_version = 8

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 8):
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
            raise LimitedSessionError("invalid Call producer metadata")
        identity = (reply["kernel_version"], reply["card_count"], reply["producer_sha256"])
        if self._producer_identity is not None and identity != self._producer_identity:
            raise LimitedSessionError("schema-8 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-8 reply fields")
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
            raise LimitedSessionError("unexpected schema-8 state fields")
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
                raise LimitedSessionError("unexpected schema-8 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_call_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            if decision["state"]["call"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Call meaning catalog differs from process header")
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
