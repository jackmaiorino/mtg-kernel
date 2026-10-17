"""Closed V14 Liliana transport. Parser mocks are not native qualification."""
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
from limited_tinybones_session_v13 import _validate_tinybones_visible, RELATION, CURRENT
from limited_vivien_session_v11 import COST
PROTOCOL="kernel_limited_liliana_jsonl"
DECISION_SCHEMA="mtg-kernel-limited-liliana-decision/v14"
TYPES=("Land","Creature","Artifact","Enchantment","Planeswalker")
LILIANA_ROOT={**ROOT,"original_source_relation":RELATION}
PTYPE=("enum",TYPES)
KIND=("variant",{"SacrificeCreatures":None,"KeepPermanent":PTYPE,"OrderGraveyard":None})
SELECTION={"occurrence":U32,"phase":("int",0,255),"actor":SEAT,"kind":KIND,"candidates":("list",RELATION),"exact_count":("int",0,65535),"ordered":bool,"selected_prefix":("list",RELATION),"current":bool}
SCENE={"original":RELATION,"permanent_types":("list",PTYPE),"current_original":("optional",REF)}
KEEPER={"permanent_type":PTYPE,"permanent":RELATION}
RESOLUTION={"activation":U32,"active_player":SEAT,"controller":SEAT,"ultimate":bool,"rosters":("pair",("list",SCENE),("list",SCENE)),"selections":("list",SELECTION),"chosen_sacrifices":("pair",("list",RELATION),("list",RELATION)),"keepers":("list",KEEPER),"graveyard_orders":("pair",("list",RELATION),("list",RELATION)),"completed_sacrifices":("list",RELATION),"finished":bool}
STAGE=("enum",("Paid","Published","Resolving","NativeCompleted","Disposed","NoLongerPublished"))
ACTIVATION={"occurrence":U32,"source":LILIANA_ROOT,"turn":U32,"ability_index":("int",0,2),"loyalty_before":U32,"loyalty_after":U32,"stage":STAGE,"created_zombies":("list",RELATION)}
DEATH={"occurrence":U32,"observer":LILIANA_ROOT,"creature":RELATION,"collected":bool}
DRAW={"source":LILIANA_ROOT,"stage":("enum",("Held","Published","Resolving")),"native_position":U32}
ACTION={"choices":("list",SELECTION),"activation":("optional",COST)}
RULES=['PayFourBlackBlackWithOrdinaryPlaneswalkerTimingAndResponses', 'EnterWithSixLoyaltySubjectToActualCounterReplacements', 'CaptureEveryControlledCreatureDeathIncludingTokensAndSelfIfCreatureBeforeTheBatch', 'CaptureOriginalActiveObserverControllerAndIncarnationUsingLastKnownInformation', 'MandatoryUntargetedDrawOneTriggerUsesNativeApnapAndSurvivesSourceDeparture', 'ActivateOnePrintedLoyaltyAbilityAtSorceryTimingWithOrdinaryOncePerTurnRestriction', 'PayOriginalPlusOneMinusFourOrMinusNineLoyaltyAsCostWithoutDoublingSeason', 'PaidAbilityKeepsOriginalSourceControllerAfterSourceDepartureOrControlChange', 'PlusOneCreatesOneBlackTwoTwoZombieSubjectToActualTokenReplacements', 'MinusFourEachPlayerChoosesTwoCreaturesOrAllIfFewerInApnapOrder', 'AllChoicesAndPhysicalOwnerGraveyardOrdersPrecedeOneSimultaneousSacrificeBatch', 'MinusNineEachOpponentChoosesOnePermanentForEachRepresentedPermanentType', 'SameMultitypePermanentMayBeChosenForSeveralTypesAndKeeperUnionSurvives', 'SacrificeAllOtherOpponentPermanentsIgnoringHexproofAndIndestructibility', 'SourceLostToZeroLoyaltyBeforeResolutionCannotObserveLaterCreatureDeaths', 'CounteredOrDisposedAbilityKeepsPaidLoyaltyWithoutInventedResolution']

def _expected_liliana_meaning(catalog,producer):
    return dict(schema="rules-meaning/liliana-dreadhorde-general/v73",producer_sha256=producer,card_db_hash=catalog,card_id=491,printed=dict(generic_mana=4,colored_mana=["B","B"],mana_value=6,colors=["B"],types=["Planeswalker"],subtypes=["Liliana"],supertypes=["Legendary"],starting_loyalty=6,loyalty_costs=[1,-4,-9],loyalty_sorcery_only=True,zombie_card_id=492,zombie_power=2,zombie_toughness=2,zombie_colors=["B"],zombie_types=["Creature"],zombie_subtypes=["Zombie"]),ordered_rules=RULES.copy(),full_engine_admitted=False,complete_consumer_admitted=False)

def _validate_liliana_visible(visible,actor,annotations,actions):
    def fail(message):raise LimitedSessionError(message)
    if type(visible)is not dict or "liliana"not in visible:fail("missing Liliana facts")
    if type(annotations)is not list or any(type(a)is not dict or "liliana"not in a for a in annotations):fail("missing Liliana action facts")
    _validate_tinybones_visible({k:v for k,v in visible.items()if k!="liliana"},actor,[{k:v for k,v in a.items()if k!="liliana"}for a in annotations],actions)
    f=visible["liliana"]
    fields={"viewer","active_player","turn_key","meaning","authorized_printings","battlefield","activations","resolutions","deaths","current_draws"}
    if type(f)is not dict or set(f)!=fields or f["viewer"]!=actor:fail("Liliana closed facts/viewer changed")
    _shape(f["active_player"],SEAT,"Liliana active player");_shape(f["turn_key"],U32,"Liliana turn")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Liliana producer changed")
    _uint(m.get("card_db_hash"),"Liliana catalog")
    if not _same_json_value(m,_expected_liliana_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Liliana printed meaning changed")
    for key,shape in (("authorized_printings",("list",REF)),("battlefield",("pair",("list",CURRENT),("list",CURRENT))),("activations",("list",ACTIVATION)),("resolutions",("list",RESOLUTION)),("deaths",("list",DEATH)),("current_draws",("list",DRAW))):_shape(f[key],shape,"Liliana "+key)
    if any(r["name"]not in("Liliana, Dreadhorde General","Zombie Token")for r in f["authorized_printings"]):fail("Liliana authorized printing changed")
    if not _same_json_value(f["battlefield"],visible["tinybones"]["battlefield"]):fail("Liliana complete native roster differs")
    def source(r):
        _shape(r["original_source_relation"],RELATION,"Liliana original source relation")
        original=r["original_source"];relation=r["original_source_relation"]
        if any(original[k]!=relation[k]for k in ("owner","controller","zone")):fail("Liliana original source role changed")
        if r["original_source"]["name"]!="Liliana, Dreadhorde General"or r["original_source"]["zone"]!="Battlefield"or r["original_source"]["controller"]!=r["captured_controller"]:fail("Liliana original source/controller changed")
    def rel(r):return(r["object_role"],r["incarnation_role"])
    def unique(rows):return len({rel(r)for r in rows})==len(rows)
    for i,a in enumerate(f["activations"]):
        source(a["source"])
        if a["occurrence"]!=i or a["loyalty_after"]!=a["loyalty_before"]+(1,-4,-9)[a["ability_index"]]:fail("Liliana paid loyalty changed")
        if a["created_zombies"]and(a["ability_index"]!=0 or a["stage"]!="NativeCompleted"):fail("Liliana token outcome stage changed")
        if any(r["owner"]!=a["source"]["captured_controller"]or r["controller"]!=a["source"]["captured_controller"]or r["zone"]!="Battlefield"for r in a["created_zombies"]):fail("Liliana original token relation changed")
    current=[]
    for r in f["resolutions"]:
        if r["activation"]>=len(f["activations"]):fail("Liliana original activation absent")
        a=f["activations"][r["activation"]]
        if a["ability_index"]not in(1,2)or r["ultimate"]!=(a["ability_index"]==2)or r["controller"]!=a["source"]["captured_controller"]:fail("Liliana original resolution root changed")
        all_original=[o["original"]for rows in r["rosters"]for o in rows]
        if not unique(all_original):fail("Liliana original scene repeated")
        for seat,rows in zip(("p0","p1"),r["rosters"]):
            if any(o["original"]["controller"]!=seat or o["original"]["zone"]!="Battlefield"or len(o["permanent_types"])!=len(set(o["permanent_types"]))for o in rows):fail("Liliana original scene changed")
        for i,c in enumerate(r["selections"]):
            if c["occurrence"]!=i or not unique(c["candidates"])or not unique(c["selected_prefix"])or not len(c["selected_prefix"])<=c["exact_count"]<=len(c["candidates"])or any(s not in c["candidates"]for s in c["selected_prefix"]):fail("Liliana original choice cardinality changed")
            if c["kind"]=="OrderGraveyard"and(not c["ordered"]or c["exact_count"]!=len(c["candidates"])):fail("Liliana physical-owner graveyard order changed")
            if c["current"]:current.append(c)
        for k in r["keepers"]:
            matches=[o for rows in r["rosters"]for o in rows if o["original"]==k["permanent"]]
            if len(matches)!=1 or k["permanent_type"]not in matches[0]["permanent_types"]:fail("Liliana represented-type keeper changed")
        if len({k["permanent_type"]for k in r["keepers"]})!=len(r["keepers"]):fail("Liliana permanent type chosen twice")
        kept={rel(k["permanent"])for k in r["keepers"]}
        if any(rel(c)in kept for c in r["completed_sacrifices"]):fail("Liliana keeper union was sacrificed")
        if not unique(r["completed_sacrifices"])or(r["completed_sacrifices"]and not r["finished"]):fail("Liliana simultaneous batch completion changed")
    if len(current)>1:fail("multiple current Liliana choices")
    for i,d in enumerate(f["deaths"]):
        source(d["observer"])
        if d["occurrence"]!=i or d["creature"]["zone"]!="Battlefield"or d["creature"]["controller"]!=d["observer"]["captured_controller"]:fail("Liliana prebatch observer binding changed")
    positions=set()
    for d in f["current_draws"]:
        source(d["source"]);position=(d["stage"],d["native_position"])
        if position in positions:fail("Liliana current draw position repeated")
        positions.add(position)
        same=lambda r:_same_json_value(r,d["source"])
        count=sum(same(x["source"])for x in f["current_draws"])
        captures=sum(x["collected"]and same(x["observer"])for x in f["deaths"])
        if count>captures:fail("Liliana current draw exceeds collected original observers")
    for row in annotations:
        _shape(row["liliana"],ACTION,"Liliana action facts")
        if not _same_json_value(row["liliana"]["choices"],current):fail("Liliana current original choice context changed")
        c=row["liliana"]["activation"]
        if c is not None and(c["source"]["name"]!="Liliana, Dreadhorde General"or c["source"]["zone"]!="Battlefield"or c["after"]!=c["before"]+(1,-4,-9)[c["ability_index"]]or not c["paid_before_publication"]or c["countering_refunds_cost"]):fail("Liliana activation cost changed")
    _validate_references([visible,annotations])

class LimitedLilianaClientV14(LimitedClientV1):
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
        self.schema_version = 14

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 14):
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
            raise LimitedSessionError("schema-14 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-14 reply fields")
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
            raise LimitedSessionError("unexpected schema-14 state fields")
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
                raise LimitedSessionError("unexpected schema-14 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_liliana_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
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
            if decision["state"]["liliana"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Liliana catalog differs from header")
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
