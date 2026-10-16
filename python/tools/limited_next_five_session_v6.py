"""Strict schema-6 client for the trusted two-seat next-five host.

All protocol fixtures are explicit mocks. This client neither injects engine
state nor grants model, sampler, search, printing or gameplay admission.
"""
from __future__ import annotations

from copy import deepcopy
import math
from typing import Any, Sequence

from limited_decks_v1 import ImportedDeck
from limited_session_v1 import LimitedClientV1, LimitedSessionError, _uint
from limited_elenda_session_v4 import _same_json_value
from limited_prepared_session_v5 import (
    _validate_prepared_visible, REF, SEAT, ZONE, U8, U16, U32, I16, I32,
    USIZE, ROOT, OPTIONAL_CURRENT,
)

PROTOCOL = "kernel_limited_next_five_jsonl"
DECISION_SCHEMA = "mtg-kernel-limited-next-five-decision/v6"

I64 = ("int", -(1 << 63), (1 << 63) - 1)
U64 = ("int", 0, (1 << 64) - 1)
TARGET = ("tagged", "target_kind", {
    "player": {"player": SEAT}, "object": {"object": REF},
})


def _shape(value: Any, spec: Any, field: str) -> None:
    """Closed recursive JSON grammar with exact integer types and native widths."""
    def fail():
        raise LimitedSessionError(f"invalid next-five value or fields: {field}")
    if isinstance(spec, dict):
        if type(value) is not dict or set(value) != set(spec):
            fail()
        for key, child in spec.items():
            _shape(value[key], child, f"{field}.{key}")
        if spec is REF and not value["handle"]:
            fail()
    elif spec in (str, bool):
        if type(value) is not spec:
            fail()
    elif spec[0] == "int":
        if type(value) is not int or not spec[1] <= value <= spec[2]:
            fail()
    elif spec[0] == "enum":
        if type(value) is not str or value not in spec[1]:
            fail()
    elif spec[0] == "optional":
        if value is not None:
            _shape(value, spec[1], field)
    elif spec[0] == "list":
        if type(value) is not list:
            fail()
        for i, item in enumerate(value):
            _shape(item, spec[1], f"{field}.{i}")
    elif spec[0] == "pair":
        if type(value) is not list or len(value) != 2:
            fail()
        _shape(value[0],spec[1],field+".0")
        _shape(value[1],spec[2],field+".1")
    elif spec[0] == "variant":
        variants=spec[1]
        if type(value) is str and value in variants and variants[value] is None:
            return
        if type(value) is not dict or len(value) != 1:
            fail()
        key=next(iter(value))
        if key not in variants or variants[key] is None:
            fail()
        _shape(value[key], variants[key], field+"."+key)
    elif spec[0] == "tagged":
        tag, variants=spec[1:]
        if type(value) is not dict or type(value.get(tag)) is not str or value[tag] not in variants:
            fail()
        _shape({key:item for key,item in value.items() if key != tag}, variants[value[tag]], field)
    else:
        fail()


# Static rules are compared as complete ordered JSON trees, never as names or hashes.
def _expected_next_five_meanings() -> list[dict[str, Any]]:
    def mana(generic, colored=(), x=0):
        return {"generic": generic, "colored": list(colored), "x_multiplicity": x}
    def printed(generic, colored, types, *, subtypes=(), supers=(), power=None,
                toughness=None, keywords=0, token=False, x=0, colors=None):
        return {"mana": mana(generic, colored, x), "mana_value": generic+len(colored),
                "types": list(types), "subtypes": list(subtypes), "supertypes": list(supers),
                "colors": list(dict.fromkeys(colored)) if colors is None else colors,
                "power": power, "toughness": toughness, "keyword_bits": keywords, "is_token": token}
    def cast(creature=False):
        ordered=["PayPrintedMana", "MoveOriginalSpellToBattlefieldUnderCaster"]
        return {"Cast": {"timing":"MainPhaseEmptyStackPriority", "origin":"Hand", "targets":"None",
                        "ordered":ordered, "failed_or_countered_spell_to_owner_graveyard":True,
                        "ordinary_zone_replacements_apply":True}}
    def op(zone,timing,targets,generic,pips,costs,steps):
        return {"zone":zone,"timing":timing,"targets":targets,
                "mana":{"generic":generic,"colored_pips":pips,"x_multiplicity":0},
                "ordered_costs":costs,"ordered_resolution":steps,
                **{key:True for key in ("native_candidate_and_payment_required",
                  "source_must_be_controlled_by_actor","complete_costs_before_stack_publication",
                  "countering_does_not_refund_paid_costs",
                  "source_departure_or_new_incarnation_does_not_change_original_controller",
                  "ordinary_zone_replacements_apply")}}
    snack_print={"mana":{"generic":2,"colored_pips":["B"],"x_multiplicity":0},"mana_value":3,
                 "types":["Enchantment"],"subtypes":[],"colors":["B"],"legendary":False,
                 "power":None,"toughness":None,"keyword_bits":0,"is_token":False}
    food_print={"mana":{"generic":0,"colored_pips":[],"x_multiplicity":0},"mana_value":0,
                "types":["Artifact"],"subtypes":["Food"],"colors":[],"legendary":False,
                "power":None,"toughness":None,"keyword_bits":0,"is_token":True}
    priority="PriorityWithNativeCostRestrictions"
    snack={"printed":snack_print,"ordered_rules":[
        {"OrdinaryEnchantmentSpell":{"operation":op("Hand","CasterMainPhaseEmptyStackPriorityFromHand",
          "None",2,["B"],["PayNativeMana"],["MoveOriginalSpellFromStackToBattlefieldUnderCasterControl"]),
          "countered_physical_spell_goes_to_owners_graveyard":True}},
        {"Raid":{"printed_index":0,"home":"Battlefield","original_controller_own_end_step":True,
          "successful_nonempty_attack_declaration_in_current_turn":True,"checks":["AtCollection","AtResolutionBeforeTokenCreation"],
          "captured_source_incarnation_and_controller":True,"source_survival_not_required_after_collection":True,
          "mandatory_food_count":1,"food_controller_is_original_trigger_controller":True,"token_replacements_apply_once":True}},
        {"PrintedActivation":{"printed_index":0,"operation":op("Battlefield",priority,
          "ExactlyOriginalControllerOpponent",2,["B"],["PayNativeMana","SacrificeOriginalSource"],
          ["TargetLegalityBeforeReadingAmount","SumActualPositiveLifeGainEventsForOriginalActivationControllerInActualResolutionTurn",
           "LifeLossesAndOpponentsGainsDoNotSubtractOrAdd","ResponsesBeforeResolutionCanChangeTotal",
           "OriginalTargetLosesExactlyThatFullWidthAmount"])}}],
        "food":{"printed":food_print,"activation":op("Battlefield",priority,"None",2,[],
                 ["PayNativeMana","TapOriginalSource","SacrificeOriginalSource"],["GainThreeLifeForOriginalController"])}}
    result=[{"family":"Snack","printed":printed(2,["B"],["Enchantment"]),
             "ordered_rules":[{"Snack":{"meaning":snack}}]}]
    replication={"Trigger":{"event":"ControllerUpkeep","actor":"CapturedAbilityController","home":"Battlefield",
        "printed_ability_active_at_event":True,"targets":"AnotherControlledNonlandPermanentExcludingOriginalPhysicalSource",
        "ordered":[{"CopyOriginalTarget":{"controller":"CapturedAbilityController","current_copiable_face":True,
        "face_down_uses_anonymous_characteristics":True,"copy_counters_and_noncopy_effects":False,
        "apply_token_replacements_once":True,"untargeted_aura_host_before_entry_observers":True,
        "no_legal_host_creates_no_aura":True,"one_choice_per_expanded_aura":True}}]}}
    infernal={"Trigger":{"event":"SelfDiesWhileNotDemonOnPrebatchBattlefield","actor":"CapturedAbilityController","home":"Battlefield",
        "printed_ability_active_at_event":True,"targets":"None","ordered":[{"ReturnOriginalGraveyardIncarnation":{
        "controller":"PhysicalCardOwner","token_cannot_return":True,"requires_exact_successor_generation":True,
        "plus_one_counters":2,"added_subtype":"Demon","entry_characteristics_before_observers":True,
        "entry_counter_replacements_apply":True}}]}}
    result.append({"family":"Replication","printed":printed(4,["U","U"],["Enchantment"]),"ordered_rules":[cast(),replication]})
    result.append({"family":"Infernal","printed":printed(2,["B"],["Creature"],subtypes=["Human","Cleric"],power=2,toughness=1),
                   "ordered_rules":[cast(),infernal]})
    result.append({"family":"Giada","printed":printed(1,["W"],["Creature"],subtypes=["Angel"],supers=["Legendary"],power=2,toughness=2,keywords=9),
        "ordered_rules":[cast(),{"AngelEntry":{key:True for key in ("source_must_be_active_before_batch","other_controlled_angel",
        "count_already_controlled_angels_before_entry","exclude_entrant_from_count","one_application_per_original_source",
        "counters_exist_before_observers","counter_replacements_apply")}},
        {"RestrictedMana":{"tap_original_source":True,"amount":1,"color":"W","spell_subtype_only":"Angel",
         "creature_tap_sickness_rules_apply":True,"not_for_activations":True,"restriction_survives_source_departure":True}}]})
    goblin_cast=cast()["Cast"]
    goblin_cast["targets"]="OneCreature"
    goblin_cast["ordered"]=["PayPrintedMana",{"DealChosenXToOriginalCreature":{
        "actual_committed_damage_after_replacement_and_prevention":True}}, {"CreateExcessGoblins":{
        "token":{"printed":printed(0,[],["Creature"],subtypes=["Goblin"],power=1,toughness=1,token=True,colors=["R"])},
        "controller":"OriginalSpellController","lethal_uses_predamage_effective_toughness_less_marked_damage":True,
        "source_deathtouch_lethal_one":True,"no_sba_between_damage_and_tokens":True,"one_complete_replacement_batch":True}}]
    result.append({"family":"Goblin","printed":printed(0,["R","R"],["Sorcery"],x=1),"ordered_rules":[{"Cast":goblin_cast}]})
    return result


FAMILY = ("enum", ("Snack", "Replication", "Infernal", "Giada", "Goblin"))
NEXT_TARGET = ("variant", {"Player":SEAT,"Object":REF})
CURRENT = {"family":FAMILY, **OPTIONAL_CURRENT}
CAST = {"family":FAMILY,"source":REF,"original_controller":SEAT,"is_copy":bool,
        "payment_pending":bool,"x":("optional",U16),"targets":("list",NEXT_TARGET)}
TRIGGER = {"family":FAMILY,"occurrence":U32,"source":ROOT,"stage":("enum",("Held","Published")),
           "placement_ordered":bool,"targets":("list",NEXT_TARGET)}
AURA = {"source":ROOT,"target":NEXT_TARGET,"template_name":str,"template_face":U8,"chooser":SEAT,
        "expanded_count":U64,"chosen_hosts":("list",REF),"candidate_hosts":("list",REF),
        "accepted_host_awaiting_advance":("optional",REF),"remaining_count":U64,"untargeted":bool}
DEATH = {"occurrence":U32,"source":ROOT,"captured_not_demon":bool,"collection_completed":bool,
         "exact_graveyard_card_now":("optional",REF),"owner_on_return":SEAT,"return_incarnation_available_now":bool}
ANGEL = {"source":REF,"controller":SEAT,"current_angels":("list",REF),"original_angel_count_now":U32,"self_entry_excluded":bool}
MANA = {"color":("enum",("W","U","B","R","G","C")),"only_cast_angel_spell":bool,"current_original_source":("optional",REF)}
QUOTE = {"source":REF,"original_controller":SEAT,"original_x":U16,"target":NEXT_TARGET,"target_legal_now":bool,
         "creature_lethal_damage_now":("optional",I64),"quote_can_change_before_resolution":bool}
ORDERING = {"source":REF,"target":REF,"controller":SEAT,"chooser":SEAT,"original_x":U16,"lethal_damage":I64,"remaining_damage":I32}
OPTION = {"kind":("enum",("multiplier","barricade","protection","shield","color")),"source":REF,"shield_remaining":("optional",I32)}
COMPLETED = {"source":REF,"target":REF,"controller":SEAT,"original_x":U16,"lethal_damage":I64,
             "actual_damage":I32,"excess":U32,"tokens_created":U64}
SNACK_CAST = {key:CAST[key] for key in ("source","original_controller","is_copy","payment_pending")}
RAID = {"source":ROOT,"placement_ordered":bool,"on_stack":bool}
SNACK_ACTIVATION = {"source":ROOT,"stage":("enum",("ChoosingOpponentBeforeCosts","OpponentChosenAwaitingCosts","PaidAndSacrificedOnStack")),
    "target":("optional",SEAT),"target_legal_now":("optional",bool),"original_controller_gained_this_turn_now":I32}


def _validate_next_five_visible(visible: Any, actor: str, annotations: Any, actions: Any) -> None:
    if type(visible) is not dict or set(visible) != {"common","prepared","next_five"}:
        raise LimitedSessionError("missing complete next-five visible state")
    # This validates an inherited field layout only, not an artifact or its tags.
    _validate_prepared_visible({key:visible[key] for key in ("common","prepared")},actor,annotations,actions)
    facts=visible["next_five"]
    fields={"viewer","meanings","snack","authorized","current","casts","triggers","aura_copy","infernal_deaths",
            "angel_replacements","angel_mana","goblin_quotes","goblin_ordering","goblin_ordering_options","goblin_completed"}
    if type(facts) is not dict or set(facts) != fields or facts["viewer"] != actor:
        raise LimitedSessionError("invalid next-five actor or complete fields")
    expected=_expected_next_five_meanings()
    if not _same_json_value(facts["meanings"],expected):
        raise LimitedSessionError("ordered next-five printed meaning changed")
    snack=facts["snack"]
    snack_fields={"viewer","meaning","attacked_this_turn","gained_this_turn","authorized_printings","current","current_food",
                  "casts","held_raid","published_raid","activations"}
    if (type(snack) is not dict or set(snack) != snack_fields or snack["viewer"] != actor
            or not _same_json_value(snack["meaning"],expected[0]["ordered_rules"][0]["Snack"]["meaning"])):
        raise LimitedSessionError("invalid Snack actor or complete meaning")
    for key,spec in (("current",("list",CURRENT)),("casts",("list",CAST)),("triggers",("list",TRIGGER)),
                     ("aura_copy",("optional",AURA)),("infernal_deaths",("list",DEATH)),
                     ("angel_replacements",("list",ANGEL)),("angel_mana",("list",("list",MANA))),
                     ("goblin_quotes",("list",QUOTE)),("goblin_ordering",("optional",ORDERING)),
                     ("goblin_ordering_options",("list",OPTION)),("goblin_completed",("list",COMPLETED))):
        _shape(facts[key],spec,key)
    _shape(facts["authorized"],("list",("pair",FAMILY,REF)),"authorized")
    for key,spec in (("attacked_this_turn",("list",bool)),("gained_this_turn",("list",I32)),
                     ("authorized_printings",("list",REF)),("current",("list",CURRENT)),("current_food",("list",CURRENT)),
                     ("casts",("list",SNACK_CAST)),("held_raid",("list",RAID)),("published_raid",("list",RAID)),
                     ("activations",("list",SNACK_ACTIVATION))):
        _shape(snack[key],spec,"snack."+key)
    if any(len(value) != 2 for value in (snack["attacked_this_turn"],snack["gained_this_turn"],facts["angel_mana"])) or any(n < 0 for n in snack["gained_this_turn"]):
        raise LimitedSessionError("invalid two-seat gain or restricted mana roster")
    full={row["source"]["handle"]:row for cards in visible["prepared"]["drake"]["battlefield"] for row in cards}
    for rows in (facts["current"],snack["current"],snack["current_food"]):
        seen=set()
        for row in rows:
            handle=row["source"]["handle"]
            actual=full.get(handle)
            if (handle in seen or actual is None
                    or any(not _same_json_value(row[key],actual[key]) for key in OPTIONAL_CURRENT)):
                raise LimitedSessionError("next-five current form differs from complete battlefield")
            seen.add(handle)
    for row in snack["current"]+snack["current_food"]:
        if row["family"] != "Snack":
            raise LimitedSessionError("Snack current family changed")
    # Every use of one safe handle denotes the same complete public reference.
    refs={}
    pending=[visible]
    while pending:
        item=pending.pop()
        if type(item) is dict:
            if set(item) & {"arena_id","zone_change_count","object_id","card_db_id","private_world",
                            "original_world","operation_world","original_stack","event_history","private_binding",
                            "executable_binding","debug","journal_index"}:
                raise LimitedSessionError("raw engine authority in visible prompt")
            if set(item)==set(REF):
                _shape(item,REF,"safe reference")
                previous=refs.setdefault(item["handle"],item)
                if not _same_json_value(item,previous):
                    raise LimitedSessionError("safe handle identity changed")
            pending.extend(item.values())
        elif type(item) is list:
            pending.extend(item)
    for row in visible["common"]["predecessor"].get("own_hand",[]):
        _shape(row,{"stable":REF},"own_hand")
        if row["stable"]["owner"] != actor or row["stable"]["zone"] != "Hand":
            raise LimitedSessionError("foreign private hand in actor prompt")
    def live_battlefield(reference):
        row=full.get(reference["handle"])
        return row is not None and _same_json_value(reference,row["source"])
    def root(value):
        old=value["original_source"]; current=value["current_original_source"]
        if old["zone"] != "Battlefield" or old["controller"] != value["captured_controller"] or (current is not None and (not live_battlefield(current) or current["owner"] != old["owner"])):
            raise LimitedSessionError("original battlefield source/controller changed")
    for row in facts["triggers"]+facts["infernal_deaths"]+snack["held_raid"]+snack["published_raid"]+snack["activations"]:
        root(row["source"])
    for i,row in enumerate(facts["triggers"]):
        if row["occurrence"] != i or row["family"] not in ("Snack","Replication","Infernal") or (row["stage"] == "Published" and not row["placement_ordered"]) or (row["family"] != "Replication" and row["targets"]) or (row["family"] == "Replication" and (len(row["targets"]) > 1 or any("Object" not in t for t in row["targets"]))):
            raise LimitedSessionError("trigger occurrence/order changed")
    for row in facts["casts"]:
        if (row["source"]["zone"] != "Stack" or len(row["targets"]) > (1 if row["family"]=="Goblin" else 0)
                or (row["x"] is None and not row["payment_pending"])
                or (row["family"] != "Goblin" and row["x"] not in (None,0))
                or (row["family"] == "Goblin" and any("Object" not in t for t in row["targets"]))
                or (row["family"] == "Goblin" and not row["payment_pending"] and len(row["targets"]) != 1)):
            raise LimitedSessionError("invalid genuine cast target/X staging")
    for row in snack["held_raid"]:
        if row["on_stack"]:
            raise LimitedSessionError("held Raid claims stack publication")
    for row in snack["published_raid"]:
        if not row["on_stack"] or not row["placement_ordered"]:
            raise LimitedSessionError("Raid publication changed")
    for row in snack["activations"]:
        controller=row["source"]["captured_controller"]
        if row["original_controller_gained_this_turn_now"] != snack["gained_this_turn"][0 if controller=="p0" else 1]:
            raise LimitedSessionError("Snack amount uses another controller's gains")
        if row["stage"] == "ChoosingOpponentBeforeCosts":
            if row["target"] is not None or row["target_legal_now"] is not None:
                raise LimitedSessionError("unselected Snack target claims legality")
        elif row["target"] != ("p1" if controller=="p0" else "p0"):
            raise LimitedSessionError("Snack target differs from original controller's opponent")
        elif (row["stage"] == "OpponentChosenAwaitingCosts" and row["target_legal_now"] is not None) or (row["stage"] == "PaidAndSacrificedOnStack" and row["target_legal_now"] is None):
            raise LimitedSessionError("Snack legality was claimed at the wrong payment stage")
    aura=facts["aura_copy"]
    if aura is not None:
        root(aura["source"])
        if (not aura["template_name"] or not aura["untargeted"] or aura["chooser"] != aura["source"]["captured_controller"]
                or len(aura["chosen_hosts"])+aura["remaining_count"] != aura["expanded_count"]):
            raise LimitedSessionError("invalid expanded Aura progress")
        candidates=aura["candidate_hosts"]
        if len({r["handle"] for r in candidates}) != len(candidates) or any(not live_battlefield(r) for r in candidates+aura["chosen_hosts"]):
            raise LimitedSessionError("invalid Aura host roster")
        accepted=aura["accepted_host_awaiting_advance"]
        if accepted is not None and (aura["remaining_count"]==0 or not any(_same_json_value(accepted,r) for r in candidates)):
            raise LimitedSessionError("accepted Aura host is outside current menu")
    for i,row in enumerate(facts["infernal_deaths"]):
        card=row["exact_graveyard_card_now"]
        if (row["occurrence"] != i or not row["captured_not_demon"] or row["owner_on_return"] != row["source"]["original_source"]["owner"]
                or (card is not None and (card["owner"] != row["owner_on_return"] or card["zone"] != "Graveyard"))
                or (row["return_incarnation_available_now"] and card is None)):
            raise LimitedSessionError("Infernal return changed its original owner/incarnation")
    for row in facts["angel_replacements"]:
        if (row["source"]["controller"] != row["controller"] or not live_battlefield(row["source"])
                or not row["self_entry_excluded"] or row["original_angel_count_now"] != len(row["current_angels"])
                or len({r["handle"] for r in row["current_angels"]}) != len(row["current_angels"])
                or any(not live_battlefield(r) or r["controller"] != row["controller"] for r in row["current_angels"])):
            raise LimitedSessionError("Giada original Angel count/controller changed")
    for units in facts["angel_mana"]:
        for row in units:
            if row["color"] != "W" or not row["only_cast_angel_spell"] or (row["current_original_source"] is not None and not live_battlefield(row["current_original_source"])):
                raise LimitedSessionError("Giada restricted mana changed")
    for row in facts["goblin_quotes"]:
        amount=row["creature_lethal_damage_now"]
        if (not row["quote_can_change_before_resolution"] or "Object" not in row["target"]
                or row["target_legal_now"] != (amount is not None) or (amount is not None and amount < 0)):
            raise LimitedSessionError("invalid current Goblin quote")
    ordering=facts["goblin_ordering"]
    options=facts["goblin_ordering_options"]
    if ordering is None and options:
        raise LimitedSessionError("Goblin options lack authenticated ordering context")
    if ordering is not None and (ordering["chooser"] != actor or ordering["chooser"] != ordering["target"]["controller"] or ordering["lethal_damage"] < 0 or ordering["remaining_damage"] < 0 or not options):
        raise LimitedSessionError("Goblin ordering changed affected chooser/context")
    for row in options:
        if (row["kind"]=="shield") != (row["shield_remaining"] is not None) or (row["shield_remaining"] is not None and row["shield_remaining"] <= 0):
            raise LimitedSessionError("invalid current Goblin shield option")
    for row in facts["goblin_completed"]:
        if row["lethal_damage"] < 0 or row["actual_damage"] < 0 or row["excess"] != max(0,row["actual_damage"]-row["lethal_damage"]) or row["tokens_created"] < row["excess"] or (row["excess"] == 0 and row["tokens_created"] != 0):
            raise LimitedSessionError("Goblin completion does not describe actual excess/token count")


def _validate_play_context(decision: dict[str, Any]) -> None:
    london = decision["london_mulligans"]
    if london is not None:
        _shape(london, {"phase":str,"counts":("list",U8),"kept":("list",bool)}, "london")
        if len(london["counts"]) != 2 or len(london["kept"]) != 2:
            raise LimitedSessionError("missing two-seat London context")
    combat=decision["foundations_combat"]
    if combat is not None:
        target=TARGET
        _shape(combat,{"phase":str,"assignments":("list",{"source":REF,"recipient":target,"amount":("int",0,(1<<31)-1)})},"combat")


class LimitedNextFiveClientV6(LimitedClientV1):
    def __init__(self, command: Sequence[str], *, expected_card_db_hash: int,
                 timeout_s: float = 10.0) -> None:
        self.expected_card_db_hash = _uint(expected_card_db_hash, "card_db_hash")
        if not math.isfinite(timeout_s) or timeout_s <= 0:
            raise ValueError("timeout_s must be finite and positive")
        self._issued_state: dict[str, Any] | None = None
        self._authority_generation: int | None = None
        self._episode_id: int | None = None
        self._snapshots: set[int] = set()
        super().__init__(command, timeout_s=timeout_s)
        self.schema_version = 6

    def _validate_reply(self, reply: dict[str, Any], request: dict[str, Any]) -> str:
        if (reply.get("protocol") != PROTOCOL
                or type(reply.get("schema_version")) is not int
                or reply["schema_version"] != 6):
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
            raise LimitedSessionError("invalid next-five producer metadata")
        kind = reply.get("response_type")
        header={"protocol","schema_version","request_id","kernel_version","card_db_hash","card_count",
                "producer_sha256","priority_mode","combat_rules","mulligan_rules","response_type"}
        if (kind == "error" and set(reply) != header | {"code"}) or (kind == "state" and (not header | {"state"} <= set(reply) or set(reply) - header - {"state","receipt","snapshot_id"})):
            raise LimitedSessionError("unexpected schema-6 reply fields")
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
            raise LimitedSessionError("unexpected schema-6 state fields")
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
                raise LimitedSessionError("unexpected schema-6 decision fields")
            if decision.get("actor") not in ("p0", "p1"):
                raise LimitedSessionError("invalid decision actor")
            _uint(decision.get("prompt_seq"), "prompt_seq")
            if not isinstance(decision.get("state"), dict):
                raise LimitedSessionError("missing visible state")
            _validate_next_five_visible(decision["state"], decision["actor"], decision.get("action_meanings"), decision.get("actions"))
            _validate_play_context(decision)
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
