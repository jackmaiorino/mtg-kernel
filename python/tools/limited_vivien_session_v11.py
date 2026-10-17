"""Closed schema-11 Vivien host client. Parser checks are not native qualification."""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U32, ROOT
from limited_next_five_session_v6 import _shape, _validate_play_context
from limited_graveyard_return_session_v7 import _validate_references
from limited_call_session_v8 import CURRENT
from limited_genesis_wave_session_v10 import _validate_genesis_wave_visible
PROTOCOL="kernel_limited_vivien_jsonl"
DECISION_SCHEMA="mtg-kernel-limited-vivien-decision/v11"
U64=("int",0,(1<<64)-1)
STAGE=("enum",("Published","Looking","AnsweredLook","LookFinished","EmblemCreated","Disposed","NoLongerPublished"))
TARGET={"original":REF,"current_original":("optional",REF),"legal_now":bool}
ACTIVATION={"occurrence":U32,"source":ROOT,"ability_index":("int",0,2),"loyalty_before":U32,"loyalty_after":U32,"original_targets":("list",TARGET),"stage":STAGE}
EMBLEM={"occurrence":U32,"activation":U32,"controller":SEAT,"timestamp":U64}
COST={"source":REF,"ability_index":("int",0,2),"before":U32,"after":U32,"paid_before_publication":bool,"countering_refunds_cost":bool}
CONTEXT={"occurrence":U32,"ability_index":("int",0,2),"stage":STAGE}
RULES=['CastPermanentForThreeGGWithOrdinaryTimingAndResponses', 'LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent', 'PayLoyaltyBeforePublishingAndKeepCostsWhenCountered', 'PlusOneIsACostUnaffectedByDoublingSeason', 'LookAtUpToFourTopCardsPrivately', 'MayRevealOneCreatureOrLandCardAndPutItIntoYourHand', 'PutAllRemainingLookedAtCardsOnLibraryBottomInRandomOrder', 'MinusThreeTargetsArtifactOrEnchantmentOrCreatureWithFlying', 'RecheckTheOriginalTargetIncarnationAndLegalityOnResolution', 'DestroyThatTargetWithNormalIndestructibleAndReplacementRules', 'MinusEightCreatesAnEmblemForCapturedController', 'EmblemPersistsAfterSourceLeavesOrChangesController', 'CurrentCreaturesYouControlGetPlusTwoPlusTwo', 'CurrentCreaturesYouControlHaveVigilanceTrampleIndestructible', 'ApplyTimestampedLayerSixAbilityGrantsAndLayerSevenCounterAndModifierRules', 'DisposedIsDistinctFromResolvedOrCountered']

def _expected_vivien_meaning(catalog,producer):
    return dict(schema="rules-meaning/vivien-reid/v70",producer_sha256=producer,card_db_hash=catalog,card_id=488,printed=dict(generic_mana=3,colored_mana=["G","G"],mana_value=5,types=["Planeswalker"],subtypes=["Vivien"],supertypes=["Legendary"],colors=["G"],starting_loyalty=5),ordered_rules=RULES.copy(),full_engine_admitted=False,owning_consumer_admitted=False,model_admitted=False,sampler_admitted=False)
def _contexts(activations):
    return [{k:a[k] for k in ("occurrence","ability_index","stage")}for a in activations if a["stage"] in ("Looking","AnsweredLook")]
def _validate_vivien_visible(visible,actor,annotations,actions):
    def fail(message):raise LimitedSessionError(message)
    if type(visible)is not dict or set(visible)!={"common","prepared","next_five","graveyard_return","call","end_turn","genesis_wave","vivien"}:fail("missing closed Vivien visible state")
    if type(annotations)is not list or any(type(a)is not dict or "vivien_choices"not in a or "vivien_activation"not in a for a in annotations):fail("missing Vivien annotation")
    _validate_genesis_wave_visible({k:v for k,v in visible.items()if k!="vivien"},actor,[{k:v for k,v in a.items()if k not in ("vivien_choices","vivien_activation")}for a in annotations],actions)
    f=visible["vivien"]
    if type(f)is not dict or set(f)!={"viewer","meaning","visible_printings","current","activations","emblems"}or f["viewer"]!=actor:fail("Vivien fields/viewer changed")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Vivien producer changed")
    _uint(m.get("card_db_hash"),"Vivien catalog")
    if not _same_json_value(m,_expected_vivien_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Vivien printed meaning changed")
    for key,shape in (("visible_printings",REF),("current",CURRENT),("activations",ACTIVATION),("emblems",EMBLEM)):_shape(f[key],("list",shape),"Vivien "+key)
    if any(r["name"]!="Vivien Reid" for r in f["visible_printings"]):fail("Vivien printing changed")
    if any(c["card"]["name"]!="Vivien Reid"or c["card"]["zone"]!="Battlefield"for c in f["current"]):fail("Vivien current roster changed")
    for i,a in enumerate(f["activations"]):
        expected=a["loyalty_before"]+(1,-3,-8)[a["ability_index"]]
        if a["occurrence"]!=i or a["loyalty_after"]!=expected or a["source"]["original_source"]["name"]!="Vivien Reid"or a["source"]["original_source"]["zone"]!="Battlefield"or a["source"]["original_source"]["controller"]!=a["source"]["captured_controller"] or len(a["original_targets"])!=(a["ability_index"]==1):fail("Vivien paid binding changed")
        if a["stage"]in ("Looking","AnsweredLook","LookFinished")and a["ability_index"]!=0 or a["stage"]=="EmblemCreated"and a["ability_index"]!=2:fail("Vivien stage changed")
        for t in a["original_targets"]:
            if t["original"]["zone"]!="Battlefield"or t["legal_now"]and t["current_original"]is None:fail("Vivien target relation changed")
    seen=set();stamps=set()
    for i,e in enumerate(f["emblems"]):
        if e["occurrence"]!=i or e["activation"]>=len(f["activations"])or e["activation"]in seen or e["timestamp"]in stamps:fail("Vivien emblem changed")
        a=f["activations"][e["activation"]]
        if a["stage"]!="EmblemCreated"or e["controller"]!=a["source"]["captured_controller"]:fail("Vivien emblem source changed")
        seen.add(e["activation"]);stamps.add(e["timestamp"])
    if any(a["stage"]=="EmblemCreated"and a["occurrence"]not in seen for a in f["activations"]):fail("Vivien emblem omitted")
    contexts=_contexts(f["activations"])
    if len(contexts)>1:fail("multiple Vivien looks")
    for a in annotations:
        _shape(a["vivien_activation"],("optional",COST),"Vivien activation cost")
        cost=a["vivien_activation"]
        if cost is not None and(cost["source"]["name"]!="Vivien Reid"or cost["source"]["zone"]!="Battlefield"or cost["after"]!=cost["before"]+(1,-3,-8)[cost["ability_index"]]or not cost["paid_before_publication"]or cost["countering_refunds_cost"]):fail("Vivien activation cost changed")
        _shape(a["vivien_choices"],("list",CONTEXT),"Vivien action context")
        if not _same_json_value(a["vivien_choices"],contexts):fail("Vivien context omitted/changed")
    _validate_references([visible,annotations])

class LimitedVivienClientV11(LimitedClientV1):
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
        self.schema_version = 11

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 11):
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
            raise LimitedSessionError("schema-11 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-11 reply fields")
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
            raise LimitedSessionError("unexpected schema-11 state fields")
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
                raise LimitedSessionError("unexpected schema-11 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_vivien_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            if decision["state"]["call"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Call meaning catalog differs from process header")
            if decision["state"]["end_turn"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Time Stop meaning catalog differs from header")
            if decision["state"]["genesis_wave"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Genesis Wave meaning catalog differs from header")
            if decision["state"]["vivien"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Vivien catalog differs from header")
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
