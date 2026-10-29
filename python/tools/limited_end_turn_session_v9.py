"""Closed schema-9 Time Stop host client. Parser mocks are not native qualification.
The current registry keeps Time Stop Partial. Full-only reset remains enforced.
"""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U8, U16, U32, I16, I32
from limited_next_five_session_v6 import _shape, _validate_play_context
from limited_graveyard_return_session_v7 import _validate_references
from limited_call_session_v8 import _validate_call_visible, CANDIDATE as INHERITED_CANDIDATE
PROTOCOL = "kernel_limited_end_turn_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-end-turn-decision/v9"
PHASE = ("enum", ("BeforeCleanup","Discard","Expiration","Priority","GameOver"))
STEP = ("enum", ("Untap","Upkeep","Draw","Main1","BeginCombat","DeclareAttackers","DeclareBlockers","CombatDamage","EndCombat","Main2","End","Cleanup"))
KIND = ("enum", ("spell","activated_ability","triggered_ability","madness_offer"))
METHOD = ("enum", ("normal","alternative","flashback","madness","plotted","escape","bestow","omen"))
TARGET = ("tagged", "target_kind", {
    "player":{"player":SEAT}, "object":{"object":REF},
    "stack_item":{"stack_handle":str,"stack_index":("optional",U32),"source":REF,"controller":SEAT,"kind":KIND}
})
STACK = {"stack_index":U32,"source":REF,"controller":SEAT,"targets":("list",TARGET),
    "stack_item_kind":KIND,"is_copy":bool,"is_flashback":bool,"mode_chosen":U8,
    "madness_offer":bool,"kicked":bool,"cast_method":("optional",METHOD),"face_index":U8,
    "x_value":U16,"paid_cost_refs":("list",REF)}
COUNTERS = {"plus1_plus1":I32,"minus1_minus1":I16,"minus0_minus1":I16,"stun":I16,
    "lore":I16,"oil":I16,"charge":I32,"net":I32}
STACK_EXTRA = {
    "counter_distribution":{"total":U32,"targets":("list",TARGET),"amounts":("list",U32)},
    "counter_transfer":{"counters":COUNTERS,"extras":{"loyalty":U32,"lifelink":I16,"time":U8,"finality":U16}},
    "granted_ability":("pair",REF,U16)
}
CLEANUP={"disposal_occurrence":U32,"turn":U32,"active_player":SEAT,"phase":PHASE,"discard_count":("optional",U32)}
COMPATIBILITY_TARGET=("variant",{
    "DrakeCombat":{"occurrence":U32},"DrakeActivation":{"occurrence":U32},
    "RavenousActivation":{"occurrence":U32},"AjaniActivation":{"position":U32},
    "FishingOperation":{"position":U32},"FaebloomSpell":{"ordinal":U32},"FaebloomReflexive":{"ordinal":U32}
})
TERMINAL={"target":COMPATIBILITY_TARGET,"disposal_occurrence":U32,"original_stack_position":U32,"outcome":("enum",("Disposed",))}

RULES=["PayPrintedManaAndAllowResponses","InstantTimingRequiresNativeCastPermissionAndPriority",
    "CounteredSpellDoesNotEndTheTurn","GenuineVirtualSpellCopiesUseTheSameEndTurnProgram",
    "CeaseEarlierUnpublishedTriggers","ExileEveryOriginalStackObjectIncludingResolvingSpell",
    "CheckStateBasedActionsWithoutPriorityOrTriggerPlacement","EndCurrentStepRemoveCombatAndSkipToCleanup",
    "DiscardToCurrentMaximumHandSizeBeforeExpiration","RemoveDamageAndExpireCleanupDurations",
    "CheckStateBasedActionsThenPlaceNewTriggersAndGrantCleanupPriority","RepeatEntireCleanupAfterCompletedPriorityWindow"]

def _expected_end_turn_meaning(catalog,producer):
    return dict(schema="rules-meaning/time-stop/v68",producer_sha256=producer,card_db_hash=catalog,card_id=486,
        printed=dict(generic_mana=4,colored_mana=["U","U"],mana_value=6,instant=True,targets=0,x_count=0,types=["Instant"],subtypes=[],colors=["U"],legendary=False,power=None,toughness=None,keyword_bits=0),ordered_rules=RULES.copy(),
        full_engine_admitted=False,owning_consumer_admitted=False,model_admitted=False,sampler_admitted=False)

def _validate_end_turn_visible(visible,actor,annotations,actions):
    def fail(message): raise LimitedSessionError(message)
    if type(visible)is not dict or set(visible)!={"common","prepared","next_five","graveyard_return","call","end_turn"}:
        fail("missing closed Time Stop visible state")
    if type(annotations)is not list or type(actions)is not list or len(annotations)!=len(actions):
        fail("Time Stop candidate alignment changed")
    extra={"end_turn_cleanup","disposed_stack_outcomes","compatibility_terminals"}
    for row in annotations:
        if type(row)is not dict or set(row)!=set(INHERITED_CANDIDATE)|extra:
            fail("Time Stop annotation fields changed")
    _validate_call_visible({k:v for k,v in visible.items()if k!="end_turn"},actor,
        [{k:v for k,v in row.items()if k not in extra}for row in annotations],actions)
    f=visible["end_turn"]
    if type(f)is not dict or set(f)!={"viewer","meaning","visible_printings","disposals","cleanup","compatibility_terminals"}or f["viewer"]!=actor:
        fail("Time Stop viewer/fields changed")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):
        fail("Time Stop producer changed")
    _uint(m.get("card_db_hash"),"Time Stop catalog")
    if not _same_json_value(m,_expected_end_turn_meaning(m["card_db_hash"],m["producer_sha256"])):
        fail("Time Stop complete printed program changed")
    _shape(f["visible_printings"],("list",REF),"Time Stop visible printings")
    if any(v["name"]!="Time Stop"for v in f["visible_printings"]):fail("Time Stop visible printing changed")
    if type(f["disposals"])is not list:fail("Time Stop disposals changed")
    for occurrence,d in enumerate(f["disposals"]):
        if type(d)is not dict or set(d)!={"occurrence","ending_spell_position","original_step","original_active_player","disposed_rows"}:
            fail("Time Stop disposal fields changed")
        _shape(d["occurrence"],U32,"disposal occurrence");_shape(d["ending_spell_position"],U32,"ending position")
        _shape(d["original_step"],STEP,"original step");_shape(d["original_active_player"],SEAT,"original active player")
        rows=d["disposed_rows"]
        if d["occurrence"]!=occurrence or type(rows)is not list or not rows or d["ending_spell_position"]!=len(rows)-1:
            fail("Time Stop original stack order changed")
        for position,row in enumerate(rows):
            if type(row)is not dict or set(row)!={"original_stack_position","original"}:fail("disposed row fields changed")
            _shape(row["original_stack_position"],U32,"original stack position")
            v=row["original"]
            if type(v)is not dict or not set(STACK)<=set(v) or set(v)-set(STACK)-set(STACK_EXTRA):fail("disposed stack row incomplete")
            _shape({k:v[k]for k in STACK},STACK,"disposed stack row")
            for k in set(v)&set(STACK_EXTRA):_shape(v[k],STACK_EXTRA[k],"disposed "+k)
            if row["original_stack_position"]!=position or v["stack_index"]!=position:fail("disposed original position changed")
            if (v["stack_item_kind"]=="spell")!=(v["cast_method"]is not None):fail("disposed spell cast contract changed")
            if "counter_distribution"in v:
                c=v["counter_distribution"]
                if len(c["targets"])!=len(c["amounts"])or sum(c["amounts"])!=c["total"]:fail("disposed counter allocation changed")
        ending=rows[-1]["original"]
        if ending["stack_item_kind"]!="spell"or ending["source"]["name"]!="Time Stop":fail("ending spell changed")
    _shape(f["compatibility_terminals"],("list",TERMINAL),"Time Stop compatibility terminals")
    seen=set()
    for t in f["compatibility_terminals"]:
        tag,args=next(iter(t["target"].items()));position=next(iter(args.values()));key=(tag,position)
        if key in seen or t["disposal_occurrence"]>=len(f["disposals"]):fail("duplicate/missing Time Stop terminal target")
        seen.add(key)
        if t["original_stack_position"]>=len(f["disposals"][t["disposal_occurrence"]]["disposed_rows"]):fail("Time Stop terminal lost original row")
        original=f["disposals"][t["disposal_occurrence"]]["disposed_rows"][t["original_stack_position"]]["original"]
        try:
            if tag in ("DrakeCombat","DrakeActivation"):
                row=visible["prepared"]["drake"]["combat"if tag=="DrakeCombat"else"activations"][position]
                source=row["original_source"];status=row["outcome"];expected="Published"
            else:
                ext=visible["common"]["predecessor"]["extensions"]
                if tag=="RavenousActivation":row=ext["ravenous"]["activations"][position];source=row["original_source"];status=row["outcome"];expected="Published"
                elif tag=="AjaniActivation":row=ext["ajani"]["activations"][position];source=row["source_at_payment"];status=row["stage"];expected="OnStack"
                elif tag=="FishingOperation":
                    families=[v for v in ext["families"]if v["family"]=="Fishing"]
                    if len(families)!=1:fail("Time Stop Fishing family changed")
                    row=families[0]["facts"]["operations"][position];source=row["original_pole"];status=row["stage"];expected="OnStack"
                    if row["stack_ordinal"]is not None:fail("disposed Fishing row is live")
                else:
                    row=ext["faebloom"]["spells"if tag=="FaebloomSpell"else"reflexives"][position]
                    source=row["original_source"];status=row["stage"];expected="OnStack"
                    if row["stack_ordinal"]is not None:fail("disposed Faebloom row is live")
        except (KeyError,IndexError,TypeError):fail("Time Stop compatibility role is absent")
        if status!=expected or not _same_json_value(source,original["source"])or source["controller"]!=original["controller"]:
            fail("Time Stop compatibility terminal lost original public source or acquired another outcome")

    cleanup=f["cleanup"]
    if cleanup is not None:
        _shape(cleanup,CLEANUP,"Time Stop cleanup")
        if cleanup["disposal_occurrence"]>=len(f["disposals"])or (cleanup["phase"]=="Discard")!=(cleanup["discard_count"]is not None)or cleanup["discard_count"]==0:
            fail("Time Stop cleanup occurrence/obligation changed")
    for row in annotations:
        if not _same_json_value(row["end_turn_cleanup"],cleanup)or not _same_json_value(row["disposed_stack_outcomes"],f["disposals"])or not _same_json_value(row["compatibility_terminals"],f["compatibility_terminals"]):
            fail("Time Stop executable annotation differs from complete public facts")
    _validate_references([visible,annotations])

class LimitedEndTurnClientV9(LimitedClientV1):
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
        self.schema_version = 9

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 9):
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
            raise LimitedSessionError("schema-9 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-9 reply fields")
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
            raise LimitedSessionError("unexpected schema-9 state fields")
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
                raise LimitedSessionError("unexpected schema-9 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_end_turn_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            if decision["state"]["call"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Call meaning catalog differs from process header")
            if decision["state"]["end_turn"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Time Stop meaning catalog differs from header")
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
