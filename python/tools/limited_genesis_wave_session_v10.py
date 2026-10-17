"""Closed schema-10 Genesis Wave host client. Parser mocks are not native qualification.
The current registry keeps Genesis Wave Partial. Full-only reset remains enforced.
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
PROTOCOL = "kernel_limited_genesis_wave_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-genesis-wave-decision/v10"
from limited_end_turn_session_v9 import METHOD
ZONE=("enum",("Hand","Library","Battlefield","Graveyard","Exile","Stack"))
REVEALED={"card":REF,"mana_value":U16,"permanent":bool,"eligible":bool,"aura":bool}
ATTACHMENT={"revealed_index":U16,"original_host":REF}
# Stage is checked separately because the native enum has unit and record variants.
U64=("int",0,(1<<64)-1)
WAVE={"occurrence":U64,"source":REF,"controller":SEAT,"x_value":U16,"is_copy":bool,"is_flashback":bool,"cast_method":METHOD,
    "revealed":("list",REVEALED),"selected_indices":("list",U16),"attachments":("list",ATTACHMENT),"failed_aura_indices":("list",U16),
    "remainder_indices":("list",U16),"answer_indices":("list",U16),"answer_host":("optional",REF),"answer_waits_for_advance":bool,"spell_departure":("optional",ZONE),"stage":object}
RULES=['AnnounceNormalHandSorceryXPlusGGGOrAuthenticGrantedCastMethod', 'KeepNativeCastPermissionAdditionalCostsAndResponseWindows', 'WithoutPayingManaCostForcesXZero', 'CopiesUseOriginalXWithoutBeingCastOrPayingAgain', 'UseThePaidOrCopiedNativeU16X', 'RevealUpToXTopCardsOfYourLibraryToBothPlayers', 'MayChooseAnyNumberOfRevealedPermanentCardsWithPrintedManaValueAtMostX', 'KeepChosenSubsetInOriginalRevealOrder', 'ChooseLegalPreexistingAuraHostsWithoutTargeting', 'AuraWithoutLegalHostCannotEnter', 'PutAllChosenEnteringPermanentsOntoBattlefieldSimultaneously', 'CompleteNativeAsEntersChoicesAndEntryReplacements', 'PutEveryRevealedCardNotEnteredOntoItsActualGraveyardReplacementPath', 'OrderOnlyActualGraveyardEntrantsAndCompleteMandatoryShuffles', 'FinishActualSpellDepartureBeforePlacingEntryTriggers', 'CounteredBeforeResolutionDoesNotReveal', 'PhysicalFlashbackExilesAndVirtualSpellCopiesCease']
def _expected_genesis_wave_meaning(catalog,producer):
    return dict(schema="rules-meaning/genesis-wave/v69",producer_sha256=producer,card_db_hash=catalog,card_id=487,
        printed=dict(generic_mana=0,colored_mana=["G","G","G"],mana_value=3,instant=False,targets=0,x_count=1,types=["Sorcery"],subtypes=[],colors=["G"],legendary=False,power=None,toughness=None,keyword_bits=0),ordered_rules=RULES.copy(),
        full_engine_admitted=False,owning_consumer_admitted=False,model_admitted=False,sampler_admitted=False)

def _wave_contexts(waves):
    return [{k:w[k]for k in ("occurrence","stage","source","x_value")}for w in waves if w["stage"]in ("Subset","EntryChoices","GraveyardOrder")or type(w["stage"])is dict and "AuraHost"in w["stage"]]

def _validate_genesis_wave_visible(visible,actor,annotations,actions):
    from limited_end_turn_session_v9 import _validate_end_turn_visible
    def fail(message): raise LimitedSessionError(message)
    if type(visible)is not dict or set(visible)!={"common","prepared","next_five","graveyard_return","call","end_turn","genesis_wave"}:
        fail("missing closed Genesis Wave visible state")
    if type(annotations)is not list or len(annotations)!=len(actions):fail("Genesis Wave candidate alignment changed")
    if any(type(r)is not dict or "genesis_waves"not in r for r in annotations):fail("Genesis Wave annotation missing")
    _validate_end_turn_visible({k:v for k,v in visible.items()if k!="genesis_wave"},actor,[{k:v for k,v in r.items()if k!="genesis_waves"}for r in annotations],actions)
    f=visible["genesis_wave"]
    if type(f)is not dict or set(f)!={"viewer","meaning","visible_printings","waves"}or f["viewer"]!=actor:fail("Genesis Wave viewer/fields changed")
    m=f["meaning"]
    if type(m)is not dict or type(m.get("producer_sha256"))is not str or len(m["producer_sha256"])!=64 or any(c not in "0123456789abcdef"for c in m["producer_sha256"]):fail("Genesis Wave producer changed")
    _uint(m.get("card_db_hash"),"Genesis Wave catalog")
    if not _same_json_value(m,_expected_genesis_wave_meaning(m["card_db_hash"],m["producer_sha256"])):fail("Genesis Wave complete printed program changed")
    _shape(f["visible_printings"],("list",REF),"Genesis Wave visible printings")
    if any(r["name"]!="Genesis Wave"for r in f["visible_printings"]):fail("Genesis Wave visible printing changed")
    if type(f["waves"])is not list:fail("Genesis Wave waves changed")
    for occurrence,w in enumerate(f["waves"]):
        if type(w)is not dict or set(w)!=set(WAVE):fail("Genesis Wave occurrence fields changed")
        _shape({k:v for k,v in w.items()if k!="stage"},{k:v for k,v in WAVE.items()if k!="stage"},"Genesis Wave occurrence")
        stage=w["stage"]
        if type(stage)is str:
            if stage not in ("Published","Subset","GraveyardOrder","EntryChoices","RemainderCommitted","Finished"):fail("Genesis Wave stage changed")
        elif type(stage)is dict and set(stage)=={"AuraHost"}:_shape(stage["AuraHost"],{"selected_index":U16},"Genesis Wave Aura cursor")
        else:fail("Genesis Wave stage changed")
        if w["occurrence"]!=occurrence or w["source"]["name"]!="Genesis Wave"or w["source"]["controller"]!=w["controller"]or w["source"]["zone"]!="Stack"or w["is_flashback"]!=(w["cast_method"]=="flashback"):fail("Genesis Wave source relation changed")
        if w["answer_host"]is not None and(w["answer_host"]["zone"]!="Battlefield"or type(stage)is not dict or "AuraHost"not in stage):fail("Genesis Wave current Aura answer changed")
        n=len(w["revealed"])
        if n>w["x_value"]:fail("Genesis Wave revealed too many cards")
        for r in w["revealed"]:
            if r["card"]["zone"]!="Library"or r["card"]["controller"]!=w["controller"]or r["eligible"]!=(r["permanent"]and r["mana_value"]<=w["x_value"]):fail("Genesis Wave revealed operand changed")
        for key in ("selected_indices","failed_aura_indices","remainder_indices","answer_indices"):
            if len(set(w[key]))!=len(w[key])or any(i>=n for i in w[key]):fail("Genesis Wave revealed slot changed")
        if w["selected_indices"]!=sorted(w["selected_indices"])or any(not w["revealed"][i]["eligible"]for i in w["selected_indices"]):fail("Genesis Wave chosen permanent changed")
        if type(stage)is dict:
            cursor=stage["AuraHost"]["selected_index"]
            if cursor>=len(w["selected_indices"])or not w["revealed"][w["selected_indices"][cursor]]["aura"]:fail("Genesis Wave Aura cursor changed")
        seen=set()
        for a in w["attachments"]:
            i=a["revealed_index"]
            if i not in w["selected_indices"]or i in seen or i in w["failed_aura_indices"]or not w["revealed"][i]["aura"]or a["original_host"]["zone"]!="Battlefield":fail("Genesis Wave Aura host changed")
            seen.add(i)
        if any(i not in w["selected_indices"]or not w["revealed"][i]["aura"]for i in w["failed_aura_indices"]):fail("Genesis Wave failed Aura changed")
        if w["spell_departure"]is not None and stage!="Finished":fail("Genesis Wave terminal departure changed")
        if stage=="Published"and(w["revealed"]or w["selected_indices"]or w["answer_indices"]):fail("Genesis Wave published row acquired reveal state")
    contexts=_wave_contexts(f["waves"])
    if len(contexts)>1:fail("Genesis Wave has multiple active resolving waves")
    for row in annotations:
        if not _same_json_value(row["genesis_waves"],contexts):fail("Genesis Wave candidate context differs from complete observation")
    _validate_references([visible,annotations])

class LimitedGenesisWaveClientV10(LimitedClientV1):
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
        self.schema_version = 10

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 10):
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
            raise LimitedSessionError("schema-10 process producer identity changed")
        self._producer_identity = identity
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-10 reply fields")
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
            raise LimitedSessionError("unexpected schema-10 state fields")
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
                raise LimitedSessionError("unexpected schema-10 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_genesis_wave_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            if decision["state"]["call"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Call meaning catalog differs from process header")
            if decision["state"]["end_turn"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Time Stop meaning catalog differs from header")
            if decision["state"]["genesis_wave"]["meaning"]["card_db_hash"] != catalog:
                raise LimitedSessionError("Genesis Wave meaning catalog differs from header")
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
