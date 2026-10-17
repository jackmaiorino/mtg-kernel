"""Closed V16 Kaito transport. Parser mocks are not native qualification."""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1,LimitedSessionError,_uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF,SEAT,U32
from limited_next_five_session_v6 import _shape,_validate_play_context
from limited_graveyard_return_session_v7 import _validate_references
from limited_tinybones_session_v13 import RELATION,CURRENT,METHOD
from limited_liliana_session_v14 import LILIANA_ROOT
from limited_chandra_session_v15 import _validate_chandra_visible
from limited_vivien_session_v11 import COST
PROTOCOL="kernel_limited_kaito_jsonl"
DECISION_SCHEMA="mtg-kernel-limited-kaito-decision/v16"
STAGE=("enum",("Captured","Held","Published","Resolving","WaitingDiscard","Resolved","Countered","Fizzled","Disposed"))
DRAW=("enum",("NotReached","Drawn","EmptyLibrary"))
TARGET={"original":RELATION,"current_original":("optional",REF),"legal_now":bool}
DISCARD={"count":U32,"answered":bool,"private_choices":("optional",("list",REF)),"private_chosen":("optional",("list",RELATION))}
EVASION={"target":("optional",RELATION),"active_on_original":bool}
ACTIVATION={"occurrence":U32,"source":LILIANA_ROOT,"original_turn":U32,"ability_index":("int",0,2),"loyalty_before":U32,"loyalty_after":U32,"targets":("list",TARGET),"stage":STAGE,"draw":DRAW,"discard":("optional",DISCARD),"evasion":("optional",EVASION)}
COMBAT={"occurrence":U32,"observer":LILIANA_ROOT,"original_creature":RELATION,"current_original_creature":("optional",REF),"damaged_player":SEAT,"final_damage":("int",1,(1<<31)-1)}
EMBLEM={"occurrence":U32,"activation":U32,"controller":SEAT}
CAST={"occurrence":U32,"emblem":U32,"actual_cast":U32,"original_spell":RELATION,"current_original_spell":("optional",REF),"x_value":("int",0,65535),"kicked":bool,"cast_method":METHOD}
TRIGGER={"occurrence":U32,"origin":("variant",{"Combat":U32,"EmblemCast":U32}),"source":LILIANA_ROOT,"stage":STAGE,"native_position":("optional",U32)}
NINJA={"original":REF,"current_original":("optional",REF)}
BATCH={"occurrence":U32,"publication":("variant",{"Activation":U32,"Trigger":U32}),"controller":SEAT,"created":("list",NINJA)}
ACTION={"activation":("optional",COST),"activations":("list",ACTIVATION),"triggers":("list",TRIGGER),"selected_target":("optional",REF),"selected_discard":("optional",("list",REF))}
RULES=['CastPermanentForOneUUWithOrdinaryTimingAndResponses', 'LoyaltyAtSorceryTimingOncePerTurnOnThisPermanent', 'PayLoyaltyBeforePublishingAndKeepCostsWhenCountered', 'PlusOneCostIsNotAnEffectCounterPlacement', 'PlusOneTargetsZeroOrOneCreatureYouControl', 'RecheckOriginalTargetIncarnationAndControlOnResolution', 'SoleIllegalTargetFizzlesTheEntireAbility', 'SelectedOriginalCreatureCannotBeBlockedThisTurnAsARuleEffect', 'ThenDrawOneAndDiscardOneWithControllersPrivateChoice', 'EmptyLibraryDrawIsAttemptedAndOrdinaryLossRulesStillApply', 'MinusTwoCreatesOneTwoOneBlueNinjaWithNormalTokenReplacements', 'MinusNineCreatesASeparatePersistentEmblemForCapturedController', 'CaptureEveryFinalPositiveControlledCreatureCombatDamageToAPlayerBeforeTheNativeWave', 'PublishOneMandatoryLoyaltyTriggerPerObserverAndActualDamageOccurrence', 'SourceDepartureOrControlChangeDoesNotRewriteCapturedController', 'LoyaltyTriggerOnlyAddsToOriginalSurvivingIncarnationWithCurrentReplacements', 'EachEmblemObservesActualCastsByAnyPlayerIncludingLaterCounteredSpells', 'SpellCopiesAreNotCastsAndDoNotCreateEmblemCastOccurrences', 'EachEmblemTriggerCreatesItsOwnNinjaBatchWithNormalReplacements', 'DisposedIsDistinctFromResolvedCounteredAndFizzled']

def _expected_kaito_meaning(catalog,producer):
    return dict(schema="rules-meaning/kaito-cunning-infiltrator/v75",producer_sha256=producer,card_db_hash=catalog,card_id=494,printed=dict(generic_mana=1,colored_mana=["U","U"],mana_value=3,types=["Planeswalker"],subtypes=["Kaito"],supertypes=["Legendary"],colors=["U"],starting_loyalty=3,loyalty_costs=[1,-2,-9],sorcery_speed_only=True),ninja=dict(card_id=495,colors=["U"],types=["Creature"],subtypes=["Ninja"],power=2,toughness=1,keyword_bits=0,activated_abilities=0),ordered_rules=RULES.copy(),full_engine_admitted=False,owning_consumer_admitted=False,model_admitted=False,sampler_admitted=False)

def _validate_kaito_visible(visible,actor,annotations,actions):
    def fail(message):raise LimitedSessionError(message)
    if type(visible)is not dict or "kaito"not in visible:fail("missing Kaito facts")
    if type(annotations)is not list or any(type(a)is not dict or "kaito"not in a for a in annotations):fail("missing Kaito action facts")
    _validate_chandra_visible({k:v for k,v in visible.items()if k!="kaito"},actor,[{k:v for k,v in a.items()if k!="kaito"}for a in annotations],actions)
    f=visible["kaito"];fields={"viewer","active_player","turn_key","meaning","authorized_printings","battlefield","activations","combats","emblems","casts","triggers","ninja_batches"}
    if type(f)is not dict or set(f)!=fields or f["viewer"]!=actor:fail("Kaito closed facts/viewer changed")
    _shape(f["active_player"],SEAT,"Kaito active player");_shape(f["turn_key"],U32,"Kaito turn")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Kaito producer changed")
    _uint(m.get("card_db_hash"),"Kaito catalog")
    if not _same_json_value(m,_expected_kaito_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Kaito printed meaning changed")
    for key,shape in(("authorized_printings",("list",REF)),("battlefield",("pair",("list",CURRENT),("list",CURRENT))),("activations",("list",ACTIVATION)),("combats",("list",COMBAT)),("emblems",("list",EMBLEM)),("casts",("list",CAST)),("triggers",("list",TRIGGER)),("ninja_batches",("list",BATCH))):_shape(f[key],shape,"Kaito "+key)
    if any(r["name"]not in("Kaito, Cunning Infiltrator","Ninja Token")for r in f["authorized_printings"]):fail("Kaito authorized printing changed")
    if not _same_json_value(f["battlefield"],visible["tinybones"]["battlefield"]):fail("Kaito complete native roster differs")
    def source(r):
        a,b=r["original_source"],r["original_source_relation"]
        if a["name"]!="Kaito, Cunning Infiltrator"or a["zone"]!="Battlefield"or any(a[k]!=b[k]for k in("owner","controller","zone"))or a["controller"]!=r["captured_controller"]:fail("Kaito original observer source changed")
    for i,a in enumerate(f["activations"]):
        source(a["source"]);index=a["ability_index"]
        if a["occurrence"]!=i or a["loyalty_after"]!=a["loyalty_before"]+(1,-2,-9)[index]or len(a["targets"])>int(index==0)or a["stage"]in("Captured","Held"):fail("Kaito paid activation changed")
        if index!=0 and(a["draw"]!="NotReached"or a["discard"]is not None or a["evasion"]is not None):fail("Kaito other root acquired loot/evasion")
        for t in a["targets"]:
            if t["original"]["zone"]!="Battlefield"or t["original"]["controller"]!=a["source"]["captured_controller"]or(t["legal_now"]and t["current_original"]is None):fail("Kaito original target changed")
        e=a["evasion"]
        if e is not None and(not _same_json_value(e["target"],a["targets"][0]["original"]if a["targets"]else None)or(e["active_on_original"]and e["target"]is None)):fail("Kaito original evasion changed")
        d=a["discard"]
        if d is not None:
            if d["count"]!=1 or a["draw"]!="Drawn"or(a["stage"]=="WaitingDiscard"and d["answered"]):fail("Kaito actual draw/discard changed")
            if d["private_choices"]is not None and(actor!=a["source"]["captured_controller"]or d["answered"]or any(c["zone"]!="Hand"or c["owner"]!=actor for c in d["private_choices"])):fail("Kaito issued discard privacy changed")
            if d["private_chosen"]is not None and(actor!=a["source"]["captured_controller"]or not d["answered"]or len(d["private_chosen"])!=1 or d["private_chosen"][0]["zone"]!="Hand"):fail("Kaito original chosen discard privacy changed")
    for i,c in enumerate(f["combats"]):
        source(c["observer"])
        if c["occurrence"]!=i or c["original_creature"]["zone"]!="Battlefield"or c["original_creature"]["controller"]!=c["observer"]["captured_controller"]:fail("Kaito original combat observer changed")
    for i,e in enumerate(f["emblems"]):
        if e["activation"]>=len(f["activations"]):fail("Kaito emblem activation absent")
        a=f["activations"][e["activation"]]
        if e["occurrence"]!=i or a["ability_index"]!=2 or a["stage"]!="Resolved"or e["controller"]!=a["source"]["captured_controller"]:fail("Kaito persistent emblem changed")
    actual={};observed=set()
    for i,c in enumerate(f["casts"]):
        if c["occurrence"]!=i or c["emblem"]>=len(f["emblems"])or c["original_spell"]["zone"]!="Stack"or(c["emblem"],c["actual_cast"])in observed:fail("Kaito actual cast role changed")
        observed.add((c["emblem"],c["actual_cast"]))
        value={k:c[k]for k in("original_spell","x_value","kicked","cast_method")}
        if c["actual_cast"]in actual:
            if not _same_json_value(value,actual[c["actual_cast"]]):fail("Kaito independent emblems disagree on actual cast")
        else:
            if c["actual_cast"]!=len(actual):fail("Kaito original cast order changed")
            actual[c["actual_cast"]]=value
    origins=set()
    def tagged(value,kinds):
        if type(value)is not dict or len(value)!=1 or next(iter(value))not in kinds:fail("Kaito closed native relation changed")
        key=next(iter(value));_shape(value[key],U32,"Kaito native relation ordinal");return key,value[key]
    for i,t in enumerate(f["triggers"]):
        kind,ordinal=tagged(t["origin"],("Combat","EmblemCast"));key=(kind,ordinal)
        if kind=="Combat":
            if ordinal>=len(f["combats"]):fail("Kaito combat trigger origin absent")
            expected=f["combats"][ordinal]["observer"]
        else:
            if ordinal>=len(f["casts"]):fail("Kaito emblem trigger origin absent")
            expected=f["activations"][f["emblems"][f["casts"][ordinal]["emblem"]]["activation"]]["source"]
        if key in origins or t["occurrence"]!=i or not _same_json_value(t["source"],expected)or t["stage"]=="WaitingDiscard"or(t["native_position"]is not None)!=(t["stage"]in("Held","Published")):fail("Kaito trigger stage/source binding changed")
        origins.add(key)
    if len(origins)!=len(f["combats"])+len(f["casts"]):fail("Kaito native trigger inventory incomplete")
    batches=set()
    for i,b in enumerate(f["ninja_batches"]):
        kind,ordinal=tagged(b["publication"],("Activation","Trigger"));key=(kind,ordinal)
        if kind=="Activation":
            if ordinal>=len(f["activations"]):fail("Kaito Ninja activation absent")
            a=f["activations"][ordinal]
            if a["ability_index"]!=1 or a["stage"]!="Resolved":fail("Kaito Ninja activation unresolved")
            controller=a["source"]["captured_controller"]
        else:
            if ordinal>=len(f["triggers"]):fail("Kaito Ninja trigger absent")
            t=f["triggers"][ordinal]
            if "EmblemCast"not in t["origin"]or t["stage"]!="Resolved":fail("Kaito Ninja trigger unresolved")
            controller=t["source"]["captured_controller"]
        if key in batches or b["occurrence"]!=i or b["controller"]!=controller or not b["created"]:fail("Kaito actual Ninja batch changed")
        batches.add(key)
        for n in b["created"]:
            if n["original"]["name"]!="Ninja Token"or n["original"]["zone"]!="Battlefield"or n["original"]["owner"]!=controller or n["original"]["controller"]!=controller:fail("Kaito exact original Ninja changed")
    for a in f["activations"]:
        if a["stage"]=="Resolved"and a["ability_index"]==1 and("Activation",a["occurrence"])not in batches:fail("Kaito resolved activation lost actual Ninja batch")
        if sum(e["activation"]==a["occurrence"]for e in f["emblems"])!=int(a["stage"]=="Resolved"and a["ability_index"]==2):fail("Kaito actual emblem inventory changed")
    for t in f["triggers"]:
        if t["stage"]=="Resolved"and "EmblemCast"in t["origin"]and("Trigger",t["occurrence"])not in batches:fail("Kaito resolved emblem trigger lost actual Ninja batch")
    for row in annotations:
        a=row["kaito"];_shape(a,ACTION,"Kaito action meaning")
        if not _same_json_value(a["activations"],f["activations"])or not _same_json_value(a["triggers"],f["triggers"]):fail("Kaito actual action context changed")
        q=a["activation"]
        if q is not None and(q["source"]["name"]!="Kaito, Cunning Infiltrator"or q["source"]["zone"]!="Battlefield"or q["after"]!=q["before"]+(1,-2,-9)[q["ability_index"]]or not q["paid_before_publication"]or q["countering_refunds_cost"]):fail("Kaito exact loyalty cost changed")
        if a["selected_target"]is not None and(a["selected_target"]["zone"]!="Battlefield"or a["selected_target"]["controller"]!=actor):fail("Kaito selected target changed")
        if a["selected_discard"]is not None:
            choices=[c for x in f["activations"]if x["stage"]=="WaitingDiscard"and x["discard"]is not None for c in(x["discard"]["private_choices"]or[])]
            if len(a["selected_discard"])!=1 or a["selected_discard"][0]not in choices:fail("Kaito selected private discard absent")
    _validate_references([visible,annotations])

class LimitedKaitoClientV16(LimitedClientV1):
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
        self.schema_version = 16

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 16):
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
            raise LimitedSessionError("schema-15 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-15 reply fields")
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
            raise LimitedSessionError("unexpected schema-15 state fields")
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
                raise LimitedSessionError("unexpected schema-15 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_kaito_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
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
            if decision["state"]["liliana"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Liliana catalog differs from header")
            if decision["state"]["chandra"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Chandra catalog differs from header")
            if decision["state"]["kaito"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Kaito catalog differs from header")
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
