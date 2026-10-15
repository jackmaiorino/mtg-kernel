"""Client for the trusted two-seat kernel_limited_prepared_jsonl host.

Schema 5 retains inherited V51/V52 facts and complete Prepared V57 facts.
Replies contain only the current actor's safe prompt. A network host must route
them to that actor. This client is transport tooling, not a StateSpec injector,
MageZero model adapter, belief sampler or playing policy.
"""
from __future__ import annotations

from copy import deepcopy
import math
from typing import Any, Sequence

from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value, _bounded_int, _validate_elenda_visible

PROTOCOL = "kernel_limited_prepared_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-prepared-decision/v5"


# Exact JSON type/width grammar. No truthiness or bool/int equivalence is used.
I32 = ("int", -(1 << 31), (1 << 31) - 1)
U32 = ("int", 0, (1 << 32) - 1)
U16 = ("int", 0, (1 << 16) - 1)
I16 = ("int", -(1 << 15), (1 << 15) - 1)
U8 = ("int", 0, 255)
USIZE = ("int", 0, (1 << 64) - 1)
SEAT = ("enum", ("p0", "p1"))
ZONE = ("enum", ("Battlefield", "Graveyard", "Exile", "Stack", "Hand", "Library", "Command"))
CARD_TYPES = ("enum", ("Land", "Creature", "Artifact", "Enchantment", "Instant", "Sorcery", "Planeswalker"))
REF = {"handle": str, "name": str, "owner": SEAT, "controller": SEAT, "zone": ZONE}
COUNTERS = {**{key:("int",0,(1 << 31)-1) for key in ("plus_one", "charge", "net", "bait", "soul", "incubation")},
            **{key:("int",0,(1 << 15)-1) for key in ("minus_one", "minus_toughness", "stun", "lore", "oil", "lifelink")},
            "time": U8, "finality": U16, "loyalty": U32}
FORM = {"names": ("list",str), "mana_value":U16, "types":("list",CARD_TYPES),
        "supertypes":("list",("enum",("Basic","Snow","Legendary"))), "subtype_ids":("list",U16),
        "color_mask":("int",0,31), "base_power":("optional",I32), "base_toughness":("optional",I32),
        "power":("optional",I32), "toughness":("optional",I32), "keyword_bits":U32,
        "ward_generic":U16, "minimum_blockers":USIZE,
        "printed_abilities_active":bool, "abilities_removed":bool}
BOOST = {"binding":("enum",("OriginalIncarnation","LockedObjectSet")), "power":I32,
         "toughness":I32, "grant_haste":bool, "until_end_of_turn":bool}
FULL_CARD = {"source":REF, "face_index":("optional",U8), "is_token":bool, "tapped":bool,
             "summoning_sick":bool, "marked_damage":U32, "counters":COUNTERS,"form":FORM,
             "temporary_boosts":("list",BOOST), "attachments":("list",REF)}
OPTIONAL_CURRENT = {key:FULL_CARD[key] for key in ("source","is_token","tapped","summoning_sick","marked_damage","counters","form")}
ROOT = {"original_source":REF,"current_original_source":("optional",REF),"captured_controller":SEAT}
PLACEMENT = {"before":I32,"added":I32,"after":I32}
COMBAT_OUTCOME = ("variant", {"Held":None,"Published":None,"Countered":None,
                            "Resolved":{"placement_on_original_incarnation":("optional",PLACEMENT)}})
COMBAT = {"occurrence":USIZE,"original_source":REF,"current_original_source":("optional",REF),
          "original_controller":SEAT,"damaged_player":SEAT,"final_positive_damage":I32,"outcome":COMBAT_OUTCOME}
TOKEN = {"created":REF,"current_original_token":("optional",REF),"original_controller":SEAT}
ACTIVATION = {"occurrence":USIZE,"original_source":REF,"current_original_source":("optional",REF),
              "original_controller":SEAT,"payment":{"kind":("enum",("Incubation",)),"removed":U16,"before":I32,"after":I32},
              "outcome":("variant",{"Published":None,"Countered":None,"Resolved":{"tokens":("list",TOKEN)}})}
ANNOTATION = {"source":REF,"kind":("enum",("Incubation",)),"count":U16,"before":I32,"after":I32,
              "any_priority":bool,"costs_paid_before_stack":bool,"countering_refunds_costs":bool}
SPINNER_STAGE = ("variant", {"OnStack":None,"AwaitingReveal":None,"Answered":{"reveal":bool}})
QUANDARY_STAGE = ("variant", {"OnStack":None,
    "AwaitingPayment":{"payer":SEAT,"minimum":U16,"maximum":U16,
                       "payer_hand":("optional",{"original_hand":("list",REF),"selected":("list",REF)})},
    "Answered":{"payer":SEAT,"payer_selected":("optional",("list",REF))},
    "PrintedUnpaidContinuation":{"payer":SEAT,"life_loss":I32},"InitialPrintedContinuation":None})
PHOENIX_STAGE = ("variant",{"Held":{"placement_ordered":bool},"Published":None,
    "PaymentOffered":{"can_pay":bool},"AcceptedAwaitingPayment":None,"DeclinedAwaitingCompletion":None})
PHOENIX_CURRENT = {"source":REF,"token":bool,"effective_names":("list",str),"effective_types":("list",CARD_TYPES),
    "effective_subtype_ids":("list",U16),"color_mask":("int",0,31),"printed_abilities_active":bool,
    "power":("optional",I32),"toughness":("optional",I32),"flying":bool,"haste":bool,
    "attacks_each_combat_if_able":bool,"graveyard_ability_functions_here":bool}


def _shape(value: Any, spec: Any, field: str) -> None:
    if isinstance(spec, dict):
        if type(value) is not dict or set(value) != set(spec):
            raise LimitedSessionError(f"invalid Prepared fields: {field}")
        for key, child in spec.items():
            _shape(value[key],child,f"{field}.{key}")
        if spec is REF and not value["handle"]:
            raise LimitedSessionError(f"empty Prepared reference: {field}")
    elif spec in (str,bool):
        if type(value) is not spec:
            raise LimitedSessionError(f"invalid exact Prepared type: {field}")
    elif spec[0] == "int":
        _bounded_int(value,field,spec[1],spec[2])
    elif spec[0] == "enum":
        if type(value) is not str or value not in spec[1]:
            raise LimitedSessionError(f"invalid Prepared enum: {field}")
    elif spec[0] == "optional":
        if value is not None:
            _shape(value,spec[1],field)
    elif spec[0] == "list":
        if type(value) is not list:
            raise LimitedSessionError(f"invalid Prepared list: {field}")
        for index,child in enumerate(value):
            _shape(child,spec[1],f"{field}.{index}")
    elif spec[0] == "variant":
        variants=spec[1]
        if type(value) is str and value in variants and variants[value] is None:
            return
        if type(value) is not dict or len(value) != 1:
            raise LimitedSessionError(f"invalid Prepared variant: {field}")
        key=next(iter(value))
        if key not in variants or variants[key] is None:
            raise LimitedSessionError(f"invalid Prepared variant tag: {field}")
        _shape(value[key],variants[key],field+"."+key)
    else:
        raise LimitedSessionError(f"invalid Prepared schema: {field}")


def _expected_prepared_meanings() -> dict[str, Any]:
    source="OriginalBattlefieldIncarnation"
    printed={"generic_mana":1,"colored_pips":["U"],"colors":["U"],"types":["Creature"],
             "subtypes":["Human","Wizard"],"power":1,"toughness":3,"keyword_bits":8}
    trigger={"controller":"SourceControllerAtEvent","printed_ability_required_at_event":True,
             "untargeted":True,"original_source_incarnation_required_for_application":True}
    prowess={"root":"Prowess","printed":printed,"program":{"Triggered":{**trigger,
        "event":"ControllerCastsNoncreatureSpell","ordered_instructions":[{"BoostUntilEndOfTurn":{
            "source":source,"power":1,"toughness":1}}]}}}
    combat={"root":"CombatIncubation","printed":printed,"program":{"Triggered":{**trigger,
        "event":"SourceDealsFinalPositiveCombatDamageToPlayer","ordered_instructions":[{"PlaceCounters":{
            "source":source,"kind":"Incubation","amount":"FinalPositiveCommittedPlayerCombatDamage",
            "replacements_from_resolution_world":True}}]}}}
    activation={"root":"RemoveIncubationForDrake","printed":printed,"program":{"Activated":{
        "activation_zone":"Battlefield","source":source,"controller":"OriginalActivatingController",
        "any_priority_timing":True,"untargeted":True,"ordered_costs":[{"RemoveCounters":{
            "source":source,"kind":"Incubation","count":3}}],"costs_paid_before_stack":True,
        "countering_refunds_costs":False,"ordered_instructions":[{"CreateToken":{
            "controller":"OriginalActivatingController","count":1,"token":{"power":2,"toughness":2,
            "colors":["U"],"types":["Creature"],"subtypes":["Drake"],"keyword_bits":1},
            "independent_of_source_survival":True}}]}}}
    cast={"ordinary_hand_route":True,"caster_main_phase_empty_stack_priority":True,"untargeted":True,
          "ordered_instructions":["PayPrintedMana","MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"],
          "physical_departure":"BattlefieldOnResolutionOtherwiseOwnersGraveyard",
          "other_permissions_require_actual_candidate":True,"zone_replacements_apply":True}
    spinner={"printed":{"generic_mana":2,"colored_pips":["G"],"mana_value":3,"types":["Creature"],
        "subtypes":["Spider","Spirit"],"colors":["G"],"power":4,"toughness":3},"cast":cast,
        "ordered_rules":["Reach",{"OtherControlledNontokenCreatureDies":{
        "exclude_source":True,"prebatch_battlefield_controller_and_incarnation":True,
        "printed_front_face_ability_required_at_event":True,"retain_original_trigger_controller":True,
        "optional_reveal":["ControllerChoosesBeforeLibraryReadOrRng","DeclineDoesNothing",
          {"PublicRevealThroughFirstMatchingCardOrLibraryEnd":{"card_type":"Creature"}},
          "PutMatchingCardInControllersHandWithPublicIdentity","PutOnlyRevealedNonmatchesOnBottomInRandomOrder",
          "PreserveUnrevealedSuffixAndInvalidateRandomizedBottomKnowledge","EmptyOrNoMatchDoesNotDraw"]}}]}
    quandary={"printed":{"generic_mana":3,"colored_pips":["B","B"],"mana_value":5,"types":["Enchantment"],
        "subtypes":[],"colors":["B"],"power":None,"toughness":None},"cast":cast,
        "ordered_rules":[{"OpponentGenuinelyCastsSpell":{"copies_are_not_casts":True,
          "printed_battlefield_ability_required":True,"retain_original_source_incarnation_and_trigger_controller":True,
          "ordered_resolution":["OriginalCastingOpponentMaySelectZeroOrOneFromExactCurrentPrivateHand",
          "EmptyHandForcesUnpaidBranch","SelectedCardUsesActualDiscardEventAndReplacements",
          {"OtherwiseOriginalCastingOpponentLosesLife":{"amount":5}},"ContinueOnlyOriginalOrderedSiblings"]}}]}
    phoenix={"printed":{"generic_mana":1,"colored_pips":["R","R"],"mana_value":3,"colors":["R"],
        "types":["Creature"],"subtypes":["Phoenix"],"power":2,"toughness":2},
        "battlefield_entry":"UntappedByDefaultSubjectToEntryReplacements",
        "ordered_rules":["CreatureSpellEntersBattlefield",{"Keyword":"Flying"},{"Keyword":"Haste"},
          "AttacksEachCombatIfAble",{"GraveyardReturn":{"home":"Graveyard","event":"BeginningOfCombatOnYourTurn",
          "actor":"OriginalGraveyardOwnerAndController","intervening_if":{"controller":"OriginalGraveyardOwnerAndController",
          "minimum_effective_creature_power":4,"checks":["TriggerCollection","ResolutionBeforeOptionalPayment"]},
          "optional_colored_cost":["R"],"optional_generic_cost":0,"untargeted":True,
          "active_printed_ability_required_at_collection":True,"recheck_ferocious_after_payment_offer":False,
          "payment_allowed_after_original_source_departure":True,"return_failure_refunds_payment":False,
          "ordered_steps":["OfferOptionalRedMana","OnAcceptancePayRedMana","AfterPaymentCheckOriginalGraveyardIncarnation",
          "IfStillPresentReturnToBattlefieldUnderOriginalOwner"]}}]}
    return {"drake":[prowess,combat,activation],"spinner":spinner,"quandary":quandary,"phoenix":phoenix}


def _meaning(value: Any, keys: set[str], field: str) -> None:
    if type(value) is not dict or set(value) != keys:
        raise LimitedSessionError(f"incomplete ordered meaning: {field}")
    # Meanings are immutable public typed programs. Metadata, object identity,
    # selectors and executable bindings have no role in the Human wire shape.
    pending=[value]
    banned={"schema","producer_sha256","root_sha256","selector","ability_index",
            "arena_id","generation","private_snapshot","EffectOp","operation_world"}
    while pending:
        item=pending.pop()
        if type(item) is dict:
            if not all(type(k) is str for k in item) or set(item) & banned:
                raise LimitedSessionError(f"private or metadata field in meaning: {field}")
            pending.extend(item.values())
        elif type(item) is list:
            pending.extend(item)
        elif type(item) not in (str,int,bool,type(None)):
            raise LimitedSessionError(f"non-JSON typed meaning: {field}")


def _validate_prepared_visible(visible: Any, actor: str, annotations: Any, actions: Any) -> None:
    if type(visible) is not dict or set(visible) != {"common","prepared"}:
        raise LimitedSessionError("missing complete Prepared visible state")
    _validate_elenda_visible(visible["common"])
    facts=visible["prepared"]
    if type(facts) is not dict or set(facts) != {"drake","spinner","quandary","phoenix"}:
        raise LimitedSessionError("missing complete Prepared families")
    d,s,q,p=(facts[k] for k in ("drake","spinner","quandary","phoenix"))
    for value,keys,name in ((d,{"meanings","battlefield","combat","activations"},"drake"),
         (s,{"meaning","current","held","stack"},"spinner"),
         (q,{"viewer","meaning","current","held","stack"},"quandary"),
         (p,{"meaning","ferocious_now","current","spells","triggers"},"phoenix")):
        if type(value) is not dict or set(value) != keys:
            raise LimitedSessionError(f"incomplete Prepared {name}")
    _shape(d["battlefield"],("list",("list",FULL_CARD)),"battlefield")
    if len(d["battlefield"]) != 2:
        raise LimitedSessionError("missing full two-seat battlefield")
    legacy=visible["common"]["predecessor"]["public"]["battlefield"]
    for seat,rows,old in zip(("p0","p1"),d["battlefield"],legacy):
        expected={row["card"]["stable"]["handle"]:row["card"]["stable"] for row in old}
        exact={row["card"]["stable"]["handle"]:row for row in old}
        seen=set()
        for row in rows:
            ref=row["source"]
            if (not ref["handle"] or ref["handle"] in seen or ref["controller"] != seat
                    or ref["zone"] != "Battlefield" or not _same_json_value(ref,expected.get(ref["handle"]))):
                raise LimitedSessionError("full battlefield is not an exact safe roster")
            inherited=exact[ref["handle"]]
            if (row["marked_damage"] != inherited["marked_damage"]
                    or row["counters"]["plus_one"] != inherited["plus_one_counters"]
                    or row["counters"]["soul"] != inherited["soul"]):
                raise LimitedSessionError("full battlefield disagrees with exact inherited values")
            seen.add(ref["handle"])
        if seen != set(expected):
            raise LimitedSessionError("incomplete full battlefield roster")
    if type(d["meanings"]) is not list or len(d["meanings"]) != 3:
        raise LimitedSessionError("missing three ordered Drake meanings")
    for m,root in zip(d["meanings"],("Prowess","CombatIncubation","RemoveIncubationForDrake")):
        _meaning(m,{"root","printed","program"},"drake")
        if type(m["root"]) is not str or m["root"] != root:
            raise LimitedSessionError("Drake meaning root order changed")
    _meaning(s["meaning"],{"printed","cast","ordered_rules"},"spinner")
    _meaning(q["meaning"],{"printed","cast","ordered_rules"},"quandary")
    _meaning(p["meaning"],{"printed","battlefield_entry","ordered_rules"},"phoenix")
    expected=_expected_prepared_meanings()
    for actual,wanted in ((d["meanings"],expected["drake"]),(s["meaning"],expected["spinner"]),
                          (q["meaning"],expected["quandary"]),(p["meaning"],expected["phoenix"])):
        if not _same_json_value(actual,wanted):
            raise LimitedSessionError("ordered Prepared meaning or exact JSON types changed")
    _shape(d["combat"],("list",COMBAT),"combat")
    _shape(d["activations"],("list",ACTIVATION),"activations")
    for value,stage in ((s,SPINNER_STAGE),(q,QUANDARY_STAGE)):
        _shape(value["current"],("list",OPTIONAL_CURRENT),"current")
        _shape(value["held"],("list",ROOT),"held")
        _shape(value["stack"],("list",{"stack_position":USIZE,"root":ROOT,"stage":stage}),"stack")
    if q["viewer"] != actor:
        raise LimitedSessionError("Quandary facts are not actor-scoped")
    for row in q["stack"]:
        for tag,key in (("AwaitingPayment","payer_hand"),("Answered","payer_selected")):
            if type(row["stage"]) is dict and tag in row["stage"]:
                stage=row["stage"][tag]
                if stage["payer"] != actor and stage[key] is not None:
                    raise LimitedSessionError("foreign Quandary private hand")
    _shape(p["ferocious_now"],("list",bool),"ferocious_now")
    if len(p["ferocious_now"]) != 2:
        raise LimitedSessionError("missing two-seat ferocious facts")
    _shape(p["current"],("list",PHOENIX_CURRENT),"phoenix.current")
    _shape(p["spells"],("list",{"source":REF,"is_copy":bool}),"phoenix.spells")
    _shape(p["triggers"],("list",{"original_source":REF,"current_original_source":("optional",REF),"original_actor":SEAT,"stage":PHOENIX_STAGE}),"phoenix.triggers")
    if type(actions) is not list or type(annotations) is not list or len(annotations) != len(actions):
        raise LimitedSessionError("missing exact parallel action meanings")
    _shape(annotations,("list",("optional",ANNOTATION)),"action_meanings")
    full={row["source"]["handle"]:row for cards in d["battlefield"] for row in cards}
    for row in annotations:
        if row is not None:
            current=full.get(row["source"]["handle"])
            if (current is None or not _same_json_value(row["source"],current["source"])
                    or row["source"]["controller"] != actor
                    or row["before"] != current["counters"]["incubation"]):
                raise LimitedSessionError("activation is not bound to the current exact public source")
        if row is not None and (row["count"] != 3 or row["before"] < 3 or row["after"] != row["before"]-3
                or not row["any_priority"] or not row["costs_paid_before_stack"] or row["countering_refunds_costs"]):
            raise LimitedSessionError("invalid printed Hatcher payment semantics")


class LimitedPreparedClientV5(LimitedClientV1):
    def __init__(self, command: Sequence[str], *, expected_card_db_hash: int,
                 timeout_s: float = 10.0) -> None:
        self.expected_card_db_hash = _uint(expected_card_db_hash, "card_db_hash")
        if not math.isfinite(timeout_s) or timeout_s <= 0:
            raise ValueError("timeout_s must be finite and positive")
        self._issued_state: dict[str, Any] | None = None
        self._authority_generation: int | None = None
        self._snapshots: set[int] = set()
        super().__init__(command, timeout_s=timeout_s)
        self.schema_version = 5

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 5):
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
            raise LimitedSessionError("invalid Prepared producer metadata")
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-5 reply fields")
        if kind == "error":
            if not isinstance(reply.get("code"), str) or not reply["code"]:
                raise LimitedSessionError("invalid error code")
            return kind
        if kind != "state":
            raise LimitedSessionError("unexpected state response")
        state = reply.get("state")
        if not isinstance(state, dict):
            raise LimitedSessionError("missing common state")
        _uint(state.get("episode_id"), "episode_id")
        state_type = state.get("state_type")
        expected_state={"decision":{"state_type","episode_id","authority_generation","decision"},
                        "terminal":{"state_type","episode_id","outcome","classification","code","winner","policy_steps","physical_decisions"},
                        "unsupported":{"state_type","episode_id","code"}}
        if type(state_type) is not str or state_type not in expected_state or set(state) != expected_state[state_type]:
            raise LimitedSessionError("unexpected schema-5 state fields")
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
                raise LimitedSessionError("unexpected schema-5 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_prepared_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
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
        if request["request_type"] == "snapshot":
            if _uint(reply.get("snapshot_id"), "snapshot_id") == 0:
                raise LimitedSessionError("invalid snapshot handle")
        return kind

    def _server_error(self, reply: dict[str, Any]) -> str:
        return reply["code"]

    def exchange(self, payload: dict[str, Any]) -> dict[str, Any]:
        reply = super().exchange(payload)
        self._issued_state = deepcopy(reply["state"])
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
