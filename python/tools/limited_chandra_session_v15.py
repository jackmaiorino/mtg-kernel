"""Closed V15 Chandra transport. Parser mocks are not native qualification."""
from __future__ import annotations
from copy import deepcopy
import math
from typing import Any, Sequence
from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import REF, SEAT, U32
from limited_next_five_session_v6 import _shape, _validate_play_context
from limited_graveyard_return_session_v7 import _validate_references
from limited_tinybones_session_v13 import RELATION, CURRENT
from limited_liliana_session_v14 import _validate_liliana_visible,LILIANA_ROOT
from limited_vivien_session_v11 import COST
PROTOCOL="kernel_limited_chandra_jsonl"
DECISION_SCHEMA="mtg-kernel-limited-chandra-decision/v15"
U8=("int",0,255)
U16=("int",0,65535)
I32=("int",-(1<<31),(1<<31)-1)
U64=("int",0,(1<<64)-1)
TARGET={"original":RELATION,"current_original":("optional",REF),"allocated_damage":("optional",("int",1,8)),"legal_now":bool}
ALLOCATION={"source":LILIANA_ROOT,"loyalty_before":U32,"targets":("list",TARGET),"remaining_units":("int",0,8),"finished":bool,"can_finish_zero":bool,"candidates":("list",REF)}
DAMAGE_SOURCE={"source":LILIANA_ROOT,"historical":bool,"color_mask":U8,"lifelink":bool,"deathtouch":bool,"werewolf":bool}
STAGE=("enum",("Paid","Published","Resolving","NativeCompleted","Countered","Disposed","NotResolved"))
ACTIVATION={"occurrence":U32,"source":LILIANA_ROOT,"ability_index":("int",0,2),"loyalty_before":U32,"loyalty_after":U32,"targets":("list",TARGET),"stage":STAGE,"damage_source":("optional",DAMAGE_SOURCE)}
EXILED={"original":REF,"current_original":("optional",REF)}
EXILE={"activation":U32,"cards":("list",EXILED),"choice_current":bool,"selected_prefix":("list",U16),"chosen":("optional",U16),"grant_for_land":("optional",bool),"permission_present":bool,"expired":bool}
PREVENTION={"menu_index":U16,"kind":("enum",("Multiplier","Barricade","Protection","Shield","Color")),"source":("optional",REF),"shield_remaining":("optional",I32)}
DAMAGE={"activation":U32,"chooser":SEAT,"recipient":REF,"original_amount":I32,"remaining_amount":I32,"simultaneous_waiting":U64,"options":("list",PREVENTION)}
COPIED={"occurrence":U32,"source":LILIANA_ROOT,"sacrifice_instances":U32,"current_haste":("optional",bool)}
OBSERVER={"token":U32,"source":LILIANA_ROOT,"instances":U32}
END_STEP={"occurrence":U32,"observers":("list",OBSERVER),"collected":bool,"published_count":U32,"finished_count":U32}
ACTION={"selected_damage_option":("optional",U16),"selected_allocation_target":("optional",REF),"selected_exile":("optional",U16),"copy_target":("optional",REF),"activation":("optional",COST),"allocation":("optional",ALLOCATION),"exiles":("list",EXILE),"damage":("optional",DAMAGE)}
RULES=['PayFiveRedRedWithOrdinaryPlaneswalkerTimingAndResponses', 'EnterWithSixLoyaltySubjectToActualCounterReplacements', 'ActivateAtSorceryTimingWithOrdinaryOncePerTurnLoyaltyRestriction', 'PayOriginalPlusTwoPlusOneOrMinusFourLoyaltyAsCostWithoutDoublingSeason', 'PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange', 'PlusTwoAddsThreeRedManaAndPubliclyExilesTopThreeCardsOrAllIfFewer', 'ChooseExactlyOneOfThosePublicExiledCardsWhenAnyExist', 'ChosenOriginalExileCardMayBePlayedThisTurnWithOrdinaryCostsTimingAndLandQuota', 'GrantNeverExtendsToOtherExiledCardsOrNewIncarnationsAndExpiresAtCleanup', 'PlusOneTargetsOneCreatureYouControlAndCopiesItsCopiableValues', 'CopyHasCopiableHasteAndOneAdditionalCopiableBeginningOfEndStepSacrificeAbility', 'CopyOfCopyRetainsExistingSacrificeAbilityMultiplicity', 'TokenCreationUsesActualReplacementsIncludingDoublingSeason', 'MinusFourChoosesZeroThroughEightDistinctCreatureOrPlaneswalkerTargetsBeforePayment', 'EachChosenTargetReceivesAtLeastOneOfEightOneUnitAllocationsBeforePayment', 'OriginalTargetsAndDivisionSurviveIndividualTargetLossWithoutRedistribution', 'ZeroTargetsIsLegalAndDealsNoDamage', 'SourceDepartureKeepsOriginalDamageIncarnationAndNativeLastKnownDamageFacts', 'ActualAffectedPlayersChooseCrystalReplacementOrderingForOriginalSimultaneousBatch', 'CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution']

def _expected_chandra_meaning(catalog,producer):
    return dict(schema="rules-meaning/chandra-flameshaper/v74",producer_sha256=producer,card_db_hash=catalog,card_id=493,printed=dict(generic_mana=5,colored_mana=["R","R"],mana_value=7,colors=["R"],types=["Planeswalker"],subtypes=["Chandra"],supertypes=["Legendary"],starting_loyalty=6,loyalty_costs=[2,1,-4],loyalty_sorcery_only=True,plus_two_red_mana=3,plus_two_exile_count=3,plus_two_choose_count=1,minus_four_damage=8,minus_four_maximum_targets=8),ordered_rules=RULES.copy(),full_engine_admitted=False,complete_consumer_admitted=False)

def _validate_chandra_visible(visible,actor,annotations,actions):
    def fail(message):raise LimitedSessionError(message)
    if type(visible)is not dict or "chandra"not in visible:fail("missing Chandra facts")
    if type(annotations)is not list or any(type(a)is not dict or "chandra"not in a for a in annotations):fail("missing Chandra action facts")
    _validate_liliana_visible({k:v for k,v in visible.items()if k!="chandra"},actor,[{k:v for k,v in a.items()if k!="chandra"}for a in annotations],actions)
    f=visible["chandra"]
    fields={"viewer","active_player","turn_key","meaning","authorized_printings","battlefield","pending_allocation","activations","exiles","pending_damage","copied_tokens","end_steps"}
    if type(f)is not dict or set(f)!=fields or f["viewer"]!=actor:fail("Chandra closed facts/viewer changed")
    _shape(f["active_player"],SEAT,"Chandra active player");_shape(f["turn_key"],U32,"Chandra turn")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Chandra producer changed")
    _uint(m.get("card_db_hash"),"Chandra catalog")
    if not _same_json_value(m,_expected_chandra_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Chandra printed meaning changed")
    for key,shape in (("authorized_printings",("list",REF)),("battlefield",("pair",("list",CURRENT),("list",CURRENT))),("pending_allocation",("optional",ALLOCATION)),("activations",("list",ACTIVATION)),("exiles",("list",EXILE)),("pending_damage",("optional",DAMAGE)),("copied_tokens",("list",COPIED)),("end_steps",("list",END_STEP))):_shape(f[key],shape,"Chandra "+key)
    if any(r["name"]!="Chandra, Flameshaper"for r in f["authorized_printings"]):fail("Chandra authorized printing changed")
    if not _same_json_value(f["battlefield"],visible["tinybones"]["battlefield"]):fail("Chandra complete native roster differs")
    def relation(r):return(r["object_role"],r["incarnation_role"])
    def source(r,chandra=False):
        a,b=r["original_source"],r["original_source_relation"]
        if a["zone"]!="Battlefield"or any(a[k]!=b[k]for k in("owner","controller","zone"))or a["controller"]!=r["captured_controller"]or(chandra and a["name"]!="Chandra, Flameshaper"):fail("Chandra original source role changed")
    def targets(ts,divided):
        if len(ts)>8 or len({relation(t["original"])for t in ts})!=len(ts)or any(t["original"]["zone"]!="Battlefield"for t in ts):fail("Chandra original target contracts changed")
        if divided and any(t["allocated_damage"]is None for t in ts):fail("Chandra assigned target lacks division")
        if not divided and any(t["allocated_damage"]is not None for t in ts):fail("Chandra copy target acquired damage")
    alloc=f["pending_allocation"]
    if alloc is not None:
        source(alloc["source"],True);targets(alloc["targets"],True)
        if sum(t["allocated_damage"]for t in alloc["targets"])+alloc["remaining_units"]!=8 or alloc["can_finish_zero"]!=(not alloc["finished"]and not alloc["targets"]):fail("Chandra eight units before payment changed")
    for i,a in enumerate(f["activations"]):
        source(a["source"],True);index=a["ability_index"]
        if a["occurrence"]!=i or a["loyalty_after"]!=a["loyalty_before"]+(2,1,-4)[index]:fail("Chandra paid loyalty changed")
        targets(a["targets"],index==2)
        if(index==0 and a["targets"])or(index==1 and len(a["targets"])!=1)or(index==2 and a["targets"]and sum(t["allocated_damage"]for t in a["targets"])!=8):fail("Chandra original division/copy target changed")
        if a["damage_source"]is not None:
            source(a["damage_source"]["source"],True)
            if index!=2 or not _same_json_value(a["damage_source"]["source"],a["source"]):fail("Chandra original damage source changed")
    choices=[e for e in f["exiles"]if e["choice_current"]]
    if len(choices)>1:fail("multiple Chandra exile choices")
    for e in f["exiles"]:
        if e["activation"]>=len(f["activations"])or f["activations"][e["activation"]]["ability_index"]!=0 or len(e["cards"])>3 or any(c["original"]["zone"]!="Exile"for c in e["cards"]):fail("Chandra original public exile batch changed")
        if len(e["selected_prefix"])>1 or any(i>=len(e["cards"])for i in e["selected_prefix"])or(e["chosen"]is not None and e["chosen"]>=len(e["cards"])):fail("Chandra original selected exile index changed")
        if(e["chosen"]is None)!=(e["grant_for_land"]is None)or(e["permission_present"]and(e["chosen"]is None or e["expired"])):fail("Chandra original play grant changed")
    damage=f["pending_damage"]
    if damage is not None:
        if damage["activation"]>=len(f["activations"])or f["activations"][damage["activation"]]["ability_index"]!=2 or damage["chooser"]!=actor or damage["remaining_amount"]<0 or damage["original_amount"]<0:fail("Chandra affected damage choice changed")
        if not damage["options"]or [r["menu_index"]for r in damage["options"]]!=list(range(len(damage["options"]))):fail("Chandra native damage menu changed")
        for o in damage["options"]:
            if(o["kind"]=="Shield")!=(o["shield_remaining"]is not None)or(o["shield_remaining"]is not None and o["shield_remaining"]<0):fail("Chandra current shield facts changed")
    for i,t in enumerate(f["copied_tokens"]):
        source(t["source"])
        if t["occurrence"]!=i or t["sacrifice_instances"]<1:fail("Chandra copiable sacrifice multiplicity changed")
    for i,e in enumerate(f["end_steps"]):
        if e["occurrence"]!=i or e["finished_count"]>e["published_count"]or(e["published_count"]and not e["collected"]):fail("Chandra actual end-step roles changed")
        for o in e["observers"]:
            source(o["source"])
            if o["token"]>=len(f["copied_tokens"])or o["instances"]<1 or relation(o["source"]["original_source_relation"])!=relation(f["copied_tokens"][o["token"]]["source"]["original_source_relation"]):fail("Chandra end-step observer binding changed")
    options=[];exiles=[];allocation_targets=[]
    for row in annotations:
        c=row["chandra"];_shape(c,ACTION,"Chandra action facts")
        if not _same_json_value(c["allocation"],alloc)or not _same_json_value(c["exiles"],choices)or not _same_json_value(c["damage"],damage):fail("Chandra current action context changed")
        q=c["activation"]
        if q is not None and(q["source"]["name"]!="Chandra, Flameshaper"or q["source"]["zone"]!="Battlefield"or q["after"]!=q["before"]+(2,1,-4)[q["ability_index"]]or not q["paid_before_publication"]or q["countering_refunds_cost"]):fail("Chandra activation cost changed")
        if c["selected_damage_option"]is not None:
            if damage is None or c["selected_damage_option"]not in[o["menu_index"]for o in damage["options"]]:fail("Chandra selected native damage option absent")
            options.append(c["selected_damage_option"])
        if c["selected_exile"]is not None:
            if len(choices)!=1 or c["selected_exile"]>=len(choices[0]["cards"]):fail("Chandra selected original exile card absent")
            exiles.append(c["selected_exile"])
        if c["selected_allocation_target"]is not None:
            if alloc is None or c["selected_allocation_target"]not in alloc["candidates"]:fail("Chandra selected allocation target absent")
            allocation_targets.append(c["selected_allocation_target"]["handle"])
    if damage is not None and(sorted(options)!=[o["menu_index"]for o in damage["options"]]):fail("Chandra native option coverage changed")
    if len(exiles)!=len(set(exiles))or len(allocation_targets)!=len(set(allocation_targets)):fail("Chandra selected native target repeated")
    _validate_references([visible,annotations])

class LimitedChandraClientV15(LimitedClientV1):
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
        self.schema_version = 15

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 15):
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
            _validate_chandra_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
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
