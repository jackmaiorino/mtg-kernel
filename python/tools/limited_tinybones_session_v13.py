"""Closed V13 Tinybones transport. Parser mocks are not native qualification."""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U32, ROOT, I32
from limited_next_five_session_v6 import _shape, _validate_play_context
from limited_graveyard_return_session_v7 import _validate_references
from limited_call_session_v8 import CALL_COUNTERS, RELATION, FORM
from limited_muldrotha_session_v12 import _validate_muldrotha_visible
PROTOCOL="kernel_limited_tinybones_jsonl"
DECISION_SCHEMA="mtg-kernel-limited-tinybones-decision/v13"
COUNTERS={**CALL_COUNTERS,"stash":("int",0,(1<<31)-1)}
ZONE=RELATION["zone"]
CURRENT={"card":REF,"relation":RELATION,"is_token":bool,"tapped":bool,"summoning_sick":bool,"marked_damage":U32,"counters":COUNTERS,"form":FORM}
DISCARD={"occurrence":U32,"original_card":RELATION,"discarder":SEAT,"outcome_zone":ZONE,"current_original_outcome":("optional",REF),"observers":("list",ROOT)}
TRIGGER={"discard":U32,"observer":U32,"source":ROOT,"stage":("enum",("Held","Published","Resolving","Resolved","Countered","Disposed")),"native_position":("optional",U32),"stashed":("optional",RELATION)}
STASH={"card":REF,"counters":COUNTERS,"can_play":("pair",bool,bool),"any_type_mana":("pair",bool,bool),"current_grant_sources":("pair",("list",ROOT),("list",ROOT))}
METHOD=("enum",("normal","alternative","flashback","madness","plotted","escape","bestow","omen"))
PLAY={"occurrence":U32,"original_card":REF,"permission_source":ROOT,"actor":SEAT,"original_turn":U32,"stash":("int",1,(1<<31)-1),"land":bool,"stage":("enum",("Announced","Paid","LandPlayed","NativeAborted")),"announced_form":("optional",METHOD),"paid_form":("optional",METHOD),"paid_x":("optional",("int",0,65535)),"variable_targeting_kicked":("optional",bool)}
ACTIVATION={"native_position":U32,"source":ROOT,"generic_mana":("int",0,255),"colored_mana":("list",("enum",("W","U","B","R","G","C"))),"tap_cost":bool,"sorcery_only":bool}
ACTION={"triggers":("list",TRIGGER),"plays":("list",PLAY)}
RULES=['PayPrintedOneBlackAndEnterWithNormalCreatureTimingAndResponses', 'ObserveEveryActualOpponentDiscardBeforeItsReplacement', 'CaptureOriginalDiscardedCardAndEveryActiveFaceUpObserverController', 'PublishMandatoryUntargetedTriggerWithOriginalIncarnationsAndNativeApnap', 'OriginalGraveyardCardOnlyExilesWithOneStashCounter', 'MadnessAndOtherReplacementOrLeftReturnedCardDoNotBecomeOriginalGraveyardCard', 'SourceDepartureAndLaterControlDoNotChangeCapturedTrigger', 'CurrentActiveSourceGrantsOwnTurnPlayOfUnownedStashedExileCards', 'StashIssuerIsIndependentOfCurrentPermissionSource', 'PreserveNormalTimingLandQuotaOriginalCostsTargetsAdditionalCostsAndSpellForms', 'ActualCurrentSourceLetsManaMatchAnyTypeForForeignStashedCards', 'ManaMatchingIsDistinctFromOwnTurnPermissionAndPreservesConvokeAndPhyrexianLife', 'ActivateAtSorceryTimingByPayingThreeBlackAndTapWithOrdinarySickness', 'ActivationMakesEachOpponentDiscardOneCardWithTheirOwnChoice', 'SourceRemovalAndCounteringKeepAlreadyPaidCosts', 'NativeAnnouncementReversalPreservesOriginalStashAtReboundIncarnation', 'ForeignAdventureGrantsActualCasterItsPermanentFacePermission']

def _expected_tinybones_meaning(catalog,producer):
    return dict(schema="rules-meaning/tinybones-bauble-burglar/v72",producer_sha256=producer,card_db_hash=catalog,card_id=490,printed=dict(generic_mana=1,colored_mana=["B"],mana_value=2,colors=["B"],activation_generic_mana=3,activation_colored_mana=["B"],activation_tap=True,activation_sorcery_only=True,types=["Creature"],subtypes=["Skeleton","Rogue"],supertypes=["Legendary"],power=1,toughness=3),ordered_rules=RULES.copy(),full_engine_admitted=False,complete_consumer_admitted=False)

def _validate_tinybones_visible(visible,actor,annotations,actions):
    def fail(s):raise LimitedSessionError(s)
    if type(visible)is not dict or "tinybones"not in visible:fail("missing Tinybones facts")
    if type(annotations)is not list or any(type(a)is not dict or "tinybones"not in a for a in annotations):fail("missing Tinybones action facts")
    _validate_muldrotha_visible({k:v for k,v in visible.items()if k!="tinybones"},actor,[{k:v for k,v in a.items()if k!="tinybones"}for a in annotations],actions)
    f=visible["tinybones"]
    fields={"viewer","active_player","turn_key","meaning","authorized_printings","battlefield","discards","triggers","stash","plays","activations"}
    if type(f)is not dict or set(f)!=fields or f["viewer"]!=actor:fail("Tinybones closed facts/viewer changed")
    _shape(f["active_player"],SEAT,"Tinybones active player");_shape(f["turn_key"],U32,"Tinybones turn")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Tinybones producer changed")
    _uint(m.get("card_db_hash"),"Tinybones catalog")
    if not _same_json_value(m,_expected_tinybones_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Tinybones printed meaning changed")
    for key,shape in (("authorized_printings",("list",REF)),("battlefield",("pair",("list",CURRENT),("list",CURRENT))),("discards",("list",DISCARD)),("triggers",("list",TRIGGER)),("stash",("list",STASH)),("plays",("list",PLAY)),("activations",("list",ACTIVATION))):_shape(f[key],shape,"Tinybones "+key)
    if any(r["name"]!="Tinybones, Bauble Burglar"for r in f["authorized_printings"]):fail("Tinybones authorized printing changed")
    for seat,rows in zip(("p0","p1"),f["battlefield"]):
        if any(r["card"]["zone"]!="Battlefield"or r["card"]["controller"]!=seat or r["relation"]["zone"]!="Battlefield"or r["relation"]["controller"]!=seat or r["card"]["owner"]!=r["relation"]["owner"]for r in rows):fail("Tinybones battlefield membership changed")
    # The authoritative eighteenth-counter roster retains every inherited
    # visible battlefield member in native seat order and its exact old prefix.
    for rows,prior in zip(f["battlefield"],visible["call"]["battlefield"]):
        if len(rows)!=len(prior):fail("Tinybones battlefield prefix cardinality changed")
        for row,old in zip(rows,prior):
            for key in ("card","is_token","tapped","summoning_sick","marked_damage","form"):
                if not _same_json_value(row[key],old[key]):fail("Tinybones battlefield prefix changed")
            if not _same_json_value({k:v for k,v in row["counters"].items()if k!="stash"},old["counters"]):fail("Tinybones counter prefix changed")
    def source(r):
        if r["original_source"]["name"]!="Tinybones, Bauble Burglar"or r["original_source"]["zone"]!="Battlefield"or r["original_source"]["controller"]!=r["captured_controller"]:fail("Tinybones source/controller changed")
    for i,d in enumerate(f["discards"]):
        if d["occurrence"]!=i or d["original_card"]["zone"]!="Hand"or d["original_card"]["controller"]!=d["discarder"]:fail("Tinybones original discard changed")
        for r in d["observers"]:
            source(r)
            if r["captured_controller"]==d["discarder"]:fail("Tinybones opponent relation changed")
    for t in f["triggers"]:
        if t["discard"]>=len(f["discards"]):fail("Tinybones discard binding absent")
        d=f["discards"][t["discard"]]
        if t["observer"]>=len(d["observers"])or not _same_json_value(t["source"],d["observers"][t["observer"]]):fail("Tinybones observer binding changed")
        if (t["stage"]in("Held","Published"))!=(t["native_position"]is not None)or t["stashed"]is not None and t["stage"]!="Resolved":fail("Tinybones trigger position/outcome changed")
        if t["stashed"]is not None and (t["stashed"]["object_role"]!=d["original_card"]["object_role"]or t["stashed"]["owner"]!=d["original_card"]["owner"]or t["stashed"]["zone"]!="Exile"):fail("Tinybones stashed incarnation changed")
    for c in f["stash"]:
        if c["card"]["zone"]!="Exile"or c["counters"]["stash"]<=0:fail("Tinybones current stash changed")
        for i,seat in enumerate(("p0","p1")):
            for r in c["current_grant_sources"][i]:
                source(r)
                if r["captured_controller"]!=seat:fail("Tinybones current grant controller changed")
            if (c["can_play"][i]or c["any_type_mana"][i])and(c["card"]["owner"]==seat or not c["current_grant_sources"][i]):fail("Tinybones unowned/current grant changed")
            if c["can_play"][i]and seat!=f["active_player"]:fail("Tinybones own-turn permission changed")
    for i,p in enumerate(f["plays"]):
        source(p["permission_source"])
        if p["occurrence"]!=i or p["original_card"]["zone"]!="Exile"or p["original_card"]["owner"]==p["actor"]or p["permission_source"]["captured_controller"]!=p["actor"]:fail("Tinybones original play/source changed")
        if (p["stage"]=="Paid")!=(p["paid_x"]is not None)or (p["stage"]=="Paid")!=(p["paid_form"]is not None)or p["land"]!=(p["stage"]=="LandPlayed")or p["land"]==(p["announced_form"]is not None):fail("Tinybones paid/announced form changed")
    for a in f["activations"]:
        source(a["source"])
        if a["generic_mana"]!=3 or a["colored_mana"]!=["B"]or not a["tap_cost"]or not a["sorcery_only"]:fail("Tinybones printed activation changed")
    for a in annotations:
        _shape(a["tinybones"],ACTION,"Tinybones action facts")
        if not _same_json_value(a["tinybones"],dict(triggers=f["triggers"],plays=f["plays"])):fail("Tinybones action/native occurrence context changed")
    _validate_references([visible,annotations])

class LimitedTinybonesClientV13(LimitedClientV1):
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
        self.schema_version = 13

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 13):
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
            raise LimitedSessionError("schema-13 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-13 reply fields")
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
            raise LimitedSessionError("unexpected schema-13 state fields")
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
                raise LimitedSessionError("unexpected schema-13 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_tinybones_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            if decision["state"]["call"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Call meaning catalog differs from process header")
            if decision["state"]["end_turn"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Time Stop meaning catalog differs from header")
            if decision["state"]["genesis_wave"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Genesis Wave meaning catalog differs from header")
            if decision["state"]["vivien"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Vivien catalog differs from header")
            if decision["state"]["muldrotha"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Muldrotha catalog differs from header")
            if decision["state"]["tinybones"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Tinybones catalog differs from header")
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
